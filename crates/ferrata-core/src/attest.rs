//! Attestations: facts proven under a [`HeaderRecord`], their statement identifiers, and
//! conditions on them. This is the layer consumers call.
//!
//! **Where roots come from.** Every function here takes its execution roots from a
//! [`HeaderRecord`], never from the caller. A record is built in one of four ways only:
//! from a [`FinalizedStore`]'s finalized header ([`HeaderRecord::from_payload_header`] for the
//! Capella to Fulu layouts, [`HeaderRecord::from_block_hash`] for Gloas), from another record
//! through an EIP-2935 hop ([`history_hop`]), from the record the program wrote once for a
//! finalized header (the LEZ program's `header(id, slot)`: [`HeaderRecord::from_recorded`], or
//! [`HeaderRecord::from_recorded_block_hash`] with the block's RLP header when the record holds
//! a block hash only), or restored from this type's own bytes ([`HeaderRecord::from_ssz`]).
//!
//! **Chain binding.** Each call takes the chain id the consumer is configured for and refuses a
//! record of another chain (`ChainMismatch`). The record's chain id is the instance's, fixed when
//! the instance was initialised.
//!
//! **Statement identifiers** are SHA-256 over a fixed tag, a one-byte kind and fixed-width
//! big-endian fields ([`account_id`], [`storage_id`], [`log_id`]). The derivation is part of the
//! interface, so a consumer can recompute an identifier from the attested fields.
//! * A state fact (account, storage slot) is identified by what it says, not by where it was
//!   read: chain id, address, and the account's four fields or the slot and its value. The block
//!   number, the record and the light-client instance are left out, so the same value read at two
//!   blocks, or through two instances, has one identifier. An identifier of a state fact
//!   therefore names a value, not an event: a balance that changes and changes back has the same
//!   identifier both times. Consumers that need "once per event" key on a log.
//! * A log is identified by chain id, block number, the transaction's index in the block, the
//!   log's position in its receipt, and [`log_hash`]. A finalized block number names one block
//!   per chain, so the identifier does not depend on which record (direct or reached through
//!   history hops) proved it.
//! * The record's anchor slot, binding and hop count never enter an identifier.
//!
//! **Two identifiers for a state fact.** The default ([`account_id`], [`storage_id`]) follows RFP
//! Functionality #6: identical state, identical identifier. It is safe for statements meant to
//! hold whenever they are read ("this balance is at least X"), and unsafe as a once-per-event
//! nullifier, since a value that changes and changes back looks the same. The block-bound
//! variant ([`account_id_at_block`], [`storage_id_at_block`], [`Attested::id_at_block`]) adds the
//! block number. It is safe when the consumer pins the block (a snapshot) and wants one use per
//! pinned block. Neither identifies an event: the same unchanged value read at two blocks has two
//! block-bound identifiers. Events are attested as logs or as accumulator leaves
//! ([`crate::accumulator`]), whose identifiers are one per event.

use alloc::vec::Vec;

use sha2::{Digest, Sha256};

use crate::{
    exec::{
        decode_header, decode_header_with_hash, prove_account, prove_receipt, prove_storage, Account, Address, Log,
        Word,
    },
    keccak::keccak256,
    merkle::ZERO,
    mpt::hex,
    policy::FinalizedStore,
    types::Execution,
    witness::{self, ExecutionSummary},
    Error, Root,
};

/// EIP-2935 `HISTORY_STORAGE_ADDRESS`.
pub const HISTORY_STORAGE_ADDRESS: Address = hex("0000f90827f1c53a10cb7a02335b175320002935");
/// EIP-2935 `HISTORY_SERVE_WINDOW`.
pub const HISTORY_SERVE_WINDOW: u64 = 8191;
/// Keccak-256 of the history contract's runtime code as EIP-2935 specifies it (deployed by the
/// same pre-signed transaction on every chain).
pub const HISTORY_CODE_HASH: Root = hex("6e49e66782037c0555897870e29fa5e552daf4719552131a0abce779daec0a5d");

/// Which binding a record's execution data came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Binding {
    /// A Capella to Fulu light-client header: the execution payload header, proven to the beacon
    /// block body by the header's branch.
    PayloadHeader,
    /// A Gloas light-client header: the bound execution block hash (the bid's parent block), and
    /// the RLP header that hashes to it.
    BlockHash,
    /// `hops` EIP-2935 hops back from a record of one of the kinds above.
    History { hops: u32 },
}

/// The execution data of one finalized block, as the program records it. Fields are private:
/// see the module documentation for the only ways to obtain one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeaderRecord {
    chain_id: u64,
    anchor_slot: u64,
    number: u64,
    timestamp: u64,
    block_hash: Root,
    parent_hash: Root,
    state_root: Root,
    receipts_root: Root,
    binding: Binding,
}

impl HeaderRecord {
    /// Length of [`HeaderRecord::to_ssz`].
    pub const SSZ_LEN: usize = 4 * 8 + 4 * 32 + 1 + 4;

    /// The record of `store`'s finalized header when it carries an execution payload header
    /// (Capella to Fulu). `chain_id` is the instance's configured chain id.
    pub fn from_payload_header(store: &FinalizedStore, chain_id: u64) -> Result<Self, Error> {
        let header = &store.core().finalized_header;
        let Execution::Payload { header: payload, .. } = &header.execution else {
            return Err(Error::BindingMismatch);
        };
        if payload.block_hash == ZERO {
            return Err(Error::BindingMismatch);
        }
        Ok(Self {
            chain_id,
            anchor_slot: header.beacon.slot,
            number: payload.block_number,
            timestamp: payload.timestamp,
            block_hash: payload.block_hash,
            parent_hash: payload.parent_hash,
            state_root: payload.state_root,
            receipts_root: payload.receipts_root,
            binding: Binding::PayloadHeader,
        })
    }

    /// The record of `store`'s finalized header when it binds only an execution block hash
    /// (Gloas): `header_rlp` must hash to that block hash, and the roots come from it.
    pub fn from_block_hash(store: &FinalizedStore, chain_id: u64, header_rlp: &[u8]) -> Result<Self, Error> {
        let header = &store.core().finalized_header;
        let Execution::BlockHash { block_hash, .. } = &header.execution else {
            return Err(Error::BindingMismatch);
        };
        if *block_hash == ZERO {
            return Err(Error::BindingMismatch);
        }
        let exec = decode_header_with_hash(header_rlp, block_hash)?;
        Ok(Self {
            chain_id,
            anchor_slot: header.beacon.slot,
            number: exec.number,
            timestamp: exec.timestamp,
            block_hash: *block_hash,
            parent_hash: exec.parent_hash,
            state_root: exec.state_root,
            receipts_root: exec.receipts_root,
            binding: Binding::BlockHash,
        })
    }

    /// The execution view of a record the program wrote for a finalized header
    /// ([`witness::HeaderRecord`], the state of the LEZ program's `header(id, slot)`), under the
    /// chain id of the instance that wrote it. Trusted input: only for a record read from the
    /// program's own write-once record account, in the program itself or by a consumer in the
    /// program's reply. It equals [`HeaderRecord::from_payload_header`] of the store that
    /// finalized the header. A record without a payload summary is refused with
    /// `BindingMismatch`: the Altair layout has no execution data, and a record that holds a
    /// block hash only (Gloas) needs that block's header, see
    /// [`HeaderRecord::from_recorded_block_hash`].
    pub fn from_recorded(record: &witness::HeaderRecord, chain_id: u64) -> Result<Self, Error> {
        let ExecutionSummary::Payload { block_number, block_hash, parent_hash, state_root, receipts_root, timestamp } =
            &record.execution
        else {
            return Err(Error::BindingMismatch);
        };
        if *block_hash == ZERO {
            return Err(Error::BindingMismatch);
        }
        Ok(Self {
            chain_id,
            anchor_slot: record.beacon.slot,
            number: *block_number,
            timestamp: *timestamp,
            block_hash: *block_hash,
            parent_hash: *parent_hash,
            state_root: *state_root,
            receipts_root: *receipts_root,
            binding: Binding::PayloadHeader,
        })
    }

    /// The execution view of a record the program wrote for a finalized header in the Gloas
    /// layout, which holds only the execution block hash the header binds (the bid's parent
    /// block). `header_rlp` is that block's RLP header, supplied by whoever asks: it must hash
    /// to the recorded block hash (`HeaderHashMismatch`), so every field read from it (the
    /// number, the timestamp, the parent hash, the state root and the receipts root) is the
    /// bound block's. The record is trusted input as for [`HeaderRecord::from_recorded`]. It
    /// equals [`HeaderRecord::from_block_hash`] of the store that finalized the header. A record
    /// of another layout, or one that binds no block (a zero hash: a header from before the
    /// merge carried into a Gloas store), is refused with `BindingMismatch`.
    pub fn from_recorded_block_hash(
        record: &witness::HeaderRecord,
        chain_id: u64,
        header_rlp: &[u8],
    ) -> Result<Self, Error> {
        let ExecutionSummary::BlockHash(block_hash) = &record.execution else {
            return Err(Error::BindingMismatch);
        };
        if *block_hash == ZERO {
            return Err(Error::BindingMismatch);
        }
        let exec = decode_header_with_hash(header_rlp, block_hash)?;
        Ok(Self {
            chain_id,
            anchor_slot: record.beacon.slot,
            number: exec.number,
            timestamp: exec.timestamp,
            block_hash: *block_hash,
            parent_hash: exec.parent_hash,
            state_root: exec.state_root,
            receipts_root: exec.receipts_root,
            binding: Binding::BlockHash,
        })
    }

    /// The chain id of the instance that made the record.
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }
    /// The beacon slot of the finalized light-client header the record rests on, its anchor.
    /// For a record built from the light client, that header's slot. For a record reached
    /// through EIP-2935 hops ([`Binding::History`]), the slot of the light-client record the hops
    /// started from: not the slot of the record's own (older) block, which the record does not
    /// know. Use [`HeaderRecord::number`] and [`HeaderRecord::timestamp`] to place the block.
    pub fn anchor_slot(&self) -> u64 {
        self.anchor_slot
    }
    /// The execution block number.
    pub fn number(&self) -> u64 {
        self.number
    }
    /// The execution block's timestamp.
    pub fn timestamp(&self) -> u64 {
        self.timestamp
    }
    /// The execution block hash.
    pub fn block_hash(&self) -> Root {
        self.block_hash
    }
    /// The parent block's hash.
    pub fn parent_hash(&self) -> Root {
        self.parent_hash
    }
    /// The state root.
    pub fn state_root(&self) -> Root {
        self.state_root
    }
    /// The receipts root.
    pub fn receipts_root(&self) -> Root {
        self.receipts_root
    }
    /// Where the execution data came from.
    pub fn binding(&self) -> Binding {
        self.binding
    }

    /// The SSZ encoding of the fixed-size container `(chain_id, anchor_slot, number, timestamp: uint64;
    /// block_hash, parent_hash, state_root, receipts_root: Bytes32; binding: uint8 (0 payload
    /// header, 1 block hash, 2 history); hops: uint32)`, as the program stores it.
    pub fn to_ssz(&self) -> [u8; Self::SSZ_LEN] {
        let mut out = [0u8; Self::SSZ_LEN];
        let (tag, hops) = match self.binding {
            Binding::PayloadHeader => (0u8, 0u32),
            Binding::BlockHash => (1, 0),
            Binding::History { hops } => (2, hops),
        };
        let mut at = 0;
        let mut put = |bytes: &[u8]| {
            out[at..at + bytes.len()].copy_from_slice(bytes);
            at += bytes.len();
        };
        put(&self.chain_id.to_le_bytes());
        put(&self.anchor_slot.to_le_bytes());
        put(&self.number.to_le_bytes());
        put(&self.timestamp.to_le_bytes());
        put(&self.block_hash);
        put(&self.parent_hash);
        put(&self.state_root);
        put(&self.receipts_root);
        put(&[tag]);
        put(&hops.to_le_bytes());
        out
    }

    /// Restore a record from [`HeaderRecord::to_ssz`] bytes. Trusted input: only for bytes a
    /// verifier wrote itself from a record it had built in one of the other ways (the LEZ program
    /// stores a [`witness::HeaderRecord`] instead: see [`HeaderRecord::from_recorded`]). Refuses
    /// a wrong length, an unknown binding, and a hop count that does not fit the binding
    /// (`Malformed`).
    pub fn from_ssz(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() != Self::SSZ_LEN {
            return Err(Error::Malformed);
        }
        let u64_at = |o: usize| u64::from_le_bytes(bytes[o..o + 8].try_into().unwrap_or_default());
        let root_at = |o: usize| -> Root { bytes[o..o + 32].try_into().unwrap_or_default() };
        let hops = u32::from_le_bytes(bytes[161..165].try_into().unwrap_or_default());
        let binding = match (bytes[160], hops) {
            (0, 0) => Binding::PayloadHeader,
            (1, 0) => Binding::BlockHash,
            (2, h) if h > 0 => Binding::History { hops: h },
            _ => return Err(Error::Malformed),
        };
        Ok(Self {
            chain_id: u64_at(0),
            anchor_slot: u64_at(8),
            number: u64_at(16),
            timestamp: u64_at(24),
            block_hash: root_at(32),
            parent_hash: root_at(64),
            state_root: root_at(96),
            receipts_root: root_at(128),
            binding,
        })
    }

    fn for_chain(&self, chain_id: u64) -> Result<&Self, Error> {
        if self.chain_id != chain_id {
            return Err(Error::ChainMismatch);
        }
        Ok(self)
    }
}

/// The tag every statement identifier starts with.
pub const STATEMENT_TAG: &[u8; 20] = b"ferrata/statement/v1";
/// The tag [`log_hash`] starts with.
pub const LOG_TAG: &[u8; 14] = b"ferrata/log/v1";
const KIND_ACCOUNT: u8 = 1;
const KIND_STORAGE: u8 = 2;
const KIND_LOG: u8 = 3;
const KIND_ACCOUNT_AT_BLOCK: u8 = 0x11;
const KIND_STORAGE_AT_BLOCK: u8 = 0x12;

fn statement(kind: u8, chain_id: u64) -> Sha256 {
    let mut h = Sha256::new();
    h.update(STATEMENT_TAG);
    h.update([kind]);
    h.update(chain_id.to_be_bytes());
    h
}

/// `SHA-256(STATEMENT_TAG ‖ 0x01 ‖ chain_id ‖ address ‖ nonce ‖ balance ‖ storage_root ‖
/// code_hash)`, integers as big-endian `uint64` (nonce, chain id) or 32 bytes (balance). An
/// absent account is identified by its all-zero fields ([`Account::ABSENT`]).
pub fn account_id(chain_id: u64, address: &Address, account: &Account) -> Root {
    let mut h = statement(KIND_ACCOUNT, chain_id);
    h.update(address);
    h.update(account.nonce.to_be_bytes());
    h.update(account.balance);
    h.update(account.storage_root);
    h.update(account.code_hash);
    h.finalize().into()
}

/// `SHA-256(STATEMENT_TAG ‖ 0x02 ‖ chain_id ‖ address ‖ slot ‖ value)`; an absent slot has value
/// zero.
pub fn storage_id(chain_id: u64, address: &Address, slot: &Word, value: &Word) -> Root {
    let mut h = statement(KIND_STORAGE, chain_id);
    h.update(address);
    h.update(slot);
    h.update(value);
    h.finalize().into()
}

/// The block-bound variant of [`account_id`]: `SHA-256(STATEMENT_TAG ‖ 0x11 ‖ chain_id ‖
/// block_number ‖ address ‖ nonce ‖ balance ‖ storage_root ‖ code_hash)`.
pub fn account_id_at_block(chain_id: u64, block_number: u64, address: &Address, account: &Account) -> Root {
    let mut h = statement(KIND_ACCOUNT_AT_BLOCK, chain_id);
    h.update(block_number.to_be_bytes());
    h.update(address);
    h.update(account.nonce.to_be_bytes());
    h.update(account.balance);
    h.update(account.storage_root);
    h.update(account.code_hash);
    h.finalize().into()
}

/// The block-bound variant of [`storage_id`]: `SHA-256(STATEMENT_TAG ‖ 0x12 ‖ chain_id ‖
/// block_number ‖ address ‖ slot ‖ value)`.
pub fn storage_id_at_block(chain_id: u64, block_number: u64, address: &Address, slot: &Word, value: &Word) -> Root {
    let mut h = statement(KIND_STORAGE_AT_BLOCK, chain_id);
    h.update(block_number.to_be_bytes());
    h.update(address);
    h.update(slot);
    h.update(value);
    h.finalize().into()
}

/// `SHA-256(LOG_TAG ‖ address ‖ topic count (1 byte) ‖ topics ‖ data length (uint64) ‖ data)`.
/// Defined over the log's fields rather than its RLP, so it survives a change of receipt
/// encoding.
pub fn log_hash(log: &Log<'_>) -> Root {
    let mut h = Sha256::new();
    h.update(LOG_TAG);
    h.update(log.address);
    h.update([log.topics().len() as u8]);
    for topic in log.topics() {
        h.update(topic);
    }
    h.update((log.data.len() as u64).to_be_bytes());
    h.update(log.data);
    h.finalize().into()
}

/// `SHA-256(STATEMENT_TAG ‖ 0x03 ‖ chain_id ‖ block_number ‖ tx_index ‖ log_index ‖
/// log_hash(log))`, integers as big-endian `uint64`; `log_index` is the log's position in its
/// receipt.
pub fn log_id(chain_id: u64, block_number: u64, tx_index: u64, log_index: u64, log: &Log<'_>) -> Root {
    let mut h = statement(KIND_LOG, chain_id);
    h.update(block_number.to_be_bytes());
    h.update(tx_index.to_be_bytes());
    h.update(log_index.to_be_bytes());
    h.update(log_hash(log));
    h.finalize().into()
}

/// An account fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccountFact {
    pub address: Address,
    /// [`Account::ABSENT`] if the address has no account.
    pub account: Account,
}

/// A storage fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageFact {
    pub address: Address,
    pub slot: Word,
    /// Zero if the slot is absent.
    pub value: Word,
}

/// A log fact. The transaction succeeded (receipt status 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogFact<'a> {
    /// The transaction's index in the block.
    pub tx_index: u64,
    /// The log's position in the transaction's receipt (not the block-wide `logIndex` that
    /// JSON-RPC reports).
    pub log_index: u64,
    pub log: Log<'a>,
}

/// Any fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fact<'a> {
    Account(AccountFact),
    Storage(StorageFact),
    Log(LogFact<'a>),
}

/// A fact proven under a record, with its statement identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attested<T> {
    pub chain_id: u64,
    /// The block the fact was proven at (not part of a state fact's identifier).
    pub block_number: u64,
    pub block_hash: Root,
    pub fact: T,
    pub id: Root,
}

impl<T> Attested<T> {
    fn at(record: &HeaderRecord, fact: T, id: Root) -> Self {
        Self { chain_id: record.chain_id, block_number: record.number, block_hash: record.block_hash, fact, id }
    }
}

impl Attested<AccountFact> {
    /// The block-bound identifier ([`account_id_at_block`]); `id` is the default one.
    pub fn id_at_block(&self) -> Root {
        account_id_at_block(self.chain_id, self.block_number, &self.fact.address, &self.fact.account)
    }
}

impl Attested<StorageFact> {
    /// The block-bound identifier ([`storage_id_at_block`]); `id` is the default one.
    pub fn id_at_block(&self) -> Root {
        storage_id_at_block(self.chain_id, self.block_number, &self.fact.address, &self.fact.slot, &self.fact.value)
    }
}

impl Attested<Fact<'_>> {
    /// The block-bound identifier of a state fact; a log's identifier already holds its block.
    pub fn id_at_block(&self) -> Root {
        match &self.fact {
            Fact::Account(a) => account_id_at_block(self.chain_id, self.block_number, &a.address, &a.account),
            Fact::Storage(s) => storage_id_at_block(self.chain_id, self.block_number, &s.address, &s.slot, &s.value),
            Fact::Log(_) => self.id,
        }
    }
}

/// A comparison of a 256-bit value with a constant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// `value <cmp> rhs`, unsigned 256-bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Predicate {
    pub cmp: Cmp,
    pub rhs: Word,
}

impl Predicate {
    /// True if `value <cmp> rhs` (big-endian words compare as unsigned integers).
    pub fn holds(&self, value: &Word) -> bool {
        let ord = value.cmp(&self.rhs);
        match self.cmp {
            Cmp::Eq => ord.is_eq(),
            Cmp::Ne => ord.is_ne(),
            Cmp::Lt => ord.is_lt(),
            Cmp::Le => ord.is_le(),
            Cmp::Gt => ord.is_gt(),
            Cmp::Ge => ord.is_ge(),
        }
    }
}

/// A field of a fact, as a 256-bit word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    /// Account nonce.
    Nonce,
    /// Account balance.
    Balance,
    /// Account storage root.
    StorageRoot,
    /// Account code hash.
    CodeHash,
    /// Storage value.
    Value,
    /// The log's emitting address, left-padded to 32 bytes.
    Emitter,
    /// A log topic, by position.
    Topic(u8),
    /// The `i`-th 32-byte word of a log's data (bytes `32 i .. 32 i + 32`).
    DataWord(u32),
}

/// `field <cmp> rhs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Condition {
    pub field: Field,
    pub predicate: Predicate,
}

impl Fact<'_> {
    /// The value of `field`. A field of another kind of fact is `QueryInvalid`; a topic or data
    /// word the log does not have is `PredicateFailed`.
    pub fn field(&self, field: Field) -> Result<Word, Error> {
        let mut w = [0u8; 32];
        match (self, field) {
            (Fact::Account(a), Field::Nonce) => w[24..].copy_from_slice(&a.account.nonce.to_be_bytes()),
            (Fact::Account(a), Field::Balance) => w = a.account.balance,
            (Fact::Account(a), Field::StorageRoot) => w = a.account.storage_root,
            (Fact::Account(a), Field::CodeHash) => w = a.account.code_hash,
            (Fact::Storage(s), Field::Value) => w = s.value,
            (Fact::Log(l), Field::Emitter) => w[12..].copy_from_slice(&l.log.address),
            (Fact::Log(l), Field::Topic(i)) => {
                w = *l.log.topics().get(usize::from(i)).ok_or(Error::PredicateFailed)?;
            }
            (Fact::Log(l), Field::DataWord(i)) => {
                let start = usize::try_from(i).ok().and_then(|i| i.checked_mul(32)).ok_or(Error::PredicateFailed)?;
                let end = start.checked_add(32).ok_or(Error::PredicateFailed)?;
                let bytes = l.log.data.get(start..end).ok_or(Error::PredicateFailed)?;
                w.copy_from_slice(bytes);
            }
            _ => return Err(Error::QueryInvalid),
        }
        Ok(w)
    }

    /// Every condition holds (`PredicateFailed` otherwise).
    pub fn check(&self, conditions: &[Condition]) -> Result<(), Error> {
        for c in conditions {
            if !c.predicate.holds(&self.field(c.field)?) {
                return Err(Error::PredicateFailed);
            }
        }
        Ok(())
    }
}

fn account_under<P: AsRef<[u8]>>(
    record: &HeaderRecord,
    address: &Address,
    proof: &[P],
) -> Result<Attested<AccountFact>, Error> {
    let account = prove_account(&record.state_root, address, proof)?;
    let id = account_id(record.chain_id, address, &account);
    Ok(Attested::at(record, AccountFact { address: *address, account }, id))
}

fn storage_under<P: AsRef<[u8]>>(
    record: &HeaderRecord,
    account: &AccountFact,
    slot: &Word,
    proof: &[P],
) -> Result<Attested<StorageFact>, Error> {
    let value = prove_storage(&account.account, slot, proof)?;
    let id = storage_id(record.chain_id, &account.address, slot, &value);
    Ok(Attested::at(record, StorageFact { address: account.address, slot: *slot, value }, id))
}

/// The account at `address` (or [`Account::ABSENT`]) in the record's state.
pub fn attest_account<P: AsRef<[u8]>>(
    record: &HeaderRecord,
    chain_id: u64,
    address: &Address,
    proof: &[P],
) -> Result<Attested<AccountFact>, Error> {
    account_under(record.for_chain(chain_id)?, address, proof)
}

/// The value of `slot` in `address`'s storage in the record's state (zero if absent).
pub fn attest_storage<P: AsRef<[u8]>>(
    record: &HeaderRecord,
    chain_id: u64,
    address: &Address,
    account_proof: &[P],
    slot: &Word,
    storage_proof: &[P],
) -> Result<Attested<StorageFact>, Error> {
    let record = record.for_chain(chain_id)?;
    let account = account_under(record, address, account_proof)?;
    storage_under(record, &account.fact, slot, storage_proof)
}

/// The log at position `log_index` of the receipt of transaction `tx_index` in the record's
/// block. The transaction must have succeeded (`ReceiptFailed` otherwise). `log_index` counts
/// from zero within the receipt, not across the block as JSON-RPC's `logIndex` does;
/// [`crate::exec::locate_log`] converts.
pub fn attest_log<'p, P: AsRef<[u8]>>(
    record: &HeaderRecord,
    chain_id: u64,
    tx_index: u64,
    log_index: u64,
    proof: &'p [P],
) -> Result<Attested<LogFact<'p>>, Error> {
    let record = record.for_chain(chain_id)?;
    let receipt = prove_receipt(&record.receipts_root, tx_index, proof)?;
    if !receipt.status {
        return Err(Error::ReceiptFailed);
    }
    let log = receipt.log(log_index)?;
    let id = log_id(record.chain_id, record.number, tx_index, log_index, &log);
    Ok(Attested::at(record, LogFact { tx_index, log_index, log }, id))
}

/// A storage slot to prove under the account of its [`AccountQuery`].
#[derive(Debug)]
pub struct SlotQuery<'q, P> {
    pub slot: Word,
    pub proof: &'q [P],
    pub conditions: &'q [Condition],
}

/// An account to prove (the `accountProof` of `eth_getProof`), and slots of its storage.
#[derive(Debug)]
pub struct AccountQuery<'q, P> {
    pub address: Address,
    pub proof: &'q [P],
    pub conditions: &'q [Condition],
    pub slots: &'q [SlotQuery<'q, P>],
}

/// A log to prove.
#[derive(Debug)]
pub struct LogQuery<'q, P> {
    pub tx_index: u64,
    pub log_index: u64,
    pub proof: &'q [P],
    pub conditions: &'q [Condition],
}

/// A field of one of a batch's facts, by the fact's position in [`attest_batch`]'s output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Operand {
    pub fact: usize,
    pub field: Field,
}

/// `left <cmp> right`: a comparison between two attested values of one batch, so under one
/// record (soft requirement #4), for example "this account's balance is at least that slot".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Relation {
    pub left: Operand,
    pub cmp: Cmp,
    pub right: Operand,
}

/// Several facts under one record, each with conditions that must all hold, and relations
/// between them that must all hold.
#[derive(Debug)]
pub struct Batch<'q, P> {
    pub accounts: &'q [AccountQuery<'q, P>],
    pub logs: &'q [LogQuery<'q, P>],
    pub relations: &'q [Relation],
}

// The queries hold only references, so they are `Copy` whatever the proof element type is
// (a derive would require `P: Copy`).
macro_rules! copy_without_bound {
    ($($t:ident),*) => {$(
        impl<P> Clone for $t<'_, P> {
            fn clone(&self) -> Self {
                *self
            }
        }
        impl<P> Copy for $t<'_, P> {}
    )*};
}
copy_without_bound!(SlotQuery, AccountQuery, LogQuery, Batch);

/// Prove every fact of `batch` under one record and check every condition and relation: a
/// compound statement over several values of one block (for example a balance and a storage
/// slot). Each account is proven once for all its slots. Returns the facts in order (each
/// account, then its slots, then the logs); relations name facts by that order. Fails as a whole
/// on the first refusal; an empty batch, or a relation naming a fact or field that is not there,
/// is `QueryInvalid`.
pub fn attest_batch<'q, P: AsRef<[u8]>>(
    record: &HeaderRecord,
    chain_id: u64,
    batch: &Batch<'q, P>,
) -> Result<Vec<Attested<Fact<'q>>>, Error> {
    let record = record.for_chain(chain_id)?;
    if batch.accounts.is_empty() && batch.logs.is_empty() {
        return Err(Error::QueryInvalid);
    }
    let mut out = Vec::new();
    for q in batch.accounts {
        let account = account_under(record, &q.address, q.proof)?;
        let fact = Fact::Account(account.fact);
        fact.check(q.conditions)?;
        out.push(Attested::at(record, fact, account.id));
        for s in q.slots {
            let storage = storage_under(record, &account.fact, &s.slot, s.proof)?;
            let fact = Fact::Storage(storage.fact);
            fact.check(s.conditions)?;
            out.push(Attested::at(record, fact, storage.id));
        }
    }
    for q in batch.logs {
        let log = attest_log(record, chain_id, q.tx_index, q.log_index, q.proof)?;
        let fact = Fact::Log(log.fact);
        fact.check(q.conditions)?;
        out.push(Attested::at(record, fact, log.id));
    }
    let value = |o: &Operand| out.get(o.fact).ok_or(Error::QueryInvalid)?.fact.field(o.field);
    for r in batch.relations {
        if !(Predicate { cmp: r.cmp, rhs: value(&r.right)? }).holds(&value(&r.left)?) {
            return Err(Error::PredicateFailed);
        }
    }
    Ok(out)
}

/// One EIP-2935 hop: a record for an older block `n`, from a record of block `R`.
///
/// `header_rlp` is block `n`'s header; `n` is read from it, never supplied. The hop requires
/// `R - 8191 <= n < R` (`HistoryOutOfWindow`), the history contract with its specified code in
/// the record's state (`HistoryContractMissing`), and storage slot `n mod 8191` holding
/// `KEC(header_rlp)` (`HistoryHashMismatch`). The new record keeps the chain id and the anchor
/// slot, and counts one more hop; hops chain.
pub fn history_hop<P: AsRef<[u8]>>(
    record: &HeaderRecord,
    account_proof: &[P],
    storage_proof: &[P],
    header_rlp: &[u8],
) -> Result<HeaderRecord, Error> {
    let header = decode_header(header_rlp)?;
    let n = header.number;
    if !(n < record.number && record.number - n <= HISTORY_SERVE_WINDOW) {
        return Err(Error::HistoryOutOfWindow);
    }
    let contract = prove_account(&record.state_root, &HISTORY_STORAGE_ADDRESS, account_proof)?;
    if contract.code_hash != HISTORY_CODE_HASH {
        return Err(Error::HistoryContractMissing);
    }
    let mut slot = [0u8; 32];
    slot[24..].copy_from_slice(&(n % HISTORY_SERVE_WINDOW).to_be_bytes());
    let block_hash = keccak256(header_rlp);
    if prove_storage(&contract, &slot, storage_proof)? != block_hash {
        return Err(Error::HistoryHashMismatch);
    }
    let hops = match record.binding {
        Binding::History { hops } => hops.saturating_add(1),
        Binding::PayloadHeader | Binding::BlockHash => 1,
    };
    Ok(HeaderRecord {
        chain_id: record.chain_id,
        anchor_slot: record.anchor_slot,
        number: n,
        timestamp: header.timestamp,
        block_hash,
        parent_hash: header.parent_hash,
        state_root: header.state_root,
        receipts_root: header.receipts_root,
        binding: Binding::History { hops },
    })
}
