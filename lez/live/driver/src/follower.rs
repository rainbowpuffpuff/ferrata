//! A follower of the chain: LEZ's own block application over the blocks a sequencer serves.
//!
//! The sequencer's RPC has no receipts: a block lists the indices of its reverted transactions
//! and carries the gas totals of the block in its forced fee transaction, nothing per
//! transaction, and never the reason of a revert. So, as LEZ's indexer does, this replays every
//! block from the pre-genesis state (`testnet_initial_state::initial_state`) with
//! `chain_state::apply::apply_block` (hash, chain link, the reverted list and the fee
//! transaction are all checked there), and before that settles the block's user transactions
//! one by one on a scratch copy with `chain_state::apply::settle_transaction`, the function the
//! sequencer's block builder and `apply_block` both use, to read what each was charged. The
//! reason of a revert comes from `lee::ValidatedStateDiff::from_*_transaction_metered` on the
//! state the transaction ran against.

use std::collections::BTreeMap;

use anyhow::{anyhow, ensure, Result};
use borsh::{BorshDeserialize, BorshSerialize};
use chain_state::apply::{apply_block, opening_fee_state, settle_transaction, Tip};
use common::{block::Block, transaction::LeeTransaction, HashType};
use fee_core::BlockFeeSummary;
use lee::{V03State, ValidatedStateDiff, GENESIS_BLOCK_ID};
use serde::{Deserialize, Serialize};

/// What LEZ's settlement did with one user transaction.
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Receipt {
    pub tx_hash: String,
    pub block_id: u64,
    /// Index among the block's transactions.
    pub position: u64,
    pub timestamp_ms: u64,
    pub private: bool,
    pub reverted: bool,
    /// LEZ's message for a reverted action.
    pub reason: Option<String>,
    /// The program's exit code, if the action was refused with one.
    pub exit_code: Option<u32>,
    /// Execution gas charged (cycles, clamped to the gas limit; plus the flat verification gas
    /// of a private transaction).
    pub gas_exec: u64,
    /// Storage gas charged (the transaction's bytes; a flat constant for a private one).
    pub gas_stor: u64,
    /// Base fee charged, at the block's opening base fees.
    pub fee_base: String,
    pub base_fee_exec: u64,
    pub base_fee_stor: u64,
    pub gas_limit: Option<u64>,
    /// Size of the transaction as a borsh `LeeTransaction`.
    pub tx_bytes: u64,
    /// User transactions in the block, and the block's totals as its fee transaction states them.
    pub block_user_txs: u64,
    pub block_gas_exec: u64,
    pub block_gas_stor: u64,
    /// Events the action emitted.
    pub events: u64,
}

impl Receipt {
    pub fn status(&self) -> &'static str {
        match (self.reverted, self.exit_code) {
            (false, _) => "applied",
            (true, Some(_)) => "refused",
            (true, None) => "reverted",
        }
    }
}

/// The fee summary a block's forced fee transaction carries.
pub fn block_summary(block: &Block) -> Option<BlockFeeSummary> {
    let txs = &block.body.transactions;
    let fee_tx = txs.len().checked_sub(2).and_then(|i| txs.get(i))?;
    let LeeTransaction::Public(fee_tx) = fee_tx else { return None };
    match borsh::from_slice(&fee_tx.message().execution.root.message) {
        Ok(fee_core::Message::Distribute { summary, .. }) => Some(summary),
        _ => None,
    }
}

/// `Program exited with non-zero code {code} after {cycles} cycles` (lee's `LeeError`).
pub fn exit_code_of(reason: &str) -> Option<u32> {
    let rest = reason.strip_prefix("Program exited with non-zero code ")?;
    rest.split_whitespace().next()?.parse().ok()
}

/// One block as the follower applied it.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct BlockInfo {
    pub block_id: u64,
    pub hash: String,
    pub timestamp_ms: u64,
    pub transactions: u64,
    pub user_transactions: u64,
    pub reverted: Vec<u64>,
    /// The base fees the block opened with.
    pub base_fee_exec: u64,
    pub base_fee_stor: u64,
    /// The totals in the block's fee transaction (zero for genesis, which settles no fee).
    pub gas_exec: u64,
    pub gas_stor: u64,
    pub revenue_base: String,
}

pub struct Follower {
    pub state: V03State,
    pub tip: Option<Tip>,
    pub blocks: Vec<Block>,
    pub chain: Vec<BlockInfo>,
    pub receipts: BTreeMap<String, Receipt>,
}

#[derive(BorshSerialize, BorshDeserialize)]
struct Snapshot {
    state: V03State,
    tip: Option<(u64, [u8; 32])>,
    chain: Vec<BlockInfo>,
    receipts: Vec<Receipt>,
}

impl Follower {
    /// At the pre-genesis state of a zone without the cross-zone programs, as LEZ's
    /// `local.json` configures it.
    pub fn new() -> Self {
        Self {
            // LEZ's pre-genesis state; a zone with the cross-zone programs (`FERRATA_FOLLOWER_CROSS_ZONE=1`, for a
            // network whose `getProgramIds` lists them: `lez/testnet04/day1.sh discover`) starts from that one.
            state: testnet_initial_state::initial_state(std::env::var("FERRATA_FOLLOWER_CROSS_ZONE").as_deref() == Ok("1")),
            tip: None,
            blocks: Vec::new(),
            chain: Vec::new(),
            receipts: BTreeMap::new(),
        }
    }

    pub fn height(&self) -> u64 {
        self.tip.as_ref().map_or(0, |tip| tip.block_id)
    }

    /// The block after the tip: settle its user transactions one by one for their receipts,
    /// then apply it as LEZ does.
    pub fn apply(&mut self, block: Block) -> Result<Vec<Receipt>> {
        let block_id = block.header.block_id;
        let timestamp = block.header.timestamp;
        let txs = &block.body.transactions;
        ensure!(txs.len() >= 2, "block {block_id} has no fee and clock transactions");
        let user_txs = &txs[..txs.len() - 2];
        let mut receipts = Vec::new();
        let opening_fees = opening_fee_state(&self.state);
        let mut info = BlockInfo {
            block_id,
            hash: block.header.hash.to_string(),
            timestamp_ms: timestamp,
            transactions: txs.len() as u64,
            user_transactions: user_txs.len() as u64,
            reverted: block.body.reverted.clone(),
            base_fee_exec: opening_fees.base_fee_exec,
            base_fee_stor: opening_fees.base_fee_stor,
            gas_exec: 0,
            gas_stor: 0,
            revenue_base: "0".to_string(),
        };
        if block_id != GENESIS_BLOCK_ID {
            let stated = block_summary(&block).ok_or_else(|| anyhow!("block {block_id}: no fee transaction"))?;
            info.gas_exec = stated.gas_used_exec;
            info.gas_stor = stated.gas_used_stor;
            info.revenue_base = stated.revenue_base.to_string();
            let mut scratch = self.state.clone();
            let opening = opening_fee_state(&scratch);
            let mut summary = BlockFeeSummary::default();
            for (index, tx) in user_txs.iter().enumerate() {
                let before = summary;
                let pre = scratch.clone();
                let (events, reverted) =
                    settle_transaction(tx, &mut scratch, &opening, block_id, timestamp, index as u64, &mut summary)
                        .map_err(|e| anyhow!("block {block_id}, transaction {index}: {e}"))?;
                let tx_bytes = borsh::to_vec(tx)?.len() as u64;
                let (gas_limit, reason) = match tx {
                    LeeTransaction::Public(public_tx) => {
                        let gas_limit = public_tx.message().execution.fee.map(|fee| fee.gas_limit);
                        let reason = if reverted {
                            let budget = gas_limit.unwrap_or(fee_core::market::MAX_GAS_EXEC);
                            let (charge, _) = ValidatedStateDiff::from_public_transaction_metered(
                                public_tx, &pre, block_id, timestamp, budget,
                            );
                            charge.revert
                        } else {
                            None
                        };
                        (gas_limit, reason)
                    }
                    LeeTransaction::PrivacyPreserving(private_tx) => {
                        let reason = if reverted {
                            ValidatedStateDiff::from_privacy_preserving_transaction_metered(
                                private_tx,
                                &pre,
                                block_id,
                                timestamp,
                                fee_core::market::MAX_GAS_EXEC,
                            )
                            .ok()
                            .and_then(|(_, charge)| charge.revert)
                        } else {
                            None
                        };
                        (None, reason)
                    }
                };
                receipts.push(Receipt {
                    tx_hash: tx.hash().to_string(),
                    block_id,
                    position: index as u64,
                    timestamp_ms: timestamp,
                    private: matches!(tx, LeeTransaction::PrivacyPreserving(_)),
                    reverted,
                    exit_code: reason.as_deref().and_then(exit_code_of),
                    reason,
                    gas_exec: summary.gas_used_exec - before.gas_used_exec,
                    gas_stor: summary.gas_used_stor - before.gas_used_stor,
                    fee_base: (summary.revenue_base - before.revenue_base).to_string(),
                    base_fee_exec: opening.base_fee_exec,
                    base_fee_stor: opening.base_fee_stor,
                    gas_limit,
                    tx_bytes,
                    block_user_txs: user_txs.len() as u64,
                    block_gas_exec: stated.gas_used_exec,
                    block_gas_stor: stated.gas_used_stor,
                    events: events.len() as u64,
                });
            }
            ensure!(
                summary == stated,
                "block {block_id}: settling its transactions gives {summary:?}, its fee transaction says {stated:?}"
            );
        }
        // LEZ's own validation and application of the whole block.
        apply_block(self.tip.as_ref(), &block, &mut self.state).map_err(|e| anyhow!("block {block_id}: {e}"))?;
        self.tip = Some(Tip::from(&block));
        self.chain.push(info);
        for receipt in &receipts {
            self.receipts.insert(receipt.tx_hash.clone(), receipt.clone());
        }
        self.blocks.push(block);
        Ok(receipts)
    }

    pub fn receipt(&self, hash: &HashType) -> Option<&Receipt> {
        self.receipts.get(&hash.to_string())
    }

    /// The state and receipts, to resume in another process without replaying the chain.
    pub fn snapshot(&self) -> Result<Vec<u8>> {
        let snapshot = Snapshot {
            state: self.state.clone(),
            tip: self.tip.as_ref().map(|tip| (tip.block_id, tip.hash.0)),
            chain: self.chain.clone(),
            receipts: self.receipts.values().cloned().collect(),
        };
        Ok(borsh::to_vec(&snapshot)?)
    }

    pub fn restore(snapshot: &[u8], blocks: Vec<Block>) -> Result<Self> {
        let snapshot: Snapshot = borsh::from_slice(snapshot)?;
        let tip = snapshot.tip.map(|(block_id, hash)| Tip { block_id, hash: HashType(hash) });
        ensure!(
            blocks.last().map(|b| b.header.block_id) == tip.as_ref().map(|t| t.block_id),
            "the saved state and the saved blocks end at different heights"
        );
        Ok(Self {
            state: snapshot.state,
            tip,
            blocks,
            chain: snapshot.chain,
            receipts: snapshot.receipts.into_iter().map(|r| (r.tx_hash.clone(), r)).collect(),
        })
    }
}
