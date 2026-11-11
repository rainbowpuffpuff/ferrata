//! The second consumer of the attestation (`guests/holdings`): "`holder` held at least `minimum`
//! of ERC-20 `token` at a finalized block", on mainnet block 26,384,851 (the Stage 4 fixture):
//! WETH's balance of Uniswap v2's USDC/WETH pair, from the committed `eth_getProof` response,
//! cross-checked against the committed `eth_call balanceOf`.
//!
//! One chain for every case: a refused transaction changes no state. Writes
//! `results/holdings.md` (the transactions, their gas and cycles).

use std::fs;

use ferrata_core::Error;
use ferrata_lez_holdings_wire::{
    code as holdings_code, holding_actor, holding_seed, mapping_key, HoldingsMessage, HoldingsState,
};
use ferrata_lez_wire::{code, header_actor};
use harness::{
    attest::{storage_statement_id, u256},
    exit_name,
    holdings::{holdings_account, Holdings, WETH_BALANCES_SLOT},
    render, render_transitions, results_dir, TxReport,
};
use lee::PublicAccountEvidence;

/// One WETH, in wei.
const WETH: u128 = 1_000_000_000_000_000_000;

fn plus_one(word: &[u8; 32]) -> [u8; 32] {
    let mut out = *word;
    for byte in out.iter_mut().rev() {
        let (sum, carry) = byte.overflowing_add(1);
        *byte = sum;
        if !carry {
            break;
        }
    }
    out
}

/// A refusal: the transaction exits with `expected`, the native evaluation of the same request
/// (when Ferrata refuses) agrees, and the holding actor is left as it was.
fn refused(
    h: &Holdings,
    report: &TxReport,
    expected: u8,
    before: &Option<HoldingsState>,
    gate: &ferrata_lez_holdings_wire::Gate,
) {
    assert_eq!(
        report.exit_code,
        Some(u32::from(expected)),
        "{}: {:?} ({})",
        report.label,
        report.charge.revert,
        report.exit_code.map(exit_name).unwrap_or_default()
    );
    assert!(report.events.is_empty(), "{}: a refusal emits nothing", report.label);
    assert_eq!(&h.holding(gate, &h.holder), before, "{}: the holding is unchanged", report.label);
    println!("refused as expected: {} → exit {expected} ({})", report.label, exit_name(u32::from(expected)));
}

#[test]
fn holdings_consumer() {
    let mut h = Holdings::new();
    let mut reports = Vec::new();
    let slot = h.slot();
    let balance = h.balance();
    let fx_block = h.s.fx.block;

    // The fixture is what the program will be asked about: the balance key is WETH9's
    // `balanceOf[holder]` (mapping at slot 3), and `eth_getProof` agrees with `eth_call`.
    assert_eq!(mapping_key(&h.holder, &u256(WETH_BALANCES_SLOT)), h.s.fx.weth_slot, "balanceOf is at slot 3");
    assert_eq!(balance, h.s.fx.weth_balance_by_call(fx_block), "eth_getProof and eth_call agree");
    println!("holder 0x{} held 0x{} wei of WETH at block {fx_block}", hex::encode(h.holder), hex::encode(balance));

    // --- Refusals (each leaves the chain as it was) ---------------------------------------------
    let gate = h.gate(u256(1_000 * WETH));
    let (account_proof, storage_proof) = h.proofs();
    let request = gate.request(&h.holder, account_proof.clone(), storage_proof.clone(), None);

    // The wrong chain: a gate that expects Sepolia, against mainnet's records.
    let sepolia = ferrata_lez_holdings_wire::Gate { chain_id: 11_155_111, ..gate.clone() };
    let native = h.s.expected(slot, &sepolia.request(&h.holder, account_proof.clone(), storage_proof.clone(), None));
    assert_eq!(native.err(), Some(Error::ChainMismatch.code() as u8));
    let before = h.holding(&sepolia, &h.holder);
    let r = h.check("wrong chain (a gate for Sepolia)", &sepolia, slot);
    refused(&h, &r, Error::ChainMismatch.code() as u8, &before, &sepolia);
    reports.push(r);

    // An unfinalized block: the update's attested header was never finalized, so nothing records
    // it. The record actor is admitted by evidence, so that the attestation program refuses.
    let attested = h.s.fx.update.attested_header.beacon.slot;
    assert!(attested > slot);
    let evidence = h.record_evidence(&gate, attested);
    let before = h.holding(&gate, &h.holder);
    let holder = h.holder;
    let r = h.check_with(
        "unfinalized block (the attested, not finalized, header)",
        &gate,
        attested,
        &holder,
        account_proof.clone(),
        storage_proof.clone(),
        evidence,
    );
    refused(&h, &r, code::NO_RECORD, &before, &gate);
    reports.push(r);

    // An unknown block: a slot with no record.
    let evidence = h.record_evidence(&gate, slot + 1);
    let r = h.check_with(
        "unknown block (no record at this slot)",
        &gate,
        slot + 1,
        &holder,
        account_proof.clone(),
        storage_proof.clone(),
        evidence,
    );
    refused(&h, &r, code::NO_RECORD, &before, &gate);
    reports.push(r);

    // A record of another block (the bootstrap's): the proofs do not hash to its state root.
    let other_block = h.s.fx.bootstrap_slot;
    let native = h.s.expected(other_block, &request).expect_err("refused natively");
    let r = h.check("proof of another block (the bootstrap's record)", &gate, other_block);
    refused(&h, &r, native, &before, &gate);
    reports.push(r);

    // A bad proof: one byte of the storage proof's leaf changed.
    let mut bad = storage_proof.clone();
    let last = bad.last_mut().unwrap();
    let at = last.len() - 1;
    last[at] ^= 0x01;
    let native =
        h.s.expected(slot, &gate.request(&h.holder, account_proof.clone(), bad.clone(), None))
            .expect_err("refused natively");
    let r = h.check_with(
        "bad proof (a storage-proof byte flipped)",
        &gate,
        slot,
        &holder,
        account_proof.clone(),
        bad,
        vec![],
    );
    refused(&h, &r, native, &before, &gate);
    reports.push(r);

    // A bad proof: one byte of the account proof's root node changed.
    let mut bad = account_proof.clone();
    bad[0][5] ^= 0x01;
    let native =
        h.s.expected(slot, &gate.request(&h.holder, bad.clone(), storage_proof.clone(), None))
            .expect_err("refused natively");
    let r = h.check_with(
        "bad proof (an account-proof byte flipped)",
        &gate,
        slot,
        &holder,
        bad,
        storage_proof.clone(),
        vec![],
    );
    refused(&h, &r, native, &before, &gate);
    reports.push(r);

    // A mismatched value: a minimum one wei above the balance.
    let above = h.gate(plus_one(&balance));
    let native =
        h.s.expected(slot, &above.request(&h.holder, account_proof.clone(), storage_proof.clone(), None))
            .expect_err("refused natively");
    let before_above = h.holding(&above, &h.holder);
    let r = h.check("mismatched value (minimum = balance + 1 wei)", &above, slot);
    refused(&h, &r, native, &before_above, &above);
    reports.push(r);

    // A mismatched holder: the pair's proofs, claimed for another address.
    let other = h.s.fx.transfer.to;
    let native =
        h.s.expected(slot, &gate.request(&other, account_proof.clone(), storage_proof.clone(), None))
            .expect_err("refused natively");
    let r = h.check_with(
        "mismatched holder (the pair's proofs for another address)",
        &gate,
        slot,
        &other,
        account_proof.clone(),
        storage_proof.clone(),
        vec![],
    );
    assert_eq!(r.exit_code, Some(u32::from(native)), "{:?}", r.charge.revert);
    assert!(h.holding(&gate, &other).is_none());
    println!("refused as expected: {} → exit {native} ({})", r.label, exit_name(u32::from(native)));
    reports.push(r);

    // The wrong actor: a check for the holder sent to another holder's actor (this program's
    // own refusal, before anything is asked).
    let program = holdings_account();
    let wrong = holding_actor(program, &gate.id(), &other);
    let message = HoldingsMessage::Check {
        gate: gate.clone(),
        slot,
        holder: h.holder,
        account_proof: account_proof.clone(),
        storage_proof: storage_proof.clone(),
        header: None,
    };
    let record = header_actor(gate.ferrata, &gate.instance_id, slot);
    let time = h.s.time;
    let r = h.s.w.chain.submit(
        "wrong actor (a check sent to another holder's actor)",
        wrong,
        borsh::to_vec(&message).unwrap(),
        vec![wrong, record],
        vec![PublicAccountEvidence::Pda { program, seed: holding_seed(&gate.id(), &other) }],
        time,
    );
    refused(&h, &r, holdings_code::WRONG_RECEIVER, &before, &gate);
    reports.push(r);

    // --- Accepted -----------------------------------------------------------------------------
    // At least 1,000 WETH.
    let r = h.check("accepted: the pair held ≥ 1,000 WETH", &gate, slot);
    assert!(r.ok(), "{:?} (exit {:?})", r.charge.revert, r.exit_code);
    let Some(HoldingsState::Held(held)) = h.holding(&gate, &h.holder) else { panic!("no holding") };
    let expected_id = storage_statement_id(h.s.fx.chain_id, &h.s.fx.weth, &h.s.fx.weth_slot, &balance);
    assert_eq!(held.gate_id, gate.id());
    assert_eq!(held.holder, h.holder);
    assert_eq!(held.balance, balance, "the balance eth_call gives");
    assert_eq!(held.block_number, fx_block);
    let manifest: serde_json::Value = serde_json::from_slice(&harness::attest::fixture_file("fixture.json")).unwrap();
    assert_eq!(hex::encode(held.block_hash), manifest["block_hash"].as_str().unwrap().trim_start_matches("0x"));
    assert_eq!(held.beacon_slot, slot);
    assert_eq!(held.statement_id, expected_id, "the statement identifier of ferrata_core::attest");
    println!("accepted: {held:?}");
    reports.push(r);

    // The boundary: a minimum equal to the balance passes.
    let exact = h.gate(balance);
    let r = h.check("accepted: minimum = the balance exactly", &exact, slot);
    assert!(r.ok(), "{:?} (exit {:?})", r.charge.revert, r.exit_code);
    assert!(matches!(h.holding(&exact, &h.holder), Some(HoldingsState::Held(held)) if held.balance == balance));
    reports.push(r);

    // A second check at the same block cannot replace the holding (this program's own refusal,
    // after the attestation).
    let before = h.holding(&gate, &h.holder);
    let r = h.check("stale: the same block again", &gate, slot);
    refused(&h, &r, holdings_code::STALE, &before, &gate);
    reports.push(r);

    let table = format!(
        "# ferrata_holdings on the Stage 4 chain (dev mode, in-process harness)\n\n\
         Written by `RISC0_DEV_MODE=1 cargo test -p harness --locked --test holdings -- --nocapture`.\n\
         Holder 0x{} (Uniswap v2 USDC/WETH), WETH 0x{}, block {fx_block}, record `header(id, {slot})`,\n\
         balance 0x{} wei.\n\n## Transactions\n\n{}\n## Handlers\n\n{}",
        hex::encode(h.holder),
        hex::encode(h.s.fx.weth),
        hex::encode(balance),
        render(&reports),
        render_transitions(&reports),
    );
    println!("{table}");
    fs::write(results_dir().join("holdings.md"), table).unwrap();
}
