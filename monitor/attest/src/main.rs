//! `attest-monitor`: the attestation side's monitor (RFP-021 S.7, VS.4). See `src/lib.rs` and `README.md`.
//!
//! Exit status of `check`: 0 healthy (no alert, or information only), 10 a warning, 20 a critical alert;
//! 1 an error (the instance could not be read); 2 a usage error.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};

use ferrata_attest_monitor::alert::{Level, Sink};
use ferrata_attest_monitor::source::{query_provider, read_state, Provider, StateSource, Target};
use ferrata_attest_monitor::watch::{self, Clock, Watch};
use ferrata_attest_monitor::{assess, hex0x, parse_root, Policy, ProviderView};
use lee_core::account::AccountId;

#[derive(Parser)]
#[command(name = "attest-monitor", version, about = "Watches a Ferrata attestation instance against several beacon providers: stall, weak-subjectivity, divergence and ahead-of-finality alarms")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Args, Clone)]
struct Common {
    /// The instance's state: lez:URL (a LEZ sequencer's JSON-RPC), file:PATH or cmd:COMMAND.
    #[arg(long)]
    state: StateSource,
    /// The attestation program's account (hex), for lez:.
    #[arg(long)]
    program: Option<String>,
    /// The instance id (hex), for lez:.
    #[arg(long)]
    instance: Option<String>,
    /// A beacon provider, NAME=URL or URL; repeat for each.
    #[arg(long = "provider")]
    providers: Vec<Provider>,
    /// Instead of asking providers: their answers as JSON (a list of `ProviderView`), for offline checks.
    #[arg(long, conflicts_with = "providers")]
    views: Option<PathBuf>,
    /// Agreeing providers needed, with none disagreeing, to confirm the instance's finalized header (1 or more).
    #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u64).range(1..))]
    min_agreeing: u64,
    /// Disagreeing providers that raise `instance-diverged` (critical), whatever the number agreeing. 1 (default): any
    /// provider that disagrees. 2, with three or more providers: no single provider can raise the alarm or hide it.
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u64).range(1..))]
    diverge_at: u64,
    /// Lag behind the providers' finalized header that raises `instance-stalled`.
    #[arg(long, default_value_t = Policy::default().stall_after_s)]
    stall_after: u64,
    /// Seconds past the chain's finality delay (two epochs) that the instance's finalized header may be ahead of every
    /// provider (none has finalized its slot or holds it there) before `instance-ahead` (critical).
    #[arg(long, default_value_t = Policy::default().ahead_grace_s)]
    ahead_grace: u64,
    /// The chain's clock at the start, UNIX seconds (default: the system clock).
    #[arg(long)]
    now: Option<u64>,
    #[arg(long, default_value_t = 15)]
    timeout: u64,
    /// Append every alert to this file (JSON lines).
    #[arg(long)]
    alert_log: Option<PathBuf>,
    /// Run this command (sh -c) for each alert at or above --alert-cmd-level, the alert's JSON on its standard input:
    /// a pager, a webhook, or the consumer's freeze tool.
    #[arg(long)]
    alert_cmd: Option<String>,
    #[arg(long, default_value = "warn")]
    alert_cmd_level: Level,
}

impl Common {
    fn target(&self) -> Result<Option<Target>> {
        Ok(match (&self.program, &self.instance) {
            (Some(p), Some(i)) => Some(Target { program: AccountId::new(parse_root(p)?), instance_id: parse_root(i)? }),
            _ => None,
        })
    }
    fn policy(&self) -> Policy {
        Policy {
            min_agreeing: self.min_agreeing as usize,
            diverge_at: self.diverge_at as usize,
            stall_after_s: self.stall_after,
            ahead_grace_s: self.ahead_grace,
        }
    }
    fn sink(&self) -> Sink {
        Sink { log: self.alert_log.clone(), cmd: self.alert_cmd.clone(), cmd_level: Some(self.alert_cmd_level) }
    }
    fn clock(&self) -> Clock {
        self.now.map(Clock::at).unwrap_or_else(Clock::system)
    }
}

#[derive(Subcommand)]
enum Cmd {
    /// One assessment: prints it as JSON, emits its alerts, and exits 0, 10 or 20.
    Check(Common),
    /// The daemon: an assessment every --interval seconds, alerts on changes, until SIGTERM, SIGINT or --max-polls.
    Watch {
        #[command(flatten)]
        common: Common,
        #[arg(long, default_value_t = 300)]
        interval: u64,
        /// Emit an alert that still holds again after this many seconds.
        #[arg(long, default_value_t = 3600)]
        repeat: u64,
        #[arg(long)]
        max_polls: Option<u64>,
        /// The latest assessment, rewritten after each poll (JSON).
        #[arg(long)]
        status_file: Option<PathBuf>,
        /// The finalized headers seen and their verdicts, kept across restarts (JSON).
        #[arg(long)]
        state_file: Option<PathBuf>,
    },
    /// The instance's state, decoded (JSON).
    State(Common),
}

fn main() {
    let code = match run() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("attest-monitor: {e:#}");
            1
        }
    };
    std::process::exit(code);
}

fn run() -> Result<i32> {
    match Cli::parse().cmd {
        Cmd::Check(c) => {
            let target = c.target()?;
            let state = read_state(&c.state, target.as_ref(), c.timeout)?;
            let slot = ferrata_core::witness::CompactStore::decode(&state.store)
                .map_err(|e| anyhow::anyhow!("the instance's store does not decode: {e:?}"))?
                .finalized
                .slot;
            let views: Vec<ProviderView> = match &c.views {
                Some(p) => serde_json::from_slice(&std::fs::read(p).with_context(|| format!("reading {}", p.display()))?)
                    .context("the views do not parse")?,
                None => c.providers.iter().map(|p| query_provider(p, slot, c.timeout)).collect(),
            };
            let a = assess(&state, &views, c.clock().now(), &c.policy())?;
            let sink = c.sink();
            for alert in &a.alerts {
                sink.emit(alert);
            }
            println!("{}", serde_json::to_string_pretty(&a)?);
            Ok(match a.level() {
                Some(Level::Critical) => 20,
                Some(Level::Warn) => 10,
                _ => 0,
            })
        }
        Cmd::Watch { common: c, interval, repeat, max_polls, status_file, state_file } => {
            let w = Watch {
                state: c.state.clone(),
                target: c.target()?,
                providers: c.providers.clone(),
                policy: c.policy(),
                interval_s: interval,
                repeat_s: repeat,
                max_polls,
                timeout_s: c.timeout,
                clock: c.clock(),
                sink: c.sink(),
                status_file,
                state_file,
            };
            let stop = Arc::new(AtomicBool::new(false));
            signal_hook::flag::register(signal_hook::consts::SIGTERM, stop.clone())?;
            signal_hook::flag::register(signal_hook::consts::SIGINT, stop.clone())?;
            let s = watch::run(&w, stop);
            println!("{}", serde_json::json!({ "polls": s.polls, "alerts": s.emitted.len(), "seen": s.seen }));
            Ok(0)
        }
        Cmd::State(c) => {
            let state = read_state(&c.state, c.target()?.as_ref(), c.timeout)?;
            let store = ferrata_core::witness::CompactStore::decode(&state.store)
                .map_err(|e| anyhow::anyhow!("the instance's store does not decode: {e:?}"))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "id": hex0x(&state.id),
                    "root_id": hex0x(&state.root_id),
                    "chain_id": state.config.chain_id,
                    "active": state.active,
                    "applied": state.applied,
                    "finalized_slot": store.finalized.slot,
                    "finalized_root": hex0x(&store.finalized.hash_tree_root()),
                    "next_committee_known": store.next.is_some(),
                    "ws_end_slot": state.config.ws_end_slot(store.finalized.slot),
                    "genesis_time": state.config.genesis_time,
                }))?
            );
            Ok(0)
        }
    }
}
