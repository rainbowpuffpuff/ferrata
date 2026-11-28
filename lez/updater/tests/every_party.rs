//! RFP-021 Functionality #12, walked party by party: every off-chain participant the attestation side requires, as
//! `docs/trust.md` §2.4 lists them, is made to lie, lag, stop or censor in turn, and each scenario must still end with
//! an attestation produced: `Attest` applied at a record the instance wrote, the statement equal to the native
//! evaluation. One test per party; `trust_md_lists_exactly_the_parties_walked_here` keeps the document and this
//! file in step (a party added to the table without a test here, or a test here without its row, fails).
//!
//! What runs: `ferrata-updater` (the submitter daemon) against LEZ v0.4.0-rc1's state machine in process
//! (`HarnessLedger`: the attestation program at genesis, LEZ's fee settlement, RISC0_DEV_MODE), fed the mainnet
//! fixture of 10 November 2026 (`crates/ferrata-spectests/fixtures/mainnet-attest-2026-11-10`: the bootstrap of slot
//! 15,622,144 and the finality update attested at slot 15,624,544 that finalizes slot 15,624,480, execution block 26,384,851), and that block's
//! recorded `eth_getProof` for WETH's storage as the attested fact. The checkpoint scenario also initialises a second
//! instance from the lab's recorded mainnet checkpoint (`lez/lab/results/record/net/mainnet`).
//!
//! `RISC0_DEV_MODE=1 cargo test -p ferrata-updater --features harness --locked --test every_party -- --nocapture`
#![cfg(feature = "harness")]

use std::{collections::BTreeSet, fs};

use anyhow::{bail, Result};
use ferrata_advance::{job_step, layout_for, signer_witness, Planned};
use ferrata_core::{
    merkle::{compute_merkle_branch_root, floorlog2, subtree_index, EXECUTION_PAYLOAD_GINDEX},
    steps,
    store::ApplyOutcome,
    types::{Execution, LightClientUpdate, SyncCommittee},
    validate::{finalized_root_gindex_at_slot, get_lc_execution_root},
    witness::{CommitteeSummary, CompactStore, CompactUpdate},
    Error, Root,
};
use ferrata_lez_wire::{
    attest::{evaluate, Attestation, Request},
    code, instance_id, job_id, Config, Message, RecordState,
};
use ferrata_updater::{
    chain::{decode_finality_update, preset, root},
    config::{Fees, Policy},
    ledger::{HarnessLedger, Ledger, Tx, TxOutcome},
    source::{lab_record, Clock, Fault, Faulty, Recorded, Source},
    updater::{Instance, Plan, Step, Updater},
};
use harness::attest::{account_query, fixture_file, Fixture};
use lee::{AccountId, Actor};
use ferrata_updater::lez::PayerKey;

const GAS: u64 = 4_800_000;
const MAX_FEE: u128 = 1_000_000_000;

/// The scenarios of this file, one per row of `docs/trust.md` §2.4 (its last column names them).
const WALKED: &[&str] = &[
    "submitters_any_account_advances_none_is_needed",
    "provers_none_is_needed_on_the_public_job",
    "beacon_providers_lie_lag_or_stop",
    "execution_providers_cannot_forge_a_fact",
    "checkpoint_source_misleads_only_until_checked",
    "sequencers_censor_or_stall_the_instance_still_advances",
    "deployer_holds_nothing_after_deployment",
];

fn key(b: u8) -> PayerKey {
    ferrata_updater::lez::payer_key([b; 32])
}

fn id_of(k: &PayerKey) -> AccountId {
    ferrata_updater::lez::payer_account(k)
}

fn no_sleep(_: u64) {}

fn policy() -> Policy {
    Policy { defer_window: 1, retry_base_ms: 0, ..Policy::default() }
}

/// A beacon node serving the fixture: the bootstrap of its checkpoint (`any_root`: for whatever root is asked, as a
/// careless or lying node would) and its finality update (`behind`: not yet, as a node that lags the chain and has
/// not seen it). It serves no period update: the bootstrap and the update are in one sync-committee period.
struct FixtureNode {
    label: String,
    checkpoint: Root,
    bootstrap: Vec<u8>,
    finality: Vec<u8>,
    any_root: bool,
    behind: bool,
}

impl Source for FixtureNode {
    fn name(&self) -> &str {
        &self.label
    }
    fn bootstrap(&mut self, block_root: &Root) -> Result<Vec<u8>> {
        if *block_root == self.checkpoint || self.any_root {
            return Ok(self.bootstrap.clone());
        }
        bail!("{}: no bootstrap for 0x{}", self.label, hex::encode(block_root))
    }
    fn updates(&mut self, _start_period: u64, _count: u64) -> Result<Vec<Vec<u8>>> {
        Ok(vec![])
    }
    fn finality_update(&mut self) -> Result<Vec<u8>> {
        if self.behind {
            bail!("{}: no finality update newer than the checkpoint yet (404)", self.label)
        }
        Ok(self.finality.clone())
    }
}

struct Fx {
    fx: Fixture,
    config: Config,
    bootstrap: Vec<u8>,
    finality: Vec<u8>,
}

impl Fx {
    fn load() -> Self {
        Self {
            fx: Fixture::load(),
            config: preset("mainnet", 2).unwrap(),
            bootstrap: fixture_file("bootstrap.ssz"),
            finality: fixture_file("finality_update.ssz"),
        }
    }
    fn node(&self, label: &str) -> FixtureNode {
        FixtureNode {
            label: label.into(),
            checkpoint: self.fx.checkpoint,
            bootstrap: self.bootstrap.clone(),
            finality: self.finality.clone(),
            any_root: false,
            behind: false,
        }
    }
    fn honest(&self, label: &str) -> Box<dyn Source> {
        Box::new(self.node(label))
    }
    fn lagging(&self, label: &str) -> Box<dyn Source> {
        Box::new(FixtureNode { behind: true, ..self.node(label) })
    }
    fn faulty(&self, label: &str, fault: Fault) -> Box<dyn Source> {
        Box::new(Faulty::new(self.node(label), fault, self.config.clone()))
    }
    fn instance_at(&self, checkpoint: Root) -> Instance {
        Instance::new("mainnet", HarnessLedger::program(), self.config.clone(), checkpoint)
    }
    fn instance(&self) -> Instance {
        self.instance_at(self.fx.checkpoint)
    }
    fn updater(&self, label: &str, payer: PayerKey, sources: Vec<Box<dyn Source>>) -> Updater {
        updater_for(self.instance(), label, payer, sources)
    }
    /// What a consumer asks: WETH's account and the holder's balance slot at block 26,384,851, bound to `inst`.
    fn request(&self, inst: &Instance) -> Request {
        let proof = self.fx.weth_proof();
        Request {
            instance_id: inst.id,
            chain_id: self.fx.chain_id,
            accounts: vec![account_query(&proof, vec![], vec![(self.fx.weth_slot, vec![])])],
            ..Request::default()
        }
    }
    /// A block time after the finality update was signed (`Attest` carries no validity window).
    fn time(&self) -> u64 {
        self.config.slot_start_ms(self.fx.update.signature_slot) + 24_000
    }
}

fn updater_for(inst: Instance, label: &str, payer: PayerKey, sources: Vec<Box<dyn Source>>) -> Updater {
    let mut u = Updater::new(label, inst, payer, Fees::default(), policy(), sources, None).unwrap();
    u.sleep = no_sleep;
    u
}

/// `Attest { request }` as the root of a transaction at `header`.
fn attest_tx(header: Actor, request: &Request, time_ms: u64) -> Tx {
    Tx {
        label: "Attest".into(),
        root: header,
        message: borsh::to_vec(&Message::Attest { request: request.clone() }).unwrap(),
        actors: vec![header],
        evidence: vec![],
        time_ms,
    }
}

fn record_at(l: &dyn Ledger, header: Actor) -> Option<RecordState> {
    let s = l.actor_state(header).unwrap();
    (!s.is_empty()).then(|| borsh::from_slice(&s).expect("a record"))
}

/// The end of every scenario: the consumer's statement is produced. The record of the fixture's finalized slot
/// exists, the program applies `Attest` on it, and the statement is the native evaluation's.
fn attestation_produced(l: &mut dyn Ledger, f: &Fx, inst: &Instance, payer: &PayerKey) -> Attestation {
    let header = inst.header(f.fx.finalized_slot);
    let rec = record_at(l, header).expect("a record at the fixture's finalized slot");
    let request = f.request(inst);
    let expected = evaluate(&rec.instance_id, rec.chain_id, &rec.header().unwrap(), &request)
        .unwrap_or_else(|c| panic!("refused natively with {c}"));
    let out = l.submit(payer, &attest_tx(header, &request, f.time()), GAS, MAX_FEE).unwrap();
    assert!(out.applied, "Attest not applied: {out:?}");
    assert_eq!(expected.block_number, f.fx.block);
    expected
}

/// Rounds until the updater has nothing more to send.
fn settle(u: &mut Updater, l: &mut dyn Ledger, max: usize) -> usize {
    for round in 0..max {
        match u.tick(l).unwrap() {
            Plan::UpToDate => return round,
            Plan::Stuck(w) | Plan::Waiting(w) => panic!("{}: {w}", u.label),
            _ => {}
        }
    }
    panic!("{}: not up to date after {max} rounds", u.label)
}

fn finalized(u: &Updater, l: &dyn Ledger) -> Option<u64> {
    u.inst.store(l).unwrap().map(|s| s.finalized.slot)
}

fn refused(l: &HarnessLedger) -> usize {
    l.log.iter().filter(|(_, _, o)| !o.applied).count()
}

fn sent_by(l: &HarnessLedger, k: &PayerKey) -> usize {
    l.log.iter().filter(|(p, _, _)| *p == id_of(k)).count()
}

/// The job an adversary opens for `update` on `inst`, built as an honest submitter builds it: the layout, the
/// compact update, the signer witness from the full committee that signs it (`committee`, which must be the one the
/// store summarises) and the job id over the store the job starts from. Only the update is the adversary's.
fn job_for(
    f: &Fx,
    inst: &Instance,
    store: &CompactStore,
    committee: &SyncCommittee,
    update: &LightClientUpdate,
) -> Planned {
    let spec = f.config.spec();
    let layout = layout_for(&f.config, store.fork, update).expect("a layout at the update's slot");
    let store_u = steps::store_for_update(store, layout, update.attested_header.beacon.slot, &spec).unwrap();
    let compact = CompactUpdate::of(update);
    let jid = job_id(&inst.id, &store_u.digest(), &compact.digest(layout));
    Planned {
        update: update.clone(),
        layout,
        compact,
        witness: signer_witness(committee, update),
        jid,
        outcome: ApplyOutcome { advanced: true, ..ApplyOutcome::default() },
    }
}

/// `step` of the job `p` as a transaction (time: a slot after the update's signature slot, as the updater sends it).
fn step_tx(f: &Fx, inst: &Instance, p: &Planned, step: Step) -> Tx {
    let t = job_step(inst.program, &inst.id, &f.config, p, step).unwrap();
    Tx {
        label: format!("forged {}", step.name()),
        root: t.root,
        message: t.message,
        actors: t.actors,
        evidence: vec![],
        time_ms: f.config.slot_start_ms(p.update.signature_slot) + 12_000,
    }
}

/// The program's checks on the job `p`, run natively step by step (`Open` is step 1, `Miller` the Miller loops,
/// `Finish` the final exponentiation): the first step that refuses and its code, or `None` if every step passes.
fn native_refusal(f: &Fx, store: &CompactStore, p: &Planned) -> Option<(Step, u16)> {
    let spec = f.config.spec();
    let store_u = steps::store_for_update(store, p.layout, p.update.attested_header.beacon.slot, &spec).unwrap();
    let opened = steps::step1(
        &store_u,
        &p.compact,
        &p.witness,
        p.update.signature_slot,
        &spec,
        &f.config.genesis_validators_root,
        f.config.adoption_parts,
    );
    let mut job = match opened {
        Ok(job) => job,
        Err(e) => return Some((Step::Open, e.code())),
    };
    if let Err(e) = job.step2() {
        return Some((Step::Miller, e.code()));
    }
    job.step3().err().map(|e| (Step::Finish, e.code()))
}

/// `update` with a false finalized header that every Merkle check accepts: the execution state root replaced (the
/// root every attested fact is proven against), the finalized header's `body_root` recomputed from its execution
/// branch, and the attested header's `state_root` recomputed from the finality branch. The sync aggregate is the
/// committee's genuine one, which signed the real attested header: only the signature check can tell.
fn with_false_state_root(f: &Fx, update: &LightClientUpdate, layout: ferrata_core::fork::LcFork) -> LightClientUpdate {
    let spec = f.config.spec();
    let mut u = update.clone();
    let Execution::Payload { header, .. } = &mut u.finalized_header.execution else {
        panic!("the fixture's headers carry an execution payload header")
    };
    header.state_root = [0x42; 32];
    let execution_root = get_lc_execution_root(&u.finalized_header, layout, &spec);
    let Execution::Payload { branch, .. } = &u.finalized_header.execution else { unreachable!() };
    u.finalized_header.beacon.body_root = compute_merkle_branch_root(
        &execution_root,
        branch,
        floorlog2(EXECUTION_PAYLOAD_GINDEX),
        subtree_index(EXECUTION_PAYLOAD_GINDEX),
    )
    .unwrap();
    let gindex = finalized_root_gindex_at_slot(u.attested_header.beacon.slot, layout, &spec);
    let depth = floorlog2(gindex);
    let padding = u.finality_branch.len() - depth;
    u.attested_header.beacon.state_root = compute_merkle_branch_root(
        &u.finalized_header.beacon.hash_tree_root(),
        &u.finality_branch[padding..],
        depth,
        subtree_index(gindex),
    )
    .unwrap();
    u
}

/// Submitters (updater operators): liveness only. Whoever ran the updater stops after `Init`; an account that never
/// touched the instance, with no state and no registration, carries it to the newest finalized header and the
/// attestation is produced. Forged updates sent by anyone, with no native check and with everything but the
/// signature right (the store's committee, the update's signer witness, the job id), are refused by the program's
/// signature check and change nothing: a signature with one byte flipped at `Open` (41, not a point of G2's
/// subgroup), and the committee's genuine signature over a false finalized header whose every Merkle branch was
/// recomputed to verify at `Finish` (40, the pairing check), each with the code the native steps give, while the
/// genuine update with the same witness passes every step natively. As a class: when every operator stops, the
/// consumer runs the same daemon with its own key (the stranger here).
#[test]
fn submitters_any_account_advances_none_is_needed() {
    let f = Fx::load();
    let (first, stranger, forger) = (key(0x41), key(0x42), key(0x43));
    let mut l = HarnessLedger::new(&[first.clone(), stranger.clone(), forger.clone()]);
    let mut a = f.updater("first-operator", first.clone(), vec![f.honest("node-a")]);
    for _ in 0..10 {
        if finalized(&a, &l).is_some() {
            break;
        }
        a.tick(&mut l).unwrap();
    }
    assert_eq!(finalized(&a, &l), Some(f.fx.bootstrap_slot), "Init and adoption applied, nothing more");
    drop(a); // the first operator stops for good
    assert_eq!(sent_by(&l, &stranger), 0, "the stranger has never sent anything");

    // Forged updates, sent the way an adversary sends them: no native check, and everything but the signature right.
    let inst = f.instance();
    let store = inst.store(&l).unwrap().expect("the instance is active");
    let committee = &f.fx.bootstrap.current_sync_committee;
    assert_eq!(CommitteeSummary::of(committee), store.current, "the bootstrap's committee is the store's");
    let state_before = l.actor_state(inst.actor()).unwrap();
    let (genuine, _) = decode_finality_update(&f.finality, &f.config).unwrap();
    let honest = job_for(&f, &inst, &store, committee, &genuine);
    assert!(!honest.witness.keys.is_empty(), "the update has non-signers, so its witness is not empty");
    assert_eq!(native_refusal(&f, &store, &honest), None, "the genuine update passes every step with this witness");
    const MALFORMED: u16 = Error::SignatureMalformed.code();
    const INVALID: u16 = Error::SignatureInvalid.code();

    // (a) One byte of the signature flipped: a signature present, and not a point of G2's prime-order subgroup.
    let liar = Faulty::new(f.node("x"), Fault::BadSignature, f.config.clone());
    let (flipped, _) = decode_finality_update(&liar.flip_signature(f.finality.clone(), false), &f.config).unwrap();
    assert_ne!(flipped.sync_aggregate.signature, genuine.sync_aggregate.signature);
    let a = job_for(&f, &inst, &store, committee, &flipped);
    assert_eq!(a.witness, honest.witness, "the same signer witness as the genuine update");
    assert_eq!(native_refusal(&f, &store, &a), Some((Step::Open, MALFORMED)));
    let out_a = l.submit(&forger, &step_tx(&f, &inst, &a, Step::Open), GAS, MAX_FEE).unwrap();
    assert!(!out_a.applied, "a flipped signature opens no job: {out_a:?}");
    assert_eq!(out_a.exit_code, Some(u32::from(MALFORMED)), "refused by the signature check (SignatureMalformed)");
    assert!(l.actor_state(inst.job(&a.jid)).unwrap().is_empty(), "no job account");

    // (b) The committee's genuine signature over a false finalized header (another execution state root) whose
    // execution and finality branches were recomputed to verify: well-formed, and wrong for what it is sent with.
    let false_header = with_false_state_root(&f, &genuine, honest.layout);
    let b = job_for(&f, &inst, &store, committee, &false_header);
    assert_eq!(b.witness, honest.witness);
    assert_eq!(native_refusal(&f, &store, &b), Some((Step::Finish, INVALID)), "every check passes but the pairing");
    for step in [Step::Open, Step::Miller] {
        let out = l.submit(&forger, &step_tx(&f, &inst, &b, step), GAS, MAX_FEE).unwrap();
        assert!(out.applied, "{} of the false header: {out:?}", step.name());
    }
    let out_b = l.submit(&forger, &step_tx(&f, &inst, &b, Step::Finish), GAS, MAX_FEE).unwrap();
    assert!(!out_b.applied, "the false header is not applied: {out_b:?}");
    assert_eq!(out_b.exit_code, Some(u32::from(INVALID)), "refused by the pairing check (SignatureInvalid)");
    assert_eq!(l.actor_state(inst.actor()).unwrap(), state_before, "the instance, store included, did not move");
    assert!(record_at(&l, inst.header(f.fx.finalized_slot)).is_none(), "no record of the false header");
    println!(
        "forged updates with the signer witness right: a flipped signature byte refused at Open (exit {:?}); the \
         genuine signature over a false execution state root, every branch recomputed: Open and Miller applied, \
         Finish refused (exit {:?}); the instance unchanged, no record",
        out_a.exit_code, out_b.exit_code
    );

    let mut b = f.updater("stranger", stranger.clone(), vec![f.honest("node-b")]);
    settle(&mut b, &mut l, 50);
    assert_eq!(finalized(&b, &l), Some(f.fx.finalized_slot));
    assert!(sent_by(&l, &stranger) >= 3, "the stranger sent the update's job itself");
    let att = attestation_produced(&mut l, &f, &b.inst, &stranger);
    println!(
        "submitters: the first operator stopped after Init; a stranger sent {} transactions; attestation of block {} \
         with {} statement(s)",
        sent_by(&l, &stranger),
        att.block_number,
        att.statements.len()
    );
}

/// Provers: liveness only, and none is needed. The instance advances on the public job (`Init`, `Adopt`, `Open`,
/// `Miller`, `Finish`), which the sequencer executes; no transaction of the run carries a proof, and the attestation
/// is produced. The proven path (`VerifyAll`, then `ApplyVerified`) is an option for whoever wants one transaction.
#[test]
fn provers_none_is_needed_on_the_public_job() {
    let f = Fx::load();
    let op = key(0x44);
    let mut l = HarnessLedger::new(std::slice::from_ref(&op));
    let mut u = f.updater("public-job-only", op.clone(), vec![f.honest("node")]);
    settle(&mut u, &mut l, 50);
    let labels: Vec<String> = l.log.iter().map(|(_, label, _)| label.clone()).collect();
    assert!(!labels.is_empty());
    for label in &labels {
        assert!(!label.contains("Verify") && !label.contains("Apply"), "a proven-path transaction: {label}");
    }
    assert_eq!(refused(&l), 0);
    attestation_produced(&mut l, &f, &u.inst, &op);
    println!("provers: {} public transactions, none proven: {labels:?}", labels.len());
}

/// Beacon API providers: liveness only. One serves forged signatures, one a bootstrap of another block, one lags
/// (it has the checkpoint but not yet the newest finality update), one is down; nothing they serve reaches the chain
/// (no gas refused) and the honest one carries the instance. As a class: with every third-party provider lying,
/// lagging or down the instance holds still (no false header, no gas wasted); the consumer adds its own node and the
/// attestation is produced.
#[test]
fn beacon_providers_lie_lag_or_stop() {
    let f = Fx::load();
    let op = key(0x45);
    let mut l = HarnessLedger::new(std::slice::from_ref(&op));
    let mut u = f.updater(
        "mixed-sources",
        op.clone(),
        vec![
            f.faulty("liar-signature", Fault::BadSignature),
            f.faulty("liar-bootstrap", Fault::WrongBootstrap),
            f.lagging("lagging"),
            f.faulty("down", Fault::Down),
            f.honest("honest"),
        ],
    );
    settle(&mut u, &mut l, 60);
    assert_eq!(finalized(&u, &l), Some(f.fx.finalized_slot));
    assert_eq!(refused(&l), 0, "nothing the liars served reached the chain");
    assert!(u.metrics.sources["liar-signature(BadSignature)"].refused >= 1);
    assert_eq!(u.health().0, "degraded");
    attestation_produced(&mut l, &f, &u.inst, &op);

    // Every third-party provider lies or is down.
    let op2 = key(0x46);
    let mut l2 = HarnessLedger::new(std::slice::from_ref(&op2));
    let mut v = f.updater(
        "third-parties-only",
        op2.clone(),
        vec![f.faulty("liar-signature", Fault::BadSignature), f.lagging("lagging"), f.faulty("down", Fault::Down)],
    );
    for _ in 0..20 {
        let _ = v.tick(&mut l2).ok();
    }
    // The liar's bootstrap is the real one (it must hash to the checkpoint), so the instance may exist; its forged
    // update never reaches the chain.
    assert!(finalized(&v, &l2).is_none_or(|s| s == f.fx.bootstrap_slot), "no header beyond the checkpoint");
    assert_eq!(refused(&l2), 0, "no gas spent on what they served");
    assert!(record_at(&l2, v.inst.header(f.fx.finalized_slot)).is_none(), "no record of the fixture's block yet");
    let held = l2.log.len();
    drop(v);
    // The consumer adds its own node.
    let mut w = f.updater(
        "with-own-node",
        op2.clone(),
        vec![
            f.faulty("liar-signature", Fault::BadSignature),
            f.lagging("lagging"),
            f.faulty("down", Fault::Down),
            f.honest("own-node"),
        ],
    );
    settle(&mut w, &mut l2, 60);
    attestation_produced(&mut l2, &f, &w.inst, &op2);
    println!(
        "beacon providers: liars, a lagging and a down node beside an honest one: 0 refused; third parties only: held \
         with {held} transactions sent, then the consumer's own node: attestation produced"
    );
}

/// Execution RPC providers: liveness only. A proof with one byte changed (what a lying provider would serve) is
/// refused by the program with the code the native evaluation gives, and nothing changes; the honest proof, which
/// any full node (the consumer's own included) serves for the same block, is accepted. The program checks a proof
/// only against the record's state root: no provider identity is part of the request.
#[test]
fn execution_providers_cannot_forge_a_fact() {
    let f = Fx::load();
    let op = key(0x47);
    let mut l = HarnessLedger::new(std::slice::from_ref(&op));
    let mut u = f.updater("updater", op.clone(), vec![f.honest("node")]);
    settle(&mut u, &mut l, 50);
    let header = u.inst.header(f.fx.finalized_slot);
    let rec = record_at(&l, header).unwrap();
    let recorded = rec.header().unwrap();

    let mut lying = f.request(&u.inst);
    let leaf = lying.accounts[0].slots[0].proof.last_mut().expect("a storage proof");
    let at = leaf.len() / 2;
    leaf[at] ^= 0x01;
    let code = evaluate(&rec.instance_id, rec.chain_id, &recorded, &lying).expect_err("a changed proof refuses");
    let watched = [header, u.inst.actor()];
    let before: Vec<Vec<u8>> = watched.iter().map(|a| l.actor_state(*a).unwrap()).collect();
    let out: TxOutcome = l.submit(&op, &attest_tx(header, &lying, f.time()), GAS, MAX_FEE).unwrap();
    assert!(!out.applied);
    assert_eq!(out.exit_code, Some(u32::from(code)), "the program refuses with the native code");
    let after: Vec<Vec<u8>> = watched.iter().map(|a| l.actor_state(*a).unwrap()).collect();
    assert_eq!(after, before, "a refused Attest changes neither the record nor the instance");

    let mut lying_account = f.request(&u.inst);
    let node = lying_account.accounts[0].proof.first_mut().expect("an account proof");
    let at = node.len() / 2;
    node[at] ^= 0x01;
    let code2 = evaluate(&rec.instance_id, rec.chain_id, &recorded, &lying_account).expect_err("refuses");
    let out2 = l.submit(&op, &attest_tx(header, &lying_account, f.time()), GAS, MAX_FEE).unwrap();
    assert_eq!(out2.exit_code, Some(u32::from(code2)));

    let att = attestation_produced(&mut l, &f, &u.inst, &op);
    println!(
        "execution providers: a changed storage proof refused with {code}, a changed account proof with {code2}; the \
         honest proof attested ({} statements)",
        att.statements.len()
    );
}

/// The checkpoint source: the one party that affects safety, until the checkpoint is checked. A root no honest
/// beacon node can serve a bootstrap for never yields an instance (nothing is sent, nothing paid). An instance from
/// another real checkpoint is another instance (its id hashes the checkpoint), so a consumer pinned to the
/// instance it checked refuses that instance's records (211). The source cannot block: a checkpoint is public, any
/// node serves it, and anyone may initialise an instance from one it checked (the rest of this file does).
#[test]
fn checkpoint_source_misleads_only_until_checked() {
    let f = Fx::load();
    let op = key(0x48);
    let mut l = HarnessLedger::new(std::slice::from_ref(&op));

    // A false root: one bit of the real checkpoint flipped.
    let mut false_root = f.fx.checkpoint;
    false_root[31] ^= 0x01;
    assert_ne!(instance_id(&f.config, &false_root), instance_id(&f.config, &f.fx.checkpoint));
    let mut any = f.node("serves-any-root");
    any.any_root = true;
    let mut misled = updater_for(
        f.instance_at(false_root),
        "misled",
        op.clone(),
        vec![Box::new(any), Box::new(Faulty::new(f.node("liar"), Fault::WrongBootstrap, f.config.clone()))],
    );
    let mut stuck = 0;
    for _ in 0..10 {
        if matches!(misled.tick(&mut l).unwrap(), Plan::Stuck(_)) {
            stuck += 1;
        }
    }
    assert!(misled.inst.state(&l).unwrap().is_none(), "no instance under a root nobody can serve");
    assert!(l.log.is_empty(), "nothing sent, nothing paid");
    drop(misled);

    // The checked instance, and one from another real mainnet checkpoint (the lab's), on one chain.
    let mut good = f.updater("checked", op.clone(), vec![f.honest("node")]);
    settle(&mut good, &mut l, 50);
    let lab = lab_record("mainnet");
    let other_cp = root(&fs::read_to_string(lab.join("checkpoint")).unwrap()).unwrap();
    assert_ne!(other_cp, f.fx.checkpoint);
    let src = Recorded::open("lab", &lab, &f.config, Clock::new(1)).unwrap();
    let mut other = updater_for(f.instance_at(other_cp), "other-checkpoint", op.clone(), vec![Box::new(src)]);
    for _ in 0..10 {
        if finalized(&other, &l).is_some() {
            break;
        }
        other.tick(&mut l).unwrap();
    }
    let other_slot = finalized(&other, &l).expect("the other instance is active");
    assert_ne!(other.inst.id, good.inst.id);
    let other_header = other.inst.header(other_slot);
    let rec = record_at(&l, other_header).expect("the other instance's first record");
    let pinned = f.request(&good.inst);
    let code = evaluate(&rec.instance_id, rec.chain_id, &rec.header().unwrap(), &pinned).expect_err("refused");
    assert_eq!(code, ferrata_lez_wire::code::WRONG_INSTANCE);
    let out = l.submit(&op, &attest_tx(other_header, &pinned, f.time()), GAS, MAX_FEE).unwrap();
    assert_eq!(out.exit_code, Some(u32::from(code)));
    attestation_produced(&mut l, &f, &good.inst, &op);
    println!(
        "checkpoint source: a false root gave no instance ({stuck} of 10 rounds stuck, 0 sent); another checkpoint's \
         instance refused the pinned consumer's request with {code}; the checked instance attested"
    );
}

/// A sequencer that leaves out one submitter's transactions.
struct Censoring {
    inner: HarnessLedger,
    blocked: AccountId,
    left_out: usize,
}

impl Ledger for Censoring {
    fn actor_state(&self, actor: Actor) -> Result<Vec<u8>> {
        self.inner.actor_state(actor)
    }
    fn submit(&mut self, payer: &PayerKey, tx: &Tx, gas_limit: u64, max_fee: u128) -> Result<TxOutcome> {
        if id_of(payer) == self.blocked {
            self.left_out += 1;
            return Ok(TxOutcome { rejected: true, note: "left out by the sequencer".into(), ..Default::default() });
        }
        self.inner.submit(payer, tx, gas_limit, max_fee)
    }
}

/// LEZ sequencers: for Ferrata, liveness only (the zone's execution is LEZ's own trust, as for every program). One
/// that leaves out a submitter's transactions does not stop another submitter; one that drops connections for a
/// while costs retries, not a duplicated step. As a class, a zone that includes nothing stops every program on it,
/// Ferrata's included, until it resumes; the instance then resumes from its own state (RFP-021 R.7).
#[test]
fn sequencers_censor_or_stall_the_instance_still_advances() {
    let f = Fx::load();
    let (censored, other) = (key(0x49), key(0x4a));
    let mut c = Censoring {
        inner: HarnessLedger::new(&[censored.clone(), other.clone()]),
        blocked: id_of(&censored),
        left_out: 0,
    };
    let mut x = f.updater("censored", censored.clone(), vec![f.honest("node-x")]);
    let mut y = f.updater("other", other.clone(), vec![f.honest("node-y")]);
    let mut done = false;
    for _ in 0..80 {
        let px = x.tick(&mut c).unwrap();
        let py = y.tick(&mut c).unwrap();
        if matches!((px, py), (Plan::UpToDate, Plan::UpToDate)) {
            done = true;
            break;
        }
    }
    assert!(done, "both updaters up to date");
    assert!(c.left_out > 0, "the censored submitter tried");
    assert_eq!(sent_by(&c.inner, &censored), 0, "none of its transactions was included");
    assert_eq!(finalized(&y, &c), Some(f.fx.finalized_slot));
    attestation_produced(&mut c, &f, &y.inst, &other);

    // A sequencer that drops the connection: two failures before each of three steps.
    let op = key(0x4b);
    let mut l = HarnessLedger::new(std::slice::from_ref(&op));
    let mut u = f.updater("stalled", op.clone(), vec![f.honest("node")]);
    for round in 0..50 {
        if round < 3 {
            l.fail_next = 2;
        }
        if matches!(u.tick(&mut l).unwrap(), Plan::UpToDate) {
            break;
        }
    }
    assert_eq!(refused(&l), 0, "no step sent twice");
    let opens = l.log.iter().filter(|(_, label, o)| label.contains("Open") && o.applied).count();
    assert_eq!(opens, 1, "one job for one update");
    assert!(u.metrics.transport_errors >= 2 && u.metrics.retries >= 2);
    attestation_produced(&mut l, &f, &u.inst, &op);
    println!(
        "sequencers: {} transactions of the censored submitter left out, the other carried the instance; {} dropped \
         connections cost {} retries and no duplicate",
        c.left_out, u.metrics.transport_errors, u.metrics.retries
    );
}

/// The deployer: nothing after deployment. The account that initialised the instance sends its own `Init` again,
/// and an `Init` for the same instance with another chain id: both refused, the instance's state byte for byte as
/// it was. (The program itself is immutable: LEZ v0.4's loader refuses a second header; lez/RESULTS.md §9.)
#[test]
fn deployer_holds_nothing_after_deployment() {
    let f = Fx::load();
    let deployer = key(0x4c);
    let mut l = HarnessLedger::new(std::slice::from_ref(&deployer));
    let mut u = f.updater("deployer", deployer.clone(), vec![f.honest("node")]);
    let init = match u.plan(&l).unwrap() {
        Plan::Send(a) if a.step == Step::Init => a.tx.clone(),
        p => panic!("expected Init first, got {p:?}"),
    };
    settle(&mut u, &mut l, 50);
    let before = l.actor_state(u.inst.actor()).unwrap();
    let again = l.submit(&deployer, &init, GAS, MAX_FEE).unwrap();
    assert!(!again.applied, "a second Init is refused: {again:?}");
    assert_eq!(again.exit_code, Some(u32::from(code::ALREADY_INITIALIZED)), "203: the instance exists");
    let mut other_chain = init.clone();
    let Message::Init { mut config, checkpoint, bootstrap } = borsh::from_slice(&init.message).unwrap() else {
        panic!("an Init")
    };
    config.chain_id = 11_155_111;
    other_chain.message = borsh::to_vec(&Message::Init { config, checkpoint, bootstrap }).unwrap();
    let rewrite = l.submit(&deployer, &other_chain, GAS, MAX_FEE).unwrap();
    assert!(!rewrite.applied, "an Init with another configuration is refused: {rewrite:?}");
    assert_eq!(
        rewrite.exit_code,
        Some(u32::from(code::WRONG_RECEIVER)),
        "201: another configuration is another instance, not this one"
    );
    assert_eq!(l.actor_state(u.inst.actor()).unwrap(), before, "the instance is unchanged");
    attestation_produced(&mut l, &f, &u.inst, &deployer);
    println!(
        "deployer: Init again refused (exit {:?}), Init with another chain id refused (exit {:?}); instance unchanged",
        again.exit_code, rewrite.exit_code
    );
}

/// `docs/trust.md` §2.4 names, in its last column, the scenario that walks each party; every scenario of this file
/// is named there once, every row names one of them, and every name is a test of this file (a renamed test fails).
#[test]
fn trust_md_lists_exactly_the_parties_walked_here() {
    let this = include_str!("every_party.rs");
    for name in WALKED {
        assert!(this.contains(&format!("#[test]\nfn {name}()")), "{name} is not a test of every_party.rs");
    }
    let doc = include_str!("../../../docs/trust.md");
    let start = doc.find("### 2.4").expect("trust.md §2.4");
    let rest = &doc[start..];
    let end = rest[4..].find("\n### ").map_or(rest.len(), |e| e + 4);
    let table: Vec<&str> =
        rest[..end].lines().skip_while(|l| !l.starts_with('|')).take_while(|l| l.starts_with('|')).skip(2).collect();
    assert!(table.len() >= WALKED.len(), "the participant table");
    let mut named = BTreeSet::new();
    for row in &table {
        let last = row.trim_end_matches('|').rsplit('|').next().unwrap();
        let fns: Vec<&str> = last.split('`').skip(1).step_by(2).filter(|n| WALKED.contains(n)).collect();
        assert_eq!(fns.len(), 1, "row without exactly one scenario of every_party.rs: {row}");
        assert!(named.insert(fns[0]), "scenario named twice: {}", fns[0]);
    }
    let walked: BTreeSet<&str> = WALKED.iter().copied().collect();
    assert_eq!(named, walked, "every scenario has its row");
    println!("trust.md §2.4: {} parties, each walked by its scenario", table.len());
}
