//! `ferrata-v03`: deploys and drives Ferrata's LEZ v0.3.1 light-client program on a LEZ v0.3.1
//! sequencer with LEZ v0.3.1's wallet (its key storage, signing and sequencer client).
//! EXPERIMENTAL, UNAUDITED (`../README.md`).
//!
//! ```text
//! ferrata-v03 new-wallet RUN URL      a fresh wallet at $LEE_WALLET_HOME_DIR pointed at URL: the payer, the
//!                                     program's header account and its segment accounts
//! ferrata-v03 queue RUN [--dry-run]   sign every step RUN's data allows that is not signed yet, simulate the whole
//!                                     signed sequence on LEZ v0.3.1's state machine here, then queue the new ones
//! ferrata-v03 resend RUN              send again, byte for byte, every signed transaction the chain does not hold
//! ferrata-v03 wait RUN SECONDS        wait until every signed transaction is in a block (resending after a rewind);
//!                                     write RUN/txs.tsv with each one's block and its verdict, read from state
//! ferrata-v03 status RUN              each signed transaction's block and the block's Bedrock status
//! ferrata-v03 finalized RUN           the finalized slot after every signed update, and the execution block its
//!                                     header binds (for the proofs of the attestation)
//! ferrata-v03 record RUN              read every verdict back, write RUN/record.json
//! ferrata-v03 verify RUN              read every verdict back, compare with RUN/record.json (exit 0 iff all match)
//! ```
//!
//! **Why queued.** On the public testnet blocks come hours apart and a block can be rewound
//! (`spike/tmp/testnet-ops/FOR-SIBLINGS.md`). So every transaction is built and signed up front with
//! explicit nonces, declares its measured execution gas plus a small margin (a block admits a
//! transaction only if the gas charged so far plus its *declared* limit fits the 10M cap), and is
//! queued at once; LEZ's builder then makes three blocks of the sequence: the deployment and `Init`;
//! the first finality update; the second and the attestation. The signed bytes are kept
//! (`RUN/signed/`): after a rewind the only safe recovery is to send the same bytes again (a
//! duplicate is dropped at the nonce check); nothing is ever rebuilt once signed, since a rebuilt
//! deployment to existing segments is included as a panic and charged in full, and a rebuilt call
//! executes twice.
//!
//! RUN holds the data `scripts/fetch.py` fetched (`chain.json`, `bootstrap.ssz`, `updates/`,
//! `proofs/<slot>/`) and what this program writes: `state.json` (payer, program, deployment
//! accounts), `nonces.json`, `applied.txt` (the updates signed, in order: the native light client
//! is rebuilt from them), `signed.tsv` and `signed/` (every signed transaction, its simulated block
//! and gas), `batch.json` (the same sequence in testnet-ops's `ferrata_ops batch` format),
//! `ferrata_v03.bin`, `txs.tsv` (the chain's blocks and verdicts) and `record.json`.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, bail, Context, Result};
use common::{transaction::LeeTransaction, HashType};
use ferrata_core::{
    fork::LcFork,
    types::{LightClientBootstrap, LightClientUpdate},
    witness::{CompactStore, ExecutionSummary, HeaderRecord},
};
use ferrata_v03_wire::{InstanceState, RecordState, Statement};
use harness_v03::*;
use lee::{
    public_transaction::{Message, WitnessSet},
    AccountId, FeeDeclaration, PublicTransaction,
};
use lee_core::{
    account::{Account, Nonce, ProgramShardSelector},
    program::{ProgramHeader, PROGRAM_LOADER_ACCOUNT_ID},
};
use sequencer_service_rpc::RpcClient as _;
use serde_json::{json, Value};
use wallet::WalletCore;

/// What a loader transaction declares (it is native: it charges no execution gas).
const LOADER_GAS: u64 = 100_000;
/// Margin on a program transaction's simulated execution gas: 3% and 30,000.
fn declared(cycles: u64) -> u64 {
    let g = cycles + cycles / 33 + 30_000;
    (g.div_ceil(10_000) * 10_000).min(MAX_GAS_EXEC)
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["new-wallet", run, url] => new_wallet(Path::new(run), url),
        ["queue", run] => queue(Path::new(run), false).await,
        ["queue", run, "--dry-run"] => queue(Path::new(run), true).await,
        ["resend", run] => resend(Path::new(run)).await.map(|_| ()),
        ["wait", run, secs] => wait(Path::new(run), secs.parse().unwrap_or(600)).await,
        ["finalized", run] => finalized(Path::new(run)).await,
        ["goal", run] => match goal(Path::new(run)).await {
            Ok(true) => Ok(()),
            Ok(false) => std::process::exit(4),
            Err(e) => Err(e),
        },
        ["status", run] => status(Path::new(run)).await,
        ["record", run] => record(Path::new(run)).await.map(|_| ()),
        ["verify", run] => verify(Path::new(run)).await,
        _ => {
            eprintln!("usage: see the documentation at the top of lez-v03/client/src/main.rs");
            std::process::exit(2)
        }
    };
    if let Err(e) = result {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

fn utc(ms: u64) -> String {
    let s = ms / 1000;
    let (days, rem) = ((s / 86_400) as i64, s % 86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

// ---------------------------------------------------------------------------------------------
// The run directory
// ---------------------------------------------------------------------------------------------

fn read_json(path: &Path) -> Result<Value> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("{}", path.display()))?)
        .with_context(|| format!("{} is not JSON", path.display()))
}

fn write_json(path: &Path, v: &Value) -> Result<()> {
    fs::write(path, serde_json::to_string_pretty(v)? + "\n").with_context(|| format!("{}", path.display()))
}

fn read_or_empty(path: &Path) -> Result<Value> {
    if path.exists() {
        read_json(path)
    } else {
        Ok(json!({}))
    }
}

fn account(v: &Value) -> Result<AccountId> {
    v.as_str().ok_or_else(|| anyhow!("not an account id: {v}"))?.parse().map_err(|e| anyhow!("account id {v}: {e:?}"))
}

/// Sepolia's configuration (`harness_v03::sepolia_config`), the store in the bootstrap's layout,
/// the committee adopted in one part (so `Init` is one transaction).
fn config(run: &Path) -> Result<ferrata_v03_wire::Config> {
    let chain = read_json(&run.join("chain.json"))?;
    if chain["chain"] != "sepolia" {
        bail!("only Sepolia is configured: {}", chain["chain"]);
    }
    let mut c = sepolia_config(1);
    c.store_fork = match chain["layout"].as_str() {
        Some("electra") => 3,
        Some("gloas") => 4,
        other => bail!("unknown bootstrap layout {other:?}"),
    };
    Ok(c)
}

fn bootstrap(run: &Path, cfg: &ferrata_v03_wire::Config) -> Result<LightClientBootstrap> {
    LightClientBootstrap::decode(&fs::read(run.join("bootstrap.ssz"))?, cfg.fork().unwrap(), 512)
        .map_err(|e| anyhow!("bootstrap.ssz: {e:?}"))
}

fn checkpoint(run: &Path) -> Result<ferrata_core::Root> {
    let chain = read_json(&run.join("chain.json"))?;
    Ok(root(chain["checkpoint"].as_str().ok_or_else(|| anyhow!("chain.json: no checkpoint"))?))
}

/// `*-finality-*.ssz` (a Gloas `LightClientFinalityUpdate`) or `*-period-*.ssz` (a Gloas `LightClientUpdate`).
fn load_update(file: &Path) -> Result<LightClientUpdate> {
    let name = file.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let b = fs::read(file).with_context(|| format!("{}", file.display()))?;
    if name.contains("-finality-") {
        Ok(gloas_finality_update(&b))
    } else if name.contains("-period-") {
        LightClientUpdate::decode(&b, LcFork::Gloas, 512).map_err(|e| anyhow!("{name}: {e:?}"))
    } else {
        bail!("{name}: neither a finality nor a period update file")
    }
}

fn applied(run: &Path) -> Vec<String> {
    fs::read_to_string(run.join("applied.txt"))
        .map(|s| s.lines().filter(|l| !l.is_empty()).map(str::to_owned).collect())
        .unwrap_or_default()
}

fn program_of(run: &Path) -> Result<AccountId> {
    account(&read_or_empty(&run.join("state.json"))?["program"]).context("state.json names no program: new-wallet first")
}

/// The instance and the native light client after every signed update.
fn instance(run: &Path, program: AccountId) -> Result<Instance> {
    let cfg = config(run)?;
    let boot = bootstrap(run, &cfg)?;
    let mut inst = Instance::new(program, cfg, checkpoint(run)?, &boot);
    for name in applied(run) {
        inst.follow(&load_update(&run.join("updates").join(&name))?);
    }
    Ok(inst)
}

/// One signed transaction of the run (`signed.tsv`).
#[derive(Clone, Debug)]
struct Signed {
    seq: usize,
    label: String,
    kind: String,
    hash: String,
    payer_nonce: u128,
    bytes: usize,
    gas_limit: u64,
    sim_block: u64,
    sim_exec: u64,
    sim_fee: u128,
    sim_status: String,
    tx: PublicTransaction,
}

const SIGNED_HEADER: &str = "seq\tlabel\tkind\ttx_hash\tpayer_nonce\tbytes\tgas_limit\tsim_block\tsim_exec_gas\tsim_fee\tsim_status\tfile";

fn load_signed(run: &Path) -> Result<Vec<Signed>> {
    let Ok(text) = fs::read_to_string(run.join("signed.tsv")) else { return Ok(vec![]) };
    let mut out = vec![];
    for line in text.lines().skip(1).filter(|l| !l.is_empty()) {
        let f: Vec<&str> = line.split('\t').collect();
        let bytes = hex::decode(fs::read_to_string(run.join(f[11]))?.trim())?;
        let LeeTransaction::Public(tx) = borsh::from_slice::<LeeTransaction>(&bytes)? else { bail!("{}: not public", f[11]) };
        out.push(Signed {
            seq: f[0].parse()?,
            label: f[1].into(),
            kind: f[2].into(),
            hash: f[3].into(),
            payer_nonce: f[4].parse()?,
            bytes: f[5].parse()?,
            gas_limit: f[6].parse()?,
            sim_block: f[7].parse()?,
            sim_exec: f[8].parse()?,
            sim_fee: f[9].parse()?,
            sim_status: f[10].into(),
            tx,
        });
    }
    Ok(out)
}

fn save_signed(run: &Path, all: &[Signed]) -> Result<()> {
    fs::create_dir_all(run.join("signed"))?;
    let mut text = String::from(SIGNED_HEADER);
    text.push('\n');
    for s in all {
        let file = format!("signed/{:03}.hex", s.seq);
        let wire = borsh::to_vec(&LeeTransaction::Public(s.tx.clone()))?;
        fs::write(run.join(&file), hex::encode(&wire) + "\n")?;
        text.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            s.seq, s.label, s.kind, s.hash, s.payer_nonce, s.bytes, s.gas_limit, s.sim_block, s.sim_exec, s.sim_fee, s.sim_status, file
        ));
    }
    fs::write(run.join("signed.tsv"), text)?;
    Ok(())
}

fn tx_hash(tx: &PublicTransaction) -> String {
    LeeTransaction::Public(tx.clone()).hash().to_string()
}

// ---------------------------------------------------------------------------------------------
// The wallet
// ---------------------------------------------------------------------------------------------

fn wallet_home() -> Result<PathBuf> {
    std::env::var_os(wallet::HOME_DIR_ENV_VAR)
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("set {} to the wallet's directory", wallet::HOME_DIR_ENV_VAR))
}

fn segment_count() -> Result<usize> {
    let binary = risc0_binfmt::ProgramBinary::decode(FERRATA_V03_ELF).map_err(|e| anyhow!("{e}"))?;
    Ok(binary.user_elf.len().div_ceil(program_loader_core::MAX_SEGMENT_DATA_LEN))
}

fn new_wallet(run: &Path, url: &str) -> Result<()> {
    let home = wallet_home()?;
    if home.join("storage.json").exists() {
        bail!("{} already holds a wallet", home.display());
    }
    fs::create_dir_all(&home)?;
    fs::create_dir_all(run)?;
    write_json(
        &home.join("wallet_config.json"),
        &json!({"sequencer": {"sequencer_addr": url}, "seq_poll_timeout": "30s", "seq_tx_poll_max_blocks": 120,
                "seq_poll_max_retries": 20, "seq_block_poll_max_amount": 100, "gas_limit": 2_000_000}),
    )?;
    let mut seed = [0u8; 32];
    use std::io::Read;
    fs::File::open("/dev/urandom")?.read_exact(&mut seed)?;
    let password = hex::encode(seed);
    fs::write(home.join("password"), &password)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&home, fs::Permissions::from_mode(0o700))?;
        fs::set_permissions(home.join("password"), fs::Permissions::from_mode(0o600))?;
    }
    let (mut w, _mnemonic) =
        WalletCore::new_init_storage(home.join("wallet_config.json"), home.join("storage.json"), None, &password)?;
    let payer = w.create_new_account_public(None).0;
    // The program's account is chosen before it is deployed, so every instruction can be built up front.
    let header = w.create_new_account_public(None).0;
    let segments: Vec<AccountId> = (0..segment_count()?).map(|_| w.create_new_account_public(None).0).collect();
    w.store_persistent_data()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(home.join("storage.json"), fs::Permissions::from_mode(0o600))?;
    }
    let mut s = read_or_empty(&run.join("state.json"))?;
    s["payer"] = json!(payer.to_string());
    s["program"] = json!(header.to_string());
    s["segments"] = json!(segments.iter().map(|a| a.to_string()).collect::<Vec<_>>());
    s["sequencer"] = json!(url);
    s["image_id"] = json!(image_id_hex(FERRATA_V03_ID));
    s["elf_bytes"] = json!(FERRATA_V03_ELF.len());
    write_json(&run.join("state.json"), &s)?;
    println!("{payer}");
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Building and signing the sequence
// ---------------------------------------------------------------------------------------------

/// A step to sign: its instruction, accounts and extra signers (besides the payer).
struct Step {
    label: String,
    kind: String,
    program: AccountId,
    accounts: Vec<AccountId>,
    data: Vec<u8>,
    signers: Vec<AccountId>,
    /// `true` for the program loader's transactions (no execution gas).
    loader: bool,
    /// What testnet-ops's batch file says for it.
    batch: Value,
}

/// The instance with the native light client at the checkpoint (`plan_from_state` moves it to
/// the chain's store).
fn base_instance(run: &Path, program: AccountId) -> Result<Instance> {
    let cfg = config(run)?;
    let boot = bootstrap(run, &cfg)?;
    Ok(Instance::new(program, cfg, checkpoint(run)?, &boot))
}

/// The fetched updates, in order.
fn updates(run: &Path) -> Result<Vec<(String, LightClientUpdate)>> {
    let mut names: Vec<String> = fs::read_dir(run.join("updates"))
        .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    names.sort();
    names.into_iter().map(|n| Ok((n.clone(), load_update(&run.join("updates").join(&n))?))).collect()
}

/// Every full committee the run holds: the bootstrap's and the updates' next committees.
fn known(boot: &LightClientBootstrap, ups: &[(String, LightClientUpdate)]) -> Vec<ferrata_core::types::SyncCommittee> {
    let mut k = vec![boot.current_sync_committee.clone()];
    for (_, u) in ups {
        if u.next_sync_committee.pubkeys.iter().any(|p| *p != [0u8; 48]) {
            k.push(u.next_sync_committee.clone());
        }
    }
    k
}

/// The proofs fetched: (beacon slot of the record, query).
fn proofs(run: &Path, chain_id: u64) -> Vec<(u64, ferrata_v03_wire::StorageQuery)> {
    let mut out = vec![];
    let mut dirs: Vec<PathBuf> = fs::read_dir(run.join("proofs")).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    dirs.sort();
    for dir in dirs {
        let (Ok(proof), Ok(rlp)) = (fs::read(dir.join("proof.json")), fs::read(dir.join("header.rlp"))) else { continue };
        let Some(slot) = dir.file_name().and_then(|n| n.to_str()).and_then(|n| n.parse::<u64>().ok()) else { continue };
        out.push((slot, storage_query(&proof, chain_id, Some(rlp), &[])));
    }
    out
}

/// The record `header(id, slot)` holds or will hold: the chain's, else the one an update of the
/// run finalizing `slot` writes.
fn record_at(inst: &Instance, view: &MapView, ups: &[(String, LightClientUpdate)], slot: u64) -> Option<RecordState> {
    let shard = view.shard_of(inst.header_account(slot), inst.program);
    if !shard.is_empty() {
        return borsh::from_slice(&shard).ok();
    }
    let (_, u) = ups.iter().find(|(_, u)| u.finalized_header.beacon.slot == slot)?;
    let layout = inst.layout_for(inst.config.fork().unwrap(), u);
    let record = HeaderRecord::of(&u.finalized_header, layout, &inst.spec);
    Some(RecordState { instance_id: inst.id, chain_id: inst.config.chain_id, record: record.encode() })
}

/// Every account a run reads before it signs: the deployment, the instance, the records of the
/// checkpoint and of every fetched update, and the statements of every fetched proof.
fn watched(run: &Path) -> Result<Vec<AccountId>> {
    let s = read_json(&run.join("state.json"))?;
    let program = account(&s["program"])?;
    let mut ids = vec![program];
    ids.extend(s["segments"].as_array().unwrap().iter().map(account).collect::<Result<Vec<_>>>()?);
    let inst = base_instance(run, program)?;
    ids.push(inst.account());
    let ups = updates(run)?;
    let mut slots = vec![read_json(&run.join("chain.json"))?["bootstrap_slot"].as_u64().unwrap_or(0)];
    slots.extend(ups.iter().map(|(_, u)| u.finalized_header.beacon.slot));
    let empty = MapView::default();
    for (slot, query) in proofs(run, inst.config.chain_id) {
        slots.push(slot);
        if let Some(rec) = record_at(&inst, &empty, &ups, slot) {
            for st in inst.statements(&rec, &query) {
                ids.push(ferrata_v03_wire::statement_account(&program, &inst.id, &st.id_at_block));
            }
        }
    }
    slots.sort();
    slots.dedup();
    ids.extend(slots.into_iter().map(|slot| inst.header_account(slot)));
    Ok(ids)
}

/// The watched accounts as the chain holds them: their shards, and the accounts themselves (to
/// seed the simulation's replica).
async fn read_state(client: &sequencer_service_rpc::SequencerClient, ids: &[AccountId]) -> Result<(MapView, Vec<(AccountId, Account)>)> {
    let mut view = MapView::default();
    let mut accounts = vec![];
    for id in ids {
        let a = client.get_account(*id).await.map_err(|e| anyhow!("getAccount {id}: {e}"))?;
        for (program, data) in &a.data.shards {
            view.0.insert((*id, *program), data.to_vec());
        }
        if !a.data.shards.is_empty() || a.nonce.0 > 0 {
            accounts.push((*id, a));
        }
    }
    Ok((view, accounts))
}

/// Every step the state calls for (see `harness_v03::Instance::plan_from_state`), the attestation
/// last; `Err("TAKEN: …")` if the deployment's accounts were written by someone else.
fn plan_steps(run: &Path, view: &MapView, notes: &mut Vec<String>) -> Result<Vec<Step>> {
    let s = read_json(&run.join("state.json"))?;
    let program = account(&s["program"])?;
    let segments: Vec<AccountId> = s["segments"].as_array().unwrap().iter().map(account).collect::<Result<_>>()?;
    let mut steps = vec![];
    let call = |label: &str, kind: &str, p: &Planned| -> Result<Step> {
        let data = borsh::to_vec(&p.instruction)?;
        Ok(Step {
            label: label.into(),
            kind: kind.into(),
            program,
            accounts: p.accounts.clone(),
            batch: json!({"call": {"label": label, "program": program.to_string(),
                "shards": p.accounts.iter().map(|a| json!([a.to_string(), program.to_string()])).collect::<Vec<_>>(),
                "signers": [], "data_hex": hex::encode(&data)}}),
            data,
            signers: vec![],
            loader: false,
        })
    };
    match deployment(view, program, &segments) {
        Deployment::Ours => notes.push(format!("the program is at {program} (image {})", image_id_hex(FERRATA_V03_ID))),
        Deployment::Taken(why) => bail!("TAKEN: {why}"),
        Deployment::Free { written } => {
            let binary = risc0_binfmt::ProgramBinary::decode(FERRATA_V03_ELF).map_err(|e| anyhow!("{e}"))?;
            let chunks: Vec<&[u8]> = binary.user_elf.chunks(program_loader_core::MAX_SEGMENT_DATA_LEN).collect();
            for i in (0..chunks.len()).rev() {
                if written[i] {
                    notes.push(format!("segment {} already holds its chunk", i + 1));
                    continue;
                }
                let next = segments.get(i + 1).copied();
                let mut accounts = vec![segments[i]];
                accounts.extend(next);
                let data = borsh::to_vec(&program_loader_core::Instruction::WriteSegment { bytecode: chunks[i].to_vec(), next_segment: next })?;
                steps.push(Step {
                    label: format!("deploy: WriteSegment {} of {} ({} bytes)", i + 1, chunks.len(), chunks[i].len()),
                    kind: format!("segment:{i}"),
                    program: PROGRAM_LOADER_ACCOUNT_ID,
                    accounts,
                    data,
                    signers: vec![],
                    loader: true,
                    batch: Value::Null,
                });
            }
            let mut accounts = vec![program];
            accounts.extend(&segments);
            let data = borsh::to_vec(&program_loader_core::Instruction::CreateHeader { first_segment: segments[0], immutable: true })?;
            steps.push(Step {
                label: "deploy: CreateHeader (immutable)".into(),
                kind: "header".into(),
                program: PROGRAM_LOADER_ACCOUNT_ID,
                accounts,
                data,
                signers: vec![program],
                loader: true,
                batch: json!({"deploy": {"elf": run.join("ferrata_v03.bin").display().to_string(), "header": program.to_string(),
                    "segments": segments.iter().map(|a| a.to_string()).collect::<Vec<_>>(), "immutable": true}}),
            });
        }
    }
    let mut inst = base_instance(run, program)?;
    let boot = bootstrap(run, &inst.config)?;
    let ups = updates(run)?;
    let plan = inst.plan_from_state(view, &boot, &ups, &known(&boot, &ups), notes).map_err(|e| anyhow!("{e}"))?;
    for p in &plan {
        steps.push(call(&p.planned.label, &p.kind, &p.planned)?);
    }
    // The attestation, under the record of the finalized header the instance will hold.
    let slot = inst.native.core().finalized_header.beacon.slot;
    match (record_at(&inst, view, &ups, slot), proofs(run, inst.config.chain_id).into_iter().find(|(s, _)| *s == slot)) {
        (Some(rec), Some((_, query))) => {
            let statements = inst.statements(&rec, &query);
            let present = statements.iter().all(|st| {
                let got = view.shard_of(ferrata_v03_wire::statement_account(&program, &inst.id, &st.id_at_block), program);
                borsh::from_slice::<Statement>(&got).ok().as_ref() == Some(st)
            });
            if present {
                notes.push(format!("the statements under slot {slot} are on chain"));
            } else {
                let p = inst.attest("attest", slot, query.clone(), statements.clone(), 0);
                steps.push(call(&format!("Attest {} slot(s) of 0x{} under slot {slot}", statements.len(), hex::encode(query.address)), &format!("attest:{slot}"), &p)?);
            }
        }
        (Some(rec), None) => {
            let hash = match HeaderRecord::decode(&rec.record).map(|r| r.execution) {
                Ok(ExecutionSummary::BlockHash(h)) | Ok(ExecutionSummary::Payload { block_hash: h, .. }) => h,
                _ => bail!("the record of slot {slot} binds no execution block"),
            };
            notes.push(format!("NEED-PROOF {slot} 0x{}", hex::encode(hash)));
        }
        (None, _) => notes.push(format!("no record known for slot {slot} yet")),
    }
    let mut a = applied(run);
    for p in &plan {
        if let Some(name) = p.kind.strip_prefix("update:") {
            if !a.contains(&name.to_owned()) {
                a.push(name.to_owned());
            }
        }
    }
    fs::write(run.join("applied.txt.next"), a.join("\n") + "\n")?;
    Ok(steps)
}

/// Keys the run made for replacement header accounts (beside the wallet, never inside it).
fn header_keys_path() -> Result<PathBuf> {
    Ok(wallet_home()?.parent().ok_or_else(|| anyhow!("the wallet home has no parent"))?.join("header-keys.json"))
}

fn random32() -> Result<[u8; 32]> {
    let mut b = [0u8; 32];
    use std::io::Read;
    fs::File::open("/dev/urandom")?.read_exact(&mut b)?;
    Ok(b)
}

/// The deployment's accounts were written by someone else: fresh ones (a fresh header key, kept in
/// `header-keys.json` beside the wallet; segment accounts need no key). Recorded in `state.json`.
fn replace_accounts(run: &Path, why: &str) -> Result<()> {
    let path = header_keys_path()?;
    let mut keys = read_or_empty(&path)?;
    let key = loop {
        if let Ok(k) = lee::PrivateKey::try_new(random32()?) {
            break k;
        }
    };
    let header = account_of(&key);
    keys[header.to_string()] = json!(hex::encode(key.value()));
    write_json(&path, &keys)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    let mut s = read_json(&run.join("state.json"))?;
    let n = s["segments"].as_array().map(|a| a.len()).unwrap_or(0);
    let segments: Vec<String> = (0..n).map(|_| random32().map(|b| AccountId::new(b).to_string())).collect::<Result<_>>()?;
    let mut replaced = s["replaced"].as_array().cloned().unwrap_or_default();
    replaced.push(json!({"program": s["program"], "segments": s["segments"], "reason": why, "utc": utc(now_ms())}));
    s["replaced"] = json!(replaced);
    s["program"] = json!(header.to_string());
    s["segments"] = json!(segments);
    write_json(&run.join("state.json"), &s)?;
    println!("queue: {why}: the program will be deployed at fresh accounts, header {header}");
    Ok(())
}

fn signing_key(w: &WalletCore, id: AccountId) -> Result<lee::PrivateKey> {
    if let Some(k) = w.get_account_public_signing_key(id) {
        return Ok(k.clone());
    }
    let keys = read_or_empty(&header_keys_path()?)?;
    let hexkey = keys[id.to_string()].as_str().ok_or_else(|| anyhow!("no key of {id} in the wallet or header-keys.json"))?;
    let b: [u8; 32] = hex::decode(hexkey)?.try_into().map_err(|_| anyhow!("bad key length"))?;
    lee::PrivateKey::try_new(b).map_err(|e| anyhow!("{e:?}"))
}

fn sign(w: &WalletCore, payer: AccountId, step: &Step, nonces: &mut BTreeMap<String, u128>, gas_limit: u64) -> Result<PublicTransaction> {
    let mut keys = vec![];
    let mut ns = vec![];
    for id in step.signers.iter().chain(std::iter::once(&payer)) {
        keys.push(signing_key(w, *id)?);
        let n = nonces.entry(id.to_string()).or_insert(0);
        ns.push(Nonce(*n));
        *n += 1;
    }
    let selectors = step.accounts.iter().map(|a| ProgramShardSelector::new(*a, step.program)).collect();
    let fee = FeeDeclaration::new(payer, gas_limit, 0, wallet::max_fee_for(gas_limit));
    let message = Message::new_preserialized(step.program, selectors, ns, step.data.clone(), Some(fee));
    let refs: Vec<&lee::PrivateKey> = keys.iter().collect();
    let witness = WitnessSet::for_message(&message, &refs);
    Ok(PublicTransaction::new(message, witness))
}

/// The signed transactions on a replica of the chain seeded with the accounts read from it, as
/// LEZ v0.3.1's builder groups them.
fn simulate(payer: AccountId, balance: u128, start: &BTreeMap<String, u128>, seed: &[(AccountId, Account)], txs: &[(String, PublicTransaction, u64)]) -> Vec<TxReport> {
    let mut extra: Vec<(AccountId, Account)> = seed.to_vec();
    for (id, n) in start {
        let a: AccountId = id.parse().unwrap();
        let mut acc = if a == payer { Account::funded(balance) } else { extra.iter().find(|(x, _)| *x == a).map(|(_, acc)| acc.clone()).unwrap_or_default() };
        acc.nonce = Nonce(*n);
        extra.retain(|(x, _)| *x != a);
        extra.push((a, acc));
    }
    let mut chain = Chain::with_accounts(extra);
    chain.settle_queued(txs, now_ms())
}

async fn queue(run: &Path, dry_run: bool) -> Result<()> {
    let w = WalletCore::from_env()?;
    let client = w.client_owned();
    let payer = account(&read_json(&run.join("state.json"))?["payer"])?;
    fs::write(run.join("ferrata_v03.bin"), FERRATA_V03_ELF)?;
    let mut signed = load_signed(run)?;
    let chain_nonce = client.get_accounts_nonces(vec![payer]).await.map(|v| v[0].0).unwrap_or(0);
    // Earlier signed transactions not in a block yet: send the same bytes again; sign nothing new
    // until they are resolved (a new transaction could take their nonce, or repeat their call).
    let pending: Vec<&Signed> = signed.iter().filter(|x| x.payer_nonce >= chain_nonce).collect();
    if !pending.is_empty() && !dry_run {
        for x in &pending {
            client.send_transaction(LeeTransaction::Public(x.tx.clone())).await.map_err(|e| anyhow!("sendTransaction ({}): {e}", x.label))?;
        }
        println!("queue: {} signed transaction(s) not in a block yet: sent again, byte for byte; nothing new signed", pending.len());
        return Ok(());
    }
    // Read the chain, then sign what it is missing.
    let mut notes = vec![];
    let (mut view, mut seed) = read_state(&client, &watched(run)?).await?;
    let steps = match plan_steps(run, &view, &mut notes) {
        Err(e) if e.to_string().starts_with("TAKEN: ") => {
            replace_accounts(run, e.to_string().trim_start_matches("TAKEN: "))?;
            (view, seed) = read_state(&client, &watched(run)?).await?;
            notes.clear();
            plan_steps(run, &view, &mut notes)?
        }
        other => other?,
    };
    let s = read_json(&run.join("state.json"))?;
    let program = account(&s["program"])?;
    for n in &notes {
        println!("queue: {n}");
    }
    let mut log = fs::read_to_string(run.join("notes.txt")).unwrap_or_default();
    for n in &notes {
        log.push_str(&format!("{} {n}\n", utc(now_ms())));
    }
    fs::write(run.join("notes.txt"), log)?;
    if steps.is_empty() {
        let _ = fs::remove_file(run.join("applied.txt.next"));
        println!("queue: the chain holds everything the data allows: nothing to sign");
        return Ok(());
    }
    let mut start: BTreeMap<String, u128> = BTreeMap::new();
    start.insert(payer.to_string(), chain_nonce);
    if steps.iter().any(|s| s.signers.contains(&program)) {
        start.insert(program.to_string(), client.get_accounts_nonces(vec![program]).await.map(|v| v[0].0).unwrap_or(0));
    }
    let balance = client.get_account_balance(payer).await.unwrap_or(0);
    let sim_balance = if balance > 0 { balance } else { 1_000_000_000_000 };
    // Pass 1: every step at the cap, to measure it; pass 2: the declared limits, grouped as the builder will.
    let mut n1 = start.clone();
    let mut probe = vec![];
    for st in &steps {
        let limit = if st.loader { LOADER_GAS } else { MAX_GAS_EXEC };
        probe.push((st.label.clone(), sign(&w, payer, st, &mut n1, limit)?, limit));
    }
    let measured = simulate(payer, sim_balance, &start, &seed, &probe);
    let mut n2 = start.clone();
    let mut fresh = vec![];
    for (k, st) in steps.iter().enumerate() {
        let m = &measured[k];
        if m.status != "applied" {
            bail!("{} would not apply in the simulation: {} {:?}", st.label, m.status, m.exit);
        }
        let limit = if st.loader { LOADER_GAS } else { declared(m.exec_gas) };
        fresh.push((st.label.clone(), sign(&w, payer, st, &mut n2, limit)?, limit));
    }
    let sim = simulate(payer, sim_balance, &start, &seed, &fresh);
    let base = signed.len();
    for (k, (st, (label, tx, limit))) in steps.iter().zip(&fresh).enumerate() {
        let r = &sim[k];
        if r.status != "applied" {
            bail!("{label} would not apply as queued: {} {:?}", r.status, r.exit);
        }
        signed.push(Signed {
            seq: base + k,
            label: label.clone(),
            kind: st.kind.clone(),
            hash: tx_hash(tx),
            payer_nonce: tx.message().nonces.last().unwrap().0,
            bytes: r.stor_gas as usize,
            gas_limit: *limit,
            sim_block: r.block,
            sim_exec: r.exec_gas,
            sim_fee: r.fee,
            sim_status: r.status.clone(),
            tx: tx.clone(),
        });
    }
    let first = sim.iter().map(|r| r.block).filter(|b| *b > 0).min().unwrap_or(0);
    println!("seq\tsim. block\tlabel\tbytes\tdeclared gas\texec gas (simulated)\tof 10M\tfee (simulated, base fee)\ttx hash");
    for (k, x) in signed[base..].iter().enumerate() {
        let r = &sim[k];
        println!("{}\t{}\t{}\t{}\t{}\t{}\t{:.1}%\t{} ({})\t{}", x.seq, r.block.saturating_sub(first) + 1, x.label, x.bytes, x.gas_limit, x.sim_exec, x.sim_exec as f64 / 1e5, x.sim_fee, r.base_fee_exec, x.hash);
    }
    let total: u128 = sim.iter().map(|r| r.fee).sum();
    let reserve = signed[base..].iter().map(|x| u128::from(x.gas_limit) * 8 + x.bytes as u128 * 8).max().unwrap_or(0);
    let blocks = sim.iter().map(|r| r.block).filter(|b| *b > 0).collect::<std::collections::BTreeSet<_>>().len();
    println!(
        "queue: {} transactions in {blocks} blocks (simulated), fees {total} native units at the simulated base fees, the largest reserve {reserve} at base fee 8; payer {payer} holds {balance}",
        signed.len() - base
    );
    let mut batch_steps = vec![];
    for st in &steps {
        if st.loader {
            if st.kind == "header" {
                batch_steps.push(st.batch.clone());
            }
            continue;
        }
        let x = signed[base..].iter().find(|x| x.kind == st.kind).unwrap();
        let mut b = st.batch.clone();
        b["call"]["gas_limit"] = json!(x.gas_limit);
        batch_steps.push(b);
    }
    write_json(&run.join(if dry_run { "batch.dry-run.json" } else { "batch.json" }), &json!({"payer": payer.to_string(), "gas_limit": LOADER_GAS, "steps": batch_steps}))?;
    if dry_run {
        let _ = fs::remove_file(run.join("applied.txt.next"));
        write_json(
            &run.join("plan.json"),
            &json!({"image_id": image_id_hex(FERRATA_V03_ID), "elf_bytes": FERRATA_V03_ELF.len(), "fees_total": total.to_string(),
                    "largest_reserve_at_base_fee_8": reserve.to_string(), "simulated_utc": utc(now_ms()), "blocks": blocks,
                    "transactions": signed[base..].iter().enumerate().map(|(k, x)| json!({"label": x.label, "bytes": x.bytes, "declared_gas": x.gas_limit,
                        "exec_gas": x.sim_exec, "block": sim[k].block.saturating_sub(first) + 1, "fee": x.sim_fee.to_string()})).collect::<Vec<_>>()}),
        )?;
        println!("queue --dry-run: nothing signed was kept, nothing sent");
        return Ok(());
    }
    // Keep everything before sending anything.
    save_signed(run, &signed)?;
    fs::rename(run.join("applied.txt.next"), run.join("applied.txt"))?;
    let mut runlog = fs::read_to_string(run.join("run-log.tsv")).unwrap_or_else(|_| "utc\tevent\tpayer_nonce\tpayer_balance\thead\n".into());
    let head = client.get_last_block_id().await.unwrap_or(0);
    runlog.push_str(&format!("{}\tqueue {} new transaction(s)\t{chain_nonce}\t{balance}\t{head}\n", utc(now_ms()), fresh.len()));
    fs::write(run.join("run-log.tsv"), runlog)?;
    for (label, tx, _) in &fresh {
        let hash = client.send_transaction(LeeTransaction::Public(tx.clone())).await.map_err(|e| anyhow!("sendTransaction ({label}): {e}"))?;
        println!("queued {hash} {label}");
    }
    Ok(())
}

/// Send again, byte for byte, every signed transaction whose payer nonce the chain has not passed.
async fn resend(run: &Path) -> Result<usize> {
    let w = WalletCore::from_env()?;
    let client = w.client_owned();
    let payer = account(&read_json(&run.join("state.json"))?["payer"])?;
    let now = client.get_accounts_nonces(vec![payer]).await.map_err(|e| anyhow!("getAccountsNonces: {e}"))?[0].0;
    let mut n = 0;
    for x in load_signed(run)? {
        if x.payer_nonce >= now {
            client.send_transaction(LeeTransaction::Public(x.tx.clone())).await.map_err(|e| anyhow!("sendTransaction ({}): {e}", x.label))?;
            n += 1;
        }
    }
    println!("resend: {n} transaction(s) sent again (the payer's nonce is {now})");
    Ok(n)
}

/// What each signed transaction should have done, read from state and compared in full with
/// what the run's data implies (a v0.3.1 transaction reports no status: a refused one is in a
/// block too).
fn verdict(view: &MapView, run: &Path, x: &Signed) -> Result<String> {
    let s = read_json(&run.join("state.json"))?;
    let program = account(&s["program"])?;
    let inst = base_instance(run, program)?;
    let boot = bootstrap(run, &inst.config)?;
    let ups = updates(run)?;
    let kind = x.kind.as_str();
    let segments: Vec<AccountId> = s["segments"].as_array().unwrap().iter().map(account).collect::<Result<_>>()?;
    Ok(if let Some(i) = kind.strip_prefix("segment:") {
        let i: usize = i.parse()?;
        let want = &segment_shards(&segments)[i];
        if view.shard_of(segments[i], PROGRAM_LOADER_ACCOUNT_ID) == *want { "segment holds its chunk".into() } else { "MISSING: the segment does not hold its chunk".into() }
    } else if kind == "header" {
        match ProgramHeader::from_bytes(&view.shard_of(program, PROGRAM_LOADER_ACCOUNT_ID)) {
            Some(h) if h.image_id == FERRATA_V03_ID && h.program_first_segment == segments[0] && h.immutable => {
                format!("program at {program}, image {}, immutable", image_id_hex(h.image_id))
            }
            other => format!("MISSING program header ({other:?})"),
        }
    } else if kind == "init" || kind == "adopt-init" {
        let slot = boot.header.beacon.slot;
        let fork = inst.config.fork().unwrap();
        let (store, job) = ferrata_core::steps::initialize(&inst.checkpoint, &ferrata_core::witness::CompactBootstrap::of(&boot), fork, &inst.spec, 1)
            .map_err(|e| anyhow!("{e:?}"))?;
        let want = RecordState { instance_id: inst.id, chain_id: inst.config.chain_id, record: job.pending.record().encode() };
        let rec = view.shard_of(inst.header_account(slot), program);
        let st = view.shard_of(inst.account(), program);
        let active = borsh::from_slice::<InstanceState>(&st).map(|st| st.active && st.id == inst.id).unwrap_or(false);
        if rec == borsh::to_vec(&want)? && active {
            format!("instance {} active; record of the checkpoint (slot {slot}) equal to the bootstrap's; initial store {}", hex::encode(&inst.id[..8]), hex::encode(&store.digest()[..8]))
        } else {
            "MISSING: the instance is not active or the checkpoint's record is not the bootstrap's".into()
        }
    } else if let Some(name) = kind.strip_prefix("update:").or(kind.strip_prefix("continue:")) {
        let (_, u) = ups.iter().find(|(n, _)| n == name).ok_or_else(|| anyhow!("no update {name}"))?;
        let slot = u.finalized_header.beacon.slot;
        let want = record_at(&inst, &MapView::default(), &ups, slot).unwrap();
        if view.shard_of(inst.header_account(slot), program) == borsh::to_vec(&want)? {
            format!("record of finalized slot {slot} equal to the update's header")
        } else {
            format!("MISSING: no record of slot {slot} equal to the update's header")
        }
    } else if kind == "adopt-pending" {
        let st = view.shard_of(inst.account(), program);
        match borsh::from_slice::<InstanceState>(&st) {
            Ok(st) if st.job.is_none() => "the pending handoff adopted and applied".into(),
            _ => "MISSING: a job still waits".into(),
        }
    } else if let Some(slot) = kind.strip_prefix("attest:") {
        let slot: u64 = slot.parse()?;
        let (_, query) = proofs(run, inst.config.chain_id).into_iter().find(|(s, _)| *s == slot).ok_or_else(|| anyhow!("no proof for {slot}"))?;
        let rec = record_at(&inst, view, &ups, slot).ok_or_else(|| anyhow!("no record of {slot}"))?;
        let mut out = vec![];
        let mut block = 0;
        for st in inst.statements(&rec, &query) {
            block = st.block_number;
            let got = view.shard_of(ferrata_v03_wire::statement_account(&program, &inst.id, &st.id_at_block), program);
            let ok = borsh::from_slice::<Statement>(&got).ok().as_ref() == Some(&st);
            out.push(format!("{}slot {} = 0x{}", if ok { "" } else { "MISSING " }, u64::from_be_bytes(st.slot[24..].try_into().unwrap()), hex::encode(st.value)));
        }
        format!("{} at Sepolia block {block}", out.join(", "))
    } else {
        "?".into()
    })
}

async fn wait(run: &Path, secs: u64) -> Result<()> {
    let w = WalletCore::from_env()?;
    let client = w.client_owned();
    let signed = load_signed(run)?;
    let payer = account(&read_json(&run.join("state.json"))?["payer"])?;
    let last = signed.iter().map(|x| x.payer_nonce).max().ok_or_else(|| anyhow!("nothing signed"))?;
    let t0 = Instant::now();
    let mut stalled_since = client.get_last_block_id().await.unwrap_or(0);
    loop {
        let nonce = client.get_accounts_nonces(vec![payer]).await.map_err(|e| anyhow!("getAccountsNonces: {e}"))?[0].0;
        if nonce > last {
            break;
        }
        if t0.elapsed() > Duration::from_secs(secs) {
            bail!("PENDING: the payer's nonce is {nonce}, the last signed is {last}; run again later (it re-sends the same bytes)");
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
        let now = client.get_last_block_id().await.unwrap_or(stalled_since);
        // Two blocks without our next transaction: it may have been dropped behind a rewound one.
        let moved = client.get_accounts_nonces(vec![payer]).await.map(|v| v[0].0).unwrap_or(nonce) != nonce;
        if moved {
            stalled_since = now;
        } else if now >= stalled_since + 2 {
            resend(run).await?;
            let mut runlog = fs::read_to_string(run.join("run-log.tsv")).unwrap_or_default();
            runlog.push_str(&format!("{}\tresend after two blocks without progress\t{nonce}\t-\t{now}\n", utc(now_ms())));
            fs::write(run.join("run-log.tsv"), runlog)?;
            stalled_since = now;
        }
    }
    let balance = client.get_account_balance(payer).await.unwrap_or(0);
    let head = client.get_last_block_id().await.unwrap_or(0);
    let mut runlog = fs::read_to_string(run.join("run-log.tsv")).unwrap_or_default();
    runlog.push_str(&format!("{}\tevery signed transaction in a block\t{}\t{balance}\t{head}\n", utc(now_ms()), last + 1));
    fs::write(run.join("run-log.tsv"), runlog)?;
    let (view, _) = read_state(&client, &watched(run)?).await?;
    let mut lines = vec!["seq\tlabel\ttx_hash\tblock\tbedrock_status\tbytes\tdeclared_gas\texec_gas_simulated\tverdict".to_owned()];
    for x in &signed {
        let hash: HashType = x.hash.parse().map_err(|e| anyhow!("{e:?}"))?;
        let (block, status) = match client.get_transaction(hash).await.map_err(|e| anyhow!("getTransaction: {e}"))? {
            Some((_, b)) => {
                let st = client.get_block(b).await.ok().flatten().map(|blk| format!("{:?}", blk.bedrock_status)).unwrap_or_default();
                (b.to_string(), st)
            }
            None => ("-".into(), "-".into()),
        };
        let v = verdict(&view, run, x)?;
        println!("{} block {block} ({status}): {} — {v}", x.seq, x.label);
        lines.push(format!("{}\t{}\t{}\t{block}\t{status}\t{}\t{}\t{}\t{v}", x.seq, x.label, x.hash, x.bytes, x.gas_limit, x.sim_exec));
    }
    fs::write(run.join("txs.tsv"), lines.join("\n") + "\n")?;
    println!("wait: every signed transaction is in a block ({:.0} s)", t0.elapsed().as_secs_f64());
    Ok(())
}

/// Each signed transaction's block and that block's Bedrock status (tab-separated: hash, block, status, label).
async fn status(run: &Path) -> Result<()> {
    let w = WalletCore::from_env()?;
    let client = w.client_owned();
    for x in load_signed(run)? {
        let hash: HashType = x.hash.parse().map_err(|e| anyhow!("{e:?}"))?;
        match client.get_transaction(hash).await.map_err(|e| anyhow!("getTransaction: {e}"))? {
            Some((_, b)) => {
                let st = client.get_block(b).await.ok().flatten().map(|blk| format!("{:?}", blk.bedrock_status)).unwrap_or("-".into());
                println!("{}\t{b}\t{st}\t{}", x.hash, x.label);
            }
            None => println!("{}\t-\t-\t{}", x.hash, x.label),
        }
    }
    Ok(())
}

/// Whether the run's goal holds on chain (exit 0), or what the next round needs (exit 4): the
/// program deployed; the instance active, no job waiting, at least two updates applied after the
/// checkpoint; the statements under the record of its finalized header that the proofs cover.
/// `NEED-FINALITY <slot>`: fetch a finality update newer than the slot; `NEED-PROOF <slot> <hash>`:
/// fetch the vault's proof at that block.
async fn goal(run: &Path) -> Result<bool> {
    let w = WalletCore::from_env()?;
    let client = w.client_owned();
    let (view, _) = read_state(&client, &watched(run)?).await?;
    let s = read_json(&run.join("state.json"))?;
    let program = account(&s["program"])?;
    let segments: Vec<AccountId> = s["segments"].as_array().unwrap().iter().map(account).collect::<Result<_>>()?;
    let mut needs = vec![];
    if deployment(&view, program, &segments) != Deployment::Ours {
        needs.push("NEED-ROUND the program is not deployed".to_owned());
    }
    let inst = base_instance(run, program)?;
    let st = view.shard_of(inst.account(), program);
    match borsh::from_slice::<InstanceState>(&st) {
        Err(_) => needs.push("NEED-ROUND no instance".into()),
        Ok(state) => {
            let store = CompactStore::decode(&state.store).map_err(|e| anyhow!("{e:?}"))?;
            let ups = updates(run)?;
            let newer = ups.iter().filter(|(_, u)| u.finalized_header.beacon.slot > store.finalized.slot).count();
            if !state.active || state.job.is_some() {
                needs.push("NEED-ROUND the instance is not active or a job waits".into());
            } else if newer > 0 {
                needs.push(format!("NEED-ROUND {newer} fetched update(s) newer than the store"));
            } else if state.applied < 3 {
                needs.push(format!("NEED-FINALITY {}", store.finalized.slot));
            } else {
                let mut notes = vec![];
                let _ = plan_steps(run, &view, &mut notes);
                if let Some(n) = notes.iter().find(|n| n.starts_with("NEED-PROOF")) {
                    needs.push(n.clone());
                } else if !notes.iter().any(|n| n.starts_with("the statements under slot")) {
                    needs.push("NEED-ROUND the attestation".into());
                }
            }
        }
    }
    for n in &needs {
        println!("{n}");
    }
    if needs.is_empty() {
        println!("DONE");
    }
    Ok(needs.is_empty())
}

/// The finalized slot the instance holds on chain and the execution block its record binds.
async fn finalized(run: &Path) -> Result<()> {
    let w = WalletCore::from_env()?;
    let client = w.client_owned();
    let program = program_of(run)?;
    let inst = base_instance(run, program)?;
    let (view, _) = read_state(&client, &[inst.account()]).await?;
    let st: InstanceState = borsh::from_slice(&view.shard_of(inst.account(), program)).map_err(|_| anyhow!("no instance on chain"))?;
    let slot = CompactStore::decode(&st.store).map_err(|e| anyhow!("{e:?}"))?.finalized.slot;
    let (view, _) = read_state(&client, &[inst.header_account(slot)]).await?;
    let rec: RecordState = borsh::from_slice(&view.shard_of(inst.header_account(slot), program)).map_err(|_| anyhow!("no record at {slot}"))?;
    let hash = match HeaderRecord::decode(&rec.record).map_err(|e| anyhow!("{e:?}"))?.execution {
        ExecutionSummary::BlockHash(h) => h,
        ExecutionSummary::Payload { block_hash, .. } => block_hash,
        ExecutionSummary::None => bail!("the record binds no execution block"),
    };
    println!("{slot} 0x{}", hex::encode(hash));
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The record
// ---------------------------------------------------------------------------------------------

/// Every verdict, read back from the sequencer's state.
async fn snapshot(run: &Path) -> Result<Value> {
    let w = WalletCore::from_env()?;
    let client = w.client_owned();
    let s = read_json(&run.join("state.json"))?;
    let program = account(&s["program"])?;
    let inst = instance(run, program)?;
    let shard = async |id: AccountId, p: AccountId| -> Result<Vec<u8>> {
        Ok(client.get_account(id).await.map_err(|e| anyhow!("getAccount: {e}"))?.data.shard(p).to_vec())
    };
    let header = ProgramHeader::from_bytes(&shard(program, PROGRAM_LOADER_ACCOUNT_ID).await?);
    let raw = shard(inst.account(), program).await?;
    let st: Option<InstanceState> = (!raw.is_empty()).then(|| borsh::from_slice(&raw)).transpose()?;
    let mut slots = vec![read_json(&run.join("chain.json"))?["bootstrap_slot"].as_u64().unwrap_or(0)];
    let mut replay = Instance::new(program, inst.config.clone(), inst.checkpoint, &bootstrap(run, &inst.config)?);
    for name in applied(run) {
        replay.follow(&load_update(&run.join("updates").join(&name))?);
        slots.push(replay.native.core().finalized_header.beacon.slot);
    }
    slots.dedup();
    let mut records = vec![];
    for slot in slots {
        let r = shard(inst.header_account(slot), program).await?;
        records.push(json!({"slot": slot, "account": inst.header_account(slot).to_string(), "shard": (!r.is_empty()).then(|| hex::encode(&r))}));
    }
    let mut statements = vec![];
    let mut dirs: Vec<PathBuf> = fs::read_dir(run.join("proofs")).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    dirs.sort();
    for dir in dirs {
        let (Ok(proof), Ok(rlp)) = (fs::read(dir.join("proof.json")), fs::read(dir.join("header.rlp"))) else { continue };
        let Some(slot) = dir.file_name().and_then(|n| n.to_str()).and_then(|n| n.parse::<u64>().ok()) else { continue };
        let r = shard(inst.header_account(slot), program).await?;
        if r.is_empty() {
            continue;
        }
        let rec: RecordState = borsh::from_slice(&r)?;
        let query = storage_query(&proof, inst.config.chain_id, Some(rlp), &[]);
        for claimed in inst.statements(&rec, &query) {
            let id = ferrata_v03_wire::statement_account(&program, &inst.id, &claimed.id_at_block);
            let got = shard(id, program).await?;
            statements.push(json!({
                "account": id.to_string(),
                "address": format!("0x{}", hex::encode(claimed.address)),
                "slot": format!("0x{}", hex::encode(claimed.slot)),
                "value": format!("0x{}", hex::encode(claimed.value)),
                "block_number": claimed.block_number,
                "statement_id": hex::encode(claimed.id),
                "shard": (!got.is_empty()).then(|| hex::encode(&got)),
            }));
        }
    }
    let mut txs = vec![];
    for x in load_signed(run)? {
        let hash: HashType = x.hash.parse().map_err(|e| anyhow!("{e:?}"))?;
        let got = client.get_transaction(hash).await.map_err(|e| anyhow!("getTransaction: {e}"))?;
        txs.push(json!({"label": x.label, "tx_hash": x.hash, "block": got.map(|(_, b)| b)}));
    }
    Ok(json!({
        "sequencer": s["sequencer"],
        "program": program.to_string(),
        "image_id": header.map(|h| image_id_hex(h.image_id)),
        "instance": {
            "id": hex::encode(inst.id),
            "account": inst.account().to_string(),
            "shard": (!raw.is_empty()).then(|| hex::encode(&raw)),
            "active": st.as_ref().map(|st| st.active),
            "applied": st.as_ref().map(|st| st.applied),
            "finalized_slot": st.as_ref().and_then(|st| CompactStore::decode(&st.store).ok()).map(|c| c.finalized.slot),
        },
        "records": records,
        "statements": statements,
        "transactions": txs,
    }))
}

async fn record(run: &Path) -> Result<Value> {
    let snap = snapshot(run).await?;
    write_json(&run.join("record.json"), &snap)?;
    println!("record: written {}", run.join("record.json").display());
    Ok(snap)
}

async fn verify(run: &Path) -> Result<()> {
    let want = read_json(&run.join("record.json"))?;
    let got = snapshot(run).await?;
    let mut bad = 0;
    let mut check = |what: &str, a: &Value, b: &Value| {
        let ok = a == b && !a.is_null();
        if !ok {
            bad += 1;
        }
        println!("{} {what}", if ok { "MATCH   " } else { "MISMATCH" });
    };
    check("program image id", &want["image_id"], &got["image_id"]);
    check("instance state", &want["instance"], &got["instance"]);
    let empty = vec![];
    for (i, r) in want["records"].as_array().unwrap_or(&empty).iter().enumerate() {
        check(&format!("record of slot {}", r["slot"]), r, &got["records"][i]);
    }
    for (i, r) in want["statements"].as_array().unwrap_or(&empty).iter().enumerate() {
        check(&format!("statement {} = {}", r["slot"], r["value"]), r, &got["statements"][i]);
    }
    for (i, t) in want["transactions"].as_array().unwrap_or(&empty).iter().enumerate() {
        check(&format!("transaction {} in block {}", t["label"], t["block"]), t, &got["transactions"][i]);
    }
    let state_checks = 2 + want["records"].as_array().map_or(0, |a| a.len()) + want["statements"].as_array().map_or(0, |a| a.len());
    let inclusion_checks = want["transactions"].as_array().map_or(0, |a| a.len());
    // Independent of record.json: each signed transaction's effect, read from state and compared in
    // full with what the run's data implies (the chunks, the image id, the bootstrap's record and
    // instance, each update's record, the statements).
    let w = WalletCore::from_env()?;
    let (view, _) = read_state(&w.client_owned(), &watched(run)?).await?;
    let signed = load_signed(run)?;
    for x in &signed {
        let v = verdict(&view, run, x)?;
        let ok = !v.contains("MISSING");
        if !ok {
            bad += 1;
        }
        println!("{} effect of {}: {v}", if ok { "MATCH   " } else { "MISMATCH" }, x.label);
    }
    println!(
        "verify: {} checks: {state_checks} compare state (image id, instance, records, statements) with record.json; {inclusion_checks} compare \
         a transaction's block of inclusion (v0.3.1 includes a refused transaction too); {} compare each transaction's effect in \
         state with what the data implies",
        state_checks + inclusion_checks + signed.len(),
        signed.len()
    );
    if state_checks + inclusion_checks + signed.len() == 0 {
        bail!("no check ran");
    }
    if bad > 0 {
        bail!("{bad} check(s) failed");
    }
    println!("verify: every check matches ({})", run.join("record.json").display());
    Ok(())
}
