//! The aggregate signature must be a point of G2 (a canonical compressed encoding of a curve point
//! in the prime-order subgroup) and not the identity, on every verification path: the spec path
//! (`fast_aggregate_verify`), the checked-committee path (`verify_checked`) and the split path
//! (`Blst::miller_half`, then `Blst::final_half`), with both libraries and every key
//! aggregation, and through both stores.
//!
//! Where this is enforced:
//! * blst: `miller()` in `bls/blst_backend.rs`, which every blst path calls before any pairing,
//!   `miller_half` included: `blst_p2_uncompress` (encoding, on the curve), then
//!   `blst_p2_affine_in_g2` (subgroup), both `SignatureMalformed`; then `blst_p2_affine_is_inf`
//!   (identity, `SignatureInvalid`). `final_half` only finishes a product `miller_half` made, so a
//!   program feeds it only products it stored itself.
//! * zkcrypto: `pairing_check()` in `bls/zkcrypto_backend.rs`: `G2Affine::from_compressed`
//!   (encoding, curve and subgroup, `SignatureMalformed`), then `is_identity` (`SignatureInvalid`).

use blst::min_pk::Signature;
use ferrata_core::{
    bls::{Aggregation, BlsBackend, Blst, Zkcrypto},
    fork::{ChainSpec, LcFork, Preset, FAR_FUTURE_EPOCH},
    policy::FinalizedStore,
    store::{LightClientStore, StoreCore},
    types::{BlsSignature, LightClientHeader},
    validate::Context,
    Error,
};
use ferrata_spectests::testkit::{fabricated_update, fake_committee, sign, Fabrication};

fn identity() -> BlsSignature {
    let mut s = [0u8; 96];
    s[0] = 0xc0;
    s
}

/// `x = (0, k)`: the first small `k` for which the point decodes but is not in G2 (`want_on_curve`),
/// or does not decode at all (not on the curve).
fn small_x(want_on_curve: bool) -> BlsSignature {
    (1u8..=255)
        .map(|k| {
            let mut s = [0u8; 96];
            s[0] = 0x80;
            s[95] = k;
            s
        })
        .find(|s| match Signature::uncompress(s) {
            Ok(sig) => want_on_curve && sig.validate(true).is_err(),
            Err(_) => !want_on_curve,
        })
        .unwrap()
}

/// Every blst and zkcrypto configuration.
fn backends() -> Vec<(String, Box<dyn BlsBackend>)> {
    let mut out: Vec<(String, Box<dyn BlsBackend>)> = Vec::new();
    for a in [Aggregation::Auto, Aggregation::Additive, Aggregation::Subtractive] {
        out.push((format!("blst/{a:?}"), Box::new(Blst::new(a))));
        out.push((format!("zkcrypto/{a:?}"), Box::new(Zkcrypto::new(a))));
    }
    out
}

#[test]
fn every_path_refuses_signatures_outside_g2_and_the_identity() {
    let fake = fake_committee(8, 0x31);
    let bits = [true, true, false, true, true, true, false, true];
    let message = [7u8; 32];
    let good = sign(&fake, &bits, &message);
    let mut no_flag = good;
    no_flag[0] &= 0x7f;
    let mut infinity_with_bits = identity();
    infinity_with_bits[95] = 1;
    let cases: [(&str, BlsSignature, Result<(), Error>); 7] = [
        ("valid", good, Ok(())),
        ("valid, another message", sign(&fake, &bits, &[8u8; 32]), Err(Error::SignatureInvalid)),
        ("the identity", identity(), Err(Error::SignatureInvalid)),
        ("on the curve, outside G2", small_x(true), Err(Error::SignatureMalformed)),
        ("not on the curve", small_x(false), Err(Error::SignatureMalformed)),
        ("no compression flag", no_flag, Err(Error::SignatureMalformed)),
        ("identity flag with other bits set", infinity_with_bits, Err(Error::SignatureMalformed)),
    ];
    for (name, bls) in backends() {
        for (what, sig, want) in &cases {
            assert_eq!(
                bls.fast_aggregate_verify(&fake.committee, &bits, &message, sig),
                *want,
                "{name} spec path: {what}"
            );
            assert_eq!(bls.verify_checked(&fake.committee, &bits, &message, sig), *want, "{name} checked path: {what}");
        }
    }
    // The split: the first half refuses before any product exists; a product it made is checked
    // by the second half.
    for a in [Aggregation::Auto, Aggregation::Additive, Aggregation::Subtractive] {
        let blst = Blst::new(a);
        for (what, sig, want) in &cases {
            let split = blst.miller_half(&fake.committee, &bits, &message, sig).and_then(|p| Blst::final_half(&p));
            assert_eq!(split, *want, "blst/{a:?} split path: {what}");
        }
        let refused: Vec<_> = cases[2..]
            .iter()
            .map(|(_, sig, _)| blst.miller_half(&fake.committee, &bits, &message, sig).err())
            .collect();
        assert_eq!(
            refused,
            [
                Some(Error::SignatureInvalid),
                Some(Error::SignatureMalformed),
                Some(Error::SignatureMalformed),
                Some(Error::SignatureMalformed),
                Some(Error::SignatureMalformed)
            ],
            "blst/{a:?}: miller_half itself refuses"
        );
    }
}

#[test]
fn both_stores_refuse_them() {
    let fake = fake_committee(32, 0x42);
    let mut versions = [[0u8; 4]; 9];
    for (i, v) in versions.iter_mut().enumerate() {
        *v = [i as u8, 1, 2, 3];
    }
    let spec = ChainSpec {
        preset: Preset::MINIMAL,
        versions,
        epochs: [0, 0, 0, 0, 0, 0, FAR_FUTURE_EPOCH, FAR_FUTURE_EPOCH, FAR_FUTURE_EPOCH],
    };
    for (name, bls) in backends() {
        let ctx = Context { spec: &spec, genesis_validators_root: [9; 32], bls: bls.as_ref() };
        let update = fabricated_update(
            &Fabrication {
                finalized_slot: Some(72),
                attested_slot: 80,
                signature_slot: 81,
                signers: 32,
                next_committee: None,
            },
            &fake,
            &ctx,
        );
        // A store at slot 64 (period 1) that holds the committee.
        let mut finalized_core = StoreCore {
            fork: LcFork::Electra,
            finalized_header: LightClientHeader::empty(LcFork::Electra),
            current_sync_committee: fake.committee.clone(),
            next_sync_committee: ferrata_core::types::SyncCommittee::empty(32),
        };
        finalized_core.finalized_header.beacon.slot = 64;
        for (what, sig, want) in [
            ("valid", update.sync_aggregate.signature, Ok(())),
            ("the identity", identity(), Err(Error::SignatureInvalid)),
            ("outside G2", small_x(true), Err(Error::SignatureMalformed)),
            ("not on the curve", small_x(false), Err(Error::SignatureMalformed)),
        ] {
            let mut u = update.clone();
            u.sync_aggregate.signature = sig;
            let mut policy = FinalizedStore::from_core(finalized_core.clone(), bls.as_ref()).unwrap();
            assert_eq!(policy.process_update(&u, 82, &ctx).map(|_| ()), want, "{name} policy store: {what}");
            let mut spec_store = LightClientStore {
                core: finalized_core.clone(),
                best_valid_update: None,
                optimistic_header: finalized_core.finalized_header.clone(),
                previous_max_active_participants: 0,
                current_max_active_participants: 0,
            };
            assert_eq!(spec_store.process_update(&u, 82, &ctx), want, "{name} spec store: {what}");
        }
    }
}
