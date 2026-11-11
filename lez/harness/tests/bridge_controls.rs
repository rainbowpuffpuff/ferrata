//! The registry (F.8, F.10), fees by leaf index (F.13), caps (F.11), freezes and roles (F.14) and
//! `Sunset` (retirement) of the bridge program, through LEZ's state machine on the synthetic vaults
//! of `harness::bridge`, every refusal with its code; then what the checkpoint program collects
//! of a `Sunset` (`checkpoint-core`'s own parser and decoder). Writes `results/bridge-controls.md`.
//!
//! Caps and freezes run on attested Ethereum time: time moves here only when a later block's
//! record exists (`World::finalize`) and a `Tick` or `Freeze` has read it. What a block's producer
//! can and cannot do to that clock is `bridge_time.rs`'s.

use std::{fs, path::PathBuf};

use checkpoint_core::{
    lez::{parse_block, Actor as CpActor, Origin, TxKind, Visitor},
    wire::BridgeMessage as CpMessage,
};
use ferrata_lez_bridge_wire::{
    self as bw, code, config_actor, holder_actor, token_actor, BridgeMessage, CapParams, ConfigState, FeeSchedule,
    Note, ScheduleRange, TokenEntry, MAX_FREEZE_MS,
};
use harness::{
    bridge::{
        eth_entry, eth_schedule_one, fee_recipient, relayer, render, schedule_zero, t2_entry, t3_entry, World,
        ADMIN_SEED, DENOMINATIONS, ETH, FLOOR, FREEZE_SEED, GLOBAL_CAP, LEZ_UNIT, MINT_FEES_1, OUTSIDER_SEED,
        RELAY_FEES, T2, T2_DENOMINATIONS, T3, VAULT_B,
    },
    render_transitions, TxReport,
};

const TARGET_GAS_EXEC: u64 = 5_000_000;
const HOUR_MS: u64 = 3_600_000;

fn results_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../results").join(name)
}

fn run(reports: &mut Vec<TxReport>, report: TxReport, code: Option<u8>) {
    expect(&report, code);
    reports.push(report);
}

fn expect(report: &TxReport, code: Option<u8>) {
    match code {
        None => assert!(report.ok(), "{}: {:?} (exit {:?})", report.label, report.charge.revert, report.exit_code),
        Some(c) => {
            assert_eq!(report.exit_code, Some(u32::from(c)), "{}: {:?}", report.label, report.charge.revert);
            assert!(!report.ok(), "{}", report.label);
        }
    }
    assert!(report.charge.gas_exec <= TARGET_GAS_EXEC, "{} is over TARGET_GAS_EXEC", report.label);
}

/// A burn note with tag `i` (the program checks the token and the amount, not the wallet's
/// derivations, which `bridge_burn.rs` covers).
fn note(i: u8, token: [u8; 20], amount: u128) -> Note {
    Note { r: [i; 32], memo: [i; 20], mac: [i; 16], token, amount: bw::word_of_u128(amount), c_out: [i; 32] }
}

/// The checkpoint's view: every call to the bridge program a block shows in the clear.
struct Calls<'a> {
    bridge: [u8; 32],
    calls: Vec<(u32, Origin, &'a [u8])>,
}

impl<'a> Visitor<'a> for Calls<'a> {
    fn tx(&mut self, _index: u32, _kind: TxKind, _bytes: &'a [u8]) {}
    fn call(&mut self, index: u32, origin: Origin, _from: Option<CpActor<'a>>, to: CpActor<'a>, message: &'a [u8]) {
        if *to.program_account_id == self.bridge {
            self.calls.push((index, origin, message));
        }
    }
}

/// A LEZ block (`borsh(Block)`) holding `txs`, `reverted` the positions of reverted ones.
fn block(txs: &[&[u8]], reverted: &[u64]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&7u64.to_le_bytes());
    b.extend_from_slice(&[0u8; 32]);
    b.extend_from_slice(&[0u8; 32]);
    b.extend_from_slice(&1_700_000_000_000u64.to_le_bytes());
    b.extend_from_slice(&(txs.len() as u32).to_le_bytes());
    for t in txs {
        b.extend_from_slice(t);
    }
    b.extend_from_slice(&(reverted.len() as u32).to_le_bytes());
    for r in reverted {
        b.extend_from_slice(&r.to_le_bytes());
    }
    b.push(2);
    b
}

#[test]
fn registry_fees_caps_freezes_and_sunset() {
    let mut w = World::new();
    let (a, b) = (w.config_a.clone(), w.config_b.clone());
    let (cid_a, cid_b, cid_a2) = (w.cid_a, w.cid_b, w.cid_a2);
    let va = w.vault_a.address;
    let program = w.program;
    let payer = w.chain.payer;
    let mut reports = Vec::new();
    for (r, c) in [(w.init("Init A", &a, 0), None), (w.init("Init B", &b, 0), None)] {
        run(&mut reports, r, c);
    }
    // The configurations' clocks read the newest record (block 102's); every token registered
    // below starts at its configuration's attested time.
    let t0 = w.blocks[2].time_ms();
    for (label, cid) in [("Tick A: block 102's record", cid_a), ("Tick B: block 102's record", cid_b)] {
        let r = w.tick(label, &cid, None, 2);
        run(&mut reports, r, None);
    }
    assert!(matches!(w.config_state(&cid_a), Some(ConfigState::Ready { gate, .. }) if gate.now_ms == t0));

    // --- Registry (F.8, F.10) -------------------------------------------------------------------
    let r = w.register("Register ETH (A): entry, schedules 0 and 1, count at block 100", &cid_a, 0, &eth_entry(), &va);
    run(&mut reports, r, None);
    let eth_a = w.token_state(&cid_a, &ETH).expect("token(A, ETH)");
    assert_eq!(eth_a.gate.now_ms, t0, "a token starts at its configuration's attested time");
    assert_eq!(
        (eth_a.entry, eth_a.schedules.len(), eth_a.known_until),
        (eth_entry(), 1, 5),
        "schedule 1 absent: covers 5"
    );
    let r = w.register("Register ETH (A) again", &cid_a, 1, &eth_entry(), &va);
    run(&mut reports, r, Some(code::ALREADY_WRITTEN));
    let r = w.register("Register T2 (A): vault A holds no entry for it", &cid_a, 0, &t2_entry(), &va);
    run(&mut reports, r, Some(code::IMPORT_MISMATCH));
    let r = w.register("Register ETH (A'): configuration never initialised", &cid_a2, 0, &eth_entry(), &va);
    run(&mut reports, r, Some(code::NOT_FOUND));
    let altered = TokenEntry { relay_fees: RELAY_FEES.iter().map(|f| f / 2).collect(), ..eth_entry() };
    let r = w.register("Register ETH (B) with other relay fees than the vault's", &cid_b, 0, &altered, &VAULT_B);
    run(&mut reports, r, Some(code::IMPORT_MISMATCH));
    let nine = TokenEntry {
        token: [0x99; 20],
        denominations: (1..=9).map(|i| i * DENOMINATIONS[0]).collect(),
        relay_fees: vec![0; 9],
        ..eth_entry()
    };
    let not_multiple =
        TokenEntry { denominations: vec![DENOMINATIONS[0] + 1, DENOMINATIONS[1], DENOMINATIONS[2]], ..eth_entry() };
    let fee_too_high =
        TokenEntry { relay_fees: vec![DENOMINATIONS[0] / 50, RELAY_FEES[1], RELAY_FEES[2]], ..eth_entry() };
    let unit_not_ten = TokenEntry { lez_unit: 2 * LEZ_UNIT, ..eth_entry() };
    for (label, entry, schedule) in [
        ("Register: nine denominations", &nine, schedule_zero(&nine)),
        ("Register: a denomination not a multiple of the LEZ unit", &not_multiple, schedule_zero(&not_multiple)),
        ("Register: a relay fee of 2%", &fee_too_high, schedule_zero(&fee_too_high)),
        ("Register: a LEZ unit that is not a power of ten", &unit_not_ten, schedule_zero(&unit_not_ten)),
        (
            "Register: schedule 0 with a mint fee",
            &eth_entry(),
            FeeSchedule { mint_fees: RELAY_FEES.to_vec(), ..schedule_zero(&eth_entry()) },
        ),
    ] {
        let r = w.register_with(label, &cid_b, 0, entry, &schedule, &VAULT_B);
        run(&mut reports, r, Some(code::BAD_MESSAGE));
    }
    let r = w.register("Register ETH (B)", &cid_b, 0, &eth_entry(), &VAULT_B);
    run(&mut reports, r, None);
    let r = w.register("Register T2 (B): a second token of the same configuration", &cid_b, 0, &t2_entry(), &VAULT_B);
    run(&mut reports, r, None);
    assert_eq!(w.token_state(&cid_b, &T2).unwrap().entry.denominations, T2_DENOMINATIONS.to_vec());
    let r =
        w.register("Register T3 (A) at block 101: schedule 1 present, covers nothing yet", &cid_a, 1, &t3_entry(), &va);
    run(&mut reports, r, None);
    assert_eq!(w.token_state(&cid_a, &T3).unwrap().known_until, 0);

    // --- Roots ----------------------------------------------------------------------------------
    for (label, cid, block, root, n, vault) in [
        ("ImportRoot A n=5", cid_a, 0, w.vault_a.root(5), 5, va),
        ("ImportRoot A n=8", cid_a, 1, w.vault_a.root(8), 8, va),
        ("ImportRoot B n=8", cid_b, 0, w.vault_b.root(8), 8, VAULT_B),
    ] {
        let r = w.import(label, &cid, block, root, n, &vault);
        run(&mut reports, r, None);
    }

    // --- Fees by leaf index (F.13) --------------------------------------------------------------
    let leaf6 = w.vault_a.witness(&w.payer_deposit.clone(), 8);
    let vector = w.vault_a.witness(&w.vector.clone(), 5);
    let me = Some(*relayer().value());
    let range = |index, from_leaf, until_leaf| ScheduleRange { index, from_leaf, until_leaf };
    let r = w.claim_with(
        "Claim leaf 6 before its schedule is known: range [0, 8)",
        &cid_a,
        8,
        &va,
        leaf6.clone(),
        range(0, 0, 8),
        me,
    );
    run(&mut reports, r, Some(code::LEAF_NOT_IMPORTED));
    let r = w.import_fees(
        "ImportFees ETH (A) schedule 1 at block 100 (not there yet)",
        &cid_a,
        0,
        ETH,
        1,
        Some(eth_schedule_one()),
        &va,
    );
    run(&mut reports, r, Some(code::IMPORT_MISMATCH));
    let r = w.import_fees("ImportFees ETH (A): schedule 2 before 1", &cid_a, 1, ETH, 2, None, &va);
    run(&mut reports, r, Some(code::IMPORT_MISMATCH));
    let r =
        w.import_fees("ImportFees ETH (A) schedule 1 at block 101", &cid_a, 1, ETH, 1, Some(eth_schedule_one()), &va);
    run(&mut reports, r, None);
    assert_eq!(w.token_state(&cid_a, &ETH).unwrap().known_until, 6, "below schedule 1's first leaf");
    let r = w.import_fees("ImportFees ETH (A) schedule 1 again", &cid_a, 1, ETH, 1, Some(eth_schedule_one()), &va);
    run(&mut reports, r, Some(code::IMPORT_MISMATCH));
    let r = w.claim_with(
        "Claim leaf 6 in schedule 1's range [6, 8) before it is closed",
        &cid_a,
        8,
        &va,
        leaf6.clone(),
        range(1, 6, 8),
        me,
    );
    run(&mut reports, r, Some(code::LEAF_NOT_IMPORTED));
    let r = w.import_fees("ImportFees ETH (A): no schedule 2 at block 101 (8 leaves)", &cid_a, 1, ETH, 2, None, &va);
    run(&mut reports, r, None);
    assert_eq!(w.token_state(&cid_a, &ETH).unwrap().known_until, 8);
    let [notice, high] = harness::bridge::t3_bad_schedules();
    let r = w.import_fees("ImportFees T3 (A) schedule 1: one day of notice", &cid_a, 1, T3, 1, Some(notice), &va);
    run(&mut reports, r, Some(code::BAD_MESSAGE));
    let r = w.import_fees("ImportFees T3 (A) schedule 1: a 2% mint fee", &cid_a, 2, T3, 1, Some(high), &va);
    run(&mut reports, r, Some(code::BAD_MESSAGE));
    let r =
        w.claim_with("Claim leaf 6 under schedule 0's range [0, 6)", &cid_a, 8, &va, leaf6.clone(), range(0, 0, 6), me);
    run(&mut reports, r, Some(code::LEAF_NOT_IMPORTED));
    let r = w.claim_with(
        "Claim leaf 2 under schedule 1's range [6, 8)",
        &cid_a,
        5,
        &va,
        vector.clone(),
        range(1, 6, 8),
        me,
    );
    run(&mut reports, r, Some(code::LEAF_NOT_IMPORTED));

    // --- Caps (F.11): token and global, mints and burns, on attested time ----------------------
    let eth_a_actor = token_actor(program, &cid_a, &ETH);
    let set_cap = |cid, token, mints, cap: CapParams| BridgeMessage::SetCap { config_id: cid, token, mints, cap };
    let eleven = CapParams { capacity: 11_000_000_000, refill_per_second: 10_000_000 };
    let r = w.role(
        "SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s",
        ADMIN_SEED,
        &set_cap(cid_a, Some(ETH), true, eleven),
        eth_a_actor,
    );
    run(&mut reports, r, None);
    let below = CapParams { capacity: FLOOR.capacity - 1, ..FLOOR };
    let r = w.role("SetCap (admin) below the floor", ADMIN_SEED, &set_cap(cid_a, Some(ETH), true, below), eth_a_actor);
    run(&mut reports, r, Some(code::BAD_MESSAGE));
    let r =
        w.role("SetCap by the freeze authority", FREEZE_SEED, &set_cap(cid_a, Some(ETH), true, eleven), eth_a_actor);
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    let r =
        w.role("SetCap by a signer with no role", OUTSIDER_SEED, &set_cap(cid_a, Some(ETH), true, eleven), eth_a_actor);
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    let r = w.send(
        "SetCap sent to the token's account itself",
        eth_a_actor,
        &set_cap(cid_a, Some(ETH), true, eleven),
        vec![eth_a_actor],
    );
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    let r = w.claim(
        "Claim leaf 6: 10 ETH to the payer, 0.05 mint fee, 0.01 relay fee (10.06 ETH of the cap)",
        &cid_a,
        8,
        &va,
        leaf6,
    );
    run(&mut reports, r, None);
    assert_eq!(w.balance(payer, &cid_a), DENOMINATIONS[2], "the user gets exactly the denomination");
    assert_eq!(w.balance(fee_recipient(), &cid_a), MINT_FEES_1[2]);
    assert_eq!(w.balance(relayer(), &cid_a), RELAY_FEES[2]);
    let r = w.claim("Claim leaf 2 (1.001 ETH) with 0.94 ETH left in the bucket", &cid_a, 5, &va, vector.clone());
    run(&mut reports, r, Some(code::CAPPED));
    assert!(w.nullifier(&va, &w.vector_n).is_none(), "a capped claim leaves its deposit claimable");
    let ten_s = w.finalize(t0 / 1000 + 10);
    let r =
        w.claim("Claim leaf 2 once a record 10 s later exists, no Tick: not refilled", &cid_a, 5, &va, vector.clone());
    run(&mut reports, r, Some(code::CAPPED));
    let r = w.tick("Tick ETH (A): the record 10 s later", &cid_a, Some(ETH), ten_s);
    run(&mut reports, r, None);
    let r = w.claim("Claim leaf 2 after the Tick (0.1 ETH refilled)", &cid_a, 5, &va, vector);
    run(&mut reports, r, None);
    assert_eq!(w.balance(w.vector_dest, &cid_a), DENOMINATIONS[1]);
    let a_actor = config_actor(program, &cid_a);
    let b_actor = config_actor(program, &cid_b);
    let r = w.role(
        "SetCap (admin): global mints of B at the floor (1 ETH)",
        ADMIN_SEED,
        &set_cap(cid_b, None, true, FLOOR),
        b_actor,
    );
    run(&mut reports, r, None);
    let deposit_b = w.vault_b.witness(&w.deposit_b.clone(), 8);
    let r = w.claim(
        "Claim B's 1 ETH (1.001 ETH with its relay fee) under the global cap",
        &cid_b,
        8,
        &VAULT_B,
        deposit_b.clone(),
    );
    run(&mut reports, r, Some(code::CAPPED));
    let t2 = w.vault_b.witness(&w.deposit_t2.clone(), 8);
    let r = w.claim("Claim B's 1,000 T2 (1 ETH of value units) under the same global cap", &cid_b, 8, &VAULT_B, t2);
    run(&mut reports, r, None);
    assert_eq!(w.balance_of(payer, &cid_b, &T2), T2_DENOMINATIONS[1]);
    let r = w.role(
        "SetCap (admin): global mints of B back to 200 ETH",
        ADMIN_SEED,
        &set_cap(cid_b, None, true, GLOBAL_CAP),
        b_actor,
    );
    run(&mut reports, r, None);

    // Burns: the token's attested clock refills its bucket, as for claims.
    let holder = holder_actor(program, payer);
    let burn = |w: &mut World, label: &str, cid: [u8; 32], note: &Note| -> TxReport {
        let actors = w.burn_actors(holder, &cid, note);
        w.send(label, holder, &BridgeMessage::burn(cid, note), actors)
    };
    let one_eth = CapParams { capacity: DENOMINATIONS[1] / LEZ_UNIT, refill_per_second: 10_000_000 };
    let r = w.role(
        "SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s",
        ADMIN_SEED,
        &set_cap(cid_a, Some(ETH), false, one_eth),
        eth_a_actor,
    );
    run(&mut reports, r, None);
    let r = burn(&mut w, "Burn 1 ETH (A)", cid_a, &note(1, ETH, DENOMINATIONS[1]));
    run(&mut reports, r, None);
    let r = burn(&mut w, "Burn 1 ETH (A) again: the burn bucket is empty", cid_a, &note(2, ETH, DENOMINATIONS[1]));
    run(&mut reports, r, Some(code::CAPPED));
    let later = w.finalize(t0 / 1000 + 110);
    let r = burn(
        &mut w,
        "Burn 1 ETH (A) once a record 100 s later exists, no Tick: the token's clock has not read it",
        cid_a,
        &note(2, ETH, DENOMINATIONS[1]),
    );
    run(&mut reports, r, Some(code::CAPPED));
    let r = w.tick("Tick ETH (A): the record 100 s later", &cid_a, Some(ETH), later);
    run(&mut reports, r, None);
    let r = burn(&mut w, "Burn 1 ETH (A) after the Tick", cid_a, &note(2, ETH, DENOMINATIONS[1]));
    run(&mut reports, r, None);

    // --- Freezes and roles (F.14) ---------------------------------------------------------------
    let newest = w.finalize(t0 / 1000 + 210);
    let r = w.tick("Tick ETH (A): a record 100 s later again", &cid_a, Some(ETH), newest);
    run(&mut reports, r, None);
    let r = w.role(
        "SetCap (admin): ETH (A) burns back to 100 ETH (refilled to 1 ETH first)",
        ADMIN_SEED,
        &set_cap(cid_a, Some(ETH), false, harness::bridge::TOKEN_CAP),
        eth_a_actor,
    );
    run(&mut reports, r, None);
    assert_eq!(w.token_state(&cid_a, &ETH).unwrap().gate.burn.level, DENOMINATIONS[1] / LEZ_UNIT);
    // Every freeze names the newest record; its bound is against the attested time it gives.
    let t = w.blocks[newest].time_ms();
    let record = w.blocks[newest].time_record();
    let freeze = |cid, token, mints, burns, until_ms| BridgeMessage::Freeze {
        config_id: cid,
        token,
        mints,
        burns,
        until_ms,
        record: record.clone(),
    };
    let eth_b_actor = token_actor(program, &cid_b, &ETH);
    let r = w.role(
        "Freeze (freeze authority): ETH (B) mints for one hour",
        FREEZE_SEED,
        &freeze(cid_b, Some(ETH), true, false, t + HOUR_MS),
        eth_b_actor,
    );
    run(&mut reports, r, None);
    assert_eq!(w.token_state(&cid_b, &ETH).unwrap().gate.now_ms, t, "the freeze read the record");
    let r = w.claim("Claim B's 1 ETH while its mints are frozen", &cid_b, 8, &VAULT_B, deposit_b.clone());
    run(&mut reports, r, Some(code::FROZEN));
    let r = w.role(
        "Freeze by the admin: unfreeze ETH (B)",
        ADMIN_SEED,
        &freeze(cid_b, Some(ETH), true, false, 0),
        eth_b_actor,
    );
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    let r = w.role(
        "Freeze for 30 days and 1 ms after the record's time",
        FREEZE_SEED,
        &freeze(cid_b, Some(ETH), true, false, t + MAX_FREEZE_MS + 1),
        eth_b_actor,
    );
    run(&mut reports, r, Some(code::BAD_MESSAGE));
    let r = w.role(
        "Freeze until the record's time itself (already past)",
        FREEZE_SEED,
        &freeze(cid_b, Some(ETH), true, false, t),
        eth_b_actor,
    );
    run(&mut reports, r, Some(code::BAD_MESSAGE));
    let r = w.role(
        "Freeze (freeze authority): unfreeze ETH (B)",
        FREEZE_SEED,
        &freeze(cid_b, Some(ETH), true, false, 0),
        eth_b_actor,
    );
    run(&mut reports, r, None);
    let r = w.role(
        "Freeze (freeze authority): all mints of B for 30 days",
        FREEZE_SEED,
        &freeze(cid_b, None, true, false, t + MAX_FREEZE_MS),
        b_actor,
    );
    run(&mut reports, r, None);
    let r = w.claim("Claim B's 1 ETH under the global mint freeze", &cid_b, 8, &VAULT_B, deposit_b.clone());
    run(&mut reports, r, Some(code::FROZEN));
    let r = w.role(
        "Freeze (freeze authority): ETH (A) burns for one hour",
        FREEZE_SEED,
        &freeze(cid_a, Some(ETH), false, true, t + HOUR_MS),
        eth_a_actor,
    );
    run(&mut reports, r, None);
    let r = burn(&mut w, "Burn 1 ETH (A) while its burns are frozen", cid_a, &note(3, ETH, DENOMINATIONS[1]));
    run(&mut reports, r, Some(code::FROZEN));
    let r = w.role(
        "Freeze (freeze authority): unfreeze ETH (A) burns",
        FREEZE_SEED,
        &freeze(cid_a, Some(ETH), false, true, 0),
        eth_a_actor,
    );
    run(&mut reports, r, None);
    let r = w.role(
        "Freeze (freeze authority): all burns of A for one hour",
        FREEZE_SEED,
        &freeze(cid_a, None, false, true, t + HOUR_MS),
        a_actor,
    );
    run(&mut reports, r, None);
    let r = burn(&mut w, "Burn 1 ETH (A) under the global burn freeze", cid_a, &note(3, ETH, DENOMINATIONS[1]));
    run(&mut reports, r, Some(code::FROZEN));
    let hour = w.finalize(t / 1000 + 3600);
    let r = burn(
        &mut w,
        "Burn 1 ETH (A) once a record an hour later exists, no Tick: still frozen by the attested clock",
        cid_a,
        &note(3, ETH, DENOMINATIONS[1]),
    );
    run(&mut reports, r, Some(code::FROZEN));
    let r = w.tick("Tick A: the record an hour later", &cid_a, None, hour);
    run(&mut reports, r, None);
    let r =
        burn(&mut w, "Burn 1 ETH (A) after the Tick: the freeze has lapsed", cid_a, &note(3, ETH, DENOMINATIONS[1]));
    run(&mut reports, r, None);

    // --- Sunset (retirement) --------------------------------------------------------------------
    let sunset_b = BridgeMessage::Sunset { config_id: cid_b };
    let r = w.role("Sunset B by a signer with no role", OUTSIDER_SEED, &sunset_b, b_actor);
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    let r = w.role("Sunset B by the admin", ADMIN_SEED, &sunset_b, b_actor);
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    let r = w.send("Sunset sent to config(B) itself", b_actor, &sunset_b, vec![b_actor]);
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    let r = w.role("Sunset B (freeze authority)", FREEZE_SEED, &sunset_b, b_actor);
    run(&mut reports, r, None);
    assert!(matches!(w.config_state(&cid_b), Some(ConfigState::Ready { sunset: true, .. })));
    let r = w.role("Sunset B again", FREEZE_SEED, &sunset_b, b_actor);
    run(&mut reports, r, Some(code::ALREADY_WRITTEN));
    let r = w.import("ImportRoot B after Sunset", &cid_b, 1, w.vault_b.root(8), 8, &VAULT_B);
    run(&mut reports, r, Some(code::FROZEN));
    let month = w.finalize((t + MAX_FREEZE_MS) / 1000);
    let r = w.tick("Tick B: the record 30 days later", &cid_b, None, month);
    run(&mut reports, r, None);
    let r = w.claim(
        "Claim B's 1 ETH after Sunset, the global freeze lapsed: claims continue",
        &cid_b,
        8,
        &VAULT_B,
        deposit_b,
    );
    run(&mut reports, r, None);
    assert_eq!(w.balance(payer, &cid_b), DENOMINATIONS[1]);
    let r = burn(&mut w, "Burn 1 ETH (B) after Sunset: burns continue", cid_b, &note(4, ETH, DENOMINATIONS[1]));
    run(&mut reports, r, None);
    assert_eq!(w.balance(payer, &cid_b), 0);

    // --- What the checkpoint collects of the Sunset ---------------------------------------------
    let tx = |label: &str| -> Vec<u8> {
        w.txs.iter().find(|(l, _)| l == label).unwrap_or_else(|| panic!("no transaction {label}")).1.clone()
    };
    let rows: Vec<(&str, Vec<u8>, bool)> = vec![
        ("Sunset B by a signer with no role", tx("Sunset B by a signer with no role"), true),
        ("Sunset B (freeze authority)", tx("Sunset B (freeze authority)"), false),
        ("Sunset B again", tx("Sunset B again"), true),
        ("ImportRoot B after Sunset", tx("ImportRoot B after Sunset"), true),
        ("Burn 1 ETH (B) after Sunset: burns continue", tx("Burn 1 ETH (B) after Sunset: burns continue"), false),
    ];
    let txs: Vec<&[u8]> = rows.iter().map(|(_, b, _)| b.as_slice()).collect();
    let reverted: Vec<u64> = rows.iter().enumerate().filter(|(_, r)| r.2).map(|(i, _)| i as u64).collect();
    let bytes = block(&txs, &reverted);
    let mut visitor = Calls { bridge: *program.value(), calls: Vec::new() };
    let summary = parse_block(&bytes, &mut visitor).expect("the checkpoint parses the block");
    let mut table = String::from(
        "| transaction | origin | message bytes | checkpoint's decoder | collected |\n|---|---|---:|---|---|\n",
    );
    let (mut sunsets, mut burns, mut imports) = (0, 0, 0);
    for (index, origin, message) in &visitor.calls {
        let decoded = CpMessage::decode(message);
        let collected = origin.is_public_execution() && !summary.is_reverted(*index);
        let what = match &decoded {
            Ok(Some(CpMessage::Sunset { config_id })) => {
                assert_eq!(*config_id, cid_b);
                sunsets += usize::from(collected);
                "Sunset"
            }
            Ok(Some(CpMessage::Burn(_))) => {
                burns += usize::from(collected);
                "Burn"
            }
            Ok(Some(CpMessage::ImportRoot(_))) => {
                imports += usize::from(collected);
                "ImportRoot"
            }
            Ok(Some(CpMessage::Echo { .. })) => "Echo",
            Ok(None) => "ignored (variant 4 and up)",
            Err(_) => panic!("a public bridge message the checkpoint cannot decode"),
        };
        let status = if summary.is_reverted(*index) {
            "no (reverted)"
        } else if collected {
            "yes"
        } else {
            "no"
        };
        table.push_str(&format!(
            "| {} | {origin:?} | {} | {what} | {status} |\n",
            rows[*index as usize].0,
            message.len()
        ));
    }
    println!("{table}");
    assert_eq!((sunsets, burns, imports), (1, 1, 0), "exactly the authority's Sunset and the burn are collected");
    let sunset_bytes = borsh::to_vec(&sunset_b).unwrap();
    assert_eq!(sunset_bytes.len(), 33, "the checkpoint's variant 2");
    assert_eq!(CpMessage::Sunset { config_id: cid_b }.encode(), sunset_bytes);

    let table_reports = render(&reports);
    fs::write(
        results_path("bridge-controls.md"),
        format!(
            "# Registry, fees, caps, freezes and Sunset (`harness/tests/bridge_controls.rs`)\n\n\
             Bridge program image `{}`; every transaction settled by LEZ's own functions in a block of its own.\n\n\
             {table_reports}\n## Handler executions\n\n{}\n\
             ## What the checkpoint program collects of the Sunset (`checkpoint-core`'s `parse_block` and `BridgeMessage::decode`)\n\n\
             One LEZ block holding the transactions below (the refused ones marked reverted, as settlement recorded them):\n\n{table}",
            harness::image_id_hex(methods::FERRATA_BRIDGE_ID),
            render_transitions(&reports),
        ),
    )
    .unwrap();
}
