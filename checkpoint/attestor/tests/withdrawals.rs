//! Review M-3 (CK-2): LEZ publishes every block that carries withdrawals with `publish_atomic_withdraw`,
//! whose inscription transaction is `[ChannelInscribe, ChannelTransfer (0x14), ChannelWithdraw (0x13),
//! Transfer]`. The attestor refused every final tier holding one ("not one LEZ sends"), so one withdrawal
//! stopped every attestor. Here such a block (203, on the lab's real 202) is signed, and the checkpoint
//! program proves it as `T` from the same transaction bytes: the attestor's encoding and the program's
//! framing agree. The transaction's bytes, hash and signature are written by `support` from
//! `checkpoint-core::mantle`'s format, independently of the attestor's encoder.
mod support;

use std::fs;

use attestor_core::{sign_certificate, Deployment};
use checkpoint_core::input::CheckpointInputB;
use checkpoint_core::mantle::{opcodes, ElidedInscription};
use checkpoint_core::rule_b::run_checkpoint_b;
use ferrata_attestor::{
    collect, deployment_of, devnet_lez_standin, final_inscriptions, final_tier, rekey_base, upgrade_pre_producer_input,
    Attestor, FinalTier, NodeAnswers,
};
use serde_json::Value;
use support::*;

/// The real span 186..=202 with T's inscription, re-keyed to {lab, k1, k2} with nMin 3, tMin 2.
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

/// The final tier read with a LEZ node that holds every inscribed block final (the honest case).
fn tier_of(a: &NodeAnswers) -> FinalTier {
    final_tier(a, &mut devnet_lez_standin(a, &channel()).unwrap(), &channel(), None).unwrap()
}

/// Block 203 on the real 202, inscribed by k1 in a transaction of `ops` (the inscription at `at`).
fn withdrawal_block(a: &mut NodeAnswers, ops: impl Fn(Op) -> Vec<Op>, at: usize) -> (Vec<u8>, Vec<u8>, Value) {
    let p202 = payload_of(a, 202);
    let b203 = derive_block(&p202, 203, &carried_hash(&p202), 1);
    let k1 = key("channel-1");
    let (txj, bytes, _) = tx(&k1, &ops(inscribe_op(&k1, &b203, &msg_of(a, 202))), at);
    a.blocks.push(node_block("withdrawal", a.lib_slot, vec![txj.clone()]));
    (b203, bytes, txj)
}

#[test]
fn a_block_with_an_atomic_withdrawal_is_signed_and_proven() {
    let mut a = answers_with_keys(&n3_keys());
    let lez_shape = |ins: Op| {
        vec![ins, channel_transfer(&[[5; 32], [6; 32]], &[(5, [8; 32])]), channel_withdraw(&[[9; 32]]), fee_transfer(3)]
    };
    let (b203, bytes, txj) = withdrawal_block(&mut a, lez_shape, 0);
    assert_eq!(opcodes(&bytes).unwrap(), vec![0x11, 0x14, 0x13, 0x00], "checkpoint-core frames the transaction");
    let tier = tier_of(&a);
    assert_eq!((tier.tip.tip_id, tier.tip.tip_hash), (203, carried_hash(&b203)));
    assert_eq!(tier.tip_signer, pk(&key("channel-1")));

    // Two of three attestors sign it; the program proves T = 203 from the same transaction.
    let base = base_n3();
    let d = deployment_of(&CheckpointInputB::decode(&base).unwrap().config);
    let atts = [attestor(1, &d).attest(&tier, None).unwrap(), attestor(2, &d).attest(&tier, None).unwrap()];
    let mut input = CheckpointInputB::decode(&base).unwrap();
    input.blocks.push(&b203);
    input.t_index = 17;
    let sig: [u8; 64] = ferrata_attestor::hexa(&txj["ops_proofs"][0]["Ed25519Sig"], "sig").unwrap();
    input.producer = ElidedInscription::split(&bytes, 0, &sig).unwrap();
    let with_t = input.encode();
    let c = collect(&with_t, &atts).unwrap();
    let outcome = run_checkpoint_b(&CheckpointInputB::decode(&c.input).unwrap()).unwrap();
    assert_eq!((outcome.journal.tip_id, outcome.journal.tip_hash, outcome.attestors), (203, carried_hash(&b203), 2));
    assert_eq!(outcome.producer, pk(&key("channel-1")));
}

#[test]
fn a_channel_transfer_alone_and_two_withdrawals_are_encoded() {
    for (what, ops) in [
        (
            "channel transfer",
            (|ins: Op| vec![ins, channel_transfer(&[[1; 32]], &[]), fee_transfer(4)]) as fn(Op) -> Vec<Op>,
        ),
        ("two withdrawals", |ins: Op| {
            vec![ins, channel_withdraw(&[[2; 32], [3; 32]]), channel_withdraw(&[]), fee_transfer(5)]
        }),
    ] {
        let mut a = answers_with_keys(&n3_keys());
        let (b203, _, _) = withdrawal_block(&mut a, ops, 0);
        assert_eq!(tier_of(&a).tip.tip_hash, carried_hash(&b203), "{what}");
    }
}

/// The program frames an inscription at any position of its transaction, and so does the attestor now:
/// a turn holder that puts its block's inscription second cannot stall the attestors.
#[test]
fn an_inscription_that_is_not_the_first_op_is_signed_and_proven() {
    let mut a = answers_with_keys(&n3_keys());
    let second = |ins: Op| vec![fee_transfer(6), ins, channel_withdraw(&[[7; 32]])];
    let (b203, bytes, txj) = withdrawal_block(&mut a, second, 1);
    let tier = tier_of(&a);
    assert_eq!((tier.tip.tip_id, tier.tip.tip_hash), (203, carried_hash(&b203)));
    let base = base_n3();
    let d = deployment_of(&CheckpointInputB::decode(&base).unwrap().config);
    let atts = [attestor(1, &d).attest(&tier, None).unwrap(), attestor(2, &d).attest(&tier, None).unwrap()];
    let mut input = CheckpointInputB::decode(&base).unwrap();
    input.blocks.push(&b203);
    input.t_index = 17;
    let sig: [u8; 64] = ferrata_attestor::hexa(&txj["ops_proofs"][1]["Ed25519Sig"], "sig").unwrap();
    input.producer = ElidedInscription::split(&bytes, 1, &sig).unwrap();
    let with_t = input.encode();
    let outcome =
        run_checkpoint_b(&CheckpointInputB::decode(&collect(&with_t, &atts).unwrap().input).unwrap()).unwrap();
    assert_eq!((outcome.journal.tip_id, outcome.attestors), (203, 2));
}
