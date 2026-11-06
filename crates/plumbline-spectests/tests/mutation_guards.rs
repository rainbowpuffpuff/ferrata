//! Tests that pin individual checks: each fails if the check it names is removed or weakened.
//! They were written for mutants that survived the earlier suite in an independent review's
//! mutation run (IDs M01–M29 in the review). A few more cover Gloas and SSZ decoding, and one
//! documents the finalized-only store's liveness limit.

use plumbline_core::{
    bls::{Aggregation, BlsBackend, Blst, Zkcrypto},
    fork::{ChainSpec, LcFork, Preset, FAR_FUTURE_EPOCH},
    merkle::{CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA, EXECUTION_PAYLOAD_GINDEX, ZERO},
    policy::FinalizedStore,
    store::{LightClientStore, StoreCore},
    types::{
        BeaconBlockHeader, Execution, ExecutionPayloadHeader, LightClientBootstrap, LightClientHeader,
        LightClientUpdate, SyncAggregate,
    },
    upgrade::upgrade_header,
    validate::{get_lc_execution_root, is_better_update, is_valid_light_client_header, Context},
    Error,
};
use plumbline_spectests::{
    cases, load_sync_case, read_ssz_snappy, run_sync_case,
    testkit::{fabricated_header, fabricated_update, fake_committee, sparse_tree, with_wrong_aggregate, Fabrication},
    vectors_dir, LoadedCase,
};

static BLST: Blst = Blst::new(Aggregation::Auto);

fn electra() -> LoadedCase {
    load_sync_case(&vectors_dir().join("electra/light_client/sync/pyspec_tests/light_client_sync")).unwrap()
}

fn ctx(case: &LoadedCase) -> Context<'_> {
    Context { spec: &case.config.spec, genesis_validators_root: case.genesis_validators_root, bls: &BLST }
}

fn policy(case: &LoadedCase) -> FinalizedStore {
    FinalizedStore::initialize(&case.trusted_block_root, &case.bootstrap, case.store_fork, &case.config.spec, &BLST)
        .unwrap()
}

fn spec_store(case: &LoadedCase) -> LightClientStore {
    LightClientStore::initialize(&case.trusted_block_root, &case.bootstrap, case.store_fork, &case.config.spec).unwrap()
}

/// Slots of the case's first update, for fabricated updates that pass the period checks.
fn slots(case: &LoadedCase) -> (u64, u64, u64, u64) {
    let (u, current) = &case.updates[0];
    (u.finalized_header.beacon.slot, u.attested_header.beacon.slot, u.signature_slot, *current)
}

// ---- Committee adoption (M01, M02) --------------------------------------------------------

#[test]
fn policy_checks_the_bootstrap_committee() {
    let case = electra();
    let spec = &case.config.spec;
    let inconsistent = with_wrong_aggregate(&fake_committee(32, 0x11).committee);
    let mut header = fabricated_header(16, 500);
    let (state_root, branches) =
        sparse_tree(&[(CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA, inconsistent.hash_tree_root())], 9);
    header.beacon.state_root = state_root;
    let bootstrap = LightClientBootstrap {
        header: header.clone(),
        current_sync_committee: inconsistent,
        current_sync_committee_branch: branches[0].clone(),
    };
    let trusted = header.beacon.hash_tree_root();
    assert!(LightClientStore::initialize(&trusted, &bootstrap, LcFork::Electra, spec).is_ok(), "spec store");
    assert_eq!(
        FinalizedStore::initialize(&trusted, &bootstrap, LcFork::Electra, spec, &BLST).err(),
        Some(Error::CommitteeInvalid)
    );
}

#[test]
fn policy_checks_a_committee_at_adoption() {
    let case = electra();
    let (finalized, attested, signature, current) = slots(&case);
    let fake = fake_committee(32, 0x22);
    let next = fake_committee(32, 0x23);
    let core = StoreCore { current_sync_committee: fake.committee.clone(), ..policy(&case).core().clone() };
    let trusting = FinalizedStore::from_core(core, &BLST).unwrap();
    let fabricate = |committee| Fabrication {
        finalized_slot: Some(finalized),
        attested_slot: attested,
        signature_slot: signature,
        signers: 32,
        next_committee: Some(committee),
    };
    let inconsistent = with_wrong_aggregate(&next.committee);
    let bad = fabricated_update(&fabricate(&inconsistent), &fake, &ctx(&case));
    let mut store = trusting.clone();
    assert_eq!(store.process_update(&bad, current, &ctx(&case)), Err(Error::CommitteeInvalid));
    assert_eq!(store, trusting, "a refused adoption changed the store");
    let good = fabricated_update(&fabricate(&next.committee), &fake, &ctx(&case));
    let outcome = trusting.clone().process_update(&good, current, &ctx(&case)).unwrap();
    assert!(outcome.adopted);
}

// ---- Next-committee handling (M03, M15, M28, M29) -----------------------------------------

#[test]
fn conflicting_next_committee_is_refused() {
    let case = electra();
    let (u, current) = case.updates[0].clone();
    let mut store = policy(&case);
    store.process_update(&u, current, &ctx(&case)).unwrap();
    let mut conflicting = u.clone();
    conflicting.next_sync_committee.pubkeys[3][7] ^= 1;
    assert_eq!(store.clone().process_update(&conflicting, current, &ctx(&case)), Err(Error::NextCommitteeMismatch));
}

#[test]
fn committee_without_a_branch_is_refused() {
    // A non-sync-committee update (zero branch) must carry the empty committee. Otherwise
    // `apply_core` would adopt whatever committee is attached.
    let case = electra();
    let (mut u, current) = case.updates[0].clone();
    u.next_sync_committee_branch = vec![ZERO; u.next_sync_committee_branch.len()];
    assert_eq!(policy(&case).process_update(&u, current, &ctx(&case)), Err(Error::NextCommitteeUnexpected));
    assert_eq!(spec_store(&case).process_update(&u, current, &ctx(&case)), Err(Error::NextCommitteeUnexpected));
}

#[test]
fn stale_update_is_irrelevant_once_the_next_committee_is_known() {
    let case = electra();
    let (_, _, signature, current) = slots(&case);
    let fake = fake_committee(32, 0x44);
    let next = fake_committee(32, 0x45);
    let mut store = spec_store(&case);
    store.core.current_sync_committee = fake.committee.clone();
    let at = store.core.finalized_header.beacon.slot;
    // Next committee unknown, no committee supplied, not newer: irrelevant (M17).
    let plain = Fabrication {
        finalized_slot: Some(at),
        attested_slot: at,
        signature_slot: signature,
        signers: 32,
        next_committee: None,
    };
    let update = fabricated_update(&plain, &fake, &ctx(&case));
    assert_eq!(store.clone().process_update(&update, current, &ctx(&case)), Err(Error::UpdateIrrelevant));
    // Next committee known: supplying it again does not make a stale update relevant (M28).
    store.core.next_sync_committee = next.committee.clone();
    let supplying = Fabrication { next_committee: Some(&next.committee), ..plain };
    let update = fabricated_update(&supplying, &fake, &ctx(&case));
    assert_eq!(store.clone().process_update(&update, current, &ctx(&case)), Err(Error::UpdateIrrelevant));
}

#[test]
fn supplying_the_missing_committee_from_a_past_update_is_applied() {
    let supply: Vec<_> = cases("sync")
        .unwrap()
        .into_iter()
        .filter(|(_, d)| d.ends_with("supply_sync_committee_from_past_update"))
        .collect();
    assert_eq!(supply.len(), 7);
    for (fork, dir) in supply {
        let out = run_sync_case(&dir, &BLST).unwrap();
        assert!(out.final_next_committee_known, "{}: next committee not learned", fork.name());
    }
}

// ---- Slots, periods and finality (M04, M13, M14, M16) -------------------------------------

#[test]
fn attested_slot_may_equal_the_finalized_slot() {
    let case = electra();
    let (_, attested, signature, current) = slots(&case);
    let fake = fake_committee(32, 0x33);
    let core = StoreCore { current_sync_committee: fake.committee.clone(), ..policy(&case).core().clone() };
    let trusting = FinalizedStore::from_core(core, &BLST).unwrap();
    let update = fabricated_update(
        &Fabrication {
            finalized_slot: Some(attested),
            attested_slot: attested,
            signature_slot: signature,
            signers: 32,
            next_committee: None,
        },
        &fake,
        &ctx(&case),
    );
    assert!(trusting.clone().process_update(&update, current, &ctx(&case)).is_ok());
}

#[test]
fn finality_at_genesis_needs_the_empty_header() {
    let case = electra();
    let (_, attested, signature, current) = slots(&case);
    let fake = fake_committee(32, 0x55);
    let mut store = spec_store(&case);
    store.core.current_sync_committee = fake.committee.clone();
    let update = fabricated_update(
        &Fabrication {
            finalized_slot: Some(0),
            attested_slot: attested,
            signature_slot: signature,
            signers: 32,
            next_committee: None,
        },
        &fake,
        &ctx(&case),
    );
    assert_eq!(store.process_update(&update, current, &ctx(&case)), Err(Error::FinalizedHeaderInvalid));
}

#[test]
fn update_without_finality_must_not_carry_a_finalized_header() {
    let case = electra();
    let (_, attested, signature, current) = slots(&case);
    let fake = fake_committee(32, 0x66);
    let mut store = spec_store(&case);
    store.core.current_sync_committee = fake.committee.clone();
    let mut update = fabricated_update(
        &Fabrication {
            finalized_slot: None,
            attested_slot: attested,
            signature_slot: signature,
            signers: 32,
            next_committee: None,
        },
        &fake,
        &ctx(&case),
    );
    // The signature covers the attested header only, so this stays validly signed.
    update.finalized_header = fabricated_header(8, 600);
    assert_eq!(store.process_update(&update, current, &ctx(&case)), Err(Error::FinalizedHeaderInvalid));
}

#[test]
fn update_signed_two_periods_ahead_is_refused() {
    let case = electra();
    let spec = &case.config.spec;
    let (u, current) = case.updates.iter().find(|(u, _)| spec.period_at_slot(u.signature_slot) == 1).cloned().unwrap();
    assert_eq!(policy(&case).process_update(&u, current, &ctx(&case)), Err(Error::PeriodSkipped));
    assert_eq!(spec_store(&case).process_update(&u, current, &ctx(&case)), Err(Error::PeriodSkipped));
}

// ---- Ranking and the optimistic header (M19, M23) -----------------------------------------

#[test]
fn ranking_prefers_older_attested_data_on_a_tie() {
    let case = electra();
    let (finalized, attested, signature, _) = slots(&case);
    let fake = fake_committee(32, 0x77);
    let make = |attested_slot| {
        fabricated_update(
            &Fabrication {
                finalized_slot: Some(finalized),
                attested_slot,
                signature_slot: signature,
                signers: 30,
                next_committee: None,
            },
            &fake,
            &ctx(&case),
        )
    };
    let (older, newer) = (make(attested - 1), make(attested));
    assert!(is_better_update(&older, &newer, &case.config.spec));
    assert!(!is_better_update(&newer, &older, &case.config.spec));
}

#[test]
fn optimistic_header_needs_more_than_the_safety_threshold() {
    let case = electra();
    let (finalized, attested, signature, current) = slots(&case);
    let fake = fake_committee(32, 0x88);
    let mut store = spec_store(&case);
    store.core.current_sync_committee = fake.committee.clone();
    let full = Fabrication {
        finalized_slot: Some(finalized),
        attested_slot: attested,
        signature_slot: signature,
        signers: 32,
        next_committee: None,
    };
    store.process_update(&fabricated_update(&full, &fake, &ctx(&case)), current, &ctx(&case)).unwrap();
    let optimistic = store.optimistic_header.beacon.slot;
    assert_eq!(store.safety_threshold(), 16);
    // 10 signers is not more than the threshold: the optimistic header must not move.
    let weak = Fabrication {
        finalized_slot: None,
        attested_slot: attested + 2,
        signature_slot: signature + 2,
        signers: 10,
        next_committee: None,
    };
    store.process_update(&fabricated_update(&weak, &fake, &ctx(&case)), current + 2, &ctx(&case)).unwrap();
    assert_eq!(store.optimistic_header.beacon.slot, optimistic);
}

// ---- Header validity across forks (M10, M11, M12, M20) ------------------------------------

/// Minimal preset with Capella at epoch 1, Deneb and Electra at 2, Gloas at 3.
fn staggered() -> ChainSpec {
    ChainSpec {
        preset: Preset::MINIMAL,
        versions: [
            [0, 0, 0, 1],
            [1, 0, 0, 1],
            [2, 0, 0, 1],
            [3, 0, 0, 1],
            [4, 0, 0, 1],
            [5, 0, 0, 1],
            [6, 0, 0, 1],
            [7, 0, 0, 1],
            [8, 0, 0, 1],
        ],
        epochs: [0, 0, 0, 1, 2, 2, 2, 3, FAR_FUTURE_EPOCH],
    }
}

fn payload_header_at(slot: u64, payload: ExecutionPayloadHeader, root: [u8; 32]) -> LightClientHeader {
    let (body_root, branches) = sparse_tree(&[(EXECUTION_PAYLOAD_GINDEX, root)], slot);
    LightClientHeader {
        beacon: BeaconBlockHeader { slot, proposer_index: 1, parent_root: [1; 32], state_root: [2; 32], body_root },
        execution: Execution::Payload { header: payload, branch: branches[0].clone() },
    }
}

#[test]
fn blob_gas_before_deneb_is_invalid() {
    let spec = staggered();
    let slot = 8; // epoch 1: Capella, before Deneb
    let mut payload = plumbline_spectests::testkit::payload_header(slot, 1);
    let ok = payload_header_at(slot, payload.clone(), payload.hash_tree_root_capella());
    assert!(is_valid_light_client_header(&ok, LcFork::Deneb, &spec));
    payload.blob_gas_used = 1;
    let bad = payload_header_at(slot, payload.clone(), payload.hash_tree_root_capella());
    assert!(!is_valid_light_client_header(&bad, LcFork::Deneb, &spec));
}

#[test]
fn execution_data_before_capella_must_be_empty() {
    let spec = staggered();
    let empty = LightClientHeader::empty(LcFork::Deneb);
    let mut at_genesis = empty.clone();
    at_genesis.beacon.proposer_index = 3;
    assert!(is_valid_light_client_header(&at_genesis, LcFork::Deneb, &spec));
    let mut with_payload = at_genesis.clone();
    let Execution::Payload { header, .. } = &mut with_payload.execution else { unreachable!() };
    header.block_number = 1;
    assert!(!is_valid_light_client_header(&with_payload, LcFork::Deneb, &spec));

    let mut gloas = LightClientHeader::empty(LcFork::Gloas);
    gloas.beacon.proposer_index = 3;
    assert!(is_valid_light_client_header(&gloas, LcFork::Gloas, &spec));
    let Execution::BlockHash { block_hash, .. } = &mut gloas.execution else { unreachable!() };
    block_hash[0] = 1;
    assert!(!is_valid_light_client_header(&gloas, LcFork::Gloas, &spec));
}

#[test]
fn gloas_store_reports_the_empty_payload_root_at_genesis() {
    let mut spec = staggered();
    spec.epochs = [0, 0, 0, 0, 0, 0, 0, 3, FAR_FUTURE_EPOCH];
    let header = LightClientHeader {
        beacon: BeaconBlockHeader::default(),
        execution: Execution::BlockHash { block_hash: [9; 32], branch: vec![ZERO; 11] },
    };
    assert_eq!(
        get_lc_execution_root(&header, LcFork::Gloas, &spec),
        ExecutionPayloadHeader::empty().hash_tree_root_deneb()
    );
}

// ---- Gloas binding ------------------------------------------------------------------------

#[test]
fn gloas_block_hash_and_branch_are_checked() {
    let case = load_sync_case(&vectors_dir().join("gloas/light_client/sync/pyspec_tests/light_client_sync")).unwrap();
    let (u, current) = case.updates[0].clone();
    let store = policy(&case);
    assert!(store.clone().process_update(&u, current, &ctx(&case)).is_ok());
    let tamper = |f: &dyn Fn(&mut LightClientUpdate)| {
        let mut t = u.clone();
        f(&mut t);
        store.clone().process_update(&t, current, &ctx(&case))
    };
    let attested_hash = |t: &mut LightClientUpdate| match &mut t.attested_header.execution {
        Execution::BlockHash { block_hash, .. } => block_hash[0] ^= 1,
        _ => panic!("gloas layout"),
    };
    assert_eq!(tamper(&attested_hash), Err(Error::AttestedHeaderInvalid));
    for i in 0..11 {
        let entry = move |t: &mut LightClientUpdate| match &mut t.attested_header.execution {
            Execution::BlockHash { branch, .. } => branch[i][5] ^= 1,
            _ => panic!("gloas layout"),
        };
        assert_eq!(tamper(&entry), Err(Error::AttestedHeaderInvalid), "branch entry {i}");
    }
    let finalized_hash = |t: &mut LightClientUpdate| match &mut t.finalized_header.execution {
        Execution::BlockHash { block_hash, .. } => block_hash[31] ^= 1,
        _ => panic!("gloas layout"),
    };
    assert_eq!(tamper(&finalized_hash), Err(Error::FinalizedHeaderInvalid));
}

#[test]
fn legacy_headers_in_a_gloas_store_keep_zero_padding() {
    let case = load_sync_case(&vectors_dir().join("fulu/light_client/sync/pyspec_tests/gloas_fork")).unwrap();
    let spec = &case.config.spec;
    let (u, _) = &case.updates[0];
    let header = upgrade_header(u.attested_header.clone(), LcFork::Electra, LcFork::Gloas, spec).unwrap();
    assert!(is_valid_light_client_header(&header, LcFork::Gloas, spec));
    let Execution::BlockHash { branch, .. } = &header.execution else { panic!("gloas layout") };
    assert_eq!(&branch[..2], &[ZERO, ZERO], "a Deneb-era proof is two levels shorter than Gloas's");
    for i in 0..2 {
        let mut t = header.clone();
        let Execution::BlockHash { branch, .. } = &mut t.execution else { unreachable!() };
        branch[i][0] = 1;
        assert!(!is_valid_light_client_header(&t, LcFork::Gloas, spec), "padding entry {i}");
    }
}

// ---- Signatures outside G2 (M07) ----------------------------------------------------------

#[test]
fn signature_outside_g2_is_malformed() {
    // A compressed G2 point on the curve but outside the subgroup: x = (0, k) for small k.
    let point = (1u8..=255)
        .map(|k| {
            let mut s = [0u8; 96];
            s[0] = 0x80;
            s[95] = k;
            s
        })
        .find(|s| blst::min_pk::Signature::uncompress(s).is_ok_and(|sig| sig.validate(true).is_err()))
        .unwrap();
    let fake = fake_committee(4, 0x99);
    let bits = [true; 4];
    for bls in [&BLST as &dyn BlsBackend, &Zkcrypto::new(Aggregation::Auto)] {
        assert_eq!(bls.fast_aggregate_verify(&fake.committee, &bits, &[1; 32], &point), Err(Error::SignatureMalformed));
        assert_eq!(bls.verify_checked(&fake.committee, &bits, &[1; 32], &point), Err(Error::SignatureMalformed));
    }
}

// ---- SSZ decoding (M05, M06) --------------------------------------------------------------

#[test]
fn ssz_decoding_is_strict() {
    // ExecutionPayloadHeader (Deneb): 584 fixed bytes, then extra_data, at most 32 bytes.
    let encode = |extra: usize| {
        let mut b = vec![0u8; 584];
        b[436..440].copy_from_slice(&584u32.to_le_bytes());
        b.extend(std::iter::repeat(7u8).take(extra));
        b
    };
    assert!(ExecutionPayloadHeader::decode(&encode(32), true).is_ok());
    assert_eq!(ExecutionPayloadHeader::decode(&encode(33), true), Err(Error::Malformed));

    // Bitvector[7] in one byte: the unused top bit must be zero.
    let mut agg = vec![0b0111_1111u8];
    agg.extend([0u8; 96]);
    assert!(SyncAggregate::decode(&agg, 7).is_ok());
    agg[0] = 0b1111_1111;
    assert_eq!(SyncAggregate::decode(&agg, 7), Err(Error::Malformed));

    // A real Electra update: truncation, a trailing byte, and bad offsets are all refused.
    let dir = vectors_dir().join("electra/light_client/sync/pyspec_tests/light_client_sync");
    let name = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.file_name().unwrap().to_string_lossy().starts_with("update_"));
    let bytes = read_ssz_snappy(&name.unwrap()).unwrap();
    assert!(LightClientUpdate::decode(&bytes, LcFork::Electra, 32).is_ok());
    assert_eq!(LightClientUpdate::decode(&bytes[..bytes.len() - 1], LcFork::Electra, 32), Err(Error::Malformed));
    // A trailing byte lands in the last variable-size field (the finalized payload's
    // extra_data), so it decodes to a different value; it is not a second encoding of this one.
    let mut longer = bytes.clone();
    longer.push(0);
    let other = LightClientUpdate::decode(&longer, LcFork::Electra, 32).unwrap();
    assert_ne!(other, LightClientUpdate::decode(&bytes, LcFork::Electra, 32).unwrap());
    // Fixed-size containers must be consumed exactly.
    assert_eq!(BeaconBlockHeader::decode(&[0u8; 113]), Err(Error::Malformed));
    assert_eq!(BeaconBlockHeader::decode(&[0u8; 111]), Err(Error::Malformed));
    let first = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    for delta in [-1i64, 1] {
        let mut b = bytes.clone();
        b[0..4].copy_from_slice(&((first as i64 + delta) as u32).to_le_bytes());
        assert_eq!(LightClientUpdate::decode(&b, LcFork::Electra, 32), Err(Error::Malformed), "offset {delta:+}");
    }
    // The finalized header's offset sits after the attested offset, committee and branch.
    let at = 4 + 33 * 48 + 6 * 32;
    let mut crossed = bytes.clone();
    crossed[at..at + 4].copy_from_slice(&(first - 1).to_le_bytes());
    assert_eq!(LightClientUpdate::decode(&crossed, LcFork::Electra, 32), Err(Error::Malformed));
}

// ---- Liveness limit of the finalized-only policy -------------------------------------------

#[test]
fn stops_safely_after_a_period_without_finality() {
    // `light_client_sync` runs through a stretch without finality that the spec store crosses
    // only by forcing a non-finalized header. The policy store never forces, so it stops at the
    // last finalized handoff and refuses what follows; it never holds a header the spec store
    // did not finalize (asserted inside `run_sync_case` at every step).
    let all: Vec<_> = cases("sync")
        .unwrap()
        .into_iter()
        .filter(|(_, d)| d.file_name().is_some_and(|n| n == "light_client_sync"))
        .collect();
    assert_eq!(all.len(), 7);
    for (fork, dir) in all {
        let out = run_sync_case(&dir, &BLST).unwrap();
        assert!(out.forced_divergence, "{}", fork.name());
        assert_eq!(out.policy_final_slot, 96, "{}", fork.name());
        let allowed = [
            Error::PeriodSkipped,
            Error::NotFinalized,
            Error::BelowSupermajority,
            Error::NotNewer,
            Error::UpdateIrrelevant,
        ]
        .map(Error::code);
        assert!(
            out.policy_refused_after_divergence.keys().all(|c| allowed.contains(c)),
            "{}: {:?}",
            fork.name(),
            out.policy_refused_after_divergence
        );
    }
}

// ---- Bootstrap ------------------------------------------------------------------------------

#[test]
fn bootstrap_must_match_the_checkpoint_and_its_branch() {
    let case = electra();
    let spec = &case.config.spec;
    let init = |root: &[u8; 32], b: &LightClientBootstrap| {
        (
            FinalizedStore::initialize(root, b, LcFork::Electra, spec, &BLST).err(),
            LightClientStore::initialize(root, b, LcFork::Electra, spec).err(),
        )
    };
    let mut wrong_root = case.trusted_block_root;
    wrong_root[0] ^= 1;
    assert_eq!(init(&wrong_root, &case.bootstrap), (Some(Error::CheckpointMismatch), Some(Error::CheckpointMismatch)));
    let mut b = case.bootstrap.clone();
    b.current_sync_committee_branch[2][0] ^= 1;
    let e = Some(Error::CurrentCommitteeBranchInvalid);
    assert_eq!(init(&case.trusted_block_root, &b), (e, e));
    let mut b = case.bootstrap.clone();
    let Execution::Payload { header, .. } = &mut b.header.execution else { panic!("electra layout") };
    header.gas_used += 1;
    let e = Some(Error::BootstrapHeaderInvalid);
    assert_eq!(init(&case.trusted_block_root, &b), (e, e));
}

// ---- On-chain committee adoption ----------------------------------------------------------

#[test]
fn onchain_adoption_skips_only_the_subgroup_check() {
    use blst::min_pk::PublicKey;
    let fake = fake_committee(4, 0xab);
    let mut keys = fake.committee.pubkeys.clone();
    // A curve point outside the prime-order subgroup, with x != 0.
    let outside = (1u8..=255)
        .map(|x| {
            let mut k = [0u8; 48];
            k[0] = 0x80;
            k[47] = x;
            k
        })
        .find(|k| PublicKey::uncompress(k).is_ok_and(|pk| pk.validate().is_err()))
        .unwrap();
    let mut identity = [0u8; 48];
    identity[0] = 0xc0;
    for bls in [&BLST as &dyn BlsBackend, &Zkcrypto::new(Aggregation::Auto)] {
        assert_eq!(bls.check_committee(&fake.committee), Ok(()));
        assert_eq!(bls.check_committee_onchain(&fake.committee), Ok(()));
        // Identity seat: both refuse.
        keys[1] = identity;
        let c = plumbline_core::types::SyncCommittee {
            pubkeys: keys.clone(),
            aggregate_pubkey: fake.committee.aggregate_pubkey,
        };
        assert_eq!(bls.check_committee_onchain(&c), Err(Error::CommitteeInvalid));
        // Wrong sum: both refuse.
        keys = fake.committee.pubkeys.clone();
        let c = with_wrong_aggregate(&fake.committee);
        assert_eq!(bls.check_committee_onchain(&c), Err(Error::CommitteeInvalid));
        // Outside the subgroup, with the matching aggregate: only the full check refuses. The
        // program relies on every key of a genuine beacon state having passed `KeyValidate` at
        // deposit, and `verify_checkpoint` and `monitor` run the full check off chain.
        let mut odd = fake.committee.clone();
        odd.pubkeys[2] = outside;
        odd.aggregate_pubkey = sum_of(&odd.pubkeys);
        assert_eq!(bls.check_committee(&odd), Err(Error::CommitteeInvalid));
        assert_eq!(bls.check_committee_onchain(&odd), Ok(()));
    }
}

#[test]
fn store_adoption_mode_decides_the_check() {
    use blst::min_pk::PublicKey;
    use plumbline_core::policy::Adoption;
    let case = electra();
    let spec = &case.config.spec;
    let fake = fake_committee(32, 0xcd);
    let outside = (1u8..=255)
        .map(|x| {
            let mut k = [0u8; 48];
            k[0] = 0x80;
            k[47] = x;
            k
        })
        .find(|k| PublicKey::uncompress(k).is_ok_and(|pk| pk.validate().is_err()))
        .unwrap();
    let mut odd = fake.committee.clone();
    odd.pubkeys[7] = outside;
    odd.aggregate_pubkey = sum_of(&odd.pubkeys);
    let mut header = fabricated_header(16, 700);
    let (state_root, branches) = sparse_tree(&[(CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA, odd.hash_tree_root())], 11);
    header.beacon.state_root = state_root;
    let bootstrap = LightClientBootstrap {
        header: header.clone(),
        current_sync_committee: odd,
        current_sync_committee_branch: branches[0].clone(),
    };
    let trusted = header.beacon.hash_tree_root();
    let init = |adoption| FinalizedStore::initialize_with(&trusted, &bootstrap, LcFork::Electra, spec, &BLST, adoption);
    assert_eq!(init(Adoption::Full).err(), Some(Error::CommitteeInvalid));
    assert!(init(Adoption::OnChain).is_ok());
}

/// Sum of compressed keys that are curve points (not necessarily in G1), compressed.
fn sum_of(keys: &[[u8; 48]]) -> [u8; 48] {
    use bls12_381::{G1Affine, G1Projective};
    let total = keys.iter().fold(G1Projective::identity(), |acc, k| {
        acc + Option::<G1Affine>::from(G1Affine::from_compressed_unchecked(k)).unwrap()
    });
    G1Affine::from(total).to_compressed()
}
