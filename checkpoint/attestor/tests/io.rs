//! Review M7 and L-5 through the code that talks to nodes: [`round`] against a local HTTP stub that
//! serves the lab node's recorded answers (`/cryptarchia/info`, `/channel/<id>`, `/cryptarchia/blocks`)
//! and a LEZ node's `getBlock` (JSON-RPC, base64 `borsh(Block)` as LEZ serves it), asked with curl as in
//! production. The second round reads only from the signed tip's slot; what a round saved replays to the
//! same decision; a reused directory is refused; a LEZ node that does not answer is a node error.
mod support;

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use attestor_core::{sign_certificate, Deployment};
use ferrata_attestor::{
    base64_encode, final_tier, read_state, round, Attestor, LezSaved, NodeAnswers, RoundError, LEZ_FINALIZED,
};
use serde_json::{json, Value};
use support::*;

/// A node stub on 127.0.0.1: the Bedrock endpoints by GET, LEZ's `getBlock` by POST. Every request is
/// logged as `METHOD target [body]`.
struct Stub {
    url: String,
    log: Arc<Mutex<Vec<String>>>,
}

fn serve(answers: NodeAnswers, info: Value, lez: BTreeMap<u64, Option<Vec<u8>>>, lez_error: bool) -> Stub {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let log = Arc::new(Mutex::new(Vec::new()));
    let seen = log.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut line = String::new();
            if r.read_line(&mut line).is_err() {
                continue;
            }
            let mut len = 0usize;
            loop {
                let mut h = String::new();
                if r.read_line(&mut h).is_err() || h == "\r\n" || h.is_empty() {
                    break;
                }
                if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0; len];
            let _ = r.read_exact(&mut body);
            let mut parts = line.split_whitespace();
            let (method, target) = (parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("").to_string());
            let reply = if method == "POST" {
                let req: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                let id = req["params"][0].as_u64().unwrap_or(0);
                if lez_error {
                    json!({"jsonrpc": "2.0", "id": 1, "error": {"code": -32603, "message": "stub error"}})
                } else {
                    let result = lez.get(&id).cloned().flatten().map(|b| base64_encode(&b));
                    json!({"jsonrpc": "2.0", "id": 1, "result": result})
                }
            } else if target == "/cryptarchia/info" {
                info.clone()
            } else if target.starts_with("/channel/") {
                answers.channel.clone()
            } else if let Some(q) = target.strip_prefix("/cryptarchia/blocks?") {
                let get = |k: &str| -> u64 {
                    q.split('&').find_map(|kv| kv.strip_prefix(&format!("{k}="))).unwrap().parse().unwrap()
                };
                let (from, to) = (get("slot_from"), get("slot_to"));
                Value::Array(
                    answers
                        .blocks
                        .iter()
                        .filter(|b| (from..=to).contains(&b["header"]["slot"].as_u64().unwrap()))
                        .cloned()
                        .collect(),
                )
            } else {
                Value::Null
            };
            seen.lock().unwrap().push(format!("{method} {target} {}", String::from_utf8_lossy(&body)));
            let b = reply.to_string();
            let _ = write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",
                b.len()
            );
        }
    });
    Stub { url, log }
}

impl Stub {
    fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.log.lock().unwrap())
    }
}

fn scratch(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("io-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn lab_info() -> Value {
    serde_json::from_slice(&fs::read(dir().join("vectors/lab-node/info.json")).unwrap()).unwrap()
}

/// The lab's LEZ node: blocks 186..=202 final (the measured status of its sequencer's blocks).
fn lab_lez() -> BTreeMap<u64, Option<Vec<u8>>> {
    let a = lab_answers();
    (186..=202).map(|id| (id, Some(with_status(&payload_of(&a, id), LEZ_FINALIZED)))).collect()
}

fn the_attestor() -> Attestor {
    let d = Deployment { evm_chain_id: [0; 32], vault: [1; 20], lez_config_id: [2; 32], channel_id: channel() };
    let (ch, att) = (key("channel-1"), key("attestation-1"));
    Attestor { deployment: d, channel_key: pk(&ch), certificate: sign_certificate(&ch, &d, &pk(&att)), att_key: att }
}

fn blocks_asked(log: &[String]) -> Vec<String> {
    log.iter().filter(|l| l.starts_with("GET /cryptarchia/blocks")).cloned().collect()
}

#[test]
fn the_second_round_reads_from_the_signed_tip_and_a_round_replays_from_what_it_saved() {
    let stub = serve(answers_with_keys(&n3_keys()), lab_info(), lab_lez(), false);
    let work = scratch("rounds");
    let state = work.join("state.json");
    let att = the_attestor();
    let first_slot = 8_848_857; // the lab extraction's first window

    let r1 = round(&att, &stub.url, &stub.url, &work.join("round-1"), &state, first_slot).unwrap();
    let log1 = stub.take();
    assert_eq!((r1.scanned_from, r1.tier.tip.tip_id), (first_slot, 202));
    assert!(r1.before.is_none());
    let windows1 = blocks_asked(&log1);
    assert_eq!(windows1.len(), 15, "{windows1:?}"); // (8_856_126 - 8_848_857 + 1) / 500, rounded up
    assert!(log1.iter().any(|l| l.starts_with("POST / ") && l.contains("getBlock")));
    let s = read_state(&state, &channel()).unwrap().unwrap();
    assert_eq!((s.last.tip_id, s.last.tip_hash), (202, r1.tier.tip.tip_hash));
    let tip_slot = s.record.as_ref().unwrap().tip_slot;
    assert_eq!(tip_slot, slot_of(&lab_answers(), 202));
    assert_eq!(fs::read_dir(work.join("round-1/blocks")).unwrap().count(), 15);
    assert!(fs::read_dir(work.join("round-1/lez")).unwrap().count() >= 1);

    // Round 2: the state's slot, one window; the same tip and the same signature.
    let r2 = round(&att, &stub.url, &stub.url, &work.join("round-2"), &state, first_slot).unwrap();
    let windows2 = blocks_asked(&stub.take());
    assert_eq!(r2.scanned_from, tip_slot);
    assert_eq!(windows2, vec![format!("GET /cryptarchia/blocks?slot_from={tip_slot}&slot_to=8856126 ")]);
    assert_eq!(r2.attestation.encode(), r1.attestation.encode());
    assert_eq!(r2.tier.first_block, 202);

    // What round 2 saved replays to the same tier, with the state it started from.
    let saved = NodeAnswers::load(&work.join("round-2")).unwrap();
    let replay = final_tier(&saved, &mut LezSaved::new(&work.join("round-2")), &channel(), r2.before.as_ref()).unwrap();
    assert_eq!((replay.tip, replay.hashes.clone()), (r2.tier.tip, r2.tier.hashes.clone()));

    // A directory that holds a read already is refused (its windows would be mixed with the new ones).
    match round(&att, &stub.url, &stub.url, &work.join("round-2"), &state, first_slot) {
        Err(RoundError::Node(e)) => assert!(format!("{e:#}").contains("fresh directory"), "{e:#}"),
        Err(e) => panic!("{e}"),
        Ok(_) => panic!("a reused directory was read"),
    }
    let _ = fs::remove_dir_all(&work);
}

#[test]
fn a_lez_node_that_does_not_answer_is_a_node_error_and_nothing_is_signed() {
    let work = scratch("lez-down");
    let state = work.join("state.json");
    let att = the_attestor();
    let bedrock = serve(answers_with_keys(&n3_keys()), lab_info(), lab_lez(), false);
    // Nothing listens there (a port the stub had, closed again).
    let closed = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}", l.local_addr().unwrap())
    };
    match round(&att, &bedrock.url, &closed, &work.join("r1"), &state, 8_856_000) {
        Err(RoundError::Node(e)) => assert!(format!("{e:#}").contains("LEZ node"), "{e:#}"),
        other => panic!("{:?}", other.err().map(|e| e.to_string())),
    }
    // A LEZ node that answers with a JSON-RPC error.
    let erring = serve(answers_with_keys(&n3_keys()), lab_info(), lab_lez(), true);
    match round(&att, &bedrock.url, &erring.url, &work.join("r2"), &state, 8_856_000) {
        Err(RoundError::Node(e)) => assert!(format!("{e:#}").contains("stub error"), "{e:#}"),
        other => panic!("{:?}", other.err().map(|e| e.to_string())),
    }
    assert!(!state.exists(), "no state is written when nothing is signed");
    let _ = fs::remove_dir_all(&work);
}
