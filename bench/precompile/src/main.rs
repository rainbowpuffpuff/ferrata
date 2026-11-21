//! RFP-021 P.8: the native cost of the BLS12-381 operations one light-client update needs, for the
//! model of a hypothetical LEZ precompile (`docs/performance.md`, RFP-021 §P.8). Native blst
//! 0.3.16, the library the attestation program's guest runs in RISC Zero's accelerated build.
//!
//! The operations, as the program's steps perform them for a 512-member committee:
//! `Open`: hash-to-G2 of the signing root and the subtractive aggregation of the signing key
//! (the committee's aggregate minus the non-signers); `Miller`: two Miller loops and their
//! product; `Finish`: the final exponentiation and the check against one; `Adopt` (a handoff,
//! per part of 256 keys): each uncompressed key deserialized and checked on the curve,
//! recompressed and compared with the committed key, and added to the sum.
//!
//! `cargo run --release -- [RUNS]` prints one tab-separated line per operation: the median
//! wall time of RUNS repetitions (default 200, after 10 warm-up runs), single-threaded.

use std::{hint::black_box, time::Instant};

use blst::{
    min_pk::{AggregatePublicKey, AggregateSignature, PublicKey, SecretKey, Signature},
    *,
};

const DST: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";

fn median_ns(runs: usize, mut f: impl FnMut()) -> f64 {
    for _ in 0..10 {
        f();
    }
    let mut v: Vec<f64> = (0..runs)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed().as_nanos() as f64
        })
        .collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn p1_deserialize(b: &[u8; 96]) -> blst_p1_affine {
    let mut a = blst_p1_affine::default();
    let e = unsafe { blst_p1_deserialize(&mut a, b.as_ptr()) };
    assert_eq!(e, BLST_ERROR::BLST_SUCCESS);
    a
}

fn main() {
    let runs: usize = std::env::args().nth(1).and_then(|r| r.parse().ok()).unwrap_or(200);
    let sks: Vec<SecretKey> = (0..512u32)
        .map(|i| {
            let mut ikm = [7u8; 32];
            ikm[..4].copy_from_slice(&i.to_le_bytes());
            SecretKey::key_gen(&ikm, &[]).unwrap()
        })
        .collect();
    let pks: Vec<PublicKey> = sks.iter().map(SecretKey::sk_to_pk).collect();
    let msg = [0x42u8; 32];
    let sigs: Vec<Signature> = sks.iter().map(|s| s.sign(&msg, DST, &[])).collect();
    let agg_sig = AggregateSignature::aggregate(&sigs.iter().collect::<Vec<_>>(), true).unwrap().to_signature();
    let agg_pk = AggregatePublicKey::aggregate(&pks.iter().collect::<Vec<_>>(), true).unwrap().to_public_key();
    let pk_ser: Vec<[u8; 96]> = pks.iter().map(PublicKey::serialize).collect();
    let pk_cmp: Vec<[u8; 48]> = pks.iter().map(PublicKey::compress).collect();
    let agg_ser = agg_pk.serialize();

    let mut out: Vec<(&str, f64)> = Vec::new();

    // Open: hash-to-G2 of the signing root, then to affine.
    let mut hm = blst_p2_affine::default();
    out.push((
        "hash_to_g2",
        median_ns(runs, || unsafe {
            let mut p = blst_p2::default();
            blst_hash_to_g2(&mut p, msg.as_ptr(), msg.len(), DST.as_ptr(), DST.len(), std::ptr::null(), 0);
            blst_p2_to_affine(&mut hm, &p);
            black_box(&hm);
        }),
    ));

    // Open: the signing key, subtractively (aggregate minus k non-signers, negated for the check).
    let mut neg_key = blst_p1_affine::default();
    for (name, k) in [("aggregate_subtract_2", 2usize), ("aggregate_subtract_170", 170)] {
        out.push((
            name,
            median_ns(runs, || unsafe {
                let agg = p1_deserialize(&agg_ser);
                let mut acc = blst_p1::default();
                for b in &pk_ser[..k] {
                    let a = p1_deserialize(b);
                    blst_p1_add_or_double_affine(&mut acc, &acc, &a);
                }
                // key = agg - sum(non-signers); the check uses -key.
                blst_p1_cneg(&mut acc, true);
                blst_p1_add_or_double_affine(&mut acc, &acc, &agg);
                blst_p1_cneg(&mut acc, true);
                blst_p1_to_affine(&mut neg_key, &acc);
                black_box(&neg_key);
            }),
        ));
    }
    // The key with every member signing (no non-signer), for the real check below.
    unsafe {
        let mut p = blst_p1::default();
        let agg = p1_deserialize(&agg_ser);
        blst_p1_from_affine(&mut p, &agg);
        blst_p1_cneg(&mut p, true);
        blst_p1_to_affine(&mut neg_key, &p);
    }

    // Miller: ML(H(m), -key) * ML(sig, g1).
    let sig_aff = {
        let mut a = blst_p2_affine::default();
        let e = unsafe { blst_p2_deserialize(&mut a, agg_sig.serialize().as_ptr()) };
        assert_eq!(e, BLST_ERROR::BLST_SUCCESS);
        a
    };
    let g1 = unsafe { *blst_p1_affine_generator() };
    let mut f = blst_fp12::default();
    out.push((
        "miller_loops_2_and_mul",
        median_ns(runs, || unsafe {
            let mut f2 = blst_fp12::default();
            blst_miller_loop(&mut f, &hm, &neg_key);
            blst_miller_loop(&mut f2, &sig_aff, &g1);
            blst_fp12_mul(&mut f, &f, &f2);
            black_box(&f);
        }),
    ));

    // Finish: final exponentiation and the check against one (it must hold: every member signed).
    let mut ok = false;
    out.push((
        "final_exp_and_is_one",
        median_ns(runs, || unsafe {
            let mut r = blst_fp12::default();
            blst_final_exp(&mut r, &f);
            ok = blst_fp12_is_one(&r);
            black_box(ok);
        }),
    ));
    assert!(ok, "the aggregate signature verifies");

    // Adopt, one part of 256 uncompressed keys.
    out.push((
        "adopt_256_uncompressed",
        median_ns(runs.min(50), || unsafe {
            let mut acc = blst_p1::default();
            for (b, c) in pk_ser[..256].iter().zip(&pk_cmp[..256]) {
                let a = p1_deserialize(b);
                let mut again = [0u8; 48];
                blst_p1_affine_compress(again.as_mut_ptr(), &a);
                assert_eq!(&again, c);
                blst_p1_add_or_double_affine(&mut acc, &acc, &a);
            }
            black_box(&acc);
        }),
    ));

    // For reference: subgroup checks the program does not make per key, and blst's whole
    // fast_aggregate_verify over 512 keys (keys aggregated from scratch, no key validation).
    out.push((
        "g1_subgroup_check",
        median_ns(runs, || unsafe {
            black_box(blst_p1_affine_in_g1(&p1_deserialize(&pk_ser[0])));
        }),
    ));
    out.push((
        "g2_subgroup_check",
        median_ns(runs, || unsafe {
            black_box(blst_p2_affine_in_g2(&sig_aff));
        }),
    ));
    let refs: Vec<&PublicKey> = pks.iter().collect();
    out.push((
        "fast_aggregate_verify_512",
        median_ns(runs.min(50), || {
            let r = agg_sig.fast_aggregate_verify(false, &msg, DST, &refs);
            assert_eq!(r, BLST_ERROR::BLST_SUCCESS);
        }),
    ));

    println!("op\tmedian_ns\truns");
    for (name, ns) in out {
        println!("{name}\t{ns:.0}\t{runs}");
    }
}
