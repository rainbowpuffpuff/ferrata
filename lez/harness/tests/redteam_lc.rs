//! Red team, light client: the blind vectors `LC-*` of `vectors/redteam-2026-11-18` (written from
//! the specification before any implementation was read) through the attestation program on LEZ
//! v0.4's state machine (the in-process harness; run with `RISC0_DEV_MODE=1`). Each vector's input
//! is built from a real fixture (mainnet periods 1899-1903, Sepolia through Gloas) or, where the
//! vector needs keys no real committee gives out, from a synthetic chain whose committee the test
//! holds (`ferrata_spectests::testkit`). The verdicts are the shared judge's
//! (`redteam_common`); results go to `results/redteam-2026-11-18/redteam_lc.<part>.json`.
//! Three vectors whose target is the native core (LC-SIG-15, LC-PART-04, LC-PART-05) run in
//! `crates/ferrata-spectests/tests/redteam_core.rs`.

mod redteam_common;

use ferrata_core::{
    fork::{Fork, LcFork},
    merkle::{CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA, ZERO},
    steps,
    types::{Execution, LightClientBootstrap, LightClientHeader, LightClientUpdate},
    validate::{sync_committee_signing_root, Context},
    witness::{uncompressed_keys, CompactBootstrap, CompactUpdate, SignerWitness, IDENTITY_UNCOMPRESSED},
};
use ferrata_lez_wire::{
    header_actor, instance_actor, instance_id, instance_seed, job_actor, job_id, job_seed, successor_id, Config,
    Message, UPGRADED_SELECTOR,
};
use ferrata_spectests::testkit::{fabricated_header, fabricated_update, fake_committee, sign, sparse_tree, Fabrication};
use harness::{
    as_finality_update, ferrata_account, mainnet_config, plan_job, root,
    scenarios::{World, BLS},
    scripted_account,
    sepolia::{self, FIRST_PERIOD, GLOAS_PERIOD},
    Instance, TxReport, EXPECTED_FINALIZED,
};
use lee::{AccountId, Actor, PublicAccountEvidence};
use redteam_common::{Observed, Results};
use test_guest_core::Script;

const FILE: &str = "redteam_lc";

// ---------------------------------------------------------------------------------------------
// Worlds
// ---------------------------------------------------------------------------------------------

/// B-LC-MAIN: the mainnet instance of period 1899 (Init, adoption in two parts) after update_0,
/// which supplies the next committee; its store is in period 1899 with the next committee known.
fn base() -> World {
    let mut w = World::new(2);
    let u0 = w.updates[0].clone();
    w.apply(&u0, "update_0");
    w
}

/// U: the next real finality update of the base world (update_1 without its next committee).
fn u(w: &World) -> LightClientUpdate {
    as_finality_update(&w.updates[1])
}

/// The submission time a daemon uses for an update: 12 s into its signature slot.
fn time_of(w: &World, update: &LightClientUpdate) -> u64 {
    w.inst.config.slot_start_ms(update.signature_slot) + 12_000
}

/// The witness for `update` from the native store's full committees.
fn witness(w: &World, update: &LightClientUpdate) -> SignerWitness {
    Instance::signer_witness(w.native.core(), update, &w.spec)
}

/// A whole job for `update` in `layout`, as a submitter sends it: `Open`, `Miller`, the adoption
/// parts if the job (natively planned) adopts a committee, `Finish`; it stops at the first refused
/// transaction. Unlike `Instance::update_in`, `Finish` always lists the record the update would
/// write, so that an acceptance the native plan did not foresee is seen as an acceptance.
fn run_job(w: &mut World, update: &LightClientUpdate, layout: LcFork, wit: &SignerWitness, label: &str) -> Vec<TxReport> {
    let inst = w.inst.clone();
    let spec = inst.config.spec();
    let stored = inst.store(&w.chain);
    let store = steps::store_for_update(&stored, layout, update.attested_header.beacon.slot, &spec)
        .unwrap_or_else(|_| stored.clone());
    let compact = CompactUpdate::of(update);
    let jid = job_id(&inst.id, &store.digest(), &compact.digest(layout));
    let job = job_actor(inst.program, &inst.id, &jid);
    let (planned, _) = plan_job(&store, &compact, wit, update, &inst.config);
    let time = time_of(w, update);
    let mut reports = Vec::new();
    let open = w.chain.submit(
        &format!("{label}: Open"),
        inst.actor(),
        borsh::to_vec(&Message::Open { update: compact.encode(layout), signers: wit.encode() }).unwrap(),
        vec![inst.actor(), job],
        vec![],
        time,
    );
    let ok = open.ok();
    reports.push(open);
    if !ok {
        return reports;
    }
    let miller =
        w.chain.submit(&format!("{label}: Miller"), job, borsh::to_vec(&Message::Miller).unwrap(), vec![job], vec![], time);
    let ok = miller.ok();
    reports.push(miller);
    if !ok {
        return reports;
    }
    if planned.as_ref().is_some_and(|j| j.adoption.is_some()) {
        if let Ok(keys) = uncompressed_keys(&update.next_sync_committee) {
            let parts = inst.config.adoption_parts as usize;
            let chunk = keys.len() / parts;
            for p in 0..parts {
                let r = w.chain.submit(
                    &format!("{label}: Adopt {p}"),
                    job,
                    borsh::to_vec(&Message::Adopt { part: p as u8, keys: keys[p * chunk..(p + 1) * chunk].to_vec() })
                        .unwrap(),
                    vec![job],
                    vec![],
                    time,
                );
                let ok = r.ok();
                reports.push(r);
                if !ok {
                    return reports;
                }
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

/// [`run_job`] in the layout a beacon node serves `update` in, with the honest witness.
fn job_for(w: &mut World, update: &LightClientUpdate, label: &str) -> Vec<TxReport> {
    let layout = w.inst.layout_for(&w.inst.store(&w.chain), update);
    let wit = witness(w, update);
    run_job(w, update, layout, &wit, label)
}

/// `Open` with raw bytes (a malformed encoding); the job actor named is the one the honest bytes
/// would open.
fn open_raw(w: &mut World, update_bytes: Vec<u8>, signers: Vec<u8>, honest: &LightClientUpdate, label: &str) -> TxReport {
    let inst = w.inst.clone();
    let store = inst.store(&w.chain);
    let jid = job_id(&inst.id, &store.digest(), &CompactUpdate::of(honest).digest(LcFork::Electra));
    let job = job_actor(inst.program, &inst.id, &jid);
    let time = time_of(w, honest);
    w.chain.submit(
        label,
        inst.actor(),
        borsh::to_vec(&Message::Open { update: update_bytes, signers }).unwrap(),
        vec![inst.actor(), job],
        vec![],
        time,
    )
}

/// `Init` of `config` at `checkpoint` with `bootstrap_bytes`, as one transaction; the adoption job
/// actor is named if the inputs verify natively.
fn init_raw(
    chain: &mut harness::Chain,
    config: &Config,
    checkpoint: [u8; 32],
    bootstrap: Option<&LightClientBootstrap>,
    bootstrap_bytes: Vec<u8>,
    time: u64,
    label: &str,
) -> TxReport {
    let id = instance_id(config, &checkpoint);
    let inst = instance_actor(ferrata_account(), &id);
    let mut actors = vec![inst];
    if let (Some(b), Some(fork)) = (bootstrap, config.fork()) {
        if let Ok((_, job)) =
            steps::initialize(&checkpoint, &CompactBootstrap::of(b), fork, &config.spec(), config.adoption_parts)
        {
            actors.push(job_actor(ferrata_account(), &id, &job_id(&id, &job.store_digest, &job.update_digest)));
        }
    }
    chain.submit(
        label,
        inst,
        borsh::to_vec(&Message::Init { config: config.clone(), checkpoint, bootstrap: bootstrap_bytes }).unwrap(),
        actors,
        vec![PublicAccountEvidence::Pda { program: ferrata_account(), seed: instance_seed(&id) }],
        time,
    )
}

/// The time of the base world's Init (12 s into the bootstrap's slot).
fn init_time(w: &World) -> u64 {
    w.inst.config.slot_start_ms(w.bootstrap.header.beacon.slot) + 12_000
}

/// A fresh mainnet world with Init sent but the committee not adopted, and the adoption job actor.
fn init_only() -> (World, Actor, u64) {
    let w0 = World::new(2);
    let mut w = w0.clone();
    // Re-create the chain from scratch: a new chain with only the Init transaction.
    w.chain = harness::Chain::new();
    let compact = CompactBootstrap::of(&w.bootstrap);
    let (_, job) = steps::initialize(&w.inst.checkpoint, &compact, LcFork::Electra, &w.spec, 2).unwrap();
    let jid = job_id(&w.inst.id, &job.store_digest, &job.update_digest);
    let job = job_actor(w.inst.program, &w.inst.id, &jid);
    let time = init_time(&w);
    let r = w.chain.submit(
        "Init only",
        w.inst.actor(),
        borsh::to_vec(&Message::Init {
            config: w.inst.config.clone(),
            checkpoint: w.inst.checkpoint,
            bootstrap: compact.encode(LcFork::Electra),
        })
        .unwrap(),
        vec![w.inst.actor(), job],
        vec![PublicAccountEvidence::Pda { program: w.inst.program, seed: instance_seed(&w.inst.id) }],
        time,
    );
    assert!(r.ok(), "Init: {:?}", r.charge.revert);
    (w, job, time)
}

/// An Adopt transaction at `job`; the last part also lists the instance and the bootstrap record.
fn adopt(w: &mut World, job: Actor, part: u8, keys: Vec<[u8; 96]>, last: bool, time: u64, label: &str) -> TxReport {
    let mut actors = vec![job];
    if last {
        actors.extend([w.inst.actor(), header_actor(w.inst.program, &w.inst.id, w.bootstrap.header.beacon.slot)]);
    }
    w.chain.submit(label, job, borsh::to_vec(&Message::Adopt { part, keys }).unwrap(), actors, vec![], time)
}

/// An actor of LEZ's `scripted` test program (another program's actor) sending `message` to `to`.
fn scripted(w: &mut World, label: &str, to: Actor, message: &Message, target_seed: Option<lee_core::program::PdaSeed>) -> TxReport {
    let seed = lee_core::program::PdaSeed::new([7; 32]);
    let attacker = Actor::new(AccountId::for_public_pda(&scripted_account(), &seed), scripted_account());
    let mut evidence = vec![PublicAccountEvidence::Pda { program: scripted_account(), seed }];
    if let Some(s) = target_seed {
        evidence.push(PublicAccountEvidence::Pda { program: w.inst.program, seed: s });
    }
    let script = Script::default().send_sync(to, message);
    let time = time_of(w, &w.updates[1].clone());
    w.chain.submit(label, attacker, borsh::to_vec(&script).unwrap(), vec![attacker, to], evidence, time)
}

/// The synthetic chain (B-LC-SYN): a mainnet-preset instance whose 512-member committee the test
/// holds, initialised from a fabricated bootstrap at `slot0` (period 1899), next to the native store.
struct Synthetic {
    w: World,
    fake: ferrata_spectests::testkit::FakeCommittee,
    slot0: u64,
}

fn synthetic(config: Config, seed: u8) -> Synthetic {
    let fake = fake_committee(512, seed);
    let slot0 = EXPECTED_FINALIZED[0].0 + 64;
    let mut header = fabricated_header(slot0, 9_000);
    let (state_root, branches) =
        sparse_tree(&[(CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA, fake.committee.hash_tree_root())], 9_100);
    header.beacon.state_root = state_root;
    let bootstrap = LightClientBootstrap {
        header,
        current_sync_committee: fake.committee.clone(),
        current_sync_committee_branch: branches[0].clone(),
    };
    let checkpoint = bootstrap.header.beacon.hash_tree_root();
    let w = World::from_parts(config, checkpoint, bootstrap, vec![]);
    Synthetic { w, fake, slot0 }
}

impl Synthetic {
    fn ctx(&self) -> Context<'_> {
        Context { spec: &self.w.spec, genesis_validators_root: self.w.inst.config.genesis_validators_root, bls: &BLS }
    }

    /// A fabricated update in the store's period: finalized at slot0 + 64, attested at slot0 + 96,
    /// signed at slot0 + 97 by the first `signers` members.
    fn update(&self, signers: usize, finality: bool) -> LightClientUpdate {
        let f = Fabrication {
            finalized_slot: finality.then_some(self.slot0 + 64),
            attested_slot: self.slot0 + 96,
            signature_slot: self.slot0 + 97,
            signers,
            next_committee: None,
        };
        fabricated_update(&f, &self.fake, &self.ctx())
    }

    fn run(&mut self, update: &LightClientUpdate, label: &str) -> Vec<TxReport> {
        let wit = SignerWitness::build(&self.fake.committee, &update.sync_aggregate.bits).unwrap();
        run_job(&mut self.w, update, LcFork::Electra, &wit, label)
    }
}

/// Flip one bit of a 32-byte root.
fn flip(r: &mut [u8; 32]) {
    r[0] ^= 1;
}

fn payload_mut(h: &mut LightClientHeader) -> &mut ferrata_core::types::ExecutionPayloadHeader {
    match &mut h.execution {
        Execution::Payload { header, .. } => header,
        other => panic!("not a payload header: {other:?}"),
    }
}

fn exec_branch_mut(h: &mut LightClientHeader) -> &mut Vec<[u8; 32]> {
    match &mut h.execution {
        Execution::Payload { branch, .. } | Execution::BlockHash { branch, .. } => branch,
        Execution::None => panic!("no execution branch"),
    }
}

// ---------------------------------------------------------------------------------------------
// Signatures
// ---------------------------------------------------------------------------------------------

#[test]
fn lc_signatures() {
    let mut r = Results::new(FILE, "sig");
    let b = base();

    // A mutation of U's signature, sent as a whole job.
    let with_signature = |sig: [u8; 96], label: &str| {
        let mut w = b.clone();
        let mut x = u(&w);
        x.sync_aggregate.signature = sig;
        Observed::of_reports(&job_for(&mut w, &x, label), label)
    };
    let honest_sig = u(&b).sync_aggregate.signature;

    // LC-SIG-01: update_2's (valid, canonical) signature.
    let other = b.updates[2].sync_aggregate.signature;
    r.run("LC-SIG-01", || with_signature(other, "update_2's signature on U"));
    // LC-SIG-02: lowest bit of the last byte flipped.
    let mut s = honest_sig;
    s[95] ^= 1;
    r.run("LC-SIG-02", || with_signature(s, "last bit flipped"));
    // LC-SIG-03: the compressed point at infinity, signers unchanged.
    let mut inf = [0u8; 96];
    inf[0] = 0xc0;
    r.run("LC-SIG-03", || with_signature(inf, "infinity signature"));
    // LC-SIG-04: compression flag cleared.
    let mut s = honest_sig;
    s[0] &= 0x7f;
    r.run("LC-SIG-04", || with_signature(s, "compression flag cleared"));
    // LC-SIG-05: x >= p (flags kept: compressed, not infinity, sign bit as it was).
    let mut s = [0xffu8; 96];
    s[0] = 0x80 | (honest_sig[0] & 0x20) | 0x1f;
    r.run("LC-SIG-05", || with_signature(s, "x coordinate above the modulus"));
    // LC-SIG-06: x = (c0 = 2, c1 = 0): on E2 (the norm (2^3+4)^2+16 = 160 is a square mod p,
    // computed for this test), and not in G2 (cofactor ~2^381; signature_paths.rs uses the same family).
    let mut s = [0u8; 96];
    s[0] = 0x80;
    s[95] = 2;
    r.run("LC-SIG-06", || with_signature(s, "point of E2 outside G2"));
    // LC-SIG-07, LC-SIG-09: synthetic chain (lc_synthetic). LC-SIG-08: another chain's instance.
    r.run("LC-SIG-08", || {
        let mut w = b.clone();
        let mut config = mainnet_config(2);
        config.genesis_validators_root = [0x42; 32];
        let other = Instance::new(ferrata_account(), config, w.inst.checkpoint);
        let init = other.init(&mut w.chain, &w.bootstrap.clone(), "other chain's instance");
        assert!(init.reports.iter().all(TxReport::ok));
        // The other instance's next update is update_0 (it supplies the next committee); the
        // witness comes from a store that has not seen update_0 (the bootstrap's committees).
        let u0 = w.updates[0].clone();
        let full = ferrata_core::policy::FinalizedStore::initialize_with(
            &w.inst.checkpoint,
            &w.bootstrap,
            LcFork::Electra,
            &w.spec,
            &BLS,
            ferrata_core::policy::Adoption::OnChain,
        )
        .unwrap();
        let run = other.update_in(&mut w.chain, &u0, LcFork::Electra, full.core(), "update_0 at the other chain's instance");
        Observed::of_reports(&run.reports, "update_0 sent to an instance with genesis_validators_root 0x42…")
    });
    // LC-SIG-10..12: the signer witness tampered.
    let committee = b.native.core().next_sync_committee.clone();
    let honest = witness(&b, &u(&b));
    assert!(honest.keys.len() >= 2, "U has at least two non-signers");
    r.run("LC-SIG-10", || {
        let mut w = b.clone();
        let mut wit = honest.clone();
        // A valid G1 key of another committee (period 1902's), not in the signing committee.
        let outsider = uncompressed_keys(&w.updates[2].next_sync_committee).unwrap()[3];
        wit.keys[0] = outsider;
        let x = u(&w);
        Observed::of_reports(&run_job(&mut w, &x, LcFork::Electra, &wit, "witness key replaced"), "witness key 0 replaced")
    });
    r.run("LC-SIG-11", || {
        let mut w = b.clone();
        let mut wit = honest.clone();
        wit.keys.pop();
        wit.branches.pop();
        let x = u(&w);
        Observed::of_reports(&run_job(&mut w, &x, LcFork::Electra, &wit, "witness entry dropped"), "last non-signer dropped")
    });
    r.run("LC-SIG-12", || {
        let mut w = b.clone();
        let mut wit = honest.clone();
        wit.keys[1] = wit.keys[0];
        wit.branches[1] = wit.branches[0].clone();
        let x = u(&w);
        Observed::of_reports(
            &run_job(&mut w, &x, LcFork::Electra, &wit, "witness entry duplicated"),
            "non-signer 0 listed twice, non-signer 1 dropped",
        )
    });
    r.run("LC-SIG-13", || {
        let mut w = b.clone();
        let mut x = u(&w);
        let i = x.sync_aggregate.bits.iter().position(|b| !*b).expect("a non-signer");
        x.sync_aggregate.bits[i] = true;
        let wit = SignerWitness::build(&committee, &x.sync_aggregate.bits).unwrap();
        Observed::of_reports(&run_job(&mut w, &x, LcFork::Electra, &wit, "bit set for a non-signer"), &format!("bit {i} set"))
    });
    r.run("LC-SIG-14", || {
        let mut w = b.clone();
        let mut x = u(&w);
        let i = x.sync_aggregate.bits.iter().position(|b| *b).expect("a signer");
        x.sync_aggregate.bits[i] = false;
        let wit = SignerWitness::build(&committee, &x.sync_aggregate.bits).unwrap();
        Observed::of_reports(&run_job(&mut w, &x, LcFork::Electra, &wit, "bit cleared for a signer"), &format!("bit {i} cleared"))
    });
    r.finish(
        &["lc"],
        &["LC-SIG-01", "LC-SIG-02", "LC-SIG-03", "LC-SIG-04", "LC-SIG-05", "LC-SIG-06", "LC-SIG-08", "LC-SIG-10", "LC-SIG-11", "LC-SIG-12", "LC-SIG-13", "LC-SIG-14"],
    );
}

// ---------------------------------------------------------------------------------------------
// Participation, slots and periods
// ---------------------------------------------------------------------------------------------

#[test]
fn lc_participation_and_slots() {
    let mut r = Results::new(FILE, "part-slot");
    let b = base();

    // LC-PART-01: no participants, the infinity signature.
    r.run("LC-PART-01", || {
        let mut w = b.clone();
        let mut x = u(&w);
        x.sync_aggregate.bits = vec![false; 512];
        x.sync_aggregate.signature = {
            let mut s = [0u8; 96];
            s[0] = 0xc0;
            s
        };
        let wit = SignerWitness::build(&w.native.core().next_sync_committee, &x.sync_aggregate.bits).unwrap();
        Observed::of_reports(&run_job(&mut w, &x, LcFork::Electra, &wit, "no participants"), "0 of 512")
    });
    // LC-PART-06: the participation bitvector one byte too long (520 bits).
    r.run("LC-PART-06", || {
        let mut w = b.clone();
        let x = u(&w);
        let bytes = CompactUpdate::of(&x).encode(LcFork::Electra);
        let agg = ferrata_core::witness::encode_sync_aggregate(&x.sync_aggregate);
        let at = bytes.windows(agg.len()).position(|win| win == agg.as_slice());
        let Some(at) = at else {
            return Observed::Unsupported { reason: "the sync aggregate's SSZ is not contiguous in the encoding".into() };
        };
        let mut mutated = bytes.clone();
        mutated.insert(at + 64, 0);
        let wit = witness(&w, &x).encode();
        Observed::of_report(&open_raw(&mut w, mutated, wit, &x, "bitvector of 65 bytes"), "a zero byte inserted after the 64 bit bytes")
    });
    // LC-SLOT-01, 02: signature slot equal to / below the attested slot (same fork version).
    for (id, delta, label) in [("LC-SLOT-01", 0i64, "signature_slot = attested slot"), ("LC-SLOT-02", -1, "signature_slot = attested - 1")] {
        r.run(id, || {
            let mut w = b.clone();
            let mut x = u(&w);
            x.signature_slot = (x.attested_header.beacon.slot as i64 + delta) as u64;
            Observed::of_reports(&job_for(&mut w, &x, label), label)
        });
    }
    // LC-SLOT-04: Open before the signature slot starts.
    r.run("LC-SLOT-04", || {
        let mut w = b.clone();
        let x = u(&w);
        let bytes = CompactUpdate::of(&x).encode(LcFork::Electra);
        let wit = witness(&w, &x).encode();
        let inst = w.inst.clone();
        let store = inst.store(&w.chain);
        let jid = job_id(&inst.id, &store.digest(), &CompactUpdate::of(&x).digest(LcFork::Electra));
        let job = job_actor(inst.program, &inst.id, &jid);
        let early = inst.config.slot_start_ms(x.signature_slot) - 1;
        let rep = w.chain.submit(
            "Open 1 ms before the signature slot",
            inst.actor(),
            borsh::to_vec(&Message::Open { update: bytes, signers: wit }).unwrap(),
            vec![inst.actor(), job],
            vec![],
            early,
        );
        Observed::of_report(&rep, "timestamp = slot start - 1 ms")
    });
    // LC-SLOT-05: signed two periods ahead of the store.
    r.run("LC-SLOT-05", || {
        let mut w = b.clone();
        let mut x = u(&w);
        x.signature_slot += 2 * 8192;
        Observed::of_reports(&job_for(&mut w, &x, "signature two periods ahead"), "signature_slot + 2 periods")
    });
    // LC-SLOT-06: next-period signature with the next committee unknown (no update_0).
    r.run("LC-SLOT-06", || {
        let mut w = World::new(2);
        let x = as_finality_update(&w.updates[1]);
        let wit = SignerWitness::build(&w.updates[0].next_sync_committee, &x.sync_aggregate.bits).unwrap();
        Observed::of_reports(&run_job(&mut w, &x, LcFork::Electra, &wit, "next period, next unknown"), "store right after Init")
    });
    // LC-SLOT-07: an update of an older period than the store.
    r.run("LC-SLOT-07", || {
        let mut w = b.clone();
        let u1 = w.updates[1].clone();
        w.apply(&u1, "update_1 (rotation)");
        let u0 = w.updates[0].clone();
        let wit = SignerWitness::build(&w.bootstrap.current_sync_committee, &u0.sync_aggregate.bits).unwrap();
        Observed::of_reports(&run_job(&mut w, &u0, LcFork::Electra, &wit, "period-1899 update after rotation"), "update_0 after the store moved to period 1900")
    });
    r.finish(&["lc"], &["LC-PART-01", "LC-PART-06", "LC-SLOT-01", "LC-SLOT-02", "LC-SLOT-04", "LC-SLOT-05", "LC-SLOT-06", "LC-SLOT-07"]);
}

// ---------------------------------------------------------------------------------------------
// Newness, replays, concurrency
// ---------------------------------------------------------------------------------------------

#[test]
fn lc_newness_and_replays() {
    let mut r = Results::new(FILE, "new");
    let b = base();

    // LC-NEW-01: replay of an applied update.
    r.run("LC-NEW-01", || {
        let mut w = b.clone();
        let x = u(&w);
        let first = job_for(&mut w, &x, "U");
        assert!(first.iter().all(TxReport::ok), "U applies: {:?}", first.last());
        let wit = witness(&b, &x);
        Observed::of_reports(&run_job(&mut w, &x, LcFork::Electra, &wit, "U again"), "U sent again after it applied")
    });
    // LC-NEW-02: a stale update (the store advanced by two newer updates).
    r.run("LC-NEW-02", || {
        let mut w = b.clone();
        let (u1, u2) = (w.updates[1].clone(), w.updates[2].clone());
        w.apply(&u1, "update_1");
        w.apply(&u2, "update_2");
        let x = u(&b);
        let wit = witness(&b, &x);
        Observed::of_reports(&run_job(&mut w, &x, LcFork::Electra, &wit, "stale U"), "U (period 1900) after update_2 (store period 1901)")
    });
    // LC-NEW-03: duplicate Open.
    r.run("LC-NEW-03", || {
        let mut w = b.clone();
        let x = u(&w);
        let bytes = CompactUpdate::of(&x).encode(LcFork::Electra);
        let wit = witness(&w, &x).encode();
        let first = open_raw(&mut w, bytes.clone(), wit.clone(), &x, "Open U");
        assert!(first.ok());
        Observed::of_report(&open_raw(&mut w, bytes, wit, &x, "Open U again"), "the second Open of the same job")
    });
    // LC-NEW-04, 05: two valid jobs on one store; the later one finished second, then reopened.
    let (new04, new05) = {
        let mut w = b.clone();
        let fin = u(&w); // update_1 without its next committee
        let full = w.updates[1].clone(); // update_1 with it (a different digest, so a different job)
        let inst = w.inst.clone();
        let store = inst.store(&w.chain);
        let time = time_of(&w, &full);
        let wit_full = witness(&w, &full);
        let jid_full = job_id(&inst.id, &store.digest(), &CompactUpdate::of(&full).digest(LcFork::Electra));
        let job_full = job_actor(inst.program, &inst.id, &jid_full);
        let open_full = w.chain.submit(
            "Open update_1 (with next committee)",
            inst.actor(),
            borsh::to_vec(&Message::Open { update: CompactUpdate::of(&full).encode(LcFork::Electra), signers: wit_full.encode() })
                .unwrap(),
            vec![inst.actor(), job_full],
            vec![],
            time,
        );
        assert!(open_full.ok(), "{:?}", open_full.charge.revert);
        let fin_run = job_for(&mut w, &fin, "U (finality)");
        assert!(fin_run.iter().all(TxReport::ok), "U applies first");
        let mut later = vec![w.chain.submit("Miller (stale job)", job_full, borsh::to_vec(&Message::Miller).unwrap(), vec![job_full], vec![], time)];
        if later[0].ok() {
            let keys = uncompressed_keys(&full.next_sync_committee).unwrap();
            for p in 0..2usize {
                let rep = w.chain.submit(
                    &format!("Adopt {p} (stale job)"),
                    job_full,
                    borsh::to_vec(&Message::Adopt { part: p as u8, keys: keys[p * 256..(p + 1) * 256].to_vec() }).unwrap(),
                    vec![job_full],
                    vec![],
                    time,
                );
                let ok = rep.ok();
                later.push(rep);
                if !ok {
                    break;
                }
            }
        }
        if later.iter().all(TxReport::ok) {
            let header = header_actor(inst.program, &inst.id, full.finalized_header.beacon.slot);
            later.push(w.chain.submit(
                "Finish (stale job)",
                job_full,
                borsh::to_vec(&Message::Finish).unwrap(),
                vec![job_full, inst.actor(), header],
                vec![],
                time,
            ));
        }
        let new04 = Observed::of_reports(&later, "update_1's job opened on the pre-U store, finished after U applied");
        // LC-NEW-05: update_1 reopened on the advanced store.
        let again = job_for(&mut w, &full, "update_1 reopened");
        let new05 = Observed::of_reports(&again, "update_1 opened again on the store U advanced");
        (new04, new05)
    };
    r.run("LC-NEW-04", || new04);
    r.run("LC-NEW-05", || new05);
    // LC-NEW-06: VerifyAll against a stale store snapshot.
    let verify_all = |w: &mut World, store: Vec<u8>, x: &LightClientUpdate, adoption: Vec<[u8; 96]>, label: &str| {
        let wit = witness(w, x);
        let message = Message::VerifyAll {
            config: w.inst.config.clone(),
            checkpoint: w.inst.checkpoint,
            store,
            update: CompactUpdate::of(x).encode(LcFork::Electra),
            signers: wit.encode(),
            adoption,
        };
        let verifier = Actor::new(w.chain.payer, ferrata_account());
        let header = header_actor(w.inst.program, &w.inst.id, x.finalized_header.beacon.slot);
        let time = time_of(w, x);
        w.chain.submit(label, verifier, borsh::to_vec(&message).unwrap(), vec![verifier, w.inst.actor(), header], vec![], time)
    };
    r.run("LC-NEW-06", || {
        let mut w = b.clone();
        let stale = w.inst.store(&w.chain).encode();
        let x = u(&w);
        let applied = job_for(&mut w, &x, "U");
        assert!(applied.iter().all(TxReport::ok));
        let full = w.updates[1].clone();
        let keys = uncompressed_keys(&full.next_sync_committee).unwrap();
        // The witness from the pre-U committees (what the stale prover holds).
        let wit_store = b.clone();
        let wit = witness(&wit_store, &full);
        let message = Message::VerifyAll {
            config: w.inst.config.clone(),
            checkpoint: w.inst.checkpoint,
            store: stale,
            update: CompactUpdate::of(&full).encode(LcFork::Electra),
            signers: wit.encode(),
            adoption: keys,
        };
        let verifier = Actor::new(w.chain.payer, ferrata_account());
        let header = header_actor(w.inst.program, &w.inst.id, full.finalized_header.beacon.slot);
        let time = time_of(&w, &full);
        let rep = w.chain.submit("VerifyAll on a stale store", verifier, borsh::to_vec(&message).unwrap(), vec![verifier, w.inst.actor(), header], vec![], time);
        Observed::of_report(&rep, "update_1 verified against the pre-U store after U applied")
    });
    // LC-NEW-07: VerifyAll replayed.
    r.run("LC-NEW-07", || {
        let mut w = b.clone();
        let x = u(&w);
        let store = w.inst.store(&w.chain).encode();
        let first = verify_all(&mut w, store.clone(), &x, vec![], "VerifyAll U");
        assert!(first.ok(), "{:?}", first.charge.revert);
        Observed::of_report(&verify_all(&mut w, store, &x, vec![], "VerifyAll U again"), "the same VerifyAll in a second transaction")
    });
    r.finish(&["lc"], &["LC-NEW-"]);
}

// ---------------------------------------------------------------------------------------------
// Finality and headers
// ---------------------------------------------------------------------------------------------

#[test]
fn lc_finality_and_headers() {
    let mut r = Results::new(FILE, "fin");
    let b = base();
    let mutated = |f: &dyn Fn(&mut LightClientUpdate), label: &str| {
        let mut w = b.clone();
        let mut x = u(&w);
        f(&mut x);
        Observed::of_reports(&job_for(&mut w, &x, label), label)
    };
    // LC-FIN-01: synthetic (lc_synthetic).
    r.run("LC-FIN-02", || mutated(&|x| flip(&mut x.finality_branch[0]), "finality branch node 0 flipped"));
    r.run("LC-FIN-03", || mutated(&|x| { x.finality_branch.pop(); }, "finality branch one node short"));
    r.run("LC-FIN-04", || {
        let older = b.bootstrap.header.beacon.clone();
        mutated(&move |x| x.finalized_header.beacon = older.clone(), "finalized beacon header replaced by the checkpoint's")
    });
    r.run("LC-FIN-05", || mutated(&|x| flip(&mut payload_mut(&mut x.finalized_header).state_root), "finalized execution state root"));
    r.run("LC-FIN-06", || mutated(&|x| flip(&mut payload_mut(&mut x.finalized_header).receipts_root), "finalized execution receipts root"));
    r.run("LC-FIN-07", || mutated(&|x| payload_mut(&mut x.finalized_header).block_number += 1, "finalized execution block number + 1"));
    r.run("LC-FIN-08", || mutated(&|x| flip(&mut payload_mut(&mut x.finalized_header).block_hash), "finalized execution block hash"));
    r.run("LC-FIN-09", || mutated(&|x| payload_mut(&mut x.finalized_header).timestamp += 12, "finalized execution timestamp + 12"));
    r.run("LC-FIN-10", || mutated(&|x| flip(&mut exec_branch_mut(&mut x.finalized_header)[0]), "finalized execution branch node 0"));
    r.run("LC-FIN-11", || mutated(&|x| flip(&mut payload_mut(&mut x.attested_header).state_root), "attested execution state root"));
    r.run("LC-FIN-12", || mutated(&|x| x.finalized_header = LightClientHeader::empty(LcFork::Electra), "empty finalized header, real finality branch"));
    r.finish(&["lc"], &["LC-FIN-02", "LC-FIN-03", "LC-FIN-04", "LC-FIN-05", "LC-FIN-06", "LC-FIN-07", "LC-FIN-08", "LC-FIN-09", "LC-FIN-10", "LC-FIN-11", "LC-FIN-12"]);
}

// ---------------------------------------------------------------------------------------------
// Committees and handoffs
// ---------------------------------------------------------------------------------------------

#[test]
fn lc_committees() {
    let mut r = Results::new(FILE, "com");
    let b = base();
    // update_1 as served carries the next committee (a rotation that adopts one).
    let rotation = |f: &dyn Fn(&mut LightClientUpdate, &World), label: &str| {
        let mut w = b.clone();
        let mut x = w.updates[1].clone();
        f(&mut x, &w);
        Observed::of_reports(&job_for(&mut w, &x, label), label)
    };
    r.run("LC-COM-01", || rotation(&|x, w| x.next_sync_committee = w.native.core().current_sync_committee.clone(), "next committee = the current one"));
    r.run("LC-COM-02", || rotation(&|x, _| flip(&mut x.next_sync_committee_branch[0]), "next committee branch node 0"));
    r.run("LC-COM-03", || rotation(&|x, _| x.next_sync_committee.aggregate_pubkey[47] ^= 1, "next committee aggregate key byte 47"));
    // LC-COM-04: synthetic. LC-COM-05..11: the Init adoption job.
    let adoption = |mutate: &dyn Fn(&mut Vec<[u8; 96]>, &mut Vec<[u8; 96]>) -> Vec<(u8, bool)>, label: &str| {
        let (mut w, job, time) = init_only();
        let keys = uncompressed_keys(&w.bootstrap.current_sync_committee).unwrap();
        let (mut p0, mut p1) = (keys[..256].to_vec(), keys[256..].to_vec());
        let order = mutate(&mut p0, &mut p1);
        let mut reports = Vec::new();
        for (part, last) in order {
            let keys = if part == 0 { p0.clone() } else { p1.clone() };
            let rep = adopt(&mut w, job, part, keys, last, time, &format!("{label}: Adopt {part}"));
            let ok = rep.ok();
            reports.push(rep);
            if !ok {
                break;
            }
        }
        Observed::of_reports(&reports, label)
    };
    r.run("LC-COM-05", || {
        let outsider = uncompressed_keys(&b.updates[2].next_sync_committee).unwrap()[3];
        adoption(&move |p0, _| { p0[7] = outsider; vec![(0, false), (1, true)] }, "key 7 replaced")
    });
    r.run("LC-COM-06", || adoption(&|p0, _| { p0.swap(0, 1); vec![(0, false), (1, true)] }, "keys 0 and 1 swapped"));
    r.run("LC-COM-07", || adoption(&|p0, _| { p0[3] = IDENTITY_UNCOMPRESSED; vec![(0, false), (1, true)] }, "key 3 the identity"));
    r.run("LC-COM-08", || adoption(&|p0, _| { p0[3][95] ^= 1; vec![(0, false), (1, true)] }, "key 3's y changed"));
    r.run("LC-COM-09", || adoption(&|p0, _| { p0.pop(); vec![(0, false)] }, "part 0 with 255 keys"));
    r.run("LC-COM-10", || adoption(&|_, _| vec![(0, false), (0, false)], "part 0 twice"));
    r.run("LC-COM-11", || adoption(&|_, _| vec![(1, true)], "part 1 first"));
    // LC-COM-12: Finish of a rotation before its adoption.
    r.run("LC-COM-12", || {
        let mut w = b.clone();
        let x = w.updates[1].clone();
        let inst = w.inst.clone();
        let store = inst.store(&w.chain);
        let wit = witness(&w, &x);
        let jid = job_id(&inst.id, &store.digest(), &CompactUpdate::of(&x).digest(LcFork::Electra));
        let job = job_actor(inst.program, &inst.id, &jid);
        let time = time_of(&w, &x);
        let open = w.chain.submit("Open update_1", inst.actor(), borsh::to_vec(&Message::Open { update: CompactUpdate::of(&x).encode(LcFork::Electra), signers: wit.encode() }).unwrap(), vec![inst.actor(), job], vec![], time);
        assert!(open.ok());
        let miller = w.chain.submit("Miller", job, borsh::to_vec(&Message::Miller).unwrap(), vec![job], vec![], time);
        assert!(miller.ok());
        let header = header_actor(inst.program, &inst.id, x.finalized_header.beacon.slot);
        let fin = w.chain.submit("Finish before adoption", job, borsh::to_vec(&Message::Finish).unwrap(), vec![job, inst.actor(), header], vec![], time);
        Observed::of_report(&fin, "Finish with the adoption not done")
    });
    // LC-COM-13: Adopt on a job that adopts nothing.
    r.run("LC-COM-13", || {
        let mut w = b.clone();
        let x = u(&w);
        let inst = w.inst.clone();
        let store = inst.store(&w.chain);
        let wit = witness(&w, &x);
        let jid = job_id(&inst.id, &store.digest(), &CompactUpdate::of(&x).digest(LcFork::Electra));
        let job = job_actor(inst.program, &inst.id, &jid);
        let time = time_of(&w, &x);
        let open = w.chain.submit("Open U", inst.actor(), borsh::to_vec(&Message::Open { update: CompactUpdate::of(&x).encode(LcFork::Electra), signers: wit.encode() }).unwrap(), vec![inst.actor(), job], vec![], time);
        assert!(open.ok());
        let miller = w.chain.submit("Miller", job, borsh::to_vec(&Message::Miller).unwrap(), vec![job], vec![], time);
        assert!(miller.ok());
        let keys = uncompressed_keys(&w.native.core().current_sync_committee).unwrap()[..256].to_vec();
        let rep = w.chain.submit("Adopt on a finality job", job, borsh::to_vec(&Message::Adopt { part: 0, keys }).unwrap(), vec![job], vec![], time);
        Observed::of_report(&rep, "Adopt part 0 at U's job")
    });
    // LC-COM-14: the fabricated chain, signed by 512 keys the test holds, sent to the mainnet instance.
    r.run("LC-COM-14", || {
        let mut w = b.clone();
        let fake = fake_committee(512, 0x5e);
        let s0 = w.inst.store(&w.chain).finalized.slot;
        let ctx = Context { spec: &w.spec, genesis_validators_root: w.inst.config.genesis_validators_root, bls: &BLS };
        let f = Fabrication { finalized_slot: Some(s0 + 64), attested_slot: s0 + 96, signature_slot: s0 + 97, signers: 512, next_committee: None };
        let x = fabricated_update(&f, &fake, &ctx);
        let wit = witness(&w, &x);
        Observed::of_reports(&run_job(&mut w, &x, LcFork::Electra, &wit, "fabricated chain"), "self-consistent update of a chain that never existed, 512 foreign signers")
    });
    r.finish(&["lc"], &["LC-COM-01", "LC-COM-02", "LC-COM-03", "LC-COM-05", "LC-COM-06", "LC-COM-07", "LC-COM-08", "LC-COM-09", "LC-COM-10", "LC-COM-11", "LC-COM-12", "LC-COM-13", "LC-COM-14"]);
}

// ---------------------------------------------------------------------------------------------
// Init and checkpoints
// ---------------------------------------------------------------------------------------------

#[test]
fn lc_init() {
    let mut r = Results::new(FILE, "init");
    let b = World::new(2);
    let t = init_time(&b);
    let fresh = |config: &Config, checkpoint: [u8; 32], bootstrap: &LightClientBootstrap, bytes: Vec<u8>, time: u64, label: &str| {
        let mut chain = harness::Chain::new();
        Observed::of_report(&init_raw(&mut chain, config, checkpoint, Some(bootstrap), bytes, time, label), label)
    };
    let config = b.inst.config.clone();
    let cp = b.inst.checkpoint;
    let boot = b.bootstrap.clone();
    let enc = |bt: &LightClientBootstrap| CompactBootstrap::of(bt).encode(LcFork::Electra);

    r.run("LC-INIT-01", || fresh(&config, root(EXPECTED_FINALIZED[1].1), &boot, enc(&boot), t, "checkpoint of another block"));
    r.run("LC-INIT-02", || {
        let mut bt = boot.clone();
        flip(&mut bt.current_sync_committee_branch[0]);
        fresh(&config, cp, &bt, enc(&bt), t, "committee branch node 0")
    });
    r.run("LC-INIT-03", || {
        let mut bt = boot.clone();
        bt.current_sync_committee = b.updates[0].next_sync_committee.clone();
        fresh(&config, cp, &bt, enc(&bt), t, "committee replaced by period 1900's")
    });
    r.run("LC-INIT-04", || {
        let mut bt = boot.clone();
        flip(&mut payload_mut(&mut bt.header).state_root);
        fresh(&config, cp, &bt, enc(&bt), t, "bootstrap execution state root")
    });
    r.run("LC-INIT-05", || {
        let mut w = b.clone();
        let rep = init_raw(&mut w.chain, &config, cp, Some(&boot), enc(&boot), t, "Init again");
        Observed::of_report(&rep, "the same Init after the instance exists")
    });
    r.run("LC-INIT-06", || {
        let mut c = config.clone();
        c.store_fork = 7;
        fresh(&c, cp, &boot, enc(&boot), t, "store_fork = 7")
    });
    r.run("LC-INIT-07", || {
        let mut c = config.clone();
        c.ws_period_epochs = 0;
        fresh(&c, cp, &boot, enc(&boot), t, "ws_period_epochs = 0")
    });
    r.run("LC-INIT-08", || {
        let mut c = config.clone();
        c.store_fork = 1;
        fresh(&c, cp, &boot, enc(&boot), t, "Electra-layout bootstrap with store_fork = 1 (Capella)")
    });
    r.run("LC-INIT-09", || {
        let end = config.ws_end_ms(boot.header.beacon.slot).expect("a bounded period");
        fresh(&config, cp, &boot, enc(&boot), end, "Init at the end of the bootstrap's weak-subjectivity period")
    });
    r.run("LC-INIT-10", || {
        let mut w = b.clone();
        let mut c = config.clone();
        c.chain_id = 11_155_111;
        let other = Instance::new(ferrata_account(), c, cp);
        assert_ne!(other.id, w.inst.id);
        let run = other.init(&mut w.chain, &boot, "second instance, chain id 11155111");
        let ok = run.reports.iter().all(TxReport::ok);
        let first_unchanged = w.inst.state(&w.chain).map(|s| s.active).unwrap_or(false);
        let other_active = other.state(&w.chain).map(|s| s.active).unwrap_or(false);
        if ok && !(first_unchanged && other_active) {
            return Observed::EffectMismatch { note: format!("first active {first_unchanged}, second active {other_active}") };
        }
        Observed::of_reports(&run.reports, "both instances active, ids differ")
    });
    r.run("LC-INIT-11", || fresh(&config, cp, &boot, vec![], t, "empty bootstrap bytes"));
    r.finish(&["lc"], &["LC-INIT-"]);
}

// ---------------------------------------------------------------------------------------------
// Steps and messages
// ---------------------------------------------------------------------------------------------

#[test]
fn lc_steps() {
    let mut r = Results::new(FILE, "step");
    let b = base();
    // U's job actor and its messages.
    let job_of = |w: &World| {
        let x = u(w);
        let store = w.inst.store(&w.chain);
        let jid = job_id(&w.inst.id, &store.digest(), &CompactUpdate::of(&x).digest(LcFork::Electra));
        (jid, job_actor(w.inst.program, &w.inst.id, &jid), time_of(w, &x))
    };
    let open = |w: &mut World| {
        let x = u(w);
        let wit = witness(w, &x).encode();
        let rep = open_raw(w, CompactUpdate::of(&x).encode(LcFork::Electra), wit, &x, "Open U");
        assert!(rep.ok(), "{:?}", rep.charge.revert);
    };
    let step = |w: &mut World, job: Actor, m: &Message, actors: Vec<Actor>, time: u64, label: &str| {
        w.chain.submit(label, job, borsh::to_vec(m).unwrap(), actors, vec![], time)
    };

    r.run("LC-STEP-01", || {
        let mut w = b.clone();
        let (_, job, time) = job_of(&w);
        open(&mut w);
        let ia = w.inst.actor();
        Observed::of_report(&step(&mut w, job, &Message::Finish, vec![job, ia], time, "Finish before Miller"), "")
    });
    r.run("LC-STEP-02", || {
        let mut w = b.clone();
        let (_, job, time) = job_of(&w);
        open(&mut w);
        assert!(step(&mut w, job, &Message::Miller, vec![job], time, "Miller").ok());
        Observed::of_report(&step(&mut w, job, &Message::Miller, vec![job], time, "Miller again"), "")
    });
    r.run("LC-STEP-03", || {
        let mut w = b.clone();
        let (_, job, time) = job_of(&w);
        let x = u(&w);
        let run = job_for(&mut w, &x, "U");
        assert!(run.iter().all(TxReport::ok));
        let header = header_actor(w.inst.program, &w.inst.id, x.finalized_header.beacon.slot);
        let ia = w.inst.actor();
        Observed::of_report(&step(&mut w, job, &Message::Finish, vec![job, ia, header], time, "Finish after apply"), "")
    });
    r.run("LC-STEP-04", || {
        let mut w = b.clone();
        let (jid, _, time) = job_of(&w);
        let mut config = mainnet_config(2);
        config.genesis_validators_root = [0x42; 32];
        let other = Instance::new(ferrata_account(), config, w.inst.checkpoint);
        let init = other.init(&mut w.chain, &w.bootstrap.clone(), "other instance");
        assert!(init.reports.iter().all(TxReport::ok));
        open(&mut w);
        let b_job = job_actor(other.program, &other.id, &jid);
        let rep = w.chain.submit(
            "Miller at the other instance's job address",
            b_job,
            borsh::to_vec(&Message::Miller).unwrap(),
            vec![b_job],
            vec![PublicAccountEvidence::Pda { program: other.program, seed: job_seed(&other.id, &jid) }],
            time,
        );
        Observed::of_report(&rep, "U's job id at another instance")
    });
    // A job the program never verified, for the internal messages.
    let fake_job = |w: &World| ferrata_core::steps::Job {
        store_digest: w.inst.store(&w.chain).digest(),
        update_digest: [0; 32],
        phase: ferrata_core::steps::Phase::Verified,
        adoption: None,
        pending: ferrata_core::steps::PendingApply::Update {
            finalized: ferrata_core::types::BeaconBlockHeader { slot: 99_999_999, ..Default::default() },
            next: None,
            record: ferrata_core::steps::HeaderRecord::of(&w.bootstrap.header, LcFork::Electra, &w.spec),
        },
    };
    r.run("LC-STEP-05", || {
        let mut w = b.clone();
        let m = Message::Apply { instance_id: w.inst.id, job_id: [7; 32], job: fake_job(&w).encode() };
        let (_, _, time) = job_of(&w);
        let a = w.inst.actor();
        Observed::of_report(&step(&mut w, a, &m, vec![a], time, "Apply as the transaction root"), "")
    });
    r.run("LC-STEP-06", || {
        let mut w = b.clone();
        let jid = [9u8; 32];
        let job = job_actor(w.inst.program, &w.inst.id, &jid);
        let m = Message::JobOpen { instance_id: w.inst.id, job_id: jid, committee_size: 512, job: fake_job(&w).encode() };
        let (_, _, time) = job_of(&w);
        let rep = w.chain.submit(
            "JobOpen as the transaction root",
            job,
            borsh::to_vec(&m).unwrap(),
            vec![job],
            vec![PublicAccountEvidence::Pda { program: w.inst.program, seed: job_seed(&w.inst.id, &jid) }],
            time,
        );
        Observed::of_report(&rep, "")
    });
    r.run("LC-STEP-07", || {
        let mut w = b.clone();
        let slot = 99_999_999;
        let header = header_actor(w.inst.program, &w.inst.id, slot);
        let record = ferrata_core::steps::HeaderRecord::of(&w.bootstrap.header, LcFork::Electra, &w.spec).encode();
        let m = Message::Record { instance_id: w.inst.id, slot, chain_id: 1, record, root_id: w.inst.id };
        let (_, _, time) = job_of(&w);
        let rep = w.chain.submit(
            "Record as the transaction root",
            header,
            borsh::to_vec(&m).unwrap(),
            vec![header],
            vec![PublicAccountEvidence::Pda { program: w.inst.program, seed: ferrata_lez_wire::header_seed(&w.inst.id, slot) }],
            time,
        );
        let written = w.inst.record(&w.chain, slot).is_some();
        if rep.ok() || written {
            return Observed::EffectMismatch { note: format!("record written: {written}") };
        }
        Observed::of_report(&rep, "an outsider's Record for an unwritten slot")
    });
    r.run("LC-STEP-08", || {
        let mut w = b.clone();
        let m = Message::ApplyVerified { instance_id: w.inst.id, job: fake_job(&w).encode() };
        let to = w.inst.actor();
        Observed::of_report(&scripted(&mut w, "ApplyVerified from another program", to, &m, None), "")
    });
    r.run("LC-STEP-09", || {
        let mut w = b.clone();
        let x = u(&w);
        let header = header_actor(w.inst.program, &w.inst.id, w.bootstrap.header.beacon.slot);
        let wit = witness(&w, &x).encode();
        let time = time_of(&w, &x);
        let m = Message::Open { update: CompactUpdate::of(&x).encode(LcFork::Electra), signers: wit };
        Observed::of_report(&w.chain.submit("Open at a header actor", header, borsh::to_vec(&m).unwrap(), vec![header], vec![], time), "")
    });
    r.run("LC-STEP-10", || {
        let (mut w, _, _) = init_only();
        let x = as_finality_update(&w.updates[0]);
        let wit = SignerWitness::build(&w.bootstrap.current_sync_committee, &x.sync_aggregate.bits).unwrap();
        Observed::of_report(&open_raw(&mut w, CompactUpdate::of(&x).encode(LcFork::Electra), wit.encode(), &x, "Open on an inactive instance"), "update_0 before the adoption")
    });
    r.run("LC-STEP-11", || {
        let mut w = b.clone();
        let x = u(&w);
        let fake_id = [0x77u8; 32];
        let at = instance_actor(w.inst.program, &fake_id);
        let wit = witness(&w, &x).encode();
        let time = time_of(&w, &x);
        let m = Message::Open { update: CompactUpdate::of(&x).encode(LcFork::Electra), signers: wit };
        let rep = w.chain.submit(
            "Open at an address with no instance",
            at,
            borsh::to_vec(&m).unwrap(),
            vec![at],
            vec![PublicAccountEvidence::Pda { program: w.inst.program, seed: instance_seed(&fake_id) }],
            time,
        );
        Observed::of_report(&rep, "")
    });
    r.run("LC-STEP-12", || {
        let mut w = b.clone();
        let (_, _, time) = job_of(&w);
        let a = w.inst.actor();
        Observed::of_report(&w.chain.submit("7 random bytes", a, vec![0xde, 0xad, 0xbe, 0xef, 0x01, 0x02, 0x03], vec![a], vec![], time), "")
    });
    r.run("LC-STEP-13", || {
        let mut w = b.clone();
        let x = u(&w);
        let mut bytes = CompactUpdate::of(&x).encode(LcFork::Electra);
        bytes.pop();
        let wit = witness(&w, &x).encode();
        Observed::of_report(&open_raw(&mut w, bytes, wit, &x, "update encoding truncated by one byte"), "")
    });
    r.run("LC-STEP-14", || {
        let mut w = b.clone();
        let (_, _, time) = job_of(&w);
        let a = w.inst.actor();
        Observed::of_report(&step(&mut w, a, &Message::Read, vec![a], time, "Read as the transaction root"), "")
    });
    r.finish(&["lc"], &["LC-STEP-"]);
}

// ---------------------------------------------------------------------------------------------
// Successors, forks and Gloas, weak subjectivity (Sepolia)
// ---------------------------------------------------------------------------------------------

fn pre_gloas_config() -> Config {
    let mut c = sepolia::sepolia_config(2);
    c.fork_epochs[Fork::Gloas as usize] = ferrata_core::fork::FAR_FUTURE_EPOCH;
    c
}

#[test]
fn lc_successors_and_forks() {
    let mut r = Results::new(FILE, "suc-gl");
    // The parent: pre-Gloas schedule, period 1378 applied.
    let mut parent_w = sepolia::sepolia_world(pre_gloas_config());
    let u1378 = sepolia::sepolia_update(FIRST_PERIOD);
    parent_w.apply(&u1378, "period 1378");
    let u1379 = sepolia::sepolia_update(GLOAS_PERIOD);
    let store = parent_w.inst.store(&parent_w.chain);
    let checkpoint = store.finalized.hash_tree_root();
    let now = parent_w.inst.config.slot_start_ms(u1379.signature_slot) + 12_000;
    let succeed = |w: &mut World, config: &Config, checkpoint: [u8; 32], label: &str| {
        let parent = w.inst.clone();
        let root_id = parent.state(&w.chain).unwrap().root_id;
        let id = successor_id(&root_id, &parent.id, config, &checkpoint);
        let successor = instance_actor(parent.program, &id);
        let rep = w.chain.submit(
            label,
            parent.actor(),
            borsh::to_vec(&Message::Succeed { config: config.clone(), checkpoint }).unwrap(),
            vec![parent.actor(), successor],
            vec![],
            now,
        );
        (rep, Instance { program: parent.program, config: config.clone(), checkpoint, id })
    };
    let with_gloas = sepolia::sepolia_config(2);
    let changed = |f: &dyn Fn(&mut Config), label: &str| {
        let mut w = parent_w.clone();
        let mut c = with_gloas.clone();
        f(&mut c);
        let (rep, _) = succeed(&mut w, &c, checkpoint, label);
        Observed::of_report(&rep, label)
    };
    r.run("LC-SUC-01", || changed(&|c| c.chain_id = 1, "Gloas added, chain id changed"));
    r.run("LC-SUC-02", || {
        let mut w = parent_w.clone();
        let (rep, _) = succeed(&mut w, &pre_gloas_config(), checkpoint, "the parent's own config");
        Observed::of_report(&rep, "")
    });
    r.run("LC-SUC-03", || changed(&|c| c.fork_versions[Fork::Electra as usize][3] ^= 1, "Gloas added, Electra version changed"));
    r.run("LC-SUC-04", || changed(&|c| c.fork_epochs[Fork::Gloas as usize] = c.fork_epochs[Fork::Fulu as usize] - 1, "Gloas before Fulu"));
    r.run("LC-SUC-05", || {
        let mut w = parent_w.clone();
        let older = sepolia::sepolia_bootstrap().header.beacon.hash_tree_root();
        let (rep, _) = succeed(&mut w, &with_gloas, older, "checkpoint = the bootstrap's (older) record");
        Observed::of_report(&rep, "")
    });
    r.run("LC-SUC-06", || changed(&|c| c.ws_period_epochs += 1, "Gloas added, ws period + 1"));
    r.run("LC-SUC-07", || {
        let mut w = parent_w.clone();
        let forged_id = successor_id(&w.inst.id, &w.inst.id, &with_gloas, &checkpoint);
        let m = Message::Spawn {
            root_id: w.inst.id,
            parent_id: w.inst.id,
            checkpoint,
            config: with_gloas.clone(),
            store: w.inst.store(&w.chain).encode(),
        };
        let at = instance_actor(w.inst.program, &forged_id);
        let rep = w.chain.submit(
            "Spawn sent directly",
            at,
            borsh::to_vec(&m).unwrap(),
            vec![at],
            vec![PublicAccountEvidence::Pda { program: w.inst.program, seed: instance_seed(&forged_id) }],
            now,
        );
        Observed::of_report(&rep, "")
    });
    r.run("LC-SUC-08", || {
        let mut w = parent_w.clone();
        let (rep, successor) = succeed(&mut w, &with_gloas, checkpoint, "Succeed adding Gloas");
        if !rep.ok() {
            return Observed::of_report(&rep, "");
        }
        let s = successor.state(&w.chain);
        match s {
            Some(st) if st.active && st.root_id == w.inst.id && st.parent_id == w.inst.id => {
                Observed::of_report(&rep, "successor active, root id = parent id")
            }
            other => Observed::EffectMismatch { note: format!("successor state {:?}", other.map(|s| (s.active, s.root_id, s.parent_id))) },
        }
    });

    // Gloas: an instance with Sepolia's schedule, period 1378 applied.
    let mut gw = sepolia::sepolia_world(sepolia::sepolia_config(2));
    gw.apply(&u1378, "period 1378");
    r.run("LC-GL-01", || {
        let mut w = gw.clone();
        let run = job_for(&mut w, &u1379, "first Gloas-era update");
        if !run.iter().all(TxReport::ok) {
            return Observed::of_reports(&run, "");
        }
        let upgrades: Vec<Vec<u8>> = run
            .iter()
            .flat_map(|t| t.events.iter().filter(|e| e.event.selector == UPGRADED_SELECTOR).map(|e| e.event.data.clone()))
            .collect();
        if upgrades != vec![vec![3u8, 4u8]] {
            return Observed::EffectMismatch { note: format!("UPGRADED events {upgrades:?}") };
        }
        Observed::of_reports(&run, "UPGRADED 3 -> 4 emitted")
    });
    r.run("LC-GL-02", || {
        let mut w = parent_w.clone();
        let wit = witness(&w, &u1379);
        Observed::of_reports(&run_job(&mut w, &u1379, LcFork::Gloas, &wit, "Gloas update, schedule without Gloas"), "")
    });
    // After the upgrade: the store in the Gloas layout.
    let mut gw2 = gw.clone();
    gw2.apply(&u1379, "period 1379 (Gloas)");
    let u1380 = sepolia::sepolia_update(GLOAS_PERIOD + 1);
    r.run("LC-GL-03", || {
        let mut w = gw2.clone();
        let mut x = u1380.clone();
        match &mut x.finalized_header.execution {
            Execution::BlockHash { block_hash, .. } => flip(block_hash),
            other => return Observed::Unsupported { reason: format!("period 1380's finalized header is not in the Gloas layout: {other:?}") },
        }
        Observed::of_reports(&job_for(&mut w, &x, "Gloas finalized block hash flipped"), "")
    });
    r.run("LC-GL-04", || {
        // A normalized branch with padding: the first Gloas update's finalized header, if it predates
        // the fork, carries its execution branch with leading zero nodes; else the finality branch.
        let mut w = gw.clone();
        let mut x = u1379.clone();
        let note;
        match &mut x.finalized_header.execution {
            Execution::BlockHash { branch, .. } if branch[0] == ZERO => {
                branch[0] = [1; 32];
                note = "padding node 0 of the finalized header's execution branch set to 0x01…";
            }
            _ if x.finality_branch[0] == ZERO => {
                x.finality_branch[0] = [1; 32];
                note = "padding node 0 of the finality branch set to 0x01…";
            }
            _ => return Observed::Unsupported { reason: "no zero padding node in the first Gloas update's branches".into() },
        }
        Observed::of_reports(&job_for(&mut w, &x, "non-zero padding"), note)
    });
    r.run("LC-GL-05", || {
        let mut w = gw.clone();
        let electra_byte = CompactUpdate::of(&u1378).encode(LcFork::Electra)[0];
        let mut bytes = CompactUpdate::of(&u1379).encode(LcFork::Gloas);
        bytes[0] = electra_byte;
        let wit = witness(&w, &u1379).encode();
        let inst = w.inst.clone();
        let store = inst.store(&w.chain);
        let jid = job_id(&inst.id, &store.digest(), &CompactUpdate::of(&u1379).digest(LcFork::Gloas));
        let job = job_actor(inst.program, &inst.id, &jid);
        let time = time_of(&w, &u1379);
        let rep = w.chain.submit(
            "Gloas update labelled Electra",
            inst.actor(),
            borsh::to_vec(&Message::Open { update: bytes, signers: wit }).unwrap(),
            vec![inst.actor(), job],
            vec![],
            time,
        );
        Observed::of_report(&rep, "the Gloas encoding with the Electra layout byte")
    });
    r.run("LC-GL-06", || {
        let mut w = gw2.clone();
        let wit = SignerWitness::build(&sepolia::sepolia_bootstrap().current_sync_committee, &u1378.sync_aggregate.bits).unwrap_or_default();
        Observed::of_reports(&run_job(&mut w, &u1378, LcFork::Electra, &wit, "period 1378 again, after the upgrade"), "")
    });
    r.run("LC-GL-07", || {
        let mut w = gw2.clone();
        let mut x = u1380.clone();
        x.finality_branch.drain(0..2);
        Observed::of_reports(&job_for(&mut w, &x, "Gloas finality branch of 7 nodes"), "")
    });
    r.run("LC-GL-08", || {
        let mut c = sepolia::sepolia_config(2);
        c.fork_epochs[Fork::Gloas as usize] += 256;
        let mut w = sepolia::sepolia_world(c);
        w.apply(&u1378, "period 1378");
        let wit = witness(&w, &u1379);
        Observed::of_reports(&run_job(&mut w, &u1379, LcFork::Gloas, &wit, "Gloas one period late in the schedule"), "")
    });
    // LC-WS-01: Open after the weak-subjectivity period of the store's header (mainnet).
    r.run("LC-WS-01", || {
        let mut w = base();
        let x = u(&w);
        let inst = w.inst.clone();
        let store = inst.store(&w.chain);
        let end = inst.config.ws_end_ms(store.finalized.slot).unwrap();
        let jid = job_id(&inst.id, &store.digest(), &CompactUpdate::of(&x).digest(LcFork::Electra));
        let job = job_actor(inst.program, &inst.id, &jid);
        let wit = witness(&w, &x).encode();
        let rep = w.chain.submit(
            "Open at the end of the period",
            inst.actor(),
            borsh::to_vec(&Message::Open { update: CompactUpdate::of(&x).encode(LcFork::Electra), signers: wit }).unwrap(),
            vec![inst.actor(), job],
            vec![],
            end,
        );
        Observed::of_report(&rep, "timestamp = ws end (exclusive)")
    });
    r.finish(&["lc"], &["LC-SUC-", "LC-GL-", "LC-WS-01"]);
}

// ---------------------------------------------------------------------------------------------
// The synthetic chain (keys the test holds)
// ---------------------------------------------------------------------------------------------

#[test]
fn lc_synthetic() {
    let mut r = Results::new(FILE, "syn");
    let syn = synthetic(mainnet_config(2), 0x31);

    // LC-SIG-07: the finalized header signed instead of the attested one.
    r.run("LC-SIG-07", || {
        let mut s = Synthetic { w: syn.w.clone(), fake: fake_committee(512, 0x31), slot0: syn.slot0 };
        let mut x = s.update(512, true);
        let mut tmp = x.clone();
        tmp.attested_header = x.finalized_header.clone();
        let root = sync_committee_signing_root(&tmp, &s.ctx());
        x.sync_aggregate.signature = sign(&s.fake, &x.sync_aggregate.bits, &root);
        Observed::of_reports(&s.run(&x, "finalized header signed"), "")
    });
    // LC-SIG-09: the domain from the fork version at signature_slot (a fork starts there).
    r.run("LC-SIG-09", || {
        let epoch_sig = (syn.slot0 + 128) / 32 + 1;
        let sig_slot = epoch_sig * 32;
        let mut c = mainnet_config(2);
        c.fork_epochs[Fork::Fulu as usize] = epoch_sig;
        let mut s = synthetic(c, 0x32);
        let mut wrong_spec = s.w.spec.clone();
        wrong_spec.epochs[Fork::Fulu as usize] = epoch_sig - 1;
        let f = Fabrication {
            finalized_slot: Some(s.slot0 + 64),
            attested_slot: sig_slot - 1,
            signature_slot: sig_slot,
            signers: 512,
            next_committee: None,
        };
        let wrong_ctx = Context { spec: &wrong_spec, genesis_validators_root: s.w.inst.config.genesis_validators_root, bls: &BLS };
        let x = fabricated_update(&f, &s.fake, &wrong_ctx);
        // The same update signed with the right domain is accepted (checked natively, not sent).
        let right = fabricated_update(&f, &s.fake, &s.ctx());
        let mut native = s.w.native.clone();
        let native_ok = native.process_update(&right, right.signature_slot + 1, &s.ctx()).is_ok();
        let obs = Observed::of_reports(&s.run(&x, "domain of the fork at signature_slot"), "");
        match obs {
            Observed::Code { code, note } => Observed::Code { code, note: format!("{note}; the correctly signed twin verifies natively: {native_ok}") },
            o => o,
        }
    });
    // LC-PART-02, 03: 341 and 342 of 512.
    for (id, n) in [("LC-PART-02", 341usize), ("LC-PART-03", 342)] {
        r.run(id, || {
            let mut s = Synthetic { w: syn.w.clone(), fake: fake_committee(512, 0x31), slot0: syn.slot0 };
            let x = s.update(n, true);
            let reports = s.run(&x, &format!("{n} of 512"));
            if reports.iter().all(TxReport::ok) && s.w.inst.record(&s.w.chain, x.finalized_header.beacon.slot).is_none() {
                return Observed::EffectMismatch { note: "applied but no record written".into() };
            }
            Observed::of_reports(&reports, &format!("{n} signers"))
        });
    }
    // LC-SLOT-03: finalized slot above the attested slot (branches consistent).
    r.run("LC-SLOT-03", || {
        let mut s = Synthetic { w: syn.w.clone(), fake: fake_committee(512, 0x31), slot0: syn.slot0 };
        let f = Fabrication { finalized_slot: Some(s.slot0 + 97), attested_slot: s.slot0 + 96, signature_slot: s.slot0 + 98, signers: 512, next_committee: None };
        let x = fabricated_update(&f, &s.fake, &s.ctx());
        Observed::of_reports(&s.run(&x, "finalized after attested"), "")
    });
    // LC-FIN-01: no finality.
    r.run("LC-FIN-01", || {
        let mut s = Synthetic { w: syn.w.clone(), fake: fake_committee(512, 0x31), slot0: syn.slot0 };
        let x = s.update(512, false);
        Observed::of_reports(&s.run(&x, "optimistic update"), "")
    });
    // LC-COM-04: a next committee without a branch.
    r.run("LC-COM-04", || {
        let mut s = Synthetic { w: syn.w.clone(), fake: fake_committee(512, 0x31), slot0: syn.slot0 };
        let mut x = s.update(512, true);
        x.next_sync_committee = fake_committee(512, 0x33).committee;
        Observed::of_reports(&s.run(&x, "next committee, empty branch"), "")
    });
    // LC-WS-02: VerifyAll of an update signed after the weak-subjectivity end.
    r.run("LC-WS-02", || {
        let mut c = mainnet_config(2);
        c.ws_period_epochs = 1;
        let mut s = synthetic(c, 0x34);
        let end_slot = s.w.inst.config.ws_end_slot(s.slot0).unwrap();
        let f = Fabrication { finalized_slot: Some(s.slot0 + 1), attested_slot: end_slot + 1, signature_slot: end_slot + 2, signers: 512, next_committee: None };
        let x = fabricated_update(&f, &s.fake, &s.ctx());
        let wit = SignerWitness::build(&s.fake.committee, &x.sync_aggregate.bits).unwrap();
        let message = Message::VerifyAll {
            config: s.w.inst.config.clone(),
            checkpoint: s.w.inst.checkpoint,
            store: s.w.inst.store(&s.w.chain).encode(),
            update: CompactUpdate::of(&x).encode(LcFork::Electra),
            signers: wit.encode(),
            adoption: vec![],
        };
        let verifier = Actor::new(s.w.chain.payer, ferrata_account());
        let header = header_actor(s.w.inst.program, &s.w.inst.id, x.finalized_header.beacon.slot);
        let time = s.w.inst.config.slot_start_ms(x.signature_slot) + 12_000;
        let rep = s.w.chain.submit("VerifyAll after the ws end", verifier, borsh::to_vec(&message).unwrap(), vec![verifier, s.w.inst.actor(), header], vec![], time);
        Observed::of_report(&rep, &format!("ws_period_epochs 1; signature slot {} (ws end slot {end_slot})", x.signature_slot))
    });
    r.finish(&["lc"], &["LC-SIG-07", "LC-SIG-09", "LC-PART-02", "LC-PART-03", "LC-SLOT-03", "LC-FIN-01", "LC-COM-04", "LC-WS-02"]);
}

// ---------------------------------------------------------------------------------------------
// Positive controls
// ---------------------------------------------------------------------------------------------

#[test]
fn lc_controls() {
    let mut r = Results::new(FILE, "ctl");
    r.run("LC-CTL-01", || {
        let w = World::new(2);
        let active = w.inst.state(&w.chain).map(|s| s.active).unwrap_or(false);
        let record = w.inst.record(&w.chain, w.bootstrap.header.beacon.slot).is_some();
        if !(active && record) {
            return Observed::EffectMismatch { note: format!("active {active}, record {record}") };
        }
        Observed::of_reports(&w.runs[0].reports, "instance active, first record written")
    });
    let b = base();
    r.run("LC-CTL-02", || {
        let mut w = b.clone();
        let x = u(&w);
        let run = job_for(&mut w, &x, "U");
        if run.iter().all(TxReport::ok) && w.inst.record(&w.chain, x.finalized_header.beacon.slot).is_none() {
            return Observed::EffectMismatch { note: "no record".into() };
        }
        Observed::of_reports(&run, "three transactions, record written")
    });
    r.run("LC-CTL-03", || {
        let mut w = b.clone();
        let x = w.updates[1].clone();
        let run = job_for(&mut w, &x, "update_1 (rotation)");
        Observed::of_reports(&run, &format!("{} transactions", run.len()))
    });
    r.run("LC-CTL-04", || {
        let mut w = b.clone();
        let x = u(&w);
        let wit = witness(&w, &x);
        let message = Message::VerifyAll {
            config: w.inst.config.clone(),
            checkpoint: w.inst.checkpoint,
            store: w.inst.store(&w.chain).encode(),
            update: CompactUpdate::of(&x).encode(LcFork::Electra),
            signers: wit.encode(),
            adoption: vec![],
        };
        let verifier = Actor::new(w.chain.payer, ferrata_account());
        let header = header_actor(w.inst.program, &w.inst.id, x.finalized_header.beacon.slot);
        let time = time_of(&w, &x);
        Observed::of_report(&w.chain.submit("VerifyAll U", verifier, borsh::to_vec(&message).unwrap(), vec![verifier, w.inst.actor(), header], vec![], time), "")
    });
    // LC-CTL-05, 06: the steps signed by accounts that never touched the instance (the harness has
    // one fee payer; the program reads no signer, so the extra signatures only show it does not care).
    let fresh_signed = |keys: [[u8; 32]; 3], label: &str| {
        let mut w = b.clone();
        let x = u(&w);
        let inst = w.inst.clone();
        let store = inst.store(&w.chain);
        let wit = witness(&w, &x);
        let jid = job_id(&inst.id, &store.digest(), &CompactUpdate::of(&x).digest(LcFork::Electra));
        let job = job_actor(inst.program, &inst.id, &jid);
        let time = time_of(&w, &x);
        let header = header_actor(inst.program, &inst.id, x.finalized_header.beacon.slot);
        let k: Vec<lee::PrivateKey> = keys.iter().map(|b| lee::PrivateKey::try_new(*b).unwrap()).collect();
        let mut reps = vec![w.chain.submit_signed(
            "Open (fresh signer)",
            inst.actor(),
            borsh::to_vec(&Message::Open { update: CompactUpdate::of(&x).encode(LcFork::Electra), signers: wit.encode() }).unwrap(),
            vec![inst.actor(), job],
            vec![],
            time,
            &[&k[0]],
        )];
        reps.push(w.chain.submit_signed("Miller (fresh signer)", job, borsh::to_vec(&Message::Miller).unwrap(), vec![job], vec![], time, &[&k[1]]));
        reps.push(w.chain.submit_signed("Finish (fresh signer)", job, borsh::to_vec(&Message::Finish).unwrap(), vec![job, inst.actor(), header], vec![], time, &[&k[2]]));
        Observed::of_reports(&reps, label)
    };
    r.run("LC-CTL-05", || fresh_signed([[0x51; 32], [0x51; 32], [0x51; 32]], "one fresh account signs every step"));
    r.run("LC-CTL-06", || fresh_signed([[0x52; 32], [0x53; 32], [0x54; 32]], "three fresh accounts, one per step"));
    r.finish(&["lc"], &["LC-CTL-"]);
}

// ---------------------------------------------------------------------------------------------
// A probe beside the vectors (not a vector): LC-NEW-06's property where it can be reached
// ---------------------------------------------------------------------------------------------

/// LC-NEW-06 as written (a rotation verified in one public transaction against a stale store)
/// runs out of LEZ's 10M-cycle budget before any check can refuse it. The property it aims at,
/// the proven path's compare-and-swap on the store digest, is reached here with two finality
/// updates of the synthetic chain: U1 applied by the public job, then `VerifyAll` of U2 (newer,
/// valid on the original store) against the store as it was before U1, and, as its control, the
/// same `VerifyAll` against the current store. Written to `redteam_lc.probes.json`.
#[test]
fn lc_probe_stale_store_on_the_proven_path() {
    let mut s = synthetic(mainnet_config(2), 0x35);
    let stale = s.w.inst.store(&s.w.chain).encode();
    let u1 = s.update(512, true);
    let applied = s.run(&u1, "U1");
    assert!(applied.iter().all(TxReport::ok), "U1 applies");
    let f = Fabrication { finalized_slot: Some(s.slot0 + 128), attested_slot: s.slot0 + 160, signature_slot: s.slot0 + 161, signers: 512, next_committee: None };
    let u2 = fabricated_update(&f, &s.fake, &s.ctx());
    let wit = SignerWitness::build(&s.fake.committee, &u2.sync_aggregate.bits).unwrap();
    let verify = |s: &mut Synthetic, store: Vec<u8>, label: &str| {
        let message = Message::VerifyAll {
            config: s.w.inst.config.clone(),
            checkpoint: s.w.inst.checkpoint,
            store,
            update: CompactUpdate::of(&u2).encode(LcFork::Electra),
            signers: wit.encode(),
            adoption: vec![],
        };
        let verifier = Actor::new(s.w.chain.payer, ferrata_account());
        let header = header_actor(s.w.inst.program, &s.w.inst.id, u2.finalized_header.beacon.slot);
        let time = s.w.inst.config.slot_start_ms(u2.signature_slot) + 12_000;
        let (ia, payload) = (s.w.inst.actor(), borsh::to_vec(&message).unwrap());
        s.w.chain.submit(label, verifier, payload, vec![verifier, ia, header], vec![], time)
    };
    let mut s2 = Synthetic { w: s.w.clone(), fake: fake_committee(512, 0x35), slot0: s.slot0 };
    let stale_run = verify(&mut s, stale, "VerifyAll U2 on the pre-U1 store");
    let current = s2.w.inst.store(&s2.w.chain).encode();
    let control = verify(&mut s2, current, "VerifyAll U2 on the current store");
    let describe = |r: &TxReport| {
        serde_json::json!({
            "label": r.label, "applied": r.ok(), "exit_code": r.exit_code,
            "code_name": r.exit_code.map(redteam_common::code_name), "revert": r.charge.revert, "gas_exec": r.charge.gas_exec,
        })
    };
    let doc = serde_json::json!({
        "test": "lez/harness/tests/redteam_lc.rs", "probe": "LC-NEW-06's property on the synthetic chain",
        "stale_store": describe(&stale_run), "control_current_store": describe(&control),
    });
    let dir = harness::results_dir().join("redteam-2026-11-18");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("redteam_lc.probes.json"), serde_json::to_string_pretty(&doc).unwrap() + "\n").unwrap();
    println!("{doc}");
}
