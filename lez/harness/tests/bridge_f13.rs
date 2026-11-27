//! F.13 (ADR 0048): the vault escrows the mint fee of each deposit and the burn fee of each
//! release at one cumulative total per token (`lez_fees_slot`). Claims and burns carry no fee
//! value, so claims on both sides of an imported fee change mint the same denomination and fixed
//! relay fee; `CollectFees` (anyone, at `config(cid)`) attests that total and mints what is not
//! minted yet, at most a limit, to the configuration's fee recipient — one transaction that names
//! no user's claim, deposit or leaf, so no user's transaction is told apart by its fees.

use ferrata_bridge_core::inbound::DestKind;
use ferrata_lez_bridge_wire::{code, config_actor, fees_actor, own_actor, token_actor, word_of_u128, BridgeMessage};
use harness::{
    bridge::{
        catalog, eth_entry, eth_schedule_one, fee_recipient, lez_fees_slots, registry_slots, relayer, Block, Deposit,
        Vault, VaultSlots, World, ADMIN_SEED, CHAIN_ID, DENOMINATIONS, ETH, FLOOR, FREEZE_SEED, MINT_FEES_1,
        RELAY_FEES, T3,
    },
    settle::LeeTransaction,
    TxReport,
};

const RESERVE: u128 = 100_000_000;
const HOUR_MS: u64 = 3_600_000;

fn ok(report: &TxReport) {
    assert!(report.ok(), "{}: {:?} (exit {:?})", report.label, report.charge.revert, report.exit_code);
}

fn refused(report: &TxReport, c: u8) {
    assert_eq!(report.exit_code, Some(u32::from(c)), "{}: {:?}", report.label, report.charge.revert);
    assert!(!report.ok(), "{}", report.label);
}

/// A block holding `vault` (of configuration `cid`, its deposit tree at all its leaves) with its
/// cumulative protocol-fee total of `token` at `total` base units in the vault's `lez_fees_slot`
/// (ADR 0048), recorded at the attestation program as the world's own blocks are. Returns the
/// block's index: the record a `CollectFees` names.
fn fees_block(w: &mut World, cid: [u8; 32], vault: &Vault, token: [u8; 20], total: u128) -> usize {
    let count = vault.leaves.len() as u64;
    let slots = VaultSlots {
        address: vault.address,
        root: vault.root(count),
        count,
        config_id: cid,
        extra: lez_fees_slots(&token, total),
    };
    let slot = w.next_slot();
    w.add_block(Block::new(slot, 5_000 + slot, &[slots]))
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

// ---------------------------------------------------------------------------------------------
// CollectFees (ADR 0048)
// ---------------------------------------------------------------------------------------------

/// A collection mints the attested total to the fee recipient once; the same record again mints
/// nothing (244), a later record mints the growth, and an older record is refused (247).
#[test]
fn a_collection_mints_the_attested_total_once() {
    let mut w = World::with_roots();
    let (cid, vault) = (w.cid_a, w.vault_a.clone());
    let (x, y) = (MINT_FEES_1[1], MINT_FEES_1[2]);
    let first = fees_block(&mut w, cid, &vault, ETH, x);
    ok(&w.collect_fees("CollectFees: the vault's total X", &cid, ETH, first, u128::MAX, &vault.address));
    assert_eq!(w.balance(fee_recipient(), &cid), x, "the fee recipient is credited the total");
    assert_eq!(w.fees_state(&cid, &ETH).unwrap().collected_u128(), x);

    refused(
        &w.collect_fees("CollectFees: the same record again", &cid, ETH, first, u128::MAX, &vault.address),
        code::ALREADY_WRITTEN,
    );
    assert_eq!(w.balance(fee_recipient(), &cid), x, "nothing more is minted");
    assert_eq!(w.fees_state(&cid, &ETH).unwrap().collected_u128(), x);

    let later = fees_block(&mut w, cid, &vault, ETH, x + y);
    ok(&w.collect_fees("CollectFees: a later record, X + Y", &cid, ETH, later, u128::MAX, &vault.address));
    assert_eq!(w.balance(fee_recipient(), &cid), x + y, "only the growth is minted");
    assert_eq!(w.fees_state(&cid, &ETH).unwrap().collected_u128(), x + y);

    refused(
        &w.collect_fees("CollectFees: the older record (X)", &cid, ETH, first, u128::MAX, &vault.address),
        code::IMPORT_MISMATCH,
    );
    assert_eq!(w.balance(fee_recipient(), &cid), x + y, "an older record mints nothing");
    assert_eq!(w.fees_state(&cid, &ETH).unwrap().collected_u128(), x + y);
}

/// A limit below what is due mints only the limit; the next collection mints the rest.
#[test]
fn a_limit_splits_a_collection() {
    let mut w = World::with_roots();
    let (cid, vault) = (w.cid_a, w.vault_a.clone());
    let total = 3 * MINT_FEES_1[1];
    let block = fees_block(&mut w, cid, &vault, ETH, total);
    ok(&w.collect_fees("CollectFees with a limit below the total", &cid, ETH, block, MINT_FEES_1[1], &vault.address));
    assert_eq!(w.balance(fee_recipient(), &cid), MINT_FEES_1[1]);
    assert_eq!(w.fees_state(&cid, &ETH).unwrap().collected_u128(), MINT_FEES_1[1]);
    ok(&w.collect_fees("CollectFees again: the rest", &cid, ETH, block, u128::MAX, &vault.address));
    assert_eq!(w.balance(fee_recipient(), &cid), total);
    assert_eq!(w.fees_state(&cid, &ETH).unwrap().collected_u128(), total);
    refused(
        &w.collect_fees("CollectFees a third time: nothing due", &cid, ETH, block, u128::MAX, &vault.address),
        code::ALREADY_WRITTEN,
    );
}

/// The token's mint freeze (the freeze authority's `Freeze` with `mints: true`) stops a collection
/// (242) with nothing minted or counted; once the freeze ends, the same collection succeeds.
#[test]
fn a_mint_freeze_stops_a_collection_until_it_ends() {
    let mut w = World::with_roots();
    let (cid, vault) = (w.cid_a, w.vault_a.clone());
    let block = fees_block(&mut w, cid, &vault, ETH, MINT_FEES_1[1]);
    let eth_actor = token_actor(w.program, &cid, &ETH);
    let later = w.finalize(w.blocks[2].time_ms() / 1000 + 60);
    let t = w.blocks[later].time_ms();
    let record = w.blocks[later].time_record();
    let freeze = |until_ms: u64| BridgeMessage::Freeze {
        config_id: cid,
        token: Some(ETH),
        mints: true,
        burns: false,
        until_ms,
        record: record.clone(),
    };
    ok(&w.role("Freeze (freeze authority): ETH (A) mints for an hour", FREEZE_SEED, &freeze(t + HOUR_MS), eth_actor));
    refused(
        &w.collect_fees("CollectFees while the token's mints are frozen", &cid, ETH, block, u128::MAX, &vault.address),
        code::FROZEN,
    );
    assert_eq!(w.balance(fee_recipient(), &cid), 0, "nothing is minted under the freeze");
    assert!(w.fees_state(&cid, &ETH).is_none(), "nothing is counted either");
    ok(&w.role("Freeze (freeze authority): unfreeze ETH (A) mints", FREEZE_SEED, &freeze(0), eth_actor));
    ok(&w.collect_fees("CollectFees after the freeze ends", &cid, ETH, block, u128::MAX, &vault.address));
    assert_eq!(w.balance(fee_recipient(), &cid), MINT_FEES_1[1]);
    assert_eq!(w.fees_state(&cid, &ETH).unwrap().collected_u128(), MINT_FEES_1[1]);
}

/// A token mint bucket below what is due refuses the collection (255) with nothing minted or
/// counted; a limit within the bucket succeeds, and the rest waits for the bucket to refill.
#[test]
fn a_token_bucket_below_what_is_due_refuses_and_a_limit_within_it_succeeds() {
    let mut w = World::with_roots();
    let (cid, vault) = (w.cid_a, w.vault_a.clone());
    let total = 400 * MINT_FEES_1[1]; // 2 ETH of escrowed fees
    let block = fees_block(&mut w, cid, &vault, ETH, total);
    let eth_actor = token_actor(w.program, &cid, &ETH);
    let set_cap = BridgeMessage::SetCap {
        config_id: cid,
        token: Some(ETH),
        mints: true,
        cap: FLOOR, // 1 ETH, the immutable floor
    };
    ok(&w.role("SetCap (admin): ETH (A) mints at the floor (1 ETH)", ADMIN_SEED, &set_cap, eth_actor));
    refused(
        &w.collect_fees("CollectFees above the token's mint bucket", &cid, ETH, block, u128::MAX, &vault.address),
        code::CAPPED,
    );
    assert_eq!(w.balance(fee_recipient(), &cid), 0, "nothing is minted above the bucket");
    assert!(w.fees_state(&cid, &ETH).is_none(), "nothing is counted either");
    ok(&w.collect_fees(
        "CollectFees with a limit within the bucket",
        &cid,
        ETH,
        block,
        DENOMINATIONS[1], // 1 ETH of fees
        &vault.address,
    ));
    assert_eq!(w.balance(fee_recipient(), &cid), DENOMINATIONS[1]);
    assert_eq!(w.fees_state(&cid, &ETH).unwrap().collected_u128(), DENOMINATIONS[1]);
    refused(
        &w.collect_fees("CollectFees of the rest: the bucket is empty", &cid, ETH, block, u128::MAX, &vault.address),
        code::CAPPED,
    );
    assert_eq!(w.balance(fee_recipient(), &cid), DENOMINATIONS[1]);
    assert_eq!(w.fees_state(&cid, &ETH).unwrap().collected_u128(), DENOMINATIONS[1]);
}

/// A total that is not a multiple of the LEZ unit is refused at `MintFees` (240) and the whole
/// transaction is undone; a limit of zero is refused outright (240).
#[test]
fn a_total_that_is_not_whole_lez_units_refuses_and_undoes_the_transaction() {
    let mut w = World::with_roots();
    let (cid, vault) = (w.cid_a, w.vault_a.clone());
    let odd = fees_block(&mut w, cid, &vault, ETH, MINT_FEES_1[1] + 1); // one base unit past a whole gwei
    refused(
        &w.collect_fees(
            "CollectFees of a total one base unit past whole LEZ units",
            &cid,
            ETH,
            odd,
            u128::MAX,
            &vault.address,
        ),
        code::BAD_MESSAGE,
    );
    assert_eq!(w.balance(fee_recipient(), &cid), 0, "nothing is minted");
    assert!(w.fees_state(&cid, &ETH).is_none(), "the count is undone with the whole transaction");
    let block = fees_block(&mut w, cid, &vault, ETH, MINT_FEES_1[1]);
    refused(&w.collect_fees("CollectFees with a zero limit", &cid, ETH, block, 0, &vault.address), code::BAD_MESSAGE);
    assert!(w.fees_state(&cid, &ETH).is_none());
    assert_eq!(w.balance(fee_recipient(), &cid), 0);
}

/// A token the configuration never registered is refused (251) with nothing written.
#[test]
fn a_token_not_registered_under_the_configuration_refuses() {
    let mut w = World::with_roots();
    let (cid, vault) = (w.cid_a, w.vault_a.clone());
    // T3 sits in vault A's registry storage (the world's blocks carry it) but was never registered
    // under cid_a on LEZ.
    let block = fees_block(&mut w, cid, &vault, T3, MINT_FEES_1[1]);
    refused(
        &w.collect_fees(
            "CollectFees of T3, in the vault's storage but not registered under A",
            &cid,
            T3,
            block,
            u128::MAX,
            &vault.address,
        ),
        code::NOT_REGISTERED,
    );
    assert!(w.fees_state(&cid, &T3).is_none(), "no fees account is written");
    assert!(w.fees_state(&cid, &ETH).is_none());
    assert_eq!(w.balance(fee_recipient(), &cid), 0);
}

/// Nobody but the program's own actors can send `AccrueFees` or `MintFees`: each sent by a user as
/// a top-level call is refused (243) and changes nothing.
#[test]
fn nobody_but_the_program_can_accrue_or_mint_fees() {
    let mut w = World::with_roots();
    let (cid, vault) = (w.cid_a, w.vault_a.clone());
    let block = fees_block(&mut w, cid, &vault, ETH, MINT_FEES_1[1]);
    ok(&w.collect_fees("CollectFees: the vault's total", &cid, ETH, block, u128::MAX, &vault.address));
    let fees = fees_actor(w.program, &cid, &ETH);
    let eth_actor = token_actor(w.program, &cid, &ETH);
    let accrue = BridgeMessage::AccrueFees { config_id: cid, token: ETH, total: word_of_u128(2 * MINT_FEES_1[1]), limit: u128::MAX };
    refused(
        &w.send("AccrueFees as a user's top-level call at fees(A, ETH)", fees, &accrue, vec![fees]),
        code::WRONG_SENDER,
    );
    let mint = BridgeMessage::MintFees { config_id: cid, token: ETH, amount: MINT_FEES_1[1] };
    refused(
        &w.send("MintFees as a user's top-level call at token(A, ETH)", eth_actor, &mint, vec![eth_actor]),
        code::WRONG_SENDER,
    );
    assert_eq!(w.balance(fee_recipient(), &cid), MINT_FEES_1[1], "nothing changed");
    assert_eq!(w.fees_state(&cid, &ETH).unwrap().collected_u128(), MINT_FEES_1[1]);
}

/// F.16: a collection under configuration B leaves configuration A's fees and fee recipient
/// unchanged, and the other way round (the fee recipient is the same account in both).
#[test]
fn a_collection_stays_within_its_configuration() {
    let mut w = World::with_roots();
    let (cid_a, cid_b) = (w.cid_a, w.cid_b);
    let (vault_a, vault_b) = (w.vault_a.clone(), w.vault_b.clone());
    let block_b = fees_block(&mut w, cid_b, &vault_b, ETH, MINT_FEES_1[1]);
    ok(&w.collect_fees("CollectFees under B", &cid_b, ETH, block_b, u128::MAX, &vault_b.address));
    assert_eq!(w.balance(fee_recipient(), &cid_b), MINT_FEES_1[1]);
    assert_eq!(w.fees_state(&cid_b, &ETH).unwrap().collected_u128(), MINT_FEES_1[1]);
    assert_eq!(w.balance(fee_recipient(), &cid_a), 0, "A's fee recipient is untouched");
    assert!(w.fees_state(&cid_a, &ETH).is_none(), "A's fees account is untouched");

    let block_a = fees_block(&mut w, cid_a, &vault_a, ETH, 2 * MINT_FEES_1[1]);
    ok(&w.collect_fees("CollectFees under A", &cid_a, ETH, block_a, u128::MAX, &vault_a.address));
    assert_eq!(w.balance(fee_recipient(), &cid_a), 2 * MINT_FEES_1[1]);
    assert_eq!(w.fees_state(&cid_a, &ETH).unwrap().collected_u128(), 2 * MINT_FEES_1[1]);
    assert_eq!(w.balance(fee_recipient(), &cid_b), MINT_FEES_1[1], "B's fee recipient is untouched");
    assert_eq!(w.fees_state(&cid_b, &ETH).unwrap().collected_u128(), MINT_FEES_1[1]);
}

/// The F.13 privacy point end to end: two claims of 1 ETH on both sides of a fee change mint the
/// same amounts; then one collection of the vault's total — the mint fee of the deposit made under
/// schedule 1 — credits the fee recipient exactly that, in a transaction whose message carries
/// only `config_id`, `token`, `record_slot`, `limit` and the vault proof: no claim, deposit or leaf.
#[test]
fn claims_stay_uniform_and_one_collection_names_no_user() {
    let mut w = World::with_roots(); // includes ImportFees of schedule 1, from leaf 6
    let (cid, vault) = (w.cid_a, w.vault_a.clone());
    let new = new_schedule_deposit(&mut w);
    let old_vault = w.vault_a.clone();
    let old = w.vector.clone();
    let new_vault =
        Vault { address: vault.address, leaves: w.vault_a.leaves.iter().copied().chain([new.leaf()]).collect() };

    for (label, deposit, at, count, destination) in
        [("old schedule", &old, &old_vault, 5, w.vector_dest), ("new schedule", &new, &new_vault, 9, w.chain.payer)]
    {
        let before = (w.balance(destination, &cid), w.balance(relayer(), &cid), w.balance(fee_recipient(), &cid));
        ok(&w.claim(
            &format!("Claim {label}: denomination + relay fee only"),
            &cid,
            count,
            &vault.address,
            at.witness(deposit, count),
        ));
        assert_eq!(w.balance(destination, &cid) - before.0, DENOMINATIONS[1], "{label}: denomination");
        assert_eq!(w.balance(relayer(), &cid) - before.1, RELAY_FEES[1], "{label}: fixed relay fee");
        assert_eq!(w.balance(fee_recipient(), &cid), before.2, "{label}: no LEZ mint fee at the claim");
    }

    // The vault's cumulative total holds the mint fee of the deposit made under schedule 1 (leaf 8,
    // 1 ETH) and nothing of schedule 0's: one collection credits exactly that.
    let block = fees_block(&mut w, cid, &vault, ETH, MINT_FEES_1[1]);
    ok(&w.collect_fees("CollectFees: the vault's escrowed mint fees", &cid, ETH, block, u128::MAX, &vault.address));
    assert_eq!(w.balance(fee_recipient(), &cid), MINT_FEES_1[1], "the schedule 1 mint fee, and nothing else");
    assert_eq!(w.fees_state(&cid, &ETH).unwrap().collected_u128(), MINT_FEES_1[1]);

    // The collection's transaction names no claim, deposit or leaf: its root message is a
    // CollectFees, and nothing but its five fields decodes from its bytes.
    let (_, bytes) = w.txs.last().expect("the collection's transaction");
    let tx: LeeTransaction = borsh::from_slice(bytes).unwrap();
    let LeeTransaction::Public(public) = tx else { unreachable!("a public collection") };
    let message = borsh::from_slice::<BridgeMessage>(&public.message.execution.root.message).unwrap();
    let BridgeMessage::CollectFees { config_id, token, record_slot, limit, proof } = message else {
        panic!("the collection's root message names a claim, a deposit or a leaf");
    };
    assert_eq!((config_id, token, record_slot), (cid, ETH, w.blocks[block].slot));
    assert_eq!(limit, u128::MAX);
    let proof_len = borsh::to_vec(&proof).unwrap().len();
    assert_eq!(
        borsh::to_vec(&BridgeMessage::CollectFees { config_id, token, record_slot, limit, proof }).unwrap().len(),
        1 + 32 + 20 + 8 + 16 + proof_len,
        "the message carries only config_id, token, record_slot, limit and the proof"
    );
}

/// F.12, as for `ImportRoot`: under a record less than the admin's inbound depth behind the newest record the
/// configuration's clock has read, a collection waits (242) and writes nothing; once the clock has read a record deep
/// enough past it, the same collection mints.
#[test]
fn a_collection_waits_for_the_admins_inbound_depth() {
    let mut w = World::with_roots();
    let (cid, vault) = (w.cid_a, w.vault_a.clone());
    let block = fees_block(&mut w, cid, &vault, ETH, MINT_FEES_1[1]);
    let at = config_actor(w.program, &cid);
    ok(&w.role("SetDepth 64 (admin)", ADMIN_SEED, &BridgeMessage::SetDepth { config_id: cid, slots: 64 }, at));
    refused(
        &w.collect_fees("CollectFees under a record not 64 slots behind the clock's", &cid, ETH, block, u128::MAX, &vault.address),
        code::FROZEN,
    );
    assert!(w.fees_state(&cid, &ETH).is_none(), "a held collection writes nothing");
    assert_eq!(w.balance(fee_recipient(), &cid), 0);
    // Two records past the fees block (32 slots each), the second read by the configuration's clock.
    let t = w.newest_record_seconds();
    w.finalize(t + 32 * 12);
    let deep = w.finalize(t + 64 * 12);
    assert!(w.blocks[deep].slot >= w.blocks[block].slot + 64);
    ok(&w.tick("Tick A: a record 64 slots past the fees block", &cid, None, deep));
    ok(&w.collect_fees("CollectFees under the same record, now deep enough", &cid, ETH, block, u128::MAX, &vault.address));
    assert_eq!(w.balance(fee_recipient(), &cid), MINT_FEES_1[1]);
    assert_eq!(w.fees_state(&cid, &ETH).unwrap().collected_u128(), MINT_FEES_1[1]);
}

/// A collection imports no root, so `Sunset` does not stop it (ADR 0048): the fees the vault escrowed are still minted
/// to the fee recipient on LEZ, while an import under the same record is refused (242).
#[test]
fn a_collection_still_mints_after_sunset() {
    let mut w = World::with_roots();
    let (cid, vault) = (w.cid_a, w.vault_a.clone());
    let block = fees_block(&mut w, cid, &vault, ETH, MINT_FEES_1[1]);
    let at = config_actor(w.program, &cid);
    ok(&w.role("Sunset A (freeze authority)", FREEZE_SEED, &BridgeMessage::Sunset { config_id: cid }, at));
    let n = vault.leaves.len() as u64;
    refused(&w.import("ImportRoot A after Sunset", &cid, block, vault.root(n), n, &vault.address), code::FROZEN);
    ok(&w.collect_fees("CollectFees A after Sunset", &cid, ETH, block, u128::MAX, &vault.address));
    assert_eq!(w.balance(fee_recipient(), &cid), MINT_FEES_1[1]);
    assert_eq!(w.fees_state(&cid, &ETH).unwrap().collected_u128(), MINT_FEES_1[1]);
}

/// The configuration's mint freeze (a `Freeze` with no token) stops a collection too: `MintFees` has `config(cid)` apply
/// the global freezes and buckets (`Admit`), as a claim's `Issue` does; nothing is minted or counted until it ends.
#[test]
fn the_configurations_mint_freeze_stops_a_collection() {
    let mut w = World::with_roots();
    let (cid, vault) = (w.cid_a, w.vault_a.clone());
    let block = fees_block(&mut w, cid, &vault, ETH, MINT_FEES_1[1]);
    let at = config_actor(w.program, &cid);
    let later = w.finalize(w.blocks[2].time_ms() / 1000 + 60);
    let t = w.blocks[later].time_ms();
    let record = w.blocks[later].time_record();
    let freeze = |until_ms: u64| BridgeMessage::Freeze {
        config_id: cid,
        token: None,
        mints: true,
        burns: false,
        until_ms,
        record: record.clone(),
    };
    ok(&w.role("Freeze (freeze authority): A's mints for an hour", FREEZE_SEED, &freeze(t + HOUR_MS), at));
    refused(
        &w.collect_fees("CollectFees while the configuration's mints are frozen", &cid, ETH, block, u128::MAX, &vault.address),
        code::FROZEN,
    );
    assert!(w.fees_state(&cid, &ETH).is_none(), "nothing is counted under the global freeze");
    assert_eq!(w.balance(fee_recipient(), &cid), 0, "nothing is minted under the global freeze");
    ok(&w.role("Freeze (freeze authority): unfreeze A's mints", FREEZE_SEED, &freeze(0), at));
    ok(&w.collect_fees("CollectFees after the global freeze ends", &cid, ETH, block, u128::MAX, &vault.address));
    assert_eq!(w.balance(fee_recipient(), &cid), MINT_FEES_1[1]);
}
