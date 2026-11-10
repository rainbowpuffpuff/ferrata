//! The encodings this crate shares with `spike/ferrata-bridge` (an independent implementation of
//! the same SPEC-v2 encodings): `configId`, the burn leaf, frontier and root, `keySetHash`, `S`, and
//! words 1-12 of the checkpoint journal (their vectors are SPEC-v2's 13-word journal; SPEC-v3 keeps
//! those words), checked against that project's vectors (copied to
//! `vectors/external/ferrata-bridge/`, provenance in `PROVENANCE.txt`).

use checkpoint_core::journal::CheckpointJournal;
use checkpoint_core::state::{key_set_hash, state_digest, Config};
use checkpoint_core::tree::{burn_leaf, zero_hashes, Frontier};
use checkpoint_core::Hash;
use serde_json::Value;

fn load(name: &str) -> Value {
    let path = format!("{}/../../vectors/external/ferrata-bridge/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(&path).expect(&path)).unwrap()
}

fn bytes(v: &Value) -> Vec<u8> {
    hex::decode(v.as_str().unwrap().trim_start_matches("0x")).unwrap()
}

fn h32(v: &Value) -> Hash {
    bytes(v).try_into().unwrap()
}

fn num(v: &Value) -> u64 {
    v.as_u64().unwrap_or_else(|| v.as_str().unwrap().parse().unwrap())
}

#[test]
fn config_ids() {
    let v = load("keys-config-v2.json");
    let configs = v["configs"].as_array().unwrap();
    assert!(!configs.is_empty());
    for c in configs {
        let config = Config {
            bedrock_genesis_id: h32(&c["bedrock_genesis_id"]),
            channel_id: h32(&c["channel_id"]),
            bridge_header: h32(&c["bridge_header"]),
            bridge_image_id: h32(&c["bridge_image_id"]),
            evm_chain_id: h32(&c["evm_chain_id"]),
            vault: bytes(&c["vault"]).try_into().unwrap(),
            attestation_root_id: h32(&c["attestation_root_id"]),
            salt: h32(&c["salt"]),
            n_min: 0,
            t_min: 0,
        };
        assert_eq!(config.config_id(), h32(&c["config_id"]));
    }
}

#[test]
fn burn_tree() {
    let v = load("burn-tree-v2.json");
    let config_id = h32(&v["config_id"]);
    let zeros = zero_hashes();
    let mut f = Frontier::default();
    for l in v["leaves"].as_array().unwrap() {
        let leaf = burn_leaf(
            &config_id,
            num(&l["burn_index"]),
            &bytes(&l["token"]).try_into().unwrap(),
            &h32(&l["amount"]),
            &h32(&l["C_out"]),
            &h32(&l["r"]),
        );
        assert_eq!(leaf, h32(&l["leaf"]));
        f.append(leaf).unwrap();
        let want: Vec<Hash> = l["frontier_after"].as_array().unwrap().iter().map(h32).collect();
        assert_eq!(f.nodes.to_vec(), want);
        assert_eq!(f.hash(), h32(&l["frontier_hash_after"]));
        assert_eq!(f.root(&zeros), h32(&l["root_after"]));
    }
    assert_eq!(f.root(&zeros), h32(&v["final_root"]));
}

#[test]
fn key_sets_states_journals() {
    let v = load("checkpoint-v2.json");
    for k in v["keysets"].as_array().unwrap() {
        let keys: Vec<Hash> = k["keys"].as_array().unwrap().iter().map(h32).collect();
        assert_eq!(key_set_hash(&keys), h32(&k["key_set_hash"]));
    }
    for s in v["states"].as_array().unwrap() {
        let digest = state_digest(
            num(&s["tip_id"]),
            &h32(&s["tip_hash"]),
            &h32(&s["key_set_hash"]),
            &h32(&s["config_tip_hash"]),
            num(&s["burn_count"]),
            &h32(&s["burn_frontier_hash"]),
            num(&s["imported_leaf_count"]),
            s["sunset"].as_bool().unwrap(),
        );
        assert_eq!(digest, h32(&s["S"]));
    }
    for j in v["journals"].as_array().unwrap() {
        let journal = CheckpointJournal {
            evm_chain_id: h32(&j["chainId"]),
            vault: bytes(&j["vault"]).try_into().unwrap(),
            lez_config_id: h32(&j["lezConfigId"]),
            n_min: u32::try_from(num(&j["nMin"])).unwrap(),
            t_min: u32::try_from(num(&j["tMin"])).unwrap(),
            parent_state: h32(&j["parentState"]),
            new_state: h32(&j["newState"]),
            burn_root: h32(&j["burnRoot"]),
            burn_count: num(&j["burnCount"]),
            imported_leaf_count: num(&j["importedLeafCount"]),
            sunset: j["sunset"].as_bool().unwrap(),
            echo_nonce: h32(&j["echoNonce"]),
            tip_id: 0,
            tip_hash: [0; 32],
            n: 0,
            t: 0,
        };
        // Their vectors are SPEC-v2 journals (13 words, v2 domain); SPEC-v3 keeps words 1-12 and
        // changes the domain and appends four words. Compare the shared words.
        let v2 = bytes(&j["journal"]);
        assert_eq!(v2.len(), 13 * 32);
        assert_eq!(&journal.encode()[32..13 * 32], &v2[32..]);
    }
}
