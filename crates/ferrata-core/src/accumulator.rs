//! Accumulator facts: the leaves of a SHA-256 binary Merkle accumulator whose root a contract
//! keeps in one storage slot.
//!
//! **The tree.** Depth 32 (2^32 leaves). An inner node is `SHA-256(0x01 ‖ left ‖ right)`, and the
//! empty leaf is 32 zero bytes, so a tree that is not full is completed with empty subtrees. A
//! leaf's path is its 32 siblings, from the leaf level up; at height `h`, bit `h` of the leaf
//! index says on which side the running node is (1: it is the right child, the sibling is on the
//! left). The index therefore fixes the path, and an index of 2^32 or more is refused: its low 32
//! bits would verify the same path and give the same leaf a second identifier.
//!
//! **Where the root comes from.** The root is a storage fact ([`attest_accumulator_root`]), read
//! under a [`HeaderRecord`] like every other execution value. A leaf is then proven against that
//! root ([`attest_accumulator_leaf`]).
//!
//! **The identifier** ([`accumulator_leaf_id`]) is `SHA-256("ferrata/fact/acc-leaf/v1" ‖ chain_id
//! ‖ address ‖ slot ‖ leaf_index ‖ leaf)`, with the chain id and the slot as 32-byte big-endian
//! integers and the index as 8 bytes. It names the accumulator (chain, contract, slot), the
//! position and the leaf, and not the root, the block or the record: the same leaf proven under
//! any later root has one identifier, which is what a consumer keys a once-per-leaf nullifier on.
//!
//! Nothing here is specific to a contract: the leaf is 32 opaque bytes, and any contract that
//! keeps such a root in a slot can be attested.

use sha2::{Digest, Sha256};

use crate::{
    attest::{attest_storage, Attested, HeaderRecord},
    exec::{Address, Word},
    Error, Root,
};

/// The depth of the tree: paths have this many siblings, and indices are below 2^32.
pub const ACCUMULATOR_DEPTH: usize = 32;
/// The tag [`accumulator_leaf_id`] starts with.
pub const ACC_LEAF_TAG: &[u8; 24] = b"ferrata/fact/acc-leaf/v1";

/// An accumulator root attested from a contract's storage slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccumulatorRoot {
    chain_id: u64,
    address: Address,
    slot: Word,
    root: Root,
    block_number: u64,
    block_hash: Root,
}

impl AccumulatorRoot {
    /// A root a consumer attested earlier with [`attest_accumulator_root`] and kept in its own
    /// state (for example an imported root). Trusted input: a consumer restores only what it
    /// stored itself.
    pub fn restore(
        chain_id: u64,
        address: Address,
        slot: Word,
        root: Root,
        block_number: u64,
        block_hash: Root,
    ) -> Self {
        Self { chain_id, address, slot, root, block_number, block_hash }
    }
    /// The chain the root was read on.
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }
    /// The contract that keeps the accumulator.
    pub fn address(&self) -> Address {
        self.address
    }
    /// The storage slot that holds the root.
    pub fn slot(&self) -> Word {
        self.slot
    }
    /// The root.
    pub fn root(&self) -> Root {
        self.root
    }
    /// The block the root was read at.
    pub fn block_number(&self) -> u64 {
        self.block_number
    }
    /// That block's hash.
    pub fn block_hash(&self) -> Root {
        self.block_hash
    }
}

/// The root of the accumulator kept at `slot` of `address`, in the record's state: a storage
/// attestation whose value is the root.
pub fn attest_accumulator_root<P: AsRef<[u8]>>(
    record: &HeaderRecord,
    chain_id: u64,
    address: &Address,
    account_proof: &[P],
    slot: &Word,
    storage_proof: &[P],
) -> Result<AccumulatorRoot, Error> {
    let s = attest_storage(record, chain_id, address, account_proof, slot, storage_proof)?;
    Ok(AccumulatorRoot {
        chain_id: s.chain_id,
        address: *address,
        slot: *slot,
        root: s.fact.value,
        block_number: s.block_number,
        block_hash: s.block_hash,
    })
}

/// A leaf of an attested accumulator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccumulatorLeafFact {
    pub address: Address,
    pub slot: Word,
    pub leaf_index: u64,
    pub leaf: Root,
    /// The root the leaf was proven under (not part of the identifier).
    pub root: Root,
}

/// An inner node: `SHA-256(0x01 ‖ left ‖ right)`.
pub fn accumulator_node(left: &Root, right: &Root) -> Root {
    let mut h = Sha256::new();
    h.update([0x01]);
    h.update(left);
    h.update(right);
    h.finalize().into()
}

/// The root that `path` gives `leaf` at `leaf_index`. Refuses a path that is not 32 siblings
/// (`AccumulatorPathInvalid`) and an index of 2^32 or more (`AccumulatorIndexOutOfRange`).
pub fn accumulator_root_of(leaf_index: u64, leaf: &Root, path: &[Root]) -> Result<Root, Error> {
    if path.len() != ACCUMULATOR_DEPTH {
        return Err(Error::AccumulatorPathInvalid);
    }
    if leaf_index >> ACCUMULATOR_DEPTH != 0 {
        return Err(Error::AccumulatorIndexOutOfRange);
    }
    let mut node = *leaf;
    for (height, sibling) in path.iter().enumerate() {
        node = if (leaf_index >> height) & 1 == 1 {
            accumulator_node(sibling, &node)
        } else {
            accumulator_node(&node, sibling)
        };
    }
    Ok(node)
}

/// `SHA-256("ferrata/fact/acc-leaf/v1" ‖ chain_id (32 bytes) ‖ address ‖ slot ‖ leaf_index
/// (8 bytes) ‖ leaf)`, integers big-endian.
pub fn accumulator_leaf_id(chain_id: u64, address: &Address, slot: &Word, leaf_index: u64, leaf: &Root) -> Root {
    let mut chain = [0u8; 32];
    chain[24..].copy_from_slice(&chain_id.to_be_bytes());
    let mut h = Sha256::new();
    h.update(ACC_LEAF_TAG);
    h.update(chain);
    h.update(address);
    h.update(slot);
    h.update(leaf_index.to_be_bytes());
    h.update(leaf);
    h.finalize().into()
}

/// The leaf at `leaf_index` of the accumulator whose root is `root`: `path` must hash `leaf` to
/// the attested root (`AccumulatorRootMismatch`), and the leaf must not be the empty leaf
/// (`AccumulatorLeafEmpty`). Returns the fact and its identifier, which does not depend on the
/// root.
pub fn attest_accumulator_leaf(
    root: &AccumulatorRoot,
    leaf_index: u64,
    leaf: &Root,
    path: &[Root],
) -> Result<Attested<AccumulatorLeafFact>, Error> {
    let computed = accumulator_root_of(leaf_index, leaf, path)?;
    if *leaf == [0u8; 32] {
        return Err(Error::AccumulatorLeafEmpty);
    }
    if computed != root.root {
        return Err(Error::AccumulatorRootMismatch);
    }
    Ok(Attested {
        chain_id: root.chain_id,
        block_number: root.block_number,
        block_hash: root.block_hash,
        fact: AccumulatorLeafFact { address: root.address, slot: root.slot, leaf_index, leaf: *leaf, root: root.root },
        id: accumulator_leaf_id(root.chain_id, &root.address, &root.slot, leaf_index, leaf),
    })
}
