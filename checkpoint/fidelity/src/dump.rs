//! Real blocks produced by LEZ's own sequencer, wallet and prover: the committed test fixture
//! `test_fixtures/fixtures/prebuilt_sequencer_db.dump` of LEZ v0.4.0-rc1 ("genesis +
//! private-funding blocks", made with `RISC0_DEV_MODE=1`, so its privacy-preserving receipts are
//! dev-mode receipts, not real proofs).
//!
//! Format (`lez/sequencer/actors/storage/src/actor/db.rs`): `zstd(borsh(Dump { entries:
//! Vec<DumpEntry { column_family: String, key: Vec<u8>, value: Vec<u8> }> }))`; a stored block is
//! the entity `Block { block: common::block::Block }` (`entities.rs`) under column family `Block`,
//! key `"Block" ‖ 0x00 ‖ id (big-endian)`, so its value is `borsh(common::block::Block)`.

use std::path::PathBuf;

use anyhow::{bail, Context as _, Result};
use borsh::BorshDeserialize;
use common::block::Block;

#[derive(BorshDeserialize)]
struct Dump {
    entries: Vec<DumpEntry>,
}

#[derive(BorshDeserialize)]
struct DumpEntry {
    column_family: String,
    key: Vec<u8>,
    value: Vec<u8>,
}

pub fn dump_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../upstream/lez-v0.4.0-rc1/test_fixtures/fixtures/prebuilt_sequencer_db.dump")
}

/// The stored blocks, by id, as the raw bytes the database held and as decoded by LEZ's types.
pub fn dump_blocks() -> Result<Vec<(Vec<u8>, Block)>> {
    let compressed = std::fs::read(dump_path()).context("read the LEZ sequencer dump")?;
    let raw = zstd::decode_all(compressed.as_slice()).context("zstd")?;
    let dump: Dump = borsh::from_slice(&raw).context("borsh Dump")?;
    let mut blocks = Vec::new();
    for e in dump.entries {
        if e.column_family == "Block" && e.key.starts_with(b"Block\0") {
            let block: Block = borsh::from_slice(&e.value).context("borsh Block")?;
            let id = u64::from_be_bytes(e.key[6..].try_into().context("key")?);
            if id != block.header.block_id {
                bail!("key {id} holds block {}", block.header.block_id);
            }
            blocks.push((e.value, block));
        }
    }
    blocks.sort_by_key(|(_, b)| b.header.block_id);
    Ok(blocks)
}
