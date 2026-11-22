//! A captured sync committee's exposure of the bridge, and the consumer's caps and freeze that
//! bound it (RFP-021, Verification Security #4: "Document the failure modes that follow from a
//! compromised or colluding sync committee, and what a consuming application can do to bound its
//! exposure (for example, caps and freeze authorities on the consumer side, as RFP-022
//! specifies)").
//!
//! The consuming application is the bridge program: it mints wrapped tokens against the deposits
//! an attested record proves. `docs/trust.md` §2.2 ("What a captured committee can do", VS.4):
//! "a forged record whose vault slot 0 holds invented deposits lets `ImportRoot` accept them and
//! claims mint wrapped tokens with no escrow behind them. What bounds it: the bridge's mint caps
//! (per token and global, on LEZ) until the freeze authority freezes mints; wrapped supply then
//! exceeds escrow by at most what the caps let through"; §6's first row names the lost positions
//! ("wrapped supply above escrow; the shortfall falls on late redeemers") and the same bound.
//!
//! The fabricated record is made exactly as this harness makes every record (the `harness::bridge`
//! header): a synthetic execution block holding the vault's storage as the forger chooses it —
//! invented deposits appended to the vault's tree — whose record `World::add_block` places at the
//! attestation program's `header(instance, slot)` the way the light client writes one after a
//! finality update. Nothing downstream can tell the difference; the only dishonest step is the
//! two-thirds signature that finalized a block Ethereum never produced (the consensus program's
//! half, not the consumer's: trust.md §2.2, "Evidence that a non-captured committee is required").
//! The record carries an honest-looking timestamp (48 s after the newest honest record, behind the
//! LEZ clock as a record of a final block is). A record stamped ahead of every LEZ block is the
//! committee's other move, and the last sections show it bounded as well (trust.md §2.2: "a
//! captured committee's far-future record refills no bucket and ends no freeze unless a block
//! producer also stamps its block that far ahead").
//!
//! Against one configuration with two registered tokens (ETH and T3), whose admin sized the caps
//! before the attack (trust.md §2.2: "a consumer sizes what its caps let through before a freeze
//! to what it can afford to lose"):
//! - claims of the invented deposits mint (the forger's account and the relayer credited); the
//!   fabricated root is spent like an honest one;
//! - the fourth claim of a token is refused with the cap's typed code (CAPPED, 255) and the
//!   token's bucket empty: at most the per-token cap is minted in a window;
//! - with the global cap below the two tokens' caps combined (4 claims against their 6), the two
//!   together are refused the same way with the global bucket empty while a token's bucket still
//!   holds more than a claim;
//! - after one refill window of attested time, exactly one more window's worth is let through:
//!   the loss over time is the cap per window, asserted as `capacity + elapsed × refill` and
//!   stated in the printed table;
//! - once the freeze authority freezes mints globally, every further claim is refused with the
//!   freeze code (FROZEN, 242) whatever the record says: a second fabricated record, imported
//!   while frozen, does not help, and a record ahead of every LEZ block cannot even move the
//!   clock;
//! - a token-scoped freeze refuses that token's claims while the other token's continue within
//!   its caps.
//!
//! Writes `results/captured-committee-exposure.md`.

use std::{fs, path::PathBuf};

use ferrata_bridge_core::{
    chain_id_u256,
    inbound::{deposit_secret, DestKind},
    BridgeKeys,
};
use ferrata_lez_bridge_wire::{
    self as bw, code, config_actor, token_actor, BridgeMessage, CapParams, ConfigState, Gate, TimeRecord, MAX_FREEZE_MS,
};
use harness::bridge::{
    eth_entry, fee_recipient, registry_slots, relayer, render, role_account, t3_entry, Block, Deposit, Vault,
    VaultSlots, World, ADMIN_SEED, CHAIN_ID, DENOMINATIONS, ETH, FLOOR, FREEZE_SEED, GWEI, OUTSIDER_SEED, RELAY_FEES,
    T3,
};
use harness::{render_transitions, TxReport};
use lee::AccountId;

const TARGET_GAS_EXEC: u64 = 5_000_000;
const HOUR_MS: u64 = 3_600_000;

/// What one invented claim of the 1 ETH denomination takes from a cap bucket: the
/// denomination plus its relay fee (schedule 0 charges no mint fee), in the caps' value units
/// (ETH-weighted: one unit per gwei, `unitWeight` 1). 1_001_000_000 — the figure `bridge_time.rs`
/// asserts for a 1 ETH claim.
const CLAIM_VALUE: u128 = DENOMINATIONS[1] / GWEI + RELAY_FEES[1] / GWEI;
/// The per-token mint cap the admin sets before the attack: three claims' worth a window, never
/// below the immutable floor.
const TOKEN_TEST: CapParams = CapParams { capacity: 3 * CLAIM_VALUE, refill_per_second: FLOOR.refill_per_second };
/// The global mint cap: four claims' worth — below the two tokens' caps combined (six claims), so
/// the global bound is the one that binds the two together (trust.md §2.2: "per token and global").
const GLOBAL_TEST: CapParams = CapParams { capacity: 4 * CLAIM_VALUE, refill_per_second: FLOOR.refill_per_second };
/// The global bucket's refill window: the seconds a drained bucket takes to fill again.
const WINDOW_S: u64 = (GLOBAL_TEST.capacity / GLOBAL_TEST.refill_per_second) as u64;

fn results_path(name: &str) -> PathBuf {
    harness::results_dir().join(name)
}

fn expect(report: &TxReport, expected: Option<u8>) {
    match expected {
        None => assert!(report.ok(), "{}: {:?} (exit {:?})", report.label, report.charge.revert, report.exit_code),
        Some(c) => {
            assert_eq!(report.exit_code, Some(u32::from(c)), "{}: {:?}", report.label, report.charge.revert);
            assert!(!report.ok(), "{}", report.label);
        }
    }
    assert!(report.charge.gas_exec <= TARGET_GAS_EXEC, "{} is over TARGET_GAS_EXEC", report.label);
}

fn run(reports: &mut Vec<TxReport>, report: TxReport, code: Option<u8>) {
    expect(&report, code);
    reports.push(report);
}

/// Refused by LEZ itself, before any exit code: the block's validity window. A `Tick` naming a
/// record ahead of its block — which only a captured committee could have had written — moves no
/// clock (`bridge_time.rs` shows the same refusal).
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

fn config_gate(w: &World, cid: &[u8; 32]) -> Gate {
    match w.config_state(cid) {
        Some(ConfigState::Ready { gate, .. }) => gate,
        other => panic!("not a ready configuration: {other:?}"),
    }
}

/// The level of `token`'s mint bucket (the value units still mintable this window).
fn mint_level(w: &World, cid: &[u8; 32], token: &[u8; 20]) -> u128 {
    w.token_state(cid, token).expect("a registered token").gate.mint.level
}

/// What the forger has minted of `token` so far, in the caps' value units: the destination's and
/// the relayer's holdings together — the denomination plus the relay fee is the whole of a mint
/// under schedule 0.
fn minted_value(w: &World, cid: &[u8; 32], dest: AccountId, token: [u8; 20]) -> u128 {
    (w.balance_of(dest, cid, &token) + w.balance_of(relayer(), cid, &token)) / GWEI
}

/// One claim's outcome asserted, and one row of the summary table recorded: what it minted so
/// far, or the typed code that refused it and why (which bucket or freeze refused). `why` is
/// shown only for refusals.
#[allow(clippy::too_many_arguments)]
fn claim_step(
    report: TxReport,
    expected: Option<u8>,
    why: &str,
    w: &World,
    cid: &[u8; 32],
    dest: AccountId,
    token: [u8; 20],
    reports: &mut Vec<TxReport>,
    table: &mut Vec<String>,
) {
    expect(&report, expected);
    let label = report.label.clone();
    let minted = minted_value(w, cid, dest, token);
    let outcome = match expected {
        None => format!("minted ({minted} value units of the caps so far)"),
        Some(c) => {
            let name = harness::bridge::code_name(u32::from(c)).unwrap_or("?");
            format!("refused, exit {c} ({name}): {why}")
        }
    };
    table.push(format!("| {label} | {outcome} |"));
    reports.push(report);
}

/// Vault A's storage as the captured committee writes it into a fabricated block: the deposit
/// tree `vault` (honest leaves and invented ones alike), the count it claims, the configuration
/// id the honest vault holds, and the two tokens' registry entries with schedule 0 only (no fee
/// schedule has taken effect, so every claim falls under schedule 0).
fn forged_slots(cid: &[u8; 32], vault: &Vault, n: u64) -> VaultSlots {
    let mut extra = registry_slots(&eth_entry(), &[]);
    extra.extend(registry_slots(&t3_entry(), &[]));
    VaultSlots { address: vault.address, root: vault.root(n), count: n, config_id: *cid, extra }
}

/// The freeze authority's `Freeze` of mints (`burns` never touched here).
fn freeze_mints(cid: [u8; 32], token: Option<[u8; 20]>, until_ms: u64, record: &TimeRecord) -> BridgeMessage {
    BridgeMessage::Freeze { config_id: cid, token, mints: true, burns: false, until_ms, record: record.clone() }
}

#[test]
fn a_captured_committee_mints_only_what_the_caps_let_through() {
    let mut w = World::new();
    let program = w.program;
    let a = w.config_a.clone();
    let cid = w.cid_a;
    let va = w.vault_a.address;
    let attacker = role_account(OUTSIDER_SEED);
    let mut reports = Vec::new();
    let mut table = Vec::new();
    assert_eq!(CLAIM_VALUE, 1_001_000_000, "the figure bridge_time.rs asserts for a 1 ETH claim");
    assert_eq!(w.vault_a.leaves.len(), 8, "the honest tree: eight deposits, eight escrows on Ethereum");

    // --- The captured committee's record (trust.md §2.2, VS.4) ---------------------------------
    // Ten invented deposits appended to vault A's tree (leaves 8-12 of ETH, 13-17 of T3), each
    // made exactly as a wallet makes a deposit: nothing about them is real but the arithmetic, and
    // every one of them pays the forger the 1 ETH denomination. The block below carries
    // the vault's storage with their root in slot 0 and the count 18 in slot 1; its record is
    // placed as the light client would place it after a finality update that Ethereum never saw.
    let keys = BridgeKeys::from_bridge_seed(&[0x66; 32]);
    let forger: [u8; 20] = [0xdd; 20];
    let mut leaves = w.vault_a.leaves.clone();
    let mut invented = Vec::new();
    for (leaf, token) in (8..13).map(|l| (l, ETH)).chain((13..18).map(|l| (l, T3))) {
        let d = Deposit {
            leaf_index: leaf,
            depositor: forger,
            token,
            amount: bw::word_of_u128(DENOMINATIONS[1]),
            s: deposit_secret(&keys.auth, &chain_id_u256(CHAIN_ID), &va, &forger, leaf),
            kind: DestKind::Public,
            dest: *attacker.value(),
        };
        assert_eq!(leaves.len() as u64, leaf);
        leaves.push(d.leaf());
        invented.push(d);
    }
    let (eth_d, t3_d) = invented.split_at(5);
    let vault18 = Vault { address: va, leaves };
    let f1 = w.add_block(Block::new(w.next_slot(), 103 + w.blocks.len() as u64, &[forged_slots(&cid, &vault18, 18)]));
    let t_f = w.blocks[f1].time_ms();

    // --- The consumer pairs against an honest record, sizes its caps, lets the record in --------
    // trust.md §2.2: the design "bounds the loss, not the cost: a consumer sizes what its caps
    // let through before a freeze to what it can afford to lose". The admin sets both tokens at
    // three claims a window and the global bucket at four — below the tokens' six combined.
    run(&mut reports, w.init("Init A (an honest record, block 100)", &a, 0), None);
    run(&mut reports, w.tick("Tick A: the captured committee's record", &cid, None, f1), None);
    run(&mut reports, w.register("Register ETH (A)", &cid, 0, &eth_entry(), &va), None);
    run(&mut reports, w.register("Register T3 (A)", &cid, 0, &t3_entry(), &va), None);
    let a_actor = config_actor(program, &cid);
    let eth_actor = token_actor(program, &cid, &ETH);
    let t3_actor = token_actor(program, &cid, &T3);
    let set_cap =
        |token: Option<[u8; 20]>, cap: CapParams| BridgeMessage::SetCap { config_id: cid, token, mints: true, cap };
    run(
        &mut reports,
        w.role(
            "SetCap (admin): the global mint bucket at 4 claims a window",
            ADMIN_SEED,
            &set_cap(None, GLOBAL_TEST),
            a_actor,
        ),
        None,
    );
    run(
        &mut reports,
        w.role(
            "SetCap (admin): ETH (A) mints at 3 claims a window",
            ADMIN_SEED,
            &set_cap(Some(ETH), TOKEN_TEST),
            eth_actor,
        ),
        None,
    );
    run(
        &mut reports,
        w.role(
            "SetCap (admin): T3 (A) mints at 3 claims a window",
            ADMIN_SEED,
            &set_cap(Some(T3), TOKEN_TEST),
            t3_actor,
        ),
        None,
    );
    assert_eq!(config_gate(&w, &cid).now_ms, t_f, "the clocks read the fabricated record like any other");
    assert_eq!(config_gate(&w, &cid).mint.level, GLOBAL_TEST.capacity, "the buckets start full");
    assert_eq!(mint_level(&w, &cid, &ETH), TOKEN_TEST.capacity);
    assert_eq!(mint_level(&w, &cid, &T3), TOKEN_TEST.capacity);
    table.push(
        "| Caps set before the attack | per token 3 claims a window, globally 4 (below the tokens' 6) |".to_string(),
    );

    // The fabricated root is imported as an honest one would be; the fee schedules are closed at
    // the invented count so the invented leaves fall under schedule 0 (no protocol fee).
    run(
        &mut reports,
        w.import("ImportRoot A: 18 leaves under the fabricated record", &cid, f1, vault18.root(18), 18, &va),
        None,
    );
    assert_eq!(
        w.root_state(&cid, 18).expect("root(A, 18)").root,
        vault18.root(18),
        "trust.md §2.2 (VS.4): a forged record's invented deposits are imported as any other"
    );
    run(
        &mut reports,
        w.import_fees("ImportFees ETH (A): the schedules cover 18 leaves", &cid, f1, ETH, 1, None, &va),
        None,
    );
    run(
        &mut reports,
        w.import_fees("ImportFees T3 (A): the schedules cover 18 leaves", &cid, f1, T3, 1, None, &va),
        None,
    );

    // --- The first window: claims succeed, the token cap refuses, the global cap refuses ---------
    // trust.md §2.2 (VS.4): "claims mint wrapped tokens with no escrow behind them" — three do.
    for i in 0..3u64 {
        let r = w.claim(
            &format!("Claim the invented ETH deposit at leaf {}", 8 + i),
            &cid,
            18,
            &va,
            vault18.witness(&eth_d[i as usize], 18),
        );
        claim_step(r, None, "", &w, &cid, attacker, ETH, &mut reports, &mut table);
    }
    assert_eq!(w.balance_of(attacker, &cid, &ETH), 3 * DENOMINATIONS[1], "three unbacked denominations minted");
    assert_eq!(w.balance_of(relayer(), &cid, &ETH), 3 * RELAY_FEES[1], "the relay fees minted with them");
    assert_eq!(w.balance_of(fee_recipient(), &cid, &ETH), 0, "schedule 0: no protocol fee");
    assert_eq!(
        minted_value(&w, &cid, attacker, ETH),
        TOKEN_TEST.capacity,
        "trust.md §2.2, §6: the window's minting is at most the per-token cap — here exactly it"
    );
    // The fourth is refused with the cap's typed code, the token's bucket empty (§6's bound).
    let r = w.claim(
        "Claim the invented ETH deposit at leaf 11: the token's bucket is empty",
        &cid,
        18,
        &va,
        vault18.witness(&eth_d[3], 18),
    );
    claim_step(
        r,
        Some(code::CAPPED),
        "the per-token mint bucket is empty",
        &w,
        &cid,
        attacker,
        ETH,
        &mut reports,
        &mut table,
    );
    assert_eq!(mint_level(&w, &cid, &ETH), 0, "the token's mint bucket is what refused");
    assert_eq!(config_gate(&w, &cid).mint.level, CLAIM_VALUE, "the global bucket still holds one claim: not reached");
    assert!(
        w.nullifier(&cid, &va, &eth_d[3].nullifier(CHAIN_ID, &va)).is_none(),
        "a refused claim leaves its invented deposit claimable"
    );
    // The other token still has its own bucket, but the global one is what is left: one T3 claim
    // empties it, and the next is refused by the global cap while T3's bucket still holds two.
    let r = w.claim(
        "Claim the invented T3 deposit at leaf 13 (the global bucket's last claim)",
        &cid,
        18,
        &va,
        vault18.witness(&t3_d[0], 18),
    );
    claim_step(r, None, "", &w, &cid, attacker, T3, &mut reports, &mut table);
    assert_eq!(
        minted_value(&w, &cid, attacker, ETH) + minted_value(&w, &cid, attacker, T3),
        GLOBAL_TEST.capacity,
        "trust.md §2.2, §6: the total over both tokens is at most the global cap — exactly it here, \
         though the two tokens' own caps add to 6 claims"
    );
    let r = w.claim(
        "Claim the invented T3 deposit at leaf 14: the global bucket is empty",
        &cid,
        18,
        &va,
        vault18.witness(&t3_d[1], 18),
    );
    claim_step(
        r,
        Some(code::CAPPED),
        "the global mint bucket is empty while T3's still holds two claims",
        &w,
        &cid,
        attacker,
        T3,
        &mut reports,
        &mut table,
    );
    assert_eq!(config_gate(&w, &cid).mint.level, 0, "the global bucket is what refused");
    assert_eq!(
        mint_level(&w, &cid, &T3),
        2 * CLAIM_VALUE,
        "T3's own bucket still holds more than a claim: the global cap bounds the two tokens together"
    );

    // --- The window rolls on attested time: one more window's worth (§2.2: the caps are refilled
    // on attested Ethereum time; the loss over time is the cap per window until the freeze) ------
    let mid = w.finalize(t_f / 1000 + WINDOW_S);
    let t2 = w.blocks[mid].time_ms();
    for (name, token) in [("the global gate", None), ("ETH (A)", Some(ETH)), ("T3 (A)", Some(T3))] {
        let r = w.tick(&format!("Tick {name}: a record one refill window later"), &cid, token, mid);
        run(&mut reports, r, None);
    }
    for i in 1..4usize {
        let r = w.claim(
            &format!("Claim the invented T3 deposit at leaf {}", 13 + i),
            &cid,
            18,
            &va,
            vault18.witness(&t3_d[i], 18),
        );
        claim_step(r, None, "", &w, &cid, attacker, T3, &mut reports, &mut table);
    }
    let r = w.claim(
        "Claim the invented T3 deposit at leaf 17: T3's bucket is empty again",
        &cid,
        18,
        &va,
        vault18.witness(&t3_d[4], 18),
    );
    claim_step(
        r,
        Some(code::CAPPED),
        "T3's mint bucket is empty (the global one still holds a claim)",
        &w,
        &cid,
        attacker,
        T3,
        &mut reports,
        &mut table,
    );
    assert_eq!(mint_level(&w, &cid, &T3), 0, "the second window's per-token cap reached");
    assert_eq!(
        config_gate(&w, &cid).mint.level,
        CLAIM_VALUE,
        "the global bucket still holds a claim: the token's cap bound this window"
    );
    let r = w.claim(
        "Claim the invented ETH deposit at leaf 11 again, the next window",
        &cid,
        18,
        &va,
        vault18.witness(&eth_d[3], 18),
    );
    claim_step(r, None, "", &w, &cid, attacker, ETH, &mut reports, &mut table);
    let r = w.claim(
        "Claim the invented ETH deposit at leaf 12: the global bucket is empty again",
        &cid,
        18,
        &va,
        vault18.witness(&eth_d[4], 18),
    );
    claim_step(
        r,
        Some(code::CAPPED),
        "the global mint bucket is empty while ETH's still holds two claims",
        &w,
        &cid,
        attacker,
        ETH,
        &mut reports,
        &mut table,
    );
    assert_eq!(config_gate(&w, &cid).mint.level, 0);
    assert_eq!(mint_level(&w, &cid, &ETH), 2 * CLAIM_VALUE);
    assert_eq!(
        minted_value(&w, &cid, attacker, ETH) + minted_value(&w, &cid, attacker, T3),
        2 * GLOBAL_TEST.capacity,
        "one refill window later, exactly one more window's worth went through: the loss over time \
         is the cap per window"
    );

    // --- The freeze authority freezes all mints: every further claim refused (§2.2, §6 row 1) ---
    let mid_record = w.blocks[mid].time_record();
    run(
        &mut reports,
        w.role(
            "Freeze (freeze authority): all mints of A for one hour of attested time",
            FREEZE_SEED,
            &freeze_mints(cid, None, t2 + HOUR_MS, &mid_record),
            a_actor,
        ),
        None,
    );
    table.push(
        "| Freeze (freeze authority): all mints of A | applied, until one hour of attested time passes |".to_string(),
    );
    let r = w.claim(
        "Claim the invented ETH deposit at leaf 12 while mints are frozen",
        &cid,
        18,
        &va,
        vault18.witness(&eth_d[4], 18),
    );
    claim_step(r, Some(code::FROZEN), "mints frozen globally", &w, &cid, attacker, ETH, &mut reports, &mut table);

    // A second fabricated record arrives while mints are frozen: two more invented deposits
    // (leaves 18 and 19), stamped a minute after the freeze's record — the age an honest finality
    // update would carry. Imports are not frozen (only Sunset refuses them), so the record goes
    // in; the freeze is what refuses its deposits.
    let mut leaves20 = vault18.leaves.clone();
    let mut more = Vec::new();
    for (leaf, token) in [(18u64, ETH), (19, T3)] {
        let d = Deposit {
            leaf_index: leaf,
            depositor: forger,
            token,
            amount: bw::word_of_u128(DENOMINATIONS[1]),
            s: deposit_secret(&keys.auth, &chain_id_u256(CHAIN_ID), &va, &forger, leaf),
            kind: DestKind::Public,
            dest: *attacker.value(),
        };
        assert_eq!(leaves20.len() as u64, leaf);
        leaves20.push(d.leaf());
        more.push(d);
    }
    let vault20 = Vault { address: va, leaves: leaves20 };
    let f2 = w.add_block(
        Block::new(w.next_slot(), 103 + w.blocks.len() as u64, &[forged_slots(&cid, &vault20, 20)]).at(t2 / 1000 + 60),
    );
    run(
        &mut reports,
        w.import("ImportRoot A: 20 leaves under the second fabricated record", &cid, f2, vault20.root(20), 20, &va),
        None,
    );
    run(
        &mut reports,
        w.import_fees("ImportFees ETH (A): the schedules cover 20 leaves", &cid, f2, ETH, 1, None, &va),
        None,
    );
    run(
        &mut reports,
        w.import_fees("ImportFees T3 (A): the schedules cover 20 leaves", &cid, f2, T3, 1, None, &va),
        None,
    );
    table.push(
        "| ImportRoot under the second fabricated record | imported: a freeze stops mints, not imports |".to_string(),
    );
    let r = w.claim(
        "Claim the second record's invented ETH deposit at leaf 18 while mints are frozen",
        &cid,
        20,
        &va,
        vault20.witness(&more[0], 20),
    );
    claim_step(
        r,
        Some(code::FROZEN),
        "a fresh fabricated record does not unfreeze",
        &w,
        &cid,
        attacker,
        ETH,
        &mut reports,
        &mut table,
    );
    // The clocks read the second record too (a minute on): the freeze ends on attested time, and
    // attested time has not reached its end. "Whatever the record says" changes nothing here.
    for (name, token) in [("the global gate", None), ("ETH (A)", Some(ETH)), ("T3 (A)", Some(T3))] {
        let r = w.tick(&format!("Tick {name}: the second fabricated record, a minute later"), &cid, token, f2);
        run(&mut reports, r, None);
    }
    let r = w.claim(
        "Claim the invented ETH deposit at leaf 12 a minute of attested time later",
        &cid,
        18,
        &va,
        vault18.witness(&eth_d[4], 18),
    );
    claim_step(
        r,
        Some(code::FROZEN),
        "the freeze ends on attested time, not on records",
        &w,
        &cid,
        attacker,
        ETH,
        &mut reports,
        &mut table,
    );
    // The committee's other move: a record stamped ahead of every LEZ block, which only it could
    // have had written. The Tick naming it is valid only in a block stamped at or after it, so it
    // cannot even move the clock (§2.2: "refills no bucket and ends no freeze unless a block
    // producer also stamps its block that far ahead").
    let far = w.record_at(t2 / 1000 + 2 * 3600);
    outside_window(&mut reports, w.tick("Tick A with a record two hours ahead of every LEZ block", &cid, None, far));
    assert_eq!(
        config_gate(&w, &cid).now_ms,
        w.blocks[f2].time_ms(),
        "no clock moved: a captured committee alone does not advance attested time"
    );
    let r = w.claim(
        "Claim the invented ETH deposit at leaf 12 after the far-future record",
        &cid,
        18,
        &va,
        vault18.witness(&eth_d[4], 18),
    );
    claim_step(
        r,
        Some(code::FROZEN),
        "the far-future record ended no freeze",
        &w,
        &cid,
        attacker,
        ETH,
        &mut reports,
        &mut table,
    );

    // --- A token-scoped freeze: that token's claims refused, the other's continue ----------------
    let f2_record = w.blocks[f2].time_record();
    run(
        &mut reports,
        w.role(
            "Freeze (freeze authority): unfreeze A's mints",
            FREEZE_SEED,
            &freeze_mints(cid, None, 0, &f2_record),
            a_actor,
        ),
        None,
    );
    run(
        &mut reports,
        w.role(
            "Freeze (freeze authority): ETH (A) mints for 30 days of attested time",
            FREEZE_SEED,
            &freeze_mints(cid, Some(ETH), w.blocks[f2].time_ms() + MAX_FREEZE_MS, &f2_record),
            eth_actor,
        ),
        None,
    );
    table.push("| Freeze (freeze authority): ETH (A) mints only | applied, for 30 days of attested time |".to_string());
    let later = w.finalize(w.blocks[f2].time_ms() / 1000 + WINDOW_S);
    let t_end = w.blocks[later].time_ms();
    for (name, token) in [("the global gate", None), ("ETH (A)", Some(ETH)), ("T3 (A)", Some(T3))] {
        let r = w.tick(&format!("Tick {name}: a record one refill window later again"), &cid, token, later);
        run(&mut reports, r, None);
    }
    let r = w.claim(
        "Claim the second record's invented ETH deposit at leaf 18 while only ETH's mints are frozen",
        &cid,
        20,
        &va,
        vault20.witness(&more[0], 20),
    );
    claim_step(
        r,
        Some(code::FROZEN),
        "ETH's own gate is frozen, a check that runs before its bucket",
        &w,
        &cid,
        attacker,
        ETH,
        &mut reports,
        &mut table,
    );
    // The token-scoped freeze isolates: T3's claims run as before, within its caps.
    let r = w.claim(
        "Claim the invented T3 deposit at leaf 17 while only ETH is frozen",
        &cid,
        20,
        &va,
        vault20.witness(&t3_d[4], 20),
    );
    claim_step(r, None, "", &w, &cid, attacker, T3, &mut reports, &mut table);
    let r = w.claim(
        "Claim the second record's invented T3 deposit at leaf 19",
        &cid,
        20,
        &va,
        vault20.witness(&more[1], 20),
    );
    claim_step(r, None, "", &w, &cid, attacker, T3, &mut reports, &mut table);

    // --- The bound (trust.md §2.2, §6 "Supply") --------------------------------------------------
    // Every denomination minted here comes from a leaf the committee invented (index 8 and up),
    // so the whole of it is wrapped supply with no escrow behind it: the shortfall that falls on
    // late redeemers (§6's first row), of which the caps let through at most the following.
    let (eth_minted, t3_minted) = (minted_value(&w, &cid, attacker, ETH), minted_value(&w, &cid, attacker, T3));
    assert_eq!(eth_minted, 4 * CLAIM_VALUE, "four invented ETH denominations minted, and no more");
    assert_eq!(t3_minted, 6 * CLAIM_VALUE, "six invented T3 denominations minted, and no more");
    assert_eq!(w.balance_of(attacker, &cid, &ETH), 4 * DENOMINATIONS[1]);
    assert_eq!(w.balance_of(attacker, &cid, &T3), 6 * DENOMINATIONS[1]);
    assert_eq!(w.balance_of(relayer(), &cid, &ETH), 4 * RELAY_FEES[1]);
    assert_eq!(w.balance_of(relayer(), &cid, &T3), 6 * RELAY_FEES[1]);
    let elapsed_s = u128::from((t_end - t_f) / 1000);
    let bound = |cap: CapParams| cap.capacity + elapsed_s * cap.refill_per_second;
    assert!(eth_minted <= bound(TOKEN_TEST), "§6: per token at most capacity + elapsed × refill before the freeze");
    assert!(t3_minted <= bound(TOKEN_TEST), "§6: per token at most capacity + elapsed × refill before the freeze");
    assert!(
        eth_minted + t3_minted <= bound(GLOBAL_TEST),
        "§6 Supply: wrapped supply exceeds escrow by at most what the caps let through"
    );
    table.push(format!(
        "| The bound | per token at most capacity + elapsed × refill; over both tokens the global \
         capacity's share: over {elapsed_s} s of attested time, {} of the {} global bound ({} ETH, {} T3) |",
        eth_minted + t3_minted,
        bound(GLOBAL_TEST),
        eth_minted,
        t3_minted
    ));

    let steps = table.join("\n");
    let bound_text = format!(
        "Until the freeze authority froze mints, the caps admitted at most `capacity + elapsed × \
         refill` value units per token, and at most the same with the global capacity over all \
         tokens together: {} units per {} s window ({CLAIM_VALUE} per claim). Over {elapsed_s} s of \
         attested time the forger took {} value units of ETH (bound {}), {} of T3 (bound {}), {} \
         in total (bound {}); while the freezes held, every claim was refused with 242; after the \
         30-day freeze of ETH alone, only ETH's claims stopped. A captured committee that keeps its \
         signatures can take the cap per window until the freeze — nothing more.",
        GLOBAL_TEST.capacity,
        WINDOW_S,
        eth_minted,
        bound(TOKEN_TEST),
        t3_minted,
        bound(TOKEN_TEST),
        eth_minted + t3_minted,
        bound(GLOBAL_TEST),
    );
    println!("## What each step minted or was refused\n\n| step | outcome |\n|---|---|\n{steps}\n\n{bound_text}");
    fs::write(
        results_path("captured-committee-exposure.md"),
        format!(
            "# A captured sync committee's exposure, bounded by the consumer's caps and freeze \
             (`harness/tests/captured_committee_exposure.rs`)\n\n\
             Bridge program image `{}`; every transaction settled by LEZ's own functions in a block \
             of its own. RFP-021 Verification Security #4; `docs/trust.md` §2.2 (VS.4) and §6.\n\n\
             ## The fabricated record\n\n\
             A captured committee signs a finality update for a block Ethereum never finalized; the \
             attestation program records it as any other (`World::add_block` places it at \
             `header(instance, slot)` the way the light client writes one after a finality update). \
             Its state trie holds vault A's storage with ten invented deposits appended to the tree \
             (leaves 8-17), each paying the forger 1 ETH; `ImportRoot` accepts the root, and claims \
             of the invented leaves are indistinguishable from honest ones. A second such record \
             (leaves 18-19, stamped a minute after the freeze's) arrives while mints are frozen, and \
             a third stamped two hours ahead of every LEZ block — which only a captured committee \
             could have had written — cannot even move the consumer's clock.\n\n\
             ## What each step minted or was refused\n\n| step | outcome |\n|---|---|\n{steps}\n\n\
             ## The bound\n\n{bound_text}\n\n\
             ## The transactions\n\n{}\n## Handler executions\n\n{}\n",
            harness::image_id_hex(harness::bridge::BRIDGE_ID),
            render(&reports),
            render_transitions(&reports),
        ),
    )
    .unwrap();
}
