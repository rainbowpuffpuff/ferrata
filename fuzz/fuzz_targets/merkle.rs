//! SSZ Merkle branches (`ferrata_core::merkle`) against the consensus specs' definitions,
//! transcribed independently here: `is_valid_merkle_branch`, `is_valid_normalized_merkle_branch`
//! and `normalize_merkle_branch`, for any depth, index, generalized index and branch length.
#![no_main]
use ferrata_core::merkle::{
    compute_merkle_branch_root, is_valid_merkle_branch, is_valid_normalized_merkle_branch, normalize_merkle_branch,
};
use ferrata_fuzz::Input;
use libfuzzer_sys::fuzz_target;
use sha2_ref::hash_pair;

mod sha2_ref {
    pub fn hash_pair(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
        let mut buf = [0u8; 64];
        buf[..32].copy_from_slice(a);
        buf[32..].copy_from_slice(b);
        ferrata_core::hash::sha256(&buf)
    }
}

/// The spec's loop; `None` where Python would raise (a branch shorter than the depth).
fn reference_root(leaf: &[u8; 32], branch: &[[u8; 32]], depth: usize, index: u64) -> Option<[u8; 32]> {
    let mut value = *leaf;
    for i in 0..depth {
        let sibling = branch.get(i)?;
        // index // 2**i % 2, for any i (2**i beyond 64 bits leaves a zero bit).
        let bit = if i < 64 { (index >> i) & 1 } else { 0 };
        value = if bit == 1 { hash_pair(sibling, &value) } else { hash_pair(&value, sibling) };
    }
    Some(value)
}

fuzz_target!(|data: &[u8]| {
    let mut i = Input(data);
    let leaf: [u8; 32] = i.array();
    // Depth at most 64 and gindex at least 1: the domain every caller stays in (findings M1 and
    // M2 of fuzz/RESULTS.md, fixed on branch fuzz-core-merkle-domain).
    let depth = usize::from(i.u8() % 65);
    let index = i.u64();
    let gindex = i.u64();
    let zeros = usize::from(i.u8() % 4);
    let mut branch: Vec<[u8; 32]> = vec![[0u8; 32]; zeros];
    while !i.0.is_empty() && branch.len() < 200 {
        let mut r = [0u8; 32];
        r[0] = i.u8(); // short roots keep the corpus small; the rest of the root stays zero
        r[31] = i.u8();
        branch.push(r);
    }

    let reference = reference_root(&leaf, &branch, depth, index);
    assert_eq!(compute_merkle_branch_root(&leaf, &branch, depth, index), reference);
    if let Some(root) = reference {
        assert!(is_valid_merkle_branch(&leaf, &branch, depth, index, &root));
        let mut wrong = root;
        wrong[5] ^= 0x10;
        assert!(!is_valid_merkle_branch(&leaf, &branch, depth, index, &wrong));
    }

    // Normalized branches. gindex 0 has no depth (the spec's floorlog2 is undefined there).
    if gindex != 0 {
        let d = 63 - gindex.leading_zeros() as usize;
        let sub = gindex - (1u64 << d);
        let expected = (branch.len() >= d && branch[..branch.len() - d].iter().all(|b| *b == [0u8; 32]))
            .then(|| reference_root(&leaf, &branch[branch.len() - d..], d, sub))
            .flatten();
        let root = expected.unwrap_or(leaf);
        assert_eq!(
            is_valid_normalized_merkle_branch(&leaf, &branch, gindex, &root),
            expected.is_some() || {
                // Not a valid branch: ferrata must refuse it for every root, the leaf included.
                false
            }
        );
        let normalized = normalize_merkle_branch(&branch, gindex);
        assert_eq!(normalized.len(), branch.len().max(d));
        assert!(normalized.ends_with(&branch));
    }
    // gindex 0 is outside the functions' domain (see fuzz/RESULTS.md, finding M1): every caller
    // passes a constant generalized index of at least 1.
});
