//! Mantle transactions of Logos Blockchain 0.3.5-rc.1 (`f6d4267f`): [`crate::mantle`]'s formats with the fork digest.
//!
//! - transaction = `fork_digest(32) ‖ u8 count ‖ (opcode u8 ‖ payload)*`: `Ops`' canonical encoding, the `ForkDigest`
//!   then the operations' column (`core/src/mantle/transactions/tx_list/ops.rs`, `BinaryEncode for Ops`);
//! - transaction hash = `BLAKE2b-256("MANTLE_TXHASH_V1" ‖ fork_digest ‖ ops)` (`tx_list/op_refs.rs`,
//!   `OpRefs::as_signing`), which every op proof signs: [`crate::mantle::tx_hash`] over the whole transaction;
//! - payloads as at `d480efc6` ([`crate::mantle`]'s module doc) for Transfer (0x00), ChannelConfig (0x10),
//!   ChannelInscribe (0x11), ChannelWithdraw (0x13) and ChannelTransfer (0x14); other opcodes are refused;
//! - a `ChannelConfigOp` decodes only with both thresholds non-zero and `configuration_threshold` at most its key count
//!   (`ChannelConfigFields` → `ChannelConfigOp::new`, logos-blockchain #3781), and is refused otherwise.
//!
//! The framing is [`crate::mantle`]'s, repeated here after the digest so that the default build's file, whose panic
//! locations are compiled into the deployed guests, stays as it is. The data types (`ElidedInscription`,
//! `ConfigChange`, `ChannelConfigOp`, `CheckedInscription`) are the default build's: an elided inscription's `prefix`
//! now starts with the transaction's fork digest.

use alloc::vec::Vec;

use blake2::Digest as _;

use super::Error;
use crate::hash::Blake2b256;
use crate::mantle::{
    tx_hash, verify_strict, ChannelConfigOp, CheckedInscription, ConfigChange, ElidedInscription, OP_CHANNEL_CONFIG,
    OP_CHANNEL_INSCRIBE, OP_CHANNEL_TRANSFER, OP_CHANNEL_WITHDRAW, OP_TRANSFER, TX_HASH_PREFIX,
};
use crate::reader::Reader;
use crate::Hash;

/// Bytes of the fork digest in front of a transaction's operations.
pub const FORK_DIGEST_LEN: usize = 32;

/// Reads the fork digest at the start of a transaction and refuses it unless it is `expected`.
fn digest_is(r: &mut Reader<'_>, expected: &Hash) -> Result<(), Error> {
    if r.array::<FORK_DIGEST_LEN>()? != expected {
        return Err(Error::WrongForkDigest);
    }
    Ok(())
}

/// The fork digest a whole encoded transaction carries.
pub fn fork_digest_of(tx: &[u8]) -> Result<&Hash, Error> {
    Ok(Reader::new(tx).array::<FORK_DIGEST_LEN>()?)
}

/// Skips one operation's payload (after its opcode).
fn skip_op(r: &mut Reader<'_>, opcode: u8) -> Result<(), Error> {
    match opcode {
        OP_TRANSFER => {
            inputs(r)?;
            outputs(r)
        }
        OP_CHANNEL_CONFIG => read_config(r).map(|_| ()),
        OP_CHANNEL_INSCRIBE => {
            r.skip(32)?;
            let len = usize::try_from(r.u32_le()?).map_err(|_| crate::Error::BadLength { what: "inscription" })?;
            r.skip(len)?;
            Ok(r.skip(64)?)
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
        opcode => Err(crate::Error::UnsupportedOpcode { opcode }.into()),
    }
}

fn inputs(r: &mut Reader<'_>) -> Result<(), Error> {
    let n = usize::from(r.u8()?);
    Ok(r.skip(32 * n)?)
}

fn outputs(r: &mut Reader<'_>) -> Result<(), Error> {
    let n = usize::from(r.u8()?);
    Ok(r.skip(40 * n)?)
}

/// A `ChannelConfigOp` payload, decoded as 0.3.5-rc.1 decodes it (module docs).
fn read_config<'a>(r: &mut Reader<'a>) -> Result<ChannelConfigOp<'a>, Error> {
    let start = r.pos();
    let channel = r.array::<32>()?;
    let parent = r.array::<32>()?;
    let n = usize::from(r.u16_le()?);
    if n == 0 {
        return Err(crate::Error::ConfigMalformed.into());
    }
    let mut keys = Vec::with_capacity(n.min(r.remaining() / 32));
    for _ in 0..n {
        keys.push(r.array::<32>()?);
    }
    let posting_timeframe = r.u32_le()?;
    let posting_timeout = r.u32_le()?;
    let configuration_threshold = r.u16_le()?;
    let transfer_threshold = r.u16_le()?;
    if configuration_threshold == 0 || transfer_threshold == 0 {
        return Err(crate::Error::ConfigMalformed.into());
    }
    if usize::from(configuration_threshold) > n {
        return Err(Error::UnreachableThreshold {
            threshold: configuration_threshold,
            keys: u32::try_from(n).unwrap_or(u32::MAX),
        });
    }
    Ok(ChannelConfigOp {
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

/// The opcodes of a whole encoded transaction (fork digest, then the operations), checked to frame exactly. The digest
/// is not compared here.
pub fn opcodes(tx: &[u8]) -> Result<Vec<u8>, Error> {
    let mut r = Reader::new(tx);
    r.skip(FORK_DIGEST_LEN)?;
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

/// Splits a whole encoded transaction around the payload of its inscription op number `op_index` (host side); the
/// `prefix` starts with the transaction's fork digest.
pub fn split<'a>(tx: &'a [u8], op_index: usize, signature: &'a [u8; 64]) -> Result<ElidedInscription<'a>, Error> {
    let mut r = Reader::new(tx);
    r.skip(FORK_DIGEST_LEN)?;
    let count = usize::from(r.u8()?);
    for i in 0..count {
        let opcode = r.u8()?;
        if i == op_index {
            if opcode != OP_CHANNEL_INSCRIBE {
                return Err(crate::Error::NotAnInscription.into());
            }
            r.skip(32)?;
            let len = usize::try_from(r.u32_le()?).map_err(|_| crate::Error::BadLength { what: "inscription" })?;
            let prefix = r.since(0);
            r.skip(len)?;
            let suffix = &tx[r.pos()..];
            return Ok(ElidedInscription { prefix, suffix, signature });
        }
        skip_op(&mut r, opcode)?;
    }
    Err(crate::Error::NotAnInscription.into())
}

/// [`ElidedInscription::check`] for 0.3.5-rc.1: the transaction `prefix ‖ block ‖ suffix` carries the fork digest
/// `fork_digest`, then is a well-framed operations column whose op ending `prefix` is an inscription of `block` on
/// `channel_id`; its signer signed the transaction hash over the digest and the operations (`verify_strict`). Hashes
/// the whole transaction once with BLAKE2b; the block bytes are streamed, not copied.
pub fn check(
    inscription: &ElidedInscription<'_>,
    block: &[u8],
    channel_id: &Hash,
    fork_digest: &Hash,
) -> Result<CheckedInscription, Error> {
    let mut r = Reader::new(inscription.prefix);
    digest_is(&mut r, fork_digest)?;
    let count = usize::from(r.u8()?);
    let mut before = 0usize;
    loop {
        if before >= count {
            return Err(crate::Error::NotAnInscription.into());
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
        return Err(crate::Error::WrongChannel.into());
    }
    if usize::try_from(len).ok() != Some(block.len()) {
        return Err(crate::Error::PayloadMismatch.into());
    }
    let mut s = Reader::new(inscription.suffix);
    let parent = *s.array::<32>()?;
    let signer = *s.array::<32>()?;
    for _ in before + 1..count {
        let opcode = s.u8()?;
        skip_op(&mut s, opcode)?;
    }
    s.finish()?;

    let mut h = Blake2b256::new();
    h.update(TX_HASH_PREFIX);
    h.update(inscription.prefix);
    h.update(block);
    h.update(inscription.suffix);
    let tx_hash: Hash = h.finalize().into();
    verify_strict(&signer, &tx_hash, inscription.signature)?;
    Ok(CheckedInscription { signer, parent, tx_hash })
}

/// [`ConfigChange::op`] for 0.3.5-rc.1: the transaction carries `fork_digest`, frames exactly, and holds a
/// `ChannelConfigOp` at `op_index` that decodes as 0.3.5-rc.1 decodes it; returns the op and the transaction hash.
pub fn config_op<'a>(change: &ConfigChange<'a>, fork_digest: &Hash) -> Result<(ChannelConfigOp<'a>, Hash), Error> {
    let mut r = Reader::new(change.ops);
    digest_is(&mut r, fork_digest)?;
    let count = r.u8()?;
    let mut found = None;
    for i in 0..count {
        let opcode = r.u8()?;
        if i == change.op_index {
            if opcode != OP_CHANNEL_CONFIG {
                return Err(crate::Error::ConfigMalformed.into());
            }
            found = Some(read_config(&mut r)?);
        } else {
            skip_op(&mut r, opcode)?;
        }
    }
    r.finish()?;
    let op = found.ok_or(crate::Error::ConfigMalformed)?;
    Ok((op, tx_hash(change.ops)))
}
