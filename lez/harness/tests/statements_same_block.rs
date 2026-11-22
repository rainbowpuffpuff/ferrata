//! RFP-021 Reliability #11: "Statement submissions are independent of one another, so that
//! multiple private transactions can submit statements in the same block, and a private
//! transaction can compute its resulting state before inclusion."
//!
//! Four private statement submissions — two consumers reading the same finalized header's
//! record (one keeping its query to itself, one publishing it to the program), one verifying an
//! ERC-20 `Transfer` inside its own execution, one calling the program's attestation — are
//! proven in dev mode (`RISC0_DEV_MODE=1`) against one pre-state, none of them included yet.
//! They are then settled into one block, and on a second copy of the same pre-state into the
//! same block in the reverse order. Settlement is the sequencer's own
//! (`harness::settle::settle_private_transaction`) at one block id, against one opening fee
//! state and one shared block fee summary, as LEZ v0.4.0-rc1 ingests a block's user transactions
//! (`lez/chain_state/src/apply.rs`, the loop over a block's transactions); the harness's own
//! entry points instead give every transaction a block of its own (`Chain::block_id += 1`
//! after each settlement), so the block is held together here by a helper built from the
//! library's public pieces.
//!
//! The control is the dependence the claim excludes: two private transactions from one account,
//! both proven against the same pre-state, spend the same note — the block accepts the first
//! and refuses the second at the nullifier check.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use fee_core::{state::FeeState, BlockFeeSummary};
use ferrata_lez_wire::{
    consumer::{ConsumerMessage, ConsumerState, Deposit},
    Message,
};
use harness::{
    attest::{Executed, Stage4},
    consumer_account, grouped,
    private::{catalog, PrivateAccount, ALIAS_OPENING},
    settle::{classify, opening_fee_state, settle_private_transaction, Charge, LeeTransaction},
    Chain,
};
use lee::{
    execute_and_prove,
    privacy_preserving_transaction::{circuit::ProgramCatalog, Message as PrivateMessage, WitnessSet},
    test_utils::{no_seal, update_witness},
    Actor, BoundaryStep, PrivacyPreservingTransaction, ProvingInput, PublicExecutionContext, SenderPresentation,
    Simulation, TransactionEntry,
};
use lee_core::{program::TransactionEvent, Commitment, CommitmentSetDigest, FeeTransfer, Nullifier, RootCall};

/// The fee every statement submission reserves (the reserve of the private tests:
/// `tests/private_consumer.rs`, `tests/attest_private.rs`, `tests/vault_consumer.rs`).
const RESERVE: u128 = 100_000_000;

/// Two consumers' queries about the record they read (the first is `tests/private_consumer.rs`'s
/// QUERY); two different queries, so the two reads are two different statements.
const QUERY_A: &[u8] = b"storage(0x00000000219ab540356cBB839Cbe05303d7705Fa, slot 7) = deposit #4242";
const QUERY_B: &[u8] = b"storage(0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2, slot 2) = balance #77";

fn require_dev_mode() {
    assert_eq!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1"), "run these tests with RISC0_DEV_MODE=1");
}

/// One private statement submission, proven but not yet included anywhere.
#[derive(Clone)]
struct Proven {
    what: &'static str,
    /// The transaction as it will be submitted.
    tx: LeeTransaction,
    /// The commitment-tree root its private actions are anchored at: the root of the state it
    /// was proven against, which by inclusion need no longer be the tree's current root.
    root: CommitmentSetDigest,
    /// What inclusion will make public, computed here before inclusion: the new commitments and
    /// the nullifiers its private actions carry.
    commitments: Vec<Commitment>,
    nullifiers: Vec<Nullifier>,
}

/// The proving half of `Chain::prove_and_settle_private_shaped` in the bare shape, the shape
/// every private transaction of these tests is built in (`harness/src/private.rs`): prove
/// `message` at `sender`'s actor of the consumer program, with `reserve` paid from the sender's
/// native balance inside the proof, against the state the chain is in — and settle nothing, so
/// that several transactions can be proven against one pre-state. The state is only read.
#[allow(clippy::too_many_arguments)]
fn prove_private(
    chain: &Chain,
    sender: &PrivateAccount,
    root: Actor,
    message: Vec<u8>,
    mut public_actors: Vec<Actor>,
    programs: &ProgramCatalog,
    reserve: u128,
    what: &'static str,
) -> Result<Proven, String> {
    public_actors.push(Actor::native_balance(system_accounts::fee_inbox_account_id()));
    let public_actor_states: HashMap<Actor, _> = public_actors.iter().map(|a| (*a, chain.actor_state(*a))).collect();
    let membership = chain
        .state
        .get_proof_for_commitment(&Commitment::new(&sender.id(), &sender.account))
        .ok_or("the sender's commitment is not in the state")?;
    let input = ProvingInput {
        root: TransactionEntry::Sync(RootCall { to: root, message }),
        claims: Vec::new(),
        fee: Some(FeeTransfer {
            payer: sender.id(),
            recipient: system_accounts::fee_inbox_account_id(),
            amount: reserve,
            refund_to: Some(chain.payer),
        }),
        public_context: PublicExecutionContext::new(public_actors, []),
        private_witnesses: vec![update_witness(&sender.keys, sender.account.clone(), membership)],
        dummy_inputs: Vec::new(),
        ciphertext_padding: None,
        recoveries: Vec::new(),
        private_promotions: BTreeSet::new(),
    };
    let simulation = Simulation { public_actor_states, admitted_accounts: None };
    let (output, proof) = execute_and_prove(
        input,
        &simulation,
        programs,
        |_| SenderPresentation::Blinded(ALIAS_OPENING),
        |_, _| false,
        no_seal,
    )
    .map_err(|e| format!("proving failed: {e}"))?;
    let message = PrivateMessage::from_circuit_output(BTreeMap::new(), output);
    let tx = PrivacyPreservingTransaction::new(message, WitnessSet::from_raw_parts(vec![], proof));
    let execution = &tx.message.execution;
    let state_root = chain.state.commitment_root();
    for action in &execution.private_actions {
        assert_eq!(action.root, state_root, "{what}: the proof is anchored at the state's current root");
    }
    let commitments = execution.commitments();
    let nullifiers = execution.nullifiers().into_iter().map(|(n, _)| n).collect::<Vec<_>>();
    Ok(Proven { what, tx: LeeTransaction::PrivacyPreserving(tx), root: state_root, commitments, nullifiers })
}

/// Settle one proven transaction into the block the chain is in, without advancing the block:
/// the same call the harness makes (`harness/src/private.rs` settlement half), except that the
/// block id, the block's opening fee state and its fee summary are the caller's, shared by the
/// block's transactions as LEZ's sequencer shares them (`lez/chain_state/src/apply.rs`: one
/// `opening_fee_state` and one `BlockFeeSummary` per block, one block id for all its user
/// transactions).
fn settle_into_block(
    chain: &mut Chain,
    opening: &FeeState,
    summary: &mut BlockFeeSummary,
    tx: &LeeTransaction,
    timestamp_ms: u64,
) -> Result<(Vec<TransactionEvent>, Charge), String> {
    let view = classify(tx)?;
    settle_private_transaction(tx, &view, &mut chain.state, opening, chain.block_id, timestamp_ms, summary)
}

/// What `s` made public on inclusion is what its proof computed: every commitment it named is
/// now in the commitment tree, every nullifier it spent is spent.
fn made_public(chain: &Chain, s: &Proven) {
    for commitment in &s.commitments {
        assert!(
            chain.state.get_proof_for_commitment(commitment).is_some(),
            "{}: its new commitment is not in the tree",
            s.what
        );
    }
    for nullifier in &s.nullifiers {
        assert!(chain.state.is_spent(nullifier), "{}: its nullifier is not spent", s.what);
    }
}

/// Settle `statements` into one block of `chain`, in this order, and check each: `chain` must
/// hold the pre-state all of them were proven against. Every settlement must be accepted at the
/// block's id (the block never advances between them), with no events — a record's reply goes
/// back to its caller and the native fee credit emits nothing (`lez/guests/program/src/main.rs`
/// `read`, and `attest` there replies to a caller instead of emitting an event when it has one,
/// as both readers and the attestation caller here do) — and its public effect must be exactly
/// the commitments and nullifiers its proof carried.
fn settle_block(chain: &mut Chain, statements: &[Proven], timestamp_ms: u64) -> Vec<(Vec<TransactionEvent>, Charge)> {
    let block = chain.block_id;
    let opening = opening_fee_state(&chain.state);
    let mut summary = BlockFeeSummary::default();
    let mut out = Vec::new();
    for s in statements {
        let (events, charge) = settle_into_block(chain, &opening, &mut summary, &s.tx, timestamp_ms)
            .unwrap_or_else(|e| panic!("{}: the block refused it: {e}", s.what));
        assert!(charge.revert.is_none(), "{}: reverted: {:?}", s.what, charge.revert);
        assert_eq!(chain.block_id, block, "one block holds several transactions; it does not advance");
        assert!(events.is_empty(), "{}: no handler emits an event here: {events:?}", s.what);
        made_public(chain, s);
        out.push((events, charge));
    }
    chain.block_id += 1; // the sequencer opens the next block after the last of this one
    out
}

/// The consumer state the last of `executed` transitions kept (`tests/attest_private.rs` keeps
/// the same helper).
fn kept(executed: &[Executed]) -> Option<ConsumerState> {
    let state = executed.last()?.transition.as_ref()?.response.post_state.clone()?;
    Some(borsh::from_slice(&state).expect("a consumer state"))
}

#[test]
fn private_statements_settle_in_one_block_in_either_order() {
    require_dev_mode();
    let mut s = Stage4::new();
    let slot = s.fx.finalized_slot;
    let header = s.header(slot);
    let mut accounts = Vec::new();
    for seed in [0xa1, 0xa2, 0xa3, 0xa4] {
        accounts.push(s.w.chain.add_private_account(seed, 1_000_000_000_000));
    }
    let t = s.fx.transfer.clone();
    let pool = s.pool();

    // The four statements, as their submitters build them.
    let read = borsh::to_vec(&ConsumerMessage::ReadRecord {
        ferrata: s.w.inst.program,
        instance_id: s.w.inst.id,
        slot,
        query: QUERY_A.to_vec(),
    })
    .unwrap();
    let padded = borsh::to_vec(&ConsumerMessage::ReadPadded {
        ferrata: s.w.inst.program,
        instance_id: s.w.inst.id,
        slot,
        padding: QUERY_B.to_vec(),
    })
    .unwrap();
    let private_attest = s.private_message(&pool, slot, &t);
    let request = pool.request(t.tx_index, t.log_index, s.fx.receipt_proof(t.tx_index), None);
    // What the program will answer the call with, computed here, before any inclusion.
    let expected = s.expected(slot, &request).unwrap();
    let call =
        borsh::to_vec(&ConsumerMessage::CallAttest { ferrata: s.w.inst.program, slot, request: request.clone() })
            .unwrap();

    // A private transaction computes its resulting state before inclusion: the attestation's
    // two private transitions, in the executor, keep the verified statement (as
    // `tests/attest_private.rs` checks the same message).
    let executed = s.private_transitions(accounts[2].id(), slot, &private_attest);
    assert_eq!(executed.iter().map(|e| e.exit_code).collect::<Vec<_>>(), [0, 0]);
    let deposit =
        Deposit { statement_id: s.transfer_id(&t), block_number: s.fx.block, owner: t.from, amount: t.amount() };
    let state = kept(&executed).expect("the consumer keeps its result");
    let ConsumerState::Private(private) = state else { panic!("a private actor's state") };
    assert_eq!(private.last, Some((pool.id(), deposit)), "the fact was verified and kept");
    assert_eq!((private.awaiting, private.pending, private.attested), (None, None, 1), "the query is not kept");

    // All four proven against the same pre-state, before any of them is included.
    let programs = catalog();
    let pre_root = s.w.chain.state.commitment_root();
    let prove = |chain: &Chain, account: &PrivateAccount, message: Vec<u8>, what: &'static str| {
        let root = Actor::new(account.id(), consumer_account());
        let p = prove_private(chain, account, root, message, vec![header], &programs, RESERVE, what)
            .unwrap_or_else(|e| panic!("{what}: {e}"));
        assert_eq!(p.root, pre_root, "{what}: proven against the pre-state");
        p
    };
    let proven = [
        prove(&s.w.chain, &accounts[0], read, "a consumer reads the record, its query kept to itself"),
        prove(&s.w.chain, &accounts[1], padded, "a consumer publishes its query to the record"),
        prove(
            &s.w.chain,
            &accounts[2],
            borsh::to_vec(&private_attest).unwrap(),
            "a consumer attests a Transfer in its own execution",
        ),
        prove(&s.w.chain, &accounts[3], call, "a consumer calls the program's attestation"),
    ];
    for (i, a) in proven.iter().enumerate() {
        for b in proven.iter().skip(i + 1) {
            assert_ne!(a.nullifiers, b.nullifiers, "independent submissions spend different notes");
            assert_ne!(a.commitments, b.commitments, "independent submissions commit different states");
        }
    }

    // What the call will make public, recorded before inclusion: the request in the clear, and
    // the pre-computed attestation as its answer. At settlement LEZ replays the public part
    // against the live state and refuses a boundary that differs from it (`CrossMessageMismatch`,
    // `lee/state_machine/core/src/execution_state.rs`), so acceptance is that equality.
    let LeeTransaction::PrivacyPreserving(call_tx) = &proven[3].tx else { unreachable!("a private transaction") };
    let sent = borsh::to_vec(&Message::Attest { request }).unwrap();
    let answer = borsh::to_vec(&expected).unwrap();
    assert!(
        call_tx.message.execution.boundary.iter().any(|step| matches!(step,
            BoundaryStep::PrivateToPublic(d) if d.envelope.message == sent)),
        "the request crosses into public execution in the clear"
    );
    assert!(
        call_tx.message.execution.boundary.iter().any(|step| matches!(step,
            BoundaryStep::PublicToPrivate(d) if d.envelope.message == answer)),
        "the pre-computed attestation is the answer the transaction carries"
    );

    // The same block, in one order: the pre-state all four were proven against, kept for the
    // reversed block below.
    let mut reverse = s.w.chain.clone();
    let forward = settle_block(&mut s.w.chain, &proven, s.time);
    // Every settlement but the first includes a transaction proven at a root that is no longer
    // the tree's current one. LEZ accepts it because a private action's root has to be a root
    // the commitment tree has actually had, not the current one: `is_known_commitment_root`
    // reads the root history that `append` records at every insertion
    // (`lee/state_machine/src/state/mod.rs`), which this test shows is enough for several
    // statements of one block — the rule allows any historical root.
    assert_ne!(s.w.chain.state.commitment_root(), pre_root, "the block's settlements moved the root");

    // The same block on a second copy of the same pre-state, in the reverse order.
    let reversed: Vec<Proven> = proven.iter().rev().cloned().collect();
    let backward = settle_block(&mut reverse, &reversed, s.time);
    assert_ne!(reverse.state.commitment_root(), pre_root);

    // Order independence: every statement's charge and surfaced events are the same in both
    // orders, and every account the transactions touch ends in the same state.
    for ((f_events, f_charge), (b_events, b_charge)) in forward.iter().zip(backward.iter().rev()) {
        assert_eq!(f_charge, b_charge, "the charge does not depend on the block's order");
        assert_eq!(f_events, b_events, "the surfaced events do not depend on the block's order");
    }
    let touched = [
        ("the fee payer, which receives the refunds", s.w.chain.payer),
        ("the fee inbox, credited every reserve", system_accounts::fee_inbox_account_id()),
        ("the fee state account", system_accounts::fee_state_account_id()),
        ("the fee escrow", system_accounts::fee_escrow_account_id()),
        ("the record all four statements read", header.account_id),
        ("the attestations' program", s.w.inst.program),
    ];
    for (what, id) in touched {
        assert_eq!(
            s.w.chain.state.get_account_by_id(id),
            reverse.state.get_account_by_id(id),
            "{what}: the same final state in both orders"
        );
    }
    for (statement, (_, charge)) in proven.iter().zip(forward.iter()) {
        println!(
            "{}: execution gas {}, storage gas {}",
            statement.what,
            grouped(charge.gas_exec),
            grouped(charge.gas_stor)
        );
    }
}

/// The control: the dependence the requirement's clause excludes. Two private transactions
/// proven against one pre-state from ONE account spend the same note — the update nullifier is
/// derived from the spent commitment and the nullifier key alone
/// (`Nullifier::for_account_update`), so both proofs spend the same one — and the block accepts
/// the first and refuses the second at the nullifier check (`check_nullifiers_are_valid`,
/// `lee/state_machine/src/state/mod.rs`: "Nullifier already seen"). The refusal changes nothing.
#[test]
fn the_second_spend_of_the_same_note_in_one_block_is_refused() {
    require_dev_mode();
    let mut s = Stage4::new();
    let slot = s.fx.finalized_slot;
    let header = s.header(slot);
    let account = s.w.chain.add_private_account(0xb1, 1_000_000_000_000);
    let root = Actor::new(account.id(), consumer_account());
    let read = |query: &[u8]| {
        borsh::to_vec(&ConsumerMessage::ReadRecord {
            ferrata: s.w.inst.program,
            instance_id: s.w.inst.id,
            slot,
            query: query.to_vec(),
        })
        .unwrap()
    };
    let programs = catalog();
    // Two different statements (different queries) with different reserves, so the two
    // transactions are distinct and commit different post-states: what conflicts is the spent
    // note, not the rest of the transaction.
    let first = prove_private(
        &s.w.chain,
        &account,
        root,
        read(QUERY_A),
        vec![header],
        &programs,
        RESERVE,
        "the first read of the account",
    )
    .unwrap();
    let second = prove_private(
        &s.w.chain,
        &account,
        root,
        read(QUERY_B),
        vec![header],
        &programs,
        2 * RESERVE,
        "the second read of the same account",
    )
    .unwrap();
    assert_eq!(first.nullifiers, second.nullifiers, "both spend the same note of the same account");
    assert_ne!(first.commitments, second.commitments, "they commit different post-states");

    let block = s.w.chain.block_id;
    let opening = opening_fee_state(&s.w.chain.state);
    let mut summary = BlockFeeSummary::default();
    let (_, charge) =
        settle_into_block(&mut s.w.chain, &opening, &mut summary, &first.tx, s.time).expect("the first spend settles");
    assert!(charge.revert.is_none());
    assert_eq!(s.w.chain.block_id, block);
    let settled =
        (s.w.chain.state.commitment_root(), s.w.chain.state.get_account_by_id(system_accounts::fee_inbox_account_id()));
    let err = settle_into_block(&mut s.w.chain, &opening, &mut summary, &second.tx, s.time)
        .expect_err("the same note is spent only once");
    assert!(err.contains("Nullifier already seen"), "refused at the nullifier check, not elsewhere: {err}");
    // The refused transaction changed nothing.
    assert_eq!(s.w.chain.state.commitment_root(), settled.0, "no commitment moved");
    assert_eq!(s.w.chain.state.get_account_by_id(system_accounts::fee_inbox_account_id()), settled.1, "no fee moved");
    made_public(&s.w.chain, &first);
}
