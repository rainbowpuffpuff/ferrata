//! What the guests commit.
//!
//! The checkpoint journal is SPEC-v3 D3 item 5 (which replaces SPEC-v2 §4.2 item 5), ABI-encoded
//! by hand (seventeen static words):
//!
//! | word | field | Solidity type | layout |
//! |---|---|---|---|
//! | 0 | `CHECKPOINT_DOMAIN` = `sha256("ferrata/vault/checkpoint/v3")` | bytes32 | as is |
//! | 1 | `evmChainId` | uint256 | as is (big-endian) |
//! | 2 | `vault` | address | 12 zero bytes ‖ 20 bytes |
//! | 3 | `lezConfigId` | bytes32 | as is |
//! | 4, 5 | `nMin`, `tMin` | uint32 | 28 zero bytes ‖ u32 BE |
//! | 6, 7 | `parentState`, `newState` | bytes32 | as is |
//! | 8 | `burnRoot` | bytes32 | as is |
//! | 9, 10 | `burnCount`, `importedLeafCount` | uint64 | 24 zero bytes ‖ u64 BE |
//! | 11 | `sunset` | bool | 31 zero bytes ‖ 0x00/0x01 |
//! | 12 | `echoNonce` | bytes32 | as is |
//! | 13 | `tipId` (`T`'s block id) | uint64 | 24 zero bytes ‖ u64 BE |
//! | 14 | `tipHash` (`T`'s hash) | bytes32 | as is |
//! | 15, 16 | `n`, `t` (`|K|` and `t(n)` of the key set in `newState`) | uint32 | 28 zero bytes ‖ u32 BE |
//!
//! Words 1–12 are laid out exactly as in SPEC-v2's 13-word journal.
//!
//! The segment journal is this crate's own (composition, `rule::run_segment`); no contract reads
//! it, only the checkpoint guest, through `env::verify`.

use crate::hash::{CHECKPOINT_DOMAIN, TAG_SEGMENT};
use crate::reader::Reader;
use crate::{Address, Error, Hash};

pub const CHECKPOINT_JOURNAL_LEN: usize = 17 * 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckpointJournal {
    pub evm_chain_id: Hash,
    pub vault: Address,
    pub lez_config_id: Hash,
    pub n_min: u32,
    pub t_min: u32,
    pub parent_state: Hash,
    pub new_state: Hash,
    pub burn_root: Hash,
    pub burn_count: u64,
    pub imported_leaf_count: u64,
    pub sunset: bool,
    pub echo_nonce: Hash,
    /// `T`'s block id and hash.
    pub tip_id: u64,
    pub tip_hash: Hash,
    /// Size and threshold of the key set in `newState`.
    pub n: u32,
    pub t: u32,
}

fn word_u64(x: u64) -> Hash {
    let mut w = [0u8; 32];
    w[24..].copy_from_slice(&x.to_be_bytes());
    w
}

fn word_address(a: &Address) -> Hash {
    let mut w = [0u8; 32];
    w[12..].copy_from_slice(a);
    w
}

impl CheckpointJournal {
    pub fn encode(&self) -> [u8; CHECKPOINT_JOURNAL_LEN] {
        let words: [Hash; 17] = [
            CHECKPOINT_DOMAIN,
            self.evm_chain_id,
            word_address(&self.vault),
            self.lez_config_id,
            word_u64(u64::from(self.n_min)),
            word_u64(u64::from(self.t_min)),
            self.parent_state,
            self.new_state,
            self.burn_root,
            word_u64(self.burn_count),
            word_u64(self.imported_leaf_count),
            word_u64(u64::from(self.sunset)),
            self.echo_nonce,
            word_u64(self.tip_id),
            self.tip_hash,
            word_u64(u64::from(self.n)),
            word_u64(u64::from(self.t)),
        ];
        let mut out = [0u8; CHECKPOINT_JOURNAL_LEN];
        for (i, w) in words.iter().enumerate() {
            out[32 * i..32 * (i + 1)].copy_from_slice(w);
        }
        out
    }
}

/// `"ferrata/checkpoint-segment/v1"(29) ‖ lezConfigId ‖ startState ‖ endState`.
pub const SEGMENT_JOURNAL_LEN: usize = 29 + 3 * 32;

/// A segment proves `startState → endState` by hashing and extracting a bounded run of blocks,
/// with no support check; only a checkpoint guest that verifies it can turn it into a journal
/// the vault accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegmentJournal {
    pub lez_config_id: Hash,
    pub start_state: Hash,
    pub end_state: Hash,
}

impl SegmentJournal {
    pub fn encode(&self) -> [u8; SEGMENT_JOURNAL_LEN] {
        let mut out = [0u8; SEGMENT_JOURNAL_LEN];
        out[..29].copy_from_slice(TAG_SEGMENT);
        out[29..61].copy_from_slice(&self.lez_config_id);
        out[61..93].copy_from_slice(&self.start_state);
        out[93..].copy_from_slice(&self.end_state);
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(bytes);
        if r.take(29)? != TAG_SEGMENT {
            return Err(Error::SegmentConfigMismatch);
        }
        let j = Self { lez_config_id: *r.array()?, start_state: *r.array()?, end_state: *r.array()? };
        r.finish()?;
        Ok(j)
    }
}
