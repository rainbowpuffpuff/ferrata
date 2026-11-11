//! Ferrata bridge core, specification v2 (`notes/design/SPEC-v2.md`): the commitment scheme of a
//! privacy-preserving lock-and-mint bridge between an Ethereum vault (`FerrataVault.sol`) and a LEZ
//! bridge program.
//!
//! Prototype, unaudited. Every byte layout is defined once, in [`encoding`]; the other modules only
//! combine those encodings with SHA-256 and HMAC-SHA256.
//!
//! * [`hash`] (re-exported): SHA-256, HMAC, the wallet keys `kAuth` / `kView`.
//! * [`tree`]: the depth-32 SHA-256 tree of both the deposit and the burn tree (incremental append,
//!   canonical frontier, path verification with direction bits; path creation with `std`).
//! * [`inbound`]: `s`, `C` (with the destination kind), deposit leaf, `fid`, `N`, the claim
//!   statement, the nullifier registry, wallet recovery over candidate destinations.
//! * [`gasdrop`]: the gas drop's receipt, the claim statement with a journal binding
//!   `(N, oneTimeAddress, relayer)` (v3 D13).
//! * [`outbound`]: `r_i`, `sOut`, memo, MAC, `COut`, burn leaf, `burnId`, `NOut`, the release
//!   statement and journal, the burn-tag registry, burn recovery.
//! * [`checkpoint`]: the LEZ checkpoint state `S`, `keySetHash`, `burnFrontierHash`, `t(n)`, the
//!   checkpoint journal.
//!
//! The crate is `no_std` without the default `std` feature and contains no `unsafe` code.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod checkpoint;
pub mod encoding;
pub mod gasdrop;
pub mod inbound;
pub mod outbound;
pub mod tree;

mod hash;

pub use encoding::{Address, Hash, Mac, U256};
pub use hash::{derive_keys, hmac_sha256, sha256, BridgeKey, BridgeKeys};

/// Why a claim or release witness was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerifyError {
    /// The path's direction bits differ from the leaf index the leaf commits to (this includes
    /// every index ≥ 2^32).
    DirectionsMismatch,
    /// The leaf recomputed from the witness, hashed up its Merkle path, does not give the root.
    RootMismatch,
}

/// A write-once set, as the LEZ bridge program keeps them: one account per inbound nullifier and
/// one per burn tag. `insert_new` returns `false` (and changes nothing) if the key is present.
pub trait WriteOnceSet<K> {
    /// Inserts `key` if absent; `false` if it was already there.
    fn insert_new(&mut self, key: K) -> bool;
}

#[cfg(feature = "std")]
impl<K: Eq + core::hash::Hash> WriteOnceSet<K> for std::collections::HashSet<K> {
    fn insert_new(&mut self, key: K) -> bool {
        self.insert(key)
    }
}

/// `evmChainId` as the `uint256` every v2 preimage carries.
pub fn chain_id_u256(chain_id: u64) -> U256 {
    encoding::u64_word(chain_id)
}
