//! The LEZ v0.3.1 program through LEZ v0.3.1's own block application (`chain_state::apply`), with
//! real mainnet and Sepolia light-client data. EXPERIMENTAL, UNAUDITED (`../../README.md`).
//!
//! `cargo test --release -p harness-v03 --test v03 -- --nocapture --test-threads 1`; every
//! transaction is written to `lez-v03/results/<test>.tsv` (gas against `MAX_GAS_EXEC`, fees).

use ferrata_core::{
    fork::LcFork,
    steps::Phase,
    witness::{CompactUpdate, HeaderRecord, SignerWitness},
    Error,
};
use ferrata_v03_wire::{code, Instruction, Step, Verify, Window};
use harness_v03::*;

fn residue() -> Verify {
    Verify::Residue { c: vec![], w: vec![] }
}

fn core_code(e: Error) -> u8 {
    e.code() as u8
}

/// A chain with the program deployed through LEZ's program loader.
fn chain_with_program() -> (Chain, lee::AccountId) {
    let mut chain = Chain::new();
    let program = chain.deploy("ferrata-v03", FERRATA_V03_ELF);
    println!("program {program} image {} ({} ELF bytes)", image_id_hex(FERRATA_V03_ID), FERRATA_V03_ELF.len());
    (chain, program)
}

/// `Init` with every adoption part in the one transaction.
fn init(chain: &mut Chain, inst: &Instance, bootstrap: &ferrata_core::types::LightClientBootstrap) -> TxReport {
    let (p, rest) = inst.init(bootstrap, usize::MAX);
    assert!(rest.is_empty());
    let r = inst.send(chain, &p, GAS_LIMIT);
    assert!(r.ok(), "Init: {r:?}");
    let state = inst.state(chain).unwrap();
    assert!(state.active && state.job.is_none() && state.applied == 1);
    assert!(inst.record(chain, bootstrap.header.beacon.slot).is_some(), "the checkpoint's record");
    r
}

/// A period update (it adopts the next committee): `Update` (residue witness, step 1 to the
/// verified signature), then `Continue` with the adoption, which applies it. Two transactions.
fn period_update_residue(chain: &mut Chain, inst: &mut Instance, u: &ferrata_core::types::LightClientUpdate, label: &str) -> Vec<TxReport> {
    let (p, rest, record) = inst.update(&format!("{label}: Update (residue witness)"), u, residue(), Signers::Core, false);
    let r1 = inst.send(chain, &p, GAS_LIMIT);
    assert!(r1.ok(), "{label}: {r1:?}");
    let steps: Vec<Step> = rest.into_iter().map(Step::Adopt).collect();
    let c = inst.continue_with(&format!("{label}: Continue (adopt {} keys, apply)", steps.len() * 512 / inst.config.adoption_parts as usize), steps, record.as_ref(), p.timestamp_ms + 12_000);
    let r2 = inst.send(chain, &c, GAS_LIMIT);
    assert!(r2.ok(), "{label}: {r2:?}");
    inst.follow(u);
    inst.assert_in_step(chain);
    vec![r1, r2]
}

#[test]
fn mainnet_one_transaction_finality_update_and_handoffs() {
    let (mut chain, program) = chain_with_program();
    let bootstrap = mainnet_bootstrap();
    let updates = mainnet_updates();
    let mut inst = Instance::new(program, mainnet_config(1), root(MAINNET_CHECKPOINT), &bootstrap);
    init(&mut chain, &inst, &bootstrap);

    // update_0: a handoff (adopts the next committee), in two transactions.
    period_update_residue(&mut chain, &mut inst, &updates[0], "update_0 (period)");

    // update_1 as a finality update (510 of 512 signers): ONE transaction, residue witness.
    let u1 = as_finality_update(&updates[1]);
    assert_eq!(u1.sync_aggregate.participants(), 510);
    let (p, rest, record) = inst.update("update_1 (finality, 510/512): Update (residue witness)", &u1, residue(), Signers::Core, true);
    assert!(rest.is_empty() && record.is_some());
    let r = inst.send(&mut chain, &p, GAS_LIMIT);
    assert!(r.ok(), "{r:?}");
    assert!(r.exec_gas < MAX_GAS_EXEC);
    inst.follow(&u1);
    inst.assert_in_step(&chain);
    println!("ONE-TX finality update (mainnet update_1, 510/512, residue): {} execution gas ({:.1}% of MAX_GAS_EXEC)", r.exec_gas, r.exec_gas as f64 / 1e5);

    // Refusals.
    // A replayed Init: the instance exists.
    let (mut pi, _) = inst.init(&bootstrap, usize::MAX);
    pi.label = "refusal: Init replayed".into();
    let r = inst.send(&mut chain, &pi, GAS_LIMIT);
    assert!(r.refused_with(code::ALREADY_INITIALIZED), "{r:?}");
    // A stale update: update_1 again, with the window of the current store: not newer.
    let mut stale = p.clone();
    if let Instruction::Update { window, .. } = &mut stale.instruction {
        let finalized = inst.native.core().finalized_header.beacon.slot;
        window.to_ms = inst.config.ws_end_slot(finalized).map(|e| inst.config.slot_start_ms(e));
    }
    stale.label = "refusal: update_1 again (stale)".into();
    let r = inst.send(&mut chain, &stale, GAS_LIMIT);
    assert!(r.refused_with(core_code(Error::NotNewer)), "{r:?}");
    // The same update replayed as sent (its window was the old store's): refused too.
    let mut replay = p.clone();
    replay.label = "refusal: update_1 replayed as sent".into();
    let r = inst.send(&mut chain, &replay, GAS_LIMIT);
    assert!(r.refused_with(code::WRONG_WINDOW), "{r:?}");
    inst.assert_in_step(&chain);
    write_results("mainnet-residue", &chain.reports);
}

#[test]
fn mainnet_every_verification_path() {
    let (mut chain, program) = chain_with_program();
    let bootstrap = mainnet_bootstrap();
    let updates = mainnet_updates();
    let mut inst = Instance::new(program, mainnet_config(1), root(MAINNET_CHECKPOINT), &bootstrap);
    init(&mut chain, &inst, &bootstrap);
    period_update_residue(&mut chain, &mut inst, &updates[0], "update_0 (period)");

    // update_1, a handoff, by the core alone, step by step: Open (step 1), Miller (step 2),
    // FinalExp (step 3), then the adoption, which applies it.
    let (p, rest, record) = inst.update("update_1 (period): Update Open (step 1, core)", &updates[1], Verify::Open, Signers::Core, false);
    let r = inst.send(&mut chain, &p, GAS_LIMIT);
    assert!(r.ok(), "{r:?}");
    let t = p.timestamp_ms;
    for (label, s) in [("Continue Miller (step 2)", Step::Miller), ("Continue FinalExp (step 3, the core's final exponentiation)", Step::FinalExp)] {
        let c = inst.continue_with(&format!("update_1 (period): {label}"), vec![s], None, t + 12_000);
        let r = inst.send(&mut chain, &c, GAS_LIMIT);
        assert!(r.ok(), "{r:?}");
    }
    let job = inst.state(&chain).unwrap().job.expect("a job waiting for its adoption");
    assert_eq!(ferrata_core::steps::Job::decode(&job).unwrap().phase, Phase::Verified);
    // While the verified job waits, another update is refused (anyone can finish it instead).
    let mut again = p.clone();
    again.label = "refusal: another Update while update_1 waits for its adoption".into();
    let r = inst.send(&mut chain, &again, GAS_LIMIT);
    assert!(r.refused_with(code::JOB_PENDING), "{r:?}");
    let steps: Vec<Step> = rest.into_iter().map(Step::Adopt).collect();
    let c = inst.continue_with("update_1 (period): Continue (adopt 512 keys, apply)", steps, record.as_ref(), t + 24_000);
    let r = inst.send(&mut chain, &c, GAS_LIMIT);
    assert!(r.ok(), "{r:?}");
    inst.follow(&updates[1]);
    inst.assert_in_step(&chain);

    // update_2, a handoff: steps 1-3 by the core in one transaction (if it fits), then the adoption.
    let (p, rest, record) = inst.update("update_2 (period): Update Core (steps 1-3, the core alone)", &updates[2], Verify::Core, Signers::Core, false);
    let r = inst.send(&mut chain, &p, GAS_LIMIT);
    println!("core steps 1-3 in one transaction: {} ({})", r.status, r.exec_gas);
    if r.ok() {
        let c = inst.continue_with("update_2 (period): Continue (adopt 512 keys, apply)", rest.into_iter().map(Step::Adopt).collect(), record.as_ref(), p.timestamp_ms + 12_000);
        let r = inst.send(&mut chain, &c, GAS_LIMIT);
        assert!(r.ok(), "{r:?}");
    } else {
        // Over the cap: the core's path in steps instead.
        let (p, rest, record) = inst.update("update_2 (period): Update Miller (steps 1-2, core)", &updates[2], Verify::Miller, Signers::Core, false);
        assert!(inst.send(&mut chain, &p, GAS_LIMIT).ok());
        let mut steps = vec![Step::FinalExp];
        steps.extend(rest.into_iter().map(Step::Adopt));
        let c = inst.continue_with("update_2 (period): Continue FinalExp + adopt", steps, record.as_ref(), p.timestamp_ms + 12_000);
        assert!(inst.send(&mut chain, &c, GAS_LIMIT).ok());
    }
    inst.follow(&updates[2]);
    inst.assert_in_step(&chain);

    // update_3, a handoff with the EXPERIMENTAL multiproof of the non-signers and the residue witness.
    let (p, rest, record) = inst.update("update_3 (period): Update (multiproof + residue witness)", &updates[3], residue(), Signers::Multiproof, false);
    let r = inst.send(&mut chain, &p, GAS_LIMIT);
    assert!(r.ok(), "{r:?}");
    let c = inst.continue_with("update_3 (period): Continue (adopt 512 keys, apply)", rest.into_iter().map(Step::Adopt).collect(), record.as_ref(), p.timestamp_ms + 12_000);
    assert!(inst.send(&mut chain, &c, GAS_LIMIT).ok());
    inst.follow(&updates[3]);
    inst.assert_in_step(&chain);

    // update_4 as a finality update by the core alone, steps 1-3 in one transaction.
    let u4 = as_finality_update(&updates[4]);
    let (p, rest, _) = inst.update("update_4 (finality): Update Core (steps 1-3, the core alone)", &u4, Verify::Core, Signers::Core, true);
    assert!(rest.is_empty());
    let r = inst.send(&mut chain, &p, GAS_LIMIT);
    println!("finality update by the core alone in one transaction: {} {} gas", r.status, r.exec_gas);
    if r.ok() {
        inst.follow(&u4);
        inst.assert_in_step(&chain);
    }
    write_results("mainnet-paths", &chain.reports);
}

#[test]
fn mainnet_tampered_updates_are_refused() {
    let (mut chain, program) = chain_with_program();
    let bootstrap = mainnet_bootstrap();
    let updates = mainnet_updates();
    let mut inst = Instance::new(program, mainnet_config(1), root(MAINNET_CHECKPOINT), &bootstrap);
    init(&mut chain, &inst, &bootstrap);
    period_update_residue(&mut chain, &mut inst, &updates[0], "update_0 (period)");
    let u1 = as_finality_update(&updates[1]);
    let layout = inst.layout_for(inst.native.core().fork, &u1);
    let (good, _, _) = inst.update("good", &u1, residue(), Signers::Core, true);

    // A participation bit cleared (one signer fewer claimed), the witness rebuilt for it: the
    // signature no longer verifies.
    let mut bad = u1.clone();
    let i = bad.sync_aggregate.bits.iter().position(|b| *b).unwrap();
    bad.sync_aggregate.bits[i] = false;
    let committee = inst.signing_committee(inst.native.core(), &bad).clone();
    let mut p = good.clone();
    p.label = "refusal: a participation bit cleared".into();
    if let Instruction::Update { update, signers, .. } = &mut p.instruction {
        *update = CompactUpdate::of(&bad).encode(layout);
        *signers = SignerWitness::build(&committee, &bad.sync_aggregate.bits).unwrap().encode();
    }
    let r = inst.send(&mut chain, &p, GAS_LIMIT);
    assert!(r.refused_with(core_code(Error::SignatureInvalid)), "{r:?}");

    // The finalized header's state root changed: its finality branch fails.
    let mut bad = u1.clone();
    bad.finalized_header.beacon.state_root[0] ^= 1;
    let mut p = good.clone();
    p.label = "refusal: the finalized header's state root changed".into();
    if let Instruction::Update { update, .. } = &mut p.instruction {
        *update = CompactUpdate::of(&bad).encode(layout);
    }
    let r = inst.send(&mut chain, &p, GAS_LIMIT);
    assert!(r.status == "refused", "{r:?}");

    // A residue witness for another product: refused.
    let mut p = good.clone();
    p.label = "refusal: a tampered residue witness".into();
    if let Instruction::Update { verify: Verify::Residue { w, .. }, .. } = &mut p.instruction {
        w[0] ^= 1;
    }
    let r = inst.send(&mut chain, &p, GAS_LIMIT);
    assert!(r.status == "refused", "{r:?}");

    // A claimed record that is not the update's: refused.
    let mut p = good.clone();
    p.label = "refusal: a wrong claimed record".into();
    if let Instruction::Update { record: Some(claim), .. } = &mut p.instruction {
        claim.chain_id += 1;
    }
    let r = inst.send(&mut chain, &p, GAS_LIMIT);
    assert!(r.refused_with(code::WRONG_RECORD), "{r:?}");

    // A claimed window that is not the program's: refused.
    let mut p = good.clone();
    p.label = "refusal: a widened timestamp window".into();
    if let Instruction::Update { window, .. } = &mut p.instruction {
        *window = Window { from_ms: window.from_ms, to_ms: None };
    }
    let r = inst.send(&mut chain, &p, GAS_LIMIT);
    assert!(r.refused_with(code::WRONG_WINDOW), "{r:?}");

    // Sent before its signature slot by the sequencer's clock: outside its window, reverted.
    let mut p = good.clone();
    p.label = "refusal: sent before the signature slot".into();
    p.timestamp_ms = inst.config.slot_start_ms(u1.signature_slot) - 60_000;
    chain.last_timestamp = chain.last_timestamp.min(p.timestamp_ms - 1);
    let r = inst.send(&mut chain, &p, GAS_LIMIT);
    assert!(r.status != "applied", "{r:?}");

    // Nothing of these changed the instance; the honest update still applies.
    inst.assert_in_step(&chain);
    let mut ok = good.clone();
    ok.label = "update_1 (finality): the honest transaction after the refusals".into();
    let r = inst.send(&mut chain, &ok, GAS_LIMIT);
    assert!(r.ok(), "{r:?}");
    inst.follow(&u1);
    inst.assert_in_step(&chain);
    write_results("mainnet-refusals", &chain.reports);
}

#[test]
fn sepolia_through_gloas_finality_updates_and_attestations() {
    let (mut chain, program) = chain_with_program();
    let bootstrap = sepolia_bootstrap();
    let mut inst = Instance::new(program, sepolia_config(1), root(SEPOLIA_CHECKPOINT), &bootstrap);
    init(&mut chain, &inst, &bootstrap);

    // Every period's update from 1378 to 1409: 32 handoffs, the store upgrading itself to the
    // Gloas layout with the first update attested under Gloas (period 1379).
    for period in SEPOLIA_FIRST_PERIOD..=SEPOLIA_LAST_PERIOD {
        let u = sepolia_update(period);
        period_update_residue(&mut chain, &mut inst, &u, &format!("period {period}"));
        let expected = if period < SEPOLIA_GLOAS_PERIOD { LcFork::Electra } else { LcFork::Gloas };
        assert_eq!(inst.store(&chain).fork, expected, "period {period}: the layout");
    }

    // Two finality updates of period 1410, ONE transaction each (residue witness), and an
    // attestation of a vault's deposit accumulator under each finalized header (Gloas records:
    // the block's RLP header travels with the proofs).
    for (dir, vault, label) in [
        ("sepolia-inbound-2026-11-11-1", "0xd0b5fcc0b47d8e6f514f9d246bb0b416b2048af7", "inbound vault"),
        ("sepolia-vault-v5-2026-11-11", "0xfe3199fa2634413cbac317371076cae3eb2d9e54", "vault v5"),
    ] {
        let u = gloas_finality_update(&fixture(dir, "finality_update.ssz"));
        let (p, rest, record) =
            inst.update(&format!("{label}: finality update ({}/512, slot {}): Update (residue witness)", u.sync_aggregate.participants(), u.finalized_header.beacon.slot), &u, residue(), Signers::Core, true);
        assert!(rest.is_empty() && record.is_some());
        let r = inst.send(&mut chain, &p, GAS_LIMIT);
        assert!(r.ok(), "{r:?}");
        inst.follow(&u);
        inst.assert_in_step(&chain);
        println!("ONE-TX Sepolia finality update: {} execution gas", r.exec_gas);

        let slot = u.finalized_header.beacon.slot;
        let rec = inst.record(&chain, slot).unwrap();
        let block = match HeaderRecord::decode(&rec.record).unwrap().execution {
            ferrata_core::witness::ExecutionSummary::BlockHash(_) => (),
            other => panic!("a Gloas record binds a block hash: {other:?}"),
        };
        let _ = block;
        let proof_name = std::fs::read_dir(fixture_dir(dir))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .find(|n| n.starts_with(&format!("proof_{vault}_")))
            .unwrap();
        // Slots 0 and 1: depositRoot and nextLeafIndex, as the LEZ v0.4 bridge's ImportRoot reads them.
        let query = storage_query(&fixture(dir, &proof_name), SEPOLIA_CHAIN_ID, Some(fixture(dir, "header.rlp")), &[0, 1]);
        let statements = inst.statements(&rec, &query);
        let a = inst.attest(&format!("{label}: Attest slots 0 (depositRoot) and 1 (nextLeafIndex)"), slot, query.clone(), statements.clone(), chain.last_timestamp + 1000);
        let r = inst.send(&mut chain, &a, GAS_LIMIT);
        assert!(r.ok(), "{r:?}");
        for s in &statements {
            assert_eq!(inst.statement(&chain, s).as_ref(), Some(s), "the statement account holds it");
            println!("attested {label} slot {} = {} at block {} (statement {})", hex::encode(s.slot), hex::encode(s.value), s.block_number, hex::encode(s.id));
        }

        // Refusals of the attestation.
        let mut wrong = statements.clone();
        wrong[1].value[31] ^= 1;
        let a2 = inst.attest(&format!("refusal: {label}: a wrong claimed value"), slot, query.clone(), wrong, chain.last_timestamp + 1000);
        let mut a2 = a2;
        a2.accounts = inst.attest("", slot, query.clone(), statements.clone(), 0).accounts;
        let r = inst.send(&mut chain, &a2, GAS_LIMIT);
        assert!(r.status == "refused", "{r:?}");
        let mut bad = query.clone();
        let last = bad.slots[0].proof.len() - 1;
        bad.slots[0].proof[last][5] ^= 1;
        let a3 = inst.attest(&format!("refusal: {label}: a storage proof node changed"), slot, bad, statements.clone(), chain.last_timestamp + 1000);
        let r = inst.send(&mut chain, &a3, GAS_LIMIT);
        assert!(r.status == "refused", "{r:?}");
        let mut no_header = query.clone();
        no_header.header = None;
        let a4 = inst.attest(&format!("refusal: {label}: no block header for a Gloas record"), slot, no_header, statements.clone(), chain.last_timestamp + 1000);
        let r = inst.send(&mut chain, &a4, GAS_LIMIT);
        assert!(r.refused_with(code::HEADER_REQUIRED), "{r:?}");
    }
    let summary: Vec<_> = chain.reports.iter().filter(|r| r.ok()).map(|r| r.exec_gas).collect();
    println!("Sepolia: {} transactions applied, the largest {} gas", summary.len(), summary.iter().max().unwrap());
    write_results("sepolia", &chain.reports);
}

/// EXPERIMENTAL (`bench/RESIDUE.md` §10; sound only under the light client's honest-supermajority
/// assumption): every handoff in ONE transaction, the next committee adopted by its summary
/// (`Verify::ResidueSummary`), with the residue witness and one multiproof of the non-signers:
/// mainnet update_0 to update_3 (each a period update) and Sepolia's 32 handoffs through the Gloas
/// layout change. After each, the instance's store is the native policy store's (whose adoption
/// checks the 512 keys) and the record is written. Refused: a next-committee branch with one node
/// altered (27), adoption parts sent beside the summary adoption (`NoAdoption`, 84), a stale
/// handoff. Writes `results/mainnet-handoff-one-tx.tsv` and `results/sepolia-handoff-one-tx.tsv`.
#[test]
fn handoffs_in_one_transaction_each() {
    let summary = || Verify::ResidueSummary { c: vec![], w: vec![] };

    // Mainnet.
    let (mut chain, program) = chain_with_program();
    let bootstrap = mainnet_bootstrap();
    let updates = mainnet_updates();
    let mut inst = Instance::new(program, mainnet_config(1), root(MAINNET_CHECKPOINT), &bootstrap);
    init(&mut chain, &inst, &bootstrap);
    let mut first = None;
    for (i, u) in updates.iter().enumerate().take(4) {
        let label = format!(
            "update_{i} (period, {}/512): Update (one multiproof, residue witness, committee adopted by its summary)",
            u.sync_aggregate.participants()
        );
        let (p, rest, record) = inst.update(&label, u, summary(), Signers::Multiproof, false);
        assert!(rest.is_empty(), "{label}: one transaction");
        // A record is written when the update advances the finalized header.
        println!("{label}: writes a record: {}", record.is_some());
        if i == 0 {
            let mut tampered = u.clone();
            tampered.next_sync_committee_branch[1][4] ^= 1;
            let mut bad = p.clone();
            if let Instruction::Update { update, .. } = &mut bad.instruction {
                *update = CompactUpdate::of(&tampered).encode(LcFork::Electra);
            }
            bad.label = "refusal: update_0 with one node of its next-committee branch altered".into();
            let r = inst.send(&mut chain, &bad, GAS_LIMIT);
            assert!(r.refused_with(core_code(Error::NextCommitteeBranchInvalid)), "{r:?}");
            let mut both = p.clone();
            if let Instruction::Update { adopt, .. } = &mut both.instruction {
                *adopt = inst.parts(&u.next_sync_committee);
            }
            both.label = "refusal: update_0 with the summary adoption and the 512 keys".into();
            let r = inst.send(&mut chain, &both, GAS_LIMIT);
            assert!(r.refused_with(84), "NoAdoption: {r:?}");
        }
        let r = inst.send(&mut chain, &p, GAS_LIMIT);
        assert!(r.ok(), "{label}: {r:?}");
        assert!(r.exec_gas < MAX_GAS_EXEC);
        assert!(inst.follow(u).adopted, "{label}: a handoff");
        inst.assert_in_step(&chain);
        println!(
            "ONE-TX handoff (mainnet update_{i}, {}/512): {} execution gas ({:.1}% of MAX_GAS_EXEC)",
            u.sync_aggregate.participants(),
            r.exec_gas,
            r.exec_gas as f64 / 1e5
        );
        first.get_or_insert(p);
    }
    let mut stale = first.unwrap();
    if let Instruction::Update { window, .. } = &mut stale.instruction {
        let finalized = inst.native.core().finalized_header.beacon.slot;
        window.to_ms = inst.config.ws_end_slot(finalized).map(|e| inst.config.slot_start_ms(e));
    }
    stale.label = "refusal: update_0 again (stale)".into();
    let r = inst.send(&mut chain, &stale, GAS_LIMIT);
    assert!([23u8, 24, 52].iter().any(|c| r.refused_with(*c)), "stale: {r:?}");
    inst.assert_in_step(&chain);
    write_results("mainnet-handoff-one-tx", &chain.reports);

    // Sepolia: periods 1378 to 1409, the store upgrading itself to the Gloas layout at 1379.
    let (mut chain, program) = chain_with_program();
    let bootstrap = sepolia_bootstrap();
    let mut inst = Instance::new(program, sepolia_config(1), root(SEPOLIA_CHECKPOINT), &bootstrap);
    init(&mut chain, &inst, &bootstrap);
    let (mut most, mut handoffs) = (0u64, 0usize);
    for period in SEPOLIA_FIRST_PERIOD..=SEPOLIA_LAST_PERIOD {
        let u = sepolia_update(period);
        let label = format!(
            "period {period} ({}/512): Update (one multiproof, residue witness, committee adopted by its summary)",
            u.sync_aggregate.participants()
        );
        let (p, rest, _) = inst.update(&label, &u, summary(), Signers::Multiproof, false);
        assert!(rest.is_empty(), "{label}: one transaction");
        let r = inst.send(&mut chain, &p, GAS_LIMIT);
        assert!(r.ok(), "{label}: {r:?}");
        most = most.max(r.exec_gas);
        handoffs += usize::from(inst.follow(&u).adopted);
        inst.assert_in_step(&chain);
        let expected = if period < SEPOLIA_GLOAS_PERIOD { LcFork::Electra } else { LcFork::Gloas };
        assert_eq!(inst.store(&chain).fork, expected, "period {period}: the layout");
    }
    assert_eq!(handoffs, (SEPOLIA_LAST_PERIOD - SEPOLIA_FIRST_PERIOD + 1) as usize, "every period update is a handoff");
    println!(
        "ONE-TX handoffs on Sepolia: {handoffs} periods, one transaction each; the most execution gas {most} ({:.1}% of MAX_GAS_EXEC)",
        most as f64 / 1e5
    );
    write_results("sepolia-handoff-one-tx", &chain.reports);
}
