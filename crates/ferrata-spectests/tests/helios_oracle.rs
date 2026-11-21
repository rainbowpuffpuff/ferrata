//! A differential oracle for the light client: Helios's consensus core (`helios-consensus-core`,
//! a16z/helios tag 0.12.0, rev 2af9b41) beside ferrata-core.
//!
//! * Every official `light_client/sync` case (consensus-specs v1.7.0-beta.4, minimal preset),
//!   replayed step by step through both: Helios's `verify_generic_update`, then
//!   `apply_generic_update` or `force_update`, against ferrata's spec store. After every step the
//!   verdicts and the finalized and optimistic headers (slot, beacon root, execution root) must
//!   agree. Cases whose data Helios cannot decode are listed instead: those holding data in the
//!   Altair layout (Helios decodes light-client data as an SSZ union of layouts in which the
//!   header is a variable-size field, so the fixed-size Altair header does not decode).
//! * Three real chains: mainnet periods 1899 to 1903 (bootstrap and five updates), the mainnet
//!   end-to-end fixture (bootstrap and a finality update of period 1906), and Sepolia through its
//!   Gloas fork (the bootstrap of period 1378, the update of every period from 1378 to 1409, and a
//!   Gloas finality update).
//! * Mutated inputs, as bytes both decode: on mainnet Electra data, a flipped signature byte,
//!   finality-branch byte, next-committee-branch byte, next-committee key, attested state root,
//!   signer bit; no signers; a signature slot in the future; a store holding the wrong committee.
//!   On Sepolia Gloas data, the same and the bound execution block hashes and their branch. All
//!   refuse.
//! * Participation from 0 to 32 of 32 signers with real signatures, on a fabricated chain whose
//!   keys the test holds (the compared vectors have no update below two thirds).
//! * The inputs on which Helios departs from the specification, pinned with the spec rule that
//!   decides each, and the two departures of 0.9.4 that 0.12.0 no longer has
//!   (`where_helios_departs_from_the_specification`).
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
    validate::{get_lc_execution_root, Context},
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
        gloas: f(Fork::Gloas),
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

/// (slot, beacon root, execution root) of the finalized and the optimistic header.
type Heads = ((u64, Root, Root), (u64, Root, Root));

fn ours(store: &LightClientStore, spec: &ChainSpec) -> Heads {
    let h = |x: &LightClientHeader| {
        (x.beacon.slot, x.beacon.hash_tree_root(), get_lc_execution_root(x, store.core.fork, spec))
    };
    (h(&store.core.finalized_header), h(&store.optimistic_header))
}

fn theirs<S: HeliosSpec>(store: &HStore<S>) -> Heads {
    let h = |x: &helios_consensus_core::types::LightClientHeader| {
        (x.beacon().slot, x.beacon().tree_hash_root().0, x.execution_root().0)
    };
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

impl Tally {
    fn add(&mut self, t: &Tally) {
        self.cases += t.cases;
        self.steps += t.steps;
        self.updates_accepted_by_both += t.updates_accepted_by_both;
        self.below_supermajority += t.below_supermajority;
        self.force_updates += t.force_updates;
        self.upgrades += t.upgrades;
    }
}

/// Why a case did not run to the end.
#[derive(Debug)]
enum Fail {
    /// Helios cannot decode some of the case's data.
    Undecodable(String),
    /// A disagreement (or a vector ferrata cannot process).
    Differ(String),
}

impl From<String> for Fail {
    fn from(e: String) -> Self {
        Fail::Differ(e)
    }
}

/// Replay one sync case through both: what was compared, or why the comparison stopped.
fn compare_case(dir: &Path) -> Result<Tally, Fail> {
    let mut tally = Tally::default();
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
        .map_err(|e| Fail::Undecodable(format!("the bootstrap: {e:?}")))?;
    verify_bootstrap(&hboot, b256(&trusted), &hforks).map_err(|e| format!("helios refuses the bootstrap: {e}"))?;
    let mut hstore = HStore::<MinimalConsensusSpec>::default();
    apply_bootstrap(&mut hstore, &hboot);
    if ours(&store, spec) != theirs(&hstore) {
        return Err(Fail::Differ("bootstrap: headers differ".into()));
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
                    .ok_or_else(|| Fail::Undecodable(format!("step {i}: {name}")))?;
                let helios = verify_generic_update(&hupdate, current_slot, &hstore, b256(&gvr), &hforks);
                if ferrata.is_ok() != helios.is_ok() {
                    return Err(Fail::Differ(format!(
                        "step {i} ({name}): ferrata {ferrata:?}, helios {:?}",
                        helios.err()
                    )));
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
            other => return Err(Fail::Differ(format!("unknown step {other}"))),
        }
        if ours(&store, spec) != theirs(&hstore) {
            return Err(Fail::Differ(format!(
                "step {i} ({}): ferrata {:?}, helios {:?}",
                kind.as_str().unwrap(),
                ours(&store, spec),
                theirs(&hstore)
            )));
        }
        tally.steps += 1;
    }
    tally.cases = 1;
    Ok(tally)
}

/// Whether a case holds data in the Altair layout (by the fork digests its bootstrap and
/// updates are served under).
fn has_altair_layout_data(dir: &Path) -> bool {
    let cfg = parse_config(&dir.join("config.yaml")).unwrap();
    let meta = yaml(&dir.join("meta.yaml"));
    let gvr = hex32(&meta["genesis_validators_root"]);
    let mut digests = vec![digest(&meta["bootstrap_fork_digest"])];
    for step in yaml(&dir.join("steps.yaml")).as_sequence().unwrap() {
        if let Some(body) = step.get("process_update") {
            digests.push(digest(&body["update_fork_digest"]));
        }
    }
    digests.iter().any(|d| cfg.fork_for_digest(&gvr, *d).unwrap().lc_fork() == Some(LcFork::Altair))
}

#[test]
fn official_sync_vectors_agree_with_helios() {
    let mut tally = Tally::default();
    let mut undecodable = Vec::new();
    let mut by_fork = std::collections::BTreeMap::new();
    let mut gloas_scheduled = 0;
    for (fork, dir) in cases("sync").unwrap() {
        let name = format!("{}/{}", fork.name(), dir.file_name().unwrap().to_string_lossy());
        let result = compare_case(&dir);
        // Helios decodes every case except exactly those with Altair-layout data.
        let decoded = !matches!(result, Err(Fail::Undecodable(_)));
        assert_eq!(decoded, !has_altair_layout_data(&dir), "{name}: {result:?}");
        match result {
            Ok(t) => {
                tally.add(&t);
                *by_fork.entry(fork.name()).or_insert(0) += 1;
                let cfg = parse_config(&dir.join("config.yaml")).unwrap();
                gloas_scheduled += usize::from(cfg.spec.fork_epoch(Fork::Gloas) != FAR_FUTURE_EPOCH);
            }
            Err(Fail::Undecodable(what)) => undecodable.push(format!("{name} ({what})")),
            Err(Fail::Differ(e)) => panic!("{name}: {e}"),
        }
    }
    println!("helios oracle: {tally:?}; compared by fork {by_fork:?}");
    println!("not decoded by helios ({}): {undecodable:?}", undecodable.len());
    assert_eq!((tally.cases, undecodable.len()), (33, 19), "every sync case either ran or is listed");
    let compared: Vec<_> = by_fork.into_iter().collect();
    assert_eq!(compared, [("capella", 10), ("deneb", 8), ("electra", 5), ("fulu", 6), ("gloas", 4)]);
    // Among them every case whose chain schedules Gloas (three fork transitions into Gloas and
    // the four Gloas cases); Helios 0.9.4 had no Gloas fork and could take none of them.
    assert_eq!(gloas_scheduled, 7);
    assert_eq!((tally.steps, tally.updates_accepted_by_both, tally.force_updates, tally.upgrades), (129, 111, 15, 3));
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

/// Sepolia's fork schedule (eth-clients/sepolia `metadata/config.yaml`, as in
/// `tests/sepolia_gloas.rs`): Gloas from epoch 353,024.
fn sepolia() -> ChainSpec {
    let v = |b: u8| [0x90, 0x00, 0x00, b];
    ChainSpec {
        preset: Preset::MAINNET,
        versions: [v(0x69), v(0x70), v(0x71), v(0x72), v(0x73), v(0x74), v(0x75), v(0x76), [0; 4]],
        epochs: [0, 50, 100, 56_832, 132_608, 222_464, 272_640, 353_024, FAR_FUTURE_EPOCH],
    }
}

const SEPOLIA: &str = "sepolia-gloas-2026-11-10";
const SEPOLIA_GVR: &str = "d8ea171f3c94aea21ebc42a1ed61052acf3f9209c00e4efbaaddac09ed9b8078";

fn root(s: &str) -> Root {
    hex::decode(s.trim_start_matches("0x")).unwrap().try_into().unwrap()
}

fn fixture(dir: &str, name: &str) -> Vec<u8> {
    fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures").join(dir).join(name)).unwrap()
}

/// `LightClientFinalityUpdate` as the update it stands for: in the Electra layout the two
/// headers sit behind offsets; in the Gloas layout every field is fixed-size.
fn finality_update(b: &[u8], layout: LcFork) -> LightClientUpdate {
    let (attested, finalized, rest) = match layout {
        LcFork::Electra => {
            let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as usize;
            let (att, fin) = (u32_at(0), u32_at(4));
            (&b[att..fin], &b[fin..], &b[8..att])
        }
        LcFork::Gloas => {
            const H: usize = 112 + 32 + 11 * 32;
            (&b[..H], &b[H..2 * H], &b[2 * H..])
        }
        other => panic!("no finality update layout for {other:?} here"),
    };
    // The finality branch, the sync aggregate (160 bytes), the signature slot.
    let branch = 32 * layout.finality_branch_len();
    assert_eq!(rest.len(), branch + 168);
    LightClientUpdate {
        attested_header: LightClientHeader::decode(attested, layout).unwrap(),
        next_sync_committee: SyncCommittee::empty(512),
        next_sync_committee_branch: vec![ZERO; layout.sync_committee_branch_len()],
        finalized_header: LightClientHeader::decode(finalized, layout).unwrap(),
        finality_branch: rest[..branch].chunks_exact(32).map(|c| c.try_into().unwrap()).collect(),
        sync_aggregate: SyncAggregate::decode(&rest[branch..branch + 160], 512).unwrap(),
        signature_slot: u64::from_le_bytes(rest[branch + 160..].try_into().unwrap()),
    }
}

/// What some bytes are to ferrata, which decodes by the fork the data is served under (Helios
/// finds out by trial decoding).
#[derive(Clone, Copy, Debug)]
enum Data {
    Update(LcFork),
    FinalityUpdate(LcFork),
}

/// Ferrata's policy store and spec store and Helios's store, bootstrapped from the same bytes.
#[derive(Clone)]
struct Pair {
    spec: ChainSpec,
    gvr: Root,
    ours: FinalizedStore,
    spec_store: LightClientStore,
    theirs: HStore<MainnetConsensusSpec>,
}

impl Pair {
    fn new(spec: ChainSpec, gvr: &str, dir: &str, trusted: &str) -> Pair {
        let raw = fixture(dir, "bootstrap.ssz");
        let boot = LightClientBootstrap::decode(&raw, LcFork::Electra, 512).unwrap();
        let ours = FinalizedStore::initialize(&root(trusted), &boot, LcFork::Electra, &spec, &BLST).unwrap();
        let spec_store = LightClientStore::initialize(&root(trusted), &boot, LcFork::Electra, &spec).unwrap();
        let hboot = HBootstrap::<MainnetConsensusSpec>::from_ssz_bytes(&raw).unwrap();
        verify_bootstrap(&hboot, b256(&root(trusted)), &forks(&spec)).unwrap();
        let mut theirs = HStore::default();
        apply_bootstrap(&mut theirs, &hboot);
        Pair { spec, gvr: root(gvr), ours, spec_store, theirs }
    }

    fn mainnet(dir: &str, trusted: &str) -> Pair {
        Pair::new(mainnet(), GVR, dir, trusted)
    }

    /// Sepolia from the bootstrap of period 1378, the last before Gloas.
    fn sepolia() -> Pair {
        Pair::new(sepolia(), SEPOLIA_GVR, SEPOLIA, "0x5f71d7f650f9252b48749b5f062c0c234e01afd69f364ab600e2b6051494b20d")
    }

    /// Submit one update (as SSZ bytes) to all three stores; returns (ferrata policy, ferrata
    /// spec, helios) verdicts.
    fn submit(&mut self, bytes: &[u8], data: Data) -> (bool, bool, bool) {
        let spec = self.spec.clone();
        let ctx = Context { spec: &spec, genesis_validators_root: self.gvr, bls: &BLST };
        let update = match data {
            Data::FinalityUpdate(layout) => std::panic::catch_unwind(|| finality_update(bytes, layout)).ok(),
            Data::Update(layout) => LightClientUpdate::decode(bytes, layout, 512).ok(),
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
        let ok = verify_generic_update(update, slot, &self.theirs, b256(&self.gvr), &forks(&self.spec)).is_ok();
        if ok {
            apply_generic_update(&mut self.theirs, update);
        }
        ok
    }

    /// Ferrata's stores move to a later layout at a fork; Helios's holds headers of any layout.
    fn upgrade(&mut self, to: LcFork) {
        self.ours.upgrade(to, &self.spec).unwrap();
        self.spec_store.upgrade(to, &self.spec).unwrap();
    }

    fn finalized(&self) -> (Root, Root, Root) {
        (
            self.ours.core().finalized_header.beacon.hash_tree_root(),
            self.spec_store.core.finalized_header.beacon.hash_tree_root(),
            self.theirs.finalized_header.beacon().tree_hash_root().0,
        )
    }

    /// The three hold the same finalized header, and ferrata's spec store and Helios's store the
    /// same optimistic header.
    fn agree(&self) -> bool {
        let (a, b, c) = self.finalized();
        a == b && b == c && ours(&self.spec_store, &self.spec) == theirs(&self.theirs)
    }
}

#[test]
fn mainnet_periods_1899_to_1903_agree_with_helios() {
    let mut pair = Pair::mainnet("mainnet-p1899", "0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac");
    for i in 0..5 {
        let bytes = fixture("mainnet-p1899", &format!("update_{i}.ssz"));
        assert_eq!(pair.submit(&bytes, Data::Update(LcFork::Electra)), (true, true, true), "update {i}");
        assert!(pair.agree(), "update {i}: headers differ");
    }
    // The canonical root after update 4 (second provider, fixtures/mainnet-p1899/SOURCE.md).
    assert_eq!(pair.finalized().2, root("0x5d1200f2c031f01aa2c3385c881ca6cac727c23d3080e0ac53169160c8ddd573"));
}

#[test]
fn mainnet_end_to_end_finality_update_agrees_with_helios() {
    let mut pair =
        Pair::mainnet("mainnet-e2e-2026-11-09", "0xdab6b45fd34b9da264b15ad3e85dad586f836f3b11f5922e4c116b53574aefd3");
    let bytes = fixture("mainnet-e2e-2026-11-09", "finality_update.ssz");
    assert_eq!(pair.submit(&bytes, Data::FinalityUpdate(LcFork::Electra)), (true, true, true));
    assert!(pair.agree());
    assert_eq!(pair.finalized().2, root("0x816b21ca5caac9461691752d78c06fa70df01537dce36caa77a753bdf46f1f0c"));
}

/// The update of Sepolia period `p`: Electra layout before the Gloas fork, Gloas from 1379.
fn sepolia_update(p: u64) -> (Vec<u8>, Data) {
    let layout = if p < 1379 { LcFork::Electra } else { LcFork::Gloas };
    (fixture(SEPOLIA, &format!("update_{p}.ssz")), Data::Update(layout))
}

/// The three stores after Sepolia's updates up to period `last`, upgraded at the fork.
fn sepolia_through(last: u64) -> Pair {
    let mut pair = Pair::sepolia();
    for p in 1378..=last {
        if p == 1379 {
            pair.upgrade(LcFork::Gloas);
        }
        let (bytes, data) = sepolia_update(p);
        assert_eq!(pair.submit(&bytes, data), (true, true, true), "period {p}");
        assert!(pair.agree(), "period {p}: headers differ");
    }
    pair
}

#[test]
fn sepolia_through_the_gloas_fork_agrees_with_helios() {
    // Every period's update from 1378 (Fulu) to 1409, 31 of them under Gloas; then the latest
    // Gloas finality update.
    let mut pair = sepolia_through(1409);
    let bytes = fixture(SEPOLIA, "finality_update.ssz");
    assert_eq!(pair.submit(&bytes, Data::FinalityUpdate(LcFork::Gloas)), (true, true, true));
    assert!(pair.agree());
    // The canonical root at slot 11547104 (second provider, the fixture's SOURCE.md), and the
    // execution block hash both bind: the bid's parent block.
    assert_eq!(pair.finalized().2, root("0xface3c3725d1760873c4e115e74446e7e5c0164a71da7228bbb66b92bc8ed6ef"));
    let helios_hash = pair.theirs.finalized_header.execution_root();
    let ferrata_core::types::Execution::BlockHash { block_hash, .. } = &pair.ours.core().finalized_header.execution
    else {
        panic!("not the Gloas layout")
    };
    assert_eq!(helios_hash, B256::from(*block_hash));
    assert_eq!(helios_hash, B256::from(root("0xcd9fa3661248bb2411636d3498a1dda97f0ee50d02b8ac894969e391f2978435")));
}

/// Byte offsets inside Sepolia's Gloas-layout data (512 keys; every field fixed-size).
mod gloas_layout {
    pub const HEADER: usize = 112 + 32 + 11 * 32;
    /// Within a header: the beacon state root, the execution block hash, the execution branch.
    pub const STATE_ROOT: usize = 48;
    pub const BLOCK_HASH: usize = 112;
    pub const EXECUTION_BRANCH: usize = 144;
    /// `LightClientFinalityUpdate`.
    pub const FU_FINALIZED: usize = HEADER;
    pub const FU_FINALITY_BRANCH: usize = 2 * HEADER;
    pub const FU_BITS: usize = FU_FINALITY_BRANCH + 9 * 32;
    pub const FU_SIGNATURE: usize = FU_BITS + 64;
    pub const FU_SIGNATURE_SLOT: usize = FU_SIGNATURE + 96;
    /// `LightClientUpdate`.
    pub const NEXT_COMMITTEE: usize = HEADER;
    pub const NEXT_BRANCH: usize = NEXT_COMMITTEE + 513 * 48;
}

#[test]
fn mutated_sepolia_gloas_data_is_refused_by_both() {
    use gloas_layout::*;
    // The offsets are right: they hold what the decoder reads there.
    let fu_bytes = fixture(SEPOLIA, "finality_update.ssz");
    let fu = finality_update(&fu_bytes, LcFork::Gloas);
    assert_eq!(&fu_bytes[FU_SIGNATURE..FU_SIGNATURE + 96], &fu.sync_aggregate.signature);
    assert_eq!(&fu_bytes[FU_FINALITY_BRANCH..FU_FINALITY_BRANCH + 32], &fu.finality_branch[0]);
    assert_eq!(&fu_bytes[STATE_ROOT..STATE_ROOT + 32], &fu.attested_header.beacon.state_root);
    let ferrata_core::types::Execution::BlockHash { block_hash, branch } = &fu.finalized_header.execution else {
        panic!("not the Gloas layout")
    };
    assert_eq!(&fu_bytes[FU_FINALIZED + BLOCK_HASH..FU_FINALIZED + BLOCK_HASH + 32], block_hash);
    assert_eq!(&fu_bytes[FU_FINALIZED + EXECUTION_BRANCH + 320..FU_FINALIZED + EXECUTION_BRANCH + 352], &branch[10]);
    let at_1409 = sepolia_through(1409);
    let mutations: Vec<Mutation> = vec![
        ("signature byte", Box::new(|b: &mut Vec<u8>| b[FU_SIGNATURE + 40] ^= 0x01)),
        ("finality branch byte", Box::new(|b: &mut Vec<u8>| b[FU_FINALITY_BRANCH + 3 * 32] ^= 0x01)),
        ("attested state root", Box::new(|b: &mut Vec<u8>| b[STATE_ROOT] ^= 0x01)),
        ("attested execution block hash", Box::new(|b: &mut Vec<u8>| b[BLOCK_HASH + 5] ^= 0x01)),
        ("finalized execution block hash", Box::new(|b: &mut Vec<u8>| b[FU_FINALIZED + BLOCK_HASH] ^= 0x01)),
        ("finalized execution branch", Box::new(|b: &mut Vec<u8>| b[FU_FINALIZED + EXECUTION_BRANCH + 330] ^= 0x01)),
        ("one signer bit", Box::new(|b: &mut Vec<u8>| b[FU_BITS] ^= 0x01)),
        ("no signers", Box::new(|b: &mut Vec<u8>| b[FU_BITS..FU_SIGNATURE].fill(0))),
        ("signature slot far in the future", Box::new(|b: &mut Vec<u8>| b[FU_SIGNATURE_SLOT + 4] ^= 0x10)),
    ];
    for (what, mutate) in &mutations {
        let mut bytes = fu_bytes.clone();
        mutate(&mut bytes);
        let mut pair = at_1409.clone();
        let before = pair.finalized();
        assert_eq!(pair.submit(&bytes, Data::FinalityUpdate(LcFork::Gloas)), (false, false, false), "{what}");
        assert_eq!(pair.finalized(), before, "{what}: a refused update changed a store");
    }
    // The first committee handoff under Gloas (period 1379), with its next committee altered.
    let mut at_fork = sepolia_through(1378);
    at_fork.upgrade(LcFork::Gloas);
    let (u1379, data) = sepolia_update(1379);
    assert_eq!(at_fork.clone().submit(&u1379, data), (true, true, true));
    let handoff: Vec<Mutation> = vec![
        ("next committee key byte", Box::new(|b: &mut Vec<u8>| b[NEXT_COMMITTEE + 100] ^= 0x01)),
        ("next-committee branch byte", Box::new(|b: &mut Vec<u8>| b[NEXT_BRANCH + 2 * 32 + 7] ^= 0x01)),
    ];
    for (what, mutate) in &handoff {
        let mut bytes = u1379.clone();
        mutate(&mut bytes);
        assert_eq!(at_fork.clone().submit(&bytes, data), (false, false, false), "{what}");
    }
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
    let electra = Data::Update(LcFork::Electra);
    for (what, mutate) in &mutations {
        let mut bytes = original.clone();
        mutate(&mut bytes);
        let mut pair =
            Pair::mainnet("mainnet-p1899", "0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac");
        let before = pair.finalized();
        assert_eq!(pair.submit(&bytes, electra), (false, false, false), "{what}");
        assert_eq!(pair.finalized(), before, "{what}: a refused update changed a store");
    }
    // A store that holds the wrong committee: the period-1900 committee in place of 1899's.
    let mut pair = Pair::mainnet("mainnet-p1899", "0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac");
    let next = decoded.next_sync_committee.clone();
    let core = StoreCore { current_sync_committee: next.clone(), ..pair.ours.core().clone() };
    pair.ours = FinalizedStore::from_core(core.clone(), &BLST).unwrap();
    pair.spec_store.core = core;
    let hnext = helios_update::<MainnetConsensusSpec>(&original).unwrap().next_sync_committee.unwrap();
    pair.theirs.current_sync_committee = hnext;
    assert_eq!(pair.submit(&original, electra), (false, false, false), "wrong committee");
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
        assert_eq!(ours(&spec_store, spec), theirs(&hstore), "{signers} signers");
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

/// Inputs on which Helios departs from the light-client specification, each pinned with the spec
/// rule that decides it. None occurs on a genuine chain with an honest committee, and none was
/// among the inputs compared above. Helios 0.9.4 departed on inputs 1 to 4; at 0.12.0 it no longer
/// departs on 1 (a16z/helios#819) or 4 (#820), still departs on 2 and 3, and departs on 5, which
/// came with #751.
#[test]
fn where_helios_departs_from_the_specification() {
    use ferrata_spectests::testkit::{fabricated_update, fake_committee, sign, with_wrong_aggregate, Fabrication};
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
    //    assert update.next_sync_committee == store.next_sync_committee`). Ferrata refuses. Helios
    //    0.9.4 had no such check and accepted; 0.12.0 refuses as well (a16z/helios#819).
    let (b, c) = (fake_committee(32, 0x6b).committee, fake_committee(32, 0x7c).committee);
    let (mut ours1, mut theirs1) = chain.stores();
    assert_eq!(chain.submit(&mut ours1, &mut theirs1, &finality(72, 80, Some(&b)), 82), (Ok(()), true));
    let (ferrata, helios) = chain.submit(&mut ours1, &mut theirs1, &finality(88, 96, Some(&c)), 98);
    assert_eq!(
        (ferrata, helios),
        (Err(ferrata_core::Error::NextCommitteeMismatch), false),
        "next committee contradiction"
    );
    assert_eq!(ours(&ours1, &chain.spec), theirs(&theirs1));

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
    // The aggregate stays the sum of the keys (the identity adds nothing), so that Helios's use
    // of that field (input 5) does not decide this input.
    with_identity.aggregate_pubkey = {
        use blst::min_pk::{AggregatePublicKey, PublicKey};
        let keys: Vec<PublicKey> =
            with_identity.pubkeys.iter().filter(|k| k[0] != 0xc0).map(|k| PublicKey::from_bytes(k).unwrap()).collect();
        AggregatePublicKey::aggregate(&keys.iter().collect::<Vec<_>>(), false).unwrap().to_public_key().compress()
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
    //    then treats the attested header as finalized. Ferrata does. Helios 0.9.4's `force_update`
    //    unwrapped the absent finalized header and panicked; 0.12.0 does what the spec does
    //    (a16z/helios#820).
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
    helios_force_update(&mut theirs4, timed_out);
    assert_eq!(ours(&ours4, &chain.spec), theirs(&theirs4), "forced update");

    // 5. A committee whose `aggregate_pubkey` is not the sum of its keys. The spec never reads the
    //    field: `validate_light_client_update` checks the signature against the participants' keys
    //    (`eth_fast_aggregate_verify`), so ferrata's spec store accepts an update all members
    //    signed (its policy store refuses such a committee at bootstrap). Helios 0.12.0 derives the
    //    participants' key from the field when more than half sign (a16z/helios#751), and refuses.
    let chain5 = fabricated_chain(&with_wrong_aggregate(&fake.committee));
    let ctx5 = chain5.ctx();
    let signed_by_all = fabricated_update(
        &Fabrication {
            finalized_slot: Some(72),
            attested_slot: 80,
            signature_slot: 81,
            signers: 32,
            next_committee: None,
        },
        &fake,
        &ctx5,
    );
    let (mut ours5, mut theirs5) = chain5.stores();
    assert_eq!(chain5.submit(&mut ours5, &mut theirs5, &signed_by_all, 82), (Ok(()), false), "aggregate field");
    assert_eq!(
        FinalizedStore::initialize(&chain5.trusted, &chain5.bootstrap, LcFork::Electra, &chain5.spec, &BLST)
            .unwrap_err(),
        ferrata_core::Error::CommitteeInvalid
    );
    // With no more than half signing, Helios sums the participants' keys, as the spec does.
    let signed_by_half = fabricated_update(
        &Fabrication {
            finalized_slot: Some(72),
            attested_slot: 80,
            signature_slot: 81,
            signers: 16,
            next_committee: None,
        },
        &fake,
        &ctx5,
    );
    let (mut ours5, mut theirs5) = chain5.stores();
    assert_eq!(chain5.submit(&mut ours5, &mut theirs5, &signed_by_half, 82), (Ok(()), true), "half signing");
}

/// The seed and size of the random mutations below.
const MUTATION_SEED: u64 = 0x2611_21f8;
const MUTATIONS_PER_UPDATE: usize = 400;
/// The trusted root of the `mainnet-p1899` fixture's bootstrap.
const P1899_TRUSTED: &str = "0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac";

/// One seeded mutation of one of mainnet periods 1899-1903's five real updates.
struct RandomMutation {
    /// Which update (0 to 4) it mutates.
    update: usize,
    /// Its index among that update's mutations.
    index: usize,
    region: &'static str,
    /// (byte, bit) of every flip.
    flips: Vec<(usize, usize)>,
    bytes: Vec<u8>,
}

/// The five real updates of mainnet periods 1899-1903, as served.
fn p1899_updates() -> Vec<Vec<u8>> {
    (0..5).map(|i| fixture("mainnet-p1899", &format!("update_{i}.ssz"))).collect()
}

/// [`MUTATIONS_PER_UPDATE`] seeded mutations of each of the five updates: one to three distinct
/// bits flipped (never the same bit twice, so no flip cancels another), in a region of the
/// update's SSZ encoding chosen uniformly (the two offsets, the next committee and its branch, the
/// finality branch, the participation bits, the signature, the signature slot, and each header's
/// beacon fields, execution branch and payload), at a uniform offset in it.
fn random_mutations(originals: &[Vec<u8>]) -> Vec<RandomMutation> {
    use ferrata_spectests::execkit::Rng;
    let mut rng = Rng::new(MUTATION_SEED);
    let mut out = Vec::new();
    for (i, original) in originals.iter().enumerate() {
        let u32_at = |o: usize| u32::from_le_bytes(original[o..o + 4].try_into().unwrap()) as usize;
        let (att, fin) = (u32_at(0), u32_at(layout::NEXT_BRANCH + 6 * 32));
        assert_eq!((att, fin > att), (layout::FIXED, true), "update {i}: the offsets");
        let regions: [(&str, usize, usize); 14] = [
            ("offset of the attested header", 0, 4),
            ("next committee", layout::NEXT_COMMITTEE, 513 * 48),
            ("next-committee branch", layout::NEXT_BRANCH, 6 * 32),
            ("offset of the finalized header", layout::NEXT_BRANCH + 6 * 32, 4),
            ("finality branch", layout::FINALITY_BRANCH, 7 * 32),
            ("participation bits", layout::BITS, 64),
            ("signature", layout::SIGNATURE, 96),
            ("signature slot", layout::SIGNATURE_SLOT, 8),
            ("attested beacon header", att, 112),
            ("attested execution branch and offset", att + 112, 132),
            ("attested payload", att + 244, fin - att - 244),
            ("finalized beacon header", fin, 112),
            ("finalized execution branch and offset", fin + 112, 132),
            ("finalized payload", fin + 244, original.len() - fin - 244),
        ];
        for index in 0..MUTATIONS_PER_UPDATE {
            let (region, start, len) = regions[rng.below(regions.len())];
            let mut bytes = original.clone();
            let count = 1 + rng.below(3);
            let mut flips: Vec<(usize, usize)> = Vec::new();
            while flips.len() < count {
                let flip = (start + rng.below(len), rng.below(8));
                if !flips.contains(&flip) {
                    bytes[flip.0] ^= 1 << flip.1;
                    flips.push(flip);
                }
            }
            assert_ne!(&bytes, original, "a mutation changes the bytes");
            out.push(RandomMutation { update: i, index, region, flips, bytes });
        }
    }
    out
}

/// Seeded random mutations of every real update of mainnet periods 1899 to 1903 ([`random_mutations`]),
/// each against the stores as they stand just before that update. ferrata's spec store and Helios
/// must give the same verdict; where both accept, the same finalized and optimistic headers;
/// ferrata's policy store (what the program runs) may accept only what the spec store accepts.
/// Prints the tally by region.
#[test]
fn random_mutations_of_real_updates_agree_with_helios() {
    let originals = p1899_updates();
    let mutations = random_mutations(&originals);
    let mut pair = Pair::mainnet("mainnet-p1899", P1899_TRUSTED);
    let electra = Data::Update(LcFork::Electra);
    // region -> (mutations, all three refuse, spec store and Helios accept, policy refuses what both accept)
    let mut tally: std::collections::BTreeMap<String, [usize; 4]> = std::collections::BTreeMap::new();
    let mut disagreements = Vec::new();
    let mut accepted = Vec::new();
    for (i, original) in originals.iter().enumerate() {
        for RandomMutation { index: m, region: name, flips: at, bytes, .. } in mutations.iter().filter(|x| x.update == i) {
            let mut p = pair.clone();
            let (policy, spec, helios) = p.submit(bytes, electra);
            let t = tally.entry(name.to_string()).or_default();
            t[0] += 1;
            if !policy && !spec && !helios {
                t[1] += 1;
            }
            if spec && helios {
                t[2] += 1;
                if !policy {
                    t[3] += 1;
                }
                accepted.push(format!("update {i} mutation {m} ({name}, flips {at:?}): policy {policy}"));
                if !p.agree() {
                    disagreements.push(format!(
                        "update {i} mutation {m} ({name}, flips {at:?}): both accept, the headers differ"
                    ));
                }
            }
            if spec != helios {
                disagreements.push(format!(
                    "update {i} mutation {m} ({name}, flips {at:?}): spec store {spec}, Helios {helios}"
                ));
            }
            if policy && !spec {
                disagreements.push(format!("update {i} mutation {m} ({name}, flips {at:?}): the policy store accepts what the spec store refuses"));
            }
        }
        assert_eq!(pair.submit(original, electra), (true, true, true), "update {i}");
        assert!(pair.agree());
    }
    let mut out = format!(
        "{} seeded mutations (seed {MUTATION_SEED:#x}, {MUTATIONS_PER_UPDATE} per update) of mainnet periods 1899-1903's five updates:\n\
         region | mutations | refused by all three | accepted by spec store and Helios (same heads) | of those, refused by the policy store\n",
        mutations.len()
    );
    for (name, t) in &tally {
        out.push_str(&format!("{name} | {} | {} | {} | {}\n", t[0], t[1], t[2], t[3]));
    }
    out.push_str("mutations accepted by both (each with the same heads in both):\n");
    for a in &accepted {
        out.push_str(&format!("  {a}\n"));
    }
    out.push_str(&format!("disagreements: {}\n", disagreements.len()));
    for d in &disagreements {
        out.push_str(&format!("  {d}\n"));
    }
    println!("{out}");
    assert!(disagreements.is_empty(), "{out}");
}

/// Writes the corpus of [`random_mutations`] for a client other than Helios to judge
/// (`lodestar/diff.mjs` runs Lodestar's light client over it; see `HELIOS-DIFFERENTIAL.md`), into
/// the directory `FERRATA_DIFF_CORPUS` names: `meta.json`, the bootstrap, `corpus.bin` (the five
/// updates as served, then the mutations; each record four little-endian `u32`s, kind (0 an
/// update as served, 1 a mutation), update, mutation index, length, then the bytes) and
/// `ferrata.tsv` (every record's verdicts in ferrata's policy store, its spec store and Helios,
/// and the spec store's finalized and optimistic headers after it where it accepts).
#[test]
#[ignore = "writes a corpus for lodestar/diff.mjs; run with FERRATA_DIFF_CORPUS=<dir> and --ignored"]
fn export_random_mutations_for_a_second_client() {
    let dir = std::path::PathBuf::from(std::env::var("FERRATA_DIFF_CORPUS").expect("FERRATA_DIFF_CORPUS"));
    fs::create_dir_all(&dir).unwrap();
    let originals = p1899_updates();
    let mutations = random_mutations(&originals);
    let electra = Data::Update(LcFork::Electra);
    let mut pair = Pair::mainnet("mainnet-p1899", P1899_TRUSTED);
    let mut corpus = Vec::new();
    let mut tsv = String::from(
        "kind\tupdate\tmutation\tregion\tpolicy\tspec\thelios\tfinalized_slot\tfinalized_root\toptimistic_slot\toptimistic_root\n",
    );
    let record = |corpus: &mut Vec<u8>, kind: u32, update: usize, index: usize, bytes: &[u8]| {
        for v in [kind, update as u32, index as u32, bytes.len() as u32] {
            corpus.extend_from_slice(&v.to_le_bytes());
        }
        corpus.extend_from_slice(bytes);
    };
    let row = |kind: u32, update: usize, index: usize, region: &str, v: (bool, bool, bool), p: &Pair| {
        let heads = if v.1 {
            let ((fs, fr, _), (os, or, _)) = ours(&p.spec_store, &p.spec);
            format!("{fs}\t{}\t{os}\t{}", hex::encode(fr), hex::encode(or))
        } else {
            "-\t-\t-\t-".to_string()
        };
        format!("{kind}\t{update}\t{index}\t{region}\t{}\t{}\t{}\t{heads}\n", v.0 as u8, v.1 as u8, v.2 as u8)
    };
    for (i, original) in originals.iter().enumerate() {
        for m in mutations.iter().filter(|x| x.update == i) {
            let mut p = pair.clone();
            let v = p.submit(&m.bytes, electra);
            record(&mut corpus, 1, i, m.index, &m.bytes);
            tsv.push_str(&row(1, i, m.index, m.region, v, &p));
        }
        let v = pair.submit(original, electra);
        assert_eq!(v, (true, true, true), "update {i}");
        record(&mut corpus, 0, i, 0, original);
        tsv.push_str(&row(0, i, 0, "as served", v, &pair));
    }
    fs::write(dir.join("corpus.bin"), &corpus).unwrap();
    fs::write(dir.join("ferrata.tsv"), &tsv).unwrap();
    fs::write(dir.join("bootstrap.ssz"), fixture("mainnet-p1899", "bootstrap.ssz")).unwrap();
    fs::write(
        dir.join("meta.json"),
        format!(
            "{{\"chain\": \"mainnet\", \"genesis_validators_root\": \"0x{GVR}\", \"trusted_root\": \"{P1899_TRUSTED}\", \"seed\": \"{MUTATION_SEED:#x}\", \"per_update\": {MUTATIONS_PER_UPDATE}, \"records\": {}}}\n",
            originals.len() + mutations.len()
        ),
    )
    .unwrap();
    println!("{} records ({} bytes) written to {}", originals.len() + mutations.len(), corpus.len(), dir.display());
}
