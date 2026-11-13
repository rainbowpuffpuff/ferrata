//! The crown run's LEZ side on a running LEZ v0.4 sequencer (`../README.md`).
//!
//! `crown-driver <command> [--option value]...` Every public transaction is built, signed and sent
//! by LEZ's wallet library; every private one is executed and proven by it (r0vm, LEZ's prover);
//! the result of each is read by following the chain with LEZ's block application.
//!
//! Commands against a sequencer on this machine (`--rpc`, default http://127.0.0.1:3040):
//!   crown-a   session A: deploy the attestation program and the merged bridge program, the
//!             Sepolia instance (Init and the repository fixture's updates, or the stand-in's
//!             bootstrap), then write `--identity-out` (session-a.json) at the last final block
//!   crown-b   session B: the captures' updates and finality updates, the bridge's Init (pairing),
//!             Register ETH, ImportRoot n=1, n=2, Claim (public), ClaimPrivately (real proof), Burn
//!             (public), Burn (private, real proof); then `--session-out` and `--notes-out`
//!   export    the records of the run into `--out` (blocks.borsh, chain.tsv, segments.tsv, ...)
//! Without a sequencer:
//!   config-id   the configuration id for `--vault` from `--identity` (session-a.json)
//!   check       the chain natively: the fixture's bootstrap and catch-up, then the `--capture`s: the light
//!               client, the bridge's requests under each record, every deposit's opening (what crown-b will
//!               send); fails if the next session's transactions would fall outside the weak-subjectivity
//!               period by this machine's clock (without --identity: session A's)
//!   capture-slots  the vault slots a capture must prove
//!
//! Common options: --program-bin FILE (attestation program), --bridge-bin FILE (merged bridge),
//! --chain sepolia|stand-in, --fixture DIR (a checkpoint fixture: crates/ferrata-spectests/fixtures/sepolia-2026-11-12),
//! --run-dir DIR (the driver's state: wallet, follower, manifest), --out DIR, --gas-limit N,
//! --wait-final yes|no, --sequencer-log FILE.
//! crown-a: --channel FILE (the node's /channel/<id> answer), --genesis FILE (deployment
//! settings), --identity-out FILE, [--vault ADDR].
//! crown-b: --identity FILE, --capture DIR (repeatable, in order), [--eth-fees-at UNIX_S],
//! [--recipient ADDR], [--private real|fake], [--claim-time-offset-s S (default -30)], --session-out FILE,
//! --notes-out FILE, [--operator-only yes] (the updates, Init, Register, ImportFees and ImportRoot only: the
//! claims and burns are a user's wallet's, e2e/seqnet).

#![allow(dead_code)]

#[path = "../../../live/driver/src/blocklog.rs"]
mod blocklog;
#[path = "../../../live/driver/src/follower.rs"]
mod follower;
#[path = "../../../live/driver/src/keys.rs"]
mod keys;
#[path = "../../../live/driver/src/live.rs"]
mod live;
#[path = "../../../live/driver/src/manifest.rs"]
mod manifest;
#[path = "../../../live/driver/src/plan.rs"]
mod plan;
#[path = "../../../live/driver/src/private.rs"]
mod private;
#[path = "../../../live/driver/src/programs.rs"]
mod programs;
#[path = "../../../live/driver/src/scenario.rs"]
mod scenario;
#[path = "../../../live/driver/src/verify.rs"]
mod verify;

mod crown;

use std::{
    collections::HashMap,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, ensure, Context, Result};
use ferrata_core::witness::CompactStore;
use ferrata_lez_wire::InstanceState;
use lee::PrivateKey;

use crate::{
    keys::lez_test_payers,
    live::{Live, Options},
    programs::GuestBinary,
    scenario::{deployment_accounts, deployment_keys},
};

pub struct Args {
    command: String,
    options: HashMap<String, Vec<String>>,
}

impl Args {
    fn parse() -> Result<Self> {
        let mut args = std::env::args().skip(1);
        let command = args.next().ok_or_else(|| anyhow!("usage: crown-driver <command> [--option value]... (see src/main.rs)"))?;
        let mut options: HashMap<String, Vec<String>> = HashMap::new();
        while let Some(arg) = args.next() {
            let name = arg.strip_prefix("--").ok_or_else(|| anyhow!("unexpected argument {arg}"))?.to_string();
            let value = args.next().ok_or_else(|| anyhow!("--{name} needs a value"))?;
            options.entry(name).or_default().push(value);
        }
        Ok(Self { command, options })
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.options.get(name).and_then(|v| v.last()).map(String::as_str)
    }

    pub fn path(&self, name: &str) -> Result<PathBuf> {
        self.get(name).map(PathBuf::from).ok_or_else(|| anyhow!("--{name} is required"))
    }

    pub fn all(&self, name: &str) -> Vec<PathBuf> {
        self.options.get(name).map(|v| v.iter().map(PathBuf::from).collect()).unwrap_or_default()
    }
}

fn binaries(args: &Args) -> Result<(GuestBinary, GuestBinary)> {
    Ok((
        GuestBinary::load(crown::ATTEST, &args.path("program-bin")?)?,
        GuestBinary::load(crown::BRIDGE, &args.path("bridge-bin")?)?,
    ))
}

/// The base chain of the run (`--chain`): Sepolia from the repository fixture's bootstrap, or the
/// stand-in's.
fn base_chain(args: &Args) -> Result<crown::Chain> {
    match args.get("chain").unwrap_or("sepolia") {
        "sepolia" => crown::sepolia_base(&args.path("fixture")?),
        "stand-in" => crown::stand_in_base(),
        other => bail!("--chain {other}: sepolia or stand-in"),
    }
}

/// The base chain with the captures' updates and stages.
fn full_chain(args: &Args) -> Result<crown::Chain> {
    let mut chain = base_chain(args)?;
    crown::add_captures(&mut chain, &args.all("capture"))?;
    Ok(chain)
}

async fn open(args: &Args, phase: &str, program: &GuestBinary, bridge: &GuestBinary) -> Result<Live> {
    let payer_keys = lez_test_payers()?;
    let mut keys: Vec<PrivateKey> = payer_keys;
    keys.extend(deployment_keys(crown::ATTEST, program));
    keys.extend(deployment_keys(crown::BRIDGE, bridge));
    let run_dir = args.path("run-dir")?;
    let opts = Options {
        rpc: args.get("rpc").unwrap_or("http://127.0.0.1:3040").to_string(),
        out_dir: args.get("out").map(PathBuf::from).unwrap_or_else(|| run_dir.join("out")),
        run_dir: run_dir.clone(),
        // The live driver's harness tables are not used here: an empty directory.
        harness_results: run_dir.join("no-harness-tables"),
        sequencer_log: args.get("sequencer-log").map(PathBuf::from),
        gas_limit: args.get("gas-limit").map(str::parse).transpose()?.unwrap_or(4_000_000),
        wait_final: args.get("wait-final") != Some("no"),
    };
    fs::create_dir_all(&opts.harness_results)?;
    let mut live = Live::open(opts, &keys).await?;
    live.phase = phase.to_string();
    Ok(live)
}

/// Write the records of the run: the manifest, the chain (blocks without program bytecode, the
/// chunks removed), the programs and the instance as the chain holds them (the live driver's
/// `export`, for the crown deployment).
async fn export(live: &mut Live, program: &GuestBinary, bridge: &GuestBinary, chain: &crown::Chain) -> Result<()> {
    live.sync().await?;
    live.save()?;
    let out = live.opts.out_dir.clone();
    fs::create_dir_all(&out)?;
    manifest::write(&out.join("run.tsv"), &live.rows)?;

    let mut blocks = live.follower.blocks.clone();
    let mut statuses = Vec::new();
    for block in &mut blocks {
        let id = block.header.block_id;
        let served = live.wallet.get_block(id).await?.ok_or_else(|| anyhow!("the sequencer has no block {id}"))?;
        ensure!(served.header.hash == block.header.hash, "block {id} changed under the run");
        block.bedrock_status = served.bedrock_status;
        statuses.push(format!("{:?}", block.bedrock_status));
    }
    let (elided, removed) = blocklog::elide(&blocks)?;
    blocklog::save(&out.join("blocks.borsh"), &elided)?;
    blocklog::write_segments(&out.join("segments.tsv"), &removed)?;

    let mut table = String::from(
        "block\thash\ttimestamp_ms\ttransactions\tuser_transactions\treverted\tbase_fee_exec\tbase_fee_stor\tgas_exec\tgas_stor\trevenue_base\tbedrock_status\n",
    );
    for (info, status) in live.follower.chain.iter().zip(&statuses) {
        let reverted = info.reverted.iter().map(u64::to_string).collect::<Vec<_>>().join(",");
        writeln!(
            table,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            info.block_id,
            info.hash,
            info.timestamp_ms,
            info.transactions,
            info.user_transactions,
            if reverted.is_empty() { "-" } else { &reverted },
            info.base_fee_exec,
            info.base_fee_stor,
            info.gas_exec,
            info.gas_stor,
            info.revenue_base,
            status
        )?;
    }
    fs::write(out.join("chain.tsv"), table)?;

    let mut programs = String::from("name\theader_account\timage_id\tbinary_sha256\tbinary_bytes\tuser_elf_bytes\tsegments\n");
    for (name, binary) in [(crown::ATTEST, program), (crown::BRIDGE, bridge)] {
        let (header, segments) = deployment_accounts(name, binary);
        writeln!(
            programs,
            "{name}\t{header}\t{}\t{}\t{}\t{}\t{}",
            binary.image_hex(),
            binary.sha256(),
            binary.bytes.len(),
            binary.user_elf.len(),
            segments.len()
        )?;
    }
    fs::write(out.join("programs.tsv"), programs)?;

    let attest = deployment_accounts(crown::ATTEST, program).0;
    let inst = crate::plan::Instance::new(&chain.name, attest, chain.config.clone(), chain.checkpoint);
    let mut instances = String::from("instance\tprogram_account\tinstance_id\tinstance_account\tactive\tjobs_applied\tfinalized_slot\tfinalized_root\n");
    let state = live.actor_state(inst.pda().actor()).await?;
    if !state.is_empty() {
        let state: InstanceState = borsh::from_slice(&state)?;
        let store = CompactStore::decode(&state.store).map_err(|e| anyhow!("{}: {e:?}", inst.name))?;
        writeln!(
            instances,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            inst.name,
            inst.program,
            hex::encode(inst.id),
            inst.pda().account(),
            state.active,
            state.applied,
            store.finalized.slot,
            hex::encode(store.finalized.hash_tree_root())
        )?;
    }
    fs::write(out.join("instances.tsv"), instances)?;

    let count = |status: &str| live.rows.iter().filter(|r| r.status == status).count();
    let mut summary = String::new();
    writeln!(summary, "blocks: 1..={}", live.follower.height())?;
    writeln!(
        summary,
        "transactions: {} sent; {} applied, {} refused with an exit code, {} reverted otherwise, {} dropped by the sequencer, {} rejected at its RPC, {} unfinished",
        live.rows.len(),
        count("applied"),
        count("refused"),
        count("reverted"),
        count("dropped"),
        count("rejected"),
        count("unfinished")
    )?;
    writeln!(
        summary,
        "blocks finalized on Bedrock at export: {} of {}",
        statuses.iter().filter(|s| s.as_str() == "Finalized").count(),
        statuses.len()
    )?;
    fs::write(out.join("summary.txt"), &summary)?;
    print!("{summary}");
    println!("records written to {}", out.display());
    Ok(())
}

async fn run(args: Args) -> Result<()> {
    match args.command.as_str() {
        "config-id" => {
            let identity = crown::Identity::load(&args.path("identity")?)?;
            let vault = crown::address(args.get("vault").ok_or_else(|| anyhow!("--vault is required"))?)?;
            println!("{}", serde_json::to_string_pretty(&identity.config_json(vault))?);
            return Ok(());
        }
        "destinations" => {
            let (_, public0) = crown::destination(0)?;
            let (_, private0) = crown::destination(1)?;
            println!("{}", serde_json::json!({ "public0": format!("0x{}", hex::encode(public0)), "private0": format!("0x{}", hex::encode(private0)) }));
            return Ok(());
        }
        "capture-slots" => {
            println!("{}", serde_json::to_string_pretty(&crown::capture_slots())?);
            return Ok(());
        }
        "check" => {
            let (program, bridge) = binaries(&args)?;
            let chain = full_chain(&args)?;
            let identity = match args.get("identity") {
                Some(p) => Some(crown::Identity::load(Path::new(p))?),
                None => None,
            };
            return crown::check(&chain, &program, &bridge, identity.as_ref());
        }
        _ => {}
    }
    let (program, bridge) = binaries(&args)?;
    let mut live = open(&args, args.command.as_str(), &program, &bridge).await?;
    match args.command.as_str() {
        "crown-a" => {
            let chain = base_chain(&args)?;
            let a = crown::SessionA {
                channel: args.path("channel")?,
                genesis: args.path("genesis")?,
                identity_out: args.path("identity-out")?,
                vault: args.get("vault").map(crown::address).transpose()?,
            };
            crown::session_a(&mut live, &program, &bridge, &chain, &a).await?;
        }
        "crown-b" => {
            let chain = full_chain(&args)?;
            let b = crown::SessionB {
                identity: crown::Identity::load(&args.path("identity")?)?,
                claim_time_offset_s: args.get("claim-time-offset-s").map(str::parse).transpose()?.unwrap_or(-30),
                eth_fees_at: args.get("eth-fees-at").map(str::parse).transpose()?,
                recipient: crown::address(args.get("recipient").unwrap_or(crown::DEFAULT_RECIPIENT))?,
                private: args.get("private").unwrap_or("real").to_string(),
                session_out: args.path("session-out")?,
                notes_out: args.path("notes-out")?,
                operator_only: args.get("operator-only") == Some("yes"),
            };
            crown::session_b(&mut live, &program, &bridge, &chain, &b).await?;
        }
        "export" => {
            let chain = base_chain(&args)?;
            export(&mut live, &program, &bridge, &chain).await?;
        }
        other => bail!("unknown command {other}"),
    }
    live.save().context("saving the run's state")
}

fn main() -> Result<()> {
    std::env::set_var(wallet::SUPPRESS_VERBOSE_PRINTS, "1");
    let _ = env_logger::try_init();
    let args = Args::parse()?;
    tokio::runtime::Builder::new_multi_thread().enable_all().build()?.block_on(run(args))
}
