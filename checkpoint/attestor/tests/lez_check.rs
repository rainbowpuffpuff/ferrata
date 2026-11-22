//! Review H-2 (CK-1), M-4 (CK-3) and M7: what the attestor's LEZ node decides, what it skips, and what
//! its state guarantees.
//!
//! - H-2: a turn holder inscribes a well-formed block F that LEZ parks (it does not apply). Before the
//!   fix the attestor signed F from the Bedrock node's view alone, and `t` signatures of F make the
//!   checkpoint program accept a burn root no LEZ node holds. Now it signs only what its LEZ node holds
//!   final, so F is never signed and the honest block at F's height is, once final.
//! - M-4: garbage, short payloads, repeated ids, re-inscriptions and unreadable ops are passed over.
//! - M7: the read starts at the signed tip; the guard against two histories is total; and over random
//!   sequences of rounds with restarts (the state read from its file each round), forged blocks, garbage,
//!   withdrawals, compromised nodes and outages, no two signed tips conflict.
mod support;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use attestor_core::{sign_certificate, Deployment, FinalTip, SignedAttestation};
use checkpoint_core::input::CheckpointInputB;
use checkpoint_core::mantle::{inscription_msg_id, ElidedInscription};
use checkpoint_core::rule_b::run_checkpoint_b;
use checkpoint_core::{Error, Hash};
use ferrata_attestor::{
    assemble_unchecked, base64_decode, base64_encode, bedrock_tier, collect, deployment_of, final_inscriptions,
    final_tier, hexa, read_state, rekey_base, scan_from, state_from_json, state_json, upgrade_pre_producer_input,
    write_state, Attestor, AttestorState, FinalTier, NodeAnswers,
};
use serde_json::{json, Value};
use support::*;

type Lez = BTreeMap<u64, Option<Vec<u8>>>;

fn base_n3() -> Vec<u8> {
    let old = fs::read(dir().join("../realblocks/out/real-lab-186-202.input")).unwrap();
    let ins = final_inscriptions(&lab_answers(), &channel()).unwrap();
    let real = upgrade_pre_producer_input(&old, &ins[&202].elided().unwrap()).unwrap();
    rekey_base(&real, &n3_keys(), 3, 2).unwrap()
}

fn attestor(i: u32, d: &Deployment) -> Attestor {
    let ch = key(&format!("channel-{i}"));
    let att = key(&format!("attestation-{i}"));
    Attestor { deployment: *d, channel_key: pk(&ch), certificate: sign_certificate(&ch, d, &pk(&att)), att_key: att }
}

fn dep_of(base: &[u8]) -> Deployment {
    deployment_of(&CheckpointInputB::decode(base).unwrap().config)
}

/// A LEZ node holding the lab chain's real blocks 186..=`last`, each served with `status`.
fn lez_lab(last: u64, status: u8) -> Lez {
    let a = lab_answers();
    (186..=last).map(|id| (id, Some(with_status(&payload_of(&a, id), status)))).collect()
}

fn read(a: &NodeAnswers, lez: &Lez, state: Option<&AttestorState>) -> anyhow::Result<FinalTier> {
    final_tier(a, &mut lez.clone(), &channel(), state)
}

fn copy(a: &NodeAnswers) -> NodeAnswers {
    NodeAnswers { channel: a.channel.clone(), lib_slot: a.lib_slot, blocks: a.blocks.clone() }
}

fn slot(b: &Value) -> u64 {
    b["header"]["slot"].as_u64().unwrap()
}

/// The lab answers with key set n3 and a forged block F = 201' on the real 200, inscribed by k1 in its
/// turn just before the honest 201. Returns the answers, F, F's transaction bytes and JSON.
fn with_forged_f() -> (NodeAnswers, Vec<u8>, Vec<u8>, Value) {
    let mut a = answers_with_keys(&n3_keys());
    let s201 = slot_of(&a, 201);
    let (p200, p201) = (payload_of(&a, 200), payload_of(&a, 201));
    let f = derive_block(&p201, 201, &carried_hash(&p200), 1);
    assert_ne!(carried_hash(&f), carried_hash(&p201));
    let k1 = key("channel-1");
    let (ftx, bytes, _) = tx(&k1, &[inscribe_op(&k1, &f, &msg_of(&a, 200)), fee_transfer(7)], 0);
    let at = a.blocks.iter().position(|b| slot(b) >= s201).unwrap();
    a.blocks.insert(at, node_block("F", s201 - 1, vec![ftx.clone()]));
    (a, f, bytes, ftx)
}

/// The state of an attestor that signed the real 200 (read when Bedrock was final up to 200).
fn signed_200() -> AttestorState {
    let mut earlier = answers_with_keys(&n3_keys());
    earlier.lib_slot = slot_of(&earlier, 200);
    let t = read(&earlier, &lez_lab(200, FINALIZED), None).unwrap();
    assert_eq!(t.tip.tip_id, 200);
    AttestorState::of(&t)
}

#[test]
fn h2_a_block_that_lez_parked_is_never_signed_and_the_forgery_needs_it() {
    let (a, f, f_bytes, ftx) = with_forged_f();
    let lab = lab_answers();
    let (p200, p201) = (payload_of(&lab, 200), payload_of(&lab, 201));
    let st200 = signed_200();
    let base = base_n3();
    let d = dep_of(&base);
    let (a1, a2) = (attestor(1, &d), attestor(2, &d));

    // Phase A: F is final on Bedrock, the honest 201 not yet. The LEZ node parked F: it holds
    // 186..=200 final and the honest 201 at its head (Pending).
    let mut phase_a = copy(&a);
    phase_a.lib_slot = slot_of(&lab, 201) - 1;
    let mut lez_a = lez_lab(200, FINALIZED);
    lez_a.insert(201, Some(with_status(&p201, PENDING)));
    // The Bedrock node's view alone (the attestor before the fix) ends at F.
    let bedrock = bedrock_tier(&phase_a, &channel()).unwrap();
    assert_eq!((bedrock.tip.tip_id, bedrock.tip.tip_hash), (201, carried_hash(&f)));
    for state in [Some(&st200), None] {
        let t = read(&phase_a, &lez_a, state).unwrap();
        assert_eq!((t.tip.tip_id, t.tip.tip_hash), (200, carried_hash(&p200)), "state {:?}", state.is_some());
        assert!(
            t.notes.iter().any(|n| n.contains("201..=201: inscribed, but not final in the LEZ node")),
            "{:?}",
            t.notes
        );
    }
    let t = read(&phase_a, &lez_a, Some(&st200)).unwrap();
    let honest = [a1.attest(&t, Some(st200.last)).unwrap(), a2.attest(&t, Some(st200.last)).unwrap()];
    assert!(honest.iter().all(|x| x.tip.tip_hash == carried_hash(&p200)));

    // The forgery: T = F over 186..=200, F. With t signatures of F the program accepts it (it executes
    // nothing under rule (B)); the honest attestors' signatures of 200 cannot make it.
    let mut input = CheckpointInputB::decode(&base).unwrap();
    input.blocks.truncate(15);
    input.blocks.push(&f);
    input.t_index = 15;
    let sig: [u8; 64] = hexa(&ftx["ops_proofs"][0]["Ed25519Sig"], "sig").unwrap();
    input.producer = ElidedInscription::split(&f_bytes, 0, &sig).unwrap();
    let forged_base = input.encode();
    let f_tip = FinalTip {
        tip_id: 201,
        tip_hash: carried_hash(&f),
        final_msg_id: inscription_msg_id(&channel(), &f, &msg_of(&lab, 200), &pk(&key("channel-1"))),
    };
    let forced: Vec<SignedAttestation> = [&a1, &a2]
        .iter()
        .map(|x| SignedAttestation::new(&x.att_key, x.channel_key, x.certificate, &d, &t.key_set_hash, f_tip))
        .collect();
    let accepted =
        run_checkpoint_b(&CheckpointInputB::decode(&assemble_unchecked(&forged_base, &forced).unwrap()).unwrap())
            .unwrap();
    assert_eq!((accepted.journal.tip_id, accepted.journal.tip_hash), (201, carried_hash(&f)));
    let refused =
        run_checkpoint_b(&CheckpointInputB::decode(&assemble_unchecked(&forged_base, &honest).unwrap()).unwrap());
    assert_eq!(refused.unwrap_err(), Error::NotADescendant { tip_id: 200 });

    // Phase B: the honest 201 (inscribed after F) and 202 are final; the LEZ node holds 186..=202 final.
    // At 201 two blocks are inscribed on 200: the LEZ node decides. (In the lab data the honest 201's
    // inscription names 200's message as its parent, where after F Bedrock would have it name F's; the
    // attestor does not follow inscription parents, which a parked block breaks.)
    let lez_b = lez_lab(202, FINALIZED);
    for state in [Some(&st200), None] {
        let t = read(&a, &lez_b, state).unwrap();
        assert_eq!(t.tip.tip_id, 202);
        if state.is_some() {
            assert_eq!(t.hashes[&201], carried_hash(&p201));
            assert!(t.notes.iter().any(|n| n.starts_with("block 201: 2 blocks inscribed")), "{:?}", t.notes);
        }
    }
    let t = read(&a, &lez_b, Some(&st200)).unwrap();
    let both = [a1.attest(&t, Some(st200.last)).unwrap(), a2.attest(&t, Some(st200.last)).unwrap()];
    let outcome = run_checkpoint_b(&CheckpointInputB::decode(&collect(&base, &both).unwrap().input).unwrap()).unwrap();
    assert_eq!((outcome.journal.tip_id, outcome.attestors), (202, 2));
}

#[test]
fn h2_the_lez_node_must_serve_the_inscribed_bytes_with_status_finalized() {
    let (a, f, _, _) = with_forged_f();
    let lab = lab_answers();
    let mut phase_a = copy(&a);
    phase_a.lib_slot = slot_of(&lab, 201) - 1;
    let st200 = signed_200();
    let mut other = with_status(&f, FINALIZED);
    other[100] ^= 1; // another block at 201 (one transaction byte)
    let cases: [(&str, Option<Vec<u8>>, u64); 5] = [
        ("nothing at 201", None, 200),
        ("F, Pending", Some(with_status(&f, PENDING)), 200),
        ("F, Safe", Some(with_status(&f, 1)), 200),
        ("another block, Finalized", Some(other), 200),
        // A LEZ node that applied F: F is valid by LEZ's rules, and is signed.
        ("F, Finalized", Some(with_status(&f, FINALIZED)), 201),
    ];
    for (what, at201, want) in cases {
        let mut lez = lez_lab(200, FINALIZED);
        lez.insert(201, at201);
        let t = read(&phase_a, &lez, Some(&st200)).unwrap();
        assert_eq!(t.tip.tip_id, want, "{what}");
    }
}

#[test]
fn h2_a_lez_node_that_does_not_hold_the_signed_tip_refuses() {
    let a = answers_with_keys(&n3_keys());
    let lab = lab_answers();
    let st200 = signed_200();
    let p200 = payload_of(&lab, 200);
    let other200 = derive_block(&p200, 200, &p200[8..40].try_into().unwrap(), 5);
    let cases: [(&str, Option<Vec<u8>>, &str); 3] = [
        ("another block 200", Some(with_status(&other200, FINALIZED)), "another history"),
        ("200, Pending", Some(with_status(&p200, PENDING)), "status 0"),
        ("nothing at 200", None, "holds no block 200"),
    ];
    for (what, at200, want) in cases {
        let mut lez = lez_lab(202, FINALIZED);
        lez.insert(200, at200);
        let e = read(&a, &lez, Some(&st200)).unwrap_err().to_string();
        assert!(e.contains(want), "{what}: {e}");
    }
    // A first read with a LEZ node that holds nothing.
    let e = read(&a, &Lez::new(), None).unwrap_err().to_string();
    assert!(e.contains("holds none of the blocks inscribed"), "{e}");
}

#[test]
fn m4_garbage_repeats_and_unreadable_ops_never_halt() {
    let mut a = answers_with_keys(&n3_keys());
    let lab = lab_answers();
    let (k2, outsider) = (key("channel-2"), key("channel-9"));
    let (p150, p200, p202) = (payload_of(&lab, 186), payload_of(&lab, 200), payload_of(&lab, 202));
    let parent = msg_of(&lab, 202);
    let mut dup_id = 200u64.to_le_bytes().to_vec();
    dup_id.extend(b"not a block");
    let low = derive_block(&p150, 150, &[3; 32], 9);
    let zero = derive_block(&p150, 0, &[4; 32], 9);
    let max = derive_block(&p150, u64::MAX, &[5; 32], 9);
    let sibling = derive_block(&p202, 202, &p202[8..40].try_into().unwrap(), 4);
    let fake203 = derive_block(&p202, 203, &carried_hash(&p202), 2);
    let fake204 = derive_block(&p202, 204, &carried_hash(&fake203), 3);
    let mut txs: Vec<Value> = vec![
        inscription_tx(&k2, b"hello", &parent, 1).0,
        inscription_tx(&k2, b"not a block", &parent, 2).0,
        inscription_tx(&k2, &dup_id, &parent, 3).0,
        inscription_tx(&k2, &p200, &parent, 4).0, // the real 200, inscribed again
        inscription_tx(&k2, &low, &parent, 5).0,
        inscription_tx(&k2, &zero, &parent, 10).0,
        inscription_tx(&k2, &max, &parent, 11).0,
        inscription_tx(&outsider, &sibling, &parent, 6).0,
        inscription_tx(&k2, &fake203, &parent, 7).0,
        inscription_tx(&k2, &fake204, &parent, 8).0,
        json!({"mantle_tx": {"hash": "00".repeat(32)}, "ops_proofs": []}),
    ];
    let mut bad_hex = inscription_tx(&k2, &fake203, &parent, 9).0;
    bad_hex["mantle_tx"]["ops"][0]["payload"]["inscription"] = json!("zz");
    txs.push(bad_hex);
    let lib = a.lib_slot;
    a.blocks.push(node_block("garbage", lib, txs));
    let lez = lez_lab(202, FINALIZED);
    let st200 = signed_200();
    for state in [Some(&st200), None] {
        let t = read(&a, &lez, state).unwrap();
        assert_eq!((t.tip.tip_id, t.tip.tip_hash), (202, carried_hash(&p202)));
        assert!(t.notes.len() >= 4, "{:?}", t.notes);
    }
    // The strict Bedrock-only view, as the attestor read before the fix, refuses the same answers.
    assert!(bedrock_tier(&a, &channel()).is_err());
}

#[test]
fn m7_the_read_starts_at_the_signed_tip_and_the_state_round_trips() {
    let a = answers_with_keys(&n3_keys());
    let lab = lab_answers();
    let st200 = signed_200();
    let from = scan_from(Some(&st200), 0);
    assert_eq!(from, slot_of(&lab, 200));
    let mut window = copy(&a);
    window.blocks.retain(|b| slot(b) >= from);
    // 30 of the 190 node blocks of the lab extraction lie at or after 200's inscription.
    assert!(window.blocks.len() < a.blocks.len() / 5, "{} of {}", window.blocks.len(), a.blocks.len());
    let t = read(&window, &lez_lab(202, FINALIZED), Some(&st200)).unwrap();
    assert_eq!((t.tip.tip_id, t.first_block, t.hashes.len()), (202, 200, 3));
    // A window that starts after the signed tip's inscription: the run still starts at the signed tip.
    let mut above = copy(&a);
    above.blocks.retain(|b| slot(b) > from);
    let t2 = read(&above, &lez_lab(202, FINALIZED), Some(&st200)).unwrap();
    assert_eq!((t2.tip, t2.first_block), (t.tip, 200));
    // A first-version state (tip id and hash only) reads from --slot-from.
    let v1 = AttestorState { last: st200.last, record: None };
    assert_eq!(scan_from(Some(&v1), 7), 7);
    assert_eq!(read(&a, &lez_lab(202, FINALIZED), Some(&v1)).unwrap().tip.tip_id, 202);
    let v1_json = json!({"tip_id": 200, "tip_hash": hex::encode(st200.last.tip_hash)});
    assert_eq!(state_from_json(&channel(), &v1_json).unwrap(), v1);
    // The state file round-trips, and a state of another channel is refused.
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("m7-state-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let p = dir.join("state.json");
    write_state(&p, &channel(), &st200).unwrap();
    assert_eq!(read_state(&p, &channel()).unwrap(), Some(st200.clone()));
    let left: Vec<String> =
        fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(left, ["state.json"], "no temporary file is left beside the state");
    assert!(format!("{:#}", read_state(&p, &[7; 32]).unwrap_err()).contains("of channel"));
    assert!(state_from_json(&[7; 32], &state_json(&channel(), &st200)).unwrap_err().to_string().contains("of channel"));
    assert_eq!(read_state(&dir.join("absent.json"), &channel()).unwrap(), None);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn base64_matches_rfc4648() {
    for (plain, enc) in [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ] {
        assert_eq!(base64_encode(plain.as_bytes()), enc);
        assert_eq!(base64_decode(enc).unwrap(), plain.as_bytes());
        assert_eq!(base64_decode(enc.trim_end_matches('=')).unwrap(), plain.as_bytes());
    }
    for bad in ["Z", "Zm9v*", "Zm=9", "Zm 9"] {
        assert!(base64_decode(bad).is_err(), "{bad}");
    }
}

/// Review L2 (`first-root-unchecked`): on a first read, a well-formed sibling J of 202 on 201, inscribed by a
/// key in K just before the real 202, sits at the highest id the LEZ node holds final. The root counts as
/// confirmed, so it must be the block the LEZ node holds byte for byte, never J (a mutant that took the first
/// candidate at that id signed J, a block no LEZ node holds).
#[test]
fn l2_a_first_read_never_roots_at_a_sibling_the_lez_node_does_not_hold() {
    let mut a = answers_with_keys(&n3_keys());
    let lab = lab_answers();
    let (p201, p202) = (payload_of(&lab, 201), payload_of(&lab, 202));
    let j = derive_block(&p202, 202, &carried_hash(&p201), 3);
    assert_ne!(carried_hash(&j), carried_hash(&p202));
    let (jtx, _) = inscription_tx(&key("channel-2"), &j, &msg_of(&lab, 201), 9);
    let s202 = slot_of(&lab, 202);
    let at = a.blocks.iter().position(|b| slot(b) >= s202).unwrap();
    a.blocks.insert(at, node_block("J", s202 - 1, vec![jtx]));
    for state in [None, Some(signed_200())] {
        let t = read(&a, &lez_lab(202, FINALIZED), state.as_ref()).unwrap();
        assert_eq!((t.tip.tip_id, t.tip.tip_hash), (202, carried_hash(&p202)), "state {}", state.is_some());
        assert_eq!(t.hashes.get(&202), Some(&carried_hash(&p202)));
    }
}

/// Review L2 (`signer-not-in-k-ok`): block 203, applied and final in the LEZ node, was inscribed by a key the
/// channel no longer accredits (channel-9: rotated out). The program refuses such a `T`, so the attestor passes
/// over it: with a state it signs 202 below it; a first read, whose run starts at 203, refuses until a block
/// by a key in K is final (a liveness pause, never a signature of 203).
#[test]
fn l2_a_tip_inscribed_by_a_key_outside_k_is_passed_over() {
    let mut a = answers_with_keys(&n3_keys());
    let lab = lab_answers();
    let p202 = payload_of(&lab, 202);
    let b203 = derive_block(&p202, 203, &carried_hash(&p202), 1);
    let (tx203, _) = inscription_tx(&key("channel-9"), &b203, &msg_of(&lab, 202), 12);
    let lib = a.lib_slot;
    a.blocks.push(node_block("203 by channel-9", lib, vec![tx203]));
    let mut lez = lez_lab(202, FINALIZED);
    lez.insert(203, Some(with_status(&b203, FINALIZED)));
    let t = read(&a, &lez, Some(&signed_200())).unwrap();
    assert_eq!((t.tip.tip_id, t.tip.tip_hash), (202, carried_hash(&p202)));
    assert!(
        t.notes.iter().any(|n| n.starts_with("block 203: inscribed by") && n.contains("does not accredit")),
        "{:?}",
        t.notes
    );
    let e = read(&a, &lez, None).unwrap_err().to_string();
    assert!(
        e.contains("refused: no block of the read is final in the LEZ node with an inscription that checks"),
        "{e}"
    );
}

/// The second review's N5 (`recorded-tip-signer-unchecked`): the lab key, which inscribed 186..=202, has left K
/// (the channel now accredits channel-1, -2 and -9), so every block of the read is passed over and nothing above
/// the signed tip 200 checks. The attestor does not fall back to signing 200 again as recorded: its signer is
/// outside K, and the program refuses such a `T` (a liveness pause, as L2's first read).
#[test]
fn n5_the_signed_tip_is_not_signed_again_once_its_signer_left_k() {
    let st200 = signed_200();
    let rotated = [pk(&key("channel-1")), pk(&key("channel-2")), pk(&key("channel-9"))];
    let e = read(&answers_with_keys(&rotated), &lez_lab(202, FINALIZED), Some(&st200)).unwrap_err().to_string();
    assert!(
        e.starts_with("refused: the signed tip 200 was inscribed by") && e.contains("no block above it checks"),
        "{e}"
    );
    // The same read with the lab key still in K signs 202.
    let t = read(&answers_with_keys(&n3_keys()), &lez_lab(202, FINALIZED), Some(&st200)).unwrap();
    assert_eq!(t.tip.tip_id, 202);
}

/// Review L5: the LEZ node is ahead of the Bedrock node's LIB (it holds the honest 203..=230 final, whose
/// inscriptions are not final on Bedrock yet), and a turn holder inscribed 28 junk blocks with those ids, final
/// on Bedrock. Each reads "final in the LEZ node" by its id and none is held byte for byte: the first read walks
/// down every one of them to 202 (a cap of 16 ids made it refuse).
#[test]
fn l5_junk_ids_above_the_chain_never_stop_a_first_read() {
    let mut a = answers_with_keys(&n3_keys());
    let lab = lab_answers();
    let p202 = payload_of(&lab, 202);
    let mut lez = lez_lab(202, FINALIZED);
    let mut prev = carried_hash(&p202);
    for id in 203..=230u64 {
        let honest = derive_block(&p202, id, &prev, id);
        prev = carried_hash(&honest);
        lez.insert(id, Some(with_status(&honest, FINALIZED)));
    }
    let k2 = key("channel-2");
    let junk: Vec<Value> = (203..=230u64)
        .map(|id| inscription_tx(&k2, &derive_block(&p202, id, &[0x77; 32], 7), &msg_of(&lab, 202), id as u8).0)
        .collect();
    assert!(junk.len() > 16);
    let lib = a.lib_slot;
    a.blocks.push(node_block("junk ids", lib, junk));
    let t = read(&a, &lez, None).unwrap();
    assert_eq!((t.tip.tip_id, t.tip.tip_hash, t.first_block), (202, carried_hash(&p202), 202));
    // With a state the run starts at the signed tip and the junk is passed over as before.
    assert_eq!(read(&a, &lez, Some(&signed_200())).unwrap().tip.tip_id, 202);
}

// ---- random sequences of rounds ---------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// The chain as an honest LEZ node and its Bedrock node see it, and what a turn holder adds.
struct World {
    nodes: Vec<Value>,
    next_slot: u64,
    lib: u64,
    /// The honest chain from 186: id, bytes, slot of its inscription.
    honest: Vec<(u64, Vec<u8>, u64)>,
    lez_final: u64,
    /// Every block made: hash -> (id, prev).
    all: HashMap<Hash, (u64, Hash)>,
    template: Vec<u8>,
    salt: u64,
}

impl World {
    fn new() -> Self {
        let a = answers_with_keys(&n3_keys());
        let honest: Vec<(u64, Vec<u8>, u64)> =
            (186..=202).map(|id| (id, payload_of(&a, id), slot_of(&a, id))).collect();
        let all = honest.iter().map(|(id, b, _)| (carried_hash(b), (*id, b[8..40].try_into().unwrap()))).collect();
        World {
            next_slot: a.lib_slot + 1,
            lib: a.lib_slot,
            template: honest.last().unwrap().1.clone(),
            nodes: a.blocks,
            honest,
            lez_final: 202,
            all,
            salt: 0,
        }
    }
    fn tip(&self) -> (u64, Hash) {
        let (id, b, _) = self.honest.last().unwrap();
        (*id, carried_hash(b))
    }
    fn block(&mut self, id: u64, prev: &Hash, salt: u64) -> Vec<u8> {
        let b = derive_block(&self.template, id, prev, 1000 * id + salt);
        self.all.insert(carried_hash(&b), (id, *prev));
        b
    }
    fn inscribe(&mut self, signer: &str, payload: &[u8], withdrawal: bool) -> u64 {
        let k = key(signer);
        self.salt += 1;
        let s = (self.salt % 200) as u8;
        let ins = inscribe_op(&k, payload, &[s; 32]);
        let ops = if withdrawal {
            vec![ins, channel_transfer(&[[s; 32]], &[(1, [s; 32])]), channel_withdraw(&[[s ^ 1; 32]]), fee_transfer(s)]
        } else {
            vec![ins, fee_transfer(s)]
        };
        let (j, _, _) = tx(&k, &ops, 0);
        let slot = self.next_slot;
        self.next_slot += 1;
        self.nodes.push(node_block(&format!("w{slot}"), slot, vec![j]));
        slot
    }
    fn extend(&mut self, rng: &mut Rng) {
        let (id, h) = self.tip();
        let b = self.block(id + 1, &h, 0);
        let signer = if rng.below(2) == 0 { "channel-1" } else { "channel-2" };
        let slot = self.inscribe(signer, &b, rng.below(4) == 0);
        self.honest.push((id + 1, b, slot));
    }
    /// Bedrock finality moves to the last slot or the one before; the LEZ node follows 0 or 1 block behind.
    fn settle(&mut self, rng: &mut Rng) {
        self.lib = self.lib.max(self.next_slot - 1 - rng.below(2));
        let final_honest = self.honest.iter().filter(|(_, _, s)| *s <= self.lib).map(|(id, _, _)| *id).max().unwrap();
        self.lez_final = self.lez_final.max(final_honest.saturating_sub(rng.below(2)));
    }
    fn lez(&self) -> Lez {
        self.honest
            .iter()
            .map(|(id, b, _)| (*id, Some(with_status(b, if *id <= self.lez_final { FINALIZED } else { PENDING }))))
            .collect()
    }
    fn answers(&self, nodes: &[Value], from: u64) -> NodeAnswers {
        let mut a = answers_with_keys(&n3_keys());
        a.lib_slot = self.lib;
        a.blocks = nodes.iter().filter(|b| slot(b) >= from).cloned().collect();
        a
    }
}

#[test]
fn m7_never_two_histories_across_restarts() {
    let d = Deployment { evm_chain_id: [0; 32], vault: [1; 20], lez_config_id: [2; 32], channel_id: channel() };
    let att = attestor(1, &d);
    for seed in [0x9e37_79b9_7f4a_7c15u64, 7, 1234567, 0xdead_beef] {
        let mut rng = Rng(seed);
        let mut w = World::new();
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("m7-{seed:x}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        let honest_hashes = |w: &World| w.honest.iter().map(|(_, b, _)| carried_hash(b)).collect::<HashSet<_>>();
        let mut signed_at: HashMap<u64, Hash> = HashMap::new();
        let mut last: Option<(u64, Hash)> = None;
        let (mut signed, mut refused, mut forged, mut compromised) = (0, 0, 0, 0);
        for round in 0..150 {
            // What happens in the world before this round.
            let mut nodes = w.nodes.clone();
            let mut lez = None;
            let mut expect_ok = true;
            match rng.below(12) {
                0..=4 => {
                    for _ in 0..=rng.below(3) {
                        w.extend(&mut rng);
                    }
                    w.settle(&mut rng);
                    nodes = w.nodes.clone();
                }
                5 | 6 => {
                    // A turn holder's block on the honest tip that LEZ parks, and blocks on top of it.
                    forged += 1;
                    let (id, h) = w.tip();
                    let f = w.block(id + 1, &h, 1 + rng.below(1000));
                    w.inscribe("channel-2", &f, false);
                    let mut prev = carried_hash(&f);
                    for k in 0..rng.below(3) {
                        let g = w.block(id + 2 + k, &prev, 7);
                        prev = carried_hash(&g);
                        w.inscribe("channel-1", &g, false);
                    }
                    w.settle(&mut rng);
                    nodes = w.nodes.clone();
                }
                7 => {
                    let len = rng.below(40) as usize;
                    let junk: Vec<u8> = (0..len).map(|_| rng.below(256) as u8).collect();
                    w.inscribe("channel-1", &junk, false);
                    w.settle(&mut rng);
                    nodes = w.nodes.clone();
                }
                8 | 9 if last.is_some() => {
                    // A compromised view for this round only: blocks that conflict with what was signed,
                    // from the signed tip or a little below it.
                    compromised += 1;
                    let (tip_id, _) = last.unwrap();
                    let f = (tip_id - rng.below(3)).max(187);
                    let mut prev = carried_hash(&w.honest[(f - 1 - 186) as usize].1);
                    let mut fork = Vec::new();
                    for id in f..=tip_id + 1 {
                        let b = w.block(id, &prev, 555 + rng.below(1000));
                        prev = carried_hash(&b);
                        fork.push(b);
                    }
                    let both = rng.below(2) == 0; // the LEZ node too, or only the Bedrock node
                    let mut l = w.lez();
                    for b in &fork {
                        let s = w.lib;
                        nodes.push(node_block(
                            &format!("fork{s}-{}", nodes.len()),
                            s,
                            vec![inscription_tx(&key("channel-2"), b, &[9; 32], 9).0],
                        ));
                        if both {
                            l.insert(u64::from_le_bytes(b[0..8].try_into().unwrap()), Some(with_status(b, FINALIZED)));
                        }
                    }
                    lez = Some(l);
                    expect_ok = !both;
                }
                10 => {
                    lez = Some(Lez::new()); // the LEZ node is down or wiped
                    expect_ok = false;
                }
                _ => {} // nothing new
            }
            // The round, as after a restart: the state is read from its file.
            let state = read_state(&path, &channel()).unwrap();
            let answers = w.answers(&nodes, scan_from(state.as_ref(), 0));
            let mut lez = lez.unwrap_or_else(|| w.lez());
            let out = final_tier(&answers, &mut lez, &channel(), state.as_ref())
                .and_then(|t| Ok((att.attest(&t, state.as_ref().map(|s| s.last))?, t)));
            match out {
                Ok((a, t)) => {
                    write_state(&path, &channel(), &AttestorState::of(&t)).unwrap();
                    let now = (a.tip.tip_id, a.tip.tip_hash);
                    assert!(
                        honest_hashes(&w).contains(&now.1),
                        "seed {seed:x} round {round}: signed a block not on the honest chain"
                    );
                    if let Some(h) = signed_at.insert(now.0, now.1) {
                        assert_eq!(h, now.1, "seed {seed:x} round {round}: two blocks signed at {}", now.0);
                    }
                    if let Some((lid, lh)) = last {
                        assert!(now.0 >= lid, "seed {seed:x} round {round}: signed {} after {lid}", now.0);
                        let mut cur = now;
                        while cur.0 > lid {
                            let (_, prev) = w.all[&cur.1];
                            cur = (cur.0 - 1, prev);
                        }
                        assert_eq!(
                            cur.1, lh,
                            "seed {seed:x} round {round}: {} does not descend from the signed {lid}",
                            now.0
                        );
                    }
                    if expect_ok && lez_is_honest(&lez, &w) {
                        assert_eq!(
                            now.0, w.lez_final,
                            "seed {seed:x} round {round}: the tip is the LEZ node's final tip"
                        );
                    }
                    last = Some(now);
                    signed += 1;
                }
                Err(e) => {
                    assert!(!expect_ok, "seed {seed:x} round {round}: refused an honest round: {e:#}");
                    refused += 1;
                }
            }
        }
        let top = last.unwrap().0;
        eprintln!("seed {seed:x}: {signed} rounds signed, {refused} refused, {forged} forged turns, {compromised} compromised views; signed up to {top} (honest tip {})", w.tip().0);
        assert!(top > 230 && forged > 5 && compromised > 5 && refused > 3, "seed {seed:x}: the run reached too little");
        let _ = fs::remove_dir_all(&dir);
    }
}

/// Whether `lez` is the honest node's map (no compromised view, no outage).
fn lez_is_honest(lez: &Lez, w: &World) -> bool {
    *lez == w.lez()
}
