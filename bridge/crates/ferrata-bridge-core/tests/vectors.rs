//! Known-answer vectors for SPEC-v2 in `vectors/*-v2.json` at the repository root.
//!
//! `FERRATA_WRITE_VECTORS=1 cargo test --test vectors` rewrites them; otherwise this test fails if
//! any file differs from what the code computes. Every input is in the files next to every output,
//! so `scripts/check_vectors_v2.py` (Python standard library only) recomputes them all.

mod common;

use std::path::PathBuf;

use common::{addr, h};
use ferrata_bridge_core::checkpoint::{burn_frontier_hash, key_set_hash, LezState};
use ferrata_bridge_core::encoding::*;
use ferrata_bridge_core::inbound::{self, ClaimWitness, DestKind};
use ferrata_bridge_core::outbound::{self, BurnNote, ReleasePublic, ReleaseTarget, ReleaseWitness};
use ferrata_bridge_core::tree::{zero_hashes, IncrementalTree, MerklePath, Tree};
use ferrata_bridge_core::{chain_id_u256, derive_keys, sha256, BridgeKeys, Hash};
use serde_json::{json, Value};

fn hx(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

fn path_json(p: &MerklePath) -> Value {
    json!({"directions": p.directions, "siblings": p.siblings.iter().map(|s| hx(s)).collect::<Vec<_>>()})
}

fn write_or_check(name: &str, value: Value) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vectors").join(name);
    let text = serde_json::to_string_pretty(&value).unwrap() + "\n";
    if std::env::var("FERRATA_WRITE_VECTORS").as_deref() == Ok("1") {
        std::fs::write(&path, text).unwrap();
    } else {
        let on_disk = std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("{} is missing; run with FERRATA_WRITE_VECTORS=1", path.display()));
        assert!(on_disk == text, "{name} differs from what the code computes");
    }
}

fn config_identity(n: usize) -> ConfigIdentity {
    ConfigIdentity {
        bedrock_genesis_id: h(&format!("vectors: bedrock genesis {n}")),
        channel_id: h(&format!("vectors: channel {n}")),
        bridge_header: h(&format!("vectors: bridge header {n}")),
        bridge_image_id: h(&format!("vectors: bridge image {n}")),
        evm_chain_id: chain_id_u256([1, 11_155_111, 31_337][n % 3]),
        vault: addr(&format!("vectors: vault {n}")),
        attestation_root_id: h(&format!("vectors: attestation root {n}")),
        salt: h(&format!("vectors: salt {n}")),
    }
}

#[test]
fn tags() {
    let hashed: Vec<Value> = [LEAF_TAG_STR, BURN_TAG_STR, RELEASE_DOMAIN_STR, CHECKPOINT_DOMAIN_STR]
        .iter()
        .map(|t| json!({"tag": t, "sha256": hx(&sha256(t.as_bytes()))}))
        .collect();
    let prefix: Vec<Value> = PREFIX_TAGS
        .iter()
        .map(|t| json!({"tag": std::str::from_utf8(t).unwrap(), "bytes": hx(t), "length": t.len()}))
        .collect();
    write_or_check(
        "tags-v2.json",
        json!({
            "description": "SPEC-v2 tags. 'hashed' tags enter preimages as the SHA-256 of the ASCII string; 'prefix' tags as the ASCII bytes. Leaf prefixes: deposit leaf 0x00, node 0x01, burn leaf 0x02. ferrata/burn-frontier/v1 is this crate's choice (SPEC-v2 does not define burnFrontierHash's encoding).",
            "hashed": hashed, "prefix": prefix,
            "prefixes": {"deposit_leaf": "0x00", "node": "0x01", "burn_leaf": "0x02"},
        }),
    );
}

#[test]
fn keys_and_config() {
    let keys: Vec<Value> = (0..3)
        .map(|n| {
            let seed = h(&format!("vectors: bridge seed {n}"));
            let (k_auth, k_view) = derive_keys(&seed);
            json!({"bridge_seed": hx(&seed), "k_auth": hx(&k_auth), "k_view": hx(&k_view)})
        })
        .collect();
    let configs: Vec<Value> = (0..3)
        .map(|n| {
            let c = config_identity(n);
            let pre = config_id_preimage(&c);
            json!({
                "bedrock_genesis_id": hx(&c.bedrock_genesis_id), "channel_id": hx(&c.channel_id),
                "bridge_header": hx(&c.bridge_header), "bridge_image_id": hx(&c.bridge_image_id),
                "evm_chain_id": hx(&c.evm_chain_id), "vault": hx(&c.vault), "attestation_root_id": hx(&c.attestation_root_id),
                "salt": hx(&c.salt), "preimage": hx(&pre), "config_id": hx(&sha256(&pre)),
            })
        })
        .collect();
    write_or_check(
        "keys-config-v2.json",
        json!({
            "description": "kAuth = HMAC-SHA256(bridge_seed, 'ferrata/auth/v1'), kView = HMAC-SHA256(bridge_seed, 'ferrata/view/v1'); configId = sha256(preimage), preimage as in encoding.rs (261 bytes, evmChainId as uint256).",
            "keys": keys, "configs": configs,
        }),
    );
}

#[test]
fn deposit_tree() {
    let tokens = [[0u8; 20], addr("vectors: USDC"), addr("vectors: DAI"), addr("vectors: USDT")];
    let mut inc = IncrementalTree::new();
    let (mut leaves, mut entries) = (Vec::new(), Vec::new());
    for i in 0..8u64 {
        let (depositor, token) = (addr(&format!("vectors: depositor {}", i % 3)), tokens[(i % 4) as usize]);
        let amount = u256_from_u128(10u128.pow(6 + (i % 3) as u32 * 6));
        let c = h(&format!("vectors: deposit commitment {i}"));
        let pre = deposit_leaf_preimage(i, &depositor, &token, &amount, &c);
        let leaf = inbound::deposit_leaf(i, &depositor, &token, &amount, &c);
        assert_eq!(sha256(&pre), leaf);
        inc.append(leaf).unwrap();
        leaves.push(leaf);
        entries.push(json!({
            "leaf_index": i, "depositor": hx(&depositor), "token": hx(&token), "amount": hx(&amount), "commitment": hx(&c),
            "preimage": hx(&pre), "leaf": hx(&leaf), "root_after": hx(&inc.root()),
        }));
    }
    let tree = Tree::from_leaves(leaves).unwrap();
    assert_eq!(tree.root(), inc.root());
    write_or_check(
        "deposit-tree-v2.json",
        json!({
            "description": "Deposit tree v2: leaf = sha256(0x00 || sha256('ferrata/vault/leaf/v2') || leafIndex:u64 || depositor:20 || token:20 || amount:u256 || C), node = sha256(0x01 || l || r), empty leaf 0, depth 32. Paths: direction bits (= leaf index) and siblings, leaf level first, in the final 8-leaf tree.",
            "depth": 32,
            "zero_hashes": zero_hashes().iter().map(|z| hx(z)).collect::<Vec<_>>(),
            "leaves": entries, "final_root": hx(&tree.root()),
            "paths": (0..8u64).map(|i| path_json(&tree.path(i).unwrap())).collect::<Vec<_>>(),
        }),
    );
}

#[test]
fn burn_tree_and_checkpoint() {
    let config_id = sha256(&config_id_preimage(&config_identity(0)));
    let tokens = [[0u8; 20], addr("vectors: USDC")];
    let mut inc = IncrementalTree::new();
    let (mut leaves, mut entries) = (Vec::new(), Vec::new());
    for i in 0..8u64 {
        let token = tokens[(i % 2) as usize];
        let amount = u256_from_u128(10u128.pow(8 + (i % 3) as u32));
        let (c_out, r) = (h(&format!("vectors: COut {i}")), h(&format!("vectors: r {i}")));
        let pre = burn_leaf_preimage(&config_id, i, &token, &amount, &c_out, &r);
        let leaf = outbound::burn_leaf(&config_id, i, &token, &amount, &c_out, &r);
        assert_eq!(sha256(&pre), leaf);
        inc.append(leaf).unwrap();
        leaves.push(leaf);
        entries.push(json!({
            "burn_index": i, "token": hx(&token), "amount": hx(&amount), "C_out": hx(&c_out), "r": hx(&r),
            "preimage": hx(&pre), "leaf": hx(&leaf), "root_after": hx(&inc.root()),
            "frontier_after": inc.frontier().iter().map(|f| hx(f)).collect::<Vec<_>>(),
            "frontier_hash_after": hx(&burn_frontier_hash(&inc)),
        }));
    }
    let tree = Tree::from_leaves(leaves).unwrap();
    assert_eq!(tree.root(), inc.root());
    write_or_check(
        "burn-tree-v2.json",
        json!({
            "description": "Burn tree v2: leaf = sha256(0x02 || sha256('ferrata/burn/leaf/v2') || configId || burnIndex:u64 || token:20 || amount:u256 || COut || r); same node rule and depth as the deposit tree. frontier_after: the canonical frontier after that append (entry h = completed left-edge subtree of height h if bit h of the count is 1, else zero); frontier_hash_after = sha256('ferrata/burn-frontier/v1' || f_0 || ... || f_31) (this crate's encoding; SPEC-v2 leaves it open).",
            "config_id": hx(&config_id),
            "leaves": entries, "final_root": hx(&tree.root()),
            "paths": (0..8u64).map(|i| path_json(&tree.path(i).unwrap())).collect::<Vec<_>>(),
        }),
    );

    // Checkpoint encodings.
    let keysets: Vec<Value> = [0usize, 1, 4]
        .iter()
        .map(|&n| {
            let keys: Vec<Hash> = (0..n).map(|i| h(&format!("vectors: sequencer key {i}"))).collect();
            json!({"keys": keys.iter().map(|k| hx(k)).collect::<Vec<_>>(), "key_set_hash": hx(&key_set_hash(&keys))})
        })
        .collect();
    let states: Vec<LezState> = vec![
        LezState {
            tip_id: 0,
            tip_hash: h("vectors: genesis tip"),
            key_set_hash: key_set_hash(&(0..4).map(|i| h(&format!("vectors: sequencer key {i}"))).collect::<Vec<_>>()),
            config_tip_hash: h("vectors: config tip 0"),
            burn_count: 0,
            burn_frontier_hash: burn_frontier_hash(&IncrementalTree::new()),
            imported_leaf_count: 0,
            sunset: false,
        },
        LezState {
            tip_id: 4242,
            tip_hash: h("vectors: tip 4242"),
            key_set_hash: key_set_hash(&(0..4).map(|i| h(&format!("vectors: sequencer key {i}"))).collect::<Vec<_>>()),
            config_tip_hash: h("vectors: config tip 1"),
            burn_count: inc.len(),
            burn_frontier_hash: burn_frontier_hash(&inc),
            imported_leaf_count: 6,
            sunset: true,
        },
    ];
    let states_json: Vec<Value> = states
        .iter()
        .map(|s| {
            json!({
                "tip_id": s.tip_id, "tip_hash": hx(&s.tip_hash), "key_set_hash": hx(&s.key_set_hash),
                "config_tip_hash": hx(&s.config_tip_hash), "burn_count": s.burn_count,
                "burn_frontier_hash": hx(&s.burn_frontier_hash), "imported_leaf_count": s.imported_leaf_count,
                "sunset": s.sunset, "preimage": hx(&lez_state_preimage(s)), "S": hx(&s.hash()),
            })
        })
        .collect();
    let journals = [
        CheckpointJournal {
            chain_id: chain_id_u256(1),
            vault: addr("vectors: vault 0"),
            lez_config_id: config_id,
            n_min: 4,
            t_min: 3,
            parent_state: states[0].hash(),
            new_state: states[1].hash(),
            burn_root: tree.root(),
            burn_count: 8,
            imported_leaf_count: 6,
            sunset: true,
            echo_nonce: [0u8; 32],
        },
        CheckpointJournal {
            chain_id: chain_id_u256(31_337),
            vault: addr("vectors: vault 1"),
            lez_config_id: h("vectors: another config"),
            n_min: 1,
            t_min: 1,
            parent_state: h("vectors: parent"),
            new_state: h("vectors: new"),
            burn_root: h("vectors: burn root"),
            burn_count: u64::MAX,
            imported_leaf_count: 0,
            sunset: false,
            echo_nonce: h("vectors: beacon nonce"),
        },
        CheckpointJournal {
            chain_id: [0xff; 32],
            vault: [0xff; 20],
            lez_config_id: [0xff; 32],
            n_min: u32::MAX,
            t_min: u32::MAX,
            parent_state: [0xff; 32],
            new_state: [0xff; 32],
            burn_root: [0xff; 32],
            burn_count: u64::MAX,
            imported_leaf_count: u64::MAX,
            sunset: true,
            echo_nonce: [0xff; 32],
        },
    ];
    let journals_json: Vec<Value> = journals
        .iter()
        .map(|j| {
            let bytes = j.encode();
            json!({
                "chainId": hx(&j.chain_id), "vault": hx(&j.vault), "lezConfigId": hx(&j.lez_config_id), "nMin": j.n_min,
                "tMin": j.t_min, "parentState": hx(&j.parent_state), "newState": hx(&j.new_state), "burnRoot": hx(&j.burn_root),
                "burnCount": j.burn_count.to_string(), "importedLeafCount": j.imported_leaf_count.to_string(), "sunset": j.sunset,
                "echoNonce": hx(&j.echo_nonce), "journal": hx(&bytes), "digest": hx(&sha256(&bytes)),
            })
        })
        .collect();
    write_or_check(
        "checkpoint-v2.json",
        json!({
            "description": "SPEC-v2 §4.1-§4.2. keySetHash = sha256('ferrata/keyset/v1' || n:u32 || keys); S = sha256(preimage), 173-byte preimage as in encoding.rs; checkpoint journal = abi.encode(CHECKPOINT_DOMAIN, evmChainId, vault, lezConfigId, nMin, tMin, parentState, newState, burnRoot, burnCount, importedLeafCount, sunset, echoNonce), 416 bytes. State 1 is the burn tree of burn-tree-v2.json after its 8 leaves. burnCount and importedLeafCount are decimal strings (u64).",
            "keysets": keysets, "states": states_json, "count": journals_json.len(), "journals": journals_json,
        }),
    );
}

#[test]
fn inbound_derivations() {
    let mut cases = Vec::new();
    let params = [
        (1u64, 0u64, 0u64, DestKind::Private),
        (11_155_111, 3, 41, DestKind::Public),
        (31_337, u64::MAX, (1 << 32) - 1, DestKind::Private),
    ];
    for (n, (chain, j, leaf_index, kind)) in params.into_iter().enumerate() {
        let seed = h(&format!("vectors: inbound seed {n}"));
        let (k_auth, _) = derive_keys(&seed);
        let keys = BridgeKeys::from_bridge_seed(&seed);
        let chain_id = chain_id_u256(chain);
        let (vault, depositor, dest) = (
            addr(&format!("vectors: vault {n}")),
            addr(&format!("vectors: depositor {n}")),
            h(&format!("vectors: dest {n}")),
        );
        let (token, amount) = (addr(&format!("vectors: token {n}")), u256_from_u128(1_000_000 * (n as u128 + 1)));
        let s = inbound::deposit_secret(&keys.auth, &chain_id, &vault, &depositor, j);
        let c = inbound::commitment(&s, kind, &dest);
        let leaf = inbound::deposit_leaf(leaf_index, &depositor, &token, &amount, &c);
        let fid = inbound::deposit_id(&chain_id, &vault, leaf_index, &leaf);
        cases.push(json!({
            "bridge_seed": hx(&seed), "k_auth": hx(&k_auth), "evm_chain_id": hx(&chain_id), "vault": hx(&vault),
            "depositor": hx(&depositor), "j": j.to_string(), "kind": kind as u8, "dest": hx(&dest),
            "s_message": hx(&deposit_secret_message(&chain_id, &vault, &depositor, j)), "s": hx(&s),
            "C_preimage": hx(&commitment_preimage(&s, kind, &dest)), "C": hx(&c),
            "leaf_index": leaf_index, "token": hx(&token), "amount": hx(&amount), "leaf": hx(&leaf),
            "fid_preimage": hx(&deposit_id_preimage(&chain_id, &vault, leaf_index, &leaf)), "fid": hx(&fid),
            "N_preimage": hx(&nullifier_preimage(&s, &fid)), "N": hx(&inbound::nullifier(&s, &fid)),
        }));
    }
    // One complete claim: our deposit at leaf 2 of a 5-leaf tree, public destination.
    let seed = h("vectors: claim seed");
    let keys = BridgeKeys::from_bridge_seed(&seed);
    let (chain_id, vault, depositor, dest) =
        (chain_id_u256(1), addr("vectors: claim vault"), addr("vectors: claim depositor"), h("vectors: claim dest"));
    let (token, amount) = ([0u8; 20], u256_from_u128(1_000_000_000_000_000_000));
    let s = inbound::deposit_secret(&keys.auth, &chain_id, &vault, &depositor, 0);
    let mut tree = Tree::new();
    for i in 0..5u64 {
        let c =
            if i == 2 { inbound::commitment(&s, DestKind::Public, &dest) } else { h(&format!("vectors: other C {i}")) };
        tree.append(inbound::deposit_leaf(i, &depositor, &token, &amount, &c)).unwrap();
    }
    let w = ClaimWitness {
        depositor,
        token,
        amount,
        leaf_index: 2,
        path: tree.path(2).unwrap(),
        s,
        kind: DestKind::Public,
        dest,
    };
    let n_claim = inbound::verify_claim(&tree.root(), &w, &chain_id, &vault).unwrap();
    let claim_input = ClaimInput { root: tree.root(), chain_id, vault, witness: w.clone() };
    let claim = json!({
        "bridge_seed": hx(&seed), "evm_chain_id": hx(&chain_id), "vault": hx(&vault), "depositor": hx(&depositor), "j": "0",
        "kind": 2, "dest": hx(&dest), "s": hx(&s), "token": hx(&token), "amount": hx(&amount), "leaf_index": 2,
        "leaves": tree.leaves().iter().map(|l| hx(l)).collect::<Vec<_>>(), "root": hx(&tree.root()),
        "path": path_json(&w.path), "N": hx(&n_claim),
        "guest_input": hx(&claim_input.encode()),
        "guest_journal": hx(&claim_journal(&tree.root(), &chain_id, &vault, &n_claim, &token, &amount)),
    });
    write_or_check(
        "inbound-v2.json",
        json!({
            "description": "Deposit -> mint (SPEC-v2 §3). s = HMAC-SHA256(kAuth, s_message); C = sha256(C_preimage) with kind 0x01 private / 0x02 public; leaf = deposit leaf v2 (binds depositor); fid = sha256(fid_preimage) (accumulator-leaf identifier, slot 0, root-independent); N = sha256(N_preimage). 'claim': a complete claim under a 5-leaf tree, with the claim measurement guest's input and journal. j is a decimal string (u64).",
            "cases": cases, "claim": claim,
        }),
    );
}

#[test]
fn outbound_derivations() {
    let mut cases = Vec::new();
    let params = [(1u64, 0u64, 0u64), (11_155_111, 7, 1234), (31_337, u64::MAX, (1 << 32) - 1)];
    for (n, (chain, i, burn_index)) in params.into_iter().enumerate() {
        let seed = h(&format!("vectors: outbound seed {n}"));
        let (k_auth, k_view) = derive_keys(&seed);
        let keys = BridgeKeys::from_bridge_seed(&seed);
        let config_id = sha256(&config_id_preimage(&config_identity(n)));
        let chain_id = chain_id_u256(chain);
        let (vault, recipient) = (addr(&format!("vectors: vault {n}")), addr(&format!("vectors: recipient {n}")));
        let (token, amount) =
            (addr(&format!("vectors: token {n}")), u256_from_u128(5 * 10u128.pow(17) * (n as u128 + 1)));
        let note = BurnNote::create(&keys, &config_id, i, &ReleaseTarget { chain_id, vault, recipient }, token, amount);
        let s_out = outbound::burn_secret(&keys.auth, &config_id, &note.r);
        let leaf = outbound::burn_leaf(&config_id, burn_index, &token, &amount, &note.c_out, &note.r);
        let bid = outbound::burn_id(&config_id, burn_index, &leaf);
        cases.push(json!({
            "bridge_seed": hx(&seed), "k_auth": hx(&k_auth), "k_view": hx(&k_view), "config_id": hx(&config_id), "i": i.to_string(),
            "r_message": hx(&burn_tag_message(&config_id, i)), "r": hx(&note.r),
            "s_out_message": hx(&burn_secret_message(&config_id, &note.r)), "s_out": hx(&s_out),
            "memo_message": hx(&memo_pad_message(&note.r)), "memo_pad": hx(&outbound::memo_pad(&keys.view, &note.r)),
            "recipient": hx(&recipient), "memo": hx(&note.memo), "evm_chain_id": hx(&chain_id), "vault": hx(&vault),
            "C_out_preimage": hx(&burn_commitment_preimage(&s_out, &chain_id, &vault, &recipient)), "C_out": hx(&note.c_out),
            "token": hx(&token), "amount": hx(&amount),
            "mac_message": hx(&burn_mac_message(&note.r, &note.memo, &token, &amount, &note.c_out)), "mac": hx(&note.mac),
            "burn_index": burn_index, "burn_leaf_preimage": hx(&burn_leaf_preimage(&config_id, burn_index, &token, &amount, &note.c_out, &note.r)),
            "burn_leaf": hx(&leaf), "burn_id_preimage": hx(&burn_id_preimage(&config_id, burn_index, &leaf)), "burn_id": hx(&bid),
            "N_out_preimage": hx(&burn_nullifier_preimage(&s_out)), "N_out": hx(&outbound::burn_nullifier(&s_out)),
        }));
    }
    // One complete release: our burn at index 3 of a 6-leaf burn tree, with a relayer and fee.
    let seed = h("vectors: release seed");
    let keys = BridgeKeys::from_bridge_seed(&seed);
    let config_id = sha256(&config_id_preimage(&config_identity(2)));
    let target = ReleaseTarget {
        chain_id: chain_id_u256(31_337),
        vault: addr("vectors: release vault"),
        recipient: addr("vectors: release recipient"),
    };
    let (token, amount) = (addr("vectors: release token"), u256_from_u128(100_000_000));
    let note = BurnNote::create(&keys, &config_id, 0, &target, token, amount);
    let mut tree = Tree::new();
    for i in 0..6u64 {
        let (c, r) = if i == 3 {
            (note.c_out, note.r)
        } else {
            (h(&format!("vectors: other COut {i}")), h(&format!("vectors: other r {i}")))
        };
        tree.append(outbound::burn_leaf(&config_id, i, &token, &amount, &c, &r)).unwrap();
    }
    let public = ReleasePublic {
        chain_id: target.chain_id,
        vault: target.vault,
        config_id,
        token,
        amount,
        recipient: target.recipient,
        relayer: addr("vectors: relayer"),
        relay_fee: u256_from_u128(250_000),
    };
    let w = ReleaseWitness {
        burn_index: 3,
        path: tree.path(3).unwrap(),
        s_out: outbound::burn_secret(&keys.auth, &config_id, &note.r),
        r: note.r,
    };
    let n_out = outbound::verify_release(&tree.root(), &w, &public).unwrap();
    let journal = outbound::release_journal(&tree.root(), &n_out, &public);
    let release = json!({
        "bridge_seed": hx(&seed), "config_id": hx(&config_id), "i": "0", "r": hx(&note.r), "memo": hx(&note.memo), "mac": hx(&note.mac),
        "evm_chain_id": hx(&public.chain_id), "vault": hx(&public.vault), "recipient": hx(&public.recipient),
        "token": hx(&token), "amount": hx(&amount), "C_out": hx(&note.c_out), "relayer": hx(&public.relayer), "relay_fee": hx(&public.relay_fee),
        "burn_index": 3, "leaves": tree.leaves().iter().map(|l| hx(l)).collect::<Vec<_>>(), "burn_root": hx(&tree.root()),
        "path": path_json(&w.path), "s_out": hx(&w.s_out), "N_out": hx(&n_out),
        "journal": hx(&journal), "journal_sha256": hx(&sha256(&journal)),
        "guest_input": hx(&ReleaseInput { burn_root: tree.root(), witness: w.clone(), public: public.clone() }.encode()),
    });
    write_or_check(
        "outbound-v2.json",
        json!({
            "description": "Burn -> release (SPEC-v2 §5). r = HMAC(kView, r_message); sOut = HMAC(kAuth, s_out_message); memo_pad = HMAC(kView, memo_message)[0..20]; memo = recipient XOR memo_pad; COut = sha256(C_out_preimage); mac = HMAC(kView, mac_message)[0..16] with D12's mac_message ('ferrata/burn-mac/v2' || r || memo || token || amount || COut); burn_leaf = burn leaf v2; burn_id = sha256(burn_id_preimage) (SPEC-v2's, no longer in N_out); N_out = sha256(N_out_preimage) with D13's preimage ('ferrata/nf-out/v2' || sOut), which does not depend on the burn index. 'release': a complete release under a 6-leaf burn tree, with the vault journal and the release guest's 1340-byte input. i is a decimal string (u64).",
            "cases": cases, "release": release,
        }),
    );
}

#[test]
fn release_journals() {
    let ff20 = [0xffu8; 20];
    let ff32 = [0xffu8; 32];
    let journals = [
        ReleaseJournal {
            chain_id: chain_id_u256(1),
            vault: addr("journal: vault 1"),
            lez_config_id: h("journal: config 1"),
            burn_root: h("journal: root 1"),
            nullifier: h("journal: nf 1"),
            token: [0u8; 20],
            amount: u256_from_u128(100_000_000_000_000_000),
            recipient: addr("journal: recipient 1"),
            relayer: [0u8; 20],
            relay_fee: [0u8; 32],
        },
        ReleaseJournal {
            chain_id: chain_id_u256(11_155_111),
            vault: addr("journal: vault 2"),
            lez_config_id: h("journal: config 2"),
            burn_root: h("journal: root 2"),
            nullifier: h("journal: nf 2"),
            token: addr("journal: USDC"),
            amount: u256_from_u128(100_000_000),
            recipient: addr("journal: recipient 2"),
            relayer: addr("journal: relayer"),
            relay_fee: u256_from_u128(250_000),
        },
        ReleaseJournal {
            chain_id: chain_id_u256(31_337),
            vault: addr("journal: vault 3"),
            lez_config_id: h("journal: config 3"),
            burn_root: h("journal: root 3"),
            nullifier: h("journal: nf 3"),
            token: addr("journal: DAI"),
            amount: u256_from_u128(u128::MAX),
            recipient: addr("journal: recipient 3"),
            relayer: addr("journal: relayer 3"),
            relay_fee: u256_from_u128(u128::MAX - 1),
        },
        ReleaseJournal {
            chain_id: ff32,
            vault: ff20,
            lez_config_id: ff32,
            burn_root: ff32,
            nullifier: ff32,
            token: ff20,
            amount: ff32,
            recipient: ff20,
            relayer: ff20,
            relay_fee: ff32,
        },
        ReleaseJournal {
            chain_id: [0u8; 32],
            vault: [0u8; 20],
            lez_config_id: [0u8; 32],
            burn_root: [0u8; 32],
            nullifier: [0u8; 32],
            token: [0u8; 20],
            amount: [0u8; 32],
            recipient: [0u8; 20],
            relayer: [0u8; 20],
            relay_fee: [0u8; 32],
        },
        ReleaseJournal {
            chain_id: chain_id_u256(17_000),
            vault: addr("journal: vault 6"),
            lez_config_id: h("journal: config 6"),
            burn_root: h("journal: root 6"),
            nullifier: h("journal: nf 6"),
            token: addr("journal: USDT"),
            amount: u256_from_u128(10_000_000_000),
            recipient: addr("journal: recipient 6"),
            relayer: addr("journal: recipient 6"),
            relay_fee: u256_from_u128(1),
        },
    ];
    let cases: Vec<Value> = journals
        .iter()
        .map(|j| {
            let bytes = j.encode();
            json!({
                "chainId": hx(&j.chain_id), "vault": hx(&j.vault), "lezConfigId": hx(&j.lez_config_id), "burnRoot": hx(&j.burn_root),
                "nullifier": hx(&j.nullifier), "token": hx(&j.token), "amount": hx(&j.amount), "recipient": hx(&j.recipient),
                "relayer": hx(&j.relayer), "relayFee": hx(&j.relay_fee), "journal": hx(&bytes), "digest": hx(&sha256(&bytes)),
            })
        })
        .collect();
    write_or_check(
        "release-journal-v2.json",
        json!({
            "description": "Release journals v2: abi.encode(RELEASE_DOMAIN, evmChainId, vault, lezConfigId, burnRoot, NOut, token, amount, recipient, relayer, relayFee), 352 bytes, RELEASE_DOMAIN = sha256('ferrata/vault/release/v2'); digest = sha256(journal). chainId, amount and relayFee are uint256 as 32-byte hex. Checked against Solidity by evm-verify/test/JournalEquality.t.sol.",
            "count": cases.len(), "cases": cases,
        }),
    );
}
