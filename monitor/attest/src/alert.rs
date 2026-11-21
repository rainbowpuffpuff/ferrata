//! Alerts, with the same contract as the watchers' `monitor` (`monitor/src/alert.rs`): one JSON line per event to
//! standard error, to an append-only log file, and to an operator's command (a pager, a webhook, or the consumer's
//! freeze tool, run with `sh -c`, the JSON on its standard input) at or above a level. Nothing else is sent anywhere.

use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Info,
    Warn,
    Critical,
}

impl std::str::FromStr for Level {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "info" => Ok(Level::Info),
            "warn" => Ok(Level::Warn),
            "critical" => Ok(Level::Critical),
            _ => Err(format!("{s:?}: info, warn or critical")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alert {
    pub level: Level,
    pub kind: String,
    pub message: String,
    #[serde(flatten)]
    pub fields: Map<String, Value>,
}

impl Alert {
    pub fn new(level: Level, kind: &str, message: &str) -> Self {
        Self { level, kind: kind.into(), message: message.into(), fields: Map::new() }
    }

    pub fn with(mut self, key: &str, value: impl Serialize) -> Self {
        self.fields.insert(key.into(), serde_json::to_value(value).unwrap_or(Value::Null));
        self
    }
}

/// Where alerts go.
#[derive(Clone, Debug, Default)]
pub struct Sink {
    pub log: Option<PathBuf>,
    pub cmd: Option<String>,
    /// The lowest level passed to `cmd` (everything goes to the log and to standard error). Default: warn.
    pub cmd_level: Option<Level>,
}

impl Sink {
    /// Emits `{ts, level, kind, message, ...fields}`. A failure to deliver is reported on standard error and never
    /// stops the monitor.
    pub fn emit(&self, alert: &Alert) {
        let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default();
        let mut line = serde_json::json!({ "ts": ts });
        if let (Some(obj), Ok(Value::Object(a))) = (line.as_object_mut(), serde_json::to_value(alert)) {
            obj.extend(a);
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
                eprintln!("attest-monitor: alert log {}: {e}", path.display());
            }
        }
        if let Some(cmd) = &self.cmd {
            if alert.level >= self.cmd_level.unwrap_or(Level::Warn) {
                let sent = Command::new("sh").arg("-c").arg(cmd).stdin(Stdio::piped()).spawn().and_then(|mut child| {
                    if let Some(mut stdin) = child.stdin.take() {
                        stdin.write_all(text.as_bytes())?;
                    }
                    child.wait()
                });
                match sent {
                    Ok(s) if s.success() => {}
                    Ok(s) => eprintln!("attest-monitor: alert command exited with {s}"),
                    Err(e) => eprintln!("attest-monitor: alert command: {e}"),
                }
            }
        }
    }
}
