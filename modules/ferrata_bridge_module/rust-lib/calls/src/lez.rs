//! LEZ for the module's wallet session through Logos Core's own LEZ module, `lez_core`
//! (logos-blockchain/logos-execution-zone-module, LEZ v0.4.0-rc1's wallet as a module), called by name.
//!
//! The wallet method `lez` chooses the path: `{"via": "lez_core", "wallet": {"config", "storage",
//! "password_file"}, "program_bin"}` opens the user's LEZ wallet inside `lez_core` (`open(config, storage,
//! password, force_rpc = true)`: a client of the zone's sequencer, the same wallet storage the chain service
//! holds) and from then on:
//!
//! * **reads**: every bridge actor state the session reads from the chain service (`config(cid)`,
//!   `token(cid, ETH)`, `root(cid, n)`, a deposit's `nullifier(...)`, `holder(account)`) is read again through
//!   `lez_core.get_account_public(account)` (the sequencer's `getAccount`, by LEZ's wallet library inside
//!   Logos Core) and compared byte for byte; `lez_core`'s value is used, the chain service's kept beside it;
//! * **the private holding**: `lez_core.sync_to_block(head)` then `get_account_private(account)` (LEZ's
//!   wallet decrypts it), beside the chain service's own wallet's view;
//! * **private sends** (the private claim and the private burn): `lez_core.
//!   send_generic_private_transaction_with_accounts` with the module's message, its accounts (the private
//!   account as `private_owned`, the bridge program's PDAs as `public_pda`) and the bridge program's binary,
//!   which LEZ's wallet executes and proves inside Logos Core; the chain service then confirms the block that
//!   applied it (`module.tx`, the plain path's view) beside `lez_core.poll_transaction_status`.
//!
//! What stays with the chain service (the plain path): the zone's history (imported roots, published burns,
//! the accounts private transactions named), which `lez_core` serves only for a sequencer it runs itself
//! (`getBlocks`, `getTransaction`: the sequencer FFI's queries need its in-process sequencer), and
//! everything on Ethereum. When `lez_core` fails before sending, the chain service sends instead and the
//! reply says so (`fallback`). `{"via": "plain"}` turns the path off.
//!
//! Every call through `lez_core` is logged (`lez_report`): method, milliseconds, and for reads whether the
//! two paths agreed.

use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use ferrata_lez_bridge_wire as bw;
use ferrata_wallet::AccountId;
use serde_json::{json, Value};

use crate::{fixed, hex, unhex};

/// How the module crate reaches `lez_core`: (method, the arguments as a JSON array text, a timeout in
/// milliseconds) -> the method's reply as text (a JSON document for the methods that return one), or why
/// the call did not reach it.
pub type LezCoreCall = fn(&str, &str, u64) -> Result<String, String>;

static LEZ_CORE: OnceLock<LezCoreCall> = OnceLock::new();

/// Installed once by the module crate (inside Logos Core); natively nothing is installed and the path
/// answers with an error.
pub fn set_lez_core(f: LezCoreCall) {
    let _ = LEZ_CORE.set(f);
}

const READ_MS: u64 = 60_000;
const SYNC_MS: u64 = 900_000;
/// A private transaction is executed and proven before it is sent: minutes on a CPU.
const SEND_MS: u64 = 2_400_000;
/// Who proves a private send on this path (a job's `proving` phase).
const PROVER: &str = "lez_core.send_generic_private_transaction_with_accounts: LEZ's wallet executes and proves it inside Logos Core, then sends it";

struct Lez {
    program_elf: Vec<u8>,
    elf_sha256: String,
    wallet: Value,
    /// Make the first private send once as `lez_core` at cc73ebf mentions accounts (none authorizing).
    probe_unauthorized: bool,
    log: Vec<Value>,
}

static LEZ: Mutex<Option<Lez>> = Mutex::new(None);

fn with<T>(f: impl FnOnce(&mut Lez) -> T) -> Option<T> {
    LEZ.lock().ok().and_then(|mut g| g.as_mut().map(f))
}

/// Whether the session's LEZ goes through `lez_core`.
pub(crate) fn active() -> bool {
    LEZ.lock().map(|g| g.is_some()).unwrap_or(false)
}

fn log(entry: Value) {
    with(|l| l.log.push(entry));
}

/// One call of `lez_core` by name: its reply text and the milliseconds it took.
fn call(method: &str, args: &Value, timeout_ms: u64) -> Result<(String, u64), String> {
    let f = LEZ_CORE.get().ok_or("lez_core is reachable only inside Logos Core (the module crate installs the call)")?;
    let t0 = Instant::now();
    let reply = f(method, &args.to_string(), timeout_ms).map_err(|e| format!("lez_core.{method}: {e}"))?;
    Ok((reply, t0.elapsed().as_millis() as u64))
}

/// Lower-case hex with `0x`; `0x` for none.
fn norm(h: &str) -> String {
    format!("0x{}", h.trim().trim_start_matches("0x").trim_start_matches("0X").to_lowercase())
}

/// The canonical bytes argument of Logos Core's calls (`{"_bytes": base64url, unpadded}`: logos-rust-sdk
/// `src/bytes.rs`, logos-protocol's QVariant/JSON converter), which a C++ module receives as `QByteArray`.
pub(crate) fn bytes_arg(b: &[u8]) -> Value {
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

/// `wallet {"method": "lez", "params": ...}`: choose the session's LEZ path.
pub(crate) fn configure(params: &Value, private0: Option<String>) -> Result<Value, String> {
    match params["via"].as_str() {
        Some("plain") | None => {
            let had = LEZ.lock().map_err(|_| "the lez lock is poisoned")?.take().map(|l| l.log.len());
            return Ok(json!({ "via": "plain", "lez_core_calls_before": had }));
        }
        Some("lez_core") => {}
        Some(other) => return Err(format!("lez.via {other:?}: \"lez_core\" (Logos Core's LEZ module) or \"plain\"")),
    }
    let w = &params["wallet"];
    let config = w["config"].as_str().ok_or("lez.wallet.config: the LEZ wallet's wallet_config.json")?;
    let storage = w["storage"].as_str().ok_or("lez.wallet.storage: the LEZ wallet's storage.json")?;
    let password_file = w["password_file"].as_str().ok_or("lez.wallet.password_file")?;
    let password = std::fs::read_to_string(password_file).map_err(|e| format!("{password_file}: {e}"))?;
    let password = password.trim_end_matches(['\n', '\r']).to_string();
    let bin = params["program_bin"].as_str().ok_or("lez.program_bin: the bridge program's binary (for private execution)")?;
    let program_elf = std::fs::read(bin).map_err(|e| format!("{bin}: {e}"))?;
    let elf_sha256 = hex(&ferrata_bridge_core::sha256(&program_elf));
    // force_rpc = true: the wallet is a client of the zone's sequencer (its config names it), not of a
    // sequencer lez_core would run itself.
    let mut calls = Vec::new();
    let (opened, ms) = call("open", &json!([config, storage, password, true]), READ_MS)?;
    calls.push(json!({ "method": "open", "reply": opened, "ms": ms }));
    let (addr, ms) = call("get_sequencer_addr", &json!([]), READ_MS)?;
    calls.push(json!({ "method": "get_sequencer_addr", "reply": addr, "ms": ms }));
    let (height, ms) = call("get_current_block_height", &json!([]), READ_MS)?;
    calls.push(json!({ "method": "get_current_block_height", "reply": height, "ms": ms }));
    let (accounts, ms) = call("list_accounts", &json!([]), READ_MS)?;
    calls.push(json!({ "method": "list_accounts", "ms": ms }));
    let list: Value = serde_json::from_str(&accounts).unwrap_or(Value::Null);
    let ids: Vec<String> = list
        .as_array()
        .into_iter()
        .flatten()
        .filter(|a| a["is_public"] == false)
        .filter_map(|a| a["account_id"].as_str().map(norm))
        .collect();
    let has_private0 = private0.as_deref().map(|p| ids.contains(&norm(p)));
    if opened.trim() != "0" && height.trim() == "0" {
        return Err(format!("lez_core did not open the wallet (open answered {opened}; its current block height is {height})"));
    }
    if has_private0 == Some(false) {
        return Err("lez_core's wallet does not hold the session's private account /0".into());
    }
    let wallet = json!({ "config": config, "storage": storage, "sequencer": addr, "head": height.trim(),
                         "private_accounts": ids.len(), "holds_private_0": has_private0 });
    let summary = json!({
        "via": "lez_core",
        "wallet": wallet,
        "program_bin_sha256": elf_sha256,
        "program_bin_bytes": program_elf.len(),
        "calls": calls,
        "reads": "every bridge actor state through lez_core.get_account_public, beside the chain service's",
        "private_holding": "lez_core.sync_to_block + get_account_private, beside the chain service's wallet",
        "private_sends": "lez_core.send_generic_private_transaction_with_accounts (executed and proven by LEZ's wallet inside Logos Core)",
        "plain_path": "the zone's history and Ethereum: the chain service; a send lez_core refuses goes through the chain service",
    });
    let probe_unauthorized = params["probe_unauthorized"].as_bool().unwrap_or(false);
    let mut log = vec![json!({ "kind": "open", "calls": summary["calls"].clone() })];
    log.extend(calls.into_iter().map(|c| json!({ "kind": "call", "call": c })));
    *LEZ.lock().map_err(|_| "the lez lock is poisoned")? =
        Some(Lez { program_elf, elf_sha256, wallet, probe_unauthorized, log });
    Ok(summary)
}

/// The bridge program's actor state of `account` as `lez_core.get_account_public` answers it (`None`: no answer), the
/// milliseconds and how many actor states the account holds.
fn actor_state(account: &str, program: &str) -> Result<(Option<String>, u64, usize), String> {
    let (reply, ms) = call("get_account_public", &json!([norm(account)]), READ_MS)?;
    let (state, states) = parse_actor_state(&reply, program)?;
    Ok((state, ms, states))
}

/// `get_account_public`'s reply read for the bridge program's state: `None` when `lez_core` answered nothing. It
/// answers "" whenever its wallet FFI reports an error (lez_core_module.cpp `get_account_public`): for an account the
/// zone does not have (`AccountNotFound`) and for a read that failed alike, so an empty reply is no value at all, and
/// never counts as agreeing with the chain service's "absent".
fn parse_actor_state(reply: &str, program: &str) -> Result<(Option<String>, usize), String> {
    if reply.trim().is_empty() {
        return Ok((None, 0));
    }
    let v: Value = serde_json::from_str(reply).map_err(|e| format!("get_account_public: not JSON ({e}): {reply}"))?;
    let states = v["actor_states"].as_array().ok_or_else(|| format!("get_account_public: no actor_states: {reply}"))?;
    let p = norm(program);
    let data = states.iter().find(|s| s["program"].as_str().map(norm) == Some(p.clone())).and_then(|s| s["data"].as_str()).map(norm);
    Ok((Some(data.unwrap_or_else(|| "0x".into())), states.len()))
}

/// One read's log entry and the value the module goes on with: `lez_core`'s when it answered (compared with the chain
/// service's), the chain service's otherwise. An unanswered read (`answered: false`) and a failed call (`error`) are
/// neither equal nor different: they are not counted as agreeing.
fn read_entry(what: &str, account: &str, answer: Result<(Option<String>, u64, usize), String>, plain: &str) -> (String, Value) {
    let plain = norm(plain);
    let bytes = |h: &str| (h.len() - 2) / 2;
    match answer {
        Ok((Some(lez), ms, states)) => {
            let equal = lez == plain;
            (lez.clone(), json!({ "kind": "read", "what": what, "account": norm(account), "via": "lez_core.get_account_public", "answered": true,
                                  "equal": equal, "bytes": bytes(&lez), "plain_bytes": bytes(&plain), "actor_states": states, "ms": ms }))
        }
        Ok((None, ms, _)) => (plain.clone(), json!({ "kind": "read", "what": what, "account": norm(account), "via": "lez_core.get_account_public",
            "answered": false, "equal": Value::Null, "plain_bytes": bytes(&plain), "ms": ms,
            "note": "lez_core answered nothing (its wallet reports an absent account and a failed read alike): not compared; the chain service's value kept" })),
        Err(e) => (plain, json!({ "kind": "read", "what": what, "account": norm(account), "error": e, "fallback": "the chain service's value" })),
    }
}

/// One bridge state read again through `lez_core`, compared with the chain service's ([`read_entry`]).
fn reread(what: &str, account: &str, program: &str, plain: &str) -> (String, Value) {
    let (value, entry) = read_entry(what, account, actor_state(account, program), plain);
    log(entry.clone());
    (value, entry)
}

/// The bridge actor states of a `module.inputs` reply that `lez_core` reads again: (the key in the chain service's
/// reply, what it is, the account that holds it). `token(cid, token)` for the token the inputs were asked for.
pub(crate) fn wanted_reads(inputs: &Value, s: &Value, nullifier: Option<&str>, token: &[u8; 20]) -> Result<Vec<(&'static str, String, AccountId)>, String> {
    let program = AccountId::new(fixed::<32>(&s["program"], "program")?);
    let cid = fixed::<32>(&s["config_id"], "config_id")?;
    let name = if *token == [0u8; 20] { "ETH".to_string() } else { hex(token) };
    let mut wanted = vec![
        ("config_state", "config(cid)".to_string(), bw::config_actor(program, &cid).account_id),
        ("token_state", format!("token(cid, {name})"), bw::token_actor(program, &cid, token).account_id),
    ];
    if let Some(n) = inputs["leaf_count"].as_u64() {
        wanted.push(("root_state", format!("root(cid, {n})"), bw::root_actor(program, &cid, n).account_id));
    }
    if let Some(nf) = nullifier {
        let chain_id = s["chain_id"].as_u64().ok_or("chain_id")?;
        let vault: [u8; 20] = fixed(&s["vault"], "vault")?;
        let nf: [u8; 32] = fixed(&json!(nf), "nullifier")?;
        wanted.push(("nullifier_state", "nullifier(cid, chain, vault, N)".into(), bw::nullifier_actor(program, &cid, chain_id, &vault, &nf).account_id));
    }
    Ok(wanted)
}

/// The chain service's `module.inputs` with every bridge actor state in it read again through `lez_core`
/// (`config_state`, `token_state` of `token`, `root_state`, `nullifier_state`, `public_holder_states`). The reply
/// gains `lez`: what was read, and whether the two paths agreed.
pub(crate) fn verify_inputs(inputs: &mut Value, s: &Value, nullifier: Option<&str>, token: &[u8; 20]) -> Result<(), String> {
    let program = AccountId::new(fixed::<32>(&s["program"], "program")?);
    let cid = fixed::<32>(&s["config_id"], "config_id")?;
    let p = hex(program.value());
    let mut reads = Vec::new();
    for (key, what, account) in wanted_reads(inputs, s, nullifier, token)? {
        if let Some(plain) = inputs[key].as_str().map(str::to_string) {
            let (v, e) = reread(&what, &hex(account.value()), &p, &plain);
            inputs[key] = json!(v);
            reads.push(e);
        }
    }
    if let Some(holders) = inputs["public_holder_states"].as_array_mut() {
        for h in holders.iter_mut() {
            let Some(account) = h["account"].as_str() else { continue };
            let id = AccountId::new(fixed::<32>(&json!(account), "public holder")?);
            let holder = bw::holder_actor(program, &cid, id).account_id;
            let plain = h["state"].as_str().unwrap_or("0x").to_string();
            let (v, e) = reread(&format!("holder({})", &norm(account)[..10]), &hex(holder.value()), &p, &plain);
            h["state"] = json!(v);
            reads.push(e);
        }
    }
    let equal = reads.iter().filter(|r| r["equal"] == true).count();
    let unanswered = reads.iter().filter(|r| r["answered"] == false).count();
    inputs["lez"] = json!({
        "via": "lez_core",
        "reads": reads.len(),
        "equal": equal,
        "unanswered": unanswered,
        "detail": reads,
        "history": "the chain service's (lez_core's block queries answer only for a sequencer it runs itself)",
    });
    Ok(())
}

/// The wallet's private account as `lez_core`'s wallet sees it after a sync to the zone's head, compared with
/// the chain service's wallet (`plain`: `{known, bridge_state, native_balance}`). Returns the module's view in
/// the chain service's shape, from `lez_core`.
pub(crate) fn private_state(account: &str, program: &str, plain: &Value) -> Result<Value, String> {
    let (height, ms_h) = call("get_current_block_height", &json!([]), READ_MS)?;
    let head: i64 = height.trim().parse().map_err(|_| format!("get_current_block_height: {height}"))?;
    let (synced, ms_s) = call("sync_to_block", &json!([head]), SYNC_MS)?;
    let (reply, ms_a) = call("get_account_private", &json!([norm(account)]), READ_MS)?;
    let p = norm(program);
    let (known, state) = if reply.trim().is_empty() {
        (false, "0x".to_string())
    } else {
        let v: Value = serde_json::from_str(&reply).map_err(|e| format!("get_account_private: not JSON ({e})"))?;
        let s = v["actor_states"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|s| s["program"].as_str().map(norm) == Some(p.clone()))
            .and_then(|s| s["data"].as_str())
            .map(norm)
            .unwrap_or_else(|| "0x".into());
        (true, s)
    };
    let plain_state = norm(plain["bridge_state"].as_str().unwrap_or("0x"));
    let equal = state == plain_state && known == (plain["known"] == true);
    log(json!({ "kind": "private", "account": norm(account), "head": head, "sync": synced.trim(), "known": known,
                "equal": equal, "bytes": (state.len() - 2) / 2, "plain_bytes": (plain_state.len() - 2) / 2,
                "ms": { "height": ms_h, "sync": ms_s, "read": ms_a } }));
    Ok(json!({ "known": known, "bridge_state": state, "native_balance": plain["native_balance"],
               "lez": { "via": "lez_core.get_account_private", "synced_to": head, "equal": equal, "plain_bridge_state": plain_state } }))
}

/// The module's accounts (the chain service's `module.submit` shape) as `lez_core`'s `accounts_json`
/// entries, and each one's actor-state program.
pub(crate) fn accounts_json(accounts: &Value, authorizes: Option<bool>) -> Result<(Vec<String>, Vec<String>), String> {
    let mut out = Vec::new();
    let mut programs = Vec::new();
    for a in accounts.as_array().ok_or("accounts: a list")? {
        let account = norm(a["account"].as_str().ok_or("accounts[].account")?);
        let program = norm(a["program"].as_str().ok_or("accounts[].program")?);
        let mut e = match a["kind"].as_str().unwrap_or("") {
            "private_self" => json!({ "kind": "private_owned", "account_id": account }),
            "pda" => json!({ "kind": "public_pda", "account_id": account, "program": program,
                             "seed": norm(a["seed"].as_str().ok_or("accounts[].seed")?) }),
            "signer" => json!({ "kind": "public", "account_id": account }),
            "public" => json!({ "kind": "public_no_sign", "account_id": account }),
            k => return Err(format!("account kind {k}: not one lez_core takes")),
        };
        if let Some(f) = authorizes {
            if e["kind"] != "public_no_sign" {
                e["authorizes"] = json!(f);
            }
        }
        out.push(e.to_string());
        programs.push(program);
    }
    Ok((out, programs))
}

/// A private transaction through `lez_core`: synced to the zone's head, then executed and proven by LEZ's
/// wallet inside Logos Core and sent. `Ok` carries `lez_core`'s reply (`success`, `tx_hash`); `Err` means
/// nothing was sent.
pub(crate) fn send_private(message_hex: &str, accounts: &Value, root_index: u64, program: &str, what: &str) -> Result<Value, String> {
    let (elf, sha, probe) = with(|l| (l.program_elf.clone(), l.elf_sha256.clone(), std::mem::take(&mut l.probe_unauthorized)))
        .ok_or("the lez_core path is not configured")?;
    let message = unhex(message_hex)?;
    let p = norm(program);
    let kind = json!({ "kind": "disclosed", "account_id": p }).to_string();
    let (height, _) = call("get_current_block_height", &json!([]), READ_MS)?;
    let head: i64 = height.trim().parse().map_err(|_| format!("get_current_block_height: {height}"))?;
    let (synced, ms_sync) = call("sync_to_block", &json!([head]), SYNC_MS)?;
    let mut attempts = Vec::new();
    if probe {
        // As lez_core at cc73ebf mentions every account: none authorizing.
        let (acc, programs) = accounts_json(accounts, Some(false))?;
        let args = json!([acc, root_index, bytes_arg(&message), bytes_arg(&elf), kind, [], [], programs]);
        crate::jobs::phase("proving", json!({ "by": PROVER, "attempt": "as lez_core cc73ebf mentions accounts", "synced_to": head }));
        let (reply, ms) = call("send_generic_private_transaction_with_accounts", &args, SEND_MS)?;
        let v: Value = serde_json::from_str(&reply).unwrap_or(json!({ "raw": reply }));
        let e = json!({ "kind": "send", "what": what, "attempt": "as lez_core cc73ebf mentions accounts (none authorizing)",
                        "accounts": acc, "reply": v, "ms": ms });
        log(e.clone());
        attempts.push(e);
        if v["success"] == true && v["tx_hash"].as_str().is_some_and(|h| !h.is_empty()) {
            return Ok(json!({ "reply": v, "attempts": attempts, "synced_to": head, "program_bin_sha256": sha }));
        }
    }
    let (acc, programs) = accounts_json(accounts, None)?;
    let args = json!([acc, root_index, bytes_arg(&message), bytes_arg(&elf), kind, [], [], programs]);
    crate::jobs::phase("proving", json!({ "by": PROVER, "synced_to": head }));
    let (reply, ms) = call("send_generic_private_transaction_with_accounts", &args, SEND_MS)?;
    let v: Value = serde_json::from_str(&reply).map_err(|e| format!("send_generic_private_transaction_with_accounts: not JSON ({e}): {reply}"))?;
    let e = json!({ "kind": "send", "what": what, "attempt": "authorizing as LEZ's wallet API (the patched lez_core)",
                    "accounts": acc, "reply": v, "ms": ms, "sync": { "to": head, "rc": synced.trim(), "ms": ms_sync } });
    log(e.clone());
    attempts.push(e);
    if v["success"] != true || v["tx_hash"].as_str().map_or(true, str::is_empty) {
        return Err(format!("lez_core did not send it: {}", v["error"].as_str().unwrap_or(&reply)));
    }
    Ok(json!({ "reply": v, "attempts": attempts, "synced_to": head, "program_bin_sha256": sha, "prove_ms": ms }))
}

/// `wallet {"method": "lez_probe"}`: what `lez_core` answers for one of the bridge program's PDAs (the
/// configuration account `config(cid)`), as the private claim and burn mention them, before any wallet or
/// program is involved (the message and the program are one zero byte each). A `lez_core` without the
/// `public_pda` kind refuses the entry; one with it gets past the accounts and fails later (no bytecode).
pub(crate) fn probe(s: &Value) -> Result<Value, String> {
    let program = AccountId::new(fixed::<32>(&s["program"], "program")?);
    let cid = fixed::<32>(&s["config_id"], "config_id")?;
    let account = bw::config_actor(program, &cid).account_id;
    let seed = bw::config_seed(&cid);
    let p = hex(program.value());
    let entry = json!({ "kind": "public_pda", "account_id": hex(account.value()), "program": p, "seed": hex(seed.as_bytes()) }).to_string();
    let kind = json!({ "kind": "disclosed", "account_id": p }).to_string();
    let args = json!([[entry], 0, bytes_arg(&[0]), bytes_arg(&[0]), kind, [], [], [p]]);
    let (reply, ms) = call("send_generic_private_transaction_with_accounts", &args, READ_MS)?;
    Ok(json!({ "method": "send_generic_private_transaction_with_accounts", "accounts_json": [entry], "reply": serde_json::from_str::<Value>(&reply).unwrap_or(json!(reply)), "ms": ms }))
}

/// `lez_core.poll_transaction_status(hash)`: whether LEZ's wallet finds it on the zone.
pub(crate) fn poll(tx_hash: &str) -> Value {
    match call("poll_transaction_status", &json!([tx_hash.trim_start_matches("0x")]), READ_MS) {
        Ok((r, ms)) => {
            let e = json!({ "kind": "poll", "tx_hash": tx_hash, "found": r.trim() == "true", "ms": ms });
            log(e.clone());
            e
        }
        Err(e) => json!({ "kind": "poll", "tx_hash": tx_hash, "error": e }),
    }
}

/// `wallet {"method": "lez_report"}`: every call through `lez_core` this session, and the totals.
pub(crate) fn report() -> Value {
    with(|l| {
        let reads: Vec<&Value> = l.log.iter().filter(|e| e["kind"] == "read").collect();
        let privates: Vec<&Value> = l.log.iter().filter(|e| e["kind"] == "private").collect();
        let sends: Vec<&Value> = l.log.iter().filter(|e| e["kind"] == "send").collect();
        json!({
            "via": "lez_core",
            "wallet": l.wallet,
            "program_bin_sha256": l.elf_sha256,
            "reads": reads.len(),
            "reads_equal": reads.iter().filter(|e| e["equal"] == true).count(),
            "reads_unanswered": reads.iter().filter(|e| e["answered"] == false).count(),
            "reads_failed": reads.iter().filter(|e| !e["error"].is_null()).count(),
            "private_reads": privates.len(),
            "private_reads_equal": privates.iter().filter(|e| e["equal"] == true).count(),
            "sends": sends.len(),
            "sends_sent": sends.iter().filter(|e| e["reply"]["success"] == true).count(),
            "log": l.log,
        })
    })
    .unwrap_or_else(|| json!({ "via": "plain" }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_are_the_sdk_canonical_form() {
        // logos-rust-sdk src/bytes.rs: base64url, unpadded.
        assert_eq!(bytes_arg(b""), json!({ "_bytes": "" }));
        assert_eq!(bytes_arg(b"f"), json!({ "_bytes": "Zg" }));
        assert_eq!(bytes_arg(b"fo"), json!({ "_bytes": "Zm8" }));
        assert_eq!(bytes_arg(b"foo"), json!({ "_bytes": "Zm9v" }));
        assert_eq!(bytes_arg(&[0xfb, 0xff]), json!({ "_bytes": "-_8" }));
    }

    #[test]
    fn accounts_map_to_lez_core_kinds() {
        let a = json!([
            { "account": "0xAA", "program": "0xBB", "kind": "private_self" },
            { "account": "0xCC", "program": "0xBB", "kind": "pda", "seed": "0xDD" },
            { "account": "0xEE", "program": "0xBB", "kind": "public" },
        ]);
        let (acc, programs) = accounts_json(&a, None).unwrap();
        let v: Vec<Value> = acc.iter().map(|s| serde_json::from_str(s).unwrap()).collect();
        assert_eq!(v[0], json!({ "kind": "private_owned", "account_id": "0xaa" }));
        assert_eq!(v[1], json!({ "kind": "public_pda", "account_id": "0xcc", "program": "0xbb", "seed": "0xdd" }));
        assert_eq!(v[2], json!({ "kind": "public_no_sign", "account_id": "0xee" }));
        assert_eq!(programs, vec!["0xbb", "0xbb", "0xbb"]);
        let (acc, _) = accounts_json(&a, Some(false)).unwrap();
        let v: Vec<Value> = acc.iter().map(|s| serde_json::from_str(s).unwrap()).collect();
        assert_eq!(v[0]["authorizes"], false);
        assert_eq!(v[1]["authorizes"], false);
        assert!(v[2]["authorizes"].is_null());
        assert!(accounts_json(&json!([{ "account": "0x01", "program": "0x02", "kind": "nope" }]), None).is_err());
    }

    /// The registry and the admin tab read each token's LEZ side again through lez_core: config(cid), the asked
    /// token's own token(cid, token) (not ETH's), the newest imported root.
    #[test]
    fn every_tokens_lez_states_are_read_again_through_lez_core() {
        let program = AccountId::new([0x22; 32]);
        let cid = [0x33u8; 32];
        let s = json!({ "program": format!("0x{}", "22".repeat(32)), "config_id": format!("0x{}", "33".repeat(32)),
                        "chain_id": 11155111, "vault": format!("0x{}", "44".repeat(20)) });
        let tusd = [0x59u8; 20];
        let inputs = json!({ "config_state": "0x01", "token_state": "0x02", "leaf_count": 3, "root_state": "0x03" });
        let w = wanted_reads(&inputs, &s, None, &tusd).unwrap();
        let keys: Vec<&str> = w.iter().map(|(k, _, _)| *k).collect();
        assert_eq!(keys, vec!["config_state", "token_state", "root_state"]);
        assert_eq!(w[0].2, bw::config_actor(program, &cid).account_id);
        assert_eq!(w[1].2, bw::token_actor(program, &cid, &tusd).account_id, "the asked token's gate");
        assert_ne!(w[1].2, bw::token_actor(program, &cid, &[0u8; 20]).account_id);
        assert_eq!(w[1].1, format!("token(cid, 0x{})", "59".repeat(20)));
        assert_eq!(w[2].2, bw::root_actor(program, &cid, 3).account_id);
        let eth = wanted_reads(&json!({}), &s, None, &[0u8; 20]).unwrap();
        assert_eq!(eth.len(), 2);
        assert_eq!(eth[1].1, "token(cid, ETH)");
        // Natively lez_core is not reachable: every read keeps the chain service's value and says why.
        let mut i = inputs.clone();
        verify_inputs(&mut i, &s, None, &tusd).unwrap();
        assert_eq!(i["lez"]["reads"], 3);
        assert_eq!(i["token_state"], "0x02");
        assert!(i["lez"]["detail"][1]["error"].as_str().unwrap().contains("lez_core"));
    }

    /// An empty `lez_core` reply (its wallet's error: an absent account or a failed read) is no answer: it is not
    /// counted as agreeing with the chain service's "absent" (review: the second token's 17 empty reads were "equal").
    #[test]
    fn an_empty_lez_core_reply_is_no_answer_never_equal() {
        let program = format!("0x{}", "22".repeat(32));
        assert_eq!(parse_actor_state("", &program).unwrap(), (None, 0));
        assert_eq!(parse_actor_state("  \n", &program).unwrap(), (None, 0));
        let held = json!({ "actor_states": [{ "program": program.to_uppercase().replace("0X", "0x"), "data": "0xABCD" }] }).to_string();
        assert_eq!(parse_actor_state(&held, &program).unwrap(), (Some("0xabcd".to_string()), 1));
        let other = json!({ "actor_states": [{ "program": format!("0x{}", "99".repeat(32)), "data": "0x01" }] }).to_string();
        assert_eq!(parse_actor_state(&other, &program).unwrap(), (Some("0x".to_string()), 1), "an account without the program's state");
        assert!(parse_actor_state("not json", &program).is_err());
        // Absent at the chain service, no answer from lez_core: not equal, not counted; the chain service's value kept.
        let (v, e) = read_entry("token(cid, 0x59..)", "0xaa", Ok((None, 4, 0)), "0x");
        assert_eq!((v.as_str(), &e["answered"], &e["equal"]), ("0x", &json!(false), &Value::Null));
        // Both answered: compared.
        let (v, e) = read_entry("config(cid)", "0xaa", Ok((Some("0xabcd".into()), 4, 1)), "0xABCD");
        assert_eq!((v.as_str(), &e["equal"], &e["bytes"]), ("0xabcd", &json!(true), &json!(2)));
        let (_, e) = read_entry("config(cid)", "0xaa", Ok((Some("0x".into()), 4, 1)), "0xabcd");
        assert_eq!(e["equal"], false);
        // A failed call: the chain service's value, the error said.
        let (v, e) = read_entry("root(cid, 1)", "0xaa", Err("lez_core.get_account_public: timed out".into()), "0x01");
        assert_eq!(v, "0x01");
        assert!(e["equal"].is_null() && e["error"].as_str().unwrap().contains("timed out"));
        // The totals a reply carries count the unanswered read apart.
        let reads = [read_entry("a", "0x1", Ok((None, 1, 0)), "0x").1, read_entry("b", "0x2", Ok((Some("0x01".into()), 1, 1)), "0x01").1];
        assert_eq!(reads.iter().filter(|r| r["equal"] == true).count(), 1);
        assert_eq!(reads.iter().filter(|r| r["answered"] == false).count(), 1);
    }

    #[test]
    fn natively_lez_core_is_not_reachable() {
        assert!(call("list_accounts", &json!([]), 10).is_err());
        assert!(!active());
        assert_eq!(report(), json!({ "via": "plain" }));
    }
}
