//! Refused steps (review-code-lc UP-H1). A step LEZ refuses while it stays due is charged (outside its validity
//! window: the whole gas limit), and the updater used to send it again every round. Here: `Init` and `Open` are not
//! sent outside the program's validity window by the chain's clock (its nearest admitted time is sent and applied);
//! a step refused out of gas is charged once and held, `/health` names it, and raising the gas limit releases it; when
//! every round brings a new job that the chain refuses, the breaker bounds the refusals; a refusal that lost a race to
//! another updater is not held. The in-process chain (LEZ v0.4.0-rc1's state machine and fee settlement, dev mode)
//! with its block time set (`HarnessLedger::now`), the lab's recorded mainnet data.
#![cfg(feature = "harness")]

use std::{
    cell::Cell,
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use ferrata_core::{
    types::{LightClientBootstrap, LightClientUpdate},
    witness::CompactStore,
    Root,
};
use ferrata_lez_wire::Config;
use ferrata_updater::{
    chain::{decode_finality_update, preset, root},
    config::{Fees, Policy},
    ledger::{HarnessLedger, Ledger, Tx, TxOutcome},
    source::{lab_record, load_entries, Clock, Entry, Recorded, Source},
    updater::{
        Instance, Plan, Refusal, Step, Updater, BREAKER_AFTER, BREAKER_MAX_PAUSE, FUTURE_MARGIN_MS, HOLD_ROUNDS,
        MAX_REFUSAL_BACKOFF, REFUSAL_BACKOFF, WINDOW_MARGIN_MS,
    },
};
use lee::{Actor, PrivateKey};

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

fn settle(u: &mut Updater, l: &mut dyn Ledger, max: usize) {
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
/// in five rounds, nothing charged, `/health` `down` naming why (the review: five `Init`s refused, 38,415,184 each;
/// the review of the fixes, M1: degraded, HTTP 200, although nothing can be sent until a person acts).
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
            assert_eq!(status, "down");
            assert!(why.iter().any(|w| w.contains("Init not sent") && w.contains("newer checkpoint")), "{why:?}");
        }
    }
}

/// P2. An instance whose finalized header leaves its weak-subjectivity period by the chain's clock: no `Open` is sent
/// at the margin or past the end, and `/health` says `down` (the review: five refused, 38,428,328 each, `/health` ok;
/// the review of the fixes, M1: degraded, HTTP 200); at the nearest admitted time it is sent, the instance advances
/// and `/health` is `ok` again.
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
        assert_eq!(status, "down");
        assert!(
            why.iter().any(|w| w.contains("weak-subjectivity") && w.contains("initialise a new instance")),
            "{why:?}"
        );
    }
    l.now = Some(ws_end - WINDOW_MARGIN_MS - 1);
    let n0 = l.log.len();
    settle(&mut u, &mut l, 30);
    let moved = store(&u, &l).finalized.slot;
    println!(
        "at {} ms from the end: {} sent, finalized slot {fin} -> {moved}; health {:?}",
        -(WINDOW_MARGIN_MS as i64) - 1,
        l.log.len() - n0,
        u.health()
    );
    assert!(moved > fin);
    assert_eq!(refused(&l, 0), 0);
    assert!(u.expired.is_none());
    assert_eq!(u.health().0, "ok");
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

/// A chain shared by two updaters, so that a source of one can let the other's transactions land while it plans.
#[derive(Clone)]
struct Shared(Arc<Mutex<HarnessLedger>>);

impl Ledger for Shared {
    fn actor_state(&self, actor: Actor) -> anyhow::Result<Vec<u8>> {
        self.0.lock().unwrap().actor_state(actor)
    }
    fn submit(&mut self, payer: &PrivateKey, tx: &Tx, gas_limit: u64, max_fee: u128) -> anyhow::Result<TxOutcome> {
        self.0.lock().unwrap().submit(payer, tx, gas_limit, max_fee)
    }
    fn now_ms(&self) -> Option<u64> {
        self.0.lock().unwrap().now_ms()
    }
}

/// A source that, the first time it is asked for the finality update, lets another updater apply that update's whole
/// job on the shared chain (a faster party), then answers.
struct Overtaken {
    inner: Box<dyn Source>,
    other: Option<(Updater, Shared)>,
}

impl Source for Overtaken {
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
        if let Some((mut other, mut chain)) = self.other.take() {
            for _ in 0..20 {
                if matches!(other.tick(&mut chain)?, Plan::UpToDate) {
                    break;
                }
            }
        }
        self.inner.finality_update()
    }
}

/// UP-L5 (RFP-021 R.9, the submitter re-checks before sending): while this updater asks its sources, another applies
/// the same update; the `Open` planned against the store read before is not sent, because the round reads the chain
/// again just before sending; nothing is charged. Sent anyway, the chain refuses it.
#[test]
fn a_step_made_moot_while_planning_is_not_sent() {
    let r = mainnet();
    let clock = Clock::new(3);
    let (slow, fast) = (key(0x47), key(0x48));
    let chain = Shared(Arc::new(Mutex::new(HarnessLedger::new(&[slow.clone(), fast.clone()]))));
    let mut other = r.updater("fast", fast, vec![r.source("b", &clock)], None);
    let mut c = chain.clone();
    for _ in 0..60 {
        if matches!(other.tick(&mut c).unwrap(), Plan::UpToDate) {
            break;
        }
    }
    clock.set(4);
    let source = Overtaken { inner: r.source("a", &clock), other: Some((other, chain.clone())) };
    let mut u = r.updater("slow", slow.clone(), vec![Box::new(source)], None);
    let (n0, before) = {
        let l = chain.0.lock().unwrap();
        (l.log.len(), l.balance(&slow))
    };
    let plan = u.tick(&mut c).unwrap();
    let (others, mine, after) = {
        let l = chain.0.lock().unwrap();
        let mine = l.log[n0..].iter().filter(|(p, ..)| *p == u.payer_id).count();
        (l.log.len() - n0 - mine, mine, l.balance(&slow))
    };
    println!(
        "while this updater planned, the other sent {others}; this one planned {}, sent {mine}, charged {}; steps done by \
         others {}",
        match &plan {
            Plan::Send(a) => a.tx.label.clone(),
            p => format!("{p:?}"),
        },
        before - after,
        u.metrics.skipped_done
    );
    let Plan::Send(a) = &plan else { panic!("{plan:?}") };
    assert_eq!(a.step, Step::Open);
    assert!(others >= 3, "the other updater's job landed meanwhile");
    assert_eq!(mine, 0, "nothing sent");
    assert_eq!(after, before, "nothing charged");
    assert_eq!(u.metrics.skipped_done, 1);
    let out = u.submit(&mut c, a, false).unwrap().unwrap();
    println!("sent anyway: applied {} exit {:?} fee {}", out.applied, out.exit_code, out.fee);
    assert!(!out.applied);
}

/// The in-process chain, refusing without executing it (nothing charged) each of the next `refuse` transactions whose
/// label ends with `suffix`, with no exit code and a note that names neither the window nor the gas: a refusal of a
/// cause the updater does not know ([`Refusal::Other`]). After each refusal the next `unreadable` reads fail.
struct Flaky {
    inner: HarnessLedger,
    suffix: &'static str,
    refuse: usize,
    refused: usize,
    unreadable: usize,
    fail: Cell<usize>,
}

impl Flaky {
    fn new(inner: HarnessLedger, suffix: &'static str) -> Self {
        Self { inner, suffix, refuse: 0, refused: 0, unreadable: 0, fail: Cell::new(0) }
    }
}

impl Ledger for Flaky {
    fn actor_state(&self, actor: Actor) -> anyhow::Result<Vec<u8>> {
        if self.fail.get() > 0 {
            self.fail.set(self.fail.get() - 1);
            anyhow::bail!("actor state unavailable");
        }
        self.inner.actor_state(actor)
    }
    fn submit(&mut self, payer: &PrivateKey, tx: &Tx, gas_limit: u64, max_fee: u128) -> anyhow::Result<TxOutcome> {
        if self.refuse > 0 && tx.label.ends_with(self.suffix) {
            self.refuse -= 1;
            self.refused += 1;
            self.fail.set(self.unreadable);
            return Ok(TxOutcome {
                note: "Program violated execution rules".into(),
                gas_exec: 1,
                ..Default::default()
            });
        }
        self.inner.submit(payer, tx, gas_limit, max_fee)
    }
    fn now_ms(&self) -> Option<u64> {
        self.inner.now_ms()
    }
}

/// Probe A of the review of these fixes (L3): a `Miller` refused three times for a cause the updater does not know,
/// after which the chain would accept it, while no newer update is served. It used to be held for good after the
/// third refusal (sent again only after 4,096 rounds, a move of the store or a restart: 286 rounds of 300 stuck, the
/// instance unmoved). Now it is sent again after every hold (4, 8, 16 rounds, doubling up to 64), the breaker bounding
/// the refusals in between: the fourth attempt lands, the job is finished and `/health` is `ok` again.
#[test]
fn a_refusal_of_unknown_cause_is_sent_again_until_it_lands() {
    let r = mainnet();
    let clock = Clock::new(3);
    let payer = key(0x49);
    let mut l = Flaky::new(HarnessLedger::new(std::slice::from_ref(&payer)), "Miller");
    let mut u = r.updater("unknown", payer, vec![r.source("a", &clock)], None);
    settle(&mut u, &mut l, 60);
    let start = store(&u, &l.inner).finalized.slot;
    clock.set(4);
    l.refuse = 3;
    let (mut millers, mut moved, mut rounds) = (Vec::<u64>::new(), None, 0u64);
    // (refusals so far, the hold's length in rounds) after each refusal.
    let mut holds = Vec::new();
    for round in 1..=300u64 {
        rounds = round;
        let p = u.tick(&mut l).unwrap();
        if matches!(&p, Plan::Send(a) if a.step == Step::Miller) {
            millers.push(round);
        }
        let now = u.metrics.rounds;
        holds.extend(u.held.values().filter(|h| h.round == now).map(|h| (h.count, h.retry_at.map(|at| at - h.round))));
        if moved.is_none() && store(&u, &l.inner).finalized.slot != start {
            moved = Some(round);
        }
        if moved.is_some() && matches!(p, Plan::UpToDate) {
            break;
        }
    }
    let end = store(&u, &l.inner).finalized.slot;
    println!(
        "a Miller refused {} times for an unknown cause: sent in rounds {millers:?}; holds (refusals, rounds held) \
         {holds:?}; breaker {:?}; finalized slot {start} -> {end}, moved in round {moved:?}, up to date in round {rounds}; \
         refusals {:?}; health {:?}",
        l.refused,
        u.breaker,
        u.metrics.refusals,
        u.health()
    );
    assert_eq!(l.refused, 3);
    assert_eq!(millers.len(), 4, "three refused, the fourth sent and applied: {millers:?}");
    // Held 4, 8, then 16 rounds; never past MAX_REFUSAL_BACKOFF.
    let gaps: Vec<u64> = millers.windows(2).map(|w| w[1] - w[0]).collect();
    assert_eq!(gaps, [REFUSAL_BACKOFF, 2 * REFUSAL_BACKOFF, 4 * REFUSAL_BACKOFF], "{millers:?}");
    assert_eq!(holds, [(1, Some(REFUSAL_BACKOFF)), (2, Some(2 * REFUSAL_BACKOFF)), (3, Some(4 * REFUSAL_BACKOFF))]);
    assert!(holds.iter().all(|(_, held)| held.is_some_and(|n| n <= MAX_REFUSAL_BACKOFF)));
    assert!(end > start, "the instance moved");
    assert!(moved.is_some_and(|m| m <= 2 * MAX_REFUSAL_BACKOFF), "{moved:?}");
    assert_eq!(u.metrics.refusals.get(&("other", "due")), Some(&3));
    assert!(u.held.is_empty());
    assert_eq!(u.health().0, "ok", "{:?}", u.health());
}

/// One updater's view of a chain shared with another, where the next `fails` reads after one of its transactions is
/// refused fail, as the sequencer backend's do while another party's transaction touching the instance is in flight
/// (review UP-L3).
struct ReadFailsAfterRefusal {
    chain: Shared,
    fails: usize,
    fail: Cell<usize>,
}

impl Ledger for ReadFailsAfterRefusal {
    fn actor_state(&self, actor: Actor) -> anyhow::Result<Vec<u8>> {
        if self.fail.get() > 0 {
            self.fail.set(self.fail.get() - 1);
            anyhow::bail!("actor state unavailable: a transaction is in flight");
        }
        self.chain.actor_state(actor)
    }
    fn submit(&mut self, payer: &PrivateKey, tx: &Tx, gas_limit: u64, max_fee: u128) -> anyhow::Result<TxOutcome> {
        let out = self.chain.submit(payer, tx, gas_limit, max_fee)?;
        if !out.applied && !out.rejected {
            self.fail.set(self.fails);
        }
        Ok(out)
    }
    fn now_ms(&self) -> Option<u64> {
        self.chain.now_ms()
    }
}

/// Two updaters send the same step; the faster lands it, the slower is refused, and the slower's view of the chain
/// fails `fails` reads after the refusal. The races, and the slower updater.
fn races_with_failing_reads(fails: usize, payers: (u8, u8)) -> (usize, Updater, Vec<String>) {
    let r = mainnet();
    let clock = Clock::new(3);
    let (k1, k2) = (key(payers.0), key(payers.1));
    let chain = Shared(Arc::new(Mutex::new(HarnessLedger::new(&[k1.clone(), k2.clone()]))));
    let mut c1 = chain.clone();
    let mut c2 = ReadFailsAfterRefusal { chain: chain.clone(), fails, fail: Cell::new(0) };
    let mut u1 = r.updater("fast", k1, vec![r.source("a", &clock)], None);
    let mut u2 = r.updater("slow", k2, vec![r.source("b", &clock)], None);
    settle(&mut u1, &mut c1, 60);
    let (mut races, mut lines) = (0, Vec::new());
    for t in 4..=r.entries.len().min(12) {
        clock.set(t);
        let p1 = u1.plan(&c1).unwrap();
        let p2 = u2.plan(&c2).unwrap();
        if let (Plan::Send(a1), Plan::Send(a2)) = (&p1, &p2) {
            if (a1.job_id, a1.step) == (a2.job_id, a2.step) {
                assert!(u1.submit(&mut c1, a1, false).unwrap().unwrap().applied);
                let o2 = u2.submit(&mut c2, a2, false).unwrap().unwrap();
                assert!(!o2.applied && !o2.rejected, "{o2:?}");
                races += 1;
                lines.push(format!(
                    "t {t}: both sent {}; the slower refused (exit {:?}); its strikes {}, held {}, failing sends {}",
                    a1.step.name(),
                    o2.exit_code,
                    u2.breaker.strikes,
                    u2.held.len(),
                    u2.metrics.failing_sends
                ));
            }
        }
        for _ in 0..30 {
            if matches!(u1.tick(&mut c1).unwrap(), Plan::UpToDate) {
                break;
            }
        }
        let _ = u2.tick(&mut c2).unwrap();
    }
    (races, u2, lines)
}

/// Probe B of the review of these fixes (L4): two updaters, the faster lands each `Open` first, the slower is refused
/// (207, the job exists) and its first read after the refusal fails. That read used to count as "still due": 4 races
/// classed `due`, 4 failing sends, the breaker tripped twice while the other updater did the work. Now the chain is
/// read again (up to `max_attempts`, the transport backoff between): every one is a race, nothing held, no strike, no
/// failing send, `/health` says nothing of it.
#[test]
fn a_failed_read_after_a_lost_race_is_read_again() {
    let (races, u, lines) = races_with_failing_reads(1, (0x4a, 0x4b));
    let (status, why) = u.health();
    println!(
        "{}\n{races} races, each followed by one failed read: refusals {:?}; breaker {:?}; failing sends {}; health \
         {status} {why:?}",
        lines.join("\n"),
        u.metrics.refusals,
        u.breaker,
        u.metrics.failing_sends
    );
    assert!(races >= 4, "{races}");
    assert_eq!(u.metrics.refusals.get(&("program", "race")), Some(&(races as u64)));
    assert_eq!(u.metrics.refusals.len(), 1, "{:?}", u.metrics.refusals);
    assert_eq!((u.breaker.strikes, u.breaker.trips, u.breaker.paused_until), (0, 0, 0));
    assert_eq!(u.metrics.failing_sends, 0);
    assert!(u.held.is_empty());
    assert!(!why.iter().any(|w| w.contains("did not land") || w.contains("sending paused")), "{why:?}");
}

/// The same with the chain unreadable after the refusal for every attempt (`max_attempts` reads fail): whether the
/// step stayed due is not known then, so it is held whatever the cause (`after="unread"`), without a strike or a
/// failing send; the next round reads the chain, finds the other updater's transaction applied, and judges it a race:
/// the hold dropped, still no strike and no failing send (9c L-U3), and the slower updater goes on.
#[test]
fn a_refusal_with_the_chain_unreadable_after_it_is_held_briefly() {
    let attempts = Policy::default().max_attempts as usize;
    let (races, u, lines) = races_with_failing_reads(attempts, (0x4c, 0x4d));
    let (status, why) = u.health();
    println!(
        "{}\n{races} races, each followed by {attempts} failed reads: refusals {:?}; judged once read again {:?}; held \
         {:?}; breaker {:?}; failing sends {}; health {status} {why:?}",
        lines.join("\n"),
        u.metrics.refusals,
        u.metrics.unread_judged,
        u.held.values().map(|h| (h.step.name(), h.count, h.due, h.retry_at.map(|r| r - h.round))).collect::<Vec<_>>(),
        u.breaker,
        u.metrics.failing_sends
    );
    assert_eq!(races, 9, "{races}");
    assert_eq!(u.metrics.refusals.get(&("program", "unread")), Some(&(races as u64)));
    assert_eq!(u.metrics.unread_judged.get("race"), Some(&(races as u64)));
    assert_eq!(u.metrics.unread_judged.get("due"), None);
    assert_eq!((u.breaker.strikes, u.breaker.trips), (0, 0));
    assert_eq!(u.metrics.failing_sends, 0);
    assert!(u.held.is_empty(), "a race is not held once judged");
    assert!(!why.iter().any(|w| w.contains("did not land") || w.contains("sending paused")), "{why:?}");
}

/// 9c L-U3: a `Miller` refused for a cause the updater does not know, three times, the chain unreadable after each
/// refusal (every one of `max_attempts` reads fails), and no other party. Then it is not known whether a race was lost,
/// so the step is held; the next round reads the chain, finds the step still due, and counts the refusal as any of a
/// step that stayed due: a send that did not land (`/health` degraded from that round) and a strike, three of which
/// pause sending. (It used to be neither, whatever followed: no failing send, no strike, `/health` silent.) The fourth
/// attempt lands and the instance moves.
#[test]
fn a_refusal_with_the_chain_unreadable_after_it_is_judged_once_it_reads() {
    let r = mainnet();
    let clock = Clock::new(3);
    let payer = key(0x4b);
    let mut l = Flaky::new(HarnessLedger::new(std::slice::from_ref(&payer)), "Miller");
    l.unreadable = Policy::default().max_attempts as usize;
    let mut u = r.updater("unjudged", payer, vec![r.source("a", &clock)], None);
    settle(&mut u, &mut l, 60);
    let start = store(&u, &l.inner).finalized.slot;
    clock.set(4);
    l.refuse = 3;
    // (round, refusals so far, failing sends, strikes, paused) after each round; the rounds a Miller was sent.
    let (mut trace, mut millers, mut degraded_at, mut tripped) = (Vec::new(), Vec::new(), Vec::new(), 0);
    for round in 1..=120u64 {
        let p = u.tick(&mut l).unwrap();
        if matches!(&p, Plan::Send(a) if a.step == Step::Miller) {
            millers.push(round);
        }
        let paused = u.metrics.rounds < u.breaker.paused_until;
        tripped = tripped.max(u.breaker.trips);
        trace.push((round, l.refused, u.metrics.failing_sends, u.breaker.strikes, paused));
        if u.health().1.iter().any(|w| w.contains("did not land")) {
            degraded_at.push(round);
        }
        if store(&u, &l.inner).finalized.slot != start && matches!(p, Plan::UpToDate) {
            break;
        }
    }
    let end = store(&u, &l.inner).finalized.slot;
    println!(
        "a Miller refused {} times, the chain unreadable after each: Millers sent in rounds {millers:?}; (round, \
         refusals, failing sends, strikes, paused) {:?}; `did not land` in rounds {degraded_at:?}; breaker trips {tripped}; \
         refusals {:?}; judged {:?}; finalized slot {start} -> {end}; health {:?}",
        l.refused,
        trace.iter().filter(|t| t.2 > 0 || t.4).collect::<Vec<_>>(),
        u.metrics.refusals,
        u.metrics.unread_judged,
        u.health()
    );
    assert_eq!(l.refused, 3);
    assert_eq!(millers.len(), 4, "three refused, the fourth applied: {millers:?}");
    assert_eq!(u.metrics.refusals.get(&("other", "unread")), Some(&3));
    assert_eq!(u.metrics.unread_judged.get("due"), Some(&3));
    // Each refusal is judged in the round after it: a failing send from then on, until the fourth lands.
    for (k, &sent) in millers[..3].iter().enumerate() {
        let after = trace.iter().find(|t| t.0 == sent + 1).expect("a round after the refusal");
        assert_eq!((after.2, after.3), (k as u32 + 1, k as u32 + 1), "round {}: {after:?}", sent + 1);
    }
    assert_eq!(degraded_at.first(), Some(&(millers[0] + 1)), "degraded from the round that judged the first");
    assert_eq!(tripped, 1, "the third strike paused sending");
    assert!(end > start, "the instance moved");
    assert_eq!(u.metrics.failing_sends, 0, "the fourth landed");
    assert!(u.held.is_empty());
    assert_eq!(u.health().0, "ok", "{:?}", u.health());
}

/// `bytes`, a finality update, with its `signature_slot` set to `to`, a later slot of the same sync-committee period.
/// The signature does not cover that field (only its fork version), so the copy verifies; its `Open` window opens at
/// the later slot.
fn redate(mut b: Vec<u8>, config: &Config, to: u64) -> anyhow::Result<Vec<u8>> {
    let (u, _) = decode_finality_update(&b, config)?;
    anyhow::ensure!(to > u.signature_slot && to / 8192 == u.signature_slot / 8192, "{to}: not later in the period");
    let from = u.signature_slot.to_le_bytes();
    let at: Vec<usize> = b.windows(8).enumerate().filter(|(_, w)| *w == from).map(|(i, _)| i).collect();
    anyhow::ensure!(at.len() == 1, "signature_slot found {} times in the served bytes", at.len());
    b[at[0]..at[0] + 8].copy_from_slice(&to.to_le_bytes());
    let (re, _) = decode_finality_update(&b, config)?;
    anyhow::ensure!(re.signature_slot == to && re.finalized_header == u.finalized_header, "re-dating failed");
    Ok(b)
}

/// A source that serves every finality update re-dated `bump` slots later (at most to the period's last slot)
/// (review-integ9c M4).
struct Redated {
    inner: Box<dyn Source>,
    config: Config,
    bump: u64,
}

impl Source for Redated {
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
        let b = self.inner.finality_update()?;
        let (u, _) = decode_finality_update(&b, &self.config)?;
        let period_end = (u.signature_slot / 8192 + 1) * 8192 - 1;
        redate(b, &self.config, (u.signature_slot + self.bump).min(period_end))
    }
}

/// Signers of an update's sync aggregate.
fn signers(u: &LightClientUpdate) -> usize {
    u.sync_aggregate.bits.iter().filter(|b| **b).count()
}

/// Rounds until the instance is up to date (at most `max`): the plans, and how many waited.
fn rounds(u: &mut Updater, l: &mut dyn Ledger, max: usize) -> (Vec<Plan>, usize) {
    let mut plans = Vec::new();
    for _ in 0..max {
        let p = u.tick(l).unwrap();
        let done = matches!(p, Plan::UpToDate);
        plans.push(p);
        if done {
            break;
        }
    }
    let waits = plans.iter().filter(|p| matches!(p, Plan::Waiting(_))).count();
    (plans, waits)
}

/// review-integ9c M4: one source re-dates every finality update it serves (600 slots later, two hours, in the same
/// period); another serves them as they are. Listed first, the liar's copy used to win the tie (a stable sort over a
/// key without `signature_slot`), and its window not being open ended every round in `Waiting`, for up to a
/// sync-committee period. Now a copy signed in the future by the chain's clock is refused and its source demoted
/// (`FUTURE_MARGIN_MS`): the update is sent and applied in the first rounds, no round waits, nothing is refused.
#[test]
fn a_re_dated_copy_of_an_update_does_not_hold_it_back() {
    let r = mainnet();
    let clock = Clock::new(3);
    let payer = key(0x4a);
    let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
    let liar = Redated { inner: r.source("liar", &clock), config: r.config.clone(), bump: 600 };
    let mut u = r.updater("redated", payer.clone(), vec![Box::new(liar), r.source("honest", &clock)], None);
    settle(&mut u, &mut l, 60);
    let fin = store(&u, &l).finalized.slot;
    clock.set(4);
    let (real, _) = decode_finality_update(&r.entries[3].bytes, &r.config).unwrap();
    // The chain's clock one slot after the real signature slot: the honest copy's window is open, the re-dated
    // copy's (600 slots later) is not.
    l.now = Some(r.config.slot_start_ms(real.signature_slot) + 12_000);
    let (plans, waits) = rounds(&mut u, &mut l, 30);
    let moved = store(&u, &l).finalized.slot;
    let (status, why) = u.health();
    println!(
        "a re-dated copy listed first: {} rounds, {waits} waiting, finalized slot {fin} -> {moved} (the update's {}); \
         the liar refused {} time(s); health {status} {why:?}",
        plans.len(),
        real.finalized_header.beacon.slot,
        u.metrics.sources["liar"].refused
    );
    assert_eq!(waits, 0, "{plans:?}");
    assert_eq!(moved, real.finalized_header.beacon.slot, "the honest copy was applied");
    assert_eq!(refused(&l, 0), 0);
    assert_eq!(u.metrics.sources["liar"].refused, 1, "demoted once (not asked again while demoted)");
    assert_eq!(u.metrics.sources["honest"].refused, 0);
    assert!(why.iter().any(|w| w.contains("liar demoted") && w.contains("signed in the future")), "{why:?}");
}

/// review-integ9c M4, the copy only the liar serves: the other source is one update behind, so the re-dated update
/// (the furthest finalized header) has no earlier copy to take instead. It used to be chosen and to wait for its
/// window, up to a sync-committee period, while the other source's update was never sent and the liar never demoted.
/// Now it is refused as signed in the future, the liar demoted, and the other source's update applied.
#[test]
fn a_re_dated_update_only_one_source_serves_is_refused() {
    let r = mainnet();
    let clock = Clock::new(3);
    let payer = key(0x4e);
    let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
    let liar = Redated { inner: r.source("liar", &clock), config: r.config.clone(), bump: 600 };
    let mut behind = Recorded::open("behind", &r.dir, &r.config, clock.clone()).unwrap();
    behind.lag = 1;
    let mut u = r.updater("only-the-liar", payer.clone(), vec![Box::new(liar), Box::new(behind)], None);
    settle(&mut u, &mut l, 60);
    let fin = store(&u, &l).finalized.slot;
    clock.set(5);
    let (newest, _) = decode_finality_update(&r.entries[4].bytes, &r.config).unwrap();
    let (older, _) = decode_finality_update(&r.entries[3].bytes, &r.config).unwrap();
    l.now = Some(r.config.slot_start_ms(newest.signature_slot) + 12_000);
    let (plans, waits) = rounds(&mut u, &mut l, 30);
    let moved = store(&u, &l).finalized.slot;
    let (status, why) = u.health();
    println!(
        "a re-dated update only the liar serves (finalizing {}), the other source one behind (finalizing {}): {} rounds, \
         {waits} waiting, finalized slot {fin} -> {moved}; the liar refused {} time(s); health {status} {why:?}",
        newest.finalized_header.beacon.slot,
        older.finalized_header.beacon.slot,
        plans.len(),
        u.metrics.sources["liar"].refused
    );
    assert_eq!(waits, 0, "{plans:?}");
    assert_eq!(moved, older.finalized_header.beacon.slot, "the other source's update was applied");
    assert_eq!(refused(&l, 0), 0);
    assert_eq!(u.metrics.sources["liar"].refused, 1);
    assert!(why.iter().any(|w| w.contains("liar demoted") && w.contains("signed in the future")), "{why:?}");
}

/// The client-diversity probe's mainnet answers of 18 November 2026 (`lez/updater/results/client-diversity-2026-11-18/`,
/// read from public beacon APIs): two real finality updates of one finalized header (slot 15,682,752), Lodestar's
/// (signed at 15,682,818, 511 signers) and Nimbus's (signed at 15,682,828, 510 signers). The lab's record reaches that
/// period: its entry 105 finalizes 15,682,208.
fn probe(node: &str) -> Vec<u8> {
    let dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("results/client-diversity-2026-11-18/probe/mainnet/answers");
    fs::read(dir.join(format!("mainnet-{node}-finality-round0.ssz"))).unwrap()
}

/// A recorded source that, once `on` is set, serves `finality` as its finality update, re-dated to slot `to` if given.
struct Serving {
    inner: Box<dyn Source>,
    config: Config,
    finality: Vec<u8>,
    to: Option<u64>,
    on: Arc<AtomicBool>,
}

impl Source for Serving {
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
        if !self.on.load(Ordering::SeqCst) {
            return self.inner.finality_update();
        }
        match self.to {
            Some(to) => redate(self.finality.clone(), &self.config, to),
            None => Ok(self.finality.clone()),
        }
    }
}

/// review-integ9d M1, the bypass of the tie-break: a source serves another real update of the same finalized header
/// with more signers (Lodestar's, 511, of the probe), re-dated to a later slot, while the other serves Nimbus's (510).
/// The liar's used to sort first (the most signers before the earliest signature slot), and its window not being open
/// ended every round in `Waiting`, up to a sync-committee period. Re-dated 600 slots (two hours): refused as signed in
/// the future, the liar demoted, Nimbus's applied. Re-dated three slots (24 s past the chain's clock, within
/// `FUTURE_MARGIN_MS`, as a clock skew could make an honest update look): not refused, the liar not demoted, and
/// Nimbus's, signed earlier, applied first. Neither waits a round.
#[test]
fn a_re_dated_update_with_more_signers_does_not_hold_back_another() {
    let r = mainnet();
    let (lodestar, nimbus) = (probe("lodestar-mainnet.chainsafe.io"), probe("testing.mainnet.beacon-api.nimbus.team"));
    let (more, _) = decode_finality_update(&lodestar, &r.config).unwrap();
    let (fewer, _) = decode_finality_update(&nimbus, &r.config).unwrap();
    assert_eq!(more.finalized_header, fewer.finalized_header, "one finalized header");
    assert_eq!((signers(&more), signers(&fewer)), (511, 510));
    assert!(more.signature_slot < fewer.signature_slot);
    // The chain's clock one slot after Nimbus's signature slot.
    let now = r.config.slot_start_ms(fewer.signature_slot) + 12_000;
    let near = fewer.signature_slot + 3;
    assert!(r.config.slot_start_ms(near) > now && r.config.slot_start_ms(near) <= now + FUTURE_MARGIN_MS);
    for (case, to, payer) in [("600 slots", more.signature_slot + 600, key(0x50)), ("three slots", near, key(0x51))] {
        let clock = Clock::new(105);
        let on = Arc::new(AtomicBool::new(false));
        let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
        let serving = |name: &str, finality: &[u8], to: Option<u64>| -> Box<dyn Source> {
            let inner = r.source(name, &clock);
            Box::new(Serving { inner, config: r.config.clone(), finality: finality.to_vec(), to, on: on.clone() })
        };
        let sources = vec![serving("liar", &lodestar, Some(to)), serving("honest", &nimbus, None)];
        let mut u = r.updater("more-signers", payer.clone(), sources, None);
        settle(&mut u, &mut l, 80);
        let fin = store(&u, &l).finalized.slot;
        assert_eq!(fin, 15_682_208, "the record's entry 105");
        on.store(true, Ordering::SeqCst);
        l.now = Some(now);
        let n0 = l.log.len();
        let (plans, waits) = rounds(&mut u, &mut l, 30);
        let moved = store(&u, &l).finalized.slot;
        let opens: Vec<&String> = l.log[n0..]
            .iter()
            .filter(|(_, label, o)| label.ends_with("Open") && o.applied)
            .map(|(_, label, _)| label)
            .collect();
        let liar = u.metrics.sources["liar"].clone();
        let (status, why) = u.health();
        println!(
            "Lodestar's update (511 signers) re-dated {case} (to slot {to}) by the source listed first, Nimbus's (510) \
             by the other: {} rounds, {waits} waiting, finalized slot {fin} -> {moved}, Opens applied {opens:?}; the \
             liar refused {} time(s); health {status} {why:?}",
            plans.len(),
            liar.refused
        );
        assert_eq!(waits, 0, "{case}: {plans:?}");
        assert_eq!(moved, fewer.finalized_header.beacon.slot, "{case}");
        assert_eq!(opens.len(), 1, "{case}: one job");
        assert_eq!(refused(&l, 0), 0, "{case}");
        if to == near {
            assert_eq!(liar.refused, 0, "within the margin: not demoted");
        } else {
            assert_eq!(liar.refused, 1);
            assert!(why.iter().any(|w| w.contains("liar demoted") && w.contains("signed in the future")), "{why:?}");
        }
    }
}

/// 9c L-U5: a hold for good (the initialization's `Adopt` refused out of gas) outlives `HOLD_ROUNDS` rounds and a restart
/// (`state_dir/<chain>/held.tsv`): one refusal in `HOLD_ROUNDS` + 10 rounds and ten more after a restart (it used to be
/// forgotten after 4,096 rounds, about 13.7 h at 12 s rounds, and at every restart, and the step refused and charged
/// again). Raising the gas limit releases it; once the step lands the file is empty.
#[test]
fn a_hold_for_good_outlives_its_rounds_and_a_restart() {
    let r = mainnet();
    let clock = Clock::new(3);
    let payer = key(0x52);
    let dir = scratch("held");
    let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
    let mut u = r.updater("held", payer.clone(), vec![r.source("a", &clock)], Some(dir.clone()));
    u.fees.gas_limit = 1_000_000;
    let total = HOLD_ROUNDS + 10;
    for _ in 0..total {
        u.tick(&mut l).unwrap();
    }
    let file = dir.join("mainnet/held.tsv");
    let kept = fs::read_to_string(&file).unwrap();
    println!("{total} rounds: {} refused; held.tsv: {kept:?}", refused(&l, 0));
    assert_eq!(refused(&l, 0), 1, "one refusal in {total} rounds");
    assert_eq!(kept.lines().count(), 1);
    assert!(kept.contains("\tgas\t"), "{kept}");
    drop(u);

    let mut again = r.updater("held", payer.clone(), vec![r.source("a", &clock)], Some(dir.clone()));
    again.fees.gas_limit = 1_000_000;
    for _ in 0..10 {
        again.tick(&mut l).unwrap();
    }
    let (status, why) = again.health();
    println!("restarted, ten rounds: {} refused; held {}; health {status} {why:?}", refused(&l, 0), again.held.len());
    assert_eq!(refused(&l, 0), 1, "not sent again after the restart");
    assert_eq!(again.held.len(), 1);
    assert!(why.iter().any(|w| w.contains("out of gas") && w.contains("raise it")), "{why:?}");

    again.fees.gas_limit = Fees::default().gas_limit;
    settle(&mut again, &mut l, 60);
    assert!(again.held.is_empty());
    assert_eq!(fs::read_to_string(&file).unwrap(), "", "released and applied: nothing kept");
    assert_eq!(refused(&l, 0), 1);
}
