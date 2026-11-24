//! `wallet(request)`: the mini-app's calls, run inside `ferrata_bridge_module` (the view's module transport).
//!
//! The view (`modules/ferrata_bridge_ui/qml/Main.qml`) sends `{"method", "params"}` with the methods it also
//! sends to the devnet backend (`open`, `info`, `registry`, `price`, `deposit`, `scan`, `claim`, `burn`,
//! `release`, `set_endpoints`). Each is answered here with the module's own methods (the `ferrata` tool's
//! code): `deposit_of` (the commitment and the vault call), `recover_of` (the wallet's deposits and burns
//! from its seed), `wallet::claim_with_state` (which imported root, the program's checks, the
//! `ClaimPrivately` message and its accounts), `burn_of` (the note and the `Burn` message),
//! `wallet::release_by_rule` (the newest accepted root whose release delay has passed, the journal),
//! `wallet::registry_of`. The chains' state comes from, and the signing and sending go to, a chain service
//! at `params.chain` of `open`: the stand-in for Logos Core's own modules (`eth_rpc_module`,
//! `keystore_module`/`tx_sender_module`, `lez_core`), which are not available here. On a LEZ sequencer
//! that is `e2e/seqnet`'s `seqnet-chain` (LEZ's wallet library and the `ferrata` tool's Ethereum reads);
//! its methods are the mini-app devnet's `module.*` ones. Plain `http://` only, as the module's other reads.
//!
//! `eth` routes the session's Ethereum side through Logos Core's own modules instead of the chain service: every vault
//! read through `eth_rpc_module` (`evm.rs`), and the user's deposit and release and the admin's vault calls through
//! the send stack (`tx_sender_module` -> one approval in the Logos signer -> `keystore_module` signs; `send.rs`), each
//! a job the view polls while the person approves. The chain service then answers for LEZ alone (`lez_only`) and
//! stands in for the release's prover only (the vault's verifier is the tests' mock: no Groth16 wrap here).
//!
//! `set_endpoints` (the view's "Apply and test", RFP-022 U.9) switches the endpoints where the connections are held: the
//! LEZ sequencer at the chain service (`module.set_endpoints`), the Ethereum RPC at `eth_rpc_module` (its endpoint for
//! the session's chain) on the `eth` path or at the chain service on the plain path. A new endpoint is used only once
//! it has answered for this deployment; otherwise the endpoints in use are kept (`set_endpoints` below).
//!
//! Every reply is `{"ok": true, "result"}` or `{"ok": false, "error"}`, the envelope the view reads.
//!
//! The network session (`open_network`, `net.rs`) is the mini-app's own (review H1): `seal_key` and `open_network` are
//! answered only to the view, `ferrata_bridge_ui`, as Logos Core names the caller of a call (`wallet_as`: from Logos
//! protocol 0.6 the host tells a module who calls it, by the token the caller presented; Basecamp 0.4.0-rc.2 gives each
//! mini-app a token of its own; capability_module checks only that the name is a registered module, so the name is
//! token-bound, not proven), and once a session is open every call must also carry a MAC under the key the view agreed
//! with this module when it opened it (`seal.rs`): no other caller, and no other view, can use it or replace it, and a
//! call is never accepted twice. A LEZ transaction is built for the person's approval (`claim`, `burn` answer
//! `to_approve`) and signed only by `confirm` with its id, as approved. Every reply names the session's public account
//! /0 (`session_account`), which the view checks against the one it opened.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use borsh::BorshDeserialize as _;
use ferrata_bridge_core::Hash;
use ferrata_lez_bridge_wire::{self as bw, BridgeState};
use ferrata_wallet::AccountId;
use serde_json::{json, Value};

use crate::{fixed, hex, unhex, wallet as w};

const ETH: &str = "0x0000000000000000000000000000000000000000";

struct Session {
    chain: String,
    s: Value,
    calls: u64,
}

static SESSION: Mutex<Option<Session>> = Mutex::new(None);

/// The session's Ethereum path when it goes through Logos Core's modules (`wallet {"method": "eth"}`): `eth` (the
/// `Rpc` path: `{"via": "eth_rpc_module", "chain_id"}`, or `{"via": "http", "rpc_url"}` natively), `chain_id`,
/// `from_block`, and the accounts the send stack sends from: `user` (the depositor), `release_sender`, `admin`,
/// `freeze_authority` (each held by `keystore_module`; the module holds no key).
static ETH_PATH: Mutex<Option<Value>> = Mutex::new(None);

fn eth_path() -> Option<Value> {
    ETH_PATH.lock().ok().and_then(|g| g.clone())
}

/// `wallet {"method": "eth", "params": {...}}`: the Ethereum path (`{"via": "plain"}`: back to the chain service).
fn configure_eth(params: &Value) -> Result<Value, String> {
    let via = params["via"].as_str().unwrap_or("");
    if via == "plain" {
        *ETH_PATH.lock().map_err(|_| "poisoned")? = None;
        return Ok(json!({ "via": "the chain service (seqnet-chain)" }));
    }
    let s = SESSION.lock().ok().and_then(|g| g.as_ref().map(|s| s.s.clone())).ok_or("open the session first")?;
    let eth = match via {
        "eth_rpc_module" => json!({ "via": "eth_rpc_module", "chain_id": params["chain_id"].as_u64().ok_or("chain_id")? }),
        "http" => json!({ "via": "http", "rpc_url": params["rpc_url"].as_str().ok_or("rpc_url")? }),
        other => return Err(format!("eth.via {other:?}: eth_rpc_module (Logos Core), http (native) or plain")),
    };
    let chain_id = params["chain_id"].as_u64().or_else(|| s["chain_id"].as_u64()).ok_or("chain_id")?;
    if Some(chain_id) != s["chain_id"].as_u64() {
        return Err(format!("chain {chain_id} is not the session's ({})", s["chain_id"]));
    }
    let depositor = s["depositor"].as_str().ok_or("the session has no depositor")?.to_lowercase();
    let user = params["user"].as_str().map(str::to_lowercase).unwrap_or_else(|| depositor.clone());
    if user != depositor {
        return Err(format!("the user's account {user} is not the session's depositor {depositor}: deposits name their depositor"));
    }
    // On this path the release is sent from an account of the user's own (no relayer): never the depositor, whose
    // address would then link the deposit to the release on Ethereum.
    let release_sender = params["release_sender"].as_str().map(str::to_lowercase)
        .ok_or("release_sender: the account the release is sent from on this path (another of the user's accounts: not the depositor)")?;
    if release_sender == user {
        return Err(format!("release_sender {release_sender} is the depositor: one address would link the deposit and the release on Ethereum"));
    }
    let cfg = json!({
        "eth": eth,
        "chain_id": chain_id,
        "from_block": params["from_block"].as_u64().unwrap_or(0),
        "user": user,
        "release_sender": release_sender,
        "admin": params["admin"],
        "freeze_authority": params["freeze_authority"],
    });
    // One read through the path, to answer with what it sees (the chain id and the vault's deposits).
    let vault: [u8; 20] = fixed(&s["vault"], "vault")?;
    let mut v = crate::evm::Vault::new(&cfg["eth"], vault, 0)?;
    let seen = json!({ "chain_id": v.chain_id()?, "deposits": v.next_leaf_index(None)?, "block": v.block_number()? });
    if seen["chain_id"].as_u64() != Some(chain_id) {
        return Err(format!("the Ethereum path answers chain {}, not {chain_id}", seen["chain_id"]));
    }
    *ETH_PATH.lock().map_err(|_| "poisoned")? = Some(cfg.clone());
    Ok(json!({ "configured": cfg, "seen": seen, "reads": v.report(), "sends": "tx_sender_module (one approval per send in the Logos signer)" }))
}

/// The caller of a call made natively (tests, the `call` example): not through Logos Core.
pub const NATIVE: &str = "native";
/// The callers the network session answers: the mini-app's view as Logos Core names it, and a native caller.
const VIEW_CALLERS: [&str; 2] = ["module:ferrata_bridge_ui", NATIVE];

/// `wallet(request)`: one call of the view, made natively.
pub fn wallet(request: &str) -> String {
    wallet_as(NATIVE, request)
}

/// `wallet(request)` as Logos Core delivers it: `caller` is who calls (`module:<name>`, `host`, `unknown`; the module's
/// glue reads it from the Logos SDK). Every reply is `{"ok", "result"|"error"}`, and names the open network session's
/// public account /0 (`session_account`).
pub fn wallet_as(caller: &str, request: &str) -> String {
    crate::net::expire_idle();
    let result = match serde_json::from_str::<Value>(request) {
        Ok(r) => route(caller, &r),
        Err(e) => Err(format!("invalid request: {e}")),
    };
    let mut reply = match result {
        Ok(v) => json!({ "ok": true, "result": v }),
        Err(e) => json!({ "ok": false, "error": e }),
    };
    if let Some(a) = crate::net::account0() {
        reply["session_account"] = json!(a);
    }
    reply.to_string()
}

/// Who may call what (review H1): with a network session open, only its view, each call with a MAC under the session's
/// key and a counter never used before; without one, `seal_key` and `open_network` only from the view.
fn route(caller: &str, r: &Value) -> Result<Value, String> {
    let view = VIEW_CALLERS.contains(&caller);
    if crate::net::is_open() {
        if !view {
            return Err(format!("refused: this module's network session belongs to the Ferrata Bridge view that opened it; this call came from {caller}"));
        }
        let body = r["call"].as_str().ok_or("refused: a network session is open: each call must carry its counter and MAC (only the view that opened the session holds its key)")?;
        let n = r["n"].as_u64().ok_or("refused: the call's counter n")?;
        let tag = unhex(r["mac"].as_str().ok_or("refused: the call's mac")?)?;
        crate::net::verify_call(n, body, &tag)?;
        let inner: Value = serde_json::from_str(body).map_err(|e| format!("invalid call: {e}"))?;
        let method = inner["method"].as_str().unwrap_or("");
        // Only here, in a verified call of the session's view, may the bridge's functions use the session's keys.
        return crate::net::within(crate::net::CALL_LIMIT, || crate::with_session_keys(|| dispatch(method, &inner["params"], caller)));
    }
    let method = r["method"].as_str().unwrap_or("");
    if matches!(method, "seal_key" | "open_network") {
        if !view {
            return Err(format!("refused: a network session is opened by the Ferrata Bridge view (ferrata_bridge_ui) only; this call came from {caller}"));
        }
        return crate::net::within(crate::net::CALL_LIMIT, || dispatch(method, &r["params"], caller));
    }
    dispatch(method, &r["params"], caller)
}

fn dispatch(method: &str, params: &Value, caller: &str) -> Result<Value, String> {
    // An open network session is not replaced, by another network session or a chain service's (review H1).
    if crate::net::is_open() && matches!(method, "open_network" | "seal_key" | "open") {
        return Err(crate::net::ALREADY_OPEN.into());
    }
    if method == "seal_key" {
        // The module's one-time key for the view's sealed phrase (seal.rs), and who Logos Core says is calling. The
        // process stops being dumpable now, before any secret arrives (review round 2, MF3).
        crate::net::not_dumpable();
        let (key_id, public) = crate::seal::new_key()?;
        // 32 bytes of the operating system's randomness for the view's own secret: QML has no cryptographic generator
        // (Math.random is Qt's one process-wide MT19937), and only the caller receives this reply (review round 2, MF2).
        let mut entropy = [0u8; 32];
        getrandom::getrandom(&mut entropy).map_err(|e| format!("no randomness from the operating system: {e}"))?;
        return Ok(json!({ "key_id": key_id, "public_key": hex(&public), "entropy": hex(&entropy), "scheme": crate::seal::SCHEME, "caller": caller }));
    }
    // The jobs (jobs.rs) answer from the module's memory, never from the chains or lez_core.
    if method == "job" {
        return crate::jobs::get(params);
    }
    if method == "jobs" {
        // The view asks this first, on load: with it, whether Qt's debug logging is on in this process (Basecamp's
        // environment), which writes every key typed in Basecamp's window to its log: the view then warns before the
        // phrase is typed and takes it by paste only (review M1).
        let mut v = crate::jobs::list();
        v["qt_debug_logging"] = json!(qt_debug_logging());
        return Ok(v);
    }
    // While a job runs, a call that would call lez_core or reread the chains on the module's Qt thread is refused at
    // once (jobs.rs: it would wait there for lez_core, busy proving). `info` and `price` read through the chain service
    // and, on the `eth` path, eth_rpc_module, never lez_core; `deposit` sends through the chain service, or is itself a
    // job on the `eth` path; `claim` and `burn` answer the running job, or are refused. `set_endpoints` is refused.
    // `close` ends the session even while a job runs (the job then finds no session and fails, sending nothing more).
    if method == "close" {
        return Ok(close());
    }
    if let Some(job) = crate::jobs::active() {
        // A deposit or a vault admin call through the send stack is itself a job: started again it answers that job,
        // and jobs.rs refuses it while another job runs.
        if !matches!(method, "info" | "price" | "deposit" | "claim" | "burn" | "admin.vault" | "admin.set_cap" | "admin.lez" | "eth_report") {
            return Err(crate::jobs::busy(method, &job));
        }
    }
    if method == "open" {
        return open(params);
    }
    // The network session (net.rs): the mini-app against a test network's public endpoints, keys from the phrase.
    if method == "open_network" {
        return open_network(params);
    }
    if network_session() {
        // A private claim or burn is refused at once in the network session (it needs a proof this module does not
        // make), rather than started as a job that fails.
        if matches!(method, "claim" | "burn") && params["private"] != false {
            return Err(crate::net::PRIVATE_NEEDS_PROOF.into());
        }
        if let Some(why) = match method {
            "lez" | "eth" | "lez_probe" => Some("this session reads and sends through the network's own endpoints"),
            "set_endpoints" => Some("this session's endpoints are the network's: close it and open it again with another Ethereum endpoint"),
            "admin.vault" | "admin.set_cap" | "admin.lez" => Some("the bridge's admin calls are its operator's, not a user's"),
            _ => None,
        } {
            return Err(format!("{method}: {why}"));
        }
    }
    if method == "lez" {
        // The session's LEZ path: Logos Core's lez_core (reads, the private holding, private sends) or plain.
        let private0 = SESSION.lock().ok().and_then(|g| g.as_ref().and_then(|s| s.s["private_accounts"][0].as_str().map(str::to_string)));
        return crate::lez::configure(params, private0);
    }
    if method == "eth" {
        return configure_eth(params);
    }
    if method == "eth_report" {
        return Ok(json!({ "path": eth_path(), "via": if eth_path().is_some() { "Logos Core: eth_rpc_module (reads), tx_sender_module (sends)" } else { "the chain service" } }));
    }
    if method == "lez_report" {
        return Ok(crate::lez::report());
    }
    if method == "lez_probe" {
        let s = SESSION.lock().ok().and_then(|g| g.as_ref().map(|s| s.s.clone())).ok_or("open the session first")?;
        return crate::lez::probe(&s);
    }
    if method == "set_endpoints" {
        return set_endpoints(params);
    }
    let (chain, s) = {
        let mut g = SESSION.lock().map_err(|_| "the session lock is poisoned")?;
        let sess = g.as_mut().ok_or("no wallet session: open it first (wallet {\"method\":\"open\",\"params\":{\"chain\":URL}})")?;
        sess.calls += 1;
        (sess.chain.clone(), sess.s.clone())
    };
    let c = Chain { url: chain, s };
    match method {
        "faucet" if c.network() => crate::net::faucet(params["account"].as_u64().unwrap_or(0) as u32),
        "balances" if c.network() => Ok(json!({ "accounts": crate::net::lgo_balances()?, "unit": "10^-9 LGO" })),
        "deposit_plan" => c.deposit_plan(params),
        "deposit" if c.network() => Err(DEPOSITS_IN_THE_WEB_WALLET.into()),
        "release" if c.network() => c.release_handoff(params),
        // A LEZ transaction of the network session: built for the person's approval (signed by nothing yet), then
        // `confirm` with its id signs exactly that and sends it (a job: it waits for its block).
        "claim" if c.network() => c.claim(params),
        "burn" if c.network() => c.burn(params),
        "confirm" if c.network() => {
            let id = params["pending_id"].as_str().ok_or("confirm needs the pending_id of the transaction approved")?.to_string();
            let kind = crate::net::pending_kind(&id)?;
            crate::jobs::start(kind, params, move |_| crate::net::confirm(&id))
        }
        "info" => c.info(),
        "registry" => c.registry(),
        "price" => match eth_path() {
            Some(e) => c.vault(&e)?.price(&[0u8; 20], params["denomination"].as_u64().unwrap_or(0) as u8),
            None => c.post("module.price", json!({ "denomination": params["denomination"], "token": ETH })),
        },
        // Through the send stack the deposit waits for the person's approval in the Logos signer: a job, polled.
        "deposit" if eth_path().is_some() => crate::jobs::start("deposit", params, move |p| c.deposit_logos(p)),
        "deposit" => c.deposit(params),
        "scan" => c.scan(),
        // A private claim or burn proven on a CPU takes minutes: a job (jobs.rs), answered at once, polled with `job`.
        "claim" => crate::jobs::start("claim", params, move |p| c.claim(p)),
        "burn" => crate::jobs::start("burn", params, move |p| c.burn(p)),
        "release" if eth_path().is_some() => crate::jobs::start("release", params, move |p| c.release_logos(p)),
        "release" => c.release(params),
        "admin.status" if eth_path().is_some() => c.admin_status(),
        "admin.vault" if eth_path().is_some() => crate::jobs::start("admin", params, move |p| c.admin_vault(p)),
        // A LEZ admin message built by the module and sent as a public transaction (one block): a job.
        "admin.set_cap" | "admin.lez" => {
            let m = method.to_string();
            crate::jobs::start("admin_lez", params, move |p| c.admin_lez(&m, p))
        }
        m if m.starts_with("admin.") => Err(format!(
            "{m}: not on this path. The module answers admin.status and admin.vault once the session's Ethereum path goes through Logos Core (`eth`), and builds the LEZ role messages (admin.set_cap, admin.lez: SetCap, Freeze, Pause, Tick, Sunset), each sent only after a person approves it at the role's wallet. A LEZ Freeze, Pause or Tick needs a time record whose Gloas layout carries the execution block's RLP header, which the module does not fetch"
        )),
        "relay" | "checkpoint" | "advance_time" => {
            Err(format!("{method} is a devnet stand-in: on a sequencer the relayer and the checkpoint prover do it, and time passes"))
        }
        m => Err(format!("unknown method {m}")),
    }
}

// ---- the endpoints (RFP-022 U.9) ------------------------------------------------------------------------------------

const ETH_RPC_MODULE: &str = "eth_rpc_module";
/// A call about eth_rpc_module's chain registry (no network).
const REGISTRY_MS: u64 = 10_000;
/// One reading of a candidate endpoint: a closed port answers at once, a silent host does not, and Basecamp cuts a
/// view's call to a core module at 20 s.
const PROBE_MS: u64 = 6_000;

/// Who holds the session's Ethereum connection.
enum EthOwner {
    /// Logos Core's `eth_rpc_module`: its endpoint for the chain (`chains.json`, the device's own: every Logos app that
    /// reads the chain uses it). The session's `eth` path.
    Module { chain_id: u64 },
    /// The module's own HTTP client (`eth {"via": "http"}`: natively).
    Http,
    /// The chain service (the plain path).
    Service,
}

impl EthOwner {
    fn of_session() -> Self {
        match eth_path() {
            Some(e) if e["eth"]["via"] == "eth_rpc_module" => {
                EthOwner::Module { chain_id: e["eth"]["chain_id"].as_u64().or_else(|| e["chain_id"].as_u64()).unwrap_or(0) }
            }
            Some(_) => EthOwner::Http,
            None => EthOwner::Service,
        }
    }

    fn label(&self) -> String {
        match self {
            EthOwner::Module { chain_id } => format!(
                "Logos Core's eth_rpc_module, its endpoint for chain {chain_id} (this device's: every Logos app that reads chain {chain_id} uses it)"
            ),
            EthOwner::Http => "the module's own HTTP client".into(),
            EthOwner::Service => "the chain service (seqnet-chain)".into(),
        }
    }

    /// The endpoint in use: eth_rpc_module's (from its registry), the module's own, or the chain service's (`svc`: its
    /// `module.endpoints` or `module.info`).
    fn endpoint(&self, svc: &Value) -> Result<String, String> {
        match self {
            EthOwner::Module { chain_id } => module_endpoint(*chain_id),
            EthOwner::Http => Ok(eth_path().and_then(|e| e["eth"]["rpc_url"].as_str().map(str::to_string)).unwrap_or_default()),
            EthOwner::Service => Ok(svc["eth_rpc"].as_str().unwrap_or("").to_string()),
        }
    }
}

const SEQUENCER_OWNER: &str = "the chain service (seqnet-chain)";

fn short(v: &Value) -> String {
    let t = v["error"].as_str().map_or_else(|| v.to_string(), str::to_string);
    let code = v["code"].as_str().map(|c| format!("{c}: ")).unwrap_or_default();
    format!("{code}{}", t.chars().take(300).collect::<String>())
}

/// eth_rpc_module's endpoint for `chain_id`.
fn module_endpoint(chain_id: u64) -> Result<String, String> {
    let v = crate::send::call_module(ETH_RPC_MODULE, "get_chain_config", json!([chain_id]), REGISTRY_MS)?;
    v["config"]["endpoint"].as_str().map(str::to_string).ok_or_else(|| format!("eth_rpc_module holds no endpoint for chain {chain_id}: {}", short(&v)))
}

/// eth_rpc_module's endpoint for `chain_id` replaced by `url`, nothing else of the chain's record changed (its proxy,
/// verified-proxy mode and timeouts stay).
fn patch_endpoint(chain_id: u64, url: &str) -> Result<(), String> {
    let v = crate::send::call_module(ETH_RPC_MODULE, "patch_chain_endpoint", json!([chain_id, url]), REGISTRY_MS)?;
    if v["ok"] == true {
        Ok(())
    } else {
        Err(short(&v))
    }
}

/// One JSON-RPC call to `url` through eth_rpc_module's client for `chain_id` (its proxy policy), nothing stored
/// (`raw_rpc_url`); an error starts with eth_rpc_module's code (`verified_bypass`: a proof-backed read eth_rpc_module
/// sends only through the verified proxy, on a chain set to `required`).
fn probe_url(chain_id: u64, url: &str, method: &str, params: Value) -> Result<Value, String> {
    let v = crate::send::call_module(ETH_RPC_MODULE, "raw_rpc_url", json!([chain_id, url, method, params.to_string()]), PROBE_MS)?;
    if v["ok"] == true {
        Ok(v["result"].clone())
    } else {
        Err(short(&v))
    }
}

fn quantity(v: &Value, what: &str) -> Result<u64, String> {
    let t = v.as_str().ok_or_else(|| format!("{what}: not a quantity: {v}"))?;
    u64::from_str_radix(t.trim_start_matches("0x"), 16).map_err(|e| format!("{what} {t}: {e}"))
}

fn same_hex(a: &str, b: &str) -> bool {
    let n = |h: &str| h.trim().trim_start_matches("0x").to_lowercase();
    n(a) == n(b) && !n(a).is_empty()
}

/// An endpoint as the view gives it: trimmed, `http://` or `https://`; absent or empty: unchanged.
fn endpoint_param(params: &Value, key: &str) -> Result<Option<String>, String> {
    match params[key].as_str().map(str::trim).filter(|u| !u.is_empty()) {
        None => Ok(None),
        Some(u) if u.starts_with("http://") || u.starts_with("https://") => Ok(Some(u.to_string())),
        Some(u) => Err(format!("{key} {u:?}: an http:// or https:// URL")),
    }
}

/// The block the endpoint in use holds final (`finalized`), read through `call`: its number and hash.
fn final_block(mut call: impl FnMut(&str, Value) -> Result<Value, String>) -> Result<(u64, String), String> {
    let b = call("eth_getBlockByNumber", json!(["finalized", false]))?;
    let n = quantity(&b["number"], "the final block's number")?;
    let h = b["hash"].as_str().ok_or("the final block has no hash")?.to_string();
    Ok((n, h))
}

/// `anchor` (the endpoint in use's final block) the same block on the endpoint `call` reaches.
fn same_history(anchor: &(u64, String), mut call: impl FnMut(&str, Value) -> Result<Value, String>) -> Result<(), String> {
    let (n, h) = anchor;
    let b = call("eth_getBlockByNumber", json!([format!("0x{n:x}"), false]))?;
    if b.is_null() {
        return Err(format!("it has no block {n}, final on the endpoint in use: behind, or another network"));
    }
    let got = b["hash"].as_str().unwrap_or("");
    if !same_hex(got, h) {
        return Err(format!("its block {n} is {got}, not {h} as on the endpoint in use: another history (a fork, or another network)"));
    }
    Ok(())
}

/// A candidate Ethereum RPC, read without switching to it (`call` reaches it): it serves the session's chain, holds the
/// session's vault, whose `lezConfigId` is the session's configuration, and, when the endpoint in use answered
/// (`anchor`), has the block that endpoint holds final. A check of the endpoint's configuration, not of its honesty: an
/// endpoint that answers all of this falsely is not detected. A read eth_rpc_module sends only through its verified
/// proxy is left to the reading after the switch (`after_switch`).
fn check_eth(chain_id: u64, s: &Value, anchor: Option<&(u64, String)>, mut call: impl FnMut(&str, Value) -> Result<Value, String>) -> Result<Value, String> {
    let vault = s["vault"].as_str().ok_or("the session names no vault")?;
    let cid = s["config_id"].as_str().ok_or("the session names no configuration")?;
    let id = quantity(&call("eth_chainId", json!([]))?, "eth_chainId")?;
    if id != chain_id {
        return Err(format!("it serves chain {id}, not the session's chain {chain_id}"));
    }
    let mut after_switch = Vec::new();
    match call("eth_getCode", json!([vault, "latest"])) {
        Ok(code) if code.as_str().map_or(true, |c| c.trim_start_matches("0x").is_empty()) => {
            return Err(format!("chain {id} there has no contract at the session's vault {vault}: not this deployment's network"));
        }
        Ok(_) => {}
        Err(e) if e.starts_with("verified_bypass") => after_switch.push("eth_getCode"),
        Err(e) => return Err(format!("eth_getCode: {e}")),
    }
    let data = hex(&crate::evm::selector("lezConfigId()"));
    match call("eth_call", json!([{ "to": vault, "data": data }, "latest"])) {
        Ok(r) => {
            let got = r.as_str().unwrap_or("");
            if !same_hex(got, cid) {
                return Err(format!("the vault {vault} there names configuration {got}, not the session's {cid}"));
            }
        }
        Err(e) if e.starts_with("verified_bypass") => after_switch.push("lezConfigId()"),
        Err(e) => return Err(format!("lezConfigId(): {e}")),
    }
    if let Some(a) = anchor {
        match same_history(a, &mut call) {
            Ok(()) => {}
            Err(e) if e.starts_with("verified_bypass") => after_switch.push("the final block"),
            Err(e) => return Err(e),
        }
    }
    let block = call("eth_blockNumber", json!([])).ok().and_then(|b| quantity(&b, "eth_blockNumber").ok());
    Ok(json!({ "chain_id": id, "vault": vault, "config_id": cid, "block": block, "same_final_block": anchor.map(|(n, _)| n),
               "after_switch": after_switch }))
}

/// The session's Ethereum side read again through its own path (eth_rpc_module's route, for the session's chain) after
/// a switch, with the block the old endpoint held final (`anchor`).
fn recheck_eth(s: &Value, anchor: Option<&(u64, String)>) -> Result<Value, String> {
    let e = eth_path().ok_or("the session has no Ethereum path through Logos Core")?;
    let session_chain = s["chain_id"].as_u64().ok_or("the session names no chain")?;
    if let Some(route_chain) = e["eth"]["chain_id"].as_u64() {
        if route_chain != session_chain {
            return Err(format!("the route reads eth_rpc_module's record for chain {route_chain}, not the session's chain {session_chain}"));
        }
    }
    let vault: [u8; 20] = fixed(&s["vault"], "vault")?;
    let mut v = crate::evm::Vault::new(&e["eth"], vault, 0)?;
    let id = v.chain_id()?;
    if id != session_chain {
        return Err(format!("it serves chain {id}, not the session's chain {session_chain}"));
    }
    let cid = hex(&v.getter("lezConfigId")?);
    if !same_hex(&cid, s["config_id"].as_str().unwrap_or("")) {
        return Err(format!("the vault there names configuration {cid}, not the session's {}", s["config_id"]));
    }
    if let Some(a) = anchor {
        same_history(a, |m, p| v.rpc.call(m, p))?;
    }
    let block = v.block_number()?;
    Ok(json!({ "chain_id": id, "config_id": cid, "block": block, "reads": v.report() }))
}

/// `set_endpoints {eth_rpc?, sequencer?}`, the view's "Apply and test" (RFP-022 U.9), through the modules that hold the
/// connections: the LEZ sequencer is the chain service's; the Ethereum RPC is Logos Core's eth_rpc_module's endpoint for
/// the session's chain when the session reads Ethereum through it (`eth`), else the chain service's. Every endpoint
/// given that is not the one in use is read first without switching (the chain service's `module.set_endpoints` dry
/// run; `eth_rpc_module.raw_rpc_url`). Only when all of them answer for this deployment does anything switch, and an
/// eth_rpc_module endpoint is read again through that module's own route afterwards: if that fails, both endpoints are
/// put back. A refused switch keeps the endpoints in use and names them. While a job runs it is refused (dispatch).
fn set_endpoints(params: &Value) -> Result<Value, String> {
    let (url, s) = {
        let g = SESSION.lock().map_err(|_| "the session lock is poisoned")?;
        let sess = g.as_ref().ok_or("no wallet session: open it first (wallet {\"method\":\"open\",\"params\":{\"chain\":URL}})")?;
        (sess.chain.clone(), sess.s.clone())
    };
    let c = Chain { url, s: s.clone() };
    let eth_want = endpoint_param(params, "eth_rpc")?;
    let seq_want = endpoint_param(params, "sequencer")?;
    let svc = c.post("module.endpoints", json!({}))?;
    let seq_cur = svc["sequencer"].as_str().unwrap_or("").to_string();
    let owner = EthOwner::of_session();
    // The Ethereum path names its chain; eth_rpc_module's record of that chain is what would be switched. Open keeps the
    // path only for the same deployment, so this holds; refused rather than switch another chain's record (M-1).
    if let EthOwner::Module { chain_id: path_chain } = &owner {
        if Some(*path_chain) != s["chain_id"].as_u64() {
            return Err(format!(
                "not switched: the session's Ethereum path reads eth_rpc_module's record for chain {path_chain}, but the session is on chain {}: configure the path again (wallet eth)",
                s["chain_id"]
            ));
        }
    }
    let eth_cur = owner.endpoint(&svc)?;
    let kept = |why: &str| format!("not switched: {why}. Kept: Ethereum RPC {eth_cur} ({}), LEZ sequencer {seq_cur} ({SEQUENCER_OWNER})", owner.label());
    let eth_new = eth_want.filter(|u| *u != eth_cur);
    let seq_new = seq_want.filter(|u| *u != seq_cur);
    if seq_new.is_some() && crate::lez::active() {
        return Err(kept(
            "the session's LEZ goes through Logos Core's lez_core, whose wallet holds its own connection (the sequencer its \
             wallet config names): turn that path off first (wallet {\"method\": \"lez\", \"params\": {\"via\": \"plain\"}})",
        ));
    }
    let svc_eth = if matches!(owner, EthOwner::Service) { eth_new.clone() } else { None };

    // 1. Every new endpoint read, nothing switched.
    let mut refused = Vec::new();
    let mut checks = json!({});
    if seq_new.is_some() || svc_eth.is_some() {
        let dry = c.post("module.set_endpoints", json!({ "sequencer": seq_new, "eth_rpc": svc_eth, "dry_run": true }))?;
        refused.extend(dry["refused"].as_array().into_iter().flatten().filter_map(|r| r.as_str().map(str::to_string)));
        checks["chain_service"] = dry["checks"].clone();
    }
    // The endpoint in use's final block, which the new one must have too (when the one in use answers: a person often
    // switches away from an endpoint that does not). The chain service reads its own (seqnet-chain).
    let mut anchor = None;
    if let (Some(_), Some(e)) = (&eth_new, eth_path()) {
        let read = crate::rpc::Rpc::from_request(&json!({ "eth": e["eth"] }), PROBE_MS).and_then(|mut r| final_block(|m, p| r.call(m, p)));
        checks["eth_rpc_in_use_final"] = match &read {
            Ok((n, h)) => json!({ "number": n, "hash": h }),
            Err(e) => json!({ "error": e, "compared": false }),
        };
        anchor = read.ok();
    }
    if let Some(u) = &eth_new {
        let chain_id = s["chain_id"].as_u64().ok_or("the session names no chain")?;
        let read = match &owner {
            EthOwner::Module { chain_id: id } => Some(check_eth(chain_id, &s, anchor.as_ref(), |m, p| probe_url(*id, u, m, p))),
            EthOwner::Http => Some(
                crate::rpc::Rpc::new(u, PROBE_MS).and_then(|mut rpc| check_eth(chain_id, &s, anchor.as_ref(), |m, p| rpc.call(m, p))),
            ),
            EthOwner::Service => None,
        };
        match read {
            Some(Ok(v)) => checks["eth_rpc"] = v,
            Some(Err(e)) => refused.push(format!("the Ethereum RPC {u}: {e}")),
            None => {}
        }
    }
    if !refused.is_empty() {
        return Err(kept(&refused.join("; ")));
    }

    // 2. The switch: the chain service first (LEZ, and Ethereum on the plain path: it reads each again and switches
    //    both or neither), then eth_rpc_module's endpoint, read again through its own route.
    let mut seq_switched = false;
    if seq_new.is_some() || svc_eth.is_some() {
        let r = c.post("module.set_endpoints", json!({ "sequencer": seq_new, "eth_rpc": svc_eth }))?;
        seq_switched = r["switched"]["sequencer"] == true;
        checks["chain_service_in_use"] = r["in_use"].clone();
    }
    if let (Some(u), EthOwner::Module { chain_id }) = (&eth_new, &owner) {
        let undo = |why: String| -> String {
            let eth_back = patch_endpoint(*chain_id, &eth_cur).map_or_else(|e| format!(" (putting it back failed: {e})"), |_| String::new());
            let seq_back = if seq_switched {
                c.post("module.set_endpoints", json!({ "sequencer": seq_cur }))
                    .map_or_else(|e| format!(" (putting it back failed: {e})"), |_| String::new())
            } else {
                String::new()
            };
            format!("not switched: {why}. Put back: Ethereum RPC {eth_cur} ({}){eth_back}, LEZ sequencer {seq_cur}{seq_back}", owner.label())
        };
        if let Err(e) = patch_endpoint(*chain_id, u) {
            return Err(undo(format!("eth_rpc_module did not take {u}: {e}")));
        }
        match recheck_eth(&s, anchor.as_ref()) {
            Ok(v) => checks["eth_rpc_after"] = v,
            Err(e) => return Err(undo(format!("the Ethereum RPC {u}, read through eth_rpc_module after the switch: {e}"))),
        }
    }
    if let (Some(u), EthOwner::Http) = (&eth_new, &owner) {
        if let Ok(mut g) = ETH_PATH.lock() {
            if let Some(cfg) = g.as_mut() {
                cfg["eth"]["rpc_url"] = json!(u);
            }
        }
    }
    let eth_now = eth_new.clone().unwrap_or_else(|| eth_cur.clone());
    let seq_now = seq_new.clone().unwrap_or_else(|| seq_cur.clone());
    if let Ok(mut g) = SESSION.lock() {
        if let Some(sess) = g.as_mut() {
            sess.s["sequencer"] = json!(seq_now);
            if svc_eth.is_some() {
                sess.s["eth_rpc"] = json!(eth_now);
            }
        }
    }
    Ok(json!({
        "eth_rpc": eth_now,
        "sequencer": seq_now,
        "switched": { "eth_rpc": eth_new.is_some(), "sequencer": seq_new.is_some() },
        "before": { "eth_rpc": eth_cur, "sequencer": seq_cur },
        "via": { "eth_rpc": owner.label(), "sequencer": SEQUENCER_OWNER },
        "checks": checks,
    }))
}

/// Opens the session: the chain service's account of the wallet (its bridge seed, accounts, the
/// configuration).
fn open(params: &Value) -> Result<Value, String> {
    let url = params["chain"].as_str().ok_or("open needs `chain` (the chain service's http:// URL)")?.to_string();
    let c = Chain { url: url.clone(), s: Value::Null };
    let s = c.post("module.session", json!({}))?;
    for k in ["bridge_seed", "chain_id", "vault", "config_id", "program", "depositor", "private_accounts", "public_accounts"] {
        if s[k].is_null() {
            return Err(format!("the chain service's session has no `{k}`"));
        }
    }
    // The session's Ethereum path (`eth`) and LEZ path (`lez`) were configured for one deployment: its chain, its vault,
    // the user's accounts there. Opened on another deployment (another chain service, or another configuration), the
    // session drops them, so that nothing reads, sends or switches an endpoint on the old one's behalf (review M-1:
    // a kept path switched eth_rpc_module's endpoint of the old chain to an RPC of the new one). The same deployment
    // opened again (the view opened again) keeps them.
    let same = SESSION.lock().map_err(|_| "the session lock is poisoned")?.as_ref().is_some_and(|old| {
        old.chain == url && ["chain_id", "vault", "config_id", "program", "depositor"].iter().all(|k| old.s[*k] == s[*k])
    });
    let mut reset = Vec::new();
    if !same {
        if ETH_PATH.lock().map_err(|_| "the eth path lock is poisoned")?.take().is_some() {
            reset.push("eth");
        }
        if crate::lez::active() {
            crate::lez::configure(&json!({ "via": "plain" }), None)?;
            reset.push("lez");
        }
    }
    crate::net::close();
    let summary = json!({
        "chain": url,
        "chain_id": s["chain_id"],
        "vault": s["vault"],
        "config_id": s["config_id"],
        "program": s["program"],
        "depositor": s["depositor"],
        "proofs": s["proofs"],
        "module": "ferrata_bridge_module wallet session",
        "paths_reset": reset,
    });
    *SESSION.lock().map_err(|_| "the session lock is poisoned")? = Some(Session { chain: url, s, calls: 0 });
    Ok(summary)
}

/// Whether the open session is a network session (`net.rs`).
fn network_session() -> bool {
    SESSION.lock().ok().is_some_and(|g| g.as_ref().is_some_and(|s| s.chain.starts_with("network:")))
}

/// `open_network`: the mini-app against a Ferrata test network's public endpoints (net.rs: what is checked, where the
/// phrase's keys live). The phrase comes sealed to the module's one-time key (`seal_key`; `key_id`, `view_key`,
/// `phrase_sealed`), never in clear (review M1). Its Ethereum side is the session's `eth` path over the endpoint the
/// person chose: every vault read here; no Ethereum send in this session (deposits are made in the web wallet).
fn open_network(params: &Value) -> Result<Value, String> {
    if params.get("phrase").is_some_and(|p| !p.is_null()) {
        return Err("refused: open_network takes the phrase sealed to the module's one-time key (seal_key, then key_id, view_key and \
                    phrase_sealed), never in clear: Logos Core's IPC carries a call's arguments in clear, and Qt's debug logging \
                    writes them to Basecamp's log".into());
    }
    if params["generate"] == true && qt_debug_logging() {
        return Err("refused: no new phrase is made here while Qt debug logging is on in this Basecamp (review round 2, MF2): \
                    make one in the network's web wallet (/wallet/, its browser's cryptographic generator), then open with it".into());
    }
    let key_id = params["key_id"].as_u64().ok_or("open_network needs key_id (from seal_key)")?;
    let view_pk: [u8; 32] = fixed(&params["view_key"], "view_key")?;
    let (channel, _) = crate::seal::agree(key_id, &view_pk)?;
    let (s, mut summary) = crate::net::open(params, channel)?;
    let eth = json!({
        "eth": { "via": "http", "rpc_url": s["eth_rpc"] },
        "chain_id": s["chain_id"],
        "from_block": s["from_block"],
        "user": s["depositor"].as_str().map(str::to_lowercase),
        "release_sender": null,
        "network_session": true,
    });
    *ETH_PATH.lock().unwrap_or_else(|p| p.into_inner()) = Some(eth);
    if crate::lez::active() {
        crate::lez::configure(&json!({ "via": "plain" }), None)?;
    }
    let chain = format!("network:{}", s["network"].as_str().unwrap_or(""));
    *SESSION.lock().unwrap_or_else(|p| p.into_inner()) = Some(Session { chain, s, calls: 0 });
    if qt_debug_logging() {
        if let Some(c) = summary["checks"].as_array_mut() {
            c.push(json!(QT_LOG_NOTE));
        }
    }
    Ok(summary)
}

/// What open_network says when Qt's debug logging is on (review round 2, L1 and L3: the phrase field's keys are kept from
/// Qt's key logging; the calls are not).
const QT_LOG_NOTE: &str = "note: Qt debug logging is on in this Basecamp (QT_LOGGING_RULES or a qtlogging.ini): its log holds this \
session's calls and answers in clear (addresses, account ids, positions, balances, amounts) and keys typed in Basecamp's other \
fields; never the phrase or a key (the phrase is sealed, and its field's keys are kept from Qt's key logging)";

/// Whether Qt's debug logging looks enabled in this process's environment for Qt's own categories (`*`, `qt.*`): the
/// remote-objects transport's (what crosses it is sealed or MAC'd; the answers are private but not secret) and the
/// key handling's (`qt.gui.shortcutmap`, `qt.qpa.keymapper`: every key typed in Basecamp's window, with its text).
fn qt_debug_logging() -> bool {
    let on = |rules: &str| rules.lines().flat_map(|l| l.split(';')).any(|r| {
        let r = r.trim().to_ascii_lowercase().replace(' ', "");
        r.ends_with("=true") && (r.contains("debug") || !r.contains(".critical") && !r.contains(".warning") && !r.contains(".info"))
            && (r.starts_with('*') || r.starts_with("qt."))
    });
    std::env::var("QT_LOGGING_RULES").is_ok_and(|v| on(&v))
        || std::env::var("QT_LOGGING_CONF").ok().and_then(|p| std::fs::read_to_string(p).ok()).is_some_and(|v| on(&v))
        || std::env::var("HOME").ok().and_then(|h| std::fs::read_to_string(format!("{h}/.config/QtProject/qtlogging.ini")).ok()).is_some_and(|v| on(&v))
}

/// `close`: ends the session; a network session's keys are wiped as they are dropped, and its jobs' records (amounts,
/// recipients, transactions) are forgotten.
fn close() -> Value {
    let was = SESSION.lock().unwrap_or_else(|p| p.into_inner()).take().map(|s| s.chain.starts_with("network:"));
    *ETH_PATH.lock().unwrap_or_else(|p| p.into_inner()) = None;
    let net = crate::net::close();
    if net {
        crate::jobs::forget();
    }
    json!({ "closed": was.is_some() || net, "network_session": was == Some(true) || net, "keys": "wiped" })
}

/// A deposit in the network session (review M4): Basecamp 0.4.0-rc.2 has no Logos signer for Sepolia, so the module
/// sends none; the person deposits in the web wallet with the same phrase.
const DEPOSITS_IN_THE_WEB_WALLET: &str = "refused: deposits are made in the network's web wallet (its /wallet/ page, Bridge \
tab) with this phrase, to one of its public accounts: this Basecamp has no Logos signer for Sepolia (Logos Core's \
tx_sender_module, keystore_module and a signer), so this module cannot have one signed; claim and burn it here afterwards";

struct Chain {
    url: String,
    s: Value,
}

impl Chain {
    fn network(&self) -> bool {
        self.url.starts_with("network:")
    }

    /// A deposit planned to its approval, nothing sent: the commitment C, the vault call and what it costs, for the
    /// person to check in the wallet's prompt (and then in the Logos signer, which signs it). The vault is asked to
    /// simulate it from the depositor first (`preflight`); its refusal is shown, not hidden.
    fn deposit_plan(&self, params: &Value) -> Result<Value, String> {
        let e = eth_path().ok_or("deposit_plan: the session's Ethereum path is a chain service's (it plans and sends at once)")?;
        if self.s["depositor"].is_null() {
            return Err("a deposit needs your Sepolia address (the account Logos's signer sends it from): enter it when you open the session".into());
        }
        let d = params["denomination"].as_u64().unwrap_or(0);
        let kind = params["kind"].as_str().unwrap_or("private");
        let a = params["account"].as_u64().unwrap_or(0);
        let gap = self.s["gap"].as_u64().unwrap_or(8);
        if a >= gap {
            return Err(format!("account /{a} is outside the recovery window ({gap}): a wallet would not find it"));
        }
        let mut v = self.vault(&e)?;
        let depositor: [u8; 20] = fixed(&self.s["depositor"], "depositor")?;
        let j = v.deposit_count(&depositor)?;
        let price = v.price(&[0u8; 20], d as u8)?;
        let mut req = self.seed_fields();
        req["j"] = json!(j);
        req["kind"] = json!(kind);
        req["dest"] = json!(self.account(kind, a)?);
        req["token"] = json!(ETH);
        req["denom_index"] = json!(d);
        let made = crate::deposit_of(&req)?;
        let call = &made["vault_call"];
        let data = unhex(call["data"].as_str().ok_or("vault_call.data")?)?;
        let to: [u8; 20] = fixed(&call["to"], "vault_call.to")?;
        let total = price["total"].as_str().ok_or("price.total")?.to_string();
        let preflight = match crate::user::preflight(&mut v, &depositor, &to, &data, &total) {
            Ok(()) => json!("the vault accepts it (simulated from your address)"),
            Err(e) => json!(format!("the vault's simulation from your address refuses it now: {e}")),
        };
        let dest = if kind == "public" {
            fixed::<32>(&json!(self.account(kind, a)?), "dest").map(|id| crate::lezkey::id_string(&id)).unwrap_or_default()
        } else {
            format!("your private account /{a} (its id stays inside C)")
        };
        Ok(json!({
            "chain_id": self.s["chain_id"],
            "from": hex(&depositor),
            "to": hex(&to),
            "value_wei": total,
            "data": hex(&data),
            "function": "depositETH(uint8,bytes32,uint64)",
            "denomination": d,
            "amount_wei": price["denomination"],
            "price": price,
            "j": j,
            "commitment": made["commitment"],
            "kind": kind,
            "account": a,
            "destination": dest,
            "preflight": preflight,
            "signs": "Logos's signer, through Logos Core's tx_sender_module (keystore_module holds the key); this module holds no Ethereum key",
            // Review M4: Basecamp 0.4.0-rc.2 has neither, so in the network session the plan is not sent from here.
            "sendable": !self.network(),
            "deposit_in": if self.network() { json!(format!("{}/wallet/ (Bridge tab), with this phrase", self.s["network"].as_str().unwrap_or("the network"))) } else { Value::Null },
            "public_on_ethereum": ["the depositor address", "the token", "the denomination and fees", "the time", "the commitment C"],
            "hidden": ["the LEZ destination (C commits to it)"],
        }))
    }

    /// A burn from public account `/a`'s holding (the network session; the web wallet's public burn): the note and
    /// `Burn` message of the module, as a public call of that account, built for the person's approval (`to_approve`);
    /// `confirm` signs it. The release's address is the person's, entered in its checksummed form (review M3).
    fn burn_public(&self, params: &Value) -> Result<Value, String> {
        let a = params["account"].as_u64().unwrap_or(0) as u32;
        let amount: u128 = params["amount"].as_str().ok_or("amount (base units, a string)")?.parse().map_err(|_| "amount: a decimal")?;
        let recipient = crate::net::checksummed_address(params["recipient"].as_str().unwrap_or(""), "release to")?;
        let held = crate::net::holding(a, &[0u8; 20])?;
        if held < amount {
            return Err(format!("refused: LEZ bridge program exit code 254 INSUFFICIENT_BALANCE (checked before sending: public /{a} holds {held} base units on the bridge)"));
        }
        let (i, _) = crate::net::own_burns()?;
        let mut req = self.seed_fields();
        req["i"] = json!(i);
        req["recipient"] = json!(hex(&recipient));
        req["token"] = json!(ETH);
        req["amount"] = json!(amount.to_string());
        let b = crate::burn_of(&req)?;
        let r: Hash = fixed(&b["note"]["r"], "note.r")?;
        let program = self.program()?;
        let cid = self.cid()?;
        let (id, _) = crate::net::public_account(a)?;
        let holder = AccountId::new(id);
        let pda = |actor: ferrata_wallet::Actor, seed: &[u8]| {
            json!({ "account": hex(actor.account_id.value()), "program": hex(actor.program_account_id.value()), "kind": "pda", "seed": hex(seed) })
        };
        let me = bw::own_actor(program, holder);
        let accounts = json!([
            { "account": hex(me.account_id.value()), "program": hex(me.program_account_id.value()), "kind": "signer" },
            pda(bw::holder_actor(program, &cid, holder), bw::holder_seed(&cid, holder.value()).as_bytes()),
            pda(bw::token_actor(program, &cid, &[0u8; 20]), bw::token_seed(&cid, &[0u8; 20]).as_bytes()),
            pda(bw::config_actor(program, &cid), bw::config_seed(&cid).as_bytes()),
            pda(bw::burn_actor(program, &cid, &r), bw::burn_seed(&cid, &r).as_bytes()),
        ]);
        let what = format!("Burn (public) of /{a} with tag counter {i}");
        let recipient_text = crate::net::eip55(&recipient);
        let reply = json!({
            "private": false,
            "burn_index": null,
            "i": i,
            "account": a,
            "amount": amount.to_string(),
            "recipient": recipient_text,
            "release_delay_ms": params["release_delay_ms"],
            "proof": null,
            "made_by": format!("ferrata_bridge_module burn (the note and the Burn message), signed with public /{a}'s key from your phrase after your approval, sent to the network's RPC"),
            "public_on_lez": ["your account and its holding", "the note: r, memo, mac, c_out", "the token and denomination", "the time"],
            "hidden": ["the Ethereum recipient (encrypted in the memo; only your phrase opens it)"],
            "release": release_handoff_text(i),
        });
        let message = unhex(b["message"].as_str().ok_or("message")?)?;
        let mut rec = crate::net::prepare("burn", &message, &accounts, 0, &what, a, Some(bw::burn_actor(program, &cid, &r)), crate::net::BURN_GAS, reply, a)?;
        rec["account"] = json!(a);
        rec["amount"] = json!(amount.to_string());
        rec["recipient"] = json!(recipient_text);
        rec["recipient_note"] = json!("the vault releases to exactly this address on Sepolia, once a checkpoint holds the burn: check it is yours; a release to a wrong address cannot be undone");
        rec["i"] = json!(i);
        Ok(json!({ "to_approve": rec }))
    }

    /// The release in the network session: handed off, as the web wallet does. The release proof's private input says
    /// which burn is released (its index, path, sOut, r), so it is made on the person's machine by the `ferrata` tool
    /// from the same phrase; then a relayer (or the person) presents it to the vault once a checkpoint holds the burn
    /// and its delay has passed. Nothing is sent from here.
    fn release_handoff(&self, params: &Value) -> Result<Value, String> {
        let i = params["i"].as_u64().or_else(|| params["burn_index"].as_u64()).ok_or("release needs the burn's counter i")?;
        let inputs = self.inputs(json!({ "token": ETH }))?;
        let (_, _, burns) = self.positions(&inputs)?;
        let b = burns.iter().find(|b| b["i"].as_u64() == Some(i)).ok_or("not a burn of this wallet")?;
        Ok(json!({
            "settled": false,
            "handoff": true,
            "i": i,
            "recipient": b["recipient"],
            "amount": b["amount"],
            "released": b["released"],
            "what": if b["released"] == true { "already released by the vault".to_string() } else { release_handoff_text(i) },
        }))
    }

    /// One call of the chain service: `{"method", "params"}` -> its `result`, or its `error`.
    fn post(&self, method: &str, params: Value) -> Result<Value, String> {
        if self.network() {
            return crate::net::answer(method, &params);
        }
        let rest = self.url.strip_prefix("http://").ok_or("the chain service: only http:// (127.0.0.1)")?;
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        let addr = authority
            .to_socket_addrs()
            .map_err(|e| format!("{method}: {authority}: {e}"))?
            .next()
            .ok_or_else(|| format!("{method}: no address for {authority}"))?;
        let body = json!({ "method": method, "params": params }).to_string();
        // A private transaction is proven before it is sent: minutes on a CPU.
        let timeout = Duration::from_secs(1800);
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(10))
            .map_err(|e| format!("{method}: the chain service {addr} does not answer: {e}"))?;
        stream.set_read_timeout(Some(timeout)).map_err(|e| e.to_string())?;
        let head = format!(
            "POST {path} HTTP/1.1\r\nHost: {authority}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream
            .write_all(head.as_bytes())
            .and_then(|_| stream.write_all(body.as_bytes()))
            .map_err(|e| format!("{method}: send: {e}"))?;
        let mut raw = Vec::new();
        stream.take(256 << 20).read_to_end(&mut raw).map_err(|e| format!("{method}: read: {e}"))?;
        let end = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or_else(|| format!("{method}: no HTTP header"))?;
        let v: Value = serde_json::from_slice(&raw[end + 4..]).map_err(|e| format!("{method}: the reply is not JSON: {e}"))?;
        if v["ok"].as_bool() == Some(true) {
            Ok(v["result"].clone())
        } else {
            Err(v["error"].as_str().map(str::to_string).unwrap_or_else(|| format!("{method}: {v}")))
        }
    }

    /// The chain service's `module.inputs`; on the `lez_core` path every bridge actor state in it read again
    /// through Logos Core's `lez_core` and compared (`lez` in the reply, `lez.rs`).
    fn inputs(&self, params: Value) -> Result<Value, String> {
        let mut inputs = match eth_path() {
            Some(e) => self.inputs_logos(&e, params.clone())?,
            None => self.post("module.inputs", params.clone())?,
        };
        if crate::lez::active() {
            let token: [u8; 20] = if params["token"].is_null() { [0u8; 20] } else { fixed(&params["token"], "token")? };
            crate::lez::verify_inputs(&mut inputs, &self.s, params["nullifier"].as_str(), &token)?;
        }
        Ok(inputs)
    }

    /// The chain service's LEZ states of `token` (`module.inputs` for LEZ alone: config(cid), token(cid, token), the
    /// newest imported root), each read again through Logos Core's `lez_core` and compared on the `lez_core` path
    /// (`lez` in the reply): what the registry view and the admin tab show on the `eth` path.
    fn lez_inputs(&self, token: &[u8; 20]) -> Result<Value, String> {
        let mut inputs = self.post("module.inputs", json!({ "token": hex(token), "lez_only": true }))?;
        if crate::lez::active() {
            crate::lez::verify_inputs(&mut inputs, &self.s, None, token)?;
        }
        Ok(inputs)
    }

    /// Sends the module's message. A private one (its first account the wallet's private account) on the
    /// `lez_core` path: executed, proven and sent by LEZ's wallet inside Logos Core, then the block that applied
    /// it read by the chain service (`module.tx`, the plain path's view) beside `lez_core`'s own status. Else, or
    /// when `lez_core` sent nothing, the chain service sends it (`module.submit`).
    fn submit(&self, message: &Value, accounts: &Value, root_index: &Value, what: &str) -> Result<Value, String> {
        let private = accounts[0]["kind"] == "private_self";
        let plain = || -> Result<Value, String> {
            if private {
                crate::jobs::phase("proving", json!({ "by": "the chain service (seqnet-chain): LEZ's wallet library executes and proves it, then sends it" }));
            }
            let sent = self.post("module.submit", json!({ "message": message, "accounts": accounts, "root_index": root_index, "what": what }))?;
            crate::jobs::phase("sent", json!({ "tx_hash": sent["tx_hash"], "block": sent["block"], "by": "the chain service" }));
            Ok(sent)
        };
        if !(private && crate::lez::active()) {
            let mut sent = plain()?;
            if crate::lez::active() {
                sent["lez"] = json!({ "via": "the chain service", "why": "a public call: lez_core's public send has no account kind for the bridge program's PDAs" });
            }
            return Ok(sent);
        }
        let since = self.post("module.info", json!({}))?["lez_head"].as_u64().unwrap_or(0);
        let message_hex = message.as_str().ok_or("message: hex")?;
        let program = self.s["program"].as_str().ok_or("program")?;
        match crate::lez::send_private(message_hex, accounts, root_index.as_u64().unwrap_or(0), program, what) {
            Ok(lez) => {
                let hash = lez["reply"]["tx_hash"].as_str().unwrap_or("").to_string();
                crate::jobs::phase("sent", json!({ "tx_hash": hash, "prove_ms": lez["prove_ms"], "by": "lez_core", "waiting_for": "the block that applies it" }));
                let applied = self.post("module.tx", json!({ "hash": hash, "since": since }))?;
                let status = crate::lez::poll(&hash);
                Ok(json!({
                    "settled": applied["reverted"] == false,
                    "private": true,
                    "what": what,
                    "tx_hash": hash,
                    "block": applied["block"],
                    "reverted": applied["reverted"],
                    "tx_bytes": applied["tx_bytes"],
                    "proof": { "mode": self.s["proofs"], "prove_ms": lez["prove_ms"], "receipt_bytes": applied["receipt_bytes"],
                               "tx_bytes": applied["tx_bytes"],
                               "by": "LEZ's wallet inside Logos Core (lez_core.send_generic_private_transaction_with_accounts): executed and proven there, verified by the sequencer" },
                    "lez": { "via": "lez_core", "attempts": lez["attempts"], "synced_to": lez["synced_to"],
                             "program_bin_sha256": lez["program_bin_sha256"], "poll_transaction_status": status,
                             "applied_per_the_chain_service": applied },
                }))
            }
            Err(e) => {
                let mut sent = plain()?;
                sent["lez"] = json!({ "via": "the chain service (fallback)", "lez_core_error": e });
                Ok(sent)
            }
        }
    }

    /// The fields the bridge's functions take with the wallet's keys: the chain service's bridge seed, or in the network
    /// session none (`crate::keys` borrows the session's keys under its lock, within the view's verified call only:
    /// review M2, the bridge seed is never in a JSON value or string there; round 2 MF1, no request selects them).
    fn seed_fields(&self) -> Value {
        let mut f = json!({ "chain_id": self.s["chain_id"], "vault": self.s["vault"], "config_id": self.s["config_id"],
                            "program": self.s["program"], "depositor": self.s["depositor"] });
        if !self.network() {
            f["bridge_seed"] = self.s["bridge_seed"].clone();
        }
        f
    }

    fn account(&self, kind: &str, a: u64) -> Result<String, String> {
        let list = if kind == "private" { &self.s["private_accounts"] } else { &self.s["public_accounts"] };
        list[a as usize].as_str().map(str::to_string).ok_or_else(|| format!("account /{a} is outside the recovery window"))
    }

    fn program(&self) -> Result<AccountId, String> {
        Ok(AccountId::new(fixed::<32>(&self.s["program"], "program")?))
    }

    fn cid(&self) -> Result<Hash, String> {
        fixed::<32>(&self.s["config_id"], "config_id")
    }

    fn info(&self) -> Result<Value, String> {
        let i = match eth_path() {
            Some(e) => {
                let mut i = self.post("module.info", json!({ "lez_only": true }))?;
                let mut v = self.vault(&e)?;
                i["eth_block"] = json!(v.block_number()?);
                i["eth_time"] = json!(v.now()?);
                if !self.s["depositor"].is_null() {
                    let depositor: [u8; 20] = fixed(&self.s["depositor"], "depositor")?;
                    i["eth_balance_wei"] = json!(crate::evm::dec(&v.balance(&depositor)?));
                }
                i["eth"] = v.report();
                i
            }
            None => self.post("module.info", json!({}))?,
        };
        // The endpoints in use, each from the module that holds the connection (RFP-022 U.9): the chain service's
        // sequencer; eth_rpc_module's endpoint for the chain on the `eth` path, else the chain service's.
        let owner = EthOwner::of_session();
        // On the `eth` path the endpoint is eth_rpc_module's: when its record cannot be read, that is said, never the
        // chain service's endpoint in its place (review L-7).
        let module_path = matches!(owner, EthOwner::Module { .. });
        let (eth_rpc, eth_error) = match owner.endpoint(&i) {
            Ok(u) if !u.is_empty() => (json!(u), Value::Null),
            Ok(_) if module_path => (Value::Null, json!("eth_rpc_module named no endpoint for the chain")),
            Err(e) if module_path => (Value::Null, json!(e)),
            _ => (self.s["eth_rpc"].clone(), Value::Null),
        };
        let sequencer = if i["sequencer"].is_string() { i["sequencer"].clone() } else { self.s["sequencer"].clone() };
        Ok(json!({
            "network": format!("LEZ through the sequencer {} and Ethereum chain {} through {}",
                               sequencer.as_str().unwrap_or("?"), self.s["chain_id"], eth_rpc.as_str().unwrap_or("?")),
            "endpoints": {
                "eth_rpc": { "url": eth_rpc, "via": owner.label(), "error": eth_error },
                "sequencer": { "url": sequencer, "via": if self.network() { "the network's gateway (its /rpc), read and sent to by the module" } else { SEQUENCER_OWNER } },
            },
            "private_proofs": match self.s["proofs"].as_str() {
                Some("real") => "real: LEZ's wallet proves each private transaction on the CPU; the sequencer verifies it",
                _ => "dev mode: LEZ's wallet makes a fake receipt (RISC0_DEV_MODE=1), which this devnet's sequencer accepts",
            },
            "eth_rpc": eth_rpc,
            "sequencer": sequencer,
            "chain_id": self.s["chain_id"],
            "vault": self.s["vault"],
            "config_id": self.s["config_id"],
            "program": self.s["program"],
            "eth_block": i["eth_block"],
            "eth_time": i["eth_time"],
            "lez_head": i["lez_head"],
            "user": {
                "depositor": self.s["depositor"],
                "eth_balance_wei": i["eth_balance_wei"],
                "public_accounts": self.s["public_accounts"],
                "private_accounts": self.s["private_accounts"],
            },
            "relayer": i["relayer"],
            "network_session": if self.network() { json!({
                "address": self.s["network"],
                "public_accounts": self.s["public_accounts"].as_array().into_iter().flatten()
                    .filter_map(|h| fixed::<32>(h, "account").ok()).map(|id| crate::lezkey::id_string(&id)).collect::<Vec<_>>(),
                "private_proofs": "not made here: a private claim or burn is the ferrata tool's (same phrase)",
                "deposits": "made in the network's web wallet with the same phrase (this Basecamp has no Logos signer for Sepolia)",
                "signing": "each LEZ transaction is built for your approval in the view and signed here only after it (confirm)",
            }) } else { Value::Null },
            "backend": if self.network() {
                "ferrata_bridge_module in Logos Basecamp (network session): LEZ through the network's RPC, Ethereum through the endpoint above, both read here; LEZ public calls signed here with keys from your phrase, each after your approval; no Ethereum send (deposits in the web wallet)"
            } else if eth_path().is_some() {
                "ferrata_bridge_module in Logos Core; Ethereum through Logos Core's eth_rpc_module (reads) and tx_sender_module (sends, approved in the Logos signer); LEZ through the chain service or lez_core"
            } else {
                "ferrata_bridge_module in Logos Core; the chains through seqnet-chain (the stand-in for Logos Core's own modules)"
            },
            "eth": i["eth"],
            "stand_in": "the chain service stands in for Logos Core's own LEZ module (and, on the plain path, its Ethereum modules); on the e2e/seqnet devnets the vault's verifier is the vault tests' mock (no Groth16 wrap) and a stand-in committee finalizes anvil's blocks for LEZ",
        }))
    }

    fn registry(&self) -> Result<Value, String> {
        if let Some(e) = eth_path() {
            return self.registry_logos(&e);
        }
        let r = self.post("module.registry", json!({ "token": ETH }))?;
        let inputs = self.inputs(json!({ "token": ETH }))?;
        let mut token = r["token"].clone();
        token["symbol"] = json!("ETH");
        token["decimals"] = json!(18);
        // Before the operator's first import the bridge has no configuration or token on LEZ yet (its `Init` and
        // `Register` read the vault's state under an attested record): the vault's side alone is shown then.
        let present = |k: &str| inputs[k].as_str().is_some_and(|h| h.len() > 2);
        let mut req = json!({ "bridge_header": self.s["program"] });
        for k in ["token_state", "config_state", "root_state"] {
            if present(k) {
                req[k] = inputs[k].clone();
            }
        }
        let lez = if present("token_state") || present("config_state") { w::registry_of(&req)? } else { json!({}) };
        token["lez"] = lez_token(&lez["token"]);
        Ok(json!({ "tokens": [token], "vault": r["vault"], "lez_config": lez["config"], "imported_root": lez["root"] }))
    }

    /// The registry through Logos Core's eth_rpc_module: EVERY token the vault registered (its `TokenRegistered` logs),
    /// each with its denominations (the permitted amounts), fees, caps and their use and freezes, its symbol and
    /// decimals read from the token, and its LEZ side (token(cid, token)'s gate) when the bridge program has one.
    fn registry_logos(&self, e: &Value) -> Result<Value, String> {
        let mut v = self.vault(e)?;
        let mut tokens = Vec::new();
        let mut vault_side = Value::Null;
        let (mut lez_config, mut imported_root) = (Value::Null, Value::Null);
        for t in v.registered_tokens()? {
            let r = v.registry(&t)?;
            vault_side = r["vault"].clone();
            let mut token = r["token"].clone();
            let (symbol, decimals) = if t == [0u8; 20] { ("ETH".to_string(), 18) } else { v.erc20_meta(&t) };
            token["symbol"] = json!(symbol);
            token["decimals"] = json!(decimals);
            // Read again through lez_core on that path (review: the registry's LEZ side came from the chain service alone).
            let inputs = self.lez_inputs(&t)?;
            let present = |k: &str| inputs[k].as_str().is_some_and(|h| h.len() > 2);
            let mut req = json!({ "bridge_header": self.s["program"] });
            for k in ["token_state", "config_state", "root_state"] {
                if present(k) {
                    req[k] = inputs[k].clone();
                }
            }
            let lez = if present("token_state") || present("config_state") { w::registry_of(&req)? } else { json!({}) };
            if lez_config.is_null() {
                lez_config = lez["config"].clone();
                imported_root = lez["root"].clone();
            }
            token["lez"] = lez_token(&lez["token"]);
            token["lez_read_via"] = lez_read_via(&inputs);
            tokens.push(token);
        }
        Ok(json!({ "tokens": tokens, "vault": vault_side, "lez_config": lez_config, "imported_root": imported_root, "eth": v.report() }))
    }

    fn deposit(&self, params: &Value) -> Result<Value, String> {
        let d = params["denomination"].as_u64().unwrap_or(0);
        let kind = params["kind"].as_str().unwrap_or("private");
        let a = params["account"].as_u64().unwrap_or(0);
        let gap = self.s["gap"].as_u64().unwrap_or(8);
        if a >= gap {
            return Err(format!("account /{a} is outside the recovery window ({gap}): a wallet would not find it"));
        }
        let inputs = self.post("module.deposit_inputs", json!({}))?;
        let price = self.post("module.price", json!({ "denomination": d, "token": ETH }))?;
        let mut req = self.seed_fields();
        req["j"] = inputs["j"].clone();
        req["kind"] = json!(kind);
        req["dest"] = json!(self.account(kind, a)?);
        req["token"] = json!(ETH);
        req["denom_index"] = json!(d);
        let made = crate::deposit_of(&req)?;
        let call = &made["vault_call"];
        let sent = self.post("module.send_deposit", json!({ "to": call["to"], "data": call["data"], "value": price["total"] }))?;
        Ok(json!({
            "tx_hash": sent["tx_hash"],
            "block": sent["block"],
            "leaf_index": sent["leaf_index"],
            "j": sent["j"],
            "amount": price["denomination"],
            "total_paid": price["total"],
            "commitment": made["commitment"],
            "kind": kind,
            "account": a,
            "made_by": "ferrata_bridge_module deposit (the commitment and the vault call); signed and sent by seqnet-chain",
            "public_on_ethereum": ["the depositor address", "the token", "the denomination and fees", "the time", "the commitment C"],
            "hidden": ["the LEZ destination (C commits to it)"],
        }))
    }

    /// The wallet's deposits and burns (`recover`) among the chains' public ones, with where each stands.
    fn positions(&self, inputs: &Value) -> Result<(Value, Vec<Value>, Vec<Value>), String> {
        let gap = self.s["gap"].as_u64().unwrap_or(8);
        let mut dests = Vec::new();
        for kind in ["private", "public"] {
            for a in 0..gap {
                dests.push(json!({ "kind": kind, "index": a, "dest": self.account(kind, a)? }));
            }
        }
        let mut req = self.seed_fields();
        req["gap"] = json!(gap);
        req["destinations"] = json!(dests);
        req["deposits"] = inputs["deposits"].clone();
        // `recover` takes each published burn's note fields beside its index (the chain service nests them).
        req["burns"] = Value::Array(
            inputs["burns"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|b| {
                    let n = &b["note"];
                    json!({ "burn_index": b["burn_index"], "r": n["r"], "memo": n["memo"], "mac": n["mac"],
                            "token": n["token"], "amount": n["amount"], "c_out": n["c_out"] })
                })
                .collect(),
        );
        let rec = crate::recover_of(&req)?;
        let program = self.program()?;
        let cid = self.cid()?;
        let chain_id = self.s["chain_id"].as_u64().ok_or("chain_id")?;
        let vault: [u8; 20] = fixed(&self.s["vault"], "vault")?;
        let touched: Vec<String> = inputs["touched_privately"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_lowercase)).collect();
        let claimed: Vec<String> = inputs["claimed_plaintext"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_lowercase)).collect();
        let refunded: Vec<u64> = inputs["refunded_leaves"].as_array().into_iter().flatten().filter_map(Value::as_u64).collect();
        let released: Vec<String> = inputs["released_nullifiers"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_lowercase)).collect();
        let imported = inputs["imported"].as_array().into_iter().flatten().filter_map(Value::as_u64).max().unwrap_or(0);
        let mut deposits = Vec::new();
        for d in rec["deposits"].as_array().into_iter().flatten() {
            let n: Hash = fixed(&d["nullifier"], "nullifier")?;
            let n_account = hex(bw::nullifier_actor(program, &cid, chain_id, &vault, &n).account_id.value()).to_lowercase();
            let leaf = d["leaf_index"].as_u64().unwrap_or(0);
            let spent_on_lez = self.network() && crate::net::exists(&bw::nullifier_actor(program, &cid, chain_id, &vault, &n))?;
            let state = if claimed.contains(&hex(&n).to_lowercase()) || touched.contains(&n_account) || spent_on_lez {
                "claimed"
            } else if refunded.contains(&leaf) {
                "refunded"
            } else if leaf < imported {
                "claimable"
            } else {
                "waiting for the root import"
            };
            deposits.push(json!({
                "leaf_index": leaf,
                "j": d["j"],
                "amount": word_dec(&d["amount"])?,
                "kind": d["kind"],
                "account": d["account_index"],
                "state": state,
                "nullifier": d["nullifier"],
                "dest": d["dest"],
            }));
        }
        let mut burns = Vec::new();
        for b in rec["burns"].as_array().into_iter().flatten() {
            let n = b["nullifier"].as_str().unwrap_or("").to_lowercase();
            burns.push(json!({
                // In the network session the zone's own burn index is not read (it needs the zone's history): the counter.
                "burn_index": if self.network() { Value::Null } else { b["burn_index"].clone() },
                "i": b["i"],
                "amount": word_dec(&b["amount"])?,
                "recipient": b["recipient"],
                "released": released.contains(&n),
                "nullifier": b["nullifier"],
            }));
        }
        Ok((rec, deposits, burns))
    }

    /// The private holding of account `/a` as the wallet sees it (LEZ's wallet decrypts it).
    fn private_holding(&self, a: u64) -> Result<(u128, Value), String> {
        let id = self.account("private", a)?;
        let mut p = self.post("module.private", json!({ "account": id }))?;
        if crate::lez::active() {
            p = crate::lez::private_state(&id, self.s["program"].as_str().ok_or("program")?, &p)?;
        }
        let state = p["bridge_state"].as_str().unwrap_or("0x");
        let bytes = unhex(state)?;
        let held = if bytes.is_empty() {
            0
        } else {
            match BridgeState::try_from_slice(&bytes) {
                Ok(BridgeState::Holder(h)) => {
                    let cid = self.cid()?;
                    h.holdings.iter().find(|x| x.config_id == cid && x.token == [0u8; 20]).map_or(0, |x| x.amount)
                }
                _ => 0,
            }
        };
        Ok((held, p))
    }

    fn scan(&self) -> Result<Value, String> {
        let gap = self.s["gap"].as_u64().unwrap_or(8);
        let publics: Vec<Value> = (0..gap).map(|a| self.account("public", a).map(Value::from)).collect::<Result<_, _>>()?;
        let inputs = self.inputs(json!({ "token": ETH, "public_holders": publics }))?;
        let (rec, deposits, burns) = self.positions(&inputs)?;
        let mut holdings = Vec::new();
        let (held, _) = self.private_holding(0)?;
        if held > 0 {
            holdings.push(json!({ "kind": "private", "account": 0, "balance": held.to_string() }));
        }
        for (a, h) in inputs["public_holder_states"].as_array().into_iter().flatten().enumerate() {
            let bytes = unhex(h["state"].as_str().unwrap_or("0x"))?;
            if let Ok(BridgeState::Holder(s)) = BridgeState::try_from_slice(&bytes) {
                let cid = self.cid()?;
                let amount = s.holdings.iter().find(|x| x.config_id == cid && x.token == [0u8; 20]).map_or(0, |x| x.amount);
                if amount > 0 {
                    holdings.push(json!({ "kind": "public", "account": a, "balance": amount.to_string() }));
                }
            }
        }
        Ok(json!({
            "deposits": deposits,
            "burns": burns,
            "next_burn_counter": rec["next_i"],
            "holdings": holdings,
            "vault_deposits": inputs["deposits"].as_array().map(|a| a.len()),
            "imported_roots": inputs["imported"],
            "how": if self.network() {
                "ferrata_bridge_module recover over the vault's whole deposit list (checked against depositRoot) and your burns found by their tags; claims read from their nullifier accounts on LEZ"
            } else {
                "ferrata_bridge_module recover over the whole deposit list (checked against depositRoot by the chain service) and the zone's applied calls: no per-item query"
            },
        }))
    }

    fn claim(&self, params: &Value) -> Result<Value, String> {
        let leaf = params["leaf_index"].as_u64().ok_or("claim needs leaf_index")?;
        let private = params["private"].as_bool().unwrap_or(true);
        if private && self.network() {
            return Err(crate::net::PRIVATE_NEEDS_PROOF.into());
        }
        let inputs = self.inputs(json!({ "token": ETH }))?;
        let (_, deposits, _) = self.positions(&inputs)?;
        let d = deposits.iter().find(|d| d["leaf_index"].as_u64() == Some(leaf)).ok_or("not a deposit of this wallet")?;
        let inputs = self.inputs(json!({ "token": ETH, "nullifier": d["nullifier"] }))?;
        let mut req = self.seed_fields();
        for k in ["leaves", "imported", "root_state", "token_state", "nullifier_state"] {
            req[k] = inputs[k].clone();
        }
        req["j"] = d["j"].clone();
        req["leaf_index"] = json!(leaf);
        req["token"] = json!(ETH);
        req["amount"] = d["amount"].clone();
        req["kind"] = d["kind"].clone();
        req["dest"] = d["dest"].clone();
        req["private"] = json!(private);
        if private && d["kind"] == "public" {
            req["prover"] = json!(self.account("private", 0)?);
        }
        if self.network() && d["kind"] != "public" {
            return Err(format!("leaf {leaf} was deposited to your private account /{}: {}", d["account"], crate::net::PRIVATE_NEEDS_PROOF));
        }
        let c = w::claim_with_state(&req)?;
        if c["verdict"] != "claimable" {
            return Err(format!("refused by the module (checked before sending): {} {}", c["error"].as_str().unwrap_or(""), c["detail"].as_str().unwrap_or("")));
        }
        let what = format!("{} of leaf {leaf}", c["variant"].as_str().unwrap_or("claim"));
        if self.network() {
            // Paid and co-signed by the deposit's own public account (it needs LGO for the fee: the faucet), built for the
            // person's approval; `confirm` signs it.
            let dest: [u8; 32] = fixed(&d["dest"], "dest")?;
            let payer = crate::net::index_of(&dest).unwrap_or(0);
            let n: Hash = fixed(&c["nullifier"], "nullifier")?;
            let chain_id = self.s["chain_id"].as_u64().ok_or("chain_id")?;
            let vault: [u8; 20] = fixed(&self.s["vault"], "vault")?;
            let expect = bw::nullifier_actor(self.program()?, &self.cid()?, chain_id, &vault, &n);
            let root_index = c["root_index"].as_u64().unwrap_or(0) as usize;
            let a = d["account"].as_u64().unwrap_or(0);
            let reply = json!({
                "private": false,
                "account": a,
                "nullifier": d["nullifier"],
                "root_leaf_count": c["leaf_count"],
                "proof": null,
                "variant": c["variant"],
                "accounts": c["accounts"],
                "made_by": "ferrata_bridge_module claim (the tool's claim with LEZ state), signed with your public account's key after your approval",
                "public_on_lez": ["the nullifier N (its account)", "the deposit it spends (a plaintext claim names it)", "the destination account and its balance", "the token and denomination"],
                "hidden": ["the depositor's other deposits"],
            });
            let message = unhex(c["message"].as_str().ok_or("message")?)?;
            let mut rec = crate::net::prepare("claim", &message, &c["accounts"], root_index, &what, payer, Some(expect), crate::net::CLAIM_GAS, reply, a as u32)?;
            rec["leaf_index"] = json!(leaf);
            rec["account"] = json!(a);
            rec["amount"] = d["amount"].clone();
            rec["destination"] = json!(format!("your public account /{a}"));
            return Ok(json!({ "to_approve": rec }));
        }
        let sent = self.submit(&c["message"], &c["accounts"], &c["root_index"], &what)?;
        let a = d["account"].as_u64().unwrap_or(0);
        let balance = if private && d["kind"] == "private" {
            self.private_holding(a)?.0.to_string()
        } else {
            String::new()
        };
        Ok(json!({
            "settled": sent["settled"],
            "private": private,
            "what": format!("{} under root(cid, {}) (block {}, transaction {})", c["variant"].as_str().unwrap_or("?"), c["leaf_count"],
                            sent["block"], sent["tx_hash"].as_str().unwrap_or("?")),
            "account": a,
            "nullifier": d["nullifier"],
            "root_leaf_count": c["leaf_count"],
            "balance": balance,
            "tx_hash": sent["tx_hash"],
            "block": sent["block"],
            "proof": sent["proof"],
            "variant": c["variant"],
            "accounts": c["accounts"],
            "made_by": sent_by(&sent, "ferrata_bridge_module claim (the tool's claim with LEZ state)"),
            "lez": sent["lez"],
            "public_on_lez": ["the nullifier N (its account)", "the token and denomination", "the root read", "the fee schedule's range", "the relayer"],
            "hidden": ["the destination account (an alias appears)", "the leaf index", "the depositor", "the private balance"],
        }))
    }

    fn burn(&self, params: &Value) -> Result<Value, String> {
        let private = params["private"].as_bool().unwrap_or(true);
        if self.network() {
            if private {
                return Err(crate::net::PRIVATE_NEEDS_PROOF.into());
            }
            return self.burn_public(params);
        }
        if !private {
            return Err("a public burn is not run through the module on the sequencer (the round trip is the private one)".into());
        }
        let a = params["account"].as_u64().unwrap_or(0);
        let amount: u128 = params["amount"].as_str().ok_or("amount (base units, a string)")?.parse().map_err(|_| "amount: a decimal")?;
        let recipient = params["recipient"].as_str().filter(|s| !s.is_empty()).ok_or("burn needs a recipient")?;
        let (held, _) = self.private_holding(a)?;
        if held < amount {
            return Err(format!("refused: LEZ bridge program exit code 254 INSUFFICIENT_BALANCE (checked before sending: the private holding of /{a} is {held} base units)"));
        }
        let inputs = self.inputs(json!({ "token": ETH }))?;
        let (rec, _, _) = self.positions(&inputs)?;
        let i = rec["next_i"].as_u64().unwrap_or(0);
        let mut req = self.seed_fields();
        req["i"] = json!(i);
        req["recipient"] = json!(recipient);
        req["token"] = json!(ETH);
        req["amount"] = json!(amount.to_string());
        let b = crate::burn_of(&req)?;
        let n = &b["note"];
        let note = bw::Note {
            r: fixed(&n["r"], "note.r")?,
            memo: fixed(&n["memo"], "note.memo")?,
            mac: fixed(&n["mac"], "note.mac")?,
            token: [0u8; 20],
            amount: fixed(&n["amount"], "note.amount")?,
            c_out: fixed(&n["c_out"], "note.c_out")?,
        };
        let program = self.program()?;
        let cid = self.cid()?;
        let holder = AccountId::new(fixed::<32>(&json!(self.account("private", a)?), "account")?);
        let pda = |account: &AccountId, of: &AccountId, seed: &[u8]| {
            json!({ "account": hex(account.value()), "program": hex(of.value()), "kind": "pda", "seed": hex(seed) })
        };
        let me = bw::own_actor(program, holder);
        let (t, ts) = (bw::token_actor(program, &cid, &[0u8; 20]), bw::token_seed(&cid, &[0u8; 20]));
        let (c, cs) = (bw::config_actor(program, &cid), bw::config_seed(&cid));
        let (u, us) = (bw::burn_actor(program, &cid, &note.r), bw::burn_seed(&cid, &note.r));
        let accounts = json!([
            { "account": hex(me.account_id.value()), "program": hex(me.program_account_id.value()), "kind": "private_self" },
            pda(&t.account_id, &t.program_account_id, ts.as_bytes()),
            pda(&c.account_id, &c.program_account_id, cs.as_bytes()),
            pda(&u.account_id, &u.program_account_id, us.as_bytes()),
        ]);
        let sent = self.submit(&b["message"], &accounts, &json!(0), &format!("Burn (private) with tag counter {i}"))?;
        let burn_index = inputs["burns"].as_array().map_or(0, |v| v.len());
        Ok(json!({
            "settled": sent["settled"],
            "private": true,
            "what": format!("Burn (private) of /{a} (block {}, transaction {})", sent["block"], sent["tx_hash"].as_str().unwrap_or("?")),
            "burn_index": burn_index,
            "i": i,
            "account": a,
            "amount": amount.to_string(),
            "recipient": recipient,
            // The view's random release delay, kept with the burn so that a view opened again holds the release as chosen.
            "release_delay_ms": params["release_delay_ms"],
            "balance": self.private_holding(a)?.0.to_string(),
            "tx_hash": sent["tx_hash"],
            "block": sent["block"],
            "proof": sent["proof"],
            "made_by": sent_by(&sent, "ferrata_bridge_module burn (the note and the Burn message)"),
            "lez": sent["lez"],
            "public_on_lez": ["the note: r, memo, mac, c_out (the recipient encrypted)", "the token and denomination", "the time"],
            "hidden": ["the holder (a private account: an alias appears)", "the Ethereum recipient (encrypted in the memo)"],
        }))
    }

    fn release(&self, params: &Value) -> Result<Value, String> {
        let burn_index = params["burn_index"].as_u64().ok_or("release needs burn_index")?;
        let inputs = self.inputs(json!({ "token": ETH }))?;
        let (_, _, burns) = self.positions(&inputs)?;
        let b = burns.iter().find(|b| b["burn_index"].as_u64() == Some(burn_index)).ok_or("not a burn of this wallet")?;
        if b["released"] == true {
            return Err("already released: the vault's NullifierAlreadyUsed for this burn's NOut (checked before sending)".into());
        }
        let roots = self.post("module.vault_roots", json!({ "token": ETH }))?;
        // Any relayer may send it and is paid the relay fee in force for the burn's denomination (the tool's
        // `flow::release_fees`): the vault's current schedule.
        let reg = self.post("module.registry", json!({ "token": ETH }))?;
        let relay_fee = reg["token"]["denominations"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|d| d["amount"] == b["amount"])
            .map(|d| d["relay_fee"].clone())
            .ok_or("the burn's amount is not one of the vault's denominations")?;
        let mut req = self.seed_fields();
        req["relay_fee"] = relay_fee;
        req["i"] = b["i"].clone();
        req["burns"] = inputs["burns"].clone();
        req["roots"] = roots["roots"].clone();
        req["now"] = roots["now"].clone();
        let r = match w::release_by_rule(&req)? {
            Ok((v, _)) => v,
            Err(refusal) => {
                let first = refusal["first_open"].as_u64();
                return Err(match first {
                    Some(t) => format!("refused: vault error ReleaseDelayNotElapsed() (checked before sending: the roots that cover this burn open at Ethereum time {t})"),
                    None => format!("refused (checked before sending): {}", refusal["error"].as_str().unwrap_or("no accepted burn root covers this burn yet: wait for the next checkpoint")),
                });
            }
        };
        let t0 = Instant::now();
        let sent = self.post("module.vault_release", json!({ "release": r }))?;
        Ok(json!({
            "settled": sent["settled"],
            "tx_hash": sent["tx_hash"],
            "block": sent["block"],
            "recipient": sent["recipient"],
            "paid_wei": sent["paid_wei"],
            "sent_by": sent["sent_by"],
            "picked": r["picked"],
            "journal_sha256": r["journal_sha256"],
            "relay_ms": t0.elapsed().as_millis() as u64,
            "made_by": "ferrata_bridge_module release (the wallet's rule: the newest accepted root whose delay has passed; the journal); handed to ferrata-relayer by seqnet-chain",
            "public_on_ethereum": ["the recipient", "the token and amount", "NOut (the release nullifier)", "the burn root used", "the time", "the relayer"],
            "stand_in": sent["stand_in_prover"]["what"],
        }))
    }
}


/// What the release needs, after a burn of counter `i` in the network session.
fn release_handoff_text(i: u64) -> String {
    format!("Release on Sepolia, outside Basecamp for now: with the same phrase, `ferrata release --burn {i}` proves on your \
             machine that one of the checkpointed burns is yours without saying which (a prover service given that input \
             would learn the link to your recipient); a relayer, or you, then presents it to the vault once a checkpoint \
             holds the burn and its release delay has passed")
}

/// A deposit asked for while an earlier deposit of this session was broadcast and reported not yet mined (its job ended
/// `unconfirmed`) is refused as long as that transaction has no receipt: it may still be mined, and then only one of the two lands: the vault refuses the other (it carries the same deposit
/// index, `WrongDepositIndex`) after it has cost its gas.
/// `again: true` (the view's "Deposit again anyway", offered on this refusal) sends it all the same, for an earlier
/// deposit the person knows will not be mined (dropped from the mempool).
fn refuse_while_unmined(v: &mut crate::evm::Vault, params: &Value) -> Result<(), String> {
    if params["again"] == true {
        return Ok(());
    }
    for h in crate::jobs::unconfirmed_hashes("deposit") {
        if v.rpc.call("eth_getTransactionReceipt", json!([h]))?.is_null() {
            return Err(format!("refused: your earlier deposit, transaction {h}, was broadcast and is not mined yet; it may still be. \
                                Wait for it (refresh your positions), or deposit again anyway (again: true): if the earlier one is mined \
                                after all, only one of the two lands: the vault refuses the other (the same deposit index) after it costs its gas"));
        }
    }
    Ok(())
}

/// The session's Ethereum side through Logos Core's own modules (`eth`): reads through `eth_rpc_module`, sends through
/// `tx_sender_module`. Each send waits for one approval in the Logos signer, so it runs as a job the view polls.
impl Chain {
    fn vault(&self, e: &Value) -> Result<crate::evm::Vault, String> {
        let vault: [u8; 20] = fixed(&self.s["vault"], "vault")?;
        crate::evm::Vault::new(&e["eth"], vault, e["from_block"].as_u64().unwrap_or(0))
    }

    /// `module.inputs` for LEZ alone (`lez_only`), with Ethereum's part read here: the deposit list (checked against
    /// `depositRoot`), its leaves, the released nullifiers, the refunded leaves and Ethereum's time.
    fn inputs_logos(&self, e: &Value, params: Value) -> Result<Value, String> {
        let mut p = params;
        p["lez_only"] = json!(true);
        let mut inputs = self.post("module.inputs", p)?;
        let mut v = self.vault(e)?;
        let (block, deposits, root) = v.deposit_list()?;
        inputs["leaves"] = json!(deposits
            .iter()
            .map(|d| hex(&ferrata_bridge_core::inbound::deposit_leaf(d.leaf_index, &d.depositor, &d.token, &d.amount, &d.commitment)))
            .collect::<Vec<_>>());
        inputs["deposits"] = json!(deposits
            .iter()
            .map(|d| json!({ "leaf_index": d.leaf_index, "depositor": hex(&d.depositor), "j": d.j, "token": hex(&d.token),
                             "amount": hex(&d.amount), "commitment": hex(&d.commitment), "block": d.block }))
            .collect::<Vec<_>>());
        inputs["deposit_root"] = json!(hex(&root));
        inputs["eth_block"] = json!(block);
        inputs["eth_time"] = json!(v.now()?);
        inputs["released_nullifiers"] = json!(v.released_nullifiers()?.iter().map(|n| hex(n)).collect::<Vec<_>>());
        inputs["refunded_leaves"] = json!(v.refunded_leaves()?);
        inputs["eth"] = v.report();
        Ok(inputs)
    }

    /// Sends `calls` from `from` through `tx_sender_module` (gas limits with the margin, send.rs), waits for the
    /// person's approval and the broadcast (the job's phases), and reads every call's receipt back through
    /// `eth_rpc_module`; `reverted` says which call reverted, if one did.
    fn send_logos(&self, e: &Value, v: &mut crate::evm::Vault, from: &str, calls: &[crate::send::Call], purpose: &str,
                  receipt_wait: Duration) -> Result<crate::send::Sent, String> {
        let chain_id = e["chain_id"].as_u64().ok_or("chain_id")?;
        crate::send::through_stack(v, chain_id, from, calls, purpose, true, Duration::from_secs(900), receipt_wait, json!({}))
    }

    /// The deposit through Logos Core's send stack: the module's commitment and vault call, the price and the
    /// depositor's count read through `eth_rpc_module`, one approval, the `Deposit` event read back.
    fn deposit_logos(&self, params: &Value) -> Result<Value, String> {
        let e = eth_path().ok_or("the Ethereum path is no longer Logos Core's")?;
        let d = params["denomination"].as_u64().unwrap_or(0);
        let kind = params["kind"].as_str().unwrap_or("private");
        let a = params["account"].as_u64().unwrap_or(0);
        let gap = self.s["gap"].as_u64().unwrap_or(8);
        if a >= gap {
            return Err(format!("account /{a} is outside the recovery window ({gap}): a wallet would not find it"));
        }
        let mut v = self.vault(&e)?;
        refuse_while_unmined(&mut v, params)?;
        let depositor: [u8; 20] = fixed(&self.s["depositor"], "depositor")?;
        let j = v.deposit_count(&depositor)?;
        let price = v.price(&[0u8; 20], d as u8)?;
        let mut req = self.seed_fields();
        req["j"] = json!(j);
        req["kind"] = json!(kind);
        req["dest"] = json!(self.account(kind, a)?);
        req["token"] = json!(ETH);
        req["denom_index"] = json!(d);
        let made = crate::deposit_of(&req)?;
        let call = &made["vault_call"];
        let data = unhex(call["data"].as_str().ok_or("vault_call.data")?)?;
        let total = price["total"].as_str().ok_or("price.total")?.to_string();
        let to: [u8; 20] = fixed(&call["to"], "vault_call.to")?;
        // The vault's refusal (a frozen token, a disabled deposit, a cap) is named before anything is asked or signed.
        crate::user::preflight(&mut v, &depositor, &to, &data, &total)?;
        let calls = [crate::send::Call::new(to, total.clone(), data, format!("Ferrata deposit: {} wei into the vault", total))];
        let user = e["user"].as_str().ok_or("user")?.to_string();
        // `receipt_timeout_s`: how long the person's view waits for the receipt (the module's own 600 s by default).
        let sent = self.send_logos(&e, &mut v, &user, &calls,
            &format!("Ferrata Bridge deposit: denomination {d} of ETH to a {kind} LEZ account (hidden in the commitment)"),
            crate::send::receipt_wait(params))?;
        if let Some(why) = sent.unconfirmed_text("this deposit") {
            // Broadcast, not yet mined: the job ends `unconfirmed` (jobs.rs), never "not deposited"; the module refuses
            // another deposit while this one is unmined (above).
            return Ok(json!({ "settled": false, "pending": true, "what": why, "tx_hash": sent.last_hash(), "send": sent.record, "eth": v.report() }));
        }
        if let Some(why) = &sent.reverted {
            return Ok(json!({ "settled": false, "what": format!("{why}; nothing was deposited"), "tx_hash": sent.last_hash(),
                              "send": sent.record, "eth": v.report() }));
        }
        let receipt = sent.last().cloned().ok_or("tx_sender_module broadcast no transaction")?;
        let (ok, block, event) = crate::evm::receipt_summary(&receipt, &to)?;
        let event = event.ok_or("the receipt has no Deposit event of the vault")?;
        if event.commitment != fixed::<32>(&made["commitment"], "commitment")? {
            return Err("the Deposit event's commitment is not the module's".into());
        }
        Ok(json!({
            "settled": ok,
            "tx_hash": sent.last_hash(),
            "block": block,
            "gas_used": receipt["gasUsed"],
            "leaf_index": event.leaf_index,
            "j": event.j,
            "amount": price["denomination"],
            "total_paid": total,
            "commitment": made["commitment"],
            "kind": kind,
            "account": a,
            "made_by": "ferrata_bridge_module deposit (the commitment and the vault call), sent through Logos Core's tx_sender_module after one approval in the Logos signer; keystore_module signed",
            "send": sent.record,
            "eth": v.report(),
            "public_on_ethereum": ["the depositor address", "the token", "the denomination and fees", "the time", "the commitment C"],
            "hidden": ["the LEZ destination (C commits to it)"],
        }))
    }

    /// The release through Logos Core's send stack, sent by the session's `release_sender` (anyone may send a release;
    /// the relay fee goes to the sender): the wallet's rule over the accepted roots read through `eth_rpc_module`, the
    /// stand-in prover's seal (the vault's verifier is the tests' mock), then `release(r, seal)` after one approval.
    fn release_logos(&self, params: &Value) -> Result<Value, String> {
        let e = eth_path().ok_or("the Ethereum path is no longer Logos Core's")?;
        let burn_index = params["burn_index"].as_u64().ok_or("release needs burn_index")?;
        let inputs = self.inputs(json!({ "token": ETH }))?;
        let (_, _, burns) = self.positions(&inputs)?;
        let b = burns.iter().find(|b| b["burn_index"].as_u64() == Some(burn_index)).ok_or("not a burn of this wallet")?;
        if b["released"] == true {
            return Err("already released: the vault's NullifierAlreadyUsed for this burn's NOut (checked before sending)".into());
        }
        let mut v = self.vault(&e)?;
        let roots = v.roots_for_release(&[0u8; 20])?;
        let reg = v.registry(&[0u8; 20])?;
        let denominations = reg["token"]["denominations"].as_array().cloned().unwrap_or_default();
        // The position's amount is decimal already (positions: word_dec).
        let amount = b["amount"].as_str().ok_or("the burn's amount")?.to_string();
        let relay_fee = denominations
            .iter()
            .find(|d| d["amount"].as_str() == Some(amount.as_str()))
            .map(|d| d["relay_fee"].clone())
            .ok_or("the burn's amount is not one of the vault's denominations")?;
        let mut req = self.seed_fields();
        req["relay_fee"] = relay_fee;
        req["i"] = b["i"].clone();
        req["burns"] = inputs["burns"].clone();
        req["roots"] = roots["roots"].clone();
        req["now"] = roots["now"].clone();
        let r = match w::release_by_rule(&req)? {
            Ok((v, _)) => v,
            Err(refusal) => {
                return Err(match refusal["first_open"].as_u64() {
                    Some(t) => format!("refused: vault error ReleaseDelayNotElapsed() (checked before sending: the roots that cover this burn open at Ethereum time {t})"),
                    None => format!("refused (checked before sending): {}", refusal["error"].as_str().unwrap_or("no accepted burn root covers this burn yet: wait for the next checkpoint")),
                });
            }
        };
        crate::jobs::phase("proving", json!({ "by": "the stand-in prover (seqnet-chain): the vault tests' mock verifier is told to accept the exact journal; no Groth16 wrap here" }));
        let proof = self.post("module.prove_release", json!({ "release": r }))?;
        let seal = unhex(proof["seal"].as_str().ok_or("the prover's seal")?)?;
        let (data, recipient) = crate::user::release_call(&mut v, &r, &seal)?;
        let sender = e["release_sender"].as_str().ok_or("release_sender")?.to_string();
        let before = v.balance(&recipient)?;
        let vault: [u8; 20] = fixed(&self.s["vault"], "vault")?;
        crate::user::preflight(&mut v, &fixed(&json!(sender), "release_sender")?, &vault, &data, "0")?;
        let calls = [crate::send::Call::new(vault, "0", data, "Ferrata release to the burn's recipient")];
        let sent = self.send_logos(&e, &mut v, &sender, &calls,
            "Ferrata Bridge release: the vault pays a burn's recipient (the relay fee to the sender)", crate::send::RECEIPT_WAIT)?;
        if let Some(why) = sent.unconfirmed_text("this release") {
            return Ok(json!({ "settled": false, "pending": true, "what": why, "tx_hash": sent.last_hash(), "recipient": hex(&recipient),
                              "send": sent.record, "eth": v.report() }));
        }
        if let Some(why) = &sent.reverted {
            return Ok(json!({ "settled": false, "what": format!("{why}; nothing was released"), "tx_hash": sent.last_hash(),
                              "recipient": hex(&recipient), "paid_wei": "0", "send": sent.record, "eth": v.report() }));
        }
        let receipt = sent.last().cloned().ok_or("tx_sender_module broadcast no transaction")?;
        let (ok, block, _) = crate::evm::receipt_summary(&receipt, &vault)?;
        let after = v.balance(&recipient)?;
        let paid = crate::evm::as_u128(&after)?.saturating_sub(crate::evm::as_u128(&before)?);
        Ok(json!({
            "settled": ok && paid > 0,
            "what": if ok && paid > 0 { Value::Null } else { json!(format!("the release in block {block} paid the recipient nothing")) },
            "tx_hash": sent.last_hash(),
            "block": block,
            "gas_used": receipt["gasUsed"],
            "recipient": hex(&recipient),
            "paid_wei": paid.to_string(),
            "sent_by": format!("{sender} through Logos Core's tx_sender_module (one approval in the Logos signer)"),
            "picked": r["picked"],
            "journal_sha256": r["journal_sha256"],
            "send": sent.record,
            "eth": v.report(),
            "made_by": "ferrata_bridge_module release (the wallet's rule over the accepted roots read through eth_rpc_module; the journal; the vault call), sent through tx_sender_module",
            "public_on_ethereum": ["the recipient", "the token and amount", "NOut (the release nullifier)", "the burn root used", "the time", "the sender"],
            "stand_in": proof["stand_in_prover"],
        }))
    }

    /// Both sides' roles and controls: the vault's through `eth_rpc_module`, LEZ's from the bridge program's states.
    fn admin_status(&self) -> Result<Value, String> {
        let e = eth_path().ok_or("admin.status reads the vault through the Ethereum path")?;
        let mut v = self.vault(&e)?;
        let roles = v.roles()?;
        let vault_registry = v.registry(&[0u8; 20])?;
        let registry = self.registry()?;
        let inputs = self.lez_inputs(&[0u8; 20])?;
        let present = |k: &str| inputs[k].as_str().is_some_and(|h| h.len() > 2);
        let mut req = json!({ "bridge_header": self.s["program"] });
        for k in ["token_state", "config_state"] {
            if present(k) {
                req[k] = inputs[k].clone();
            }
        }
        let lez = if present("config_state") { w::registry_of(&req)? } else { Value::Null };
        let controls = &lez["config"]["controls"];
        Ok(json!({
            // The LEZ configuration's roles and floor, as the view's LEZ section shows them (the devnet backend's shape).
            "floor": if controls["floor"].is_null() { json!({ "capacity": "?" }) } else { controls["floor"].clone() },
            "vault_roles": roles,
            "vault": vault_registry,
            "lez": lez,
            "registry": registry,
            "senders": { "admin": e["admin"], "freeze_authority": e["freeze_authority"] },
            "lez_read_via": lez_read_via(&inputs),
            // The view's LEZ section (the bridge program's roles); the vault's are `vault_roles`.
            "admin": controls["admin"], "freeze_authority": controls["freeze_authority"], "fee_recipient": controls["fee_recipient"],
            "eth": v.report(),
        }))
    }

    /// A vault admin or freeze-authority call (`admin.rs`), sent from the role's account through the send stack.
    fn admin_vault(&self, params: &Value) -> Result<Value, String> {
        let e = eth_path().ok_or("the Ethereum path is no longer Logos Core's")?;
        let action = params["action"].as_str().ok_or("admin.vault needs an action")?;
        let role = crate::admin::role_of(action, &params["params"])?;
        let from = match role {
            "admin" | "pending_admin" => e["admin"].clone(),
            "freeze_authority" => e["freeze_authority"].clone(),
            _ => e["user"].clone(),
        };
        if from.is_null() {
            return Err(format!("no account for the vault's {role} in the session's Ethereum path"));
        }
        let mut v = self.vault(&e)?;
        // `gas_limit`: the admin's own limit for the call (the view's field); absent, the stack's estimate + the margin.
        let sender = json!({ "via": crate::send::TX_SENDER, "from": from, "wait": true, "timeout_s": 900, "gas_limit": params["gas_limit"] });
        let mut out = crate::admin::vault_admin(&mut v, action, &params["params"], &sender, e["chain_id"].as_u64().unwrap_or(0))?;
        out["settled"] = json!(out["verdict"] == "settled");
        if out["settled"] != true {
            // jobs.rs fails the job with this sentence: the view shows it where the admin acted.
            out["what"] = out["error"].clone();
        }
        Ok(out)
    }

    /// A LEZ admin message built by the module (`admin.rs`: at the role's own actor, with config(cid), the token and,
    /// for a freeze, the record) and sent as a public transaction through the chain service, as the session's other
    /// public calls are (`submit`: lez_core's public send has no account kind for the bridge program's PDAs). The
    /// signer is the configuration's role holder read from config(cid)'s state, or `params.signer` (an account id, or
    /// `"user"`: the wallet's first public account). A signer that does not hold the role is refused here before
    /// anything is sent, unless `params.unchecked` asks to send it anyway (to show the program's own refusal).
    ///
    /// No role message is sent without a person's approval: the module hands a role message only to a signer with an
    /// approval step (the chain service's `module.role_gate`: the stand-in for the role's own wallet holds it until a
    /// person approves exactly that message with the role password), and waits for that decision (the job's
    /// `awaiting_approval` phase); unapproved after [`ROLE_WAIT`] it is withdrawn. A `Sunset` (irreversible) is no
    /// exception.
    fn admin_lez(&self, method: &str, params: &Value) -> Result<Value, String> {
        let (action, p) = if method == "admin.set_cap" {
            ("lez_set_cap".to_string(), json!({ "token": ETH, "mints": params["mints"].as_bool().unwrap_or(true),
                                                "capacity": params["capacity"], "refill_per_second": params["refill_per_second"] }))
        } else {
            (params["action"].as_str().ok_or("admin.lez needs an action (lez_*)")?.to_string(), params["params"].clone())
        };
        // The program's states read again through lez_core (Logos Core's LEZ wallet) on that path, compared with the
        // service's.
        let token: [u8; 20] = if p["token"].is_null() { [0u8; 20] } else { fixed(&p["token"], "params.token")? };
        let inputs = self.lez_inputs(&token)?;
        let config_state = inputs["config_state"].as_str().filter(|h| h.len() > 2).ok_or("the bridge has no configuration on LEZ yet")?.to_string();
        let mut req = json!({ "action": action, "program": self.s["program"], "config_id": self.s["config_id"], "params": p,
                              "config_state": config_state });
        // Who signs: first the role holder the configuration names (the module's own reading of its state).
        let probe = { let mut r = req.clone(); r["signer"] = json!(format!("0x{}", "00".repeat(32))); crate::admin::admin_of(&r)? };
        let signer = match params["signer"].as_str() {
            Some("user") => self.account("public", 0)?,
            Some(a) => a.to_string(),
            None => match &probe["role_holder"] {
                Value::String(h) => h.clone(),
                Value::Array(m) if m.len() == 1 => m[0].as_str().unwrap_or_default().to_string(),
                other => return Err(format!("the configuration's {} is not one key ({other}): sign it with the role's own wallet", probe["role"])),
            },
        };
        req["signer"] = json!(signer);
        let built = crate::admin::admin_of(&req)?;
        if built["signer_holds_role"] == false && params["unchecked"] != true {
            return Err(format!("refused before sending: {signer} is not the configuration's {} ({})", built["role"], built["role_holder"]));
        }
        let what = format!("{} (admin view){}", built["what"].as_str().unwrap_or("LEZ admin"),
                           if action == "lez_sunset" { ": ends this configuration for good (irreversible)" } else { "" });
        if built["role"] != "anyone" {
            // Only to a signer that asks a person first.
            role_gate_ok(self.post("module.role_gate", json!({})))?;
        }
        // The role's account holds no LEZ: the wallet's public account /0 pays the fee and co-signs (outside the
        // message's accounts), a stand-in for the role's own funded wallet.
        let payer = self.account("public", 0)?;
        let mut sent = self.post("module.submit", json!({ "message": built["message"], "accounts": built["accounts"],
                                                           "root_index": built["root_index"], "what": what, "payer": payer }))?;
        let mut approval = Value::Null;
        if sent["pending_approval"] == true {
            approval = sent.clone();
            sent = self.wait_role(&sent)?;
        }
        crate::jobs::phase("sent", json!({ "tx_hash": sent["tx_hash"], "block": sent["block"], "by": "the chain service's LEZ wallet", "signer": signer, "payer": payer }));
        let settled = sent["settled"] == true;
        let mut out = built;
        out["settled"] = json!(settled);
        out["tx_hash"] = sent["tx_hash"].clone();
        out["block"] = sent["block"].clone();
        out["sent"] = sent;
        out["signed_by"] = json!("the chain service's LEZ wallet (seqnet-chain, LEZ's wallet library), holding the role's key in this run: the stand-in for the role's own LEZ wallet or multisig");
        out["approved"] = if approval.is_null() {
            json!("not held: not signed by a role key the wallet holds (the user's own account)")
        } else {
            json!({ "approval_id": approval["approval_id"], "message_sha256": approval["message_sha256"], "by": "a person, at the role wallet's approval step (the role password)" })
        };
        out["fee_paid_by"] = json!(payer);
        if settled {
            if action == "lez_set_cap" {
                out["after"] = self.registry()?["tokens"][0]["lez"].clone();
            }
        } else {
            out["what"] = json!(format!("refused by the bridge program: the transaction {} was included in block {} and reverted (signer {signer}, the configuration's {}: {})",
                                        out["tx_hash"].as_str().unwrap_or("?"), out["block"], out["role"], out["role_holder"]));
        }
        Ok(out)
    }
}

/// Whether the chain service's signer asks a person before it signs a LEZ role message (`module.role_gate`): the module
/// hands it no role message otherwise.
fn role_gate_ok(gate: Result<Value, String>) -> Result<(), String> {
    match gate {
        Ok(g) if g["approval"] == "password" => Ok(()),
        Ok(g) => Err(format!("refused before sending: the chain service would sign LEZ role messages without a person's approval ({g})")),
        Err(e) => Err(format!("refused before sending: the chain service has no approval step for LEZ role messages ({e})")),
    }
}

/// How long a LEZ role message waits for the person's approval at the role wallet before it is withdrawn.
const ROLE_WAIT: Duration = Duration::from_secs(900);

impl Chain {
    /// Waits for the person's decision on a role message the chain service holds (`held`: its `pending_approval` reply):
    /// the job is `awaiting_approval` meanwhile. The send's reply once approved and sent; the refusal otherwise; at
    /// [`ROLE_WAIT`] the request is withdrawn (`module.cancel_role`) so that it cannot still be approved untracked.
    fn wait_role(&self, held: &Value) -> Result<Value, String> {
        let id = held["approval_id"].as_str().ok_or("the chain service held the message without an approval_id")?.to_string();
        crate::jobs::phase("awaiting_approval", json!({
            "approval_id": id, "what": held["what"], "message": held["message"], "signer": held["signer"],
            "message_sha256": held["message_sha256"],
            "approve_in": "the role's LEZ wallet (here its stand-in, seqnet-chain: module.approve_role with the role password; headless e2e/seqnet/ops.sh approve-lez)",
        }));
        let t0 = Instant::now();
        loop {
            let st = self.post("module.role_status", json!({ "approval_id": id }))?;
            match st["state"].as_str().unwrap_or("") {
                "awaiting_approval" | "approved" => {}
                "sent" => return Ok(st["result"].clone()),
                "rejected" => return Err(format!("rejected at the role wallet ({id}): the person declined; nothing was signed or sent")),
                "cancelled" => return Err(format!("withdrawn ({id}): nothing was signed or sent")),
                "expired" => return Err(format!("not approved in time at the role wallet ({id}): nothing was signed or sent")),
                other => return Err(format!("the role wallet answered {other} for {id}: {}", st["error"].as_str().unwrap_or("no reason given"))),
            }
            if t0.elapsed() > ROLE_WAIT {
                let w = self.post("module.cancel_role", json!({ "approval_id": id }));
                return Err(format!("not approved within {} s at the role wallet ({id}); {}", ROLE_WAIT.as_secs(), match w {
                    Ok(_) => "withdrawn: nothing will be signed for it".to_string(),
                    Err(e) => format!("withdrawing it failed ({e}): it may still be approved there"),
                }));
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    }
}

/// Where a reply's LEZ states came from: `lez_core` (read again and compared: how many reads, how many equal, how many
/// it answered nothing for: an account the zone does not have, which is not counted as equal) or the chain service
/// alone.
fn lez_read_via(inputs: &Value) -> Value {
    match inputs["lez"]["via"].as_str() {
        Some(via) => json!({ "via": via, "reads": inputs["lez"]["reads"], "equal": inputs["lez"]["equal"], "unanswered": inputs["lez"]["unanswered"] }),
        None => json!({ "via": "the chain service (seqnet-chain)" }),
    }
}

/// A token's LEZ side as the registry view shows it (token(cid, token)'s gate), or null when it has none.
fn lez_token(t: &Value) -> Value {
    if t.is_null() {
        return Value::Null;
    }
    json!({
        "now_ms": t["gate"]["now_ms"],
        "mints_frozen_until_ms": t["gate"]["mints_until"],
        "burns_frozen_until_ms": t["gate"]["burns_until"],
        "mint_cap": t["gate"]["mint"],
        "burn_cap": t["gate"]["burn"],
        "known_until_leaf": t["known_until"],
        "fee_schedules": t["schedules"].as_array().map(|a| a.len()),
        "decoded_by": "ferrata_bridge_module registry (token(cid, token) with the bridge program's types)",
    })
}

/// Who made a send: the module, and who executed, proved and sent it.
fn sent_by(sent: &Value, made: &str) -> String {
    match sent["lez"]["via"].as_str() {
        Some("lez_core") => format!("{made}; executed, proven and sent by LEZ's wallet inside Logos Core (lez_core)"),
        _ => format!("{made}; executed, proven and sent by LEZ's wallet library (seqnet-chain)"),
    }
}

/// A 32-byte word (hex) as a decimal string.
fn word_dec(v: &Value) -> Result<String, String> {
    let w: [u8; 32] = fixed(v, "amount")?;
    if w[..16].iter().any(|b| *b != 0) {
        return Err("amount above 2^128".into());
    }
    Ok(u128::from_be_bytes(w[16..].try_into().expect("16 bytes")).to_string())
}

/// The view's side of the network session for a native client (the tests, the `call` example): what Main.qml does
/// with `qml/js/seal.js`, here in Rust. `open` asks `seal_key`, seals the phrase to it (or asks for a new one, whose
/// words come back sealed), and `call` sends each later call with its counter and MAC, as `caller`.
pub struct ViewClient {
    caller: String,
    keys: Option<crate::seal::Keys>,
    n: u64,
}

impl ViewClient {
    pub fn new(caller: &str) -> Self {
        Self { caller: caller.to_string(), keys: None, n: 0 }
    }

    /// `open_network` with `params` (network, eth_rpc, depositor) and `phrase` (`None`: a new one). The module's reply,
    /// and a new phrase's words opened (wiped when dropped).
    pub fn open(&mut self, params: &Value, phrase: Option<&str>) -> (String, Option<zeroize::Zeroizing<String>>) {
        let fail = |e: String| (json!({ "ok": false, "error": e }).to_string(), None);
        let k: Value = serde_json::from_str(&wallet_as(&self.caller, r#"{"method":"seal_key","params":{}}"#)).unwrap_or(Value::Null);
        if k["ok"] != true {
            return (k.to_string(), None);
        }
        let module_pk: [u8; 32] = match fixed(&k["result"]["public_key"], "public_key") {
            Ok(p) => p,
            Err(e) => return fail(e),
        };
        let mut secret = zeroize::Zeroizing::new([0u8; 32]);
        if let Err(e) = getrandom::getrandom(secret.as_mut()) {
            return fail(format!("no randomness: {e}"));
        }
        let (keys, view_pk) = crate::seal::view_side(&secret, &module_pk);
        let mut p = params.clone();
        p["key_id"] = k["result"]["key_id"].clone();
        p["view_key"] = json!(hex(&view_pk));
        match phrase {
            Some(ph) => {
                let (ct, tag) = keys.seal_phrase(ph.as_bytes());
                p["phrase_sealed"] = json!({ "ct": hex(&ct), "mac": hex(&tag) });
            }
            None => p["generate"] = json!(true),
        }
        let reply = wallet_as(&self.caller, &json!({ "method": "open_network", "params": p }).to_string());
        let v: Value = serde_json::from_str(&reply).unwrap_or(Value::Null);
        let words = v["result"]["words_sealed"].as_object().and_then(|w| {
            let ct = unhex(w.get("ct")?.as_str()?).ok()?;
            let tag = unhex(w.get("mac")?.as_str()?).ok()?;
            let bytes = keys.open_words(&ct, &tag).ok()?;
            std::str::from_utf8(&bytes).ok().map(|t| zeroize::Zeroizing::new(t.to_string()))
        });
        if v["ok"] == true {
            self.keys = Some(keys);
            self.n = 0;
        }
        (reply, words)
    }

    /// The envelope of call `method` (its counter and MAC), without sending it.
    pub fn envelope(&mut self, method: &str, params: &Value) -> String {
        let body = json!({ "method": method, "params": params }).to_string();
        match &self.keys {
            None => body,
            Some(k) => {
                self.n += 1;
                json!({ "call": body, "n": self.n, "mac": hex(&k.mac_call(self.n, &body)) }).to_string()
            }
        }
    }

    /// One call of the session (a plain call before it opens, and after it closes).
    pub fn call(&mut self, method: &str, params: &Value) -> String {
        let e = self.envelope(method, params);
        let reply = wallet_as(&self.caller, &e);
        if method == "close" && serde_json::from_str::<Value>(&reply).is_ok_and(|r| r["ok"] == true) {
            self.keys = None;
        }
        reply
    }

    /// An envelope sent as it is (a replay, in the tests).
    pub fn send(&self, envelope: &str) -> String {
        wallet_as(&self.caller, envelope)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::tests::{serial, wait_done};
    use std::sync::mpsc;

    fn call(method: &str, params: Value) -> Value {
        serde_json::from_str(&wallet(&json!({ "method": method, "params": params }).to_string())).expect("the reply is JSON")
    }

    /// A stand-in chain service: answers each request with the next of `replies` (`{"ok": true, "result"}`), and
    /// records the methods asked.
    fn fake_chain(replies: Vec<Value>) -> (String, std::sync::Arc<Mutex<Vec<String>>>) {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", l.local_addr().unwrap());
        let seen = std::sync::Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            let mut replies = replies.into_iter();
            for stream in l.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                let body = loop {
                    let n = stream.read(&mut chunk).unwrap_or(0);
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..end]).to_lowercase();
                        let len = head.lines().find_map(|l| l.strip_prefix("content-length:")).and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(0);
                        if buf.len() >= end + 4 + len {
                            break buf[end + 4..end + 4 + len].to_vec();
                        }
                    }
                    if n == 0 {
                        break Vec::new();
                    }
                };
                let req: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                log.lock().unwrap().push(req["method"].as_str().unwrap_or("").to_string());
                let Some(reply) = replies.next() else { break };
                let text = json!({ "ok": true, "result": reply }).to_string();
                let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", text.len(), text);
            }
        });
        (url, seen)
    }

    /// No LEZ role message leaves without a person's approval: the module hands one only to a signer that asks first
    /// (`module.role_gate`), waits for the decision, and reports a rejection as nothing sent.
    #[test]
    fn a_lez_role_message_is_sent_only_after_a_persons_approval() {
        assert!(role_gate_ok(Ok(json!({ "approval": "password" }))).is_ok());
        assert!(role_gate_ok(Ok(json!({ "approval": "none" }))).unwrap_err().starts_with("refused before sending"));
        assert!(role_gate_ok(Err("unknown method module.role_gate".into())).unwrap_err().contains("no approval step"));
        let held = json!({ "pending_approval": true, "approval_id": "lra_1", "what": "Sunset (admin view): ends this configuration for good (irreversible)",
                           "message": "Sunset", "message_sha256": "0xab" });
        let (url, seen) = fake_chain(vec![
            json!({ "state": "awaiting_approval" }),
            json!({ "state": "sent", "result": { "settled": true, "tx_hash": "0x01", "block": 7 } }),
        ]);
        let sent = Chain { url, s: Value::Null }.wait_role(&held).unwrap();
        assert_eq!(sent["block"], 7);
        assert_eq!(*seen.lock().unwrap(), vec!["module.role_status", "module.role_status"]);
        let (url, _) = fake_chain(vec![json!({ "state": "rejected" })]);
        let e = Chain { url, s: Value::Null }.wait_role(&held).unwrap_err();
        assert!(e.starts_with("rejected at the role wallet (lra_1): the person declined; nothing was signed or sent"), "{e}");
        let (url, _) = fake_chain(vec![json!({ "state": "expired" })]);
        assert!(Chain { url, s: Value::Null }.wait_role(&held).unwrap_err().contains("not approved in time"));
    }

    // ---- the network session (review round 1: H1, M1, M2, M3, M4) ----

    use crate::net::tests::{stand_in_lez, PHRASE, SKIP_CHECKS};
    use std::sync::atomic::Ordering;

    const VIEW: &str = "module:ferrata_bridge_ui";

    fn v(s: &str) -> Value {
        serde_json::from_str(s).expect("the reply is JSON")
    }

    /// A network session opened by the view on a stand-in LEZ (the network's checks skipped), public /0 of the test
    /// phrase holding `held` base units of ETH on the bridge: the view's client, the stand-in, open_network's reply.
    fn open_view(held: u128) -> (ViewClient, crate::net::tests::StandIn, Value) {
        close();
        let seed = crate::lezkey::seed_of_phrase(PHRASE).unwrap();
        let id0 = crate::lezkey::public_accounts(&seed, 1).unwrap()[0].id;
        let dep = &crate::net::KNOWN[0];
        let program = AccountId::new(crate::lezkey::id_from_string(dep.bridge_program).unwrap());
        let cid: Hash = fixed(&json!(dep.config_id), "c").unwrap();
        let holder = *bw::holder_actor(program, &cid, AccountId::new(id0)).account_id.value();
        let net = stand_in_lez(holder, held, Duration::ZERO);
        SKIP_CHECKS.store(true, Ordering::SeqCst);
        let mut view = ViewClient::new(VIEW);
        let (reply, _) = view.open(&json!({ "network": net.url, "eth_rpc": "http://127.0.0.1:9" }), Some(PHRASE));
        SKIP_CHECKS.store(false, Ordering::SeqCst);
        let r = v(&reply);
        assert_eq!(r["ok"], true, "{r}");
        (view, net, r)
    }

    /// Review H1: only the view that opened the network session can use it, close it or replace it; a call is never
    /// accepted twice; every reply names the session's /0. Review M2: the session's record holds no bridge seed.
    #[test]
    fn h1_only_the_view_that_opened_the_network_session_can_use_it() {
        let _s = serial();
        close();
        let r = v(&wallet_as("module:other_module", r#"{"method":"seal_key","params":{}}"#));
        assert!(r["error"].as_str().unwrap().contains("opened by the Ferrata Bridge view (ferrata_bridge_ui) only"), "{r}");
        assert_eq!(v(&wallet_as("host", r#"{"method":"open_network","params":{}}"#))["ok"], false);
        assert_eq!(v(&wallet_as("unknown", r#"{"method":"open_network","params":{}}"#))["ok"], false);
        let (mut view, _net, opened) = open_view(0);
        let account0 = opened["result"]["account0"].clone();
        assert!(account0.is_string());
        assert_eq!(opened["session_account"], account0, "every reply names the session's /0");
        let s = SESSION.lock().unwrap().as_ref().unwrap().s.clone();
        assert!(s.get("bridge_seed").is_none() && !s.to_string().contains("43bff369e1a90a09"), "no bridge seed in the record: {s}");
        // Another module; the view's caller without the session's key; a forged MAC: refused.
        for (caller, req) in [
            ("module:other_module", r#"{"method":"info","params":{}}"#.to_string()),
            (VIEW, r#"{"method":"burn","params":{"private":false,"account":0,"amount":"1","recipient":"0x9858EfFD232B4033E47d90003D41EC34EcaEda94"}}"#.to_string()),
            (VIEW, r#"{"call":"{\"method\":\"close\",\"params\":{}}","n":99,"mac":"00"}"#.to_string()),
            ("module:other_module", view.envelope("close", &json!({}))),
        ] {
            let r = v(&wallet_as(caller, &req));
            assert_eq!(r["ok"], false, "{caller} {req}: {r}");
            assert!(r["error"].as_str().unwrap().starts_with("refused"), "{r}");
        }
        let e = view.envelope("balances", &json!({}));
        assert_eq!(v(&view.send(&e))["ok"], true);
        assert!(v(&view.send(&e))["error"].as_str().unwrap().contains("a replay"));
        // A second session over the open one (the reviewer's hijack, r1): refused, for another view and for this one.
        let mut other = ViewClient::new(VIEW);
        let (r, words) = other.open(&json!({ "network": "https://n.example" }), None);
        assert!(v(&r)["error"].as_str().unwrap().starts_with("refused") && words.is_none(), "{r}");
        let r = v(&view.call("open_network", &json!({ "network": "https://n.example", "generate": true })));
        assert!(r["error"].as_str().unwrap().contains("a network session is open in this module; close it first"), "{r}");
        let r = v(&view.call("open", &json!({ "chain": "http://127.0.0.1:1/" })));
        assert!(r["error"].as_str().unwrap().contains("close it first"), "{r}");
        assert_eq!(v(&view.call("nothing", &json!({})))["session_account"], account0, "still the view's session");
        // The view closes it; a new one may then be opened.
        let r = v(&view.call("close", &json!({})));
        assert_eq!(r["result"]["closed"], true, "{r}");
        assert!(!crate::net::is_open());
        assert_eq!(v(&wallet_as(VIEW, r#"{"method":"seal_key","params":{}}"#))["ok"], true);
    }

    /// Review H1 and M3: a burn is built for the person's approval and sent only when the view confirms it, as approved,
    /// once; its release address is the one the person entered, checksummed.
    #[test]
    fn h1_m3_a_burn_waits_for_approval_and_needs_a_checksummed_address() {
        let _s = serial();
        let (mut view, net, _) = open_view(1_000_000_000_000_000);
        let mut burn = |recipient: &str| v(&view.call("burn", &json!({ "private": false, "account": 0, "amount": "1000000000000000", "recipient": recipient })));
        let r = burn("");
        assert!(r["error"].as_str().unwrap().contains("there is no default"), "{r}");
        let r = burn("0x9858effd232b4033e47d90003d41ec34ecaeda94");
        assert!(r["error"].as_str().unwrap().contains("checksummed form"), "{r}");
        let r = burn("0x9858EfFD232B4033E47d90003D41EC34EcaEda94");
        let rec = r["result"]["to_approve"].clone();
        assert_eq!(rec["signed"], false, "{r}");
        assert_eq!(rec["recipient"], "0x9858EfFD232B4033E47d90003D41EC34EcaEda94");
        assert!(rec["max_fee"].is_string() && rec["message_hash"].is_string() && rec["payer"].as_str().unwrap().starts_with("public /0"), "{rec}");
        assert!(net.sent.lock().unwrap().is_empty(), "nothing is sent before the approval");
        let id = rec["pending_id"].as_str().unwrap().to_string();
        let r = v(&wallet_as("module:other_module", &json!({ "method": "confirm", "params": { "pending_id": id } }).to_string()));
        assert_eq!(r["ok"], false);
        let r = v(&view.call("confirm", &json!({ "pending_id": id })));
        assert_eq!(r["result"]["job"]["kind"], "burn", "{r}");
        let done = wait_done(r["result"]["job"]["id"].as_u64().unwrap());
        assert_eq!(done["state"], "settled", "{done}");
        assert_eq!(done["result"]["approved"]["message_hash"], rec["message_hash"]);
        assert_eq!(done["result"]["recipient"], "0x9858EfFD232B4033E47d90003D41EC34EcaEda94");
        assert_eq!(net.sent.lock().unwrap().len(), 1);
        let r = v(&view.call("confirm", &json!({ "pending_id": id })));
        assert!(r["error"].as_str().unwrap().contains("no transaction waits"), "{r}");
        view.call("close", &json!({}));
        assert!(v(&view.call("jobs", &json!({})))["result"]["jobs"].as_array().unwrap().is_empty(), "the session's jobs are forgotten on close");
    }

    /// Review M1: the phrase is taken only sealed to the module's one-time key, and a refusal names none of it.
    #[test]
    fn m1_the_phrase_is_never_taken_in_clear() {
        let _s = serial();
        close();
        let r = v(&wallet_as(VIEW, &json!({ "method": "open_network", "params": { "network": "https://n.example", "phrase": PHRASE } }).to_string()));
        let e = r["error"].as_str().unwrap();
        assert!(e.contains("sealed to the module's one-time key") && !e.contains("abandon"), "{e}");
        let (mut params, _) = crate::net::tests::sealed(json!({ "network": "http://127.0.0.1:9", "eth_rpc": "http://127.0.0.1:9" }), Some(PHRASE));
        let (other, _) = crate::net::tests::sealed(json!({}), Some("legal winner thank year wave sausage worth useful legal winner thank yellow"));
        params["phrase_sealed"] = other["phrase_sealed"].clone();
        SKIP_CHECKS.store(true, Ordering::SeqCst);
        let r = v(&wallet_as(VIEW, &json!({ "method": "open_network", "params": params }).to_string()));
        SKIP_CHECKS.store(false, Ordering::SeqCst);
        let e = r["error"].as_str().unwrap();
        assert!(e.contains("does not verify") && !e.contains("abandon") && !e.contains("legal"), "{e}");
        assert!(!crate::net::is_open());
    }

    /// Review M1: a new phrase's words come back sealed to the view; no reply holds them in clear.
    #[test]
    fn m1_a_new_phrases_words_come_back_sealed() {
        let _s = serial();
        close();
        let net = stand_in_lez([0u8; 32], 0, Duration::ZERO);
        SKIP_CHECKS.store(true, Ordering::SeqCst);
        let mut view = ViewClient::new(VIEW);
        let (reply, words) = view.open(&json!({ "network": net.url, "eth_rpc": "http://127.0.0.1:9" }), None);
        SKIP_CHECKS.store(false, Ordering::SeqCst);
        let words = words.expect("the view opens the words");
        assert_eq!(words.split(' ').count(), 24);
        assert!(crate::lezkey::seed_of_phrase(&words).is_ok());
        assert!(!reply.contains(words.as_str()) && v(&reply)["result"].get("words").is_none(), "{reply}");
        assert!(v(&reply)["result"]["words_sealed"]["ct"].is_string());
        view.call("close", &json!({}));
    }

    /// Review M1 (what Qt logs): with Qt's debug logging on, Qt writes every key typed in Basecamp's window to its log;
    /// the view learns it on load (`jobs`) and takes the phrase by paste only.
    #[test]
    fn m1_the_view_learns_that_qt_logs_typed_keys() {
        let _s = serial();
        close();
        let seen = |rules: Option<&str>| {
            match rules {
                Some(r) => std::env::set_var("QT_LOGGING_RULES", r),
                None => std::env::remove_var("QT_LOGGING_RULES"),
            }
            v(&wallet_as(VIEW, r#"{"method":"jobs","params":{}}"#))["result"]["qt_debug_logging"].clone()
        };
        assert_eq!(seen(Some("*.debug=true")), json!(true));
        assert_eq!(seen(Some("qt.gui.shortcutmap.debug=true")), json!(true));
        assert_eq!(seen(Some("qt.qpa.*=true")), json!(true));
        assert_eq!(seen(Some("*.debug=false")), json!(false));
        assert_eq!(seen(Some("ferrata.debug=true")), json!(false));
        assert_eq!(seen(None), json!(false));
    }

    /// Review round 2, L1: with Qt's debug logging on, the open's note says what its log holds, and that the phrase is not
    /// in it (its field's keys never reach Qt's key logging).
    #[test]
    fn r2_l1_the_opens_note_says_what_the_debug_log_holds() {
        let _s = serial();
        close();
        std::env::set_var("QT_LOGGING_RULES", "*.debug=true");
        let net = stand_in_lez([0u8; 32], 0, Duration::ZERO);
        SKIP_CHECKS.store(true, Ordering::SeqCst);
        let (r, _) = ViewClient::new(VIEW).open(&json!({ "network": net.url, "eth_rpc": "http://127.0.0.1:9" }), Some(PHRASE));
        SKIP_CHECKS.store(false, Ordering::SeqCst);
        std::env::remove_var("QT_LOGGING_RULES");
        let checks = v(&r)["result"]["checks"].to_string();
        assert!(checks.contains("its field's keys are kept from Qt's key logging") && !checks.contains("can be read back"), "{checks}");
        close();
    }

    /// Review round 2, MF2: seal_key gives the view 32 fresh bytes of the operating system's randomness for its secret,
    /// and no new phrase is made while Qt debug logging is on.
    #[test]
    fn r2_mf2_the_views_secret_gets_the_modules_randomness() {
        let _s = serial();
        close();
        let key = |_: u8| v(&wallet_as(VIEW, r#"{"method":"seal_key","params":{}}"#))["result"]["entropy"].as_str().unwrap_or("").to_string();
        let (a, b) = (key(0), key(1));
        assert!(a.len() == 66 && a.starts_with("0x") && a != b, "{a} {b}");
        std::env::set_var("QT_LOGGING_RULES", "*.debug=true");
        let net = stand_in_lez([0u8; 32], 0, Duration::ZERO);
        SKIP_CHECKS.store(true, Ordering::SeqCst);
        let (r, words) = ViewClient::new(VIEW).open(&json!({ "network": net.url, "eth_rpc": "http://127.0.0.1:9" }), None);
        SKIP_CHECKS.store(false, Ordering::SeqCst);
        std::env::remove_var("QT_LOGGING_RULES");
        assert!(v(&r)["error"].as_str().unwrap_or("").contains("no new phrase is made here while Qt debug logging is on") && words.is_none(), "{r}");
        assert!(!crate::net::is_open());
    }

    /// Review round 2, MF3: the process is not dumpable from seal_key on, and an open refused for its inputs or its
    /// network opens no phrase and derives no seed.
    #[test]
    fn r2_mf3_nothing_of_the_phrase_before_the_checks() {
        let _s = serial();
        close();
        #[cfg(target_os = "linux")]
        {
            // SAFETY: this process's own attribute.
            unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 1, 0, 0, 0) };
            assert_eq!(v(&wallet_as(VIEW, r#"{"method":"seal_key","params":{}}"#))["ok"], true);
            // SAFETY: as above.
            assert_eq!(unsafe { libc::prctl(libc::PR_GET_DUMPABLE, 0, 0, 0, 0) }, 0, "not dumpable after seal_key");
        }
        let seeds = || crate::lezkey::SEEDS.with(std::cell::Cell::get);
        let before = seeds();
        // A mistyped depositor, then a network that does not answer: refused, no seed derived.
        for (depositor, network) in [("0x9858EfFD232B4033E47d90003D41EC34EcaEdA94", "https://n.example"), ("", "http://127.0.0.1:9")] {
            let mut view = ViewClient::new(VIEW);
            // Plain http to this machine: the https client (a global hook rpc.rs's own test expects unset) is not installed.
            let (r, _) = view.open(&json!({ "network": network, "eth_rpc": "http://127.0.0.1:9", "depositor": depositor }), Some(PHRASE));
            assert_eq!(v(&r)["ok"], false, "{r}");
            assert!(!r.contains("abandon"), "{r}");
        }
        assert_eq!(seeds(), before, "no seed derived for a refused open");
        assert!(!crate::net::is_open());
    }

    /// Review round 2, MF1: no public method of the module reaches the open session's keys: each refuses a request that
    /// names keys, and without the view's verified call no keys of the session are taken.
    #[test]
    fn r2_mf1_no_public_method_reaches_the_sessions_keys() {
        let _s = serial();
        let (_view, _net, _) = open_view(0);
        let req = json!({ "keys": "network_session", "chain_id": 11155111, "vault": "0x19499061cec0bac2a3e1869ba5ae6b403aa82787",
                          "config_id": format!("0x{}", crate::net::KNOWN[0].config_id), "depositor": "0x9858EfFD232B4033E47d90003D41EC34EcaEda94",
                          "j": 0, "kind": "public", "dest": format!("0x{}", "11".repeat(32)), "token": ETH, "denom_index": 0, "i": 0,
                          "recipient": "0x9858EfFD232B4033E47d90003D41EC34EcaEda94", "amount": "1000000000000000", "gap": 8,
                          "destinations": [], "deposits": [], "burns": [] }).to_string();
        let calls: [(&str, fn(&str) -> String); 10] = [
            ("deposit", crate::deposit), ("claim", crate::claim), ("burn", crate::burn), ("release", crate::release),
            ("prove_release", crate::prove_release), ("import_root", crate::import_root), ("registry", crate::registry),
            ("recover", crate::recover), ("status", crate::status), ("admin", crate::admin),
        ];
        for (name, f) in calls {
            let r = v(&f(&req));
            let e = r["error"].as_str().unwrap_or("");
            assert!(e.starts_with("refused: a request does not name keys"), "{name}: {r}");
            assert!(r.get("commitment").is_none() && r.get("note").is_none() && r.get("nullifier").is_none(), "{name}: {r}");
        }
        // Without `keys`, a public method takes no keys of the session either (outside the view's verified call).
        let mut plain: Value = serde_json::from_str(&req).unwrap();
        plain.as_object_mut().unwrap().remove("keys");
        let r = v(&crate::deposit(&plain.to_string()));
        assert_eq!(r["error"], "the request needs `bridge_seed` or `lez_seed`", "{r}");
        close();
    }

    /// Review M4: no deposit is attempted from the network session; the view sends the person to the web wallet.
    #[test]
    fn m4_a_deposit_is_not_sent_from_the_network_session() {
        let _s = serial();
        let (mut view, _net, _) = open_view(0);
        let r = v(&view.call("deposit", &json!({ "denomination": 0, "kind": "public", "account": 0 })));
        assert!(r["error"].as_str().unwrap().contains("deposits are made in the network's web wallet"), "{r}");
        view.call("close", &json!({}));
    }

    /// Review H1 (a view that went away): a session nobody calls ends by itself, its keys wiped.
    #[test]
    fn h1_an_idle_network_session_ends_by_itself() {
        let _s = serial();
        let (_view, _net, _) = open_view(0);
        crate::net::tests::IDLE_MS.store(50, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(120));
        let r = v(&wallet_as("module:other_module", r#"{"method":"jobs","params":{}}"#));
        crate::net::tests::IDLE_MS.store(0, Ordering::SeqCst);
        assert!(!crate::net::is_open(), "ended: {r}");
        assert!(r.get("session_account").is_none());
        close();
    }

    #[test]
    fn the_view_reaches_the_jobs_through_wallet() {
        let _s = serial();
        let r = call("job", json!({ "id": 424_242 }));
        assert_eq!(r["ok"], false);
        assert!(r["error"].as_str().unwrap().starts_with("no job 424242 in this module"), "{r}");
        let l = call("jobs", json!({}));
        assert_eq!(l["ok"], true);
        assert!(l["result"]["jobs"].is_array());
    }

    #[test]
    fn while_a_job_runs_what_would_wait_for_it_is_refused_at_once() {
        let _s = serial();
        let (go, gate) = mpsc::channel::<()>();
        let r = crate::jobs::start("claim", &json!({ "leaf_index": 0 }), move |_| {
            gate.recv().ok();
            Ok(json!({ "settled": true }))
        })
        .unwrap();
        let id = r["job"]["id"].as_u64().unwrap();
        for m in ["scan", "registry", "release", "lez", "lez_report", "open"] {
            let r = call(m, json!({}));
            assert_eq!(r["ok"], false);
            assert!(r["error"].as_str().unwrap().starts_with(&format!("busy: job {id} (the private claim)")), "{m}: {r}");
        }
        // The chain service's own calls are not held (here: no session).
        assert!(call("info", json!({}))["error"].as_str().unwrap().contains("no wallet session"));
        // The job's own poll and the listing answer.
        assert_eq!(call("job", json!({ "id": id }))["result"]["running"], true);
        assert_eq!(call("jobs", json!({}))["result"]["running"].as_u64(), Some(id));
        go.send(()).unwrap();
        assert_eq!(wait_done(id)["state"], "settled");
        assert!(call("scan", json!({}))["error"].as_str().unwrap().contains("no wallet session"));
    }

    /// The session refuses a second deposit while its earlier one was broadcast and is still unmined (the job ended
    /// `unconfirmed`, no receipt yet), unless the person says `again`; once the earlier one is mined there is nothing to
    /// wait for (review L-C: this refusal had no test, and the view could not override it).
    #[test]
    fn a_second_deposit_waits_while_the_first_is_unmined_unless_the_person_says_again() {
        let _g = serial();
        crate::send::fake::install();
        let h = format!("0x{}", "cd".repeat(32));
        let reply = json!({ "settled": false, "pending": true, "tx_hash": h, "what": "broadcast, not yet mined: transaction ..." });
        let r = crate::jobs::start("deposit", &json!({}), move |_| Ok(reply)).unwrap();
        assert_eq!(wait_done(r["job"]["id"].as_u64().unwrap())["state"], "unconfirmed");
        let mut v = crate::evm::Vault::new(&json!({ "via": "eth_rpc_module", "chain_id": 11155111 }), [2; 20], 0).unwrap();
        *crate::send::fake::MINED.lock().unwrap() = false;
        let e = refuse_while_unmined(&mut v, &json!({ "denomination": 0 })).unwrap_err();
        assert!(e.starts_with(&format!("refused: your earlier deposit, transaction {h}, was broadcast and is not mined yet")), "{e}");
        assert!(e.contains("deposit again anyway (again: true)") && e.contains("only one of the two lands"), "{e}");
        refuse_while_unmined(&mut v, &json!({ "denomination": 0, "again": true })).expect("the person's own decision: sent anyway");
        *crate::send::fake::MINED.lock().unwrap() = true;
        refuse_while_unmined(&mut v, &json!({ "denomination": 0 })).expect("mined since: nothing to wait for");
    }
}
