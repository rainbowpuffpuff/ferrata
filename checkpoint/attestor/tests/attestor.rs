//! ferrata_attestor over the lab's real node answers (vectors/lab-node: the node blocks that hold
//! the inscriptions of LEZ blocks 186..=202, its /channel and /cryptarchia/info answers, copied from
//! the extraction of 10 November) and the real rule (B) span realblocks/out/real-lab-186-202.input.
//!
//! The lab runs one sequencer (n = 1). For t of n with t > 1 the tests re-key the parent state to a
//! stand-in set K = {the lab's key (producer of T), k1, k2} with nMin = 3, tMin = 2, and give the
//! node's channel answer the same three keys.
//!
//! The attestor signs only what its LEZ node holds final. The lab's LEZ node is not recorded; its
//! sequencer served all of its blocks `Finalized` (measured, `docs/design/lez-v0.4-findings.md` §3), so
//! these tests stand it in with every inscribed block served as final ([`honest_lez`]). The tests of what
//! the LEZ node decides are in `lez_check.rs`.
use std::fs;
use std::path::PathBuf;

use attestor_core::{sign_certificate, verify_certificate, Deployment, SignedAttestation};
use checkpoint_core::hash::sha256_parts;
use checkpoint_core::input::CheckpointInputB;
use checkpoint_core::rule_b::run_checkpoint_b;
use checkpoint_core::{Error, Hash};
use ed25519_dalek::SigningKey;
use ferrata_attestor::{
    assemble_unchecked, bedrock_tier, channel_config_tx, collect, deployment_of, devnet_lez_standin,
    final_inscriptions, final_tier, hexa, rekey_base, rekey_base_with_timeframe, retip, rotate_base,
    upgrade_pre_producer_input, Attestor, AttestorState, Dropped, FinalTier, LastSigned, NodeAnswers,
};
use serde_json::{json, Value};

const LAB_KEY: &str = "3b14e3980d05dd3651ff5956758dd4cfecc33b837775c9e3d90279d34d3fb0a2";
const TIP_HASH: &str = "774780c60182852a283544372f21f24d9f532953bbdd8d71823a012edb322bbb";
const TIP_MSG: &str = "cc5ae8ce6324d1be95406175c3ba3934914e8ff91fac717ae3d95eb568cca79e";
const CHANNEL: &str = "0101010101010101010101010101010101010101010101010101010101010101";

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn h(s: &str) -> Hash {
    hexa(&json!(s), "hex").unwrap()
}

fn key(label: &str) -> SigningKey {
    SigningKey::from_bytes(&sha256_parts(&[b"ferrata/attestor/devnet-key/", label.as_bytes()]))
}

fn pk(k: &SigningKey) -> Hash {
    k.verifying_key().to_bytes()
}

fn lab_answers() -> NodeAnswers {
    NodeAnswers::load(&dir().join("vectors/lab-node")).unwrap()
}

/// The lab's LEZ node, stood in: every block inscribed on the channel, served as final.
fn honest_lez() -> std::collections::BTreeMap<u64, Option<Vec<u8>>> {
    devnet_lez_standin(&lab_answers(), &h(CHANNEL)).unwrap()
}

/// The final tier read with the honest LEZ node, from `state` (or a first read).
fn read(answers: &NodeAnswers, state: Option<&AttestorState>) -> FinalTier {
    final_tier(answers, &mut honest_lez(), &h(CHANNEL), state).unwrap()
}

/// A first-version state naming block `id` of the lab chain (the run is then followed from it).
fn state_at(id: u64) -> AttestorState {
    let hash = bedrock_tier(&lab_answers(), &h(CHANNEL)).unwrap().hashes[&id];
    AttestorState { last: LastSigned { tip_id: id, tip_hash: hash }, record: None }
}

/// The real span with T's inscription as the node holds it (the recorded input predates the field).
fn real_input() -> Vec<u8> {
    let old = fs::read(dir().join("../realblocks/out/real-lab-186-202.input")).unwrap();
    let ins = final_inscriptions(&lab_answers(), &h(CHANNEL)).unwrap();
    upgrade_pre_producer_input(&old, &ins[&202].elided().unwrap()).unwrap()
}

/// The real span, re-keyed to {lab, k1, k2} with nMin 3, tMin 2.
fn base_n3() -> Vec<u8> {
    let real = real_input();
    rekey_base(&real, &[h(LAB_KEY), pk(&key("channel-1")), pk(&key("channel-2"))], 3, 2).unwrap()
}

fn deployment(base: &[u8]) -> Deployment {
    deployment_of(&CheckpointInputB::decode(base).unwrap().config)
}

/// The node's answers with the stand-in key set.
fn tier_n3() -> FinalTier {
    let mut answers = lab_answers();
    answers.channel["accredited_keys"] =
        json!([LAB_KEY, hex::encode(pk(&key("channel-1"))), hex::encode(pk(&key("channel-2")))]);
    read(&answers, None)
}

fn attestor(i: u32, d: &Deployment) -> Attestor {
    let channel = key(&format!("channel-{i}"));
    let att = key(&format!("attestation-{i}"));
    Attestor {
        deployment: *d,
        channel_key: pk(&channel),
        certificate: sign_certificate(&channel, d, &pk(&att)),
        att_key: att,
    }
}

/// The lab sequencer's attestor of the real-blocks run: its attestation key (public label) and the
/// certificate the lab's channel key gave it for the nMin = 1 deployment of that run.
fn lab_attestor(d: &Deployment) -> Attestor {
    let v: Value = serde_json::from_slice(&fs::read(dir().join("../realblocks/out/attestor.json")).unwrap()).unwrap();
    Attestor {
        deployment: *d,
        channel_key: hexa(&v["channel_key"], "channel_key").unwrap(),
        certificate: hexa(&v["certificate"], "certificate").unwrap(),
        att_key: SigningKey::from_bytes(&sha256_parts(&[b"ferrata/realblocks/attestation-key/v1"])),
    }
}

fn other_vault(d: &Deployment) -> Deployment {
    let mut o = *d;
    o.vault[19] ^= 1;
    o
}

fn other_chain(d: &Deployment) -> Deployment {
    let mut o = *d;
    o.evm_chain_id[31] ^= 1; // 31337 -> 31336
    o
}

fn other_config(d: &Deployment) -> Deployment {
    let mut o = *d;
    o.lez_config_id[0] ^= 1;
    o
}

fn run(bytes: &[u8]) -> Result<checkpoint_core::rule_b::OutcomeB, Error> {
    run_checkpoint_b(&CheckpointInputB::decode(bytes).unwrap())
}

#[test]
fn reads_the_lab_nodes_final_tier() {
    let channel = h("0101010101010101010101010101010101010101010101010101010101010101");
    // A first read starts at the highest block the LEZ node holds final.
    let tier = read(&lab_answers(), None);
    assert!(tier.lez_checked);
    assert_eq!(tier.keys, vec![h(LAB_KEY)]);
    assert_eq!(tier.tip.tip_id, 202);
    assert_eq!(tier.tip.tip_hash, h(TIP_HASH));
    assert_eq!(tier.tip.final_msg_id, h(TIP_MSG));
    assert_eq!(tier.tip_signer, h(LAB_KEY));
    assert_eq!(tier.lib_slot, 8_856_126);
    assert_eq!((tier.first_block, tier.hashes.len()), (202, 1));
    // From a state at 186, the run 186..=202 is followed.
    let from186 = read(&lab_answers(), Some(&state_at(186)));
    assert_eq!((from186.first_block, from186.hashes.len(), from186.tip.tip_id), (186, 17, 202));
    assert_eq!(from186.tip.final_msg_id, h(TIP_MSG));
    assert!(from186.notes.is_empty(), "{:?}", from186.notes);
    // The Bedrock node's view alone gives the same run and tip, but is not signable.
    let bedrock = bedrock_tier(&lab_answers(), &channel).unwrap();
    assert_eq!((bedrock.first_block, bedrock.hashes.len(), bedrock.tip), (186, 17, tier.tip));
    assert_eq!(bedrock.hashes, from186.hashes);
    assert!(!bedrock.lez_checked);
    // The channel's own tip message is T's inscription: the node's final tier ends at T.
    assert_eq!(lab_answers().channel["tip_message"].as_str(), Some(TIP_MSG));
}

#[test]
fn a_tip_beyond_the_nodes_lib_is_not_final() {
    let mut answers = lab_answers();
    answers.lib_slot = 8_856_116; // one slot below T's node block
    let tier = read(&answers, None);
    assert_eq!(tier.tip.tip_id, 201);
    // The exact limit: T's node block at lib_slot is final.
    answers.lib_slot = 8_856_117;
    assert_eq!(read(&answers, None).tip.tip_id, 202);
}

#[test]
fn the_lab_certificate_is_real_and_binds_chain_vault_configuration_and_channel() {
    let real = real_input();
    // The upgraded real input is accepted as it was recorded: n = 1, t = 1, the lab's attestation.
    let outcome = run(&real).unwrap();
    assert_eq!((outcome.journal.n, outcome.journal.t, outcome.producer), (1, 1, h(LAB_KEY)));
    let d1 = deployment(&real);
    let a0 = lab_attestor(&d1);
    assert!(verify_certificate(&a0.channel_key, &d1, &pk(&a0.att_key), &a0.certificate));
    // lezConfigId does not cover nMin and tMin (they are journal words the vault pins on their own):
    // the three-key deployment is the same deployment for the certificate.
    let d3 = deployment(&base_n3());
    assert_eq!(d1, d3);
    for other in [other_vault(&d1), other_chain(&d1), other_config(&d1)] {
        assert!(!verify_certificate(&a0.channel_key, &other, &pk(&a0.att_key), &a0.certificate));
    }
}

#[test]
fn three_attestors_sign_and_any_two_are_accepted() {
    let base = base_n3();
    let d = deployment(&base);
    let tier = tier_n3();
    assert_eq!(tier.keys.len(), 3);
    // The lab sequencer's attestor with its real certificate, and two stand-in sequencers' attestors.
    let a0 = lab_attestor(&d).attest(&tier, None).unwrap();
    let a1 = attestor(1, &d).attest(&tier, None).unwrap();
    let a2 = attestor(2, &d).attest(&tier, None).unwrap();
    let all = collect(&base, &[a0, a1, a2]).unwrap();
    assert_eq!((all.n, all.t, all.kept.len()), (3, 2, 3));
    let outcome = run(&all.input).unwrap();
    assert_eq!((outcome.journal.n, outcome.journal.t, outcome.attestors), (3, 2, 3));
    assert_eq!((outcome.journal.n_min, outcome.journal.t_min), (3, 2));
    assert_eq!(outcome.journal.tip_id, 202);
    assert_eq!(outcome.journal.tip_hash, h(TIP_HASH));
    assert_eq!(outcome.producer, h(LAB_KEY));
    for pair in [[a0, a1], [a0, a2], [a1, a2]] {
        let c = collect(&base, &pair).unwrap();
        assert_eq!(run(&c.input).unwrap().attestors, 2);
    }
    // Rule (B) journals over the same blocks and key set do not depend on who attested.
    let j = |atts: &[SignedAttestation]| run(&collect(&base, atts).unwrap().input).unwrap().journal;
    assert_eq!(j(&[a1, a2]), j(&[a0, a1, a2]));
}

#[test]
fn refused_too_few_signatures() {
    let base = base_n3();
    let d = deployment(&base);
    let a1 = attestor(1, &d).attest(&tier_n3(), None).unwrap();
    assert!(collect(&base, &[a1]).unwrap_err().to_string().contains("too few attestations: 1 valid"));
    let bytes = assemble_unchecked(&base, &[a1]).unwrap();
    assert_eq!(run(&bytes).unwrap_err(), Error::InsufficientAttestations { have: 1, need: 2 });
    // None at all.
    assert_eq!(
        run(&assemble_unchecked(&base, &[]).unwrap()).unwrap_err(),
        Error::InsufficientAttestations { have: 0, need: 2 }
    );
}

#[test]
fn refused_an_uncertified_key() {
    let base = base_n3();
    let d = deployment(&base);
    let tier = tier_n3();
    let a1 = attestor(1, &d).attest(&tier, None).unwrap();
    // k2 certified attestation-2; attestation-3 signs, presenting k2's certificate of attestation-2.
    let mut rogue = attestor(2, &d);
    rogue.att_key = key("attestation-3");
    assert!(rogue.attest(&tier, None).unwrap_err().to_string().contains("does not certify"));
    let forged =
        SignedAttestation::new(&rogue.att_key, rogue.channel_key, rogue.certificate, &d, &tier.key_set_hash, tier.tip);
    let c = collect(&base, &[a1, forged]);
    assert!(c.is_err(), "the collector keeps one, below t");
    assert_eq!(run(&assemble_unchecked(&base, &[a1, forged]).unwrap()).unwrap_err(), Error::BadCertificate);
    // A channel key outside K certifying its own attestation key.
    let outsider = attestor(3, &d);
    let out = SignedAttestation::new(
        &outsider.att_key,
        outsider.channel_key,
        outsider.certificate,
        &d,
        &tier.key_set_hash,
        tier.tip,
    );
    assert_eq!(run(&assemble_unchecked(&base, &[a1, out]).unwrap()).unwrap_err(), Error::NotAccredited);
    assert!(outsider.attest(&tier, None).unwrap_err().to_string().contains("not in the channel's live key set"));
}

#[test]
fn refused_a_key_of_another_deployment() {
    let base = base_n3();
    let d = deployment(&base);
    let tier = tier_n3();
    let a1 = attestor(1, &d).attest(&tier, None).unwrap();
    let ch2 = key("channel-2");
    let att2 = key("attestation-2");
    for other in [other_vault(&d), other_chain(&d), other_config(&d)] {
        // k2 certified attestation-2 for another deployment; attestation-2 signs this one.
        let cert_other = sign_certificate(&ch2, &other, &pk(&att2));
        let a2 = SignedAttestation::new(&att2, pk(&ch2), cert_other, &d, &tier.key_set_hash, tier.tip);
        assert_eq!(run(&assemble_unchecked(&base, &[a1, a2]).unwrap()).unwrap_err(), Error::BadCertificate);
        assert!(collect(&base, &[a1, a2]).unwrap_err().to_string().contains("Certificate"));
        // ... or signs entirely for the other deployment (certificate and message).
        let a2_other = SignedAttestation::new(&att2, pk(&ch2), cert_other, &other, &tier.key_set_hash, tier.tip);
        assert_eq!(run(&assemble_unchecked(&base, &[a1, a2_other]).unwrap()).unwrap_err(), Error::BadCertificate);
        // The attestor itself refuses to sign with a certificate of another deployment.
        let wrong = Attestor { deployment: d, channel_key: pk(&ch2), certificate: cert_other, att_key: att2.clone() };
        assert!(wrong.attest(&tier, None).unwrap_err().to_string().contains("does not certify"));
    }
    // The lab key's real certificate presented to a vault of another deployment.
    let lab = lab_attestor(&other_vault(&d));
    assert!(lab.attest(&tier, None).unwrap_err().to_string().contains("does not certify"));
}

#[test]
fn refused_a_repeated_key_and_a_signature_over_another_key_set() {
    let base = base_n3();
    let d = deployment(&base);
    let tier = tier_n3();
    let a1 = attestor(1, &d).attest(&tier, None).unwrap();
    assert_eq!(run(&assemble_unchecked(&base, &[a1, a1]).unwrap()).unwrap_err(), Error::DuplicateAttestor);
    let c = collect(&base, &[a1, a1]);
    assert!(c.is_err());
    // Attestor 2 signs the one-key set its node would have shown before the stand-in.
    let lab_tier = read(&lab_answers(), None);
    let a2 = attestor(2, &d);
    let stale =
        SignedAttestation::new(&a2.att_key, a2.channel_key, a2.certificate, &d, &lab_tier.key_set_hash, lab_tier.tip);
    assert_eq!(run(&assemble_unchecked(&base, &[a1, stale]).unwrap()).unwrap_err(), Error::BadAttestation);
}

#[test]
fn the_attestor_never_signs_two_histories() {
    let d = deployment(&base_n3());
    let mut answers = lab_answers();
    answers.channel["accredited_keys"] =
        json!([LAB_KEY, hex::encode(pk(&key("channel-1"))), hex::encode(pk(&key("channel-2")))]);
    let tier = read(&answers, Some(&state_at(186)));
    let a1 = attestor(1, &d);
    a1.attest(&tier, Some(LastSigned { tip_id: 202, tip_hash: h(TIP_HASH) })).unwrap();
    a1.attest(&tier, Some(LastSigned { tip_id: 190, tip_hash: tier.hashes[&190] })).unwrap();
    let mut other = h(TIP_HASH);
    other[0] ^= 1;
    let e = a1.attest(&tier, Some(LastSigned { tip_id: 202, tip_hash: other })).unwrap_err().to_string();
    assert!(e.contains("this attestor signed"), "{e}");
    let e = a1.attest(&tier, Some(LastSigned { tip_id: 203, tip_hash: other })).unwrap_err().to_string();
    assert!(e.contains("below the tip"), "{e}");
    // A signed tip below the run read (review M7's case): the tier does not show that it descends from it.
    let e =
        a1.attest(&tier, Some(LastSigned { tip_id: tier.first_block - 1, tip_hash: other })).unwrap_err().to_string();
    assert!(e.contains("does not hold block 185"), "{e}");
    // A Bedrock-only view is never signed.
    let bedrock = bedrock_tier(&answers, &h(CHANNEL)).unwrap();
    assert!(a1.attest(&bedrock, None).unwrap_err().to_string().contains("not checked against a LEZ node"));
}

#[test]
fn a_dropped_reason_for_each_bad_attestation() {
    let base = base_n3();
    let d = deployment(&base);
    let tier = tier_n3();
    let a1 = attestor(1, &d).attest(&tier, None).unwrap();
    let a2 = attestor(2, &d).attest(&tier, None).unwrap();
    let ch0 = h(LAB_KEY);
    let lab = lab_attestor(&d);
    let cert_other = sign_certificate(&key("channel-2"), &other_vault(&d), &pk(&key("attestation-2")));
    let bad_cert = SignedAttestation::new(
        &key("attestation-2"),
        pk(&key("channel-2")),
        cert_other,
        &d,
        &tier.key_set_hash,
        tier.tip,
    );
    let outsider = attestor(3, &d);
    let out = SignedAttestation::new(
        &outsider.att_key,
        outsider.channel_key,
        outsider.certificate,
        &d,
        &tier.key_set_hash,
        tier.tip,
    );
    let mut bad_sig = lab.attest(&tier, None).unwrap();
    bad_sig.signature[0] ^= 1;
    assert_eq!(bad_sig.channel_key, ch0);
    let c = collect(&base, &[a1, bad_cert, out, a1, bad_sig, a2]).unwrap();
    assert_eq!(c.kept, vec![0, 5]);
    assert_eq!(
        c.dropped,
        vec![(1, Dropped::Certificate), (2, Dropped::NotInKeySet), (3, Dropped::Repeated), (4, Dropped::Signature)]
    );
    assert_eq!(run(&c.input).unwrap().attestors, 2);
}

#[test]
fn attestations_of_a_descendant_of_t_count_and_an_attestor_behind_t_is_refused() {
    let base = base_n3();
    let d = deployment(&base);
    let tier = tier_n3();
    let a0 = lab_attestor(&d).attest(&tier, None).unwrap();
    let a1 = attestor(1, &d).attest(&tier, None).unwrap();
    // T = 200: blocks 201 and 202 are the path after T; both attestations are of 202.
    let mut answers = lab_answers();
    answers.channel["accredited_keys"] =
        json!([LAB_KEY, hex::encode(pk(&key("channel-1"))), hex::encode(pk(&key("channel-2")))]);
    let base200 = retip(&base, &answers, 200).unwrap();
    let c = collect(&base200, &[a0, a1]).unwrap();
    let outcome = run(&c.input).unwrap();
    assert_eq!((outcome.journal.tip_id, outcome.path_after_tip, outcome.attestors), (200, 2, 2));
    assert_eq!(outcome.journal.tip_hash, state_at(200).last.tip_hash);
    // An attestor whose node had finalised only up to 201 cannot support T = 202.
    answers.lib_slot = 8_856_116;
    let behind = read(&answers, None);
    assert_eq!(behind.tip.tip_id, 201);
    let a2_behind = attestor(2, &d).attest(&behind, None).unwrap();
    assert_eq!(
        run(&assemble_unchecked(&base, &[a0, a2_behind]).unwrap()).unwrap_err(),
        Error::NotADescendant { tip_id: 201 }
    );
    // ... but it supports T = 200 (201 is on the path after 200).
    let ok = run(&assemble_unchecked(&base200, &[a0, a2_behind]).unwrap()).unwrap();
    assert_eq!(ok.attestors, 2);
}

#[test]
fn a_rotation_by_two_of_three_and_attestations_under_the_new_set() {
    // Stand-in: a non-zero posting timeframe (a rotation needs one, SPEC-v3 D3; the lab's channel has 0).
    const P: u32 = 20;
    let (k1, k2, k3) = (key("channel-1"), key("channel-2"), key("channel-3"));
    let old = [h(LAB_KEY), pk(&k1), pk(&k2)];
    let new = [h(LAB_KEY), pk(&k1), pk(&k3)];
    let base = rekey_base_with_timeframe(&real_input(), &old, 3, 2, Some(P)).unwrap();
    let d = deployment(&base);
    let ops = channel_config_tx(&h(CHANNEL), &[0u8; 32], &new, P, 2).unwrap();
    let rotated = rotate_base(&base, &ops, &[(1, &k1), (2, &k2)]).unwrap();
    let tier_with = |keys: &[Hash]| {
        let mut answers = lab_answers();
        answers.channel["accredited_keys"] = json!(keys.iter().map(hex::encode).collect::<Vec<_>>());
        answers.channel["posting_timeframe"] = json!(P);
        read(&answers, None)
    };
    let tier = tier_with(&new);
    let a0 = lab_attestor(&d).attest(&tier, None).unwrap();
    let a1 = attestor(1, &d).attest(&tier, None).unwrap();
    let c = collect(&rotated, &[a0, a1]).unwrap();
    let outcome = run(&c.input).unwrap();
    assert_eq!(outcome.new_state.keys, new.to_vec());
    assert_eq!((outcome.journal.n, outcome.journal.t, outcome.attestors), (3, 2, 2));
    // Refused: a key the rotation removed (k2's attestor refuses to sign; a forced signature is not counted).
    let at2 = attestor(2, &d);
    assert!(at2.attest(&tier, None).unwrap_err().to_string().contains("not in the channel's live key set"));
    let a2 = SignedAttestation::new(&at2.att_key, at2.channel_key, at2.certificate, &d, &tier.key_set_hash, tier.tip);
    assert_eq!(run(&assemble_unchecked(&rotated, &[a0, a2]).unwrap()).unwrap_err(), Error::NotAccredited);
    // Refused: the rotation signed by one key of three.
    let weak = rotate_base(&base, &ops, &[(1, &k1)]).unwrap();
    assert_eq!(
        run(&assemble_unchecked(&weak, &[a0, a1]).unwrap()).unwrap_err(),
        Error::ConfigThresholdUnmet { have: 1, need: 2 }
    );
    // Refused: attestations over the set before the rotation.
    let before = tier_with(&old);
    let b0 = lab_attestor(&d).attest(&before, None).unwrap();
    let b1 = attestor(1, &d).attest(&before, None).unwrap();
    assert_eq!(run(&assemble_unchecked(&rotated, &[b0, b1]).unwrap()).unwrap_err(), Error::BadAttestation);
    // ... and they are what the input without the rotation needs.
    assert_eq!(run(&collect(&base, &[b0, b1]).unwrap().input).unwrap().attestors, 2);
}
