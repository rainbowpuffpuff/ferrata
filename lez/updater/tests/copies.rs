//! Copies of one update (RFP-021 R.9): beacon nodes serve different finality updates for one finalized checkpoint
//! (another attested header, signature slot or set of signers), and a light-client update's `signature_slot` is not
//! covered by the sync committee's signature (only its fork version is), so independent operators' sources need not
//! serve the same bytes for the same update. A job's id hashes the whole update, so before the updater learned other
//! parties' updates from their `Open`s on chain (`Ledger::messages_to`), it could not find a job opened for a copy its
//! sources never served, and opened a second job for the same update. Here: the bytes an `Open` carries give back the
//! bytes a source served (`served_update_of_open`, the SSZ encoders), and on the lab's recorded mainnet chain, two
//! updaters on one in-process LEZ chain whose sources serve different copies of each update.
#![cfg(feature = "harness")]

use std::fs;

use ferrata_core::{witness::CompactUpdate, Root};
use ferrata_lez_wire::Config;
use ferrata_updater::{
    chain::{
        committee_key, decode_finality_update, decode_update, encode_finality_update, encode_update, preset, root,
        served_update_of_open,
    },
    config::{Fees, Policy},
    ledger::HarnessLedger,
    source::{lab_record, load_entries, Clock, Entry, Recorded, Source},
    updater::{Instance, Plan, Step, Updater},
};
use lee::PrivateKey;

fn key(b: u8) -> PrivateKey {
    PrivateKey::try_new([b; 32]).unwrap()
}

fn no_sleep(_: u64) {}

/// Every recorded update, finality and period, of both recorded chains: the SSZ encoders give back the bytes the source
/// served (SSZ has one encoding per value), and the compact encoding an `Open` carries gives back the served bytes and
/// the kind of update (a period update only with the full committee its summary names).
#[test]
fn an_open_gives_back_the_bytes_a_source_served() {
    for name in ["mainnet", "sepolia"] {
        let config = preset(name, 2).unwrap();
        let entries = load_entries(&lab_record(name).join("updates"), &config).unwrap();
        let (mut finality, mut period) = (0, 0);
        for e in &entries {
            let decoded =
                if e.period_update { decode_update(&e.bytes, &config) } else { decode_finality_update(&e.bytes, &config) };
            let (u, layout) = decoded.unwrap();
            let again = if e.period_update { encode_update(&u, layout) } else { encode_finality_update(&u, layout) };
            assert_eq!(again, e.bytes, "{name}: entry {}: the encoder gives back the served bytes", e.seq);
            let compact = CompactUpdate::of(&u).encode(layout);
            let next = u.next_sync_committee.clone();
            let committee = |r: &Root| (committee_key(&next) == *r).then(|| next.clone());
            let (is_period, raw, back) = served_update_of_open(&compact, &config, committee).unwrap();
            assert_eq!((is_period, &raw, &back), (e.period_update, &e.bytes, &u), "{name}: entry {}", e.seq);
            if e.period_update {
                assert!(served_update_of_open(&compact, &config, |_| None).is_none(), "a period update needs its committee");
                period += 1;
            } else {
                finality += 1;
            }
        }
        println!("{name}: {finality} finality updates and {period} period updates given back byte for byte");
        assert!(finality > 0 && period > 0);
    }
}

/// A copy of the update `bytes` (a period update when `period`): its `signature_slot` moved by one slot, later unless
/// that leaves its sync-committee period, then earlier; the same finalized header, other bytes, another job id.
fn copy_of(bytes: &[u8], period: bool, config: &Config) -> Vec<u8> {
    let (mut u, layout) = if period { decode_update(bytes, config) } else { decode_finality_update(bytes, config) }.unwrap();
    let spec = config.spec();
    let s = u.signature_slot;
    let same = |t: u64| spec.period_at_slot(t) == spec.period_at_slot(s) && t > u.attested_header.beacon.slot;
    u.signature_slot = if same(s + 1) { s + 1 } else { s - 1 };
    assert!(same(u.signature_slot));
    if period {
        encode_update(&u, layout)
    } else {
        encode_finality_update(&u, layout)
    }
}

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

    /// An updater (window 1: no wait; join or wait at its default) whose one source serves the recorded updates, or
    /// (`copies`) a copy of each.
    fn updater(&self, label: &str, payer: PrivateKey, copies: bool) -> Updater {
        let mut source = Recorded::open("a", &self.dir, &self.config, self.clock.clone()).unwrap();
        if copies {
            for e in &mut source.entries {
                e.bytes = copy_of(&e.bytes, e.period_update, &self.config);
            }
        }
        let inst = Instance::new("mainnet", HarnessLedger::program(), self.config.clone(), self.checkpoint);
        let policy = Policy { defer_window: 1, retry_base_ms: 0, ..Policy::default() };
        let sources: Vec<Box<dyn Source>> = vec![Box::new(source)];
        let mut u = Updater::new(label, inst, payer, Fees::default(), policy, sources, None).unwrap();
        u.sleep = no_sleep;
        u
    }

    /// The first clock `c` after which entry `c` (served from clock `c + 1`) is a finality update finalizing a later slot
    /// than every entry before it.
    fn new_finality_update(&self) -> usize {
        let e = &self.entries;
        let head = |upto: usize| e[..upto].iter().map(|x| x.finalized_slot).max().unwrap();
        (3..e.len() - 1).find(|&c| !e[c].period_update && e[c].finalized_slot > head(c)).expect("a new finality update")
    }
}

fn catch_up(fx: &Fx, a: &mut Updater, l: &mut HarnessLedger, c: usize) {
    fx.clock.set(c);
    for _ in 0..400 {
        if matches!(a.tick(l).unwrap(), Plan::UpToDate) {
            return;
        }
    }
    panic!("no catch-up to clock {c}");
}

fn show(p: &Plan) -> String {
    match p {
        Plan::Send(x) => format!("send {} of 0x{}", x.step.name(), &hex::encode(x.job_id)[..8]),
        p => format!("{p:?}"),
    }
}

/// `a`, whose source serves the recorded updates, opens U's job. `b`, whose source serves only a copy of U (never the
/// bytes `a` was served), learns U from `a`'s `Open` on chain, finds U's job open on the store, recognizes it as the same
/// update for the store (the same finalized header, no adoption), and joins it: its first step is that job's `Miller`,
/// not an `Open` of its copy. Without the chain's transactions (`serve_messages = false`, the updater before), `b`
/// opens its copy's job beside `a`'s.
#[test]
fn a_job_opened_for_a_copy_never_served_is_found_and_joined() {
    for messages in [true, false] {
        let fx = Fx::load();
        let c = fx.new_finality_update();
        let (ka, kb) = (key(0x81), key(0x82));
        let mut l = HarnessLedger::new(&[ka.clone(), kb.clone()]);
        l.serve_messages = messages;
        let mut a = fx.updater("a", ka, false);
        let mut b = fx.updater("b", kb, true);
        catch_up(&fx, &mut a, &mut l, c);
        fx.clock.set(c + 1);
        let ja = match a.tick(&mut l).unwrap() {
            Plan::Send(x) if x.step == Step::Open => x.job_id,
            p => panic!("a: {}", show(&p)),
        };
        let first = b.tick(&mut l).unwrap();
        println!("messages {messages}: b's first round with U's job open: {}", show(&first));
        if messages {
            assert!(matches!(&first, Plan::Send(x) if x.step == Step::Miller && x.job_id == ja), "{}", show(&first));
            assert_eq!(b.metrics.updates_from_chain, 1, "U learned from a's Open");
            assert_eq!(b.metrics.jobs_joined, 1);
            for _ in 0..40 {
                let pa = a.tick(&mut l).unwrap();
                let pb = b.tick(&mut l).unwrap();
                if matches!((&pa, &pb), (Plan::UpToDate, Plan::UpToDate)) {
                    break;
                }
            }
            assert_eq!(a.inst.store(&l).unwrap().unwrap().finalized.slot, fx.entries[c].finalized_slot, "U applied");
            assert_eq!(b.metrics.count("refused"), 0);
            assert!(b.metrics.tx.keys().all(|(step, _)| step != "Open"), "b paid no Open: {:?}", b.metrics.tx);
        } else {
            assert!(matches!(&first, Plan::Send(x) if x.step == Step::Open && x.job_id != ja), "{}", show(&first));
            assert_eq!(b.metrics.updates_from_chain, 0);
        }
    }
}

/// `a` and `b` (served different copies of U) open their copies' jobs in one block: neither could read the other's.
/// Then each learns the other's copy from the chain; the job whose update ranks higher (as many steps; the earlier
/// signature slot) leads, both continue it, and the other is abandoned with its `Open` alone. Without the chain's
/// transactions each carries its own job on, and the one that does not finish is abandoned with more than its `Open`.
#[test]
fn of_two_copies_opened_in_one_block_one_is_finished_and_the_other_left_at_its_open() {
    let mut abandoned_steps = Vec::new();
    for messages in [true, false] {
        let fx = Fx::load();
        let c = fx.new_finality_update();
        let (ka, kb) = (key(0x83), key(0x84));
        let mut l = HarnessLedger::new(&[ka.clone(), kb.clone()]);
        l.serve_messages = messages;
        let mut a = fx.updater("a", ka, false);
        let mut b = fx.updater("b", kb, true);
        catch_up(&fx, &mut a, &mut l, c);
        fx.clock.set(c + 1);
        // Both plan against the same store (one block), then both are settled.
        let (Plan::Send(xa), Plan::Send(xb)) = (a.plan(&l).unwrap(), b.plan(&l).unwrap()) else { panic!("both open") };
        assert!(xa.step == Step::Open && xb.step == Step::Open && xa.job_id != xb.job_id);
        assert!(a.submit(&mut l, &xa, false).unwrap().unwrap().applied);
        assert!(b.submit(&mut l, &xb, false).unwrap().unwrap().applied);
        let mut sent: Vec<(Root, Step, bool)> = vec![(xa.job_id, Step::Open, true), (xb.job_id, Step::Open, true)];
        for _ in 0..60 {
            let mut idle = true;
            for u in [&mut a, &mut b] {
                let n = l.log.len();
                let p = u.tick(&mut l).unwrap();
                if let (Plan::Send(x), true) = (&p, l.log.len() > n) {
                    sent.push((x.job_id, x.step, l.log.last().unwrap().2.applied));
                }
                idle &= matches!(p, Plan::UpToDate);
            }
            if idle {
                break;
            }
        }
        assert_eq!(a.inst.store(&l).unwrap().unwrap().finalized.slot, fx.entries[c].finalized_slot, "U applied");
        let applied = |j: &Root| sent.iter().filter(|s| s.0 == *j && s.2).count();
        let finished = |j: &Root| sent.iter().any(|s| s.0 == *j && s.1 == Step::Finish && s.2);
        assert!(finished(&xa.job_id) != finished(&xb.job_id), "exactly one job finished");
        let other = if finished(&xa.job_id) { xb.job_id } else { xa.job_id };
        println!("messages {messages}: the job not finished has {} applied step(s)", applied(&other));
        if messages {
            // The original's signature slot is the earlier: its job leads at as many steps.
            assert!(finished(&xa.job_id), "a's job (the earlier signature slot) finished");
            assert!(b.metrics.updates_from_chain >= 1 && a.metrics.updates_from_chain >= 1);
        }
        abandoned_steps.push(applied(&other));
    }
    assert_eq!(abandoned_steps[0], 1, "learning each other's copies, the job left has its Open alone");
    assert!(abandoned_steps[1] > 1, "without it, the job left has more: {}", abandoned_steps[1]);
}
