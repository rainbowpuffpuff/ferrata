//! The updater against LEZ v0.4.0-rc1's state machine (the harness ledger, RISC0_DEV_MODE), fed
//! the lab's recorded mainnet and Sepolia light-client data (`lez/lab/results/record/net/`):
//! catch-up, honest and faulty sources, a restart in the middle of a job and of a period, two
//! updaters sharing the work, and the A3 done gate with an adversarial updater.
#![cfg(feature = "harness")]

use std::{fs, path::PathBuf};

use ferrata_core::{witness::CompactStore, Root};
use ferrata_lez_wire::Config;
use ferrata_updater::{
    chain::{decode_finality_update, decode_update, preset, root},
    config::{Fees, Policy},
    ledger::{HarnessLedger, Ledger},
    source::{lab_record, load_entries, Clock, Entry, Fault, Faulty, Recorded, Source},
    updater::{Instance, Plan, Step, Updater},
};
use lee::PrivateKey;

fn key(b: u8) -> PrivateKey {
    PrivateKey::try_new([b; 32]).unwrap()
}

fn no_sleep(_: u64) {}

struct Rec {
    name: &'static str,
    config: Config,
    dir: PathBuf,
    checkpoint: Root,
    entries: Vec<Entry>,
}

fn rec(name: &'static str) -> Rec {
    let config = preset(name, 2).unwrap();
    let dir = lab_record(name);
    let checkpoint = root(&fs::read_to_string(dir.join("checkpoint")).unwrap()).unwrap();
    let entries = load_entries(&dir.join("updates"), &config).unwrap();
    Rec { name, config, dir, checkpoint, entries }
}

impl Rec {
    fn source(&self, label: &str, clock: &Clock, lag: usize) -> Recorded {
        let mut s = Recorded::open(label, &self.dir, &self.config, clock.clone()).unwrap();
        s.lag = lag;
        s
    }
    fn honest(&self, label: &str, clock: &Clock) -> Box<dyn Source> {
        Box::new(self.source(label, clock, 0))
    }
    fn faulty(&self, label: &str, clock: &Clock, fault: Fault) -> Box<dyn Source> {
        Box::new(Faulty::new(self.source(label, clock, 0), fault, self.config.clone()))
    }
    fn updater(
        &self,
        label: &str,
        payer: PrivateKey,
        sources: Vec<Box<dyn Source>>,
        state: Option<PathBuf>,
        policy: Policy,
    ) -> Updater {
        let inst = Instance::new(self.name, HarnessLedger::program(), self.config.clone(), self.checkpoint);
        let mut u = Updater::new(label, inst, payer, Fees::default(), policy, sources, state).unwrap();
        u.sleep = no_sleep;
        u
    }
    /// The finalized slot and header root the last recorded entry finalizes.
    fn last_finalized(&self) -> (u64, Root) {
        let e = self.entries.last().unwrap();
        let (u, _) = if e.period_update {
            decode_update(&e.bytes, &self.config)
        } else {
            decode_finality_update(&e.bytes, &self.config)
        }
        .unwrap();
        (u.finalized_header.beacon.slot, u.finalized_header.beacon.hash_tree_root())
    }
}

fn policy(defer: u64) -> Policy {
    Policy { defer_window: defer, retry_base_ms: 0, ..Policy::default() }
}

fn store(u: &Updater, l: &HarnessLedger) -> CompactStore {
    u.inst.store(l).unwrap().expect("active instance")
}

fn applied(u: &Updater, l: &HarnessLedger) -> u64 {
    u.inst.state(l).unwrap().map_or(0, |s| s.applied)
}

/// Rounds until the updater has nothing to send; panics if it gets stuck.
fn settle(u: &mut Updater, l: &mut HarnessLedger, max: usize) -> usize {
    for round in 0..max {
        match u.tick(l).unwrap() {
            Plan::UpToDate => return round,
            Plan::Stuck(w) | Plan::Waiting(w) => panic!("{}: {w}", u.inst.name),
            _ => {}
        }
    }
    panic!("{}: not up to date after {max} rounds", u.inst.name)
}

fn scratch(name: &str) -> PathBuf {
    let base = std::env::var_os("CARGO_TARGET_TMPDIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let d = base.join(format!("updater-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn refused_total(l: &HarnessLedger) -> usize {
    l.log.iter().filter(|(_, _, o)| !o.applied).count()
}

/// Catch-up from the checkpoint to the newest recorded update, with two honest sources: the
/// instance ends where the lab's ended (the same finalized header).
#[test]
fn catches_up_mainnet_and_sepolia_from_recorded_data() {
    for r in [rec("mainnet"), rec("sepolia")] {
        let clock = Clock::new(r.entries.len());
        let payer = key(0x21);
        let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
        let mut u = r.updater("u1", payer, vec![r.honest("a", &clock), r.honest("b", &clock)], None, policy(1));
        let rounds = settle(&mut u, &mut l, 400);
        let s = store(&u, &l);
        let (slot, root) = r.last_finalized();
        assert_eq!((s.finalized.slot, s.finalized.hash_tree_root()), (slot, root), "{}", r.name);
        assert_eq!(refused_total(&l), 0, "{}: no transaction refused", r.name);
        println!(
            "{}: {rounds} rounds, {} jobs applied, {} transactions, finalized slot {slot} root {}, health {}",
            r.name,
            applied(&u, &l),
            l.log.len(),
            hex::encode(root),
            u.health().0
        );
        assert_eq!(u.health().0, "ok");
    }
}

/// A source that serves forged signatures (and a wrong bootstrap) is refused before any
/// transaction: no gas is spent on what it serves, it is demoted, health says so, and the
/// honest source carries the instance to the end.
#[test]
fn lying_source_is_refused_without_gas() {
    let r = rec("mainnet");
    let clock = Clock::new(r.entries.len());
    let payer = key(0x22);
    let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
    let sources = vec![
        r.faulty("liar-bootstrap", &clock, Fault::WrongBootstrap),
        r.faulty("liar-signature", &clock, Fault::BadSignature),
        r.honest("honest", &clock),
    ];
    let mut u = r.updater("u1", payer, sources, None, policy(1));
    // The instance's Init comes from the honest source; the first liar is demoted for it.
    settle(&mut u, &mut l, 400);
    assert_eq!(store(&u, &l).finalized.slot, r.last_finalized().0);
    assert_eq!(refused_total(&l), 0, "nothing the liars served reached the chain");
    let m = &u.metrics.sources;
    assert!(m["liar-bootstrap(WrongBootstrap)"].refused >= 1);
    assert!(m["liar-signature(BadSignature)"].refused >= 1);
    assert_eq!(m["honest"].refused, 0);
    let (status, why) = u.health();
    assert_eq!(status, "degraded");
    assert!(why.iter().any(|w| w.contains("liar-signature")), "{why:?}");
    let text = u.render_metrics();
    assert!(text.contains(
        "ferrata_updater_source_refused_total{daemon=\"u1\",chain=\"mainnet\",source=\"liar-signature(BadSignature)\"}"
    ));
    println!("{text}");
}

/// A source that lags is found lagging and the up-to-date one is used; one that is down is
/// counted and skipped; with `min_agreeing = 2` the updater sends only what two sources serve.
#[test]
fn stale_and_down_sources_fall_back() {
    let r = rec("sepolia");
    let n = r.entries.len();
    let clock = Clock::new(n);
    let payer = key(0x23);
    let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
    let stale: Box<dyn Source> = Box::new(r.source("stale", &clock, 6));
    let down = r.faulty("down", &clock, Fault::Down);
    let mut u = r.updater("u1", payer.clone(), vec![stale, down, r.honest("fresh", &clock)], None, policy(1));
    settle(&mut u, &mut l, 400);
    assert_eq!(store(&u, &l).finalized.slot, r.last_finalized().0, "the fresh source's newest update is applied");
    assert!(u.metrics.sources["stale"].lagging, "the stale source is flagged");
    assert!(u.metrics.sources["down(Down)"].errors > 0);
    assert_eq!(u.health().0, "degraded");

    // Two sources must agree: with one fresh and one stale, the updater stops at what both serve.
    let clock2 = Clock::new(n - 4);
    let payer2 = key(0x24);
    let mut l2 = HarnessLedger::new(std::slice::from_ref(&payer2));
    let both = Policy { min_agreeing: 2, ..policy(1) };
    let stale2: Box<dyn Source> = Box::new(r.source("stale", &clock2, 2));
    let mut u2 = r.updater("u2", payer2, vec![stale2, r.honest("fresh", &clock2)], None, both);
    let mut last = Plan::UpToDate;
    for _ in 0..400 {
        last = u2.tick(&mut l2).unwrap();
        if matches!(last, Plan::UpToDate | Plan::Waiting(_)) {
            break;
        }
    }
    let held = store(&u2, &l2).finalized.slot;
    let stale_head = r.entries[..n - 4 - 2].iter().map(|e| e.finalized_slot).max().unwrap();
    let fresh_head = r.entries[..n - 4].iter().map(|e| e.finalized_slot).max().unwrap();
    println!(
        "min_agreeing 2: held {held}, stale serves up to {stale_head}, fresh up to {fresh_head}; last plan {last:?}"
    );
    assert!(held <= stale_head, "never beyond what the stale source also serves");
    assert!(held < fresh_head);
}

/// A restart in the middle of a job and of a sync-committee period: the first updater sends
/// `Open` and `Miller` of a finality update inside period 1410 and is killed; a second one, on
/// the same state directory, continues the same job (no second `Open`); a third, with no state
/// directory at all, refills its committees from the sources and continues. A dropped
/// connection is retried with backoff.
#[test]
fn restart_mid_job_and_mid_period() {
    let r = rec("sepolia");
    let n = r.entries.len();
    // Stop the clock three finality updates before the end: the store is inside period 1410.
    let clock = Clock::new(n - 3);
    let payer = key(0x25);
    let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
    let dir = scratch("restart");
    let mut u1 = r.updater(
        "u1",
        payer.clone(),
        vec![r.honest("a", &clock), r.honest("b", &clock)],
        Some(dir.clone()),
        policy(1),
    );
    settle(&mut u1, &mut l, 400);
    let spec = r.config.spec();
    let before = store(&u1, &l);
    let period = spec.period_at_slot(before.finalized.slot);
    let next = &r.entries[n - 3];
    assert!(!next.period_update);
    assert_eq!(spec.period_at_slot(next.finalized_slot), period, "the next update is in the same period");
    clock.set(n - 2);
    // Open and Miller, then the process dies.
    let mut sent = Vec::new();
    while sent.len() < 2 {
        if let Plan::Send(a) = u1.tick(&mut l).unwrap() {
            sent.push(a.step);
        }
    }
    assert_eq!(sent, vec![Step::Open, Step::Miller]);
    drop(u1);
    // Restart on the same state directory.
    let mut u2 = r.updater(
        "u1",
        payer.clone(),
        vec![r.honest("a", &clock), r.honest("b", &clock)],
        Some(dir.clone()),
        policy(1),
    );
    let first = (0..20)
        .find_map(|_| match u2.plan(&l).unwrap() {
            Plan::Send(a) => Some(a),
            _ => None,
        })
        .expect("a step is due");
    assert_eq!(first.step, Step::Finish, "the restarted updater finishes the open job");
    l.fail_next = 2;
    let out = u2.submit(&mut l, &first, true).unwrap().expect("sent");
    assert!(out.applied);
    assert_eq!(u2.metrics.retries, 2, "two dropped connections, retried");
    assert_eq!(store(&u2, &l).finalized.slot, next.finalized_slot);
    // The same daemon after losing its disk: committees come back from the sources.
    clock.set(n);
    let mut l3 = l;
    let mut u3 = r.updater("u1", payer, vec![r.honest("a", &clock)], None, policy(1));
    settle(&mut u3, &mut l3, 50);
    assert_eq!(store(&u3, &l3).finalized.slot, r.last_finalized().0);
    assert_eq!(refused_total(&l3), 0, "no step was sent twice");
    let journal = fs::read_to_string(dir.join("sepolia/journal.tsv")).unwrap();
    println!("journal of the first two lives: {} lines; ledger {} transactions", journal.lines().count(), l3.log.len());
}

/// Tiny deterministic generator for the order of a block's transactions.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = (self.next() % (i as u64 + 1)) as usize;
            v.swap(i, j);
        }
    }
}

/// The A3 done gate on recorded data: two honest updaters and one adversarial updater follow
/// mainnet and Sepolia on one LEZ chain while the record replays one update at a time (every
/// recorded update is the newest once). Each round is a block: every updater plans against the
/// state the previous block left, then the block's transactions settle in a shuffled order (so
/// two updaters that send the same step in one block collide, as on a sequencer). The adversary
/// races every due step a third of the time without waiting its turn, and opens a job for a
/// forged update and replays an applied one each time a new update appears.
#[test]
fn gate_two_honest_one_adversary() {
    let recs = [rec("mainnet"), rec("sepolia")];
    let (h1, h2, adv) = (key(0x31), key(0x32), key(0x33));
    let mut l = HarnessLedger::new(&[h1.clone(), h2.clone(), adv.clone()]);
    let clocks: Vec<Clock> = recs.iter().map(|_| Clock::new(1)).collect();
    // Per chain: [H1, H2, A].
    let mut ups: Vec<Vec<Updater>> = recs
        .iter()
        .zip(&clocks)
        .map(|(r, c)| {
            vec![
                r.updater(
                    "honest-1",
                    h1.clone(),
                    vec![r.honest("a", c), r.honest("b", c), r.faulty("liar", c, Fault::BadSignature)],
                    None,
                    policy(8),
                ),
                r.updater(
                    "honest-2",
                    h2.clone(),
                    vec![r.honest("b", c), r.honest("c", c), Box::new(r.source("stale", c, 3))],
                    None,
                    policy(8),
                ),
                r.updater("adversary", adv.clone(), vec![r.honest("a", c)], None, policy(1)),
            ]
        })
        .collect();
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut junk_due = vec![false; recs.len()];
    let mut rounds = 0;
    loop {
        rounds += 1;
        assert!(rounds < 20_000, "the gate run did not finish");
        // Plan against the state the previous block left.
        let mut block: Vec<(
            usize,
            usize,
            Option<ferrata_updater::updater::Action>,
            Option<ferrata_updater::ledger::Tx>,
        )> = Vec::new();
        let mut idle = vec![true; recs.len()];
        for (c, chain) in ups.iter_mut().enumerate() {
            for (d, u) in chain.iter_mut().enumerate() {
                let plan = u.plan(&l).unwrap();
                match plan {
                    Plan::Send(a) => {
                        if d < 2 {
                            idle[c] = false;
                        }
                        // The adversary races a third of the time.
                        if d < 2 || rng.next() % 3 == 0 {
                            block.push((c, d, Some(a), None));
                        }
                    }
                    Plan::Deferred(_) if d < 2 => idle[c] = false,
                    Plan::Stuck(w) if d < 2 => panic!("{} {}: {w}", u.label, u.inst.name),
                    _ => {}
                }
            }
            // The adversary's junk, once per new update: a forged update's Open, an applied
            // update's Open again.
            if junk_due[c] && chain[2].inst.store(&l).unwrap().is_some() {
                junk_due[c] = false;
                let r = &recs[c];
                let t = clocks[c].now();
                let liar = Faulty::new(r.source("x", &clocks[c], 0), Fault::BadSignature, r.config.clone());
                let e = &r.entries[t - 1];
                let bytes = liar.flip_signature(e.bytes.clone(), e.period_update);
                let forged = if e.period_update {
                    decode_update(&bytes, &r.config)
                } else {
                    decode_finality_update(&bytes, &r.config)
                };
                if let Ok((forged, _)) = forged {
                    block.push((c, 2, None, Some(chain[2].open_unchecked(&l, &forged).unwrap())));
                }
                if t >= 3 {
                    let e = &r.entries[t - 3];
                    let old = if e.period_update {
                        decode_update(&e.bytes, &r.config)
                    } else {
                        decode_finality_update(&e.bytes, &r.config)
                    };
                    if let Ok((old, _)) = old {
                        block.push((c, 2, None, Some(chain[2].open_unchecked(&l, &old).unwrap())));
                    }
                }
            }
        }
        rng.shuffle(&mut block);
        for (c, d, action, junk) in block {
            let u = &mut ups[c][d];
            match (action, junk) {
                (Some(a), _) => {
                    u.submit(&mut l, &a, false).unwrap();
                }
                (None, Some(tx)) => {
                    l.submit(&adv, &tx, 4_800_000, 1_000_000_000).unwrap();
                }
                _ => {}
            }
        }
        // Ethereum moves on once both honest updaters have nothing left to do on a chain.
        let mut done = true;
        for c in 0..recs.len() {
            if !(idle[c] && clocks[c].now() >= recs[c].entries.len()) {
                done = false;
            }
            if idle[c] && clocks[c].now() < recs[c].entries.len() {
                clocks[c].set(clocks[c].now() + 1);
                junk_due[c] = true;
            }
        }
        if done {
            break;
        }
    }
    // Every recorded update applied exactly once, and the instance where the lab's ended.
    let mut lines = vec![format!("gate\trounds\t{rounds}")];
    let mut total_updates = 0;
    for (c, r) in recs.iter().enumerate() {
        let s = store(&ups[c][0], &l);
        let jobs = applied(&ups[c][0], &l);
        assert_eq!(s.finalized.slot, r.last_finalized().0, "{}", r.name);
        assert_eq!(jobs, 1 + r.entries.len() as u64, "{}: the activation and every recorded update, each once", r.name);
        total_updates += r.entries.len();
        lines.push(format!(
            "{}\tupdates applied\t{}\tjobs (with Init)\t{jobs}\tfinalized slot\t{}",
            r.name,
            r.entries.len(),
            s.finalized.slot
        ));
    }
    // Per daemon, across both chains.
    let sum = |d: usize, f: &dyn Fn(&Updater) -> u128| ups.iter().map(|chain| f(&chain[d])).sum::<u128>();
    let gas_ok = |d: usize| sum(d, &|u| u.metrics.gas_applied as u128);
    let gas_bad = |d: usize| sum(d, &|u| u.metrics.gas_refused as u128);
    let fee = |d: usize| sum(d, &|u| u.metrics.spent());
    let honest_ok = gas_ok(0) + gas_ok(1);
    let honest_bad = gas_bad(0) + gas_bad(1);
    let honest_fee = fee(0) + fee(1);
    for (d, name) in ["honest-1", "honest-2", "adversary"].iter().enumerate() {
        let applied_tx = ups.iter().map(|ch| ch[d].metrics.count("applied")).sum::<u64>();
        let refused_tx = ups.iter().map(|ch| ch[d].metrics.count("refused")).sum::<u64>();
        let deferrals = ups.iter().map(|ch| ch[d].metrics.deferrals).sum::<u64>();
        lines.push(format!(
            "{name}\tapplied tx\t{applied_tx}\trefused tx\t{refused_tx}\tgas applied\t{}\tgas refused\t{}\tfee\t{}\tdeferrals\t{deferrals}",
            gas_ok(d),
            gas_bad(d),
            fee(d)
        ));
    }
    let adv_junk = l.log.iter().filter(|(p, label, _)| *p == ups[0][2].payer_id && label.contains("unchecked")).count();
    let share1 = fee(0) as f64 / honest_fee as f64;
    let share2 = fee(1) as f64 / honest_fee as f64;
    let ratio = honest_bad as f64 / honest_ok as f64;
    lines.push(format!("honest refused gas / applied gas\t{ratio:.4}\t(gate <= 0.10)"));
    lines.push(format!("honest-1 share of honest fees\t{share1:.4}\thonest-2\t{share2:.4}\t(gate 0.35-0.65 each)"));
    lines.push(format!(
        "updates\t{total_updates}\tadversary junk Opens\t{adv_junk}\tledger transactions\t{}",
        l.log.len()
    ));
    let text = lines.join("\n") + "\n";
    println!("{text}");
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("results");
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("gate.txt"), &text).unwrap();
    assert!(ratio <= 0.10, "honest refused gas {ratio:.4} of applied");
    assert!((0.35..=0.65).contains(&share1) && (0.35..=0.65).contains(&share2), "shares {share1:.3} {share2:.3}");
}
