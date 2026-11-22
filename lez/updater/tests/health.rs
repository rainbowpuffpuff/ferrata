//! `/health` reflects what lands on the chain (review-code-lc UP-M1: it said `ok`, HTTP 200, while every transaction
//! was rejected, failed at transport or was refused, and could never say `down` with the shipped example). Here:
//! degraded from the first send that does not land, `down` after [`DOWN_AFTER_FAILED_SENDS`] in a row, `ok` again once
//! one lands; `down` when no light-client source answers; degraded when the instance stops moving while the sources
//! serve updates past it; a source that served the checkpoint's bootstrap is not reported unavailable. The in-process
//! chain, the lab's recorded mainnet data.
#![cfg(feature = "harness")]

use std::{fs, path::PathBuf};

use ferrata_core::Root;
use ferrata_lez_wire::Config;
use ferrata_updater::{
    chain::{preset, root},
    config::{Fees, Policy},
    ledger::HarnessLedger,
    source::{lab_record, Clock, Fault, Faulty, Recorded, Source},
    updater::{Instance, Plan, Updater, DOWN_AFTER_FAILED_SENDS},
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
    fn updater(&self, label: &str, payer: PrivateKey, sources: Vec<Box<dyn Source>>, policy: Policy) -> Updater {
        let inst = Instance::new("mainnet", HarnessLedger::program(), self.config.clone(), self.checkpoint);
        let mut u = Updater::new(label, inst, payer, Fees::default(), policy, sources, None).unwrap();
        u.sleep = no_sleep;
        u
    }
}

fn policy() -> Policy {
    Policy { defer_window: 1, retry_base_ms: 0, ..Policy::default() }
}

fn settle(u: &mut Updater, l: &mut HarnessLedger) {
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
/// waits): `ok` until `6 × defer_window + 10` rounds without progress, then degraded, saying so. Before activation the
/// source that served the bootstrap is not reported unavailable.
#[test]
fn no_progress_while_the_sources_move_on_is_degraded() {
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
    let stall = 6 * u.policy.defer_window + 10;
    let mut first_degraded = None;
    for round in 1..=stall + 5 {
        let p = u.tick(&mut l).unwrap();
        if first_degraded.is_none() && u.health().0 != "ok" {
            first_degraded = Some((round, format!("{p:?}"), u.health().1));
        }
    }
    println!("min_agreeing 2 with one source: first not ok {first_degraded:?}; json {}", u.health_json());
    let (round, _, why) = first_degraded.expect("degraded at last");
    assert!(round >= stall, "not before {stall} rounds without progress: {round}");
    assert!(why.iter().any(|w| w.contains("no progress")), "{why:?}");
    assert_eq!(u.health().0, "degraded");
}
