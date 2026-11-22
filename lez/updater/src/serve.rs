//! `/health` (JSON) and `/metrics` (Prometheus text) on a plain HTTP listener, from a snapshot the
//! daemon refreshes after every round.
//!
//! Bounded against its clients (any local process, or the network when `daemon.listen` is not on loopback), and so
//! that a monitor gets through: each connection is served on a thread of its own. At most [`MAX_CONNECTIONS`] are
//! served at once with the full [`DEADLINE`] to send their request head, and of those at most [`MAX_PER_CLIENT`] from
//! one host other than loopback (an IPv4 address, or an IPv6 /64 network; on loopback every client is this machine, one
//! address: they share the [`MAX_CONNECTIONS`]). A connection beyond those bounds is still served, on at most [`MAX_QUICK`] more threads, if
//! its request head arrives within [`QUICK_DEADLINE`], as a monitor's does (its request goes out with the connection);
//! beyond those it is closed at once. A request head is at most [`MAX_HEAD`] bytes (`400` beyond); a write that does
//! not drain within [`DEADLINE`] ends the connection. A connection's place is given back before it is closed, so a
//! client that connects again at once finds it free. Clients that keep connections idle therefore hold at most
//! [`MAX_CONNECTIONS`] threads for [`DEADLINE`] each, and to keep a prompt request out they must also fill the
//! [`MAX_QUICK`] places again at least every [`QUICK_DEADLINE`] (review of the fixes, L1: eight clients re-opening
//! idle connections used to deny `/health` for as long as they kept at it). One other host gets at most
//! [`MAX_PER_CLIENT`] of the [`MAX_CONNECTIONS`], so it cannot; several hosts, or a process on this machine, can: keep
//! `listen` on loopback.
//! A snapshot that no round has refreshed for longer than its limit ([`Snapshot::with_stale_after`]) is served with
//! `503`: a round that hangs (a settle wait, a source without a time limit) is not reported healthy.

use std::{
    collections::BTreeMap,
    io::{ErrorKind, Read, Write},
    net::{IpAddr, Ipv6Addr, Shutdown, TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result};

/// Connections served at once with the full [`DEADLINE`].
pub const MAX_CONNECTIONS: usize = 8;
/// Of the [`MAX_CONNECTIONS`], at most this many from one host other than loopback (an IPv4 address, or an
/// IPv6 address's /64 network).
pub const MAX_PER_CLIENT: usize = 4;
/// Connections served at once beyond [`MAX_CONNECTIONS`] (or beyond a client's [`MAX_PER_CLIENT`]), each given
/// [`QUICK_DEADLINE`] to send its request head; a connection beyond them is closed without an answer.
pub const MAX_QUICK: usize = 8;
/// The time a connection beyond the [`MAX_CONNECTIONS`] has to send its request head.
pub const QUICK_DEADLINE: Duration = Duration::from_millis(250);
/// The most bytes of a request head (request line, headers, the blank line) read from a client.
pub const MAX_HEAD: usize = 8 * 1024;
/// The time a client has to send its request head, and a write has to drain.
pub const DEADLINE: Duration = Duration::from_secs(2);
/// The default age after which a snapshot is served with `503`.
pub const STALE_AFTER: Duration = Duration::from_secs(600);

/// The age after which a daemon polling every `poll_secs` serves its snapshot with `503`: ten polls, and at least
/// [`STALE_AFTER`] (the binary's; a host of the daemon, such as the Logos module, sets the same with
/// [`Snapshot::with_stale_after`]).
pub fn stale_after(poll_secs: u64) -> Duration {
    STALE_AFTER.max(Duration::from_secs(poll_secs.saturating_mul(10)))
}
/// After a `400`, what is still read (and dropped) before the connection is closed, so that the client receives the
/// answer rather than a reset: at most this long and this many bytes.
const DRAIN: (Duration, usize) = (Duration::from_millis(200), 64 * 1024);

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

/// The connections being served: in the full lane (with the hosts other than loopback among them) and in the quick lane.
#[derive(Default)]
struct Lanes {
    full: usize,
    clients: BTreeMap<IpAddr, usize>,
    quick: usize,
}

/// Where a connection is served.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lane {
    /// Among the [`MAX_CONNECTIONS`]; the host ([`host`]) if it counts towards [`MAX_PER_CLIENT`] (not loopback).
    Full(Option<IpAddr>),
    /// Among the [`MAX_QUICK`].
    Quick,
}

/// What a client other than loopback is counted as for [`MAX_PER_CLIENT`]: its IPv4 address (an IPv4-mapped IPv6 address
/// as that IPv4 address), or its IPv6 address's /64 network. A host is usually given a whole /64 and can answer from any
/// address in it (temporary addresses), so counting each IPv6 address apart would give one host as many shares as it
/// has addresses (9c L-U4f).
fn host(ip: IpAddr) -> IpAddr {
    match ip.to_canonical() {
        IpAddr::V6(v6) => IpAddr::V6(Ipv6Addr::from(u128::from(v6) & !(u128::from(u64::MAX)))),
        v4 => v4,
    }
}

impl Lanes {
    /// The lane for a new connection from `peer`, taken; `None`: no room (close it).
    fn take(&mut self, peer: Option<IpAddr>) -> Option<Lane> {
        let client = peer.map(|ip| ip.to_canonical()).filter(|ip| !ip.is_loopback()).map(host);
        let mine = client.map_or(0, |ip| self.clients.get(&ip).copied().unwrap_or(0));
        if self.full < MAX_CONNECTIONS && mine < MAX_PER_CLIENT {
            self.full += 1;
            if let Some(ip) = client {
                *self.clients.entry(ip).or_default() += 1;
            }
            return Some(Lane::Full(client));
        }
        if self.quick < MAX_QUICK {
            self.quick += 1;
            return Some(Lane::Quick);
        }
        None
    }

    fn give_back(&mut self, lane: Lane) {
        match lane {
            Lane::Full(client) => {
                self.full -= 1;
                if let Some(ip) = client {
                    if let Some(n) = self.clients.get_mut(&ip) {
                        *n -= 1;
                        if *n == 0 {
                            self.clients.remove(&ip);
                        }
                    }
                }
            }
            Lane::Quick => self.quick -= 1,
        }
    }
}

/// A connection's place in its lane: given back when dropped (its thread ends, or never starts).
struct Place(Arc<Mutex<Lanes>>, Lane);

impl Place {
    fn deadline(&self) -> Duration {
        match self.1 {
            Lane::Full(_) => DEADLINE,
            Lane::Quick => QUICK_DEADLINE,
        }
    }
}

impl Drop for Place {
    fn drop(&mut self) {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).give_back(self.1);
    }
}

/// Serve the snapshot on `addr` from a thread of its own; returns the bound address.
pub fn serve(addr: &str, snapshot: Snapshot) -> Result<std::net::SocketAddr> {
    let listener = TcpListener::bind(addr).with_context(|| format!("listen on {addr}"))?;
    let local = listener.local_addr()?;
    let lanes = Arc::new(Mutex::new(Lanes::default()));
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let peer = stream.peer_addr().ok().map(|a| a.ip());
            let lane = lanes.lock().unwrap_or_else(|p| p.into_inner()).take(peer);
            let Some(lane) = lane else {
                let _ = stream.shutdown(Shutdown::Both);
                continue;
            };
            let place = Place(lanes.clone(), lane);
            let snapshot = snapshot.clone();
            // If the thread cannot start, the closure (and the place in it) is dropped: the place is given back.
            let _ =
                thread::Builder::new().name("ferrata-health".into()).spawn(move || answer(stream, &snapshot, place));
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

fn answer(mut stream: TcpStream, snapshot: &Snapshot, place: Place) {
    let _ = stream.set_write_timeout(Some(DEADLINE));
    match read_head(&mut stream, Instant::now(), place.deadline()) {
        Ok(line) => {
            let _ = stream.write_all(respond(&line, snapshot).as_bytes());
        }
        Err(Refused::TooLong) => {
            let body = format!("request head longer than {MAX_HEAD} bytes\n");
            let _ = stream.write_all(reply("400 Bad Request", "text/plain", &body, None).as_bytes());
            // What the client still sends is read and dropped for a moment: closed with unread input, the connection
            // would be reset and the client might never see the 400.
            let _ = stream.shutdown(Shutdown::Write);
            drain(&mut stream);
        }
        Err(Refused::Gone) => {}
    }
    // The place is given back before the close: a client that connects again as soon as it sees the close finds it.
    drop(place);
    let _ = stream.shutdown(Shutdown::Both);
}

/// Reads and drops what the client sends, for at most [`DRAIN`] (time and bytes) or until it closes.
fn drain(stream: &mut TcpStream) {
    let (limit, max) = DRAIN;
    let start = Instant::now();
    let mut chunk = [0u8; 4096];
    let mut read = 0usize;
    while read < max {
        let left = limit.saturating_sub(start.elapsed());
        if left.is_zero() || stream.set_read_timeout(Some(left)).is_err() {
            return;
        }
        match stream.read(&mut chunk) {
            Ok(0) => return,
            Ok(n) => read += n,
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(_) => return,
        }
    }
}

/// The request line, read with the rest of the head (up to the blank line), at most [`MAX_HEAD`] bytes in all, within
/// `deadline` of `start` (what has already arrived is always read once, however late the thread starts). A client that
/// has sent its request line but not its blank line by then is answered from the line.
fn read_head(stream: &mut TcpStream, start: Instant, deadline: Duration) -> std::result::Result<String, Refused> {
    let mut buf: Vec<u8> = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    let line = |buf: &[u8]| {
        buf.iter().position(|&b| b == b'\n').map(|end| String::from_utf8_lossy(&buf[..end]).trim_end().to_string())
    };
    let mut first = true;
    loop {
        if let Some(len) = head_len(&buf) {
            return if len > MAX_HEAD { Err(Refused::TooLong) } else { line(&buf).ok_or(Refused::Gone) };
        }
        if buf.len() >= MAX_HEAD {
            return Err(Refused::TooLong);
        }
        let left = deadline.saturating_sub(start.elapsed());
        if left.is_zero() && !first {
            return line(&buf).ok_or(Refused::Gone);
        }
        first = false;
        if stream.set_read_timeout(Some(left.max(Duration::from_millis(1)))).is_err() {
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

#[cfg(test)]
mod tests {
    use super::{host, Lane, Lanes, MAX_CONNECTIONS, MAX_PER_CLIENT, MAX_QUICK};
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    fn ip(last: u8) -> Option<IpAddr> {
        Some(IpAddr::V4(Ipv4Addr::new(192, 0, 2, last)))
    }

    /// One host other than loopback gets at most MAX_PER_CLIENT of the full lane, then the quick lane, then nothing;
    /// another host still finds the rest of the full lane; loopback clients share the full lane; places come back.
    #[test]
    fn per_client_and_total_bounds() {
        let mut l = Lanes::default();
        let a: Vec<_> = (0..MAX_PER_CLIENT + MAX_QUICK + 1).map(|_| l.take(ip(1))).collect();
        assert!(a[..MAX_PER_CLIENT].iter().all(|x| matches!(x, Some(Lane::Full(Some(_))))), "{a:?}");
        assert!(a[MAX_PER_CLIENT..MAX_PER_CLIENT + MAX_QUICK].iter().all(|x| *x == Some(Lane::Quick)), "{a:?}");
        assert_eq!(a[MAX_PER_CLIENT + MAX_QUICK], None, "one host fills its share and the quick lane, no more");
        let b: Vec<_> = (0..MAX_CONNECTIONS - MAX_PER_CLIENT).map(|_| l.take(ip(2))).collect();
        assert!(b.iter().all(|x| matches!(x, Some(Lane::Full(Some(_))))), "another host gets the rest: {b:?}");
        assert_eq!(l.take(ip(3)), None, "full and quick lanes taken");
        l.give_back(a[0].unwrap());
        assert_eq!(l.take(ip(3)), Some(Lane::Full(ip(3))), "a place given back is found");
        assert_eq!(l.clients.get(&ip(1).unwrap()), Some(&(MAX_PER_CLIENT - 1)));

        let mut l = Lanes::default();
        let local = Some(IpAddr::V4(Ipv4Addr::LOCALHOST));
        let mapped = Some(IpAddr::V6(Ipv4Addr::LOCALHOST.to_ipv6_mapped()));
        let v6 = Some(IpAddr::V6(Ipv6Addr::LOCALHOST));
        let c: Vec<_> = (0..MAX_CONNECTIONS).map(|i| l.take([local, mapped, v6][i % 3])).collect();
        assert!(c.iter().all(|x| *x == Some(Lane::Full(None))), "loopback shares the full lane: {c:?}");
        assert!(l.clients.is_empty());
        assert_eq!(l.take(local), Some(Lane::Quick));
        for x in c {
            l.give_back(x.unwrap());
        }
        assert_eq!(l.full, 0);
    }
    /// 9c L-U4f: an IPv6 client is counted by its /64 network, so one host answering from many addresses of its /64
    /// gets one share of the full lane; another /64 is another host; an IPv4-mapped address counts as its IPv4 address;
    /// loopback (`::1`) still shares the full lane uncounted.
    #[test]
    fn ipv6_clients_count_by_their_64_network() {
        let v6 =
            |net: u16, last: u16| Some(IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, net, 0x1111, 0x2222, 0x3333, last)));
        let mut l = Lanes::default();
        let a: Vec<_> = (1..=MAX_PER_CLIENT as u16 + 1).map(|i| l.take(v6(1, i))).collect();
        assert!(a[..MAX_PER_CLIENT].iter().all(|x| matches!(x, Some(Lane::Full(Some(_))))), "{a:?}");
        assert_eq!(a[MAX_PER_CLIENT], Some(Lane::Quick), "a fifth address of the same /64: {a:?}");
        assert_eq!(l.clients.len(), 1, "one host: {:?}", l.clients);
        assert!(matches!(l.take(v6(2, 1)), Some(Lane::Full(Some(_)))), "another /64 is another host");
        assert_eq!(l.clients.len(), 2);
        let v4 = Ipv4Addr::new(192, 0, 2, 7);
        assert_eq!(host(IpAddr::V6(v4.to_ipv6_mapped())), IpAddr::V4(v4));
        assert_eq!(l.take(Some(IpAddr::V6(Ipv6Addr::LOCALHOST))), Some(Lane::Full(None)));
        for x in a {
            l.give_back(x.unwrap());
        }
        assert_eq!(l.clients.len(), 1, "places given back: only the other /64 left");
    }
}
