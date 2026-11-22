//! Jobs other parties open and leave (review-code-lc UP-M3). A party opens the job of each new update first and never
//! continues it, while the newest update changes every round (as a source serving the newest finality update each
//! slot): the updater used to pay a `Miller` each round for a job it left the next (P10b: 7 rounds, 7 `Miller`s, the
//! store unmoved). Now it keeps every job it sends a step for and finishes it before choosing another update: the
//! instance advances and every step it pays belongs to an update that is applied (at most one job is left in flight).
#![cfg(feature = "harness")]

use std::fs;

use ferrata_updater::{
    chain::{decode_finality_update, preset, root},
    config::{Fees, Policy},
    ledger::{HarnessLedger, Ledger},
    source::{lab_record, load_entries, Clock, Recorded},
    updater::{Instance, Plan, Updater},
};
use lee::PrivateKey;

fn key(b: u8) -> PrivateKey {
    PrivateKey::try_new([b; 32]).unwrap()
}

fn no_sleep(_: u64) {}

#[test]
fn jobs_another_party_opens_and_leaves_are_finished_not_paid_for_again() {
    let config = preset("mainnet", 2).unwrap();
    let dir = lab_record("mainnet");
    let checkpoint = root(&fs::read_to_string(dir.join("checkpoint")).unwrap()).unwrap();
    let entries = load_entries(&dir.join("updates"), &config).unwrap();
    let (updater_key, opener) = (key(0x61), key(0x62));
    let clock = Clock::new(1);
    let mut l = HarnessLedger::new(&[updater_key.clone(), opener.clone()]);
    let inst = Instance::new("mainnet", HarnessLedger::program(), config.clone(), checkpoint);
    let policy = Policy { defer_window: 1, retry_base_ms: 0, ..Policy::default() };
    let source = Recorded::open("a", &dir, &config, clock.clone()).unwrap();
    let mut u =
        Updater::new("updater", inst, updater_key, Fees::default(), policy, vec![Box::new(source)], None).unwrap();
    u.sleep = no_sleep;
    for _ in 0..40 {
        if matches!(u.tick(&mut l).unwrap(), Plan::UpToDate) {
            break;
        }
    }
    let applied = |u: &Updater, l: &HarnessLedger| u.inst.state(l).unwrap().map_or(0, |s| s.applied);
    let slot = |u: &Updater, l: &HarnessLedger| u.inst.store(l).unwrap().unwrap().finalized.slot;
    let (start, applied0, n0) = (slot(&u, &l), applied(&u, &l), l.log.len());

    // Each round a new finality update appears; the other party opens its job first and never continues it.
    let mut lines = Vec::new();
    let mut t = 2;
    let mut rounds = 0;
    while rounds < 10 && t <= entries.len() {
        if entries[t - 1].period_update {
            t += 1;
            continue;
        }
        clock.set(t);
        let (update, _) = decode_finality_update(&entries[t - 1].bytes, &config).unwrap();
        let opened = match u.open_unchecked(&l, &update) {
            Ok(tx) => l.submit(&opener, &tx, 4_800_000, 1_000_000_000).unwrap().applied,
            Err(_) => false,
        };
        let plan = match u.tick(&mut l).unwrap() {
            Plan::Send(a) => format!("{} of job 0x{}", a.step.name(), &hex::encode(a.job_id)[..8]),
            p => format!("{p:?}"),
        };
        lines.push(format!("t {t}: the other party's Open applied {opened}; the updater: {plan}"));
        t += 1;
        rounds += 1;
    }
    let mine: Vec<_> = l.log[n0..].iter().filter(|(p, ..)| *p == u.payer_id).collect();
    let millers = mine.iter().filter(|(_, label, o)| label.ends_with("Miller") && o.applied).count();
    let finishes = mine.iter().filter(|(_, label, o)| label.ends_with("Finish") && o.applied).count();
    let paid: u128 = mine.iter().map(|(_, _, o)| o.fee).sum();
    let theirs: u128 = l.log[n0..].iter().filter(|(p, ..)| *p != u.payer_id).map(|(_, _, o)| o.fee).sum();
    let (end, updates) = (slot(&u, &l), applied(&u, &l) - applied0);
    println!(
        "{}\nstore {start} -> {end}; {updates} updates applied; the updater's Millers {millers}, Finishes {finishes}, \
         paid {paid}; the other party paid {theirs}; kept jobs at the end {}",
        lines.join("\n"),
        u.carried_jobs()
    );
    assert!(end > start, "the instance advances");
    assert!(updates >= 2, "{updates}");
    assert!(millers <= updates as usize + 1, "a Miller paid for a job left behind: {millers} for {updates} updates");
    assert_eq!(
        mine.iter().filter(|(_, _, o)| !o.applied && !o.rejected).count(),
        0,
        "nothing of the updater's refused"
    );
}
