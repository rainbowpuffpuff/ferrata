//! The one-step handoff (`residue_handoff::one_step`: one multiproof, the residue witness, the next
//! committee adopted by its summary) against the core's own steps (a branch per non-signer, the
//! final exponentiation, the 512-key adoption in two parts, apply). Experimental and unaudited;
//! `bench/RESIDUE.md` §10.
//!
//! * every update of the official `sync` vectors, of the mainnet replay and of the Sepolia chain
//!   through Gloas: the same store, or the same refusal code, and every handoff counted;
//! * the refusals of a handoff: a bad next-committee branch, a period update below the policy, a
//!   stale one;
//! * the one difference (an aggregate field that is not the sum of the committee's keys, which only
//!   a state the beacon chain's transition did not produce can hold) and what it does afterwards.
//!
//! Needs the vectors (`scripts/fetch-vectors.sh`).

use std::{fs, path::PathBuf};

use ferrata_core::{
    bls::{Aggregation, Blst, DST},
    fork::{ChainSpec, LcFork, Preset, FAR_FUTURE_EPOCH},
    merkle::ZERO,
    policy::{Adoption, FinalizedStore},
    steps::{self, Job, PendingApply, Phase, StepError},
    store::{ApplyOutcome, StoreCore},
    types::{LightClientBootstrap, LightClientHeader, LightClientUpdate, SyncAggregate, SyncCommittee},
    upgrade::{upgrade_bootstrap, upgrade_update},
    validate::{sync_committee_signing_root, Context},
    witness::{uncompressed_keys, CommitteeSummary, CompactStore, CompactUpdate, SignerWitness},
    Error, Root,
};
use ferrata_spectests::{
    cases, load_sync_case, parse_config, read_ssz_snappy,
    testkit::{fabricated_update, fake_committee, sign, with_wrong_aggregate, Fabrication},
    vectors_dir,
};
use residue::{decode_fp12, encode_fp12, witness, FP12_BYTES};
use residue_agg::{NonSigners, Sum};
use residue_handoff::{adopt_summary, one_step};
use serde_yaml::Value;
use sha2::{Digest, Sha256};

/// The core's adoption arrives in two parts (any power of two dividing the committee size would do).
const PARTS: u8 = 2;

// ---------------------------------------------------------------------------------------------
// Helpers (as residue-agg's tests)
// ---------------------------------------------------------------------------------------------

fn fixtures(dir: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/ferrata-spectests/fixtures").join(dir)
}

/// A fixture file, checked against its directory's pinned `SHA256SUMS`.
fn fixture(dir: &str, name: &str) -> Vec<u8> {
    let bytes = fs::read(fixtures(dir).join(name)).unwrap();
    let sums = fs::read_to_string(fixtures(dir).join("SHA256SUMS")).unwrap();
    let want = sums.lines().find(|l| l.ends_with(&format!("  {name}"))).unwrap().split_whitespace().next().unwrap();
    assert_eq!(hex::encode(Sha256::digest(&bytes)), want, "{dir}/{name} does not match SHA256SUMS");
    bytes
}

fn root(s: &str) -> Root {
    hex::decode(s.trim_start_matches("0x")).unwrap().try_into().unwrap()
}

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

const MAINNET_GVR: &str = "0x4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95";
const MAINNET_TRUSTED: &str = "0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac";

fn sepolia() -> ChainSpec {
    let v = |b: u8| [0x90, 0x00, 0x00, b];
    ChainSpec {
        preset: Preset::MAINNET,
        versions: [v(0x69), v(0x70), v(0x71), v(0x72), v(0x73), v(0x74), v(0x75), v(0x76), [0; 4]],
        epochs: [0, 50, 100, 56_832, 132_608, 222_464, 272_640, 353_024, FAR_FUTURE_EPOCH],
    }
}

const SEPOLIA_GVR: &str = "0xd8ea171f3c94aea21ebc42a1ed61052acf3f9209c00e4efbaaddac09ed9b8078";
const SEPOLIA_TRUSTED: &str = "0x5f71d7f650f9252b48749b5f062c0c234e01afd69f364ab600e2b6051494b20d";

fn sepolia_finality_update(b: &[u8]) -> LightClientUpdate {
    const H: usize = 112 + 32 + 11 * 32;
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

/// The committee that signs `update` in the full store, if known.
fn signing_committee<'a>(full: &'a StoreCore, update: &LightClientUpdate, spec: &ChainSpec) -> Option<&'a SyncCommittee> {
    let c = if spec.period_at_slot(update.signature_slot) == spec.period_at_slot(full.finalized_header.beacon.slot) {
        &full.current_sync_committee
    } else {
        &full.next_sync_committee
    };
    (!c.is_empty()).then_some(c)
}

fn as_finality(u: &LightClientUpdate) -> LightClientUpdate {
    let mut f = u.clone();
    f.next_sync_committee = SyncCommittee::empty(u.sync_aggregate.bits.len());
    f.next_sync_committee_branch = vec![ZERO; u.next_sync_committee_branch.len()];
    f
}

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

// ---------------------------------------------------------------------------------------------
// The two paths
// ---------------------------------------------------------------------------------------------

type Applied = (CompactStore, ApplyOutcome, Job);

/// The core's own steps: step 1 with a branch per non-signer, the Miller loop, the final
/// exponentiation, the adoption of the update's next committee from its uncompressed keys in
/// `PARTS` parts, and apply.
fn core_steps(
    store: &CompactStore,
    update: &LightClientUpdate,
    committee: Option<&SyncCommittee>,
    current_slot: u64,
    spec: &ChainSpec,
    gvr: &Root,
) -> Result<Applied, StepError> {
    let bits = &update.sync_aggregate.bits;
    let witness = committee.and_then(|c| SignerWitness::build(c, bits).ok()).unwrap_or_default();
    let compact = CompactUpdate::of(update);
    let mut job = steps::step1(store, &compact, &witness, current_slot, spec, gvr, PARTS)?;
    job.step2()?;
    job.step3()?;
    if job.adoption_pending() {
        let keys = uncompressed_keys(&update.next_sync_committee).map_err(StepError::Core)?;
        let n = keys.len();
        for (i, part) in keys.chunks(n / PARTS as usize).enumerate() {
            job.adopt(i as u8, part, n)?;
        }
    }
    let (next, outcome) = steps::apply(store, &job, spec)?;
    Ok((next, outcome, job))
}

/// The honest residue witness of the update's Miller-loop product, if step 1 accepts (zeros
/// otherwise: step 1 refuses before the check).
fn honest_witness(
    store: &CompactStore,
    compact: &CompactUpdate,
    signers: &NonSigners,
    current_slot: u64,
    spec: &ChainSpec,
    gvr: &Root,
) -> ([u8; FP12_BYTES], [u8; FP12_BYTES]) {
    let Ok(mut job) = residue_agg::step1(store, compact, signers, current_slot, spec, gvr, 1, Sum::Batch) else {
        return ([0; FP12_BYTES], [0; FP12_BYTES]);
    };
    job.step2().unwrap();
    let Phase::FinalExp { product } = &job.phase else { panic!("step 2 leaves the product") };
    let (c, w) = witness(&decode_fp12(product).unwrap());
    (encode_fp12(&c), encode_fp12(&w))
}

/// The one-step path with honest witnesses for `committee` (empty ones if it is unknown), with
/// blst's batch sum and with the chain of additions: both must agree; returns the batch one.
fn one_step_path(
    store: &CompactStore,
    update: &LightClientUpdate,
    committee: Option<&SyncCommittee>,
    current_slot: u64,
    spec: &ChainSpec,
    gvr: &Root,
) -> Result<Applied, StepError> {
    let bits = &update.sync_aggregate.bits;
    let signers = committee.and_then(|c| NonSigners::build(c, bits).ok()).unwrap_or_default();
    let compact = CompactUpdate::of(update);
    let (c, w) = honest_witness(store, &compact, &signers, current_slot, spec, gvr);
    let batch = one_step(store, store, &compact, &signers, current_slot, &c, &w, spec, gvr, Sum::Batch);
    let chain = one_step(store, store, &compact, &signers, current_slot, &c, &w, spec, gvr, Sum::Chain);
    match (&batch, &chain) {
        (Ok(a), Ok(b)) => assert_eq!(a, b, "batch and chain sums give different results"),
        (Err(a), Err(b)) => assert_eq!(a.code(), b.code()),
        _ => panic!("batch {:?}, chain {:?}", batch.as_ref().map(|_| ()), chain.map(|_| ())),
    }
    batch
}

#[derive(Default, Debug)]
struct Tally {
    updates: usize,
    accepted: usize,
    handoffs: usize,
    refused: usize,
}

/// Both paths on one update: the same store, outcome and job (but for the job's `adoption`, `None`
/// after the summary adoption, the completed progress after the core's), or the same refusal code.
fn compare(
    label: &str,
    tally: &mut Tally,
    store: &CompactStore,
    update: &LightClientUpdate,
    committee: Option<&SyncCommittee>,
    current_slot: u64,
    spec: &ChainSpec,
    gvr: &Root,
) -> Result<Applied, StepError> {
    let core = core_steps(store, update, committee, current_slot, spec, gvr);
    let mine = one_step_path(store, update, committee, current_slot, spec, gvr);
    tally.updates += 1;
    match (&core, &mine) {
        (Ok((core_store, core_outcome, core_job)), Ok((store_1, outcome_1, job_1))) => {
            assert_eq!(store_1, core_store, "{label}: the stores differ");
            assert_eq!(outcome_1, core_outcome, "{label}: the outcomes differ");
            let mut expected = core_job.clone();
            if core_outcome.adopted {
                assert!(core_job.adoption.as_ref().is_some_and(|a| a.complete), "{label}: the core adopted");
                assert_eq!(job_1.adoption, None, "{label}: the summary adoption leaves no progress");
                expected.adoption = None;
            }
            assert_eq!(job_1, &expected, "{label}: the jobs differ beyond the adoption progress");
            tally.accepted += 1;
            tally.handoffs += usize::from(core_outcome.adopted);
        }
        (Err(a), Err(b)) => {
            assert_eq!(a.code(), b.code(), "{label}: refused with {a:?} (core) and {b:?} (one step)");
            tally.refused += 1;
        }
        _ => panic!("{label}: core {:?}, one step {:?}", core.as_ref().map(|_| ()), mine.as_ref().map(|_| ())),
    }
    core
}

// ---------------------------------------------------------------------------------------------
// Every handoff of the vectors and the real chains
// ---------------------------------------------------------------------------------------------

#[test]
fn one_step_equals_the_cores_steps_on_every_update_and_handoff() {
    let bls = Blst::new(Aggregation::Auto);
    let mut vectors = Tally::default();
    let all = cases("sync").expect("run scripts/fetch-vectors.sh first");
    assert_eq!(all.len(), 52);
    for (fork, dir) in &all {
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
        let mut policy =
            FinalizedStore::initialize_with(&trusted, &bootstrap, store_fork, spec, &bls, Adoption::OnChain).unwrap();
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
                    let update = upgrade_update(update, update_fork, policy.core().fork, spec).unwrap();
                    let current_slot = as_u64(&body["current_slot"]);
                    let store = CompactStore::of(policy.core());
                    let committee = signing_committee(policy.core(), &update, spec).cloned();
                    let label = format!("{name} step {i}");
                    let core =
                        compare(&label, &mut vectors, &store, &update, committee.as_ref(), current_slot, spec, &gvr);
                    let policy_result = policy.process_update(&update, current_slot, &ctx);
                    // The policy store moves exactly as the steps say.
                    match (core, policy_result) {
                        (Ok((next, ..)), Ok(_)) => assert_eq!(next, CompactStore::of(policy.core()), "{label}"),
                        (Err(_), Err(_)) => {}
                        (a, b) => panic!("{label}: steps {:?}, policy {:?}", a.map(|_| ()), b.map(|_| ())),
                    }
                }
                "force_update" => {}
                "upgrade_store" => {
                    let to = lc(cfg.fork_for_version(as_version(&body["store_fork_version"])).unwrap());
                    policy.upgrade(to, spec).unwrap();
                }
                other => panic!("{name}: unknown step {other}"),
            }
        }
    }
    assert!(vectors.handoffs > 0 && vectors.refused > 10, "the vectors exercise handoffs and refusals: {vectors:?}");

    // Mainnet: the bootstrap and five updates as served (each a period update).
    let spec = mainnet();
    let gvr = root(MAINNET_GVR);
    let ctx = Context { spec: &spec, genesis_validators_root: gvr, bls: &bls };
    let d = "mainnet-p1899";
    let bootstrap = LightClientBootstrap::decode(&fixture(d, "bootstrap.ssz"), LcFork::Electra, 512).unwrap();
    let mut policy = FinalizedStore::initialize_with(
        &root(MAINNET_TRUSTED),
        &bootstrap,
        LcFork::Electra,
        &spec,
        &bls,
        Adoption::OnChain,
    )
    .unwrap();
    let mut mainnet_tally = Tally::default();
    for i in 0..5 {
        let u = LightClientUpdate::decode(&fixture(d, &format!("update_{i}.ssz")), LcFork::Electra, 512).unwrap();
        let store = CompactStore::of(policy.core());
        let committee = signing_committee(policy.core(), &u, &spec).cloned();
        let (next, ..) = compare(
            &format!("mainnet update_{i}"),
            &mut mainnet_tally,
            &store,
            &u,
            committee.as_ref(),
            u.signature_slot,
            &spec,
            &gvr,
        )
        .unwrap_or_else(|e| panic!("mainnet update_{i}: {e:?}"));
        policy.process_update(&u, u.signature_slot + 1, &ctx).unwrap();
        assert_eq!(next, CompactStore::of(policy.core()), "mainnet update_{i}: the policy store");
    }

    // Sepolia through Gloas: 32 period updates and the finality update.
    let spec = sepolia();
    let gvr = root(SEPOLIA_GVR);
    let ctx = Context { spec: &spec, genesis_validators_root: gvr, bls: &bls };
    let d = "sepolia-gloas-2026-11-10";
    let boot = LightClientBootstrap::decode(&fixture(d, "bootstrap.ssz"), LcFork::Electra, 512).unwrap();
    let mut policy =
        FinalizedStore::initialize_with(&root(SEPOLIA_TRUSTED), &boot, LcFork::Electra, &spec, &bls, Adoption::OnChain)
            .unwrap();
    let mut sepolia_tally = Tally::default();
    for period in 1378..=1409u64 {
        if period == 1379 {
            policy.upgrade(LcFork::Gloas, &spec).unwrap();
        }
        let layout = if period < 1379 { LcFork::Electra } else { LcFork::Gloas };
        let u = LightClientUpdate::decode(&fixture(d, &format!("update_{period}.ssz")), layout, 512).unwrap();
        let store = CompactStore::of(policy.core());
        let committee = signing_committee(policy.core(), &u, &spec).cloned();
        let label = format!("Sepolia period {period}");
        let (next, ..) =
            compare(&label, &mut sepolia_tally, &store, &u, committee.as_ref(), u.signature_slot, &spec, &gvr)
                .unwrap_or_else(|e| panic!("{label}: {e:?}"));
        policy.process_update(&u, u.signature_slot + 1, &ctx).unwrap();
        assert_eq!(next, CompactStore::of(policy.core()), "{label}: the policy store");
    }
    let fu = sepolia_finality_update(&fixture(d, "finality_update.ssz"));
    let store = CompactStore::of(policy.core());
    let committee = signing_committee(policy.core(), &fu, &spec).cloned();
    compare("Sepolia finality update", &mut sepolia_tally, &store, &fu, committee.as_ref(), fu.signature_slot, &spec, &gvr)
        .unwrap();
    assert!(mainnet_tally.handoffs > 0 && sepolia_tally.handoffs > 0, "the real chains have handoffs");
    println!(
        "one step = the core's steps (store, outcome, job but its adoption progress; or the refusal code):\n  \
         official sync vectors: {vectors:?}\n  mainnet replay: {mainnet_tally:?}\n  Sepolia through Gloas: \
         {sepolia_tally:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// Refusals of a handoff (mainnet update_1 as served: a period update)
// ---------------------------------------------------------------------------------------------

/// The store after mainnet update_0, update_1 as served (a handoff), its real signing committee, and
/// the bench's committee (seed 7) substituted in the store for it.
struct Base {
    spec: ChainSpec,
    gvr: Root,
    store: CompactStore,
    real: SyncCommittee,
    substituted: CompactStore,
    fake: ferrata_spectests::testkit::FakeCommittee,
    handoff: LightClientUpdate,
    older: LightClientUpdate,
}

fn base() -> Base {
    let spec = mainnet();
    let gvr = root(MAINNET_GVR);
    let bls = Blst::new(Aggregation::Auto);
    let ctx = Context { spec: &spec, genesis_validators_root: gvr, bls: &bls };
    let d = "mainnet-p1899";
    let bootstrap = LightClientBootstrap::decode(&fixture(d, "bootstrap.ssz"), LcFork::Electra, 512).unwrap();
    let u0 = LightClientUpdate::decode(&fixture(d, "update_0.ssz"), LcFork::Electra, 512).unwrap();
    let u1 = LightClientUpdate::decode(&fixture(d, "update_1.ssz"), LcFork::Electra, 512).unwrap();
    let mut policy = FinalizedStore::initialize_with(
        &root(MAINNET_TRUSTED),
        &bootstrap,
        LcFork::Electra,
        &spec,
        &bls,
        Adoption::OnChain,
    )
    .unwrap();
    policy.process_update(&u0, u0.signature_slot + 1, &ctx).unwrap();
    let store = CompactStore::of(policy.core());
    let real = signing_committee(policy.core(), &u1, &spec).unwrap().clone();
    let fake = fake_committee(512, 7);
    let mut substituted = store.clone();
    if substituted.current == CommitteeSummary::of(&real) {
        substituted.current = CommitteeSummary::of(&fake.committee);
    } else {
        substituted.next = Some(CommitteeSummary::of(&fake.committee));
    }
    Base { spec, gvr, store, real, substituted, fake, handoff: u1, older: u0 }
}

/// `update` signed by the bench's committee with `bits`.
fn signed(b: &Base, update: &LightClientUpdate, bits: Vec<bool>) -> LightClientUpdate {
    let bls = Blst::new(Aggregation::Auto);
    let ctx = Context { spec: &b.spec, genesis_validators_root: b.gvr, bls: &bls };
    let mut u = update.clone();
    u.sync_aggregate.bits = bits;
    u.sync_aggregate.signature = sign(&b.fake, &u.sync_aggregate.bits, &sync_committee_signing_root(&u, &ctx));
    u
}

/// Both paths refuse with `want`.
fn refused_alike(label: &str, store: &CompactStore, u: &LightClientUpdate, c: &SyncCommittee, b: &Base, want: u16) {
    let mut tally = Tally::default();
    match compare(label, &mut tally, store, u, Some(c), u.signature_slot, &b.spec, &b.gvr) {
        Err(e) => assert_eq!(e.code(), want, "{label}: refused with {e:?}"),
        Ok(_) => panic!("{label}: accepted"),
    }
    println!("refused alike by the core's steps and the one-step path: {label}: code {want}");
}

#[test]
fn refusals_of_a_handoff_are_the_cores() {
    let b = base();
    // The honest handoff first: accepted alike, a committee adopted.
    let mut tally = Tally::default();
    let (after, outcome, _) =
        compare("update_1 (handoff)", &mut tally, &b.store, &b.handoff, Some(&b.real), b.handoff.signature_slot, &b.spec, &b.gvr)
            .unwrap();
    assert!(outcome.adopted, "update_1 as served adopts a committee");
    assert_eq!(b.handoff.sync_aggregate.participants(), 510);

    // A bad next-committee branch: one node altered; the next committee's aggregate altered, its
    // branch unchanged; one of its keys altered.
    for level in [0usize, 3, 5] {
        let mut u = b.handoff.clone();
        u.next_sync_committee_branch[level][7] ^= 1;
        refused_alike(&format!("next-committee branch node {level} altered"), &b.store, &u, &b.real, &b, 27);
    }
    let mut u = b.handoff.clone();
    u.next_sync_committee.aggregate_pubkey = b.real.pubkeys[0];
    refused_alike("next committee's aggregate replaced, branch unchanged", &b.store, &u, &b.real, &b, 27);
    let mut u = b.handoff.clone();
    u.next_sync_committee.pubkeys[100] = u.next_sync_committee.pubkeys[101];
    refused_alike("one key of the next committee replaced, branch unchanged", &b.store, &u, &b.real, &b, 27);

    // Below the policy: 341 of 512 signers (by the bench's committee), and no finality.
    let bits_341: Vec<bool> = (0..512).map(|i| i >= 171).collect();
    let u = signed(&b, &b.handoff, bits_341);
    refused_alike("341 of 512 signers", &b.substituted, &u, &b.fake.committee, &b, 51);
    let u = signed(&b, &b.handoff, (0..512).map(|i| i % 3 != 2).collect());
    let mut tally = Tally::default();
    compare("342 of 512 signers", &mut tally, &b.substituted, &u, Some(&b.fake.committee), u.signature_slot, &b.spec, &b.gvr)
        .unwrap();
    let mut u = b.handoff.clone();
    u.finality_branch = vec![ZERO; u.finality_branch.len()];
    u.finalized_header = LightClientHeader::empty(LcFork::Electra);
    refused_alike("a period update without finality", &b.store, &u, &b.real, &b, 50);

    // Stale: the same handoff again on the store it produced, and the older update_0.
    for (label, u) in [("update_1 again, after it was applied", &b.handoff), ("update_0, older than the store", &b.older)] {
        let code = core_steps(&after, u, Some(&b.real), u.signature_slot, &b.spec, &b.gvr).unwrap_err().code();
        assert!([23, 24, 52].contains(&code), "{label}: refused as stale (PeriodSkipped, UpdateIrrelevant or NotNewer), not {code}");
        refused_alike(label, &after, u, &b.real, &b, code);
    }
}

#[test]
fn the_summary_adoption_refuses_a_job_that_adopts_nothing() {
    let b = base();
    // A finality update adopts nothing.
    let u = as_finality(&b.handoff);
    let compact = CompactUpdate::of(&u);
    let signers = NonSigners::build(&b.real, &u.sync_aggregate.bits).unwrap();
    let mut job =
        residue_agg::step1(&b.store, &compact, &signers, u.signature_slot, &b.spec, &b.gvr, 1, Sum::Batch).unwrap();
    assert_eq!(job.adoption, None);
    assert_eq!(adopt_summary(&mut job), Err(StepError::NoAdoption));
    // An initialization keeps the core's adoption.
    let bootstrap =
        LightClientBootstrap::decode(&fixture("mainnet-p1899", "bootstrap.ssz"), LcFork::Electra, 512).unwrap();
    let compact = ferrata_core::witness::CompactBootstrap::of(&bootstrap);
    let (_, mut init) = steps::initialize(&root(MAINNET_TRUSTED), &compact, LcFork::Electra, &b.spec, 1).unwrap();
    assert!(matches!(init.pending, PendingApply::Activate { .. }) && init.adoption_pending());
    assert_eq!(adopt_summary(&mut init), Err(StepError::NoAdoption));
    assert!(init.adoption_pending(), "a refused summary adoption changes nothing");
    // A handoff's job: adopted once, then nothing left to adopt.
    let compact = CompactUpdate::of(&b.handoff);
    let signers = NonSigners::build(&b.real, &b.handoff.sync_aggregate.bits).unwrap();
    let mut job =
        residue_agg::step1(&b.store, &compact, &signers, b.handoff.signature_slot, &b.spec, &b.gvr, 1, Sum::Batch)
            .unwrap();
    assert!(job.adoption_pending());
    assert_eq!(adopt_summary(&mut job), Ok(()));
    assert!(!job.adoption_pending());
    assert_eq!(adopt_summary(&mut job), Err(StepError::NoAdoption));
}

// ---------------------------------------------------------------------------------------------
// The one difference, and what it does (bench/RESIDUE.md §10.3)
// ---------------------------------------------------------------------------------------------

/// A store of the Electra `sync` vector case whose current committee is a committee whose keys the
/// test holds, and fabricated updates about a chain that never existed (as
/// `ferrata-spectests/tests/mutation_guards.rs`, `policy_checks_a_committee_at_adoption`): a
/// handoff to a next committee whose aggregate field is not the sum of its keys (the first key in
/// its place) is refused by the core's adoption and accepted by the summary adoption; afterwards the
/// honest committee's signatures are refused and its first member alone signs for all of it. From
/// the consistent handoff, the reverse.
#[test]
fn an_aggregate_that_is_not_the_sum_is_the_one_difference() {
    let bls = Blst::new(Aggregation::Auto);
    let case = load_sync_case(&vectors_dir().join("electra/light_client/sync/pyspec_tests/light_client_sync")).unwrap();
    let spec = &case.config.spec;
    let gvr = case.genesis_validators_root;
    let ctx = Context { spec, genesis_validators_root: gvr, bls: &bls };
    let (u, current) = &case.updates[0];
    let (finalized, attested, signature) = (u.finalized_header.beacon.slot, u.attested_header.beacon.slot, u.signature_slot);
    let fake = fake_committee(32, 0x22);
    let next = fake_committee(32, 0x23);
    let policy = FinalizedStore::initialize(&case.trusted_block_root, &case.bootstrap, case.store_fork, spec, &bls).unwrap();
    let core = StoreCore { current_sync_committee: fake.committee.clone(), ..policy.core().clone() };
    let store = CompactStore::of(&core);
    let handoff = |committee: &SyncCommittee| {
        let f = Fabrication {
            finalized_slot: Some(finalized),
            attested_slot: attested,
            signature_slot: signature,
            signers: 32,
            next_committee: Some(committee),
        };
        fabricated_update(&f, &fake, &ctx)
    };
    let inconsistent = with_wrong_aggregate(&next.committee);
    assert_ne!(inconsistent.aggregate_pubkey, next.committee.aggregate_pubkey);

    // The handoff: the core refuses at its adoption, the summary adoption accepts.
    let forged = handoff(&inconsistent);
    let refused = core_steps(&store, &forged, Some(&fake.committee), *current, spec, &gvr).unwrap_err();
    assert_eq!(refused, StepError::Core(Error::CommitteeInvalid), "the core's adoption refuses");
    let (after_forged, outcome, _) =
        one_step_path(&store, &forged, Some(&fake.committee), *current, spec, &gvr).expect("the one step accepts");
    assert!(outcome.adopted);
    assert_eq!(after_forged.next, Some(CommitteeSummary::of(&inconsistent)));
    let honest = handoff(&next.committee);
    let mut tally = Tally::default();
    let (after_honest, ..) =
        compare("the consistent handoff", &mut tally, &store, &honest, Some(&fake.committee), *current, spec, &gvr)
            .unwrap();

    // The next period, signed by the adopted committee: all 32 members honestly, or the first
    // member alone with every bit set.
    let len = spec.preset.slots_per_epoch * spec.preset.epochs_per_sync_committee_period;
    let f = Fabrication {
        finalized_slot: Some(finalized + len),
        attested_slot: attested + len,
        signature_slot: signature + len,
        signers: 32,
        next_committee: None,
    };
    let by_all = fabricated_update(&f, &next, &ctx);
    let mut by_one = by_all.clone();
    by_one.sync_aggregate.signature =
        next.keys[0].sign(&sync_committee_signing_root(&by_one, &ctx), DST, &[]).compress();
    let later = current + len;
    let run = |s: &CompactStore, u: &LightClientUpdate| one_step_path(s, u, Some(&next.committee), later, spec, &gvr);
    assert_eq!(
        run(&after_forged, &by_all).unwrap_err(),
        StepError::Core(Error::SignatureInvalid),
        "after the inconsistent adoption the honest committee is refused"
    );
    let (taken, ..) = run(&after_forged, &by_one).expect("after the inconsistent adoption one member signs for all");
    assert_eq!(taken.current, CommitteeSummary::of(&inconsistent), "the store rotated to it");
    assert!(run(&after_honest, &by_all).is_ok(), "after the consistent adoption the honest committee is accepted");
    assert_eq!(run(&after_honest, &by_one).unwrap_err(), StepError::Core(Error::SignatureInvalid));
    let mut tally = Tally::default();
    compare("the next period after the consistent handoff", &mut tally, &after_honest, &by_all, Some(&next.committee), later, spec, &gvr)
        .unwrap();
    println!(
        "an aggregate field that is not the sum of the keys: the core's adoption refuses the handoff (code 45), \
         the summary adoption accepts it; afterwards the honest committee's signature is refused (40) and its \
         first member alone, every bit set, is accepted. The consistent handoff: accepted alike, and the reverse."
    );
}
