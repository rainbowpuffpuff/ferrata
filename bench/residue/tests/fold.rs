//! The folded variant (`residue::fold`): its Miller loop equals blst's byte for byte, and the folded
//! check accepts exactly the products the final exponentiation accepts.
//!
//! `cargo test --release -p residue --test fold -- --nocapture`

use std::thread;

use blst::{
    blst_fp12, blst_hash_to_g2, blst_miller_loop_n, blst_p1, blst_p1_affine, blst_p1_affine_generator, blst_p1_cneg,
    blst_p1_deserialize, blst_p1_from_affine, blst_p1_to_affine, blst_p2, blst_p2_affine, blst_p2_deserialize,
    blst_p2_to_affine,
    min_pk::SecretKey,
    BLST_ERROR,
};
use residue::{
    check, decode_fp12, encode_fp12, eq, final_exp_is_one, fold, inverse, mul, one, witness, Refusal, FP12_BYTES,
};

const DST: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";

fn rng(seed: u64) -> impl FnMut() -> u64 {
    let mut s = seed ^ 0xf01d;
    move || {
        s = s.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = s;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

fn bytes32(next: &mut impl FnMut() -> u64) -> [u8; 32] {
    let mut out = [0u8; 32];
    for c in out.chunks_mut(8) {
        c.copy_from_slice(&next().to_be_bytes());
    }
    out
}

/// The signature pairs of the core's check: `(H(m), −key)` and `(signature, g1)`, for a key, a
/// message and a signature (valid if `valid`).
fn pairs(seed: u64, valid: bool) -> (blst_p2_affine, blst_p1_affine, blst_p2_affine) {
    let mut next = rng(seed);
    let sk = SecretKey::key_gen(&bytes32(&mut next), &[]).unwrap();
    let msg = bytes32(&mut next);
    let signed = if valid { msg } else { bytes32(&mut next) };
    let mut key = blst_p1_affine::default();
    // SAFETY: 96 serialized bytes.
    assert_eq!(unsafe { blst_p1_deserialize(&mut key, sk.sk_to_pk().serialize().as_ptr()) }, BLST_ERROR::BLST_SUCCESS);
    let (mut j, mut neg_key) = (blst_p1::default(), blst_p1_affine::default());
    let (mut h, mut hm) = (blst_p2::default(), blst_p2_affine::default());
    let mut sig = blst_p2_affine::default();
    // SAFETY: valid buffers, lengths and points.
    unsafe {
        blst_p1_from_affine(&mut j, &key);
        blst_p1_cneg(&mut j, true);
        blst_p1_to_affine(&mut neg_key, &j);
        blst_hash_to_g2(&mut h, msg.as_ptr(), msg.len(), DST.as_ptr(), DST.len(), core::ptr::null(), 0);
        blst_p2_to_affine(&mut hm, &h);
        assert_eq!(
            blst_p2_deserialize(&mut sig, sk.sign(&signed, DST, &[]).serialize().as_ptr()),
            BLST_ERROR::BLST_SUCCESS
        );
    }
    (hm, neg_key, sig)
}

fn blst_product(hm: &blst_p2_affine, neg_key: &blst_p1_affine, sig: &blst_p2_affine) -> blst_fp12 {
    let mut out = blst_fp12::default();
    // SAFETY: two valid points of each group; blst reads exactly two of each.
    unsafe {
        let qs: [*const blst_p2_affine; 2] = [hm, sig];
        let ps: [*const blst_p1_affine; 2] = [neg_key, blst_p1_affine_generator()];
        blst_miller_loop_n(&mut out, qs.as_ptr(), ps.as_ptr(), 2);
    }
    out
}

#[test]
fn the_rust_loop_equals_blst_and_the_folded_check_agrees() {
    const N: u64 = 2_000;
    let threads = thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let counts: Vec<(usize, usize)> = thread::scope(|s| {
        let handles: Vec<_> = (0..threads)
            .map(|t| {
                s.spawn(move || {
                    let (mut valid, mut invalid) = (0, 0);
                    for i in (t as u64..N).step_by(threads) {
                        let want_valid = i % 2 == 0;
                        let (hm, neg_key, sig) = pairs(i, want_valid);
                        let f = blst_product(&hm, &neg_key, &sig);
                        let (lh, ls) = (fold::precompute_lines(&hm), fold::precompute_lines(&sig));
                        // SAFETY: blst's static generator.
                        let g1 = unsafe { *blst_p1_affine_generator() };
                        let ours = fold::miller_loop(&[&lh, &ls], &[neg_key, g1], None);
                        assert_eq!(encode_fp12(&ours), encode_fp12(&f), "case {i}: the loop differs from blst's");
                        let truth = final_exp_is_one(&f);
                        assert_eq!(truth, want_valid, "case {i}");
                        let (c, w) = witness(&f);
                        let c_inv = inverse(&c);
                        let folded = fold::check_folded(&lh, &neg_key, &ls, &c, &c_inv, &w);
                        assert_eq!(folded.is_ok(), truth, "case {i}: folded check against final exponentiation");
                        assert_eq!(folded.is_ok(), check(&f, &c, &w).is_ok(), "case {i}: folded against plain");
                        if truth {
                            valid += 1;
                            // Tampered: c^-1 that is not the inverse, w outside Fp6, c flipped.
                            let not_inverse = mul(&c_inv, &c_inv);
                            assert_eq!(
                                fold::check_folded(&lh, &neg_key, &ls, &c, &not_inverse, &w),
                                Err(Refusal::ZeroResidue)
                            );
                            let mut w_out = w;
                            w_out.fp6[1] = one().fp6[0];
                            assert_eq!(
                                fold::check_folded(&lh, &neg_key, &ls, &c, &c_inv, &w_out),
                                Err(Refusal::ScalingNotInFp6)
                            );
                            let mut cb = encode_fp12(&c);
                            cb[47] ^= 1;
                            let c2 = decode_fp12(&cb).unwrap();
                            assert!(fold::check_folded(&lh, &neg_key, &ls, &c2, &inverse(&c2), &w).is_err());
                        } else {
                            invalid += 1;
                        }
                    }
                    (valid, invalid)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let (valid, invalid) = counts.iter().fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
    // c = 0 is refused (0 · c_inv != 1).
    let zero = decode_fp12(&[0u8; FP12_BYTES]).unwrap();
    let (hm, neg_key, sig) = pairs(0, true);
    let (lh, ls) = (fold::precompute_lines(&hm), fold::precompute_lines(&sig));
    assert_eq!(fold::check_folded(&lh, &neg_key, &ls, &zero, &zero, &zero), Err(Refusal::ZeroResidue));
    assert!(eq(&mul(&one(), &one()), &one()));
    println!(
        "fold: {N} products ({valid} valid, {invalid} invalid): the Rust loop equals blst_miller_loop_n byte for \
         byte; the folded check accepts exactly the valid ones, as the plain check and the final exponentiation; \
         {valid} x 3 tampered witnesses refused; c = 0 refused"
    );
}
