//! SHA-256 and HMAC-SHA256 (RFC 2104), from RustCrypto's `sha2` and `hmac`.
//!
//! Inside a RISC Zero guest, `sha2` is replaced by RISC Zero's accelerated fork through the
//! guest workspace's `[patch.crates-io]`; nothing here changes.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use crate::Hash;

/// SHA-256 of `data`.
pub fn sha256(data: &[u8]) -> Hash {
    Sha256::digest(data).into()
}

/// The 32-byte bridge key `k`, held as an HMAC-SHA256 state already keyed with it, so that each
/// PRF call hashes only the message (the two key blocks are absorbed once, in [`BridgeKey::new`]).
///
/// It has no `Debug` implementation, so it cannot end up in a log by accident.
#[derive(Clone)]
pub struct BridgeKey(Hmac<Sha256>);

impl BridgeKey {
    /// Keys HMAC-SHA256 with `k`.
    pub fn new(k: &Hash) -> Self {
        // A 32-byte key is shorter than SHA-256's 64-byte block, so HMAC zero-pads it.
        Self(Hmac::<Sha256>::new_from_slice(k).expect("HMAC accepts keys of any length"))
    }

    /// `HMAC-SHA256(k, message)`.
    pub fn prf(&self, message: &[u8]) -> Hash {
        let mut mac = self.0.clone();
        mac.update(message);
        mac.finalize().into_bytes().into()
    }
}
