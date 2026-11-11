//! `/health` (JSON) and `/metrics` (Prometheus text) on a plain HTTP listener, from a snapshot the
//! daemon refreshes after every round.

use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
};

use anyhow::{Context as _, Result};

/// What the listener serves: (metrics text, health JSON, healthy).
#[derive(Clone, Default)]
pub struct Snapshot(pub Arc<Mutex<(String, String, bool)>>);

impl Snapshot {
    pub fn set(&self, metrics: String, health: String, healthy: bool) {
        *self.0.lock().expect("snapshot lock") = (metrics, health, healthy);
    }
}

/// Serve the snapshot on `addr` from a thread of its own; returns the bound address.
pub fn serve(addr: &str, snapshot: Snapshot) -> Result<std::net::SocketAddr> {
    let listener = TcpListener::bind(addr).with_context(|| format!("listen on {addr}"))?;
    let local = listener.local_addr()?;
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut line = String::new();
            if BufReader::new(&stream).read_line(&mut line).is_err() {
                continue;
            }
            let path = line.split_whitespace().nth(1).unwrap_or("/");
            let (metrics, health, healthy) = snapshot.0.lock().map(|s| s.clone()).unwrap_or_default();
            let (status, kind, body) = match path {
                "/metrics" => ("200 OK", "text/plain; version=0.0.4", metrics),
                "/health" => (if healthy { "200 OK" } else { "503 Service Unavailable" }, "application/json", health),
                _ => ("404 Not Found", "text/plain", "not found\n".to_string()),
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    Ok(local)
}
