//! `ferrata-v03`: deploys and drives Ferrata's LEZ v0.3.1 light-client program on a LEZ v0.3.1
//! sequencer with LEZ v0.3.1's wallet (its key storage, signing, program loader facade and
//! sequencer client). EXPERIMENTAL, UNAUDITED (`../README.md`).
//!
//! ```text
//! ferrata-v03 new-wallet RUN URL        a fresh wallet at $LEE_WALLET_HOME_DIR pointed at URL, one public account (the payer)
//! ferrata-v03 plan RUN                  every transaction of a run on RUN's data, simulated on LEZ v0.3.1's state machine
//! ferrata-v03 deploy RUN                the program through LEZ's program loader (resumes; skipped once on chain)
//! ferrata-v03 init RUN                  Init from RUN/bootstrap.ssz, the committee adopted in the same transaction
//! ferrata-v03 update RUN FILE           a light-client update; skipped if the instance already holds it
//! ferrata-v03 finalized RUN             the instance's finalized slot and the execution block its record binds
//! ferrata-v03 attest RUN PROOF HEADER   Attest the proof's slots under the record of the finalized header
//! ferrata-v03 record RUN                read every verdict back, write RUN/record.json
//! ferrata-v03 verify RUN                read every verdict back, compare with RUN/record.json (exit 0 iff all match)
//! ```
//!
//! RUN holds the data `scripts/fetch.py` fetched (`chain.json`, `bootstrap.ssz`, `updates/`,
//! `proofs/`) and what this program writes: `state.json` (the program, the payer, the deployment),
//! `applied.txt` (the updates the instance holds, in order: the native light client is rebuilt
//! from them), `txs.tsv` (every transaction sent) and `record.json`. Every command first reads the
//! chain, so a command run twice does nothing the second time, and an interrupted run resumes.

use std::{
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
use ferrata_v03_wire::{InstanceState, RecordState, Statement, Verify};
use harness_v03::*;
use lee::{
    public_transaction::{Message, WitnessSet},
    AccountId, FeeDeclaration, PublicTransaction,
};
use lee_core::{
    account::ProgramShardSelector,
    program::{ProgramHeader, PROGRAM_LOADER_ACCOUNT_ID},
};
use sequencer_service_rpc::RpcClient as _;
use serde_json::{json, Value};
use wallet::{program_facades::program_loader::ProgramLoader, WalletCore};

/// The gas limit every program transaction declares: LEZ v0.3.1's cap (the fee charged is the
/// cycles used; the reserve of the rest is refunded in the same block).
const PROGRAM_GAS_LIMIT: u64 = MAX_GAS_EXEC;
/// Exit code of `update` when the update is signed by a committee the store does not know yet:
/// the store's period update must be applied first.
const EXIT_NEEDS_PERIOD_UPDATE: i32 = 3;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["new-wallet", run, url] => new_wallet(Path::new(run), url),
        ["plan", run] => plan(Path::new(run)),
        ["deploy", run] => deploy(Path::new(run)).await,
        ["init", run] => init(Path::new(run)).await,
        ["update", run, file] => update(Path::new(run), Path::new(file)).await,
        ["finalized", run] => finalized(Path::new(run)).await,
        ["attest", run, proof, header] => attest(Path::new(run), Path::new(proof), Path::new(header)).await,
        ["record", run] => record(Path::new(run)).await.map(|_| ()),
        ["verify", run] => verify(Path::new(run)).await,
        _ => {
            eprintln!("usage: see the documentation at the top of lez-v03/client/src/main.rs");
            std::process::exit(2)
        }
    };
    if let Err(e) = result {
        eprintln!("error: {e:#}");
        let code = if format!("{e:#}").contains("NEEDS-PERIOD-UPDATE") { EXIT_NEEDS_PERIOD_UPDATE } else { 1 };
        std::process::exit(code);
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

fn utc(ms: u64) -> String {
    // RFC 3339 without a date library: days since the epoch to a civil date (Howard Hinnant's algorithm).
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

fn state(run: &Path) -> Result<Value> {
    let p = run.join("state.json");
    if p.exists() {
        read_json(&p)
    } else {
        Ok(json!({}))
    }
}

fn save_state(run: &Path, s: &Value) -> Result<()> {
    write_json(&run.join("state.json"), s)
}

fn account(v: &Value) -> Result<AccountId> {
    v.as_str().ok_or_else(|| anyhow!("not an account id: {v}"))?.parse().map_err(|e| anyhow!("account id {v}: {e:?}"))
}

/// The instance's configuration: Sepolia's (`harness_v03::sepolia_config`), the store in the
/// bootstrap's layout, the committee adopted in one part.
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
    let fork = cfg.fork().unwrap();
    LightClientBootstrap::decode(&fs::read(run.join("bootstrap.ssz"))?, fork, 512)
        .map_err(|e| anyhow!("bootstrap.ssz: {e:?}"))
}

fn checkpoint(run: &Path) -> Result<ferrata_core::Root> {
    let chain = read_json(&run.join("chain.json"))?;
    Ok(root(chain["checkpoint"].as_str().ok_or_else(|| anyhow!("chain.json: no checkpoint"))?))
}

/// An update file of `updates/`: `*-finality-*.ssz` (a Gloas `LightClientFinalityUpdate`) or
/// `*-period-*.ssz` (a `LightClientUpdate` in the Gloas layout).
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

/// The instance (program from `state.json`) and the native light client rebuilt from the
/// bootstrap and every update the instance holds.
fn instance(run: &Path, program: AccountId) -> Result<Instance> {
    let cfg = config(run)?;
    let boot = bootstrap(run, &cfg)?;
    let mut inst = Instance::new(program, cfg, checkpoint(run)?, &boot);
    for name in applied(run) {
        let u = load_update(&run.join("updates").join(&name))?;
        inst.follow(&u);
    }
    Ok(inst)
}

fn log_tx(run: &Path, line: &[String]) -> Result<()> {
    let p = run.join("txs.tsv");
    let mut s = fs::read_to_string(&p).unwrap_or_else(|_| {
        "label\ttx_hash\tblock\tbytes\tgas_limit\tfee\texec_gas_derived\tbase_fee_exec\tbase_fee_stor\tsent_utc\tincluded_s\tverdict\n"
            .to_owned()
    });
    s.push_str(&line.join("\t"));
    s.push('\n');
    fs::write(p, s)?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The wallet and the sequencer
// ---------------------------------------------------------------------------------------------

fn wallet_home() -> Result<PathBuf> {
    std::env::var_os(wallet::HOME_DIR_ENV_VAR)
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("set {} to the wallet's directory", wallet::HOME_DIR_ENV_VAR))
}

fn new_wallet(run: &Path, url: &str) -> Result<()> {
    let home = wallet_home()?;
    if home.join("storage.json").exists() {
        bail!("{} already holds a wallet", home.display());
    }
    fs::create_dir_all(&home)?;
    fs::create_dir_all(run)?;
    let cfg = json!({
        "sequencer": {"sequencer_addr": url},
        "seq_poll_timeout": "30s",
        "seq_tx_poll_max_blocks": 120,
        "seq_poll_max_retries": 20,
        "seq_block_poll_max_amount": 100,
        "gas_limit": 2_000_000
    });
    write_json(&home.join("wallet_config.json"), &cfg)?;
    // The storage's password (its seed): 32 random bytes, kept beside the storage, never printed.
    let mut seed = [0u8; 32];
    use std::io::Read;
    fs::File::open("/dev/urandom")?.read_exact(&mut seed)?;
    let password = hex::encode(seed);
    fs::write(home.join("password"), &password)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(home.join("password"), fs::Permissions::from_mode(0o600))?;
    }
    let (mut w, _mnemonic) = WalletCore::new_init_storage(
        home.join("wallet_config.json"),
        home.join("storage.json"),
        None,
        &password,
    )?;
    let (payer, _) = w.create_new_account_public(None);
    w.store_persistent_data()?;
    let mut s = state(run)?;
    s["payer"] = json!(payer.to_string());
    s["sequencer"] = json!(url);
    save_state(run, &s)?;
    println!("{payer}");
    Ok(())
}

struct Node {
    w: WalletCore,
}

struct Sent {
    hash: HashType,
    block: u64,
    fee: u128,
}

impl Node {
    fn open() -> Result<Self> {
        Ok(Self { w: WalletCore::from_env()? })
    }

    async fn account(&self, id: AccountId) -> Result<lee_core::account::Account> {
        self.w.client_owned().get_account(id).await.map_err(|e| anyhow!("getAccount {id}: {e}"))
    }

    async fn shard(&self, id: AccountId, program: AccountId) -> Result<Vec<u8>> {
        Ok(self.account(id).await?.data.shard(program).to_vec())
    }

    async fn balance(&self, id: AccountId) -> Result<u128> {
        self.w.client_owned().get_account_balance(id).await.map_err(|e| anyhow!("getAccountBalance: {e}"))
    }

    async fn fee_state(&self) -> Result<(u64, u64)> {
        let q = self.w.client_owned().get_fee_state().await.map_err(|e| anyhow!("getFeeState: {e}"))?;
        Ok((q.base_fee_exec, q.base_fee_stor))
    }

    /// Send one public transaction to `program`, paid and signed by `payer` alone (the program
    /// needs no other signature), and wait until it is in a block.
    async fn send(
        &self,
        run: &Path,
        label: &str,
        payer: AccountId,
        program: AccountId,
        accounts: &[AccountId],
        data: Vec<u8>,
        verdict: impl AsyncFnOnce() -> Result<String>,
    ) -> Result<Sent> {
        let client = self.w.client_owned();
        let nonce = client.get_accounts_nonces(vec![payer]).await.map_err(|e| anyhow!("getAccountsNonces: {e}"))?[0];
        let selectors = accounts.iter().map(|a| ProgramShardSelector::new(*a, program)).collect();
        let fee = FeeDeclaration::new(payer, PROGRAM_GAS_LIMIT, 0, wallet::max_fee_for(PROGRAM_GAS_LIMIT));
        let message = Message::new_preserialized(program, selectors, vec![nonce], data, Some(fee));
        let key = self.w.get_account_public_signing_key(payer).ok_or_else(|| anyhow!("the wallet holds no key of {payer}"))?;
        let witness = WitnessSet::for_message(&message, &[key]);
        let tx = LeeTransaction::Public(PublicTransaction::new(message, witness));
        let bytes = borsh::to_vec(&tx)?.len();
        let (base_exec, base_stor) = self.fee_state().await?;
        let before = self.balance(payer).await?;
        let sent_at = now_ms();
        let t0 = Instant::now();
        let hash = client.send_transaction(tx).await.map_err(|e| anyhow!("sendTransaction ({label}): {e}"))?;
        println!("{label}: sent {hash} ({bytes} bytes)");
        let (_, block) = self.w.poll_transaction(hash).await.with_context(|| format!("{label}: {hash} not in a block"))?;
        let included = t0.elapsed().as_secs_f64();
        let after = self.balance(payer).await?;
        let fee = before.saturating_sub(after);
        // The execution gas the block charged, derived from the fee at the quoted base fees.
        let exec = fee.saturating_sub(bytes as u128 * u128::from(base_stor)) / u128::from(base_exec.max(1));
        let v = verdict().await?;
        println!("{label}: block {block}, fee {fee}, about {exec} execution gas, {included:.1} s: {v}");
        log_tx(
            run,
            &[
                label.into(),
                hash.to_string(),
                block.to_string(),
                bytes.to_string(),
                PROGRAM_GAS_LIMIT.to_string(),
                fee.to_string(),
                exec.to_string(),
                base_exec.to_string(),
                base_stor.to_string(),
                utc(sent_at),
                format!("{included:.1}"),
                v,
            ],
        )?;
        Ok(Sent { hash, block, fee })
    }

    async fn instance_state(&self, inst: &Instance) -> Result<Option<InstanceState>> {
        let shard = self.shard(inst.account(), inst.program).await?;
        Ok((!shard.is_empty()).then(|| borsh::from_slice(&shard)).transpose()?)
    }

    async fn record_state(&self, inst: &Instance, slot: u64) -> Result<Option<RecordState>> {
        let shard = self.shard(inst.header_account(slot), inst.program).await?;
        Ok((!shard.is_empty()).then(|| borsh::from_slice(&shard)).transpose()?)
    }

    async fn statement(&self, inst: &Instance, s: &Statement) -> Result<Option<Statement>> {
        let id = ferrata_v03_wire::statement_account(&inst.program, &inst.id, &s.id_at_block);
        let shard = self.shard(id, inst.program).await?;
        Ok((!shard.is_empty()).then(|| borsh::from_slice(&shard)).transpose()?)
    }
}

fn program_of(s: &Value) -> Result<AccountId> {
    account(&s["program"]).context("state.json names no program: run `deploy` first")
}

// ---------------------------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------------------------

async fn deploy(run: &Path) -> Result<()> {
    let mut node = Node::open()?;
    let mut s = state(run)?;
    let payer = account(&s["payer"])?;
    if let Ok(program) = program_of(&s) {
        let header = ProgramHeader::from_bytes(&node.shard(program, PROGRAM_LOADER_ACCOUNT_ID).await?);
        if header.is_some_and(|h| h.image_id == FERRATA_V03_ID) {
            println!("deploy: the program is at {program} (image {}): nothing to do", image_id_hex(FERRATA_V03_ID));
            return Ok(());
        }
    }
    let binary = risc0_binfmt::ProgramBinary::decode(FERRATA_V03_ELF).map_err(|e| anyhow!("{e}"))?;
    let chunks: Vec<&[u8]> = binary.user_elf.chunks(program_loader_core::MAX_SEGMENT_DATA_LEN).collect();
    // The accounts are chosen once and kept, so an interrupted deployment resumes.
    if s["deploy"].is_null() {
        let header = node.w.create_new_account_public(None).0;
        let segments: Vec<AccountId> = (0..chunks.len()).map(|_| node.w.create_new_account_public(None).0).collect();
        node.w.store_persistent_data()?;
        s["deploy"] = json!({"header": header.to_string(), "segments": segments.iter().map(|a| a.to_string()).collect::<Vec<_>>()});
        save_state(run, &s)?;
    }
    let header = account(&s["deploy"]["header"])?;
    let segments: Vec<AccountId> = s["deploy"]["segments"].as_array().unwrap().iter().map(account).collect::<Result<_>>()?;
    let loader = ProgramLoader(&node.w);
    for i in (0..chunks.len()).rev() {
        if !node.shard(segments[i], PROGRAM_LOADER_ACCOUNT_ID).await?.is_empty() {
            println!("deploy: segment {} of {} already written", i + 1, chunks.len());
            continue;
        }
        let before = node.balance(payer).await?;
        let t0 = Instant::now();
        let sent_at = now_ms();
        let hash = loader
            .write_segment(segments[i], chunks[i].to_vec(), segments.get(i + 1).copied(), Some(payer))
            .await
            .map_err(|e| anyhow!("WriteSegment {i}: {e:?}"))?;
        let (_, block) = node.w.poll_transaction(hash).await?;
        let fee = before.saturating_sub(node.balance(payer).await?);
        let label = format!("deploy: WriteSegment {} of {} ({} bytes)", i + 1, chunks.len(), chunks[i].len());
        println!("{label}: {hash} block {block} fee {fee}");
        log_tx(run, &[label, hash.to_string(), block.to_string(), "-".into(), "wallet".into(), fee.to_string(), "0".into(), "-".into(), "-".into(), utc(sent_at), format!("{:.1}", t0.elapsed().as_secs_f64()), "segment written".into()])?;
    }
    let before = node.balance(payer).await?;
    let t0 = Instant::now();
    let sent_at = now_ms();
    let hash = loader
        .create_header(header, segments[0], &segments, true, Some(payer))
        .await
        .map_err(|e| anyhow!("CreateHeader: {e:?}"))?;
    let (_, block) = node.w.poll_transaction(hash).await?;
    let fee = before.saturating_sub(node.balance(payer).await?);
    let h = ProgramHeader::from_bytes(&node.shard(header, PROGRAM_LOADER_ACCOUNT_ID).await?)
        .ok_or_else(|| anyhow!("no program header at {header}"))?;
    if h.image_id != FERRATA_V03_ID {
        bail!("the loader computed image {}, not {}", image_id_hex(h.image_id), image_id_hex(FERRATA_V03_ID));
    }
    let label = "deploy: CreateHeader (immutable)".to_owned();
    log_tx(run, &[label.clone(), hash.to_string(), block.to_string(), "-".into(), "wallet".into(), fee.to_string(), "0".into(), "-".into(), "-".into(), utc(sent_at), format!("{:.1}", t0.elapsed().as_secs_f64()), format!("image {}", image_id_hex(h.image_id))])?;
    s["program"] = json!(header.to_string());
    s["image_id"] = json!(image_id_hex(FERRATA_V03_ID));
    s["elf_bytes"] = json!(FERRATA_V03_ELF.len());
    s["segments"] = json!(chunks.len());
    save_state(run, &s)?;
    println!("deploy: the program is at {header}, image {} (computed by the loader)", image_id_hex(h.image_id));
    Ok(())
}

async fn init(run: &Path) -> Result<()> {
    let node = Node::open()?;
    let mut s = state(run)?;
    let payer = account(&s["payer"])?;
    let program = program_of(&s)?;
    let inst = instance(run, program)?;
    let boot = bootstrap(run, &inst.config)?;
    if let Some(st) = node.instance_state(&inst).await? {
        println!("init: instance {} exists (active {}, {} jobs applied): nothing to do", hex::encode(inst.id), st.active, st.applied);
    } else {
        let (p, rest) = inst.init(&boot, usize::MAX);
        assert!(rest.is_empty());
        if now_ms() >= p_window_end(&p) {
            bail!("the checkpoint's weak-subjectivity period has ended: fetch a new checkpoint");
        }
        let data = borsh::to_vec(&p.instruction)?;
        let slot = boot.header.beacon.slot;
        node.send(run, "Init (checkpoint, 512 keys adopted)", payer, program, &p.accounts, data, async || {
            let st = node.instance_state(&inst).await?.ok_or_else(|| anyhow!("Init: no instance after the transaction"))?;
            let rec = node.record_state(&inst, slot).await?;
            if !st.active || rec.is_none() {
                bail!("Init refused: the instance is not active or the checkpoint's record is missing");
            }
            Ok(format!("instance {} active, record of slot {slot}", hex::encode(inst.id)))
        })
        .await?;
    }
    s["instance"] = json!({"id": hex::encode(inst.id), "account": inst.account().to_string(), "checkpoint": hex::encode(inst.checkpoint), "checkpoint_slot": boot.header.beacon.slot});
    save_state(run, &s)?;
    Ok(())
}

fn p_window_end(p: &Planned) -> u64 {
    match &p.instruction {
        ferrata_v03_wire::Instruction::Init { window, .. } | ferrata_v03_wire::Instruction::Update { window, .. } => {
            window.to_ms.unwrap_or(u64::MAX)
        }
        _ => u64::MAX,
    }
}

async fn update(run: &Path, file: &Path) -> Result<()> {
    let node = Node::open()?;
    let s = state(run)?;
    let payer = account(&s["payer"])?;
    let program = program_of(&s)?;
    let mut inst = instance(run, program)?;
    let name = file.file_name().and_then(|n| n.to_str()).ok_or_else(|| anyhow!("bad file name"))?.to_owned();
    if applied(run).contains(&name) {
        println!("update {name}: already applied: nothing to do");
        return Ok(());
    }
    let u = load_update(file)?;
    let st = node.instance_state(&inst).await?.ok_or_else(|| anyhow!("no instance on chain: run `init` first"))?;
    let chain_store = CompactStore::decode(&st.store).map_err(|e| anyhow!("{e:?}"))?;
    if chain_store != CompactStore::of(inst.native.core()) {
        // The chain may already hold this update (an earlier run stopped before writing applied.txt).
        let mut next = inst.clone();
        next.follow(&u);
        if chain_store == CompactStore::of(next.native.core()) {
            append_applied(run, &name)?;
            println!("update {name}: the instance already holds it: recorded");
            return Ok(());
        }
        bail!("the instance's store is neither the native store nor the store after {name}");
    }
    let layout = inst.layout_for(inst.native.core().fork, &u);
    if inst.native_job(&u, layout).is_none() {
        let finalized = inst.native.core().finalized_header.beacon.slot;
        let sig_period = inst.spec.period_at_slot(u.signature_slot);
        let store_period = inst.spec.period_at_slot(finalized);
        if sig_period > store_period && !inst.native.core().is_next_sync_committee_known() {
            bail!("NEEDS-PERIOD-UPDATE {store_period}: {name} is signed in period {sig_period}; apply period {store_period}'s update first");
        }
        bail!("{name} is refused by the native light client (stale or invalid): not sent");
    }
    let (p, rest, record) = inst.update(&format!("update {name}"), &u, Verify::Residue { c: vec![], w: vec![] }, Signers::Core, false);
    let wait = (p.timestamp_ms - 12_000 + 2_000).saturating_sub(now_ms());
    if wait > 0 {
        tokio::time::sleep(Duration::from_millis(wait)).await;
    }
    if now_ms() >= p_window_end(&p) {
        bail!("{name}: the store's weak-subjectivity period has ended");
    }
    let mut expected = inst.clone();
    expected.follow(&u);
    let expected_store = CompactStore::of(expected.native.core());
    let check = async |done: bool| -> Result<String> {
        let st = node.instance_state(&inst).await?.ok_or_else(|| anyhow!("no instance"))?;
        let store = CompactStore::decode(&st.store).map_err(|e| anyhow!("{e:?}"))?;
        if done {
            if store != expected_store || st.job.is_some() {
                bail!("the update was refused: the instance's store is not the expected one");
            }
            Ok(format!("store finalized slot {} (root {})", store.finalized.slot, hex::encode(store.finalized.hash_tree_root())))
        } else {
            if st.job.is_none() {
                bail!("the update was refused: no job in progress");
            }
            Ok("job verified, waiting for its adoption".into())
        }
    };
    let one_tx = rest.is_empty();
    let label = if one_tx {
        format!("update {name}: Update (residue witness, ONE transaction)")
    } else {
        format!("update {name}: Update (residue witness)")
    };
    node.send(run, &label, payer, program, &p.accounts, borsh::to_vec(&p.instruction)?, async || check(one_tx).await).await?;
    if !one_tx {
        let c = inst.continue_with(
            &format!("update {name}: Continue (adopt 512 keys, apply)"),
            rest.into_iter().map(ferrata_v03_wire::Step::Adopt).collect(),
            record.as_ref(),
            now_ms(),
        );
        node.send(run, &c.label.clone(), payer, program, &c.accounts, borsh::to_vec(&c.instruction)?, async || check(true).await)
            .await?;
    }
    inst.follow(&u);
    append_applied(run, &name)?;
    Ok(())
}

fn append_applied(run: &Path, name: &str) -> Result<()> {
    let mut a = applied(run);
    a.push(name.to_owned());
    fs::write(run.join("applied.txt"), a.join("\n") + "\n")?;
    Ok(())
}

/// The record of the instance's finalized header on chain: its slot and the execution block it binds.
async fn finalized(run: &Path) -> Result<()> {
    let node = Node::open()?;
    let s = state(run)?;
    let inst = instance(run, program_of(&s)?)?;
    let slot = inst.native.core().finalized_header.beacon.slot;
    let rec = node.record_state(&inst, slot).await?.ok_or_else(|| anyhow!("no record at slot {slot}"))?;
    let recorded = HeaderRecord::decode(&rec.record).map_err(|e| anyhow!("{e:?}"))?;
    let hash = match recorded.execution {
        ExecutionSummary::BlockHash(h) => h,
        ExecutionSummary::Payload { block_hash, .. } => block_hash,
        ExecutionSummary::None => bail!("the record binds no execution block"),
    };
    println!("{slot} 0x{}", hex::encode(hash));
    Ok(())
}

async fn attest(run: &Path, proof: &Path, header: &Path) -> Result<()> {
    let node = Node::open()?;
    let s = state(run)?;
    let payer = account(&s["payer"])?;
    let program = program_of(&s)?;
    let inst = instance(run, program)?;
    let slot = inst.native.core().finalized_header.beacon.slot;
    let rec = node.record_state(&inst, slot).await?.ok_or_else(|| anyhow!("no record at slot {slot}"))?;
    let query = storage_query(&fs::read(proof)?, inst.config.chain_id, Some(fs::read(header)?), &[]);
    let statements = inst.statements(&rec, &query);
    let mut present = true;
    for st in &statements {
        present &= node.statement(&inst, st).await?.as_ref() == Some(st);
    }
    if present {
        println!("attest: every statement is already on chain: nothing to do");
        return Ok(());
    }
    let p = inst.attest(&format!("Attest {} slot(s) of 0x{} under slot {slot}", statements.len(), hex::encode(query.address)), slot, query, statements.clone(), now_ms());
    node.send(run, &p.label.clone(), payer, program, &p.accounts, borsh::to_vec(&p.instruction)?, async || {
        let mut out = vec![];
        for st in &statements {
            if node.statement(&inst, st).await?.as_ref() != Some(st) {
                bail!("the attestation was refused: statement {} is not on chain", hex::encode(st.id));
            }
            out.push(format!("slot {} = 0x{} at block {}", hex::encode(st.slot), hex::encode(st.value), st.block_number));
        }
        Ok(out.join("; "))
    })
    .await?;
    let mut done = s.clone();
    let mut list = done["attested"].as_array().cloned().unwrap_or_default();
    list.push(json!({"proof": proof.file_name().unwrap().to_string_lossy(), "slot": slot}));
    done["attested"] = json!(list);
    save_state(run, &done)?;
    Ok(())
}

/// Every verdict, read back from the sequencer.
async fn snapshot(run: &Path) -> Result<Value> {
    let node = Node::open()?;
    let s = state(run)?;
    let program = program_of(&s)?;
    let inst = instance(run, program)?;
    let header = ProgramHeader::from_bytes(&node.shard(program, PROGRAM_LOADER_ACCOUNT_ID).await?);
    let st = node.instance_state(&inst).await?;
    let mut records = vec![];
    let mut slots = vec![read_json(&run.join("chain.json"))?["bootstrap_slot"].as_u64().unwrap_or(0)];
    let mut replay = Instance::new(program, inst.config.clone(), inst.checkpoint, &bootstrap(run, &inst.config)?);
    for name in applied(run) {
        replay.follow(&load_update(&run.join("updates").join(&name))?);
        slots.push(replay.native.core().finalized_header.beacon.slot);
    }
    slots.dedup();
    for slot in slots {
        let r = node.record_state(&inst, slot).await?;
        records.push(json!({"slot": slot, "account": inst.header_account(slot).to_string(), "shard": r.map(|r| hex::encode(borsh::to_vec(&r).unwrap()))}));
    }
    let mut statements = vec![];
    let mut dirs: Vec<PathBuf> = fs::read_dir(run.join("proofs")).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    dirs.sort();
    for dir in dirs {
        let (Ok(proof), Ok(rlp)) = (fs::read(dir.join("proof.json")), fs::read(dir.join("header.rlp"))) else { continue };
        let Some(slot) = dir.file_name().and_then(|n| n.to_str()).and_then(|n| n.parse::<u64>().ok()) else { continue };
        let Some(rec) = node.record_state(&inst, slot).await? else { continue };
        let query = storage_query(&proof, inst.config.chain_id, Some(rlp), &[]);
        for claimed in inst.statements(&rec, &query) {
            let got = node.statement(&inst, &claimed).await?;
            statements.push(json!({
                "account": ferrata_v03_wire::statement_account(&program, &inst.id, &claimed.id_at_block).to_string(),
                "address": format!("0x{}", hex::encode(claimed.address)),
                "slot": format!("0x{}", hex::encode(claimed.slot)),
                "value": format!("0x{}", hex::encode(claimed.value)),
                "block_number": claimed.block_number,
                "statement_id": hex::encode(claimed.id),
                "shard": got.map(|g| hex::encode(borsh::to_vec(&g).unwrap())),
            }));
        }
    }
    let txs: Vec<Value> = fs::read_to_string(run.join("txs.tsv"))
        .unwrap_or_default()
        .lines()
        .skip(1)
        .map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            json!({"label": f[0], "tx_hash": f[1], "block": f[2].parse::<u64>().unwrap_or(0)})
        })
        .collect();
    let mut tx_blocks = vec![];
    for t in &txs {
        let hash: HashType = t["tx_hash"].as_str().unwrap().parse().map_err(|e| anyhow!("{e:?}"))?;
        let got = node.w.client_owned().get_transaction(hash).await.map_err(|e| anyhow!("getTransaction: {e}"))?;
        tx_blocks.push(json!({"label": t["label"], "tx_hash": t["tx_hash"], "block": got.map(|(_, b)| b)}));
    }
    Ok(json!({
        "sequencer": s["sequencer"],
        "program": program.to_string(),
        "image_id": header.map(|h| image_id_hex(h.image_id)),
        "instance": {
            "id": hex::encode(inst.id),
            "account": inst.account().to_string(),
            "shard": st.as_ref().map(|st| hex::encode(borsh::to_vec(st).unwrap())),
            "active": st.as_ref().map(|st| st.active),
            "applied": st.as_ref().map(|st| st.applied),
            "finalized_slot": st.as_ref().and_then(|st| CompactStore::decode(&st.store).ok()).map(|c| c.finalized.slot),
        },
        "records": records,
        "statements": statements,
        "transactions": tx_blocks,
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
    if bad > 0 {
        bail!("{bad} verdict(s) differ from {}", run.join("record.json").display());
    }
    println!("verify: every verdict matches {}", run.join("record.json").display());
    Ok(())
}

/// Every transaction of a run on RUN's data, through LEZ v0.3.1's state machine in this process
/// (the harness's chain: its testnet genesis, one payer, one transaction per block).
fn plan(run: &Path) -> Result<()> {
    let cfg = config(run)?;
    let boot = bootstrap(run, &cfg)?;
    let mut chain = Chain::new();
    let program = chain.deploy("plan", FERRATA_V03_ELF);
    let mut inst = Instance::new(program, cfg.clone(), checkpoint(run)?, &boot);
    let (p, _) = inst.init(&boot, usize::MAX);
    let r = inst.send(&mut chain, &p, PROGRAM_GAS_LIMIT);
    if !r.ok() {
        bail!("Init would be refused: {r:?}");
    }
    let mut names: Vec<String> = fs::read_dir(run.join("updates"))
        .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    names.sort();
    for name in names {
        let u = load_update(&run.join("updates").join(&name))?;
        let layout = inst.layout_for(inst.native.core().fork, &u);
        if inst.native_job(&u, layout).is_none() {
            println!("plan: {name} would not be sent (stale or signed by an unknown committee)");
            continue;
        }
        let (p, rest, record) = inst.update(&format!("update {name}"), &u, Verify::Residue { c: vec![], w: vec![] }, Signers::Core, false);
        let r = inst.send(&mut chain, &p, PROGRAM_GAS_LIMIT);
        if !r.ok() {
            bail!("{name} would be refused: {r:?}");
        }
        if !rest.is_empty() {
            let c = inst.continue_with(&format!("update {name}: Continue (adopt)"), rest.into_iter().map(ferrata_v03_wire::Step::Adopt).collect(), record.as_ref(), chain.last_timestamp + 1);
            assert!(inst.send(&mut chain, &c, PROGRAM_GAS_LIMIT).ok());
        }
        inst.follow(&u);
        let slot = inst.native.core().finalized_header.beacon.slot;
        let dir = run.join("proofs").join(slot.to_string());
        if let (Ok(proof), Ok(rlp)) = (fs::read(dir.join("proof.json")), fs::read(dir.join("header.rlp"))) {
            let rec = inst.record(&chain, slot).expect("a record");
            let query = storage_query(&proof, cfg.chain_id, Some(rlp), &[]);
            let statements = inst.statements(&rec, &query);
            let a = inst.attest(&format!("Attest {} slot(s) under slot {slot}", statements.len()), slot, query, statements, chain.last_timestamp + 1);
            let r = inst.send(&mut chain, &a, PROGRAM_GAS_LIMIT);
            if !r.ok() {
                bail!("the attestation would be refused: {r:?}");
            }
        }
    }
    println!("{}", TxReport::tsv_header());
    let mut total: u128 = 0;
    let mut reserve: u128 = 0;
    for r in &chain.reports {
        println!("{}", r.tsv());
        total += r.fee;
        let at_cap = u128::from(r.gas_limit) * u128::from(r.base_fee_exec) + u128::from(r.stor_gas) * u128::from(r.base_fee_stor);
        reserve = reserve.max(at_cap);
    }
    let heavy = chain.reports.iter().filter(|r| r.exec_gas > 0).count();
    println!(
        "plan: {} transactions ({} deployment, {} program), fees {} native units at the simulated base fees; the largest reserve {}; image {}",
        chain.reports.len(),
        chain.reports.len() - heavy,
        heavy,
        total,
        reserve,
        image_id_hex(FERRATA_V03_ID)
    );
    write_json(
        &run.join("plan.json"),
        &json!({
            "image_id": image_id_hex(FERRATA_V03_ID),
            "elf_bytes": FERRATA_V03_ELF.len(),
            "transactions": chain.reports.iter().map(|r| json!({"label": r.label, "bytes": r.stor_gas, "exec_gas": r.exec_gas, "gas_limit": r.gas_limit, "fee": r.fee.to_string(), "base_fee_exec": r.base_fee_exec})).collect::<Vec<_>>(),
            "fees_total": total.to_string(),
            "largest_reserve": reserve.to_string(),
            "simulated_utc": utc(now_ms()),
        }),
    )?;
    Ok(())
}
