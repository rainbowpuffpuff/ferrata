//! Byte layouts (SPEC-v2): every encoding restated here field by field from the spec text and compared
//! with the library's; hashed tags checked against their strings; prefix tags exact and prefix-free;
//! journal and input codecs round-trip and reject non-canonical bytes.

mod common;

use common::{addr, h, kind_strategy, path_strategy};
use ferrata_bridge_core::encoding::*;
use ferrata_bridge_core::inbound::{ClaimWitness, DestKind};
use ferrata_bridge_core::outbound::{ReleasePublic, ReleaseWitness};
use ferrata_bridge_core::sha256;
use ferrata_bridge_core::tree::{MerklePath, TREE_DEPTH};
use proptest::prelude::*;

fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

#[test]
fn hashed_tags_are_sha256_of_their_strings() {
    assert_eq!(LEAF_TAG, sha256(b"ferrata/vault/leaf/v2"));
    assert_eq!(BURN_TAG, sha256(b"ferrata/burn/leaf/v2"));
    assert_eq!(RELEASE_DOMAIN, sha256(b"ferrata/vault/release/v2"));
    assert_eq!(CHECKPOINT_DOMAIN, sha256(b"ferrata/vault/checkpoint/v2"));
    for (c, s) in [
        (LEAF_TAG, LEAF_TAG_STR),
        (BURN_TAG, BURN_TAG_STR),
        (RELEASE_DOMAIN, RELEASE_DOMAIN_STR),
        (CHECKPOINT_DOMAIN, CHECKPOINT_DOMAIN_STR),
    ] {
        assert_eq!(c, sha256(s.as_bytes()));
    }
}

#[test]
fn prefix_tags_are_exact_strings_and_prefix_free() {
    let expected = [
        "ferrata/auth/v1",
        "ferrata/view/v1",
        "ferrata/config/v1",
        "ferrata/s/v1",
        "ferrata/C/v1",
        "ferrata/fact/acc-leaf/v1",
        "ferrata/nf/v1",
        "ferrata/burn-tag/v1",
        "ferrata/s-out/v1",
        "ferrata/Cout/v1",
        "ferrata/memo/v1",
        "ferrata/burn-mac/v2",
        "ferrata/burn/v1",
        "ferrata/nf-out/v2",
        "ferrata/keyset/v1",
        "ferrata/burn-frontier/v1",
        "ferrata/lez-state/v1",
    ];
    for (tag, want) in PREFIX_TAGS.iter().zip(expected) {
        assert_eq!(*tag, want.as_bytes());
        assert!(![LEAF_PREFIX, NODE_PREFIX, BURN_LEAF_PREFIX].contains(&tag[0]));
    }
    for (i, a) in PREFIX_TAGS.iter().enumerate() {
        for (j, b) in PREFIX_TAGS.iter().enumerate() {
            if i != j {
                assert!(!b.starts_with(a), "{:?} is a prefix of {:?}", a, b);
            }
        }
    }
}

#[test]
fn layouts_match_the_spec_field_by_field() {
    let (vault, depositor, token, recipient) = (addr("vault"), addr("depositor"), addr("token"), addr("recipient"));
    let (amount, c, s, dest, leaf, r, cfg) = (h("amount"), h("C"), h("s"), h("dest"), h("leaf"), h("r"), h("configId"));
    let (fid, left, right, c_out, s_out) = (h("fid"), h("left"), h("right"), h("COut"), h("sOut"));
    let chain = h("a uint256 chain id");
    let (j, i, idx) = (7u64, 9u64, 0x1122_3344_5566_7788u64);

    let ci = ConfigIdentity {
        bedrock_genesis_id: h("genesis"),
        channel_id: h("channel"),
        bridge_header: h("header"),
        bridge_image_id: h("image"),
        evm_chain_id: chain,
        vault,
        attestation_root_id: h("attestation"),
        salt: h("salt"),
    };
    let cases: Vec<(Vec<u8>, Vec<u8>, usize)> = vec![
        (
            config_id_preimage(&ci).to_vec(),
            cat(&[
                b"ferrata/config/v1",
                &h("genesis"),
                &h("channel"),
                &h("header"),
                &h("image"),
                &chain,
                &vault,
                &h("attestation"),
                &h("salt"),
            ]),
            261,
        ),
        (
            deposit_leaf_preimage(idx, &depositor, &token, &amount, &c).to_vec(),
            cat(&[&[0x00], &sha256(b"ferrata/vault/leaf/v2"), &idx.to_be_bytes(), &depositor, &token, &amount, &c]),
            145,
        ),
        (
            burn_leaf_preimage(&cfg, idx, &token, &amount, &c_out, &r).to_vec(),
            cat(&[&[0x02], &sha256(b"ferrata/burn/leaf/v2"), &cfg, &idx.to_be_bytes(), &token, &amount, &c_out, &r]),
            189,
        ),
        (node_preimage(&left, &right).to_vec(), cat(&[&[0x01], &left, &right]), 65),
        (
            deposit_secret_message(&chain, &vault, &depositor, j).to_vec(),
            cat(&[b"ferrata/s/v1", &chain, &vault, &depositor, &j.to_be_bytes()]),
            92,
        ),
        (commitment_preimage(&s, DestKind::Private, &dest).to_vec(), cat(&[b"ferrata/C/v1", &s, &[0x01], &dest]), 77),
        (commitment_preimage(&s, DestKind::Public, &dest).to_vec(), cat(&[b"ferrata/C/v1", &s, &[0x02], &dest]), 77),
        (
            deposit_id_preimage(&chain, &vault, idx, &leaf).to_vec(),
            cat(&[b"ferrata/fact/acc-leaf/v1", &chain, &vault, &[0u8; 32], &idx.to_be_bytes(), &leaf]),
            148,
        ),
        (nullifier_preimage(&s, &fid).to_vec(), cat(&[b"ferrata/nf/v1", &s, &fid]), 77),
        (burn_tag_message(&cfg, i).to_vec(), cat(&[b"ferrata/burn-tag/v1", &cfg, &i.to_be_bytes()]), 59),
        (burn_secret_message(&cfg, &r).to_vec(), cat(&[b"ferrata/s-out/v1", &cfg, &r]), 80),
        (
            burn_commitment_preimage(&s_out, &chain, &vault, &recipient).to_vec(),
            cat(&[b"ferrata/Cout/v1", &s_out, &chain, &vault, &recipient]),
            119,
        ),
        (memo_pad_message(&r).to_vec(), cat(&[b"ferrata/memo/v1", &r]), 47),
        (
            burn_mac_message(&r, &recipient, &token, &amount, &c_out).to_vec(),
            cat(&[b"ferrata/burn-mac/v2", &r, &recipient, &token, &amount, &c_out]),
            155,
        ),
        (burn_id_preimage(&cfg, idx, &leaf).to_vec(), cat(&[b"ferrata/burn/v1", &cfg, &idx.to_be_bytes(), &leaf]), 87),
        (burn_nullifier_preimage(&s_out).to_vec(), cat(&[b"ferrata/nf-out/v2", &s_out]), 49),
    ];
    for (n, (got, want, len)) in cases.into_iter().enumerate() {
        assert_eq!(got, want, "case {n}");
        assert_eq!(got.len(), len, "case {n}");
    }
}

#[test]
fn checkpoint_layouts_match_the_spec() {
    let keys = [h("key 1"), h("key 2"), h("key 3")];
    assert_eq!(
        key_set_hash(&keys),
        sha256(&cat(&[b"ferrata/keyset/v1", &3u32.to_be_bytes(), &keys[0], &keys[1], &keys[2]]))
    );
    assert_eq!(key_set_hash(&[]), sha256(&cat(&[b"ferrata/keyset/v1", &0u32.to_be_bytes()])));

    let frontier: [Hash; TREE_DEPTH] = core::array::from_fn(|i| h(&format!("f{i}")));
    let mut want = b"ferrata/burn-frontier/v1".to_vec();
    for f in &frontier {
        want.extend_from_slice(f);
    }
    assert_eq!(burn_frontier_preimage(&frontier).to_vec(), want);
    assert_eq!(want.len(), 1048);

    let s = LezState {
        tip_id: 0x0102_0304_0506_0708,
        tip_hash: h("tip"),
        key_set_hash: h("keyset"),
        config_tip_hash: h("config tip"),
        burn_count: 42,
        burn_frontier_hash: h("frontier"),
        imported_leaf_count: 77,
        sunset: true,
    };
    let want = cat(&[
        b"ferrata/lez-state/v1",
        &0x0102_0304_0506_0708u64.to_be_bytes(),
        &h("tip"),
        &h("keyset"),
        &h("config tip"),
        &42u64.to_be_bytes(),
        &h("frontier"),
        &77u64.to_be_bytes(),
        &[1],
    ]);
    assert_eq!(lez_state_preimage(&s).to_vec(), want);
    assert_eq!(want.len(), 173);
    assert_eq!(lez_state_preimage(&LezState { sunset: false, ..s })[172], 0);
}

fn word_n(v: u64) -> Vec<u8> {
    let mut w = vec![0u8; 24];
    w.extend_from_slice(&v.to_be_bytes());
    w
}

fn word_addr(a: &Address) -> Vec<u8> {
    let mut w = vec![0u8; 12];
    w.extend_from_slice(a);
    w
}

fn release_journal_strategy() -> impl Strategy<Value = ReleaseJournal> {
    (
        (any::<[u8; 32]>(), any::<[u8; 20]>(), any::<[u8; 32]>(), any::<[u8; 32]>(), any::<[u8; 32]>()),
        (any::<[u8; 20]>(), any::<[u8; 32]>(), any::<[u8; 20]>(), any::<[u8; 20]>(), any::<[u8; 32]>()),
    )
        .prop_map(
            |(
                (chain_id, vault, lez_config_id, burn_root, nullifier),
                (token, amount, recipient, relayer, relay_fee),
            )| {
                ReleaseJournal {
                    chain_id,
                    vault,
                    lez_config_id,
                    burn_root,
                    nullifier,
                    token,
                    amount,
                    recipient,
                    relayer,
                    relay_fee,
                }
            },
        )
}

fn checkpoint_journal_strategy() -> impl Strategy<Value = CheckpointJournal> {
    (
        (any::<[u8; 32]>(), any::<[u8; 20]>(), any::<[u8; 32]>(), any::<u32>(), any::<u32>()),
        (any::<[u8; 32]>(), any::<[u8; 32]>(), any::<[u8; 32]>()),
        (any::<u64>(), any::<u64>(), any::<bool>(), any::<[u8; 32]>()),
    )
        .prop_map(
            |(
                (chain_id, vault, lez_config_id, n_min, t_min),
                (parent_state, new_state, burn_root),
                (burn_count, imported_leaf_count, sunset, echo_nonce),
            )| CheckpointJournal {
                chain_id,
                vault,
                lez_config_id,
                n_min,
                t_min,
                parent_state,
                new_state,
                burn_root,
                burn_count,
                imported_leaf_count,
                sunset,
                echo_nonce,
            },
        )
}

proptest! {
    #[test]
    fn release_journal_is_eleven_abi_words(j in release_journal_strategy()) {
        let bytes = j.encode();
        let want = cat(&[
            &sha256(b"ferrata/vault/release/v2"), &j.chain_id, &word_addr(&j.vault), &j.lez_config_id, &j.burn_root,
            &j.nullifier, &word_addr(&j.token), &j.amount, &word_addr(&j.recipient), &word_addr(&j.relayer), &j.relay_fee,
        ]);
        prop_assert_eq!(bytes.to_vec(), want);
        prop_assert_eq!(ReleaseJournal::decode(&bytes), Some(j));
    }

    #[test]
    fn release_journal_decode_rejects_non_canonical_bytes(j in release_journal_strategy(), word in 0usize..11, at in 0usize..32, mask in 1u8..=255) {
        let mut bytes = j.encode();
        bytes[word * 32 + at] ^= mask;
        let padding = matches!(word, 2 | 6 | 8 | 9) && at < 12;
        let decoded = ReleaseJournal::decode(&bytes);
        if word == 0 || padding {
            prop_assert_eq!(decoded, None);
        } else {
            prop_assert!(decoded.is_some() && decoded != Some(j));
        }
        prop_assert_eq!(ReleaseJournal::decode(&bytes[..351]), None);
    }

    #[test]
    fn checkpoint_journal_is_thirteen_abi_words(j in checkpoint_journal_strategy()) {
        let bytes = j.encode();
        let want = cat(&[
            &sha256(b"ferrata/vault/checkpoint/v2"), &j.chain_id, &word_addr(&j.vault), &j.lez_config_id,
            &word_n(u64::from(j.n_min)), &word_n(u64::from(j.t_min)), &j.parent_state, &j.new_state, &j.burn_root,
            &word_n(j.burn_count), &word_n(j.imported_leaf_count), &word_n(u64::from(j.sunset)), &j.echo_nonce,
        ]);
        prop_assert_eq!(bytes.len(), 416);
        prop_assert_eq!(bytes.to_vec(), want);
        prop_assert_eq!(CheckpointJournal::decode(&bytes), Some(j));
    }

    #[test]
    fn checkpoint_journal_decode_rejects_non_canonical_bytes(j in checkpoint_journal_strategy(), word in 0usize..13, at in 0usize..32, mask in 1u8..=255) {
        let mut bytes = j.encode();
        bytes[word * 32 + at] ^= mask;
        // Padding: address word 2 (12 bytes); u32 words 4, 5 (28); u64 words 9, 10 (24); bool word 11
        // (31 bytes, and the last byte may only be 0 or 1).
        let canonical_change = match word {
            0 => false,
            2 => at >= 12,
            4 | 5 => at >= 28,
            9 | 10 => at >= 24,
            11 => at == 31 && (bytes[word * 32 + 31] <= 1),
            _ => true,
        };
        let decoded = CheckpointJournal::decode(&bytes);
        if canonical_change {
            prop_assert!(decoded.is_some() && decoded != Some(j));
        } else {
            prop_assert_eq!(decoded, None);
        }
    }

    #[test]
    fn release_input_round_trips(burn_root in any::<[u8; 32]>(), burn_index in any::<u64>(), path in path_strategy(),
                                 s_out in any::<[u8; 32]>(), r in any::<[u8; 32]>(), j in release_journal_strategy()) {
        let input = ReleaseInput {
            burn_root,
            witness: ReleaseWitness { burn_index, path, s_out, r },
            public: ReleasePublic {
                chain_id: j.chain_id, vault: j.vault, config_id: j.lez_config_id, token: j.token, amount: j.amount,
                recipient: j.recipient, relayer: j.relayer, relay_fee: j.relay_fee,
            },
        };
        let bytes = input.encode();
        prop_assert_eq!(&bytes[40..44], &path.directions.to_be_bytes()[..]);
        prop_assert_eq!(&bytes[1068..1100], &s_out[..]);
        prop_assert_eq!(&bytes[1100..1132], &r[..]);
        prop_assert_eq!(&bytes[1308..], &j.relay_fee[..]);
        prop_assert_eq!(ReleaseInput::decode(&bytes), input);
    }

    #[test]
    fn claim_input_round_trips(root in any::<[u8; 32]>(), chain_id in any::<[u8; 32]>(), vault in any::<[u8; 20]>(),
                               depositor in any::<[u8; 20]>(), token in any::<[u8; 20]>(), amount in any::<[u8; 32]>(),
                               leaf_index in any::<u64>(), path in path_strategy(), s in any::<[u8; 32]>(),
                               kind in kind_strategy(), dest in any::<[u8; 32]>()) {
        let input = ClaimInput { root, chain_id, vault, witness: ClaimWitness { depositor, token, amount, leaf_index, path, s, kind, dest } };
        let bytes = input.encode();
        prop_assert_eq!(bytes[1224], kind as u8);
        prop_assert_eq!(&bytes[1228..], &dest[..]);
        prop_assert_eq!(ClaimInput::decode(&bytes), input);
    }
}

#[test]
#[should_panic(expected = "kind must be 0x01 or 0x02")]
fn claim_input_rejects_an_unknown_kind() {
    let w = ClaimWitness {
        depositor: addr("d"),
        token: addr("t"),
        amount: [0u8; 32],
        leaf_index: 0,
        path: MerklePath { directions: 0, siblings: [[0u8; 32]; 32] },
        s: h("s"),
        kind: DestKind::Private,
        dest: h("dest"),
    };
    let mut bytes = ClaimInput { root: h("root"), chain_id: [0u8; 32], vault: addr("v"), witness: w }.encode();
    bytes[1224] = 0x03;
    ClaimInput::decode(&bytes);
}
