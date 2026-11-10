//! Stage 3: a consumer actor of another program, executed privately, that reads one
//! `header(id, slot)` record with LEZ v0.4's read idiom (`Read` → `StateReply`), and the same
//! consumer sending its query to the program instead. Dev mode (`RISC0_DEV_MODE=1`): executed, not
//! proven; settlement and the public part are the real ones.
//!
//! Each test writes the whole serialized transaction (`lez/results/stage3-*-tx.hex`) and its
//! decoded public fields (`lez/results/stage3-*-public-outputs.txt`), and checks the bytes for
//! the consumer's private inputs: the query, the account id it acts for, its keys. A read must
//! contain none of them; a call contains the query and nothing else.

use std::fs;

use ferrata_lez_wire::{consumer::ConsumerMessage, header_actor, RecordState};
use harness::{private::*, scenarios::World, settle::LeeTransaction, *};
use lee::Actor;
use lee_core::{
    execution_state::BoundaryStep,
    program::{immutable_mirror_commitment, StateReply, PROGRAM_LOADER_ACCOUNT_ID},
};

/// The consumer's query: what it wants to know about the record it reads.
const QUERY: &[u8] = b"storage(0x00000000219ab540356cBB839Cbe05303d7705Fa, slot 7) = deposit #4242";

fn require_dev_mode() {
    assert_eq!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1"), "run these tests with RISC0_DEV_MODE=1");
}

/// A world with records at the checkpoint slot and at update_1's finalized slot.
fn world() -> World {
    let mut w = World::new(2);
    let u0 = w.updates[0].clone();
    w.apply(&u0, "update_0");
    let u1 = as_finality_update(&w.updates[1]);
    w.apply(&u1, "update_1 as finality update");
    w
}

/// The `StateReply` the read brought into the private part, from the boundary.
fn reply_from_boundary(run: &PrivateRun) -> Option<RecordState> {
    run.tx.message.execution.boundary.iter().find_map(|step| match step {
        BoundaryStep::PublicToPrivate(d) => {
            let reply: StateReply = borsh::from_slice(&d.envelope.message).ok()?;
            borsh::from_slice(&reply.state).ok()
        }
        _ => None,
    })
}

/// The consumer's private inputs, by name.
fn private_inputs(account: &PrivateAccount) -> Vec<(&'static str, Vec<u8>)> {
    let keys = &account.keys;
    vec![
        ("the query", QUERY.to_vec()),
        ("the account id the consumer acts for", account.id().value().to_vec()),
        ("the authorization secret key", keys.ask.0.to_vec()),
        ("the nullifier secret key", keys.nsk().to_vec()),
        ("the nullifier public key", keys.npk().0.to_vec()),
        ("the viewing public key (its first 32 of 1,184 bytes)", keys.vpk().to_bytes()[..32].to_vec()),
        ("the viewing key seed d", keys.d.to_vec()),
        ("the viewing key seed z", keys.z.to_vec()),
    ]
}

fn occurs(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Search the whole serialized transaction for each private input; `allowed` may occur. Writes
/// the hex dump and the decoded public fields under `lez/results/`, with the search's result.
fn check_and_dump(name: &str, title: &str, run: &PrivateRun, account: &PrivateAccount, allowed: &[&str]) {
    let bytes = borsh::to_vec(&LeeTransaction::PrivacyPreserving(run.tx.clone())).unwrap();
    let mut report = format!(
        "The {} serialized bytes of the transaction were searched for the consumer's private inputs:\n",
        bytes.len()
    );
    for (input, needle) in private_inputs(account) {
        let found = occurs(&bytes, &needle);
        report.push_str(&format!("- {input} ({} bytes): {}\n", needle.len(), if found { "FOUND" } else { "absent" }));
        assert_eq!(found, allowed.contains(&input), "{name}: {input} found = {found}");
    }
    let dir = harness::results_dir();
    let mut hex_dump = format!(
        "# {title}\n# The complete borsh serialization of the LeeTransaction ({} bytes): its message (the public\n# outputs of the privacy-preserving circuit, nonces, admission evidence) and its witness set (signatures,\n# proof). RISC Zero dev mode: the proof is a fake receipt ({} bytes), not a succinct receipt.\n# Written by lez/harness/tests/private_consumer.rs. Offsets in hex, 32 bytes per line.\n",
        bytes.len(),
        run.proof_bytes
    );
    for (i, chunk) in bytes.chunks(32).enumerate() {
        hex_dump.push_str(&format!("{:06x}  {}\n", i * 32, hex::encode(chunk)));
    }
    fs::write(dir.join(format!("{name}-tx.hex")), hex_dump).unwrap();
    let fields = format!(
        "{title}\n\nDecoded public fields (harness::private::describe_public_outputs):\n\n{}\n{report}\nCharged: execution gas {} (PRIVATE_VERIFY_GAS 409,764 + public part {} cycles), storage gas {}.\n",
        describe_public_outputs(&run.tx),
        grouped(run.charge.gas_exec),
        grouped(run.charge.cycles),
        grouped(run.charge.gas_stor),
    );
    fs::write(dir.join(format!("{name}-public-outputs.txt")), &fields).unwrap();
    println!("{fields}");
}

#[test]
fn a_private_consumer_reads_a_record() {
    require_dev_mode();
    let mut w = world();
    let slot = w.updates[1].finalized_header.beacon.slot;
    let header = header_actor(w.inst.program, &w.inst.id, slot);
    let record = w.inst.record(&w.chain, slot).expect("the record exists");
    let time = w.inst.config.slot_start_ms(w.updates[2].signature_slot) + 12_000;
    let read = borsh::to_vec(&ConsumerMessage::ReadRecord {
        ferrata: w.inst.program,
        instance_id: w.inst.id,
        slot,
        query: QUERY.to_vec(),
    })
    .unwrap();

    // Read, with the consumer's image disclosed (the default claim).
    let reader = w.chain.add_private_account(0x71, 1_000_000_000_000);
    let run = w
        .chain
        .prove_and_settle_private(
            &reader,
            Actor::new(reader.id(), consumer_account()),
            read.clone(),
            vec![header],
            &catalog(),
            100_000_000,
            time,
        )
        .expect("settles");
    assert!(run.charge.revert.is_none());
    assert_eq!(reply_from_boundary(&run), Some(record.clone()), "the reply carries the record");
    check_and_dump(
        "stage3-read",
        "Stage 3: a private consumer reads header(id, slot), image disclosed",
        &run,
        &reader,
        &[],
    );

    // Read, with the consumer's image undisclosed: the claim names only a commitment root, but
    // the boundary still names the sender's program.
    let reader2 = w.chain.add_private_account(0x72, 1_000_000_000_000);
    let program_header = harness::program_header(
        w.chain.state.get_account_by_id(consumer_account()).data.actor_state(PROGRAM_LOADER_ACCOUNT_ID),
    )
    .expect("the consumer is deployed");
    assert!(program_header.immutable);
    let membership = w
        .chain
        .state
        .get_proof_for_commitment(&immutable_mirror_commitment(consumer_account(), &program_header))
        .expect("an immutable program is mirrored into the commitment tree");
    let undisclosed = catalog().with_undisclosed(consumer_account(), program_header, membership);
    let run = w
        .chain
        .prove_and_settle_private(
            &reader2,
            Actor::new(reader2.id(), consumer_account()),
            read,
            vec![header],
            &undisclosed,
            100_000_000,
            time,
        )
        .expect("settles");
    assert!(run.charge.revert.is_none());
    check_and_dump(
        "stage3-read-undisclosed",
        "Stage 3: a private consumer reads header(id, slot), image undisclosed",
        &run,
        &reader2,
        &[],
    );
    let names_consumer = run.tx.message.execution.boundary.iter().any(|step| {
        matches!(step, BoundaryStep::PrivateToPublic(d) if d.envelope.from.program_account_id == consumer_account())
    });
    assert!(names_consumer, "the read's sender is named with its program in the boundary");
}

#[test]
fn a_private_consumer_calling_the_program_publishes_what_it_sends() {
    require_dev_mode();
    let mut w = world();
    let slot = w.updates[1].finalized_header.beacon.slot;
    let header = header_actor(w.inst.program, &w.inst.id, slot);
    let time = w.inst.config.slot_start_ms(w.updates[2].signature_slot) + 12_000;
    let caller = w.chain.add_private_account(0x73, 1_000_000_000_000);
    // The same read with the query sent along (`ReadPadded`, which the program ignores): whatever
    // a private execution sends to a public actor is published. The real call, `Attest` with a
    // request and its proofs, is in `attest_private.rs`.
    let call = borsh::to_vec(&ConsumerMessage::ReadPadded {
        ferrata: w.inst.program,
        instance_id: w.inst.id,
        slot,
        padding: QUERY.to_vec(),
    })
    .unwrap();
    let run = w
        .chain
        .prove_and_settle_private(
            &caller,
            Actor::new(caller.id(), consumer_account()),
            call,
            vec![header],
            &catalog(),
            100_000_000,
            time,
        )
        .expect("settles");
    assert!(run.charge.revert.is_none());
    let published = run.tx.message.execution.boundary.iter().any(|step| match step {
        BoundaryStep::PrivateToPublic(d) => occurs(&d.envelope.message, QUERY),
        _ => false,
    });
    assert!(published, "the query crosses into public execution in the clear");
    // Calling publishes the query; nothing else of the consumer's may occur.
    check_and_dump(
        "stage3-call",
        "Stage 3: the same consumer sends its query to the program (ReadPadded)",
        &run,
        &caller,
        &["the query"],
    );
}
