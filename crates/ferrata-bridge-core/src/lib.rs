//! Ferrata bridge core: the commitment scheme of a privacy-preserving lock-and-mint bridge
//! between an Ethereum vault (`FerrataVault.sol`) and a LEZ bridge program.
//!
//! Prototype, unaudited. Every byte layout is defined once, in [`encoding`]; the other modules
//! only combine those encodings with SHA-256 and HMAC-SHA256.
//!
//! * [`tree`]: the depth-32 SHA-256 Merkle tree shared by the vault's deposit tree and the LEZ
//!   burn tree (incremental append, root, path verification; path creation with `std`).
//! * [`inbound`]: deposit -> mint. Deposit secret `s`, commitment `C`, deposit id `fid`,
//!   nullifier `N`, the claim statement and the wallet's recovery scan.
//! * [`outbound`]: burn -> release. Burn tag `r`, release secret `s_out`, memo, `C_out`, burn
//!   leaf, nullifier `N_out`, the release statement, the vault's release journal and the
//!   wallet's burn recovery scan.
//!
//! The crate is `no_std` without the default `std` feature and contains no `unsafe` code.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod encoding;
pub mod inbound;
pub mod outbound;
pub mod tree;

mod hash;

pub use encoding::{Address, Hash, U256};
pub use hash::{sha256, BridgeKey};

/// Why a claim or release witness was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerifyError {
    /// The leaf index is not below 2^32, the capacity of a depth-32 tree.
    LeafIndexOutOfRange,
    /// The leaf recomputed from the witness, hashed up its Merkle path, does not give the root.
    RootMismatch,
}
