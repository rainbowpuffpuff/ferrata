//! The checkpoint programs' Mantle transaction parsers (`checkpoint_core::mantle`): the opcode
//! walk, the elided inscription (`split`, then `check`: channel, payload length, parent, signer,
//! BLAKE2b transaction hash, Ed25519 signature) and the channel configuration op. Input: a
//! selector byte, an op index, then the transaction's ops bytes.
//!
//! Properties: no panic (overflow checks on); a split inscription is a prefix and a suffix of the
//! ops around the payload; re-signed by a known key (its signer field replaced, the transaction
//! hash signed), `check` accepts it exactly when the whole ops walk is well-formed, and then
//! returns the hash of the whole transaction (the same hash `tx_hash` computes on the unelided
//! bytes), that signer and the parent written in the op; an accepted configuration op sits in a
//! well-formed transaction at its index, with the hash of the whole transaction.
#![no_main]

use checkpoint_core::mantle::{opcodes, tx_hash, ConfigChange, ElidedInscription, OP_CHANNEL_CONFIG};
use ed25519_dalek::{Signer, SigningKey};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let [selector, index, ops @ ..] = data else { return };
    let index = usize::from(*index);
    match selector % 3 {
        0 => {
            let _ = opcodes(ops);
        }
        1 => {
            let Ok(e) = ElidedInscription::split(ops, index, &[0; 64]) else { return };
            let (p, s) = (e.prefix.len(), e.suffix.len());
            assert!(p >= 37 && p + s <= ops.len(), "split: the parts overlap");
            assert_eq!(&ops[..p], e.prefix);
            assert_eq!(&ops[ops.len() - s..], e.suffix);
            if s < 64 {
                return;
            }
            let key = SigningKey::from_bytes(&[7; 32]);
            let signer = key.verifying_key().to_bytes();
            let mut signed = ops.to_vec();
            let at = ops.len() - s;
            signed[at + 32..at + 64].copy_from_slice(&signer);
            let hash = tx_hash(&signed);
            let signature = key.sign(&hash).to_bytes();
            let e = ElidedInscription::split(&signed, index, &signature).expect("the same structure splits");
            let channel: [u8; 32] = signed[p - 36..p - 4].try_into().unwrap();
            let payload = &signed[p..signed.len() - s];
            match e.check(payload, &channel) {
                Ok(c) => {
                    assert_eq!(c.tx_hash, hash, "check hashes another transaction");
                    assert_eq!(c.signer, signer);
                    assert_eq!(&c.parent[..], &signed[at..at + 32]);
                    assert!(opcodes(&signed).is_ok(), "check accepts a malformed transaction");
                }
                Err(err) => {
                    assert!(opcodes(&signed).is_err(), "check refuses a well-formed signed inscription: {err:?}");
                }
            }
        }
        _ => {
            let Ok(op_index) = u8::try_from(index) else { return };
            let change = ConfigChange { ops, op_index, signatures: Vec::new() };
            if let Ok((op, hash)) = change.op() {
                assert_eq!(hash, tx_hash(ops));
                assert!(!op.keys.is_empty());
                let codes = opcodes(ops).expect("an accepted configuration op is in a well-formed transaction");
                assert_eq!(codes.get(index), Some(&OP_CHANNEL_CONFIG));
            }
        }
    }
});
