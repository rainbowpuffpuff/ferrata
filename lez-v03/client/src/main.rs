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
use ferrata_v03_wire::{InstanceState, RecordState, Statement, Verify};
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
        ["finalized", run] => finalized(Path::new(run)),
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

/// Every step RUN's data allows after the ones already signed, built against the native light
/// client after the signed updates (each signed update is added to `applied.txt`).
fn new_steps(run: &Path, signed: &[Signed]) -> Result<Vec<Step>> {
    let s = read_json(&run.join("state.json"))?;
    let program = account(&s["program"])?;
    let segments: Vec<AccountId> = s["segments"].as_array().unwrap().iter().map(account).collect::<Result<_>>()?;
    let mut inst = instance(run, program)?;
    let cfg = inst.config.clone();
    let boot = bootstrap(run, &cfg)?;
    let done = |kind: &str| signed.iter().any(|x| x.kind == kind);
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
    if !done("header") {
        let binary = risc0_binfmt::ProgramBinary::decode(FERRATA_V03_ELF).map_err(|e| anyhow!("{e}"))?;
        let chunks: Vec<&[u8]> = binary.user_elf.chunks(program_loader_core::MAX_SEGMENT_DATA_LEN).collect();
        if chunks.len() != segments.len() {
            bail!("the program has {} segments, state.json names {}", chunks.len(), segments.len());
        }
        for i in (0..chunks.len()).rev() {
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
    if !done("init") {
        let (p, rest) = inst.init(&boot, usize::MAX);
        assert!(rest.is_empty());
        steps.push(call("Init (checkpoint, its 512 keys adopted)", "init", &p)?);
    }
    let mut names: Vec<String> = fs::read_dir(run.join("updates"))
        .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    names.sort();
    let mut newly = applied(run);
    for name in names {
        if newly.contains(&name) {
            continue;
        }
        let u = load_update(&run.join("updates").join(&name))?;
        let layout = inst.layout_for(inst.native.core().fork, &u);
        if inst.native_job(&u, layout).is_none() {
            eprintln!("queue: {name} is not newer than the store or is signed by a committee it does not know yet: skipped");
            continue;
        }
        let (p, rest, record) = inst.update(&name, &u, Verify::Residue { c: vec![], w: vec![] }, Signers::Core, false);
        let one = rest.is_empty();
        let label = if one { format!("update {name} (residue witness, one transaction)") } else { format!("update {name} (residue witness)") };
        steps.push(call(&label, &format!("update:{name}"), &p)?);
        if !one {
            let c = inst.continue_with(&name, rest.into_iter().map(ferrata_v03_wire::Step::Adopt).collect(), record.as_ref(), 0);
            steps.push(call(&format!("update {name}: Continue (adopt 512 keys, apply)"), &format!("continue:{name}"), &c)?);
        }
        inst.follow(&u);
        newly.push(name);
    }
    // The attestation: the vault's slots under the record of the finalized header after every update.
    let slot = inst.native.core().finalized_header.beacon.slot;
    let dir = run.join("proofs").join(slot.to_string());
    if !done(&format!("attest:{slot}")) {
        if let (Ok(proof), Ok(rlp)) = (fs::read(dir.join("proof.json")), fs::read(dir.join("header.rlp"))) {
            let record = HeaderRecord::of(&inst.native.core().finalized_header, inst.native.core().fork, &inst.spec);
            let rec = RecordState { instance_id: inst.id, chain_id: cfg.chain_id, record: record.encode() };
            let query = storage_query(&proof, cfg.chain_id, Some(rlp), &[]);
            let statements = inst.statements(&rec, &query);
            let p = inst.attest("attest", slot, query.clone(), statements.clone(), 0);
            steps.push(call(
                &format!("Attest {} slot(s) of 0x{} under slot {slot}", statements.len(), hex::encode(query.address)),
                &format!("attest:{slot}"),
                &p,
            )?);
        }
    }
    fs::write(run.join("applied.txt.next"), newly.join("\n") + "\n")?;
    Ok(steps)
}

fn sign(w: &WalletCore, payer: AccountId, step: &Step, nonces: &mut BTreeMap<String, u128>, gas_limit: u64) -> Result<PublicTransaction> {
    let mut keys = vec![];
    let mut ns = vec![];
    for id in step.signers.iter().chain(std::iter::once(&payer)) {
        keys.push(w.get_account_public_signing_key(*id).ok_or_else(|| anyhow!("the wallet holds no key of {id}"))?);
        let n = nonces.entry(id.to_string()).or_insert(0);
        ns.push(Nonce(*n));
        *n += 1;
    }
    let selectors = step.accounts.iter().map(|a| ProgramShardSelector::new(*a, step.program)).collect();
    let fee = FeeDeclaration::new(payer, gas_limit, 0, wallet::max_fee_for(gas_limit));
    let message = Message::new_preserialized(step.program, selectors, ns, step.data.clone(), Some(fee));
    let witness = WitnessSet::for_message(&message, &keys);
    Ok(PublicTransaction::new(message, witness))
}

/// The whole signed sequence on a replica of the chain, as LEZ v0.3.1's builder groups it.
fn simulate(payer: AccountId, balance: u128, start: &BTreeMap<String, u128>, txs: &[(String, PublicTransaction, u64)]) -> Vec<TxReport> {
    let mut extra = vec![];
    for (id, n) in start {
        let a: AccountId = id.parse().unwrap();
        let mut acc = if a == payer { Account::funded(balance) } else { Account::default() };
        acc.nonce = Nonce(*n);
        extra.push((a, acc));
    }
    let mut chain = Chain::with_accounts(extra);
    chain.settle_queued(txs, now_ms())
}

async fn queue(run: &Path, dry_run: bool) -> Result<()> {
    let w = WalletCore::from_env()?;
    let client = w.client_owned();
    let s = read_json(&run.join("state.json"))?;
    let payer = account(&s["payer"])?;
    let program = account(&s["program"])?;
    fs::write(run.join("ferrata_v03.bin"), FERRATA_V03_ELF)?;
    let mut signed = load_signed(run)?;
    let steps = new_steps(run, &signed)?;
    if steps.is_empty() {
        println!("queue: nothing new to sign ({} signed)", signed.len());
        return Ok(());
    }
    // Nonces: where the signed sequence ends, or the chain's now.
    let mut nonces: BTreeMap<String, u128> = serde_json::from_value(read_or_empty(&run.join("nonces.json"))?["next"].clone()).unwrap_or_default();
    let mut start: BTreeMap<String, u128> = serde_json::from_value(read_or_empty(&run.join("nonces.json"))?["initial"].clone()).unwrap_or_default();
    for id in [payer, program] {
        if !nonces.contains_key(&id.to_string()) {
            let n = client.get_accounts_nonces(vec![id]).await.map(|v| v[0].0).unwrap_or(0);
            nonces.insert(id.to_string(), n);
            start.entry(id.to_string()).or_insert(n);
        }
    }
    let balance = client.get_account_balance(payer).await.unwrap_or(0);
    let sim_balance = if balance > 0 { balance } else { 1_000_000_000_000 };
    let previous: Vec<(String, PublicTransaction, u64)> = signed.iter().map(|x| (x.label.clone(), x.tx.clone(), x.gas_limit)).collect();
    // Pass 1: every new step at the cap, to measure it.
    let mut n1 = nonces.clone();
    let mut probe = previous.clone();
    for st in &steps {
        let limit = if st.loader { LOADER_GAS } else { MAX_GAS_EXEC };
        probe.push((st.label.clone(), sign(&w, payer, st, &mut n1, limit)?, limit));
    }
    let measured = simulate(payer, sim_balance, &start, &probe);
    // Pass 2: the declared limits, then the sequence as the builder will group it.
    let mut n2 = nonces.clone();
    let mut fresh = vec![];
    for (k, st) in steps.iter().enumerate() {
        let m = &measured[previous.len() + k];
        if m.status != "applied" {
            bail!("{} would not apply in the simulation: {} {:?}", st.label, m.status, m.exit);
        }
        let limit = if st.loader { LOADER_GAS } else { declared(m.exec_gas) };
        fresh.push((st.label.clone(), sign(&w, payer, st, &mut n2, limit)?, limit));
    }
    let mut all = previous.clone();
    all.extend(fresh.iter().cloned());
    let sim = simulate(payer, sim_balance, &start, &all);
    let base = signed.len();
    for (k, (st, (label, tx, limit))) in steps.iter().zip(&fresh).enumerate() {
        let r = &sim[base + k];
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
    // The simulated blocks, relative to the first block of the run.
    let first = sim.iter().map(|r| r.block).filter(|b| *b > 0).min().unwrap_or(0);
    println!("{}", "seq\tsim. block\tlabel\tbytes\tdeclared gas\texec gas (simulated)\tof 10M\tfee (simulated, base fee)\ttx hash");
    for x in &signed {
        let r = &sim[x.seq];
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{:.1}%\t{} ({})\t{}",
            x.seq,
            r.block.saturating_sub(first) + 1,
            x.label,
            x.bytes,
            x.gas_limit,
            x.sim_exec,
            x.sim_exec as f64 / 1e5,
            x.sim_fee,
            r.base_fee_exec,
            x.hash
        );
    }
    let total: u128 = sim.iter().map(|r| r.fee).sum();
    let reserve = signed.iter().map(|x| u128::from(x.gas_limit) * 8 + x.bytes as u128 * 8).max().unwrap_or(0);
    println!(
        "queue: {} transactions in {} blocks (simulated), fees {} native units at the simulated base fees, the largest reserve {} at base fee 8; payer {} holds {}",
        signed.len(),
        sim.iter().map(|r| r.block).filter(|b| *b > 0).collect::<std::collections::BTreeSet<_>>().len(),
        total,
        reserve,
        payer,
        balance
    );
    // The batch file of testnet-ops's queued sender (the same instructions; it signs with its own nonces).
    let mut batch_steps = vec![];
    let mut deploy_done = false;
    for st in &steps {
        if st.loader {
            if !deploy_done && st.kind == "header" {
                batch_steps.push(st.batch.clone());
                deploy_done = true;
            }
            continue;
        }
        let x = signed.iter().find(|x| x.kind == st.kind).unwrap();
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
                    "largest_reserve_at_base_fee_8": reserve.to_string(), "simulated_utc": utc(now_ms()),
                    "transactions": signed.iter().map(|x| json!({"label": x.label, "bytes": x.bytes, "declared_gas": x.gas_limit,
                        "exec_gas": x.sim_exec, "block": sim[x.seq].block.saturating_sub(first) + 1, "fee": x.sim_fee.to_string()})).collect::<Vec<_>>()}),
        )?;
        println!("queue --dry-run: nothing signed was kept, nothing sent");
        return Ok(());
    }
    // Keep everything before sending anything.
    save_signed(run, &signed)?;
    fs::rename(run.join("applied.txt.next"), run.join("applied.txt"))?;
    write_json(&run.join("nonces.json"), &json!({"initial": start, "next": n2}))?;
    for (label, tx, _) in &fresh {
        let hash = client
            .send_transaction(LeeTransaction::Public(tx.clone()))
            .await
            .map_err(|e| anyhow!("sendTransaction ({label}): {e}"))?;
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

/// What each signed transaction should have done, read from state.
async fn verdict(client: &sequencer_service_rpc::SequencerClient, run: &Path, inst: &Instance, x: &Signed) -> Result<String> {
    let get = async |id: AccountId, program: AccountId| -> Result<Vec<u8>> {
        Ok(client.get_account(id).await.map_err(|e| anyhow!("getAccount: {e}"))?.data.shard(program).to_vec())
    };
    let s = read_json(&run.join("state.json"))?;
    let kind = x.kind.as_str();
    Ok(if let Some(i) = kind.strip_prefix("segment:") {
        let seg = account(&s["segments"][i.parse::<usize>()?])?;
        if get(seg, PROGRAM_LOADER_ACCOUNT_ID).await?.is_empty() { "MISSING segment".into() } else { "segment written".into() }
    } else if kind == "header" {
        match ProgramHeader::from_bytes(&get(inst.program, PROGRAM_LOADER_ACCOUNT_ID).await?) {
            Some(h) if h.image_id == FERRATA_V03_ID => format!("program at {}, image {}", inst.program, image_id_hex(h.image_id)),
            other => format!("MISSING program header ({other:?})"),
        }
    } else if kind == "init" {
        let shard = get(inst.account(), inst.program).await?;
        let slot = read_json(&run.join("chain.json"))?["bootstrap_slot"].as_u64().unwrap_or(0);
        let rec = get(inst.header_account(slot), inst.program).await?;
        match (shard.is_empty(), rec.is_empty()) {
            (false, false) => format!("instance {} created, record of the checkpoint (slot {slot})", hex::encode(&inst.id[..8])),
            _ => "MISSING instance or checkpoint record".into(),
        }
    } else if let Some(name) = kind.strip_prefix("update:").or(kind.strip_prefix("continue:")) {
        let u = load_update(&run.join("updates").join(name))?;
        let slot = u.finalized_header.beacon.slot;
        let rec = get(inst.header_account(slot), inst.program).await?;
        if rec.is_empty() {
            format!("MISSING record of slot {slot}")
        } else {
            format!("record of finalized slot {slot} written")
        }
    } else if let Some(slot) = kind.strip_prefix("attest:") {
        let dir = run.join("proofs").join(slot);
        let rec: RecordState = borsh::from_slice(&get(inst.header_account(slot.parse()?), inst.program).await?)?;
        let query = storage_query(&fs::read(dir.join("proof.json"))?, inst.config.chain_id, Some(fs::read(dir.join("header.rlp"))?), &[]);
        let mut out = vec![];
        let mut block = 0;
        for st in inst.statements(&rec, &query) {
            block = st.block_number;
            let id = ferrata_v03_wire::statement_account(&inst.program, &inst.id, &st.id_at_block);
            let got = get(id, inst.program).await?;
            let ok = !got.is_empty() && borsh::from_slice::<Statement>(&got).ok().as_ref() == Some(&st);
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
    let program = program_of(run)?;
    let inst = instance(run, program)?;
    let signed = load_signed(run)?;
    let payer = account(&read_json(&run.join("state.json"))?["payer"])?;
    let last = signed.iter().map(|x| x.payer_nonce).max().ok_or_else(|| anyhow!("nothing signed"))?;
    let t0 = Instant::now();
    let mut head = client.get_last_block_id().await.unwrap_or(0);
    let mut stalled_since = head;
    loop {
        let nonce = client.get_accounts_nonces(vec![payer]).await.map_err(|e| anyhow!("getAccountsNonces: {e}"))?[0].0;
        if nonce > last {
            break;
        }
        if t0.elapsed() > Duration::from_secs(secs) {
            bail!("PENDING: the payer's nonce is {nonce}, the last signed is {last}; run `resend` and `wait` again later");
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
        let now = client.get_last_block_id().await.unwrap_or(head);
        // Two blocks without our next transaction: it may have been dropped behind a rewound one.
        if now >= stalled_since + 2 && client.get_accounts_nonces(vec![payer]).await.map(|v| v[0].0).unwrap_or(nonce) == nonce {
            resend(run).await?;
            stalled_since = now;
        }
        if now != head {
            head = now;
        }
    }
    let mut lines = vec!["seq\tlabel\ttx_hash\tblock\tbedrock_status\tbytes\tdeclared_gas\texec_gas_simulated\tverdict".to_owned()];
    let mut missing = 0;
    for x in &signed {
        let hash: HashType = x.hash.parse().map_err(|e| anyhow!("{e:?}"))?;
        let (block, status) = match client.get_transaction(hash).await.map_err(|e| anyhow!("getTransaction: {e}"))? {
            Some((_, b)) => {
                let st = client.get_block(b).await.ok().flatten().map(|blk| format!("{:?}", blk.bedrock_status)).unwrap_or_default();
                (b.to_string(), st)
            }
            None => ("-".into(), "-".into()),
        };
        let v = verdict(&client, run, &inst, x).await?;
        if v.contains("MISSING") || block == "-" {
            missing += 1;
        }
        println!("{} block {block} ({status}): {} — {v}", x.seq, x.label);
        lines.push(format!("{}\t{}\t{}\t{block}\t{status}\t{}\t{}\t{}\t{v}", x.seq, x.label, x.hash, x.bytes, x.gas_limit, x.sim_exec));
    }
    fs::write(run.join("txs.tsv"), lines.join("\n") + "\n")?;
    if missing > 0 {
        bail!("{missing} transaction(s) without their effect in state");
    }
    println!("wait: every signed transaction is in a block and did what it should ({:.0} s)", t0.elapsed().as_secs_f64());
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

/// The finalized slot after every signed update and the execution block its header binds.
fn finalized(run: &Path) -> Result<()> {
    let inst = instance(run, program_of(run)?)?;
    let core = inst.native.core();
    let record = HeaderRecord::of(&core.finalized_header, core.fork, &inst.spec);
    let hash = match record.execution {
        ExecutionSummary::BlockHash(h) => h,
        ExecutionSummary::Payload { block_hash, .. } => block_hash,
        ExecutionSummary::None => bail!("the record binds no execution block"),
    };
    println!("{} 0x{}", core.finalized_header.beacon.slot, hex::encode(hash));
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
    if bad > 0 {
        bail!("{bad} verdict(s) differ from {}", run.join("record.json").display());
    }
    println!("verify: every verdict matches {}", run.join("record.json").display());
    Ok(())
}
