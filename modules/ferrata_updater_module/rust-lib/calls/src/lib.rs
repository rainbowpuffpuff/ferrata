//! The methods of `ferrata_updater_module` as plain functions over JSON strings, so that they run and
//! are tested without the module scaffold. The work is `ferrata-updater`'s (`lez/updater`, staged into
//! the module by `scripts/stage-updater.sh`): the same configuration file, the same set-up and the same
//! rounds as the binary (`ferrata_updater::daemon`).
//!
//! The chain is the binary's `ledger = "harness"`: LEZ v0.4.0-rc1's state machine in process, with
//! LEZ's fee program and settlement and the attestation program as the guests were built (the chain the
//! updater's tests run on; `RISC0_DEV_MODE=1` in the module's environment, as for those tests). It lives
//! in the daemon's thread: `start` makes it, `stop` ends it. `ledger = "sequencer"` is refused, as by
//! the binary (that backend is not built).

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context as _, Result};
use ferrata_updater::config::File;
use ferrata_updater::daemon::Daemon as Rounds;
use ferrata_updater::serve::{serve, Snapshot};
use serde_json::{json, Value};

/// What `scripts/stage-updater.sh` recorded: the updater's sources, LEZ, the guests' ids.
const STAGED: &str = include_str!("../../vendor/STAGED.json");

pub const METHODS: &[&str] = &["version", "check_config", "run", "start", "status", "metrics", "stop"];

fn image_id_hex(id: [u32; 8]) -> String {
    let mut s = String::from("0x");
    for w in id {
        for b in w.to_le_bytes() {
            s.push_str(&format!("{b:02x}"));
        }
    }
    s
}

/// `version()`: the module's version, the updater it was built from, and the attestation program it
/// carries: the image id declared for it and the one computed from its bytes.
pub fn version() -> String {
    let computed = risc0_binfmt::compute_image_id(methods::FERRATA_LEZ_ELF)
        .map(|d| format!("0x{}", hex_of(d.as_bytes())))
        .unwrap_or_else(|e| format!("error: {e}"));
    json!({
        "module": "ferrata_updater_module",
        "version": env!("CARGO_PKG_VERSION"),
        "updater": serde_json::from_str::<Value>(STAGED).unwrap_or(Value::Null),
        "program": {
            "name": "ferrata_lez",
            "declared_id": image_id_hex(methods::FERRATA_LEZ_ID),
            "computed_id": computed,
            "bytes": methods::FERRATA_LEZ_ELF.len(),
        },
        "ledgers": ["harness"],
        "dev_mode": std::env::var("RISC0_DEV_MODE").ok(),
        "methods": METHODS,
    })
    .to_string()
}

fn hex_of(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn reply(r: Result<Value>) -> String {
    r.unwrap_or_else(|e| json!({ "verdict": "invalid_request", "error": format!("{e:#}") })).to_string()
}

fn parse(request: &str) -> Result<Value> {
    if request.trim().is_empty() {
        return Ok(json!({}));
    }
    let v: Value = serde_json::from_str(request).context("the request is not JSON")?;
    if !v.is_object() {
        bail!("the request is not a JSON object");
    }
    Ok(v)
}

fn config_path(r: &Value) -> Result<PathBuf> {
    Ok(PathBuf::from(r.get("config_file").and_then(Value::as_str).context("config_file (a path) is required")?))
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default()
}

/// `check_config(request)`: `{config_file}`: the file as the daemon reads it (refused as the binary
/// refuses it: an unknown field, preset or ledger, a bad checkpoint).
pub fn check_config(request: &str) -> String {
    reply((|| {
        let r = parse(request)?;
        let f = File::load(&config_path(&r)?)?;
        Ok(json!({
            "verdict": "ok", "daemon": f.daemon.name, "ledger": f.daemon.ledger, "poll_secs": f.daemon.poll_secs,
            "listen": f.daemon.listen,
            "chains": f.chains.iter().map(|c| json!({ "name": c.name, "preset": c.preset, "checkpoint": c.checkpoint,
                "sources": c.sources.iter().map(|s| json!({ "name": s.name, "url": s.url, "dir": s.dir })).collect::<Vec<_>>() }))
                .collect::<Vec<_>>(),
        }))
    })())
}

fn snapshot_of(d: &Rounds) -> Value {
    let (health, healthy) = d.health();
    json!({ "round": d.round, "healthy": healthy, "up_to_date": d.up_to_date(), "health": health,
            "instances": d.instances() })
}

/// `run(request)`: `{config_file, rounds, until_up_to_date?, quiet?}`: the daemon for at most
/// `rounds` rounds (the binary's `--rounds`, its `poll_secs` between them), or until every instance is
/// up to date; then its health, the instances as the chain holds them, and its metrics.
pub fn run(request: &str) -> String {
    reply((|| {
        let r = parse(request)?;
        let path = config_path(&r)?;
        let rounds = r.get("rounds").and_then(Value::as_u64).context("rounds (a number) is required")?;
        let until = r.get("until_up_to_date").and_then(Value::as_bool).unwrap_or(false);
        let quiet = r.get("quiet").and_then(Value::as_bool).unwrap_or(true);
        let t0 = Instant::now();
        let mut d = match Rounds::open(&path, !quiet) {
            Ok(d) => d,
            Err(e) => return Ok(json!({ "verdict": "error", "error": format!("{e:#}") })),
        };
        let mut notes = VecDeque::new();
        while d.round < rounds {
            for n in d.round() {
                push(&mut notes, n);
            }
            if until && d.up_to_date() {
                break;
            }
            if d.file.daemon.poll_secs > 0 && d.round < rounds {
                std::thread::sleep(Duration::from_secs(d.file.daemon.poll_secs));
            }
        }
        let mut v = snapshot_of(&d);
        v["verdict"] = json!("ok");
        v["seconds"] = json!(t0.elapsed().as_secs_f64());
        v["notes"] = json!(notes);
        v["metrics"] = json!(d.metrics());
        Ok(v)
    })())
}

fn push(notes: &mut VecDeque<String>, n: String) {
    notes.push_back(n);
    while notes.len() > 50 {
        notes.pop_front();
    }
}

// ---- the updater as a daemon -------------------------------------------------------------------------

#[derive(Default)]
struct Shared {
    running: bool,
    started_at: u64,
    stopped_at: Option<u64>,
    error: Option<String>,
    snapshot: Value,
    metrics: String,
    notes: VecDeque<String>,
}

/// One `/health` and `/metrics` listener per address for the module's process: a daemon started again
/// on the same address serves through the listener of the first.
static LISTENERS: Mutex<Vec<(String, Snapshot, String)>> = Mutex::new(Vec::new());

fn listener(addr: &str) -> Result<(Snapshot, String)> {
    let mut l = LISTENERS.lock().unwrap_or_else(|p| p.into_inner());
    if let Some((_, s, bound)) = l.iter().find(|(a, _, _)| a == addr) {
        return Ok((s.clone(), bound.clone()));
    }
    let s = Snapshot::default();
    let bound = serve(addr, s.clone())?.to_string();
    l.push((addr.to_owned(), s.clone(), bound.clone()));
    Ok((s, bound))
}

/// The updater's rounds in a thread of the module's process; [`Daemon::stop`] ends them between rounds.
pub struct Daemon {
    stop: Arc<AtomicBool>,
    shared: Arc<Mutex<Shared>>,
    handle: Option<JoinHandle<()>>,
    config: Value,
}

impl Daemon {
    /// `start(request)`: `{config_file, rounds?, quiet?}`: the daemon as the binary runs it (its
    /// `poll_secs` between rounds; `/health` and `/metrics` on `daemon.listen` when set), until `stop`
    /// or `rounds`. The reply comes once the set-up is done (or failed: an error then, as the binary
    /// exits 1).
    pub fn start(request: &str) -> std::result::Result<Self, String> {
        let r = parse(request).map_err(|e| reply(Err(e)))?;
        let path = config_path(&r).map_err(|e| reply(Err(e)))?;
        let rounds = r.get("rounds").and_then(Value::as_u64);
        let quiet = r.get("quiet").and_then(Value::as_bool).unwrap_or(true);
        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Mutex::new(Shared { running: true, started_at: now_secs(), ..Shared::default() }));
        let (s2, sh2, p2) = (stop.clone(), shared.clone(), path.clone());
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<std::result::Result<Option<String>, String>>();
        let handle = std::thread::Builder::new()
            .name("ferrata-updater".into())
            .spawn(move || run_loop(&p2, rounds, quiet, s2, sh2, ready_tx))
            .map_err(|e| reply(Err(anyhow::anyhow!("spawning the updater's thread: {e}"))))?;
        let served = match ready_rx.recv() {
            Ok(Ok(served)) => served,
            Ok(Err(e)) => {
                let _ = handle.join();
                return Err(json!({ "verdict": "error", "error": e }).to_string());
            }
            Err(_) => {
                let _ = handle.join();
                return Err(json!({ "verdict": "error", "error": "the updater's thread ended during its set-up" }).to_string());
            }
        };
        Ok(Self {
            stop,
            shared,
            handle: Some(handle),
            config: json!({ "config_file": path, "rounds": rounds, "served": served }),
        })
    }

    pub fn running(&self) -> bool {
        self.shared.lock().map(|s| s.running).unwrap_or(false)
    }

    /// `status()`: the rounds done, each instance's health and state on the chain, the last notes.
    pub fn status(&self) -> String {
        let s = self.shared.lock().unwrap_or_else(|p| p.into_inner());
        json!({
            "verdict": if s.error.is_some() { "error" } else { "ok" }, "running": s.running,
            "started_at": s.started_at, "stopped_at": s.stopped_at, "error": s.error,
            "snapshot": s.snapshot, "notes": s.notes.iter().collect::<Vec<_>>(), "config": self.config,
        })
        .to_string()
    }

    /// `metrics()`: `/metrics` as the last round left it (Prometheus text, in `metrics`).
    pub fn metrics(&self) -> String {
        let s = self.shared.lock().unwrap_or_else(|p| p.into_inner());
        json!({ "verdict": "ok", "round": s.snapshot["round"], "metrics": s.metrics }).to_string()
    }

    /// `stop()`: the round in hand finishes, then the daemon ends (the in-process chain with it).
    pub fn stop(&mut self) -> String {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        self.status()
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn run_loop(
    path: &Path,
    rounds: Option<u64>,
    quiet: bool,
    stop: Arc<AtomicBool>,
    shared: Arc<Mutex<Shared>>,
    ready: std::sync::mpsc::Sender<std::result::Result<Option<String>, String>>,
) {
    let set = |f: &mut dyn FnMut(&mut Shared)| {
        let mut s = shared.lock().unwrap_or_else(|p| p.into_inner());
        f(&mut s);
    };
    let opened = Rounds::open(path, !quiet).and_then(|d| {
        let served = match &d.file.daemon.listen {
            Some(addr) => Some(listener(addr)?),
            None => None,
        };
        Ok((d, served))
    });
    let (mut d, served) = match opened {
        Ok(x) => x,
        Err(e) => {
            let e = format!("{e:#}");
            set(&mut |s| {
                s.running = false;
                s.stopped_at = Some(now_secs());
                s.error = Some(e.clone());
            });
            let _ = ready.send(Err(e));
            return;
        }
    };
    let _ = ready.send(Ok(served.as_ref().map(|(_, b)| b.clone())));
    let mut last_snapshot = snapshot_of(&d);
    set(&mut |s| s.snapshot = last_snapshot.clone());
    while !stop.load(Ordering::SeqCst) && rounds.is_none_or(|n| d.round < n) {
        let notes = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| d.round()))
            .unwrap_or_else(|_| vec!["the round panicked".to_string()]);
        last_snapshot = snapshot_of(&d);
        let metrics = d.metrics();
        if let Some((snap, _)) = &served {
            let (health, healthy) = d.health();
            snap.set(metrics.clone(), serde_json::to_string_pretty(&health).unwrap_or_default() + "\n", healthy);
        }
        set(&mut |s| {
            s.snapshot = last_snapshot.clone();
            s.metrics = metrics.clone();
            for n in &notes {
                push(&mut s.notes, n.clone());
            }
        });
        let poll = Duration::from_secs(d.file.daemon.poll_secs);
        let mut slept = Duration::ZERO;
        while slept < poll && !stop.load(Ordering::SeqCst) {
            let step = Duration::from_millis(100).min(poll - slept);
            std::thread::sleep(step);
            slept += step;
        }
    }
    set(&mut |s| {
        s.running = false;
        s.stopped_at = Some(now_secs());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(f: fn(&str) -> String, r: Value) -> Value {
        serde_json::from_str(&f(&r.to_string())).unwrap()
    }

    #[test]
    fn unreadable_requests_are_refused_as_such() {
        for f in [check_config as fn(&str) -> String, run] {
            assert_eq!(call(f, json!({}))["verdict"], "invalid_request");
            assert_eq!(serde_json::from_str::<Value>(&f("[1]")).unwrap()["verdict"], "invalid_request");
        }
    }

    #[test]
    fn the_program_carried_is_the_one_declared() {
        let v: Value = serde_json::from_str(&version()).unwrap();
        assert_eq!(v["program"]["declared_id"], v["program"]["computed_id"], "{v}");
    }
}
