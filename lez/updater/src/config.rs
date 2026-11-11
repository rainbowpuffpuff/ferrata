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
    /// LEZ sequencer through its RPC (not in this build).
    #[serde(default = "default_ledger")]
    pub ledger: String,
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
    /// The maximum fee every transaction declares.
    pub max_fee_per_tx: u128,
    /// What the daemon may spend from its start; at the limit it stops sending and says so.
    pub budget: u128,
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
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
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
        for c in &self.chains {
            crate::chain::preset(&c.preset, c.adoption_parts)?;
            crate::chain::root(&c.checkpoint).with_context(|| format!("chain {}: checkpoint", c.name))?;
            if c.sources.is_empty() {
                bail!("chain {}: no [[chain.source]]", c.name);
            }
            for s in &c.sources {
                if s.url.is_some() == s.dir.is_some() {
                    bail!("chain {} source {}: exactly one of url and dir", c.name, s.name);
                }
            }
        }
        Ok(())
    }
}
