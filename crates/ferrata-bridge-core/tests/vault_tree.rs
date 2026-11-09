//! The deposit tree, bit for bit against the Solidity vault: vectors/external/bridge-evm-tree.json is
//! the vault project's tree.json (8 deposits made on FerrataVault, leaves and roots read back from it).
//! Every value in it is recomputed here: tag, zero hashes, empty root, each leaf preimage and leaf,
//! the incremental root after each insertion, each Merkle path, and the final root.

use std::path::PathBuf;

use ferrata_bridge_core::encoding::{deposit_leaf_preimage, u256_from_u128, LEAF_TAG};
use ferrata_bridge_core::inbound::deposit_leaf;
use ferrata_bridge_core::tree::{verify_path, zero_hashes, IncrementalTree, Path, Tree, TREE_DEPTH};
use serde_json::Value;

fn bytes(v: &Value) -> Vec<u8> {
    let s = v.as_str().expect("hex string");
    hex::decode(s.strip_prefix("0x").expect("0x prefix")).expect("hex")
}

fn h32(v: &Value) -> [u8; 32] {
    bytes(v).try_into().expect("32 bytes")
}

#[test]
fn deposit_tree_matches_the_vault_bit_for_bit() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vectors/external/bridge-evm-tree.json");
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();

    assert_eq!(v["treeDepth"].as_u64(), Some(TREE_DEPTH as u64));
    assert_eq!(v["leafTagPreimage"].as_str(), Some("ferrata/vault/leaf/v1"));
    assert_eq!(h32(&v["leafTag"]), LEAF_TAG);
    let zero = zero_hashes();
    let file_zero: Vec<[u8; 32]> = v["zeroHashes"].as_array().unwrap().iter().map(h32).collect();
    assert_eq!(file_zero, zero.to_vec());
    assert_eq!(h32(&v["emptyRoot"]), zero[TREE_DEPTH]);
    assert_eq!(IncrementalTree::new().root(), zero[TREE_DEPTH]);

    let deposits = v["deposits"].as_array().unwrap();
    assert_eq!(deposits.len(), 8);
    let mut inc = IncrementalTree::new();
    let mut leaves = Vec::new();
    for (i, d) in deposits.iter().enumerate() {
        let leaf_index = d["leafIndex"].as_u64().unwrap();
        assert_eq!(leaf_index, i as u64);
        let token: [u8; 20] = bytes(&d["token"]).try_into().unwrap();
        let amount = u256_from_u128(d["amount"].as_str().unwrap().parse::<u128>().unwrap());
        let commitment = h32(&d["commitment"]);

        let preimage = deposit_leaf_preimage(leaf_index, &token, &amount, &commitment);
        assert_eq!(preimage.to_vec(), bytes(&d["leafPreimage"]), "leaf preimage {i}");
        let leaf = deposit_leaf(leaf_index, &token, &amount, &commitment);
        assert_eq!(leaf, h32(&d["leaf"]), "leaf {i}");

        assert_eq!(inc.append(leaf), Ok(leaf_index));
        assert_eq!(inc.root(), h32(&d["rootAfterInsert"]), "root after insert {i}");
        leaves.push(leaf);
    }

    let tree = Tree::from_leaves(leaves.clone()).unwrap();
    let final_root = h32(&v["finalRoot"]);
    assert_eq!(tree.root(), final_root);
    assert_eq!(inc.root(), final_root);
    for (i, d) in deposits.iter().enumerate() {
        let file_path: Path =
            d["pathToFinalRoot"].as_array().unwrap().iter().map(h32).collect::<Vec<_>>().try_into().unwrap();
        assert_eq!(tree.path(i as u64).unwrap(), file_path, "path {i}");
        assert_eq!(verify_path(&final_root, &leaves[i], i as u64, &file_path), Ok(()));
    }
}
