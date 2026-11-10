//! Accumulator-leaf facts: the shared vectors (`vectors/acc-leaf.json`, written by an
//! independent Python implementation), roots attested from storage under two records, the
//! identifier's independence from the root, and refusals with exact codes.

use std::{fs, path::PathBuf};

use ferrata_core::{
    accumulator::{
        accumulator_leaf_id, accumulator_node, accumulator_root_of, attest_accumulator_leaf, attest_accumulator_root,
        AccumulatorRoot, ACCUMULATOR_DEPTH,
    },
    attest::HeaderRecord,
    Error, Root,
};
use ferrata_spectests::execkit::{account_rlp, enc, keccak, record_ssz, w, Trie};
use serde_json::Value;

fn vectors() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vectors/acc-leaf.json");
    serde_json::from_slice(&fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))).unwrap()
}

fn h32(v: &Value) -> Root {
    hex::decode(v.as_str().unwrap().trim_start_matches("0x")).unwrap().try_into().unwrap()
}

fn path(v: &Value) -> Vec<Root> {
    v.as_array().unwrap().iter().map(h32).collect()
}

fn error(name: &str) -> Error {
    match name {
        "AccumulatorPathInvalid" => Error::AccumulatorPathInvalid,
        "AccumulatorIndexOutOfRange" => Error::AccumulatorIndexOutOfRange,
        "AccumulatorLeafEmpty" => Error::AccumulatorLeafEmpty,
        "AccumulatorRootMismatch" => Error::AccumulatorRootMismatch,
        other => panic!("unknown error {other}"),
    }
}

const ADDRESS: [u8; 20] = [0x11; 20];

#[test]
fn shared_vectors() {
    let v = vectors();
    assert_eq!(v["treeDepth"].as_u64(), Some(ACCUMULATOR_DEPTH as u64));
    // Zero hashes and the empty root. The empty root equals the bridge vault's `emptyRoot`.
    let zeros: Vec<Root> = path(&v["zeroHashes"]);
    for h in 0..ACCUMULATOR_DEPTH {
        assert_eq!(accumulator_node(&zeros[h], &zeros[h]), zeros[h + 1]);
    }
    assert_eq!(hex::encode(h32(&v["emptyRoot"])), "782d35b1fdad7d54e7a1b36a2ab1021e872c7692bb80fdd12bfc321e9e420409");
    let (chain_id, address, slot) = (v["chainId"].as_u64().unwrap(), ADDRESS, h32(&v["slot"]));
    assert_eq!(hex::decode(v["address"].as_str().unwrap().trim_start_matches("0x")).unwrap(), address);
    // Every leaf of both snapshots, and the same identifier under both roots.
    let mut ids = std::collections::BTreeMap::new();
    let mut proven = 0;
    for snap in v["snapshots"].as_array().unwrap() {
        let root = AccumulatorRoot::restore(chain_id, address, slot, h32(&snap["root"]), 100, [0; 32]);
        for e in snap["leaves"].as_array().unwrap() {
            let (index, leaf, p) = (e["leafIndex"].as_u64().unwrap(), h32(&e["leaf"]), path(&e["path"]));
            assert_eq!(accumulator_root_of(index, &leaf, &p), Ok(root.root()));
            let a = attest_accumulator_leaf(&root, index, &leaf, &p).unwrap();
            assert_eq!(a.id, h32(&e["id"]), "leaf {index}");
            assert_eq!((a.fact.leaf_index, a.fact.leaf, a.fact.root, a.chain_id), (index, leaf, root.root(), chain_id));
            if let Some(previous) = ids.insert(index, a.id) {
                assert_eq!(previous, a.id, "leaf {index}: one identifier under every root");
            }
            proven += 1;
        }
    }
    assert_eq!(proven, 14);
    // Refusals.
    for case in v["invalid"].as_array().unwrap() {
        let root = AccumulatorRoot::restore(chain_id, address, slot, h32(&case["root"]), 100, [0; 32]);
        let got = attest_accumulator_leaf(
            &root,
            case["leafIndex"].as_u64().unwrap(),
            &h32(&case["leaf"]),
            &path(&case["path"]),
        );
        assert_eq!(got.map(|a| a.id), Err(error(case["error"].as_str().unwrap())), "{}", case["case"]);
    }
    assert_eq!(v["invalid"].as_array().unwrap().len(), 7);
    // The identifier names chain, contract, slot, position and leaf.
    let variants = v["identifierVariants"].as_array().unwrap();
    for x in variants {
        let a: [u8; 20] =
            hex::decode(x["address"].as_str().unwrap().trim_start_matches("0x")).unwrap().try_into().unwrap();
        let id = accumulator_leaf_id(
            x["chainId"].as_u64().unwrap(),
            &a,
            &h32(&x["slot"]),
            x["leafIndex"].as_u64().unwrap(),
            &h32(&x["leaf"]),
        );
        assert_eq!(id, h32(&x["id"]));
    }
    let distinct: std::collections::BTreeSet<Root> = variants.iter().map(|x| h32(&x["id"])).collect();
    assert_eq!(distinct.len(), 4);
}

/// A state in which `address` keeps `root` in storage slot `slot`, a record of it, the account
/// proof, and the storage trie.
fn record_with(root: &Root, slot: &Root, number: u64) -> (HeaderRecord, Vec<Vec<u8>>, Trie) {
    let storage = Trie::new([(keccak(slot).to_vec(), enc::uint(root)), (keccak(&w(7)).to_vec(), enc::uint(&w(9)))]);
    let accounts: Vec<(Vec<u8>, Vec<u8>)> = (0..50u8)
        .map(|i| (keccak(&[i; 20]).to_vec(), account_rlp(1, 1, keccak(&[0x80]), keccak(b""))))
        .chain([(keccak(&ADDRESS).to_vec(), account_rlp(1, 0, storage.root(), keccak(b"vault code")))])
        .collect();
    let state = Trie::new(accounts);
    let record = HeaderRecord::from_ssz(&record_ssz(
        1,
        9_000_000 + number,
        number,
        0,
        keccak(&number.to_be_bytes()),
        state.root(),
        [0; 32],
        0,
        0,
    ))
    .unwrap();
    (record, state.proof(&keccak(&ADDRESS)), storage)
}

#[test]
fn a_leaf_under_two_attested_roots_has_one_identifier() {
    let v = vectors();
    let snaps = v["snapshots"].as_array().unwrap();
    let slot = [0u8; 32];
    let mut ids = Vec::new();
    for (snap, number) in snaps.iter().zip([1_000u64, 2_000]) {
        let root = h32(&snap["root"]);
        let (record, account_proof, storage) = record_with(&root, &slot, number);
        let storage_proof = storage.proof(&keccak(&slot));
        let attested = attest_accumulator_root(&record, 1, &ADDRESS, &account_proof, &slot, &storage_proof).unwrap();
        assert_eq!(
            (attested.root(), attested.block_number(), attested.address(), attested.slot()),
            (root, number, ADDRESS, slot)
        );
        let e = &snap["leaves"][3];
        let a = attest_accumulator_leaf(&attested, 3, &h32(&e["leaf"]), &path(&e["path"])).unwrap();
        assert_eq!(a.block_number, number);
        ids.push(a.id);
        // The other snapshot's path does not prove the leaf under this root.
        let other = if number == 1_000 { &snaps[1] } else { &snaps[0] };
        assert_eq!(
            attest_accumulator_leaf(&attested, 3, &h32(&e["leaf"]), &path(&other["leaves"][3]["path"])).map(|a| a.id),
            Err(Error::AccumulatorRootMismatch)
        );
        // Another chain's consumer.
        assert_eq!(
            attest_accumulator_root(&record, 5, &ADDRESS, &account_proof, &slot, &storage_proof),
            Err(Error::ChainMismatch)
        );
    }
    assert_eq!(ids[0], ids[1], "same leaf, two roots, two blocks: one identifier");
    // The same root kept in another slot is another accumulator: another identifier.
    let other_slot = w(1);
    let root = h32(&snaps[0]["root"]);
    let (record, ap, storage) = record_with(&root, &other_slot, 3_000);
    let attested =
        attest_accumulator_root(&record, 1, &ADDRESS, &ap, &other_slot, &storage.proof(&keccak(&other_slot))).unwrap();
    let e = &snaps[0]["leaves"][3];
    let a = attest_accumulator_leaf(&attested, 3, &h32(&e["leaf"]), &path(&e["path"])).unwrap();
    assert_ne!(a.id, ids[0]);
    // An empty slot attests the zero root, under which no leaf verifies.
    let empty_slot = w(2);
    let zero =
        attest_accumulator_root(&record, 1, &ADDRESS, &ap, &empty_slot, &storage.proof(&keccak(&empty_slot))).unwrap();
    assert_eq!(zero.root(), [0; 32]);
    assert_eq!(
        attest_accumulator_leaf(&zero, 3, &h32(&e["leaf"]), &path(&e["path"])).map(|a| a.id),
        Err(Error::AccumulatorRootMismatch)
    );
}

#[test]
fn every_sibling_and_every_index_bit_is_bound() {
    let v = vectors();
    let snap = &v["snapshots"][1];
    let root = AccumulatorRoot::restore(1, ADDRESS, [0; 32], h32(&snap["root"]), 1, [0; 32]);
    let e = &snap["leaves"][6];
    let (leaf, p) = (h32(&e["leaf"]), path(&e["path"]));
    assert!(attest_accumulator_leaf(&root, 6, &leaf, &p).is_ok());
    for height in 0..ACCUMULATOR_DEPTH {
        let mut q = p.clone();
        q[height][31] ^= 1;
        assert_eq!(
            attest_accumulator_leaf(&root, 6, &leaf, &q).map(|a| a.id),
            Err(Error::AccumulatorRootMismatch),
            "sibling {height}"
        );
        let other = 6 ^ (1u64 << height);
        assert_eq!(
            attest_accumulator_leaf(&root, other, &leaf, &p).map(|a| a.id),
            Err(Error::AccumulatorRootMismatch),
            "index bit {height}"
        );
    }
    for high in [32u32, 33, 40, 63] {
        assert_eq!(
            attest_accumulator_leaf(&root, 6 | (1u64 << high), &leaf, &p).map(|a| a.id),
            Err(Error::AccumulatorIndexOutOfRange),
            "bit {high}"
        );
    }
    assert_eq!(
        attest_accumulator_leaf(&root, u32::MAX as u64 + 1, &leaf, &p).map(|a| a.id),
        Err(Error::AccumulatorIndexOutOfRange)
    );
    let mut tampered = leaf;
    tampered[0] ^= 1;
    assert_eq!(attest_accumulator_leaf(&root, 6, &tampered, &p).map(|a| a.id), Err(Error::AccumulatorRootMismatch));
    assert_eq!(attest_accumulator_leaf(&root, 6, &leaf, &[]).map(|a| a.id), Err(Error::AccumulatorPathInvalid));
}
