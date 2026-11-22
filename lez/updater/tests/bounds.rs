//! Bounds on what a source may answer (review-code-lc UP-M2: a 256 MiB answer was accepted and kept, RSS +263 MB):
//! `curl` answers read through [`fetch`] at most to their bound, with or without a `Content-Length`; the same through
//! `BeaconApi` and `HeaderWitness`; `beacon_api_module`'s reply text and the provider's messages bounded. A loopback
//! HTTP server stands in for a beacon node. The tests take one lock: the memory measurements must not see another
//! test's allocations.

use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::{Mutex, OnceLock},
    time::Instant,
};

use ferrata_updater::source::{
    clip, fetch, set_beacon_api_module, BeaconApi, BeaconApiModule, HeaderWitness, Source, ANSWERED_KEPT,
    MAX_JSON_BYTES, MAX_MESSAGE_CHARS, MAX_MODULE_REPLY_BYTES, MAX_OBJECT_BYTES, MAX_UPDATES_BYTES,
};

static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

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
                            return;
                        }
                        left -= k;
                    }
                });
            }
        });
        format!("http://{addr}")
    })
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
    #[cfg(target_os = "linux")]
    let before = rss_kib();
    let t = Instant::now();
    let huge = src(&format!("/close/{}", 256usize << 20)).finality_update();
    let took = t.elapsed();
    println!(
        "256 MiB finality update without a length: {:?} in {took:?}",
        huge.as_ref().map(Vec::len).map_err(|e| format!("{e:#}"))
    );
    assert!(huge.is_err());
    #[cfg(target_os = "linux")]
    {
        let after = rss_kib();
        println!("RSS {before} KiB -> {after} KiB (bound {} KiB)", MAX_OBJECT_BYTES / 1024);
        assert!(after < before + 8 * 1024, "RSS grew by {} KiB", after.saturating_sub(before));
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
    assert_eq!(clip("a\tb\u{7f}c"), "a_b_c");
}
