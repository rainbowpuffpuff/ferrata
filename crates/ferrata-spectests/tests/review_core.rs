//! Tests written by the independent review of `docs/REVIEW-core.md` (11 November 2026). Each pins
//! what the code does where a row of that table describes it differently, so the owner can see the
//! behaviour the row should state. They pass on the reviewed code.

use ferrata_core::{
    attest::{attest_batch, AccountQuery, Batch, Cmp, Field, LogQuery, Operand, Relation},
    bls::Blst,
    Error,
};
use ferrata_spectests::{
    execkit::World,
    testkit::{fake_committee, sign},
};

/// The BLS12-381 base-field modulus, big-endian.
const P: [u8; 48] = [
    0x1a, 0x01, 0x11, 0xea, 0x39, 0x7f, 0xe6, 0x9a, 0x4b, 0x1b, 0xa7, 0xb6, 0x43, 0x4b, 0xac, 0xd7, 0x64, 0x77, 0x4b,
    0x84, 0xf3, 0x85, 0x12, 0xbf, 0x67, 0x30, 0xd2, 0xa0, 0xf6, 0xb0, 0xf6, 0x24, 0x1e, 0xab, 0xff, 0xfe, 0xb1, 0x53,
    0xff, 0xff, 0xb9, 0xfe, 0xff, 0xff, 0xff, 0xff, 0xaa, 0xab,
];

/// `c + P` in 48 big-endian bytes (no overflow for `c < P`, since `2P < 2^384`).
fn plus_p(c: &[u8]) -> [u8; 48] {
    let mut out = [0u8; 48];
    let mut carry = 0u16;
    for i in (0..48).rev() {
        let s = u16::from(c[i]) + u16::from(P[i]) + carry;
        out[i] = s as u8;
        carry = s >> 8;
    }
    assert_eq!(carry, 0);
    out
}

/// Row "`Blst::miller_half`, `Blst::final_half`": the row says "canonical big-endian Fp12
/// encoding". `miller_half` writes canonical coefficients, but `final_half` reads any 48-byte
/// value modulo p: a product with a coefficient replaced by `c + p` is a second encoding of the
/// same element, and `final_half` accepts it. The canonicity check is the caller's
/// (`steps::Job::step3` runs `canonical_fp12` first), not `final_half`'s.
#[test]
fn final_half_reads_coefficients_modulo_p() {
    let fake = fake_committee(32, 0x5a);
    let bits = vec![true; 32];
    let message = [7u8; 32];
    let signature = sign(&fake, &bits, &message);
    let product = Blst::default().miller_half(&fake.committee, &bits, &message, &signature).unwrap();
    assert_eq!(Blst::final_half(&product), Ok(()));
    for k in 0..12 {
        let mut second = product;
        second[48 * k..48 * k + 48].copy_from_slice(&plus_p(&product[48 * k..48 * k + 48]));
        assert_ne!(second, product);
        assert_eq!(Blst::final_half(&second), Ok(()), "coefficient {k} + p is accepted by final_half");
    }
    // A different element is still refused.
    let mut other = product;
    other[47] ^= 1;
    assert_eq!(Blst::final_half(&other), Err(Error::SignatureInvalid));
}

/// Row "`Relation`, `Operand` in `attest_batch`": the row (and `attest_batch`'s documentation)
/// says a relation naming a fact or field that is not there is `QueryInvalid`. A topic or data word
/// that a log does not have is `PredicateFailed` instead (`Fact::field`'s own documentation says
/// so); only a missing fact, or a field of another kind of fact, is `QueryInvalid`.
#[test]
fn a_relation_on_a_missing_log_topic_or_data_word_is_predicate_failed() {
    let world = World::new(2);
    let record = world.record(1, 600);
    // Receipt 3 holds three logs, each with three topics and one 32-byte data word.
    let proof = world.receipt_proof(3);
    let logs = [LogQuery { tx_index: 3, log_index: 0, proof: proof.as_slice(), conditions: &[] }];
    let none: [AccountQuery<'_, Vec<u8>>; 0] = [];
    let run = |l: Field, r_fact: usize, r: Field| {
        let relations =
            [Relation { left: Operand { fact: 0, field: l }, cmp: Cmp::Eq, right: Operand { fact: r_fact, field: r } }];
        attest_batch(&record, 1, &Batch { accounts: &none, logs: &logs, relations: &relations }).map(|f| f.len())
    };
    assert_eq!(run(Field::Topic(0), 0, Field::Topic(0)), Ok(1));
    assert_eq!(run(Field::Topic(3), 0, Field::Topic(0)), Err(Error::PredicateFailed), "a fourth topic");
    assert_eq!(run(Field::DataWord(1), 0, Field::Topic(0)), Err(Error::PredicateFailed), "a second data word");
    assert_eq!(run(Field::Value, 0, Field::Topic(0)), Err(Error::QueryInvalid), "a field of another kind");
    assert_eq!(run(Field::Topic(0), 1, Field::Topic(0)), Err(Error::QueryInvalid), "a fact that is not there");
}

/// Row "`get_lc_execution_root`": the root depends on the store's layout, not only on the slot. A
/// Capella store keeps hashing a header as `capella.ExecutionPayloadHeader` (15 fields) after
/// the Deneb fork epoch, as the Capella spec does; only a Deneb-or-later store hashes the 17-field
/// Deneb header. Review mutant R03 (dropping `fork >= LcFork::Deneb`) survived every test before
/// this one.
#[test]
fn execution_root_follows_the_store_layout_after_deneb() {
    use ferrata_core::{
        fork::{ChainSpec, LcFork, Preset, FAR_FUTURE_EPOCH},
        types::{BeaconBlockHeader, Execution, ExecutionPayloadHeader, LightClientHeader},
        validate::get_lc_execution_root,
    };
    // Capella and Deneb from genesis; nothing later.
    let mut epochs = [FAR_FUTURE_EPOCH; 9];
    epochs[..5].copy_from_slice(&[0, 0, 0, 0, 0]);
    let spec = ChainSpec { preset: Preset::MINIMAL, versions: [[0; 4]; 9], epochs };
    let mut payload = ExecutionPayloadHeader::empty();
    payload.block_number = 7;
    payload.block_hash = [9; 32];
    let header = LightClientHeader {
        beacon: BeaconBlockHeader { slot: 100, ..Default::default() },
        execution: Execution::Payload { header: payload.clone(), branch: vec![[0; 32]; 4] },
    };
    assert_ne!(payload.hash_tree_root_capella(), payload.hash_tree_root_deneb());
    assert_eq!(get_lc_execution_root(&header, LcFork::Capella, &spec), payload.hash_tree_root_capella());
    assert_eq!(get_lc_execution_root(&header, LcFork::Deneb, &spec), payload.hash_tree_root_deneb());
    assert_eq!(get_lc_execution_root(&header, LcFork::Electra, &spec), payload.hash_tree_root_deneb());
}

/// Row "`validate_light_client_update`": its shape checks include the finalized header's layout.
/// An update without finality must carry the empty finalized header *of the store's layout*; the
/// all-zero header of another layout is `ForkMismatch`, though `is_empty` holds for it. Review
/// mutant R12 (dropping that layout check) survived every test before this one. (Decoded updates
/// always have the store's layout, so this guards in-memory callers only.)
#[test]
fn an_empty_finalized_header_of_another_layout_is_refused() {
    use ferrata_core::{
        bls::{Aggregation, Blst},
        merkle::ZERO,
        store::LightClientStore,
        types::{BeaconBlockHeader, Execution, LightClientHeader},
        validate::{validate_light_client_update, Context, SignaturePath},
    };
    use ferrata_spectests::{load_sync_case, vectors_dir};
    static BLST: Blst = Blst::new(Aggregation::Auto);
    let case = load_sync_case(&vectors_dir().join("electra/light_client/sync/pyspec_tests/light_client_sync")).unwrap();
    let spec = &case.config.spec;
    let ctx = Context { spec, genesis_validators_root: case.genesis_validators_root, bls: &BLST };
    let store = LightClientStore::initialize(&case.trusted_block_root, &case.bootstrap, case.store_fork, spec).unwrap();
    let (u, current) = &case.updates[0];
    let mut plain = u.clone();
    plain.finality_branch = vec![ZERO; u.finality_branch.len()];
    plain.finalized_header = LightClientHeader::empty(case.store_fork);
    assert_eq!(validate_light_client_update(&store.core, &plain, *current, &ctx, SignaturePath::Spec), Ok(()));
    let mut other = plain.clone();
    other.finalized_header = LightClientHeader { beacon: BeaconBlockHeader::default(), execution: Execution::None };
    assert!(other.finalized_header.is_empty());
    assert_eq!(
        validate_light_client_update(&store.core, &other, *current, &ctx, SignaturePath::Spec),
        Err(Error::ForkMismatch)
    );
}

/// `FinalizedStore::from_core` (row 11's store): "Both committees are checked, as at adoption"
/// (policy.rs:81-82). Review mutant R07 (no check of a known next committee) survived every test
/// before this one.
#[test]
fn from_core_checks_a_known_next_committee() {
    use ferrata_core::{
        bls::{Aggregation, Blst},
        fork::LcFork,
        policy::FinalizedStore,
        store::StoreCore,
        types::{LightClientHeader, SyncCommittee},
    };
    use ferrata_spectests::testkit::with_wrong_aggregate;
    let bls = Blst::new(Aggregation::Auto);
    let core = |next: SyncCommittee| StoreCore {
        fork: LcFork::Electra,
        finalized_header: LightClientHeader::empty(LcFork::Electra),
        current_sync_committee: fake_committee(32, 0x31).committee,
        next_sync_committee: next,
    };
    assert!(FinalizedStore::from_core(core(SyncCommittee::empty(32)), &bls).is_ok(), "next unknown");
    assert!(FinalizedStore::from_core(core(fake_committee(32, 0x32).committee), &bls).is_ok());
    let bad = with_wrong_aggregate(&fake_committee(32, 0x33).committee);
    assert_eq!(FinalizedStore::from_core(core(bad), &bls).err(), Some(Error::CommitteeInvalid));
}

/// The on-chain program validates through `steps::validate_compact`, a second transcription of row
/// 1 that has no row of its own. Row 1's guards for M03 (conflicting next committee), M04 (attested
/// slot equal to the finalized slot) and M17 (stale update) run on row 1 only; review mutants S03,
/// S09 and S02 (the same changes in `steps.rs`) survived every test before this one. Here both
/// transcriptions see the same inputs and must give the same verdict and code (row 1 also checks
/// the signature, which `validate_compact` leaves to the later steps; these inputs are all signed).
#[test]
fn the_compact_validation_agrees_with_row_1_on_the_guard_cases() {
    use ferrata_core::{
        bls::{Aggregation, Blst},
        policy::FinalizedStore,
        steps,
        store::{LightClientStore, StoreCore},
        types::LightClientUpdate,
        validate::{validate_light_client_update, Context, SignaturePath},
        witness::{CompactStore, CompactUpdate},
    };
    use ferrata_spectests::{
        load_sync_case,
        testkit::{fabricated_update, Fabrication},
        vectors_dir,
    };
    static BLST: Blst = Blst::new(Aggregation::Auto);
    let case = load_sync_case(&vectors_dir().join("electra/light_client/sync/pyspec_tests/light_client_sync")).unwrap();
    let spec = &case.config.spec;
    let gvr = case.genesis_validators_root;
    let ctx = Context { spec, genesis_validators_root: gvr, bls: &BLST };
    let (u0, current) = case.updates[0].clone();
    let (attested, signature) = (u0.attested_header.beacon.slot, u0.signature_slot);
    let both = |core: &StoreCore, update: &LightClientUpdate| {
        let row1 = validate_light_client_update(core, update, current, &ctx, SignaturePath::Spec);
        let compact = steps::validate_compact(&CompactStore::of(core), &CompactUpdate::of(update), current, spec, &gvr)
            .map(|_| ());
        (row1, compact)
    };

    // M03 / S03: a next committee that conflicts with the known one.
    let mut policy =
        FinalizedStore::initialize(&case.trusted_block_root, &case.bootstrap, case.store_fork, spec, &BLST).unwrap();
    policy.process_update(&u0, current, &ctx).unwrap();
    let mut conflicting = u0.clone();
    conflicting.next_sync_committee.pubkeys[3][7] ^= 1;
    let want = Err(Error::NextCommitteeMismatch);
    assert_eq!(both(policy.core(), &conflicting), (want, want), "conflicting next committee");

    // M17 / S02: a stale update (attested at the store's finalized slot, nothing supplied).
    let fake = fake_committee(32, 0x44);
    let mut spec_store =
        LightClientStore::initialize(&case.trusted_block_root, &case.bootstrap, case.store_fork, spec).unwrap();
    spec_store.core.current_sync_committee = fake.committee.clone();
    let at = spec_store.core.finalized_header.beacon.slot;
    let stale = fabricated_update(
        &Fabrication {
            finalized_slot: Some(at),
            attested_slot: at,
            signature_slot: signature,
            signers: 32,
            next_committee: None,
        },
        &fake,
        &ctx,
    );
    let want = Err(Error::UpdateIrrelevant);
    assert_eq!(both(&spec_store.core, &stale), (want, want), "stale update");

    // M04 / S09: the attested slot may equal the finalized slot.
    let fake = fake_committee(32, 0x33);
    let equal = fabricated_update(
        &Fabrication {
            finalized_slot: Some(attested),
            attested_slot: attested,
            signature_slot: signature,
            signers: 32,
            next_committee: None,
        },
        &fake,
        &ctx,
    );
    let initial = StoreCore {
        current_sync_committee: fake.committee.clone(),
        ..FinalizedStore::initialize(&case.trusted_block_root, &case.bootstrap, case.store_fork, spec, &BLST)
            .unwrap()
            .core()
            .clone()
    };
    assert_eq!(both(&initial, &equal), (Ok(()), Ok(())), "attested slot equal to the finalized slot");
}
