//! Stage 1: the program driven through LEZ v0.4.0-rc1's settlement with real mainnet data.
//! The scenarios (in `harness::scenarios`) assert against the native `FinalizedStore` after every
//! job; these tests add the gas bounds and print the tables.

use harness::{render, scenarios};

const MAX_GAS_EXEC: u64 = 10_000_000;

#[test]
fn mainnet_replay_through_lez() {
    let world = scenarios::replay(2);
    println!("{}", render(&world.chain.reports));
    assert_eq!(world.chain.reports.len(), 3 + 5 * 5, "Init: 3 transactions; each adopting update: 5");
    for r in &world.chain.reports {
        assert!(r.ok(), "{}", r.label);
        assert!(r.charge.gas_exec <= MAX_GAS_EXEC, "{} is over MAX_GAS_EXEC", r.label);
    }
}

#[test]
fn a_finality_update_is_three_transactions() {
    let world = scenarios::finality_path();
    println!("{}", render(&world.chain.reports));
    for r in &world.chain.reports {
        assert!(r.ok(), "{}", r.label);
        assert!(r.charge.gas_exec <= MAX_GAS_EXEC, "{} is over MAX_GAS_EXEC", r.label);
    }
}

#[test]
fn refusals_exit_with_a_code_and_are_charged_what_they_used() {
    let refusals = scenarios::refusals();
    let reports: Vec<_> =
        refusals.iter().map(|r| harness::TxReport { label: r.case.to_string(), ..r.report.clone() }).collect();
    println!("{}", render(&reports));
    for r in &refusals {
        assert!(r.report.charge.gas_exec <= MAX_GAS_EXEC, "{}", r.case);
    }
}

#[test]
fn a_finality_update_in_one_public_transaction() {
    let reports = scenarios::one_transaction_update();
    println!("{}", render(&reports));
    assert!(reports[0].charge.gas_exec <= MAX_GAS_EXEC, "over MAX_GAS_EXEC");
}

#[test]
fn shell_costs() {
    let reports = scenarios::shell_costs();
    println!("{}", harness::render_transitions(&reports));
}
