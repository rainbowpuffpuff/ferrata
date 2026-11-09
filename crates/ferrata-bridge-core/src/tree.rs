//! The depth-32 SHA-256 Merkle tree used for both the vault's deposit tree and the LEZ burn tree.
//!
//! Leaf `i` sits at position `i` of the bottom level; a node is
//! `sha256(0x01 ‖ left ‖ right)` ([`crate::encoding::node_preimage`]); an empty leaf is 32 zero
//! bytes, so `zero[h]`, the root of an empty subtree of height `h`, is `zero[0] = 0x00…00` and
//! `zero[h + 1] = node(zero[h], zero[h])`. A Merkle path lists the 32 siblings from the leaf level
//! up; at height `h`, bit `h` of the leaf index says whether the running node is the right child.
//!
//! [`IncrementalTree`] is the vault's `_insertLeaf` algorithm (O(32) per append, O(32) state).
//! [`Tree`] (with `std`) keeps every leaf and creates paths.

use crate::encoding::node_preimage;
use crate::{sha256, Hash, VerifyError};

/// Height of the tree.
pub const TREE_DEPTH: usize = 32;
/// Number of leaves a depth-32 tree holds.
pub const MAX_LEAVES: u64 = 1 << TREE_DEPTH;

/// A Merkle path: the siblings of the path from a leaf to the root, leaf level first.
pub type Path = [Hash; TREE_DEPTH];

/// An inner node.
pub fn node(left: &Hash, right: &Hash) -> Hash {
    sha256(&node_preimage(left, right))
}

/// `zero[h]` is the root of an empty subtree of height `h`, for `h = 0..=32`; `zero[32]` is the
/// root of the empty tree.
pub fn zero_hashes() -> [Hash; TREE_DEPTH + 1] {
    let mut zero = [[0u8; 32]; TREE_DEPTH + 1];
    for h in 0..TREE_DEPTH {
        zero[h + 1] = node(&zero[h], &zero[h]);
    }
    zero
}

/// Hashes `leaf` up its path. Rejects indices that do not fit a depth-32 tree, so no two
/// distinct indices share a path.
pub fn root_from_path(leaf: &Hash, leaf_index: u64, path: &Path) -> Result<Hash, VerifyError> {
    if leaf_index >= MAX_LEAVES {
        return Err(VerifyError::LeafIndexOutOfRange);
    }
    let mut running = *leaf;
    for (h, sibling) in path.iter().enumerate() {
        running = if (leaf_index >> h) & 1 == 0 { node(&running, sibling) } else { node(sibling, &running) };
    }
    Ok(running)
}

/// Checks that `leaf` is at `leaf_index` in the tree with root `root`.
pub fn verify_path(root: &Hash, leaf: &Hash, leaf_index: u64, path: &Path) -> Result<(), VerifyError> {
    if root_from_path(leaf, leaf_index, path)? == *root {
        Ok(())
    } else {
        Err(VerifyError::RootMismatch)
    }
}

/// The tree has 2^32 leaves already.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeFull;

/// Append-only tree that keeps only the frontier, exactly as `FerrataVault._insertLeaf` does.
#[derive(Clone, Debug)]
pub struct IncrementalTree {
    /// `branch[h]`: the most recently completed subtree of height `h` (read while bit `h` of
    /// `next_index` is set, i.e. while it is the left neighbour of the next leaf's path).
    branch: Path,
    zero: [Hash; TREE_DEPTH + 1],
    next_index: u64,
    root: Hash,
}

impl Default for IncrementalTree {
    fn default() -> Self {
        Self::new()
    }
}

impl IncrementalTree {
    /// The empty tree.
    pub fn new() -> Self {
        let zero = zero_hashes();
        Self { branch: [[0u8; 32]; TREE_DEPTH], zero, next_index: 0, root: zero[TREE_DEPTH] }
    }

    /// Number of leaves appended (the index the next leaf gets).
    pub fn len(&self) -> u64 {
        self.next_index
    }

    /// True before the first append.
    pub fn is_empty(&self) -> bool {
        self.next_index == 0
    }

    /// Current root.
    pub fn root(&self) -> Hash {
        self.root
    }

    /// Appends `leaf` and returns its index.
    pub fn append(&mut self, leaf: Hash) -> Result<u64, TreeFull> {
        let index = self.next_index;
        if index >= MAX_LEAVES {
            return Err(TreeFull);
        }
        let mut running = leaf;
        let mut saved = false;
        for h in 0..TREE_DEPTH {
            if (index >> h) & 1 == 1 {
                running = node(&self.branch[h], &running);
            } else {
                // The lowest 0 bit: `running` is a subtree this leaf has just completed.
                if !saved {
                    self.branch[h] = running;
                    saved = true;
                }
                running = node(&running, &self.zero[h]);
            }
        }
        self.root = running;
        self.next_index = index + 1;
        Ok(index)
    }
}

/// A tree that keeps all its leaves, for creating paths (wallets, provers, tests).
#[cfg(feature = "std")]
#[derive(Clone, Debug, Default)]
pub struct Tree {
    leaves: Vec<Hash>,
}

#[cfg(feature = "std")]
impl Tree {
    /// The empty tree.
    pub fn new() -> Self {
        Self::default()
    }

    /// The tree whose first leaves are `leaves`.
    pub fn from_leaves(leaves: Vec<Hash>) -> Result<Self, TreeFull> {
        if leaves.len() as u64 > MAX_LEAVES {
            return Err(TreeFull);
        }
        Ok(Self { leaves })
    }

    /// Appends `leaf` and returns its index.
    pub fn append(&mut self, leaf: Hash) -> Result<u64, TreeFull> {
        let index = self.leaves.len() as u64;
        if index >= MAX_LEAVES {
            return Err(TreeFull);
        }
        self.leaves.push(leaf);
        Ok(index)
    }

    /// The leaves, in index order.
    pub fn leaves(&self) -> &[Hash] {
        &self.leaves
    }

    /// Root, recomputed level by level from all leaves.
    pub fn root(&self) -> Hash {
        let zero = zero_hashes();
        let mut level = self.leaves.clone();
        for zero_h in zero.iter().take(TREE_DEPTH) {
            level = parent_level(&level, zero_h);
        }
        level.first().copied().unwrap_or(zero[TREE_DEPTH])
    }

    /// Path of the leaf at `leaf_index`, or `None` if there is no such leaf.
    pub fn path(&self, leaf_index: u64) -> Option<Path> {
        if leaf_index >= self.leaves.len() as u64 {
            return None;
        }
        let zero = zero_hashes();
        let mut path = [[0u8; 32]; TREE_DEPTH];
        let mut level = self.leaves.clone();
        let mut position = leaf_index as usize;
        for h in 0..TREE_DEPTH {
            path[h] = level.get(position ^ 1).copied().unwrap_or(zero[h]);
            level = parent_level(&level, &zero[h]);
            position >>= 1;
        }
        Some(path)
    }
}

#[cfg(feature = "std")]
fn parent_level(level: &[Hash], empty_sibling: &Hash) -> Vec<Hash> {
    level.chunks(2).map(|pair| node(&pair[0], pair.get(1).unwrap_or(empty_sibling))).collect()
}
