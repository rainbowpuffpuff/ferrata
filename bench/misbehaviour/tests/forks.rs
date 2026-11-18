//! Written by an independent review of this crate (review-misbehaviour, 18 Nov 2026) and kept here as written. Needs
//! the vectors (`scripts/fetch-vectors.sh`).
//!
//! Honest forks before finality and a head reorg across a period boundary, modelled on a fabricated chain: the
//! orphaned branch's attested state holds what the beacon chain's transition gives it (the same finalized
//! checkpoint; the same next committee if it split after the period began, another one if it split before).
//! None of them may count as evidence.

use ferrata_core::{
    bls::{Aggregation, Blst},
    fork::{ChainSpec, LcFork},
    merkle::{FINALIZED_ROOT_GINDEX_ELECTRA, NEXT_SYNC_COMMITTEE_GINDEX_ELECTRA, ZERO},
    types::{LightClientHeader, LightClientUpdate, SyncAggregate, SyncCommittee},
    validate::{sync_committee_signing_root, Context},
    witness::{CommitteeSummary, CompactStore, CompactUpdate},
    Root,
};
use ferrata_spectests::{
    load_sync_case,
    testkit::{fabricated_header, fake_committee, sign, sparse_tree, FakeCommittee},
    vectors_dir,
};
use misbehaviour::{EvidenceError, Instance, Pairing, NO_CONTRADICTION};
use residue_agg::{NonSigners, Sum};

static BLS: Blst = Blst::new(Aggregation::Auto);

struct World {
    spec: ChainSpec,
    gvr: Root,
    base: u64,
    len: u64,
    current: FakeCommittee,
    next: FakeCommittee,
    rival: FakeCommittee,
}

fn world() -> World {
    let case = load_sync_case(&vectors_dir().join("electra/light_client/sync/pyspec_tests/light_client_sync")).unwrap();
    let spec = case.config.spec.clone();
    let len = spec.preset.slots_per_epoch * spec.preset.epochs_per_sync_committee_period;
    let base = (spec.period_at_slot(case.bootstrap.header.beacon.slot) + 2) * len;
    World {
        spec,
        gvr: case.genesis_validators_root,
        base,
        len,
        current: fake_committee(32, 0x41),
        next: fake_committee(32, 0x42),
        rival: fake_committee(32, 0x43),
    }
}

/// As `evidence.rs`'s `fabricate`, with `branch` changing the attested header (another block at that slot, on
/// another branch) and the attested state (`tag`).
#[allow(clippy::too_many_arguments)]
fn fabricate(
    w: &World,
    finalized: Option<&LightClientHeader>,
    attested_slot: u64,
    signature_slot: u64,
    bits: &[bool],
    next: Option<&SyncCommittee>,
    signer: &FakeCommittee,
    branch: u64,
) -> LightClientUpdate {
    let n = bits.len();
    let mut attested = fabricated_header(attested_slot, 80 + attested_slot + 1_000_000 * branch);
    let mut leaves = Vec::new();
    if let Some(h) = finalized {
        leaves.push((FINALIZED_ROOT_GINDEX_ELECTRA, h.beacon.hash_tree_root()));
    }
    if let Some(c) = next {
        leaves.push((NEXT_SYNC_COMMITTEE_GINDEX_ELECTRA, c.hash_tree_root()));
    }
    let (state_root, branches) = sparse_tree(&leaves, 7_000 + attested_slot + 1_000_000 * branch);
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

fn ns(c: &FakeCommittee, u: &LightClientUpdate) -> NonSigners {
    NonSigners::build(&c.committee, &u.sync_aggregate.bits).unwrap()
}

fn code<T>(r: &Result<T, EvidenceError>) -> Option<u16> {
    r.as_ref().err().map(|e| e.code())
}

/// An instance at the world's checkpoint, after one update that finalizes `base + 8` and adopts `w.next`.
fn advanced(w: &World) -> (Instance, LightClientHeader) {
    let checkpoint = fabricated_header(w.base, 9_000);
    let store = CompactStore {
        fork: LcFork::Electra,
        finalized: checkpoint.beacon.clone(),
        current: CommitteeSummary::of(&w.current.committee),
        next: None,
    };
    let mut i = Instance::new(store, &checkpoint.beacon, w.spec.clone(), w.gvr, None);
    let f1 = fabricated_header(w.base + 8, 40 + w.base + 8);
    let u1 = fabricate(w, Some(&f1), w.base + 16, w.base + 17, &first(30, 32), Some(&w.next.committee), &w.current, 0);
    let out = i.update(&CompactUpdate::of(&u1), LcFork::Electra, &ns(&w.current, &u1), &Pairing::FinalExp, u1.signature_slot).unwrap();
    assert!(out.advanced && out.adopted);
    (i, f1)
}

#[test]
fn honest_forks_and_reorgs_are_not_evidence() {
    let w = world();
    let mut rows: Vec<String> = Vec::new();
    let mut submit = |label: &str, i: &Instance, u: &LightClientUpdate, signer: &FakeCommittee, record: Option<u64>| {
        let mut j = i.clone();
        let r = j.evidence(&CompactUpdate::of(u), LcFork::Electra, &ns(signer, u), Sum::Batch, record, &Pairing::FinalExp, u.signature_slot);
        assert_eq!(code(&r), Some(NO_CONTRADICTION), "{label}: {r:?}");
        assert_eq!(&j, i, "{label}: the instance unchanged");
        rows.push(format!("| {label} | {} |", code(&r).unwrap()));
    };

    // 1. A fork before finality: two heads at the same slot (canonical, orphaned), both after the finalized header
    //    f1, so both states hold f1 as finalized and the same next committee. The orphaned head's update.
    let (i, f1) = advanced(&w);
    let canonical = fabricate(&w, Some(&f1), w.base + 40, w.base + 41, &first(30, 32), Some(&w.next.committee), &w.current, 0);
    let orphaned = fabricate(&w, Some(&f1), w.base + 40, w.base + 41, &first(30, 32), Some(&w.next.committee), &w.current, 1);
    assert_ne!(canonical.attested_header.beacon.hash_tree_root(), orphaned.attested_header.beacon.hash_tree_root());
    submit("orphaned head at the canonical head's slot, against f1's record", &i, &orphaned, &w.current, Some(f1.beacon.slot));
    submit("orphaned head at the canonical head's slot, against the adopted committee", &i, &orphaned, &w.current, None);

    // 2. The same fork, the orphaned head's state not yet aware of f1's finality (it finalizes the checkpoint):
    //    a finalized header below the store's, at a recorded slot, same root.
    let checkpoint = fabricated_header(w.base, 9_000);
    let lagging = fabricate(&w, Some(&checkpoint), w.base + 42, w.base + 43, &first(30, 32), Some(&w.next.committee), &w.current, 2);
    submit("orphaned head lagging finality (finalizes the checkpoint), against the checkpoint's record", &i, &lagging, &w.current, Some(w.base));
    submit("the same, against the adopted committee", &i, &lagging, &w.current, None);

    // 3. A head reorg across the period boundary: the branches split in the store's period before its last epoch,
    //    so the orphaned branch computed another committee for period + 2 at the boundary; its head in period + 1,
    //    finalized header still f1 (in the store's period), signed by the held next committee.
    let reorged = fabricate(&w, Some(&f1), w.base + w.len + 3, w.base + w.len + 4, &first(30, 32), Some(&w.rival.committee), &w.next, 3);
    submit("head reorg across the boundary (another committee for period + 2), against f1's record", &i, &reorged, &w.next, Some(f1.beacon.slot));
    submit("the same, against the adopted committee", &i, &reorged, &w.next, None);

    // 4. An update without a sync-committee branch (a finality update as the beacon API serves it), orphaned head.
    let bare = fabricate(&w, Some(&f1), w.base + 44, w.base + 45, &first(30, 32), None, &w.current, 4);
    submit("finality update without a next committee, orphaned head, against f1's record", &i, &bare, &w.current, Some(f1.beacon.slot));
    submit("the same, against the adopted committee", &i, &bare, &w.current, None);

    println!("| honest fork or reorg (fabricated chain) | refused |\n|---|---|\n{}", rows.join("\n"));
}
