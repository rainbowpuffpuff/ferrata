//! Stage 4, the public path: attestation through the program on real mainnet data
//! (`harness::attest`; the fixture's SOURCE.md lists every request). The instance verifies the
//! finality update of 10 November 2026 through the public job; every attestation here is then
//! proven against the record `header(id, slot)` holds, in a transaction settled by LEZ's own
//! functions. The scenarios assert each answer against JSON-RPC; these tests add the gas bounds
//! and print the tables.

use ferrata_lez_wire::consumer::code as consumer_code;
use harness::{attest, render, render_transitions};

const MAX_GAS_EXEC: u64 = 10_000_000;
const TARGET_GAS_EXEC: u64 = 5_000_000;

#[test]
fn every_kind_of_fact_is_attested_under_the_real_record() {
    let (_, reports) = attest::accepted();
    println!("{}", render(&reports));
    println!("{}", render_transitions(&reports));
    assert_eq!(reports.len(), 8, "account, slot, absent slot, absent account, two logs, a hop, a batch");
    for r in &reports {
        assert!(r.ok(), "{}", r.label);
        assert_eq!(r.transitions.len(), 1, "{}: the attestation is one handler execution", r.label);
        assert!(r.charge.gas_exec <= TARGET_GAS_EXEC, "{} is over TARGET_GAS_EXEC", r.label);
    }
}

#[test]
fn the_same_attestations_with_software_keccak_in_the_program() {
    // The comparison build (`guests/program-keccak-soft`), deployed at an address of its own:
    // the same scenario, the same answers, every Keccak-f permutation computed in the guest.
    let (_, soft) = attest::accepted_at(harness::KECCAK_SOFT);
    let (_, accelerated) = attest::accepted();
    println!("{}", render(&soft));
    assert_eq!(soft.len(), accelerated.len());
    for (s, a) in soft.iter().zip(&accelerated) {
        assert!(s.ok(), "{}", s.label);
        assert_eq!(s.label, a.label);
        assert_eq!(s.charge.gas_stor, a.charge.gas_stor, "{}: the same transaction bytes", s.label);
        assert!(s.charge.gas_exec > a.charge.gas_exec, "{}: software Keccak costs more cycles", s.label);
        assert!(s.charge.gas_exec <= MAX_GAS_EXEC, "{} is over MAX_GAS_EXEC", s.label);
    }
}

#[test]
fn the_reference_consumer_credits_a_transfer_once() {
    let (_, reports) = attest::consumer_path();
    println!("{}", render(&reports));
    println!("{}", render_transitions(&reports));
    for r in &reports {
        assert!(r.charge.gas_exec <= TARGET_GAS_EXEC, "{} is over TARGET_GAS_EXEC", r.label);
    }
    let replay = reports.last().unwrap();
    assert_eq!(replay.exit_code, Some(u32::from(consumer_code::ALREADY_CLAIMED)));
}

#[test]
fn refusals_exit_with_their_codes_and_change_nothing() {
    let refusals = attest::refusals();
    let reports: Vec<_> =
        refusals.iter().map(|r| harness::TxReport { label: r.case.to_string(), ..r.report.clone() }).collect();
    println!("{}", render(&reports));
    for r in &refusals {
        assert!(r.report.exit_code.is_some(), "{}: refused with a code", r.case);
        assert!(!r.report.ok(), "{}", r.case);
        // A coded refusal is charged the cycles it used, never the gas limit.
        assert!(r.report.charge.gas_exec < MAX_GAS_EXEC, "{}", r.case);
    }
    // Every code of the brief's list occurs: chain (140), instance (211), no record (213), proof
    // (110, 111, 112), predicate (130), log index (125), receipt (122, 124), hop (150, 151),
    // malformed (131, 200, 214), and the consumer's replay (223).
    let codes: std::collections::BTreeSet<u32> = refusals.iter().filter_map(|r| r.report.exit_code).collect();
    for code in [110, 111, 112, 122, 124, 125, 130, 131, 140, 150, 151, 200, 209, 211, 213, 214, 220, 223, 225, 229] {
        assert!(codes.contains(&code), "no refusal with code {code}: {codes:?}");
    }
}
