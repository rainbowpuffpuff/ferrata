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
    /// Nothing was signed and broadcast, by `tx_sender_module`'s own account of a request it holds (refused, rejected,
    /// cancelled, failed before any leg began to leave, or withdrawn unapproved): the plain path may send it.
    NotSent(String),
    /// It may have left, and `tx_sender_module` gives no hash: `stuck` (a broadcast whose RPC did not answer), a leg
    /// that began to leave and failed, a request still moving past the approval timeout or whose withdrawal was not
    /// confirmed, `send_status` not answering, or (`forgotten`) a request the module no longer holds (restarted: its
    /// sends are in memory). The relayer never sends it a second time on that word: the chain decides, at the leg's
    /// `nonce` when the module said it (and, for a forgotten request, the module's durable history:
    /// [`Route::history_row`]). The monitor, holding its watcher key, sends it the plain way at its node's next nonce
    /// (`monitor/src/chain.rs`: a duplicate of its calls reverts or moves a pause's end; a number the module burnt
    /// without broadcasting is filled).
    Unknown { reason: String, forgotten: bool, nonce: Option<u64> },
}

/// A daemon's sends through Logos Core: which chain, which keystore account.
#[derive(Clone, Debug)]
pub struct Route {
    pub call: ModuleCall,
    pub chain_id: u64,
    pub from: Address,
    /// Who asks, in the purpose line the signer shows ("Ferrata monitor: cancelCandidate, call 0x…").
    pub purpose: &'static str,
    /// The daemon holds its own key too: what `tx_sender_module` did not broadcast goes the plain way (for the
    /// monitor, also what the module cannot account for, [`Outcome::Unknown`]).
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

/// A call's tag in its request's purpose line (the first four bytes of the calldata's keccak-256): what tells this
/// call's request from another of the same label in `live_sends`, which carries no calldata.
pub fn call_tag(input: &Bytes) -> String {
    format!("0x{}", alloy::hex::encode(&alloy::primitives::keccak256(input)[..4]))
}

impl Route {
    /// The purpose line of a request for `label` with `input`.
    fn purpose_of(&self, label: &str, input: &Bytes) -> String {
        format!("{}: {label}, call {}", self.purpose, call_tag(input))
    }

    /// The request `tx_sender_module.send` takes for one call of `to` with `input` (no value; the gas estimated by
    /// `fee_module` as the chain will find it). `nonce` pins it onto that number: it then replaces a transaction that
    /// may have left with the number, and at most one of the two can be mined.
    pub fn request(&self, to: Address, input: &Bytes, label: &str, nonce: Option<u64>) -> Value {
        let mut r = json!({
            "chainId": self.chain_id,
            "from": self.from.to_checksum(None),
            "calls": [{ "to": to.to_checksum(None), "value": "0", "data": input.to_string(), "label": label }],
            "purpose": self.purpose_of(label, input),
        });
        if let Some(n) = nonce {
            r["nonce"] = json!(n);
        }
        r
    }

    /// `send`, then `send_status` until it is final. Blocking: run it off the async runtime.
    pub fn send(&self, to: Address, input: &Bytes, label: &str) -> Outcome {
        let t0 = Instant::now();
        match self.open(to, input, label, None) {
            Ok(id) => self.drive(&id, t0),
            Err(o) => o,
        }
    }

    /// `send` alone (with `nonce` pinned, if given): the request made (its nonce reserved) and its id; nothing is
    /// signed or broadcast before [`Route::drive`]. `Err`: nothing was reserved ([`Outcome::NotSent`]). Blocking.
    pub fn open(&self, to: Address, input: &Bytes, label: &str, nonce: Option<u64>) -> Result<String, Outcome> {
        let t0 = Instant::now();
        let request = self.request(to, input, label, nonce).to_string();
        let v = loop {
            let v = match (self.call)(TX_SENDER, "send", &json!([request]), CALL_TIMEOUT) {
                Ok(r) => parse(&r),
                Err(e) => return Err(Outcome::NotSent(format!("{TX_SENDER}.send did not answer: {e}"))),
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
            return Err(Outcome::NotSent(format!("{TX_SENDER}.send refused: {}", why(&v))));
        }
        v.get("requestId")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| Outcome::NotSent(format!("{TX_SENDER}.send answered no requestId: {v}")))
    }

    /// `send_status` until it is final, for a request [`Route::open`] made, here or before a restart: `send_status` is
    /// what broadcasts (`tx_sender_module` has no background advancer), so a request left open is driven to its end
    /// rather than made again. The approval timeout runs from `t0`. `NotSent` only on the module's own word for a
    /// request it holds; anything that may have left is [`Outcome::Unknown`]. Blocking.
    pub fn drive(&self, id: &str, t0: Instant) -> Outcome {
        let id = id.to_string();
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
            let ok = s.get("ok") == Some(&Value::Bool(true));
            let is_final = s.get("final") == Some(&Value::Bool(true));
            let status = s.get("status").and_then(Value::as_str).unwrap_or("");
            let legs = s.get("legs").and_then(Value::as_array).cloned().unwrap_or_default();
            let nonce = legs.first().and_then(|l| l.get("nonce")).and_then(Value::as_u64);
            let left = legs.iter().any(|l| l.get("left") == Some(&Value::Bool(true)));
            let unknown = |reason: String, forgotten: bool| Outcome::Unknown { reason, forgotten, nonce };
            if ok && status == "broadcast" {
                let hash =
                    s.get("hash").and_then(Value::as_str).and_then(|h| h.parse().ok()).or(hashes.first().copied());
                return match hash {
                    Some(hash) => Outcome::Broadcast { hash, request_id: id, polls, millis },
                    None => unknown(format!("{TX_SENDER} says broadcast without a hash: {s}"), false),
                };
            }
            if let Some(&hash) = hashes.first() {
                // A leg went out; whatever the bundle's status, it is waited for, never sent again.
                if is_final {
                    return Outcome::Broadcast { hash, request_id: id, polls, millis };
                }
            } else if is_final && !ok {
                // Final refusals come only for a request the module's ledger does not hold (logos-evm-tx-sender-module
                // send.rs `refusal_is_final`): it restarted and its sends went with its memory. Not "nothing sent".
                return unknown(format!("{TX_SENDER} holds no request {id} ({}): whether it left is unknown", why(&s)), true);
            } else if is_final {
                if status == "stuck" || left {
                    return unknown(format!("{TX_SENDER}: request {id} {status}, no hash ({})", why(&s)), false);
                }
                return Outcome::NotSent(format!("{TX_SENDER}: request {id} {status} ({})", why(&s)));
            }
            if t0.elapsed() > self.approval_timeout {
                if status == "awaitingApproval" || (!ok && hashes.is_empty()) {
                    // Withdrawn only if the module says so: a cancel it refuses or does not answer leaves it unknown.
                    let c = (self.call)(TX_SENDER, "cancel_send", &json!([id]), CALL_TIMEOUT).map(|r| parse(&r));
                    if c.as_ref().is_ok_and(|v| v.get("ok") == Some(&Value::Bool(true))) {
                        return Outcome::NotSent(format!(
                            "{TX_SENDER}: request {id} not approved within {:?} (withdrawn); is the account unlocked \
                             for {TX_SENDER} without confirmation (`custody`)?",
                            self.approval_timeout
                        ));
                    }
                    let c = c.map_or_else(|e| e, |v| why(&v));
                    return unknown(
                        format!("{TX_SENDER}: request {id} not settled within {:?} and not withdrawn ({c})", self.approval_timeout),
                        false,
                    );
                }
                return unknown(format!("{TX_SENDER}: request {id} still {status} after {:?}", t0.elapsed()), false);
            }
            std::thread::sleep(POLL);
        }
    }
}

impl Route {
    /// The requests `tx_sender_module` still holds open (`live_sends`) for this route's chain and account whose
    /// purpose is this route's for `label` and this very call (its tag, [`call_tag`]): what a daemon killed before it
    /// learnt a request's id finds again; another call's request of the same label is not taken for it. Blocking.
    pub fn live(&self, label: &str, input: &Bytes) -> Result<Vec<String>, String> {
        let v = parse(&(self.call)(TX_SENDER, "live_sends", &json!([]), CALL_TIMEOUT)?);
        if v.get("ok") != Some(&Value::Bool(true)) {
            return Err(format!("{TX_SENDER}.live_sends refused: {}", why(&v)));
        }
        let purpose = self.purpose_of(label, input);
        Ok(v.get("sends")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|s| s.get("chainId").and_then(Value::as_u64) == Some(self.chain_id))
            .filter(|s| s.get("from").and_then(Value::as_str).and_then(|f| f.parse::<Address>().ok()) == Some(self.from))
            .filter(|s| s.get("purpose").and_then(Value::as_str).is_some_and(|p| p.starts_with(&purpose)))
            .filter_map(|s| s.get("requestId").and_then(Value::as_str).map(str::to_string))
            .collect())
    }

    /// What `tx_sender_module`'s durable history (`history`, kept on disk across its restarts) holds of request `id`:
    /// the nonce its call was signed at and its hash (`None` while its broadcast never answered). The module writes
    /// that row before the call leaves ("write ahead", logos-evm-tx-sender-module glue.rs `advance_send`), so
    /// `Ok(None)`, no row, means the request was never broadcast. `Err`: the history did not answer (not open yet,
    /// locked, not reached), which says nothing either way. Blocking.
    pub fn history_row(&self, id: &str) -> Result<Option<(u64, Option<B256>)>, String> {
        let args = json!([self.from.to_checksum(None), self.chain_id]);
        let v = parse(&(self.call)(TX_SENDER, "history", &args, CALL_TIMEOUT)?);
        if v.get("ok") != Some(&Value::Bool(true)) {
            return Err(format!("{TX_SENDER}.history refused: {}", why(&v)));
        }
        let rows = v
            .get("transactions")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("{TX_SENDER}.history answered no transactions: {v}"))?;
        let row = rows.iter().rev().find(|r| {
            r.get("requestId").and_then(Value::as_str) == Some(id) && r.get("leg").and_then(Value::as_u64).unwrap_or(0) == 0
        });
        let Some(r) = row else { return Ok(None) };
        let nonce = r
            .get("nonce")
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("{TX_SENDER}.history: the row of request {id} has no nonce"))?;
        let hash = r.get("hash").and_then(Value::as_str).filter(|h| !h.is_empty()).and_then(|h| h.parse().ok());
        Ok(Some((nonce, hash)))
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
    use std::sync::{Mutex, MutexGuard, PoisonError};

    // One fake Logos Core per test binary run: the calls it saw and the replies it gives, by method.
    static SEEN: Mutex<Vec<(String, String, Value)>> = Mutex::new(Vec::new());
    static SCRIPT: Mutex<Vec<(&'static str, String)>> = Mutex::new(Vec::new());
    static SERIAL: Mutex<()> = Mutex::new(());

    // A failed test does not poison the mutexes for the others.
    fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
        m.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn fake(module: &str, method: &str, args: &Value, _t: Duration) -> Result<String, String> {
        lock(&SEEN).push((module.into(), method.into(), args.clone()));
        let mut s = lock(&SCRIPT);
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
        lock(&SEEN).clear();
        *lock(&SCRIPT) = replies.iter().map(|(m, v)| (*m, v.to_string())).collect();
    }

    const H: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";

    #[test]
    fn a_broadcast_send_answers_its_hash_and_the_request_carries_the_calldata() {
        let _g = lock(&SERIAL);
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
        let seen = lock(&SEEN);
        assert_eq!(seen[0].0, TX_SENDER);
        let req: Value = serde_json::from_str(seen[0].2[0].as_str().unwrap()).unwrap();
        assert_eq!(req["calls"][0]["data"], "0xabcd");
        assert_eq!(req["calls"][0]["to"], "0x5FbDB2315678afecb367f032d93F642f64180aa3");
        assert_eq!(req["from"], "0x70997970C51812dc3A010C7d01b50e0d17dc79C8");
        assert_eq!(req["chainId"], 31337);
        assert_eq!(req["purpose"], format!("Ferrata monitor: cancelCandidate, call {}", call_tag(&input)));
        assert!(req.get("nonce").is_none(), "not pinned");
        assert_eq!(seen[1].2, json!(["r1"]));
        drop(seen);
        // Pinned: the request carries the nonce.
        assert_eq!(route().request(to, &input, "cancelCandidate", Some(44))["nonce"], 44);
    }

    #[test]
    fn a_refused_or_unapproved_send_is_not_sent_and_an_unapproved_one_is_withdrawn() {
        let _g = lock(&SERIAL);
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
        assert!(lock(&SEEN).iter().any(|(_, m, a)| m == "cancel_send" && *a == json!(["r2"])));
    }

    #[test]
    fn a_send_refused_while_the_history_opens_is_asked_again() {
        let _g = lock(&SERIAL);
        script(&[
            ("send", json!({ "ok": false, "code": "unavailable", "error": "This account's history isn't open yet" })),
            ("send", json!({ "ok": true, "requestId": "r5" })),
            ("send_status", json!({ "ok": true, "status": "broadcast", "final": true, "hashes": [H], "hash": H })),
        ]);
        assert!(matches!(route().send(Address::ZERO, &Bytes::new(), "x"), Outcome::Broadcast { polls: 1, .. }));
    }

    #[test]
    fn a_stuck_send_is_never_reported_as_not_sent() {
        let _g = lock(&SERIAL);
        script(&[
            ("send", json!({ "ok": true, "requestId": "r3" })),
            ("send_status", json!({ "ok": true, "status": "stuck", "final": true, "hashes": [] })),
        ]);
        assert!(matches!(route().send(Address::ZERO, &Bytes::new(), "x"), Outcome::Unknown { forgotten: false, .. }));
        script(&[
            ("send", json!({ "ok": true, "requestId": "r4" })),
            ("send_status", json!({ "ok": true, "status": "failed", "final": true, "hashes": [H] })),
        ]);
        assert!(matches!(route().send(Address::ZERO, &Bytes::new(), "x"), Outcome::Broadcast { .. }));
        // Failed after its leg began to leave, with no hash: it may be out; its nonce is the module's.
        script(&[
            ("send", json!({ "ok": true, "requestId": "r9" })),
            ("send_status", json!({ "ok": true, "status": "failed", "final": true, "hashes": [],
                                    "legs": [{ "nonce": 44, "left": true }], "reason": "no hash" })),
        ]);
        let o = route().send(Address::ZERO, &Bytes::new(), "x");
        assert!(matches!(o, Outcome::Unknown { forgotten: false, nonce: Some(44), .. }), "{o:?}");
    }

    /// The second review's M2: tx_sender_module restarted (its sends are in memory) answers a final refusal for a
    /// request it no longer holds: unknown, never "not sent". A withdrawal it does not confirm is unknown too.
    #[test]
    fn a_request_the_module_no_longer_holds_or_did_not_withdraw_is_unknown() {
        let _g = lock(&SERIAL);
        script(&[(
            "send_status",
            json!({ "ok": false, "error": "no send with id 'snd_7'", "final": true }),
        )]);
        let o = route().drive("snd_7", Instant::now());
        assert!(matches!(&o, Outcome::Unknown { forgotten: true, nonce: None, reason } if reason.contains("holds no request")), "{o:?}");
        let waiting = json!({ "ok": true, "status": "awaitingApproval", "final": false, "hashes": [],
                              "legs": [{ "nonce": 45, "left": false }] });
        let mut replies: Vec<(&'static str, Value)> = std::iter::repeat_n(("send_status", waiting), 10).collect();
        replies.push(("cancel_send", json!({ "ok": false, "error": "snd_8 is broadcasting" })));
        script(&replies);
        let o = route().drive("snd_8", Instant::now());
        assert!(matches!(&o, Outcome::Unknown { forgotten: false, nonce: Some(45), reason } if reason.contains("not withdrawn")), "{o:?}");
    }

    #[test]
    fn the_history_row_of_a_request_gives_its_nonce_and_hash_and_no_row_means_never_broadcast() {
        let _g = lock(&SERIAL);
        script(&[
            ("history", json!({ "ok": true, "transactions": [
                { "requestId": "snd_1", "leg": 0, "nonce": 40, "hash": H, "status": "confirmed" },
                { "requestId": "snd_2", "leg": 0, "nonce": 41, "hash": "", "status": "unknown" },
            ] })),
            ("history", json!({ "ok": true, "transactions": [] })),
            ("history", json!({ "ok": false, "code": "unavailable", "error": "This account's history isn't open yet" })),
            ("history", json!({ "ok": true, "transactions": [{ "requestId": "snd_2", "leg": 0, "nonce": 41, "hash": "" }] })),
        ]);
        assert_eq!(route().history_row("snd_1"), Ok(Some((40, Some(H.parse().unwrap())))));
        assert_eq!(route().history_row("snd_9"), Ok(None));
        assert!(route().history_row("snd_2").unwrap_err().contains("unavailable"));
        assert_eq!(route().history_row("snd_2"), Ok(Some((41, None))), "an intent row whose broadcast never answered");
        let seen = lock(&SEEN);
        assert_eq!(seen[0].2, json!(["0x70997970C51812dc3A010C7d01b50e0d17dc79C8", 31337]));
    }

    #[test]
    fn a_request_left_open_is_found_and_driven_to_its_end() {
        let _g = lock(&SERIAL);
        let from = "0x70997970c51812dc3a010c7d01b50e0d17dc79c8";
        let (mine, other): (Bytes, Bytes) = (vec![1, 2].into(), vec![3, 4].into());
        let p = |input: &Bytes| format!("Ferrata monitor: cancelCandidate, call {}", call_tag(input));
        script(&[
            ("send", json!({ "ok": true, "pending": true, "requestId": "r7" })),
            ("live_sends", json!({ "ok": true, "sends": [
                { "requestId": "r6", "chainId": 31337, "from": from, "status": "awaitingApproval", "purpose": "Ferrata monitor: other" },
                { "requestId": "r7", "chainId": 31337, "from": from, "status": "awaitingApproval", "purpose": format!("{} (ferrata_monitor_module)", p(&mine)) },
                { "requestId": "r8", "chainId": 1, "from": from, "status": "awaitingApproval", "purpose": p(&mine) },
                // Another call of the same label (the second review's L7): not taken for this one.
                { "requestId": "r9", "chainId": 31337, "from": from, "status": "awaitingApproval", "purpose": p(&other) },
            ] })),
            ("send_status", json!({ "ok": true, "status": "broadcast", "final": true, "hashes": [H], "hash": H })),
        ]);
        let id = route().open(Address::ZERO, &mine, "cancelCandidate", None).unwrap();
        assert_eq!(route().live("cancelCandidate", &mine).unwrap(), vec![id.clone()]);
        assert!(matches!(route().drive(&id, Instant::now()), Outcome::Broadcast { request_id, .. } if request_id == "r7"));
        assert_eq!(lock(&SEEN).iter().filter(|(_, m, _)| m == "send").count(), 1, "one request, driven");
    }

    #[test]
    fn custody_imports_waits_for_the_manager_and_unlocks_for_tx_sender_without_confirmation() {
        let _g = lock(&SERIAL);
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
        let seen = lock(&SEEN);
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
