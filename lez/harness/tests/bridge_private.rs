//! The private claim (`lez/BRIDGE.md` §2): `ClaimPrivately` at the destination's own private
//! account reads `root(cid, n)`, verifies the deposit inside its own execution and sends only
//! `Spend`; proven with LEZ's privacy-preserving circuit in RISC Zero dev mode (executed, not
//! proven) and settled by LEZ's private fee path. The test dumps the transaction and searches its
//! bytes for every private input. Writes `results/bridge-private-claim.md`.

use std::{fs, path::PathBuf};

use ferrata_lez_bridge_wire::{
    holder_actor, nullifier_actor, root_actor, token_actor, BridgeMessage, BridgeState, HolderState, Holding,
    NullifierState, ScheduleRange,
};
use harness::{
    attest::render_executed,
    bridge::{
        catalog, fee_recipient, private_claim_transitions, private_keys, private_post_state, relayer, World, CHAIN_ID,
        DENOMINATIONS, ETH, MINT_FEES_1, RELAY_FEES,
    },
    private::{describe_public_outputs, PrivateAccount},
    settle::LeeTransaction,
};
use lee::Actor;
use lee_core::{execution_state::BoundaryStep, native_token, program::StateReply};

const RESERVE: u128 = 100_000_000;

fn require_dev_mode() {
    assert_eq!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1"), "run these tests with RISC0_DEV_MODE=1");
}

fn occurs(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn results_path(name: &str) -> PathBuf {
    harness::results_dir().join(name)
}

#[test]
fn a_private_claim_publishes_only_the_nullifier_the_denomination_and_the_root_read() {
    require_dev_mode();
    let mut w = World::with_roots();
    let seed = w.private_seed;
    let account = w.chain.add_private_account(seed, 1_000_000_000_000);
    let deposit = w.private_deposit.clone();
    assert_eq!(account.id().value(), &deposit.dest, "the deposit is for this private account");
    let (cid, va) = (w.cid_a, w.vault_a.address);
    let witness = w.vault_a.witness(&deposit, 8);
    let message = w.claim_privately(&cid, 8, witness.clone());
    let n = deposit.nullifier(CHAIN_ID, &va);

    // The two private transitions in the executor: what a prover has to prove for the bridge.
    let executed = private_claim_transitions(&w, account.id(), &message, &cid, 8);
    assert_eq!(executed.iter().map(|e| e.exit_code).collect::<Vec<_>>(), [0, 0]);
    let executed_table = render_executed(&executed);
    println!("{executed_table}");

    // Through LEZ: proven (dev mode), settled.
    let root = root_actor(w.program, &cid, 8);
    let token = token_actor(w.program, &cid, &ETH);
    let publics = w.private_claim_actors(&cid, 8, &va, &deposit);
    assert!(publics.contains(&nullifier_actor(w.program, CHAIN_ID, &va, &n)));
    let me = holder_actor(w.program, account.id());
    let run = w
        .chain
        .prove_and_settle_private(&account, me, borsh::to_vec(&message).unwrap(), publics, &catalog(), RESERVE, w.time)
        .expect("settles");
    assert!(run.charge.revert.is_none(), "{:?}", run.charge.revert);

    // What crosses into public execution: `Read` to the root, the root back, `Issue`, the fee.
    let fee_inbox = Actor::native_balance(system_accounts::fee_inbox_account_id());
    let mut outgoing = Vec::new();
    let mut incoming = Vec::new();
    for step in &run.tx.message.execution.boundary {
        match step {
            BoundaryStep::PrivateToPublic(d) => outgoing.push((d.envelope.to, d.envelope.message.clone())),
            BoundaryStep::PublicToPrivate(d) => incoming.push((d.envelope.from, d.envelope.message.clone())),
            BoundaryStep::EndPrivateSubtree | BoundaryStep::EndPublicSubtree => {}
        }
    }
    assert_eq!(outgoing.len(), 3, "the read, the issue and the fee: {outgoing:?}");
    assert_eq!(outgoing[0], (root, borsh::to_vec(&BridgeMessage::Read).unwrap()), "one Read of the root");
    assert_eq!(outgoing[1].0, token);
    assert_eq!(
        borsh::from_slice::<BridgeMessage>(&outgoing[1].1).unwrap(),
        BridgeMessage::Issue {
            config_id: cid,
            token: ETH,
            nullifier: n,
            amount: DENOMINATIONS[1],
            range: ScheduleRange { index: 0, from_leaf: 0, until_leaf: 6 },
            leaf_count: 8,
            mint_to: None,
            relayer: Some(*relayer().value()),
        },
        "Issue carries N, the token, the denomination, the root read, the fee schedule's range (not the leaf \
         index) and the relayer; no destination and no time"
    );
    assert_eq!(outgoing[2].0, fee_inbox, "the fee transfer");
    assert!(matches!(borsh::from_slice(&outgoing[2].1), Ok(native_token::Message::Credit(_))));
    assert_eq!(incoming.len(), 1);
    assert_eq!(incoming[0].0, root, "the reply comes from the root");
    let reply: StateReply = borsh::from_slice(&incoming[0].1).unwrap();
    assert_eq!(
        borsh::from_slice::<BridgeState>(&reply.state).unwrap(),
        BridgeState::Root(w.root_state(&cid, 8).unwrap())
    );

    // Public state: N is spent; nothing public at the private account.
    assert_eq!(
        w.nullifier(&va, &n),
        Some(NullifierState { config_id: cid, token: ETH, leaf_count: 8, amount: DENOMINATIONS[1] })
    );
    assert!(w.holder(account.id()).is_none(), "no public holder state at the private account");
    assert_eq!(w.balance(relayer(), &cid), RELAY_FEES[1], "the relay fee, minted publicly");
    assert_eq!(w.balance(fee_recipient(), &cid), 0, "leaf 5 is under schedule 0: no mint fee");
    // Private state: the chain holds the commitment of the account with its 1 ETH holding.
    let holder =
        HolderState { holdings: vec![Holding { config_id: cid, token: ETH, amount: DENOMINATIONS[1] }], pending: None };
    let post = private_post_state(&w.chain, &private_keys(seed), &account.account, RESERVE, &holder)
        .expect("the chain holds the claimed account's commitment");
    let _ = PrivateAccount { keys: private_keys(seed), account: post };

    // Nothing of the deposit is in the transaction.
    let bytes = borsh::to_vec(&LeeTransaction::PrivacyPreserving(run.tx.clone())).unwrap();
    assert_eq!(bytes.len(), run.tx_bytes);
    let mut needles: Vec<(String, Vec<u8>)> = vec![
        ("the destination (the private account id)".into(), deposit.dest.to_vec()),
        ("the deposit secret s".into(), deposit.s.to_vec()),
        ("the commitment C".into(), deposit.commitment().to_vec()),
        ("the deposit leaf".into(), deposit.leaf().to_vec()),
        ("the depositor".into(), deposit.depositor.to_vec()),
        ("the borsh witness".into(), borsh::to_vec(&witness).unwrap()),
    ];
    for (i, sibling) in witness.path.iter().enumerate().filter(|(_, s)| **s != [0u8; 32]) {
        needles.push((format!("path sibling {i}"), sibling.to_vec()));
    }
    let mut found = Vec::new();
    let mut report = format!(
        "The {} serialized bytes of the transaction were searched for the claim's private inputs:\n",
        bytes.len()
    );
    for (what, needle) in &needles {
        let hit = occurs(&bytes, needle);
        report.push_str(&format!("- {what} ({} bytes): {}\n", needle.len(), if hit { "FOUND" } else { "absent" }));
        if hit {
            found.push(what.clone());
        }
    }
    // And what must be there (B6): N, the configuration, the root read.
    for (what, needle) in [("N", n.to_vec()), ("configId", cid.to_vec()), ("the root read", w.vault_a.root(8).to_vec())]
    {
        let hit = occurs(&bytes, &needle);
        report.push_str(&format!("- {what} (public by design): {}\n", if hit { "present" } else { "MISSING" }));
        assert!(hit, "{what} is public");
    }
    println!("{report}");
    assert!(found.is_empty(), "private inputs in the transaction: {found:?}");

    let charge = &run.charge;
    fs::write(
        results_path("bridge-private-claim.md"),
        format!(
            "# The private claim (`harness/tests/bridge_private.rs`), RISC Zero dev mode\n\n\
             Bridge program image `{}`. `ClaimPrivately` at the destination's own private account, for vault A's leaf 5 \
             (1 ETH, private kind) under `root(A, 8)`.\n\n\
             ## The private transitions in the executor\n\n{executed_table}\n\
             ## What LEZ charged\n\n\
             | execution gas | storage gas (tx bytes) | base fee | transaction bytes (dev mode: no real receipt) | proof bytes |\n\
             |---:|---:|---:|---:|---:|\n| {} | {} | {} | {} | {} |\n\n\
             ## Public outputs (`harness::private::describe_public_outputs`)\n\n{}\n\n## Search\n\n{report}",
            harness::image_id_hex(methods::FERRATA_BRIDGE_ID),
            harness::grouped(charge.gas_exec),
            harness::grouped(charge.gas_stor),
            harness::grouped(charge.fee_base as u64),
            harness::grouped(run.tx_bytes as u64),
            harness::grouped(run.proof_bytes as u64),
            describe_public_outputs(&run.tx),
        ),
    )
    .unwrap();
}

#[test]
fn a_private_claim_to_a_public_destination_and_the_refusals() {
    require_dev_mode();
    let mut w = World::with_roots();
    let (cid, va) = (w.cid_a, w.vault_a.address);
    // A private claim of the payer's public-kind deposit, run from a private account of its own:
    // the destination is minted publicly, the deposit stays hidden.
    let prover = w.chain.add_private_account(0x51, 1_000_000_000_000);
    let deposit = w.payer_deposit.clone();
    let message = w.claim_privately(&cid, 8, w.vault_a.witness(&deposit, 8));
    let publics = w.private_claim_actors(&cid, 8, &va, &deposit);
    let me = holder_actor(w.program, prover.id());
    let run = w
        .chain
        .prove_and_settle_private(
            &prover,
            me,
            borsh::to_vec(&message).unwrap(),
            publics.clone(),
            &catalog(),
            RESERVE,
            w.time,
        )
        .expect("settles");
    assert!(run.charge.revert.is_none());
    assert_eq!(w.balance(w.chain.payer, &cid), DENOMINATIONS[2], "10 ETH minted to the public destination");
    assert_eq!(w.balance(fee_recipient(), &cid), MINT_FEES_1[2], "leaf 6 is under schedule 1: its mint fee");
    let bytes = borsh::to_vec(&LeeTransaction::PrivacyPreserving(run.tx.clone())).unwrap();
    assert!(!occurs(&bytes, &deposit.leaf()) && !occurs(&bytes, &deposit.s), "which deposit paid it is hidden");
    assert!(occurs(&bytes, w.chain.payer.value()), "the public destination is visible");
    println!(
        "private claim, public destination: execution gas {}, storage gas {}",
        run.charge.gas_exec, run.charge.gas_stor
    );

    // The same claim again: the public part refuses (ALREADY_CLAIMED), so nothing can be proven.
    let again = w.chain.prove_and_settle_private(
        &prover,
        me,
        borsh::to_vec(&message).unwrap(),
        publics,
        &catalog(),
        RESERVE,
        w.time,
    );
    let err = again.err().expect("a replay cannot be proven");
    println!("replay: {err}");
    assert!(err.contains("250"), "{err}");

    // A private-kind deposit claimed by a private account that is not its destination.
    let other = w.chain.add_private_account(0x61, 1_000_000_000_000);
    let private = w.private_deposit.clone();
    let n = private.nullifier(CHAIN_ID, &va);
    let message = w.claim_privately(&cid, 8, w.vault_a.witness(&private, 8));
    let executed = private_claim_transitions(&w, other.id(), &message, &cid, 8);
    assert_eq!(executed.iter().map(|e| e.exit_code).collect::<Vec<_>>(), [0, 252], "WRONG_DESTINATION");
    let publics = w.private_claim_actors(&cid, 8, &va, &private);
    let err = w
        .chain
        .prove_and_settle_private(
            &other,
            holder_actor(w.program, other.id()),
            borsh::to_vec(&message).unwrap(),
            publics,
            &catalog(),
            RESERVE,
            w.time,
        )
        .err()
        .expect("not provable");
    println!("not the destination: {err}");
    assert!(err.contains("252"), "{err}");
    assert!(w.nullifier(&va, &n).is_none(), "nothing spent");
}
