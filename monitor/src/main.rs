//! `monitor run`: the watcher daemon. `monitor check`: one offline verdict (no chain, no key).

use std::path::PathBuf;
use std::time::Duration;

use alloy::{primitives::Address, signers::local::PrivateKeySigner};
use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

use ferrata_monitor::alert::{Alerts, Level};
use ferrata_monitor::chain::Chain;
use ferrata_monitor::check::{evaluate, Candidate, Policy, Rule};
use ferrata_monitor::daemon::{self, Config};
use ferrata_monitor::node::{KeySet, NodeSource, NodeView, H32};

#[derive(Parser)]
#[command(
    name = "monitor",
    version,
    about = "Ferrata watcher: checks every candidate checkpoint against its own nodes and cancels a wrong one in time"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Follow a vault: scan its candidates, check them against the node views, cancel, pause, alert.
    Run(RunArgs),
    /// Print a node view built from a LEZ sequencer's block record (`chain.tsv` of the lab driver:
    /// id, hash, bedrock_status) and a key-set list (JSON); usable as `--node 'cmd:monitor node-view ...'`.
    NodeView {
        #[arg(long)]
        chain_tsv: PathBuf,
        #[arg(long, default_value = "lez")]
        label: String,
        /// JSON list of key sets: [{"fromBlock", "keys", "postingTimeframe"}].
        #[arg(long)]
        key_sets: Option<PathBuf>,
    },
    /// One verdict, offline: a candidate (JSON) against node views at a given chain time.
    Check {
        #[arg(long, value_parser = parse_rule)]
        rule: Rule,
        #[arg(long)]
        candidate: PathBuf,
        #[arg(long = "node", required = true)]
        nodes: Vec<NodeSource>,
        #[arg(long)]
        now: u64,
        #[arg(long, default_value_t = 0)]
        turn_seconds: u64,
        #[arg(long, default_value_t = 300)]
        b_lag_grace: u64,
        #[arg(long, default_value_t = 7200)]
        delta_g: u64,
        #[arg(long, default_value_t = 120)]
        margin: u64,
    },
}

#[derive(clap::Args)]
struct RunArgs {
    /// Ethereum JSON-RPC endpoint (the watcher's own node).
    #[arg(long, env = "MONITOR_RPC_URL")]
    rpc: String,
    #[arg(long, env = "MONITOR_VAULT")]
    vault: Address,
    /// A node view source, file:PATH or cmd:COMMAND; repeat for several nodes (all must agree).
    #[arg(long = "node", required = true)]
    nodes: Vec<NodeSource>,
    /// The state file (restart safety); written atomically.
    #[arg(long)]
    state: PathBuf,
    /// Name of an environment variable that holds the watcher key (hex). Never on the command line.
    #[arg(long, conflicts_with = "keystore")]
    key_env: Option<String>,
    /// A Web3 Secret Storage keystore with the watcher key, opened with --password-file.
    #[arg(long, requires = "password_file")]
    keystore: Option<PathBuf>,
    #[arg(long)]
    password_file: Option<PathBuf>,
    /// Append alerts (JSON lines) to this file.
    #[arg(long)]
    alert_log: Option<PathBuf>,
    /// Run this command (sh -c) with each alert of at least --alert-level on its standard input.
    #[arg(long)]
    alert_cmd: Option<String>,
    #[arg(long, default_value = "warn", value_parser = parse_level)]
    alert_level: Level,
    /// First block to scan when there is no state file (the vault's deployment block).
    #[arg(long, default_value_t = 0)]
    start_block: u64,
    /// Seconds between polls.
    #[arg(long, default_value_t = 12)]
    poll: u64,
    /// Stop after this many polls.
    #[arg(long)]
    max_polls: Option<u64>,
    /// Rule (B): seconds after a submission the own node may still lack the tip in its final tier.
    #[arg(long, default_value_t = 300)]
    b_lag_grace: u64,
    /// Rule (A): Δ_g in seconds (2 h; 3 h with partial participation, D13).
    #[arg(long, default_value_t = 7200)]
    delta_g: u64,
    /// Seconds before a candidate's deadline by which a cancellation is sent.
    #[arg(long, default_value_t = 120)]
    margin: u64,
    #[arg(long, default_value_t = 2)]
    confirmations: u64,
    #[arg(long, default_value_t = 64)]
    reorg_margin: u64,
    #[arg(long, default_value_t = 5000)]
    log_chunk: u64,
    /// Pause checkpoints when a state is cancelled for the n-th time (D11).
    #[arg(long, default_value_t = 2)]
    pause_after_cancels: u32,
    /// Rule (A): pause and alert when the final tier has not advanced for this many seconds (0: off).
    #[arg(long, default_value_t = 0)]
    stall_pause_after: u64,
    /// Rule (B): the channel id, to audit the attestors' published final-tier messages and certificates.
    #[arg(long)]
    channel_id: Option<H32>,
    /// Seconds a sent transaction may stay unmined before the poll gives up on it (alerted; retried).
    #[arg(long, default_value_t = 180)]
    tx_timeout: u64,
    /// Decide and alert; send no transaction.
    #[arg(long)]
    dry_run: bool,
}

fn parse_rule(s: &str) -> Result<Rule> {
    match s {
        "A" | "a" => Ok(Rule::A),
        "B" | "b" => Ok(Rule::B),
        _ => bail!("rule A or B"),
    }
}

fn parse_level(s: &str) -> Result<Level> {
    match s {
        "info" => Ok(Level::Info),
        "warn" => Ok(Level::Warn),
        "critical" => Ok(Level::Critical),
        _ => bail!("info, warn or critical"),
    }
}

fn signer(a: &RunArgs) -> Result<Option<PrivateKeySigner>> {
    if let Some(var) = &a.key_env {
        let hexed = std::env::var(var).with_context(|| format!("{var} is not set"))?;
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

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Run(a) => {
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
            daemon::run(cfg, chain, alerts).await
        }
        Cmd::NodeView { chain_tsv, label, key_sets } => {
            let text =
                std::fs::read_to_string(&chain_tsv).with_context(|| format!("reading {}", chain_tsv.display()))?;
            let sets: Vec<KeySet> = match key_sets {
                Some(p) => serde_json::from_slice(&std::fs::read(&p)?).context("the key sets do not parse")?,
                None => Vec::new(),
            };
            println!("{}", serde_json::to_string_pretty(&NodeView::from_chain_tsv(&label, &text, sets)?)?);
            Ok(())
        }
        Cmd::Check { rule, candidate, nodes, now, turn_seconds, b_lag_grace, delta_g, margin } => {
            let c: Candidate =
                serde_json::from_slice(&std::fs::read(&candidate)?).context("the candidate does not parse")?;
            let views = nodes.iter().map(NodeSource::read).collect::<Result<Vec<_>>>()?;
            let p = Policy { rule, turn_seconds, b_lag_grace, delta_g, margin };
            println!("{}", serde_json::to_string_pretty(&evaluate(&p, &c, &views, now))?);
            Ok(())
        }
    }
}
