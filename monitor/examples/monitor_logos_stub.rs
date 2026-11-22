//! Tests only: `monitor run` with Logos Core's module call replaced by a client of `e2e/netfault/txsender_stub.py` (a
//! stand-in for `tx_sender_module` over a local anvil), so that the monitor's `--send-via logos` route and its fallback
//! run as a process (`tests/logos-fallback.sh`; RFP-022 R.06). `FERRATA_LOGOS_STUB=HOST:PORT` names the stand-in (a
//! loopback address). The real route runs inside `ferrata_monitor_module`, whose module crate installs Logos Core's own
//! call (`modules/scripts/monitor-module.sh`). The same client is `cli/examples/relayer_logos_stub.rs`'s.

use std::io::{Read as _, Write as _};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use anyhow::{bail, Context as _, Result};
use clap::{Parser, Subcommand};
use ferrata_monitor::cli::{self, RunArgs};
use serde_json::{json, Value};

#[derive(Parser)]
#[command(name = "monitor_logos_stub", about = "monitor run, its sends through a stand-in for tx_sender_module (tests only)")]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,
}

// Parsed once at start; the size of the `run` variant does not matter.
#[allow(clippy::large_enum_variant)]
#[derive(Subcommand)]
enum Cmd {
    /// `monitor run`'s arguments.
    Run(RunArgs),
}

/// One call to the stand-in: POST {module, method, args}; its body is the method's reply.
fn call(module: &str, method: &str, args: &Value, timeout: Duration) -> std::result::Result<String, String> {
    let addr: SocketAddr = std::env::var("FERRATA_LOGOS_STUB")
        .map_err(|_| "FERRATA_LOGOS_STUB is not set".to_string())?
        .parse()
        .map_err(|e| format!("FERRATA_LOGOS_STUB: {e}"))?;
    let body = json!({ "module": module, "method": method, "args": args }).to_string();
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).map_err(|e| format!("{module}: {e}"))?;
    s.set_read_timeout(Some(timeout)).map_err(|e| e.to_string())?;
    write!(
        s,
        "POST / HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .map_err(|e| format!("{module}.{method}: {e}"))?;
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).map_err(|e| format!("{module}.{method} did not answer: {e}"))?;
    let text = String::from_utf8_lossy(&raw);
    let (head, reply) = text.split_once("\r\n\r\n").ok_or_else(|| format!("{module}.{method}: no answer"))?;
    if !head.starts_with("HTTP/1.0 200") && !head.starts_with("HTTP/1.1 200") {
        return Err(format!("{module}.{method}: {}", head.lines().next().unwrap_or("")));
    }
    Ok(reply.to_string())
}

#[tokio::main]
async fn main() -> Result<()> {
    let stub = std::env::var("FERRATA_LOGOS_STUB").context("FERRATA_LOGOS_STUB=HOST:PORT (txsender_stub.py)")?;
    let addr: SocketAddr = stub.parse().context("FERRATA_LOGOS_STUB")?;
    if !addr.ip().is_loopback() {
        bail!("FERRATA_LOGOS_STUB must be a loopback address (a test's stand-in)");
    }
    ferrata_monitor::logos_send::install(call);
    match Args::parse().cmd {
        Cmd::Run(a) => cli::run(a, None).await,
    }
}
