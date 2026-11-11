//! The guests' inputs and their byte encoding. The host encodes, the guest decodes the same bytes
//! (zero-copy: blocks and transactions stay slices of the input buffer).
//!
//! All integers are little-endian; `vec<T>` is a `u32` count then the items; `bytes` is a `u32`
//! length then the bytes.
//!
//! ```text
//! Config        = bedrock_genesis_id ‖ channel_id ‖ bridge_header ‖ bridge_image_id ‖
//!                 evm_chain_id ‖ vault(20) ‖ attestation_root_id ‖ salt ‖ n_min u32 ‖ t_min u32
//! State         = tip_id u64 ‖ tip_hash ‖ vec<key(32)> ‖ posting_timeframe u32 ‖ config_tip_hash ‖
//!                 burn_count u64 ‖ frontier(32·32) ‖ imported_leaf_count u64 ‖ sunset u8
//! ConfigChange  = bytes(ops) ‖ op_index u8 ‖ vec<(key_index u16 ‖ signature(64))>
//! Inscription   = bytes(prefix) ‖ bytes(suffix) ‖ signature(64)      (see mantle::ElidedInscription)
//! Support       = block_index u32 ‖ Inscription
//! CheckpointInput = Config ‖ State ‖ vec<ConfigChange> ‖ vec<bytes(block)> ‖ t_index u32 ‖
//!                   echo_nonce(32) ‖ Inscription (of T) ‖ vec<Support> ‖ vec<SegmentJournal(125)>
//! Attestation   = channel_key ‖ att_key ‖ certificate(64) ‖ tip_id u64 ‖ tip_hash ‖ final_msg_id ‖
//!                 signature(64)                       (264 bytes, attestor_core::SignedAttestation)
//! CheckpointInputB = Config ‖ State ‖ vec<ConfigChange> ‖ vec<bytes(block)> ‖ t_index u32 ‖
//!                    Inscription (of T) ‖ vec<Attestation> ‖ vec<SegmentJournal(125)>   (rule (B))
//! SegmentInput  = Config ‖ State ‖ vec<bytes(block)>
//! ```

use alloc::vec::Vec;

use attestor_core::{SignedAttestation, SIGNED_ATTESTATION_LEN};

use crate::journal::{SegmentJournal, SEGMENT_JOURNAL_LEN};
use crate::mantle::{ConfigChange, ElidedInscription};
use crate::reader::Reader;
use crate::state::{Config, State};
use crate::tree::{Frontier, DEPTH};
use crate::{Error, Hash};

/// A support entry: the inscription of block `block_index` (position in the input's blocks).
#[derive(Clone, Copy, Debug)]
pub struct Support<'a> {
    pub block_index: u32,
    pub inscription: ElidedInscription<'a>,
}

/// Everything a checkpoint needs (SPEC-v2 §4.2). `blocks` are the inscribed `borsh(Block)` bytes
/// of every block from `parent.tip_id + 1` to `E`; `blocks[t_index]` is `T`.
#[derive(Clone, Debug)]
pub struct CheckpointInput<'a> {
    pub config: Config,
    /// The full state the checkpoint continues from: the parent, or the end of the last segment.
    pub parent: State,
    pub config_changes: Vec<ConfigChange<'a>>,
    pub blocks: Vec<&'a [u8]>,
    pub t_index: u32,
    pub echo_nonce: Hash,
    /// `T`'s inscription, which names the key that inscribed `T`.
    pub producer: ElidedInscription<'a>,
    pub support: Vec<Support<'a>>,
    /// Journals of segments proving `P → … → parent`, verified by the guest with `env::verify`.
    pub segments: Vec<SegmentJournal>,
}

/// Rule (B) (SPEC-v3 D7, D13): as [`CheckpointInput`], with final-tier attestations in place of the
/// echo nonce (always 0) and the support inscriptions. `T`'s inscription is kept: its signer must be
/// a key of the live set, as in rule (A). `blocks` run from `parent.tip_id + 1` to the furthest
/// attested tip; `blocks[t_index]` is `T`.
#[derive(Clone, Debug)]
pub struct CheckpointInputB<'a> {
    pub config: Config,
    pub parent: State,
    pub config_changes: Vec<ConfigChange<'a>>,
    pub blocks: Vec<&'a [u8]>,
    pub t_index: u32,
    /// `T`'s inscription, which names the channel key that inscribed `T`.
    pub producer: ElidedInscription<'a>,
    pub attestations: Vec<SignedAttestation>,
    pub segments: Vec<SegmentJournal>,
}

/// A bounded run of blocks, hashed and extracted without a support check (composition).
#[derive(Clone, Debug)]
pub struct SegmentInput<'a> {
    pub config: Config,
    pub parent: State,
    pub blocks: Vec<&'a [u8]>,
}

// ---- encoding (host) -------------------------------------------------------------------------------

/// Appends little-endian fields to a byte vector.
#[derive(Default)]
pub struct Writer(pub Vec<u8>);

impl Writer {
    pub fn raw(&mut self, b: &[u8]) -> &mut Self {
        self.0.extend_from_slice(b);
        self
    }
    pub fn u8(&mut self, x: u8) -> &mut Self {
        self.raw(&[x])
    }
    pub fn u16(&mut self, x: u16) -> &mut Self {
        self.raw(&x.to_le_bytes())
    }
    pub fn u32(&mut self, x: u32) -> &mut Self {
        self.raw(&x.to_le_bytes())
    }
    pub fn u64(&mut self, x: u64) -> &mut Self {
        self.raw(&x.to_le_bytes())
    }
    fn len(&mut self, n: usize) -> &mut Self {
        self.u32(u32::try_from(n).expect("input sections are far below 4 GiB"))
    }
    pub fn bytes(&mut self, b: &[u8]) -> &mut Self {
        self.len(b.len()).raw(b)
    }

    pub fn config(&mut self, c: &Config) -> &mut Self {
        self.raw(&c.bedrock_genesis_id)
            .raw(&c.channel_id)
            .raw(&c.bridge_header)
            .raw(&c.bridge_image_id)
            .raw(&c.evm_chain_id)
            .raw(&c.vault)
            .raw(&c.attestation_root_id)
            .raw(&c.salt)
            .u32(c.n_min)
            .u32(c.t_min)
    }

    pub fn state(&mut self, s: &State) -> &mut Self {
        self.u64(s.tip_id).raw(&s.tip_hash).len(s.keys.len());
        for k in &s.keys {
            self.raw(k);
        }
        self.u32(s.posting_timeframe).raw(&s.config_tip_hash).u64(s.burns.count);
        for n in &s.burns.nodes {
            self.raw(n);
        }
        self.u64(s.imported_leaf_count).u8(u8::from(s.sunset))
    }

    pub fn inscription(&mut self, i: &ElidedInscription<'_>) -> &mut Self {
        self.bytes(i.prefix).bytes(i.suffix).raw(i.signature)
    }

    pub fn blocks(&mut self, blocks: &[&[u8]]) -> &mut Self {
        self.len(blocks.len());
        for b in blocks {
            self.bytes(b);
        }
        self
    }

    pub fn config_changes(&mut self, changes: &[ConfigChange<'_>]) -> &mut Self {
        self.len(changes.len());
        for c in changes {
            self.bytes(c.ops).u8(c.op_index).len(c.signatures.len());
            for (index, sig) in &c.signatures {
                self.u16(*index).raw(*sig);
            }
        }
        self
    }

    pub fn segments(&mut self, segments: &[SegmentJournal]) -> &mut Self {
        self.len(segments.len());
        for s in segments {
            self.raw(&s.encode());
        }
        self
    }
}

impl CheckpointInput<'_> {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.config(&self.config).state(&self.parent).config_changes(&self.config_changes);
        w.blocks(&self.blocks).u32(self.t_index).raw(&self.echo_nonce).inscription(&self.producer);
        w.len(self.support.len());
        for s in &self.support {
            w.u32(s.block_index).inscription(&s.inscription);
        }
        w.segments(&self.segments);
        w.0
    }
}

impl CheckpointInputB<'_> {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.config(&self.config).state(&self.parent).config_changes(&self.config_changes);
        w.blocks(&self.blocks).u32(self.t_index).inscription(&self.producer).len(self.attestations.len());
        for a in &self.attestations {
            w.raw(&a.encode());
        }
        w.segments(&self.segments);
        w.0
    }
}

impl SegmentInput<'_> {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.config(&self.config).state(&self.parent).blocks(&self.blocks);
        w.0
    }
}

// ---- decoding (guest) ------------------------------------------------------------------------------

fn config(r: &mut Reader<'_>) -> Result<Config, Error> {
    Ok(Config {
        bedrock_genesis_id: *r.array()?,
        channel_id: *r.array()?,
        bridge_header: *r.array()?,
        bridge_image_id: *r.array()?,
        evm_chain_id: *r.array()?,
        vault: *r.array()?,
        attestation_root_id: *r.array()?,
        salt: *r.array()?,
        n_min: r.u32_le()?,
        t_min: r.u32_le()?,
    })
}

fn state(r: &mut Reader<'_>) -> Result<State, Error> {
    let tip_id = r.u64_le()?;
    let tip_hash = *r.array()?;
    let n = r.len_u32("keys", 32)?;
    let mut keys = Vec::with_capacity(n);
    for _ in 0..n {
        keys.push(*r.array()?);
    }
    let posting_timeframe = r.u32_le()?;
    let config_tip_hash = *r.array()?;
    let count = r.u64_le()?;
    let mut nodes = [[0u8; 32]; DEPTH];
    for node in &mut nodes {
        *node = *r.array()?;
    }
    let imported_leaf_count = r.u64_le()?;
    let sunset = r.bool("sunset")?;
    Ok(State {
        tip_id,
        tip_hash,
        keys,
        posting_timeframe,
        config_tip_hash,
        burns: Frontier { count, nodes },
        imported_leaf_count,
        sunset,
    })
}

fn inscription<'a>(r: &mut Reader<'a>) -> Result<ElidedInscription<'a>, Error> {
    Ok(ElidedInscription { prefix: r.bytes_u32("prefix")?, suffix: r.bytes_u32("suffix")?, signature: r.array()? })
}

fn blocks<'a>(r: &mut Reader<'a>) -> Result<Vec<&'a [u8]>, Error> {
    let n = r.len_u32("blocks", 4)?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(r.bytes_u32("block")?);
    }
    Ok(out)
}

fn config_changes<'a>(r: &mut Reader<'a>) -> Result<Vec<ConfigChange<'a>>, Error> {
    let n = r.len_u32("config changes", 9)?;
    let mut config_changes = Vec::with_capacity(n);
    for _ in 0..n {
        let ops = r.bytes_u32("config ops")?;
        let op_index = r.u8()?;
        let k = r.len_u32("config signatures", 66)?;
        let mut signatures = Vec::with_capacity(k);
        for _ in 0..k {
            signatures.push((r.u16_le()?, r.array()?));
        }
        config_changes.push(ConfigChange { ops, op_index, signatures });
    }
    Ok(config_changes)
}

fn segments(r: &mut Reader<'_>) -> Result<Vec<SegmentJournal>, Error> {
    let n = r.len_u32("segments", SEGMENT_JOURNAL_LEN)?;
    let mut segments = Vec::with_capacity(n);
    for _ in 0..n {
        segments.push(SegmentJournal::decode(r.take(SEGMENT_JOURNAL_LEN)?)?);
    }
    Ok(segments)
}

impl<'a> CheckpointInput<'a> {
    pub fn decode(bytes: &'a [u8]) -> Result<Self, Error> {
        let mut r = Reader::new(bytes);
        let config = config(&mut r)?;
        let parent = state(&mut r)?;
        let config_changes = config_changes(&mut r)?;
        let blocks = blocks(&mut r)?;
        let t_index = r.u32_le()?;
        let echo_nonce = *r.array()?;
        let producer = inscription(&mut r)?;
        let n = r.len_u32("support", 76)?;
        let mut support = Vec::with_capacity(n);
        for _ in 0..n {
            support.push(Support { block_index: r.u32_le()?, inscription: inscription(&mut r)? });
        }
        let segments = segments(&mut r)?;
        r.finish()?;
        Ok(Self { config, parent, config_changes, blocks, t_index, echo_nonce, producer, support, segments })
    }
}

impl<'a> CheckpointInputB<'a> {
    pub fn decode(bytes: &'a [u8]) -> Result<Self, Error> {
        let mut r = Reader::new(bytes);
        let config = config(&mut r)?;
        let parent = state(&mut r)?;
        let config_changes = config_changes(&mut r)?;
        let blocks = blocks(&mut r)?;
        let t_index = r.u32_le()?;
        let producer = inscription(&mut r)?;
        let n = r.len_u32("attestations", SIGNED_ATTESTATION_LEN)?;
        let mut attestations = Vec::with_capacity(n);
        for _ in 0..n {
            attestations.push(SignedAttestation::decode(r.array()?));
        }
        let segments = segments(&mut r)?;
        r.finish()?;
        Ok(Self { config, parent, config_changes, blocks, t_index, producer, attestations, segments })
    }
}

impl<'a> SegmentInput<'a> {
    pub fn decode(bytes: &'a [u8]) -> Result<Self, Error> {
        let mut r = Reader::new(bytes);
        let config = config(&mut r)?;
        let parent = state(&mut r)?;
        let blocks = blocks(&mut r)?;
        r.finish()?;
        Ok(Self { config, parent, blocks })
    }
}
