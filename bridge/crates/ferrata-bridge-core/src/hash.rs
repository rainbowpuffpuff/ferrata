//! SHA-256, HMAC-SHA256 (RFC 2104) and the wallet's key split (§2), from RustCrypto's `sha2` and
//! `hmac`. Inside a RISC Zero guest, `sha2` is replaced by RISC Zero's accelerated fork through
//! the guest workspace's `[patch.crates-io]`; nothing here changes.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use crate::encoding::{TAG_AUTH, TAG_VIEW};
use crate::Hash;

/// SHA-256 of `data`.
pub fn sha256(data: &[u8]) -> Hash {
    Sha256::digest(data).into()
}

/// HMAC-SHA256 with an arbitrary key (keys longer than 64 bytes are hashed first, as RFC 2104 says).
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> Hash {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts keys of any length");
    mac.update(message);
    mac.finalize().into_bytes().into()
}

/// A 32-byte key held as an HMAC-SHA256 state already keyed with it, so each PRF call hashes only
/// the message. No `Debug` implementation, so it cannot end up in a log by accident.
#[derive(Clone)]
pub struct BridgeKey(Hmac<Sha256>);

impl BridgeKey {
    /// Keys HMAC-SHA256 with `k`.
    pub fn new(k: &Hash) -> Self {
        Self(Hmac::<Sha256>::new_from_slice(k).expect("HMAC accepts keys of any length"))
    }

    /// `HMAC-SHA256(k, message)`.
    pub fn prf(&self, message: &[u8]) -> Hash {
        let mut mac = self.0.clone();
        mac.update(message);
        mac.finalize().into_bytes().into()
    }
}

/// `(kAuth, kView)` from the bridge seed (§2): `kAuth = HMAC(bridgeSeed, "ferrata/auth/v1")`,
/// `kView = HMAC(bridgeSeed, "ferrata/view/v1")`. How `bridgeSeed` is derived from the LEZ wallet
/// seed (under labels disjoint from LEZ's `/LEE-Keys/v1/…` tree) is outside this crate.
pub fn derive_keys(bridge_seed: &[u8]) -> (Hash, Hash) {
    (hmac_sha256(bridge_seed, TAG_AUTH), hmac_sha256(bridge_seed, TAG_VIEW))
}

/// The two wallet keys. `auth` derives the secrets that authorise claims and releases (`s`, `sOut`);
/// `view` derives burn tags, memo pads and MACs, so sharing it discloses burns and recipients without
/// letting the viewer claim or release.
#[derive(Clone)]
pub struct BridgeKeys {
    /// `kAuth`.
    pub auth: BridgeKey,
    /// `kView`.
    pub view: BridgeKey,
}

impl BridgeKeys {
    /// Both keys from the bridge seed.
    pub fn from_bridge_seed(bridge_seed: &[u8]) -> Self {
        let (k_auth, k_view) = derive_keys(bridge_seed);
        Self { auth: BridgeKey::new(&k_auth), view: BridgeKey::new(&k_view) }
    }
}
