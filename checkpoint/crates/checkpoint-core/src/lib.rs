//! The Ferrata LEZ → Ethereum checkpoint rule (SPEC-v2 §4.1–§4.2, burn leaf of §5.1), written to
//! run unchanged on the host and inside a RISC Zero guest.
//!
//! - [`lez`]: strict parser for LEZ v0.4.0-rc1 `borsh(Block)` and the block hash.
//! - [`wire`]: the bridge program's messages (defined here; the program does not exist yet).
//! - [`mantle`]: Logos Blockchain transactions at LEZ's pinned revision: framing, the BLAKE2b
//!   transaction hash and `MsgId`, inscription checks, `ChannelConfigOp`.
//! - [`tree`], [`state`], [`journal`]: the burn tree, the state hash `S`, the ABI journal.
//! - [`input`]: the guests' input format.
//! - [`rule`]: [`rule::run_checkpoint`] (finality rule (A)) and [`rule::run_segment`].
//! - [`rule_b`]: [`rule_b::run_checkpoint_b`], finality rule (B): final-tier attestations by
//!   certified attestation keys ([`attestor_core`]).
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod error;
pub mod hash;
pub mod input;
pub mod journal;
pub mod lez;
pub mod mantle;
pub mod reader;
pub mod rule;
pub mod rule_b;
pub mod state;
pub mod tree;
pub mod wire;

pub use error::Error;

/// A 32-byte hash, key, id or `uint256`/`bytes32` word.
pub type Hash = [u8; 32];
/// A 20-byte Ethereum address (or other 20-byte field).
pub type Address = [u8; 20];

/// Logos Blockchain 0.3.5-rc.1's transaction layout (a fork digest before the operations) and the two rules over it:
/// the second build, cargo feature `bedrock-0-3-5`, off by default (`bedrock_0_3_5/mod.rs`).
#[cfg(feature = "bedrock-0-3-5")]
pub mod bedrock_0_3_5;
