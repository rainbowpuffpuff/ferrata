//! Priv.5 on LEZ (RFP-022): "Failure and error paths must not reveal which deposit or burn was involved: a rejected
//! claim, a repeat claim, and a cap rejection must be indistinguishable in that respect."
//!
//! Measured here, through LEZ v0.4.0-rc1's state machine, for the three refusals on both claim paths:
//!
//! * The private claim (the default path of the CLI and the bridge module): the claimant's own actor verifies the
//!   deposit inside a privacy-preserving execution, and the client proves only what the public part accepts
//!   (`Chain::prove_and_settle_private` simulates it first). A rejected claim (a wrong `s`: 249), a repeat (250) and a
//!   cap rejection (255) are each refused before any proof exists: nothing reaches the chain, no block, no fee, no
//!   state, so the three are indistinguishable on chain, and none names a deposit.
//! * The plaintext claim (an explicit opt-out, `--i-accept-plaintext-links-deposit`): the witness travels in the clear,
//!   so a plaintext claim names its deposit whether it applies or not. The three refusals are included with their exit
//!   codes; each publishes the same fields of the deposit as a successful plaintext claim, and nothing more.
//!
//! Re-run (in `lez/`): `RISC0_DEV_MODE=1 cargo test -p harness --locked --test bridge_refusals -- --nocapture`.
//! Writes `lez/results/bridge-refusals.md` (the records of this branch: `lez/results/b1/`, with
//! `FERRATA_LEZ_RESULTS_DIR`; `lez/results/b1/README.md`).

use std::{fmt::Write as _, fs, path::PathBuf};

use ferrata_lez_bridge_wire::{code, config_actor, own_actor, token_actor, BridgeMessage, CapParams, ClaimWitness};
use harness::{
    bridge::{
        catalog, fee_recipient, holder_of, nullifier_of, relayer, Deposit, World, ADMIN_SEED, CHAIN_ID, ETH, FLOOR,
    },
    TxReport,
};
use lee::Actor;

const RESERVE: u128 = 100_000_000;

fn results_path(name: &str) -> PathBuf {
    harness::results_dir().join(name)
}

fn occurs(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Every public account a claim of `deposit` under `cid` could touch, and their states: what a refusal must leave as
/// it was.
fn watched(w: &World, deposit: &Deposit) -> Vec<(Actor, Vec<u8>)> {
    let (program, cid, va) = (w.program, w.cid_a, w.vault_a.address);
    let n = deposit.nullifier(CHAIN_ID, &va);
    let mut actors = vec![
        config_actor(program, &cid),
        token_actor(program, &cid, &ETH),
        nullifier_of(program, &cid, CHAIN_ID, &va, &n),
        holder_of(program, &cid, fee_recipient()),
        holder_of(program, &cid, relayer()),
    ];
    actors.push(holder_of(program, &cid, lee::AccountId::new(deposit.dest)));
    actors.into_iter().map(|a| (a, w.chain.actor_state(a).to_vec())).collect()
}

/// What a public observer can read off the bytes of a transaction about `deposit`.
fn names(bytes: &[u8], deposit: &Deposit, va: &[u8; 20]) -> String {
    let mut found = Vec::new();
    for (what, needle) in [
        ("s", deposit.s.to_vec()),
        ("the leaf", deposit.leaf().to_vec()),
        ("the commitment", deposit.commitment().to_vec()),
        ("N", deposit.nullifier(CHAIN_ID, va).to_vec()),
        ("the destination", deposit.dest.to_vec()),
    ] {
        if occurs(bytes, &needle) {
            found.push(what);
        }
    }
    if found.is_empty() {
        "none".into()
    } else {
        found.join(", ")
    }
}

#[test]
fn rejected_repeated_and_capped_claims_reveal_no_deposit() {
    assert_eq!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1"), "run with RISC0_DEV_MODE=1");
    let mut w = World::with_roots();
    let (program, cid, va) = (w.program, w.cid_a, w.vault_a.address);
    let mut out =
        String::from("# Refused, repeated and capped claims (`harness/tests/bridge_refusals.rs`, RFP-022 Priv.5)\n\n");
    let _ = writeln!(out, "Bridge program image `{}`.\n", harness::image_id_hex(harness::bridge::BRIDGE_ID));

    // ---- The private claim: the default path ------------------------------------------------------------------
    let prover = w.chain.add_private_account(0x51, 1_000_000_000_000);
    let me = own_actor(program, prover.id());
    let payer_deposit = w.payer_deposit.clone();
    let vector = w.vector.clone();
    let private = |w: &mut World, label: &str, deposit: &Deposit, witness: ClaimWitness| {
        let message = w.claim_privately(&cid, 8, witness);
        let publics = w.private_claim_actors(&cid, 8, &va, deposit);
        let (block, before) = (w.chain.block_id, watched(w, deposit));
        let run = w.chain.prove_and_settle_private(
            &prover,
            me,
            borsh::to_vec(&message).unwrap(),
            publics,
            &catalog(),
            RESERVE,
            w.time,
        );
        (label.to_string(), run, block, before)
    };
    let mut rows = String::from(
        "## The private claim (the default)\n\n| claim | outcome | blocks added | public accounts changed |\n|---|---|---:|---:|\n",
    );
    type Attempt = (String, Result<harness::private::PrivateRun, String>, u64, Vec<(Actor, Vec<u8>)>);
    let check_refused = |w: &World, rows: &mut String, deposit: &Deposit, r: Attempt, c: u8| {
        let (label, run, block, before) = r;
        let err = run.err().unwrap_or_else(|| panic!("{label}: applied, expected {c}"));
        assert!(err.contains(&c.to_string()), "{label}: {err}");
        assert_eq!(w.chain.block_id, block, "{label}: no block");
        let after = watched(w, deposit);
        let changed = before.iter().zip(&after).filter(|(b, a)| b.1 != a.1).count();
        assert_eq!(changed, 0, "{label}: no public state changed");
        let _ = writeln!(
            rows,
            "| {label} | refused before any proof ({}) | 0 | 0 |",
            harness::bridge::code_name(u32::from(c)).unwrap()
        );
    };
    // Rejected: a wrong s (no leaf opens).
    let mut wrong = w.vault_a.witness(&payer_deposit, 8);
    wrong.s = [0x42; 32];
    let r = private(&mut w, "rejected: leaf 6 with a wrong s", &payer_deposit, wrong);
    check_refused(&w, &mut rows, &payer_deposit, r, code::CLAIM_INVALID);
    // The deposit claimed once, privately: applied.
    let witness = w.vault_a.witness(&payer_deposit, 8);
    let r = private(&mut w, "leaf 6, its owner's claim", &payer_deposit, witness);
    let applied = r.1.as_ref().expect("the owner's private claim settles");
    assert!(applied.charge.revert.is_none());
    let _ = writeln!(rows, "| {} | applied | 1 | (a mint) |", r.0);
    // Repeated.
    let witness = w.vault_a.witness(&payer_deposit, 8);
    let r = private(&mut w, "repeated: leaf 6 again", &payer_deposit, witness);
    check_refused(&w, &mut rows, &payer_deposit, r, code::ALREADY_CLAIMED);
    // Capped: the admin sets ETH's mint bucket at the floor (1 ETH's value), below any claim with its fees.
    let cap = BridgeMessage::SetCap { config_id: cid, token: Some(ETH), mints: true, cap: FLOOR };
    let r = w.role("SetCap: ETH mints at the floor", ADMIN_SEED, &cap, token_actor(program, &cid, &ETH));
    assert!(r.ok(), "SetCap: {:?}", r.exit_code);
    let witness = w.vault_a.witness(&vector, 8);
    let r = private(&mut w, "capped: leaf 2 (1 ETH and its fees)", &vector, witness);
    check_refused(&w, &mut rows, &vector, r, code::CAPPED);
    out.push_str(&rows);
    out.push_str(
        "\nNo refusal of the private path adds a block, a fee or any public state: on chain the three are the same \
         (nothing), and none names a deposit.\n\n",
    );

    // ---- The plaintext claim: the explicit opt-out ---------------------------------------------------------------
    let mut table = String::from(
        "## The plaintext claim (opt-out)\n\n| claim | exit code | transaction bytes | gas_exec | deposit fields in the transaction |\n\
         |---|---:|---:|---:|---|\n",
    );
    let mut plain =
        |w: &mut World, label: &str, deposit: &Deposit, witness: ClaimWitness, want: Option<u8>| -> TxReport {
            let r = w.claim(label, &cid, 8, &va, witness);
            match want {
                None => assert!(r.ok(), "{label}: {:?}", r.exit_code),
                Some(c) => assert_eq!(r.exit_code, Some(u32::from(c)), "{label}"),
            }
            let bytes = w.chain.last_tx.clone();
            let _ = writeln!(
                table,
                "| {label} | {} | {} | {} | {} |",
                r.exit_code.map_or("0 (applied)".into(), |c| c.to_string()),
                bytes.len(),
                r.charge.gas_exec,
                names(&bytes, deposit, &va)
            );
            r
        };
    let set_cap = |w: &mut World, label: &str, cap: CapParams| {
        let message = BridgeMessage::SetCap { config_id: cid, token: Some(ETH), mints: true, cap };
        let r = w.role(label, ADMIN_SEED, &message, token_actor(program, &cid, &ETH));
        assert!(r.ok(), "{label}: {:?}", r.exit_code);
    };
    // A world of its own (a lifted cap refills only with attested time): rejected, applied, repeated, capped.
    let mut w = World::with_roots();
    let mut wrong = w.vault_a.witness(&vector, 8);
    wrong.s = [0x42; 32];
    plain(&mut w, "rejected: leaf 2 with a wrong s", &vector, wrong, Some(code::CLAIM_INVALID));
    let witness = w.vault_a.witness(&payer_deposit, 8);
    plain(&mut w, "leaf 6, applied (for comparison)", &payer_deposit, witness.clone(), None);
    plain(&mut w, "repeated: leaf 6 again", &payer_deposit, witness, Some(code::ALREADY_CLAIMED));
    set_cap(&mut w, "SetCap: ETH mints at the floor", FLOOR);
    let witness = w.vault_a.witness(&vector, 8);
    plain(&mut w, "capped: leaf 2", &vector, witness, Some(code::CAPPED));
    out.push_str(&table);
    out.push_str(
        "\nA plaintext claim carries its witness, so every plaintext claim, applied or refused, names its deposit (`s`, \
         the destination; the leaf and `N` follow from them). A refusal names no more than the claim that applies.\n",
    );
    println!("{out}");
    fs::write(results_path("bridge-refusals.md"), out).unwrap();
}
