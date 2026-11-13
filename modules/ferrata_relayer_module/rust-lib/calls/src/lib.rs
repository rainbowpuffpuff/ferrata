//! The methods of `ferrata_relayer_module` as plain functions over JSON strings, so that they run and
//! are tested without the module scaffold. The work is the relayer daemon's own: `ferrata_cli::relayerd`,
//! the code of the `ferrata-relayer` binary, staged by `scripts/stage-relayer.sh` from `cli/` without the
//! `lez-wallet` feature (LEZ's wallet library links LEZ's node-side crates). `run` and `start` take the
//! binary's arguments as a list and parse them with the binary's own parser; the daemon's JSON lines,
//! which the binary prints, are kept and returned.
//!
//! Inside a Logos Core module the process and its signals are the host's, so the daemon never installs a
//! signal handler here: a blocking `run` ends after its `--rounds` (or `status`), a daemon started with
//! `start` ends with `stop` after the round in progress, as SIGTERM ends the binary; `stop` names the
//! reason the `stop` line gives (a caller standing in for the binary passes the signal it received).
//!
//! The LEZ side without LEZ's wallet: `--lez-devnet` (the mini-app devnet's zone, a stand-in); `--sequencer`
//! is refused by this build. Ethereum keys as for the binary: a keystore file and its password file, or
//! `--unlocked` on a local anvil.

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context as _, Result};
use clap::Parser;
use ferrata_cli::relayerd::{self, Cli, Command, Sink};
use serde_json::{json, Value};

/// What `scripts/stage-relayer.sh` recorded about the sources this crate was built against.
const STAGED: &str = include_str!("../../vendor/STAGED.json");

pub const METHODS: &[&str] = &["version", "run", "start", "status", "stop"];

/// The most lines a daemon keeps (the oldest are dropped first; `events_from` says where the kept ones start).
const KEEP: usize = 10_000;

/// `version()`: the module's version and the relayer it was built from.
pub fn version() -> String {
    json!({
        "module": "ferrata_relayer_module",
        "version": env!("CARGO_PKG_VERSION"),
        "relayer": serde_json::from_str::<Value>(STAGED).unwrap_or(Value::Null),
        // Staged without the `lez-wallet` feature: the zone is the devnet's (`--lez-devnet`).
        "lez_wallet": false,
        "zones": ["devnet"],
        "methods": METHODS,
    })
    .to_string()
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

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default()
}

/// The binary's arguments (`args`, a list of strings: global options, then `run ...` or `status`), parsed by
/// the binary's own parser.
fn cli_of(r: &Value) -> Result<Cli> {
    let argv = r
        .get("args")
        .and_then(Value::as_array)
        .context("args (the arguments of `ferrata-relayer`, a list of strings) is required")?
        .iter()
        .map(|v| v.as_str().map(str::to_owned).context("args: not a list of strings"))
        .collect::<Result<Vec<_>>>()?;
    Cli::try_parse_from(std::iter::once("ferrata-relayer".to_string()).chain(argv))
        .map_err(|e| anyhow!("{}", e.render().to_string().trim_end()))
}

fn runtime() -> Result<tokio::runtime::Runtime> {
    // Multi-threaded: the relayer wraps receipts in `block_in_place`.
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("ferrata-relayer")
        .enable_all()
        .build()
        .context("starting the relayer's runtime")
}

/// The lines a daemon logged, the oldest dropped past [`KEEP`].
#[derive(Default)]
struct Lines {
    kept: Vec<Value>,
    /// Lines logged in all (the index of the next one).
    total: u64,
}

impl Lines {
    fn push(&mut self, line: Value) {
        self.kept.push(line);
        self.total += 1;
        if self.kept.len() > KEEP {
            let over = self.kept.len() - KEEP;
            self.kept.drain(..over);
        }
    }

    fn from_index(&self) -> u64 {
        self.total - self.kept.len() as u64
    }

    /// The kept lines from index `since` on.
    fn since(&self, since: u64) -> &[Value] {
        let first = self.from_index();
        let skip = since.saturating_sub(first).min(self.kept.len() as u64) as usize;
        &self.kept[skip..]
    }
}

fn sink_into(lines: Arc<Mutex<Lines>>) -> Sink {
    Arc::new(move |line| lines.lock().unwrap_or_else(|p| p.into_inner()).push(line))
}

/// `run(request)`: `{args}`: `ferrata-relayer ARGS` to its end, which `run --rounds N` (N > 0) or `status` must
/// give (a daemon without an end is `start`). `verdict` `ok` (the binary's exit 0) or `error` (its exit 1,
/// with the message the binary prints), and the daemon's lines, as the binary prints them (no timing of its
/// own: a reply depends only on the request and the chains).
pub fn run(request: &str) -> String {
    reply((|| {
        let r = parse(request)?;
        let cli = cli_of(&r)?;
        if let Command::Run { rounds: 0, .. } = cli.command {
            bail!("run needs `run --rounds N` or `status` in args (it blocks until done); start runs until stop");
        }
        let lines = Arc::new(Mutex::new(Lines::default()));
        // The sender lives until the run ends: the loop ends by its rounds, never by a stop.
        let (_tx, rx) = tokio::sync::watch::channel(None);
        let result = runtime()?.block_on(relayerd::run(cli, sink_into(lines.clone()), rx));
        let events = lines.lock().unwrap_or_else(|p| p.into_inner()).kept.clone();
        Ok(match result {
            Ok(()) => json!({ "verdict": "ok", "events": events }),
            Err(e) => json!({ "verdict": "error", "error": format!("{e:#}"), "events": events }),
        })
    })())
}

// ---- the relayer as a daemon -------------------------------------------------------------------------

#[derive(Default)]
struct Outcome {
    running: bool,
    started_at: u64,
    stopped_at: Option<u64>,
    /// The daemon's end: `Ok` (the binary's exit 0) or its error (exit 1).
    result: Option<std::result::Result<(), String>>,
}

/// `ferrata-relayer run` in a thread of the module's process; [`Daemon::stop`] ends it after the round in
/// progress, as SIGTERM ends the binary.
pub struct Daemon {
    tx: tokio::sync::watch::Sender<Option<String>>,
    handle: Option<JoinHandle<()>>,
    outcome: Arc<Mutex<Outcome>>,
    lines: Arc<Mutex<Lines>>,
    args: Vec<String>,
}

impl Daemon {
    /// `start(request)`: `{args}` of `ferrata-relayer ... run ...`. Returns once the daemon has set itself up
    /// (its `start` line: the endpoint answered, the vault and chain were read, the zone opened) or ended with
    /// the binary's error; `Err` is the reply to give (the request refused, or the daemon's error).
    pub fn start(request: &str) -> std::result::Result<Self, String> {
        let r = parse(request).map_err(|e| reply(Err(e)))?;
        let cli = cli_of(&r).map_err(|e| reply(Err(e)))?;
        if !matches!(cli.command, Command::Run { .. }) {
            return Err(reply(Err(anyhow!("start runs `ferrata-relayer ... run ...`; status is a run"))));
        }
        let args: Vec<String> = r["args"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_owned)).collect();
        let runtime = runtime().map_err(|e| json!({ "verdict": "error", "error": format!("{e:#}") }).to_string())?;
        let (tx, rx) = tokio::sync::watch::channel(None);
        let outcome = Arc::new(Mutex::new(Outcome { running: true, started_at: now_secs(), ..Default::default() }));
        let lines = Arc::new(Mutex::new(Lines::default()));
        let (o, sink) = (outcome.clone(), sink_into(lines.clone()));
        let handle = std::thread::Builder::new()
            .name("ferrata-relayer-daemon".into())
            .spawn(move || {
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| runtime.block_on(relayerd::run(cli, sink, rx))));
                let result = match r {
                    Ok(Ok(())) => Ok(()),
                    Ok(Err(e)) => Err(format!("{e:#}")),
                    Err(_) => Err("the daemon panicked".to_string()),
                };
                let mut g = o.lock().unwrap_or_else(|p| p.into_inner());
                g.running = false;
                g.stopped_at = Some(now_secs());
                g.result = Some(result);
            })
            .map_err(|e| json!({ "verdict": "error", "error": format!("starting the daemon's thread: {e}") }).to_string())?;
        let d = Daemon { tx, handle: Some(handle), outcome, lines, args };
        // Set up, or ended: the `start` line, or the daemon's end (an unreachable endpoint, a missing argument).
        let t0 = Instant::now();
        while t0.elapsed() < Duration::from_secs(300) {
            let started = d.lines.lock().unwrap_or_else(|p| p.into_inner()).kept.iter().any(|l| l["event"] == "start");
            if started || !d.running() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        if let Some(Err(e)) = &d.outcome.lock().unwrap_or_else(|p| p.into_inner()).result {
            return Err(json!({ "verdict": "error", "error": e, "events": d.lines.lock().unwrap_or_else(|p| p.into_inner()).kept }).to_string());
        }
        Ok(d)
    }

    pub fn running(&self) -> bool {
        self.outcome.lock().unwrap_or_else(|p| p.into_inner()).running
    }

    /// `status(request)`: `{since?}`: whether the daemon runs, how it ended (`exit` 0 or 1 with `error`), the
    /// rounds it logged, and its lines from index `since` on (`events_total`: the index after the last).
    pub fn status(&self, request: &str) -> String {
        let since = parse(request).ok().and_then(|r| r["since"].as_u64()).unwrap_or(0);
        let o = self.outcome.lock().unwrap_or_else(|p| p.into_inner());
        let l = self.lines.lock().unwrap_or_else(|p| p.into_inner());
        let exit = match &o.result {
            None => Value::Null,
            Some(Ok(())) => json!(0),
            Some(Err(_)) => json!(1),
        };
        let error = match &o.result {
            Some(Err(e)) => json!(e),
            _ => Value::Null,
        };
        let from = since.max(l.from_index());
        json!({
            "verdict": "ok",
            "running": o.running,
            "started_at": o.started_at,
            "stopped_at": o.stopped_at,
            "exit": exit,
            "error": error,
            "args": self.args,
            "events_total": l.total,
            "events_from": from,
            "events": l.since(since),
        })
        .to_string()
    }

    /// `stop(request)`: `{reason?}` (default `stop`): the daemon ends after the round in progress and logs its
    /// `stop` line with that reason; its final status (lines from `since`, as `status`).
    pub fn stop(&mut self, request: &str) -> String {
        let reason = parse(request)
            .ok()
            .and_then(|r| r["reason"].as_str().map(str::to_owned))
            .unwrap_or_else(|| "stop".to_string());
        let _ = self.tx.send(Some(reason));
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        self.status(request)
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        if self.handle.is_some() {
            let _ = self.stop(r#"{"reason":"unload"}"#);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_are_parsed_by_the_binarys_parser() {
        let v: Value = serde_json::from_str(&run(r#"{"args": ["--frobnicate", "status"]}"#)).unwrap();
        assert_eq!(v["verdict"], "invalid_request");
        assert!(v["error"].as_str().unwrap().contains("--frobnicate"), "{v}");
        let v: Value = serde_json::from_str(&run(r#"{"args": ["run", "--inbox", "x"]}"#)).unwrap();
        assert_eq!(v["verdict"], "invalid_request", "an endless run is start's");
        let v: Value = serde_json::from_str(&run(r#"{"args": ["status"]}"#)).unwrap();
        assert_eq!(v["verdict"], "error");
        assert_eq!(v["error"], "--eth-rpc is required");
        let v: Value = serde_json::from_str(&run("[1]")).unwrap();
        assert_eq!(v["verdict"], "invalid_request");
    }

    #[test]
    fn start_refuses_what_is_not_a_daemon() {
        let e = Daemon::start(r#"{"args": ["status"]}"#).err().unwrap();
        assert!(e.contains("invalid_request"), "{e}");
        let e = Daemon::start(r#"{"args": ["run", "--rounds", "1"]}"#).err().unwrap();
        assert!(e.contains("--eth-rpc is required"), "{e}");
    }

    #[test]
    fn lines_keep_the_newest() {
        let mut l = Lines::default();
        for i in 0..(KEEP as u64 + 5) {
            l.push(json!({ "i": i }));
        }
        assert_eq!(l.total, KEEP as u64 + 5);
        assert_eq!(l.from_index(), 5);
        assert_eq!(l.since(0)[0]["i"], 5);
        assert_eq!(l.since(KEEP as u64 + 3).len(), 2);
        assert!(l.since(u64::MAX).is_empty());
    }

    #[test]
    fn version_names_the_methods() {
        let v: Value = serde_json::from_str(&version()).unwrap();
        assert_eq!(v["module"], "ferrata_relayer_module");
        assert_eq!(v["methods"].as_array().unwrap().len(), METHODS.len());
    }
}
