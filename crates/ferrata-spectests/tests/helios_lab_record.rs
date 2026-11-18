//! Helios beside ferrata-core on every light-client update the lab recorded, and the two inputs on
//! which the two part (a16z/helios issues 871 and 872), each on the data that triggers it.
//!
//! The lab (`lez/lab/`) runs Ferrata's light client as a LEZ program fed from Sepolia's and
//! mainnet's beacon endpoints. For each chain it keeps the bootstrap, the trusted checkpoint and the
//! served bytes of every update the instance applied, in order
//! (`lez/lab/results/record/net/<chain>/updates/NNNNN.<kind>.<layout>.ssz`), one row per job in
//! `net/<chain>/jobs.tsv`, and each instance's end state in `instances.tsv`. Each chain's record is
//! replayed here from those bytes through three stores: ferrata-core's policy store
//! (`FinalizedStore` with `Adoption::OnChain`, what the program runs), its spec store
//! (`LightClientStore`), and Helios's consensus core (a16z/helios tag 0.12.0, rev 2af9b41:
//! `verify_generic_update`, then `apply_generic_update`), each update at
//! `current_slot = signature_slot + 1`, as the lab did. After every update:
//! * all three verdicts are the same, and an acceptance (the lab applied every kept update);
//! * the finalized header is the same (slot and beacon root, so the same sync-committee period),
//!   with the same execution block (its hash, its number where the layout carries the payload
//!   header) and the same execution root;
//! * the current and the next sync committee are the same, and the optimistic header of the spec
//!   store and of Helios;
//! * whether the update rotated the committees, advanced the finalized header, or made the store
//!   adopt a next committee is the same read off ferrata's store and off Helios's, and equal to the
//!   policy store's outcome and the lab's `jobs.tsv` row, whose slots equal the decoded update's.
//!
//! At the end, the finalized slot, period and root equal the lab's `instances.tsv`. The official
//! `sync` vectors are compared step by step in `helios_oracle.rs` (the same pin).
//!
//! Run: `cargo test -p ferrata-spectests --test helios_lab_record --locked -- --nocapture`.

use std::{
    fs,
    path::{Path, PathBuf},
};

use alloy_primitives::{FixedBytes, B256};
use ferrata_core::{
    bls::{Aggregation, Blst},
    fork::{ChainSpec, Fork, LcFork, Preset, FAR_FUTURE_EPOCH},
    merkle::ZERO,
    policy::{Adoption, FinalizedStore},
    store::LightClientStore,
    types::{Execution, LightClientBootstrap, LightClientHeader, LightClientUpdate, SyncAggregate, SyncCommittee},
    validate::{get_lc_execution_root, Context},
    Error, Root,
};
use helios_consensus_core::{
    apply_bootstrap, apply_generic_update,
    consensus_spec::{ConsensusSpec as HeliosSpec, MainnetConsensusSpec, MinimalConsensusSpec},
    types::{
        Bootstrap as HBootstrap, FinalityUpdate as HFinalityUpdate, Fork as HFork, Forks, GenericUpdate,
        LightClientHeader as HHeader, LightClientStore as HStore, Update as HUpdate,
    },
    verify_bootstrap, verify_generic_update,
};
use ssz::Decode;
use tree_hash::TreeHash;

static BLST: Blst = Blst::new(Aggregation::Auto);

/// `SLOTS_PER_EPOCH * EPOCHS_PER_SYNC_COMMITTEE_PERIOD`, mainnet preset (both chains).
const SLOTS_PER_PERIOD: u64 = 32 * 256;

fn record_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lez/lab/results/record")
}

/// Mainnet's fork schedule (consensus-specs `configs/mainnet.yaml`), as in `helios_oracle.rs`.
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

/// Sepolia's fork schedule (eth-clients/sepolia `metadata/config.yaml`): Gloas from epoch 353,024.
fn sepolia() -> ChainSpec {
    let v = |b: u8| [0x90, 0x00, 0x00, b];
    ChainSpec {
        preset: Preset::MAINNET,
        versions: [v(0x69), v(0x70), v(0x71), v(0x72), v(0x73), v(0x74), v(0x75), v(0x76), [0; 4]],
        epochs: [0, 50, 100, 56_832, 132_608, 222_464, 272_640, 353_024, FAR_FUTURE_EPOCH],
    }
}

const MAINNET_GVR: &str = "4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95";
const SEPOLIA_GVR: &str = "d8ea171f3c94aea21ebc42a1ed61052acf3f9209c00e4efbaaddac09ed9b8078";

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

fn root(s: &str) -> Root {
    hex::decode(s.trim().trim_start_matches("0x")).unwrap().try_into().unwrap()
}

fn b256(r: &Root) -> B256 {
    B256::from(*r)
}

/// Helios's decoding of served bytes: `LightClientUpdate`, else `LightClientFinalityUpdate`
/// (Helios picks the layout by trial decoding; ferrata by the fork the data is served under).
fn helios_update<S: HeliosSpec>(bytes: &[u8]) -> Option<GenericUpdate<S>> {
    if let Ok(u) = HUpdate::<S>::from_ssz_bytes(bytes) {
        return Some(GenericUpdate::from(&u));
    }
    HFinalityUpdate::<S>::from_ssz_bytes(bytes).ok().map(|u| GenericUpdate::from(&u))
}

/// `LightClientFinalityUpdate` as the update it stands for (empty committee fields, as the spec's
/// `process_light_client_finality_update` does): the Electra layout puts the two headers behind
/// offsets; in the Gloas layout every field is fixed-size.
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

fn layout_of(name: &str) -> LcFork {
    match name {
        "electra" => LcFork::Electra,
        "gloas" => LcFork::Gloas,
        other => panic!("layout {other}"),
    }
}

/// What both stores hold that the comparison reads: the finalized header (slot, beacon root,
/// execution block number where the layout has it, execution block hash, execution root), the
/// current and the next committee's roots, the optimistic header (slot, beacon root).
#[derive(Debug, PartialEq, Eq, Clone)]
struct View {
    finalized: (u64, Root),
    block: (Option<u64>, Root),
    execution_root: Root,
    current: Root,
    next: Option<Root>,
    optimistic: (u64, Root),
}

fn ferrata_view(store: &LightClientStore, spec: &ChainSpec) -> View {
    let core = &store.core;
    let header = &core.finalized_header;
    let block = match &header.execution {
        Execution::Payload { header, .. } => (Some(header.block_number), header.block_hash),
        Execution::BlockHash { block_hash, .. } => (None, *block_hash),
        Execution::None => (None, ZERO),
    };
    View {
        finalized: (header.beacon.slot, header.beacon.hash_tree_root()),
        block,
        execution_root: get_lc_execution_root(header, core.fork, spec),
        current: core.current_sync_committee.hash_tree_root(),
        next: (!core.next_sync_committee.is_empty()).then(|| core.next_sync_committee.hash_tree_root()),
        optimistic: (store.optimistic_header.beacon.slot, store.optimistic_header.beacon.hash_tree_root()),
    }
}

fn helios_view<S: HeliosSpec>(store: &HStore<S>) -> View {
    let header = &store.finalized_header;
    let block = match header {
        HHeader::Gloas(g) => (None, g.execution_block_hash.0),
        other => match other.execution() {
            Ok(p) => (Some(*p.block_number()), p.block_hash().0),
            Err(_) => (None, ZERO),
        },
    };
    View {
        finalized: (header.beacon().slot, header.beacon().tree_hash_root().0),
        block,
        execution_root: header.execution_root().0,
        current: store.current_sync_committee.tree_hash_root().0,
        next: store.next_sync_committee.as_ref().map(|c| c.tree_hash_root().0),
        optimistic: (store.optimistic_header.beacon().slot, store.optimistic_header.beacon().tree_hash_root().0),
    }
}

/// (rotated, advanced, adopted) read off two views of one store: the current committee changed,
/// the finalized slot grew, a next committee became known or changed.
fn moved(before: &View, after: &View) -> (bool, bool, bool) {
    (
        after.current != before.current,
        after.finalized.0 > before.finalized.0,
        after.next.is_some() && after.next != before.next,
    )
}

/// One row of `net/<chain>/jobs.tsv` after `init`.
#[derive(Debug)]
struct Job {
    kind: String,
    attested: u64,
    signature: u64,
    finalized: u64,
    layout: String,
    outcome: (bool, bool, bool),
}

fn jobs(chain: &str) -> (String, Vec<Job>) {
    let text = fs::read_to_string(record_dir().join("net").join(chain).join("jobs.tsv")).unwrap();
    let mut lines = text.lines();
    let head: Vec<&str> = lines.next().unwrap().split('\t').collect();
    let col = |name: &str| head.iter().position(|h| *h == name).unwrap_or_else(|| panic!("no column {name}"));
    let (kind, att, sig, fin, layout) =
        (col("kind"), col("attested_slot"), col("signature_slot"), col("finalized_slot"), col("layout"));
    let (rot, adv, ado, status) = (col("rotated"), col("advanced"), col("adopted"), col("status"));
    let mut init_layout = String::new();
    let mut out = Vec::new();
    for line in lines.filter(|l| !l.trim().is_empty()) {
        let f: Vec<&str> = line.split('\t').collect();
        assert_eq!(f[status], "applied", "{chain}: a job not applied: {line}");
        if f[kind] == "init" {
            init_layout = f[layout].to_string();
            continue;
        }
        let b = |i: usize| match f[i] {
            "true" => true,
            "false" => false,
            other => panic!("{chain}: not a bool: {other}"),
        };
        out.push(Job {
            kind: f[kind].to_string(),
            attested: f[att].parse().unwrap(),
            signature: f[sig].parse().unwrap(),
            finalized: f[fin].parse().unwrap(),
            layout: f[layout].to_string(),
            outcome: (b(rot), b(adv), b(ado)),
        });
    }
    (init_layout, out)
}

/// The instance's end state in `instances.tsv`: finalized slot, period, root, next committee known.
fn instance(chain: &str) -> (u64, u64, Root, bool) {
    let text = fs::read_to_string(record_dir().join("instances.tsv")).unwrap();
    let mut lines = text.lines();
    let head: Vec<&str> = lines.next().unwrap().split('\t').collect();
    let col = |name: &str| head.iter().position(|h| *h == name).unwrap();
    let f: Vec<&str> = lines.map(|l| l.split('\t').collect::<Vec<_>>()).find(|f| f[col("chain")] == chain).unwrap();
    (
        f[col("finalized_slot")].parse().unwrap(),
        f[col("finalized_period")].parse().unwrap(),
        root(f[col("finalized_root")]),
        f[col("next_committee_known")] == "true",
    )
}

/// Ferrata's policy store and spec store and Helios's store, bootstrapped from the same bytes.
struct Trio {
    spec: ChainSpec,
    gvr: Root,
    policy: FinalizedStore,
    spec_store: LightClientStore,
    helios: HStore<MainnetConsensusSpec>,
}

/// The three verdicts on one update, with ferrata's errors.
#[derive(Debug, PartialEq)]
struct Verdicts {
    policy: Result<(bool, bool, bool), Error>,
    spec: Result<(), Error>,
    helios: bool,
}

impl Trio {
    fn new(chain: &str, spec: ChainSpec, gvr: &str, layout: LcFork) -> Trio {
        let dir = record_dir().join("net").join(chain);
        let raw = fs::read(dir.join("bootstrap.ssz")).unwrap();
        let trusted = root(&fs::read_to_string(dir.join("checkpoint")).unwrap());
        let boot = LightClientBootstrap::decode(&raw, layout, 512).unwrap();
        let policy = FinalizedStore::initialize_with(&trusted, &boot, layout, &spec, &BLST, Adoption::OnChain).unwrap();
        let spec_store = LightClientStore::initialize(&trusted, &boot, layout, &spec).unwrap();
        let hboot = HBootstrap::<MainnetConsensusSpec>::from_ssz_bytes(&raw).unwrap();
        verify_bootstrap(&hboot, b256(&trusted), &forks(&spec)).unwrap();
        let mut helios = HStore::default();
        apply_bootstrap(&mut helios, &hboot);
        let trio = Trio { spec, gvr: root(gvr), policy, spec_store, helios };
        assert_eq!(trio.ours(), trio.theirs(), "{chain}: after the bootstrap");
        trio
    }

    fn ours(&self) -> View {
        ferrata_view(&self.spec_store, &self.spec)
    }

    fn theirs(&self) -> View {
        helios_view(&self.helios)
    }

    /// Ferrata's stores move to a later layout at a fork; Helios's holds headers of any layout.
    fn upgrade_to(&mut self, layout: LcFork) -> bool {
        if layout <= self.policy.core().fork {
            return false;
        }
        self.policy.upgrade(layout, &self.spec).unwrap();
        self.spec_store.upgrade(layout, &self.spec).unwrap();
        true
    }

    /// Submit one update, decoded by ferrata as `update` and by Helios from `bytes`.
    fn submit(&mut self, update: &LightClientUpdate, bytes: &[u8]) -> Verdicts {
        let spec = self.spec.clone();
        let ctx = Context { spec: &spec, genesis_validators_root: self.gvr, bls: &BLST };
        let slot = update.signature_slot + 1;
        let policy = self.policy.process_update(update, slot, &ctx).map(|o| (o.rotated, o.advanced, o.adopted));
        let spec_verdict = self.spec_store.process_update(update, slot, &ctx);
        let h = helios_update::<MainnetConsensusSpec>(bytes).expect("Helios decodes the bytes");
        let helios = verify_generic_update(&h, slot, &self.helios, b256(&self.gvr), &forks(&self.spec)).is_ok();
        if helios {
            apply_generic_update(&mut self.helios, &h);
        }
        Verdicts { policy, spec: spec_verdict, helios }
    }
}

#[derive(Default, Debug)]
struct Tally {
    updates: usize,
    period_updates: usize,
    finality_updates: usize,
    upgrades: usize,
    rotated: usize,
    advanced: usize,
    adopted: usize,
    periods: std::collections::BTreeSet<u64>,
}

fn replay(chain: &str, spec: ChainSpec, gvr: &str) -> Tally {
    let (init_layout, jobs) = jobs(chain);
    let mut trio = Trio::new(chain, spec, gvr, layout_of(&init_layout));
    let dir = record_dir().join("net").join(chain).join("updates");
    let mut names: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.ends_with(".ssz"))
        .collect();
    names.sort();
    assert_eq!(names.len(), jobs.len(), "{chain}: one kept update per job after init");
    let mut t = Tally::default();
    t.periods.insert(trio.ours().finalized.0 / SLOTS_PER_PERIOD);
    for (name, job) in names.iter().zip(&jobs) {
        let parts: Vec<&str> = name.trim_end_matches(".ssz").split('.').collect();
        let (kind, layout_name) = (parts[1], parts[2]);
        assert_eq!((kind, layout_name), (job.kind.as_str(), job.layout.as_str()), "{chain} {name}: the jobs.tsv row");
        let layout = layout_of(layout_name);
        let bytes = fs::read(dir.join(name)).unwrap();
        let update = match kind {
            "finality" => finality_update(&bytes, layout),
            "period" => LightClientUpdate::decode(&bytes, layout, 512).unwrap(),
            other => panic!("{chain} {name}: kind {other}"),
        };
        assert_eq!(
            (update.attested_header.beacon.slot, update.signature_slot, update.finalized_header.beacon.slot),
            (job.attested, job.signature, job.finalized),
            "{chain} {name}: the slots of the jobs.tsv row"
        );
        if trio.upgrade_to(layout) {
            t.upgrades += 1;
        }
        let (ours_before, theirs_before) = (trio.ours(), trio.theirs());
        let v = trio.submit(&update, &bytes);
        assert_eq!(v, Verdicts { policy: Ok(job.outcome), spec: Ok(()), helios: true }, "{chain} {name}: verdicts");
        let (ours, theirs) = (trio.ours(), trio.theirs());
        assert_eq!(ours, theirs, "{chain} {name}: ferrata's store and Helios's differ");
        assert_eq!(trio.policy.core().finalized_header, trio.spec_store.core.finalized_header, "{chain} {name}");
        let (ours_moved, theirs_moved) = (moved(&ours_before, &ours), moved(&theirs_before, &theirs));
        assert_eq!(ours_moved, job.outcome, "{chain} {name}: ferrata's store against the lab's row");
        assert_eq!(theirs_moved, job.outcome, "{chain} {name}: Helios's store against the lab's row");
        t.updates += 1;
        if kind == "period" {
            t.period_updates += 1;
        } else {
            t.finality_updates += 1;
        }
        t.rotated += usize::from(job.outcome.0);
        t.advanced += usize::from(job.outcome.1);
        t.adopted += usize::from(job.outcome.2);
        t.periods.insert(ours.finalized.0 / SLOTS_PER_PERIOD);
    }
    let (slot, period, finalized_root, next_known) = instance(chain);
    let end = trio.theirs();
    assert_eq!(
        (end.finalized.0, end.finalized.0 / SLOTS_PER_PERIOD, end.finalized.1, end.next.is_some()),
        (slot, period, finalized_root, next_known),
        "{chain}: the end state against instances.tsv"
    );
    println!(
        "{chain}: {} updates ({} period, {} finality), {} layout upgrades, {} rotations, {} advances, {} adoptions; \
         finalized periods {:?}..={:?}; end: slot {slot}, period {period}, root 0x{}, block {:?} 0x{}",
        t.updates,
        t.period_updates,
        t.finality_updates,
        t.upgrades,
        t.rotated,
        t.advanced,
        t.adopted,
        t.periods.first().unwrap(),
        t.periods.last().unwrap(),
        hex::encode(finalized_root),
        end.block.0,
        hex::encode(end.block.1),
    );
    t
}

#[test]
fn every_recorded_sepolia_update_agrees_with_helios() {
    let t = replay("sepolia", sepolia(), SEPOLIA_GVR);
    // The bootstrap is in the Electra layout (period 1378, before Gloas); the stores move to the
    // Gloas layout once.
    assert_eq!(t.upgrades, 1);
    assert!(t.updates >= 125, "the record of 17 Nov 2026 holds 125 Sepolia updates");
}

#[test]
fn every_recorded_mainnet_update_agrees_with_helios() {
    let t = replay("mainnet", mainnet(), MAINNET_GVR);
    assert_eq!(t.upgrades, 0);
    assert!(t.updates >= 95, "the record of 17 Nov 2026 holds 95 mainnet updates");
}

/// a16z/helios issue 872, on real data: a genuine finality update with its finalized header
/// replaced by the empty header, its finality branch kept.
///
/// The spec (`validate_light_client_update`): the update is a finality update, since its branch is
/// not all zero; its finalized slot is `GENESIS_SLOT`, so the header must be the empty
/// `LightClientHeader()` and the branch must prove the zero root at the finalized-root gindex of
/// the attested state. The kept branch proves the real finalized root, not zero: invalid. Helios
/// (`default_header_to_none`) turns any finalized header at slot 0 into "no finalized header" and
/// then checks no finality branch at all, so it accepts, and moves its optimistic header to the
/// attested header; its finalized header stays. The signature covers only the attested header, so
/// anyone holding a genuine update can build this.
#[test]
fn helios_872_a_finality_branch_under_an_empty_finalized_header() {
    let dir = record_dir().join("net/mainnet/updates");
    // Mainnet's first two kept updates: the period update of 1907, then a finality update.
    let period = fs::read(dir.join("00001.period.electra.ssz")).unwrap();
    let genuine = fs::read(dir.join("00002.finality.electra.ssz")).unwrap();
    let fresh = || {
        let mut trio = Trio::new("mainnet", mainnet(), MAINNET_GVR, LcFork::Electra);
        let u = LightClientUpdate::decode(&period, LcFork::Electra, 512).unwrap();
        assert_eq!(trio.submit(&u, &period), Verdicts { policy: Ok((false, false, true)), spec: Ok(()), helios: true });
        trio
    };
    let real = finality_update(&genuine, LcFork::Electra);
    assert!(real.finality_branch.iter().any(|n| *n != ZERO));

    // The genuine update: all three accept and agree.
    let mut trio = fresh();
    let v = trio.submit(&real, &genuine);
    assert_eq!((v.policy.is_ok(), v.spec, v.helios), (true, Ok(()), true));
    assert_eq!(trio.ours(), trio.theirs());

    // 1. The finalized header emptied (every field zero, as `LightClientHeader()`), the genuine
    //    branch kept. Encoded as a `LightClientUpdate` (the committee fields empty), which both
    //    decode.
    let mut emptied = real.clone();
    emptied.finalized_header = LightClientHeader::empty(LcFork::Electra);
    let bytes = encode::update(&emptied);
    assert_eq!(LightClientUpdate::decode(&bytes, LcFork::Electra, 512).unwrap(), emptied);
    let mut trio = fresh();
    let before = (trio.ours(), trio.theirs());
    let v = trio.submit(&emptied, &bytes);
    assert_eq!(
        v,
        Verdicts { policy: Err(Error::FinalityBranchInvalid), spec: Err(Error::FinalityBranchInvalid), helios: true },
        "empty finalized header, genuine branch"
    );
    let (ours, theirs) = (trio.ours(), trio.theirs());
    assert_eq!(ours, before.0, "ferrata's spec store is unchanged");
    assert_eq!(theirs.finalized, before.1.finalized, "Helios's finalized header stays");
    assert_eq!(
        theirs.optimistic,
        (real.attested_header.beacon.slot, real.attested_header.beacon.hash_tree_root()),
        "Helios's optimistic header moves to the attested header"
    );
    println!(
        "872, empty finalized header: ferrata {:?} / {:?}; Helios accepts; its optimistic header {} -> {}",
        v.policy, v.spec, before.1.optimistic.0, theirs.optimistic.0
    );

    // 2. The precise trigger in Helios is the slot: the genuine finalized header with only its slot
    //    set to 0. The spec: at `GENESIS_SLOT` the header must be empty, and this one is not.
    let mut slot_zero = real.clone();
    slot_zero.finalized_header.beacon.slot = 0;
    let bytes = encode::update(&slot_zero);
    let mut trio = fresh();
    let v = trio.submit(&slot_zero, &bytes);
    assert_eq!(
        v,
        Verdicts { policy: Err(Error::FinalizedHeaderInvalid), spec: Err(Error::FinalizedHeaderInvalid), helios: true },
        "finalized header at slot 0"
    );

    // 3. The same bytes with the branch zeroed too: no longer a finality update. The spec accepts
    //    it as an update without finality (the policy store, which applies finality updates only,
    //    refuses it); Helios accepts. No departure.
    let mut plain = emptied.clone();
    plain.finality_branch = vec![ZERO; plain.finality_branch.len()];
    let bytes = encode::update(&plain);
    let mut trio = fresh();
    let v = trio.submit(&plain, &bytes);
    assert_eq!(v, Verdicts { policy: Err(Error::NotFinalized), spec: Ok(()), helios: true }, "no finality");
    assert_eq!(trio.ours(), trio.theirs());
}

/// a16z/helios issue 871: a sync committee holding the identity point (`0xc0` then 47 zero bytes,
/// the compressed point at infinity) as one member's key, that member's participation bit set.
///
/// The spec checks the signature with `eth_fast_aggregate_verify`, whose `FastAggregateVerify`
/// requires `KeyValidate` on every participant's key, and `KeyValidate` refuses the identity: the
/// update is invalid, whatever the signature. Helios adds the participants' keys (or, with more
/// than half signing, takes the committee's `aggregate_pubkey` field and subtracts the
/// non-participants', a16z/helios#751) and checks the signature against the sum; the identity
/// adds nothing, so a signature by the other participants verifies, and Helios accepts.
/// On a genuine chain the identity is not a validator's key (a deposit's proof of possession
/// cannot be made for it), so the committee here is fabricated: a minimal-preset chain at Electra
/// whose 32 keys the test holds, with member 5's key replaced. It does not involve LEZ.
#[test]
fn helios_871_the_identity_key_among_the_participants() {
    use ferrata_spectests::testkit::{fabricated_update, fake_committee, sign, Fabrication};
    let fake = fake_committee(32, 0x5a);
    let mut with_identity = fake.committee.clone();
    let mut identity = [0u8; 48];
    identity[0] = 0xc0;
    with_identity.pubkeys[5] = identity;
    // The aggregate field stays the sum of the keys (the identity adds nothing), so that Helios's
    // use of the field does not decide the input.
    with_identity.aggregate_pubkey = {
        use blst::min_pk::{AggregatePublicKey, PublicKey};
        let keys: Vec<PublicKey> = with_identity
            .pubkeys
            .iter()
            .filter(|k| **k != identity)
            .map(|k| PublicKey::from_bytes(k).unwrap())
            .collect();
        AggregatePublicKey::aggregate(&keys.iter().collect::<Vec<_>>(), false).unwrap().to_public_key().compress()
    };
    let chain = fabricated_chain(&with_identity);
    let ctx = chain.ctx();

    // Ferrata's policy store refuses such a committee before any update: at the bootstrap, under
    // either committee check (`KeyValidate` on every key; on chain, every key not the identity).
    for adoption in [Adoption::Full, Adoption::OnChain] {
        assert_eq!(
            FinalizedStore::initialize_with(
                &chain.trusted,
                &chain.bootstrap,
                LcFork::Electra,
                &chain.spec,
                &BLST,
                adoption
            )
            .unwrap_err(),
            Error::CommitteeInvalid,
            "{adoption:?}"
        );
    }
    // Helios's bootstrap takes it (the stores below unwrap `verify_bootstrap`), and so does the
    // spec's `initialize_light_client_store`, which checks only the branch.

    // All 32 bits set (member 5 among the participants); the signature is the aggregate of the
    // other 31 members' signatures over the update's signing root.
    for signers in [32usize, 16] {
        let mut u = fabricated_update(
            &Fabrication {
                finalized_slot: Some(72),
                attested_slot: 80,
                signature_slot: 81,
                signers,
                next_committee: None,
            },
            &fake,
            &ctx,
        );
        assert!(u.sync_aggregate.bits[5], "member 5 participates");
        let mut others = u.sync_aggregate.bits.clone();
        others[5] = false;
        u.sync_aggregate.signature =
            sign(&fake, &others, &ferrata_core::validate::sync_committee_signing_root(&u, &ctx));
        let (mut ours, mut theirs) = chain.stores();
        let finalized_before = helios_view(&theirs).finalized.0;
        let (ferrata, helios) = chain.submit(&mut ours, &mut theirs, &u, 82);
        assert_eq!((ferrata, helios), (Err(Error::PubkeyInvalid), true), "{signers} signers");
        // With a supermajority (32 of 32), Helios also finalizes the update's header; with 16 of
        // 32 it moves only its optimistic header.
        let after = helios_view(&theirs);
        assert_eq!(after.finalized.0, if signers == 32 { 72 } else { finalized_before }, "{signers} signers");
        assert_eq!(after.optimistic.0, 80, "{signers} signers");
        println!(
            "871, {signers} of 32 bits set, member 5 the identity: ferrata Err(PubkeyInvalid), Helios accepts \
             (finalized slot {finalized_before} -> {}, optimistic -> {})",
            after.finalized.0, after.optimistic.0
        );
    }

    // The same update with member 5's key genuine and its signature included: both accept.
    let chain_ok = fabricated_chain(&fake.committee);
    let ctx_ok = chain_ok.ctx();
    let u = fabricated_update(
        &Fabrication {
            finalized_slot: Some(72),
            attested_slot: 80,
            signature_slot: 81,
            signers: 32,
            next_committee: None,
        },
        &fake,
        &ctx_ok,
    );
    let (mut ours, mut theirs) = chain_ok.stores();
    assert_eq!(chain_ok.submit(&mut ours, &mut theirs, &u, 82), (Ok(()), true));
    assert_eq!(ferrata_view(&ours, &chain_ok.spec), helios_view(&theirs));
}

/// A minimal-preset chain at Electra from genesis whose bootstrap commits `committee` (as in
/// `helios_oracle.rs`).
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

    /// Submit to both; returns (ferrata spec store, Helios) verdicts.
    fn submit(
        &self,
        ours: &mut LightClientStore,
        theirs: &mut HStore<MinimalConsensusSpec>,
        update: &LightClientUpdate,
        slot: u64,
    ) -> (Result<(), Error>, bool) {
        let ferrata = ours.process_update(update, slot, &self.ctx());
        let h = helios_update::<MinimalConsensusSpec>(&encode::update(update)).unwrap();
        let helios = verify_generic_update(&h, slot, theirs, b256(&self.gvr), &forks(&self.spec)).is_ok();
        if helios {
            apply_generic_update(theirs, &h);
        }
        (ferrata, helios)
    }
}

/// SSZ encoding of Electra-layout light-client data (the inverse of ferrata's decoders), as in
/// `helios_oracle.rs`, which checks it re-encodes real bytes exactly.
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
