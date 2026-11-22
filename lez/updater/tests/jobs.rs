//! Jobs other parties open and leave (review-code-lc UP-M3). A party opens the job of each new update first and never
//! continues it, while the newest update changes every round (as a source serving the newest finality update each
//! slot): the updater used to pay a `Miller` each round for a job it left the next (P10b: 7 rounds, 7 `Miller`s, the
//! store unmoved). Now it keeps every job it sends a step for and finishes it before choosing another update: the
//! instance advances and every step it pays belongs to an update that is applied (at most one job is left in flight).
//! And a job it keeps is not left because its next step is held for a while (the review of these fixes, L2).
#![cfg(feature = "harness")]

use std::{fs, path::PathBuf};

use ferrata_updater::{
    chain::{decode_finality_update, preset, root},
    config::{Fees, Policy},
    ledger::{HarnessLedger, Ledger, Tx, TxOutcome},
    source::{lab_record, load_entries, Clock, Recorded},
    updater::{Instance, Plan, Updater, REFUSAL_BACKOFF},
};
use lee::{Actor, PrivateKey};

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
    // The figures EVIDENCE row 200 gives (9c L-U7: they were printed, and only weaker bounds asserted).
    assert!(end > start, "the instance advances");
    assert_eq!(updates, 5, "{updates}");
    assert_eq!(millers, updates as usize, "a Miller paid for a job left behind: {millers} for {updates} updates");
    assert_eq!(finishes, updates as usize);
    assert_eq!(
        mine.iter().filter(|(_, _, o)| !o.applied && !o.rejected).count(),
        0,
        "nothing of the updater's refused"
    );
}

/// The in-process chain, refusing without executing it (nothing charged) each of the next `refuse` transactions whose
/// label ends with `suffix`, with no exit code and a note naming neither the window nor the gas: a cause that may pass.
struct Flaky {
    inner: HarnessLedger,
    suffix: &'static str,
    refuse: usize,
    refused: usize,
}

impl Ledger for Flaky {
    fn actor_state(&self, actor: Actor) -> anyhow::Result<Vec<u8>> {
        self.inner.actor_state(actor)
    }
    fn submit(&mut self, payer: &PrivateKey, tx: &Tx, gas_limit: u64, max_fee: u128) -> anyhow::Result<TxOutcome> {
        if self.refuse > 0 && tx.label.ends_with(self.suffix) {
            self.refuse -= 1;
            self.refused += 1;
            return Ok(TxOutcome {
                note: "Program violated execution rules".into(),
                gas_exec: 1,
                ..Default::default()
            });
        }
        self.inner.submit(payer, tx, gas_limit, max_fee)
    }
}

/// Probe D of the review of these fixes (L2): this updater's `Open` lands (the job is kept, its update in
/// `state_dir/<chain>/open/`), then its `Miller` is refused once for a cause that may pass (held [`REFUSAL_BACKOFF`]
/// rounds) and the sources move on to a newer update. The job used to be left at the first held round (its file
/// deleted), so the updater opened the newer update's job and the first `Open` was paid for nothing: 2 `Open`s paid,
/// 1 update applied. Now the job is kept through the hold and finished first: every `Open` paid belongs to an update
/// that is applied.
#[test]
fn a_kept_job_is_not_left_for_a_short_hold() {
    let config = preset("mainnet", 2).unwrap();
    let dir = lab_record("mainnet");
    let checkpoint = root(&fs::read_to_string(dir.join("checkpoint")).unwrap()).unwrap();
    let entries = load_entries(&dir.join("updates"), &config).unwrap();
    assert!(!entries[3].period_update && !entries[4].period_update, "entries 4 and 5: finality updates");
    let base = std::env::var_os("CARGO_TARGET_TMPDIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let state = base.join(format!("jobs-hold-{}", std::process::id()));
    let _ = fs::remove_dir_all(&state);
    fs::create_dir_all(&state).unwrap();
    let payer = key(0x63);
    let clock = Clock::new(3);
    let mut l =
        Flaky { inner: HarnessLedger::new(std::slice::from_ref(&payer)), suffix: "Miller", refuse: 0, refused: 0 };
    let inst = Instance::new("mainnet", HarnessLedger::program(), config.clone(), checkpoint);
    let policy = Policy { defer_window: 1, retry_base_ms: 0, ..Policy::default() };
    let source = Recorded::open("a", &dir, &config, clock.clone()).unwrap();
    let mut u =
        Updater::new("carry", inst, payer, Fees::default(), policy, vec![Box::new(source)], Some(state.clone()))
            .unwrap();
    u.sleep = no_sleep;
    for _ in 0..60 {
        if matches!(u.tick(&mut l).unwrap(), Plan::UpToDate) {
            break;
        }
    }
    let applied = |u: &Updater, l: &Flaky| u.inst.state(&l.inner).unwrap().map_or(0, |s| s.applied);
    let (applied0, n0) = (applied(&u, &l), l.inner.log.len());
    let files = || fs::read_dir(state.join("mainnet/open")).map(|d| d.count()).unwrap_or(0);

    clock.set(4);
    l.refuse = 1;
    let mut lines = Vec::new();
    let mut held_rounds = Vec::new();
    for round in 1..=40 {
        if round == 3 {
            clock.set(5); // the sources move on while the Miller is held
        }
        let p = u.tick(&mut l).unwrap();
        let what = match &p {
            Plan::Send(a) => format!("send {}", a.step.name()),
            Plan::Stuck(w) => format!("stuck: {}", w.chars().take(100).collect::<String>()),
            p => format!("{p:?}"),
        };
        if matches!(&p, Plan::Stuck(w) if w.starts_with("miller of job")) {
            held_rounds.push((round, u.carried_jobs(), files()));
        }
        lines.push(format!("round {round}: {what}; kept jobs {}, files in open/ {}", u.carried_jobs(), files()));
        if matches!(p, Plan::UpToDate) {
            break;
        }
    }
    let mine: Vec<_> = l.inner.log[n0..].iter().filter(|(p, ..)| *p == u.payer_id).collect();
    let opens = mine.iter().filter(|(_, label, o)| label.ends_with("Open") && o.applied).count();
    let open_fees: u128 = mine.iter().filter(|(_, label, _)| label.ends_with("Open")).map(|(_, _, o)| o.fee).sum();
    let updates = applied(&u, &l) - applied0;
    println!(
        "{}\nthe updater's applied Opens {opens} (fees {open_fees}); updates applied {updates}; held rounds (round, kept \
         jobs, files) {held_rounds:?}",
        lines.join("\n")
    );
    assert_eq!(held_rounds.len() as u64, REFUSAL_BACKOFF - 1, "held until the hold ends: {held_rounds:?}");
    assert!(held_rounds.iter().all(|&(_, kept, files)| kept == 1 && files == 1), "kept through the hold");
    assert_eq!(updates, 2, "entries 4 and 5 applied");
    assert_eq!(opens as u64, updates, "every Open paid belongs to an applied update");
    assert_eq!(l.refused, 1, "one Miller refused");
    assert!(u.held.is_empty(), "the hold ends when the step lands");
    assert_eq!((u.carried_jobs(), files()), (0, 0), "nothing kept once done");
}
