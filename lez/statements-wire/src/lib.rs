//! `ferrata_statements`: Ferrata's verified statements recorded to public accounts (RFP-021 soft requirement 1,
//! "Recorded-statement account"), and `ferrata_statements_reader`, a consumer that reads them.
//!
//! **The recorder.** `Record { ferrata, slot, request }` is sent to `statement(ferrata, slot, query)`, the actor of
//! the recorder whose key is the request without its proofs (the question, not the evidence). The recorder sends the
//! attestation program's own `Attest { request }` to `header(ferrata, request.instance_id, slot)`: the direct path,
//! unchanged, so both paths share one verification core and its audit surface. A refusal there ends the whole
//! transaction with the attestation program's code, so nothing is written for a statement that was not proven. The
//! reply, accepted only from that record while the recorder waits, is kept as it came: a [`Recorded`] entry, once.
//! `Read`, from any actor, is answered with the entry (a [`Recorded`], borsh) and changes nothing.
//!
//! **Who is trusted.** A consumer that reads an entry trusts the program that wrote it. Here that is this recorder's
//! code, which writes only the reply of the attestation program named in the entry's key: no submitter can write an
//! entry. A consumer must therefore fix the recorder's program id and the attestation program's id in immutable
//! state (the rule of RFP-021 Verification Security #3 applies to both), derive the entry's actor itself, and check
//! the entry's instance and chain as it would check a direct attestation. The direct path asks no trust in a
//! recorder at all. An entry is a statement about one finalized block, recorded once: it does not become more
//! recent; a consumer that needs a recent block checks `block_number`, as with the direct path.
//!
//! **The reader** (`ferrata_statements_reader`): `Use { recorder, key, ferrata, instance_id, chain_id,
//! statement_id, tag }` at `use(config, statement_id)`, `config` = [`use_config`] of everything it trusts: sends
//! `Read` to `statement(key)` of `recorder`, accepts the reply only from that actor, checks the entry's program, key,
//! instance and chain and that it holds `statement_id`, and keeps [`Used`]. The actor is keyed on the statement
//! identifier (RFP-021 Functionality #6) under one trust configuration, so a statement is used once per
//! configuration: the consumer pattern's nullifier, on the recorded path.

use borsh::{BorshDeserialize, BorshSerialize};
use ferrata_lez_wire::attest::{Attestation, Request};
use lee_core::{
    account::{AccountId, Actor},
    program::PdaSeed,
};
use sha2::{Digest, Sha256};

/// Root and hash type of the attestation program (32 bytes).
pub type Root = [u8; 32];

const DOMAIN: &[u8] = b"ferrata-statements/v1/";

fn tagged_hash(tag: &[u8], parts: &[&[u8]]) -> Root {
    let mut h = Sha256::new();
    h.update(DOMAIN);
    h.update(tag);
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

/// The question a request asks: the request with every proof cleared (account, storage and log proofs, leaf paths,
/// hops' proofs) and no execution header. Two requests that differ only in their evidence ask the same question.
pub fn query(request: &Request) -> Request {
    let mut q = request.clone();
    q.header = None;
    for a in &mut q.accounts {
        a.proof.clear();
        for s in &mut a.slots {
            s.proof.clear();
        }
    }
    for l in &mut q.logs {
        l.proof.clear();
    }
    for l in &mut q.leaves {
        l.path.clear();
    }
    for h in &mut q.hops {
        h.account_proof.clear();
        h.storage_proof.clear();
    }
    q
}

/// The key of a recorded statement: the attestation program, the record's slot, and the question.
pub fn statement_key(ferrata: &AccountId, slot: u64, request: &Request) -> Root {
    let q = borsh::to_vec(&query(request)).expect("borsh serialization is infallible");
    tagged_hash(b"statement", &[ferrata.as_ref(), &slot.to_be_bytes(), &q])
}

pub fn statement_seed(key: &Root) -> PdaSeed {
    PdaSeed::new(tagged_hash(b"statement-actor", &[key]))
}

/// `statement(key)` of the recorder deployed at `recorder`.
pub fn statement_actor(recorder: AccountId, key: &Root) -> Actor {
    Actor::new(AccountId::for_public_pda(&recorder, &statement_seed(key)), recorder)
}

/// The reader's trust configuration as one value: the recorder and attestation program it reads, the instance and
/// chain it accepts, and the consumer's own namespace `tag`. Its use actors are keyed on it, so an entry from any
/// other recorder, program, instance or chain can only ever land in another configuration's actors.
pub fn use_config(recorder: &AccountId, ferrata: &AccountId, instance_id: &Root, chain_id: u64, tag: &Root) -> Root {
    tagged_hash(b"use-config", &[recorder.as_ref(), ferrata.as_ref(), instance_id, &chain_id.to_be_bytes(), tag])
}

pub fn use_seed(config: &Root, statement_id: &Root) -> PdaSeed {
    PdaSeed::new(tagged_hash(b"use", &[config, statement_id]))
}

/// `use(config, statement_id)` of the reader deployed at `reader`.
pub fn use_actor(reader: AccountId, config: &Root, statement_id: &Root) -> Actor {
    Actor::new(AccountId::for_public_pda(&reader, &use_seed(config, statement_id)), reader)
}

/// Messages of the recorder.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum StatementsMessage {
    /// Record the statements `request` yields at `header(ferrata, request.instance_id, slot)`.
    Record { ferrata: AccountId, slot: u64, request: Request },
    /// Reply with the entry.
    Read,
}

/// The recorder's wait for the attestation program's reply (never outlives its transaction).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Pending {
    pub ferrata: AccountId,
    pub record: Actor,
    pub key: Root,
}

/// A recorded statement: who produced it (the attestation program and its record actor) and its reply, as it came.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Recorded {
    pub ferrata: AccountId,
    pub record: Actor,
    pub key: Root,
    pub attestation: Attestation,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum StatementsState {
    Pending(Pending),
    Recorded(Recorded),
}

/// Messages of the reader.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ReaderMessage {
    /// Use `statement_id` from `statement(key)` of `recorder`, once per `tag`.
    Use {
        recorder: AccountId,
        key: Root,
        ferrata: AccountId,
        instance_id: Root,
        chain_id: u64,
        statement_id: Root,
        tag: Root,
    },
}

/// The reader's wait for the recorder's reply (never outlives its transaction).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Waiting {
    pub entry: Actor,
    pub key: Root,
    pub ferrata: AccountId,
    pub instance_id: Root,
    pub chain_id: u64,
    pub statement_id: Root,
}

/// A statement used once: what the reader keeps.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Used {
    pub statement_id: Root,
    pub block_number: u64,
    pub block_hash: Root,
    pub entry: Actor,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ReaderState {
    Waiting(Waiting),
    Used(Used),
}

/// The exit codes `ferrata_statements` and its reader own: 62–72, a block of their own beside the light client's
/// 1–61 and the rebate program's 73–79 (`ferrata_lez_wire::CODE_BLOCKS` names them; `harness/tests/exit_codes.rs`
/// checks that no other program's code is in it). A `Record` that the attestation program refuses ends with that
/// program's own code (100–217).
pub const CODE_BLOCK: core::ops::RangeInclusive<u8> = 62..=72;

macro_rules! exit_codes {
    ($($(#[doc = $doc:literal])* $name:ident = $value:literal;)*) => {
        $($(#[doc = $doc])* pub const $name: u8 = $value;)*
        /// Every code of this module, with its name.
        pub const ALL: &[(&str, u8)] = &[$((stringify!($name), $name)),*];
    };
}

/// Exit codes of the recorder and the reader.
pub mod code {
    exit_codes! {
        /// The message does not decode.
        BAD_MESSAGE = 62;
        /// Stored state that does not decode.
        BAD_STATE = 63;
        /// The receiver is not the actor the message is for.
        WRONG_RECEIVER = 64;
        /// A reply from an actor other than the one waited for, or a `Read` with no sender to reply to.
        WRONG_SENDER = 65;
        /// A reply that does not decode.
        BAD_REPLY = 66;
        /// `Record` of a statement already recorded.
        ALREADY_RECORDED = 67;
        /// `Read` of a statement not recorded.
        NOT_RECORDED = 68;
        /// The reader: the entry is of another attestation program, instance or chain, or another key.
        WRONG_SOURCE = 69;
        /// The reader: the entry does not hold the statement.
        NO_SUCH_STATEMENT = 70;
        /// The reader: the statement was already used under this configuration.
        ALREADY_USED = 71;
    }
}

/// The reader's acceptance of an entry: the source it trusts, and the statement it wants.
pub fn accept(w: &Waiting, entry: &Recorded) -> Result<Used, u8> {
    if entry.ferrata != w.ferrata
        || entry.key != w.key
        || entry.attestation.instance_id != w.instance_id
        || entry.attestation.chain_id != w.chain_id
    {
        return Err(code::WRONG_SOURCE);
    }
    if !entry.attestation.statements.iter().any(|s| s.id == w.statement_id) {
        return Err(code::NO_SUCH_STATEMENT);
    }
    Ok(Used {
        statement_id: w.statement_id,
        block_number: entry.attestation.block_number,
        block_hash: entry.attestation.block_hash,
        entry: w.entry,
    })
}

/// The statements of an entry, for tables.
pub fn statement_ids(entry: &Recorded) -> Vec<Root> {
    entry.attestation.statements.iter().map(|s| s.id).collect()
}
