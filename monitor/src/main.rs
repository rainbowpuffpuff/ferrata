//! `monitor run`: the watcher daemon. `monitor check`: one offline verdict (no chain, no key).

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

use ferrata_monitor::check::{evaluate, Candidate, Policy, Rule};
use ferrata_monitor::cli;
use ferrata_monitor::node::{KeySet, NodeSource, NodeView};

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

// Parsed once at start; the size of the `run` variant does not matter.
#[allow(clippy::large_enum_variant)]
#[derive(Subcommand)]
enum Cmd {
    /// Follow a vault: scan its candidates, check them against the node views, cancel, pause, alert.
    Run(cli::RunArgs),
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

fn parse_rule(s: &str) -> Result<Rule> {
    match s {
        "A" | "a" => Ok(Rule::A),
        "B" | "b" => Ok(Rule::B),
        _ => bail!("rule A or B"),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Run(a) => cli::run(a, None).await,
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
