//! The second build's rules (feature `bedrock-0-3-5`, `src/bedrock_0_3_5/rule.rs`) against the default build's, on
//! the same hand-built blocks (a public and a private burn among them) and the same key rotation: over transactions in
//! the 0.3.5-rc.1 layout (the zone's fork digest in front, signed over it) rule (A) and rule (B) give, word for word,
//! the journals the default rules give over the same transactions in the `d480efc6` layout. The second build refuses
//! the `d480efc6` layout, another fork digest, and a rotation to an unreachable threshold (logos-blockchain #3781),
//! which the default build applies. Every input goes through its byte encoding first, as the guest receives it.
#![cfg(feature = "bedrock-0-3-5")]
mod common;

use attestor_core::{sign_certificate, Deployment, FinalTip, SignedAttestation};
use checkpoint_core::bedrock_0_3_5::{mantle as new_mantle, rule as new, Error};
use checkpoint_core::input::{CheckpointInput, CheckpointInputB, Support};
use checkpoint_core::journal::CheckpointJournal;
use checkpoint_core::mantle::{tx_hash, ConfigChange, ElidedInscription, OP_CHANNEL_CONFIG};
use checkpoint_core::rule::run_checkpoint;
use checkpoint_core::rule_b::run_checkpoint_b;
use checkpoint_core::state::{key_set_hash, Config, State};
use checkpoint_core::tree::Frontier;
use checkpoint_core::wire::{BridgeMessage, Burn};
use checkpoint_core::Hash;
use common::*;
use ed25519_dalek::{Signer as _, SigningKey};

/// The zone's fork digest: the configuration's `bedrockGenesisId` in the second build.
const DIGEST: Hash = [0xd1; 32];
const TIMEFRAME: u32 = 30;
/// `T` is block 4 (index 2 of blocks 2..=8).
const T: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Layout {
    D480efc6,
    V035,
}

impl Layout {
    /// The whole transaction for an operations column, and its hash (what its signers sign).
    fn tx(self, column: &[u8]) -> (Vec<u8>, Hash) {
        let tx = match self {
            Self::D480efc6 => column.to_vec(),
            Self::V035 => [DIGEST.as_slice(), column].concat(),
        };
        let h = tx_hash(&tx);
        (tx, h)
    }
    fn split<'a>(self, tx: &'a [u8], sig: &'a [u8; 64]) -> ElidedInscription<'a> {
        match self {
            Self::D480efc6 => ElidedInscription::split(tx, 0, sig).unwrap(),
            Self::V035 => new_mantle::split(tx, 0, sig).unwrap(),
        }
    }
}

fn uint256(x: u64) -> Hash {
    let mut w = [0u8; 32];
    w[24..].copy_from_slice(&x.to_be_bytes());
    w
}

fn config() -> Config {
    Config {
        bedrock_genesis_id: DIGEST,
        channel_id: [2; 32],
        bridge_header: [3; 32],
        bridge_image_id: [4; 32],
        evm_chain_id: uint256(11_155_111),
        vault: [5; 20],
        attestation_root_id: [6; 32],
        salt: [7; 32],
        n_min: 3,
        t_min: 2,
    }
}

fn pk(k: &SigningKey) -> Hash {
    k.verifying_key().to_bytes()
}
fn channel(i: u8) -> SigningKey {
    key(0x10 + i)
}
fn att(i: u8) -> SigningKey {
    key(0x80 + i)
}

fn parent() -> State {
    State {
        tip_id: 1,
        tip_hash: [9; 32],
        keys: (0..4).map(|i| pk(&channel(i))).collect(),
        posting_timeframe: TIMEFRAME,
        config_tip_hash: [8; 32],
        burns: Frontier::default(),
        imported_leaf_count: 0,
        sunset: false,
    }
}

fn burn(c: &Config, r: u8) -> Vec<u8> {
    BridgeMessage::Burn(Burn {
        config_id: c.config_id(),
        r: [r; 32],
        memo: [0; 20],
        mac: [0; 16],
        token: [0; 20],
        amount: uint256(1_000_000_000_000_000),
        c_out: [r ^ 0xff; 32],
    })
    .encode()
}

/// Blocks 2..=8 on the parent's tip: a public burn in block 3, a private one (a private-to-public delivery) in block 4.
fn chain(c: &Config) -> (Vec<Vec<u8>>, Vec<Hash>) {
    let mut prev = [9u8; 32];
    let (mut blocks, mut hashes) = (Vec::new(), Vec::new());
    for id in 2u64..=8 {
        let txs = match id {
            3 => vec![public_tx(&[0xaa; 32], &c.bridge_header, &burn(c, 1))],
            4 => vec![pp_tx(&[0xbb; 32], &c.bridge_header, &burn(c, 2), 64)],
            _ => Vec::new(),
        };
        let (b, h) = block(id, &prev, 1_000 + id, &txs, &[]);
        blocks.push(b);
        hashes.push(h);
        prev = h;
    }
    (blocks, hashes)
}

/// `[ChannelInscribe(block), Transfer]` by `key`, in `layout`.
fn inscription(layout: Layout, c: &Config, payload: &[u8], key: &SigningKey) -> (Vec<u8>, [u8; 64]) {
    let mut col = vec![2u8, checkpoint_core::mantle::OP_CHANNEL_INSCRIBE];
    col.extend_from_slice(&c.channel_id);
    col.extend_from_slice(&u32le(payload.len()));
    col.extend_from_slice(payload);
    col.extend_from_slice(&[6; 32]);
    col.extend_from_slice(key.verifying_key().as_bytes());
    col.push(checkpoint_core::mantle::OP_TRANSFER);
    col.push(1);
    col.extend_from_slice(&[9u8; 32]);
    col.push(1);
    col.extend_from_slice(&5u64.to_le_bytes());
    col.extend_from_slice(&[8u8; 32]);
    let (tx, h) = layout.tx(&col);
    let sig = key.sign(&h).to_bytes();
    (tx, sig)
}

type Rotation = (Vec<u8>, Vec<(u16, [u8; 64])>);

/// A `ChannelConfigOp` on the parent's configuration tip adding channel key 4, with `configuration_threshold`
/// `threshold`, signed by current keys 0, 1, 2 (t(4) = 3), in `layout`.
fn rotation(layout: Layout, c: &Config, threshold: u16) -> Rotation {
    let keys: Vec<Hash> = (0..5).map(|i| pk(&channel(i))).collect();
    let mut col = vec![1u8, OP_CHANNEL_CONFIG];
    col.extend_from_slice(&c.channel_id);
    col.extend_from_slice(&[8; 32]);
    col.extend_from_slice(&u16::try_from(keys.len()).unwrap().to_le_bytes());
    for k in &keys {
        col.extend_from_slice(k);
    }
    col.extend_from_slice(&TIMEFRAME.to_le_bytes());
    col.extend_from_slice(&60u32.to_le_bytes());
    col.extend_from_slice(&threshold.to_le_bytes());
    col.extend_from_slice(&3u16.to_le_bytes());
    let (tx, h) = layout.tx(&col);
    let sigs = (0..3).map(|i| (u16::from(i), channel(i).sign(&h).to_bytes())).collect();
    (tx, sigs)
}

fn changes(rotations: &[Rotation]) -> Vec<ConfigChange<'_>> {
    rotations
        .iter()
        .map(|(ops, sigs)| ConfigChange { ops, op_index: 0, signatures: sigs.iter().map(|(i, s)| (*i, s)).collect() })
        .collect()
}

/// Everything signed for one layout: `T`'s inscription by channel key 0, support inscriptions of blocks 5, 6, 7 (and
/// 8, once a rotation added key 4) by keys 1, 2, 3 (and 4) for rule (A), and the rotations.
struct Signed {
    producer: (Vec<u8>, [u8; 64]),
    support: Vec<(u32, Vec<u8>, [u8; 64])>,
    rotations: Vec<Rotation>,
}

fn signed(layout: Layout, c: &Config, blocks: &[Vec<u8>], rotations: Vec<Rotation>) -> Signed {
    let producer = inscription(layout, c, &blocks[T as usize], &channel(0));
    let supporters = if rotations.is_empty() { 3u8 } else { 4 };
    let support = (1u8..=supporters)
        .map(|k| {
            let index = T + u32::from(k);
            let (tx, sig) = inscription(layout, c, &blocks[index as usize], &channel(k));
            (index, tx, sig)
        })
        .collect();
    Signed { producer, support, rotations }
}

/// Rule (B)'s t(n) attestations, by attestors 0, 1, 3 (and 4) over blocks 4, 5, 6 (and 7), under the key set `keys`.
fn attestations(c: &Config, hashes: &[Hash], keys: &[Hash]) -> Vec<SignedAttestation> {
    let d = Deployment { evm_chain_id: c.evm_chain_id, vault: c.vault, lez_config_id: c.config_id(), channel_id: c.channel_id };
    let ksh = key_set_hash(keys, TIMEFRAME);
    let need = checkpoint_core::hash::threshold(u32::try_from(keys.len()).unwrap()) as usize;
    [(0u8, 2usize), (1, 3), (3, 4), (4, 5)][..need]
        .iter()
        .map(|&(i, index)| {
            let tip = FinalTip { tip_id: 2 + index as u64, tip_hash: hashes[index], final_msg_id: [0x5e; 32] };
            let (ch, a) = (channel(i), att(i));
            SignedAttestation::new(&a, pk(&ch), sign_certificate(&ch, &d, &pk(&a)), &d, &ksh, tip)
        })
        .collect()
}

fn input_a<'a>(c: &Config, blocks: &'a [Vec<u8>], s: &'a Signed, layout: Layout) -> CheckpointInput<'a> {
    CheckpointInput {
        config: c.clone(),
        parent: parent(),
        config_changes: changes(&s.rotations),
        blocks: blocks.iter().map(Vec::as_slice).collect(),
        t_index: T,
        echo_nonce: [0; 32],
        producer: layout.split(&s.producer.0, &s.producer.1),
        support: s
            .support
            .iter()
            .map(|(index, tx, sig)| Support { block_index: *index, inscription: layout.split(tx, sig) })
            .collect(),
        segments: Vec::new(),
    }
}

fn input_b<'a>(
    c: &Config,
    blocks: &'a [Vec<u8>],
    s: &'a Signed,
    layout: Layout,
    attestations: Vec<SignedAttestation>,
) -> CheckpointInputB<'a> {
    CheckpointInputB {
        config: c.clone(),
        parent: parent(),
        config_changes: changes(&s.rotations),
        blocks: blocks.iter().map(Vec::as_slice).collect(),
        t_index: T,
        producer: layout.split(&s.producer.0, &s.producer.1),
        attestations,
        segments: Vec::new(),
    }
}

/// Both rules of one build, each over its input as the guest receives it (encoded, decoded, re-encoded).
fn run(build: Layout, a: &CheckpointInput<'_>, b: &CheckpointInputB<'_>) -> (Result<CheckpointJournal, Error>, Result<CheckpointJournal, Error>) {
    let bytes_a = a.encode();
    let da = CheckpointInput::decode(&bytes_a).expect("input decodes");
    assert_eq!(da.encode(), bytes_a);
    let bytes_b = b.encode();
    let db = CheckpointInputB::decode(&bytes_b).expect("input decodes");
    assert_eq!(db.encode(), bytes_b);
    match build {
        Layout::D480efc6 => (
            run_checkpoint(&da).map(|o| o.journal).map_err(Error::Core),
            run_checkpoint_b(&db).map(|o| o.journal).map_err(Error::Core),
        ),
        Layout::V035 => {
            (new::run_checkpoint(&da).map(|o| o.journal), new::run_checkpoint_b(&db).map(|o| o.journal))
        }
    }
}

fn five_keys() -> Vec<Hash> {
    (0..5).map(|i| pk(&channel(i))).collect()
}

#[test]
fn both_rules_give_the_default_builds_journals_over_the_same_blocks() {
    let c = config();
    let (blocks, hashes) = chain(&c);
    let old = signed(Layout::D480efc6, &c, &blocks, vec![rotation(Layout::D480efc6, &c, 3)]);
    let new = signed(Layout::V035, &c, &blocks, vec![rotation(Layout::V035, &c, 3)]);
    let att = attestations(&c, &hashes, &five_keys());
    let (old_a, old_b) = run(
        Layout::D480efc6,
        &input_a(&c, &blocks, &old, Layout::D480efc6),
        &input_b(&c, &blocks, &old, Layout::D480efc6, att.clone()),
    );
    let (new_a, new_b) =
        run(Layout::V035, &input_a(&c, &blocks, &new, Layout::V035), &input_b(&c, &blocks, &new, Layout::V035, att));
    let (old_a, old_b, new_a, new_b) = (old_a.unwrap(), old_b.unwrap(), new_a.unwrap(), new_b.unwrap());
    assert_eq!(new_a, old_a);
    assert_eq!(new_b, old_b);
    // Rule (B)'s journal is rule (A)'s candidate journal; two burns; the five-key set after the rotation; T = block 4.
    assert_eq!(new_b, new_a);
    assert_eq!((new_b.burn_count, new_b.n, new_b.t, new_b.tip_id), (2, 5, 4, 4));
    assert_eq!(new_b.tip_hash, hashes[T as usize]);
    // In the 0.3.5-rc.1 layout the transactions' hashes differ from the d480efc6 ones (the digest is hashed).
    assert_ne!(tx_hash(&new.producer.0), tx_hash(&old.producer.0));
}

#[test]
fn the_new_rules_refuse_the_d480efc6_layout() {
    let c = config();
    let (blocks, hashes) = chain(&c);
    let old = signed(Layout::D480efc6, &c, &blocks, Vec::new());
    let att = attestations(&c, &hashes, &parent().keys);
    // Split as the default build splits them; the new rules read the first 32 bytes as the fork digest.
    let (a, b) = run(
        Layout::V035,
        &input_a(&c, &blocks, &old, Layout::D480efc6),
        &input_b(&c, &blocks, &old, Layout::D480efc6, att.clone()),
    );
    assert_eq!(a.unwrap_err(), Error::WrongForkDigest);
    assert_eq!(b.unwrap_err(), Error::WrongForkDigest);
    // And the default rules refuse the 0.3.5-rc.1 layout (the digest's first byte is read as the op count).
    let new = signed(Layout::V035, &c, &blocks, Vec::new());
    let (a, b) = run(
        Layout::D480efc6,
        &input_a(&c, &blocks, &new, Layout::V035),
        &input_b(&c, &blocks, &new, Layout::V035, att),
    );
    assert!(a.is_err() && b.is_err());
}

#[test]
fn the_new_rules_refuse_another_fork_digest() {
    let c = config();
    let (blocks, hashes) = chain(&c);
    let new = signed(Layout::V035, &c, &blocks, Vec::new());
    let att = attestations(&c, &hashes, &parent().keys);
    assert!(run(Layout::V035, &input_a(&c, &blocks, &new, Layout::V035), &input_b(&c, &blocks, &new, Layout::V035, att.clone()))
        .1
        .is_ok());
    // The same transactions under a deployment whose digest is another: every Bedrock transaction is refused. (The
    // configuration id moves with the digest, so the attestations are re-made for that deployment.)
    let mut other = c.clone();
    other.bedrock_genesis_id = [0xd2; 32];
    let att = attestations(&other, &hashes, &parent().keys);
    let (a, b) = run(
        Layout::V035,
        &input_a(&other, &blocks, &new, Layout::V035),
        &input_b(&other, &blocks, &new, Layout::V035, att),
    );
    assert_eq!(a.unwrap_err(), Error::WrongForkDigest);
    assert_eq!(b.unwrap_err(), Error::WrongForkDigest);
    // A rotation that carries another digest: refused before any block is read.
    let mut rot = rotation(Layout::V035, &c, 3);
    rot.0[..32].copy_from_slice(&[0xd2; 32]);
    let h = tx_hash(&rot.0);
    rot.1 = (0..3).map(|i| (u16::from(i), channel(i).sign(&h).to_bytes())).collect();
    let new = signed(Layout::V035, &c, &blocks, vec![rot]);
    let att = attestations(&c, &hashes, &five_keys());
    let (_, b) = run(Layout::V035, &input_a(&c, &blocks, &new, Layout::V035), &input_b(&c, &blocks, &new, Layout::V035, att));
    assert_eq!(b.unwrap_err(), Error::WrongForkDigest);
}

#[test]
fn a_rotation_to_an_unreachable_threshold_is_refused_by_the_new_rules_and_applied_by_the_default() {
    let c = config();
    let (blocks, hashes) = chain(&c);
    let att = attestations(&c, &hashes, &five_keys());
    // configuration_threshold 6 over five keys: Bedrock 0.3.5-rc.1 does not decode it (#3781); d480efc6 applied it.
    let old = signed(Layout::D480efc6, &c, &blocks, vec![rotation(Layout::D480efc6, &c, 6)]);
    let (_, b) = run(
        Layout::D480efc6,
        &input_a(&c, &blocks, &old, Layout::D480efc6),
        &input_b(&c, &blocks, &old, Layout::D480efc6, att.clone()),
    );
    assert_eq!(b.unwrap().n, 5);
    let new = signed(Layout::V035, &c, &blocks, vec![rotation(Layout::V035, &c, 6)]);
    let (a, b) = run(Layout::V035, &input_a(&c, &blocks, &new, Layout::V035), &input_b(&c, &blocks, &new, Layout::V035, att.clone()));
    assert_eq!(a.unwrap_err(), Error::UnreachableThreshold { threshold: 6, keys: 5 });
    assert_eq!(b.unwrap_err(), Error::UnreachableThreshold { threshold: 6, keys: 5 });
    // At the key count it is reachable, and applied.
    let new = signed(Layout::V035, &c, &blocks, vec![rotation(Layout::V035, &c, 5)]);
    let (_, b) = run(Layout::V035, &input_a(&c, &blocks, &new, Layout::V035), &input_b(&c, &blocks, &new, Layout::V035, att));
    assert_eq!(b.unwrap().n, 5);
}
