//! Every way a checkpoint input can be refused. The guest panics on any of these, so a refused
//! input has no receipt.

use core::fmt;

/// Why an input was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    // ---- byte-level decoding -------------------------------------------------------------------
    /// Fewer bytes than the encoding needs at offset `at`.
    Truncated { at: usize },
    /// Bytes left over after a complete value (`at` = first unread byte).
    TrailingBytes { at: usize },
    /// An enum, option or bool tag outside its range.
    BadTag { what: &'static str, tag: u8 },
    /// A borsh `BTreeSet`/`BTreeMap` whose keys are not strictly increasing (LEZ decodes with
    /// borsh's `de_strict_order`, which refuses them).
    NotStrictlyOrdered { what: &'static str },
    /// A length that does not fit the remaining input or `usize`.
    BadLength { what: &'static str },

    // ---- LEZ blocks ----------------------------------------------------------------------------
    /// `reverted` is not strictly ascending or names a transaction the block does not have.
    BadRevertedList { block: u64 },
    /// Block `index` (position in the input) does not carry id `expected`.
    BadBlockId { index: u32, expected: u64, got: u64 },
    /// Block `index` does not link by `prev_block_hash` to the block before it.
    BrokenLink { index: u32 },
    /// Block `index`'s `header.hash` is not the hash of its contents.
    HashMismatch { index: u32 },
    /// The input holds no block up to `T`, or `T`'s index is past the last block.
    BadTipIndex,

    // ---- Mantle transactions (inscriptions, channel configuration) -----------------------------
    /// An opcode this program does not parse; such transactions are refused rather than guessed.
    UnsupportedOpcode { opcode: u8 },
    /// The witness does not describe an inscription operation at the claimed position.
    NotAnInscription,
    /// An inscription for another channel.
    WrongChannel,
    /// The inscription's payload is not the block's bytes.
    PayloadMismatch,
    /// An Ed25519 public key that does not decode (or is of small order where that is refused).
    InvalidKey,
    /// An Ed25519 signature that does not verify (`verify_strict`).
    BadSignature,
    /// An inscription signer, configuration signer or (rule (B)) attesting channel key that is not
    /// in the current key set.
    NotAccredited,

    // ---- the rule (SPEC-v2 §4.2) ---------------------------------------------------------------
    /// The parent state's frontier is not canonical for its burn count, or the count is too big.
    BadParentFrontier,
    /// The burn tree is full (2^32 - 1 leaves).
    BurnTreeFull,
    /// `echoNonce != 0` and no block after `T` carries it in a message to `bridgeHeader`.
    EchoNotFound,
    /// A support entry names a block outside `(Y, E]`.
    SupportOutOfRange { block_index: u32 },
    /// Fewer distinct supporting keys than the threshold.
    InsufficientSupport { have: u32, need: u32 },
    /// A key set smaller than the deployment floor `nMin`.
    KeySetTooSmall { n: u32, n_min: u32 },
    /// A threshold below the deployment floor `tMin`.
    ThresholdTooLow { t: u32, t_min: u32 },

    // ---- channel configuration changes (key rotation) ------------------------------------------
    /// A configuration op whose `parent` is not the current configuration tip.
    ConfigParentMismatch,
    /// A configuration op with a zero threshold or no keys (Bedrock's preverify refuses these).
    ConfigMalformed,
    /// A configuration signature index that is out of range or repeated.
    ConfigBadIndex,
    /// Fewer configuration signatures than `t(current n)`.
    ConfigThresholdUnmet { have: u32, need: u32 },
    /// A configuration op whose `posting_timeframe` differs from the one bound in the current
    /// key-set hash, or is 0 (SPEC-v3 D3).
    ConfigTimeframe { current: u32, proposed: u32 },

    // ---- rule (B): final-tier attestations (SPEC-v3 D7, D13) ----------------------------------
    /// An attestation whose channel key did not certify its attestation key for this deployment.
    BadCertificate,
    /// An attestation whose key did not sign the final-tier message v2 for this deployment, the
    /// live key set's hash and the tip it names.
    BadAttestation,
    /// Two attestations by one channel key, or one attestation key under two channel keys.
    DuplicateAttestor,
    /// An attested tip that is neither `T` nor a block after `T` on the input's hash-linked path.
    NotADescendant { tip_id: u64 },
    /// Fewer attestations than the threshold.
    InsufficientAttestations { have: u32, need: u32 },

    // ---- composition ---------------------------------------------------------------------------
    /// A segment journal of another configuration, or one that does not decode.
    SegmentConfigMismatch,
    /// Consecutive segments do not chain (end state of one != start state of the next), or the
    /// last segment does not end at the state the input continues from.
    SegmentChainBroken,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
