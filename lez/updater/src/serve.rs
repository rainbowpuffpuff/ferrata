//! `/health` (JSON) and `/metrics` (Prometheus text) on a plain HTTP listener, from a snapshot the
//! daemon refreshes after every round.
//!
//! Bounded against its clients (any local process, or the network when `daemon.listen` is not on loopback): each
//! connection is served on a thread of its own, at most [`MAX_CONNECTIONS`] at once (one more is closed at once); a
//! client has [`DEADLINE`] in all to send its request head, at most [`MAX_HEAD`] bytes (`400` beyond, then the
//! connection is closed); a write that does not drain within [`DEADLINE`] ends the connection. One idle or slow client
//! therefore holds one thread for at most [`DEADLINE`] and at most [`MAX_HEAD`] bytes, and never blocks another.
//! A snapshot that no round has refreshed for longer than its limit ([`Snapshot::with_stale_after`]) is served with
//! `503`: a round that hangs (a settle wait, a source without a time limit) is not reported healthy.

use std::{
    io::{ErrorKind, Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result};

/// Connections served at once; a connection beyond them is closed without an answer.
pub const MAX_CONNECTIONS: usize = 8;
/// The most bytes of a request head (request line, headers, the blank line) read from a client.
pub const MAX_HEAD: usize = 8 * 1024;
/// The time a client has to send its request head, and a write has to drain.
pub const DEADLINE: Duration = Duration::from_secs(2);
/// The default age after which a snapshot is served with `503`.
pub const STALE_AFTER: Duration = Duration::from_secs(600);

struct State {
    metrics: String,
    health: String,
    healthy: bool,
    /// When `set` was last called (`None`: never).
    at: Option<Instant>,
    stale_after: Duration,
}

/// What the listener serves: the metrics text, the health JSON, whether the daemon is healthy, and when the daemon set
/// them. Until the first `set`, `/health` answers `503`.
#[derive(Clone)]
pub struct Snapshot(Arc<Mutex<State>>);

impl Default for Snapshot {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(State {
            metrics: String::new(),
            health: String::new(),
            healthy: false,
            at: None,
            stale_after: STALE_AFTER,
        })))
    }
}

impl Snapshot {
    /// The same snapshot, served with `503` once no `set` has refreshed it for `limit`.
    pub fn with_stale_after(self, limit: Duration) -> Self {
        self.lock().stale_after = limit;
        self
    }

    pub fn set(&self, metrics: String, health: String, healthy: bool) {
        let mut s = self.lock();
        s.metrics = metrics;
        s.health = health;
        s.healthy = healthy;
        s.at = Some(Instant::now());
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// (metrics, health, healthy and fresh, age in whole seconds if set).
    fn read(&self) -> (String, String, bool, Option<u64>) {
        let s = self.lock();
        let age = s.at.map(|t| t.elapsed());
        let fresh = age.is_some_and(|a| a <= s.stale_after);
        (s.metrics.clone(), s.health.clone(), s.healthy && fresh, age.map(|a| a.as_secs()))
    }
}

/// A slot among the [`MAX_CONNECTIONS`]: given back when its connection's thread ends (or never starts).
struct Slot(Arc<AtomicUsize>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Serve the snapshot on `addr` from a thread of its own; returns the bound address.
pub fn serve(addr: &str, snapshot: Snapshot) -> Result<std::net::SocketAddr> {
    let listener = TcpListener::bind(addr).with_context(|| format!("listen on {addr}"))?;
    let local = listener.local_addr()?;
    let active = Arc::new(AtomicUsize::new(0));
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            if active.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
                active.fetch_sub(1, Ordering::SeqCst);
                let _ = stream.shutdown(Shutdown::Both);
                continue;
            }
            let slot = Slot(active.clone());
            let snapshot = snapshot.clone();
            // If the thread cannot start, the closure (and the slot in it) is dropped: the slot is given back.
            let _ = thread::Builder::new().name("ferrata-health".into()).spawn(move || {
                let _slot = slot;
                answer(stream, &snapshot);
            });
        }
    });
    Ok(local)
}

/// Why a request head was not read.
enum Refused {
    /// Longer than [`MAX_HEAD`]: answered `400`.
    TooLong,
    /// Not complete within [`DEADLINE`], closed by the client before a request line, or a read error: closed.
    Gone,
}

fn answer(mut stream: TcpStream, snapshot: &Snapshot) {
    let _ = stream.set_write_timeout(Some(DEADLINE));
    let reply = match read_head(&mut stream, Instant::now()) {
        Ok(line) => respond(&line, snapshot),
        Err(Refused::TooLong) => {
            reply("400 Bad Request", "text/plain", &format!("request head longer than {MAX_HEAD} bytes\n"), None)
        }
        Err(Refused::Gone) => {
            let _ = stream.shutdown(Shutdown::Both);
            return;
        }
    };
    let _ = stream.write_all(reply.as_bytes());
    let _ = stream.shutdown(Shutdown::Both);
}

/// The request line, read with the rest of the head (up to the blank line), at most [`MAX_HEAD`] bytes in all, within
/// [`DEADLINE`] of `start`. A client that has sent its request line but not its blank line by then is answered from
/// the line.
fn read_head(stream: &mut TcpStream, start: Instant) -> std::result::Result<String, Refused> {
    let mut buf: Vec<u8> = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    let line = |buf: &[u8]| {
        buf.iter().position(|&b| b == b'\n').map(|end| String::from_utf8_lossy(&buf[..end]).trim_end().to_string())
    };
    loop {
        if let Some(len) = head_len(&buf) {
            return if len > MAX_HEAD { Err(Refused::TooLong) } else { line(&buf).ok_or(Refused::Gone) };
        }
        if buf.len() >= MAX_HEAD {
            return Err(Refused::TooLong);
        }
        let left = DEADLINE.saturating_sub(start.elapsed());
        if left.is_zero() || stream.set_read_timeout(Some(left)).is_err() {
            return line(&buf).ok_or(Refused::Gone);
        }
        match stream.read(&mut chunk) {
            Ok(0) => return line(&buf).ok_or(Refused::Gone),
            // At most MAX_HEAD - 1 + 1024 bytes are ever kept: the length is checked before each read.
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(_) => return line(&buf).ok_or(Refused::Gone),
        }
    }
}

/// The length of the request head in `buf` (through its blank line), if `buf` holds all of it.
fn head_len(buf: &[u8]) -> Option<usize> {
    let crlf = buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4);
    let lf = buf.windows(2).position(|w| w == b"\n\n").map(|p| p + 2);
    match (crlf, lf) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

fn respond(line: &str, snapshot: &Snapshot) -> String {
    let path = line.split_whitespace().nth(1).unwrap_or("/");
    let (metrics, health, healthy, age) = snapshot.read();
    match path {
        "/metrics" => reply("200 OK", "text/plain; version=0.0.4", &metrics, age),
        "/health" => {
            reply(if healthy { "200 OK" } else { "503 Service Unavailable" }, "application/json", &health, age)
        }
        _ => reply("404 Not Found", "text/plain", "not found\n", None),
    }
}

fn reply(status: &str, kind: &str, body: &str, age: Option<u64>) -> String {
    let age = age.map(|a| format!("X-Snapshot-Age-Seconds: {a}\r\n")).unwrap_or_default();
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\n{age}Connection: close\r\n\r\n{body}",
        body.len()
    )
}
