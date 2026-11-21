//! RFP-021 R.7: an instance left unadvanced does not lose safety. Attestations against headers it has
//! already verified keep verifying while it is stalled, and when any party submits the outstanding updates
//! it resumes, with no manual repair and no gap in the verified handoff chain. Also an old record read after
//! later committee rotations (R.3).
//!
//! Real Sepolia data throughout, in LEZ v0.4.0-rc1's state machine (the harness, RISC Zero dev mode):
//! the instance follows Sepolia from the period-1378 bootstrap to the attestation fixture's finality update
//! (`sepolia::Stage5`: record `R` at slot 11,549,664, block 12,105,936, 10 Nov 2026). Then nobody advances it.
//! The outstanding updates are the ones the lab recorded from Sepolia's beacon nodes afterwards
//! (`lez/lab/results/record/net/sepolia/updates/`, to period 1417, 18 Nov 2026): about eight days of
//! Ethereum, eight sync-committee handoffs. They are submitted late, all at once, at one block time after the
//! newest of them was signed, as a party that comes back to a stalled instance would submit them.
//!
//! `FERRATA_STALL_FIXTURES=DIR` writes the instance's state and records at the stall and after the resume, for
//! the attestation side's `monitor` tests (`monitor/tests/fixtures/attest/`).

use std::{fs, path::PathBuf};

use ferrata_core::{
    fork::LcFork,
    steps,
    types::LightClientUpdate,
    witness::{uncompressed_keys, CompactUpdate},
};
use ferrata_lez_wire::{
    attest::{Attestation, Cmp, Field, LogQuery, Request},
    consumer::Pool,
    header_actor, job_actor, job_id, Message,
};
use harness::{
    attest::{account_query, condition},
    plan_job,
    scenarios::World,
    sepolia::{self, Stage5},
    Instance, JobRun, TxReport,
};

/// One update the lab recorded, in its order.
struct Entry {
    name: String,
    period_update: bool,
    update: LightClientUpdate,
}

fn lab_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../lab/results/record/net/sepolia")
}

/// Every update of the lab's Sepolia record (`NNNNN.{period,finality}.{electra,gloas}.ssz`), in order.
fn lab_entries() -> Vec<Entry> {
    let dir = lab_dir().join("updates");
    let mut names: Vec<String> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.ends_with(".ssz"))
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let parts: Vec<&str> = name.split('.').collect();
            let layout = match parts[2] {
                "electra" => LcFork::Electra,
                "gloas" => LcFork::Gloas,
                other => panic!("{name}: layout {other}"),
            };
            let bytes = fs::read(dir.join(&name)).unwrap();
            let period_update = parts[1] == "period";
            let update = if period_update {
                LightClientUpdate::decode(&bytes, layout, 512).unwrap_or_else(|e| panic!("{name}: {e:?}"))
            } else {
                assert_eq!(layout, LcFork::Gloas, "{name}: only Gloas finality updates follow the stall");
                sepolia::gloas_finality_update(&bytes)
            };
            Entry { name, period_update, update }
        })
        .collect()
}

/// The slot whose start is at or before `time_ms`.
fn slot_at(w: &World, time_ms: u64) -> u64 {
    let c = &w.inst.config;
    (time_ms / 1000 - c.genesis_time) / c.seconds_per_slot
}

/// Whether the native store (the program's own code, natively) would take `update` at `time_ms`: what an
/// updater checks before it pays.
fn useful(w: &World, update: &LightClientUpdate, time_ms: u64) -> bool {
    let layout = w.inst.layout_for(&w.inst.store(&w.chain), update);
    let mut next = w.native.clone();
    if layout > next.core().fork && next.upgrade(layout, &w.spec).is_err() {
        return false;
    }
    next.process_update(update, slot_at(w, time_ms), &w.ctx()).is_ok()
}

/// `World::apply` with every transaction of the job at `time_ms` (the harness sends a job just after the
/// update's signature slot; a party resuming a stalled instance sends it later). The job as the harness
/// builds it (`Instance::update_in`): `Open`, `Miller`, the adoption parts if it adopts, `Finish`; stops at
/// the first refused transaction. The native store follows only if the program applied the job.
fn apply_at(w: &mut World, update: &LightClientUpdate, label: &str, time_ms: u64) -> JobRun {
    let spec = w.spec.clone();
    let stored = w.inst.store(&w.chain);
    let layout = w.inst.layout_for(&stored, update);
    let mut next = w.native.clone();
    if layout > next.core().fork {
        next.upgrade(layout, &spec).expect("a store upgrades forwards");
    }
    let before = next.core().clone();
    let native = next.process_update(update, slot_at(w, time_ms), &w.ctx());

    let inst: &Instance = &w.inst;
    let store = steps::store_for_update(&stored, layout, update.attested_header.beacon.slot, &spec)
        .unwrap_or_else(|_| stored.clone());
    let compact = CompactUpdate::of(update);
    let witness = Instance::signer_witness(&before, update, &spec);
    let jid = job_id(&inst.id, &store.digest(), &compact.digest(layout));
    let job = job_actor(inst.program, &inst.id, &jid);
    let (planned, outcome) = plan_job(&store, &compact, &witness, update, &inst.config);
    let (program, id, parts) = (inst.program, inst.id, inst.config.adoption_parts);
    let instance = inst.actor();

    let mut reports: Vec<TxReport> = Vec::new();
    let open = w.chain.submit(
        &format!("{label} Open (step 1)"),
        instance,
        borsh::to_vec(&Message::Open { update: compact.encode(layout), signers: witness.encode() }).unwrap(),
        vec![instance, job],
        vec![],
        time_ms,
    );
    let opened = open.ok();
    reports.push(open);
    if opened {
        reports.push(w.chain.submit(
            &format!("{label} Miller (step 2)"),
            job,
            borsh::to_vec(&Message::Miller).unwrap(),
            vec![job],
            vec![],
            time_ms,
        ));
        if planned.as_ref().is_some_and(|j| j.adoption.is_some()) {
            let keys = uncompressed_keys(&update.next_sync_committee).unwrap();
            let chunk = keys.len() / parts as usize;
            for p in 0..parts {
                reports.push(w.chain.submit(
                    &format!("{label} adopt {}/{parts}", p + 1),
                    job,
                    borsh::to_vec(&Message::Adopt { part: p, keys: keys[p as usize * chunk..(p as usize + 1) * chunk].to_vec() })
                        .unwrap(),
                    vec![job],
                    vec![],
                    time_ms,
                ));
            }
        }
        let mut actors = vec![job, instance];
        if outcome.is_some_and(|o| o.advanced) {
            actors.push(header_actor(program, &id, update.finalized_header.beacon.slot));
        }
        reports.push(w.chain.submit(
            &format!("{label} Finish (step 3)"),
            job,
            borsh::to_vec(&Message::Finish).unwrap(),
            actors,
            vec![],
            time_ms,
        ));
    }
    let all_ok = reports.iter().all(TxReport::ok);
    match native {
        Ok(o) => {
            assert!(all_ok, "{label}: LEZ refused what the native store accepts: {:#?}", reports.last());
            assert_eq!(outcome, Some(o), "{label}: outcomes differ");
            w.native = next;
            w.check_store();
            if o.advanced {
                w.check_record(update.finalized_header.beacon.slot);
            }
            assert!(w.inst.job(&w.chain, &jid).is_none(), "{label}: the job is cleared");
        }
        Err(e) => {
            assert!(!all_ok, "{label}: LEZ accepted what the native store refuses ({e:?})");
            w.check_store();
        }
    }
    let _ = id;
    JobRun { job_id: jid, reports, outcome }
}

/// The attestations asked before, during and after the stall, all under record `R`: a storage slot with a
/// condition (Sepolia USDC's `totalSupply_`), a log (the block's first ERC-20 `Transfer`, its emitter and
/// recipient checked), an account (Multicall3's code hash).
fn requests(s: &Stage5) -> Vec<(&'static str, Request)> {
    let usdc = s.fx.storage_proof();
    let storage = Request {
        accounts: vec![account_query(&usdc, vec![], vec![(s.fx.slot, vec![condition(Field::Value, Cmp::Eq, s.fx.value)])])],
        ..s.request()
    };
    let t = s.fx.transfer.clone();
    let log = Request {
        logs: vec![LogQuery {
            tx_index: t.tx_index,
            log_index: t.log_index,
            proof: s.fx.receipt_proof(t.tx_index),
            // The reference consumer's acceptance rule: emitted by the token, a `Transfer`, to the recipient.
            conditions: Pool { ferrata: s.w.inst.program, instance_id: s.w.inst.id, chain_id: s.w.inst.config.chain_id, token: t.token, recipient: t.to }
                .conditions(),
        }],
        ..s.request()
    };
    let multicall = s.fx.same_address_proof();
    let account = Request {
        accounts: vec![account_query(&multicall, vec![condition(Field::CodeHash, Cmp::Eq, multicall.code_hash)], vec![])],
        ..s.request()
    };
    vec![("storage slot (USDC totalSupply_ = value)", storage), ("log (ERC-20 Transfer)", log), ("account (Multicall3)", account)]
}

fn attest_all(s: &mut Stage5, phase: &str, slot: u64, reqs: &[(&str, Request)]) -> Vec<Attestation> {
    reqs.iter()
        .map(|(label, r)| {
            let (report, a) = s.accept(&format!("{phase}: {label}"), slot, r);
            println!("{phase}: {label}: applied, {} execution gas, {} statement(s)", report.charge.gas_exec, a.statements.len());
            a
        })
        .collect()
}

fn hex0x(b: &[u8]) -> String {
    format!("0x{}", hex::encode(b))
}

#[test]
fn a_stalled_instance_keeps_attesting_and_resumes_without_repair() {
    let mut s = Stage5::new();
    let slot_r = s.fx.finalized_slot;
    let reqs = requests(&s);
    let ws_period = s.w.inst.config.ws_period_epochs;

    // 1. Before the stall: three attestations under R.
    let t0 = s.time;
    let before = attest_all(&mut s, "before the stall", slot_r, &reqs);
    let stalled = s.w.inst.state(&s.w.chain).unwrap();
    let store_stalled = s.w.inst.store(&s.w.chain);
    assert_eq!(store_stalled.finalized.slot, slot_r);

    // The updates Ethereum produced after R, as the lab recorded them.
    let entries: Vec<Entry> =
        lab_entries().into_iter().filter(|e| e.update.finalized_header.beacon.slot > slot_r).collect();
    assert!(!entries.is_empty(), "the lab recorded updates after R");
    let newest_sig = entries.iter().map(|e| e.update.signature_slot).max().unwrap();
    let periods: std::collections::BTreeSet<u64> = entries
        .iter()
        .filter(|e| e.period_update)
        .map(|e| s.w.spec.period_at_slot(e.update.attested_header.beacon.slot))
        .collect();
    // Resume one hour after the newest outstanding update was signed.
    let t_resume = s.w.inst.config.slot_start_ms(newest_sig) + 3_600_000;
    let stall_s = (t_resume - t0) / 1000;
    let ws_end = s.w.inst.config.ws_end_slot(slot_r).expect("Sepolia's instance has a bound");
    println!(
        "stall: R at slot {slot_r}; {} updates recorded after it (period updates of {:?}); resume at slot {} \
         ({stall_s} s = {:.1} days after the last attestation); weak-subjectivity bound {ws_period} epochs, \
         R's period ends at slot {ws_end}",
        entries.len(),
        periods,
        slot_at(&s.w, t_resume),
        stall_s as f64 / 86_400.0
    );
    assert!(slot_at(&s.w, t_resume) < ws_end, "the stall stays inside R's weak-subjectivity period");
    assert!(periods.len() >= 7, "the stall spans several sync-committee handoffs: {periods:?}");

    // 2. During the stall (nobody advanced it), at the block time the resume will use: the same three
    // attestations under R verify, with the same statements and identifiers.
    s.time = t_resume - 1000;
    let during = attest_all(&mut s, "during the stall", slot_r, &reqs);
    assert_eq!(during, before, "attestations under R are unchanged by the stall");
    assert_eq!(s.w.inst.state(&s.w.chain).unwrap(), stalled, "attesting changes nothing on the instance");

    // 3. No gap: the newest period's update, sent first, is refused (its committee is not known to the store),
    // and leaves the instance as it was.
    let last_period = entries.iter().rev().find(|e| e.period_update).unwrap();
    assert!(!useful(&s.w, &last_period.update, t_resume), "{}: natively refused out of order", last_period.name);
    let run = apply_at(&mut s.w, &last_period.update, &format!("out of order: {}", last_period.name), t_resume);
    let refusal = run.reports.last().unwrap();
    assert!(!refusal.ok(), "an update two or more handoffs ahead is refused");
    println!("out of order: {} refused at Open with exit {:?}", last_period.name, refusal.exit_code);
    assert_eq!(s.w.inst.state(&s.w.chain).unwrap(), stalled, "the refusal leaves the instance unchanged");

    // 4. The resume: the outstanding updates in order, each checked natively first (as an updater does), every
    // job at t_resume. No other instruction is sent: no repair, no re-Init.
    let (mut applied, mut skipped, mut adopted, mut txs) = (0usize, 0usize, Vec::new(), 0usize);
    for e in &entries {
        if !useful(&s.w, &e.update, t_resume) {
            skipped += 1;
            continue;
        }
        let run = apply_at(&mut s.w, &e.update, &format!("resume: {}", e.name), t_resume);
        assert!(run.reports.iter().all(TxReport::ok), "{}", e.name);
        txs += run.reports.len();
        applied += 1;
        let o = run.outcome.unwrap();
        if o.adopted {
            adopted.push(s.w.spec.period_at_slot(e.update.attested_header.beacon.slot));
        }
    }
    let store = s.w.inst.store(&s.w.chain);
    let lab_last = entries.last().unwrap().update.finalized_header.beacon.slot;
    println!(
        "resume: {applied} jobs applied ({txs} transactions), {skipped} recorded updates not needed; \
         committees adopted from the updates of periods {adopted:?}; finalized slot {slot_r} -> {}",
        store.finalized.slot
    );
    assert_eq!(store.finalized.slot, lab_last, "the instance catches up to the newest recorded finalized header");
    // The handoff chain has no gap: a committee was adopted from every period's update between R and the end.
    let expected: Vec<u64> = periods.iter().copied().collect();
    assert_eq!(adopted, expected, "one adoption per period, in order, none skipped");
    // The finalized root is Sepolia's canonical root (a second provider's, recorded by the fixture of 18 Nov).
    let check = [
        ("sepolia-2026-11-18", 11_606_240u64),
        ("sepolia-2026-11-18", 11_599_872),
        ("sepolia-2026-11-12", 11_562_112),
        ("sepolia-2026-11-12", 11_558_912),
    ];
    let mut checked = 0;
    for (fixture, slot) in check {
        if let Some(rec) = s.w.inst.record(&s.w.chain, slot) {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../crates/ferrata-spectests/fixtures")
                .join(fixture)
                .join(format!("beacon_header_{slot}_second.json"));
            let v: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            let canonical = harness::root(v["data"]["root"].as_str().unwrap().trim_start_matches("0x"));
            let header = rec.header().unwrap();
            assert_eq!(header.beacon.hash_tree_root(), canonical, "record at slot {slot} is Sepolia's canonical header");
            checked += 1;
        }
    }
    println!("records written on the resume compared with a second provider's canonical roots: {checked}");

    // 5. After the resume (eight rotations later), the old record R still attests, unchanged (R.3).
    s.time = t_resume + 12_000;
    let after = attest_all(&mut s, "after the resume", slot_r, &reqs);
    assert_eq!(after, before, "R's attestations after the committee rotations");

    if let Some(dir) = std::env::var_os("FERRATA_STALL_FIXTURES").map(PathBuf::from) {
        fs::create_dir_all(&dir).unwrap();
        let resumed = s.w.inst.state(&s.w.chain).unwrap();
        let rec = |slot: u64| hex0x(&borsh::to_vec(&s.w.inst.record(&s.w.chain, slot).unwrap()).unwrap());
        let summary = serde_json::json!({
            "about": "LEZ states of the R.7 test's Sepolia instance (lez/harness/tests/stall.rs), real Sepolia data, harness in dev mode",
            "program": hex0x(s.w.inst.program.value()),
            "instance_id": hex0x(&s.w.inst.id),
            "stalled": {
                "instance_state": hex0x(&borsh::to_vec(&stalled).unwrap()),
                "finalized_slot": slot_r,
                "record": rec(slot_r),
                "time_ms": t0,
            },
            "resumed": {
                "instance_state": hex0x(&borsh::to_vec(&resumed).unwrap()),
                "finalized_slot": store.finalized.slot,
                "record": rec(store.finalized.slot),
                "time_ms": t_resume,
            },
            "ws_end_slot_of_stalled": ws_end,
        });
        fs::write(dir.join("stall.json"), serde_json::to_string_pretty(&summary).unwrap()).unwrap();
        println!("wrote {}", dir.join("stall.json").display());
    }
}
