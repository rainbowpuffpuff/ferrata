//! The LEZ v0.4.0 build (feature `lez-0-4-0`, `src/lez_0_4_0/`).
//!
//! - Over the 107 LEZ blocks a real v0.4.0 zone inscribed (four sequencer keys on a Logos Blockchain 0.4.0-rc.2 node,
//!   `tests/data/zone-v040-four-keys-1/`, taken from `e2e/runs/zone-v040-four-keys-1/answers`): every block parses, its
//!   recomputed hash is the hash it carries, the chain links, the visible calls are reported; the default parser
//!   refuses them; mutations of them are refused with the error for what was broken.
//! - Their 107 inscriptions check under the `bedrock-0-3-5` Mantle layout (fork digest, hash, Ed25519 signature,
//!   accredited signer, parent chain), and rule (A), rule (B) and segments run over the real blocks and inscriptions.
//! - What the zone's blocks do not hold (privacy-preserving transactions, fee declarations, keyed evidence, failed
//!   receipts) over upstream's pinned layouts at v0.4.0, with each refusal.
#![cfg(feature = "lez-0-4-0")]
mod common;

use attestor_core::{sign_certificate, Deployment, FinalTip, SignedAttestation};
use checkpoint_core::bedrock_0_3_5::{self, mantle as mantle_0_3_5};
use checkpoint_core::hash::sha256_parts;
use checkpoint_core::input::{CheckpointInput, CheckpointInputB, SegmentInput, Support};
use checkpoint_core::lez_0_4_0::{
    hash_parsed_block, parse_block, rule, Actor, BlockSummary, NoVisitor, Origin, TxKind, Visitor, BLOCK_HASH_PREFIX,
    HEADER_LEN,
};
use checkpoint_core::mantle::{inscription_msg_id, ElidedInscription};
use checkpoint_core::state::{key_set_hash, Config, State};
use checkpoint_core::tree::Frontier;
use checkpoint_core::{Error, Hash};
use common::{key, u32le};
use ed25519_dalek::SigningKey;
use serde_json::Value;

// ---- the zone's data ------------------------------------------------------------------------------

const BLOCKS_HEX: &str = include_str!("data/zone-v040-four-keys-1/blocks.hex");
const BEDROCK_JSON: &str = include_str!("data/zone-v040-four-keys-1/bedrock.json");

/// One inscription as the node holds it: the transaction split around the inscribed block.
struct Inscription {
    tx_hash: Hash,
    op_index: u64,
    prefix: Vec<u8>,
    suffix: Vec<u8>,
    signature: [u8; 64],
}

impl Inscription {
    fn elided(&self) -> ElidedInscription<'_> {
        ElidedInscription { prefix: &self.prefix, suffix: &self.suffix, signature: &self.signature }
    }
    /// The signer the inscription op names (the suffix starts with its `parent`, then its `signer`).
    fn signer(&self) -> Hash {
        self.suffix[32..64].try_into().unwrap()
    }
}

struct Zone {
    blocks: Vec<Vec<u8>>,
    inscriptions: Vec<Inscription>,
    channel_id: Hash,
    fork_digest: Hash,
    accredited: Vec<Hash>,
    posting_timeframe: u32,
    config_tip_hash: Hash,
    tip_message: Hash,
}

fn h32(v: &Value) -> Hash {
    hex::decode(v.as_str().unwrap()).unwrap().try_into().unwrap()
}

fn zone() -> Zone {
    let blocks: Vec<Vec<u8>> = BLOCKS_HEX.lines().map(|l| hex::decode(l).unwrap()).collect();
    let v: Value = serde_json::from_str(BEDROCK_JSON).unwrap();
    let inscriptions: Vec<Inscription> = v["inscriptions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| Inscription {
            tx_hash: h32(&i["tx_hash"]),
            op_index: i["op_index"].as_u64().unwrap(),
            prefix: hex::decode(i["prefix"].as_str().unwrap()).unwrap(),
            suffix: hex::decode(i["suffix"].as_str().unwrap()).unwrap(),
            signature: hex::decode(i["signature"].as_str().unwrap()).unwrap().try_into().unwrap(),
        })
        .collect();
    assert_eq!(blocks.len(), inscriptions.len());
    let channel = &v["channel"];
    Zone {
        blocks,
        inscriptions,
        channel_id: h32(&v["channel_id"]),
        fork_digest: h32(&v["fork_digest"]),
        accredited: channel["accredited_keys"].as_array().unwrap().iter().map(h32).collect(),
        posting_timeframe: u32::try_from(channel["posting_timeframe"].as_u64().unwrap()).unwrap(),
        config_tip_hash: h32(&channel["config_tip_hash"]),
        tip_message: h32(&channel["tip_message"]),
    }
}

/// A visible call as reported: transaction index, origin, sender, target, message.
type Call<'a> = (u32, Origin, Option<Actor<'a>>, Actor<'a>, &'a [u8]);

/// Everything the parser reports.
#[derive(Default)]
struct Record<'a> {
    txs: Vec<(u32, TxKind, &'a [u8])>,
    calls: Vec<Call<'a>>,
}

impl<'a> Visitor<'a> for Record<'a> {
    fn tx(&mut self, index: u32, kind: TxKind, bytes: &'a [u8]) {
        self.txs.push((index, kind, bytes));
    }
    fn call(&mut self, index: u32, origin: Origin, from: Option<Actor<'a>>, to: Actor<'a>, message: &'a [u8]) {
        self.calls.push((index, origin, from, to, message));
    }
}

fn parse(bytes: &[u8]) -> Result<BlockSummary, Error> {
    parse_block(bytes, &mut NoVisitor)
}

/// Offset of `part` (a slice the parser handed out) in `base`.
fn offset(base: &[u8], part: &[u8]) -> usize {
    let (b, p) = (base.as_ptr() as usize, part.as_ptr() as usize);
    assert!(p >= b && p + part.len() <= b + base.len(), "not a sub-slice");
    p - b
}

/// The offsets of transaction `index`'s span in `block`.
fn tx_span(block: &[u8], index: u32) -> (usize, usize) {
    let mut rec = Record::default();
    parse_block(block, &mut rec).unwrap();
    let (_, _, bytes) = rec.txs[index as usize];
    let start = offset(block, bytes);
    (start, start + bytes.len())
}

/// Where the receipt list starts: right after the last transaction.
fn receipts_at(block: &[u8]) -> usize {
    let mut rec = Record::default();
    parse_block(block, &mut rec).unwrap();
    let (_, _, last) = rec.txs.last().unwrap();
    offset(block, last) + last.len()
}

// ---- the zone's blocks ------------------------------------------------------------------------------

#[test]
fn every_inscribed_block_parses_rehashes_and_links() {
    let z = zone();
    assert_eq!(z.blocks.len(), 107);
    // Unchanged since v0.4.0-rc1: the hash's domain and the header.
    assert_eq!(BLOCK_HASH_PREFIX, checkpoint_core::lez::BLOCK_HASH_PREFIX);
    assert_eq!(HEADER_LEN, checkpoint_core::lez::HEADER_LEN);
    let (mut prev_hash, mut calls_total) = ([0u8; 32], 0usize);
    let mut system_programs: Option<(Hash, Hash)> = None;
    for (i, b) in z.blocks.iter().enumerate() {
        let mut rec = Record::default();
        let s = parse_block(b, &mut rec).unwrap_or_else(|e| panic!("block {}: {e:?}", i + 1));
        // The chain: ids from the genesis block 1, each linked to the one before (block 1 to zero).
        assert_eq!(s.header.block_id, i as u64 + 1);
        assert_eq!(s.header.prev_block_hash, prev_hash, "block {}", i + 1);
        let hash = hash_parsed_block(b).unwrap();
        assert_eq!(hash, s.header.hash, "block {}: recomputed hash", i + 1);
        // The same byte ranges as the default build's hash.
        assert_eq!(hash, checkpoint_core::lez::hash_parsed_block(b).unwrap());
        prev_hash = hash;
        // As inscribed: Pending. The genesis block holds 13 user transactions, each block the fee and clock ones.
        assert_eq!(s.status, 0);
        let (txs, user) = if i == 0 { (15, 13) } else { (2, 0) };
        assert_eq!((s.tx_count, s.receipts.len()), (txs, user), "block {}", i + 1);
        assert!(s.receipts.iter().all(|r| r.success && r.fee.fee_base == 0));
        assert!((0..txs + 1).all(|t| !s.is_reverted(t)));
        // Spans: exactly the transaction list, back to back, each a public transaction.
        assert_eq!(rec.txs.len(), txs as usize);
        let mut at = HEADER_LEN + 4;
        for (k, (index, kind, bytes)) in rec.txs.iter().enumerate() {
            assert_eq!((*index as usize, *kind), (k, TxKind::Public));
            assert_eq!(offset(b, bytes), at);
            at += bytes.len();
        }
        assert_eq!(at + 4 + 65 * s.receipts.len() + 1, b.len());
        // Visible calls: each transaction's root call, inside its own span, in transaction order.
        assert_eq!(rec.calls.len(), txs as usize);
        for (k, (index, origin, from, _, message)) in rec.calls.iter().enumerate() {
            assert_eq!((*index as usize, *origin, from.is_none()), (k, Origin::PublicRoot, true));
            let (span_start, span_end) = tx_span(b, *index);
            let m = offset(b, message);
            assert!(m >= span_start && m + message.len() <= span_end);
        }
        calls_total += rec.calls.len();
        // The last two transactions call the same two programs in every block (the fee, then the clock).
        let last_two =
            (*rec.calls[txs as usize - 2].3.program_account_id, *rec.calls[txs as usize - 1].3.program_account_id);
        assert_eq!(*system_programs.get_or_insert(last_two), last_two, "block {}", i + 1);
    }
    assert_eq!(calls_total, 15 + 106 * 2);
}

#[test]
fn the_default_parser_refuses_the_v040_blocks() {
    let z = zone();
    for b in &z.blocks {
        assert!(checkpoint_core::lez::parse_block(b, &mut checkpoint_core::lez::NoVisitor).is_err());
    }
    // What the attestor's run over these answers reported (`e2e/runs/zone-v040-four-keys-1/attest-run.txt`): block 2's
    // actors carry an authority byte each, which v0.4.0-rc1's layout reads as part of the next actor.
    assert_eq!(
        checkpoint_core::lez::parse_block(&z.blocks[1], &mut checkpoint_core::lez::NoVisitor),
        Err(Error::NotStrictlyOrdered { what: "actors" })
    );
}

#[test]
fn mutated_zone_blocks_are_refused_with_the_error_for_what_broke() {
    let z = zone();
    let genesis = &z.blocks[0];
    let block2 = &z.blocks[1];

    // A reordered actor map: block 2's fee transaction declares four actors; swap the first two entries.
    let (s, _) = tx_span(block2, 0);
    assert_eq!(&block2[s..s + 5], &[0, 4, 0, 0, 0]);
    let mut b = block2.clone();
    let (first, second) = (s + 5, s + 5 + 65);
    let entry: Vec<u8> = b[first..second].to_vec();
    b.copy_within(second..second + 65, first);
    b[second..second + 65].copy_from_slice(&entry);
    assert_eq!(parse(&b), Err(Error::NotStrictlyOrdered { what: "actors" }));
    // A repeated actor is refused too (strictly ascending, as borsh's `de_strict_order`).
    let mut b = block2.clone();
    b.copy_within(first..second, second);
    assert_eq!(parse(&b), Err(Error::NotStrictlyOrdered { what: "actors" }));

    // An Authority tag of 2 (the first actor's: None = 0, Signer = 1).
    let mut b = block2.clone();
    assert_eq!(b[first + 64], 0);
    b[first + 64] = 2;
    assert_eq!(parse(&b), Err(Error::BadTag { what: "Authority", tag: 2 }));

    // The genesis block's receipt list: 13 receipts of 65 bytes, then the status byte.
    let r = receipts_at(genesis);
    assert_eq!(&genesis[r..r + 4], &u32le(13));
    assert_eq!(r + 4 + 13 * 65 + 1, genesis.len());
    // Truncated inside the list: the length names more receipts than the bytes hold.
    for cut in [r + 4, r + 4 + 65, r + 4 + 12 * 65 + 10, r + 4 + 13 * 65 - 1] {
        assert_eq!(parse(&genesis[..cut]), Err(Error::BadLength { what: "receipts" }), "cut at {cut}");
    }
    // Inside the length itself, or with the list whole and the status byte cut.
    assert_eq!(parse(&genesis[..r + 2]), Err(Error::Truncated { at: r }));
    assert_eq!(parse(&genesis[..genesis.len() - 1]), Err(Error::Truncated { at: genesis.len() - 1 }));
    // A shorter list that is well formed: one user transaction has no receipt. Refused beyond borsh.
    let mut b = genesis[..r].to_vec();
    b.extend_from_slice(&u32le(12));
    b.extend_from_slice(&genesis[r + 4..r + 4 + 12 * 65]);
    b.push(0);
    assert_eq!(parse(&b), Err(Error::BadRevertedList { block: 1 }));
    // So is a longer one (a receipt for the fee transaction).
    let mut b = genesis[..r].to_vec();
    b.extend_from_slice(&u32le(14));
    b.extend_from_slice(&genesis[r + 4..r + 4 + 13 * 65]);
    b.extend_from_slice(&genesis[r + 4..r + 4 + 65]);
    b.push(0);
    assert_eq!(parse(&b), Err(Error::BadRevertedList { block: 1 }));
    // A receipt's `success` byte is a bool.
    let mut b = genesis.clone();
    b[r + 4] = 2;
    assert_eq!(parse(&b), Err(Error::BadTag { what: "receipt success", tag: 2 }));
    // A failed receipt parses, and its transaction reads as reverted (the hash no longer matches, as it covers it).
    let mut b = genesis.clone();
    b[r + 4 + 65 * 5] = 0;
    let s = parse(&b).unwrap();
    assert!(s.is_reverted(5) && !s.is_reverted(4) && !s.is_reverted(13) && !s.is_reverted(14));
    assert_ne!(hash_parsed_block(&b).unwrap(), s.header.hash);

    // Trailing bytes, and a status out of range.
    for b in [genesis, block2] {
        let mut t = b.clone();
        t.push(0);
        assert_eq!(parse(&t), Err(Error::TrailingBytes { at: b.len() }));
        let mut t = b.clone();
        *t.last_mut().unwrap() = 3;
        assert_eq!(parse(&t), Err(Error::BadTag { what: "BedrockStatus", tag: 3 }));
        // The status is not hashed: Finalized parses with the same hash.
        let mut t = b.clone();
        *t.last_mut().unwrap() = 2;
        assert_eq!(parse(&t).unwrap().status, 2);
        assert_eq!(hash_parsed_block(&t).unwrap(), hash_parsed_block(b).unwrap());
    }

    // A witness entry's next key of a key type v0.4.0 does not have (genesis transaction 9 is signed by two accounts;
    // its last 33 bytes are its last entry's `next`).
    let (_, end) = tx_span(genesis, 9);
    let mut b = genesis.clone();
    assert_eq!(b[end - 33], 0);
    b[end - 33] = 1;
    assert_eq!(parse(&b), Err(Error::BadTag { what: "KeyType", tag: 1 }));

    // A LeeTransaction tag out of range.
    let mut b = block2.clone();
    b[HEADER_LEN + 4] = 2;
    assert_eq!(parse(&b), Err(Error::BadTag { what: "LeeTransaction", tag: 2 }));

    // Any change to a hashed byte: the block still parses, its recomputed hash is no longer the one it carries.
    let mut rec = Record::default();
    parse_block(block2, &mut rec).unwrap();
    let message_at = offset(block2, rec.calls[1].4);
    let mut b = block2.clone();
    b[message_at] ^= 1;
    let s = parse(&b).unwrap();
    assert_ne!(hash_parsed_block(&b).unwrap(), s.header.hash);

    // Every proper prefix of a block is refused.
    for cut in 0..block2.len() {
        assert!(parse(&block2[..cut]).is_err(), "prefix of {cut} bytes");
    }
}

// ---- the zone's inscriptions under the bedrock-0-3-5 Mantle layout ------------------------------------

#[test]
fn every_inscription_checks_in_the_0_3_5_layout_with_the_zones_fork_digest() {
    let z = zone();
    let mut parent = [0u8; 32];
    let mut per_key = std::collections::BTreeMap::<Hash, u32>::new();
    for (k, (block, ins)) in z.blocks.iter().zip(&z.inscriptions).enumerate() {
        let tx = [ins.prefix.as_slice(), block, &ins.suffix].concat();
        // The node's transaction, as 0.3.5-rc.1 frames it: the zone's fork digest, then the operations.
        assert_eq!(mantle_0_3_5::fork_digest_of(&tx).unwrap(), &z.fork_digest);
        let ops = mantle_0_3_5::opcodes(&tx).unwrap();
        // The first transaction also creates the channel ([ChannelConfig, ChannelInscribe, Transfer]).
        let expected: &[u8] = if k == 0 { &[0x10, 0x11, 0x00] } else { &[0x11, 0x00] };
        assert_eq!(ops, expected);
        assert_eq!(ins.op_index, if k == 0 { 1 } else { 0 });
        // The program's check: framing, hash over the digest and the operations (the node's hash), verify_strict.
        let checked = mantle_0_3_5::check(&ins.elided(), block, &z.channel_id, &z.fork_digest)
            .unwrap_or_else(|e| panic!("inscription of block {}: {e:?}", k + 1));
        assert_eq!(checked.tx_hash, ins.tx_hash, "block {}", k + 1);
        assert!(z.accredited.contains(&checked.signer));
        // The channel's message chain: each inscription's parent is the one before's MsgId.
        assert_eq!(checked.parent, parent, "block {}", k + 1);
        parent = inscription_msg_id(&z.channel_id, block, &checked.parent, &checked.signer);
        *per_key.entry(checked.signer).or_default() += 1;
        // Another fork digest refuses it.
        assert_eq!(
            mantle_0_3_5::check(&ins.elided(), block, &z.channel_id, &[0xd2; 32]).unwrap_err(),
            bedrock_0_3_5::Error::WrongForkDigest
        );
    }
    // The chain ends at the channel's tip message; every key inscribed (seq-0 38, the others 23 each).
    assert_eq!(parent, z.tip_message);
    let counts: Vec<u32> = z.accredited.iter().map(|k| per_key[k]).collect();
    assert_eq!(counts, [38, 23, 23, 23]);
}

// ---- the rules over the zone --------------------------------------------------------------------------

fn uint256(x: u64) -> Hash {
    let mut w = [0u8; 32];
    w[24..].copy_from_slice(&x.to_be_bytes());
    w
}

/// A deployment over the zone: its fork digest as `bedrockGenesisId` (as the `bedrock-0-3-5` build pins it), its
/// channel; the bridge fields are labels (the zone runs no bridge program).
fn config(z: &Zone) -> Config {
    Config {
        bedrock_genesis_id: z.fork_digest,
        channel_id: z.channel_id,
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

/// The state before the genesis block, with key set `keys`.
fn genesis_state(z: &Zone, keys: Vec<Hash>) -> State {
    State {
        tip_id: 0,
        tip_hash: [0; 32],
        keys,
        posting_timeframe: z.posting_timeframe,
        config_tip_hash: z.config_tip_hash,
        burns: Frontier::default(),
        imported_leaf_count: 0,
        sunset: false,
    }
}

/// `T` is block 24 (index 23), the last of seq-0's first turn.
const T: usize = 23;

fn pk(k: &SigningKey) -> Hash {
    k.verifying_key().to_bytes()
}

#[test]
fn rule_a_over_the_zone_with_its_own_keys_and_signatures() {
    let z = zone();
    let c = config(&z);
    let producer = z.inscriptions[T].signer();
    // Support: the first inscription after T by each of the three other keys (blocks 25, 48 and 71).
    let mut support_at: Vec<usize> = Vec::new();
    for k in T + 1..z.inscriptions.len() {
        let signer = z.inscriptions[k].signer();
        if signer != producer && support_at.iter().all(|&j| z.inscriptions[j].signer() != signer) {
            support_at.push(k);
        }
    }
    assert_eq!(support_at, [24, 47, 70]);
    let end = support_at[2];
    let input = CheckpointInput {
        config: c.clone(),
        parent: genesis_state(&z, z.accredited.clone()),
        config_changes: Vec::new(),
        blocks: z.blocks[..=end].iter().map(Vec::as_slice).collect(),
        t_index: T as u32,
        echo_nonce: [0; 32],
        producer: z.inscriptions[T].elided(),
        support: support_at
            .iter()
            .map(|&k| Support { block_index: k as u32, inscription: z.inscriptions[k].elided() })
            .collect(),
        segments: Vec::new(),
    };
    // As the guest receives it.
    let bytes = input.encode();
    let input = CheckpointInput::decode(&bytes).unwrap();
    assert_eq!(input.encode(), bytes);
    let out = rule::run_checkpoint(&input).unwrap();
    let t_hash = parse(&z.blocks[T]).unwrap().header.hash;
    assert_eq!((out.producer, out.support_keys, out.threshold, out.y_index), (producer, 3, 3, T as u32));
    let j = &out.journal;
    assert_eq!((j.tip_id, j.tip_hash, j.n, j.t, j.burn_count), (24, t_hash, 4, 3, 0));
    assert_eq!(j.parent_state, genesis_state(&z, z.accredited.clone()).digest());
    // Two of three supporters are not enough.
    let mut short = input.clone();
    short.support.pop();
    assert_eq!(
        rule::run_checkpoint(&short).unwrap_err(),
        bedrock_0_3_5::Error::Core(Error::InsufficientSupport { have: 2, need: 3 })
    );
    // Neither the default build nor the bedrock-0-3-5 build reads these blocks.
    assert!(checkpoint_core::rule::run_checkpoint(&input).is_err());
    assert!(bedrock_0_3_5::rule::run_checkpoint(&input).is_err());
}

/// Rule (B)'s attestations under the key set `keys`, by `attestors` (channel key, attestation key) naming the blocks
/// `tips` (indices into the zone's blocks).
fn attestations(
    c: &Config,
    z: &Zone,
    keys: &[Hash],
    attestors: &[(SigningKey, SigningKey)],
    tips: &[usize],
) -> Vec<SignedAttestation> {
    let d = Deployment {
        evm_chain_id: c.evm_chain_id,
        vault: c.vault,
        lez_config_id: c.config_id(),
        channel_id: c.channel_id,
    };
    let ksh = key_set_hash(keys, z.posting_timeframe);
    attestors
        .iter()
        .zip(tips)
        .map(|((ch, a), &tip)| {
            let header = parse(&z.blocks[tip]).unwrap().header;
            let tip = FinalTip { tip_id: header.block_id, tip_hash: header.hash, final_msg_id: [0x5e; 32] };
            SignedAttestation::new(a, pk(ch), sign_certificate(ch, &d, &pk(a)), &d, &ksh, tip)
        })
        .collect()
}

/// The producer of `T` (seq-0, a real key with its real inscription) and three stand-in channel keys whose
/// attestation keys attest `T` and the two blocks after it: the zone's own keys cannot certify attestation keys here.
fn rule_b_setup(z: &Zone) -> (Vec<Hash>, Vec<(SigningKey, SigningKey)>) {
    let attestors: Vec<(SigningKey, SigningKey)> = (1..=3).map(|i| (key(0x10 + i), key(0x80 + i))).collect();
    let mut keys = vec![z.inscriptions[T].signer()];
    keys.extend(attestors.iter().map(|(ch, _)| pk(ch)));
    (keys, attestors)
}

fn input_b<'a>(
    c: &Config,
    z: &'a Zone,
    parent: State,
    from: usize,
    to: usize,
    atts: Vec<SignedAttestation>,
) -> CheckpointInputB<'a> {
    CheckpointInputB {
        config: c.clone(),
        parent,
        config_changes: Vec::new(),
        blocks: z.blocks[from..=to].iter().map(Vec::as_slice).collect(),
        t_index: (T - from) as u32,
        producer: z.inscriptions[T].elided(),
        attestations: atts,
        segments: Vec::new(),
    }
}

#[test]
fn rule_b_over_the_zone_with_the_real_producer() {
    let z = zone();
    let c = config(&z);
    let (keys, attestors) = rule_b_setup(&z);
    let atts = attestations(&c, &z, &keys, &attestors, &[T, T + 1, T + 2]);
    let input = input_b(&c, &z, genesis_state(&z, keys.clone()), 0, T + 2, atts.clone());
    let bytes = input.encode();
    let input = CheckpointInputB::decode(&bytes).unwrap();
    assert_eq!(input.encode(), bytes);
    let out = rule::run_checkpoint_b(&input).unwrap();
    let t_hash = parse(&z.blocks[T]).unwrap().header.hash;
    assert_eq!((out.producer, out.attestors, out.threshold, out.path_after_tip), (keys[0], 3, 3, 2));
    let j = &out.journal;
    assert_eq!((j.tip_id, j.tip_hash, j.n, j.t, j.burn_count, j.echo_nonce), (24, t_hash, 4, 3, 0, [0; 32]));

    // Neither the default build nor the bedrock-0-3-5 build reads these blocks.
    assert!(checkpoint_core::rule_b::run_checkpoint_b(&input).is_err());
    assert!(bedrock_0_3_5::rule::run_checkpoint_b(&input).is_err());

    // A deployment pinned to another fork digest refuses the zone's inscription of T (attestations re-made for it).
    let mut other = c.clone();
    other.bedrock_genesis_id = [0xd2; 32];
    let atts_other = attestations(&other, &z, &keys, &attestors, &[T, T + 1, T + 2]);
    let wrong = input_b(&other, &z, genesis_state(&z, keys.clone()), 0, T + 2, atts_other);
    assert_eq!(rule::run_checkpoint_b(&wrong).unwrap_err(), bedrock_0_3_5::Error::WrongForkDigest);

    // A producer outside the key set: the three stand-ins and another key.
    let mut outsiders = keys.clone();
    outsiders[0] = pk(&key(0x40));
    let atts_out = attestations(&c, &z, &outsiders, &attestors, &[T, T + 1, T + 2]);
    let refused = input_b(&c, &z, genesis_state(&z, outsiders), 0, T + 2, atts_out);
    assert_eq!(rule::run_checkpoint_b(&refused).unwrap_err(), bedrock_0_3_5::Error::Core(Error::NotAccredited));

    // A block after T that does not hash to what it carries.
    let mut tampered: Vec<Vec<u8>> = z.blocks[..=T + 2].to_vec();
    tampered[T + 1][HEADER_LEN - 1] ^= 1; // its timestamp's top byte
    let mut input = input_b(&c, &z, genesis_state(&z, keys.clone()), 0, T + 2, atts.clone());
    input.blocks = tampered.iter().map(Vec::as_slice).collect();
    assert_eq!(
        rule::run_checkpoint_b(&input).unwrap_err(),
        bedrock_0_3_5::Error::Core(Error::HashMismatch { index: (T + 1) as u32 })
    );
    // An attestation of a block past the input's last.
    let atts_far = attestations(&c, &z, &keys, &attestors, &[T, T + 1, T + 3]);
    let far = input_b(&c, &z, genesis_state(&z, keys), 0, T + 2, atts_far);
    assert_eq!(
        rule::run_checkpoint_b(&far).unwrap_err(),
        bedrock_0_3_5::Error::Core(Error::NotADescendant { tip_id: 27 })
    );
}

#[test]
fn a_segment_of_v040_blocks_composes_with_rule_b() {
    let z = zone();
    let c = config(&z);
    let (keys, attestors) = rule_b_setup(&z);
    let genesis = genesis_state(&z, keys.clone());
    // Blocks 1..=12 as a segment.
    let seg_input = SegmentInput {
        config: c.clone(),
        parent: genesis.clone(),
        blocks: z.blocks[..12].iter().map(Vec::as_slice).collect(),
    };
    let bytes = seg_input.encode();
    let seg_input = SegmentInput::decode(&bytes).unwrap();
    let (segment, state12) = rule::run_segment(&seg_input).unwrap();
    assert_eq!((state12.tip_id, state12.tip_hash), (12, parse(&z.blocks[11]).unwrap().header.hash));
    assert_eq!(
        (segment.start_state, segment.end_state, segment.lez_config_id),
        (genesis.digest(), state12.digest(), c.config_id())
    );
    assert!(checkpoint_core::rule::run_segment(&seg_input).is_err());
    // Rule (B) from the segment's end state over blocks 13..=26: the journal of the same checkpoint from genesis.
    let atts = attestations(&c, &z, &keys, &attestors, &[T, T + 1, T + 2]);
    let mut composed = input_b(&c, &z, state12, 12, T + 2, atts.clone());
    composed.segments = vec![segment];
    let direct = input_b(&c, &z, genesis, 0, T + 2, atts);
    let composed = rule::run_checkpoint_b(&composed).unwrap().journal;
    assert_eq!(composed, rule::run_checkpoint_b(&direct).unwrap().journal);
    assert_eq!(composed.parent_state, segment.start_state);
}

// ---- upstream's pinned layouts (v0.4.0) and what the zone did not produce -------------------------------

fn actor(account: u8, program: u8) -> Vec<u8> {
    [[account; 32], [program; 32]].concat()
}

const PUBLIC_PREFIX: &[u8; 32] = b"/LEE/v0.3/Message/Public/\x00\x00\x00\x00\x00\x00\x00";
const PUBLIC_KEYED_PREFIX: &[u8; 32] = b"/LEE/v0.3/KeyedMessage/Public/\x00\x00";
const PRIVACY_PREFIX: &[u8; 32] = b"/LEE/v0.3/Message/Privacy/\x00\x00\x00\x00\x00\x00";
const PRIVACY_KEYED_PREFIX: &[u8; 32] = b"/LEE/v0.3/KeyedMessage/Privacy/\x00";

/// What a signer signs: `SHA256(keyed prefix ‖ SHA256(prefix ‖ message) ‖ next keys)`
/// (`lee/state_machine/src/transaction_message.rs:94-102, 155-167`), here with one next key `Schnorr, [7; 32]`.
fn keyed(prefix: &[u8; 32], keyed_prefix: &[u8; 32], message: &[u8]) -> Hash {
    let next = [&[0u8][..], &[7; 32]].concat();
    sha256_parts(&[keyed_prefix, &sha256_parts(&[prefix, message]), &next])
}

/// The knobs of a public transaction around upstream's pinned message
/// (`lee/state_machine/src/public_transaction/message.rs:141-160`: one actor `([42; 32], [0; 32])` with `Signer`
/// authority, its account authorized, a root call to it with message `[0]`, its nonce 1, no fee, no evidence).
struct Public {
    authority: u8,
    nonces: Vec<([u8; 32], u128)>,
    fee: Vec<u8>,
    evidence: Vec<u8>,
    witness: Vec<u8>,
}

impl Default for Public {
    fn default() -> Self {
        Self {
            authority: 1,
            nonces: vec![([42; 32], 1)],
            fee: vec![0],
            evidence: u32le(0).to_vec(),
            // One witness entry: 64 bytes of auth data, next key Schnorr [7; 32].
            witness: [&u32le(1)[..], &u32le(64), &[0x55; 64], &[0], &[7; 32]].concat(),
        }
    }
}

impl Public {
    fn message(&self) -> Vec<u8> {
        let mut m = Vec::new();
        m.extend_from_slice(&u32le(1)); // public_context.actors
        m.extend_from_slice(&actor(42, 0));
        m.push(self.authority);
        m.extend_from_slice(&u32le(1)); // authorized_accounts
        m.extend_from_slice(&[42; 32]);
        m.extend_from_slice(&u32le(0)); // promotions
        m.extend_from_slice(&u32le(0)); // private_signers
        m.extend_from_slice(&actor(42, 0)); // execution.root.to
        m.extend_from_slice(&u32le(1)); // execution.root.message
        m.push(0);
        m.extend_from_slice(&u32le(self.nonces.len()));
        for (account, nonce) in &self.nonces {
            m.extend_from_slice(account);
            m.extend_from_slice(&nonce.to_le_bytes());
        }
        m.extend_from_slice(&self.fee);
        m.extend_from_slice(&self.evidence);
        m
    }
    fn tx(&self) -> Vec<u8> {
        [&[0u8][..], &self.message(), &self.witness].concat()
    }
}

/// The knobs of upstream's pinned circuit output (`lee/state_machine/core/src/circuit_io.rs:748-832`: the public
/// context and `ProvenExecution` of a privacy-preserving message); by default exactly its bytes.
#[derive(Clone)]
struct Pp {
    actors: Vec<(Vec<u8>, u8)>,
    authorized: Vec<[u8; 32]>,
    promotions: Vec<u64>,
    private_signers: Vec<Vec<u8>>,
    /// The first delivery's grants and PDA seeds.
    grants: Vec<Vec<u8>>,
    pda_seeds: Vec<[u8; 32]>,
    /// The tag of the third boundary step (`EndPrivateSubtree` = 2).
    end_private_tag: u8,
    /// The tag of the first publication (`Clear` = 0).
    clear_tag: u8,
    /// The tag of the second program image claim (`Undisclosed` = 1).
    claim_tag: u8,
    proven_fee: Vec<u8>,
}

impl Default for Pp {
    fn default() -> Self {
        Self {
            actors: vec![(actor(0, 1), 0)],
            authorized: vec![[4; 32]],
            promotions: vec![5],
            private_signers: vec![actor(11, 12)],
            grants: vec![actor(7, 10)],
            pda_seeds: vec![[8; 32]],
            end_private_tag: 2,
            clear_tag: 0,
            claim_tag: 1,
            proven_fee: vec![0],
        }
    }
}

impl Pp {
    fn circuit_output(&self) -> Vec<u8> {
        let mut o = Vec::new();
        o.extend_from_slice(&u32le(self.actors.len()));
        for (a, authority) in &self.actors {
            o.extend_from_slice(a);
            o.push(*authority);
        }
        o.extend_from_slice(&u32le(self.authorized.len()));
        for a in &self.authorized {
            o.extend_from_slice(a);
        }
        o.extend_from_slice(&u32le(self.promotions.len()));
        for x in &self.promotions {
            o.extend_from_slice(&x.to_le_bytes());
        }
        o.extend_from_slice(&u32le(self.private_signers.len()));
        for a in &self.private_signers {
            o.extend_from_slice(a);
        }
        // boundary: four steps
        o.extend_from_slice(&u32le(4));
        o.push(0); // PrivateToPublic: private (2, 3) to public (0, 1), message [6]
        o.extend_from_slice(&actor(2, 3));
        o.extend_from_slice(&actor(0, 1));
        o.extend_from_slice(&u32le(1));
        o.push(6);
        o.extend_from_slice(&u32le(self.grants.len()));
        for g in &self.grants {
            o.extend_from_slice(g);
        }
        o.extend_from_slice(&u32le(self.pda_seeds.len()));
        for s in &self.pda_seeds {
            o.extend_from_slice(s);
        }
        o.push(1); // PublicToPrivate: public (0, 1) to private (2, 3), message [9], no grants, no seeds
        o.extend_from_slice(&actor(0, 1));
        o.extend_from_slice(&actor(2, 3));
        o.extend_from_slice(&u32le(1));
        o.push(9);
        o.extend_from_slice(&u32le(0));
        o.extend_from_slice(&u32le(0));
        o.push(self.end_private_tag);
        o.push(3); // EndPublicSubtree
                   // async_messages: a clear publication from (2, 3) to (27, 28) carrying [29], and a sealed one
        o.extend_from_slice(&u32le(2));
        o.push(self.clear_tag);
        o.extend_from_slice(&actor(2, 3));
        o.extend_from_slice(&actor(27, 28));
        o.extend_from_slice(&u32le(1));
        o.push(29);
        o.push(1);
        o.extend_from_slice(&[10; 32]);
        o.extend_from_slice(&[&u32le(3)[..], &[11; 3], &u32le(2), &[12; 2]].concat());
        // recovery_bindings: one
        o.extend_from_slice(&u32le(1));
        o.extend_from_slice(&[13; 32]);
        o.extend_from_slice(&[&u32le(3)[..], &[14; 3], &u32le(2), &[15; 2]].concat());
        // public_root: Some, to (0, 1) carrying [16, 16]
        o.push(1);
        o.extend_from_slice(&actor(0, 1));
        o.extend_from_slice(&u32le(2));
        o.extend_from_slice(&[16; 2]);
        // private_actions: one
        o.extend_from_slice(&u32le(1));
        o.extend_from_slice(&[[17; 32], [18; 32], [19; 32]].concat());
        o.extend_from_slice(&[&u32le(3)[..], &[20; 3], &u32le(2), &[21; 2]].concat());
        // validity: blocks from 22, timestamps to 23
        o.push(1);
        o.extend_from_slice(&22u64.to_le_bytes());
        o.extend_from_slice(&[0, 0, 1]);
        o.extend_from_slice(&23u64.to_le_bytes());
        // program_image_claims: Disclosed (24, image id 25 x 8 words), Undisclosed (26)
        o.extend_from_slice(&u32le(2));
        o.push(0);
        o.extend_from_slice(&[24; 32]);
        o.extend_from_slice(&[25, 0, 0, 0].repeat(8));
        o.push(self.claim_tag);
        o.extend_from_slice(&[26; 32]);
        o.extend_from_slice(&self.proven_fee);
        o
    }

    /// `borsh(LeeTransaction::PrivacyPreserving(..))`: the circuit output, then the message's tail as upstream's pinned
    /// privacy message has it (`lee/state_machine/src/privacy_preserving_transaction/message.rs:161-168`), no witness
    /// entry and a 3-byte proof.
    fn tx(&self) -> Vec<u8> {
        [&[1u8][..], &self.circuit_output(), &pp_message_tail(), &u32le(0), &u32le(3), &[0xab; 3]].concat()
    }
}

/// `nonces` (`[2; 32]` at 1), `fee: None`, `admission_evidence` (one `Pda { program: [0; 32], seed: [1; 32] }`).
fn pp_message_tail() -> Vec<u8> {
    [&u32le(1)[..], &[2; 32], &1u128.to_le_bytes(), &[0], &u32le(1), &[1], &[0; 32], &[1; 32]].concat()
}

/// A stand-in for the fee or clock transaction (the parser does not read what they call).
fn system_tx() -> Vec<u8> {
    Public { witness: u32le(0).to_vec(), ..Public::default() }.tx()
}

/// `borsh(Block)` of `user` transactions with the given receipts' `success`, then the fee and clock stand-ins.
fn block040(user: &[Vec<u8>], success: &[bool]) -> Vec<u8> {
    let (id, prev, timestamp) = (9u64, [8u8; 32], 1_000u64);
    let mut body = u32le(user.len() + 2).to_vec();
    for tx in user {
        body.extend_from_slice(tx);
    }
    body.extend_from_slice(&system_tx());
    body.extend_from_slice(&system_tx());
    body.extend_from_slice(&u32le(success.len()));
    for (k, ok) in success.iter().enumerate() {
        body.push(u8::from(*ok));
        body.extend_from_slice(&(k as u64).to_le_bytes()); // gas_exec
        body.extend_from_slice(&2u64.to_le_bytes()); // gas_stor
        for x in [3u128, 4, 5] {
            body.extend_from_slice(&x.to_le_bytes()); // fee_base, tip, refund
        }
    }
    let hash = sha256_parts(&[BLOCK_HASH_PREFIX, &id.to_le_bytes(), &prev, &timestamp.to_le_bytes(), &body]);
    [&id.to_le_bytes()[..], &prev, &hash, &timestamp.to_le_bytes(), &body, &[1]].concat()
}

#[test]
fn the_pinned_layouts_are_upstreams() {
    // Upstream's known answers for the message each layout test pins (`message_with_keys_has_known_answers`).
    let public = Public::default().message();
    assert_eq!(public.len(), 239);
    assert_eq!(
        keyed(PUBLIC_PREFIX, PUBLIC_KEYED_PREFIX, &public),
        checkpoint_core::hash::hex32("ae983ec82822a8363e7fc615b34760e312acba3b319b34b6d58e96b4192b6ed7")
    );
    let empty_pp =
        [&[0u8; 16][..], &u32le(0), &u32le(0), &u32le(0), &[0], &u32le(0), &[0; 4], &u32le(0), &[0]].concat();
    let pp_message = [empty_pp.as_slice(), &pp_message_tail()].concat();
    assert_eq!(
        keyed(PRIVACY_PREFIX, PRIVACY_KEYED_PREFIX, &pp_message),
        checkpoint_core::hash::hex32("2c2b3dc299c78e6295c07d79be857b89b7c136d0b951bbf3cb60b0576bac039e")
    );
    // The circuit output's frame length (`[87, 4, 0, 0]`: 1111 bytes).
    assert_eq!(Pp::default().circuit_output().len(), 1111);
    // Both pinned messages parse as transactions of a block.
    let empty_pp_tx = [&[1u8][..], &pp_message, &u32le(0), &u32le(0)].concat();
    let b = block040(&[Public::default().tx(), empty_pp_tx], &[true, true]);
    let mut rec = Record::default();
    let s = parse_block(&b, &mut rec).unwrap();
    assert_eq!((s.tx_count, s.receipts.len()), (4, 2));
    assert_eq!(
        rec.txs.iter().map(|t| t.1).collect::<Vec<_>>(),
        [TxKind::Public, TxKind::PrivacyPreserving, TxKind::Public, TxKind::Public]
    );
    assert_eq!(hash_parsed_block(&b).unwrap(), s.header.hash);
}

#[test]
fn a_privacy_preserving_transaction_reports_its_visible_calls() {
    let pp = Pp::default().tx();
    let public = Public::default().tx();
    let b = block040(&[pp.clone(), public.clone()], &[true, false]);
    let mut rec = Record::default();
    let s = parse_block(&b, &mut rec).unwrap();
    assert_eq!(s.header.block_id, 9);
    assert_eq!(s.status, 1);
    // Spans are the transactions' exact bytes.
    assert_eq!(rec.txs[0], (0, TxKind::PrivacyPreserving, pp.as_slice()));
    assert_eq!(rec.txs[1], (1, TxKind::Public, public.as_slice()));
    // Receipts: the public transaction failed; the fee and clock transactions have none.
    assert_eq!(s.receipts[1].fee.gas_exec, 1);
    assert_eq!((s.receipts[0].fee.fee_base, s.receipts[0].fee.tip, s.receipts[0].fee.refund), (3, 4, 5));
    assert_eq!((0..4).map(|t| s.is_reverted(t)).collect::<Vec<_>>(), [false, true, false, false]);
    // In encoding order: the boundary's deliveries, the clear publication, the public root; then the public root call.
    // Each actor by the first bytes of its two account ids.
    type Seen = (u32, Origin, Option<(u8, u8)>, (u8, u8), Vec<u8>);
    let a = |x: u8, y: u8| (x, y);
    let calls: Vec<Seen> = rec
        .calls
        .iter()
        .map(|(i, o, from, to, m)| {
            (
                *i,
                *o,
                from.map(|f| a(f.account_id[0], f.program_account_id[0])),
                a(to.account_id[0], to.program_account_id[0]),
                m.to_vec(),
            )
        })
        .collect();
    assert_eq!(
        calls[..5],
        [
            (0, Origin::PrivateToPublic, Some((2, 3)), (0, 1), vec![6]),
            (0, Origin::PublicToPrivate, Some((0, 1)), (2, 3), vec![9]),
            (0, Origin::ClearPublication, Some((2, 3)), (27, 28), vec![29]),
            (0, Origin::PrivacyRoot, None, (0, 1), vec![16, 16]),
            (1, Origin::PublicRoot, None, (42, 0), vec![0]),
        ]
    );
    assert_eq!(calls.len(), 7);
    // Executed by a public actor in normal settlement: the roots and the private-to-public delivery only.
    let public_execution: Vec<Origin> = rec.calls.iter().map(|c| c.1).filter(|o| o.is_public_execution()).collect();
    assert_eq!(
        public_execution,
        [Origin::PrivateToPublic, Origin::PrivacyRoot, Origin::PublicRoot, Origin::PublicRoot, Origin::PublicRoot]
    );
    // The visible calls the default build's Origin also has are classified as it classifies them.
    use checkpoint_core::lez::Origin as Rc1;
    for (new, old) in [
        (Origin::PublicRoot, Rc1::PublicRoot),
        (Origin::PrivacyRoot, Rc1::PrivacyRoot),
        (Origin::PrivateToPublic, Rc1::PrivateToPublic),
        (Origin::PublicToPrivate, Rc1::PublicToPrivate),
        (Origin::FallbackPrivateToPublic, Rc1::FallbackPrivateToPublic),
        (Origin::FallbackPublicToPrivate, Rc1::FallbackPublicToPrivate),
    ] {
        assert_eq!(new.is_public_execution(), old.is_public_execution());
    }
    assert!(!Origin::ClearPublication.is_public_execution());
    assert_eq!(hash_parsed_block(&b).unwrap(), s.header.hash);
}

/// `Some(ProvenFee { refund_to, tip: 7, fallback })`, the fallback's boundary one private-to-public delivery of [39]
/// from (35, 36) to (37, 38).
fn proven_fee(refund_to: bool, fallback: bool) -> Vec<u8> {
    let mut f = vec![1u8];
    if refund_to {
        f.push(1);
        f.extend_from_slice(&[30; 32]);
    } else {
        f.push(0);
    }
    f.extend_from_slice(&7u64.to_le_bytes());
    if fallback {
        f.push(1);
        f.extend_from_slice(&[[31; 32], [32; 32], [33; 32]].concat());
        f.extend_from_slice(&[&u32le(3)[..], &[34; 3]].concat());
        f.extend_from_slice(&u32le(2));
        f.push(0);
        f.extend_from_slice(&actor(35, 36));
        f.extend_from_slice(&actor(37, 38));
        f.extend_from_slice(&[&u32le(1)[..], &[39], &u32le(0), &u32le(0)].concat());
        f.push(3);
    } else {
        f.push(0);
    }
    f
}

#[test]
fn a_proven_fee_and_its_fallback_parse_and_the_fallback_delivery_is_reported() {
    for (refund_to, fallback) in [(false, false), (true, false), (false, true), (true, true)] {
        let pp = Pp { proven_fee: proven_fee(refund_to, fallback), ..Pp::default() };
        let b = block040(&[pp.tx()], &[true]);
        let mut rec = Record::default();
        parse_block(&b, &mut rec).unwrap();
        let fallbacks: Vec<_> = rec.calls.iter().filter(|c| c.1 == Origin::FallbackPrivateToPublic).collect();
        assert_eq!(fallbacks.len(), usize::from(fallback));
        if fallback {
            let (index, _, from, to, message) = fallbacks[0];
            assert_eq!(
                (*index, from.unwrap().account_id[0], to.program_account_id[0], *message),
                (0, 35, 38, &[39u8][..])
            );
            // It is the transaction's last visible call (encoding order).
            assert_eq!(rec.calls.iter().rfind(|c| c.0 == 0).unwrap().1, Origin::FallbackPrivateToPublic);
        }
    }
}

#[test]
fn fees_evidence_and_witness_entries_of_a_public_transaction() {
    let fee = [&[1u8][..], &[42; 32], &5u64.to_le_bytes(), &1u64.to_le_bytes(), &9u128.to_le_bytes()].concat();
    // Evidence: a key (Schnorr, [9; 32]) and a PDA.
    let evidence = [&u32le(2)[..], &[0, 0], &[9; 32], &[1], &[0; 32], &[1; 32]].concat();
    let two_entries = [&u32le(2)[..], &u32le(0), &[0], &[7; 32], &u32le(3), &[1, 2, 3], &[0], &[8; 32]].concat();
    let ok = Public { fee: fee.clone(), evidence: evidence.clone(), witness: two_entries, ..Public::default() };
    let b = block040(&[ok.tx()], &[true]);
    let mut rec = Record::default();
    parse_block(&b, &mut rec).unwrap();
    assert_eq!(rec.txs[0].2, ok.tx().as_slice());

    let refused = |p: Public| parse(&block040(&[p.tx()], &[true])).unwrap_err();
    assert_eq!(
        refused(Public { fee: vec![2], ..Public::default() }),
        Error::BadTag { what: "fee declaration", tag: 2 }
    );
    assert_eq!(refused(Public { authority: 2, ..Public::default() }), Error::BadTag { what: "Authority", tag: 2 });
    let bad_key = [&u32le(1)[..], &[0, 1], &[9; 32]].concat();
    assert_eq!(refused(Public { evidence: bad_key, ..Public::default() }), Error::BadTag { what: "KeyType", tag: 1 });
    let bad_evidence = [&u32le(1)[..], &[2], &[9; 64]].concat();
    assert_eq!(
        refused(Public { evidence: bad_evidence, ..Public::default() }),
        Error::BadTag { what: "PublicAccountEvidence", tag: 2 }
    );
    let bad_next = [&u32le(1)[..], &u32le(0), &[1], &[7; 32]].concat();
    assert_eq!(refused(Public { witness: bad_next, ..Public::default() }), Error::BadTag { what: "KeyType", tag: 1 });
    for nonces in [vec![([42; 32], 1), ([41; 32], 2)], vec![([42; 32], 1), ([42; 32], 2)]] {
        assert_eq!(refused(Public { nonces, ..Public::default() }), Error::NotStrictlyOrdered { what: "nonces" });
    }
    // A witness entry count the input cannot hold.
    let huge = [&u32le(1_000_000)[..], &u32le(0), &[0], &[7; 32]].concat();
    assert_eq!(refused(Public { witness: huge, ..Public::default() }), Error::BadLength { what: "witness entries" });
}

#[test]
fn malformed_privacy_preserving_transactions_are_refused() {
    let refused = |pp: Pp| parse(&block040(&[pp.tx()], &[true])).unwrap_err();
    let d = Pp::default;
    assert_eq!(refused(Pp { actors: vec![(actor(0, 1), 2)], ..d() }), Error::BadTag { what: "Authority", tag: 2 });
    for actors in [vec![(actor(0, 1), 0), (actor(0, 0), 1)], vec![(actor(0, 1), 0), (actor(0, 1), 1)]] {
        assert_eq!(refused(Pp { actors, ..d() }), Error::NotStrictlyOrdered { what: "actors" });
    }
    assert_eq!(
        refused(Pp { authorized: vec![[4; 32], [4; 32]], ..d() }),
        Error::NotStrictlyOrdered { what: "authorized_accounts" }
    );
    // Promotions are ordered as numbers: 1 < 256, although 256's little-endian bytes sort first.
    assert!(parse(&block040(&[Pp { promotions: vec![1, 256], ..d() }.tx()], &[true])).is_ok());
    assert_eq!(refused(Pp { promotions: vec![256, 1], ..d() }), Error::NotStrictlyOrdered { what: "promotions" });
    assert_eq!(
        refused(Pp { private_signers: vec![actor(12, 0), actor(11, 12)], ..d() }),
        Error::NotStrictlyOrdered { what: "private_signers" }
    );
    assert_eq!(
        refused(Pp { grants: vec![actor(7, 10), actor(7, 9)], ..d() }),
        Error::NotStrictlyOrdered { what: "grants" }
    );
    assert_eq!(
        refused(Pp { pda_seeds: vec![[8; 32], [8; 32]], ..d() }),
        Error::NotStrictlyOrdered { what: "pda_seeds" }
    );
    assert_eq!(refused(Pp { end_private_tag: 4, ..d() }), Error::BadTag { what: "BoundaryStep", tag: 4 });
    assert_eq!(refused(Pp { clear_tag: 2, ..d() }), Error::BadTag { what: "Publication", tag: 2 });
    assert_eq!(refused(Pp { claim_tag: 2, ..d() }), Error::BadTag { what: "ProgramImageClaim", tag: 2 });
    assert_eq!(refused(Pp { proven_fee: vec![2], ..d() }), Error::BadTag { what: "proven_fee", tag: 2 });
    let mut fee = proven_fee(true, true);
    fee[1] = 2;
    assert_eq!(refused(Pp { proven_fee: fee, ..d() }), Error::BadTag { what: "refund_to", tag: 2 });
    let mut fee = proven_fee(false, true);
    fee[10] = 2;
    assert_eq!(refused(Pp { proven_fee: fee, ..d() }), Error::BadTag { what: "fallback", tag: 2 });

    // The receipts must number the user transactions.
    let pp = d().tx();
    assert_eq!(parse(&block040(std::slice::from_ref(&pp), &[])), Err(Error::BadRevertedList { block: 9 }));
    assert_eq!(parse(&block040(std::slice::from_ref(&pp), &[true, true])), Err(Error::BadRevertedList { block: 9 }));
    assert_eq!(parse(&block040(&[], &[])).map(|s| s.tx_count), Ok(2));

    // Every proper prefix is refused, and so is any trailing byte.
    let b = block040(&[pp, Pp { proven_fee: proven_fee(true, true), ..d() }.tx()], &[true, false]);
    assert!(parse(&b).is_ok());
    for cut in 0..b.len() {
        assert!(parse(&b[..cut]).is_err(), "prefix of {cut} bytes");
    }
    let mut t = b.clone();
    t.push(0);
    assert_eq!(parse(&t), Err(Error::TrailingBytes { at: b.len() }));
}
