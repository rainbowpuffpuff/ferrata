//! The burn tree (SPEC-v2 §5.1): depth 32, `node = H(0x01 ‖ left ‖ right)`, empty leaf = 32 zero
//! bytes, kept as an incremental frontier so a checkpoint carries 32 nodes instead of the leaves.

use crate::hash::{sha256, sha256_parts, BURN_TAG, TAG_BURN_FRONTIER};
use crate::{Address, Error, Hash};

pub const DEPTH: usize = 32;
/// Leaves the frontier can hold. Leaf index 2^32 - 1, the last one, would complete the tree and
/// leave nothing for the frontier to store, so it is refused (as the Ethereum deposit contract
/// refuses its last leaf).
pub const MAX_LEAVES: u64 = (1 << DEPTH) - 1;

/// `H(0x01 ‖ left ‖ right)`.
pub fn node(left: &Hash, right: &Hash) -> Hash {
    sha256_parts(&[&[0x01], left, right])
}

/// `burnLeaf = H(0x02 ‖ BURN_TAG ‖ configId ‖ burnIndex:u64 ‖ token:20 ‖ amount:u256 ‖ COut ‖ r)`.
pub fn burn_leaf(config_id: &Hash, burn_index: u64, token: &Address, amount: &Hash, c_out: &Hash, r: &Hash) -> Hash {
    sha256_parts(&[&[0x02], &BURN_TAG, config_id, &burn_index.to_be_bytes(), token, amount, c_out, r])
}

/// `Z[0]` = the empty leaf, `Z[h+1] = node(Z[h], Z[h])`.
pub fn zero_hashes() -> [Hash; DEPTH + 1] {
    let mut z = [[0u8; 32]; DEPTH + 1];
    for h in 0..DEPTH {
        z[h + 1] = node(&z[h], &z[h]);
    }
    z
}

/// The tree's left edge: `nodes[h]` is the root of the completed subtree of height `h` waiting for
/// a right sibling when bit `h` of `count` is 1, and 32 zero bytes when it is 0 (canonical, so its
/// hash depends on the leaves alone).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frontier {
    pub count: u64,
    pub nodes: [Hash; DEPTH],
}

impl Default for Frontier {
    fn default() -> Self {
        Self { count: 0, nodes: [[0u8; 32]; DEPTH] }
    }
}

impl Frontier {
    /// Whether `count` fits and every node at a 0 bit of `count` is zero.
    pub fn is_canonical(&self) -> bool {
        self.count <= MAX_LEAVES && (0..DEPTH).all(|h| (self.count >> h) & 1 == 1 || self.nodes[h] == [0u8; 32])
    }

    pub fn append(&mut self, leaf: Hash) -> Result<(), Error> {
        if self.count >= MAX_LEAVES {
            return Err(Error::BurnTreeFull);
        }
        let mut carry = leaf;
        for h in 0..DEPTH {
            if (self.count >> h) & 1 == 0 {
                self.nodes[h] = carry;
                break;
            }
            carry = node(&self.nodes[h], &carry);
            self.nodes[h] = [0u8; 32];
        }
        self.count += 1;
        Ok(())
    }

    /// The root of the depth-32 tree whose first `count` leaves are the appended ones.
    pub fn root(&self, zeros: &[Hash; DEPTH + 1]) -> Hash {
        let mut acc = zeros[0];
        for (h, (left, zero)) in self.nodes.iter().zip(zeros).enumerate() {
            acc = if (self.count >> h) & 1 == 1 { node(left, &acc) } else { node(&acc, zero) };
        }
        acc
    }

    /// `burnFrontierHash = sha256("ferrata/burn-frontier/v1" ‖ nodes[0] ‖ … ‖ nodes[31])`.
    pub fn hash(&self) -> Hash {
        let mut buf = [0u8; 24 + 32 * DEPTH];
        buf[..24].copy_from_slice(TAG_BURN_FRONTIER);
        for (h, n) in self.nodes.iter().enumerate() {
            buf[24 + 32 * h..24 + 32 * (h + 1)].copy_from_slice(n);
        }
        sha256(&buf)
    }
}
