//! The bridge code review's probe P1 (22 November 2026), kept as the review ran it (only this header changed): copied
//! into lez/harness/tests on a checkout, run, and deleted; docs/reviews/lost-burn-2026-11-22/README.md says how.
//! P1: two burns that reuse one tag `r` (same wallet counter `i`) but name different recipients are both
//! accepted by the deployed bridge program (the burn account is keyed on the hash of the whole note,
//! D13), and both have the same `sOut`, hence the same release nullifier NOut = H("ferrata/nf-out/v2" || sOut):
//! the vault (`nullifierUsed[NOut]`) can release only one of them.

use ferrata_bridge_core::{
    chain_id_u256,
    outbound::{burn_commitment, burn_nullifier, burn_secret, burn_tag, xor_memo},
    BridgeKeys, Hash,
};
use ferrata_lez_bridge_wire::{burn_actor, burn_mac_v2, own_actor, BridgeMessage, BridgeState, Note};
use harness::bridge::{World, CHAIN_ID, DENOMINATIONS, ETH};

fn note_to(keys: &BridgeKeys, cid: &Hash, vault: &[u8; 20], i: u64, amount: u128, recipient: [u8; 20]) -> (Note, Hash) {
    let r = burn_tag(&keys.view, cid, i);
    let s_out = burn_secret(&keys.auth, cid, &r);
    let c_out = burn_commitment(&s_out, &chain_id_u256(CHAIN_ID), vault, &recipient);
    let memo = xor_memo(&keys.view, &r, &recipient);
    let amount = ferrata_lez_bridge_wire::word_of_u128(amount);
    let mac = burn_mac_v2(&keys.view, &r, &memo, &ETH, &amount, &c_out);
    (Note { r, memo, mac, token: ETH, amount, c_out }, s_out)
}

#[test]
fn p1_two_burns_with_one_tag_both_apply_and_share_nout() {
    assert_eq!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1"), "run with RISC0_DEV_MODE=1");
    let mut w = World::with_roots();
    let cid_a = w.cid_a;
    let vault_a = w.vault_a.address;
    let payer = w.chain.payer;
    let program = w.program;
    let witness = w.vault_a.witness(&w.payer_deposit.clone(), 8);
    let r = w.claim("P1 setup: claim leaf 6 (10 ETH to the payer)", &cid_a, 8, &vault_a, witness);
    assert!(r.ok(), "setup claim: {:?} {:?}", r.exit_code, r.charge.revert);
    let start = w.balance(payer, &cid_a);

    let keys = BridgeKeys::from_bridge_seed(&[0x77; 32]);
    // The same counter i = 0 (as the CLI computes it from the applied history while the first burn is in flight).
    let (n1, s1) = note_to(&keys, &cid_a, &vault_a, 0, DENOMINATIONS[1], [0xe1; 20]);
    let (n2, s2) = note_to(&keys, &cid_a, &vault_a, 0, DENOMINATIONS[1], [0xe2; 20]);
    assert_eq!(n1.r, n2.r, "one tag");
    assert_ne!(n1.hash(), n2.hash(), "two notes, two burn accounts");
    assert_eq!(s1, s2, "one sOut");
    assert_eq!(burn_nullifier(&s1), burn_nullifier(&s2), "one release nullifier for both burns");

    let holder = own_actor(program, payer);
    for (label, n) in [("P1 burn 1 (recipient e1, i = 0)", &n1), ("P1 burn 2 (recipient e2, the same i = 0)", &n2)] {
        let actors = w.public_burn_actors(holder.account_id, &cid_a, n);
        let rep = w.send(label, holder, &BridgeMessage::burn(cid_a, n), actors);
        println!("PROBE {label}: ok={} exit={:?} gas_exec={}", rep.ok(), rep.exit_code, rep.charge.gas_exec);
        assert!(rep.ok(), "{label}: {:?} {:?}", rep.exit_code, rep.charge.revert);
    }
    for n in [&n1, &n2] {
        let st = w.chain.actor_state(burn_actor(program, &cid_a, &n.hash()));
        assert!(matches!(borsh::from_slice::<BridgeState>(&st).unwrap(), BridgeState::Burn(_)), "burn record written");
    }
    assert_eq!(w.balance(payer, &cid_a), start - 2 * DENOMINATIONS[1], "both denominations debited");
    println!(
        "PROBE P1 RESULT: both burns applied; balance {} -> {}; shared NOut {}",
        start,
        w.balance(payer, &cid_a),
        hex::encode(burn_nullifier(&s1))
    );
}
