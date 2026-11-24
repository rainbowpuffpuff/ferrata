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
    /// The sequencer's JSON-RPC address (`sequencer_service start --rpc-listen-address`); loopback only, or the one
    /// remote sequencer `FERRATA_SEQUENCER_REMOTE` names exactly (`chain::check_loopback`), which the backend sends to
    /// only with `FERRATA_TESTNET_SEND=yes`.
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
    /// defer_window` rounds after it first sees the step due (1: never waits). A step of a job it carries counts from the
    /// first round that step was due; an `Open`, or a step of a job it does not carry, from the first round a step of
    /// that kind was due on the instance's state, whatever job it is for; after it sends a step there, the next of that
    /// kind waits `defer_window - 1` rounds.
    pub defer_window: u64,
    /// A source whose newest verified finalized slot is this far behind the best is lagging.
    pub max_lag_slots: u64,
    /// How many sources must serve the chosen update's finalized header before it is sent.
    pub min_agreeing: usize,
    /// Backoff after a transport failure (and a failed read of the chain): `retry_base_ms * 2^(attempt-1)` after
    /// attempt 1, 2, ..., at most `retry_max_ms`; `max_attempts` attempts in all.
    pub retry_base_ms: u64,
    pub retry_max_ms: u64,
    pub max_attempts: u32,
    /// Rounds a source that served something the program would refuse is not asked again.
    pub demote_rounds: u64,
    /// Periods asked of the `updates` endpoint at once.
    pub max_periods_per_request: u64,
    /// Finish a job this updater sent a step for (its own `Open`, or a later step of a job another party opened)
    /// before choosing another update (default): a beacon node serves only its newest finality update, so the update
    /// a job was opened for can be gone from every source before the job's last step, and the steps already paid would
    /// be lost (and a party that opens every new update's job and leaves it would have the updater pay a step of each).
    /// The update is kept in `state_dir/<chain>/open/` until the job is finished or moot, so a restarted updater
    /// finishes it too.
    #[serde(default = "default_true")]
    pub finish_own_jobs: bool,
    /// Client diversity (0, the default: off; otherwise at least 2): send only an update that sources reporting at
    /// least this many different consensus clients agree on, as `client_match` says. A source's client is what it
    /// reports at `/eth/v1/node/version`, asked every round, by its first word (`Lodestar/v1.51.0/7923280`:
    /// `lodestar`); a source that does not answer it counts under none. The provider's own, unauthenticated claim.
    /// The initialization and the committees are not subject to it: they are bound by hashes to the checkpoint and to
    /// the summaries on chain.
    #[serde(default)]
    pub min_distinct_clients: usize,
    /// What sources of different clients must agree on (with `min_distinct_clients`): `bytes` (default): the update's
    /// SSZ, byte for byte, as each light-client source served it (a finality update with a finality update, a period
    /// update with a period update); `finalized-header`: its finalized header, served by a light-client source or held
    /// canonical at its slot by a `headers` source (the agreement `min_agreeing` counts).
    #[serde(default)]
    pub client_match: ClientMatch,
}

/// What `policy.min_distinct_clients` compares.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClientMatch {
    #[default]
    Bytes,
    FinalizedHeader,
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
            min_distinct_clients: 0,
            client_match: ClientMatch::Bytes,
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
    /// A beacon node's REST API base URL; for `kind = "execution"`, an execution endpoint's JSON-RPC URL.
    #[serde(default)]
    pub url: Option<String>,
    /// A recorded directory in the lab's layout (dev mode).
    #[serde(default)]
    pub dir: Option<String>,
    /// `"beacon_api_module"`: Logos Core's module of that name, for `chain_id` (inside Logos Core only); for
    /// `kind = "execution"`, `"eth_rpc_module"`: Logos Core's Ethereum JSON-RPC module, whichever endpoint it holds for
    /// `chain_id`.
    #[serde(default)]
    pub via: Option<String>,
    /// The chain id `beacon_api_module` (or `eth_rpc_module`) is asked about (with `via`).
    #[serde(default)]
    pub chain_id: Option<i64>,
    /// The provider id `beacon_api_module` tries first (with `via`; default: its own order).
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// `light-client` (default): it serves bootstraps, updates and finality updates. `headers`: it only serves
    /// `/eth/v1/beacon/headers/{slot}` (most public beacon APIs), and counts towards `min_agreeing` for an update
    /// whose finalized header is the block it reports canonical at that slot. `execution`: an Ethereum execution
    /// endpoint (JSON-RPC: a `url`, or `via = "eth_rpc_module"`), which counts towards `min_agreeing` for an update
    /// whose finalized header's execution block (the block hash the light client binds) it holds canonical at its
    /// number (`eth_getBlockByHash`, then `eth_getBlockByNumber`).
    #[serde(default)]
    pub kind: SourceKind,
    /// A recorded `dir` only (it has no `/eth/v1/node/version`): the client it stands for, as a node would report it
    /// (`Nimbus/v26.10.2`), for `policy.min_distinct_clients`. A `url` source's client is what the node reports.
    #[serde(default)]
    pub client: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceKind {
    #[default]
    LightClient,
    Headers,
    Execution,
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
        if self.policy.min_distinct_clients == 1 {
            bail!("policy.min_distinct_clients = 1 asks nothing: 0 (off) or at least 2");
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
                    let known = if s.kind == SourceKind::Execution { "eth_rpc_module" } else { "beacon_api_module" };
                    if via != known {
                        bail!(
                            "chain {} source {}: via = {via:?}: \"beacon_api_module\" (light-client data), or \
                             \"eth_rpc_module\" with kind = \"execution\"",
                            c.name,
                            s.name
                        );
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
                if s.kind == SourceKind::Execution && s.dir.is_some() {
                    bail!("chain {} source {}: an execution source is a url or via = \"eth_rpc_module\"", c.name, s.name);
                }
                if s.provider.is_some() && s.kind == SourceKind::Execution {
                    bail!("chain {} source {}: provider is beacon_api_module's (an execution source has none)", c.name, s.name);
                }
            }
            if c.sources.iter().all(|s| s.kind != SourceKind::LightClient) {
                bail!(
                    "chain {}: no light-client source (every source is kind = \"headers\" or \"execution\")",
                    c.name
                );
            }
            if let Some(s) = c.sources.iter().find(|s| s.client.is_some() && s.dir.is_none()) {
                bail!(
                    "chain {} source {}: client is set only for a recorded dir (a node reports its own at \
                     /eth/v1/node/version)",
                    c.name,
                    s.name
                );
            }
            let k = self.policy.min_distinct_clients;
            if k >= 2 {
                // Sources that can report a client: a node's API (light-client, or headers when the finalized header is
                // what must agree), a recorded dir given one; not beacon_api_module, which fails over between providers.
                let able = c
                    .sources
                    .iter()
                    .filter(|s| s.via.is_none() && (s.url.is_some() || s.client.is_some()))
                    .filter(|s| {
                        s.kind == SourceKind::LightClient
                            || (s.kind == SourceKind::Headers && self.policy.client_match == ClientMatch::FinalizedHeader)
                    })
                    .count();
                if able < k {
                    bail!(
                        "chain {}: policy.min_distinct_clients = {k} but only {able} source(s) can count towards it",
                        c.name
                    );
                }
                // One provider listed twice would count as two clients (its version answer and its data can come from
                // different backends). Two hosts can still be one provider: the updater cannot tell.
                let mut hosts: std::collections::BTreeMap<String, &str> = std::collections::BTreeMap::new();
                for s in c.sources.iter().filter(|s| s.kind != SourceKind::Execution) {
                    let Some(url) = &s.url else { continue };
                    let host = url_host(url);
                    if let Some(other) = hosts.insert(host.clone(), &s.name) {
                        bail!(
                            "chain {} sources {other} and {}: the same host {host:?}; with policy.min_distinct_clients \
                             one provider would count as two clients",
                            c.name,
                            s.name
                        );
                    }
                }
            }
        }
        Ok(())
    }
}

/// The host of a URL, lower case, without scheme, user, port or path (`https://A.example:8080/x` -> `a.example`).
pub fn url_host(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = match host_port.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(""),
        None => host_port.split(':').next().unwrap_or(""),
    };
    host.trim_end_matches('.').to_ascii_lowercase()
}
