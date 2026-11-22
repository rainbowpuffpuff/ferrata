//! The `/health` and `/metrics` listener against clients that misbehave (review-code-ops M3; review-code-lc UP-M2):
//! one idle connection, a request with no end, a client that trickles, more connections than it serves, clients that
//! keep re-opening idle connections while a monitor polls (the review of the fixes, L1), and a snapshot no round
//! refreshes. Loopback only; no chain.

use std::{
    io::{ErrorKind, Read, Write},
    net::{SocketAddr, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use ferrata_updater::serve::{serve, Snapshot, DEADLINE, MAX_CONNECTIONS, MAX_HEAD, MAX_QUICK, QUICK_DEADLINE};

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

/// A line of `/proc/self/status`, in KiB (`VmRSS`, `VmHWM`; `None` where the kernel does not report it).
fn status_kib(field: &str) -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|l| l.strip_prefix(field)?.strip_prefix(':'))
        .and_then(|v| v.split_whitespace().next()?.parse().ok())
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

/// A request head of exactly [`MAX_HEAD`] bytes is answered; one byte more is refused with `400` (and the client
/// receives it: the server reads what is still coming before it closes); a client offering 64 MiB with no newline is
/// cut off after a few MiB (what the sockets hold) and the process's peak resident set does not grow by them (the
/// review measured +257 MiB for 256 MiB). 9c L-U6: the test read `VmRSS` after the connection, which a server that
/// read everything and then refused would also have passed; now the peak (`VmHWM`, reset first where the kernel
/// allows) and the bytes the client got through.
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
    let over = ask(addr, &head(MAX_HEAD + 1), Duration::from_secs(3)).expect("an answer, not a reset");
    println!("head of {} bytes: {:?}", MAX_HEAD + 1, status(&over));
    assert_eq!(status(&over), "HTTP/1.1 400 Bad Request", "{over}");

    let chunk = vec![b'a'; 64 * 1024];
    let reset = std::fs::write("/proc/self/clear_refs", "5").is_ok();
    let before = status_kib("VmHWM");
    let mut c = TcpStream::connect(addr).unwrap();
    c.set_write_timeout(Some(Duration::from_secs(3))).unwrap();
    let mut sent = 0usize;
    while sent < (64 << 20) {
        match c.write(&chunk) {
            Ok(n) => sent += n,
            Err(_) => break,
        }
    }
    let ended = closed_within(&mut c, Duration::from_secs(3));
    let peak = status_kib("VmHWM");
    println!(
        "64 MiB offered without a newline: {sent} bytes written before the server ended it (after {ended:?} more); peak          RSS {before:?} KiB -> {peak:?} KiB (reset first: {reset})"
    );
    assert!(ended.is_some(), "the server ends the connection");
    assert!(sent < 32 << 20, "{sent} bytes got through: the server read on");
    if let (Some(before), Some(peak)) = (before, peak) {
        assert!(peak < before + 4 * 1024, "the peak RSS grew by {} KiB", peak.saturating_sub(before));
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

/// Three times [`MAX_CONNECTIONS`] idle connections: the first [`MAX_CONNECTIONS`] are served (each closed at
/// [`DEADLINE`]), the next [`MAX_QUICK`] are closed at [`QUICK_DEADLINE`], the rest at once; meanwhile `/health`
/// answers at once (it used to wait for the deadline to end the idle ones: 1.7 s).
#[test]
fn more_connections_than_it_serves() {
    let addr = listener(healthy());
    let n = 3 * MAX_CONNECTIONS;
    let mut idle: Vec<TcpStream> = (0..n).map(|_| TcpStream::connect(addr).unwrap()).collect();
    thread::sleep(QUICK_DEADLINE + Duration::from_millis(50));
    let t = Instant::now();
    let answer = ask(addr, b"GET /health HTTP/1.1\r\n\r\n", Duration::from_secs(3));
    let took = t.elapsed();
    let closed: Vec<bool> = idle.iter_mut().map(|c| closed_within(c, Duration::from_millis(100)).is_some()).collect();
    let quickly = closed.iter().filter(|x| **x).count();
    println!(
        "{n} idle connections: /health {:?} after {took:?}; {quickly} already closed ({} of the first {MAX_CONNECTIONS})",
        answer.as_deref().map(status),
        closed[..MAX_CONNECTIONS].iter().filter(|x| **x).count()
    );
    assert_eq!(answer.as_deref().map(status).ok(), Some("HTTP/1.1 200 OK"));
    assert!(took < Duration::from_secs(1), "answered at once, not after the deadline: {took:?}");
    assert!(closed[..MAX_CONNECTIONS].iter().all(|x| !x), "the first ones are served until their deadline");
    assert_eq!(quickly, n - MAX_CONNECTIONS, "the others are not kept");
}

/// The review of the fixes, probe E (L1): [`MAX_CONNECTIONS`] clients each keep an idle connection open and open
/// another as soon as the server ends one, for 8 s, while a monitor asks for `/health` every 100 ms. It used to get no
/// answer in 80 tries. Now every try is answered.
#[test]
fn health_answers_while_clients_keep_reopening_idle_connections() {
    let addr = listener(healthy());
    let stop = Arc::new(AtomicBool::new(false));
    let clients: Vec<_> = (0..MAX_CONNECTIONS)
        .map(|_| {
            let stop = stop.clone();
            thread::spawn(move || {
                let mut opened = 0usize;
                while !stop.load(Ordering::SeqCst) {
                    let Ok(mut c) = TcpStream::connect(addr) else { continue };
                    opened += 1;
                    let _ = c.set_read_timeout(Some(Duration::from_millis(100)));
                    let mut b = [0u8; 64];
                    while !stop.load(Ordering::SeqCst) {
                        match c.read(&mut b) {
                            Ok(0) => break,
                            Ok(_) => {}
                            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                            Err(_) => break,
                        }
                    }
                }
                opened
            })
        })
        .collect();
    thread::sleep(Duration::from_millis(300));
    let (mut answered, mut tries, mut slowest) = (0, 0, Duration::ZERO);
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(8) {
        tries += 1;
        let asked = Instant::now();
        if ask(addr, b"GET /health HTTP/1.1\r\n\r\n", Duration::from_secs(1))
            .is_ok_and(|a| status(&a) == "HTTP/1.1 200 OK")
        {
            answered += 1;
            slowest = slowest.max(asked.elapsed());
        }
        thread::sleep(Duration::from_millis(100));
    }
    stop.store(true, Ordering::SeqCst);
    let opened: usize = clients.into_iter().map(|h| h.join().unwrap()).sum();
    println!(
        "{MAX_CONNECTIONS} clients re-opening idle connections for 8 s ({opened} connections): /health answered \
         {answered} of {tries} tries, the slowest in {slowest:?}"
    );
    assert!(opened >= 4 * MAX_CONNECTIONS, "the clients kept at it: {opened}");
    assert_eq!(answered, tries, "every try answered");
}

/// 400 connections at once: at most [`MAX_CONNECTIONS`] + [`MAX_QUICK`] are kept open, the rest are closed at once;
/// `/health` answers meanwhile and after.
#[test]
fn four_hundred_connections_at_once() {
    let addr = listener(healthy());
    let mut conns: Vec<TcpStream> = (0..400).filter_map(|_| TcpStream::connect(addr).ok()).collect();
    let opened = conns.len();
    let open = conns.iter_mut().map(|c| closed_within(c, Duration::from_millis(50)).is_none()).filter(|x| *x).count();
    let during = ask(addr, b"GET /health HTTP/1.1\r\n\r\n", Duration::from_secs(3));
    drop(conns);
    thread::sleep(DEADLINE + Duration::from_millis(500));
    let after = get(addr, "/health");
    println!(
        "400 connections at once: {opened} opened, {open} still open after the server's first answers; /health \
         meanwhile {:?}, after {:?}",
        during.as_deref().map(status),
        after.as_deref().map(status)
    );
    assert!(open <= MAX_CONNECTIONS + MAX_QUICK, "{open} kept open");
    assert_eq!(during.as_deref().map(status).ok(), Some("HTTP/1.1 200 OK"));
    assert_eq!(after.as_deref().map(status).ok(), Some("HTTP/1.1 200 OK"));
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
