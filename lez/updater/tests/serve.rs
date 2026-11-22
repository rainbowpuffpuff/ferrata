//! The `/health` and `/metrics` listener against clients that misbehave (review-code-ops M3; review-code-lc UP-M2):
//! one idle connection, a request with no end, a client that trickles, more connections than it serves, and a
//! snapshot no round refreshes. Loopback only; no chain.

use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    thread,
    time::{Duration, Instant},
};

use ferrata_updater::serve::{serve, Snapshot, DEADLINE, MAX_CONNECTIONS, MAX_HEAD};

fn listener(snapshot: Snapshot) -> SocketAddr {
    serve("127.0.0.1:0", snapshot).unwrap()
}

fn healthy() -> Snapshot {
    let s = Snapshot::default();
    s.set("ferrata_updater_rounds_total 1\n".into(), "[{\"status\":\"ok\"}]\n".into(), true);
    s
}

/// The whole answer to `request` (the server closes after it), or why there is none within `timeout`.
fn ask(addr: SocketAddr, request: &[u8], timeout: Duration) -> std::io::Result<String> {
    let mut c = TcpStream::connect(addr)?;
    c.set_read_timeout(Some(timeout))?;
    c.set_write_timeout(Some(timeout))?;
    c.write_all(request)?;
    let mut out = Vec::new();
    c.read_to_end(&mut out)?;
    Ok(String::from_utf8_lossy(&out).into_owned())
}

fn get(addr: SocketAddr, path: &str) -> std::io::Result<String> {
    ask(addr, format!("GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes(), Duration::from_secs(3))
}

fn status(answer: &str) -> &str {
    answer.lines().next().unwrap_or("")
}

/// Waits until the server ends `c` (end of stream or a reset); how long that took.
fn closed_within(c: &mut TcpStream, limit: Duration) -> Option<Duration> {
    let t = Instant::now();
    c.set_read_timeout(Some(limit)).unwrap();
    let mut b = [0u8; 4096];
    loop {
        match c.read(&mut b) {
            Ok(0) => return Some(t.elapsed()),
            Ok(_) => continue,
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => return None,
            Err(_) => return Some(t.elapsed()),
        }
    }
}

#[cfg(target_os = "linux")]
fn rss_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("VmRSS:"))
        .and_then(|v| v.split_whitespace().next()?.parse().ok())
        .unwrap()
}

/// The review's reproduction: one client that connects and sends nothing (and one that sends `GET ` and stops) no
/// longer keep `/health` from answering, and each is closed by the server within [`DEADLINE`].
#[test]
fn health_answers_beside_an_idle_connection() {
    let addr = listener(healthy());
    let mut idle = TcpStream::connect(addr).unwrap();
    let mut partial = TcpStream::connect(addr).unwrap();
    partial.write_all(b"GET ").unwrap();
    thread::sleep(Duration::from_millis(200));
    let t = Instant::now();
    let a = get(addr, "/health").expect("an answer beside the idle connections");
    let took = t.elapsed();
    println!("/health beside two idle connections: {:?} in {took:?}", status(&a));
    assert_eq!(status(&a), "HTTP/1.1 200 OK");
    assert!(a.ends_with("[{\"status\":\"ok\"}]\n"), "{a}");
    assert!(took < Duration::from_secs(1), "answered at once, not after the idle clients: {took:?}");
    let idle_closed = closed_within(&mut idle, DEADLINE + Duration::from_secs(2));
    let partial_closed = closed_within(&mut partial, DEADLINE + Duration::from_secs(2));
    println!("idle connection closed after {idle_closed:?} more; the one that sent `GET ` after {partial_closed:?}");
    assert!(idle_closed.is_some() && partial_closed.is_some(), "the server ends connections that send no request");
    assert_eq!(status(&get(addr, "/metrics").unwrap()), "HTTP/1.1 200 OK");
}

/// A request head of exactly [`MAX_HEAD`] bytes is answered; one byte more is refused with `400`; a megabyte with no
/// newline is refused without the process growing (the review measured +257 MiB for 256 MiB).
#[test]
fn request_heads_are_bounded() {
    let addr = listener(healthy());
    let head = |len: usize| {
        let start = "GET /health HTTP/1.1\r\nX-Pad: ";
        let end = "\r\n\r\n";
        let mut h = start.to_string();
        h.push_str(&"a".repeat(len - start.len() - end.len()));
        h.push_str(end);
        assert_eq!(h.len(), len);
        h.into_bytes()
    };
    let at = ask(addr, &head(MAX_HEAD), Duration::from_secs(3)).unwrap();
    println!("head of {MAX_HEAD} bytes: {:?}", status(&at));
    assert_eq!(status(&at), "HTTP/1.1 200 OK");
    // One byte more: a 400, or (if the reset overtakes the answer) the connection ended without one.
    match ask(addr, &head(MAX_HEAD + 1), Duration::from_secs(3)) {
        Ok(a) => {
            println!("head of {} bytes: {:?}", MAX_HEAD + 1, status(&a));
            assert!(a.is_empty() || status(&a) == "HTTP/1.1 400 Bad Request", "{a}");
        }
        Err(e) => {
            println!("head of {} bytes: connection ended ({e})", MAX_HEAD + 1);
            assert!(!matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut), "{e}");
        }
    }

    let chunk = vec![b'a'; 64 * 1024];
    #[cfg(target_os = "linux")]
    let before = rss_kib();
    let mut c = TcpStream::connect(addr).unwrap();
    c.set_write_timeout(Some(Duration::from_secs(3))).unwrap();
    let mut sent = 0usize;
    while sent < (1 << 20) {
        match c.write(&chunk) {
            Ok(n) => sent += n,
            Err(_) => break,
        }
    }
    let ended = closed_within(&mut c, Duration::from_secs(3));
    println!("1 MiB without a newline: {sent} bytes written before the server ended it (after {ended:?} more)");
    assert!(ended.is_some(), "the server ends the connection");
    #[cfg(target_os = "linux")]
    {
        let after = rss_kib();
        println!("RSS {before} KiB -> {after} KiB");
        assert!(after < before + 4 * 1024, "RSS grew by {} KiB", after.saturating_sub(before));
    }
    assert_eq!(status(&get(addr, "/health").unwrap()), "HTTP/1.1 200 OK");
}

/// A client that sends one byte every 300 ms is cut off at [`DEADLINE`]; meanwhile `/health` answers.
#[test]
fn a_trickling_client_is_cut_off() {
    let addr = listener(healthy());
    let mut slow = TcpStream::connect(addr).unwrap();
    slow.set_write_timeout(Some(Duration::from_secs(1))).unwrap();
    let t = Instant::now();
    let mut written = 0;
    for b in b"GET /health HTTP/1.1\r\nHost: x\r\n\r\n".iter().take(20) {
        if slow.write_all(&[*b]).is_err() {
            break;
        }
        written += 1;
        if written == 3 {
            assert_eq!(status(&get(addr, "/health").unwrap()), "HTTP/1.1 200 OK");
        }
        thread::sleep(Duration::from_millis(300));
    }
    let ended = closed_within(&mut slow, Duration::from_secs(3));
    let total = t.elapsed();
    println!("trickling client: {written} bytes sent, ended {ended:?} after the last, {total:?} from the start");
    assert!(ended.is_some());
    assert!(total < DEADLINE + Duration::from_secs(4), "{total:?}");
}

/// Three times [`MAX_CONNECTIONS`] idle connections: those beyond the limit are closed at once, and `/health` answers
/// again once the deadline has ended the rest.
#[test]
fn more_connections_than_it_serves() {
    let addr = listener(healthy());
    let mut idle: Vec<TcpStream> = (0..3 * MAX_CONNECTIONS).map(|_| TcpStream::connect(addr).unwrap()).collect();
    thread::sleep(Duration::from_millis(300));
    let t = Instant::now();
    let answered = loop {
        match ask(addr, b"GET /health HTTP/1.1\r\n\r\n", Duration::from_millis(500)) {
            Ok(a) if status(&a) == "HTTP/1.1 200 OK" => break Some(t.elapsed()),
            _ if t.elapsed() > DEADLINE + Duration::from_secs(3) => break None,
            _ => thread::sleep(Duration::from_millis(100)),
        }
    };
    let quickly =
        idle.iter_mut().map(|c| closed_within(c, Duration::from_millis(100)).is_some()).filter(|x| *x).count();
    println!(
        "{} idle connections: /health answered after {answered:?}; {quickly} already closed by then",
        3 * MAX_CONNECTIONS
    );
    assert!(answered.is_some(), "the listener recovers");
    assert!(quickly >= 2 * MAX_CONNECTIONS - 1, "connections beyond the limit are not kept");
}

/// A snapshot no round has refreshed for longer than its limit answers `503`, and `200` once refreshed; before the
/// first refresh, `503`.
#[test]
fn a_stale_snapshot_is_unhealthy() {
    let s = Snapshot::default().with_stale_after(Duration::from_millis(400));
    let addr = listener(s.clone());
    assert_eq!(status(&get(addr, "/health").unwrap()), "HTTP/1.1 503 Service Unavailable", "nothing set yet");
    s.set(String::new(), "[]\n".into(), true);
    assert_eq!(status(&get(addr, "/health").unwrap()), "HTTP/1.1 200 OK");
    thread::sleep(Duration::from_millis(600));
    let stale = get(addr, "/health").unwrap();
    println!("after 600 ms without a refresh: {:?}", status(&stale));
    assert_eq!(status(&stale), "HTTP/1.1 503 Service Unavailable");
    assert!(stale.contains("X-Snapshot-Age-Seconds: 0\r\n"), "{stale}");
    s.set(String::new(), "[]\n".into(), true);
    assert_eq!(status(&get(addr, "/health").unwrap()), "HTTP/1.1 200 OK");
    s.set(String::new(), "[]\n".into(), false);
    assert_eq!(status(&get(addr, "/health").unwrap()), "HTTP/1.1 503 Service Unavailable");
}
