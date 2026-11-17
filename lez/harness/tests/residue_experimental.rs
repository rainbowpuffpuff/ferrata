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
//! `RISC0_DEV_MODE=1 cargo test -p harness --test residue_experimental -- --nocapture`

use std::borrow::Cow;

use ferrata_core::{
    fork::LcFork,
    policy::Adoption,
    steps::{self, Phase},
    validate::Context,
    witness::CompactUpdate,
};
use ferrata_lez_wire::{header_actor, Message};
use harness::{scenarios::World, *};
use lee::{program::Program, AccountId, Actor, V03State};

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
    chain.state = state.with_named_programs([(EXPERIMENTAL, experimental_program())]);

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
