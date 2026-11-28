//! LEZ v0.4.0's block layout for the tests that hand a block to checkpoint-core's v0.4.0 decoder
//! (`checkpoint_core::lez_0_4_0`, feature `lez-0-4-0`): one receipt per user transaction (LEZ #1107,
//! `common::block::TransactionReceipt`: `success`, then the fee receipt) in place of the release candidates' reverted
//! list, and the two system transactions a block ends with (the fee and clock transactions, which carry no receipt):
//! here stand-ins, root calls to an all-zero actor that the tests' collectors ignore, since the decoder requires
//! exactly one receipt per user transaction.

use harness::settle::Receipt;

/// The system transactions [`block`] appends after the user transactions.
pub const SYSTEM_TXS: usize = 2;

/// `borsh(Block)` at v0.4.0 with a receipt per user transaction that says only whether it applied (the positions in
/// `reverted` did not) and charges nothing: for the tests that read what the checkpoint collects, not the fees.
#[allow(dead_code)]
pub fn block(block_id: u64, timestamp_ms: u64, txs: &[&[u8]], reverted: &[u64]) -> Vec<u8> {
    let receipts: Vec<Receipt> = (0..txs.len() as u64)
        .map(|i| Receipt { success: !reverted.contains(&i), gas_exec: 0, gas_stor: 0, fee_base: 0, tip: 0, refund: 0 })
        .collect();
    block_with_receipts(block_id, timestamp_ms, txs, &receipts)
}

/// `borsh(Block)` at v0.4.0: `block_id`, the zero previous hash and hash (the harness does not chain blocks),
/// `timestamp_ms`, the user transactions `txs` and the two stand-ins, `receipts` (one per user transaction, as the
/// harness's settlement wrote them: `Charge::receipt`), and the status `Finalized`.
#[allow(dead_code)]
pub fn block_with_receipts(block_id: u64, timestamp_ms: u64, txs: &[&[u8]], receipts: &[Receipt]) -> Vec<u8> {
    assert_eq!(txs.len(), receipts.len(), "one receipt per user transaction");
    let mut b = Vec::new();
    b.extend_from_slice(&block_id.to_le_bytes());
    b.extend_from_slice(&[0u8; 32]);
    b.extend_from_slice(&[0u8; 32]);
    b.extend_from_slice(&timestamp_ms.to_le_bytes());
    b.extend_from_slice(&((txs.len() + SYSTEM_TXS) as u32).to_le_bytes());
    for t in txs {
        b.extend_from_slice(t);
    }
    for _ in 0..SYSTEM_TXS {
        b.extend_from_slice(&stand_in_system_transaction());
    }
    b.extend_from_slice(&(receipts.len() as u32).to_le_bytes());
    for r in receipts {
        b.push(u8::from(r.success));
        b.extend_from_slice(&r.gas_exec.to_le_bytes());
        b.extend_from_slice(&r.gas_stor.to_le_bytes());
        b.extend_from_slice(&r.fee_base.to_le_bytes());
        b.extend_from_slice(&r.tip.to_le_bytes());
        b.extend_from_slice(&r.refund.to_le_bytes());
    }
    b.push(2);
    b
}

/// A public transaction in v0.4.0's encoding with nothing in it: no actors, signers, promotions or private signers,
/// a root call to the all-zero actor with an empty message, no nonces, no fee, no evidence, no witness.
fn stand_in_system_transaction() -> Vec<u8> {
    let mut t = vec![0u8]; // LeeTransaction::Public
    t.extend_from_slice(&[0u8; 4 * 4]); // actors, authorized_accounts, promotions, private_signers: empty
    t.extend_from_slice(&[0u8; 64]); // root call: to
    t.extend_from_slice(&[0u8; 4]); // root call: message, empty
    t.extend_from_slice(&[0u8; 4]); // nonces: empty
    t.push(0); // fee: None
    t.extend_from_slice(&[0u8; 4]); // admission_evidence: empty
    t.extend_from_slice(&[0u8; 4]); // witness entries: empty
    t
}
