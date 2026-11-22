//! Refused steps (review-code-lc UP-H1). A step LEZ refuses while it stays due is charged (outside its validity
//! window: the whole gas limit), and the updater used to send it again every round. Here: `Init` and `Open` are not
//! sent outside the program's validity window by the chain's clock (its nearest admitted time is sent and applied);
//! a step refused out of gas is charged once and held, `/health` names it, and raising the gas limit releases it; when
//! every round brings a new job that the chain refuses, the breaker bounds the refusals; a refusal that lost a race to
//! another updater is not held. The in-process chain (LEZ v0.4.0-rc1's state machine and fee settlement, dev mode)
//! with its block time set (`HarnessLedger::now`), the lab's recorded mainnet data.
#![cfg(feature = "harness")]

use std::{fs, path::PathBuf};

use ferrata_core::{types::LightClientBootstrap, witness::CompactStore, Root};
use ferrata_lez_wire::Config;
use ferrata_updater::{
    chain::{preset, root},
    config::{Fees, Policy},
    ledger::HarnessLedger,
    source::{lab_record, load_entries, Clock, Entry, Recorded, Source},
    updater::{Instance, Plan, Refusal, Step, Updater, BREAKER_AFTER, BREAKER_MAX_PAUSE, WINDOW_MARGIN_MS},
};
use lee::PrivateKey;

fn key(b: u8) -> PrivateKey {
    PrivateKey::try_new([b; 32]).unwrap()
}

fn no_sleep(_: u64) {}

struct Rec {
    config: Config,
    dir: PathBuf,
    checkpoint: Root,
    entries: Vec<Entry>,
}

fn mainnet() -> Rec {
    let config = preset("mainnet", 2).unwrap();
    let dir = lab_record("mainnet");
    let checkpoint = root(&fs::read_to_string(dir.join("checkpoint")).unwrap()).unwrap();
    let entries = load_entries(&dir.join("updates"), &config).unwrap();
    Rec { config, dir, checkpoint, entries }
}

impl Rec {
    fn source(&self, label: &str, clock: &Clock) -> Box<dyn Source> {
        Box::new(Recorded::open(label, &self.dir, &self.config, clock.clone()).unwrap())
    }
    fn updater(
        &self,
        label: &str,
        payer: PrivateKey,
        sources: Vec<Box<dyn Source>>,
        state: Option<PathBuf>,
    ) -> Updater {
        let inst = Instance::new("mainnet", HarnessLedger::program(), self.config.clone(), self.checkpoint);
        let policy = Policy { defer_window: 1, retry_base_ms: 0, ..Policy::default() };
        let mut u = Updater::new(label, inst, payer, Fees::default(), policy, sources, state).unwrap();
        u.sleep = no_sleep;
        u
    }
    fn checkpoint_slot(&self) -> u64 {
        let b = fs::read(self.dir.join("bootstrap.ssz")).unwrap();
        LightClientBootstrap::decode(&b, self.config.fork().unwrap(), 512).unwrap().header.beacon.slot
    }
}

fn store(u: &Updater, l: &HarnessLedger) -> CompactStore {
    u.inst.store(l).unwrap().expect("active instance")
}

fn settle(u: &mut Updater, l: &mut HarnessLedger, max: usize) {
    for _ in 0..max {
        match u.tick(l).unwrap() {
            Plan::UpToDate => return,
            Plan::Stuck(w) | Plan::Waiting(w) => panic!("{w}"),
            _ => {}
        }
    }
    panic!("not up to date after {max} rounds");
}

fn scratch(name: &str) -> PathBuf {
    let base = std::env::var_os("CARGO_TARGET_TMPDIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let d = base.join(format!("refusals-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// Refused (charged, not applied) transactions in `l` from index `from`.
fn refused(l: &HarnessLedger, from: usize) -> usize {
    l.log[from..].iter().filter(|(_, _, o)| !o.applied && !o.rejected).count()
}

/// P1. The chain's clock at the last time `Init` is still sent (its window, [`WINDOW_MARGIN_MS`] before the end of the
/// checkpoint's weak-subjectivity period, is open): sent and applied. At the margin and past the end: nothing is sent
/// in five rounds, nothing charged, `/health` degraded naming why (the review: five `Init`s refused, 38,415,184 each).
#[test]
fn init_is_not_sent_outside_its_window() {
    let r = mainnet();
    let clock = Clock::new(r.entries.len());
    let ws_end = r.config.ws_end_ms(r.checkpoint_slot()).unwrap();
    for (now, sent) in
        [(ws_end - WINDOW_MARGIN_MS - 1, true), (ws_end - WINDOW_MARGIN_MS, false), (ws_end + 1_000, false)]
    {
        let payer = key(0x41);
        let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
        l.now = Some(now);
        let before = l.balance(&payer);
        let mut u = r.updater("init", payer.clone(), vec![r.source("a", &clock)], None);
        let plans: Vec<Plan> = (0..5).map(|_| u.tick(&mut l).unwrap()).collect();
        let (status, why) = u.health();
        println!(
            "Init with the chain's clock {} ms from the window's end: {} transactions, charged {}; last plan {}; health {status} {why:?}",
            now as i128 - ws_end as i128,
            l.log.len(),
            before - l.balance(&payer),
            u.metrics.last_plan
        );
        if sent {
            assert!(l.log.iter().any(|(_, label, o)| label.ends_with("Init") && o.applied), "sent and applied");
            assert_eq!(refused(&l, 0), 0);
        } else {
            assert!(l.log.is_empty(), "nothing sent");
            assert_eq!(l.balance(&payer), before, "nothing charged");
            assert!(plans.iter().all(|p| matches!(p, Plan::Stuck(w) if w.contains("weak-subjectivity"))));
            assert_eq!(status, "degraded");
            assert!(why.iter().any(|w| w.contains("Init not sent")), "{why:?}");
        }
    }
}

/// P2. An instance whose finalized header leaves its weak-subjectivity period by the chain's clock: no `Open` is sent
/// at the margin or past the end (the review: five refused, 38,428,328 each, `/health` ok); at the nearest admitted
/// time it is sent and the instance advances.
#[test]
fn open_is_not_sent_outside_its_window() {
    let r = mainnet();
    let clock = Clock::new(3);
    let payer = key(0x42);
    let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
    let mut u = r.updater("open", payer.clone(), vec![r.source("a", &clock)], None);
    settle(&mut u, &mut l, 60);
    let fin = store(&u, &l).finalized.slot;
    let ws_end = r.config.ws_end_ms(fin).unwrap();
    clock.set(4);
    for now in [ws_end + 1_000, ws_end - WINDOW_MARGIN_MS] {
        l.now = Some(now);
        let (n0, before) = (l.log.len(), l.balance(&payer));
        let plans: Vec<Plan> = (0..5).map(|_| u.tick(&mut l).unwrap()).collect();
        let (status, why) = u.health();
        println!(
            "Open with the chain's clock {} ms from the window's end: {} sent, charged {}; health {status} {why:?}",
            now as i128 - ws_end as i128,
            l.log.len() - n0,
            before - l.balance(&payer)
        );
        assert_eq!(l.log.len(), n0, "nothing sent");
        assert_eq!(l.balance(&payer), before, "nothing charged");
        assert!(plans.iter().all(|p| matches!(p, Plan::Stuck(w) if w.starts_with("Open not sent"))));
        assert_eq!(status, "degraded");
        assert!(why.iter().any(|w| w.contains("weak-subjectivity")), "{why:?}");
    }
    l.now = Some(ws_end - WINDOW_MARGIN_MS - 1);
    let n0 = l.log.len();
    settle(&mut u, &mut l, 30);
    let moved = store(&u, &l).finalized.slot;
    println!(
        "at {} ms from the end: {} sent, finalized slot {fin} -> {moved}",
        -(WINDOW_MARGIN_MS as i64) - 1,
        l.log.len() - n0
    );
    assert!(moved > fin);
    assert_eq!(refused(&l, 0), 0);
}

/// P9. A gas limit below what `Adopt` needs: the step is refused (charged) once, then held: in ten rounds one
/// refusal and no charge after it (the review: refused in every round); `/health` says what to change; the journal
/// keeps the chain's reason. Raising the gas limit releases it and the instance catches up.
#[test]
fn a_step_refused_out_of_gas_is_not_charged_again() {
    let r = mainnet();
    let clock = Clock::new(r.entries.len());
    let payer = key(0x43);
    let dir = scratch("gas");
    let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
    let mut u = r.updater("gas", payer.clone(), vec![r.source("a", &clock)], Some(dir.clone()));
    u.fees.gas_limit = 1_000_000;
    let mut balances = Vec::new();
    let mut plans = Vec::new();
    for _ in 0..10 {
        plans.push(match u.tick(&mut l).unwrap() {
            Plan::Send(a) => format!("send {}", a.step.name()),
            Plan::Stuck(w) => format!("stuck: {w}"),
            p => format!("{p:?}"),
        });
        balances.push(l.balance(&payer));
    }
    let (status, why) = u.health();
    println!("gas_limit 1,000,000, ten rounds: {plans:#?}\nbalances {balances:?}\nhealth {status} {why:?}");
    assert_eq!(refused(&l, 0), 1, "one refusal in ten rounds, not one a round");
    let (_, label, out) = l.log.iter().find(|(_, _, o)| !o.applied).unwrap();
    assert!(label.ends_with("adopt 1/2") && Refusal::of(out) == Refusal::Gas, "{label} {out:?}");
    let at = l.log.iter().position(|(_, _, o)| !o.applied).unwrap();
    assert!(balances[at..].windows(2).all(|w| w[0] == w[1]), "nothing charged after the refusal");
    assert_eq!(u.held.len(), 1);
    assert!(u.held.values().all(|h| h.cause == Refusal::Gas && h.step == Step::Adopt(0) && h.count == 1));
    assert_eq!(status, "degraded");
    assert!(why.iter().any(|w| w.contains("out of gas") && w.contains("raise it")), "{why:?}");
    let journal = fs::read_to_string(dir.join("mainnet/journal.tsv")).unwrap();
    let line = journal.lines().find(|l| l.contains("\trefused\t")).unwrap();
    println!("journal: {line}");
    assert_eq!(line.split('\t').count(), 13);
    assert!(line.split('\t').nth(12).unwrap().contains("cycle budget"));
    assert_eq!(
        u.render_metrics().lines().find(|m| m.starts_with("ferrata_updater_refusals_total")).map(|m| m.ends_with(" 1")),
        Some(true)
    );

    u.fees.gas_limit = Fees::default().gas_limit;
    settle(&mut u, &mut l, 80);
    println!("gas_limit {}: caught up to {}", u.fees.gas_limit, store(&u, &l).finalized.slot);
    assert!(u.held.is_empty());
    assert_eq!(refused(&l, 0), 1);
}

/// Every round brings a new finality update, so a new job, and the chain refuses each one's `Open` (a gas limit below
/// what step 1 needs, standing for any refusal no check here foresees): a hold per job cannot stop that, the breaker
/// does: [`BREAKER_AFTER`] refusals, a pause, one step tried, a longer pause. Then the cause goes (the limit raised): the
/// step tried after the pause is applied and the instance advances.
#[test]
fn the_breaker_bounds_refusals_when_every_round_brings_a_new_job() {
    let r = mainnet();
    let clock = Clock::new(3);
    let payer = key(0x44);
    let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
    let mut u = r.updater("breaker", payer.clone(), vec![r.source("a", &clock)], None);
    settle(&mut u, &mut l, 60);
    let start = store(&u, &l).finalized.slot;
    u.fees.gas_limit = 1_000_000;
    let n0 = l.log.len();
    let rounds = 40;
    let mut paused = 0;
    let mut jobs = std::collections::BTreeSet::new();
    for i in 0..rounds {
        clock.set((4 + i).min(r.entries.len()));
        match u.tick(&mut l).unwrap() {
            Plan::Send(a) => {
                jobs.insert(a.job_id);
            }
            Plan::Stuck(w) if w.starts_with("sending paused") => paused += 1,
            _ => {}
        }
    }
    let (n, sent) = (refused(&l, n0), l.log.len() - n0);
    let (status, why) = u.health();
    println!(
        "{rounds} rounds, a new update each: {sent} sent, {n} refused ({} distinct jobs), {paused} rounds paused by the \
         breaker; health {status} {why:?}",
        jobs.len()
    );
    // Three in a row, a pause of 8 rounds, one tried, a pause of 16, one tried, a pause of 32: five in 40 rounds.
    assert_eq!(sent, n, "nothing applied: every Open needs more gas");
    assert!(n >= BREAKER_AFTER as usize && n <= BREAKER_AFTER as usize + 2, "{n} refusals");
    assert!(paused >= 10, "{paused}");
    assert_ne!(status, "ok");
    assert!(why.iter().any(|w| w.contains("sending paused")), "{why:?}");

    u.fees.gas_limit = Fees::default().gas_limit;
    let mut waited = 0;
    loop {
        waited += 1;
        assert!(waited < BREAKER_MAX_PAUSE as usize + 100, "the breaker never closed");
        if matches!(u.tick(&mut l).unwrap(), Plan::UpToDate) {
            break;
        }
    }
    let end = store(&u, &l).finalized.slot;
    println!(
        "gas limit raised: up to date after {waited} more rounds, finalized slot {start} -> {end}; breaker {:?}",
        u.breaker
    );
    assert!(end > start);
    assert_eq!(u.breaker.strikes, 0);
    assert_eq!(refused(&l, n0), n, "no refusal after the cause went");
}

/// Two updaters send the same `Open` against the same state; the second is refused (207, the job exists) because the
/// first came first: a race, so it is not held and does not count towards the breaker, and the second updater goes on
/// with the job's next step.
#[test]
fn a_refusal_that_lost_a_race_is_not_held() {
    let r = mainnet();
    let clock = Clock::new(3);
    let (k1, k2) = (key(0x45), key(0x46));
    let mut l = HarnessLedger::new(&[k1.clone(), k2.clone()]);
    let mut u1 = r.updater("race-1", k1, vec![r.source("a", &clock)], None);
    let mut u2 = r.updater("race-2", k2, vec![r.source("b", &clock)], None);
    settle(&mut u1, &mut l, 60);
    clock.set(4);
    let send = |u: &mut Updater, l: &HarnessLedger| match u.plan(l).unwrap() {
        Plan::Send(a) => a,
        p => panic!("{p:?}"),
    };
    let (a1, a2) = (send(&mut u1, &l), send(&mut u2, &l));
    assert_eq!((a1.step, a1.job_id), (Step::Open, a2.job_id));
    assert!(u1.submit(&mut l, &a1, false).unwrap().unwrap().applied);
    let o2 = u2.submit(&mut l, &a2, false).unwrap().unwrap();
    println!(
        "the second Open: applied {} exit {:?} note {:?}; refusals {:?}",
        o2.applied, o2.exit_code, o2.note, u2.metrics.refusals
    );
    assert!(!o2.applied && !o2.rejected);
    assert!(u2.held.is_empty(), "a race is not held");
    assert_eq!(u2.breaker.strikes, 0);
    assert_eq!(u2.metrics.refusals.get(&("program", "race")), Some(&1));
    assert_eq!(send(&mut u2, &l).step, Step::Miller);
}
