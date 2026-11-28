//! `/health` reflects what lands on the chain (review-code-lc UP-M1: it said `ok`, HTTP 200, while every transaction
//! was rejected, failed at transport or was refused, and could never say `down` with the shipped example). Here:
//! degraded from the first send that does not land, `down` after [`DOWN_AFTER_FAILED_SENDS`] in a row, `ok` again once
//! one lands; `down` when no light-client source answers; degraded when the instance stops moving while the sources
//! serve updates past it; a source that served the checkpoint's bootstrap is not reported unavailable. The in-process
//! chain, the lab's recorded mainnet data.
//!
//! And `down` whenever this updater can move nothing (the review of these fixes, M1: a chain that answers nothing, and
//! an instance past its weak-subjectivity period, stayed `degraded`, HTTP 200, for 200 rounds of 200): rounds that end
//! in an error, the instance past its period, and no progress for twice the stall alarm's rounds; the daemon's
//! `/health` then answers 503.
#![cfg(feature = "harness")]

use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Result;
use ferrata_core::Root;
use ferrata_lez_wire::Config;
use ferrata_updater::{
    chain::{preset, root},
    config::{Fees, File, Policy},
    daemon::Daemon,
    ledger::{HarnessLedger, Ledger, Tx, TxOutcome},
    serve::{serve, Snapshot},
    source::{lab_record, Clock, Fault, Faulty, Recorded, Source},
    updater::{Instance, Plan, Updater, DOWN_AFTER_FAILED_ROUNDS, DOWN_AFTER_FAILED_SENDS},
};
use lee::{AccountId, Actor};
use ferrata_updater::lez::PayerKey;

fn key(b: u8) -> PayerKey {
    ferrata_updater::lez::payer_key([b; 32])
}

fn no_sleep(_: u64) {}

struct Rec {
    config: Config,
    dir: PathBuf,
    checkpoint: Root,
}

fn mainnet() -> Rec {
    let config = preset("mainnet", 2).unwrap();
    let dir = lab_record("mainnet");
    let checkpoint = root(&fs::read_to_string(dir.join("checkpoint")).unwrap()).unwrap();
    Rec { config, dir, checkpoint }
}

impl Rec {
    fn source(&self, label: &str, clock: &Clock) -> Recorded {
        Recorded::open(label, &self.dir, &self.config, clock.clone()).unwrap()
    }
    fn updater(&self, label: &str, payer: PayerKey, sources: Vec<Box<dyn Source>>, policy: Policy) -> Updater {
        let inst = Instance::new("mainnet", HarnessLedger::program(), self.config.clone(), self.checkpoint);
        let mut u = Updater::new(label, inst, payer, Fees::default(), policy, sources, None).unwrap();
        u.sleep = no_sleep;
        u
    }
}

fn policy() -> Policy {
    Policy { defer_window: 1, retry_base_ms: 0, ..Policy::default() }
}

fn settle(u: &mut Updater, l: &mut dyn Ledger) {
    for _ in 0..60 {
        if matches!(u.tick(l).unwrap(), Plan::UpToDate) {
            return;
        }
    }
    panic!("not up to date");
}

/// P3a: an updater whose payer has no funds (every transaction rejected, nothing charged): degraded after the first,
/// `down` after five in a row, the reason named.
#[test]
fn rejected_sends_degrade_then_take_it_down() {
    let r = mainnet();
    let clock = Clock::new(3);
    let funded = key(0x51);
    let mut l = HarnessLedger::new(std::slice::from_ref(&funded));
    let mut u0 = r.updater("funded", funded, vec![Box::new(r.source("a", &clock))], policy());
    settle(&mut u0, &mut l);
    clock.set(4);
    let mut u = r.updater("no-funds", key(0x52), vec![Box::new(r.source("a", &clock))], policy());
    let mut seen = Vec::new();
    for _ in 0..DOWN_AFTER_FAILED_SENDS {
        u.tick(&mut l).unwrap();
        seen.push(u.health().0);
    }
    let (status, why) = u.health();
    println!("no funds: rejected {}; health by round {seen:?}; {why:?}", u.metrics.count("rejected"));
    assert_eq!(u.metrics.count("rejected"), DOWN_AFTER_FAILED_SENDS as u64);
    assert_eq!(seen[0], "degraded");
    assert_eq!(status, "down");
    assert!(why.iter().any(|w| w.contains("did not land") && w.contains("rejected")), "{why:?}");
    assert_eq!(u.health_json()["failing_sends"], DOWN_AFTER_FAILED_SENDS);
}

/// P3b: every send fails at transport: degraded after the first round, `down` after five; once the connection is back
/// the next transaction lands and the updater is `ok` again.
#[test]
fn transport_failures_degrade_then_take_it_down_and_recover() {
    let r = mainnet();
    let clock = Clock::new(3);
    let payer = key(0x53);
    let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
    let mut u = r.updater("transport", payer, vec![Box::new(r.source("a", &clock))], policy());
    settle(&mut u, &mut l);
    assert_eq!(u.health().0, "ok");
    clock.set(4);
    l.fail_next = usize::MAX;
    let mut seen = Vec::new();
    for _ in 0..DOWN_AFTER_FAILED_SENDS {
        assert!(u.tick(&mut l).is_err());
        seen.push(u.health().0);
    }
    let (status, why) = u.health();
    println!("transport down: {} errors; health by round {seen:?}; {why:?}", u.metrics.transport_errors);
    assert_eq!(seen[0], "degraded");
    assert_eq!(status, "down");
    assert!(why.iter().any(|w| w.contains("did not land") && w.contains("transport")), "{why:?}");
    l.fail_next = 0;
    settle(&mut u, &mut l);
    println!("connection back: health {:?}", u.health());
    assert_eq!(u.health().0, "ok");
}

/// P3c: the only light-client source is down: `down` (HTTP 503), not `degraded`.
#[test]
fn no_light_client_source_answering_is_down() {
    let r = mainnet();
    let clock = Clock::new(3);
    let payer = key(0x54);
    let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
    let down: Box<dyn Source> = Box::new(Faulty::new(r.source("lc", &clock), Fault::Down, r.config.clone()));
    let mut u = r.updater("down", payer, vec![down], policy());
    let _ = u.tick(&mut l);
    println!("the only source down: {:?}", u.health());
    assert_eq!(u.health().0, "down");
}

/// The instance does not move while the sources serve updates past it (one source, `min_agreeing = 2`: every update
/// waits): `ok` until `6 × defer_window + 10` rounds without progress, then degraded, saying so; `down` after twice as
/// many (this updater moves nothing: before the review of these fixes it stayed degraded, HTTP 200, for good). Before
/// activation the source that served the bootstrap is not reported unavailable.
#[test]
fn no_progress_while_the_sources_move_on_is_degraded_then_down() {
    let r = mainnet();
    let clock = Clock::new(r.source("x", &Clock::new(0)).entries.len());
    let payer = key(0x55);
    let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
    let mut u =
        r.updater("stall", payer, vec![Box::new(r.source("a", &clock))], Policy { min_agreeing: 2, ..policy() });
    // Init and the committee's adoption are not subject to min_agreeing.
    u.tick(&mut l).unwrap();
    println!("after Init: {:?}", u.health());
    assert!(!u.health().1.iter().any(|w| w.contains("unavailable")), "{:?}", u.health());
    let (stall, down) = (u.stall_rounds(), u.stall_down_rounds());
    assert_eq!((stall, down), (6 * u.policy.defer_window + 10, 2 * (6 * u.policy.defer_window + 10)));
    let (mut first_degraded, mut first_down) = (None, None);
    for round in 1..=down + 5 {
        let p = u.tick(&mut l).unwrap();
        if first_degraded.is_none() && u.health().0 != "ok" {
            first_degraded = Some((round, format!("{p:?}"), u.health().1));
        }
        if first_down.is_none() && u.health().0 == "down" {
            first_down = Some((round, u.health().1));
        }
    }
    println!(
        "min_agreeing 2 with one source: first not ok {first_degraded:?}; first down {first_down:?}; json {}",
        u.health_json()
    );
    let (round, _, why) = first_degraded.expect("degraded at last");
    assert!(round >= stall, "not before {stall} rounds without progress: {round}");
    assert!(why.iter().any(|w| w.contains("no progress")), "{why:?}");
    let (round_down, why) = first_down.expect("down at last");
    assert!(round_down >= down && round_down > round, "not before {down} rounds without progress: {round_down}");
    assert!(why.iter().any(|w| w.contains("no progress")), "{why:?}");
    assert_eq!(u.health().0, "down");
}

/// The in-process chain, unreachable while `down`: every read and every send fails at transport (a sequencer that is
/// gone).
struct Unreachable {
    inner: HarnessLedger,
    down: bool,
}

impl Ledger for Unreachable {
    fn actor_state(&self, actor: Actor) -> Result<Vec<u8>> {
        if self.down {
            anyhow::bail!("connection refused");
        }
        self.inner.actor_state(actor)
    }
    fn submit(&mut self, payer: &PayerKey, tx: &Tx, gas_limit: u64, max_fee: u128) -> Result<TxOutcome> {
        if self.down {
            anyhow::bail!("connection refused");
        }
        self.inner.submit(payer, tx, gas_limit, max_fee)
    }
    fn now_ms(&self) -> Option<u64> {
        self.inner.now_ms()
    }
}

/// Probe C (1) of the review of these fixes: the chain unreachable for 200 rounds. Every round fails at its first read,
/// before anything is sent, so no send fails; `/health` was degraded (HTTP 200) in 200 rounds of 200. Now: degraded in
/// rounds 1-4, `down` from round 5 on, the error named; `ok` again once the chain answers and the instance moves (the
/// sources serve one more update meanwhile).
#[test]
fn an_unreachable_chain_takes_it_down_and_back() {
    let r = mainnet();
    let clock = Clock::new(3);
    let payer = key(0x56);
    let mut l = Unreachable { inner: HarnessLedger::new(std::slice::from_ref(&payer)), down: false };
    let mut u = r.updater("unreachable", payer, vec![Box::new(r.source("a", &clock))], policy());
    settle(&mut u, &mut l);
    assert_eq!(u.health().0, "ok");
    l.down = true;
    clock.set(4);
    let mut seen = Vec::new();
    for _ in 0..200 {
        assert!(u.tick(&mut l).is_err());
        seen.push(u.health().0);
    }
    let (status, why) = u.health();
    let count = |s: &str| seen.iter().filter(|x| **x == s).count();
    println!(
        "chain unreachable for 200 rounds: degraded in {} rounds, down in {} (the first in round {:?}); failing sends \
         {}; {status} {why:?}",
        count("degraded"),
        count("down"),
        seen.iter().position(|s| *s == "down").map(|i| i + 1),
        u.metrics.failing_sends
    );
    let rounds = DOWN_AFTER_FAILED_ROUNDS as usize;
    assert!(seen[..rounds - 1].iter().all(|s| *s == "degraded"), "{seen:?}");
    assert!(seen[rounds - 1..].iter().all(|s| *s == "down"), "{seen:?}");
    assert_eq!(u.metrics.failing_sends, 0, "nothing was sent, so no send failed");
    assert!(
        why.iter().any(|w| w.contains("200 round(s) ended in an error") && w.contains("connection refused")),
        "{why:?}"
    );
    assert_eq!(u.health_json()["failed_rounds"], 200);
    l.down = false;
    let mut back = Vec::new();
    for _ in 0..60 {
        let p = u.tick(&mut l).unwrap();
        back.push(u.health().0);
        if matches!(p, Plan::UpToDate) {
            break;
        }
    }
    println!("the chain back: health by round {back:?}; {:?}", u.health());
    assert_eq!(u.health().0, "ok");
    assert_eq!(u.metrics.failed_rounds, 0);
}

/// Probe C (2): the instance past its weak-subjectivity period by the chain's clock (`HarnessLedger::now`) for 200
/// rounds while the sources serve updates past it: the program refuses every `Open` from now on, and `/health` was
/// degraded (HTTP 200) in 200 rounds of 200. Now `down` in every round, naming what to do; nothing sent.
#[test]
fn an_instance_past_its_weak_subjectivity_period_is_down() {
    let r = mainnet();
    let clock = Clock::new(3);
    let payer = key(0x57);
    let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
    let mut u = r.updater("past-ws", payer, vec![Box::new(r.source("a", &clock))], policy());
    settle(&mut u, &mut l);
    let fin = u.inst.store(&l).unwrap().expect("active").finalized.slot;
    l.now = Some(r.config.ws_end_ms(fin).unwrap() + 1_000);
    let n0 = l.log.len();
    let last = r.source("x", &Clock::new(0)).entries.len();
    let mut seen = Vec::new();
    for i in 0..200 {
        clock.set((4 + i).min(last));
        assert!(matches!(u.tick(&mut l).unwrap(), Plan::Stuck(_)));
        seen.push(u.health().0);
    }
    let (status, why) = u.health();
    println!(
        "past its weak-subjectivity period, 200 rounds: down in {} of 200, {} sent; {status} {why:?}",
        seen.iter().filter(|s| **s == "down").count(),
        l.log.len() - n0
    );
    assert!(seen.iter().all(|s| *s == "down"), "{seen:?}");
    assert_eq!(l.log.len(), n0, "nothing sent");
    assert!(why.iter().any(|w| w.contains("initialise a new instance")), "{why:?}");
    let metrics = u.render_metrics();
    assert!(metrics.contains("ferrata_updater_past_weak_subjectivity{daemon=\"past-ws\",chain=\"mainnet\"} 1"));
    assert!(metrics.contains("ferrata_updater_health{daemon=\"past-ws\",chain=\"mainnet\"} 2"));
}

/// Stands in for a chain backend that answers nothing (the daemon's opener; `daemon.ledger = "harness"`).
fn open_unreachable(_: &File, _: &Path, payer: &PayerKey) -> Result<(Box<dyn Ledger>, AccountId)> {
    let l = Unreachable { inner: HarnessLedger::new(std::slice::from_ref(payer)), down: true };
    Ok((Box::new(l), HarnessLedger::program()))
}

/// A configuration in a directory of its own: the dev key, the lab's recorded mainnet data.
fn daemon_config(name: &str) -> PathBuf {
    let base = std::env::var_os("CARGO_TARGET_TMPDIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!("health-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let record = lab_record("mainnet");
    let checkpoint = fs::read_to_string(record.join("checkpoint")).unwrap();
    let key = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("dev/payer.key");
    let text = format!(
        "[daemon]\nname = \"{name}\"\nstate_dir = \"state\"\nledger = \"harness\"\n[keys]\npayer_key_file = {key:?}\n\
         [[chain]]\nname = \"mainnet\"\npreset = \"mainnet\"\ncheckpoint = \"{}\"\n  [[chain.source]]\n  name = \"record\"\n  \
         dir = {record:?}\n",
        checkpoint.trim()
    );
    let path = dir.join("updater.toml");
    fs::write(&path, text).unwrap();
    path
}

fn get_status(addr: SocketAddr, path: &str) -> String {
    let mut c = TcpStream::connect(addr).unwrap();
    c.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    c.write_all(format!("GET {path} HTTP/1.1\r\n\r\n").as_bytes()).unwrap();
    let mut out = String::new();
    let _ = c.read_to_string(&mut out);
    out.lines().next().unwrap_or("").to_string()
}

/// The daemon whose chain answers nothing, as the binary runs it: each round's health set on its listener (as `run`
/// does). `/health` answers 200 (degraded) in rounds 1-4 and 503 from round 5 on.
#[test]
fn a_daemon_cut_off_from_its_chain_answers_503() {
    let mut d = Daemon::open_with(&daemon_config("cut-off"), false, open_unreachable).unwrap();
    let snapshot = Snapshot::default();
    let addr = serve("127.0.0.1:0", snapshot.clone()).unwrap();
    let mut seen = Vec::new();
    for _ in 0..DOWN_AFTER_FAILED_ROUNDS + 1 {
        let notes = d.round();
        assert!(notes.iter().any(|n| n.contains("connection refused")), "{notes:?}");
        let (health, healthy) = d.health();
        snapshot.set(d.metrics(), serde_json::to_string(&health).unwrap(), healthy);
        seen.push(get_status(addr, "/health"));
    }
    println!("a daemon cut off from its chain: /health by round {seen:?}");
    let rounds = DOWN_AFTER_FAILED_ROUNDS as usize;
    assert!(seen[..rounds - 1].iter().all(|s| s == "HTTP/1.1 200 OK"), "{seen:?}");
    assert!(seen[rounds - 1..].iter().all(|s| s == "HTTP/1.1 503 Service Unavailable"), "{seen:?}");
}

/// 9c L-U2: an updater restarted without its committee cache, whose only source does not serve the store's period's
/// update (here one that serves the checkpoint's bootstrap and nothing else), can judge no update: each round ends in
/// an error, so `/health` is degraded in rounds 1-4 and `down` from round 5, naming why (it used to end `stuck`, before
/// the sources' head was read, and stay degraded for good). Given a source that serves the period's update, its next
/// round ends without an error.
#[test]
fn committees_no_source_serves_take_it_down() {
    let r = mainnet();
    let clock = Clock::new(12);
    let payer = key(0x58);
    let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
    let mut u = r.updater("cached", payer, vec![Box::new(r.source("a", &clock))], policy());
    settle(&mut u, &mut l);
    let period = u.metrics.period;
    let mut blind = r.source("bootstrap-only", &clock);
    blind.lag = 12;
    let mut fresh = r.updater("no-cache", key(0x59), vec![Box::new(blind)], policy());
    let mut seen = Vec::new();
    for _ in 0..10 {
        assert!(fresh.tick(&mut l).is_err());
        seen.push(fresh.health().0);
    }
    let (status, why) = fresh.health();
    println!(
        "the store in period {period}, no source serving its committees: health by round {seen:?}; {status} {why:?}"
    );
    let rounds = DOWN_AFTER_FAILED_ROUNDS as usize;
    assert!(seen[..rounds - 1].iter().all(|s| *s == "degraded"), "{seen:?}");
    assert!(seen[rounds - 1..].iter().all(|s| *s == "down"), "{seen:?}");
    assert!(why.iter().any(|w| w.contains("committees are not served by any source")), "{why:?}");
    fresh.sources.push(Box::new(r.source("a", &clock)));
    let p = fresh.tick(&mut l).unwrap();
    println!("a source serving the period's update added: {p:?}; {:?}", fresh.health());
    assert!(matches!(p, Plan::UpToDate), "{p:?}");
    assert_eq!(fresh.metrics.failed_rounds, 0);
    assert_ne!(fresh.health().0, "down");
}
