//! A differential oracle for the light client: Helios's consensus core (`helios-consensus-core`,
//! a16z/helios tag 0.9.4, rev 3686d99) beside ferrata-core.
//!
//! * Every official `light_client/sync` case (consensus-specs v1.7.0-beta.4, minimal preset)
//!   that Helios can express, replayed step by step through both: Helios's
//!   `verify_generic_update`, then `apply_generic_update` or `force_update`, against ferrata's
//!   spec store. After every step the verdicts and the finalized and optimistic headers must
//!   agree. Helios 0.9.4 cannot express two classes, which are skipped and listed: data in the
//!   Altair layout (it decodes a light-client header as an SSZ union of the Capella, Deneb and
//!   Electra layouts, so the fixed-size Altair header does not decode), and anything with Gloas
//!   (no Gloas fork in its `Forks`, no Gloas layout).
//! * The two mainnet fixtures: periods 1899 to 1903 (bootstrap and five updates), and the
//!   end-to-end fixture (bootstrap and a finality update of period 1906).
//! * Mutated inputs, as bytes both decode: a flipped signature byte, finality-branch byte,
//!   next-committee-branch byte, next-committee key, attested state root, signer bit; no
//!   signers; a signature slot in the future; and a store holding the wrong committee. All refuse.
//! * Participation from 0 to 32 of 32 signers with real signatures, on a fabricated chain whose
//!   keys the test holds (the compared vectors have no update below two thirds).
//! * Four inputs on which Helios departs from the specification, pinned with the spec rule that
//!   decides each (`where_helios_departs_from_the_specification`).
//!
//! Both sides decode the same SSZ bytes: Helios picks its layout by trial decoding, ferrata by the
//! fork the vector names, as the spec does.

use std::{fs, path::Path};

use alloy_primitives::{FixedBytes, B256};
use ferrata_core::{
    bls::{Aggregation, Blst},
    fork::{ChainSpec, Fork, LcFork, Preset, FAR_FUTURE_EPOCH},
    merkle::ZERO,
    policy::FinalizedStore,
    store::{LightClientStore, StoreCore},
    types::{LightClientBootstrap, LightClientHeader, LightClientUpdate, SyncAggregate, SyncCommittee},
    upgrade::{upgrade_bootstrap, upgrade_update},
    validate::Context,
    Root,
};
use ferrata_spectests::{cases, parse_config, read_ssz_snappy, CaseConfig};
use helios_consensus_core::{
    apply_bootstrap, apply_generic_update,
    consensus_spec::{ConsensusSpec as HeliosSpec, MainnetConsensusSpec, MinimalConsensusSpec},
    force_update as helios_force_update,
    types::{
        Bootstrap as HBootstrap, FinalityUpdate as HFinalityUpdate, Fork as HFork, Forks, GenericUpdate,
        LightClientStore as HStore, Update as HUpdate,
    },
    verify_bootstrap, verify_generic_update,
};
use serde_yaml::Value;
use ssz::Decode;
use tree_hash::TreeHash;

static BLST: Blst = Blst::new(Aggregation::Auto);

fn forks(spec: &ChainSpec) -> Forks {
    let f =
        |fork: Fork| HFork { epoch: spec.fork_epoch(fork), fork_version: FixedBytes::from(spec.fork_version(fork)) };
    Forks {
        genesis: f(Fork::Phase0),
        altair: f(Fork::Altair),
        bellatrix: f(Fork::Bellatrix),
        capella: f(Fork::Capella),
        deneb: f(Fork::Deneb),
        electra: f(Fork::Electra),
        fulu: f(Fork::Fulu),
    }
}

fn b256(r: &Root) -> B256 {
    B256::from(*r)
}

fn yaml(path: &Path) -> Value {
    serde_yaml::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn hex32(v: &Value) -> Root {
    hex::decode(v.as_str().unwrap().trim_start_matches("0x")).unwrap().try_into().unwrap()
}

fn digest(v: &Value) -> [u8; 4] {
    hex::decode(v.as_str().unwrap().trim_start_matches("0x")).unwrap().try_into().unwrap()
}

fn version(v: &Value) -> [u8; 4] {
    match v {
        Value::Number(n) => (n.as_u64().unwrap() as u32).to_be_bytes(),
        other => digest(other),
    }
}

/// Helios's decoding of an update: `LightClientUpdate`, else `LightClientFinalityUpdate`.
fn helios_update<S: HeliosSpec>(bytes: &[u8]) -> Option<GenericUpdate<S>> {
    if let Ok(u) = HUpdate::<S>::from_ssz_bytes(bytes) {
        return Some(GenericUpdate::from(&u));
    }
    HFinalityUpdate::<S>::from_ssz_bytes(bytes).ok().map(|u| GenericUpdate::from(&u))
}

/// (slot, beacon root) of the finalized and optimistic headers.
type Heads = ((u64, Root), (u64, Root));

fn ours(store: &LightClientStore) -> Heads {
    let h = |x: &LightClientHeader| (x.beacon.slot, x.beacon.hash_tree_root());
    (h(&store.core.finalized_header), h(&store.optimistic_header))
}

fn theirs<S: HeliosSpec>(store: &HStore<S>) -> Heads {
    let h = |x: &helios_consensus_core::types::LightClientHeader| (x.beacon().slot, x.beacon().tree_hash_root().0);
    (h(&store.finalized_header), h(&store.optimistic_header))
}

#[derive(Default, Debug)]
struct Tally {
    cases: usize,
    steps: usize,
    updates_accepted_by_both: usize,
    /// Of those, updates signed by fewer than two thirds of the committee (valid, but they
    /// cannot finalize: both must leave the finalized header alone).
    below_supermajority: usize,
    force_updates: usize,
    upgrades: usize,
}

/// Replay one sync case through both; `Err` names the first disagreement.
fn compare_case(dir: &Path, tally: &mut Tally) -> Result<(), String> {
    let meta = yaml(&dir.join("meta.yaml"));
    let cfg: CaseConfig = parse_config(&dir.join("config.yaml"))?;
    let spec = &cfg.spec;
    let n = spec.preset.sync_committee_size;
    let gvr = hex32(&meta["genesis_validators_root"]);
    let trusted = hex32(&meta["trusted_block_root"]);
    let lc = |f: Fork| f.lc_fork().ok_or("no light client before Altair".to_string());
    let boot_fork = lc(cfg.fork_for_digest(&gvr, digest(&meta["bootstrap_fork_digest"]))?)?;
    let store_fork = lc(cfg.fork_for_version(version(&meta["store_fork_version"]))?)?;
    let ctx = Context { spec, genesis_validators_root: gvr, bls: &BLST };
    let hforks = forks(spec);

    let raw = read_ssz_snappy(&dir.join("bootstrap.ssz_snappy"))?;
    let boot = upgrade_bootstrap(
        LightClientBootstrap::decode(&raw, boot_fork, n).map_err(|e| e.to_string())?,
        boot_fork,
        store_fork,
        spec,
    )
    .map_err(|e| e.to_string())?;
    let mut store = LightClientStore::initialize(&trusted, &boot, store_fork, spec)
        .map_err(|e| format!("ferrata bootstrap: {e}"))?;
    let hboot = HBootstrap::<MinimalConsensusSpec>::from_ssz_bytes(&raw)
        .map_err(|e| format!("helios cannot decode the bootstrap: {e:?}"))?;
    verify_bootstrap(&hboot, b256(&trusted), &hforks).map_err(|e| format!("helios refuses the bootstrap: {e}"))?;
    let mut hstore = HStore::<MinimalConsensusSpec>::default();
    apply_bootstrap(&mut hstore, &hboot);
    if ours(&store) != theirs(&hstore) {
        return Err("bootstrap: headers differ".into());
    }

    for (i, step) in yaml(&dir.join("steps.yaml")).as_sequence().unwrap().iter().enumerate() {
        let (kind, body) = step.as_mapping().and_then(|m| m.iter().next()).unwrap();
        match kind.as_str().unwrap() {
            "process_update" => {
                let current_slot = body["current_slot"].as_u64().unwrap();
                let name = body["update"].as_str().unwrap();
                let bytes = read_ssz_snappy(&dir.join(format!("{name}.ssz_snappy")))?;
                let fork = lc(cfg.fork_for_digest(&gvr, digest(&body["update_fork_digest"]))?)?;
                let update = upgrade_update(
                    LightClientUpdate::decode(&bytes, fork, n).map_err(|e| e.to_string())?,
                    fork,
                    store.core.fork,
                    spec,
                )
                .map_err(|e| e.to_string())?;
                let ferrata = store.process_update(&update, current_slot, &ctx);
                let hupdate = helios_update::<MinimalConsensusSpec>(&bytes)
                    .ok_or(format!("step {i}: helios cannot decode {name}"))?;
                let helios = verify_generic_update(&hupdate, current_slot, &hstore, b256(&gvr), &hforks);
                if ferrata.is_ok() != helios.is_ok() {
                    return Err(format!("step {i} ({name}): ferrata {ferrata:?}, helios {:?}", helios.err()));
                }
                if helios.is_ok() {
                    apply_generic_update(&mut hstore, &hupdate);
                    tally.updates_accepted_by_both += 1;
                    let signers = update.sync_aggregate.participants() as usize;
                    tally.below_supermajority += usize::from(signers * 3 < n * 2);
                }
            }
            "force_update" => {
                let current_slot = body["current_slot"].as_u64().unwrap();
                store.process_force_update(current_slot, spec).map_err(|e| e.to_string())?;
                helios_force_update(&mut hstore, current_slot);
                tally.force_updates += 1;
            }
            "upgrade_store" => {
                // Helios's store holds headers of any layout and has nothing to upgrade.
                let to = lc(cfg.fork_for_version(version(&body["store_fork_version"]))?)?;
                store.upgrade(to, spec).map_err(|e| e.to_string())?;
                tally.upgrades += 1;
            }
            other => return Err(format!("unknown step {other}")),
        }
        if ours(&store) != theirs(&hstore) {
            return Err(format!(
                "step {i} ({}): ferrata {:?}, helios {:?}",
                kind.as_str().unwrap(),
                ours(&store),
                theirs(&hstore)
            ));
        }
        tally.steps += 1;
    }
    tally.cases += 1;
    Ok(())
}

/// Why Helios cannot take a case, if it cannot: its data must be in a layout Helios decodes
/// (Capella, Deneb or Electra; Helios 0.9.4 decodes a light-client header as an SSZ union of those
/// three, so the fixed-size Altair header does not decode, and it has no Gloas layout), and the
/// chain must not schedule Gloas (Helios's `Forks` ends at Fulu).
fn inexpressible(dir: &Path) -> Option<&'static str> {
    let cfg = parse_config(&dir.join("config.yaml")).unwrap();
    if cfg.spec.fork_epoch(Fork::Gloas) != FAR_FUTURE_EPOCH {
        return Some("Gloas scheduled (no Gloas fork or layout in Helios)");
    }
    let meta = yaml(&dir.join("meta.yaml"));
    let gvr = hex32(&meta["genesis_validators_root"]);
    let mut digests = vec![digest(&meta["bootstrap_fork_digest"])];
    for step in yaml(&dir.join("steps.yaml")).as_sequence().unwrap() {
        if let Some(body) = step.get("process_update") {
            digests.push(digest(&body["update_fork_digest"]));
        }
    }
    let layouts = digests.iter().map(|d| cfg.fork_for_digest(&gvr, *d).unwrap().lc_fork().unwrap());
    layouts.into_iter().any(|l| l == LcFork::Altair).then_some("Altair-layout data (Helios cannot decode it)")
}

#[test]
fn official_sync_vectors_agree_with_helios() {
    let mut tally = Tally::default();
    let mut skipped = std::collections::BTreeMap::<&str, Vec<String>>::new();
    let mut by_fork = std::collections::BTreeMap::new();
    for (fork, dir) in cases("sync").unwrap() {
        let name = format!("{}/{}", fork.name(), dir.file_name().unwrap().to_string_lossy());
        if let Some(reason) = inexpressible(&dir) {
            skipped.entry(reason).or_default().push(name);
            continue;
        }
        compare_case(&dir, &mut tally).unwrap_or_else(|e| panic!("{name}: {e}"));
        *by_fork.entry(fork.name()).or_insert(0) += 1;
    }
    println!("helios oracle: {tally:?}; compared by fork {by_fork:?}");
    for (reason, names) in &skipped {
        println!("skipped, {reason}: {} cases: {names:?}", names.len());
    }
    let skipped_total: usize = skipped.values().map(Vec::len).sum();
    assert_eq!(tally.cases + skipped_total, 52, "every sync case either ran or is listed as skipped");
    let counts = |r: &str| skipped.get(r).map_or(0, Vec::len);
    assert_eq!(
        (
            tally.cases,
            counts("Altair-layout data (Helios cannot decode it)"),
            counts("Gloas scheduled (no Gloas fork or layout in Helios)")
        ),
        (26, 19, 7)
    );
    assert_eq!((tally.steps, tally.updates_accepted_by_both, tally.force_updates, tally.upgrades), (100, 86, 12, 2));
    // Every compared vector update has at least two thirds of the signers; participation below
    // that is compared on fabricated data (`participation_from_zero_to_full_agrees_with_helios`).
    assert_eq!(tally.below_supermajority, 0);
}

/// Mainnet fork schedule (consensus-specs `configs/mainnet.yaml`).
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

const GVR: &str = "4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95";

fn root(s: &str) -> Root {
    hex::decode(s.trim_start_matches("0x")).unwrap().try_into().unwrap()
}

fn fixture(dir: &str, name: &str) -> Vec<u8> {
    fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures").join(dir).join(name)).unwrap()
}

/// `LightClientFinalityUpdate` (Electra layout) as the update it stands for.
fn finality_update(b: &[u8]) -> LightClientUpdate {
    let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as usize;
    let (att, fin) = (u32_at(0), u32_at(4));
    LightClientUpdate {
        attested_header: LightClientHeader::decode(&b[att..fin], LcFork::Electra).unwrap(),
        next_sync_committee: SyncCommittee::empty(512),
        next_sync_committee_branch: vec![ZERO; LcFork::Electra.sync_committee_branch_len()],
        finalized_header: LightClientHeader::decode(&b[fin..], LcFork::Electra).unwrap(),
        finality_branch: b[8..232].chunks_exact(32).map(|c| c.try_into().unwrap()).collect(),
        sync_aggregate: SyncAggregate::decode(&b[232..392], 512).unwrap(),
        signature_slot: u64::from_le_bytes(b[392..400].try_into().unwrap()),
    }
}

/// Both stores, bootstrapped from the same bytes.
struct Pair {
    spec: ChainSpec,
    ours: FinalizedStore,
    spec_store: LightClientStore,
    theirs: HStore<MainnetConsensusSpec>,
}

impl Pair {
    fn new(dir: &str, trusted: &str) -> Pair {
        let spec = mainnet();
        let raw = fixture(dir, "bootstrap.ssz");
        let boot = LightClientBootstrap::decode(&raw, LcFork::Electra, 512).unwrap();
        let ours = FinalizedStore::initialize(&root(trusted), &boot, LcFork::Electra, &spec, &BLST).unwrap();
        let spec_store = LightClientStore::initialize(&root(trusted), &boot, LcFork::Electra, &spec).unwrap();
        let hboot = HBootstrap::<MainnetConsensusSpec>::from_ssz_bytes(&raw).unwrap();
        verify_bootstrap(&hboot, b256(&root(trusted)), &forks(&spec)).unwrap();
        let mut theirs = HStore::default();
        apply_bootstrap(&mut theirs, &hboot);
        Pair { spec, ours, spec_store, theirs }
    }

    /// Submit one update (as SSZ bytes) to all three stores; returns (ferrata policy, ferrata
    /// spec, helios) verdicts.
    fn submit(&mut self, bytes: &[u8], finality_only: bool) -> (bool, bool, bool) {
        let spec = self.spec.clone();
        let ctx = Context { spec: &spec, genesis_validators_root: root(GVR), bls: &BLST };
        let update = if finality_only {
            std::panic::catch_unwind(|| finality_update(bytes)).ok()
        } else {
            LightClientUpdate::decode(bytes, LcFork::Electra, 512).ok()
        };
        let helios = helios_update::<MainnetConsensusSpec>(bytes);
        let Some(update) = update else {
            return (false, false, helios.is_some_and(|h| self.helios(&h, 0)));
        };
        let slot = update.signature_slot + 1;
        let policy = self.ours.process_update(&update, slot, &ctx).is_ok();
        let spec_ok = self.spec_store.process_update(&update, slot, &ctx).is_ok();
        let helios_ok = helios.is_some_and(|h| self.helios(&h, slot));
        (policy, spec_ok, helios_ok)
    }

    fn helios(&mut self, update: &GenericUpdate<MainnetConsensusSpec>, slot: u64) -> bool {
        let ok = verify_generic_update(update, slot, &self.theirs, b256(&root(GVR)), &forks(&self.spec)).is_ok();
        if ok {
            apply_generic_update(&mut self.theirs, update);
        }
        ok
    }

    fn finalized(&self) -> (Root, Root, Root) {
        (
            self.ours.core().finalized_header.beacon.hash_tree_root(),
            self.spec_store.core.finalized_header.beacon.hash_tree_root(),
            self.theirs.finalized_header.beacon().tree_hash_root().0,
        )
    }
}

#[test]
fn mainnet_periods_1899_to_1903_agree_with_helios() {
    let mut pair = Pair::new("mainnet-p1899", "0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac");
    for i in 0..5 {
        let bytes = fixture("mainnet-p1899", &format!("update_{i}.ssz"));
        assert_eq!(pair.submit(&bytes, false), (true, true, true), "update {i}");
        let (a, b, c) = pair.finalized();
        assert!(a == b && b == c, "update {i}: finalized roots differ");
    }
    // The canonical root after update 4 (second provider, fixtures/mainnet-p1899/SOURCE.md).
    assert_eq!(pair.finalized().2, root("0x5d1200f2c031f01aa2c3385c881ca6cac727c23d3080e0ac53169160c8ddd573"));
}

#[test]
fn mainnet_end_to_end_finality_update_agrees_with_helios() {
    let mut pair =
        Pair::new("mainnet-e2e-2026-11-09", "0xdab6b45fd34b9da264b15ad3e85dad586f836f3b11f5922e4c116b53574aefd3");
    let bytes = fixture("mainnet-e2e-2026-11-09", "finality_update.ssz");
    assert_eq!(pair.submit(&bytes, true), (true, true, true));
    let (a, b, c) = pair.finalized();
    assert!(a == b && b == c);
    assert_eq!(c, root("0x816b21ca5caac9461691752d78c06fa70df01537dce36caa77a753bdf46f1f0c"));
}

/// Byte offsets inside a mainnet `LightClientUpdate` (Electra layout, 512 keys).
mod layout {
    pub const NEXT_COMMITTEE: usize = 4;
    pub const NEXT_BRANCH: usize = NEXT_COMMITTEE + 513 * 48;
    pub const FINALITY_BRANCH: usize = NEXT_BRANCH + 6 * 32 + 4;
    pub const BITS: usize = FINALITY_BRANCH + 7 * 32;
    pub const SIGNATURE: usize = BITS + 64;
    pub const SIGNATURE_SLOT: usize = SIGNATURE + 96;
    pub const FIXED: usize = SIGNATURE_SLOT + 8;
}

/// A named change to update bytes.
type Mutation = (&'static str, Box<dyn Fn(&mut Vec<u8>)>);

#[test]
fn mutated_mainnet_updates_are_refused_by_both() {
    let original = fixture("mainnet-p1899", "update_0.ssz");
    // The offsets are right: they hold what the decoder reads there.
    let decoded = LightClientUpdate::decode(&original, LcFork::Electra, 512).unwrap();
    assert_eq!(&original[layout::SIGNATURE..layout::SIGNATURE + 96], &decoded.sync_aggregate.signature);
    assert_eq!(&original[layout::FINALITY_BRANCH..layout::FINALITY_BRANCH + 32], &decoded.finality_branch[0]);
    assert_eq!(
        u64::from_le_bytes(original[layout::SIGNATURE_SLOT..layout::FIXED].try_into().unwrap()),
        decoded.signature_slot
    );
    let attested = u32::from_le_bytes(original[0..4].try_into().unwrap()) as usize;
    assert_eq!(attested, layout::FIXED);
    let mutations: Vec<Mutation> = vec![
        ("signature byte", Box::new(|b: &mut Vec<u8>| b[layout::SIGNATURE + 40] ^= 0x01)),
        ("finality branch byte", Box::new(|b: &mut Vec<u8>| b[layout::FINALITY_BRANCH + 3 * 32] ^= 0x01)),
        ("next-committee branch byte", Box::new(|b: &mut Vec<u8>| b[layout::NEXT_BRANCH + 2 * 32 + 7] ^= 0x01)),
        ("next committee key byte", Box::new(|b: &mut Vec<u8>| b[layout::NEXT_COMMITTEE + 100] ^= 0x01)),
        ("attested state root", Box::new(move |b: &mut Vec<u8>| b[attested + 48] ^= 0x01)),
        ("one signer bit", Box::new(|b: &mut Vec<u8>| b[layout::BITS] ^= 0x01)),
        ("no signers", Box::new(|b: &mut Vec<u8>| b[layout::BITS..layout::SIGNATURE].fill(0))),
        ("signature slot far in the future", Box::new(|b: &mut Vec<u8>| b[layout::SIGNATURE_SLOT + 4] ^= 0x10)),
    ];
    for (what, mutate) in &mutations {
        let mut bytes = original.clone();
        mutate(&mut bytes);
        let mut pair = Pair::new("mainnet-p1899", "0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac");
        let before = pair.finalized();
        assert_eq!(pair.submit(&bytes, false), (false, false, false), "{what}");
        assert_eq!(pair.finalized(), before, "{what}: a refused update changed a store");
    }
    // A store that holds the wrong committee: the period-1900 committee in place of 1899's.
    let mut pair = Pair::new("mainnet-p1899", "0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac");
    let next = decoded.next_sync_committee.clone();
    let core = StoreCore { current_sync_committee: next.clone(), ..pair.ours.core().clone() };
    pair.ours = FinalizedStore::from_core(core.clone(), &BLST).unwrap();
    pair.spec_store.core = core;
    let hnext = helios_update::<MainnetConsensusSpec>(&original).unwrap().next_sync_committee.unwrap();
    pair.theirs.current_sync_committee = hnext;
    assert_eq!(pair.submit(&original, false), (false, false, false), "wrong committee");
}

/// SSZ encoding of Electra-layout light-client data (the inverse of ferrata's decoders), to hand
/// fabricated data to Helios. Checked by re-encoding real vector bytes exactly.
mod encode {
    use ferrata_core::types::{
        BeaconBlockHeader, Execution, ExecutionPayloadHeader, LightClientBootstrap, LightClientHeader,
        LightClientUpdate, SyncAggregate, SyncCommittee,
    };

    fn beacon(b: &BeaconBlockHeader) -> Vec<u8> {
        [&b.slot.to_le_bytes()[..], &b.proposer_index.to_le_bytes(), &b.parent_root, &b.state_root, &b.body_root]
            .concat()
    }

    /// Deneb layout: 584 fixed bytes, then `extra_data`.
    fn payload(p: &ExecutionPayloadHeader) -> Vec<u8> {
        let fixed = [
            &p.parent_hash[..],
            &p.fee_recipient,
            &p.state_root,
            &p.receipts_root,
            &p.logs_bloom,
            &p.prev_randao,
            &p.block_number.to_le_bytes(),
            &p.gas_limit.to_le_bytes(),
            &p.gas_used.to_le_bytes(),
            &p.timestamp.to_le_bytes(),
            &584u32.to_le_bytes(),
            &p.base_fee_per_gas,
            &p.block_hash,
            &p.transactions_root,
            &p.withdrawals_root,
            &p.blob_gas_used.to_le_bytes(),
            &p.excess_blob_gas.to_le_bytes(),
        ]
        .concat();
        assert_eq!(fixed.len(), 584);
        [fixed, p.extra_data.clone()].concat()
    }

    pub fn header(h: &LightClientHeader) -> Vec<u8> {
        let Execution::Payload { header: p, branch } = &h.execution else { panic!("Electra layout only") };
        [beacon(&h.beacon), 244u32.to_le_bytes().to_vec(), branch.concat(), payload(p)].concat()
    }

    pub fn committee(c: &SyncCommittee) -> Vec<u8> {
        [c.pubkeys.concat(), c.aggregate_pubkey.to_vec()].concat()
    }

    fn aggregate(a: &SyncAggregate) -> Vec<u8> {
        let mut bits = vec![0u8; a.bits.len().div_ceil(8)];
        for (i, b) in a.bits.iter().enumerate() {
            bits[i / 8] |= u8::from(*b) << (i % 8);
        }
        [bits, a.signature.to_vec()].concat()
    }

    pub fn bootstrap(b: &LightClientBootstrap) -> Vec<u8> {
        let fixed = 4 + committee(&b.current_sync_committee).len() + 32 * b.current_sync_committee_branch.len();
        [
            (fixed as u32).to_le_bytes().to_vec(),
            committee(&b.current_sync_committee),
            b.current_sync_committee_branch.concat(),
            header(&b.header),
        ]
        .concat()
    }

    pub fn update(u: &LightClientUpdate) -> Vec<u8> {
        let (att, fin) = (header(&u.attested_header), header(&u.finalized_header));
        let middle = [committee(&u.next_sync_committee), u.next_sync_committee_branch.concat()].concat();
        let tail = [u.finality_branch.concat(), aggregate(&u.sync_aggregate), u.signature_slot.to_le_bytes().to_vec()]
            .concat();
        let fixed = 4 + middle.len() + 4 + tail.len();
        [
            (fixed as u32).to_le_bytes().to_vec(),
            middle,
            ((fixed + att.len()) as u32).to_le_bytes().to_vec(),
            tail,
            att,
            fin,
        ]
        .concat()
    }
}

#[test]
fn the_test_encoder_reproduces_real_bytes() {
    let dir = ferrata_spectests::vectors_dir().join("electra/light_client/sync/pyspec_tests/light_client_sync");
    let raw = read_ssz_snappy(&dir.join("bootstrap.ssz_snappy")).unwrap();
    assert_eq!(encode::bootstrap(&LightClientBootstrap::decode(&raw, LcFork::Electra, 32).unwrap()), raw);
    let mainnet = fixture("mainnet-p1899", "update_0.ssz");
    assert_eq!(encode::update(&LightClientUpdate::decode(&mainnet, LcFork::Electra, 512).unwrap()), mainnet);
}

#[test]
fn participation_from_zero_to_full_agrees_with_helios() {
    use ferrata_spectests::testkit::{fabricated_update, fake_committee, Fabrication};
    // A minimal-preset chain at Electra from genesis, with a committee whose keys the test holds.
    let fake = fake_committee(32, 0x5a);
    let chain = fabricated_chain(&fake.committee);
    let (spec, gvr, bootstrap, trusted) = (&chain.spec, chain.gvr, &chain.bootstrap, chain.trusted);
    let ctx = chain.ctx();
    let boot_bytes = encode::bootstrap(bootstrap);
    let hforks = forks(spec);
    for signers in [0usize, 1, 10, 21, 22, 31, 32] {
        let mut spec_store = LightClientStore::initialize(&trusted, bootstrap, LcFork::Electra, spec).unwrap();
        let mut policy = FinalizedStore::initialize(&trusted, bootstrap, LcFork::Electra, spec, &BLST).unwrap();
        let hboot = HBootstrap::<MinimalConsensusSpec>::from_ssz_bytes(&boot_bytes).unwrap();
        verify_bootstrap(&hboot, b256(&trusted), &hforks).unwrap();
        let mut hstore = HStore::<MinimalConsensusSpec>::default();
        apply_bootstrap(&mut hstore, &hboot);
        // No signer cannot be aggregated: sign with one key, then clear the bits.
        let mut update = fabricated_update(
            &Fabrication {
                finalized_slot: Some(72),
                attested_slot: 80,
                signature_slot: 81,
                signers: signers.max(1),
                next_committee: None,
            },
            &fake,
            &ctx,
        );
        if signers == 0 {
            update.sync_aggregate.bits = vec![false; 32];
        }
        let bytes = encode::update(&update);
        assert_eq!(LightClientUpdate::decode(&bytes, LcFork::Electra, 32).unwrap(), update);
        let ferrata = spec_store.process_update(&update, 82, &ctx);
        let hupdate = helios_update::<MinimalConsensusSpec>(&bytes).unwrap();
        let helios = verify_generic_update(&hupdate, 82, &hstore, b256(&gvr), &hforks);
        assert_eq!(
            ferrata.is_ok(),
            helios.is_ok(),
            "{signers} signers: ferrata {ferrata:?}, helios {:?}",
            helios.err()
        );
        assert_eq!(ferrata.is_ok(), signers > 0, "{signers} signers");
        if helios.is_ok() {
            apply_generic_update(&mut hstore, &hupdate);
        }
        assert_eq!(ours(&spec_store), theirs(&hstore), "{signers} signers");
        // Both finalize exactly from 22 of 32 (two thirds); the policy store also refuses below.
        let finalized = spec_store.core.finalized_header.beacon.slot == 72;
        assert_eq!(finalized, signers >= 22, "{signers} signers");
        assert_eq!(policy.process_update(&update, 82, &ctx).is_ok(), signers >= 22, "{signers} signers (policy)");
    }
}

/// A minimal-preset chain at Electra from genesis whose bootstrap commits `committee`.
struct Fabricated {
    spec: ChainSpec,
    gvr: Root,
    bootstrap: LightClientBootstrap,
    trusted: Root,
}

fn fabricated_chain(committee: &SyncCommittee) -> Fabricated {
    use ferrata_core::merkle::CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA;
    use ferrata_spectests::testkit::{fabricated_header, sparse_tree};
    let mut versions = [[0u8; 4]; 9];
    for (i, v) in versions.iter_mut().enumerate() {
        *v = [i as u8, 0, 0, 9];
    }
    let spec = ChainSpec {
        preset: Preset::MINIMAL,
        versions,
        epochs: [0, 0, 0, 0, 0, 0, FAR_FUTURE_EPOCH, FAR_FUTURE_EPOCH, FAR_FUTURE_EPOCH],
    };
    let mut header = fabricated_header(64, 3);
    let (state_root, branches) =
        sparse_tree(&[(CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA, committee.hash_tree_root())], 11);
    header.beacon.state_root = state_root;
    let bootstrap = LightClientBootstrap {
        header,
        current_sync_committee: committee.clone(),
        current_sync_committee_branch: branches[0].clone(),
    };
    let trusted = bootstrap.header.beacon.hash_tree_root();
    Fabricated { spec, gvr: [0x42; 32], bootstrap, trusted }
}

impl Fabricated {
    fn ctx(&self) -> Context<'_> {
        Context { spec: &self.spec, genesis_validators_root: self.gvr, bls: &BLST }
    }
    fn stores(&self) -> (LightClientStore, HStore<MinimalConsensusSpec>) {
        let ours = LightClientStore::initialize(&self.trusted, &self.bootstrap, LcFork::Electra, &self.spec).unwrap();
        let hboot = HBootstrap::<MinimalConsensusSpec>::from_ssz_bytes(&encode::bootstrap(&self.bootstrap)).unwrap();
        verify_bootstrap(&hboot, b256(&self.trusted), &forks(&self.spec)).unwrap();
        let mut theirs = HStore::default();
        apply_bootstrap(&mut theirs, &hboot);
        (ours, theirs)
    }
    /// Submit to both; returns (ferrata spec store, helios) verdicts.
    fn submit(
        &self,
        ours: &mut LightClientStore,
        theirs: &mut HStore<MinimalConsensusSpec>,
        update: &LightClientUpdate,
        slot: u64,
    ) -> (Result<(), ferrata_core::Error>, bool) {
        let ferrata = ours.process_update(update, slot, &self.ctx());
        let h = helios_update::<MinimalConsensusSpec>(&encode::update(update)).unwrap();
        let helios = verify_generic_update(&h, slot, theirs, b256(&self.gvr), &forks(&self.spec)).is_ok();
        if helios {
            apply_generic_update(theirs, &h);
        }
        (ferrata, helios)
    }
}

/// Four inputs on which Helios 0.9.4 departs from the light-client specification. None occurs on
/// a genuine chain with an honest committee, and none was among the inputs compared above; each
/// is pinned here so the README can state it, with the spec's rule that decides it.
#[test]
fn where_helios_departs_from_the_specification() {
    use ferrata_spectests::testkit::{fabricated_update, fake_committee, sign, Fabrication};
    let fake = fake_committee(32, 0x5a);
    let chain = fabricated_chain(&fake.committee);
    let ctx = chain.ctx();
    let finality = |finalized: u64, attested: u64, next: Option<&SyncCommittee>| {
        fabricated_update(
            &Fabrication {
                finalized_slot: Some(finalized),
                attested_slot: attested,
                signature_slot: attested + 1,
                signers: 32,
                next_committee: next,
            },
            &fake,
            &ctx,
        )
    };

    // 1. A next committee that contradicts the one the store holds (validate_light_client_update:
    //    `if update_attested_period == store_period and is_next_sync_committee_known(store):
    //    assert update.next_sync_committee == store.next_sync_committee`). Ferrata refuses; Helios
    //    has no such check and accepts.
    let (b, c) = (fake_committee(32, 0x6b).committee, fake_committee(32, 0x7c).committee);
    let (mut ours, mut theirs) = chain.stores();
    assert_eq!(chain.submit(&mut ours, &mut theirs, &finality(72, 80, Some(&b)), 82), (Ok(()), true));
    let (ferrata, helios) = chain.submit(&mut ours, &mut theirs, &finality(88, 96, Some(&c)), 98);
    assert_eq!(
        (ferrata, helios),
        (Err(ferrata_core::Error::NextCommitteeMismatch), true),
        "next committee contradiction"
    );

    // 2. A signer key that is the identity point. FastAggregateVerify requires KeyValidate on every
    //    key, and KeyValidate refuses the identity. Ferrata's spec store refuses (its policy store
    //    already refuses such a committee at bootstrap); Helios aggregates the keys and checks only
    //    the sum, and accepts.
    let mut with_identity = fake.committee.clone();
    with_identity.pubkeys[5] = {
        let mut id = [0u8; 48];
        id[0] = 0xc0;
        id
    };
    let chain2 = fabricated_chain(&with_identity);
    let ctx2 = chain2.ctx();
    let mut u = fabricated_update(
        &Fabrication {
            finalized_slot: Some(72),
            attested_slot: 80,
            signature_slot: 81,
            signers: 32,
            next_committee: None,
        },
        &fake,
        &ctx2,
    );
    let mut others = vec![true; 32];
    others[5] = false;
    u.sync_aggregate.signature = sign(&fake, &others, &ferrata_core::validate::sync_committee_signing_root(&u, &ctx2));
    let (mut ours2, mut theirs2) = chain2.stores();
    assert_eq!(
        chain2.submit(&mut ours2, &mut theirs2, &u, 82),
        (Err(ferrata_core::Error::PubkeyInvalid), true),
        "identity key"
    );
    assert_eq!(
        FinalizedStore::initialize(&chain2.trusted, &chain2.bootstrap, LcFork::Electra, &chain2.spec, &BLST)
            .unwrap_err(),
        ferrata_core::Error::CommitteeInvalid
    );

    // 3. An empty finalized header with a finality branch that does not prove the zero root (the
    //    genesis rule: `if update_finalized_slot == GENESIS_SLOT: assert update.finalized_header ==
    //    LightClientHeader(); finalized_root = Bytes32()`, then the branch must prove it).
    //    Anyone can build this from a genuine signed update, since the signature covers only the
    //    attested header. Ferrata refuses; Helios skips the finality check and accepts.
    let mut garbage = fabricated_update(
        &Fabrication { finalized_slot: None, attested_slot: 80, signature_slot: 81, signers: 32, next_committee: None },
        &fake,
        &ctx,
    );
    garbage.finality_branch = vec![[0x77; 32]; 7];
    let (mut ours3, mut theirs3) = chain.stores();
    assert_eq!(
        chain.submit(&mut ours3, &mut theirs3, &garbage, 82),
        (Err(ferrata_core::Error::FinalityBranchInvalid), true),
        "garbage finality branch"
    );

    // 4. A forced update whose best valid update carries no finality (a valid update): the spec
    //    then treats the attested header as finalized. Ferrata does; Helios's `force_update`
    //    unwraps the absent finalized header and panics.
    let optimistic = fabricated_update(
        &Fabrication { finalized_slot: None, attested_slot: 80, signature_slot: 81, signers: 32, next_committee: None },
        &fake,
        &ctx,
    );
    let (mut ours4, mut theirs4) = chain.stores();
    assert_eq!(chain.submit(&mut ours4, &mut theirs4, &optimistic, 82), (Ok(()), true));
    let timed_out = 64 + chain.spec.preset.update_timeout() + 1;
    ours4.process_force_update(timed_out, &chain.spec).unwrap();
    assert_eq!(ours4.core.finalized_header.beacon.slot, 80, "the spec forces the attested header");
    let forced =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| helios_force_update(&mut theirs4, timed_out)));
    assert!(forced.is_err(), "helios force_update panics on a best update without finality");
}
