//! The Stage 1 scenarios, shared by the tests (which assert) and the `tables` binary (which
//! writes `lez/results/`). Each runs the program through LEZ's settlement next to the native
//! `FinalizedStore` (`Adoption::OnChain`) and asserts, after every job, that the instance holds
//! the compact form of the native store and the native finalized header's record.

use ferrata_core::{
    bls::{Aggregation, Blst},
    fork::{ChainSpec, LcFork},
    policy::{Adoption, FinalizedStore},
    steps::HeaderRecord,
    types::{LightClientBootstrap, LightClientUpdate},
    validate::Context,
    witness::CompactStore,
};
use ferrata_lez_wire::{header_actor, instance_seed, job_actor, Config, Message, RecordState};
use lee::{AccountId, Actor, PublicAccountEvidence};
use test_guest_core::Script;

use crate::*;

pub static BLS: Blst = Blst::new(Aggregation::Auto);

/// A chain with one instance, next to the native store. `Clone`, so that a test can fork one world
/// into many (the red-team tests, `tests/redteam_lc.rs`).
#[derive(Clone)]
pub struct World {
    pub chain: Chain,
    pub inst: Instance,
    pub native: FinalizedStore,
    pub spec: ChainSpec,
    pub bootstrap: LightClientBootstrap,
    pub updates: Vec<LightClientUpdate>,
    pub runs: Vec<JobRun>,
}

impl World {
    /// The mainnet instance of periods 1899–1903: `Init` and the bootstrap committee's adoption,
    /// in `parts` transactions.
    pub fn new(parts: u8) -> Self {
        Self::from_parts(mainnet_config(parts), root(TRUSTED_BLOCK_ROOT), mainnet_bootstrap(), mainnet_updates())
    }

    /// An instance of `config` initialized from `bootstrap` (in the layout `config.store_fork`
    /// names) at `checkpoint`, on a new chain; `updates` are kept for the caller to apply.
    pub fn from_parts(
        config: Config,
        checkpoint: Root,
        bootstrap: LightClientBootstrap,
        updates: Vec<LightClientUpdate>,
    ) -> Self {
        Self::at(ferrata_account(), config, checkpoint, bootstrap, updates)
    }

    /// As [`World::from_parts`], for the deployment of the program at `program`.
    pub fn at(
        program: AccountId,
        config: Config,
        checkpoint: Root,
        bootstrap: LightClientBootstrap,
        updates: Vec<LightClientUpdate>,
    ) -> Self {
        let mut chain = Chain::new();
        let spec = config.spec();
        let inst = Instance::new(program, config, checkpoint);
        let native =
            FinalizedStore::initialize_with(&checkpoint, &bootstrap, inst.fork(), &spec, &BLS, Adoption::OnChain)
                .unwrap();
        let run = inst.init(&mut chain, &bootstrap, "Init");
        assert!(run.reports.iter().all(TxReport::ok), "initialization refused: {:?}", run.reports);
        let world = Self { chain, inst, native, spec, bootstrap, updates, runs: vec![run] };
        let state = world.inst.state(&world.chain).unwrap();
        assert!(state.active, "the instance activates with its last adoption part");
        world.check_store();
        world.check_record(world.bootstrap.header.beacon.slot);
        world
    }

    pub fn ctx(&self) -> Context<'_> {
        Context { spec: &self.spec, genesis_validators_root: self.inst.config.genesis_validators_root, bls: &BLS }
    }

    /// The instance holds the compact form of the native store.
    pub fn check_store(&self) {
        assert_eq!(
            self.inst.store(&self.chain),
            CompactStore::of(self.native.core()),
            "instance and native store differ"
        );
    }

    /// `header(id, slot)` holds the native finalized header's record.
    pub fn check_record(&self, slot: u64) {
        let record = self.inst.record(&self.chain, slot).expect("record written");
        assert_eq!(record.instance_id, self.inst.id);
        assert_eq!(record.chain_id, self.inst.config.chain_id, "the record carries the instance's chain id");
        let native = self.native.core();
        let expected = HeaderRecord::of(&native.finalized_header, native.fork, &self.spec);
        assert_eq!(record.record, expected.encode(), "the record is the native finalized header's");
    }

    /// Run `update` as a job on LEZ and natively; they must agree. The update goes to the
    /// instance in the layout a beacon node serves it in ([`Instance::layout_for`]). Where that
    /// is a later layout than the store's, the instance upgrades itself; the native store is
    /// upgraded here by hand, as a light client does at a fork, and keeps the upgrade only if
    /// the update is accepted.
    pub fn apply(&mut self, update: &LightClientUpdate, label: &str) -> JobRun {
        self.apply_with(update, label, |chain, label, _job_id, job_actor, actors, time| {
            chain.submit(label, job_actor, borsh::to_vec(&Message::Finish).unwrap(), actors, vec![], time)
        })
    }

    /// [`World::apply`] with the job's last step sent by `finish` ([`Instance::update_in_with`]).
    pub fn apply_with<F>(&mut self, update: &LightClientUpdate, label: &str, finish: F) -> JobRun
    where
        F: FnOnce(&mut Chain, &str, Root, Actor, Vec<Actor>, u64) -> TxReport,
    {
        let layout = self.inst.layout_for(&self.inst.store(&self.chain), update);
        let mut next = self.native.clone();
        if layout > next.core().fork {
            next.upgrade(layout, &self.spec).expect("a store upgrades forwards");
        }
        let before = next.core().clone();
        let ctx =
            Context { spec: &self.spec, genesis_validators_root: self.inst.config.genesis_validators_root, bls: &BLS };
        let native = next.process_update(update, update.signature_slot + 1, &ctx);
        let run = self.inst.update_in_with(&mut self.chain, update, layout, &before, label, finish);
        let all_ok = run.reports.iter().all(TxReport::ok);
        match native {
            Ok(outcome) => {
                self.native = next;
                assert!(all_ok, "{label}: LEZ refused what the native store accepts: {:#?}", run.reports.last());
                assert_eq!(run.outcome, Some(outcome), "{label}: outcomes differ");
                self.check_store();
                if outcome.advanced {
                    self.check_record(update.finalized_header.beacon.slot);
                }
                assert!(self.inst.job(&self.chain, &run.job_id).is_none(), "{label}: the job is cleared");
            }
            Err(e) => {
                assert!(!all_ok, "{label}: LEZ accepted what the native store refuses ({e:?})");
                self.check_store();
            }
        }
        self.runs.push(run.clone());
        run
    }
}

/// Init, then the five mainnet updates as served (each adopts a committee; four rotate).
pub fn replay(parts: u8) -> World {
    let mut world = World::new(parts);
    for (i, (slot, block_root)) in EXPECTED_FINALIZED.into_iter().enumerate() {
        let update = world.updates[i].clone();
        world.apply(&update, &format!("update_{i}"));
        let store = world.inst.store(&world.chain);
        assert_eq!(store.finalized.slot, slot);
        assert_eq!(store.finalized.hash_tree_root(), root(block_root), "the canonical root a second provider reports");
    }
    world
}

/// Init, update_0, then update_1 stripped to a finality update (three transactions, no
/// adoption), then update_1 as served (it now only supplies the next committee).
pub fn finality_path() -> World {
    let mut world = World::new(2);
    let u0 = world.updates[0].clone();
    world.apply(&u0, "update_0");
    let finality = as_finality_update(&world.updates[1]);
    let run = world.apply(&finality, "update_1 as finality update");
    assert_eq!(run.reports.len(), 3, "a finality update is three transactions");
    let u1 = world.updates[1].clone();
    let run = world.apply(&u1, "update_1 (next committee only)");
    assert_eq!(run.outcome.map(|o| (o.advanced, o.adopted)), Some((false, true)));
    world
}

/// One refusal and what LEZ charged for it.
pub struct Refusal {
    pub case: &'static str,
    pub report: TxReport,
}

/// The negative cases, each against a fresh world after update_0.
pub fn refusals() -> Vec<Refusal> {
    let mut out = Vec::new();
    let fresh = || {
        let mut w = World::new(2);
        let u0 = w.updates[0].clone();
        w.apply(&u0, "update_0");
        w
    };

    // A forged signature: update_1 (as a finality update) carrying update_2's signature, a valid
    // point of G2 over another message. Steps 1 and 2 pass; step 3 refuses.
    {
        let mut w = fresh();
        let mut forged = as_finality_update(&w.updates[1]);
        forged.sync_aggregate.signature = w.updates[2].sync_aggregate.signature;
        let before = w.inst.store(&w.chain);
        let run = w.apply(&forged, "forged signature");
        assert_eq!(run.reports.len(), 3);
        assert!(run.reports[0].ok() && run.reports[1].ok());
        assert_eq!(run.reports[2].exit_code, Some(40), "SignatureInvalid at the final exponentiation");
        assert_eq!(w.inst.store(&w.chain), before, "a refused job changes nothing");
        for (r, case) in run.reports.into_iter().zip([
            "forged signature: Open (passes)",
            "forged signature: Miller (passes)",
            "forged signature: Finish",
        ]) {
            out.push(Refusal { case, report: r });
        }
    }

    // A signature that is not a point: refused at step 1.
    {
        let mut w = fresh();
        let mut bad = as_finality_update(&w.updates[1]);
        bad.sync_aggregate.signature[0] &= 0x7f; // compression flag cleared
        let run = w.apply(&bad, "malformed signature");
        assert_eq!(run.reports.len(), 1);
        assert_eq!(run.reports[0].exit_code, Some(41), "SignatureMalformed at step 1");
        out.push(Refusal { case: "malformed signature: Open", report: run.reports[0].clone() });
    }

    // Replayed and duplicated steps.
    {
        let mut w = fresh();
        let update = as_finality_update(&w.updates[1]);
        let before = w.native.core().clone();
        let compact = ferrata_core::witness::CompactUpdate::of(&update);
        let witness = Instance::signer_witness(&before, &update, &w.spec);
        let store = w.inst.store(&w.chain);
        let jid = ferrata_lez_wire::job_id(&w.inst.id, &store.digest(), &compact.digest(LcFork::Electra));
        let job = job_actor(w.inst.program, &w.inst.id, &jid);
        let time = w.inst.config.slot_start_ms(update.signature_slot) + 12_000;
        let open_msg =
            borsh::to_vec(&Message::Open { update: compact.encode(LcFork::Electra), signers: witness.encode() })
                .unwrap();
        let open = w.chain.submit("Open", w.inst.actor(), open_msg.clone(), vec![w.inst.actor(), job], vec![], time);
        assert!(open.ok());
        let dup = w.chain.submit(
            "Open again (duplicate job)",
            w.inst.actor(),
            open_msg.clone(),
            vec![w.inst.actor(), job],
            vec![],
            time,
        );
        assert_eq!(dup.exit_code, Some(ferrata_lez_wire::code::JOB_EXISTS as u32));
        out.push(Refusal { case: "duplicate Open (same job)", report: dup });
        let finish_early = w.chain.submit(
            "Finish before Miller",
            job,
            borsh::to_vec(&Message::Finish).unwrap(),
            vec![job, w.inst.actor()],
            vec![],
            time,
        );
        assert_eq!(finish_early.exit_code, Some(81), "StepError::Phase");
        out.push(Refusal { case: "Finish before Miller (skipped step)", report: finish_early });
        let miller = w.chain.submit("Miller", job, borsh::to_vec(&Message::Miller).unwrap(), vec![job], vec![], time);
        assert!(miller.ok());
        let replay =
            w.chain.submit("Miller again", job, borsh::to_vec(&Message::Miller).unwrap(), vec![job], vec![], time);
        assert_eq!(replay.exit_code, Some(81), "StepError::Phase");
        out.push(Refusal { case: "Miller replayed", report: replay });
        let header = header_actor(w.inst.program, &w.inst.id, update.finalized_header.beacon.slot);
        let finish = w.chain.submit(
            "Finish",
            job,
            borsh::to_vec(&Message::Finish).unwrap(),
            vec![job, w.inst.actor(), header],
            vec![],
            time,
        );
        assert!(finish.ok());
        let again = w.chain.submit(
            "Finish again",
            job,
            borsh::to_vec(&Message::Finish).unwrap(),
            vec![job, w.inst.actor(), header],
            vec![],
            time,
        );
        assert_eq!(again.exit_code, Some(ferrata_lez_wire::code::NO_JOB as u32));
        out.push(Refusal { case: "Finish replayed after the job applied", report: again });
        let reopen = w.chain.submit(
            "Open the applied update again",
            w.inst.actor(),
            open_msg,
            vec![w.inst.actor(), job],
            vec![],
            time,
        );
        assert_eq!(reopen.exit_code, Some(52), "NotNewer: the store moved on");
        out.push(Refusal { case: "Open an applied update again", report: reopen });
    }

    // Steps sent by the wrong actor.
    {
        let mut w = fresh();
        let inst = w.inst.clone();
        let fake_job = ferrata_core::steps::Job {
            store_digest: w.inst.store(&w.chain).digest(),
            update_digest: [0; 32],
            phase: ferrata_core::steps::Phase::Verified,
            adoption: None,
            pending: ferrata_core::steps::PendingApply::Update {
                finalized: ferrata_core::types::BeaconBlockHeader { slot: 99_999_999, ..Default::default() },
                next: None,
                record: HeaderRecord::of(&w.bootstrap.header, LcFork::Electra, &w.spec),
            },
        };
        let jid = [7u8; 32];
        let apply = Message::Apply { instance_id: inst.id, job_id: jid, job: fake_job.encode() };
        let time = inst.config.slot_start_ms(w.updates[1].signature_slot);
        let root_apply = w.chain.submit(
            "Apply as the transaction root",
            inst.actor(),
            borsh::to_vec(&apply).unwrap(),
            vec![inst.actor()],
            vec![],
            time,
        );
        assert_eq!(root_apply.exit_code, Some(ferrata_lez_wire::code::WRONG_SENDER as u32));
        out.push(Refusal { case: "Apply sent as the transaction root", report: root_apply });

        // An actor of another program (LEZ's `scripted` test program) sending internal messages.
        let seed = lee_core::program::PdaSeed::new([7; 32]);
        let attacker = Actor::new(AccountId::for_public_pda(&scripted_account(), &seed), scripted_account());
        let evidence = vec![PublicAccountEvidence::Pda { program: scripted_account(), seed }];
        // The target accounts are admitted by evidence, so that the program's own check is what
        // refuses (an unadmitted actor is refused by LEZ itself, at the whole gas limit).
        let mut scripted =
            |label: &str, to: Actor, message: &Message, target_seed: Option<lee_core::program::PdaSeed>| {
                let script = Script::default().send_sync(to, message);
                let mut evidence = evidence.clone();
                if let Some(seed) = target_seed {
                    evidence.push(PublicAccountEvidence::Pda { program: inst.program, seed });
                }
                w.chain.submit(label, attacker, borsh::to_vec(&script).unwrap(), vec![attacker, to], evidence, time)
            };
        let r = scripted("Apply from another program", inst.actor(), &apply, None);
        assert_eq!(r.exit_code, Some(ferrata_lez_wire::code::WRONG_SENDER as u32));
        out.push(Refusal { case: "Apply from another program's actor", report: r });
        let verified = Message::ApplyVerified { instance_id: inst.id, job: fake_job.encode() };
        let r = scripted("ApplyVerified from another program", inst.actor(), &verified, None);
        assert_eq!(r.exit_code, Some(ferrata_lez_wire::code::WRONG_SENDER as u32));
        out.push(Refusal { case: "ApplyVerified from another program's actor", report: r });
        let job = job_actor(inst.program, &inst.id, &jid);
        let open = Message::JobOpen { instance_id: inst.id, job_id: jid, committee_size: 512, job: fake_job.encode() };
        let r = scripted("JobOpen from another program", job, &open, Some(ferrata_lez_wire::job_seed(&inst.id, &jid)));
        assert_eq!(r.exit_code, Some(ferrata_lez_wire::code::WRONG_SENDER as u32));
        out.push(Refusal { case: "JobOpen from another program's actor", report: r });
        let header = header_actor(inst.program, &inst.id, 99_999_999);
        let record = Message::Record {
            instance_id: inst.id,
            slot: 99_999_999,
            chain_id: 1,
            record: vec![1, 2, 3],
            root_id: inst.id,
        };
        let r = scripted(
            "Record from another program",
            header,
            &record,
            Some(ferrata_lez_wire::header_seed(&inst.id, 99_999_999)),
        );
        assert_eq!(r.exit_code, Some(ferrata_lez_wire::code::WRONG_SENDER as u32));
        out.push(Refusal { case: "Record from another program's actor", report: r });
        // The same image deployed at another address is another program: its jobs are not ours.
        let twin_job = job_actor(TWIN, &inst.id, &jid);
        let twin_evidence =
            vec![PublicAccountEvidence::Pda { program: TWIN, seed: ferrata_lez_wire::job_seed(&inst.id, &jid) }];
        let r = w.chain.submit(
            "Miller at the twin deployment",
            twin_job,
            borsh::to_vec(&Message::Miller).unwrap(),
            vec![twin_job],
            twin_evidence,
            time,
        );
        assert_eq!(r.exit_code, Some(ferrata_lez_wire::code::NO_JOB as u32));
        assert!(w.inst.record(&w.chain, 99_999_999).is_none());
        w.check_store();
    }

    // An update for another instance: the same mainnet data, an instance of another chain (its
    // genesis_validators_root differs). Its bootstrap verifies (no signature), the update does not.
    {
        let mut w = fresh();
        let mut config = mainnet_config(2);
        config.genesis_validators_root = [0x42; 32];
        let other = Instance::new(ferrata_account(), config, root(TRUSTED_BLOCK_ROOT));
        assert_ne!(other.id, w.inst.id);
        let init = other.init(&mut w.chain, &w.bootstrap.clone(), "other chain");
        assert!(init.reports.iter().all(TxReport::ok));
        let full = w.native.core().clone();
        // The other instance has not seen update_0; as on its own chain, update_0 is next.
        let mut native_other = FinalizedStore::initialize_with(
            &root(TRUSTED_BLOCK_ROOT),
            &w.bootstrap,
            LcFork::Electra,
            &w.spec,
            &BLS,
            Adoption::OnChain,
        )
        .unwrap();
        let _ = full;
        let ctx = Context { spec: &w.spec, genesis_validators_root: [0x42; 32], bls: &BLS };
        let u0 = w.updates[0].clone();
        assert_eq!(
            native_other.process_update(&u0, u0.signature_slot + 1, &ctx),
            Err(ferrata_core::Error::SignatureInvalid)
        );
        let run = other.update(&mut w.chain, &u0, native_other.core(), "update_0 on another chain's instance");
        let last = run.reports.last().unwrap();
        assert_eq!(last.exit_code, Some(40), "SignatureInvalid");
        out.push(Refusal { case: "update for another chain's instance: Finish", report: last.clone() });
        // And a step of instance A's job sent to instance B's job address: no such job.
        let a_job = w.runs.last().unwrap().job_id;
        let b_job = job_actor(other.program, &other.id, &a_job);
        let b_evidence = vec![PublicAccountEvidence::Pda {
            program: other.program,
            seed: ferrata_lez_wire::job_seed(&other.id, &a_job),
        }];
        let r = w.chain.submit(
            "A's job id at B",
            b_job,
            borsh::to_vec(&Message::Finish).unwrap(),
            vec![b_job],
            b_evidence,
            last_time(&w),
        );
        assert_eq!(r.exit_code, Some(ferrata_lez_wire::code::NO_JOB as u32));
        out.push(Refusal { case: "instance A's job id sent to instance B", report: r });
    }

    // Too early: an Open before its signature slot is valid only from that slot's start.
    {
        let mut w = fresh();
        let update = as_finality_update(&w.updates[1]);
        let store = w.inst.store(&w.chain);
        let compact = ferrata_core::witness::CompactUpdate::of(&update);
        let witness = Instance::signer_witness(w.native.core(), &update, &w.spec);
        let jid = ferrata_lez_wire::job_id(&w.inst.id, &store.digest(), &compact.digest(LcFork::Electra));
        let job = job_actor(w.inst.program, &w.inst.id, &jid);
        let early = w.inst.config.slot_start_ms(update.signature_slot) - 1;
        let msg = borsh::to_vec(&Message::Open { update: compact.encode(LcFork::Electra), signers: witness.encode() })
            .unwrap();
        let r = w.chain.submit(
            "Open before the signature slot",
            w.inst.actor(),
            msg,
            vec![w.inst.actor(), job],
            vec![],
            early,
        );
        assert!(r.charge.revert.as_deref().is_some_and(|e| e.contains("validity window")), "{:?}", r.charge.revert);
        out.push(Refusal { case: "Open before its signature slot (validity window)", report: r });
    }

    // Init again.
    {
        let mut w = fresh();
        let inst = w.inst.clone();
        let msg = Message::Init {
            config: inst.config.clone(),
            checkpoint: inst.checkpoint,
            bootstrap: ferrata_core::witness::CompactBootstrap::of(&w.bootstrap).encode(LcFork::Electra),
        };
        let r = w.chain.submit(
            "Init again",
            inst.actor(),
            borsh::to_vec(&msg).unwrap(),
            vec![inst.actor()],
            vec![PublicAccountEvidence::Pda { program: inst.program, seed: instance_seed(&inst.id) }],
            0,
        );
        assert_eq!(r.exit_code, Some(ferrata_lez_wire::code::ALREADY_INITIALIZED as u32));
        out.push(Refusal { case: "Init of an existing instance", report: r });
    }
    out
}

fn last_time(w: &World) -> u64 {
    w.inst.config.slot_start_ms(w.updates[1].signature_slot) + 12_000
}

/// The record of `slot`, decoded.
pub fn record(w: &World, slot: u64) -> Option<RecordState> {
    w.inst.record(&w.chain, slot)
}

/// What the LEZ shell costs, measured with this program: a public consumer's read of a record
/// (consumer → `Read` → `StateReply` → consumer), and the same read with padding that grows
/// (`ReadPadded`, which the program ignores), so the header's handler differs only in the bytes
/// it receives and echoes in its journal.
pub fn shell_costs() -> Vec<TxReport> {
    use ferrata_lez_wire::consumer::ConsumerMessage;
    let mut w = World::new(2);
    let u0 = w.updates[0].clone();
    w.apply(&u0, "update_0");
    let slot = w.bootstrap.header.beacon.slot;
    let seed = lee_core::program::PdaSeed::new([0x33; 32]);
    let consumer = Actor::new(AccountId::for_public_pda(&consumer_account(), &seed), consumer_account());
    let evidence = vec![PublicAccountEvidence::Pda { program: consumer_account(), seed }];
    let header = header_actor(w.inst.program, &w.inst.id, slot);
    let mut out = Vec::new();
    let read = ConsumerMessage::ReadRecord { ferrata: w.inst.program, instance_id: w.inst.id, slot, query: vec![] };
    out.push(w.chain.submit(
        "public consumer reads header(id, slot)",
        consumer,
        borsh::to_vec(&read).unwrap(),
        vec![consumer, header],
        evidence.clone(),
        0,
    ));
    for size in [0usize, 8 * 1024, 24 * 1024, 48 * 1024] {
        let call = ConsumerMessage::ReadPadded {
            ferrata: w.inst.program,
            instance_id: w.inst.id,
            slot,
            padding: vec![0xab; size],
        };
        out.push(w.chain.submit(
            &format!("public consumer reads with {size} bytes of padding (ReadPadded)"),
            consumer,
            borsh::to_vec(&call).unwrap(),
            vec![consumer, header],
            evidence.clone(),
            0,
        ));
    }
    for r in &out {
        assert!(r.ok(), "{}: {:?}", r.label, r.charge.revert);
    }
    out
}

/// The whole verification of a finality update in one public transaction: `VerifyAll` sent to a
/// public actor of the program (the payer's account under the program), which hands the
/// verified job to the instance with `ApplyVerified`.
pub fn one_transaction_update() -> Vec<TxReport> {
    use ferrata_core::witness::CompactUpdate;
    let mut w = World::new(2);
    let u0 = w.updates[0].clone();
    w.apply(&u0, "update_0");
    let update = as_finality_update(&w.updates[1]);
    let store = w.inst.store(&w.chain);
    let witness = Instance::signer_witness(w.native.core(), &update, &w.spec);
    let message = Message::VerifyAll {
        config: w.inst.config.clone(),
        checkpoint: w.inst.checkpoint,
        store: store.encode(),
        update: CompactUpdate::of(&update).encode(LcFork::Electra),
        signers: witness.encode(),
        adoption: vec![],
    };
    let verifier = Actor::new(w.chain.payer, ferrata_account());
    let header = header_actor(w.inst.program, &w.inst.id, update.finalized_header.beacon.slot);
    let time = w.inst.config.slot_start_ms(update.signature_slot) + 12_000;
    let report = w.chain.submit(
        "finality update in one transaction (VerifyAll)",
        verifier,
        borsh::to_vec(&message).unwrap(),
        vec![verifier, w.inst.actor(), header],
        vec![],
        time,
    );
    assert!(report.ok(), "{:?}", report.charge.revert);
    let spec = w.spec.clone();
    let ctx = Context { spec: &spec, genesis_validators_root: root(GENESIS_VALIDATORS_ROOT), bls: &BLS };
    w.native.process_update(&update, update.signature_slot + 1, &ctx).unwrap();
    w.check_store();
    w.check_record(update.finalized_header.beacon.slot);
    vec![report]
}
