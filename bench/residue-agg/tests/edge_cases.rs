//! Edge cases of `residue_agg` (experimental, unaudited), written by an independent review of the
//! composition (18 Nov 2026), apart from the crate's own tests (`agg.rs`, `batch_g1.rs`) and with
//! references built here. Native only; `bench/RESIDUE.md` §9.5.
//!
//! 1. The multiproof exhaustively: every non-empty leaf set at depths 1 to 4 round trips, equals
//!    one branch per leaf, needs `hashes − leaves + 1` witness nodes (never more than the tree is
//!    wide: the decoder's bound), and every tampering is refused.
//! 2. Committees whose size is not a power of two, and the minimal preset's 32: the honest
//!    multiproof is accepted and the key is `aggregate − Σ absent` under both sums.
//! 3. The identity: non-signers that sum to it, signers that sum to it (`SigningKeyIdentity`), an
//!    aggregate that does not decode (`PubkeyInvalid`), no non-signer with a node supplied
//!    (`Witness`), one key at many absent positions.
//! 4. `NonSigners::decode` at its edges (counts at and past the committee size, trailing and
//!    missing bytes).
//! 5. blst's batch addition on G1 points with many copies, cancellations and large sets: equal to
//!    the chain of additions and to scalar multiplications.
//! 6. The same outside G1 (curve points with shifts by the points of order 3): where the batch sum
//!    differs from the chain it differs by (0, ±2), and the pairing with a G2 point is equal.
//! 7. `compressed` against blst on curve points outside G1.
//!
//! Each test prints one line with its counts (`results/residue/agg/edge-cases.txt`):
//! `cargo test --release -p residue-agg --test edge_cases -- --nocapture`
#![allow(unsafe_code)]

use blst::{
    blst_final_exp, blst_fp, blst_fp12, blst_fp12_is_equal, blst_fp_from_bendian, blst_hash_to_g1, blst_hash_to_g2,
    blst_miller_loop, blst_p1, blst_p1_add_or_double, blst_p1_add_or_double_affine, blst_p1_affine,
    blst_p1_affine_compress, blst_p1_affine_in_g1, blst_p1_affine_is_inf, blst_p1_affine_serialize, blst_p1_cneg,
    blst_p1_compress, blst_p1_from_affine, blst_p1_is_inf, blst_p1_mult, blst_p1_to_affine, blst_p1_uncompress,
    blst_p1s_add, blst_p2, blst_p2_affine, blst_p2_to_affine, BLST_ERROR,
};
use ferrata_core::{
    merkle::is_valid_merkle_branch,
    steps::StepError,
    types::{BlsPubkey, SyncCommittee},
    witness::{key_branch_depth, pubkey_leaf, uncompress_key, CommitteeSummary, SignerWitness},
    Error, Root,
};
use ferrata_spectests::testkit::fake_committee;
use residue_agg::*;

fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

fn hashes_for(indices: &[usize], depth: usize) -> usize {
    let mut level: std::collections::BTreeSet<usize> = indices.iter().copied().collect();
    let mut total = 0;
    for _ in 0..depth {
        level = level.iter().map(|i| i >> 1).collect();
        total += level.len();
    }
    total
}

// ---------------------------------------------------------------------------------------------
// 1. The multiproof, exhaustively at small depths, against one branch per leaf
// ---------------------------------------------------------------------------------------------

#[test]
fn multiproof_exhaustive_small_depths() {
    let mut s = 99u64;
    let (mut sets, mut tamper) = (0usize, 0usize);
    for depth in 1..=4usize {
        let width = 1usize << depth;
        let leaves: Vec<Root> = (0..width).map(|i| hash64(&[(i as u8).wrapping_mul(37) ^ 0x5a; 64])).collect();
        let tree = layers(&leaves, width);
        let root = tree[depth][0];
        for mask in 1u32..(1u32 << width) {
            let idx: Vec<usize> = (0..width).filter(|i| mask >> i & 1 == 1).collect();
            let at: Vec<(u32, Root)> = idx.iter().map(|&i| (i as u32, leaves[i])).collect();
            let sib = multiproof_siblings(&tree, &idx);
            assert_eq!(multiproof_root(&at, &sib, depth), Some(root));
            let h = hashes_for(&idx, depth);
            assert_eq!(sib.len(), h - idx.len() + 1, "siblings = hashes - leaves + 1");
            assert!(sib.len() <= width, "the decoder's bound");
            // Each leaf also verifies by a single branch (the core's check) against the same root.
            for &i in &idx {
                let branch: Vec<Root> = (0..depth).map(|l| tree[l][(i >> l) ^ 1]).collect();
                assert!(is_valid_merkle_branch(&leaves[i], &branch, depth, i as u64, &root));
            }
            // Depth confusion: the same proof at another depth never gives the root.
            if depth > 1 {
                assert_ne!(multiproof_root(&at, &sib, depth - 1), Some(root));
            }
            assert_ne!(multiproof_root(&at, &sib, depth + 1), Some(root));
            // A leaf moved to another free position, or replaced by another leaf's value.
            let free: Vec<usize> = (0..width).filter(|i| mask >> i & 1 == 0).collect();
            if !free.is_empty() {
                let j = free[(splitmix(&mut s) as usize) % free.len()];
                let mut moved: Vec<(u32, Root)> = at.clone();
                moved[0].0 = j as u32;
                moved.sort_by_key(|x| x.0);
                assert_ne!(multiproof_root(&moved, &sib, depth), Some(root));
                let mut other = at.clone();
                other[0].1 = leaves[j];
                assert_ne!(multiproof_root(&other, &sib, depth), Some(root));
                tamper += 2;
            }
            // A leaf's value that is an interior node of the tree (a level-1 node at a leaf position).
            let mut interior = at.clone();
            interior[0].1 = tree[1][idx[0] >> 1];
            assert_ne!(multiproof_root(&interior, &sib, depth), Some(root));
            // Every single sibling replaced by every other node of the tree (any level).
            if sib.len() <= 4 && depth <= 3 {
                for k in 0..sib.len() {
                    for lvl in &tree {
                        for nd in lvl {
                            if *nd == sib[k] {
                                continue;
                            }
                            let mut t = sib.clone();
                            t[k] = *nd;
                            assert_ne!(multiproof_root(&at, &t, depth), Some(root));
                            tamper += 1;
                        }
                    }
                }
            }
            sets += 1;
        }
    }
    println!("multiproof: every non-empty leaf set at depths 1-4 ({sets} sets) round trips, equals the per-leaf branches, siblings = hashes - leaves + 1 <= width; {tamper} tamperings refused");
}

// ---------------------------------------------------------------------------------------------
// 2. Committees whose size is not a power of two, and the minimal preset
// ---------------------------------------------------------------------------------------------

fn point(pk: &BlsPubkey) -> blst_p1_affine {
    let mut a = blst_p1_affine::default();
    // SAFETY: 48 bytes read.
    assert_eq!(unsafe { blst_p1_uncompress(&mut a, pk.as_ptr()) }, BLST_ERROR::BLST_SUCCESS);
    a
}

/// aggregate − Σ absent, one mixed addition each (a reference built here).
fn reference_key(c: &SyncCommittee, bits: &[bool]) -> [u8; 96] {
    let mut acc = blst_p1::default();
    for (pk, _) in c.pubkeys.iter().zip(bits).filter(|(_, b)| !**b) {
        let p = point(pk);
        let a: *mut blst_p1 = &mut acc;
        // SAFETY: aliasing allowed.
        unsafe { blst_p1_add_or_double_affine(a, a, &p) };
    }
    let mut key = blst_p1::default();
    let agg = point(&c.aggregate_pubkey);
    // SAFETY: valid points.
    unsafe {
        blst_p1_from_affine(&mut key, &agg);
        blst_p1_cneg(&mut acc, true);
        let k: *mut blst_p1 = &mut key;
        blst_p1_add_or_double(k, k, &acc);
    }
    neg_key_bytes(&key)
}

#[test]
fn non_power_of_two_and_minimal_committees() {
    let mut s = 7u64;
    let mut checked = 0usize;
    for n in [1usize, 2, 3, 5, 6, 7, 9, 12, 32, 33, 100, 300, 511, 513] {
        let fake = fake_committee(n, 41);
        let c = &fake.committee;
        let summary = CommitteeSummary::of(c);
        let depth = key_branch_depth(n);
        let mut patterns: Vec<Vec<bool>> = vec![vec![true; n], vec![false; n]];
        let mut first = vec![true; n];
        first[0] = false;
        patterns.push(first);
        let mut last = vec![true; n];
        last[n - 1] = false;
        patterns.push(last);
        for _ in 0..12 {
            patterns.push((0..n).map(|_| !splitmix(&mut s).is_multiple_of(3)).collect());
        }
        for bits in patterns {
            let w = NonSigners::build(c, &bits).unwrap();
            assert_eq!(NonSigners::decode(&w.encode(), n).as_ref(), Ok(&w), "n = {n}: decode round trip");
            let absent = bits.iter().filter(|b| !**b).count();
            // The core's branches verify against the same root at the same depth.
            let old = SignerWitness::build(c, &bits).unwrap();
            for ((i, _), br) in bits.iter().enumerate().filter(|(_, b)| !**b).zip(&old.branches) {
                assert!(is_valid_merkle_branch(
                    &pubkey_leaf(&c.pubkeys[i]),
                    br,
                    depth,
                    i as u64,
                    &summary.pubkeys_root
                ));
            }
            let want = reference_key(c, &bits);
            for sum in [Sum::Batch, Sum::Chain] {
                match signing_key(&summary, &bits, &w, sum) {
                    Ok(k) => assert_eq!(neg_key_bytes(&k), want, "n = {n}, {absent} absent, {sum:?}"),
                    Err(e) => {
                        // Only the identity (all absent: aggregate − aggregate).
                        assert_eq!(e, StepError::from(Error::SigningKeyIdentity), "n = {n}, {absent} absent");
                        assert_eq!(absent, n);
                    }
                }
            }
            // One altered sibling: refused.
            if !w.siblings.is_empty() {
                let mut t = w.clone();
                let j = (splitmix(&mut s) as usize) % t.siblings.len();
                t.siblings[j][7] ^= 0x10;
                assert_eq!(signing_key(&summary, &bits, &t, Sum::Batch).unwrap_err(), StepError::Witness);
            }
            checked += 1;
        }
    }
    println!("committees of 1, 2, 3, 5, 6, 7, 9, 12, 32 (minimal), 33, 100, 300, 511, 513 members: {checked} bitfields; the honest multiproof accepted, the key = aggregate − Σ absent (batch and chain), the core's branches verify against the same root, an altered sibling refused");
}

// ---------------------------------------------------------------------------------------------
// 3. The identity: non-signers summing to it, the signing key equal to it, the empty set
// ---------------------------------------------------------------------------------------------

fn neg(pk: &BlsPubkey) -> BlsPubkey {
    let a = point(pk);
    let mut p = blst_p1::default();
    let mut out = [0u8; 48];
    // SAFETY: valid inputs and outputs.
    unsafe {
        blst_p1_from_affine(&mut p, &a);
        blst_p1_cneg(&mut p, true);
        blst_p1_compress(out.as_mut_ptr(), &p);
    }
    out
}

fn sum_keys(keys: &[BlsPubkey]) -> BlsPubkey {
    let mut acc = blst_p1::default();
    for k in keys {
        let a = point(k);
        let p: *mut blst_p1 = &mut acc;
        // SAFETY: aliasing allowed.
        unsafe { blst_p1_add_or_double_affine(p, p, &a) };
    }
    let mut out = [0u8; 48];
    // SAFETY: valid point.
    unsafe { blst_p1_compress(out.as_mut_ptr(), &acc) };
    out
}

#[test]
fn identity_cases() {
    let fake = fake_committee(512, 51);
    // (a) The 170 non-signers cancel in pairs: the key is the aggregate.
    let mut c = fake.committee.clone();
    for i in 0..85 {
        c.pubkeys[2 * i + 1] = neg(&c.pubkeys[2 * i]);
    }
    c.aggregate_pubkey = sum_keys(&c.pubkeys);
    let summary = CommitteeSummary::of(&c);
    let bits: Vec<bool> = (0..512).map(|i| i >= 170).collect();
    let w = NonSigners::build(&c, &bits).unwrap();
    let agg = point(&c.aggregate_pubkey);
    let mut agg_p = blst_p1::default();
    // SAFETY: valid point.
    unsafe { blst_p1_from_affine(&mut agg_p, &agg) };
    for sum in [Sum::Batch, Sum::Chain] {
        let k = signing_key(&summary, &bits, &w, sum).unwrap();
        assert_eq!(neg_key_bytes(&k), neg_key_bytes(&agg_p), "(a) {sum:?}: the non-signers sum to the identity");
    }
    // Interleaved: every third absent, the absent ones cancelling pairwise across the committee.
    let mut c2 = fake.committee.clone();
    let absent: Vec<usize> = (0..512).filter(|i| i % 3 == 2).collect();
    for p in absent.chunks_exact(2) {
        c2.pubkeys[p[1]] = neg(&c2.pubkeys[p[0]]);
    }
    c2.aggregate_pubkey = sum_keys(&c2.pubkeys);
    let s2 = CommitteeSummary::of(&c2);
    let bits2: Vec<bool> = (0..512).map(|i| i % 3 != 2).collect();
    let w2 = NonSigners::build(&c2, &bits2).unwrap();
    let a2 = signing_key(&s2, &bits2, &w2, Sum::Batch).unwrap();
    let b2 = signing_key(&s2, &bits2, &w2, Sum::Chain).unwrap();
    assert_eq!(neg_key_bytes(&a2), neg_key_bytes(&b2));
    assert_eq!(neg_key_bytes(&a2), reference_key(&c2, &bits2));

    // (b) The 342 signers cancel in pairs: aggregate = Σ non-signers, the key is the identity.
    let mut c3 = fake.committee.clone();
    for i in 0..171 {
        c3.pubkeys[170 + 2 * i + 1] = neg(&c3.pubkeys[170 + 2 * i]);
    }
    c3.aggregate_pubkey = sum_keys(&c3.pubkeys);
    let s3 = CommitteeSummary::of(&c3);
    let w3 = NonSigners::build(&c3, &bits).unwrap();
    for sum in [Sum::Batch, Sum::Chain] {
        assert_eq!(
            signing_key(&s3, &bits, &w3, sum).unwrap_err(),
            StepError::from(Error::SigningKeyIdentity),
            "(b) {sum:?}"
        );
    }
    // (c) Every key cancels: the aggregate is the identity's encoding; the key is the identity.
    let mut c4 = fake.committee.clone();
    for i in 0..256 {
        c4.pubkeys[2 * i + 1] = neg(&c4.pubkeys[2 * i]);
    }
    c4.aggregate_pubkey = sum_keys(&c4.pubkeys);
    assert_eq!(c4.aggregate_pubkey[0], 0xc0, "the identity's encoding");
    let s4 = CommitteeSummary::of(&c4);
    for b in [bits.clone(), vec![true; 512]] {
        let w4 = NonSigners::build(&c4, &b).unwrap();
        for sum in [Sum::Batch, Sum::Chain] {
            let r = signing_key(&s4, &b, &w4, sum);
            let all_present = b.iter().all(|x| *x);
            if all_present {
                assert_eq!(r.unwrap_err(), StepError::from(Error::SigningKeyIdentity), "(c) none absent");
            } else {
                // 170 absent members cancel; the key is the aggregate, the identity.
                assert_eq!(r.unwrap_err(), StepError::from(Error::SigningKeyIdentity), "(c) {sum:?}");
            }
        }
    }
    // (d) An aggregate that does not decompress: PubkeyInvalid, before any key is looked at.
    let mut s5 = CommitteeSummary::of(&fake.committee);
    s5.aggregate_pubkey = [0xff; 48];
    let w5 = NonSigners::build(&fake.committee, &bits).unwrap();
    for sum in [Sum::Batch, Sum::Chain] {
        assert_eq!(signing_key(&s5, &bits, &w5, sum).unwrap_err(), StepError::from(Error::PubkeyInvalid));
    }
    // (e) No non-signer: no node may be supplied; with none, the key is the aggregate.
    let s6 = CommitteeSummary::of(&fake.committee);
    let all = vec![true; 512];
    let empty = NonSigners::default();
    assert!(signing_key(&s6, &all, &empty, Sum::Batch).is_ok());
    let extra = NonSigners { keys: vec![], siblings: vec![s6.pubkeys_root] };
    assert_eq!(signing_key(&s6, &all, &extra, Sum::Batch).unwrap_err(), StepError::Witness);
    // (f) The same key at two absent positions (a committee with a duplicate): batch = chain = reference.
    let mut c7 = fake.committee.clone();
    for i in 0..60 {
        c7.pubkeys[3 * i + 5] = c7.pubkeys[2];
    }
    c7.aggregate_pubkey = sum_keys(&c7.pubkeys);
    let s7 = CommitteeSummary::of(&c7);
    let bits7: Vec<bool> = (0..512).map(|i| !(i == 2 || (i >= 5 && (i - 5) % 3 == 0 && i < 185))).collect();
    let w7 = NonSigners::build(&c7, &bits7).unwrap();
    let a7 = signing_key(&s7, &bits7, &w7, Sum::Batch).unwrap();
    assert_eq!(neg_key_bytes(&a7), reference_key(&c7, &bits7), "(f) duplicates");
    println!("identity: non-signers cancelling (contiguous, every third), signers cancelling (key = identity: SigningKeyIdentity by both sums), all keys cancelling, an undecodable aggregate (PubkeyInvalid), the empty set with a node (Witness), 61 copies of one key absent: as the reference");
}

// ---------------------------------------------------------------------------------------------
// 4. NonSigners::decode at its edges
// ---------------------------------------------------------------------------------------------

#[test]
fn decode_edges() {
    let n = 512usize;
    let enc = |k: usize, s: usize, extra: isize| -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&(k as u16).to_le_bytes());
        b.extend(std::iter::repeat_n(0u8, 96 * k));
        b.extend_from_slice(&(s as u16).to_le_bytes());
        b.extend(std::iter::repeat_n(0u8, 32 * s));
        if extra > 0 {
            b.extend(std::iter::repeat_n(0u8, extra as usize));
        } else {
            b.truncate((b.len() as isize + extra) as usize);
        }
        b
    };
    assert!(NonSigners::decode(&enc(0, 0, 0), n).is_ok());
    assert!(NonSigners::decode(&enc(n, n, 0), n).is_ok());
    assert_eq!(NonSigners::decode(&enc(n + 1, 0, 0), n), Err(Error::Malformed));
    assert_eq!(NonSigners::decode(&enc(0, n + 1, 0), n), Err(Error::Malformed));
    assert_eq!(NonSigners::decode(&enc(3, 4, 1), n), Err(Error::Malformed));
    assert_eq!(NonSigners::decode(&enc(3, 4, -1), n), Err(Error::Malformed));
    assert_eq!(NonSigners::decode(&[], n), Err(Error::Malformed));
    assert_eq!(NonSigners::decode(&[0], n), Err(Error::Malformed));
    assert_eq!(NonSigners::decode(&[0, 0, 0], n), Err(Error::Malformed));
    assert_eq!(NonSigners::decode(&[0xff, 0xff], n), Err(Error::Malformed));
    // A key count whose keys run past the input: the sibling count is read beyond the end.
    let mut b = enc(2, 0, 0);
    b[0] = 3;
    assert_eq!(NonSigners::decode(&b, n), Err(Error::Malformed));
    // n = 65535: the largest counts.
    assert!(NonSigners::decode(&enc(0, 65535, 0), 65535).is_ok());
    println!("decode: counts at n and n + 1, a trailing and a missing byte, empty and short inputs, a key count past the end: as specified");
}

// ---------------------------------------------------------------------------------------------
// 5. blst's batch addition: G1, with many copies, cancellations and large sets
// ---------------------------------------------------------------------------------------------

fn g1(i: u64) -> blst_p1_affine {
    let (mut p, mut a) = (blst_p1::default(), blst_p1_affine::default());
    let (msg, dst) = (i.to_le_bytes(), b"REVIEW-LP-G1");
    // SAFETY: valid destinations.
    unsafe {
        blst_hash_to_g1(&mut p, msg.as_ptr(), msg.len(), dst.as_ptr(), dst.len(), core::ptr::null(), 0);
        blst_p1_to_affine(&mut a, &p);
    }
    a
}

fn neg_a(a: &blst_p1_affine) -> blst_p1_affine {
    let (mut p, mut out) = (blst_p1::default(), blst_p1_affine::default());
    // SAFETY: valid points.
    unsafe {
        blst_p1_from_affine(&mut p, a);
        blst_p1_cneg(&mut p, true);
        blst_p1_to_affine(&mut out, &p);
    }
    out
}

fn add_a(a: &blst_p1_affine, b: &blst_p1_affine) -> blst_p1_affine {
    let (mut p, mut out) = (blst_p1::default(), blst_p1_affine::default());
    // SAFETY: valid points; aliasing allowed.
    unsafe {
        blst_p1_from_affine(&mut p, a);
        let pp: *mut blst_p1 = &mut p;
        blst_p1_add_or_double_affine(pp, pp, b);
        blst_p1_to_affine(&mut out, &p);
    }
    out
}

fn batch(points: &[blst_p1_affine]) -> blst_p1 {
    let mut sum = blst_p1::default();
    let ptrs: [*const blst_p1_affine; 2] = [points.as_ptr(), core::ptr::null()];
    // SAFETY: contiguous valid points.
    unsafe { blst_p1s_add(&mut sum, ptrs.as_ptr(), points.len()) };
    sum
}

fn chain(points: &[blst_p1_affine]) -> blst_p1 {
    let mut sum = blst_p1::default();
    for p in points {
        let acc: *mut blst_p1 = &mut sum;
        // SAFETY: aliasing allowed.
        unsafe { blst_p1_add_or_double_affine(acc, acc, p) };
    }
    sum
}

fn ser(p: &blst_p1) -> [u8; 96] {
    let (mut a, mut out) = (blst_p1_affine::default(), [0u8; 96]);
    // SAFETY: valid source and destinations.
    unsafe {
        blst_p1_to_affine(&mut a, p);
        blst_p1_affine_serialize(out.as_mut_ptr(), &a);
    }
    out
}

fn by_scalars(base: &[blst_p1_affine], counts: &[i64]) -> blst_p1 {
    let mut sum = blst_p1::default();
    for (b, &c) in base.iter().zip(counts) {
        let (mut p, mut term) = (blst_p1::default(), blst_p1::default());
        let scalar = c.unsigned_abs().to_le_bytes();
        let acc: *mut blst_p1 = &mut sum;
        // SAFETY: valid points; aliasing allowed.
        unsafe {
            blst_p1_from_affine(&mut p, b);
            blst_p1_mult(&mut term, &p, scalar.as_ptr(), 64);
            blst_p1_cneg(&mut term, c < 0);
            blst_p1_add_or_double(acc, acc, &term);
        }
    }
    sum
}

#[test]
fn batch_g1_copies_cancellations_and_large_sets() {
    let base: Vec<blst_p1_affine> = (0..4).map(g1).collect();
    let negs: Vec<blst_p1_affine> = base.iter().map(neg_a).collect();
    let mut s = 0x5eed_2026_1118u64;
    let (mut cases, mut zero) = (0usize, 0usize);
    let mut check = |picks: &[(usize, bool)]| {
        let pts: Vec<blst_p1_affine> = picks.iter().map(|&(i, n)| if n { negs[i] } else { base[i] }).collect();
        let mut counts = [0i64; 4];
        for &(i, n) in picks {
            counts[i] += if n { -1 } else { 1 };
        }
        let (b, c, r) = (ser(&batch(&pts)), ser(&chain(&pts)), ser(&by_scalars(&base, &counts)));
        assert_eq!(b, c, "batch vs chain, {} points", pts.len());
        assert_eq!(b, r, "batch vs scalars, {} points", pts.len());
        cases += 1;
        // SAFETY: valid point.
        zero += usize::from(unsafe { blst_p1_is_inf(&batch(&pts)) });
    };
    // Many copies of one point; a point and its negation in blocks, alternating, shuffled; lengths to 600.
    for len in [
        1usize, 2, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128, 129, 169, 170, 171, 255, 256, 257, 340, 342, 511, 512,
        513, 600,
    ] {
        check(&vec![(0, false); len]);
        let h = len / 2;
        let mut blocks: Vec<(usize, bool)> = vec![(0, false); h];
        blocks.extend(vec![(0, true); len - h]);
        check(&blocks);
        check(&(0..len).map(|i| (0, i % 2 == 1)).collect::<Vec<_>>());
        for _ in 0..8 {
            let mut v = blocks.clone();
            for i in (1..v.len()).rev() {
                let j = (splitmix(&mut s) % (i as u64 + 1)) as usize;
                v.swap(i, j);
            }
            check(&v);
        }
    }
    // Random multisets from 1 to 4 points and their negations, 1 to 600 long, plus sets whose total is zero.
    for _ in 0..3000 {
        let pool = 1 + (splitmix(&mut s) % 4) as usize;
        let len = 1 + (splitmix(&mut s) % 600) as usize;
        let mut picks: Vec<(usize, bool)> = (0..len)
            .map(|_| {
                let r = splitmix(&mut s);
                ((r % pool as u64) as usize, (r >> 40) & 1 == 1)
            })
            .collect();
        check(&picks);
        // Append the negation of every pick and shuffle: the total is the identity.
        let negated: Vec<(usize, bool)> = picks.iter().map(|&(i, n)| (i, !n)).collect();
        picks.extend(negated);
        for i in (1..picks.len()).rev() {
            let j = (splitmix(&mut s) % (i as u64 + 1)) as usize;
            picks.swap(i, j);
        }
        check(&picks);
    }
    assert!(zero > 3000);
    // 1,100 distinct points (past blst's scratch stride of 1,024).
    let many: Vec<blst_p1_affine> = (0..1100).map(|i| g1(1000 + i)).collect();
    assert_eq!(ser(&batch(&many)), ser(&chain(&many)));
    println!("batch addition in G1: {cases} multisets (copies, blocks, alternations, shuffles, random to 600 points, {zero} summing to the identity) and 1,100 distinct points: batch = chain = scalar multiplications");
}

// ---------------------------------------------------------------------------------------------
// 6. Outside G1: random curve points and order-3 shifts; the difference and the pairing
// ---------------------------------------------------------------------------------------------

/// A random point of E(Fp) (x random; almost surely outside G1).
fn random_curve_point(s: &mut u64) -> blst_p1_affine {
    loop {
        let mut x = [0u8; 48];
        for chunk in x.chunks_mut(8) {
            chunk.copy_from_slice(&splitmix(s).to_be_bytes()[..chunk.len()]);
        }
        x[0] &= 0x0f; // below p
        x[0] |= 0x80; // compressed
        if splitmix(s) & 1 == 1 {
            x[0] |= 0x20;
        }
        let mut a = blst_p1_affine::default();
        // SAFETY: 48 bytes read.
        if unsafe { blst_p1_uncompress(&mut a, x.as_ptr()) } == BLST_ERROR::BLST_SUCCESS {
            return a;
        }
    }
}

fn t_point() -> blst_p1_affine {
    let mut y = blst_fp::default();
    let mut two = [0u8; 48];
    two[47] = 2;
    // SAFETY: 48 bytes read.
    unsafe { blst_fp_from_bendian(&mut y, two.as_ptr()) };
    blst_p1_affine { x: blst_fp::default(), y }
}

fn pairing(p: &blst_p1, q: &blst_p2_affine) -> blst_fp12 {
    let mut a = blst_p1_affine::default();
    let (mut f, mut e) = (blst_fp12::default(), blst_fp12::default());
    // SAFETY: valid inputs.
    unsafe {
        blst_p1_to_affine(&mut a, p);
        blst_miller_loop(&mut f, q, &a);
        blst_final_exp(&mut e, &f);
    }
    e
}

#[test]
fn batch_outside_g1_differs_only_by_order_3_and_keeps_the_pairing() {
    let mut s = 0x00de_17a3_u64;
    let t = t_point();
    let mt = neg_a(&t);
    let mut q = blst_p2::default();
    let mut qa = blst_p2_affine::default();
    // SAFETY: valid destinations.
    unsafe {
        blst_hash_to_g2(&mut q, b"review".as_ptr(), 6, b"REVIEW-LP-G2".as_ptr(), 12, core::ptr::null(), 0);
        blst_p2_to_affine(&mut qa, &q);
    }
    let ks: Vec<blst_p1_affine> = (0..6).map(|i| g1(500 + i)).collect();
    let rs: Vec<blst_p1_affine> = (0..6).map(|_| random_curve_point(&mut s)).collect();
    // SAFETY: valid point.
    assert!(rs.iter().all(|r| !unsafe { blst_p1_affine_in_g1(r) }));
    // The pool: K, −K, K + T, K − T, R, −R, R + T, and T-free G1 points; a key is never x = 0.
    let mut pool: Vec<blst_p1_affine> = Vec::new();
    for k in &ks {
        pool.extend([*k, neg_a(k), add_a(k, &t), add_a(k, &mt)]);
    }
    for r in &rs {
        pool.extend([*r, neg_a(r), add_a(r, &t)]);
    }
    let (mut trials, mut differ, mut both_zero_or_t) = (0usize, 0usize, 0usize);
    for _ in 0..4000 {
        let len = 16 + (splitmix(&mut s) % 185) as usize;
        let size = 1 + (splitmix(&mut s) % pool.len() as u64) as usize;
        let pts: Vec<blst_p1_affine> = (0..len).map(|_| pool[(splitmix(&mut s) % size as u64) as usize]).collect();
        let (b, c) = (batch(&pts), chain(&pts));
        // d = b − c must be O or (0, ±2).
        let mut d = c;
        let mut out = [0u8; 48];
        // SAFETY: valid points; aliasing allowed.
        unsafe {
            blst_p1_cneg(&mut d, true);
            let dp: *mut blst_p1 = &mut d;
            blst_p1_add_or_double(dp, dp, &b);
            if !blst_p1_is_inf(&d) {
                let mut da = blst_p1_affine::default();
                blst_p1_to_affine(&mut da, &d);
                blst_p1_affine_compress(out.as_mut_ptr(), &da);
                assert_eq!(out[1..], [0u8; 47], "the difference has x = 0");
                assert_eq!(out[0] & 0x1f, 0);
                differ += 1;
            }
        }
        // The pairing with a G2 point is the same for both sums (when neither is the identity).
        // SAFETY: valid points.
        let (bi, ci) = unsafe { (blst_p1_is_inf(&b), blst_p1_is_inf(&c)) };
        if !bi && !ci {
            let (eb, ec) = (pairing(&b, &qa), pairing(&c, &qa));
            // SAFETY: valid elements.
            assert!(unsafe { blst_fp12_is_equal(&eb, &ec) }, "the pairing differs");
        } else {
            // One of them is the identity: the other is O or of order 3, on which the pairing is 1.
            both_zero_or_t += 1;
            let other = if bi { c } else { b };
            // SAFETY: valid point.
            if !unsafe { blst_p1_is_inf(&other) } {
                let mut oa = blst_p1_affine::default();
                // SAFETY: valid point.
                unsafe { blst_p1_to_affine(&mut oa, &other) };
                let mut x = [0u8; 48];
                // SAFETY: valid point.
                unsafe { blst_p1_affine_compress(x.as_mut_ptr(), &oa) };
                assert_eq!(x[1..], [0u8; 47], "the other sum is of order 3");
            }
        }
        trials += 1;
    }
    // A pairing check that T itself is in the kernel: e(K + T, Q) = e(K, Q).
    let (kt, k) = (add_a(&ks[0], &t), ks[0]);
    let (mut ktp, mut kp) = (blst_p1::default(), blst_p1::default());
    // SAFETY: valid points.
    unsafe {
        blst_p1_from_affine(&mut ktp, &kt);
        blst_p1_from_affine(&mut kp, &k);
        assert!(blst_fp12_is_equal(&pairing(&ktp, &qa), &pairing(&kp, &qa)));
        assert!(!blst_p1_affine_is_inf(&kt));
    }
    assert!(differ > 100, "the trials reach the mis-addition ({differ})");
    println!("batch addition outside G1: {trials} random multisets of curve points with order-3 shifts; {differ} differ from the chain, always by (0, ±2); the pairing with a G2 point equal in every case ({both_zero_or_t} with a sum at the identity, the other then O or of order 3)");
}

// ---------------------------------------------------------------------------------------------
// 7. compressed() and key_point on curve points outside G1
// ---------------------------------------------------------------------------------------------

#[test]
fn compressed_outside_g1() {
    let mut s = 0xc0ffeeu64;
    for _ in 0..20_000 {
        let p = random_curve_point(&mut s);
        let mut u = [0u8; 96];
        let mut c = [0u8; 48];
        // SAFETY: valid point, sized outputs.
        unsafe {
            blst_p1_affine_serialize(u.as_mut_ptr(), &p);
            blst_p1_affine_compress(c.as_mut_ptr(), &p);
        }
        assert!(key_point(&u).is_ok());
        assert_eq!(compressed(&u), c);
        assert_eq!(uncompress_key(&c).unwrap(), u);
    }
    println!("compressed() = blst on 20,000 random curve points outside G1 (both signs of y)");
}
