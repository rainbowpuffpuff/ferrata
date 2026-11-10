//! The recorded chain: the sequencer's blocks as borsh, as its RPC serves them.
//!
//! Two forms. The unabridged log (a borsh `Vec<Block>`) stays in the run's scratch directory.
//! The published log is the same list with the bytecode of every program-loader `WriteSegment`
//! removed: the guest binaries embed the absolute paths of the machine that built them, so they
//! are not published, and each removed chunk is listed with its SHA-256 instead
//! (`segments.tsv`). Putting the chunks back from the binaries reproduces the blocks byte for
//! byte, which the block hashes then prove.

use std::{collections::HashMap, fs, path::Path};

use anyhow::{anyhow, bail, Context, Result};
use common::{block::Block, transaction::LeeTransaction};
use lee::PublicTransaction;
use lee_core::program::PROGRAM_LOADER_ACCOUNT_ID;
use program_loader_core::Message as LoaderMessage;

use crate::programs::sha256_hex;

pub fn save(path: &Path, blocks: &[Block]) -> Result<()> {
    let bytes = borsh::to_vec(&blocks.to_vec())?;
    fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))
}

pub fn load(path: &Path) -> Result<Vec<Block>> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    borsh::from_slice(&bytes).map_err(|e| anyhow!("{}: not a borsh list of LEZ blocks: {e}", path.display()))
}

/// A program chunk removed from the published log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Elided {
    pub block_id: u64,
    pub position: u64,
    /// Hash of the transaction as the sequencer included it (with the chunk).
    pub tx_hash: String,
    /// The segment account the chunk was written to.
    pub segment: String,
    pub bytes: u64,
    pub sha256: String,
}

pub const SEGMENTS_HEADER: &str = "block\tposition\ttx_hash\tsegment_account\tbytes\tsha256";

impl Elided {
    pub fn line(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            self.block_id, self.position, self.tx_hash, self.segment, self.bytes, self.sha256
        )
    }

    pub fn parse(line: &str) -> Result<Self> {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() != 6 {
            bail!("segments.tsv: expected 6 fields, got {}: {line}", f.len());
        }
        Ok(Self {
            block_id: f[0].parse()?,
            position: f[1].parse()?,
            tx_hash: f[2].to_string(),
            segment: f[3].to_string(),
            bytes: f[4].parse()?,
            sha256: f[5].to_string(),
        })
    }
}

pub fn write_segments(path: &Path, elided: &[Elided]) -> Result<()> {
    let mut text = String::from(SEGMENTS_HEADER);
    text.push('\n');
    for e in elided {
        text.push_str(&e.line());
        text.push('\n');
    }
    fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

pub fn read_segments(path: &Path) -> Result<Vec<Elided>> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    text.lines().skip(1).filter(|l| !l.trim().is_empty()).map(Elided::parse).collect()
}

/// The `WriteSegment` a transaction carries, if it is one.
fn write_segment(tx: &LeeTransaction) -> Option<(&PublicTransaction, Vec<u8>, Option<lee::AccountId>)> {
    let LeeTransaction::Public(public_tx) = tx else { return None };
    let root = &public_tx.message().execution.root;
    if root.to.program_account_id != PROGRAM_LOADER_ACCOUNT_ID {
        return None;
    }
    match borsh::from_slice::<LoaderMessage>(&root.message) {
        Ok(LoaderMessage::WriteSegment { bytecode, next_segment }) => Some((public_tx, bytecode, next_segment)),
        _ => None,
    }
}

fn with_bytecode(public_tx: &PublicTransaction, bytecode: Vec<u8>, next_segment: Option<lee::AccountId>) -> Result<LeeTransaction> {
    let mut message = public_tx.message().clone();
    message.execution.root.message = borsh::to_vec(&LoaderMessage::WriteSegment { bytecode, next_segment })?;
    Ok(LeeTransaction::Public(PublicTransaction::new(message, public_tx.witness_set().clone())))
}

/// The blocks without program bytecode, and what was removed.
pub fn elide(blocks: &[Block]) -> Result<(Vec<Block>, Vec<Elided>)> {
    let mut out = blocks.to_vec();
    let mut removed = Vec::new();
    for block in &mut out {
        for (position, tx) in block.body.transactions.iter_mut().enumerate() {
            let Some((public_tx, bytecode, next_segment)) = write_segment(tx) else { continue };
            if bytecode.is_empty() {
                continue;
            }
            removed.push(Elided {
                block_id: block.header.block_id,
                position: position as u64,
                tx_hash: tx.hash().to_string(),
                segment: public_tx.message().execution.root.to.account_id.to_string(),
                bytes: bytecode.len() as u64,
                sha256: sha256_hex(&bytecode),
            });
            let stripped = with_bytecode(public_tx, Vec::new(), next_segment)?;
            *tx = stripped;
        }
    }
    Ok((out, removed))
}

/// Put the removed chunks back. `chunks` maps a chunk's SHA-256 to its bytes.
pub fn restore(blocks: &mut [Block], elided: &[Elided], chunks: &HashMap<String, Vec<u8>>) -> Result<()> {
    for e in elided {
        let block = blocks
            .iter_mut()
            .find(|b| b.header.block_id == e.block_id)
            .ok_or_else(|| anyhow!("segments.tsv names block {}, which the log does not hold", e.block_id))?;
        let tx = block
            .body
            .transactions
            .get_mut(e.position as usize)
            .ok_or_else(|| anyhow!("block {} has no transaction {}", e.block_id, e.position))?;
        let (public_tx, bytecode, next_segment) =
            write_segment(tx).ok_or_else(|| anyhow!("block {} transaction {} is not a WriteSegment", e.block_id, e.position))?;
        if !bytecode.is_empty() {
            bail!("block {} transaction {} still holds its bytecode", e.block_id, e.position);
        }
        let chunk = chunks.get(&e.sha256).ok_or_else(|| {
            anyhow!(
                "no guest binary given holds the chunk {} ({} bytes) of block {} transaction {}",
                e.sha256,
                e.bytes,
                e.block_id,
                e.position
            )
        })?;
        let restored = with_bytecode(public_tx, chunk.clone(), next_segment)?;
        if restored.hash().to_string() != e.tx_hash {
            bail!("block {} transaction {}: the restored transaction has another hash", e.block_id, e.position);
        }
        *tx = restored;
    }
    Ok(())
}
