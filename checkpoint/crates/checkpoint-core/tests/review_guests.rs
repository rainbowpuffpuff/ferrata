//! Tests of the agent review of 12 November 2026 (docs/REVIEW-guests.md, row 17): the refusal of a key
//! set with a repeated key, which no test pinned (mutant G13 of `scripts/review-guest-mutants.py`
//! survived every test), and what that refusal does not cover (a repeated key in the parent's own set
//! when the first checkpoint rotates it away). Rule (B) inputs, as in `rule_b.rs`, on hand-built blocks;
//! the check is `rule::up_to_tip`'s, shared by both rules. Nothing here changes a guest.
mod common;

use attestor_core::{sign_certificate, Deployment, FinalTip, SignedAttestation};
use checkpoint_core::input::CheckpointInputB;
use checkpoint_core::mantle::{tx_hash, ConfigChange, ElidedInscription, OP_CHANNEL_CONFIG};
use checkpoint_core::rule_b::{run_checkpoint_b, OutcomeB};
use checkpoint_core::state::{key_set_hash, Config, State};
use checkpoint_core::tree::Frontier;
use checkpoint_core::{Error, Hash};
use common::*;
use ed25519_dalek::{Signer as _, SigningKey};

const TIMEFRAME: u32 = 30;
const T: u32 = 2;

fn config() -> Config {
    let mut chain = [0u8; 32];
    chain[24..].copy_from_slice(&11_155_111u64.to_be_bytes());
    Config {
        bedrock_genesis_id: [1; 32],
        channel_id: [2; 32],
        bridge_header: [3; 32],
        bridge_image_id: [4; 32],
        evm_chain_id: chain,
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

/// Blocks 2..2+len on the parent's tip, and their hashes.
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

type Rotation = (Vec<u8>, Vec<(u16, [u8; 64])>);

/// One `ChannelConfigOp` on `parent_tip` setting `keys`, signed by `signers` (index into the current list, key).
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
    producer: ElidedInscription<'a>,
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
        producer,
        attestations,
        segments: Vec::new(),
    }
}

/// Encodes and decodes (as the guest receives it), then runs rule (B).
fn run_b(input: &CheckpointInputB<'_>) -> Result<OutcomeB, Error> {
    let bytes = input.encode();
    let decoded = CheckpointInputB::decode(&bytes).expect("input decodes");
    run_checkpoint_b(&decoded)
}

/// Attestor `i` (channel key `i`, attestation key `i`) over block `index`, under key-set hash `ksh`.
fn by(c: &Config, hashes: &[Hash], ksh: &Hash, i: u8, index: usize) -> SignedAttestation {
    let d = Deployment {
        evm_chain_id: c.evm_chain_id,
        vault: c.vault,
        lez_config_id: c.config_id(),
        channel_id: c.channel_id,
    };
    let tip = FinalTip { tip_id: 2 + u64::try_from(index).unwrap(), tip_hash: hashes[index], final_msg_id: [0x5e; 32] };
    let (ch, a) = (channel(i), att(i));
    SignedAttestation::new(&a, pk(&ch), sign_certificate(&ch, &d, &pk(&a)), &d, ksh, tip)
}

#[test]
fn a_rotation_to_a_key_list_with_a_repeated_key_is_refused() {
    let c = config();
    let p = parent(&(0..4).map(|i| pk(&channel(i))).collect::<Vec<_>>());
    let (blocks, hashes) = chain(5);
    let producer = inscription_tx(&channel(0), &c.channel_id, &blocks[T as usize], &[6; 32]);
    // Key 0 twice: four entries, three keys. Signed by keys 0..2 of the current set (t(4) = 3).
    let repeated: Vec<Hash> = [0u8, 0, 1, 2].iter().map(|i| pk(&channel(*i))).collect();
    let rot = vec![rotation(&c, &p.config_tip_hash, &repeated, &[(0, channel(0)), (1, channel(1)), (2, channel(2))])];
    let live = key_set_hash(&repeated, TIMEFRAME);
    let atts = vec![by(&c, &hashes, &live, 0, 2), by(&c, &hashes, &live, 1, 3), by(&c, &hashes, &live, 2, 4)];
    let i = input(&c, &p, &blocks, &rot, elide(&producer.0, &producer.1), atts);
    assert_eq!(run_b(&i).unwrap_err(), Error::ConfigMalformed);
}

#[test]
fn a_parent_key_set_with_a_repeated_key_is_refused_when_no_rotation_replaces_it() {
    let c = config();
    let keys: Vec<Hash> = [0u8, 0, 1, 2].iter().map(|i| pk(&channel(*i))).collect();
    let p = parent(&keys);
    let (blocks, hashes) = chain(5);
    let producer = inscription_tx(&channel(0), &c.channel_id, &blocks[T as usize], &[6; 32]);
    let ksh = key_set_hash(&keys, TIMEFRAME);
    let atts = vec![by(&c, &hashes, &ksh, 0, 2), by(&c, &hashes, &ksh, 1, 3), by(&c, &hashes, &ksh, 2, 4)];
    let i = input(&c, &p, &blocks, &[], elide(&producer.0, &producer.1), atts);
    assert_eq!(run_b(&i).unwrap_err(), Error::ConfigMalformed);
}

/// The parent's own set is checked only when no rotation replaces it (`rule::up_to_tip`; README: a genesis
/// channel may start below the floors and rotate up). So a parent that repeats a key, which only a genesis state
/// can (every set a checkpoint reaches is checked), lets that key fill two signature indices of the rotation
/// that replaces it: here key 0 gives two of the three signatures `t(4)` asks. The vault's genesis state is a
/// trust root (security review I-1); the deployment check must refuse a key list with a repeated key.
#[test]
fn a_repeated_key_in_a_genesis_parent_fills_two_signature_indices_of_its_rotation() {
    let c = config();
    let genesis: Vec<Hash> = [0u8, 0, 1, 2].iter().map(|i| pk(&channel(*i))).collect();
    let p = parent(&genesis);
    let (blocks, hashes) = chain(5);
    let producer = inscription_tx(&channel(0), &c.channel_id, &blocks[T as usize], &[6; 32]);
    let next: Vec<Hash> = (0..4).map(|i| pk(&channel(i))).collect();
    let rot = vec![rotation(&c, &p.config_tip_hash, &next, &[(0, channel(0)), (1, channel(0)), (2, channel(1))])];
    let live = key_set_hash(&next, TIMEFRAME);
    let atts = vec![by(&c, &hashes, &live, 0, 2), by(&c, &hashes, &live, 1, 3), by(&c, &hashes, &live, 2, 4)];
    let out = run_b(&input(&c, &p, &blocks, &rot, elide(&producer.0, &producer.1), atts)).unwrap();
    assert_eq!(out.new_state.keys, next);
    assert_eq!(out.journal.parent_state, p.digest());
}
