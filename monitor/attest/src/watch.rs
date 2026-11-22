//! The daemon: poll, assess, alert on changes, keep a status file and the headers seen, stop cleanly.
//!
//! An alert is emitted when it is first raised, again every `repeat_s` while it holds, again at the next poll while
//! the alert command fails to take it, and a `recovered` alert (info) when a poll that can judge it no longer raises
//! it. An outage is not a recovery: a poll that cannot read the instance judges nothing, one with too few providers
//! for a verdict judges no divergence, and one without any provider's finalized slot judges no stall, so what was
//! raised is held through it.
//!
//! Every finalized header the instance shows is remembered with the providers' verdict (`state_file`), and judged
//! again, whether or not the instance has moved on, until the verdict is settled: confirmed or contested with no
//! provider behind its slot. So a forged header the providers could not judge when the instance showed it (ahead of
//! their finality) is judged once they have finalized its slot. A header found diverged, or too new when shown (no
//! honest instance can hold it), stays raised (`instance-diverged`, `instance-ahead`, one alert per header) whether
//! the instance moves on or the providers drop out; a diverged one, or one ahead of every provider, clears only when a
//! later poll confirms that header (`--min-agreeing` agree, none disagrees). The loop ends after `max_polls`, or on
//! the stop flag (SIGTERM or SIGINT in the binary) after the poll in hand.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::alert::{Alert, Level, Sink};
use crate::source::{query_provider, read_state, view_at, Provider, StateSource, Target};
use crate::{assess, judge, parse_root, Ahead, Assessment, Divergence, Policy, ProviderView};
use ferrata_lez_wire::InstanceState;

/// Earlier headers judged again per poll, at most: never-decided ones first, then the oldest.
pub const RECHECK_PER_POLL: usize = 8;

/// The chain's clock: `start_s` at the start, then the wall clock's progress (UNIX seconds).
#[derive(Clone, Copy, Debug)]
pub struct Clock {
    start_s: u64,
    started: Instant,
}

impl Clock {
    pub fn at(start_s: u64) -> Self {
        Self { start_s, started: Instant::now() }
    }

    pub fn system() -> Self {
        Self::at(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default())
    }

    pub fn now(&self) -> u64 {
        self.start_s + self.started.elapsed().as_secs()
    }
}

pub struct Watch {
    pub state: StateSource,
    pub target: Option<Target>,
    pub providers: Vec<Provider>,
    pub policy: Policy,
    pub interval_s: u64,
    pub repeat_s: u64,
    pub max_polls: Option<u64>,
    pub timeout_s: u64,
    pub clock: Clock,
    pub sink: Sink,
    pub status_file: Option<PathBuf>,
    pub state_file: Option<PathBuf>,
}

/// A finalized header the instance has shown, with the providers' verdict.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Seen {
    pub root: String,
    pub first_seen_s: u64,
    /// The latest verdict: `unconfirmed` never replaces one, and `diverged` is replaced only by `confirmed`.
    pub divergence: Divergence,
    /// Confirmed or contested with no provider behind the slot: not judged again. Absent from a state file written
    /// before this field: read as settled when its verdict is confirmed or contested (an unconfirmed or diverged header
    /// is judged again; a confirmed one, which an older provider may no longer serve, is not).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settled: Option<bool>,
    /// `too-new` when the instance showed it (it stays raised), or ahead of every provider (until confirmed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ahead: Option<Ahead>,
}

impl Seen {
    fn new(root: String, now: u64) -> Self {
        Self { root, first_seen_s: now, divergence: Divergence::Unconfirmed, settled: Some(false), ahead: None }
    }

    pub fn is_settled(&self) -> bool {
        self.settled.unwrap_or(matches!(self.divergence, Divergence::Confirmed | Divergence::Contested))
    }

    /// Records a verdict reached with `behind` providers that had not finalized the slot.
    fn record(&mut self, d: Divergence, behind: usize) {
        if d == Divergence::Unconfirmed {
            return;
        }
        if !(self.divergence == Divergence::Diverged && d == Divergence::Contested) {
            self.divergence = d;
        }
        self.settled = Some(matches!(self.divergence, Divergence::Confirmed | Divergence::Contested) && behind == 0);
        if self.divergence == Divergence::Confirmed && self.ahead == Some(Ahead::AheadOfProviders) {
            self.ahead = None;
        }
    }

    fn note_ahead(&mut self, a: Option<Ahead>) {
        match a {
            Some(Ahead::TooNew) => self.ahead = Some(Ahead::TooNew),
            Some(Ahead::AheadOfProviders) if self.ahead.is_none() && self.divergence != Divergence::Confirmed => self.ahead = a,
            _ => {}
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Summary {
    pub polls: u64,
    pub emitted: Vec<Alert>,
    pub seen: BTreeMap<u64, Seen>,
    pub last: Option<Assessment>,
}

/// One poll: the instance's state, then each provider at its finalized slot, then the decision at `now_s`.
pub fn poll(state: &StateSource, target: Option<&Target>, providers: &[Provider], policy: &Policy, now_s: u64, timeout_s: u64) -> Result<Assessment> {
    poll_views(state, target, providers, policy, now_s, timeout_s).map(|(_, _, a)| a)
}

fn poll_views(
    state: &StateSource,
    target: Option<&Target>,
    providers: &[Provider],
    policy: &Policy,
    now_s: u64,
    timeout_s: u64,
) -> Result<(InstanceState, Vec<ProviderView>, Assessment)> {
    let s = read_state(state, target, timeout_s)?;
    let slot = ferrata_core::witness::CompactStore::decode(&s.store)
        .map_err(|e| anyhow::anyhow!("the instance's store does not decode: {e:?}"))?
        .finalized
        .slot;
    let views: Vec<_> = providers.iter().map(|p| query_provider(p, slot, timeout_s)).collect();
    let a = assess(&s, &views, now_s, policy)?;
    Ok((s, views, a))
}

/// Judges again the earlier headers in `seen` that some provider can judge now (its finalized slot at or past theirs).
fn recheck(seen: &mut BTreeMap<u64, Seen>, current: u64, s: &InstanceState, providers: &[Provider], views: &[ProviderView], w: &Watch) {
    let Some(highest) = views.iter().filter(|v| v.error.is_none()).filter_map(|v| v.finalized_slot).max() else { return };
    let mut due: Vec<(bool, u64)> = seen
        .iter()
        .filter(|(slot, e)| **slot != current && **slot <= highest && !e.is_settled())
        .map(|(slot, e)| (e.divergence != Divergence::Unconfirmed, *slot))
        .collect();
    due.sort_unstable();
    for (_, slot) in due.into_iter().take(RECHECK_PER_POLL) {
        let Ok(root) = parse_root(&seen[&slot].root) else { continue };
        let at: Vec<ProviderView> = providers.iter().zip(views).map(|(p, v)| view_at(p, v, slot, w.timeout_s)).collect();
        let j = judge(&s.config, slot, &root, &at, &w.policy);
        if let Some(e) = seen.get_mut(&slot) {
            e.record(j.divergence, j.behind);
        }
    }
}

/// The alerts every header on record keeps raised: diverged (until confirmed), too new, ahead of every provider.
fn on_record(seen: &BTreeMap<u64, Seen>, instance: &str, current: u64) -> Vec<Alert> {
    let mut out = Vec::new();
    for (slot, e) in seen {
        if e.divergence == Divergence::Diverged {
            out.push(
                Alert::new(Level::Critical, "instance-diverged", "a finalized header the instance showed is not the canonical block at its slot: freeze what consumes this instance")
                    .with("instance", instance)
                    .with("slot", *slot)
                    .with("instance_root", &e.root)
                    .with("current", *slot == current)
                    .with("first_seen_s", e.first_seen_s),
            );
        }
        if let Some(why) = e.ahead {
            out.push(
                Alert::new(Level::Critical, "instance-ahead", "a finalized header the instance showed is ahead of the chain's finality: freeze what consumes this instance")
                    .with("instance", instance)
                    .with("why", why)
                    .with("slot", *slot)
                    .with("instance_root", &e.root)
                    .with("current", *slot == current)
                    .with("first_seen_s", e.first_seen_s),
            );
        }
    }
    out
}

/// An alert raised and not cleared: the latest of it, when it was last emitted, and whether the alert command took it.
struct Raised {
    alert: Alert,
    last: u64,
    delivered: bool,
}

/// The alert kinds a poll could not judge, whose raised alerts it holds.
enum Unjudged {
    All,
    Kinds(Vec<&'static str>),
}

impl Unjudged {
    fn holds(&self, kind: &str) -> bool {
        match self {
            Unjudged::All => true,
            Unjudged::Kinds(k) => k.contains(&kind),
        }
    }
}

/// An alert's identity: its kind and provider; a header's alarms, its kind and slot (one alert per header).
fn key(a: &Alert) -> String {
    match a.kind.as_str() {
        "instance-diverged" | "instance-ahead" => format!("{}@{}", a.kind, a.fields.get("slot").and_then(|v| v.as_u64()).unwrap_or_default()),
        _ => format!("{}:{}", a.kind, a.fields.get("provider").and_then(|v| v.as_str()).unwrap_or("")),
    }
}

fn write_atomic(path: &PathBuf, text: &str) {
    let tmp = path.with_extension("tmp");
    if let Err(e) = std::fs::write(&tmp, text).and_then(|()| std::fs::rename(&tmp, path)) {
        eprintln!("attest-monitor: {}: {e}", path.display());
    }
}

pub fn run(w: &Watch, stop: Arc<AtomicBool>) -> Summary {
    let mut summary = Summary::default();
    if let Some(p) = &w.state_file {
        if let Ok(text) = std::fs::read_to_string(p) {
            summary.seen = serde_json::from_str(&text).unwrap_or_default();
        }
    }
    let mut active: BTreeMap<String, Raised> = BTreeMap::new();
    let start = Alert::new(Level::Info, "started", "the attestation monitor started")
        .with("providers", w.providers.iter().map(|p| p.name.clone()).collect::<Vec<_>>())
        .with("interval_s", w.interval_s);
    w.sink.emit(&start);
    summary.emitted.push(start);
    loop {
        let now = w.clock.now();
        let (alerts, unjudged) = match poll_views(&w.state, w.target.as_ref(), &w.providers, &w.policy, now, w.timeout_s) {
            Ok((s, views, a)) => {
                let entry = summary.seen.entry(a.finalized_slot).or_insert_with(|| Seen::new(a.finalized_root.clone(), now));
                if entry.root != a.finalized_root {
                    // Another header at a slot on record (the program never replaces a finalized header at its slot: the
                    // source now shows another instance state): a new entry.
                    *entry = Seen::new(a.finalized_root.clone(), now);
                }
                entry.record(a.divergence, a.providers_behind);
                entry.note_ahead(a.ahead);
                recheck(&mut summary.seen, a.finalized_slot, &s, &w.providers, &views, w);
                let mut alerts = a.alerts.clone();
                let raised: Vec<String> = alerts.iter().map(key).collect();
                alerts.extend(on_record(&summary.seen, &a.instance_id, a.finalized_slot).into_iter().filter(|r| !raised.contains(&key(r))));
                let mut unjudged = Vec::new();
                if a.divergence == Divergence::Unconfirmed {
                    unjudged.push("providers-disagree");
                }
                if a.lag_s.is_none() {
                    unjudged.push("instance-stalled");
                }
                if let Some(p) = &w.status_file {
                    write_atomic(p, &serde_json::to_string_pretty(&a).unwrap_or_default());
                }
                summary.last = Some(a);
                (alerts, Unjudged::Kinds(unjudged))
            }
            Err(e) => (
                vec![Alert::new(Level::Warn, "instance-unreadable", "the instance's state could not be read").with("error", format!("{e:#}"))],
                Unjudged::All,
            ),
        };
        let mut current: BTreeMap<String, Alert> = alerts.into_iter().map(|a| (key(&a), a)).collect();
        // Held: raised before, and not judged by this poll.
        for (k, r) in &active {
            if !current.contains_key(k) && unjudged.holds(&r.alert.kind) {
                current.insert(k.clone(), r.alert.clone());
            }
        }
        for (k, a) in &current {
            let due = match active.get(k) {
                None => true,
                Some(r) => !r.delivered || now.saturating_sub(r.last) >= w.repeat_s,
            };
            if due {
                let delivered = w.sink.emit(a);
                summary.emitted.push(a.clone());
                active.insert(k.clone(), Raised { alert: a.clone(), last: now, delivered });
            } else if let Some(r) = active.get_mut(k) {
                r.alert = a.clone();
            }
        }
        let cleared: Vec<String> = active.keys().filter(|k| !current.contains_key(*k)).cloned().collect();
        for k in cleared {
            let was = active.remove(&k).unwrap().alert;
            let a = Alert::new(Level::Info, "recovered", "an alert cleared").with("cleared", &was.kind).with("was", &was);
            w.sink.emit(&a);
            summary.emitted.push(a);
        }
        if let Some(p) = &w.state_file {
            write_atomic(p, &serde_json::to_string_pretty(&summary.seen).unwrap_or_default());
        }
        summary.polls += 1;
        if w.max_polls.is_some_and(|m| summary.polls >= m) || stop.load(Ordering::SeqCst) {
            break;
        }
        let until = Instant::now() + Duration::from_secs(w.interval_s);
        while Instant::now() < until && !stop.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(100));
        }
        if stop.load(Ordering::SeqCst) {
            break;
        }
    }
    let end = Alert::new(Level::Info, "stopped", "the attestation monitor stopped").with("polls", summary.polls);
    w.sink.emit(&end);
    summary.emitted.push(end);
    summary
}
