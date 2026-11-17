//! The residue witness folded into the Miller loop (experimental, unaudited, bench only).
//!
//! The check `f · w == c^λ = Frobenius(c) · c^|u|` spends 63 squarings on `c^|u|`. The Miller loop
//! squares its accumulator along the same bits of |u|, so seeding the accumulator with `c^-1` (as
//! gnark's `PairingCheck` and OpenVM's `try_honest_pairing_check` do) gets `f · c^-|u|` for the
//! price of the loop and a few multiplications, and the check becomes
//! `f · c^-|u| · w == Frobenius(c)`.
//!
//! blst offers no Miller loop with a seed, so [`miller_loop`] re-implements blst's two-pair loop
//! (`miller_loop_n` in blst's `pairing.c`) on blst's own line precomputation
//! (`blst_precompute_lines`, the lines of `precompute_lines`, evaluated at P as `post_line_by_Px2`
//! does) and blst's Fp12 arithmetic. Without a seed it returns blst's product byte for byte
//! (`tests/fold.rs`). With the seed `s = conj(c^-1)` it returns `conj(Π lines · s^|u|) = f ·
//! conj(s)^|u| = f · c^-|u|`, the conjugation at the end being blst's own (the seed u is negative).
//!
//! The submitter supplies `c`, `c^-1` and `w`; the program checks `c · c^-1 == 1` (so `c ≠ 0`),
//! `w ∈ Fp6`, and `g · w == Frobenius(c)` for the seeded loop's `g`. That equation holds iff
//! `f · w == c^λ` (multiply both sides by `c^|u|`), so the soundness argument is the plain check's.

use blst::{
    blst_fp, blst_fp12, blst_fp12_conjugate, blst_fp12_mul_by_xy00z0, blst_fp2, blst_fp6, blst_fp_add, blst_fp_cneg,
    blst_fp_mul, blst_p1_affine, blst_p1_affine_generator, blst_p2_affine, blst_precompute_lines,
};

use crate::{eq, frobenius, in_fp6, mul, one, sqr, Refusal};

/// Number of lines blst precomputes for one G2 point (`precompute_lines`).
pub const LINES: usize = 68;

/// blst's addition-then-doublings schedule over |u| = 0xd201000000010000: (index of the addition
/// line, number of doublings after it), after the first doubling at index 0.
const SCHEDULE: [(usize, usize); 5] = [(1, 2), (4, 3), (8, 9), (18, 32), (51, 16)];

fn zero_fp() -> blst_fp {
    // SAFETY: `blst_fp` is an array of integer limbs (six of 64 bits natively, twelve of 32 bits in
    // the guest); all-zero limbs are the field's zero in Montgomery form.
    unsafe { core::mem::zeroed() }
}

/// The zero of Fp12 (in the `blst` crate, `blst_fp12::default()` is one).
fn zero_fp12() -> blst_fp12 {
    let fp2 = blst_fp2 { fp: [zero_fp(); 2] };
    let fp6 = blst_fp6 { fp2: [fp2; 3] };
    blst_fp12 { fp6: [fp6; 2] }
}

/// blst's 68 lines of `q`.
pub fn precompute_lines(q: &blst_p2_affine) -> Vec<blst_fp6> {
    let fp2 = blst_fp2 { fp: [zero_fp(); 2] };
    let mut lines = vec![blst_fp6 { fp2: [fp2; 3] }; LINES];
    // SAFETY: `lines` holds the 68 elements blst writes; `q` is a valid affine point.
    unsafe { blst_precompute_lines(lines.as_mut_ptr(), q) };
    lines
}

/// `(−2·P.x, 2·P.y)`, as blst's loop prepares P.
fn px2(p: &blst_p1_affine) -> (blst_fp, blst_fp) {
    let (mut x, mut y) = (zero_fp(), zero_fp());
    // SAFETY: valid field elements; blst allows the output to alias an input.
    unsafe {
        blst_fp_add(&mut x, &p.x, &p.x);
        let x2 = x;
        blst_fp_cneg(&mut x, &x2, true);
        blst_fp_add(&mut y, &p.y, &p.y);
    }
    (x, y)
}

/// A precomputed line evaluated at P (`post_line_by_Px2`).
fn at(line: &blst_fp6, p: &(blst_fp, blst_fp)) -> blst_fp6 {
    let mut out = *line;
    // SAFETY: valid field elements.
    unsafe {
        blst_fp_mul(&mut out.fp2[1].fp[0], &line.fp2[1].fp[0], &p.0);
        blst_fp_mul(&mut out.fp2[1].fp[1], &line.fp2[1].fp[1], &p.0);
        blst_fp_mul(&mut out.fp2[2].fp[0], &line.fp2[2].fp[0], &p.1);
        blst_fp_mul(&mut out.fp2[2].fp[1], &line.fp2[2].fp[1], &p.1);
    }
    out
}

fn mul_by_line(acc: &mut blst_fp12, line: &blst_fp6) {
    let a = *acc;
    // SAFETY: valid destination and sources.
    unsafe { blst_fp12_mul_by_xy00z0(acc, &a, line) };
}

/// The Miller loop over the pairs `(lines[i], ps[i])`, as blst's `miller_loop_n`, with the
/// accumulator seeded by `seed` along the bits of |u| (so the result carries `conj(seed)^|u|`).
pub fn miller_loop(lines: &[&[blst_fp6]], ps: &[blst_p1_affine], seed: Option<&blst_fp12>) -> blst_fp12 {
    assert!(!lines.is_empty() && lines.len() == ps.len() && lines.iter().all(|l| l.len() == LINES));
    let px: Vec<_> = ps.iter().map(px2).collect();
    // The first doubling: ret = line (1^2 * line), then times the other pairs' first lines.
    let first = at(&lines[0][0], &px[0]);
    let mut acc = zero_fp12();
    acc.fp6[0].fp2[0] = first.fp2[0];
    acc.fp6[0].fp2[1] = first.fp2[1];
    acc.fp6[1].fp2[1] = first.fp2[2];
    for i in 1..lines.len() {
        mul_by_line(&mut acc, &at(&lines[i][0], &px[i]));
    }
    // The seed's exponent follows |u|'s prefix: 0x2 here.
    if let Some(s) = seed {
        acc = mul(&acc, &sqr(s));
    }
    for (add, doublings) in SCHEDULE {
        for i in 0..lines.len() {
            mul_by_line(&mut acc, &at(&lines[i][add], &px[i]));
        }
        if let Some(s) = seed {
            acc = mul(&acc, s);
        }
        for d in 1..=doublings {
            acc = sqr(&acc);
            for i in 0..lines.len() {
                mul_by_line(&mut acc, &at(&lines[i][add + d], &px[i]));
            }
        }
    }
    // SAFETY: valid element.
    unsafe { blst_fp12_conjugate(&mut acc) };
    acc
}

/// The conjugate of `a` (`a^(p^6)`).
pub fn conjugate(a: &blst_fp12) -> blst_fp12 {
    let mut out = *a;
    // SAFETY: valid element.
    unsafe { blst_fp12_conjugate(&mut out) };
    out
}

/// The folded check for the signature pairs `(H(m), −key)` and `(signature, g1)`: `c · c_inv == 1`,
/// `w ∈ Fp6`, and `ML · c^-|u| · w == Frobenius(c)` with the loop seeded by `conj(c_inv)`.
pub fn check_folded(
    hm_lines: &[blst_fp6],
    neg_key: &blst_p1_affine,
    sig_lines: &[blst_fp6],
    c: &blst_fp12,
    c_inv: &blst_fp12,
    w: &blst_fp12,
) -> Result<(), Refusal> {
    if !eq(&mul(c, c_inv), &one()) {
        return Err(Refusal::ZeroResidue);
    }
    if !in_fp6(w) {
        return Err(Refusal::ScalingNotInFp6);
    }
    // SAFETY: blst's static generator.
    let g1 = unsafe { *blst_p1_affine_generator() };
    let g = miller_loop(&[hm_lines, sig_lines], &[*neg_key, g1], Some(&conjugate(c_inv)));
    if eq(&mul(&g, w), &frobenius(c)) {
        Ok(())
    } else {
        Err(Refusal::Mismatch)
    }
}
