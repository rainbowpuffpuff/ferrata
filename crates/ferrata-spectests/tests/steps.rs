//! The step API (`ferrata_core::steps`) against `FinalizedStore::process_update`, and the compact
//! encodings and uncompressed-key adoption of `ferrata_core::witness`.
//!
//! The property: an update run as a job (step 1, Miller loops, final exponentiation, committee
//! adoption from uncompressed keys, apply) is accepted exactly when `process_update` with
//! `Adoption::OnChain` accepts it, with the same outcome, and the job's store is the compact form
//! of `process_update`'s store. Refusal codes are equal for the inputs here, each of which has a
//! single defect. On every job the tests also check that a step cannot be skipped or replayed and
//! that a job cannot be applied to a store with another digest.

use std::{fs, path::PathBuf};

use blst::min_pk::PublicKey;
use ferrata_core::{
    bls::{Aggregation, BlsBackend, Blst},
    fork::{ChainSpec, LcFork, Preset, FAR_FUTURE_EPOCH},
    policy::{Adoption, FinalizedStore},
    steps::{self, HeaderRecord, Job, Phase, StepError},
    store::{ApplyOutcome, StoreCore},
    types::{BlsPubkey, LightClientBootstrap, LightClientUpdate, SyncCommittee},
    upgrade::{upgrade_bootstrap, upgrade_update},
    validate::{sync_committee_signing_root, Context},
    witness::{
        adopt_committee, uncompress_key, uncompressed_keys, AdoptionProgress, CommitteeSummary, CompactBootstrap,
        CompactStore, CompactUpdate, SignerWitness,
    },
    Error, Root,
};
use ferrata_spectests::{cases, parse_config, read_ssz_snappy, testkit::fake_committee};
use serde_yaml::Value;
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------------------------
// Running an update as a job
// ---------------------------------------------------------------------------------------------

fn roundtrip(job: &Job) {
    assert_eq!(Job::decode(&job.encode()).as_ref(), Ok(job), "job encoding must round-trip");
}

/// The committee that signs `update` in the full store, if it is known.
fn signing_committee<'a>(
    full: &'a StoreCore,
    update: &LightClientUpdate,
    spec: &ChainSpec,
) -> Option<&'a SyncCommittee> {
    let store_period = spec.period_at_slot(full.finalized_header.beacon.slot);
    let c = if spec.period_at_slot(update.signature_slot) == store_period {
        &full.current_sync_committee
    } else {
        &full.next_sync_committee
    };
    (!c.is_empty()).then_some(c)
}

/// What a program does with one update: the job, run to its end, with honest witnesses built from
/// the full store `full` (the policy store before the update).
#[allow(clippy::too_many_arguments)]
fn run_job(
    store: &CompactStore,
    full: &StoreCore,
    update: &LightClientUpdate,
    witness: Option<SignerWitness>,
    current_slot: u64,
    spec: &ChainSpec,
    gvr: &Root,
    parts: u8,
) -> Result<(CompactStore, ApplyOutcome, Job), StepError> {
    let n = spec.preset.sync_committee_size;
    let compact = CompactUpdate::of(update);
    assert_eq!(CompactUpdate::decode(&compact.encode(store.fork), store.fork, n).as_ref(), Ok(&compact));
    let witness = witness.unwrap_or_else(|| {
        signing_committee(full, update, spec)
            .and_then(|c| SignerWitness::build(c, &update.sync_aggregate.bits).ok())
            .unwrap_or_default()
    });
    assert_eq!(SignerWitness::decode(&witness.encode(), n).as_ref(), Ok(&witness));

    let mut job = steps::step1(store, &compact, &witness, current_slot, spec, gvr, parts)?;
    roundtrip(&job);
    assert_eq!(job.clone().step3(), Err(StepError::Phase), "the Miller loops cannot be skipped");
    assert_eq!(steps::apply(store, &job, spec).unwrap_err(), StepError::Phase, "nor the whole check");
    job.step2()?;
    roundtrip(&job);
    assert_eq!(job.clone().step2(), Err(StepError::Phase), "the Miller loops cannot be replayed");
    job.step3()?;
    roundtrip(&job);
    assert_eq!(job.clone().step3(), Err(StepError::Phase), "the final exponentiation cannot be replayed");
    if job.adoption_pending() {
        assert_eq!(steps::apply(store, &job, spec).unwrap_err(), StepError::AdoptionIncomplete);
        // An honest witness exists only for keys that decompress: else adoption fails, as
        // `check_committee_onchain` does.
        let keys = uncompressed_keys(&update.next_sync_committee).map_err(|_| Error::CommitteeInvalid)?;
        let chunk = n / parts as usize;
        for p in 0..parts {
            let part = &keys[p as usize * chunk..(p as usize + 1) * chunk];
            if p + 1 < parts {
                assert_eq!(job.clone().adopt(p + 1, part, n), Err(StepError::Core(Error::Malformed)), "out of order");
            }
            job.adopt(p, part, n)?;
            roundtrip(&job);
        }
        assert_eq!(job.clone().adopt(0, &keys[..chunk], n), Err(StepError::NoAdoption), "adoption replayed");
    } else {
        assert_eq!(job.clone().adopt(0, &[], n), Err(StepError::NoAdoption));
    }
    assert!(job.is_verified());
    let mut other = store.clone();
    other.finalized.proposer_index ^= 1;
    assert_eq!(steps::apply(&other, &job, spec).unwrap_err(), StepError::StoreChanged, "another store");
    let (next, outcome) = steps::apply(store, &job, spec)?;
    Ok((next, outcome, job))
}

/// Initialization as a job, against `FinalizedStore::initialize_with(.., OnChain)`.
fn init_both(
    trusted: &Root,
    bootstrap: &LightClientBootstrap,
    fork: LcFork,
    spec: &ChainSpec,
    bls: &dyn BlsBackend,
    parts: u8,
) -> (FinalizedStore, CompactStore) {
    let policy = FinalizedStore::initialize_with(trusted, bootstrap, fork, spec, bls, Adoption::OnChain).unwrap();
    let compact = CompactBootstrap::of(bootstrap);
    assert_eq!(CompactBootstrap::decode(&compact.encode(fork), fork).as_ref(), Ok(&compact));
    let (store, mut job) = steps::initialize(trusted, &compact, fork, spec, parts).unwrap();
    roundtrip(&job);
    assert_eq!(steps::apply(&store, &job, spec).unwrap_err(), StepError::AdoptionIncomplete);
    let n = spec.preset.sync_committee_size;
    let keys = uncompressed_keys(&bootstrap.current_sync_committee).unwrap();
    let chunk = n / parts as usize;
    for p in 0..parts {
        job.adopt(p, &keys[p as usize * chunk..(p as usize + 1) * chunk], n).unwrap();
    }
    let (active, outcome) = steps::apply(&store, &job, spec).unwrap();
    assert_eq!(outcome, ApplyOutcome::default());
    assert_eq!(active, CompactStore::of(policy.core()));
    assert_eq!(job.pending.record(), &HeaderRecord::of(&policy.core().finalized_header, fork, spec));
    assert_eq!(CompactStore::decode(&active.encode()).as_ref(), Ok(&active));
    (policy, active)
}

/// Feed one update to both and compare. Returns true if it was applied.
#[allow(clippy::too_many_arguments)]
fn compare_update(
    label: &str,
    policy: &mut FinalizedStore,
    compact: &mut CompactStore,
    update: &LightClientUpdate,
    current_slot: u64,
    ctx: &Context<'_>,
    parts: u8,
) -> Result<bool, Error> {
    let spec = ctx.spec;
    let before = policy.core().clone();
    let native = policy.process_update(update, current_slot, ctx);
    let stepped = run_job(compact, &before, update, None, current_slot, spec, &ctx.genesis_validators_root, parts);
    match (native, stepped) {
        (Ok(o1), Ok((next, o2, job))) => {
            assert_eq!(o1, o2, "{label}: outcomes differ");
            assert_eq!(next, CompactStore::of(policy.core()), "{label}: stores differ");
            if o1.advanced {
                let record = HeaderRecord::of(&policy.core().finalized_header, policy.core().fork, spec);
                assert_eq!(job.pending.record(), &record, "{label}: records differ");
                assert_eq!(HeaderRecord::decode(&record.encode()).as_ref(), Ok(&record));
            }
            *compact = next;
            Ok(true)
        }
        (Err(e1), Err(e2)) => {
            assert_eq!(e1.code(), e2.code(), "{label}: refused with {e1:?} natively, {e2:?} by the steps");
            assert_eq!(*compact, CompactStore::of(policy.core()), "{label}: a refusal changed a store");
            Err(e1)
        }
        (native, stepped) => panic!("{label}: process_update {native:?}, steps {:?}", stepped.map(|s| s.1)),
    }
}

// ---------------------------------------------------------------------------------------------
// Spec vectors
// ---------------------------------------------------------------------------------------------

fn yaml(path: PathBuf) -> Value {
    serde_yaml::from_str(&fs::read_to_string(&path).unwrap()).unwrap()
}

fn hex_n<const N: usize>(v: &Value) -> [u8; N] {
    hex::decode(v.as_str().unwrap().trim().trim_start_matches("0x")).unwrap().try_into().unwrap()
}

fn as_u64(v: &Value) -> u64 {
    match v {
        Value::Number(n) => n.as_u64().unwrap(),
        Value::String(s) => s.trim().parse().unwrap(),
        other => panic!("not an integer: {other:?}"),
    }
}

fn as_version(v: &Value) -> [u8; 4] {
    match v {
        Value::Number(n) => u32::try_from(n.as_u64().unwrap()).unwrap().to_be_bytes(),
        other => hex_n::<4>(other),
    }
}

#[test]
fn steps_equal_process_update_on_every_sync_vector() {
    let all = cases("sync").unwrap();
    assert_eq!(all.len(), 52, "the pinned vectors have 52 sync cases");
    let bls = Blst::default();
    let (mut applied, mut refused, mut compared) = (0, 0, 0);
    let (mut told_upgrades, mut self_upgrades) = (0, 0);
    for (case_index, (fork, dir)) in all.iter().enumerate() {
        let name = format!("{}/{}", fork.name(), dir.file_name().unwrap().to_string_lossy());
        let meta = yaml(dir.join("meta.yaml"));
        let cfg = parse_config(&dir.join("config.yaml")).unwrap();
        let spec = &cfg.spec;
        let n = spec.preset.sync_committee_size;
        let gvr: Root = hex_n(&meta["genesis_validators_root"]);
        let trusted: Root = hex_n(&meta["trusted_block_root"]);
        let lc = |f: ferrata_core::fork::Fork| f.lc_fork().unwrap();
        let bootstrap_fork = lc(cfg.fork_for_digest(&gvr, hex_n(&meta["bootstrap_fork_digest"])).unwrap());
        let store_fork = lc(cfg.fork_for_version(as_version(&meta["store_fork_version"])).unwrap());
        let bootstrap = LightClientBootstrap::decode(
            &read_ssz_snappy(&dir.join("bootstrap.ssz_snappy")).unwrap(),
            bootstrap_fork,
            n,
        )
        .unwrap();
        let bootstrap = upgrade_bootstrap(bootstrap, bootstrap_fork, store_fork, spec).unwrap();
        // Alternate one-part and two-part adoptions across cases.
        let parts = if case_index % 2 == 0 { 1 } else { 2 };
        let (mut policy, mut compact) = init_both(&trusted, &bootstrap, store_fork, spec, &bls, parts);
        // A second compact store that is never told to upgrade: it moves to a later layout only
        // when an update arrives in one, as a program's instance does (`steps::store_for_update`).
        let mut auto = compact.clone();
        let ctx = Context { spec, genesis_validators_root: gvr, bls: &bls };
        for (i, step) in yaml(dir.join("steps.yaml")).as_sequence().unwrap().iter().enumerate() {
            let (kind, body) = step.as_mapping().and_then(|m| m.iter().next()).unwrap();
            match kind.as_str().unwrap() {
                "process_update" => {
                    let update_fork = lc(cfg.fork_for_digest(&gvr, hex_n(&body["update_fork_digest"])).unwrap());
                    let file = body["update"].as_str().unwrap().trim();
                    let update = LightClientUpdate::decode(
                        &read_ssz_snappy(&dir.join(format!("{file}.ssz_snappy"))).unwrap(),
                        update_fork,
                        n,
                    )
                    .unwrap();
                    let as_served = update.clone();
                    let update = upgrade_update(update, update_fork, policy.core().fork, spec).unwrap();
                    let label = format!("{name} step {i}");
                    let current_slot = as_u64(&body["current_slot"]);
                    let before = policy.core().clone();
                    let told = compare_update(&label, &mut policy, &mut compact, &update, current_slot, &ctx, parts);
                    match told {
                        Ok(_) => applied += 1,
                        Err(_) => refused += 1,
                    }
                    // The self-upgrading store takes the update in the layout it was served in,
                    // or upgraded to its own if it is already ahead.
                    let layout = update_fork.max(auto.fork);
                    let served = upgrade_update(as_served, update_fork, layout, spec).unwrap();
                    self_upgrades += usize::from(layout > auto.fork);
                    let stepped = steps::store_for_update(&auto, layout, served.attested_header.beacon.slot, spec)
                        .map_err(StepError::Core)
                        .and_then(|base| run_job(&base, &before, &served, None, current_slot, spec, &gvr, parts));
                    match (told, stepped) {
                        (Ok(_), Ok((next, _, job))) => {
                            // The job started from the upgraded store; the stored one is not
                            // upgraded yet, and applying the job to it gives the same store.
                            assert_eq!(steps::apply(&auto, &job, spec).map(|(s, _)| s), Ok(next.clone()), "{label}");
                            auto = next;
                        }
                        (Err(e1), Err(e2)) => assert_eq!(e1.code(), e2.code(), "{label}: the self-upgrading store"),
                        (told, stepped) => panic!("{label}: told {told:?}, self-upgrading {:?}", stepped.map(|s| s.1)),
                    }
                }
                "force_update" => {} // the policy never forces
                "upgrade_store" => {
                    let to = lc(cfg.fork_for_version(as_version(&body["store_fork_version"])).unwrap());
                    policy.upgrade(to, spec).unwrap();
                    assert_eq!(compact.upgraded(compact.fork), Ok(compact.clone()), "to its own layout: unchanged");
                    compact = compact.upgraded(to).expect("the vectors only upgrade forwards");
                    told_upgrades += 1;
                }
                other => panic!("{name}: unknown step {other}"),
            }
            assert_eq!(compact, CompactStore::of(policy.core()), "{name} step {i}");
            // The self-upgrading store is the told one, except that it may not have moved yet.
            assert_eq!(auto.upgraded(compact.fork).as_ref(), Ok(&compact), "{name} step {i}: the self-upgrading store");
            compared += 1;
        }
    }
    println!(
        "steps vs process_update: 52 cases, {compared} steps compared, {applied} updates applied, {refused} refused \
         alike; {told_upgrades} store upgrades told by the vectors, {self_upgrades} made by a store that upgrades \
         itself when an update arrives in a later layout"
    );
    assert!(applied > 50 && refused > 10, "the vectors exercise both outcomes");
    assert!(told_upgrades > 0 && self_upgrades > 0, "the vectors cross forks");
}

// ---------------------------------------------------------------------------------------------
// Mainnet
// ---------------------------------------------------------------------------------------------

fn mainnet() -> ChainSpec {
    ChainSpec {
        preset: Preset::MAINNET,
        versions: [
            [0, 0, 0, 0],
            [1, 0, 0, 0],
            [2, 0, 0, 0],
            [3, 0, 0, 0],
            [4, 0, 0, 0],
            [5, 0, 0, 0],
            [6, 0, 0, 0],
            [7, 0, 0, 0],
            [8, 0, 0, 0],
        ],
        epochs: [0, 74_240, 144_896, 194_048, 269_568, 364_032, 411_392, FAR_FUTURE_EPOCH, FAR_FUTURE_EPOCH],
    }
}

fn root(hex_str: &str) -> Root {
    hex::decode(hex_str.trim_start_matches("0x")).unwrap().try_into().unwrap()
}

const GENESIS_VALIDATORS_ROOT: &str = "0x4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95";
const TRUSTED_BLOCK_ROOT: &str = "0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac";
const EXPECTED_FINALIZED: [(u64, &str); 5] = [
    (15_556_608, "0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac"),
    (15_564_800, "0x96949350abfe8a04e6f1ae22b3064fdcee177dec32133348a2a69d200151e09a"),
    (15_572_992, "0x7e2771b1939afa205540003849bca0b0aa6ee71dd5a767cca2e35e084a008a4c"),
    (15_581_184, "0xe7c0ae95dd6a82d9eca7d9dc4e1b42a4c8197fbb0e4cd880bc5f9631de2b992b"),
    (15_589_599, "0x5d1200f2c031f01aa2c3385c881ca6cac727c23d3080e0ac53169160c8ddd573"),
];

fn fixture(name: &str) -> Vec<u8> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/mainnet-p1899");
    let bytes = fs::read(dir.join(name)).unwrap();
    let sums = fs::read_to_string(dir.join("SHA256SUMS")).unwrap();
    let want = sums.lines().find(|l| l.ends_with(name)).unwrap().split_whitespace().next().unwrap();
    assert_eq!(hex::encode(Sha256::digest(&bytes)), want, "{name} does not match SHA256SUMS");
    bytes
}

fn mainnet_data() -> (LightClientBootstrap, Vec<LightClientUpdate>) {
    let bootstrap = LightClientBootstrap::decode(&fixture("bootstrap.ssz"), LcFork::Electra, 512).unwrap();
    let updates = (0..5)
        .map(|i| LightClientUpdate::decode(&fixture(&format!("update_{i}.ssz")), LcFork::Electra, 512).unwrap())
        .collect();
    (bootstrap, updates)
}

#[test]
fn mainnet_replay_through_the_steps() {
    let spec = mainnet();
    let (bootstrap, updates) = mainnet_data();
    let bls = Blst::new(Aggregation::Auto);
    let ctx = Context { spec: &spec, genesis_validators_root: root(GENESIS_VALIDATORS_ROOT), bls: &bls };
    for parts in [1u8, 2, 4] {
        let (mut policy, mut compact) =
            init_both(&root(TRUSTED_BLOCK_ROOT), &bootstrap, LcFork::Electra, &spec, &bls, parts);
        for (i, update) in updates.iter().enumerate() {
            let applied = compare_update(
                &format!("mainnet update {i}"),
                &mut policy,
                &mut compact,
                update,
                update.signature_slot + 1,
                &ctx,
                parts,
            );
            assert_eq!(applied, Ok(true), "update {i}");
            let (slot, block_root) = EXPECTED_FINALIZED[i];
            assert_eq!(compact.finalized.slot, slot);
            assert_eq!(compact.finalized.hash_tree_root(), root(block_root));
        }
    }
}

#[test]
fn the_miller_step_equals_miller_half_byte_for_byte() {
    let spec = mainnet();
    let (bootstrap, updates) = mainnet_data();
    let bls = Blst::new(Aggregation::Auto);
    let gvr = root(GENESIS_VALIDATORS_ROOT);
    let ctx = Context { spec: &spec, genesis_validators_root: gvr, bls: &bls };
    let mut policy = FinalizedStore::initialize_with(
        &root(TRUSTED_BLOCK_ROOT),
        &bootstrap,
        LcFork::Electra,
        &spec,
        &bls,
        Adoption::OnChain,
    )
    .unwrap();
    for update in &updates {
        let compact = CompactStore::of(policy.core());
        let committee = signing_committee(policy.core(), update, &spec).unwrap().clone();
        let witness = SignerWitness::build(&committee, &update.sync_aggregate.bits).unwrap();
        assert_eq!(witness.keys.len(), 512 - update.sync_aggregate.participants() as usize);
        let mut job =
            steps::step1(&compact, &CompactUpdate::of(update), &witness, update.signature_slot, &spec, &gvr, 2)
                .unwrap();
        job.step2().unwrap();
        let Phase::FinalExp { product } = job.phase else { panic!("step 2 leaves the product") };
        let signing_root = sync_committee_signing_root(update, &ctx);
        let expected = bls
            .miller_half(&committee, &update.sync_aggregate.bits, &signing_root, &update.sync_aggregate.signature)
            .unwrap();
        assert_eq!(product, expected);
        policy.process_update(update, update.signature_slot + 1, &ctx).unwrap();
    }
}

/// update_1 against the store after update_0, changed in one way at a time.
#[test]
fn mutated_mainnet_updates_are_refused_alike() {
    let spec = mainnet();
    let (bootstrap, updates) = mainnet_data();
    let bls = Blst::new(Aggregation::Auto);
    let gvr = root(GENESIS_VALIDATORS_ROOT);
    let ctx = Context { spec: &spec, genesis_validators_root: gvr, bls: &bls };
    let (mut base_policy, mut base_compact) =
        init_both(&root(TRUSTED_BLOCK_ROOT), &bootstrap, LcFork::Electra, &spec, &bls, 2);
    compare_update(
        "update 0",
        &mut base_policy,
        &mut base_compact,
        &updates[0],
        updates[0].signature_slot + 1,
        &ctx,
        2,
    )
    .unwrap();
    let good = updates[1].clone();
    let signer = good.sync_aggregate.bits.iter().position(|b| *b).unwrap();
    let non_signer = good.sync_aggregate.bits.iter().position(|b| !*b).unwrap();
    type Mutation = (&'static str, fn(&mut LightClientUpdate));
    let mutations: Vec<Mutation> = vec![
        ("as served", |_| {}),
        ("stripped to a finality update", |u| {
            u.next_sync_committee = SyncCommittee::empty(512);
            u.next_sync_committee_branch = vec![[0; 32]; u.next_sync_committee_branch.len()];
        }),
        ("signature byte flipped", |u| u.sync_aggregate.signature[40] ^= 1),
        ("signature compression flag cleared", |u| u.sync_aggregate.signature[0] &= 0x7f),
        ("finality branch tampered", |u| u.finality_branch[3][0] ^= 1),
        ("finality branch removed", |u| u.finality_branch = vec![[0; 32]; u.finality_branch.len()]),
        ("next-committee branch tampered", |u| u.next_sync_committee_branch[2][5] ^= 1),
        ("next committee key tampered", |u| u.next_sync_committee.pubkeys[7][47] ^= 1),
        ("next aggregate tampered", |u| u.next_sync_committee.aggregate_pubkey[47] ^= 1),
        ("attested state root tampered", |u| u.attested_header.beacon.state_root[0] ^= 1),
        ("attested body root tampered", |u| u.attested_header.beacon.body_root[0] ^= 1),
        ("finalized body root tampered", |u| u.finalized_header.beacon.body_root[0] ^= 1),
        ("finalized proposer tampered", |u| u.finalized_header.beacon.proposer_index ^= 1),
        ("signature slot not after attested", |u| u.signature_slot = u.attested_header.beacon.slot),
        ("signature slot in a later period", |u| u.signature_slot += 8192 * 2),
        ("participation below two thirds", |u| {
            let mut cleared = 0;
            for b in u.sync_aggregate.bits.iter_mut() {
                if *b && cleared < 200 {
                    *b = false;
                    cleared += 1;
                }
            }
        }),
    ];
    let mut codes = Vec::new();
    for (name, mutate) in &mutations {
        let mut update = good.clone();
        mutate(&mut update);
        let (mut policy, mut compact) = (base_policy.clone(), base_compact.clone());
        let r = compare_update(name, &mut policy, &mut compact, &update, update.signature_slot + 1, &ctx, 2);
        codes.push((*name, r.map(|_| "applied").map_err(|e| e.code())));
    }
    // Bits that no longer match the signature: the witness is built for the bits as sent.
    for (name, index) in [("a signer's bit cleared", signer), ("a non-signer's bit set", non_signer)] {
        let mut update = good.clone();
        update.sync_aggregate.bits[index] = !update.sync_aggregate.bits[index];
        let (mut policy, mut compact) = (base_policy.clone(), base_compact.clone());
        let r = compare_update(name, &mut policy, &mut compact, &update, update.signature_slot + 1, &ctx, 2);
        assert_eq!(r, Err(Error::SignatureInvalid), "{name}");
        codes.push((name, r.map(|_| "applied").map_err(|e| e.code())));
    }
    // Another chain's instance: the same update under another genesis_validators_root.
    {
        let other = Context { spec: &spec, genesis_validators_root: [0x42; 32], bls: &bls };
        let (mut policy, mut compact) = (base_policy.clone(), base_compact.clone());
        let r = compare_update("another chain", &mut policy, &mut compact, &good, good.signature_slot + 1, &other, 2);
        assert_eq!(r, Err(Error::SignatureInvalid));
        codes.push(("another genesis_validators_root", r.map(|_| "applied").map_err(|e| e.code())));
    }
    // Replays and skips.
    for (name, update) in [("update 0 replayed", &updates[0]), ("update 2 before update 1", &updates[2])] {
        let (mut policy, mut compact) = (base_policy.clone(), base_compact.clone());
        let r = compare_update(name, &mut policy, &mut compact, update, update.signature_slot + 1, &ctx, 2);
        codes.push((name, r.map(|_| "applied").map_err(|e| e.code())));
    }
    for (name, r) in &codes {
        println!("  {name:<38} {r:?}");
    }
    assert_eq!(codes[0].1, Ok("applied"));
    assert_eq!(codes[1].1, Ok("applied"));
    assert!(codes[2..].iter().all(|(_, r)| r.is_err()), "every other mutation is refused");
}

#[test]
fn a_bad_signer_witness_is_refused() {
    let spec = mainnet();
    let (bootstrap, updates) = mainnet_data();
    let bls = Blst::default();
    let gvr = root(GENESIS_VALIDATORS_ROOT);
    let mut policy = FinalizedStore::initialize_with(
        &root(TRUSTED_BLOCK_ROOT),
        &bootstrap,
        LcFork::Electra,
        &spec,
        &bls,
        Adoption::OnChain,
    )
    .unwrap();
    let ctx = Context { spec: &spec, genesis_validators_root: gvr, bls: &bls };
    policy.process_update(&updates[0], updates[0].signature_slot + 1, &ctx).unwrap();
    // update_1 (510 of 512 signers) is signed by the committee update_0 handed over.
    let update = &updates[1];
    let compact = CompactStore::of(policy.core());
    let cu = CompactUpdate::of(update);
    let committee = &policy.core().next_sync_committee;
    let good = SignerWitness::build(committee, &update.sync_aggregate.bits).unwrap();
    assert!(!good.keys.is_empty());
    let run = |w: &SignerWitness| steps::step1(&compact, &cu, w, update.signature_slot, &spec, &gvr, 2).map(|_| ());
    assert_eq!(run(&good), Ok(()));
    let mut fewer = good.clone();
    fewer.keys.pop();
    fewer.branches.pop();
    assert_eq!(run(&fewer), Err(StepError::Witness));
    let mut bad_branch = good.clone();
    bad_branch.branches[0][3][0] ^= 1;
    assert_eq!(run(&bad_branch), Err(StepError::Witness));
    let mut other_key = good.clone();
    let signer = update.sync_aggregate.bits.iter().position(|b| *b).unwrap();
    other_key.keys[0] = uncompress_key(&committee.pubkeys[signer]).unwrap();
    assert_eq!(run(&other_key), Err(StepError::Witness));
    let mut negated = good.clone();
    let p = PublicKey::deserialize(&good.keys[0]).unwrap();
    let mut neg_bytes = p.compress();
    neg_bytes[0] ^= 0x20;
    negated.keys[0] = uncompress_key(&neg_bytes).unwrap();
    assert_eq!(run(&negated), Err(StepError::Witness), "the negated point re-compresses to other bytes");
    let mut compressed_form = good.clone();
    compressed_form.keys[0][..48].copy_from_slice(&p.compress());
    assert_eq!(run(&compressed_form), Err(StepError::Witness), "a compressed encoding is not a witness");
    // Another key root for the signing committee: the honest witness no longer verifies.
    let mut wrong_store = compact.clone();
    wrong_store.next.as_mut().unwrap().pubkeys_root[0] ^= 1;
    assert_eq!(
        steps::step1(&wrong_store, &cu, &good, update.signature_slot, &spec, &gvr, 2).map(|_| ()),
        Err(StepError::Witness)
    );
}

// ---------------------------------------------------------------------------------------------
// Adoption from uncompressed keys against `check_committee_onchain`
// ---------------------------------------------------------------------------------------------

fn add_g1(a: &BlsPubkey, b: &BlsPubkey) -> BlsPubkey {
    use blst::{
        blst_p1, blst_p1_add_or_double_affine, blst_p1_affine, blst_p1_compress, blst_p1_from_affine,
        blst_p1_uncompress,
    };
    let mut pa = blst_p1_affine::default();
    let mut pb = blst_p1_affine::default();
    let mut sum = blst_p1::default();
    let mut out = [0u8; 48];
    unsafe {
        assert_eq!(blst_p1_uncompress(&mut pa, a.as_ptr()), blst::BLST_ERROR::BLST_SUCCESS);
        assert_eq!(blst_p1_uncompress(&mut pb, b.as_ptr()), blst::BLST_ERROR::BLST_SUCCESS);
        blst_p1_from_affine(&mut sum, &pa);
        let s: *mut blst_p1 = &mut sum;
        blst_p1_add_or_double_affine(s, s, &pb);
        blst_p1_compress(out.as_mut_ptr(), &sum);
    }
    out
}

fn compressed_candidates(want_on_curve: bool, want_in_group: bool) -> BlsPubkey {
    (1u8..=255)
        .map(|x| {
            let mut k = [0u8; 48];
            k[0] = 0x80;
            k[47] = x;
            k
        })
        .find(|k| match PublicKey::uncompress(k) {
            Ok(pk) => want_on_curve && (pk.validate().is_ok() == want_in_group),
            Err(_) => !want_on_curve,
        })
        .unwrap()
}

/// Adoption with an honest witness where one exists, else with a stand-in valid point.
fn adopt_honestly(committee: &SyncCommittee) -> Result<(), Error> {
    let standin = uncompress_key(&fake_committee(1, 99).committee.pubkeys[0]).unwrap();
    let keys: Vec<_> = committee.pubkeys.iter().map(|k| uncompress_key(k).unwrap_or(standin)).collect();
    adopt_committee(&CommitteeSummary::of(committee), &keys)
}

#[test]
fn adoption_from_uncompressed_keys_agrees_with_check_committee_onchain() {
    let bls = Blst::default();
    let (bootstrap, updates) = mainnet_data();
    let mut committees = vec![bootstrap.current_sync_committee.clone()];
    committees.extend(updates.iter().map(|u| u.next_sync_committee.clone()));
    for size in [2usize, 32, 512] {
        committees.push(fake_committee(size, size as u8).committee);
    }
    let a = fake_committee(2, 7).committee;
    let (k0, k1) = (a.pubkeys[0], a.pubkeys[1]);
    let non_subgroup = compressed_candidates(true, false);
    let off_curve = compressed_candidates(false, false);
    let mut identity = [0u8; 48];
    identity[0] = 0xc0;
    let mut order_three = [0u8; 48];
    order_three[0] = 0x80;
    let crafted = [
        ("wrong aggregate", SyncCommittee { pubkeys: vec![k0, k1], aggregate_pubkey: k0 }),
        ("identity key", SyncCommittee { pubkeys: vec![k0, identity], aggregate_pubkey: k0 }),
        ("order-3 key", SyncCommittee { pubkeys: vec![k0, order_three], aggregate_pubkey: k0 }),
        ("off-curve key", SyncCommittee { pubkeys: vec![k0, off_curve], aggregate_pubkey: k0 }),
        (
            "outside the subgroup",
            SyncCommittee { pubkeys: vec![k0, non_subgroup], aggregate_pubkey: add_g1(&k0, &non_subgroup) },
        ),
        ("empty", SyncCommittee { pubkeys: vec![], aggregate_pubkey: identity }),
    ];
    for c in &committees {
        assert_eq!(bls.check_committee_onchain(c), Ok(()));
        assert_eq!(adopt_honestly(c), Ok(()));
        // In parts, the same.
        let keys = uncompressed_keys(c).unwrap();
        let n = keys.len();
        let mut progress = AdoptionProgress::new(CommitteeSummary::of(c), 2);
        progress.add_part(0, &keys[..n / 2], n).unwrap();
        assert_eq!(AdoptionProgress::decode(&progress.encode()).as_ref(), Ok(&progress));
        progress.add_part(1, &keys[n / 2..], n).unwrap();
        assert!(progress.complete);
    }
    for (name, c) in &crafted {
        let onchain = bls.check_committee_onchain(c);
        let witness = adopt_honestly(c);
        println!(
            "  {name:<22} check_committee_onchain {onchain:?}  witness {witness:?}  full {:?}",
            bls.check_committee(c)
        );
        assert_eq!(onchain, witness, "{name}");
    }
    assert_eq!(bls.check_committee_onchain(&crafted[4].1), Ok(()), "neither check tests the subgroup");
}

#[test]
fn a_bad_adoption_witness_is_refused_and_changes_nothing() {
    let c = &mainnet_data().0.current_sync_committee;
    let summary = CommitteeSummary::of(c);
    let keys = uncompressed_keys(c).unwrap();
    let mut progress = AdoptionProgress::new(summary, 2);
    let first = keys[..256].to_vec();
    // A key replaced by its negation: on the curve, but other bytes under the root.
    let mut negated = first.clone();
    let mut bytes = PublicKey::deserialize(&negated[3]).unwrap().compress();
    bytes[0] ^= 0x20;
    negated[3] = uncompress_key(&bytes).unwrap();
    progress.add_part(0, &negated, 512).unwrap(); // a part alone cannot tell
    assert_eq!(progress.clone().add_part(1, &keys[256..], 512), Err(Error::CommitteeInvalid));
    let mut progress = AdoptionProgress::new(summary, 2);
    let before = progress.clone();
    let mut compressed = first.clone();
    compressed[0][..48].copy_from_slice(&c.pubkeys[0]);
    assert_eq!(progress.add_part(0, &compressed, 512), Err(Error::Malformed));
    assert_eq!(progress.add_part(0, &first[..255], 512), Err(Error::Malformed));
    assert_eq!(progress.add_part(1, &keys[256..], 512), Err(Error::Malformed));
    let mut identity = first.clone();
    identity[9] = ferrata_core::witness::IDENTITY_UNCOMPRESSED;
    assert_eq!(progress.add_part(0, &identity, 512), Err(Error::CommitteeInvalid));
    assert_eq!(progress, before, "a refused part changes nothing");
    progress.add_part(0, &first, 512).unwrap();
    progress.add_part(1, &keys[256..], 512).unwrap();
    assert_eq!(progress.add_part(1, &keys[256..], 512), Err(Error::Malformed), "complete");
    // The wrong aggregate under the right keys.
    let mut wrong = summary;
    wrong.aggregate_pubkey = c.pubkeys[0];
    assert_eq!(adopt_committee(&wrong, &keys), Err(Error::CommitteeInvalid));
}

#[test]
fn adopted_keys_must_match_committee_key_root() {
    let spec = mainnet();
    let (bootstrap, updates) = mainnet_data();
    let bls = Blst::new(Aggregation::Auto);
    let gvr = root(GENESIS_VALIDATORS_ROOT);
    let trusted = root(TRUSTED_BLOCK_ROOT);
    let ctx = Context { spec: &spec, genesis_validators_root: gvr, bls: &bls };

    // Swapping two distinct keys preserves the G1 aggregate sum while changing `pubkeys_root`, so
    // the aggregate-sum check alone cannot catch it; only the Merkle key-root check in
    // `AdoptionProgress::add_part` refuses it.
    let c = &bootstrap.current_sync_committee;
    let summary = CommitteeSummary::of(c);
    let keys = uncompressed_keys(c).unwrap();
    assert_ne!(keys[0], keys[1], "distinct keys for within-part swap");
    assert_ne!(keys[0], keys[256], "distinct keys for cross-part swap");

    // Confirm the swapped committee still sums to the exact committed `aggregate_pubkey` while
    // hashing to a different `pubkeys_root`.
    for (label, i, j) in [("within part", 0usize, 1usize), ("across parts", 0, 256)] {
        let mut swapped_c = c.clone();
        swapped_c.pubkeys.swap(i, j);
        assert_eq!(bls.check_committee_onchain(&swapped_c), Ok(()), "sum unchanged ({label})");
        assert_ne!(CommitteeSummary::of(&swapped_c).pubkeys_root, summary.pubkeys_root, "root changed ({label})");
    }

    let mut swapped_within = keys.clone();
    swapped_within.swap(0, 1);
    let mut swapped_across = keys.clone();
    swapped_across.swap(0, 256);

    // 1. Single-part `adopt_committee`: both swaps must be refused with `CommitteeInvalid`.
    assert_eq!(adopt_committee(&summary, &swapped_within), Err(Error::CommitteeInvalid));
    assert_eq!(adopt_committee(&summary, &swapped_across), Err(Error::CommitteeInvalid));

    // 2. Multi-part `AdoptionProgress::add_part`: refused on the final part, progress unchanged.
    for (label, swapped) in [("within part", &swapped_within), ("across parts", &swapped_across)] {
        let mut progress = AdoptionProgress::new(summary, 2);
        progress.add_part(0, &swapped[..256], 512).unwrap();
        let before_final = progress.clone();
        assert_eq!(
            progress.add_part(1, &swapped[256..], 512),
            Err(Error::CommitteeInvalid),
            "2-part AdoptionProgress swap {label}"
        );
        assert_eq!(progress, before_final, "refused final part must leave progress unchanged ({label})");
    }

    // 3. Full bootstrap step path (`steps::initialize` + `Job::adopt`), 2-part and 1-part.
    let compact_boot = CompactBootstrap::of(&bootstrap);
    for (label, swapped) in [("within part", &swapped_within), ("across parts", &swapped_across)] {
        let (_, mut job2) = steps::initialize(&trusted, &compact_boot, LcFork::Electra, &spec, 2).unwrap();
        job2.adopt(0, &swapped[..256], 512).unwrap();
        assert_eq!(
            job2.adopt(1, &swapped[256..], 512),
            Err(StepError::Core(Error::CommitteeInvalid)),
            "bootstrap 2-part Job::adopt swap {label}"
        );
        let (_, mut job1) = steps::initialize(&trusted, &compact_boot, LcFork::Electra, &spec, 1).unwrap();
        assert_eq!(
            job1.adopt(0, swapped, 512),
            Err(StepError::Core(Error::CommitteeInvalid)),
            "bootstrap 1-part Job::adopt swap {label}"
        );
    }

    // 4. Full update step path (`steps::step1` -> `step2` -> `Job::adopt` on mainnet update_1):
    //    adopting `next_sync_committee` with two swapped keys must be refused in 2-part and 1-part modes.
    let (mut policy, mut compact) = init_both(&trusted, &bootstrap, LcFork::Electra, &spec, &bls, 2);
    compare_update("update 0", &mut policy, &mut compact, &updates[0], updates[0].signature_slot + 1, &ctx, 2).unwrap();
    let u1 = &updates[1];
    let cu1 = CompactUpdate::of(u1);
    let sw1 = SignerWitness::build(&policy.core().next_sync_committee, &u1.sync_aggregate.bits).unwrap();
    let u1_keys = uncompressed_keys(&u1.next_sync_committee).unwrap();
    assert_ne!(u1_keys[0], u1_keys[1]);
    assert_ne!(u1_keys[0], u1_keys[256]);
    let mut u1_swap_within = u1_keys.clone();
    u1_swap_within.swap(0, 1);
    let mut u1_swap_across = u1_keys.clone();
    u1_swap_across.swap(0, 256);

    for (label, swapped) in [("within part", &u1_swap_within), ("across parts", &u1_swap_across)] {
        let mut job2 = steps::step1(&compact, &cu1, &sw1, u1.signature_slot + 1, &spec, &gvr, 2).unwrap();
        job2.step2().unwrap();
        job2.adopt(0, &swapped[..256], 512).unwrap();
        assert_eq!(
            job2.adopt(1, &swapped[256..], 512),
            Err(StepError::Core(Error::CommitteeInvalid)),
            "update 2-part Job::adopt swap {label}"
        );
        let mut job1 = steps::step1(&compact, &cu1, &sw1, u1.signature_slot + 1, &spec, &gvr, 1).unwrap();
        job1.step2().unwrap();
        job1.step3().unwrap();
        assert_eq!(
            job1.adopt(0, swapped, 512),
            Err(StepError::Core(Error::CommitteeInvalid)),
            "update 1-part Job::adopt swap {label}"
        );
    }
}
