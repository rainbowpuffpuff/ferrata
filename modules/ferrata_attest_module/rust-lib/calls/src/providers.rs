//! `verify_checkpoint`'s provider checks (`docs/design/attestation.md` §11.4, F.9): a checkpoint root asked of several
//! independent beacon API providers, and the checkpoint's execution block asked of an execution JSON-RPC endpoint (the
//! design's `verified_proxy_module`, whose answers its own light client verifies; any node answers the same calls).
//!
//! A beacon provider **agrees** when it holds the root as the canonical block at its slot (by root, with `canonical`,
//! and by slot), has finalized that slot, and describes the same chain as the instance's configuration: its genesis
//! (`genesis_validators_root`, time, genesis fork version) and its spec (`DEPOSIT_CHAIN_ID`, the execution chain id the
//! light client cannot check, so that chain id and `genesis_validators_root` are tied by the providers themselves; the
//! preset; every fork the configuration schedules). It **disagrees** when it answers otherwise (a fork the network
//! schedules and the configuration does not is a note, not a disagreement: a successor instance crosses it). It is
//! **unavailable** when a request fails, or when it does not serve the beacon headers API. Agreeing providers are
//! counted by host.
//!
//! Nothing a provider serves is trusted beyond the comparison: the bootstrap's own checks (the program's `Init`) and
//! the committee's `KeyValidate` run on the bytes the caller gives. Reads only; HTTP through `curl` (as
//! `ferrata-updater` reads beacon nodes), a generic User-Agent, nothing that identifies the caller.

use std::{
    collections::BTreeSet,
    io::Write as _,
    process::{Command, Stdio},
};

use ferrata_core::{fork::Fork, Root};
use serde_json::{json, Value};

use crate::{hex, Config};

/// A response: HTTP status and body, or a transport failure.
pub type Reply = Result<(u16, String), String>;

/// The two kinds of request the checks make.
pub trait Http {
    fn get(&mut self, url: &str) -> Reply;
    fn post(&mut self, url: &str, body: &str) -> Reply;
}

/// `curl`: `http://` and `https://` only, a bounded time and size, User-Agent `ferrata-verify-checkpoint`.
pub struct Curl {
    pub timeout_secs: u64,
}

impl Curl {
    fn call(&self, url: &str, body: Option<&str>) -> Reply {
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(format!("{url}: only http:// and https:// endpoints"));
        }
        let mut c = Command::new("curl");
        c.args(["-sS", "--proto", "=http,https", "--max-filesize", "16000000", "-m"])
            .arg(self.timeout_secs.to_string())
            .args(["-A", "ferrata-verify-checkpoint", "-H", "accept: application/json", "-w", "\n%{http_code}"]);
        if body.is_some() {
            c.args(["-X", "POST", "-H", "content-type: application/json", "--data-binary", "@-"]);
        }
        c.arg(url).stdout(Stdio::piped()).stderr(Stdio::piped());
        c.stdin(if body.is_some() { Stdio::piped() } else { Stdio::null() });
        let mut child = c.spawn().map_err(|e| format!("curl did not start: {e}"))?;
        if let (Some(b), Some(mut stdin)) = (body, child.stdin.take()) {
            stdin.write_all(b.as_bytes()).map_err(|e| format!("curl: {e}"))?;
        }
        let out = child.wait_with_output().map_err(|e| format!("curl: {e}"))?;
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let (body, code) = text.rsplit_once('\n').unwrap_or(("", text.as_str()));
        match code.trim().parse::<u16>() {
            Ok(code) if code != 0 => Ok((code, body.to_owned())),
            _ => Err(String::from_utf8_lossy(&out.stderr).trim().to_string()),
        }
    }
}

impl Http for Curl {
    fn get(&mut self, url: &str) -> Reply {
        self.call(url, None)
    }
    fn post(&mut self, url: &str, body: &str) -> Reply {
        self.call(url, Some(body))
    }
}

/// What the providers are asked about: the checkpoint, its slot when the bootstrap gives it, the instance's chain.
pub struct Expected<'a> {
    pub checkpoint: &'a Root,
    pub slot: Option<u64>,
    pub config: &'a Config,
}

/// `host[:port]` of a URL, lower case: providers are counted by it.
pub fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.split(['/', '?', '#']).next().unwrap_or("").to_ascii_lowercase()
}

/// The requests made of one provider and what they showed.
struct Asked<'a> {
    http: &'a mut dyn Http,
    requests: Vec<Value>,
    disagree: Vec<String>,
    unavailable: Vec<String>,
    notes: Vec<String>,
}

impl Asked<'_> {
    /// GET `url`; `Some((status, json))` for an answer (200, or 404 when `allow_404`), recorded; `None` (recorded as
    /// unavailable) for a transport failure, another status or a body that is not JSON.
    fn get(&mut self, what: &str, url: &str, allow_404: bool) -> Option<(u16, Value)> {
        let r = self.http.get(url);
        self.finish(what, url, r, allow_404)
    }

    fn post(&mut self, what: &str, url: &str, body: &str) -> Option<(u16, Value)> {
        let r = self.http.post(url, body);
        self.finish(what, url, r, false)
    }

    fn finish(&mut self, what: &str, url: &str, r: Reply, allow_404: bool) -> Option<(u16, Value)> {
        match r {
            Err(e) => {
                self.requests.push(json!({ "url": url, "error": e }));
                self.unavailable.push(format!("{what}: {e}"));
                None
            }
            Ok((status, body)) => {
                self.requests.push(json!({ "url": url, "status": status, "bytes": body.len() }));
                if status == 200 || (status == 404 && allow_404) {
                    match serde_json::from_str::<Value>(&body) {
                        Ok(v) => return Some((status, v)),
                        Err(_) if status == 404 => return Some((status, Value::Null)),
                        Err(_) => self.unavailable.push(format!("{what}: HTTP {status}, not JSON")),
                    }
                } else {
                    self.unavailable.push(format!("{what}: HTTP {status}"));
                }
                None
            }
        }
    }

    fn differs(&mut self, what: &str, theirs: impl std::fmt::Display, ours: impl std::fmt::Display) {
        self.disagree.push(format!("{what}: {theirs}, the instance's {ours}"));
    }
}

fn root_of(v: &Value) -> Option<Root> {
    crate::unhex(v.as_str()?.strip_prefix("0x")?).ok()?.try_into().ok()
}

/// A decimal string or number (beacon APIs give integers as strings).
fn num(v: &Value) -> Option<u64> {
    match v {
        Value::String(s) => s.parse().ok(),
        Value::Number(n) => n.as_u64(),
        _ => None,
    }
}

/// `0x`-prefixed hex quantity (JSON-RPC).
fn quantity(v: &Value) -> Option<u64> {
    u64::from_str_radix(v.as_str()?.strip_prefix("0x")?, 16).ok()
}

/// One beacon API provider (`base`: its URL without the `/eth/...` path).
pub fn beacon(http: &mut dyn Http, name: &str, base: &str, e: &Expected) -> Value {
    let base = base.trim_end_matches('/');
    let mut a = Asked { http, requests: Vec::new(), disagree: Vec::new(), unavailable: Vec::new(), notes: Vec::new() };
    let c = e.config;
    let root = hex(e.checkpoint);

    // The chain: genesis.
    if let Some((_, g)) = a.get("genesis", &format!("{base}/eth/v1/beacon/genesis"), false) {
        let d = &g["data"];
        match root_of(&d["genesis_validators_root"]) {
            Some(r) if r == c.genesis_validators_root => {}
            _ => a.differs("genesis_validators_root", &d["genesis_validators_root"], hex(&c.genesis_validators_root)),
        }
        if num(&d["genesis_time"]) != Some(c.genesis_time) {
            a.differs("genesis_time", &d["genesis_time"], c.genesis_time);
        }
        if d["genesis_fork_version"].as_str() != Some(hex(&c.fork_versions[0]).as_str()) {
            a.differs("genesis_fork_version", &d["genesis_fork_version"], hex(&c.fork_versions[0]));
        }
    }

    // The chain: its spec (the execution chain id, the preset, the forks the configuration schedules).
    if let Some((_, s)) = a.get("spec", &format!("{base}/eth/v1/config/spec"), false) {
        let d = &s["data"];
        if num(&d["DEPOSIT_CHAIN_ID"]) != Some(c.chain_id) {
            a.differs("DEPOSIT_CHAIN_ID", &d["DEPOSIT_CHAIN_ID"], c.chain_id);
        }
        for (key, ours) in [
            ("SECONDS_PER_SLOT", c.seconds_per_slot),
            ("SLOTS_PER_EPOCH", c.slots_per_epoch),
            ("EPOCHS_PER_SYNC_COMMITTEE_PERIOD", c.epochs_per_sync_committee_period),
            ("SYNC_COMMITTEE_SIZE", u64::from(c.sync_committee_size)),
            ("MIN_SYNC_COMMITTEE_PARTICIPANTS", c.min_sync_committee_participants),
        ] {
            match num(&d[key]) {
                Some(v) if v == ours => {}
                None if d[key].is_null() => a.notes.push(format!("{key} not served")),
                _ => a.differs(key, &d[key], ours),
            }
        }
        for fork in Fork::ALL.into_iter().skip(1) {
            let i = fork as usize;
            let name = fork.name().to_ascii_uppercase();
            let (version, epoch) = (&d[format!("{name}_FORK_VERSION")], &d[format!("{name}_FORK_EPOCH")]);
            let scheduled = c.fork_epochs[i] != u64::MAX;
            if scheduled {
                // A value served must be the configuration's; one not served is only noted (a checkpoint-sync
                // server, for one, serves a spec without the fork schedule).
                match num(epoch) {
                    Some(v) if v == c.fork_epochs[i] => {}
                    None if epoch.is_null() => a.notes.push(format!("{name}_FORK_EPOCH not served")),
                    _ => a.differs(&format!("{name}_FORK_EPOCH"), epoch, c.fork_epochs[i]),
                }
                match version.as_str() {
                    Some(v) if v == hex(&c.fork_versions[i]) => {}
                    None if version.is_null() => a.notes.push(format!("{name}_FORK_VERSION not served")),
                    _ => a.differs(&format!("{name}_FORK_VERSION"), version, hex(&c.fork_versions[i])),
                }
            } else if let Some(at) = num(epoch).filter(|x| *x != u64::MAX) {
                a.notes.push(format!(
                    "the network schedules {name} at epoch {at}; this instance does not (a successor instance crosses it)"
                ));
            }
        }
    }

    // The block: finalized on this provider, and canonical at its slot.
    let finalized = a.get("finalized header", &format!("{base}/eth/v1/beacon/headers/finalized"), false);
    let serves_headers = finalized.is_some();
    let finalized_slot = finalized.as_ref().and_then(|(_, f)| num(&f["data"]["header"]["message"]["slot"]));
    let mut slot = e.slot;
    if serves_headers {
        match a.get("header by root", &format!("{base}/eth/v1/beacon/headers/{root}"), true) {
            Some((404, _)) => a.disagree.push(format!("no block with root {root}")),
            Some((_, h)) => {
                let d = &h["data"];
                if root_of(&d["root"]) != Some(*e.checkpoint) {
                    a.differs("header root", &d["root"], &root);
                }
                if d["canonical"] != json!(true) {
                    a.differs("canonical", &d["canonical"], true);
                }
                let at = num(&d["header"]["message"]["slot"]);
                match (at, e.slot) {
                    (Some(at), Some(want)) if at != want => a.differs("slot", at, want),
                    (Some(at), None) => slot = Some(at),
                    (None, _) => a.differs("slot", "none", e.slot.map_or("?".into(), |s| s.to_string())),
                    _ => {}
                }
            }
            None => {}
        }
        if let Some(s) = slot {
            match a.get("header at slot", &format!("{base}/eth/v1/beacon/headers/{s}"), true) {
                Some((404, _)) => a.disagree.push(format!("no block at slot {s}")),
                Some((_, h)) => {
                    if root_of(&h["data"]["root"]) != Some(*e.checkpoint) {
                        a.differs(&format!("canonical root at slot {s}"), &h["data"]["root"], &root);
                    }
                }
                None => {}
            }
            match finalized_slot {
                Some(f) if f >= s => {}
                Some(f) => a.differs("finalized slot", f, format!("at least {s}")),
                None => a.unavailable.push("finalized header: no slot".into()),
            }
        }
    } else {
        a.notes.push("does not serve the beacon headers API: the block is not checked here".into());
    }

    let verdict = if !a.disagree.is_empty() {
        "disagrees"
    } else if !a.unavailable.is_empty() || !serves_headers {
        "unavailable"
    } else {
        "agrees"
    };
    json!({
        "name": name, "url": base, "host": host_of(base), "verdict": verdict, "slot": slot,
        "finalized_slot": finalized_slot, "disagreements": a.disagree, "unavailable": a.unavailable,
        "notes": a.notes, "requests": a.requests,
    })
}

/// The checkpoint's execution block, as an execution JSON-RPC endpoint holds it: the block of `block_hash` (and number,
/// when the bootstrap's layout carries it), canonical at its number, at or below the endpoint's finalized block, on the
/// instance's chain id.
pub fn execution(
    http: &mut dyn Http,
    name: &str,
    url: &str,
    block_hash: &Root,
    block_number: Option<u64>,
    chain_id: u64,
) -> Value {
    let mut a = Asked { http, requests: Vec::new(), disagree: Vec::new(), unavailable: Vec::new(), notes: Vec::new() };
    let hash = hex(block_hash);
    fn rpc(a: &mut Asked, url: &str, what: &str, method: &str, params: Value) -> Option<Value> {
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string();
        let (_, v) = a.post(what, url, &body)?;
        if !v["error"].is_null() {
            a.unavailable.push(format!("{what}: {}", v["error"]));
            return None;
        }
        Some(v["result"].clone())
    }
    if let Some(id) = rpc(&mut a, url, "eth_chainId", "eth_chainId", json!([])) {
        if quantity(&id) != Some(chain_id) {
            a.differs("eth_chainId", &id, chain_id);
        }
    }
    let mut number = None;
    if let Some(b) = rpc(&mut a, url, "block by hash", "eth_getBlockByHash", json!([hash, false])) {
        if b.is_null() {
            a.disagree.push(format!("no block with hash {hash}"));
        } else {
            number = quantity(&b["number"]);
            if root_of(&b["hash"]) != Some(*block_hash) {
                a.differs("block hash", &b["hash"], &hash);
            }
            if let (Some(n), Some(want)) = (number, block_number) {
                if n != want {
                    a.differs("block number", n, want);
                }
            }
        }
    }
    let mut finalized = None;
    if let Some(n) = number {
        if let Some(b) =
            rpc(&mut a, url, "block at number", "eth_getBlockByNumber", json!([format!("0x{n:x}"), false]))
        {
            if root_of(&b["hash"]) != Some(*block_hash) {
                a.differs(&format!("canonical block {n}"), &b["hash"], &hash);
            }
        }
        if let Some(f) = rpc(&mut a, url, "finalized block", "eth_getBlockByNumber", json!(["finalized", false])) {
            finalized = quantity(&f["number"]);
            match finalized {
                Some(f) if f >= n => {}
                Some(f) => a.differs("finalized block", f, format!("at least {n}")),
                None => a.unavailable.push("finalized block: no number".into()),
            }
        }
    }
    let verdict = if !a.disagree.is_empty() {
        "disagrees"
    } else if !a.unavailable.is_empty() || number.is_none() {
        "unavailable"
    } else {
        "agrees"
    };
    json!({
        "name": name, "url": url, "verdict": verdict, "block_hash": hash, "block_number": number,
        "finalized_number": finalized, "disagreements": a.disagree, "unavailable": a.unavailable,
        "requests": a.requests,
    })
}

/// The providers' verdicts together: agreeing hosts, and whether any provider disagrees.
pub fn tally(results: &[Value]) -> (u64, bool) {
    let hosts: BTreeSet<&str> =
        results.iter().filter(|r| r["verdict"] == "agrees").filter_map(|r| r["host"].as_str()).collect();
    (hosts.len() as u64, results.iter().any(|r| r["verdict"] == "disagrees"))
}

/// Every request and its response, kept (`examples/call.rs` with `FERRATA_HTTP_RECORD`): what the checks saw, replayed
/// by [`Replay`] in the tests.
pub struct Recording<H: Http> {
    pub inner: H,
    pub log: Vec<Value>,
}

impl<H: Http> Recording<H> {
    fn keep(&mut self, method: &str, url: &str, body: Option<&str>, r: Reply) -> Reply {
        self.log.push(match &r {
            Ok((status, text)) => {
                json!({ "method": method, "url": url, "body": body, "status": status, "response": text })
            }
            Err(e) => json!({ "method": method, "url": url, "body": body, "error": e }),
        });
        r
    }
}

impl<H: Http> Http for Recording<H> {
    fn get(&mut self, url: &str) -> Reply {
        let r = self.inner.get(url);
        self.keep("GET", url, None, r)
    }
    fn post(&mut self, url: &str, body: &str) -> Reply {
        let r = self.inner.post(url, body);
        self.keep("POST", url, Some(body), r)
    }
}

/// Recorded responses, by method, URL and body; a request not recorded fails as a transport error would.
pub struct Replay {
    pub log: Vec<Value>,
}

impl Replay {
    fn find(&self, method: &str, url: &str, body: Option<&str>) -> Reply {
        let hit = self.log.iter().find(|e| e["method"] == method && e["url"] == url && e["body"].as_str() == body);
        match hit {
            Some(e) if e["error"].is_string() => Err(e["error"].as_str().unwrap_or_default().to_owned()),
            Some(e) => {
                Ok((e["status"].as_u64().unwrap_or(0) as u16, e["response"].as_str().unwrap_or_default().to_owned()))
            }
            None => Err(format!("not recorded: {method} {url}")),
        }
    }
}

impl Http for Replay {
    fn get(&mut self, url: &str) -> Reply {
        self.find("GET", url, None)
    }
    fn post(&mut self, url: &str, body: &str) -> Reply {
        self.find("POST", url, Some(body))
    }
}
