//! The updater against LEZ v0.4.0-rc1's state machine (the harness ledger, RISC0_DEV_MODE), fed
//! the lab's recorded mainnet and Sepolia light-client data (`lez/lab/results/record/net/`):
//! catch-up, honest and faulty sources, a restart in the middle of a job and of a period, two
//! updaters sharing the work, and the A3 done gate with an adversarial updater.
#![cfg(feature = "harness")]

use std::{fs, path::PathBuf};
use ferrata_updater::lez::PayerKey;

use sha2::Digest as _;

use ferrata_core::{witness::CompactStore, Root};
use ferrata_lez_wire::Config;
use ferrata_updater::{
    chain::{decode_finality_update, decode_update, encode_finality_update, encode_update, preset, root},
    config::{Fees, Policy},
    ledger::{HarnessLedger, Ledger},
    source::{lab_record, load_entries, Clock, Entry, Fault, Faulty, Recorded, Source},
    updater::{Instance, Plan, Step, Updater},
};


fn key(b: u8) -> PayerKey {
    ferrata_updater::lez::payer_key([b; 32])
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
    /// A source serving copies of every recorded update (`copy_of`), `lag` entries behind.
    fn copies(&self, label: &str, clock: &Clock, lag: usize) -> Box<dyn Source> {
        let mut s = self.source(label, clock, lag);
        for e in &mut s.entries {
            e.bytes = copy_of(&e.bytes, e.period_update, &self.config);
        }
        Box::new(s)
    }
    fn updater(
        &self,
        label: &str,
        payer: PayerKey,
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
    let spec = r.config.spec();
    // Stop the clock before the record's last finality update that moves the store inside its period (when this
    // test was written, three entries before the end, in period 1410; the lab's record has grown since).
    let head = |upto: usize| r.entries[..upto].iter().map(|e| e.finalized_slot).max().unwrap();
    let stop = (2..n - 1)
        .rev()
        .find(|&s| {
            let e = &r.entries[s];
            !e.period_update
                && e.finalized_slot > head(s)
                && spec.period_at_slot(e.finalized_slot) == spec.period_at_slot(head(s))
        })
        .expect("a finality update inside a period");
    let clock = Clock::new(stop);
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
    let before = store(&u1, &l);
    let period = spec.period_at_slot(before.finalized.slot);
    let next = &r.entries[stop];
    assert!(!next.period_update);
    assert_eq!(spec.period_at_slot(next.finalized_slot), period, "the next update is in the same period");
    println!("restart: entry {stop} of {n}, finalized slot {} inside period {period}", next.finalized_slot);
    clock.set(stop + 1);
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

/// The A3 done gate's chain (`run_gate`): every transaction an updater sent, as it settled.
struct Sent {
    chain: usize,
    /// The round (each round is a block).
    block: usize,
    /// 0 and 1: the honest updaters; 2: the adversary.
    daemon: usize,
    job: Root,
    step: Step,
    /// It is its job's last step (`Finish`, or the initialization's last `Adopt` part).
    last: bool,
    applied: bool,
    gas: u64,
}

/// What `run_gate` runs.
struct GateOpts {
    chains: Vec<&'static str>,
    adversary: bool,
    salt: String,
    seed: Option<u64>,
    /// `None`: Ethereum moves on once both honest updaters have nothing left to do on a chain (A3's gate: every
    /// recorded update is the newest once); `Some(k)`: a new recorded update every `k` rounds whatever the updaters do
    /// (as the sequencer gate's replay: jobs overlap).
    reveal_every: Option<usize>,
    /// The honest updaters' `policy.join_wait_rounds` (`None`: the default).
    honest_join_wait: Option<u64>,
    /// honest-1 alone (honest-2 never plans): what one updater pays on the same schedule.
    solo: bool,
    /// The honest updaters read the chain every `polls[d]` rounds (A3's gate: every round); an updater whose transaction
    /// is not settled yet does not read it (the daemon's `submit` waits for its transaction's block).
    polls: [usize; 2],
    /// A block every `block_every` rounds (A3's gate: every round): the transactions sent since the last block settle
    /// together, in a shuffled order.
    block_every: usize,
    /// The ledger gives the updaters its height (`Ledger::height`: the blocks settled), the clock their turns count; off:
    /// none, and each counts its own rounds (the updater before the turns counted blocks).
    heights: bool,
    /// honest-2's sources serve copies of every update (its signature slot moved by one, `copy_of`), never the bytes
    /// honest-1's sources serve: each job honest-1 opens is of a copy honest-2 was never served, and the reverse.
    copies: bool,
    /// The ledger gives the transactions it applied (`Ledger::messages_to`), from which an updater learns the updates of
    /// other parties' jobs; off: it does not (the updater before it learned them).
    messages: bool,
}

impl GateOpts {
    /// A3's gate: every recorded update the newest once, two honest updaters reading the chain each round, a block a
    /// round, the ledger's height and transactions given.
    fn a3(chains: Vec<&'static str>, adversary: bool) -> Self {
        GateOpts {
            chains,
            adversary,
            salt: String::new(),
            seed: None,
            reveal_every: None,
            honest_join_wait: None,
            solo: false,
            polls: [1, 1],
            block_every: 1,
            heights: true,
            copies: false,
            messages: true,
        }
    }
}

/// A copy of the update `bytes` (a period update when `period`): its `signature_slot`, which the signature does not cover
/// (only its fork version), moved by one slot, later unless that leaves its sync-committee period, then earlier; the same
/// finalized header, so its job leaves the store where the original's does, but other bytes, so another job id.
fn copy_of(bytes: &[u8], period: bool, config: &Config) -> Vec<u8> {
    let (mut u, layout) = if period { decode_update(bytes, config) } else { decode_finality_update(bytes, config) }.unwrap();
    let spec = config.spec();
    let s = u.signature_slot;
    let same = |t: u64| spec.period_at_slot(t) == spec.period_at_slot(s) && t > u.attested_header.beacon.slot;
    u.signature_slot = if same(s + 1) {
        s + 1
    } else if same(s - 1) {
        s - 1
    } else {
        panic!("no copy of the update signed at {s}")
    };
    let out = if period { encode_update(&u, layout) } else { encode_finality_update(&u, layout) };
    let back = if period { decode_update(&out, config) } else { decode_finality_update(&out, config) }.unwrap();
    assert_eq!(back.0, u, "the copy decodes to the update it encodes");
    assert_ne!(out, bytes);
    out
}

struct GateRun {
    rounds: usize,
    lines: Vec<String>,
    ratio: f64,
    share1: f64,
    share2: f64,
    sent: Vec<Sent>,
    /// Per chain, per recorded finality update: the first round the sources served it and the round whose block first
    /// left the instance at a finalized slot at least as far (`None`: not covered).
    latency: Vec<Vec<(usize, Option<usize>)>>,
}

/// Two honest updaters and (`opts.adversary`) one adversarial updater follow the recorded chains on one LEZ chain while
/// the record replays one update at a time. Each round is a block: every updater plans against the state the previous
/// block left, then the block's transactions settle in a shuffled order (so two updaters that send the same step in one
/// block collide, as on a sequencer). The adversary races every due step a third of the time without waiting its turn,
/// does not wait for other parties' jobs (`join_wait_rounds = 0`), and opens a job for a forged update and replays an
/// applied one each time a new update appears.
fn run_gate(opts: &GateOpts) -> GateRun {
    let recs: Vec<Rec> = opts.chains.iter().map(|n| rec(n)).collect();
    let (h1, h2, adv) = (key(0x31), key(0x32), key(0x33));
    let mut l = HarnessLedger::new(&[h1.clone(), h2.clone(), adv.clone()]);
    // The blocks settled so far: the height the updaters' reads reflect.
    let mut height = 0u64;
    if opts.heights {
        l.height = Some(height);
    }
    l.serve_messages = opts.messages;
    let clocks: Vec<Clock> = recs.iter().map(|_| Clock::new(1)).collect();
    let salt = &opts.salt;
    let honest = Policy { join_wait_rounds: opts.honest_join_wait, ..policy(8) };
    // Per chain: [H1, H2, A].
    let mut ups: Vec<Vec<Updater>> = recs
        .iter()
        .zip(&clocks)
        .map(|(r, c)| {
            let h2_sources: Vec<Box<dyn Source>> = if opts.copies {
                vec![r.copies("b", c, 0), r.copies("c", c, 0), r.copies("stale", c, 3)]
            } else {
                vec![r.honest("b", c), r.honest("c", c), Box::new(r.source("stale", c, 3))]
            };
            vec![
                r.updater(
                    &format!("honest-1{salt}"),
                    h1.clone(),
                    vec![r.honest("a", c), r.honest("b", c), r.faulty("liar", c, Fault::BadSignature)],
                    None,
                    honest.clone(),
                ),
                r.updater(&format!("honest-2{salt}"), h2.clone(), h2_sources, None, honest.clone()),
                r.updater(
                    "adversary",
                    adv.clone(),
                    vec![r.honest("a", c)],
                    None,
                    Policy { join_wait_rounds: Some(0), ..policy(1) },
                ),
            ]
        })
        .collect();
    let mut rng = Rng(opts.seed.unwrap_or(0x9e37_79b9_7f4a_7c15));
    let adversary = opts.adversary;
    let mut junk_due = vec![false; recs.len()];
    let mut sent = Vec::new();
    let mut latency: Vec<Vec<(usize, Option<usize>)>> = recs.iter().map(|_| Vec::new()).collect();
    // Per chain and updater: a transaction sent and not settled yet (it does not read the chain until it is), and
    // whether its last plan found nothing to do.
    let mut busy = vec![[false; 3]; recs.len()];
    let mut idle_last = vec![[true; 3]; recs.len()];
    // The transactions sent since the last block.
    #[allow(clippy::type_complexity)]
    let mut pending: Vec<(usize, usize, Option<ferrata_updater::updater::Action>, Option<ferrata_updater::ledger::Tx>)> =
        Vec::new();
    let mut rounds: usize = 0;
    loop {
        rounds += 1;
        assert!(rounds < 40_000, "the gate run did not finish");
        // Plan against the state the last block left.
        for (c, chain) in ups.iter_mut().enumerate() {
            for (d, u) in chain.iter_mut().enumerate() {
                if (opts.solo && d == 1) || busy[c][d] {
                    continue;
                }
                if d < 2 && !rounds.is_multiple_of(opts.polls[d]) {
                    continue;
                }
                // The adversary reads the chain in the first round of each block (each round, in A3's gate), and races a
                // third of the steps due then, as in A3's gate.
                if d == 2 && !(rounds - 1).is_multiple_of(opts.block_every) {
                    continue;
                }
                let plan = u.plan(&l).unwrap();
                if d < 2 {
                    idle_last[c][d] = !matches!(plan, Plan::Send(_) | Plan::Deferred(_));
                }
                match plan {
                    Plan::Send(a) => {
                        // The adversary races a third of the time.
                        if d < 2 || (adversary && rng.next().is_multiple_of(3)) {
                            busy[c][d] = true;
                            pending.push((c, d, Some(a), None));
                        }
                    }
                    Plan::Stuck(w) if d < 2 => panic!("{} {}: {w}", u.label, u.inst.name),
                    _ => {}
                }
            }
            // The adversary's junk, once per new update: a forged update's Open, an applied
            // update's Open again.
            if adversary && junk_due[c] && chain[2].inst.store(&l).unwrap().is_some() {
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
                    pending.push((c, 2, None, Some(chain[2].open_unchecked(&l, &forged).unwrap())));
                }
                if t >= 3 {
                    let e = &r.entries[t - 3];
                    let old = if e.period_update {
                        decode_update(&e.bytes, &r.config)
                    } else {
                        decode_finality_update(&e.bytes, &r.config)
                    };
                    if let Ok((old, _)) = old {
                        pending.push((c, 2, None, Some(chain[2].open_unchecked(&l, &old).unwrap())));
                    }
                }
            }
        }
        if rounds.is_multiple_of(opts.block_every) {
            height += 1;
            let mut block = std::mem::take(&mut pending);
            rng.shuffle(&mut block);
            for (c, d, action, junk) in block {
                busy[c][d] = false;
                let u = &mut ups[c][d];
                match (action, junk) {
                    (Some(a), _) => {
                        if let Some(out) = u.submit(&mut l, &a, false).unwrap() {
                            let parts = recs[c].config.adoption_parts;
                            let last = a.step == Step::Finish
                                || (matches!(a.step, Step::Adopt(p) if p + 1 == parts) && a.tx.actors.len() > 1);
                            sent.push(Sent {
                                chain: c,
                                block: height as usize,
                                daemon: d,
                                job: a.job_id,
                                step: a.step,
                                last,
                                applied: out.applied,
                                gas: out.gas_exec + out.gas_stor,
                            });
                        }
                    }
                    (None, Some(tx)) => {
                        let out = l.submit(&adv, &tx, 4_800_000, 1_000_000_000).unwrap();
                        // A junk job is never finished: named by its account.
                        let job: Root = sha2::Sha256::digest(format!("{:?}", tx.actors.get(1)).as_bytes()).into();
                        sent.push(Sent {
                            chain: c,
                            block: height as usize,
                            daemon: 2,
                            job,
                            step: Step::Open,
                            last: false,
                            applied: out.applied,
                            gas: out.gas_exec + out.gas_stor,
                        });
                    }
                    _ => {}
                }
            }
            if opts.heights {
                l.height = Some(height);
            }
            // What each instance holds after the block: the revealed finality updates it now covers.
            for (c, r) in recs.iter().enumerate() {
                if let Some(s) = ups[c][0].inst.store(&l).unwrap() {
                    for (i, (_, covered)) in latency[c].iter_mut().enumerate() {
                        if covered.is_none() && s.finalized.slot >= r.entries[i].finalized_slot {
                            *covered = Some(rounds);
                        }
                    }
                }
            }
        }
        // Ethereum moves on: once both honest updaters have nothing left to do on a chain, or every `reveal_every`
        // rounds.
        let mut done = true;
        for c in 0..recs.len() {
            let at_end = clocks[c].now() >= recs[c].entries.len();
            let idle = (0..2).all(|d| (opts.solo && d == 1) || (idle_last[c][d] && !busy[c][d]));
            if !(idle && at_end) {
                done = false;
            }
            let due = match opts.reveal_every {
                None => idle,
                Some(k) => rounds.is_multiple_of(k),
            };
            if due && !at_end {
                clocks[c].set(clocks[c].now() + 1);
                junk_due[c] = true;
                // Not idle until each honest updater has read the chain again.
                idle_last[c] = [false; 3];
            }
            // Entries served from the next round on (the first entry from round 1).
            while latency[c].len() < clocks[c].now() {
                let at = if latency[c].is_empty() { 1 } else { rounds + 1 };
                latency[c].push((at, None));
            }
        }
        if done {
            break;
        }
    }
    // Every recorded update's effect reached, and the instance where the lab's ended.
    let mut lines = vec![format!(
        "gate\trounds\t{rounds}\tseed\t{}\tadversary\t{adversary}\tsalt\t{salt}",
        opts.seed.map_or("default".into(), |s| s.to_string())
    )];
    if let Some(k) = opts.reveal_every {
        lines[0] += &format!("\treveal every\t{k}\thonest join_wait_rounds\t{:?}", opts.honest_join_wait);
    }
    if opts.solo {
        lines[0] += "\thonest-1 alone";
    }
    if opts.polls != [1, 1] || opts.block_every != 1 {
        lines[0] += &format!(
            "\thonest-1 reads every\t{}\thonest-2 reads every\t{}\ta block every\t{} rounds",
            opts.polls[0], opts.polls[1], opts.block_every
        );
    }
    if !opts.heights {
        lines[0] += "\tturns counted in each updater's own rounds (no height)";
    }
    if opts.copies {
        lines[0] += "\thonest-2 served copies";
    }
    if !opts.messages {
        lines[0] += "\tno Opens learned from the chain";
    }
    for (c, r) in recs.iter().enumerate() {
        let s = store(&ups[c][0], &l);
        let jobs = applied(&ups[c][0], &l);
        assert_eq!(s.finalized.slot, r.last_finalized().0, "{}", r.name);
        if opts.reveal_every.is_none() {
            assert_eq!(
                jobs,
                1 + r.entries.len() as u64,
                "{}: the activation and every recorded update, each once",
                r.name
            );
        }
        lines.push(format!(
            "{}\tupdates applied\t{}\tjobs (with Init)\t{jobs}\tfinalized slot\t{}",
            r.name,
            jobs - 1,
            s.finalized.slot
        ));
    }
    // Per daemon, across the chains.
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
    let total_updates: usize = recs.iter().map(|r| r.entries.len()).sum();
    lines.push(format!(
        "updates\t{total_updates}\tadversary junk Opens\t{adv_junk}\tledger transactions\t{}",
        l.log.len()
    ));
    let waits = |d: usize| ups.iter().map(|ch| ch[d].metrics.job_waits).sum::<u64>();
    let joined = |d: usize| ups.iter().map(|ch| ch[d].metrics.jobs_joined).sum::<u64>();
    let passed =
        |d: usize| ups.iter().map(|ch| ch[d].metrics.stalled_jobs_passed + ch[d].metrics.wait_caps).sum::<u64>();
    lines.push(format!(
        "join or wait\thonest-1 rounds waited\t{}\tjoined\t{}\twaits ended by a stall or the cap\t{}\thonest-2 rounds waited\t{}\tjoined\t{}\twaits ended by a stall or the cap\t{}",
        waits(0),
        joined(0),
        passed(0),
        waits(1),
        joined(1),
        passed(1)
    ));
    if opts.copies || !opts.messages {
        let learned = |d: usize| ups.iter().map(|ch| ch[d].metrics.updates_from_chain).sum::<u64>();
        lines.push(format!(
            "updates learned from other parties' Opens\thonest-1\t{}\thonest-2\t{}",
            learned(0),
            learned(1)
        ));
    }
    let latency: Vec<Vec<(usize, Option<usize>)>> = latency
        .into_iter()
        .zip(&recs)
        .map(|(v, r)| v.into_iter().zip(&r.entries).filter(|(_, e)| !e.period_update).map(|(x, _)| x).collect())
        .collect();
    GateRun { rounds, lines, ratio, share1, share2, sent, latency }
}

/// The jobs of a gate run (`run_gate`): a job is completed when its last step is applied; a job with an applied step
/// that never completed is abandoned when another job completed at or after its first applied step (it moved the store
/// the job began on, which a job id binds, so the job's remaining steps could no longer apply), as
/// `bench/perf/summarize_gate.py` counts them on the sequencer.
struct Fates {
    /// Per daemon, execution and storage gas: of applied steps of completed jobs, of abandoned jobs, of refused steps
    /// (the adversary's include its junk `Open`s, sent outside its updater).
    gas: [(u64, u64, u64); 3],
    abandoned: Vec<Abandoned>,
    completed: usize,
    /// Jobs with an applied step that neither completed nor were overtaken (in flight at the end), and their gas.
    unfinished: (usize, u64),
}

struct Abandoned {
    chain: usize,
    /// The daemon whose `Open` of it was applied (for a job without one, the first to apply a step of it).
    opener: usize,
    /// Its applied steps.
    steps: usize,
    /// The round of its `Open`.
    open: Option<usize>,
    /// The job that overtook it (the first to complete at or after its first applied step), and that job's `Open` round.
    over: Root,
    over_open: Option<usize>,
}

fn fates(sent: &[Sent]) -> Fates {
    use std::collections::BTreeMap;
    let mut jobs: BTreeMap<(usize, Root), Vec<&Sent>> = BTreeMap::new();
    for s in sent.iter().filter(|s| s.applied) {
        jobs.entry((s.chain, s.job)).or_default().push(s);
    }
    let done: BTreeMap<(usize, Root), usize> =
        jobs.iter().filter_map(|(k, v)| v.iter().find(|s| s.last).map(|s| (*k, s.block))).collect();
    let open_round =
        |k: &(usize, Root)| jobs.get(k).and_then(|v| v.iter().find(|s| s.step == Step::Open)).map(|s| s.block);
    let mut gas = [(0u64, 0u64, 0u64); 3];
    let mut abandoned = Vec::new();
    let mut unfinished = (0usize, 0u64);
    for (k, v) in &jobs {
        if done.contains_key(k) {
            for s in v {
                gas[s.daemon].0 += s.gas;
            }
            continue;
        }
        let first = v.iter().map(|s| s.block).min().expect("an applied step");
        let over = done.iter().filter(|(j, b)| j.0 == k.0 && **b >= first).min_by_key(|(_, b)| **b);
        let Some((oj, _)) = over else {
            unfinished.0 += 1;
            unfinished.1 += v.iter().map(|s| s.gas).sum::<u64>();
            continue;
        };
        for s in v {
            gas[s.daemon].1 += s.gas;
        }
        let opener = v.iter().find(|s| s.step == Step::Open).map_or(v[0].daemon, |s| s.daemon);
        abandoned.push(Abandoned {
            chain: k.0,
            opener,
            steps: v.len(),
            open: open_round(k),
            over: oj.1,
            over_open: open_round(oj),
        });
    }
    for s in sent.iter().filter(|s| !s.applied) {
        gas[s.daemon].2 += s.gas;
    }
    Fates { gas, abandoned, completed: done.len(), unfinished }
}

impl Fates {
    /// The honest updaters' abandoned jobs, and those opened in the same block as the job that overtook them (each
    /// planned against a state without the other: neither could read the other's job).
    fn honest(&self) -> (Vec<&Abandoned>, usize) {
        let h: Vec<&Abandoned> = self.abandoned.iter().filter(|a| a.opener < 2).collect();
        let same = h.iter().filter(|a| a.open.is_some() && a.open == a.over_open).count();
        (h, same)
    }

    fn lines(&self) -> Vec<String> {
        let (h, same) = self.honest();
        let hgas: u64 = self.gas[0].1 + self.gas[1].1;
        let hdone: u64 = self.gas[0].0 + self.gas[1].0;
        let open_only = h.iter().filter(|a| a.steps == 1).count();
        let mut out = vec![format!(
            "jobs\tcompleted\t{}\tabandoned (applied steps, never completed, overtaken)\t{}\tof them opened by an honest updater\t{}\topened in the block of the job that overtook it\t{same}\twith their Open alone applied\t{open_only}",
            self.completed,
            self.abandoned.len(),
            h.len()
        )];
        for (d, name) in ["honest-1", "honest-2", "adversary (refused: its junk Opens too)"].iter().enumerate() {
            let (a, b, c) = self.gas[d];
            out.push(format!(
                "{name}\texecution and storage gas in completed jobs\t{a}\tin abandoned jobs\t{b}\ton refused steps\t{c}"
            ));
        }
        out.push(format!("unfinished jobs (in flight at the end)\t{}\tgas\t{}", self.unfinished.0, self.unfinished.1));
        out.push(format!(
            "honest gas in abandoned jobs / in completed jobs\t{:.4}",
            if hdone == 0 { 0.0 } else { hgas as f64 / hdone as f64 }
        ));
        out
    }
}

/// The median (of an even count, the mean of the middle two, as Python's `statistics.median`) and the value at index
/// round(0.9 (n - 1)) of the sorted values (halves to even, as `bench/perf/summarize_gate.py`'s `pct`).
fn median_p90(v: &mut [usize]) -> (f64, usize) {
    v.sort_unstable();
    let n = v.len();
    if n == 0 {
        return (0.0, 0);
    }
    let med = if n % 2 == 1 { v[n / 2] as f64 } else { (v[n / 2 - 1] + v[n / 2]) as f64 / 2.0 };
    let x = 0.9 * (n - 1) as f64;
    let i = if (x - x.floor() - 0.5).abs() < 1e-9 && (x.floor() as usize) % 2 == 0 { x.floor() } else { x.round() };
    (med, v[i as usize])
}

fn gate_lines(run: &GateRun, f: &Fates) -> String {
    let mut lines = run.lines.clone();
    lines.extend(f.lines());
    let mut lat: Vec<usize> = run.latency.iter().flatten().filter_map(|(a, b)| b.map(|b| b - a)).collect();
    let covered = lat.len();
    let total = run.latency.iter().map(|v| v.len()).sum::<usize>();
    let (med, p90) = median_p90(&mut lat);
    lines.push(format!(
        "latency (rounds from the first round a finality update is served to the round whose block leaves the instance at least as fresh, inclusive)\tcovered\t{covered}\tof\t{total}\tmedian\t{med:.1}\tp90\t{p90}"
    ));
    lines.join("\n") + "\n"
}

/// The A3 done gate on recorded data (`run_gate`): every recorded update is the newest once, two honest updaters and
/// the adversary. Each update is applied exactly once; the honest updaters' gas on refused steps is at most 10% of their
/// gas on applied steps, and each pays 35–65% of the honest fees. Abandoned jobs (an applied step, never completed,
/// overtaken) are counted too (RFP-021 R.9).
#[test]
fn gate_two_honest_one_adversary() {
    // GATE_SALT: other names for the honest updaters, so another draw of their deferral waits (a wait is a hash of the
    // updater's name and key, the job and the step). GATE_SEED: another order of every block's transactions (and of
    // the adversary's races). GATE_ADVERSARY=0: the two honest updaters alone (a baseline for what deferral costs).
    let salt = std::env::var("GATE_SALT").unwrap_or_default();
    let seed = std::env::var("GATE_SEED").ok().and_then(|v| v.parse::<u64>().ok());
    let adversary = std::env::var("GATE_ADVERSARY").ok().as_deref() != Some("0");
    let opts = GateOpts { salt: salt.clone(), seed, ..GateOpts::a3(vec!["mainnet", "sepolia"], adversary) };
    let run = run_gate(&opts);
    let f = fates(&run.sent);
    let text = gate_lines(&run, &f);
    println!("{text}");
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("results");
    fs::create_dir_all(&out).unwrap();
    let name = match (seed, adversary, salt.as_str()) {
        (_, _, s) if !s.is_empty() => format!("gate-salt-{s}.txt"),
        (_, false, _) => "gate-no-adversary.txt".to_string(),
        (None, true, _) => "gate.txt".to_string(),
        (Some(s), true, _) => format!("gate-seed-{s}.txt"),
    };
    fs::write(out.join(name), &text).unwrap();
    let (ratio, share1, share2) = (run.ratio, run.share1, run.share2);
    assert!(ratio <= 0.10, "honest refused gas {ratio:.4} of applied");
    assert!((0.35..=0.65).contains(&share1) && (0.35..=0.65).contains(&share2), "shares {share1:.3} {share2:.3}");
    // Every honest job abandoned was opened in the block of the job that overtook it (neither updater could read the
    // other's job when it sent its Open), or overtaken by a job the adversary (which waits for no one) opened. In the
    // default run none is abandoned: Ethereum moves on only when both honest updaters are idle.
    let (h, same) = f.honest();
    let pushed_by_adversary = |a: &Abandoned| {
        run.sent
            .iter()
            .any(|s| s.chain == a.chain && s.job == a.over && s.applied && s.daemon == 2 && s.step == Step::Open)
    };
    let explained = h.iter().filter(|a| (a.open.is_some() && a.open == a.over_open) || pushed_by_adversary(a)).count();
    assert_eq!(
        explained,
        h.len(),
        "honest abandoned jobs neither opened in the block of the job that overtook them nor overtaken by a job the \
         adversary opened: {} of {} ({same} in the same block)",
        h.len() - explained,
        h.len()
    );
}

/// RFP-021 R.9 with overlapping jobs (updates revealed faster than two updaters apply them, as in the sequencer gate's
/// replay): the two honest updaters alone on the recorded mainnet chain, a new update every 6 rounds whatever they do,
/// with join or wait (the default) and without it (`join_wait_rounds = 0`, the updater before it). In this chain every
/// transaction of a block is readable in the next round (no finality delay, unlike a sequencer), and both updaters are
/// served the same bytes, so two `Open`s in one block are of one job (one refused), never of two: without join or wait
/// every abandoned job was opened once the job that overtook it could be read; with it none is abandoned (the residue
/// of two jobs opened in one block is constructed in `tests/join_wait.rs`). R.9's own measure, the gas the two updaters
/// spend besides the completed jobs (applied steps of abandoned jobs, refused steps) over the completed jobs' gas, is
/// held to A3's 10% with it; honest-1 alone on the same schedule spends nothing besides (it applies fewer updates: a
/// second updater applies more of the overlapping ones). The instance ends where the record does, and A3's two limits
/// hold. The three runs' figures go to `results/gate-overlap.txt`.
#[test]
fn gate_overlapping_updates_join_or_wait() {
    let mut text = String::new();
    let mut runs = Vec::new();
    for (join_wait, solo) in [(None, false), (Some(0), false), (None, true)] {
        let opts = GateOpts {
            reveal_every: Some(6),
            honest_join_wait: join_wait,
            solo,
            ..GateOpts::a3(vec!["mainnet"], false)
        };
        let run = run_gate(&opts);
        let f = fates(&run.sent);
        text += &gate_lines(&run, &f);
        // R.9's measure: what the updaters spent besides the completed jobs (applied steps of abandoned jobs, refused
        // steps), over the completed jobs' gas; one updater alone spends nothing besides.
        let done: u64 = (0..2).map(|d| f.gas[d].0).sum();
        let besides: u64 = (0..2).map(|d| f.gas[d].1 + f.gas[d].2).sum();
        let waste = besides as f64 / done as f64;
        text += &format!(
            "honest execution and storage gas besides the completed jobs' / the completed jobs'\t{waste:.4}\t({besides} / {done})\n\n"
        );
        runs.push((run, f, waste));
    }
    println!("{text}");
    let (on, f_on, waste_on) = &runs[0];
    let (_, f_off, waste_off) = &runs[1];
    let (_, _, waste_solo) = &runs[2];
    assert_eq!(*waste_solo, 0.0, "one updater alone spends nothing besides its completed jobs");
    assert!(*waste_on <= 0.10, "with join or wait, at most A3's 10% besides the completed jobs: {waste_on:.4}");
    assert!(waste_off > waste_on, "without it, more: {waste_off:.4} against {waste_on:.4}");
    let (h, _) = f_on.honest();
    assert!(h.is_empty() && f_on.abandoned.is_empty(), "with join or wait, no job abandoned: {}", h.len());
    assert!(on.ratio <= 0.10, "A3's ratio {:.4}", on.ratio);
    assert!((0.35..=0.65).contains(&on.share1), "shares {:.3} {:.3}", on.share1, on.share2);
    let (h_off, same_off) = f_off.honest();
    let gas = |f: &Fates| f.gas[0].1 + f.gas[1].1;
    assert!(gas(f_on) < gas(f_off), "join or wait abandons less: {} against {}", gas(f_on), gas(f_off));
    assert!(
        !h_off.is_empty() && same_off == 0,
        "without it, honest jobs abandoned, each opened after the other was readable"
    );
    // Written once the assertions hold (a mutant's run leaves the record as it was).
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("results");
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("gate-overlap.txt"), text.trim_end().to_string() + "\n").unwrap();
}

/// RFP-021 R.10's first clause, that cost does not fall systematically on whichever party reacts fastest: honest-1 reads
/// the chain every round, honest-2 every third round, a new recorded mainnet update every 12 rounds whatever they do
/// (one every 4 blocks at a block every three rounds; the sequencer gate's replay reveals one every 6 blocks). With a
/// block every three rounds (honest-2 reads once a block, honest-1 three times) and the turns counted on the chain's
/// height (`Ledger::height`, the updater now), each honest updater pays 35-65% of the honest fees (A3's limits) and A3's
/// ratio holds; counted in each updater's own rounds (the updater before), the one reading three times as often waits a
/// third as long and pays more than 65%. Then: a block every two rounds (honest-2 reads every 1.5 blocks); A3's whole
/// gate (both recorded chains, every update the newest once, the adversary racing a third of the steps due in each block
/// and waiting for no one) with the pair reading once and three times a block: A3's limits hold in both. Last, honest-2
/// reading every third block (every sixth round, a block every two): it notices its turns up to three blocks late and
/// pays less than honest-1, the residual the operator guide §3.4 states (an operator should read the chain at least once
/// a block). `GATE_POLL_SALT` names the updaters otherwise (another draw of every wait). The runs' figures go to
/// `results/gate-polls.txt` (`gate-polls-salt-SALT.txt`).
#[test]
fn gate_poll_speeds_share_the_cost() {
    let salt = std::env::var("GATE_POLL_SALT").unwrap_or_default();
    let mut text = String::new();
    let mut runs = Vec::new();
    // (heights, reads, a block every, A3's whole gate)
    for (heights, polls, block_every, a3) in [
        (true, [1, 3], 3, false),
        (false, [1, 3], 3, false),
        (true, [1, 3], 2, false),
        (true, [1, 3], 3, true),
        (true, [1, 6], 2, false),
    ] {
        let opts = if a3 {
            GateOpts {
                polls,
                block_every,
                heights,
                salt: salt.clone(),
                ..GateOpts::a3(vec!["mainnet", "sepolia"], true)
            }
        } else {
            GateOpts {
                reveal_every: Some(12),
                polls,
                block_every,
                heights,
                salt: salt.clone(),
                ..GateOpts::a3(vec!["mainnet"], false)
            }
        };
        let run = run_gate(&opts);
        let f = fates(&run.sent);
        text += &gate_lines(&run, &f);
        text += "\n";
        runs.push(run);
    }
    println!("{text}");
    for (i, run) in runs.iter().enumerate() {
        println!("run {i}: honest-1 share {:.4}, A3's ratio {:.4}", run.share1, run.ratio);
    }
    // The record first, so that a draw that fails an assertion below still leaves its figures.
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("results");
    fs::create_dir_all(&out).unwrap();
    let name = if salt.is_empty() { "gate-polls.txt".to_string() } else { format!("gate-polls-salt-{salt}.txt") };
    fs::write(out.join(name), text.trim_end().to_string() + "\n").unwrap();
    let (blocks, rounds) = (&runs[0], &runs[1]);
    for (name, run) in [("heights", blocks), ("heights, 1.5 blocks a read", &runs[2]), ("A3's whole gate", &runs[3])] {
        assert!(
            (0.35..=0.65).contains(&run.share1) && (0.35..=0.65).contains(&run.share2),
            "{name}: shares {:.3} {:.3}",
            run.share1,
            run.share2
        );
        assert!(run.ratio <= 0.10, "{name}: A3's ratio {:.4}", run.ratio);
    }
    assert!(
        rounds.share1 > 0.65 && rounds.share1 > blocks.share1,
        "counted in rounds, the updater reading three times as often pays more: {:.3} (heights: {:.3})",
        rounds.share1,
        blocks.share1
    );
    // A reader every third block notices its turns late and pays less than half: 0.329 in the recorded draw, 0.431 and
    // 0.473 under GATE_POLL_SALT a and b (a measured residual of RFP-021 R.10, not a goal).
    assert!(runs[4].share2 < 0.50, "a reader every third block pays less than half: {:.3}", runs[4].share2);
}

/// RFP-021 R.9 when the updaters' sources serve different copies of each update (honest-2's sources serve every update
/// with its signature slot moved by one, `copy_of`: the same finalized header, other bytes, another job id), with jobs
/// overlapping (a new recorded mainnet update every 6 rounds, as `gate_overlapping_updates_join_or_wait`). Learning the
/// updates of other parties' jobs from their `Open`s on chain (`Ledger::messages_to`, the updater now), each updater
/// finds the job the other opened for its copy and joins it: the gas spent besides the completed jobs' is held to A3's
/// 10%, and every honest job abandoned was opened in the block of the job that overtook it (neither could read the
/// other's). Without it (the updater before), each opens its own copy's job beside the other's, and more is abandoned.
/// The runs' figures go to `results/gate-copies.txt`.
#[test]
fn gate_copies_join_or_wait() {
    let mut text = String::new();
    let mut runs = Vec::new();
    for messages in [true, false] {
        let opts = GateOpts { reveal_every: Some(6), copies: true, messages, ..GateOpts::a3(vec!["mainnet"], false) };
        let run = run_gate(&opts);
        let f = fates(&run.sent);
        text += &gate_lines(&run, &f);
        let done: u64 = (0..2).map(|d| f.gas[d].0).sum();
        let besides: u64 = (0..2).map(|d| f.gas[d].1 + f.gas[d].2).sum();
        let waste = besides as f64 / done as f64;
        text += &format!(
            "honest execution and storage gas besides the completed jobs' / the completed jobs'\t{waste:.4}\t({besides} / {done})\n\n"
        );
        runs.push((run, f, waste));
    }
    println!("{text}");
    let (on, f_on, waste_on) = &runs[0];
    let (_, f_off, waste_off) = &runs[1];
    assert!(*waste_on <= 0.10, "learning other parties' copies, at most A3's 10% besides the completed jobs: {waste_on:.4}");
    assert!(waste_off > waste_on, "without it, more: {waste_off:.4} against {waste_on:.4}");
    let (h, same) = f_on.honest();
    assert_eq!(same, h.len(), "every honest job abandoned was opened in the block of the job that overtook it");
    let gas = |f: &Fates| f.gas[0].1 + f.gas[1].1;
    assert!(gas(f_on) < gas(f_off), "learning abandons less: {} against {}", gas(f_on), gas(f_off));
    assert!(on.ratio <= 0.10, "A3's ratio {:.4}", on.ratio);
    assert!((0.35..=0.65).contains(&on.share1), "shares {:.3} {:.3}", on.share1, on.share2);
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("results");
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("gate-copies.txt"), text.trim_end().to_string() + "\n").unwrap();
}

/// A beacon node on loopback serving a recorded chain over the beacon API (SSZ, the `updates`
/// endpoint's length-prefixed chunks), or failing every request with `status`.
fn mock_beacon(rec: Recorded, fail_status: Option<u16>) -> String {
    mock_beacon_reporting(rec, fail_status, None)
}

/// As [`mock_beacon`], also answering `/eth/v1/node/version` with `version` (a recorded answer, JSON).
fn mock_beacon_reporting(rec: Recorded, fail_status: Option<u16>, version: Option<String>) -> String {
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        sync::{Arc, Mutex},
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let rec = Arc::new(Mutex::new(rec));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            // Drain the headers.
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).is_err() || h == "\r\n" || h.is_empty() {
                    break;
                }
            }
            let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
            let mut r = rec.lock().unwrap();
            let body: Result<Vec<u8>, ()> = if fail_status.is_some() {
                Err(())
            } else if path == "/eth/v1/node/version" {
                version.clone().map(String::into_bytes).ok_or(())
            } else if let Some(root) = path.strip_prefix("/eth/v1/beacon/light_client/bootstrap/0x") {
                r.bootstrap(&root_of(root)).map_err(|_| ())
            } else if path == "/eth/v1/beacon/light_client/finality_update" {
                r.finality_update().map_err(|_| ())
            } else if let Some(q) = path.strip_prefix("/eth/v1/beacon/light_client/updates?") {
                let arg = |k: &str| {
                    q.split('&').find_map(|kv| kv.strip_prefix(k)).and_then(|v| v.parse::<u64>().ok()).unwrap()
                };
                let list = r.updates(arg("start_period="), arg("count=")).unwrap();
                let mut out = Vec::new();
                for u in list {
                    // length (fork digest + payload), fork digest, payload
                    out.extend_from_slice(&((u.len() + 4) as u64).to_le_bytes());
                    out.extend_from_slice(&[0xab, 0xcd, 0xef, 0x01]);
                    out.extend_from_slice(&u);
                }
                Ok(out)
            } else {
                Err(())
            };
            let (status, body) = match body {
                Ok(b) => ("200 OK".to_string(), b),
                Err(()) => (format!("{} Error", fail_status.unwrap_or(404)), b"{\"code\":404}".to_vec()),
            };
            let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            let _ = stream.write_all(&body);
        }
    });
    format!("http://{addr}")
}

fn root_of(hex_str: &str) -> Root {
    root(hex_str).unwrap()
}

/// The HTTP path: `BeaconApi` (curl, SSZ, the `updates` endpoint's chunks) against two beacon
/// nodes on loopback, one serving the recorded mainnet chain and one answering 503 to everything.
#[test]
fn beacon_api_sources_over_http() {
    use ferrata_updater::source::BeaconApi;
    let r = rec("mainnet");
    let clock = Clock::new(r.entries.len());
    let good = mock_beacon(r.source("node", &clock, 0), None);
    let bad = mock_beacon(r.source("node", &clock, 0), Some(503));
    let payer = key(0x27);
    let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
    let sources: Vec<Box<dyn Source>> = vec![
        Box::new(BeaconApi { name: "down".into(), base: bad, timeout_secs: 5 }),
        Box::new(BeaconApi { name: "up".into(), base: good, timeout_secs: 5 }),
    ];
    let mut u = r.updater("http", payer, sources, None, policy(1));
    settle(&mut u, &mut l, 100);
    assert_eq!(store(&u, &l).finalized.slot, r.last_finalized().0);
    assert_eq!(refused_total(&l), 0);
    assert!(u.metrics.sources["down"].errors > 0 && !u.metrics.sources["down"].available);
    assert!(u.metrics.sources["up"].available);
    let (status, why) = u.health();
    assert_eq!(status, "degraded");
    assert!(why.iter().any(|w| w.contains("down unavailable")), "{why:?}");
    println!(
        "http: finalized {} via {} requests to `up`, {} errors from `down`",
        store(&u, &l).finalized.slot,
        u.metrics.sources["up"].requests,
        u.metrics.sources["down"].errors
    );
}

/// Fee limits are declared in every transaction and enforced: a maximum fee below what a step costs
/// gets the transaction rejected by LEZ's settlement (nothing charged); the budget stops sending.
#[test]
fn fee_limits_are_declared_and_enforced() {
    let r = rec("mainnet");
    let clock = Clock::new(r.entries.len());
    let payer = key(0x28);
    let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
    let mut u = r.updater("cheap", payer.clone(), vec![r.honest("a", &clock)], None, policy(1));
    u.fees.max_fee_per_tx = 1_000;
    let before = l.balance(&payer);
    let plan = u.tick(&mut l).unwrap();
    assert!(matches!(plan, Plan::Send(_)));
    let (_, label, out) = l.log.last().unwrap();
    assert!(out.rejected, "{label}: a max fee of 1,000 is below the step's cost: {out:?}");
    assert_eq!(l.balance(&payer), before, "nothing charged");
    assert!(u.inst.state(&l).unwrap().is_none(), "nothing applied");
    assert_eq!(u.metrics.count("rejected"), 1);

    // A budget of two transactions' maximum fees: the third is not sent.
    let payer2 = key(0x29);
    let mut l2 = HarnessLedger::new(std::slice::from_ref(&payer2));
    let mut u2 = r.updater("budget", payer2, vec![r.honest("a", &clock)], None, policy(1));
    u2.fees.max_fee_per_tx = 100_000_000;
    u2.fees.budget = 250_000_000;
    let mut sent = 0;
    let err = loop {
        match u2.tick(&mut l2) {
            Ok(Plan::Send(_)) => sent += 1,
            Ok(_) => {}
            Err(e) => break e,
        }
        assert!(sent < 10, "the budget must stop it");
    };
    assert!(format!("{err:#}").contains("budget reached"), "{err:#}");
    assert!(u2.health().1.iter().any(|w| w == "budget reached"));
    println!("budget: {sent} sent ({} charged) before the stop", u2.metrics.spent());
}

/// The finalized header (slot, root) of every update of the record.
fn finalized_roots(r: &Rec) -> std::collections::BTreeMap<u64, Root> {
    r.entries
        .iter()
        .map(|e| {
            let (u, _) = if e.period_update {
                decode_update(&e.bytes, &r.config)
            } else {
                decode_finality_update(&e.bytes, &r.config)
            }
            .unwrap();
            (u.finalized_header.beacon.slot, u.finalized_header.beacon.hash_tree_root())
        })
        .collect()
}

/// A beacon node on loopback that serves only `/eth/v1/beacon/headers/{slot}` (JSON, as publicnode's): the recorded
/// finalized headers' roots, or (`lie`) another root at every slot.
fn mock_headers(roots: std::collections::BTreeMap<u64, Root>, lie: bool) -> String {
    mock_headers_reporting(roots, lie, None)
}

/// As [`mock_headers`], also answering `/eth/v1/node/version` with `version` (a recorded answer, JSON).
fn mock_headers_reporting(roots: std::collections::BTreeMap<u64, Root>, lie: bool, version: Option<String>) -> String {
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).is_err() || h == "\r\n" || h.is_empty() {
                    break;
                }
            }
            let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
            let slot = path.strip_prefix("/eth/v1/beacon/headers/").and_then(|s| s.parse::<u64>().ok());
            if let (true, Some(v)) = (path == "/eth/v1/node/version", &version) {
                let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{v}", v.len());
                continue;
            }
            let (status, body) = match slot.and_then(|s| roots.get(&s).map(|r| (s, *r))) {
                Some((s, mut r)) => {
                    if lie {
                        r[0] ^= 0xff;
                    }
                    let body = format!(
                        "{{\"execution_optimistic\":false,\"finalized\":true,\"data\":{{\"root\":\"0x{}\",\"canonical\":true,\
                         \"header\":{{\"message\":{{\"slot\":\"{s}\"}}}}}}}}",
                        hex::encode(r)
                    );
                    ("200 OK", body)
                }
                None => ("404 Not Found", "{\"code\":404,\"message\":\"NOT_FOUND\"}".to_string()),
            };
            let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        }
    });
    format!("http://{addr}")
}

/// A headers source (as publicnode's, which serves no light-client data) beside one light-client source, with
/// `min_agreeing = 2`: when it holds each update's finalized header canonical, the updater catches up as with two
/// light-client sources; when it holds another block there, nothing after the checkpoint's `Init` is sent (the
/// updater waits), nothing is refused, and `/health` names the source.
#[test]
fn headers_source_confirms_or_holds_back() {
    use ferrata_updater::source::HeaderWitness;
    let r = rec("mainnet");
    let clock = Clock::new(r.entries.len());
    let roots = finalized_roots(&r);
    for lie in [false, true] {
        let payer = key(if lie { 0x2b } else { 0x2a });
        let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
        let witness = HeaderWitness { name: "headers".into(), base: mock_headers(roots.clone(), lie), timeout_secs: 5 };
        let sources: Vec<Box<dyn Source>> = vec![r.honest("lc", &clock), Box::new(witness)];
        let mut u = r.updater("hdr", payer, sources, None, Policy { min_agreeing: 2, ..policy(1) });
        let h = &u.metrics.sources["headers"];
        assert_eq!((h.confirmed, h.mismatches), (0, 0));
        if !lie {
            settle(&mut u, &mut l, 200);
            assert_eq!(store(&u, &l).finalized.slot, r.last_finalized().0);
            let h = &u.metrics.sources["headers"];
            assert!(h.confirmed > 0 && h.mismatches == 0 && h.available, "{h:?}");
            assert_eq!(u.health().0, "ok", "{:?}", u.health().1);
            println!("headers source honest: {} confirmations, {} jobs applied", h.confirmed, applied(&u, &l));
        } else {
            let mut waiting = 0;
            for _ in 0..30 {
                if let Plan::Waiting(_) = u.tick(&mut l).unwrap() {
                    waiting += 1;
                }
            }
            assert!(waiting >= 25, "the updater waits once the instance is initialised ({waiting} rounds)");
            assert_eq!(applied(&u, &l), 1, "only the initialisation");
            let h = &u.metrics.sources["headers"];
            assert!(h.mismatches > 0 && h.confirmed == 0, "{h:?}");
            let (status, why) = u.health();
            assert_eq!(status, "degraded");
            assert!(why.iter().any(|w| w.contains("headers holds another block")), "{why:?}");
            println!("headers source lying: {waiting} rounds waiting, {} mismatches, health {status}", h.mismatches);
        }
        assert_eq!(refused_total(&l), 0);
    }
}

/// The execution blocks the recorded updates' finalized headers bind: (block hash, its number), the number the
/// payload header carries, or (Gloas: the hash only) one made up in order, which the mock below keeps consistent.
fn finalized_execution(r: &Rec) -> Vec<(Root, u64)> {
    use ferrata_core::types::Execution;
    let mut out = Vec::new();
    for (k, e) in r.entries.iter().enumerate() {
        let (u, _) = if e.period_update {
            decode_update(&e.bytes, &r.config)
        } else {
            decode_finality_update(&e.bytes, &r.config)
        }
        .unwrap();
        match &u.finalized_header.execution {
            Execution::Payload { header, .. } => out.push((header.block_hash, header.block_number)),
            Execution::BlockHash { block_hash, .. } => out.push((*block_hash, 1_000_000 + k as u64)),
            Execution::None => {}
        }
    }
    out
}

/// An execution endpoint on loopback (JSON-RPC over HTTP, as a public Sepolia endpoint): `eth_getBlockByHash` gives
/// each recorded finalized header's execution block with its number, `eth_getBlockByNumber` the block at that
/// number; `lie`: another block at every number (the hash it was asked about is not canonical there).
fn mock_execution(blocks: Vec<(Root, u64)>, lie: bool) -> String {
    use std::{
        io::{BufRead, BufReader, Read, Write},
        net::TcpListener,
    };
    let by_hash: std::collections::BTreeMap<String, u64> =
        blocks.iter().map(|(h, n)| (format!("0x{}", hex::encode(h)), *n)).collect();
    let by_number: std::collections::BTreeMap<u64, String> =
        blocks.iter().map(|(h, n)| (*n, format!("0x{}", hex::encode(h)))).collect();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            let mut length = 0usize;
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).is_err() || h == "\r\n" || h.is_empty() {
                    break;
                }
                if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0u8; length];
            if reader.read_exact(&mut body).is_err() {
                continue;
            }
            let req: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
            let p = &req["params"][0];
            let result = match req["method"].as_str().unwrap_or("") {
                "eth_getBlockByHash" => {
                    let h = p.as_str().unwrap_or("").to_ascii_lowercase();
                    by_hash.get(&h).map_or(serde_json::Value::Null, |n| {
                        serde_json::json!({ "hash": h, "number": format!("0x{n:x}") })
                    })
                }
                "eth_getBlockByNumber" => {
                    let n = u64::from_str_radix(p.as_str().unwrap_or("0x0").trim_start_matches("0x"), 16).unwrap_or(0);
                    match (by_number.get(&n), lie) {
                        (Some(_), true) => serde_json::json!({ "hash": format!("0x{}", "ee".repeat(32)), "number": format!("0x{n:x}") }),
                        (Some(h), false) => serde_json::json!({ "hash": h, "number": format!("0x{n:x}") }),
                        (None, _) => serde_json::Value::Null,
                    }
                }
                _ => serde_json::Value::Null,
            };
            let answer = serde_json::json!({ "jsonrpc": "2.0", "id": req["id"], "result": result }).to_string();
            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}", answer.len());
        }
    });
    format!("http://{addr}")
}

/// An execution source (`kind = "execution"`) beside one light-client source, with `min_agreeing = 2`: when the
/// endpoint holds each update's finalized execution block canonical, the updater catches up as with two light-client
/// sources; when it holds another block at that number, nothing after the checkpoint's `Init` is sent (the updater
/// waits), nothing is refused, and `/health` names the source.
#[test]
fn execution_source_confirms_or_holds_back() {
    use ferrata_updater::source::ExecutionRpc;
    let r = rec("mainnet");
    let clock = Clock::new(r.entries.len());
    let blocks = finalized_execution(&r);
    assert!(!blocks.is_empty(), "the record's headers bind execution blocks");
    for lie in [false, true] {
        let payer = key(if lie { 0x2e } else { 0x2d });
        let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
        let el = ExecutionRpc::new("el", &mock_execution(blocks.clone(), lie), 5);
        let sources: Vec<Box<dyn Source>> = vec![r.honest("lc", &clock), Box::new(el)];
        let mut u = r.updater("exe", payer, sources, None, Policy { min_agreeing: 2, ..policy(1) });
        if !lie {
            settle(&mut u, &mut l, 200);
            assert_eq!(store(&u, &l).finalized.slot, r.last_finalized().0);
            let s = &u.metrics.sources["el"];
            assert!(s.confirmed > 0 && s.mismatches == 0 && s.available, "{s:?}");
            assert_eq!(u.health().0, "ok", "{:?}", u.health().1);
            println!("execution source honest: {} confirmations, {} jobs applied", s.confirmed, applied(&u, &l));
        } else {
            let mut waiting = 0;
            for _ in 0..30 {
                if let Plan::Waiting(_) = u.tick(&mut l).unwrap() {
                    waiting += 1;
                }
            }
            assert!(waiting >= 25, "the updater waits once the instance is initialised ({waiting} rounds)");
            assert_eq!(applied(&u, &l), 1, "only the initialisation");
            let s = &u.metrics.sources["el"];
            assert!(s.mismatches > 0 && s.confirmed == 0, "{s:?}");
            let (status, why) = u.health();
            assert_eq!(status, "degraded");
            assert!(why.iter().any(|w| w.contains("el does not hold a finalized header's execution block")), "{why:?}");
            println!("execution source lying: {waiting} rounds waiting, {} mismatches, health {status}", s.mismatches);
        }
        assert_eq!(refused_total(&l), 0);
    }
}

/// `finish_own_jobs`: a source that moves on to a newer finality update while this updater's job for the previous
/// one is half done (as a beacon node serves only its newest). With it (the default), the updater finishes its own
/// job first, also after a restart on the same state directory; without it, it opens the newer update's job and the
/// first one stays open.
#[test]
fn own_job_is_finished_when_the_source_moves_on() {
    let r = rec("mainnet");
    let fin_of = |i: usize| {
        let e = &r.entries[i];
        assert!(!e.period_update);
        decode_finality_update(&e.bytes, &r.config).unwrap().0.finalized_header.beacon.slot
    };
    // Two finality updates in a row, each further than the one before.
    let i = (1..r.entries.len() - 1)
        .find(|&i| !r.entries[i].period_update && !r.entries[i + 1].period_update && fin_of(i + 1) > fin_of(i))
        .expect("two finality updates in a row");
    let (fin1, fin2) = (fin_of(i), fin_of(i + 1));
    for (finish, restart) in [(true, false), (true, true), (false, false)] {
        let dir = scratch(&format!("own-job-{finish}-{restart}"));
        let clock = Clock::new(i);
        let payer = key(0x2c);
        let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
        let pol = Policy { finish_own_jobs: finish, ..policy(1) };
        let mut u = r.updater("own", payer.clone(), vec![r.honest("a", &clock)], Some(dir.clone()), pol.clone());
        settle(&mut u, &mut l, 200);
        assert!(store(&u, &l).finalized.slot < fin1, "the store is behind the first update");
        clock.set(i + 1);
        match u.tick(&mut l).unwrap() {
            Plan::Send(a) => assert_eq!((a.step, a.finalized_slot), (Step::Open, fin1)),
            other => panic!("expected the first update's Open, got {other:?}"),
        }
        clock.set(i + 2);
        if restart {
            u = r.updater("own", payer.clone(), vec![r.honest("a", &clock)], Some(dir.clone()), pol.clone());
        }
        let next = match u.tick(&mut l).unwrap() {
            Plan::Send(a) => (a.step, a.finalized_slot),
            other => panic!("expected a transaction, got {other:?}"),
        };
        if finish {
            assert_eq!(next, (Step::Miller, fin1), "its own job first (restart {restart})");
        } else {
            assert_eq!(next, (Step::Open, fin2), "the newer update's job");
        }
        settle(&mut u, &mut l, 200);
        assert_eq!(store(&u, &l).finalized.slot, fin2);
        assert_eq!(refused_total(&l), 0);
        let open_left = fs::read_dir(dir.join("mainnet/open")).map(|d| d.count()).unwrap_or(0);
        assert_eq!(open_left, 0, "no kept job left once the store is past it");
        println!(
            "finish_own_jobs {finish}, restart {restart}: after the source moved on, {:?}; {} transactions in all",
            next,
            l.log.len()
        );
    }
}

// ---- client diversity (`policy.min_distinct_clients`) ------------------------------------------------------------

/// `/eth/v1/node/version` as three public beacon APIs answered it on 18 November 2026 (`fixtures/node-version/`,
/// `../results/client-diversity-2026-11-18/`).
fn answer(client: &str) -> String {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/node-version");
    fs::read_to_string(dir.join(format!("{client}.json"))).unwrap()
}

/// What a node with that recorded answer reports.
fn version(client: &str) -> String {
    ferrata_updater::source::parse_version_answer(&answer(client)).unwrap()
}

fn diverse(k: usize) -> Policy {
    Policy { min_distinct_clients: k, ..policy(1) }
}

impl Rec {
    /// A recorded source reporting `client`'s recorded version, `ahead` entries ahead of the clock.
    fn reporting(&self, label: &str, clock: &Clock, client: &str, ahead: usize) -> Recorded {
        let mut s = self.source(label, clock, 0);
        s.client = Some(version(client));
        s.ahead = ahead;
        s
    }
}

/// Rounds until the updater waits (or is up to date); the plans it made.
fn until_waiting(u: &mut Updater, l: &mut HarnessLedger, max: usize) -> Vec<Plan> {
    let mut plans = Vec::new();
    for _ in 0..max {
        let p = u.tick(l).unwrap();
        let stop = matches!(p, Plan::UpToDate | Plan::Waiting(_));
        plans.push(p);
        if stop {
            break;
        }
    }
    plans
}

/// Two sources of different clients that serve the same bytes: the updater catches up exactly as without the policy.
/// Two sources of the same client: after the initialization (not subject to it) nothing is sent, the updater waits.
#[test]
fn distinct_clients_must_serve_the_same_bytes() {
    let r = rec("mainnet");
    let clock = Clock::new(r.entries.len());
    let payer = key(0x31);
    let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
    let sources: Vec<Box<dyn Source>> = vec![
        Box::new(r.reporting("nimbus", &clock, "nimbus", 0)),
        Box::new(r.reporting("lodestar", &clock, "lodestar", 0)),
    ];
    let mut u = r.updater("div", payer, sources, None, diverse(2));
    settle(&mut u, &mut l, 200);
    assert_eq!(store(&u, &l).finalized.slot, r.last_finalized().0);
    assert_eq!(store(&u, &l).finalized.hash_tree_root(), r.last_finalized().1);
    assert_eq!(refused_total(&l), 0);
    let m = &u.metrics.sources;
    assert_eq!(
        (m["nimbus"].client.as_str(), m["lodestar"].client.as_str()),
        (version("nimbus").as_str(), version("lodestar").as_str())
    );
    assert_eq!(m["nimbus"].disagreements + m["lodestar"].disagreements, 0);
    assert_eq!(u.health().0, "ok", "{:?}", u.health().1);
    let text = u.render_metrics();
    assert!(text.contains(&format!(
        "ferrata_updater_source_client_info{{daemon=\"div\",chain=\"mainnet\",source=\"lodestar\",client=\"lodestar\",version=\"{}\"}} 1",
        version("lodestar")
    )), "{text}");
    assert_eq!(u.health_json()["clients"]["nimbus"], version("nimbus"));
    println!("two clients: {} applied, finalized {}", applied(&u, &l), store(&u, &l).finalized.slot);

    // The same client twice is one client.
    let payer = key(0x32);
    let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
    let sources: Vec<Box<dyn Source>> = vec![
        Box::new(r.reporting("lodestar-a", &clock, "lodestar", 0)),
        Box::new(r.reporting("lodestar-b", &clock, "lodestar", 0)),
    ];
    let mut u = r.updater("div", payer, sources, None, diverse(2));
    let mut waiting = Vec::new();
    for _ in 0..30 {
        if let Plan::Waiting(w) = u.tick(&mut l).unwrap() {
            waiting.push(w);
        }
    }
    assert!(waiting.len() >= 25, "{} rounds waiting", waiting.len());
    assert_eq!(applied(&u, &l), 1, "only the initialisation");
    assert!(waiting[0].contains("1 of 2 clients agree byte for byte (lodestar)"), "{}", waiting[0]);
    assert_eq!(refused_total(&l), 0);
    println!("one client twice: {} rounds waiting: {}", waiting.len(), waiting[0]);
}

/// A source that serves valid updates no other client serves (the stand-in for a fork that one implementation, or a
/// captured committee's accomplice, serves: the program's checks pass it). Without the policy the updater follows
/// it; with `min_distinct_clients = 2` it holds at what both clients serve byte for byte, then follows once the other
/// client serves the same updates.
#[test]
fn an_update_one_client_serves_is_held_back() {
    let r = rec("sepolia");
    let n = r.entries.len();
    let head = |upto: usize| r.entries[..upto].iter().map(|e| e.finalized_slot).max().unwrap();
    let (honest_head, ahead_head) = (head(n - 4), head(n - 1));
    assert!(ahead_head > honest_head, "the record moves on within those entries");
    for k in [0, 2] {
        let clock = Clock::new(n - 4);
        let payer = key(0x33 + k as u8);
        let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
        let sources: Vec<Box<dyn Source>> = vec![
            Box::new(r.reporting("nimbus", &clock, "nimbus", 0)),
            Box::new(r.reporting("lodestar-ahead", &clock, "lodestar", 3)),
        ];
        let mut u = r.updater("fork", payer, sources, None, diverse(k));
        let plans = until_waiting(&mut u, &mut l, 400);
        let held = store(&u, &l).finalized.slot;
        assert_eq!(refused_total(&l), 0);
        if k == 0 {
            assert!(held > honest_head, "without the policy, the updater follows the one source beyond the other");
            println!("k = 0: followed the source ahead to {held} (the other serves up to {honest_head})");
            continue;
        }
        assert!(held <= honest_head, "held {held}, the other client serves up to {honest_head}");
        let Some(Plan::Waiting(why)) = plans.last() else { panic!("{:?}", plans.last()) };
        assert!(why.contains("1 of 2 clients agree byte for byte (lodestar)"), "{why}");
        println!("k = 2: held at {held} (the other client serves up to {honest_head}): {why}");
        // The other client catches up: the same updates, byte for byte.
        clock.set(n);
        settle(&mut u, &mut l, 400);
        assert_eq!(store(&u, &l).finalized.slot, r.last_finalized().0);
        assert_eq!(refused_total(&l), 0);
        println!("k = 2: both serve the record's end: {}", store(&u, &l).finalized.slot);
    }
}

/// A source that lies (forged signatures) while claiming a second client: the updater sends nothing after the
/// initialization (one honest client is not two), the liar is demoted by the program's checks, both sources' bytes
/// for the same updates are logged as a disagreement and named in `/health`. A third, honest client restores
/// progress.
#[test]
fn a_lying_client_is_logged_and_does_not_count() {
    let r = rec("mainnet");
    let clock = Clock::new(r.entries.len());
    let liar = |r: &Rec| -> Box<dyn Source> {
        Box::new(Faulty::new(r.reporting("lodestar", &clock, "lodestar", 0), Fault::BadSignature, r.config.clone()))
    };
    let payer = key(0x36);
    let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
    let sources: Vec<Box<dyn Source>> = vec![Box::new(r.reporting("nimbus", &clock, "nimbus", 0)), liar(&r)];
    let mut u = r.updater("liar", payer, sources, None, diverse(2));
    let mut health = None;
    for round in 0..30 {
        let p = u.tick(&mut l).unwrap();
        if round == 3 {
            health = Some(u.health());
        }
        assert!(!matches!(p, Plan::UpToDate), "never up to date: the liar's updates are refused, one client remains");
    }
    assert_eq!(applied(&u, &l), 1, "only the initialisation");
    assert_eq!(refused_total(&l), 0);
    let m = &u.metrics.sources;
    assert!(m["lodestar(BadSignature)"].refused >= 1, "demoted by the program's checks");
    assert!(m["nimbus"].disagreements >= 1 && m["lodestar(BadSignature)"].disagreements >= 1, "{m:?}");
    let (status, why) = health.unwrap();
    assert_eq!(status, "degraded");
    assert!(why.iter().any(|w| w.contains("lodestar(BadSignature) demoted")), "{why:?}");
    assert!(why.iter().any(|w| w.contains("nimbus serves other bytes than lodestar(BadSignature)")), "{why:?}");
    println!(
        "liar: {why:?}; disagreements nimbus {} liar {}",
        m["nimbus"].disagreements, m["lodestar(BadSignature)"].disagreements
    );

    // A third client, honest: nimbus and teku agree byte for byte; the liar is still refused.
    let payer = key(0x37);
    let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
    let mut teku = r.reporting("teku", &clock, "nimbus", 0);
    teku.client = Some("teku/stand-in (no recorded answer)".into());
    let sources: Vec<Box<dyn Source>> =
        vec![Box::new(r.reporting("nimbus", &clock, "nimbus", 0)), liar(&r), Box::new(teku)];
    let mut u = r.updater("liar3", payer, sources, None, diverse(2));
    settle(&mut u, &mut l, 200);
    assert_eq!(store(&u, &l).finalized.slot, r.last_finalized().0);
    assert_eq!(refused_total(&l), 0);
    println!("liar among three: caught up to {}", store(&u, &l).finalized.slot);
}

/// Over HTTP: beacon nodes on loopback answering `/eth/v1/node/version` with the recorded answers of Nimbus, Lodestar
/// and Lighthouse. Bytes: a Nimbus and a Lodestar node serving the recorded chain. Finalized header: a Lodestar
/// light-client node and a Lighthouse `headers` node; then the `headers` node reporting Lodestar too (one client).
#[test]
fn clients_reported_over_http() {
    use ferrata_updater::{
        config::ClientMatch,
        source::{BeaconApi, HeaderWitness},
    };
    let r = rec("mainnet");
    let clock = Clock::new(r.entries.len());
    let node = |client: &str| mock_beacon_reporting(r.source("node", &clock, 0), None, Some(answer(client)));
    let payer = key(0x38);
    let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
    let sources: Vec<Box<dyn Source>> = vec![
        Box::new(BeaconApi { name: "nimbus".into(), base: node("nimbus"), timeout_secs: 5 }),
        Box::new(BeaconApi { name: "lodestar".into(), base: node("lodestar"), timeout_secs: 5 }),
    ];
    let mut u = r.updater("http-bytes", payer, sources, None, diverse(2));
    settle(&mut u, &mut l, 100);
    assert_eq!(store(&u, &l).finalized.slot, r.last_finalized().0);
    assert_eq!(u.metrics.sources["lodestar"].client, version("lodestar"));
    println!("http, bytes: caught up to {}", store(&u, &l).finalized.slot);

    let roots = finalized_roots(&r);
    for (headers_client, ok) in [("lighthouse", true), ("lodestar", false)] {
        let payer = key(if ok { 0x39 } else { 0x3a });
        let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
        let witness = HeaderWitness {
            name: "headers".into(),
            base: mock_headers_reporting(roots.clone(), false, Some(answer(headers_client))),
            timeout_secs: 5,
        };
        let sources: Vec<Box<dyn Source>> = vec![
            Box::new(BeaconApi { name: "lodestar".into(), base: node("lodestar"), timeout_secs: 5 }),
            Box::new(witness),
        ];
        let pol = Policy { client_match: ClientMatch::FinalizedHeader, ..diverse(2) };
        let mut u = r.updater("http-header", payer, sources, None, pol);
        if ok {
            settle(&mut u, &mut l, 100);
            assert_eq!(store(&u, &l).finalized.slot, r.last_finalized().0);
            assert_eq!(u.metrics.sources["headers"].client, version("lighthouse"));
            println!("http, finalized header, lodestar + lighthouse: caught up to {}", store(&u, &l).finalized.slot);
        } else {
            let plans = until_waiting(&mut u, &mut l, 30);
            let Some(Plan::Waiting(why)) = plans.last() else { panic!("{:?}", plans.last()) };
            assert!(why.contains("1 of 2 clients agree its finalized header (lodestar)"), "{why}");
            assert_eq!(applied(&u, &l), 1, "only the initialisation");
            println!("http, finalized header, lodestar twice: {why}");
        }
        assert_eq!(refused_total(&l), 0);
    }
}
