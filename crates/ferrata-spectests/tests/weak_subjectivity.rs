//! The weak-subjectivity bound (`ferrata_core::ws`, `FinalizedStore::with_weak_subjectivity_period`).
//!
//! Spec-derived: the period against the tables of `consensus-specs` v1.7.0-beta.4
//! (`specs/phase0/weak-subjectivity.md`, `specs/electra/weak-subjectivity.md`) and the
//! `is_within_weak_subjectivity_period` predicate at its edges. Adversarial: a store past its
//! period refuses a supermajority-signed update with valid branches; the same update is accepted
//! by a store recovered from a newer trusted checkpoint (the documented recovery); the bound
//! changes nothing on the official `sync` vectors.

use ferrata_core::{
    bls::{Aggregation, Blst},
    fork::{ChainSpec, Preset},
    merkle::CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA,
    policy::FinalizedStore,
    types::LightClientBootstrap,
    validate::Context,
    ws::{
        check_within, compute_weak_subjectivity_period, compute_weak_subjectivity_period_phase0,
        get_balance_churn_limit, is_within_weak_subjectivity_period, weak_subjectivity_end_slot, WsConfig, ETH_TO_GWEI,
    },
    Error,
};
use ferrata_spectests::{
    cases, load_sync_case,
    testkit::{fabricated_header, fabricated_update, fake_committee, sparse_tree, Fabrication},
    vectors_dir,
};

static BLST: Blst = Blst::new(Aggregation::Auto);

/// The smallest period the spec's formulas give: `MIN_VALIDATOR_WITHDRAWABILITY_DELAY`.
const MIN_PERIOD: u64 = 256;

fn minimal_spec() -> ChainSpec {
    ChainSpec { preset: Preset::MINIMAL, versions: [[0; 4]; 9], epochs: [0; 9] }
}

// ---- The period, against the spec's tables ---------------------------------------------------

#[test]
fn electra_period_matches_the_spec_table() {
    // `specs/electra/weak-subjectivity.md`: SAFETY_DECAY 10, total active balance in ETH.
    let table = [
        (1_048_576u64, 665u64),
        (2_097_152, 1_075),
        (4_194_304, 1_894),
        (8_388_608, 3_532),
        (16_777_216, 3_532),
        (33_554_432, 3_532),
    ];
    for (eth, epochs) in table {
        assert_eq!(compute_weak_subjectivity_period(eth * ETH_TO_GWEI, &WsConfig::MAINNET), Some(epochs), "{eth} ETH");
    }
}

#[test]
fn phase0_period_matches_the_spec_table() {
    // `specs/phase0/weak-subjectivity.md`: SAFETY_DECAY 10, average balance (ETH), validators.
    let table = [
        (28u64, 32_768u64, 504u64),
        (28, 65_536, 752),
        (28, 131_072, 1_248),
        (28, 262_144, 2_241),
        (28, 524_288, 2_241),
        (28, 1_048_576, 2_241),
        (32, 32_768, 665),
        (32, 65_536, 1_075),
        (32, 131_072, 1_894),
        (32, 262_144, 3_532),
        (32, 524_288, 3_532),
        (32, 1_048_576, 3_532),
    ];
    for (avg, n, epochs) in table {
        let total = avg * ETH_TO_GWEI * n;
        assert_eq!(
            compute_weak_subjectivity_period_phase0(n, total, &WsConfig::MAINNET),
            Some(epochs),
            "{avg} ETH x {n}"
        );
    }
}

#[test]
fn phase0_period_in_the_branches_the_table_does_not_reach() {
    // The table's rows all take the validator-churn term. Worked by hand from the formula:
    // avg 27 ETH, N = 2^22: delta = 64, churn = N * 1280 // (600 * 64 * 86) = 1625, top-ups =
    // N * 230 // (600 * 512) = 3140, so the top-ups term decides: 256 + 3140.
    let n = 4_194_304u64;
    assert_eq!(compute_weak_subjectivity_period_phase0(n, 27 * ETH_TO_GWEI * n, &WsConfig::MAINNET), Some(3_396));
    // avg 23 ETH: t * 320 = T * 230 = 7360, so the second branch: 3 * N * 10 * 23 // (200 * 512 * 9).
    let n = 32_768u64;
    assert_eq!(compute_weak_subjectivity_period_phase0(n, 23 * ETH_TO_GWEI * n, &WsConfig::MAINNET), Some(256 + 24));
}

#[test]
fn balance_churn_limit_is_the_spec_formula() {
    let c = WsConfig::MAINNET;
    // max(MIN_PER_EPOCH_CHURN_LIMIT_ELECTRA, t // CHURN_LIMIT_QUOTIENT), down to a whole increment.
    assert_eq!(get_balance_churn_limit(1_000 * ETH_TO_GWEI, &c), Some(128 * ETH_TO_GWEI));
    assert_eq!(get_balance_churn_limit(34_000_000 * ETH_TO_GWEI, &c), Some(518 * ETH_TO_GWEI));
    assert_eq!(get_balance_churn_limit(65_536 * 300 * ETH_TO_GWEI + 65_535, &c), Some(300 * ETH_TO_GWEI));
    // The minimal configuration's values.
    assert_eq!(get_balance_churn_limit(32 * 100 * ETH_TO_GWEI, &WsConfig::MINIMAL), Some(100 * ETH_TO_GWEI));
    assert_eq!(get_balance_churn_limit(0, &WsConfig::MINIMAL), Some(64 * ETH_TO_GWEI));
}

#[test]
fn the_period_is_never_below_the_withdrawability_delay() {
    for eth in [0u64, 1, 32, 1_000, 1_048_575, 34_000_000, 120_000_000] {
        for c in [WsConfig::MAINNET, WsConfig::MINIMAL] {
            let p = compute_weak_subjectivity_period(eth * ETH_TO_GWEI, &c).unwrap();
            assert!(p >= c.min_validator_withdrawability_delay, "{eth} ETH: {p}");
        }
    }
}

#[test]
fn spec_arithmetic_failures_are_none() {
    // `SAFETY_DECAY * t` past uint64 raises in the spec.
    assert_eq!(compute_weak_subjectivity_period(u64::MAX, &WsConfig::MAINNET), None);
    // No active validators: phase 0 divides by N.
    assert_eq!(compute_weak_subjectivity_period_phase0(0, 0, &WsConfig::MAINNET), None);
    // A configuration with a zero quotient or increment.
    let bad = WsConfig { churn_limit_quotient: 0, ..WsConfig::MAINNET };
    assert_eq!(compute_weak_subjectivity_period(32 * ETH_TO_GWEI, &bad), None);
    let bad = WsConfig { effective_balance_increment: 0, ..WsConfig::MAINNET };
    assert_eq!(compute_weak_subjectivity_period(32 * ETH_TO_GWEI, &bad), None);
    // SAFETY_DECAY 0 with every balance at the maximum: the spec's `T - t` divisor is zero.
    let d0 = WsConfig { safety_decay: 0, ..WsConfig::MAINNET };
    assert_eq!(compute_weak_subjectivity_period_phase0(1, 32 * ETH_TO_GWEI, &d0), None);
}

// ---- The predicate ---------------------------------------------------------------------------

#[test]
fn within_the_period_is_current_epoch_at_most_ws_epoch_plus_period() {
    let spec = minimal_spec();
    // A header at slot 83 is in epoch 10; with a period of 5 the last epoch inside is 15.
    assert!(is_within_weak_subjectivity_period(83, 127, 5, &spec));
    assert!(!is_within_weak_subjectivity_period(83, 128, 5, &spec));
    assert_eq!(weak_subjectivity_end_slot(83, 5, &spec), Some(128));
    // The header's own epoch counts from its start: slot 80 and slot 87 end at the same slot.
    assert_eq!(weak_subjectivity_end_slot(80, 5, &spec), weak_subjectivity_end_slot(87, 5, &spec));
    // A current slot before the header is inside (nothing to refuse).
    assert!(is_within_weak_subjectivity_period(83, 0, 5, &spec));
    // Period 0: only the header's own epoch.
    assert!(is_within_weak_subjectivity_period(83, 87, 0, &spec));
    assert!(!is_within_weak_subjectivity_period(83, 88, 0, &spec));
    assert_eq!(check_within(83, 128, 5, &spec), Err(Error::OutsideWeakSubjectivityPeriod));
    assert_eq!(Error::OutsideWeakSubjectivityPeriod.code(), 53);
    assert_eq!(check_within(83, 127, 5, &spec), Ok(()));
}

#[test]
fn an_end_past_u64_is_never_reached() {
    let spec = minimal_spec();
    assert!(is_within_weak_subjectivity_period(u64::MAX, u64::MAX, u64::MAX, &spec));
    assert!(is_within_weak_subjectivity_period(0, u64::MAX, u64::MAX, &spec));
    assert_eq!(weak_subjectivity_end_slot(0, u64::MAX, &spec), None);
    // An end epoch inside u64 whose first slot is not: never reached either.
    let last_epoch = u64::MAX / 8;
    assert_eq!(weak_subjectivity_end_slot(0, last_epoch, &spec), None);
    assert!(is_within_weak_subjectivity_period(0, u64::MAX, last_epoch, &spec));
}

#[test]
fn the_end_slot_agrees_with_the_predicate() {
    let spec = minimal_spec();
    for ws_slot in [0u64, 1, 7, 8, 63, 64, 1_000] {
        for period in [0u64, 1, 3, 256] {
            let end = weak_subjectivity_end_slot(ws_slot, period, &spec).unwrap();
            for current in end.saturating_sub(20)..end + 20 {
                assert_eq!(
                    is_within_weak_subjectivity_period(ws_slot, current, period, &spec),
                    current < end,
                    "{ws_slot} {period} {current}"
                );
            }
        }
    }
}

// ---- The policy store on the official vectors ------------------------------------------------

#[test]
fn a_store_older_than_its_period_refuses_a_valid_update() {
    let case = load_sync_case(&vectors_dir().join("electra/light_client/sync/pyspec_tests/light_client_sync")).unwrap();
    let spec = &case.config.spec;
    let ctx = Context { spec, genesis_validators_root: case.genesis_validators_root, bls: &BLST };
    let store = FinalizedStore::initialize(&case.trusted_block_root, &case.bootstrap, case.store_fork, spec, &BLST)
        .unwrap()
        .with_weak_subjectivity_period(MIN_PERIOD);
    assert_eq!(store.weak_subjectivity_period(), Some(MIN_PERIOD));
    let (update, _) = case.updates[0].clone();
    let finalized = store.core().finalized_header.beacon.slot;
    let end = weak_subjectivity_end_slot(finalized, MIN_PERIOD, spec).unwrap();
    assert!(end > update.signature_slot);

    // The last slot inside: applied.
    let mut inside = store.clone();
    assert!(inside.is_within_weak_subjectivity_period(end - 1, spec));
    inside.process_update(&update, end - 1, &ctx).unwrap();
    assert!(inside.core().finalized_header.beacon.slot > finalized);

    // The first slot outside: refused, and the store is unchanged.
    let mut outside = store.clone();
    assert!(!outside.is_within_weak_subjectivity_period(end, spec));
    assert_eq!(outside.process_update(&update, end, &ctx), Err(Error::OutsideWeakSubjectivityPeriod));
    assert_eq!(outside, store);
    // Far later too, and however often it is tried.
    assert_eq!(outside.process_update(&update, end + 1_000_000, &ctx), Err(Error::OutsideWeakSubjectivityPeriod));
    assert_eq!(outside.process_update(&update, u64::MAX, &ctx), Err(Error::OutsideWeakSubjectivityPeriod));
    assert_eq!(outside, store);

    // The bound is the only reason: the same store without a period applies it at that slot.
    let mut unbounded =
        FinalizedStore::initialize(&case.trusted_block_root, &case.bootstrap, case.store_fork, spec, &BLST).unwrap();
    assert!(unbounded.is_within_weak_subjectivity_period(u64::MAX, spec));
    unbounded.process_update(&update, end, &ctx).unwrap();

    // The spec's own validation still runs first: an update the spec refuses keeps its code.
    let mut tampered = update.clone();
    tampered.sync_aggregate.signature[5] ^= 1;
    let refused = store.clone().process_update(&tampered, end, &ctx).unwrap_err();
    assert_ne!(refused, Error::OutsideWeakSubjectivityPeriod);
}

#[test]
fn the_period_moves_with_the_finalized_header() {
    let case = load_sync_case(&vectors_dir().join("electra/light_client/sync/pyspec_tests/light_client_sync")).unwrap();
    let spec = &case.config.spec;
    let ctx = Context { spec, genesis_validators_root: case.genesis_validators_root, bls: &BLST };
    let mut store = FinalizedStore::initialize(&case.trusted_block_root, &case.bootstrap, case.store_fork, spec, &BLST)
        .unwrap()
        .with_weak_subjectivity_period(MIN_PERIOD);
    let first_end = weak_subjectivity_end_slot(store.core().finalized_header.beacon.slot, MIN_PERIOD, spec).unwrap();
    let mut applied = 0;
    for (update, current_slot) in &case.updates {
        if store.process_update(update, *current_slot, &ctx).is_ok() {
            applied += 1;
        }
    }
    assert!(applied > 0);
    let last = store.core().finalized_header.beacon.slot;
    let end = weak_subjectivity_end_slot(last, MIN_PERIOD, spec).unwrap();
    assert!(end > first_end, "a store that advanced has a later end");
    assert!(store.is_within_weak_subjectivity_period(first_end, spec));
    assert!(!store.is_within_weak_subjectivity_period(end, spec));
}

#[test]
fn the_bound_changes_nothing_on_the_official_sync_vectors() {
    // Every `light_client_sync` case (one per fork, Altair to Gloas), step by step: a store with the
    // smallest period the spec gives agrees with a store without one, at the vectors' own clocks.
    let all: Vec<_> = cases("sync")
        .unwrap()
        .into_iter()
        .filter(|(_, d)| d.file_name().is_some_and(|n| n == "light_client_sync"))
        .collect();
    assert_eq!(all.len(), 7);
    for (fork, dir) in all {
        let case = load_sync_case(&dir).unwrap();
        let spec = &case.config.spec;
        let ctx = Context { spec, genesis_validators_root: case.genesis_validators_root, bls: &BLST };
        let mut plain =
            FinalizedStore::initialize(&case.trusted_block_root, &case.bootstrap, case.store_fork, spec, &BLST)
                .unwrap();
        let mut bounded = plain.clone().with_weak_subjectivity_period(MIN_PERIOD);
        let mut applied = 0;
        for (update, current_slot) in &case.updates {
            let a = plain.process_update(update, *current_slot, &ctx);
            let b = bounded.process_update(update, *current_slot, &ctx);
            assert_eq!(a, b, "{} at slot {current_slot}", fork.name());
            applied += usize::from(a.is_ok());
        }
        assert_eq!(plain.core(), bounded.core(), "{}", fork.name());
        assert!(applied > 0, "{}", fork.name());
    }
}

// ---- Recovery: a new bootstrap from a trusted checkpoint -------------------------------------

/// A bootstrap at `slot` whose state holds `committee` as its current sync committee (Electra
/// layout), and its trusted root.
fn fabricated_bootstrap(
    slot: u64,
    committee: &ferrata_core::types::SyncCommittee,
) -> (ferrata_core::Root, LightClientBootstrap) {
    let mut header = fabricated_header(slot, 500 + slot);
    let (state_root, branches) =
        sparse_tree(&[(CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA, committee.hash_tree_root())], 900 + slot);
    header.beacon.state_root = state_root;
    let root = header.beacon.hash_tree_root();
    let bootstrap = LightClientBootstrap {
        header,
        current_sync_committee: committee.clone(),
        current_sync_committee_branch: branches[0].clone(),
    };
    (root, bootstrap)
}

#[test]
fn a_store_past_its_period_is_recovered_by_a_new_bootstrap() {
    // The chain's configuration and fork schedule of the official Electra vectors (minimal preset:
    // 8 slots an epoch, 64 a period), a committee whose keys the test holds.
    let case = load_sync_case(&vectors_dir().join("electra/light_client/sync/pyspec_tests/light_client_sync")).unwrap();
    let spec = &case.config.spec;
    let ctx = Context { spec, genesis_validators_root: case.genesis_validators_root, bls: &BLST };
    let n = spec.preset.sync_committee_size;
    let fake = fake_committee(n, 61);

    // The stale store: checkpoint at slot 64 (epoch 8). The recovered one: slot 72 (epoch 9).
    let (root1, boot1) = fabricated_bootstrap(64, &fake.committee);
    let (root2, boot2) = fabricated_bootstrap(72, &fake.committee);
    let stale = FinalizedStore::initialize(&root1, &boot1, case.store_fork, spec, &BLST)
        .unwrap()
        .with_weak_subjectivity_period(MIN_PERIOD);
    let recovered = FinalizedStore::initialize(&root2, &boot2, case.store_fork, spec, &BLST)
        .unwrap()
        .with_weak_subjectivity_period(MIN_PERIOD);

    // A finality update of the same period, signed by every member: valid for both stores.
    let update = fabricated_update(
        &Fabrication {
            finalized_slot: Some(80),
            attested_slot: 88,
            signature_slot: 96,
            signers: n,
            next_committee: None,
        },
        &fake,
        &ctx,
    );
    // At epoch 8 + 256 + 1 = 265 the stale store is past its period; the recovered one is not.
    let now = (8 + MIN_PERIOD + 1) * spec.preset.slots_per_epoch;
    assert_eq!(weak_subjectivity_end_slot(64, MIN_PERIOD, spec), Some(now));

    let mut s = stale.clone();
    assert_eq!(s.process_update(&update, now, &ctx), Err(Error::OutsideWeakSubjectivityPeriod));
    assert_eq!(s, stale);
    // One slot earlier the stale store would still have taken it.
    stale.clone().process_update(&update, now - 1, &ctx).unwrap();

    let mut r = recovered.clone();
    r.process_update(&update, now, &ctx).unwrap();
    assert_eq!(r.core().finalized_header.beacon.slot, 80);
    // A bootstrap that does not match its trusted root is no recovery.
    let mut wrong = boot2.clone();
    wrong.header.beacon.proposer_index ^= 1;
    assert_eq!(
        FinalizedStore::initialize(&root2, &wrong, case.store_fork, spec, &BLST),
        Err(Error::CheckpointMismatch)
    );
}
