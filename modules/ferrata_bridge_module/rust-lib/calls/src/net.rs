//! The network session (`wallet {"method": "open_network"}`): the mini-app inside Logos Basecamp against a Ferrata
//! test network's public endpoints, with no chain service. The module itself reads LEZ through the network's gateway
//! (`<network>/rpc`, LEZ's JSON-RPC) and Ethereum through an endpoint the person chooses (a public Sepolia RPC by
//! default), both over HTTPS (rustls, Mozilla's roots); plain `http://` only to this machine.
//!
//! What it accepts: only a deployment this module knows ([`KNOWN`]), checked at open as the web wallet and the
//! `ferrata` tool check it, and refused on any difference: the network's own `/wallet/config.json` must name the
//! known bridge program, configuration, vault and chain; the Ethereum endpoint must answer that chain, with the vault's
//! contract holding that configuration (`lezConfigId()`); on LEZ the bridge program's account must carry the known
//! image id and the configuration must be paired (`config(cid)` ready) with that vault and chain.
//!
//! Keys: the person's recovery phrase (BIP-39), entered in the mini-app or made here (its words returned once, in the
//! reply that makes it). From it the session keeps, in this process's memory only and zeroized on `close`: the
//! signing keys of public accounts `/0`..`/7` (LEZ's key tree, `lezkey.rs`) and the bridge seed. The phrase and the
//! 64-byte seed are dropped as soon as those are derived. Nothing is written to disk; nothing is logged; no key leaves
//! the module (the network sees signatures, public keys and account ids). Ethereum transactions are never signed here:
//! a deposit goes through Logos Core's send stack (`tx_sender_module`, one approval in the Logos signer).
//!
//! What runs here: the public side, as the web wallet runs it in a page: the registry, the positions recovered from the
//! phrase, the faucet, a deposit planned to its approval, the plaintext claim of a deposit to a public account, the
//! public burn, and the release's hand-off (the release proof is the `ferrata` tool's, on the person's machine). A
//! private claim or burn needs a proof of LEZ's privacy circuit, which this module does not make.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use borsh::BorshDeserialize as _;
use ferrata_bridge_core::{outbound, BridgeKeys, Hash};
use ferrata_lez_bridge_wire::{self as bw, BridgeState, ConfigState};
use ferrata_wallet::{AccountId, Actor as WActor};
use serde_json::{json, Value};
use zeroize::Zeroizing;

use crate::lezkey::{self, Evidence, Fee, Message, PublicAccount};
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

const TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) const PRIVATE_NEEDS_PROOF: &str = "a private claim or burn needs a proof of LEZ's privacy circuit made with the \
account's keys, which this session does not make: run it with the `ferrata` tool from the same phrase (ferrata claim, \
ferrata burn), or choose the plaintext claim or public burn here";

// ------------------------------------------------------------------------------------------------ endpoints

fn host_of(authority: &str) -> &str {
    if let Some(rest) = authority.strip_prefix('[') {
        return rest.split(']').next().unwrap_or("");
    }
    authority.rsplit_once(':').map_or(authority, |(h, p)| if p.chars().all(|c| c.is_ascii_digit()) { h } else { authority })
}

/// An endpoint the session may use: `https://`, or plain `http://` to this machine only (127.0.0.0/8, ::1,
/// localhost). Returned without a trailing slash. No credentials in the URL.
pub(crate) fn endpoint(url: &str, what: &str) -> Result<String, String> {
    let u = url.trim().trim_end_matches('/');
    let (scheme, rest) = u.split_once("://").ok_or_else(|| format!("{what} {u:?}: not a URL (https://…)"))?;
    let authority = rest.split('/').next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') || u.chars().any(char::is_whitespace) {
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
    let r = agent(TIMEOUT).get(url).header("Accept", "application/json").call().map_err(|e| format!("{url}: {e}"))?;
    let (s, t, _) = body_of(r)?;
    Ok((s, t))
}

fn post(url: &str, body: &str, timeout: Duration) -> Result<(u16, String, Option<u64>), String> {
    let r = agent(timeout)
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
/// may answer 502/503 while the sequencer restarts: the call waits and tries again, at most four times (nothing reached
/// the sequencer on such an answer, so a resend of `sendTransaction` is safe).
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
                let wait = retry.map_or(600 * 2u64.pow(attempt), |s| s * 1000).min(10_000);
                std::thread::sleep(Duration::from_millis(wait));
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
        let v = &a["data"]["actor_states"][lezkey::id_string(program)];
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

/// An amount the RPC sends as a JSON number or a decimal string.
fn amount(v: &Value, what: &str) -> Result<u128, String> {
    match v {
        Value::Number(n) => n.as_u64().map(u128::from).ok_or_else(|| format!("{what}: not an amount")),
        Value::String(s) => s.parse::<u128>().map_err(|_| format!("{what}: not an amount")),
        _ => Err(format!("{what}: no amount")),
    }
}

// ------------------------------------------------------------------------------------------------ the session

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
    keys: BridgeKeys,
    _bridge_seed: Zeroizing<[u8; 32]>,
}

static NET: Mutex<Option<Net>> = Mutex::new(None);

fn with<T>(f: impl FnOnce(&Net) -> Result<T, String>) -> Result<T, String> {
    let g = NET.lock().map_err(|_| "the network session's lock is poisoned")?;
    f(g.as_ref().ok_or("no network session: open one first")?)
}

/// Ends the session: its keys are zeroized as they are dropped.
pub(crate) fn close() {
    if let Ok(mut g) = NET.lock() {
        *g = None;
    }
}

fn same_hex(a: &str, b: &str) -> bool {
    a.trim_start_matches("0x").eq_ignore_ascii_case(b.trim_start_matches("0x"))
}

/// An Ethereum address, `0x` and 40 hex digits; a mixed-case one must pass EIP-55 (as the web wallet requires).
pub(crate) fn address(s: &str, what: &str) -> Result<[u8; 20], String> {
    let t = s.trim();
    let h = t.strip_prefix("0x").ok_or_else(|| format!("{what}: an address starts with 0x"))?;
    let b: [u8; 20] = unhex(h).ok().and_then(|v| v.try_into().ok()).ok_or_else(|| format!("{what}: 40 hex digits after 0x"))?;
    if h.chars().any(|c| c.is_ascii_uppercase()) && h.chars().any(|c| c.is_ascii_lowercase()) {
        use tiny_keccak::Hasher as _;
        let lower = h.to_ascii_lowercase();
        let mut k = tiny_keccak::Keccak::v256();
        let mut d = [0u8; 32];
        k.update(lower.as_bytes());
        k.finalize(&mut d);
        for (i, c) in h.chars().enumerate() {
            let nibble = (d[i / 2] >> (if i % 2 == 0 { 4 } else { 0 })) & 0xf;
            if c.is_ascii_alphabetic() && c.is_ascii_uppercase() != (nibble >= 8) {
                return Err(format!("{what}: {t} fails its EIP-55 checksum (a mistyped character?)"));
            }
        }
    }
    if b == [0u8; 20] {
        return Err(format!("{what}: the zero address"));
    }
    Ok(b)
}

/// Opens the session (see the file's comment for what is checked). Returns the session's record for `session.rs`
/// (`s`, the same fields a chain service's `module.session` gives) and what the view shows.
pub(crate) fn open(params: &Value) -> Result<(Value, Value), String> {
    let base = endpoint(
        params["network"].as_str().ok_or("open_network needs `network`: the test network's address (https://…)")?,
        "network",
    )?;
    let eth = endpoint(params["eth_rpc"].as_str().filter(|s| !s.trim().is_empty()).unwrap_or(DEFAULT_SEPOLIA_RPC), "eth_rpc")?;
    // The phrase, entered or made here; its words leave the module only in this reply, and only when made here.
    let (phrase, made) = match (params["phrase"].as_str().filter(|s| !s.trim().is_empty()), params["generate"] == true) {
        (Some(_), true) => return Err("open_network takes either a phrase or generate, not both".into()),
        (Some(p), false) => (Zeroizing::new(p.to_string()), false),
        (None, true) => (lezkey::new_phrase()?, true),
        (None, false) => return Err("open_network needs your recovery phrase (or generate: true for a new one)".into()),
    };
    let seed = lezkey::seed_of_phrase(&phrase)?;
    let depositor = match params["depositor"].as_str().filter(|s| !s.trim().is_empty()) {
        Some(a) => Some(address(a, "depositor")?),
        None => None,
    };
    if base.starts_with("https://") || eth.starts_with("https://") {
        crate::rpc::set_https(https_post);
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
    let lez = Lez { url: same_origin(&base, cfg["rpc"].as_str().unwrap_or("/rpc"), "its RPC")? };
    let faucet = same_origin(&base, cfg["faucet"].as_str().unwrap_or("/faucet"), "its faucet")?;
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

    // 3. LEZ: the program's code and the configuration, paired with this vault and chain.
    let head = lez.head()?;
    let program_account = lez.account(program.value())?;
    let image: [u8; 32] = fixed(&json!(dep.bridge_image_id), "image id")?;
    let carries = program_account["data"]["actor_states"].as_object().is_some_and(|m| {
        m.values().any(|s| {
            let bytes: Vec<u8> = s.as_array().map(|a| a.iter().filter_map(|x| x.as_u64().map(|y| y as u8)).collect()).unwrap_or_default();
            bytes.len() >= 33 && bytes[1..33] == image
        })
    });
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
    checks.push(format!("LEZ (head {head}): the bridge program carries its image id 0x{}…; its configuration is paired with this vault", &dep.bridge_image_id[..12]));

    // 4. The keys, only now (nothing above read anything with the phrase).
    let accounts = lezkey::public_accounts(&seed, GAP)?;
    let private = lezkey::private_account_ids(&seed, GAP);
    let bridge_seed = Zeroizing::new(lezkey::hmac256(seed.as_ref(), crate::BRIDGE_SEED_LABEL));
    drop(seed);
    let keys = BridgeKeys::from_bridge_seed(bridge_seed.as_ref());
    let s = json!({
        "bridge_seed": hex(bridge_seed.as_ref()),
        "chain_id": dep.evm_chain_id,
        "vault": hex(&vault),
        "config_id": hex(&cid),
        "program": hex(program.value()),
        "depositor": depositor.map(|d| hex(&d)),
        "gap": GAP,
        "public_accounts": accounts.iter().map(|a| hex(&a.id)).collect::<Vec<_>>(),
        "private_accounts": private.iter().map(|p| hex(p)).collect::<Vec<_>>(),
        "proofs": "none",
        "eth_rpc": eth,
        "sequencer": lez.url,
        "network": base,
        "network_session": true,
        "from_block": dep.from_block,
    });
    let mut summary = json!({
        "network": dep.name,
        "address": base,
        "lez_rpc": lez.url,
        "eth_rpc": eth,
        "chain_id": dep.evm_chain_id,
        "vault": hex(&vault),
        "config_id": hex(&cid),
        "program": dep.bridge_program,
        "lez_head": head,
        "eth_block": eth_block,
        "checks": checks,
        "public_accounts": accounts.iter().map(|a| json!({ "index": a.index, "id": lezkey::id_string(&a.id) })).collect::<Vec<_>>(),
        "private_accounts": private.iter().enumerate().map(|(i, p)| json!({ "index": i, "id": hex(p) })).collect::<Vec<_>>(),
        "depositor": depositor.map(|d| hex(&d)),
        "phrase": "held by ferrata_bridge_module in this Basecamp's memory for this session only: the keys of public accounts /0-/7 and the bridge seed derived from it; the phrase itself is not kept; nothing is written to disk or logged; closing the session or Basecamp drops them",
    });
    if made {
        // Shown once: the module keeps no copy of the words.
        summary["words"] = json!(phrase.split(' ').collect::<Vec<_>>());
    }
    *NET.lock().map_err(|_| "the network session's lock is poisoned")? = Some(Net {
        lez,
        faucet,
        program,
        cid,
        vault,
        chain_id: dep.evm_chain_id,
        eth,
        from_block: dep.from_block,
        accounts,
        keys,
        _bridge_seed: bridge_seed,
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
/// before it, read from their actors (no history scan): `(burn counter, note)`.
pub(crate) fn own_burns() -> Result<(u64, Vec<(u64, bw::Note)>), String> {
    let lez = lez()?;
    let (program, cid, view) = with(|n| Ok((n.program, n.cid, n.keys.view.clone())))?;
    let mut out = Vec::new();
    for i in 0..256u64 {
        let r = outbound::burn_tag(&view, &cid, i);
        match lez.state(&bw::burn_actor(program, &cid, &r))? {
            Some(b) => match BridgeState::try_from_slice(&b) {
                Ok(BridgeState::Burn(rec)) => out.push((i, rec.note)),
                _ => return Err(format!("burn(cid, r_{i}) is not a burn record")),
            },
            None => return Ok((i, out)),
        }
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

/// Signs and sends the module's message as a LEZ public transaction paid (and co-signed) by public account `/payer`,
/// then waits for its block. `expect`: the actor the call writes (a claim's nullifier, a burn's record); written
/// means applied, absent once the transaction is in a block means the program refused it (its fee was charged).
pub(crate) fn submit(message: &[u8], accounts: &Value, root_index: usize, what: &str, payer: u32, expect: Option<WActor>,
                     gas_limit: u64) -> Result<Value, String> {
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
    let balance = lez.balance(&payer_id)?;
    if balance < max_fee {
        return Err(format!(
            "{what} needs up to {max_fee} units (10^-9 LGO) in LEZ fees; account /{payer} holds {balance}: use the faucet first"
        ));
    }
    let bytes = lezkey::encode_message(&build(max_fee));
    let h = lezkey::message_hash(&bytes);
    let witnesses = with(|n| {
        signers
            .iter()
            .map(|s| {
                let a = n.accounts.iter().find(|a| &a.id == s).ok_or("a signer outside the session")?;
                Ok((a.sign(&h)?, a.pk))
            })
            .collect::<Result<Vec<_>, String>>()
    })?;
    let tx = lezkey::encode_transaction(&bytes, &witnesses);
    let (wire, hash) = lezkey::wire(&tx);
    crate::jobs::phase("sent", json!({ "by": "ferrata_bridge_module (network session): signed here, sent to the network's RPC", "tx_hash": hash }));
    let answered = lez.call("sendTransaction", json!([wire]))?;
    if answered.as_str().is_some_and(|a| !a.trim_start_matches("0x").eq_ignore_ascii_case(&hash)) {
        return Err(format!("{what}: the RPC named the transaction {answered}, not {hash}"));
    }
    let t0 = Instant::now();
    let block = loop {
        if let Some(b) = lez.block_of(&hash)? {
            break b;
        }
        if t0.elapsed() > Duration::from_secs(180) {
            return Err(format!("{what}: transaction {hash} is not in a LEZ block after three minutes; check before trying again"));
        }
        std::thread::sleep(Duration::from_secs(2));
    };
    let applied = match &expect {
        Some(a) => Some(lez.state(a)?.is_some()),
        None => None,
    };
    Ok(json!({
        "settled": applied.unwrap_or(true),
        "reverted": applied.map(|x| !x),
        "private": false,
        "what": what,
        "tx_hash": hash,
        "block": block,
        "tx_bytes": tx.len(),
        "max_fee": max_fee.to_string(),
        "payer": format!("public /{payer} ({})", lezkey::id_string(&payer_id)),
        "proof": Value::Null,
    }))
}

/// One claim at the network's faucet for public account `/index` (its id and public key: a never-used account needs
/// the key as LEZ's admission evidence). The faucet's limits are its own (one claim per account per day).
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
    let mut after = lez.balance(&id).unwrap_or(0);
    let t0 = Instant::now();
    while after <= before && t0.elapsed() < Duration::from_secs(30) {
        std::thread::sleep(Duration::from_secs(2));
        after = lez.balance(&id).unwrap_or(0);
    }
    Ok(json!({
        "account": index,
        "id": lezkey::id_string(&id),
        "tx_hash": reply["hash"],
        "credited": reply["credited"],
        "balance_before": before.to_string(),
        "balance": after.to_string(),
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
mod tests {
    use super::*;

    #[test]
    fn endpoints_https_or_this_machine() {
        assert_eq!(endpoint("https://example.org/", "network").unwrap(), "https://example.org");
        assert!(endpoint("http://127.0.0.1:8080", "network").is_ok());
        assert!(endpoint("http://localhost:3040/rpc", "network").is_ok());
        assert!(endpoint("http://[::1]:3040", "network").is_ok());
        for bad in ["http://example.org", "http://10.0.0.5:8545", "http://192.168.1.2", "ftp://example.org", "example.org",
                    "https://user@example.org", "http://127.0.0.1.example.org"] {
            assert!(endpoint(bad, "network").is_err(), "{bad}");
        }
        assert!(same_origin("https://n.example", "//evil.example/rpc", "its RPC").is_err());
        assert!(same_origin("https://n.example", "https://evil.example/rpc", "its RPC").is_err());
        assert_eq!(same_origin("https://n.example", "/rpc", "its RPC").unwrap(), "https://n.example/rpc");
    }

    const PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    /// A stand-in network on 127.0.0.1: its `/wallet/config.json`, and an Ethereum JSON-RPC (any other path) answering
    /// `chain` for eth_chainId and `held` for the vault's lezConfigId(). One request per connection.
    fn stand_in(config: Value, chain: &'static str, held: String) -> String {
        use std::io::{Read as _, Write as _};
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
        let e = open(&json!({ "network": base, "eth_rpc": format!("{base}/eth"), "phrase": PHRASE })).err().expect("refused");
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
        let e = open(&json!({ "network": "http://example.org", "phrase": PHRASE })).err().unwrap();
        assert!(e.contains("plain http:// is refused"), "{e}");
        let base = stand_in(live_config(), "0xaa36a7", format!("0x{}", KNOWN[0].config_id));
        let e = open(&json!({ "network": base, "eth_rpc": "http://10.1.2.3:8545", "phrase": PHRASE })).err().unwrap();
        assert!(e.contains("eth_rpc") && e.contains("plain http:// is refused"), "{e}");
        // Neither reached anything: no session.
        assert!(with(|_| Ok(())).is_err());
    }

    #[test]
    fn a_bad_phrase_is_refused_without_naming_a_word() {
        let e = open(&json!({ "network": "https://n.example", "phrase": "abandon abandon zebra" })).err().unwrap();
        assert!(e.contains("not a valid BIP-39") && !e.contains("zebra") && !e.contains("abandon"), "{e}");
    }

    #[test]
    fn addresses_and_eip55() {
        assert!(address("0x19499061CeC0BaC2a3E1869Ba5AE6B403aA82787", "a").is_ok());
        assert!(address("0x19499061cec0bac2a3e1869ba5ae6b403aa82787", "a").is_ok());
        assert!(address("0x19499061CEC0BaC2a3E1869Ba5AE6B403aA82787", "a").is_err());
        assert!(address("0x0000000000000000000000000000000000000000", "a").is_err());
        assert!(address("19499061cec0bac2a3e1869ba5ae6b403aa82787", "a").is_err());
    }
}
