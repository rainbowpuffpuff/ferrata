//! `ferrata_attest_module`'s own Ethereum reads against a failing endpoint (RFP-021 Reliability #5: "Temporary RPC or
//! connectivity failure on the Ethereum side leaves any off-chain component in a recoverable state, able to resume
//! without duplicating work already done").
//!
//! The module reads Ethereum in `fetch_log_proof` (`eth_getBlockByNumber`, then `eth_getBlockReceipts`), and in
//! `build_attest_tx` and `dry_run_attest` when their `storage` names an endpoint (`eth_getProof`); `verify_checkpoint`'s
//! providers are tested in the crate's own tests. Each is one call that reads and sends nothing. The first test reads
//! over the plain path (`rpc_url`, `rpc.rs`'s HTTP/1.1 client); the second over the route through Logos Core's
//! `eth_rpc_module`, the module's call replaced by a stand-in (the module crate installs the real one inside Logos Core). Against an endpoint that refuses connections, holds the request
//! without an answer, cuts its answer (in the first request, and in `fetch_log_proof`'s second after the first was
//! answered), answers HTTP 503, or answers with an error, each call returns an error reply (`invalid_request`, naming
//! the method and the failure) within its bound (`timeout_ms`, 1,000 here, per connection and per read); the same call
//! made again once the endpoint answers returns its result: a call keeps nothing, so nothing is resumed or done twice.
//!
//! The endpoint is a scripted JSON-RPC server on 127.0.0.1 answering from the recorded mainnet responses of
//! `crates/ferrata-spectests/fixtures/mainnet-attest-2026-11-10`. Nothing leaves 127.0.0.1.
//!
//! Run (after `modules/scripts/stage.sh ferrata_attest_module` and the SDK link, as `scripts/ci/modules-resolve.sh`
//! makes them): `cd modules/ferrata_attest_module/rust-lib && cargo test --locked -p ferrata-attest-calls --test
//! failing_endpoint -- --nocapture` (CI's `modules-resolve` job).

use std::{
    io::{BufRead as _, BufReader, Read as _, Write as _},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, AtomicU8, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use ferrata_attest_calls::{build_attest_tx, dry_run_attest, fetch_log_proof, set_eth_rpc_module};
use serde_json::{json, Value};

const FIXTURE: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../crates/ferrata-spectests/fixtures/mainnet-attest-2026-11-10");
const BLOCK: u64 = 26_384_851;
const RECORD_SLOT: u64 = 15_624_480;
const WETH: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
const WETH_SLOT: &str = "0xb374801ace2c02f5db0425ab5920a2b7ed1d5a00abbcd395fda7530ba1d666c0";
/// Each call's bound in these requests (`timeout_ms`): per connection and per read.
const TIMEOUT_MS: u64 = 1_000;

fn fixture(name: &str) -> PathBuf {
    Path::new(FIXTURE).join(name)
}

fn result_of(path: &Path) -> Value {
    let v: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    if v.get("jsonrpc").is_some() {
        v["result"].clone()
    } else {
        v
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fault {
    None,
    /// Takes the request and never answers.
    Hang,
    /// Sends the head and half the body (its `Content-Length` the whole), then closes.
    Cut,
    Http503,
    /// A JSON-RPC error.
    RpcError,
}

struct Endpoint {
    fault: Fault,
    /// The method the fault applies to (all if none).
    only: Option<&'static str>,
    /// Every request's method, in order.
    calls: Vec<String>,
}

/// The recorded answer to `method(params)`.
fn recorded(method: &str, params: &Value) -> Result<Value, String> {
    let tag = json!(format!("0x{BLOCK:x}"));
    match method {
        "eth_getBlockByNumber" if params[0] == tag => Ok(result_of(&fixture(&format!("block_{BLOCK}.json")))),
        "eth_getBlockReceipts" if params[0] == tag => Ok(result_of(&fixture(&format!("receipts_{BLOCK}.json")))),
        "eth_getProof" if params[2] == tag && params[0].as_str().unwrap_or("").eq_ignore_ascii_case(WETH) => {
            let mut r = result_of(&fixture(&format!("proof_weth_{BLOCK}.json")));
            let keys: Vec<String> =
                params[1].as_array().into_iter().flatten().filter_map(|k| k.as_str()).map(str::to_lowercase).collect();
            let kept: Vec<Value> = r["storageProof"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|e| keys.contains(&e["key"].as_str().unwrap_or("").to_lowercase()))
                .cloned()
                .collect();
            r["storageProof"] = Value::Array(kept);
            Ok(r)
        }
        _ => Err(format!("{method}: not recorded")),
    }
}

fn respond(w: &mut TcpStream, status: &str, body: &[u8]) {
    let _ = write!(
        w,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = w.write_all(body);
    let _ = w.flush();
}

/// The endpoint on 127.0.0.1: its URL.
fn serve(endpoint: Arc<Mutex<Endpoint>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let endpoint = endpoint.clone();
            std::thread::spawn(move || {
                let mut r = BufReader::new(stream.try_clone().unwrap());
                let mut w = stream;
                // The request: its head, then its body.
                let mut len = 0;
                loop {
                    let mut line = String::new();
                    if r.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(v) = line.to_lowercase().strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0; len];
                if r.read_exact(&mut body).is_err() {
                    return;
                }
                let req: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                let method = req["method"].as_str().unwrap_or("").to_string();
                let fault = {
                    let mut e = endpoint.lock().unwrap();
                    e.calls.push(method.clone());
                    if e.only.is_none_or(|m| m == method) {
                        e.fault
                    } else {
                        Fault::None
                    }
                };
                let answer = |result: Result<Value, String>| match result {
                    Ok(v) => json!({ "jsonrpc": "2.0", "id": req["id"], "result": v }),
                    Err(e) => json!({ "jsonrpc": "2.0", "id": req["id"], "error": { "code": -32000, "message": e } }),
                };
                match fault {
                    Fault::Hang => std::thread::sleep(Duration::from_secs(30)),
                    Fault::Http503 => respond(&mut w, "503 Service Unavailable", b"busy"),
                    Fault::RpcError => {
                        respond(&mut w, "200 OK", answer(Err("missing trie node".into())).to_string().as_bytes())
                    }
                    Fault::Cut => {
                        let bytes = answer(recorded(&method, &req["params"])).to_string().into_bytes();
                        let _ = write!(
                            w,
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                            bytes.len()
                        );
                        let _ = w.write_all(&bytes[..bytes.len() / 2]);
                        let _ = w.flush();
                    }
                    Fault::None => {
                        respond(&mut w, "200 OK", answer(recorded(&method, &req["params"])).to_string().as_bytes())
                    }
                }
            });
        }
    });
    url
}

/// The record of the fixture's block, as `dry_run_attest` takes its fields.
fn record() -> Value {
    let b = result_of(&fixture(&format!("block_{BLOCK}.json")));
    json!({
        "chain_id": 1,
        "anchor_slot": RECORD_SLOT,
        "block_number": b["number"],
        "block_hash": b["hash"],
        "parent_hash": b["parentHash"],
        "state_root": b["stateRoot"],
        "receipts_root": b["receiptsRoot"],
        "timestamp": b["timestamp"],
    })
}

/// The calls that read Ethereum.
const CALLS: [&str; 3] = ["fetch_log_proof", "build_attest_tx", "dry_run_attest"];

/// `name`'s request reading from `url`.
fn request(name: &str, url: &str) -> String {
    request_via(name, &json!({ "rpc_url": url }))
}

/// `name`'s request reading over `route`: `{"rpc_url": URL}` or `{"eth": {"via": "eth_rpc_module", "chain_id": N}}`.
fn request_via(name: &str, route: &Value) -> String {
    let with_route = |mut v: Value| {
        for (k, x) in route.as_object().unwrap() {
            v[k] = x.clone();
        }
        v
    };
    let storage = with_route(json!({ "address": WETH, "slot": WETH_SLOT, "timeout_ms": TIMEOUT_MS }));
    match name {
        "fetch_log_proof" => {
            with_route(json!({ "block": BLOCK, "tx_index": 0, "log_index": 0, "timeout_ms": TIMEOUT_MS }))
        }
        "build_attest_tx" => {
            let mut at_block = storage;
            at_block["block"] = json!(BLOCK);
            json!({
                "program_id": format!("0x{}", "11".repeat(32)),
                "instance_id": format!("0x{}", "22".repeat(32)),
                "chain_id": 1,
                "slot": RECORD_SLOT,
                "storage": at_block,
            })
        }
        _ => json!({ "chain_id": 1, "record": record(), "storage": storage }),
    }
    .to_string()
}

/// `name` called on `url`, timed: its reply and how long it took.
fn call(name: &str, url: &str) -> (Value, Duration) {
    call_request(name, request(name, url))
}

/// `name` called with `req`, timed.
fn call_request(name: &str, req: String) -> (Value, Duration) {
    let t = Instant::now();
    let reply = match name {
        "fetch_log_proof" => fetch_log_proof(&req),
        "build_attest_tx" => build_attest_tx(&req),
        _ => dry_run_attest(&req),
    };
    (serde_json::from_str(&reply).unwrap(), t.elapsed())
}

/// `name`'s reply is its result: a log's proof fetched, a message built, a slot attested.
fn succeeded(name: &str, reply: &Value) {
    match name {
        "fetch_log_proof" => assert_eq!(reply["verdict"], "fetched", "{name}: {reply}"),
        "build_attest_tx" => assert!(reply["message"].as_str().is_some_and(|m| m.len() > 2), "{name}: {reply}"),
        _ => assert_eq!(reply["verdict"], "attested", "{name}: {reply}"),
    }
}

#[test]
fn each_ethereum_read_fails_within_its_bound_and_succeeds_once_the_endpoint_answers() {
    let endpoint = Arc::new(Mutex::new(Endpoint { fault: Fault::None, only: None, calls: Vec::new() }));
    let url = serve(endpoint.clone());
    let closed = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}", l.local_addr().unwrap())
    };
    // The calls succeed against the endpoint answering.
    for name in CALLS {
        succeeded(name, &call(name, &url).0);
    }
    let cases: Vec<(&str, Fault, &str)> = vec![
        ("refused", Fault::None, "connect"),
        ("hang", Fault::Hang, "read"),
        ("cut", Fault::Cut, "truncated body"),
        ("http-503", Fault::Http503, "503"),
        ("rpc-error", Fault::RpcError, "the node answered with an error"),
    ];
    let mut lines = Vec::new();
    for (case, fault, expect) in cases {
        for name in CALLS {
            {
                let mut e = endpoint.lock().unwrap();
                (e.fault, e.only) = (fault, None);
            }
            let target = if case == "refused" { closed.as_str() } else { url.as_str() };
            let (reply, took) = call(name, target);
            assert_eq!(reply["verdict"], "invalid_request", "{case} {name}: {reply}");
            let error = reply["error"].as_str().unwrap_or_default().to_string();
            let method = if name == "fetch_log_proof" { "eth_getBlockByNumber" } else { "eth_getProof" };
            assert!(error.starts_with(method) && error.contains(expect), "{case} {name}: {error}");
            // Within its bound: one connection and its reads, each at most TIMEOUT_MS (a held request is cut by it).
            let bound = Duration::from_millis(TIMEOUT_MS + 1_500);
            assert!(took < bound, "{case} {name}: {took:?} (bound {bound:?})");
            if fault == Fault::Hang {
                assert!(took >= Duration::from_millis(TIMEOUT_MS - 100), "{case} {name}: {took:?}");
            }
            // The endpoint answering again, the same call returns its result: nothing was kept from the failure.
            endpoint.lock().unwrap().fault = Fault::None;
            succeeded(name, &call(name, &url).0);
            lines.push(format!("{case:<10} {name:<16} {:>5} ms  {error}", took.as_millis()));
        }
    }
    // A failure mid-sequence: `fetch_log_proof`'s receipts cut after its block was answered.
    {
        let mut e = endpoint.lock().unwrap();
        (e.fault, e.only) = (Fault::Cut, Some("eth_getBlockReceipts"));
        e.calls.clear();
    }
    let (reply, took) = call("fetch_log_proof", &url);
    let error = reply["error"].as_str().unwrap_or_default().to_string();
    assert_eq!(reply["verdict"], "invalid_request", "{reply}");
    assert!(error.starts_with("eth_getBlockReceipts") && error.contains("truncated body"), "{error}");
    assert_eq!(endpoint.lock().unwrap().calls, ["eth_getBlockByNumber", "eth_getBlockReceipts"]);
    assert!(took < Duration::from_millis(2 * TIMEOUT_MS + 1_500), "{took:?}");
    lines.push(format!("{:<10} {:<16} {:>5} ms  {error}", "cut-second", "fetch_log_proof", took.as_millis()));
    endpoint.lock().unwrap().fault = Fault::None;
    succeeded("fetch_log_proof", &call("fetch_log_proof", &url).0);
    for l in &lines {
        println!("{l}");
    }
    println!("{} failing calls, each an error within its bound, each answered once the endpoint was back", lines.len());
}

/// What the stand-in for Logos Core's `eth_rpc_module.raw_rpc` does: 0 answers from the recorded responses; 1 the call
/// does not reach the module; 2 the module answers `ok: false` (the endpoint behind it failed); 3 a reply that is not
/// JSON.
static MODULE_MODE: AtomicU8 = AtomicU8::new(0);
/// The bound the calls crate handed the module (Logos's module enforces it on its own request).
static MODULE_TIMEOUT: AtomicU64 = AtomicU64::new(0);

/// The stand-in for `eth_rpc_module.raw_rpc(chain_id, method, params)`, installed with `set_eth_rpc_module` as the
/// module crate installs Logos Core's call.
fn module_stub(chain_id: i64, method: &str, params: &str, timeout_ms: u64) -> Result<String, String> {
    MODULE_TIMEOUT.store(timeout_ms, Ordering::SeqCst);
    assert_eq!(chain_id, 1);
    match MODULE_MODE.load(Ordering::SeqCst) {
        1 => Err("eth_rpc_module did not answer (the call into Logos Core failed)".into()),
        2 => Ok(json!({ "ok": false, "code": "upstream_timeout", "error": "the endpoint did not answer in time" })
            .to_string()),
        3 => Ok("<html>502 Bad Gateway</html>".into()),
        _ => {
            let params: Value = serde_json::from_str(params).unwrap();
            Ok(match recorded(method, &params) {
                Ok(r) => json!({ "ok": true, "result": r, "route": "direct" }),
                Err(e) => json!({ "ok": false, "code": "not_found", "error": e }),
            }
            .to_string())
        }
    }
}

/// The route through Logos Core's `eth_rpc_module` (`"eth": {"via": "eth_rpc_module", "chain_id": 1}`), with the module's
/// call replaced by a stand-in answering from the same recorded responses or failing: the call not reaching the module,
/// the module answering `ok: false`, a reply that is not JSON. Each of the three calls returns an error reply; the
/// stand-in is handed the request's `timeout_ms` (Logos's module bounds its own request with it); the same call
/// succeeds once the stand-in answers, over that route.
#[test]
fn the_eth_rpc_module_route_fails_cleanly_and_recovers() {
    set_eth_rpc_module(module_stub);
    let route = json!({ "eth": { "via": "eth_rpc_module", "chain_id": 1 } });
    for name in CALLS {
        MODULE_MODE.store(0, Ordering::SeqCst);
        let (reply, _) = call_request(name, request_via(name, &route));
        succeeded(name, &reply);
        assert_eq!(reply["eth"]["via"], "eth_rpc_module", "{name}: {reply}");
    }
    let mut lines = Vec::new();
    for (mode, case, expect) in [
        (1u8, "unreached", "did not answer"),
        (2, "ok-false", "eth_rpc_module answered"),
        (3, "not-json", "is not JSON"),
    ] {
        for name in CALLS {
            MODULE_MODE.store(mode, Ordering::SeqCst);
            MODULE_TIMEOUT.store(0, Ordering::SeqCst);
            let (reply, took) = call_request(name, request_via(name, &route));
            assert_eq!(reply["verdict"], "invalid_request", "{case} {name}: {reply}");
            let error = reply["error"].as_str().unwrap_or_default().to_string();
            assert!(error.contains(expect), "{case} {name}: {error}");
            assert_eq!(
                MODULE_TIMEOUT.load(Ordering::SeqCst),
                TIMEOUT_MS,
                "{case} {name}: the bound handed to the module"
            );
            MODULE_MODE.store(0, Ordering::SeqCst);
            succeeded(name, &call_request(name, request_via(name, &route)).0);
            lines.push(format!("{case:<10} {name:<16} {:>5} ms  {error}", took.as_millis()));
        }
    }
    for l in &lines {
        println!("{l}");
    }
    println!(
        "{} failing calls over eth_rpc_module, each an error, each answered once the module answered",
        lines.len()
    );
}
