//! Byte layouts: every encoding is restated here field by field from the written spec and compared
//! with the library's, the hashed tags are checked against their strings and against the vault's
//! constants, and the codecs round-trip.

mod common;

use common::{addr, h};
use ferrata_bridge_core::encoding::*;
use ferrata_bridge_core::outbound::{ReleasePublic, ReleaseWitness};
use ferrata_bridge_core::sha256;
use ferrata_bridge_core::tree::Path;
use proptest::prelude::*;

fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

#[test]
fn hashed_tags_are_sha256_of_their_strings() {
    assert_eq!(LEAF_TAG, sha256(LEAF_TAG_STR.as_bytes()));
    assert_eq!(BURN_LEAF_TAG, sha256(BURN_LEAF_TAG_STR.as_bytes()));
    assert_eq!(RELEASE_DOMAIN, sha256(RELEASE_DOMAIN_STR.as_bytes()));
}

#[test]
fn hashed_tags_equal_the_vault_constants() {
    // FerrataVault.sol: LEAF_TAG and RELEASE_DOMAIN, copied verbatim.
    assert_eq!(hex::encode(LEAF_TAG), "084d5599858dde3bdadaec9a70979121276922b791de25071d145ac6b13c4286");
    assert_eq!(hex::encode(RELEASE_DOMAIN), "38d947b280ef2710d905d714dc6c55d9fdaa844e4769df08152f6bed4bc9f444");
}

#[test]
fn prefix_tags_are_exact_strings_and_prefix_free() {
    let expected: [&str; 10] = [
        "ferrata/s/v1",
        "ferrata/C/v1",
        "ferrata/deposit/v1",
        "ferrata/nf/v1",
        "ferrata/burn-tag/v1",
        "ferrata/s-out/v1",
        "ferrata/memo/v1",
        "ferrata/Cout/v1",
        "ferrata/burn/v1",
        "ferrata/nf-out/v1",
    ];
    for (tag, want) in PREFIX_TAGS.iter().zip(expected) {
        assert_eq!(*tag, want.as_bytes());
        assert!(tag[0] != LEAF_PREFIX && tag[0] != NODE_PREFIX);
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
    let (token, vault, depositor, recipient) = (addr("token"), addr("vault"), addr("depositor"), addr("recipient"));
    let (amount, c, s, dest, leaf, r) = (h("amount"), h("C"), h("s"), h("dest"), h("leaf"), h("r"));
    let (config, fid, left, right, burn_id) = (h("config"), h("fid"), h("left"), h("right"), h("burn id"));
    let (chain_id, j, i, idx) = (0x0102_0304_0506_0708u64, 7u64, 9u64, 0x1122_3344u64);

    let leaf_pre = cat(&[&[0x00], &LEAF_TAG, &idx.to_be_bytes(), &token, &amount, &c]);
    assert_eq!(leaf_pre.len(), 125);
    assert_eq!(deposit_leaf_preimage(idx, &token, &amount, &c).to_vec(), leaf_pre);
    let burn_pre = cat(&[&[0x00], &BURN_LEAF_TAG, &idx.to_be_bytes(), &token, &amount, &c]);
    assert_eq!(burn_leaf_preimage(idx, &token, &amount, &c).to_vec(), burn_pre);
    assert_eq!(node_preimage(&left, &right).to_vec(), cat(&[&[0x01], &left, &right]));

    let cases: Vec<(Vec<u8>, Vec<u8>, usize)> = vec![
        (
            deposit_secret_message(chain_id, &vault, &depositor, j).to_vec(),
            cat(&[b"ferrata/s/v1", &chain_id.to_be_bytes(), &vault, &depositor, &j.to_be_bytes()]),
            68,
        ),
        (commitment_preimage(&s, &dest).to_vec(), cat(&[b"ferrata/C/v1", &s, &dest]), 76),
        (
            deposit_id_preimage(chain_id, &vault, idx, &leaf).to_vec(),
            cat(&[b"ferrata/deposit/v1", &chain_id.to_be_bytes(), &vault, &idx.to_be_bytes(), &leaf]),
            86,
        ),
        (nullifier_preimage(&s, &fid).to_vec(), cat(&[b"ferrata/nf/v1", &s, &fid]), 77),
        (burn_tag_message(&config, i).to_vec(), cat(&[b"ferrata/burn-tag/v1", &config, &i.to_be_bytes()]), 59),
        (burn_secret_message(&r).to_vec(), cat(&[b"ferrata/s-out/v1", &r]), 48),
        (memo_pad_message(&r).to_vec(), cat(&[b"ferrata/memo/v1", &r]), 47),
        (
            burn_commitment_preimage(&s, chain_id, &vault, &recipient).to_vec(),
            cat(&[b"ferrata/Cout/v1", &s, &chain_id.to_be_bytes(), &vault, &recipient]),
            95,
        ),
        (
            burn_id_preimage(&config, idx, &leaf).to_vec(),
            cat(&[b"ferrata/burn/v1", &config, &idx.to_be_bytes(), &leaf]),
            87,
        ),
        (burn_nullifier_preimage(&s, &burn_id).to_vec(), cat(&[b"ferrata/nf-out/v1", &s, &burn_id]), 81),
    ];
    for (got, want, len) in cases {
        assert_eq!(got, want);
        assert_eq!(got.len(), len);
    }
}

fn word_u64(v: u64) -> Vec<u8> {
    let mut w = vec![0u8; 24];
    w.extend_from_slice(&v.to_be_bytes());
    w
}

fn word_addr(a: &Address) -> Vec<u8> {
    let mut w = vec![0u8; 12];
    w.extend_from_slice(a);
    w
}

fn journal_from_parts(j: &ReleaseJournal) -> Vec<u8> {
    cat(&[
        &sha256(b"ferrata/vault/release/v1"),
        &word_u64(j.chain_id),
        &word_addr(&j.vault),
        &j.lez_config_id,
        &j.burn_root,
        &j.nullifier,
        &word_addr(&j.token),
        &j.amount,
        &word_addr(&j.recipient),
        &word_addr(&j.fee_recipient),
        &j.relay_fee,
    ])
}

fn journal_strategy() -> impl Strategy<Value = ReleaseJournal> {
    (
        (any::<u64>(), any::<[u8; 20]>(), any::<[u8; 32]>(), any::<[u8; 32]>(), any::<[u8; 32]>()),
        (any::<[u8; 20]>(), any::<[u8; 32]>(), any::<[u8; 20]>(), any::<[u8; 20]>(), any::<[u8; 32]>()),
    )
        .prop_map(
            |(
                (chain_id, vault, lez_config_id, burn_root, nullifier),
                (token, amount, recipient, fee_recipient, relay_fee),
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
                    fee_recipient,
                    relay_fee,
                }
            },
        )
}

fn path_strategy() -> impl Strategy<Value = Path> {
    prop::collection::vec(any::<[u8; 32]>(), 32).prop_map(|v| v.try_into().unwrap())
}

proptest! {
    #[test]
    fn release_journal_is_eleven_abi_words(j in journal_strategy()) {
        let bytes = j.encode();
        prop_assert_eq!(bytes.len(), RELEASE_JOURNAL_LEN);
        prop_assert_eq!(bytes.to_vec(), journal_from_parts(&j));
        prop_assert_eq!(ReleaseJournal::decode(&bytes), Some(j));
    }

    #[test]
    fn release_journal_decode_rejects_non_canonical_bytes(j in journal_strategy(), word in 0usize..11, at in 0usize..32, mask in 1u8..=255) {
        let mut bytes = j.encode();
        let pos = word * 32 + at;
        bytes[pos] ^= mask;
        // Canonical iff the flipped byte was data, not padding or the domain.
        let padding = (matches!(word, 2 | 6 | 8 | 9) && at < 12) || (word == 1 && at < 24);
        let decoded = ReleaseJournal::decode(&bytes);
        if word == 0 || padding {
            prop_assert_eq!(decoded, None);
        } else {
            prop_assert!(decoded.is_some());
            prop_assert_ne!(decoded, Some(j));
        }
        prop_assert_eq!(ReleaseJournal::decode(&bytes[..351]), None);
    }

    #[test]
    fn release_input_round_trips(burn_root in any::<[u8; 32]>(), leaf_index in any::<u64>(), path in path_strategy(),
                                 s_out in any::<[u8; 32]>(), j in journal_strategy()) {
        let input = ReleaseInput {
            burn_root,
            witness: ReleaseWitness { leaf_index, path, s_out },
            public: ReleasePublic {
                chain_id: j.chain_id, vault: j.vault, lez_config_id: j.lez_config_id, token: j.token,
                amount: j.amount, recipient: j.recipient, fee_recipient: j.fee_recipient, relay_fee: j.relay_fee,
            },
        };
        let bytes = input.encode();
        prop_assert_eq!(&bytes[..32], &burn_root[..]);
        prop_assert_eq!(&bytes[1064..1096], &s_out[..]);
        prop_assert_eq!(&bytes[1248..], &j.relay_fee[..]);
        prop_assert_eq!(ReleaseInput::decode(&bytes), input);
    }

    #[test]
    fn claim_input_round_trips(root in any::<[u8; 32]>(), chain_id in any::<u64>(), vault in any::<[u8; 20]>(),
                               token in any::<[u8; 20]>(), amount in any::<[u8; 32]>(), leaf_index in any::<u64>(),
                               path in path_strategy(), s in any::<[u8; 32]>(), dest in any::<[u8; 32]>()) {
        let input = ClaimInput {
            root, chain_id, vault,
            witness: ferrata_bridge_core::inbound::ClaimWitness { token, amount, leaf_index, path, s, dest },
        };
        let bytes = input.encode();
        prop_assert_eq!(&bytes[1176..], &dest[..]);
        prop_assert_eq!(ClaimInput::decode(&bytes), input);
    }
}
