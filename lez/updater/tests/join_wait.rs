//! Join or wait (`policy.join_wait_rounds`, RFP-021 R.9): a job finishes only on the store it began on, so two jobs
//! open on one store pay the steps of the one that does not finish for nothing (on a running sequencer, gate runs G6
//! and G7: 127.0M and 72.7M execution gas in jobs opened, then overtaken). An updater does not open a job while another
//! job is open on the instance's store and moving, and does not send a step of a job that another open job leads; it
//! waits for that job, at most `join_wait_rounds` rounds after the job last moved. On the lab's recorded mainnet chain,
//! two updaters on one in-process LEZ chain.
#![cfg(feature = "harness")]

use std::fs;

use ferrata_core::Root;
use ferrata_lez_wire::Config;
use ferrata_updater::{
    chain::{decode_finality_update, preset, root},
    config::{Fees, Policy},
    ledger::{HarnessLedger, Ledger},
    source::{lab_record, load_entries, Clock, Entry, Recorded},
    updater::{leads, Instance, Plan, Step, Updater},
};
use lee::PrivateKey;

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
        let inst = Instance::new("mainnet", HarnessLedger::program(), self.config.clone(), self.checkpoint);
        let policy = Policy { defer_window: 1, retry_base_ms: 0, join_wait_rounds, ..Policy::default() };
        let source = Recorded::open("a", &self.dir, &self.config, self.clock.clone()).unwrap();
        let mut u = Updater::new(label, inst, payer, Fees::default(), policy, vec![Box::new(source)], None).unwrap();
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

/// Both updaters at the same pace (a round each, in turn) until both have nothing to send, at most `max` rounds each.
fn settle(run: &mut Run, a: &mut Updater, b: &mut Updater, l: &mut HarnessLedger, max: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for round in 0..max {
        let pa = run.tick(a, l);
        let pb = run.tick(b, l);
        let show = |p: &Plan| match p {
            Plan::Send(x) => format!("send {} of 0x{}", x.step.name(), &hex::encode(x.job_id)[..8]),
            p => format!("{p:?}"),
        };
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
        lines.push(format!("join_wait_rounds {policy:?}: b's first round with U2 served: {first:?}"));
        if policy.is_none() {
            assert!(matches!(first, Plan::Deferred(Step::Open)), "{first:?}");
            assert_eq!(b.metrics.job_waits, 1);
            assert!(b.metrics.last_plan.contains("waiting for it"), "{}", b.metrics.last_plan);
            assert!(b.render_metrics().contains("ferrata_updater_waits_for_other_jobs_total{daemon=\"b\",chain=\"mainnet\"} 1"));
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
    println!("waited {waited} rounds; then {:?}", &plans[waited as usize..]);
    assert_eq!(waited, wait, "held back exactly join_wait_rounds rounds");
    assert_eq!(b.metrics.job_waits, wait);
    assert_eq!(b.metrics.stalled_jobs_passed, 1);
    assert!(matches!(plans.last(), Some(Plan::UpToDate)));
    assert_eq!(finalized(&b, &l), fx.entries[c + 1].finalized_slot, "U2 applied after the wait");
}

/// Two jobs opened in one block (each updater planned against a store with no job open): the one that does not lead
/// ([`leads`]: as many steps, the later update) is not continued; the leading job finishes, the other is abandoned
/// with its `Open` alone, and its update is opened again on the new store. `a`, busy with its own job, learns U2 from
/// its sources and finds `b`'s job, which does not lead.
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
    assert_eq!(abandoned, vec![(open2.job_id, 1)], "U2's first job abandoned with its Open alone");
    assert_eq!(run.sent_by("b", &open2.job_id, Step::Miller), 0, "b never paid a step of the job behind");
    assert!(gas > 0 && gas < 4_800_000, "one Open's gas: {gas}");
    assert!(b.metrics.job_waits >= 1);
    assert_eq!(finalized(&a, &l), fx.entries[c + 1].finalized_slot, "U2 applied on the store U1's job left");
    let jobs = run.jobs();
    assert_eq!(jobs.iter().filter(|j| j.2).count(), 2, "U1's job and U2's second job completed: {jobs:?}");
}

/// [`leads`] is a total order: an open job leads one not opened; more steps lead; at as many, the lower-ranked update
/// (as a rule the older); then the greater id.
#[test]
fn which_job_leads() {
    use std::cmp::Reverse;
    let rank = |fin: u64, sig: u64| (false, fin, Reverse(sig), 400usize);
    let (x, y) = ([1u8; 32], [2u8; 32]);
    assert!(leads(&(1, rank(10, 11), x), &(0, rank(20, 21), y)), "an open job leads one not opened");
    assert!(leads(&(2, rank(20, 21), x), &(1, rank(10, 11), y)), "more steps lead");
    assert!(leads(&(1, rank(10, 11), x), &(1, rank(20, 21), y)), "at as many steps, the older update");
    assert!(leads(&(1, (false, 20, Reverse(25), 400), x), &(1, (false, 20, Reverse(21), 400), y)), "the later signature");
    assert!(leads(&(1, (false, 20, Reverse(21), 400), x), &(1, (true, 10, Reverse(11), 400), y)), "not adopting first");
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
/// so the per-job bound alone would hold the updater back for as long as the party keeps opening; the cap
/// (`JOIN_WAIT_CAP` times `join_wait_rounds` from the first round it waited to send a step) lets it send after that
/// many rounds, while the party is still opening, and the instance advances.
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
        lines.push(format!("round {round}: clock {t}; {p:?}; waits {}", b.metrics.job_waits));
        if first_send.is_none() && matches!(p, Plan::Send(ref x) if x.step == Step::Open) {
            first_send = Some(round);
        }
        party_opens(&b, &mut l, t);
    }
    println!("{}\nthe party's applied Opens {opened_by_party}", lines.join("\n"));
    assert_eq!(first_send, Some(cap + 1), "the updater's Open sent once it had waited the cap");
    assert!(b.metrics.wait_caps >= 1);
    assert!(finalized(&b, &l) > start, "the instance advanced while the party kept opening jobs");
}
