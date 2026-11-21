//! Delisting on LEZ (RFP-022 F.10): "An admin authority ... can register a supported ERC-20 ... and deregister a
//! token", with "minimum impact for users, including delisting of a token".
//!
//! The registry's source is the vault: the admin delists a token there (`setDepositsEnabled(token, false)`, which
//! stops its deposits and writes 1 at the SHA-256 slot `sha256("ferrata/vault/listing/v1" ‖ token)`; relisting writes
//! 0). On LEZ, anyone attests that slot (`AttestListing`) and `token(cid, token)` then holds whether the token is
//! delisted, and from which record: the LEZ registry follows the vault's, so a LEZ-side client (the bridge module,
//! the mini-app, the CLI) shows the delisting and offers no deposit of the token. The impact on users is nil by
//! construction, and this test asserts it: claims of deposits made before the delisting, burns and transfers of the
//! token go on after it (R.4: a valid claim stays valid), and an older record cannot undo a delisting or a relisting.
//!
//! Re-run (in `lez/`): `RISC0_DEV_MODE=1 cargo test -p harness --locked --test bridge_delist -- --nocapture`. Writes
//! `lez/results/bridge-delist.md` (the records of this branch: `lez/results/b1/`, with
//! `FERRATA_LEZ_RESULTS_DIR`; `lez/results/b1/README.md`).

use std::{fs, path::PathBuf};

use ferrata_lez_bridge_wire::{code, listing_slot, own_actor, word_of, word_of_u128, BridgeMessage, Note};
use harness::{
    bridge::{eth_entry, eth_schedule_one, render, Block, VaultSlots, World, DENOMINATIONS, ETH},
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

/// A later block holding vault A's storage with the listing slot of ETH at `word` (`None`: never written, 0), its
/// record placed as the attestation program writes it, at an Ethereum time behind the world's LEZ clock.
fn vault_a_block(w: &mut World, word: Option<u64>) -> usize {
    let slot = w.next_slot();
    let number = 1_100 + w.blocks.len() as u64;
    let seconds = w.newest_record_seconds() + 12;
    let extra = word.map(|v| vec![(listing_slot(&ETH), word_of(v))]).unwrap_or_default();
    let slots = VaultSlots { address: w.vault_a.address, root: w.vault_a.root(8), count: 8, config_id: w.cid_a, extra };
    w.add_block(Block::new(slot, number, &[slots]).at(seconds))
}

#[test]
fn delisting_is_mirrored_on_lez_and_takes_no_claim_or_holding() {
    assert_eq!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1"), "run with RISC0_DEV_MODE=1");
    let mut w = World::new();
    let (program, cid_a, va, payer) = (w.program, w.cid_a, w.vault_a.address, w.chain.payer);
    let a = w.config_a.clone();
    let mut reports = Vec::new();
    run(&mut reports, w.init("Init A", &a, 0), None);
    run(&mut reports, w.register("Register ETH (block 100)", &cid_a, 0, &eth_entry(), &va), None);
    run(&mut reports, w.import("ImportRoot root(A, 8) (block 101)", &cid_a, 1, w.vault_a.root(8), 8, &va), None);
    let fees = w.import_fees("ImportFees ETH schedule 1", &cid_a, 1, ETH, 1, Some(eth_schedule_one()), &va);
    run(&mut reports, fees, None);
    run(&mut reports, w.import_fees("ImportFees ETH, no schedule 2", &cid_a, 1, ETH, 2, None, &va), None);
    let token = |w: &World| w.token_state(&cid_a, &ETH).expect("token(A, ETH)");
    assert_eq!((token(&w).delisted, token(&w).listing_slot), (false, w.blocks[0].slot), "listed from registration");

    // The payer's deposit (leaf 6, 10 ETH) is claimed before the delisting.
    let witness = w.vault_a.witness(&w.payer_deposit.clone(), 8);
    run(
        &mut reports,
        w.claim("Claim of leaf 6 (10 ETH to the payer), before the delisting", &cid_a, 8, &va, witness),
        None,
    );

    // The vault delists ETH: the listing slot holds 1.
    let delisted = vault_a_block(&mut w, Some(1));
    let slot = w.blocks[delisted].slot;
    run(
        &mut reports,
        w.attest_listing("AttestListing ETH: the vault's slot holds 1", &cid_a, ETH, delisted, &va),
        None,
    );
    assert_eq!((token(&w).delisted, token(&w).listing_slot), (true, slot), "delisted, from that record");
    // Neither an older record (block 101: the slot never written) nor the same one again undoes it.
    let r = w.attest_listing("AttestListing ETH under block 101 (older, slot unwritten)", &cid_a, ETH, 1, &va);
    run(&mut reports, r, Some(code::IMPORT_MISMATCH));
    let r = w.attest_listing("AttestListing ETH under the same record again", &cid_a, ETH, delisted, &va);
    run(&mut reports, r, Some(code::IMPORT_MISMATCH));
    assert!(token(&w).delisted);

    // No user loses anything: a deposit made before the delisting is claimed, and holders burn and transfer.
    let vector = w.vector.clone();
    let witness = w.vault_a.witness(&vector, 8);
    run(&mut reports, w.claim("Claim of leaf 2 (1 ETH), after the delisting", &cid_a, 8, &va, witness), None);
    assert_eq!(w.balance(w.vector_dest, &cid_a), DENOMINATIONS[1]);
    let note = Note {
        r: [0x6e; 32],
        memo: [0x6d; 20],
        mac: [0x6c; 16],
        token: ETH,
        amount: word_of_u128(DENOMINATIONS[1]),
        c_out: [0x6b; 32],
    };
    let actors = w.public_burn_actors(payer, &cid_a, &note);
    let own = own_actor(program, payer);
    let burn =
        w.send("Burn of 1 ETH by the payer, after the delisting", own, &BridgeMessage::burn(cid_a, &note), actors);
    run(&mut reports, burn, None);
    let dest = w.vector_dest;
    let r = w.transfer("Transfer of 1 ETH from the payer, after the delisting", &cid_a, ETH, DENOMINATIONS[1], dest);
    run(&mut reports, r, None);
    assert_eq!(w.balance(payer, &cid_a), DENOMINATIONS[2] - 2 * DENOMINATIONS[1]);
    assert_eq!(w.balance(dest, &cid_a), 2 * DENOMINATIONS[1]);

    // A listing word other than 0 or 1 is not the vault's.
    let odd = vault_a_block(&mut w, Some(2));
    run(
        &mut reports,
        w.attest_listing("AttestListing ETH: the slot holds 2", &cid_a, ETH, odd, &va),
        Some(code::IMPORT_MISMATCH),
    );

    // The vault relists ETH (the slot back to 0): attested from a newer record.
    let relisted = vault_a_block(&mut w, None);
    let slot = w.blocks[relisted].slot;
    run(
        &mut reports,
        w.attest_listing("AttestListing ETH: relisted (the slot holds 0)", &cid_a, ETH, relisted, &va),
        None,
    );
    assert_eq!((token(&w).delisted, token(&w).listing_slot), (false, slot));
    // The delisting's record, now older, cannot bring it back.
    let r = w.attest_listing("AttestListing ETH under the delisting's record (now older)", &cid_a, ETH, delisted, &va);
    run(&mut reports, r, Some(code::IMPORT_MISMATCH));
    assert!(!token(&w).delisted);

    let table = render(&reports);
    let transitions = render_transitions(&reports);
    println!("{table}");
    fs::write(
        results_path("bridge-delist.md"),
        format!(
            "# Delisting on LEZ (`harness/tests/bridge_delist.rs`, RFP-022 F.10)\n\n\
             Bridge program image `{}`; LEZ v0.4.0-rc1's state machine, every transaction settled in a block of its \
             own (RISC Zero dev mode). The vault's listing slot of ETH is placed in synthetic blocks of vault A's \
             storage, as `FerrataVault.setDepositsEnabled` writes it (`vault/test/Admin.t.sol`).\n\n{table}\n\
             ## Handler executions\n\n{transitions}",
            harness::image_id_hex(harness::bridge::BRIDGE_ID)
        ),
    )
    .unwrap();
}
