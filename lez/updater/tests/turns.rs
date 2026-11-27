//! The clock the turns count (RFP-021 R.10: "Submission timing must not systematically concentrate cost on whichever
//! party reacts fastest"): each updater waits `H(key, job, step) mod defer_window` turns of a clock before it sends a
//! step, and join or wait bounds its waits on the same clock. Counted in the updater's own polling rounds (before), an
//! updater polling more often waited fewer seconds and won more turns; counted on the chain's height (`Ledger::height`,
//! the blocks every party reading the chain reads alike), it reaches its turn at the same height however often it
//! reads. The in-process chain (LEZ v0.4.0-rc1's state machine, dev mode) with its height set by the test
//! (`HarnessLedger::height`), on the lab's recorded mainnet data.
#![cfg(feature = "harness")]

use std::fs;

use ferrata_core::Root;
use ferrata_lez_wire::Config;
use ferrata_updater::{
    chain::{decode_finality_update, preset, root},
    config::{Fees, Policy},
    ledger::{HarnessLedger, Ledger},
    source::{lab_record, load_entries, Clock, Entry, Recorded, Source},
    updater::{Instance, Plan, Step, Updater},
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

    fn updater(&self, label: &str, payer: PrivateKey, policy: Policy) -> Updater {
        let source = Recorded::open("a", &self.dir, &self.config, self.clock.clone()).unwrap();
        let inst = Instance::new("mainnet", HarnessLedger::program(), self.config.clone(), self.checkpoint);
        let sources: Vec<Box<dyn Source>> = vec![Box::new(source)];
        let mut u = Updater::new(label, inst, payer, Fees::default(), policy, sources, None).unwrap();
        u.sleep = no_sleep;
        u
    }

    fn new_finality_updates(&self) -> usize {
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

fn window(w: u64) -> Policy {
    Policy { defer_window: w, retry_base_ms: 0, ..Policy::default() }
}

/// The first round and the height at which an updater named `label` (window 8) sends the instance's `Init`, reading the
/// chain once a round, a block every `rounds_per_block` rounds; `heights`: the ledger gives its height.
fn first_send(fx: &Fx, label: &str, rounds_per_block: u64, heights: bool) -> (u64, u64) {
    let payer = key(0x91);
    let l = HarnessLedger::new(std::slice::from_ref(&payer));
    let mut l = l;
    let mut u = fx.updater(label, payer, window(8));
    for round in 1..=200u64 {
        let height = (round - 1) / rounds_per_block;
        l.height = heights.then_some(height);
        match u.plan(&l).unwrap() {
            Plan::Send(a) => {
                assert_eq!(a.step, Step::Init);
                assert_eq!(u.metrics.turn_clock_height, heights);
                return (round, height);
            }
            Plan::Deferred(Step::Init) => {}
            p => panic!("{p:?}"),
        }
    }
    panic!("no Init sent");
}

/// An updater's turn comes at a height, not after a count of its reads: read once a block it sends its `Init` at height
/// `w` (its draw, at least 2 here), read five times a block it sends at the same height after five times the reads.
/// Counting its own rounds instead (a ledger with no height: the updater before), it sends after the same number of
/// reads whatever the block rate, so reading five times a block it sends at a fifth of the height: a party reading more
/// often reacts sooner, by as much as it reads more often.
#[test]
fn a_turn_comes_at_a_height_not_after_a_count_of_reads() {
    let fx = Fx::load();
    let label = (0..100)
        .map(|i| format!("t{i}"))
        .find(|l| first_send(&fx, l, 1, true).1 >= 2)
        .expect("a label whose Init waits two blocks or more");
    let (r1, h1) = first_send(&fx, &label, 1, true);
    let (r5, h5) = first_send(&fx, &label, 5, true);
    let (q1, _) = first_send(&fx, &label, 1, false);
    let (q5, g5) = first_send(&fx, &label, 5, false);
    println!("{label}: heights: once a block round {r1} height {h1}, five a block round {r5} height {h5}; rounds: {q1}, {q5} (height {g5})");
    assert_eq!(r1, h1 + 1);
    assert_eq!(h5, h1, "the same height");
    assert_eq!(r5, 5 * h1 + 1, "five times the reads");
    assert_eq!((q1, q5), (h1 + 1, h1 + 1), "counted in rounds: the same number of reads");
    assert!(g5 < h1, "counted in rounds, reading five times a block sends at a lower height: {g5} < {h1}");
}

/// Join or wait's bound counts blocks too: a party opens U1's job and never sends another step. While the chain's height
/// does not move, the updater (which saw U1 served) waits however many times it reads (no block has passed: the job has
/// not been shown to stall); once blocks come, it waits exactly `join_wait_rounds` of them after it first read the job,
/// then opens U2's job and finishes it.
#[test]
fn a_stalled_job_is_passed_after_join_wait_blocks_however_often_the_chain_is_read() {
    let fx = Fx::load();
    let c = fx.new_finality_updates();
    let (kb, opener) = (key(0x92), key(0x93));
    let mut l = HarnessLedger::new(&[kb.clone(), opener.clone()]);
    let mut b = fx.updater("b", kb, window(1));
    let mut height = 0u64;
    fx.clock.set(c);
    for _ in 0..400 {
        height += 1;
        l.height = Some(height);
        if matches!(b.tick(&mut l).unwrap(), Plan::UpToDate) {
            break;
        }
    }
    fx.clock.set(c + 1);
    assert!(matches!(b.plan(&l).unwrap(), Plan::Send(ref x) if x.step == Step::Open));
    let (u1, _) = decode_finality_update(&fx.entries[c].bytes, &fx.config).unwrap();
    let tx = b.open_unchecked(&l, &u1).unwrap();
    assert!(l.submit(&opener, &tx, 4_800_000, 1_000_000_000).unwrap().applied, "the other party's Open of U1");
    fx.clock.set(c + 2);
    let wait = b.policy.join_wait();
    for _ in 0..100 {
        let p = b.tick(&mut l).unwrap();
        assert!(matches!(p, Plan::Deferred(Step::Open)), "the height not moving: still waiting ({p:?})");
    }
    assert_eq!(b.metrics.stalled_jobs_passed, 0);
    let mut blocks = 0;
    loop {
        height += 1;
        blocks += 1;
        l.height = Some(height);
        let p = b.tick(&mut l).unwrap();
        if !matches!(p, Plan::Deferred(Step::Open)) {
            assert!(matches!(p, Plan::Send(ref x) if x.step == Step::Open), "{p:?}");
            break;
        }
        assert!(blocks < 200, "never passed");
    }
    println!("passed after {blocks} blocks (join_wait_rounds {wait}), 100 reads at one height before");
    assert_eq!(blocks, wait, "held back exactly join_wait_rounds blocks");
    assert_eq!(b.metrics.stalled_jobs_passed, 1);
    for _ in 0..40 {
        height += 1;
        l.height = Some(height);
        if matches!(b.tick(&mut l).unwrap(), Plan::UpToDate) {
            break;
        }
    }
    assert_eq!(b.inst.store(&l).unwrap().unwrap().finalized.slot, fx.entries[c + 1].finalized_slot, "U2 applied");
}

/// The stall alarm counts blocks without progress (room for a job's steps each waiting a full deferral of blocks), and,
/// when the chain's height stops, the rounds it has not moved for: degraded after `stall_rounds`, down after twice as
/// many, whichever comes first. An updater that can send nothing (`min_agreeing = 2`, one source) on a chain making a
/// block every two rounds is degraded only after `stall_rounds` blocks, not rounds; on a chain whose height stops, after
/// `stall_rounds` rounds.
#[test]
fn the_stall_alarm_counts_blocks_and_a_height_that_stops() {
    for frozen in [false, true] {
        let fx = Fx::load();
        let all = Clock::new(fx.entries.len());
        let payer = key(0x94);
        let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
        let source = Recorded::open("a", &fx.dir, &fx.config, all.clone()).unwrap();
        let inst = Instance::new("mainnet", HarnessLedger::program(), fx.config.clone(), fx.checkpoint);
        let policy = Policy { min_agreeing: 2, ..window(1) };
        let mut u = Updater::new("stall", inst, payer, Fees::default(), policy, vec![Box::new(source)], None).unwrap();
        u.sleep = no_sleep;
        l.height = Some(0);
        u.tick(&mut l).unwrap();
        let (stall, down) = (u.stall_rounds(), u.stall_down_rounds());
        let (mut first_degraded, mut first_down) = (None, None);
        for round in 1..=2 * down + 10 {
            if !frozen && round % 2 == 0 {
                l.height = Some(round / 2);
            }
            u.tick(&mut l).unwrap();
            let (status, why) = u.health();
            if first_degraded.is_none() && status != "ok" {
                first_degraded = Some((round, why.clone()));
            }
            if first_down.is_none() && status == "down" {
                first_down = Some((round, why));
            }
        }
        println!("frozen {frozen}: first not ok {first_degraded:?}; first down {first_down:?}");
        let (r, why) = first_degraded.expect("degraded");
        let (rd, why_down) = first_down.expect("down");
        assert!(why.iter().any(|w| w.contains("no progress")), "{why:?}");
        assert!(why_down.iter().any(|w| w.contains("no progress")), "{why_down:?}");
        if frozen {
            assert!(why.iter().any(|w| w.contains("height") && w.contains("not moving")), "{why:?}");
            assert!((stall..stall + 3).contains(&r), "degraded after {stall} rounds of a height that stopped: {r}");
            assert!((down..down + 3).contains(&rd), "down after {down}: {rd}");
        } else {
            assert!(r >= 2 * stall, "not before {stall} blocks (two rounds each): {r}");
            assert!(rd >= 2 * down, "not before {down} blocks: {rd}");
        }
    }
}
