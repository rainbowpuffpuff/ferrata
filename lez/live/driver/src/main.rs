//! Ferrata on a running LEZ sequencer.
//!
//! `live-driver <command> [--option value]...` Every transaction is built, signed and sent by
//! LEZ's wallet library to a sequencer on this machine; the result of each is read by following
//! the chain with LEZ's block application. See `../README.md`.
//!
//! Commands (in the order `../run.sh run` calls them):
//!   info            the guest binaries, the accounts of the run, the sequencer's head
//!   inbound-deploy  deploy the attestation program (attest) and the bridge (bridge, --bridge-bin)
//!   inbound         the inbound crossing (--chain stand-in | sepolia --capture DIR [--capture DIR]
//!                   [--vault ADDR]; --private real | fake | skip): ../inbound/README.md
//!   deploy          deploy the program twice (main, twin) and LEZ's scripted test program
//!   replay          instance A on the main deployment: Init and the five mainnet updates
//!   scenarios       the twin deployment: finality path and refusals
//!   capacity        three heavy transactions at once
//!   private-stored  submit lez/results/stage2-private-tx.borsh (the real-proof transaction)
//!   private-fake    the private update for the live instance with a fake receipt (RISC0_DEV_MODE=1)
//!   private-real    the same with a real proof (RISC0_DEV_MODE unset)
//!   attest-private  the reference consumer (--consumer-bin, deployed as consumer) attests privately, with a
//!                   real proof, a Transfer log of the mainnet block of --attest-fixture (default the harness's
//!                   Stage 4 fixture) on a new instance of the main deployment; --private-account N (default 1)
//!   attest-block    (LEZ v0.4.0-rc3) several private reads in one block, RFP-021 R.11: four new private accounts
//!                   each prove the consumer's AttestPrivately of another Transfer log of --attest-fixture's block
//!                   against one snapshot, held; the chain moves (a private transaction of the control, a finality
//!                   update of an instance from --other-fixture, default the mainnet end-to-end fixture); then the
//!                   four are sent back to back and must apply in one block; the control's second transaction from
//!                   the same snapshot must be refused (src/block.rs; --consumer-bin)
//!   submit-proven   submit a private update proven elsewhere (--tx FILE, from prove-from-transcript)
//!   fresh-follow, fresh-scenarios, fresh-consume, fresh-private
//!                   the run on fresh Sepolia data (--fresh-checkpoint DIR --fresh-attest DIR, fetched when the run
//!                   starts; src/fresh.rs): instance A of `main` follows Sepolia from a recent checkpoint; the twin's
//!                   refusals on that data; the attestation consumed publicly (root `Attest`, the reference consumer's
//!                   `CallAttest` and `Claim`) and privately (`AttestPrivately`, a real proof; --consumer-bin)
//!   export          write the records into --out
//! Without a sequencer:
//!   fresh-check     the fresh data natively (--fresh-checkpoint, --fresh-attest; --out FILE for its JSON): exit 3
//!                   when its finality update does not advance the store (fetch it again after the next epoch), 4
//!                   when the bootstrap or the finalized block is older than its bound (a stale endpoint)
//!   inbound-check   the inbound chain natively (--program-bin, --bridge-bin, --chain ...)
//!   inbound-plan    the inbound run's configuration identity (--attest-bin, --bridge-bin, --vault HEX)
//!   verify          re-check a recorded run in --dir, fully if --binary FILE ... are given
//!   prove-from-transcript  prove the private update from a recorded RPC transcript
//!   describe-tx     the size, receipt size and claims of proven transactions (--tx FILE, repeatable)
//! Against a running sequencer:
//!   check-rpc       the sequencer serves the chain recorded in --dir
//!
//! Options: --rpc URL (default http://127.0.0.1:3040), --run-dir DIR, --out DIR,
//! --program-bin FILE, --scripted-bin FILE, --fixtures DIR, --harness-results DIR,
//! --sequencer-log FILE, --gas-limit N (default 4000000), --stored-tx FILE, --dir DIR,
//! --binary FILE (repeatable), --transcript FILE, --tx-out FILE, --tx FILE, --consumer-bin FILE,
//! --attest-fixture DIR, --other-fixture DIR, --private-account N, --fresh-checkpoint DIR, --fresh-attest DIR,
//! --wait-final yes|no (default yes: a transaction is settled once its block is final on Bedrock;
//! no for LEZ's mock Bedrock, where no block becomes final).

mod attest;
#[cfg(feature = "lez-rc3")]
mod block;
mod blocklog;
mod follower;
mod fresh;
mod inbound;
mod keys;
mod live;
mod manifest;
mod plan;
mod private;
mod programs;
mod scenario;
mod verify;

use std::{
    collections::HashMap,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, ensure, Context, Result};
use ferrata_core::witness::CompactStore;
use ferrata_lez_wire::InstanceState;
use lee::{AccountId, PrivateKey};

use crate::{
    keys::{account_of, lez_test_payers},
    live::{Live, Options},
    plan::{load_fixtures, mainnet_config, root, Fixtures, Instance, TRUSTED_BLOCK_ROOT},
    programs::GuestBinary,
    scenario::{deployment_accounts, deployment_keys},
};

struct Args {
    command: String,
    options: HashMap<String, Vec<String>>,
}

impl Args {
    fn parse() -> Result<Self> {
        let mut args = std::env::args().skip(1);
        let command = args.next().ok_or_else(|| anyhow!("usage: live-driver <command> [--option value]... (see src/main.rs)"))?;
        let mut options: HashMap<String, Vec<String>> = HashMap::new();
        while let Some(arg) = args.next() {
            let name = arg.strip_prefix("--").ok_or_else(|| anyhow!("unexpected argument {arg}"))?.to_string();
            let value = args.next().ok_or_else(|| anyhow!("--{name} needs a value"))?;
            options.entry(name).or_default().push(value);
        }
        Ok(Self { command, options })
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.options.get(name).and_then(|v| v.last()).map(String::as_str)
    }

    fn path(&self, name: &str) -> Result<PathBuf> {
        self.get(name).map(PathBuf::from).ok_or_else(|| anyhow!("--{name} is required"))
    }

    fn all(&self, name: &str) -> Vec<PathBuf> {
        self.options.get(name).map(|v| v.iter().map(PathBuf::from).collect()).unwrap_or_default()
    }
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn fixtures(args: &Args) -> Result<Fixtures> {
    let dir = args
        .get("fixtures")
        .map(PathBuf::from)
        .unwrap_or_else(|| repository_root().join("crates/ferrata-spectests/fixtures/mainnet-p1899"));
    load_fixtures(&dir)
}

/// Everything a phase needs.
struct Run {
    live: Live,
    fixtures: Fixtures,
    program: GuestBinary,
    scripted: GuestBinary,
    /// The bridge program (`--bridge-bin`), for the inbound commands.
    bridge: Option<GuestBinary>,
    /// The reference consumer (`--consumer-bin`), for `attest-private`.
    consumer: Option<GuestBinary>,
    /// The fresh Sepolia data (`--fresh-checkpoint`, `--fresh-attest`), for the `fresh-*` phases.
    fresh: Option<fresh::Fresh>,
    payers: Vec<AccountId>,
}

impl Run {
    fn main(&self) -> AccountId {
        deployment_accounts("main", &self.program).0
    }

    fn twin(&self) -> AccountId {
        deployment_accounts("twin", &self.program).0
    }

    fn scripted_at(&self) -> AccountId {
        deployment_accounts("scripted", &self.scripted).0
    }
}

async fn open(args: &Args, phase: &str) -> Result<Run> {
    let program = GuestBinary::load("program", &args.path("program-bin")?)?;
    let scripted = GuestBinary::load("scripted", &args.path("scripted-bin")?)?;
    let payer_keys = lez_test_payers()?;
    let mut keys: Vec<PrivateKey> = payer_keys.clone();
    keys.extend(deployment_keys("main", &program));
    keys.extend(deployment_keys("twin", &program));
    keys.extend(deployment_keys("scripted", &scripted));
    let bridge = args.get("bridge-bin").map(|p| GuestBinary::load("bridge", Path::new(p))).transpose()?;
    if let Some(bridge) = &bridge {
        keys.extend(deployment_keys(inbound::ATTEST, &program));
        keys.extend(deployment_keys(inbound::BRIDGE, bridge));
    }
    let consumer = args.get("consumer-bin").map(|p| GuestBinary::load("consumer", Path::new(p))).transpose()?;
    if let Some(consumer) = &consumer {
        keys.extend(deployment_keys("consumer", consumer));
    }
    let run_dir = args.path("run-dir")?;
    let opts = Options {
        rpc: args.get("rpc").unwrap_or("http://127.0.0.1:3040").to_string(),
        out_dir: args.get("out").map(PathBuf::from).unwrap_or_else(|| run_dir.join("out")),
        run_dir,
        harness_results: args.get("harness-results").map(PathBuf::from).unwrap_or_else(|| repository_root().join("lez/results")),
        sequencer_log: args.get("sequencer-log").map(PathBuf::from),
        gas_limit: args.get("gas-limit").map(str::parse).transpose()?.unwrap_or(4_000_000),
        wait_final: args.get("wait-final") != Some("no"),
    };
    // This driver sends through LEZ's wallet directly too: a sequencer not on this machine only as `check_send` allows.
    crate::live::check_send(&opts.rpc)?;
    let fresh = fresh_data(args)?;
    let mut live = Live::open(opts, &keys).await?;
    live.phase = phase.to_string();
    Ok(Run {
        live,
        fixtures: fixtures(args)?,
        program,
        scripted,
        bridge,
        consumer,
        fresh,
        payers: payer_keys.iter().map(account_of).collect(),
    })
}

/// The fresh Sepolia data, if both directories are given.
fn fresh_data(args: &Args) -> Result<Option<fresh::Fresh>> {
    match (args.get("fresh-checkpoint"), args.get("fresh-attest")) {
        (Some(c), Some(a)) => Ok(Some(fresh::Fresh::load(Path::new(c), Path::new(a))?)),
        (None, None) => Ok(None),
        _ => bail!("--fresh-checkpoint and --fresh-attest go together"),
    }
}

async fn info(run: &Run) -> Result<()> {
    let live = &run.live;
    println!(
        "sequencer {}: head block {}, followed to block {}{}",
        live.opts.rpc,
        live.head().await?,
        live.follower.height(),
        if live.opts.wait_final { " (the last one final on Bedrock)" } else { " (no Bedrock finality: LEZ's mock)" }
    );
    let quote = {
        use wallet::WalletSequencerClient as _;
        live.wallet.client().get_fee_state().await?
    };
    println!(
        "fee state at height {}: base fee exec {} / stor {}; block caps exec {} / stor {}",
        quote.height, quote.base_fee_exec, quote.base_fee_stor, quote.max_gas_exec, quote.max_gas_stor
    );
    println!(
        "LEZ constants: wallet default gas limit {}, TARGET_GAS_EXEC {}, MAX_GAS_EXEC {}, TARGET_GAS_STOR {}, MAX_GAS_STOR {}, PRIVATE_VERIFY_GAS {}, PRIVATE_GAS_STOR {}",
        wallet::DEFAULT_GAS_LIMIT,
        fee_core::market::TARGET_GAS_EXEC,
        fee_core::market::MAX_GAS_EXEC,
        fee_core::market::TARGET_GAS_STOR,
        fee_core::market::MAX_GAS_STOR,
        fee_core::market::PRIVATE_VERIFY_GAS,
        fee_core::market::PRIVATE_GAS_STOR
    );
    for binary in [&run.program, &run.scripted] {
        println!(
            "{}: image {}, {} bytes (sha256 {}), user ELF {} bytes in {} segment(s)",
            binary.name,
            binary.image_hex(),
            binary.bytes.len(),
            binary.sha256(),
            binary.user_elf.len(),
            binary.chunks().len()
        );
    }
    println!("deployments: main {}, twin {}, scripted {}", run.main(), run.twin(), run.scripted_at());
    for (i, payer) in run.payers.iter().enumerate() {
        println!("payer {i}: {payer} holds {}", live.balance(*payer).await?);
    }
    println!("harness tables: {} rows from {}", live.harness.len(), live.opts.harness_results.display());
    Ok(())
}

/// The instances the scenarios create, by name.
fn instances(run: &Run) -> Vec<Instance> {
    let checkpoint = root(TRUSTED_BLOCK_ROOT);
    let mut other = mainnet_config(2);
    other.genesis_validators_root = [0x42; 32];
    let mut late = mainnet_config(2);
    late.genesis_time += 315_360_000;
    let mut all = vec![
        Instance::new("A (main deployment, mainnet)", run.main(), mainnet_config(2), checkpoint),
        Instance::new("B (twin deployment, mainnet)", run.twin(), mainnet_config(2), checkpoint),
        Instance::new("C (twin deployment, another chain's genesis_validators_root)", run.twin(), other, checkpoint),
        Instance::new("D (twin deployment, genesis time ten years later)", run.twin(), late, checkpoint),
    ];
    if let Some(fresh) = &run.fresh {
        for which in ["A", "B", "C", "D"] {
            all.push(fresh.instance(which, run.main(), run.twin()));
        }
    }
    if run.consumer.is_some() {
        if let Ok(fx) = attest::AttestFixture::load(&attest_fixture_dir(None)) {
            all.push(Instance::new("attest-mainnet (main deployment, the attestation fixture)", run.main(), mainnet_config(2), fx.checkpoint));
        }
    }
    all
}

fn attest_fixture_dir(given: Option<&str>) -> PathBuf {
    given
        .map(PathBuf::from)
        .unwrap_or_else(|| repository_root().join("crates/ferrata-spectests/fixtures/mainnet-attest-2026-11-10"))
}

/// Write the records of the run: the manifest, the chain (blocks without program bytecode, the
/// chunks removed, one line per block), the programs and the instances as the chain holds them.
async fn export(run: &mut Run) -> Result<()> {
    let live = &mut run.live;
    live.sync().await?;
    live.save()?;
    let out = live.opts.out_dir.clone();
    fs::create_dir_all(&out)?;

    manifest::write(&out.join("run.tsv"), &live.rows)?;

    // The blocks, with the Bedrock status the sequencer reports for each now.
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

    let mut chain = String::from(
        "block\thash\ttimestamp_ms\ttransactions\tuser_transactions\treverted\tbase_fee_exec\tbase_fee_stor\tgas_exec\tgas_stor\trevenue_base\tbedrock_status\n",
    );
    for (info, status) in live.follower.chain.iter().zip(&statuses) {
        let reverted = info.reverted.iter().map(u64::to_string).collect::<Vec<_>>().join(",");
        writeln!(
            chain,
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
    fs::write(out.join("chain.tsv"), chain)?;

    let mut programs = String::from("name\theader_account\timage_id\tbinary_sha256\tbinary_bytes\tuser_elf_bytes\tsegments\n");
    let mut deployed: Vec<(&str, &GuestBinary)> = match &run.bridge {
        Some(bridge) => vec![(inbound::ATTEST, &run.program), (inbound::BRIDGE, bridge)],
        None => vec![("main", &run.program), ("twin", &run.program), ("scripted", &run.scripted)],
    };
    if let Some(consumer) = &run.consumer {
        deployed.push(("consumer", consumer));
    }
    for (name, binary) in deployed {
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

    let mut table = String::from("instance\tprogram_account\tinstance_id\tinstance_account\tactive\tjobs_applied\tfinalized_slot\tfinalized_root\tnext_committee_known\n");
    for inst in instances(run) {
        let actor = inst.pda().actor();
        let state = run.live.actor_state(actor).await?;
        if state.is_empty() {
            continue;
        }
        let state: InstanceState = borsh::from_slice(&state)?;
        let store = CompactStore::decode(&state.store).map_err(|e| anyhow!("{}: {e:?}", inst.name))?;
        writeln!(
            table,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            inst.name,
            inst.program,
            hex::encode(inst.id),
            actor.account_id,
            state.active,
            state.applied,
            store.finalized.slot,
            hex::encode(store.finalized.hash_tree_root()),
            store.is_next_sync_committee_known()
        )?;
    }
    fs::write(out.join("instances.tsv"), table)?;

    let live = &run.live;
    let count = |status: &str| live.rows.iter().filter(|r| r.status == status).count();
    let compared: Vec<_> = live.rows.iter().filter(|r| !r.harness_exec_gas.is_empty() && !r.block.is_empty()).collect();
    let equal = compared.iter().filter(|r| r.harness_exec_gas == r.exec_gas).count();
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
    let total = |f: fn(&manifest::Row) -> &String| -> u128 { live.rows.iter().filter_map(|r| f(r).parse::<u128>().ok()).sum() };
    writeln!(
        summary,
        "charged in all: execution gas {}, storage gas {}, base fees {}",
        total(|r| &r.exec_gas),
        total(|r| &r.stor_gas),
        total(|r| &r.fee)
    )?;
    writeln!(
        summary,
        "against the harness tables: {} transactions have a harness row; execution gas equal in {}, different in {}",
        compared.len(),
        equal,
        compared.len() - equal
    )?;
    for row in compared.iter().filter(|r| r.harness_exec_gas != r.exec_gas) {
        writeln!(summary, "  differs: {} live {} harness {} ({})", row.step, row.exec_gas, row.harness_exec_gas, row.note)?;
    }
    let fees: Vec<u64> = live.follower.chain.iter().map(|b| b.base_fee_exec).collect();
    writeln!(
        summary,
        "base fee exec over the blocks: min {}, max {}; blocks finalized on Bedrock at export: {} of {}",
        fees.iter().min().unwrap_or(&0),
        fees.iter().max().unwrap_or(&0),
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
        "verify" => return verify::verify(&args.path("dir")?, &args.all("binary")),
        "inbound-check" => {
            let program = GuestBinary::load("program", &args.path("program-bin")?)?;
            let bridge = GuestBinary::load("bridge", &args.path("bridge-bin")?)?;
            let attest = deployment_accounts(inbound::ATTEST, &program).0;
            let chain = match args.get("chain").unwrap_or("stand-in") {
                "stand-in" => inbound::stand_in(attest, &bridge, &repository_root())?,
                "sepolia" => inbound::sepolia(&repository_root(), &args.all("capture"), args.get("vault"))?,
                other => bail!("--chain {other}: stand-in or sepolia"),
            };
            return inbound::check(&chain, attest, &bridge);
        }
        "inbound-plan" => {
            let ids = inbound::plan_ids(&args.path("attest-bin")?, &args.path("bridge-bin")?, args.get("vault"))?;
            println!("{}", serde_json::to_string_pretty(&ids)?);
            return Ok(());
        }
        "fresh-check" => {
            let data = fresh::Fresh::load(&args.path("fresh-checkpoint")?, &args.path("fresh-attest")?)?;
            match fresh::check(&data)? {
                Ok(summary) => {
                    let text = serde_json::to_string(&summary)?;
                    if let Some(out) = args.get("out") {
                        fs::write(out, format!("{}\n", serde_json::to_string_pretty(&summary)?))?;
                    }
                    println!("{text}");
                    return Ok(());
                }
                Err(refused) => {
                    eprintln!("fresh-check: {}", refused.message());
                    std::process::exit(refused.exit_code());
                }
            }
        }
        "describe-tx" => {
            for path in args.all("tx") {
                println!("{}\t{}", path.display(), private::describe_private(&path)?);
            }
            return Ok(());
        }
        "prove-from-transcript" => {
            let program = GuestBinary::load("program", &args.path("program-bin")?)?;
            let twin = deployment_accounts("twin", &program).0;
            let tx_out = args.path("tx-out")?;
            let scratch = tx_out.parent().map(Path::to_path_buf).unwrap_or_default().join("prove-from-transcript");
            return private::prove_from_transcript(twin, &fixtures(&args)?, &program, &args.path("transcript")?, &tx_out, &scratch).await;
        }
        _ => {}
    }
    let phase = match args.command.as_str() {
        "private-stored" | "private-fake" | "private-real" | "submit-proven" => "private",
        other => other,
    };
    let mut run = open(&args, phase).await?;
    match args.command.as_str() {
        "info" => info(&run).await?,
        "deploy" => {
            let payer = run.payers[0];
            let program = run.program.clone();
            let scripted = run.scripted.clone();
            scenario::deploy(&mut run.live, "main", &program, payer).await?;
            scenario::deploy(&mut run.live, "twin", &program, payer).await?;
            scenario::deploy(&mut run.live, "scripted", &scripted, payer).await?;
        }
        "replay" => {
            let (program, payer) = (run.main(), run.payers[0]);
            scenario::replay(&mut run.live, program, &run.fixtures, payer).await?;
        }
        "scenarios" => {
            let (main, twin, scripted, payer) = (run.main(), run.twin(), run.scripted_at(), run.payers[1]);
            scenario::scenarios(&mut run.live, main, twin, scripted, &run.fixtures, payer).await?;
        }
        "capacity" => {
            let twin = run.twin();
            let payers = run.payers[1..].to_vec();
            scenario::capacity(&mut run.live, twin, &run.fixtures, &payers).await?;
        }
        "private-stored" => private::stored(&mut run.live, &args.path("stored-tx")?).await?,
        "private-fake" | "private-real" => {
            let twin = run.twin();
            let (proof, name) =
                if args.command == "private-fake" { (private::Proof::Fake, "fake") } else { (private::Proof::Real, "real") };
            let program = run.program.clone();
            private::private_update(&mut run.live, twin, &run.fixtures, &program, proof, name).await?;
        }
        "submit-proven" => {
            let twin = run.twin();
            private::submit_proven(&mut run.live, twin, &run.fixtures, &args.path("tx")?).await?;
        }
        "attest-private" => {
            let consumer = run.consumer.clone().ok_or_else(|| anyhow!("--consumer-bin is required"))?;
            let (program, payer) = (run.program.clone(), run.payers[1]);
            let index = args.get("private-account").map(str::parse).transpose()?.unwrap_or(1);
            let dir = attest_fixture_dir(args.get("attest-fixture"));
            attest::attest_private(&mut run.live, &program, &consumer, &dir, payer, index).await?;
        }
        "fresh-follow" | "fresh-scenarios" | "fresh-consume" | "fresh-private" => {
            let data = run.fresh.take().ok_or_else(|| anyhow!("--fresh-checkpoint and --fresh-attest are required"))?;
            let (main, twin, scripted) = (run.main(), run.twin(), run.scripted_at());
            let payer = run.payers[1];
            let result = match args.command.as_str() {
                "fresh-follow" => fresh::follow(&mut run.live, main, twin, &data, payer).await,
                "fresh-scenarios" => fresh::scenarios(&mut run.live, main, twin, scripted, &data, payer).await,
                "fresh-consume" => {
                    let consumer = run.consumer.clone().ok_or_else(|| anyhow!("--consumer-bin is required"))?;
                    fresh::consume(&mut run.live, main, twin, &consumer, &data, payer).await
                }
                _ => {
                    let consumer = run.consumer.clone().ok_or_else(|| anyhow!("--consumer-bin is required"))?;
                    let program = run.program.clone();
                    let index = args.get("private-account").map(str::parse).transpose()?.unwrap_or(1);
                    fresh::private(&mut run.live, main, twin, &program, &consumer, &data, payer, index).await
                }
            };
            run.fresh = Some(data);
            result?;
        }
        #[cfg(feature = "lez-rc3")]
        "attest-block" => {
            let consumer = run.consumer.clone().ok_or_else(|| anyhow!("--consumer-bin is required"))?;
            let (program, payer) = (run.program.clone(), run.payers[1]);
            let funder_key = lez_test_payers()?[0].clone();
            let dir = attest_fixture_dir(args.get("attest-fixture"));
            let other = args.get("other-fixture").map(PathBuf::from).unwrap_or_else(|| {
                repository_root().join("crates/ferrata-spectests/fixtures/mainnet-e2e-2026-11-09")
            });
            block::attest_block(&mut run.live, &program, &consumer, &dir, &other, payer, &funder_key).await?;
        }
        #[cfg(not(feature = "lez-rc3"))]
        "attest-block" => bail!("attest-block needs LEZ v0.4.0-rc3 (lez/scripts/lez-select.sh v0.4.0-rc3)"),
        "inbound-deploy" => {
            let bridge = run.bridge.clone().ok_or_else(|| anyhow!("--bridge-bin is required"))?;
            let (program, payer) = (run.program.clone(), run.payers[1]);
            scenario::deploy(&mut run.live, inbound::ATTEST, &program, payer).await?;
            scenario::deploy(&mut run.live, inbound::BRIDGE, &bridge, payer).await?;
        }
        "inbound" => {
            let bridge = run.bridge.clone().ok_or_else(|| anyhow!("--bridge-bin is required"))?;
            let attest = deployment_accounts(inbound::ATTEST, &run.program).0;
            let chain = match args.get("chain").unwrap_or("stand-in") {
                "stand-in" => inbound::stand_in(attest, &bridge, &repository_root())?,
                "sepolia" => inbound::sepolia(&repository_root(), &args.all("capture"), args.get("vault"))?,
                other => bail!("--chain {other}: stand-in or sepolia"),
            };
            let private = args.get("private").unwrap_or("real").to_string();
            let payer = run.payers[1];
            inbound::session(&mut run.live, attest, &bridge, &chain, payer, &private).await?;
        }
        "export" => export(&mut run).await?,
        "check-rpc" => verify::check_rpc(&args.path("dir")?, &run.live.wallet).await?,
        other => bail!("unknown command {other}"),
    }
    run.live.save().context("saving the run's state")
}

fn main() -> Result<()> {
    // LEZ's wallet otherwise prints every included transaction in full.
    std::env::set_var(wallet::SUPPRESS_VERBOSE_PRINTS, "1");
    let _ = env_logger::try_init();
    let args = Args::parse()?;
    tokio::runtime::Builder::new_multi_thread().enable_all().build()?.block_on(run(args))
}
