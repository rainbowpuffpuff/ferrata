//! Typed errors with stable numeric codes.
//!
//! The same codes are returned by the on-chain program and by the off-chain dry run, and an
//! on-chain error carries the code only: never the query, a header or an identifier.

use core::fmt;

/// Every way verification can refuse an input. Codes are stable: a code is never reused
/// for a different meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u16)]
#[non_exhaustive]
pub enum Error {
    /// SSZ input is malformed: wrong length, bad offset, or wrong shape for the fork.
    Malformed = 1,

    /// Bootstrap header root differs from the trusted checkpoint root.
    CheckpointMismatch = 10,
    /// Bootstrap header fails `is_valid_light_client_header`.
    BootstrapHeaderInvalid = 11,
    /// Bootstrap `current_sync_committee` branch does not verify against the state root.
    CurrentCommitteeBranchInvalid = 12,

    /// Fewer than `MIN_SYNC_COMMITTEE_PARTICIPANTS` signers.
    ParticipationLow = 20,
    /// Attested header fails `is_valid_light_client_header`.
    AttestedHeaderInvalid = 21,
    /// Violates `current_slot >= signature_slot > attested_slot >= finalized_slot`.
    SlotOrderInvalid = 22,
    /// Signature period is neither the store period nor (with a known next committee) the next.
    PeriodSkipped = 23,
    /// Update neither advances the store nor supplies the missing next committee.
    UpdateIrrelevant = 24,
    /// Finalized header is invalid, or non-empty when it must be empty.
    FinalizedHeaderInvalid = 25,
    /// Finality branch does not verify against the attested state root.
    FinalityBranchInvalid = 26,
    /// Next-sync-committee branch does not verify against the attested state root.
    NextCommitteeBranchInvalid = 27,
    /// Next committee contradicts the one the store already holds for that period.
    NextCommitteeMismatch = 28,
    /// A non-sync-committee update carries a non-empty next committee.
    NextCommitteeUnexpected = 29,

    /// The aggregate signature does not verify.
    SignatureInvalid = 40,
    /// The signature is not a valid compressed G2 point in the prime-order subgroup.
    SignatureMalformed = 41,
    /// A public key fails `KeyValidate`: not a curve point, the identity, or outside G1.
    PubkeyInvalid = 42,
    /// The signing key (aggregate of the signers) is the identity point.
    SigningKeyIdentity = 43,
    /// A committee key fails `KeyValidate`, or the keys do not sum to `aggregate_pubkey`.
    CommitteeInvalid = 45,

    /// Plumbline policy: only finality updates may advance an instance.
    NotFinalized = 50,
    /// Plumbline policy: participation is below the supermajority (2/3) floor.
    BelowSupermajority = 51,
    /// Plumbline policy: the update does not advance the finalized header (duplicate or stale).
    NotNewer = 52,

    /// Store invariant: next committee unknown and the update finalizes outside the store period.
    CommitteeUnknown = 60,
    /// Data of one fork was handed to a store of another (or a downgrade was requested).
    ForkMismatch = 61,
}

impl Error {
    /// The stable numeric code.
    pub const fn code(self) -> u16 {
        self as u16
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} (code {})", self, self.code())
    }
}
