//! The bridge program's wire messages, as far as checkpoints read them.
//!
//! The bridge program does not exist yet, so this module *defines* the format: a borsh enum,
//! encoded by hand here (no_std, no derive) and checked against `#[derive(BorshSerialize)]` in the
//! tests. The bridge program must decode with exactly this code (it is no_std for that reason):
//! the checkpoint extracts what this decoder accepts, and the program must act on exactly that.
//!
//! ```text
//! enum BridgeMessage {                          // borsh: u8 variant index, then the fields
//!     Burn { config_id: [u8; 32], r: [u8; 32], memo: [u8; 20], mac: [u8; 16],
//!            token: [u8; 20], amount: [u8; 32] /* u256 big-endian */, c_out: [u8; 32] },  // 0
//!     ImportRoot { config_id: [u8; 32], record_slot: u64, root: [u8; 32], leaf_count: u64 }, // 1
//!     Sunset { config_id: [u8; 32] },                                                      // 2
//!     Echo { nonce: [u8; 32] },                                                            // 3
//! }
//! ```
//! A real bridge program has more messages (claims, `Init`, fees, admin); they take variant
//! indices from 4 up, and checkpoints ignore them.

use alloc::vec::Vec;

use crate::reader::Reader;
use crate::{Address, Error, Hash};

/// A burn note as published on LEZ (SPEC-v2 §5.1), with the configuration it is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Burn {
    pub config_id: Hash,
    pub r: Hash,
    pub memo: Address,
    pub mac: [u8; 16],
    pub token: Address,
    /// `uint256`, big-endian (as in the burn leaf and on Ethereum).
    pub amount: Hash,
    pub c_out: Hash,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportRoot {
    pub config_id: Hash,
    pub record_slot: u64,
    pub root: Hash,
    pub leaf_count: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BridgeMessage {
    Burn(Burn),
    ImportRoot(ImportRoot),
    Sunset { config_id: Hash },
    Echo { nonce: Hash },
}

pub const TAG_BURN: u8 = 0;
pub const TAG_IMPORT_ROOT: u8 = 1;
pub const TAG_SUNSET: u8 = 2;
pub const TAG_ECHO: u8 = 3;

/// Encoded length of a `Burn` message: tag + 32 + 32 + 20 + 16 + 20 + 32 + 32.
pub const BURN_LEN: usize = 185;

impl BridgeMessage {
    /// Decodes a whole message. `Ok(None)` for a well-formed message of a variant checkpoints do
    /// not read (index 4 and up); `Err` for bytes that are not exactly one message of a known
    /// variant.
    pub fn decode(bytes: &[u8]) -> Result<Option<Self>, Error> {
        let mut r = Reader::new(bytes);
        let msg = match r.u8()? {
            TAG_BURN => Self::Burn(Burn {
                config_id: *r.array()?,
                r: *r.array()?,
                memo: *r.array()?,
                mac: *r.array()?,
                token: *r.array()?,
                amount: *r.array()?,
                c_out: *r.array()?,
            }),
            TAG_IMPORT_ROOT => Self::ImportRoot(ImportRoot {
                config_id: *r.array()?,
                record_slot: r.u64_le()?,
                root: *r.array()?,
                leaf_count: r.u64_le()?,
            }),
            TAG_SUNSET => Self::Sunset { config_id: *r.array()? },
            TAG_ECHO => Self::Echo { nonce: *r.array()? },
            _ => return Ok(None),
        };
        r.finish()?;
        Ok(Some(msg))
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(BURN_LEN);
        match self {
            Self::Burn(b) => {
                out.push(TAG_BURN);
                for part in [&b.config_id[..], &b.r, &b.memo, &b.mac, &b.token, &b.amount, &b.c_out] {
                    out.extend_from_slice(part);
                }
            }
            Self::ImportRoot(m) => {
                out.push(TAG_IMPORT_ROOT);
                out.extend_from_slice(&m.config_id);
                out.extend_from_slice(&m.record_slot.to_le_bytes());
                out.extend_from_slice(&m.root);
                out.extend_from_slice(&m.leaf_count.to_le_bytes());
            }
            Self::Sunset { config_id } => {
                out.push(TAG_SUNSET);
                out.extend_from_slice(config_id);
            }
            Self::Echo { nonce } => {
                out.push(TAG_ECHO);
                out.extend_from_slice(nonce);
            }
        }
        out
    }

    /// The configuration a message is for; `None` for `Echo`, which is configuration-free.
    pub const fn config_id(&self) -> Option<&Hash> {
        match self {
            Self::Burn(b) => Some(&b.config_id),
            Self::ImportRoot(m) => Some(&m.config_id),
            Self::Sunset { config_id } => Some(config_id),
            Self::Echo { .. } => None,
        }
    }
}
