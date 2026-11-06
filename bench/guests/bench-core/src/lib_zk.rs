//! BLS phases with zkcrypto `bls12_381`, mirroring plumbline's `Zkcrypto` backend step by step.

use std::hint::black_box;

use bls12_381::{
    hash_to_curve::{ExpandMsgXmd, HashToCurve},
    multi_miller_loop, G1Affine, G1Projective, G2Affine, G2Prepared, G2Projective, Gt,
};
use plumbline_core::{
    bls::{Aggregation, Zkcrypto, DST},
    types::{BlsPubkey, BlsSignature, SyncCommittee},
    Root,
};

use crate::Rec;

pub const NAME: &str = "zkcrypto bls12_381";

pub fn backend(aggregation: Aggregation) -> Zkcrypto {
    Zkcrypto::new(aggregation)
}

fn dec_g1(pk: &BlsPubkey) -> G1Affine {
    Option::from(G1Affine::from_compressed_unchecked(pk)).expect("pubkey decode failed")
}

/// `KeyValidate` as plumbline's zkcrypto backend does it: `from_compressed` (decompress + subgroup).
fn key_validate(pk: &BlsPubkey) -> G1Affine {
    let p: G1Affine = Option::from(G1Affine::from_compressed(pk)).expect("key fails KeyValidate");
    assert!(!bool::from(p.is_identity()));
    p
}

pub fn bls_phases(rec: &mut Rec, c: &SyncCommittee, bits: &[bool], msg: &Root, sig_bytes: &BlsSignature) {
    let signers: Vec<&BlsPubkey> = c.pubkeys.iter().zip(bits).filter(|(_, b)| **b).map(|(k, _)| k).collect();
    let absent: Vec<&BlsPubkey> = c.pubkeys.iter().zip(bits).filter(|(_, b)| !**b).map(|(k, _)| k).collect();

    // 5
    let key_sub = rec.time("5s aggregation, subtractive: decompress aggregate + non-signers, add, negate", || {
        let agg = dec_g1(&c.aggregate_pubkey);
        let mut a = G1Projective::identity();
        for k in &absent {
            a += dec_g1(k);
        }
        G1Projective::from(agg) - a
    });
    let key_add = rec.time("5a aggregation, additive: decompress every signer key + add", || {
        let mut a = G1Projective::identity();
        for k in &signers {
            a += dec_g1(k);
        }
        a
    });
    let points: Vec<G1Affine> = signers.iter().map(|k| dec_g1(k)).collect();
    let key_add2 = rec.time("5a' additive, point additions only (keys already decompressed)", || {
        let mut a = G1Projective::identity();
        for p in &points {
            a += p;
        }
        a
    });
    rec.time("5d one G1 decompression (for scale)", || dec_g1(black_box(signers[0])));
    rec.time("5g one G1 subgroup check, affine (for scale)", || bool::from(black_box(&points[0]).is_torsion_free()));
    assert!(key_sub == key_add && key_add == key_add2);
    rec.time("5k G1 subgroup check of the aggregate key (earlier design; not in verify_checked)", || {
        assert!(!bool::from(key_sub.is_identity()));
        assert!(bool::from(G1Affine::from(key_sub).is_torsion_free()));
    });
    let neg_key = rec.time("5n identity check + negate key + to affine (as pairing_check)", || {
        assert!(!bool::from(key_sub.is_identity()));
        G1Affine::from(-key_sub)
    });

    // 6
    let sig: G2Affine = rec.time("6a signature decompression (G2)", || {
        Option::from(G2Affine::from_compressed_unchecked(sig_bytes)).expect("signature decode failed")
    });
    rec.time("6b signature G2 subgroup check + not identity", || {
        assert!(bool::from(sig.is_torsion_free()) && !bool::from(sig.is_identity()))
    });

    // 7
    let hm = rec.time("7  hash_to_G2(signing root) + to affine", || {
        G2Affine::from(<G2Projective as HashToCurve<ExpandMsgXmd<sha2_09::Sha256>>>::hash_to_curve(msg, DST))
    });

    // 8: G2Prepared (line coefficients) for both G2 points, then the 2-pair multi Miller loop,
    // as plumbline's pairing_check: e(-key, H(m)) * e(g1, sig).
    let g1 = G1Affine::generator();
    let (hm_p, sig_p) = rec.time("8p G2Prepared::from x2 (line precomputation)", || {
        (G2Prepared::from(hm), G2Prepared::from(sig))
    });
    let ml = rec.time("8l multi_miller_loop, 2 pairs (prepared)", || multi_miller_loop(&[(&neg_key, &hm_p), (&g1, &sig_p)]));
    let n = rec.0.len();
    let total = rec.0[n - 1].1 + rec.0[n - 2].1;
    rec.note("8  Miller loop total (8p + 8l)", total);

    // 9
    let ok = rec.time("9  final exponentiation + == identity", || ml.final_exponentiation() == Gt::identity());
    assert!(ok, "pairing check failed");
}

pub fn adoption_phases(rec: &mut Rec, c: &SyncCommittee, unc: &[[u8; 96]]) {
    rec.time("10a adoption, compressed: 512 x KeyValidate (decompress + G1 subgroup) + sum + compare aggregate", || {
        let mut sum = G1Projective::identity();
        for k in &c.pubkeys {
            sum += key_validate(k);
        }
        assert!(G1Affine::from(sum) == dec_g1(&c.aggregate_pubkey), "committee keys do not sum to the aggregate");
    });
    rec.time("10a0 same, on-curve only (no per-key subgroup check; earlier design)", || {
        let mut sum = G1Projective::identity();
        for k in &c.pubkeys {
            sum += dec_g1(k);
        }
        assert!(G1Affine::from(sum) == dec_g1(&c.aggregate_pubkey), "committee keys do not sum to the aggregate");
    });
    rec.time("10b adoption, uncompressed: 512 x (deserialize+on-curve, recompress==committed, G1 subgroup) + sum + compress==aggregate", || {
        let mut sum = G1Projective::identity();
        for (k, u) in c.pubkeys.iter().zip(unc) {
            let p: G1Affine = Option::from(G1Affine::from_uncompressed_unchecked(u)).expect("bad uncompressed key");
            assert!(bool::from(p.is_on_curve()), "key not on curve");
            assert!(p.to_compressed() == *k, "uncompressed key does not match the committed key");
            assert!(bool::from(p.is_torsion_free()) && !bool::from(p.is_identity()), "key fails KeyValidate");
            sum += p;
        }
        assert!(G1Affine::from(sum).to_compressed() == c.aggregate_pubkey, "committee keys do not sum to the aggregate");
    });
    rec.time("10b0 same, uncompressed without the per-key subgroup check", || {
        let mut sum = G1Projective::identity();
        for (k, u) in c.pubkeys.iter().zip(unc) {
            let p: G1Affine = Option::from(G1Affine::from_uncompressed_unchecked(u)).expect("bad uncompressed key");
            assert!(bool::from(p.is_on_curve()), "key not on curve");
            assert!(p.to_compressed() == *k, "uncompressed key does not match the committed key");
            sum += p;
        }
        assert!(G1Affine::from(sum).to_compressed() == c.aggregate_pubkey, "committee keys do not sum to the aggregate");
    });
    let points: Vec<G1Affine> = c.pubkeys.iter().map(dec_g1).collect();
    rec.time("10g 512 G1 subgroup checks alone (for scale)", || {
        for p in &points {
            assert!(bool::from(p.is_torsion_free()));
        }
    });
}

pub fn split_ml(_rec: &mut Rec, _c: &SyncCommittee, _bits: &[bool], _msg: &Root, _sig: &BlsSignature) -> Vec<u8> {
    unimplemented!("the split demonstration uses blst's raw Fp12 type")
}

pub fn split_fe(_rec: &mut Rec, _bytes: &[u8]) -> bool {
    unimplemented!("the split demonstration uses blst's raw Fp12 type")
}

pub fn core_miller_half(_rec: &mut Rec, _c: &SyncCommittee, _bits: &[bool], _msg: &Root, _sig: &BlsSignature) -> Vec<u8> {
    unimplemented!("the core's split seam (miller_half/final_half) exists for blst only")
}

pub fn core_final_half(_rec: &mut Rec, _bytes: &[u8]) -> bool {
    unimplemented!("the core's split seam (miller_half/final_half) exists for blst only")
}

pub fn step1(_rec: &mut Rec, _c: &SyncCommittee, _bits: &[bool], _msg: &Root, _sig: &BlsSignature) -> Vec<u8> {
    unimplemented!("the 3-step job is measured with blst")
}

pub fn step2(_rec: &mut Rec, _bytes: &[u8]) -> Vec<u8> {
    unimplemented!("the 3-step job is measured with blst")
}
