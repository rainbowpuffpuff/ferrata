//! The SPEC-v3 D3 checkpoint encodings (`checkpoint.rs`): the 17-word journal the v3 vault hashes and
//! the key-set hash that binds the posting timeframe. Layouts are restated here field by field from
//! the specification text, not taken from the crate.

mod common;

use common::{addr, h};
use ferrata_bridge_core::checkpoint::{
    key_set_hash, key_set_hash_v2, CheckpointJournal, CheckpointJournalV3, CHECKPOINT_DOMAIN_V3,
    CHECKPOINT_DOMAIN_V3_STR, CHECKPOINT_JOURNAL_V3_LEN, TAG_KEYSET_V2,
};
use ferrata_bridge_core::encoding::PREFIX_TAGS;
use ferrata_bridge_core::{chain_id_u256, sha256};
use proptest::prelude::*;

fn sample() -> CheckpointJournalV3 {
    CheckpointJournalV3 {
        chain_id: chain_id_u256(11_155_111),
        vault: addr("vault"),
        lez_config_id: h("config"),
        n_min: 3,
        t_min: 2,
        parent_state: h("parent"),
        new_state: h("new"),
        burn_root: h("root"),
        burn_count: 5,
        imported_leaf_count: 2,
        sunset: false,
        echo_nonce: h("nonce"),
        tip_id: 6,
        tip_hash: h("tip"),
        n: 4,
        t: 3,
    }
}

/// A big-endian integer right-aligned in a 32-byte word, as `abi.encode` writes static integers.
fn word(value: u64) -> [u8; 32] {
    let mut w = [0u8; 32];
    w[24..].copy_from_slice(&value.to_be_bytes());
    w
}

#[test]
fn the_domain_is_the_hash_of_its_string_and_the_vault_s_constant() {
    assert_eq!(CHECKPOINT_DOMAIN_V3_STR, "ferrata/vault/checkpoint/v3");
    assert_eq!(CHECKPOINT_DOMAIN_V3, sha256(CHECKPOINT_DOMAIN_V3_STR.as_bytes()));
    // FerrataVault.CHECKPOINT_DOMAIN (vault/src/FerrataVault.sol).
    assert_eq!(hex::encode(CHECKPOINT_DOMAIN_V3), "bdb3b52df781536a6bcce7088d156b3951a00a45ec8bcf2805ba8406fbeb5ec7");
}

#[test]
fn the_key_set_tag_is_exact_and_prefix_free_against_every_other_tag() {
    assert_eq!(TAG_KEYSET_V2, b"ferrata/keyset/v2");
    for other in PREFIX_TAGS {
        assert!(!other.starts_with(TAG_KEYSET_V2) && !TAG_KEYSET_V2.starts_with(other), "{other:?}");
    }
}

#[test]
fn the_key_set_hash_is_sha256_of_tag_count_timeframe_and_keys() {
    let keys = [h("k1"), h("k2"), h("k3"), h("k4")];
    let mut preimage = b"ferrata/keyset/v2".to_vec();
    preimage.extend_from_slice(&4u32.to_be_bytes());
    preimage.extend_from_slice(&900u32.to_be_bytes());
    for key in &keys {
        preimage.extend_from_slice(key);
    }
    assert_eq!(preimage.len(), 17 + 4 + 4 + 4 * 32);
    assert_eq!(key_set_hash_v2(&keys, 900), sha256(&preimage));
    // The turn length, the order and the count are bound; the SPEC-v2 hash is another value.
    assert_ne!(key_set_hash_v2(&keys, 900), key_set_hash_v2(&keys, 901));
    assert_ne!(key_set_hash_v2(&keys, 900), key_set_hash_v2(&[keys[1], keys[0], keys[2], keys[3]], 900));
    assert_ne!(key_set_hash_v2(&keys[..3], 900), key_set_hash_v2(&keys, 900));
    assert_ne!(key_set_hash_v2(&keys, 900), key_set_hash(&keys));
    assert_ne!(key_set_hash_v2(&[], 0), key_set_hash_v2(&[[0u8; 32]], 0));
}

#[test]
fn the_journal_is_its_seventeen_abi_words() {
    let j = sample();
    let mut vault_word = [0u8; 32];
    vault_word[12..].copy_from_slice(&j.vault);
    let want: Vec<u8> = [
        sha256(b"ferrata/vault/checkpoint/v3"),
        j.chain_id,
        vault_word,
        j.lez_config_id,
        word(3),
        word(2),
        j.parent_state,
        j.new_state,
        j.burn_root,
        word(5),
        word(2),
        word(0),
        j.echo_nonce,
        word(6),
        j.tip_hash,
        word(4),
        word(3),
    ]
    .concat();
    assert_eq!(CHECKPOINT_JOURNAL_V3_LEN, 544);
    assert_eq!(j.encode().to_vec(), want);
    assert_eq!(CheckpointJournalV3::decode(&want), Some(j));
    let sunset = CheckpointJournalV3 { sunset: true, ..j };
    assert_eq!(sunset.encode()[11 * 32..12 * 32], word(1));
    assert_eq!(CheckpointJournalV3::decode(&sunset.encode()), Some(sunset));
}

#[test]
fn words_1_to_12_are_the_v2_journal_s() {
    let j = sample();
    let v2 = CheckpointJournal {
        chain_id: j.chain_id,
        vault: j.vault,
        lez_config_id: j.lez_config_id,
        n_min: j.n_min,
        t_min: j.t_min,
        parent_state: j.parent_state,
        new_state: j.new_state,
        burn_root: j.burn_root,
        burn_count: j.burn_count,
        imported_leaf_count: j.imported_leaf_count,
        sunset: j.sunset,
        echo_nonce: j.echo_nonce,
    };
    assert_eq!(j.encode()[32..13 * 32], v2.encode()[32..]);
    assert_ne!(j.encode()[..32], v2.encode()[..32], "the domain word differs");
}

#[test]
fn decoding_rejects_everything_but_the_canonical_encoding() {
    let good = sample().encode();
    assert_eq!(CheckpointJournalV3::decode(&good[..543]), None);
    assert_eq!(CheckpointJournalV3::decode(&[good.as_slice(), &[0]].concat()), None);
    let mut other_domain = good;
    other_domain[0] ^= 1;
    assert_eq!(CheckpointJournalV3::decode(&other_domain), None);
    // A non-zero byte in the padding of every padded word: the address (12 bytes), the u32s (28),
    // the u64s (24) and the bool (31, and a value other than 0 or 1).
    for (index, padding) in
        [(2usize, 12usize), (4, 28), (5, 28), (9, 24), (10, 24), (11, 31), (13, 24), (15, 28), (16, 28)]
    {
        for at in [0, padding - 1] {
            let mut bad = good;
            bad[32 * index + at] = 1;
            assert_eq!(CheckpointJournalV3::decode(&bad), None, "word {index}, byte {at}");
        }
    }
    let mut two = good;
    two[11 * 32 + 31] = 2;
    assert_eq!(CheckpointJournalV3::decode(&two), None);
}

proptest! {
    #[test]
    fn encode_and_decode_round_trip(
        words in prop::collection::vec(any::<[u8; 32]>(), 7),
        vault in any::<[u8; 20]>(),
        small in (any::<u32>(), any::<u32>(), any::<u32>(), any::<u32>()),
        counts in (any::<u64>(), any::<u64>(), any::<u64>()),
        sunset in any::<bool>(),
    ) {
        let j = CheckpointJournalV3 {
            chain_id: words[0], vault, lez_config_id: words[1], n_min: small.0, t_min: small.1,
            parent_state: words[2], new_state: words[3], burn_root: words[4], burn_count: counts.0,
            imported_leaf_count: counts.1, sunset, echo_nonce: words[5], tip_id: counts.2, tip_hash: words[6],
            n: small.2, t: small.3,
        };
        prop_assert_eq!(CheckpointJournalV3::decode(&j.encode()), Some(j));
    }
}
