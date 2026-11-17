//! The embedded exponents, re-derived from p, r and u with num-bigint (independently of
//! gen_constants.py), every relation the soundness argument and the witness algorithm use checked
//! again, and the check's fast exponentiation compared with the generic one.

use blst::{blst_fp12, blst_fp_from_bendian};
use num_bigint::{BigInt, BigUint};
use residue::*;

fn big(be: &[u8]) -> BigUint {
    BigUint::from_bytes_be(be)
}

fn p() -> BigUint {
    BigUint::parse_bytes(
        b"1a0111ea397fe69a4b1ba7b6434bacd764774b84f38512bf6730d2a0f6b0f6241eabfffeb153ffffb9feffffffffaaab",
        16,
    )
    .unwrap()
}

fn r() -> BigUint {
    BigUint::parse_bytes(b"73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001", 16).unwrap()
}

fn gcd(a: &BigUint, b: &BigUint) -> BigUint {
    let (mut a, mut b) = (a.clone(), b.clone());
    while b != BigUint::from(0u8) {
        let t = &a % &b;
        a = b;
        b = t;
    }
    a
}

fn modinv(a: &BigUint, m: &BigUint) -> BigUint {
    // Extended Euclid over signed integers.
    let (mut old_r, mut r) = (BigInt::from(a.clone()), BigInt::from(m.clone()));
    let (mut old_s, mut s) = (BigInt::from(1), BigInt::from(0));
    while r != BigInt::from(0) {
        let q = &old_r / &r;
        let t = &old_r - &q * &r;
        old_r = r;
        r = t;
        let t = &old_s - &q * &s;
        old_s = s;
        s = t;
    }
    assert_eq!(old_r, BigInt::from(1), "not invertible");
    let m = BigInt::from(m.clone());
    (((old_s % &m) + &m) % &m).to_biguint().unwrap()
}

#[test]
fn exponents_rederived_and_relations_hold() {
    let (p, r) = (p(), r());
    let u_abs = BigUint::from(ABS_U);
    let one = BigUint::from(1u8);
    // p = (u-1)^2 (u^4-u^2+1)/3 + u and r = u^4 - u^2 + 1, with u = -|u|.
    let u = -BigInt::from(ABS_U);
    let (pi, ri) = (BigInt::from(p.clone()), BigInt::from(r.clone()));
    let (one_i, three_i) = (BigInt::from(1), BigInt::from(3));
    assert_eq!(ri, u.pow(4) - u.pow(2) + &one_i);
    assert_eq!(pi, (&u - &one_i).pow(2) * &ri / &three_i + &u);
    let n = p.pow(12) - &one;
    assert_eq!(&n % &r, BigUint::from(0u8));
    let h = &n / &r;
    assert_ne!(&h % &r, BigUint::from(0u8), "r^2 must not divide p^12 - 1");
    assert_eq!(big(&H), h);
    let lambda = &p + &u_abs;
    assert_eq!(big(&LAMBDA), lambda);
    assert_eq!(&lambda % &r, BigUint::from(0u8), "r | lambda");
    let poly = (BigUint::from(ABS_U) + &one) / BigUint::from(3u8);
    assert_eq!(poly, BigUint::from(POLY));
    assert_eq!(lambda, BigUint::from(3u8) * &poly * &poly * &r);
    let f = &h / (BigUint::from(27u8) * &poly);
    assert_eq!(&f * BigUint::from(27u8) * &poly, h);
    assert_eq!(big(&F), f);
    for (a, b) in [(&f, &BigUint::from(3u8)), (&f, &poly), (&poly, &BigUint::from(3u8)), (&f, &lambda)] {
        assert_eq!(gcd(a, b), one);
    }
    assert_eq!((p.pow(6) - &one) % (BigUint::from(27u8) * &poly), BigUint::from(0u8), "27 POLY | p^6 - 1");
    assert_eq!(&h % (p.pow(6) - &one), BigUint::from(0u8), "p^6 - 1 | h");
    assert_eq!(big(&LAMBDA_INV_MOD_F), modinv(&lambda, &f));
    let a = &poly - modinv(&(BigUint::from(27u8) * &f % &poly), &poly);
    let b = BigUint::from(27u8) - modinv(&(&poly * &f % BigUint::from(27u8)), &BigUint::from(27u8));
    assert_eq!(BigUint::from(A), a);
    assert_eq!(BigUint::from(B), b);
    assert_eq!(big(&W_EXP), BigUint::from(27u8) * &a + &poly * &b);
}

/// Deterministic pseudo-random Fp12 elements (splitmix64; coefficients reduced below p).
fn random_fp12(seed: u64) -> blst_fp12 {
    let mut s = seed;
    let mut next = || {
        s = s.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = s;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    };
    let mut f = blst_fp12::default();
    for fp in f.fp6.iter_mut().flat_map(|x| x.fp2.iter_mut()).flat_map(|x| x.fp.iter_mut()) {
        let mut b = [0u8; 48];
        for chunk in b.chunks_exact_mut(8) {
            chunk.copy_from_slice(&next().to_be_bytes());
        }
        b[0] &= 0x0f; // below 2^380 < p
                      // SAFETY: `b` holds the 48 bytes blst reads.
        unsafe { blst_fp_from_bendian(fp, b.as_ptr()) };
    }
    f
}

#[test]
fn the_fast_lambda_power_equals_the_generic_one() {
    for seed in 0..32 {
        let c = random_fp12(seed);
        assert!(eq(&pow_abs_u(&c), &pow_be(&c, &ABS_U.to_be_bytes())), "c^|u|, seed {seed}");
        assert!(eq(&pow_lambda(&c), &pow_be(&c, &LAMBDA)), "c^lambda, seed {seed}");
        // The encoding round-trips and is canonical.
        assert!(eq(&decode_fp12(&encode_fp12(&c)).unwrap(), &c));
    }
}
