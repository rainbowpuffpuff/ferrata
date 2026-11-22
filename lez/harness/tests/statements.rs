//! RFP-021 soft requirement 1, "Recorded-statement account": a consumption path that evaluates a predicate and
//! records the verified statement to a public account, sharing one verification core with the direct path.
//!
//! `ferrata_statements` (guests/statements) records what the attestation program's own `Attest` replies, once, at
//! `statement(key)`; `ferrata_statements_reader` (guests/statements-reader) is a reference consumer: under a trust
//! configuration that fixes the recorder, the attestation program, the instance, the chain, the question and the
//! oldest block it accepts, it derives the entry's key itself and uses one statement of the entry once. On the Stage 4
//! chain (mainnet block 26,384,851): the holdings consumer's question (WETH's balance of Uniswap v2's USDC/WETH pair is
//! at least one WETH) asked directly, then recorded and read by two consumers; then each of the four checks the guide
//! asks of a consumer (`docs/guides/recorded-statements.md`), refusing what it must. Every refusal changes no state.
//! Writes `results/statements.md` and `results/statements-checks.md`.
//!
//! `RISC0_DEV_MODE=1 cargo test -p harness --features statements --locked --test statements -- --nocapture`
#![cfg(feature = "statements")]

use std::{borrow::Cow, fs};

use ferrata_lez_holdings_wire::{Gate, HoldingsState};
use ferrata_lez_statements_wire::{
    code as st, query, question_id, statement_actor, statement_ids, statement_key, statement_seed, use_actor,
    use_config, use_seed, ReaderMessage, ReaderState, Recorded, Root, StatementsMessage, StatementsState, Waiting,
};
use ferrata_lez_wire::{
    attest::{Fact, Request, Word},
    header_actor,
};
use harness::{
    attest::{storage_statement_id, u256},
    exit_name,
    holdings::Holdings,
    render, render_transitions, results_dir, TxReport,
};
use lee::{program::Program, Account, AccountId, Actor, PublicAccountEvidence, V03State};
use lee_core::account::ActorState;

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

/// The gate's request at the current proofs.
fn request(h: &Holdings, gate: &Gate) -> Request {
    let (account_proof, storage_proof) = h.proofs();
    gate.request(&h.holder, account_proof, storage_proof, None)
}

/// `Record` of the gate's question at `header(id, slot)`, sent to `statement(at)` (the question's own key unless a
/// test says otherwise).
fn record(h: &mut Holdings, label: &str, gate: &Gate, slot: u64, at: Option<Root>) -> (TxReport, Actor, Root) {
    let request = request(h, gate);
    let key = statement_key(&gate.ferrata, slot, &request);
    let at = at.unwrap_or(key);
    let actor = statement_actor(recorder(), &at);
    let record = header_actor(gate.ferrata, &gate.instance_id, slot);
    let message = StatementsMessage::Record { ferrata: gate.ferrata, slot, request };
    let evidence = vec![PublicAccountEvidence::Pda { program: recorder(), seed: statement_seed(&at) }];
    let r = h.s.w.chain.submit(label, actor, borsh::to_vec(&message).unwrap(), vec![actor, record], evidence, h.s.time);
    (r, statement_actor(recorder(), &key), key)
}

/// What a consumer of the reader fixes: its trust configuration (everything but the record's slot).
#[derive(Clone)]
struct Trust {
    ferrata: AccountId,
    instance_id: Root,
    chain_id: u64,
    question: Request,
    min_block: u64,
    tag: Root,
}

impl Trust {
    /// The gate's own question, under the genuine recorder and the gate's attestation program, instance and chain.
    fn of(h: &Holdings, gate: &Gate, min_block: u64, tag: Root) -> Self {
        let question = query(&request(h, gate));
        Trust {
            ferrata: gate.ferrata,
            instance_id: gate.instance_id,
            chain_id: gate.chain_id,
            question,
            min_block,
            tag,
        }
    }

    fn config(&self) -> Root {
        let question = question_id(&self.question);
        use_config(&recorder(), &self.ferrata, &self.instance_id, self.chain_id, &question, self.min_block, &self.tag)
    }

    /// The entry this configuration reads at `slot`: derived from the question, as the reader derives it.
    fn key(&self, slot: u64) -> Root {
        statement_key(&self.ferrata, slot, &self.question)
    }

    fn use_actor(&self, statement_id: &Root) -> Actor {
        use_actor(reader(), &self.config(), statement_id)
    }
}

/// The reader's `Use` of `statement_id` from the entry of the configuration's question at `header(id, slot)`.
fn use_statement(h: &mut Holdings, label: &str, t: &Trust, slot: u64, statement_id: Root) -> (TxReport, Actor) {
    let actor = t.use_actor(&statement_id);
    let key = t.key(slot);
    let entry = statement_actor(recorder(), &key);
    let message = ReaderMessage::Use {
        recorder: recorder(),
        ferrata: t.ferrata,
        instance_id: t.instance_id,
        chain_id: t.chain_id,
        question: t.question.clone(),
        min_block: t.min_block,
        slot,
        statement_id,
        tag: t.tag,
    };
    let evidence = vec![
        PublicAccountEvidence::Pda { program: reader(), seed: use_seed(&t.config(), &statement_id) },
        PublicAccountEvidence::Pda { program: recorder(), seed: statement_seed(&key) },
    ];
    let r = h.s.w.chain.submit(label, actor, borsh::to_vec(&message).unwrap(), vec![actor, entry], evidence, h.s.time);
    (r, actor)
}

/// Writes `state` at `actor` as its program writes it: for states no honest transaction leaves behind (an entry the
/// recorder never writes there, a reader's wait that never outlives its transaction).
fn place(h: &mut Holdings, actor: Actor, state: Vec<u8>) {
    let account = Account::default().with_actor_state(actor.program_account_id, ActorState::from(state));
    h.s.w.chain.state.force_insert_account(actor.account_id, account);
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

    // Two consumers (two trust configurations' namespaces) use the statement; neither verifies a proof. Each fixes
    // the question and accepts blocks from the entry's on.
    let consumer_a = Trust::of(&h, &gate, attestation.block_number, [0xa1; 32]);
    let consumer_b = Trust::of(&h, &gate, attestation.block_number, [0xb2; 32]);
    assert_eq!(consumer_a.key(slot), key, "the reader derives the key the recorder wrote");
    let (r, first) = use_statement(&mut h, "consumer A: Use (Read of the entry)", &consumer_a, slot, storage_id);
    assert!(r.ok(), "{:?} (exit {:?})", r.charge.revert, r.exit_code);
    let used_a = r.clone();
    reports.push(r);
    let (r, second) = use_statement(&mut h, "consumer B: Use (Read of the entry)", &consumer_b, slot, storage_id);
    assert!(r.ok(), "{:?} (exit {:?})", r.charge.revert, r.exit_code);
    reports.push(r);
    for actor in [first, second] {
        let Some(ReaderState::Used(used)) = reader_state(&h, actor) else { panic!("not used") };
        assert_eq!(used.statement_id, storage_id);
        assert_eq!(used.block_number, attestation.block_number);
        assert_eq!(used.entry, statement_actor(recorder(), &key));
    }

    // The nullifier on the recorded path: consumer A cannot use the statement twice.
    let (r, _) = use_statement(&mut h, "consumer A again (same configuration)", &consumer_a, slot, storage_id);
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
    let storage_id = storage_statement_id(gate.chain_id, &gate.token, &gate.balance_key(&h.holder), &balance);
    let consumer = Trust::of(&h, &gate, 0, [1; 32]);
    let (r, used) = use_statement(&mut h, "Use before any Record", &consumer, slot, storage_id);
    refused(&r, st::NOT_RECORDED);
    assert!(reader_state(&h, used).is_none());

    // Recorded; then a `Read` that no program sent (a transaction's root): there is no one to reply to.
    let (r, actor, key) = record(&mut h, "Record", &gate, slot, None);
    assert!(r.ok(), "{:?} (exit {:?})", r.charge.revert, r.exit_code);
    let before = entry(&h, actor);
    let message = borsh::to_vec(&StatementsMessage::Read).unwrap();
    let evidence = vec![PublicAccountEvidence::Pda { program: recorder(), seed: statement_seed(&key) }];
    let r =
        h.s.w.chain.submit("Read as a transaction's root (no sender)", actor, message, vec![actor], evidence, h.s.time);
    refused(&r, st::WRONG_SENDER);
    assert_eq!(entry(&h, actor), before);
}

/// The four checks `docs/guides/recorded-statements.md` asks of a consumer, each made by the reference reader and each
/// refusing what it must. Writes `results/statements-checks.md`.
#[test]
fn the_reader_makes_the_four_consumer_checks() {
    let mut h = world();
    let slot = h.slot();
    let balance = h.balance();
    let strong = h.gate(u256(WETH));
    let weak = h.gate(u256(0));
    let storage_id = storage_statement_id(strong.chain_id, &strong.token, &strong.balance_key(&h.holder), &balance);
    let mut reports = Vec::new();

    // Check 2, the question: only the weaker question (balance ≥ 0) is recorded. Its entry holds the very storage
    // statement the stronger question (≥ 1 WETH) is about, under the same identifier; a consumer of the stronger
    // question derives another key and never reads it.
    let (r, weak_actor, weak_key) = record(&mut h, "Record the weaker question (balance ≥ 0)", &weak, slot, None);
    assert!(r.ok(), "{:?} (exit {:?})", r.charge.revert, r.exit_code);
    reports.push(r);
    let Some(StatementsState::Recorded(weak_entry)) = entry(&h, weak_actor) else { panic!("nothing recorded") };
    let block = weak_entry.attestation.block_number;
    assert!(statement_ids(&weak_entry).contains(&storage_id));
    let strong_consumer = Trust::of(&h, &strong, block, [0xc1; 32]);
    assert_ne!(strong_consumer.key(slot), weak_key);
    let (r, used) = use_statement(
        &mut h,
        "check 2: Use under the stronger question while only the weaker one is recorded",
        &strong_consumer,
        slot,
        storage_id,
    );
    refused(&r, st::NOT_RECORDED);
    assert!(reader_state(&h, used).is_none());
    reports.push(r);
    // The weaker question's own consumer is served, in its own configuration's actors only.
    let weak_consumer = Trust::of(&h, &weak, block, [0xc1; 32]);
    let (r, used_weak) =
        use_statement(&mut h, "Use under the weaker question itself", &weak_consumer, slot, storage_id);
    assert!(r.ok(), "{:?} (exit {:?})", r.charge.revert, r.exit_code);
    reports.push(r);
    assert_ne!(used_weak, strong_consumer.use_actor(&storage_id));
    assert!(
        reader_state(&h, strong_consumer.use_actor(&storage_id)).is_none(),
        "the stronger configuration is untouched"
    );
    // Once the stronger question itself is recorded, its consumer is served. The two entries carry the same
    // statement identifiers: the identifier does not encode the predicate, the key does.
    let (r, strong_actor, _) = record(&mut h, "Record the stronger question (balance ≥ 1 WETH)", &strong, slot, None);
    assert!(r.ok(), "{:?} (exit {:?})", r.charge.revert, r.exit_code);
    reports.push(r);
    let Some(StatementsState::Recorded(strong_entry)) = entry(&h, strong_actor) else { panic!("nothing recorded") };
    assert_eq!(statement_ids(&strong_entry), statement_ids(&weak_entry), "the same identifiers under both predicates");

    // Check 4, the block: an entry older than the configuration accepts is refused; the exact bound is accepted.
    let mut later = strong_consumer.clone();
    later.min_block = block + 1;
    let (r, used) =
        use_statement(&mut h, "check 4: Use requiring a block newer than the entry's", &later, slot, storage_id);
    refused(&r, st::STALE);
    assert!(reader_state(&h, used).is_none());
    reports.push(r);
    let (r, used) = use_statement(
        &mut h,
        "Use under the stronger question, oldest block = the entry's",
        &strong_consumer,
        slot,
        storage_id,
    );
    assert!(r.ok(), "{:?} (exit {:?})", r.charge.revert, r.exit_code);
    let Some(ReaderState::Used(u)) = reader_state(&h, used) else { panic!("not used") };
    assert_eq!((u.statement_id, u.block_number, u.entry), (storage_id, block, strong_actor));
    reports.push(r);

    // Check 3, the instance, the chain and the statement.
    for (label, instance_id, chain_id, statement_id, code) in [
        ("check 3: Use trusting another instance", [9; 32], strong.chain_id, storage_id, st::WRONG_SOURCE),
        ("check 3: Use trusting another chain (Sepolia)", strong.instance_id, 11_155_111, storage_id, st::WRONG_SOURCE),
        (
            "check 3: Use of a statement the entry does not hold",
            strong.instance_id,
            strong.chain_id,
            [5; 32],
            st::NO_SUCH_STATEMENT,
        ),
    ] {
        let t = Trust { instance_id, chain_id, tag: [0xc3; 32], ..strong_consumer.clone() };
        let (r, used) = use_statement(&mut h, label, &t, slot, statement_id);
        refused(&r, code);
        assert!(reader_state(&h, used).is_none());
        reports.push(r);
    }

    // Check 1, the programs. An attestation program of someone's own choosing can answer `Record` with anything:
    // the recorder then keeps that answer at a key holding that program's id. Here such an entry is placed as the
    // recorder would write it (no such program is deployed), claiming a balance of 2^256 - 1 for the same statement
    // identifier, for a question nobody else recorded (minimum 2 WETH).
    let mut h2 = world();
    let other_question = h2.gate(u256(2 * WETH));
    let rogue = AccountId::new([0xfa; 32]);
    let mut forged = h2.s.expected(slot, &request(&h2, &other_question)).expect("the genuine answer");
    if let Fact::Storage { value, .. } = &mut forged.statements[1].fact {
        *value = [0xff; 32];
    }
    let genuine = Trust::of(&h2, &other_question, block, [0xc4; 32]);
    let fooled = Trust { ferrata: rogue, ..genuine.clone() };
    let rogue_key = fooled.key(slot);
    let rogue_entry = Recorded {
        ferrata: rogue,
        record: header_actor(rogue, &genuine.instance_id, slot),
        key: rogue_key,
        attestation: forged.clone(),
    };
    place(
        &mut h2,
        statement_actor(recorder(), &rogue_key),
        borsh::to_vec(&StatementsState::Recorded(rogue_entry)).unwrap(),
    );
    let (r, used) = use_statement(
        &mut h2,
        "check 1: Use fixing the genuine program, beside another program's entry",
        &genuine,
        slot,
        storage_id,
    );
    refused(&r, st::NOT_RECORDED);
    assert!(reader_state(&h2, used).is_none());
    reports.push(r);
    // A configuration that names the other program reads that program's entry: what it accepts is what it fixed.
    let (r, used) = use_statement(
        &mut h2,
        "Use under a configuration that names the other program (it trusts that program)",
        &fooled,
        slot,
        storage_id,
    );
    assert!(r.ok(), "{:?} (exit {:?})", r.charge.revert, r.exit_code);
    assert!(matches!(reader_state(&h2, used), Some(ReaderState::Used(_))));
    reports.push(r);
    // At the actor the genuine configuration derives, an entry naming another program, or another key, is refused
    // (placed: the recorder never writes either there).
    let genuine_key = genuine.key(slot);
    let at = statement_actor(recorder(), &genuine_key);
    for (label, entry) in [
        (
            "check 1: an entry at the derived actor naming another attestation program (placed)",
            Recorded {
                ferrata: rogue,
                record: header_actor(rogue, &genuine.instance_id, slot),
                key: genuine_key,
                attestation: forged.clone(),
            },
        ),
        (
            "check 1: an entry at the derived actor carrying another key (placed)",
            Recorded {
                ferrata: genuine.ferrata,
                record: header_actor(genuine.ferrata, &genuine.instance_id, slot),
                key: rogue_key,
                attestation: forged.clone(),
            },
        ),
    ] {
        place(&mut h2, at, borsh::to_vec(&StatementsState::Recorded(entry)).unwrap());
        let (r, used) = use_statement(&mut h2, label, &Trust { tag: [0xc5; 32], ..genuine.clone() }, slot, storage_id);
        refused(&r, st::WRONG_SOURCE);
        assert!(reader_state(&h2, used).is_none());
        reports.push(r);
    }

    // Check 2, the reply's sender: a reader waiting for its entry accepts a reply from that entry only. A wait never
    // outlives its transaction, so it is placed here; a forged entry sent to it by anyone else is refused.
    let waiting_config = Trust { tag: [0xc6; 32], ..genuine.clone() };
    let waiting_at = waiting_config.use_actor(&storage_id);
    let waiting = Waiting {
        entry: at,
        key: genuine_key,
        ferrata: genuine.ferrata,
        instance_id: genuine.instance_id,
        chain_id: genuine.chain_id,
        statement_id: storage_id,
        min_block: block,
    };
    place(&mut h2, waiting_at, borsh::to_vec(&ReaderState::Waiting(waiting)).unwrap());
    let reply = Recorded {
        ferrata: genuine.ferrata,
        record: header_actor(genuine.ferrata, &genuine.instance_id, slot),
        key: genuine_key,
        attestation: forged,
    };
    let evidence =
        vec![PublicAccountEvidence::Pda { program: reader(), seed: use_seed(&waiting_config.config(), &storage_id) }];
    let r = h2.s.w.chain.submit(
        "check 2: a forged entry sent to a waiting reader with no sender, as a transaction's root (the wait placed)",
        waiting_at,
        borsh::to_vec(&reply).unwrap(),
        vec![waiting_at],
        evidence,
        h2.s.time,
    );
    refused(&r, st::WRONG_SENDER);
    assert!(matches!(reader_state(&h2, waiting_at), Some(ReaderState::Waiting(_))));
    reports.push(r);

    let table = format!(
        "# ferrata_statements_reader: the four consumer checks (dev mode, in-process harness)\n\n\
         Written by `RISC0_DEV_MODE=1 cargo test -p harness --features statements --locked --test statements -- \
         --nocapture` (`the_reader_makes_the_four_consumer_checks`). RFP-021 soft requirement 1; the checks of \
         `docs/guides/recorded-statements.md`. Holder 0x{} (Uniswap v2 USDC/WETH), record `header(id, {slot})`, \
         execution block {block}. Check 1: the recorder's and the attestation program's ids fixed in the \
         configuration. Check 2: the key derived from the configuration's question (an entry for a weaker predicate, \
         with the same statement identifiers, is never read), and a reply accepted only from that entry. Check 3: \
         the entry's instance, chain and statement. Check 4: the entry's block not older than the configuration \
         accepts. Rows marked placed write a state no honest transaction leaves behind, to reach the check.\n\n\
         ## Transactions\n\n{}",
        hex::encode(h.holder),
        render(&reports),
    );
    fs::write(results_dir().join("statements-checks.md"), table).unwrap();
}
