//! The one configuration file (TOML): the daemon, its key, its fee limits, its policy, and per
//! chain the instance and its sources. `ferrata-updater.example.toml` documents every field.

use std::{fs, path::Path};

use anyhow::{bail, Context as _, Result};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct File {
    pub daemon: Daemon,
    pub keys: Keys,
    #[serde(default)]
    pub fees: Fees,
    #[serde(default)]
    pub policy: Policy,
    /// The sequencer `daemon.ledger = "sequencer"` writes to.
    #[serde(default)]
    pub sequencer: Option<SequencerCfg>,
    #[serde(rename = "chain")]
    pub chains: Vec<ChainCfg>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Daemon {
    /// A name for logs and metrics.
    #[serde(default = "default_name")]
    pub name: String,
    /// Where the committee cache and the journal live (one subdirectory per chain).
    pub state_dir: String,
    /// Seconds between two rounds.
    #[serde(default = "default_poll")]
    pub poll_secs: u64,
    /// `host:port` of `/health` and `/metrics` (none: not served).
    #[serde(default)]
    pub listen: Option<String>,
    /// `harness`: LEZ's state machine in process (dev mode, recorded sources); `sequencer`: a
    /// LEZ sequencer through its RPC (`[sequencer]`; the build of `sequencer/`).
    #[serde(default = "default_ledger")]
    pub ledger: String,
}

/// A running LEZ sequencer (`daemon.ledger = "sequencer"`).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SequencerCfg {
    /// The sequencer's JSON-RPC address (`sequencer_service start --rpc-listen-address`); loopback only.
    pub rpc: String,
    /// The attestation program's account on that chain (base58), as its deployment computed it.
    pub program: String,
    /// Read the chain, and count a transaction as settled, only in blocks final on Bedrock (default). `false`: the
    /// sequencer's head (LEZ's standalone sequencer has no Bedrock, so no block of it is ever final).
    #[serde(default = "default_true")]
    pub wait_final: bool,
    /// The backend's own files: the follower of the chain, the blocks it applied, the manifest of every transaction
    /// (default `<state_dir>/ledger`).
    #[serde(default)]
    pub run_dir: Option<String>,
    /// The sequencer's log, read for the reason of a transaction it dropped (optional).
    #[serde(default)]
    pub log: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Keys {
    /// A file holding the payer's 32-byte secret key in hex. The payer signs and pays every
    /// transaction; it needs no other role (no step checks a sender).
    pub payer_key_file: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fees {
    /// The gas limit every step declares (the largest step measured is 3,731,712).
    pub gas_limit: u64,
    /// The maximum fee every transaction declares (TOML integers are 64-bit).
    pub max_fee_per_tx: u64,
    /// What the daemon may spend from its start; at the limit it stops sending and says so.
    pub budget: u64,
}

impl Default for Fees {
    fn default() -> Self {
        Self { gas_limit: 4_800_000, max_fee_per_tx: 1_000_000_000, budget: 1_000_000_000_000 }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    /// Rounds over which updaters spread who sends a step: each waits `H(key, job, step) mod
    /// defer_window` rounds after it first sees the step due (1: never waits).
    pub defer_window: u64,
    /// A source whose newest verified finalized slot is this far behind the best is lagging.
    pub max_lag_slots: u64,
    /// How many sources must serve the chosen update's finalized header before it is sent.
    pub min_agreeing: usize,
    /// Backoff after a transport failure: `retry_base_ms * 2^attempt`, at most `retry_max_ms`.
    pub retry_base_ms: u64,
    pub retry_max_ms: u64,
    pub max_attempts: u32,
    /// Rounds a source that served something the program would refuse is not asked again.
    pub demote_rounds: u64,
    /// Periods asked of the `updates` endpoint at once.
    pub max_periods_per_request: u64,
    /// Finish a job this updater opened before choosing another update (default): a beacon node serves only its
    /// newest finality update, so the update a job was opened for can be gone from every source before the job's
    /// last step, and the steps already paid would be lost. The update is kept in `state_dir/<chain>/open/` until the
    /// job is finished or moot, so a restarted updater finishes it too.
    #[serde(default = "default_true")]
    pub finish_own_jobs: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            defer_window: 8,
            max_lag_slots: 64,
            min_agreeing: 1,
            retry_base_ms: 500,
            retry_max_ms: 30_000,
            max_attempts: 5,
            demote_rounds: 50,
            max_periods_per_request: 16,
            finish_own_jobs: true,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChainCfg {
    pub name: String,
    /// `mainnet` or `sepolia` (`chain::preset`).
    pub preset: String,
    /// The instance's trusted checkpoint (a finalized beacon block root, hex).
    pub checkpoint: String,
    #[serde(default = "default_parts")]
    pub adoption_parts: u8,
    /// The light-client layout of the checkpoint's bootstrap, which the instance's store starts in
    /// (`Config::store_fork`): `electra` (default) or `gloas` (a bootstrap of the Gloas era, as
    /// `crates/ferrata-spectests/fixtures/sepolia-2026-11-12`); the instance's id hashes it.
    #[serde(default)]
    pub layout: Option<String>,
    #[serde(rename = "source")]
    pub sources: Vec<SourceCfg>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceCfg {
    pub name: String,
    /// A beacon node's REST API base URL.
    #[serde(default)]
    pub url: Option<String>,
    /// A recorded directory in the lab's layout (dev mode).
    #[serde(default)]
    pub dir: Option<String>,
    /// `"beacon_api_module"`: Logos Core's module of that name, for `chain_id` (inside Logos Core only).
    #[serde(default)]
    pub via: Option<String>,
    /// The chain id `beacon_api_module` is asked about (with `via`).
    #[serde(default)]
    pub chain_id: Option<i64>,
    /// The provider id `beacon_api_module` tries first (with `via`; default: its own order).
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// `light-client` (default): it serves bootstraps, updates and finality updates. `headers`: it only serves
    /// `/eth/v1/beacon/headers/{slot}` (most public beacon APIs), and counts towards `min_agreeing` for an update
    /// whose finalized header is the block it reports canonical at that slot.
    #[serde(default)]
    pub kind: SourceKind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceKind {
    #[default]
    LightClient,
    Headers,
}

fn default_name() -> String {
    "ferrata-updater".into()
}
fn default_poll() -> u64 {
    12
}
fn default_ledger() -> String {
    "harness".into()
}
fn default_parts() -> u8 {
    2
}
fn default_timeout() -> u64 {
    30
}
fn default_true() -> bool {
    true
}

impl File {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| format!("{}", path.display()))?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Self> {
        let f: File = toml::from_str(text).context("configuration")?;
        f.check()?;
        Ok(f)
    }

    fn check(&self) -> Result<()> {
        if self.chains.is_empty() {
            bail!("no [[chain]]");
        }
        if self.fees.gas_limit == 0 || self.fees.max_fee_per_tx == 0 {
            bail!("fees: gas_limit and max_fee_per_tx must be positive");
        }
        if self.policy.defer_window == 0 || self.policy.max_attempts == 0 {
            bail!("policy: defer_window and max_attempts must be at least 1");
        }
        match (self.daemon.ledger.as_str(), &self.sequencer) {
            ("sequencer", None) => bail!("daemon.ledger = \"sequencer\" needs a [sequencer] section"),
            ("sequencer", Some(s)) => {
                crate::chain::check_loopback(&s.rpc)?;
                if s.program.trim().is_empty() {
                    bail!("sequencer.program: the attestation program's account");
                }
            }
            ("harness", _) => {}
            (other, _) => bail!("daemon.ledger = {other:?}: harness or sequencer"),
        }
        for c in &self.chains {
            crate::chain::instance_config(&c.preset, c.adoption_parts, c.layout.as_deref())
                .with_context(|| format!("chain {}", c.name))?;
            crate::chain::root(&c.checkpoint).with_context(|| format!("chain {}: checkpoint", c.name))?;
            if c.sources.is_empty() {
                bail!("chain {}: no [[chain.source]]", c.name);
            }
            for s in &c.sources {
                if [s.url.is_some(), s.dir.is_some(), s.via.is_some()].iter().filter(|x| **x).count() != 1 {
                    bail!("chain {} source {}: exactly one of url, dir and via", c.name, s.name);
                }
                if let Some(via) = &s.via {
                    if via != "beacon_api_module" {
                        bail!("chain {} source {}: via = {via:?}: \"beacon_api_module\"", c.name, s.name);
                    }
                    if !s.chain_id.is_some_and(|id| id > 0) {
                        bail!("chain {} source {}: via needs chain_id (a positive integer)", c.name, s.name);
                    }
                    if s.kind == SourceKind::Headers {
                        bail!("chain {} source {}: beacon_api_module is a light-client source", c.name, s.name);
                    }
                }
                if s.kind == SourceKind::Headers && s.url.is_none() {
                    bail!("chain {} source {}: a headers source needs a url", c.name, s.name);
                }
            }
            if c.sources.iter().all(|s| s.kind == SourceKind::Headers) {
                bail!("chain {}: no light-client source (every source is kind = \"headers\")", c.name);
            }
        }
        Ok(())
    }
}
