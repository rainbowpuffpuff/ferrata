//! Tests only: `ferrata-relayer` with Logos Core's module call replaced by a client of `e2e/netfault/txsender_stub.py`
//! (a stand-in for `tx_sender_module` over a local anvil), so that the daemon's `--send-via logos` route can be killed
//! and restarted as a process (`cli/tests/relayer_crash.rs`, part 5; RFP-022 R.06). `FERRATA_LOGOS_STUB=HOST:PORT`
//! names the stand-in (a loopback address). The real route runs inside `ferrata_relayer_module`, whose module crate
//! installs Logos Core's own call (`modules/scripts/relayer-module.sh`).

use std::io::{Read as _, Write as _};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context as _, Result};
use clap::Parser;
use ferrata_cli::relayerd::{self, Cli};
use serde_json::{json, Value};

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
    ferrata_cli::logos_send::install(call);
    let cli = Cli::parse();
    use tokio::signal::unix::{signal, SignalKind};
    let (mut term, mut int) = (signal(SignalKind::terminate())?, signal(SignalKind::interrupt())?);
    let (tx, rx) = tokio::sync::watch::channel(None);
    tokio::spawn(async move {
        let reason = tokio::select! {
            _ = term.recv() => "SIGTERM",
            _ = int.recv() => "SIGINT",
        };
        let _ = tx.send(Some(reason.to_string()));
    });
    relayerd::run(cli, Arc::new(|line| println!("{line}")), rx).await
}
