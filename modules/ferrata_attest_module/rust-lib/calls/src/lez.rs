//! LEZ through Logos Core's own LEZ module, `lez_core` (logos-execution-zone-module: LEZ v0.4.0-rc1's wallet as a
//! module, at its merged v0.4 state with `modules/patches/lez_core-cc73ebf-authorizes-public-pda.patch`), called by
//! name (RFP-021 U.1: the module's LEZ reads and sends through Logos Core's modules, not around them).
//!
//! * `lez({"via": "lez_core", "wallet": {"config", "storage", "password_file"}})` opens a LEZ wallet inside `lez_core`
//!   (`open(config, storage, password, force_rpc = true)`: a client of the zone's sequencer its config names) and lists
//!   its public accounts, one of which pays the fees; `{"via": "plain"}` closes the path.
//! * `lez_read({"program", "instance_id", "kind": "instance" | "record" | "job", "slot", "job_id"})`: the attestation
//!   program's actor state at `instance(id)`, `header(id, slot)` or `job(id, job_id)`, read with
//!   `lez_core.get_account_public` (the sequencer's `getAccount`, by LEZ's wallet library inside Logos Core), and
//!   decoded (an instance as `read_instance` decodes it).
//! * `lez_submit({"program", "payer", "steps": [...]})`: each step (as `advance` returns them, or as `build_attest_tx`
//!   returns its transaction: a root actor, the accounts it names, the message) sent with
//!   `lez_core.send_generic_public_transaction` (every account named without signing, each seeing the attestation
//!   program's actor state; the payer, a public account of the opened wallet, pays and co-signs), then
//!   `lez_core.poll_transaction_status` until the transaction is in a block. A step is sent only once the one before it
//!   is in a block. The program's verdict on each (applied, or the exit code) is the zone's: `lez_core` reports none
//!   (the sequencer's RPC has no receipts), so a caller reads the state back with `lez_read`, or follows the chain
//!   (`ferrata attest`'s follower).

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use ferrata_lez_wire::{header_actor, instance_actor, job_actor, JobState, RecordState};
use lee_core::account::AccountId;
use serde_json::{json, Value};

use crate::{hex, unhex};

/// How the module crate reaches `lez_core`: (method, the arguments as a JSON array text, a timeout in milliseconds) ->
/// the method's reply as text, or why the call did not reach it.
pub type LezCoreCall = fn(&str, &str, u64) -> Result<String, String>;

static LEZ_CORE: OnceLock<LezCoreCall> = OnceLock::new();

/// Installed once by the module crate (inside Logos Core); natively nothing is installed and the path answers with an
/// error, unless a test installs its own.
pub fn set_lez_core(f: LezCoreCall) {
    let _ = LEZ_CORE.set(f);
}

const READ_MS: u64 = 60_000;
const SEND_MS: u64 = 300_000;

#[derive(Default)]
struct Session {
    open: bool,
    calls: Vec<Value>,
}

static SESSION: Mutex<Option<Session>> = Mutex::new(None);

fn call(method: &str, args: &Value, timeout_ms: u64) -> Result<(String, u64), String> {
    let f = LEZ_CORE.get().ok_or("lez_core is reachable only inside Logos Core (the module crate installs the call)")?;
    let t0 = Instant::now();
    let reply = f(method, &args.to_string(), timeout_ms).map_err(|e| format!("lez_core.{method}: {e}"))?;
    let ms = t0.elapsed().as_millis() as u64;
    if let Ok(mut g) = SESSION.lock() {
        if let Some(s) = g.as_mut() {
            s.calls.push(json!({ "method": method, "ms": ms }));
        }
    }
    Ok((reply, ms))
}

/// `0x` + lower-case hex.
fn norm(h: &str) -> String {
    format!("0x{}", h.trim().trim_start_matches("0x").trim_start_matches("0X").to_lowercase())
}

/// Logos Core's bytes argument (`{"_bytes": base64url, unpadded}`), which a C++ module receives as `QByteArray`.
fn bytes_arg(b: &[u8]) -> Value {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut s = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (u32::from(c[0]) << 16) | (u32::from(*c.get(1).unwrap_or(&0)) << 8) | u32::from(*c.get(2).unwrap_or(&0));
        s.push(A[(n >> 18) as usize & 63] as char);
        s.push(A[(n >> 12) as usize & 63] as char);
        if c.len() > 1 {
            s.push(A[(n >> 6) as usize & 63] as char);
        }
        if c.len() > 2 {
            s.push(A[n as usize & 63] as char);
        }
    }
    json!({ "_bytes": s })
}

fn root32(v: &Value, what: &str) -> Result<[u8; 32], String> {
    let s = v.as_str().ok_or_else(|| format!("{what}: a 32-byte hex string"))?;
    unhex(s)?.try_into().map_err(|_| format!("{what}: not 32 bytes"))
}

pub(crate) fn lez(request: &Value) -> Result<Value, String> {
    match request["via"].as_str() {
        Some("plain") | None => {
            let had = SESSION.lock().map_err(|_| "poisoned")?.take().map(|s| s.calls.len());
            return Ok(json!({ "verdict": "closed", "via": "plain", "lez_core_calls_before": had }));
        }
        Some("lez_core") => {}
        Some(other) => return Err(format!("via {other:?}: \"lez_core\" or \"plain\"")),
    }
    let w = &request["wallet"];
    let config = w["config"].as_str().ok_or("wallet.config: the LEZ wallet's wallet_config.json")?;
    let storage = w["storage"].as_str().ok_or("wallet.storage: the LEZ wallet's storage.json")?;
    let password_file = w["password_file"].as_str().ok_or("wallet.password_file")?;
    let password = std::fs::read_to_string(password_file).map_err(|e| format!("{password_file}: {e}"))?;
    let password = password.trim_end_matches(['\n', '\r']).to_string();
    *SESSION.lock().map_err(|_| "poisoned")? = Some(Session::default());
    let (opened, _) = call("open", &json!([config, storage, password, true]), READ_MS)?;
    // Anything but 0 is a refusal, including "a wallet is already open" (lez_core keeps one per process): going on
    // would read and sign with whichever wallet that is, not the one asked for.
    if opened.trim() != "0" {
        *SESSION.lock().map_err(|_| "poisoned")? = None;
        return Err(format!(
            "lez_core did not open the wallet (open answered {}: a wallet already open in this Logos Core, or the wallet refused)",
            opened.trim()
        ));
    }
    let (addr, _) = call("get_sequencer_addr", &json!([]), READ_MS)?;
    let (height, _) = call("get_current_block_height", &json!([]), READ_MS)?;
    let (accounts, _) = call("list_accounts", &json!([]), READ_MS)?;
    let list: Value = serde_json::from_str(&accounts).unwrap_or(Value::Null);
    let public: Vec<String> = list
        .as_array()
        .into_iter()
        .flatten()
        .filter(|a| a["is_public"] == true)
        .filter_map(|a| a["account_id"].as_str().map(norm))
        .collect();
    if let Some(s) = SESSION.lock().map_err(|_| "poisoned")?.as_mut() {
        s.open = true;
    }
    Ok(json!({ "verdict": "opened", "via": "lez_core", "sequencer": addr, "head": height.trim(), "public_accounts": public,
               "open_reply": opened.trim() }))
}

fn active() -> Result<(), String> {
    match SESSION.lock().map_err(|_| "poisoned")?.as_ref() {
        Some(s) if s.open => Ok(()),
        _ => Err("call lez with {\"via\": \"lez_core\", \"wallet\": ...} first".into()),
    }
}

/// `program`'s actor state of `account` as `lez_core.get_account_public` answers it.
fn actor_state(account: &AccountId, program: &AccountId) -> Result<(Vec<u8>, u64, usize), String> {
    let (reply, ms) = call("get_account_public", &json!([hex(account.value())]), READ_MS)?;
    if reply.trim().is_empty() {
        return Ok((Vec::new(), ms, 0));
    }
    let v: Value = serde_json::from_str(&reply).map_err(|e| format!("get_account_public: not JSON ({e}): {reply}"))?;
    let states = v["actor_states"].as_array().ok_or_else(|| format!("get_account_public: no actor_states: {reply}"))?;
    let p = norm(&hex(program.value()));
    let data = states.iter().find(|s| s["program"].as_str().map(norm) == Some(p.clone())).and_then(|s| s["data"].as_str());
    Ok((data.map(unhex).transpose()?.unwrap_or_default(), ms, states.len()))
}

pub(crate) fn lez_read(request: &Value) -> Result<Value, String> {
    active()?;
    let program = AccountId::new(root32(&request["program"], "program")?);
    let id = root32(&request["instance_id"], "instance_id")?;
    let kind = request["kind"].as_str().unwrap_or("instance");
    let actor = match kind {
        "instance" => instance_actor(program, &id),
        "record" => header_actor(program, &id, request["slot"].as_u64().ok_or("slot: the record's slot")?),
        "job" => job_actor(program, &id, &root32(&request["job_id"], "job_id")?),
        other => return Err(format!("kind {other:?}: instance, record or job")),
    };
    let (bytes, ms, states) = actor_state(&actor.account_id, &program)?;
    let mut out = json!({
        "verdict": if bytes.is_empty() { "absent" } else { "read" },
        "via": "lez_core.get_account_public",
        "kind": kind,
        "account": hex(actor.account_id.value()),
        "state": hex(&bytes),
        "bytes": bytes.len(),
        "actor_states": states,
        "ms": ms,
    });
    if bytes.is_empty() {
        return Ok(out);
    }
    match kind {
        "instance" => {
            let decoded: Value = serde_json::from_str(&crate::read_instance(&json!({ "state": hex(&bytes) }).to_string()))
                .unwrap_or(Value::Null);
            out["decoded"] = decoded;
        }
        "record" => {
            let r: RecordState = borsh::from_slice(&bytes).map_err(|e| format!("record: {e}"))?;
            out["decoded"] = json!({ "instance_id": hex(&r.instance_id), "chain_id": r.chain_id,
                "header": r.header().map(|h| format!("{:?}", h.execution)).unwrap_or_else(|e| format!("{e:?}")) });
            // As `dry_run_attest` takes a record read from LEZ: `{chain_id, encoded}`.
            out["record"] = json!({ "chain_id": r.chain_id, "encoded": hex(&r.record) });
        }
        _ => {
            let j: JobState = borsh::from_slice(&bytes).map_err(|e| format!("job: {e}"))?;
            out["decoded"] = json!({ "instance_id": hex(&j.instance_id), "job_id": hex(&j.job_id), "job_bytes": j.job.len() });
        }
    }
    Ok(out)
}

/// The accounts a step names, with the index of its root: `accounts` (strings or `{account}`) and `root` (a string,
/// `{account}`, or absent: the first account), or `build_attest_tx`'s `account` and `accounts`.
fn step_accounts(step: &Value) -> Result<(Vec<String>, u64), String> {
    let acc = |v: &Value| v.as_str().map(norm).or_else(|| v["account"].as_str().map(norm));
    let list: Vec<String> = step["accounts"]
        .as_array()
        .ok_or("step.accounts: the accounts it names")?
        .iter()
        .map(|v| acc(v).ok_or("step.accounts: hex strings or {account}"))
        .collect::<Result<_, _>>()?;
    let root = acc(&step["root"]).or_else(|| step["account"].as_str().map(norm)).unwrap_or_else(|| list[0].clone());
    let index = list.iter().position(|a| *a == root).ok_or("step: its root is not among its accounts")?;
    Ok((list, index as u64))
}

pub(crate) fn lez_submit(request: &Value) -> Result<Value, String> {
    active()?;
    let program = norm(request["program"].as_str().ok_or("program: the attestation program's account")?);
    let payer = norm(request["payer"].as_str().ok_or("payer: a public account of the opened wallet")?);
    let poll_s = request["poll_timeout_s"].as_u64().unwrap_or(600);
    let steps = request["steps"].as_array().ok_or("steps: the transactions, in order")?;
    let mut out = Vec::new();
    for (i, step) in steps.iter().enumerate() {
        let name = step["name"].as_str().map(str::to_string).unwrap_or_else(|| format!("step {i}"));
        let (accounts, root) = step_accounts(step)?;
        let message = unhex(step["message"].as_str().ok_or("step.message: hex")?)?;
        let n = accounts.len();
        let args = json!([accounts, root, vec![false; n], bytes_arg(&message), payer, vec![program.clone(); n]]);
        let (reply, ms) = call("send_generic_public_transaction", &args, SEND_MS)?;
        let v: Value = serde_json::from_str(&reply).unwrap_or(json!({ "raw": reply }));
        let hash = v["tx_hash"].as_str().map(norm);
        let mut entry = json!({ "name": name, "accounts": n, "root": root, "message_bytes": message.len(),
                                "send_ms": ms, "reply": v, "tx_hash": hash });
        let Some(hash) = hash.filter(|_| v["success"].as_bool() != Some(false)) else {
            entry["verdict"] = json!("not_sent");
            out.push(entry);
            return Ok(json!({ "verdict": "stopped", "via": "lez_core.send_generic_public_transaction", "steps": out }));
        };
        let t0 = Instant::now();
        let mut found = false;
        while t0.elapsed() < Duration::from_secs(poll_s) {
            let (r, _) = call("poll_transaction_status", &json!([hash.trim_start_matches("0x")]), READ_MS)?;
            if r.trim() == "true" {
                found = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(1000));
        }
        entry["in_block"] = json!(found);
        entry["poll_s"] = json!(t0.elapsed().as_secs_f64());
        entry["verdict"] = json!(if found { "in_block" } else { "not_seen" });
        out.push(entry);
        if !found {
            return Ok(json!({ "verdict": "stopped", "via": "lez_core.send_generic_public_transaction", "steps": out }));
        }
    }
    Ok(json!({ "verdict": "sent", "via": "lez_core.send_generic_public_transaction", "steps": out }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI64, Ordering};

    /// What the fake `lez_core.open` answers (0: opened).
    static OPEN_REPLY: AtomicI64 = AtomicI64::new(0);

    fn fake(method: &str, args: &str, _t: u64) -> Result<String, String> {
        let a: Value = serde_json::from_str(args).unwrap();
        Ok(match method {
            "open" => OPEN_REPLY.load(Ordering::SeqCst).to_string(),
            "get_sequencer_addr" => "http://127.0.0.1:3040".into(),
            "get_current_block_height" => "12".into(),
            "list_accounts" => json!([{ "account_id": "aa".repeat(32), "is_public": true }]).to_string(),
            "send_generic_public_transaction" => {
                // The arguments as lez_core takes them: accounts, root mention, signing flags, bytes, payer, programs.
                assert_eq!(a[2], json!([false, false]));
                assert!(a[3]["_bytes"].is_string());
                assert_eq!(a[5].as_array().unwrap().len(), 2);
                json!({ "success": true, "tx_hash": "ab".repeat(32) }).to_string()
            }
            "poll_transaction_status" => "true".into(),
            "get_account_public" => json!({ "actor_states": [] }).to_string(),
            other => panic!("{other}"),
        })
    }

    #[test]
    fn the_path_through_lez_core_shapes_its_calls() {
        set_lez_core(fake);
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target");
        std::fs::create_dir_all(&dir).unwrap();
        let pw = dir.join("ferrata-lez-test.pass");
        std::fs::write(&pw, "x\n").unwrap();
        let o = lez(&json!({ "via": "lez_core", "wallet": { "config": "c", "storage": "s", "password_file": pw } })).unwrap();
        assert_eq!(o["verdict"], "opened");
        let program = format!("0x{}", "01".repeat(32));
        let step = json!({ "name": "open", "root": { "account": "0x".to_string() + &"02".repeat(32) },
                           "accounts": [ { "account": "0x".to_string() + &"02".repeat(32) }, { "account": "0x".to_string() + &"03".repeat(32) } ],
                           "message": "0x0102" });
        let s = lez_submit(&json!({ "program": program, "payer": "aa".repeat(32), "steps": [step] })).unwrap();
        assert_eq!(s["verdict"], "sent");
        let r = lez_read(&json!({ "program": program, "instance_id": "0x".to_string() + &"04".repeat(32), "kind": "instance" })).unwrap();
        assert_eq!(r["verdict"], "absent");
        // `open` refused while the chain has blocks (lez_core's "a wallet is already open" answers 99,
        // FfiOperationError::InternalError in LEZ's core_ffi): an error, and the path closed, not "opened" on another wallet.
        OPEN_REPLY.store(99, Ordering::SeqCst);
        let e = lez(&json!({ "via": "lez_core", "wallet": { "config": "c", "storage": "s", "password_file": pw } })).unwrap_err();
        OPEN_REPLY.store(0, Ordering::SeqCst);
        assert!(e.contains("did not open the wallet (open answered 99"), "{e}");
        assert!(lez_submit(&json!({ "program": program, "payer": "aa".repeat(32), "steps": [step] })).is_err());
    }
}
