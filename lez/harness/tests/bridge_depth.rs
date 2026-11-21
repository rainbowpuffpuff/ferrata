//! The inbound finality depth (RFP-022 F.12): "The finality depth required before a deposit may be claimed ... is
//! configurable by the admin authority per deployment, defaulting to the depth recommended ... A change to the
//! configured depth must not invalidate a claim or release that was already valid under the previous depth."
//!
//! Inbound, a deposit is claimable once a root holding it is imported. `ImportRoot` reads the vault under a record of
//! the configuration's attestation instance, and every record is of a finalized header: depth 0, the default, is
//! Ethereum finality (the depth the RFP's Design Rationale recommends). The admin (and no one else) sets a deeper
//! depth in beacon slots (`SetDepth`, at most 216,000): an import under the record at slot `s` then waits (242
//! `FROZEN`) until the configuration's clock has read a record at slot `s + depth` or later (`Tick`, or any message
//! that reads a record at `config(cid)`). A change applies to imports from then on; roots already imported stay
//! imported, so a claim that was valid stays valid. (The outbound depth is the vault's, `Checkpoint.t.sol` and
//! `ReleaseDelay.t.sol`.)
//!
//! Re-run (in `lez/`): `RISC0_DEV_MODE=1 cargo test -p harness --locked --test bridge_depth -- --nocapture`. Writes
//! `lez/results/bridge-depth.md`.

use std::{fs, path::PathBuf};

use ferrata_lez_bridge_wire::{code, config_actor, BridgeMessage, ConfigState, MAX_INBOUND_DEPTH_SLOTS};
use harness::{
    bridge::{
        eth_entry, eth_schedule_one, render, World, ADMIN_SEED, DENOMINATIONS, ETH, FREEZE_SEED, OUTSIDER_SEED,
    },
    render_transitions, TxReport,
};

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
    reports.push(report);
}

/// `(inbound_depth_slots, tip_slot)` of `config(cid)`.
fn depth(w: &World, cid: &[u8; 32]) -> (u64, u64) {
    match w.config_state(cid) {
        Some(ConfigState::Ready { roles, .. }) => (roles.inbound_depth_slots, roles.tip_slot),
        other => panic!("not a ready configuration: {other:?}"),
    }
}

#[test]
fn the_admin_sets_the_inbound_depth_and_no_valid_claim_is_lost() {
    assert_eq!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1"), "run with RISC0_DEV_MODE=1");
    let mut w = World::new();
    let (program, cid_a, va) = (w.program, w.cid_a, w.vault_a.address);
    let a = w.config_a.clone();
    let at = config_actor(program, &cid_a);
    let mut reports = Vec::new();
    let (s100, s101, s102) = (w.blocks[0].slot, w.blocks[1].slot, w.blocks[2].slot);
    assert_eq!((s100, s101, s102), (9_000_000, 9_000_032, 9_000_064));

    run(&mut reports, w.init("Init A", &a, 0), None);
    run(&mut reports, w.register("Register ETH (block 100)", &cid_a, 0, &eth_entry(), &va), None);
    assert_eq!(depth(&w, &cid_a), (0, 0), "Init: depth 0 (Ethereum finality); no record read yet");

    // Depth 0: an import under any finalized record, with no record read by the clock.
    let r = w.import("ImportRoot root(A, 5) under block 100 (slot 9,000,000), depth 0", &cid_a, 0, w.vault_a.root(5), 5, &va);
    run(&mut reports, r, None);

    // Only the admin sets the depth, within its bound.
    let set = |slots: u64| BridgeMessage::SetDepth { config_id: cid_a, slots };
    run(&mut reports, w.role("SetDepth 65 by the freeze authority", FREEZE_SEED, &set(65), at), Some(code::NOT_AUTHORIZED));
    run(&mut reports, w.role("SetDepth 65 by an outsider", OUTSIDER_SEED, &set(65), at), Some(code::NOT_AUTHORIZED));
    let r = w.role("SetDepth 216,001 by the admin", ADMIN_SEED, &set(MAX_INBOUND_DEPTH_SLOTS + 1), at);
    run(&mut reports, r, Some(code::BAD_MESSAGE));
    run(&mut reports, w.role("SetDepth 65 by the admin", ADMIN_SEED, &set(65), at), None);
    assert_eq!(depth(&w, &cid_a), (65, 0));

    // root(A, 8) lies under block 101 (slot 9,000,032): at depth 65 it needs a record at slot 9,000,097 or later.
    let root8 = w.vault_a.root(8);
    let label = "ImportRoot root(A, 8) under block 101 (slot 9,000,032), depth 65, no record read";
    run(&mut reports, w.import(label, &cid_a, 1, root8, 8, &va), Some(code::FROZEN));
    assert!(w.root_state(&cid_a, 8).is_none(), "a held import writes nothing");
    run(&mut reports, w.tick("Tick A: block 102 (slot 9,000,064)", &cid_a, None, 2), None);
    assert_eq!(depth(&w, &cid_a), (65, s102), "the clock's record is the tip");
    let label = "ImportRoot root(A, 8) under block 101: 32 slots behind the tip, 65 needed";
    run(&mut reports, w.import(label, &cid_a, 1, root8, 8, &va), Some(code::FROZEN));
    let later = w.newest_record_seconds() + 32 * 12;
    let b = w.finalize(later);
    assert_eq!(w.blocks[b].slot, 9_000_096);
    run(&mut reports, w.tick("Tick A: a record at slot 9,000,096", &cid_a, None, b), None);
    assert_eq!(depth(&w, &cid_a).1, 9_000_096);
    let label = "ImportRoot root(A, 8) under block 101: 64 slots behind the tip, 65 needed (one short)";
    run(&mut reports, w.import(label, &cid_a, 1, root8, 8, &va), Some(code::FROZEN));
    // Lowering the depth delays nothing: the same import applies at once.
    run(&mut reports, w.role("SetDepth 64 by the admin", ADMIN_SEED, &set(64), at), None);
    let label = "ImportRoot root(A, 8) under block 101: 64 slots behind the tip, 64 needed";
    run(&mut reports, w.import(label, &cid_a, 1, root8, 8, &va), None);
    assert!(w.root_state(&cid_a, 8).is_some());
    // The fee schedules for leaves 5 to 7 (ImportFees reads the vault too, but the depth is the imports' only).
    let fees = w.import_fees("ImportFees ETH schedule 1 (block 101)", &cid_a, 1, ETH, 1, Some(eth_schedule_one()), &va);
    run(&mut reports, fees, None);
    run(&mut reports, w.import_fees("ImportFees ETH, no schedule 2 (block 101)", &cid_a, 1, ETH, 2, None, &va), None);

    // A change never invalidates: at the maximum depth, claims against both imported roots apply.
    let r = w.role("SetDepth 216,000 (the maximum) by the admin", ADMIN_SEED, &set(MAX_INBOUND_DEPTH_SLOTS), at);
    run(&mut reports, r, None);
    let vector = w.vector.clone();
    let witness = w.vault_a.witness(&vector, 5);
    let label = "Claim of leaf 2 under root(A, 5) (imported at depth 0), depth now 216,000";
    run(&mut reports, w.claim(label, &cid_a, 5, &va, witness), None);
    let payer = w.chain.payer;
    let witness = w.vault_a.witness(&w.payer_deposit.clone(), 8);
    let label = "Claim of leaf 6 under root(A, 8) (imported at depth 64), depth now 216,000";
    run(&mut reports, w.claim(label, &cid_a, 8, &va, witness), None);
    assert_eq!(w.balance(payer, &cid_a), DENOMINATIONS[2]);
    // A new import is held to the new depth, refused before anything is attested.
    let label = "ImportRoot under block 102 (slot 9,000,064) at depth 216,000";
    run(&mut reports, w.import(label, &cid_a, 2, root8, 6, &va), Some(code::FROZEN));
    run(&mut reports, w.role("SetDepth 0 by the admin", ADMIN_SEED, &set(0), at), None);
    assert_eq!(depth(&w, &cid_a), (0, 9_000_096));
    // At depth 0 the same import applies at once (block 102 holds the root of 8 leaves with the count 6).
    let label = "the same ImportRoot at depth 0";
    run(&mut reports, w.import(label, &cid_a, 2, root8, 6, &va), None);
    assert!(w.root_state(&cid_a, 6).is_some());

    let table = render(&reports);
    let transitions = render_transitions(&reports);
    println!("{table}");
    fs::write(
        results_path("bridge-depth.md"),
        format!(
            "# The inbound finality depth (`harness/tests/bridge_depth.rs`, RFP-022 F.12)\n\n\
             Bridge program image `{}`; LEZ v0.4.0-rc1's state machine, every transaction settled in a block of its \
             own (RISC Zero dev mode). Depth 0 (the default) is Ethereum finality; the admin's depth is in beacon slots \
             beyond it, measured against the newest record the configuration's clock read.\n\n{table}\n\
             ## Handler executions\n\n{transitions}",
            harness::image_id_hex(harness::bridge::BRIDGE_ID)
        ),
    )
    .unwrap();
}
