//! Sepolia through its Gloas fork with the step API (`ferrata_core::steps`), as an instance of
//! the LEZ program follows it (`fixtures/sepolia-gloas-2026-11-10`, the data of
//! `tests/sepolia_gloas.rs`).
//!
//! The compact store starts in the Electra layout, from the bootstrap of the last pre-Gloas
//! period, and is never told to upgrade. It takes each period's update in the layout the beacon
//! node served it in: `steps::store_for_update` gives the store the update is verified against,
//! which for the first Gloas update is the store upgraded to the Gloas layout, and `steps::apply`
//! makes that upgrade the store's when the verified job is applied. After every update the store
//! must be the compact form of the policy store, which is upgraded by hand at the fork as
//! `tests/sepolia_gloas.rs` does. The record of the last finalized header binds a block hash
//! only; with the block's real RLP header it gives the execution view the policy store gives.

use std::{fs, path::PathBuf};

use ferrata_core::{
    attest,
    bls::{Aggregation, Blst},
    fork::{ChainSpec, LcFork, Preset, FAR_FUTURE_EPOCH},
    merkle::ZERO,
    policy::{Adoption, FinalizedStore},
    steps::{self, HeaderRecord, Job, StepError},
    store::StoreCore,
    types::{LightClientBootstrap, LightClientHeader, LightClientUpdate, SyncAggregate, SyncCommittee},
    upgrade::upgrade_update,
    validate::Context,
    witness::{uncompressed_keys, CompactBootstrap, CompactStore, CompactUpdate, ExecutionSummary, SignerWitness},
    Error, Root,
};
use ferrata_spectests::execkit::header_rlp;
use serde_json::Value;
use sha2::{Digest, Sha256};

const SEPOLIA: u64 = 11_155_111;
const GVR: &str = "0xd8ea171f3c94aea21ebc42a1ed61052acf3f9209c00e4efbaaddac09ed9b8078";
const BOOTSTRAP_ROOT: &str = "0x5f71d7f650f9252b48749b5f062c0c234e01afd69f364ab600e2b6051494b20d";
const FINALIZED_SLOT: u64 = 11_547_104;
const BLOCK_NUMBER: u64 = 12_103_422;
const BLOCK_HASH: &str = "0xcd9fa3661248bb2411636d3498a1dda97f0ee50d02b8ac894969e391f2978435";
const FIRST: u64 = 1378;
const GLOAS: u64 = 1379;
const LAST: u64 = 1409;
const PARTS: u8 = 2;

/// Sepolia's fork schedule (eth-clients/sepolia `metadata/config.yaml` at 7b1b9d9).
fn sepolia() -> ChainSpec {
    let v = |b: u8| [0x90, 0x00, 0x00, b];
    ChainSpec {
        preset: Preset::MAINNET,
        versions: [v(0x69), v(0x70), v(0x71), v(0x72), v(0x73), v(0x74), v(0x75), v(0x76), [0; 4]],
        epochs: [0, 50, 100, 56_832, 132_608, 222_464, 272_640, 353_024, FAR_FUTURE_EPOCH],
    }
}

fn root(s: &str) -> Root {
    hex::decode(s.trim_start_matches("0x")).unwrap().try_into().unwrap()
}

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/sepolia-gloas-2026-11-10")
}

/// A fixture file, checked against the pinned `SHA256SUMS`.
fn fixture(name: &str) -> Vec<u8> {
    let bytes = fs::read(dir().join(name)).unwrap();
    let sums = fs::read_to_string(dir().join("SHA256SUMS")).unwrap();
    let want = sums.lines().find(|l| l.ends_with(&format!("  {name}"))).unwrap().split_whitespace().next().unwrap();
    assert_eq!(hex::encode(Sha256::digest(&bytes)), want, "{name} does not match SHA256SUMS");
    bytes
}

/// The layout the node served a period's update in: Electra before the fork, Gloas from 1379.
fn layout(period: u64) -> LcFork {
    if period < GLOAS {
        LcFork::Electra
    } else {
        LcFork::Gloas
    }
}

fn update(period: u64) -> LightClientUpdate {
    LightClientUpdate::decode(&fixture(&format!("update_{period}.ssz")), layout(period), 512).unwrap()
}

/// The Gloas `LightClientFinalityUpdate`: every field is fixed-size.
fn finality_update() -> LightClientUpdate {
    const H: usize = 112 + 32 + 11 * 32;
    let b = fixture("finality_update.ssz");
    assert_eq!(b.len(), 2 * H + 9 * 32 + 160 + 8);
    LightClientUpdate {
        attested_header: LightClientHeader::decode(&b[..H], LcFork::Gloas).unwrap(),
        next_sync_committee: SyncCommittee::empty(512),
        next_sync_committee_branch: vec![ZERO; LcFork::Gloas.sync_committee_branch_len()],
        finalized_header: LightClientHeader::decode(&b[H..2 * H], LcFork::Gloas).unwrap(),
        finality_branch: b[2 * H..2 * H + 288].chunks_exact(32).map(|c| c.try_into().unwrap()).collect(),
        sync_aggregate: SyncAggregate::decode(&b[2 * H + 288..2 * H + 448], 512).unwrap(),
        signature_slot: u64::from_le_bytes(b[2 * H + 448..].try_into().unwrap()),
    }
}

/// The policy store and the compact store an `Init` job produces, from the pre-Gloas bootstrap.
fn init(spec: &ChainSpec, bls: &Blst) -> (FinalizedStore, CompactStore) {
    let boot = LightClientBootstrap::decode(&fixture("bootstrap.ssz"), LcFork::Electra, 512).unwrap();
    let trusted = root(BOOTSTRAP_ROOT);
    let policy =
        FinalizedStore::initialize_with(&trusted, &boot, LcFork::Electra, spec, bls, Adoption::OnChain).unwrap();
    let (store, mut job) =
        steps::initialize(&trusted, &CompactBootstrap::of(&boot), LcFork::Electra, spec, PARTS).unwrap();
    let keys = uncompressed_keys(&boot.current_sync_committee).unwrap();
    for p in 0..PARTS as usize {
        job.adopt(p as u8, &keys[p * 256..(p + 1) * 256], 512).unwrap();
    }
    let (active, _) = steps::apply(&store, &job, spec).unwrap();
    assert_eq!(active, CompactStore::of(policy.core()));
    assert_eq!(active.fork, LcFork::Electra);
    (policy, active)
}

/// One update as a job on `base`, run to the end with honest witnesses from `full`, the policy
/// store's state before the update.
fn job_on(
    base: &CompactStore,
    full: &StoreCore,
    update: &LightClientUpdate,
    spec: &ChainSpec,
) -> Result<Job, StepError> {
    let committee = if spec.period_at_slot(update.signature_slot) == spec.period_at_slot(full.finalized_header.beacon.slot)
    {
        &full.current_sync_committee
    } else {
        &full.next_sync_committee
    };
    let witness = SignerWitness::build(committee, &update.sync_aggregate.bits).unwrap_or_default();
    let compact = CompactUpdate::of(update);
    let encoded = compact.encode(base.fork);
    assert_eq!(CompactUpdate::layout_of(&encoded), Ok(base.fork), "an update's encoding names its layout");
    assert_eq!(CompactUpdate::decode(&encoded, base.fork, 512).as_ref(), Ok(&compact));
    let mut job = steps::step1(base, &compact, &witness, update.signature_slot, spec, &root(GVR), PARTS)?;
    job.step2()?;
    job.step3()?;
    if job.adoption_pending() {
        let keys = uncompressed_keys(&update.next_sync_committee).unwrap();
        for p in 0..PARTS as usize {
            job.adopt(p as u8, &keys[p * 256..(p + 1) * 256], 512)?;
        }
    }
    Ok(job)
}

/// The update as an instance takes it: verified against `store_for_update`'s store, applied to
/// the stored one. Returns the job; `stored` becomes the store after it.
fn take(
    stored: &mut CompactStore,
    full: &StoreCore,
    update: &LightClientUpdate,
    layout: LcFork,
    spec: &ChainSpec,
) -> Result<Job, StepError> {
    let base = steps::store_for_update(stored, layout, update.attested_header.beacon.slot, spec)?;
    assert_eq!(base.fork, layout);
    let job = job_on(&base, full, update, spec)?;
    let (next, _) = steps::apply(stored, &job, spec)?;
    assert_eq!(steps::apply(&base, &job, spec).map(|(s, _)| s), Ok(next.clone()), "the same from the upgraded store");
    *stored = next;
    Ok(job)
}

/// The stores after `Init` and the updates of periods 1378 to `last`.
fn follow(spec: &ChainSpec, bls: &Blst, last: u64) -> (FinalizedStore, CompactStore) {
    let ctx = Context { spec, genesis_validators_root: root(GVR), bls };
    let (mut policy, mut compact) = init(spec, bls);
    for period in FIRST..=last {
        if period == GLOAS {
            policy.upgrade(LcFork::Gloas, spec).unwrap();
        }
        let u = update(period);
        let before = policy.core().clone();
        policy.process_update(&u, u.signature_slot + 1, &ctx).unwrap_or_else(|e| panic!("period {period}: {e}"));
        let was = compact.fork;
        let job = take(&mut compact, &before, &u, layout(period), spec).unwrap_or_else(|e| panic!("{period}: {e:?}"));
        assert_eq!(compact, CompactStore::of(policy.core()), "period {period}");
        assert_eq!((was, compact.fork), (layout(period.max(GLOAS) - 1), layout(period)), "period {period}");
        let record = HeaderRecord::of(&policy.core().finalized_header, policy.core().fork, spec);
        assert_eq!(job.pending.record(), &record, "period {period}: the record the job writes");
    }
    (policy, compact)
}

#[test]
fn the_compact_store_follows_sepolia_through_the_fork_by_itself() {
    let spec = sepolia();
    let bls = Blst::new(Aggregation::Auto);
    let ctx = Context { spec: &spec, genesis_validators_root: root(GVR), bls: &bls };
    let (mut policy, mut compact) = follow(&spec, &bls, LAST);
    assert_eq!(compact.fork, LcFork::Gloas);

    // The latest finality update, and the record it writes: a block hash only.
    let fu = finality_update();
    let before = policy.core().clone();
    policy.process_update(&fu, fu.signature_slot + 1, &ctx).unwrap();
    let job = take(&mut compact, &before, &fu, LcFork::Gloas, &spec).unwrap();
    assert_eq!(compact, CompactStore::of(policy.core()));
    assert_eq!(compact.finalized.slot, FINALIZED_SLOT);
    let record = job.pending.record().clone();
    assert_eq!(record.execution, ExecutionSummary::BlockHash(root(BLOCK_HASH)));
    assert_eq!(HeaderRecord::decode(&record.encode()).as_ref(), Ok(&record));
    println!(
        "Sepolia through the steps: Init at period {FIRST} (Electra layout), {} updates, the store upgraded \
         itself to the Gloas layout with the update of period {GLOAS}; finalized slot {FINALIZED_SLOT}, record of \
         {} bytes binding block hash {BLOCK_HASH}",
        LAST - FIRST + 2,
        record.encode().len()
    );

    // With the block's real RLP header, the record gives the execution view the policy store
    // gives, and the fields two execution providers report.
    let rlp = fixture("header.rlp");
    let view = attest::HeaderRecord::from_recorded_block_hash(&record, SEPOLIA, &rlp).unwrap();
    assert_eq!(view, attest::HeaderRecord::from_block_hash(&policy, SEPOLIA, &rlp).unwrap());
    assert_eq!((view.number(), view.block_hash(), view.binding()), (BLOCK_NUMBER, root(BLOCK_HASH), attest::Binding::BlockHash));
    for name in ["block.json", "block_second.json"] {
        let block: Value = serde_json::from_slice(&fixture(name)).unwrap();
        let field = |f: &str| root(block["result"][f].as_str().unwrap());
        assert_eq!(view.state_root(), field("stateRoot"), "{name}");
        assert_eq!(view.receipts_root(), field("receiptsRoot"), "{name}");
        assert_eq!(view.parent_hash(), field("parentHash"), "{name}");
    }
}

#[test]
fn a_gloas_record_needs_the_header_of_its_own_block() {
    let spec = sepolia();
    let bls = Blst::new(Aggregation::Auto);
    let ctx = Context { spec: &spec, genesis_validators_root: root(GVR), bls: &bls };
    let (mut policy, mut compact) = follow(&spec, &bls, LAST);
    let fu = finality_update();
    let before = policy.core().clone();
    policy.process_update(&fu, fu.signature_slot + 1, &ctx).unwrap();
    let record = take(&mut compact, &before, &fu, LcFork::Gloas, &spec).unwrap().pending.record().clone();
    let rlp = fixture("header.rlp");
    let view = |header: &[u8]| attest::HeaderRecord::from_recorded_block_hash(&record, SEPOLIA, header);
    assert!(view(&rlp).is_ok());

    // Without a header there is no view: the constructor of the payload layouts refuses.
    assert_eq!(attest::HeaderRecord::from_recorded(&record, SEPOLIA), Err(Error::BindingMismatch));
    // A flipped byte anywhere, a missing last byte, an extra byte.
    for at in [0, 5, 100, rlp.len() / 2, rlp.len() - 1] {
        let mut t = rlp.clone();
        t[at] ^= 1;
        assert_eq!(view(&t), Err(Error::HeaderHashMismatch), "byte {at}");
    }
    assert_eq!(view(&rlp[..rlp.len() - 1]), Err(Error::HeaderHashMismatch), "truncated");
    assert_eq!(view(&[rlp.as_slice(), &[0]].concat()), Err(Error::HeaderHashMismatch), "over-long");
    assert_eq!(view(&[]), Err(Error::HeaderHashMismatch), "empty");
    // A well-formed header of another block: the same block's fields with the next number.
    let mut other: Value = serde_json::from_slice::<Value>(&fixture("block.json")).unwrap()["result"].clone();
    assert_eq!(header_rlp(&other), rlp, "the header is rebuilt from the provider's fields");
    other["number"] = Value::String(format!("{:#x}", BLOCK_NUMBER + 1));
    let other = header_rlp(&other);
    assert!(ferrata_core::exec::decode_header(&other).is_ok_and(|h| h.number == BLOCK_NUMBER + 1));
    assert_eq!(view(&other), Err(Error::HeaderHashMismatch), "another block's header");

    // The constructor reads Gloas records only, and only ones that bind a block.
    let (electra, _) = init(&spec, &bls);
    let payload = HeaderRecord::of(&electra.core().finalized_header, LcFork::Electra, &spec);
    assert!(matches!(payload.execution, ExecutionSummary::Payload { .. }));
    assert!(attest::HeaderRecord::from_recorded(&payload, SEPOLIA).is_ok());
    assert_eq!(
        attest::HeaderRecord::from_recorded_block_hash(&payload, SEPOLIA, &rlp),
        Err(Error::BindingMismatch),
        "a payload record"
    );
    let unbound = HeaderRecord { execution: ExecutionSummary::BlockHash(ZERO), ..record.clone() };
    assert_eq!(
        attest::HeaderRecord::from_recorded_block_hash(&unbound, SEPOLIA, &rlp),
        Err(Error::BindingMismatch),
        "a record that binds no block"
    );
}

#[test]
fn a_store_changes_layout_only_as_its_schedule_says() {
    let spec = sepolia();
    let bls = Blst::new(Aggregation::Auto);
    let ctx = Context { spec: &spec, genesis_validators_root: root(GVR), bls: &bls };
    let (policy, electra) = follow(&spec, &bls, FIRST);
    assert_eq!(electra.fork, LcFork::Electra);
    let (u1378, u1379) = (update(FIRST), update(GLOAS));
    let attested = |u: &LightClientUpdate| u.attested_header.beacon.slot;
    let refusal = |r: Result<CompactStore, Error>| r.map(|_| ()).unwrap_err();

    // The first Gloas update moves the store, and only in the Gloas layout.
    assert_eq!(
        steps::store_for_update(&electra, LcFork::Gloas, attested(&u1379), &spec),
        electra.upgraded(LcFork::Gloas)
    );
    assert_eq!(steps::store_for_update(&electra, LcFork::Electra, attested(&u1379), &spec), Ok(electra.clone()));
    // Step 1 in the store's own layout refuses the Gloas update's shape, as the policy store does.
    let gloas_as_electra =
        steps::step1(&electra, &CompactUpdate::of(&u1379), &SignerWitness::default(), 0, &spec, &root(GVR), PARTS);
    assert_eq!(gloas_as_electra.unwrap_err(), StepError::Core(Error::ForkMismatch));

    // A schedule without Gloas (the same chain, the fork not scheduled): no upgrade.
    let mut unscheduled = spec.clone();
    unscheduled.epochs[7] = FAR_FUTURE_EPOCH;
    assert_eq!(
        refusal(steps::store_for_update(&electra, LcFork::Gloas, attested(&u1379), &unscheduled)),
        Error::ForkMismatch
    );
    // An update attested before the fork cannot move the store, whatever layout it is sent in.
    assert_eq!(refusal(steps::store_for_update(&electra, LcFork::Gloas, attested(&u1378), &spec)), Error::ForkMismatch);
    // A layout the store has left, and a layout that does not exist at that slot.
    let gloas = electra.upgraded(LcFork::Gloas).unwrap();
    assert_eq!(refusal(steps::store_for_update(&gloas, LcFork::Electra, attested(&u1379), &spec)), Error::ForkMismatch);
    assert_eq!(refusal(steps::store_for_update(&electra, LcFork::Deneb, attested(&u1379), &spec)), Error::ForkMismatch);
    assert_eq!(gloas.upgraded(LcFork::Electra), Err(Error::ForkMismatch), "no downgrade");

    // A job names the store it started from, layout included.
    let job = job_on(&gloas, policy.core(), &u1379, &spec).unwrap();
    let (applied, _) = steps::apply(&electra, &job, &spec).unwrap();
    assert_eq!(applied.fork, LcFork::Gloas, "applying the job upgrades the stored store");
    let mut other = electra.clone();
    other.finalized.proposer_index ^= 1;
    assert_eq!(steps::apply(&other, &job, &spec).unwrap_err(), StepError::StoreChanged, "another store");
    assert_eq!(steps::apply(&applied, &job, &spec).unwrap_err(), StepError::StoreChanged, "replayed");

    // An update served before the fork still fits a store that is already in the Gloas layout,
    // once its submitter has upgraded it: the policy store, upgraded before the fork as the
    // specification allows, takes the period-1378 update that way, and so do the steps.
    let (mut early, compact) = init(&spec, &bls);
    early.upgrade(LcFork::Gloas, &spec).unwrap();
    let upgraded = upgrade_update(u1378.clone(), LcFork::Electra, LcFork::Gloas, &spec).unwrap();
    let before = early.core().clone();
    early.process_update(&upgraded, upgraded.signature_slot + 1, &ctx).unwrap();
    let mut early_compact = compact.upgraded(LcFork::Gloas).unwrap();
    assert_eq!(early_compact, CompactStore::of(&before));
    let job = take(&mut early_compact, &before, &upgraded, LcFork::Gloas, &spec).unwrap();
    assert_eq!(early_compact, CompactStore::of(early.core()));
    // Its record binds the block hash of the finalized header's own payload.
    let electra_record = HeaderRecord::of(&policy.core().finalized_header, LcFork::Electra, &spec);
    let ExecutionSummary::Payload { block_hash, .. } = electra_record.execution else { panic!("a payload record") };
    assert_eq!(job.pending.record().execution, ExecutionSummary::BlockHash(block_hash));
    assert_eq!(job.pending.record().beacon, electra_record.beacon);
    // In its own layout the same update is refused by that store.
    assert_eq!(
        take(&mut compact.upgraded(LcFork::Gloas).unwrap(), &before, &u1378, LcFork::Electra, &spec).unwrap_err(),
        StepError::Core(Error::ForkMismatch)
    );
}
