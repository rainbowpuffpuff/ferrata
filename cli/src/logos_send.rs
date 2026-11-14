//! A daemon's Ethereum transactions through Logos Core's send stack (`--send-via logos`; only inside its module,
//! `ferrata_monitor_module` or `ferrata_relayer_module`, which installs the call: natively there is none and the flag
//! is refused). The same file is `monitor/src/logos_send.rs` and `cli/src/logos_send.rs`.
//!
//! `tx_sender_module.send` prices the call through `fee_module`, reserves the nonce and asks
//! `signer_manager_module` for one approval; `keystore_module` signs. Under an unlock the custodian made for
//! `tx_sender_module` without confirmation ([`custody`]) the approval is signed as it arrives, with no person
//! (signer manager `request_approval`: "Inside an unlock the person granted without confirmation, it is signed as
//! it arrives"). `send_status`, polled, is what broadcasts (through `eth_rpc_module.send_raw_transaction`). The
//! daemon then waits for the receipt on its own node, as on the plain path.
//!
//! Sources (github.com/logos-co, the revisions modules/scripts/send-stack.sh builds): logos-evm-tx-sender-module
//! f1754c3 `rust-lib/src/glue.rs` 45-137 (`prepare`, `send`, `send_status`, `cancel_send`); logos-signer-manager-module
//! bd5c84d `rust-lib/src/glue.rs` 27-120 (`configure`, `unlock`, `request_approval`), `access.rs` 755-800;
//! logos-evm-keystore-module e83a899 `rust-lib/src/glue.rs` 36-45 (`configure`), 466-479 (an import enrols its
//! wallet with the signer manager), `gate.rs` 145-224 (tier D: a configured custodian module, never the host).

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use alloy::primitives::{Address, Bytes, B256};
use serde_json::{json, Value};

/// One call of `method` of the Logos module `module` with the JSON array `args`, by name through Logos Core
/// (the SDK's `call_json_with_timeout`); the method's reply as text, or why there is none.
pub type ModuleCall = fn(module: &str, method: &str, args: &Value, timeout: Duration) -> Result<String, String>;

static CALL: OnceLock<ModuleCall> = OnceLock::new();

/// Installed once by the module crate; natively nothing is.
pub fn install(call: ModuleCall) {
    let _ = CALL.set(call);
}

/// The installed call, if the monitor runs inside Logos Core.
pub fn installed() -> Option<ModuleCall> {
    CALL.get().copied()
}

pub const TX_SENDER: &str = "tx_sender_module";
pub const SIGNER_MANAGER: &str = "signer_manager_module";
pub const KEYSTORE: &str = "keystore_module";

/// One call's own timeout (the module's `send` prices the bundle first: a fee estimate and a nonce read).
const CALL_TIMEOUT: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(200);

/// How a transaction went out, for the alert or the line that reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Via {
    /// Signed here with the daemon's own key and sent over its RPC endpoint.
    Plain,
    /// Through `tx_sender_module`: its request id, `send_status` polls, milliseconds from `send` to the broadcast.
    Logos { request_id: String, polls: u32, millis: u64 },
    /// `tx_sender_module` did not broadcast it (why), so it went the plain way.
    Fallback { reason: String },
}

impl Via {
    pub fn to_json(&self) -> Value {
        match self {
            Via::Plain => json!({ "path": "plain" }),
            Via::Logos { request_id, polls, millis } => {
                json!({ "path": "logos", "module": TX_SENDER, "requestId": request_id, "statusPolls": polls,
                        "millisToBroadcast": millis })
            }
            Via::Fallback { reason } => json!({ "path": "plain", "fallback": true, "reason": reason }),
        }
    }
}

/// Where an attempt through `tx_sender_module` ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Broadcast: the daemon waits for its receipt.
    Broadcast { hash: B256, request_id: String, polls: u32, millis: u64 },
    /// Nothing was signed and broadcast: the plain path may send it.
    NotSent(String),
    /// It may have left (`stuck`: a broadcast whose RPC did not answer): never sent a second time here; the next poll
    /// looks at the vault again.
    Unknown(String),
}

/// A daemon's sends through Logos Core: which chain, which keystore account.
#[derive(Clone, Debug)]
pub struct Route {
    pub call: ModuleCall,
    pub chain_id: u64,
    pub from: Address,
    /// Who asks, in the purpose line the signer shows ("Ferrata monitor: cancelCandidate").
    pub purpose: &'static str,
    /// The daemon holds its own key too: what `tx_sender_module` did not broadcast goes the plain way.
    pub fallback: bool,
    /// How long a request may wait for its approval before it is withdrawn (`cancel_send`).
    pub approval_timeout: Duration,
}

fn parse(reply: &str) -> Value {
    serde_json::from_str(reply).unwrap_or_else(|_| json!({ "ok": false, "error": reply }))
}

fn why(v: &Value) -> String {
    let code = v.get("code").and_then(Value::as_str).map(|c| format!("{c}: ")).unwrap_or_default();
    let error = v.get("error").or_else(|| v.get("reason")).map(|e| e.as_str().map_or(e.to_string(), str::to_string));
    format!("{code}{}", error.unwrap_or_else(|| v.to_string()))
}

impl Route {
    /// The request `tx_sender_module.send` takes for one call of `to` with `input` (no value; the gas estimated by
    /// `fee_module` as the chain will find it).
    pub fn request(&self, to: Address, input: &Bytes, label: &str) -> Value {
        json!({
            "chainId": self.chain_id,
            "from": self.from.to_checksum(None),
            "calls": [{ "to": to.to_checksum(None), "value": "0", "data": input.to_string(), "label": label }],
            "purpose": format!("{}: {label}", self.purpose),
        })
    }

    /// `send`, then `send_status` until it is final. Blocking: run it off the async runtime.
    pub fn send(&self, to: Address, input: &Bytes, label: &str) -> Outcome {
        let t0 = Instant::now();
        let request = self.request(to, input, label).to_string();
        let v = loop {
            let v = match (self.call)(TX_SENDER, "send", &json!([request]), CALL_TIMEOUT) {
                Ok(r) => parse(&r),
                Err(e) => return Outcome::NotSent(format!("{TX_SENDER}.send did not answer: {e}")),
            };
            // `unavailable`: the account's history is still opening (its key being asked of the signer manager);
            // nothing was reserved or signed, so it is asked again until the approval timeout.
            let opening = v.get("code").and_then(Value::as_str) == Some("unavailable");
            if v.get("ok") == Some(&Value::Bool(true)) || !opening || t0.elapsed() > self.approval_timeout {
                break v;
            }
            std::thread::sleep(POLL);
        };
        if v.get("ok") != Some(&Value::Bool(true)) {
            return Outcome::NotSent(format!("{TX_SENDER}.send refused: {}", why(&v)));
        }
        let Some(id) = v.get("requestId").and_then(Value::as_str).map(str::to_string) else {
            return Outcome::NotSent(format!("{TX_SENDER}.send answered no requestId: {v}"));
        };
        let mut polls = 0u32;
        loop {
            polls += 1;
            let s = match (self.call)(TX_SENDER, "send_status", &json!([id]), CALL_TIMEOUT) {
                Ok(r) => parse(&r),
                Err(e) => json!({ "ok": false, "error": format!("send_status did not answer: {e}"), "final": false }),
            };
            let millis = u64::try_from(t0.elapsed().as_millis()).unwrap_or(u64::MAX);
            let hashes: Vec<B256> = s
                .get("hashes")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|h| h.as_str()?.parse().ok())
                .collect();
            let is_final = s.get("final") == Some(&Value::Bool(true));
            let status = s.get("status").and_then(Value::as_str).unwrap_or("");
            if s.get("ok") == Some(&Value::Bool(true)) && status == "broadcast" {
                let hash =
                    s.get("hash").and_then(Value::as_str).and_then(|h| h.parse().ok()).or(hashes.first().copied());
                return match hash {
                    Some(hash) => Outcome::Broadcast { hash, request_id: id, polls, millis },
                    None => Outcome::Unknown(format!("{TX_SENDER} says broadcast without a hash: {s}")),
                };
            }
            if let Some(&hash) = hashes.first() {
                // A leg went out; whatever the bundle's status, it is waited for, never sent again.
                if is_final {
                    return Outcome::Broadcast { hash, request_id: id, polls, millis };
                }
            } else if is_final {
                return match status {
                    "stuck" => Outcome::Unknown(format!("{TX_SENDER}: request {id} stuck ({})", why(&s))),
                    _ => Outcome::NotSent(format!("{TX_SENDER}: request {id} {status} ({})", why(&s))),
                };
            }
            if t0.elapsed() > self.approval_timeout {
                if status == "awaitingApproval" || (s.get("ok") != Some(&Value::Bool(true)) && hashes.is_empty()) {
                    let c = (self.call)(TX_SENDER, "cancel_send", &json!([id]), CALL_TIMEOUT).unwrap_or_default();
                    return Outcome::NotSent(format!(
                        "{TX_SENDER}: request {id} not approved within {:?} (withdrawn: {c}); is the account unlocked \
                         for {TX_SENDER} without confirmation (`custody`)?",
                        self.approval_timeout
                    ));
                }
                return Outcome::Unknown(format!("{TX_SENDER}: request {id} still {status} after {:?}", t0.elapsed()));
            }
            std::thread::sleep(POLL);
        }
    }
}

/// What [`custody`] does: the key into `keystore_module` and an unlock for `tx_sender_module`.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct CustodyRequest {
    /// A file with the key (hex), as `--key-file`: the key never travels in a request.
    pub key_file: std::path::PathBuf,
    /// A file with the vault password the keystore encrypts it under.
    pub password_file: std::path::PathBuf,
    /// The unlock's lifetime and its number of signatures (the signer manager's terms).
    #[serde(default = "CustodyRequest::ttl")]
    pub ttl_ms: u64,
    #[serde(default)]
    pub count: Option<u32>,
    /// The modules the unlock covers (default `tx_sender_module`).
    #[serde(default = "CustodyRequest::apps")]
    pub apps: Vec<String>,
}

impl CustodyRequest {
    fn ttl() -> u64 {
        3_600_000
    }
    fn apps() -> Vec<String> {
        vec![TX_SENDER.to_string()]
    }
}

/// The custodian's two steps, made by the module that calls this (it must be a custodian of both
/// `keystore_module` and `signer_manager_module`: their `configure`): `keystore_module.import_private_key`
/// (tier D; the keystore enrols the new wallet with the signer manager itself), then, once the signer manager
/// lists the account, `signer_manager_module.unlock` for `apps` with `confirm: false`, so that `tx_sender_module`'s
/// approvals for it are signed as they arrive. An account the keystore holds already is unlocked as it is.
pub fn custody(call: ModuleCall, req: &CustodyRequest) -> Value {
    let read = |p: &std::path::Path| std::fs::read_to_string(p).map(|s| s.trim_end_matches(['\n', '\r']).to_string());
    let key = match read(&req.key_file) {
        Ok(k) => k.trim().trim_start_matches("0x").to_string(),
        Err(e) => return json!({ "verdict": "error", "error": format!("reading the key file: {e}") }),
    };
    let password = match read(&req.password_file) {
        Ok(p) => p,
        Err(e) => return json!({ "verdict": "error", "error": format!("reading the password file: {e}") }),
    };
    let address = match key.parse::<alloy::signers::local::PrivateKeySigner>() {
        Ok(s) => s.address(),
        Err(e) => return json!({ "verdict": "error", "error": format!("the key does not parse: {e}") }),
    };
    let t0 = Instant::now();
    let imported = match call(KEYSTORE, "import_private_key", &json!([key, password]), CALL_TIMEOUT) {
        Ok(r) => parse(&r),
        Err(e) => return json!({ "verdict": "error", "error": format!("{KEYSTORE}.import_private_key: {e}") }),
    };
    let held_already =
        imported.get("ok") != Some(&Value::Bool(true)) && why(&imported).to_lowercase().contains("exist");
    if imported.get("ok") != Some(&Value::Bool(true)) && !held_already {
        return json!({ "verdict": "refused", "step": "import_private_key", "address": address.to_checksum(None),
                       "reply": imported });
    }
    // The keystore enrols the wallet with the signer manager in its own thread: wait until the manager lists it.
    let mut listed = Value::Null;
    for _ in 0..100 {
        if let Ok(r) = call(SIGNER_MANAGER, "accounts", &json!([]), CALL_TIMEOUT) {
            let v = parse(&r);
            let hit = v.get("accounts").and_then(Value::as_array).into_iter().flatten().find(|a| {
                a.get("address").and_then(Value::as_str).and_then(|s| s.parse::<Address>().ok()) == Some(address)
            });
            if let Some(a) = hit {
                listed = a.clone();
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    if listed.is_null() {
        return json!({ "verdict": "refused", "step": "accounts", "address": address.to_checksum(None),
                       "error": format!("{SIGNER_MANAGER} does not list the imported account after 20 s") });
    }
    // The manager finds an account by its `id` (registry.rs `same_id`), which `accounts` lists beside the address.
    let account = listed.get("id").and_then(Value::as_str).map_or_else(|| address.to_checksum(None), str::to_string);
    let terms = json!({ "account": account, "password": password, "ttlMs": req.ttl_ms,
                        "count": req.count, "apps": req.apps, "confirm": false });
    let unlocked = match call(SIGNER_MANAGER, "unlock", &json!([terms.to_string()]), CALL_TIMEOUT) {
        Ok(r) => parse(&r),
        Err(e) => return json!({ "verdict": "error", "error": format!("{SIGNER_MANAGER}.unlock: {e}") }),
    };
    if unlocked.get("ok") != Some(&Value::Bool(true)) {
        return json!({ "verdict": "refused", "step": "unlock", "address": address.to_checksum(None),
                       "reply": unlocked });
    }
    // tx_sender_module opens an account's history with its key from the signer manager, in its own thread, and
    // refuses a send until it has (`code: unavailable`, then `locked` if the wallet is locked): wait for it here.
    let mut history = Value::Null;
    if req.apps.iter().any(|a| a == TX_SENDER) {
        for _ in 0..100 {
            history = match call(TX_SENDER, "history", &json!([address.to_checksum(None), 0]), CALL_TIMEOUT) {
                Ok(r) => parse(&r),
                Err(e) => json!({ "ok": false, "error": e }),
            };
            if history.get("ok") == Some(&Value::Bool(true)) {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        if history.get("ok") != Some(&Value::Bool(true)) {
            return json!({ "verdict": "refused", "step": "history", "address": address.to_checksum(None),
                           "reply": history, "unlock": unlocked });
        }
    }
    json!({
        "verdict": "ok",
        "address": address.to_checksum(None),
        "imported": if held_already { json!("held already") } else { imported },
        "listed": listed,
        "unlock": unlocked,
        "historyOpen": history.get("ok") == Some(&Value::Bool(true)),
        "apps": req.apps,
        "confirm": false,
        "millis": u64::try_from(t0.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // One fake Logos Core per test binary run: the calls it saw and the replies it gives, by method.
    static SEEN: Mutex<Vec<(String, String, Value)>> = Mutex::new(Vec::new());
    static SCRIPT: Mutex<Vec<(&'static str, String)>> = Mutex::new(Vec::new());
    static SERIAL: Mutex<()> = Mutex::new(());

    fn fake(module: &str, method: &str, args: &Value, _t: Duration) -> Result<String, String> {
        SEEN.lock().unwrap().push((module.into(), method.into(), args.clone()));
        let mut s = SCRIPT.lock().unwrap();
        match s.iter().position(|(m, _)| *m == method) {
            Some(i) => Ok(s.remove(i).1),
            None => Err(format!("no reply scripted for {method}")),
        }
    }

    fn route() -> Route {
        Route {
            call: fake,
            chain_id: 31337,
            from: "0x70997970C51812dc3A010C7d01b50e0d17dc79C8".parse().unwrap(),
            purpose: "Ferrata monitor",
            fallback: true,
            approval_timeout: Duration::from_millis(600),
        }
    }

    fn script(replies: &[(&'static str, Value)]) {
        SEEN.lock().unwrap().clear();
        *SCRIPT.lock().unwrap() = replies.iter().map(|(m, v)| (*m, v.to_string())).collect();
    }

    const H: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";

    #[test]
    fn a_broadcast_send_answers_its_hash_and_the_request_carries_the_calldata() {
        let _g = SERIAL.lock().unwrap();
        script(&[
            ("send", json!({ "ok": true, "pending": true, "requestId": "r1", "handle": "h1" })),
            ("send_status", json!({ "ok": true, "status": "awaitingApproval", "final": false, "hashes": [] })),
            ("send_status", json!({ "ok": true, "status": "broadcast", "final": true, "hashes": [H], "hash": H })),
        ]);
        let to: Address = "0x5FbDB2315678afecb367f032d93F642f64180aa3".parse().unwrap();
        let input: Bytes = vec![0xab, 0xcd].into();
        match route().send(to, &input, "cancelCandidate") {
            Outcome::Broadcast { hash, request_id, polls, .. } => {
                assert_eq!(hash, H.parse::<B256>().unwrap());
                assert_eq!(request_id, "r1");
                assert_eq!(polls, 2);
            }
            o => panic!("{o:?}"),
        }
        let seen = SEEN.lock().unwrap();
        assert_eq!(seen[0].0, TX_SENDER);
        let req: Value = serde_json::from_str(seen[0].2[0].as_str().unwrap()).unwrap();
        assert_eq!(req["calls"][0]["data"], "0xabcd");
        assert_eq!(req["calls"][0]["to"], "0x5FbDB2315678afecb367f032d93F642f64180aa3");
        assert_eq!(req["from"], "0x70997970C51812dc3A010C7d01b50e0d17dc79C8");
        assert_eq!(req["chainId"], 31337);
        assert_eq!(req["purpose"], "Ferrata monitor: cancelCandidate");
        assert_eq!(seen[1].2, json!(["r1"]));
    }

    #[test]
    fn a_refused_or_unapproved_send_is_not_sent_and_an_unapproved_one_is_withdrawn() {
        let _g = SERIAL.lock().unwrap();
        script(&[("send", json!({ "ok": false, "code": "locked", "error": "the account's wallet is locked" }))]);
        let to = Address::ZERO;
        assert!(matches!(route().send(to, &Bytes::new(), "x"), Outcome::NotSent(w) if w.contains("locked")));
        let waiting = json!({ "ok": true, "status": "awaitingApproval", "final": false, "hashes": [] });
        let mut replies = vec![("send", json!({ "ok": true, "requestId": "r2" }))];
        replies.extend(std::iter::repeat_n(("send_status", waiting), 10));
        replies.push(("cancel_send", json!({ "ok": true })));
        script(&replies);
        match route().send(to, &Bytes::new(), "x") {
            Outcome::NotSent(w) => assert!(w.contains("not approved"), "{w}"),
            o => panic!("{o:?}"),
        }
        assert!(SEEN.lock().unwrap().iter().any(|(_, m, a)| m == "cancel_send" && *a == json!(["r2"])));
    }

    #[test]
    fn a_send_refused_while_the_history_opens_is_asked_again() {
        let _g = SERIAL.lock().unwrap();
        script(&[
            ("send", json!({ "ok": false, "code": "unavailable", "error": "This account's history isn't open yet" })),
            ("send", json!({ "ok": true, "requestId": "r5" })),
            ("send_status", json!({ "ok": true, "status": "broadcast", "final": true, "hashes": [H], "hash": H })),
        ]);
        assert!(matches!(route().send(Address::ZERO, &Bytes::new(), "x"), Outcome::Broadcast { polls: 1, .. }));
    }

    #[test]
    fn a_stuck_send_is_never_reported_as_not_sent() {
        let _g = SERIAL.lock().unwrap();
        script(&[
            ("send", json!({ "ok": true, "requestId": "r3" })),
            ("send_status", json!({ "ok": true, "status": "stuck", "final": true, "hashes": [] })),
        ]);
        assert!(matches!(route().send(Address::ZERO, &Bytes::new(), "x"), Outcome::Unknown(_)));
        script(&[
            ("send", json!({ "ok": true, "requestId": "r4" })),
            ("send_status", json!({ "ok": true, "status": "failed", "final": true, "hashes": [H] })),
        ]);
        assert!(matches!(route().send(Address::ZERO, &Bytes::new(), "x"), Outcome::Broadcast { .. }));
    }

    #[test]
    fn custody_imports_waits_for_the_manager_and_unlocks_for_tx_sender_without_confirmation() {
        let _g = SERIAL.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("ferrata-logos-send-custody-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (k, p) = (dir.join("k"), dir.join("p"));
        std::fs::write(&k, "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d\n").unwrap();
        std::fs::write(&p, "pw\n").unwrap();
        let a = "0x70997970C51812dc3A010C7d01b50e0d17dc79C8";
        script(&[
            ("import_private_key", json!({ "ok": true, "address": a })),
            ("accounts", json!({ "ok": true, "accounts": [] })),
            (
                "accounts",
                json!({ "ok": true, "accounts": [{ "id": a.to_lowercase(), "address": a, "family": "evm", "signer": "keystore_module" }] }),
            ),
            ("unlock", json!({ "ok": true, "account": a, "expiresInMs": 3600000, "countLeft": null })),
            ("history", json!({ "ok": false, "code": "unavailable", "address": a })),
            ("history", json!({ "ok": true, "address": a, "transactions": [] })),
        ]);
        let req = CustodyRequest {
            key_file: k,
            password_file: p,
            ttl_ms: 3_600_000,
            count: None,
            apps: CustodyRequest::apps(),
        };
        let r = custody(fake, &req);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(r["verdict"], "ok", "{r}");
        assert_eq!(r["address"], a);
        let seen = SEEN.lock().unwrap();
        assert_eq!(seen[0].2[0], "59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d");
        let unlock = seen.iter().find(|(_, m, _)| m == "unlock").unwrap();
        let unlock: Value = serde_json::from_str(unlock.2[0].as_str().unwrap()).unwrap();
        assert_eq!(unlock["apps"], json!(["tx_sender_module"]));
        assert_eq!(unlock["confirm"], false);
        assert_eq!(unlock["account"], a.to_lowercase());
        assert_eq!(r["historyOpen"], true);
        assert_eq!(seen.last().unwrap().1, "history");
    }
}
