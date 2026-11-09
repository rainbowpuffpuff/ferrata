//! BLS phases with raw `blst` calls, mirroring ferrata's `Blst` backend step by step.

use std::hint::black_box;

use blst::{
    blst_bendian_from_fp, blst_final_exp, blst_fp12, blst_fp12_is_one, blst_fp12_mul, blst_hash_to_g2,
    blst_miller_loop, blst_miller_loop_n, blst_p1, blst_p1_add_or_double, blst_p1_add_or_double_affine,
    blst_p1_affine, blst_p1_affine_compress, blst_p1_affine_generator, blst_p1_affine_in_g1, blst_p1_affine_is_inf,
    blst_p1_affine_serialize, blst_p1_cneg, blst_p1_compress, blst_p1_deserialize, blst_p1_from_affine,
    blst_p1_in_g1, blst_p1_is_equal, blst_p1_is_inf, blst_p1_to_affine, blst_p1_uncompress, blst_p2,
    blst_p2_affine, blst_p2_affine_in_g2, blst_p2_affine_is_inf, blst_p2_affine_serialize, blst_p2_deserialize,
    blst_p2_to_affine, blst_p2_uncompress, BLST_ERROR,
};
use ferrata_core::{
    bls::{Aggregation, Blst, DST, FP12_BYTES},
    types::{BlsPubkey, BlsSignature, SyncCommittee},
    Root,
};

use crate::Rec;

pub const NAME: &str = "blst";

pub fn backend(aggregation: Aggregation) -> Blst {
    Blst::new(aggregation)
}

fn dec_g1(pk: &BlsPubkey) -> blst_p1_affine {
    let mut out = blst_p1_affine::default();
    let rc = unsafe { blst_p1_uncompress(&mut out, pk.as_ptr()) };
    assert!(rc == BLST_ERROR::BLST_SUCCESS, "pubkey decode failed");
    out
}

/// `KeyValidate` as ferrata's blst backend does it: on-curve decode, not identity, in G1.
fn key_validate(pk: &BlsPubkey) -> blst_p1_affine {
    let p = dec_g1(pk);
    assert!(unsafe { !blst_p1_affine_is_inf(&p) && blst_p1_affine_in_g1(&p) }, "key fails KeyValidate");
    p
}

fn add_affine(acc: &mut blst_p1, p: &blst_p1_affine) {
    let a: *mut blst_p1 = acc;
    unsafe { blst_p1_add_or_double_affine(a, a, p) };
}

fn neg_affine(p: &blst_p1) -> blst_p1_affine {
    let mut n = *p;
    let mut out = blst_p1_affine::default();
    unsafe {
        blst_p1_cneg(&mut n, true);
        blst_p1_to_affine(&mut out, &n);
    }
    out
}

fn subtractive(c: &SyncCommittee, absent: &[&BlsPubkey]) -> blst_p1 {
    let agg = dec_g1(&c.aggregate_pubkey);
    let mut absent_sum = blst_p1::default();
    for k in absent {
        add_affine(&mut absent_sum, &dec_g1(k));
    }
    let mut key = blst_p1::default();
    unsafe {
        blst_p1_from_affine(&mut key, &agg);
        blst_p1_cneg(&mut absent_sum, true);
        let kp: *mut blst_p1 = &mut key;
        blst_p1_add_or_double(kp, kp, &absent_sum);
    }
    key
}

/// Signature decode as ferrata does it: decompress, G2 subgroup check, not identity.
fn dec_sig_checked(sig: &BlsSignature) -> blst_p2_affine {
    let mut s = blst_p2_affine::default();
    assert!(unsafe { blst_p2_uncompress(&mut s, sig.as_ptr()) } == BLST_ERROR::BLST_SUCCESS);
    assert!(unsafe { blst_p2_affine_in_g2(&s) && !blst_p2_affine_is_inf(&s) });
    s
}

fn hash_to_g2(msg: &Root) -> blst_p2_affine {
    let mut p = blst_p2::default();
    let mut a = blst_p2_affine::default();
    unsafe {
        blst_hash_to_g2(&mut p, msg.as_ptr(), msg.len(), DST.as_ptr(), DST.len(), core::ptr::null(), 0);
        blst_p2_to_affine(&mut a, &p);
    }
    a
}

/// The two Miller loops exactly as ferrata's `pairing_check`: ML(H(m), −key) · ML(sig, g1).
fn miller_two_single(hm: &blst_p2_affine, neg_key: &blst_p1_affine, sig: &blst_p2_affine) -> blst_fp12 {
    let (mut lhs, mut rhs, mut product) = (blst_fp12::default(), blst_fp12::default(), blst_fp12::default());
    unsafe {
        blst_miller_loop(&mut lhs, hm, neg_key);
        blst_miller_loop(&mut rhs, sig, blst_p1_affine_generator());
        blst_fp12_mul(&mut product, &lhs, &rhs);
    }
    product
}

/// The same product from one 2-pair Miller loop (shared Fp12 squarings).
fn miller_one_double(hm: &blst_p2_affine, neg_key: &blst_p1_affine, sig: &blst_p2_affine) -> blst_fp12 {
    let mut out = blst_fp12::default();
    let qs: [*const blst_p2_affine; 2] = [hm, sig];
    let ps: [*const blst_p1_affine; 2] = [neg_key, unsafe { blst_p1_affine_generator() }];
    unsafe { blst_miller_loop_n(&mut out, qs.as_ptr(), ps.as_ptr(), 2) };
    out
}

fn final_exp_is_one(f: &blst_fp12) -> bool {
    let mut r = blst_fp12::default();
    unsafe {
        blst_final_exp(&mut r, f);
        blst_fp12_is_one(&r)
    }
}

pub fn bls_phases(rec: &mut Rec, c: &SyncCommittee, bits: &[bool], msg: &Root, sig_bytes: &BlsSignature) {
    let signers: Vec<&BlsPubkey> = c.pubkeys.iter().zip(bits).filter(|(_, b)| **b).map(|(k, _)| k).collect();
    let absent: Vec<&BlsPubkey> = c.pubkeys.iter().zip(bits).filter(|(_, b)| !**b).map(|(k, _)| k).collect();

    // 5: the signing key, both compositions (on-curve decode only, as verify_checked).
    let key_sub = rec.time("5s aggregation, subtractive: decompress aggregate + non-signers, add, negate", || {
        subtractive(c, &absent)
    });
    let key_add = rec.time("5a aggregation, additive: decompress every signer key + add", || {
        let mut key = blst_p1::default();
        for k in &signers {
            add_affine(&mut key, &dec_g1(k));
        }
        key
    });
    let points: Vec<blst_p1_affine> = signers.iter().map(|k| dec_g1(k)).collect();
    let key_add2 = rec.time("5a' additive, point additions only (keys already decompressed)", || {
        let mut key = blst_p1::default();
        for p in &points {
            add_affine(&mut key, p);
        }
        key
    });
    rec.time("5d one G1 decompression (for scale)", || dec_g1(black_box(signers[0])));
    rec.time("5g one G1 subgroup check, affine (for scale)", || unsafe { blst_p1_affine_in_g1(black_box(&points[0])) });
    assert!(unsafe { blst_p1_is_equal(&key_sub, &key_add) && blst_p1_is_equal(&key_add, &key_add2) });
    rec.time("5k G1 subgroup check of the aggregate key (earlier design; not in verify_checked)", || {
        assert!(unsafe { !blst_p1_is_inf(&key_sub) && blst_p1_in_g1(&key_sub) })
    });
    let neg_key = rec.time("5n identity check + negate key + to affine (as pairing_check)", || {
        assert!(!unsafe { blst_p1_is_inf(&key_sub) });
        neg_affine(&key_sub)
    });

    // 6
    let sig = rec.time("6a signature decompression (G2)", || {
        let mut s = blst_p2_affine::default();
        assert!(unsafe { blst_p2_uncompress(&mut s, sig_bytes.as_ptr()) } == BLST_ERROR::BLST_SUCCESS);
        s
    });
    rec.time("6b signature G2 subgroup check + not identity", || {
        assert!(unsafe { blst_p2_affine_in_g2(&sig) && !blst_p2_affine_is_inf(&sig) })
    });

    // 7
    let hm = rec.time("7  hash_to_G2(signing root) + to affine", || hash_to_g2(msg));

    // 8
    let f_two = rec.time("8  Miller loops as ferrata: ML(H(m),-key) + ML(sig,g1) + Fp12 mul", || {
        miller_two_single(&hm, &neg_key, &sig)
    });
    let f_one = rec.time("8m alternative: one 2-pair blst_miller_loop_n", || miller_one_double(&hm, &neg_key, &sig));

    // 9
    let ok = rec.time("9  final exponentiation + is_one", || final_exp_is_one(&f_two));
    assert!(ok, "pairing check (two loops) failed");
    assert!(final_exp_is_one(&f_one), "pairing check (2-pair loop) failed");
}

pub fn adoption_phases(rec: &mut Rec, c: &SyncCommittee, unc: &[[u8; 96]]) {
    rec.time("10a adoption, compressed: 512 x KeyValidate (decompress + G1 subgroup) + sum + compare aggregate", || {
        let mut sum = blst_p1::default();
        for k in &c.pubkeys {
            add_affine(&mut sum, &key_validate(k));
        }
        let agg = dec_g1(&c.aggregate_pubkey);
        let mut s = blst_p1_affine::default();
        unsafe { blst_p1_to_affine(&mut s, &sum) };
        assert!(s == agg, "committee keys do not sum to the aggregate");
    });
    rec.time("10a0 same, on-curve only (no per-key subgroup check; earlier design)", || {
        let mut sum = blst_p1::default();
        for k in &c.pubkeys {
            add_affine(&mut sum, &dec_g1(k));
        }
        let agg = dec_g1(&c.aggregate_pubkey);
        let mut s = blst_p1_affine::default();
        unsafe { blst_p1_to_affine(&mut s, &sum) };
        assert!(s == agg, "committee keys do not sum to the aggregate");
    });
    rec.time("10b adoption, uncompressed: 512 x (deserialize+on-curve, recompress==committed, G1 subgroup) + sum + compress==aggregate", || {
        let mut sum = blst_p1::default();
        for (k, u) in c.pubkeys.iter().zip(unc) {
            let mut a = blst_p1_affine::default();
            assert!(unsafe { blst_p1_deserialize(&mut a, u.as_ptr()) } == BLST_ERROR::BLST_SUCCESS);
            let mut comp = [0u8; 48];
            unsafe { blst_p1_affine_compress(comp.as_mut_ptr(), &a) };
            assert!(comp == *k, "uncompressed key does not match the committed key");
            assert!(unsafe { !blst_p1_affine_is_inf(&a) && blst_p1_affine_in_g1(&a) }, "key fails KeyValidate");
            add_affine(&mut sum, &a);
        }
        let mut comp = [0u8; 48];
        unsafe { blst_p1_compress(comp.as_mut_ptr(), &sum) };
        assert!(comp == c.aggregate_pubkey, "committee keys do not sum to the aggregate");
    });
    rec.time("10b0 same, uncompressed without the per-key subgroup check", || {
        let mut sum = blst_p1::default();
        for (k, u) in c.pubkeys.iter().zip(unc) {
            let mut a = blst_p1_affine::default();
            assert!(unsafe { blst_p1_deserialize(&mut a, u.as_ptr()) } == BLST_ERROR::BLST_SUCCESS);
            let mut comp = [0u8; 48];
            unsafe { blst_p1_affine_compress(comp.as_mut_ptr(), &a) };
            assert!(comp == *k, "uncompressed key does not match the committed key");
            add_affine(&mut sum, &a);
        }
        let mut comp = [0u8; 48];
        unsafe { blst_p1_compress(comp.as_mut_ptr(), &sum) };
        assert!(comp == c.aggregate_pubkey, "committee keys do not sum to the aggregate");
    });
    let points: Vec<blst_p1_affine> = c.pubkeys.iter().map(dec_g1).collect();
    rec.time("10g 512 G1 subgroup checks alone (for scale)", || {
        for p in &points {
            assert!(unsafe { blst_p1_affine_in_g1(p) });
        }
    });
}

/// Split, first half: signing key, signature checks, hash-to-G2 and the Miller loops.
pub fn split_ml(rec: &mut Rec, c: &SyncCommittee, bits: &[bool], msg: &Root, sig_bytes: &BlsSignature) -> Vec<u8> {
    let absent: Vec<&BlsPubkey> = c.pubkeys.iter().zip(bits).filter(|(_, b)| !**b).map(|(k, _)| k).collect();
    let neg_key = rec.time("S5 signing key, subtractive, negated, affine", || {
        let key = subtractive(c, &absent);
        assert!(!unsafe { blst_p1_is_inf(&key) });
        neg_affine(&key)
    });
    let sig = rec.time("S6 signature decompression + G2 subgroup check", || dec_sig_checked(sig_bytes));
    let hm = rec.time("S7 hash_to_G2", || hash_to_g2(msg));
    let f = rec.time("S8 Miller loops (as ferrata: 2 x single-pair + Fp12 mul)", || miller_two_single(&hm, &neg_key, &sig));
    let bytes = unsafe {
        core::slice::from_raw_parts(&f as *const blst_fp12 as *const u8, core::mem::size_of::<blst_fp12>())
    };
    bytes.to_vec()
}

/// Split, second half: final exponentiation of the committed Fp12 and the `== 1` check.
pub fn split_fe(rec: &mut Rec, bytes: &[u8]) -> bool {
    assert_eq!(bytes.len(), core::mem::size_of::<blst_fp12>());
    let mut f = blst_fp12::default();
    unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), &mut f as *mut blst_fp12 as *mut u8, bytes.len()) };
    rec.time("F9 final exponentiation + is_one", || final_exp_is_one(&f))
}

/// The core's split seam, first half: `Blst::miller_half` (Auto = subtractive at 510/512).
pub fn core_miller_half(rec: &mut Rec, c: &SyncCommittee, bits: &[bool], msg: &Root, sig: &BlsSignature) -> Vec<u8> {
    let b = Blst::new(Aggregation::Auto);
    let f = rec.time("C1e Blst::miller_half: signing key, signature checks, hash-to-G2, Miller loops, encode", || {
        b.miller_half(c, bits, msg, sig).unwrap()
    });
    f.to_vec()
}

/// The core's split seam, second half: `Blst::final_half`.
pub fn core_final_half(rec: &mut Rec, bytes: &[u8]) -> bool {
    let product: [u8; FP12_BYTES] = bytes.try_into().expect("576 bytes");
    rec.time("C2 Blst::final_half: decode Fp12, final exponentiation, == 1", || Blst::final_half(&product).is_ok())
}

/// The canonical Fp12 encoding of ferrata's blst backend: twelve big-endian Fp coefficients in
/// `fp6[i].fp2[j].fp[k]` order (the host checks it against the core's `miller_half`).
fn encode_fp12(f: &blst_fp12) -> Vec<u8> {
    let mut out = vec![0u8; FP12_BYTES];
    let coefficients = f.fp6.iter().flat_map(|fp6| fp6.fp2.iter()).flat_map(|fp2| fp2.fp.iter());
    for (chunk, fp) in out.chunks_exact_mut(48).zip(coefficients) {
        unsafe { blst_bendian_from_fp(chunk.as_mut_ptr(), fp) };
    }
    out
}

/// 3-step job, step 1 (BLS part): everything before the Miller loops, as the core's
/// `checked_signing_key` and `miller` do it. Hands over H(m) (G2), -key (G1) and the signature
/// (G2) as 480 bytes of uncompressed points.
pub fn step1(rec: &mut Rec, c: &SyncCommittee, bits: &[bool], msg: &Root, sig_bytes: &BlsSignature) -> Vec<u8> {
    let absent: Vec<&BlsPubkey> = c.pubkeys.iter().zip(bits).filter(|(_, b)| !**b).map(|(k, _)| k).collect();
    let key = rec.time("T1e signing key, subtractive + identity check", || {
        let k = subtractive(c, &absent);
        assert!(!unsafe { blst_p1_is_inf(&k) });
        k
    });
    let sig = rec.time("T1f signature decompression + G2 subgroup check + not identity", || dec_sig_checked(sig_bytes));
    let hm = rec.time("T1g hash_to_G2 + to affine", || hash_to_g2(msg));
    let neg_key = rec.time("T1h negate key + to affine", || neg_affine(&key));
    rec.time("T1i serialize H(m), -key, signature (480 bytes)", || {
        let mut out = vec![0u8; 480];
        unsafe {
            blst_p2_affine_serialize(out[..192].as_mut_ptr(), &hm);
            blst_p1_affine_serialize(out[192..288].as_mut_ptr(), &neg_key);
            blst_p2_affine_serialize(out[288..].as_mut_ptr(), &sig);
        }
        out
    })
}

/// 3-step job, step 2: deserialize the three points, the Miller loops, encode the product.
pub fn step2(rec: &mut Rec, bytes: &[u8]) -> Vec<u8> {
    assert_eq!(bytes.len(), 480);
    let (hm, neg_key, sig) = rec.time("T2a deserialize H(m), -key, signature (on-curve checks)", || {
        let (mut hm, mut nk, mut sig) = (blst_p2_affine::default(), blst_p1_affine::default(), blst_p2_affine::default());
        unsafe {
            assert!(blst_p2_deserialize(&mut hm, bytes[..192].as_ptr()) == BLST_ERROR::BLST_SUCCESS);
            assert!(blst_p1_deserialize(&mut nk, bytes[192..288].as_ptr()) == BLST_ERROR::BLST_SUCCESS);
            assert!(blst_p2_deserialize(&mut sig, bytes[288..].as_ptr()) == BLST_ERROR::BLST_SUCCESS);
        }
        (hm, nk, sig)
    });
    let f = rec.time("T2b Miller loops: ML(H(m),-key) + ML(sig,g1) + Fp12 mul", || miller_two_single(&hm, &neg_key, &sig));
    rec.time("T2c encode the product (576 bytes, the core's canonical encoding)", || encode_fp12(&f))
}
