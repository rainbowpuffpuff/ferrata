//! Accumulator leaves through the program, and Ferrata's vault on Sepolia (`harness::vault`).

use harness::{render, vault};

const MAX_GAS_EXEC: u64 = 10_000_000;

/// The vault's request (its account, slots 0 and 1, deposit leaves against the root in slot 0)
/// through the program in the guest, on a synthetic state that holds the vault's own eight
/// deposits under a record in the Gloas layout; then the refusals.
#[test]
fn accumulator_leaves_through_the_program_on_a_synthetic_vault() {
    let (accepted, refused) = vault::synthetic_attestations();
    println!("{}", render(&accepted));
    println!("{}", render(&refused));
    for r in accepted.iter().chain(&refused) {
        assert!(r.charge.gas_exec <= MAX_GAS_EXEC, "{} is over MAX_GAS_EXEC", r.label);
    }
}

/// The same request on the deployed vault, from the fixture of
/// `lez/scripts/fetch-attest-fixture.py --network sepolia --storage
/// 0xdfedf9A73Cfb337277b95a9a1C941c3496A4385F:0=depositRoot(),1=nextLeafIndex() --deposits
/// 0xdfedf9A73Cfb337277b95a9a1C941c3496A4385F:DEPLOY_BLOCK --updates-from 1410` in
/// `crates/ferrata-spectests/fixtures/sepolia-vault` (or `FERRATA_VAULT_FIXTURE_DIR`). Until that
/// fixture exists the test says so and passes.
#[test]
fn the_vault_on_sepolia() {
    let Some(dir) = vault::vault_fixture_dir() else {
        println!("no vault fixture yet (crates/ferrata-spectests/fixtures/sepolia-vault): nothing to attest");
        return;
    };
    let reports = vault::real(&dir);
    println!("{}", render(&reports));
}
