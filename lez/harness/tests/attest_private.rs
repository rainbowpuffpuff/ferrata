//! Stage 4, the private path: a consumer executed privately reads the record of a real finalized
//! mainnet block (`Read` → `StateReply`) and verifies an ERC-20 `Transfer` log against it inside
//! its own execution, with the function the program runs for a public call
//! (`ferrata_lez_wire::attest::evaluate`). Dev mode (`RISC0_DEV_MODE=1`): the private transitions
//! and LEZ's privacy-preserving circuit are executed, not proven; the settlement and the public
//! part are the real ones.
//!
//! What must hold (`docs/design/attestation.md` §6.2): the only things that cross into public
//! execution are the one-byte `Read`, the record in the reply, and the fee; the query, its proof,
//! the fact and its statement identifier appear nowhere in the serialized transaction.

use std::{fs, path::PathBuf};

use ferrata_lez_wire::{
    attest::Request,
    consumer::{code as consumer_code, ConsumerState, Deposit, Pool},
    Message, RecordState,
};
use harness::{
    attest::{Executed, Stage4, Transfer},
    private::*,
    settle::LeeTransaction,
    *,
};
use lee::Actor;
use lee_core::{execution_state::BoundaryStep, native_token, program::StateReply};

fn require_dev_mode() {
    assert_eq!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1"), "run these tests with RISC0_DEV_MODE=1");
}

fn occurs(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Everything the private attestation of `t` in `pool` must keep out of the transaction.
fn private_inputs(s: &Stage4, account: &PrivateAccount, pool: &Pool, t: &Transfer) -> Vec<(String, Vec<u8>)> {
    let keys = &account.keys;
    let proof = s.fx.receipt_proof(t.tx_index);
    let mut inputs = vec![
        ("the token address".to_string(), t.token.to_vec()),
        ("the transfer's sender".to_string(), t.from.to_vec()),
        ("the transfer's recipient".to_string(), t.to.to_vec()),
        ("the amount (its 32-byte word)".to_string(), t.value.to_vec()),
        ("the statement identifier".to_string(), s.transfer_id(t).to_vec()),
        ("the pool id".to_string(), pool.id().to_vec()),
        ("the account id the consumer acts for".to_string(), account.id().value().to_vec()),
        ("the authorization secret key".to_string(), keys.ask.0.to_vec()),
        ("the nullifier secret key".to_string(), keys.nsk().to_vec()),
        ("the nullifier public key".to_string(), keys.npk().0.to_vec()),
        ("the viewing public key (its first 32 of 1,184 bytes)".to_string(), keys.vpk().to_bytes()[..32].to_vec()),
        ("the viewing key seed d".to_string(), keys.d.to_vec()),
        ("the viewing key seed z".to_string(), keys.z.to_vec()),
    ];
    for (i, node) in proof.iter().enumerate() {
        inputs.push((format!("proof node {i} (its first 32 of {} bytes)", node.len()), node[..32].to_vec()));
        inputs.push((format!("proof node {i} (its last 32 bytes)"), node[node.len() - 32..].to_vec()));
    }
    inputs
}

/// The deposit the consumer keeps after the reply, from the executor's run of its two transitions.
fn kept(executed: &[Executed]) -> Option<(ConsumerState, usize)> {
    let state = executed.last()?.transition.as_ref()?.response.post_state.clone()?;
    Some((borsh::from_slice(&state).expect("a consumer state"), state.len()))
}

#[test]
fn a_private_consumer_attests_a_transfer_and_only_the_read_is_public() {
    require_dev_mode();
    let mut s = Stage4::new();
    let slot = s.fx.finalized_slot;
    let t = s.fx.transfer.clone();
    let pool = s.pool();
    let header = s.header(slot);
    let record = s.record(slot);
    let account = s.w.chain.add_private_account(0x81, 1_000_000_000_000);
    let message = s.private_message(&pool, slot, &t);

    // The two private transitions in the executor: what a prover has to prove for the consumer.
    let executed = s.private_transitions(account.id(), slot, &message);
    println!("{}", harness::attest::render_executed(&executed));
    assert_eq!(executed.iter().map(|e| e.exit_code).collect::<Vec<_>>(), [0, 0]);
    let deposit =
        Deposit { statement_id: s.transfer_id(&t), block_number: s.fx.block, owner: t.from, amount: t.amount() };
    let (state, state_bytes) = kept(&executed).expect("the consumer keeps its result");
    let ConsumerState::Private(private) = state else { panic!("a private actor's state") };
    assert_eq!(private.last, Some((pool.id(), deposit)), "the fact was verified and kept");
    assert_eq!((private.awaiting, private.pending, private.attested), (None, None, 1), "the query is not kept");

    // The same through LEZ: proven (dev mode) with the privacy-preserving circuit, settled.
    let root = Actor::new(account.id(), consumer_account());
    let run =
        s.w.chain
            .prove_and_settle_private(
                &account,
                root,
                borsh::to_vec(&message).unwrap(),
                vec![header],
                &catalog(),
                100_000_000,
                s.time,
            )
            .expect("settles");
    assert!(run.charge.revert.is_none());

    // What crosses into public execution: `Read` to the record, the record back, the fee.
    let boundary = &run.tx.message.execution.boundary;
    let read = borsh::to_vec(&Message::Read).unwrap();
    assert_eq!(read.len(), 1);
    let fee_inbox = Actor::native_balance(system_accounts::fee_inbox_account_id());
    let mut outgoing = Vec::new();
    let mut incoming = Vec::new();
    for step in boundary {
        match step {
            BoundaryStep::PrivateToPublic(d) => outgoing.push((d.envelope.to, d.envelope.message.clone())),
            BoundaryStep::PublicToPrivate(d) => incoming.push((d.envelope.from, d.envelope.message.clone())),
            BoundaryStep::EndPrivateSubtree | BoundaryStep::EndPublicSubtree => {}
        }
    }
    assert_eq!(outgoing.len(), 2, "the read and the fee: {boundary:?}");
    assert_eq!(outgoing[0], (header, read), "one fixed-size Read to the record");
    assert_eq!(outgoing[1].0, fee_inbox, "the fee transfer");
    assert!(matches!(borsh::from_slice(&outgoing[1].1), Ok(native_token::Message::Credit(_))));
    assert_eq!(incoming.len(), 1);
    assert_eq!(incoming[0].0, header, "the reply comes from the record");
    let reply: StateReply = borsh::from_slice(&incoming[0].1).unwrap();
    assert_eq!(borsh::from_slice::<RecordState>(&reply.state).unwrap(), record, "the reply is the record");
    // No public state of the consumer exists: nothing was written for it.
    assert!(s.claim_state(&pool, &s.transfer_id(&t)).is_none() && s.balance(&pool, &t.from).is_none());

    // Nothing of the query, its proof or its result is in the transaction.
    let bytes = borsh::to_vec(&LeeTransaction::PrivacyPreserving(run.tx.clone())).unwrap();
    let mut report = format!(
        "The {} serialized bytes of the transaction were searched for the consumer's private inputs:\n",
        bytes.len()
    );
    for (input, needle) in private_inputs(&s, &account, &pool, &t) {
        let found = occurs(&bytes, &needle);
        report.push_str(&format!("- {input} ({} bytes): {}\n", needle.len(), if found { "FOUND" } else { "absent" }));
        assert!(!found, "{input} is in the transaction");
    }
    let cycles: u64 = executed.iter().map(|e| e.cycles).sum();
    let fields = format!(
        "Stage 4: a private consumer attests an ERC-20 Transfer of mainnet block {} against the record it reads\n\n\
         Decoded public fields (harness::private::describe_public_outputs):\n\n{}\n{report}\n\
         Charged: execution gas {} (PRIVATE_VERIFY_GAS 409,764 + public part {} cycles), storage gas {}.\n\
         The transaction is {} bytes; its proof field is RISC Zero's fake receipt ({} bytes, dev mode).\n\n\
         The consumer's private transitions, in the executor (no proof; software Keccak):\n\n{}\n\
         Together {} user cycles. The private state the consumer keeps afterwards is {} bytes, whatever the proof was.\n",
        s.fx.block,
        describe_public_outputs(&run.tx),
        grouped(run.charge.gas_exec),
        grouped(run.charge.cycles),
        grouped(run.charge.gas_stor),
        grouped(run.tx_bytes as u64),
        grouped(run.proof_bytes as u64),
        harness::attest::render_executed(&executed),
        grouped(cycles),
        state_bytes,
    );
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../results");
    fs::write(dir.join("stage4-attest-private-public-outputs.txt"), &fields).unwrap();
    println!("{fields}");
}

#[test]
fn a_private_attestation_that_does_not_verify_cannot_be_proven() {
    require_dev_mode();
    let mut s = Stage4::new();
    let slot = s.fx.finalized_slot;
    let t = s.fx.transfer.clone();
    let pool = s.pool();
    let header = s.header(slot);
    let account = s.w.chain.add_private_account(0x82, 1_000_000_000_000);
    let root = Actor::new(account.id(), consumer_account());

    // (what, message, slot read, exit code of the consumer's reply transition)
    let mut forged = s.private_message(&pool, slot, &t);
    if let ferrata_lez_wire::consumer::ConsumerMessage::AttestPrivately { proof, .. } = &mut forged {
        let last = proof.len() - 1;
        let middle = proof[last].len() / 2;
        proof[last][middle] ^= 0x01;
    }
    let other_recipient = s.private_message(&pool, slot, &s.fx.other_recipient.clone());
    let other_chain = s.private_message(&Pool { chain_id: 11_155_111, ..pool.clone() }, slot, &t);
    let other_record = s.private_message(&pool, s.fx.bootstrap_slot, &t);
    let no_record = s.private_message(&pool, slot + 1, &t);
    let cases = [
        ("a forged receipt", forged, slot, 110u32),
        ("a Transfer to another recipient", other_recipient, slot, 130),
        ("a pool configured for another chain id", other_chain, slot, 140),
        ("a proof valid for another header (the bootstrap's record)", other_record, s.fx.bootstrap_slot, 110),
        ("a slot without a record", no_record, slot + 1, u32::from(consumer_code::BAD_REPLY)),
    ];
    for (what, message, read_slot, code) in cases {
        // In the executor: the reply transition exits with the code.
        let executed = s.private_transitions(account.id(), read_slot, &message);
        assert_eq!(executed.len(), 2, "{what}");
        assert_eq!((executed[0].exit_code, executed[1].exit_code), (0, code), "{what}");
        // Through LEZ's prover: a transition that exits non-zero has no receipt, so there is no
        // transaction to submit.
        let public = if read_slot == slot { header } else { s.header(read_slot) };
        let result = s.w.chain.prove_and_settle_private(
            &account,
            root,
            borsh::to_vec(&message).unwrap(),
            vec![public],
            &catalog(),
            100_000_000,
            s.time,
        );
        let err = result.err().unwrap_or_else(|| panic!("{what}: proven"));
        println!("{what}: {err}");
        assert!(err.contains(&format!("code {code}")), "{what}: {err}");
    }
    s.w.check_store();
}

#[test]
fn a_private_consumer_calling_attest_publishes_its_request() {
    require_dev_mode();
    let mut s = Stage4::new();
    let slot = s.fx.finalized_slot;
    let t = s.fx.transfer.clone();
    let pool = s.pool();
    let header = s.header(slot);
    let account = s.w.chain.add_private_account(0x83, 1_000_000_000_000);
    let request: Request = pool.request(t.tx_index, t.log_index, s.fx.receipt_proof(t.tx_index), None);
    let expected = s.expected(slot, &request).unwrap();
    let call = ferrata_lez_wire::consumer::ConsumerMessage::CallAttest {
        ferrata: s.w.inst.program,
        slot,
        request: request.clone(),
    };
    let run =
        s.w.chain
            .prove_and_settle_private(
                &account,
                Actor::new(account.id(), consumer_account()),
                borsh::to_vec(&call).unwrap(),
                vec![header],
                &catalog(),
                100_000_000,
                s.time,
            )
            .expect("settles");
    assert!(run.charge.revert.is_none());
    // The request crosses into public execution in the clear, and so does the answer.
    let sent = borsh::to_vec(&Message::Attest { request }).unwrap();
    let answer = borsh::to_vec(&expected).unwrap();
    let boundary = &run.tx.message.execution.boundary;
    assert!(boundary.iter().any(|step| matches!(step, BoundaryStep::PrivateToPublic(d) if d.envelope.message == sent)));
    assert!(boundary
        .iter()
        .any(|step| matches!(step, BoundaryStep::PublicToPrivate(d) if d.envelope.message == answer)));
    let bytes = borsh::to_vec(&LeeTransaction::PrivacyPreserving(run.tx.clone())).unwrap();
    for (what, needle) in [
        ("the token address", t.token.to_vec()),
        ("the transfer's sender", t.from.to_vec()),
        ("the amount", t.value.to_vec()),
        ("the statement identifier", s.transfer_id(&t).to_vec()),
    ] {
        assert!(occurs(&bytes, &needle), "{what} is published by a call");
    }
    // The consumer's own keys and account still are not.
    assert!(!occurs(&bytes, account.id().value()) && !occurs(&bytes, &account.keys.nsk()));
    println!(
        "a private call: execution gas {} (public part {} cycles), storage gas {}; the {}-byte request and the \
         {}-byte attestation are in the transaction",
        grouped(run.charge.gas_exec),
        grouped(run.charge.cycles),
        grouped(run.charge.gas_stor),
        sent.len(),
        answer.len()
    );
}
