//! Merkle-Patricia proofs: the official `ethereum/tests` TrieTests, random tries against
//! `alloy-trie` (roots, proofs and verdicts, both ways), and the node decoder's every rule.

use std::{collections::BTreeMap, fs, path::PathBuf};

use alloy_primitives::{Bytes, B256};
use alloy_trie::{
    proof::{verify_proof, ProofRetainer},
    HashBuilder, Nibbles,
};
use ferrata_core::{
    mpt::{verify, EMPTY_ROOT},
    rlp::encode_u64,
    Error,
};
use ferrata_spectests::execkit::{enc, hex_prefix, keccak, relink, Rng, Trie};
use serde_json::Value;

fn trie_vectors_dir() -> PathBuf {
    std::env::var_os("FERRATA_TRIE_VECTORS").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vectors/ethereum-tests-c67e485/TrieTests")
    })
}

fn text(s: &str) -> Vec<u8> {
    match s.strip_prefix("0x") {
        Some(h) => hex::decode(h).unwrap(),
        None => s.as_bytes().to_vec(),
    }
}

/// Every key of `trie` proves its value; deleted keys and near misses prove absence.
fn prove_everything(trie: &Trie, extra_absent: &[Vec<u8>]) -> usize {
    let root = trie.root();
    let mut checked = 0;
    let mut keys = trie.keys();
    for k in trie.keys() {
        keys.push([k.as_slice(), &[0x00]].concat());
        keys.push([k.as_slice(), &[0x5a]].concat());
        if !k.is_empty() {
            keys.push(k[..k.len() - 1].to_vec());
        }
    }
    keys.extend(extra_absent.iter().cloned());
    for k in keys {
        let proof = trie.proof(&k);
        assert_eq!(verify(&root, &k, &proof), Ok(trie.get(&k)), "key {}", hex::encode(&k));
        checked += 1;
    }
    checked
}

#[test]
fn ethereum_tests_trie_vectors() {
    let dir = trie_vectors_dir();
    let mut cases = 0;
    let mut proofs = 0;
    for (file, secure) in [
        ("trietest.json", false),
        ("trieanyorder.json", false),
        ("trietest_secureTrie.json", true),
        ("trieanyorder_secureTrie.json", true),
        ("hex_encoded_securetrie_test.json", true),
    ] {
        let path = dir.join(file);
        let text_json = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: {e} (run scripts/fetch-vectors.sh)", path.display()));
        let all: BTreeMap<String, Value> = serde_json::from_str(&text_json).unwrap();
        for (name, case) in all {
            let pairs: Vec<(Vec<u8>, Option<Vec<u8>>)> = match &case["in"] {
                Value::Array(a) => a.iter().map(|p| (text(p[0].as_str().unwrap()), p[1].as_str().map(text))).collect(),
                Value::Object(m) => m.iter().map(|(k, v)| (text(k), v.as_str().map(text))).collect(),
                other => panic!("{name}: {other:?}"),
            };
            let key = |k: &[u8]| if secure { keccak(k).to_vec() } else { k.to_vec() };
            let deleted: Vec<Vec<u8>> = pairs.iter().filter(|(_, v)| v.is_none()).map(|(k, _)| key(k)).collect();
            let trie = Trie::new(pairs.iter().map(|(k, v)| (key(k), v.clone().unwrap_or_default())));
            let want: [u8; 32] = text(case["root"].as_str().unwrap()).try_into().unwrap();
            assert_eq!(trie.root(), want, "{file} {name}: root");
            proofs += prove_everything(&trie, &deleted);
            cases += 1;
        }
    }
    // trietestnextprev.json tests iteration order (next/prev key), not roots or proofs.
    assert_eq!(cases, 25, "every TrieTests case with a root ran");
    assert_eq!(proofs, 368);
    println!("TrieTests: {cases} cases, {proofs} proofs verified");
}

/// The proofs alloy's `ProofRetainer` keeps for `target`, in `eth_getProof`'s convention: the
/// root and every node of 32 bytes or more (alloy also lists embedded nodes, which live inside
/// their parent).
fn alloy_trie(entries: &BTreeMap<Vec<u8>, Vec<u8>>, targets: &[Vec<u8>]) -> (B256, Vec<Vec<Vec<u8>>>) {
    let retainer = ProofRetainer::new(targets.iter().map(Nibbles::unpack).collect());
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

/// One family of random tries.
fn random_entries(rng: &mut Rng, family: usize) -> BTreeMap<Vec<u8>, Vec<u8>> {
    let mut m = BTreeMap::new();
    let n = 1 + rng.below(if family == 1 { 40 } else { 200 });
    let prefix_len = 29 + rng.below(3);
    let prefix = rng.bytes(prefix_len);
    for i in 0..n {
        let key = match family {
            // Secure-trie keys.
            0 => rng.bytes(32),
            // A long shared prefix: deep leaves, embedded nodes.
            1 => [prefix.as_slice(), &rng.bytes(32 - prefix_len)].concat(),
            // Receipts-trie keys.
            2 => {
                let (b, l) = encode_u64(i as u64);
                b[..l].to_vec()
            }
            // Dense two-byte keys.
            _ => rng.bytes(2),
        };
        let len = match rng.below(4) {
            0 => 1,
            1 => 1 + rng.below(4),
            2 => 1 + rng.below(40),
            _ => 1 + rng.below(300),
        };
        m.insert(key, rng.bytes(len));
    }
    m
}

#[test]
fn random_tries_match_alloy_roots_proofs_and_verdicts() {
    let mut rng = Rng::new(0x7_7215);
    let (mut inclusion, mut exclusion, mut embedded_tries, mut tiny_roots) = (0, 0, 0, 0);
    for round in 0..240 {
        let family = round % 4;
        let entries = random_entries(&mut rng, family);
        let trie = Trie::new(entries.iter().map(|(k, v)| (k.clone(), v.clone())));
        let root = trie.root();
        let mut targets: Vec<Vec<u8>> = entries.keys().cloned().collect();
        // Absent keys: random, and near misses of present keys.
        let key_len = entries.keys().next().unwrap().len();
        for k in entries.keys().take(20) {
            let mut miss = k.clone();
            *miss.last_mut().unwrap() ^= 0x01;
            targets.push(miss);
            let mut miss = k.clone();
            *miss.last_mut().unwrap() ^= 0x10;
            targets.push(miss);
        }
        for _ in 0..10 {
            targets.push(rng.bytes(key_len.max(1)));
        }
        if family == 2 {
            targets.extend((entries.len()..entries.len() + 5).map(|i| {
                let (b, l) = encode_u64(i as u64);
                b[..l].to_vec()
            }));
        }
        targets.sort();
        targets.dedup();
        let (alloy_root, alloy_proofs) = alloy_trie(&entries, &targets);
        assert_eq!(B256::from(root), alloy_root, "family {family}: roots differ");
        // alloy refuses every proof of a trie whose root node is under 32 bytes (see
        // `documented_differences_from_alloy`).
        let tiny_root = trie.proof(&targets[0])[0].len() < 32;
        tiny_roots += usize::from(tiny_root);
        let mut saw_embedded = false;
        for (k, theirs) in targets.iter().zip(&alloy_proofs) {
            let ours = trie.proof(k);
            assert_eq!(&ours, theirs, "family {family}: proofs differ for {}", hex::encode(k));
            let want = entries.get(k).map(Vec::as_slice);
            assert_eq!(verify(&root, k, &ours), Ok(want), "ferrata, family {family}");
            assert_eq!(alloy_accepts(&root, k, want, &ours), !tiny_root, "alloy, family {family}");
            // The opposite claim is refused by alloy; ferrata returned the true answer above.
            let other = if want.is_some() { None } else { Some(&b"x"[..]) };
            assert!(!alloy_accepts(&root, k, other, &ours));
            if want.is_some() {
                inclusion += 1;
            } else {
                exclusion += 1;
            }
            saw_embedded |= has_embedded_child(&ours);
        }
        embedded_tries += usize::from(saw_embedded);
    }
    println!(
        "{inclusion} inclusion and {exclusion} exclusion proofs agree; {embedded_tries} tries had embedded nodes, \
         {tiny_roots} a root node under 32 bytes"
    );
    assert_eq!((inclusion, exclusion, embedded_tries, tiny_roots), (19_173, 8_968, 179, 1));
}

/// True if a node of the proof holds an embedded child (a list where a reference belongs).
fn has_embedded_child(proof: &[Vec<u8>]) -> bool {
    proof.iter().any(|node| {
        let item = ferrata_core::rlp::decode(node).unwrap();
        item.list().unwrap().any(|i| i.unwrap().is_list)
    })
}

fn mutate(rng: &mut Rng, proof: &[Vec<u8>], donor: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let mut p = proof.to_vec();
    match rng.below(6) {
        0 if !p.is_empty() => {
            let i = rng.below(p.len());
            let j = rng.below(p[i].len());
            p[i][j] ^= 1 << rng.below(8);
        }
        1 if !p.is_empty() => {
            p.remove(rng.below(p.len()));
        }
        2 if !donor.is_empty() => {
            let at = rng.below(p.len() + 1);
            p.insert(at, donor[rng.below(donor.len())].clone());
        }
        3 if p.len() > 1 => {
            let (i, j) = (rng.below(p.len()), rng.below(p.len()));
            p.swap(i, j);
        }
        4 if !p.is_empty() => {
            let i = rng.below(p.len());
            let keep = rng.below(p[i].len());
            p[i].truncate(keep);
        }
        _ => p.push(donor.first().cloned().unwrap_or_else(|| vec![0x80])),
    }
    p
}

#[test]
fn mutated_proofs_get_the_same_verdict_from_ferrata_and_alloy() {
    let mut rng = Rng::new(0xbad_9f00f);
    let (mut refused, mut still_valid) = (0, 0);
    for round in 0..120 {
        let entries = random_entries(&mut rng, round % 4);
        let trie = Trie::new(entries.iter().map(|(k, v)| (k.clone(), v.clone())));
        let root = trie.root();
        let keys: Vec<Vec<u8>> = entries.keys().cloned().collect();
        if trie.proof(&keys[0])[0].len() < 32 {
            continue; // alloy refuses every proof of such a trie
        }
        for _ in 0..40 {
            let k = &keys[rng.below(keys.len())];
            let donor = trie.proof(&keys[rng.below(keys.len())]);
            let p = mutate(&mut rng, &trie.proof(k), &donor);
            let truth = entries.get(k).map(Vec::as_slice);
            match verify(&root, k, &p) {
                Ok(v) => {
                    // A mutation that leaves a valid proof (a swap of equal nodes, a node that
                    // was not on the path dropped from an exclusion proof...) must prove the truth.
                    assert_eq!(v, truth);
                    assert!(alloy_accepts(&root, k, v, &p));
                    still_valid += 1;
                }
                Err(e) => {
                    assert!(
                        !alloy_accepts(&root, k, truth, &p) && !alloy_accepts(&root, k, None, &p),
                        "alloy accepts a proof ferrata refuses ({e:?})"
                    );
                    refused += 1;
                }
            }
        }
    }
    println!("mutated proofs: {refused} refused by both, {still_valid} still valid for both");
    assert_eq!((refused, still_valid), (4_453, 307));
}

#[test]
fn documented_differences_from_alloy() {
    // 1. The empty trie: ferrata takes only the empty proof; alloy also takes [0x80].
    assert_eq!(verify::<Vec<u8>>(&EMPTY_ROOT, b"k", &[]), Ok(None));
    assert!(alloy_accepts(&EMPTY_ROOT, b"k", None, &[]));
    assert_eq!(verify(&EMPTY_ROOT, b"k", &[vec![0x80]]), Err(Error::ProofNodeInvalid));
    assert!(alloy_accepts(&EMPTY_ROOT, b"k", None, &[vec![0x80]]));
    // 2. A trie whose root node is shorter than 32 bytes: the root is still its hash (Yellow
    //    Paper), and ferrata proves its keys; alloy-trie 0.9.9 compares the inline node with the
    //    root hash and refuses. Ethereum's state, storage and receipt tries never have such a
    //    root: one leaf with a 32-byte key or a receipt is already longer.
    let trie = Trie::new([(vec![0x01], vec![0x02])]);
    let proof = trie.proof(&[0x01]);
    assert!(proof[0].len() < 32);
    assert_eq!(verify(&trie.root(), &[0x01], &proof), Ok(Some(&[0x02][..])));
    assert!(!alloy_accepts(&trie.root(), &[0x01], Some(&[0x02]), &proof));
}

#[test]
fn values_at_branch_slots_and_prefix_keys() {
    // "do", "dog", "doge": values in branches (TrieTests' shape, not Ethereum's).
    let trie = Trie::new([
        (b"do".to_vec(), b"verb".to_vec()),
        (b"dog".to_vec(), b"puppy".to_vec()),
        (b"doge".to_vec(), b"coin".to_vec()),
        (b"horse".to_vec(), b"stallion".to_vec()),
    ]);
    let root = trie.root();
    for (k, v) in [(&b"do"[..], &b"verb"[..]), (b"dog", b"puppy"), (b"doge", b"coin"), (b"horse", b"stallion")] {
        assert_eq!(verify(&root, k, &trie.proof(k)), Ok(Some(v)));
    }
    for k in [&b"d"[..], b"dogs", b"doges", b"h", b"horses", b"", b"cat"] {
        assert_eq!(verify(&root, k, &trie.proof(k)), Ok(None), "{}", String::from_utf8_lossy(k));
    }
}

/// A proof for one key of a large secure trie, to forge nodes in.
fn sample() -> (Trie, Vec<u8>, Vec<Vec<u8>>) {
    let mut rng = Rng::new(42);
    let trie = Trie::new((0..300).map(|_| (rng.bytes(32), rng.bytes(40))));
    let key = trie.keys()[123].clone();
    let proof = trie.proof(&key);
    assert!(proof.len() >= 3);
    (trie, key, proof)
}

/// Forge the last node of the sample proof (a leaf) and re-link the proof to a new root.
fn forged_leaf(f: impl Fn(&[u8], &[u8]) -> Vec<u8>) -> Result<Option<Vec<u8>>, Error> {
    let (_, key, mut proof) = sample();
    let last = proof.len() - 1;
    let leaf = ferrata_core::rlp::decode(&proof[last]).unwrap();
    let [path, value] = leaf.items::<2>().unwrap();
    let node = f(path.payload, value.payload);
    let root = relink(&mut proof, last, node);
    verify(&root, &key, &proof).map(|v| v.map(<[u8]>::to_vec))
}

#[test]
fn node_rules_behind_the_hash_check() {
    // The genuine leaf, re-linked unchanged, still verifies.
    let genuine = forged_leaf(|p, v| enc::list(&[enc::bytes(p), enc::bytes(v)]));
    assert!(matches!(genuine, Ok(Some(_))));
    // Hex-prefix flags 4 to 15 are not node kinds.
    for flag in 4u8..16 {
        let r = forged_leaf(|p, v| {
            let mut p = p.to_vec();
            p[0] = (flag << 4) | (p[0] & 0x0f);
            enc::list(&[enc::bytes(&p), enc::bytes(v)])
        });
        assert_eq!(r, Err(Error::ProofNodeInvalid), "flag {flag}");
    }
    // An even path pads its flag byte with a zero nibble.
    let r = forged_leaf(|p, v| {
        let mut p = p.to_vec();
        if p[0] >> 4 == 2 {
            p[0] |= 0x01;
        } else {
            p.insert(1, (p[0] & 0x0f) << 4);
            p[0] = 0x2f;
        }
        enc::list(&[enc::bytes(&p), enc::bytes(v)])
    });
    assert_eq!(r, Err(Error::ProofNodeInvalid), "padding nibble");
    // An empty path string, a list as the path, an empty value, a list as the value.
    assert_eq!(forged_leaf(|_, v| enc::list(&[enc::bytes(&[]), enc::bytes(v)])), Err(Error::ProofNodeInvalid));
    assert_eq!(
        forged_leaf(|p, v| enc::list(&[enc::list(&[enc::bytes(p)]), enc::bytes(v)])),
        Err(Error::ProofNodeInvalid)
    );
    assert_eq!(forged_leaf(|p, _| enc::list(&[enc::bytes(p), enc::bytes(&[])])), Err(Error::ProofNodeInvalid));
    assert_eq!(
        forged_leaf(|p, v| enc::list(&[enc::bytes(p), enc::list(&[enc::bytes(v)])])),
        Err(Error::ProofNodeInvalid)
    );
    // Wrong arity: 1, 3, 16 and 18 items; and a string instead of a list.
    assert_eq!(forged_leaf(|p, _| enc::list(&[enc::bytes(p)])), Err(Error::ProofNodeInvalid));
    assert_eq!(
        forged_leaf(|p, v| enc::list(&[enc::bytes(p), enc::bytes(v), enc::bytes(v)])),
        Err(Error::ProofNodeInvalid)
    );
    assert_eq!(forged_leaf(|p, v| enc::list(&vec![enc::bytes(&[p, v].concat()); 16])), Err(Error::ProofNodeInvalid));
    assert_eq!(forged_leaf(|p, v| enc::list(&vec![enc::bytes(&[p, v].concat()); 18])), Err(Error::ProofNodeInvalid));
    assert_eq!(forged_leaf(|p, v| enc::bytes(&[p, v].concat())), Err(Error::ProofNodeInvalid));
    // Non-canonical RLP, and bytes after the node.
    assert_eq!(
        forged_leaf(|p, v| {
            let mut n = enc::list(&[enc::bytes(p), enc::bytes(v)]);
            let payload = n.split_off(if n[0] >= 0xf8 { 2 } else { 1 });
            [vec![0xf9, 0x00, payload.len() as u8], payload].concat()
        }),
        Err(Error::RlpNonCanonical)
    );
    assert_eq!(
        forged_leaf(|p, v| [enc::list(&[enc::bytes(p), enc::bytes(v)]), vec![0x00]].concat()),
        Err(Error::RlpMalformed)
    );
}

/// A proof that ends in a branch: the second-to-last node of the sample.
fn forged_branch(f: impl Fn(Vec<Vec<u8>>) -> Vec<Vec<u8>>) -> Result<Option<Vec<u8>>, Error> {
    let (_, key, mut proof) = sample();
    let at = proof.len() - 2;
    let items: Vec<Vec<u8>> =
        ferrata_core::rlp::decode(&proof[at]).unwrap().list().unwrap().map(|i| i.unwrap().raw.to_vec()).collect();
    assert_eq!(items.len(), 17);
    let root = relink(&mut proof, at, enc::list(&f(items)));
    verify(&root, &key, &proof).map(|v| v.map(<[u8]>::to_vec))
}

#[test]
fn branch_rules_behind_the_hash_check() {
    assert!(matches!(forged_branch(|i| i), Ok(Some(_))));
    // An unused child that is not a valid reference still makes the node invalid.
    let (_, key, _) = sample();
    let nibble = |depth: usize| usize::from(if depth % 2 == 0 { key[depth / 2] >> 4 } else { key[depth / 2] & 0xf });
    let (_, _, proof) = sample();
    let depth = proof.len() - 2; // the branch's depth: one nibble per branch above it here
    let unused = (0..16).find(|n| *n != nibble(depth)).unwrap();
    for bad in [enc::bytes(&[7; 31]), enc::bytes(&[7; 33]), enc::bytes(&[7]), enc::list(&vec![enc::bytes(&[7; 4]); 8])]
    {
        let r = forged_branch(|mut items| {
            items[unused] = bad.clone();
            items
        });
        assert_eq!(r, Err(Error::ProofNodeInvalid), "child {}", hex::encode(&bad));
    }
    // The value slot must be a string.
    assert_eq!(
        forged_branch(|mut items| {
            items[16] = enc::list(&[]);
            items
        }),
        Err(Error::ProofNodeInvalid)
    );
}

#[test]
fn embedded_and_hashed_nodes_by_size() {
    // Two keys sharing 31 bytes with one-byte values: the branch at depth 62 holds two embedded
    // leaves and is itself small enough to be embedded in the extension above it.
    let k1 = [[0xab; 31].as_slice(), &[0x10]].concat();
    let k2 = [[0xab; 31].as_slice(), &[0x20]].concat();
    let trie = Trie::new([(k1.clone(), vec![0x01]), (k2.clone(), vec![0x02])]);
    let root = trie.root();
    let proof = trie.proof(&k1);
    assert_eq!(proof.len(), 1, "everything below the root is embedded");
    assert_eq!(verify(&root, &k1, &proof), Ok(Some(&[0x01][..])));
    assert_eq!(verify(&root, &k2, &trie.proof(&k2)), Ok(Some(&[0x02][..])));
    let k3 = [[0xab; 31].as_slice(), &[0x30]].concat();
    assert_eq!(verify(&root, &k3, &trie.proof(&k3)), Ok(None));
    // Listing the embedded node as a proof element as well is refused: it is not referenced by hash.
    let ext = ferrata_core::rlp::decode(&proof[0]).unwrap();
    let [_, child] = ext.items::<2>().unwrap();
    assert!(child.is_list && child.raw.len() < 32);
    assert_eq!(verify(&root, &k1, &[proof[0].clone(), child.raw.to_vec()]), Err(Error::ProofTrailingNodes));
    // A node shorter than 32 bytes referenced by hash (forged by hashing an embedded child).
    let hashed_child = enc::list(&[enc::bytes(&hex_prefix(&[0xa, 0xb], false)), enc::bytes(&keccak(child.raw))]);
    let forged_root = keccak(&hashed_child);
    let k = [[0xab; 1].as_slice(), &k1[1..]].concat();
    assert_eq!(verify(&forged_root, &k, &[hashed_child, child.raw.to_vec()]), Err(Error::ProofNodeInvalid));
    // The 32-byte boundary, both ways: a leaf of 31 bytes is embedded and one of 32 is hashed;
    // the other way round is refused.
    let leaf = |len: usize| enc::list(&[enc::bytes(&hex_prefix(&[1], true)), enc::bytes(&vec![9; len])]);
    let (leaf31, leaf32) = (leaf(28), leaf(29));
    assert_eq!((leaf31.len(), leaf32.len()), (31, 32));
    let embedding = |node: &[u8]| enc::list(&[enc::bytes(&hex_prefix(&[0xa], false)), node.to_vec()]);
    let hashing = |node: &[u8]| enc::list(&[enc::bytes(&hex_prefix(&[0xa], false)), enc::bytes(&keccak(node))]);
    let ok31 = embedding(&leaf31);
    assert_eq!(verify(&keccak(&ok31), &[0xa1], std::slice::from_ref(&ok31)), Ok(Some(&[9; 28][..])));
    let ok32 = hashing(&leaf32);
    assert_eq!(verify(&keccak(&ok32), &[0xa1], &[ok32.clone(), leaf32.clone()]), Ok(Some(&[9; 29][..])));
    let bad32 = embedding(&leaf32);
    assert_eq!(verify(&keccak(&bad32), &[0xa1], std::slice::from_ref(&bad32)), Err(Error::ProofNodeInvalid));
    let bad31 = hashing(&leaf31);
    assert_eq!(verify(&keccak(&bad31), &[0xa1], &[bad31.clone(), leaf31.clone()]), Err(Error::ProofNodeInvalid));
}

#[test]
fn extension_rules() {
    // An extension with an empty path, and one with no child.
    let child = Trie::new([(vec![0x10], vec![1; 40]), (vec![0x20], vec![2; 40])]);
    let branch = child.proof(&[0x10])[0].clone();
    let child_ref = enc::bytes(&keccak(&branch));
    let empty_path = enc::list(&[enc::bytes(&hex_prefix(&[], false)), child_ref.clone()]);
    assert_eq!(
        verify(&keccak(&empty_path), &[0x10], &[empty_path.clone(), branch.clone()]),
        Err(Error::ProofNodeInvalid)
    );
    let no_child = enc::list(&[enc::bytes(&hex_prefix(&[1], false)), enc::bytes(&[])]);
    assert_eq!(verify(&keccak(&no_child), &[0x10], &[no_child]), Err(Error::ProofNodeInvalid));
    // A genuine extension: a key that diverges inside the extension's path is absent, and a key
    // that ends inside it too.
    let trie = Trie::new([(vec![0xab, 0xc1], vec![1; 40]), (vec![0xab, 0xc2], vec![2; 40])]);
    let root = trie.root();
    assert_eq!(verify(&root, &[0xab, 0xc1], &trie.proof(&[0xab, 0xc1])), Ok(Some(&[1; 40][..])));
    assert_eq!(verify(&root, &[0xaa, 0xc1], &trie.proof(&[0xaa, 0xc1])), Ok(None));
    assert_eq!(verify(&root, &[0xab], &trie.proof(&[0xab])), Ok(None));
    // The extension's child is needed for a key that follows it: a proof without it is incomplete.
    assert_eq!(verify(&root, &[0xab, 0xc1], &trie.proof(&[0xab, 0xc1])[..1]), Err(Error::ProofIncomplete));
}

#[test]
fn empty_and_missing_proofs() {
    assert_eq!(verify::<Vec<u8>>(&EMPTY_ROOT, &[1; 32], &[]), Ok(None));
    assert_eq!(verify::<Vec<u8>>(&[7; 32], &[1; 32], &[]), Err(Error::ProofIncomplete));
    assert_eq!(keccak(&[0x80]), EMPTY_ROOT);
    // Anything after an empty-trie proof is unused.
    let (trie, key, proof) = sample();
    assert_eq!(verify(&EMPTY_ROOT, &key, &proof), Err(Error::ProofNodeMismatch));
    assert_eq!(verify(&trie.root(), &key, &proof), Ok(trie.get(&key)));
}
