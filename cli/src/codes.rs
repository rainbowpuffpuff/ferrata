//! Every refusal the tool reports names its source, its code and what the code means: the LEZ
//! bridge program's exit codes (240–255, `lez/bridge-wire/src/lib.rs`, `code`) and the vault's
//! custom errors (`vault/src/FerrataVault.sol`; none carries parameters, so a refusal echoes no
//! user data). The tool checks what it can before it sends (with the program's own check
//! functions), so most refusals come before anything is signed.

use std::fmt;

use alloy::primitives::keccak256;

/// Who refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// The LEZ bridge program (an exit code), or the tool's check with the program's own function.
    Program,
    /// The Ethereum vault (a custom error).
    Vault,
}

/// A refusal with its code and meaning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub source: Source,
    /// `250` for the program; the error's name for the vault.
    pub code: String,
    pub name: &'static str,
    pub meaning: &'static str,
    /// Where it was found: "checked before sending" or "returned by …".
    pub how: String,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.source {
            Source::Program => write!(
                f,
                "refused: LEZ bridge program exit code {} {} ({}): {}",
                self.code, self.name, self.how, self.meaning
            ),
            Source::Vault => write!(f, "refused: vault error {}() ({}): {}", self.name, self.how, self.meaning),
        }
    }
}

impl std::error::Error for Refusal {}

/// The bridge program's exit codes: (code, name, meaning).
pub const PROGRAM_CODES: [(u32, &str, &str); 16] = [
    (240, "BAD_MESSAGE", "a message that does not decode or that this actor does not take, or values outside the specified bounds"),
    (241, "BAD_STATE", "stored state that does not decode, or is another role's, or a transaction-internal step found where none can be"),
    (242, "FROZEN", "mints or burns are frozen (of the token or of the whole configuration), or an import after Sunset; nothing is consumed: retry once the freeze ends (`ferrata status` shows the freezes)"),
    (243, "WRONG_SENDER", "an internal message from an unexpected actor, or a receiver that is not the actor the message names"),
    (244, "ALREADY_WRITTEN", "a write-once account is already written (a configuration, a root at this count, a token entry, a burn note, the sunset)"),
    (245, "NOT_FOUND", "no configuration, or no imported root at this leaf count"),
    (246, "PAIRING_MISMATCH", "the vault's slot 2 does not hold the configuration's id"),
    (247, "IMPORT_MISMATCH", "the attested facts are not the vault's slots holding what the message says"),
    (248, "LEAF_NOT_IMPORTED", "the leaf index is at or above the imported root's leaf count, or outside the fee schedule range the claim names"),
    (249, "CLAIM_INVALID", "the opening, the leaf and the path do not hash to the root (a wrong leaf, path or destination); the deposit stays claimable: claim it with the seed that made it, to the destination it commits to (`ferrata scan` lists this seed's deposits)"),
    (250, "ALREADY_CLAIMED", "the deposit's nullifier is spent: it was claimed already"),
    (251, "NOT_REGISTERED", "the token has no registry entry in this configuration, or the amount is not one of its denominations"),
    (252, "WRONG_DESTINATION", "a destination this path cannot credit: a plaintext claim of a private-kind deposit, or a private claim not run as its destination; the deposit stays claimable: claim a private-kind deposit privately (the default, without --plaintext), as its destination account"),
    (253, "NOT_AUTHORIZED", "a burn not authorised by its holder or not a top-level call, or a control message not from its role"),
    (254, "INSUFFICIENT_BALANCE", "the holding is smaller than the denomination burned"),
    (255, "CAPPED", "the token's or the global mint or burn bucket holds less than the amount; retry later"),
];

/// The vault's custom errors a user or relayer can meet: (name, meaning).
pub const VAULT_ERRORS: [(&str, &str); 41] = [
    ("VaultRetired", "the vault is retired: no deposits"),
    ("DepositsAreFrozen", "deposits are frozen for the whole vault; nothing is sent: retry once the freeze ends (`ferrata status` shows it)"),
    ("TokenDepositsAreFrozen", "deposits of this token are frozen; nothing is sent: retry once the freeze ends (`ferrata status` shows it)"),
    ("DepositsDisabled", "deposits of this token are disabled"),
    ("UseDepositETH", "ETH is deposited with depositETH"),
    ("InvalidDenominationIndex", "no denomination has this index: a token is bridged in its fixed denominations only (`ferrata status --token <address>` lists them by index)"),
    ("ZeroCommitment", "the commitment is zero"),
    ("WrongDepositIndex", "the index j is not depositCount[sender]: another deposit from this address came first"),
    ("WrongEthAmount", "msg.value is not the denomination plus the fees in force"),
    ("TransferAmountMismatch", "the token moved another amount than asked (fee-on-transfer tokens are refused)"),
    ("TreeFull", "the deposit tree is full"),
    ("TokenDepositCapExceeded", "the token's deposit bucket holds less than this; retry later"),
    ("GlobalDepositCapExceeded", "the global deposit bucket holds less than this; retry later"),
    ("TokenNotRegistered", "the token is not registered: only registered tokens are bridged (`ferrata status --token <address>` shows whether a token is, and its denominations)"),
    ("ReleasesAreFrozen", "releases are frozen for the whole vault; the burn stays releasable: retry once the freeze ends (`ferrata status` shows it)"),
    ("TokenReleasesAreFrozen", "releases of this token are frozen; the burn stays releasable: retry once the freeze ends (`ferrata status` shows it)"),
    ("BurnRootNotAccepted", "the burn root is not one of a finalized, unrevoked checkpoint"),
    ("ReleaseDelayNotElapsed", "the release delay after the checkpoint has not passed"),
    ("NullifierAlreadyUsed", "this burn was released already"),
    ("InvalidRelayFee", "the relay fee is neither 0 nor the registered relay fee"),
    ("ZeroRecipient", "the recipient is the zero address"),
    ("InsufficientEscrow", "the vault holds less escrow of this token than the release"),
    ("TokenReleaseCapExceeded", "the token's release bucket holds less than this; retry later"),
    ("GlobalReleaseCapExceeded", "the global release bucket holds less than this; retry later"),
    ("EthTransferFailed", "the ETH payout failed"),
    ("NothingOwed", "the vault owes this address nothing in this token"),
    ("VerifierStopped", "the verifier's emergency stop is tripped"),
    // The RISC Zero verifier's error (IRiscZeroVerifier), raised through the vault's call to it.
    ("VerificationFailed", "the RISC Zero verifier rejects the proof: the seal is not a proof of this call's journal (a release's: another recipient, relay fee, root or image id; a checkpoint's: another root, committee or image id) or is damaged; nothing is sent: prove it again for this journal (`ferrata release --prove`, or a prover given `--input-out`'s input, then `--seal`; for a checkpoint, its prover again for this candidate), or ask the relayer for a new proof"),
    ("CheckpointsAreFrozen", "checkpoints are frozen; retry once the freeze ends"),
    ("WrongMode", "this finalisation path is not the vault's mode"),
    ("WaitNotOver", "the candidate's wait is not over"),
    ("ParentNotFinal", "the candidate does not build on the final state"),
    ("UnknownCandidate", "no such candidate"),
    ("UnknownBeacon", "no such beacon"),
    ("UnknownLeaf", "no such leaf"),
    ("AlreadyRefunded", "the deposit was refunded already"),
    ("NotRefundable", "the deposit is not refundable"),
    ("RefundNotYetOpen", "refunds are not open yet"),
    ("InvalidMerklePath", "the Merkle path does not lead to the root"),
    ("DirectEthNotAccepted", "ETH sent without a deposit is refused"),
    ("FunctionNotFound", "the vault has no such function"),
];

/// The program's code `code`, as a refusal.
pub fn program_refusal(code: u32, how: impl Into<String>) -> Refusal {
    let (name, meaning) = PROGRAM_CODES
        .iter()
        .find(|(c, _, _)| *c == code)
        .map(|(_, n, m)| (*n, *m))
        .unwrap_or(("UNKNOWN", "not one of the bridge program's codes (240-255): another program's or LEZ's"));
    Refusal { source: Source::Program, code: code.to_string(), name, meaning, how: how.into() }
}

/// The selector of a parameterless custom error.
pub fn selector(name: &str) -> [u8; 4] {
    let h = keccak256(format!("{name}()").as_bytes());
    [h[0], h[1], h[2], h[3]]
}

/// The vault error whose selector starts `revert_data`, as a refusal.
pub fn vault_refusal(revert_data: &[u8], how: impl Into<String>) -> Option<Refusal> {
    let sel: [u8; 4] = revert_data.get(..4)?.try_into().ok()?;
    VAULT_ERRORS.iter().find(|(name, _)| selector(name) == sel).map(|(name, meaning)| Refusal {
        source: Source::Vault,
        code: (*name).to_string(),
        name,
        meaning,
        how: how.into(),
    })
}

#[cfg(test)]
mod tests {
    use ferrata_lez_bridge_wire::code;

    use super::*;

    #[test]
    fn program_codes_are_the_wire_crates() {
        let wire = [
            code::BAD_MESSAGE as u32,
            code::BAD_STATE as u32,
            code::FROZEN as u32,
            code::WRONG_SENDER as u32,
            code::ALREADY_WRITTEN as u32,
            code::NOT_FOUND as u32,
            code::PAIRING_MISMATCH as u32,
            code::IMPORT_MISMATCH as u32,
            code::LEAF_NOT_IMPORTED as u32,
            code::CLAIM_INVALID as u32,
            code::ALREADY_CLAIMED as u32,
            code::NOT_REGISTERED as u32,
            code::WRONG_DESTINATION as u32,
            code::NOT_AUTHORIZED as u32,
            code::INSUFFICIENT_BALANCE as u32,
            code::CAPPED as u32,
        ];
        let ours: Vec<u32> = PROGRAM_CODES.iter().map(|(c, _, _)| *c).collect();
        assert_eq!(ours, wire);
        assert_eq!(program_refusal(250, "x").name, "ALREADY_CLAIMED");
        assert_eq!(program_refusal(7, "x").name, "UNKNOWN");
    }

    #[test]
    fn vault_errors_decode_from_their_selector() {
        // keccak256("NullifierAlreadyUsed()")[..4], computed independently of `selector`.
        let data = alloy::primitives::keccak256(b"NullifierAlreadyUsed()");
        let r = vault_refusal(&data[..4], "eth_call").expect("known");
        assert_eq!(r.name, "NullifierAlreadyUsed");
        assert!(vault_refusal(&[0, 0, 0, 0], "x").is_none());
        let names: std::collections::HashSet<_> = VAULT_ERRORS.iter().map(|(n, _)| selector(n)).collect();
        assert_eq!(names.len(), VAULT_ERRORS.len(), "selectors are distinct");
    }

    #[test]
    fn the_verifiers_rejection_is_named() {
        // `cast sig 'VerificationFailed()'` = 0x439cc0cd: the RISC Zero verifier's error, raised through the vault
        // (an internal review's M1: the tool printed it as a raw revert).
        let r = vault_refusal(&[0x43, 0x9c, 0xc0, 0xcd], "eth_call, nothing sent").expect("named");
        assert_eq!(r.name, "VerificationFailed");
        assert!(r.meaning.contains("prove it again for this journal"), "{}", r.meaning);
    }
}
