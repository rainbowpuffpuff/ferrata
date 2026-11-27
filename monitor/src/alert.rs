//! Alerts: one JSON line per event, to standard error, to an append-only log file, and to an
//! operator's command (a pager or webhook script, run with `sh -c`, the JSON on its standard input).
//! The monitor itself sends nothing anywhere else.

use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;

use serde::Serialize;
use serde_json::{json, Value};

/// Environment variables no command the monitor runs may see: the one `--key-env` names, registered by
/// `cli::signer` (internal review of 27 November 2026, C-4: alert and node commands inherited the watcher key).
static HIDDEN_ENV: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Keeps `var` out of every command the monitor runs from now on.
pub fn hide_env(var: &str) {
    let mut hidden = HIDDEN_ENV.lock().unwrap_or_else(|e| e.into_inner());
    if !hidden.iter().any(|v| v == var) {
        hidden.push(var.to_owned());
    }
}

/// `sh -c command`, its environment the monitor's without the hidden variables.
pub fn shell(command: &str) -> Command {
    let mut c = Command::new("sh");
    c.arg("-c").arg(command);
    for var in HIDDEN_ENV.lock().unwrap_or_else(|e| e.into_inner()).iter() {
        c.env_remove(var);
    }
    c
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Info,
    Warn,
    Critical,
}

#[derive(Clone, Debug, Default)]
pub struct Alerts {
    pub log: Option<PathBuf>,
    pub cmd: Option<String>,
    /// The minimum level passed to `cmd` (everything goes to the log).
    pub cmd_level: Option<Level>,
}

fn rank(l: Level) -> u8 {
    match l {
        Level::Info => 0,
        Level::Warn => 1,
        Level::Critical => 2,
    }
}

impl Alerts {
    /// Emits `{ts, level, kind, chainTime, ...fields}`. Failures to deliver are reported on standard
    /// error and never stop the monitor.
    pub fn emit(&self, level: Level, kind: &str, chain_time: u64, fields: Value) {
        let ts =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default();
        let mut line = json!({ "ts": ts, "level": level, "kind": kind, "chainTime": chain_time });
        if let (Some(obj), Value::Object(extra)) = (line.as_object_mut(), fields) {
            obj.extend(extra);
        }
        let text = line.to_string();
        eprintln!("{text}");
        if let Some(path) = &self.log {
            let written = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .and_then(|mut f| writeln!(f, "{text}").and_then(|()| f.sync_data()));
            if let Err(e) = written {
                eprintln!("monitor: alert log {}: {e}", path.display());
            }
        }
        if let Some(cmd) = &self.cmd {
            if rank(level) >= rank(self.cmd_level.unwrap_or(Level::Warn)) {
                let sent = shell(cmd).stdin(Stdio::piped()).spawn().and_then(|mut child| {
                    if let Some(mut stdin) = child.stdin.take() {
                        stdin.write_all(text.as_bytes())?;
                    }
                    child.wait()
                });
                match sent {
                    Ok(s) if s.success() => {}
                    Ok(s) => eprintln!("monitor: alert command exited with {s}"),
                    Err(e) => eprintln!("monitor: alert command: {e}"),
                }
            }
        }
    }
}
