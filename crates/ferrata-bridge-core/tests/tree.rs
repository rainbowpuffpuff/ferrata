//! The depth-32 tree: incremental and full trees agree, paths verify, and any change to the leaf,
//! the index, a sibling or the root breaks verification.

mod common;

use ferrata_bridge_core::tree::{
    node, root_from_path, verify_path, zero_hashes, IncrementalTree, Tree, MAX_LEAVES, TREE_DEPTH,
};
use ferrata_bridge_core::VerifyError;
use proptest::prelude::*;

/// Distinct random leaves (a raw tree only binds the index when siblings differ; protocol leaves
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
fn indices_beyond_the_tree_are_rejected() {
    let tree = Tree::from_leaves(vec![[7u8; 32]]).unwrap();
    let path = tree.path(0).unwrap();
    assert_eq!(verify_path(&tree.root(), &[7u8; 32], 0, &path), Ok(()));
    // 2^32 has the same low 32 bits as 0: without the range check it would verify.
    assert_eq!(verify_path(&tree.root(), &[7u8; 32], MAX_LEAVES, &path), Err(VerifyError::LeafIndexOutOfRange));
    assert_eq!(root_from_path(&[7u8; 32], u64::MAX, &path), Err(VerifyError::LeafIndexOutOfRange));
    assert!(tree.path(1).is_none());
}

#[test]
fn last_index_hashes_from_the_right_at_every_height() {
    // Leaf 2^32 - 1: every path bit is 1, so every sibling is a left neighbour.
    let path: [[u8; 32]; 32] = core::array::from_fn(|i| [i as u8 + 1; 32]);
    let leaf = [0xabu8; 32];
    let mut want = leaf;
    for sibling in &path {
        want = node(sibling, &want);
    }
    assert_eq!(root_from_path(&leaf, MAX_LEAVES - 1, &path), Ok(want));
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

    #[test]
    fn every_path_verifies(leaves in distinct_leaves(40)) {
        let tree = Tree::from_leaves(leaves.clone()).unwrap();
        let root = tree.root();
        for (i, leaf) in leaves.iter().enumerate() {
            let path = tree.path(i as u64).unwrap();
            prop_assert_eq!(verify_path(&root, leaf, i as u64, &path), Ok(()));
        }
    }

    #[test]
    fn tampered_path_leaf_or_root_fails(leaves in distinct_leaves(40), pick in any::<prop::sample::Index>(),
                                        height in 0usize..TREE_DEPTH, at in 0usize..32, mask in 1u8..=255) {
        let tree = Tree::from_leaves(leaves.clone()).unwrap();
        let i = pick.index(leaves.len());
        let (root, leaf, path) = (tree.root(), leaves[i], tree.path(i as u64).unwrap());

        let mut bad_path = path;
        bad_path[height][at] ^= mask;
        prop_assert_eq!(verify_path(&root, &leaf, i as u64, &bad_path), Err(VerifyError::RootMismatch));

        let mut bad_leaf = leaf;
        bad_leaf[at] ^= mask;
        prop_assert_eq!(verify_path(&root, &bad_leaf, i as u64, &path), Err(VerifyError::RootMismatch));

        let mut bad_root = root;
        bad_root[at] ^= mask;
        prop_assert_eq!(verify_path(&bad_root, &leaf, i as u64, &path), Err(VerifyError::RootMismatch));
    }

    #[test]
    fn wrong_index_fails(leaves in distinct_leaves(40), pick in any::<prop::sample::Index>(), other in 0u64..MAX_LEAVES) {
        let tree = Tree::from_leaves(leaves.clone()).unwrap();
        let i = pick.index(leaves.len()) as u64;
        prop_assume!(other != i);
        let path = tree.path(i).unwrap();
        prop_assert_eq!(verify_path(&tree.root(), &leaves[i as usize], other, &path), Err(VerifyError::RootMismatch));
    }

    #[test]
    fn root_of_an_older_tree_does_not_accept_newer_leaves(leaves in distinct_leaves(30), extra in any::<[u8; 32]>()) {
        let old = Tree::from_leaves(leaves.clone()).unwrap();
        let mut new = old.clone();
        let idx = new.append(extra).unwrap();
        prop_assume!(!leaves.contains(&extra));
        prop_assert_eq!(verify_path(&old.root(), &extra, idx, &new.path(idx).unwrap()), Err(VerifyError::RootMismatch));
        // Old leaves verify under both roots, with each tree's own path.
        prop_assert_eq!(verify_path(&old.root(), &leaves[0], 0, &old.path(0).unwrap()), Ok(()));
        prop_assert_eq!(verify_path(&new.root(), &leaves[0], 0, &new.path(0).unwrap()), Ok(()));
    }
}
