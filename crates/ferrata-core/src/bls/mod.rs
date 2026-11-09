//! BLS12-381 behind one trait, so a LEZ precompile or a different library is a local change.
//!
//! There are two verification paths, and the stores pick one.
//!
//! * [`BlsBackend::fast_aggregate_verify`] is the spec's `FastAggregateVerify` as the pyspec
//!   runs it (py_ecc's `G2ProofOfPossession`). Every signer key must pass `KeyValidate`: it must
//!   be a curve point, not the identity, and in the prime-order subgroup. The aggregate must not
//!   be the identity, and the signature must be a non-identity point of G2. The spec store
//!   ([`crate::store::LightClientStore`]) uses this path.
//! * [`BlsBackend::verify_checked`] is for a committee that has passed
//!   [`BlsBackend::check_committee`]: every key `KeyValidate`d, and the keys summing to
//!   `aggregate_pubkey`. Ferrata's policy store ([`crate::policy::FinalizedStore`]) runs that
//!   check once on each committee it adopts. After that, the signing key can be built
//!   additively, or as `aggregate_pubkey − Σ non-signers`. At mainnet participation (over 95%)
//!   the second needs a handful of point additions instead of about 500. It also needs no
//!   per-update subgroup check, because a sum of points of G1 stays in G1. For every committee
//!   that passed the check, both compositions return exactly what `fast_aggregate_verify`
//!   returns, and the tests run every vector through all of them.
//!
//! The pairing is a Miller-loop product followed by one final exponentiation. The `blst` backend
//! exposes that seam ([`Blst::miller_half`] and [`Blst::final_half`]), so a multi-transaction
//! program can store the 576-byte Miller-loop product between transactions; zkcrypto computes one
//! `multi_miller_loop` and offers no such seam.

use crate::{
    types::{BlsSignature, SyncCommittee},
    Error, Root,
};

#[cfg(feature = "blst")]
mod blst_backend;
#[cfg(feature = "zkcrypto")]
mod zkcrypto_backend;

#[cfg(feature = "blst")]
pub use blst_backend::{Blst, FP12_BYTES};
#[cfg(feature = "zkcrypto")]
pub use zkcrypto_backend::Zkcrypto;

/// The Ethereum proof-of-possession ciphersuite tag.
pub const DST: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";

/// How [`BlsBackend::verify_checked`] builds the signing key.
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

/// A BLS12-381 implementation of the checks the light client needs.
///
/// Error codes, shared by every backend: [`Error::ParticipationLow`] (no signer),
/// [`Error::PubkeyInvalid`] (a key fails `KeyValidate`), [`Error::SigningKeyIdentity`],
/// [`Error::SignatureMalformed`] (not a point of G2), [`Error::SignatureInvalid`],
/// [`Error::CommitteeInvalid`].
pub trait BlsBackend {
    /// The spec's `FastAggregateVerify` over the members whose bit is set.
    fn fast_aggregate_verify(
        &self,
        committee: &SyncCommittee,
        bits: &[bool],
        signing_root: &Root,
        signature: &BlsSignature,
    ) -> Result<(), Error>;

    /// Check a committee once, when a store adopts it. Every key must pass `KeyValidate`, and the
    /// keys must sum to `aggregate_pubkey`.
    fn check_committee(&self, committee: &SyncCommittee) -> Result<(), Error>;

    /// The check the on-chain program makes when it adopts a committee: every key is a curve
    /// point and not the identity, and the keys sum to `aggregate_pubkey`. It leaves out the
    /// per-key subgroup check, which costs about 180M cycles per committee in a RISC Zero guest
    /// (measured, `bench/`) and which every key of a genuine beacon state already passed when its
    /// deposit was processed. For a committee whose keys are all in G1, `verify_checked` then
    /// equals `fast_aggregate_verify`; `check_committee` (off chain) confirms that property.
    fn check_committee_onchain(&self, committee: &SyncCommittee) -> Result<(), Error>;

    /// `FastAggregateVerify` for a committee that passed [`BlsBackend::check_committee`]. It
    /// decodes keys with an on-curve check only and builds the signing key by the backend's
    /// [`Aggregation`]. Called on an unchecked committee, its verdict is not defined.
    fn verify_checked(
        &self,
        committee: &SyncCommittee,
        bits: &[bool],
        signing_root: &Root,
        signature: &BlsSignature,
    ) -> Result<(), Error>;
}

/// Shape and participation shared by every path: one bit per member, and at least one signer.
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
