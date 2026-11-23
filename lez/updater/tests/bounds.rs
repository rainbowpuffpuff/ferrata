//! Bounds on what a source may answer (review-code-lc UP-M2: a 256 MiB answer was accepted and kept, RSS +263 MB):
//! `curl` answers read through [`fetch`] at most to their bound, with or without a `Content-Length`; the same through
//! `BeaconApi` and `HeaderWitness`; `beacon_api_module`'s reply text and the provider's messages bounded. A loopback
//! HTTP server stands in for a beacon node. The tests take one lock: the memory measurements must not see another
//! test's allocations.

use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use ferrata_updater::source::{
    clip, fetch, set_beacon_api_module, BeaconApi, BeaconApiModule, HeaderWitness, Source, ANSWERED_KEPT,
    MAX_JSON_BYTES, MAX_MESSAGE_CHARS, MAX_MODULE_REPLY_BYTES, MAX_OBJECT_BYTES, MAX_UPDATES_BYTES,
};

static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// Per request path, the bytes the test server pushed before the answer ended or the client went away.
static PUSHED: Mutex<BTreeMap<String, usize>> = Mutex::new(BTreeMap::new());

/// The bytes the server pushed for the request whose path starts with `prefix`, once its connection has ended (at most
/// 30 s).
fn pushed(prefix: &str) -> Option<usize> {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(30) {
        let found = PUSHED.lock().unwrap().iter().find(|(p, _)| p.starts_with(prefix)).map(|(_, n)| *n);
        if found.is_some() {
            return found;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

fn lock() -> std::sync::MutexGuard<'static, ()> {
    ONE_AT_A_TIME.lock().unwrap_or_else(|p| p.into_inner())
}

/// A loopback server answering `GET /len/N/...` with N bytes and a `Content-Length`, `GET /close/N/...` with N bytes
/// and no length (the end of the answer is the end of the connection), whatever follows N (a source appends its
/// endpoint's path). Returns its base URL.
fn server() -> &'static str {
    static BASE: OnceLock<String> = OnceLock::new();
    BASE.get_or_init(|| {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let chunk = std::sync::Arc::new(vec![b'z'; 1 << 20]);
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { continue };
                let chunk = chunk.clone();
                std::thread::spawn(move || {
                    let mut r = BufReader::new(s.try_clone().unwrap());
                    let mut line = String::new();
                    let _ = r.read_line(&mut line);
                    loop {
                        let mut h = String::new();
                        if r.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                            break;
                        }
                    }
                    let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                    let n: usize = path.split('/').nth(2).and_then(|p| p.parse().ok()).unwrap_or(0);
                    let length =
                        if path.starts_with("/len/") { format!("Content-Length: {n}\r\n") } else { String::new() };
                    if write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\n{length}Connection: close\r\n\r\n")
                        .is_err()
                    {
                        return;
                    }
                    let mut left = n;
                    while left > 0 {
                        let k = left.min(chunk.len());
                        if s.write_all(&chunk[..k]).is_err() {
                            break;
                        }
                        left -= k;
                    }
                    PUSHED.lock().unwrap().insert(path, n - left);
                });
            }
        });
        format!("http://{addr}")
    })
}

/// A line of `/proc/self/status`, in KiB (`VmRSS`, `VmHWM`; `None` where the kernel does not report it).
fn status_kib(field: &str) -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|l| l.strip_prefix(field)?.strip_prefix(':'))
        .and_then(|v| v.split_whitespace().next()?.parse().ok())
}

/// The process's peak resident set so far (`VmHWM`), after resetting it to the current one where the kernel allows
/// (`/proc/self/clear_refs`, "5"): what an answer read whole would raise, even if freed before the test looks (9c L-U6:
/// the test read `VmRSS` after the call, which a read-everything-then-refuse implementation would also have passed).
fn peak_reset() -> (Option<u64>, bool) {
    let reset = std::fs::write("/proc/self/clear_refs", "5").is_ok();
    (status_kib("VmHWM"), reset)
}

/// The exact bound is read whole; one byte more is refused, whether the answer announces its length (curl refuses it
/// before reading) or not (curl is stopped past the bound).
#[test]
fn fetch_reads_exactly_its_bound() {
    let _g = lock();
    let base = server();
    let max = 1000;
    for kind in ["len", "close"] {
        let at = fetch(&format!("{base}/{kind}/{max}"), "application/octet-stream", &["--fail"], 10, max).unwrap();
        println!("{kind}: an answer of {max} bytes with a bound of {max}: ok {} len {}", at.ok, at.body.len());
        assert!(at.ok);
        assert_eq!(at.body.len(), max);
        let past = fetch(&format!("{base}/{kind}/{}", max + 1), "application/octet-stream", &["--fail"], 10, max);
        let err = past.err().map(|e| format!("{e:#}"));
        println!("{kind}: an answer of {} bytes with a bound of {max}: {err:?}", max + 1);
        assert!(err.as_deref().is_some_and(|e| e.contains("longer than 1000 bytes")), "{err:?}");
    }
    // Neither file:// nor any protocol but HTTP(S).
    let local = fetch("file:///proc/self/status", "application/octet-stream", &["--fail"], 10, max).unwrap();
    println!("file:// URL: ok {} ({})", local.ok, local.message);
    assert!(!local.ok && local.body.is_empty());
}

/// The review's P6, through the sources: a finality update, a bootstrap or an `updates` answer one byte past its bound
/// is refused, and a 256 MiB answer without a length is cut at the bound: the process does not grow by it.
#[test]
fn huge_answers_are_refused_without_growing_the_process() {
    let _g = lock();
    let base = server();
    let src = |path: &str| BeaconApi { name: "big".into(), base: format!("{base}{path}"), timeout_secs: 30 };
    for (what, n, ok) in [("at the bound", MAX_OBJECT_BYTES, true), ("one byte past", MAX_OBJECT_BYTES + 1, false)] {
        for kind in ["len", "close"] {
            let got = src(&format!("/{kind}/{n}")).finality_update();
            println!(
                "finality update {what} ({n} bytes, {kind}): {:?}",
                got.as_ref().map(Vec::len).map_err(|e| format!("{e:#}"))
            );
            assert_eq!(got.is_ok(), ok);
        }
    }
    let (before, reset) = peak_reset();
    let rss = status_kib("VmRSS");
    let t = Instant::now();
    let huge = src(&format!("/close/{}/huge", 256usize << 20)).finality_update();
    let took = t.elapsed();
    let peak = status_kib("VmHWM");
    let sent = pushed(&format!("/close/{}/huge", 256usize << 20)).expect("the server's count");
    println!(
        "256 MiB finality update without a length: {:?} in {took:?}; the server pushed {sent} bytes before curl was \
         stopped; peak RSS {before:?} KiB -> {peak:?} KiB (reset to the current {rss:?} KiB: {reset}; bound {} KiB)",
        huge.as_ref().map(Vec::len).map_err(|e| format!("{e:#}")),
        MAX_OBJECT_BYTES / 1024
    );
    assert!(huge.is_err());
    // Read whole, all 256 MiB would have left the server; a bounded reader stops curl past 1 MiB, and what the
    // sockets and the pipe hold between them is a few MiB.
    assert!(sent < 64 << 20, "the server pushed {sent} bytes");
    if let (Some(before), Some(peak)) = (before, peak) {
        assert!(peak < before + 8 * 1024, "the peak RSS grew by {} KiB", peak.saturating_sub(before));
    }
    let announced = src(&format!("/len/{}", 256usize << 20)).finality_update();
    println!(
        "256 MiB finality update with its length: {:?}",
        announced.as_ref().map(Vec::len).map_err(|e| format!("{e:#}"))
    );
    assert!(announced.is_err());
    let updates = src(&format!("/close/{}", MAX_UPDATES_BYTES + 1)).updates(1, 1);
    println!(
        "updates one byte past {MAX_UPDATES_BYTES}: {:?}",
        updates.as_ref().map(Vec::len).map_err(|e| format!("{e:#}"))
    );
    assert!(updates.is_err());
    // A headers source: its JSON answer past MAX_JSON_BYTES (and the status line curl adds).
    let mut h =
        HeaderWitness { name: "h".into(), base: format!("{base}/close/{}", MAX_JSON_BYTES + 17), timeout_secs: 30 };
    let past = h.canonical_root(1);
    println!("headers answer past {MAX_JSON_BYTES}: {:?}", past.as_ref().map_err(|e| format!("{e:#}")));
    assert!(past.is_err_and(|e| format!("{e:#}").contains("longer than")));
}

/// `beacon_api_module`: a reply text past [`MAX_MODULE_REPLY_BYTES`] is refused; a provider's message is kept to
/// [`MAX_MESSAGE_CHARS`] printable characters; the providers that answered are kept to the last [`ANSWERED_KEPT`].
#[test]
fn module_replies_and_messages_are_bounded() {
    let _g = lock();
    set_beacon_api_module(|_, endpoint, _, _, _| {
        match endpoint {
        "getLightClientFinalityUpdate" => Ok(" ".repeat(MAX_MODULE_REPLY_BYTES + 1)),
        "getLightClientBootstrap" => Ok(serde_json::json!({
            "ok": false, "code": "E\nX".repeat(100), "error": format!("{}\n\u{1b}[31mforged log line", "e".repeat(10_000))
        })
        .to_string()),
        _ => Ok(serde_json::json!({ "ok": true, "provider": "p".repeat(1000), "body": "{}" }).to_string()),
    }
    });
    let mut m = BeaconApiModule {
        name: "module".into(),
        chain_id: 1,
        provider: String::new(),
        timeout_secs: 5,
        answered: Vec::new(),
    };
    let big = m.finality_update();
    println!("a reply of {} bytes: {:?}", MAX_MODULE_REPLY_BYTES + 1, big.as_ref().map_err(|e| format!("{e:#}")));
    assert!(big.is_err_and(|e| format!("{e:#}").contains("longer than")));
    let msg = format!("{:#}", m.bootstrap(&[0u8; 32]).unwrap_err());
    println!(
        "a provider's 10,000-character error with a newline and an escape: {} characters: {msg:.120}…",
        msg.chars().count()
    );
    assert!(msg.chars().count() < 3 * MAX_MESSAGE_CHARS + 200, "{}", msg.len());
    assert!(!msg.contains('\n') && !msg.contains('\u{1b}'));
    for _ in 0..3 * ANSWERED_KEPT {
        let _ = m.updates(1, 1);
    }
    println!(
        "after {} answers: {} providers kept, each {} characters",
        3 * ANSWERED_KEPT,
        m.answered.len(),
        m.answered[0].chars().count()
    );
    assert_eq!(m.answered.len(), ANSWERED_KEPT);
    assert!(m.answered.iter().all(|p| p.chars().count() <= MAX_MESSAGE_CHARS + 1));
    // A source that names its provider takes no other provider's answer (beacon_api_module failed over).
    m.provider = "lodestar".into();
    let other = m.updates(1, 1);
    println!("an answer from another provider: {:?}", other.as_ref().map(|_| ()).map_err(|e| format!("{e:#}")));
    assert!(other.is_err_and(|e| format!("{e:#}").contains("not lodestar")));
    assert_eq!(m.answered.len(), ANSWERED_KEPT, "a refused answer is not counted");
    assert_eq!(clip("a\tb\u{7f}c"), "a_b_c");
}
