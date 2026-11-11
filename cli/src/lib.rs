//! `ferrata`: the user's and the relayer's tool for the Ferrata bridge (prototype, unaudited).
//!
//! * [`seed`]: the seed file; LEZ's wallet keys, the bridge seed, `kAuth`, `kView`, destinations.
//! * [`eth`]: the vault's calls and events, a keystore signer, the verified deposit list.
//! * [`lez`]: the zone's history, the claim and burn calls with the program's own checks, and
//!   submission through LEZ's wallet library.
//! * [`scan`]: recovery from the seed alone.
//! * [`release`]: the release witness, journal, guest run and vault call.
//! * [`relayer`]: the relayer daemon's checks and calls: releases and rule (B) checkpoints on the
//!   vault, the gas drop on LEZ (`ferrata-relayer`).
//! * [`codes`], [`units`]: refusals with their codes and meanings; amounts with decimals.

#![forbid(unsafe_code)]

pub mod codes;
pub mod eth;
pub mod lez;
pub mod relayer;
pub mod release;
pub mod scan;
pub mod seed;
pub mod units;

/// `0x`-prefixed hex.
pub fn hx(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

/// 32 bytes from hex (with or without `0x`).
pub fn h32(text: &str) -> anyhow::Result<[u8; 32]> {
    let bytes = hex::decode(text.trim().trim_start_matches("0x"))?;
    bytes.try_into().map_err(|_| anyhow::anyhow!("not 32 bytes: {text}"))
}

/// A LEZ account id from its 32-byte hex.
pub fn account(text: &str) -> anyhow::Result<lee::AccountId> {
    Ok(lee::AccountId::new(h32(text)?))
}
