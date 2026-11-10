//! Hand-written encoders for the core's own tests (no upstream crates here; the fidelity crate
//! checks the same parsers against blocks and transactions built by LEZ and Logos Blockchain).
#![allow(dead_code)]

use checkpoint_core::hash::sha256_parts;
use checkpoint_core::lez::BLOCK_HASH_PREFIX;
use checkpoint_core::mantle::{tx_hash, ElidedInscription, OP_CHANNEL_INSCRIBE, OP_TRANSFER};
use checkpoint_core::Hash;
use ed25519_dalek::{Signer as _, SigningKey};

pub fn u32le(n: usize) -> [u8; 4] {
    u32::try_from(n).unwrap().to_le_bytes()
}

/// `borsh(LeeTransaction::Public(..))` with one actor, no signer, no fee: a root call to
/// `(account, program)` carrying `message`.
pub fn public_tx(account: &Hash, program: &Hash, message: &[u8]) -> Vec<u8> {
    let mut t = vec![0u8]; // LeeTransaction::Public
    t.extend_from_slice(&u32le(1)); // actors
    t.extend_from_slice(account);
    t.extend_from_slice(program);
    t.extend_from_slice(&u32le(0)); // authorized_accounts
    t.extend_from_slice(&u32le(0)); // promotions
    t.extend_from_slice(account); // root.to
    t.extend_from_slice(program);
    t.extend_from_slice(&u32le(message.len()));
    t.extend_from_slice(message);
    t.push(0); // fee: None
    t.extend_from_slice(&u32le(0)); // nonces
    t.extend_from_slice(&u32le(0)); // admission_evidence
    t.extend_from_slice(&u32le(0)); // witness set
    t
}

/// `borsh(LeeTransaction::PrivacyPreserving(..))` whose boundary holds one `PrivateToPublic`
/// delivery to `(account, program)` carrying `message`, with a `proof_len`-byte proof.
pub fn pp_tx(account: &Hash, program: &Hash, message: &[u8], proof_len: usize) -> Vec<u8> {
    let mut t = vec![1u8];
    t.extend_from_slice(&u32le(0)); // actors
    t.extend_from_slice(&u32le(0)); // authorized
    t.extend_from_slice(&u32le(0)); // promotions
    t.extend_from_slice(&u32le(1)); // boundary: one step
    t.push(0); // PrivateToPublic
    t.extend_from_slice(&[7u8; 64]); // from
    t.extend_from_slice(account); // to
    t.extend_from_slice(program);
    t.extend_from_slice(&u32le(message.len()));
    t.extend_from_slice(message);
    t.extend_from_slice(&u32le(0)); // inherited_authorizations
    t.push(0); // inherits_entry_authorizations
    t.extend_from_slice(&u32le(0)); // pda_seeds
    t.extend_from_slice(&u32le(0)); // async_messages
    t.extend_from_slice(&u32le(0)); // recovery_bindings
    t.push(0); // public_root: None
    t.extend_from_slice(&u32le(1)); // private_actions: one
    t.extend_from_slice(&[3u8; 96]);
    t.extend_from_slice(&u32le(5));
    t.extend_from_slice(&[4u8; 5]);
    t.extend_from_slice(&u32le(3));
    t.extend_from_slice(&[5u8; 3]);
    t.extend_from_slice(&[0, 0, 0, 0]); // validity: four None
    t.extend_from_slice(&u32le(0)); // program_image_claims
    t.push(0); // fee: None
    t.extend_from_slice(&u32le(0)); // nonces
    t.extend_from_slice(&u32le(0)); // admission
    t.extend_from_slice(&u32le(0)); // signatures
    t.extend_from_slice(&u32le(proof_len));
    t.extend(std::iter::repeat_n(0xAB, proof_len));
    t
}

/// `borsh(Block)` and its hash.
pub fn block(id: u64, prev: &Hash, timestamp: u64, txs: &[Vec<u8>], reverted: &[u64]) -> (Vec<u8>, Hash) {
    let mut body = Vec::new();
    body.extend_from_slice(&u32le(txs.len()));
    for tx in txs {
        body.extend_from_slice(tx);
    }
    body.extend_from_slice(&u32le(reverted.len()));
    for r in reverted {
        body.extend_from_slice(&r.to_le_bytes());
    }
    let hash = sha256_parts(&[BLOCK_HASH_PREFIX, &id.to_le_bytes(), prev, &timestamp.to_le_bytes(), &body]);
    let mut b = Vec::new();
    b.extend_from_slice(&id.to_le_bytes());
    b.extend_from_slice(prev);
    b.extend_from_slice(&hash);
    b.extend_from_slice(&timestamp.to_le_bytes());
    b.extend_from_slice(&body);
    b.push(0); // Pending
    (b, hash)
}

/// A Mantle transaction `[ChannelInscribe(channel, payload, parent, signer), Transfer(1 in, 1
/// out)]` signed by `key`; returns the encoded ops and the signature.
pub fn inscription_tx(key: &SigningKey, channel: &Hash, payload: &[u8], parent: &Hash) -> (Vec<u8>, [u8; 64]) {
    let mut ops = vec![2u8, OP_CHANNEL_INSCRIBE];
    ops.extend_from_slice(channel);
    ops.extend_from_slice(&u32le(payload.len()));
    ops.extend_from_slice(payload);
    ops.extend_from_slice(parent);
    ops.extend_from_slice(key.verifying_key().as_bytes());
    ops.push(OP_TRANSFER);
    ops.push(1);
    ops.extend_from_slice(&[9u8; 32]);
    ops.push(1);
    ops.extend_from_slice(&5u64.to_le_bytes());
    ops.extend_from_slice(&[8u8; 32]);
    let sig = key.sign(&tx_hash(&ops)).to_bytes();
    (ops, sig)
}

pub fn elide<'a>(ops: &'a [u8], sig: &'a [u8; 64]) -> ElidedInscription<'a> {
    ElidedInscription::split(ops, 0, sig).unwrap()
}

pub fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}
