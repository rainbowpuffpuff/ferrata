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
    // GATE_SALT: other names for the honest updaters, so another draw of their deferral waits
    // (a wait is a hash of the updater's name and key, the job and the step).
    let salt = std::env::var("GATE_SALT").unwrap_or_default();
    // Per chain: [H1, H2, A].
    let mut ups: Vec<Vec<Updater>> = recs
        .iter()
        .zip(&clocks)
        .map(|(r, c)| {
            vec![
                r.updater(
                    &format!("honest-1{salt}"),
                    h1.clone(),
                    vec![r.honest("a", c), r.honest("b", c), r.faulty("liar", c, Fault::BadSignature)],
                    None,
                    policy(8),
                ),
                r.updater(
                    &format!("honest-2{salt}"),
                    h2.clone(),
                    vec![r.honest("b", c), r.honest("c", c), Box::new(r.source("stale", c, 3))],
                    None,
                    policy(8),
                ),
                r.updater("adversary", adv.clone(), vec![r.honest("a", c)], None, policy(1)),
            ]
        })
        .collect();
    // GATE_SEED: another order of every block's transactions (and of the adversary's races).
    let seed = std::env::var("GATE_SEED").ok().and_then(|v| v.parse::<u64>().ok());
    let mut rng = Rng(seed.unwrap_or(0x9e37_79b9_7f4a_7c15));
    // GATE_ADVERSARY=0: the two honest updaters alone (a baseline for what deferral costs).
    let adversary = std::env::var("GATE_ADVERSARY").ok().as_deref() != Some("0");
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
                        if d < 2 || (adversary && rng.next().is_multiple_of(3)) {
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
    let mut lines = vec![format!(
        "gate\trounds\t{rounds}\tseed\t{}\tadversary\t{adversary}\tsalt\t{salt}",
        seed.map_or("default".into(), |s| s.to_string())
    )];
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
    let name = match (seed, adversary, salt.as_str()) {
        (_, _, s) if !s.is_empty() => format!("gate-salt-{s}.txt"),
        (_, false, _) => "gate-no-adversary.txt".to_string(),
        (None, true, _) => "gate.txt".to_string(),
        (Some(s), true, _) => format!("gate-seed-{s}.txt"),
    };
    fs::write(out.join(name), &text).unwrap();
    assert!(ratio <= 0.10, "honest refused gas {ratio:.4} of applied");
    assert!((0.35..=0.65).contains(&share1) && (0.35..=0.65).contains(&share2), "shares {share1:.3} {share2:.3}");
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
