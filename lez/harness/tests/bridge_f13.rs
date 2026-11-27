//! F.13: the vault takes the mint fee at deposit. Claims on both sides of an imported fee
//! change mint the same denomination and fixed relay fee, with no schedule-dependent output.

use ferrata_bridge_core::inbound::DestKind;
use ferrata_lez_bridge_wire::{code, own_actor, word_of_u128, BridgeMessage};
use harness::{
    bridge::{
        catalog, eth_entry, eth_schedule_one, fee_recipient, registry_slots, relayer, Block, Deposit, Vault,
        VaultSlots, World, CHAIN_ID, DENOMINATIONS, ETH, RELAY_FEES,
    },
    TxReport,
};

const RESERVE: u128 = 100_000_000;

fn ok(report: &TxReport) {
    assert!(report.ok(), "{}: {:?} (exit {:?})", report.label, report.charge.revert, report.exit_code);
}

/// Add a public 1 ETH deposit at leaf 8, under schedule 1 (imported by `World::with_roots`),
/// attest and import the nine-leaf root. Leaf 2's vector deposit is 1 ETH under schedule 0.
fn new_schedule_deposit(w: &mut World) -> Deposit {
    let deposit = Deposit {
        leaf_index: 8,
        depositor: [0xf1; 20],
        token: ETH,
        amount: word_of_u128(DENOMINATIONS[1]),
        s: [0xf2; 32],
        kind: DestKind::Public,
        dest: *w.chain.payer.value(),
    };
    let mut leaves = w.vault_a.leaves.clone();
    assert_eq!(leaves.len(), 8);
    leaves.push(deposit.leaf());
    let vault = Vault { address: w.vault_a.address, leaves };
    let extra = registry_slots(&eth_entry(), &[eth_schedule_one()]);
    let slots = VaultSlots { address: vault.address, root: vault.root(9), count: 9, config_id: w.cid_a, extra };
    let slot = w.next_slot();
    let block = w.add_block(Block::new(slot, 5_000 + slot, &[slots]));
    let root = vault.root(9);
    let address = vault.address;
    let cid = w.cid_a;
    ok(&w.import("ImportRoot A n=9, after the fee change", &cid, block, root, 9, &address));
    deposit
}

#[test]
fn old_and_new_schedule_public_claims_mint_the_same_and_an_unregistered_amount_is_refused() {
    let mut w = World::with_roots(); // includes ImportFees of schedule 1, from leaf 6
    let (cid, vault) = (w.cid_a, w.vault_a.address);
    let new = new_schedule_deposit(&mut w);
    let old_vault = w.vault_a.clone();
    let old = w.vector.clone();
    let new_vault = Vault { address: vault, leaves: w.vault_a.leaves.iter().copied().chain([new.leaf()]).collect() };

    for (label, deposit, at, count, destination) in
        [("old schedule", &old, &old_vault, 5, w.vector_dest), ("new schedule", &new, &new_vault, 9, w.chain.payer)]
    {
        let before = (w.balance(destination, &cid), w.balance(relayer(), &cid), w.balance(fee_recipient(), &cid));
        ok(&w.claim(
            &format!("Claim {label}: denomination + relay fee only"),
            &cid,
            count,
            &vault,
            at.witness(deposit, count),
        ));
        assert_eq!(w.balance(destination, &cid) - before.0, DENOMINATIONS[1], "{label}: denomination");
        assert_eq!(w.balance(relayer(), &cid) - before.1, RELAY_FEES[1], "{label}: fixed relay fee");
        assert_eq!(w.balance(fee_recipient(), &cid), before.2, "{label}: no LEZ mint fee");
    }

    let odd = w.vault_a.witness(&w.odd_deposit.clone(), 8);
    let refused = w.claim("An unregistered amount is still refused", &cid, 8, &vault, odd);
    assert_eq!(refused.exit_code, Some(u32::from(code::NOT_REGISTERED)));
    assert!(!refused.ok());
}

#[test]
fn without_a_relayer_the_fixed_relay_fee_goes_to_the_fee_recipient() {
    let mut w = World::with_roots();
    let (cid, vault) = (w.cid_a, w.vault_a.address);
    let witness = w.vault_a.witness(&w.vector.clone(), 5);
    ok(&w.claim_with("Claim without a relayer", &cid, 5, &vault, witness, None));
    assert_eq!(w.balance(w.vector_dest, &cid), DENOMINATIONS[1]);
    assert_eq!(w.balance(relayer(), &cid), 0);
    assert_eq!(w.balance(fee_recipient(), &cid), RELAY_FEES[1], "relay fee only, no mint fee");
}

#[test]
fn old_and_new_schedule_private_claims_mint_the_same() {
    assert_eq!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1"), "run with RISC0_DEV_MODE=1");
    let mut w = World::with_roots(); // includes ImportFees of schedule 1, from leaf 6
    let (cid, vault) = (w.cid_a, w.vault_a.address);
    let new = new_schedule_deposit(&mut w);
    let old_vault = w.vault_a.clone();
    let old = w.vector.clone();
    let new_vault = Vault { address: vault, leaves: w.vault_a.leaves.iter().copied().chain([new.leaf()]).collect() };

    for (label, deposit, at, count, destination) in
        [("old schedule", &old, &old_vault, 5, w.vector_dest), ("new schedule", &new, &new_vault, 9, w.chain.payer)]
    {
        // Each private proof must start from a fresh account: reusing one would replay its first output commitment.
        let prover = w.chain.add_private_account(0xf0 + count as u8, 1_000_000_000_000);
        let me = own_actor(w.program, prover.id());
        let message = w.claim_privately(&cid, count, at.witness(deposit, count));
        let BridgeMessage::ClaimPrivately { .. } = message else { unreachable!() };
        let actors = w.private_claim_actors(&cid, count, &vault, deposit);
        let before = (w.balance(destination, &cid), w.balance(relayer(), &cid), w.balance(fee_recipient(), &cid));
        let run = w
            .chain
            .prove_and_settle_private(
                &prover,
                me,
                borsh::to_vec(&message).unwrap(),
                actors,
                &catalog(),
                RESERVE,
                w.time,
            )
            .unwrap_or_else(|e| panic!("{label} private claim: {e}"));
        assert!(run.charge.revert.is_none(), "{label}: {:?}", run.charge.revert);
        assert_eq!(w.balance(destination, &cid) - before.0, DENOMINATIONS[1], "{label}: denomination");
        assert_eq!(w.balance(relayer(), &cid) - before.1, RELAY_FEES[1], "{label}: fixed relay fee");
        assert_eq!(w.balance(fee_recipient(), &cid), before.2, "{label}: no LEZ mint fee");
    }

    // The destination remains public in this test; only the claim execution is private.
    assert_eq!(w.nullifier(&cid, &vault, &old.nullifier(CHAIN_ID, &vault)).unwrap().amount, DENOMINATIONS[1]);
    assert_eq!(w.nullifier(&cid, &vault, &new.nullifier(CHAIN_ID, &vault)).unwrap().amount, DENOMINATIONS[1]);
}
