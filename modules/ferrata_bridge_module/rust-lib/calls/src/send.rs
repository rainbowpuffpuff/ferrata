//! The module's Ethereum transactions through Logos Core's send stack: `tx_sender_module` (the one EVM sender on the
//! device), which prices a bundle through `fee_module`, reserves its nonces, asks `signer_manager_module` for ONE human
//! approval over every call of the bundle and, once approved, has the account's signer (`keystore_module`) sign and
//! broadcasts through `eth_rpc_module` as `send_status` is polled. The module never holds an Ethereum key and never
//! signs: it builds the calls (the vault's `depositETH`/`deposit`, an ERC-20 `approve`, `release`, the admin's and the
//! freeze authority's calls) and hands them over. The person approves in Logos's own approval surface: the Signer
//! app (`evm_signer_ui`, raised in Basecamp with the intent `evm.signing.approve {handle}`) or, headless, the
//! approver `evm_signer_cli` (`approve handle bundle_id @password-file`).
//!
//! Contract (logos-co/logos-evm-tx-sender-module f1754c3, README "The contract", `rust-lib/src/glue.rs` 45-137):
//! `send(request_json)` -> `{ok, pending: true, requestId, handle}`, never a hash; `send_status(request_id)` ->
//! `{ok, requestId, handle, status, final, legs, hashes, hash?, reason?}`, `status` one of `awaitingApproval`,
//! `broadcasting`, `stuck`, `broadcast`, `rejected`, `cancelled`, `failed`; poll until `final`. `unavailable` from
//! `send` while the account's history opens: nothing was reserved or signed, ask again. Values are decimal wei.
//!
//! Inside Logos Core the module crate installs the call by name ([`set_module_call`]); natively nothing is, and every
//! send here is refused (the tests install a stand-in of the contract above).

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::hex;

/// One call of `method` of the Logos module `module` with the JSON array `args` (the SDK's `call_json_with_timeout`,
/// by name); the reply's text, or why there is none.
pub type ModuleCall = fn(module: &str, method: &str, args: &Value, timeout_ms: u64) -> Result<String, String>;

static CALL: OnceLock<ModuleCall> = OnceLock::new();

/// Installed once by the module crate (inside Logos Core).
pub fn set_module_call(f: ModuleCall) {
    let _ = CALL.set(f);
}

pub(crate) const TX_SENDER: &str = "tx_sender_module";
const CALL_TIMEOUT_MS: u64 = 60_000;
const POLL: Duration = Duration::from_millis(500);

/// One call of a bundle: `to`, `value` in wei (decimal), `data`, and the line the person reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Call {
    pub to: [u8; 20],
    pub value: String,
    pub data: Vec<u8>,
    pub label: String,
}

impl Call {
    pub(crate) fn to_json(&self) -> Value {
        json!({ "to": crate::evm::checksum(&self.to), "value": self.value, "data": hex(&self.data), "label": self.label })
    }
}

/// The request `tx_sender_module.send` takes: the chain, the account, the calls in order, the purpose (one line).
pub(crate) fn request(chain_id: u64, from: &str, calls: &[Call], purpose: &str) -> Value {
    // The account as EIP-55 text when it parses (tx_sender_module keys its per-account history by the text it prints).
    let from = crate::unhex(from)
        .ok()
        .and_then(|b| <[u8; 20]>::try_from(b).ok())
        .map_or_else(|| from.to_string(), |a| crate::evm::checksum(&a));
    json!({
        "chainId": chain_id,
        "from": from,
        "calls": calls.iter().map(Call::to_json).collect::<Vec<_>>(),
        "purpose": purpose,
    })
}

fn module_call(method: &str, args: Value) -> Result<Value, String> {
    let f = CALL.get().ok_or_else(|| format!("{TX_SENDER}.{method}: Logos Core's send stack is reachable only from inside Logos Core"))?;
    let text = f(TX_SENDER, method, &args, CALL_TIMEOUT_MS)?;
    Ok(serde_json::from_str(&text).unwrap_or_else(|_| json!({ "ok": false, "error": text })))
}

fn why(v: &Value) -> String {
    let code = v["code"].as_str().map(|c| format!("{c}: ")).unwrap_or_default();
    let error = v.get("error").or_else(|| v.get("reason")).map(|e| e.as_str().map_or(e.to_string(), str::to_string));
    format!("{code}{}", error.unwrap_or_else(|| v.to_string()))
}

/// `tx_sender_module.send`: the request id and the signer manager's handle (`{requestId, handle, request}`).
/// Asked again while the account's history opens (`unavailable`), for at most `open_timeout`.
pub(crate) fn send(req: &Value, open_timeout: Duration) -> Result<Value, String> {
    let t0 = Instant::now();
    let text = req.to_string();
    loop {
        let v = module_call("send", json!([text]))?;
        if v["ok"] == true {
            let id = v["requestId"].as_str().ok_or("tx_sender_module.send answered no requestId")?;
            return Ok(json!({ "requestId": id, "handle": v["handle"], "pending": v["pending"], "request": req,
                              "asked_ms": t0.elapsed().as_millis() as u64 }));
        }
        if v["code"] != "unavailable" || t0.elapsed() > open_timeout {
            return Err(format!("{TX_SENDER}.send refused: {}", why(&v)));
        }
        std::thread::sleep(POLL);
    }
}

/// `tx_sender_module.send_status(request_id)`, as it answers.
pub(crate) fn status(request_id: &str) -> Result<Value, String> {
    module_call("send_status", json!([request_id]))
}

/// `send_status` polled until the send is final (`broadcast`, `rejected`, `cancelled`, `failed`, `stuck`) or
/// `timeout` passes; each status seen is passed to `seen` (the job's phases). The final reply.
pub(crate) fn wait(request_id: &str, timeout: Duration, mut seen: impl FnMut(&Value)) -> Result<Value, String> {
    let t0 = Instant::now();
    let mut last = String::new();
    loop {
        let s = status(request_id)?;
        let state = s["status"].as_str().unwrap_or("").to_string();
        if state != last {
            seen(&s);
            last = state;
        }
        if s["final"] == true {
            return Ok(s);
        }
        if t0.elapsed() > timeout {
            return Err(format!("{TX_SENDER}: request {request_id} not final after {} s (last: {})", timeout.as_secs(), why(&s)));
        }
        std::thread::sleep(POLL);
    }
}

/// `tx_sender_module.cancel_send(request_id)`.
pub(crate) fn cancel(request_id: &str) -> Result<Value, String> {
    module_call("cancel_send", json!([request_id]))
}

/// The hashes of a final `send_status` that broadcast, in the order of the calls; the refusal otherwise.
pub(crate) fn broadcast_hashes(s: &Value) -> Result<Vec<String>, String> {
    if s["ok"] == true && s["status"] == "broadcast" {
        let mut hashes: Vec<String> = s["hashes"].as_array().into_iter().flatten().filter_map(|h| h.as_str().map(str::to_string)).collect();
        if hashes.is_empty() {
            if let Some(h) = s["hash"].as_str() {
                hashes.push(h.to_string());
            }
        }
        if hashes.is_empty() {
            return Err("tx_sender_module reports `broadcast` without a hash".into());
        }
        return Ok(hashes);
    }
    let state = s["status"].as_str().unwrap_or("refused");
    Err(match state {
        "rejected" => "rejected: the person declined the approval in the Logos signer; nothing was signed or sent".to_string(),
        "cancelled" => "cancelled: the send was withdrawn before it was signed".to_string(),
        other => format!("{other}: {}", why(s)),
    })
}

/// Installs a stand-in of `tx_sender_module` for the tests: it approves (or rejects) every send as soon as it is
/// asked and answers the hashes it is given.
#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use std::sync::Mutex;

    pub(crate) static SENT: Mutex<Vec<Value>> = Mutex::new(Vec::new());
    pub(crate) static VERDICT: Mutex<&'static str> = Mutex::new("broadcast");

    fn call(module: &str, method: &str, args: &Value, _t: u64) -> Result<String, String> {
        assert_eq!(module, TX_SENDER);
        match method {
            "send" => {
                let req: Value = serde_json::from_str(args[0].as_str().unwrap()).unwrap();
                let mut sent = SENT.lock().unwrap();
                sent.push(req);
                Ok(json!({ "ok": true, "pending": true, "requestId": format!("req-{}", sent.len()), "handle": format!("smh_{}", sent.len()) }).to_string())
            }
            "send_status" => {
                let verdict = *VERDICT.lock().unwrap();
                let n = SENT.lock().unwrap().len();
                let hashes: Vec<String> = (0..SENT.lock().unwrap()[n - 1]["calls"].as_array().unwrap().len())
                    .map(|k| format!("0x{:064x}", 0xabc0 + k))
                    .collect();
                Ok(json!({ "ok": true, "requestId": args[0], "status": verdict, "final": true, "hashes": if verdict == "broadcast" { json!(hashes) } else { json!([]) } }).to_string())
            }
            other => Err(format!("unexpected {other}")),
        }
    }

    pub(crate) fn install() {
        set_module_call(call);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bundle_is_sent_and_its_hashes_collected() {
        fake::install();
        *fake::VERDICT.lock().unwrap() = "broadcast";
        let calls = vec![
            Call { to: [1; 20], value: "0".into(), data: vec![0x09, 0x5e, 0xa7, 0xb3], label: "approve".into() },
            Call { to: [2; 20], value: "0".into(), data: vec![0xde, 0xad], label: "deposit".into() },
        ];
        let req = request(11155111, "0x0000000000000000000000000000000000000003", &calls, "Ferrata deposit");
        let sent = send(&req, Duration::from_secs(1)).unwrap();
        assert!(sent["requestId"].as_str().unwrap().starts_with("req-"));
        assert!(sent["handle"].as_str().unwrap().starts_with("smh_"));
        let last = fake::SENT.lock().unwrap().last().cloned().unwrap();
        assert_eq!(last["chainId"], 11155111);
        assert_eq!(last["from"], "0x0000000000000000000000000000000000000003");
        assert_eq!(last["calls"][1]["data"], "0xdead");
        let s = wait(sent["requestId"].as_str().unwrap(), Duration::from_secs(1), |_| {}).unwrap();
        assert_eq!(broadcast_hashes(&s).unwrap().len(), 2);
        *fake::VERDICT.lock().unwrap() = "rejected";
        let s = wait(sent["requestId"].as_str().unwrap(), Duration::from_secs(1), |_| {}).unwrap();
        assert!(broadcast_hashes(&s).unwrap_err().starts_with("rejected"));
        *fake::VERDICT.lock().unwrap() = "broadcast";
    }
}
