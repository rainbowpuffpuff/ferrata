//! The checkpoint rule (SPEC-v2 §4.2) on real-format LEZ blocks and Logos Blockchain
//! inscriptions: one passing scenario and every refusal the rule must make.

use checkpoint_core::input::CheckpointInput;
use checkpoint_core::rule::{run_checkpoint, Outcome};
use checkpoint_core::tree::{burn_leaf, zero_hashes, Frontier};
use checkpoint_core::wire::BridgeMessage;
use checkpoint_core::{Error, Hash};
use fidelity::bedrock::{config_change, inscribe, Extra, Sequencer};
use fidelity::lez_build::{pp_stand_in, private_key, public_call, Chain, PpSpec};
use fidelity::scenario::{checkpoint_input, label, run_segment, Deployment, Inscribed, OwnedInput};
use lee::{AccountId, Actor};

/// Chain (n = 4, so t = 3; nMin = 3, tMin = 2), by index (block id = index + 1):
/// 0 genesis · 1 empty · 2 public calls to the bridge: Burn A, Burn of another configuration,
/// Burn C (reverted), ImportRoot(5), a message of an unknown variant · 3 a privacy-preserving
/// transaction whose boundary carries Burn D to the bridge, with a fee fallback carrying Burn E
/// (applies only on revert) and a reply from the bridge to a private actor · 4 empty (= T) ·
/// 5..=12 empty, except 7 which carries `Echo(ECHO)`.
struct World {
    dep: Deployment,
    chain: Chain,
    ins: Inscribed,
}

const ECHO: Hash = [0xEC; 32];
const T: usize = 4;

fn world() -> World {
    let dep = Deployment::new(4, 3, 2);
    let mut chain = Chain::genesis().unwrap();
    chain.push_valid(Vec::new()).unwrap();
    let user = private_key(7);
    let other_config = label("another configuration");
    chain
        .push_stand_in(
            vec![
                public_call(&user, 0, dep.bridge(), dep.burn(0, None).encode()),
                public_call(&user, 1, dep.bridge(), dep.burn(1, Some(other_config)).encode()),
                public_call(&user, 2, dep.bridge(), dep.burn(2, None).encode()),
                public_call(&user, 3, dep.bridge(), dep.import_root(5).encode()),
                public_call(&user, 4, dep.bridge(), vec![0xFF, 0xFF]),
            ],
            vec![2],
        )
        .unwrap();
    let private_actor = Actor::new(AccountId::new([0xA1; 32]), AccountId::new(label("token program")));
    let pp = PpSpec {
        deliveries: vec![
            (true, private_actor, dep.bridge(), dep.burn(3, None).encode()),
            (false, dep.bridge(), private_actor, dep.burn(5, None).encode()),
        ],
        fallback: Some(vec![(true, private_actor, dep.bridge(), dep.burn(4, None).encode())]),
        proof_len: 4096,
        ..PpSpec::default()
    };
    chain.push_stand_in(vec![pp_stand_in(&pp, 3)], vec![]).unwrap();
    chain.push_valid(Vec::new()).unwrap(); // 4 = T
    chain.push_valid(Vec::new()).unwrap(); // 5
    chain.push_valid(Vec::new()).unwrap(); // 6
    chain
        .push_stand_in(vec![public_call(&user, 9, dep.bridge(), BridgeMessage::Echo { nonce: ECHO }.encode())], vec![0])
        .unwrap(); // 7
    for _ in 8..=12 {
        chain.push_valid(Vec::new()).unwrap();
    }
    let ins = Inscribed::new(&dep, &chain);
    World { dep, chain, ins }
}

fn input(w: &World, y: usize, e: usize, echo: Hash) -> OwnedInput {
    checkpoint_input(&w.dep, &w.ins, w.dep.genesis_state(&w.chain), 0, T, y, e, echo, 3)
}

fn run(i: &OwnedInput) -> Result<Outcome, Error> {
    // Through the guest's byte format, as the guest sees it.
    let bytes = i.encode();
    run_checkpoint(&CheckpointInput::decode(&bytes).unwrap())
}

/// The burns the rule must collect: A (block 2) then D (block 3).
fn expected_root(w: &World) -> (u64, Hash) {
    let id = w.dep.config_id();
    let mut f = Frontier::default();
    for (index, i) in [(0u64, 0u64), (1, 3)] {
        let BridgeMessage::Burn(b) = w.dep.burn(i, None) else { unreachable!() };
        f.append(burn_leaf(&id, index, &b.token, &b.amount, &b.c_out, &b.r)).unwrap();
    }
    (f.count, f.root(&zero_hashes()))
}

#[test]
fn a_checkpoint_collects_exactly_the_non_reverted_burns_of_its_configuration() {
    let w = world();
    let i = input(&w, T, 7, [0; 32]);
    let out = run(&i).unwrap();
    let (count, root) = expected_root(&w);
    assert_eq!((out.journal.burn_count, out.journal.burn_root), (count, root));
    assert_eq!(out.journal.imported_leaf_count, 5);
    assert!(!out.journal.sunset);
    assert_eq!(out.journal.parent_state, w.dep.genesis_state(&w.chain).digest());
    assert_eq!(out.new_state.tip_id, w.chain.blocks[T].header.block_id);
    assert_eq!(out.new_state.tip_hash, w.chain.blocks[T].header.hash.0);
    assert_eq!(out.journal.new_state, out.new_state.digest());
    assert_eq!((out.support_keys, out.threshold), (3, 3));
    assert_eq!(out.producer, w.dep.sequencers[w.ins.producer[T]].public());
    assert_eq!(out.journal.lez_config_id, w.dep.config_id());
    // SPEC-v3 words 13-16: T's id and hash, |K| and t(|K|).
    assert_eq!((out.journal.tip_id, out.journal.tip_hash), (out.new_state.tip_id, out.new_state.tip_hash));
    assert_eq!((out.journal.n, out.journal.t), (4, 3));
    println!("journal {}", hex::encode(out.journal.encode()));
}

#[test]
fn a_finalisation_checkpoint_counts_support_only_after_the_echo() {
    let w = world();
    // Y = block 7 (carries the echo, in a reverted transaction: inclusion is what counts).
    let i = input(&w, 7, 12, ECHO);
    let out = run(&i).unwrap();
    assert_eq!(out.y_index, 6); // block 7 is input index 6 (blocks start at 1)
    assert_eq!(out.journal.echo_nonce, ECHO);
    // A support block at or before Y is refused.
    let mut early = i.clone();
    early.support[0] = (5, w.ins.inscriptions[6].clone());
    assert_eq!(run(&early), Err(Error::SupportOutOfRange { block_index: 5 }));
    // An echo nobody published.
    let mut absent = i.clone();
    absent.echo_nonce = [0xAB; 32];
    assert_eq!(run(&absent), Err(Error::EchoNotFound));
}

#[test]
fn broken_links_and_reordering_are_refused() {
    let w = world();
    let i = input(&w, T, 7, [0; 32]);
    // A block out of order.
    let mut swapped = i.clone();
    swapped.blocks.swap(0, 1);
    assert_eq!(run(&swapped), Err(Error::BadBlockId { index: 0, expected: 2, got: 3 }));
    // A block of another chain with the right id: it does not link.
    let mut fork = Chain::genesis().unwrap();
    fork.push_valid(Vec::new()).unwrap();
    fork.push_stand_in(vec![public_call(&private_key(9), 0, w.dep.bridge(), w.dep.burn(7, None).encode())], vec![])
        .unwrap();
    let mut broken = i.clone();
    broken.blocks[1] = fork.inscribed(2);
    assert_eq!(run(&broken), Err(Error::BrokenLink { index: 2 }));
    // Contents changed under a stale hash (a burn's amount).
    let mut tampered = i.clone();
    let at = tampered.blocks[1].windows(8).position(|x| x == 1_000_000u64.to_be_bytes()).unwrap();
    tampered.blocks[1][at + 7] ^= 1;
    assert_eq!(run(&tampered), Err(Error::HashMismatch { index: 1 }));
}

#[test]
fn reverted_and_foreign_burns_are_not_collected() {
    let w = world();
    let out = run(&input(&w, T, 7, [0; 32])).unwrap();
    // 5 calls to the bridge carried a Burn; only A and D count (C reverted, one for another
    // configuration, E only in the fee fallback). The reply to a private actor is not executed by
    // the bridge.
    assert_eq!(out.journal.burn_count, 2);
    // The same block without the reverted mark: C is collected too.
    let mut chain = Chain::genesis().unwrap();
    chain.push_valid(Vec::new()).unwrap();
    let user = private_key(7);
    chain
        .push_stand_in(
            vec![
                public_call(&user, 0, w.dep.bridge(), w.dep.burn(0, None).encode()),
                public_call(&user, 2, w.dep.bridge(), w.dep.burn(2, None).encode()),
            ],
            vec![],
        )
        .unwrap();
    for _ in 0..4 {
        chain.push_valid(Vec::new()).unwrap();
    }
    let ins = Inscribed::new(&w.dep, &chain);
    let i = checkpoint_input(&w.dep, &ins, w.dep.genesis_state(&chain), 0, 2, 2, 6, [0; 32], 3);
    assert_eq!(run(&i).unwrap().journal.burn_count, 2);
}

#[test]
fn support_must_come_from_enough_distinct_accredited_keys_other_than_t_s_producer() {
    let w = world();
    let i = input(&w, T, 12, [0; 32]);
    let producer = w.ins.producer[T];
    // T's producer inscribes block 9 (id 10) too.
    let by_producer = (5..=12).find(|&b| w.ins.producer[b] == producer).unwrap();
    // An entry by D is checked but not counted (SPEC-v3 D3): replacing a supporter by D leaves 2.
    let mut with_producer = i.clone();
    with_producer.support[0] = (u32::try_from(by_producer - 1).unwrap(), w.ins.inscriptions[by_producer].clone());
    assert_eq!(run(&with_producer), Err(Error::InsufficientSupport { have: 2, need: 3 }));
    // ... and adding it to three others changes nothing.
    let mut plus_producer = i.clone();
    plus_producer.support.push((u32::try_from(by_producer - 1).unwrap(), w.ins.inscriptions[by_producer].clone()));
    assert_eq!(run(&plus_producer).unwrap().support_keys, 3);
    // The same key twice counts once.
    let first = i.support[0].1.clone();
    let again = (6..=12).find(|&b| w.ins.producer[b] == w.ins.producer[5] && b != 5).unwrap();
    let mut twice = i.clone();
    twice.support = vec![(4, first), (u32::try_from(again - 1).unwrap(), w.ins.inscriptions[again].clone())];
    assert_eq!(run(&twice), Err(Error::InsufficientSupport { have: 1, need: 3 }));
    // A key that is not accredited.
    let outsider = Sequencer::from_seed(99);
    let mut foreign = i.clone();
    foreign.support[0].1 = inscribe(&outsider, w.dep.config.channel_id, &w.ins.bytes[5], [0; 32], Extra::None, 1);
    assert_eq!(run(&foreign), Err(Error::NotAccredited));
    // Another channel.
    let seq = &w.dep.sequencers[w.ins.producer[5]];
    let mut channel = i.clone();
    channel.support[0].1 = inscribe(seq, [2; 32], &w.ins.bytes[5], [0; 32], Extra::None, 1);
    assert_eq!(run(&channel), Err(Error::WrongChannel));
    // A payload that is not the block: the same bytes with another status byte (not covered by
    // the block hash, covered by the signature), and another block.
    let mut other_status = w.ins.bytes[5].clone();
    *other_status.last_mut().unwrap() = 2;
    let mut payload = i.clone();
    payload.support[0].1 = inscribe(seq, w.dep.config.channel_id, &other_status, [0; 32], Extra::None, 1);
    assert_eq!(run(&payload), Err(Error::BadSignature));
    let mut other_block = i.clone();
    other_block.support[0].1 = inscribe(seq, w.dep.config.channel_id, &w.ins.bytes[2], [0; 32], Extra::None, 1);
    assert_eq!(run(&other_block), Err(Error::PayloadMismatch));
    // A tampered signature.
    let mut sig = i.clone();
    sig.support[0].1.signature[10] ^= 4;
    assert_eq!(run(&sig), Err(Error::BadSignature));
    // T's inscription must be by an accredited key too.
    let mut fake_producer = i.clone();
    fake_producer.producer = inscribe(&outsider, w.dep.config.channel_id, &w.ins.bytes[T], [0; 32], Extra::None, 1);
    assert_eq!(run(&fake_producer), Err(Error::NotAccredited));
    // Support inscriptions carrying extra channel ops still parse.
    let mut extra = i.clone();
    extra.support[0].1 = inscribe(seq, w.dep.config.channel_id, &w.ins.bytes[5], [0; 32], Extra::ChannelWithdraw, 1);
    assert!(run(&extra).is_ok());
}

#[test]
fn key_rotations_need_t_current_signatures_and_respect_the_floors() {
    let w = world();
    let s = &w.dep.sequencers;
    let tip = w.dep.genesis_state(&w.chain).config_tip_hash;
    let channel = w.dep.config.channel_id;
    let base = input(&w, T, 12, [0; 32]);
    let as_change = |c: &fidelity::bedrock::ConfigChange| (c.tx.ops.clone(), 0u8, c.signatures.clone());

    // The same keys in another order, signed by 3 of 4: accepted; S changes (order is part of
    // keySetHash), and support still counts.
    let reordered = [&s[3], &s[2], &s[1], &s[0]];
    let ok = config_change(channel, tip, &reordered, 3, &[(0, &s[0]), (1, &s[1]), (3, &s[3])]);
    let mut i = base.clone();
    i.config_changes = vec![as_change(&ok)];
    let out = run(&i).unwrap();
    assert_eq!(out.new_state.keys, reordered.iter().map(|k| k.public()).collect::<Vec<_>>());
    assert_eq!(out.new_state.config_tip_hash, ok.tx.op_id);

    // Signed by 2 of 4 (t(4) = 3).
    let few = config_change(channel, tip, &reordered, 3, &[(0, &s[0]), (1, &s[1])]);
    let mut i = base.clone();
    i.config_changes = vec![as_change(&few)];
    assert_eq!(run(&i), Err(Error::ConfigThresholdUnmet { have: 2, need: 3 }));

    // The same signer at two indices is refused.
    let mut dup = few.signatures.clone();
    dup.push((0, dup[0].1));
    let mut i = base.clone();
    i.config_changes = vec![(few.tx.ops.clone(), 0, dup)];
    assert_eq!(run(&i), Err(Error::ConfigBadIndex));

    // A signature by a key at another index does not verify.
    let mut wrong = ok.signatures.clone();
    wrong[0].0 = 2;
    let mut i = base.clone();
    i.config_changes = vec![(ok.tx.ops.clone(), 0, wrong)];
    assert_eq!(run(&i), Err(Error::BadSignature));

    // Not chained on the current configuration tip.
    let stale = config_change(channel, [7; 32], &reordered, 3, &[(0, &s[0]), (1, &s[1]), (2, &s[2])]);
    let mut i = base.clone();
    i.config_changes = vec![as_change(&stale)];
    assert_eq!(run(&i), Err(Error::ConfigParentMismatch));

    // A key set below nMin = 3.
    let small = config_change(channel, tip, &[&s[0], &s[1]], 1, &[(0, &s[0]), (1, &s[1]), (2, &s[2])]);
    let mut i = base.clone();
    i.config_changes = vec![as_change(&small)];
    assert_eq!(run(&i), Err(Error::KeySetTooSmall { n: 2, n_min: 3 }));

    // A threshold below tMin: n = 3 gives t = 2; with tMin = 3 the set is refused.
    let mut strict = base.clone();
    strict.config.t_min = 3;
    let three = config_change(channel, tip, &[&s[1], &s[2], &s[3]], 2, &[(0, &s[0]), (1, &s[1]), (2, &s[2])]);
    strict.config_changes = vec![as_change(&three)];
    assert_eq!(run(&strict), Err(Error::ThresholdTooLow { t: 2, t_min: 3 }));
}

#[test]
fn sunset_and_imports() {
    let dep = Deployment::new(4, 3, 2);
    let mut chain = Chain::genesis().unwrap();
    let user = private_key(7);
    let calls = [
        dep.import_root(5),
        dep.import_root(3), // the count never goes down
        dep.burn(0, None),
        BridgeMessage::Sunset { config_id: dep.config_id() },
        dep.import_root(9), // refused after Sunset
        dep.burn(1, None),  // burns continue
    ];
    let txs = calls.iter().enumerate().map(|(k, m)| public_call(&user, k as u128, dep.bridge(), m.encode())).collect();
    chain.push_stand_in(txs, vec![]).unwrap();
    for _ in 0..4 {
        chain.push_valid(Vec::new()).unwrap();
    }
    let ins = Inscribed::new(&dep, &chain);
    let i = checkpoint_input(&dep, &ins, dep.genesis_state(&chain), 0, 1, 1, 5, [0; 32], 3);
    let out = run(&i).unwrap();
    assert_eq!((out.journal.imported_leaf_count, out.journal.sunset, out.journal.burn_count), (5, true, 2));
}

#[test]
fn segments_compose_to_the_same_journal() {
    let w = world();
    let whole = input(&w, T, 7, [0; 32]);
    let one = run(&whole).unwrap();
    // Segment over blocks 1..=2, then a checkpoint over 3..=7 that verifies it.
    let genesis = w.dep.genesis_state(&w.chain);
    let (seg, mid) = run_segment(&w.dep, &w.ins, &genesis, 1, 2).unwrap();
    let mut second = checkpoint_input(&w.dep, &w.ins, mid.clone(), 2, T, T, 7, [0; 32], 3);
    second.segments = vec![seg];
    let two = run(&second).unwrap();
    assert_eq!(one.journal, two.journal, "two pieces prove the same journal as one");
    // A segment that does not end where the checkpoint continues.
    let mut gap = second.clone();
    let (seg1, _) = run_segment(&w.dep, &w.ins, &genesis, 1, 1).unwrap();
    gap.segments = vec![seg1];
    assert_eq!(run(&gap), Err(Error::SegmentChainBroken));
    // Two segments that do not chain.
    let mut unchained = second.clone();
    unchained.segments = vec![seg1, seg];
    assert_eq!(run(&unchained), Err(Error::SegmentChainBroken));
    // A segment of another configuration.
    let mut foreign = second.clone();
    foreign.segments[0].lez_config_id = [9; 32];
    assert_eq!(run(&foreign), Err(Error::SegmentConfigMismatch));
}

#[test]
fn a_single_key_zone_needs_n_min_one() {
    let dep = Deployment::new(1, 1, 1);
    let mut chain = Chain::genesis().unwrap();
    for _ in 0..3 {
        chain.push_valid(Vec::new()).unwrap();
    }
    let ins = Inscribed::new(&dep, &chain);
    // Support by the one key that also inscribed T.
    let mut i = checkpoint_input(&dep, &ins, dep.genesis_state(&chain), 0, 1, 1, 3, [0; 32], 1);
    i.support = vec![(1, ins.inscriptions[2].clone())];
    let out = run(&i).unwrap();
    assert_eq!((out.support_keys, out.threshold), (1, 1));
    // The same with nMin = 2 is refused.
    let mut strict = i.clone();
    strict.config.n_min = 2;
    assert_eq!(run(&strict), Err(Error::KeySetTooSmall { n: 1, n_min: 2 }));
}

/// SPEC-v3 D3: the producer `D` is one *checked* inscription of `T`, by a key of `K`; support is by
/// `t` distinct keys of `K` other than `D`. The attack SPEC-v2 allowed: `t` keys (here keys 1, 2, 3
/// of 4; `t(4) = 3`) produce `T` (inscribed by key 1) and name the honest key 0 as `D`, so that all
/// three of theirs count as support.
#[test]
fn the_producer_is_a_checked_inscription_of_t() {
    let w = world();
    let s = &w.dep.sequencers;
    let channel = w.dep.config.channel_id;
    assert_eq!(w.ins.producer[T], 1, "T (block id 5) is inscribed by key 1");
    // Blocks after T, by key: id 6 -> key 2, id 7 -> key 3, id 8 -> key 0 (honest), id 9 -> key 1.
    let by = |key: usize| (T + 1..=12).find(|&b| w.ins.producer[b] == key).unwrap();
    let entry = |b: usize| (u32::try_from(b - 1).unwrap(), w.ins.inscriptions[b].clone());
    let attack = OwnedInput { support: vec![entry(by(1)), entry(by(2)), entry(by(3))], ..input(&w, T, 12, [0; 32]) };

    // D unchecked: the honest key's signature over T's inscription is forged.
    let mut forged = attack.clone();
    forged.producer = inscribe(&s[0], channel, &w.ins.bytes[T], [0; 32], Extra::None, 1);
    forged.producer.signature = w.ins.inscriptions[T].signature;
    assert_eq!(run(&forged), Err(Error::BadSignature));
    // D named with the honest key's real inscriptions of other blocks: one of T's length (the
    // signature then fails over T's bytes) and the echo block (whose length already differs).
    let same_len =
        (T + 1..=12).find(|&b| w.ins.producer[b] == 0 && w.ins.bytes[b].len() == w.ins.bytes[T].len()).unwrap();
    let mut borrowed = attack.clone();
    borrowed.producer = w.ins.inscriptions[same_len].clone();
    assert_eq!(run(&borrowed), Err(Error::BadSignature));
    assert_ne!(w.ins.bytes[by(0)].len(), w.ins.bytes[T].len());
    borrowed.producer = w.ins.inscriptions[by(0)].clone();
    assert_eq!(run(&borrowed), Err(Error::PayloadMismatch));
    // D not in K: an outsider "inscribed" T.
    let mut outsider = attack.clone();
    outsider.producer = inscribe(&Sequencer::from_seed(77), channel, &w.ins.bytes[T], [0; 32], Extra::None, 1);
    assert_eq!(run(&outsider), Err(Error::NotAccredited));
    // With the true D (key 1), key 1's own support is not counted: 2 of 3.
    assert_eq!(attack.producer.signature, w.ins.inscriptions[T].signature);
    assert_eq!(run(&attack), Err(Error::InsufficientSupport { have: 2, need: 3 }));
    // The honest key's support completes it.
    let mut honest = attack.clone();
    honest.support.push(entry(by(0)));
    let out = run(&honest).unwrap();
    assert_eq!((out.support_keys, out.producer), (3, s[1].public()));
}
