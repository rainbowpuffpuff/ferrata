//! An independent check of the one library routine `Sum::Batch` rests on: blst's batch addition
//! (`blst_p1s_add`) against the chain of mixed additions the core uses, and against a third
//! reference (one scalar multiplication per distinct point), on multisets of G1 points built to
//! collide. Keys are drawn with repetition from a pool of at most eight points and their
//! negations, so that the pairwise rounds inside the batch addition meet doublings, cancelling
//! pairs, the identity as an intermediate sum, and sums of such results, at every length from 1
//! to 200 (below and above the 16 points at which blst starts its rounds).
//!
//! Experimental and unaudited, like the crate.
#![allow(unsafe_code)]

use blst::{
    blst_hash_to_g1, blst_p1, blst_p1_add_or_double, blst_p1_add_or_double_affine, blst_p1_affine,
    blst_p1_affine_serialize, blst_p1_cneg, blst_p1_from_affine, blst_p1_mult, blst_p1_to_affine, blst_p1s_add,
};

fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// A point of G1 (hash to curve clears the cofactor).
fn g1(i: u64) -> blst_p1_affine {
    let (mut p, mut a) = (blst_p1::default(), blst_p1_affine::default());
    let (msg, dst) = (i.to_le_bytes(), b"FERRATA-BENCH-BATCH-G1");
    // SAFETY: valid destinations; the lengths are those of the slices.
    unsafe {
        blst_hash_to_g1(&mut p, msg.as_ptr(), msg.len(), dst.as_ptr(), dst.len(), core::ptr::null(), 0);
        blst_p1_to_affine(&mut a, &p);
    }
    a
}

fn neg(a: &blst_p1_affine) -> blst_p1_affine {
    let (mut p, mut out) = (blst_p1::default(), blst_p1_affine::default());
    // SAFETY: valid points.
    unsafe {
        blst_p1_from_affine(&mut p, a);
        blst_p1_cneg(&mut p, true);
        blst_p1_to_affine(&mut out, &p);
    }
    out
}

fn bytes(p: &blst_p1) -> [u8; 96] {
    let (mut a, mut out) = (blst_p1_affine::default(), [0u8; 96]);
    // SAFETY: valid source and destinations.
    unsafe {
        blst_p1_to_affine(&mut a, p);
        blst_p1_affine_serialize(out.as_mut_ptr(), &a);
    }
    out
}

/// As `residue_agg::signing_key` calls it.
fn batch(points: &[blst_p1_affine]) -> [u8; 96] {
    let mut sum = blst_p1::default();
    let ptrs: [*const blst_p1_affine; 2] = [points.as_ptr(), core::ptr::null()];
    // SAFETY: `points` holds `points.len()` valid affine points, contiguous.
    unsafe { blst_p1s_add(&mut sum, ptrs.as_ptr(), points.len()) };
    bytes(&sum)
}

/// As the core's `add_affine`, once per point.
fn chain(points: &[blst_p1_affine]) -> [u8; 96] {
    let mut sum = blst_p1::default();
    for p in points {
        let acc: *mut blst_p1 = &mut sum;
        // SAFETY: blst allows the output to alias the first input.
        unsafe { blst_p1_add_or_double_affine(acc, acc, p) };
    }
    bytes(&sum)
}

/// Σ countᵢ · baseᵢ by one scalar multiplication per distinct point.
fn by_scalars(base: &[blst_p1_affine], counts: &[i64]) -> [u8; 96] {
    let mut sum = blst_p1::default();
    for (b, &c) in base.iter().zip(counts) {
        let (mut p, mut term) = (blst_p1::default(), blst_p1::default());
        let scalar = c.unsigned_abs().to_le_bytes();
        let acc: *mut blst_p1 = &mut sum;
        // SAFETY: valid points; the scalar has the 64 bits blst reads; aliasing allowed.
        unsafe {
            blst_p1_from_affine(&mut p, b);
            blst_p1_mult(&mut term, &p, scalar.as_ptr(), 64);
            blst_p1_cneg(&mut term, c < 0);
            blst_p1_add_or_double(acc, acc, &term);
        }
    }
    bytes(&sum)
}

#[test]
fn batch_addition_is_exact_on_colliding_multisets_of_g1_points() {
    let base: Vec<blst_p1_affine> = (0..8).map(g1).collect();
    let negs: Vec<blst_p1_affine> = base.iter().map(neg).collect();
    let identity = bytes(&blst_p1::default());
    let mut seed = 0x2026_1117_5eed_0001u64;
    let (mut cases, mut to_identity, mut with_rounds) = (0u32, 0u32, 0u32);
    let mut check = |picks: &[(usize, bool)]| {
        let points: Vec<blst_p1_affine> = picks.iter().map(|&(i, n)| if n { negs[i] } else { base[i] }).collect();
        let mut counts = [0i64; 8];
        for &(i, n) in picks {
            counts[i] += if n { -1 } else { 1 };
        }
        let (b, c, s) = (batch(&points), chain(&points), by_scalars(&base, &counts));
        assert_eq!(b, c, "batch against chain: {picks:?}");
        assert_eq!(b, s, "batch against scalar multiplications: {picks:?}");
        cases += 1;
        to_identity += u32::from(b == identity);
        with_rounds += u32::from(points.len() >= 16);
    };

    // Random multisets: a pool of 1 to 8 points, with or without negations, 1 to 200 keys.
    for _ in 0..20_000 {
        let pool = 1 + (splitmix(&mut seed) % 8) as usize;
        let len = 1 + (splitmix(&mut seed) % 200) as usize;
        let negations = splitmix(&mut seed) % 4 != 0;
        let picks: Vec<(usize, bool)> = (0..len)
            .map(|_| {
                let r = splitmix(&mut seed);
                ((r % pool as u64) as usize, negations && (r >> 40) & 1 == 1)
            })
            .collect();
        check(&picks);
    }
    // Structured: one key repeated; a key and its negation alternating, and in runs; pairs that
    // cancel followed by pairs that double; every length from 1 to 170.
    for len in 1..=170usize {
        check(&vec![(0, false); len]);
        check(&(0..len).map(|i| (0, i % 2 == 1)).collect::<Vec<_>>());
        check(&(0..len).map(|i| (0, (i / 2) % 2 == 1)).collect::<Vec<_>>());
        check(&(0..len).map(|i| (i / 4 % 8, i % 4 == 1)).collect::<Vec<_>>());
        check(&(0..len).map(|i| (i % 2, i % 4 >= 2)).collect::<Vec<_>>());
    }
    assert!(to_identity > 100 && with_rounds > 10_000, "the cases must reach the identity and the rounds");
    println!(
        "batch addition = chain = scalar multiplications on {cases} multisets of G1 points \
         ({with_rounds} of 16 or more points, {to_identity} summing to the identity)"
    );
}
