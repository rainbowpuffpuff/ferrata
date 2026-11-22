//! RFP-021's verification-security requirements through the attestation program on LEZ v0.4's
//! state machine (the in-process harness; run with `RISC0_DEV_MODE=1`): every forgery is sent as
//! the transactions a submitter or a consumer would send, settled by LEZ's own functions, and
//! judged by the exit code the program returns. Writes `results/lc-security/*.md`.
//!
//! - VS.1: a fabricated chain (a self-consistent header and inclusion proof over state that was
//!   never finalised) against the canonical mainnet instance, and the same chain accepted by an
//!   instance a forger initialised himself, to show it is self-consistent.
//! - VS.2 and F.2: the six listed forgeries, and every way of offering a header that is not
//!   finalised.
//! - U.5: each listed failure mode with the code it returns and the IDL's message for it.
//! - F.4: the six comparisons against a constant and between two facts, from the consumer.
//! - R.3: records read and attested after committee rotations (synthetic, mainnet, Sepolia).
//!
//! Where a forgery needs a committee whose keys no real chain gives out, the chain is synthetic
//! (`security_common::Synth`: a mainnet-preset instance whose committees the test holds). Every
//! update sent there with `World::apply` also runs through Ferrata's policy store (`FinalizedStore`,
//! the native store whose rules the program runs), which must give the same verdict. The rest are
//! sent as the program's own transactions only (`run_job`, `verify_all`), with no native verdict
//! beside them: the refusals the native store does not model (an adoption with a replaced key, 45;
//! the weak-subjectivity cases, 53 and LEZ's validity window; the update ten years on) and the
//! `VerifyAll` forms. The specification's store (`LightClientStore`) refuses less than the policy
//! store: it is run on the two cases where they part (an update without finality, 341 of 512
//! signers), and refuses neither but finalizes neither (`spec_store_verdict`).

mod security_common;

use ferrata_core::{
    fork::LcFork,
    policy::{Adoption, FinalizedStore},
    steps,
    store::LightClientStore,
    types::{Execution, LightClientUpdate},
    witness::{uncompressed_keys, CompactBootstrap, CompactUpdate, HeaderRecord},
    Root,
};
use ferrata_lez_wire::{
    attest::{evaluate, AccountQuery, Address, Cmp, Condition, Field, LogQuery, Operand, Relation, Request, Word},
    consumer::{balance_actor, claim_actor, claim_seed, nullifier, ConsumerMessage, ConsumerState},
    header_actor, header_seed, instance_actor, instance_seed, job_actor, job_id, successor_id, Message,
    ATTESTED_SELECTOR,
};
use ferrata_spectests::{
    execkit::relink,
    testkit::{fabricated_bootstrap, fake_committee, update_over},
};
use harness::{
    attest::{account_query, consumer_state, Stage4},
    consumer_account, ferrata_account, mainnet_config, plan_job,
    scenarios::{World, BLS},
    sepolia::{self, SepoliaFacts, SEPOLIA_CHAIN_ID},
    vault::{attest_root, vault_file},
    Instance, TxReport, EXPECTED_FINALIZED,
};
use lee::{Actor, PublicAccountEvidence};
use lee_core::program::PdaSeed;
use security_common::*;

const YEAR_MS: u64 = 365 * 24 * 3_600_000;

fn require_dev_mode() {
    assert_eq!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1"), "run these tests with RISC0_DEV_MODE=1");
}

/// The attestation event's data in a report, if the program emitted one.
fn attested(report: &TxReport) -> Option<Vec<u8>> {
    report.events.iter().find(|e| e.event.selector == ATTESTED_SELECTOR).map(|e| e.event.data.clone())
}

fn row(case: &str, expected: &str, verdict: Verdict, note: &str) -> Row {
    Row { case: case.into(), expected: expected.into(), verdict, note: note.into() }
}

/// A row whose verdict must be `code` (or applied, for `None`).
fn check(rows: &mut Vec<Row>, case: &str, expected: Option<u32>, verdict: Verdict, note: &str) {
    assert_eq!(verdict.code(), expected, "{case}: {}", verdict.show());
    if expected.is_none() {
        assert_eq!(verdict, Verdict::Applied, "{case}");
    }
    let shown = expected.map_or("applied".to_string(), named);
    rows.push(row(case, &shown, verdict, note));
}

// ---------------------------------------------------------------------------------------------
// Jobs sent as a submitter sends them
// ---------------------------------------------------------------------------------------------

/// `Open`, `Miller`, the adoption parts (with `keys` in place of the update's next committee if
/// given) and `Finish` of `update` against `w`'s instance, at `time`; stops at the first refusal.
/// Unlike `World::apply` it does not ask the native store first, so a job the native store would
/// accept can be sent with a forged adoption.
fn run_job(
    w: &mut World,
    update: &LightClientUpdate,
    keys: Option<Vec<[u8; 96]>>,
    time: u64,
    label: &str,
) -> Vec<TxReport> {
    let inst = w.inst.clone();
    let spec = inst.config.spec();
    let layout = LcFork::Electra;
    let stored = inst.store(&w.chain);
    let store = steps::store_for_update(&stored, layout, update.attested_header.beacon.slot, &spec)
        .unwrap_or_else(|_| stored.clone());
    let compact = CompactUpdate::of(update);
    let wit = Instance::signer_witness(w.native.core(), update, &w.spec);
    let jid = job_id(&inst.id, &store.digest(), &compact.digest(layout));
    let job = job_actor(inst.program, &inst.id, &jid);
    let (planned, _) = plan_job(&store, &compact, &wit, update, &inst.config);
    let mut reports = Vec::new();
    let open = Message::Open { update: compact.encode(layout), signers: wit.encode() };
    reports.push(w.chain.submit(
        &format!("{label}: Open"),
        inst.actor(),
        borsh::to_vec(&open).unwrap(),
        vec![inst.actor(), job],
        vec![],
        time,
    ));
    if !reports.last().unwrap().ok() {
        return reports;
    }
    reports.push(w.chain.submit(
        &format!("{label}: Miller"),
        job,
        borsh::to_vec(&Message::Miller).unwrap(),
        vec![job],
        vec![],
        time,
    ));
    if !reports.last().unwrap().ok() {
        return reports;
    }
    if planned.as_ref().is_some_and(|j| j.adoption.is_some()) {
        let all = keys.unwrap_or_else(|| uncompressed_keys(&update.next_sync_committee).unwrap());
        let parts = inst.config.adoption_parts as usize;
        let chunk = all.len() / parts;
        for p in 0..parts {
            let adopt = Message::Adopt { part: p as u8, keys: all[p * chunk..(p + 1) * chunk].to_vec() };
            reports.push(w.chain.submit(
                &format!("{label}: Adopt {p}"),
                job,
                borsh::to_vec(&adopt).unwrap(),
                vec![job],
                vec![],
                time,
            ));
            if !reports.last().unwrap().ok() {
                return reports;
            }
        }
    }
    let header = header_actor(inst.program, &inst.id, update.finalized_header.beacon.slot);
    reports.push(w.chain.submit(
        &format!("{label}: Finish"),
        job,
        borsh::to_vec(&Message::Finish).unwrap(),
        vec![job, inst.actor(), header],
        vec![],
        time,
    ));
    reports
}

/// `VerifyAll` of `update` against `w`'s instance in one transaction, at `time`.
fn verify_all(w: &mut World, update: &LightClientUpdate, time: u64, label: &str) -> TxReport {
    let message = Message::VerifyAll {
        config: w.inst.config.clone(),
        checkpoint: w.inst.checkpoint,
        store: w.inst.store(&w.chain).encode(),
        update: CompactUpdate::of(update).encode(LcFork::Electra),
        signers: Instance::signer_witness(w.native.core(), update, &w.spec).encode(),
        adoption: vec![],
    };
    let verifier = Actor::new(w.chain.payer, ferrata_account());
    let header = header_actor(w.inst.program, &w.inst.id, update.finalized_header.beacon.slot);
    w.chain.submit(
        label,
        verifier,
        borsh::to_vec(&message).unwrap(),
        vec![verifier, w.inst.actor(), header],
        vec![],
        time,
    )
}

/// `Attest` at `header(id, slot)`, an actor that holds no record, admitted by its PDA evidence so
/// that the program decides (without the evidence LEZ refuses the transaction before the program
/// runs, with no code: "Program violated execution rules").
fn attest_no_record(
    chain: &mut harness::Chain,
    label: &str,
    program: lee::AccountId,
    instance_id: &Root,
    slot: u64,
    request: &Request,
    time: u64,
) -> TxReport {
    let actor = header_actor(program, instance_id, slot);
    let message = borsh::to_vec(&Message::Attest { request: request.clone() }).unwrap();
    let evidence = vec![PublicAccountEvidence::Pda { program, seed: header_seed(instance_id, slot) }];
    chain.submit(label, actor, message, vec![actor], evidence, time)
}

/// Run `update` through `w` with `World::apply` (the program and Ferrata's native policy store,
/// `FinalizedStore`, must give the same verdict).
fn apply(w: &mut World, update: &LightClientUpdate, label: &str) -> Verdict {
    Verdict::of(&w.apply(update, label).reports)
}

/// The specification's store (`LightClientStore`, initialised from `w`'s checkpoint and
/// bootstrap: `w` as it is just after `Init`) on `update`, at the slot `World::apply` uses: its
/// verdict, and whether it finalized the update's header. Ferrata's policy store refuses more
/// than this store does (it takes finality updates only, signed by at least two thirds).
fn spec_store_verdict(w: &World, update: &LightClientUpdate) -> (Result<(), ferrata_core::Error>, bool) {
    assert_eq!(w.runs.len(), 1, "the specification's store is compared with an instance just after Init");
    let mut store = LightClientStore::initialize(&w.inst.checkpoint, &w.bootstrap, w.inst.fork(), &w.spec)
        .expect("the specification's store initialises from the checkpoint");
    let before = store.core.finalized_header.clone();
    let verdict = store.process_update(update, update.signature_slot + 1, &w.ctx());
    (verdict, store.core.finalized_header != before)
}

// ---------------------------------------------------------------------------------------------
// The synthetic chain and the cases on it
// ---------------------------------------------------------------------------------------------

/// The synthetic mainnet-preset chain of period 1899 (the red team's B-LC-SYN): `Init` from a
/// checkpoint at `slot0` whose committee the test holds, `periods` committees.
fn synth(seed: u8, periods: usize) -> Synth {
    let slot0 = EXPECTED_FINALIZED[0].0 + 64;
    Synth::new(mainnet_config(2), seed, slot0, filler_header(slot0), periods)
}

/// The slot of `s`'s checkpoint.
fn slot0(s: &Synth) -> u64 {
    s.w.bootstrap.header.beacon.slot
}

/// A valid finality update in the checkpoint's period: finalized at slot0 + 64, attested at
/// slot0 + 96, signed at slot0 + 97 by committee 0's first `signers` members; `next` hands off.
fn in_period(s: &Synth, signers: usize, next: Option<usize>) -> LightClientUpdate {
    let s0 = slot0(s);
    s.update(0, Some(filler_header(s0 + 64)), filler_header(s0 + 96), next, s0 + 97, signers)
}

/// A valid finality update in period `p0 + k`, signed by committee `signer`.
fn in_next_period(s: &Synth, k: u64, signer: usize, next: Option<usize>) -> LightClientUpdate {
    let start = s.start(k);
    s.update(signer, Some(filler_header(start + 64)), filler_header(start + 96), next, start + 97, 512)
}

fn payload_mut(u: &mut LightClientUpdate) -> &mut ferrata_core::types::ExecutionPayloadHeader {
    match &mut u.finalized_header.execution {
        Execution::Payload { header, .. } => header,
        other => panic!("not a payload header: {other:?}"),
    }
}

/// The light-client cases, each on a fresh copy of the synthetic chain. Returns rows keyed by a
/// short name, so that the tests below can pick theirs.
fn light_client_cases() -> Vec<(&'static str, Row)> {
    let base = synth(0x31, 3);
    let s0 = slot0(&base);
    let mut out = Vec::new();
    let mut put = |key: &'static str, case: &str, expected: Option<u32>, verdict: Verdict, note: &str| {
        let mut rows = Vec::new();
        check(&mut rows, case, expected, verdict, note);
        out.push((key, rows.pop().unwrap()));
    };

    // Committee: one never handed off from the checkpoint.
    {
        let mut w = base.w.clone();
        let foreign = fake_committee(512, 0xee);
        let u = update_over(
            Some(filler_header(s0 + 64)),
            filler_header(s0 + 96),
            None,
            s0 + 97,
            512,
            &foreign,
            &base.w.ctx(),
        );
        put(
            "committee-foreign",
            "an update signed by 512 keys of a committee never handed off from the checkpoint",
            Some(40),
            apply(&mut w, &u, "foreign committee"),
            "every branch verifies; refused on the aggregate signature at `Finish`",
        );
    }
    {
        let mut w = base.w.clone();
        let mut u = in_period(&base, 512, Some(1));
        u.next_sync_committee = base.committees[2].committee.clone();
        put(
            "handoff-branch",
            "a handoff whose next committee is replaced (its branch kept)",
            Some(27),
            apply(&mut w, &u, "handoff: next committee replaced"),
            "the attested state does not prove the committee handed off",
        );
    }
    {
        let mut w = base.w.clone();
        let u = in_period(&base, 512, Some(1));
        let mut keys = uncompressed_keys(&base.committees[1].committee.clone()).unwrap();
        keys[7] = uncompressed_keys(&base.committees[2].committee).unwrap()[7];
        let time = base.time(&u);
        let reports = run_job(&mut w, &u, Some(keys), time, "handoff: one adopted key replaced");
        put(
            "handoff-adoption",
            "a handoff whose adopted keys are not the proven committee's (key 7 replaced)",
            Some(45),
            Verdict::of(&reports),
            "the update verifies; the adoption parts must give exactly the committee the attested state proves",
        );
        assert!(w.inst.record(&w.chain, s0 + 64).is_none(), "a refused job writes no record");
    }
    {
        let mut w = base.w.clone();
        let u = in_next_period(&base, 1, 1, None);
        put(
            "handoff-none",
            "an update of the next period with no handoff before it (signed by the next period's real committee)",
            Some(23),
            apply(&mut w, &u, "next period without a handoff"),
            "the store knows no next committee, so no committee of that period is accepted",
        );
    }
    {
        let mut w = base.w.clone();
        let handoff = in_period(&base, 512, Some(1));
        assert_eq!(apply(&mut w, &handoff, "handoff of committee 1"), Verdict::Applied);
        let u = in_next_period(&base, 1, 2, None);
        put(
            "committee-not-handed-off",
            "after the handoff of committee 1, an update of that period signed by another committee",
            Some(40),
            apply(&mut w, &u, "next period, another committee"),
            "the handed-off committee alone signs for its period",
        );
    }

    // Participation.
    for (key, n, expected) in [("participation-341", 341usize, Some(51)), ("participation-342", 342, None)] {
        let mut w = base.w.clone();
        let u = in_period(&base, n, None);
        let (spec, spec_finalized) = spec_store_verdict(&base.w, &u);
        assert!(spec.is_ok(), "{n} of 512: the specification's store refuses it: {spec:?}");
        assert_eq!(spec_finalized, expected.is_none(), "{n} of 512: the specification finalizes at two thirds only");
        let note = if expected.is_some() {
            "one signer below two thirds of 512; the specification's store does not refuse it, but does not finalize it \
             (it keeps it as its best valid update)"
        } else {
            "control: two thirds of 512 (rounded up)"
        };
        put(key, &format!("{n} of 512 members sign"), expected, apply(&mut w, &u, &format!("{n} of 512")), note);
    }

    // Finality.
    {
        let mut w = base.w.clone();
        let u = base.update(0, None, filler_header(s0 + 96), None, s0 + 97, 512);
        let (spec, spec_finalized) = spec_store_verdict(&base.w, &u);
        assert!(spec.is_ok() && !spec_finalized, "the specification's store on an optimistic update: {spec:?}");
        put(
            "not-finalized",
            "an update without finality (optimistic)",
            Some(50),
            apply(&mut w, &u, "optimistic update"),
            "no header enters without a finality branch; the specification's store does not refuse it, but only moves \
             its optimistic header",
        );
        assert!(w.inst.record(&w.chain, s0 + 96).is_none());
    }
    {
        let mut w = base.w.clone();
        let mut u = in_period(&base, 512, None);
        u.finality_branch[0][0] ^= 1;
        put(
            "finality-branch",
            "a finality branch with one sibling flipped",
            Some(26),
            apply(&mut w, &u, "finality branch flipped"),
            "",
        );
    }
    {
        let mut w = base.w.clone();
        let mut u = in_period(&base, 512, None);
        u.finality_branch.pop();
        put(
            "finality-branch-short",
            "a finality branch one node short",
            Some(1),
            apply(&mut w, &u, "finality branch short"),
            "malformed input",
        );
    }
    {
        let mut w = base.w.clone();
        let mut u = in_period(&base, 512, None);
        u.finalized_header.beacon.slot += 1;
        put(
            "finalized-slot",
            "the finalized header's slot changed, its branch kept",
            Some(26),
            apply(&mut w, &u, "finalized header changed"),
            "the attested state does not prove that header",
        );
    }
    for (key, which) in [("finalized-state-root", "state"), ("finalized-receipts-root", "receipts")] {
        let mut w = base.w.clone();
        let mut u = in_period(&base, 512, None);
        let p = payload_mut(&mut u);
        if which == "state" {
            p.state_root[0] ^= 1
        } else {
            p.receipts_root[0] ^= 1
        }
        put(
            key,
            &format!("a finalized header whose execution {which} root is tampered (beacon header intact)"),
            Some(25),
            apply(&mut w, &u, &format!("{which} root tampered")),
            "the execution branch no longer verifies against the beacon body",
        );
    }
    {
        let mut w = base.w.clone();
        let mut u = in_period(&base, 512, None);
        u.sync_aggregate.signature = [0xff; 96];
        put(
            "signature-malformed",
            "signature bytes that are not a G2 point",
            Some(41),
            apply(&mut w, &u, "signature not a point"),
            "",
        );
    }
    out
}

/// A valid inclusion proof against a header that is signed but not finalised: an update whose
/// attested header carries block `att` and whose finalized header carries block `fin`.
fn not_finalised_cases() -> (Vec<Row>, String) {
    let mut base = synth(0x52, 2);
    let s0 = slot0(&base);
    let contract: Address = [0x0c; 20];
    let fin = Block::new(4_000, &[AccountSpec::contract(contract, vec![(w(1), w(0x1111))])], &[plain_receipt(0)]);
    let att = Block::new(4_010, &[AccountSpec::contract(contract, vec![(w(1), w(0x2222))])], &[plain_receipt(0)]);
    let u = base.update(0, Some(header_of(s0 + 64, &fin)), header_of(s0 + 96, &att), None, s0 + 97, 512);
    let mut rows = Vec::new();
    check(
        &mut rows,
        "the update (finalizes block 4,000; attests block 4,010)",
        None,
        apply(&mut base.w, &u, "attested header with state"),
        "a record is written for the finalized header only",
    );
    assert!(base.w.inst.record(&base.w.chain, s0 + 64).is_some());
    assert!(base.w.inst.record(&base.w.chain, s0 + 96).is_none(), "no record for the attested header");
    let request = Request {
        instance_id: base.w.inst.id,
        chain_id: base.w.inst.config.chain_id,
        accounts: vec![AccountQuery {
            address: contract,
            proof: att.account_proof(&contract),
            conditions: vec![],
            slots: vec![ferrata_lez_wire::attest::SlotQuery {
                slot: w(1),
                proof: att.storage_proof(&contract, &w(1)),
                conditions: vec![Condition { field: Field::Value, cmp: Cmp::Eq, rhs: w(0x2222) }],
            }],
        }],
        ..Request::default()
    };
    // The proof is valid against the attested (not finalised) header: natively, against a record
    // of that header, it verifies.
    let as_if = HeaderRecord::of(&u.attested_header, LcFork::Electra, &base.w.spec);
    let native = evaluate(&base.w.inst.id, base.w.inst.config.chain_id, &as_if, &request);
    assert!(native.is_ok(), "the proof is valid against the attested header: {native:?}");
    let time = base.time(&u);
    let (program, id) = (base.w.inst.program, base.w.inst.id);
    let r = attest_no_record(
        &mut base.w.chain,
        "Attest at the attested header's slot",
        program,
        &id,
        s0 + 96,
        &request,
        time,
    );
    check(
        &mut rows,
        "a valid inclusion proof against the attested, not finalised header, at its slot",
        Some(213),
        Verdict::of(&[r]),
        "no record exists for a header that was not finalised",
    );
    let r = attest_root(
        &mut base.w.chain,
        "Attest at the finalized header's slot",
        header_actor(program, &id, s0 + 64),
        &request,
        time,
    );
    check(
        &mut rows,
        "the same proof at the finalized header's record",
        Some(110),
        Verdict::of(&[r]),
        "the record's state root is the finalized block's",
    );
    (
        rows,
        format!(
            "natively, against a record of the attested header, the same request verifies ({} statements)",
            native.unwrap().statements.len()
        ),
    )
}

/// The execution-side cases on the real mainnet record of Stage 4 (block 26,384,851).
fn attestation_cases() -> Vec<(&'static str, Row)> {
    let mut s = Stage4::new();
    let slot = s.fx.finalized_slot;
    let weth = s.fx.weth_proof();
    let base = |s: &Stage4| s.request();
    let mut out = Vec::new();
    let mut run =
        |s: &mut Stage4, key: &'static str, case: &str, request: &Request, expected: Option<u32>, note: &str| {
            let r = s.attest(case, slot, request);
            let mut rows = Vec::new();
            check(&mut rows, case, expected, Verdict::of(&[r]), note);
            out.push((key, rows.pop().unwrap()));
        };

    // Inclusion proofs.
    let mut tampered = weth.proof.clone();
    let last = tampered.len() - 1;
    let mut leaf = tampered[last].clone();
    let n = leaf.len();
    leaf[n - 1] ^= 1;
    let forged_root = relink(&mut tampered, last, leaf);
    let q = AccountQuery { address: weth.address, proof: tampered, conditions: vec![], slots: vec![] };
    let r = Request { accounts: vec![q], ..base(&s) };
    run(
        &mut s,
        "root-tampered",
        "WETH's account proof re-linked to a tampered state root (self-consistent under it)",
        &r,
        Some(110),
        &format!("the proof's root 0x{}… is not the record's", hex::encode(&forged_root[..6])),
    );
    let mut short = account_query(&weth, vec![], vec![]);
    short.proof.pop();
    let r = Request { accounts: vec![short], ..base(&s) };
    run(&mut s, "proof-incomplete", "WETH's account proof missing its last node", &r, Some(111), "");
    let mut long = account_query(&weth, vec![], vec![]);
    long.proof.push(long.proof[0].clone());
    let r = Request { accounts: vec![long], ..base(&s) };
    run(&mut s, "proof-trailing", "WETH's account proof with a trailing node", &r, Some(112), "");

    // A predicate over state the proof does not cover.
    let mut other_slot = account_query(&weth, vec![], vec![(s.fx.weth_slot, vec![])]);
    other_slot.slots[0].slot = s.fx.empty_slot;
    let r = Request { accounts: vec![other_slot], ..base(&s) };
    run(
        &mut s,
        "uncovered-slot",
        "the proof of one storage slot presented for another slot",
        &r,
        Some(110),
        "the proof covers WETH's holder slot, the request names another",
    );
    let mut other_account = account_query(&weth, vec![], vec![]);
    other_account.address = s.fx.transfer.to;
    let r = Request { accounts: vec![other_account], ..base(&s) };
    run(&mut s, "uncovered-account", "WETH's account proof presented for another address", &r, Some(110), "");
    let t = s.fx.transfer.clone();
    let topic3 =
        s.log_request(t.tx_index, t.log_index, vec![Condition { field: Field::Topic(3), cmp: Cmp::Eq, rhs: [0; 32] }]);
    run(
        &mut s,
        "uncovered-topic",
        "a condition on a fourth topic of a three-topic Transfer log",
        &topic3,
        Some(130),
        "a field the proven log does not have is never satisfied",
    );
    let mut rel = s.log_request(t.tx_index, t.log_index, vec![]);
    rel.relations = vec![Relation {
        left: Operand { fact: 0, field: Field::DataWord(0) },
        cmp: Cmp::Eq,
        right: Operand { fact: 5, field: Field::DataWord(0) },
    }];
    run(&mut s, "relation-out-of-range", "a relation naming a fact the request does not prove", &rel, Some(131), "");

    // A predicate that does not hold.
    let wrong = s.log_request(
        t.tx_index,
        t.log_index,
        vec![Condition { field: Field::Topic(2), cmp: Cmp::Eq, rhs: pad(&[0x99; 20]) }],
    );
    run(&mut s, "predicate", "a Transfer's recipient predicate on another recipient", &wrong, Some(130), "");

    // Chain and instance binding.
    let other_chain = Request { chain_id: SEPOLIA_CHAIN_ID, ..s.log_request(t.tx_index, t.log_index, vec![]) };
    run(
        &mut s,
        "chain-id",
        "the mainnet record asked as a Sepolia (11155111) statement",
        &other_chain,
        Some(140),
        "a proof replayed against another chain id",
    );
    let other_instance = Request { instance_id: [0x77; 32], ..s.log_request(t.tx_index, t.log_index, vec![]) };
    run(&mut s, "instance", "a request bound to another instance", &other_instance, Some(211), "");

    // Malformed input.
    let r = base(&s);
    run(&mut s, "empty-request", "an empty request", &r, Some(131), "");
    let with_header = Request { header: Some(vec![0xc0]), ..s.log_request(t.tx_index, t.log_index, vec![]) };
    run(
        &mut s,
        "bad-query",
        "a block header sent with a request to a record that holds the payload",
        &with_header,
        Some(214),
        "",
    );
    let header = s.header(slot);
    let r = s.w.chain.submit(
        "message bytes that do not decode",
        header,
        vec![0xff, 0x00, 0x13],
        vec![header],
        vec![],
        s.time,
    );
    let mut rows = Vec::new();
    check(&mut rows, "message bytes that do not decode, sent to the record", Some(200), Verdict::of(&[r]), "");
    out.push(("bad-message", rows.pop().unwrap()));
    out
}

// ---------------------------------------------------------------------------------------------
// VS.1
// ---------------------------------------------------------------------------------------------

#[test]
fn vs1_a_fabricated_chain_is_refused_through_the_program() {
    require_dev_mode();
    let mut s = Stage4::new();
    let program = s.w.inst.program;
    let victim = s.w.inst.clone();
    let chain_id = s.fx.chain_id;
    let s0 = s.fx.finalized_slot;
    let (fb, fs, att, sig) = (s0 + 32, s0 + 64, s0 + 96, s0 + 97);
    assert_eq!(sig / PERIOD, s0 / PERIOD, "the forgery is signed in the store's own period");
    let pool = s.pool();
    let t = s.fx.transfer.clone();
    let attacker: Address = [0xa7; 20];
    let amount: u128 = 10u128.pow(15);

    // The fabricated execution block: the attacker's account with 10^24 wei, the token's contract,
    // and, at transaction 1, a Transfer of the pool's token from the attacker to the pool's
    // recipient. Its header is finalised by a forged finality update whose every branch verifies,
    // signed by 512 keys the forger holds.
    let block = Block::new(
        s.fx.block + 5,
        &[AccountSpec::eoa(attacker, 10u128.pow(24)), AccountSpec::contract(t.token, vec![(w(7), w(amount))])],
        &[plain_receipt(0), transfer_receipt(t.token, attacker, t.to, amount)],
    );
    let forger = fake_committee(512, 0xf0);
    let x = update_over(Some(header_of(fs, &block)), filler_header(att), None, sig, 512, &forger, &s.w.ctx());
    let time = s.w.inst.config.slot_start_ms(sig) + 12_000;
    let request = |instance_id: Root| Request {
        instance_id,
        chain_id,
        accounts: vec![AccountQuery {
            address: attacker,
            proof: block.account_proof(&attacker),
            conditions: vec![],
            slots: vec![],
        }],
        logs: vec![LogQuery {
            tx_index: 1,
            log_index: 0,
            proof: block.receipt_proof(1),
            conditions: pool.conditions(),
        }],
        ..Request::default()
    };
    let mut rows = Vec::new();

    // 1. The chain is self-consistent: an instance of the same program that trusts the forger's
    // checkpoint (anyone may Init one) verifies the update, records the header and attests the
    // Transfer and the balance under it.
    let fboot = fabricated_bootstrap(filler_header(fb), &forger, 0xf00);
    let fcheckpoint = fboot.header.beacon.hash_tree_root();
    let finst = Instance::new(program, victim.config.clone(), fcheckpoint);
    let native =
        FinalizedStore::initialize_with(&fcheckpoint, &fboot, finst.fork(), &s.w.spec, &BLS, Adoption::OnChain)
            .unwrap();
    let mut fw = World {
        chain: std::mem::take(&mut s.w.chain),
        inst: finst.clone(),
        native,
        spec: s.w.spec.clone(),
        bootstrap: fboot.clone(),
        updates: vec![],
        runs: vec![],
    };
    let init = fw.inst.init(&mut fw.chain, &fboot, "forger: Init");
    check(
        &mut rows,
        "control: a forger initialises his own instance from the fabricated checkpoint (Init is permissionless)",
        None,
        Verdict::of(&init.reports),
        "same program, same mainnet configuration, another instance id",
    );
    let run = fw.apply(&x, "forger's instance: the fabricated update");
    check(
        &mut rows,
        "control: the forger's instance verifies the fabricated finality update",
        None,
        Verdict::of(&run.reports),
        "Ferrata's policy store (`FinalizedStore`) bootstrapped from the forger's checkpoint accepts it too",
    );
    s.w.chain = std::mem::take(&mut fw.chain);
    let frecord = finst.record(&s.w.chain, fs).expect("the forger's instance recorded the fabricated header");
    let fattested = evaluate(&frecord.instance_id, frecord.chain_id, &frecord.header().unwrap(), &request(finst.id))
        .expect("the fabricated proofs verify under the fabricated header");
    let fheader = header_actor(program, &finst.id, fs);
    let r = attest_root(
        &mut s.w.chain,
        "forger's record: the fabricated Transfer and balance",
        fheader,
        &request(finst.id),
        time,
    );
    assert_eq!(attested(&r), Some(fattested.event_data()));
    check(&mut rows, "control: the forger's record attests the fabricated Transfer and the attacker's balance (request bound to the forger's instance)", None, Verdict::of(&[r]), "a self-consistent header and inclusion proof over state never finalised on the canonical chain");

    // 2. The canonical instance refuses the chain at every door.
    let before = s.w.inst.store(&s.w.chain);
    let v = apply(&mut s.w, &x, "canonical instance: the fabricated update");
    check(
        &mut rows,
        "the fabricated finality update sent to the canonical instance (Open, Miller, Finish)",
        Some(40),
        v,
        "the committee was never handed off from the checkpoint",
    );
    assert_eq!(s.w.inst.store(&s.w.chain), before, "a refused update changes nothing");
    assert!(s.w.inst.record(&s.w.chain, fs).is_none(), "no record of the fabricated header");
    let r = verify_all(&mut s.w, &x, time, "canonical instance: the fabricated update by VerifyAll");
    check(&mut rows, "the same update in one transaction (VerifyAll)", Some(40), Verdict::of(&[r]), "");
    let r = attest_no_record(
        &mut s.w.chain,
        "canonical: Attest at the fabricated slot",
        program,
        &victim.id,
        fs,
        &request(victim.id),
        time,
    );
    check(
        &mut rows,
        "the fabricated proofs at the canonical instance's record of the fabricated header's slot",
        Some(213),
        Verdict::of(&[r]),
        "no record exists there",
    );
    let real_header = s.header(s0);
    let r = attest_root(&mut s.w.chain, "canonical: Attest at the real record", real_header, &request(victim.id), time);
    check(
        &mut rows,
        "the fabricated proofs at the canonical instance's real record (block 26,384,851)",
        Some(110),
        Verdict::of(&[r]),
        "the proofs do not lead to the real record's state root",
    );
    let r = attest_root(
        &mut s.w.chain,
        "forger's record, request bound to the canonical instance",
        fheader,
        &request(victim.id),
        time,
    );
    check(
        &mut rows,
        "the forger's record asked by a caller bound to the canonical instance",
        Some(211),
        Verdict::of(&[r]),
        "a consumer bound to one instance never accepts another's statement",
    );

    // 3. The reference consumer, bound to the canonical instance, claims the fabricated Transfer.
    let log_id = fattested.statements[1].id;
    let pool_id = pool.id();
    let n = nullifier(&pool_id, &log_id);
    let claim = claim_actor(consumer_account(), &pool_id, &n);
    let balance = balance_actor(consumer_account(), &pool_id, &attacker);
    for (slot, header, expected, note) in [
        (
            fs,
            header_actor(program, &victim.id, fs),
            213,
            "the consumer reads the canonical instance's record of that slot: none",
        ),
        (s0, s.header(s0), 110, "the real record's roots refuse the fabricated receipt proof"),
    ] {
        let message = ConsumerMessage::Claim {
            pool: pool.clone(),
            slot,
            tx_index: 1,
            log_index: 0,
            proof: block.receipt_proof(1),
            header: None,
        };
        let r = s.w.chain.submit(
            "reference consumer: Claim of the fabricated Transfer",
            claim,
            borsh::to_vec(&message).unwrap(),
            vec![claim, header, balance],
            [PublicAccountEvidence::Pda { program: consumer_account(), seed: claim_seed(&pool_id, &n) }]
                .into_iter()
                .chain(
                    (slot == fs).then(|| PublicAccountEvidence::Pda { program, seed: header_seed(&victim.id, slot) }),
                )
                .collect(),
            time,
        );
        check(
            &mut rows,
            &format!("the reference consumer's Claim of the fabricated Transfer at slot {slot}"),
            Some(expected),
            Verdict::of(&[r]),
            note,
        );
    }
    assert!(consumer_state(&s.w.chain, balance).is_none(), "the attacker is credited nothing");

    // 4. Controls on the canonical instance: the real Transfer is attested and claimed.
    let real = s.log_request(t.tx_index, t.log_index, pool.conditions());
    let (r, _) = s.accept("control: the real Transfer under the real record", s0, &real);
    check(&mut rows, "control: the real Transfer under the canonical record", None, Verdict::of(&[r]), "");
    let id = s.transfer_id(&t);
    let r = s.claim("control: the reference consumer claims the real Transfer", &pool, s0, &t, &id);
    check(&mut rows, "control: the reference consumer claims the real Transfer", None, Verdict::of(&[r]), "");

    let out = format!(
        "# VS.1: a fabricated chain through the attestation program\n\n\
         Generated by `RISC0_DEV_MODE=1 cargo test -p harness --locked --test security_lc vs1` ({}; LEZ's state machine \
         and fee settlement in process, dev mode). The canonical instance is Stage 4's: mainnet, initialised from the \
         checkpoint of slot {} and advanced by the real finality update of slot {s0} (block 26,384,851). The forger \
         fabricates a self-consistent chain in the store's own period: a checkpoint at slot {fb} proving a committee of \
         512 keys he holds, and a finality update (finalized {fs}, attested {att}, signed at {sig} by all 512) whose \
         finalized header carries block {} with real Merkle-Patricia tries: the attacker's account with 10^24 wei and, at \
         transaction 1, a Transfer of the pool's token ({}) to the pool's recipient. Every branch, proof and signature \
         verifies under the forger's committee.\n\n{}\n\
         The forged chain is accepted only by an instance that trusts the forger's checkpoint, and a consumer bound to the \
         canonical instance never reads that instance's records (211 when asked directly; its own reads go to the \
         canonical instance's record actors). Which instance is canonical is the consumer's choice, checked against public \
         Ethereum (F.9).\n",
        harness::LEZ_TAG,
        s.fx.bootstrap_slot,
        block.number,
        hex::encode(t.token),
        table(&rows),
    );
    write("vs1-fabricated-chain.md", &out);
    println!("{out}");
}

// ---------------------------------------------------------------------------------------------
// VS.2 and F.2
// ---------------------------------------------------------------------------------------------

#[test]
fn vs2_and_f2_forgeries_are_refused_through_the_program() {
    require_dev_mode();
    let lc = light_client_cases();
    let at = attestation_cases();
    let (nf, nf_note) = not_finalised_cases();
    let pick = |keys: &[&str]| -> Vec<&Row> {
        keys.iter()
            .map(|k| &lc.iter().chain(at.iter()).find(|(key, _)| key == k).unwrap_or_else(|| panic!("no case {k}")).1)
            .collect()
    };
    let render = |rows: Vec<&Row>| {
        let mut out = String::from("| Case | Expected | Observed through the program | Note |\n|---|---|---|---|\n");
        for r in rows {
            out.push_str(&format!("| {} | {} | {} | {} |\n", r.case, r.expected, r.verdict.show(), r.note));
        }
        out
    };
    let vs2 = format!(
        "# VS.2: the six forgeries through the attestation program\n\n\
         Generated by `RISC0_DEV_MODE=1 cargo test -p harness --locked --test security_lc vs2` ({}; in process, dev \
         mode). Light-client cases on a synthetic mainnet-preset chain whose committees the test holds (every update \
         sent with `World::apply` also runs through Ferrata's policy store, `FinalizedStore`, the native store whose rules \
         the program runs, which must give the same verdict; the adoption with a replaced key, 45, a refusal that store \
         does not model, is sent as the program's own transactions only; the specification's store, `LightClientStore`, \
         is run on the two cases where it and the policy store part, 50 and 51: it refuses neither, and finalizes \
         neither); execution cases on Stage 4's real mainnet record \
         (block 26,384,851). The fabricated chain of VS.1 is in `vs1-fabricated-chain.md`.\n\n\
         ## 1. A header signed by a committee never validly handed off from the checkpoint\n\n{}\n\
         ## 2. A signature below the participation threshold\n\n{}\n\
         ## 3. A valid inclusion proof against a header that is not finalised\n\n{}\n{nf_note}.\n\n\
         ## 4. A valid finalised header with a tampered state or receipts root\n\n{}\n\
         ## 5. A proof replayed against another chain id\n\n{}\n\
         ## 6. A predicate over state the inclusion proof does not cover\n\n{}",
        harness::LEZ_TAG,
        render(pick(&["committee-foreign", "handoff-branch", "handoff-adoption", "handoff-none", "committee-not-handed-off"])),
        render(pick(&["participation-341", "participation-342"])),
        render(nf.iter().collect()),
        render(pick(&["finalized-state-root", "finalized-receipts-root", "root-tampered"])),
        render(pick(&["chain-id", "instance"])),
        render(pick(&["uncovered-slot", "uncovered-account", "uncovered-topic", "relation-out-of-range"])),
    );
    write("vs2-forgeries.md", &vs2);
    let f2 = format!(
        "# F.2: only finalised headers, through the attestation program\n\n\
         Generated by `RISC0_DEV_MODE=1 cargo test -p harness --locked --test security_lc vs2` ({}; in process, dev \
         mode), on the synthetic chain. A record is written only by a verified finality update, for its finalized header; \
         every update that offers a header without verified finality is refused, and so is a proof against a header \
         never finalised. The two other ways of offering a header are refused through the program in earlier runs: a \
         block header sent with a request ([`stage5-attest-refusals.md`](../stage5-attest-refusals.md): 215, 120) \
         and EIP-2935 hop headers ([`stage4-attest-refusals.md`](../stage4-attest-refusals.md): 150, 151).\n\n{}\n{}\n\
         {nf_note}.\n",
        harness::LEZ_TAG,
        render(pick(&["not-finalized", "finality-branch", "finality-branch-short", "finalized-slot", "finalized-state-root", "finalized-receipts-root"])),
        render(nf.iter().collect()),
    );
    write("f2-finalised-only.md", &f2);
    println!("{vs2}\n{f2}");
}

// ---------------------------------------------------------------------------------------------
// U.5
// ---------------------------------------------------------------------------------------------

/// The weak-subjectivity cases: a synthetic instance whose period is one epoch.
fn weak_subjectivity_cases() -> Vec<Row> {
    let mut config = mainnet_config(2);
    config.ws_period_epochs = 1;
    let slot0 = EXPECTED_FINALIZED[0].0 + 64;
    let base = Synth::new(config.clone(), 0x61, slot0, filler_header(slot0), 2);
    let end = config.ws_end_slot(slot0).expect("a period that ends");
    assert!(end > slot0 + 1 && end / PERIOD == slot0 / PERIOD, "the end falls in the checkpoint's period ({end})");
    let mut rows = Vec::new();
    // Signed at the end: refused in the guest.
    let u = base.update(0, Some(filler_header(slot0 + 1)), filler_header(end - 1), None, end, 512);
    let mut w = base.w.clone();
    let time = base.time(&u);
    let reports = run_job(&mut w, &u, None, time, "update signed at the weak-subjectivity end");
    check(
        &mut rows,
        "an update signed at or after the end of the store's weak-subjectivity period (Open)",
        Some(53),
        Verdict::of(&reports),
        "the program's own code",
    );
    let mut w = base.w.clone();
    let r = verify_all(&mut w, &u, time, "the same by VerifyAll");
    check(&mut rows, "the same update in one transaction (VerifyAll)", Some(53), Verdict::of(&[r]), "");
    // Signed inside the period, sent after its end by the sequencer's clock: LEZ refuses it on the
    // validity window the program declares (the program has no clock of its own).
    let u = base.update(0, Some(filler_header(slot0 + 1)), filler_header(slot0 + 2), None, slot0 + 3, 512);
    let mut w = base.w.clone();
    let late = config.slot_start_ms(end) + 1;
    let reports = run_job(&mut w, &u, None, late, "a valid update sent after the end");
    let v = Verdict::of(&reports);
    assert!(matches!(&v, Verdict::Lez { revert, .. } if revert.contains("validity window")), "{}", v.show());
    rows.push(row(
        "a valid update sent after the end of the period by the sequencer's clock",
        "refused by LEZ on the program's validity window (no program code)",
        v,
        "the transaction is outside the window `[signature slot, weak-subjectivity end)` the program declared",
    ));
    // `Init` from a checkpoint whose period has ended by the sequencer's clock: the program
    // declares the window `[.., end of the checkpoint's period)` at `Init`, and LEZ refuses the
    // transaction outside it. Control: the same `Init` one millisecond before the end applies.
    let init_at = |time: u64, label: &str| {
        let mut chain = harness::Chain::new();
        let inst = Instance::new(ferrata_account(), config.clone(), base.w.inst.checkpoint);
        let compact = CompactBootstrap::of(&base.w.bootstrap);
        let fork = inst.fork();
        let (_, job) = steps::initialize(&inst.checkpoint, &compact, fork, &config.spec(), config.adoption_parts)
            .expect("the bootstrap verifies");
        let jid = job_id(&inst.id, &job.store_digest, &job.update_digest);
        let init =
            Message::Init { config: config.clone(), checkpoint: inst.checkpoint, bootstrap: compact.encode(fork) };
        chain.submit(
            label,
            inst.actor(),
            borsh::to_vec(&init).unwrap(),
            vec![inst.actor(), job_actor(inst.program, &inst.id, &jid)],
            vec![PublicAccountEvidence::Pda { program: inst.program, seed: instance_seed(&inst.id) }],
            time,
        )
    };
    let end_ms = config.ws_end_ms(slot0).expect("a period that ends");
    let control = init_at(end_ms - 1, "Init one millisecond before the checkpoint's period ends");
    assert!(control.ok(), "the control Init: {:?}", control.charge.revert);
    let v = Verdict::of(&[init_at(end_ms, "Init from a checkpoint whose period has ended")]);
    assert!(matches!(&v, Verdict::Lez { revert, .. } if revert.contains("validity window")), "{}", v.show());
    rows.push(row(
        "`Init` from a checkpoint whose weak-subjectivity period has ended, by the sequencer's clock",
        "refused by LEZ on the program's validity window (no program code)",
        v,
        "the program declares the window `[.., end of the checkpoint's period)` at `Init`; the same `Init` one millisecond before the end applies",
    ));
    // A successor (`Succeed`) from a checkpoint that is not the instance's latest record.
    let mut s = synth(0x64, 2);
    let u = in_period(&s, 512, None);
    assert_eq!(apply(&mut s.w, &u, "a finality update (the latest record moves)"), Verdict::Applied);
    let parent = s.w.inst.clone();
    let root_id = parent.state(&s.w.chain).expect("the instance").root_id;
    let earlier = s.w.bootstrap.header.beacon.hash_tree_root();
    let successor = instance_actor(parent.program, &successor_id(&root_id, &parent.id, &parent.config, &earlier));
    let r = s.w.chain.submit(
        "Succeed from an earlier record",
        parent.actor(),
        borsh::to_vec(&Message::Succeed { config: parent.config.clone(), checkpoint: earlier }).unwrap(),
        vec![parent.actor(), successor],
        vec![],
        s.time(&u),
    );
    check(
        &mut rows,
        "a successor (`Succeed`) from the instance's checkpoint after a later record was written",
        Some(u32::from(ferrata_lez_wire::code::STALE_CHECKPOINT)),
        Verdict::of(&[r]),
        "a stale checkpoint: a successor starts only from its parent's latest record",
    );
    rows
}

#[test]
fn u5_every_listed_failure_mode_returns_its_code_through_the_program() {
    require_dev_mode();
    let lc = light_client_cases();
    let at = attestation_cases();
    let (nf, _) = not_finalised_cases();
    let ws = weak_subjectivity_cases();
    let idl = idl_codes();
    let find =
        |k: &str| &lc.iter().chain(at.iter()).find(|(key, _)| *key == k).unwrap_or_else(|| panic!("no case {k}")).1;
    let modes: Vec<(&str, Vec<&Row>)> = vec![
        ("header not finalised", vec![find("not-finalized"), find("finality-branch"), &nf[1]]),
        ("sync-committee signature invalid", vec![find("committee-foreign"), find("signature-malformed")]),
        (
            "committee handoff not verified",
            vec![find("handoff-branch"), find("handoff-adoption"), find("handoff-none")],
        ),
        ("checkpoint stale or outside the weak-subjectivity period", ws.iter().collect()),
        ("inclusion proof invalid", vec![find("root-tampered"), find("proof-incomplete"), find("proof-trailing")]),
        ("predicate not satisfied", vec![find("predicate")]),
        ("chain-ID mismatch", vec![find("chain-id")]),
        (
            "malformed input",
            vec![find("finality-branch-short"), find("empty-request"), find("bad-query"), find("bad-message")],
        ),
    ];
    let mut out = format!(
        "# U.5: every listed failure mode, through the attestation program\n\n\
         Generated by `RISC0_DEV_MODE=1 cargo test -p harness --locked --test security_lc u5` ({}; in process, dev mode). \
         Each failure mode of RFP-021 Usability #5, sent as the transactions a submitter or consumer sends; the code each \
         returns, and its name and message as the IDL (`idl/ferrata_attestation.idl.json`) gives them to the CLI \
         (`ferrata-attest` prints them) and to any client; where LEZ refuses the transaction before the program runs, \
         LEZ's own text, which is what a client shows (`ferrata-attest` prints it as it is).\n\n\
         | Failure mode | Case | Observed through the program | Message a client shows (the IDL's; LEZ's own text where LEZ refuses) |\n|---|---|---|---|\n",
        harness::LEZ_TAG
    );
    for (mode, rows) in &modes {
        assert!(!rows.is_empty(), "{mode}");
        for r in rows {
            let msg = match r.verdict.code() {
                Some(code) => {
                    let (_, msg) = idl.get(&code).unwrap_or_else(|| panic!("code {code} is not in the IDL"));
                    assert!(!msg.is_empty(), "code {code} has no message in the IDL");
                    msg.clone()
                }
                None => match &r.verdict {
                    Verdict::Lez { revert, .. } => {
                        assert!(!revert.is_empty(), "{}: LEZ refused without a text", r.case);
                        format!("LEZ's own text: \"{revert}\" ({})", r.note)
                    }
                    other => panic!("{}: neither a code nor LEZ's refusal: {}", r.case, other.show()),
                },
            };
            out.push_str(&format!("| {mode} | {} | {} | {msg} |\n", r.case, r.verdict.show()));
        }
    }
    write("u5-failure-modes.md", &out);
    println!("{out}");
}

// ---------------------------------------------------------------------------------------------
// F.4
// ---------------------------------------------------------------------------------------------

/// `w` plus `d` (a small signed step), as a 256-bit big-endian word.
fn step(w: Word, d: i64) -> Word {
    let mut x = primitive_u256(&w);
    if d >= 0 {
        x = x.checked_add(d as u128).expect("no overflow");
    } else {
        x = x.checked_sub(d.unsigned_abs() as u128).expect("no underflow");
    }
    let mut out = [0u8; 32];
    out[16..].copy_from_slice(&x.to_be_bytes());
    out
}

/// The word as a u128 (the values used here fit).
fn primitive_u256(w: &Word) -> u128 {
    assert_eq!(w[..16], [0u8; 16], "fits 128 bits");
    u128::from_be_bytes(w[16..].try_into().unwrap())
}

#[test]
fn f4_every_comparison_from_the_consumer() {
    require_dev_mode();
    let mut s = Stage4::new();
    let slot = s.fx.finalized_slot;
    let weth = s.fx.weth_proof();
    let b = weth.balance;
    let (t, o) = (s.fx.transfer.clone(), s.fx.other_recipient.clone());
    let (a1, a2) = (t.value, o.value);
    assert_ne!(a1, a2, "two Transfers of different amounts");
    let log = |s: &Stage4, tr: &harness::attest::Transfer, conditions: Vec<Condition>| LogQuery {
        tx_index: tr.tx_index,
        log_index: tr.log_index,
        proof: s.fx.receipt_proof(tr.tx_index),
        conditions,
    };
    let mut rows = Vec::new();
    let mut run = |s: &mut Stage4, case: String, request: Request, holds: bool| {
        let (r, reply) = s.call(&case, slot, &request);
        let expected = s.expected(slot, &request);
        if holds {
            let reply = reply.unwrap_or_else(|| panic!("{case}: refused {:?}", r.exit_code));
            assert_eq!(Ok(reply), expected, "{case}: the consumer keeps what the native evaluation gives");
        } else {
            assert_eq!(expected.err(), Some(130), "{case}: natively");
        }
        check(&mut rows, &case, if holds { None } else { Some(130) }, Verdict::of(&[r]), "");
    };
    let all = [Cmp::Eq, Cmp::Ne, Cmp::Lt, Cmp::Le, Cmp::Gt, Cmp::Ge];
    // Against a constant: WETH's balance, and the first Transfer's amount (data word 0).
    for cmp in all {
        let (hold, fail) = match cmp {
            Cmp::Eq => (0, 1),
            Cmp::Ne => (1, 0),
            Cmp::Lt => (1, 0),
            Cmp::Le => (0, -1),
            Cmp::Gt => (-1, 0),
            Cmp::Ge => (0, 1),
        };
        for (d, holds) in [(hold, true), (fail, false)] {
            let rhs = step(b, d);
            let q = account_query(&weth, vec![Condition { field: Field::Balance, cmp, rhs }], vec![]);
            let case =
                format!("WETH's balance {cmp:?} balance{}", if d == 0 { String::new() } else { format!("{d:+}") });
            let request = Request { accounts: vec![q], ..s.request() };
            run(&mut s, case, request, holds);
            let rhs = step(a1, d);
            let case =
                format!("Transfer amount {cmp:?} amount{}", if d == 0 { String::new() } else { format!("{d:+}") });
            let request = Request {
                logs: vec![log(&s, &t, vec![Condition { field: Field::DataWord(0), cmp, rhs }])],
                ..s.request()
            };
            run(&mut s, case, request, holds);
        }
    }
    // Between two facts of one request: the amounts of two Transfers (facts 0 and 1) and the
    // first again (fact 2).
    let (lo, hi) = if primitive_u256(&a1) < primitive_u256(&a2) { (0u32, 1u32) } else { (1, 0) };
    let facts = Request { logs: vec![log(&s, &t, vec![]), log(&s, &o, vec![]), log(&s, &t, vec![])], ..s.request() };
    let pair = |l: u32, r: u32| {
        (Operand { fact: l, field: Field::DataWord(0) }, Operand { fact: r, field: Field::DataWord(0) })
    };
    for cmp in all {
        let (hold, fail) = match cmp {
            Cmp::Eq => ((0, 2), (lo, hi)),
            Cmp::Ne => ((lo, hi), (0, 2)),
            Cmp::Lt => ((lo, hi), (0, 2)),
            Cmp::Le => ((0, 2), (hi, lo)),
            Cmp::Gt => ((hi, lo), (0, 2)),
            Cmp::Ge => ((0, 2), (lo, hi)),
        };
        for ((l, r), holds) in [(hold, true), (fail, false)] {
            let (left, right) = pair(l, r);
            let request = Request { relations: vec![Relation { left, cmp, right }], ..facts.clone() };
            run(&mut s, format!("relation: amount of fact {l} {cmp:?} amount of fact {r}"), request, holds);
        }
    }
    let out = format!(
        "# F.4: every comparison, carried in the consumer's request, through the attestation program\n\n\
         Generated by `RISC0_DEV_MODE=1 cargo test -p harness --locked --test security_lc f4` ({}; in process, dev \
         mode). Each request is the reference consumer's (`ConsumerMessage::CallAttest` at a public actor of the consumer \
         program): the consumer program calls `Attest` at the real mainnet record of block 26,384,851 and keeps the reply, \
         so the predicate is set by the consuming program without any change to the attestation program. Each comparison \
         is run where it holds (the transaction applies and the consumer keeps exactly the native evaluation's reply) and \
         where it fails (130). Facts: WETH's balance ({} wei), the amounts of two USDC Transfers ({} and {}; fact 2 is the \
         first again).\n\n{}",
        harness::LEZ_TAG,
        primitive_u256(&b),
        primitive_u256(&a1),
        primitive_u256(&a2),
        table(&rows)
    );
    write("f4-comparisons.md", &out);
    println!("{out}");
}

// ---------------------------------------------------------------------------------------------
// R.3
// ---------------------------------------------------------------------------------------------

/// A public actor of the consumer reads `header(id, slot)` and keeps it; returns the record bytes
/// it kept.
fn read_record(w: &mut World, slot: u64, time: u64, label: &str) -> (TxReport, Option<Vec<u8>>) {
    let seed = PdaSeed::new([0x45; 32]);
    let program = consumer_account();
    let reader = Actor::new(lee::AccountId::for_public_pda(&program, &seed), program);
    let header = header_actor(w.inst.program, &w.inst.id, slot);
    let message = ConsumerMessage::ReadRecord { ferrata: w.inst.program, instance_id: w.inst.id, slot, query: vec![] };
    let r = w.chain.submit(
        label,
        reader,
        borsh::to_vec(&message).unwrap(),
        vec![reader, header],
        vec![PublicAccountEvidence::Pda { program, seed }],
        time,
    );
    let kept = match consumer_state(&w.chain, reader) {
        Some(ConsumerState::Reader(state)) if r.ok() => state.record,
        _ => None,
    };
    (r, kept)
}

#[test]
fn r3_records_stay_valid_after_committee_rotations() {
    require_dev_mode();
    let mut out = format!(
        "# R.3: statements over finalised headers stay valid after committee rotations\n\n\
         Generated by `RISC0_DEV_MODE=1 cargo test -p harness --locked --test security_lc r3` ({}; in process, dev mode).\n\n",
        harness::LEZ_TAG
    );

    // 1. Synthetic: records attested before and after each of four rotations, and ten years later.
    let slot0 = EXPECTED_FINALIZED[0].0 + 64;
    let contract: Address = [0x0c; 20];
    let token: Address = [0x0d; 20];
    let kblock = Block::new(
        500,
        &[AccountSpec::contract(contract, vec![(w(1), w(0xc0ffee))])],
        &[transfer_receipt(token, [1; 20], [2; 20], 77)],
    );
    let rblock = Block::new(
        600,
        &[AccountSpec::contract(contract, vec![(w(1), w(0xbeef))])],
        &[plain_receipt(0), transfer_receipt(token, [3; 20], [4; 20], 99)],
    );
    let mut s = Synth::new(mainnet_config(2), 0x41, slot0, header_of(slot0, &kblock), 6);
    let (program, id, chain_id) = (s.w.inst.program, s.w.inst.id, s.w.inst.config.chain_id);
    let f0 = slot0 + 256;
    let u0 = s.update(0, Some(header_of(f0, &rblock)), filler_header(slot0 + 320), Some(1), slot0 + 321, 512);
    assert_eq!(apply(&mut s.w, &u0, "record of block 600, handoff of committee 1"), Verdict::Applied);
    let request = |b: &Block, value: u128, tx: u64| Request {
        instance_id: id,
        chain_id,
        accounts: vec![AccountQuery {
            address: contract,
            proof: b.account_proof(&contract),
            conditions: vec![],
            slots: vec![ferrata_lez_wire::attest::SlotQuery {
                slot: w(1),
                proof: b.storage_proof(&contract, &w(1)),
                conditions: vec![Condition { field: Field::Value, cmp: Cmp::Eq, rhs: w(value) }],
            }],
        }],
        logs: vec![LogQuery { tx_index: tx, log_index: 0, proof: b.receipt_proof(tx), conditions: vec![] }],
        ..Request::default()
    };
    let (rk, rr) = (request(&kblock, 0xc0ffee, 0), request(&rblock, 0xbeef, 1));
    let records = |w: &World| (w.inst.record(&w.chain, slot0).unwrap(), w.inst.record(&w.chain, f0).unwrap());
    let written = records(&s.w);
    let mut t = s.time(&u0);
    let k0 = attest_root(&mut s.w.chain, "checkpoint record, before", header_actor(program, &id, slot0), &rk, t);
    let r0 = attest_root(&mut s.w.chain, "record of block 600, before", header_actor(program, &id, f0), &rr, t);
    let (ek, er) = (attested(&k0).expect("attested"), attested(&r0).expect("attested"));
    let mut rows = vec![row(
        "before any rotation: both records attested",
        "applied",
        Verdict::of(&[k0, r0]),
        "the statements every later row must reproduce",
    )];
    for k in 1..=4u64 {
        let u = in_next_period(&s, k, k as usize, Some(k as usize + 1));
        assert_eq!(apply(&mut s.w, &u, &format!("rotation into period p0+{k}")), Verdict::Applied);
        let store = s.w.inst.store(&s.w.chain);
        assert_eq!(store.finalized.slot / PERIOD, s.p0 + k, "the store moved into the next period");
        assert_eq!(records(&s.w), written, "the records are unchanged");
        t = s.time(&u);
        let a = attest_root(&mut s.w.chain, "checkpoint record", header_actor(program, &id, slot0), &rk, t);
        let b = attest_root(&mut s.w.chain, "record of block 600", header_actor(program, &id, f0), &rr, t);
        assert_eq!(
            (attested(&a), attested(&b)),
            (Some(ek.clone()), Some(er.clone())),
            "rotation {k}: the same statements"
        );
        rows.push(row(
            &format!("after rotation {k} (committee {k} current, committee {} handed off)", k + 1),
            "applied, the same statements",
            Verdict::of(&[a, b]),
            "event data byte-identical to the first row's; record bytes unchanged",
        ));
    }
    let far = t + 10 * YEAR_MS;
    let a = attest_root(&mut s.w.chain, "checkpoint record, ten years on", header_actor(program, &id, slot0), &rk, far);
    let b = attest_root(&mut s.w.chain, "record of block 600, ten years on", header_actor(program, &id, f0), &rr, far);
    assert_eq!((attested(&a), attested(&b)), (Some(ek.clone()), Some(er.clone())));
    rows.push(row(
        "ten years after the last update (the instance long past its weak-subjectivity period)",
        "applied, the same statements",
        Verdict::of(&[a, b]),
        "`Attest` carries no validity window",
    ));
    let u5 = in_next_period(&s, 5, 5, None);
    let reports = run_job(&mut s.w, &u5, None, far, "an update ten years on");
    let v = Verdict::of(&reports);
    assert!(matches!(&v, Verdict::Lez { revert, .. } if revert.contains("validity window")), "{}", v.show());
    rows.push(row(
        "the stale instance itself, ten years on: a valid update",
        "refused (the instance is replaced, F.10)",
        v,
        "only advancing stops; the records stay valid",
    ));
    out.push_str(&format!(
        "## 1. Synthetic chain: four rotations\n\nA mainnet-preset instance whose committees the test holds (every update sent with `World::apply` also runs through Ferrata's policy store, `FinalizedStore`, the native store whose rules the program runs, which must give the same verdict; the update ten years on, refused on LEZ's validity window, which that store does not model, is sent as the program's own transactions only): the checkpoint (slot {slot0}) binds block 500, a finality update records block 600 at slot {f0} and hands off committee 1; then four updates, each in the next period, rotate the committee and hand off the next one. Both records are attested (a storage slot with a condition and a Transfer log) before the first rotation, after each, and ten years later.\n\n{}\n",
        table(&rows)
    ));

    // 2. Mainnet, real data: the records of periods 1899 to 1903 read after the four real rotations.
    let mut w = World::new(2);
    let mut written =
        vec![(w.bootstrap.header.beacon.slot, w.inst.record(&w.chain, w.bootstrap.header.beacon.slot).unwrap())];
    for i in 0..5 {
        let u = w.updates[i].clone();
        assert_eq!(apply(&mut w, &u, &format!("update_{i}")), Verdict::Applied);
        let slot = w.inst.store(&w.chain).finalized.slot;
        if written.iter().all(|(s, _)| *s != slot) {
            written.push((slot, w.inst.record(&w.chain, slot).unwrap()));
        }
    }
    let last = w.updates[4].signature_slot;
    let mut rows = Vec::new();
    for (when, time) in [
        ("after the four rotations", w.inst.config.slot_start_ms(last) + 12_000),
        ("ten years later", w.inst.config.slot_start_ms(last) + 10 * YEAR_MS),
    ] {
        for (slot, record) in &written {
            let (r, kept) = read_record(&mut w, *slot, time, &format!("read the record of slot {slot} {when}"));
            assert_eq!(kept, Some(record.record.clone()), "slot {slot}: the record read is the record written");
            rows.push(row(
                &format!("the reference consumer reads the record of slot {slot} (period {}) {when}", slot / PERIOD),
                "applied, the bytes written",
                Verdict::of(&[r]),
                "",
            ));
        }
    }
    out.push_str(&format!(
        "## 2. Mainnet, real data: four real rotations\n\nThe mainnet instance of periods 1899–1903 (`World::new`, fixture `mainnet-p1899`): Init, then the five real updates as served (four rotate the committee). Every record it wrote is read afterwards by a public actor of the reference consumer (`ReadRecord`) and must be the bytes written (no execution proofs exist for these blocks in the fixtures, so they are read, not attested).\n\n{}\n",
        table(&rows)
    ));

    // 3. Sepolia, real data: the same storage slot attested before and after a real rotation.
    let mut w = sepolia::replay(2, 1408);
    let fx = SepoliaFacts::load();
    assert_eq!(apply(&mut w, &fx.update, "the record of slot S"), Verdict::Applied);
    let usdc = fx.storage_proof();
    let request = Request {
        instance_id: w.inst.id,
        chain_id: SEPOLIA_CHAIN_ID,
        header: Some(fx.header.clone()),
        accounts: vec![account_query(
            &usdc,
            vec![],
            vec![(fx.slot, vec![Condition { field: Field::Value, cmp: Cmp::Eq, rhs: fx.value }])],
        )],
        ..Request::default()
    };
    let header = header_actor(w.inst.program, &w.inst.id, fx.finalized_slot);
    let t0 = w.inst.config.slot_start_ms(fx.update.signature_slot) + 12_000;
    let before = attest_root(&mut w.chain, "USDC's totalSupply_ under S, before", header, &request, t0);
    let e0 = attested(&before).expect("attested");
    let period_before = w.inst.store(&w.chain).finalized.slot / PERIOD;
    assert_eq!(
        apply(&mut w, &sepolia::sepolia_update(1409), "period 1409's update (hands off 1410's committee)"),
        Verdict::Applied
    );
    let finality = sepolia::gloas_finality_update(&vault_file("finality_update.ssz"));
    assert_eq!(apply(&mut w, &finality, "a finality update of period 1410"), Verdict::Applied);
    let period_after = w.inst.store(&w.chain).finalized.slot / PERIOD;
    assert!(period_after > period_before, "the store rotated ({period_before} -> {period_after})");
    let t1 = w.inst.config.slot_start_ms(finality.signature_slot) + 12_000;
    let after = attest_root(&mut w.chain, "USDC's totalSupply_ under S, after", header, &request, t1);
    assert_eq!(attested(&after), Some(e0), "the same statement after the rotation");
    let rows = vec![
        row(
            &format!(
                "USDC's `totalSupply_` (= {}) under the record of slot {} (period {period_before})",
                hex::encode(fx.value),
                fx.finalized_slot
            ),
            "applied",
            Verdict::of(&[before]),
            "",
        ),
        row(
            &format!("the same after the handoff and the rotation into period {period_after}"),
            "applied, the same statement",
            Verdict::of(&[after]),
            "event data byte-identical",
        ),
    ];
    out.push_str(&format!(
        "## 3. Sepolia, real data: one real rotation\n\nThe Sepolia instance replayed from period 1378 to 1408, the attestation fixture's finality update (the record of slot S, a Gloas record of block {}), then period 1409's update (hands off period 1410's committee) and the vault fixture's finality update (period 1410): the store rotated. The fixtures hold no further continuous updates.\n\n{}",
        fx.block,
        table(&rows)
    ));
    write("r3-rotations.md", &out);
    println!("{out}");
}
