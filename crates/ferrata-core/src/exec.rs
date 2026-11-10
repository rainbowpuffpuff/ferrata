//! Execution-layer objects decoded from RLP: block headers, accounts, storage values, receipts
//! and logs.
//!
//! Every decoder is strict: canonical RLP ([`crate::rlp`]), the exact number of items, the exact
//! size of every fixed-size field, and nothing after the last item. Integers that the consensus
//! layer carries as `uint64` (block number, gas, timestamp) must fit 64 bits.
//!
//! **Header shapes.** [`decode_header`] accepts exactly three layouts, told apart by their field
//! count, as `execution-specs` defines them (`src/ethereum/forks/*/blocks.py`; Amsterdam read on
//! the `forks/amsterdam` branch at `1689800` of 6 Oct 2026, where `blocks.py` last changed in
//! `5bb2a51`, and identical on `master` and `devnets/bal/3`):
//!
//! | Shape | Fields | Added |
//! |---|---|---|
//! | Cancun | 20 | `blob_gas_used`, `excess_blob_gas` (EIP-4844), `parent_beacon_block_root` (EIP-4788) |
//! | Prague, Osaka | 21 | `requests_hash` (EIP-7685); Osaka changed no header field |
//! | Amsterdam | 23 | `block_access_list_hash` (EIP-7928), then `slot_number` (EIP-7843) |
//!
//! Anything else is refused with `HeaderUnsupported`:
//! * pre-Cancun headers: no RLP header this crate needs is older. Capella to Fulu records take
//!   their roots from the SSZ payload header, a Gloas record's header is Osaka or later, and an
//!   EIP-2935 hop reaches back at most to the last Cancun block (the history contract's first
//!   entry, written by the first Prague block);
//! * a 22-field header (`block_access_list_hash` without `slot_number`), which no current spec or
//!   devnet branch defines;
//! * any header with fields after the last known one. The fields this crate reads sit at the
//!   same positions in every shape, and the block hash binds the whole encoding, so accepting
//!   unknown trailing fields would not change what is extracted; refusing them keeps a decoder
//!   built for today's forks from accepting a layout nobody reviewed. A fork that adds a header
//!   field therefore needs a new release before it activates.

use crate::{
    keccak::keccak256,
    mpt::{self, hex, EMPTY_ROOT},
    rlp::{self, Item},
    Error, Root,
};

/// A 20-byte account address.
pub type Address = [u8; 20];
/// A 256-bit value, big-endian: storage slots and values, balances, predicate constants.
pub type Word = [u8; 32];

/// `KEC("")`: the code hash of an account without code.
pub const KECCAK_EMPTY: Root = hex("c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470");

/// `MAX_EXTRA_DATA_BYTES`, enforced by the consensus layer on every post-merge payload.
pub const MAX_EXTRA_DATA_BYTES: usize = 32;

/// The header layouts [`decode_header`] accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeaderShape {
    /// 20 fields.
    Cancun,
    /// 21 fields; also Osaka's.
    Prague,
    /// 23 fields.
    Amsterdam,
}

/// An execution block header. `logs_bloom` (256 bytes) and `extra_data` (at most 32 bytes) are
/// checked but not kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockHeader {
    pub shape: HeaderShape,
    pub parent_hash: Root,
    pub ommers_hash: Root,
    pub beneficiary: Address,
    pub state_root: Root,
    pub transactions_root: Root,
    pub receipts_root: Root,
    pub difficulty: Word,
    pub number: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub timestamp: u64,
    pub mix_hash: Root,
    pub nonce: [u8; 8],
    pub base_fee_per_gas: Word,
    pub withdrawals_root: Root,
    pub blob_gas_used: u64,
    pub excess_blob_gas: u64,
    pub parent_beacon_block_root: Root,
    /// Prague onward.
    pub requests_hash: Option<Root>,
    /// Amsterdam onward.
    pub block_access_list_hash: Option<Root>,
    /// Amsterdam onward.
    pub slot_number: Option<u64>,
}

/// Decode an RLP block header of an accepted shape (see the module documentation).
pub fn decode_header(encoded: &[u8]) -> Result<BlockHeader, Error> {
    let mut f = [Item::EMPTY; 23];
    let mut count = 0;
    for item in rlp::decode(encoded)?.list()? {
        let slot = f.get_mut(count).ok_or(Error::HeaderUnsupported)?;
        *slot = item?;
        count += 1;
    }
    let shape = match count {
        20 => HeaderShape::Cancun,
        21 => HeaderShape::Prague,
        23 => HeaderShape::Amsterdam,
        _ => return Err(Error::HeaderUnsupported),
    };
    f[6].fixed::<256>()?; // logs_bloom
    if f[12].bytes()?.len() > MAX_EXTRA_DATA_BYTES {
        return Err(Error::RlpMalformed);
    }
    Ok(BlockHeader {
        shape,
        parent_hash: f[0].fixed()?,
        ommers_hash: f[1].fixed()?,
        beneficiary: f[2].fixed()?,
        state_root: f[3].fixed()?,
        transactions_root: f[4].fixed()?,
        receipts_root: f[5].fixed()?,
        difficulty: f[7].word()?,
        number: f[8].u64()?,
        gas_limit: f[9].u64()?,
        gas_used: f[10].u64()?,
        timestamp: f[11].u64()?,
        mix_hash: f[13].fixed()?,
        nonce: f[14].fixed()?,
        base_fee_per_gas: f[15].word()?,
        withdrawals_root: f[16].fixed()?,
        blob_gas_used: f[17].u64()?,
        excess_blob_gas: f[18].u64()?,
        parent_beacon_block_root: f[19].fixed()?,
        requests_hash: if count >= 21 { Some(f[20].fixed()?) } else { None },
        block_access_list_hash: if count >= 23 { Some(f[21].fixed()?) } else { None },
        slot_number: if count >= 23 { Some(f[22].u64()?) } else { None },
    })
}

/// [`decode_header`], after checking that `KEC(encoded)` is `block_hash`.
pub fn decode_header_with_hash(encoded: &[u8], block_hash: &Root) -> Result<BlockHeader, Error> {
    if keccak256(encoded) != *block_hash {
        return Err(Error::HeaderHashMismatch);
    }
    decode_header(encoded)
}

/// An account: `[nonce, balance, storage_root, code_hash]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Account {
    /// At most 2^64 - 1 (EIP-2681).
    pub nonce: u64,
    pub balance: Word,
    pub storage_root: Root,
    pub code_hash: Root,
}

impl Account {
    /// An address without an account: every field zero, as `eth_getProof` reports it and as
    /// `EXTCODEHASH` returns its code hash. No existing account has a zero code hash or storage
    /// root (both are Keccak-256 outputs), and [`Account::decode`] refuses one, so this value
    /// cannot be confused with an existing account.
    pub const ABSENT: Account = Account { nonce: 0, balance: [0; 32], storage_root: [0; 32], code_hash: [0; 32] };

    /// False for [`Account::ABSENT`].
    pub fn exists(&self) -> bool {
        self.code_hash != [0; 32]
    }

    /// Decode a state-trie value.
    pub fn decode(encoded: &[u8]) -> Result<Self, Error> {
        let [nonce, balance, storage_root, code_hash] = rlp::decode(encoded)?.items()?;
        let account = Account {
            nonce: nonce.u64()?,
            balance: balance.word()?,
            storage_root: storage_root.fixed()?,
            code_hash: code_hash.fixed()?,
        };
        if account.storage_root == [0; 32] || account.code_hash == [0; 32] {
            return Err(Error::RlpMalformed);
        }
        Ok(account)
    }
}

/// Decode a storage-trie value: a canonical non-zero integer of at most 32 bytes. Zero is never
/// stored (writing zero deletes the slot), so a stored zero is refused.
pub fn decode_storage_value(encoded: &[u8]) -> Result<Word, Error> {
    let value = rlp::decode(encoded)?.word()?;
    if value == [0; 32] {
        return Err(Error::RlpMalformed);
    }
    Ok(value)
}

/// The account at `address` under `state_root` (secure trie: the key is `KEC(address)`).
pub(crate) fn prove_account<P: AsRef<[u8]>>(
    state_root: &Root,
    address: &Address,
    proof: &[P],
) -> Result<Account, Error> {
    match mpt::verify(state_root, &keccak256(address), proof)? {
        Some(encoded) => Account::decode(encoded),
        None => Ok(Account::ABSENT),
    }
}

/// The value of `slot` in `account`'s storage (secure trie: the key is `KEC(slot)`); zero when
/// the slot is absent. An absent account has empty storage, proven by the empty proof.
pub(crate) fn prove_storage<P: AsRef<[u8]>>(account: &Account, slot: &Word, proof: &[P]) -> Result<Word, Error> {
    let root = if account.exists() { account.storage_root } else { EMPTY_ROOT };
    match mpt::verify(&root, &keccak256(slot), proof)? {
        Some(encoded) => decode_storage_value(encoded),
        None => Ok([0; 32]),
    }
}

/// A log: `[address, topics, data]`, with at most four topics (`LOG0` to `LOG4`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Log<'a> {
    pub address: Address,
    topics: [Root; 4],
    topic_count: usize,
    pub data: &'a [u8],
}

impl<'a> Log<'a> {
    /// The topics, in order.
    pub fn topics(&self) -> &[Root] {
        &self.topics[..self.topic_count]
    }

    fn decode(item: Item<'a>) -> Result<Self, Error> {
        let [address, topic_list, data] = item.items()?;
        let mut topics = [[0u8; 32]; 4];
        let mut topic_count = 0;
        for topic in topic_list.list()? {
            *topics.get_mut(topic_count).ok_or(Error::RlpMalformed)? = topic?.fixed()?;
            topic_count += 1;
        }
        Ok(Log { address: address.fixed()?, topics, topic_count, data: data.bytes()? })
    }
}

/// A receipt (post-Byzantium): `[status, cumulative_gas_used, logs_bloom, logs]`, legacy or
/// typed (EIP-2718: the type byte, then that list). Every log is checked when the receipt is
/// decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Receipt<'a> {
    /// 0 for a legacy receipt, else the EIP-2718 type: 1 (EIP-2930), 2 (EIP-1559), 3 (EIP-4844)
    /// or 4 (EIP-7702).
    pub tx_type: u8,
    /// EIP-658: true if the transaction succeeded.
    pub status: bool,
    pub cumulative_gas_used: u64,
    pub logs_bloom: &'a [u8],
    logs: Item<'a>,
    log_count: usize,
}

impl<'a> Receipt<'a> {
    /// Decode a receipts-trie value. Refuses pre-Byzantium receipts (a 32-byte state root where
    /// the status belongs) and unknown types with `ReceiptUnsupported`.
    pub fn decode(encoded: &'a [u8]) -> Result<Self, Error> {
        let (&first, rest) = encoded.split_first().ok_or(Error::RlpMalformed)?;
        let (tx_type, body) = match first {
            0x01..=0x04 => (first, rest),
            0xc0..=0xff => (0, encoded),
            _ => return Err(Error::ReceiptUnsupported),
        };
        let [status, cumulative_gas_used, logs_bloom, logs] = rlp::decode(body)?.items()?;
        if status.bytes()?.len() == 32 {
            return Err(Error::ReceiptUnsupported);
        }
        let status = match status.u64()? {
            0 => false,
            1 => true,
            _ => return Err(Error::RlpMalformed),
        };
        let mut log_count = 0;
        for log in logs.list()? {
            Log::decode(log?)?;
            log_count += 1;
        }
        Ok(Receipt {
            tx_type,
            status,
            cumulative_gas_used: cumulative_gas_used.u64()?,
            logs_bloom: logs_bloom.fixed::<256>().map(|_| logs_bloom.payload)?,
            logs,
            log_count,
        })
    }

    /// The number of logs.
    pub fn log_count(&self) -> usize {
        self.log_count
    }

    /// The log at `index` (its position in this receipt).
    pub fn log(&self, index: u64) -> Result<Log<'a>, Error> {
        let i = usize::try_from(index).map_err(|_| Error::LogIndexOutOfRange)?;
        if i >= self.log_count {
            return Err(Error::LogIndexOutOfRange);
        }
        // `decode` counted the logs, so the list has an item at `i`.
        let item = self.logs.list()?.nth(i).ok_or(Error::RlpMalformed)??;
        Log::decode(item)
    }
}

/// Where a log sits, from JSON-RPC's block-wide `logIndex`. JSON-RPC numbers the logs of a block
/// consecutively across its receipts; [`Receipt::log`] and `attest_log` take the log's position
/// in its own receipt, counted from zero. Given the number of logs in every receipt of the block,
/// in transaction order (from `eth_getBlockReceipts`), this returns the transaction index and the
/// position, or `None` past the last log.
///
/// A query helper for clients, not a check: an attestation proves the log at the position it is
/// given, so a wrong conversion yields another log or `LogIndexOutOfRange`, never a false
/// statement.
pub fn locate_log(log_counts: &[u64], block_log_index: u64) -> Option<(u64, u64)> {
    let mut first: u64 = 0;
    for (tx_index, count) in log_counts.iter().enumerate() {
        let end = first.checked_add(*count)?;
        if block_log_index < end {
            return Some((tx_index as u64, block_log_index - first));
        }
        first = end;
    }
    None
}

/// The inverse of [`locate_log`]: the block-wide `logIndex` of the log at `position` in the
/// receipt of transaction `tx_index`, or `None` if there is no such log.
pub fn block_log_index(log_counts: &[u64], tx_index: u64, position: u64) -> Option<u64> {
    let tx = usize::try_from(tx_index).ok()?;
    if position >= *log_counts.get(tx)? {
        return None;
    }
    log_counts[..tx].iter().try_fold(position, |acc, count| acc.checked_add(*count))
}

/// The receipt of transaction `tx_index` under `receipts_root` (the key is `RLP(tx_index)`).
pub(crate) fn prove_receipt<'p, P: AsRef<[u8]>>(
    receipts_root: &Root,
    tx_index: u64,
    proof: &'p [P],
) -> Result<Receipt<'p>, Error> {
    let (key, len) = rlp::encode_u64(tx_index);
    let encoded = mpt::verify(receipts_root, &key[..len], proof)?.ok_or(Error::ReceiptNotIncluded)?;
    Receipt::decode(encoded)
}
