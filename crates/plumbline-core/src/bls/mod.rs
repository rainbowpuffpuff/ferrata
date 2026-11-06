//! BLS12-381 behind one trait, so a LEZ precompile or a different library is a local change.
//!
//! # The signing-key composition
//!
//! The spec calls `FastAggregateVerify(participant_pubkeys, signing_root, signature)`. A
//! backend may build that aggregate in either of two equal ways:
//!
//! * **additive**: the sum of the signers' keys;
//! * **subtractive**: the committee's `aggregate_pubkey` minus the non-signers' keys. With
//!   typical participation (over 95% on mainnet) this needs a handful of point additions
//!   instead of about 500, which matters inside a zkVM.
//!
//! The two are equal whenever every key is a curve point and `aggregate_pubkey` is their sum.
//! The beacon state guarantees both for every committee the light client accepts, and
//! Plumbline's policy store does not rely on that: it runs [`BlsBackend::check_committee`] once
//! on each committee it adopts, so the subtractive composition is exactly the spec's
//! `FastAggregateVerify` for every committee an instance can hold. (Without that check, the
//! subtractive path never decodes the signers' keys and would not notice an invalid one.)
//! Both compositions are run over every spec vector.
//!
//! Keys are decoded with an on-curve check only. Instead of a subgroup check per key, the
//! *aggregate* is checked once: it must not be the identity and must lie in the prime-order
//! subgroup. That is what the pyspec's `py_ecc` does (`KeyValidate` on the aggregate inside
//! `CoreVerify`). The signature is decoded with its subgroup check.

use crate::{
    types::{BlsSignature, SyncCommittee},
    Error, Root,
};

#[cfg(feature = "blst")]
mod blst_backend;
#[cfg(feature = "zkcrypto")]
mod zkcrypto_backend;

#[cfg(feature = "blst")]
pub use blst_backend::Blst;
#[cfg(feature = "zkcrypto")]
pub use zkcrypto_backend::Zkcrypto;

/// The Ethereum proof-of-possession ciphersuite tag.
pub const DST: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";

/// How a backend builds the signing key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Aggregation {
    /// Sum of the signers' keys.
    Additive,
    /// `aggregate_pubkey` minus the non-signers' keys.
    Subtractive,
    /// Whichever needs fewer point additions.
    #[default]
    Auto,
}

impl Aggregation {
    /// True if the subtractive composition should be used for `signers` of `n`.
    pub fn subtract(self, signers: usize, n: usize) -> bool {
        match self {
            Aggregation::Additive => false,
            Aggregation::Subtractive => true,
            Aggregation::Auto => n - signers < signers,
        }
    }
}

/// A BLS12-381 implementation of the one check the light client needs.
pub trait BlsBackend {
    /// `FastAggregateVerify` of `signature` over `signing_root` by the committee members whose
    /// bit is set. Errors: [`Error::ParticipationLow`] with no signers, [`Error::PubkeyInvalid`],
    /// [`Error::SigningKeyIdentity`], [`Error::SigningKeyNotInGroup`],
    /// [`Error::SignatureMalformed`], [`Error::SignatureInvalid`].
    fn verify_sync_aggregate(
        &self,
        committee: &SyncCommittee,
        bits: &[bool],
        signing_root: &Root,
        signature: &BlsSignature,
    ) -> Result<(), Error>;

    /// Check a committee once, when an instance adopts it: every key is a point on the curve
    /// and the keys sum to `aggregate_pubkey`. Error: [`Error::CommitteeInvalid`].
    fn check_committee(&self, committee: &SyncCommittee) -> Result<(), Error>;
}

pub(crate) fn check_shape(committee: &SyncCommittee, bits: &[bool]) -> Result<usize, Error> {
    if bits.len() != committee.pubkeys.len() {
        return Err(Error::Malformed);
    }
    let signers = bits.iter().filter(|b| **b).count();
    if signers == 0 {
        return Err(Error::ParticipationLow);
    }
    Ok(signers)
}
