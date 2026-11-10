//! The accounts of the run. None holds anything of value: the payers are LEZ's own published
//! test accounts (funded by the genesis of LEZ's local network), and every other key is derived
//! from a label, so that a second run deploys at the same addresses.

use anyhow::{anyhow, Result};
use lee::{AccountId, PrivateKey, PublicKey};
use sha2::{Digest, Sha256};

pub fn account_of(key: &PrivateKey) -> AccountId {
    AccountId::from(&PublicKey::new_from_private_key(key))
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
const LEZ_LOCAL_TEST_KEYS: [&str; 2] = [
    "7f273098f25b71e6c005a9519f2678da8d1c7f01f6a27778e2d9948abdf901fb",
    "f434f8741720014586ae43356d2aec6257da086222f604ddb75d69733b86fc4c",
];

/// The funded public accounts of LEZ's local network whose keys LEZ publishes: the two of
/// `testnet_initial_state` (10^13 and 2·10^13) and the two of local.json's genesis.
pub fn lez_test_payers() -> Result<Vec<PrivateKey>> {
    let mut keys: Vec<PrivateKey> =
        testnet_initial_state::initial_pub_accounts_private_keys().into_iter().map(|a| a.pub_sign_key).collect();
    for hex_key in LEZ_LOCAL_TEST_KEYS {
        let bytes: [u8; 32] =
            hex::decode(hex_key)?.try_into().map_err(|_| anyhow!("a LEZ test key is not 32 bytes"))?;
        keys.push(PrivateKey::try_new(bytes).map_err(|e| anyhow!("a LEZ test key is not a scalar: {e}"))?);
    }
    Ok(keys)
}
