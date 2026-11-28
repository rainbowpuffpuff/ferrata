//! The run on fresh Sepolia data: CI's `sequencer-attest` job (`scripts/ci/sequencer-attest.sh`).
//!
//! A standalone sequencer stamps its blocks with the wall clock, and the program bounds every `Init` and every
//! update by the weak-subjectivity period of the instance's finalized header (3,532 epochs, about 15.7 days;
//! `crates/ferrata-core/src/ws.rs`). Recorded Ethereum data therefore ages out: the job's run of 21 November 2026
//! stopped at `twin Init`, "Execution outside of the validity window", on the mainnet data of 31 October
//! (`e2e/runs/ci-sequencer-attest-2026-11-21/`). These phases take their Ethereum data from two directories fetched
//! from public Sepolia endpoints when the run starts, every file pinned by the directory's `SHA256SUMS`:
//!
//! - `--fresh-checkpoint DIR` (`scripts/fetch-sepolia-checkpoint.py`): the bootstrap of a recent finalized
//!   checkpoint, the root a second beacon provider reports at its slot (the instance's trusted checkpoint), and the
//!   best update of every sync-committee period from the bootstrap's on (each carries the next committee);
//! - `--fresh-attest DIR` (`lez/scripts/fetch-attest-fixture.py --network sepolia`, fetched after the build): the
//!   newest finality update, the root the second provider reports for the block it finalizes, the execution block
//!   that block binds (its RLP header, checked against the bound hash, and its receipts), `eth_getProof` of Sepolia
//!   USDC's `totalSupply_` at that block (checked against `eth_call totalSupply()`), and the block's first ERC-20
//!   `Transfer`.
//!
//! Commands (`../main.rs`):
//!   fresh-check      no sequencer: the whole chain natively (bootstrap, the period updates, the finality update,
//!                    which must advance the store; the facts evaluated under the record it makes); one JSON line.
//!                    Exit 3 when the finality update does not advance the store (fetch it again after the next
//!                    finalized epoch), 4 when the bootstrap is over 32 h or the finalized block over 2 h old by this
//!                    machine's clock (Sepolia not finalizing, or a stale endpoint), 1 on any other failure
//!   fresh-follow     instance A of the `main` deployment: `Init`, the period updates, the finality update; its
//!                    finalized root must be the one the second provider reports
//!   fresh-scenarios  the `twin` deployment: the refusals of `scenario::scenarios` on this data (a signature that is
//!                    not a point, an update carrying another update's signature, steps sent by the wrong actor, a
//!                    second `Init`, an update for another chain's instance, a job before its signature slot on an
//!                    instance whose clock runs ten years later, duplicated and replayed steps around the finality
//!                    update, which applies)
//!   fresh-consume    the attestation consumed publicly under A's record of the finalized block: `Attest` as the
//!                    root of a transaction (a storage slot, a log, both at once) and its refusals; the reference
//!                    consumer deployed, its `CallAttest` storing the program's whole reply, its `Claim` of the
//!                    `Transfer` crediting the sender once and the replay refused
//!   fresh-private    the reference consumer's `AttestPrivately` of the same `Transfer` at a private account's actor,
//!                    proven for real by LEZ's wallet and verified by the sequencer (`RISC0_DEV_MODE` unset)
//!
//! No key is held for any of it: every refusal is made from the real data (another update's signature, a cleared
//! compression flag, another chain's `genesis_validators_root`, a clock ten years later, a tampered block header).

use std::{borrow::Cow, fs, path::Path, time::Instant};

use anyhow::{anyhow, bail, ensure, Context, Result};
use ferrata_core::{
    fork::LcFork,
    merkle::ZERO,
    rlp::encode_u64,
    steps::{self, HeaderRecord, Job, PendingApply, Phase},
    store::ApplyOutcome,
    types::{BeaconBlockHeader, Execution, LightClientBootstrap, LightClientHeader, LightClientUpdate, SyncAggregate, SyncCommittee},
    witness::{CompactBootstrap, CompactStore, CompactUpdate},
    Root,
};
use ferrata_lez_wire::{
    attest::{evaluate, AccountQuery, Address, Attestation, Cmp, Condition, Field, LogQuery, Request, SlotQuery, Word},
    code,
    consumer::{self, balance_seed, claim_seed, nullifier, ClaimState, ConsumerMessage, ConsumerState, Deposit, Pool},
    job_id, Config, Message, RecordState,
};
use ferrata_spectests::execkit::{account_proof, hexb, receipt_rlp, word, AccountProof, Trie};
use lee::{privacy_preserving_transaction::circuit::ProgramCatalog, program::Program, AccountId};
use lee_core::program::PdaSeed;
use serde_json::{json, Value};
use test_guest_core::Script;
use wallet::AccountIdentity;

use crate::{
    attest::{check_sums, read},
    live::{mention, Live, Sent, Tx},
    manifest::Row,
    plan::{sepolia_config, signer_witness, Instance, Native, Pda, SEPOLIA_CHAIN_ID},
    private::{describe_private, private_wallet_at, TapClient},
    programs::GuestBinary,
    scenario::{
        apply, check_record, check_store, deploy, finish_init, init, init_txs, instance_state, layout_for, store, Harness,
        World,
    },
};

/// Ten years: an instance whose clock puts every slot of the fetched data in the future (as `scenario.rs`).
const FUTURE_SHIFT_SECONDS: u64 = 315_360_000;
/// The light-client layouts a Sepolia bootstrap or update can be served in since Electra.
const LAYOUTS: [LcFork; 2] = [LcFork::Electra, LcFork::Gloas];
/// The consumer's public reader actor (the harness's, `lez/harness/src/attest.rs`).
const READER_SEED: [u8; 32] = [0x44; 32];
/// Exit status of `fresh-check` when the finality update does not move the store (fetch it again later).
pub const NOT_NEWER_EXIT: i32 = 3;
/// Exit status of `fresh-check` when the data is older than its bound: Sepolia has not finalized within it, or an
/// endpoint served stale data (no retry helps).
pub const STALE_EXIT: i32 = 4;
/// The oldest the finality update's finalized block may be when it is checked: Sepolia finalizes every 6.4 minutes,
/// and a light-client endpoint serves its newest finality update; two hours of lag is an endpoint (or a chain) that has
/// stopped, not data to attest.
pub const MAX_FINALIZED_AGE_S: u64 = 2 * 3600;
/// The oldest the bootstrap may be: the first checkpoint of the finality update's period (or a later finalized one) is
/// at most one period (27.3 h) old, and CI fetches it before the build: with the steps' limits (checkpoint 15 min,
/// build 170 min, data 50 min) at most 31.2 h when checked; 32 hours.
pub const MAX_BOOTSTRAP_AGE_S: u64 = 32 * 3600;

/// The light-client layout of the fork that `config`'s schedule puts at `slot`.
fn layout_at(config: &Config, slot: u64) -> Option<LcFork> {
    let spec = config.spec();
    spec.fork_at_epoch(spec.epoch_at_slot(slot)).lc_fork()
}

/// `Config::store_fork`'s byte of a layout.
fn store_fork_of(layout: LcFork) -> u8 {
    match layout {
        LcFork::Altair => 0,
        LcFork::Capella => 1,
        LcFork::Deneb => 2,
        LcFork::Electra => 3,
        LcFork::Gloas => 4,
    }
}

fn hex0x(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

fn hex32(v: &Value, what: &str) -> Result<Root> {
    let s = v.as_str().ok_or_else(|| anyhow!("{what}: not a string"))?;
    hexb(s).try_into().map_err(|_| anyhow!("{what}: not 32 bytes"))
}

fn address(v: &Value, what: &str) -> Result<Address> {
    let s = v.as_str().ok_or_else(|| anyhow!("{what}: not a string"))?;
    hexb(s).try_into().map_err(|_| anyhow!("{what}: not 20 bytes"))
}

/// The block root the second beacon provider reported as canonical at `slot` (`beacon_header_<slot>_second.json`,
/// kept byte for byte by both fetch scripts).
fn second_root(dir: &Path, slot: u64) -> Result<Root> {
    let name = format!("beacon_header_{slot}_second.json");
    let v: Value = serde_json::from_slice(&read(dir, &name)?).with_context(|| name.clone())?;
    let data = &v["data"];
    ensure!(data["header"]["message"]["slot"].as_str() == Some(slot.to_string().as_str()), "{name}: not the header at slot {slot}");
    ensure!(data["canonical"].as_bool() != Some(false), "{name}: not canonical");
    hex32(&data["root"], &format!("{name}: data.root"))
}

/// A Gloas `LightClientFinalityUpdate` (every field fixed-size) as the update it stands for.
fn gloas_finality_update(b: &[u8]) -> Result<LightClientUpdate> {
    const H: usize = 112 + 32 + 11 * 32;
    ensure!(b.len() == 2 * H + 9 * 32 + 160 + 8, "not a Gloas LightClientFinalityUpdate ({} bytes)", b.len());
    let header = |r: &[u8]| LightClientHeader::decode(r, LcFork::Gloas).map_err(|e| anyhow!("header: {e:?}"));
    Ok(LightClientUpdate {
        attested_header: header(&b[..H])?,
        next_sync_committee: SyncCommittee::empty(512),
        next_sync_committee_branch: vec![ZERO; LcFork::Gloas.sync_committee_branch_len()],
        finalized_header: header(&b[H..2 * H])?,
        finality_branch: b[2 * H..2 * H + 288].chunks_exact(32).map(|c| c.try_into().expect("32 bytes")).collect(),
        sync_aggregate: SyncAggregate::decode(&b[2 * H + 288..2 * H + 448], 512).map_err(|e| anyhow!("aggregate: {e:?}"))?,
        signature_slot: u64::from_le_bytes(b[2 * H + 448..].try_into()?),
    })
}

/// The block's first ERC-20 `Transfer` (`fixture.json`'s `transfer`).
#[derive(Clone, Debug)]
pub struct Transfer {
    pub tx_index: u64,
    pub log_index: u64,
    pub token: Address,
    pub from: Address,
    pub to: Address,
}

/// What the run reads from the two directories, checked against each other.
pub struct Fresh {
    pub config: Config,
    pub checkpoint: Root,
    pub bootstrap: LightClientBootstrap,
    pub bootstrap_fork: LcFork,
    /// `(period, update)`, from the bootstrap's period on, without a gap.
    pub updates: Vec<(u64, LightClientUpdate)>,
    pub finality: LightClientUpdate,
    pub finalized_slot: u64,
    /// The root the second provider reports at `finalized_slot`.
    pub finalized_root: Root,
    pub block: u64,
    pub block_hash: Root,
    /// The bound block's RLP header (a Gloas record binds the block hash only).
    pub header: Vec<u8>,
    /// Sepolia USDC and its `totalSupply_` slot and value at `block`.
    pub contract: Address,
    pub slot: Word,
    pub value: Word,
    pub storage: AccountProof,
    pub transfer: Transfer,
    receipts_trie: Trie,
}

impl Fresh {
    pub fn load(checkpoint_dir: &Path, attest_dir: &Path) -> Result<Self> {
        check_sums(checkpoint_dir).with_context(|| format!("{}", checkpoint_dir.display()))?;
        check_sums(attest_dir).with_context(|| format!("{}", attest_dir.display()))?;
        let mut config = sepolia_config(2);
        let bytes = read(checkpoint_dir, "bootstrap.ssz")?;
        let (bootstrap, bootstrap_fork) = LAYOUTS
            .into_iter()
            .find_map(|layout| {
                let b = LightClientBootstrap::decode(&bytes, layout, 512).ok()?;
                (layout_at(&config, b.header.beacon.slot) == Some(layout)).then_some((b, layout))
            })
            .ok_or_else(|| anyhow!("bootstrap.ssz decodes in no layout of the fork at its slot"))?;
        config.store_fork = store_fork_of(bootstrap_fork);
        let spec = config.spec();
        let boot_slot = bootstrap.header.beacon.slot;
        let checkpoint = second_root(checkpoint_dir, boot_slot)?;
        ensure!(
            bootstrap.header.beacon.hash_tree_root() == checkpoint,
            "bootstrap.ssz (slot {boot_slot}) does not hash to the root the second provider reports at its slot"
        );

        let mut periods = Vec::new();
        for entry in fs::read_dir(checkpoint_dir).with_context(|| checkpoint_dir.display().to_string())? {
            let name = entry?.file_name().to_string_lossy().into_owned();
            if let Some(p) = name.strip_prefix("update_").and_then(|r| r.strip_suffix(".ssz")) {
                periods.push(p.parse::<u64>().map_err(|_| anyhow!("{name}: not update_<period>.ssz"))?);
            }
        }
        periods.sort_unstable();
        let first = spec.period_at_slot(boot_slot);
        ensure!(
            !periods.is_empty() && periods.iter().enumerate().all(|(i, p)| *p == first + i as u64),
            "the checkpoint directory's updates {periods:?} are not periods {first}, {}, ... without a gap",
            first + 1
        );
        let mut updates = Vec::new();
        for p in periods {
            let name = format!("update_{p}.ssz");
            let bytes = read(checkpoint_dir, &name)?;
            let update = LAYOUTS
                .into_iter()
                .find_map(|layout| {
                    let u = LightClientUpdate::decode(&bytes, layout, 512).ok()?;
                    (layout_at(&config, u.attested_header.beacon.slot) == Some(layout)).then_some(u)
                })
                .ok_or_else(|| anyhow!("{name} decodes in no layout of the fork at its attested slot"))?;
            ensure!(spec.period_at_slot(update.signature_slot) == p, "{name} is not signed in period {p}");
            updates.push((p, update));
        }

        let m: Value = serde_json::from_slice(&read(attest_dir, "fixture.json")?).context("fixture.json")?;
        ensure!(m["chain_id"].as_u64() == Some(SEPOLIA_CHAIN_ID), "the attestation data is not Sepolia's");
        let finality = gloas_finality_update(&read(attest_dir, "finality_update.ssz")?)?;
        let finalized_slot = m["finalized_slot"].as_u64().ok_or_else(|| anyhow!("fixture.json: finalized_slot"))?;
        ensure!(finality.finalized_header.beacon.slot == finalized_slot, "the finality update finalizes another slot");
        let finalized_root = second_root(attest_dir, finalized_slot)?;
        ensure!(
            finality.finalized_header.beacon.hash_tree_root() == finalized_root,
            "the finalized header is not the block the second provider reports at slot {finalized_slot}"
        );
        let Execution::BlockHash { block_hash, .. } = &finality.finalized_header.execution else {
            bail!("the finalized header binds no block hash (not the Gloas layout)");
        };
        let block_hash = *block_hash;
        ensure!(hex32(&m["block_hash"], "fixture.json: block_hash")? == block_hash, "fixture.json names another block hash");
        let header = read(attest_dir, "header.rlp")?;
        let decoded = ferrata_core::exec::decode_header_with_hash(&header, &block_hash)
            .map_err(|e| anyhow!("header.rlp is not the bound block's header: {e:?}"))?;
        let block = m["block"].as_u64().ok_or_else(|| anyhow!("fixture.json: block"))?;
        ensure!(decoded.number == block, "header.rlp is block {}, fixture.json names {block}", decoded.number);

        let served: Value = serde_json::from_slice(&read(attest_dir, &format!("receipts_{block}.json"))?)?;
        let receipts = served["result"].as_array().ok_or_else(|| anyhow!("receipts_{block}.json: no result array"))?;
        let receipts_trie = Trie::new(receipts.iter().enumerate().map(|(i, r)| {
            let (key, len) = encode_u64(i as u64);
            (key[..len].to_vec(), receipt_rlp(r))
        }));
        ensure!(
            receipts_trie.root() == decoded.receipts_root,
            "the receipts rebuilt from eth_getBlockReceipts do not hash to the block's receipts root"
        );

        let storage_json = &m["storage"][0];
        let row = &storage_json["slots"][0];
        let contract = address(&storage_json["address"], "fixture.json: storage address")?;
        let slot = word(row["slot"].as_str().ok_or_else(|| anyhow!("fixture.json: storage slot"))?);
        let value = word(row["value"].as_str().ok_or_else(|| anyhow!("fixture.json: storage value"))?);
        let storage = account_proof(&read(attest_dir, &format!("proof_{}_{block}.json", hex0x(&contract)))?);
        ensure!(storage.address == contract, "the storage proof is of another account");
        ensure!(
            storage.storage.iter().any(|s| s.key == slot && s.value == value),
            "the storage proof does not hold fixture.json's slot and value"
        );

        let t = &m["transfer"];
        let transfer = Transfer {
            tx_index: t["tx_index"].as_u64().ok_or_else(|| anyhow!("fixture.json: transfer.tx_index"))?,
            log_index: t["log_index"].as_u64().ok_or_else(|| anyhow!("fixture.json: transfer.log_index"))?,
            token: address(&t["token"], "transfer.token")?,
            from: address(&t["from"], "transfer.from")?,
            to: address(&t["to"], "transfer.to")?,
        };
        Ok(Self {
            config,
            checkpoint,
            bootstrap,
            bootstrap_fork,
            updates,
            finality,
            finalized_slot,
            finalized_root,
            block,
            block_hash,
            header,
            contract,
            slot,
            value,
            storage,
            transfer,
            receipts_trie,
        })
    }

    pub fn receipt_proof(&self, tx_index: u64) -> Vec<Vec<u8>> {
        let (key, len) = encode_u64(tx_index);
        self.receipts_trie.proof(&key[..len])
    }

    /// The instances of a run, by name: A on `main`; B, C (another chain's `genesis_validators_root`) and D (a clock
    /// ten years later) on `twin`.
    pub fn instance(&self, which: &str, main: AccountId, twin: AccountId) -> Instance {
        match which {
            "A" => Instance::new("fresh A (main deployment, Sepolia)", main, self.config.clone(), self.checkpoint),
            "B" => Instance::new("fresh B (twin deployment, Sepolia)", twin, self.config.clone(), self.checkpoint),
            "C" => {
                let mut config = self.config.clone();
                config.genesis_validators_root = [0x42; 32];
                Instance::new("fresh C (twin deployment, another chain's genesis_validators_root)", twin, config, self.checkpoint)
            }
            _ => {
                let mut config = self.config.clone();
                config.genesis_time += FUTURE_SHIFT_SECONDS;
                Instance::new("fresh D (twin deployment, genesis time ten years later)", twin, config, self.checkpoint)
            }
        }
    }

    fn world(&self, inst: &Instance) -> Result<World> {
        World::with_checkpoint(&inst.name, inst.program, inst.config.clone(), self.checkpoint, &self.bootstrap, self.bootstrap_fork)
    }

    /// A request bound to `instance_id` and to Sepolia, with the bound block's RLP header.
    pub fn request(&self, instance_id: Root) -> Request {
        Request { instance_id, chain_id: SEPOLIA_CHAIN_ID, header: Some(self.header.clone()), ..Request::default() }
    }

    /// USDC's `totalSupply_` equal to its value at the block.
    pub fn storage_request(&self, instance_id: Root) -> Result<Request> {
        let slot = self.storage.storage.iter().find(|s| s.key == self.slot).ok_or_else(|| anyhow!("no proof of the slot"))?;
        Ok(Request {
            accounts: vec![AccountQuery {
                address: self.storage.address,
                proof: self.storage.proof.clone(),
                conditions: vec![],
                slots: vec![SlotQuery {
                    slot: self.slot,
                    proof: slot.proof.clone(),
                    conditions: vec![Condition { field: Field::Value, cmp: Cmp::Eq, rhs: self.value }],
                }],
            }],
            ..self.request(instance_id)
        })
    }

    /// The pool of the reference consumer for this `Transfer`: its token, its recipient.
    pub fn pool(&self, ferrata: AccountId, instance_id: Root) -> Pool {
        Pool { ferrata, instance_id, chain_id: SEPOLIA_CHAIN_ID, token: self.transfer.token, recipient: self.transfer.to }
    }

    /// The `Transfer` with the pool's conditions (emitter, topic 0, recipient).
    pub fn log_request(&self, instance_id: Root) -> Request {
        let t = &self.transfer;
        Request {
            logs: vec![LogQuery {
                tx_index: t.tx_index,
                log_index: t.log_index,
                proof: self.receipt_proof(t.tx_index),
                conditions: self.pool(AccountId::new([0; 32]), instance_id).conditions(),
            }],
            ..self.request(instance_id)
        }
    }

    /// The finality update with the signature of another update of the data: a point of G2 over another message.
    pub fn forged(&self) -> LightClientUpdate {
        let mut forged = self.finality.clone();
        forged.sync_aggregate.signature = self.updates[0].1.sync_aggregate.signature;
        forged
    }
}

// ---------------------------------------------------------------------------------------------
// fresh-check: the whole chain natively, before any sequencer
// ---------------------------------------------------------------------------------------------

/// Unix milliseconds as `YYYY-MM-DDTHH:MM:SSZ`.
fn utc(ms: u64) -> String {
    let s = ms / 1000;
    let (days, rem) = (s / 86_400, s % 86_400);
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Process `update` on the native store as `scenario::apply` does (upgrading the layout first if the update's is
/// later), keeping the result only if it is accepted.
fn native_process(native: &mut Native, config: &Config, update: &LightClientUpdate) -> Result<ApplyOutcome> {
    let layout = layout_for(config, &CompactStore::of(native.core()), update);
    let mut next = native.clone();
    if layout > next.core().fork {
        next.store.upgrade(layout, &next.spec).map_err(|e| anyhow!("the native store does not upgrade: {e:?}"))?;
    }
    let outcome = next.process(update).map_err(|e| anyhow!("the native store refuses it: {e:?}"))?;
    *native = next;
    Ok(outcome)
}

/// Why `fresh-check` refuses data that loads: not newer than the store yet (exit 3: fetch again later), or older than
/// its bound (exit 4: Sepolia not finalizing, or a stale endpoint).
pub enum Refused {
    NotNewer(String),
    Stale(String),
}

impl Refused {
    pub fn exit_code(&self) -> i32 {
        match self {
            Refused::NotNewer(_) => NOT_NEWER_EXIT,
            Refused::Stale(_) => STALE_EXIT,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Refused::NotNewer(why) => why.clone(),
            Refused::Stale(why) => format!("stale data (Sepolia not finalizing, or a stale endpoint): {why}"),
        }
    }
}

/// The chain natively, and a summary for the job's record. `Ok(Err(Refused::NotNewer))` when the finality update does
/// not advance the store (the data is good, only too early); `Ok(Err(Refused::Stale))` when the bootstrap or the
/// finalized block is older than its bound by this machine's clock.
pub fn check(fresh: &Fresh) -> Result<std::result::Result<Value, Refused>> {
    let inst = fresh.instance("A", AccountId::new([1; 32]), AccountId::new([2; 32]));
    let now_ms = u64::try_from(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis())?;
    let age_s = |slot: u64| now_ms.saturating_sub(inst.config.slot_start_ms(slot)) / 1000;
    let boot_slot = fresh.bootstrap.header.beacon.slot;
    if age_s(boot_slot) > MAX_BOOTSTRAP_AGE_S {
        return Ok(Err(Refused::Stale(format!(
            "the bootstrap (slot {boot_slot}, {}) is {} s old, over the bound of {MAX_BOOTSTRAP_AGE_S} s",
            utc(inst.config.slot_start_ms(boot_slot)),
            age_s(boot_slot)
        ))));
    }
    if age_s(fresh.finalized_slot) > MAX_FINALIZED_AGE_S {
        return Ok(Err(Refused::Stale(format!(
            "the finality update's finalized block (slot {}, {}) is {} s old, over the bound of {MAX_FINALIZED_AGE_S} s",
            fresh.finalized_slot,
            utc(inst.config.slot_start_ms(fresh.finalized_slot)),
            age_s(fresh.finalized_slot)
        ))));
    }
    let mut world = fresh.world(&inst)?;
    let mut periods = Vec::new();
    for (p, update) in &fresh.updates {
        let o = native_process(&mut world.native, &inst.config, update).with_context(|| format!("period {p} update"))?;
        periods.push(json!({"period": p, "finalizedSlot": update.finalized_header.beacon.slot, "advanced": o.advanced, "adopted": o.adopted}));
    }
    let before = world.native.core().finalized_header.beacon.slot;
    if fresh.finalized_slot <= before {
        return Ok(Err(Refused::NotNewer(format!(
            "the finality update finalizes slot {}, not newer than the store's finalized slot {before}: fetch it again after the next finalized epoch",
            fresh.finalized_slot
        ))));
    }
    let o = native_process(&mut world.native, &inst.config, &fresh.finality).context("the finality update")?;
    ensure!(o.advanced, "the finality update does not advance the store");
    let core = world.native.core();
    ensure!(core.finalized_header.beacon.hash_tree_root() == fresh.finalized_root, "the store's finalized root");
    let record = HeaderRecord::of(&core.finalized_header, core.fork, &world.native.spec);
    let storage = evaluate(&inst.id, SEPOLIA_CHAIN_ID, &record, &fresh.storage_request(inst.id)?)
        .map_err(|c| anyhow!("the storage slot does not attest under the record: code {c}"))?;
    let log = evaluate(&inst.id, SEPOLIA_CHAIN_ID, &record, &fresh.log_request(inst.id))
        .map_err(|c| anyhow!("the Transfer does not attest under the record: code {c}"))?;
    let deposit = fresh
        .pool(inst.program, inst.id)
        .accept(&log)
        .map_err(|c| anyhow!("the reference consumer would not accept the Transfer: code {c}"))?;
    ensure!(deposit.owner == fresh.transfer.from, "the Transfer's sender is not the one fixture.json names");
    ensure!(storage.block_number == fresh.block && log.block_number == fresh.block, "the facts' block");
    let spec = inst.config.spec();
    let ws_end = inst.config.ws_end_slot(fresh.bootstrap.header.beacon.slot).ok_or_else(|| anyhow!("no ws period"))?;
    Ok(Ok(json!({
        "chain": "sepolia",
        "checkedAt": utc(now_ms),
        "bootstrapAgeS": age_s(boot_slot),
        "finalizedAgeS": age_s(fresh.finalized_slot),
        "maxBootstrapAgeS": MAX_BOOTSTRAP_AGE_S,
        "maxFinalizedAgeS": MAX_FINALIZED_AGE_S,
        "checkpoint": hex0x(&fresh.checkpoint),
        "bootstrapSlot": fresh.bootstrap.header.beacon.slot,
        "bootstrapTime": utc(inst.config.slot_start_ms(fresh.bootstrap.header.beacon.slot)),
        "bootstrapPeriod": spec.period_at_slot(fresh.bootstrap.header.beacon.slot),
        "layout": format!("{:?}", fresh.bootstrap_fork),
        "initValidUntil": utc(inst.config.slot_start_ms(ws_end)),
        "periods": periods,
        "finalizedSlot": fresh.finalized_slot,
        "finalizedTime": utc(inst.config.slot_start_ms(fresh.finalized_slot)),
        "finalizedRoot": hex0x(&fresh.finalized_root),
        "signatureSlot": fresh.finality.signature_slot,
        "signers": fresh.finality.sync_aggregate.bits.iter().filter(|b| **b).count(),
        "block": fresh.block,
        "blockHash": hex0x(&fresh.block_hash),
        "storage": {"contract": hex0x(&fresh.contract), "slot": hex0x(&fresh.slot), "value": hex0x(&fresh.value)},
        "transfer": {"txIndex": fresh.transfer.tx_index, "logIndex": fresh.transfer.log_index, "token": hex0x(&fresh.transfer.token),
                     "from": hex0x(&deposit.owner), "to": hex0x(&fresh.transfer.to), "amount": deposit.amount.to_string()},
        "statementIds": {"storage": hex0x(&storage.statements[1].id), "log": hex0x(&log.statements[0].id)},
    })))
}

// ---------------------------------------------------------------------------------------------
// fresh-follow: instance A of the main deployment follows Sepolia
// ---------------------------------------------------------------------------------------------

pub async fn follow(live: &mut Live, main: AccountId, twin: AccountId, fresh: &Fresh, payer: AccountId) -> Result<()> {
    let inst = fresh.instance("A", main, twin);
    let mut world = fresh.world(&inst)?;
    println!(
        "[fresh] instance A 0x{} of {main}: checkpoint 0x{} (slot {}), {} period update(s), then the finality update of slot {}",
        hex::encode(inst.id),
        hex::encode(fresh.checkpoint),
        fresh.bootstrap.header.beacon.slot,
        fresh.updates.len(),
        fresh.finalized_slot
    );
    init(live, &mut world, &fresh.bootstrap, "fresh A Init", Harness::None, payer).await?;
    for (p, update) in &fresh.updates {
        let run = apply(live, &mut world, update, &format!("fresh A period {p} update"), Harness::None, payer).await?;
        let o = run.outcome.ok_or_else(|| anyhow!("period {p} update: not applied"))?;
        println!("[fresh] period {p} update applied: advanced {}, adopted {}", o.advanced, o.adopted);
    }
    let run = apply(live, &mut world, &fresh.finality, "fresh A finality update", Harness::None, payer).await?;
    ensure!(run.outcome.is_some_and(|o| o.advanced), "the finality update does not advance instance A");
    let on_chain = store(live, &world.inst).await?;
    ensure!(on_chain.finalized.slot == fresh.finalized_slot, "A's finalized slot is {}", on_chain.finalized.slot);
    ensure!(
        on_chain.finalized.hash_tree_root() == fresh.finalized_root,
        "A's finalized root is not the root the second provider reports at slot {}",
        fresh.finalized_slot
    );
    check_record(live, &world, fresh.finalized_slot).await?;
    println!(
        "[fresh] instance A holds finalized slot {} with root 0x{} (the second provider's) and its record",
        fresh.finalized_slot,
        hex::encode(fresh.finalized_root)
    );
    live.save()
}

// ---------------------------------------------------------------------------------------------
// fresh-scenarios: the twin deployment's refusals on this data
// ---------------------------------------------------------------------------------------------

fn tx(step: &str, actors: Vec<Pda>, message: Vec<u8>, payer: AccountId) -> Tx {
    Tx { step: step.to_string(), harness: None, actors, root: 0, message, payer, gas_limit: None, note: String::new() }
}

pub async fn scenarios(
    live: &mut Live,
    main: AccountId,
    twin: AccountId,
    scripted: AccountId,
    fresh: &Fresh,
    payer: AccountId,
) -> Result<()> {
    let spec = fresh.config.spec();
    let layout = fresh.bootstrap_fork;
    let mut b = fresh.world(&fresh.instance("B", main, twin))?;

    // Init, with one detour: the wallet's default gas limit on the first adoption part.
    let (init_job, txs) = init_txs(&b.inst, &fresh.bootstrap, "fresh twin Init", payer)?;
    for (index, t) in txs.into_iter().enumerate() {
        if index == 1 {
            let detour = Tx {
                step: format!("{} {}", t.step, crate::live::DETOUR_GAS_STEP),
                harness: None,
                actors: t.actors.clone(),
                root: t.root,
                message: t.message.clone(),
                payer,
                gas_limit: Some(crate::live::DETOUR_GAS),
                note: format!("{}; the step needs more", crate::live::DETOUR_GAS_NOTE),
            };
            let out = live.submit(detour).await?;
            ensure!(out.row.status == "reverted", "expected an out-of-gas revert, got {} {}", out.row.status, out.row.note);
            ensure!(
                out.row.exec_gas == crate::live::DETOUR_GAS.to_string(),
                "an out-of-gas action is charged its whole gas limit, got {}",
                out.row.exec_gas
            );
        }
        live.submit(t).await?.expect_applied()?;
    }
    b.jobs.push(init_job);
    finish_init(live, &b, &fresh.bootstrap).await?;

    // The period update: hands over the next committee.
    let (period, update_p) = fresh.updates[0].clone();
    let run = apply(live, &mut b, &update_p, &format!("fresh twin period {period} update"), Harness::None, payer).await?;
    ensure!(run.outcome.is_some_and(|o| o.adopted), "the period update hands over the next committee");
    let b_update_job = run.job_id;
    let store_before = store(live, &b.inst).await?;

    // A signature that is not a point: refused at step 1.
    {
        let mut bad = fresh.finality.clone();
        bad.sync_aggregate.signature[0] &= 0x7f; // compression flag cleared
        let run = apply(live, &mut b, &bad, "fresh malformed signature:", Harness::None, payer).await?;
        ensure!(run.outcomes.len() == 1 && run.outcomes[0].exit_code() == Some(41), "SignatureMalformed at step 1");
    }

    // A forged signature (the period update's, over the finality update): steps 1 and 2 pass, step 3 refuses.
    {
        let run = apply(live, &mut b, &fresh.forged(), "fresh forged signature:", Harness::None, payer).await?;
        ensure!(run.outcomes.len() == 3 && run.outcomes[0].applied() && run.outcomes[1].applied(), "steps 1 and 2 pass");
        ensure!(run.outcomes[2].exit_code() == Some(40), "SignatureInvalid at the final exponentiation");
        ensure!(store(live, &b.inst).await? == store_before, "a refused job changes nothing");
    }

    // Steps sent by the wrong actor.
    {
        let inst = b.inst.clone();
        let fake_job = Job {
            store_digest: store(live, &inst).await?.digest(),
            update_digest: [0; 32],
            phase: Phase::Verified,
            adoption: None,
            pending: PendingApply::Update {
                finalized: BeaconBlockHeader { slot: 99_999_999, ..Default::default() },
                next: None,
                record: HeaderRecord::of(&fresh.bootstrap.header, layout, &spec),
            },
        };
        let jid = [7u8; 32];
        let apply_msg = Message::Apply { instance_id: inst.id, job_id: jid, job: fake_job.encode() };
        live.submit(tx("fresh Apply sent as the transaction root", vec![inst.pda()], borsh::to_vec(&apply_msg)?, payer))
            .await?
            .expect_exit(code::WRONG_SENDER)?;
        // An actor of another program (LEZ's `scripted` test program) sending internal messages.
        let attacker = Pda { program: scripted, seed: PdaSeed::new([7; 32]) };
        let scripted_tx = |step: &str, to: Pda, message: &Message| -> Result<Tx> {
            let script = Script::default().send_sync(to.actor(), message);
            Ok(tx(step, vec![attacker, to], borsh::to_vec(&script)?, payer))
        };
        live.submit(scripted_tx("fresh Apply from another program's actor", inst.pda(), &apply_msg)?)
            .await?
            .expect_exit(code::WRONG_SENDER)?;
        let verified = Message::ApplyVerified { instance_id: inst.id, job: fake_job.encode() };
        live.submit(scripted_tx("fresh ApplyVerified from another program's actor", inst.pda(), &verified)?)
            .await?
            .expect_exit(code::WRONG_SENDER)?;
        let open = Message::JobOpen { instance_id: inst.id, job_id: jid, committee_size: 512, job: fake_job.encode() };
        live.submit(scripted_tx("fresh JobOpen from another program's actor", inst.job(&jid), &open)?)
            .await?
            .expect_exit(code::WRONG_SENDER)?;
        let record = Message::Record {
            instance_id: inst.id,
            slot: 99_999_999,
            chain_id: inst.config.chain_id,
            record: vec![1, 2, 3],
            root_id: inst.id,
        };
        live.submit(scripted_tx("fresh Record from another program's actor", inst.header(99_999_999), &record)?)
            .await?
            .expect_exit(code::WRONG_SENDER)?;
        // The same image deployed at another address is another program: its jobs are not ours.
        let other_deployment = Instance::new("A", main, inst.config.clone(), inst.checkpoint);
        live.submit(tx(
            "fresh Miller at the other deployment's address of the job",
            vec![other_deployment.job(&jid)],
            borsh::to_vec(&Message::Miller)?,
            payer,
        ))
        .await?
        .expect_exit(code::NO_JOB)?;
        ensure!(live.actor_state(inst.header(99_999_999).actor()).await?.is_empty(), "no record was written");
        check_store(live, &b).await?;
    }

    // Init again.
    {
        let msg = Message::Init {
            config: b.inst.config.clone(),
            checkpoint: b.inst.checkpoint,
            bootstrap: CompactBootstrap::of(&fresh.bootstrap).encode(layout),
        };
        live.submit(tx("fresh Init of an existing instance", vec![b.inst.pda()], borsh::to_vec(&msg)?, payer))
            .await?
            .expect_exit(code::ALREADY_INITIALIZED)?;
    }

    // An update for another instance: the same data, an instance of another chain (its genesis_validators_root
    // differs). Its bootstrap verifies (no signature), the update does not.
    {
        let mut other = fresh.world(&fresh.instance("C", main, twin))?;
        ensure!(other.inst.id != b.inst.id, "another instance");
        init(live, &mut other, &fresh.bootstrap, "fresh other chain Init", Harness::None, payer).await?;
        let run = apply(live, &mut other, &update_p, "fresh other chain period update", Harness::None, payer).await?;
        let last = run.outcomes.last().ok_or_else(|| anyhow!("no transaction"))?;
        ensure!(last.exit_code() == Some(40), "SignatureInvalid, got {} {}", last.row.status, last.row.exit);
        live.submit(tx(
            "fresh instance B's job id sent to instance C",
            vec![other.inst.job(&b_update_job)],
            borsh::to_vec(&Message::Finish)?,
            payer,
        ))
        .await?
        .expect_exit(code::NO_JOB)?;
    }

    // Too early: an Open before its signature slot is valid only from that slot's start; a live sequencer stamps
    // blocks with its own clock, so the early instance is one whose genesis time is ten years later.
    {
        let mut early = fresh.world(&fresh.instance("D", main, twin))?;
        init(live, &mut early, &fresh.bootstrap, "fresh late clock Init", Harness::None, payer).await?;
        let inst = &early.inst;
        let on_chain = store(live, inst).await?;
        let fork = layout_for(&inst.config, &on_chain, &update_p);
        let compact = CompactUpdate::of(&update_p);
        let witness = signer_witness(early.native.core(), &update_p, &inst.config.spec());
        let jid = job_id(&inst.id, &on_chain.digest(), &compact.digest(fork));
        let mut t = tx(
            "fresh Open before its signature slot (validity window)",
            vec![inst.pda(), inst.job(&jid)],
            borsh::to_vec(&Message::Open { update: compact.encode(fork), signers: witness.encode() })?,
            payer,
        );
        t.note = "the period update on an instance whose slots start ten years later".into();
        let out = live.submit(t).await?;
        ensure!(
            out.row.status == "reverted" && out.row.note.contains("validity window"),
            "expected LEZ's validity-window revert, got {} {}",
            out.row.status,
            out.row.note
        );
        ensure!(out.row.exec_gas == live.opts.gas_limit.to_string(), "LEZ's own refusals are charged the whole gas limit");
    }

    // Duplicated and replayed steps around the finality update, which applies.
    {
        let update = fresh.finality.clone();
        let before = b.native.core().clone();
        let inst = b.inst.clone();
        let stored = store(live, &inst).await?;
        let fork = layout_for(&inst.config, &stored, &update);
        let on_chain = steps::store_for_update(&stored, fork, update.attested_header.beacon.slot, &spec).unwrap_or(stored);
        let compact = CompactUpdate::of(&update);
        let witness = signer_witness(&before, &update, &b.native.spec);
        let jid = job_id(&inst.id, &on_chain.digest(), &compact.digest(fork));
        let job = inst.job(&jid);
        let header = inst.header(update.finalized_header.beacon.slot);
        let open_msg = borsh::to_vec(&Message::Open { update: compact.encode(fork), signers: witness.encode() })?;
        let finish = borsh::to_vec(&Message::Finish)?;
        let miller = borsh::to_vec(&Message::Miller)?;
        live.submit(tx("fresh finality update Open (step 1)", vec![inst.pda(), job], open_msg.clone(), payer))
            .await?
            .expect_applied()?;
        live.submit(tx("fresh duplicate Open (same job)", vec![inst.pda(), job], open_msg.clone(), payer))
            .await?
            .expect_exit(code::JOB_EXISTS)?;
        live.submit(tx("fresh Finish before Miller (skipped step)", vec![job, inst.pda()], finish.clone(), payer))
            .await?
            .expect_exit(81)?;
        live.submit(tx("fresh finality update Miller (step 2)", vec![job], miller.clone(), payer)).await?.expect_applied()?;
        live.submit(tx("fresh Miller replayed", vec![job], miller, payer)).await?.expect_exit(81)?;
        live.submit(tx("fresh finality update Finish (step 3)", vec![job, inst.pda(), header], finish.clone(), payer))
            .await?
            .expect_applied()?;
        live.submit(tx("fresh Finish replayed after the job applied", vec![job, inst.pda(), header], finish, payer))
            .await?
            .expect_exit(code::NO_JOB)?;
        live.submit(tx("fresh Open an applied update again", vec![inst.pda(), job], open_msg, payer))
            .await?
            .expect_exit(52)?;
        let outcome = native_process(&mut b.native, &inst.config, &update).context("the native store and the finality update")?;
        ensure!(outcome.advanced && !outcome.adopted, "a finality update advances and adopts nothing");
        b.jobs.push(jid);
        check_store(live, &b).await?;
        check_record(live, &b, update.finalized_header.beacon.slot).await?;
    }
    live.save()
}

// ---------------------------------------------------------------------------------------------
// fresh-consume: the attestation consumed publicly
// ---------------------------------------------------------------------------------------------

/// The record of `inst` at `slot`, as the chain holds it.
async fn record(live: &Live, inst: &Instance, slot: u64) -> Result<HeaderRecord> {
    let state = live.actor_state(inst.header(slot).actor()).await?;
    ensure!(!state.is_empty(), "instance {}: no record at slot {slot} (run fresh-follow first)", inst.name);
    let state: RecordState = borsh::from_slice(&state)?;
    state.header().map_err(|c| anyhow!("the record at slot {slot} does not decode ({c})"))
}

async fn consumer_state(live: &Live, pda: Pda) -> Result<Option<ConsumerState>> {
    let state = live.actor_state(pda.actor()).await?;
    if state.is_empty() {
        return Ok(None);
    }
    Ok(Some(borsh::from_slice(&state)?))
}

pub async fn consume(
    live: &mut Live,
    main: AccountId,
    twin: AccountId,
    consumer_bin: &GuestBinary,
    fresh: &Fresh,
    payer: AccountId,
) -> Result<()> {
    let inst = fresh.instance("A", main, twin);
    let slot = fresh.finalized_slot;
    let recorded = record(live, &inst, slot).await?;
    let header = inst.header(slot);
    let expect = |request: &Request| evaluate(&inst.id, SEPOLIA_CHAIN_ID, &recorded, request);

    // 1. `Attest` as the root of a transaction: each accepted request applies and emits one event.
    let storage = fresh.storage_request(inst.id)?;
    let log = fresh.log_request(inst.id);
    let both = Request { accounts: storage.accounts.clone(), logs: log.logs.clone(), ..fresh.request(inst.id) };
    for (step, request) in [
        ("fresh Attest: storage slot (Sepolia USDC totalSupply_), with the block header", &storage),
        ("fresh Attest: an ERC-20 Transfer log (emitter, topic 0, recipient), with the block header", &log),
        ("fresh Attest: the slot and the log in one request", &both),
    ] {
        let expected = expect(request).map_err(|c| anyhow!("{step}: refused natively ({c})"))?;
        let out = live.submit(tx(step, vec![header], borsh::to_vec(&Message::Attest { request: request.clone() })?, payer)).await?;
        let out = out.expect_applied()?;
        let events = out.receipt.as_ref().map_or(0, |r| r.events);
        ensure!(events >= 1, "{step}: no event");
        println!("[fresh] {step}: applied, {} statement(s), block {}", expected.statements.len(), expected.block_number);
    }
    // Its refusals, each with the code the native evaluation gives.
    let rlp = fresh.header.clone();
    let mut tampered = rlp.clone();
    tampered[rlp.len() / 2] ^= 1;
    let cases = [
        ("fresh Attest refused: no block header for a record that binds a block hash", Request { header: None, ..storage.clone() }),
        ("fresh Attest refused: the block header with one byte changed", Request { header: Some(tampered), ..storage.clone() }),
        ("fresh Attest refused: the same proof asked as a mainnet statement (chain id 1)", Request { chain_id: 1, ..storage.clone() }),
        (
            "fresh Attest refused: a false value for the slot",
            Request {
                accounts: vec![AccountQuery {
                    slots: vec![SlotQuery {
                        conditions: vec![Condition { field: Field::Value, cmp: Cmp::Ne, rhs: fresh.value }],
                        ..storage.accounts[0].slots[0].clone()
                    }],
                    ..storage.accounts[0].clone()
                }],
                ..storage.clone()
            },
        ),
    ];
    for (step, request) in cases {
        let code = match expect(&request) {
            Err(code) => code,
            Ok(_) => bail!("{step}: the native evaluation accepts it"),
        };
        live.submit(tx(step, vec![header], borsh::to_vec(&Message::Attest { request })?, payer)).await?.expect_exit(code)?;
    }

    // 2. The reference consumer: deployed, its public reader calls `Attest` and keeps the whole reply.
    let consumer_at = deploy(live, "consumer", consumer_bin, payer).await?;
    let reader = Pda { program: consumer_at, seed: PdaSeed::new(READER_SEED) };
    for (step, request) in [
        ("fresh consumer CallAttest: storage slot", &storage),
        ("fresh consumer CallAttest: the Transfer log", &log),
    ] {
        let expected: Attestation = expect(request).map_err(|c| anyhow!("{step}: refused natively ({c})"))?;
        let message = ConsumerMessage::CallAttest { ferrata: main, slot, request: request.clone() };
        live.submit(tx(step, vec![reader, header], borsh::to_vec(&message)?, payer)).await?.expect_applied()?;
        let Some(ConsumerState::Reader(state)) = consumer_state(live, reader).await? else {
            bail!("{step}: the reader holds no reader state");
        };
        ensure!(state.attestation == Some(borsh::to_vec(&expected)?), "{step}: the reply kept is not the whole attestation");
        println!("[fresh] {step}: the consumer keeps the program's reply ({} statement(s))", expected.statements.len());
    }

    // 3. Its claim of the Transfer: credits the sender once; the same claim again is refused by its nullifier.
    let pool = fresh.pool(main, inst.id);
    let attested = expect(&log).map_err(|c| anyhow!("the Transfer: refused natively ({c})"))?;
    let deposit: Deposit = pool.accept(&attested).map_err(|c| anyhow!("the pool does not accept the Transfer ({c})"))?;
    let pool_id = pool.id();
    let spent = nullifier(&pool_id, &deposit.statement_id);
    let claim = Pda { program: consumer_at, seed: claim_seed(&pool_id, &spent) };
    let balance = Pda { program: consumer_at, seed: balance_seed(&pool_id, &deposit.owner) };
    let t = &fresh.transfer;
    let message = borsh::to_vec(&ConsumerMessage::Claim {
        pool: pool.clone(),
        slot,
        tx_index: t.tx_index,
        log_index: t.log_index,
        proof: fresh.receipt_proof(t.tx_index),
        header: Some(fresh.header.clone()),
    })?;
    ensure!(consumer_state(live, claim).await?.is_none(), "the claim's nullifier is unused before the claim");
    live.submit(tx(
        "fresh consumer Claim: the ERC-20 Transfer (credits its sender once)",
        vec![claim, header, balance],
        message.clone(),
        payer,
    ))
    .await?
    .expect_applied()?;
    let Some(ConsumerState::Claim(ClaimState::Spent(held))) = consumer_state(live, claim).await? else {
        bail!("the claim's nullifier is not spent");
    };
    ensure!(held == deposit, "the claim holds another deposit: {held:?}");
    let credited = |state: Option<ConsumerState>| match state {
        Some(ConsumerState::Balance(b)) => Some((b.amount, b.credits)),
        _ => None,
    };
    ensure!(
        credited(consumer_state(live, balance).await?) == Some((deposit.amount, 1)),
        "the sender is not credited the Transfer's amount once"
    );
    live.submit(tx("fresh consumer Claim again (replay)", vec![claim, header, balance], message, payer))
        .await?
        .expect_exit(consumer::code::ALREADY_CLAIMED)?;
    ensure!(credited(consumer_state(live, balance).await?) == Some((deposit.amount, 1)), "credited once");
    println!(
        "[fresh] the consumer credited 0x{} with {} of token 0x{} once; the replay refused",
        hex::encode(deposit.owner),
        deposit.amount,
        hex::encode(t.token)
    );
    live.save()
}

// ---------------------------------------------------------------------------------------------
// fresh-private: the reference consumer's AttestPrivately, proven for real
// ---------------------------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub async fn private(
    live: &mut Live,
    main: AccountId,
    twin: AccountId,
    program: &GuestBinary,
    consumer_bin: &GuestBinary,
    fresh: &Fresh,
    payer: AccountId,
    private_account: usize,
) -> Result<()> {
    ensure!(
        !matches!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1") | Ok("true")),
        "RISC0_DEV_MODE must be unset: the private attestation is proven for real"
    );
    let inst = fresh.instance("A", main, twin);
    let slot = fresh.finalized_slot;
    let recorded = record(live, &inst, slot).await?;
    let consumer_at = deploy(live, "consumer", consumer_bin, payer).await?;
    let t = &fresh.transfer;
    let pool = fresh.pool(main, inst.id);
    evaluate(&inst.id, SEPOLIA_CHAIN_ID, &recorded, &fresh.log_request(inst.id))
        .map_err(|c| anyhow!("the Transfer does not attest under A's record ({c})"))?;
    let message = borsh::to_vec(&ConsumerMessage::AttestPrivately {
        pool,
        slot,
        tx_index: t.tx_index,
        log_index: t.log_index,
        proof: fresh.receipt_proof(t.tx_index),
        header: Some(fresh.header.clone()),
    })?;
    let record_pda = inst.header(slot);

    let dir = live.opts.run_dir.join("private-attest");
    fs::create_dir_all(&dir)?;
    let transcript = dir.join("rpc-transcript.jsonl");
    let tx_out = dir.join("transaction.borsh");
    let _ = fs::remove_file(&tx_out);
    let client = TapClient::recording(live.wallet.client_owned(), &transcript, &tx_out)?;
    let (mut wallet, me) = private_wallet_at(&dir.join("wallet"), &live.opts.rpc, client, private_account)?;
    let tip = wallet.start_at_chain_tip().await?;
    #[cfg(feature = "lez-040")]
    crate::private::receive_native_credits("[fresh]", live, &mut wallet, me).await?;
    let catalog = ProgramCatalog::from([
        (consumer_at, Program::new(Cow::Owned(consumer_bin.bytes.clone()))?),
        (main, Program::new(Cow::Owned(program.bytes.clone()))?),
    ]);
    let mentions = || vec![AccountIdentity::PrivateOwned(me).select_program_actor_state(consumer_at), mention(record_pda)];
    println!(
        "[fresh] AttestPrivately of the Transfer (block {}, tx {} log {}) at the private account {me}'s actor of {consumer_at}; wallet synced at block {tip}; a real proof",
        fresh.block, t.tx_index, t.log_index
    );

    wallet.set_dry_run(true);
    match wallet.send_privacy_preserving_tx_with_pre_check(mentions(), 0, message.clone(), &catalog, None, |_| Ok(())).await {
        #[cfg(feature = "lez-rc2")]
        Err(wallet::ExecutionFailureKind::DryRun(_)) => {}
        #[cfg(not(feature = "lez-rc2"))]
        Err(wallet::ExecutionFailureKind::DryRun) => {}
        Err(error) => bail!("the wallet's dry run failed: {error}: {error:?}"),
        Ok(_) => bail!("a dry run sent a transaction"),
    }
    wallet.set_dry_run(false);

    let step = "fresh private attestation (consumer AttestPrivately), real proof";
    live.rows.push(Row {
        phase: live.phase.clone(),
        step: step.to_string(),
        status: "unfinished".into(),
        note: "the proof was started and had not finished when the run was recorded".into(),
        ..Row::default()
    });
    live.save()?;
    let pending_index = live.rows.len() - 1;
    let started = Instant::now();
    let sent = wallet.send_privacy_preserving_tx_with_pre_check(mentions(), 0, message, &catalog, None, |_| Ok(())).await;
    let seconds = started.elapsed().as_secs_f64();
    let since_head = live.head().await?;
    live.rows.remove(pending_index);
    let hash = match sent {
        Ok((hash, _)) => hash,
        Err(error) => {
            let row = Row {
                phase: live.phase.clone(),
                step: step.to_string(),
                status: "rejected".into(),
                note: format!("after {seconds:.1} s: {error}: {error:?}"),
                ..Row::default()
            };
            println!("[fresh] {step}: rejected: {}", row.note);
            live.rows.push(row);
            live.save()?;
            bail!("the private attestation was not sent");
        }
    };
    let described = describe_private(&tx_out)?;
    println!("[fresh] proved and sent in {seconds:.1} s: {described}");
    let outcome = live
        .settle(Sent {
            step: step.to_string(),
            hash,
            sent_at: Instant::now(),
            since_head,
            payer: None,
            balance_before: 0,
            harness: None,
            note: format!("execute_and_prove and submission {seconds:.1} s; {described}"),
        })
        .await?;
    println!("[fresh] {step}: {} in block {}", outcome.row.status, outcome.row.block);
    ensure!(outcome.applied(), "the private attestation was not applied: {} {}", outcome.row.status, outcome.row.note);
    ensure!(instance_state(live, &inst).await?.is_some_and(|s| s.active), "instance A is active");
    live.save()
}
