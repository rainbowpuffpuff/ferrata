//! RFP-021 soft requirement 1, "Recorded-statement account": a consumption path that evaluates a predicate and
//! records the verified statement to a public account, sharing one verification core with the direct path.
//!
//! `ferrata_statements` (guests/statements) records what the attestation program's own `Attest` replies, once, at
//! `statement(key)`; `ferrata_statements_reader` (guests/statements-reader) reads an entry and uses one statement of
//! it once per trust configuration. On the Stage 4 chain (mainnet block 26,384,851): the holdings consumer's
//! question (WETH's balance of Uniswap v2's USDC/WETH pair is at least one WETH) asked directly, then recorded and
//! read by two consumers. Every refusal changes no state. Writes `results/statements.md`.
//!
//! `RISC0_DEV_MODE=1 cargo test -p harness --features statements --locked --test statements -- --nocapture`
#![cfg(feature = "statements")]

use std::{borrow::Cow, fs};

use ferrata_lez_holdings_wire::{Gate, HoldingsState};
use ferrata_lez_statements_wire::{
    code as st, statement_actor, statement_key, statement_seed, use_actor, use_config, use_seed, ReaderMessage,
    ReaderState, Recorded, Root, StatementsMessage, StatementsState,
};
use ferrata_lez_wire::{attest::Word, header_actor};
use harness::{
    attest::{storage_statement_id, u256},
    exit_name,
    holdings::Holdings,
    render, render_transitions, results_dir, TxReport,
};
use lee::{program::Program, AccountId, Actor, PublicAccountEvidence, V03State};

/// One WETH, in wei.
const WETH: u128 = 1_000_000_000_000_000_000;

fn recorder() -> AccountId {
    AccountId::from_builtin_program(methods_statements::FERRATA_STATEMENTS_ID)
}

fn reader() -> AccountId {
    AccountId::from_builtin_program(methods_statements::FERRATA_STATEMENTS_READER_ID)
}

/// The holdings chain with the recorder and the reader deployed beside the attestation program.
fn world() -> Holdings {
    let mut h = Holdings::new();
    let state = std::mem::replace(&mut h.s.w.chain.state, V03State::new());
    h.s.w.chain.state = state.with_genesis_programs([
        (
            Program::new_unchecked(
                methods_statements::FERRATA_STATEMENTS_ID,
                Cow::Borrowed(methods_statements::FERRATA_STATEMENTS_ELF),
            ),
            true,
        ),
        (
            Program::new_unchecked(
                methods_statements::FERRATA_STATEMENTS_READER_ID,
                Cow::Borrowed(methods_statements::FERRATA_STATEMENTS_READER_ELF),
            ),
            true,
        ),
    ]);
    h
}

fn plus_one(word: &Word) -> Word {
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

/// `Record` of the gate's question at `header(id, slot)`, sent to `statement(at)` (the question's own key unless a
/// test says otherwise).
fn record(h: &mut Holdings, label: &str, gate: &Gate, slot: u64, at: Option<Root>) -> (TxReport, Actor, Root) {
    let (account_proof, storage_proof) = h.proofs();
    let request = gate.request(&h.holder, account_proof, storage_proof, None);
    let key = statement_key(&gate.ferrata, slot, &request);
    let at = at.unwrap_or(key);
    let actor = statement_actor(recorder(), &at);
    let record = header_actor(gate.ferrata, &gate.instance_id, slot);
    let message = StatementsMessage::Record { ferrata: gate.ferrata, slot, request };
    let evidence = vec![PublicAccountEvidence::Pda { program: recorder(), seed: statement_seed(&at) }];
    let r = h.s.w.chain.submit(label, actor, borsh::to_vec(&message).unwrap(), vec![actor, record], evidence, h.s.time);
    (r, statement_actor(recorder(), &key), key)
}

/// The reader's `Use` of `statement_id` from `statement(key)`, under a trust configuration of the gate's program,
/// `instance_id`, `chain_id` and `tag`.
#[allow(clippy::too_many_arguments)]
fn use_statement(
    h: &mut Holdings,
    label: &str,
    key: Root,
    gate: &Gate,
    instance_id: Root,
    chain_id: u64,
    statement_id: Root,
    tag: Root,
) -> (TxReport, Actor) {
    let config = use_config(&recorder(), &gate.ferrata, &instance_id, chain_id, &tag);
    let actor = use_actor(reader(), &config, &statement_id);
    let entry = statement_actor(recorder(), &key);
    let message = ReaderMessage::Use {
        recorder: recorder(),
        key,
        ferrata: gate.ferrata,
        instance_id,
        chain_id,
        statement_id,
        tag,
    };
    let evidence = vec![
        PublicAccountEvidence::Pda { program: reader(), seed: use_seed(&config, &statement_id) },
        PublicAccountEvidence::Pda { program: recorder(), seed: statement_seed(&key) },
    ];
    let r = h.s.w.chain.submit(label, actor, borsh::to_vec(&message).unwrap(), vec![actor, entry], evidence, h.s.time);
    (r, actor)
}

fn entry(h: &Holdings, actor: Actor) -> Option<StatementsState> {
    let state = h.s.w.chain.actor_state(actor);
    (!state.is_empty()).then(|| borsh::from_slice(&state).expect("a statements state"))
}

fn reader_state(h: &Holdings, actor: Actor) -> Option<ReaderState> {
    let state = h.s.w.chain.actor_state(actor);
    (!state.is_empty()).then(|| borsh::from_slice(&state).expect("a reader state"))
}

fn refused(r: &TxReport, code: u8) {
    assert!(!r.ok(), "{} was applied", r.label);
    assert_eq!(r.exit_code, Some(u32::from(code)), "{}: {:?}", r.label, r.charge.revert);
    println!("refused as expected: {} → exit {code} ({})", r.label, exit_name(u32::from(code)));
}

fn cycles(r: &TxReport) -> u64 {
    r.transitions.iter().map(|t| t.cycles).sum()
}

#[test]
fn a_statement_recorded_once_serves_several_consumers() {
    let mut h = world();
    let slot = h.slot();
    let gate = h.gate(u256(WETH));
    let balance = h.balance();
    let mut reports = Vec::new();

    // The direct path, for reference: the holdings consumer asks the same question through `Attest`.
    let direct = h.check("direct path: ferrata_holdings Check (Attest in the same transaction)", &gate, slot);
    assert!(direct.ok(), "{:?} (exit {:?})", direct.charge.revert, direct.exit_code);
    let Some(HoldingsState::Held(held)) = h.holding(&gate, &h.holder.clone()) else { panic!("no holding") };
    reports.push(direct);

    // Record: the recorder sends the attestation program's own `Attest`; the reply is kept as it came.
    let (r, actor, key) = record(&mut h, "Record the question (Attest through the recorder)", &gate, slot, None);
    assert!(r.ok(), "{:?} (exit {:?})", r.charge.revert, r.exit_code);
    reports.push(r);
    let Some(StatementsState::Recorded(Recorded { ferrata, record: rec, key: k, attestation })) = entry(&h, actor)
    else {
        panic!("nothing recorded")
    };
    assert_eq!((ferrata, k), (gate.ferrata, key));
    assert_eq!(rec, header_actor(gate.ferrata, &gate.instance_id, slot));
    // One verification core: the entry is what the attestation program itself returns for the request, and its
    // storage statement has the identifier the direct path's consumer keyed its holding on.
    let (account_proof, storage_proof) = h.proofs();
    let expected = h.s.expected(slot, &gate.request(&h.holder, account_proof, storage_proof, None)).expect("attested");
    assert_eq!(attestation, expected, "the recorded reply equals the attestation program's own");
    let storage_id = storage_statement_id(gate.chain_id, &gate.token, &gate.balance_key(&h.holder), &balance);
    assert_eq!(attestation.statements[1].id, storage_id);
    assert_eq!(held.statement_id, storage_id, "the same identifier on both paths");
    println!(
        "recorded at {:?}: {} statements, storage statement 0x{}",
        actor.account_id,
        attestation.statements.len(),
        hex::encode(storage_id)
    );

    // Two consumers (two trust configurations' namespaces) use the statement; neither verifies a proof.
    let (r, first) = use_statement(
        &mut h,
        "consumer A: Use (Read of the entry)",
        key,
        &gate,
        gate.instance_id,
        gate.chain_id,
        storage_id,
        [0xa1; 32],
    );
    assert!(r.ok(), "{:?} (exit {:?})", r.charge.revert, r.exit_code);
    let used_a = r.clone();
    reports.push(r);
    let (r, second) = use_statement(
        &mut h,
        "consumer B: Use (Read of the entry)",
        key,
        &gate,
        gate.instance_id,
        gate.chain_id,
        storage_id,
        [0xb2; 32],
    );
    assert!(r.ok(), "{:?} (exit {:?})", r.charge.revert, r.exit_code);
    reports.push(r);
    for actor in [first, second] {
        let Some(ReaderState::Used(used)) = reader_state(&h, actor) else { panic!("not used") };
        assert_eq!(used.statement_id, storage_id);
        assert_eq!(used.block_number, attestation.block_number);
        assert_eq!(used.entry, statement_actor(recorder(), &key));
    }

    // The nullifier on the recorded path: consumer A cannot use the statement twice.
    let (r, _) = use_statement(
        &mut h,
        "consumer A again (same configuration)",
        key,
        &gate,
        gate.instance_id,
        gate.chain_id,
        storage_id,
        [0xa1; 32],
    );
    refused(&r, st::ALREADY_USED);
    reports.push(r);
    // Once: a second `Record` of the same question is refused and the entry stays as it was.
    let before = entry(&h, actor);
    let (r, _, _) = record(&mut h, "Record the same question again", &gate, slot, None);
    refused(&r, st::ALREADY_RECORDED);
    assert_eq!(entry(&h, actor), before);
    reports.push(r);

    // The cost: a use reads the entry; the record verified the proofs once.
    let record_cycles = cycles(&reports[1]);
    let use_cycles = cycles(&used_a);
    let direct_cycles = cycles(&reports[0]);
    println!("cycles: direct {direct_cycles}, record {record_cycles}, one use {use_cycles}");
    assert!(use_cycles * 2 < direct_cycles, "a use costs less than half a direct attestation");

    let table = format!(
        "# ferrata_statements: a recorded statement serves several consumers (dev mode, in-process harness)\n\n\
         Written by `RISC0_DEV_MODE=1 cargo test -p harness --features statements --locked --test statements -- \
         --nocapture`. RFP-021 soft requirement 1. Holder 0x{} (Uniswap v2 USDC/WETH), WETH 0x{}, record \
         `header(id, {slot})`, balance 0x{} wei, question: balance ≥ 1 WETH.\n\n\
         Summed guest cycles of each transaction (every program it ran): direct path (`ferrata_holdings` with \
         `Attest`) {direct_cycles}; `Record` {record_cycles}; one consumer's `Use` (reads the entry, verifies no \
         proof) {use_cycles}. With `k` consumers of one question the recorded path costs `Record` + k × `Use`, \
         the direct path k × direct.\n\n## Transactions\n\n{}\n## Handlers\n\n{}",
        hex::encode(h.holder),
        hex::encode(h.s.fx.weth),
        hex::encode(balance),
        render(&reports),
        render_transitions(&reports),
    );
    fs::write(results_dir().join("statements.md"), table).unwrap();
}

#[test]
fn refusals_record_nothing() {
    let mut h = world();
    let slot = h.slot();
    let balance = h.balance();
    let gate = h.gate(u256(WETH));

    // A predicate that does not hold: the attestation program refuses, the whole transaction ends with its code and
    // the recorder keeps nothing.
    let above = h.gate(plus_one(&balance));
    let (account_proof, storage_proof) = h.proofs();
    let native =
        h.s.expected(slot, &above.request(&h.holder, account_proof, storage_proof, None)).expect_err("refused");
    let (r, actor, _) = record(&mut h, "Record of a false predicate (minimum = balance + 1 wei)", &above, slot, None);
    refused(&r, native);
    assert!(entry(&h, actor).is_none());

    // At an actor that is not the question's.
    let other = statement_actor(recorder(), &[7; 32]);
    let (r, _, _) = record(&mut h, "Record at another statement's actor", &gate, slot, Some([7; 32]));
    refused(&r, st::WRONG_RECEIVER);
    assert!(entry(&h, other).is_none());

    // A use of a question never recorded.
    let (account_proof, storage_proof) = h.proofs();
    let key = statement_key(&gate.ferrata, slot, &gate.request(&h.holder, account_proof, storage_proof, None));
    let storage_id = storage_statement_id(gate.chain_id, &gate.token, &gate.balance_key(&h.holder), &balance);
    let (r, used) = use_statement(
        &mut h,
        "Use before any Record",
        key,
        &gate,
        gate.instance_id,
        gate.chain_id,
        storage_id,
        [1; 32],
    );
    refused(&r, st::NOT_RECORDED);
    assert!(reader_state(&h, used).is_none());

    // Recorded; then a reader that trusts another instance or chain, or wants a statement the entry does not hold.
    let (r, _, _) = record(&mut h, "Record", &gate, slot, None);
    assert!(r.ok(), "{:?} (exit {:?})", r.charge.revert, r.exit_code);
    for (label, instance_id, chain_id, statement_id, code) in [
        ("Use trusting another instance", [9; 32], gate.chain_id, storage_id, st::WRONG_SOURCE),
        ("Use trusting another chain (Sepolia)", gate.instance_id, 11_155_111, storage_id, st::WRONG_SOURCE),
        ("Use of a statement the entry does not hold", gate.instance_id, gate.chain_id, [5; 32], st::NO_SUCH_STATEMENT),
    ] {
        let (r, used) = use_statement(&mut h, label, key, &gate, instance_id, chain_id, statement_id, [2; 32]);
        refused(&r, code);
        assert!(reader_state(&h, used).is_none());
    }
}
