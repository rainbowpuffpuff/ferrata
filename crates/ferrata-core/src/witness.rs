//! Compact forms of the store, committees and updates, for a program's actor state and messages;
//! their strict encodings; and committee adoption from an uncompressed-key witness.
//!
//! A program that runs the light client across several transactions (the LEZ v0.4 program in
//! `lez/`) cannot afford to hold two 512-key committees (49 KB) in the state a handler loads on
//! every call. It keeps each committee as a [`CommitteeSummary`]: the root of its 512 key leaves
//! and its aggregate key, 80 bytes, from which the committee's `hash_tree_root` follows. Keys
//! then arrive as witnesses:
//!
//! * [`SignerWitness`]: the non-signers of one update, uncompressed, each with its Merkle branch
//!   to the summary's key root. The signing key is `aggregate − Σ non-signers`
//!   (see [`crate::steps`]).
//! * [`AdoptionProgress`]: the whole committee, uncompressed, in one or more parts, when a store
//!   adopts it. Every key must be a curve point, not the identity, and re-compress to the bytes
//!   committed under the key root; the keys must sum to the aggregate. That is
//!   [`BlsBackend::check_committee_onchain`](crate::bls::BlsBackend::check_committee_onchain)
//!   without its 512 decompressions; like it, it leaves out the per-key subgroup check.
//!
//! [`HeaderRecord`] is what a store records about each finalized header, the state of the
//! program's write-once `header(id, slot)` actor.
//!
//! Every encoding here is strict: fixed field order, every length checked, flags in `{0, 1}`, and
//! the input consumed exactly. Headers are carried as SSZ and decoded by the strict SSZ decoders
//! of [`crate::types`].

use alloc::{vec, vec::Vec};

use crate::{
    fork::{ChainSpec, LcFork},
    hash::{hash_pair, sha256},
    merkle::{floorlog2, is_zero_branch, merkleize},
    store::StoreCore,
    types::{
        BeaconBlockHeader, BlsPubkey, Execution, ExecutionPayloadHeader, LightClientBootstrap, LightClientHeader,
        LightClientUpdate, SyncAggregate, SyncCommittee,
    },
    validate::get_lc_execution_root,
    Error, Root,
};

/// An uncompressed G1 point: big-endian `x ‖ y`, the ZCash serialization with the compression
/// flag clear (blst's `blst_p1_affine_serialize`).
pub type UncompressedPubkey = [u8; 96];

// ---------------------------------------------------------------------------------------------
// Strict byte codec
// ---------------------------------------------------------------------------------------------

/// Appends fields in a fixed order.
#[derive(Default)]
pub(crate) struct Writer {
    pub(crate) out: Vec<u8>,
}

impl Writer {
    pub(crate) fn u8(&mut self, v: u8) {
        self.out.push(v);
    }
    pub(crate) fn u16(&mut self, v: u16) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    pub(crate) fn u32(&mut self, v: u32) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    pub(crate) fn u64(&mut self, v: u64) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    pub(crate) fn bytes(&mut self, b: &[u8]) {
        self.out.extend_from_slice(b);
    }
    /// A `u32` length, then the bytes.
    pub(crate) fn prefixed(&mut self, b: &[u8]) {
        self.u32(u32::try_from(b.len()).expect("encoded field fits u32"));
        self.bytes(b);
    }
    pub(crate) fn roots(&mut self, roots: &[Root]) {
        for r in roots {
            self.bytes(r);
        }
    }
}

/// Reads fields in a fixed order; every read is bounds-checked.
pub(crate) struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(b: &'a [u8]) -> Self {
        Self { b, pos: 0 }
    }
    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let end = self.pos.checked_add(n).ok_or(Error::Malformed)?;
        let out = self.b.get(self.pos..end).ok_or(Error::Malformed)?;
        self.pos = end;
        Ok(out)
    }
    pub(crate) fn array<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }
    pub(crate) fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }
    pub(crate) fn u16(&mut self) -> Result<u16, Error> {
        Ok(u16::from_le_bytes(self.array()?))
    }
    pub(crate) fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.array()?))
    }
    pub(crate) fn u64(&mut self) -> Result<u64, Error> {
        Ok(u64::from_le_bytes(self.array()?))
    }
    /// A presence flag: 0 or 1, nothing else.
    pub(crate) fn flag(&mut self) -> Result<bool, Error> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::Malformed),
        }
    }
    pub(crate) fn prefixed(&mut self) -> Result<&'a [u8], Error> {
        let n = self.u32()? as usize;
        self.take(n)
    }
    pub(crate) fn roots(&mut self, n: usize) -> Result<Vec<Root>, Error> {
        (0..n).map(|_| self.array()).collect()
    }
    /// The input must be consumed exactly.
    pub(crate) fn finish(self) -> Result<(), Error> {
        if self.pos == self.b.len() {
            Ok(())
        } else {
            Err(Error::Malformed)
        }
    }
}

pub(crate) fn fork_byte(fork: LcFork) -> u8 {
    match fork {
        LcFork::Altair => 0,
        LcFork::Capella => 1,
        LcFork::Deneb => 2,
        LcFork::Electra => 3,
        LcFork::Gloas => 4,
    }
}

pub(crate) fn fork_from_byte(b: u8) -> Result<LcFork, Error> {
    Ok(match b {
        0 => LcFork::Altair,
        1 => LcFork::Capella,
        2 => LcFork::Deneb,
        3 => LcFork::Electra,
        4 => LcFork::Gloas,
        _ => return Err(Error::Malformed),
    })
}

// ---------------------------------------------------------------------------------------------
// SSZ encoders (the inverse of the strict decoders in `types`)
// ---------------------------------------------------------------------------------------------

/// SSZ of a `BeaconBlockHeader`.
pub fn encode_beacon_header(h: &BeaconBlockHeader) -> [u8; BeaconBlockHeader::SSZ_LEN] {
    let mut out = [0u8; BeaconBlockHeader::SSZ_LEN];
    out[0..8].copy_from_slice(&h.slot.to_le_bytes());
    out[8..16].copy_from_slice(&h.proposer_index.to_le_bytes());
    out[16..48].copy_from_slice(&h.parent_root);
    out[48..80].copy_from_slice(&h.state_root);
    out[80..112].copy_from_slice(&h.body_root);
    out
}

/// SSZ of an `ExecutionPayloadHeader`, Capella (`deneb == false`) or Deneb layout.
pub fn encode_payload_header(p: &ExecutionPayloadHeader, deneb: bool) -> Vec<u8> {
    let fixed = if deneb { 584 } else { 568 };
    let mut w = Writer::default();
    w.bytes(&p.parent_hash);
    w.bytes(&p.fee_recipient);
    w.bytes(&p.state_root);
    w.bytes(&p.receipts_root);
    w.bytes(&p.logs_bloom);
    w.bytes(&p.prev_randao);
    w.u64(p.block_number);
    w.u64(p.gas_limit);
    w.u64(p.gas_used);
    w.u64(p.timestamp);
    w.u32(fixed); // offset of extra_data
    w.bytes(&p.base_fee_per_gas);
    w.bytes(&p.block_hash);
    w.bytes(&p.transactions_root);
    w.bytes(&p.withdrawals_root);
    if deneb {
        w.u64(p.blob_gas_used);
        w.u64(p.excess_blob_gas);
    }
    debug_assert_eq!(w.out.len(), fixed as usize);
    w.bytes(&p.extra_data);
    w.out
}

/// SSZ of a `LightClientHeader` in the layout of `fork`. The header must have that layout.
pub fn encode_light_client_header(h: &LightClientHeader, fork: LcFork) -> Vec<u8> {
    let mut w = Writer::default();
    w.bytes(&encode_beacon_header(&h.beacon));
    match &h.execution {
        Execution::None => {}
        Execution::Payload { header, branch } => {
            // Fixed part: beacon, offset of `execution`, `execution_branch`.
            let fixed = BeaconBlockHeader::SSZ_LEN + 4 + 32 * branch.len();
            w.u32(u32::try_from(fixed).expect("small"));
            w.roots(branch);
            w.bytes(&encode_payload_header(header, fork >= LcFork::Deneb));
        }
        Execution::BlockHash { block_hash, branch } => {
            w.bytes(block_hash);
            w.roots(branch);
        }
    }
    w.out
}

/// SSZ of a `SyncAggregate`.
pub fn encode_sync_aggregate(a: &SyncAggregate) -> Vec<u8> {
    let mut bits = vec![0u8; a.bits.len().div_ceil(8)];
    for (i, b) in a.bits.iter().enumerate() {
        if *b {
            bits[i / 8] |= 1 << (i % 8);
        }
    }
    bits.extend_from_slice(&a.signature);
    bits
}

// ---------------------------------------------------------------------------------------------
// Committees
// ---------------------------------------------------------------------------------------------

/// The SSZ leaf of a public key: `hash(pubkey ‖ 16 zero bytes)`.
pub fn pubkey_leaf(pk: &BlsPubkey) -> Root {
    let mut buf = [0u8; 64];
    buf[..48].copy_from_slice(pk);
    sha256(&buf)
}

/// A sync committee as a store keeps it on chain: the root of its key leaves and its aggregate
/// key. [`CommitteeSummary::root`] equals `SyncCommittee::hash_tree_root`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommitteeSummary {
    /// `hash_tree_root(pubkeys)`: the 512 (or 32) key leaves, merkleized.
    pub pubkeys_root: Root,
    pub aggregate_pubkey: BlsPubkey,
}

impl CommitteeSummary {
    /// Encoded length.
    pub const ENCODED_LEN: usize = 80;

    /// The summary of a full committee.
    pub fn of(c: &SyncCommittee) -> Self {
        let leaves: Vec<Root> = c.pubkeys.iter().map(pubkey_leaf).collect();
        Self {
            pubkeys_root: merkleize(&leaves, leaves.len().next_power_of_two()),
            aggregate_pubkey: c.aggregate_pubkey,
        }
    }

    /// The summary of `SyncCommittee::empty(n)`, without hashing `n` leaves.
    pub fn empty(n: usize) -> Self {
        let mut node = pubkey_leaf(&[0u8; 48]);
        for _ in 0..floorlog2(n.next_power_of_two() as u64) {
            node = hash_pair(&node, &node);
        }
        Self { pubkeys_root: node, aggregate_pubkey: [0u8; 48] }
    }

    /// `hash_tree_root` of the committee.
    pub fn root(&self) -> Root {
        hash_pair(&self.pubkeys_root, &pubkey_leaf(&self.aggregate_pubkey))
    }

    pub(crate) fn write(&self, w: &mut Writer) {
        w.bytes(&self.pubkeys_root);
        w.bytes(&self.aggregate_pubkey);
    }

    pub(crate) fn read(r: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(Self { pubkeys_root: r.array()?, aggregate_pubkey: r.array()? })
    }

    /// Strict encoding (80 bytes).
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        self.write(&mut w);
        w.out
    }

    /// Strict decoding.
    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(b);
        let s = Self::read(&mut r)?;
        r.finish()?;
        Ok(s)
    }
}

/// `None` for the empty committee (all-zero keys), as the store's "next committee unknown".
fn summary_or_none(c: &SyncCommittee) -> Option<CommitteeSummary> {
    if c.is_empty() {
        None
    } else {
        Some(CommitteeSummary::of(c))
    }
}

// ---------------------------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------------------------

/// The policy store as a program keeps it: everything validation and `apply_core` read, with
/// each committee as its summary. The finalized header's execution part is not part of the
/// store (no check reads it); the program keeps it in the header record instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompactStore {
    pub fork: LcFork,
    /// `finalized_header.beacon`.
    pub finalized: BeaconBlockHeader,
    pub current: CommitteeSummary,
    /// `None` while the next committee is unknown.
    pub next: Option<CommitteeSummary>,
}

impl CompactStore {
    /// The compact form of a store's state.
    pub fn of(core: &StoreCore) -> Self {
        Self {
            fork: core.fork,
            finalized: core.finalized_header.beacon.clone(),
            current: CommitteeSummary::of(&core.current_sync_committee),
            next: summary_or_none(&core.next_sync_committee),
        }
    }

    /// `is_next_sync_committee_known`.
    pub fn is_next_sync_committee_known(&self) -> bool {
        self.next.is_some()
    }

    /// `upgrade_lc_store_to_*` on the compact store, composed up to `to`: only the layout
    /// changes. The upgrades rewrite the execution part of the finalized header, which the
    /// compact store does not hold, and leave its beacon header and both committees as they are
    /// (`ferrata-spectests/tests/steps.rs` compares this with the full store's upgrade on every
    /// fork-transition vector). A downgrade is `ForkMismatch`.
    pub fn upgraded(&self, to: LcFork) -> Result<Self, Error> {
        if to < self.fork {
            return Err(Error::ForkMismatch);
        }
        Ok(Self { fork: to, ..self.clone() })
    }

    /// Strict encoding: fork, finalized beacon header (SSZ), current summary, next flag and
    /// summary.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.u8(fork_byte(self.fork));
        w.bytes(&encode_beacon_header(&self.finalized));
        self.current.write(&mut w);
        w.u8(u8::from(self.next.is_some()));
        if let Some(next) = &self.next {
            next.write(&mut w);
        }
        w.out
    }

    /// Strict decoding.
    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(b);
        let fork = fork_from_byte(r.u8()?)?;
        let finalized = BeaconBlockHeader::decode(r.take(BeaconBlockHeader::SSZ_LEN)?)?;
        let current = CommitteeSummary::read(&mut r)?;
        let next = if r.flag()? { Some(CommitteeSummary::read(&mut r)?) } else { None };
        r.finish()?;
        Ok(Self { fork, finalized, current, next })
    }

    /// `sha256` of the encoding. A job is bound to the store it started from by this digest.
    pub fn digest(&self) -> Root {
        sha256(&self.encode())
    }
}

// ---------------------------------------------------------------------------------------------
// Bootstrap and update with the committee replaced by its summary
// ---------------------------------------------------------------------------------------------

/// A `LightClientBootstrap` whose committee is given by its summary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompactBootstrap {
    pub header: LightClientHeader,
    pub current: CommitteeSummary,
    pub current_sync_committee_branch: Vec<Root>,
}

impl CompactBootstrap {
    pub fn of(b: &LightClientBootstrap) -> Self {
        Self {
            header: b.header.clone(),
            current: CommitteeSummary::of(&b.current_sync_committee),
            current_sync_committee_branch: b.current_sync_committee_branch.clone(),
        }
    }

    /// Strict encoding in the layout of `fork`. The value must have that layout.
    pub fn encode(&self, fork: LcFork) -> Vec<u8> {
        let mut w = Writer::default();
        w.u8(fork_byte(fork));
        w.prefixed(&encode_light_client_header(&self.header, fork));
        self.current.write(&mut w);
        w.roots(&self.current_sync_committee_branch);
        w.out
    }

    /// Strict decoding in the layout of `fork`.
    pub fn decode(b: &[u8], fork: LcFork) -> Result<Self, Error> {
        let mut r = Reader::new(b);
        if fork_from_byte(r.u8()?)? != fork {
            return Err(Error::ForkMismatch);
        }
        let header = LightClientHeader::decode(r.prefixed()?, fork)?;
        let current = CommitteeSummary::read(&mut r)?;
        let current_sync_committee_branch = r.roots(fork.sync_committee_branch_len())?;
        r.finish()?;
        Ok(Self { header, current, current_sync_committee_branch })
    }
}

/// A `LightClientUpdate` whose next committee is given by its summary (`None`: the empty
/// committee, as in a finality update).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompactUpdate {
    pub attested_header: LightClientHeader,
    pub next_sync_committee: Option<CommitteeSummary>,
    pub next_sync_committee_branch: Vec<Root>,
    pub finalized_header: LightClientHeader,
    pub finality_branch: Vec<Root>,
    pub sync_aggregate: SyncAggregate,
    pub signature_slot: u64,
}

impl CompactUpdate {
    pub fn of(u: &LightClientUpdate) -> Self {
        Self {
            attested_header: u.attested_header.clone(),
            next_sync_committee: summary_or_none(&u.next_sync_committee),
            next_sync_committee_branch: u.next_sync_committee_branch.clone(),
            finalized_header: u.finalized_header.clone(),
            finality_branch: u.finality_branch.clone(),
            sync_aggregate: u.sync_aggregate.clone(),
            signature_slot: u.signature_slot,
        }
    }

    /// `is_sync_committee_update`.
    pub fn is_sync_committee_update(&self) -> bool {
        !is_zero_branch(&self.next_sync_committee_branch)
    }

    /// `is_finality_update`.
    pub fn is_finality_update(&self) -> bool {
        !is_zero_branch(&self.finality_branch)
    }

    /// Strict encoding in the layout of `fork`. The value must have that layout (the shape
    /// checks of validation refuse any other).
    pub fn encode(&self, fork: LcFork) -> Vec<u8> {
        let mut w = Writer::default();
        w.u8(fork_byte(fork));
        w.prefixed(&encode_light_client_header(&self.attested_header, fork));
        w.u8(u8::from(self.next_sync_committee.is_some()));
        if let Some(s) = &self.next_sync_committee {
            s.write(&mut w);
        }
        w.roots(&self.next_sync_committee_branch);
        w.prefixed(&encode_light_client_header(&self.finalized_header, fork));
        w.roots(&self.finality_branch);
        w.bytes(&encode_sync_aggregate(&self.sync_aggregate));
        w.u64(self.signature_slot);
        w.out
    }

    /// The layout an encoding names (its first byte), which is the layout to decode it in.
    pub fn layout_of(b: &[u8]) -> Result<LcFork, Error> {
        fork_from_byte(*b.first().ok_or(Error::Malformed)?)
    }

    /// Strict decoding in the layout of `fork`, for a committee of `n`.
    pub fn decode(b: &[u8], fork: LcFork, n: usize) -> Result<Self, Error> {
        let mut r = Reader::new(b);
        if fork_from_byte(r.u8()?)? != fork {
            return Err(Error::ForkMismatch);
        }
        let attested_header = LightClientHeader::decode(r.prefixed()?, fork)?;
        let next_sync_committee = if r.flag()? { Some(CommitteeSummary::read(&mut r)?) } else { None };
        let next_sync_committee_branch = r.roots(fork.sync_committee_branch_len())?;
        let finalized_header = LightClientHeader::decode(r.prefixed()?, fork)?;
        let finality_branch = r.roots(fork.finality_branch_len())?;
        let sync_aggregate = SyncAggregate::decode(r.take(SyncAggregate::ssz_len(n))?, n)?;
        let signature_slot = r.u64()?;
        r.finish()?;
        Ok(Self {
            attested_header,
            next_sync_committee,
            next_sync_committee_branch,
            finalized_header,
            finality_branch,
            sync_aggregate,
            signature_slot,
        })
    }

    /// `sha256` of the encoding in the layout of `fork`.
    pub fn digest(&self, fork: LcFork) -> Root {
        sha256(&self.encode(fork))
    }
}

// ---------------------------------------------------------------------------------------------
// Witnesses
// ---------------------------------------------------------------------------------------------

/// The non-signers of one update: for each member whose bit is clear, in index order, its key
/// uncompressed and its Merkle branch (depth `log2(n)`) to the committee's key root.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SignerWitness {
    pub keys: Vec<UncompressedPubkey>,
    pub branches: Vec<Vec<Root>>,
}

impl SignerWitness {
    /// Strict encoding: a `u16` count, then each key and its branch.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.u16(u16::try_from(self.keys.len()).expect("a committee has fewer than 2^16 members"));
        for (key, branch) in self.keys.iter().zip(&self.branches) {
            w.bytes(key);
            w.roots(branch);
        }
        w.out
    }

    /// Strict decoding for a committee of `n` (branches of depth `log2(n)`).
    pub fn decode(b: &[u8], n: usize) -> Result<Self, Error> {
        let depth = floorlog2(n.next_power_of_two() as u64);
        let mut r = Reader::new(b);
        let count = r.u16()? as usize;
        if count > n {
            return Err(Error::Malformed);
        }
        let mut out = Self { keys: Vec::with_capacity(count), branches: Vec::with_capacity(count) };
        for _ in 0..count {
            out.keys.push(r.array()?);
            out.branches.push(r.roots(depth)?);
        }
        r.finish()?;
        Ok(out)
    }
}

/// Progress of a committee adoption that arrives in parts (one transaction each).
///
/// The committee's `n` keys are split into `parts` equal, consecutive chunks. Each part checks its
/// keys (canonical uncompressed encoding, on the curve, not the identity), hashes their
/// re-compressed bytes into the chunk's subtree root, and adds them to a running sum. The last
/// part checks that the chunk roots merkleize to the committed key root and that the sum
/// compresses to the committed aggregate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdoptionProgress {
    pub committee: CommitteeSummary,
    pub parts: u8,
    /// Subtree roots of the parts checked so far, in order.
    pub chunk_roots: Vec<Root>,
    /// Sum of the keys checked so far, as an uncompressed point (`0x40 ‖ 0…` is the identity).
    pub sum: UncompressedPubkey,
    /// True once the last part's final checks passed.
    pub complete: bool,
}

/// The uncompressed encoding of the identity point.
pub const IDENTITY_UNCOMPRESSED: UncompressedPubkey = {
    let mut b = [0u8; 96];
    b[0] = 0x40;
    b
};

impl AdoptionProgress {
    /// Adoption of `committee` in `parts` parts (1, 2, 4, …; it must divide the committee size).
    pub fn new(committee: CommitteeSummary, parts: u8) -> Self {
        Self { committee, parts, chunk_roots: Vec::new(), sum: IDENTITY_UNCOMPRESSED, complete: false }
    }

    /// The index of the next part to send.
    pub fn next_part(&self) -> u8 {
        self.chunk_roots.len() as u8
    }

    pub(crate) fn write(&self, w: &mut Writer) {
        self.committee.write(w);
        w.u8(self.parts);
        w.u8(self.chunk_roots.len() as u8);
        w.roots(&self.chunk_roots);
        w.bytes(&self.sum);
        w.u8(u8::from(self.complete));
    }

    pub(crate) fn read(r: &mut Reader<'_>) -> Result<Self, Error> {
        let committee = CommitteeSummary::read(r)?;
        let parts = r.u8()?;
        let done = r.u8()?;
        if parts == 0 || !parts.is_power_of_two() || done > parts {
            return Err(Error::Malformed);
        }
        let chunk_roots = r.roots(done as usize)?;
        let sum = r.array()?;
        let complete = r.flag()?;
        if complete && done != parts {
            return Err(Error::Malformed);
        }
        Ok(Self { committee, parts, chunk_roots, sum, complete })
    }

    /// Strict encoding.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        self.write(&mut w);
        w.out
    }

    /// Strict decoding.
    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(b);
        let p = Self::read(&mut r)?;
        r.finish()?;
        Ok(p)
    }
}

// ---------------------------------------------------------------------------------------------
// Header record
// ---------------------------------------------------------------------------------------------

/// What a store records about a finalized header: the beacon header, the execution root it binds
/// (`get_lc_execution_root`) and the execution fields a consumer proves against. The LEZ program
/// writes one, once, per finalized header (`header(id, slot)`); an attestation restores its
/// execution view with [`crate::attest::HeaderRecord::from_recorded`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeaderRecord {
    pub beacon: BeaconBlockHeader,
    pub execution_root: Root,
    pub execution: ExecutionSummary,
}

/// The execution part of a [`HeaderRecord`], by header layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecutionSummary {
    /// Altair layout: no execution data.
    None,
    /// Capella to Electra layouts: fields of the execution payload header.
    Payload {
        block_number: u64,
        block_hash: Root,
        parent_hash: Root,
        state_root: Root,
        receipts_root: Root,
        timestamp: u64,
    },
    /// Gloas layout: the bound block hash (the bid's parent block hash).
    BlockHash(Root),
}

impl HeaderRecord {
    /// The record of a header in the layout of `fork`.
    pub fn of(header: &LightClientHeader, fork: LcFork, spec: &ChainSpec) -> Self {
        let execution = match &header.execution {
            Execution::None => ExecutionSummary::None,
            Execution::Payload { header: p, .. } => ExecutionSummary::Payload {
                block_number: p.block_number,
                block_hash: p.block_hash,
                parent_hash: p.parent_hash,
                state_root: p.state_root,
                receipts_root: p.receipts_root,
                timestamp: p.timestamp,
            },
            Execution::BlockHash { block_hash, .. } => ExecutionSummary::BlockHash(*block_hash),
        };
        Self { beacon: header.beacon.clone(), execution_root: get_lc_execution_root(header, fork, spec), execution }
    }

    pub(crate) fn write(&self, w: &mut Writer) {
        w.bytes(&encode_beacon_header(&self.beacon));
        w.bytes(&self.execution_root);
        match &self.execution {
            ExecutionSummary::None => w.u8(0),
            ExecutionSummary::Payload {
                block_number,
                block_hash,
                parent_hash,
                state_root,
                receipts_root,
                timestamp,
            } => {
                w.u8(1);
                w.u64(*block_number);
                w.bytes(block_hash);
                w.bytes(parent_hash);
                w.bytes(state_root);
                w.bytes(receipts_root);
                w.u64(*timestamp);
            }
            ExecutionSummary::BlockHash(h) => {
                w.u8(2);
                w.bytes(h);
            }
        }
    }

    pub(crate) fn read(r: &mut Reader<'_>) -> Result<Self, Error> {
        let beacon = BeaconBlockHeader::decode(r.take(BeaconBlockHeader::SSZ_LEN)?)?;
        let execution_root = r.array()?;
        let execution = match r.u8()? {
            0 => ExecutionSummary::None,
            1 => ExecutionSummary::Payload {
                block_number: r.u64()?,
                block_hash: r.array()?,
                parent_hash: r.array()?,
                state_root: r.array()?,
                receipts_root: r.array()?,
                timestamp: r.u64()?,
            },
            2 => ExecutionSummary::BlockHash(r.array()?),
            _ => return Err(Error::Malformed),
        };
        Ok(Self { beacon, execution_root, execution })
    }

    /// Strict encoding: the beacon header (SSZ, 112 bytes), the execution root, a layout byte
    /// (0 none, 1 payload, 2 block hash), then for a payload the block number, the block hash,
    /// the parent hash, the state root, the receipts root and the timestamp (integers
    /// little-endian), or for a block hash its 32 bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        self.write(&mut w);
        w.out
    }

    /// Strict decoding.
    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        let mut r = Reader::new(b);
        let out = Self::read(&mut r)?;
        r.finish()?;
        Ok(out)
    }
}

#[cfg(feature = "blst")]
pub use self::blst_witness::*;

#[cfg(feature = "blst")]
mod blst_witness {
    #![allow(unsafe_code)]

    use alloc::vec::Vec;

    use blst::{
        blst_p1, blst_p1_add_or_double_affine, blst_p1_affine, blst_p1_affine_compress, blst_p1_affine_is_inf,
        blst_p1_affine_serialize, blst_p1_compress, blst_p1_deserialize, blst_p1_from_affine, blst_p1_to_affine,
        blst_p1_uncompress, BLST_ERROR,
    };

    use super::{pubkey_leaf, AdoptionProgress, CommitteeSummary, SignerWitness, UncompressedPubkey};
    use crate::{
        merkle::{floorlog2, merkle_proof, merkleize},
        types::{BlsPubkey, SyncCommittee},
        Error, Root,
    };

    /// Decode a witness key: the canonical uncompressed encoding (compression and sign flags
    /// clear, coordinates below the modulus) of a curve point. blst refuses the x = 0 points
    /// (order 3). The identity (`0x40 ‖ 0…`) decodes to the identity; callers refuse it.
    pub(crate) fn deserialize_g1(bytes: &UncompressedPubkey) -> Result<blst_p1_affine, Error> {
        // blst would read a compressed encoding from the first 48 bytes and ignore the rest.
        if bytes[0] & 0x80 != 0 {
            return Err(Error::Malformed);
        }
        let mut out = blst_p1_affine::default();
        // SAFETY: `out` is a valid destination and `bytes` holds the 96 bytes blst reads.
        if unsafe { blst_p1_deserialize(&mut out, bytes.as_ptr()) } != BLST_ERROR::BLST_SUCCESS {
            return Err(Error::PubkeyInvalid);
        }
        Ok(out)
    }

    /// A witness key that is a curve point and not the identity, with its compressed bytes.
    pub(crate) fn witness_key(bytes: &UncompressedPubkey) -> Result<(blst_p1_affine, BlsPubkey), Error> {
        let point = deserialize_g1(bytes)?;
        // SAFETY: `point` is a valid affine point.
        if unsafe { blst_p1_affine_is_inf(&point) } {
            return Err(Error::PubkeyInvalid);
        }
        let mut compressed = [0u8; 48];
        // SAFETY: `compressed` has the 48 bytes blst writes.
        unsafe { blst_p1_affine_compress(compressed.as_mut_ptr(), &point) };
        Ok((point, compressed))
    }

    pub(crate) fn add_affine(acc: &mut blst_p1, point: &blst_p1_affine) {
        let acc_ptr: *mut blst_p1 = acc;
        // SAFETY: blst allows the output to alias the first input.
        unsafe { blst_p1_add_or_double_affine(acc_ptr, acc_ptr, point) };
    }

    pub(crate) fn serialize_g1(p: &blst_p1) -> UncompressedPubkey {
        let mut affine = blst_p1_affine::default();
        let mut out = [0u8; 96];
        // SAFETY: valid source and destinations.
        unsafe {
            blst_p1_to_affine(&mut affine, p);
            blst_p1_affine_serialize(out.as_mut_ptr(), &affine);
        }
        out
    }

    /// The uncompressed form of every key of a committee, for an adoption witness (host side).
    /// Fails if a key is not a valid compressed curve point.
    pub fn uncompressed_keys(committee: &SyncCommittee) -> Result<Vec<UncompressedPubkey>, Error> {
        committee.pubkeys.iter().map(uncompress_key).collect()
    }

    /// Decompress a key and serialize it uncompressed (host side).
    pub fn uncompress_key(pk: &BlsPubkey) -> Result<UncompressedPubkey, Error> {
        let mut point = blst_p1_affine::default();
        // SAFETY: valid destination; `pk` holds the 48 bytes blst reads.
        if unsafe { blst_p1_uncompress(&mut point, pk.as_ptr()) } != BLST_ERROR::BLST_SUCCESS {
            return Err(Error::PubkeyInvalid);
        }
        let mut out = [0u8; 96];
        // SAFETY: `out` has the 96 bytes blst writes.
        unsafe { blst_p1_affine_serialize(out.as_mut_ptr(), &point) };
        Ok(out)
    }

    impl SignerWitness {
        /// The witness for the members of `committee` whose bit in `bits` is clear (host side).
        pub fn build(committee: &SyncCommittee, bits: &[bool]) -> Result<Self, Error> {
            let n = committee.pubkeys.len();
            if bits.len() != n {
                return Err(Error::Malformed);
            }
            let leaves: Vec<Root> = committee.pubkeys.iter().map(pubkey_leaf).collect();
            let width = n.next_power_of_two();
            let mut out = Self::default();
            for (i, _) in bits.iter().enumerate().filter(|(_, b)| !**b) {
                out.keys.push(uncompress_key(&committee.pubkeys[i])?);
                out.branches.push(merkle_proof(&leaves, width, i));
            }
            Ok(out)
        }
    }

    impl AdoptionProgress {
        /// Check part `index` (it must be the next one) of a committee of `n`: `n / parts`
        /// uncompressed keys. After the last part, the chunk roots must merkleize to the
        /// committed key root and the keys must sum to the committed aggregate.
        ///
        /// Errors: [`Error::Malformed`] for a part out of order, a wrong key count, an adoption
        /// already complete or a non-canonical encoding; [`Error::CommitteeInvalid`] for a key
        /// that is not a curve point or is the identity, and for a key root or aggregate that
        /// does not match. On error the progress is unchanged.
        pub fn add_part(&mut self, index: u8, keys: &[UncompressedPubkey], n: usize) -> Result<(), Error> {
            let parts = self.parts as usize;
            if n == 0 {
                // As `check_committee_onchain`: a committee without keys is invalid.
                return Err(Error::CommitteeInvalid);
            }
            if self.complete
                || index != self.next_part()
                || !n.is_power_of_two()
                || parts == 0
                || n % parts != 0
                || keys.len() != n / parts
            {
                return Err(Error::Malformed);
            }
            let mut sum = blst_p1::default();
            let resumed = deserialize_g1(&self.sum).map_err(|_| Error::Malformed)?;
            // SAFETY: valid destination and source.
            unsafe { blst_p1_from_affine(&mut sum, &resumed) };
            let mut leaves = Vec::with_capacity(keys.len());
            for key in keys {
                let (point, compressed) = witness_key(key).map_err(|e| match e {
                    Error::Malformed => Error::Malformed,
                    _ => Error::CommitteeInvalid,
                })?;
                leaves.push(pubkey_leaf(&compressed));
                add_affine(&mut sum, &point);
            }
            let chunk_root = merkleize(&leaves, keys.len());
            let mut chunk_roots = self.chunk_roots.clone();
            chunk_roots.push(chunk_root);
            let complete = chunk_roots.len() == parts;
            if complete {
                if merkleize(&chunk_roots, parts) != self.committee.pubkeys_root {
                    return Err(Error::CommitteeInvalid);
                }
                let mut compressed = [0u8; 48];
                // SAFETY: `compressed` has the 48 bytes blst writes; `sum` is a valid point.
                unsafe { blst_p1_compress(compressed.as_mut_ptr(), &sum) };
                if compressed != self.committee.aggregate_pubkey {
                    return Err(Error::CommitteeInvalid);
                }
            }
            self.sum = serialize_g1(&sum);
            self.chunk_roots = chunk_roots;
            self.complete = complete;
            Ok(())
        }
    }

    /// Adopt a whole committee from its uncompressed keys in one call: the check
    /// `check_committee_onchain` makes, from an uncompressed-key witness.
    pub fn adopt_committee(committee: &CommitteeSummary, keys: &[UncompressedPubkey]) -> Result<(), Error> {
        let mut progress = AdoptionProgress::new(*committee, 1);
        progress.add_part(0, keys, keys.len())
    }

    /// The depth of a key's branch in a committee of `n`.
    pub fn key_branch_depth(n: usize) -> usize {
        floorlog2(n.next_power_of_two() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_summary_matches_the_empty_committee() {
        for n in [32usize, 512] {
            let empty = SyncCommittee::empty(n);
            assert_eq!(CommitteeSummary::empty(n), CommitteeSummary::of(&empty));
            assert_eq!(CommitteeSummary::empty(n).root(), empty.hash_tree_root());
        }
    }

    #[test]
    fn reader_is_strict() {
        let mut r = Reader::new(&[2]);
        assert_eq!(r.flag(), Err(Error::Malformed));
        let r = Reader::new(&[0, 0]);
        assert_eq!(r.finish(), Err(Error::Malformed));
        let mut r = Reader::new(&[0xff, 0xff, 0xff, 0xff]);
        assert_eq!(r.prefixed().err(), Some(Error::Malformed));
        assert_eq!(CommitteeSummary::decode(&[0u8; 79]), Err(Error::Malformed));
        assert_eq!(CommitteeSummary::decode(&[0u8; 81]), Err(Error::Malformed));
    }

    #[test]
    fn store_encoding_round_trips_and_is_strict() {
        let store = CompactStore {
            fork: LcFork::Electra,
            finalized: BeaconBlockHeader { slot: 7, proposer_index: 3, ..Default::default() },
            current: CommitteeSummary { pubkeys_root: [1; 32], aggregate_pubkey: [2; 48] },
            next: Some(CommitteeSummary { pubkeys_root: [3; 32], aggregate_pubkey: [4; 48] }),
        };
        let bytes = store.encode();
        assert_eq!(CompactStore::decode(&bytes), Ok(store.clone()));
        let without_next = CompactStore { next: None, ..store.clone() };
        assert_eq!(CompactStore::decode(&without_next.encode()), Ok(without_next.clone()));
        assert_ne!(without_next.digest(), store.digest());
        // Trailing byte, truncated, bad flag, bad fork.
        let mut long = bytes.clone();
        long.push(0);
        assert_eq!(CompactStore::decode(&long), Err(Error::Malformed));
        assert_eq!(CompactStore::decode(&bytes[..bytes.len() - 1]), Err(Error::Malformed));
        let mut flag = bytes.clone();
        flag[1 + 112 + 80] = 2;
        assert_eq!(CompactStore::decode(&flag), Err(Error::Malformed));
        let mut fork = bytes;
        fork[0] = 9;
        assert_eq!(CompactStore::decode(&fork), Err(Error::Malformed));
    }
}
