//! Evidence of conflicting finality (`misbehaviour`, `bench/MISBEHAVIOUR.md`). Experimental and unaudited.
//!
//! * `honest_data_never_freezes`: every update of the official `sync` vectors, of the mainnet replay, of the Sepolia
//!   chain through Gloas and of the lab's record (the mainnet and Sepolia instances the lab ran on its LEZ sequencer,
//!   every update they applied), submitted as evidence against the instance's records and its adopted next committee
//!   at every state of the replay: none is accepted. Every one that passes the instance's checks has a signature
//!   that verifies under the stored committee: it is refused for contradicting nothing. `validate_evidence` is also
//!   compared with the core's `validate_compact` on every pair (they differ in the two checks it drops only).
//! * `real_contradictions_freeze`: on a chain that never existed (a committee the test holds), a recorded finalized
//!   header contradicted at its slot, the checkpoint's record contradicted, and an adopted next committee
//!   contradicted: each refused as an update, accepted as evidence; the frozen instance refuses updates,
//!   attestations and further evidence with its code, and keeps its records and store.
//! * `griefing_and_replay`: a committee the instance does not hold (another period, or another key set), an
//!   unfinalized update, below two thirds, an honest update replayed, a bad finality branch, a future signature
//!   slot, the genesis placeholder, a slot with no record, the weak-subjectivity bound, a next committee of another
//!   period's finality; the residue-witness pairing (honest, and one flipped bit).
//!
//! Needs the vectors (`scripts/fetch-vectors.sh`).

use std::{collections::BTreeMap, fs, path::PathBuf};

use ferrata_core::{
    bls::{Aggregation, Blst},
    fork::{ChainSpec, LcFork, Preset, FAR_FUTURE_EPOCH},
    merkle::{FINALIZED_ROOT_GINDEX_ELECTRA, NEXT_SYNC_COMMITTEE_GINDEX_ELECTRA, ZERO},
    policy::{Adoption, FinalizedStore},
    steps::{self, Phase},
    store::StoreCore,
    types::{LightClientBootstrap, LightClientHeader, LightClientUpdate, SyncAggregate, SyncCommittee},
    upgrade::{upgrade_bootstrap, upgrade_update},
    validate::{sync_committee_signing_root, Context},
    witness::{CommitteeSummary, CompactStore, CompactUpdate},
    Error, Root,
};
use ferrata_spectests::{
    cases, load_sync_case, parse_config, read_ssz_snappy,
    testkit::{fabricated_header, fake_committee, sign, sparse_tree, FakeCommittee},
    vectors_dir,
};
use misbehaviour::{
    check_evidence, signature_job, validate_evidence, verify_signature, Against, Contradiction, EvidenceError,
    Instance, Pairing, FROZEN, NO_CONTRADICTION, NO_RECORD,
};
use residue::{decode_fp12, encode_fp12, witness};
use residue_agg::{NonSigners, Sum};
use serde_yaml::Value;
use sha2::{Digest, Sha256};

static BLS: Blst = Blst::new(Aggregation::Auto);

// ---------------------------------------------------------------------------------------------
// Data (as residue-handoff's tests)
// ---------------------------------------------------------------------------------------------

fn repo(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel)
}

/// A fixture file, checked against its directory's pinned `SHA256SUMS`.
fn fixture(dir: &str, name: &str) -> Vec<u8> {
    let d = repo("crates/ferrata-spectests/fixtures").join(dir);
    let bytes = fs::read(d.join(name)).unwrap();
    let sums = fs::read_to_string(d.join("SHA256SUMS")).unwrap();
    let want = sums.lines().find(|l| l.ends_with(&format!("  {name}"))).unwrap().split_whitespace().next().unwrap();
    assert_eq!(hex::encode(Sha256::digest(&bytes)), want, "{dir}/{name} does not match SHA256SUMS");
    bytes
}

fn root(s: &str) -> Root {
    hex::decode(s.trim().trim_start_matches("0x")).unwrap().try_into().unwrap()
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

/// A `LightClientFinalityUpdate` as the `LightClientUpdate` it stands for (empty committee fields): Gloas (fixed
/// size) or Electra (two header offsets). The decoding of `lez/lab/driver/src/lab.rs`, `decode_finality_update`.
fn finality_update(b: &[u8], layout: LcFork) -> LightClientUpdate {
    let u64_at = |o: usize| u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
    match layout {
        LcFork::Gloas => {
            const H: usize = 112 + 32 + 11 * 32;
            assert_eq!(b.len(), 2 * H + 9 * 32 + 160 + 8);
            LightClientUpdate {
                attested_header: LightClientHeader::decode(&b[..H], LcFork::Gloas).unwrap(),
                next_sync_committee: SyncCommittee::empty(512),
                next_sync_committee_branch: vec![ZERO; LcFork::Gloas.sync_committee_branch_len()],
                finalized_header: LightClientHeader::decode(&b[H..2 * H], LcFork::Gloas).unwrap(),
                finality_branch: b[2 * H..2 * H + 288].chunks_exact(32).map(|c| c.try_into().unwrap()).collect(),
                sync_aggregate: SyncAggregate::decode(&b[2 * H + 288..2 * H + 448], 512).unwrap(),
                signature_slot: u64_at(2 * H + 448),
            }
        }
        LcFork::Electra => {
            let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as usize;
            let (att, fin) = (u32_at(0), u32_at(4));
            assert!(att == 400 && fin > att && fin < b.len());
            LightClientUpdate {
                attested_header: LightClientHeader::decode(&b[att..fin], LcFork::Electra).unwrap(),
                next_sync_committee: SyncCommittee::empty(512),
                next_sync_committee_branch: vec![ZERO; LcFork::Electra.sync_committee_branch_len()],
                finalized_header: LightClientHeader::decode(&b[fin..], LcFork::Electra).unwrap(),
                finality_branch: b[8..232].chunks_exact(32).map(|c| c.try_into().unwrap()).collect(),
                sync_aggregate: SyncAggregate::decode(&b[232..392], 512).unwrap(),
                signature_slot: u64_at(392),
            }
        }
        other => panic!("no finality update layout {other:?}"),
    }
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
// Honest data
// ---------------------------------------------------------------------------------------------

enum Step {
    /// An update in its layout and the current slot it is processed at.
    Update(LightClientUpdate, LcFork, u64),
    Upgrade(LcFork),
}

/// One replay: a chain, its bootstrap and the steps the instance went through.
struct Replay {
    name: String,
    spec: ChainSpec,
    gvr: Root,
    trusted: Root,
    bootstrap: LightClientBootstrap,
    fork: LcFork,
    steps: Vec<Step>,
}

#[derive(Default, Debug)]
struct Tally {
    states: usize,
    updates: usize,
    /// Evidence submitted (an update against the record at its finalized slot, if any, and against the
    /// adopted next committee).
    submitted: usize,
    accepted: usize,
    /// Refusals by code.
    refused: BTreeMap<u16, usize>,
    /// (state, update) pairs that pass the instance's checks of evidence and whose signature verifies under the
    /// stored committee.
    verified: usize,
    /// Of those, against a record at the update's finalized slot (the same root, so no contradiction).
    against_a_record: usize,
    /// (state, update) pairs whose `validate_evidence` was compared with `validate_compact`.
    compared: usize,
    /// Updates the replay's policy store applied.
    applied: usize,
}

/// The committee the policy store holds for `update`'s signature period, if any.
fn held<'a>(full: &'a StoreCore, update: &LightClientUpdate, spec: &ChainSpec) -> Option<&'a SyncCommittee> {
    let c = if spec.period_at_slot(update.signature_slot) == spec.period_at_slot(full.finalized_header.beacon.slot) {
        &full.current_sync_committee
    } else {
        &full.next_sync_committee
    };
    (!c.is_empty()).then_some(c)
}

/// `validate_evidence` against `validate_compact` on one pair: equal when the update passes the policy, except
/// where `validate_compact` refuses for one of the two checks evidence drops.
fn compare(store: &CompactStore, cu: &CompactUpdate, slot: u64, spec: &ChainSpec, gvr: &Root, label: &str) -> bool {
    let n = spec.preset.sync_committee_size as u64;
    if !cu.is_finality_update() || cu.sync_aggregate.participants() * 3 < n * 2 || cu.finalized_header.beacon.slot == 0 {
        return false;
    }
    let core = steps::validate_compact(store, cu, slot, spec, gvr);
    let evidence = validate_evidence(store, cu, slot, spec, gvr);
    match (&core, &evidence) {
        (Err(Error::UpdateIrrelevant | Error::NextCommitteeMismatch), _) => {}
        (a, b) => assert_eq!(a, b, "{label}: validate_compact and validate_evidence differ"),
    }
    true
}

fn run(replay: &Replay) -> Tally {
    let spec = &replay.spec;
    let gvr = replay.gvr;
    let ctx = Context { spec, genesis_validators_root: gvr, bls: &BLS };
    let mut policy =
        FinalizedStore::initialize_with(&replay.trusted, &replay.bootstrap, replay.fork, spec, &BLS, Adoption::OnChain)
            .unwrap();
    let mut records: BTreeMap<u64, Root> = BTreeMap::new();
    records.insert(replay.bootstrap.header.beacon.slot, replay.bootstrap.header.beacon.hash_tree_root());
    let updates: Vec<(&LightClientUpdate, LcFork)> = replay
        .steps
        .iter()
        .filter_map(|s| match s {
            Step::Update(u, f, _) => Some((u, *f)),
            Step::Upgrade(_) => None,
        })
        .collect();
    let mut t = Tally { updates: updates.len(), ..Tally::default() };

    let evidence_round = |policy: &FinalizedStore, records: &BTreeMap<u64, Root>, t: &mut Tally, at: &str| {
        t.states += 1;
        let stored = CompactStore::of(policy.core());
        for (j, (u, ufork)) in updates.iter().enumerate() {
            let label = format!("{} {at}, update {j}", replay.name);
            // The submitter brings the update to the store's layout (or the store to the update's, as an update).
            let (u, layout) = if *ufork < stored.fork {
                (upgrade_update((*u).clone(), *ufork, stored.fork, spec).unwrap(), stored.fork)
            } else {
                ((*u).clone(), *ufork)
            };
            let cu = CompactUpdate::of(&u);
            let mut against = vec![Against::NextCommittee];
            if let Some(r) = records.get(&u.finalized_header.beacon.slot) {
                against.push(Against::Record { slot: u.finalized_header.beacon.slot, root: *r });
            }
            let current_slot = u.signature_slot;
            let store = match steps::store_for_update(&stored, layout, u.attested_header.beacon.slot, spec) {
                Ok(s) => s,
                Err(e) => {
                    t.submitted += against.len();
                    *t.refused.entry(e.code()).or_default() += against.len();
                    continue;
                }
            };
            if compare(&store, &cu, current_slot, spec, &gvr, &label) {
                t.compared += 1;
            }
            let validated = validate_evidence(&store, &cu, current_slot, spec, &gvr);
            let signers = match (&validated, held(policy.core(), &u, spec)) {
                (Ok(_), Some(c)) => NonSigners::build(c, &u.sync_aggregate.bits).unwrap(),
                _ => NonSigners::default(),
            };
            if let Ok((signer, signing_root)) = &validated {
                // The update is valid under the instance's checks: its signature verifies under the stored committee.
                let mut job = signature_job(&store, &cu, *signer, signing_root, &signers, Sum::Batch, spec)
                    .unwrap_or_else(|e| panic!("{label}: the signing key of an honest update: {e:?}"));
                verify_signature(&mut job, &Pairing::FinalExp)
                    .unwrap_or_else(|e| panic!("{label}: an honest update's signature: {e:?}"));
                t.verified += 1;
                if against.len() == 2 {
                    t.against_a_record += 1;
                }
            }
            for a in &against {
                t.submitted += 1;
                let r = check_evidence(&store, &cu, &signers, Sum::Batch, a, current_slot, &Pairing::FinalExp, spec, &gvr);
                match r {
                    Ok(freeze) => {
                        t.accepted += 1;
                        panic!("{label}: honest data accepted as evidence against {a:?}: {freeze:?}");
                    }
                    Err(e) => *t.refused.entry(e.code()).or_default() += 1,
                }
            }
        }
    };

    evidence_round(&policy, &records, &mut t, "at the bootstrap");
    for (i, s) in replay.steps.iter().enumerate() {
        match s {
            Step::Upgrade(to) => policy.upgrade(*to, spec).unwrap(),
            Step::Update(u, ufork, current_slot) => {
                if *ufork > policy.core().fork {
                    policy.upgrade(*ufork, spec).unwrap();
                }
                let u = upgrade_update(u.clone(), *ufork, policy.core().fork, spec).unwrap();
                if let Ok(outcome) = policy.process_update(&u, *current_slot, &ctx) {
                    t.applied += 1;
                    if outcome.advanced {
                        let h = &policy.core().finalized_header.beacon;
                        records.insert(h.slot, h.hash_tree_root());
                    }
                }
                evidence_round(&policy, &records, &mut t, &format!("after step {i}"));
            }
        }
    }
    t
}

fn vector_replays() -> Vec<Replay> {
    let all = cases("sync").expect("run scripts/fetch-vectors.sh first");
    assert_eq!(all.len(), 52);
    all.iter()
        .map(|(fork, dir)| {
            let name = format!("{}/{}", fork.name(), dir.file_name().unwrap().to_string_lossy());
            let meta = yaml(dir.join("meta.yaml"));
            let cfg = parse_config(&dir.join("config.yaml")).unwrap();
            let spec = cfg.spec.clone();
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
            let bootstrap = upgrade_bootstrap(bootstrap, bootstrap_fork, store_fork, &spec).unwrap();
            let mut steps = Vec::new();
            for step in yaml(dir.join("steps.yaml")).as_sequence().unwrap() {
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
                        steps.push(Step::Update(update, update_fork, as_u64(&body["current_slot"])));
                    }
                    "upgrade_store" => {
                        steps.push(Step::Upgrade(lc(cfg.fork_for_version(as_version(&body["store_fork_version"])).unwrap())))
                    }
                    "force_update" => {}
                    other => panic!("{name}: unknown step {other}"),
                }
            }
            Replay { name, spec, gvr, trusted, bootstrap, fork: store_fork, steps }
        })
        .collect()
}

fn fixture_replays() -> Vec<Replay> {
    let d = "mainnet-p1899";
    let mainnet_steps = (0..5)
        .map(|i| {
            let u = LightClientUpdate::decode(&fixture(d, &format!("update_{i}.ssz")), LcFork::Electra, 512).unwrap();
            let slot = u.signature_slot + 1;
            Step::Update(u, LcFork::Electra, slot)
        })
        .collect();
    let mainnet_replay = Replay {
        name: "mainnet replay (mainnet-p1899)".into(),
        spec: mainnet(),
        gvr: root(MAINNET_GVR),
        trusted: root(MAINNET_TRUSTED),
        bootstrap: LightClientBootstrap::decode(&fixture(d, "bootstrap.ssz"), LcFork::Electra, 512).unwrap(),
        fork: LcFork::Electra,
        steps: mainnet_steps,
    };
    let d = "sepolia-gloas-2026-11-10";
    let mut steps = Vec::new();
    for period in 1378..=1409u64 {
        let layout = if period < 1379 { LcFork::Electra } else { LcFork::Gloas };
        let u = LightClientUpdate::decode(&fixture(d, &format!("update_{period}.ssz")), layout, 512).unwrap();
        let slot = u.signature_slot + 1;
        steps.push(Step::Update(u, layout, slot));
    }
    let fu = finality_update(&fixture(d, "finality_update.ssz"), LcFork::Gloas);
    let slot = fu.signature_slot + 1;
    steps.push(Step::Update(fu, LcFork::Gloas, slot));
    let sepolia_replay = Replay {
        name: "Sepolia through Gloas (sepolia-gloas-2026-11-10)".into(),
        spec: sepolia(),
        gvr: root(SEPOLIA_GVR),
        trusted: root(SEPOLIA_TRUSTED),
        bootstrap: LightClientBootstrap::decode(&fixture(d, "bootstrap.ssz"), LcFork::Electra, 512).unwrap(),
        fork: LcFork::Electra,
        steps,
    };
    vec![mainnet_replay, sepolia_replay]
}

/// The lab's record (`lez/lab/results/record/net/<chain>/`): the bootstrap, the checkpoint and the served bytes of
/// every update the lab's instance applied, in order.
fn lab_replay(chain: &str) -> Replay {
    let dir = repo("lez/lab/results/record/net").join(chain);
    let (spec, gvr) = match chain {
        "mainnet" => (mainnet(), root(MAINNET_GVR)),
        _ => (sepolia(), root(SEPOLIA_GVR)),
    };
    let mut names: Vec<String> =
        fs::read_dir(dir.join("updates")).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    names.sort();
    let steps = names
        .iter()
        .map(|name| {
            let parts: Vec<&str> = name.split('.').collect();
            let layout = match parts[2] {
                "electra" => LcFork::Electra,
                "gloas" => LcFork::Gloas,
                other => panic!("{name}: layout {other}"),
            };
            let bytes = fs::read(dir.join("updates").join(name)).unwrap();
            let u = match parts[1] {
                "period" => LightClientUpdate::decode(&bytes, layout, 512).unwrap(),
                "finality" => finality_update(&bytes, layout),
                other => panic!("{name}: kind {other}"),
            };
            let slot = u.signature_slot + 1;
            Step::Update(u, layout, slot)
        })
        .collect();
    Replay {
        name: format!("lab record, {chain}"),
        spec,
        gvr,
        trusted: root(&fs::read_to_string(dir.join("checkpoint")).unwrap()),
        bootstrap: LightClientBootstrap::decode(&fs::read(dir.join("bootstrap.ssz")).unwrap(), LcFork::Electra, 512)
            .unwrap(),
        fork: LcFork::Electra,
        steps,
    }
}

#[test]
fn honest_data_never_freezes() {
    let mut sets: Vec<(String, Vec<Replay>)> = vec![("official sync vectors (52 cases)".into(), vector_replays())];
    for r in fixture_replays() {
        sets.push((r.name.clone(), vec![r]));
    }
    for chain in ["mainnet", "sepolia"] {
        let r = lab_replay(chain);
        sets.push((r.name.clone(), vec![r]));
    }
    let mut lines = vec![
        "| data | states | updates (applied) | evidence submitted | accepted | refused (code: count) | valid under the stored committee (of which at a recorded slot) | validate_evidence = validate_compact |".to_string(),
        "|---|---:|---:|---:|---:|---|---:|---:|".to_string(),
    ];
    let mut total = Tally::default();
    for (name, replays) in &sets {
        let mut t = Tally::default();
        for r in replays {
            let x = run(r);
            t.states += x.states;
            t.updates += x.updates;
            t.submitted += x.submitted;
            t.accepted += x.accepted;
            t.verified += x.verified;
            t.against_a_record += x.against_a_record;
            t.compared += x.compared;
            t.applied += x.applied;
            for (k, v) in x.refused {
                *t.refused.entry(k).or_default() += v;
            }
        }
        assert_eq!(t.accepted, 0, "{name}");
        if !name.starts_with("official") {
            assert_eq!(t.applied, t.updates, "{name}: every update of a real chain's replay applies");
        }
        assert_eq!(t.refused.values().sum::<usize>(), t.submitted, "{name}");
        let refused: Vec<String> = t.refused.iter().map(|(k, v)| format!("{k}: {v}")).collect();
        lines.push(format!(
            "| {name} | {} | {} ({}) | {} | {} | {} | {} ({}) | {} |",
            t.states,
            t.updates,
            t.applied,
            t.submitted,
            t.accepted,
            refused.join(", "),
            t.verified,
            t.against_a_record,
            t.compared
        ));
        total.states += t.states;
        total.updates += t.updates;
        total.submitted += t.submitted;
        total.verified += t.verified;
        total.against_a_record += t.against_a_record;
        total.compared += t.compared;
        total.applied += t.applied;
        for (k, v) in t.refused {
            *total.refused.entry(k).or_default() += v;
        }
    }
    let refused: Vec<String> = total.refused.iter().map(|(k, v)| format!("{k}: {v}")).collect();
    lines.push(format!(
        "| **all** | {} | {} ({}) | {} | 0 | {} | {} ({}) | {} |",
        total.states,
        total.updates,
        total.applied,
        total.submitted,
        refused.join(", "),
        total.verified,
        total.against_a_record,
        total.compared
    ));
    assert!(total.verified > 0 && total.against_a_record > 0, "the replays exercise the signature path and records");
    println!("honest data submitted as evidence:\n{}", lines.join("\n"));
}

// ---------------------------------------------------------------------------------------------
// A chain that never existed: a committee the test holds
// ---------------------------------------------------------------------------------------------

struct World {
    spec: ChainSpec,
    gvr: Root,
    /// The first slot of the instance's period (its checkpoint's slot).
    base: u64,
    len: u64,
    current: FakeCommittee,
    next: FakeCommittee,
    rival: FakeCommittee,
    stranger: FakeCommittee,
}

fn world() -> World {
    let case = load_sync_case(&vectors_dir().join("electra/light_client/sync/pyspec_tests/light_client_sync")).unwrap();
    let spec = case.config.spec.clone();
    let len = spec.preset.slots_per_epoch * spec.preset.epochs_per_sync_committee_period;
    // A period after the case's bootstrap (its updates are in the Electra layout, so is every slot after it).
    let base = (spec.period_at_slot(case.bootstrap.header.beacon.slot) + 2) * len;
    World {
        spec,
        gvr: case.genesis_validators_root,
        base,
        len,
        current: fake_committee(32, 0x41),
        next: fake_committee(32, 0x42),
        rival: fake_committee(32, 0x43),
        stranger: fake_committee(32, 0x44),
    }
}

/// An Electra-layout update whose finalized header (if any) is `finalized`, proven in a fabricated attested state,
/// signed by the members of `signer` whose bit is set.
fn fabricate(
    w: &World,
    finalized: Option<&LightClientHeader>,
    attested_slot: u64,
    signature_slot: u64,
    bits: &[bool],
    next: Option<&SyncCommittee>,
    signer: &FakeCommittee,
) -> LightClientUpdate {
    let n = bits.len();
    let mut attested = fabricated_header(attested_slot, 80 + attested_slot);
    let mut leaves = Vec::new();
    if let Some(h) = finalized {
        leaves.push((FINALIZED_ROOT_GINDEX_ELECTRA, h.beacon.hash_tree_root()));
    }
    if let Some(c) = next {
        leaves.push((NEXT_SYNC_COMMITTEE_GINDEX_ELECTRA, c.hash_tree_root()));
    }
    let (state_root, branches) = sparse_tree(&leaves, 7_000 + attested_slot);
    attested.beacon.state_root = state_root;
    let mut branches = branches.into_iter();
    let finality_branch = if finalized.is_some() { branches.next().unwrap() } else { vec![ZERO; 7] };
    let next_sync_committee_branch = if next.is_some() { branches.next().unwrap() } else { vec![ZERO; 6] };
    let mut update = LightClientUpdate {
        attested_header: attested,
        next_sync_committee: next.cloned().unwrap_or_else(|| SyncCommittee::empty(n)),
        next_sync_committee_branch,
        finalized_header: finalized.cloned().unwrap_or_else(|| LightClientHeader::empty(LcFork::Electra)),
        finality_branch,
        sync_aggregate: SyncAggregate { bits: bits.to_vec(), signature: [0; 96] },
        signature_slot,
    };
    let ctx = Context { spec: &w.spec, genesis_validators_root: w.gvr, bls: &BLS };
    update.sync_aggregate.signature = sign(signer, bits, &sync_committee_signing_root(&update, &ctx));
    update
}

fn first(n: usize, of: usize) -> Vec<bool> {
    (0..of).map(|i| i < n).collect()
}

/// A fresh instance at the world's checkpoint (the first slot of its period), holding `current`.
fn instance(w: &World) -> (Instance, LightClientHeader) {
    let checkpoint = fabricated_header(w.base, 9_000);
    let store = CompactStore {
        fork: LcFork::Electra,
        finalized: checkpoint.beacon.clone(),
        current: CommitteeSummary::of(&w.current.committee),
        next: None,
    };
    (Instance::new(store, &checkpoint.beacon, w.spec.clone(), w.gvr, None), checkpoint)
}

fn ns(c: &FakeCommittee, u: &LightClientUpdate) -> NonSigners {
    NonSigners::build(&c.committee, &u.sync_aggregate.bits).unwrap()
}

fn code<T>(r: &Result<T, EvidenceError>) -> Option<u16> {
    r.as_ref().err().map(|e| e.code())
}

/// The honest residue witness of an evidence update (computed from its Miller-loop product).
fn residue_pairing(i: &Instance, u: &CompactUpdate, signers: &NonSigners) -> Pairing {
    let (signer, root) = validate_evidence(&i.store, u, u.signature_slot, &i.spec, &i.genesis_validators_root).unwrap();
    let mut job = signature_job(&i.store, u, signer, &root, signers, Sum::Batch, &i.spec).unwrap();
    job.step2().unwrap();
    let Phase::FinalExp { product } = &job.phase else { unreachable!() };
    let (c, w) = witness(&decode_fp12(product).unwrap());
    Pairing::Residue { c: encode_fp12(&c), w: encode_fp12(&w) }
}

/// The instance after one update in its period that adopts `w.next` and finalizes a header at `base + 8`.
fn advanced(w: &World) -> (Instance, LightClientHeader, LightClientHeader) {
    let (mut i, checkpoint) = instance(w);
    let f1 = fabricated_header(w.base + 8, 40 + w.base + 8);
    let u1 = fabricate(w, Some(&f1), w.base + 16, w.base + 17, &first(30, 32), Some(&w.next.committee), &w.current);
    let out = i.update(&CompactUpdate::of(&u1), LcFork::Electra, &ns(&w.current, &u1), &Pairing::FinalExp, u1.signature_slot);
    let out = out.unwrap();
    assert!(out.advanced && out.adopted, "{out:?}");
    assert_eq!(i.records.get(&f1.beacon.slot), Some(&f1.beacon.hash_tree_root()));
    (i, checkpoint, f1)
}

#[test]
fn real_contradictions_freeze() {
    let w = world();
    let mut lines = Vec::new();

    // (A) A recorded finalized header contradicted at its slot.
    let (mut i, _, f1) = advanced(&w);
    let f1x = fabricated_header(f1.beacon.slot, 5_000);
    assert_ne!(f1x.beacon.hash_tree_root(), f1.beacon.hash_tree_root());
    let conflict = fabricate(&w, Some(&f1x), w.base + 20, w.base + 21, &first(30, 32), None, &w.current);
    let cu = CompactUpdate::of(&conflict);
    let signers = ns(&w.current, &conflict);
    let before = i.clone();
    let as_update = i.update(&cu, LcFork::Electra, &signers, &Pairing::FinalExp, conflict.signature_slot);
    assert_eq!(code(&as_update), Some(Error::NotNewer.code()), "as an update");
    assert_eq!(i, before, "a refused update changes nothing");
    let against_next = i.clone().evidence(&cu, LcFork::Electra, &signers, Sum::Batch, None, &Pairing::FinalExp, conflict.signature_slot);
    assert_eq!(code(&against_next), Some(NO_CONTRADICTION), "the same update against the adopted committee");
    let freeze = i
        .evidence(&cu, LcFork::Electra, &signers, Sum::Batch, Some(f1.beacon.slot), &Pairing::FinalExp, conflict.signature_slot)
        .unwrap();
    assert_eq!(
        freeze.contradiction,
        Contradiction::Finalized {
            slot: f1.beacon.slot,
            recorded: f1.beacon.hash_tree_root(),
            conflicting: f1x.beacon.hash_tree_root()
        }
    );
    assert_eq!(i.frozen, Some(freeze));
    lines.push(format!("(A) record at slot {} contradicted: refused as an update ({}), accepted as evidence: {freeze:?}", f1.beacon.slot, Error::NotNewer.code()));

    // The frozen instance: updates, attestations and further evidence refused with its code; records and store kept.
    let records = i.records.clone();
    let store = i.store.clone();
    let f2 = fabricated_header(w.base + 24, 40 + w.base + 24);
    let u2 = fabricate(&w, Some(&f2), w.base + 30, w.base + 31, &first(32, 32), None, &w.current);
    let r = i.update(&CompactUpdate::of(&u2), LcFork::Electra, &ns(&w.current, &u2), &Pairing::FinalExp, u2.signature_slot);
    assert_eq!(code(&r), Some(FROZEN), "an update of a frozen instance");
    assert_eq!(code(&i.attest(f1.beacon.slot)), Some(FROZEN), "an attestation under a record of a frozen instance");
    let again = i.evidence(&cu, LcFork::Electra, &signers, Sum::Batch, Some(f1.beacon.slot), &Pairing::FinalExp, conflict.signature_slot);
    assert_eq!(code(&again), Some(FROZEN), "evidence against a frozen instance");
    assert_eq!((i.records.clone(), i.store.clone(), i.frozen), (records, store, Some(freeze)), "kept as they were");
    // On a live copy the same update applies and an attestation reads the record.
    let mut live = before.clone();
    live.update(&CompactUpdate::of(&u2), LcFork::Electra, &ns(&w.current, &u2), &Pairing::FinalExp, u2.signature_slot).unwrap();
    assert_eq!(live.attest(f1.beacon.slot), Ok(f1.beacon.hash_tree_root()));
    lines.push(format!("frozen: update {FROZEN}, attest {FROZEN}, evidence again {FROZEN}; records and store unchanged"));

    // The checkpoint's record contradicted (a fresh instance; the update finalizes another block at its slot).
    let (mut j, checkpoint) = instance(&w);
    let cx = fabricated_header(checkpoint.beacon.slot, 6_000);
    let c_update = fabricate(&w, Some(&cx), w.base + 5, w.base + 6, &first(22, 32), None, &w.current);
    let c_cu = CompactUpdate::of(&c_update);
    let c_signers = ns(&w.current, &c_update);
    let as_update = j.clone().update(&c_cu, LcFork::Electra, &c_signers, &Pairing::FinalExp, c_update.signature_slot);
    assert_eq!(code(&as_update), Some(Error::NotNewer.code()));
    let freeze = j
        .evidence(&c_cu, LcFork::Electra, &c_signers, Sum::Batch, Some(checkpoint.beacon.slot), &Pairing::FinalExp, c_update.signature_slot)
        .unwrap();
    assert!(matches!(freeze.contradiction, Contradiction::Finalized { .. }));
    lines.push(format!("(A) the checkpoint's record contradicted, 22 of 32 signers: accepted: {freeze:?}"));

    // (B) The adopted next committee contradicted: attested and finalized in the store's period.
    let (mut k, _, f1) = advanced(&w);
    let f3 = fabricated_header(w.base + 24, 40 + w.base + 24);
    let b_update = fabricate(&w, Some(&f3), w.base + 30, w.base + 31, &first(30, 32), Some(&w.rival.committee), &w.current);
    let b_cu = CompactUpdate::of(&b_update);
    let b_signers = ns(&w.current, &b_update);
    let as_update = k.clone().update(&b_cu, LcFork::Electra, &b_signers, &Pairing::FinalExp, b_update.signature_slot);
    assert_eq!(code(&as_update), Some(Error::NextCommitteeMismatch.code()), "as an update");
    let against_record = k.clone().evidence(&b_cu, LcFork::Electra, &b_signers, Sum::Batch, Some(f1.beacon.slot), &Pairing::FinalExp, b_update.signature_slot);
    assert_eq!(code(&against_record), Some(NO_CONTRADICTION), "against a record at another slot");
    let freeze = k
        .evidence(&b_cu, LcFork::Electra, &b_signers, Sum::Batch, None, &Pairing::FinalExp, b_update.signature_slot)
        .unwrap();
    assert_eq!(
        freeze.contradiction,
        Contradiction::NextCommittee {
            period: w.spec.period_at_slot(w.base),
            adopted: w.next.committee.hash_tree_root(),
            conflicting: w.rival.committee.hash_tree_root()
        }
    );
    lines.push(format!("(B) next committee contradicted: refused as an update ({}), accepted as evidence: {freeze:?}", Error::NextCommitteeMismatch.code()));

    // (B) with the aggregate field alone changed (the same key root): still another committee.
    let mut forged = w.next.committee.clone();
    forged.aggregate_pubkey = w.rival.committee.aggregate_pubkey;
    let (mut m, _, _) = advanced(&w);
    let fu = fabricate(&w, Some(&f3), w.base + 30, w.base + 31, &first(30, 32), Some(&forged), &w.current);
    let freeze = m
        .evidence(&CompactUpdate::of(&fu), LcFork::Electra, &ns(&w.current, &fu), Sum::Batch, None, &Pairing::FinalExp, fu.signature_slot)
        .unwrap();
    assert!(matches!(freeze.contradiction, Contradiction::NextCommittee { .. }));
    lines.push("(B) the same key root with another aggregate field: accepted".into());

    // (A) through the residue-witness pairing, signed by the next committee in the next period.
    let (mut n, _, f1) = advanced(&w);
    let f1y = fabricated_header(f1.beacon.slot, 7_000);
    let late = fabricate(&w, Some(&f1y), w.base + w.len + 2, w.base + w.len + 3, &first(32, 32), None, &w.next);
    let late_cu = CompactUpdate::of(&late);
    let late_signers = ns(&w.next, &late);
    let pairing = residue_pairing(&n, &late_cu, &late_signers);
    let freeze = n
        .evidence(&late_cu, LcFork::Electra, &late_signers, Sum::Batch, Some(f1.beacon.slot), &pairing, late.signature_slot)
        .unwrap();
    assert!(matches!(freeze.contradiction, Contradiction::Finalized { .. }));
    lines.push("(A) signed by the held next committee in the next period, residue-witness pairing: accepted".into());

    println!("{}", lines.join("\n"));
}

#[test]
fn griefing_and_replay() {
    let w = world();
    let (i, checkpoint, f1) = advanced(&w);
    let f1x = fabricated_header(f1.beacon.slot, 5_000);
    let mut rows: Vec<(String, Option<u16>)> = Vec::new();
    let mut submit = |label: &str, u: &LightClientUpdate, signer: &FakeCommittee, record: Option<u64>, pairing: Option<Pairing>, at: Option<u64>, inst: Option<&Instance>| {
        let mut i = inst.cloned().unwrap_or_else(|| i.clone());
        let cu = CompactUpdate::of(u);
        let signers = NonSigners::build(&signer.committee, &u.sync_aggregate.bits).unwrap();
        let pairing = pairing.unwrap_or(Pairing::FinalExp);
        let before = i.clone();
        let r = i.evidence(&cu, LcFork::Electra, &signers, Sum::Batch, record, &pairing, at.unwrap_or(u.signature_slot));
        if r.is_err() {
            assert_eq!(i, before, "{label}: a refused evidence changes nothing");
        }
        rows.push((label.to_string(), code(&r)));
        r
    };
    let s = Some(f1.beacon.slot);
    let all = first(32, 32);

    // A committee the instance does not hold.
    let u = fabricate(&w, Some(&f1x), w.base + 2 * w.len + 2, w.base + 2 * w.len + 3, &all, None, &w.current);
    submit("signed two periods ahead (no committee held)", &u, &w.current, s, None, None, None).unwrap_err();
    let old = fabricated_header(w.base - w.len + 8, 1);
    let mut j = i.clone();
    j.records.insert(old.beacon.slot, old.beacon.hash_tree_root());
    let oldx = fabricated_header(old.beacon.slot, 2);
    let u = fabricate(&w, Some(&oldx), w.base - 10, w.base - 9, &all, None, &w.current);
    submit("signed in the previous period (that committee no longer held)", &u, &w.current, Some(old.beacon.slot), None, None, Some(&j)).unwrap_err();
    let u = fabricate(&w, Some(&f1x), w.base + 20, w.base + 21, &all, None, &w.stranger);
    submit("the right period, another key set (signature by keys the instance does not hold)", &u, &w.current, s, None, None, None).unwrap_err();
    let u = fabricate(&w, Some(&f1x), w.base + w.len + 2, w.base + w.len + 3, &all, None, &w.rival);
    submit("the next period, keys other than the adopted next committee", &u, &w.next, s, None, None, None).unwrap_err();

    // Not finality, not two thirds.
    let u = fabricate(&w, None, w.base + 20, w.base + 21, &all, None, &w.current);
    submit("no finality branch (an attested head only)", &u, &w.current, s, None, None, None).unwrap_err();
    let u = fabricate(&w, Some(&f1x), w.base + 20, w.base + 21, &first(21, 32), None, &w.current);
    submit("21 of 32 signers", &u, &w.current, s, None, None, None).unwrap_err();
    let u = fabricate(&w, Some(&f1x), w.base + 20, w.base + 21, &first(22, 32), None, &w.current);
    submit("22 of 32 signers (two thirds): accepted", &u, &w.current, s, None, None, None).unwrap();

    // Honest data replayed, stale.
    let honest = fabricate(&w, Some(&f1), w.base + 16, w.base + 17, &first(30, 32), Some(&w.next.committee), &w.current);
    submit("the update that wrote the record, replayed against it", &honest, &w.current, s, None, None, None).unwrap_err();
    submit("the same, against the adopted next committee", &honest, &w.current, None, None, None, None).unwrap_err();
    let u = fabricate(&w, Some(&f1x), w.base + 20, w.base + 21, &all, None, &w.current);
    submit("a contradicting update against a record at another slot", &u, &w.current, Some(checkpoint.beacon.slot), None, None, None).unwrap_err();
    submit("a slot with no record", &u, &w.current, Some(f1.beacon.slot + 1), None, None, None).unwrap_err();

    // Malformed evidence.
    let mut u = fabricate(&w, Some(&f1x), w.base + 20, w.base + 21, &all, None, &w.current);
    u.finality_branch[2][0] ^= 1;
    submit("a finality branch node altered", &u, &w.current, s, None, None, None).unwrap_err();
    let u = fabricate(&w, Some(&f1x), w.base + 20, w.base + 21, &all, None, &w.current);
    submit("a signature slot after the slot the caller vouches for", &u, &w.current, s, None, Some(w.base + 20), None).unwrap_err();
    let mut u = fabricate(&w, Some(&f1x), w.base + 20, w.base + 21, &all, None, &w.current);
    u.finalized_header = LightClientHeader::empty(LcFork::Electra);
    submit("the genesis placeholder as the finalized header", &u, &w.current, Some(0), None, None, None).unwrap_err();
    let mut u = fabricate(&w, Some(&f1x), w.base + 20, w.base + 21, &all, None, &w.current);
    u.sync_aggregate.signature = fabricate(&w, Some(&f1), w.base + 20, w.base + 21, &all, None, &w.current).sync_aggregate.signature;
    submit("a signature over another attested header", &u, &w.current, s, None, None, None).unwrap_err();

    // (B) needs both headers in the store's period.
    let early = fabricated_header(w.base - 4, 3);
    let u = fabricate(&w, Some(&early), w.base + 20, w.base + 21, &all, Some(&w.rival.committee), &w.current);
    submit("another next committee, finalized header in the previous period", &u, &w.current, None, None, None, None).unwrap_err();
    let u = fabricate(&w, Some(&f1x), w.base + 20, w.base + 21, &all, Some(&w.next.committee), &w.current);
    submit("the adopted next committee again", &u, &w.current, None, None, None, None).unwrap_err();
    let (fresh, _) = instance(&w);
    let u = fabricate(&w, Some(&f1x), w.base + 20, w.base + 21, &all, Some(&w.rival.committee), &w.current);
    submit("a next committee while the instance knows none", &u, &w.current, None, None, None, Some(&fresh)).unwrap_err();

    // The weak-subjectivity bound (as the programs' validity window).
    let mut bounded = i.clone();
    bounded.ws_period = Some(1);
    let u = fabricate(&w, Some(&f1x), w.base + 20, w.base + 21, &all, None, &w.current);
    submit("past the weak-subjectivity period of the stored finalized header", &u, &w.current, s, None, Some(w.base + 20 + 64 * w.spec.preset.slots_per_epoch), Some(&bounded)).unwrap_err();

    // The residue-witness pairing.
    let u = fabricate(&w, Some(&f1x), w.base + 20, w.base + 21, &all, None, &w.current);
    let cu = CompactUpdate::of(&u);
    let honest_pairing = residue_pairing(&i, &cu, &NonSigners::build(&w.current.committee, &u.sync_aggregate.bits).unwrap());
    let Pairing::Residue { c, w: ww } = honest_pairing.clone() else { unreachable!() };
    let mut c_bad = c;
    c_bad[100] ^= 1;
    submit("residue witness with one bit of c flipped", &u, &w.current, s, Some(Pairing::Residue { c: c_bad, w: ww }), None, None).unwrap_err();
    submit("residue witness, honest: accepted", &u, &w.current, s, Some(honest_pairing), None, None).unwrap();

    let table: Vec<String> = rows.iter().map(|(l, c)| format!("| {l} | {} |", c.map_or("accepted".to_string(), |c| c.to_string()))).collect();
    println!("| evidence | result |\n|---|---|\n{}", table.join("\n"));
    let want: &[(&str, Option<u16>)] = &[
        ("signed two periods ahead (no committee held)", Some(Error::PeriodSkipped.code())),
        ("signed in the previous period (that committee no longer held)", Some(Error::PeriodSkipped.code())),
        ("the right period, another key set (signature by keys the instance does not hold)", Some(Error::SignatureInvalid.code())),
        ("the next period, keys other than the adopted next committee", Some(Error::SignatureInvalid.code())),
        ("no finality branch (an attested head only)", Some(Error::NotFinalized.code())),
        ("21 of 32 signers", Some(Error::BelowSupermajority.code())),
        ("22 of 32 signers (two thirds): accepted", None),
        ("the update that wrote the record, replayed against it", Some(NO_CONTRADICTION)),
        ("the same, against the adopted next committee", Some(NO_CONTRADICTION)),
        ("a contradicting update against a record at another slot", Some(NO_CONTRADICTION)),
        ("a slot with no record", Some(NO_RECORD)),
        ("a finality branch node altered", Some(Error::FinalityBranchInvalid.code())),
        ("a signature slot after the slot the caller vouches for", Some(Error::SlotOrderInvalid.code())),
        ("the genesis placeholder as the finalized header", Some(NO_RECORD)),
        ("a signature over another attested header", Some(Error::SignatureInvalid.code())),
        ("another next committee, finalized header in the previous period", Some(NO_CONTRADICTION)),
        ("the adopted next committee again", Some(NO_CONTRADICTION)),
        ("a next committee while the instance knows none", Some(NO_CONTRADICTION)),
        ("past the weak-subjectivity period of the stored finalized header", Some(Error::OutsideWeakSubjectivityPeriod.code())),
        ("residue witness with one bit of c flipped", Some(Error::SignatureInvalid.code())),
        ("residue witness, honest: accepted", None),
    ];
    for ((label, got), (want_label, want_code)) in rows.iter().zip(want) {
        assert_eq!(label, want_label);
        assert_eq!(got, want_code, "{label}");
    }
    assert_eq!(rows.len(), want.len());

    // The genesis placeholder through the check itself (no record at slot 0 in the model): refused by validation.
    let mut u = fabricate(&w, Some(&f1x), w.base + 20, w.base + 21, &all, None, &w.current);
    u.finalized_header = LightClientHeader::empty(LcFork::Electra);
    let cu = CompactUpdate::of(&u);
    let r = validate_evidence(&i.store, &cu, u.signature_slot, &w.spec, &w.gvr);
    assert_eq!(r.map(|_| ()), Err(Error::FinalizedHeaderInvalid));
}
