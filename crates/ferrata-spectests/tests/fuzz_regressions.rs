//! Differences from the reference implementations that the coverage-guided fuzzers of `fuzz/`
//! found and that are intended (fuzz/RESULTS.md); the regressions of fixed findings are in
//! crates/ferrata-core/tests/fuzz_regressions.rs.

use std::collections::BTreeMap;

use alloy_primitives::{Bytes, B256};
use alloy_trie::{
    proof::{verify_proof, ProofRetainer},
    HashBuilder, Nibbles,
};
use ferrata_core::{mpt::verify, Error};

fn alloy_trie(entries: &BTreeMap<Vec<u8>, Vec<u8>>, targets: &[Vec<u8>]) -> (B256, Vec<Vec<Vec<u8>>>) {
    let retainer = ProofRetainer::new(targets.iter().map(|t| Nibbles::unpack(t)).collect());
    let mut hb = HashBuilder::default().with_proof_retainer(retainer);
    for (k, v) in entries {
        hb.add_leaf(Nibbles::unpack(k), v);
    }
    let root = hb.root();
    let nodes = hb.take_proof_nodes();
    let proofs = targets
        .iter()
        .map(|t| {
            nodes
                .matching_nodes_sorted(&Nibbles::unpack(t))
                .into_iter()
                .filter(|(path, node)| path.is_empty() || node.len() >= 32)
                .map(|(_, node)| node.to_vec())
                .collect()
        })
        .collect();
    (root, proofs)
}

fn alloy_accepts(root: &[u8; 32], key: &[u8], expected: Option<&[u8]>, proof: &[Vec<u8>]) -> bool {
    let nodes: Vec<Bytes> = proof.iter().map(|n| Bytes::copy_from_slice(n)).collect();
    verify_proof(B256::from(*root), Nibbles::unpack(key), expected.map(<[u8]>::to_vec), &nodes).is_ok()
}

/// Finding P1 (fuzz target `mpt`, a difference from alloy-trie 0.9.9, intended): an exclusion
/// proof that ends at an extension whose path diverges from the key, padded with the
/// extension's (correctly hashed) child. alloy keeps walking below the diverging extension and
/// accepts the padded proof; ferrata ends the walk where the trie shows the key is absent and
/// refuses the unused node (`ProofTrailingNodes`), so every statement keeps exactly one proof.
/// Both accept the unpadded proof, which is what `eth_getProof` returns.
#[test]
fn exclusion_proof_padded_below_a_diverging_extension() {
    let mut entries = BTreeMap::new();
    for last in [1u8, 2] {
        let mut key = vec![0xaa; 32];
        key[31] = last;
        entries.insert(key, vec![last; 40]);
    }
    let present = entries.keys().next().unwrap().clone();
    let absent = vec![0xbb; 32];
    let (root, proofs) = alloy_trie(&entries, &[absent.clone(), present]);
    let root: [u8; 32] = root.0;
    let (honest, donor) = (&proofs[0], &proofs[1]);
    assert_eq!(honest.len(), 1, "the root extension diverges from the absent key");
    assert_eq!(verify(&root, &absent, honest), Ok(None));
    assert!(alloy_accepts(&root, &absent, None, honest));

    let padded = vec![honest[0].clone(), donor[1].clone()];
    assert_eq!(verify(&root, &absent, &padded), Err(Error::ProofTrailingNodes));
    assert!(alloy_accepts(&root, &absent, None, &padded), "alloy-trie 0.9.9 accepts the padded proof");
}
