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
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, OnceLock,
    },
};

use anyhow::{anyhow, bail, Context as _, Result};
use ferrata_core::Root;
use ferrata_lez_wire::Config;

use crate::chain::{decode_finality_update, decode_update, split_chunks};

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

/// `GET {base}/eth/v1/node/version` with `curl` (a generic User-Agent, nothing that identifies the caller).
pub fn node_version(name: &str, base: &str, timeout_secs: u64) -> Result<String> {
    let url = format!("{}/eth/v1/node/version", base.trim_end_matches('/'));
    let out = Command::new("curl")
        .args(["-sS", "--fail", "--max-time", &timeout_secs.to_string()])
        .args(["-H", "Accept: application/json", "-A", "ferrata-updater"])
        .arg(&url)
        .output()
        .with_context(|| format!("{name}: curl did not start"))?;
    if !out.status.success() {
        bail!("{name}: {url}: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    parse_version_answer(&String::from_utf8_lossy(&out.stdout)).with_context(|| format!("{name}: {url}"))
}

/// A beacon node's REST API, read with `curl` (no TLS library in this workspace): SSZ, a generic
/// User-Agent, nothing else that identifies the caller.
pub struct BeaconApi {
    pub name: String,
    pub base: String,
    pub timeout_secs: u64,
}

impl BeaconApi {
    fn get(&self, path: &str) -> Result<Vec<u8>> {
        let url = format!("{}{}", self.base.trim_end_matches('/'), path);
        let out = Command::new("curl")
            .args(["-sS", "--fail", "--max-time", &self.timeout_secs.to_string()])
            .args(["-H", "Accept: application/octet-stream", "-A", "ferrata-updater"])
            .arg(&url)
            .output()
            .with_context(|| format!("{}: curl did not start", self.name))?;
        if !out.status.success() {
            bail!("{}: {url}: {}", self.name, String::from_utf8_lossy(&out.stderr).trim());
        }
        Ok(out.stdout)
    }
}

impl Source for BeaconApi {
    fn name(&self) -> &str {
        &self.name
    }
    fn bootstrap(&mut self, block_root: &Root) -> Result<Vec<u8>> {
        self.get(&format!("/eth/v1/beacon/light_client/bootstrap/0x{}", hex::encode(block_root)))
    }
    fn updates(&mut self, start_period: u64, count: u64) -> Result<Vec<Vec<u8>>> {
        let raw =
            self.get(&format!("/eth/v1/beacon/light_client/updates?start_period={start_period}&count={count}"))?;
        Ok(split_chunks(&raw))
    }
    fn finality_update(&mut self) -> Result<Vec<u8>> {
        self.get("/eth/v1/beacon/light_client/finality_update")
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
    /// The provider id `beacon_api_module` tries first (`""`: its own order).
    pub provider: String,
    pub timeout_secs: u64,
    /// The provider that answered each request, in order (`beacon_api_module`'s ids).
    pub answered: Vec<String>,
}

impl BeaconApiModule {
    fn request(&mut self, endpoint: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let f = BEACON_API_MODULE
            .get()
            .ok_or_else(|| anyhow!("{}: beacon_api_module is reachable only from inside Logos Core", self.name))?;
        let text = f(self.chain_id, endpoint, &params.to_string(), &self.provider, self.timeout_secs * 1000)
            .map_err(|e| anyhow!("{}: {endpoint}: {e}", self.name))?;
        let v: serde_json::Value = serde_json::from_str(&text)
            .with_context(|| format!("{}: {endpoint}: beacon_api_module's reply is not JSON", self.name))?;
        if v["ok"].as_bool() != Some(true) {
            bail!(
                "{}: {endpoint}: beacon_api_module answered {} ({})",
                self.name,
                v["error"].as_str().unwrap_or("no detail"),
                v["code"].as_str().unwrap_or("no code")
            );
        }
        self.answered.push(v["provider"].as_str().unwrap_or("?").to_string());
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
        let out = Command::new("curl")
            .args(["-sS", "--max-time", &self.timeout_secs.to_string(), "-w", "\n%{http_code}"])
            .args(["-H", "Accept: application/json", "-A", "ferrata-updater"])
            .arg(&url)
            .output()
            .with_context(|| format!("{}: curl did not start", self.name))?;
        if !out.status.success() {
            bail!("{}: {url}: {}", self.name, String::from_utf8_lossy(&out.stderr).trim());
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let (body, code) = text.rsplit_once('\n').ok_or_else(|| anyhow!("{}: {url}: no status", self.name))?;
        match code.trim() {
            "200" => parse_header_answer(body, slot).with_context(|| format!("{}: {url}", self.name)),
            "404" => Ok(None),
            other => bail!("{}: {url}: HTTP {other}: {}", self.name, body.chars().take(200).collect::<String>()),
        }
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
