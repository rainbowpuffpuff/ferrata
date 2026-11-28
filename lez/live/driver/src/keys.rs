//! The accounts of the run. None holds anything of value: the payers are LEZ's own published
//! test accounts (funded by the genesis of LEZ's local network), and every other key is derived
//! from a label, so that a second run deploys at the same addresses.

use anyhow::{anyhow, Result};
use lee::{AccountId, PrivateKey};
use sha2::{Digest, Sha256};

/// The public account of `key`: its public key on the release candidates.
#[cfg(not(feature = "lez-040"))]
pub fn account_of(key: &PrivateKey) -> AccountId {
    AccountId::from(&lee::PublicKey::new_from_private_key(key))
}

/// The public account of `key`: from v0.4.0 the key's 32 bytes are the account's secret, and the account is the hash
/// of its nonce-0 key (LEZ #1135).
#[cfg(feature = "lez-040")]
pub fn account_of(key: &PrivateKey) -> AccountId {
    lee::signature::public_account_id(&lee::signature::PublicAccountSecret::new(*key.value()))
}

/// A throwaway key: SHA-256 of a fixed prefix, the label and a counter (the first that is a
/// valid scalar).
pub fn derived_key(label: &str) -> PrivateKey {
    let mut counter = 0u32;
    loop {
        let mut h = Sha256::new();
        h.update(b"ferrata-live/v1/key/");
        h.update(label.as_bytes());
        h.update(counter.to_le_bytes());
        let bytes: [u8; 32] = h.finalize().into();
        if let Ok(key) = PrivateKey::try_new(bytes) {
            return key;
        }
        counter += 1;
    }
}

/// The private keys LEZ's Justfile imports into its wallet (`wallet-import-test-accounts`): the
/// two accounts the genesis of `lez/sequencer/service/networks/local.json` supplies.
#[cfg(not(feature = "lez-040"))]
const LEZ_LOCAL_TEST_KEYS: [&str; 2] = [
    "7f273098f25b71e6c005a9519f2678da8d1c7f01f6a27778e2d9948abdf901fb",
    "f434f8741720014586ae43356d2aec6257da086222f604ddb75d69733b86fc4c",
];
/// The account secrets LEZ v0.4.0's Justfile imports (`account import public --secret`): the two accounts its
/// `local.json` genesis supplies, regenerated for v0.4.0's typed initial keys (LEZ #1135).
#[cfg(feature = "lez-040")]
const LEZ_LOCAL_TEST_KEYS: [&str; 2] = [
    "10a26a9aec7d34b82364eeae45c5294dbb0a764b000b94eeb9b58511dc487c4d",
    "717940b1cc55e5d6b2066dbf1d9a3f26f212f4db08d02388177fcfedd8a9be1b",
];

/// The funded public accounts of LEZ's local network whose keys LEZ publishes: the two of
/// `testnet_initial_state` (10^13 and 2·10^13) and the two of local.json's genesis.
pub fn lez_test_payers() -> Result<Vec<PrivateKey>> {
    let mut keys: Vec<PrivateKey> = testnet_payer_keys()?;
    for hex_key in LEZ_LOCAL_TEST_KEYS {
        let bytes: [u8; 32] =
            hex::decode(hex_key)?.try_into().map_err(|_| anyhow!("a LEZ test key is not 32 bytes"))?;
        keys.push(PrivateKey::try_new(bytes).map_err(|e| anyhow!("a LEZ test key is not a scalar: {e}"))?);
    }
    Ok(keys)
}

/// The keys of `testnet_initial_state`'s two funded accounts.
#[cfg(not(feature = "lez-040"))]
fn testnet_payer_keys() -> Result<Vec<PrivateKey>> {
    Ok(testnet_initial_state::initial_pub_accounts_private_keys().into_iter().map(|a| a.pub_sign_key).collect())
}

/// The keys of `testnet_initial_state`'s two funded accounts: their secrets from v0.4.0 (LEZ #1135), carried here in
/// a `PrivateKey` (32 bytes; see `account_of`).
#[cfg(feature = "lez-040")]
fn testnet_payer_keys() -> Result<Vec<PrivateKey>> {
    testnet_initial_state::initial_pub_accounts_private_keys()
        .into_iter()
        .map(|a| PrivateKey::try_new(*a.secret.value()).map_err(|e| anyhow!("a LEZ test secret is not a scalar: {e}")))
        .collect()
}
