//! The bridge program's clock (B2; review r3, attack A). Freezes and caps run on attested Ethereum
//! time: the execution timestamp of a record of the configuration's attestation instance, which a
//! `Tick` or a `Freeze` makes the gate read. The producer of a block chooses its timestamp (LEZ
//! v0.4.0-rc1 neither bounds nor orders block timestamps) and here stamps its block a year ahead:
//! it can end no freeze, refill no cap and stop no new freeze. A record ahead of the block (which
//! only a captured sync committee could have had written) moves no clock unless the producer also
//! stamps its block that far ahead. With no newer record (a stalled light client), freezes hold and
//! buckets stay empty however long LEZ runs. Writes `results/bridge-time.md`.

use std::{fs, path::PathBuf};

use ferrata_lez_bridge_wire::{
    code, config_actor, own_actor, token_actor, BridgeMessage, CapParams, ConfigState, Note, TimeRecord, MAX_FREEZE_MS,
};
use ferrata_lez_wire::{header_actor, header_seed};
use harness::{
    bridge::{render, Block, World, ADMIN_SEED, DENOMINATIONS, ETH, FREEZE_SEED, INSTANCE, LEZ_UNIT, VAULT_B},
    ferrata_account, render_transitions, TxReport,
};
use lee::PublicAccountEvidence;

const TARGET_GAS_EXEC: u64 = 5_000_000;
const HOUR_MS: u64 = 3_600_000;
const YEAR_MS: u64 = 365 * 24 * HOUR_MS;

fn results_path(name: &str) -> PathBuf {
    harness::results_dir().join(name)
}

fn run(reports: &mut Vec<TxReport>, report: TxReport, code: Option<u8>) {
    match code {
        None => assert!(report.ok(), "{}: {:?} (exit {:?})", report.label, report.charge.revert, report.exit_code),
        Some(c) => {
            assert_eq!(report.exit_code, Some(u32::from(c)), "{}: {:?}", report.label, report.charge.revert);
            assert!(!report.ok(), "{}", report.label);
        }
    }
    assert!(report.charge.gas_exec <= TARGET_GAS_EXEC, "{} is over TARGET_GAS_EXEC", report.label);
    reports.push(report);
}

/// Refused by LEZ itself, before any exit code: the transaction's timestamp window.
fn outside_window(reports: &mut Vec<TxReport>, report: TxReport) {
    assert!(!report.ok() && report.exit_code.is_none(), "{}: {:?}", report.label, report.charge.revert);
    assert!(
        report.charge.revert.as_deref().is_some_and(|e| e.contains("validity window")),
        "{}: {:?}",
        report.label,
        report.charge.revert
    );
    reports.push(report);
}

fn config_now(w: &World, cid: &[u8; 32]) -> u64 {
    match w.config_state(cid) {
        Some(ConfigState::Ready { gate, .. }) => gate.now_ms,
        other => panic!("not a ready configuration: {other:?}"),
    }
}

fn note(i: u8, amount: u128) -> Note {
    Note {
        r: [i; 32],
        memo: [i; 20],
        mac: [i; 16],
        token: ETH,
        amount: ferrata_lez_bridge_wire::word_of_u128(amount),
        c_out: [i; 32],
    }
}

#[test]
fn freezes_and_caps_follow_attested_time_only() {
    let mut w = World::with_roots();
    let (cid_a, cid_b, program) = (w.cid_a, w.cid_b, w.program);
    let va = w.vault_a.address;
    let (a_actor, b_actor) = (config_actor(program, &cid_a), config_actor(program, &cid_b));
    let eth_a = token_actor(program, &cid_a, &ETH);
    let mut reports = Vec::new();

    // Every gate reads the newest record (block 102's), 40 minutes behind the LEZ clock.
    let newest = 2;
    let t0 = w.blocks[newest].time_ms();
    assert!(t0 < w.time);
    for (label, cid, token) in [
        ("Tick A: the newest record", cid_a, None),
        ("Tick ETH (A): the newest record", cid_a, Some(ETH)),
        ("Tick B: the newest record", cid_b, None),
    ] {
        let r = w.tick(label, &cid, token, newest);
        run(&mut reports, r, None);
    }
    assert_eq!(config_now(&w, &cid_b), t0, "the record's execution timestamp, in ms");

    // --- Freezes: the producer cannot end one, nor stop a new one ------------------------------
    let record = w.blocks[newest].time_record();
    let freeze_b = |until_ms, record: &TimeRecord| BridgeMessage::Freeze {
        config_id: cid_b,
        token: None,
        mints: true,
        burns: false,
        until_ms,
        record: record.clone(),
    };
    let r = w.role("Freeze B's mints for one hour", FREEZE_SEED, &freeze_b(t0 + HOUR_MS, &record), b_actor);
    run(&mut reports, r, None);
    let deposit_b = w.vault_b.witness(&w.deposit_b.clone(), 8);
    let honest = w.time;
    w.time = honest + YEAR_MS;
    let r = w.tick("A block stamped a year ahead: Tick B with the newest record", &cid_b, None, newest);
    run(&mut reports, r, None);
    assert_eq!(config_now(&w, &cid_b), t0, "the clock is the record's, not the block's");
    let r = w.claim("Claim B's 1 ETH in that block: the freeze holds", &cid_b, 8, &VAULT_B, deposit_b.clone());
    run(&mut reports, r, Some(code::FROZEN));
    let r = w.role(
        "Freeze B's mints for 30 days, in that block: accepted (bounded by attested time)",
        FREEZE_SEED,
        &freeze_b(t0 + MAX_FREEZE_MS, &record),
        b_actor,
    );
    run(&mut reports, r, None);
    w.time = honest;

    // --- Caps: the producer cannot refill a bucket ---------------------------------------------
    let set_cap = |cid, token, mints, cap: CapParams| BridgeMessage::SetCap { config_id: cid, token, mints, cap };
    let eleven = CapParams { capacity: 11_000_000_000, refill_per_second: 10_000_000 };
    let r = w.role(
        "SetCap (admin): ETH (A) mints 11 ETH, 0.01 ETH/s",
        ADMIN_SEED,
        &set_cap(cid_a, Some(ETH), true, eleven),
        eth_a,
    );
    run(&mut reports, r, None);
    let leaf6 = w.vault_a.witness(&w.payer_deposit.clone(), 8);
    let vector = w.vault_a.witness(&w.vector.clone(), 5);
    let r = w.claim("Claim leaf 6 (10.06 ETH of the 11)", &cid_a, 8, &va, leaf6);
    run(&mut reports, r, None);
    w.time = honest + YEAR_MS;
    let r = w.tick("A block stamped a year ahead: Tick ETH (A) with the newest record", &cid_a, Some(ETH), newest);
    run(&mut reports, r, None);
    let r = w.tick("Tick ETH (A) with an older record (block 100's)", &cid_a, Some(ETH), 0);
    run(&mut reports, r, None);
    assert_eq!(w.token_state(&cid_a, &ETH).unwrap().gate.now_ms, t0, "monotone: an older record moves nothing");
    let r = w.claim(
        "Claim leaf 2 (1.001 ETH) in that block: 0.94 ETH left, nothing refilled",
        &cid_a,
        5,
        &va,
        vector.clone(),
    );
    run(&mut reports, r, Some(code::CAPPED));
    w.time = honest;

    // --- Attested time moves them --------------------------------------------------------------
    let ten_s = w.finalize(t0 / 1000 + 10);
    let r = w.tick("Tick ETH (A): a record 10 s later", &cid_a, Some(ETH), ten_s);
    run(&mut reports, r, None);
    let r = w.claim("Claim leaf 2 after it: 0.1 ETH refilled", &cid_a, 5, &va, vector);
    run(&mut reports, r, None);
    assert_eq!(w.token_state(&cid_a, &ETH).unwrap().gate.mint.level, 940_000_000 + 100_000_000 - 1_001_000_000);
    let month = w.finalize((t0 + MAX_FREEZE_MS) / 1000);
    let r = w.tick("Tick B: a record 30 days after the freeze's start", &cid_b, None, month);
    run(&mut reports, r, None);
    let r = w.claim("Claim B's 1 ETH: the freeze has lapsed", &cid_b, 8, &VAULT_B, deposit_b);
    run(&mut reports, r, None);

    // --- A stalled light client: no newer record ----------------------------------------------
    let one_eth = CapParams { capacity: DENOMINATIONS[1] / LEZ_UNIT, refill_per_second: 10_000_000 };
    let r = w.role(
        "SetCap (admin): ETH (A) burns 1 ETH, 0.01 ETH/s",
        ADMIN_SEED,
        &set_cap(cid_a, Some(ETH), false, one_eth),
        eth_a,
    );
    run(&mut reports, r, None);
    let r = w.tick("Tick ETH (A): the newest record (30 days on)", &cid_a, Some(ETH), month);
    run(&mut reports, r, None);
    let stuck = w.newest_record_seconds() * 1000;
    let freeze_a = BridgeMessage::Freeze {
        config_id: cid_a,
        token: None,
        mints: false,
        burns: true,
        until_ms: stuck + HOUR_MS,
        record: w.blocks[month].time_record(),
    };
    let r = w.role("Freeze A's burns for one hour", FREEZE_SEED, &freeze_a, a_actor);
    run(&mut reports, r, None);
    let payer = w.chain.payer;
    let holder = own_actor(program, payer);
    let burn = |w: &mut World, label: &str, note: &Note| -> TxReport {
        let actors = w.public_burn_actors(payer, &cid_a, note);
        w.send(label, holder, &BridgeMessage::burn(cid_a, note), actors)
    };
    w.time += YEAR_MS;
    let r = w.tick("A year of LEZ blocks later, no newer record: Tick A", &cid_a, None, month);
    run(&mut reports, r, None);
    let r = burn(&mut w, "Burn 1 ETH (A): A's burns still frozen", &note(1, DENOMINATIONS[1]));
    run(&mut reports, r, Some(code::FROZEN));
    let BridgeMessage::Freeze { config_id, token, mints, burns, record, .. } = freeze_a else { unreachable!() };
    let unfreeze = BridgeMessage::Freeze { config_id, token, mints, burns, until_ms: 0, record };
    let r = w.role("Unfreeze A's burns (freeze authority), still no newer record", FREEZE_SEED, &unfreeze, a_actor);
    run(&mut reports, r, None);
    let r = burn(&mut w, "Burn 1 ETH (A)", &note(1, DENOMINATIONS[1]));
    run(&mut reports, r, None);
    let r = w.tick("Tick ETH (A), still no newer record", &cid_a, Some(ETH), month);
    run(&mut reports, r, None);
    let r = burn(&mut w, "Burn 1 ETH (A): its burn bucket has not refilled", &note(2, DENOMINATIONS[1]));
    run(&mut reports, r, Some(code::CAPPED));
    let r = w.role(
        "SetCap (admin): ETH (A) burns 100 ETH, still no newer record",
        ADMIN_SEED,
        &set_cap(cid_a, Some(ETH), false, harness::bridge::TOKEN_CAP),
        eth_a,
    );
    run(&mut reports, r, None);
    let r = burn(&mut w, "Burn 1 ETH (A): raising the capacity refills nothing", &note(2, DENOMINATIONS[1]));
    run(&mut reports, r, Some(code::CAPPED));

    // --- A record ahead of the block: it takes a producer as well --------------------------------
    let lez_now = w.time;
    let ahead = w.record_at(lez_now / 1000 + 3600);
    let r = w.tick(
        "Tick ETH (A) with a record an hour ahead of the block (a captured committee's)",
        &cid_a,
        Some(ETH),
        ahead,
    );
    outside_window(&mut reports, r);
    assert_eq!(w.token_state(&cid_a, &ETH).unwrap().gate.now_ms, stuck, "refused: the clock did not move");
    w.time = w.blocks[ahead].time_ms();
    let r = w.tick("The same Tick in a block its producer stamps an hour ahead too", &cid_a, Some(ETH), ahead);
    run(&mut reports, r, None);
    assert_eq!(w.token_state(&cid_a, &ETH).unwrap().gate.now_ms, w.blocks[ahead].time_ms());

    // --- Records in the Gloas layout, and refusals ---------------------------------------------
    let slot = w.next_slot();
    let gloas = w.add_block(Block::gloas(slot, &[]));
    let gloas_ms = w.blocks[gloas].time_ms();
    w.time = w.time.max(gloas_ms + 768_000);
    let gloas_slot = w.blocks[gloas].slot;
    let tick = |token: Option<[u8; 20]>, record_slot: u64, header: Option<Vec<u8>>| BridgeMessage::Tick {
        config_id: cid_a,
        token,
        record: TimeRecord { record_slot, header },
    };
    let gloas_record = header_actor(ferrata_account(), &INSTANCE, gloas_slot);
    let mut wrong = w.blocks[gloas].header.clone().expect("a Gloas block carries its header");
    let last = wrong.len() - 1;
    wrong[last] ^= 1;
    for (label, header, expected) in [
        ("Tick A: a Gloas record without its block's header", None, Some(code::IMPORT_MISMATCH)),
        ("Tick A: a Gloas record with a header that does not hash to it", Some(wrong), Some(code::IMPORT_MISMATCH)),
        ("Tick A: a Gloas record with its block's header", w.blocks[gloas].header.clone(), None),
    ] {
        let r = w.send(label, a_actor, &tick(None, gloas_slot, header), vec![a_actor, gloas_record]);
        run(&mut reports, r, expected);
    }
    assert_eq!(config_now(&w, &cid_a), gloas_ms, "the time of the header the record binds");
    let payload_record = header_actor(ferrata_account(), &INSTANCE, w.blocks[newest].slot);
    let r = w.send(
        "Tick A: a payload record with a header",
        a_actor,
        &tick(None, w.blocks[newest].slot, w.blocks[gloas].header.clone()),
        vec![a_actor, payload_record],
    );
    run(&mut reports, r, Some(code::IMPORT_MISMATCH));
    let empty = header_actor(ferrata_account(), &INSTANCE, 1);
    let r = w.submit(
        "Tick A: a slot with no record",
        a_actor,
        &tick(None, 1, None),
        vec![a_actor, empty],
        vec![PublicAccountEvidence::Pda { program: ferrata_account(), seed: header_seed(&INSTANCE, 1) }],
    );
    run(&mut reports, r, Some(code::NOT_FOUND));

    let table = render(&reports);
    fs::write(
        results_path("bridge-time.md"),
        format!(
            "# The bridge program's clock: attested Ethereum time (`harness/tests/bridge_time.rs`)\n\n\
             Bridge program image `{}`; every transaction settled by LEZ's own functions in a block of its own, \
             stamped with the world's LEZ time (a year ahead where the label says so).\n\n{table}\n\
             ## Handler executions\n\n{}\n",
            harness::image_id_hex(harness::bridge::BRIDGE_ID),
            render_transitions(&reports),
        ),
    )
    .unwrap();
}
