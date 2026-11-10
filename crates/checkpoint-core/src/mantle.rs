//! Logos Blockchain (Bedrock) Mantle transactions at the revision LEZ v0.4.0-rc1 pins
//! (`logos-blockchain` `d480efc6`): just enough to check that an accredited key signed a
//! transaction inscribing a given LEZ block, and to follow channel configuration changes.
//!
//! Formats (canonical binary codec, `binary-codec/src/canonical`; all integers little-endian; a
//! bounded collection's length prefix is 1, 2 or 4 bytes as its `MAX` fits u8, u16 or u32):
//! - transaction (`Ops`) = `u8 count ‖ (opcode u8 ‖ payload)*` (`MAX_OPS_PER_TX` = 255);
//! - transaction hash = `BLAKE2b-256("MANTLE_TXHASH_V1" ‖ Ops)` (`transactions/tx_list`), and
//!   every op proof signs these 32 bytes (`TxHash::as_signing_bytes`);
//! - `InscriptionOp` (0x11) = `channel_id(32) ‖ u32 len ‖ inscription ‖ parent MsgId(32) ‖
//!   signer(32)` (inscription bound `MAX_BYTES` = 2 MiB·7/8 > u16::MAX, hence u32), and its
//!   `MsgId = BLAKE2b-256(encode(op))`;
//! - `ChannelConfigOp` (0x10) = `channel(32) ‖ parent(32) ‖ u16 n ‖ keys(32·n) ‖
//!   posting_timeframe u32 ‖ posting_timeout u32 ‖ configuration_threshold u16 ‖
//!   transfer_threshold u16`, its id (the next `config_tip_hash`) `= BLAKE2b-256(encode(op))`;
//! - `TransferOp` (0x00) = `Inputs ‖ Outputs`; `ChannelWithdrawOp` (0x13) = `channel ‖ Inputs`;
//!   `ChannelTransferOp` (0x14) = `channel ‖ Inputs ‖ Outputs`; `Inputs` = `u8 n ‖ NoteId(32)·n`,
//!   `Outputs` = `u8 n ‖ (value u64 ‖ pk(32))·n`.
//!
//! Other opcodes are refused (fail closed). LEZ's sequencer inscribes blocks in transactions made
//! of an inscription, the fee `Transfer` its node's wallet appends, and, for deposit pins and
//! atomic withdrawals, channel transfers or withdrawals: all parsed here.
//!
//! Parsing only frames the operations; it does not repeat the field-level checks Bedrock's decoder
//! makes (canonical field elements, distinct note ids). A signature binds the exact bytes either
//! way: the framing is what must be unambiguous, so that an honest key's signature over an honest
//! transaction is never read as the inscription of another block.

use alloc::vec::Vec;

use blake2::Digest as _;
use ed25519_dalek::{Signature, VerifyingKey};

use crate::hash::{blake2b256_parts, Blake2b256};
use crate::reader::Reader;
use crate::{Error, Hash};

pub const TX_HASH_PREFIX: &[u8] = b"MANTLE_TXHASH_V1";

pub const OP_TRANSFER: u8 = 0x00;
pub const OP_CHANNEL_CONFIG: u8 = 0x10;
pub const OP_CHANNEL_INSCRIBE: u8 = 0x11;
pub const OP_CHANNEL_WITHDRAW: u8 = 0x13;
pub const OP_CHANNEL_TRANSFER: u8 = 0x14;

/// Skips one operation's payload (after its opcode).
fn skip_op(r: &mut Reader<'_>, opcode: u8) -> Result<(), Error> {
    match opcode {
        OP_TRANSFER => {
            inputs(r)?;
            outputs(r)
        }
        OP_CHANNEL_CONFIG => ChannelConfigOp::read(r).map(|_| ()),
        OP_CHANNEL_INSCRIBE => {
            r.skip(32)?;
            let len = usize::try_from(r.u32_le()?).map_err(|_| Error::BadLength { what: "inscription" })?;
            r.skip(len)?;
            r.skip(64)
        }
        OP_CHANNEL_WITHDRAW => {
            r.skip(32)?;
            inputs(r)
        }
        OP_CHANNEL_TRANSFER => {
            r.skip(32)?;
            inputs(r)?;
            outputs(r)
        }
        opcode => Err(Error::UnsupportedOpcode { opcode }),
    }
}

fn inputs(r: &mut Reader<'_>) -> Result<(), Error> {
    let n = usize::from(r.u8()?);
    r.skip(32 * n)
}

fn outputs(r: &mut Reader<'_>) -> Result<(), Error> {
    let n = usize::from(r.u8()?);
    r.skip(40 * n)
}

/// The opcodes of a whole encoded transaction, checked to frame exactly.
pub fn opcodes(ops: &[u8]) -> Result<Vec<u8>, Error> {
    let mut r = Reader::new(ops);
    let count = r.u8()?;
    let mut out = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        let opcode = r.u8()?;
        skip_op(&mut r, opcode)?;
        out.push(opcode);
    }
    r.finish()?;
    Ok(out)
}

/// The Mantle transaction hash of encoded `Ops`.
pub fn tx_hash(ops: &[u8]) -> Hash {
    blake2b256_parts(&[TX_HASH_PREFIX, ops])
}

/// A transaction that inscribes a block, with the block's bytes elided: the transaction is
/// `prefix ‖ block ‖ suffix`, where `prefix` ends with the inscription op's length field and
/// `suffix` starts with its `parent`. This keeps a block from being sent to the guest twice.
#[derive(Clone, Copy, Debug)]
pub struct ElidedInscription<'a> {
    pub prefix: &'a [u8],
    pub suffix: &'a [u8],
    /// The inscription op's `Ed25519Sig` proof.
    pub signature: &'a [u8; 64],
}

/// What a checked inscription says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckedInscription {
    pub signer: Hash,
    pub parent: Hash,
    pub tx_hash: Hash,
}

impl<'a> ElidedInscription<'a> {
    /// Splits a whole encoded transaction around the payload of its inscription op number
    /// `op_index` (host side).
    pub fn split(ops: &'a [u8], op_index: usize, signature: &'a [u8; 64]) -> Result<Self, Error> {
        let mut r = Reader::new(ops);
        let count = usize::from(r.u8()?);
        for i in 0..count {
            let opcode = r.u8()?;
            if i == op_index {
                if opcode != OP_CHANNEL_INSCRIBE {
                    return Err(Error::NotAnInscription);
                }
                r.skip(32)?;
                let len = usize::try_from(r.u32_le()?).map_err(|_| Error::BadLength { what: "inscription" })?;
                let prefix = r.since(0);
                r.skip(len)?;
                let suffix = &ops[r.pos()..];
                return Ok(Self { prefix, suffix, signature });
            }
            skip_op(&mut r, opcode)?;
        }
        Err(Error::NotAnInscription)
    }

    /// Checks that the transaction `prefix ‖ block ‖ suffix` is a well-framed Mantle transaction
    /// whose op ending `prefix` is an inscription of `block` on `channel_id`, and that its signer
    /// signed the transaction hash (`verify_strict`, as Bedrock's preverify does). Hashes the
    /// whole transaction once with BLAKE2b; the block bytes are streamed, not copied.
    pub fn check(&self, block: &[u8], channel_id: &Hash) -> Result<CheckedInscription, Error> {
        // Frame `prefix`: whole ops, then the inscription op's opcode, channel and length.
        let mut r = Reader::new(self.prefix);
        let count = usize::from(r.u8()?);
        let mut before = 0usize;
        loop {
            if before >= count {
                return Err(Error::NotAnInscription);
            }
            let opcode = r.u8()?;
            // The inscription op is the one whose header ends exactly at the end of `prefix`.
            if opcode == OP_CHANNEL_INSCRIBE && r.remaining() == 32 + 4 {
                break;
            }
            skip_op(&mut r, opcode)?;
            before += 1;
        }
        let channel = r.array::<32>()?;
        let len = r.u32_le()?;
        r.finish()?;
        if channel != channel_id {
            return Err(Error::WrongChannel);
        }
        if usize::try_from(len).ok() != Some(block.len()) {
            return Err(Error::PayloadMismatch);
        }
        // Frame `suffix`: the rest of the inscription op, then the remaining ops.
        let mut s = Reader::new(self.suffix);
        let parent = *s.array::<32>()?;
        let signer = *s.array::<32>()?;
        for _ in before + 1..count {
            let opcode = s.u8()?;
            skip_op(&mut s, opcode)?;
        }
        s.finish()?;

        let mut h = Blake2b256::new();
        h.update(TX_HASH_PREFIX);
        h.update(self.prefix);
        h.update(block);
        h.update(self.suffix);
        let tx_hash: Hash = h.finalize().into();
        verify_strict(&signer, &tx_hash, self.signature)?;
        Ok(CheckedInscription { signer, parent, tx_hash })
    }
}

/// `InscriptionOp::id()` (the `MsgId`) of an inscription of `payload`.
pub fn inscription_msg_id(channel_id: &Hash, payload: &[u8], parent: &Hash, signer: &Hash) -> Hash {
    let len = u32::try_from(payload.len()).unwrap_or(u32::MAX).to_le_bytes();
    blake2b256_parts(&[channel_id, &len, payload, parent, signer])
}

/// Ed25519 `verify_strict` of `signature` by `key` over `message` (refuses small-order keys and
/// non-canonical encodings, as `ed25519-dalek` 2.x's `verify_strict` does upstream).
pub fn verify_strict(key: &Hash, message: &[u8], signature: &[u8; 64]) -> Result<(), Error> {
    let key = VerifyingKey::from_bytes(key).map_err(|_| Error::InvalidKey)?;
    key.verify_strict(message, &Signature::from_bytes(signature)).map_err(|_| Error::BadSignature)
}

/// A decoded `ChannelConfigOp` (keys borrowed from the transaction bytes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelConfigOp<'a> {
    pub channel: &'a Hash,
    pub parent: &'a Hash,
    pub keys: Vec<&'a Hash>,
    pub posting_timeframe: u32,
    pub posting_timeout: u32,
    pub configuration_threshold: u16,
    pub transfer_threshold: u16,
    /// The op's own encoding (without the opcode), whose BLAKE2b-256 is its id.
    pub encoded: &'a [u8],
}

impl<'a> ChannelConfigOp<'a> {
    fn read(r: &mut Reader<'a>) -> Result<Self, Error> {
        let start = r.pos();
        let channel = r.array::<32>()?;
        let parent = r.array::<32>()?;
        // VerifiedChannelKeys = NonEmptyBoundedVec<Ed25519PublicKey, u16::MAX>: u16 prefix.
        let n = usize::from(r.u16_le()?);
        if n == 0 {
            return Err(Error::ConfigMalformed);
        }
        let mut keys = Vec::with_capacity(n.min(r.remaining() / 32));
        for _ in 0..n {
            keys.push(r.array::<32>()?);
        }
        let posting_timeframe = r.u32_le()?;
        let posting_timeout = r.u32_le()?;
        let configuration_threshold = r.u16_le()?;
        let transfer_threshold = r.u16_le()?;
        Ok(Self {
            channel,
            parent,
            keys,
            posting_timeframe,
            posting_timeout,
            configuration_threshold,
            transfer_threshold,
            encoded: r.since(start),
        })
    }

    /// `ChannelConfigOp::id()`: the `config_tip_hash` the channel records after this op.
    pub fn id(&self) -> Hash {
        blake2b256_parts(&[self.encoded])
    }
}

/// A channel configuration change as the program receives it: the whole transaction carrying it,
/// the position of the `ChannelConfigOp`, and its `ChannelMultiSigProof` as `(key index,
/// signature)` pairs over the transaction hash.
#[derive(Clone, Debug)]
pub struct ConfigChange<'a> {
    pub ops: &'a [u8],
    pub op_index: u8,
    pub signatures: Vec<(u16, &'a [u8; 64])>,
}

impl<'a> ConfigChange<'a> {
    /// Frames the transaction and returns the configuration op at `op_index` and the tx hash.
    pub fn op(&self) -> Result<(ChannelConfigOp<'a>, Hash), Error> {
        let mut r = Reader::new(self.ops);
        let count = r.u8()?;
        let mut found = None;
        for i in 0..count {
            let opcode = r.u8()?;
            if i == self.op_index {
                if opcode != OP_CHANNEL_CONFIG {
                    return Err(Error::ConfigMalformed);
                }
                found = Some(ChannelConfigOp::read(&mut r)?);
            } else {
                skip_op(&mut r, opcode)?;
            }
        }
        r.finish()?;
        let op = found.ok_or(Error::ConfigMalformed)?;
        Ok((op, tx_hash(self.ops)))
    }
}
