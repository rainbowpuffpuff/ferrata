//! checkpoint-core's parsers against blocks and transactions built by LEZ v0.4.0-rc1 and
//! Logos Blockchain d480efc6, and against the real blocks of LEZ's committed sequencer dump.

use checkpoint_core::lez::{hash_parsed_block, parse_block, Actor as CoreActor, NoVisitor, Origin, TxKind, Visitor};
use checkpoint_core::mantle::{inscription_msg_id, opcodes, tx_hash, ConfigChange, ElidedInscription};
use checkpoint_core::wire::BridgeMessage;
use common::block::{BedrockStatus, Block};
use common::transaction::LeeTransaction;
use fidelity::bedrock::{config_change, inscribe, Extra, Sequencer};
use fidelity::dump::dump_blocks;
use fidelity::lez_build::{
    native_transfer, pp_stand_in, private_key, public_call, resize_proof, Chain, PpSpec, PROOF_BYTES,
};
use fidelity::scenario::{label, Deployment};
use lee::{AccountId, Actor, BoundaryStep, ProgramId};
use lee_core::ProgramImageClaim;

type Pair = ([u8; 32], [u8; 32]);
type Call = (u32, Origin, Option<Pair>, Pair, Vec<u8>);

#[derive(Default)]
struct Record {
    txs: Vec<(u32, TxKind, Vec<u8>)>,
    calls: Vec<Call>,
}

fn pair(a: CoreActor<'_>) -> Pair {
    (*a.account_id, *a.program_account_id)
}

fn upair(a: &Actor) -> Pair {
    (*a.account_id.value(), *a.program_account_id.value())
}

impl<'a> Visitor<'a> for Record {
    fn tx(&mut self, index: u32, kind: TxKind, bytes: &'a [u8]) {
        self.txs.push((index, kind, bytes.to_vec()));
    }
    fn call(&mut self, index: u32, origin: Origin, from: Option<CoreActor<'a>>, to: CoreActor<'a>, message: &'a [u8]) {
        self.calls.push((index, origin, from.map(pair), pair(to), message.to_vec()));
    }
}

/// The visible calls of an upstream transaction, in the order the core reports them (encoding
/// order: boundary, public root, fee-fallback boundary).
fn expected_calls(i: u32, tx: &LeeTransaction) -> Vec<Call> {
    let steps = |steps: &[BoundaryStep], fallback: bool, out: &mut Vec<Call>| {
        for s in steps {
            let (origin, d) = match s {
                BoundaryStep::PrivateToPublic(d) if fallback => (Origin::FallbackPrivateToPublic, d),
                BoundaryStep::PrivateToPublic(d) => (Origin::PrivateToPublic, d),
                BoundaryStep::PublicToPrivate(d) if fallback => (Origin::FallbackPublicToPrivate, d),
                BoundaryStep::PublicToPrivate(d) => (Origin::PublicToPrivate, d),
                _ => continue,
            };
            out.push((i, origin, Some(upair(&d.envelope.from)), upair(&d.envelope.to), d.envelope.message.clone()));
        }
    };
    let mut out = Vec::new();
    match tx {
        LeeTransaction::Public(p) => {
            let root = &p.message().execution.root;
            out.push((i, Origin::PublicRoot, None, upair(&root.to), root.message.clone()));
        }
        LeeTransaction::PrivacyPreserving(pp) => {
            let ex = &pp.message().execution;
            steps(&ex.boundary, false, &mut out);
            if let Some(root) = &ex.public_root {
                out.push((i, Origin::PrivacyRoot, None, upair(&root.to), root.message.clone()));
            }
            if let Some(fallback) = ex.fee.as_ref().and_then(|f| f.fallback.as_ref()) {
                steps(&fallback.boundary, true, &mut out);
            }
        }
    }
    out
}

/// Parses `bytes` with the core and checks every field against LEZ's own decoding of `block`.
fn check_block(block: &Block, bytes: &[u8]) -> Record {
    assert_eq!(bytes, borsh::to_vec(block).unwrap(), "bytes are borsh(Block)");
    let mut rec = Record::default();
    let s = parse_block(bytes, &mut rec).expect("core parses a LEZ block");
    assert_eq!(s.header.block_id, block.header.block_id);
    assert_eq!(s.header.prev_block_hash, block.header.prev_block_hash.0);
    assert_eq!(s.header.hash, block.header.hash.0);
    assert_eq!(s.header.timestamp, block.header.timestamp);
    assert_eq!(hash_parsed_block(bytes).unwrap(), block.recompute_hash().0, "block hash = LEZ recompute_hash");
    assert_eq!(s.tx_count as usize, block.body.transactions.len());
    assert_eq!(s.reverted, block.body.reverted);
    let status = match block.bedrock_status {
        BedrockStatus::Pending => 0,
        BedrockStatus::Safe => 1,
        BedrockStatus::Finalized => 2,
    };
    assert_eq!(s.status, status);
    let mut want_calls = Vec::new();
    for (i, tx) in block.body.transactions.iter().enumerate() {
        let i = u32::try_from(i).unwrap();
        let (index, kind, span) = &rec.txs[i as usize];
        assert_eq!(*index, i);
        assert_eq!(
            *kind,
            if matches!(tx, LeeTransaction::Public(_)) { TxKind::Public } else { TxKind::PrivacyPreserving }
        );
        assert_eq!(span, &borsh::to_vec(tx).unwrap(), "tx {i}: span = borsh(LeeTransaction)");
        want_calls.extend(expected_calls(i, tx));
    }
    assert_eq!(rec.calls, want_calls, "visible calls");
    rec
}

#[test]
fn empty_blocks_match_lez() {
    let mut chain = Chain::genesis().unwrap();
    for _ in 0..4 {
        chain.push_valid(Vec::new()).unwrap();
    }
    for (i, block) in chain.blocks.iter().enumerate() {
        let bytes = chain.inscribed(i);
        let rec = check_block(block, &bytes);
        // An empty block holds exactly the forced fee and clock transactions.
        assert_eq!(rec.txs.len(), 2);
        println!(
            "empty block {}: {} bytes (fee tx {} bytes, clock tx {} bytes)",
            block.header.block_id,
            bytes.len(),
            rec.txs[0].2.len(),
            rec.txs[1].2.len()
        );
    }
}

#[test]
fn public_transactions_match_lez() {
    let dep = Deployment::new(4, 1, 1);
    let mut chain = Chain::genesis().unwrap();
    // Executable: native transfers between testnet genesis accounts.
    let accounts = testnet_initial_state::initial_pub_accounts_private_keys();
    let (a, to) = (&accounts[0].pub_sign_key, accounts[1].account_id);
    chain.push_valid(vec![native_transfer(a, 0, to, 10), native_transfer(a, 1, to, 20)]).unwrap();
    // Stand-in: bridge calls (the bridge program does not exist), one of them listed as reverted.
    let user = private_key(7);
    let txs = vec![
        public_call(&user, 0, dep.bridge(), dep.burn(0, None).encode()),
        public_call(&user, 1, dep.bridge(), dep.import_root(5).encode()),
        public_call(&user, 2, dep.bridge(), BridgeMessage::Echo { nonce: [9; 32] }.encode()),
        public_call(&user, 3, dep.bridge(), vec![0xFF, 1, 2]),
    ];
    chain.push_stand_in(txs, vec![1]).unwrap();
    for (i, block) in chain.blocks.iter().enumerate() {
        check_block(block, &chain.inscribed(i));
    }
}

#[test]
fn privacy_preserving_shapes_match_lez() {
    let dep = Deployment::new(4, 1, 1);
    let bridge = dep.bridge();
    let private_actor = Actor::new(AccountId::new([0xA1; 32]), AccountId::new(label("token program")));
    let fee_inbox = Actor::native_balance(AccountId::new([0xFE; 32]));
    let image: ProgramId = [7, 6, 5, 4, 3, 2, 1, 0];
    let shapes = [
        // A burn from a private holding: the private token actor sends Burn to the bridge.
        PpSpec {
            deliveries: vec![
                (true, private_actor, bridge, dep.burn(1, None).encode()),
                (true, private_actor, fee_inbox, vec![1, 2, 3]),
            ],
            ..PpSpec::default()
        },
        // A public root call to the bridge inside a privacy-preserving transaction, a reply to a
        // private actor, sealed messages, both program claim kinds, and a fee fallback whose
        // boundary also addresses the bridge.
        PpSpec {
            deliveries: vec![(false, bridge, private_actor, b"reply".to_vec())],
            public_root: Some((bridge, dep.import_root(9).encode())),
            sealed_messages: 2,
            recovery_bindings: 2,
            claims: vec![
                ProgramImageClaim::Disclosed { account_id: AccountId::new([3; 32]), image_id: image },
                ProgramImageClaim::Undisclosed { root: [4; 32] },
            ],
            fallback: Some(vec![(true, private_actor, bridge, dep.burn(2, None).encode())]),
            proof_len: 1000,
            ..PpSpec::default()
        },
        // No boundary at all, empty everything, large notes.
        PpSpec { actions: 8, note_len: 8192, recovery_bindings: 0, proof_len: PROOF_BYTES, ..PpSpec::default() },
    ];
    let mut chain = Chain::genesis().unwrap();
    let txs = shapes.iter().enumerate().map(|(i, s)| pp_stand_in(s, u8::try_from(i).unwrap())).collect();
    chain.push_stand_in(txs, vec![]).unwrap();
    let rec = check_block(&chain.blocks[1], &chain.inscribed(1));
    let origins: Vec<Origin> = rec.calls.iter().map(|c| c.1).collect();
    assert!(origins.contains(&Origin::PrivacyRoot));
    assert!(origins.contains(&Origin::FallbackPrivateToPublic));
    assert!(origins.contains(&Origin::PublicToPrivate));
}

#[test]
fn real_blocks_from_the_lez_sequencer_dump() {
    let blocks = dump_blocks().unwrap();
    assert!(!blocks.is_empty());
    let (mut pp, mut public) = (0, 0);
    for (k, (bytes, block)) in blocks.iter().enumerate() {
        let rec = check_block(block, bytes);
        if k > 0 {
            // A chain: each block links to the one before it and carries the next id.
            let prev = &blocks[k - 1].1;
            assert_eq!(block.header.block_id, prev.header.block_id + 1);
            assert_eq!(block.header.prev_block_hash, prev.header.hash);
        }
        println!("dump block {}: {} bytes, status {:?}", block.header.block_id, bytes.len(), block.bedrock_status);
        for (_, kind, span) in &rec.txs {
            match kind {
                TxKind::Public => public += 1,
                TxKind::PrivacyPreserving => {
                    pp += 1;
                    println!("dump block {}: privacy-preserving tx of {} bytes", block.header.block_id, span.len());
                }
            }
        }
    }
    println!(
        "LEZ dump: {} blocks (ids {}..={}), {public} public and {pp} privacy-preserving transactions",
        blocks.len(),
        blocks[0].1.header.block_id,
        blocks.last().unwrap().1.header.block_id
    );
    assert!(pp > 0, "the dump holds privacy-preserving transactions");
    // Their message part as LEZ's wallet made it, with a full-size proof, still parses exactly.
    for (_, block) in &blocks {
        for tx in &block.body.transactions {
            if let Some(big) = resize_proof(tx, PROOF_BYTES) {
                let mut chain = Chain::genesis().unwrap();
                chain.push_stand_in(vec![big], vec![]).unwrap();
                check_block(&chain.blocks[1], &chain.inscribed(1));
                return;
            }
        }
    }
}

/// Mutated blocks: the core accepts exactly what LEZ's decoder accepts, except for two
/// documented differences (README): the core does not validate secp256k1 public keys (LEZ's
/// decoder does), and it refuses a `reverted` list that is not strictly ascending or names a
/// missing transaction (LEZ's decoder accepts it; `apply_block` refuses it).
#[test]
fn strictness_matches_lez_decoder_on_mutations() {
    let mut chain = Chain::genesis().unwrap();
    let user = private_key(7);
    let dep = Deployment::new(4, 1, 1);
    chain.push_stand_in(vec![public_call(&user, 0, dep.bridge(), dep.burn(0, None).encode())], vec![]).unwrap();
    let pp = PpSpec {
        deliveries: vec![(true, dep.bridge(), dep.bridge(), vec![1])],
        proof_len: 64,
        actions: 2,
        note_len: 8,
        ..PpSpec::default()
    };
    chain.push_stand_in(vec![pp_stand_in(&pp, 1)], vec![0]).unwrap();
    let mut samples: Vec<Vec<u8>> = (0..chain.blocks.len()).map(|i| chain.inscribed(i)).collect();
    samples.extend(dump_blocks().unwrap().into_iter().take(3).map(|(b, _)| b));
    let (mut agree, mut key_leniency, mut reverted_strictness, mut total) = (0, 0, 0, 0);
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    for bytes in &samples {
        for _ in 0..1500 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let mut m = bytes.clone();
            let pos = (seed >> 33) as usize % m.len();
            match (seed >> 20) % 4 {
                0 => m[pos] ^= 1 << ((seed >> 8) % 8),
                1 => m[pos] = m[pos].wrapping_add(1),
                2 => m.truncate(pos),
                _ => m.insert(pos, (seed >> 12) as u8),
            }
            total += 1;
            let up = borsh::from_slice::<Block>(&m);
            let ours = parse_block(&m, &mut NoVisitor);
            match (ours.is_ok(), up.is_ok()) {
                (true, true) | (false, false) => agree += 1,
                (true, false) => {
                    let e = up.unwrap_err().to_string();
                    assert!(
                        e.to_lowercase().contains("key") || e.to_lowercase().contains("point"),
                        "core accepted what LEZ refuses: {e}"
                    );
                    key_leniency += 1;
                }
                (false, true) => {
                    assert!(
                        matches!(ours, Err(checkpoint_core::Error::BadRevertedList { .. })),
                        "core refused what LEZ accepts: {ours:?}"
                    );
                    reverted_strictness += 1;
                }
            }
        }
    }
    println!("mutations: {total}; agree {agree}; core lenient on secp256k1 keys {key_leniency}; core strict on reverted {reverted_strictness}");
    assert!(agree * 100 >= total * 95);
}

// ---- Logos Blockchain transactions -----------------------------------------------------------------

#[test]
fn inscriptions_match_logos_blockchain() {
    let seq = Sequencer::from_seed(3);
    let channel = [1u8; 32];
    let mut chain = Chain::genesis().unwrap();
    chain.push_valid(Vec::new()).unwrap();
    let payload = chain.inscribed(1);
    let parent = label("previous inscription");
    for extra in [Extra::None, Extra::ChannelTransfer, Extra::ChannelWithdraw] {
        let tx = inscribe(&seq, channel, &payload, parent, extra, 5);
        let want_ops = match extra {
            Extra::None => vec![0x11, 0x00],
            Extra::ChannelTransfer => vec![0x11, 0x14, 0x00],
            Extra::ChannelWithdraw => vec![0x11, 0x13, 0x00],
        };
        assert_eq!(opcodes(&tx.ops).unwrap(), want_ops);
        assert_eq!(tx_hash(&tx.ops), tx.tx_hash, "tx hash = Ops::hash()");
        assert_eq!(
            inscription_msg_id(&channel, &payload, &parent, &seq.public()),
            tx.op_id,
            "MsgId = InscriptionOp::id()"
        );
        let e = ElidedInscription::split(&tx.ops, 0, &tx.signature).unwrap();
        assert_eq!([e.prefix, payload.as_slice(), e.suffix].concat(), tx.ops);
        let checked = e.check(&payload, &channel).unwrap();
        assert_eq!((checked.signer, checked.parent, checked.tx_hash), (seq.public(), parent, tx.tx_hash));
    }
}

#[test]
fn channel_config_matches_logos_blockchain() {
    let seqs: Vec<Sequencer> = (10..15).map(Sequencer::from_seed).collect();
    let refs: Vec<&Sequencer> = seqs.iter().collect();
    let c = config_change([1; 32], label("config tip"), &refs, 4, &[(0, &seqs[0]), (2, &seqs[2])]);
    let change =
        ConfigChange { ops: &c.tx.ops, op_index: 0, signatures: c.signatures.iter().map(|(i, s)| (*i, s)).collect() };
    let (op, hash) = change.op().unwrap();
    assert_eq!(hash, c.tx.tx_hash);
    assert_eq!(op.id(), c.tx.op_id, "id = ChannelConfigOp::id()");
    assert_eq!((op.channel, op.parent), (&[1u8; 32], &label("config tip")));
    assert_eq!(op.keys.iter().map(|k| **k).collect::<Vec<_>>(), seqs.iter().map(Sequencer::public).collect::<Vec<_>>());
    assert_eq!(
        (op.posting_timeframe, op.posting_timeout, op.configuration_threshold, op.transfer_threshold),
        (30, 60, 4, 4)
    );
    for (i, sig) in &c.signatures {
        checkpoint_core::mantle::verify_strict(&seqs[usize::from(*i)].public(), &hash, sig).unwrap();
    }
}
