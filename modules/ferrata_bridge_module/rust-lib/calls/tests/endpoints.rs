//! RFP-022 U.9: the wallet session's `set_endpoints` (the mini-app's "Apply and test") through the modules that hold
//! the connections, run natively against stand-ins on 127.0.0.1:
//!
//! - a chain service with seqnet-chain's contract (`module.session`, `module.info`, `module.endpoints`,
//!   `module.set_endpoints` with its dry run; it holds the LEZ sequencer, and Ethereum on the plain path);
//! - Ethereum JSON-RPC endpoints: two of the session's deployment, one of another chain, one without the vault, one
//!   whose vault names another configuration, and a closed port;
//! - Logos Core's `eth_rpc_module` (its chain registry: `get_chain_config`, `patch_chain_endpoint`, `raw_rpc_url`, and
//!   `raw_rpc` through the endpoint it holds; a chain set to `required`, whose proof-backed reads `raw_rpc_url` refuses);
//! - `lez_core` (the session's LEZ path through Logos Core).
//!
//! Checked: each owner switches its own endpoint and only after the new one answered for this deployment (a fork, whose
//! block final on the endpoint in use differs, refused); a refused switch keeps both endpoints (two given, one bad:
//! neither switches); the endpoint in use given again switches nothing and reads nothing new; an eth_rpc_module endpoint
//! that fails the reading after the switch is put back, and the sequencer switched with it too; `info` names the
//! endpoints in use and who holds each; refused while a job runs, and a sequencer change while `lez_core` holds the
//! session's LEZ connection; a session opened again on another deployment drops the old one's Ethereum path, so a switch
//! never writes another chain's record of eth_rpc_module (review M-1), and the same deployment opened again keeps it.
//! The live test (two LEZ sequencers of one zone, two Sepolia RPCs, the view inside a headless Logos Core) is
//! e2e/endpoints/run.sh.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};
use tiny_keccak::{Hasher, Keccak};

const CHAIN: u64 = 11_155_111;
const VAULT: &str = "0x1111111111111111111111111111111111111111";
const DEPOSITOR: &str = "0x7777777777777777777777777777777777777777";
const RELEASE_SENDER: &str = "0x8888888888888888888888888888888888888888";
const CID: &str = "0x2222222222222222222222222222222222222222222222222222222222222222";
const OTHER_CID: &str = "0x3333333333333333333333333333333333333333333333333333333333333333";
const PRIVATE0: &str = "0x9999999999999999999999999999999999999999999999999999999999999999";
/// Block 42's hash on the deployment's chain, and on a fork of it.
const HASH: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const FORK_HASH: &str = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const HOODI: u64 = 560_048;

fn selector(signature: &str) -> String {
    let mut k = Keccak::v256();
    k.update(signature.as_bytes());
    let mut out = [0u8; 32];
    k.finalize(&mut out);
    format!("0x{}", out[..4].iter().map(|b| format!("{b:02x}")).collect::<String>())
}

// ---- HTTP on 127.0.0.1 ----------------------------------------------------------------------------------------------

fn read_request(s: &mut TcpStream) -> Option<Vec<u8>> {
    s.set_read_timeout(Some(Duration::from_secs(10))).ok()?;
    let (mut buf, mut chunk) = (Vec::new(), [0u8; 4096]);
    loop {
        let n = s.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..end]).to_ascii_lowercase();
            let len = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
                .unwrap_or(0);
            while buf.len() < end + 4 + len {
                let n = s.read(&mut chunk).ok()?;
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
            }
            return Some(buf[end + 4..(end + 4 + len).min(buf.len())].to_vec());
        }
    }
}

/// A server on 127.0.0.1 answering each request's JSON body with `f`, one connection at a time.
fn serve(f: impl Fn(Value) -> Value + Send + 'static) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut s) = s else { continue };
            let Some(body) = read_request(&mut s) else { continue };
            let out = f(serde_json::from_slice(&body).unwrap_or(Value::Null)).to_string();
            let _ = write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{out}",
                out.len()
            );
        }
    });
    format!("http://{addr}")
}

/// A port nothing listens on.
fn closed_port() -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let a = l.local_addr().unwrap();
    drop(l);
    format!("http://{a}")
}

/// An Ethereum JSON-RPC endpoint: its chain, whether the vault's code is there, the `lezConfigId` the vault names, the
/// hash of its block 42 (its final one: `finalized` and 0x2a answer that block).
fn ethereum(chain: u64, code: bool, cid: &'static str, hash: &'static str) -> String {
    let lez_config_id = selector("lezConfigId()");
    serve(move |req| {
        let id = req["id"].clone();
        let result = match req["method"].as_str().unwrap_or("") {
            "eth_chainId" => json!(format!("0x{chain:x}")),
            "eth_getCode" => json!(if code { "0x6080604052" } else { "0x" }),
            "eth_call" => {
                if req["params"][0]["data"].as_str().unwrap_or("").starts_with(&lez_config_id) {
                    json!(cid)
                } else {
                    json!(format!("0x{}", "0".repeat(64)))
                }
            }
            "eth_blockNumber" => json!("0x2a"),
            "eth_getBlockByNumber" => match req["params"][0].as_str() {
                Some("finalized" | "latest" | "0x2a") => json!({ "number": "0x2a", "timestamp": "0x6553f100", "hash": hash }),
                _ => Value::Null,
            },
            "eth_getBalance" => json!("0x0"),
            m => return json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": format!("{m}: not here") } }),
        };
        json!({ "jsonrpc": "2.0", "id": id, "result": result })
    })
}

/// JSON-RPC over plain HTTP: the stand-in eth_rpc_module's own client.
fn rpc_post(url: &str, method: &str, params: Value) -> Result<Value, String> {
    let rest = url.strip_prefix("http://").ok_or("http:// only here")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let mut s = TcpStream::connect(authority).map_err(|e| format!("connect {authority}: {e}"))?;
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string();
    write!(
        s,
        "POST {path} HTTP/1.1\r\nHost: {authority}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .map_err(|e| e.to_string())?;
    let mut raw = String::new();
    s.read_to_string(&mut raw).map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(raw.split_once("\r\n\r\n").ok_or("no HTTP header")?.1).map_err(|e| e.to_string())?;
    match v.get("error") {
        Some(e) if !e.is_null() => Err(e.to_string()),
        _ => Ok(v["result"].clone()),
    }
}

// ---- Logos Core's eth_rpc_module and lez_core, as the module crate reaches them -------------------------------------

#[derive(Default)]
struct Registry {
    endpoints: HashMap<u64, String>,
    /// The chain set to `verifiedProxyMode: required`: `raw_rpc_url` refuses its proof-backed reads.
    required: bool,
    patched: Vec<String>,
}

static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();

fn registry() -> MutexGuard<'static, Registry> {
    REGISTRY.get_or_init(|| Mutex::new(Registry::default())).lock().unwrap()
}

fn module_call(module: &str, method: &str, args: &Value, _timeout_ms: u64) -> Result<String, String> {
    if module != "eth_rpc_module" {
        return Err(format!("{module}.{method}: not in this test"));
    }
    let chain = args[0].as_u64().ok_or("chain id")?;
    Ok(match method {
        "get_chain_config" => match registry().endpoints.get(&chain).cloned() {
            Some(e) => json!({ "ok": true, "config": { "endpoint": e, "enabled": true } }),
            None => json!({ "ok": false, "error": format!("no config for chain {chain}") }),
        },
        "patch_chain_endpoint" => {
            let url = args[1].as_str().ok_or("endpoint")?.trim().to_string();
            registry().endpoints.insert(chain, url.clone());
            registry().patched.push(url.clone());
            json!({ "ok": true, "chainId": chain, "endpoint": url })
        }
        "raw_rpc_url" => {
            let (url, method) = (args[1].as_str().ok_or("url")?, args[2].as_str().ok_or("method")?);
            let params: Value = serde_json::from_str(args[3].as_str().ok_or("params")?).map_err(|e| e.to_string())?;
            if registry().required && matches!(method, "eth_getCode" | "eth_call" | "eth_getBalance" | "eth_getBlockByNumber") {
                json!({ "ok": false, "code": "verified_bypass", "error": format!("{method}: chain {chain} is set to required") })
            } else {
                match rpc_post(url, method, params) {
                    Ok(r) => json!({ "ok": true, "result": r, "route": "direct" }),
                    Err(e) => json!({ "ok": false, "code": "http", "error": e }),
                }
            }
        }
        m => return Err(format!("eth_rpc_module.{m}: not in this test")),
    }
    .to_string())
}

/// `eth_rpc_module.raw_rpc`: through the endpoint the registry holds for the chain.
fn raw_rpc(chain_id: i64, method: &str, params: &str, _timeout_ms: u64) -> Result<String, String> {
    let url = registry().endpoints.get(&(chain_id as u64)).cloned().ok_or("unknown chain")?;
    let params: Value = serde_json::from_str(params).map_err(|e| e.to_string())?;
    Ok(match rpc_post(&url, method, params) {
        Ok(r) => json!({ "ok": true, "result": r, "route": "direct" }),
        Err(e) => json!({ "ok": false, "code": "http", "error": e }),
    }
    .to_string())
}

fn lez_core(method: &str, _args: &str, _timeout_ms: u64) -> Result<String, String> {
    Ok(match method {
        "open" => "0".into(),
        "get_sequencer_addr" => "127.0.0.1:3040".into(),
        "get_current_block_height" => "7".into(),
        "list_accounts" => json!([{ "is_public": false, "account_id": PRIVATE0 }]).to_string(),
        m => return Err(format!("lez_core.{m}: not in this test")),
    })
}

// ---- the chain service ------------------------------------------------------------------------------------------------

struct Service {
    chain: u64,
    seq: String,
    eth: String,
    /// `module.set_endpoints` calls, dry runs included.
    set_calls: usize,
    /// The sequencers whose zone holds the session's configuration, and the Ethereum RPCs of the deployment.
    good_seqs: HashSet<String>,
    good_eths: HashSet<String>,
}

fn chain_service(state: Arc<Mutex<Service>>) -> String {
    serve(move |req| {
        let mut st = state.lock().unwrap();
        let p = req["params"].clone();
        let r: Result<Value, String> = match req["method"].as_str().unwrap_or("") {
            "module.session" => Ok(json!({
                "bridge_seed": format!("0x{}", "44".repeat(32)), "chain_id": st.chain, "vault": VAULT, "config_id": CID,
                "program": format!("0x{}", "55".repeat(32)), "depositor": DEPOSITOR, "private_accounts": [PRIVATE0],
                "public_accounts": [format!("0x{}", "66".repeat(32))], "proofs": "dev", "eth_rpc": st.eth, "sequencer": st.seq,
            })),
            "module.info" if p["lez_only"] == true => Ok(json!({ "lez_head": 5, "relayer": DEPOSITOR, "sequencer": st.seq })),
            "module.info" => Ok(json!({ "eth_block": 42, "eth_time": 1_700_000_000u64, "eth_balance_wei": "0", "lez_head": 5,
                                        "relayer": DEPOSITOR, "eth_rpc": st.eth, "sequencer": st.seq })),
            "module.endpoints" => Ok(json!({ "eth_rpc": st.eth, "sequencer": st.seq })),
            // A claim's first reading: slow here, so that the claim is a running job for a while; then refused.
            "module.inputs" => {
                thread::sleep(Duration::from_millis(1500));
                Err("module.inputs: not in this test".into())
            }
            "module.set_endpoints" => {
                st.set_calls += 1;
                let seq = p["sequencer"].as_str().map(str::to_string).filter(|u| *u != st.seq);
                let eth = p["eth_rpc"].as_str().map(str::to_string).filter(|u| *u != st.eth);
                let mut refused = Vec::new();
                if let Some(u) = seq.as_ref().filter(|u| !st.good_seqs.contains(*u)) {
                    refused.push(format!("the LEZ sequencer {u}: its zone holds no configuration {CID}"));
                }
                if let Some(u) = eth.as_ref().filter(|u| !st.good_eths.contains(*u)) {
                    refused.push(format!("the Ethereum RPC {u}: not this deployment's"));
                }
                if p["dry_run"] == true {
                    Ok(json!({ "dry_run": true, "refused": refused, "checks": {} }))
                } else if !refused.is_empty() {
                    Err(format!("not switched: {}", refused.join("; ")))
                } else {
                    if let Some(u) = &seq {
                        st.seq = u.clone();
                    }
                    if let Some(u) = &eth {
                        st.eth = u.clone();
                    }
                    Ok(json!({ "eth_rpc": st.eth, "sequencer": st.seq,
                               "switched": { "eth_rpc": eth.is_some(), "sequencer": seq.is_some() }, "in_use": {} }))
                }
            }
            m => Err(format!("unknown method {m}")),
        };
        match r {
            Ok(v) => json!({ "ok": true, "result": v }),
            Err(e) => json!({ "ok": false, "error": e }),
        }
    })
}

fn wallet(method: &str, params: Value) -> Value {
    serde_json::from_str(&ferrata_bridge_calls::wallet(&json!({ "method": method, "params": params }).to_string())).unwrap()
}

fn ok(method: &str, params: Value) -> Value {
    let r = wallet(method, params.clone());
    assert_eq!(r["ok"], true, "{method} {params}: {r}");
    r["result"].clone()
}

fn refused(method: &str, params: Value, parts: &[&str]) -> String {
    let r = wallet(method, params.clone());
    assert_eq!(r["ok"], false, "{method} {params} should be refused: {r}");
    let e = r["error"].as_str().unwrap_or("").to_string();
    for part in parts {
        assert!(e.contains(part), "{method} {params}: {part:?} not in {e:?}");
    }
    e
}

#[test]
fn endpoints_switch_through_their_owners_and_stay_on_failure() {
    let (e_a, e_b) = (ethereum(CHAIN, true, CID, HASH), ethereum(CHAIN, true, CID, HASH));
    let e_other = ethereum(HOODI, true, CID, HASH);
    let e_novault = ethereum(CHAIN, false, CID, HASH);
    let e_wrongcid = ethereum(CHAIN, true, OTHER_CID, HASH);
    let e_fork = ethereum(CHAIN, true, CID, FORK_HASH);
    let dead = closed_port();
    let (seq_a, seq_b, seq_other) = ("http://127.0.0.1:19300".to_string(), "http://127.0.0.1:19301".to_string(), "http://127.0.0.1:19399".to_string());
    let state = Arc::new(Mutex::new(Service {
        chain: CHAIN,
        seq: seq_a.clone(),
        eth: e_a.clone(),
        set_calls: 0,
        good_seqs: [seq_a.clone(), seq_b.clone()].into(),
        good_eths: [e_a.clone(), e_b.clone()].into(),
    }));
    let svc = chain_service(state.clone());
    let in_service = |s: &Arc<Mutex<Service>>| {
        let g = s.lock().unwrap();
        (g.eth.clone(), g.seq.clone())
    };
    let set_calls = |s: &Arc<Mutex<Service>>| s.lock().unwrap().set_calls;

    refused("set_endpoints", json!({ "sequencer": seq_b }), &["no wallet session"]);
    let o = ok("open", json!({ "chain": svc }));
    assert_eq!(o["paths_reset"], json!([]), "{o}");

    // The plain path: the chain service holds both connections.
    let r = ok("set_endpoints", json!({ "eth_rpc": e_b, "sequencer": seq_b }));
    assert_eq!((r["eth_rpc"].as_str(), r["sequencer"].as_str()), (Some(e_b.as_str()), Some(seq_b.as_str())), "{r}");
    assert_eq!(r["switched"], json!({ "eth_rpc": true, "sequencer": true }), "{r}");
    assert_eq!(in_service(&state), (e_b.clone(), seq_b.clone()));
    let i = ok("info", json!({}));
    assert_eq!((i["eth_rpc"].as_str(), i["sequencer"].as_str()), (Some(e_b.as_str()), Some(seq_b.as_str())), "{i}");
    assert!(i["endpoints"]["eth_rpc"]["via"].as_str().unwrap_or("").contains("chain service"), "{i}");
    refused("set_endpoints", json!({ "sequencer": seq_other }), &["not switched", &seq_other, &format!("Kept: Ethereum RPC {e_b}"), &seq_b]);
    // Two given, one bad: neither switches.
    refused("set_endpoints", json!({ "eth_rpc": e_other, "sequencer": seq_a }), &["not switched", &e_other]);
    assert_eq!(in_service(&state), (e_b.clone(), seq_b.clone()), "a refused pair leaves both");
    refused("set_endpoints", json!({ "eth_rpc": "ftp://127.0.0.1:1" }), &["http:// or https://"]);
    let r = ok("set_endpoints", json!({}));
    assert_eq!(r["switched"], json!({ "eth_rpc": false, "sequencer": false }), "{r}");
    ok("set_endpoints", json!({ "eth_rpc": e_a, "sequencer": seq_a }));
    assert_eq!(in_service(&state), (e_a.clone(), seq_a.clone()));
    // The endpoints in use given again: nothing is read (no dry run) and nothing switches.
    let calls = set_calls(&state);
    let r = ok("set_endpoints", json!({ "eth_rpc": e_a, "sequencer": seq_a }));
    assert_eq!(r["switched"], json!({ "eth_rpc": false, "sequencer": false }), "{r}");
    assert_eq!(set_calls(&state), calls, "the endpoints in use given again reach the chain service's switch");
    // A running job (a claim: the chain service holds its first reading for 1.5 s): the switch is refused at once.
    let j = ok("claim", json!({ "leaf_index": 0 }));
    let id = j["job"]["id"].as_u64().expect("the claim's job id");
    refused("set_endpoints", json!({ "sequencer": seq_b }), &["busy", "set_endpoints"]);
    let mut job_state = String::new();
    for _ in 0..200 {
        job_state = ok("job", json!({ "id": id }))["state"].as_str().unwrap_or("").to_string();
        if matches!(job_state.as_str(), "failed" | "settled" | "unconfirmed") {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(job_state, "failed", "the claim ended (its reading is refused here)");
    assert_eq!(in_service(&state), (e_a.clone(), seq_a.clone()), "nothing switched while it ran");

    // The eth path: Logos Core's eth_rpc_module holds the Ethereum connection; the chain service keeps LEZ.
    ferrata_bridge_calls::set_module_call(module_call);
    ferrata_bridge_calls::set_eth_rpc_module(raw_rpc);
    registry().endpoints.insert(CHAIN, e_a.clone());
    ok("eth", json!({ "via": "eth_rpc_module", "chain_id": CHAIN, "release_sender": RELEASE_SENDER }));
    let r = ok("set_endpoints", json!({ "eth_rpc": e_b, "sequencer": seq_a }));
    assert_eq!(r["switched"], json!({ "eth_rpc": true, "sequencer": false }), "{r}");
    assert!(r["via"]["eth_rpc"].as_str().unwrap_or("").contains("eth_rpc_module"), "{r}");
    assert_eq!(r["checks"]["eth_rpc"]["same_final_block"], json!(42), "the new endpoint has the final block of the one in use: {r}");
    assert_eq!(registry().endpoints.get(&CHAIN), Some(&e_b), "eth_rpc_module holds the new endpoint");
    assert_eq!(in_service(&state), (e_a.clone(), seq_a.clone()), "the chain service's own Ethereum RPC is not the session's here");
    let i = ok("info", json!({}));
    assert_eq!(i["eth_rpc"].as_str(), Some(e_b.as_str()), "{i}");
    assert!(i["endpoints"]["eth_rpc"]["via"].as_str().unwrap_or("").contains("eth_rpc_module"), "{i}");
    // Given again: no probe, no write.
    let patched = registry().patched.len();
    let r = ok("set_endpoints", json!({ "eth_rpc": e_b, "sequencer": seq_a }));
    assert_eq!(r["switched"], json!({ "eth_rpc": false, "sequencer": false }), "{r}");
    assert_eq!(registry().patched.len(), patched, "the endpoint in use given again is not written again");
    for (bad, why) in [
        (&e_other, "serves chain 560048"),
        (&e_novault, "no contract"),
        (&e_wrongcid, OTHER_CID),
        (&e_fork, "another history"),
        (&dead, "not switched"),
    ] {
        refused("set_endpoints", json!({ "eth_rpc": bad }), &["not switched", why, &format!("Kept: Ethereum RPC {e_b}")]);
        assert_eq!(registry().endpoints.get(&CHAIN), Some(&e_b), "{bad}: eth_rpc_module keeps the endpoint in use");
    }
    // A chain set to `required`: the vault's and the block's reads wait for the switch; they fail through eth_rpc_module's
    // route, so the endpoint and the sequencer switched with it are both put back (a fork, and another configuration).
    registry().required = true;
    for (bad, why) in [(&e_wrongcid, OTHER_CID), (&e_fork, "another history")] {
        let before = registry().patched.iter().filter(|u| *u == bad).count();
        refused("set_endpoints", json!({ "eth_rpc": bad, "sequencer": seq_b }), &["not switched", "after the switch", why, "Put back"]);
        assert_eq!(registry().endpoints.get(&CHAIN), Some(&e_b), "the endpoint put back");
        assert_eq!(registry().patched.iter().filter(|u| *u == bad).count(), before + 1, "it was switched once");
        assert_eq!(in_service(&state).1, seq_a, "the sequencer put back");
    }
    registry().required = false;

    // LEZ through lez_core: its wallet holds its own connection, so a sequencer change is refused; Ethereum still switches.
    ferrata_bridge_calls::set_lez_core(lez_core);
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("u9-endpoints");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("password"), "x\n").unwrap();
    std::fs::write(dir.join("bridge.bin"), b"not a program: never executed here").unwrap();
    let w = |f: &str| dir.join(f).to_string_lossy().into_owned();
    let lez = json!({ "via": "lez_core", "wallet": { "config": w("config.json"), "storage": w("storage.json"), "password_file": w("password") },
                      "program_bin": w("bridge.bin") });
    ok("lez", lez.clone());
    refused("set_endpoints", json!({ "sequencer": seq_b }), &["lez_core", "Kept"]);
    assert_eq!(in_service(&state).1, seq_a);
    ok("set_endpoints", json!({ "eth_rpc": e_a }));
    assert_eq!(registry().endpoints.get(&CHAIN), Some(&e_a));
    ok("lez", json!({ "via": "plain" }));
    ok("set_endpoints", json!({ "sequencer": seq_b }));
    assert_eq!(in_service(&state).1, seq_b);

    // The same deployment opened again (the view opened again): its paths stay.
    ok("lez", lez.clone());
    let o = ok("open", json!({ "chain": svc }));
    assert_eq!(o["paths_reset"], json!([]), "{o}");
    assert!(!ok("eth_report", json!({}))["path"].is_null(), "the same deployment keeps its Ethereum path");
    // Opened on another deployment, on chain 560048, the paths not configured again (review M-1): its Ethereum path
    // and LEZ path are dropped, so a switch goes to the new deployment's chain service and eth_rpc_module's record of
    // chain 11155111 is not touched (before the fix the chain-560048 RPC was written there and the reply said ok).
    let (e_y, e_y2) = (ethereum(HOODI, true, CID, HASH), ethereum(HOODI, true, CID, HASH));
    let y = Arc::new(Mutex::new(Service {
        chain: HOODI,
        seq: seq_a.clone(),
        eth: e_y.clone(),
        set_calls: 0,
        good_seqs: [seq_a.clone()].into(),
        good_eths: [e_y.clone(), e_y2.clone()].into(),
    }));
    let svc_y = chain_service(y.clone());
    let patched = registry().patched.len();
    let o = ok("open", json!({ "chain": svc_y }));
    assert_eq!(o["chain_id"], json!(HOODI), "{o}");
    assert_eq!(o["paths_reset"], json!(["eth", "lez"]), "{o}");
    assert!(ok("eth_report", json!({}))["path"].is_null(), "no Ethereum path after another deployment's open");
    let r = ok("set_endpoints", json!({ "eth_rpc": e_y2 }));
    assert!(r["via"]["eth_rpc"].as_str().unwrap_or("").contains("chain service"), "{r}");
    assert_eq!(in_service(&y).0, e_y2, "the new deployment's chain service switched its own Ethereum RPC");
    assert_eq!(registry().endpoints.get(&CHAIN), Some(&e_a), "eth_rpc_module's record of chain 11155111 untouched");
    assert_eq!(registry().patched.len(), patched, "nothing written to eth_rpc_module");
}
