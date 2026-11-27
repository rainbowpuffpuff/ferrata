//! Join or wait (`policy.join_wait_rounds`, RFP-021 R.9): a job finishes only on the store it began on, so two jobs
//! open on one store pay the steps of the one that does not finish for nothing (on a running sequencer, gate runs G6
//! and G7: 127.0M and 72.7M execution gas in jobs opened, then overtaken). An updater does not open a job while another
//! job is open on the instance's store and moving, and does not send a step of a job that another open job leads; it
//! waits for that job, at most `join_wait_rounds` rounds after the job last moved. On the lab's recorded mainnet chain,
//! two updaters on one in-process LEZ chain.
#![cfg(feature = "harness")]

use std::{
    fs,
    sync::{Arc, Mutex},
};

use ferrata_core::Root;
use ferrata_lez_wire::Config;
use ferrata_updater::{
    chain::{decode_finality_update, preset, root},
    config::{Fees, Policy},
    ledger::{HarnessLedger, Ledger, Tx, TxOutcome},
    source::{lab_record, load_entries, Clock, Entry, Recorded, Source},
    updater::{leads, Instance, Plan, Step, Updater},
};
use lee::{Actor, PrivateKey};

fn key(b: u8) -> PrivateKey {
    PrivateKey::try_new([b; 32]).unwrap()
}

fn no_sleep(_: u64) {}

struct Fx {
    config: Config,
    dir: std::path::PathBuf,
    checkpoint: Root,
    entries: Vec<Entry>,
    clock: Clock,
}

impl Fx {
    fn load() -> Self {
        let config = preset("mainnet", 2).unwrap();
        let dir = lab_record("mainnet");
        let checkpoint = root(&fs::read_to_string(dir.join("checkpoint")).unwrap()).unwrap();
        let entries = load_entries(&dir.join("updates"), &config).unwrap();
        Self { config, dir, checkpoint, entries, clock: Clock::new(1) }
    }

    fn updater(&self, label: &str, payer: PrivateKey, join_wait_rounds: Option<u64>) -> Updater {
        let source = Recorded::open("a", &self.dir, &self.config, self.clock.clone()).unwrap();
        self.updater_with(label, payer, join_wait_rounds, Box::new(source))
    }

    fn updater_with(&self, label: &str, payer: PrivateKey, join_wait: Option<u64>, source: Box<dyn Source>) -> Updater {
        let inst = Instance::new("mainnet", HarnessLedger::program(), self.config.clone(), self.checkpoint);
        let policy = Policy { defer_window: 1, retry_base_ms: 0, join_wait_rounds: join_wait, ..Policy::default() };
        let mut u = Updater::new(label, inst, payer, Fees::default(), policy, vec![source], None).unwrap();
        u.sleep = no_sleep;
        u
    }

    /// The first clock `c` after which the next two entries are finality updates, each finalizing a later slot than
    /// everything before it: at `c` the instance holds the newest of the first `c` entries, at `c + 1` the sources serve
    /// U1, at `c + 2` U2 (U1 then no longer served: a beacon node serves only its newest finality update).
    fn two_new_finality_updates(&self) -> usize {
        let e = &self.entries;
        let head = |upto: usize| e[..upto].iter().map(|x| x.finalized_slot).max().unwrap();
        (3..e.len() - 2)
            .find(|&c| {
                !e[c].period_update
                    && !e[c + 1].period_update
                    && e[c].finalized_slot > head(c)
                    && e[c + 1].finalized_slot > e[c].finalized_slot
                    && e[c].attested_period == e[c + 1].attested_period
            })
            .expect("two new finality updates in one period")
    }
}

/// Every transaction sent through [`Run::tick`]: (updater, job, step, applied, execution and storage gas).
#[derive(Default)]
struct Run {
    sent: Vec<(String, Root, Step, bool, u64)>,
}

impl Run {
    /// One round of `u`; a transaction it sent is recorded.
    fn tick(&mut self, u: &mut Updater, l: &mut HarnessLedger) -> Plan {
        let n = l.log.len();
        let plan = u.tick(l).unwrap();
        if let (Plan::Send(a), true) = (&plan, l.log.len() > n) {
            let out = &l.log.last().unwrap().2;
            self.sent.push((u.label.clone(), a.job_id, a.step, out.applied, out.gas_exec + out.gas_stor));
        }
        plan
    }

    /// Jobs with a step applied, in the order of their first applied step: (job, steps applied, completed).
    fn jobs(&self) -> Vec<(Root, usize, bool)> {
        let mut out: Vec<(Root, usize, bool)> = Vec::new();
        for (_, jid, step, applied, _) in &self.sent {
            if !applied {
                continue;
            }
            match out.iter_mut().find(|j| j.0 == *jid) {
                Some(j) => {
                    j.1 += 1;
                    j.2 |= *step == Step::Finish;
                }
                None => out.push((*jid, 1, *step == Step::Finish)),
            }
        }
        out
    }

    /// Jobs with a step applied that never completed (on these runs' last round, each was overtaken: another job moved
    /// the store it began on), and the gas of their applied steps.
    fn abandoned(&self) -> (Vec<(Root, usize)>, u64) {
        let open: Vec<(Root, usize)> = self.jobs().into_iter().filter(|j| !j.2).map(|j| (j.0, j.1)).collect();
        let gas = self.sent.iter().filter(|s| s.3 && open.iter().any(|j| j.0 == s.1)).map(|s| s.4).sum();
        (open, gas)
    }

    fn sent_by(&self, label: &str, jid: &Root, step: Step) -> usize {
        self.sent.iter().filter(|s| s.0 == label && s.1 == *jid && s.2 == step).count()
    }
}

/// Catch up to clock `c` with `a` alone.
fn catch_up(fx: &Fx, a: &mut Updater, l: &mut HarnessLedger, c: usize) {
    fx.clock.set(c);
    for _ in 0..400 {
        if matches!(a.tick(l).unwrap(), Plan::UpToDate) {
            return;
        }
    }
    panic!("no catch-up to clock {c}");
}

fn finalized(u: &Updater, l: &HarnessLedger) -> u64 {
    u.inst.store(l).unwrap().unwrap().finalized.slot
}

/// A plan, short.
fn show(p: &Plan) -> String {
    match p {
        Plan::Send(x) => format!("send {} of 0x{}", x.step.name(), &hex::encode(x.job_id)[..8]),
        p => format!("{p:?}"),
    }
}

/// Both updaters at the same pace (a round each, in turn) until both have nothing to send, at most `max` rounds each.
fn settle(run: &mut Run, a: &mut Updater, b: &mut Updater, l: &mut HarnessLedger, max: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for round in 0..max {
        let pa = run.tick(a, l);
        let pb = run.tick(b, l);
        lines.push(format!("round {round}: a {}; b {}", show(&pa), show(&pb)));
        if matches!((&pa, &pb), (Plan::UpToDate, Plan::UpToDate)) {
            return lines;
        }
        for p in [&pa, &pb] {
            if let Plan::Stuck(w) = p {
                panic!("stuck: {w}\n{}", lines.join("\n"));
            }
        }
    }
    panic!("not settled in {max} rounds:\n{}", lines.join("\n"))
}

/// `a` opens the job of U1; then the sources serve U2, a later update. `b` (which saw U1 served) does not open U2's job
/// on the store `a`'s job will change: it waits while that job moves, then opens U2's job on the store it left. No job
/// is abandoned; both updates are applied. With the policy off (`join_wait_rounds = 0`, the updater before it), `b`
/// opens U2's job at once and one of the two jobs is paid for and abandoned.
#[test]
fn an_open_waits_for_the_job_open_on_the_store() {
    let mut lines = Vec::new();
    let mut outcomes = Vec::new();
    for policy in [None, Some(0)] {
        let fx = Fx::load();
        let c = fx.two_new_finality_updates();
        let (ka, kb) = (key(0x71), key(0x72));
        let mut l = HarnessLedger::new(&[ka.clone(), kb.clone()]);
        let mut a = fx.updater("a", ka, policy);
        let mut b = fx.updater("b", kb, policy);
        catch_up(&fx, &mut a, &mut l, c);
        let start = finalized(&a, &l);
        let mut run = Run::default();
        // U1 served: b sees it (its plan is dropped), a opens its job.
        fx.clock.set(c + 1);
        assert!(matches!(b.plan(&l).unwrap(), Plan::Send(ref x) if x.step == Step::Open));
        let j1 = match run.tick(&mut a, &mut l) {
            Plan::Send(x) if x.step == Step::Open => x.job_id,
            p => panic!("a: {p:?}"),
        };
        // U2 served.
        fx.clock.set(c + 2);
        let first = run.tick(&mut b, &mut l);
        lines.push(format!("join_wait_rounds {policy:?}: b's first round with U2 served: {}", show(&first)));
        if policy.is_none() {
            assert!(matches!(first, Plan::Deferred(Step::Open)), "{first:?}");
            assert_eq!(b.metrics.job_waits, 1);
            assert!(b.metrics.last_plan.contains("waiting for it"), "{}", b.metrics.last_plan);
            assert!(b
                .render_metrics()
                .contains("ferrata_updater_waits_for_other_jobs_total{daemon=\"b\",chain=\"mainnet\"} 1"));
        }
        lines.extend(settle(&mut run, &mut a, &mut b, &mut l, 200));
        let (abandoned, gas) = run.abandoned();
        let jobs = run.jobs();
        let end = finalized(&a, &l);
        lines.push(format!(
            "join_wait_rounds {policy:?}: store {start} -> {end}; jobs {}; abandoned {} ({gas} gas); b's waits {}",
            jobs.len(),
            abandoned.len(),
            b.metrics.job_waits
        ));
        assert_eq!(end, fx.entries[c + 1].finalized_slot, "U2 applied");
        outcomes.push((jobs, abandoned, gas, run.sent_by("b", &j1, Step::Open)));
    }
    println!("{}", lines.join("\n"));
    let (jobs, abandoned, gas, _) = &outcomes[0];
    assert!(abandoned.is_empty() && *gas == 0, "with the policy, no job abandoned: {abandoned:?}");
    assert_eq!(jobs.len(), 2, "U1's job, then U2's: {jobs:?}");
    assert!(jobs.iter().all(|j| j.2), "both completed");
    let (_, abandoned_off, gas_off, _) = &outcomes[1];
    assert_eq!(abandoned_off.len(), 1, "without it, one job paid for and abandoned: {abandoned_off:?}");
    assert!(*gas_off > 0);
}

/// The wait is bounded: a party opens U1's job and never sends another step. `b`, which saw U1 served, waits exactly
/// `join_wait_rounds` rounds after it first read that job (the job does not move), then opens U2's job and finishes
/// it; the stalled job is counted once as passed.
#[test]
fn a_stalled_job_holds_an_updater_back_for_join_wait_rounds_at_most() {
    let fx = Fx::load();
    let c = fx.two_new_finality_updates();
    let (kb, opener) = (key(0x73), key(0x74));
    let mut l = HarnessLedger::new(&[kb.clone(), opener.clone()]);
    let mut b = fx.updater("b", kb, None);
    catch_up(&fx, &mut b, &mut l, c);
    fx.clock.set(c + 1);
    assert!(matches!(b.plan(&l).unwrap(), Plan::Send(ref x) if x.step == Step::Open));
    let (u1, _) = decode_finality_update(&fx.entries[c].bytes, &fx.config).unwrap();
    let tx = b.open_unchecked(&l, &u1).unwrap();
    assert!(l.submit(&opener, &tx, 4_800_000, 1_000_000_000).unwrap().applied, "the other party's Open of U1");
    fx.clock.set(c + 2);
    let wait = b.policy.join_wait();
    assert_eq!(wait, 12, "2 * defer_window + 10");
    let mut run = Run::default();
    let mut waited = 0;
    let mut plans = Vec::new();
    for _ in 0..100 {
        let p = run.tick(&mut b, &mut l);
        if matches!(p, Plan::Deferred(Step::Open)) {
            waited += 1;
        }
        let done = matches!(p, Plan::UpToDate);
        plans.push(p);
        if done {
            break;
        }
    }
    let rest: Vec<String> = plans[waited as usize..].iter().map(show).collect();
    println!("waited {waited} rounds; then {rest:?}");
    assert_eq!(waited, wait, "held back exactly join_wait_rounds rounds");
    assert_eq!(b.metrics.job_waits, wait);
    assert_eq!(b.metrics.stalled_jobs_passed, 1);
    assert!(matches!(plans.last(), Some(Plan::UpToDate)));
    assert_eq!(finalized(&b, &l), fx.entries[c + 1].finalized_slot, "U2 applied after the wait");
}

/// Two jobs opened in one block (each updater planned against a store with no job open): the one that does not lead
/// ([`leads`]: as many steps, the older update) is not continued; the leading job finishes, the other is abandoned with
/// its `Open` alone, and its update, superseded, needs no job. `a`, busy with its own job, learns U2 from its sources and
/// finds `b`'s job, which leads.
#[test]
fn of_two_jobs_opened_in_one_block_the_one_behind_is_not_continued() {
    let fx = Fx::load();
    let c = fx.two_new_finality_updates();
    let (ka, kb) = (key(0x75), key(0x76));
    let mut l = HarnessLedger::new(&[ka.clone(), kb.clone()]);
    let mut a = fx.updater("a", ka, None);
    let mut b = fx.updater("b", kb, None);
    catch_up(&fx, &mut a, &mut l, c);
    fx.clock.set(c + 1);
    let open1 = match a.plan(&l).unwrap() {
        Plan::Send(x) if x.step == Step::Open => x,
        p => panic!("a: {p:?}"),
    };
    assert!(matches!(b.plan(&l).unwrap(), Plan::Send(ref x) if x.step == Step::Open));
    fx.clock.set(c + 2);
    let open2 = match b.plan(&l).unwrap() {
        Plan::Send(x) if x.step == Step::Open => x,
        p => panic!("b: {p:?}"),
    };
    assert_ne!(open1.job_id, open2.job_id);
    // One block: both sent on the store neither saw a job on.
    let mut run = Run::default();
    for (u, x) in [(&mut a, &open1), (&mut b, &open2)] {
        let out = u.submit(&mut l, x, true).unwrap().expect("sent");
        assert!(out.applied);
        run.sent.push((u.label.clone(), x.job_id, Step::Open, true, out.gas_exec + out.gas_stor));
    }
    let lines = settle(&mut run, &mut a, &mut b, &mut l, 200);
    println!("{}", lines.join("\n"));
    let (abandoned, gas) = run.abandoned();
    assert_eq!(abandoned, vec![(open1.job_id, 1)], "U1's job abandoned with its Open alone");
    assert_eq!(run.sent_by("a", &open1.job_id, Step::Miller), 0, "a never paid a step of the job behind");
    assert!(gas > 0 && gas < 4_800_000, "one Open's gas: {gas}");
    assert!(a.metrics.job_waits >= 1);
    assert_eq!(finalized(&a, &l), fx.entries[c + 1].finalized_slot, "U2 applied");
    let jobs = run.jobs();
    assert_eq!(
        jobs.iter().filter(|j| j.2).map(|j| j.0).collect::<Vec<_>>(),
        vec![open2.job_id],
        "U2's job alone completed"
    );
}

/// [`leads`] is a total order: an open job leads one not opened; more steps lead; at as many, the higher-ranked update
/// (as a rule the newer); then the greater id.
#[test]
fn which_job_leads() {
    use std::cmp::Reverse;
    let rank = |fin: u64, sig: u64| (false, fin, Reverse(sig), 400usize);
    let (x, y) = ([1u8; 32], [2u8; 32]);
    assert!(leads(&(1, rank(10, 11), x), &(0, rank(20, 21), y)), "an open job leads one not opened");
    assert!(leads(&(2, rank(10, 11), x), &(1, rank(20, 21), y)), "more steps lead");
    assert!(leads(&(1, rank(20, 21), x), &(1, rank(10, 11), y)), "at as many steps, the newer update");
    assert!(
        leads(&(1, (false, 20, Reverse(21), 400), x), &(1, (false, 20, Reverse(25), 400), y)),
        "the earlier signature"
    );
    assert!(leads(&(1, (true, 10, Reverse(11), 400), x), &(1, (false, 20, Reverse(21), 400), y)), "adopting first");
    assert!(leads(&(1, rank(10, 11), y), &(1, rank(10, 11), x)), "then the greater id");
    let all = [(0, rank(10, 11), x), (1, rank(10, 11), x), (1, rank(20, 21), y), (2, rank(10, 11), y)];
    for p in &all {
        assert!(!leads(p, p), "irreflexive");
        for q in &all {
            if p != q {
                assert!(leads(p, q) != leads(q, p), "exactly one of two leads");
            }
        }
    }
}

/// The wait has a cap: a party opens a new job each round (for the update the sources served the round before, so a
/// job that leads the updater's `Open` and has just moved), and never continues one. Each job is first read moving,
/// so the bound on a job that stops alone would hold the updater back for as long as the party keeps opening; the cap
/// (`JOIN_WAIT_CAP` times `join_wait_rounds` from the first round it waited on the instance's state) lets it send after
/// that many rounds, while the party is still opening, and the instance advances.
#[test]
fn a_new_job_each_round_holds_an_updater_back_for_the_cap_at_most() {
    let fx = Fx::load();
    let e = &fx.entries;
    let k = 12;
    let head = |upto: usize| e[..upto].iter().map(|x| x.finalized_slot).max().unwrap();
    let c = (3..e.len() - k)
        .find(|&c| {
            (0..k).all(|i| {
                !e[c + i].period_update
                    && e[c + i].attested_period == e[c].attested_period
                    && e[c + i].finalized_slot > if i == 0 { head(c) } else { e[c + i - 1].finalized_slot }
            })
        })
        .expect("twelve new finality updates in one period");
    let (kb, opener) = (key(0x77), key(0x78));
    let mut l = HarnessLedger::new(&[kb.clone(), opener.clone()]);
    let mut b = fx.updater("b", kb, Some(2));
    catch_up(&fx, &mut b, &mut l, c);
    let start = finalized(&b, &l);
    let cap = 2 * ferrata_updater::updater::JOIN_WAIT_CAP;
    // The party opens the job of the newest update served (`b` has seen it served), on the store as it is.
    let mut opened_by_party = 0;
    let mut party_opens = |b: &Updater, l: &mut HarnessLedger, t: usize| {
        let (newest, _) = decode_finality_update(&e[t - 1].bytes, &fx.config).unwrap();
        if let Ok(tx) = b.open_unchecked(l, &newest) {
            if l.submit(&opener, &tx, 4_800_000, 1_000_000_000).unwrap().applied {
                opened_by_party += 1;
            }
        }
    };
    fx.clock.set(c + 1);
    assert!(matches!(b.plan(&l).unwrap(), Plan::Send(ref x) if x.step == Step::Open), "b sees the first update");
    party_opens(&b, &mut l, c + 1);
    let mut lines = Vec::new();
    let mut first_send = None;
    for round in 1..=(k as u64 + 2 * cap) {
        // A newer update each round: b chooses it; the job the party opened last round (for the update b saw then)
        // leads b's Open and has just moved.
        let t = (c + 1 + round as usize).min(c + k);
        fx.clock.set(t);
        let p = b.tick(&mut l).unwrap();
        lines.push(format!("round {round}: clock {t}; {}; waits {}", show(&p), b.metrics.job_waits));
        if first_send.is_none() && matches!(p, Plan::Send(ref x) if x.step == Step::Open) {
            first_send = Some(round);
        }
        party_opens(&b, &mut l, t);
    }
    println!("{}\nthe party's applied Opens {opened_by_party}", lines.join("\n"));
    assert_eq!(first_send, Some(cap + 1), "the updater's Open sent once it had waited the cap");
    assert!(b.metrics.wait_caps >= 1);
    assert!(finalized(&b, &l) > start, "the instance advanced while the party kept opening jobs");
    // The cap is per state: on the state the updater's job left, the party's new job holds its Open back again.
    let finish = lines.iter().position(|x| x.contains("send finish")).expect("a Finish");
    assert!(lines[finish..].iter().any(|x| x.contains("Deferred(Open)")), "a wait on the next state: {lines:?}");
}

/// A recorded source that serves `copy`, when set, as its newest finality update (a copy of a recorded update re-dated
/// to a later signature slot of its period: the signature does not cover `signature_slot`, so it is as valid).
struct CopyFinality {
    inner: Recorded,
    copy: Arc<Mutex<Option<Vec<u8>>>>,
}

impl Source for CopyFinality {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn bootstrap(&mut self, block_root: &Root) -> anyhow::Result<Vec<u8>> {
        self.inner.bootstrap(block_root)
    }
    fn updates(&mut self, start_period: u64, count: u64) -> anyhow::Result<Vec<Vec<u8>>> {
        self.inner.updates(start_period, count)
    }
    fn finality_update(&mut self) -> anyhow::Result<Vec<u8>> {
        match self.copy.lock().unwrap().clone() {
            Some(c) => Ok(c),
            None => self.inner.finality_update(),
        }
    }
}

/// `b` (a finality update) with its `signature_slot` set to `to`, as `tests/redating.rs` builds copies.
fn redate(mut b: Vec<u8>, config: &Config, to: u64) -> Vec<u8> {
    let (u, _) = decode_finality_update(&b, config).unwrap();
    assert!(to > u.attested_header.beacon.slot && to / 8192 == u.signature_slot / 8192, "{to}: not in the period");
    let from = u.signature_slot.to_le_bytes();
    let at: Vec<usize> = b.windows(8).enumerate().filter(|(_, w)| *w == from).map(|(i, _)| i).collect();
    assert_eq!(at.len(), 1, "signature_slot found once in the served bytes");
    b[at[0]..at[0] + 8].copy_from_slice(&to.to_le_bytes());
    let (re, _) = decode_finality_update(&b, config).unwrap();
    assert!(re.signature_slot == to && re.finalized_header == u.finalized_header);
    b
}

/// A job another party opened for U1 is open on the store; the sources now serve a copy of U1 (re-dated: another job
/// id, the same finalized header). The updater does not open the copy's job (it would leave the store where U1's job
/// does, and only one of the two could finish): it joins U1's job, sending its `Miller`, and never pays an `Open`.
#[test]
fn a_copy_of_the_chosen_update_open_on_the_store_is_joined() {
    let fx = Fx::load();
    let c = fx.two_new_finality_updates();
    let (kb, opener) = (key(0x79), key(0x7a));
    let mut l = HarnessLedger::new(&[kb.clone(), opener.clone()]);
    let copy = Arc::new(Mutex::new(None));
    let source =
        CopyFinality { inner: Recorded::open("a", &fx.dir, &fx.config, fx.clock.clone()).unwrap(), copy: copy.clone() };
    let mut b = fx.updater_with("b", kb, None, Box::new(source));
    catch_up(&fx, &mut b, &mut l, c);
    fx.clock.set(c + 1);
    let j1 = match b.plan(&l).unwrap() {
        Plan::Send(x) if x.step == Step::Open => x.job_id,
        p => panic!("b: {}", show(&p)),
    };
    let (u1, _) = decode_finality_update(&fx.entries[c].bytes, &fx.config).unwrap();
    let tx = b.open_unchecked(&l, &u1).unwrap();
    assert!(l.submit(&opener, &tx, 4_800_000, 1_000_000_000).unwrap().applied, "the other party's Open of U1");
    *copy.lock().unwrap() = Some(redate(fx.entries[c].bytes.clone(), &fx.config, u1.signature_slot + 1));
    let mut run = Run::default();
    let first = run.tick(&mut b, &mut l);
    println!("b with U1's job open and a copy of U1 served: {}", show(&first));
    assert!(matches!(&first, Plan::Send(x) if x.step == Step::Miller && x.job_id == j1), "{}", show(&first));
    assert_eq!(b.metrics.jobs_joined, 1);
    for _ in 0..20 {
        if matches!(run.tick(&mut b, &mut l), Plan::UpToDate) {
            break;
        }
    }
    assert_eq!(finalized(&b, &l), fx.entries[c].finalized_slot, "U1 applied");
    assert!(run.sent.iter().all(|s| s.2 != Step::Open), "b paid no Open");
    assert_eq!(run.jobs(), vec![(j1, 2, true)], "U1's job: b sent its Miller and Finish");
}

/// `a` carries U1's job (its `Open` alone applied); `b`, which never saw U1 served, opened U2's job and sent its
/// `Miller`. `a`, about to send U1's `Miller`, has asked its sources what they serve now (a carrier does so in each
/// round), finds U2's job, which leads its own (more steps applied), and waits; U2's job finishes, U1's is abandoned
/// with its `Open` alone.
#[test]
fn a_carrier_finds_a_newer_job_that_leads_its_own_and_waits() {
    let fx = Fx::load();
    let c = fx.two_new_finality_updates();
    let (ka, kb) = (key(0x7b), key(0x7c));
    let mut l = HarnessLedger::new(&[ka.clone(), kb.clone()]);
    let mut a = fx.updater("a", ka, None);
    let mut b = fx.updater("b", kb, None);
    catch_up(&fx, &mut a, &mut l, c);
    let mut run = Run::default();
    fx.clock.set(c + 1);
    let j1 = match run.tick(&mut a, &mut l) {
        Plan::Send(x) if x.step == Step::Open => x.job_id,
        p => panic!("a: {}", show(&p)),
    };
    fx.clock.set(c + 2);
    let j2 = match run.tick(&mut b, &mut l) {
        Plan::Send(x) if x.step == Step::Open => x.job_id,
        p => panic!("b's Open: {}", show(&p)),
    };
    assert!(matches!(run.tick(&mut b, &mut l), Plan::Send(ref x) if x.step == Step::Miller && x.job_id == j2));
    let p = run.tick(&mut a, &mut l);
    println!("a, U1's job with its Open applied, U2's with its Miller: {}", show(&p));
    assert!(matches!(p, Plan::Deferred(Step::Miller)), "{}", show(&p));
    assert!(a.metrics.job_waits >= 1 && a.metrics.last_plan.contains("waiting for it"), "{}", a.metrics.last_plan);
    let lines = settle(&mut run, &mut a, &mut b, &mut l, 100);
    println!("{}", lines.join("\n"));
    assert_eq!(run.sent_by("a", &j1, Step::Miller), 0, "a paid no step of the job behind");
    assert_eq!(run.abandoned().0, vec![(j1, 1)], "U1's job abandoned with its Open alone");
    assert_eq!(finalized(&a, &l), fx.entries[c + 1].finalized_slot, "U2 applied");
}

/// `a` carries U1's job, its `Open` alone applied; `b`, which never saw U1 served, opened U2's job: as many steps. U2 is
/// the update both are served, and its job, applied, leaves U1 superseded, so U2's job leads ([`leads`]): `a`, which
/// knows both, waits; `b`, which cannot find `a`'s job, carries on; U1's job is abandoned with its `Open` alone and no
/// job is paid for U1 again.
#[test]
fn a_carrier_yields_to_a_newer_job_with_as_many_steps() {
    let fx = Fx::load();
    let c = fx.two_new_finality_updates();
    let (ka, kb) = (key(0x7d), key(0x7e));
    let mut l = HarnessLedger::new(&[ka.clone(), kb.clone()]);
    let mut a = fx.updater("a", ka, None);
    let mut b = fx.updater("b", kb, None);
    catch_up(&fx, &mut a, &mut l, c);
    let mut run = Run::default();
    fx.clock.set(c + 1);
    let j1 = match run.tick(&mut a, &mut l) {
        Plan::Send(x) if x.step == Step::Open => x.job_id,
        p => panic!("a: {}", show(&p)),
    };
    fx.clock.set(c + 2);
    assert!(matches!(run.tick(&mut b, &mut l), Plan::Send(ref x) if x.step == Step::Open));
    let p = run.tick(&mut a, &mut l);
    println!("a, U1's job and U2's each with its Open applied: {}", show(&p));
    assert!(matches!(p, Plan::Deferred(Step::Miller)), "{}", show(&p));
    let lines = settle(&mut run, &mut a, &mut b, &mut l, 100);
    println!("{}", lines.join("\n"));
    assert_eq!(run.sent_by("a", &j1, Step::Miller), 0, "a paid no step of U1's job after its Open");
    assert_eq!(run.abandoned().0, vec![(j1, 1)]);
    assert_eq!(run.jobs().iter().filter(|j| j.2).count(), 1, "one job completed: U2's");
    assert_eq!(finalized(&a, &l), fx.entries[c + 1].finalized_slot);
}

/// The in-process chain, failing every read of one account while `fails` is positive, as the sequencer backend does
/// while a block not yet final holds a transaction of it ("the sequencer and the follower disagree").
struct Unreadable {
    inner: HarnessLedger,
    actor: Option<Actor>,
    fails: std::cell::Cell<usize>,
}

impl Ledger for Unreadable {
    fn actor_state(&self, actor: Actor) -> anyhow::Result<Vec<u8>> {
        if Some(actor) == self.actor && self.fails.get() > 0 {
            self.fails.set(self.fails.get() - 1);
            anyhow::bail!("the sequencer and the follower disagree on the state of {actor:?} (injected)");
        }
        self.inner.actor_state(actor)
    }
    fn submit(&mut self, payer: &PrivateKey, tx: &Tx, gas_limit: u64, max_fee: u128) -> anyhow::Result<TxOutcome> {
        self.inner.submit(payer, tx, gas_limit, max_fee)
    }
}

/// A job of a known update whose account cannot be read (its transaction in a block not yet final, on a sequencer) is
/// waited for, as a job that has just moved: the round is not an error (it used to fail the round, and five such
/// rounds in a row would turn `/health` down), and once the job reads again the usual bounds apply.
#[test]
fn an_unreadable_job_on_the_store_is_waited_for_not_an_error() {
    let fx = Fx::load();
    let c = fx.two_new_finality_updates();
    let (kb, opener) = (key(0x81), key(0x82));
    let mut l = Unreadable { inner: HarnessLedger::new(&[kb.clone(), opener.clone()]), actor: None, fails: 0.into() };
    let mut b = fx.updater("b", kb, None);
    fx.clock.set(c);
    for _ in 0..400 {
        if matches!(b.tick(&mut l).unwrap(), Plan::UpToDate) {
            break;
        }
    }
    fx.clock.set(c + 1);
    let j1 = match b.plan(&l).unwrap() {
        Plan::Send(x) if x.step == Step::Open => x.job_id,
        p => panic!("b: {}", show(&p)),
    };
    let (u1, _) = decode_finality_update(&fx.entries[c].bytes, &fx.config).unwrap();
    let tx = b.open_unchecked(&l, &u1).unwrap();
    assert!(l.inner.submit(&opener, &tx, 4_800_000, 1_000_000_000).unwrap().applied);
    fx.clock.set(c + 2);
    l.actor = Some(b.inst.job(&j1));
    l.fails.set(3);
    let mut plans = Vec::new();
    for _ in 0..40 {
        let p = b.tick(&mut l).expect("not an error");
        let done = matches!(p, Plan::Send(ref x) if x.step == Step::Open);
        plans.push(show(&p));
        if done {
            break;
        }
    }
    println!("{plans:?}; last plan {}", b.metrics.last_plan);
    assert_eq!(b.metrics.failed_rounds, 0);
    assert!(plans[..3].iter().all(|p| p == "Deferred(Open)"), "{plans:?}");
    assert_eq!(l.fails.get(), 0, "read again");
    assert_eq!(plans.len() as u64, 3 + b.policy.join_wait() + 1, "then waited as for any job that stops: {plans:?}");
}

/// When a wait ends (the job waited for stops moving), every updater waiting for it would send in the same round: the
/// step's turn starts again for each, so their hashed waits keep them apart. Two updaters, `defer_window` 8, whose
/// `Open` turns on the state have run, both first read a job a party opened and left in the same round; each block is
/// planned against the state before it and settles together, as on a sequencer. Over four pairs of updaters (other
/// names, so other waits), the `Open`s of U2's job are refused as duplicates at most once (equal draws), not once per
/// pair.
#[test]
fn updaters_released_together_take_their_turns_again() {
    let fx = Fx::load();
    let c = fx.two_new_finality_updates();
    let mut refused_opens = 0;
    let mut lines = Vec::new();
    for pair in 0..4u8 {
        let (kb, kc, opener) = (key(0x90 + pair), key(0xa0 + pair), key(0xb0 + pair));
        let mut l = HarnessLedger::new(&[kb.clone(), kc.clone(), opener.clone()]);
        let mk = |label: String, k: PrivateKey| {
            let inst = Instance::new("mainnet", HarnessLedger::program(), fx.config.clone(), fx.checkpoint);
            let policy = Policy { defer_window: 8, retry_base_ms: 0, join_wait_rounds: Some(12), ..Policy::default() };
            let source = Recorded::open("a", &fx.dir, &fx.config, fx.clock.clone()).unwrap();
            let mut u = Updater::new(&label, inst, k, Fees::default(), policy, vec![Box::new(source)], None).unwrap();
            u.sleep = no_sleep;
            u
        };
        let mut b = mk(format!("b{pair}"), kb);
        let mut cc = mk(format!("c{pair}"), kc);
        fx.clock.set(c);
        for _ in 0..400 {
            if matches!(b.tick(&mut l).unwrap(), Plan::UpToDate) {
                break;
            }
        }
        // Both are served U1 for a whole turn (their plans dropped), so that their `Open` turns on this state have run
        // and both first read the party's job in the same round.
        fx.clock.set(c + 1);
        for _ in 0..8 {
            let _ = (b.plan(&l).unwrap(), cc.plan(&l).unwrap());
        }
        let (u1, _) = decode_finality_update(&fx.entries[c].bytes, &fx.config).unwrap();
        let tx = b.open_unchecked(&l, &u1).unwrap();
        assert!(l.submit(&opener, &tx, 4_800_000, 1_000_000_000).unwrap().applied);
        fx.clock.set(c + 2);
        let n0 = l.log.len();
        for _ in 0..120 {
            let pb = b.plan(&l).unwrap();
            let pc = cc.plan(&l).unwrap();
            for (u, p) in [(&mut b, pb), (&mut cc, pc)] {
                if let Plan::Send(a) = p {
                    u.submit(&mut l, &a, false).unwrap();
                }
            }
            if finalized(&b, &l) == fx.entries[c + 1].finalized_slot {
                break;
            }
        }
        let refused = l.log[n0..].iter().filter(|(_, label, o)| label.ends_with("Open") && !o.applied).count();
        lines.push(format!(
            "pair {pair}: U2 applied {}; refused Opens {refused}",
            finalized(&b, &l) == fx.entries[c + 1].finalized_slot
        ));
        assert_eq!(finalized(&b, &l), fx.entries[c + 1].finalized_slot, "pair {pair}: U2 applied");
        refused_opens += refused;
    }
    println!("{}", lines.join("\n"));
    assert!(refused_opens <= 1, "Opens refused as duplicates: {refused_opens} in four pairs");
}

/// A party opens a new copy of U1's job in each of four rounds (copies the updater was served: the same finalized
/// header, other signature slots), while the updater has chosen U2. Copies of one update are one kind of job: a new
/// copy opened is not a move of that kind, so the updater waits `join_wait_rounds` from the first, not until the cap.
#[test]
fn new_copies_of_one_update_opened_each_round_are_not_moves() {
    let fx = Fx::load();
    let c = fx.two_new_finality_updates();
    let (kb, opener) = (key(0x83), key(0x84));
    let mut l = HarnessLedger::new(&[kb.clone(), opener.clone()]);
    let copy = Arc::new(Mutex::new(None));
    let source =
        CopyFinality { inner: Recorded::open("a", &fx.dir, &fx.config, fx.clock.clone()).unwrap(), copy: copy.clone() };
    let mut b = fx.updater_with("b", kb, None, Box::new(source));
    catch_up(&fx, &mut b, &mut l, c);
    fx.clock.set(c + 1);
    // b is served U1 and three copies of it (one a round).
    let (u1, _) = decode_finality_update(&fx.entries[c].bytes, &fx.config).unwrap();
    let mut copies = vec![u1.clone()];
    let _ = b.plan(&l).unwrap();
    for k in 1..4 {
        let bytes = redate(fx.entries[c].bytes.clone(), &fx.config, u1.signature_slot + k);
        *copy.lock().unwrap() = Some(bytes.clone());
        let _ = b.plan(&l).unwrap();
        copies.push(decode_finality_update(&bytes, &fx.config).unwrap().0);
    }
    *copy.lock().unwrap() = None;
    fx.clock.set(c + 2);
    let wait = b.policy.join_wait();
    let mut waited = 0;
    for round in 0..60 {
        if let Some(u) = copies.get(round) {
            let tx = b.open_unchecked(&l, u).unwrap();
            assert!(l.submit(&opener, &tx, 4_800_000, 1_000_000_000).unwrap().applied, "copy {round}'s Open");
        }
        match b.tick(&mut l).unwrap() {
            Plan::Deferred(Step::Open) => waited += 1,
            Plan::Send(x) if x.step == Step::Open => break,
            p => panic!("{}", show(&p)),
        }
    }
    println!("waited {waited} rounds with a new copy of U1's job opened in each of the first four");
    assert_eq!(waited, wait, "join_wait_rounds from the first copy's job");
}

/// Two copies of U1 are open on the store, one with its `Open` applied, the other also its `Miller`; the updater has
/// chosen U1 itself (not open). It joins the copy that leads, the one further on, sending its next step.
#[test]
fn of_two_copies_open_the_one_further_on_is_joined() {
    let fx = Fx::load();
    let c = fx.two_new_finality_updates();
    let (kb, kt, opener) = (key(0x85), key(0x86), key(0x87));
    let mut l = HarnessLedger::new(&[kb.clone(), kt.clone(), opener.clone()]);
    let seen = Arc::new(Mutex::new(None));
    let source =
        CopyFinality { inner: Recorded::open("a", &fx.dir, &fx.config, fx.clock.clone()).unwrap(), copy: seen.clone() };
    let mut b = fx.updater_with("b", kb, None, Box::new(source));
    catch_up(&fx, &mut b, &mut l, c);
    fx.clock.set(c + 1);
    let (u1, _) = decode_finality_update(&fx.entries[c].bytes, &fx.config).unwrap();
    let copy_a = redate(fx.entries[c].bytes.clone(), &fx.config, u1.signature_slot + 1);
    let copy_b = redate(fx.entries[c].bytes.clone(), &fx.config, u1.signature_slot + 2);
    for served in [None, Some(copy_a.clone()), Some(copy_b.clone())] {
        *seen.lock().unwrap() = served;
        let _ = b.plan(&l).unwrap();
    }
    *seen.lock().unwrap() = None;
    // Another party takes copy B through its Miller, and opens copy A.
    let theirs = Arc::new(Mutex::new(Some(copy_b)));
    let src = CopyFinality { inner: Recorded::open("t", &fx.dir, &fx.config, fx.clock.clone()).unwrap(), copy: theirs };
    let mut t = fx.updater_with("t", kt, Some(0), Box::new(src));
    let jb = match t.tick(&mut l).unwrap() {
        Plan::Send(x) if x.step == Step::Open => x.job_id,
        p => panic!("t: {}", show(&p)),
    };
    assert!(matches!(t.tick(&mut l).unwrap(), Plan::Send(ref x) if x.step == Step::Miller && x.job_id == jb));
    let (ua, _) = decode_finality_update(&copy_a, &fx.config).unwrap();
    let tx = b.open_unchecked(&l, &ua).unwrap();
    assert!(l.submit(&opener, &tx, 4_800_000, 1_000_000_000).unwrap().applied, "copy A's Open");
    let p = b.tick(&mut l).unwrap();
    println!("b, U1 chosen, copy A open, copy B through its Miller: {}", show(&p));
    assert!(matches!(&p, Plan::Send(x) if x.job_id == jb && x.step != Step::Miller), "{}", show(&p));
    assert_eq!(b.metrics.jobs_joined, 1);
}

/// A party keeps opening new jobs of a kind that has stopped moving (a lying source serves a new copy of U1 each round,
/// and the party opens the copy's job), while the updater, `defer_window` 8, has chosen U2. Each new job is passed when
/// first read; the wait ends once, its turn starts again once, and its `Open` goes out within `join_wait_rounds` and
/// one turn of its first wait. Four updaters (other names and keys, so other draws): each new job used to restart the
/// turn, so an updater whose draw for U2's `Open` was not 0 sent nothing while the party kept opening.
#[test]
fn new_jobs_of_a_stalled_kind_do_not_hold_the_open_back() {
    let fx = Fx::load();
    let c = fx.two_new_finality_updates();
    let mut lines = Vec::new();
    for n in 0..4u8 {
        let (kb, opener) = (key(0xc0 + n), key(0xd0 + n));
        let mut l = HarnessLedger::new(&[kb.clone(), opener.clone()]);
        let copy = Arc::new(Mutex::new(None));
        let liar = CopyFinality {
            inner: Recorded::open("liar", &fx.dir, &fx.config, fx.clock.clone()).unwrap(),
            copy: copy.clone(),
        };
        let honest = Recorded::open("a", &fx.dir, &fx.config, fx.clock.clone()).unwrap();
        let inst = Instance::new("mainnet", HarnessLedger::program(), fx.config.clone(), fx.checkpoint);
        let policy = Policy { defer_window: 8, retry_base_ms: 0, ..Policy::default() };
        let sources: Vec<Box<dyn Source>> = vec![Box::new(honest), Box::new(liar)];
        let mut b = Updater::new(&format!("b{n}"), inst, kb, Fees::default(), policy, sources, None).unwrap();
        b.sleep = no_sleep;
        catch_up(&fx, &mut b, &mut l, c);
        fx.clock.set(c + 1);
        let _ = b.plan(&l).unwrap();
        let (u1, _) = decode_finality_update(&fx.entries[c].bytes, &fx.config).unwrap();
        let tx = b.open_unchecked(&l, &u1).unwrap();
        assert!(l.submit(&opener, &tx, 4_800_000, 1_000_000_000).unwrap().applied, "the party's Open of U1");
        fx.clock.set(c + 2);
        let (wait, w) = (b.policy.join_wait(), b.policy.defer_window);
        let (mut first_wait, mut sent) = (None, None);
        let mut prev: Option<Vec<u8>> = None;
        for round in 0..150u64 {
            // The copy served the round before: the party opens its job, once U1's kind has stopped moving.
            if let (Some(bytes), true) = (prev.take(), first_wait.is_some_and(|f| round > f + wait)) {
                let (u, _) = decode_finality_update(&bytes, &fx.config).unwrap();
                let tx = b.open_unchecked(&l, &u).unwrap();
                assert!(l.submit(&opener, &tx, 4_800_000, 1_000_000_000).unwrap().applied, "a copy's Open");
            }
            let bytes = redate(fx.entries[c].bytes.clone(), &fx.config, u1.signature_slot + 1 + round);
            *copy.lock().unwrap() = Some(bytes.clone());
            prev = Some(bytes);
            match b.tick(&mut l).unwrap() {
                Plan::Deferred(Step::Open) if b.metrics.job_waits > 0 && first_wait.is_none() => {
                    first_wait = Some(round)
                }
                Plan::Send(x) if x.step == Step::Open => {
                    sent = Some(round);
                    break;
                }
                _ => {}
            }
        }
        lines.push(format!(
            "b{n}: first wait in round {first_wait:?}, U2's Open sent in round {sent:?}; passed {}",
            b.metrics.stalled_jobs_passed
        ));
        let (f, s) = (first_wait.expect("a wait"), sent.unwrap_or(u64::MAX));
        assert!(
            s <= f + wait + w + 1,
            "b{n}: sent {} rounds after the first wait (bound {})\n{lines:?}",
            s.saturating_sub(f),
            wait + w + 1
        );
    }
    println!("{}", lines.join("\n"));
}
