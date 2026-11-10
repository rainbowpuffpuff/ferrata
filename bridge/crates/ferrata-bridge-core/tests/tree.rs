//! The depth-32 tree: incremental and full trees agree; paths verify only with direction bits equal
//! to the leaf index; any change to the leaf, a sibling, the direction bits or the root breaks
//! verification; the canonical frontier depends on the leaves alone and resumes the tree exactly.

mod common;

use ferrata_bridge_core::tree::{
    node, root_from_path, verify_path, zero_hashes, BadFrontier, IncrementalTree, MerklePath, Tree, MAX_LEAVES,
    TREE_DEPTH,
};
use ferrata_bridge_core::VerifyError;
use proptest::prelude::*;

/// Distinct random leaves (a raw tree binds a position only when siblings differ; protocol leaves
/// always differ because they contain their own index).
fn distinct_leaves(max: usize) -> impl Strategy<Value = Vec<[u8; 32]>> {
    prop::collection::btree_set(any::<[u8; 32]>(), 1..max).prop_map(|set| set.into_iter().collect())
}

#[test]
fn empty_roots() {
    let zero = zero_hashes();
    assert_eq!(zero[0], [0u8; 32]);
    for hgt in 0..TREE_DEPTH {
        assert_eq!(zero[hgt + 1], node(&zero[hgt], &zero[hgt]));
    }
    assert_eq!(IncrementalTree::new().root(), zero[TREE_DEPTH]);
    assert_eq!(Tree::new().root(), zero[TREE_DEPTH]);
}

#[test]
fn direction_bits_must_equal_the_leaf_index() {
    let tree = Tree::from_leaves(vec![[7u8; 32], [8u8; 32]]).unwrap();
    let path = tree.path(1).unwrap();
    assert_eq!(path.directions, 1);
    assert_eq!(verify_path(&tree.root(), &[8u8; 32], 1, &path), Ok(()));
    // 2^32 + 1 has the same low 32 bits as 1: rejected by the direction check.
    assert_eq!(verify_path(&tree.root(), &[8u8; 32], MAX_LEAVES + 1, &path), Err(VerifyError::DirectionsMismatch));
    assert_eq!(verify_path(&tree.root(), &[8u8; 32], 0, &path), Err(VerifyError::DirectionsMismatch));
    assert!(tree.path(2).is_none());
}

#[test]
fn a_path_to_the_root_with_other_direction_bits_is_still_rejected() {
    // A leaf that claims index 5 but sits at position 3 of the tree: its path (directions 3) does hash
    // to the root, and the statement must still refuse it.
    let leaves: Vec<[u8; 32]> = (0..6u8).map(|i| [i + 1; 32]).collect();
    let tree = Tree::from_leaves(leaves.clone()).unwrap();
    let path_at_3 = tree.path(3).unwrap();
    assert_eq!(root_from_path(&leaves[3], &path_at_3), tree.root());
    assert_eq!(verify_path(&tree.root(), &leaves[3], 5, &path_at_3), Err(VerifyError::DirectionsMismatch));
}

#[test]
fn last_index_hashes_from_the_right_at_every_height() {
    let siblings: [[u8; 32]; 32] = core::array::from_fn(|i| [i as u8 + 1; 32]);
    let leaf = [0xabu8; 32];
    let mut want = leaf;
    for sibling in &siblings {
        want = node(sibling, &want);
    }
    let path = MerklePath { directions: u32::MAX, siblings };
    assert_eq!(root_from_path(&leaf, &path), want);
    assert_eq!(verify_path(&want, &leaf, MAX_LEAVES - 1, &path), Ok(()));
}

#[test]
fn frontier_rejects_non_canonical_input() {
    let mut f = [[0u8; 32]; TREE_DEPTH];
    f[1] = [1u8; 32]; // bit 1 of count 1 is 0
    assert_eq!(IncrementalTree::from_frontier(1, &f).err(), Some(BadFrontier));
    assert_eq!(IncrementalTree::from_frontier(MAX_LEAVES, &[[0u8; 32]; TREE_DEPTH]).err(), Some(BadFrontier));
    assert_eq!(IncrementalTree::from_frontier(0, &[[0u8; 32]; TREE_DEPTH]).unwrap().root(), zero_hashes()[TREE_DEPTH]);
}

proptest! {
    #[test]
    fn incremental_tree_matches_full_tree(leaves in prop::collection::vec(any::<[u8; 32]>(), 0..70)) {
        let mut inc = IncrementalTree::new();
        let mut full = Tree::new();
        for (i, leaf) in leaves.iter().enumerate() {
            prop_assert_eq!(inc.append(*leaf), Ok(i as u64));
            prop_assert_eq!(full.append(*leaf), Ok(i as u64));
            prop_assert_eq!(inc.root(), full.root());
        }
        prop_assert_eq!(inc.len(), leaves.len() as u64);
    }

    /// The frontier resumes the tree: from (count, frontier) alone, the root is right and further
    /// appends give the same roots as the full tree.
    #[test]
    fn frontier_resumes_the_tree(leaves in prop::collection::vec(any::<[u8; 32]>(), 0..70), split in any::<prop::sample::Index>()) {
        let k = split.index(leaves.len() + 1);
        let mut inc = IncrementalTree::new();
        for leaf in &leaves[..k] {
            inc.append(*leaf).unwrap();
        }
        let mut resumed = IncrementalTree::from_frontier(k as u64, &inc.frontier()).unwrap();
        prop_assert_eq!(resumed.root(), inc.root());
        let mut full = Tree::from_leaves(leaves[..k].to_vec()).unwrap();
        for leaf in &leaves[k..] {
            resumed.append(*leaf).unwrap();
            full.append(*leaf).unwrap();
            prop_assert_eq!(resumed.root(), full.root());
        }
        // Canonical: entries where the count has a 0 bit are zero.
        for (h, f) in inc.frontier().iter().enumerate() {
            if (k >> h) & 1 == 0 {
                prop_assert_eq!(*f, [0u8; 32]);
            }
        }
    }

    #[test]
    fn every_path_verifies(leaves in distinct_leaves(40)) {
        let tree = Tree::from_leaves(leaves.clone()).unwrap();
        let root = tree.root();
        for (i, leaf) in leaves.iter().enumerate() {
            prop_assert_eq!(verify_path(&root, leaf, i as u64, &tree.path(i as u64).unwrap()), Ok(()));
        }
    }

    #[test]
    fn tampered_path_leaf_or_root_fails(leaves in distinct_leaves(40), pick in any::<prop::sample::Index>(),
                                        height in 0usize..TREE_DEPTH, at in 0usize..32, mask in 1u8..=255) {
        let tree = Tree::from_leaves(leaves.clone()).unwrap();
        let i = pick.index(leaves.len());
        let (root, leaf, path) = (tree.root(), leaves[i], tree.path(i as u64).unwrap());

        let mut bad = path;
        bad.siblings[height][at] ^= mask;
        prop_assert_eq!(verify_path(&root, &leaf, i as u64, &bad), Err(VerifyError::RootMismatch));

        let mut bad_leaf = leaf;
        bad_leaf[at] ^= mask;
        prop_assert_eq!(verify_path(&root, &bad_leaf, i as u64, &path), Err(VerifyError::RootMismatch));

        let mut bad_root = root;
        bad_root[at] ^= mask;
        prop_assert_eq!(verify_path(&bad_root, &leaf, i as u64, &path), Err(VerifyError::RootMismatch));
    }

    /// Other direction bits with a consistent index: the leaf is hashed along another route.
    #[test]
    fn other_direction_bits_fail(leaves in distinct_leaves(40), pick in any::<prop::sample::Index>(), other in any::<u32>()) {
        let tree = Tree::from_leaves(leaves.clone()).unwrap();
        let i = pick.index(leaves.len()) as u32;
        prop_assume!(other != i);
        let mut path = tree.path(u64::from(i)).unwrap();
        path.directions = other;
        prop_assert_eq!(verify_path(&tree.root(), &leaves[i as usize], u64::from(other), &path), Err(VerifyError::RootMismatch));
        prop_assert_eq!(verify_path(&tree.root(), &leaves[i as usize], u64::from(i), &path), Err(VerifyError::DirectionsMismatch));
    }

    #[test]
    fn root_of_an_older_tree_does_not_accept_newer_leaves(leaves in distinct_leaves(30), extra in any::<[u8; 32]>()) {
        prop_assume!(!leaves.contains(&extra));
        let old = Tree::from_leaves(leaves.clone()).unwrap();
        let mut new = old.clone();
        let idx = new.append(extra).unwrap();
        prop_assert_eq!(verify_path(&old.root(), &extra, idx, &new.path(idx).unwrap()), Err(VerifyError::RootMismatch));
        prop_assert_eq!(verify_path(&old.root(), &leaves[0], 0, &old.path(0).unwrap()), Ok(()));
        prop_assert_eq!(verify_path(&new.root(), &leaves[0], 0, &new.path(0).unwrap()), Ok(()));
    }
}
