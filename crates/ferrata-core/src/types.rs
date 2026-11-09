//! Light-client containers, their SSZ decoding and the hash-tree roots the protocol needs.
//!
//! One Rust type covers every fork's layout. Which fork a value belongs to is carried by the
//! store or decoder that holds it (`LcFork`), exactly as in the spec, where a store's fork
//! decides how its headers are validated.

use alloc::{vec, vec::Vec};

use crate::{
    fork::LcFork,
    hash::{hash_pair, sha256},
    merkle::{
        floorlog2, is_zero_branch, merkle_proof, merkleize, CURRENT_SYNC_COMMITTEE_GINDEX,
        CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA, CURRENT_SYNC_COMMITTEE_GINDEX_GLOAS, EXECUTION_BLOCK_HASH_GINDEX_GLOAS,
        EXECUTION_PAYLOAD_GINDEX, FINALIZED_ROOT_GINDEX, FINALIZED_ROOT_GINDEX_ELECTRA, FINALIZED_ROOT_GINDEX_GLOAS,
        ZERO,
    },
    ssz::{decode_branch, root_at, split_container, u64_at, Field},
    Error, Root,
};

/// Compressed BLS12-381 G1 public key.
pub type BlsPubkey = [u8; 48];
/// Compressed BLS12-381 G2 signature.
pub type BlsSignature = [u8; 96];

/// `MAX_EXTRA_DATA_BYTES`.
pub const MAX_EXTRA_DATA_BYTES: usize = 32;

fn u64_chunk(v: u64) -> Root {
    let mut c = ZERO;
    c[..8].copy_from_slice(&v.to_le_bytes());
    c
}

impl LcFork {
    /// Length of `CurrentSyncCommitteeBranch` / `NextSyncCommitteeBranch`.
    pub const fn sync_committee_branch_len(self) -> usize {
        match self {
            LcFork::Altair | LcFork::Capella | LcFork::Deneb => floorlog2(CURRENT_SYNC_COMMITTEE_GINDEX),
            LcFork::Electra => floorlog2(CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA),
            LcFork::Gloas => floorlog2(CURRENT_SYNC_COMMITTEE_GINDEX_GLOAS),
        }
    }

    /// Length of `FinalityBranch`.
    pub const fn finality_branch_len(self) -> usize {
        match self {
            LcFork::Altair | LcFork::Capella | LcFork::Deneb => floorlog2(FINALIZED_ROOT_GINDEX),
            LcFork::Electra => floorlog2(FINALIZED_ROOT_GINDEX_ELECTRA),
            LcFork::Gloas => floorlog2(FINALIZED_ROOT_GINDEX_GLOAS),
        }
    }

    /// Length of `ExecutionBranch` (0 before Capella).
    pub const fn execution_branch_len(self) -> usize {
        match self {
            LcFork::Altair => 0,
            LcFork::Capella | LcFork::Deneb | LcFork::Electra => floorlog2(EXECUTION_PAYLOAD_GINDEX),
            LcFork::Gloas => floorlog2(EXECUTION_BLOCK_HASH_GINDEX_GLOAS),
        }
    }

    fn header_field(self) -> Field {
        match self {
            LcFork::Altair => Field::Fixed(BeaconBlockHeader::SSZ_LEN),
            LcFork::Capella | LcFork::Deneb | LcFork::Electra => Field::Variable,
            LcFork::Gloas => Field::Fixed(BeaconBlockHeader::SSZ_LEN + 32 + 32 * self.execution_branch_len()),
        }
    }
}

/// `BeaconBlockHeader`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BeaconBlockHeader {
    pub slot: u64,
    pub proposer_index: u64,
    pub parent_root: Root,
    pub state_root: Root,
    pub body_root: Root,
}

impl BeaconBlockHeader {
    /// Encoded length.
    pub const SSZ_LEN: usize = 112;

    /// Decode from SSZ.
    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        if b.len() != Self::SSZ_LEN {
            return Err(Error::Malformed);
        }
        Ok(Self {
            slot: u64_at(b, 0),
            proposer_index: u64_at(b, 8),
            parent_root: root_at(b, 16),
            state_root: root_at(b, 48),
            body_root: root_at(b, 80),
        })
    }

    /// `hash_tree_root`.
    pub fn hash_tree_root(&self) -> Root {
        merkleize(
            &[u64_chunk(self.slot), u64_chunk(self.proposer_index), self.parent_root, self.state_root, self.body_root],
            8,
        )
    }

    fn is_zero(&self) -> bool {
        *self == Self::default()
    }
}

/// `ExecutionPayloadHeader`, Capella and Deneb layouts. Capella-typed values have zero blob-gas
/// fields, which is also what the spec's Capella-to-Deneb upgrade writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionPayloadHeader {
    pub parent_hash: Root,
    pub fee_recipient: [u8; 20],
    pub state_root: Root,
    pub receipts_root: Root,
    pub logs_bloom: [u8; 256],
    pub prev_randao: Root,
    pub block_number: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub timestamp: u64,
    pub extra_data: Vec<u8>,
    /// `uint256`, little-endian.
    pub base_fee_per_gas: [u8; 32],
    pub block_hash: Root,
    pub transactions_root: Root,
    pub withdrawals_root: Root,
    /// Deneb onward.
    pub blob_gas_used: u64,
    /// Deneb onward.
    pub excess_blob_gas: u64,
}

impl ExecutionPayloadHeader {
    /// `ExecutionPayloadHeader.empty()`.
    pub fn empty() -> Self {
        Self {
            parent_hash: ZERO,
            fee_recipient: [0; 20],
            state_root: ZERO,
            receipts_root: ZERO,
            logs_bloom: [0; 256],
            prev_randao: ZERO,
            block_number: 0,
            gas_limit: 0,
            gas_used: 0,
            timestamp: 0,
            extra_data: Vec::new(),
            base_fee_per_gas: [0; 32],
            block_hash: ZERO,
            transactions_root: ZERO,
            withdrawals_root: ZERO,
            blob_gas_used: 0,
            excess_blob_gas: 0,
        }
    }

    /// True for the all-zero header.
    pub fn is_empty(&self) -> bool {
        *self == Self::empty()
    }

    /// Decode from SSZ. `deneb` selects the 17-field layout.
    pub fn decode(b: &[u8], deneb: bool) -> Result<Self, Error> {
        let mut fields = vec![
            Field::Fixed(32),  // parent_hash
            Field::Fixed(20),  // fee_recipient
            Field::Fixed(32),  // state_root
            Field::Fixed(32),  // receipts_root
            Field::Fixed(256), // logs_bloom
            Field::Fixed(32),  // prev_randao
            Field::Fixed(8),   // block_number
            Field::Fixed(8),   // gas_limit
            Field::Fixed(8),   // gas_used
            Field::Fixed(8),   // timestamp
            Field::Variable,   // extra_data
            Field::Fixed(32),  // base_fee_per_gas
            Field::Fixed(32),  // block_hash
            Field::Fixed(32),  // transactions_root
            Field::Fixed(32),  // withdrawals_root
        ];
        if deneb {
            fields.push(Field::Fixed(8)); // blob_gas_used
            fields.push(Field::Fixed(8)); // excess_blob_gas
        }
        let p = split_container(b, &fields)?;
        if p[10].len() > MAX_EXTRA_DATA_BYTES {
            return Err(Error::Malformed);
        }
        let mut fee_recipient = [0u8; 20];
        fee_recipient.copy_from_slice(p[1]);
        let mut logs_bloom = [0u8; 256];
        logs_bloom.copy_from_slice(p[4]);
        Ok(Self {
            parent_hash: root_at(p[0], 0),
            fee_recipient,
            state_root: root_at(p[2], 0),
            receipts_root: root_at(p[3], 0),
            logs_bloom,
            prev_randao: root_at(p[5], 0),
            block_number: u64_at(p[6], 0),
            gas_limit: u64_at(p[7], 0),
            gas_used: u64_at(p[8], 0),
            timestamp: u64_at(p[9], 0),
            extra_data: p[10].to_vec(),
            base_fee_per_gas: root_at(p[11], 0),
            block_hash: root_at(p[12], 0),
            transactions_root: root_at(p[13], 0),
            withdrawals_root: root_at(p[14], 0),
            blob_gas_used: if deneb { u64_at(p[15], 0) } else { 0 },
            excess_blob_gas: if deneb { u64_at(p[16], 0) } else { 0 },
        })
    }

    fn field_roots(&self, deneb: bool) -> Vec<Root> {
        let mut fee = ZERO;
        fee[..20].copy_from_slice(&self.fee_recipient);
        let bloom: Vec<Root> = self.logs_bloom.chunks_exact(32).map(|c| root_at(c, 0)).collect();
        let mut extra = ZERO;
        extra[..self.extra_data.len()].copy_from_slice(&self.extra_data);
        // ByteList[32]: one chunk, length mixed in.
        let extra_root = hash_pair(&extra, &u64_chunk(self.extra_data.len() as u64));
        let mut roots = vec![
            self.parent_hash,
            fee,
            self.state_root,
            self.receipts_root,
            merkleize(&bloom, 8),
            self.prev_randao,
            u64_chunk(self.block_number),
            u64_chunk(self.gas_limit),
            u64_chunk(self.gas_used),
            u64_chunk(self.timestamp),
            extra_root,
            self.base_fee_per_gas,
            self.block_hash,
            self.transactions_root,
            self.withdrawals_root,
        ];
        if deneb {
            roots.push(u64_chunk(self.blob_gas_used));
            roots.push(u64_chunk(self.excess_blob_gas));
        }
        roots
    }

    /// `hash_tree_root` as a `capella.ExecutionPayloadHeader` (blob-gas fields dropped).
    pub fn hash_tree_root_capella(&self) -> Root {
        merkleize(&self.field_roots(false), 16)
    }

    /// `hash_tree_root` as a `deneb.ExecutionPayloadHeader`.
    pub fn hash_tree_root_deneb(&self) -> Root {
        merkleize(&self.field_roots(true), 32)
    }

    /// Merkle proof of `block_hash` within this header (Capella or Deneb layout).
    pub fn block_hash_proof(&self, deneb: bool) -> Vec<Root> {
        let width = if deneb { 32 } else { 16 };
        merkle_proof(&self.field_roots(deneb), width, 12)
    }
}

/// `SyncCommittee`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncCommittee {
    pub pubkeys: Vec<BlsPubkey>,
    pub aggregate_pubkey: BlsPubkey,
}

fn pubkey_root(pk: &BlsPubkey) -> Root {
    let mut buf = [0u8; 64];
    buf[..48].copy_from_slice(pk);
    sha256(&buf)
}

impl SyncCommittee {
    /// Encoded length for a committee of `n`.
    pub const fn ssz_len(n: usize) -> usize {
        (n + 1) * 48
    }

    /// `SyncCommittee.empty()` for a committee of `n`.
    pub fn empty(n: usize) -> Self {
        Self { pubkeys: vec![[0u8; 48]; n], aggregate_pubkey: [0u8; 48] }
    }

    /// True for the all-zero committee.
    pub fn is_empty(&self) -> bool {
        self.aggregate_pubkey == [0u8; 48] && self.pubkeys.iter().all(|pk| *pk == [0u8; 48])
    }

    /// Decode a committee of `n` from SSZ.
    pub fn decode(b: &[u8], n: usize) -> Result<Self, Error> {
        if b.len() != Self::ssz_len(n) {
            return Err(Error::Malformed);
        }
        let mut keys = b.chunks_exact(48).map(|c| {
            let mut pk = [0u8; 48];
            pk.copy_from_slice(c);
            pk
        });
        let pubkeys: Vec<BlsPubkey> = keys.by_ref().take(n).collect();
        let aggregate_pubkey = keys.next().ok_or(Error::Malformed)?;
        Ok(Self { pubkeys, aggregate_pubkey })
    }

    /// `hash_tree_root`.
    pub fn hash_tree_root(&self) -> Root {
        let leaves: Vec<Root> = self.pubkeys.iter().map(pubkey_root).collect();
        let pubkeys_root = merkleize(&leaves, leaves.len().next_power_of_two());
        hash_pair(&pubkeys_root, &pubkey_root(&self.aggregate_pubkey))
    }
}

/// `SyncAggregate`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncAggregate {
    pub bits: Vec<bool>,
    pub signature: BlsSignature,
}

impl SyncAggregate {
    /// Encoded length for a committee of `n`.
    pub const fn ssz_len(n: usize) -> usize {
        n.div_ceil(8) + 96
    }

    /// Decode from SSZ for a committee of `n`.
    pub fn decode(b: &[u8], n: usize) -> Result<Self, Error> {
        let nbytes = n.div_ceil(8);
        if b.len() != nbytes + 96 {
            return Err(Error::Malformed);
        }
        let bits: Vec<bool> = (0..n).map(|i| (b[i / 8] >> (i % 8)) & 1 == 1).collect();
        // Bits beyond `n` in the last byte must be zero.
        if n % 8 != 0 && b[nbytes - 1] >> (n % 8) != 0 {
            return Err(Error::Malformed);
        }
        let mut signature = [0u8; 96];
        signature.copy_from_slice(&b[nbytes..]);
        Ok(Self { bits, signature })
    }

    /// `get_set_bit_count(sync_committee_bits)`.
    pub fn participants(&self) -> u64 {
        self.bits.iter().filter(|b| **b).count() as u64
    }
}

/// The execution part of a `LightClientHeader`, by layout.
// The payload variant is large, but headers are few and short-lived; boxing would only add an
// allocation per header in the guest.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Execution {
    /// Altair layout: no execution data.
    None,
    /// Capella to Electra layouts: the execution payload header and its branch to `body_root`.
    Payload { header: ExecutionPayloadHeader, branch: Vec<Root> },
    /// Gloas layout: the bound block hash and its (normalized) branch to `body_root`.
    BlockHash { block_hash: Root, branch: Vec<Root> },
}

/// `LightClientHeader`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LightClientHeader {
    pub beacon: BeaconBlockHeader,
    pub execution: Execution,
}

impl LightClientHeader {
    /// `LightClientHeader.empty()` in the layout of `fork`.
    pub fn empty(fork: LcFork) -> Self {
        let execution = match fork {
            LcFork::Altair => Execution::None,
            LcFork::Capella | LcFork::Deneb | LcFork::Electra => Execution::Payload {
                header: ExecutionPayloadHeader::empty(),
                branch: vec![ZERO; fork.execution_branch_len()],
            },
            LcFork::Gloas => Execution::BlockHash { block_hash: ZERO, branch: vec![ZERO; fork.execution_branch_len()] },
        };
        Self { beacon: BeaconBlockHeader::default(), execution }
    }

    /// True if this equals `LightClientHeader.empty()` of its layout.
    pub fn is_empty(&self) -> bool {
        self.beacon.is_zero()
            && match &self.execution {
                Execution::None => true,
                Execution::Payload { header, branch } => header.is_empty() && is_zero_branch(branch),
                Execution::BlockHash { block_hash, branch } => *block_hash == ZERO && is_zero_branch(branch),
            }
    }

    /// True if the value has the shape of `fork`'s layout.
    pub fn has_layout(&self, fork: LcFork) -> bool {
        match (&self.execution, fork) {
            (Execution::None, LcFork::Altair) => true,
            (Execution::Payload { branch, .. }, LcFork::Capella | LcFork::Deneb | LcFork::Electra)
            | (Execution::BlockHash { branch, .. }, LcFork::Gloas) => branch.len() == fork.execution_branch_len(),
            _ => false,
        }
    }

    /// Decode from SSZ in the layout of `fork`.
    pub fn decode(b: &[u8], fork: LcFork) -> Result<Self, Error> {
        match fork {
            LcFork::Altair => Ok(Self { beacon: BeaconBlockHeader::decode(b)?, execution: Execution::None }),
            LcFork::Capella | LcFork::Deneb | LcFork::Electra => {
                let p = split_container(
                    b,
                    &[
                        Field::Fixed(BeaconBlockHeader::SSZ_LEN),
                        Field::Variable,
                        Field::Fixed(32 * fork.execution_branch_len()),
                    ],
                )?;
                Ok(Self {
                    beacon: BeaconBlockHeader::decode(p[0])?,
                    execution: Execution::Payload {
                        header: ExecutionPayloadHeader::decode(p[1], fork >= LcFork::Deneb)?,
                        branch: decode_branch(p[2], fork.execution_branch_len())?,
                    },
                })
            }
            LcFork::Gloas => {
                let p = split_container(
                    b,
                    &[
                        Field::Fixed(BeaconBlockHeader::SSZ_LEN),
                        Field::Fixed(32),
                        Field::Fixed(32 * fork.execution_branch_len()),
                    ],
                )?;
                Ok(Self {
                    beacon: BeaconBlockHeader::decode(p[0])?,
                    execution: Execution::BlockHash {
                        block_hash: root_at(p[1], 0),
                        branch: decode_branch(p[2], fork.execution_branch_len())?,
                    },
                })
            }
        }
    }
}

/// `LightClientBootstrap`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LightClientBootstrap {
    pub header: LightClientHeader,
    pub current_sync_committee: SyncCommittee,
    pub current_sync_committee_branch: Vec<Root>,
}

impl LightClientBootstrap {
    /// Decode from SSZ in the layout of `fork`, for a committee of `n`.
    pub fn decode(b: &[u8], fork: LcFork, n: usize) -> Result<Self, Error> {
        let branch_len = fork.sync_committee_branch_len();
        let p = split_container(
            b,
            &[fork.header_field(), Field::Fixed(SyncCommittee::ssz_len(n)), Field::Fixed(32 * branch_len)],
        )?;
        Ok(Self {
            header: LightClientHeader::decode(p[0], fork)?,
            current_sync_committee: SyncCommittee::decode(p[1], n)?,
            current_sync_committee_branch: decode_branch(p[2], branch_len)?,
        })
    }
}

/// `LightClientUpdate`. Finality and optimistic updates are expressed as updates with empty
/// fields, as the spec's `process_light_client_finality_update` does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LightClientUpdate {
    pub attested_header: LightClientHeader,
    pub next_sync_committee: SyncCommittee,
    pub next_sync_committee_branch: Vec<Root>,
    pub finalized_header: LightClientHeader,
    pub finality_branch: Vec<Root>,
    pub sync_aggregate: SyncAggregate,
    pub signature_slot: u64,
}

impl LightClientUpdate {
    /// Decode from SSZ in the layout of `fork`, for a committee of `n`.
    pub fn decode(b: &[u8], fork: LcFork, n: usize) -> Result<Self, Error> {
        let sc_len = fork.sync_committee_branch_len();
        let fin_len = fork.finality_branch_len();
        let p = split_container(
            b,
            &[
                fork.header_field(),
                Field::Fixed(SyncCommittee::ssz_len(n)),
                Field::Fixed(32 * sc_len),
                fork.header_field(),
                Field::Fixed(32 * fin_len),
                Field::Fixed(SyncAggregate::ssz_len(n)),
                Field::Fixed(8),
            ],
        )?;
        Ok(Self {
            attested_header: LightClientHeader::decode(p[0], fork)?,
            next_sync_committee: SyncCommittee::decode(p[1], n)?,
            next_sync_committee_branch: decode_branch(p[2], sc_len)?,
            finalized_header: LightClientHeader::decode(p[3], fork)?,
            finality_branch: decode_branch(p[4], fin_len)?,
            sync_aggregate: SyncAggregate::decode(p[5], n)?,
            signature_slot: u64_at(p[6], 0),
        })
    }
}
