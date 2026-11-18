//! Logos Blockchain 0.3.5-rc.1 (`f6d4267f`, the revision LEZ v0.4.0-rc3 pins: logos-blockchain #3954) writes a
//! 32-byte fork digest in front of a transaction's operations, and the transaction hash every op proof signs covers it:
//! `BLAKE2b-256("MANTLE_TXHASH_V1" ‖ fork_digest ‖ ops)` (`core/src/mantle/transactions/tx_list/op_refs.rs`,
//! `Hashable for OpRefs`). The default build of this crate, and so the deployed checkpoint guests, read the layout of
//! `d480efc6` (the revision LEZ v0.4.0-rc1 and rc2 pin): the operations alone (`src/mantle.rs`). The cargo feature
//! `bedrock-0-3-5` adds the second build, `src/bedrock_0_3_5/`, which the guests of `guests-0-3-5/` use.
//!
//! The two transactions below are upstream's own codec fixtures of a transfer and an inscription
//! (`core/src/mantle/fixtures/ops/op_values.rs`, `TRANSFER_AND_INSCRIPTION_COLUMN_HEX`, the same bytes at both
//! revisions, and `FORK_DIGEST_HEX` = 0x11 x 32 at `f6d4267f`; `core/src/mantle/fixtures/transactions/tx_list/ops.rs`:
//! `Ops::from([..])` encodes to the column at `d480efc6`, `Ops::new(FORK_DIGEST, [..])` to the digest and the column at
//! `f6d4267f`). The first three tests record what the default build does with each: it frames the `d480efc6`
//! transaction and finds its inscription; it refuses the 0.3.5-rc.1 one; and the bytes without the digest, which it
//! would frame, hash to another value than the one a 0.3.5-rc.1 signer signs. They hold with the feature on as well:
//! the feature adds a module and changes nothing of the default build. The tests in `new_build` (feature
//! `bedrock-0-3-5`) are their counterparts for the second build: it frames the 0.3.5-rc.1 transaction and finds its
//! inscription, hashes what a 0.3.5-rc.1 signer signs, refuses another fork digest, refuses the `d480efc6` layout, and
//! decodes upstream's channel configuration fixtures as each revision does (logos-blockchain #3781).

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

#[cfg(feature = "bedrock-0-3-5")]
mod new_build {
    use super::{d480efc6_tx, rc_0_3_5_tx, COLUMN_HEX, FORK_DIGEST};
    use blake2::{digest::consts::U32, Blake2b, Digest as _};
    use checkpoint_core::bedrock_0_3_5::mantle::{check, config_op, fork_digest_of, opcodes, split};
    use checkpoint_core::bedrock_0_3_5::Error;
    use checkpoint_core::mantle::{ConfigChange, ElidedInscription, OP_CHANNEL_INSCRIBE, OP_TRANSFER};
    use ed25519_dalek::{Signer as _, SigningKey};

    /// The fixture inscription's channel (`INSCRIPTION`: `ChannelId::from([0x20; 32])`).
    const CHANNEL: [u8; 32] = [0x20; 32];

    /// `CHANNEL_CONFIG_HEX` at `f6d4267f`: two keys, `configuration_threshold` 2, `transfer_threshold` 0x16.
    const CHANNEL_CONFIG_F6D4267F: &str = concat!(
        "1010101010101010101010101010101010101010101010101010101010101010100000000000000000000000000000000000000000000000",
        "0000000000000000000200d04ab232742bb4ab3a1368bd4615e4e6d0224ab71a016baf8520a332c9778737204040e364c10f2bec9c1fe500",
        "a1cd4c247c89d650a01ed7e82caba867877c21130000001400000002001600",
    );
    /// `CHANNEL_CONFIG_HEX` at `d480efc6`: the same, with `configuration_threshold` 0x15 = 21 over the two keys.
    const CHANNEL_CONFIG_D480EFC6: &str = concat!(
        "1010101010101010101010101010101010101010101010101010101010101010100000000000000000000000000000000000000000000000",
        "0000000000000000000200d04ab232742bb4ab3a1368bd4615e4e6d0224ab71a016baf8520a332c9778737204040e364c10f2bec9c1fe500",
        "a1cd4c247c89d650a01ed7e82caba867877c21130000001400000015001600",
    );

    /// What a 0.3.5-rc.1 signer signs (`OpRefs::as_signing`, hashed by `tx_hasher`), written out with the `blake2`
    /// crate directly: `BLAKE2b-256("MANTLE_TXHASH_V1" ‖ fork_digest ‖ ops)`.
    fn signed_bytes(fork_digest: &[u8; 32], ops: &[u8]) -> [u8; 32] {
        let mut h = Blake2b::<U32>::new();
        h.update(b"MANTLE_TXHASH_V1");
        h.update(fork_digest);
        h.update(ops);
        h.finalize().into()
    }

    /// The fixture's 0.3.5-rc.1 transaction with its inscription's signer (the last 32 bytes; the fixture's key has no
    /// known secret) replaced by `key`'s public key, and `key`'s signature over the 0.3.5-rc.1 transaction hash.
    fn signed_0_3_5(key: &SigningKey) -> (Vec<u8>, [u8; 64]) {
        let mut tx = rc_0_3_5_tx();
        let n = tx.len();
        tx[n - 32..].copy_from_slice(key.verifying_key().as_bytes());
        let sig = key.sign(&signed_bytes(&FORK_DIGEST, &tx[32..])).to_bytes();
        (tx, sig)
    }

    fn payload<'a>(tx: &'a [u8], e: &ElidedInscription<'_>) -> &'a [u8] {
        &tx[e.prefix.len()..tx.len() - e.suffix.len()]
    }

    #[test]
    fn the_new_build_frames_the_0_3_5_rc1_transaction_and_finds_its_inscription() {
        let tx = rc_0_3_5_tx();
        assert_eq!(fork_digest_of(&tx).unwrap(), &FORK_DIGEST);
        assert_eq!(opcodes(&tx).unwrap(), vec![OP_TRANSFER, OP_CHANNEL_INSCRIBE]);
        let e = split(&tx, 1, &[0; 64]).unwrap();
        assert_eq!(&e.prefix[..32], &FORK_DIGEST);
        assert_eq!(payload(&tx, &e), b"fixture");
        // The transfer is op 0, not an inscription.
        assert_eq!(split(&tx, 0, &[0; 64]).unwrap_err(), Error::Core(checkpoint_core::Error::NotAnInscription));
    }

    #[test]
    fn the_new_build_hashes_what_a_0_3_5_rc1_signer_signs() {
        let tx = rc_0_3_5_tx();
        let column = hex::decode(COLUMN_HEX).unwrap();
        // The crate's hash of the whole 0.3.5-rc.1 transaction is the signed bytes' hash, not the column's alone.
        assert_eq!(checkpoint_core::mantle::tx_hash(&tx), signed_bytes(&FORK_DIGEST, &column));
        assert_ne!(checkpoint_core::mantle::tx_hash(&column), signed_bytes(&FORK_DIGEST, &column));
        // A signer that signs that hash is accepted, and the check returns that hash.
        let key = SigningKey::from_bytes(&[0x42; 32]);
        let (tx, sig) = signed_0_3_5(&key);
        let e = split(&tx, 1, &sig).unwrap();
        let c = check(&e, payload(&tx, &e), &CHANNEL, &FORK_DIGEST).unwrap();
        assert_eq!(c.signer, key.verifying_key().to_bytes());
        assert_eq!(c.tx_hash, signed_bytes(&FORK_DIGEST, &tx[32..]));
        assert_eq!(c.parent, [0x21; 32]);
        // The default build cannot frame it; a signature over the column alone is refused by the new build.
        assert!(ElidedInscription::split(&tx, 1, &sig).is_err());
        let wrong = key.sign(&checkpoint_core::mantle::tx_hash(&tx[32..])).to_bytes();
        let e = split(&tx, 1, &wrong).unwrap();
        assert_eq!(
            check(&e, payload(&tx, &e), &CHANNEL, &FORK_DIGEST).unwrap_err(),
            Error::Core(checkpoint_core::Error::BadSignature)
        );
    }

    #[test]
    fn the_new_build_refuses_another_fork_digest() {
        let key = SigningKey::from_bytes(&[0x42; 32]);
        let (tx, sig) = signed_0_3_5(&key);
        let e = split(&tx, 1, &sig).unwrap();
        assert_eq!(check(&e, payload(&tx, &e), &CHANNEL, &[0x22; 32]).unwrap_err(), Error::WrongForkDigest);
        // A transaction carrying another digest, signed over it by the same key, is refused against the deployment's.
        let mut other = tx.clone();
        other[..32].copy_from_slice(&[0x22; 32]);
        let sig = key.sign(&signed_bytes(&[0x22; 32], &other[32..])).to_bytes();
        let e = split(&other, 1, &sig).unwrap();
        assert!(check(&e, payload(&other, &e), &CHANNEL, &[0x22; 32]).is_ok());
        assert_eq!(check(&e, payload(&other, &e), &CHANNEL, &FORK_DIGEST).unwrap_err(), Error::WrongForkDigest);
        // And a channel configuration change.
        let mut cfg = FORK_DIGEST.to_vec();
        cfg.push(1);
        cfg.extend(hex::decode(CHANNEL_CONFIG_F6D4267F).unwrap());
        let change = ConfigChange { ops: &cfg, op_index: 0, signatures: Vec::new() };
        assert!(config_op(&change, &FORK_DIGEST).is_ok());
        assert_eq!(config_op(&change, &[0x22; 32]).unwrap_err(), Error::WrongForkDigest);
    }

    #[test]
    fn the_new_build_refuses_the_d480efc6_layout() {
        let tx = d480efc6_tx();
        // Its first 32 bytes are read as the digest, the rest does not frame.
        assert!(opcodes(&tx).is_err());
        assert!(split(&tx, 1, &[0; 64]).is_err());
        // A d480efc6 inscription signed as that node's signer signs it, split as the default build splits it, is
        // refused by the new build's check: its first 32 bytes are not the deployment's digest.
        let key = SigningKey::from_bytes(&[0x42; 32]);
        let mut old = tx.clone();
        let n = old.len();
        old[n - 32..].copy_from_slice(key.verifying_key().as_bytes());
        let sig = key.sign(&checkpoint_core::mantle::tx_hash(&old)).to_bytes();
        let e = ElidedInscription::split(&old, 1, &sig).unwrap();
        assert!(e.check(payload(&old, &e), &CHANNEL).is_ok());
        assert_eq!(check(&e, payload(&old, &e), &CHANNEL, &FORK_DIGEST).unwrap_err(), Error::WrongForkDigest);
        // Even against a digest equal to its first 32 bytes, the rest is not a 0.3.5-rc.1 transaction.
        let first: [u8; 32] = old[..32].try_into().unwrap();
        assert!(check(&e, payload(&old, &e), &CHANNEL, &first).is_err());
    }

    #[test]
    fn upstream_channel_configurations_decode_as_each_revision_decodes_them() {
        // f6d4267f's fixture: both builds read it (each in its own layout).
        let f6 = hex::decode(CHANNEL_CONFIG_F6D4267F).unwrap();
        let mut new_tx = FORK_DIGEST.to_vec();
        new_tx.push(1);
        new_tx.extend(&f6);
        let (op, h) = config_op(&ConfigChange { ops: &new_tx, op_index: 0, signatures: Vec::new() }, &FORK_DIGEST).unwrap();
        assert_eq!((op.keys.len(), op.configuration_threshold, op.transfer_threshold), (2, 2, 0x16));
        assert_eq!((op.posting_timeframe, op.posting_timeout), (0x13, 0x14));
        assert_eq!(h, signed_bytes(&FORK_DIGEST, &new_tx[32..]));
        // d480efc6's fixture: configuration_threshold 21 over two keys. The default build (and the d480efc6 node)
        // read it; 0.3.5-rc.1 does not decode it (#3781), and neither does the new build.
        let d4 = hex::decode(CHANNEL_CONFIG_D480EFC6).unwrap();
        let mut old_tx = vec![1u8];
        old_tx.extend(&d4);
        let (op, _) = ConfigChange { ops: &old_tx, op_index: 0, signatures: Vec::new() }.op().unwrap();
        assert_eq!(op.configuration_threshold, 21);
        let mut new_tx = FORK_DIGEST.to_vec();
        new_tx.extend(&old_tx);
        assert_eq!(
            config_op(&ConfigChange { ops: &new_tx, op_index: 0, signatures: Vec::new() }, &FORK_DIGEST).unwrap_err(),
            Error::UnreachableThreshold { threshold: 21, keys: 2 }
        );
        // An inscription transaction holding such an op does not frame either.
        let mut with_op = FORK_DIGEST.to_vec();
        with_op.push(1);
        with_op.extend(&d4);
        assert_eq!(opcodes(&with_op).unwrap_err(), Error::UnreachableThreshold { threshold: 21, keys: 2 });
    }
}
