//! Test support: the lab node's real answers (vectors/lab-node), and the pieces a test needs to put
//! more inscriptions into them the way a turn holder would: LEZ blocks derived from a real block (id,
//! parent and timestamp changed, the hash recomputed), Mantle transactions that inscribe them (with the
//! operations LEZ's sequencer puts next to an inscription), signed by a channel key, in the JSON shape a
//! Logos Blockchain node serves (the shape of the recorded lab answers).
//!
//! The canonical bytes of each transaction are written here from `checkpoint-core::mantle`'s format
//! description, independently of the attestor's encoder; the transaction hash and the signature are over
//! these bytes, so the attestor accepts a transaction only if its encoder reproduces them.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use checkpoint_core::hash::sha256_parts;
use checkpoint_core::lez::hash_parsed_block;
use checkpoint_core::mantle::{
    inscription_msg_id, tx_hash, OP_CHANNEL_INSCRIBE, OP_CHANNEL_TRANSFER, OP_CHANNEL_WITHDRAW, OP_TRANSFER,
};
use checkpoint_core::Hash;
use ed25519_dalek::{Signer as _, SigningKey};
use ferrata_attestor::{final_inscriptions, hexa, NodeAnswers};
use serde_json::{json, Value};

pub const CHANNEL: &str = "0101010101010101010101010101010101010101010101010101010101010101";
pub const LAB_KEY: &str = "3b14e3980d05dd3651ff5956758dd4cfecc33b837775c9e3d90279d34d3fb0a2";

pub fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn h(s: &str) -> Hash {
    hexa(&json!(s), "hex").unwrap()
}

pub fn channel() -> Hash {
    h(CHANNEL)
}

pub fn key(label: &str) -> SigningKey {
    SigningKey::from_bytes(&sha256_parts(&[b"ferrata/attestor/devnet-key/", label.as_bytes()]))
}

pub fn pk(k: &SigningKey) -> Hash {
    k.verifying_key().to_bytes()
}

pub fn lab_answers() -> NodeAnswers {
    NodeAnswers::load(&dir().join("vectors/lab-node")).unwrap()
}

/// The stand-in key set of the t-of-n tests: the lab's key (producer of the real blocks), k1, k2.
pub fn n3_keys() -> Vec<Hash> {
    vec![h(LAB_KEY), pk(&key("channel-1")), pk(&key("channel-2"))]
}

/// The lab answers with the channel accrediting `keys`.
pub fn answers_with_keys(keys: &[Hash]) -> NodeAnswers {
    let mut a = lab_answers();
    a.channel["accredited_keys"] = json!(keys.iter().map(hex::encode).collect::<Vec<_>>());
    a
}

/// The real inscribed bytes of lab block `id` (186..=202).
pub fn payload_of(a: &NodeAnswers, id: u64) -> Vec<u8> {
    final_inscriptions(a, &channel()).unwrap()[&id].payload().unwrap().to_vec()
}

/// The Bedrock slot of the node block that holds lab block `id`'s inscription.
pub fn slot_of(a: &NodeAnswers, id: u64) -> u64 {
    final_inscriptions(a, &channel()).unwrap()[&id].slot
}

/// The message id of lab block `id`'s inscription.
pub fn msg_of(a: &NodeAnswers, id: u64) -> Hash {
    let ins = final_inscriptions(a, &channel()).unwrap();
    let i = &ins[&id];
    let c = i.check(&channel()).unwrap();
    inscription_msg_id(&channel(), i.payload().unwrap(), &c.parent, &c.signer)
}

/// The hash a LEZ block carries (bytes 40..72).
pub fn carried_hash(b: &[u8]) -> Hash {
    b[40..72].try_into().unwrap()
}

/// A LEZ block made from `template`: id, parent hash and timestamp (+ `ts_delta` ms) replaced, the hash
/// recomputed over the result and written into the header. It parses and re-hashes like a real block.
pub fn derive_block(template: &[u8], id: u64, prev: &Hash, ts_delta: u64) -> Vec<u8> {
    let mut b = template.to_vec();
    b[0..8].copy_from_slice(&id.to_le_bytes());
    b[8..40].copy_from_slice(prev);
    let ts = u64::from_le_bytes(b[72..80].try_into().unwrap()) + ts_delta;
    b[72..80].copy_from_slice(&ts.to_le_bytes());
    let hash = hash_parsed_block(&b).unwrap();
    b[40..72].copy_from_slice(&hash);
    b
}

/// The same block with another `bedrock_status` byte (outside the hash).
pub fn with_status(b: &[u8], status: u8) -> Vec<u8> {
    let mut b = b.to_vec();
    *b.last_mut().unwrap() = status;
    b
}

/// One operation of a transaction: its node JSON, its canonical bytes (opcode first) and its proof JSON.
#[derive(Clone)]
pub struct Op {
    pub json: Value,
    pub bytes: Vec<u8>,
    pub proof: Value,
}

fn zk_proof() -> Value {
    json!({"ZkSig": {"pi_a": "00".repeat(32), "pi_b": "00".repeat(64), "pi_c": "00".repeat(32)}})
}

/// The fee `Transfer` LEZ's node wallet appends: one input note, one output note.
pub fn fee_transfer(salt: u8) -> Op {
    let note = [salt; 32];
    let out_pk = [salt ^ 0x5a; 32];
    let value = 1000u64 + u64::from(salt);
    let mut bytes = vec![OP_TRANSFER, 1];
    bytes.extend(note);
    bytes.push(1);
    bytes.extend(value.to_le_bytes());
    bytes.extend(out_pk);
    Op {
        json: json!({"opcode": OP_TRANSFER, "payload": {"inputs": [hex::encode(note)],
            "outputs": [{"value": value, "pk": hex::encode(out_pk)}]}}),
        bytes,
        proof: zk_proof(),
    }
}

/// `ChannelWithdrawOp` (0x13) = `channel ‖ u8 n ‖ NoteId·n` (upstream `ChannelWithdrawOp { channel_id, inputs }`).
pub fn channel_withdraw(notes: &[[u8; 32]]) -> Op {
    let mut bytes = vec![OP_CHANNEL_WITHDRAW];
    bytes.extend(channel());
    bytes.push(u8::try_from(notes.len()).unwrap());
    for n in notes {
        bytes.extend(n);
    }
    Op {
        json: json!({"opcode": OP_CHANNEL_WITHDRAW, "payload": {"channel_id": CHANNEL,
            "inputs": notes.iter().map(hex::encode).collect::<Vec<_>>()}}),
        bytes,
        proof: json!({"Ed25519Sig": "00".repeat(64)}),
    }
}

/// `ChannelTransferOp` (0x14) = `channel ‖ u8 n ‖ NoteId·n ‖ u8 m ‖ (value u64 ‖ pk)·m`.
pub fn channel_transfer(notes: &[[u8; 32]], outputs: &[(u64, [u8; 32])]) -> Op {
    let mut bytes = vec![OP_CHANNEL_TRANSFER];
    bytes.extend(channel());
    bytes.push(u8::try_from(notes.len()).unwrap());
    for n in notes {
        bytes.extend(n);
    }
    bytes.push(u8::try_from(outputs.len()).unwrap());
    for (v, p) in outputs {
        bytes.extend(v.to_le_bytes());
        bytes.extend(p);
    }
    Op {
        json: json!({"opcode": OP_CHANNEL_TRANSFER, "payload": {"channel_id": CHANNEL,
            "inputs": notes.iter().map(hex::encode).collect::<Vec<_>>(),
            "outputs": outputs.iter().map(|(v, p)| json!({"value": v, "pk": hex::encode(p)})).collect::<Vec<_>>()}}),
        bytes,
        proof: json!({"Ed25519Sig": "00".repeat(64)}),
    }
}

/// An inscription of `payload` on the lab channel chained on `parent` by `signer`: its op, unsigned.
pub fn inscribe_op(signer: &SigningKey, payload: &[u8], parent: &Hash) -> Op {
    let mut bytes = vec![OP_CHANNEL_INSCRIBE];
    bytes.extend(channel());
    bytes.extend(u32::try_from(payload.len()).unwrap().to_le_bytes());
    bytes.extend(payload);
    bytes.extend(parent);
    bytes.extend(pk(signer));
    Op {
        json: json!({"opcode": OP_CHANNEL_INSCRIBE, "payload": {"channel_id": CHANNEL,
            "inscription": hex::encode(payload), "parent": hex::encode(parent), "signer": hex::encode(pk(signer))}}),
        bytes,
        proof: Value::Null,
    }
}

/// A signed transaction of `ops` as the node serves it; the op at `inscription` gets `signer`'s
/// Ed25519 signature over the transaction hash. Returns the JSON, the encoded bytes and the hash.
pub fn tx(signer: &SigningKey, ops: &[Op], inscription: usize) -> (Value, Vec<u8>, Hash) {
    let mut bytes = vec![u8::try_from(ops.len()).unwrap()];
    for o in ops {
        bytes.extend(&o.bytes);
    }
    let hash = tx_hash(&bytes);
    let sig = signer.sign(&hash).to_bytes();
    let proofs: Vec<Value> = ops
        .iter()
        .enumerate()
        .map(|(i, o)| if i == inscription { json!({"Ed25519Sig": hex::encode(sig)}) } else { o.proof.clone() })
        .collect();
    let v = json!({"mantle_tx": {"hash": hex::encode(hash), "ops": ops.iter().map(|o| o.json.clone()).collect::<Vec<_>>()},
                   "ops_proofs": proofs});
    (v, bytes, hash)
}

/// The usual LEZ shape: `[inscription, fee transfer]`, signed by `signer`. Returns the JSON and the message id.
pub fn inscription_tx(signer: &SigningKey, payload: &[u8], parent: &Hash, salt: u8) -> (Value, Hash) {
    let (v, _, _) = tx(signer, &[inscribe_op(signer, payload, parent), fee_transfer(salt)], 0);
    (v, inscription_msg_id(&channel(), payload, parent, &pk(signer)))
}

/// A node block (header id from `label`, at `slot`) holding `txs`.
pub fn node_block(label: &str, slot: u64, txs: Vec<Value>) -> Value {
    json!({"header": {"id": hex::encode(sha256_parts(&[b"test node block/", label.as_bytes()])), "slot": slot},
           "transactions": txs})
}

/// A LEZ node that holds `blocks` (each served with `status`), nothing else.
pub fn lez_holding(blocks: &[&[u8]], status: u8) -> BTreeMap<u64, Option<Vec<u8>>> {
    blocks.iter().map(|b| (u64::from_le_bytes(b[0..8].try_into().unwrap()), Some(with_status(b, status)))).collect()
}

/// Bedrock's `Finalized` status byte.
pub const FINALIZED: u8 = 2;
pub const PENDING: u8 = 0;
