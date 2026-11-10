//! A strict parser for LEZ v0.4.0-rc1 blocks as they are inscribed: `borsh(Block)`
//! (`lez/common/src/block.rs`), whose transactions are `borsh(LeeTransaction)`
//! (`lez/common/src/transaction.rs`, `lee/state_machine/src/{public,privacy_preserving}_transaction`,
//! `lee/state_machine/core/src`).
//!
//! The parser walks every field of every transaction (borsh is not self-delimiting) and reports
//! two things to a [`Visitor`]: each transaction's exact byte span, and every *visible call*, i.e.
//! a message whose target and bytes are in the clear in the block:
//! - the root call of a public transaction;
//! - the `public_root` of a privacy-preserving transaction;
//! - every `PrivateToPublic` / `PublicToPrivate` boundary delivery of a privacy-preserving
//!   transaction (and those of its fee fallback, reported with their own [`Origin`]).
//!
//! Calls made by a program to another program inside a public execution are not in the block and
//! cannot be seen; see README ("transaction shapes").
//!
//! Strictness: tags, option and bool bytes, set/map ordering (LEZ decodes with borsh's
//! `de_strict_order`), lengths and trailing bytes are checked as upstream's decoder checks them.
//! One upstream check is not repeated: `lee::PublicKey` (BIP-340 x-only secp256k1 keys in witness
//! sets and admission evidence) is validated on decode upstream and taken as 32 opaque bytes here.

use alloc::vec::Vec;

use crate::hash::sha256_parts;
use crate::reader::Reader;
use crate::{Error, Hash};

/// `HashableBlockData::compute_hash`'s domain prefix (`lez/common/src/block.rs:137`).
pub const BLOCK_HASH_PREFIX: &[u8; 32] = b"/LEE/v0.3/Message/Block/\x00\x00\x00\x00\x00\x00\x00\x00";

/// `block_id(8) ‖ prev_block_hash(32) ‖ hash(32) ‖ timestamp(8)`: the bytes before the body.
pub const HEADER_LEN: usize = 80;

/// Bytes of a `FeeDeclaration`: payer(32) ‖ gas_limit u64 ‖ tip u64 ‖ max_fee u128.
const FEE_DECLARATION_LEN: usize = 32 + 8 + 8 + 16;
/// Bytes of one witness entry: Signature(64) ‖ PublicKey(32).
const WITNESS_LEN: usize = 64 + 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub block_id: u64,
    pub prev_block_hash: Hash,
    /// The hash the block claims; [`hash_parsed_block`] recomputes it.
    pub hash: Hash,
    pub timestamp: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxKind {
    Public,
    PrivacyPreserving,
}

/// Where a visible call sits in its transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// `PublicTransaction.message.execution.root`.
    PublicRoot,
    /// `ProvenExecution.public_root` of a privacy-preserving transaction.
    PrivacyRoot,
    /// A `BoundaryStep::PrivateToPublic` delivery (a private actor's message to a public actor).
    PrivateToPublic,
    /// A `BoundaryStep::PublicToPrivate` delivery (a public actor's message to a private actor).
    PublicToPrivate,
    /// A `PrivateToPublic` step of `ProvenFee.fallback.boundary`, which applies only when the
    /// transaction's public part fails.
    FallbackPrivateToPublic,
    /// A `PublicToPrivate` step of the fee fallback.
    FallbackPublicToPrivate,
}

impl Origin {
    /// Whether the call is executed by a *public* actor of its target program in the
    /// transaction's normal (non-fallback) settlement.
    pub const fn is_public_execution(self) -> bool {
        matches!(self, Self::PublicRoot | Self::PrivacyRoot | Self::PrivateToPublic)
    }
}

/// `lee_core::account::Actor`: `(account_id, program_account_id)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Actor<'a> {
    pub account_id: &'a Hash,
    pub program_account_id: &'a Hash,
}

impl<'a> Actor<'a> {
    fn read(r: &mut Reader<'a>) -> Result<Self, Error> {
        Ok(Self { account_id: r.array()?, program_account_id: r.array()? })
    }
}

/// Receives what the parser finds. Both methods default to doing nothing.
pub trait Visitor<'a> {
    /// Once per transaction, after it parsed, with its exact `borsh(LeeTransaction)` bytes.
    fn tx(&mut self, _index: u32, _kind: TxKind, _bytes: &'a [u8]) {}
    /// Once per visible call, in encoding order. `from` is `None` for root calls.
    fn call(&mut self, _index: u32, _origin: Origin, _from: Option<Actor<'a>>, _to: Actor<'a>, _message: &'a [u8]) {}
}

/// A visitor that ignores everything.
pub struct NoVisitor;

impl Visitor<'_> for NoVisitor {}

/// What a parsed block holds besides its transactions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockSummary {
    pub header: Header,
    pub tx_count: u32,
    /// Strictly ascending positions of reverted transactions (`BlockBody::reverted`).
    pub reverted: Vec<u64>,
    /// `BedrockStatus` (0 Pending, 1 Safe, 2 Finalized); not covered by the hash.
    pub status: u8,
}

impl BlockSummary {
    pub fn is_reverted(&self, index: u32) -> bool {
        self.reverted.binary_search(&u64::from(index)).is_ok()
    }
}

/// Parses `borsh(Block)` exactly (no trailing bytes), reporting to `v`.
///
/// Beyond borsh, it requires `reverted` to be strictly ascending and to name existing
/// transactions; LEZ's `apply_block` requires the list to equal the ascending list settlement
/// produces, so no valid block fails this.
pub fn parse_block<'a, V: Visitor<'a>>(bytes: &'a [u8], v: &mut V) -> Result<BlockSummary, Error> {
    let mut r = Reader::new(bytes);
    let header =
        Header { block_id: r.u64_le()?, prev_block_hash: *r.array()?, hash: *r.array()?, timestamp: r.u64_le()? };
    let n = r.len_u32("transactions", 1)?;
    // `len_u32` bounded n by the input length, which is far below u32::MAX.
    let n = u32::try_from(n).map_err(|_| Error::BadLength { what: "transactions" })?;
    for index in 0..n {
        let start = r.pos();
        let kind = match r.u8()? {
            0 => {
                public_transaction(&mut r, index, v)?;
                TxKind::Public
            }
            1 => {
                privacy_preserving_transaction(&mut r, index, v)?;
                TxKind::PrivacyPreserving
            }
            tag => return Err(Error::BadTag { what: "LeeTransaction", tag }),
        };
        v.tx(index, kind, r.since(start));
    }
    let m = r.len_u32("reverted", 8)?;
    let mut reverted = Vec::with_capacity(m);
    for _ in 0..m {
        let position = r.u64_le()?;
        let ascending = reverted.last().is_none_or(|&last| position > last);
        if !ascending || position >= u64::from(n) {
            return Err(Error::BadRevertedList { block: header.block_id });
        }
        reverted.push(position);
    }
    let status = r.u8()?;
    if status > 2 {
        return Err(Error::BadTag { what: "BedrockStatus", tag: status });
    }
    r.finish()?;
    Ok(BlockSummary { header, tx_count: n, reverted, status })
}

/// The LEZ block hash of bytes that [`parse_block`] accepted:
/// `SHA256(PREFIX ‖ borsh(block_id, prev_block_hash, timestamp, transactions, reverted))`.
///
/// The inscribed `borsh(Block)` differs from the hashed `borsh(HashableBlockData)` only by the
/// stored `header.hash` (bytes 40..72) and the trailing `bedrock_status` byte, so the hash input
/// is `bytes[0..40] ‖ bytes[72..len-1]`, streamed without a copy.
pub fn hash_parsed_block(bytes: &[u8]) -> Result<Hash, Error> {
    if bytes.len() < HEADER_LEN + 1 {
        return Err(Error::Truncated { at: bytes.len() });
    }
    Ok(sha256_parts(&[BLOCK_HASH_PREFIX, &bytes[..40], &bytes[72..bytes.len() - 1]]))
}

// ---- transactions ---------------------------------------------------------------------------------

/// `PublicTransaction { message: TransactionMessage<PublicExecution>, witness_set }`.
fn public_transaction<'a, V: Visitor<'a>>(r: &mut Reader<'a>, index: u32, v: &mut V) -> Result<(), Error> {
    public_execution_context(r)?;
    // PublicExecution { root: RootCall { to, message }, fee: Option<FeeDeclaration> }
    let to = Actor::read(r)?;
    let message = r.bytes_u32("root message")?;
    v.call(index, Origin::PublicRoot, None, to, message);
    if r.option("fee declaration")? {
        r.skip(FEE_DECLARATION_LEN)?;
    }
    nonces(r)?;
    admission_evidence(r)?;
    // WitnessSet { signatures_and_public_keys: Vec<(Signature, PublicKey)> }
    let k = r.len_u32("witness set", WITNESS_LEN)?;
    r.skip(k * WITNESS_LEN)
}

/// `PrivacyPreservingTransaction { message: TransactionMessage<ProvenExecution>, witness_set }`.
fn privacy_preserving_transaction<'a, V: Visitor<'a>>(r: &mut Reader<'a>, index: u32, v: &mut V) -> Result<(), Error> {
    public_execution_context(r)?;
    proven_execution(r, index, v)?;
    nonces(r)?;
    admission_evidence(r)?;
    // WitnessSet { signatures_and_public_keys, proof: Proof(Vec<u8>) }
    let k = r.len_u32("witness set", WITNESS_LEN)?;
    r.skip(k * WITNESS_LEN)?;
    r.bytes_u32("proof").map(|_| ())
}

/// `PublicExecutionContext { actors: BTreeSet<Actor>, authorized_accounts: BTreeSet<AccountId>,
/// promotions: BTreeSet<u64> }`.
fn public_execution_context(r: &mut Reader<'_>) -> Result<(), Error> {
    strict_set::<64>(r, "actors")?;
    strict_set::<32>(r, "authorized_accounts")?;
    let n = r.len_u32("promotions", 8)?;
    let mut last: Option<u64> = None;
    for _ in 0..n {
        let x = r.u64_le()?;
        if last.is_some_and(|last| x <= last) {
            return Err(Error::NotStrictlyOrdered { what: "promotions" });
        }
        last = Some(x);
    }
    Ok(())
}

/// A borsh `BTreeSet` of fixed-size byte keys (`AccountId`, `Actor`, `PdaSeed`), whose `Ord` is
/// lexicographic on the bytes.
fn strict_set<const N: usize>(r: &mut Reader<'_>, what: &'static str) -> Result<(), Error> {
    let n = r.len_u32(what, N)?;
    let mut last: Option<&[u8; N]> = None;
    for _ in 0..n {
        let key = r.array::<N>()?;
        if last.is_some_and(|last| key <= last) {
            return Err(Error::NotStrictlyOrdered { what });
        }
        last = Some(key);
    }
    Ok(())
}

/// `nonces: BTreeMap<AccountId, Nonce>`; a nonce is a little-endian `u128`.
fn nonces(r: &mut Reader<'_>) -> Result<(), Error> {
    let n = r.len_u32("nonces", 48)?;
    let mut last: Option<&[u8; 32]> = None;
    for _ in 0..n {
        let key = r.array::<32>()?;
        if last.is_some_and(|last| key <= last) {
            return Err(Error::NotStrictlyOrdered { what: "nonces" });
        }
        last = Some(key);
        r.skip(16)?;
    }
    Ok(())
}

/// `admission_evidence: Vec<PublicAccountEvidence>`: `Key(PublicKey)` | `Pda { program, seed }`.
fn admission_evidence(r: &mut Reader<'_>) -> Result<(), Error> {
    let n = r.len_u32("admission_evidence", 33)?;
    for _ in 0..n {
        match r.u8()? {
            0 => r.skip(32)?,
            1 => r.skip(64)?,
            tag => return Err(Error::BadTag { what: "PublicAccountEvidence", tag }),
        }
    }
    Ok(())
}

/// `ProvenExecution` (`lee/state_machine/core/src/circuit_io.rs`).
fn proven_execution<'a, V: Visitor<'a>>(r: &mut Reader<'a>, index: u32, v: &mut V) -> Result<(), Error> {
    boundary(r, index, v, false)?;
    // async_messages: Vec<SealedMessage { commitment, note }>
    let n = r.len_u32("async_messages", 40)?;
    for _ in 0..n {
        r.skip(32)?;
        encrypted_note(r)?;
    }
    // recovery_bindings: Vec<RecoveryBinding { address, note }>
    let n = r.len_u32("recovery_bindings", 40)?;
    for _ in 0..n {
        r.skip(32)?;
        encrypted_note(r)?;
    }
    // public_root: Option<RootCall>
    if r.option("public_root")? {
        let to = Actor::read(r)?;
        let message = r.bytes_u32("public_root message")?;
        v.call(index, Origin::PrivacyRoot, None, to, message);
    }
    // private_actions: Vec<PrivateAction { nullifier, root, commitment, encrypted_post_state }>
    let n = r.len_u32("private_actions", 104)?;
    for _ in 0..n {
        private_action(r)?;
    }
    // validity: ValidityWindows { blocks, timestamps }, each { from: Option<u64>, to: Option<u64> }
    for _ in 0..4 {
        if r.option("validity bound")? {
            r.skip(8)?;
        }
    }
    // program_image_claims: Vec<ProgramImageClaim>
    let n = r.len_u32("program_image_claims", 33)?;
    for _ in 0..n {
        match r.u8()? {
            // Disclosed { account_id, image_id: [u32; 8] }
            0 => r.skip(64)?,
            // Undisclosed { root }
            1 => r.skip(32)?,
            tag => return Err(Error::BadTag { what: "ProgramImageClaim", tag }),
        }
    }
    // fee: Option<ProvenFee { refund_to: Option<AccountId>, fallback: Option<FeeFallback> }>
    if r.option("fee")? {
        if r.option("refund_to")? {
            r.skip(32)?;
        }
        if r.option("fallback")? {
            // FeeFallback { action: PrivateAction, boundary: Boundary }
            private_action(r)?;
            boundary(r, index, v, true)?;
        }
    }
    Ok(())
}

fn private_action(r: &mut Reader<'_>) -> Result<(), Error> {
    r.skip(96)?;
    encrypted_note(r)
}

/// `EncryptedNote { ciphertext: Ciphertext(Vec<u8>), epk: EphemeralPublicKey(Vec<u8>) }`.
fn encrypted_note(r: &mut Reader<'_>) -> Result<(), Error> {
    r.bytes_u32("ciphertext")?;
    r.bytes_u32("epk")?;
    Ok(())
}

/// `Boundary = Vec<BoundaryStep>`.
fn boundary<'a, V: Visitor<'a>>(r: &mut Reader<'a>, index: u32, v: &mut V, fallback: bool) -> Result<(), Error> {
    let n = r.len_u32("boundary", 1)?;
    for _ in 0..n {
        match r.u8()? {
            0 => {
                let origin = if fallback { Origin::FallbackPrivateToPublic } else { Origin::PrivateToPublic };
                delivery(r, index, v, origin)?;
            }
            1 => {
                let origin = if fallback { Origin::FallbackPublicToPrivate } else { Origin::PublicToPrivate };
                delivery(r, index, v, origin)?;
            }
            // EndPrivateSubtree, EndPublicSubtree
            2 | 3 => {}
            tag => return Err(Error::BadTag { what: "BoundaryStep", tag }),
        }
    }
    Ok(())
}

/// `Delivery<Actor> { envelope: MessageEnvelope<Actor> { from, to, message },
/// inherited_authorizations: BTreeSet<AccountId>, inherits_entry_authorizations: bool,
/// pda_seeds: BTreeSet<PdaSeed> }`.
fn delivery<'a, V: Visitor<'a>>(r: &mut Reader<'a>, index: u32, v: &mut V, origin: Origin) -> Result<(), Error> {
    let from = Actor::read(r)?;
    let to = Actor::read(r)?;
    let message = r.bytes_u32("delivery message")?;
    v.call(index, origin, Some(from), to, message);
    strict_set::<32>(r, "inherited_authorizations")?;
    r.bool("inherits_entry_authorizations")?;
    strict_set::<32>(r, "pda_seeds")
}
