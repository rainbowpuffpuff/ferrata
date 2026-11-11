//! Milestone A1: successors and the root id, and the weak-subjectivity bound, through the program
//! (`docs/design/attestation.md` §3.5, §3.6, §4.6), on real Sepolia data.
//!
//! An instance initialised with Sepolia's schedule as it stood before Gloas was scheduled (Gloas at
//! `FAR_FUTURE_EPOCH`) follows period 1378 and cannot take the first update attested under Gloas
//! (`ForkMismatch`). A successor with the schedule that has Gloas, created from the parent's
//! latest record under the parent's root id, takes it, upgrades its store and follows the chain;
//! its records carry the root id, and an attestation bound to the root id is served by them.
//! Every invariant `Succeed` checks has a refusal here, and the bound is enforced on `Init`,
//! `Open` and `Succeed`.

use ferrata_core::{
    fork::{Fork, LcFork, FAR_FUTURE_EPOCH},
    steps,
    witness::CompactUpdate,
    Error,
};
use ferrata_lez_wire::{
    attest::Request, code, header_actor, instance_actor, instance_id, instance_seed, job_actor, job_id, successor_id,
    Config, Message, SUCCEEDED_SELECTOR, UPGRADED_SELECTOR,
};
use harness::{
    render,
    scenarios::World,
    sepolia::{self, FIRST_PERIOD, GLOAS_PERIOD},
    Chain, Instance, TxReport,
};
use lee::PublicAccountEvidence;

/// Sepolia's schedule before Gloas was scheduled: the parent's.
fn pre_gloas_config() -> Config {
    let mut c = sepolia::sepolia_config(2);
    c.fork_epochs[Fork::Gloas as usize] = FAR_FUTURE_EPOCH;
    c
}

fn submit(
    chain: &mut Chain,
    label: &str,
    at: &Instance,
    message: &Message,
    extra: Vec<lee::Actor>,
    time: u64,
) -> TxReport {
    let mut actors = vec![at.actor()];
    actors.extend(extra);
    chain.submit(label, at.actor(), borsh::to_vec(message).unwrap(), actors, vec![], time)
}

/// `Succeed` at `parent` for `config` and `checkpoint`, listing the successor it names.
fn succeed(
    chain: &mut Chain,
    parent: &Instance,
    config: &Config,
    checkpoint: [u8; 32],
    time: u64,
    label: &str,
) -> TxReport {
    let root_id = parent.state(chain).unwrap().root_id;
    let id = successor_id(&root_id, &parent.id, config, &checkpoint);
    let successor = instance_actor(parent.program, &id);
    submit(chain, label, parent, &Message::Succeed { config: config.clone(), checkpoint }, vec![successor], time)
}

fn refused_with(r: &TxReport, code: u8) {
    assert!(!r.ok(), "{}: accepted", r.label);
    assert_eq!(r.exit_code, Some(u32::from(code)), "{}: {:?}", r.label, r.charge.revert);
}

/// Refused by LEZ itself before the program runs to an exit code: the validity window.
fn refused_by_window(r: &TxReport) {
    assert!(!r.ok(), "{}: accepted", r.label);
    assert_eq!(r.exit_code, None, "{}: refused with a code, not by the window", r.label);
}

#[test]
fn a_successor_crosses_a_fork_not_in_the_schedule_fixed_at_init() {
    // The parent: Init from the bootstrap of period 1378, then period 1378's update.
    let parent_config = pre_gloas_config();
    let mut w = sepolia::sepolia_world(parent_config.clone());
    let u1378 = sepolia::sepolia_update(FIRST_PERIOD);
    let run = w.apply(&u1378, &format!("parent: period {FIRST_PERIOD}"));
    assert!(run.reports.iter().all(TxReport::ok));
    let parent = Instance {
        program: w.inst.program,
        config: w.inst.config.clone(),
        checkpoint: w.inst.checkpoint,
        id: w.inst.id,
    };
    let parent_state = parent.state(&w.chain).unwrap();
    assert_eq!(parent_state.root_id, parent.id, "an instance Init created is its own root");
    assert_eq!(parent_state.parent_id, [0; 32]);

    // The first update attested under Gloas, as a beacon node serves it: not in the parent's
    // schedule, so the parent refuses its layout.
    let u1379 = sepolia::sepolia_update(GLOAS_PERIOD);
    let before = w.native.core().clone();
    let refused =
        parent.update_in(&mut w.chain, &u1379, LcFork::Gloas, &before, &format!("parent: period {GLOAS_PERIOD}"));
    assert_eq!(refused.reports.len(), 1);
    refused_with(&refused.reports[0], Error::ForkMismatch.code() as u8);

    let store = parent.store(&w.chain);
    let checkpoint = store.finalized.hash_tree_root();
    let now = parent.config.slot_start_ms(u1379.signature_slot) + 12_000;
    let successor_config = sepolia::sepolia_config(2);

    // ---- Every invariant, refused -----------------------------------------------------------
    let mut refusals = Vec::new();
    // Invariant 2: the checkpoint is the parent's latest record, not an earlier one.
    let first_record = sepolia::sepolia_bootstrap().header.beacon.hash_tree_root();
    let r = succeed(&mut w.chain, &parent, &successor_config, first_record, now, "Succeed from an earlier record");
    refused_with(&r, code::STALE_CHECKPOINT);
    refusals.push(r);
    // Invariant 1: every immutable field copied.
    type Change = (&'static str, fn(&mut Config));
    let changes: [Change; 11] = [
        ("a changed weak-subjectivity bound", |c| c.ws_period_epochs += 1),
        ("a shorter weak-subjectivity bound", |c| c.ws_period_epochs -= 1),
        ("another chain id", |c| c.chain_id = 1),
        ("another genesis_validators_root", |c| c.genesis_validators_root[0] ^= 1),
        ("another genesis time", |c| c.genesis_time += 12),
        ("other adoption parts", |c| c.adoption_parts = 4),
        ("a settled fork at another epoch", |c| c.fork_epochs[Fork::Fulu as usize] += 1),
        ("a settled fork with another version", |c| c.fork_versions[Fork::Electra as usize][3] ^= 1),
        ("a fork inserted before the checkpoint", |c| c.fork_epochs[Fork::Gloas as usize] = 300_000),
        ("a schedule out of order", |c| c.fork_epochs[Fork::Heze as usize] = 353_000),
        ("another store layout", |c| c.store_fork = 4),
    ];
    for (label, change) in changes {
        let mut c = successor_config.clone();
        change(&mut c);
        let r = succeed(&mut w.chain, &parent, &c, checkpoint, now, &format!("Succeed with {label}"));
        refused_with(&r, code::BAD_SUCCESSOR);
        refusals.push(r);
    }
    let r = succeed(&mut w.chain, &parent, &parent_config, checkpoint, now, "Succeed with the parent's own config");
    refused_with(&r, code::BAD_SUCCESSOR);
    refusals.push(r);
    // Invariant 3: the record inside its weak-subjectivity period, by the sequencer's clock.
    let end = parent.config.ws_end_ms(store.finalized.slot).unwrap();
    let r = succeed(&mut w.chain, &parent, &successor_config, checkpoint, end, "Succeed at the end of the period");
    refused_by_window(&r);
    refusals.push(r);
    // `Spawn` from anyone but the parent instance.
    let forged_id = successor_id(&parent.id, &parent.id, &successor_config, &checkpoint);
    let forged = Message::Spawn {
        root_id: parent.id,
        parent_id: parent.id,
        checkpoint,
        config: successor_config.clone(),
        store: store.encode(),
    };
    let r = w.chain.submit(
        "Spawn sent directly",
        instance_actor(parent.program, &forged_id),
        borsh::to_vec(&forged).unwrap(),
        vec![instance_actor(parent.program, &forged_id)],
        vec![PublicAccountEvidence::Pda { program: parent.program, seed: instance_seed(&forged_id) }],
        now,
    );
    refused_with(&r, code::WRONG_SENDER);
    refusals.push(r);
    // No Init can take a successor's id (another hash domain).
    assert_ne!(instance_id(&successor_config, &checkpoint), forged_id);
    println!("{}", render(&refusals));

    // ---- The successor -----------------------------------------------------------------------
    // One millisecond inside the period, another future schedule (Heze scheduled too) is accepted.
    let mut heze = successor_config.clone();
    heze.fork_epochs[Fork::Heze as usize] = 400_000;
    let r = succeed(&mut w.chain, &parent, &heze, checkpoint, end - 1, "Succeed (Heze too) inside the period");
    assert!(r.ok(), "{:?}", r.charge.revert);

    let r = succeed(&mut w.chain, &parent, &successor_config, checkpoint, now, "Succeed");
    assert!(r.ok(), "{:?}", r.charge.revert);
    let id = successor_id(&parent.id, &parent.id, &successor_config, &checkpoint);
    let event = r.events.iter().find(|e| e.event.selector == SUCCEEDED_SELECTOR).expect("the event");
    let mut data = id.to_vec();
    data.extend_from_slice(&parent.id);
    data.extend_from_slice(&store.finalized.slot.to_le_bytes());
    assert_eq!(event.event.data, data);
    let succeed_report = r;
    let again = succeed(&mut w.chain, &parent, &successor_config, checkpoint, now, "Succeed again");
    refused_with(&again, code::ALREADY_INITIALIZED);

    let successor = Instance { program: parent.program, config: successor_config.clone(), checkpoint, id };
    let state = successor.state(&w.chain).expect("the successor exists");
    assert!(state.active, "a successor is active from the start: its committees are the parent's");
    assert_eq!(state.root_id, parent.id);
    assert_eq!(state.parent_id, parent.id);
    assert_eq!(state.store, parent.state(&w.chain).unwrap().store, "the parent's store, copied");

    // The successor follows the chain under the new schedule, beside the native store.
    let mut s = World {
        chain: w.chain,
        inst: successor,
        native: w.native.clone(),
        spec: successor_config.spec(),
        bootstrap: w.bootstrap.clone(),
        updates: vec![],
        runs: vec![],
    };
    let mut reports = vec![succeed_report];
    for period in GLOAS_PERIOD..=GLOAS_PERIOD + 2 {
        let update = sepolia::sepolia_update(period);
        let run = s.apply(&update, &format!("successor: period {period}"));
        assert!(run.reports.iter().all(TxReport::ok), "period {period}");
        let store = s.inst.store(&s.chain);
        assert_eq!(store.fork, LcFork::Gloas);
        assert_eq!(store.finalized.hash_tree_root(), sepolia::canonical_root(store.finalized.slot), "period {period}");
        let record = s.inst.record(&s.chain, store.finalized.slot).expect("a record");
        assert_eq!(record.instance_id, s.inst.id);
        assert_eq!(record.root_id, parent.id, "the successor's records carry the root id");
        reports.extend(run.reports);
    }
    let upgrades = reports.iter().filter(|r| r.events.iter().any(|e| e.event.selector == UPGRADED_SELECTOR)).count();
    assert_eq!(upgrades, 1, "the successor's store upgrades itself once, at the fork");
    println!("{}", render(&reports));

    // An attestation bound to the root id is served by the successor's record; one bound to
    // another instance is refused before anything else is looked at.
    let slot = s.inst.store(&s.chain).finalized.slot;
    let header = header_actor(s.inst.program, &s.inst.id, slot);
    let chain_id = successor_config.chain_id;
    let ask = |chain: &mut Chain, bound: [u8; 32], label: &str| {
        let request = Request { instance_id: bound, chain_id, ..Request::default() };
        chain.submit(label, header, borsh::to_vec(&Message::Attest { request }).unwrap(), vec![header], vec![], now)
    };
    let by_root = ask(&mut s.chain, parent.id, "Attest bound to the root id");
    let by_instance = ask(&mut s.chain, s.inst.id, "Attest bound to the successor");
    let by_other = ask(&mut s.chain, [7; 32], "Attest bound to another instance");
    refused_with(&by_other, code::WRONG_INSTANCE);
    // The empty request gets past the binding to the record's own refusal, the same for both.
    assert_ne!(by_root.exit_code, Some(u32::from(code::WRONG_INSTANCE)), "{:?}", by_root.exit_code);
    assert_eq!(by_root.exit_code, by_instance.exit_code);
    println!("{}", render(&[by_root, by_instance, by_other]));
}

#[test]
fn the_weak_subjectivity_bound_on_init_and_open() {
    let config = sepolia::sepolia_config(2);
    let bootstrap = sepolia::sepolia_bootstrap();
    let checkpoint = harness::root(sepolia::SEPOLIA_CHECKPOINT);

    // Init: valid only while the checkpoint is inside its own period.
    let mut chain = Chain::new();
    let inst = Instance::new(harness::ferrata_account(), config.clone(), checkpoint);
    let end = config.ws_end_ms(bootstrap.header.beacon.slot).unwrap();
    let compact = ferrata_core::witness::CompactBootstrap::of(&bootstrap);
    let fork = inst.fork();
    let (_, job) = steps::initialize(&checkpoint, &compact, fork, &config.spec(), config.adoption_parts).unwrap();
    let jid = job_id(&inst.id, &job.store_digest, &job.update_digest);
    let init = Message::Init { config: config.clone(), checkpoint, bootstrap: compact.encode(fork) };
    let evidence = || vec![PublicAccountEvidence::Pda { program: inst.program, seed: instance_seed(&inst.id) }];
    let actors = vec![inst.actor(), job_actor(inst.program, &inst.id, &jid)];
    let late = chain.submit(
        "Init after the period",
        inst.actor(),
        borsh::to_vec(&init).unwrap(),
        actors.clone(),
        evidence(),
        end,
    );
    refused_by_window(&late);
    assert!(inst.state(&chain).is_none());
    let mut zero = config.clone();
    zero.ws_period_epochs = 0;
    let zinst = Instance::new(harness::ferrata_account(), zero.clone(), checkpoint);
    let zinit = Message::Init { config: zero, checkpoint, bootstrap: compact.encode(fork) };
    let r = chain.submit(
        "Init without a period",
        zinst.actor(),
        borsh::to_vec(&zinit).unwrap(),
        vec![zinst.actor()],
        vec![PublicAccountEvidence::Pda { program: zinst.program, seed: instance_seed(&zinst.id) }],
        end - 1,
    );
    refused_with(&r, code::BAD_CONFIG);

    // Open: valid from the signature slot to the end of the stored header's period.
    let mut w = sepolia::sepolia_world(config.clone());
    let update = sepolia::sepolia_update(FIRST_PERIOD);
    let stored = w.inst.store(&w.chain);
    let end = config.ws_end_ms(stored.finalized.slot).unwrap();
    let compact = CompactUpdate::of(&update);
    let witness = Instance::signer_witness(w.native.core(), &update, &w.spec);
    let jid = job_id(&w.inst.id, &stored.digest(), &compact.digest(LcFork::Electra));
    let open = Message::Open { update: compact.encode(LcFork::Electra), signers: witness.encode() };
    let job = job_actor(w.inst.program, &w.inst.id, &jid);
    let inst = Instance {
        program: w.inst.program,
        config: w.inst.config.clone(),
        checkpoint: w.inst.checkpoint,
        id: w.inst.id,
    };
    let late = submit(&mut w.chain, "Open after the period", &inst, &open, vec![job], end);
    refused_by_window(&late);
    let last = submit(&mut w.chain, "Open in the period's last millisecond", &inst, &open, vec![job], end - 1);
    assert!(last.ok(), "{:?}", last.charge.revert);

    // An update signed after the stored header's period ends is refused in the guest (53): an
    // instance with a one-epoch period cannot take period 1378's update at all.
    let mut short = config.clone();
    short.ws_period_epochs = 1;
    let mut w = sepolia::sepolia_world(short.clone());
    let before = w.native.core().clone();
    let stored = w.inst.store(&w.chain);
    assert!(update.signature_slot >= short.ws_end_slot(stored.finalized.slot).unwrap());
    let run = w.inst.update_in(&mut w.chain, &update, LcFork::Electra, &before, "Open signed after the period");
    assert_eq!(run.reports.len(), 1);
    refused_with(&run.reports[0], Error::OutsideWeakSubjectivityPeriod.code() as u8);
    println!("{}", render(&[late, last, run.reports[0].clone()]));
}

/// The image ids of the guests this branch builds (the program, its consumers and the bridge),
/// for the record (`--nocapture`).
#[test]
fn prints_the_image_ids() {
    for (name, id, elf) in [
        ("program (guests/program)", methods::FERRATA_LEZ_ID, methods::FERRATA_LEZ_ELF.len()),
        ("consumer (guests/consumer)", methods::FERRATA_CONSUMER_ID, methods::FERRATA_CONSUMER_ELF.len()),
        ("holdings (guests/holdings)", methods::FERRATA_HOLDINGS_ID, methods::FERRATA_HOLDINGS_ELF.len()),
        ("bridge (guests/bridge)", methods::FERRATA_BRIDGE_ID, methods::FERRATA_BRIDGE_ELF.len()),
    ] {
        println!("image {name}: {} ({elf} bytes)", harness::image_id_hex(id));
    }
}
