//! The payer's key, whose type differs between LEZ's release candidates and v0.4.0 (feature `lez-040`): a Schnorr
//! private key on the release candidates, the public account's secret from v0.4.0 (LEZ #1135: the key at each nonce
//! derives from it and rotates, and the account's id is the hash of its nonce-0 key).

use anyhow::Result;
use lee::AccountId;

/// The payer's key.
#[cfg(not(feature = "lez-040"))]
pub type PayerKey = lee::PrivateKey;
/// The payer's key: the public account's secret (LEZ #1135).
#[cfg(feature = "lez-040")]
pub type PayerKey = lee::signature::PublicAccountSecret;

/// The payer key made of `bytes` (the key file's 32 bytes): refused if not a valid private key on the release
/// candidates; every 32 bytes are a secret on v0.4.0.
#[cfg(not(feature = "lez-040"))]
pub fn try_payer_key(bytes: [u8; 32]) -> Result<PayerKey> {
    lee::PrivateKey::try_new(bytes).map_err(|e| anyhow::anyhow!("payer key: {e:?}"))
}

/// The payer key made of `bytes` (the key file's 32 bytes): refused if not a valid private key on the release
/// candidates; every 32 bytes are a secret on v0.4.0.
#[cfg(feature = "lez-040")]
pub fn try_payer_key(bytes: [u8; 32]) -> Result<PayerKey> {
    Ok(lee::signature::PublicAccountSecret::new(bytes))
}

/// [`try_payer_key`] for keys known to be valid (tests, fixtures).
pub fn payer_key(bytes: [u8; 32]) -> PayerKey {
    try_payer_key(bytes).expect("a valid payer key")
}

/// The public account `key` pays from.
#[cfg(not(feature = "lez-040"))]
pub fn payer_account(key: &PayerKey) -> AccountId {
    AccountId::from(&lee::PublicKey::new_from_private_key(key))
}

/// The public account `key` pays from: the hash of its nonce-0 key (LEZ #1135).
#[cfg(feature = "lez-040")]
pub fn payer_account(key: &PayerKey) -> AccountId {
    lee::signature::public_account_id(key)
}
