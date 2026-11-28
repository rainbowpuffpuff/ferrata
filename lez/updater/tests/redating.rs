//! Light-client sources that re-date real updates: they change `signature_slot`, which the signature does not cover
//! (only its fork version), to steer which update the updater chooses and when it sends it. An internal review's M1: a
//! source re-dating an update that adopts the next committee to slots that have started, a new slot each round, and
//! choosing them with the updater's tag (its name and payer account, both public), had the deferral postpone that
//! update's `Open` every round, so that no update was sent until the source was removed. A later review's M1: the
//! same source also sending each copy's `Open` (or `Open` and `Miller`) itself, which anyone may, so that the updater's
//! next step was a later one, still counted by the job; and the strongest attacker known against the fix, cycling
//! through every kind of step. That review's L2: `finish_own_jobs = false` with honest sources serving their
//! newest update. Then the variants of an earlier review's M1 (re-dating within the margin) that the reviews
//! measured (the reviewers' attack tests, kept as regression tests). The in-process chain (LEZ v0.4.0-rc1's state
//! machine and fee settlement, dev mode) with its block time set (`HarnessLedger::now`), the lab's recorded mainnet
//! data, as `refusals.rs`.
#![cfg(feature = "harness")]

use std::{
    collections::BTreeSet,
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering::SeqCst},
        Arc, Mutex,
    },
};

use ferrata_core::{
    steps::store_for_update,
    types::LightClientUpdate,
    witness::{CompactStore, CompactUpdate},
    Root,
};
use ferrata_lez_wire::{job_id, Config};
use ferrata_updater::{
    chain::{decode_finality_update, decode_update, layout_for, preset, root},
    config::{ClientMatch, Fees, Policy},
    ledger::{HarnessLedger, Ledger},
    source::{lab_record, load_entries, Clock, Entry, Recorded, Source},
    updater::{Instance, Plan, Step, Updater},
};
use lee::AccountId;
use ferrata_updater::lez::PayerKey;
use sha2::{Digest, Sha256};

fn key(b: u8) -> PayerKey {
    ferrata_updater::lez::payer_key([b; 32])
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

/// `defer_window` 1 (the other tests' policy) or 8 (the default, and the day-one deployment's).
fn policy(defer_window: u64) -> Policy {
    Policy { defer_window, retry_base_ms: 0, ..Policy::default() }
}

impl Rec {
    fn recorded(&self, label: &str, clock: &Clock) -> Recorded {
        Recorded::open(label, &self.dir, &self.config, clock.clone()).unwrap()
    }
    /// An honest source whose period-1908 update is entry 10 (a beacon node serves the period's best update, not its
    /// newest), not entry 12.
    fn honest_1908(&self, label: &str, clock: &Clock) -> Recorded {
        let mut s = self.recorded(label, clock);
        s.entries[11].bytes = s.entries[9].bytes.clone();
        s
    }
    fn instance(&self) -> Instance {
        Instance::new("mainnet", HarnessLedger::program(), self.config.clone(), self.checkpoint)
    }
    fn updater(&self, label: &str, payer: PayerKey, sources: Vec<Box<dyn Source>>, policy: Policy) -> Updater {
        let mut u = Updater::new(label, self.instance(), payer, Fees::default(), policy, sources, None).unwrap();
        u.sleep = no_sleep;
        u
    }
    /// Entry `e`'s update (1-based, as the record's file names).
    fn entry(&self, e: usize) -> LightClientUpdate {
        let b = &self.entries[e - 1].bytes;
        if self.entries[e - 1].period_update {
            decode_update(b, &self.config).unwrap().0
        } else {
            decode_finality_update(b, &self.config).unwrap().0
        }
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

/// `b` (a period update if `period`, else a finality update) with its `signature_slot` set to `to` (after the attested
/// slot, in the same period).
fn redate(mut b: Vec<u8>, config: &Config, period: bool, to: u64) -> anyhow::Result<Vec<u8>> {
    let dec = |b: &[u8]| if period { decode_update(b, config) } else { decode_finality_update(b, config) };
    let (u, _) = dec(&b)?;
    if to == u.signature_slot {
        return Ok(b);
    }
    anyhow::ensure!(to > u.attested_header.beacon.slot && to / 8192 == u.signature_slot / 8192, "{to}: not in period");
    let from = u.signature_slot.to_le_bytes();
    let at: Vec<usize> = b.windows(8).enumerate().filter(|(_, w)| *w == from).map(|(i, _)| i).collect();
    anyhow::ensure!(at.len() == 1, "signature_slot found {} times in the served bytes", at.len());
    b[at[0]..at[0] + 8].copy_from_slice(&to.to_le_bytes());
    let (re, _) = dec(&b)?;
    anyhow::ensure!(
        re.signature_slot == to && re.finalized_header == u.finalized_header && re.attested_header == u.attested_header,
        "re-dating failed"
    );
    Ok(b)
}

fn slot_of(config: &Config, ms: u64) -> u64 {
    (ms / 1000).saturating_sub(config.genesis_time) / config.seconds_per_slot
}

/// A round's record: the plan, the finalized slot and whether the next committee is known after it.
type Rounds = Vec<(Plan, u64, bool)>;

fn waits(r: &[(Plan, u64, bool)]) -> usize {
    r.iter().filter(|x| matches!(x.0, Plan::Waiting(_))).count()
}
/// The first round (1-based) after which the store is at `slot` with its next committee known.
fn adopted_at(r: &Rounds, slot: u64) -> Option<usize> {
    r.iter().position(|x| x.1 == slot && x.2).map(|i| i + 1)
}
/// The first round (1-based) after which the store is at `slot`.
fn at_slot(r: &Rounds, slot: u64) -> Option<usize> {
    r.iter().position(|x| x.1 == slot).map(|i| i + 1)
}
/// Refused (charged, not applied) transactions in `l` from index `from`.
fn refused(l: &HarnessLedger, from: usize) -> usize {
    l.log[from..].iter().filter(|(_, _, o)| !o.applied && !o.rejected).count()
}
fn refused_of(u: &Updater, name: &str) -> u64 {
    u.metrics.sources.get(name).map_or(0, |s| s.refused)
}

/// Entry 13's finalized slot (period 1908): its finality update rotates the committees of a store at entry 9, after
/// which the store lacks the next committee.
const ROTATED: u64 = 15_631_200;
/// Entry 20's finalized slot.
const ENTRY_20: u64 = 15_635_392;

// ---- The started slots (an internal review's M1): re-dated to slots that have started, a new job each round -------

/// The wait `H(tag, job, step) mod window` that an updater whose tag is `tag` draws for `step` of job `jid`.
fn wait(tag: &[u8; 32], jid: &Root, step: Step, window: u64) -> u64 {
    let mut h = Sha256::new();
    h.update(tag);
    h.update(jid);
    h.update([step.byte()]);
    let d: [u8; 32] = h.finalize().into();
    u64::from_le_bytes(d[..8].try_into().unwrap()) % window
}

/// A source serving, as its update of the store's period, a real update that carries the next committee, re-dated
/// each round to a slot that has started by Ethereum's clock (its `Open` window is open, so it can be sent and ranks
/// first while the store lacks the next committee) and that it has not served before (a new job each round). It knows
/// the updater's tag, as anyone can (its name and its payer's account), and so the wait the deferral draws for each
/// job: of the last 512 slots it may use, it takes the one whose `Open` waits longest, then whose later steps (`Miller`,
/// the two `Adopt` parts, `Finish`) wait longest. The review's attack, as strong as the tag allows.
struct Starver {
    inner: Recorded,
    config: Config,
    eth: Arc<AtomicU64>,
    store: Arc<Mutex<Option<CompactStore>>>,
    inst_id: Root,
    tag: [u8; 32],
    window: u64,
    used: BTreeSet<u64>,
    /// The copies served: (signature slot, its `Open`'s wait, its later steps' waits summed).
    served: Arc<Mutex<Vec<(u64, u64, u64)>>>,
}

impl Starver {
    fn serve(&mut self, b: Vec<u8>) -> anyhow::Result<Vec<u8>> {
        let ms = self.eth.load(SeqCst);
        let Some(store) = self.store.lock().unwrap().clone() else { return Ok(b) };
        let (u, _) = decode_update(&b, &self.config)?;
        if ms == 0
            || u.next_sync_committee.is_empty()
            || u.attested_header.beacon.slot / 8192 != store.finalized.slot / 8192
        {
            return Ok(b);
        }
        let layout = layout_for(&self.config, store.fork, &u).expect("a layout");
        let store_u = store_for_update(&store, layout, u.attested_header.beacon.slot, &self.config.spec()).unwrap();
        let store_digest = store_u.digest();
        let mut c = CompactUpdate::of(&u);
        let end = (u.signature_slot / 8192 + 1) * 8192 - 1;
        let later = [Step::Miller, Step::Adopt(0), Step::Adopt(1), Step::Finish];
        let mut best: Option<(u64, u64, u64)> = None;
        for s in
            (u.signature_slot..=slot_of(&self.config, ms).min(end)).rev().filter(|s| !self.used.contains(s)).take(512)
        {
            c.signature_slot = s;
            let jid = job_id(&self.inst_id, &store_digest, &c.digest(layout));
            let open = wait(&self.tag, &jid, Step::Open, self.window);
            let rest: u64 = later.iter().map(|&st| wait(&self.tag, &jid, st, self.window)).sum();
            if best.is_none_or(|(o, r, _)| (open, rest) > (o, r)) {
                best = Some((open, rest, s));
            }
        }
        let Some((open, rest, s)) = best else { return Ok(b) };
        self.used.insert(s);
        self.served.lock().unwrap().push((s, open, rest));
        redate(b, &self.config, true, s)
    }
}

impl Source for Starver {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn bootstrap(&mut self, block_root: &Root) -> anyhow::Result<Vec<u8>> {
        self.inner.bootstrap(block_root)
    }
    fn updates(&mut self, start_period: u64, count: u64) -> anyhow::Result<Vec<Vec<u8>>> {
        let list = self.inner.updates(start_period, count)?;
        list.into_iter().map(|b| self.serve(b)).collect()
    }
    fn finality_update(&mut self) -> anyhow::Result<Vec<u8>> {
        self.inner.finality_update()
    }
}

/// What a [`starved`] run gives: the rounds at entry 13 and at entry 20, the updater, the chain, the length of its log
/// before entry 13, and the copies served.
type Starved = (Rounds, Rounds, Updater, HarnessLedger, usize, Vec<(u64, u64, u64)>);

/// The started slots' setting with the deployed `defer_window` 8: the store at entry 9 (period 1907, its next
/// committee known); then the sources at entry 13 for `n13` rounds 12 s apart: both serve its finality update, which
/// rotates the committees (after which the store lacks the next one); as its period-1908 update the honest source
/// serves entry 10, the [`Starver`] entry 12 (finalizing later, so it ranks first), re-dated; then at entry 20 for
/// `n20` rounds.
fn starved(finish_own_jobs: bool, payer_byte: u8, n13: usize, n20: usize) -> Starved {
    let r = mainnet();
    let clock = Clock::new(9);
    let payer = key(payer_byte);
    let label = "starved";
    let payer_id = ferrata_updater::lez::payer_account(&payer);
    let tag: [u8; 32] = Sha256::digest(format!("{label}/{payer_id:?}").as_bytes()).into();
    let eth = Arc::new(AtomicU64::new(0));
    let shared = Arc::new(Mutex::new(None));
    let served = Arc::new(Mutex::new(Vec::new()));
    let liar = Starver {
        inner: r.recorded("liar", &clock),
        config: r.config.clone(),
        eth: eth.clone(),
        store: shared.clone(),
        inst_id: r.instance().id,
        tag,
        window: 8,
        used: BTreeSet::new(),
        served: served.clone(),
    };
    let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
    let honest = r.honest_1908("honest", &clock);
    // With `finish_own_jobs = false` the updater leaves each job it opens, and those tests measure the whole turn after
    // a send: join or wait is off there, since with it the updater would find its own earlier job open and join it
    // (ADR 0047; and from its `Open` on chain, ADR 0049), which bounds its `Open`s without the turn.
    let pol = Policy { finish_own_jobs, join_wait_rounds: (!finish_own_jobs).then_some(0), ..policy(8) };
    let mut u = r.updater(label, payer.clone(), vec![Box::new(liar), Box::new(honest)], pol);
    assert_eq!(u.payer_id, payer_id, "the tag as the source computes it");
    settle(&mut u, &mut l, 200);
    assert_eq!(store(&u, &l).finalized.slot, 15_630_016, "the record's entry 9");
    let n0 = l.log.len();
    let mut t = 0;
    let mut phases = Vec::new();
    for (entry, n) in [(13usize, n13), (20, n20)] {
        clock.set(entry);
        t = t.max(r.config.slot_start_ms(r.entry(entry).signature_slot) + 12_000);
        let mut p = Vec::new();
        for _ in 0..n {
            l.now = Some(t);
            eth.store(t, SeqCst);
            *shared.lock().unwrap() = u.inst.store(&l).unwrap();
            let plan = u.tick(&mut l).unwrap();
            let s = store(&u, &l);
            p.push((plan, s.finalized.slot, s.next.is_some()));
            t += 12_000;
        }
        phases.push(p);
    }
    let p20 = phases.pop().unwrap();
    let p13 = phases.pop().unwrap();
    let served = served.lock().unwrap().clone();
    (p13, p20, u, l, n0, served)
}

/// The started slots. The source re-dates entry 12, which adopts the next committee, every round to a started slot
/// it has not used, the one whose `Open` the updater would defer longest. The deferral used to count from the first
/// round each job was due, and the job id hashes `signature_slot`: every copy was a new job, deferred in its first
/// round, so the next committee was never adopted and nothing was sent past the period's end (the review: 113 rounds),
/// the source never demoted. Now an `Open` counts from the first round an `Open` was due on the instance's state,
/// whatever update it is for: it waits at most `defer_window - 1` rounds (7), the copy is sent and applied (a real update:
/// the next committee adopted), each later step waits its own turn, and entry 20's update is applied once served. The
/// source is not demoted: a started slot is what an honest update carries.
#[test]
fn started_slots_an_adopting_update_re_dated_each_round_does_not_hold_the_instance() {
    let w = 8u64;
    let (p13, p20, u, l, n0, served) = starved(true, 0xc1, 60, 30);
    let rotated = at_slot(&p13, ROTATED);
    let adopted = adopted_at(&p13, ROTATED);
    // The `Open`s deferred after the rotation (before it, the rotating job's own `Open` waits its turn).
    let open_deferred =
        |p: &[(Plan, u64, bool)]| p.iter().filter(|x| matches!(x.0, Plan::Deferred(Step::Open))).count();
    let after_rotation = &p13[rotated.unwrap_or(p13.len())..];
    let opens: Vec<&String> = l.log[n0..]
        .iter()
        .filter(|(_, label, o)| label.ends_with("Open") && o.applied)
        .map(|(_, label, _)| label)
        .collect();
    let (status, why) = u.health();
    println!(
        "a source re-dating entry 12 to a new started slot each round, chosen by the updater's tag, defer_window {w}: at \
         entry 13, 60 rounds: rotated in round {rotated:?}, the next committee adopted in round {adopted:?}, {} `Open` \
         deferrals after the rotation, {} waiting; at entry 20, 30 rounds: finalized slot {} (entry 20's {ENTRY_20}); copies served {} \
         (the first ten as (slot, Open's wait, later waits summed): {:?}); Opens applied {opens:?}; the liar refused {}; \
         refused txs {}; health {status} {why:?}",
        open_deferred(after_rotation),
        waits(&p13),
        p20.last().unwrap().1,
        served.len(),
        &served[..served.len().min(10)],
        refused_of(&u, "liar"),
        refused(&l, 0),
    );
    let rotated = rotated.expect("rotated by entry 13's finality update");
    let adopted = adopted.expect("the next committee adopted at entry 13");
    // The source served a new copy in each round of the `Open`'s turn (while the updater carries the job it is not
    // asked), each one the deferral would hold back.
    assert!(served.len() as u64 >= w - 1, "{served:?}");
    assert!(served.iter().all(|&(_, open, _)| open >= 1), "every copy one the deferral holds back: {served:?}");
    // Each of the job's five steps (`Open`, `Miller`, two `Adopt` parts, `Finish`) waits at most `w - 1` rounds and is
    // sent in the next.
    assert!(adopted - rotated <= 5 * w as usize, "rotated in round {rotated}, adopted in round {adopted}");
    assert!((open_deferred(after_rotation) as u64) < w, "the Open waited more than its turn: {p13:?}");
    assert_eq!(waits(&p13) + waits(&p20), 0);
    assert_eq!(p20.last().unwrap().1, ENTRY_20, "entry 20 applied");
    assert_eq!(refused(&l, 0), 0);
    assert_eq!(refused_of(&u, "liar"), 0, "a started slot is not refused");
    assert_eq!(status, "ok", "{why:?}");
}

/// The same source against `finish_own_jobs = false`: each round the updater takes the best update, the source's newest
/// copy, and leaves the job it opened the round before (it finishes none: the source never serves a copy twice, as a
/// beacon node serving its newest finality update never does). An `Open` sent on a state took this updater's turn
/// there, and the next `Open` on the same state waits its turn again: one `Open` per `defer_window` rounds at most,
/// not one per round once the first turn has passed.
#[test]
fn started_slots_with_finish_own_jobs_off_an_open_is_sent_once_per_turn() {
    let w = 8u64;
    let (p13, _, u, l, n0, served) = starved(false, 0xc2, 60, 0);
    let rotated = at_slot(&p13, ROTATED).expect("rotated by entry 13's finality update");
    // Entry 12 finalizes slot 15,630,720: its copies' `Open`s, apart from the rotation's (entry 13, 15,631,200).
    let opens = |fin: u64| {
        let label = format!("mainnet fin {fin} Open");
        l.log[n0..].iter().filter(|(_, lab, o)| *lab == label && o.applied).count() as u64
    };
    let (rotation, copies) = (opens(ROTATED), opens(15_630_720));
    let after = (p13.len() - rotated) as u64;
    println!(
        "finish_own_jobs = false, the same source: 60 rounds at entry 13, rotated in round {rotated} ({rotation} Open); \
         in the {after} rounds after, {copies} Opens of the source's copies applied (one per turn: at most {}); the \
         next committee adopted in round {:?}; copies served {}; refused txs {}; health {:?}",
        after.div_ceil(w),
        adopted_at(&p13, ROTATED),
        served.len(),
        refused(&l, 0),
        u.health()
    );
    assert_eq!(rotation, 1);
    assert!(copies <= after.div_ceil(w), "{copies} Opens in {after} rounds");
    assert!(copies >= 1, "the source's copies are sent");
    assert_eq!(refused(&l, 0), 0);
}

// ---- The opener (a later review's M1): the same source opening each copy's job itself (anyone may) -----------------

fn id_of(k: &PayerKey) -> AccountId {
    ferrata_updater::lez::payer_account(k)
}

/// The updater's deferral tag as anyone computes it: from its name and its payer's account.
fn tag_of(label: &str, payer: &PayerKey) -> [u8; 32] {
    Sha256::digest(format!("{label}/{:?}", id_of(payer)).as_bytes()).into()
}

/// A source serving the attacker's current copy (when set) in place of the update with the same attested header (the
/// record's entry 12, its period-1908 update, which carries the next committee); everything else as recorded.
struct CopySource {
    inner: Recorded,
    config: Config,
    current: Arc<Mutex<Option<Vec<u8>>>>,
}

impl CopySource {
    fn serve(&self, b: Vec<u8>) -> anyhow::Result<Vec<u8>> {
        let Some(copy) = self.current.lock().unwrap().clone() else { return Ok(b) };
        let (u, _) = decode_update(&b, &self.config)?;
        let (c, _) = decode_update(&copy, &self.config)?;
        Ok(if u.attested_header == c.attested_header { copy } else { b })
    }
}

impl Source for CopySource {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn bootstrap(&mut self, block_root: &Root) -> anyhow::Result<Vec<u8>> {
        self.inner.bootstrap(block_root)
    }
    fn updates(&mut self, start_period: u64, count: u64) -> anyhow::Result<Vec<Vec<u8>>> {
        let list = self.inner.updates(start_period, count)?;
        list.into_iter().map(|b| self.serve(b)).collect()
    }
    fn finality_update(&mut self) -> anyhow::Result<Vec<u8>> {
        self.inner.finality_update()
    }
}

/// A job's steps in order: `Open`, `Miller`, the adoption's two parts (the tests' preset), `Finish`.
const STEPS: [Step; 5] = [Step::Open, Step::Miller, Step::Adopt(0), Step::Adopt(1), Step::Finish];

/// Of the started, unused slots of entry 12's period (the latest 512 up to the slot holding `ms`), the one whose job
/// scores highest by `score` (given the job id), the latest on ties: (slot, score).
fn pick_slot(
    r: &Rec,
    store: &CompactStore,
    base: &[u8],
    used: &BTreeSet<u64>,
    ms: u64,
    score: &dyn Fn(&Root) -> u64,
) -> (u64, u64) {
    let (u, _) = decode_update(base, &r.config).unwrap();
    let layout = layout_for(&r.config, store.fork, &u).expect("a layout");
    let store_u = store_for_update(store, layout, u.attested_header.beacon.slot, &r.config.spec()).unwrap();
    let digest = store_u.digest();
    let inst_id = r.instance().id;
    let mut c = CompactUpdate::of(&u);
    let end = (u.signature_slot / 8192 + 1) * 8192 - 1;
    let mut best: Option<(u64, u64)> = None;
    for s in (u.signature_slot..=slot_of(&r.config, ms).min(end)).rev().filter(|s| !used.contains(s)).take(512) {
        c.signature_slot = s;
        let v = score(&job_id(&inst_id, &digest, &c.digest(layout)));
        if best.is_none_or(|(_, b)| v > b) {
            best = Some((s, v));
        }
    }
    best.expect("a started, unused slot")
}

/// How the attacker serves copies of entry 12 while the store lacks its next committee. It knows the updater's tag, so
/// the wait the updater draws for each step of each copy's job.
#[derive(Clone, Copy, Debug)]
enum Attack {
    /// A new copy every `every` rounds, the attacker having sent its first `stage` steps itself (1: its `Open`; 2: its
    /// `Open` and `Miller`), chosen by the updater's wait for the step then due.
    Copies { stage: usize, every: usize },
    /// Each kind of step in turn, `Open` to `Finish`: for `defer_window - 1` rounds each (the longest a turn lasts), a
    /// copy at that step (the steps before it sent by the attacker), chosen by its wait; then a copy not yet opened,
    /// chosen by the waits of its later steps (the job the updater then carries). The strongest attacker known against
    /// a turn per kind of step and state.
    Cycle,
}

#[derive(Debug, Default)]
struct OpenedRun {
    rotated: Option<usize>,
    adopted: Option<usize>,
    final_slot: u64,
    sends_after_rotation: usize,
    deferrals_after_rotation: usize,
    copies: usize,
    attacker_txs: usize,
    attacker_fee: u128,
    liar_refused: u64,
    victim_refused: usize,
    health: String,
}

/// The store at entry 9; the sources at entry 13 for `n13` rounds 12 s apart (its finality update rotates the
/// committees, after which the store lacks the next one), then at entry 20 for `n20`; `defer_window` 8. The liar serves
/// the attacker's copies of entry 12 (it adopts the next committee and finalizes later than the honest source's entry
/// 10); the attacker sends steps of them from its own account just before the updater's round.
fn opened(attack: Attack, payer_byte: u8, n13: usize, n20: usize) -> OpenedRun {
    let w = 8u64;
    let r = mainnet();
    let clock = Clock::new(9);
    let victim = key(payer_byte);
    let attacker = key(payer_byte.wrapping_add(0x40));
    let label = "victim";
    let tag = tag_of(label, &victim);
    let current = Arc::new(Mutex::new(None));
    let mut l = HarnessLedger::new(&[victim.clone(), attacker.clone()]);
    let liar = CopySource { inner: r.recorded("liar", &clock), config: r.config.clone(), current: current.clone() };
    let honest = r.honest_1908("honest", &clock);
    let mut u = r.updater(label, victim.clone(), vec![Box::new(liar), Box::new(honest)], policy(w));
    assert_eq!(u.payer_id, id_of(&victim), "the tag as the attacker computes it");
    let src = CopySource { inner: r.recorded("opener", &clock), config: r.config.clone(), current: current.clone() };
    // The attacker sends the steps it chooses: it neither finishes its jobs nor waits for others' (join or wait off).
    let att_policy = Policy { finish_own_jobs: false, join_wait_rounds: Some(0), ..policy(1) };
    let mut att = r.updater("attacker", attacker.clone(), vec![Box::new(src)], att_policy);
    settle(&mut u, &mut l, 200);
    assert_eq!(store(&u, &l).finalized.slot, 15_630_016, "the record's entry 9");
    let base = r.entries[11].bytes.clone();
    let period = decode_update(&base, &r.config).unwrap().0.attested_header.beacon.slot / 8192;
    let n0 = l.log.len();
    let (mut used, mut on_current, mut lacking) = (BTreeSet::new(), 0usize, 0usize);
    let mut out = OpenedRun::default();
    let mut all: Rounds = Vec::new();
    let mut t = 0u64;
    for (entry, n) in [(13usize, n13), (20, n20)] {
        clock.set(entry);
        t = t.max(r.config.slot_start_ms(r.entry(entry).signature_slot) + 12_000);
        for _ in 0..n {
            l.now = Some(t);
            let s = store(&u, &l);
            if s.next.is_none() && s.finalized.slot / 8192 == period {
                // (steps the attacker sends of a new copy, whether a new copy is due, what the copy is chosen by)
                let turn = (w - 1) as usize;
                let (stage, new, by): (usize, bool, Vec<Step>) = match attack {
                    Attack::Copies { stage, every } => {
                        (stage, current.lock().unwrap().is_none() || on_current >= every, vec![STEPS[stage]])
                    }
                    Attack::Cycle if lacking < STEPS.len() * turn => {
                        let phase = lacking / turn;
                        (phase, lacking % turn == 0, vec![STEPS[phase]])
                    }
                    Attack::Cycle => (0, lacking == STEPS.len() * turn, STEPS[1..].to_vec()),
                };
                if new {
                    let score = |jid: &Root| by.iter().map(|&st| wait(&tag, jid, st, w)).sum::<u64>();
                    let (slot, _) = pick_slot(&r, &s, &base, &used, t, &score);
                    used.insert(slot);
                    *current.lock().unwrap() = Some(redate(base.clone(), &r.config, true, slot).unwrap());
                    on_current = 0;
                    out.copies += 1;
                    for &step in &STEPS[..stage] {
                        let p = att.tick(&mut l).unwrap();
                        assert!(matches!(&p, Plan::Send(a) if a.step == step), "the attacker's {step:?}: {p:?}");
                    }
                }
                on_current += 1;
                lacking += 1;
            } else {
                *current.lock().unwrap() = None;
            }
            let p = u.tick(&mut l).unwrap();
            let s = store(&u, &l);
            all.push((p, s.finalized.slot, s.next.is_some()));
            t += 12_000;
        }
    }
    out.rotated = at_slot(&all, ROTATED);
    out.adopted = adopted_at(&all, ROTATED);
    out.final_slot = all.last().unwrap().1;
    let after = &all[out.rotated.unwrap_or(all.len())..];
    out.sends_after_rotation = after.iter().filter(|x| matches!(x.0, Plan::Send(_))).count();
    out.deferrals_after_rotation = after.iter().filter(|x| matches!(x.0, Plan::Deferred(_))).count();
    let (victim_id, attacker_id) = (id_of(&victim), id_of(&attacker));
    for (payer, _, o) in &l.log[n0..] {
        if *payer == attacker_id {
            out.attacker_txs += 1;
            out.attacker_fee += o.fee;
        } else if *payer == victim_id && !o.applied && !o.rejected {
            out.victim_refused += 1;
        }
    }
    out.liar_refused = refused_of(&u, "liar");
    let (status, why) = u.health();
    out.health = format!("{status} {why:?}");
    out
}

/// The longest the next committee can take to be adopted after the rotation, in rounds, at `defer_window` 8 with two
/// adoption parts: `K = 5` kinds of step, each with one turn per state of at most 7 rounds (`K × 7`), then the job this
/// updater carries, from its `Open` (1) through its other four steps, each waiting at most 7 rounds and sent in the
/// next (`4 × 8`).
const BOUND: usize = 5 * 7 + 1 + 4 * 8;

fn assert_opened_bounded(what: &str, o: &OpenedRun) {
    println!(
        "{what}: rotated in round {:?}, the next committee adopted in round {:?}, final slot {}; after the rotation: {} \
         sends, {} deferrals; copies served {}; the attacker's transactions {} (fee {}); the liar refused {}; the \
         updater's refused transactions {}; health {}",
        o.rotated,
        o.adopted,
        o.final_slot,
        o.sends_after_rotation,
        o.deferrals_after_rotation,
        o.copies,
        o.attacker_txs,
        o.attacker_fee,
        o.liar_refused,
        o.victim_refused,
        o.health
    );
    let rotated = o.rotated.expect("rotated by entry 13's finality update");
    let adopted = o.adopted.expect("the next committee adopted at entry 13");
    assert!(adopted - rotated <= BOUND, "{what}: rotated in round {rotated}, adopted in round {adopted}");
    assert_eq!(o.final_slot, ENTRY_20, "{what}: entry 20 applied");
    assert_eq!(o.liar_refused, 0, "{what}: a started slot is not refused");
    assert_eq!(o.victim_refused, 0, "{what}");
}

/// The opener, the economical form: the source also sends each copy's `Open` itself, so the updater finds
/// the job open and its next step is `Miller`; a new copy every 7 rounds (the longest a turn lasts), chosen by the
/// `Miller` wait. Counted by the job, a new opened copy restarted the count: nothing sent in 115 rounds after the
/// rotation, for one `Open` per 7 rounds (the review). Now the `Miller` of a job this updater does not carry waits one
/// turn per state.
#[test]
fn opener_a_new_opened_copy_every_turn_does_not_hold_the_instance() {
    let o = opened(Attack::Copies { stage: 1, every: 7 }, 0xe2, 90, 30);
    assert_opened_bounded("a new copy every 7 rounds, its Open sent by the source", &o);
}

/// The same, a new opened copy each round (the review: nothing sent in 107 rounds, 107 `Open`s by the attacker).
#[test]
fn opener_a_new_opened_copy_each_round_does_not_hold_the_instance() {
    let o = opened(Attack::Copies { stage: 1, every: 1 }, 0xe1, 90, 30);
    assert_opened_bounded("a new copy each round, its Open sent by the source", &o);
}

/// The source sends each copy's `Open` and `Miller`, a new copy each round chosen by the first `Adopt` part's wait (the
/// review: nothing sent in 111 rounds, 222 transactions by the attacker).
#[test]
fn opener_sending_open_and_miller_does_not_hold_the_instance() {
    let o = opened(Attack::Copies { stage: 2, every: 1 }, 0xe5, 90, 30);
    assert_opened_bounded("a new copy each round, its Open and Miller sent by the source", &o);
}

/// The strongest attacker known against a turn per kind of step and state ([`Attack::Cycle`]): it uses up the turn of
/// each kind of step in turn, then leaves this updater a job to carry from its `Open`, its later steps chosen to wait
/// longest. The next committee is adopted within [`BOUND`] rounds of the rotation.
#[test]
fn opener_cycling_through_every_kind_of_step_is_bounded() {
    let o = opened(Attack::Cycle, 0xe7, 90, 30);
    assert_opened_bounded("each kind of step in turn, then a job to carry", &o);
}

// ---- Newest-only sources (its L2): finish_own_jobs = false with honest sources serving their newest update --------

/// A recorded source serving, as its finality update, its newest one re-dated to the slot that held Ethereum's time
/// `delay_ms` earlier: as a beacon node serves a new finality update each slot (a new job each round).
struct Newest {
    inner: Recorded,
    config: Config,
    eth: Arc<AtomicU64>,
    delay_ms: u64,
}

impl Source for Newest {
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
        let ms = self.eth.load(SeqCst);
        if ms == 0 {
            return Ok(b);
        }
        let (u, _) = decode_finality_update(&b, &self.config)?;
        let end = (u.signature_slot / 8192 + 1) * 8192 - 1;
        let to = slot_of(&self.config, ms.saturating_sub(self.delay_ms)).min(end).max(u.signature_slot);
        redate(b, &self.config, false, to)
    }
}

/// Newest-only sources: `finish_own_jobs = false`, two honest sources serving their newest finality update each
/// slot (a new job each round), `defer_window` 8, entries 4 to 7 for 15 rounds each. The updater takes the newest
/// update each round and leaves the job it opened, so it finishes none; after each `Open` sent on the state, the next
/// one there waits a whole turn: at most one `Open` per 8 rounds (the review: 21 in 60 rounds before the whole turn
/// after a send, a fresh turn after each send ending at the first round whose wait, drawn from that round's job, had
/// run out; 9 with the turn counted by the job). Join or wait is off (`join_wait_rounds = 0`): with it the updater finds
/// its own earlier job open and joins it (ADR 0047; and from its `Open` on chain, ADR 0049), which bounds its `Open`s
/// without the turn; that run is made too, and is bounded as well.
#[test]
fn newest_only_sources_with_finish_own_jobs_off_open_once_per_turn() {
    for join_wait in [false, true] {
        newest_only_run(join_wait);
    }
}

fn newest_only_run(join_wait: bool) {
    let w = 8u64;
    let r = mainnet();
    let clock = Clock::new(3);
    let eth = Arc::new(AtomicU64::new(0));
    let payer = key(0xf7);
    let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
    let src = |name: &str| -> Box<dyn Source> {
        Box::new(Newest {
            inner: r.recorded(name, &clock),
            config: r.config.clone(),
            eth: eth.clone(),
            delay_ms: 2_000,
        })
    };
    let pol = Policy { finish_own_jobs: false, join_wait_rounds: (!join_wait).then_some(0), ..policy(w) };
    let mut u = r.updater("newest", payer.clone(), vec![src("a"), src("b")], pol);
    settle(&mut u, &mut l, 60);
    let (n0, fin) = (l.log.len(), store(&u, &l).finalized.slot);
    let mut rounds = 0u64;
    for e in 4..8 {
        clock.set(e);
        let mut t = r.config.slot_start_ms(r.entry(e).signature_slot) + 15_000;
        for _ in 0..15 {
            l.now = Some(t);
            eth.store(t, SeqCst);
            u.tick(&mut l).unwrap();
            rounds += 1;
            t += 12_000;
        }
    }
    let sent = &l.log[n0..];
    let opens = sent.iter().filter(|(_, label, o)| label.ends_with("Open") && o.applied).count() as u64;
    let fee: u128 = sent.iter().map(|(_, _, o)| o.fee).sum();
    let moved = store(&u, &l).finalized.slot;
    println!(
        "finish_own_jobs = false, two sources serving their newest finality update each slot, defer_window {w}, join or \
         wait {join_wait}: {rounds} rounds, {opens} Opens applied (one per turn: at most {}), finalized slot {fin} -> \
         {moved}, fees {fee}; refused txs {}",
        rounds.div_ceil(w),
        refused(&l, n0)
    );
    assert!(opens <= rounds.div_ceil(w), "{opens} Opens in {rounds} rounds");
    assert!(opens >= 1);
    assert_eq!(refused(&l, n0), 0);
}

// ---- The earlier attacks' variants (re-dating within the margin): red under m1roll or m1 -------------------------

/// How many slots past the slot holding Ethereum's clock a source re-dates its update in round `n` (0: that slot,
/// which has started); at most the period's last slot, never before the recorded signature slot.
type Schedule = Box<dyn Fn(usize) -> u64 + Send>;

/// Ethereum's clock (what sources see) and the round's number, set by the test each round.
#[derive(Clone)]
struct Clocks {
    eth: Arc<AtomicU64>,
    round: Arc<AtomicUsize>,
}

impl Clocks {
    fn new() -> Self {
        Self { eth: Arc::new(AtomicU64::new(0)), round: Arc::new(AtomicUsize::new(0)) }
    }
}

/// A recorded source re-dating, each round, as `schedule(round)` says: its period updates (`period`) or its
/// finality update.
struct Liar {
    inner: Recorded,
    config: Config,
    clocks: Clocks,
    period: bool,
    schedule: Schedule,
}

impl Liar {
    fn new(inner: Recorded, r: &Rec, clocks: &Clocks, period: bool, schedule: Schedule) -> Self {
        Self { inner, config: r.config.clone(), clocks: clocks.clone(), period, schedule }
    }
    fn serve(&self, b: Vec<u8>, period: bool) -> anyhow::Result<Vec<u8>> {
        let ms = self.clocks.eth.load(SeqCst);
        if ms == 0 {
            return Ok(b);
        }
        let (u, _) = if period { decode_update(&b, &self.config)? } else { decode_finality_update(&b, &self.config)? };
        let k = (self.schedule)(self.clocks.round.load(SeqCst));
        let end = (u.signature_slot / 8192 + 1) * 8192 - 1;
        let to = (slot_of(&self.config, ms) + k).min(end).max(u.signature_slot);
        redate(b, &self.config, period, to)
    }
}

impl Source for Liar {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn bootstrap(&mut self, block_root: &Root) -> anyhow::Result<Vec<u8>> {
        self.inner.bootstrap(block_root)
    }
    fn updates(&mut self, start_period: u64, count: u64) -> anyhow::Result<Vec<Vec<u8>>> {
        let list = self.inner.updates(start_period, count)?;
        if !self.period {
            return Ok(list);
        }
        list.into_iter().map(|b| self.serve(b, true)).collect()
    }
    fn finality_update(&mut self) -> anyhow::Result<Vec<u8>> {
        let b = self.inner.finality_update()?;
        if self.period {
            return Ok(b);
        }
        self.serve(b, false)
    }
    fn client(&mut self) -> anyhow::Result<String> {
        self.inner.client()
    }
}

/// `n` rounds 12 s apart from Ethereum's time `*t` (moved on past them), the chain's clock with it.
fn run(u: &mut Updater, l: &mut HarnessLedger, c: &Clocks, t: &mut u64, n: usize) -> Rounds {
    let mut out = Vec::new();
    for _ in 0..n {
        l.now = Some(*t);
        c.eth.store(*t, SeqCst);
        let p = u.tick(l).unwrap();
        let s = store(u, l);
        out.push((p, s.finalized.slot, s.next.is_some()));
        c.round.fetch_add(1, SeqCst);
        *t += 12_000;
    }
    out
}

/// The earlier attack's setting (re-dating within the margin): the store at entry 9; the sources at entry 13 for 60
/// rounds, then at entry 20 for 30. Returns the rounds at 13 and at 20.
fn adoption(r: &Rec, u: &mut Updater, l: &mut HarnessLedger, clock: &Clock, c: &Clocks) -> (Rounds, Rounds) {
    settle(u, l, 200);
    let s0 = store(u, l);
    assert_eq!(s0.finalized.slot, 15_630_016, "the record's entry 9");
    assert!(s0.next.is_some());
    clock.set(13);
    let mut t = r.config.slot_start_ms(r.entry(13).signature_slot) + 12_000;
    let p13 = run(u, l, c, &mut t, 60);
    clock.set(20);
    t = t.max(r.config.slot_start_ms(r.entry(20).signature_slot) + 12_000);
    let p20 = run(u, l, c, &mut t, 30);
    (p13, p20)
}

/// The next committee adopted at entry 13, entry 20 applied, no round waiting, nothing refused.
fn assert_adoption_held_nothing(what: &str, p13: &Rounds, p20: &Rounds, u: &Updater, l: &HarnessLedger) {
    println!(
        "{what}: at entry 13, 60 rounds: {} waiting, rotated in round {:?}, adopted in round {:?}; at entry 20, 30 \
         rounds: {} waiting, finalized slot {} in round {:?}; refused txs {}; health {:?}",
        waits(p13),
        at_slot(p13, ROTATED),
        adopted_at(p13, ROTATED),
        waits(p20),
        p20.last().unwrap().1,
        at_slot(p20, ENTRY_20),
        refused(l, 0),
        u.health()
    );
    assert!(adopted_at(p13, ROTATED).is_some(), "{what}: the next committee adopted at entry 13: {p13:?}");
    assert_eq!(waits(p13) + waits(p20), 0, "{what}");
    assert_eq!(p20.last().unwrap().1, ENTRY_20, "{what}: entry 20 applied");
    assert_eq!(refused(l, 0), 0, "{what}");
}

/// A case of the varying re-dates: what, its schedule (slots past the clock in round `n`), `defer_window`, the payer.
type Varying = (&'static str, fn(usize) -> u64, u64, u8);

/// SplitMix64 of `n`: a schedule's pseudo-random draw.
fn mix(n: usize) -> u64 {
    let mut z = (n as u64).wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// A review's variant 1: the source re-dates entry 12 by a few slots past the clock, varying each round. The
/// review's schedule, `(n * 6364136223846793005 + 1442695040888963407) >> 33` mod 6 in round `n`, meant 0 to 5 slots;
/// it gives 5, 2, 4, 5, 2, 4, ..., 1, 2 or 4 or 5 in every round before round 107: inside the 60 s margin, not refused,
/// its window never open. With `defer_window` 1 and 8. Under the order of `81bbea61` (mutant `m1roll`) it held the
/// adoption (the review: 57 and 46 of 60 rounds waiting, never adopted). Then 0 to 5 slots by SplitMix64 (0, the slot
/// that has just started, in 15 of the first 90 rounds), `defer_window` 8: copies that can be sent and copies that
/// cannot, mixed.
#[test]
fn earlier_re_dated_a_few_slots_varying_each_round_does_not_hold_the_adoption() {
    let review = |n: usize| {
        ((n as u64).wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407) >> 33) % 6
    };
    let cases: [Varying; 3] = [
        ("1 to 5 slots, the review's schedule", review, 1, 0x83),
        ("1 to 5 slots, the review's schedule", review, 8, 0x84),
        ("0 to 5 slots, SplitMix64", |n| mix(n) % 6, 8, 0x86),
    ];
    for (what, schedule, w, payer) in cases {
        let r = mainnet();
        let clock = Clock::new(9);
        let c = Clocks::new();
        let payer = key(payer);
        let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
        let liar = Liar::new(r.recorded("liar", &clock), &r, &c, true, Box::new(schedule));
        let honest = r.honest_1908("honest", &clock);
        let mut u = r.updater("varying", payer.clone(), vec![Box::new(liar), Box::new(honest)], policy(w));
        let (p13, p20) = adoption(&r, &mut u, &mut l, &clock, &c);
        assert_adoption_held_nothing(&format!("re-dated {what}, defer_window {w}"), &p13, &p20, &u, &l);
        assert_eq!(refused_of(&u, "liar"), 0, "within the margin: not demoted");
    }
}

/// A review's variant 2: two sources re-date entry 12 alike (four slots past the clock every round: the same
/// bytes), beside one honest source; two honest sources with `min_agreeing = 2`; four sources reporting four clients
/// with `min_distinct_clients = 2` and `client_match = "bytes"`. Under `m1roll` each held the adoption (57 of 60 rounds
/// waiting).
#[test]
fn earlier_two_liars_re_dating_alike_do_not_hold_the_adoption() {
    let families = ["Lodestar/v1.51.0", "Teku/v25.11.0", "Nimbus/v26.10.2", "Lighthouse/v8.3.3"];
    for (honest, min_agreeing, k, payer) in [(1usize, 1usize, 0usize, 0x91u8), (2, 2, 0, 0x92), (2, 1, 2, 0x93)] {
        let r = mainnet();
        let clock = Clock::new(9);
        let c = Clocks::new();
        let payer = key(payer);
        let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
        let mut sources: Vec<Box<dyn Source>> = Vec::new();
        for (i, name) in ["liar-1", "liar-2"].iter().enumerate() {
            let mut inner = r.recorded(name, &clock);
            if k >= 2 {
                inner.client = Some(families[i].to_string());
            }
            sources.push(Box::new(Liar::new(inner, &r, &c, true, Box::new(|_| 4))));
        }
        for i in 0..honest {
            let mut h = r.honest_1908(&format!("honest-{i}"), &clock);
            if k >= 2 {
                h.client = Some(families[2 + i].to_string());
            }
            sources.push(Box::new(h));
        }
        let pol = Policy { min_agreeing, min_distinct_clients: k, client_match: ClientMatch::Bytes, ..policy(1) };
        let mut u = r.updater("two-liars", payer.clone(), sources, pol);
        let (p13, p20) = adoption(&r, &mut u, &mut l, &clock, &c);
        assert_adoption_held_nothing(
            &format!("two liars alike, {honest} honest, min_agreeing {min_agreeing}, min_distinct_clients {k}"),
            &p13,
            &p20,
            &u,
            &l,
        );
        assert_eq!(refused_of(&u, "liar-1") + refused_of(&u, "liar-2"), 0, "within the margin: not demoted");
    }
}

/// A review's case R3: a source that re-dates its finality update 600 slots on every other answer and is honest
/// otherwise, over four entries of 60 rounds (`demote_rounds` 50): no round waits, every update the other source serves
/// is applied, and the liar is demoted again each time it lies after its demotion has ended. Without the refusal of
/// updates signed in the future (mutant `m1`) it is never demoted.
struct Alternating {
    inner: Recorded,
    config: Config,
    n: usize,
}

impl Source for Alternating {
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
        self.n += 1;
        if self.n.is_multiple_of(2) {
            return Ok(b);
        }
        let (u, _) = decode_finality_update(&b, &self.config)?;
        let end = (u.signature_slot / 8192 + 1) * 8192 - 1;
        redate(b, &self.config, false, (u.signature_slot + 600).min(end))
    }
}

#[test]
fn earlier_a_liar_re_dating_far_on_every_other_answer_is_demoted_each_time() {
    let r = mainnet();
    let clock = Clock::new(3);
    let payer = key(0x65);
    let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
    let liar = Alternating { inner: r.recorded("liar", &clock), config: r.config.clone(), n: 0 };
    let sources: Vec<Box<dyn Source>> = vec![Box::new(liar), Box::new(r.recorded("honest", &clock))];
    let mut u = r.updater("alternating", payer.clone(), sources, policy(1));
    settle(&mut u, &mut l, 60);
    let mut waiting = 0;
    let mut applied = Vec::new();
    for e in 4..8 {
        clock.set(e);
        let f = r.entry(e);
        let mut t = r.config.slot_start_ms(f.signature_slot) + 12_000;
        for _ in 0..60 {
            l.now = Some(t);
            if matches!(u.tick(&mut l).unwrap(), Plan::Waiting(_)) {
                waiting += 1;
            }
            t += 12_000;
        }
        applied.push((f.finalized_header.beacon.slot, store(&u, &l).finalized.slot));
    }
    println!(
        "a liar re-dating 600 slots on every other answer: 4 x 60 rounds, {waiting} waiting; (served, applied) \
         {applied:?}; the liar refused {} time(s); refused txs {}",
        refused_of(&u, "liar"),
        refused(&l, 0)
    );
    assert_eq!(waiting, 0);
    assert!(applied.iter().all(|(a, b)| a == b), "{applied:?}");
    assert!(refused_of(&u, "liar") >= 2, "demoted again after its demotion ended");
    assert_eq!(refused(&l, 0), 0);
}
