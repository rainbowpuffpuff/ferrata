//! The network session (`wallet {"method": "open_network"}`): the mini-app inside Logos Basecamp against a Ferrata
//! test network's public endpoints, with no chain service. The module itself reads LEZ through the network's gateway
//! (`<network>/rpc`, LEZ's JSON-RPC) and Ethereum through an endpoint the person chooses (a public Sepolia RPC by
//! default), both over HTTPS (rustls, Mozilla's roots); plain `http://` only to this machine.
//!
//! What it accepts: only a deployment this module knows ([`KNOWN`]), checked at open as the web wallet and the
//! `ferrata` tool check it, and refused on any difference: the network's own `/wallet/config.json` must name the
//! known bridge program, configuration, vault and chain; the Ethereum endpoint must answer that chain, with the vault's
//! contract holding that configuration (`lezConfigId()`); on LEZ the bridge program's account must carry the known
//! image id and the configuration must be paired (`config(cid)` ready) with that vault and chain. The LEZ checks are
//! answered by the same host whose configuration they check: they catch a misconfigured network, not a compromised
//! one. What protects the person against a lying host is that the program, the configuration, the vault and the chain
//! are compiled in here and every message signed and every vault call built uses them, never a host's answer; a lying
//! host can still misreport positions, holdings and settlement, or censor.
//!
//! Who may use it (session.rs): only the mini-app's view (`ferrata_bridge_ui`, as Logos Core names the caller), and
//! once open only the view that opened it: every call carries a MAC under a key agreed when it opened (`seal.rs`).
//! An open session is not replaced (close it first) and ends by itself after [`IDLE`] without a call. A LEZ
//! transaction is signed in two steps: [`prepare`] builds it and returns what the person approves (the call, its payer,
//! its fee ceiling, the message hash); [`confirm`] signs exactly that, once.
//!
//! Keys: the person's recovery phrase (BIP-39), sealed by the view to this module's one-time key (`seal.rs`) or made
//! here (its words sealed back to the view, which shows them once). From it the session keeps, in this process's
//! memory only, the signing keys of public accounts `/0`..`/7` (LEZ's key tree, `lezkey.rs`) and the bridge keys
//! (kAuth, kView); the phrase, the 64-byte seed and the bridge seed are wiped as soon as those are derived, and the
//! keys are wiped when the session ends (close, idle, Basecamp quitting). The process is made non-dumpable while it
//! holds them (Linux: `PR_SET_DUMPABLE`, so no core dump and no ptrace or /proc reads by other processes of the user).
//! Nothing is written to disk; no key leaves the module (the network sees signatures, public keys and account ids).
//! Ethereum transactions are never signed here.
//!
//! What runs here: the public side, as the web wallet runs it in a page: the registry, the positions recovered from the
//! phrase, the faucet, the plaintext claim of a deposit to a public account, the public burn, and the release's hand-off
//! (the release proof is the `ferrata` tool's, on the person's machine). A deposit is made in the web wallet (Basecamp
//! has no Logos signer for Sepolia yet). A private claim or burn needs a proof of LEZ's privacy circuit, which this
//! module does not make.

use std::cell::Cell;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use borsh::BorshDeserialize as _;
use ferrata_bridge_core::{outbound, BridgeKeys, Hash};
use ferrata_lez_bridge_wire::{self as bw, BridgeState, ConfigState};
use ferrata_wallet::{AccountId, Actor as WActor};
use serde_json::{json, Value};
use zeroize::Zeroizing;

use crate::lezkey::{self, Evidence, Fee, Message, PublicAccount};
use crate::seal;
use crate::{fixed, hex, unhex};

/// One deployment this module serves: the bridge program, its configuration, the vault and the chain.
pub(crate) struct Deployment {
    pub name: &'static str,
    /// The bridge program's account on LEZ (base58).
    pub bridge_program: &'static str,
    /// The bridge program's image id, as its account on LEZ carries it.
    pub bridge_image_id: &'static str,
    pub config_id: &'static str,
    pub evm_chain_id: u64,
    pub vault: &'static str,
    /// The vault's deployment block (its events start here).
    pub from_block: u64,
}

/// The deployments this module knows: the network's configuration as recorded on 23 November 2026
/// (`deploy/testnet/web/basecamp/tests/install-test/live-wallet-config.json` and `live-programs.json`). A network that
/// names anything else is refused; a new deployment needs a new module.
pub(crate) const KNOWN: &[Deployment] = &[Deployment {
    name: "Ferrata's LEZ v0.4.0-rc3 test network (vault on Sepolia, RISC Zero 3.1)",
    bridge_program: "36iqVMFpfgHrwoeuVvZhXX3caM1JPBJh56PBw5A4H1rk",
    bridge_image_id: "847a4336c202269dbc1f46f67e469186e258a185839ed3ce61425ef99ca17515",
    config_id: "3773b58836d2fa242e16b3eb580a15630de2a437218a432f52e5b01152ae67cd",
    evm_chain_id: 11_155_111,
    vault: "19499061cec0bac2a3e1869ba5ae6b403aa82787",
    from_block: 12_196_188,
}];

/// The Ethereum endpoint when the person names none: a public Sepolia RPC.
pub(crate) const DEFAULT_SEPOLIA_RPC: &str = "https://ethereum-sepolia-rpc.publicnode.com";

/// Public accounts (and private account ids) of the phrase the session derives: LEZ's recovery window.
pub(crate) const GAP: u32 = 8;

/// Gas limits of the bridge's public calls (the web wallet's; measured ~240k for a claim, ~160k for a burn).
pub(crate) const CLAIM_GAS: u64 = 1_000_000;
pub(crate) const BURN_GAS: u64 = 1_000_000;

/// The most LEZ fee this module signs for one call, in 10^-9 LGO: 1 LGO. At the network's base fees a claim or a burn
/// reserves about 0.064; a host that asks for more than this ceiling is refused, nothing signed.
pub(crate) const MAX_FEE: u128 = 1_000_000_000;

/// How long one call of the view may take here, all its requests together: Basecamp ends a view's call to a module at
/// 20 s, and the module serves one call at a time, so a slow or stalling host must not hold the next call (`close`)
/// behind it for longer.
pub(crate) const CALL_LIMIT: Duration = Duration::from_secs(15);

/// A session nobody has called for this long ends at the next call (its keys wiped): a view that went away without
/// closing it leaves no keys behind for good, and a new session can be opened.
pub(crate) const IDLE: Duration = Duration::from_secs(15 * 60);

/// How long a transaction prepared for the person's approval waits for `confirm`.
const PENDING_TTL: Duration = Duration::from_secs(300);

const TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) const PRIVATE_NEEDS_PROOF: &str = "a private claim or burn needs a proof of LEZ's privacy circuit made with the \
account's keys, which this session does not make: run it with the `ferrata` tool from the same phrase (ferrata claim, \
ferrata burn), or choose the plaintext claim or public burn here";

pub(crate) const ALREADY_OPEN: &str = "refused: a network session is open in this module; close it first (Close the session in \
the view that opened it, or quit Basecamp; a session left 15 minutes without a call ends at its next call)";

/// What the person is told about their phrase (open_network's reply, the panel).
pub(crate) const PHRASE_NOTE: &str = "your phrase reached ferrata_bridge_module sealed to its one-time key (never in clear on \
Logos Core's IPC), was opened there only to derive the keys of public accounts /0-/7 and the bridge keys, and was wiped; those \
keys stay in the module's memory (its process not dumpable or readable by other processes) until you close the session or quit \
Basecamp (a session left 15 minutes without a call ends at its next call). What you typed or pasted passed through Basecamp's window (its keyboard events, \
a paste through the clipboard), whose memory the module cannot wipe";

// ------------------------------------------------------------------------------------------------ endpoints

fn host_of(authority: &str) -> &str {
    if let Some(rest) = authority.strip_prefix('[') {
        return rest.split(']').next().unwrap_or("");
    }
    authority.rsplit_once(':').map_or(authority, |(h, p)| if p.chars().all(|c| c.is_ascii_digit()) { h } else { authority })
}

/// An endpoint the session may use: `https://`, or plain `http://` to this machine only (127.0.0.0/8, ::1,
/// localhost). Returned without a trailing slash. No credentials, query or fragment in the URL.
pub(crate) fn endpoint(url: &str, what: &str) -> Result<String, String> {
    let u = url.trim().trim_end_matches('/');
    let (scheme, rest) = u.split_once("://").ok_or_else(|| format!("{what} {u:?}: not a URL (https://…)"))?;
    let authority = rest.split('/').next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') || u.chars().any(|c| c.is_whitespace() || c == '?' || c == '#') {
        return Err(format!("{what} {u:?}: not a usable address"));
    }
    match scheme.to_ascii_lowercase().as_str() {
        "https" => Ok(u.to_string()),
        "http" => {
            let host = host_of(authority).to_ascii_lowercase();
            let loopback = host == "localhost"
                || host == "::1"
                || host.parse::<std::net::Ipv4Addr>().is_ok_and(|ip| ip.is_loopback());
            if loopback {
                Ok(u.to_string())
            } else {
                Err(format!("{what} {u:?}: plain http:// is refused for a host other than this machine; use https://"))
            }
        }
        _ => Err(format!("{what} {u:?}: only https:// (or http:// to this machine)")),
    }
}

/// A path the network's configuration names for one of its own endpoints (`/rpc`, `/faucet`): on the same origin.
fn same_origin(base: &str, path: &str, what: &str) -> Result<String, String> {
    if !path.starts_with('/') || path.starts_with("//") || path.contains("://") {
        return Err(format!("the network's configuration names {what} {path:?}: only a path on the network's own address"));
    }
    Ok(format!("{base}{path}"))
}

// ------------------------------------------------------------------------------------------------ one call's deadline

thread_local! {
    static DEADLINE: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// Runs `f` with all the network requests it makes on this thread bounded by `limit` together (session.rs runs each
/// call of the view so; a job's thread has no such bound).
pub(crate) fn within<T>(limit: Duration, f: impl FnOnce() -> T) -> T {
    let before = DEADLINE.with(|d| d.replace(Some(Instant::now() + limit)));
    let out = f();
    DEADLINE.with(|d| d.set(before));
    out
}

/// How long a request may still take: `want`, or less near the call's deadline; an error once it has passed.
fn budget(want: Duration) -> Result<Duration, String> {
    match DEADLINE.with(Cell::get) {
        None => Ok(want),
        Some(end) => {
            let left = end.saturating_duration_since(Instant::now());
            if left < Duration::from_millis(250) {
                Err(format!(
                    "the network did not answer in time (one call here may take {} s; Basecamp ends a call at 20 s): try again",
                    CALL_LIMIT.as_secs()
                ))
            } else {
                Ok(want.min(left))
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------ HTTP(S)

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .max_redirects(0)
        .build()
        .into()
}

fn body_of(mut r: ureq::http::Response<ureq::Body>) -> Result<(u16, String, Option<u64>), String> {
    let status = r.status().as_u16();
    let retry = r.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|s| s.trim().parse::<u64>().ok());
    let text = r.body_mut().with_config().limit(16 << 20).read_to_string().map_err(|e| format!("reading the answer: {e}"))?;
    Ok((status, text, retry))
}

fn get(url: &str) -> Result<(u16, String), String> {
    let r = agent(budget(TIMEOUT)?).get(url).header("Accept", "application/json").call().map_err(|e| format!("{url}: {e}"))?;
    let (s, t, _) = body_of(r)?;
    Ok((s, t))
}

fn post(url: &str, body: &str, timeout: Duration) -> Result<(u16, String, Option<u64>), String> {
    let r = agent(budget(timeout)?)
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .send(body)
        .map_err(|e| format!("{url}: {e}"))?;
    body_of(r)
}

/// The `https://` client `rpc.rs` uses for Ethereum JSON-RPC (installed by [`open`]).
pub(crate) fn https_post(url: &str, body: &str, timeout_ms: u64) -> Result<String, String> {
    let (status, text, _) = post(url, body, Duration::from_millis(timeout_ms.clamp(1_000, 120_000)))?;
    if status != 200 {
        return Err(format!("HTTP {status} from {url}"));
    }
    Ok(text)
}

// ------------------------------------------------------------------------------------------------ LEZ's JSON-RPC

/// LEZ's sequencer through the network's gateway. A gateway limits requests per address (HTTP 429, Retry-After) and
/// may answer 502/503 while the sequencer restarts: the call waits and tries again, at most four times and never past
/// the call's deadline (nothing reached the sequencer on such an answer, so a resend of `sendTransaction` is safe).
#[derive(Clone)]
pub(crate) struct Lez {
    pub url: String,
}

impl Lez {
    pub(crate) fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string();
        for attempt in 0..5u32 {
            let (status, text, retry) = post(&self.url, &body, TIMEOUT)?;
            if matches!(status, 429 | 502 | 503) && attempt < 4 {
                let wait = Duration::from_millis(retry.map_or(600 * 2u64.pow(attempt), |s| s.saturating_mul(1000)).min(10_000));
                if budget(Duration::MAX)? < wait + Duration::from_millis(250) {
                    return Err(format!("{method}: the network's RPC asks to wait {} s, more than this call has left: try again", wait.as_secs().max(1)));
                }
                std::thread::sleep(wait);
                continue;
            }
            if status != 200 {
                return Err(format!("{method}: HTTP {status} from the network's RPC"));
            }
            let v: Value = serde_json::from_str(&text).map_err(|e| format!("{method}: the answer is not JSON: {e}"))?;
            if let Some(e) = v.get("error").filter(|e| !e.is_null()) {
                return Err(format!("{method}: {}", e["message"].as_str().map_or_else(|| e.to_string(), str::to_string)));
            }
            return Ok(v["result"].clone());
        }
        Err(format!("{method}: the network's RPC kept asking to wait"))
    }

    pub(crate) fn head(&self) -> Result<u64, String> {
        self.call("getLastBlockId", json!([]))?.as_u64().ok_or_else(|| "getLastBlockId: not a number".into())
    }

    pub(crate) fn account(&self, id: &[u8; 32]) -> Result<Value, String> {
        self.call("getAccount", json!([lezkey::id_string(id)]))
    }

    /// The account's actor state under `program`, if it has one.
    pub(crate) fn state_of(&self, account: &[u8; 32], program: &[u8; 32]) -> Result<Option<Vec<u8>>, String> {
        let a = self.account(account)?;
        state_bytes(&a["data"]["actor_states"][lezkey::id_string(program)])
    }

    pub(crate) fn state(&self, actor: &WActor) -> Result<Option<Vec<u8>>, String> {
        self.state_of(actor.account_id.value(), actor.program_account_id.value())
    }

    pub(crate) fn balance(&self, id: &[u8; 32]) -> Result<u128, String> {
        amount(&self.call("getAccountBalance", json!([lezkey::id_string(id)]))?, "getAccountBalance")
    }

    pub(crate) fn nonce(&self, id: &[u8; 32]) -> Result<u128, String> {
        let v = self.call("getAccountsNonces", json!([[lezkey::id_string(id)]]))?;
        amount(&v[0], "getAccountsNonces")
    }

    pub(crate) fn base_fees(&self) -> Result<(u128, u128), String> {
        let f = self.call("getFeeState", json!([]))?;
        Ok((amount(&f["base_fee_exec"], "base_fee_exec")?, amount(&f["base_fee_stor"], "base_fee_stor")?))
    }

    /// The block that holds a transaction, once it is in one.
    pub(crate) fn block_of(&self, hash: &str) -> Result<Option<u64>, String> {
        let t = self.call("getTransaction", json!([hash]))?;
        Ok(match &t {
            Value::Null => None,
            Value::Array(a) => a.get(1).and_then(Value::as_u64),
            _ => t["block_id"].as_u64(),
        })
    }
}

/// An actor state as the RPC gives it: bytes (each 0..=255) or base64; anything else is refused.
fn state_bytes(v: &Value) -> Result<Option<Vec<u8>>, String> {
    match v {
        Value::Null => Ok(None),
        Value::Array(bytes) => Ok(Some(
            bytes.iter().map(|b| b.as_u64().filter(|x| *x < 256).map(|x| x as u8)).collect::<Option<Vec<u8>>>().ok_or("an actor state that is not bytes")?,
        )),
        Value::String(s) => {
            use base64::Engine as _;
            Ok(Some(base64::engine::general_purpose::STANDARD.decode(s).map_err(|_| "an actor state that is not base64")?))
        }
        _ => Err("an actor state in an unknown form".into()),
    }
}

/// An amount the RPC sends as a JSON number or a decimal string.
fn amount(v: &Value, what: &str) -> Result<u128, String> {
    match v {
        Value::Number(n) => n.as_u64().map(u128::from).ok_or_else(|| format!("{what}: not an amount")),
        Value::String(s) => s.parse::<u128>().map_err(|_| format!("{what}: not an amount")),
        _ => Err(format!("{what}: no amount")),
    }
}

// ------------------------------------------------------------------------------------------------ the session

/// The bridge keys (two HMAC-SHA256 states keyed with kAuth and kView: plain data, no heap, no destructor of their
/// own), overwritten when the session ends: the bridge core's `BridgeKeys` has no zeroize.
struct WipedKeys(Box<std::mem::ManuallyDrop<BridgeKeys>>);

const _: () = assert!(!std::mem::needs_drop::<BridgeKeys>());

impl WipedKeys {
    fn new(k: BridgeKeys) -> Self {
        Self(Box::new(std::mem::ManuallyDrop::new(k)))
    }

    fn wipe(&mut self) {
        let p: *mut BridgeKeys = &mut **self.0;
        // SAFETY: `p` is the box's live value, borrowed exclusively here; any byte pattern is a valid `MaybeUninit<u8>`.
        // The value is not read again (wipe runs only on drop) and has no destructor (`ManuallyDrop`, and the assertion
        // above), so nothing observes the zeroed bytes as a `BridgeKeys`.
        let bytes = unsafe { std::slice::from_raw_parts_mut(p.cast::<std::mem::MaybeUninit<u8>>(), std::mem::size_of::<BridgeKeys>()) };
        zeroize::Zeroize::zeroize(bytes);
    }
}

impl Drop for WipedKeys {
    fn drop(&mut self) {
        self.wipe();
    }
}

impl std::ops::Deref for WipedKeys {
    type Target = BridgeKeys;
    fn deref(&self) -> &BridgeKeys {
        &self.0
    }
}

/// The counters of the calls accepted (a window of 64 below the highest): no call is accepted twice.
#[derive(Default)]
struct Seen {
    max: u64,
    bits: u64,
}

impl Seen {
    fn accept(&mut self, n: u64) -> bool {
        if n == 0 {
            return false;
        }
        if n > self.max {
            let shift = n - self.max;
            self.bits = if shift >= 64 { 0 } else { self.bits << shift };
            self.bits |= 1;
            self.max = n;
            return true;
        }
        let d = self.max - n;
        if d >= 64 || self.bits & (1u64 << d) != 0 {
            return false;
        }
        self.bits |= 1u64 << d;
        true
    }
}

/// A LEZ transaction built for the person's approval and not yet signed.
struct Pending {
    id: String,
    kind: &'static str,
    /// The message's borsh, its fee ceiling included: what is signed.
    bytes: Vec<u8>,
    signers: Vec<[u8; 32]>,
    hash: [u8; 32],
    expect: Option<WActor>,
    what: String,
    payer: u32,
    max_fee: u128,
    /// The step's reply, completed once the transaction is in a block.
    reply: Value,
    /// The public account whose bridge holding the reply reads again afterwards.
    holding_of: u32,
    made: Instant,
}

struct Net {
    lez: Lez,
    faucet: String,
    program: AccountId,
    cid: Hash,
    vault: [u8; 20],
    chain_id: u64,
    eth: String,
    from_block: u64,
    accounts: Vec<PublicAccount>,
    keys: WipedKeys,
    /// The view's channel (`seal.rs`): the calls' MAC key.
    channel: seal::Keys,
    seen: Seen,
    last_call: Instant,
    pending: Option<Pending>,
    account0: String,
}

static NET: Mutex<Option<Net>> = Mutex::new(None);

/// The session, also after a panic elsewhere held its lock: closing it must always work.
fn lock_net() -> MutexGuard<'static, Option<Net>> {
    NET.lock().unwrap_or_else(|p| p.into_inner())
}

fn with<T>(f: impl FnOnce(&Net) -> Result<T, String>) -> Result<T, String> {
    let g = lock_net();
    f(g.as_ref().ok_or("no network session: open one first")?)
}

fn with_mut<T>(f: impl FnOnce(&mut Net) -> Result<T, String>) -> Result<T, String> {
    let mut g = lock_net();
    f(g.as_mut().ok_or("no network session: open one first")?)
}

/// Makes this process non-dumpable (Linux): no core dump of it, and no other process of the person can read its memory
/// (ptrace, /proc/PID/mem). Called at seal_key, before any secret arrives, and at open; it stays so.
pub(crate) fn not_dumpable() {
    #[cfg(target_os = "linux")]
    // SAFETY: prctl(PR_SET_DUMPABLE, 0) changes only this process's own attribute and touches no memory.
    unsafe {
        libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0);
    }
}

/// Whether a network session is open.
pub(crate) fn is_open() -> bool {
    lock_net().is_some()
}

/// The open session's public account /0 (base58): every reply names it, and the view checks it is the one it opened.
pub(crate) fn account0() -> Option<String> {
    lock_net().as_ref().map(|n| n.account0.clone())
}

/// Ends the session: its keys are wiped as they are dropped. Whether one was open.
pub(crate) fn close() -> bool {
    lock_net().take().is_some()
}

/// Ends a session nobody has called for [`IDLE`]. Whether it did.
pub(crate) fn expire_idle() -> bool {
    let mut g = lock_net();
    if g.as_ref().is_some_and(|n| n.last_call.elapsed() >= idle_limit()) {
        *g = None;
        return true;
    }
    false
}

fn idle_limit() -> Duration {
    #[cfg(test)]
    {
        let ms = tests::IDLE_MS.load(std::sync::atomic::Ordering::SeqCst);
        if ms > 0 {
            return Duration::from_millis(ms);
        }
    }
    IDLE
}

/// Accepts call `n` of the session if its MAC holds under the view's key and `n` was not used before.
pub(crate) fn verify_call(n: u64, body: &str, tag: &[u8]) -> Result<(), String> {
    with_mut(|s| {
        if !s.channel.verify_call(n, body, tag) {
            return Err("refused: the call's MAC does not hold: only the view that opened this network session can call it".into());
        }
        if !s.seen.accept(n) {
            return Err(format!("refused: call {n} of this session was made already (a replay)"));
        }
        s.last_call = Instant::now();
        Ok(())
    })
}

/// The session's bridge keys, for `crate::keys` within one verified call of the view (`crate::with_session_keys`):
/// borrowed under the session's lock, never copied out. A caller holding it must not call back into this module's
/// session functions.
pub(crate) struct SessionKeys(MutexGuard<'static, Option<Net>>);

impl std::ops::Deref for SessionKeys {
    type Target = BridgeKeys;
    fn deref(&self) -> &BridgeKeys {
        &self.0.as_ref().expect("a SessionKeys holds an open session").keys
    }
}

pub(crate) fn session_keys() -> Result<SessionKeys, String> {
    let g = lock_net();
    if g.is_none() {
        return Err("no network session: open one first".into());
    }
    Ok(SessionKeys(g))
}

fn same_hex(a: &str, b: &str) -> bool {
    a.trim_start_matches("0x").eq_ignore_ascii_case(b.trim_start_matches("0x"))
}

fn eip55_upper(lower_hex: &str) -> Vec<bool> {
    use tiny_keccak::Hasher as _;
    let mut k = tiny_keccak::Keccak::v256();
    let mut d = [0u8; 32];
    k.update(lower_hex.as_bytes());
    k.finalize(&mut d);
    (0..lower_hex.len()).map(|i| ((d[i / 2] >> (if i % 2 == 0 { 4 } else { 0 })) & 0xf) >= 8).collect()
}

/// The EIP-55 (checksummed) form of an address.
pub(crate) fn eip55(a: &[u8; 20]) -> String {
    let lower = hex(a)[2..].to_string();
    let upper = eip55_upper(&lower);
    let mut out = String::from("0x");
    for (c, up) in lower.chars().zip(upper) {
        out.push(if up { c.to_ascii_uppercase() } else { c });
    }
    out
}

/// An Ethereum address, `0x` and 40 hex digits; a mixed-case one must pass EIP-55 (as the web wallet requires).
pub(crate) fn address(s: &str, what: &str) -> Result<[u8; 20], String> {
    let t = s.trim();
    let h = t.strip_prefix("0x").ok_or_else(|| format!("{what}: an address starts with 0x"))?;
    let b: [u8; 20] = unhex(h).ok().and_then(|v| v.try_into().ok()).ok_or_else(|| format!("{what}: 40 hex digits after 0x"))?;
    if h.chars().any(|c| c.is_ascii_uppercase()) && h.chars().any(|c| c.is_ascii_lowercase()) && eip55(&b)[2..] != *h {
        return Err(format!("{what}: {t} fails its EIP-55 checksum (a mistyped character?)"));
    }
    if b == [0u8; 20] {
        return Err(format!("{what}: the zero address"));
    }
    Ok(b)
}

/// An address the funds go to, which must be typed or pasted in its checksummed (EIP-55) form, as wallets show it: a
/// mistyped character is then caught before anything is signed (an all-lower- or all-upper-case address carries no
/// checksum). An address without a letter (no checksum is possible) is taken as it is.
pub(crate) fn checksummed_address(s: &str, what: &str) -> Result<[u8; 20], String> {
    let t = s.trim();
    if t.is_empty() {
        return Err(format!("{what}: enter the Ethereum address the release pays (yours, as your wallet shows it): there is no default"));
    }
    let b = address(t, what)?;
    let h = &t[2..];
    if h.chars().any(|c| c.is_ascii_alphabetic()) && eip55(&b)[2..] != *h {
        return Err(format!("{what}: enter the address in its checksummed form (EIP-55, upper and lower case as your wallet shows it), so that a mistyped character is caught: {}", eip55(&b)));
    }
    Ok(b)
}

/// What the network checks at open found: the deployment, its endpoints, and what was read.
pub(crate) struct Checked {
    dep: &'static Deployment,
    lez: Lez,
    faucet: String,
    vault: [u8; 20],
    cid: Hash,
    program: AccountId,
    head: u64,
    eth_block: u64,
    checks: Vec<String>,
}

/// The network's configuration, the vault on Ethereum and the bridge on LEZ, checked against [`KNOWN`] (the file's
/// comment says what each is).
fn check_network(base: &str, eth: &str) -> Result<Checked, String> {
    #[cfg(test)]
    if tests::SKIP_CHECKS.load(std::sync::atomic::Ordering::SeqCst) {
        return tests::checked_stand_in(base);
    }
    let mut checks = Vec::new();
    // 1. The network's own configuration must name a deployment this module knows, field by field.
    let (status, text) = get(&format!("{base}/wallet/config.json"))?;
    if status != 200 {
        return Err(format!("{base}/wallet/config.json: HTTP {status} (is this a Ferrata test network's address?)"));
    }
    let cfg: Value = serde_json::from_str(&text).map_err(|_| format!("{base}/wallet/config.json is not JSON"))?;
    let b = &cfg["bridge"];
    let program_b58 = b["program"].as_str().ok_or("the network's configuration names no bridge program")?;
    let dep = KNOWN.iter().find(|d| d.bridge_program == program_b58).ok_or_else(|| {
        format!(
            "unknown program id {program_b58}: this module serves only {} (bridge program {}); refused",
            KNOWN[0].name, KNOWN[0].bridge_program
        )
    })?;
    for (key, want) in [("configId", dep.config_id), ("vault", dep.vault)] {
        let got = b[key].as_str().unwrap_or("");
        if !same_hex(got, want) {
            return Err(format!("the network's configuration names {key} {got}, not the one this module knows for bridge program {program_b58} (0x{want}); refused"));
        }
    }
    if b["evmChainId"].as_u64() != Some(dep.evm_chain_id) {
        return Err(format!("the network's configuration names chain {}, not {} (Sepolia); refused", b["evmChainId"], dep.evm_chain_id));
    }
    if b["lezReady"] == false {
        return Err("the network says its bridge is not set up yet (lezReady: false)".into());
    }
    checks.push(format!("the network's configuration names {} (program, configuration, vault, chain)", dep.name));
    let lez = Lez { url: same_origin(base, cfg["rpc"].as_str().unwrap_or("/rpc"), "its RPC")? };
    let faucet = same_origin(base, cfg["faucet"].as_str().unwrap_or("/faucet"), "its faucet")?;
    let vault: [u8; 20] = fixed(&json!(dep.vault), "vault")?;
    let cid: Hash = fixed(&json!(dep.config_id), "config_id")?;
    let program = AccountId::new(lezkey::id_from_string(dep.bridge_program)?);

    // 2. Ethereum: the chain and the vault's configuration, at the endpoint the person uses.
    let mut v = crate::evm::Vault::new(&json!({ "via": "http", "rpc_url": eth }), vault, dep.from_block)?;
    let chain = v.chain_id()?;
    if chain != dep.evm_chain_id {
        return Err(format!("wrong chain id: {eth} answers chain {chain}, not Sepolia ({}); refused", dep.evm_chain_id));
    }
    let code = v.rpc.call("eth_getCode", json!([hex(&vault), "latest"]))?;
    if code.as_str().is_none_or(|c| c.len() <= 2) {
        return Err(format!("wrong vault: no contract at {} on chain {chain}; refused", hex(&vault)));
    }
    let held = crate::evm::word(&v.view("lezConfigId()", &[], None)?, 0)?;
    if held != cid {
        return Err(format!("wrong vault: the contract at {} holds configuration {}, not {}; refused", hex(&vault), hex(&held), hex(&cid)));
    }
    let eth_block = v.block_number()?;
    checks.push(format!("Ethereum: {eth} answers chain {chain}; the vault holds this configuration (block {eth_block})"));

    // 3. LEZ: the program's code and the configuration, paired with this vault and chain (read from the same host).
    let head = lez.head()?;
    let program_account = lez.account(program.value())?;
    let image: [u8; 32] = fixed(&json!(dep.bridge_image_id), "image id")?;
    let mut carries = false;
    for s in program_account["data"]["actor_states"].as_object().into_iter().flat_map(|m| m.values()) {
        if let Some(bytes) = state_bytes(s)? {
            carries |= bytes.len() >= 33 && bytes[1..33] == image;
        }
    }
    if !carries {
        return Err(format!("unknown program id: the bridge program's account on this LEZ network does not carry image id 0x{}; refused", dep.bridge_image_id));
    }
    let config = lez.state(&bw::config_actor(program, &cid))?.ok_or("the bridge's configuration is not on this LEZ network; refused")?;
    match BridgeState::try_from_slice(&config) {
        Ok(BridgeState::Config(ConfigState::Ready { config: c, .. })) => {
            if c.vault != vault || c.evm_chain_id != dep.evm_chain_id || c.bridge_image_id != image {
                return Err("the bridge's configuration on LEZ names another vault, chain or image; refused".into());
            }
        }
        Ok(BridgeState::Config(ConfigState::Pairing { .. })) => return Err("the bridge is still pairing with its vault: try again later".into()),
        _ => return Err("config(cid) on this LEZ network is not a bridge configuration; refused".into()),
    }
    checks.push(format!("LEZ (head {head}): the bridge program carries its image id 0x{}…; its configuration is paired with this vault (as this host reports it)", &dep.bridge_image_id[..12]));
    Ok(Checked { dep, lez, faucet, vault, cid, program, head, eth_block, checks })
}

/// Opens the session (see the file's comment for what is checked) with the view's channel (`seal.rs`): the phrase
/// sealed to it (`phrase_sealed: {ct, mac}`), or a new one made here (`generate: true`, its words sealed back in
/// `words_sealed`). Returns the session's record for `session.rs` (`s`; it holds no secret) and what the view shows.
pub(crate) fn open(params: &Value, channel: seal::Keys) -> Result<(Value, Value), String> {
    if is_open() {
        return Err(ALREADY_OPEN.into());
    }
    let base = endpoint(
        params["network"].as_str().ok_or("open_network needs `network`: the test network's address (https://…)")?,
        "network",
    )?;
    let eth = endpoint(params["eth_rpc"].as_str().filter(|s| !s.trim().is_empty()).unwrap_or(DEFAULT_SEPOLIA_RPC), "eth_rpc")?;
    // Every input is checked, and the network too, before the phrase is opened or a seed derived (review round 2, MF3:
    // a refused open leaves nothing of them behind).
    let sealed = match (params.get("phrase_sealed").filter(|p| !p.is_null()), params["generate"] == true) {
        (Some(_), true) => return Err("open_network takes either a sealed phrase or generate, not both".into()),
        (Some(p), false) => Some((unhex(p["ct"].as_str().ok_or("phrase_sealed.ct")?)?, unhex(p["mac"].as_str().ok_or("phrase_sealed.mac")?)?)),
        (None, true) => None,
        (None, false) => return Err("open_network needs your recovery phrase sealed (phrase_sealed), or generate: true for a new one".into()),
    };
    let depositor = match params["depositor"].as_str().filter(|s| !s.trim().is_empty()) {
        Some(a) => Some(address(a, "depositor")?),
        None => None,
    };
    if base.starts_with("https://") || eth.starts_with("https://") {
        crate::rpc::set_https(https_post);
    }
    let c = check_network(&base, &eth)?;

    // The phrase, only now: opened from its seal or made here (its bytes wiped when this function returns), then the
    // seed and the keys. The process is not dumpable since seal_key (no core dump of it, and no other process of the
    // person reads its memory: ptrace, /proc/PID/mem).
    not_dumpable();
    let (phrase, made): (Zeroizing<Vec<u8>>, bool) = match sealed {
        Some((ct, tag)) => (channel.open_phrase(&ct, &tag)?, false),
        None => {
            let words = lezkey::new_phrase()?;
            (Zeroizing::new(words.as_bytes().to_vec()), true)
        }
    };
    let seed = lezkey::seed_of_phrase(std::str::from_utf8(&phrase).map_err(|_| "not a valid BIP-39 English recovery phrase (not text)".to_string())?)?;
    let accounts = lezkey::public_accounts(&seed, GAP)?;
    let private = lezkey::private_account_ids(&seed, GAP);
    let keys = {
        let bridge_seed = Zeroizing::new(lezkey::hmac256(seed.as_ref(), crate::BRIDGE_SEED_LABEL));
        WipedKeys::new(BridgeKeys::from_bridge_seed(bridge_seed.as_ref()))
    };
    drop(seed);
    let account0 = lezkey::id_string(&accounts[0].id);
    let s = json!({
        "chain_id": c.dep.evm_chain_id,
        "vault": hex(&c.vault),
        "config_id": hex(&c.cid),
        "program": hex(c.program.value()),
        "depositor": depositor.map(|d| hex(&d)),
        "gap": GAP,
        "public_accounts": accounts.iter().map(|a| hex(&a.id)).collect::<Vec<_>>(),
        "private_accounts": private.iter().map(|p| hex(p)).collect::<Vec<_>>(),
        "proofs": "none",
        "eth_rpc": eth,
        "sequencer": c.lez.url,
        "network": base,
        "network_session": true,
        "from_block": c.dep.from_block,
    });
    let mut summary = json!({
        "network": c.dep.name,
        "address": base,
        "lez_rpc": c.lez.url,
        "eth_rpc": eth,
        "chain_id": c.dep.evm_chain_id,
        "vault": hex(&c.vault),
        "config_id": hex(&c.cid),
        "program": c.dep.bridge_program,
        "lez_head": c.head,
        "eth_block": c.eth_block,
        "checks": c.checks,
        "account0": account0,
        "public_accounts": accounts.iter().map(|a| json!({ "index": a.index, "id": lezkey::id_string(&a.id) })).collect::<Vec<_>>(),
        "private_accounts": private.iter().enumerate().map(|(i, p)| json!({ "index": i, "id": hex(p) })).collect::<Vec<_>>(),
        "depositor": depositor.map(|d| hex(&d)),
        "phrase": PHRASE_NOTE,
        "idle_minutes": IDLE.as_secs() / 60,
    });
    if made {
        // Shown once by the view: sealed to it, the module keeps no copy of the words.
        let (ct, tag) = channel.seal_words(&phrase);
        summary["words_sealed"] = json!({ "ct": hex(&ct), "mac": hex(&tag) });
    }
    drop(phrase);
    let mut g = lock_net();
    if g.is_some() {
        return Err(ALREADY_OPEN.into());
    }
    *g = Some(Net {
        lez: c.lez,
        faucet: c.faucet,
        program: c.program,
        cid: c.cid,
        vault: c.vault,
        chain_id: c.dep.evm_chain_id,
        eth,
        from_block: c.dep.from_block,
        accounts,
        keys,
        channel,
        seen: Seen::default(),
        last_call: Instant::now(),
        pending: None,
        account0,
    });
    Ok((s, summary))
}

fn lez() -> Result<Lez, String> {
    with(|n| Ok(n.lez.clone()))
}

/// The session's public account `/index`: (id, public key).
pub(crate) fn public_account(index: u32) -> Result<([u8; 32], [u8; 32]), String> {
    with(|n| n.accounts.get(index as usize).map(|a| (a.id, a.pk)).ok_or_else(|| format!("account /{index} is outside the recovery window")))
}

/// Which of the session's public accounts `id` is, if one.
pub(crate) fn index_of(id: &[u8; 32]) -> Option<u32> {
    with(|n| Ok(n.accounts.iter().find(|a| &a.id == id).map(|a| a.index))).ok().flatten()
}

/// Whether the actor has a state on LEZ (a nullifier spent, a burn written).
pub(crate) fn exists(actor: &WActor) -> Result<bool, String> {
    Ok(lez()?.state(actor)?.is_some())
}

/// The amount of `token` account `/index` holds on the bridge (its `holder(cid, account)`).
pub(crate) fn holding(index: u32, token: &[u8; 20]) -> Result<u128, String> {
    let (id, _) = public_account(index)?;
    let (program, cid) = with(|n| Ok((n.program, n.cid)))?;
    Ok(match lez()?.state(&bw::holder_actor(program, &cid, AccountId::new(id)))? {
        Some(b) => match BridgeState::try_from_slice(&b) {
            Ok(BridgeState::Holder(h)) => h.holdings.iter().find(|x| x.config_id == cid && x.token == *token).map_or(0, |x| x.amount),
            _ => 0,
        },
        None => 0,
    })
}

/// The first burn counter whose `burn(cid, r_i)` is not written (a refused burn writes none), and the wallet's burns
/// before it, read from their actors (no history scan): `(burn counter, note)`. The tags are made under the session's
/// lock, a few at a time; kView is never copied out of it.
pub(crate) fn own_burns() -> Result<(u64, Vec<(u64, bw::Note)>), String> {
    let lez = lez()?;
    let (program, cid) = with(|n| Ok((n.program, n.cid)))?;
    let mut out = Vec::new();
    let mut from = 0u64;
    while from < 256 {
        let tags: Vec<(u64, Hash)> = with(|n| Ok((from..(from + 16).min(256)).map(|i| (i, outbound::burn_tag(&n.keys.view, &cid, i))).collect()))?;
        for (i, r) in tags {
            match lez.state(&bw::burn_actor(program, &cid, &r))? {
                Some(b) => match BridgeState::try_from_slice(&b) {
                    Ok(BridgeState::Burn(rec)) => out.push((i, rec.note)),
                    _ => return Err(format!("burn(cid, r_{i}) is not a burn record")),
                },
                None => return Ok((i, out)),
            }
        }
        from += 16;
    }
    Ok((256, out))
}

/// `module.*` of a chain service, answered here for the network session: the LEZ side alone (`lez_only`; the session
/// reads Ethereum itself through its `eth` path).
pub(crate) fn answer(method: &str, params: &Value) -> Result<Value, String> {
    match method {
        "module.info" => Ok(json!({ "lez_head": lez()?.head()?, "relayer": null, "sequencer": lez()?.url })),
        "module.inputs" => inputs(params),
        // A private account's state is LEZ's wallet's to decrypt: not read in this session.
        "module.private" => Ok(json!({ "known": false, "bridge_state": "0x", "not_read": "private accounts are read by LEZ's wallet (the ferrata tool)" })),
        m => Err(format!("{m}: not part of the network session")),
    }
}

fn inputs(params: &Value) -> Result<Value, String> {
    let lez = lez()?;
    let (program, cid, vault, chain_id, eth, from_block) = with(|n| Ok((n.program, n.cid, n.vault, n.chain_id, n.eth.clone(), n.from_block)))?;
    let token: [u8; 20] = if params["token"].is_null() { [0u8; 20] } else { fixed(&params["token"], "token")? };
    let state_hex = |a: &WActor| -> Result<String, String> { Ok(lez.state(a)?.map_or_else(|| "0x".to_string(), |b| hex(&b))) };
    let mut out = json!({
        "program": hex(program.value()),
        "config_id": hex(&cid),
        "chain_id": chain_id,
        "vault": hex(&vault),
        "config_state": state_hex(&bw::config_actor(program, &cid))?,
        "token_state": state_hex(&bw::token_actor(program, &cid, &token))?,
        "touched_privately": [],
        "claimed_plaintext": [],
        "lez_only": true,
        "read_by": "ferrata_bridge_module (network session): the bridge's actors read one by one through the network's RPC, no history scan",
    });
    // The newest imported root: probed from the vault's leaf count down (at most 64 counts, as the web wallet looks).
    let count = crate::evm::Vault::new(&json!({ "via": "http", "rpc_url": eth }), vault, from_block)?.next_leaf_index(None)?;
    let mut imported = Vec::new();
    let low = count.saturating_sub(63).max(1);
    for k in (low..=count).rev() {
        if lez.state(&bw::root_actor(program, &cid, k))?.is_some() {
            imported.push(k);
            break;
        }
    }
    out["imported"] = json!(imported);
    out["vault_leaf_count"] = json!(count);
    if let Some(n) = params["leaf_count"].as_u64().or_else(|| imported.last().copied()) {
        out["leaf_count"] = json!(n);
        out["root_state"] = json!(state_hex(&bw::root_actor(program, &cid, n))?);
    }
    if let Some(nf) = params["nullifier"].as_str() {
        let nf: Hash = fixed(&json!(nf), "nullifier")?;
        out["nullifier_state"] = json!(state_hex(&bw::nullifier_actor(program, &cid, chain_id, &vault, &nf))?);
    }
    if let Some(holders) = params["public_holders"].as_array() {
        let mut states = Vec::new();
        for h in holders {
            let id: [u8; 32] = fixed(h, "a public holder")?;
            states.push(json!({ "account": hex(&id), "state": state_hex(&bw::holder_actor(program, &cid, AccountId::new(id)))? }));
        }
        out["public_holder_states"] = json!(states);
    }
    // The wallet's own burns by counter (`burn_index` here is the counter i: the zone's index needs its history).
    let (_, burns) = own_burns()?;
    out["burns"] = json!(burns
        .iter()
        .map(|(i, note)| json!({ "burn_index": i, "note": {
            "r": hex(&note.r), "memo": hex(&note.memo), "mac": hex(&note.mac), "token": hex(&note.token),
            "amount": hex(&note.amount), "c_out": hex(&note.c_out) } }))
        .collect::<Vec<_>>());
    Ok(out)
}

fn named(id: &[u8; 32]) -> String {
    match index_of(id) {
        Some(i) => format!("public /{i} ({})", lezkey::id_string(id)),
        None => lezkey::id_string(id),
    }
}

/// Builds the module's message as a LEZ public transaction paid (and co-signed) by public account `/payer`, for the
/// person's approval, and signs nothing: the transaction waits under a new id (one at a time; a new one replaces it)
/// until [`confirm`]. Returns what the approval shows: the call, its payer and signers, the fee ceiling, the message
/// hash. `expect`: the actor the call writes (a claim's nullifier, a burn's record): written means applied. `reply`: the
/// step's reply, completed by `confirm`; `holding_of`: the account whose bridge holding it reads again.
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare(kind: &'static str, message: &[u8], accounts: &Value, root_index: usize, what: &str, payer: u32,
                      expect: Option<WActor>, gas_limit: u64, reply: Value, holding_of: u32) -> Result<Value, String> {
    let lez = lez()?;
    let (payer_id, _) = public_account(payer)?;
    let list = accounts.as_array().ok_or("accounts")?;
    let mut actors = Vec::new();
    let mut evidence = Vec::new();
    let mut signers = vec![payer_id];
    for (i, a) in list.iter().enumerate() {
        let actor = lezkey::Actor { account: fixed(&a["account"], &format!("accounts[{i}].account"))?, program: fixed(&a["program"], &format!("accounts[{i}].program"))? };
        match a["kind"].as_str().unwrap_or("") {
            "pda" => evidence.push(Evidence::Pda { program: actor.program, seed: fixed(&a["seed"], &format!("accounts[{i}].seed"))? }),
            "signer" => {
                if index_of(&actor.account).is_none() {
                    return Err("the call needs the signature of an account this phrase does not hold".into());
                }
                signers.push(actor.account);
            }
            "public" => {}
            "private_self" => return Err(PRIVATE_NEEDS_PROOF.into()),
            k => return Err(format!("accounts[{i}]: kind {k:?}")),
        }
        actors.push(actor);
    }
    let to = *actors.get(root_index).ok_or("root_index is outside the call's accounts")?;
    signers.sort();
    signers.dedup();
    let mut nonces = Vec::new();
    for s in &signers {
        nonces.push((*s, lez.nonce(s)?));
    }
    let (be, bs) = lez.base_fees()?;
    let build = |max_fee: u128| Message {
        actors: actors.clone(),
        signers: signers.clone(),
        to,
        data: message.to_vec(),
        fee: Some(Fee { payer: payer_id, gas_limit, tip: 0, max_fee }),
        nonces: nonces.clone(),
        evidence: evidence.clone(),
    };
    let size = lezkey::encode_transaction(&lezkey::encode_message(&build(0)), &vec![([0u8; 64], [0u8; 32]); signers.len()]).len() + 1;
    let (_, max_fee) = lezkey::fee_for(gas_limit, size, be, bs);
    if max_fee > MAX_FEE {
        return Err(format!(
            "refused: at the network's fee state this call may cost up to {max_fee} units (10^-9 LGO), above this module's ceiling of {MAX_FEE}: nothing was signed"
        ));
    }
    let balance = lez.balance(&payer_id)?;
    if balance < max_fee {
        return Err(format!("{what} needs up to {max_fee} units (10^-9 LGO) in LEZ fees; account /{payer} holds {balance}: use the faucet first"));
    }
    let bytes = lezkey::encode_message(&build(max_fee));
    let hash = lezkey::message_hash(&bytes);
    let mut raw = [0u8; 16];
    getrandom::getrandom(&mut raw).map_err(|e| format!("no randomness from the operating system: {e}"))?;
    let id = hex(&raw)[2..].to_string();
    let signer_names: Vec<String> = signers.iter().map(named).collect();
    let record = json!({
        "pending_id": id,
        "kind": kind,
        "what": what,
        "payer": named(&payer_id),
        "signers": signer_names,
        "gas_limit": gas_limit,
        "max_fee": max_fee.to_string(),
        "fee_unit": "10^-9 LGO",
        "payer_balance": balance.to_string(),
        "message_hash": hex(&hash),
        "expires_in_s": PENDING_TTL.as_secs(),
        "signed": false,
    });
    with_mut(|n| {
        n.pending = Some(Pending { id, kind, bytes, signers, hash, expect, what: what.to_string(), payer, max_fee, reply, holding_of, made: Instant::now() });
        Ok(())
    })?;
    Ok(record)
}

const NOT_PENDING: &str = "refused: no transaction waits for approval under this id (it was sent, replaced by a newer one, or \
the session closed): prepare it again";

/// The kind ("claim", "burn") of the transaction waiting under `id`.
pub(crate) fn pending_kind(id: &str) -> Result<&'static str, String> {
    with(|n| n.pending.as_ref().filter(|p| p.id == id).map(|p| p.kind).ok_or_else(|| NOT_PENDING.to_string()))
}

/// Signs the transaction waiting under `id` exactly as [`prepare`] built it (once: it no longer waits), sends it to
/// the network's RPC and waits for its block; the step's reply, completed. Run in a job's thread.
pub(crate) fn confirm(id: &str) -> Result<Value, String> {
    let lez = lez()?;
    let (p, tx) = with_mut(|n| {
        match &n.pending {
            Some(p) if p.id == id => {}
            _ => return Err(NOT_PENDING.into()),
        }
        let p = n.pending.take().expect("checked above");
        if p.made.elapsed() >= PENDING_TTL {
            return Err("refused: the approval waited more than five minutes (nonces and fees move): prepare it again".into());
        }
        let witnesses = p
            .signers
            .iter()
            .map(|s| {
                let a = n.accounts.iter().find(|a| &a.id == s).ok_or("a signer outside the session")?;
                Ok((a.sign(&p.hash)?, a.pk))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let tx = lezkey::encode_transaction(&p.bytes, &witnesses);
        Ok((p, tx))
    })?;
    let (wire, hash) = lezkey::wire(&tx);
    crate::jobs::phase("sent", json!({ "by": "ferrata_bridge_module (network session): signed here after your approval, sent to the network's RPC", "tx_hash": hash }));
    let answered = lez.call("sendTransaction", json!([wire]))?;
    if answered.as_str().is_some_and(|a| !a.trim_start_matches("0x").eq_ignore_ascii_case(&hash)) {
        return Err(format!("{}: the RPC named the transaction {answered}, not {hash}", p.what));
    }
    let t0 = Instant::now();
    let block = loop {
        if let Some(b) = lez.block_of(&hash)? {
            break b;
        }
        if t0.elapsed() > Duration::from_secs(180) {
            return Err(format!("{}: transaction {hash} is not in a LEZ block after three minutes; check before trying again", p.what));
        }
        std::thread::sleep(Duration::from_secs(2));
    };
    let applied = match &p.expect {
        Some(a) => Some(lez.state(a)?.is_some()),
        None => None,
    };
    let mut out = p.reply;
    out["settled"] = json!(applied.unwrap_or(true));
    out["reverted"] = json!(applied.map(|x| !x));
    out["what"] = json!(format!("{} (block {block}, transaction {hash})", p.what));
    out["tx_hash"] = json!(hash);
    out["block"] = json!(block);
    out["tx_bytes"] = json!(tx.len());
    out["max_fee"] = json!(p.max_fee.to_string());
    out["payer"] = json!(format!("public /{}", p.payer));
    out["approved"] = json!({ "pending_id": p.id, "message_hash": hex(&p.hash) });
    out["balance"] = json!(holding(p.holding_of, &[0u8; 20]).map_or_else(|e| format!("not read again: {e}"), |h| h.to_string()));
    Ok(out)
}

/// One claim at the network's faucet for public account `/index` (its id and public key: a never-used account needs
/// the key as LEZ's admission evidence). The faucet's limits are its own (one claim per account per day). Its credit
/// is looked for while the call has time; the reply says when it was not seen yet.
pub(crate) fn faucet(index: u32) -> Result<Value, String> {
    let (id, pk) = public_account(index)?;
    let (url, lez) = with(|n| Ok((n.faucet.clone(), n.lez.clone())))?;
    let before = lez.balance(&id).unwrap_or(0);
    let body = json!({ "account": lezkey::id_string(&id), "public_key": hex(&pk)[2..] }).to_string();
    let (status, text, retry) = post(&url, &body, Duration::from_secs(120))?;
    let reply: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({ "ok": false, "error": format!("HTTP {status}") }));
    if reply["ok"] != true {
        let wait = retry.map(|s| format!(" (again in {s} s)")).unwrap_or_default();
        return Err(format!("the faucet answered: {}{wait}", reply["error"].as_str().unwrap_or("no reason given")));
    }
    let mut after = lez.balance(&id).unwrap_or(before);
    let t0 = Instant::now();
    while after <= before && t0.elapsed() < Duration::from_secs(30) && budget(Duration::MAX).is_ok_and(|left| left > Duration::from_secs(4)) {
        std::thread::sleep(Duration::from_secs(2));
        after = lez.balance(&id).unwrap_or(after);
    }
    Ok(json!({
        "account": index,
        "id": lezkey::id_string(&id),
        "tx_hash": reply["hash"],
        "credited": reply["credited"],
        "balance_before": before.to_string(),
        "balance": after.to_string(),
        "seen": after > before,
        "unit": "10^-9 LGO",
        "faucet": url,
    }))
}

/// The LGO balances (LEZ's native token, for fees) of the session's public accounts.
pub(crate) fn lgo_balances() -> Result<Vec<Value>, String> {
    let lez = lez()?;
    let ids = with(|n| Ok(n.accounts.iter().map(|a| (a.index, a.id)).collect::<Vec<_>>()))?;
    let mut out = Vec::new();
    for (index, id) in ids {
        out.push(json!({ "index": index, "id": lezkey::id_string(&id), "balance": lez.balance(&id)?.to_string() }));
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::{Read as _, Write as _};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    /// Tests that open the network session without a network (session.rs's): the checks are replaced by
    /// [`checked_stand_in`] while this is set.
    pub(crate) static SKIP_CHECKS: AtomicBool = AtomicBool::new(false);
    /// A shorter idle limit for a test (milliseconds; 0: [`IDLE`]).
    pub(crate) static IDLE_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    pub(crate) const PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    /// The network as checked, for a stand-in LEZ RPC at `base/rpc` (no Ethereum side).
    pub(crate) fn checked_stand_in(base: &str) -> Result<Checked, String> {
        let dep = &KNOWN[0];
        Ok(Checked {
            dep,
            lez: Lez { url: format!("{base}/rpc") },
            faucet: format!("{base}/faucet"),
            vault: fixed(&json!(dep.vault), "vault")?,
            cid: fixed(&json!(dep.config_id), "config_id")?,
            program: AccountId::new(lezkey::id_from_string(dep.bridge_program)?),
            head: 5,
            eth_block: 1,
            checks: vec!["(a test: the network's checks were not run)".into()],
        })
    }

    /// The view's side of a new channel: open_network's parameters (`params` with key_id, view_key and the sealed
    /// phrase, or generate) and the view's keys; the module's one-time key waits in seal.rs.
    pub(crate) fn sealed(mut params: Value, phrase: Option<&str>) -> (Value, seal::Keys) {
        let (id, module_pk) = seal::new_key().unwrap();
        let (view, view_pk) = seal::view_side(&[9u8; 32], &module_pk);
        params["key_id"] = json!(id);
        params["view_key"] = json!(hex(&view_pk));
        match phrase {
            Some(p) => {
                let (ct, tag) = view.seal_phrase(p.as_bytes());
                params["phrase_sealed"] = json!({ "ct": hex(&ct), "mac": hex(&tag) });
            }
            None => params["generate"] = json!(true),
        }
        (params, view)
    }

    /// `open` as the module runs it: the one-time key of `params` agreed first.
    fn open_sealed(params: &Value) -> Result<(Value, Value), String> {
        let view_pk: [u8; 32] = fixed(&params["view_key"], "view_key")?;
        let (channel, _) = seal::agree(params["key_id"].as_u64().unwrap(), &view_pk)?;
        open(params, channel)
    }

    /// A stand-in LEZ JSON-RPC at `<url>/rpc`: public account /0 of the test phrase holds `held` base units of ETH on
    /// the bridge; any other account has no state until a `sendTransaction` arrives (then every one has, so that the
    /// call's actor reads as written). Each request waits `delay` first. Returns the URL and the methods asked, with
    /// the transactions sent.
    pub(crate) struct StandIn {
        pub url: String,
        pub methods: Arc<Mutex<Vec<String>>>,
        pub sent: Arc<Mutex<Vec<String>>>,
        pub base_fee: Arc<std::sync::atomic::AtomicU64>,
    }

    pub(crate) fn stand_in_lez(holder: [u8; 32], held: u128, delay: Duration) -> StandIn {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        let methods = Arc::new(Mutex::new(Vec::new()));
        let sent = Arc::new(Mutex::new(Vec::new()));
        let base_fee = Arc::new(std::sync::atomic::AtomicU64::new(8));
        let (m, tx, fee) = (methods.clone(), sent.clone(), base_fee.clone());
        let dep = &KNOWN[0];
        let cid: Hash = fixed(&json!(dep.config_id), "c").unwrap();
        let holder_state = borsh::to_vec(&BridgeState::Holder(bw::HolderState {
            holdings: vec![bw::Holding { config_id: cid, token: [0u8; 20], amount: held }],
            pending: None,
        }))
        .unwrap();
        std::thread::spawn(move || {
            for s in l.incoming() {
                let Ok(mut s) = s else { break };
                let (m, tx, holder_state, fee) = (m.clone(), tx.clone(), holder_state.clone(), fee.clone());
                std::thread::spawn(move || {
                    let mut buf = Vec::new();
                    let mut chunk = [0u8; 8192];
                    let body = loop {
                        let n = s.read(&mut chunk).unwrap_or(0);
                        buf.extend_from_slice(&chunk[..n]);
                        if let Some(h) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                            let head = String::from_utf8_lossy(&buf[..h]).to_lowercase();
                            let len = head.lines().find_map(|l| l.strip_prefix("content-length:").and_then(|v| v.trim().parse::<usize>().ok())).unwrap_or(0);
                            if buf.len() >= h + 4 + len {
                                break buf[h + 4..h + 4 + len].to_vec();
                            }
                        }
                        if n == 0 {
                            break Vec::new();
                        }
                    };
                    std::thread::sleep(delay);
                    let req: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                    let method = req["method"].as_str().unwrap_or("").to_string();
                    m.lock().unwrap().push(method.clone());
                    let program = KNOWN[0].bridge_program;
                    let result = match method.as_str() {
                        "getAccountsNonces" => json!([0]),
                        "getFeeState" => json!({ "base_fee_exec": fee.load(Ordering::SeqCst), "base_fee_stor": 8 }),
                        "getAccountBalance" => json!(10_000_000_000u64),
                        "getLastBlockId" => json!(5),
                        "getTransaction" => json!(["tx", 6]),
                        "sendTransaction" => {
                            tx.lock().unwrap().push(req["params"][0].as_str().unwrap_or("").to_string());
                            Value::Null
                        }
                        "getAccount" => {
                            let id = req["params"][0].as_str().unwrap_or("");
                            let state = if id == lezkey::id_string(&holder) {
                                json!(holder_state)
                            } else if !tx.lock().unwrap().is_empty() {
                                json!([5, 0, 0])
                            } else {
                                Value::Null
                            };
                            json!({ "data": { "actor_states": { program: state } } })
                        }
                        _ => Value::Null,
                    };
                    let text = json!({ "jsonrpc": "2.0", "id": req["id"], "result": result }).to_string();
                    let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", text.len(), text);
                });
            }
        });
        StandIn { url, methods, sent, base_fee }
    }

    #[test]
    fn endpoints_https_or_this_machine() {
        assert_eq!(endpoint("https://example.org/", "network").unwrap(), "https://example.org");
        assert!(endpoint("http://127.0.0.1:8080", "network").is_ok());
        assert!(endpoint("http://localhost:3040/rpc", "network").is_ok());
        assert!(endpoint("http://[::1]:3040", "network").is_ok());
        for bad in ["http://example.org", "http://10.0.0.5:8545", "http://192.168.1.2", "ftp://example.org", "example.org",
                    "https://user@example.org", "http://127.0.0.1.example.org", "https://n.example/?x=1", "https://n.example/#a"] {
            assert!(endpoint(bad, "network").is_err(), "{bad}");
        }
        assert!(same_origin("https://n.example", "//evil.example/rpc", "its RPC").is_err());
        assert!(same_origin("https://n.example", "https://evil.example/rpc", "its RPC").is_err());
        assert_eq!(same_origin("https://n.example", "/rpc", "its RPC").unwrap(), "https://n.example/rpc");
    }

    /// A stand-in network on 127.0.0.1: its `/wallet/config.json`, and an Ethereum JSON-RPC (any other path) answering
    /// `chain` for eth_chainId and `held` for the vault's lezConfigId(). One request per connection.
    fn stand_in(config: Value, chain: &'static str, held: String) -> String {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        std::thread::spawn(move || {
            for s in l.incoming() {
                let Ok(mut s) = s else { break };
                let mut buf = Vec::new();
                let mut tmp = [0u8; 8192];
                loop {
                    let n = s.read(&mut tmp).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                    if let Some(h) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..h]).to_lowercase();
                        let len = head.lines().find_map(|l| l.strip_prefix("content-length:").and_then(|v| v.trim().parse::<usize>().ok())).unwrap_or(0);
                        if buf.len() >= h + 4 + len {
                            break;
                        }
                    }
                }
                let text = String::from_utf8_lossy(&buf).to_string();
                let body = if text.starts_with("GET /wallet/config.json") {
                    config.to_string()
                } else {
                    let b = text.find("\r\n\r\n").map_or("", |i| &text[i + 4..]);
                    let req: Value = serde_json::from_str(b).unwrap_or(Value::Null);
                    let result = match req["method"].as_str().unwrap_or("") {
                        "eth_chainId" => json!(chain),
                        "eth_getCode" => json!("0x6080"),
                        "eth_call" => json!(held.clone()),
                        "eth_blockNumber" => json!("0x10"),
                        _ => Value::Null,
                    };
                    json!({ "jsonrpc": "2.0", "id": req["id"], "result": result }).to_string()
                };
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
            }
        });
        base
    }

    fn live_config() -> Value {
        json!({ "rpc": "/rpc", "faucet": "/faucet", "bridge": { "program": KNOWN[0].bridge_program, "configId": format!("0x{}", KNOWN[0].config_id),
                "vault": "0x19499061CeC0BaC2a3E1869Ba5AE6B403aA82787", "evmChainId": 11155111, "lezReady": true, "fromBlock": 12196188 } })
    }

    fn open_at(base: &str) -> String {
        let _s = crate::jobs::tests::serial();
        let (params, _) = sealed(json!({ "network": base, "eth_rpc": format!("{base}/eth") }), Some(PHRASE));
        let e = open_sealed(&params).err().expect("refused");
        assert!(!e.contains("abandon"), "the phrase must never appear in an error: {e}");
        e
    }

    #[test]
    fn refuses_an_unknown_program_id() {
        let mut c = live_config();
        c["bridge"]["program"] = json!("11111111111111111111111111111111");
        let e = open_at(&stand_in(c, "0xaa36a7", format!("0x{}", KNOWN[0].config_id)));
        assert!(e.contains("unknown program id 11111111111111111111111111111111"), "{e}");
    }

    #[test]
    fn refuses_a_vault_it_does_not_know() {
        let mut c = live_config();
        c["bridge"]["vault"] = json!("0x00000000000000000000000000000000000000aa");
        let e = open_at(&stand_in(c, "0xaa36a7", format!("0x{}", KNOWN[0].config_id)));
        assert!(e.contains("names vault 0x00000000000000000000000000000000000000aa") && e.contains("refused"), "{e}");
    }

    #[test]
    fn refuses_a_wrong_chain_id() {
        let e = open_at(&stand_in(live_config(), "0x1", format!("0x{}", KNOWN[0].config_id)));
        assert!(e.contains("wrong chain id") && e.contains("answers chain 1,"), "{e}");
    }

    #[test]
    fn refuses_a_vault_that_holds_another_configuration() {
        let e = open_at(&stand_in(live_config(), "0xaa36a7", format!("0x{}", "11".repeat(32))));
        assert!(e.contains("wrong vault") && e.contains("holds configuration 0x1111"), "{e}");
    }

    #[test]
    fn refuses_plain_http_to_another_host() {
        let _s = crate::jobs::tests::serial();
        let (params, _) = sealed(json!({ "network": "http://example.org" }), Some(PHRASE));
        let e = open_sealed(&params).err().unwrap();
        assert!(e.contains("plain http:// is refused"), "{e}");
        let base = stand_in(live_config(), "0xaa36a7", format!("0x{}", KNOWN[0].config_id));
        let (params, _) = sealed(json!({ "network": base, "eth_rpc": "http://10.1.2.3:8545" }), Some(PHRASE));
        let e = open_sealed(&params).err().unwrap();
        assert!(e.contains("eth_rpc") && e.contains("plain http:// is refused"), "{e}");
        // Neither reached anything: no session.
        assert!(!is_open());
    }

    #[test]
    fn a_bad_phrase_is_refused_without_naming_a_word() {
        let _s = crate::jobs::tests::serial();
        // The phrase is opened only after the network's checks (review round 2, MF3): those pass here (a stand-in).
        let (params, _) = sealed(json!({ "network": "http://127.0.0.1:9", "eth_rpc": "http://127.0.0.1:9" }), Some("abandon abandon zebra"));
        SKIP_CHECKS.store(true, Ordering::SeqCst);
        let e = open_sealed(&params).err().unwrap();
        SKIP_CHECKS.store(false, Ordering::SeqCst);
        assert!(e.contains("not a valid BIP-39") && !e.contains("zebra") && !e.contains("abandon"), "{e}");
    }

    #[test]
    fn addresses_and_eip55() {
        assert!(address("0x19499061CeC0BaC2a3E1869Ba5AE6B403aA82787", "a").is_ok());
        assert!(address("0x19499061cec0bac2a3e1869ba5ae6b403aa82787", "a").is_ok());
        assert!(address("0x19499061CEC0BaC2a3E1869Ba5AE6B403aA82787", "a").is_err());
        assert!(address("0x0000000000000000000000000000000000000000", "a").is_err());
        assert!(address("19499061cec0bac2a3e1869ba5ae6b403aa82787", "a").is_err());
        assert_eq!(eip55(&address("0x19499061cec0bac2a3e1869ba5ae6b403aa82787", "a").unwrap()), "0x19499061CeC0BaC2a3E1869Ba5AE6B403aA82787");
    }

    /// Review M3: the release's address is the person's, typed in its checksummed form; none is made up.
    #[test]
    fn m3_a_release_address_must_be_entered_checksummed() {
        let e = checksummed_address("", "recipient").unwrap_err();
        assert!(e.contains("there is no default"), "{e}");
        let e = checksummed_address("0x19499061cec0bac2a3e1869ba5ae6b403aa82787", "recipient").unwrap_err();
        assert!(e.contains("checksummed form") && e.contains("0x19499061CeC0BaC2a3E1869Ba5AE6B403aA82787"), "{e}");
        assert!(checksummed_address("0x19499061CEC0BaC2a3E1869Ba5AE6B403aA82787", "recipient").unwrap_err().contains("fails its EIP-55"));
        assert!(checksummed_address(" 0x19499061CeC0BaC2a3E1869Ba5AE6B403aA82787 ", "recipient").is_ok());
        assert!(checksummed_address("0x1111111111111111111111111111111111111111", "recipient").is_ok(), "no letter: nothing to check");
    }

    #[test]
    fn each_call_counter_is_accepted_once() {
        let mut s = Seen::default();
        assert!(!s.accept(0));
        assert!(s.accept(1) && s.accept(3) && s.accept(2));
        assert!(!s.accept(2) && !s.accept(3) && !s.accept(1));
        assert!(s.accept(100));
        assert!(!s.accept(36), "below the window");
        assert!(s.accept(37) && !s.accept(37));
    }

    /// Review M2: the bridge keys are overwritten when the session ends.
    #[test]
    fn m2_the_bridge_keys_are_wiped() {
        let mut k = WipedKeys::new(BridgeKeys::from_bridge_seed(&[0x5a; 32]));
        let p: *const u8 = (&*k as *const BridgeKeys).cast();
        let n = std::mem::size_of::<BridgeKeys>();
        // SAFETY: the box is alive until `k` is dropped below; these reads are of its own bytes.
        let before = unsafe { std::slice::from_raw_parts(p, n) }.iter().filter(|b| **b != 0).count();
        assert!(before >= 32, "the keys are there: {before} of {n} bytes non-zero");
        k.wipe();
        // SAFETY: as above; `wipe` zeroed them in place.
        assert!(unsafe { std::slice::from_raw_parts(p, n) }.iter().all(|b| *b == 0), "every byte of the keys is zero after wipe");
        drop(k);
    }

    /// Review L2: a slow or stalling host cannot hold a call (and the `close` queued behind it) past the call's limit.
    #[test]
    fn l2_a_slow_host_cannot_hold_a_call_past_its_limit() {
        let net = stand_in_lez([0u8; 32], 0, Duration::from_millis(900));
        let (methods, lez) = (net.methods.clone(), Lez { url: format!("{}/rpc", net.url) });
        let t0 = Instant::now();
        let r = within(Duration::from_secs(2), || -> Result<(), String> {
            for _ in 0..20 {
                lez.head()?;
            }
            Ok(())
        });
        let took = t0.elapsed();
        let e = r.unwrap_err();
        assert!(e.contains("did not answer in time") || e.contains("timed out") || e.contains("imeout"), "{e}");
        assert!(took < Duration::from_secs(4), "the call ended after {took:?}, not at its limit");
        assert!(methods.lock().unwrap().len() <= 3, "it stopped asking: {:?}", methods.lock().unwrap());
        // Outside a call's deadline (a job's thread) the same reads are not cut short.
        assert!(lez.head().is_ok());
    }

    /// Review H1 and L3: a burn is built for the person's approval and signed only when confirmed, exactly as approved,
    /// once; a fee above the ceiling is refused. Also review M2: the session's record holds no bridge seed, and the
    /// process is not dumpable once keys are held.
    #[test]
    fn h1_a_lez_transaction_is_signed_only_when_confirmed_as_prepared() {
        let _s = crate::jobs::tests::serial();
        close();
        SKIP_CHECKS.store(true, Ordering::SeqCst);
        let seed = lezkey::seed_of_phrase(PHRASE).unwrap();
        let id0 = lezkey::public_accounts(&seed, 1).unwrap()[0].id;
        let dep = &KNOWN[0];
        let program = AccountId::new(lezkey::id_from_string(dep.bridge_program).unwrap());
        let cid: Hash = fixed(&json!(dep.config_id), "c").unwrap();
        let holder = *bw::holder_actor(program, &cid, AccountId::new(id0)).account_id.value();
        let net = stand_in_lez(holder, 1_000_000_000_000_000, Duration::ZERO);
        let (methods, sent) = (net.methods.clone(), net.sent.clone());
        let (params, _view) = sealed(json!({ "network": net.url, "eth_rpc": "http://127.0.0.1:9" }), Some(PHRASE));
        let (s, summary) = open_sealed(&params).unwrap();
        SKIP_CHECKS.store(false, Ordering::SeqCst);
        assert!(s.get("bridge_seed").is_none() && !s.to_string().contains("43bff369"), "no bridge seed in the session's record: {s}");
        assert_eq!(summary["account0"], json!(lezkey::id_string(&id0)));
        #[cfg(target_os = "linux")]
        // SAFETY: reads this process's own attribute.
        assert_eq!(unsafe { libc::prctl(libc::PR_GET_DUMPABLE, 0, 0, 0, 0) }, 0, "not dumpable while keys are held");

        let me = bw::own_actor(program, AccountId::new(id0));
        let r = [7u8; 32];
        let accounts = json!([
            { "account": hex(me.account_id.value()), "program": hex(me.program_account_id.value()), "kind": "signer" },
            { "account": hex(bw::config_actor(program, &cid).account_id.value()), "program": hex(program.value()), "kind": "pda",
              "seed": hex(bw::config_seed(&cid).as_bytes()) },
        ]);
        let record = prepare("burn", b"a burn", &accounts, 0, "Burn (public) of /0", 0, Some(bw::burn_actor(program, &cid, &r)), BURN_GAS,
                             json!({ "i": 0 }), 0).unwrap();
        assert_eq!(record["signed"], false);
        assert!(sent.lock().unwrap().is_empty(), "nothing is sent before the approval");
        assert!(!methods.lock().unwrap().contains(&"sendTransaction".to_string()));
        let id = record["pending_id"].as_str().unwrap().to_string();
        assert_eq!(pending_kind(&id).unwrap(), "burn");
        assert!(confirm("not-the-id").unwrap_err().starts_with("refused: no transaction waits"));
        let done = confirm(&id).unwrap();
        assert_eq!(done["settled"], true, "{done}");
        assert_eq!(done["approved"]["message_hash"], record["message_hash"]);
        // The one transaction sent is the message approved, signed by /0 over that hash.
        let wires = sent.lock().unwrap().clone();
        assert_eq!(wires.len(), 1);
        use base64::Engine as _;
        let raw = base64::engine::general_purpose::STANDARD.decode(&wires[0]).unwrap();
        let tx = &raw[1..];
        let (message, rest) = tx.split_at(tx.len() - 4 - 96);
        assert_eq!(rest[..4], 1u32.to_le_bytes(), "one witness");
        let hash = lezkey::message_hash(message);
        assert_eq!(hex(&hash), record["message_hash"].as_str().unwrap());
        let sig = k256::schnorr::Signature::try_from(&rest[4..68]).unwrap();
        let vk = k256::schnorr::VerifyingKey::from_bytes(&rest[68..100]).unwrap();
        assert!(vk.verify_raw(&hash, &sig).is_ok(), "signed by the payer over the approved hash");
        assert!(confirm(&id).unwrap_err().starts_with("refused: no transaction waits"), "used once");
        // Fees above the ceiling (review L3: the host sets the base fee): refused, nothing waits for approval.
        net.base_fee.store(1_000, Ordering::SeqCst);
        let e = prepare("burn", b"a burn", &accounts, 0, "Burn (public) of /0", 0, None, BURN_GAS, json!({}), 0).unwrap_err();
        assert!(e.contains("above this module's ceiling") && e.contains("nothing was signed"), "{e}");
        assert!(with(|n| Ok(n.pending.is_none())).unwrap());
        assert_eq!(sent.lock().unwrap().len(), 1);
        close();
        assert!(!is_open());
    }
}
