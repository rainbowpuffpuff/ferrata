//! Merkle helpers and the generalized indices the light client proves against.
//!
//! Branches are ordered leaf level first, as in the consensus specs: `branch[0]` is the
//! sibling of the leaf and `branch[depth - 1]` the sibling just below the root.

use alloc::{vec, vec::Vec};

use crate::{hash::hash_pair, Root};

/// The all-zero chunk.
pub const ZERO: Root = [0u8; 32];

// Generalized indices, from the light-client specs at consensus-specs v1.7.0-beta.4.
/// `altair.BeaconState.finalized_checkpoint.root`.
pub const FINALIZED_ROOT_GINDEX: u64 = 105;
/// `altair.BeaconState.current_sync_committee`.
pub const CURRENT_SYNC_COMMITTEE_GINDEX: u64 = 54;
/// `altair.BeaconState.next_sync_committee`.
pub const NEXT_SYNC_COMMITTEE_GINDEX: u64 = 55;
/// `electra.BeaconState.finalized_checkpoint.root`.
pub const FINALIZED_ROOT_GINDEX_ELECTRA: u64 = 169;
/// `electra.BeaconState.current_sync_committee`.
pub const CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA: u64 = 86;
/// `electra.BeaconState.next_sync_committee`.
pub const NEXT_SYNC_COMMITTEE_GINDEX_ELECTRA: u64 = 87;
/// `gloas.BeaconState.finalized_checkpoint.root` (EIP-7688 progressive container).
pub const FINALIZED_ROOT_GINDEX_GLOAS: u64 = 735;
/// `gloas.BeaconState.current_sync_committee`.
pub const CURRENT_SYNC_COMMITTEE_GINDEX_GLOAS: u64 = 2945;
/// `gloas.BeaconState.next_sync_committee`.
pub const NEXT_SYNC_COMMITTEE_GINDEX_GLOAS: u64 = 2946;
/// `capella.BeaconBlockBody.execution_payload`.
pub const EXECUTION_PAYLOAD_GINDEX: u64 = 25;
/// `capella.BeaconBlockBody.execution_payload.block_hash`.
pub const EXECUTION_BLOCK_HASH_GINDEX: u64 = 412;
/// `deneb.BeaconBlockBody.execution_payload.block_hash`.
pub const EXECUTION_BLOCK_HASH_GINDEX_DENEB: u64 = 812;
/// `gloas.BeaconBlockBody.signed_execution_payload_bid.message.parent_block_hash`.
///
/// Under EIP-7732 a block commits to a payload *bid* and the payload can be withheld, so the
/// light-client header binds the bid's parent block hash: the last *delivered* payload.
pub const EXECUTION_BLOCK_HASH_GINDEX_GLOAS: u64 = 2856;
/// `block_hash` inside `capella.ExecutionPayloadHeader` (15 fields, 16 leaves): 16 + 12.
pub const BLOCK_HASH_IN_CAPELLA_PAYLOAD_GINDEX: u64 = 28;
/// `block_hash` inside `deneb.ExecutionPayloadHeader` (17 fields, 32 leaves): 32 + 12.
pub const BLOCK_HASH_IN_DENEB_PAYLOAD_GINDEX: u64 = 44;

/// `floorlog2(x)` for `x > 0`.
pub const fn floorlog2(x: u64) -> usize {
    (63 - x.leading_zeros()) as usize
}

/// `get_subtree_index(gindex)`: the leaf position of `gindex` at its depth.
pub const fn subtree_index(gindex: u64) -> u64 {
    gindex % (1u64 << floorlog2(gindex))
}

fn next_layer(layer: &[Root]) -> Vec<Root> {
    layer.chunks(2).map(|pair| hash_pair(&pair[0], &pair[1])).collect()
}

/// Root of `leaves` padded with zero chunks to `width` leaves (`width` a power of two).
pub fn merkleize(leaves: &[Root], width: usize) -> Root {
    assert!(width.is_power_of_two() && leaves.len() <= width, "merkleize: bad width");
    let mut layer = leaves.to_vec();
    layer.resize(width, ZERO);
    while layer.len() > 1 {
        layer = next_layer(&layer);
    }
    layer[0]
}

/// Sibling path (leaf level first) for `index` in a tree of `leaves` padded to `width`.
pub fn merkle_proof(leaves: &[Root], width: usize, index: usize) -> Vec<Root> {
    assert!(width.is_power_of_two() && leaves.len() <= width && index < width);
    let mut layer = leaves.to_vec();
    layer.resize(width, ZERO);
    let mut idx = index;
    let mut proof = Vec::with_capacity(floorlog2(width as u64));
    while layer.len() > 1 {
        proof.push(layer[idx ^ 1]);
        layer = next_layer(&layer);
        idx /= 2;
    }
    proof
}

/// `compute_merkle_branch_root`. `None` if the branch is shorter than `depth`.
pub fn compute_merkle_branch_root(leaf: &Root, branch: &[Root], depth: usize, index: u64) -> Option<Root> {
    if branch.len() < depth {
        return None;
    }
    let mut value = *leaf;
    for (i, sibling) in branch.iter().take(depth).enumerate() {
        value = if (index >> i) & 1 == 1 { hash_pair(sibling, &value) } else { hash_pair(&value, sibling) };
    }
    Some(value)
}

/// `is_valid_merkle_branch`.
pub fn is_valid_merkle_branch(leaf: &Root, branch: &[Root], depth: usize, index: u64, root: &Root) -> bool {
    compute_merkle_branch_root(leaf, branch, depth, index).as_ref() == Some(root)
}

/// `is_valid_normalized_merkle_branch`: a branch zero-padded at the leaf end to a later
/// fork's depth verifies only if every padding entry is zero.
pub fn is_valid_normalized_merkle_branch(leaf: &Root, branch: &[Root], gindex: u64, root: &Root) -> bool {
    let depth = floorlog2(gindex);
    if branch.len() < depth {
        return false;
    }
    let num_extra = branch.len() - depth;
    if branch[..num_extra].iter().any(|b| *b != ZERO) {
        return false;
    }
    is_valid_merkle_branch(leaf, &branch[num_extra..], depth, subtree_index(gindex), root)
}

/// `normalize_merkle_branch`: zero-pad a branch to the depth of `gindex`.
pub fn normalize_merkle_branch(branch: &[Root], gindex: u64) -> Vec<Root> {
    let num_extra = floorlog2(gindex).saturating_sub(branch.len());
    let mut out = vec![ZERO; num_extra];
    out.extend_from_slice(branch);
    out
}

/// True if every entry is zero (an absent branch).
pub fn is_zero_branch(branch: &[Root]) -> bool {
    branch.iter().all(|b| *b == ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depths_match_branch_types() {
        assert_eq!(floorlog2(FINALIZED_ROOT_GINDEX), 6);
        assert_eq!(floorlog2(CURRENT_SYNC_COMMITTEE_GINDEX), 5);
        assert_eq!(floorlog2(FINALIZED_ROOT_GINDEX_ELECTRA), 7);
        assert_eq!(floorlog2(CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA), 6);
        assert_eq!(floorlog2(FINALIZED_ROOT_GINDEX_GLOAS), 9);
        assert_eq!(floorlog2(CURRENT_SYNC_COMMITTEE_GINDEX_GLOAS), 11);
        assert_eq!(floorlog2(EXECUTION_BLOCK_HASH_GINDEX_GLOAS), 11);
        assert_eq!(floorlog2(EXECUTION_PAYLOAD_GINDEX), 4);
    }

    #[test]
    fn concatenated_gindices() {
        // gindex(a.b) = a << depth(b) | subtree_index(b)
        let concat = |a: u64, b: u64| (a << floorlog2(b)) | subtree_index(b);
        assert_eq!(concat(EXECUTION_PAYLOAD_GINDEX, BLOCK_HASH_IN_CAPELLA_PAYLOAD_GINDEX), EXECUTION_BLOCK_HASH_GINDEX);
        assert_eq!(
            concat(EXECUTION_PAYLOAD_GINDEX, BLOCK_HASH_IN_DENEB_PAYLOAD_GINDEX),
            EXECUTION_BLOCK_HASH_GINDEX_DENEB
        );
        // The same field sits at the same index one level deeper after Electra's state growth.
        assert_eq!(subtree_index(FINALIZED_ROOT_GINDEX), subtree_index(FINALIZED_ROOT_GINDEX_ELECTRA));
    }

    #[test]
    fn proof_roundtrip_and_normalisation() {
        let leaves: Vec<Root> = (0u8..5).map(|i| [i; 32]).collect();
        let root = merkleize(&leaves, 8);
        for i in 0..5 {
            let proof = merkle_proof(&leaves, 8, i);
            assert!(is_valid_merkle_branch(&leaves[i], &proof, 3, i as u64, &root));
            let gindex = 8 + i as u64;
            let padded = normalize_merkle_branch(&proof, gindex << 2);
            assert_eq!(padded.len(), 5);
            // Padded to a deeper gindex with the same subtree position: verifies only via the
            // normalized check, and only while the padding stays zero.
            assert!(is_valid_normalized_merkle_branch(&leaves[i], &padded, gindex, &root));
            let mut bad = padded.clone();
            bad[0] = [1; 32];
            assert!(!is_valid_normalized_merkle_branch(&leaves[i], &bad, gindex, &root));
        }
        assert!(!is_valid_normalized_merkle_branch(&leaves[0], &[ZERO; 2], 8, &root));
    }
}
