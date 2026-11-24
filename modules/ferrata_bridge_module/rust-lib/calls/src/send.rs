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
//!
//! Every vault call goes to a deployment the person running Logos Core named ([`pinned`], `FERRATA_DEPLOYMENTS`): a caller of
//! the module cannot have it raise a signing request for another vault or chain, and the purpose the approver shows
//! names the vault and the chain. Not pinned: an ERC-20 deposit's `approve` goes to the caller's token (its spender is
//! the pinned vault), and the reads come through the caller's Ethereum connection, not checked against the pinned chain.

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

/// One call of a bundle: `to`, `value` in wei (decimal), `data`, the line the person reads, and the gas limit when the
/// caller gives one (`None`: the stack's own estimate with the margin of [`margin`], set by [`with_limits`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Call {
    pub to: [u8; 20],
    pub value: String,
    pub data: Vec<u8>,
    pub label: String,
    pub gas_limit: Option<u64>,
}

impl Call {
    pub(crate) fn new(to: [u8; 20], value: impl Into<String>, data: Vec<u8>, label: impl Into<String>) -> Self {
        Call { to, value: value.into(), data, label: label.into(), gas_limit: None }
    }

    pub(crate) fn to_json(&self) -> Value {
        let mut v = json!({ "to": crate::evm::checksum(&self.to), "value": self.value, "data": hex(&self.data), "label": self.label });
        if let Some(g) = self.gas_limit {
            // A decimal STRING: tx_sender_module f1754c3 reads a call's `gasLimit` as `Option<String>` (glue.rs
            // `CallSpec`); a JSON number is refused as an invalid request, whatever its README says.
            v["gasLimit"] = json!(g.to_string());
        }
        v
    }
}

/// EIP-7825's cap on one transaction's gas (Osaka): no limit above it is sent.
pub(crate) const TX_GAS_CAP: u64 = 1 << 24;

/// The limit a call is sent with when the caller gives none: the stack's estimate plus a quarter plus 20,000 gas.
/// The estimate is the least gas the call needed when it was estimated, but it is included later: after the person's
/// approval, in another block, with another timestamp and perhaps after other transactions. The vault's rate-limit
/// buckets write `lastUpdate = block.timestamp` (a no-op in the second the bucket was last touched, a real write a
/// second later: about 2,800 gas, measured as a 2,525-gas gap that reverted an approved `setTokenCap`), and a deposit's
/// Merkle insertion writes a different branch slot at each leaf index (up to ~17,000 gas more when the slot is still
/// empty). The margin covers these; unused gas is not paid for (the fee is gas used times the price), it only raises
/// the fee ceiling the account must hold.
pub(crate) fn margin(estimate: u64) -> u64 {
    estimate.saturating_add(estimate / 4).saturating_add(20_000).min(TX_GAS_CAP).max(estimate)
}

/// The environment variable naming the deployments this module may send to:
/// `FERRATA_DEPLOYMENTS="<chain id>:<vault address>[,<chain id>:<vault address>...]"`, set by the person who runs Logos
/// Core (its modules start with its environment). No caller of the module can change it (review L1: any module or
/// client that can call ferrata_bridge_module could otherwise have it raise signing requests to a vault it names).
pub(crate) const DEPLOYMENTS_ENV: &str = "FERRATA_DEPLOYMENTS";

fn deployments_text() -> Option<String> {
    if cfg!(test) {
        return test_deployments();
    }
    std::env::var(DEPLOYMENTS_ENV).ok().filter(|t| !t.trim().is_empty())
}

#[cfg(test)]
fn test_deployments() -> Option<String> {
    fake::DEPLOYMENTS.lock().unwrap().clone()
}

#[cfg(not(test))]
fn test_deployments() -> Option<String> {
    None
}

/// The entries of `FERRATA_DEPLOYMENTS`: (chain id, vault).
pub(crate) fn parse_deployments(text: &str) -> Result<Vec<(u64, [u8; 20])>, String> {
    text.split(',')
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(|e| {
            let (c, v) = e.split_once(':').ok_or_else(|| format!("{DEPLOYMENTS_ENV}: {e} is not <chain id>:<vault address>"))?;
            let chain: u64 = c.trim().parse().map_err(|_| format!("{DEPLOYMENTS_ENV}: the chain id {c} is not a number"))?;
            Ok((chain, crate::fixed::<20>(&json!(v.trim()), "a FERRATA_DEPLOYMENTS vault")?))
        })
        .collect()
}

/// Whether the module may send to `vault` on `chain_id`: only to a deployment the person named in
/// [`DEPLOYMENTS_ENV`]; none named, it sends nowhere. Checked before anything is asked or signed.
pub(crate) fn pinned(chain_id: u64, vault: &[u8; 20]) -> Result<(), String> {
    let text = deployments_text().ok_or_else(|| format!(
        "refused before anything was asked: no deployment is pinned. The module sends only to the deployments the person \
         running Logos Core names in {DEPLOYMENTS_ENV}=\"<chain id>:<vault>\" (asked: vault {} on chain {chain_id})", hex(vault)))?;
    if parse_deployments(&text)?.iter().any(|(c, v)| *c == chain_id && v == vault) {
        Ok(())
    } else {
        Err(format!("refused before anything was asked: vault {} on chain {chain_id} is not a deployment this module may send to \
                     ({DEPLOYMENTS_ENV}: {})", hex(vault), text.trim()))
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

/// One call of `method` of another Logos module by name (session.rs: `eth_rpc_module`'s chain registry for the
/// endpoints): its reply as JSON (a reply that is not JSON as `{"ok": false, "error": text}`), or why it did not reach it.
pub(crate) fn call_module(module: &str, method: &str, args: Value, timeout_ms: u64) -> Result<Value, String> {
    let f = CALL.get().ok_or_else(|| format!("{module}.{method}: reachable only from inside Logos Core"))?;
    let text = f(module, method, &args, timeout_ms)?;
    Ok(serde_json::from_str(&text).unwrap_or_else(|_| json!({ "ok": false, "error": text })))
}

fn why(v: &Value) -> String {
    let code = v["code"].as_str().map(|c| format!("{c}: ")).unwrap_or_default();
    let error = v.get("error").or_else(|| v.get("reason")).map(|e| e.as_str().map_or(e.to_string(), str::to_string));
    format!("{code}{}", error.unwrap_or_else(|| v.to_string()))
}

/// A quantity as `tx_sender_module` answers it: a JSON number, or a decimal or `0x` string.
fn quantity(v: &Value) -> Option<u64> {
    match v {
        Value::Number(n) => n.as_u64(),
        Value::String(t) => match t.strip_prefix("0x") {
            Some(h) => u64::from_str_radix(h, 16).ok(),
            None => t.parse().ok(),
        },
        _ => None,
    }
}

/// The request with a `gasLimit` on every call: a caller's own limit as given; otherwise the stack's own estimate
/// (`tx_sender_module.prepare`: `fee_module` estimates each call as the chain will find it, an ERC-20 `approve` earlier
/// in the bundle applied to the calls after it) plus [`margin`]. `prepare` reserves nothing and asks nobody, so a call
/// that would revert is refused here, before the person is asked. `(the request, [{estimate, limit, source}] per call)`.
pub(crate) fn with_limits(req: &Value) -> Result<(Value, Value), String> {
    let calls = req["calls"].as_array().ok_or("the request has no calls")?;
    let given: Vec<Option<u64>> = calls.iter().map(|c| quantity(&c["gasLimit"])).collect();
    let mut out = req.clone();
    if given.iter().all(Option::is_some) {
        let gas = given.iter().map(|g| json!({ "limit": g, "source": "given by the caller" })).collect::<Vec<_>>();
        return Ok((out, json!(gas)));
    }
    let p = module_call("prepare", json!([req.to_string()]))?;
    if p["ok"] != true {
        return Err(format!("{TX_SENDER}.prepare refused (nothing was asked or signed): {}", why(&p)));
    }
    let legs = p["legs"].as_array().ok_or("tx_sender_module.prepare answered no legs")?;
    if legs.len() != calls.len() {
        return Err(format!("tx_sender_module.prepare priced {} calls, not {}", legs.len(), calls.len()));
    }
    let mut gas = Vec::new();
    for (i, leg) in legs.iter().enumerate() {
        let estimate = quantity(&leg["gasLimit"]).filter(|g| *g > 0).ok_or_else(|| format!("tx_sender_module.prepare: no gasLimit for call {}", i + 1))?;
        let (limit, source) = match given[i] {
            Some(g) => (g, "given by the caller".to_string()),
            None => (margin(estimate), format!("the stack's estimate ({}) + 25% + 20000", leg["gasSource"].as_str().unwrap_or("?"))),
        };
        out["calls"][i]["gasLimit"] = json!(limit.to_string());
        gas.push(json!({ "estimate": estimate, "limit": limit, "source": source }));
    }
    Ok((out, json!(gas)))
}

/// `tx_sender_module.send`, every call given its gas limit first ([`with_limits`]): the request id and the signer
/// manager's handle (`{requestId, handle, request, gas}`). Asked again while the account's history opens
/// (`unavailable`), for at most `open_timeout`.
pub(crate) fn send(req: &Value, open_timeout: Duration) -> Result<Value, String> {
    let t0 = Instant::now();
    let (req, gas) = with_limits(req)?;
    let text = req.to_string();
    loop {
        let v = module_call("send", json!([text]))?;
        if v["ok"] == true {
            let id = v["requestId"].as_str().ok_or("tx_sender_module.send answered no requestId")?;
            return Ok(json!({ "requestId": id, "handle": v["handle"], "pending": v["pending"], "request": req, "gas": gas,
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
/// `timeout` passes; each status seen is passed to `seen` (the job's phases). The final reply. On the timeout, or when
/// three reads in a row fail, the request is withdrawn (`cancel_send`) so that it cannot still be approved and sent
/// untracked; the error says what the withdrawal answered.
pub(crate) fn wait(request_id: &str, timeout: Duration, mut seen: impl FnMut(&Value)) -> Result<Value, String> {
    let t0 = Instant::now();
    let mut last = String::new();
    let mut failed_reads = 0;
    loop {
        let s = match status(request_id) {
            Ok(s) => {
                failed_reads = 0;
                s
            }
            Err(e) => {
                failed_reads += 1;
                if failed_reads >= 3 {
                    return Err(format!("{TX_SENDER}.send_status failed three times ({e}); {}", withdraw(request_id)));
                }
                std::thread::sleep(POLL);
                continue;
            }
        };
        let state = s["status"].as_str().unwrap_or("").to_string();
        if state != last {
            seen(&s);
            last = state;
        }
        if s["final"] == true {
            return Ok(s);
        }
        if t0.elapsed() > timeout {
            return Err(format!("{TX_SENDER}: request {request_id} not final after {} s (last: {}); {}", timeout.as_secs(), why(&s), withdraw(request_id)));
        }
        std::thread::sleep(POLL);
    }
}

/// `cancel_send`, as one sentence for an error.
fn withdraw(request_id: &str) -> String {
    match cancel(request_id) {
        Ok(v) if v["ok"] == true => format!("the request {request_id} was withdrawn (cancel_send): nothing more will be signed for it"),
        Ok(v) => format!("withdrawing the request {request_id} was refused ({}): it may still be approved in the Logos signer", why(&v)),
        Err(e) => format!("withdrawing the request {request_id} failed ({e}): it may still be approved in the Logos signer"),
    }
}

/// `tx_sender_module.cancel_send(request_id)`.
pub(crate) fn cancel(request_id: &str) -> Result<Value, String> {
    module_call("cancel_send", json!([request_id]))
}

/// The hashes of a final `send_status` that broadcast, in the order of the calls; the refusal otherwise. `stuck` (a
/// broadcast whose node has not answered for 180 s: tx_sender_module cannot say it is not on chain) counts as
/// broadcast: its calls' receipts are awaited and, unseen, reported as not yet mined, never as failed. A `failed` send
/// names every call that had already left (an ERC-20 `approve` broadcast before a failed `deposit` leaves an allowance).
pub(crate) fn broadcast_hashes(s: &Value) -> Result<Vec<String>, String> {
    if s["ok"] == true && (s["status"] == "broadcast" || s["status"] == "stuck") {
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
        other => format!("{other}: {}{}", why(s), left_calls(s)),
    })
}

/// The calls of a send that had already left (`legs[].left`, with their hashes), as a clause for an error: what a
/// person must check before sending again.
fn left_calls(s: &Value) -> String {
    let legs = s["legs"].as_array().cloned().unwrap_or_default();
    let left: Vec<String> = legs
        .iter()
        .enumerate()
        .filter(|(_, l)| l["left"] == true)
        .map(|(i, l)| format!("call {} of {} ({}), transaction {}", i + 1, legs.len(), l["label"].as_str().unwrap_or("?"), l["hash"].as_str().unwrap_or("?")))
        .collect();
    if left.is_empty() {
        String::new()
    } else {
        format!("; already broadcast before it failed: {} (check it before sending again)", left.join("; "))
    }
}

/// How long a request waits for its calls' receipts once they are broadcast (`send.receipt_timeout_s`, seconds), or
/// [`RECEIPT_WAIT`].
pub(crate) fn receipt_wait(sender: &Value) -> Duration {
    sender["receipt_timeout_s"].as_u64().map_or(RECEIPT_WAIT, Duration::from_secs)
}

/// The gas limit a request gives for its vault call (`send.gas_limit`: a number or a decimal string), if any. Used
/// verbatim, as `tx_sender_module` uses a caller's own limit: a limit below what the call needs is approved, mined and
/// reverts (which the replies then report).
pub(crate) fn given_limit(sender: &Value) -> Result<Option<u64>, String> {
    match &sender["gas_limit"] {
        Value::Null => Ok(None),
        v => quantity(v).filter(|g| *g > 0).map(Some).ok_or_else(|| "send.gas_limit: a positive number of gas".to_string()),
    }
}

/// Where the person approves a send.
pub(crate) const APPROVE_IN: &str = "the Logos signer: evm_signer_ui in Basecamp (intent evm.signing.approve {handle}), or headless evm_signer_cli approve <handle> <bundle_id> @password-file";

/// What one send through the stack did.
pub(crate) struct Sent {
    /// `{via, requestId, handle, from, calls (with their gas limits), gas, statuses, final, hashes, receipts, reverted?}`.
    pub record: Value,
    /// Every call's receipt, in the calls' order; empty when the caller did not wait.
    pub receipts: Vec<Value>,
    /// The first call whose receipt has status 0, as one sentence.
    pub reverted: Option<String>,
    /// The calls broadcast whose receipt was not seen within the wait (from the first such call on): they may still be
    /// mined. Reported as broadcast and not yet mined, never as failed: a person told "not deposited" deposits again.
    pub unconfirmed: Vec<String>,
}

impl Sent {
    /// The last call's receipt (the vault's call of a bundle); `None` when the caller did not wait.
    pub(crate) fn last(&self) -> Option<&Value> {
        self.receipts.last()
    }

    /// The last call's transaction hash.
    pub(crate) fn last_hash(&self) -> Value {
        self.record["hashes"].as_array().and_then(|h| h.last()).cloned().unwrap_or(Value::Null)
    }

    /// "Broadcast, not yet mined" as one sentence, when a call's receipt was not seen: what the person must not do.
    pub(crate) fn unconfirmed_text(&self, what: &str) -> Option<String> {
        let first = self.unconfirmed.first()?;
        Some(format!(
            "broadcast, not yet mined: transaction {first}{} was sent after your approval and no receipt was seen within {} s. \
             Do not send {what} again: it may still be mined. Check that transaction (or refresh) before acting again",
            if self.unconfirmed.len() > 1 { format!(" (and {} more)", self.unconfirmed.len() - 1) } else { String::new() },
            self.record["receipt_wait_s"].as_u64().unwrap_or(0)))
    }
}

/// How long a broadcast call's receipt is awaited before the send is reported as broadcast and not yet mined.
pub(crate) const RECEIPT_WAIT: Duration = Duration::from_secs(600);

/// Every broadcast call's receipt through `v`'s Ethereum path, read until each is seen or `wait` passes. A read that
/// fails is read again (a node that does not answer for a moment is no failure of the send). (the receipts seen, in
/// the calls' order; the hashes from the first unseen call on; the last read error).
fn receipts(v: &mut crate::evm::Vault, hashes: &[String], wait: Duration) -> (Vec<Value>, Vec<String>, Option<String>) {
    let t0 = Instant::now();
    let (mut seen, mut last_error) = (Vec::new(), None);
    for (i, h) in hashes.iter().enumerate() {
        loop {
            match v.rpc.call("eth_getTransactionReceipt", json!([h])) {
                Ok(r) if !r.is_null() => {
                    seen.push(r);
                    break;
                }
                Ok(_) => {}
                Err(e) => last_error = Some(e),
            }
            if t0.elapsed() >= wait {
                return (seen, hashes[i..].to_vec(), last_error);
            }
            std::thread::sleep(POLL.min(wait));
        }
    }
    (seen, Vec::new(), last_error)
}

/// One send through Logos Core's send stack: `calls` from `from` on `chain_id`, each with its gas limit
/// ([`with_limits`]); the person's approval in the Logos signer (the job's `awaiting_approval` phase, with `detail`'s
/// fields added); then, when `wait_for_it`, the broadcast and EVERY call's receipt read back through `v`'s Ethereum
/// path. A receipt with status 0 is not a success: `reverted` names the call, its block and the gas it used of its
/// limit. A receipt not seen within `receipt_wait` is not a failure either: `unconfirmed` holds the calls broadcast and
/// not yet mined. `wait_for_it` false answers at the approval request (`record.state` `awaiting_approval`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn through_stack(v: &mut crate::evm::Vault, chain_id: u64, from: &str, calls: &[Call], purpose: &str,
                            wait_for_it: bool, timeout: Duration, receipt_wait: Duration, detail: Value) -> Result<Sent, String> {
    // Only to a deployment the person named; the approver's purpose line names it.
    pinned(chain_id, &v.vault)?;
    let purpose = format!("{purpose} (vault {} on chain {chain_id})", hex(&v.vault));
    let req = request(chain_id, from, calls, &purpose);
    let asked = send(&req, Duration::from_secs(60))?;
    let id = asked["requestId"].as_str().unwrap_or_default().to_string();
    let mut phase = json!({ "requestId": id, "handle": asked["handle"], "from": from, "gas": asked["gas"], "approve_in": APPROVE_IN });
    for (k, x) in detail.as_object().into_iter().flatten() {
        phase[k] = x.clone();
    }
    crate::jobs::phase("awaiting_approval", phase);
    let mut record = json!({ "via": TX_SENDER, "requestId": id, "handle": asked["handle"], "from": from,
                             "calls": asked["request"]["calls"], "gas": asked["gas"] });
    if !wait_for_it {
        record["state"] = json!("awaiting_approval");
        return Ok(Sent { record, receipts: Vec::new(), reverted: None, unconfirmed: Vec::new() });
    }
    let mut statuses = Vec::new();
    let st = wait(&id, timeout, |s| statuses.push(s["status"].clone()))?;
    record["statuses"] = json!(statuses);
    record["final"] = st["status"].clone();
    let hashes = broadcast_hashes(&st)?;
    crate::jobs::phase("sent", json!({ "tx_hash": hashes[hashes.len() - 1], "by": "tx_sender_module (keystore_module signed after the approval; broadcast through eth_rpc_module)" }));
    record["hashes"] = json!(hashes);
    record["receipt_wait_s"] = json!(receipt_wait.as_secs());
    let (seen, unconfirmed, read_error) = receipts(v, &hashes, receipt_wait);
    let (mut receipts, mut summary, mut reverted) = (Vec::new(), Vec::new(), None);
    for (i, (h, r)) in hashes.iter().zip(seen).enumerate() {
        let ok = r["status"].as_str() == Some("0x1");
        let (used, block, limit) = (quantity(&r["gasUsed"]), quantity(&r["blockNumber"]), asked["gas"][i]["limit"].as_u64());
        summary.push(json!({ "hash": h, "status": if ok { "success" } else { "reverted" }, "block": block, "gas_used": used, "gas_limit": limit }));
        if !ok && reverted.is_none() {
            let n = |x: Option<u64>| x.map_or_else(|| "?".to_string(), |x| x.to_string());
            reverted = Some(format!(
                "reverted: call {} of {} ({}), transaction {h}, was mined in block {} and reverted, using {} gas of its limit {}: approved and paid for, but it did nothing",
                i + 1, hashes.len(), calls.get(i).map_or("?", |c| c.label.as_str()), n(block), n(used), n(limit)));
        }
        receipts.push(r);
    }
    for h in &unconfirmed {
        summary.push(json!({ "hash": h, "status": "not yet mined", "last_read_error": read_error }));
    }
    record["receipts"] = json!(summary);
    if let Some(r) = &reverted {
        record["reverted"] = json!(r);
    }
    if !unconfirmed.is_empty() {
        record["unconfirmed"] = json!(unconfirmed);
    }
    Ok(Sent { record, receipts, reverted, unconfirmed })
}

/// Installs a stand-in of `tx_sender_module` for the tests: `prepare` estimates every limitless call at [`ESTIMATE`],
/// `send` records the request, `send_status` approves (or rejects) it at once and answers hashes `0x…abc0`, `…abc1` ...;
/// and a stand-in of `eth_rpc_module` whose receipts have the status [`STATUS`] (`0x1`, or `0x0`: reverted).
#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use std::sync::Mutex;

    pub(crate) static SENT: Mutex<Vec<Value>> = Mutex::new(Vec::new());
    pub(crate) static VERDICT: Mutex<&'static str> = Mutex::new("broadcast");
    pub(crate) static ESTIMATE: Mutex<u64> = Mutex::new(48_564);
    pub(crate) static STATUS: Mutex<&'static str> = Mutex::new("0x1");
    pub(crate) static CANCELLED: Mutex<Vec<String>> = Mutex::new(Vec::new());
    /// Whether the stand-in node has mined the transactions (`false`: every receipt read answers null).
    pub(crate) static MINED: Mutex<bool> = Mutex::new(true);
    /// How many receipt reads fail before the node answers again.
    pub(crate) static FAILING_READS: Mutex<u32> = Mutex::new(0);
    /// `FERRATA_DEPLOYMENTS` for the tests: the test vault `0x0202…` on Sepolia's chain id and on chain 1.
    pub(crate) static DEPLOYMENTS: Mutex<Option<String>> = Mutex::new(None);

    fn call(module: &str, method: &str, args: &Value, _t: u64) -> Result<String, String> {
        assert_eq!(module, TX_SENDER);
        match method {
            "prepare" => {
                let req: Value = serde_json::from_str(args[0].as_str().unwrap()).unwrap();
                let legs: Vec<Value> = req["calls"].as_array().unwrap().iter().map(|c| match quantity(&c["gasLimit"]) {
                    Some(g) => json!({ "gasLimit": g, "gasSource": "given" }),
                    None => json!({ "gasLimit": *ESTIMATE.lock().unwrap(), "gasSource": "estimated" }),
                }).collect();
                Ok(json!({ "ok": true, "legs": legs }).to_string())
            }
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
                let fin = verdict != "awaitingApproval";
                let broadcast = matches!(verdict, "broadcast" | "stuck");
                // `failed` after the first call left (an ERC-20 approve broadcast, the deposit's broadcast failed).
                let legs: Vec<Value> = hashes.iter().enumerate().map(|(k, h)| {
                    let left = broadcast || (verdict == "failed" && k == 0);
                    if left { json!({ "label": format!("call {k}"), "left": true, "hash": h }) } else { json!({ "label": format!("call {k}"), "left": false }) }
                }).collect();
                Ok(json!({ "ok": true, "requestId": args[0], "status": verdict, "final": fin, "legs": legs,
                           "reason": if verdict == "failed" { json!("the node refused the second transaction") } else { Value::Null },
                           "hashes": if broadcast { json!(hashes) } else { json!([]) } }).to_string())
            }
            "cancel_send" => {
                CANCELLED.lock().unwrap().push(args[0].as_str().unwrap_or("").to_string());
                Ok(json!({ "ok": true, "status": "cancelled" }).to_string())
            }
            other => Err(format!("unexpected {other}")),
        }
    }

    fn eth(_chain: i64, method: &str, params: &str, _t: u64) -> Result<String, String> {
        let p: Value = serde_json::from_str(params).unwrap();
        match method {
            "eth_getTransactionReceipt" => {
                let mut failing = FAILING_READS.lock().unwrap();
                if *failing > 0 {
                    *failing -= 1;
                    return Ok(json!({ "ok": false, "code": "unreachable", "error": "the node did not answer" }).to_string());
                }
                if !*MINED.lock().unwrap() {
                    return Ok(json!({ "ok": true, "route": "direct", "result": null }).to_string());
                }
                Ok(json!({ "ok": true, "route": "direct", "result": {
                    "transactionHash": p[0], "status": *STATUS.lock().unwrap(), "blockNumber": "0x8", "gasUsed": "0xbc91", "logs": [] } }).to_string())
            }
            other => Ok(json!({ "ok": false, "code": "test", "error": format!("unexpected {other}") }).to_string()),
        }
    }

    pub(crate) fn install() {
        set_module_call(call);
        crate::rpc::set_eth_rpc_module(eth);
        *DEPLOYMENTS.lock().unwrap() = Some(format!("11155111:0x{v},1:0x{v}", v = "02".repeat(20)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::tests::serial;

    fn calls() -> Vec<Call> {
        vec![
            Call::new([1; 20], "0", vec![0x09, 0x5e, 0xa7, 0xb3], "approve"),
            Call::new([2; 20], "0", vec![0xde, 0xad], "deposit"),
        ]
    }

    fn vault() -> crate::evm::Vault {
        crate::evm::Vault::new(&json!({ "via": "eth_rpc_module", "chain_id": 11155111 }), [2; 20], 0).unwrap()
    }

    #[test]
    fn the_margin_covers_the_gap_that_reverted_an_approved_cap() {
        // Measured (the first review's run): setTokenCap estimated 48,564, needed 48,273 + 2,525 when mined a second later.
        assert_eq!(margin(48_564), 48_564 + 12_141 + 20_000);
        assert!(margin(48_564) > 48_273 + 2_525);
        // The ETH deposit (estimate = used = 200,572): a second-later branch write (~17k) still fits.
        assert!(margin(200_572) >= 200_572 + 17_100);
        assert_eq!(margin(u64::MAX / 2), TX_GAS_CAP.max(u64::MAX / 2));
        assert_eq!(margin(20_000_000), 20_000_000, "never below the estimate");
        assert_eq!(margin(16_000_000), TX_GAS_CAP);
    }

    #[test]
    fn every_call_is_sent_with_a_string_gas_limit_above_the_estimate_or_the_callers_own() {
        let _g = serial();
        fake::install();
        *fake::VERDICT.lock().unwrap() = "broadcast";
        let mut c = calls();
        c[0].gas_limit = Some(60_000);
        let req = request(11155111, "0x0000000000000000000000000000000000000003", &c, "Ferrata deposit");
        let sent = send(&req, Duration::from_secs(1)).unwrap();
        assert!(sent["requestId"].as_str().unwrap().starts_with("req-"));
        let last = fake::SENT.lock().unwrap().last().cloned().unwrap();
        assert_eq!(last["chainId"], 11155111);
        assert_eq!(last["calls"][1]["data"], "0xdead");
        // The caller's own limit verbatim; the other the estimate + 25% + 20000; both as decimal strings.
        assert_eq!(last["calls"][0]["gasLimit"], json!("60000"));
        assert_eq!(last["calls"][1]["gasLimit"], json!(margin(48_564).to_string()));
        assert_eq!(sent["gas"][1]["estimate"], 48_564);
        assert!(sent["gas"][0]["source"].as_str().unwrap().starts_with("given"));
    }

    #[test]
    fn a_bundle_is_sent_and_every_receipt_checked() {
        let _g = serial();
        fake::install();
        *fake::VERDICT.lock().unwrap() = "broadcast";
        *fake::STATUS.lock().unwrap() = "0x1";
        let mut v = vault();
        let s = through_stack(&mut v, 11155111, "0x0000000000000000000000000000000000000003", &calls(), "Ferrata deposit", true,
                              Duration::from_secs(1), Duration::from_secs(1), json!({})).unwrap();
        assert_eq!(s.receipts.len(), 2);
        assert!(s.reverted.is_none());
        assert_eq!(s.record["receipts"][1]["status"], "success");
        assert_eq!(s.record["receipts"][1]["gas_limit"], margin(48_564));
        // A forced revert (status 0, e.g. a too-low limit): reported, naming the call, the block, the gas.
        *fake::STATUS.lock().unwrap() = "0x0";
        let s = through_stack(&mut v, 11155111, "0x0000000000000000000000000000000000000003", &calls(), "Ferrata deposit", true,
                              Duration::from_secs(1), Duration::from_secs(1), json!({})).unwrap();
        let why = s.reverted.clone().unwrap();
        assert!(why.starts_with("reverted: call 1 of 2 (approve)"), "{why}");
        assert!(why.contains("block 8") && why.contains("using 48273 gas"), "{why}");
        assert_eq!(s.record["receipts"][0]["status"], "reverted");
        *fake::STATUS.lock().unwrap() = "0x1";
        // Refused at the approver: nothing broadcast, said so.
        *fake::VERDICT.lock().unwrap() = "rejected";
        let e = through_stack(&mut v, 11155111, "0x0000000000000000000000000000000000000003", &calls(), "x", true, Duration::from_secs(1),
                              Duration::from_secs(1), json!({}))
            .err().unwrap();
        assert!(e.starts_with("rejected"), "{e}");
        *fake::VERDICT.lock().unwrap() = "broadcast";
    }

    /// A receipt not seen in time is not a failure (review: "not deposited" invited a second deposit): the send is
    /// reported broadcast and not yet mined, naming the transaction; a read that fails is read again; a `stuck` send
    /// counts as broadcast; a `failed` one names the calls that had already left.
    #[test]
    fn a_receipt_not_seen_in_time_is_broadcast_not_yet_mined_never_failed() {
        let _g = serial();
        fake::install();
        *fake::VERDICT.lock().unwrap() = "broadcast";
        *fake::STATUS.lock().unwrap() = "0x1";
        let from = "0x0000000000000000000000000000000000000003";
        let mut v = vault();
        *fake::MINED.lock().unwrap() = false;
        let s = through_stack(&mut v, 11155111, from, &calls(), "Ferrata deposit", true, Duration::from_secs(1), Duration::from_millis(30), json!({}))
            .expect("not an error");
        assert!(s.reverted.is_none() && s.receipts.is_empty());
        assert_eq!(s.unconfirmed.len(), 2);
        let why = s.unconfirmed_text("this deposit").unwrap();
        assert!(why.starts_with(&format!("broadcast, not yet mined: transaction {}", s.unconfirmed[0])), "{why}");
        assert!(why.contains("Do not send this deposit again"), "{why}");
        assert_eq!(s.record["receipts"][1]["status"], "not yet mined");
        *fake::MINED.lock().unwrap() = true;
        // Two failed reads, then the node answers: read again, not aborted.
        *fake::FAILING_READS.lock().unwrap() = 2;
        let s = through_stack(&mut v, 11155111, from, &calls(), "Ferrata deposit", true, Duration::from_secs(1), Duration::from_secs(5), json!({})).unwrap();
        assert_eq!((s.receipts.len(), s.unconfirmed.len()), (2, 0));
        assert!(s.unconfirmed_text("x").is_none());
        // `stuck`: the node did not answer the broadcast; its receipts are read like a broadcast's.
        *fake::VERDICT.lock().unwrap() = "stuck";
        *fake::MINED.lock().unwrap() = false;
        let s = through_stack(&mut v, 11155111, from, &calls(), "Ferrata deposit", true, Duration::from_secs(1), Duration::from_millis(30), json!({})).unwrap();
        assert_eq!(s.unconfirmed.len(), 2);
        assert_eq!(s.record["final"], "stuck");
        *fake::MINED.lock().unwrap() = true;
        // `failed` after the approve left: the error names it.
        *fake::VERDICT.lock().unwrap() = "failed";
        let e = through_stack(&mut v, 11155111, from, &calls(), "Ferrata deposit", true, Duration::from_secs(1), Duration::from_secs(1), json!({}))
            .err().unwrap();
        assert!(e.starts_with("failed: the node refused the second transaction"), "{e}");
        assert!(e.contains("already broadcast before it failed: call 1 of 2 (call 0), transaction 0x"), "{e}");
        *fake::VERDICT.lock().unwrap() = "broadcast";
        assert_eq!(receipt_wait(&json!({ "receipt_timeout_s": 12 })), Duration::from_secs(12));
        assert_eq!(receipt_wait(&json!({})), RECEIPT_WAIT);
    }

    /// Review L1 (the caller chose the vault): the module sends only to a deployment the person named
    /// (`FERRATA_DEPLOYMENTS`); another vault, another chain, or none named is refused before anything reaches
    /// tx_sender_module; the purpose the approver shows names the vault and the chain.
    #[test]
    fn only_a_pinned_deployment_is_sent_to_and_the_purpose_names_it() {
        let _g = serial();
        fake::install();
        *fake::VERDICT.lock().unwrap() = "broadcast";
        *fake::STATUS.lock().unwrap() = "0x1";
        let from = "0x0000000000000000000000000000000000000003";
        let n0 = fake::SENT.lock().unwrap().len();
        let mut other = crate::evm::Vault::new(&json!({ "via": "eth_rpc_module", "chain_id": 11155111 }), [7; 20], 0).unwrap();
        let e = through_stack(&mut other, 11155111, from, &calls(), "Ferrata deposit", true, Duration::from_secs(1), Duration::from_secs(1), json!({}))
            .err().unwrap();
        assert!(e.starts_with(&format!("refused before anything was asked: vault 0x{} on chain 11155111 is not a deployment", "07".repeat(20))), "{e}");
        let e = through_stack(&mut vault(), 5, from, &calls(), "Ferrata deposit", true, Duration::from_secs(1), Duration::from_secs(1), json!({}))
            .err().unwrap();
        assert!(e.contains("on chain 5 is not a deployment this module may send to (FERRATA_DEPLOYMENTS: 11155111:0x0202"), "{e}");
        // The module-level deposit naming another vault: refused first, before any read of that address.
        let req = json!({ "vault": format!("0x{}", "07".repeat(20)), "eth": { "via": "eth_rpc_module", "chain_id": 11155111 },
                          "depositor": from, "token": format!("0x{}", "00".repeat(20)), "denom_index": 0,
                          "send": { "via": TX_SENDER, "from": from, "chain_id": 11155111 } });
        let e = crate::user::deposit_and_send(&req).unwrap_err();
        assert!(e.starts_with("refused before anything was asked: vault 0x0707"), "{e}");
        assert_eq!(fake::SENT.lock().unwrap().len(), n0, "nothing reached tx_sender_module");
        // The pinned deployment: sent; the purpose line names it.
        through_stack(&mut vault(), 11155111, from, &calls(), "Ferrata deposit", true, Duration::from_secs(1), Duration::from_secs(1), json!({})).unwrap();
        let last = fake::SENT.lock().unwrap().last().cloned().unwrap();
        assert_eq!(last["purpose"], json!(format!("Ferrata deposit (vault 0x{} on chain 11155111)", "02".repeat(20))));
        // None named: nothing is sent anywhere.
        *fake::DEPLOYMENTS.lock().unwrap() = None;
        let e = through_stack(&mut vault(), 11155111, from, &calls(), "x", true, Duration::from_secs(1), Duration::from_secs(1), json!({})).err().unwrap();
        assert!(e.contains("no deployment is pinned") && e.contains("FERRATA_DEPLOYMENTS=\"<chain id>:<vault>\""), "{e}");
        // The list's form.
        assert_eq!(parse_deployments(&format!(" 1:0x{}, 11155111:0x{} ,", "02".repeat(20), "07".repeat(20))).unwrap(), vec![(1, [2; 20]), (11155111, [7; 20])]);
        assert!(parse_deployments("11155111").is_err());
        assert!(parse_deployments(&format!("sepolia:0x{}", "02".repeat(20))).is_err());
        assert!(parse_deployments("1:0x1234").is_err());
        fake::install();
    }

    #[test]
    fn a_request_still_open_at_the_timeout_is_withdrawn() {
        let _g = serial();
        fake::install();
        *fake::VERDICT.lock().unwrap() = "awaitingApproval";
        let sent = send(&request(1, "0x0000000000000000000000000000000000000003", &calls(), "x"), Duration::from_secs(1)).unwrap();
        let id = sent["requestId"].as_str().unwrap().to_string();
        let e = wait(&id, Duration::from_millis(10), |_| {}).unwrap_err();
        assert!(e.contains("withdrawn (cancel_send)"), "{e}");
        assert_eq!(fake::CANCELLED.lock().unwrap().last(), Some(&id));
        *fake::VERDICT.lock().unwrap() = "broadcast";
    }
}
