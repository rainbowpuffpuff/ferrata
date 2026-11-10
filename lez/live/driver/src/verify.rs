//! Re-checking a recorded run.
//!
//! `verify`: from the files alone. Without the guest binaries it checks what needs no
//! execution: the chain links, the hash of every block whose bytes are whole, the manifest
//! against the blocks (where each transaction is, whether the block lists it as reverted) and
//! against the gas totals the sequencer wrote into each block's fee transaction. With the
//! binaries it puts the program bytecode back, which every block hash then confirms, replays
//! the whole chain through LEZ's block application from the pre-genesis state, and compares
//! every row with what LEZ's settlement charges.
//!
//! `check-rpc`: against a running sequencer: it must serve the recorded chain.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, ensure, Result};
use common::{block::Block, transaction::LeeTransaction};
use ferrata_core::witness::CompactStore;
use ferrata_lez_wire::InstanceState;
use lee::{AccountId, GENESIS_BLOCK_ID};
use lee_core::program::PROGRAM_LOADER_ACCOUNT_ID;

use crate::{
    blocklog::{self, Elided},
    follower::{block_summary, Follower},
    manifest::{self, Row},
    plan::{exit_name, mainnet_config, root, Instance, EXPECTED_FINALIZED, TRUSTED_BLOCK_ROOT},
    programs::{image_id_hex, sha256_hex, GuestBinary},
};

pub struct Recorded {
    pub blocks: Vec<Block>,
    pub elided: Vec<Elided>,
    pub rows: Vec<Row>,
    pub programs: Vec<ProgramRecord>,
}

/// A row of `programs.tsv`.
#[derive(Clone, Debug)]
pub struct ProgramRecord {
    pub name: String,
    pub account: String,
    pub image: String,
}

pub fn read_programs(path: &Path) -> Result<Vec<ProgramRecord>> {
    let text = std::fs::read_to_string(path)?;
    let mut out = Vec::new();
    for line in text.lines().skip(1).filter(|l| !l.trim().is_empty()) {
        let f: Vec<&str> = line.split('\t').collect();
        ensure!(f.len() >= 3, "programs.tsv: {line}");
        out.push(ProgramRecord { name: f[0].to_string(), account: f[1].to_string(), image: f[2].to_string() });
    }
    Ok(out)
}

pub fn load(dir: &Path) -> Result<Recorded> {
    Ok(Recorded {
        blocks: blocklog::load(&dir.join("blocks.borsh"))?,
        elided: blocklog::read_segments(&dir.join("segments.tsv"))?,
        rows: manifest::read(&dir.join("run.tsv"))?,
        programs: read_programs(&dir.join("programs.tsv"))?,
    })
}

/// The hash of the transaction at `position` of `block` as the sequencer included it.
fn recorded_hash(block: &Block, position: usize, elided: &[Elided]) -> Option<String> {
    let id = block.header.block_id;
    if let Some(e) = elided.iter().find(|e| e.block_id == id && e.position as usize == position) {
        return Some(e.tx_hash.clone());
    }
    block.body.transactions.get(position).map(|tx| tx.hash().to_string())
}

/// What needs no execution.
pub fn check_static(recorded: &Recorded) -> Result<String> {
    let Recorded { blocks, elided, rows, .. } = recorded;
    ensure!(!blocks.is_empty(), "no blocks");
    let holed: HashSet<u64> = elided.iter().map(|e| e.block_id).collect();
    let mut whole = 0;
    for (i, block) in blocks.iter().enumerate() {
        let id = block.header.block_id;
        ensure!(id == GENESIS_BLOCK_ID + i as u64, "block {id} is out of sequence");
        if i > 0 {
            ensure!(block.header.prev_block_hash == blocks[i - 1].header.hash, "block {id} does not chain on block {}", id - 1);
        }
        if !holed.contains(&id) {
            ensure!(block.recompute_hash() == block.header.hash, "block {id}: the hash does not match the contents");
            whole += 1;
        }
        ensure!(block.body.transactions.len() >= 2, "block {id} has no fee and clock transactions");
        if id != GENESIS_BLOCK_ID {
            block_summary(block).ok_or_else(|| anyhow!("block {id}: no fee transaction"))?;
        }
    }
    let by_id: HashMap<u64, &Block> = blocks.iter().map(|b| (b.header.block_id, b)).collect();
    let mut every_hash: HashSet<String> = HashSet::new();
    for block in blocks {
        for position in 0..block.body.transactions.len() {
            every_hash.extend(recorded_hash(block, position, elided));
        }
    }
    let (mut included, mut absent, mut alone) = (0, 0, 0);
    for row in rows {
        let label = &row.step;
        if row.block.is_empty() {
            ensure!(
                matches!(row.status.as_str(), "dropped" | "rejected" | "unfinished"),
                "{label}: no block, but status {}",
                row.status
            );
            if !row.tx_hash.is_empty() {
                ensure!(!every_hash.contains(&row.tx_hash), "{label}: recorded as not included, but a block holds it");
            }
            absent += 1;
            continue;
        }
        let block = by_id.get(&row.block.parse::<u64>()?).ok_or_else(|| anyhow!("{label}: block {} is not in the log", row.block))?;
        let position: usize = row.position.parse()?;
        ensure!(
            recorded_hash(block, position, elided).as_deref() == Some(row.tx_hash.as_str()),
            "{label}: block {} position {position} holds another transaction",
            row.block
        );
        let listed = block.body.is_reverted(position);
        ensure!(listed == (row.status != "applied"), "{label}: status {} but the block's reverted list says {listed}", row.status);
        let summary = block_summary(block).ok_or_else(|| anyhow!("block {}: no fee transaction", row.block))?;
        ensure!(row.block_exec_gas == summary.gas_used_exec.to_string(), "{label}: the block's execution gas");
        ensure!(row.block_stor_gas == summary.gas_used_stor.to_string(), "{label}: the block's storage gas");
        let user_txs = block.body.transactions.len() - 2;
        ensure!(row.block_txs == user_txs.to_string(), "{label}: the block's transaction count");
        if user_txs == 1 {
            // Alone in its block: the sequencer's own totals are this transaction's.
            ensure!(row.exec_gas == summary.gas_used_exec.to_string(), "{label}: execution gas against the block's fee transaction");
            ensure!(row.stor_gas == summary.gas_used_stor.to_string(), "{label}: storage gas against the block's fee transaction");
            ensure!(row.fee == summary.revenue_base.to_string(), "{label}: fee against the block's fee transaction");
            alone += 1;
        }
        included += 1;
    }
    Ok(format!(
        "{} blocks (1..={}), chain links hold; {} block hashes recomputed, {} blocks hold elided program bytecode; \
         {} manifest rows: {} included at the recorded block and position with the recorded revert flag \
         ({} alone in their block: gas and fee equal the totals in the block's fee transaction), {} not included",
        blocks.len(),
        blocks.last().map_or(0, |b| b.header.block_id),
        whole,
        holed.len(),
        rows.len(),
        included,
        alone,
        absent
    ))
}

/// The chunks of the given binaries, by SHA-256.
pub fn chunk_map(binaries: &[GuestBinary]) -> HashMap<String, Vec<u8>> {
    let mut map = HashMap::new();
    for binary in binaries {
        for chunk in binary.chunks() {
            map.insert(sha256_hex(chunk), chunk.to_vec());
        }
    }
    map
}

/// With the guest binaries: restore, replay through LEZ's state machine, compare every row.
pub fn check_full(recorded: &Recorded, binaries: &[GuestBinary]) -> Result<(Follower, String)> {
    let mut blocks = recorded.blocks.clone();
    blocklog::restore(&mut blocks, &recorded.elided, &chunk_map(binaries))?;
    let mut follower = Follower::new();
    for block in blocks {
        follower.apply(block)?;
    }
    let mut compared = 0;
    for row in &recorded.rows {
        if row.block.is_empty() {
            ensure!(!follower.receipts.contains_key(&row.tx_hash), "{}: the replay includes it", row.step);
            continue;
        }
        let receipt = follower.receipts.get(&row.tx_hash).ok_or_else(|| anyhow!("{}: the replay has no such transaction", row.step))?;
        let want = Row { phase: row.phase.clone(), step: row.step.clone(), tx_hash: row.tx_hash.clone(), ..Row::default() }
            .with_receipt(receipt, exit_name);
        for (name, recorded, replayed) in [
            ("block", &row.block, &want.block),
            ("position", &row.position, &want.position),
            ("status", &row.status, &want.status),
            ("exit", &row.exit, &want.exit),
            ("exec_gas", &row.exec_gas, &want.exec_gas),
            ("stor_gas", &row.stor_gas, &want.stor_gas),
            ("gas_limit", &row.gas_limit, &want.gas_limit),
            ("base_fee_exec", &row.base_fee_exec, &want.base_fee_exec),
            ("base_fee_stor", &row.base_fee_stor, &want.base_fee_stor),
            ("fee", &row.fee, &want.fee),
            ("block_txs", &row.block_txs, &want.block_txs),
            ("block_exec_gas", &row.block_exec_gas, &want.block_exec_gas),
            ("block_stor_gas", &row.block_stor_gas, &want.block_stor_gas),
        ] {
            ensure!(recorded == replayed, "{}: {name} is recorded as {recorded:?}, the replay gives {replayed:?}", row.step);
        }
        compared += 1;
    }
    // The programs the manifest names are deployed with the images it names.
    for program in &recorded.programs {
        let account: AccountId = program.account.parse().map_err(|e| anyhow!("programs.tsv: {e:?}"))?;
        let state = follower.state.get_account_by_id(account).data.actor_state(PROGRAM_LOADER_ACCOUNT_ID).to_vec();
        let header = crate::programs::program_header(&state).ok_or_else(|| anyhow!("{}: no program header at {account}", program.name))?;
        ensure!(image_id_hex(header.image_id) == program.image, "{}: the loader computed another image", program.name);
        ensure!(header.immutable, "{}: not immutable", program.name);
    }
    // Instance A of the main deployment ends at the last update's finalized header.
    let mut finalized = String::new();
    if let Some(main) = recorded.programs.iter().find(|p| p.name == "main") {
        let account: AccountId = main.account.parse().map_err(|e| anyhow!("programs.tsv: {e:?}"))?;
        let inst = Instance::new("A", account, mainnet_config(2), root(TRUSTED_BLOCK_ROOT));
        let actor = inst.pda().actor();
        let state = follower.state.get_account_by_id(actor.account_id).data.actor_state(actor.program_account_id).to_vec();
        if !state.is_empty() {
            let state: InstanceState = borsh::from_slice(&state)?;
            let store = CompactStore::decode(&state.store).map_err(|e| anyhow!("instance A: {e:?}"))?;
            let (slot, block_root) = EXPECTED_FINALIZED[EXPECTED_FINALIZED.len() - 1];
            if state.applied as usize == EXPECTED_FINALIZED.len() + 1 {
                ensure!(store.finalized.slot == slot, "instance A: finalized slot {}", store.finalized.slot);
                ensure!(store.finalized.hash_tree_root() == root(block_root), "instance A: finalized root");
            }
            finalized = format!(
                "; instance A applied {} jobs and holds finalized slot {} with root {}",
                state.applied,
                store.finalized.slot,
                hex::encode(store.finalized.hash_tree_root())
            );
        }
    }
    let summary = format!(
        "program bytecode restored ({} chunks), every block hash holds; {} blocks applied by LEZ's chain_state from the \
         pre-genesis state; {} manifest rows equal what LEZ's settlement charges; {} program image(s) as recorded{finalized}",
        recorded.elided.len(),
        follower.blocks.len(),
        compared,
        recorded.programs.len()
    );
    Ok((follower, summary))
}

pub fn verify(dir: &Path, binary_paths: &[PathBuf]) -> Result<()> {
    let recorded = load(dir)?;
    println!("static: {}", check_static(&recorded)?);
    if binary_paths.is_empty() {
        println!("full replay: SKIPPED (no guest binaries given; the log holds no program bytecode)");
        return Ok(());
    }
    let binaries = binary_paths
        .iter()
        .map(|p| GuestBinary::load(&p.file_name().and_then(|n| n.to_str()).unwrap_or("binary").to_string(), p))
        .collect::<Result<Vec<_>>>()?;
    for binary in &binaries {
        println!("binary {}: image {}, sha256 {}", binary.name, binary.image_hex(), binary.sha256());
    }
    let (_, summary) = check_full(&recorded, &binaries)?;
    println!("full replay: {summary}");
    Ok(())
}

/// A running sequencer serves the recorded chain: same block hashes, same transactions at the
/// same blocks.
pub async fn check_rpc(dir: &Path, wallet: &wallet::WalletCore<sequencer_service_rpc::SequencerClient>) -> Result<()> {
    let recorded = load(dir)?;
    let head = wallet.get_last_block_id().await?;
    let last = recorded.blocks.last().map_or(0, |b| b.header.block_id);
    ensure!(head >= last, "the sequencer is at block {head}, the record ends at {last}");
    for block in &recorded.blocks {
        let id = block.header.block_id;
        let served = wallet.get_block(id).await?.ok_or_else(|| anyhow!("the sequencer has no block {id}"))?;
        if served.header.hash != block.header.hash {
            bail!("block {id}: the sequencer serves {}, the record holds {}", served.header.hash, block.header.hash);
        }
    }
    let mut checked = 0;
    for row in &recorded.rows {
        if row.tx_hash.is_empty() {
            continue;
        }
        let hash: common::HashType = row.tx_hash.parse().map_err(|e| anyhow!("{}: {e}", row.step))?;
        let served = wallet.get_transaction(hash).await?;
        match (&served, row.block.is_empty()) {
            (Some((tx, block_id)), false) => {
                ensure!(block_id.to_string() == row.block, "{}: the sequencer has it in block {block_id}", row.step);
                ensure!(tx.hash().to_string() == row.tx_hash, "{}: another transaction", row.step);
                let _: &LeeTransaction = tx;
            }
            (None, true) => {}
            (Some((_, block_id)), true) => bail!("{}: recorded as not included, the sequencer has it in block {block_id}", row.step),
            (None, false) => bail!("{}: the sequencer does not know it", row.step),
        }
        checked += 1;
    }
    println!(
        "sequencer at block {head}: serves the {} recorded blocks with the recorded hashes; {checked} manifest transactions are where the manifest says",
        recorded.blocks.len()
    );
    Ok(())
}
