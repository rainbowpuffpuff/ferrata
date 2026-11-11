//! The checkpoint programs' LEZ block parser (`checkpoint_core::lez::parse_block`), differential
//! against LEZ v0.4.0-rc1's own decoder (`borsh::from_slice::<common::block::Block>`).
//!
//! Properties, on arbitrary bytes:
//! - the parser accepts exactly the blocks LEZ decodes, except for its two documented
//!   differences: it also requires `reverted` to be strictly ascending and to name existing
//!   transactions (no block LEZ applies fails this), and it takes BIP-340 public keys as opaque
//!   bytes where LEZ validates the point on decode (`lee::PublicKey`);
//! - when both accept: the same header, transaction count, reverted list and status; each
//!   transaction's reported span is exactly LEZ's encoding of that transaction, of the same kind;
//!   `hash_parsed_block` is LEZ's `HashableBlockData::compute_hash`.
#![no_main]

use checkpoint_core::lez::{hash_parsed_block, parse_block, Origin, TxKind, Visitor};
use common::block::{BedrockStatus, Block, HashableBlockData};
use common::transaction::LeeTransaction;
use libfuzzer_sys::fuzz_target;

#[derive(Default)]
struct Spans<'a> {
    txs: Vec<(u32, TxKind, &'a [u8])>,
    calls: usize,
}

impl<'a> Visitor<'a> for Spans<'a> {
    fn tx(&mut self, index: u32, kind: TxKind, bytes: &'a [u8]) {
        self.txs.push((index, kind, bytes));
    }
    fn call(
        &mut self,
        _index: u32,
        _origin: Origin,
        _from: Option<checkpoint_core::lez::Actor<'a>>,
        _to: checkpoint_core::lez::Actor<'a>,
        _message: &'a [u8],
    ) {
        self.calls += 1;
    }
}

/// Whether LEZ refused the bytes only because a BIP-340 key is not a curve point.
fn invalid_key_only(e: &std::io::Error) -> bool {
    e.get_ref()
        .and_then(|inner| inner.downcast_ref::<lee::error::LeeError>())
        .is_some_and(|inner| matches!(inner, lee::error::LeeError::InvalidPublicKey(_)))
}

fuzz_target!(|data: &[u8]| {
    let mut spans = Spans::default();
    let ours = parse_block(data, &mut spans);
    let theirs = borsh::from_slice::<Block>(data);
    match (ours, theirs) {
        (Ok(summary), Ok(block)) => {
            assert_eq!(summary.header.block_id, block.header.block_id);
            assert_eq!(summary.header.prev_block_hash, block.header.prev_block_hash.0);
            assert_eq!(summary.header.hash, block.header.hash.0);
            assert_eq!(summary.header.timestamp, block.header.timestamp);
            assert_eq!(summary.tx_count as usize, block.body.transactions.len());
            assert_eq!(summary.reverted, block.body.reverted);
            let status = match block.bedrock_status {
                BedrockStatus::Pending => 0,
                BedrockStatus::Safe => 1,
                BedrockStatus::Finalized => 2,
            };
            assert_eq!(summary.status, status);
            assert_eq!(spans.txs.len(), block.body.transactions.len());
            for ((index, kind, bytes), (i, tx)) in spans.txs.iter().zip(block.body.transactions.iter().enumerate()) {
                assert_eq!(*index as usize, i);
                let lez_kind = match tx {
                    LeeTransaction::Public(_) => TxKind::Public,
                    LeeTransaction::PrivacyPreserving(_) => TxKind::PrivacyPreserving,
                };
                assert_eq!(*kind, lez_kind);
                assert_eq!(*bytes, &borsh::to_vec(tx).unwrap()[..], "transaction {i}: another span");
            }
            let hash = hash_parsed_block(data).expect("an accepted block hashes");
            assert_eq!(hash, HashableBlockData::from(block).compute_hash().0, "another block hash");
        }
        (Err(e), Ok(block)) => {
            let n = block.body.transactions.len() as u64;
            let r = &block.body.reverted;
            let documented = r.windows(2).any(|w| w[0] >= w[1]) || r.iter().any(|&p| p >= n);
            assert!(documented, "the parser refuses a block LEZ decodes: {e:?}");
        }
        (Ok(summary), Err(e)) => {
            assert!(invalid_key_only(&e), "the parser accepts a block LEZ refuses ({e}): {summary:?}");
        }
        (Err(_), Err(_)) => {}
    }
});
