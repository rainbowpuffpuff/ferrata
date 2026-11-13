//! Logos Blockchain 0.3.5-rc.1 (`f6d4267f`, the revision LEZ v0.4.0-rc3 pins: logos-blockchain #3954) writes a
//! 32-byte fork digest in front of a transaction's operations, and the transaction hash every op proof signs covers it:
//! `BLAKE2b-256("MANTLE_TXHASH_V1" ‖ fork_digest ‖ ops)` (`core/src/mantle/transactions/tx_list/op_refs.rs`,
//! `Hashable for OpRefs`). This crate, and so the checkpoint guests, read the layout of `d480efc6` (the revision LEZ
//! v0.4.0-rc1 and rc2 pin): the operations alone (`src/mantle.rs`).
//!
//! The two transactions below are upstream's own codec fixtures of a transfer and an inscription
//! (`core/src/mantle/fixtures/ops/op_values.rs`, `TRANSFER_AND_INSCRIPTION_COLUMN_HEX`, the same bytes at both
//! revisions, and `FORK_DIGEST_HEX` = 0x11 x 32 at `f6d4267f`; `core/src/mantle/fixtures/transactions/tx_list/ops.rs`:
//! `Ops::from([..])` encodes to the column at `d480efc6`, `Ops::new(FORK_DIGEST, [..])` to the digest and the column at
//! `f6d4267f`). The tests record what this tree does with each: it frames the `d480efc6` transaction and finds its
//! inscription; it refuses the 0.3.5-rc.1 one; and the bytes without the digest, which it would frame, hash to another
//! value than the one a 0.3.5-rc.1 signer signs. So a checkpoint over a zone whose Bedrock runs 0.3.5-rc.1 needs the
//! guests changed (`lez/results/v0.4.0-rc3/README.md`).

use checkpoint_core::mantle::{opcodes, tx_hash, ElidedInscription, OP_CHANNEL_INSCRIBE, OP_TRANSFER};

/// `TRANSFER_AND_INSCRIPTION_COLUMN_HEX` (upstream fixture, identical at `d480efc6` and `f6d4267f`).
const COLUMN_HEX: &str = concat!(
    "020001010000000000000000000000000000000000000000000000000000000000000001020000000000000003000000",
    "000000000000000000000000000000000000000000000000000000001120202020202020202020202020202020202020",
    "202020202020202020202020200700000066697874757265212121212121212121212121212121212121212121212121",
    "2121212121212121a09aa5f47a6759802ff955f8dc2d2a14a5c99d23be97f864127ff9383455a4f0",
);

/// `FORK_DIGEST_HEX` at `f6d4267f`.
const FORK_DIGEST: [u8; 32] = [0x11; 32];

fn d480efc6_tx() -> Vec<u8> {
    hex::decode(COLUMN_HEX).unwrap()
}

fn rc_0_3_5_tx() -> Vec<u8> {
    let mut tx = FORK_DIGEST.to_vec();
    tx.extend(d480efc6_tx());
    tx
}

#[test]
fn the_d480efc6_transaction_frames_and_its_inscription_is_found() {
    let tx = d480efc6_tx();
    assert_eq!(tx.len(), 184);
    assert_eq!(opcodes(&tx).unwrap(), vec![OP_TRANSFER, OP_CHANNEL_INSCRIBE]);
    assert!(ElidedInscription::split(&tx, 1, &[0; 64]).is_ok());
}

#[test]
fn the_0_3_5_rc1_transaction_is_refused() {
    let tx = rc_0_3_5_tx();
    assert_eq!(tx.len(), 32 + 184);
    // The digest's first byte is read as the operation count (17), the next as an inscription's opcode.
    assert!(opcodes(&tx).is_err());
    assert!(ElidedInscription::split(&tx, 1, &[0; 64]).is_err());
}

#[test]
fn without_its_digest_the_0_3_5_rc1_transaction_hashes_to_another_value_than_the_one_signed() {
    // What a 0.3.5-rc.1 signer signs (`OpRefs::as_signing`): the prefix, the digest, the operations.
    let signed = tx_hash(&rc_0_3_5_tx());
    // What this crate computes for the bytes it can frame.
    let framed = tx_hash(&d480efc6_tx());
    assert_ne!(signed, framed);
}
