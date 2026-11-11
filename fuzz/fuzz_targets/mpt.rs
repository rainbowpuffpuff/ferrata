//! Merkle-Patricia proofs (`ferrata_core::mpt::verify`) against alloy-trie 0.9.9.
//!
//! Mode 0 (structured): the fuzzer chooses a trie (fixed-length keys, or receipt-trie keys
//! `RLP(i)`), a key (present or absent) and a mutation of its proof. alloy's `HashBuilder`
//! builds the root and the proof. The honest proof must verify to the trie's value; a mutated
//! proof that ferrata accepts must prove the same value and be accepted by alloy's
//! `verify_proof`; a mutated proof ferrata refuses must be refused by alloy for both the true
//! value and absence.
//!
//! Mode 1 (raw): arbitrary key and nodes, the root being the hash of the first node. Whatever
//! ferrata accepts, alloy must accept with the same value.
//!
//! Documented differences (crates/ferrata-spectests/tests/mpt_proofs.rs,
//! `documented_differences_from_alloy`) are excluded: a root node shorter than 32 bytes, and
//! the empty trie proven by `[0x80]`.
#![no_main]
use std::collections::BTreeMap;

use alloy_primitives::{Bytes, B256};
use alloy_trie::{
    proof::{verify_proof, ProofRetainer},
    HashBuilder, Nibbles,
};
use ferrata_core::{keccak::keccak256, mpt::verify, rlp::encode_u64};
use ferrata_fuzz::{parse_raw_proof, Input};
use libfuzzer_sys::fuzz_target;

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

fn structured(i: &mut Input<'_>) {
    let family = i.u8() % 3;
    let key_len = match family {
        0 => 32,
        1 => 1 + usize::from(i.u8() % 6),
        _ => 0,
    };
    let count = 1 + usize::from(i.u8() % 48);
    let mut entries = BTreeMap::new();
    for n in 0..count {
        let key = if family == 2 {
            let (b, l) = encode_u64(u64::from(i.u16() % 300).max(n as u64 % 300));
            b[..l].to_vec()
        } else {
            i.take(key_len).iter().copied().chain(core::iter::repeat(n as u8)).take(key_len).collect()
        };
        let vlen = 1 + usize::from(i.u8() % 70);
        let value: Vec<u8> = i.take(vlen).iter().copied().chain(core::iter::repeat(0xab)).take(vlen).collect();
        entries.insert(key, value);
    }
    let keys: Vec<Vec<u8>> = entries.keys().cloned().collect();
    let pick = i.u8();
    let target = if pick & 0x80 == 0 {
        keys[usize::from(pick) % keys.len()].clone()
    } else if family == 2 {
        let (b, l) = encode_u64(u64::from(i.u16()));
        b[..l].to_vec()
    } else {
        i.take(key_len).iter().copied().chain(core::iter::repeat(0x5a)).take(key_len).collect()
    };
    let donor_key = keys[usize::from(i.u8()) % keys.len()].clone();
    let (root, proofs) = alloy_trie(&entries, &[target.clone(), donor_key]);
    let root: [u8; 32] = root.0;
    let (proof, donor) = (&proofs[0], &proofs[1]);
    let truth = entries.get(&target).map(Vec::as_slice);
    // Completeness: the honest proof proves the truth.
    assert_eq!(verify(&root, &target, proof), Ok(truth), "honest proof refused or wrong");
    if proof.first().is_some_and(|n| n.len() < 32) {
        return; // documented difference 2: alloy refuses every proof of such a trie
    }
    assert!(alloy_accepts(&root, &target, truth, proof));
    // One mutation, chosen by the fuzzer.
    let mut p = proof.clone();
    match i.u8() % 7 {
        0 if !p.is_empty() => {
            let a = usize::from(i.u8()) % p.len();
            let b = usize::from(i.u16()) % p[a].len();
            p[a][b] ^= 1 << (i.u8() % 8);
        }
        1 if !p.is_empty() => {
            p.remove(usize::from(i.u8()) % p.len());
        }
        2 if !donor.is_empty() => {
            let at = usize::from(i.u8()) % (p.len() + 1);
            p.insert(at, donor[usize::from(i.u8()) % donor.len()].clone());
        }
        3 if p.len() > 1 => {
            let (a, b) = (usize::from(i.u8()) % p.len(), usize::from(i.u8()) % p.len());
            p.swap(a, b);
        }
        4 if !p.is_empty() => {
            let a = usize::from(i.u8()) % p.len();
            let keep = usize::from(i.u16()) % p[a].len();
            p[a].truncate(keep);
        }
        5 if !p.is_empty() => {
            let a = usize::from(i.u8()) % p.len();
            let extra = i.take(8).to_vec();
            p[a].extend_from_slice(&extra);
        }
        _ => p.push(donor.first().cloned().unwrap_or_else(|| vec![0x80])),
    }
    match verify(&root, &target, &p) {
        Ok(v) => {
            assert_eq!(v, truth, "a mutated proof proves something false");
            assert!(alloy_accepts(&root, &target, v, &p), "ferrata accepts a proof alloy refuses");
        }
        Err(_) => {
            assert!(
                !alloy_accepts(&root, &target, truth, &p) && !alloy_accepts(&root, &target, None, &p),
                "alloy accepts a proof ferrata refuses"
            );
        }
    }
}

fn raw(i: &mut Input<'_>) {
    let (key, nodes) = parse_raw_proof(i);
    let Some(first) = nodes.first() else { return };
    let root = keccak256(first);
    if let Ok(v) = verify(&root, &key, &nodes) {
        if first.len() >= 32 {
            assert!(alloy_accepts(&root, &key, v, &nodes), "ferrata accepts a proof alloy refuses");
        }
        // The value is a part of a proof node, never invented.
        if let Some(v) = v {
            assert!(!v.is_empty() && nodes.iter().any(|n| n.windows(v.len()).any(|w| w == v)));
        }
    }
}

fuzz_target!(|data: &[u8]| {
    let mut i = Input(data);
    if i.u8() & 1 == 0 {
        structured(&mut i)
    } else {
        raw(&mut i)
    }
});
