//! The attestation side's monitor against real Sepolia data: the instance states of the R.7 stall test
//! (`lez/harness/tests/stall.rs`: the instance stalled at slot 11,549,664 and resumed at 11,608,192, written by
//! LEZ v0.4.0-rc1's state machine, `tests/fixtures/stall.json`), beacon providers on loopback serving the answers
//! Sepolia's public providers gave (publicnode's header at 11,549,664 from the attestation fixture, Lodestar's
//! genesis from `verify_checkpoint`'s recorded run, publicnode's root at 11,608,192 from the lab's record), and the
//! `attest-monitor` binary as an operator runs it.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ferrata_core::witness::CompactStore;
use ferrata_lez_wire::{instance_actor, InstanceState};
use lee_core::account::{Account, AccountId, ActorState};
use serde_json::{json, Value};

/// The binary: Cargo's `CARGO_BIN_EXE_attest-monitor`, else beside the test executable's `deps/`.
fn bin() -> PathBuf {
    option_env!("CARGO_BIN_EXE_attest-monitor").map(PathBuf::from).unwrap_or_else(|| {
        let exe = std::env::current_exe().unwrap();
        exe.parent().unwrap().parent().unwrap().join("attest-monitor")
    })
}

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn repo() -> PathBuf {
    manifest().join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let d = manifest().join("target/test-scratch").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

struct Fx {
    program: String,
    instance_id: String,
    stalled: Value,
    resumed: Value,
    ws_end: u64,
}

fn fx() -> Fx {
    let v: Value = serde_json::from_slice(&std::fs::read(manifest().join("tests/fixtures/stall.json")).unwrap()).unwrap();
    Fx {
        program: v["program"].as_str().unwrap().into(),
        instance_id: v["instance_id"].as_str().unwrap().into(),
        stalled: v["stalled"].clone(),
        resumed: v["resumed"].clone(),
        ws_end: v["ws_end_slot_of_stalled"].as_u64().unwrap(),
    }
}

fn state_of(v: &Value) -> InstanceState {
    borsh::from_slice(&hex::decode(v["instance_state"].as_str().unwrap().trim_start_matches("0x")).unwrap()).unwrap()
}

fn write_state(dir: &Path, name: &str, s: &InstanceState) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, format!("0x{}\n", hex::encode(borsh::to_vec(s).unwrap()))).unwrap();
    p
}

/// Lodestar's `/eth/v1/beacon/genesis` answer for Sepolia, as `verify_checkpoint`'s live run recorded it.
fn sepolia_genesis() -> String {
    let rec: Value = serde_json::from_slice(
        &std::fs::read(repo().join("modules/ferrata_attest_module/rust-lib/calls/tests/fixtures/verify-checkpoint-sepolia.record.json"))
            .unwrap(),
    )
    .unwrap();
    rec.as_array()
        .unwrap()
        .iter()
        .find(|r| r["url"].as_str().is_some_and(|u| u.ends_with("/eth/v1/beacon/genesis")))
        .map(|r| r["response"].as_str().expect("the recorded answer").to_string())
        .expect("a recorded genesis answer")
}

/// publicnode's `/eth/v1/beacon/headers/11549664` answer, from the attestation fixture.
fn header_11549664() -> String {
    String::from_utf8(
        std::fs::read(repo().join("crates/ferrata-spectests/fixtures/sepolia-attest-2026-11-10/beacon_header_11549664_second.json"))
            .unwrap(),
    )
    .unwrap()
}

/// publicnode's root at 11,608,192, as the lab's session of 18 Nov 2026 20:17Z recorded it.
fn root_11608192() -> String {
    let v: Value = serde_json::from_slice(
        &std::fs::read(repo().join("lez/lab/results/sessions/20261118T201706Z/fetch-sepolia.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(v["finality_finalized_slot"].as_u64(), Some(11_608_192));
    v["second_provider_finalized_root"].as_str().unwrap().to_string()
}

fn header_json(root: &str, slot: u64) -> String {
    json!({ "execution_optimistic": false, "finalized": true,
            "data": { "root": root, "canonical": true, "header": { "message": { "slot": slot.to_string() } } } })
    .to_string()
}

/// The root in a header answer, with its last byte changed.
fn other_root(answer: &str) -> String {
    let v: Value = serde_json::from_str(answer).unwrap();
    let mut b = hex::decode(v["data"]["root"].as_str().unwrap().trim_start_matches("0x")).unwrap();
    b[31] ^= 0x5a;
    format!("0x{}", hex::encode(b))
}

type Routes = Arc<Mutex<HashMap<String, (u16, String)>>>;

/// A loopback HTTP server answering GETs from `routes` (404 otherwise) and POSTs with `post` (the request body in,
/// the answer out).
struct Server {
    url: String,
    routes: Routes,
}

impl Server {
    fn start(routes: Vec<(String, u16, String)>, post: Option<Arc<dyn Fn(&str) -> String + Send + Sync>>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let routes: Routes = Arc::new(Mutex::new(routes.into_iter().map(|(p, c, b)| (p, (c, b))).collect()));
        let r = routes.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let mut len = 0usize;
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).is_err() || h == "\r\n" || h.is_empty() {
                        break;
                    }
                    if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0; len];
                let _ = reader.read_exact(&mut body);
                let mut parts = line.split_whitespace();
                let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or("").to_string());
                let (code, text) = if method == "POST" {
                    match &post {
                        Some(f) => (200, f(&String::from_utf8_lossy(&body))),
                        None => (405, String::new()),
                    }
                } else {
                    r.lock().unwrap().get(&path).cloned().unwrap_or((404, json!({"code": 404, "message": "NOT_FOUND"}).to_string()))
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {code} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                    text.len()
                );
            }
        });
        Self { url, routes }
    }

    fn set(&self, path: &str, code: u16, body: String) {
        self.routes.lock().unwrap().insert(path.into(), (code, body));
    }
}

/// A beacon provider: `genesis`, the header at `slot` (`header` answer, or 404 when `None`), finalized at `finalized`.
fn provider(genesis: &str, slot: u64, header: Option<String>, finalized: u64) -> Server {
    let mut routes = vec![
        ("/eth/v1/beacon/genesis".to_string(), 200, genesis.to_string()),
        ("/eth/v1/beacon/headers/finalized".to_string(), 200, header_json("0x00", finalized)),
    ];
    if let Some(h) = header {
        routes.push((format!("/eth/v1/beacon/headers/{slot}"), 200, h));
    }
    Server::start(routes, None)
}

struct Out {
    code: i32,
    report: Value,
    stderr: String,
}

fn check(args: &[String]) -> Out {
    let out = Command::new(bin()).arg("check").args(args).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let report = serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("{e}: {stdout}\n{stderr}"));
    Out { code: out.status.code().unwrap_or(-1), report, stderr }
}

fn kinds(report: &Value) -> Vec<String> {
    report["alerts"].as_array().unwrap().iter().map(|a| a["kind"].as_str().unwrap().to_string()).collect()
}

fn args(state: &Path, providers: &[&Server], now_s: u64) -> Vec<String> {
    let mut a = vec!["--state".to_string(), format!("file:{}", state.display()), "--now".into(), now_s.to_string()];
    for (i, p) in providers.iter().enumerate() {
        a.extend(["--provider".to_string(), format!("p{i}={}", p.url)]);
    }
    a
}

fn secs(ms: &Value) -> u64 {
    ms.as_u64().unwrap() / 1000
}

#[test]
fn healthy_when_three_providers_confirm_the_finalized_header() {
    let f = fx();
    let d = scratch("healthy");
    let state = write_state(&d, "stalled.hex", &state_of(&f.stalled));
    let slot = f.stalled["finalized_slot"].as_u64().unwrap();
    let g = sepolia_genesis();
    let ps: Vec<Server> = (0..3).map(|_| provider(&g, slot, Some(header_11549664()), slot + 64)).collect();
    let o = check(&args(&state, &ps.iter().collect::<Vec<_>>(), secs(&f.stalled["time_ms"]) + 60));
    assert_eq!(o.code, 0, "{}\n{}", o.report, o.stderr);
    assert_eq!(o.report["divergence"], "confirmed");
    assert_eq!(kinds(&o.report), Vec::<String>::new());
    assert_eq!(o.report["finalized_slot"].as_u64(), Some(slot));
    assert_eq!(o.report["lag_s"].as_u64(), Some(64 * 12));
    println!("healthy: {}", o.report["providers"]);
}

#[test]
fn the_r7_stall_raises_the_stall_and_half_weak_subjectivity_alarms_and_the_resume_clears_them() {
    let f = fx();
    let d = scratch("stall");
    let (s0, s1) = (f.stalled["finalized_slot"].as_u64().unwrap(), f.resumed["finalized_slot"].as_u64().unwrap());
    let now = secs(&f.resumed["time_ms"]);
    let g = sepolia_genesis();
    // Ethereum has finalized 11,608,192; the instance still holds 11,549,664.
    let ps: Vec<Server> = (0..3).map(|_| provider(&g, s0, Some(header_11549664()), s1)).collect();
    let stalled = write_state(&d, "stalled.hex", &state_of(&f.stalled));
    let o = check(&args(&stalled, &ps.iter().collect::<Vec<_>>(), now));
    assert_eq!(o.code, 20, "{}", o.report);
    assert_eq!(o.report["divergence"], "confirmed", "a stalled instance is not a diverged one");
    assert_eq!(kinds(&o.report), vec!["instance-stalled", "ws-half"]);
    assert_eq!(o.report["lag_s"].as_u64(), Some((s1 - s0) * 12));
    println!("stalled: lag {} s, age {} s, ws half at slot {}, end at {}", o.report["lag_s"], o.report["age_s"], o.report["ws_half_slot"], o.report["ws_end_slot"]);

    // After the resume (the outstanding updates submitted at that time), the same providers: no alarm.
    for p in &ps {
        p.set(&format!("/eth/v1/beacon/headers/{s1}"), 200, header_json(&root_11608192(), s1));
    }
    let resumed = write_state(&d, "resumed.hex", &state_of(&f.resumed));
    let o = check(&args(&resumed, &ps.iter().collect::<Vec<_>>(), now));
    assert_eq!(o.code, 0, "{}", o.report);
    assert_eq!(o.report["divergence"], "confirmed", "the resumed header is publicnode's canonical root");
    assert_eq!(o.report["finalized_root"].as_str(), Some(root_11608192().as_str()));
}

#[test]
fn past_the_weak_subjectivity_period_the_instance_must_be_replaced() {
    let f = fx();
    let d = scratch("ws-expired");
    let s = state_of(&f.stalled);
    let slot = f.stalled["finalized_slot"].as_u64().unwrap();
    let g = sepolia_genesis();
    let ps: Vec<Server> = (0..2).map(|_| provider(&g, slot, Some(header_11549664()), f.ws_end + 10)).collect();
    let now = s.config.genesis_time + f.ws_end * s.config.seconds_per_slot + 60;
    let o = check(&args(&write_state(&d, "s.hex", &s), &ps.iter().collect::<Vec<_>>(), now));
    assert_eq!(o.code, 20);
    assert!(kinds(&o.report).contains(&"ws-expired".to_string()), "{}", o.report);
    assert!(!kinds(&o.report).contains(&"ws-half".to_string()));
}

/// What a captured committee leaves: the instance holds a finalized header no provider has at that slot.
fn forged(s: &InstanceState) -> InstanceState {
    let mut store = CompactStore::decode(&s.store).unwrap();
    store.finalized.state_root[0] ^= 1;
    InstanceState { store: store.encode(), ..s.clone() }
}

#[test]
fn a_forged_finalized_header_is_called_diverged_and_runs_the_consumers_freeze_hook() {
    let f = fx();
    let d = scratch("diverged");
    let slot = f.stalled["finalized_slot"].as_u64().unwrap();
    let g = sepolia_genesis();
    let ps: Vec<Server> = (0..3).map(|_| provider(&g, slot, Some(header_11549664()), slot + 64)).collect();
    let state = write_state(&d, "forged.hex", &forged(&state_of(&f.stalled)));
    let freeze = d.join("freeze-requests.jsonl");
    let log = d.join("alerts.jsonl");
    let mut a = args(&state, &ps.iter().collect::<Vec<_>>(), secs(&f.stalled["time_ms"]) + 60);
    a.extend([
        "--alert-cmd".into(),
        format!("cat >> {}", freeze.display()),
        "--alert-cmd-level".into(),
        "critical".into(),
        "--alert-log".into(),
        log.display().to_string(),
    ]);
    let o = check(&a);
    assert_eq!(o.code, 20, "{}", o.report);
    assert_eq!(o.report["divergence"], "diverged");
    assert_eq!(kinds(&o.report), vec!["instance-diverged"]);
    let lines: Vec<Value> =
        std::fs::read_to_string(&freeze).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(lines.len(), 1, "the freeze hook ran once, for the critical alert");
    assert_eq!(lines[0]["kind"], "instance-diverged");
    assert_eq!(lines[0]["instance"].as_str(), Some(f.instance_id.as_str()));
    assert_eq!(lines[0]["providers"].as_array().unwrap().len(), 3);
    assert_eq!(std::fs::read_to_string(&log).unwrap().lines().count(), 1);
    println!("freeze hook got: {}", lines[0]);
}

/// The forged instance's finalized root, as a provider colluding with the forger reports it.
fn forged_root(forged_state: &InstanceState) -> String {
    format!("0x{}", hex::encode(CompactStore::decode(&forged_state.store).unwrap().finalized.hash_tree_root()))
}

fn names(alert: &Value, field: &str) -> Vec<String> {
    alert[field].as_array().unwrap().iter().map(|n| n.as_str().unwrap().to_string()).collect()
}

#[test]
fn one_colluding_provider_cannot_hide_a_divergence_from_the_freeze_hook() {
    let f = fx();
    let d = scratch("colluder");
    let slot = f.stalled["finalized_slot"].as_u64().unwrap();
    let g = sepolia_genesis();
    let now = secs(&f.stalled["time_ms"]) + 60;
    let forged_state = forged(&state_of(&f.stalled));
    let fstate = write_state(&d, "f.hex", &forged_state);
    let honest = [
        provider(&g, slot, Some(header_11549664()), slot + 64),
        provider(&g, slot, Some(header_11549664()), slot + 64),
    ];
    let colluders = [
        provider(&g, slot, Some(header_json(&forged_root(&forged_state), slot)), slot + 64),
        provider(&g, slot, Some(header_json(&forged_root(&forged_state), slot)), slot + 64),
    ];
    // A captured committee's forged header, two honest providers and one that colludes with the forger, wired as
    // the README says (the freeze tool at level critical): the alarm is critical and the freeze tool runs, once.
    let freeze = d.join("freeze-requests.jsonl");
    let mut a = args(&fstate, &[&honest[0], &honest[1], &colluders[0]], now);
    a.extend(["--alert-cmd".into(), format!("cat >> {}", freeze.display()), "--alert-cmd-level".into(), "critical".into()]);
    let o = check(&a);
    assert_eq!(o.code, 20, "{}", o.report);
    assert_eq!(o.report["divergence"], "diverged");
    assert_eq!(kinds(&o.report), vec!["instance-diverged"]);
    let lines: Vec<Value> =
        std::fs::read_to_string(&freeze).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(lines.len(), 1, "the freeze hook ran once");
    assert_eq!(lines[0]["kind"], "instance-diverged");
    assert_eq!(names(&lines[0], "providers"), vec!["p0", "p1"], "the honest providers, which disagree");
    assert_eq!(names(&lines[0], "agreeing"), vec!["p2"], "the colluder, named");
    println!("colluder: freeze hook got {}", lines[0]);
    // Two colluders of three: one honest provider still raises it (the default, --diverge-at 1).
    let o = check(&args(&fstate, &[&honest[0], &colluders[0], &colluders[1]], now));
    assert_eq!((o.code, o.report["divergence"].as_str()), (20, Some("diverged")), "{}", o.report);
    // The stated limit: when every provider that answers colludes, the forgery is not visible to the monitor.
    let o = check(&args(&fstate, &[&colluders[0], &colluders[1]], now));
    assert_eq!((o.code, o.report["divergence"].as_str()), (0, Some("confirmed")), "{}", o.report);
}

#[test]
fn the_divergence_rule_one_liar_raises_it_by_default_and_not_with_diverge_at_2() {
    let f = fx();
    let d = scratch("liar");
    let slot = f.stalled["finalized_slot"].as_u64().unwrap();
    let g = sepolia_genesis();
    let now = secs(&f.stalled["time_ms"]) + 60;
    let honest = header_11549664();
    let lie = header_json(&other_root(&honest), slot);
    let ps = [
        provider(&g, slot, Some(honest.clone()), slot + 64),
        provider(&g, slot, Some(honest.clone()), slot + 64),
        provider(&g, slot, Some(lie), slot + 64),
    ];
    let state = write_state(&d, "s.hex", &state_of(&f.stalled));
    let with = |a: Vec<String>, n: &str| -> Vec<String> { a.into_iter().chain(["--diverge-at".to_string(), n.to_string()]).collect() };
    // A healthy instance, two honest providers and a liar. By default any disagreeing provider raises the alarm (a
    // false freeze costs a pause); the alert names the liar and the two that agree.
    let o = check(&args(&state, &ps.iter().collect::<Vec<_>>(), now));
    assert_eq!((o.code, o.report["divergence"].as_str()), (20, Some("diverged")), "{}", o.report);
    let alert = &o.report["alerts"][0];
    assert_eq!((names(alert, "providers"), names(alert, "agreeing")), (vec!["p2".to_string()], vec!["p0".to_string(), "p1".to_string()]));
    // --diverge-at 2: one liar cannot raise it, only a warning.
    let o = check(&with(args(&state, &ps.iter().collect::<Vec<_>>(), now), "2"));
    assert_eq!((o.code, o.report["divergence"].as_str()), (10, Some("contested")), "{}", o.report);
    assert_eq!(kinds(&o.report), vec!["providers-disagree"]);
    // --diverge-at 2 on a forged instance: one colluder of three cannot hide it (two honest still disagree) ...
    let forged_state = forged(&state_of(&f.stalled));
    let fstate = write_state(&d, "f.hex", &forged_state);
    let colluder = |_: usize| provider(&g, slot, Some(header_json(&forged_root(&forged_state), slot)), slot + 64);
    let (c0, c1) = (colluder(0), colluder(1));
    let o = check(&with(args(&fstate, &[&ps[0], &ps[1], &c0], now), "2"));
    assert_eq!((o.code, o.report["divergence"].as_str()), (20, Some("diverged")), "{}", o.report);
    // ... two of three can (f > n - diverge_at): the rule the README states.
    let o = check(&with(args(&fstate, &[&ps[0], &c0, &c1], now), "2"));
    assert_eq!((o.code, o.report["divergence"].as_str()), (10, Some("contested")), "{}", o.report);
    // --diverge-at 0 is refused (a usage error).
    let out = Command::new(bin()).arg("check").args(with(args(&state, &[&ps[0]], now), "0")).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    // A liar cannot raise the stall alarm alone: the stall is measured against the median of three finalized slots.
    let far = provider(&g, slot, Some(honest), slot + 1_000_000);
    let o = check(&args(&state, &[&ps[0], &ps[1], &far], now));
    assert!(!kinds(&o.report).contains(&"instance-stalled".to_string()), "{}", o.report);
}

#[test]
fn the_stall_resists_one_lying_provider_with_three_providers_not_with_two() {
    let f = fx();
    let d = scratch("stall-median");
    let (s0, s1) = (f.stalled["finalized_slot"].as_u64().unwrap(), f.resumed["finalized_slot"].as_u64().unwrap());
    let now = secs(&f.resumed["time_ms"]);
    let g = sepolia_genesis();
    let state = write_state(&d, "stalled.hex", &state_of(&f.stalled));
    // Honest providers have finalized 11,608,192; a liar claims the instance's own slot as its finalized one.
    let honest = [provider(&g, s0, Some(header_11549664()), s1), provider(&g, s0, Some(header_11549664()), s1)];
    let liar = provider(&g, s0, Some(header_11549664()), s0);
    let o = check(&args(&state, &[&honest[0], &honest[1], &liar], now));
    assert!(kinds(&o.report).contains(&"instance-stalled".to_string()), "three providers: the liar cannot hide it: {}", o.report);
    // With two providers the lower finalized slot counts: the liar hides the stall (the README's stated limit).
    let o = check(&args(&state, &[&honest[0], &liar], now));
    assert!(!kinds(&o.report).contains(&"instance-stalled".to_string()), "{}", o.report);
    assert_eq!(o.report["chain_finalized_slot"].as_u64(), Some(s0));
}

#[test]
fn unreachable_and_wrong_chain_providers_are_not_counted() {
    let f = fx();
    let d = scratch("unavailable");
    let slot = f.stalled["finalized_slot"].as_u64().unwrap();
    let g = sepolia_genesis();
    let honest = provider(&g, slot, Some(header_11549664()), slot + 64);
    let broken = provider(&g, slot, None, slot + 64);
    broken.set("/eth/v1/beacon/genesis", 503, "{}".into());
    // Mainnet's genesis, answering for the same slot with Sepolia's header: another chain, not counted.
    let mainnet_genesis = json!({"data": {"genesis_validators_root": "0x4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95",
        "genesis_time": "1606824023", "genesis_fork_version": "0x00000000"}})
    .to_string();
    let other_chain = provider(&mainnet_genesis, slot, Some(header_11549664()), slot + 64);
    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let down_url = format!("http://{}", closed.local_addr().unwrap());
    drop(closed);
    let state = write_state(&d, "s.hex", &state_of(&f.stalled));
    let mut a = args(&state, &[&honest, &broken, &other_chain], secs(&f.stalled["time_ms"]) + 60);
    a.extend(["--provider".into(), format!("down={down_url}"), "--timeout".into(), "3".into()]);
    let o = check(&a);
    assert_eq!(o.code, 10, "{}", o.report);
    assert_eq!(o.report["divergence"], "unconfirmed");
    let k = kinds(&o.report);
    assert_eq!(k.iter().filter(|k| *k == "provider-unavailable").count(), 2, "{k:?}");
    assert_eq!(k.iter().filter(|k| *k == "provider-wrong-chain").count(), 1, "{k:?}");
    assert!(k.contains(&"instance-unconfirmed".to_string()));
}

#[test]
fn a_provider_behind_the_instance_is_not_counted() {
    let f = fx();
    let d = scratch("behind");
    let slot = f.stalled["finalized_slot"].as_u64().unwrap();
    let g = sepolia_genesis();
    let now = secs(&f.stalled["time_ms"]) + 60;
    let state = write_state(&d, "s.hex", &state_of(&f.stalled));
    // One honest provider and two that have finalized only up to 32 slots before the instance's header: they answer
    // 404 at its slot (a block they have not seen), which is not a disagreement. Counted, they would have raised a
    // false divergence (critical) by default.
    let honest = provider(&g, slot, Some(header_11549664()), slot + 64);
    let lagging = [provider(&g, slot, None, slot - 32), provider(&g, slot, None, slot - 32)];
    // And one that answers the slot but not what it has finalized: not counted either.
    let no_finalized = provider(&g, slot, Some(header_11549664()), slot + 64);
    no_finalized.set("/eth/v1/beacon/headers/finalized", 404, "{}".into());
    let o = check(&args(&state, &[&honest, &lagging[0], &lagging[1], &no_finalized], now));
    assert_eq!((o.code, o.report["divergence"].as_str()), (10, Some("unconfirmed")), "{}", o.report);
    let k = kinds(&o.report);
    assert_eq!(k.iter().filter(|k| *k == "provider-behind").count(), 2, "{k:?}");
    assert_eq!(k.iter().filter(|k| *k == "provider-unavailable").count(), 1, "{k:?}");
    assert!(!k.contains(&"instance-diverged".to_string()), "{k:?}");
    // Once they have finalized past the slot and still have no block there, they do disagree: the instance's header is
    // not on their chain.
    for p in &lagging {
        p.set("/eth/v1/beacon/headers/finalized", 200, header_json("0x00", slot + 64));
    }
    let o = check(&args(&state, &[&honest, &lagging[0], &lagging[1]], now));
    assert_eq!((o.code, o.report["divergence"].as_str()), (20, Some("diverged")), "{}", o.report);
}

#[test]
fn reads_the_instance_from_a_lez_sequencers_get_account() {
    let f = fx();
    let program = AccountId::new(hex::decode(f.program.trim_start_matches("0x")).unwrap().try_into().unwrap());
    let id: [u8; 32] = hex::decode(f.instance_id.trim_start_matches("0x")).unwrap().try_into().unwrap();
    let actor = instance_actor(program, &id);
    let state_bytes = borsh::to_vec(&state_of(&f.stalled)).unwrap();
    // The account as LEZ holds it, with the program's actor state, served as the sequencer's `getAccount` serves it.
    let account = Account::default().with_actor_state(actor.program_account_id, ActorState::from(state_bytes));
    let want = serde_json::to_value(actor.account_id).unwrap();
    let calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let seen = calls.clone();
    let seq = Server::start(
        vec![],
        Some(Arc::new(move |body: &str| {
            let req: Value = serde_json::from_str(body).unwrap();
            seen.lock().unwrap().push(req.clone());
            let result = if req["method"] == "getAccount" && req["params"][0] == want { serde_json::to_value(Some(&account)).unwrap() } else { Value::Null };
            json!({"jsonrpc": "2.0", "id": req["id"], "result": result}).to_string()
        })),
    );
    let slot = f.stalled["finalized_slot"].as_u64().unwrap();
    let g = sepolia_genesis();
    let ps: Vec<Server> = (0..2).map(|_| provider(&g, slot, Some(header_11549664()), slot + 64)).collect();
    let mut a = vec![
        "--state".to_string(),
        format!("lez:{}", seq.url),
        "--program".into(),
        f.program.clone(),
        "--instance".into(),
        f.instance_id.clone(),
        "--now".into(),
        (secs(&f.stalled["time_ms"]) + 60).to_string(),
    ];
    for p in &ps {
        a.extend(["--provider".to_string(), p.url.clone()]);
    }
    let o = check(&a);
    assert_eq!(o.code, 0, "{}\n{}", o.report, o.stderr);
    assert_eq!(o.report["instance_id"].as_str(), Some(f.instance_id.as_str()));
    assert_eq!(o.report["divergence"], "confirmed");
    let c = calls.lock().unwrap();
    assert_eq!(c.len(), 1);
    assert_eq!(c[0]["method"], "getAccount");
    // An instance id the account does not hold: an error, exit 1.
    let mut wrong = a.clone();
    let i = wrong.iter().position(|x| x == "--instance").unwrap();
    wrong[i + 1] = format!("0x{}", "11".repeat(32));
    let out = Command::new(bin()).arg("check").args(&wrong).output().unwrap();
    assert_eq!(out.status.code(), Some(1), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn the_daemon_alerts_on_change_recovers_and_stops_on_sigterm() {
    let f = fx();
    let d = scratch("daemon");
    let (s0, s1) = (f.stalled["finalized_slot"].as_u64().unwrap(), f.resumed["finalized_slot"].as_u64().unwrap());
    let g = sepolia_genesis();
    let ps: Vec<Server> = (0..3).map(|_| provider(&g, s0, Some(header_11549664()), s1)).collect();
    for p in &ps {
        p.set(&format!("/eth/v1/beacon/headers/{s1}"), 200, header_json(&root_11608192(), s1));
    }
    let current = d.join("current.hex");
    std::fs::copy(write_state(&d, "stalled.hex", &state_of(&f.stalled)), &current).unwrap();
    let resumed = write_state(&d, "resumed.hex", &state_of(&f.resumed));
    let (log, status, seen) = (d.join("alerts.jsonl"), d.join("status.json"), d.join("seen.json"));
    let mut cmd = Command::new(bin());
    cmd.arg("watch")
        .args(["--state", &format!("cmd:cat {}", current.display())])
        .args(["--now", &secs(&f.resumed["time_ms"]).to_string(), "--interval", "1", "--repeat", "3600"])
        .args(["--alert-log", &log.display().to_string(), "--status-file", &status.display().to_string()])
        .args(["--state-file", &seen.display().to_string()]);
    for (i, p) in ps.iter().enumerate() {
        cmd.args(["--provider", &format!("p{i}={}", p.url)]);
    }
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
    let wait_for = |pred: &dyn Fn(&[Value]) -> bool| {
        for _ in 0..100 {
            let lines: Vec<Value> = std::fs::read_to_string(&log)
                .unwrap_or_default()
                .lines()
                .filter_map(|l| serde_json::from_str(l).ok())
                .collect();
            if pred(&lines) {
                return lines;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        panic!("timed out: {}", std::fs::read_to_string(&log).unwrap_or_default());
    };
    let has = |lines: &[Value], kind: &str| lines.iter().any(|l| l["kind"] == kind);
    wait_for(&|l| has(l, "ws-half") && has(l, "instance-stalled"));
    std::thread::sleep(Duration::from_millis(2500));
    let lines = wait_for(&|l| has(l, "ws-half"));
    assert_eq!(lines.iter().filter(|l| l["kind"] == "ws-half").count(), 1, "a holding alert is not repeated within --repeat");
    // Someone submits the outstanding updates: the instance shows the resumed state.
    std::fs::copy(&resumed, &current).unwrap();
    let lines = wait_for(&|l| l.iter().filter(|x| x["kind"] == "recovered").count() >= 2);
    let cleared: Vec<&str> = lines.iter().filter(|l| l["kind"] == "recovered").map(|l| l["cleared"].as_str().unwrap()).collect();
    assert!(cleared.contains(&"ws-half") && cleared.contains(&"instance-stalled"), "{cleared:?}");
    let st: Value = serde_json::from_str(&std::fs::read_to_string(&status).unwrap()).unwrap();
    assert_eq!(st["finalized_slot"].as_u64(), Some(s1));
    // SIGTERM: the poll in hand ends, `stopped` is logged, exit 0.
    Command::new("kill").args(["-TERM", &child.id().to_string()]).status().unwrap();
    let status_code = child.wait().unwrap();
    assert!(status_code.success(), "{status_code}");
    let mut out = String::new();
    child.stdout.take().unwrap().read_to_string(&mut out).unwrap();
    let summary: Value = serde_json::from_str(out.trim()).unwrap();
    assert!(summary["polls"].as_u64().unwrap() >= 3);
    let lines = wait_for(&|l| has(l, "stopped"));
    assert!(has(&lines, "started"));
    let seen: Value = serde_json::from_str(&std::fs::read_to_string(&seen).unwrap()).unwrap();
    assert_eq!(seen[s0.to_string()]["divergence"], "confirmed");
    assert_eq!(seen[s1.to_string()]["divergence"], "confirmed");
    println!("daemon: {} alerts; seen {}", lines.len(), seen);
}

#[test]
fn the_daemon_holds_a_divergence_through_an_outage_and_retries_a_failed_freeze_hook() {
    let f = fx();
    let d = scratch("outage");
    let slot = f.stalled["finalized_slot"].as_u64().unwrap();
    let g = sepolia_genesis();
    let ps: Vec<Server> = (0..3).map(|_| provider(&g, slot, Some(header_11549664()), slot + 64)).collect();
    let current = d.join("current.hex");
    let forged_file = write_state(&d, "forged.hex", &forged(&state_of(&f.stalled)));
    let healthy = write_state(&d, "healthy.hex", &state_of(&f.stalled));
    std::fs::copy(&forged_file, &current).unwrap();
    let (log, freeze, up) = (d.join("alerts.jsonl"), d.join("freeze-requests.jsonl"), d.join("freeze-tool-up"));
    // The consumer's freeze tool fails until `up` exists (a pager or a signer's service that is down).
    let tool = format!("test -e {} && cat >> {}", up.display(), freeze.display());
    let mut cmd = Command::new(bin());
    cmd.arg("watch")
        .args(["--state", &format!("cmd:cat {}", current.display())])
        .args(["--now", &(secs(&f.stalled["time_ms"]) + 60).to_string(), "--interval", "1", "--repeat", "3600", "--timeout", "3"])
        .args(["--alert-log", &log.display().to_string(), "--alert-cmd", &tool, "--alert-cmd-level", "critical"]);
    for (i, p) in ps.iter().enumerate() {
        cmd.args(["--provider", &format!("p{i}={}", p.url)]);
    }
    let mut child = cmd.stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    let lines = || -> Vec<Value> {
        std::fs::read_to_string(&log).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
    };
    let wait_for = |what: &str, pred: &dyn Fn(&[Value]) -> bool| -> Vec<Value> {
        for _ in 0..150 {
            let l = lines();
            if pred(&l) {
                return l;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        panic!("timed out waiting for {what}: {}", std::fs::read_to_string(&log).unwrap_or_default());
    };
    let count = |l: &[Value], kind: &str| l.iter().filter(|x| x["kind"] == kind).count();
    let recovered = |l: &[Value], kind: &str| l.iter().any(|x| x["kind"] == "recovered" && x["cleared"] == kind);
    let freeze_lines = || std::fs::read_to_string(&freeze).map(|t| t.lines().count()).unwrap_or(0);
    // 1. The divergence is raised; the freeze tool fails, so the alert is emitted again at the next polls, not only
    //    after --repeat (an hour); once the tool is up it takes the alert, once.
    wait_for("the divergence emitted twice", &|l| count(l, "instance-diverged") >= 2);
    assert_eq!(freeze_lines(), 0);
    std::fs::write(&up, "").unwrap();
    let l = wait_for("the freeze tool to take it", &|_| freeze_lines() == 1);
    let emitted = count(&l, "instance-diverged");
    // 2. The instance cannot be read for a while: `instance-unreadable`, and the divergence is held.
    std::fs::write(&current, "not a state\n").unwrap();
    wait_for("instance-unreadable", &|l| count(l, "instance-unreadable") >= 1);
    std::thread::sleep(Duration::from_millis(2500));
    // 3. Readable again, every provider down: no verdict, the divergence still held.
    for p in &ps {
        p.set("/eth/v1/beacon/genesis", 503, "{}".into());
    }
    std::fs::copy(&forged_file, &current).unwrap();
    wait_for("the providers down", &|l| count(l, "provider-unavailable") >= 3 && recovered(l, "instance-unreadable"));
    std::thread::sleep(Duration::from_millis(2500));
    let l = lines();
    assert!(!recovered(&l, "instance-diverged"), "an outage is not a recovery: {l:?}");
    assert_eq!(count(&l, "instance-diverged"), emitted, "held, and not emitted again within --repeat once delivered");
    assert_eq!(freeze_lines(), 1, "the freeze tool ran once after it came up");
    // 4. Providers back and the forged header replaced by the honest one: now it has cleared.
    for p in &ps {
        p.set("/eth/v1/beacon/genesis", 200, g.clone());
    }
    std::fs::copy(&healthy, &current).unwrap();
    let l = wait_for("the divergence recovered", &|l| recovered(l, "instance-diverged"));
    Command::new("kill").args(["-TERM", &child.id().to_string()]).status().unwrap();
    assert!(child.wait().unwrap().success());
    println!("outage: {} alerts, instance-diverged emitted {emitted} times (failed tool retried), freeze tool ran once", l.len());
}
