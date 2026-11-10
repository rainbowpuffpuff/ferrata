//! LEZ -> Ethereum checkpoint encodings: the state hash `S`, `burnFrontierHash`, the threshold `t(n)`,
//! and both generations of `keySetHash` and of the journal: SPEC-v2 §4.1-§4.2 ([`CheckpointJournal`])
//! and SPEC-v3 D3 ([`CheckpointJournalV3`], below). The checkpoint program itself is in `checkpoint/`.

use crate::encoding::{address_word, burn_frontier_preimage, lez_state_preimage, u64_word};
pub use crate::encoding::{key_set_hash, CheckpointJournal, LezState};
use crate::tree::IncrementalTree;
use crate::{sha256, Address, Hash, U256};

impl LezState {
    /// `S = sha256(lez_state_preimage(self))`.
    pub fn hash(&self) -> Hash {
        sha256(&lez_state_preimage(self))
    }
}

/// `burnFrontierHash` of a burn tree: SHA-256 of its canonical frontier (layout and the choice of
/// canonical form: [`crate::encoding::burn_frontier_preimage`]).
pub fn burn_frontier_hash(tree: &IncrementalTree) -> Hash {
    sha256(&burn_frontier_preimage(&tree.frontier()))
}

/// LEZ's signing threshold for `n` accredited keys: `t(n) = max(1, min(⌈2n/3⌉, n − 1))` (§4.2).
pub fn threshold(n: u32) -> u32 {
    let two_thirds_up = (2 * u64::from(n)).div_ceil(3) as u32;
    two_thirds_up.min(n.saturating_sub(1)).max(1)
}

// ------------------------------------------------------------------------------------------------
// SPEC-v3 D3: what the v3 vault and the checkpoint program (`checkpoint/`) use. The SPEC-v2 layouts
// above and in `encoding.rs` stay for the v2 vault that `evm-verify/` pins. These items are added
// here, and nothing in `encoding.rs` moves, so that the guests' program binaries, which embed that
// file's line numbers in their panic locations, stay byte for byte what they were.
// ------------------------------------------------------------------------------------------------

/// String whose SHA-256 is [`CHECKPOINT_DOMAIN_V3`].
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
    use sha2::{Digest, Sha256};
    let n = u32::try_from(keys.len()).expect("at most u32::MAX keys");
    let mut h = Sha256::new();
    h.update(TAG_KEYSET_V2);
    h.update(n.to_be_bytes());
    h.update(posting_timeframe.to_be_bytes());
    for key in keys {
        h.update(key);
    }
    h.finalize().into()
}

/// Length of the SPEC-v3 D3 checkpoint journal: seventeen ABI words.
pub const CHECKPOINT_JOURNAL_V3_LEN: usize = 17 * 32;

/// The checkpoint program's journal as SPEC-v3 D3 defines it: `abi.encode(CHECKPOINT_DOMAIN,
/// evmChainId, vault, lezConfigId, nMin, tMin, parentState, newState, burnRoot, burnCount,
/// importedLeafCount, sunset, echoNonce, tipId, tipHash, n, t)`, seventeen 32-byte words. Words 1-12
/// are laid out as in [`CheckpointJournal`]; word 0 is [`CHECKPOINT_DOMAIN_V3`]:
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
