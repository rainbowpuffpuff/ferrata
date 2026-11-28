//! The calls whose LEZ API differs between the release candidates and v0.4.0 (feature `lez-040`), for `ferrata-day1`
//! and the sequencer ledger.
//!
//! Keys: a key file holds 32 bytes, read into a `PrivateKey` as before. On the release candidates they are a Schnorr
//! private key whose public key is the account; from v0.4.0 (LEZ #1135) they are the account's secret: the key at each
//! nonce derives from it and rotates, and the account is the hash of its nonce-0 key ("typed initial key", 33 bytes:
//! the key type then the key's commitment). A signature authorizes only the actors the transaction marks `Signer`
//! (LEZ #1144), and a block carries a receipt per user transaction instead of a list of the reverted ones (LEZ #1107).

use common::block::BlockBody;
use ferrata_updater::lez::PayerKey;
use lee::{
    public_transaction::{Message as PublicMessage, WitnessSet},
    AccountId, Actor, PrivateKey,
};

/// The public account of a key file's key.
#[cfg(not(feature = "lez-040"))]
pub fn account_of(key: &PrivateKey) -> AccountId {
    AccountId::from(&lee::PublicKey::new_from_private_key(key))
}

/// The public account of a key file's key: the hash of the nonce-0 key its secret derives (LEZ #1135).
#[cfg(feature = "lez-040")]
pub fn account_of(key: &PrivateKey) -> AccountId {
    lee::signature::public_account_id(&secret(key))
}

/// A key file's 32 bytes as the account's secret (v0.4.0).
#[cfg(feature = "lez-040")]
pub fn secret(key: &PrivateKey) -> lee::signature::PublicAccountSecret {
    lee::signature::PublicAccountSecret::new(*key.value())
}

/// The key that makes a key file's account known to others, as hex: the 32-byte public key on the release candidates.
#[cfg(not(feature = "lez-040"))]
pub fn public_key_hex(key: &PrivateKey) -> String {
    hex::encode(lee::PublicKey::new_from_private_key(key).value())
}

/// The key that makes a key file's account known to others, as hex: from v0.4.0 its typed initial key (33 bytes, `00`
/// for Schnorr then the key's commitment; LEZ #1135), as LEZ's genesis files write `identified.key`.
#[cfg(feature = "lez-040")]
pub fn public_key_hex(key: &PrivateKey) -> String {
    lee::signature::initial_key(&secret(key)).to_string()
}

/// `actors` as a public message declares them when the accounts `signers` sign it.
#[cfg(not(feature = "lez-040"))]
pub fn declared(actors: Vec<Actor>, _signers: &[AccountId]) -> Vec<Actor> {
    actors
}

/// `actors` as a public message declares them when the accounts `signers` sign it: every actor of a signing account
/// marked `Signer`, which is what a release candidate's signature authorized (LEZ #1144), the others `None`.
#[cfg(feature = "lez-040")]
pub fn declared(actors: Vec<Actor>, signers: &[AccountId]) -> Vec<(Actor, lee_core::execution_state::Authority)> {
    use lee_core::execution_state::Authority;
    actors
        .into_iter()
        .map(|actor| (actor, if signers.contains(&actor.account_id) { Authority::Signer } else { Authority::None }))
        .collect()
}

/// The witness set of `message` signed by the key files' keys `keys`.
#[cfg(not(feature = "lez-040"))]
pub fn sign(message: &PublicMessage, keys: &[&PrivateKey]) -> WitnessSet {
    WitnessSet::for_message(message, keys)
}

/// The witness set of `message` signed by the key files' keys `keys`, each with its key at the nonce the message gives
/// its account, rotating to the next nonce's Schnorr key (LEZ #1135).
#[cfg(feature = "lez-040")]
pub fn sign(message: &PublicMessage, keys: &[&PrivateKey]) -> WitnessSet {
    let secrets: Vec<_> = keys.iter().map(|key| secret(key)).collect();
    sign_secrets(message, &secrets.iter().collect::<Vec<_>>())
}

/// The witness set of `message` signed by the payer alone.
#[cfg(not(feature = "lez-040"))]
pub fn sign_payer(message: &PublicMessage, payer: &PayerKey) -> WitnessSet {
    WitnessSet::for_message(message, &[payer])
}

/// The witness set of `message` signed by the payer alone (its key at its nonce in the message, LEZ #1135).
#[cfg(feature = "lez-040")]
pub fn sign_payer(message: &PublicMessage, payer: &PayerKey) -> WitnessSet {
    sign_secrets(message, &[payer])
}

#[cfg(feature = "lez-040")]
fn sign_secrets(message: &PublicMessage, secrets: &[&lee::signature::PublicAccountSecret]) -> WitnessSet {
    use lee::signature::Signer;
    use lee_core::signing_key::KeyType;
    let signers: Vec<Signer> = secrets
        .iter()
        .map(|secret| {
            let account = lee::signature::public_account_id(secret);
            let nonce = *message.nonces.get(&account).expect("every signer has a nonce in the message");
            Signer::new(secret, nonce, KeyType::Schnorr, KeyType::Schnorr)
        })
        .collect();
    WitnessSet::sign(message, &signers)
}

/// The wallet's identity of a public account known by its key, to credit it (with admission evidence if it is new).
#[cfg(not(feature = "lez-040"))]
pub fn foreign(key: &PrivateKey) -> wallet::AccountIdentity {
    wallet::AccountIdentity::PublicForeign(lee::PublicKey::new_from_private_key(key))
}

/// The wallet's identity of a public account known by its key, to credit it: from v0.4.0 by its typed initial key.
#[cfg(feature = "lez-040")]
pub fn foreign(key: &PrivateKey) -> wallet::AccountIdentity {
    wallet::AccountIdentity::public_foreign(lee::signature::initial_key(&secret(key)))
        .expect("a Schnorr initial key gives an account")
}

/// A public account given by the key that makes it known (`--to-pk`): 64 hex digits, a public key, on the release
/// candidates. Its account and the wallet's identity to credit it with.
#[cfg(not(feature = "lez-040"))]
pub fn parse_foreign_key(hex_key: &str) -> anyhow::Result<(AccountId, wallet::AccountIdentity)> {
    let b: [u8; 32] = hex::decode(hex_key.trim().trim_start_matches("0x"))
        .map_err(|e| anyhow::anyhow!("--to-pk: not hex ({e})"))?
        .try_into()
        .map_err(|_| anyhow::anyhow!("--to-pk: not 32 bytes"))?;
    let pk = lee::PublicKey::try_new(b).map_err(|e| anyhow::anyhow!("--to-pk: not a public key ({e:?})"))?;
    Ok((AccountId::from(&pk), wallet::AccountIdentity::PublicForeign(pk)))
}

/// A public account given by the key that makes it known (`--to-pk`): from v0.4.0 its typed initial key, 66 hex
/// digits (LEZ #1135), as LEZ's wallet prints it. Its account and the wallet's identity to credit it with.
#[cfg(feature = "lez-040")]
pub fn parse_foreign_key(hex_key: &str) -> anyhow::Result<(AccountId, wallet::AccountIdentity)> {
    use std::str::FromStr as _;
    let key = lee_core::signing_key::SigningKeyState::from_str(hex_key.trim().trim_start_matches("0x"))
        .map_err(|e| anyhow::anyhow!("--to-pk: not a typed initial key, 66 hex digits ({e})"))?;
    let account =
        AccountId::for_initial_key(&key).ok_or_else(|| anyhow::anyhow!("--to-pk: not an initial key type"))?;
    let identity = wallet::AccountIdentity::public_foreign(key)
        .ok_or_else(|| anyhow::anyhow!("--to-pk: not an initial key type"))?;
    Ok((account, identity))
}

/// Whether the user transaction at `position` of a block reverted.
#[cfg(not(feature = "lez-040"))]
pub fn is_reverted(body: &BlockBody, position: usize) -> bool {
    body.reverted.contains(&(position as u64))
}

/// Whether the user transaction at `position` of a block reverted: its receipt says so (LEZ #1107).
#[cfg(feature = "lez-040")]
pub fn is_reverted(body: &BlockBody, position: usize) -> bool {
    body.is_reverted(position)
}
