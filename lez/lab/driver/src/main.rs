//! The lab driver: Ferrata's light-client program kept following Sepolia and mainnet on a LEZ
//! v0.4.0-rc1 sequencer, one session at a time (`../README.md`).
//!
//!   lab-driver session --rpc URL --run-dir DIR --program-bin BIN --inputs DIR --out DIR
//!                      [--deadline UNIX_SECONDS] [--session ID] [--chains "sepolia mainnet"]
//!   lab-driver info    --rpc URL --run-dir DIR --program-bin BIN
//!   lab-driver verify  --dir RESULTS [--binary BIN]      offline: the record replays
//!
//! The chain modules are the live driver's (`../../live/driver/src/`), unchanged: LEZ's wallet
//! sends every transaction, the follower applies only blocks final on Bedrock and reads each
//! charge with LEZ's own settlement, the manifest records every transaction.
#![allow(dead_code)]

#[path = "../../../live/driver/src/blocklog.rs"]
mod blocklog;
#[path = "../../../live/driver/src/follower.rs"]
mod follower;
#[path = "../../../live/driver/src/keys.rs"]
mod keys;
mod lab;
#[path = "../../../live/driver/src/live.rs"]
mod live;
#[path = "../../../live/driver/src/manifest.rs"]
mod manifest;
mod plan;
#[path = "../../../live/driver/src/programs.rs"]
mod programs;
#[path = "../../../live/driver/src/verify.rs"]
mod verify;

use std::{collections::HashMap, fmt::Write as _, fs, path::PathBuf};

use anyhow::{anyhow, ensure, Result};
use ferrata_core::witness::{CompactStore, HeaderRecord};
use ferrata_lez_wire::{InstanceState, RecordState};
use lee::AccountId;

use crate::{
    keys::{account_of, lez_test_payers},
    lab::{Chain, Follow, DEPLOYMENT},
    live::{Live, Options},
    programs::GuestBinary,
};

struct Args {
    command: String,
    named: HashMap<String, String>,
}

impl Args {
    fn parse() -> Result<Self> {
        let mut it = std::env::args().skip(1);
        let command = it.next().ok_or_else(|| anyhow!("usage: lab-driver session|info|verify --name value ..."))?;
        let mut named = HashMap::new();
        while let Some(k) = it.next() {
            let k = k.strip_prefix("--").ok_or_else(|| anyhow!("expected --name, got {k}"))?.to_string();
            let v = it.next().ok_or_else(|| anyhow!("--{k} needs a value"))?;
            named.insert(k, v);
        }
        Ok(Self { command, named })
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.named.get(name).map(String::as_str)
    }

    fn path(&self, name: &str) -> Result<PathBuf> {
        self.get(name).map(PathBuf::from).ok_or_else(|| anyhow!("--{name} is required"))
    }
}

async fn open(args: &Args, program: &GuestBinary) -> Result<(Live, Vec<AccountId>)> {
    let payer_keys = lez_test_payers()?;
    let mut keys = payer_keys.clone();
    keys.extend(lab::deployment_keys(DEPLOYMENT, program));
    let run_dir = args.path("run-dir")?;
    let opts = Options {
        rpc: args.get("rpc").unwrap_or("http://127.0.0.1:3040").to_string(),
        out_dir: args.get("out").map(PathBuf::from).unwrap_or_else(|| run_dir.join("out")),
        harness_results: run_dir.join("no-harness-tables"),
        run_dir,
        sequencer_log: args.get("sequencer-log").map(PathBuf::from),
        gas_limit: lab::JOB_GAS_LIMIT,
        wait_final: true,
    };
    let live = Live::open(opts, &keys).await?;
    Ok((live, payer_keys.iter().map(account_of).collect()))
}

async fn run_chain(
    live: &mut Live,
    chain: Chain,
    program: AccountId,
    args: &Args,
    payer: AccountId,
    session: &str,
    deadline: u64,
) -> Result<(usize, usize)> {
    let inputs = args.path("inputs")?;
    let run_dir = live.opts.run_dir.clone();
    let mut f = match Follow::load(&run_dir, chain, program)? {
        Some(f) => f,
        None => lab::init(live, chain, program, &inputs, payer, session).await?,
    };
    // Nothing kept from a previous session is trusted: the instance on the chain must hold the
    // native store rebuilt from the kept bootstrap and updates.
    f.check(live).await?;
    lab::follow(live, &mut f, &inputs, payer, session, deadline).await
}

async fn session(args: &Args) -> Result<i32> {
    let started = lab::now();
    let session = args.get("session").map(str::to_string).unwrap_or_else(|| started.to_string());
    let deadline = args.get("deadline").map(|s| s.parse::<u64>()).transpose()?.unwrap_or(started + 1800);
    let program = GuestBinary::load("program", &args.path("program-bin")?)?;
    let (mut live, payers) = open(args, &program).await?;
    live.phase = session.clone();
    println!("[session {session}] sequencer head {}, follower at block {}", live.head().await?, live.follower.height());
    let program_account = lab::deploy(&mut live, &program, payers[0]).await?;
    let chains: Vec<Chain> = args.get("chains").unwrap_or("sepolia mainnet").split_whitespace().map(Chain::parse).collect::<Result<_>>()?;
    let mut report = format!("session\t{session}\nstarted\t{started}\n");
    let mut failed = 0;
    for chain in chains {
        let payer = payers[chain.payer_index()];
        match run_chain(&mut live, chain, program_account, args, payer, &session, deadline).await {
            Ok((jobs, handoffs)) => {
                println!("[{}] {jobs} job(s) applied this session, {handoffs} committee handoff(s)", chain.name());
                writeln!(report, "{}\tok\tjobs {jobs}\thandoffs {handoffs}", chain.name())?;
            }
            Err(e) => {
                failed += 1;
                println!("[{}] FAILED: {e:#}", chain.name());
                writeln!(report, "{}\tFAILED\t{}", chain.name(), format!("{e:#}").replace(['\n', '\t'], " "))?;
            }
        }
    }
    let out = live.opts.out_dir.clone();
    export(&mut live, &program, program_account).await?;
    writeln!(report, "finished\t{}\nhead_block\t{}\nfollowed_to\t{}", lab::now(), live.head().await?, live.follower.height())?;
    fs::write(out.join("session.tsv"), report)?;
    Ok(if failed == 0 { 0 } else { 3 })
}

/// The records: the manifest, the chain (blocks without program bytecode, the chunks removed,
/// one line per block), the program, the instances, every job and every update applied.
async fn export(live: &mut Live, program: &GuestBinary, program_account: AccountId) -> Result<()> {
    live.sync().await?;
    live.save()?;
    let out = live.opts.out_dir.clone();
    fs::create_dir_all(&out)?;
    manifest::write(&out.join("run.tsv"), &live.rows)?;
    let (elided, removed) = blocklog::elide(&live.follower.blocks)?;
    blocklog::save(&out.join("blocks.borsh"), &elided)?;
    blocklog::write_segments(&out.join("segments.tsv"), &removed)?;
    let mut chain = String::from(
        "block\thash\ttimestamp_ms\ttransactions\tuser_transactions\treverted\tbase_fee_exec\tbase_fee_stor\tgas_exec\tgas_stor\trevenue_base\tbedrock_status\n",
    );
    for (info, block) in live.follower.chain.iter().zip(&live.follower.blocks) {
        let reverted = info.reverted.iter().map(u64::to_string).collect::<Vec<_>>().join(",");
        writeln!(
            chain,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:?}",
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
            block.bedrock_status
        )?;
    }
    fs::write(out.join("chain.tsv"), chain)?;
    let (header, segments) = lab::deployment_accounts(DEPLOYMENT, program);
    ensure!(header == program_account, "the deployment's account");
    fs::write(
        out.join("programs.tsv"),
        format!(
            "name\theader_account\timage_id\tbinary_sha256\tbinary_bytes\tuser_elf_bytes\tsegments\n{DEPLOYMENT}\t{header}\t{}\t{}\t{}\t{}\t{}\n",
            program.image_hex(),
            program.sha256(),
            program.bytes.len(),
            program.user_elf.len(),
            segments.len()
        ),
    )?;
    let mut instances = String::from(lab::INSTANCES_HEADER);
    let mut jobs = String::from(lab::JOBS_HEADER);
    let mut failed = String::from(lab::JOBS_HEADER);
    for c in Chain::ALL {
        let Some(f) = Follow::load(&live.opts.run_dir, c, program_account)? else { continue };
        if let Some(state) = f.state(live).await? {
            let store = CompactStore::decode(&state.store).map_err(|e| anyhow!("{e:?}"))?;
            writeln!(instances, "{}", lab::instance_line(&f, &state, &store))?;
        }
        let src = live.opts.run_dir.join("net").join(c.name());
        let dst = out.join("net").join(c.name());
        fs::create_dir_all(dst.join("updates"))?;
        for name in ["bootstrap.ssz", "checkpoint"] {
            fs::copy(src.join(name), dst.join(name))?;
        }
        for entry in fs::read_dir(src.join("updates"))? {
            let entry = entry?;
            fs::copy(entry.path(), dst.join("updates").join(entry.file_name()))?;
        }
        for (file, acc) in [("jobs.tsv", &mut jobs), ("jobs-failed.tsv", &mut failed)] {
            if let Ok(text) = fs::read_to_string(src.join(file)) {
                text.lines().skip(1).for_each(|l| {
                    acc.push_str(l);
                    acc.push('\n');
                });
                fs::copy(src.join(file), dst.join(file))?;
            }
        }
    }
    fs::write(out.join("instances.tsv"), instances)?;
    fs::write(out.join("jobs.tsv"), jobs)?;
    fs::write(out.join("jobs-failed.tsv"), failed)?;
    let count = |s: &str| live.rows.iter().filter(|r| r.status == s).count();
    let summary = format!(
        "blocks: 1..={} (every one final on Bedrock)\ntransactions: {} sent; {} applied, {} refused with an exit code, {} reverted otherwise, {} dropped, {} rejected at the RPC, {} unfinished\n",
        live.follower.height(),
        live.rows.len(),
        count("applied"),
        count("refused"),
        count("reverted"),
        count("dropped"),
        count("rejected"),
        count("unfinished")
    );
    fs::write(out.join("summary.txt"), &summary)?;
    print!("{summary}");
    Ok(())
}

async fn info(args: &Args) -> Result<()> {
    let program = GuestBinary::load("program", &args.path("program-bin")?)?;
    let (live, payers) = open(args, &program).await?;
    println!("head block {}, followed to block {}", live.head().await?, live.follower.height());
    println!("program: image {}, {} bytes, sha256 {}", program.image_hex(), program.bytes.len(), program.sha256());
    for (i, payer) in payers.iter().enumerate() {
        println!("payer {i}: {payer} holds {}", live.balance(*payer).await?);
    }
    Ok(())
}

/// Offline: the record's blocks link and hash, replay through LEZ's chain_state to the
/// manifest's charges (the live driver's `verify`), and each instance, as the replay leaves it,
/// holds exactly what the native light client computes from the kept bootstrap and updates.
fn verify_lab(args: &Args) -> Result<()> {
    let dir = args.path("dir")?;
    let recorded = verify::load(&dir)?;
    println!("static: {}", verify::check_static(&recorded)?);
    let Some(binary) = args.get("binary") else {
        println!("full replay: SKIPPED (no guest binary given; the log holds no program bytecode)");
        return Ok(());
    };
    let program = GuestBinary::load("program", &PathBuf::from(binary))?;
    println!("binary: image {}, sha256 {}", program.image_hex(), program.sha256());
    let (follower, summary) = verify::check_full(&recorded, std::slice::from_ref(&program))?;
    println!("full replay: {summary}");
    let program_account = recorded.programs.iter().find(|p| p.name == DEPLOYMENT).ok_or_else(|| anyhow!("no lab program"))?.account.parse::<AccountId>().map_err(|e| anyhow!("{e:?}"))?;
    for c in Chain::ALL {
        let Some(f) = Follow::load(&dir, c, program_account)? else { continue };
        let actor = f.inst.pda().actor();
        let state = follower.state.get_account_by_id(actor.account_id).data.actor_state(actor.program_account_id).to_vec();
        ensure!(!state.is_empty(), "{}: the replay holds no instance", c.name());
        let state: InstanceState = borsh::from_slice(&state)?;
        let store = CompactStore::decode(&state.store).map_err(|e| anyhow!("{e:?}"))?;
        ensure!(store == CompactStore::of(f.native.core()), "{}: the replayed instance and the native client differ", c.name());
        let core = f.native.core();
        let pda = f.inst.header(core.finalized_header.beacon.slot).actor();
        let rec = follower.state.get_account_by_id(pda.account_id).data.actor_state(pda.program_account_id).to_vec();
        let rec: RecordState = borsh::from_slice(&rec)?;
        ensure!(rec.record == HeaderRecord::of(&core.finalized_header, core.fork, &f.inst.config.spec()).encode(), "{}: the record", c.name());
        println!(
            "{}: the replayed instance holds the native store after the bootstrap and {} kept update(s): finalized slot {} (period {}), root {}, layout {:?}, {} job(s) applied",
            c.name(),
            f.applied.len(),
            store.finalized.slot,
            f.inst.config.spec().period_at_slot(store.finalized.slot),
            hex::encode(store.finalized.hash_tree_root()),
            store.fork,
            state.applied
        );
    }
    Ok(())
}

async fn run(args: Args) -> Result<i32> {
    match args.command.as_str() {
        "session" => session(&args).await,
        "info" => info(&args).await.map(|_| 0),
        "verify" => verify_lab(&args).map(|_| 0),
        other => Err(anyhow!("unknown command {other}")),
    }
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let args = Args::parse()?;
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let code = rt.block_on(run(args))?;
    std::process::exit(code);
}
