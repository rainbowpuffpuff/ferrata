//! SHA-256, the only hash the consensus layer uses.

use sha2::{Digest, Sha256};

use crate::Root;

/// `hash(data)`.
pub fn sha256(data: &[u8]) -> Root {
    Sha256::digest(data).into()
}

/// `hash(left + right)`, the Merkle node function.
pub fn hash_pair(left: &Root, right: &Root) -> Root {
    let mut h = Sha256::new();
    h.update(left);
    h.update(right);
    h.finalize().into()
}
