//! Regression tests for what the coverage-guided fuzzers of `fuzz/` found (fuzz/RESULTS.md):
//! each failed before its fix.

use ferrata_core::{
    fork::{ChainSpec, LcFork, Preset},
    store::{LightClientStore, StoreCore},
    types::{LightClientHeader, LightClientUpdate, SyncAggregate, SyncCommittee},
};

/// Finding L1 (fuzz target `lc`): `process_force_update` computed
/// `finalized_slot + UPDATE_TIMEOUT` in u64 and overflowed (a panic with overflow checks, a
/// wrapped deadline without them) when the finalized slot is within one sync-committee period
/// of 2^64. The spec computes with unbounded integers, so such a deadline is never reached and
/// the store must stay as it is.
#[test]
fn force_update_near_the_end_of_the_slot_range() {
    let spec = ChainSpec { preset: Preset::MINIMAL, versions: [[0; 4]; 9], epochs: [0; 9] };
    let n = spec.preset.sync_committee_size;
    let fork = LcFork::Altair;
    let mut finalized = LightClientHeader::empty(fork);
    finalized.beacon.slot = u64::MAX - 1;
    let mut attested = LightClientHeader::empty(fork);
    attested.beacon.slot = u64::MAX;
    let best = LightClientUpdate {
        attested_header: attested,
        next_sync_committee: SyncCommittee::empty(n),
        next_sync_committee_branch: vec![[0; 32]; 5],
        finalized_header: LightClientHeader::empty(fork),
        finality_branch: vec![[0; 32]; 6],
        sync_aggregate: SyncAggregate { bits: vec![true; n], signature: [0; 96] },
        signature_slot: u64::MAX,
    };
    let mut store = LightClientStore {
        core: StoreCore {
            fork,
            finalized_header: finalized.clone(),
            current_sync_committee: SyncCommittee::empty(n),
            next_sync_committee: SyncCommittee::empty(n),
        },
        best_valid_update: Some(best),
        optimistic_header: finalized,
        previous_max_active_participants: 0,
        current_max_active_participants: 0,
    };
    let before = store.clone();
    assert_eq!(store.process_force_update(u64::MAX, &spec), Ok(()));
    assert_eq!(store, before, "a deadline past 2^64 - 1 is never reached");
}
