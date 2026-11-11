//! The Ferrata bridge program on LEZ v0.4 (`guests/bridge`): its messages, actor state and
//! addresses, and the checks it runs on Ethereum facts (the pairing, the import of a deposit root,
//! a token's registry entry and fee schedules, the claim of a deposit), shared by the guest and the
//! host. The plan is `lez/BRIDGE.md`.
//!
//! | actor | address | state |
//! |---|---|---|
//! | `config(cid)` | PDA `H(…/config ‖ cid)` | [`ConfigState`]: the configuration, paired with its vault; global freezes, caps, `Sunset` |
//! | `token(cid, token)` | PDA `H(…/token ‖ cid ‖ token)` | [`TokenState`]: a registry entry attested from the vault, its fee schedules, the token's freezes and caps |
//! | `root(cid, n)` | PDA `H(…/root ‖ cid ‖ n)` | [`RootState`]: the vault's root at leaf count `n`, written once |
//! | `nullifier(chain, vault, N)` | PDA `H(…/nullifier ‖ chain ‖ vault ‖ N)` | [`NullifierState`]: `N` spent, written once |
//! | `holder(account)` | PDA `H(…/holder ‖ account)` | [`HolderState`]: an account's public wrapped balances (the M0 fallback: no v0.4 token program); written only by `Mint` and the account's own `Burn` |
//! | the account's own actor | the account itself | [`HolderState`] of its own private claims (in a private account, its private state); the entry point of its burns and of its role messages; runs only for its owner |
//! | `burn(cid, H(note))` | PDA `H(…/burn ‖ cid ‖ H(note))` | [`BurnRecord`]: a burn note, written once |
//!
//! `cid` is SPEC-v2 §1's `configId`. The nullifier's address has no `cid`: one namespace for the
//! whole program, so two configurations cannot both mint against one vault deposit.
//!
//! Every address a claim or a burn writes is a PDA whose seed names its role, or the own actor of an
//! account that authorised the call. A mint names its holder and writes only at `holder(account)`,
//! never at the account itself, so no destination, relayer or fee recipient a claim names can be
//! another role's PDA (security review of 11 November 2026, C-1).
//!
//! Message variants 0–3 are the checkpoint's (`checkpoint/crates/checkpoint-core/src/wire.rs`):
//! the checkpoint program decodes every call to this program that a LEZ block shows in the clear
//! with that decoder. `Burn` is byte for byte its variant 0 (185 bytes), `Sunset` its variant 2
//! (33 bytes). `ImportRoot` is its variant 1's four fields followed by the Merkle-Patricia proofs
//! the import needs in the same transaction, which the checkpoint's decoder walks strictly and
//! skips (`lez/BRIDGE-RESULTS.md`).

extern crate alloc;

use alloc::vec::Vec;

use borsh::{BorshDeserialize, BorshSerialize};
use ferrata_bridge_core::{
    chain_id_u256,
    encoding::{config_id_preimage, ConfigIdentity},
    inbound, sha256,
    tree::{MerklePath, TREE_DEPTH},
};
use ferrata_core::{attest::HeaderRecord as ExecutionRecord, witness::ExecutionSummary};
use ferrata_lez_wire::{
    attest::{AccountQuery, Attestation, Fact, Request, SlotQuery, Statement},
    RecordState,
};
use lee_core::{
    account::{AccountId, Actor},
    program::PdaSeed,
};
use sha2::{Digest, Sha256};

pub use ferrata_bridge_core::inbound::DestKind;

/// A 32-byte hash.
pub type Root = [u8; 32];
/// A 20-byte Ethereum address.
pub type Address = [u8; 20];
/// A 256-bit big-endian word.
pub type Word = [u8; 32];

const DOMAIN: &[u8] = b"ferrata-lez/v1/bridge/";

fn tagged_hash(tag: &[u8], parts: &[&[u8]]) -> Root {
    let mut h = Sha256::new();
    h.update(DOMAIN);
    h.update(tag);
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

/// SHA-256 of the concatenation of `parts` (the vault's `sha256(abi.encodePacked(…))`).
fn plain_hash(parts: &[&[u8]]) -> Root {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

/// The vault's storage slots (SPEC-v2 §3.1): the deposit root, the leaf count, `lezConfigId`.
pub const SLOT_ROOT: Word = word_of(0);
pub const SLOT_COUNT: Word = word_of(1);
pub const SLOT_CONFIG_ID: Word = word_of(2);

/// The vault's slot holding the hash of `token`'s registry entry ([`TokenEntry::hash`]):
/// `sha256("ferrata/vault/registry/v1" ‖ token)`. A SHA-256 slot, not a Solidity mapping: the guest
/// has SHA-256 accelerated and no keccak, and the slot does not depend on the vault's layout.
pub fn registry_slot(token: &Address) -> Word {
    plain_hash(&[b"ferrata/vault/registry/v1", token])
}

/// The vault's slot holding the hash of `token`'s fee schedule `index` ([`FeeSchedule::hash`]),
/// zero until that schedule takes effect: `sha256("ferrata/vault/fees/v1" ‖ token ‖ index:u64)`.
pub fn fee_slot(token: &Address, index: u64) -> Word {
    plain_hash(&[b"ferrata/vault/fees/v1", token, &index.to_be_bytes()])
}

/// `n` as a 256-bit big-endian word.
pub const fn word_of(n: u64) -> Word {
    let mut w = [0u8; 32];
    let b = n.to_be_bytes();
    let mut i = 0;
    while i < 8 {
        w[24 + i] = b[i];
        i += 1;
    }
    w
}

/// A word that fits 64 bits, as `u64`.
pub fn u64_of(word: &Word) -> Option<u64> {
    if word[..24] != [0u8; 24] {
        return None;
    }
    Some(u64::from_be_bytes(word[24..].try_into().expect("8 bytes")))
}

/// A word that fits 128 bits, as `u128`.
pub fn u128_of(word: &Word) -> Option<u128> {
    if word[..16] != [0u8; 16] {
        return None;
    }
    Some(u128::from_be_bytes(word[16..].try_into().expect("16 bytes")))
}

/// `n` as a 256-bit big-endian word.
pub fn word_of_u128(n: u128) -> Word {
    let mut w = [0u8; 32];
    w[16..].copy_from_slice(&n.to_be_bytes());
    w
}

// ---------------------------------------------------------------------------------------------
// Bounds (proposal §B4, D13)
// ---------------------------------------------------------------------------------------------

/// At most eight denominations per token (F.8).
pub const MAX_DENOMINATIONS: usize = 8;
/// The immutable fee ceiling: every fee is at most 1/100 of its denomination.
pub const FEE_CEILING_DIVISOR: u128 = 100;
/// The notice of a fee change in Ethereum seconds: seven days.
pub const FEE_NOTICE_SECONDS: u64 = 7 * 24 * 3600;
/// The longest freeze, in milliseconds of attested Ethereum time: 30 days, unless renewed.
pub const MAX_FREEZE_MS: u64 = 30 * 24 * 3600 * 1000;
/// A bucket of this capacity never refuses.
pub const UNLIMITED: u128 = u128::MAX;

// ---------------------------------------------------------------------------------------------
// The registry (F.8, F.10) and fee schedules (F.13)
// ---------------------------------------------------------------------------------------------

/// A token's registry entry, write-once on the vault and created on LEZ by attesting the hash the
/// vault holds at [`registry_slot`]. Amounts are in the token's base units.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TokenEntry {
    pub token: Address,
    /// The wrapped token's definition account on LEZ (zero in the M0 fallback: the bridge keeps
    /// the balances itself, `lez/BRIDGE.md` §3).
    pub lez_definition: Root,
    /// The token's decimals on Ethereum.
    pub decimals: u8,
    /// Base units per LEZ unit: a power of ten (the v0.4 token program has no decimals field, so
    /// the registry holds the conversion).
    pub lez_unit: u128,
    /// Global-cap value units per LEZ unit (the vault's `unitWeight`).
    pub unit_weight: u128,
    /// 1 to 8, strictly increasing, each an exact multiple of `lez_unit`.
    pub denominations: Vec<u128>,
    /// One per denomination, fixed at registration, each at most 1% of it.
    pub relay_fees: Vec<u128>,
}

fn put_word(out: &mut Vec<u8>, v: u128) {
    out.extend_from_slice(&word_of_u128(v));
}

impl TokenEntry {
    /// `sha256("ferrata/registry/v1" ‖ token ‖ lezDefinition ‖ decimals:u8 ‖ lezUnit:u256 ‖
    /// unitWeight:u256 ‖ k:u8 ‖ denominations[k]:u256 ‖ relayFees[k]:u256)`: what the vault holds.
    pub fn hash(&self) -> Root {
        let mut m = Vec::with_capacity(19 + 20 + 32 + 1 + 64 + 1 + 64 * self.denominations.len());
        m.extend_from_slice(b"ferrata/registry/v1");
        m.extend_from_slice(&self.token);
        m.extend_from_slice(&self.lez_definition);
        m.push(self.decimals);
        put_word(&mut m, self.lez_unit);
        put_word(&mut m, self.unit_weight);
        m.push(self.denominations.len() as u8);
        for d in &self.denominations {
            put_word(&mut m, *d);
        }
        for f in &self.relay_fees {
            put_word(&mut m, *f);
        }
        sha256(&m)
    }

    /// The bounds of §B4: 1 to 8 denominations, strictly increasing, each an exact multiple of a
    /// power-of-ten LEZ unit; relay fees at most 1% and multiples of the LEZ unit (they are minted
    /// on LEZ); a non-zero unit weight; and the cap arithmetic of twice the largest denomination
    /// fits 128 bits.
    pub fn check(&self) -> Result<(), u8> {
        let k = self.denominations.len();
        if k == 0 || k > MAX_DENOMINATIONS || self.relay_fees.len() != k || self.unit_weight == 0 {
            return Err(code::BAD_MESSAGE);
        }
        let mut unit = self.lez_unit;
        while unit >= 10 && unit.is_multiple_of(10) {
            unit /= 10;
        }
        if unit != 1 || 10u128.checked_pow(u32::from(self.decimals)).is_some_and(|p| self.lez_unit > p) {
            return Err(code::BAD_MESSAGE);
        }
        let mut previous = 0u128;
        for (d, fee) in self.denominations.iter().zip(&self.relay_fees) {
            if *d <= previous || d % self.lez_unit != 0 || *fee > d / FEE_CEILING_DIVISOR || fee % self.lez_unit != 0 {
                return Err(code::BAD_MESSAGE);
            }
            previous = *d;
        }
        previous.checked_mul(2).and_then(|twice| self.value(twice)).ok_or(code::BAD_MESSAGE)?;
        Ok(())
    }

    /// The index of `amount` among the denominations.
    pub fn index_of(&self, amount: u128) -> Option<usize> {
        self.denominations.iter().position(|d| *d == amount)
    }

    /// `amount` in global-cap value units: `amount / lezUnit × unitWeight`.
    pub fn value(&self, amount: u128) -> Option<u128> {
        (amount / self.lez_unit).checked_mul(self.unit_weight)
    }
}

/// A fee schedule of one token: written by the vault at [`fee_slot`]`(token, index)` when it takes
/// effect, applying to deposits with a leaf index from `from_leaf` until the next schedule's
/// `from_leaf` (a deposit keeps the schedule of its leaf index, §B4). Fees in base units, one per
/// denomination.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FeeSchedule {
    pub index: u64,
    pub from_leaf: u64,
    /// Ethereum seconds: when the admin announced it, and when it took effect.
    pub announced_at: u64,
    pub effective_at: u64,
    /// Minted on LEZ to the fee recipient by a claim (D13).
    pub mint_fees: Vec<u128>,
    /// Deducted from a release on Ethereum (D13); the LEZ side only checks the ceiling.
    pub burn_fees: Vec<u128>,
    pub checkpoint_fees: Vec<u128>,
}

impl FeeSchedule {
    /// `sha256("ferrata/fees/v1" ‖ token ‖ index:u64 ‖ fromLeaf:u64 ‖ announcedAt:u64 ‖
    /// effectiveAt:u64 ‖ k:u8 ‖ mintFees[k] ‖ burnFees[k] ‖ checkpointFees[k])` (u256 words).
    pub fn hash(&self, token: &Address) -> Root {
        let mut m = Vec::with_capacity(15 + 20 + 32 + 1 + 96 * self.mint_fees.len());
        m.extend_from_slice(b"ferrata/fees/v1");
        m.extend_from_slice(token);
        for n in [self.index, self.from_leaf, self.announced_at, self.effective_at] {
            m.extend_from_slice(&n.to_be_bytes());
        }
        m.push(self.mint_fees.len() as u8);
        for list in [&self.mint_fees, &self.burn_fees, &self.checkpoint_fees] {
            for f in list {
                put_word(&mut m, *f);
            }
        }
        sha256(&m)
    }

    /// The bounds of §B4 against `entry`: one fee per denomination, each at most 1% of it, mint
    /// fees multiples of the LEZ unit; schedule 0 (the registration's) charges no protocol fee
    /// ("protocol fees start at zero"); a later one took effect at least seven days of Ethereum time
    /// after it was announced.
    pub fn check(&self, entry: &TokenEntry) -> Result<(), u8> {
        let k = entry.denominations.len();
        if self.mint_fees.len() != k || self.burn_fees.len() != k || self.checkpoint_fees.len() != k {
            return Err(code::BAD_MESSAGE);
        }
        for (i, d) in entry.denominations.iter().enumerate() {
            let ceiling = d / FEE_CEILING_DIVISOR;
            let (mint, burn, checkpoint) = (self.mint_fees[i], self.burn_fees[i], self.checkpoint_fees[i]);
            if mint > ceiling || burn > ceiling || checkpoint > ceiling || mint % entry.lez_unit != 0 {
                return Err(code::BAD_MESSAGE);
            }
            if self.index == 0 && (mint != 0 || burn != 0) {
                return Err(code::BAD_MESSAGE);
            }
        }
        if self.index > 0 && self.announced_at.checked_add(FEE_NOTICE_SECONDS).is_none_or(|t| self.effective_at < t) {
            return Err(code::BAD_MESSAGE);
        }
        Ok(())
    }
}

/// Which fee schedule a claim says its leaf falls under: schedule `index` covers
/// `[from_leaf, until_leaf)`. The claimant checks its leaf index against the range inside its own
/// execution (privately, for a private claim); the token actor checks the range against its
/// schedules. A private claim so publishes the range, not the leaf index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ScheduleRange {
    pub index: u64,
    pub from_leaf: u64,
    pub until_leaf: u64,
}

impl ScheduleRange {
    pub fn covers(&self, leaf_index: u64) -> Result<(), u8> {
        if leaf_index < self.from_leaf || leaf_index >= self.until_leaf {
            return Err(code::LEAF_NOT_IMPORTED);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// Caps (F.11) and freezes (F.14)
// ---------------------------------------------------------------------------------------------

/// A token bucket's parameters, in global-cap value units: `capacity` and the refill per second.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CapParams {
    pub capacity: u128,
    pub refill_per_second: u128,
}

impl CapParams {
    /// At least `floor` in both parameters (the admin cannot cut a cap below the immutable floor).
    pub fn at_least(&self, floor: &CapParams) -> bool {
        self.capacity >= floor.capacity && self.refill_per_second >= floor.refill_per_second
    }
}

/// A token bucket. Invariant: `level <= capacity`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Bucket {
    pub capacity: u128,
    pub refill_per_second: u128,
    /// Available at `last_ms`.
    pub level: u128,
    pub last_ms: u64,
}

impl Bucket {
    pub fn full(p: CapParams) -> Self {
        Self { capacity: p.capacity, refill_per_second: p.refill_per_second, level: p.capacity, last_ms: 0 }
    }

    /// Refill for the time from `last_ms` to `now_ms`. A bucket that has not seen a clock yet
    /// (`last_ms` 0: its gate has read no record) starts counting at the first time it sees,
    /// rather than refilling for the decades since 1970.
    fn refill(&mut self, now_ms: u64) {
        if self.last_ms == 0 {
            self.last_ms = now_ms;
        } else if now_ms > self.last_ms {
            let add = u128::from(now_ms - self.last_ms).saturating_mul(self.refill_per_second) / 1000;
            self.level = self.capacity.min(self.level.saturating_add(add));
            self.last_ms = now_ms;
        }
    }

    /// Take `value` at `now_ms`, refilled first; refused (`CAPPED`) if the bucket holds less.
    pub fn take(&mut self, value: u128, now_ms: u64) -> Result<(), u8> {
        if self.capacity == UNLIMITED {
            return Ok(());
        }
        self.refill(now_ms);
        self.level = self.level.checked_sub(value).ok_or(code::CAPPED)?;
        Ok(())
    }

    /// New parameters at `now_ms`: refilled under the old ones first, the level kept within the
    /// new capacity.
    pub fn set(&mut self, p: CapParams, now_ms: u64) {
        self.refill(now_ms);
        self.capacity = p.capacity;
        self.refill_per_second = p.refill_per_second;
        self.level = self.level.min(p.capacity);
    }
}

/// Freezes and caps of one scope (a token, or the configuration as a whole), and its clock.
///
/// `now_ms` is attested Ethereum time: the newest execution timestamp (in milliseconds) this
/// gate has read from a record of the configuration's attestation instance ([`record_time`]),
/// moved only by `Tick` and `Freeze`, each of which reads such a record, and only forward. No
/// message carries a caller's time, so neither a caller nor the producer of the block (whose
/// timestamp LEZ v0.4.0-rc1 neither bounds nor orders) can move it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Gate {
    pub now_ms: u64,
    /// Mints (claims) or burns are refused while `now_ms < *_until`; 0: not frozen.
    pub mints_until: u64,
    pub burns_until: u64,
    pub mint: Bucket,
    pub burn: Bucket,
}

impl Gate {
    /// Full buckets of `cap`, at attested time `now_ms` (a token starts at its configuration's).
    pub fn new(cap: CapParams, now_ms: u64) -> Self {
        let bucket = Bucket { last_ms: now_ms, ..Bucket::full(cap) };
        Self { now_ms, mints_until: 0, burns_until: 0, mint: bucket, burn: bucket }
    }

    /// Advance the clock to `t`, the time of an attested record, if it is later: monotone.
    pub fn advance(&mut self, t: u64) {
        self.now_ms = self.now_ms.max(t);
    }

    /// Admit `value` minted (`mints`) or burned at the stored time: refused while frozen
    /// (`FROZEN`) or above the bucket (`CAPPED`).
    pub fn admit(&mut self, mints: bool, value: u128) -> Result<(), u8> {
        let now = self.now_ms;
        let (until, bucket) =
            if mints { (self.mints_until, &mut self.mint) } else { (self.burns_until, &mut self.burn) };
        if now < until {
            return Err(code::FROZEN);
        }
        bucket.take(value, now)
    }

    /// Freeze (or, with `until_ms = 0`, unfreeze) mints and/or burns until `until_ms`, which must
    /// lie after the gate's attested time and at most `MAX_FREEZE_MS` after it. The handler first
    /// advances the clock to the record the `Freeze` names.
    pub fn freeze(&mut self, mints: bool, burns: bool, until_ms: u64) -> Result<(), u8> {
        let now = self.now_ms;
        if until_ms != 0 && (until_ms <= now || until_ms - now > MAX_FREEZE_MS) {
            return Err(code::BAD_MESSAGE);
        }
        if mints {
            self.mints_until = until_ms;
        }
        if burns {
            self.burns_until = until_ms;
        }
        Ok(())
    }
}

/// The record a gate reads its time from: the attestation program's `header(instance,
/// record_slot)` of the configuration's own instance, and, for a record in the Gloas layout (which
/// binds the execution block's hash only), that block's RLP header, which must hash to it.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TimeRecord {
    pub record_slot: u64,
    pub header: Option<Vec<u8>>,
}

/// What a gate does once the record it asked for has replied.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Timed {
    /// Advance the clock only.
    Tick,
    /// Advance the clock, then the freeze authority's freeze (checked before the record was asked).
    Freeze { mints: bool, burns: bool, until_ms: u64 },
}

/// A gate between its `Read` of a record and the reply (within one transaction).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PendingTime {
    pub record: TimeRecord,
    pub then: Timed,
}

/// The attested time a record gives: the timestamp of its execution block, in milliseconds.
/// `state` is the record actor's state as its reply carries it (`StateReply::state`), from
/// `header(instance_id, slot)` of the configuration's attestation program (the caller checks the
/// sender); `header` is the block's RLP header for a record in the Gloas layout, `None` for one
/// that holds the payload's fields.
///
/// Refused: an empty actor (`NOT_FOUND`, no record at that slot); a record of another instance or
/// chain, one without an execution block, a header given for a payload record or missing for a
/// Gloas one, or a header that does not hash to the recorded block hash (`IMPORT_MISMATCH`);
/// bytes that are not a record (`BAD_STATE`).
pub fn record_time(instance_id: &Root, evm_chain_id: u64, state: &[u8], header: Option<&[u8]>) -> Result<u64, u8> {
    if state.is_empty() {
        return Err(code::NOT_FOUND);
    }
    let record: RecordState = borsh::from_slice(state).map_err(|_| code::BAD_STATE)?;
    if record.instance_id != *instance_id || record.chain_id != evm_chain_id {
        return Err(code::IMPORT_MISMATCH);
    }
    let recorded = record.header().map_err(|_| code::BAD_STATE)?;
    let execution = match (&recorded.execution, header) {
        (ExecutionSummary::Payload { .. }, None) => ExecutionRecord::from_recorded(&recorded, record.chain_id),
        (ExecutionSummary::BlockHash(_), Some(header)) => {
            ExecutionRecord::from_recorded_block_hash(&recorded, record.chain_id, header)
        }
        _ => return Err(code::IMPORT_MISMATCH),
    }
    .map_err(|_| code::IMPORT_MISMATCH)?;
    execution.timestamp().checked_mul(1000).ok_or(code::IMPORT_MISMATCH)
}

// ---------------------------------------------------------------------------------------------
// The configuration
// ---------------------------------------------------------------------------------------------

/// A configuration's roles and cap defaults, fixed at `Init` (committed by `configId` through the
/// salt). Roles are LEZ accounts acting through their own actor of this program.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Controls {
    /// Sets caps, never below `floor`; cannot freeze or unfreeze.
    pub admin: Root,
    /// Freezes mints and burns (≤ 30 days unless renewed) and runs `Sunset`.
    pub freeze_authority: Root,
    /// Receives the mint fees (and the relay fee of a claim that names no relayer).
    pub fee_recipient: Root,
    /// The immutable floor of every cap.
    pub floor: CapParams,
    /// The caps a token starts with at registration, and the global caps.
    pub token_cap: CapParams,
    pub global_cap: CapParams,
}

impl Controls {
    pub fn check(&self) -> Result<(), u8> {
        if !self.token_cap.at_least(&self.floor) || !self.global_cap.at_least(&self.floor) {
            return Err(code::BAD_MESSAGE);
        }
        Ok(())
    }
}

/// One configuration of the bridge (SPEC-v2 §1).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BridgeConfig {
    pub bedrock_genesis_id: Root,
    pub channel_id: Root,
    /// The bridge program's image id, as the vault's verifier pins it. The program cannot see its
    /// own image id; it is part of `configId`, which the vault pins.
    pub bridge_image_id: Root,
    /// The attestation program whose records the configuration reads Ethereum through.
    pub attestation: AccountId,
    /// The attestation instance those records belong to.
    pub instance_id: Root,
    pub evm_chain_id: u64,
    pub vault: Address,
    pub controls: Controls,
}

impl BridgeConfig {
    /// `H(…/attestation-root ‖ program ‖ instance)`: SPEC-v2's `attestationRootId`. An instance
    /// id alone does not name the program whose records it trusts (LEZ deploys one image at many
    /// addresses), so both enter `configId`.
    pub fn attestation_root_id(&self) -> Root {
        tagged_hash(b"attestation-root", &[self.attestation.value(), &self.instance_id])
    }

    /// `H(…/controls ‖ borsh(controls))`: the prototype's `salt`, so that `configId` (which the
    /// vault pins and holds in slot 2) commits to the roles and cap defaults and a front-run `Init`
    /// cannot install others. Tokens are not in it: each is registered from the vault's registry.
    pub fn salt(&self) -> Root {
        tagged_hash(b"controls", &[&borsh::to_vec(&self.controls).expect("borsh serialization is infallible")])
    }

    /// SPEC-v2 §1 `configId` for this configuration of the bridge program deployed at
    /// `bridge_header`, with the bridge core's preimage.
    pub fn id(&self, bridge_header: AccountId) -> Root {
        sha256(&config_id_preimage(&ConfigIdentity {
            bedrock_genesis_id: self.bedrock_genesis_id,
            channel_id: self.channel_id,
            bridge_header: *bridge_header.value(),
            bridge_image_id: self.bridge_image_id,
            evm_chain_id: chain_id_u256(self.evm_chain_id),
            vault: self.vault,
            attestation_root_id: self.attestation_root_id(),
            salt: self.salt(),
        }))
    }
}

// ---------------------------------------------------------------------------------------------
// Addresses
// ---------------------------------------------------------------------------------------------

pub fn config_seed(config_id: &Root) -> PdaSeed {
    PdaSeed::new(tagged_hash(b"config", &[config_id]))
}

pub fn token_seed(config_id: &Root, token: &Address) -> PdaSeed {
    PdaSeed::new(tagged_hash(b"token", &[config_id, token]))
}

pub fn root_seed(config_id: &Root, leaf_count: u64) -> PdaSeed {
    PdaSeed::new(tagged_hash(b"root", &[config_id, &leaf_count.to_le_bytes()]))
}

pub fn nullifier_seed(evm_chain_id: u64, vault: &Address, nullifier: &Root) -> PdaSeed {
    PdaSeed::new(tagged_hash(b"nullifier", &[&chain_id_u256(evm_chain_id), vault, nullifier]))
}

pub fn burn_seed(config_id: &Root, note_hash: &Root) -> PdaSeed {
    PdaSeed::new(tagged_hash(b"burn", &[config_id, note_hash]))
}

pub fn holder_seed(account: &Root) -> PdaSeed {
    PdaSeed::new(tagged_hash(b"holder", &[account]))
}

fn pda(program: AccountId, seed: &PdaSeed) -> Actor {
    Actor::new(AccountId::for_public_pda(&program, seed), program)
}

pub fn config_actor(program: AccountId, config_id: &Root) -> Actor {
    pda(program, &config_seed(config_id))
}

pub fn token_actor(program: AccountId, config_id: &Root, token: &Address) -> Actor {
    pda(program, &token_seed(config_id, token))
}

pub fn root_actor(program: AccountId, config_id: &Root, leaf_count: u64) -> Actor {
    pda(program, &root_seed(config_id, leaf_count))
}

pub fn nullifier_actor(program: AccountId, evm_chain_id: u64, vault: &Address, nullifier: &Root) -> Actor {
    pda(program, &nullifier_seed(evm_chain_id, vault, nullifier))
}

pub fn burn_actor(program: AccountId, config_id: &Root, note_hash: &Root) -> Actor {
    pda(program, &burn_seed(config_id, note_hash))
}

/// `holder(account)`: the PDA holding `account`'s public wrapped balances. Its seed carries the
/// role, so it is never the address of another actor of the program, whatever account a claim names.
pub fn holder_actor(program: AccountId, account: AccountId) -> Actor {
    pda(program, &holder_seed(account.value()))
}

/// The bridge program's actor at `account` itself: where its owner burns (top-level call or private
/// root), where a role (admin, freeze authority) acts, and where a private claimant claims and keeps
/// its private-kind credit (in a private account, its private state). Every write there needs the
/// owner's authorisation.
pub fn own_actor(program: AccountId, account: AccountId) -> Actor {
    Actor::new(account, program)
}

// ---------------------------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------------------------

/// A burn note (SPEC-v2 §5.1): `(r, memo, mac, token, amount, COut)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Note {
    pub r: Root,
    pub memo: Address,
    pub mac: [u8; 16],
    pub token: Address,
    /// `uint256`, big-endian.
    pub amount: Word,
    pub c_out: Root,
}

impl Note {
    /// `H(…/note ‖ r ‖ memo ‖ mac ‖ token ‖ amount ‖ COut)`: the burn account is keyed on the
    /// whole note (D13), so copying a pending burn's tag alone cannot make it revert.
    pub fn hash(&self) -> Root {
        tagged_hash(b"note", &[&self.r, &self.memo, &self.mac, &self.token, &self.amount, &self.c_out])
    }
}

/// First element of an `account` list that carries the block's RLP header in front of the
/// account's Merkle-Patricia nodes: `[HEADER_MARKER, header, nodes…]`. A record in the Gloas
/// layout binds the block hash only, and the attestation program takes the state root from the
/// header (`Request::header`). A trie node is an RLP list (first byte at least `0xc0`), so this
/// tag (first byte `f`) is never one. Carrying the header inside the list keeps `ImportRoot`'s
/// encoding, which the checkpoint walks list by list, as it is.
pub const HEADER_MARKER: &[u8] = b"ferrata-lez/v1/bridge/rlp-header";

/// `account` with a header in front of it, as [`split_header`] reads it.
pub fn with_header(header: Vec<u8>, account: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    let mut out = alloc::vec![HEADER_MARKER.to_vec(), header];
    out.extend(account);
    out
}

/// The header an `account` list carries, if any, and the account's own nodes.
pub fn split_header(account: &[Vec<u8>]) -> (Option<Vec<u8>>, Vec<Vec<u8>>) {
    match account {
        [marker, header, nodes @ ..] if marker.as_slice() == HEADER_MARKER => (Some(header.clone()), nodes.to_vec()),
        _ => (None, account.to_vec()),
    }
}

/// The proofs of an import: the vault's account and its slots 0 and 1 (`eth_getProof`'s
/// `accountProof` and `storageProof[i].proof`); `account` may carry the block's header in front
/// ([`with_header`]). Its encoding is what the checkpoint skips.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ImportProof {
    pub account: Vec<Vec<u8>>,
    pub root_slot: Vec<Vec<u8>>,
    pub count_slot: Vec<Vec<u8>>,
}

/// The proofs of the pairing: the vault's account and its slot 2; `account` may carry the
/// block's header in front ([`with_header`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PairingProof {
    pub account: Vec<Vec<u8>>,
    pub config_slot: Vec<Vec<u8>>,
}

/// The proofs of a registration or a fee import: the vault's account and one storage proof per
/// slot asked, in order; `account` may carry the block's header in front ([`with_header`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct VaultProof {
    pub account: Vec<Vec<u8>>,
    pub slots: Vec<Vec<Vec<u8>>>,
}

/// What a claimant knows about one deposit (`ferrata_bridge_core::inbound::ClaimWitness`). The
/// path's direction bits are the leaf index itself, so they cannot disagree with it (D9).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ClaimWitness {
    pub depositor: Address,
    pub token: Address,
    /// `uint256`, big-endian.
    pub amount: Word,
    pub leaf_index: u64,
    /// The 32 siblings, leaf level first.
    pub path: Vec<Root>,
    pub s: Root,
    /// `0x01` private, `0x02` public (SPEC-v2 §3.3).
    pub kind: u8,
    pub dest: Root,
}

/// Every message of the bridge program. The order of the variants is the encoding: 0–3 are the
/// checkpoint's (module docs). One message is decoded per handler execution, so the variants' size
/// difference (`WriteToken` carries a whole token state) costs nothing; boxing would only
/// complicate the handlers' patterns.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum BridgeMessage {
    /// Variant 0. At the holder's own actor, as a top-level call or the root of a private
    /// execution: debit one denomination (from its own state, or else from `holder(account)`, to
    /// which it forwards the note) and send the note to `token(cid, token)`, which admits it (the
    /// denomination, the burn freezes and caps) and records it once at `burn(cid, H(note))`.
    Burn { config_id: Root, r: Root, memo: Address, mac: [u8; 16], token: Address, amount: Word, c_out: Root },
    /// Variant 1. At `config(cid)`: attest the vault's slots 0 and 1 under the record at
    /// `record_slot` and write `root(cid, leaf_count)` if they hold `root` and `leaf_count`.
    /// Refused after `Sunset`.
    ImportRoot { config_id: Root, record_slot: u64, root: Root, leaf_count: u64, proof: ImportProof },
    /// Variant 2. The configuration's one-way `Sunset`: a top-level call (or private root) at the
    /// freeze authority's own actor, forwarded to `config(cid)`; stops `ImportRoot` for good.
    Sunset { config_id: Root },
    /// Variant 3. Rule (A)'s echo nonce: accepted anywhere, changes nothing.
    Echo { nonce: Root },
    /// Variant 4. At `config(cid)`, `cid` computed from `config`: attest the vault's slot 2 and
    /// write the configuration if it holds `cid` (the reciprocal pairing, B5).
    Init { config: BridgeConfig, record_slot: u64, proof: PairingProof },
    /// Variant 5. At `root(cid, n)`: the plaintext claim, for a public destination. `relayer`
    /// receives the relay fee (`None`: the fee recipient). It carries no time: the token's and the
    /// configuration's attested clocks apply.
    Claim { config_id: Root, leaf_count: u64, witness: ClaimWitness, range: ScheduleRange, relayer: Option<Root> },
    /// Variant 6. At the claimant's own actor, meant to run privately: read `root(cid, n)`, verify
    /// the claim inside this execution, then `Issue`; a private destination must be this account.
    ClaimPrivately {
        config_id: Root,
        leaf_count: u64,
        witness: ClaimWitness,
        range: ScheduleRange,
        relayer: Option<Root>,
    },
    /// Variant 7. Reply with this actor's state (`StateReply`), LEZ v0.4's read idiom.
    Read,
    /// Variant 8. `config(cid)` → `root(cid, n)`: the verified root, written once.
    WriteRoot { root: RootState },
    /// Variant 9. `token(cid, token)` → `nullifier(chain, vault, N)`: spend `N` once; mint the
    /// denomination to `mint_to` (a public account) if given, and each fee to its account.
    Spend {
        nullifier: Root,
        config_id: Root,
        evm_chain_id: u64,
        vault: Address,
        token: Address,
        leaf_count: u64,
        amount: u128,
        mint_to: Option<Root>,
        fees: Vec<(Root, u128)>,
    },
    /// Variant 10. `nullifier(chain, vault, N)` → `holder(holder)`: credit `amount` of `token`.
    Mint {
        nullifier: Root,
        config_id: Root,
        evm_chain_id: u64,
        vault: Address,
        token: Address,
        holder: Root,
        amount: u128,
    },
    /// Variant 11. At `config(cid)`: attest the vault's registry entry of `entry.token`, its fee
    /// schedules 0 and 1 and its leaf count under one record, and write `token(cid, token)`.
    Register { config_id: Root, entry: TokenEntry, schedule: FeeSchedule, record_slot: u64, proof: VaultProof },
    /// Variant 12. At `config(cid)`: attest the vault's fee schedule `index` of `token` (its hash,
    /// or zero if `schedule` is `None`) and its leaf count, and extend `token(cid, token)`.
    ImportFees {
        config_id: Root,
        token: Address,
        index: u64,
        schedule: Option<FeeSchedule>,
        record_slot: u64,
        proof: VaultProof,
    },
    /// Variant 13. `config(cid)` → `token(cid, token)`: the attested entry, written once.
    WriteToken { state: TokenState },
    /// Variant 14. `config(cid)` → `token(cid, token)`: an attested fee schedule (appended), or
    /// the attested absence of schedule `index` at a leaf count (the schedules cover up to it).
    ExtendFees { config_id: Root, token: Address, index: u64, schedule: Option<FeeSchedule>, count: u64 },
    /// Variant 15. A claimant (the root of a plaintext claim, or a private claimant) →
    /// `token(cid, token)` after verifying the deposit: the denomination, the fee schedule of the
    /// range, the mint freezes and caps; then `Admit` to `config(cid)` and `Spend`.
    Issue {
        config_id: Root,
        token: Address,
        nullifier: Root,
        amount: u128,
        range: ScheduleRange,
        leaf_count: u64,
        mint_to: Option<Root>,
        relayer: Option<Root>,
    },
    /// Variant 16. `token(cid, token)` → `config(cid)`: the global freeze and bucket of a mint or a
    /// burn of `value` units, at the configuration's attested time.
    Admit { config_id: Root, token: Address, mints: bool, value: u128 },
    /// Variant 17. The freeze authority, like `Sunset`: freeze mints and/or burns of `token` (or of
    /// the whole configuration) until `until_ms` (0 unfreezes). The target first reads `record`
    /// (as `Tick` does); `until_ms` must lie after that attested time and at most 30 days after it.
    Freeze { config_id: Root, token: Option<Address>, mints: bool, burns: bool, until_ms: u64, record: TimeRecord },
    /// Variant 18. The admin, like `Sunset`: set the mint (or burn) bucket of `token` (or the
    /// global one), never below the floor; refilled at the target's attested time first.
    SetCap { config_id: Root, token: Option<Address>, mints: bool, cap: CapParams },
    /// Variant 19. Anyone, at `config(cid)` or `token(cid, token)`: read the attestation
    /// program's `record` and advance the target's clock to its execution timestamp; the
    /// transaction is valid only in a block stamped at or after that time.
    Tick { config_id: Root, token: Option<Address>, record: TimeRecord },
}

impl BridgeMessage {
    /// The `Burn` message of `note` under `config_id`.
    pub fn burn(config_id: Root, note: &Note) -> Self {
        BridgeMessage::Burn {
            config_id,
            r: note.r,
            memo: note.memo,
            mac: note.mac,
            token: note.token,
            amount: note.amount,
            c_out: note.c_out,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------------------------

/// The state of an actor of the bridge program, by role. One value is decoded per handler
/// execution, so the variants' size difference costs nothing; boxing would only complicate the
/// handlers' patterns.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum BridgeState {
    Config(ConfigState),
    Root(RootState),
    Nullifier(NullifierState),
    Holder(HolderState),
    Burn(BurnRecord),
    Token(TokenState),
}

/// `config(cid)`.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ConfigState {
    /// `Init` asked for the vault's slot 2 and the reply is outstanding (within one transaction).
    Pairing { config: BridgeConfig, record_slot: u64 },
    /// The configuration, paired with its vault. `pending` is an attestation outstanding within
    /// one transaction (`None` between transactions); `gate` the global freezes and caps;
    /// `sunset` the one-way retirement.
    Ready { config: BridgeConfig, pending: Option<Pending>, gate: Gate, sunset: bool },
}

impl ConfigState {
    pub fn config(&self) -> &BridgeConfig {
        match self {
            ConfigState::Pairing { config, .. } | ConfigState::Ready { config, .. } => config,
        }
    }

    /// The record actor whose reply this state waits for.
    pub fn awaiting(&self) -> Option<Actor> {
        let slot = match self {
            ConfigState::Pairing { record_slot, .. } => *record_slot,
            ConfigState::Ready { pending: Some(p), .. } => p.record_slot(),
            ConfigState::Ready { pending: None, .. } => return None,
        };
        let config = self.config();
        Some(ferrata_lez_wire::header_actor(config.attestation, &config.instance_id, slot))
    }
}

/// An attestation of `config(cid)` between its `Attest` and the reply.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Pending {
    Import(PendingImport),
    Register {
        entry: TokenEntry,
        schedule: FeeSchedule,
        record_slot: u64,
    },
    Fees {
        token: Address,
        index: u64,
        schedule: Option<FeeSchedule>,
        record_slot: u64,
    },
    /// A `Read` of a record for the global gate's clock (`Tick`, `Freeze`).
    Time(PendingTime),
}

impl Pending {
    pub fn record_slot(&self) -> u64 {
        match self {
            Pending::Import(p) => p.record_slot,
            Pending::Register { record_slot, .. } | Pending::Fees { record_slot, .. } => *record_slot,
            Pending::Time(p) => p.record.record_slot,
        }
    }
}

/// An import between its `Attest` and the reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PendingImport {
    pub record_slot: u64,
    pub root: Root,
    pub leaf_count: u64,
}

/// `root(cid, n)`: the vault's deposit root when it held `leaf_count` leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RootState {
    pub config_id: Root,
    pub evm_chain_id: u64,
    pub vault: Address,
    pub leaf_count: u64,
    pub root: Root,
    /// The beacon slot of the record the root was attested under, and its execution block.
    pub record_slot: u64,
    pub block_number: u64,
}

/// `token(cid, token)`: a registered token of one configuration.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TokenState {
    pub config_id: Root,
    pub evm_chain_id: u64,
    pub vault: Address,
    /// The attestation program and instance the configuration reads Ethereum through (copied at
    /// registration): the token's clock reads their records.
    pub attestation: AccountId,
    pub instance_id: Root,
    pub entry: TokenEntry,
    /// By index; `from_leaf` non-decreasing.
    pub schedules: Vec<FeeSchedule>,
    /// Every leaf index below this falls under a known schedule.
    pub known_until: u64,
    /// The configuration's roles and floor (copied at registration; immutable).
    pub controls: Controls,
    pub gate: Gate,
    /// A `Read` of a record for the gate's clock, outstanding within one transaction (`None`
    /// between transactions).
    pub pending: Option<PendingTime>,
}

impl TokenState {
    /// The record actor whose reply this token waits for.
    pub fn awaiting(&self) -> Option<Actor> {
        let pending = self.pending.as_ref()?;
        Some(ferrata_lez_wire::header_actor(self.attestation, &self.instance_id, pending.record.record_slot))
    }

    /// The denomination index and the mint and relay fees of a claim of `amount` whose leaf lies in
    /// `range`: `range` must start at its schedule's `from_leaf` and end no later than the next
    /// schedule's, or than `known_until` for the last one.
    pub fn fees_for(&self, amount: u128, range: &ScheduleRange) -> Result<(u128, u128), u8> {
        let d = self.entry.index_of(amount).ok_or(code::NOT_REGISTERED)?;
        let index = usize::try_from(range.index).map_err(|_| code::LEAF_NOT_IMPORTED)?;
        let schedule = self.schedules.get(index).ok_or(code::LEAF_NOT_IMPORTED)?;
        let bound = self.schedules.get(index + 1).map_or(self.known_until, |next| next.from_leaf);
        if range.from_leaf != schedule.from_leaf || range.until_leaf > bound || range.until_leaf <= range.from_leaf {
            return Err(code::LEAF_NOT_IMPORTED);
        }
        Ok((schedule.mint_fees[d], self.entry.relay_fees[d]))
    }
}

/// `nullifier(chain, vault, N)`: spent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NullifierState {
    pub config_id: Root,
    pub token: Address,
    /// The leaf count of the root the claim read.
    pub leaf_count: u64,
    pub amount: u128,
}

/// A holder's wrapped balance of one token under one configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Holding {
    pub config_id: Root,
    pub token: Address,
    /// In the token's base units: a sum of whole denominations for users; the fee recipient and
    /// relayers also hold fees.
    pub amount: u128,
}

/// `holder(account)`, or an account's own actor (its own private claims).
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct HolderState {
    pub holdings: Vec<Holding>,
    /// A private claim between its `Read` and the reply (within one transaction).
    pub pending: Option<PendingClaim>,
}

impl HolderState {
    /// Add `amount` of `token` to the holding under `config_id`.
    pub fn credit(&mut self, config_id: &Root, token: &Address, amount: u128) -> Option<()> {
        match self.holdings.iter_mut().find(|h| h.config_id == *config_id && h.token == *token) {
            Some(h) => h.amount = h.amount.checked_add(amount)?,
            None => self.holdings.push(Holding { config_id: *config_id, token: *token, amount }),
        }
        Some(())
    }

    /// The holding of `token` under `config_id`.
    pub fn holding(&self, config_id: &Root, token: &Address) -> Option<&Holding> {
        self.holdings.iter().find(|h| h.config_id == *config_id && h.token == *token)
    }

    /// Take `amount` of `token` under `config_id`, if held (an emptied holding is removed).
    pub fn debit(&mut self, config_id: &Root, token: &Address, amount: u128) -> Option<()> {
        let index = self.holdings.iter().position(|h| h.config_id == *config_id && h.token == *token)?;
        let left = self.holdings[index].amount.checked_sub(amount)?;
        if left == 0 {
            self.holdings.remove(index);
        } else {
            self.holdings[index].amount = left;
        }
        Some(())
    }
}

/// A private claim between its `Read` and the reply.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PendingClaim {
    pub config_id: Root,
    pub leaf_count: u64,
    pub witness: ClaimWitness,
    pub range: ScheduleRange,
    pub relayer: Option<Root>,
}

/// `burn(cid, H(note))`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BurnRecord {
    pub config_id: Root,
    pub note: Note,
}

// ---------------------------------------------------------------------------------------------
// Checks
// ---------------------------------------------------------------------------------------------

/// The attestation request of the vault's account and `slots`. `account` may carry the block's
/// header in front ([`with_header`]): a record in the Gloas layout binds the block hash only, and
/// the attestation program then takes the state root from that header.
fn vault_query(config: &BridgeConfig, account: &[Vec<u8>], slots: Vec<SlotQuery>) -> Request {
    let (header, account) = split_header(account);
    Request {
        instance_id: config.instance_id,
        chain_id: config.evm_chain_id,
        header,
        accounts: alloc::vec![AccountQuery { address: config.vault, proof: account, conditions: Vec::new(), slots }],
        ..Request::default()
    }
}

fn slot_query(slot: Word, proof: &[Vec<u8>]) -> SlotQuery {
    SlotQuery { slot, proof: proof.to_vec(), conditions: Vec::new() }
}

/// The attestation request of an import: the vault's account and its slots 0 and 1.
pub fn import_request(config: &BridgeConfig, proof: &ImportProof) -> Request {
    vault_query(
        config,
        &proof.account,
        alloc::vec![slot_query(SLOT_ROOT, &proof.root_slot), slot_query(SLOT_COUNT, &proof.count_slot)],
    )
}

/// The attestation request of the pairing: the vault's account and its slot 2.
pub fn pairing_request(config: &BridgeConfig, proof: &PairingProof) -> Request {
    vault_query(config, &proof.account, alloc::vec![slot_query(SLOT_CONFIG_ID, &proof.config_slot)])
}

/// The slots a registration of `token` attests: its entry, its fee schedules 0 and 1, the leaf count.
pub fn register_slots(token: &Address) -> [Word; 4] {
    [registry_slot(token), fee_slot(token, 0), fee_slot(token, 1), SLOT_COUNT]
}

/// The slots a fee import attests: schedule `index` of `token`, the leaf count.
pub fn fees_slots(token: &Address, index: u64) -> [Word; 2] {
    [fee_slot(token, index), SLOT_COUNT]
}

/// The attestation request of the vault's account and `slots`, one proof each.
pub fn vault_request(config: &BridgeConfig, slots: &[Word], proof: &VaultProof) -> Result<Request, u8> {
    if proof.slots.len() != slots.len() {
        return Err(code::BAD_MESSAGE);
    }
    Ok(vault_query(config, &proof.account, slots.iter().zip(&proof.slots).map(|(s, p)| slot_query(*s, p)).collect()))
}

/// The values of the vault's slots that `attestation` proves, in the order asked, if it is an
/// attestation of exactly the vault's account and those slots, for the configuration's instance
/// and chain, at the record of `record_slot`, without hops.
pub fn vault_slots<const K: usize>(
    config: &BridgeConfig,
    record_slot: u64,
    attestation: &Attestation,
    slots: [Word; K],
) -> Option<[Word; K]> {
    if attestation.instance_id != config.instance_id
        || attestation.chain_id != config.evm_chain_id
        || attestation.slot != record_slot
        || attestation.hops != 0
        || attestation.statements.len() != K + 1
    {
        return None;
    }
    let vault = config.vault;
    let [Statement { fact: Fact::Account { address, .. }, .. }, rest @ ..] = attestation.statements.as_slice() else {
        return None;
    };
    if *address != vault {
        return None;
    }
    let mut values = [[0u8; 32]; K];
    for (i, statement) in rest.iter().enumerate() {
        let Fact::Storage { address, slot, value } = &statement.fact else {
            return None;
        };
        if *address != vault || *slot != slots[i] {
            return None;
        }
        values[i] = *value;
    }
    Some(values)
}

/// Check the reply to an import: the vault's slots 0 and 1 hold the message's root and count.
pub fn check_import(
    config: &BridgeConfig,
    config_id: &Root,
    pending: &PendingImport,
    attestation: &Attestation,
) -> Result<RootState, u8> {
    let [root, count] =
        vault_slots(config, pending.record_slot, attestation, [SLOT_ROOT, SLOT_COUNT]).ok_or(code::IMPORT_MISMATCH)?;
    if root != pending.root || count != word_of(pending.leaf_count) {
        return Err(code::IMPORT_MISMATCH);
    }
    Ok(RootState {
        config_id: *config_id,
        evm_chain_id: config.evm_chain_id,
        vault: config.vault,
        leaf_count: pending.leaf_count,
        root,
        record_slot: pending.record_slot,
        block_number: attestation.block_number,
    })
}

/// Check the reply to `Init`: the vault's slot 2 holds this configuration's id.
pub fn check_pairing(
    config: &BridgeConfig,
    config_id: &Root,
    record_slot: u64,
    attestation: &Attestation,
) -> Result<(), u8> {
    let [held] = vault_slots(config, record_slot, attestation, [SLOT_CONFIG_ID]).ok_or(code::IMPORT_MISMATCH)?;
    if held != *config_id {
        return Err(code::PAIRING_MISMATCH);
    }
    Ok(())
}

/// Check the reply to `Register`: the vault holds `entry`'s hash and schedule 0's; the token's
/// state, whose schedules cover the leaves up to the attested count if schedule 1 does not exist
/// yet (else up to schedule 0's start: `ImportFees` extends it), and whose clock starts at the
/// configuration's attested time `now_ms`.
pub fn check_register(
    config: &BridgeConfig,
    config_id: &Root,
    entry: &TokenEntry,
    schedule: &FeeSchedule,
    record_slot: u64,
    attestation: &Attestation,
    now_ms: u64,
) -> Result<TokenState, u8> {
    let [held_entry, held_fees, next_fees, count] =
        vault_slots(config, record_slot, attestation, register_slots(&entry.token)).ok_or(code::IMPORT_MISMATCH)?;
    let count = u64_of(&count).ok_or(code::IMPORT_MISMATCH)?;
    if held_entry != entry.hash() || held_fees != schedule.hash(&entry.token) || schedule.from_leaf > count {
        return Err(code::IMPORT_MISMATCH);
    }
    let known_until = if next_fees == [0u8; 32] { count } else { schedule.from_leaf };
    Ok(TokenState {
        config_id: *config_id,
        evm_chain_id: config.evm_chain_id,
        vault: config.vault,
        attestation: config.attestation,
        instance_id: config.instance_id,
        entry: entry.clone(),
        schedules: alloc::vec![schedule.clone()],
        known_until,
        controls: config.controls,
        gate: Gate::new(config.controls.token_cap, now_ms),
        pending: None,
    })
}

/// Check the reply to `ImportFees`: the vault's slot of schedule `index` holds `schedule`'s hash
/// (or zero, for `None`); gives the attested leaf count.
pub fn check_fees(
    config: &BridgeConfig,
    token: &Address,
    index: u64,
    schedule: &Option<FeeSchedule>,
    record_slot: u64,
    attestation: &Attestation,
) -> Result<u64, u8> {
    let [held, count] =
        vault_slots(config, record_slot, attestation, fees_slots(token, index)).ok_or(code::IMPORT_MISMATCH)?;
    let expected = match schedule {
        Some(s) if s.index == index => s.hash(token),
        Some(_) => return Err(code::IMPORT_MISMATCH),
        None => [0u8; 32],
    };
    if held != expected {
        return Err(code::IMPORT_MISMATCH);
    }
    u64_of(&count).ok_or(code::IMPORT_MISMATCH)
}

/// What a verified claim gives: the nullifier, the amount and the destination's kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Claimed {
    pub nullifier: Root,
    pub amount: u128,
    pub kind: DestKind,
}

/// The claim statement (SPEC-v2 §3.3, D9, D13) against an imported root: the kind is known, the
/// leaf index is below the root's count and inside the claimed fee schedule range; then the
/// bridge core recomputes `C` and the leaf from the witness, checks the path (direction bits = the
/// leaf index) to the root, and gives `N = H(s ‖ fid)` with the configuration's chain and vault.
/// The token and the denomination are the token actor's to check (`Issue`).
pub fn verify_claim(root: &RootState, w: &ClaimWitness, range: &ScheduleRange) -> Result<Claimed, u8> {
    let kind = DestKind::from_byte(w.kind).ok_or(code::CLAIM_INVALID)?;
    if w.leaf_index >= root.leaf_count {
        return Err(code::LEAF_NOT_IMPORTED);
    }
    range.covers(w.leaf_index)?;
    let directions = u32::try_from(w.leaf_index).map_err(|_| code::LEAF_NOT_IMPORTED)?;
    let amount = u128_of(&w.amount).ok_or(code::NOT_REGISTERED)?;
    let siblings: [Root; TREE_DEPTH] = w.path.as_slice().try_into().map_err(|_| code::CLAIM_INVALID)?;
    let witness = inbound::ClaimWitness {
        depositor: w.depositor,
        token: w.token,
        amount: w.amount,
        leaf_index: w.leaf_index,
        path: MerklePath { directions, siblings },
        s: w.s,
        kind,
        dest: w.dest,
    };
    let nullifier = inbound::verify_claim(&root.root, &witness, &chain_id_u256(root.evm_chain_id), &root.vault)
        .map_err(|_| code::CLAIM_INVALID)?;
    Ok(Claimed { nullifier, amount, kind })
}

// ---------------------------------------------------------------------------------------------
// The D12/D13 outbound encodings the bridge core does not have yet (wallet and release side; the
// program never sees `sOut` or `kView`)
// ---------------------------------------------------------------------------------------------

/// D13: `NOut = H("ferrata/nf-out/v2" ‖ sOut)`, independent of the burn's index (the bridge core's,
/// which the release guest commits).
pub fn nf_out_v2(s_out: &Root) -> Root {
    ferrata_bridge_core::outbound::burn_nullifier(s_out)
}

/// D12: `mac = HMAC(kView, "ferrata/burn-mac/v2" ‖ r ‖ memo ‖ token ‖ amount ‖ COut)[0..16]` (the
/// bridge core's, which wallets make and check).
pub fn burn_mac_v2(
    k_view: &ferrata_bridge_core::BridgeKey,
    r: &Root,
    memo: &Address,
    token: &Address,
    amount: &Word,
    c_out: &Root,
) -> [u8; 16] {
    ferrata_bridge_core::outbound::burn_mac(k_view, r, memo, token, amount, c_out)
}

// ---------------------------------------------------------------------------------------------
// Exit codes
// ---------------------------------------------------------------------------------------------

/// The exit codes the bridge program owns: 240–255, the block `ferrata_lez_wire::CODE_BLOCKS`
/// leaves unassigned, so that a refusal of the attestation program (1–219) inside a bridge
/// transaction stays distinguishable. Sixteen codes; related refusals share one.
pub const CODE_BLOCK: core::ops::RangeInclusive<u8> = 240..=255;

macro_rules! exit_codes {
    ($($(#[doc = $doc:literal])* $name:ident = $value:literal;)*) => {
        $($(#[doc = $doc])* pub const $name: u8 = $value;)*
        /// Every code of this module, with its name.
        pub const ALL: &[(&str, u8)] = &[$((stringify!($name), $name)),*];
    };
}

pub mod code {
    exit_codes! {
        /// A message that does not decode or that this actor does not take, or values outside the
        /// specified bounds: more than eight denominations, unsorted or zero, not multiples of a
        /// power-of-ten LEZ unit, a fee above 1% of its denomination, a protocol fee in schedule 0,
        /// a fee change with less than seven days' notice, a freeze beyond 30 days, a cap below the
        /// floor.
        BAD_MESSAGE = 240;
        /// Stored state that does not decode, or is another role's, or a transaction-internal
        /// step (a pending attestation or claim) found where none can be.
        BAD_STATE = 241;
        /// Mints or burns frozen (of the token, or of the whole configuration), or an import
        /// after `Sunset`.
        FROZEN = 242;
        /// An internal message from an actor other than the expected one, a read with nobody to
        /// reply to, or a receiver that is not the actor the message names.
        WRONG_SENDER = 243;
        /// A write-once account already written: the configuration, a root at the same leaf
        /// count, a token's entry, a burn note, the sunset.
        ALREADY_WRITTEN = 244;
        /// No configuration (an import before `Init`), or no imported root (a claim).
        NOT_FOUND = 245;
        /// The vault's slot 2 does not hold the configuration's id.
        PAIRING_MISMATCH = 246;
        /// The attested facts are not the vault's slots holding what the message says (the root
        /// and count, the registry entry, a fee schedule, a schedule out of order), or are of
        /// another instance, chain or record.
        IMPORT_MISMATCH = 247;
        /// A leaf index at or above the imported root's leaf count, or outside the fee schedule
        /// range the claim names, or beyond the leaves the imported fee schedules cover.
        LEAF_NOT_IMPORTED = 248;
        /// The opening, the leaf and the path do not hash to the root (a wrong leaf, path or
        /// destination), or an unknown kind.
        CLAIM_INVALID = 249;
        /// The deposit's nullifier is spent.
        ALREADY_CLAIMED = 250;
        /// A token with no registry entry in the configuration, or an amount that is not one of
        /// its denominations.
        NOT_REGISTERED = 251;
        /// A destination this path cannot credit: a plaintext claim of a private-kind deposit, or
        /// a private-kind claim not run as its destination.
        WRONG_DESTINATION = 252;
        /// A burn not authorised by its holder, or not a top-level call or private root (D10); an
        /// `ImportRoot` that is not a top-level call or private root (D10, security review H-1); a
        /// control message (`Sunset`, `Freeze`, `SetCap`) not from the configured role, or not a
        /// top-level call at its own actor.
        NOT_AUTHORIZED = 253;
        /// A holding smaller than the denomination burned.
        INSUFFICIENT_BALANCE = 254;
        /// The token's or the global mint or burn bucket holds less than the amount.
        CAPPED = 255;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_header_travels_in_front_of_the_account_proof() {
        let nodes = alloc::vec![alloc::vec![0xf8, 0x51, 0x80], alloc::vec![0xe2, 0x01]];
        let header = alloc::vec![0xf9, 0x02, 0x5a, 0xa0];
        assert_eq!(split_header(&nodes), (None, nodes.clone()));
        assert_eq!(split_header(&with_header(header.clone(), nodes.clone())), (Some(header.clone()), nodes.clone()));
        // A list that starts with the marker but carries nothing after it is not a header.
        let lone = alloc::vec![HEADER_MARKER.to_vec()];
        assert_eq!(split_header(&lone), (None, lone.clone()));
        // A trie node is an RLP list: its first byte is at least 0xc0, the marker's is not.
        assert!(HEADER_MARKER[0] < 0xc0);
    }
}
