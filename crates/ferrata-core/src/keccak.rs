//! Keccak-256, the execution layer's hash.
//!
//! Every execution-layer hash in this crate goes through [`keccak256`], so a guest can swap the
//! implementation in one place: RISC Zero ships an accelerated fork of `tiny-keccak` that a guest
//! patches in with `[patch.crates-io]`.

use tiny_keccak::{Hasher, Keccak};

use crate::Root;

/// `KEC(data)`: Keccak-256 with the original Keccak padding (not FIPS-202 SHA3-256).
pub fn keccak256(data: &[u8]) -> Root {
    let mut hasher = Keccak::v256();
    hasher.update(data);
    let mut out = [0u8; 32];
    hasher.finalize(&mut out);
    out
}
