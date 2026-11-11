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
        let relations = [Relation {
            left: Operand { fact: 0, field: l },
            cmp: Cmp::Eq,
            right: Operand { fact: r_fact, field: r },
        }];
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
    let case =
        load_sync_case(&vectors_dir().join("electra/light_client/sync/pyspec_tests/light_client_sync")).unwrap();
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
