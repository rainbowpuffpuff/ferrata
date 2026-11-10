//! The SPEC-v3 D3 checkpoint encodings, as the wallet side needs them to recompute what the
//! checkpoint program commits: the 17-word journal the v3 vault hashes and the key-set hash that
//! binds the channel's posting timeframe. Written from the specification text, independently of
//! `checkpoint/crates/checkpoint-core`.
//!
//! They live in this host-only crate and not in `ferrata-bridge-core`, whose checkpoint encodings
//! stay at SPEC-v2 (13 words, `ferrata/keyset/v1`): the guests are built from that crate, and any
//! change to its sources changes the release guest's program binary and so its image id
//! (measured: e2e/README.md).

use ferrata_bridge_core::encoding::{address_word, u64_word};
use ferrata_bridge_core::{sha256, Address, Hash, U256};

/// String whose SHA-256 is [`CHECKPOINT_DOMAIN_V3`].
#[cfg(test)]
pub const CHECKPOINT_DOMAIN_V3_STR: &str = "ferrata/vault/checkpoint/v3";
/// `sha256("ferrata/vault/checkpoint/v3")`, the first word of the SPEC-v3 D3 journal (tested).
pub const CHECKPOINT_DOMAIN_V3: Hash = [
    0xbd, 0xb3, 0xb5, 0x2d, 0xf7, 0x81, 0x53, 0x6a, 0x6b, 0xcc, 0xe7, 0x08, 0x8d, 0x15, 0x6b, 0x39, 0x51, 0xa0, 0x0a,
    0x45, 0xec, 0x8b, 0xcf, 0x28, 0x05, 0xba, 0x84, 0x06, 0xfb, 0xeb, 0x5e, 0xc7,
];
/// Prefix of the preimage of the SPEC-v3 D3 `keySetHash` ([`key_set_hash_v2`]).
pub const TAG_KEYSET_V2: &[u8] = b"ferrata/keyset/v2";

/// `keySetHash` as SPEC-v3 D3 defines it: `sha256("ferrata/keyset/v2"(17) ‖ n:u32(4) ‖
/// postingTimeframe:u32(4) ‖ key_1(32) ‖ … ‖ key_n(32))`, keys in the channel's `accredited_keys`
/// order. It binds the channel's turn length (slots per turn), which the vault's wait relies on.
/// Panics if there are more than `u32::MAX` keys.
pub fn key_set_hash_v2(keys: &[Hash], posting_timeframe: u32) -> Hash {
    let n = u32::try_from(keys.len()).expect("at most u32::MAX keys");
    let mut preimage = Vec::with_capacity(TAG_KEYSET_V2.len() + 8 + 32 * keys.len());
    preimage.extend_from_slice(TAG_KEYSET_V2);
    preimage.extend_from_slice(&n.to_be_bytes());
    preimage.extend_from_slice(&posting_timeframe.to_be_bytes());
    for key in keys {
        preimage.extend_from_slice(key);
    }
    sha256(&preimage)
}

/// Length of the SPEC-v3 D3 checkpoint journal: seventeen ABI words.
pub const CHECKPOINT_JOURNAL_V3_LEN: usize = 17 * 32;

/// The checkpoint program's journal as SPEC-v3 D3 defines it: `abi.encode(CHECKPOINT_DOMAIN,
/// evmChainId, vault, lezConfigId, nMin, tMin, parentState, newState, burnRoot, burnCount,
/// importedLeafCount, sunset, echoNonce, tipId, tipHash, n, t)`, seventeen 32-byte words. Words 1-12
/// are laid out as in `ferrata_bridge_core::checkpoint::CheckpointJournal`; word 0 is [`CHECKPOINT_DOMAIN_V3`]:
///
/// | word | field | word layout |
/// |---|---|---|
/// | 0 | [`CHECKPOINT_DOMAIN_V3`] | as is |
/// | 1 | `evmChainId` | uint256 |
/// | 2 | `vault` | 12 zero bytes ‖ 20 |
/// | 3 | `lezConfigId` | as is |
/// | 4, 5 | `nMin`, `tMin` | 28 zero bytes ‖ u32 |
/// | 6, 7 | `parentState`, `newState` | as is |
/// | 8 | `burnRoot` | as is |
/// | 9, 10 | `burnCount`, `importedLeafCount` | 24 zero bytes ‖ u64 |
/// | 11 | `sunset` | 31 zero bytes ‖ 0x00 or 0x01 |
/// | 12 | `echoNonce` | as is |
/// | 13 | `tipId` | 24 zero bytes ‖ u64 |
/// | 14 | `tipHash` | as is |
/// | 15, 16 | `n`, `t` | 28 zero bytes ‖ u32 |
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckpointJournalV3 {
    /// The vault's chain id.
    pub chain_id: U256,
    /// The vault.
    pub vault: Address,
    /// The vault's `lezConfigId` (= `configId`).
    pub lez_config_id: Hash,
    /// Minimum accredited key-set size (deployment constant).
    pub n_min: u32,
    /// Minimum threshold (deployment constant).
    pub t_min: u32,
    /// `S` the checkpoint starts from.
    pub parent_state: Hash,
    /// `S` at the new tip `T`.
    pub new_state: Hash,
    /// Root of the burn tree at `T`.
    pub burn_root: Hash,
    /// Burns in the tree at `T`.
    pub burn_count: u64,
    /// `importedLeafCount` at `T`.
    pub imported_leaf_count: u64,
    /// `sunset` at `T`.
    pub sunset: bool,
    /// 0 for a candidate; the beacon nonce for a finalisation.
    pub echo_nonce: Hash,
    /// LEZ block id of `T`.
    pub tip_id: u64,
    /// Hash of `T`.
    pub tip_hash: Hash,
    /// Size of the key set in `newState`.
    pub n: u32,
    /// Its threshold, `t(n)`.
    pub t: u32,
}

impl CheckpointJournalV3 {
    /// The seventeen words, in order.
    fn words(&self) -> [[u8; 32]; 17] {
        [
            CHECKPOINT_DOMAIN_V3,
            self.chain_id,
            address_word(&self.vault),
            self.lez_config_id,
            u64_word(u64::from(self.n_min)),
            u64_word(u64::from(self.t_min)),
            self.parent_state,
            self.new_state,
            self.burn_root,
            u64_word(self.burn_count),
            u64_word(self.imported_leaf_count),
            u64_word(u64::from(self.sunset)),
            self.echo_nonce,
            u64_word(self.tip_id),
            self.tip_hash,
            u64_word(u64::from(self.n)),
            u64_word(u64::from(self.t)),
        ]
    }

    /// The 544 journal bytes.
    pub fn encode(&self) -> [u8; CHECKPOINT_JOURNAL_V3_LEN] {
        let mut out = [0u8; CHECKPOINT_JOURNAL_V3_LEN];
        for (chunk, word) in out.chunks_exact_mut(32).zip(self.words()) {
            chunk.copy_from_slice(&word);
        }
        out
    }

    /// Parses journal bytes; `None` unless they are exactly what [`Self::encode`] produces.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let bytes: &[u8; CHECKPOINT_JOURNAL_V3_LEN] = bytes.try_into().ok()?;
        let word = |i: usize| -> [u8; 32] { bytes[32 * i..32 * (i + 1)].try_into().expect("32 bytes") };
        let small = |i: usize| -> Option<u64> {
            let w = word(i);
            if w[..24].iter().any(|b| *b != 0) {
                return None;
            }
            Some(u64::from_be_bytes(w[24..].try_into().expect("8 bytes")))
        };
        let j = Self {
            chain_id: word(1),
            vault: {
                let w = word(2);
                if w[..12].iter().any(|b| *b != 0) {
                    return None;
                }
                w[12..].try_into().ok()?
            },
            lez_config_id: word(3),
            n_min: u32::try_from(small(4)?).ok()?,
            t_min: u32::try_from(small(5)?).ok()?,
            parent_state: word(6),
            new_state: word(7),
            burn_root: word(8),
            burn_count: small(9)?,
            imported_leaf_count: small(10)?,
            sunset: match small(11)? {
                0 => false,
                1 => true,
                _ => return None,
            },
            echo_nonce: word(12),
            tip_id: small(13)?,
            tip_hash: word(14),
            n: u32::try_from(small(15)?).ok()?,
            t: u32::try_from(small(16)?).ok()?,
        };
        (word(0) == CHECKPOINT_DOMAIN_V3).then_some(j)
    }
}

#[cfg(test)]
mod tests {
    use ferrata_bridge_core::chain_id_u256;
    use ferrata_bridge_core::checkpoint::{key_set_hash, CheckpointJournal};
    use ferrata_bridge_core::encoding::PREFIX_TAGS;

    use super::*;

    fn h(label: &str) -> Hash {
        sha256(label.as_bytes())
    }

    fn sample() -> CheckpointJournalV3 {
        CheckpointJournalV3 {
            chain_id: chain_id_u256(11_155_111),
            vault: h("vault")[..20].try_into().unwrap(),
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
        assert_eq!(
            hex::encode(CHECKPOINT_DOMAIN_V3),
            "bdb3b52df781536a6bcce7088d156b3951a00a45ec8bcf2805ba8406fbeb5ec7"
        );
    }

    #[test]
    fn the_key_set_tag_is_exact_and_prefix_free_against_the_core_s_tags() {
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
        // A non-zero byte in the padding of every padded word: the address (12 bytes), the u32s
        // (28), the u64s (24) and the bool (31, and a value other than 0 or 1).
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
}
