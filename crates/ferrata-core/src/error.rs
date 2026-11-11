//! Typed errors with stable numeric codes.
//!
//! The same codes are returned by the on-chain program and by the off-chain dry run. Messages
//! carry the code only, never the query, a header or an identifier. (On LEZ v0.4 a refused
//! transaction is recorded only as reverted; the code reaches logs and RPC responses.)

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

    /// Ferrata policy: only finality updates may advance an instance.
    NotFinalized = 50,
    /// Ferrata policy: participation is below the supermajority (2/3) floor.
    BelowSupermajority = 51,
    /// Ferrata policy: the update does not advance the finalized header (duplicate or stale).
    NotNewer = 52,
    /// Ferrata policy: the store's finalized header is older than its weak-subjectivity period
    /// at the current slot (`crate::ws`); recover from a new bootstrap.
    OutsideWeakSubjectivityPeriod = 53,

    /// Store invariant: next committee unknown and the update finalizes outside the store period.
    CommitteeUnknown = 60,
    /// Data of one fork was handed to a store of another (or a downgrade was requested).
    ForkMismatch = 61,

    // Execution layer (RLP, Merkle-Patricia proofs, execution objects, attestations): 100 up.
    /// RLP that is not the canonical encoding: a non-minimal length, a long-form length with a
    /// leading zero, a single byte below 0x80 wrapped as a string, or an integer with a leading
    /// zero byte.
    RlpNonCanonical = 100,
    /// RLP that is truncated or does not have the expected shape: a length past the end of the
    /// input or of the enclosing list, bytes after the item, a list where a string belongs (or
    /// the reverse), a wrong item count or field size, or an integer too large for its field.
    RlpMalformed = 101,

    /// Inclusion proof: a node's Keccak-256 differs from the root or from its parent's reference.
    ProofNodeMismatch = 110,
    /// Inclusion proof: the proof ends before the walk reaches a value or the key's absence.
    ProofIncomplete = 111,
    /// Inclusion proof: elements remain after the walk ended.
    ProofTrailingNodes = 112,
    /// Inclusion proof: a node is not a canonical trie node (item count, hex-prefix flag or
    /// padding, empty extension path, child reference, empty value, or a node shorter than 32
    /// bytes referenced by hash).
    ProofNodeInvalid = 113,

    /// keccak256 of an RLP block header differs from the hash it must match.
    HeaderHashMismatch = 120,
    /// A block header that is not of the Cancun (20 fields), Prague/Osaka (21) or Amsterdam (23)
    /// layout.
    HeaderUnsupported = 121,
    /// The receipts trie has no receipt at the requested transaction index.
    ReceiptNotIncluded = 122,
    /// A pre-Byzantium receipt (state root instead of status) or an unknown receipt type.
    ReceiptUnsupported = 123,
    /// The transaction failed (receipt status 0).
    ReceiptFailed = 124,
    /// The receipt has no log at the requested position.
    LogIndexOutOfRange = 125,

    /// A condition of the statement does not hold.
    PredicateFailed = 130,
    /// A condition names a field its fact does not have, or the query is empty.
    QueryInvalid = 131,

    /// The record belongs to another chain than the consumer is configured for.
    ChainMismatch = 140,
    /// The finalized header does not carry the execution binding the record constructor reads.
    BindingMismatch = 141,

    /// EIP-2935: the older block is not among the 8191 blocks before the record's block.
    HistoryOutOfWindow = 150,
    /// EIP-2935: the history slot does not hold keccak256 of the supplied header.
    HistoryHashMismatch = 151,
    /// EIP-2935: no history contract with the specified code in the record's state.
    HistoryContractMissing = 152,

    /// Accumulator: the path does not have 32 siblings.
    AccumulatorPathInvalid = 160,
    /// Accumulator: the leaf index is 2^32 or more (it would share a path with its low 32 bits).
    AccumulatorIndexOutOfRange = 161,
    /// Accumulator: the leaf is the empty leaf (32 zero bytes).
    AccumulatorLeafEmpty = 162,
    /// Accumulator: the path does not hash the leaf to the attested root.
    AccumulatorRootMismatch = 163,
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
