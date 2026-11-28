//! A strict parser for LEZ v0.4.0 blocks (`a255b42c`) as they are inscribed: `borsh(Block)`, whose transactions are
//! `borsh(LeeTransaction)`, and the block hash over v0.4.0's `HashableBlockData`. Built only with the cargo feature
//! `lez-0-4-0` (off by default), which implies `bedrock-0-3-5`: a v0.4.0 zone runs on the Logos Blockchain 0.4.0-rc.2
//! node LEZ v0.4.0 pins, whose Mantle transactions carry a fork digest as 0.3.5-rc.1's do ([`rule`]). The default build
//! ([`crate::lez`], v0.4.0-rc1's layout) and the `bedrock-0-3-5` build do not compile this module.
//!
//! The interface is [`crate::lez`]'s: [`parse_block`] walks every field (borsh is not self-delimiting) and reports each
//! transaction's exact byte span and every *visible call* (a message whose target and bytes are in the clear in the
//! block) to a [`Visitor`], with [`crate::lez`]'s [`Header`], [`Actor`], [`TxKind`] and [`NoVisitor`]. What v0.4.0
//! changes in the encoding, against v0.4.0-rc1's (rc2 and rc3 encode as rc1):
//! - `BlockBody.reverted: Vec<u64>` is now `receipts: Vec<TransactionReceipt>`, one per user transaction (LEZ #1107):
//!   `success: bool ‖ FeeReceipt { gas_exec u64, gas_stor u64, fee_base u128, tip u128, refund u128 }`, 65 bytes.
//!   [`BlockSummary::is_reverted`] reads `success`, as `BlockBody::is_reverted` does.
//! - `PublicExecutionContext.actors` is a `BTreeMap<Actor, Authority>`, an `Authority` byte (`None` 0, `Signer` 1)
//!   after each actor (LEZ #1144), and the context gains `private_signers: BTreeSet<Actor>`.
//! - `TransactionMessage` carries `fee: Option<FeeDeclaration>` after `nonces`, in both transaction kinds (LEZ #1080);
//!   `PublicExecution` is the root call alone.
//! - Witness sets are `Vec<WitnessEntry { auth_data: Vec<u8>, next: SigningKeyState }>` (LEZ #1135), and
//!   `PublicAccountEvidence::Key` holds a `SigningKeyState` (`key_type` byte, which must be `Schnorr` = 0, then a
//!   32-byte commitment) in place of a secp256k1 public key.
//! - `Delivery` carries `grants: BTreeSet<Actor>` in place of `inherited_authorizations: BTreeSet<AccountId>` and
//!   `inherits_entry_authorizations: bool`.
//! - `ProvenExecution.async_messages` are `Publication`s: `Clear(MessageBody)` 0 or `Sealed(SealedMessage)` 1. A clear
//!   one is reported as a visible call ([`Origin::ClearPublication`]).
//! - `ProvenFee` gains `tip: u64` before `fallback`, and `FeeFallback` is `nullifier ‖ root ‖ commitment ‖
//!   ciphertext ‖ boundary` (the payer's action without its ephemeral key) in place of `action: PrivateAction ‖
//!   boundary`.
//!
//! The block header, `HashableBlockData`'s shape (the header without its hash, then the body) and the hash's domain
//! are unchanged, so [`hash_parsed_block`] hashes the same byte ranges as [`crate::lez::hash_parsed_block`].
//!
//! Strictness is LEZ's own decoder's, which reads inscriptions with `borsh::from_slice::<Block>`
//! (`lez/sequencer/actors/bedrock/src/actor.rs:775-776`): enum tags in range, option and bool bytes 0 or 1, every
//! `BTreeMap`/`BTreeSet` in strictly ascending key order (lee_core decodes with borsh's `de_strict_order`:
//! `lee/state_machine/core/Cargo.toml:12`), lengths within the input, no trailing bytes. Nothing in the v0.4.0 layout
//! is checked further on decode (v0.4.0-rc1's secp256k1 keys, which [`crate::lez`] takes as opaque, are gone), so the
//! encodings accepted are exactly those LEZ's decoder accepts, less those failing the one check added beyond borsh:
//! the receipts must number exactly the user transactions ([`parse_block`]). Only the error reported for an input
//! wrong in two places may differ from borsh's (borsh reads a whole map before checking its order).
//!
//! Every field read cites its definition at v0.4.0, with `core/` = `lee/state_machine/core/src/`, `lee/` =
//! `lee/state_machine/src/` and `common/` = `lez/common/src/`.

pub mod rule;

use alloc::vec::Vec;

use crate::hash::sha256_parts;
pub use crate::lez::{Actor, Header, NoVisitor, TxKind};
use crate::reader::Reader;
use crate::{Error, Hash};

/// `HashableBlockData::compute_hash`'s domain prefix (`common/block.rs:164`), unchanged since v0.4.0-rc1.
pub const BLOCK_HASH_PREFIX: &[u8; 32] = b"/LEE/v0.3/Message/Block/\x00\x00\x00\x00\x00\x00\x00\x00";

/// `block_id(8) ‖ prev_block_hash(32) ‖ hash(32) ‖ timestamp(8)`: the bytes before the body (`common/block.rs:41-46`).
pub const HEADER_LEN: usize = 80;

/// Bytes of a `FeeDeclaration` (`lee/fees.rs:12-20`): payer(32) ‖ gas_limit u64 ‖ tip u64 ‖ max_fee u128.
const FEE_DECLARATION_LEN: usize = 32 + 8 + 8 + 16;
/// Bytes of a `TransactionReceipt` (`common/block.rs:68-72`): success(1) ‖ `FeeReceipt` (`common/block.rs:53-62`).
const RECEIPT_LEN: usize = 1 + 8 + 8 + 16 + 16 + 16;
/// Bytes of a `SigningKeyState` (`core/signing_key.rs:36-39`): key_type(1) ‖ key_commitment(32).
const SIGNING_KEY_STATE_LEN: usize = 1 + 32;

/// Where a visible call sits in its transaction: [`crate::lez::Origin`]'s places, and a clear publication.
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
    /// A `PrivateToPublic` step of `ProvenFee.fallback.boundary`, which applies only when the transaction's public
    /// part fails.
    FallbackPrivateToPublic,
    /// A `PublicToPrivate` step of the fee fallback.
    FallbackPublicToPrivate,
    /// A `Publication::Clear` of `ProvenExecution.async_messages`: an asynchronous message from a private actor,
    /// published in the clear for a later transaction to receive. Settlement publishes it only to an address bound to
    /// a private account (`lee/validated_state_diff/mod.rs:603-607`, `UnboundAsyncDestination`), so no public actor
    /// executes it in this transaction.
    ClearPublication,
}

impl Origin {
    /// Whether the call is executed by a *public* actor of its target program in the transaction's normal
    /// (non-fallback) settlement ([`crate::lez::Origin::is_public_execution`]; a clear publication is not).
    pub const fn is_public_execution(self) -> bool {
        matches!(self, Self::PublicRoot | Self::PrivacyRoot | Self::PrivateToPublic)
    }
}

/// Receives what the parser finds ([`crate::lez::Visitor`]'s methods, with v0.4.0's [`Origin`]). Both default to
/// doing nothing.
pub trait Visitor<'a> {
    /// Once per transaction, after it parsed, with its exact `borsh(LeeTransaction)` bytes.
    fn tx(&mut self, _index: u32, _kind: TxKind, _bytes: &'a [u8]) {}
    /// Once per visible call, in encoding order. `from` is `None` for root calls.
    fn call(&mut self, _index: u32, _origin: Origin, _from: Option<Actor<'a>>, _to: Actor<'a>, _message: &'a [u8]) {}
}

impl Visitor<'_> for NoVisitor {}

/// `FeeReceipt` (`common/block.rs:53-62`): the fee one transaction paid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeeReceipt {
    pub gas_exec: u64,
    pub gas_stor: u64,
    pub fee_base: u128,
    pub tip: u128,
    pub refund: u128,
}

/// `TransactionReceipt` (`common/block.rs:68-72`): the outcome of one user transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransactionReceipt {
    /// `false`: the action reverted (its fee was still charged).
    pub success: bool,
    pub fee: FeeReceipt,
}

impl TransactionReceipt {
    fn read(r: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(Self {
            success: r.bool("receipt success")?, // common/block.rs:70
            fee: FeeReceipt {
                gas_exec: r.u64_le()?, // common/block.rs:55, Gas = u64 (core/account.rs:88)
                gas_stor: r.u64_le()?, // common/block.rs:57
                fee_base: u128_le(r)?, // common/block.rs:58, Balance = u128 (core/account.rs:83)
                tip: u128_le(r)?,      // common/block.rs:60
                refund: u128_le(r)?,   // common/block.rs:61
            },
        })
    }
}

/// What a parsed block holds besides its transactions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockSummary {
    pub header: Header,
    pub tx_count: u32,
    /// `BlockBody::receipts`, one per user transaction in transaction order; the trailing fee and clock transactions
    /// have none (`common/block.rs:91-93`).
    pub receipts: Vec<TransactionReceipt>,
    /// `BedrockStatus` (0 Pending, 1 Safe, 2 Finalized); not covered by the hash.
    pub status: u8,
}

impl BlockSummary {
    /// `BlockBody::is_reverted` (`common/block.rs:97-103`): the transaction's receipt says it did not succeed; one
    /// without a receipt (the fee and clock transactions) did not revert.
    pub fn is_reverted(&self, index: u32) -> bool {
        usize::try_from(index).ok().and_then(|i| self.receipts.get(i)).is_some_and(|receipt| !receipt.success)
    }
}

/// Parses `borsh(Block)` exactly (no trailing bytes), reporting to `v`.
///
/// Beyond borsh, it requires one receipt per user transaction: at least the two trailing transactions (the fee and
/// clock transactions, which have no receipt) and `receipts.len() == transactions.len() - 2`, refused otherwise with
/// [`Error::BadRevertedList`] (the receipts carry what `reverted` carried). LEZ's `apply_block` requires the clock and
/// fee transactions last (`lez/chain_state/src/apply.rs:161-176`) and the receipts to equal, one by one, those that
/// settling the user transactions produces (`lez/chain_state/src/apply.rs:198-259`), so no valid block fails this; and
/// a receipt cannot go missing, which [`BlockSummary::is_reverted`] would read as success.
pub fn parse_block<'a, V: Visitor<'a>>(bytes: &'a [u8], v: &mut V) -> Result<BlockSummary, Error> {
    let mut r = Reader::new(bytes);
    // Block.header: BlockHeader (common/block.rs:115, 41-46)
    let header = Header {
        block_id: r.u64_le()?,        // common/block.rs:42, BlockId = u64 (core/lib.rs:44)
        prev_block_hash: *r.array()?, // common/block.rs:43
        hash: *r.array()?,            // common/block.rs:44
        timestamp: r.u64_le()?,       // common/block.rs:45, Timestamp = u64 (core/lib.rs:46)
    };
    // Block.body: BlockBody (common/block.rs:116, 89-94); transactions: Vec<LeeTransaction> (common/block.rs:90)
    let n = r.len_u32("transactions", 1)?;
    // `len_u32` bounded n by the input length, which is far below u32::MAX.
    let n = u32::try_from(n).map_err(|_| Error::BadLength { what: "transactions" })?;
    for index in 0..n {
        let start = r.pos();
        // LeeTransaction (common/transaction.rs:23-26)
        let kind = match r.u8()? {
            0 => {
                public_transaction(&mut r, index, v)?; // common/transaction.rs:24
                TxKind::Public
            }
            1 => {
                privacy_preserving_transaction(&mut r, index, v)?; // common/transaction.rs:25
                TxKind::PrivacyPreserving
            }
            tag => return Err(Error::BadTag { what: "LeeTransaction", tag }),
        };
        v.tx(index, kind, r.since(start));
    }
    // receipts: Vec<TransactionReceipt> (common/block.rs:93)
    let m = r.len_u32("receipts", RECEIPT_LEN)?;
    let mut receipts = Vec::with_capacity(m);
    for _ in 0..m {
        receipts.push(TransactionReceipt::read(&mut r)?);
    }
    // Block.bedrock_status: BedrockStatus (common/block.rs:117, 107-111)
    let status = r.u8()?;
    if status > 2 {
        return Err(Error::BadTag { what: "BedrockStatus", tag: status });
    }
    r.finish()?;
    let user_transactions = n.checked_sub(2).and_then(|users| usize::try_from(users).ok());
    if user_transactions != Some(receipts.len()) {
        return Err(Error::BadRevertedList { block: header.block_id });
    }
    Ok(BlockSummary { header, tx_count: n, receipts, status })
}

/// The LEZ v0.4.0 block hash of bytes that [`parse_block`] accepted: `SHA256(PREFIX ‖ borsh(HashableBlockData {
/// block_id, prev_block_hash, timestamp, transactions, receipts }))` (`common/block.rs:149-156`, `compute_hash` at
/// `common/block.rs:163-176`).
///
/// The inscribed `borsh(Block)` (header, then `transactions ‖ receipts`, then the status) differs from the hashed
/// encoding only by the stored `header.hash` (bytes 40..72) and the trailing `bedrock_status` byte, so the hash input
/// is `bytes[0..40] ‖ bytes[72..len-1]`, streamed without a copy. (Borsh with strictly ordered maps is canonical: the
/// accepted bytes are the re-encoding of what they decode to.)
pub fn hash_parsed_block(bytes: &[u8]) -> Result<Hash, Error> {
    if bytes.len() < HEADER_LEN + 1 {
        return Err(Error::Truncated { at: bytes.len() });
    }
    Ok(sha256_parts(&[BLOCK_HASH_PREFIX, &bytes[..40], &bytes[72..bytes.len() - 1]]))
}

// ---- transactions ---------------------------------------------------------------------------------

/// `PublicTransaction { message: Message, witness_set: WitnessSet }` (`lee/public_transaction/transaction.rs:16-19`),
/// `Message = TransactionMessage<PublicExecution>` (`lee/public_transaction/message.rs:24`).
fn public_transaction<'a, V: Visitor<'a>>(r: &mut Reader<'a>, index: u32, v: &mut V) -> Result<(), Error> {
    // message.public_context (lee/transaction_message.rs:41)
    public_execution_context(r)?;
    // message.execution: PublicExecution { root: RootCall } (lee/transaction_message.rs:43,
    // lee/public_transaction/message.rs:20-22)
    let (to, message) = root_call(r, "root message")?;
    v.call(index, Origin::PublicRoot, None, to, message);
    message_tail(r)?;
    // witness_set: WitnessSet { entries } (lee/public_transaction/transaction.rs:18, witness_set.rs:13-15)
    witness_entries(r)
}

/// `PrivacyPreservingTransaction { message: Message, witness_set: WitnessSet }`
/// (`lee/privacy_preserving_transaction/transaction.rs:14-17`), `Message = TransactionMessage<ProvenExecution>`
/// (`lee/privacy_preserving_transaction/message.rs:11`).
fn privacy_preserving_transaction<'a, V: Visitor<'a>>(r: &mut Reader<'a>, index: u32, v: &mut V) -> Result<(), Error> {
    // message.public_context (lee/transaction_message.rs:41)
    public_execution_context(r)?;
    // message.execution: ProvenExecution (lee/transaction_message.rs:43)
    proven_execution(r, index, v)?;
    message_tail(r)?;
    // witness_set: WitnessSet { entries, proof } (lee/privacy_preserving_transaction/transaction.rs:16,
    // witness_set.rs:10-13)
    witness_entries(r)?;
    // proof: Proof(Vec<u8>) (lee/privacy_preserving_transaction/witness_set.rs:12, circuit/mod.rs:34)
    r.bytes_u32("proof").map(|_| ())
}

/// The fields of `TransactionMessage` after `execution` (`lee/transaction_message.rs:45-50`).
fn message_tail(r: &mut Reader<'_>) -> Result<(), Error> {
    // nonces: BTreeMap<AccountId, Nonce> (lee/transaction_message.rs:45)
    nonces(r)?;
    // fee: Option<FeeDeclaration> (lee/transaction_message.rs:48): payer AccountId, gas_limit Gas = u64, tip Fee = u64,
    // max_fee Balance = u128 (lee/fees.rs:16-19)
    if r.option("fee declaration")? {
        r.skip(FEE_DECLARATION_LEN)?;
    }
    // admission_evidence: Vec<PublicAccountEvidence> (lee/transaction_message.rs:50)
    admission_evidence(r)
}

/// `PublicExecutionContext { actors, authorized_accounts, promotions, private_signers }`
/// (`core/execution_state.rs:54-60`).
fn public_execution_context(r: &mut Reader<'_>) -> Result<(), Error> {
    // actors: BTreeMap<Actor, Authority> (core/execution_state.rs:55). An `Actor` is two AccountIds (core/account.rs:
    // 195-198) whose derived `Ord` is lexicographic on the 64 bytes; `Authority` is `None` 0 | `Signer` 1
    // (core/execution_state.rs:46-49).
    let n = r.len_u32("actors", 64 + 1)?;
    let mut last: Option<&[u8; 64]> = None;
    for _ in 0..n {
        let key = r.array::<64>()?;
        if last.is_some_and(|last| key <= last) {
            return Err(Error::NotStrictlyOrdered { what: "actors" });
        }
        last = Some(key);
        match r.u8()? {
            0 | 1 => {}
            tag => return Err(Error::BadTag { what: "Authority", tag }),
        }
    }
    // authorized_accounts: BTreeSet<AccountId> (core/execution_state.rs:56)
    strict_set::<32>(r, "authorized_accounts")?;
    // promotions: BTreeSet<u64> (core/execution_state.rs:57), ordered as numbers
    let n = r.len_u32("promotions", 8)?;
    let mut last: Option<u64> = None;
    for _ in 0..n {
        let x = r.u64_le()?;
        if last.is_some_and(|last| x <= last) {
            return Err(Error::NotStrictlyOrdered { what: "promotions" });
        }
        last = Some(x);
    }
    // private_signers: BTreeSet<Actor> (core/execution_state.rs:59)
    strict_set::<64>(r, "private_signers")
}

/// A borsh `BTreeSet` of fixed-size byte keys (`AccountId`, `Actor`, `PdaSeed`), whose `Ord` is lexicographic on the
/// bytes.
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

/// `nonces: BTreeMap<AccountId, Nonce>`; a `Nonce` encodes as its little-endian `u128` (`core/account.rs:19, 71-81`).
fn nonces(r: &mut Reader<'_>) -> Result<(), Error> {
    let n = r.len_u32("nonces", 32 + 16)?;
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

/// `PublicAccountEvidence`: `Key(SigningKeyState)` 0 | `Pda { program: AccountId, seed: PdaSeed }` 1
/// (`lee/transaction_message.rs:17-24`).
fn admission_evidence(r: &mut Reader<'_>) -> Result<(), Error> {
    let n = r.len_u32("admission_evidence", 1 + SIGNING_KEY_STATE_LEN)?;
    for _ in 0..n {
        match r.u8()? {
            0 => signing_key_state(r)?,
            // program (lee/transaction_message.rs:21) ‖ seed: PdaSeed([u8; 32]) (lee/transaction_message.rs:22,
            // core/program/mod.rs:116)
            1 => r.skip(32 + 32)?,
            tag => return Err(Error::BadTag { what: "PublicAccountEvidence", tag }),
        }
    }
    Ok(())
}

/// `SigningKeyState { key_type: KeyType, key_commitment: [u8; 32] }` (`core/signing_key.rs:36-39`). `KeyType` has the
/// one variant `Schnorr = 0` (`core/signing_key.rs:14-19`, encoded by its discriminant), so its byte must be 0.
fn signing_key_state(r: &mut Reader<'_>) -> Result<(), Error> {
    match r.u8()? {
        0 => {}
        tag => return Err(Error::BadTag { what: "KeyType", tag }),
    }
    r.skip(32)
}

/// A witness set's `entries: Vec<WitnessEntry>` (`lee/public_transaction/witness_set.rs:14`,
/// `lee/privacy_preserving_transaction/witness_set.rs:11`), each `WitnessEntry { auth_data: Vec<u8>, next:
/// SigningKeyState }` (`lee/public_transaction/witness_set.rs:7-10`). `auth_data` is checked against the signer at
/// admission (`check_auth_data`), not on decode.
fn witness_entries(r: &mut Reader<'_>) -> Result<(), Error> {
    let n = r.len_u32("witness entries", 4 + SIGNING_KEY_STATE_LEN)?;
    for _ in 0..n {
        r.bytes_u32("auth_data")?; // lee/public_transaction/witness_set.rs:8
        signing_key_state(r)?; // lee/public_transaction/witness_set.rs:9
    }
    Ok(())
}

/// `Actor { account_id: AccountId, program_account_id: AccountId }` (`core/account.rs:195-198`), each `AccountId {
/// value: [u8; 32] }` (`core/account.rs:229-231`).
fn actor<'a>(r: &mut Reader<'a>) -> Result<Actor<'a>, Error> {
    Ok(Actor { account_id: r.array()?, program_account_id: r.array()? })
}

/// `RootCall { to: Actor, message: MessageData }` (`core/circuit_io.rs:128-131`; `MessageData = Vec<u8>`,
/// `core/program/mod.rs:59`).
fn root_call<'a>(r: &mut Reader<'a>, what: &'static str) -> Result<(Actor<'a>, &'a [u8]), Error> {
    let to = actor(r)?; // core/circuit_io.rs:129
    let message = r.bytes_u32(what)?; // core/circuit_io.rs:130
    Ok((to, message))
}

/// `MessageEnvelope<Actor> { from: Actor, to: Actor, message: MessageData }` (`core/program/mod.rs:316-320`; it is
/// also `MessageBody`, `core/program/mod.rs:322`).
fn envelope<'a>(r: &mut Reader<'a>, what: &'static str) -> Result<(Actor<'a>, Actor<'a>, &'a [u8]), Error> {
    let from = actor(r)?; // core/program/mod.rs:317
    let to = actor(r)?; // core/program/mod.rs:318
    let message = r.bytes_u32(what)?; // core/program/mod.rs:319
    Ok((from, to, message))
}

/// `ProvenExecution` (`core/circuit_io.rs:543-560`).
fn proven_execution<'a, V: Visitor<'a>>(r: &mut Reader<'a>, index: u32, v: &mut V) -> Result<(), Error> {
    // boundary: Boundary (core/circuit_io.rs:545)
    boundary(r, index, v, false)?;
    // async_messages: Vec<Publication> (core/circuit_io.rs:547): Clear(MessageBody) 0 | Sealed(SealedMessage) 1
    // (core/program/mod.rs:327-332)
    let n = r.len_u32("async_messages", 1 + 32 + 4 + 4)?;
    for _ in 0..n {
        match r.u8()? {
            0 => {
                let (from, to, message) = envelope(r, "publication message")?; // core/program/mod.rs:329
                v.call(index, Origin::ClearPublication, Some(from), to, message);
            }
            1 => {
                // SealedMessage { commitment: Commitment, note: EncryptedNote } (core/program/mod.rs:331,
                // core/sealing.rs:17-20; Commitment([u8; 32]), core/commitment.rs:39)
                r.skip(32)?;
                encrypted_note(r)?;
            }
            tag => return Err(Error::BadTag { what: "Publication", tag }),
        }
    }
    // recovery_bindings: Vec<RecoveryBinding { address: AccountId, note: EncryptedNote }> (core/circuit_io.rs:549,
    // core/recovery.rs:38-41)
    let n = r.len_u32("recovery_bindings", 32 + 4 + 4)?;
    for _ in 0..n {
        r.skip(32)?;
        encrypted_note(r)?;
    }
    // public_root: Option<RootCall> (core/circuit_io.rs:552)
    if r.option("public_root")? {
        let (to, message) = root_call(r, "public_root message")?;
        v.call(index, Origin::PrivacyRoot, None, to, message);
    }
    // private_actions: Vec<PrivateAction> (core/circuit_io.rs:553)
    let n = r.len_u32("private_actions", 96 + 4 + 4)?;
    for _ in 0..n {
        private_action(r)?;
    }
    // validity: ValidityWindows { blocks, timestamps } (core/circuit_io.rs:554, core/program/mod.rs:597-600), each a
    // ValidityWindow { from: Option<u64>, to: Option<u64> } (core/program/mod.rs:604-607)
    for _ in 0..4 {
        if r.option("validity bound")? {
            r.skip(8)?;
        }
    }
    // program_image_claims: Vec<ProgramImageClaim> (core/circuit_io.rs:557, 98-105)
    let n = r.len_u32("program_image_claims", 1 + 32)?;
    for _ in 0..n {
        match r.u8()? {
            // Disclosed { account_id: AccountId, image_id: ProgramId = [u32; 8] } (core/circuit_io.rs:99-102,
            // core/program/mod.rs:34)
            0 => r.skip(32 + 32)?,
            // Undisclosed { root: CommitmentSetDigest = [u8; 32] } (core/circuit_io.rs:104, core/commitment.rs:106)
            1 => r.skip(32)?,
            tag => return Err(Error::BadTag { what: "ProgramImageClaim", tag }),
        }
    }
    // proven_fee: Option<ProvenFee> (core/circuit_io.rs:559)
    if r.option("proven_fee")? {
        // refund_to: Option<AccountId> (core/circuit_io.rs:162)
        if r.option("refund_to")? {
            r.skip(32)?;
        }
        // tip: Fee = u64 (core/circuit_io.rs:163, core/account.rs:86)
        r.skip(8)?;
        // fallback: Option<FeeFallback> (core/circuit_io.rs:166)
        if r.option("fallback")? {
            // FeeFallback (core/circuit_io.rs:179-187): nullifier: Nullifier([u8; 32]) (:181, core/nullifier.rs:111),
            // root: CommitmentSetDigest (:182), commitment: Commitment (:183)
            r.skip(32 + 32 + 32)?;
            // ciphertext: Ciphertext(Vec<u8>) (core/circuit_io.rs:185, core/encryption/mod.rs:67)
            r.bytes_u32("fallback ciphertext")?;
            // boundary: Boundary (core/circuit_io.rs:186)
            boundary(r, index, v, true)?;
        }
    }
    Ok(())
}

/// `PrivateAction { nullifier: Nullifier, root: CommitmentSetDigest, commitment: Commitment, encrypted_post_state:
/// EncryptedNote }` (`core/circuit_io.rs:515-523`).
fn private_action(r: &mut Reader<'_>) -> Result<(), Error> {
    r.skip(32 + 32 + 32)?; // core/circuit_io.rs:516, 517, 521
    encrypted_note(r) // core/circuit_io.rs:522
}

/// `EncryptedNote { ciphertext: Ciphertext(Vec<u8>), epk: EphemeralPublicKey(Vec<u8>) }`
/// (`core/encryption/mod.rs:87-90`, 67, 61).
fn encrypted_note(r: &mut Reader<'_>) -> Result<(), Error> {
    r.bytes_u32("ciphertext")?; // core/encryption/mod.rs:88
    r.bytes_u32("epk")?; // core/encryption/mod.rs:89
    Ok(())
}

/// `Boundary = Vec<BoundaryStep>` (`core/execution_state.rs:147`); `BoundaryStep` is `PrivateToPublic(Delivery)` 0 |
/// `PublicToPrivate(Delivery)` 1 | `EndPrivateSubtree` 2 | `EndPublicSubtree` 3 (`core/execution_state.rs:140-145`).
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
            2 | 3 => {}
            tag => return Err(Error::BadTag { what: "BoundaryStep", tag }),
        }
    }
    Ok(())
}

/// `Delivery<Actor> { envelope: MessageEnvelope<Actor>, grants: BTreeSet<Actor>, pda_seeds: BTreeSet<PdaSeed> }`
/// (`core/execution_state.rs:132-136`).
fn delivery<'a, V: Visitor<'a>>(r: &mut Reader<'a>, index: u32, v: &mut V, origin: Origin) -> Result<(), Error> {
    let (from, to, message) = envelope(r, "delivery message")?; // core/execution_state.rs:133
    v.call(index, origin, Some(from), to, message);
    strict_set::<64>(r, "grants")?; // core/execution_state.rs:134
    strict_set::<32>(r, "pda_seeds") // core/execution_state.rs:135; PdaSeed([u8; 32]), core/program/mod.rs:116
}

fn u128_le(r: &mut Reader<'_>) -> Result<u128, Error> {
    Ok(u128::from_le_bytes(*r.array()?))
}
