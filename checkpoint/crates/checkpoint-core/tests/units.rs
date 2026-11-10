//! Unit tests of the core on hand-encoded inputs. The same rule is exercised on real-format LEZ
//! blocks and Mantle transactions (built by the upstream crates) in `fidelity/tests`.

mod common;

use borsh::{BorshDeserialize, BorshSerialize};
use checkpoint_core::hash::{sha256, threshold, BURN_TAG, CHECKPOINT_DOMAIN};
use checkpoint_core::journal::{CheckpointJournal, SegmentJournal, CHECKPOINT_JOURNAL_LEN};
use checkpoint_core::lez::{parse_block, NoVisitor};
use checkpoint_core::mantle::{inscription_msg_id, opcodes, ElidedInscription};
use checkpoint_core::tree::{burn_leaf, node, zero_hashes, Frontier, MAX_LEAVES};
use checkpoint_core::wire::{BridgeMessage, Burn, ImportRoot, BURN_LEN, EMPTY_IMPORT_PROOF};
use checkpoint_core::{Error, Hash};
use common::*;

#[test]
fn threshold_matches_lez() {
    // max(1, min(ceil(2n/3), n-1)), LEZ committee_discovery.rs.
    let want = [(1, 1), (2, 1), (3, 2), (4, 3), (5, 4), (6, 4), (7, 5), (10, 7), (100, 67)];
    for (n, t) in want {
        assert_eq!(threshold(n), t, "t({n})");
    }
    assert_eq!(threshold(0), 1);
}

#[test]
fn hashed_constants() {
    assert_eq!(BURN_TAG, sha256(b"ferrata/burn/leaf/v2"));
    assert_eq!(CHECKPOINT_DOMAIN, sha256(b"ferrata/vault/checkpoint/v3"));
}

// ---- wire ------------------------------------------------------------------------------------------

/// The bridge program's `ImportProof` (`lez/bridge-wire`), written with borsh's derive.
#[derive(BorshSerialize, BorshDeserialize, PartialEq, Debug, Default, Clone)]
struct DerivedImportProof {
    account: Vec<Vec<u8>>,
    root_slot: Vec<Vec<u8>>,
    count_slot: Vec<Vec<u8>>,
}

/// The same enum written with borsh's derive; the hand codec must agree byte for byte.
#[derive(BorshSerialize, BorshDeserialize, PartialEq, Debug)]
enum Derived {
    Burn {
        config_id: [u8; 32],
        r: [u8; 32],
        memo: [u8; 20],
        mac: [u8; 16],
        token: [u8; 20],
        amount: [u8; 32],
        c_out: [u8; 32],
    },
    ImportRoot {
        config_id: [u8; 32],
        record_slot: u64,
        root: [u8; 32],
        leaf_count: u64,
        proof: DerivedImportProof,
    },
    Sunset {
        config_id: [u8; 32],
    },
    Echo {
        nonce: [u8; 32],
    },
}

#[test]
fn wire_codec_equals_borsh_derive() {
    let burn = Burn {
        config_id: [1; 32],
        r: [2; 32],
        memo: [3; 20],
        mac: [4; 16],
        token: [5; 20],
        amount: [6; 32],
        c_out: [7; 32],
    };
    let cases = [
        (
            BridgeMessage::Burn(burn),
            Derived::Burn {
                config_id: [1; 32],
                r: [2; 32],
                memo: [3; 20],
                mac: [4; 16],
                token: [5; 20],
                amount: [6; 32],
                c_out: [7; 32],
            },
        ),
        (
            BridgeMessage::ImportRoot(ImportRoot {
                config_id: [8; 32],
                record_slot: 0x0102,
                root: [9; 32],
                leaf_count: 77,
            }),
            Derived::ImportRoot {
                config_id: [8; 32],
                record_slot: 0x0102,
                root: [9; 32],
                leaf_count: 77,
                proof: DerivedImportProof::default(),
            },
        ),
        (BridgeMessage::Sunset { config_id: [10; 32] }, Derived::Sunset { config_id: [10; 32] }),
        (BridgeMessage::Echo { nonce: [11; 32] }, Derived::Echo { nonce: [11; 32] }),
    ];
    for (ours, derived) in cases {
        let bytes = borsh::to_vec(&derived).unwrap();
        assert_eq!(ours.encode(), bytes);
        assert_eq!(BridgeMessage::decode(&bytes).unwrap(), Some(ours));
    }
    assert_eq!(BridgeMessage::Burn(burn).encode().len(), BURN_LEN);
    assert_eq!(borsh::to_vec(&DerivedImportProof::default()).unwrap(), EMPTY_IMPORT_PROOF);
}

/// `ImportRoot` as the bridge program sends it: the four fields, then the proofs of the vault's
/// account and of its slots 0 and 1. The checkpoint reads the four fields and walks the proof
/// strictly; the four fields alone (the format before the program existed) are refused.
#[test]
fn wire_import_root_skips_its_proof_strictly() {
    let node = |len: usize, b: u8| vec![b; len];
    let proof = DerivedImportProof {
        account: vec![node(532, 1), node(83, 2), node(0, 3), node(104, 4)],
        root_slot: vec![node(532, 5), node(67, 6)],
        count_slot: vec![node(33, 7)],
    };
    let four = ImportRoot { config_id: [8; 32], record_slot: 9_000_000, root: [9; 32], leaf_count: 8 };
    let program = |proof: DerivedImportProof| {
        borsh::to_vec(&Derived::ImportRoot {
            config_id: [8; 32],
            record_slot: 9_000_000,
            root: [9; 32],
            leaf_count: 8,
            proof,
        })
        .unwrap()
    };
    let full = program(proof.clone());
    assert_eq!(BridgeMessage::decode(&full).unwrap(), Some(BridgeMessage::ImportRoot(four)));
    // The four fields alone: 81 bytes, the proof missing.
    assert_eq!(BridgeMessage::decode(&full[..81]), Err(Error::Truncated { at: 81 }));
    // Every shorter prefix and one byte more are refused.
    for k in 0..full.len() {
        assert!(BridgeMessage::decode(&full[..k]).is_err(), "prefix of {k} bytes");
    }
    let mut long = full.clone();
    long.push(0);
    assert!(matches!(BridgeMessage::decode(&long), Err(Error::TrailingBytes { .. })));
    // A list or element length beyond the input is refused before anything is read.
    let mut hostile = full[..81].to_vec();
    hostile.extend_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(BridgeMessage::decode(&hostile), Err(Error::BadLength { what: "ImportProof.account" })));
    let mut hostile = full[..81].to_vec();
    hostile.extend_from_slice(&1u32.to_le_bytes());
    hostile.extend_from_slice(&100u32.to_le_bytes());
    hostile.extend_from_slice(&[0; 8]);
    assert!(matches!(BridgeMessage::decode(&hostile), Err(Error::BadLength { what: "ImportProof.account" })));
    // The encoder writes an empty proof, which the program's decoder accepts too.
    let encoded = BridgeMessage::ImportRoot(four).encode();
    assert_eq!(encoded, program(DerivedImportProof::default()));
    assert_eq!(encoded.len(), 81 + EMPTY_IMPORT_PROOF.len());
}

#[test]
fn wire_decode_is_exact() {
    let echo = BridgeMessage::Echo { nonce: [1; 32] }.encode();
    let mut long = echo.clone();
    long.push(0);
    assert!(matches!(BridgeMessage::decode(&long), Err(Error::TrailingBytes { .. })));
    assert!(matches!(BridgeMessage::decode(&echo[..20]), Err(Error::Truncated { .. })));
    // Variants checkpoints do not read decode to None, whatever follows.
    assert_eq!(BridgeMessage::decode(&[4, 1, 2, 3]).unwrap(), None);
}

// ---- tree ------------------------------------------------------------------------------------------

/// Root of the depth-32 tree with these leaves at the front, computed naively level by level.
fn naive_root(leaves: &[Hash]) -> Hash {
    let zeros = zero_hashes();
    let mut level: Vec<Hash> = leaves.to_vec();
    for z in zeros.iter().take(32) {
        if level.is_empty() {
            level.push(*z);
        }
        if level.len() % 2 == 1 {
            level.push(*z);
        }
        level = level.chunks(2).map(|p| node(&p[0], &p[1])).collect();
    }
    level[0]
}

#[test]
fn frontier_root_equals_naive_root_and_stays_canonical() {
    let zeros = zero_hashes();
    let mut f = Frontier::default();
    let mut leaves = Vec::new();
    assert_eq!(f.root(&zeros), zeros[32]);
    for i in 0..40u64 {
        let leaf = burn_leaf(&[1; 32], i, &[2; 20], &[3; 32], &[4; 32], &sha256(&i.to_be_bytes()));
        f.append(leaf).unwrap();
        leaves.push(leaf);
        assert!(f.is_canonical());
        assert_eq!(f.root(&zeros), naive_root(&leaves), "count {}", i + 1);
    }
}

#[test]
fn frontier_rejects_non_canonical_and_full() {
    let mut f = Frontier { count: 2, ..Frontier::default() };
    f.nodes[0] = [1; 32]; // bit 0 of 2 is 0, so node 0 must be zero
    assert!(!f.is_canonical());
    let full = Frontier { count: MAX_LEAVES, nodes: [[1; 32]; 32] };
    assert!(full.is_canonical());
    let mut full = full;
    assert_eq!(full.append([0; 32]), Err(Error::BurnTreeFull));
    assert!(!Frontier { count: MAX_LEAVES + 1, ..Frontier::default() }.is_canonical());
}

// ---- journal ---------------------------------------------------------------------------------------

#[test]
fn journal_layout() {
    let j = CheckpointJournal {
        evm_chain_id: {
            let mut w = [0; 32];
            w[31] = 1;
            w
        },
        vault: [0xAA; 20],
        lez_config_id: [2; 32],
        n_min: 4,
        t_min: 3,
        parent_state: [5; 32],
        new_state: [6; 32],
        burn_root: [7; 32],
        burn_count: 0x0102,
        imported_leaf_count: 9,
        sunset: true,
        echo_nonce: [10; 32],
        tip_id: 0x0A0B,
        tip_hash: [11; 32],
        n: 7,
        t: 5,
    };
    let e = j.encode();
    assert_eq!(e.len(), CHECKPOINT_JOURNAL_LEN);
    assert_eq!(&e[..32], &CHECKPOINT_DOMAIN);
    assert_eq!(e[63], 1);
    assert_eq!(&e[64..76], &[0; 12]);
    assert_eq!(&e[76..96], &[0xAA; 20]);
    assert_eq!(e[159], 4);
    assert_eq!(e[191], 3);
    assert_eq!(&e[318..320], &[1, 2]);
    assert_eq!(e[351], 9);
    assert_eq!(&e[352..384], &{
        let mut w = [0; 32];
        w[31] = 1;
        w
    });
    assert_eq!(&e[384..416], &[10; 32]);
    assert_eq!((&e[416..446], e[446], e[447]), (&[0u8; 30][..], 0x0A, 0x0B));
    assert_eq!(&e[448..480], &[11; 32]);
    assert_eq!((e[511], e[543]), (7, 5));
    assert_eq!(&e[480..511], &[0; 31]);
    let s = SegmentJournal { lez_config_id: [1; 32], start_state: [2; 32], end_state: [3; 32] };
    assert_eq!(SegmentJournal::decode(&s.encode()).unwrap(), s);
}

// ---- LEZ block parser ------------------------------------------------------------------------------

#[test]
fn block_parses_and_hashes() {
    let txs = vec![public_tx(&[1; 32], &[2; 32], b"hello"), pp_tx(&[3; 32], &[4; 32], b"x", 10)];
    let (bytes, hash) = block(7, &[9; 32], 700, &txs, &[1]);
    let s = parse_block(&bytes, &mut NoVisitor).unwrap();
    assert_eq!((s.header.block_id, s.header.prev_block_hash, s.header.timestamp), (7, [9; 32], 700));
    assert_eq!(s.header.hash, hash);
    assert_eq!(checkpoint_core::lez::hash_parsed_block(&bytes).unwrap(), hash);
    assert_eq!((s.tx_count, s.reverted.clone(), s.status), (2, vec![1], 0));
    assert!(s.is_reverted(1) && !s.is_reverted(0));
}

#[test]
fn block_parser_is_strict() {
    let tx = public_tx(&[1; 32], &[2; 32], b"m");
    let (good, _) = block(1, &[0; 32], 0, std::slice::from_ref(&tx), &[]);
    // trailing byte
    let mut bad = good.clone();
    bad.push(0);
    assert!(matches!(parse_block(&bad, &mut NoVisitor), Err(Error::TrailingBytes { .. })));
    // status out of range
    let mut bad = good.clone();
    *bad.last_mut().unwrap() = 3;
    assert!(matches!(parse_block(&bad, &mut NoVisitor), Err(Error::BadTag { what: "BedrockStatus", .. })));
    // LeeTransaction tag 2
    let mut bad = good.clone();
    bad[84] = 2;
    assert!(matches!(parse_block(&bad, &mut NoVisitor), Err(Error::BadTag { what: "LeeTransaction", .. })));
    // two equal actors: not strictly increasing
    let mut t = tx.clone();
    t[1] = 2; // two actors...
    let actor: Vec<u8> = t[5..69].to_vec();
    t.splice(69..69, actor);
    let (bad, _) = block(1, &[0; 32], 0, &[t], &[]);
    assert!(matches!(parse_block(&bad, &mut NoVisitor), Err(Error::NotStrictlyOrdered { what: "actors" })));
    // reverted out of range / not ascending
    let (bad, _) = block(1, &[0; 32], 0, std::slice::from_ref(&tx), &[1]);
    assert!(matches!(parse_block(&bad, &mut NoVisitor), Err(Error::BadRevertedList { .. })));
    let (bad, _) = block(1, &[0; 32], 0, &[tx.clone(), tx.clone()], &[1, 0]);
    assert!(matches!(parse_block(&bad, &mut NoVisitor), Err(Error::BadRevertedList { .. })));
    // option tag 2 (fee declaration)
    let mut t = tx.clone();
    let fee_at = t.len() - 13;
    t[fee_at] = 2;
    let (bad, _) = block(1, &[0; 32], 0, &[t], &[]);
    assert!(matches!(parse_block(&bad, &mut NoVisitor), Err(Error::BadTag { what: "fee declaration", .. })));
    // every truncation fails cleanly
    for cut in 0..good.len() {
        assert!(parse_block(&good[..cut], &mut NoVisitor).is_err());
    }
}

// ---- Mantle ----------------------------------------------------------------------------------------

#[test]
fn inscription_check() {
    let k = key(1);
    let channel = [0x11; 32];
    let payload = b"a block".to_vec();
    let (ops, sig) = inscription_tx(&k, &channel, &payload, &[0x22; 32]);
    assert_eq!(opcodes(&ops).unwrap(), vec![0x11, 0x00]);
    let e = elide(&ops, &sig);
    let checked = e.check(&payload, &channel).unwrap();
    assert_eq!(checked.signer, *k.verifying_key().as_bytes());
    assert_eq!(checked.parent, [0x22; 32]);
    assert_eq!(checked.tx_hash, checkpoint_core::mantle::tx_hash(&ops));
    // wrong channel, other payload, tampered signature
    assert_eq!(e.check(&payload, &[0; 32]), Err(Error::WrongChannel));
    assert_eq!(e.check(b"a blocK", &channel), Err(Error::BadSignature));
    assert_eq!(e.check(b"a longer block", &channel), Err(Error::PayloadMismatch));
    let mut bad_sig = sig;
    bad_sig[3] ^= 1;
    let e2 = ElidedInscription { signature: &bad_sig, ..e };
    assert_eq!(e2.check(&payload, &channel), Err(Error::BadSignature));
    // msg id = blake2b(encode(op))
    let id = inscription_msg_id(&channel, &payload, &[0x22; 32], k.verifying_key().as_bytes());
    assert_eq!(id, checkpoint_core::hash::blake2b256_parts(&[&ops[2..ops.len() - 75]]));
    // an unknown opcode fails closed
    let mut bad = ops.clone();
    let transfer_at = ops.len() - 75;
    bad[transfer_at] = 0x12;
    assert_eq!(opcodes(&bad), Err(Error::UnsupportedOpcode { opcode: 0x12 }));
}
