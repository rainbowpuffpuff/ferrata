//! One session of the lab: the program deployed once, then for each chain (Sepolia, mainnet) an
//! instance initialized once from its bootstrap and kept following its chain with the newest
//! light-client updates, each as a job over the public path (`Open`, `Miller`, the adoption
//! parts if it hands the committee over, `Finish`), next to the native light client.
//!
//! State kept between sessions, under `--run-dir` (the Modal Volume holds it between runs):
//! the live driver's follower, block log and manifest (`blocks.borsh`, `follower.borsh`,
//! `run.tsv`), the wallet, and per chain `net/<chain>/`: the bootstrap and checkpoint the
//! instance was initialized from, every update it applied (`updates/NNNN.<layout>.ssz`) and
//! `jobs.tsv`. The native store is rebuilt from those at every start and must equal the
//! instance's store as the sequencer serves it; nothing else is trusted from a previous run.
//!
//! Inputs of a session (`--inputs`, written by `../fetch.py` from public beacon endpoints):
//! `<chain>/bootstrap.ssz` and `<chain>/checkpoint` (only used before the instance exists),
//! `<chain>/updates/*.ssz` (the `updates` endpoint, one file per period) and
//! `<chain>/finality_update.ssz`.

use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, bail, ensure, Context as _, Result};
use ferrata_core::{
    fork::LcFork,
    merkle::ZERO,
    policy::{Adoption, FinalizedStore},
    steps,
    store::{ApplyOutcome, StoreCore},
    types::{LightClientBootstrap, LightClientHeader, LightClientUpdate, SyncAggregate, SyncCommittee},
    validate::Context as CoreContext,
    witness::{uncompressed_keys, CompactBootstrap, CompactStore, CompactUpdate, HeaderRecord},
    Root,
};
use ferrata_lez_wire::{job_id, Config, InstanceState, Message, RecordState};
use lee::{AccountId, Actor};
use lee_core::program::{ProgramHeader, PROGRAM_LOADER_ACCOUNT_ID};
use wallet::program_facades::program_loader::ProgramLoader;

use crate::{
    keys::{account_of, derived_key},
    live::{Live, Outcome, Tx},
    manifest::Row,
    plan::{
        exit_name, mainnet_config, plan_job, root, sepolia_config, signer_witness, Instance, Pda, BLS,
        SEPOLIA_CHECKPOINT,
    },
    programs::{image_id_hex, GuestBinary},
};

/// The name the lab's deployment of the program is derived from (its accounts and keys).
pub const DEPLOYMENT: &str = "lab";
/// Adoption of a committee in two transactions, as in `../../RESULTS.md` §6.
pub const PARTS: u8 = 2;
/// The gas limit declared for a job step (the largest measured is 3,731,712; LEZ's wallet
/// default of 2,000,000 is too low, `../../live/README.md`).
pub const JOB_GAS_LIMIT: u64 = 4_800_000;

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// The two chains the lab follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chain {
    Sepolia,
    Mainnet,
}

impl Chain {
    pub const ALL: [Chain; 2] = [Chain::Sepolia, Chain::Mainnet];

    pub fn name(self) -> &'static str {
        match self {
            Chain::Sepolia => "sepolia",
            Chain::Mainnet => "mainnet",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        Self::ALL.into_iter().find(|c| c.name() == s).ok_or_else(|| anyhow!("unknown chain {s}"))
    }

    pub fn config(self) -> Config {
        match self {
            Chain::Sepolia => sepolia_config(PARTS),
            Chain::Mainnet => mainnet_config(PARTS),
        }
    }

    /// The payer of this chain's jobs: one of LEZ's published test accounts each.
    pub fn payer_index(self) -> usize {
        match self {
            Chain::Sepolia => 0,
            Chain::Mainnet => 1,
        }
    }
}

// ---- Deployment -------------------------------------------------------------------------------

pub fn header_key_label(name: &str) -> String {
    format!("program/{name}/header")
}

pub fn segment_key_label(name: &str, index: usize) -> String {
    format!("program/{name}/segment/{index}")
}

/// Where a deployment of `binary` named `name` lives: its header account and segment accounts.
pub fn deployment_accounts(name: &str, binary: &GuestBinary) -> (AccountId, Vec<AccountId>) {
    let header = account_of(&derived_key(&header_key_label(name)));
    let segments =
        (0..binary.chunks().len()).map(|i| account_of(&derived_key(&segment_key_label(name, i)))).collect();
    (header, segments)
}

/// Every key a deployment signs with (throwaway keys derived from public labels).
pub fn deployment_keys(name: &str, binary: &GuestBinary) -> Vec<lee::PrivateKey> {
    let mut keys = vec![derived_key(&header_key_label(name))];
    keys.extend((0..binary.chunks().len()).map(|i| derived_key(&segment_key_label(name, i))));
    keys
}

pub async fn deployed_header(live: &Live, header: AccountId) -> Result<Option<ProgramHeader>> {
    let state = live.actor_state(Actor::new(header, PROGRAM_LOADER_ACCOUNT_ID)).await?;
    Ok(ProgramHeader::from_bytes(&state))
}

/// Deploy `binary` once, as the live driver does (`ProgramLoader::deploy`, immutable), and
/// record the loader's transactions from the blocks once they are final.
pub async fn deploy(live: &mut Live, binary: &GuestBinary, payer: AccountId) -> Result<AccountId> {
    let name = DEPLOYMENT;
    let (header, segments) = deployment_accounts(name, binary);
    if let Some(existing) = deployed_header(live, header).await? {
        ensure!(existing.image_id == binary.image_id, "{header} already holds another image");
        println!("[deploy] already deployed at {header}: image {}", binary.image_hex());
        return Ok(header);
    }
    live.sync().await?;
    let from = live.follower.height();
    let balance_before = live.balance(payer).await?;
    live.set_gas_limit(wallet::DEFAULT_GAS_LIMIT);
    let returned = ProgramLoader(&live.wallet).deploy(header, &segments, binary.bytes.clone(), true, Some(payer)).await?;
    ensure!(returned == header, "the loader facade returned another header account");
    let started = std::time::Instant::now();
    loop {
        live.sync().await?;
        let followed = live
            .follower
            .blocks
            .iter()
            .filter(|b| b.header.block_id > from)
            .flat_map(|b| b.body.transactions.iter())
            .filter_map(|tx| tx.public_call())
            .filter(|(to, _)| {
                to.program_account_id == PROGRAM_LOADER_ACCOUNT_ID
                    && (to.account_id == header || segments.contains(&to.account_id))
            })
            .count();
        if followed == segments.len() + 1 {
            break;
        }
        ensure!(started.elapsed() < crate::live::SETTLE_TIMEOUT, "{followed} of {} loader transactions final", segments.len() + 1);
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    let balance_after = live.balance(payer).await?;
    let mut rows = Vec::new();
    for block in live.follower.blocks.iter().filter(|b| b.header.block_id > from) {
        for tx in &block.body.transactions {
            let Some((to, _)) = tx.public_call() else { continue };
            if to.program_account_id != PROGRAM_LOADER_ACCOUNT_ID {
                continue;
            }
            let step = if to.account_id == header {
                format!("deploy {name}: CreateHeader (immutable) at {header}")
            } else if let Some(i) = segments.iter().position(|s| *s == to.account_id) {
                format!("deploy {name}: WriteSegment {} of {} ({} bytes)", i + 1, segments.len(), binary.chunks()[i].len())
            } else {
                continue;
            };
            let hash = tx.hash();
            let receipt = live.follower.receipt(&hash).ok_or_else(|| anyhow!("no receipt for {hash}"))?;
            rows.push(
                Row { phase: live.phase.clone(), step, tx_hash: hash.to_string(), ..Row::default() }
                    .with_receipt(receipt, exit_name),
            );
        }
    }
    ensure!(rows.len() == segments.len() + 1, "expected {} loader transactions, found {}", segments.len() + 1, rows.len());
    ensure!(rows.iter().all(|r| r.status == "applied"), "a loader transaction reverted");
    if let Some(last) = rows.last_mut() {
        last.payer_debit = balance_before.saturating_sub(balance_after).to_string();
        last.note = "payer_debit is for the whole deployment".to_string();
    }
    live.rows.extend(rows);
    let on_chain = deployed_header(live, header).await?.ok_or_else(|| anyhow!("no header at {header}"))?;
    ensure!(
        on_chain.image_id == binary.image_id,
        "the loader computed image {}, the binary is {}",
        image_id_hex(on_chain.image_id),
        binary.image_hex()
    );
    println!("[deploy] image {} at {header} (computed by the sequencer's loader), immutable", binary.image_hex());
    live.save()?;
    Ok(header)
}

// ---- Decoding what the beacon endpoints serve -------------------------------------------------

/// A `LightClientUpdate` as served (one period of the `updates` stream), in the layout of the
/// fork its attested header falls in. Each candidate layout is tried; the one accepted must be
/// the layout the schedule puts at the decoded attested slot.
pub fn decode_update(bytes: &[u8], config: &Config) -> Result<(LightClientUpdate, LcFork)> {
    let spec = config.spec();
    for layout in [LcFork::Electra, LcFork::Gloas] {
        if let Ok(update) = LightClientUpdate::decode(bytes, layout, 512) {
            if crate::plan::served_layout(&spec, update.attested_header.beacon.slot) == layout {
                return Ok((update, layout));
            }
        }
    }
    bail!("an update of {} bytes in no layout this chain's schedule allows", bytes.len())
}

/// A `LightClientFinalityUpdate` as the `LightClientUpdate` it stands for (empty committee
/// fields, as the spec's `process_light_client_finality_update` does): Electra (offsets) or
/// Gloas (fixed size) layout, the one the schedule puts at its attested slot.
pub fn decode_finality_update(b: &[u8], config: &Config) -> Result<(LightClientUpdate, LcFork)> {
    let spec = config.spec();
    let u64_at = |o: usize| u64::from_le_bytes(b[o..o + 8].try_into().expect("8 bytes"));
    // Gloas: every field fixed-size (`harness::sepolia::gloas_finality_update`).
    const H: usize = 112 + 32 + 11 * 32;
    if b.len() == 2 * H + 9 * 32 + 160 + 8 {
        let update = LightClientUpdate {
            attested_header: LightClientHeader::decode(&b[..H], LcFork::Gloas).map_err(|e| anyhow!("{e:?}"))?,
            next_sync_committee: SyncCommittee::empty(512),
            next_sync_committee_branch: vec![ZERO; LcFork::Gloas.sync_committee_branch_len()],
            finalized_header: LightClientHeader::decode(&b[H..2 * H], LcFork::Gloas).map_err(|e| anyhow!("{e:?}"))?,
            finality_branch: b[2 * H..2 * H + 288].chunks(32).map(|c| c.try_into().expect("32")).collect(),
            sync_aggregate: SyncAggregate::decode(&b[2 * H + 288..2 * H + 448], 512).map_err(|e| anyhow!("{e:?}"))?,
            signature_slot: u64_at(2 * H + 448),
        };
        ensure!(crate::plan::served_layout(&spec, update.attested_header.beacon.slot) == LcFork::Gloas, "a Gloas finality update before Gloas");
        return Ok((update, LcFork::Gloas));
    }
    // Electra: two header offsets (`harness::attest::finality_update`).
    ensure!(b.len() > 400, "a finality update of {} bytes", b.len());
    let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().expect("4 bytes")) as usize;
    let (att, fin) = (u32_at(0), u32_at(4));
    ensure!(att == 400 && fin > att && fin < b.len(), "LightClientFinalityUpdate offsets {att}, {fin}");
    let update = LightClientUpdate {
        attested_header: LightClientHeader::decode(&b[att..fin], LcFork::Electra).map_err(|e| anyhow!("{e:?}"))?,
        next_sync_committee: SyncCommittee::empty(512),
        next_sync_committee_branch: vec![ZERO; LcFork::Electra.sync_committee_branch_len()],
        finalized_header: LightClientHeader::decode(&b[fin..], LcFork::Electra).map_err(|e| anyhow!("{e:?}"))?,
        finality_branch: b[8..232].chunks(32).map(|c| c.try_into().expect("32")).collect(),
        sync_aggregate: SyncAggregate::decode(&b[232..392], 512).map_err(|e| anyhow!("{e:?}"))?,
        signature_slot: u64_at(392),
    };
    ensure!(crate::plan::served_layout(&spec, update.attested_header.beacon.slot) == LcFork::Electra, "an Electra finality update under Gloas");
    Ok((update, LcFork::Electra))
}

fn layout_name(f: LcFork) -> &'static str {
    match f {
        LcFork::Altair => "altair",
        LcFork::Capella => "capella",
        LcFork::Deneb => "deneb",
        LcFork::Electra => "electra",
        LcFork::Gloas => "gloas",
    }
}

fn layout_of(name: &str) -> Result<LcFork> {
    Ok(match name {
        "electra" => LcFork::Electra,
        "gloas" => LcFork::Gloas,
        "deneb" => LcFork::Deneb,
        "capella" => LcFork::Capella,
        "altair" => LcFork::Altair,
        other => bail!("unknown layout {other}"),
    })
}

// ---- One chain's instance ---------------------------------------------------------------------

/// What the lab keeps for one chain's instance, and the native light client rebuilt from it.
pub struct Follow {
    pub chain: Chain,
    pub dir: PathBuf,
    pub inst: Instance,
    pub bootstrap: LightClientBootstrap,
    pub native: FinalizedStore,
    /// (file stem, update, layout it was sent in) of every applied update, in order.
    pub applied: Vec<(String, LightClientUpdate, LcFork)>,
}

pub const JOBS_HEADER: &str = "session\tchain\tseq\tkind\tperiod\tattested_slot\tsignature_slot\tfinalized_slot\tlayout\trotated\tadvanced\tadopted\tjob_id\ttransactions\tfirst_block\tlast_block\tstatus\tnote\n";

impl Follow {
    fn ctx<'a>(&self, spec: &'a ferrata_core::fork::ChainSpec) -> CoreContext<'a> {
        CoreContext { spec, genesis_validators_root: self.inst.config.genesis_validators_root, bls: &BLS }
    }

    /// The kept state of `chain`, if its instance was initialized; the native store replayed
    /// from the bootstrap through every applied update.
    pub fn load(run_dir: &Path, chain: Chain, program: AccountId) -> Result<Option<Self>> {
        let dir = run_dir.join("net").join(chain.name());
        let (Ok(bootstrap), Ok(checkpoint)) = (fs::read(dir.join("bootstrap.ssz")), fs::read_to_string(dir.join("checkpoint")))
        else {
            return Ok(None);
        };
        let config = chain.config();
        let checkpoint = root(checkpoint.trim());
        let bootstrap = LightClientBootstrap::decode(&bootstrap, config.fork().expect("layout"), 512)
            .map_err(|e| anyhow!("{}: bootstrap: {e:?}", chain.name()))?;
        let inst = Instance::new(chain.name(), program, config, checkpoint);
        let spec = inst.config.spec();
        let native = FinalizedStore::initialize_with(&checkpoint, &bootstrap, inst.fork(), &spec, &BLS, Adoption::OnChain)
            .map_err(|e| anyhow!("{}: the native store refuses the bootstrap: {e:?}", chain.name()))?;
        let mut follow = Self { chain, dir: dir.clone(), inst, bootstrap, native, applied: Vec::new() };
        let mut names: Vec<String> = match fs::read_dir(dir.join("updates")) {
            Ok(entries) => entries.filter_map(|e| e.ok()?.file_name().into_string().ok()).filter(|n| n.ends_with(".ssz")).collect(),
            Err(_) => Vec::new(),
        };
        names.sort();
        for name in names {
            // NNNNN.<kind>.<layout sent in>.ssz, the bytes as the endpoint served them.
            let stem = name.trim_end_matches(".ssz").to_string();
            let parts: Vec<&str> = stem.split('.').collect();
            ensure!(parts.len() == 3, "{name}: not NNNNN.kind.layout.ssz");
            let layout = layout_of(parts[2])?;
            let bytes = fs::read(dir.join("updates").join(&name))?;
            let (update, _) = match parts[1] {
                "finality" => decode_finality_update(&bytes, &follow.inst.config)?,
                _ => decode_update(&bytes, &follow.inst.config)?,
            };
            follow.replay_one(&update, layout).with_context(|| format!("{}: replaying {name}", chain.name()))?;
            follow.applied.push((stem, update, layout));
        }
        Ok(Some(follow))
    }

    fn replay_one(&mut self, update: &LightClientUpdate, layout: LcFork) -> Result<ApplyOutcome> {
        let spec = self.inst.config.spec();
        let mut next = self.native.clone();
        if layout > next.core().fork {
            next.upgrade(layout, &spec).map_err(|e| anyhow!("upgrade: {e:?}"))?;
        }
        let outcome = next.process_update(update, update.signature_slot + 1, &self.ctx(&spec)).map_err(|e| anyhow!("{e:?}"))?;
        self.native = next;
        Ok(outcome)
    }

    /// The instance's state as the sequencer serves it (checked against the follower's).
    pub async fn state(&self, live: &Live) -> Result<Option<InstanceState>> {
        let s = live.actor_state(self.inst.pda().actor()).await?;
        if s.is_empty() {
            return Ok(None);
        }
        Ok(Some(borsh::from_slice(&s)?))
    }

    pub async fn store(&self, live: &Live) -> Result<CompactStore> {
        let state = self.state(live).await?.ok_or_else(|| anyhow!("{}: no instance", self.chain.name()))?;
        CompactStore::decode(&state.store).map_err(|e| anyhow!("{}: store: {e:?}", self.chain.name()))
    }

    /// The instance holds the compact form of the native store, and the record of its finalized
    /// header.
    pub async fn check(&self, live: &Live) -> Result<()> {
        let store = self.store(live).await?;
        ensure!(store == CompactStore::of(self.native.core()), "{}: the instance and the native store differ", self.chain.name());
        let core = self.native.core();
        let pda = self.inst.header(core.finalized_header.beacon.slot);
        let s = live.actor_state(pda.actor()).await?;
        ensure!(!s.is_empty(), "{}: no record of finalized slot {}", self.chain.name(), core.finalized_header.beacon.slot);
        let record: RecordState = borsh::from_slice(&s)?;
        let spec = self.inst.config.spec();
        ensure!(record.instance_id == self.inst.id && record.chain_id == self.inst.config.chain_id, "{}: the record's instance", self.chain.name());
        ensure!(
            record.record == HeaderRecord::of(&core.finalized_header, core.fork, &spec).encode(),
            "{}: the record is not the native finalized header's",
            self.chain.name()
        );
        Ok(())
    }

    fn append_job(&self, line: &str) -> Result<()> {
        let path = self.dir.join("jobs.tsv");
        let mut text = fs::read_to_string(&path).unwrap_or_else(|_| JOBS_HEADER.to_string());
        text.push_str(line);
        text.push('\n');
        fs::write(path, text)?;
        Ok(())
    }
}

/// The adoption transactions of a job. The last one also reaches the instance and, if it applies
/// the job (an initialization), the header record at `record_slot`.
fn adopt_txs(inst: &Instance, label: &str, job_pda: Pda, keys: &[[u8; 96]], record_slot: Option<u64>, payer: AccountId) -> Result<Vec<Tx>> {
    let parts = inst.config.adoption_parts;
    let chunk = keys.len() / parts as usize;
    let mut txs = Vec::new();
    for p in 0..parts {
        let mut actors = vec![job_pda];
        if p + 1 == parts {
            if let Some(slot) = record_slot {
                actors.extend([inst.pda(), inst.header(slot)]);
            }
        }
        let part = keys[p as usize * chunk..(p as usize + 1) * chunk].to_vec();
        txs.push(tx(format!("{label} adopt {}/{parts}", p + 1), actors, borsh::to_vec(&Message::Adopt { part: p, keys: part })?, payer));
    }
    Ok(txs)
}

fn tx(step: String, actors: Vec<Pda>, message: Vec<u8>, payer: AccountId) -> Tx {
    Tx { step, harness: None, actors, root: 0, message, payer, gas_limit: Some(JOB_GAS_LIMIT), note: String::new() }
}

/// Blocks of the outcomes (first, last) and whether every transaction applied.
fn summary_of(outcomes: &[Outcome]) -> (String, String, bool) {
    let blocks: Vec<&String> = outcomes.iter().map(|o| &o.row.block).filter(|b| !b.is_empty()).collect();
    (
        blocks.first().map(|s| s.to_string()).unwrap_or_default(),
        blocks.last().map(|s| s.to_string()).unwrap_or_default(),
        outcomes.iter().all(Outcome::applied),
    )
}

/// `Init` from `inputs/<chain>/bootstrap.ssz` at the checkpoint `inputs/<chain>/checkpoint`;
/// then the kept state starts.
pub async fn init(live: &mut Live, chain: Chain, program: AccountId, inputs: &Path, payer: AccountId, session: &str) -> Result<Follow> {
    let config = chain.config();
    let fork = config.fork().expect("layout");
    let bytes = fs::read(inputs.join(chain.name()).join("bootstrap.ssz")).with_context(|| format!("{}: no bootstrap in the inputs", chain.name()))?;
    let checkpoint_hex = fs::read_to_string(inputs.join(chain.name()).join("checkpoint"))?.trim().to_string();
    if chain == Chain::Sepolia {
        ensure!(checkpoint_hex == SEPOLIA_CHECKPOINT, "sepolia: the bootstrap is to be the period-1378 one");
    }
    let checkpoint = root(&checkpoint_hex);
    let bootstrap = LightClientBootstrap::decode(&bytes, fork, 512).map_err(|e| anyhow!("{}: bootstrap: {e:?}", chain.name()))?;
    let inst = Instance::new(chain.name(), program, config, checkpoint);
    let spec = inst.config.spec();
    let compact = CompactBootstrap::of(&bootstrap);
    let (store, job) = steps::initialize(&checkpoint, &compact, fork, &spec, PARTS).map_err(|e| anyhow!("the bootstrap does not verify: {e:?}"))?;
    let jid = job_id(&inst.id, &job.store_digest, &job.update_digest);
    let job_pda = inst.job(&jid);
    let label = format!("{} Init", chain.name());
    let mut txs = vec![tx(
        format!("{label}"),
        vec![inst.pda(), job_pda],
        borsh::to_vec(&Message::Init { config: inst.config.clone(), checkpoint, bootstrap: compact.encode(fork) })?,
        payer,
    )];
    let keys = uncompressed_keys(&bootstrap.current_sync_committee).map_err(|e| anyhow!("committee keys: {e:?}"))?;
    txs.extend(adopt_txs(&inst, &label, job_pda, &keys, Some(store.finalized.slot), payer)?);
    let mut outcomes = Vec::new();
    for t in txs {
        let o = live.submit(t).await?;
        let ok = o.applied();
        outcomes.push(o);
        if !ok {
            break;
        }
    }
    let (first, last, ok) = summary_of(&outcomes);
    let dir = live.opts.run_dir.join("net").join(chain.name());
    fs::create_dir_all(dir.join("updates"))?;
    let slot = bootstrap.header.beacon.slot;
    let line = format!(
        "{session}\t{}\t0\tinit\t{}\t-\t-\t{slot}\t{}\t-\t-\t-\t{}\t{}\t{first}\t{last}\t{}\tcheckpoint {checkpoint_hex}",
        chain.name(),
        spec.period_at_slot(slot),
        layout_name(fork),
        hex::encode(jid),
        outcomes.len(),
        if ok { "applied" } else { "failed" }
    );
    if !ok {
        let path = dir.join("jobs-failed.tsv");
        let mut text = fs::read_to_string(&path).unwrap_or_else(|_| JOBS_HEADER.to_string());
        text.push_str(&line);
        text.push('\n');
        fs::write(path, text)?;
        bail!("{}: Init was not applied: {}", chain.name(), outcomes.last().map(|o| o.row.note.clone()).unwrap_or_default());
    }
    fs::write(dir.join("bootstrap.ssz"), &bytes)?;
    fs::write(dir.join("checkpoint"), format!("{checkpoint_hex}\n"))?;
    let follow = Follow::load(&live.opts.run_dir, chain, program)?.ok_or_else(|| anyhow!("kept state"))?;
    follow.append_job(&line)?;
    follow.check(live).await?;
    live.save()?;
    println!("[{}] initialized at slot {slot} (period {}), checkpoint {checkpoint_hex}", chain.name(), spec.period_at_slot(slot));
    Ok(follow)
}

/// The candidate updates of the inputs, oldest first: every period's update, then the
/// finality update.
fn candidates(inputs: &Path, chain: Chain, config: &Config) -> Result<Vec<(String, LightClientUpdate, LcFork)>> {
    let dir = inputs.join(chain.name());
    let mut out = Vec::new();
    let mut names: Vec<String> = match fs::read_dir(dir.join("updates")) {
        Ok(entries) => entries.filter_map(|e| e.ok()?.file_name().into_string().ok()).filter(|n| n.ends_with(".ssz")).collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    for name in names {
        let bytes = fs::read(dir.join("updates").join(&name))?;
        match decode_update(&bytes, config) {
            Ok((u, layout)) => out.push((format!("update {name}"), u, layout)),
            Err(e) => println!("[{}] input {name} skipped: {e}", chain.name()),
        }
    }
    out.sort_by_key(|(_, u, _)| (u.attested_header.beacon.slot, u.signature_slot));
    if let Ok(bytes) = fs::read(dir.join("finality_update.ssz")) {
        match decode_finality_update(&bytes, config) {
            Ok((u, layout)) => out.push(("finality_update".to_string(), u, layout)),
            Err(e) => println!("[{}] finality update skipped: {e}", chain.name()),
        }
    }
    Ok(out)
}

/// Apply every input update that moves the instance, each as one job; stop at the deadline or at
/// the first job that is not applied (the next session tries again).
pub async fn follow(live: &mut Live, f: &mut Follow, inputs: &Path, payer: AccountId, session: &str, deadline: u64) -> Result<(usize, usize)> {
    let chain = f.chain.name();
    let spec = f.inst.config.spec();
    let (mut jobs, mut handoffs) = (0, 0);
    for (source, update, served) in candidates(inputs, f.chain, &f.inst.config)? {
        if now() >= deadline {
            println!("[{chain}] deadline reached; the rest waits for the next session");
            break;
        }
        let store_fork = f.native.core().fork;
        let layout = f.inst.layout_for(store_fork, &update).max(served);
        let mut next = f.native.clone();
        if layout > next.core().fork {
            if let Err(e) = next.upgrade(layout, &spec) {
                println!("[{chain}] {source}: no upgrade to {}: {e:?}", layout_name(layout));
                continue;
            }
        }
        let before: StoreCore = next.core().clone();
        let outcome = match next.process_update(&update, update.signature_slot + 1, &f.ctx(&spec)) {
            Ok(o) => o,
            Err(e) => {
                println!("[{chain}] {source} (attested {}, finalized {}): not applied, the native client refuses it: {e:?}", update.attested_header.beacon.slot, update.finalized_header.beacon.slot);
                continue;
            }
        };
        if !(outcome.advanced || outcome.adopted || outcome.rotated) {
            println!("[{chain}] {source}: changes nothing; not sent");
            continue;
        }
        let seq = f.applied.len() + 1;
        let fin = update.finalized_header.beacon.slot;
        let kind = if update.next_sync_committee.is_empty() { "finality" } else { "period" };
        let label = format!("{chain} #{seq} {kind} p{} fin {fin}", spec.period_at_slot(update.attested_header.beacon.slot));
        let (jid, outcomes, planned) = update_job(live, &f.inst, &update, layout, &before, &label, payer).await?;
        let (first, last, ok) = summary_of(&outcomes);
        let agrees = planned == Some(outcome);
        let status = if ok && agrees { "applied" } else if ok { "applied-differs" } else { "failed" };
        let line = format!(
            "{session}\t{chain}\t{seq}\t{kind}\t{}\t{}\t{}\t{fin}\t{}\t{}\t{}\t{}\t{}\t{}\t{first}\t{last}\t{status}\t{source}{}",
            spec.period_at_slot(update.attested_header.beacon.slot),
            update.attested_header.beacon.slot,
            update.signature_slot,
            layout_name(layout),
            outcome.rotated,
            outcome.advanced,
            outcome.adopted,
            hex::encode(jid),
            outcomes.len(),
            if ok { String::new() } else { format!("; {}", outcomes.last().map(|o| format!("{} {} {}", o.row.status, o.row.exit, o.row.note)).unwrap_or_default()) }
        );
        if !ok {
            let path = f.dir.join("jobs-failed.tsv");
            let mut text = fs::read_to_string(&path).unwrap_or_else(|_| JOBS_HEADER.to_string());
            text.push_str(&line);
            text.push('\n');
            fs::write(path, text)?;
            live.save()?;
            bail!("{chain}: job {label} was not applied");
        }
        // Kept before the check: the chain holds the job whatever the check finds.
        let stem = format!("{seq:05}.{kind}.{}", layout_name(layout));
        fs::write(f.dir.join("updates").join(format!("{stem}.ssz")), served_bytes(&source, inputs, f.chain)?)?;
        f.native = next;
        f.applied.push((stem, update, layout));
        f.append_job(&line)?;
        live.save()?;
        f.check(live).await.with_context(|| format!("after {label}"))?;
        jobs += 1;
        if outcome.rotated {
            handoffs += 1;
        }
        println!("[{chain}] {label}: {status} ({} transactions, blocks {first}-{last}); rotated {} advanced {} adopted {}", outcomes.len(), outcome.rotated, outcome.advanced, outcome.adopted);
    }
    Ok((jobs, handoffs))
}

/// The bytes of an applied update as the endpoint served them (kept with the instance's state).
fn served_bytes(source: &str, inputs: &Path, chain: Chain) -> Result<Vec<u8>> {
    let dir = inputs.join(chain.name());
    Ok(match source.strip_prefix("update ") {
        Some(name) => fs::read(dir.join("updates").join(name))?,
        None => fs::read(dir.join("finality_update.ssz"))?,
    })
}

/// The whole job for one update encoded in `layout` (as `harness::Instance::update_in`):
/// `Open`, `Miller`, the adoption parts if it adopts a committee, `Finish`. Stops at the first
/// transaction that is not applied.
async fn update_job(
    live: &mut Live,
    inst: &Instance,
    update: &LightClientUpdate,
    layout: LcFork,
    full: &StoreCore,
    label: &str,
    payer: AccountId,
) -> Result<(Root, Vec<Outcome>, Option<ApplyOutcome>)> {
    let spec = inst.config.spec();
    let state = live.actor_state(inst.pda().actor()).await?;
    let state: InstanceState = borsh::from_slice(&state)?;
    let stored = CompactStore::decode(&state.store).map_err(|e| anyhow!("store: {e:?}"))?;
    let store = steps::store_for_update(&stored, layout, update.attested_header.beacon.slot, &spec).unwrap_or_else(|_| stored.clone());
    let compact = CompactUpdate::of(update);
    let witness = signer_witness(full, update, &spec);
    let jid = job_id(&inst.id, &store.digest(), &compact.digest(layout));
    let job_pda = inst.job(&jid);
    let (planned, outcome) = plan_job(&store, &compact, &witness, update, &inst.config);
    let mut outcomes = Vec::new();
    let open = live
        .submit(tx(
            format!("{label} Open (step 1)"),
            vec![inst.pda(), job_pda],
            borsh::to_vec(&Message::Open { update: compact.encode(layout), signers: witness.encode() })?,
            payer,
        ))
        .await?;
    let opened = open.applied();
    outcomes.push(open);
    if !opened {
        return Ok((jid, outcomes, outcome));
    }
    let miller = live.submit(tx(format!("{label} Miller (step 2)"), vec![job_pda], borsh::to_vec(&Message::Miller)?, payer)).await?;
    let ok = miller.applied();
    outcomes.push(miller);
    if !ok {
        return Ok((jid, outcomes, outcome));
    }
    if planned.as_ref().is_some_and(|job| job.adoption.is_some()) {
        let keys = uncompressed_keys(&update.next_sync_committee).map_err(|e| anyhow!("committee keys: {e:?}"))?;
        for t in adopt_txs(inst, label, job_pda, &keys, None, payer)? {
            let o = live.submit(t).await?;
            let ok = o.applied();
            outcomes.push(o);
            if !ok {
                return Ok((jid, outcomes, outcome));
            }
        }
    }
    let mut actors = vec![job_pda, inst.pda()];
    if outcome.is_some_and(|o| o.advanced) {
        actors.push(inst.header(update.finalized_header.beacon.slot));
    }
    outcomes.push(live.submit(tx(format!("{label} Finish (step 3)"), actors, borsh::to_vec(&Message::Finish)?, payer)).await?);
    Ok((jid, outcomes, outcome))
}

// ---- Status -----------------------------------------------------------------------------------

/// One line of `instances.tsv` per chain, from the chain's state as the follower holds it.
pub fn instance_line(f: &Follow, state: &InstanceState, store: &CompactStore) -> String {
    let spec = f.inst.config.spec();
    let slot = store.finalized.slot;
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{slot}\t{}\t{}\t{}\t{}\t{}",
        f.chain.name(),
        f.inst.program,
        hex::encode(f.inst.id),
        f.inst.pda().account(),
        state.active,
        state.applied,
        spec.period_at_slot(slot),
        hex::encode(store.finalized.hash_tree_root()),
        layout_name(store.fork),
        store.is_next_sync_committee_known(),
        f.applied.len()
    )
}

pub const INSTANCES_HEADER: &str = "chain\tprogram_account\tinstance_id\tinstance_account\tactive\tjobs_applied\tfinalized_slot\tfinalized_period\tfinalized_root\tstore_layout\tnext_committee_known\tupdates_kept\n";

pub fn append(path: &Path, header: &str, line: &str) -> Result<()> {
    let mut text = fs::read_to_string(path).unwrap_or_else(|_| header.to_string());
    writeln!(text, "{line}")?;
    fs::write(path, text)?;
    Ok(())
}
