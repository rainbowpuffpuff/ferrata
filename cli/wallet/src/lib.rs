//! `ferrata-wallet`: the `ferrata` tool's wallet rules and the bridge program's calls it builds, over
//! plain data. The tool (`cli/`) reads Ethereum and LEZ and then calls these functions;
//! `ferrata_bridge_module` takes the same data in its requests and calls the same functions, so each
//! rule has one implementation:
//!
//! * [`release`]: the release rule (the newest accepted burn root whose release delay has passed)
//!   and, with the `prove` feature, the release guest run through r0vm;
//! * [`claim`]: a deposit's claim, plaintext (`Claim`) or private (`ClaimPrivately`), with the bridge
//!   program's own checks, and the fee-schedule range a wallet names;
//! * [`import`]: the relayer's `ImportRoot` of a deposit root from the vault's `eth_getProof` proofs,
//!   dry-run with the attestation and bridge programs' own functions.
//!
//! Host code: no program or guest links it.

#![forbid(unsafe_code)]

pub mod claim;
pub mod import;
pub mod release;

use ferrata_bridge_core::Hash;
pub use lee_core::{
    account::{AccountId, Actor},
    program::PdaSeed,
};

/// Which configuration of which bridge program, and the vault it pairs with.
#[derive(Clone, Debug)]
pub struct Context {
    /// The bridge program's account.
    pub program: AccountId,
    /// `configId`.
    pub cid: Hash,
    pub chain_id: u64,
    pub vault: [u8; 20],
}

/// One account a call of the bridge program names, and how the sender names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mention {
    /// A PDA of a program, with its seed (the evidence that admits it when it is new).
    Pda { actor: Actor, seed: PdaSeed },
    /// A public account that signs the call.
    Signer(Actor),
    /// The caller's own private account, the root of a private call.
    PrivateSelf(Actor),
    /// A public account the call reads or writes that the sender neither signs for nor admits
    /// (another program's account, such as the attestation program's `header(id, slot)`).
    Public(Actor),
}

impl Mention {
    pub fn actor(&self) -> Actor {
        match self {
            Mention::Pda { actor, .. }
            | Mention::Signer(actor)
            | Mention::PrivateSelf(actor)
            | Mention::Public(actor) => *actor,
        }
    }

    /// `pda`, `signer`, `private_self` or `public`.
    pub fn kind(&self) -> &'static str {
        match self {
            Mention::Pda { .. } => "pda",
            Mention::Signer(_) => "signer",
            Mention::PrivateSelf(_) => "private_self",
            Mention::Public(_) => "public",
        }
    }
}

/// Why a call is refused before anything is signed: the code the bridge program would exit with,
/// or a reason of the wallet's own (the chains disagree with each other, a missing input).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The program would refuse with this code (`ferrata_lez_bridge_wire::code`).
    Program { code: u8, detail: String },
    /// The wallet stops: the data do not fit together.
    Wallet(String),
}

impl Refusal {
    pub fn program(code: u8, detail: impl Into<String>) -> Self {
        Refusal::Program { code, detail: detail.into() }
    }

    pub fn wallet(detail: impl Into<String>) -> Self {
        Refusal::Wallet(detail.into())
    }
}

impl core::fmt::Display for Refusal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Refusal::Program { code, detail } => write!(f, "the program would refuse with code {code}: {detail}"),
            Refusal::Wallet(detail) => f.write_str(detail),
        }
    }
}
