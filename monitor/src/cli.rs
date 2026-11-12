//! `monitor run`'s arguments and its start: shared by the binary (`src/main.rs`) and the Logos module
//! (`modules/ferrata_monitor_module`), which parses the same arguments and runs the same daemon.

use std::path::PathBuf;
use std::time::Duration;

use alloy::{primitives::Address, signers::local::PrivateKeySigner};
use anyhow::{bail, Context, Result};

use crate::alert::{Alerts, Level};
use crate::chain::Chain;
use crate::daemon::{self, Config};
use crate::node::{NodeSource, H32};

#[derive(clap::Args, Clone, Debug)]
pub struct RunArgs {
    /// Ethereum JSON-RPC endpoint (the watcher's own node).
    #[arg(long, env = "MONITOR_RPC_URL")]
    pub rpc: String,
    #[arg(long, env = "MONITOR_VAULT")]
    pub vault: Address,
    /// A node view source, file:PATH or cmd:COMMAND; repeat for several nodes (all must agree).
    #[arg(long = "node", required = true)]
    pub nodes: Vec<NodeSource>,
    /// The state file (restart safety); written atomically.
    #[arg(long)]
    pub state: PathBuf,
    /// Name of an environment variable that holds the watcher key (hex). Never on the command line.
    #[arg(long, conflicts_with_all = ["keystore", "key_file"])]
    pub key_env: Option<String>,
    /// A file that holds the watcher key (hex), readable by the monitor only. Never on the command line.
    #[arg(long, conflicts_with = "keystore")]
    pub key_file: Option<PathBuf>,
    /// A Web3 Secret Storage keystore with the watcher key, opened with --password-file.
    #[arg(long, requires = "password_file")]
    pub keystore: Option<PathBuf>,
    #[arg(long)]
    pub password_file: Option<PathBuf>,
    /// Append alerts (JSON lines) to this file.
    #[arg(long)]
    pub alert_log: Option<PathBuf>,
    /// Run this command (sh -c) with each alert of at least --alert-level on its standard input.
    #[arg(long)]
    pub alert_cmd: Option<String>,
    #[arg(long, default_value = "warn", value_parser = parse_level)]
    pub alert_level: Level,
    /// First block to scan when there is no state file (the vault's deployment block).
    #[arg(long, default_value_t = 0)]
    pub start_block: u64,
    /// Seconds between polls.
    #[arg(long, default_value_t = 12)]
    pub poll: u64,
    /// Stop after this many polls.
    #[arg(long)]
    pub max_polls: Option<u64>,
    /// Rule (B): seconds after a submission the own node may still lack the tip in its final tier.
    #[arg(long, default_value_t = 300)]
    pub b_lag_grace: u64,
    /// Rule (A): Δ_g in seconds (2 h; 3 h with partial participation, D13).
    #[arg(long, default_value_t = 7200)]
    pub delta_g: u64,
    /// Seconds before a candidate's deadline by which a cancellation is sent.
    #[arg(long, default_value_t = 120)]
    pub margin: u64,
    #[arg(long, default_value_t = 2)]
    pub confirmations: u64,
    #[arg(long, default_value_t = 64)]
    pub reorg_margin: u64,
    #[arg(long, default_value_t = 5000)]
    pub log_chunk: u64,
    /// Pause checkpoints when a state is cancelled for the n-th time (D11).
    #[arg(long, default_value_t = 2)]
    pub pause_after_cancels: u32,
    /// Rule (A): pause and alert when the final tier has not advanced for this many seconds (0: off).
    #[arg(long, default_value_t = 0)]
    pub stall_pause_after: u64,
    /// Rule (B): the channel id, to audit the attestors' published final-tier messages and certificates.
    #[arg(long)]
    pub channel_id: Option<H32>,
    /// Seconds a sent transaction may stay unmined before the poll gives up on it (alerted; retried).
    #[arg(long, default_value_t = 180)]
    pub tx_timeout: u64,
    /// Decide and alert; send no transaction.
    #[arg(long)]
    pub dry_run: bool,
}

pub fn parse_level(s: &str) -> Result<Level> {
    match s {
        "info" => Ok(Level::Info),
        "warn" => Ok(Level::Warn),
        "critical" => Ok(Level::Critical),
        _ => bail!("info, warn or critical"),
    }
}

/// The watcher key: from the environment variable `--key-env` names, the file `--key-file` names, or
/// the keystore; none: the monitor decides and alerts only.
pub fn signer(a: &RunArgs) -> Result<Option<PrivateKeySigner>> {
    if let Some(var) = &a.key_env {
        let hexed = std::env::var(var).with_context(|| format!("{var} is not set"))?;
        return Ok(Some(hexed.trim().parse().context("the watcher key does not parse")?));
    }
    if let Some(p) = &a.key_file {
        let hexed = std::fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))?;
        return Ok(Some(hexed.trim().parse().context("the watcher key does not parse")?));
    }
    if let (Some(ks), Some(pw)) = (&a.keystore, &a.password_file) {
        let password = std::fs::read_to_string(pw).with_context(|| format!("reading {}", pw.display()))?;
        let s = PrivateKeySigner::decrypt_keystore(ks, password.trim_end_matches(['\n', '\r']))
            .map_err(|e| anyhow::anyhow!("the keystore does not open: {e}"))?;
        return Ok(Some(s));
    }
    Ok(None)
}

/// Connects to the Ethereum node and runs the daemon ([`daemon::run_with`]): until SIGTERM or SIGINT
/// when `stop` is `None` (the binary), until `stop` turns true otherwise (the Logos module, whose
/// process the signals belong to); `--max-polls` ends it in both.
pub async fn run(a: RunArgs, stop: Option<tokio::sync::watch::Receiver<bool>>) -> Result<()> {
    let mut chain = Chain::connect(&a.rpc, a.vault, signer(&a)?)?;
    chain.tx_timeout = Duration::from_secs(a.tx_timeout);
    let chain_id = {
        use alloy::providers::Provider as _;
        chain.p.get_chain_id().await?
    };
    let alerts = Alerts { log: a.alert_log.clone(), cmd: a.alert_cmd.clone(), cmd_level: Some(a.alert_level) };
    let cfg = Config {
        vault: a.vault,
        nodes: a.nodes,
        state: a.state,
        start_block: a.start_block,
        poll: Duration::from_secs(a.poll),
        max_polls: a.max_polls,
        b_lag_grace: a.b_lag_grace,
        delta_g: a.delta_g,
        margin: a.margin,
        confirmations: a.confirmations,
        reorg_margin: a.reorg_margin,
        log_chunk: a.log_chunk,
        pause_after_cancels: a.pause_after_cancels,
        stall_pause_after: a.stall_pause_after,
        dry_run: a.dry_run,
        channel_id: a.channel_id,
        chain_id,
    };
    daemon::run_with(cfg, chain, alerts, stop).await
}
