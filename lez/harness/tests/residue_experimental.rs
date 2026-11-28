//! EXPERIMENTAL, bench-only: what LEZ's state machine charges for a finality update verified in
//! one public transaction with the residue-witness check (`bench/RESIDUE.md`) instead of the final
//! exponentiation.
//!
//! The program is `guests/program-residue-experimental`, an unaudited copy of `guests/program`
//! whose `VerifyAll` takes the residue witness `c` and the scaling factor `w` after the signer
//! witness and checks `c != 0`, `w ∈ Fp6`, `f·w == c^λ` in place of `Job::step3`; it is deployed
//! here at a named account of its own, and nowhere else. The instance is initialized and takes
//! update_0 through the copy's job path (unchanged); then mainnet update_1 as a finality update
//! is sent once with a tampered `c` (refused, state unchanged) and once with the honest witness
//! (applied; the instance's store equals the native store's, the record is written). For
//! comparison the same update goes through the deployed program's `VerifyAll`
//! (`scenarios::one_transaction_update`) in the same build.
//!
//! Behind the feature `residue-experimental` (no ordinary build of the harness builds the copy):
//! `RISC0_DEV_MODE=1 cargo test -p harness --features residue-experimental --test residue_experimental -- --nocapture`
#![cfg(feature = "residue-experimental")]

use std::borrow::Cow;

use ferrata_core::{
    fork::LcFork,
    policy::Adoption,
    steps::{self, Phase},
    types::{LightClientUpdate, SyncCommittee},
    validate::{sync_committee_signing_root, Context},
    witness::{CommitteeSummary, CompactStore, CompactUpdate, SignerWitness},
};
use ferrata_lez_wire::{header_actor, InstanceState, Message};
use ferrata_spectests::testkit::{fake_committee, sign};
use harness::{scenarios::World, *};
use lee::{program::Program, AccountId, Actor, V03State};
use lee_core::account::ActorState;
use residue_agg::{NonSigners, Sum};

/// Where the experimental copy is deployed in this test.
const EXPERIMENTAL: AccountId = AccountId::new([0x5e; 32]);

fn experimental_program() -> Program {
    Program::new_unchecked(
        methods_residue_experimental::FERRATA_LEZ_RESIDUE_EXPERIMENTAL_ID,
        Cow::Borrowed(methods_residue_experimental::FERRATA_LEZ_RESIDUE_EXPERIMENTAL_ELF),
    )
}

fn print_report(r: &TxReport) {
    let handlers: Vec<String> = r.transitions.iter().map(|t| format!("{} {}", t.handler, t.cycles)).collect();
    println!(
        "| {} | {} | {} | {:.1}% | {} | {} | {} |",
        r.label,
        handlers.join(" → "),
        r.charge.gas_exec,
        r.charge.gas_exec as f64 / 100_000.0,
        r.charge.gas_stor,
        r.charge.fee_base,
        match (&r.charge.revert, r.exit_code) {
            (None, _) => "applied".to_string(),
            (Some(_), Some(code)) => format!("refused: exit {code}"),
            (Some(e), None) => format!("reverted: {e}"),
        }
    );
}

#[test]
fn a_finality_update_in_one_transaction_with_the_residue_witness() {
    // A chain with the experimental copy deployed at its own account, beside the real programs.
    let mut chain = Chain::new();
    let state = std::mem::replace(&mut chain.state, V03State::new());
    chain.state = compat::named_programs(state, [(EXPERIMENTAL, experimental_program())]);

    // The instance, initialized and advanced by update_0 through the copy's (unchanged) job path.
    let config = mainnet_config(2);
    let checkpoint = root(TRUSTED_BLOCK_ROOT);
    let bootstrap = mainnet_bootstrap();
    let spec = config.spec();
    let inst = Instance::new(EXPERIMENTAL, config, checkpoint);
    let native = ferrata_core::policy::FinalizedStore::initialize_with(
        &checkpoint,
        &bootstrap,
        inst.fork(),
        &spec,
        &scenarios::BLS,
        Adoption::OnChain,
    )
    .unwrap();
    let run = inst.init(&mut chain, &bootstrap, "Init");
    assert!(run.reports.iter().all(TxReport::ok), "initialization refused: {:?}", run.reports);
    let mut w = World { chain, inst, native, spec, bootstrap, updates: mainnet_updates(), runs: vec![run] };
    let u0 = w.updates[0].clone();
    w.apply(&u0, "update_0");
    w.check_store();

    // update_1 as a finality update; its product and the honest witness, natively.
    let update = as_finality_update(&w.updates[1]);
    let store = w.inst.store(&w.chain);
    let signers = Instance::signer_witness(w.native.core(), &update, &w.spec);
    let gvr = w.inst.config.genesis_validators_root;
    let mut job =
        steps::step1(&store, &CompactUpdate::of(&update), &signers, update.signature_slot, &w.spec, &gvr, 1).unwrap();
    job.step2().unwrap();
    let Phase::FinalExp { product } = job.phase.clone() else { panic!("step 2 leaves the product") };
    let f = residue::decode_fp12(&product).unwrap();
    let (c, wit) = residue::witness(&f);
    assert_eq!(residue::check(&f, &c, &wit), Ok(()));
    let (c, wit) = (residue::encode_fp12(&c), residue::encode_fp12(&wit));

    let message = |c: &[u8], wit: &[u8]| {
        let mut s = signers.encode();
        s.extend_from_slice(c);
        s.extend_from_slice(wit);
        borsh::to_vec(&Message::VerifyAll {
            config: w.inst.config.clone(),
            checkpoint: w.inst.checkpoint,
            store: store.encode(),
            update: CompactUpdate::of(&update).encode(LcFork::Electra),
            signers: s,
            adoption: vec![],
        })
        .unwrap()
    };
    let verifier = Actor::new(w.chain.payer, EXPERIMENTAL);
    let header = header_actor(w.inst.program, &w.inst.id, update.finalized_header.beacon.slot);
    let time = w.inst.config.slot_start_ms(update.signature_slot) + 12_000;

    println!(
        "image ids in this build: experimental copy {}, deployed program {}",
        image_id_hex(methods_residue_experimental::FERRATA_LEZ_RESIDUE_EXPERIMENTAL_ID),
        image_id_hex(methods::FERRATA_LEZ_ID)
    );
    println!("| transaction | handlers run (cycles each) | execution gas | of MAX_GAS_EXEC 10M | storage gas (tx bytes) | base fee | result |");
    println!("|---|---|---:|---:|---:|---:|---|");

    // A tampered c: refused (SignatureInvalid, 40), nothing changes.
    let mut bad = c;
    bad[47] ^= 1;
    let msg = message(&bad, &wit);
    let refused = w.chain.submit(
        "EXPERIMENTAL VerifyAll, residue witness with c tampered",
        verifier,
        msg,
        vec![verifier, w.inst.actor(), header],
        vec![],
        time,
    );
    print_report(&refused);
    assert!(!refused.ok());
    assert_eq!(refused.exit_code, Some(40), "SignatureInvalid");
    w.check_store();

    // The honest witness: applied.
    let msg = message(&c, &wit);
    let report = w.chain.submit(
        "EXPERIMENTAL VerifyAll, residue witness (finality update_1, 510 of 512 signers)",
        verifier,
        msg,
        vec![verifier, w.inst.actor(), header],
        vec![],
        time,
    );
    print_report(&report);
    assert!(report.ok(), "{:?}", report.charge.revert);
    let spec = w.spec.clone();
    let ctx = Context { spec: &spec, genesis_validators_root: gvr, bls: &scenarios::BLS };
    w.native.process_update(&update, update.signature_slot + 1, &ctx).unwrap();
    w.check_store();
    w.check_record(update.finalized_header.beacon.slot);

    // The deployed program's VerifyAll on the same update, in the same build.
    for r in scenarios::one_transaction_update() {
        print_report(&r);
        println!(
            "difference in execution gas, deployed minus experimental: {}",
            r.charge.gas_exec as i64 - report.charge.gas_exec as i64
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The signing key from one multiproof (bench/residue-agg, bench/RESIDUE.md §9)
// ---------------------------------------------------------------------------------------------

/// The instance of the test above, initialized and advanced by update_0 through the copy's job path.
fn world_after_update_0() -> World {
    let mut chain = Chain::new();
    let state = std::mem::replace(&mut chain.state, V03State::new());
    chain.state = compat::named_programs(state, [(EXPERIMENTAL, experimental_program())]);
    let config = mainnet_config(2);
    let checkpoint = root(TRUSTED_BLOCK_ROOT);
    let bootstrap = mainnet_bootstrap();
    let spec = config.spec();
    let inst = Instance::new(EXPERIMENTAL, config, checkpoint);
    let native = ferrata_core::policy::FinalizedStore::initialize_with(
        &checkpoint,
        &bootstrap,
        inst.fork(),
        &spec,
        &scenarios::BLS,
        Adoption::OnChain,
    )
    .unwrap();
    let run = inst.init(&mut chain, &bootstrap, "Init");
    assert!(run.reports.iter().all(TxReport::ok), "initialization refused: {:?}", run.reports);
    let mut w = World { chain, inst, native, spec, bootstrap, updates: mainnet_updates(), runs: vec![run] };
    let u0 = w.updates[0].clone();
    w.apply(&u0, "update_0");
    w.check_store();
    w
}

/// `VerifyAll` at the copy with `signers` (a signer witness) followed by c and w.
fn verify_all_message(w: &World, update: &LightClientUpdate, signers: Vec<u8>, c: &[u8], wit: &[u8]) -> Vec<u8> {
    let mut s = signers;
    s.extend_from_slice(c);
    s.extend_from_slice(wit);
    borsh::to_vec(&Message::VerifyAll {
        config: w.inst.config.clone(),
        checkpoint: w.inst.checkpoint,
        store: w.inst.store(&w.chain).encode(),
        update: CompactUpdate::of(update).encode(LcFork::Electra),
        signers: s,
        adoption: vec![],
    })
    .unwrap()
}

/// The marked encoding of a `NonSigners` witness the copy reads (0xffff, then the witness): the
/// points summed by blst's batch addition.
fn marked(n: &NonSigners) -> Vec<u8> {
    let mut s = vec![0xff, 0xff];
    s.extend_from_slice(&n.encode());
    s
}

/// The same with the marker 0xfffe: one mixed addition per non-signer, as the core.
fn marked_chain(n: &NonSigners) -> Vec<u8> {
    let mut s = vec![0xfe, 0xff];
    s.extend_from_slice(&n.encode());
    s
}

/// The job of `update` on `store` with `signers` (`residue_agg::step1`, checked equal to the
/// core's `step1` with the core's witness), its honest residue witness, and the store it leads to.
fn native_job(
    w: &World,
    store: &CompactStore,
    update: &LightClientUpdate,
    committee: &SyncCommittee,
) -> (NonSigners, SignerWitness, [u8; residue::FP12_BYTES], [u8; residue::FP12_BYTES], CompactStore) {
    let gvr = w.inst.config.genesis_validators_root;
    let bits = &update.sync_aggregate.bits;
    let new = NonSigners::build(committee, bits).unwrap();
    let old = SignerWitness::build(committee, bits).unwrap();
    let cu = CompactUpdate::of(update);
    let mut job = residue_agg::step1(store, &cu, &new, update.signature_slot, &w.spec, &gvr, 1, Sum::Batch).unwrap();
    let core_job = steps::step1(store, &cu, &old, update.signature_slot, &w.spec, &gvr, 1).unwrap();
    assert_eq!(job, core_job, "residue_agg::step1 = the core's step1");
    job.step2().unwrap();
    let Phase::FinalExp { product } = job.phase.clone() else { panic!("step 2 leaves the product") };
    let f = residue::decode_fp12(&product).unwrap();
    let (c, wit) = residue::witness(&f);
    assert_eq!(residue::check(&f, &c, &wit), Ok(()));
    job.step3().expect("the signature verifies (final exponentiation, natively)");
    let (expected, _) = steps::apply(store, &job, &w.spec).unwrap();
    (new, old, residue::encode_fp12(&c), residue::encode_fp12(&wit), expected)
}

/// EXPERIMENTAL: mainnet update_1 as a finality update (510 of 512, the real committee) through
/// the copy's `VerifyAll` with the signer witness as one multiproof: applied, the store equal to
/// the native store's (compare the test above: the same update with a branch per non-signer).
#[test]
fn a_finality_update_in_one_transaction_with_one_multiproof() {
    let mut w = world_after_update_0();
    let update = as_finality_update(&w.updates[1]);
    let store = w.inst.store(&w.chain);
    let committee = signing_committee(w.native.core(), &update, &w.spec).clone();
    let (new, _, c, wit, expected) = native_job(&w, &store, &update, &committee);
    let verifier = Actor::new(w.chain.payer, EXPERIMENTAL);
    let header = header_actor(w.inst.program, &w.inst.id, update.finalized_header.beacon.slot);
    let time = w.inst.config.slot_start_ms(update.signature_slot) + 12_000;
    println!("| transaction | handlers run (cycles each) | execution gas | of MAX_GAS_EXEC 10M | storage gas (tx bytes) | base fee | result |");
    println!("|---|---|---:|---:|---:|---:|---|");
    let msg = verify_all_message(&w, &update, marked(&new), &c, &wit);
    let report = w.chain.submit(
        "EXPERIMENTAL VerifyAll, one multiproof + residue witness (finality update_1, 510 of 512 signers)",
        verifier,
        msg,
        vec![verifier, w.inst.actor(), header],
        vec![],
        time,
    );
    print_report(&report);
    assert!(report.ok(), "{:?}", report.charge.revert);
    let spec = w.spec.clone();
    let gvr = w.inst.config.genesis_validators_root;
    let ctx = Context { spec: &spec, genesis_validators_root: gvr, bls: &scenarios::BLS };
    w.native.process_update(&update, update.signature_slot + 1, &ctx).unwrap();
    w.check_store();
    assert_eq!(w.inst.store(&w.chain), expected);
    w.check_record(update.finalized_header.beacon.slot);
}

/// EXPERIMENTAL: the fewest signers the policy accepts (342 of 512) in one transaction. After
/// update_0, the instance's signing committee is replaced in its store by a committee whose keys
/// the test holds (its summary, written with LEZ's `force_insert_account`: the substitution
/// `residue-host measure` makes in the bench; the headers and branches stay real), and update_1 as
/// a finality update is signed by 342 of its members, every third absent (the placement that needs
/// the most hashes, 425, and the most witness nodes, 256; not the costliest placement: other
/// placements are in `residue_placements.rs`). Sent with the core's witness (a branch per
/// non-signer): over the cycle budget. With one node of the multiproof altered: refused. Honest:
/// applied, the store equal to the natively applied job's.
#[test]
fn a_342_signer_finality_update_in_one_transaction_with_one_multiproof() {
    let (mut w, fake, substituted) = world_with_the_bench_committee();
    let bits: Vec<bool> = (0..512).map(|i| i % 3 != 2).collect();
    let u = signed_by(&w, &fake, bits);
    assert_eq!(u.sync_aggregate.participants(), 342);
    let (new, old, c, wit, expected) = native_job(&w, &substituted, &u, &fake.committee);

    let verifier = Actor::new(w.chain.payer, EXPERIMENTAL);
    let header = header_actor(w.inst.program, &w.inst.id, u.finalized_header.beacon.slot);
    let time = w.inst.config.slot_start_ms(u.signature_slot) + 12_000;
    println!("| transaction | handlers run (cycles each) | execution gas | of MAX_GAS_EXEC 10M | storage gas (tx bytes) | base fee | result |");
    println!("|---|---|---:|---:|---:|---:|---|");

    // The core's witness (a branch per non-signer): over the limit.
    let msg = verify_all_message(&w, &u, old.encode(), &c, &wit);
    let over = w.chain.submit(
        "EXPERIMENTAL VerifyAll, a branch per non-signer + residue witness (342 of 512)",
        verifier,
        msg,
        vec![verifier, w.inst.actor(), header],
        vec![],
        time,
    );
    print_report(&over);
    assert!(!over.ok(), "a branch per non-signer does not fit at 342");
    assert_eq!(w.inst.store(&w.chain), substituted, "nothing changed");

    // One node of the multiproof altered: refused (the witness code, 80).
    let mut bad = new.clone();
    bad.siblings[100][0] ^= 1;
    let msg = verify_all_message(&w, &u, marked(&bad), &c, &wit);
    let refused = w.chain.submit(
        "EXPERIMENTAL VerifyAll, one multiproof with one node altered (342 of 512)",
        verifier,
        msg,
        vec![verifier, w.inst.actor(), header],
        vec![],
        time,
    );
    print_report(&refused);
    assert_eq!(refused.exit_code, Some(80), "StepError::Witness");
    assert_eq!(w.inst.store(&w.chain), substituted, "nothing changed");

    // Honest: applied.
    let msg = verify_all_message(&w, &u, marked(&new), &c, &wit);
    let report = w.chain.submit(
        "EXPERIMENTAL VerifyAll, one multiproof + residue witness (finality update_1, 342 of 512 signers)",
        verifier,
        msg,
        vec![verifier, w.inst.actor(), header],
        vec![],
        time,
    );
    print_report(&report);
    assert!(report.ok(), "{:?}", report.charge.revert);
    assert_eq!(w.inst.store(&w.chain), expected, "the store is the natively applied job's");
    assert!(w.inst.record(&w.chain, u.finalized_header.beacon.slot).is_some(), "the record is written");
    println!(
        "342 of 512 in one transaction: {} execution gas ({:.1}% of MAX_GAS_EXEC); the signer witness {} bytes \
         (a branch per non-signer: {} bytes)",
        report.charge.gas_exec,
        report.charge.gas_exec as f64 / 100_000.0,
        marked(&new).len(),
        old.encode().len()
    );
}

/// [`world_after_update_0`], then the store's signing committee for update_1 replaced by a
/// committee whose keys the test holds (its summary), written with LEZ's `force_insert_account`.
fn world_with_the_bench_committee() -> (World, ferrata_spectests::testkit::FakeCommittee, CompactStore) {
    let mut w = world_after_update_0();
    let update = as_finality_update(&w.updates[1]);
    let store = w.inst.store(&w.chain);
    let real = signing_committee(w.native.core(), &update, &w.spec).clone();
    let fake = fake_committee(512, 7);
    let mut substituted = store.clone();
    if substituted.current == CommitteeSummary::of(&real) {
        substituted.current = CommitteeSummary::of(&fake.committee);
    } else {
        assert_eq!(substituted.next, Some(CommitteeSummary::of(&real)));
        substituted.next = Some(CommitteeSummary::of(&fake.committee));
    }
    let actor = w.inst.actor();
    let mut account = w.chain.state.get_account_by_id(actor.account_id);
    let mut state: InstanceState = borsh::from_slice(account.data.actor_state(actor.program_account_id)).unwrap();
    state.store = substituted.encode();
    account.data.set_actor_state(actor.program_account_id, ActorState::from(borsh::to_vec(&state).unwrap()));
    w.chain.state.force_insert_account(actor.account_id, account);
    assert_eq!(w.inst.store(&w.chain), substituted);
    (w, fake, substituted)
}

/// update_1 as a finality update, signed by the members of `fake` whose bit is set.
fn signed_by(w: &World, fake: &ferrata_spectests::testkit::FakeCommittee, bits: Vec<bool>) -> LightClientUpdate {
    let update = as_finality_update(&w.updates[1]);
    let gvr = w.inst.config.genesis_validators_root;
    let ctx = Context { spec: &w.spec, genesis_validators_root: gvr, bls: &scenarios::BLS };
    let mut u = update.clone();
    u.sync_aggregate.signature = sign(fake, &bits, &sync_committee_signing_root(&update, &ctx));
    u.sync_aggregate.bits = bits;
    u
}

/// `absent` members absent, evenly spaced (with one multiproof, the most hashes that many need).
fn spread(absent: usize) -> Vec<bool> {
    let mut bits = vec![true; 512];
    for j in 0..absent {
        bits[j * 512 / absent] = false;
    }
    bits
}

/// EXPERIMENTAL: the 342-signer update of the test above, the non-signers' points summed by one
/// mixed addition each (the core's chain, exact for any points on the curve) instead of blst's
/// batch addition: applied, the store equal to the natively applied job's.
#[test]
fn a_342_signer_finality_update_with_one_multiproof_and_the_chain_of_additions() {
    let (mut w, fake, substituted) = world_with_the_bench_committee();
    let bits: Vec<bool> = (0..512).map(|i| i % 3 != 2).collect();
    let u = signed_by(&w, &fake, bits);
    let (new, _, c, wit, expected) = native_job(&w, &substituted, &u, &fake.committee);
    let verifier = Actor::new(w.chain.payer, EXPERIMENTAL);
    let header = header_actor(w.inst.program, &w.inst.id, u.finalized_header.beacon.slot);
    let time = w.inst.config.slot_start_ms(u.signature_slot) + 12_000;
    println!("| transaction | handlers run (cycles each) | execution gas | of MAX_GAS_EXEC 10M | storage gas (tx bytes) | base fee | result |");
    println!("|---|---|---:|---:|---:|---:|---|");
    let msg = verify_all_message(&w, &u, marked_chain(&new), &c, &wit);
    let report = w.chain.submit(
        "EXPERIMENTAL VerifyAll, one multiproof (chain of additions) + residue witness (finality update_1, 342 of 512 signers)",
        verifier,
        msg,
        vec![verifier, w.inst.actor(), header],
        vec![],
        time,
    );
    print_report(&report);
    assert!(report.ok(), "{:?}", report.charge.revert);
    assert_eq!(w.inst.store(&w.chain), expected, "the store is the natively applied job's");
    println!(
        "342 of 512 in one transaction, chain of additions: {} execution gas ({:.1}% of MAX_GAS_EXEC)",
        report.charge.gas_exec,
        report.charge.gas_exec as f64 / 100_000.0
    );
}

/// EXPERIMENTAL: the fewest signers whose finality update fits one transaction through the copy
/// with a branch per non-signer (the core's witness) and the residue witness: from 380 signers
/// up, each sent once, until one is applied (the bench committee, as above).
#[test]
fn the_fewest_signers_that_fit_with_a_branch_per_non_signer() {
    let (mut w, fake, substituted) = world_with_the_bench_committee();
    let verifier = Actor::new(w.chain.payer, EXPERIMENTAL);
    println!("| transaction | handlers run (cycles each) | execution gas | of MAX_GAS_EXEC 10M | storage gas (tx bytes) | base fee | result |");
    println!("|---|---|---:|---:|---:|---:|---|");
    let mut fewest = None;
    for signers in 380..=400usize {
        let u = signed_by(&w, &fake, spread(512 - signers));
        let (_, old, c, wit, expected) = native_job(&w, &substituted, &u, &fake.committee);
        let header = header_actor(w.inst.program, &w.inst.id, u.finalized_header.beacon.slot);
        let time = w.inst.config.slot_start_ms(u.signature_slot) + 12_000;
        let msg = verify_all_message(&w, &u, old.encode(), &c, &wit);
        let report = w.chain.submit(
            &format!("EXPERIMENTAL VerifyAll, a branch per non-signer + residue witness ({signers} of 512)"),
            verifier,
            msg,
            vec![verifier, w.inst.actor(), header],
            vec![],
            time,
        );
        print_report(&report);
        if report.ok() {
            assert_eq!(w.inst.store(&w.chain), expected);
            fewest = Some(signers);
            break;
        }
        assert_eq!(report.exit_code, None, "over the cycle budget, not refused by a check");
        assert_eq!(w.inst.store(&w.chain), substituted, "nothing changed");
    }
    println!("fewest signers that fit one transaction with a branch per non-signer: {fewest:?}");
    assert!(fewest.is_some());
}

fn signing_committee<'a>(
    full: &'a ferrata_core::store::StoreCore,
    update: &LightClientUpdate,
    spec: &ferrata_core::fork::ChainSpec,
) -> &'a SyncCommittee {
    if spec.period_at_slot(update.signature_slot) == spec.period_at_slot(full.finalized_header.beacon.slot) {
        &full.current_sync_committee
    } else {
        &full.next_sync_committee
    }
}
