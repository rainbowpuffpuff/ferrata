//! Known-answer vectors in `vectors/*.json` at the repository root, for other implementations.
//!
//! `FERRATA_WRITE_VECTORS=1 cargo test --test vectors` rewrites them; otherwise this test fails if
//! any file differs from what the code computes. Every input is in the files next to every
//! output, so `scripts/check_vectors.py` (Python standard library only) recomputes them all.

mod common;

use std::path::PathBuf;

use common::{addr, h};
use ferrata_bridge_core::encoding::*;
use ferrata_bridge_core::inbound::{self, ClaimWitness};
use ferrata_bridge_core::outbound::{self, ReleasePublic, ReleaseTarget, ReleaseWitness};
use ferrata_bridge_core::tree::{zero_hashes, IncrementalTree, Tree};
use ferrata_bridge_core::{sha256, BridgeKey, Hash};
use serde_json::{json, Value};

fn hx(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
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

#[test]
fn tags() {
    let hashed: Vec<Value> = [LEAF_TAG_STR, BURN_LEAF_TAG_STR, RELEASE_DOMAIN_STR, "ferrata/vault/burn-root/v1"]
        .iter()
        .map(|t| json!({"tag": t, "sha256": hx(&sha256(t.as_bytes()))}))
        .collect();
    let prefix: Vec<Value> = PREFIX_TAGS
        .iter()
        .map(|t| json!({"tag": std::str::from_utf8(t).unwrap(), "bytes": hx(t), "length": t.len()}))
        .collect();
    write_or_check(
        "tags.json",
        json!({
            "description": "Tags. 'hashed' tags enter preimages as the SHA-256 of the ASCII string; 'prefix' tags as the ASCII bytes themselves. ferrata/vault/burn-root/v1 is the vault's root-journal domain (not used by this crate).",
            "hashed": hashed,
            "prefix": prefix,
        }),
    );
}

fn tree_vectors(
    name: &str,
    kind: &str,
    leaf_fn: fn(u64, &[u8; 20], &[u8; 32], &Hash) -> Hash,
    pre_fn: fn(u64, &[u8; 20], &[u8; 32], &Hash) -> [u8; 125],
) {
    let tokens = [[0u8; 20], addr("vectors: USDC"), addr("vectors: DAI"), addr("vectors: USDT")];
    let mut inc = IncrementalTree::new();
    let mut leaves_json = Vec::new();
    let mut leaves = Vec::new();
    for i in 0..8u64 {
        let token = tokens[(i % 4) as usize];
        let amount = u256_from_u128(10u128.pow(6 + (i % 3) as u32 * 6));
        let commitment = h(&format!("vectors: {kind} commitment {i}"));
        let leaf = leaf_fn(i, &token, &amount, &commitment);
        assert_eq!(sha256(&pre_fn(i, &token, &amount, &commitment)), leaf);
        inc.append(leaf).unwrap();
        leaves.push(leaf);
        leaves_json.push(json!({
            "leaf_index": i, "token": hx(&token), "amount": hx(&amount), "commitment": hx(&commitment),
            "preimage": hx(&pre_fn(i, &token, &amount, &commitment)), "leaf": hx(&leaf), "root_after": hx(&inc.root()),
        }));
    }
    let tree = Tree::from_leaves(leaves).unwrap();
    assert_eq!(tree.root(), inc.root());
    let paths: Vec<Value> = (0..8u64)
        .map(|i| json!({"leaf_index": i, "siblings": tree.path(i).unwrap().iter().map(|s| hx(s)).collect::<Vec<_>>()}))
        .collect();
    write_or_check(
        name,
        json!({
            "description": format!("{kind} tree: depth 32, node = sha256(0x01 || left || right), empty leaf = 32 zero bytes; leaf preimage layout in encoding.rs. root_after is the root after appending that leaf; paths are siblings leaf level first, in the final 8-leaf tree."),
            "depth": 32,
            "zero_hashes": zero_hashes().iter().map(|z| hx(z)).collect::<Vec<_>>(),
            "empty_root": hx(&zero_hashes()[32]),
            "leaves": leaves_json,
            "final_root": hx(&tree.root()),
            "paths": paths,
        }),
    );
}

#[test]
fn deposit_tree() {
    tree_vectors("deposit-tree.json", "deposit", inbound::deposit_leaf, deposit_leaf_preimage);
}

#[test]
fn burn_tree() {
    tree_vectors("burn-tree.json", "burn", outbound::burn_leaf, burn_leaf_preimage);
}

#[test]
fn inbound_derivations() {
    let mut cases = Vec::new();
    for (n, (chain_id, j, leaf_index)) in
        [(1u64, 0u64, 0u64), (11_155_111, 3, 41), (u64::MAX, u64::MAX, (1 << 32) - 1)].into_iter().enumerate()
    {
        let k = h(&format!("vectors: inbound k {n}"));
        let key = BridgeKey::new(&k);
        let (vault, depositor, dest) = (
            addr(&format!("vectors: vault {n}")),
            addr(&format!("vectors: depositor {n}")),
            h(&format!("vectors: dest {n}")),
        );
        let (token, amount) = (addr(&format!("vectors: token {n}")), u256_from_u128(1_000_000 * (n as u128 + 1)));
        let s = inbound::deposit_secret(&key, chain_id, &vault, &depositor, j);
        let c = inbound::commitment(&s, &dest);
        let leaf = inbound::deposit_leaf(leaf_index, &token, &amount, &c);
        let fid = inbound::deposit_id(chain_id, &vault, leaf_index, &leaf);
        let nf = inbound::nullifier(&s, &fid);
        cases.push(json!({
            "k": hx(&k), "chain_id": chain_id, "vault": hx(&vault), "depositor": hx(&depositor), "j": j, "dest": hx(&dest),
            "s_message": hx(&deposit_secret_message(chain_id, &vault, &depositor, j)), "s": hx(&s),
            "C_preimage": hx(&commitment_preimage(&s, &dest)), "C": hx(&c),
            "leaf_index": leaf_index, "token": hx(&token), "amount": hx(&amount), "leaf": hx(&leaf),
            "fid_preimage": hx(&deposit_id_preimage(chain_id, &vault, leaf_index, &leaf)), "fid": hx(&fid),
            "N_preimage": hx(&nullifier_preimage(&s, &fid)), "N": hx(&nf),
        }));
    }

    // One complete claim: the deposit at leaf 2 of a 5-leaf tree.
    let key = BridgeKey::new(&h("vectors: claim k"));
    let (chain_id, vault, depositor, dest) =
        (1u64, addr("vectors: claim vault"), addr("vectors: claim depositor"), h("vectors: claim dest"));
    let (token, amount) = ([0u8; 20], u256_from_u128(1_000_000_000_000_000_000));
    let s = inbound::deposit_secret(&key, chain_id, &vault, &depositor, 0);
    let mut tree = Tree::new();
    for i in 0..5u64 {
        let c = if i == 2 { inbound::commitment(&s, &dest) } else { h(&format!("vectors: other C {i}")) };
        tree.append(inbound::deposit_leaf(i, &token, &amount, &c)).unwrap();
    }
    let w = ClaimWitness { token, amount, leaf_index: 2, path: tree.path(2).unwrap(), s, dest };
    let nf = inbound::verify_claim(&tree.root(), &w, chain_id, &vault).unwrap();
    let claim = json!({
        "k": hx(&h("vectors: claim k")), "chain_id": chain_id, "vault": hx(&vault), "depositor": hx(&depositor), "j": 0,
        "dest": hx(&dest), "s": hx(&s), "token": hx(&token), "amount": hx(&amount), "leaf_index": 2,
        "leaves": tree.leaves().iter().map(|l| hx(l)).collect::<Vec<_>>(), "root": hx(&tree.root()),
        "path": w.path.iter().map(|p| hx(p)).collect::<Vec<_>>(), "N": hx(&nf),
    });
    write_or_check(
        "inbound.json",
        json!({
            "description": "Deposit -> mint. s = HMAC-SHA256(k, s_message); C = sha256(C_preimage); leaf = deposit-tree leaf; fid = sha256(fid_preimage); N = sha256(N_preimage). 'claim' is a full claim witness under a 5-leaf deposit tree (other leaves use unrelated commitments). Integers are JSON numbers; amount is a 32-byte uint256.",
            "cases": cases,
            "claim": claim,
        }),
    );
}

#[test]
fn outbound_derivations() {
    let mut cases = Vec::new();
    for (n, (chain_id, i, leaf_index)) in
        [(1u64, 0u64, 0u64), (11_155_111, 7, 1234), (u64::MAX, u64::MAX, (1 << 32) - 1)].into_iter().enumerate()
    {
        let k = h(&format!("vectors: outbound k {n}"));
        let key = BridgeKey::new(&k);
        let config = h(&format!("vectors: lez config {n}"));
        let (vault, recipient) = (addr(&format!("vectors: vault {n}")), addr(&format!("vectors: recipient {n}")));
        let (token, amount) =
            (addr(&format!("vectors: token {n}")), u256_from_u128(5 * 10u128.pow(17) * (n as u128 + 1)));
        let note =
            outbound::BurnNote::create(&key, &config, i, &ReleaseTarget { chain_id, vault, recipient }, token, amount);
        let s_out = outbound::burn_secret(&key, &note.r);
        let leaf = outbound::burn_leaf(leaf_index, &token, &amount, &note.c_out);
        let bid = outbound::burn_id(&config, leaf_index, &leaf);
        cases.push(json!({
            "k": hx(&k), "lez_config_id": hx(&config), "i": i,
            "r_message": hx(&burn_tag_message(&config, i)), "r": hx(&note.r),
            "s_out_message": hx(&burn_secret_message(&note.r)), "s_out": hx(&s_out),
            "memo_message": hx(&memo_pad_message(&note.r)), "memo_pad": hx(&outbound::memo_pad(&key, &note.r)),
            "recipient": hx(&recipient), "memo": hx(&note.memo),
            "chain_id": chain_id, "vault": hx(&vault),
            "C_out_preimage": hx(&burn_commitment_preimage(&s_out, chain_id, &vault, &recipient)), "C_out": hx(&note.c_out),
            "leaf_index": leaf_index, "token": hx(&token), "amount": hx(&amount), "burn_leaf": hx(&leaf),
            "burn_id_preimage": hx(&burn_id_preimage(&config, leaf_index, &leaf)), "burn_id": hx(&bid),
            "N_out_preimage": hx(&burn_nullifier_preimage(&s_out, &bid)), "N_out": hx(&outbound::burn_nullifier(&s_out, &bid)),
        }));
    }

    // One complete release: our burn at leaf 3 of a 6-leaf burn tree.
    let key = BridgeKey::new(&h("vectors: release k"));
    let config = h("vectors: release lez config");
    let target = ReleaseTarget {
        chain_id: 31_337,
        vault: addr("vectors: release vault"),
        recipient: addr("vectors: release recipient"),
    };
    let (token, amount) = (addr("vectors: release token"), u256_from_u128(100_000_000));
    let note = outbound::BurnNote::create(&key, &config, 0, &target, token, amount);
    let mut tree = Tree::new();
    for i in 0..6u64 {
        let c = if i == 3 { note.c_out } else { h(&format!("vectors: other C_out {i}")) };
        tree.append(outbound::burn_leaf(i, &token, &amount, &c)).unwrap();
    }
    let public = ReleasePublic {
        chain_id: target.chain_id,
        vault: target.vault,
        lez_config_id: config,
        token,
        amount,
        recipient: target.recipient,
        fee_recipient: addr("vectors: relayer"),
        relay_fee: u256_from_u128(250_000),
    };
    let w = ReleaseWitness { leaf_index: 3, path: tree.path(3).unwrap(), s_out: outbound::burn_secret(&key, &note.r) };
    let n_out = outbound::verify_release(&tree.root(), &w, &public).unwrap();
    let journal = outbound::release_journal(&tree.root(), &n_out, &public);
    let release = json!({
        "k": hx(&h("vectors: release k")), "lez_config_id": hx(&config), "i": 0, "r": hx(&note.r), "memo": hx(&note.memo),
        "chain_id": public.chain_id, "vault": hx(&public.vault), "recipient": hx(&public.recipient),
        "token": hx(&token), "amount": hx(&amount), "C_out": hx(&note.c_out),
        "fee_recipient": hx(&public.fee_recipient), "relay_fee": hx(&public.relay_fee),
        "leaf_index": 3, "leaves": tree.leaves().iter().map(|l| hx(l)).collect::<Vec<_>>(), "burn_root": hx(&tree.root()),
        "path": w.path.iter().map(|p| hx(p)).collect::<Vec<_>>(), "s_out": hx(&w.s_out), "N_out": hx(&n_out),
        "journal": hx(&journal), "journal_sha256": hx(&sha256(&journal)),
        "guest_input": hx(&ReleaseInput { burn_root: tree.root(), witness: w.clone(), public: public.clone() }.encode()),
    });
    write_or_check(
        "outbound.json",
        json!({
            "description": "Burn -> release. r = HMAC-SHA256(k, r_message); s_out = HMAC(k, s_out_message); memo_pad = HMAC(k, memo_message)[0..20]; memo = recipient XOR memo_pad; C_out = sha256(C_out_preimage); burn_leaf = burn-tree leaf; burn_id = sha256(burn_id_preimage); N_out = sha256(N_out_preimage). 'release' is a full release witness under a 6-leaf burn tree, with the vault journal and the release guest's 1280-byte input.",
            "cases": cases,
            "release": release,
        }),
    );
}

#[test]
fn release_journals() {
    let ff20 = [0xffu8; 20];
    let ff32 = [0xffu8; 32];
    let journals = [
        ReleaseJournal {
            chain_id: 1,
            vault: addr("journal: vault 1"),
            lez_config_id: h("journal: config 1"),
            burn_root: h("journal: root 1"),
            nullifier: h("journal: nf 1"),
            token: [0u8; 20],
            amount: u256_from_u128(1_000_000_000_000_000_000),
            recipient: addr("journal: recipient 1"),
            fee_recipient: [0u8; 20],
            relay_fee: [0u8; 32],
        },
        ReleaseJournal {
            chain_id: 11_155_111,
            vault: addr("journal: vault 2"),
            lez_config_id: h("journal: config 2"),
            burn_root: h("journal: root 2"),
            nullifier: h("journal: nf 2"),
            token: addr("journal: USDC"),
            amount: u256_from_u128(100_000_000),
            recipient: addr("journal: recipient 2"),
            fee_recipient: addr("journal: relayer"),
            relay_fee: u256_from_u128(250_000),
        },
        ReleaseJournal {
            chain_id: 31_337,
            vault: addr("journal: vault 3"),
            lez_config_id: h("journal: config 3"),
            burn_root: h("journal: root 3"),
            nullifier: h("journal: nf 3"),
            token: addr("journal: DAI"),
            amount: u256_from_u128(u128::MAX),
            recipient: addr("journal: recipient 3"),
            fee_recipient: addr("journal: relayer 3"),
            relay_fee: u256_from_u128(u128::MAX - 1),
        },
        ReleaseJournal {
            chain_id: u64::MAX,
            vault: ff20,
            lez_config_id: ff32,
            burn_root: ff32,
            nullifier: ff32,
            token: ff20,
            amount: ff32,
            recipient: ff20,
            fee_recipient: ff20,
            relay_fee: ff32,
        },
        ReleaseJournal {
            chain_id: 0,
            vault: [0u8; 20],
            lez_config_id: [0u8; 32],
            burn_root: [0u8; 32],
            nullifier: [0u8; 32],
            token: [0u8; 20],
            amount: [0u8; 32],
            recipient: [0u8; 20],
            fee_recipient: [0u8; 20],
            relay_fee: [0u8; 32],
        },
        ReleaseJournal {
            chain_id: 17_000,
            vault: addr("journal: vault 6"),
            lez_config_id: h("journal: config 6"),
            burn_root: h("journal: root 6"),
            nullifier: h("journal: nf 6"),
            token: addr("journal: USDT"),
            amount: u256_from_u128(10_000_000_000),
            recipient: addr("journal: recipient 6"),
            fee_recipient: addr("journal: recipient 6"),
            relay_fee: u256_from_u128(1),
        },
    ];
    let cases: Vec<Value> = journals
        .iter()
        .map(|j| {
            let bytes = j.encode();
            json!({
                "chainId": j.chain_id, "vault": hx(&j.vault), "lezConfigId": hx(&j.lez_config_id), "burnRoot": hx(&j.burn_root),
                "nullifier": hx(&j.nullifier), "token": hx(&j.token), "amount": hx(&j.amount), "recipient": hx(&j.recipient),
                "feeRecipient": hx(&j.fee_recipient), "relayFee": hx(&j.relay_fee),
                "journal": hx(&bytes), "digest": hx(&sha256(&bytes)),
            })
        })
        .collect();
    write_or_check(
        "release-journal.json",
        json!({
            "description": "Release journals: abi.encode(RELEASE_DOMAIN, chainId, vault, lezConfigId, burnRoot, nullifier, token, amount, recipient, feeRecipient, relayFee), 352 bytes; digest = sha256(journal), what FerrataVault passes to verifier.verify. amount and relayFee are uint256 written as 32-byte hex. Checked against Solidity by evm-verify/test/JournalEquality.t.sol.",
            "count": cases.len(),
            "cases": cases,
        }),
    );
}
