//! Differences from the reference implementations that the coverage-guided fuzzers of `fuzz/`
//! found and that are intended, and the one difference between the two BLS libraries that the `bls`
//! target requires (fuzz/RESULTS.md); the regressions of fixed findings are in
//! crates/ferrata-core/tests/fuzz_regressions.rs.

use std::collections::BTreeMap;

use alloy_primitives::{Bytes, B256};
use alloy_trie::{
    proof::{verify_proof, ProofRetainer},
    HashBuilder, Nibbles,
};
use bls12_381::{G1Affine, G1Projective};
use ferrata_core::{
    bls::{Aggregation, BlsBackend, Blst, Zkcrypto},
    mpt::verify,
    types::{BlsPubkey, SyncCommittee},
    Error,
};
use ferrata_spectests::testkit::{fake_committee, sign};

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

/// Difference B1 (fuzz target `bls`, between the two BLS libraries; known from the code before the
/// target was written, so the target requires it instead of reporting it; intended): the two points
/// with x = 0, (0, 2) and (0, -2), are on the curve and of order 3. blst's decoder refuses them
/// even where only the on-curve check is asked (`blst_p1_uncompress` returns
/// `BLST_POINT_NOT_IN_GROUP`, `bls/blst_backend.rs`, `uncompress`); zkcrypto's on-curve decoding
/// (`G1Affine::from_compressed_unchecked`) takes them. So on the two paths that decode keys with
/// the on-curve check only, `check_committee_onchain` and `verify_checked`, a committee holding one
/// is refused by blst and may be accepted by zkcrypto: here a committee `[k, (0, 2)]` whose
/// aggregate is their sum, `k` signing, verifies under zkcrypto's subtractive composition
/// (aggregate minus the non-signer is `k`) and is refused by blst. `KeyValidate`
/// (`check_committee`, `fast_aggregate_verify`) refuses the point in both. blst, which the programs
/// use, is the stricter side, and no Ethereum committee holds such a key (each passed `KeyValidate`
/// at its deposit). The `bls` target requires exactly this: blst's refusal wherever such a key is
/// decoded on those paths.
#[test]
fn order_three_keys_on_the_on_curve_paths() {
    let mut order_three: BlsPubkey = [0u8; 48];
    order_three[0] = 0x80;
    let t = G1Affine::from_compressed_unchecked(&order_three).unwrap();
    assert!(!bool::from(t.is_torsion_free()), "(0, 2) is outside G1");
    let fake = fake_committee(1, 0x5a);
    let k = fake.committee.pubkeys[0];
    let sum = G1Projective::from(G1Affine::from_compressed(&k).unwrap()) + t;
    let committee =
        SyncCommittee { pubkeys: vec![k, order_three], aggregate_pubkey: G1Affine::from(sum).to_compressed() };
    let bits = [true, false];
    let message = [0x21u8; 32];
    let signature = sign(&fake, &[true], &message);

    for a in [Aggregation::Additive, Aggregation::Subtractive, Aggregation::Auto] {
        let (blst, zkcrypto) = (Blst::new(a), Zkcrypto::new(a));
        assert_eq!(blst.check_committee(&committee), Err(Error::CommitteeInvalid));
        assert_eq!(zkcrypto.check_committee(&committee), Err(Error::CommitteeInvalid));
        assert_eq!(blst.fast_aggregate_verify(&committee, &bits, &message, &signature), Ok(()), "k alone signs");
        assert_eq!(zkcrypto.fast_aggregate_verify(&committee, &bits, &message, &signature), Ok(()));
        assert_eq!(blst.check_committee_onchain(&committee), Err(Error::CommitteeInvalid), "blst: {a:?}");
        assert_eq!(zkcrypto.check_committee_onchain(&committee), Ok(()), "zkcrypto: {a:?}");
    }
    // The additive composition (and Auto, at one signer of two) decodes the signer only.
    for a in [Aggregation::Additive, Aggregation::Auto] {
        assert_eq!(Blst::new(a).verify_checked(&committee, &bits, &message, &signature), Ok(()));
        assert_eq!(Zkcrypto::new(a).verify_checked(&committee, &bits, &message, &signature), Ok(()));
    }
    // The subtractive one decodes the non-signer, (0, 2).
    let a = Aggregation::Subtractive;
    assert_eq!(Blst::new(a).verify_checked(&committee, &bits, &message, &signature), Err(Error::PubkeyInvalid));
    assert_eq!(Zkcrypto::new(a).verify_checked(&committee, &bits, &message, &signature), Ok(()));
}
