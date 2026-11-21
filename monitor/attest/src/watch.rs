//! The daemon: poll, assess, alert on changes, keep a status file and the headers seen, stop cleanly.
//!
//! An alert is emitted when it is first raised, again every `repeat_s` while it holds, and a `recovered` alert
//! (info) when it clears. Each finalized header the instance shows is remembered with the verdict the providers gave
//! it (`state_file`), so a header seen once and replaced later still has its verdict on record. The loop ends after
//! `max_polls`, or on the stop flag (SIGTERM or SIGINT in the binary) after the poll in hand.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::alert::{Alert, Level, Sink};
use crate::source::{query_provider, read_state, Provider, StateSource, Target};
use crate::{assess, Assessment, Divergence, Policy};

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
    pub divergence: Divergence,
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
    let s = read_state(state, target, timeout_s)?;
    let slot = ferrata_core::witness::CompactStore::decode(&s.store)
        .map_err(|e| anyhow::anyhow!("the instance's store does not decode: {e:?}"))?
        .finalized
        .slot;
    let views: Vec<_> = providers.iter().map(|p| query_provider(p, slot, timeout_s)).collect();
    assess(&s, &views, now_s, policy)
}

fn key(a: &Alert) -> String {
    let who = a.fields.get("provider").and_then(|v| v.as_str()).unwrap_or("");
    format!("{}:{who}", a.kind)
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
    let mut active: BTreeMap<String, (Alert, u64)> = BTreeMap::new();
    let start = Alert::new(Level::Info, "started", "the attestation monitor started")
        .with("providers", w.providers.iter().map(|p| p.name.clone()).collect::<Vec<_>>())
        .with("interval_s", w.interval_s);
    w.sink.emit(&start);
    summary.emitted.push(start);
    loop {
        let now = w.clock.now();
        let alerts = match poll(&w.state, w.target.as_ref(), &w.providers, &w.policy, now, w.timeout_s) {
            Ok(a) => {
                let entry = summary.seen.entry(a.finalized_slot).or_insert(Seen {
                    root: a.finalized_root.clone(),
                    first_seen_s: now,
                    divergence: a.divergence,
                });
                // A verdict reached later (providers that answer again) replaces an open one.
                if entry.divergence != a.divergence && a.divergence != Divergence::Unconfirmed {
                    entry.divergence = a.divergence;
                }
                let alerts = a.alerts.clone();
                if let Some(p) = &w.status_file {
                    write_atomic(p, &serde_json::to_string_pretty(&a).unwrap_or_default());
                }
                summary.last = Some(a);
                alerts
            }
            Err(e) => vec![Alert::new(Level::Warn, "instance-unreadable", "the instance's state could not be read").with("error", format!("{e:#}"))],
        };
        let current: BTreeMap<String, Alert> = alerts.into_iter().map(|a| (key(&a), a)).collect();
        for (k, a) in &current {
            let due = match active.get(k) {
                None => true,
                Some((_, last)) => now.saturating_sub(*last) >= w.repeat_s,
            };
            if due {
                w.sink.emit(a);
                summary.emitted.push(a.clone());
                active.insert(k.clone(), (a.clone(), now));
            }
        }
        let cleared: Vec<String> = active.keys().filter(|k| !current.contains_key(*k)).cloned().collect();
        for k in cleared {
            let (was, _) = active.remove(&k).unwrap();
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
