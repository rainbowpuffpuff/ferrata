//! The bridge program's controls of milestone B2, through LEZ's state machine on the synthetic
//! vaults of `harness::bridge`, every accepted path and every refusal with its code: transfers
//! between holders; the freeze authority's 14-day handover (propose, wait, apply, cancel) to a
//! multisig (M of N) and a watcher set; the watchers' 12-hour pause; the multisig's freezes and its
//! `Sunset` approval; the keyless `Sunset` of a retired vault; then what the checkpoint program
//! collects of the two `Sunset`s (`checkpoint-core`'s own parser and decoder), and a private
//! holder's `Transfer` (dev mode) with what it makes public. Writes `results/bridge-roles.md` and
//! `results/bridge-transfer-private.md`.

use std::path::PathBuf;

use checkpoint_core::{
    lez::{parse_block, Actor as CpActor, Origin, TxKind, Visitor},
    wire::BridgeMessage as CpMessage,
};
use ferrata_lez_bridge_wire::{
    self as bw, code, config_actor, holder_actor, own_actor, token_actor, BridgeMessage, CapParams, ConfigState,
    CosignedAction, FreezeAuthority, HolderState, Holding, Roles, ROLE_TIMELOCK_MS, SLOT_RETIRED, WATCHER_PAUSE_MS,
};
use harness::{
    attest::render_executed,
    bridge::{
        bridge_program, catalog, eth_entry, eth_schedule_one, private_keys, private_post_state, render, role_account,
        role_key, Block, VaultSlots, World, ADMIN_SEED, DENOMINATIONS, ETH, FREEZE_SEED, LEZ_UNIT, OUTSIDER_SEED,
        TOKEN_CAP, VAULT_B,
    },
    render_transitions, TxReport,
};
use lee::Actor;
use lee_core::{execution_state::BoundaryStep, program::ReceiveInput};

const TARGET_GAS_EXEC: u64 = 5_000_000;
const HOUR_MS: u64 = 3_600_000;
const DAY_S: u64 = 24 * 3600;
const RESERVE: u128 = 100_000_000;

/// Three members of the multisig freeze authority, a watcher, and a holder that receives transfers.
const M1: u8 = 0x41;
const M2: u8 = 0x42;
const M3: u8 = 0x43;
const WATCHER: u8 = 0x44;
const RECIPIENT: u8 = 0x46;

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

fn key(seed: u8) -> [u8; 32] {
    *role_account(seed).value()
}

fn roles(w: &World, cid: &[u8; 32]) -> (bool, Roles, bw::Gate) {
    match w.config_state(cid) {
        Some(ConfigState::Ready { sunset, roles, gate, .. }) => (sunset, roles, gate),
        other => panic!("not a ready configuration: {other:?}"),
    }
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

/// The vault's slot 37 as FerrataVault v5 writes it: its freeze authority in the low 20 bytes and,
/// once `retire()` ran, `retired` in byte 11 (offset 20).
fn slot_37(retired: bool) -> [u8; 32] {
    let mut word = [0u8; 32];
    word[12..].copy_from_slice(&[0xfa; 20]);
    word[bw::RETIRED_BYTE] = u8::from(retired);
    word
}

/// Inits of A and B, their clocks at block 102's record, ETH registered on both, `root(A, 8)` with
/// ETH's schedule 1 closed at 8 leaves, `root(B, 8)`, and the payer's claim of A's leaf 6 (10 ETH).
fn setup(reports: &mut Vec<TxReport>) -> World {
    let mut w = World::new();
    w.add_public_accounts(&[M1, M2, M3, WATCHER, RECIPIENT].map(role_account));
    let (a, b) = (w.config_a.clone(), w.config_b.clone());
    let (cid_a, cid_b, va) = (w.cid_a, w.cid_b, w.vault_a.address);
    for report in [
        w.init("Init A", &a, 0),
        w.init("Init B", &b, 0),
        w.tick("Tick A: block 102's record", &cid_a, None, 2),
        w.tick("Tick B: block 102's record", &cid_b, None, 2),
        w.register("Register ETH (A)", &cid_a, 0, &eth_entry(), &va),
        w.register("Register ETH (B)", &cid_b, 0, &eth_entry(), &VAULT_B),
        w.import("ImportRoot A n=8", &cid_a, 1, w.vault_a.root(8), 8, &va),
        w.import("ImportRoot B n=8", &cid_b, 0, w.vault_b.root(8), 8, &VAULT_B),
        w.import_fees("ImportFees ETH (A): schedule 1", &cid_a, 1, ETH, 1, Some(eth_schedule_one()), &va),
        w.import_fees("ImportFees ETH (A): no schedule 2 at 8 leaves", &cid_a, 1, ETH, 2, None, &va),
    ] {
        run(reports, report, None);
    }
    let leaf6 = w.vault_a.witness(&w.payer_deposit.clone(), 8);
    let r = w.claim("Claim A's leaf 6: 10 ETH to the payer", &cid_a, 8, &va, leaf6);
    run(reports, r, None);
    w
}

#[test]
fn transfers_handover_multisig_watchers_and_keyless_sunset() {
    let mut reports = Vec::new();
    let mut w = setup(&mut reports);
    let (cid_a, cid_b) = (w.cid_a, w.cid_b);
    let program = w.program;
    let payer = w.chain.payer;
    let recipient = role_account(RECIPIENT);
    let (a_actor, b_actor) = (config_actor(program, &cid_a), config_actor(program, &cid_b));
    let eth_a = token_actor(program, &cid_a, &ETH);
    let one = DENOMINATIONS[1];
    assert_eq!(w.balance(payer, &cid_a), DENOMINATIONS[2]);

    // --- Transfers between holders -------------------------------------------------------------
    let r = w.transfer("Transfer 1 ETH (A): the payer to a holder", &cid_a, ETH, one, recipient);
    run(&mut reports, r, None);
    assert_eq!((w.balance(payer, &cid_a), w.balance(recipient, &cid_a)), (DENOMINATIONS[2] - one, one));
    let r = w.transfer("Transfer 0", &cid_a, ETH, 0, recipient);
    run(&mut reports, r, Some(code::BAD_MESSAGE));
    let r = w.transfer("Transfer 1.5 gwei (not a multiple of the LEZ unit)", &cid_a, ETH, LEZ_UNIT * 3 / 2, recipient);
    run(&mut reports, r, Some(code::BAD_MESSAGE));
    let r = w.transfer("Transfer 100 ETH (more than the payer holds)", &cid_a, ETH, 10 * DENOMINATIONS[2], recipient);
    run(&mut reports, r, Some(code::INSUFFICIENT_BALANCE));
    let r = w.transfer("Transfer of a token with no holding (B's ETH)", &cid_b, ETH, one, recipient);
    run(&mut reports, r, Some(code::INSUFFICIENT_BALANCE));
    // The recipient's holding moved by a call the recipient did not sign (only the payer signs).
    let stolen = BridgeMessage::Transfer { config_id: cid_a, token: ETH, amount: one, to: *payer.value() };
    let actors = w.transfer_actors(recipient, &cid_a, &ETH, payer);
    let r = w.send("Transfer from the recipient's actor, not signed by it", actors[0], &stolen, actors.clone());
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    let at_holder = holder_actor(program, recipient);
    let r = w.send("Transfer sent to holder(recipient) itself", at_holder, &stolen, actors.clone());
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    let r = w.send("Transfer sent to token(A, ETH) itself (a credit from nothing)", eth_a, &stolen, actors.clone());
    run(&mut reports, r, Some(code::WRONG_SENDER));
    let credit = BridgeMessage::Credit { config_id: cid_a, token: ETH, amount: one, to: *payer.value() };
    let at_payer = holder_actor(program, payer);
    let r = w.send("Credit sent to holder(payer) itself", at_payer, &credit, vec![at_payer]);
    run(&mut reports, r, Some(code::WRONG_SENDER));
    let tenth = DENOMINATIONS[0];
    let onward = BridgeMessage::Transfer { config_id: cid_a, token: ETH, amount: tenth, to: *payer.value() };
    let rkey = role_key(RECIPIENT);
    let r = w.submit_signed(
        "Transfer 0.1 ETH (A): the holder, signed, to the payer",
        actors[0],
        &onward,
        actors,
        vec![],
        &[&rkey],
    );
    run(&mut reports, r, None);
    assert_eq!(w.balance(recipient, &cid_a), one - tenth);
    assert_eq!(w.balance(payer, &cid_a), DENOMINATIONS[2] - one + tenth);

    // --- The handover: propose, wait 14 days, apply; cancel ------------------------------------
    let t0 = w.blocks[2].time_ms();
    let multisig = FreezeAuthority { members: vec![key(M1), key(M2), key(M3)], threshold: 2 };
    let propose = |fa: FreezeAuthority, watchers: Vec<[u8; 32]>, w: &World| BridgeMessage::ProposeRoles {
        config_id: cid_a,
        freeze_authority: fa,
        watchers,
        record: w.blocks[2].time_record(),
    };
    let good = propose(multisig.clone(), vec![key(WATCHER)], &w);
    let r = w.role("ProposeRoles by a signer with no role", OUTSIDER_SEED, &good, a_actor);
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    let r = w.role("ProposeRoles by the freeze authority", FREEZE_SEED, &good, a_actor);
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    let r = w.send("ProposeRoles sent to config(A) itself", a_actor, &good, vec![a_actor]);
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    for (label, fa, watchers) in [
        ("ProposeRoles: threshold 0", FreezeAuthority { threshold: 0, ..multisig.clone() }, vec![]),
        ("ProposeRoles: threshold 4 of 3", FreezeAuthority { threshold: 4, ..multisig.clone() }, vec![]),
        (
            "ProposeRoles: the admin as a member",
            FreezeAuthority { members: vec![key(M1), key(ADMIN_SEED)], threshold: 2 },
            vec![],
        ),
        ("ProposeRoles: a member twice", FreezeAuthority { members: vec![key(M1), key(M1)], threshold: 2 }, vec![]),
        ("ProposeRoles: four watchers", multisig.clone(), vec![key(WATCHER), key(M1), key(M2), key(M3)]),
    ] {
        let r = w.role(label, ADMIN_SEED, &propose(fa, watchers, &w), a_actor);
        run(&mut reports, r, Some(code::BAD_MESSAGE));
    }
    let r = w.apply_roles("ApplyRoles with nothing proposed", &cid_a, 2);
    run(&mut reports, r, Some(code::NOT_FOUND));
    let cancel = BridgeMessage::CancelRoles { config_id: cid_a };
    let r = w.role("CancelRoles with nothing proposed", ADMIN_SEED, &cancel, a_actor);
    run(&mut reports, r, Some(code::NOT_FOUND));
    let r = w.role("ProposeRoles by the admin: 2 of 3, one watcher (block 102's record)", ADMIN_SEED, &good, a_actor);
    run(&mut reports, r, None);
    let (_, live, _) = roles(&w, &cid_a);
    assert_eq!(live.proposal.as_ref().map(|p| p.effective_ms), Some(t0 + ROLE_TIMELOCK_MS));
    assert_eq!(live.freeze_authority, FreezeAuthority::key(key(FREEZE_SEED)), "nothing changes before it applies");
    let r = w.role("CancelRoles by the freeze authority", FREEZE_SEED, &cancel, a_actor);
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    let r = w.role("CancelRoles by the admin", ADMIN_SEED, &cancel, a_actor);
    run(&mut reports, r, None);
    assert!(roles(&w, &cid_a).1.proposal.is_none());
    let r = w.role("ProposeRoles by the admin again", ADMIN_SEED, &good, a_actor);
    run(&mut reports, r, None);
    let r = w.apply_roles("ApplyRoles at the proposal's own record", &cid_a, 2);
    run(&mut reports, r, Some(code::FROZEN));
    let pause_a = |w: &World, token: Option<[u8; 20]>, mints: bool, burns: bool, block: usize| BridgeMessage::Pause {
        config_id: cid_a,
        token,
        mints,
        burns,
        record: w.blocks[block].time_record(),
    };
    let p = pause_a(&w, None, true, true, 2);
    let r = w.role("Pause by the proposed watcher, before the handover", WATCHER, &p, a_actor);
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    let day13 = w.finalize(t0 / 1000 + 13 * DAY_S);
    let r = w.apply_roles("ApplyRoles with a record 13 days later", &cid_a, day13);
    run(&mut reports, r, Some(code::FROZEN));
    let day14 = w.finalize(t0 / 1000 + 14 * DAY_S);
    let r = w.apply_roles("ApplyRoles with a record 14 days later (anyone)", &cid_a, day14);
    run(&mut reports, r, None);
    let (_, live, _) = roles(&w, &cid_a);
    assert_eq!(
        (live.freeze_authority.clone(), live.watchers.clone(), live.proposal.clone()),
        (multisig.clone(), vec![key(WATCHER)], None)
    );
    let r = w.apply_roles("ApplyRoles again", &cid_a, day14);
    run(&mut reports, r, Some(code::NOT_FOUND));

    // --- The multisig freeze authority -----------------------------------------------------------
    let t14 = w.blocks[day14].time_ms();
    let freeze = |token: Option<[u8; 20]>, mints: bool, burns: bool, until_ms: u64, w: &World| CosignedAction::Freeze {
        token,
        mints,
        burns,
        until_ms,
        record: w.blocks[day14].time_record(),
    };
    let old = BridgeMessage::Freeze {
        config_id: cid_a,
        token: None,
        mints: true,
        burns: false,
        until_ms: t14 + HOUR_MS,
        record: w.blocks[day14].time_record(),
    };
    let r = w.role("Freeze by the replaced freeze authority", FREEZE_SEED, &old, a_actor);
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    let r = w.role("Freeze by one member alone (2 of 3)", M1, &old, a_actor);
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    let f = freeze(None, true, false, t14 + HOUR_MS, &w);
    let r = w.cosign("Cosigned freeze by one member", &[M1], &[M1], &cid_a, f.clone());
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    let r = w.cosign("Cosigned freeze naming M1, M2, signed by M1 only", &[M1, M2], &[M1], &cid_a, f.clone());
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    let r = w.cosign(
        "Cosigned freeze by M1 and a non-member",
        &[M1, OUTSIDER_SEED],
        &[M1, OUTSIDER_SEED],
        &cid_a,
        f.clone(),
    );
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    let r = w.cosign("Cosigned freeze of A's mints for an hour by M1 and M2", &[M1, M2], &[M1, M2], &cid_a, f);
    run(&mut reports, r, None);
    assert_eq!(roles(&w, &cid_a).2.mints_until, t14 + HOUR_MS);
    let f = freeze(Some(ETH), false, true, t14 + HOUR_MS, &w);
    let r = w.cosign("Cosigned freeze of ETH (A) burns by M3 and M2", &[M3, M2], &[M3, M2], &cid_a, f);
    run(&mut reports, r, None);
    assert_eq!(w.token_state(&cid_a, &ETH).unwrap().gate.burns_until, t14 + HOUR_MS);
    let f = freeze(None, true, false, t14 + 31 * DAY_S * 1000, &w);
    let r = w.cosign("Cosigned freeze for 31 days", &[M1, M2], &[M1, M2], &cid_a, f);
    run(&mut reports, r, Some(code::BAD_MESSAGE));

    // --- Watchers: 12 hours, never shorter, never lifted by a watcher -----------------------------
    let p = pause_a(&w, None, true, true, day14);
    let r = w.role("Pause by a signer with no role", OUTSIDER_SEED, &p, a_actor);
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    let r = w.role("Pause by a member of the freeze authority", M1, &p, a_actor);
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    let r = w.role("Pause of neither mints nor burns", WATCHER, &pause_a(&w, None, false, false, day14), a_actor);
    run(&mut reports, r, Some(code::BAD_MESSAGE));
    let r = w.role("Pause A's mints and burns (the watcher)", WATCHER, &p, a_actor);
    run(&mut reports, r, None);
    let gate = roles(&w, &cid_a).2;
    assert_eq!((gate.mints_until, gate.burns_until), (t14 + WATCHER_PAUSE_MS, t14 + WATCHER_PAUSE_MS));
    let r = w.role("Pause ETH (A) mints (the watcher)", WATCHER, &pause_a(&w, Some(ETH), true, false, day14), eth_a);
    run(&mut reports, r, None);
    let token = w.token_state(&cid_a, &ETH).unwrap().gate;
    assert_eq!((token.mints_until, token.burns_until), (t14 + WATCHER_PAUSE_MS, t14 + HOUR_MS));
    let f = freeze(None, true, false, t14 + 2 * DAY_S * 1000, &w);
    let r = w.cosign("Cosigned freeze of A's mints for two days by M1 and M3", &[M1, M3], &[M1, M3], &cid_a, f);
    run(&mut reports, r, None);
    let r = w.role("Pause A's mints again (the watcher)", WATCHER, &p, a_actor);
    run(&mut reports, r, None);
    assert_eq!(roles(&w, &cid_a).2.mints_until, t14 + 2 * DAY_S * 1000, "a pause never shortens a freeze");
    let unfreeze = BridgeMessage::Freeze {
        config_id: cid_a,
        token: None,
        mints: true,
        burns: true,
        until_ms: 0,
        record: w.blocks[day14].time_record(),
    };
    let r = w.role("Unfreeze by the watcher", WATCHER, &unfreeze, a_actor);
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    let f = freeze(None, true, true, 0, &w);
    let r = w.cosign("Cosigned unfreeze of A by M2 and M3", &[M2, M3], &[M2, M3], &cid_a, f);
    run(&mut reports, r, None);
    assert_eq!((roles(&w, &cid_a).2.mints_until, roles(&w, &cid_a).2.burns_until), (0, 0));
    // The admin's token cap now reaches the token through config(A), as every role message does.
    let cap = BridgeMessage::SetCap { config_id: cid_a, token: Some(ETH), mints: true, cap: TOKEN_CAP };
    let r = w.role("SetCap ETH (A) by the admin, through config(A)", ADMIN_SEED, &cap, eth_a);
    run(&mut reports, r, None);
    let low = BridgeMessage::SetCap {
        config_id: cid_a,
        token: Some(ETH),
        mints: true,
        cap: CapParams { capacity: 1, refill_per_second: 1 },
    };
    let r = w.role("SetCap ETH (A) below the floor", ADMIN_SEED, &low, eth_a);
    run(&mut reports, r, Some(code::BAD_MESSAGE));

    // --- Sunset of A: the multisig approves, anyone sends the checkpoint's own message -----------
    let sunset_a = BridgeMessage::Sunset { config_id: cid_a };
    let r = w.send("Sunset sent to config(A) by anyone, not approved", a_actor, &sunset_a, vec![a_actor]);
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    let r = w.role("Sunset A by the replaced freeze authority", FREEZE_SEED, &sunset_a, a_actor);
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    let r = w.role("Sunset A by one member alone", M1, &sunset_a, a_actor);
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    let r = w.cosign("Cosigned Sunset by one member", &[M2], &[M2], &cid_a, CosignedAction::Sunset);
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    let r = w.cosign("Cosigned Sunset approval by M1 and M2", &[M1, M2], &[M1, M2], &cid_a, CosignedAction::Sunset);
    run(&mut reports, r, None);
    let (sunset, live, _) = roles(&w, &cid_a);
    assert!(!sunset && live.sunset_approved, "approved, not sunset: the checkpoint has seen no Sunset yet");
    let r = w.cosign("Cosigned Sunset approval again", &[M2, M3], &[M2, M3], &cid_a, CosignedAction::Sunset);
    run(&mut reports, r, Some(code::ALREADY_WRITTEN));
    let r = w.send("Sunset A sent to config(A) by anyone, approved", a_actor, &sunset_a, vec![a_actor]);
    run(&mut reports, r, None);
    assert!(roles(&w, &cid_a).0);
    let r = w.send("Sunset A again", a_actor, &sunset_a, vec![a_actor]);
    run(&mut reports, r, Some(code::ALREADY_WRITTEN));
    let va = w.vault_a.address;
    let r = w.import("ImportRoot A after Sunset", &cid_a, 1, w.vault_a.root(8), 8, &va);
    run(&mut reports, r, Some(code::FROZEN));
    let r = w.transfer("Transfer 1 ETH (A) after Sunset: balances still move", &cid_a, ETH, one, recipient);
    run(&mut reports, r, None);

    // --- Keyless Sunset of B: the vault is attested retired ---------------------------------------
    let sunset_b = BridgeMessage::Sunset { config_id: cid_b };
    let r = w.send("Sunset sent to config(B) by anyone, vault not retired", b_actor, &sunset_b, vec![b_actor]);
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    let vault_b_at = |w: &World, retired: bool| VaultSlots {
        address: VAULT_B,
        root: w.vault_b.root(8),
        count: 8,
        config_id: cid_b,
        extra: vec![(SLOT_RETIRED, slot_37(retired))],
    };
    let now_s = w.newest_record_seconds();
    let (slot, number) = (w.next_slot(), 2_000);
    let live_vault = w.add_block(Block::new(slot, number, &[vault_b_at(&w, false)]).at(now_s + 12));
    let r = w.attest_retired("AttestRetired B under block 100 (slot 37 empty)", &cid_b, 0, &VAULT_B);
    run(&mut reports, r, Some(code::IMPORT_MISMATCH));
    let r = w.attest_retired("AttestRetired B: a freeze authority, not retired", &cid_b, live_vault, &VAULT_B);
    run(&mut reports, r, Some(code::IMPORT_MISMATCH));
    let (slot, number) = (w.next_slot(), 2_001);
    let retired = w.add_block(Block::new(slot, number, &[vault_b_at(&w, true)]).at(now_s + 24));
    let r = w.attest_retired("AttestRetired B: retired (anyone)", &cid_b, retired, &VAULT_B);
    run(&mut reports, r, None);
    assert!(roles(&w, &cid_b).1.vault_retired);
    let r = w.attest_retired("AttestRetired B again", &cid_b, retired, &VAULT_B);
    run(&mut reports, r, Some(code::ALREADY_WRITTEN));
    let r = w.role(
        "Sunset B forwarded by a signer's own actor (not top-level at config(B))",
        OUTSIDER_SEED,
        &sunset_b,
        b_actor,
    );
    run(&mut reports, r, Some(code::NOT_AUTHORIZED));
    let r = w.send("Sunset B sent to config(B) by anyone, retired", b_actor, &sunset_b, vec![b_actor]);
    run(&mut reports, r, None);
    assert!(roles(&w, &cid_b).0);
    let r = w.send("Sunset B again", b_actor, &sunset_b, vec![b_actor]);
    run(&mut reports, r, Some(code::ALREADY_WRITTEN));
    let r = w.import("ImportRoot B after Sunset", &cid_b, 0, w.vault_b.root(8), 8, &VAULT_B);
    run(&mut reports, r, Some(code::FROZEN));

    // --- What the checkpoint collects -------------------------------------------------------------
    let tx = |label: &str| -> Vec<u8> {
        w.txs.iter().find(|(l, _)| l == label).unwrap_or_else(|| panic!("no transaction {label}")).1.clone()
    };
    let labels = [
        ("Cosigned Sunset approval by M1 and M2", false),
        ("Sunset sent to config(A) by anyone, not approved", true),
        ("Sunset A sent to config(A) by anyone, approved", false),
        ("Sunset A again", true),
        ("AttestRetired B: retired (anyone)", false),
        ("Sunset B forwarded by a signer's own actor (not top-level at config(B))", true),
        ("Sunset B sent to config(B) by anyone, retired", false),
        ("Transfer 1 ETH (A): the payer to a holder", false),
    ];
    let rows: Vec<(&str, Vec<u8>, bool)> = labels.iter().map(|(l, rev)| (*l, tx(l), *rev)).collect();
    let txs: Vec<&[u8]> = rows.iter().map(|(_, b, _)| b.as_slice()).collect();
    let reverted: Vec<u64> = rows.iter().enumerate().filter(|(_, r)| r.2).map(|(i, _)| i as u64).collect();
    let bytes = block(&txs, &reverted);
    let mut visitor = Calls { bridge: *program.value(), calls: Vec::new() };
    let summary = parse_block(&bytes, &mut visitor).expect("the checkpoint parses the block");
    let mut table = String::from(
        "| transaction | origin | message bytes | checkpoint's decoder | collected |\n|---|---|---:|---|---|\n",
    );
    let mut sunsets = Vec::new();
    for (index, origin, message) in &visitor.calls {
        let decoded = CpMessage::decode(message);
        let collected = origin.is_public_execution() && !summary.is_reverted(*index);
        let what = match &decoded {
            Ok(Some(CpMessage::Sunset { config_id })) => {
                if collected {
                    sunsets.push(*config_id);
                }
                "Sunset"
            }
            Ok(Some(CpMessage::Burn(_))) => "Burn",
            Ok(Some(CpMessage::ImportRoot(_))) => "ImportRoot",
            Ok(Some(CpMessage::Echo { .. })) => "Echo",
            Ok(None) => "ignored (variant 4 and up)",
            Err(_) => panic!("a public bridge message the checkpoint cannot decode"),
        };
        let status = if summary.is_reverted(*index) {
            "no (reverted)"
        } else if collected && what == "Sunset" {
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
    assert_eq!(sunsets, vec![cid_a, cid_b], "exactly the two executed Sunsets, each the checkpoint's 33 bytes");
    assert_eq!(CpMessage::Sunset { config_id: cid_a }.encode(), borsh::to_vec(&sunset_a).unwrap());

    let table_reports = render(&reports);
    std::fs::write(
        results_path("bridge-roles.md"),
        format!(
            "# Transfers, the handover, the multisig, watchers and the keyless Sunset (`harness/tests/bridge_roles.rs`)\n\n\
             Bridge program image `{}`; every transaction settled by LEZ's own functions in a block of its own.\n\n\
             {table_reports}\n## Handler executions\n\n{}\n\
             ## What the checkpoint program collects (`checkpoint-core`'s `parse_block` and `BridgeMessage::decode`)\n\n\
             One LEZ block holding the transactions below (the refused ones marked reverted, as settlement recorded them):\n\n{table}",
            harness::image_id_hex(methods::FERRATA_BRIDGE_ID),
            render_transitions(&reports),
        ),
    )
    .unwrap();
}

/// A private holder's `Transfer`: the private claim of A's leaf 5 (1 ETH) by its private
/// destination, then that holder moves 0.1 ETH to a public holder, in a private execution (dev
/// mode). The boundary shows the delivery to the token (configuration, token, amount, recipient)
/// from an alias: not the private account, its claim or its balance.
#[test]
fn a_private_holder_transfers() {
    assert_eq!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1"), "run these tests with RISC0_DEV_MODE=1");
    let mut w = World::with_roots();
    let (cid_a, vault_a, program) = (w.cid_a, w.vault_a.address, w.program);
    let recipient = role_account(RECIPIENT);
    let seed = w.private_seed;
    let account = w.chain.add_private_account(seed, 1_000_000_000_000);
    let deposit = w.private_deposit.clone();
    let claim_message = w.claim_privately(&cid_a, 8, w.vault_a.witness(&deposit, 8));
    let claim_publics = w.private_claim_actors(&cid_a, 8, &vault_a, &deposit);
    let me = own_actor(program, account.id());
    w.chain
        .prove_and_settle_private(
            &account,
            me,
            borsh::to_vec(&claim_message).unwrap(),
            claim_publics,
            &catalog(),
            RESERVE,
            w.time,
        )
        .expect("the private claim settles");
    let holding = HolderState {
        holdings: vec![Holding { config_id: cid_a, token: ETH, amount: DENOMINATIONS[1] }],
        pending: None,
    };
    let claimed = private_post_state(&w.chain, &private_keys(seed), &account.account, RESERVE, &holding)
        .expect("the claimed account's commitment");

    let tenth = DENOMINATIONS[0];
    let message = BridgeMessage::Transfer { config_id: cid_a, token: ETH, amount: tenth, to: *recipient.value() };
    // The executor alone: the root call at the private holder (what a prover proves for the bridge).
    let call = ReceiveInput {
        receiver: me,
        from: None,
        is_authorized: true,
        pre_state: claimed.data.actor_state(program).clone(),
        message: borsh::to_vec(&message).unwrap(),
    };
    let executed = harness::attest::execute(
        "Transfer at the private holder (debits, sends Transfer)",
        bridge_program().elf(),
        &call,
    );
    assert_eq!(executed.exit_code, 0);
    let executed_table = render_executed(&[executed]);
    let token_a = token_actor(program, &cid_a, &ETH);
    let publics: Vec<Actor> = vec![token_a, holder_actor(program, recipient)];
    let holder_p = harness::private::PrivateAccount { keys: private_keys(seed), account: claimed.clone() };
    let settled = w
        .chain
        .prove_and_settle_private(&holder_p, me, borsh::to_vec(&message).unwrap(), publics, &catalog(), RESERVE, w.time)
        .expect("the private transfer settles");
    assert!(settled.charge.revert.is_none());
    let outgoing: Vec<_> = settled
        .tx
        .message
        .execution
        .boundary
        .iter()
        .filter_map(|step| match step {
            BoundaryStep::PrivateToPublic(d) => Some((d.envelope.to, d.envelope.from, d.envelope.message.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(outgoing[0].0, token_a, "the transfer's one public delivery goes to the token: {outgoing:?}");
    assert_eq!(outgoing[0].2, borsh::to_vec(&message).unwrap());
    assert_ne!(outgoing[0].1.account_id, account.id(), "the holder appears as an alias");
    let tx_bytes = borsh::to_vec(&settled.tx).unwrap();
    let id = account.id();
    assert!(!tx_bytes.windows(32).any(|win| win == id.value()), "the private account's id is not in the transaction");
    assert_eq!(w.balance(recipient, &cid_a), tenth, "credited at holder(recipient)");
    let left = HolderState {
        holdings: vec![Holding { config_id: cid_a, token: ETH, amount: DENOMINATIONS[1] - tenth }],
        pending: None,
    };
    private_post_state(&w.chain, &private_keys(seed), &claimed, RESERVE, &left).expect("0.9 ETH left, privately");
    assert!(matches!(w.holder(recipient), Some(HolderState { pending: None, .. })));
    let boundary: String = outgoing
        .iter()
        .map(|(to, from, m)| {
            format!(
                "| {} | alias {} | {} bytes |\n",
                hex::encode(to.account_id.value()),
                hex::encode(&from.account_id.value()[..4]),
                m.len()
            )
        })
        .collect();
    std::fs::write(
        results_path("bridge-transfer-private.md"),
        format!(
            "# A private holder's Transfer (`harness/tests/bridge_roles.rs`, dev mode)\n\n\
             Bridge program image `{}`. The holder of the private claim of A's leaf 5 (1 ETH) moves 0.1 ETH to a public \
             holder's `holder(account)`; settled through LEZ's private settlement (dev-mode receipt): {} transaction bytes, \
             execution gas {}, storage gas {}.\n\n## The private transition\n\n{executed_table}\n\
             ## Private to public deliveries (what the transaction shows)\n\n| to | from | message |\n|---|---|---:|\n{boundary}\n\
             The private account's id does not occur in the transaction's bytes; its remaining 0.9 ETH is found only in its new commitment.\n",
            harness::image_id_hex(methods::FERRATA_BRIDGE_ID),
            tx_bytes.len(),
            settled.charge.gas_exec,
            settled.charge.gas_stor,
        ),
    )
    .unwrap();
}
