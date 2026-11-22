//! The limits of `policy.min_distinct_clients`, from an independent review of 18 November 2026, kept as tests: what
//! the policy does not do (`finalized-header` sends what one light-client source served; a `headers` answer is read
//! for `canonical`, not `finalized`; a client's name is the provider's text), and two of its fixes (the configuration
//! refuses one host twice; `/health` reports waiting for clients). Helpers as in `recorded.rs`.
#![cfg(feature = "harness")]

use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

use ferrata_core::{witness::CompactStore, Root};
use ferrata_lez_wire::Config;
use ferrata_updater::{
    chain::{decode_finality_update, decode_update, preset, root},
    config::{ClientMatch, Fees, File, Policy},
    ledger::HarnessLedger,
    source::{
        client_family, lab_record, load_entries, parse_header_answer, parse_version_answer, BeaconApi, Clock, Entry,
        HeaderWitness, Recorded, Source,
    },
    updater::{Instance, Plan, Step, Updater},
};
use lee::PrivateKey;

fn key(b: u8) -> PrivateKey {
    PrivateKey::try_new([b; 32]).unwrap()
}
fn no_sleep(_: u64) {}

struct Rec {
    name: &'static str,
    config: Config,
    dir: PathBuf,
    checkpoint: Root,
    entries: Vec<Entry>,
}

fn rec(name: &'static str) -> Rec {
    let config = preset(name, 2).unwrap();
    let dir = lab_record(name);
    let checkpoint = root(&fs::read_to_string(dir.join("checkpoint")).unwrap()).unwrap();
    let entries = load_entries(&dir.join("updates"), &config).unwrap();
    Rec { name, config, dir, checkpoint, entries }
}

impl Rec {
    fn source(&self, label: &str, clock: &Clock) -> Recorded {
        Recorded::open(label, &self.dir, &self.config, clock.clone()).unwrap()
    }
    fn reporting(&self, label: &str, clock: &Clock, client: &str, ahead: usize) -> Recorded {
        let mut s = self.source(label, clock);
        s.client = Some(version(client));
        s.ahead = ahead;
        s
    }
    fn updater(&self, label: &str, payer: PrivateKey, sources: Vec<Box<dyn Source>>, policy: Policy) -> Updater {
        let inst = Instance::new(self.name, HarnessLedger::program(), self.config.clone(), self.checkpoint);
        let mut u = Updater::new(label, inst, payer, Fees::default(), policy, sources, None).unwrap();
        u.sleep = no_sleep;
        u
    }
    fn last_finalized(&self) -> u64 {
        self.entries.iter().map(|e| e.finalized_slot).max().unwrap()
    }
}

fn diverse(k: usize, m: ClientMatch) -> Policy {
    Policy { defer_window: 1, retry_base_ms: 0, min_distinct_clients: k, client_match: m, ..Policy::default() }
}
fn store(u: &Updater, l: &HarnessLedger) -> CompactStore {
    u.inst.store(l).unwrap().expect("active instance")
}
fn applied(u: &Updater, l: &HarnessLedger) -> u64 {
    u.inst.state(l).unwrap().map_or(0, |s| s.applied)
}
fn refused_total(l: &HarnessLedger) -> usize {
    l.log.iter().filter(|(_, _, o)| !o.applied).count()
}
fn answer(client: &str) -> String {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/node-version");
    fs::read_to_string(dir.join(format!("{client}.json"))).unwrap()
}
fn version(client: &str) -> String {
    parse_version_answer(&answer(client)).unwrap()
}

/// Rounds until Waiting/UpToDate; the finalized slots of the jobs opened (Step::Open) on the way.
fn run(u: &mut Updater, l: &mut HarnessLedger, max: usize) -> (Vec<u64>, Option<Plan>) {
    let mut opened = Vec::new();
    for _ in 0..max {
        let p = u.tick(l).unwrap();
        match &p {
            Plan::Send(a) if a.step == Step::Open => opened.push(a.finalized_slot),
            Plan::UpToDate | Plan::Waiting(_) => return (opened, Some(p)),
            _ => {}
        }
    }
    (opened, None)
}

fn finalized_roots(r: &Rec) -> BTreeMap<u64, Root> {
    r.entries
        .iter()
        .map(|e| {
            let (u, _) = if e.period_update {
                decode_update(&e.bytes, &r.config)
            } else {
                decode_finality_update(&e.bytes, &r.config)
            }
            .unwrap();
            (u.finalized_header.beacon.slot, u.finalized_header.beacon.hash_tree_root())
        })
        .collect()
}

fn respond(stream: &mut std::net::TcpStream, status: &str, ctype: &str, body: &[u8]) {
    use std::io::Write;
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(body);
}

fn read_path(stream: &std::net::TcpStream) -> Option<String> {
    use std::io::{BufRead, BufReader};
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).is_err() || h == "\r\n" || h.is_empty() {
            break;
        }
    }
    Some(line.split_whitespace().nth(1).unwrap_or("/").to_string())
}

/// One provider behind a load balancer: the light-client data from ONE backend (the record), while
/// `/eth/v1/node/version` is answered round-robin from `versions` (backends of several clients).
fn mock_provider_lb(rec: Recorded, versions: Vec<String>) -> String {
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let rec = Arc::new(Mutex::new(rec));
    let next = Arc::new(AtomicUsize::new(0));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let Some(path) = read_path(&stream) else { continue };
            let mut r = rec.lock().unwrap();
            let body: Option<Vec<u8>> = if path == "/eth/v1/node/version" {
                let i = next.fetch_add(1, Ordering::SeqCst);
                Some(versions[i % versions.len()].clone().into_bytes())
            } else if let Some(root) = path.strip_prefix("/eth/v1/beacon/light_client/bootstrap/0x") {
                r.bootstrap(&ferrata_updater::chain::root(root).unwrap()).ok()
            } else if path == "/eth/v1/beacon/light_client/finality_update" {
                r.finality_update().ok()
            } else if let Some(q) = path.strip_prefix("/eth/v1/beacon/light_client/updates?") {
                let arg = |k: &str| {
                    q.split('&').find_map(|kv| kv.strip_prefix(k)).and_then(|v| v.parse::<u64>().ok()).unwrap()
                };
                let list = r.updates(arg("start_period="), arg("count=")).unwrap();
                let mut out = Vec::new();
                for u in list {
                    out.extend_from_slice(&((u.len() + 4) as u64).to_le_bytes());
                    out.extend_from_slice(&[0xab, 0xcd, 0xef, 0x01]);
                    out.extend_from_slice(&u);
                }
                Some(out)
            } else {
                None
            };
            match body {
                Some(b) => respond(&mut stream, "200 OK", "application/octet-stream", &b),
                None => respond(&mut stream, "404 Not Found", "application/json", b"{\"code\":404}"),
            }
        }
    });
    format!("http://{addr}")
}

/// A `headers` node (JSON, as publicnode's) holding the record's finalized headers canonical, reporting `version`.
fn mock_headers(roots: BTreeMap<u64, Root>, version: String) -> String {
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let Some(path) = read_path(&stream) else { continue };
            if path == "/eth/v1/node/version" {
                respond(&mut stream, "200 OK", "application/json", version.as_bytes());
                continue;
            }
            let slot = path.strip_prefix("/eth/v1/beacon/headers/").and_then(|s| s.parse::<u64>().ok());
            match slot.and_then(|s| roots.get(&s).map(|r| (s, *r))) {
                Some((s, r)) => {
                    let body = format!(
                        "{{\"execution_optimistic\":false,\"finalized\":true,\"data\":{{\"root\":\"0x{}\",\"canonical\":true,\
                         \"header\":{{\"message\":{{\"slot\":\"{s}\"}}}}}}}}",
                        hex::encode(r)
                    );
                    respond(&mut stream, "200 OK", "application/json", body.as_bytes());
                }
                None => respond(&mut stream, "404 Not Found", "application/json", b"{\"code\":404}"),
            }
        }
    });
    format!("http://{addr}")
}

/// `client_match = "finalized-header"`: an update that ONE light-client source serves (its attested header, sync
/// aggregate and, for a period update, its next sync committee seen by no other source) is sent when a `headers` node
/// of another client holds its finalized header canonical. Same sources, `bytes`: held back. (trust.md §2.4 says "a
/// forged update that one client's node serves (a bug, or a captured committee's accomplice) is not sent", without
/// restricting it to `bytes`; now it does.)
#[test]
fn finalized_header_mode_sends_what_one_light_client_source_served() {
    let r = rec("sepolia");
    let n = r.entries.len();
    let head = |upto: usize| r.entries[..upto].iter().map(|e| e.finalized_slot).max().unwrap();
    let (honest_head, ahead_head) = (head(n - 4), head(n - 1));
    let ahead: Vec<(usize, bool, u64)> =
        (n - 4..n - 1).map(|i| (r.entries[i].seq, r.entries[i].period_update, r.entries[i].finalized_slot)).collect();
    println!("C1: nimbus serves entries 1..={} (head {honest_head}); lodestar-ahead also (seq, period?, finalized) {ahead:?} (head {ahead_head})", n - 4);
    let roots = finalized_roots(&r);
    let mut held = BTreeMap::new();
    for (mode, pk) in [(ClientMatch::Bytes, 0x61u8), (ClientMatch::FinalizedHeader, 0x62)] {
        let clock = Clock::new(n - 4);
        let payer = key(pk);
        let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
        let witness = HeaderWitness {
            name: "lighthouse-headers".into(),
            base: mock_headers(roots.clone(), answer("lighthouse")),
            timeout_secs: 5,
        };
        let sources: Vec<Box<dyn Source>> = vec![
            Box::new(r.reporting("nimbus", &clock, "nimbus", 0)),
            Box::new(r.reporting("lodestar-ahead", &clock, "lodestar", 3)),
            Box::new(witness),
        ];
        let mut u = r.updater("c1", payer, sources, diverse(2, mode));
        let (opened, last) = run(&mut u, &mut l, 400);
        let at = store(&u, &l).finalized.slot;
        let only_one: Vec<u64> = opened.iter().copied().filter(|s| *s > honest_head).collect();
        println!(
            "C1 {mode:?}: store finalized {at}; opened {} jobs; opened beyond what nimbus serves (only lodestar-ahead \
             served them): {only_one:?}; refused {}; last plan {last:?}",
            opened.len(),
            refused_total(&l)
        );
        assert_eq!(refused_total(&l), 0);
        held.insert(format!("{mode:?}"), (at, only_one));
    }
    assert!(held["Bytes"].0 <= honest_head, "bytes holds back: {held:?}");
    assert!(
        held["FinalizedHeader"].0 > honest_head && !held["FinalizedHeader"].1.is_empty(),
        "finalized-header follows the single light-client source: {held:?}"
    );
}

/// One provider, listed twice (two names, the same URL), behind a load balancer whose `/eth/v1/node/version` answers
/// come from a Nimbus and a Lodestar backend in turn, while every light-client byte comes from one backend: the updater
/// itself cannot tell, and `min_distinct_clients = 2` (bytes) is met. The configuration refuses the same host twice
/// under the policy (two hosts of one provider it cannot tell apart either).
#[test]
fn one_provider_listed_twice_is_refused_by_the_configuration() {
    let r = rec("mainnet");
    let clock = Clock::new(r.entries.len());
    let url = mock_provider_lb(r.source("one-backend", &clock), vec![answer("nimbus"), answer("lodestar")]);
    let payer = key(0x63);
    let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
    let sources: Vec<Box<dyn Source>> = vec![
        Box::new(BeaconApi { name: "provider-a".into(), base: url.clone(), timeout_secs: 5 }),
        Box::new(BeaconApi { name: "provider-b".into(), base: url.clone(), timeout_secs: 5 }),
    ];
    let mut u = r.updater("c2", payer, sources, diverse(2, ClientMatch::Bytes));
    let (opened, last) = run(&mut u, &mut l, 300);
    let m = &u.metrics.sources;
    println!(
        "C2: one backend, same URL twice: store finalized {} (record's end {}), {} jobs opened, applied {}, last {last:?}; \
         clients a={} b={}; client_changes a={} b={}; health {:?}",
        store(&u, &l).finalized.slot,
        r.last_finalized(),
        opened.len(),
        applied(&u, &l),
        m["provider-a"].client,
        m["provider-b"].client,
        m["provider-a"].client_changes,
        m["provider-b"].client_changes,
        u.health()
    );
    assert_eq!(store(&u, &l).finalized.slot, r.last_finalized(), "two 'clients' agreed: one backend");
    assert!(matches!(last, Some(Plan::UpToDate)));

    // The configuration refuses it (dev.toml with both sources set to one URL; then one host, another port and path).
    let base = fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("dev/dev.toml")).unwrap();
    let rec_a = "name = \"record-a\"\n  dir = \"../../lab/results/record/net/mainnet\"";
    let rec_b = "name = \"record-b\"\n  dir = \"../../lab/results/record/net/mainnet\"";
    let text = base
        .replacen("max_periods_per_request = 16", "max_periods_per_request = 16\nmin_distinct_clients = 2", 1)
        .replacen(rec_a, "name = \"record-a\"\n  url = \"https://lodestar-mainnet.chainsafe.io\"", 1)
        .replacen(rec_b, "name = \"record-b\"\n  url = \"https://lodestar-mainnet.chainsafe.io\"", 1);
    assert_ne!(text, base);
    let parsed = File::parse(&text);
    println!(
        "C2: the same url twice with min_distinct_clients = 2: {:#}",
        parsed.as_ref().err().map_or("accepted".to_string(), |e| format!("{e:#}"))
    );
    assert!(parsed.is_err(), "the same url twice must be refused under the policy");
    let other_port = text.replacen(
        "url = \"https://lodestar-mainnet.chainsafe.io\"",
        "url = \"https://LODESTAR-mainnet.chainsafe.io:8443/eth\"",
        1,
    );
    assert_ne!(other_port, text);
    assert!(File::parse(&other_port).is_err(), "one host on another port and path is the same host");
    let off = text.replacen("min_distinct_clients = 2", "min_distinct_clients = 0", 1);
    assert!(File::parse(&off).is_ok(), "without the policy, a duplicate is the operator's business");
}

/// A `headers` source "agrees" on a block its node holds canonical but NOT finalized: the answer's
/// `finalized: false` is not read. With `client_match = "finalized-header"` it counts as a client agreeing on the
/// update's *finalized* header. Also: the family is the provider's text, so one implementation formatted two ways
/// is two clients.
#[test]
fn headers_answers_count_canonical_blocks_and_client_names_are_the_providers_text() {
    let root = format!("0x{}", "11".repeat(32));
    let body = format!(
        "{{\"execution_optimistic\":false,\"finalized\":false,\"data\":{{\"root\":\"{root}\",\"canonical\":true,\
         \"header\":{{\"message\":{{\"slot\":\"100\"}}}}}}}}"
    );
    let got = parse_header_answer(&body, 100).unwrap();
    println!("C3: headers answer with finalized=false, canonical=true -> {:?}", got.map(hex::encode));
    assert!(got.is_some(), "an unfinalized canonical block confirms");
    let a = parse_version_answer("{\"data\":{\"version\":\"Nimbus/v26.10.2\"}}").unwrap();
    let b = parse_version_answer("{\"data\":{\"version\":\"Nimbus /v26.10.2\"}}").unwrap();
    let c = parse_version_answer("{\"data\":{\"version\":\"nimbus-eth2/v26.10.2\"}}").unwrap();
    let fams = (client_family(&a), client_family(&b), client_family(&c));
    println!("C3: families {fams:?}");
    assert_ne!(fams.0, fams.1);
    assert_ne!(fams.0, fams.2);
}

/// When the configured sources cannot meet the policy (here both report one client), the updater waits for ever after
/// `Init`; `/health` says so (degraded) once the furthest verified update is more than `max_lag_slots` past the
/// instance, and `down` once the instance has not moved for `Updater::stall_down_rounds` rounds.
#[test]
fn waiting_for_clients_is_reported_in_health() {
    let r = rec("mainnet");
    let clock = Clock::new(r.entries.len());
    let payer = key(0x64);
    let mut l = HarnessLedger::new(std::slice::from_ref(&payer));
    let sources: Vec<Box<dyn Source>> = vec![
        Box::new(r.reporting("lodestar-a", &clock, "lodestar", 0)),
        Box::new(r.reporting("lodestar-b", &clock, "lodestar", 0)),
    ];
    let mut u = r.updater("c4", payer, sources, diverse(2, ClientMatch::Bytes));
    let mut waiting = 0;
    let mut seen = Vec::new();
    for _ in 0..60 {
        if let Plan::Waiting(_) = u.tick(&mut l).unwrap() {
            waiting += 1;
        }
        seen.push(u.health());
    }
    let waits = |why: &[String]| why.iter().any(|w| w.starts_with("waiting for clients"));
    let first = seen.iter().position(|(_, why)| waits(why)).expect("waiting for clients is reported");
    let first_down = seen.iter().position(|(s, _)| *s == "down");
    let (status, why) = u.health();
    println!(
        "C4: {waiting} of 60 rounds waiting, applied {}, store finalized {} (record's end {}); first reported in round \
         {} ({:?}), down from round {:?}; health {status} {why:?}; health_json status {}, last_plan {}",
        applied(&u, &l),
        store(&u, &l).finalized.slot,
        r.last_finalized(),
        first + 1,
        seen[first].0,
        first_down.map(|i| i + 1),
        u.health_json()["status"],
        u.health_json()["last_plan"]
    );
    assert!(waiting >= 55);
    assert_eq!(seen[first].0, "degraded", "{:?}", seen[first].1);
    // Waiting round after round moves nothing: `down` once the instance has not moved for twice the stall alarm's
    // rounds (the review of the fixes, M1).
    assert!(first_down.is_some_and(|d| d > first), "{first_down:?}");
    assert_eq!(status, "down", "{why:?}");
    assert!(waits(&why), "{why:?}");
    assert!(why.iter().any(|w| w.starts_with("no progress")), "{why:?}");
    assert_eq!(u.health_json()["status"], "down");
    assert!(u.render_metrics().contains("ferrata_updater_client_wait_slots{daemon=\"c4\",chain=\"mainnet\"}"));
}
