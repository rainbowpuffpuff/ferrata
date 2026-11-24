//! Where light-client data comes from: beacon API endpoints (`BeaconApi`, through `curl`), Logos Core's
//! `beacon_api_module` (`BeaconApiModule`, its `lc_request`, inside Logos Core only), a recorded directory in the
//! lab's layout (`Recorded`, with a clock the tests move), and the faults a source can have (`Faulty`: lies, lags,
//! goes down), for the tests.
//!
//! A source only serves bytes. Nothing it serves is trusted: the updater decodes every candidate
//! and runs the program's own checks on it against the instance's on-chain store before it sends
//! a transaction (`updater.rs`).

use std::{
    fs,
    io::Read as _,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, OnceLock,
    },
};

use anyhow::{anyhow, bail, Context as _, Result};
use ferrata_core::Root;
use ferrata_lez_wire::Config;

use crate::chain::{decode_finality_update, decode_update, split_chunks};

// ---- bounds on what a source may answer ---------------------------------------------------------

/// The most bytes read from a beacon node's `updates` answer: up to 128 periods (the beacon API's
/// `MAX_REQUEST_LIGHT_CLIENT_UPDATES`; the configuration asks 16 at a time by default) of about 27 KB each with a
/// 512-member committee, length-prefixed: about 3.5 MB.
pub const MAX_UPDATES_BYTES: usize = 8 << 20;
/// The most bytes read from a bootstrap or finality update answer (about 25 KB and 1 KB with a 512-member committee).
pub const MAX_OBJECT_BYTES: usize = 1 << 20;
/// The most bytes read from a JSON answer of `/eth/v1/node/version` or `/eth/v1/beacon/headers/{slot}` (under 2 KB).
pub const MAX_JSON_BYTES: usize = 64 << 10;
/// The longest reply text accepted from `beacon_api_module` (the provider's JSON for 128 updates is about 8 MB).
pub const MAX_MODULE_REPLY_BYTES: usize = 16 << 20;
/// The most bytes read from an execution endpoint's JSON-RPC answer (a block with its transaction hashes, about 69 B
/// each: about 200 KB for a 60M-gas block of simple transfers, more above).
pub const MAX_RPC_BYTES: usize = 2 << 20;
/// Characters of a provider's or curl's message kept in an error (errors reach the log and `/health`).
pub const MAX_MESSAGE_CHARS: usize = 200;

/// `text` kept to printable ASCII (others become `_`) and [`MAX_MESSAGE_CHARS`] characters: a provider chooses it.
pub fn clip(text: &str) -> String {
    let mut out: String = text
        .trim()
        .chars()
        .take(MAX_MESSAGE_CHARS)
        .map(|c| if c.is_ascii_graphic() || c == ' ' { c } else { '_' })
        .collect();
    if text.trim().chars().count() > MAX_MESSAGE_CHARS {
        out.push('…');
    }
    out
}

/// What [`fetch`] got: the answer (at most the bound asked for), whether curl succeeded, and curl's message.
pub struct Fetched {
    pub body: Vec<u8>,
    pub ok: bool,
    pub message: String,
}

/// `GET url` with `curl`, reading at most `max` bytes of the answer. Curl reads no `~/.curlrc` (`-q`), speaks only
/// HTTP and HTTPS (`--proto =http,https`), follows no redirect, gives up after `timeout_secs` (at least 1), refuses an
/// answer that announces more than `max` bytes (`--max-filesize`, exit 63) and is stopped as soon as it has written
/// more than `max` bytes (an answer that announces no length): the answer never takes more than `max` bytes of memory
/// here. `args` go before the URL (`--fail`, `-w ...`), which is passed with `--url` (a value that starts with `-` is
/// not read as an option); the User-Agent is generic, nothing identifies the caller.
pub fn fetch(url: &str, accept: &str, args: &[&str], timeout_secs: u64, max: usize) -> Result<Fetched> {
    let mut child = Command::new("curl")
        .arg("-q")
        .args(["-sS", "--proto", "=http,https", "--max-redirs", "0"])
        .args(["--max-time", &timeout_secs.max(1).to_string(), "--max-filesize", &max.to_string()])
        .args(["-H", &format!("Accept: {accept}"), "-A", "ferrata-updater"])
        .args(args)
        .arg("--url")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("curl did not start")?;
    // curl's messages on a thread of their own (kept to 4 KiB, the rest drained), so that neither pipe can fill up
    // while the other is read.
    let mut stderr = child.stderr.take().expect("piped");
    let messages = std::thread::spawn(move || {
        let mut kept = Vec::new();
        let _ = (&mut stderr).take(4096).read_to_end(&mut kept);
        let _ = std::io::copy(&mut stderr, &mut std::io::sink());
        kept
    });
    let mut body = Vec::new();
    let read = child.stdout.take().expect("piped").take(max as u64 + 1).read_to_end(&mut body);
    let over = body.len() > max;
    if over {
        let _ = child.kill();
        body.clear();
    }
    let status = child.wait().context("curl")?;
    let message = clip(&String::from_utf8_lossy(&messages.join().unwrap_or_default()));
    if over {
        bail!("an answer longer than {max} bytes (stopped there)");
    }
    read.context("reading curl's output")?;
    if status.code() == Some(63) {
        bail!("an answer longer than {max} bytes (curl stopped it)");
    }
    Ok(Fetched { body, ok: status.success(), message })
}

/// The three reads the updater makes (beacon-APIs `light_client` namespace, SSZ).
pub trait Source: Send {
    fn name(&self) -> &str;
    /// `GET /eth/v1/beacon/light_client/bootstrap/{block_root}`.
    fn bootstrap(&mut self, block_root: &Root) -> Result<Vec<u8>>;
    /// `GET /eth/v1/beacon/light_client/updates?start_period&count`: one SSZ update per period.
    fn updates(&mut self, start_period: u64, count: u64) -> Result<Vec<Vec<u8>>>;
    /// `GET /eth/v1/beacon/light_client/finality_update`.
    fn finality_update(&mut self) -> Result<Vec<u8>>;
    /// Whether it serves light-client data (`false`: a [`HeaderWitness`], which only confirms headers).
    fn light_client(&self) -> bool {
        true
    }
    /// `GET /eth/v1/beacon/headers/{slot}`: the root of the block it holds canonical at `slot` (`None`: none).
    fn canonical_root(&mut self, slot: u64) -> Result<Option<Root>> {
        bail!("{}: serves no headers (slot {slot})", self.name())
    }
    /// Whether it is an execution endpoint (`kind = "execution"`), which confirms headers by their execution block.
    fn execution(&self) -> bool {
        false
    }
    /// The number of the execution block `block_hash` if the endpoint holds it canonical (`None`: it holds no such
    /// block, or another block at its number): [`execution_canonical`].
    fn execution_block(&mut self, block_hash: &Root) -> Result<Option<u64>> {
        bail!("{}: serves no execution blocks (0x{})", self.name(), hex::encode(block_hash))
    }
    /// `GET /eth/v1/node/version`: the client the node says it runs (`data.version`, e.g. `Lodestar/v1.51.0/7923280`),
    /// for `policy.min_distinct_clients`. The provider's own claim: nothing authenticates it.
    fn client(&mut self) -> Result<String> {
        bail!("{}: reports no client", self.name())
    }
}

/// The client family of a `/eth/v1/node/version` answer: its first word, lower case (`Nimbus/v26.10.2-5628de-stateofus`:
/// `nimbus`; `Lighthouse/v8.3.3-57c2abc/x86_64-linux`: `lighthouse`).
pub fn client_family(version: &str) -> Option<String> {
    let first = version.split('/').next()?.trim().to_ascii_lowercase();
    (!first.is_empty()).then_some(first)
}

/// `data.version` of a `/eth/v1/node/version` answer, kept to printable ASCII and 100 characters (it reaches logs and
/// metric labels, and a provider chooses it).
pub fn parse_version_answer(body: &str) -> Result<String> {
    let v: serde_json::Value = serde_json::from_str(body).context("not JSON")?;
    let raw = v["data"]["version"].as_str().ok_or_else(|| anyhow!("no data.version"))?;
    let clean: String =
        raw.chars().take(100).map(|c| if c.is_ascii_graphic() && c != '"' && c != '\\' { c } else { '_' }).collect();
    if client_family(&clean).is_none() {
        bail!("an empty version");
    }
    Ok(clean)
}

/// `GET {base}/eth/v1/node/version` with `curl` ([`fetch`]: at most [`MAX_JSON_BYTES`]).
pub fn node_version(name: &str, base: &str, timeout_secs: u64) -> Result<String> {
    let url = format!("{}/eth/v1/node/version", base.trim_end_matches('/'));
    let out = fetch(&url, "application/json", &["--fail"], timeout_secs, MAX_JSON_BYTES)
        .with_context(|| format!("{name}: {url}"))?;
    if !out.ok {
        bail!("{name}: {url}: {}", out.message);
    }
    parse_version_answer(&String::from_utf8_lossy(&out.body)).with_context(|| format!("{name}: {url}"))
}

/// A beacon node's REST API, read with `curl` (no TLS library in this workspace): SSZ, a generic
/// User-Agent, nothing else that identifies the caller.
pub struct BeaconApi {
    pub name: String,
    pub base: String,
    pub timeout_secs: u64,
}

impl BeaconApi {
    /// At most `max` bytes ([`fetch`]).
    fn get(&self, path: &str, max: usize) -> Result<Vec<u8>> {
        let url = format!("{}{}", self.base.trim_end_matches('/'), path);
        let out = fetch(&url, "application/octet-stream", &["--fail"], self.timeout_secs, max)
            .with_context(|| format!("{}: {url}", self.name))?;
        if !out.ok {
            bail!("{}: {url}: {}", self.name, out.message);
        }
        Ok(out.body)
    }
}

impl Source for BeaconApi {
    fn name(&self) -> &str {
        &self.name
    }
    fn bootstrap(&mut self, block_root: &Root) -> Result<Vec<u8>> {
        self.get(&format!("/eth/v1/beacon/light_client/bootstrap/0x{}", hex::encode(block_root)), MAX_OBJECT_BYTES)
    }
    fn updates(&mut self, start_period: u64, count: u64) -> Result<Vec<Vec<u8>>> {
        let raw = self.get(
            &format!("/eth/v1/beacon/light_client/updates?start_period={start_period}&count={count}"),
            MAX_UPDATES_BYTES,
        )?;
        Ok(split_chunks(&raw))
    }
    fn finality_update(&mut self) -> Result<Vec<u8>> {
        self.get("/eth/v1/beacon/light_client/finality_update", MAX_OBJECT_BYTES)
    }
    fn client(&mut self) -> Result<String> {
        node_version(&self.name, &self.base, self.timeout_secs)
    }
}

/// How the module crate reaches `beacon_api_module.lc_request` inside Logos Core: (chain id, endpoint, the params as
/// JSON text, the provider to try first (may be empty), a deadline in milliseconds) -> that module's reply text
/// (`{ok: true, provider, body, attempts}`, `body` the provider's JSON verbatim, or `{ok: false, code, error}`), or
/// why the call did not reach it.
pub type BeaconApiModuleCall = fn(i64, &str, &str, &str, u64) -> std::result::Result<String, String>;

static BEACON_API_MODULE: OnceLock<BeaconApiModuleCall> = OnceLock::new();

/// Installed once by `ferrata_updater_module` (inside Logos Core); natively nothing is installed and a
/// [`BeaconApiModule`] source answers with an error.
pub fn set_beacon_api_module(f: BeaconApiModuleCall) {
    let _ = BEACON_API_MODULE.set(f);
}

/// Logos Core's `beacon_api_module` (`[[chain.source]] via = "beacon_api_module"`): the consensus-layer providers that
/// module holds for `chain_id` (its list, failover and proxy policy), asked through its `lc_request`. That module
/// answers with the provider's JSON, which [`crate::lc_json`] turns into the SSZ the other sources serve. It checks
/// nothing (as `beacon_api_module` itself does not): the updater runs the program's own checks on every candidate.
pub struct BeaconApiModule {
    pub name: String,
    pub chain_id: i64,
    /// The provider id `beacon_api_module` tries first (`""`: its own order); an answer from another provider (it failed
    /// over) is not taken.
    pub provider: String,
    pub timeout_secs: u64,
    /// The provider that answered each of the last [`ANSWERED_KEPT`] requests, in order (`beacon_api_module`'s ids).
    pub answered: Vec<String>,
}

/// How many providers [`BeaconApiModule::answered`] keeps.
pub const ANSWERED_KEPT: usize = 64;

impl BeaconApiModule {
    fn request(&mut self, endpoint: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let f = BEACON_API_MODULE
            .get()
            .ok_or_else(|| anyhow!("{}: beacon_api_module is reachable only from inside Logos Core", self.name))?;
        let text = f(self.chain_id, endpoint, &params.to_string(), &self.provider, self.timeout_secs * 1000)
            .map_err(|e| anyhow!("{}: {endpoint}: {}", self.name, clip(&e)))?;
        if text.len() > MAX_MODULE_REPLY_BYTES {
            bail!(
                "{}: {endpoint}: beacon_api_module's reply of {} bytes is longer than {MAX_MODULE_REPLY_BYTES}",
                self.name,
                text.len()
            );
        }
        let v: serde_json::Value = serde_json::from_str(&text)
            .with_context(|| format!("{}: {endpoint}: beacon_api_module's reply is not JSON", self.name))?;
        if v["ok"].as_bool() != Some(true) {
            bail!(
                "{}: {endpoint}: beacon_api_module answered {} ({})",
                self.name,
                clip(v["error"].as_str().unwrap_or("no detail")),
                clip(v["code"].as_str().unwrap_or("no code"))
            );
        }
        let by = clip(v["provider"].as_str().unwrap_or("?"));
        // A source that names its provider takes that provider's answers only: beacon_api_module fails over to its
        // other providers, and two sources that both fell over to one provider would count as two for min_agreeing.
        if !self.provider.is_empty() && by != self.provider {
            bail!(
                "{}: {endpoint}: beacon_api_module answered from provider {by}, not {} (it failed over): not taken",
                self.name,
                self.provider
            );
        }
        if self.answered.len() >= ANSWERED_KEPT {
            self.answered.remove(0);
        }
        self.answered.push(by);
        let body = v["body"].as_str().ok_or_else(|| anyhow!("{}: {endpoint}: no body", self.name))?;
        serde_json::from_str(body)
            .with_context(|| format!("{}: {endpoint}: the provider's body is not JSON", self.name))
    }
}

impl Source for BeaconApiModule {
    fn name(&self) -> &str {
        &self.name
    }
    fn bootstrap(&mut self, block_root: &Root) -> Result<Vec<u8>> {
        let v = self.request(
            "getLightClientBootstrap",
            serde_json::json!({ "block_root": format!("0x{}", hex::encode(block_root)) }),
        )?;
        crate::lc_json::bootstrap(&v).with_context(|| format!("{}: bootstrap", self.name))
    }
    fn updates(&mut self, start_period: u64, count: u64) -> Result<Vec<Vec<u8>>> {
        let v = self.request(
            "getLightClientUpdatesByRange",
            serde_json::json!({ "start_period": start_period, "count": count }),
        )?;
        crate::lc_json::updates(&v).with_context(|| format!("{}: updates", self.name))
    }
    fn finality_update(&mut self) -> Result<Vec<u8>> {
        let v = self.request("getLightClientFinalityUpdate", serde_json::json!({}))?;
        crate::lc_json::finality_update(&v).with_context(|| format!("{}: finality update", self.name))
    }
    fn client(&mut self) -> Result<String> {
        bail!("{}: beacon_api_module fails over between providers: no one node's client", self.name)
    }
}

/// A beacon node's REST API that serves no light-client data (most public ones; publicnode's answers 404 to
/// `light_client/finality_update`): it only reports which block it holds canonical at a slot, through
/// `/eth/v1/beacon/headers/{slot}` (JSON), and so counts towards `policy.min_agreeing` for an update whose finalized
/// header is that block (`updater.rs`). Read with `curl`, a generic User-Agent, nothing that identifies the caller.
pub struct HeaderWitness {
    pub name: String,
    pub base: String,
    pub timeout_secs: u64,
}

impl Source for HeaderWitness {
    fn name(&self) -> &str {
        &self.name
    }
    fn bootstrap(&mut self, _: &Root) -> Result<Vec<u8>> {
        bail!("{}: a headers source serves no light-client data", self.name)
    }
    fn updates(&mut self, _: u64, _: u64) -> Result<Vec<Vec<u8>>> {
        bail!("{}: a headers source serves no light-client data", self.name)
    }
    fn finality_update(&mut self) -> Result<Vec<u8>> {
        bail!("{}: a headers source serves no light-client data", self.name)
    }
    fn light_client(&self) -> bool {
        false
    }
    fn client(&mut self) -> Result<String> {
        node_version(&self.name, &self.base, self.timeout_secs)
    }
    fn canonical_root(&mut self, slot: u64) -> Result<Option<Root>> {
        let url = format!("{}/eth/v1/beacon/headers/{slot}", self.base.trim_end_matches('/'));
        // The status line curl writes after the body (`-w`) is counted in the bound too.
        let out = fetch(&url, "application/json", &["-w", "\n%{http_code}"], self.timeout_secs, MAX_JSON_BYTES + 16)
            .with_context(|| format!("{}: {url}", self.name))?;
        if !out.ok {
            bail!("{}: {url}: {}", self.name, out.message);
        }
        let text = String::from_utf8_lossy(&out.body);
        let (body, code) = text.rsplit_once('\n').ok_or_else(|| anyhow!("{}: {url}: no status", self.name))?;
        match code.trim() {
            "200" => parse_header_answer(body, slot).with_context(|| format!("{}: {url}", self.name)),
            "404" => Ok(None),
            other => bail!("{}: {url}: HTTP {}: {}", self.name, clip(other), clip(body)),
        }
    }
}

// ---- execution endpoints (`kind = "execution"`) ------------------------------------------------------------------

/// Whether an execution endpoint holds the block `block_hash` canonical: `eth_getBlockByHash(hash, false)` gives the
/// block (its `hash` must be the one asked) and its `number`, and `eth_getBlockByNumber(number, false)` must give
/// the same hash. `Some(number)` if so; `None` if it holds no block of that hash, or another block at that number (a
/// fork, or another chain). `call` is one JSON-RPC call: (method, params) -> the `result`.
pub fn execution_canonical(
    mut call: impl FnMut(&str, serde_json::Value) -> Result<serde_json::Value>,
    block_hash: &Root,
) -> Result<Option<u64>> {
    let h = format!("0x{}", hex::encode(block_hash));
    let block = call("eth_getBlockByHash", serde_json::json!([h, false]))?;
    if block.is_null() {
        return Ok(None);
    }
    let got = block["hash"].as_str().ok_or_else(|| anyhow!("eth_getBlockByHash: no hash in the block"))?;
    if !got.eq_ignore_ascii_case(&h) {
        bail!("eth_getBlockByHash {h} answered the block {}", clip(got));
    }
    let n = block["number"].as_str().ok_or_else(|| anyhow!("eth_getBlockByHash: no number in the block"))?;
    let number = u64::from_str_radix(n.trim_start_matches("0x"), 16)
        .with_context(|| format!("eth_getBlockByHash: number {}", clip(n)))?;
    let at = call("eth_getBlockByNumber", serde_json::json!([format!("0x{number:x}"), false]))?;
    Ok(at["hash"].as_str().is_some_and(|x| x.eq_ignore_ascii_case(&h)).then_some(number))
}

/// An execution endpoint's JSON-RPC, read with `curl` (POST, at most [`MAX_RPC_BYTES`] of answer, a generic
/// User-Agent, nothing that identifies the caller): a [`Source`] of `kind = "execution"`.
pub struct ExecutionRpc {
    pub name: String,
    pub url: String,
    pub timeout_secs: u64,
    pub requests: u64,
}

impl ExecutionRpc {
    pub fn new(name: &str, url: &str, timeout_secs: u64) -> Self {
        Self { name: name.into(), url: url.into(), timeout_secs, requests: 0 }
    }
    fn call(&mut self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        self.requests += 1;
        let body = serde_json::json!({ "jsonrpc": "2.0", "id": self.requests, "method": method, "params": params }).to_string();
        let out = fetch(
            &self.url,
            "application/json",
            &["-H", "Content-Type: application/json", "--data-binary", &body, "-w", "\n%{http_code}"],
            self.timeout_secs,
            MAX_RPC_BYTES + 16,
        )
        .with_context(|| format!("{}: {method}", self.name))?;
        if !out.ok {
            bail!("{}: {method}: {}", self.name, out.message);
        }
        let text = String::from_utf8_lossy(&out.body);
        let (answer, code) = text.rsplit_once('\n').ok_or_else(|| anyhow!("{}: {method}: no status", self.name))?;
        if code.trim() != "200" {
            bail!("{}: {method}: HTTP {}: {}", self.name, clip(code), clip(answer));
        }
        rpc_result(answer).with_context(|| format!("{}: {method}", self.name))
    }
}

/// A JSON-RPC answer's `result`, or its `error` as an error.
pub fn rpc_result(answer: &str) -> Result<serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(answer).context("not JSON")?;
    if let Some(e) = v.get("error").filter(|e| !e.is_null()) {
        bail!("error {}", clip(&e.to_string()));
    }
    v.get("result").cloned().ok_or_else(|| anyhow!("no result"))
}

impl Source for ExecutionRpc {
    fn name(&self) -> &str {
        &self.name
    }
    fn bootstrap(&mut self, _: &Root) -> Result<Vec<u8>> {
        bail!("{}: an execution source serves no light-client data", self.name)
    }
    fn updates(&mut self, _: u64, _: u64) -> Result<Vec<Vec<u8>>> {
        bail!("{}: an execution source serves no light-client data", self.name)
    }
    fn finality_update(&mut self) -> Result<Vec<u8>> {
        bail!("{}: an execution source serves no light-client data", self.name)
    }
    fn light_client(&self) -> bool {
        false
    }
    fn execution(&self) -> bool {
        true
    }
    fn execution_block(&mut self, block_hash: &Root) -> Result<Option<u64>> {
        execution_canonical(|m, p| self.call(m, p), block_hash)
    }
}

/// How the module crate reaches `eth_rpc_module.raw_rpc` inside Logos Core: (chain id, method, the params as a JSON
/// array's text, a deadline in milliseconds) -> that module's reply text (`{ok: true, route, result}` or
/// `{ok: false, code, error}`), or why the call did not reach it.
pub type EthRpcModuleCall = fn(i64, &str, &str, u64) -> std::result::Result<String, String>;

static ETH_RPC_MODULE: OnceLock<EthRpcModuleCall> = OnceLock::new();

/// Installed once by `ferrata_updater_module` (inside Logos Core); natively nothing is installed and an
/// [`EthRpcModule`] source answers with an error.
pub fn set_eth_rpc_module(f: EthRpcModuleCall) {
    let _ = ETH_RPC_MODULE.set(f);
}

/// Logos Core's `eth_rpc_module` (`[[chain.source]] kind = "execution"`, `via = "eth_rpc_module"`): the execution
/// endpoint that module holds for `chain_id` (its own configuration, proxy and verified-proxy policy).
pub struct EthRpcModule {
    pub name: String,
    pub chain_id: i64,
    pub timeout_secs: u64,
    /// `eth_rpc_module`'s label of the route of each of the last [`ANSWERED_KEPT`] answers (`direct`, `verified`,
    /// `proxied`).
    pub routes: Vec<String>,
}

impl EthRpcModule {
    fn call(&mut self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let f = ETH_RPC_MODULE
            .get()
            .ok_or_else(|| anyhow!("{}: eth_rpc_module is reachable only from inside Logos Core", self.name))?;
        let text = f(self.chain_id, method, &params.to_string(), self.timeout_secs * 1000)
            .map_err(|e| anyhow!("{}: {method}: {}", self.name, clip(&e)))?;
        if text.len() > MAX_RPC_BYTES {
            bail!("{}: {method}: eth_rpc_module's reply of {} bytes is longer than {MAX_RPC_BYTES}", self.name, text.len());
        }
        let v: serde_json::Value = serde_json::from_str(&text)
            .with_context(|| format!("{}: {method}: eth_rpc_module's reply is not JSON", self.name))?;
        if v["ok"].as_bool() != Some(true) {
            bail!(
                "{}: {method}: eth_rpc_module answered {} ({})",
                self.name,
                clip(v["error"].as_str().unwrap_or("no detail")),
                clip(v["code"].as_str().unwrap_or("no code"))
            );
        }
        if self.routes.len() >= ANSWERED_KEPT {
            self.routes.remove(0);
        }
        self.routes.push(clip(v["route"].as_str().unwrap_or("?")));
        Ok(v["result"].clone())
    }
}

impl Source for EthRpcModule {
    fn name(&self) -> &str {
        &self.name
    }
    fn bootstrap(&mut self, _: &Root) -> Result<Vec<u8>> {
        bail!("{}: an execution source serves no light-client data", self.name)
    }
    fn updates(&mut self, _: u64, _: u64) -> Result<Vec<Vec<u8>>> {
        bail!("{}: an execution source serves no light-client data", self.name)
    }
    fn finality_update(&mut self) -> Result<Vec<u8>> {
        bail!("{}: an execution source serves no light-client data", self.name)
    }
    fn light_client(&self) -> bool {
        false
    }
    fn execution(&self) -> bool {
        true
    }
    fn execution_block(&mut self, block_hash: &Root) -> Result<Option<u64>> {
        execution_canonical(|m, p| self.call(m, p), block_hash)
    }
}

/// The root of a `/eth/v1/beacon/headers/{slot}` answer if it is the canonical block of `slot`.
pub fn parse_header_answer(body: &str, slot: u64) -> Result<Option<Root>> {
    let v: serde_json::Value = serde_json::from_str(body).context("not JSON")?;
    let data = &v["data"];
    let root = data["root"].as_str().ok_or_else(|| anyhow!("no data.root"))?;
    let at = data["header"]["message"]["slot"].as_str().and_then(|s| s.parse::<u64>().ok());
    if data["canonical"].as_bool() != Some(true) || at != Some(slot) {
        return Ok(None);
    }
    Ok(Some(crate::chain::root(root)?))
}

/// One recorded response: its place in the record and what it is.
#[derive(Clone, Debug)]
pub struct Entry {
    pub seq: usize,
    pub period_update: bool,
    pub attested_period: u64,
    pub finalized_slot: u64,
    pub bytes: Vec<u8>,
}

/// A clock shared by recorded sources: how many entries of the record have "happened".
#[derive(Clone, Debug, Default)]
pub struct Clock(pub Arc<AtomicUsize>);

impl Clock {
    pub fn new(at: usize) -> Self {
        Self(Arc::new(AtomicUsize::new(at)))
    }
    pub fn now(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
    pub fn set(&self, at: usize) {
        self.0.store(at, Ordering::SeqCst)
    }
}

/// A recorded chain in the lab's layout (`lez/lab/results/record/net/<chain>/`): `bootstrap.ssz`,
/// `checkpoint`, `updates/NNNNN.<period|finality>.<layout>.ssz` in the order they happened. At
/// clock `t` the source serves what a beacon node served after the first `t` entries: the newest
/// finality update among them, and per period the newest period update.
pub struct Recorded {
    pub name: String,
    pub bootstrap: Vec<u8>,
    pub checkpoint: Root,
    pub entries: Vec<Entry>,
    pub clock: Clock,
    /// Entries this source is behind the clock (0: up to date).
    pub lag: usize,
    /// Entries this source is ahead of the clock: it serves valid updates no other source serves yet (the tests'
    /// stand-in for a fork that only one implementation, or a captured committee's accomplice, serves).
    pub ahead: usize,
    /// The `/eth/v1/node/version` answer it stands for (`None`: it reports none).
    pub client: Option<String>,
}

impl Recorded {
    pub fn open(name: &str, dir: &Path, config: &Config, clock: Clock) -> Result<Self> {
        let bootstrap =
            fs::read(dir.join("bootstrap.ssz")).with_context(|| format!("{}: bootstrap.ssz", dir.display()))?;
        let checkpoint = crate::chain::root(&fs::read_to_string(dir.join("checkpoint"))?)?;
        let entries = load_entries(&dir.join("updates"), config)?;
        Ok(Self { name: name.to_string(), bootstrap, checkpoint, entries, clock, lag: 0, ahead: 0, client: None })
    }

    fn visible(&self) -> usize {
        self.clock.now().saturating_add(self.ahead).saturating_sub(self.lag).min(self.entries.len())
    }
}

/// Every `NNNNN.<kind>.<layout>.ssz` of `dir`, in order, decoded once to know its periods.
pub fn load_entries(dir: &Path, config: &Config) -> Result<Vec<Entry>> {
    let spec = config.spec();
    let mut names: Vec<String> = fs::read_dir(dir)
        .with_context(|| format!("{}", dir.display()))?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.ends_with(".ssz"))
        .collect();
    names.sort();
    let mut entries = Vec::new();
    for (seq, name) in names.iter().enumerate() {
        let bytes = fs::read(dir.join(name))?;
        let period_update = name.contains(".period.");
        let (update, _) =
            if period_update { decode_update(&bytes, config)? } else { decode_finality_update(&bytes, config)? };
        entries.push(Entry {
            seq,
            period_update,
            attested_period: spec.period_at_slot(update.attested_header.beacon.slot),
            finalized_slot: update.finalized_header.beacon.slot,
            bytes,
        });
    }
    Ok(entries)
}

impl Source for Recorded {
    fn name(&self) -> &str {
        &self.name
    }
    fn bootstrap(&mut self, block_root: &Root) -> Result<Vec<u8>> {
        if *block_root != self.checkpoint {
            bail!("{}: no bootstrap for 0x{} (404)", self.name, hex::encode(block_root));
        }
        Ok(self.bootstrap.clone())
    }
    fn updates(&mut self, start_period: u64, count: u64) -> Result<Vec<Vec<u8>>> {
        let seen = &self.entries[..self.visible()];
        let mut out = Vec::new();
        for period in start_period..start_period.saturating_add(count) {
            if let Some(e) = seen.iter().rev().find(|e| e.period_update && e.attested_period == period) {
                out.push(e.bytes.clone());
            }
        }
        Ok(out)
    }
    fn finality_update(&mut self) -> Result<Vec<u8>> {
        let seen = &self.entries[..self.visible()];
        // A beacon node's newest finality update; the record keeps period updates as served by
        // the `updates` endpoint, so the newest finality-kind entry stands for it.
        seen.iter()
            .rev()
            .find(|e| !e.period_update)
            .map(|e| e.bytes.clone())
            .ok_or_else(|| anyhow!("{}: no finality update yet (404)", self.name))
    }
    fn client(&mut self) -> Result<String> {
        self.client.clone().ok_or_else(|| anyhow!("{}: reports no client", self.name))
    }
}

/// What can go wrong with a source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// Serves what it is given with one byte of every update's sync-committee signature flipped:
    /// well-formed, signed by nobody.
    BadSignature,
    /// Serves a bootstrap for any root asked (another checkpoint's).
    WrongBootstrap,
    /// Fails every request (a node that is down).
    Down,
}

/// A source with a fault.
pub struct Faulty<S: Source> {
    pub inner: S,
    pub fault: Fault,
    pub config: Config,
    pub label: String,
}

impl<S: Source> Faulty<S> {
    pub fn new(inner: S, fault: Fault, config: Config) -> Self {
        let label = format!("{}({fault:?})", inner.name());
        Self { inner, fault, config, label }
    }

    /// `bytes` with one byte of the update's sync-committee signature flipped.
    pub fn flip_signature(&self, bytes: Vec<u8>, period: bool) -> Vec<u8> {
        let decoded =
            if period { decode_update(&bytes, &self.config) } else { decode_finality_update(&bytes, &self.config) };
        let Ok((update, _)) = decoded else { return bytes };
        let sig = update.sync_aggregate.signature;
        let mut out = bytes;
        if let Some(at) = out.windows(sig.len()).position(|w| w == sig) {
            out[at + 40] ^= 0x01;
        }
        out
    }
}

impl<S: Source> Source for Faulty<S> {
    fn name(&self) -> &str {
        &self.label
    }
    fn bootstrap(&mut self, block_root: &Root) -> Result<Vec<u8>> {
        match self.fault {
            Fault::Down => bail!("{}: connection refused", self.label),
            Fault::WrongBootstrap => {
                let mut b = self.inner.bootstrap(block_root)?;
                // Another header: the bootstrap no longer hashes to the root asked for.
                if b.len() > 8 {
                    b[0] ^= 0x01;
                }
                Ok(b)
            }
            Fault::BadSignature => self.inner.bootstrap(block_root),
        }
    }
    fn updates(&mut self, start_period: u64, count: u64) -> Result<Vec<Vec<u8>>> {
        match self.fault {
            Fault::Down => bail!("{}: connection refused", self.label),
            Fault::BadSignature => {
                Ok(self.inner.updates(start_period, count)?.into_iter().map(|b| self.flip_signature(b, true)).collect())
            }
            Fault::WrongBootstrap => self.inner.updates(start_period, count),
        }
    }
    fn finality_update(&mut self) -> Result<Vec<u8>> {
        match self.fault {
            Fault::Down => bail!("{}: connection refused", self.label),
            Fault::BadSignature => {
                let b = self.inner.finality_update()?;
                Ok(self.flip_signature(b, false))
            }
            Fault::WrongBootstrap => self.inner.finality_update(),
        }
    }
    fn client(&mut self) -> Result<String> {
        match self.fault {
            Fault::Down => bail!("{}: connection refused", self.label),
            // A liar reports whatever client it likes.
            _ => self.inner.client(),
        }
    }
}

/// The lab's record of `chain` in this checkout (`lez/lab/results/record/net/<chain>`).
pub fn lab_record(chain: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../lab/results/record/net").join(chain)
}
