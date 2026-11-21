//! Where the monitor reads: the instance's state on LEZ, and beacon providers.
//!
//! * The instance: `lez:URL`, a LEZ v0.4 sequencer's JSON-RPC (`getAccount` of `instance(id)`, LEZ's own `Account`
//!   type, the attestation program's actor state in it); `file:PATH` or `cmd:COMMAND`, the state as text (hex of its
//!   Borsh encoding, or a JSON object with `instance_state`), from anything that reads LEZ (the `ferrata` tool,
//!   `ferrata_attest_module` through Logos Core's `lez_core`, a recorded state).
//! * A provider: a beacon node's REST API, `NAME=URL` or `URL`: `/eth/v1/beacon/genesis`,
//!   `/eth/v1/beacon/headers/{slot}`, `/eth/v1/beacon/headers/finalized`.
//!
//! HTTP is `curl` (a generic User-Agent, `-m` timeouts), as `ferrata_attest_module`'s `verify_checkpoint` and
//! `ferrata-updater` read beacon nodes. Nothing the providers serve is trusted beyond the comparison.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::str::FromStr;

use anyhow::{anyhow, bail, ensure, Context, Result};
use ferrata_core::Root;
use ferrata_lez_wire::{header_actor, instance_actor, InstanceState, RecordState};
use lee_core::account::{Account, AccountId};
use serde_json::{json, Value};

use crate::{decode_state, parse_root, ProviderView};

/// Where the instance's state comes from.
#[derive(Clone, Debug)]
pub enum StateSource {
    File(PathBuf),
    Cmd(String),
    Lez(String),
}

impl FromStr for StateSource {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        if let Some(p) = s.strip_prefix("file:") {
            Ok(Self::File(p.into()))
        } else if let Some(c) = s.strip_prefix("cmd:") {
            Ok(Self::Cmd(c.into()))
        } else if let Some(u) = s.strip_prefix("lez:") {
            Ok(Self::Lez(u.into()))
        } else {
            Err(format!("{s:?}: lez:URL (a LEZ sequencer's JSON-RPC), file:PATH or cmd:COMMAND"))
        }
    }
}

/// The instance on LEZ: the attestation program's account and the instance id.
#[derive(Clone, Copy, Debug)]
pub struct Target {
    pub program: AccountId,
    pub instance_id: Root,
}

/// The instance's state from `src`. `lez:` needs `target`.
pub fn read_state(src: &StateSource, target: Option<&Target>, timeout_s: u64) -> Result<InstanceState> {
    match src {
        StateSource::File(p) => {
            parse_state_text(&std::fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))?)
        }
        StateSource::Cmd(c) => {
            let out = Command::new("sh").arg("-c").arg(c).stderr(Stdio::inherit()).output().with_context(|| format!("running {c:?}"))?;
            ensure!(out.status.success(), "{c:?} exited with {}", out.status);
            parse_state_text(&String::from_utf8_lossy(&out.stdout))
        }
        StateSource::Lez(url) => {
            let t = target.ok_or_else(|| anyhow!("lez: needs --program and --instance"))?;
            let actor = instance_actor(t.program, &t.instance_id);
            let bytes = actor_state(url, actor.account_id, actor.program_account_id, timeout_s)?;
            ensure!(!bytes.is_empty(), "no instance {} at {} on {url}", crate::hex0x(&t.instance_id), actor.account_id);
            let state = decode_state(&bytes)?;
            ensure!(state.id == t.instance_id, "the account holds instance {}, not the one asked", crate::hex0x(&state.id));
            Ok(state)
        }
    }
}

/// The record `header(id, slot)` from a sequencer, if it exists.
pub fn read_record(url: &str, t: &Target, slot: u64, timeout_s: u64) -> Result<Option<RecordState>> {
    let actor = header_actor(t.program, &t.instance_id, slot);
    let bytes = actor_state(url, actor.account_id, actor.program_account_id, timeout_s)?;
    if bytes.is_empty() {
        return Ok(None);
    }
    Ok(Some(borsh::from_slice(&bytes).map_err(|e| anyhow!("the record at slot {slot} does not decode: {e}"))?))
}

/// The state as text: hex of its Borsh encoding, or a JSON object with `instance_state` (or `state`) in hex.
pub fn parse_state_text(text: &str) -> Result<InstanceState> {
    let t = text.trim();
    let h = if t.starts_with('{') {
        let v: Value = serde_json::from_str(t).context("the state's JSON does not parse")?;
        v.get("instance_state")
            .or_else(|| v.get("state"))
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("the state's JSON has no instance_state"))?
            .to_string()
    } else {
        t.to_string()
    };
    decode_state(&hex::decode(h.trim().trim_start_matches("0x")).context("the state is not hex")?)
}

/// `program`'s actor state in `account`, as a LEZ sequencer serves the account (`getAccount`).
pub fn actor_state(url: &str, account: AccountId, program: AccountId, timeout_s: u64) -> Result<Vec<u8>> {
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "getAccount", "params": [account] });
    let (status, text) = curl_post(url, &body.to_string(), timeout_s)?;
    ensure!(status == 200, "getAccount: HTTP {status}");
    let v: Value = serde_json::from_str(&text).context("getAccount: the answer does not parse")?;
    if let Some(e) = v.get("error") {
        bail!("getAccount: {e}");
    }
    let account: Option<Account> =
        serde_json::from_value(v.get("result").cloned().unwrap_or(Value::Null)).context("getAccount: not an account")?;
    Ok(account.map(|a| a.data.actor_state(program).to_vec()).unwrap_or_default())
}

/// A beacon provider: `NAME=URL` or `URL` (named by its host).
#[derive(Clone, Debug)]
pub struct Provider {
    pub name: String,
    pub url: String,
}

impl FromStr for Provider {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let (name, url) = match s.split_once('=') {
            Some((n, u)) if !n.contains('/') => (n.to_string(), u.to_string()),
            _ => {
                let host = s.split("://").nth(1).unwrap_or(s).split('/').next().unwrap_or(s);
                (host.to_string(), s.to_string())
            }
        };
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(format!("{s:?}: a beacon API base URL (http:// or https://)"));
        }
        Ok(Self { name, url: url.trim_end_matches('/').to_string() })
    }
}

/// One provider's answers for the instance's finalized `slot`.
pub fn query_provider(p: &Provider, slot: u64, timeout_s: u64) -> ProviderView {
    let mut v = ProviderView { name: p.name.clone(), ..ProviderView::default() };
    match get_json(&format!("{}/eth/v1/beacon/genesis", p.url), timeout_s) {
        Ok(Some(j)) => {
            v.genesis_validators_root = j["data"]["genesis_validators_root"].as_str().and_then(|s| parse_root(s).ok());
            v.genesis_time = j["data"]["genesis_time"].as_str().and_then(|s| s.parse().ok());
            if v.genesis_validators_root.is_none() || v.genesis_time.is_none() {
                v.error = Some("genesis: the answer has no genesis_validators_root or genesis_time".into());
                return v;
            }
        }
        Ok(None) => {
            v.error = Some("genesis: HTTP 404".into());
            return v;
        }
        Err(e) => {
            v.error = Some(format!("genesis: {e}"));
            return v;
        }
    }
    // Its finalized slot first: an answer at the instance's slot counts only once the provider has finalized that slot
    // (before, a 404 there may be a block it has not seen yet, and a root may still be reorganised away).
    match get_json(&format!("{}/eth/v1/beacon/headers/finalized", p.url), timeout_s) {
        Ok(Some(j)) => match j["data"]["header"]["message"]["slot"].as_str().and_then(|s| s.parse::<u64>().ok()) {
            Some(f) => v.finalized_slot = Some(f),
            None => {
                v.error = Some("headers/finalized: no slot in the answer".into());
                return v;
            }
        },
        Ok(None) => {
            v.error = Some("headers/finalized: HTTP 404".into());
            return v;
        }
        Err(e) => {
            v.error = Some(format!("headers/finalized: {e}"));
            return v;
        }
    }
    if v.finalized_slot.is_some_and(|f| f < slot) {
        return v; // behind the instance: not counted (assess)
    }
    match get_json(&format!("{}/eth/v1/beacon/headers/{slot}", p.url), timeout_s) {
        Ok(Some(j)) => {
            let d = &j["data"];
            let at = d["header"]["message"]["slot"].as_str().and_then(|s| s.parse::<u64>().ok());
            let canonical = d["canonical"].as_bool().unwrap_or(true);
            match d["root"].as_str().map(parse_root) {
                Some(Ok(r)) => v.root_at_slot = Some((canonical && at == Some(slot)).then_some(r)),
                _ => {
                    v.error = Some(format!("headers/{slot}: no root in the answer"));
                    return v;
                }
            }
        }
        Ok(None) => v.root_at_slot = Some(None),
        Err(e) => {
            v.error = Some(format!("headers/{slot}: {e}"));
            return v;
        }
    }
    v
}

/// A JSON document by GET; `None` on HTTP 404.
fn get_json(url: &str, timeout_s: u64) -> Result<Option<Value>> {
    let (code, body) = curl_get(url, timeout_s)?;
    match code {
        200 => Ok(Some(serde_json::from_str(&body).context("the answer does not parse")?)),
        404 => Ok(None),
        c => bail!("HTTP {c}"),
    }
}

fn curl(args: &[&str], stdin: Option<&str>) -> Result<(u16, String)> {
    let mut cmd = Command::new("curl");
    cmd.args(["-sS", "-A", "ferrata-attest-monitor", "-w", "\n%{http_code}"]).args(args);
    cmd.stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().context("running curl")?;
    if let (Some(text), Some(mut w)) = (stdin, child.stdin.take()) {
        use std::io::Write as _;
        w.write_all(text.as_bytes())?;
    }
    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let (body, code) = text.rsplit_once('\n').ok_or_else(|| anyhow!("curl gave no status"))?;
    Ok((code.trim().parse().context("curl's status")?, body.to_string()))
}

fn curl_get(url: &str, timeout_s: u64) -> Result<(u16, String)> {
    let t = timeout_s.max(1).to_string();
    curl(&["-m", &t, "-H", "Accept: application/json", url], None)
}

fn curl_post(url: &str, body: &str, timeout_s: u64) -> Result<(u16, String)> {
    let t = timeout_s.max(1).to_string();
    curl(&["-m", &t, "-H", "Content-Type: application/json", "--data-binary", "@-", url], Some(body))
}
