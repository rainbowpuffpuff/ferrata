//! The methods of `ferrata_monitor_module` as plain functions over JSON strings, so that they run and
//! are tested without the module scaffold. The work is `monitor`'s (`monitor/`, staged into the module
//! by `scripts/stage.sh`): `run` and `start` take the arguments of `monitor run` and run the same
//! daemon (`ferrata_monitor::cli::run`), `check` is `monitor check`, `node_view` is `monitor node-view`.
//!
//! Inside a Logos Core module the process and its signals are the host's, so the daemon never installs
//! a signal handler here: a blocking `run` ends after its `--max-polls`, a daemon started with `start`
//! ends with `stop` (the same clean end as SIGTERM gives the binary: a `stopped` alert, the state file
//! saved, the lock released). The watcher key reaches the module as a file (`--key-file`) or a
//! keystore, not through the client's environment.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context as _, Result};
use clap::Parser;
use ferrata_monitor::check::{evaluate, Candidate, Policy, Rule};
use ferrata_monitor::cli::RunArgs;
use ferrata_monitor::node::{KeySet, NodeSource, NodeView};
use serde_json::{json, Value};

/// What `scripts/stage.sh` recorded about the copy of `monitor/` this crate was built against.
const STAGED: &str = include_str!("../../monitor/STAGED.json");

pub const METHODS: &[&str] = &["version", "check", "node_view", "run", "start", "status", "stop", "custody"];

/// `version()`: the module's version and the monitor it was built from.
pub fn version() -> String {
    json!({
        "module": "ferrata_monitor_module",
        "version": env!("CARGO_PKG_VERSION"),
        "monitor": serde_json::from_str::<Value>(STAGED).unwrap_or(Value::Null),
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

/// The arguments of `monitor run`, as a list of strings (`args`), parsed by the binary's own parser.
#[derive(Parser)]
#[command(name = "monitor run", no_binary_name = true)]
struct RunCli {
    #[command(flatten)]
    a: RunArgs,
}

fn run_args(r: &Value) -> Result<RunArgs> {
    let argv = r
        .get("args")
        .and_then(Value::as_array)
        .context("args (the arguments of `monitor run`, a list of strings) is required")?
        .iter()
        .map(|v| v.as_str().map(str::to_owned).context("args: not a list of strings"))
        .collect::<Result<Vec<_>>>()?;
    RunCli::try_parse_from(argv).map(|c| c.a).map_err(|e| anyhow!("{}", e.render().to_string().trim_end()))
}

fn state_of(a: &RunArgs) -> Value {
    std::fs::read(&a.state).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null)
}

fn runtime() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("ferrata-monitor")
        .enable_all()
        .build()
        .context("starting the monitor's runtime")
}

/// `run(request)`: `{args}`: `monitor run ARGS` to its end, which `--max-polls` must bound (an
/// open-ended monitor is `start`). `verdict` `ok` (the binary's exit 0) or `error` (its exit 1, with
/// the message the binary prints), and the state file after the run.
pub fn run(request: &str) -> String {
    reply((|| {
        let r = parse(request)?;
        let a = run_args(&r)?;
        if a.max_polls.is_none() {
            bail!("run needs --max-polls in args (it blocks until the polls are done); start runs until stop");
        }
        if a.send_via == "logos" {
            bail!("`run` holds the module's Qt thread until the polls are done, and a call to another module from the daemon's thread waits for that thread (logos-rust-sdk src/plugin.rs 212-222: a Qt::BlockingQueuedConnection): with --send-via logos use `start`, then `status`");
        }
        let t0 = std::time::Instant::now();
        // The sender lives until the run ends: the loop ends by its polls, never by a signal.
        let (_tx, rx) = tokio::sync::watch::channel(false);
        let result = runtime()?.block_on(ferrata_monitor::cli::run(a.clone(), Some(rx)));
        let seconds = t0.elapsed().as_secs_f64();
        Ok(match result {
            Ok(()) => json!({ "verdict": "ok", "seconds": seconds, "state": state_of(&a) }),
            Err(e) => {
                json!({ "verdict": "error", "error": format!("{e:#}"), "seconds": seconds, "state": state_of(&a) })
            }
        })
    })())
}

/// Logos Core's call of another module by name, set by the module crate when Logos Core installs it; natively
/// none, and `--send-via logos` and `custody` are refused.
pub fn set_module_call(call: ferrata_monitor::logos_send::ModuleCall) {
    ferrata_monitor::logos_send::install(call);
}

/// `custody(request)`: `{key_file, password_file, ttl_ms?, count?, apps?}`: the watcher key into Logos Core's
/// `keystore_module` (`import_private_key`) and, once `signer_manager_module` lists it, an unlock of it for
/// `tx_sender_module` without confirmation (`unlock`, `confirm: false`). This module must be a custodian of both
/// (their `configure`, which names custodians). Afterwards `run` and `start` with `--send-via logos --from ADDRESS`
/// send the monitor's transactions through `tx_sender_module`. `verdict` `ok` with the address and the unlock's
/// terms, `refused` with the step and the module's reply.
pub fn custody(request: &str) -> String {
    reply((|| {
        let r = parse(request)?;
        let req: ferrata_monitor::logos_send::CustodyRequest =
            serde_json::from_value(r).context("custody takes {key_file, password_file, ttl_ms?, count?, apps?}")?;
        let call = ferrata_monitor::logos_send::installed().context(
            "custody needs Logos Core (keystore_module, signer_manager_module): call it on ferrata_monitor_module",
        )?;
        Ok(ferrata_monitor::logos_send::custody(call, &req))
    })())
}

/// `check(request)`: `{rule, candidate | candidate_file, nodes, now, turn_seconds?, b_lag_grace?,
/// delta_g?, margin?}`: one offline verdict, exactly `monitor check`'s output (its `verdict` is `pass`,
/// `wait`, `cancel` or `blind`). `nodes`: node views (objects) or sources (`file:PATH`, `cmd:COMMAND`).
pub fn check(request: &str) -> String {
    reply((|| {
        let r = parse(request)?;
        let rule = match r.get("rule").and_then(Value::as_str) {
            Some("A" | "a") => Rule::A,
            Some("B" | "b") => Rule::B,
            _ => bail!("rule: A or B"),
        };
        let candidate: Candidate = match (r.get("candidate"), r.get("candidate_file").and_then(Value::as_str)) {
            (Some(c), None) => serde_json::from_value(c.clone()).context("the candidate does not parse")?,
            (None, Some(p)) => serde_json::from_slice(&std::fs::read(p).with_context(|| p.to_string())?)
                .context("the candidate does not parse")?,
            _ => bail!("exactly one of candidate or candidate_file"),
        };
        let mut views = Vec::new();
        for v in r.get("nodes").and_then(Value::as_array).context("nodes (a list) is required")? {
            views.push(match v {
                Value::String(s) => s.parse::<NodeSource>().map_err(|e| anyhow!("{e}"))?.read()?,
                other => {
                    let view: NodeView = serde_json::from_value(other.clone()).context("a node view does not parse")?;
                    view.validate()?;
                    view
                }
            });
        }
        if views.is_empty() {
            bail!("nodes: at least one");
        }
        let num = |k: &str, d: u64| -> Result<u64> {
            match r.get(k) {
                None => Ok(d),
                Some(v) => v.as_u64().with_context(|| format!("{k}: not an unsigned integer")),
            }
        };
        let now = r.get("now").and_then(Value::as_u64).context("now (chain time, seconds) is required")?;
        let p = Policy {
            rule,
            turn_seconds: num("turn_seconds", 0)?,
            b_lag_grace: num("b_lag_grace", 300)?,
            delta_g: num("delta_g", 7200)?,
            margin: num("margin", 120)?,
        };
        // `monitor check`'s output: the verdict itself (`{"verdict": "pass" | "wait" | "cancel" | "blind", ...}`).
        Ok(serde_json::to_value(evaluate(&p, &candidate, &views, now))?)
    })())
}

/// `node_view(request)`: `{chain_tsv | chain_tsv_file, label?, key_sets? | key_sets_file?}`: the node
/// view built from a LEZ sequencer's block record (`monitor node-view`).
pub fn node_view(request: &str) -> String {
    reply((|| {
        let r = parse(request)?;
        let text = match (r.get("chain_tsv").and_then(Value::as_str), r.get("chain_tsv_file").and_then(Value::as_str)) {
            (Some(t), None) => t.to_owned(),
            (None, Some(p)) => std::fs::read_to_string(p).with_context(|| format!("reading {p}"))?,
            _ => bail!("exactly one of chain_tsv or chain_tsv_file"),
        };
        let sets: Vec<KeySet> = match (r.get("key_sets"), r.get("key_sets_file").and_then(Value::as_str)) {
            (None, None) => Vec::new(),
            (Some(k), None) => serde_json::from_value(k.clone()).context("the key sets do not parse")?,
            (None, Some(p)) => serde_json::from_slice(&std::fs::read(p).with_context(|| p.to_string())?)
                .context("the key sets do not parse")?,
            _ => bail!("at most one of key_sets or key_sets_file"),
        };
        let label = r.get("label").and_then(Value::as_str).unwrap_or("lez");
        Ok(json!({ "verdict": "ok", "view": serde_json::to_value(NodeView::from_chain_tsv(label, &text, sets)?)? }))
    })())
}

// ---- the monitor as a daemon -------------------------------------------------------------------------

#[derive(Default)]
struct Outcome {
    running: bool,
    started_at: u64,
    stopped_at: Option<u64>,
    /// The daemon's end: `Ok` (the binary's exit 0) or its error (exit 1).
    result: Option<std::result::Result<(), String>>,
}

/// `monitor run` in a thread of the module's process; [`Daemon::stop`] ends it as SIGTERM ends the binary.
pub struct Daemon {
    tx: tokio::sync::watch::Sender<bool>,
    handle: Option<JoinHandle<()>>,
    outcome: Arc<Mutex<Outcome>>,
    args: RunArgs,
    argv: Value,
}

impl Daemon {
    /// `start(request)`: `{args}`: `monitor run ARGS` until `stop` (or its `--max-polls`). The reply
    /// comes once the daemon has started (its `started` alert sent) or failed to (an error then, as the
    /// binary exits 1: an unreachable node, a lock held by another monitor).
    pub fn start(request: &str) -> std::result::Result<Self, String> {
        let r = parse(request).map_err(|e| reply(Err(e)))?;
        let a = run_args(&r).map_err(|e| reply(Err(e)))?;
        let (tx, rx) = tokio::sync::watch::channel(false);
        let outcome = Arc::new(Mutex::new(Outcome { running: true, started_at: now_secs(), ..Outcome::default() }));
        let (a2, o2) = (a.clone(), outcome.clone());
        let handle = std::thread::Builder::new()
            .name("ferrata-monitor-daemon".into())
            .spawn(move || {
                let result = match runtime() {
                    Ok(rt) => rt.block_on(ferrata_monitor::cli::run(a2, Some(rx))).map_err(|e| format!("{e:#}")),
                    Err(e) => Err(format!("{e:#}")),
                };
                let mut o = o2.lock().unwrap_or_else(|p| p.into_inner());
                o.running = false;
                o.stopped_at = Some(now_secs());
                o.result = Some(result);
            })
            .map_err(|e| reply(Err(anyhow!("spawning the monitor's thread: {e}"))))?;
        let d = Self { tx, handle: Some(handle), outcome, args: a, argv: r["args"].clone() };
        // Wait for the start: the lock taken and the state file written, or the daemon's end.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while std::time::Instant::now() < deadline {
            if !d.running() || d.started() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        Ok(d)
    }

    pub fn running(&self) -> bool {
        self.outcome.lock().map(|o| o.running).unwrap_or(false)
    }

    /// Whether this daemon's state file shows it started: the store is saved once before the first poll.
    fn started(&self) -> bool {
        let o = self.outcome.lock().map(|o| o.started_at).unwrap_or(0);
        std::fs::metadata(&self.args.state)
            .and_then(|m| m.modified())
            .map(|t| t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) >= o)
            .unwrap_or(false)
    }

    /// `status()`: whether the daemon runs, how it ended, its state file.
    pub fn status(&self) -> String {
        let o = self.outcome.lock().unwrap_or_else(|p| p.into_inner());
        let (verdict, error) = match &o.result {
            None => ("ok", Value::Null),
            Some(Ok(())) => ("ok", Value::Null),
            Some(Err(e)) => ("error", json!(e)),
        };
        json!({
            "verdict": verdict, "running": o.running, "started_at": o.started_at, "stopped_at": o.stopped_at,
            "exit": o.result.as_ref().map(|r| if r.is_ok() { 0 } else { 1 }), "error": error,
            "state_file": self.args.state, "state": state_of(&self.args), "args": self.argv,
        })
        .to_string()
    }

    /// `stop()`: ends the loop after the poll in hand (a `stopped` alert, the state saved, the lock
    /// released) and waits for it.
    pub fn stop(&mut self) -> String {
        let _ = self.tx.send(true);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        self.status()
    }

    pub fn state_file(&self) -> PathBuf {
        self.args.state.clone()
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.tx.send(true);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(f: fn(&str) -> String, r: Value) -> Value {
        serde_json::from_str(&f(&r.to_string())).unwrap()
    }

    #[test]
    fn unreadable_requests_are_refused_as_such() {
        for f in [run as fn(&str) -> String, check, node_view] {
            assert_eq!(call(f, json!({}))["verdict"], "invalid_request");
            assert_eq!(serde_json::from_str::<Value>(&f("[1]")).unwrap()["verdict"], "invalid_request");
        }
        // The binary's own parser: a missing --state is its error.
        let e = call(
            run,
            json!({"args": ["--rpc", "http://127.0.0.1:1", "--vault", "0x0000000000000000000000000000000000000001", "--node", "file:x"]}),
        );
        assert_eq!(e["verdict"], "invalid_request");
        assert!(e["error"].as_str().unwrap().contains("--state"), "{e}");
        // An open-ended run is refused (it would block the module).
        let e = call(
            run,
            json!({"args": ["--rpc", "http://127.0.0.1:1", "--vault", "0x0000000000000000000000000000000000000001",
            "--node", "file:x", "--state", "s.json"]}),
        );
        assert!(e["error"].as_str().unwrap().contains("--max-polls"), "{e}");
    }

    #[test]
    fn an_unreachable_node_ends_the_daemon_with_the_binarys_error() {
        let dir = std::env::temp_dir().join(format!("ferrata-monitor-calls-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let args = json!({"args": ["--rpc", "http://127.0.0.1:9", "--vault", "0x0000000000000000000000000000000000000001",
            "--node", format!("file:{}", dir.join("view.json").display()), "--state", dir.join("s.json"), "--max-polls", "1"]});
        let r = call(run, args.clone());
        assert_eq!(r["verdict"], "error", "{r}");
        let mut d = Daemon::start(&args.to_string()).unwrap();
        let s: Value = serde_json::from_str(&d.stop()).unwrap();
        assert_eq!(s["running"], false);
        assert_eq!(s["exit"], 1, "{s}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
