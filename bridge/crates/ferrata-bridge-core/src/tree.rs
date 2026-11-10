//! The depth-32 SHA-256 Merkle tree used for both the vault's deposit tree and the LEZ burn tree.
//!
//! Leaf `i` sits at position `i` of the bottom level; a node is `sha256(0x01 ‖ left ‖ right)`
//! ([`crate::encoding::node_preimage`]); an empty leaf is 32 zero bytes, so `zero[0] = 0x00…00` and
//! `zero[h + 1] = node(zero[h], zero[h])`.
//!
//! A [`MerklePath`] carries its direction bits explicitly (bit `h` set: the running node is the right
//! child at height `h`) next to the 32 siblings, as a circuit witness would. [`verify_path`] requires
//! the direction bits to equal the leaf index the leaf itself commits to (SPEC-v2 §3.3, §5.2): a
//! 32-bit direction field also rules out indices ≥ 2^32. Left and right are ordered with a mask, not
//! a branch, so the work is the same for every path (the release guest's cycle count must not depend
//! on the witness).
//!
//! [`IncrementalTree`] is the vault's `_insertLeaf` algorithm (O(32) per append, O(32) state) with
//! the canonical frontier the checkpoint state commits to; [`Tree`] (with `std`) keeps every leaf
//! and creates paths.

use crate::encoding::node_preimage;
use crate::{sha256, Hash, VerifyError};

/// Height of the tree.
pub const TREE_DEPTH: usize = 32;
/// Number of leaves a depth-32 tree holds.
pub const MAX_LEAVES: u64 = 1 << TREE_DEPTH;

/// A Merkle path: direction bits and the siblings from the leaf level up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MerklePath {
    /// Bit `h` set: at height `h` the running node is the right child (the sibling is on the left).
    pub directions: u32,
    /// Sibling at each height, leaf level first.
    pub siblings: [Hash; TREE_DEPTH],
}

/// An inner node.
pub fn node(left: &Hash, right: &Hash) -> Hash {
    sha256(&node_preimage(left, right))
}

/// `zero[h]` is the root of an empty subtree of height `h`, for `h = 0..=32`.
pub fn zero_hashes() -> [Hash; TREE_DEPTH + 1] {
    let mut zero = [[0u8; 32]; TREE_DEPTH + 1];
    for h in 0..TREE_DEPTH {
        zero[h + 1] = node(&zero[h], &zero[h]);
    }
    zero
}

/// `(left, right)`: `(running, sibling)` if the low bit of `bit` is 0, `(sibling, running)` if it is
/// 1, selected with a byte mask instead of a branch.
///
/// The mask passes through `core::hint::black_box`: without it, LLVM recognises
/// `a ^ ((a ^ b) & -bit)` as a select and, on rv32im (no conditional move), lowers it to a branch.
/// Measured in the release guest: 4 extra cycles per 1 bit of the leaf index, i.e. the cycle count
/// leaked the index's Hamming weight (artifacts/logs/invariance-v2.log).
fn ordered(running: &Hash, sibling: &Hash, bit: u32) -> (Hash, Hash) {
    let mask = core::hint::black_box(0u8.wrapping_sub((bit & 1) as u8));
    let mut left = [0u8; 32];
    let mut right = [0u8; 32];
    for (((l, r), a), b) in left.iter_mut().zip(right.iter_mut()).zip(running).zip(sibling) {
        let x = (a ^ b) & mask;
        *l = a ^ x;
        *r = b ^ x;
    }
    (left, right)
}

/// Hashes `leaf` up `path`, following the path's own direction bits.
pub fn root_from_path(leaf: &Hash, path: &MerklePath) -> Hash {
    let mut running = *leaf;
    for (h, sibling) in path.siblings.iter().enumerate() {
        let (left, right) = ordered(&running, sibling, path.directions >> h);
        running = node(&left, &right);
    }
    running
}

/// Checks that `leaf` sits at `leaf_index` in the tree with root `root`: the path's direction bits
/// must equal `leaf_index` (so an index ≥ 2^32 never verifies), and the path must hash to `root`.
pub fn verify_path(root: &Hash, leaf: &Hash, leaf_index: u64, path: &MerklePath) -> Result<(), VerifyError> {
    if u64::from(path.directions) != leaf_index {
        return Err(VerifyError::DirectionsMismatch);
    }
    if root_from_path(leaf, path) == *root {
        Ok(())
    } else {
        Err(VerifyError::RootMismatch)
    }
}

/// The tree has 2^32 leaves already.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeFull;

/// A frontier that is not canonical (a non-zero entry where the leaf count has a 0 bit), or a count
/// of 2^32 or more.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BadFrontier;

/// Append-only tree that keeps only the frontier, exactly as `FerrataVault._insertLeaf` does.
#[derive(Clone, Debug)]
pub struct IncrementalTree {
    /// `branch[h]`: the most recently completed subtree of height `h` (read only while bit `h` of
    /// `next_index` is set, i.e. while it is the left neighbour of the next leaf's path).
    branch: [Hash; TREE_DEPTH],
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

    /// The canonical frontier: `f[h]` is the completed subtree of height `h` on the tree's left edge
    /// if bit `h` of the leaf count is 1, and 32 zero bytes otherwise. A function of the leaves alone.
    pub fn frontier(&self) -> [Hash; TREE_DEPTH] {
        let mut f = [[0u8; 32]; TREE_DEPTH];
        for (h, slot) in f.iter_mut().enumerate() {
            if (self.next_index >> h) & 1 == 1 {
                *slot = self.branch[h];
            }
        }
        f
    }

    /// Resumes a tree from its leaf count and canonical frontier (what a checkpoint carries), so a
    /// prover can append further leaves without the earlier ones.
    pub fn from_frontier(count: u64, frontier: &[Hash; TREE_DEPTH]) -> Result<Self, BadFrontier> {
        if count >= MAX_LEAVES {
            return Err(BadFrontier);
        }
        for (h, f) in frontier.iter().enumerate() {
            if (count >> h) & 1 == 0 && *f != [0u8; 32] {
                return Err(BadFrontier);
            }
        }
        let zero = zero_hashes();
        // The root is the empty slot `count` hashed up its path: completed subtrees on the left
        // where the count has a 1 bit, empty subtrees on the right where it has a 0 bit.
        let mut running = zero[0];
        for h in 0..TREE_DEPTH {
            running = if (count >> h) & 1 == 1 { node(&frontier[h], &running) } else { node(&running, &zero[h]) };
        }
        Ok(Self { branch: *frontier, zero, next_index: count, root: running })
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

    /// Path of the leaf at `leaf_index` (direction bits = `leaf_index`), or `None` if there is none.
    pub fn path(&self, leaf_index: u64) -> Option<MerklePath> {
        if leaf_index >= self.leaves.len() as u64 {
            return None;
        }
        let zero = zero_hashes();
        let mut siblings = [[0u8; 32]; TREE_DEPTH];
        let mut level = self.leaves.clone();
        let mut position = leaf_index as usize;
        for h in 0..TREE_DEPTH {
            siblings[h] = level.get(position ^ 1).copied().unwrap_or(zero[h]);
            level = parent_level(&level, &zero[h]);
            position >>= 1;
        }
        Some(MerklePath { directions: leaf_index as u32, siblings })
    }
}

#[cfg(feature = "std")]
fn parent_level(level: &[Hash], empty_sibling: &Hash) -> Vec<Hash> {
    level.chunks(2).map(|pair| node(&pair[0], pair.get(1).unwrap_or(empty_sibling))).collect()
}
