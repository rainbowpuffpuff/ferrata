//! Rule (B): acceptance, the journal, and every refusal, on hand-built blocks (their format is
//! checked against LEZ's own in `fidelity/`) with key rotations in Bedrock's `ChannelConfigOp`
//! format. Every input goes through its byte encoding first, as the guest receives it.
//!
//! The standard case: `n = 4` channel keys (`t = 3`), the parent at block 1, blocks 2..=6, `T` is
//! block 4 (index 2), so blocks 5 and 6 descend from `T`. Keys 4 and 5 are outside the set.
mod common;

use attestor_core::{sign_attestation, sign_certificate, Deployment, FinalTip, SignedAttestation};
use checkpoint_core::input::{CheckpointInput, CheckpointInputB, Support};
use checkpoint_core::mantle::{tx_hash, ConfigChange, OP_CHANNEL_CONFIG};
use checkpoint_core::rule::run_checkpoint;
use checkpoint_core::rule_b::{run_checkpoint_b, OutcomeB};
use checkpoint_core::state::{key_set_hash, Config, State};
use checkpoint_core::tree::Frontier;
use checkpoint_core::{Error, Hash};
use common::*;
use ed25519_dalek::{Signer as _, SigningKey};

const TIMEFRAME: u32 = 30;
const T: u32 = 2;

fn uint256(x: u64) -> Hash {
    let mut w = [0u8; 32];
    w[24..].copy_from_slice(&x.to_be_bytes());
    w
}

fn config(n_min: u32, t_min: u32) -> Config {
    Config {
        bedrock_genesis_id: [1; 32],
        channel_id: [2; 32],
        bridge_header: [3; 32],
        bridge_image_id: [4; 32],
        evm_chain_id: uint256(11_155_111),
        vault: [5; 20],
        attestation_root_id: [6; 32],
        salt: [7; 32],
        n_min,
        t_min,
    }
}

fn pk(k: &SigningKey) -> Hash {
    k.verifying_key().to_bytes()
}

/// Channel keys 0..6 and attestation keys 0..6.
fn channel(i: u8) -> SigningKey {
    key(0x10 + i)
}
fn att(i: u8) -> SigningKey {
    key(0x80 + i)
}

fn parent(keys: &[Hash]) -> State {
    State {
        tip_id: 1,
        tip_hash: [9; 32],
        keys: keys.to_vec(),
        posting_timeframe: TIMEFRAME,
        config_tip_hash: [8; 32],
        burns: Frontier::default(),
        imported_leaf_count: 0,
        sunset: false,
    }
}

/// Blocks 2.. on the parent's tip; returns the blocks and their hashes.
fn chain(len: usize) -> (Vec<Vec<u8>>, Vec<Hash>) {
    let mut prev = [9u8; 32];
    let (mut blocks, mut hashes) = (Vec::new(), Vec::new());
    for i in 0..len {
        let id = 2 + u64::try_from(i).unwrap();
        let (b, h) = block(id, &prev, 1_000 + id, &[], &[]);
        blocks.push(b);
        hashes.push(h);
        prev = h;
    }
    (blocks, hashes)
}

fn deployment(c: &Config) -> Deployment {
    Deployment { evm_chain_id: c.evm_chain_id, vault: c.vault, lez_config_id: c.config_id(), channel_id: c.channel_id }
}

/// The final tip at block `index` of `hashes` (block id `2 + index`).
fn tip(hashes: &[Hash], index: usize) -> FinalTip {
    FinalTip { tip_id: 2 + u64::try_from(index).unwrap(), tip_hash: hashes[index], final_msg_id: [0x5e; 32] }
}

/// Channel key `ch` certifies `a` for `d`; `a` attests `tip` under `ksh`.
fn attest(d: &Deployment, ch: &SigningKey, a: &SigningKey, ksh: &Hash, tip: FinalTip) -> SignedAttestation {
    SignedAttestation::new(a, pk(ch), sign_certificate(ch, d, &pk(a)), d, ksh, tip)
}

type Rotation = (Vec<u8>, Vec<(u16, [u8; 64])>);

/// A Mantle transaction with one `ChannelConfigOp` on `parent_tip` setting `keys`, signed over its
/// hash by `signers` (index into the current list, key).
fn rotation(c: &Config, parent_tip: &Hash, keys: &[Hash], signers: &[(u16, SigningKey)]) -> Rotation {
    let mut ops = vec![1u8, OP_CHANNEL_CONFIG];
    ops.extend_from_slice(&c.channel_id);
    ops.extend_from_slice(parent_tip);
    ops.extend_from_slice(&u16::try_from(keys.len()).unwrap().to_le_bytes());
    for k in keys {
        ops.extend_from_slice(k);
    }
    ops.extend_from_slice(&TIMEFRAME.to_le_bytes()); // posting_timeframe
    ops.extend_from_slice(&60u32.to_le_bytes()); // posting_timeout
    ops.extend_from_slice(&3u16.to_le_bytes()); // configuration_threshold
    ops.extend_from_slice(&3u16.to_le_bytes()); // transfer_threshold
    let h = tx_hash(&ops);
    let sigs = signers.iter().map(|(i, k)| (*i, k.sign(&h).to_bytes())).collect();
    (ops, sigs)
}

fn input<'a>(
    c: &Config,
    p: &State,
    blocks: &'a [Vec<u8>],
    rotations: &'a [Rotation],
    attestations: Vec<SignedAttestation>,
) -> CheckpointInputB<'a> {
    CheckpointInputB {
        config: c.clone(),
        parent: p.clone(),
        config_changes: rotations
            .iter()
            .map(|(ops, sigs)| ConfigChange {
                ops,
                op_index: 0,
                signatures: sigs.iter().map(|(i, s)| (*i, s)).collect(),
            })
            .collect(),
        blocks: blocks.iter().map(Vec::as_slice).collect(),
        t_index: T,
        attestations,
        segments: Vec::new(),
    }
}

/// Encodes, decodes (as the guest does), re-encodes, and runs rule (B).
fn run_b(input: &CheckpointInputB<'_>) -> Result<OutcomeB, Error> {
    let bytes = input.encode();
    let decoded = CheckpointInputB::decode(&bytes).expect("input decodes");
    assert_eq!(decoded.encode(), bytes);
    run_checkpoint_b(&decoded)
}

/// The standard case's pieces.
struct World {
    c: Config,
    d: Deployment,
    p: State,
    blocks: Vec<Vec<u8>>,
    hashes: Vec<Hash>,
    ksh: Hash,
}

fn world() -> World {
    let c = config(3, 2);
    let p = parent(&(0..4).map(|i| pk(&channel(i))).collect::<Vec<_>>());
    let (blocks, hashes) = chain(5);
    let ksh = key_set_hash(&p.keys, TIMEFRAME);
    World { d: deployment(&c), c, p, blocks, hashes, ksh }
}

impl World {
    /// Attestor `i` (channel key `i`, attestation key `i`) over block `index`.
    fn by(&self, i: u8, index: usize) -> SignedAttestation {
        attest(&self.d, &channel(i), &att(i), &self.ksh, tip(&self.hashes, index))
    }
    fn run(&self, attestations: Vec<SignedAttestation>) -> Result<OutcomeB, Error> {
        run_b(&input(&self.c, &self.p, &self.blocks, &[], attestations))
    }
}

#[test]
fn accepts_t_attestations_over_the_tip_and_its_descendants() {
    let w = world();
    let out = w.run(vec![w.by(0, 2), w.by(1, 3), w.by(3, 4)]).unwrap();
    assert_eq!((out.attestors, out.threshold, out.path_after_tip), (3, 3, 2));
    let j = out.journal;
    assert_eq!(j.echo_nonce, [0; 32]);
    assert_eq!((j.tip_id, j.tip_hash), (4, w.hashes[2]));
    assert_eq!((j.n, j.t, j.n_min, j.t_min), (4, 3, 3, 2));
    assert_eq!(j.parent_state, w.p.digest());
    assert_eq!(j.new_state, out.new_state.digest());
    assert_eq!(j.lez_config_id, w.c.config_id());
    // More than t, all distinct, is fine; so is every attestation on T itself.
    assert!(w.run(vec![w.by(0, 2), w.by(1, 2), w.by(2, 3), w.by(3, 4)]).is_ok());
    assert!(w.run(vec![w.by(0, 2), w.by(1, 2), w.by(2, 2)]).is_ok());
}

#[test]
fn journal_is_rule_a_candidate_journal_over_the_same_blocks() {
    let w = world();
    let b = w.run(vec![w.by(0, 2), w.by(1, 3), w.by(2, 4)]).unwrap();
    // Rule (A) over the same blocks: T's inscription by key 0, support by keys 1..3 after T.
    let txs: Vec<(Vec<u8>, [u8; 64])> = [(0u8, 2usize), (1, 3), (2, 4), (3, 4)]
        .iter()
        .map(|(k, i)| inscription_tx(&channel(*k), &w.c.channel_id, &w.blocks[*i], &[6; 32]))
        .collect();
    let a = CheckpointInput {
        config: w.c.clone(),
        parent: w.p.clone(),
        config_changes: Vec::new(),
        blocks: w.blocks.iter().map(Vec::as_slice).collect(),
        t_index: T,
        echo_nonce: [0; 32],
        producer: elide(&txs[0].0, &txs[0].1),
        support: [(1usize, 3u32), (2, 4), (3, 4)]
            .iter()
            .map(|(k, i)| Support { block_index: *i, inscription: elide(&txs[*k].0, &txs[*k].1) })
            .collect(),
        segments: Vec::new(),
    };
    let a = run_checkpoint(&a).unwrap();
    assert_eq!(a.journal.encode(), b.journal.encode());
    assert_eq!(a.new_state, b.new_state);
}

#[test]
fn refuses_too_few() {
    let w = world();
    assert_eq!(w.run(vec![w.by(0, 2), w.by(1, 3)]).unwrap_err(), Error::InsufficientAttestations { have: 2, need: 3 });
    assert_eq!(w.run(Vec::new()).unwrap_err(), Error::InsufficientAttestations { have: 0, need: 3 });
}

#[test]
fn refuses_a_duplicate_signer() {
    let w = world();
    // The same attestation twice.
    assert_eq!(w.run(vec![w.by(0, 2), w.by(1, 2), w.by(1, 2)]).unwrap_err(), Error::DuplicateAttestor);
    // One channel key, two attestation keys it certified, two attestations.
    let second = attest(&w.d, &channel(1), &att(5), &w.ksh, tip(&w.hashes, 3));
    assert_eq!(w.run(vec![w.by(0, 2), w.by(1, 2), second]).unwrap_err(), Error::DuplicateAttestor);
    // One attestation key certified by two channel keys.
    let shared = attest(&w.d, &channel(2), &att(1), &w.ksh, tip(&w.hashes, 3));
    assert_eq!(w.run(vec![w.by(0, 2), w.by(1, 2), shared]).unwrap_err(), Error::DuplicateAttestor);
}

#[test]
fn refuses_an_uncertified_key() {
    let w = world();
    let tip2 = tip(&w.hashes, 2);
    // Certified by itself rather than by the channel key it names.
    let a = att(2);
    let self_signed =
        SignedAttestation::new(&a, pk(&channel(2)), sign_certificate(&a, &w.d, &pk(&a)), &w.d, &w.ksh, tip2);
    assert_eq!(w.run(vec![w.by(0, 2), w.by(1, 2), self_signed]).unwrap_err(), Error::BadCertificate);
    // Certified by another member of the set than the one it names.
    let borrowed =
        SignedAttestation::new(&a, pk(&channel(2)), sign_certificate(&channel(3), &w.d, &pk(&a)), &w.d, &w.ksh, tip2);
    assert_eq!(w.run(vec![w.by(0, 2), w.by(1, 2), borrowed]).unwrap_err(), Error::BadCertificate);
    // No certificate at all.
    let none = SignedAttestation::new(&a, pk(&channel(2)), [0; 64], &w.d, &w.ksh, tip2);
    assert_eq!(w.run(vec![w.by(0, 2), w.by(1, 2), none]).unwrap_err(), Error::BadCertificate);
}

#[test]
fn refuses_a_certificate_from_another_channel_or_configuration() {
    let w = world();
    let mut other_config = w.c.clone();
    other_config.salt = [0x77; 32];
    let mut elsewhere = Vec::new();
    for change in 0..4 {
        let mut d = w.d;
        match change {
            0 => d.channel_id = [0x99; 32],
            1 => d.lez_config_id = other_config.config_id(),
            2 => d.evm_chain_id = uint256(1),
            _ => d.vault = [0x42; 20],
        }
        elsewhere.push(d);
    }
    for d in elsewhere {
        let cert = sign_certificate(&channel(2), &d, &pk(&att(2)));
        let a = SignedAttestation::new(&att(2), pk(&channel(2)), cert, &w.d, &w.ksh, tip(&w.hashes, 2));
        assert_eq!(w.run(vec![w.by(0, 2), w.by(1, 2), a]).unwrap_err(), Error::BadCertificate);
    }
}

#[test]
fn refuses_a_key_not_in_the_live_set() {
    let w = world();
    // Channel key 4 never was in the set; its certificate and attestation are otherwise valid.
    assert_eq!(w.run(vec![w.by(0, 2), w.by(1, 2), w.by(4, 2)]).unwrap_err(), Error::NotAccredited);
}

#[test]
fn a_key_removed_by_a_rotation_is_refused_and_a_key_added_counts() {
    let w = world();
    // Key 3 leaves, key 4 joins; keys 0..3 sign (t(4) = 3).
    let new_keys: Vec<Hash> = [0u8, 1, 2, 4].iter().map(|i| pk(&channel(*i))).collect();
    let rot =
        vec![rotation(&w.c, &w.p.config_tip_hash, &new_keys, &[(0, channel(0)), (1, channel(1)), (2, channel(2))])];
    let live = key_set_hash(&new_keys, TIMEFRAME);
    let by = |i: u8, index: usize| attest(&w.d, &channel(i), &att(i), &live, tip(&w.hashes, index));

    let removed = input(&w.c, &w.p, &w.blocks, &rot, vec![by(0, 2), by(1, 3), by(3, 4)]);
    assert_eq!(run_b(&removed).unwrap_err(), Error::NotAccredited);

    let out = run_b(&input(&w.c, &w.p, &w.blocks, &rot, vec![by(0, 2), by(1, 3), by(4, 4)])).unwrap();
    assert_eq!(out.new_state.keys, new_keys);
    assert_eq!((out.journal.n, out.journal.t), (4, 3));
}

#[test]
fn refuses_a_wrong_key_set_hash() {
    let w = world();
    // After a rotation, attestations still naming the parent's key set (a stale attestor).
    let new_keys: Vec<Hash> = [0u8, 1, 2, 4].iter().map(|i| pk(&channel(*i))).collect();
    let rot =
        vec![rotation(&w.c, &w.p.config_tip_hash, &new_keys, &[(0, channel(0)), (1, channel(1)), (2, channel(2))])];
    let stale = input(&w.c, &w.p, &w.blocks, &rot, vec![w.by(0, 2), w.by(1, 3), w.by(2, 4)]);
    assert_eq!(run_b(&stale).unwrap_err(), Error::BadAttestation);
    // Without a rotation: the same keys under another turn length.
    let other = key_set_hash(&w.p.keys, TIMEFRAME + 1);
    let a = attest(&w.d, &channel(2), &att(2), &other, tip(&w.hashes, 2));
    assert_eq!(w.run(vec![w.by(0, 2), w.by(1, 2), a]).unwrap_err(), Error::BadAttestation);
}

#[test]
fn refuses_a_wrong_tip_chain_vault_configuration_or_domain() {
    let w = world();
    let good = w.by(2, 2);
    let resigned = |d: &Deployment, tip: &FinalTip| SignedAttestation {
        signature: sign_attestation(&att(2), d, &w.ksh, tip),
        ..good
    };
    let tip2 = tip(&w.hashes, 2);
    let mut bad = vec![
        // Signed over block 5 but presented as T (both on the path).
        SignedAttestation { tip: tip2, ..resigned(&w.d, &tip(&w.hashes, 3)) },
        // Another final message id than the one signed.
        SignedAttestation { tip: FinalTip { final_msg_id: [0x11; 32], ..tip2 }, ..good },
    ];
    // Another chain, vault, configuration or channel in the signed message.
    let mut other_config = w.c.clone();
    other_config.salt = [0x77; 32];
    for change in 0..4 {
        let mut d = w.d;
        match change {
            0 => d.evm_chain_id = uint256(1),
            1 => d.vault = [0x42; 20],
            2 => d.lez_config_id = other_config.config_id(),
            _ => d.channel_id = [0x99; 32],
        }
        bad.push(resigned(&d, &tip2));
    }
    // Another domain: D7's v1 tag over the same fields.
    let mut m = w.d.attestation_message(&w.ksh, &tip2);
    m[..21].copy_from_slice(b"ferrata/final-tier/v1");
    bad.push(SignedAttestation { signature: att(2).sign(&m).to_bytes(), ..good });
    for a in bad {
        assert_eq!(w.run(vec![w.by(0, 2), w.by(1, 2), a]).unwrap_err(), Error::BadAttestation);
    }
    assert!(w.run(vec![w.by(0, 2), w.by(1, 2), good]).is_ok());
}

#[test]
fn refuses_an_attestation_over_a_block_that_is_not_a_descendant() {
    let w = world();
    let by2 = |tip: FinalTip| attest(&w.d, &channel(2), &att(2), &w.ksh, tip);
    // An ancestor of T (block 3).
    let ancestor = by2(tip(&w.hashes, 1));
    assert_eq!(w.run(vec![w.by(0, 2), w.by(1, 2), ancestor]).unwrap_err(), Error::NotADescendant { tip_id: 3 });
    // A block 5 on a fork from T: the input's block 5 has another hash.
    let (fork, fork_hash) = block(5, &w.hashes[2], 9_999, &[], &[]);
    assert_ne!(fork, w.blocks[3]);
    let forked = by2(FinalTip { tip_id: 5, tip_hash: fork_hash, final_msg_id: [0x5e; 32] });
    assert_eq!(w.run(vec![w.by(0, 2), w.by(1, 2), forked]).unwrap_err(), Error::NotADescendant { tip_id: 5 });
    // A later block than the input holds (block 7), and an id that does not match its hash.
    let beyond = by2(FinalTip { tip_id: 7, tip_hash: [0x31; 32], final_msg_id: [0x5e; 32] });
    assert_eq!(w.run(vec![w.by(0, 2), w.by(1, 2), beyond]).unwrap_err(), Error::NotADescendant { tip_id: 7 });
    let shifted = by2(FinalTip { tip_id: 5, tip_hash: w.hashes[4], final_msg_id: [0x5e; 32] });
    assert_eq!(w.run(vec![w.by(0, 2), w.by(1, 2), shifted]).unwrap_err(), Error::NotADescendant { tip_id: 5 });
}

#[test]
fn blocks_after_the_tip_are_linked() {
    let w = world();
    let mut blocks = w.blocks.clone();
    blocks[3][72] ^= 1; // block 5's timestamp: hashed, so its stored hash no longer matches
    let tampered = input(&w.c, &w.p, &blocks, &[], vec![w.by(0, 2), w.by(1, 2), w.by(2, 2)]);
    assert_eq!(run_b(&tampered).unwrap_err(), Error::HashMismatch { index: 3 });
}

#[test]
fn single_key_only_where_the_floor_allows_it() {
    let c = config(1, 1);
    let p = parent(&[pk(&channel(0))]);
    let (blocks, hashes) = chain(5);
    let ksh = key_set_hash(&p.keys, TIMEFRAME);
    let one = attest(&deployment(&c), &channel(0), &att(0), &ksh, tip(&hashes, 2));
    let out = run_b(&input(&c, &p, &blocks, &[], vec![one])).unwrap();
    assert_eq!((out.journal.n, out.journal.t), (1, 1));
    // The same set under the production floors is refused before any attestation is read.
    let c3 = config(3, 2);
    let one = attest(&deployment(&c3), &channel(0), &att(0), &ksh, tip(&hashes, 2));
    assert_eq!(run_b(&input(&c3, &p, &blocks, &[], vec![one])).unwrap_err(), Error::KeySetTooSmall { n: 1, n_min: 3 });
}
