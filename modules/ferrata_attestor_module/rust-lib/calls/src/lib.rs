//! The methods of `ferrata_attestor_module` as plain functions over JSON strings, so that they run and
//! are tested without the module scaffold. The work is `ferrata_attestor`'s (`checkpoint/attestor`,
//! staged into the module by `scripts/stage.sh`): the same functions the command-line program calls,
//! so the same records, the same checks and the same refusals.
//!
//! Every method takes one JSON request and returns one JSON object with a `verdict`:
//! `invalid_request` (the request cannot be read), `refused` (the attestor or the collector refuses:
//! a certificate that does not certify the key, a channel key outside the live set, a tip below or
//! beside one it signed, fewer than `t(n)` valid attestations, a final tier that does not check),
//! `error` (the node could not be read), or the method's own success verdict.
//!
//! [`Daemon`] is the attestor as a daemon inside the module: a thread that asks its node every
//! `poll_secs` (from the signed tip's slot on), checks the final tier against its LEZ node (`lez`: it
//! signs only a tip the LEZ node holds final, so never a block LEZ parked), signs the tip, records it in
//! its state file before publishing (it never signs two histories, across restarts too) and publishes
//! each attestation as a file (`att-<tip>.json`, `latest.json`). One round is `ferrata_attestor::round`,
//! the same code as the command-line program's `attest --node`.

use std::collections::VecDeque;
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context as _, Result};
use attestor_core::{verify_certificate, Deployment, SignedAttestation};
use checkpoint_core::hash::sha256_parts;
use checkpoint_core::input::CheckpointInputB;
use checkpoint_core::Hash;
use ferrata_attestor::{
    assemble_unchecked, attestation_from_json, attestation_json, base_from_answers, bedrock_tier,
    collect as collect_atts, deployment_from_json, deployment_json, deployment_of, final_tier, fixture_meta, hex_str,
    hexa, is_node_fault, read_key, read_state, round, scan_from, signers, write_atomic, write_state, Attestor,
    AttestorState, LezNode, LezRpc, LezSaved, NodeAnswers, RoundError,
};
use serde_json::{json, Value};

/// What `scripts/stage.sh` recorded about the copy of `checkpoint/attestor` this crate was built against.
const STAGED: &str = include_str!("../../attestor/STAGED.json");

/// The devnet channel of LEZ v0.4.0-rc1's configurations (the CLI's default).
const DEFAULT_CHANNEL: &str = "0101010101010101010101010101010101010101010101010101010101010101";

pub const METHODS: &[&str] = &[
    "version",
    "keygen",
    "pubkey",
    "deployment",
    "certify",
    "tier",
    "attest",
    "verify",
    "collect",
    "assemble",
    "base",
    "start",
    "status",
    "stop",
];

/// `version()`: the module's version and the attestor it was built from.
pub fn version() -> String {
    json!({
        "module": "ferrata_attestor_module",
        "version": env!("CARGO_PKG_VERSION"),
        "ferrata_attestor": serde_json::from_str::<Value>(STAGED).unwrap_or(Value::Null),
        "record_version": ferrata_attestor::RECORD_VERSION,
        "methods": METHODS,
    })
    .to_string()
}

// ---- replies ---------------------------------------------------------------------------------------

enum Fail {
    Invalid(anyhow::Error),
    Refused(anyhow::Error),
    Error(anyhow::Error),
}

trait Mark<T> {
    /// The request cannot be read.
    fn bad(self) -> Result<T, Fail>;
    /// The attestor or the collector refuses.
    fn no(self) -> Result<T, Fail>;
    /// Something outside the request failed (the node, a file).
    fn fault(self) -> Result<T, Fail>;
}

impl<T> Mark<T> for Result<T> {
    fn bad(self) -> Result<T, Fail> {
        self.map_err(Fail::Invalid)
    }
    fn no(self) -> Result<T, Fail> {
        self.map_err(Fail::Refused)
    }
    fn fault(self) -> Result<T, Fail> {
        self.map_err(Fail::Error)
    }
}

fn reply(r: Result<Value, Fail>) -> String {
    match r {
        Ok(v) => v,
        Err(Fail::Invalid(e)) => json!({ "verdict": "invalid_request", "error": format!("{e:#}") }),
        Err(Fail::Refused(e)) => json!({ "verdict": "refused", "error": format!("{e:#}") }),
        Err(Fail::Error(e)) => json!({ "verdict": "error", "error": format!("{e:#}") }),
    }
    .to_string()
}

fn parse(request: &str) -> Result<Value, Fail> {
    if request.trim().is_empty() {
        return Ok(json!({}));
    }
    let v: Value = serde_json::from_str(request).context("the request is not JSON").bad()?;
    if !v.is_object() {
        return Err(Fail::Invalid(anyhow!("the request is not a JSON object")));
    }
    Ok(v)
}

fn text<'a>(r: &'a Value, k: &str) -> Result<&'a str> {
    r.get(k).and_then(Value::as_str).with_context(|| format!("{k} (a string) is required"))
}

fn opt_text<'a>(r: &'a Value, k: &str) -> Option<&'a str> {
    r.get(k).and_then(Value::as_str)
}

/// A number given as a JSON number or a decimal string.
fn opt_u64(r: &Value, k: &str) -> Result<Option<u64>> {
    match r.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => n.as_u64().map(Some).with_context(|| format!("{k}: not an unsigned integer")),
        Some(Value::String(s)) => s.parse().map(Some).with_context(|| format!("{k}: not an unsigned integer")),
        Some(_) => bail!("{k}: not a number"),
    }
}

fn u64_of(r: &Value, k: &str) -> Result<u64> {
    opt_u64(r, k)?.with_context(|| format!("{k} (a number) is required"))
}

fn read_json(path: &Path) -> Result<Value> {
    serde_json::from_slice(&fs::read(path).with_context(|| path.display().to_string())?)
        .with_context(|| path.display().to_string())
}

/// Written whole or not at all (temporary file, then rename), as a watcher may read it at any time.
fn write_json(path: &Path, v: &Value) -> Result<()> {
    write_bytes(path, (serde_json::to_string_pretty(v)? + "\n").as_bytes())
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            fs::create_dir_all(dir).with_context(|| dir.display().to_string())?;
        }
    }
    let tmp = path.with_extension("tmp-write");
    fs::write(&tmp, bytes).with_context(|| tmp.display().to_string())?;
    fs::rename(&tmp, path).with_context(|| path.display().to_string())
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(sha256_parts(&[bytes]))
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default()
}

/// The deployment: `deployment` (the object `deployment` writes) or `deployment_file`.
fn deployment_req(r: &Value) -> Result<Deployment> {
    match (r.get("deployment"), opt_text(r, "deployment_file")) {
        (Some(d), None) => deployment_from_json(d),
        (None, Some(p)) => deployment_from_json(&read_json(Path::new(p))?),
        _ => bail!("exactly one of deployment (an object) or deployment_file"),
    }
}

/// The certificate record (`certify`'s): `cert` or `cert_file`.
fn cert_req(r: &Value) -> Result<Value> {
    match (r.get("cert"), opt_text(r, "cert_file")) {
        (Some(c), None) => Ok(c.clone()),
        (None, Some(p)) => read_json(Path::new(p)),
        _ => bail!("exactly one of cert (an object) or cert_file"),
    }
}

/// Published attestation records: `attestations` (objects) and/or `att_files` (paths), in that order.
fn attestations_req(r: &Value) -> Result<(Vec<SignedAttestation>, Vec<Value>, Vec<String>)> {
    let mut records = Vec::new();
    let mut names = Vec::new();
    if let Some(list) = r.get("attestations") {
        for (i, v) in list.as_array().context("attestations: not a list")?.iter().enumerate() {
            records.push(v.clone());
            names.push(format!("attestations[{i}]"));
        }
    }
    if let Some(list) = r.get("att_files") {
        for p in list.as_array().context("att_files: not a list")? {
            let p = p.as_str().context("att_files: not a list of paths")?;
            records.push(read_json(Path::new(p))?);
            names.push(p.to_string());
        }
    }
    let atts = records.iter().map(attestation_from_json).collect::<Result<Vec<_>>>()?;
    Ok((atts, records, names))
}

/// A checkpoint_b input: `<key>_file` (a path) or `<key>` (hex).
fn input_req(r: &Value, key: &str) -> Result<Vec<u8>> {
    match (opt_text(r, key), opt_text(r, &format!("{key}_file"))) {
        (Some(h), None) => hex::decode(h.strip_prefix("0x").unwrap_or(h)).with_context(|| format!("{key}: not hex")),
        (None, Some(p)) => fs::read(p).with_context(|| p.to_string()),
        _ => bail!("exactly one of {key} (hex) or {key}_file"),
    }
}

fn channel_req(r: &Value) -> Result<Hash> {
    hex_str(opt_text(r, "channel").unwrap_or(DEFAULT_CHANNEL), "channel")
}

/// The node's answers: asked of a running node over HTTP (`node`, saved under `save_dir`, from
/// `slot_from`) or saved ones (`answers_dir`).
fn answers_req(r: &Value, channel: &Hash) -> Result<(NodeAnswers, Value), Fail> {
    match (opt_text(r, "node"), opt_text(r, "answers_dir")) {
        (Some(node), None) => {
            let save = text(r, "save_dir").bad()?;
            let from = opt_u64(r, "slot_from").bad()?.unwrap_or(0);
            let a = NodeAnswers::fetch(node, channel, from, Path::new(save)).fault()?;
            Ok((a, json!({ "node": node, "saved_to": save, "slot_from": from })))
        }
        (None, Some(dir)) => Ok((NodeAnswers::load(Path::new(dir)).bad()?, json!({ "answers_dir": dir }))),
        _ => Err(Fail::Invalid(anyhow!("exactly one of node (with save_dir) or answers_dir"))),
    }
}

// ---- one-shot methods --------------------------------------------------------------------------------

/// `keygen(request)`: `{out, seed_label?}`: a 32-byte attestation key (OS randomness; a label only for
/// reproducible devnet keys); never overwrites a file.
pub fn keygen(request: &str) -> String {
    reply((|| {
        let r = parse(request)?;
        let out = PathBuf::from(text(&r, "out").bad()?);
        let label = opt_text(&r, "seed_label");
        let secret: [u8; 32] = match label {
            Some(label) => sha256_parts(&[b"ferrata/attestor/devnet-key/", label.as_bytes()]),
            None => {
                let mut b = [0u8; 32];
                fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).context("/dev/urandom").fault()?;
                b
            }
        };
        if out.exists() {
            return Err(Fail::Refused(anyhow!("{} exists: a key is never overwritten", out.display())));
        }
        write_secret(&out, &secret).fault()?;
        let public = ed25519_dalek::SigningKey::from_bytes(&secret).verifying_key().to_bytes();
        Ok(json!({ "verdict": "ok", "public_key": hex::encode(public), "out": out, "devnet_label": label }))
    })())
}

fn write_secret(path: &Path, secret: &[u8; 32]) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt as _;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| path.display().to_string())?;
    f.write_all(secret)?;
    f.sync_all()?;
    Ok(())
}

/// `pubkey(request)`: `{key_file}`: the public key of a 32-byte secret key file.
pub fn pubkey(request: &str) -> String {
    reply((|| {
        let r = parse(request)?;
        let key = read_key(Path::new(text(&r, "key_file").bad()?)).bad()?;
        Ok(json!({ "verdict": "ok", "public_key": hex::encode(key.verifying_key().to_bytes()) }))
    })())
}

/// `deployment(request)`: `{input | input_file, out?}`: the deployment (chain, vault, `lezConfigId`,
/// channel) a `checkpoint_b` input's configuration serves.
pub fn deployment(request: &str) -> String {
    reply((|| {
        let r = parse(request)?;
        let bytes = input_req(&r, "input").bad()?;
        let input = CheckpointInputB::decode(&bytes).map_err(|e| anyhow!("input: {e:?}")).bad()?;
        let d = deployment_json(&deployment_of(&input.config));
        if let Some(out) = opt_text(&r, "out") {
            write_json(Path::new(out), &d).fault()?;
        }
        Ok(json!({ "verdict": "ok", "deployment": d }))
    })())
}

/// `certify(request)`: `{channel_key_file, att_pub, deployment | deployment_file, out?}`: the channel
/// key's one certificate of an attestation key, for this deployment.
pub fn certify(request: &str) -> String {
    reply((|| {
        let r = parse(request)?;
        let channel = read_key(Path::new(text(&r, "channel_key_file").bad()?)).bad()?;
        let d = deployment_req(&r).bad()?;
        let att_pub: Hash = hex_str(text(&r, "att_pub").bad()?, "att_pub").bad()?;
        let certificate = attestor_core::sign_certificate(&channel, &d, &att_pub);
        let channel_pub = channel.verifying_key().to_bytes();
        if !verify_certificate(&channel_pub, &d, &att_pub, &certificate) {
            return Err(Fail::Refused(anyhow!("certificate does not verify")));
        }
        let v = json!({
            "channel_key": hex::encode(channel_pub), "attestation_key": hex::encode(att_pub),
            "deployment": deployment_json(&d), "certificate_message": hex::encode(d.certificate_message(&att_pub)),
            "certificate": hex::encode(certificate),
        });
        if let Some(out) = opt_text(&r, "out") {
            write_json(Path::new(out), &v).fault()?;
        }
        Ok(json!({ "verdict": "certified", "cert": v }))
    })())
}

fn tier_json(answers: &NodeAnswers, channel: &Hash) -> Result<Value> {
    let tier = bedrock_tier(answers, channel)?;
    let signed_by = signers(answers, channel)?;
    let mut per: std::collections::BTreeMap<String, Vec<u64>> = std::collections::BTreeMap::new();
    for (id, k) in &signed_by {
        per.entry(hex::encode(k)).or_default().push(*id);
    }
    Ok(json!({
        "tip_id": tier.tip.tip_id, "tip_hash": hex::encode(tier.tip.tip_hash),
        "final_msg_id": hex::encode(tier.tip.final_msg_id), "tip_signer": hex::encode(tier.tip_signer),
        "first_block": tier.first_block, "blocks_checked": tier.hashes.len(), "lib_slot": tier.lib_slot,
        "keys": tier.keys.iter().map(hex::encode).collect::<Vec<_>>(), "posting_timeframe": tier.posting_timeframe,
        "blocks_per_signer": per.iter().map(|(k, ids)| (k.clone(), json!({"count": ids.len(),
            "first": ids.first(), "last": ids.last()}))).collect::<serde_json::Map<_, _>>(),
    }))
}

/// `tier(request)`: `{answers_dir | node + save_dir [+ slot_from], channel?, out?}`: the Bedrock node's
/// view of the final tier alone (tip, key set, blocks per signer), as the CLI's `tier` prints it: a
/// diagnostic, not what the attestor signs (that is checked against its LEZ node, `attest`).
pub fn tier(request: &str) -> String {
    reply((|| {
        let r = parse(request)?;
        let channel = channel_req(&r).bad()?;
        let (answers, source) = answers_req(&r, &channel)?;
        let mut v = tier_json(&answers, &channel).no()?;
        if let Some(out) = opt_text(&r, "out") {
            write_json(Path::new(out), &v).fault()?;
        }
        v["verdict"] = json!("ok");
        v["source"] = source;
        Ok(v)
    })())
}

fn attestor_from(d: Deployment, cert: &Value, att_key_file: &Path) -> Result<Attestor> {
    Ok(Attestor {
        deployment: d,
        channel_key: hexa(&cert["channel_key"], "cert channel_key")?,
        certificate: hexa(&cert["certificate"], "cert certificate")?,
        att_key: read_key(att_key_file)?,
    })
}

/// The reads `attest` signs from: a running node over HTTP (`node`, saved under `save_dir`, from the
/// signed tip's slot or `slot_from`) with the attestor's own LEZ node (`lez`, JSON-RPC), or saved answers
/// (`answers_dir`, with the LEZ node's saved answers in `lez_dir`, default `answers_dir`).
fn attest_reads(
    r: &Value,
    channel: &Hash,
    state: Option<&AttestorState>,
) -> Result<(NodeAnswers, Box<dyn LezNode>, Value), Fail> {
    match (opt_text(r, "node"), opt_text(r, "answers_dir")) {
        (Some(node), None) => {
            let save = text(r, "save_dir").bad()?;
            let lez = text(r, "lez").bad()?;
            let from = scan_from(state, opt_u64(r, "slot_from").bad()?.unwrap_or(0));
            let a = NodeAnswers::fetch(node, channel, from, Path::new(save)).fault()?;
            let l = LezRpc::new(lez, Path::new(save)).fault()?;
            Ok((a, Box::new(l), json!({ "node": node, "lez": lez, "saved_to": save, "slot_from": from })))
        }
        (None, Some(dir)) => {
            let lez_dir = opt_text(r, "lez_dir").unwrap_or(dir);
            let a = NodeAnswers::load(Path::new(dir)).bad()?;
            Ok((a, Box::new(LezSaved::new(Path::new(lez_dir))), json!({ "answers_dir": dir, "lez_dir": lez_dir })))
        }
        _ => Err(Fail::Invalid(anyhow!("exactly one of node (with lez and save_dir) or answers_dir"))),
    }
}

/// `attest(request)`: `{deployment | deployment_file, cert | cert_file, att_key_file, answers_dir [+ lez_dir] |
/// node + lez + save_dir + state_file [+ slot_from], channel?, state_file?, out?}`: read the node's final tier,
/// check its tip against the LEZ node, sign it (the CLI's `attest`). With `state_file`, the read starts at the
/// signed tip, refuses a tip that does not descend from it, and records the new one before `out` is written.
/// Against live nodes (`node`) a state file is required, as the CLI requires `--state` with `--node`: an
/// attestor that signs live keeps what it signed. A LEZ node that does not answer is an `error`, not a refusal.
pub fn attest(request: &str) -> String {
    reply((|| {
        let r = parse(request)?;
        let d = deployment_req(&r).bad()?;
        let cert = cert_req(&r).bad()?;
        let attestor = attestor_from(d, &cert, Path::new(text(&r, "att_key_file").bad()?)).bad()?;
        let state = opt_text(&r, "state_file").map(PathBuf::from);
        if opt_text(&r, "node").is_some() && state.is_none() {
            return Err(Fail::Invalid(anyhow!(
                "state_file is required with node: a live attestor keeps a state (what it signed)"
            )));
        }
        let before = match &state {
            Some(p) => read_state(p, &d.channel_id).bad()?,
            None => None,
        };
        let (answers, mut lez, source) = attest_reads(&r, &d.channel_id, before.as_ref())?;
        let tier = final_tier(&answers, lez.as_mut(), &d.channel_id, before.as_ref())
            .map_err(|e| if is_node_fault(&e) { Fail::Error(e) } else { Fail::Refused(e) })?;
        let att = attestor.attest(&tier, before.as_ref().map(|s| s.last)).no()?;
        let record = attestation_json(&d, &att, &tier);
        if let Some(p) = &state {
            write_state(p, &d.channel_id, &AttestorState::of(&tier)).fault()?;
        }
        if let Some(out) = opt_text(&r, "out") {
            let text = serde_json::to_string_pretty(&record).map_err(anyhow::Error::from).fault()? + "\n";
            write_atomic(Path::new(out), text.as_bytes()).fault()?;
        }
        Ok(json!({
            "verdict": "attested", "tip_id": att.tip.tip_id, "tip_hash": hex::encode(att.tip.tip_hash),
            "final_msg_id": hex::encode(att.tip.final_msg_id), "lib_slot": tier.lib_slot,
            "blocks_checked": tier.hashes.len(), "first_block": tier.first_block, "n": tier.keys.len(),
            "key_set_hash": hex::encode(tier.key_set_hash), "source": source, "attestation": record,
            "notes": tier.notes,
        }))
    })())
}

/// `verify(request)`: `{deployment | deployment_file, attestations? , att_files?}`: anyone's check of
/// published attestations (the certificate and the final-tier signature, over the key-set hash each carries).
pub fn verify(request: &str) -> String {
    reply((|| {
        let r = parse(request)?;
        let d = deployment_req(&r).bad()?;
        let (atts, records, names) = attestations_req(&r).bad()?;
        let mut results = Vec::new();
        let mut bad = 0;
        for ((att, v), name) in atts.iter().zip(&records).zip(&names) {
            let ksh: Hash = hexa(&v["key_set_hash"], "key_set_hash").bad()?;
            match att.verify(&d, &ksh) {
                Ok(()) => results.push(json!({ "name": name, "verifies": true,
                    "channel_key": hex::encode(att.channel_key), "tip_id": att.tip.tip_id })),
                Err(e) => {
                    bad += 1;
                    results.push(json!({ "name": name, "verifies": false, "error": format!("{e:?}"),
                        "channel_key": hex::encode(att.channel_key), "tip_id": att.tip.tip_id }));
                }
            }
        }
        Ok(json!({ "verdict": if bad == 0 { "verified" } else { "refused" }, "refused": bad, "results": results }))
    })())
}

/// `collect(request)`: `{base | base_file, attestations?, att_files?, name?, description?, out?,
/// meta_out?}`: the valid attestations (channel key in the parent's set, certificate and signature
/// good, no key repeated) into the input; refused below `t(n)`; the rule run natively.
pub fn collect(request: &str) -> String {
    reply((|| {
        let r = parse(request)?;
        let base = input_req(&r, "base").bad()?;
        let (atts, _, names) = attestations_req(&r).bad()?;
        let c = collect_atts(&base, &atts).no()?;
        let name = opt_text(&r, "name").unwrap_or("collected");
        let description = opt_text(&r, "description").unwrap_or("rule (B): t of n attestations collected by ferrata_attestor_module");
        let meta = fixture_meta(name, &c.input, description, None).no()?;
        if let Some(out) = opt_text(&r, "out") {
            write_bytes(Path::new(out), &c.input).fault()?;
        }
        if let Some(out) = opt_text(&r, "meta_out") {
            write_json(Path::new(out), &meta).fault()?;
        }
        Ok(json!({
            "verdict": "collected", "n": c.n, "t": c.t,
            "kept": c.kept.iter().map(|i| &names[*i]).collect::<Vec<_>>(),
            "dropped": c.dropped.iter().map(|(i, why)| json!({ "name": names[*i], "reason": format!("{why:?}") })).collect::<Vec<_>>(),
            "native": meta["native"], "journal_sha256": meta["journal_sha256"],
            "input_bytes": c.input.len(), "input_sha256": sha256_hex(&c.input), "meta": meta,
        }))
    })())
}

/// `assemble(request)`: `{base | base_file, attestations?, att_files?, name, expect, description?,
/// out?, meta_out?}`: the base with exactly these attestations, unchecked: an input the checkpoint
/// program must refuse; the native run must refuse it with an error starting with `expect`.
pub fn assemble(request: &str) -> String {
    reply((|| {
        let r = parse(request)?;
        let base = input_req(&r, "base").bad()?;
        let (atts, _, _) = attestations_req(&r).bad()?;
        let name = text(&r, "name").bad()?;
        let expect = text(&r, "expect").bad()?;
        let bytes = assemble_unchecked(&base, &atts).no()?;
        let meta = fixture_meta(name, &bytes, opt_text(&r, "description").unwrap_or("an input the program must refuse"), Some(expect))
            .no()?;
        if let Some(out) = opt_text(&r, "out") {
            write_bytes(Path::new(out), &bytes).fault()?;
        }
        if let Some(out) = opt_text(&r, "meta_out") {
            write_json(Path::new(out), &meta).fault()?;
        }
        Ok(json!({ "verdict": "assembled", "native": meta["native"], "input_bytes": bytes.len(),
                   "input_sha256": sha256_hex(&bytes), "meta": meta }))
    })())
}

/// `base(request)`: `{answers_dir, genesis_file, n_min, t_min, from?, to?, channel?, out}`: devnet
/// set-up, the CLI's `base`: an input over final blocks `from..=to` from the node's answers alone (the
/// configuration's bridge values are stand-ins). Without `from`/`to`: from the block after the first
/// final inscription to the final tip, as `attestor/zone-attest.sh` takes them.
pub fn base(request: &str) -> String {
    reply((|| {
        let r = parse(request)?;
        let channel = channel_req(&r).bad()?;
        let answers = NodeAnswers::load(Path::new(text(&r, "answers_dir").bad()?)).bad()?;
        let genesis = fs::read(text(&r, "genesis_file").bad()?).context("genesis_file").bad()?;
        let genesis_id = sha256_parts(&[genesis.as_slice()]);
        let (from, to) = match (opt_u64(&r, "from").bad()?, opt_u64(&r, "to").bad()?) {
            (Some(f), Some(t)) => (f, t),
            (None, None) => {
                let tier = bedrock_tier(&answers, &channel).no()?;
                (tier.first_block + 1, tier.tip.tip_id)
            }
            _ => return Err(Fail::Invalid(anyhow!("give both from and to, or neither"))),
        };
        let n_min = u32::try_from(u64_of(&r, "n_min").bad()?).context("n_min").bad()?;
        let t_min = u32::try_from(u64_of(&r, "t_min").bad()?).context("t_min").bad()?;
        let bytes = base_from_answers(&answers, &channel, genesis_id, from, to, n_min, t_min).no()?;
        let out = text(&r, "out").bad()?;
        write_bytes(Path::new(out), &bytes).fault()?;
        let input = CheckpointInputB::decode(&bytes).map_err(|e| anyhow!("{e:?}")).no()?;
        Ok(json!({
            "verdict": "ok", "from": from, "to": to, "n": input.parent.keys.len(),
            "config_id": hex::encode(input.config.config_id()), "parent_state": hex::encode(input.parent.digest()),
            "deployment": deployment_json(&deployment_of(&input.config)),
            "input_bytes": bytes.len(), "input_sha256": sha256_hex(&bytes), "out": out,
        }))
    })())
}

// ---- the attestor as a daemon ------------------------------------------------------------------------

struct DaemonCfg {
    attestor: Attestor,
    node: String,
    lez: String,
    slot_from: u64,
    work_dir: PathBuf,
    publish_dir: PathBuf,
    state_file: PathBuf,
    poll: Duration,
    max_rounds: Option<u64>,
    keep_rounds: u64,
}

#[derive(Default)]
struct Shared {
    running: bool,
    started_at: u64,
    stopped_at: Option<u64>,
    rounds: u64,
    attested: u64,
    unchanged: u64,
    refused: u64,
    errors: u64,
    last_round: Value,
    last_attestation: Value,
    history: VecDeque<Value>,
}

/// The attestor's loop in a thread of the module's process; [`Daemon::stop`] ends it between rounds.
pub struct Daemon {
    stop: Arc<AtomicBool>,
    shared: Arc<Mutex<Shared>>,
    handle: Option<JoinHandle<()>>,
    config: Value,
}

impl Daemon {
    /// `start(request)`: `{deployment | deployment_file, cert | cert_file, att_key_file, node, lez, work_dir,
    /// publish_dir, state_file, poll_secs? (10), slot_from? (0, a first read only), max_rounds?, keep_rounds? (2)}`.
    /// Refuses at once a certificate that does not certify the key for this deployment.
    pub fn start(request: &str) -> Result<Self, String> {
        let r = parse(request).map_err(|f| reply(Err(f)))?;
        Self::start_parsed(&r).map_err(|f| reply(Err(f)))
    }

    fn start_parsed(r: &Value) -> Result<Self, Fail> {
        let d = deployment_req(r).bad()?;
        let cert = cert_req(r).bad()?;
        let attestor = attestor_from(d, &cert, Path::new(text(r, "att_key_file").bad()?)).bad()?;
        let att_pub = attestor.att_key.verifying_key().to_bytes();
        if !verify_certificate(&attestor.channel_key, &d, &att_pub, &attestor.certificate) {
            return Err(Fail::Refused(anyhow!(
                "the certificate does not certify attestation key {} by channel key {} for this deployment",
                hex::encode(att_pub),
                hex::encode(attestor.channel_key)
            )));
        }
        let cfg = DaemonCfg {
            node: text(r, "node").bad()?.to_string(),
            lez: text(r, "lez").bad()?.to_string(),
            slot_from: opt_u64(r, "slot_from").bad()?.unwrap_or(0),
            work_dir: PathBuf::from(text(r, "work_dir").bad()?),
            publish_dir: PathBuf::from(text(r, "publish_dir").bad()?),
            state_file: PathBuf::from(text(r, "state_file").bad()?),
            poll: Duration::from_millis(opt_u64(r, "poll_ms").bad()?.unwrap_or(1000 * opt_u64(r, "poll_secs").bad()?.unwrap_or(10))),
            max_rounds: opt_u64(r, "max_rounds").bad()?,
            keep_rounds: opt_u64(r, "keep_rounds").bad()?.unwrap_or(2).max(1),
            attestor,
        };
        fs::create_dir_all(&cfg.work_dir).with_context(|| cfg.work_dir.display().to_string()).fault()?;
        fs::create_dir_all(&cfg.publish_dir).with_context(|| cfg.publish_dir.display().to_string()).fault()?;
        let config = json!({
            "node": cfg.node, "lez": cfg.lez, "slot_from": cfg.slot_from, "work_dir": cfg.work_dir, "publish_dir": cfg.publish_dir,
            "state_file": cfg.state_file, "poll_ms": cfg.poll.as_millis() as u64, "max_rounds": cfg.max_rounds,
            "channel_key": hex::encode(cfg.attestor.channel_key), "attestation_key": hex::encode(att_pub),
            "deployment": deployment_json(&d),
        });
        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Mutex::new(Shared { running: true, started_at: now_secs(), ..Shared::default() }));
        let (s2, sh2) = (stop.clone(), shared.clone());
        let handle = std::thread::Builder::new()
            .name("ferrata-attestor".into())
            .spawn(move || run_loop(cfg, s2, sh2))
            .context("spawning the attestor's thread")
            .fault()?;
        Ok(Self { stop, shared, handle: Some(handle), config })
    }

    pub fn running(&self) -> bool {
        self.shared.lock().map(|s| s.running).unwrap_or(false)
    }

    /// `status()`: rounds, what each did, the last attestation.
    pub fn status(&self) -> String {
        let s = match self.shared.lock() {
            Ok(s) => s,
            Err(p) => p.into_inner(),
        };
        json!({
            "verdict": "ok", "running": s.running, "started_at": s.started_at, "stopped_at": s.stopped_at,
            "rounds": s.rounds, "attested": s.attested, "unchanged": s.unchanged, "refused": s.refused,
            "errors": s.errors, "last_round": s.last_round, "last_attestation": s.last_attestation,
            "history": s.history.iter().collect::<Vec<_>>(), "config": self.config,
        })
        .to_string()
    }

    /// `stop()`: ends the loop after the round in hand and waits for it.
    pub fn stop(&mut self) -> String {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        self.status()
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn run_loop(cfg: DaemonCfg, stop: Arc<AtomicBool>, shared: Arc<Mutex<Shared>>) {
    let mut round = 0u64;
    let mut published: Option<u64> = None;
    while !stop.load(Ordering::SeqCst) {
        round += 1;
        let started = now_secs();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| one_round(&cfg, round, &mut published)))
            .unwrap_or_else(|_| Round::Error(anyhow!("the round panicked")));
        if round > cfg.keep_rounds {
            let _ = fs::remove_dir_all(cfg.work_dir.join(format!("round-{}", round - cfg.keep_rounds)));
        }
        let mut record = json!({ "round": round, "at": started, "seconds": now_secs() - started });
        {
            let mut s = match shared.lock() {
                Ok(s) => s,
                Err(p) => p.into_inner(),
            };
            s.rounds = round;
            match outcome {
                Round::Attested(v, new) => {
                    if new {
                        s.attested += 1;
                    } else {
                        s.unchanged += 1;
                    }
                    record["outcome"] = json!(if new { "attested" } else { "unchanged" });
                    record["tip_id"] = v["tip_id"].clone();
                    record["lib_slot"] = v["lib_slot"].clone();
                    s.last_attestation = v;
                }
                Round::Refused(e) => {
                    s.refused += 1;
                    record["outcome"] = json!("refused");
                    record["error"] = json!(format!("{e:#}"));
                }
                Round::Error(e) => {
                    s.errors += 1;
                    record["outcome"] = json!("error");
                    record["error"] = json!(format!("{e:#}"));
                }
            }
            s.last_round = record.clone();
            s.history.push_back(record.clone());
            while s.history.len() > 50 {
                s.history.pop_front();
            }
        }
        append_log(&cfg.work_dir.join("attestor.log"), &record);
        if cfg.max_rounds.is_some_and(|m| round >= m) {
            break;
        }
        let mut slept = Duration::ZERO;
        while slept < cfg.poll && !stop.load(Ordering::SeqCst) {
            let step = Duration::from_millis(100).min(cfg.poll - slept);
            std::thread::sleep(step);
            slept += step;
        }
    }
    let mut s = match shared.lock() {
        Ok(s) => s,
        Err(p) => p.into_inner(),
    };
    s.running = false;
    s.stopped_at = Some(now_secs());
}

enum Round {
    /// The attestation's summary; whether its tip is new (published).
    Attested(Value, bool),
    Refused(anyhow::Error),
    Error(anyhow::Error),
}

fn one_round(cfg: &DaemonCfg, n: u64, published: &mut Option<u64>) -> Round {
    let d = cfg.attestor.deployment;
    let dir = cfg.work_dir.join(format!("round-{n}"));
    let _ = fs::remove_dir_all(&dir);
    // Read from the signed tip's slot, check against the LEZ node, sign, record the state: then publish.
    let r = match round(&cfg.attestor, &cfg.node, &cfg.lez, &dir, &cfg.state_file, cfg.slot_from) {
        Ok(r) => r,
        Err(RoundError::Refused(e)) => return Round::Refused(e),
        Err(RoundError::Node(e)) => return Round::Error(e.context("reading the nodes")),
        Err(RoundError::State(e)) => return Round::Error(e.context("the state file")),
    };
    let (att, tier) = (&r.attestation, &r.tier);
    let record = attestation_json(&d, att, tier);
    let tip = att.tip.tip_id;
    let new = *published != Some(tip);
    let file = cfg.publish_dir.join(format!("att-{tip}.json"));
    let written = (|| -> Result<()> {
        let bytes = serde_json::to_string_pretty(&record)? + "\n";
        if new {
            write_atomic(&file, bytes.as_bytes())?;
        }
        write_atomic(&cfg.publish_dir.join("latest.json"), bytes.as_bytes())
    })();
    if let Err(e) = written {
        return Round::Error(e.context("publishing"));
    }
    *published = Some(tip);
    Round::Attested(
        json!({ "tip_id": tip, "tip_hash": hex::encode(att.tip.tip_hash), "lib_slot": tier.lib_slot,
                "blocks_checked": tier.hashes.len(), "n": tier.keys.len(), "file": file, "answers": dir,
                "read_from_slot": r.scanned_from, "notes": tier.notes }),
        new,
    )
}

fn append_log(path: &Path, v: &Value) {
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{v}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..")
    }

    fn call(f: fn(&str) -> String, r: Value) -> Value {
        serde_json::from_str(&f(&r.to_string())).unwrap()
    }

    #[test]
    fn unreadable_requests_are_refused_as_such() {
        for f in [attest as fn(&str) -> String, collect, verify, certify, tier, base] {
            assert_eq!(call(f, json!({}))["verdict"], "invalid_request");
            assert_eq!(serde_json::from_str::<Value>(&f("[1]")).unwrap()["verdict"], "invalid_request");
        }
    }

    /// The zone of four's saved answers (checkpoint/attestor/runs/zone): through these functions, the
    /// collector's 3 of 4 is the input `attestor/zone-attest.sh` wrote and the checkpoint program proved.
    #[test]
    fn the_zone_replayed_gives_the_proven_input() {
        let zone = repo().join("checkpoint/attestor/runs/zone");
        let dev = repo().join("../upstream/lez-v0.4.0-rc1/lez/configs/docker-all-in-one/devnet");
        if !dev.exists() || !zone.exists() {
            eprintln!("skipped: {} or {} missing", dev.display(), zone.display());
            return;
        }
        let tmp = std::env::temp_dir().join(format!("ferrata-attestor-calls-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let answers = zone.join("answers");
        // The zone run recorded no LEZ node's answers: its blocks, all applied there, stood in as final.
        let recorded = NodeAnswers::load(&answers).unwrap();
        let standin =
            ferrata_attestor::devnet_lez_standin(&recorded, &hex_str(DEFAULT_CHANNEL, "channel").unwrap()).unwrap();
        ferrata_attestor::save_lez_answers(&tmp, &standin).unwrap();
        let base_file = tmp.join("base.input");
        let b = call(
            base,
            json!({"answers_dir": answers, "genesis_file": zone.join("deployment-settings.yaml"),
            "n_min": 4, "t_min": 3, "out": base_file}),
        );
        assert_eq!(b["verdict"], "ok", "{b}");
        let dep = b["deployment"].clone();
        let mut atts = Vec::new();
        for i in 0..4 {
            let key = tmp.join(format!("attestation-{i}.key"));
            let k = call(keygen, json!({"out": key, "seed_label": format!("zone-attestation-{i}")}));
            let c = call(
                certify,
                json!({"channel_key_file": dev.join(format!("seq-{i}/channel_signing_key")),
                "att_pub": k["public_key"], "deployment": dep}),
            );
            assert_eq!(c["verdict"], "certified", "{c}");
            let a = call(
                attest,
                json!({"deployment": dep, "cert": c["cert"], "att_key_file": key,
                "answers_dir": answers, "lez_dir": tmp, "state_file": tmp.join(format!("state-{i}.json"))}),
            );
            assert_eq!(a["verdict"], "attested", "{a}");
            atts.push(a["attestation"].clone());
        }
        let v = call(verify, json!({"deployment": dep, "attestations": atts}));
        assert_eq!(v["verdict"], "verified", "{v}");
        let c = call(collect, json!({"base_file": base_file, "attestations": atts[..3]}));
        assert_eq!(c["verdict"], "collected", "{c}");
        let proven = fs::read(zone.join("rbz-zone-n4-t3.input")).unwrap();
        assert_eq!(c["input_sha256"], json!(sha256_hex(&proven)));
        let two = call(collect, json!({"base_file": base_file, "attestations": atts[..2]}));
        assert_eq!(two["verdict"], "refused", "{two}");
        // The attestor never signs below what it signed: a state naming a later tip.
        let st = tmp.join("state-ahead.json");
        fs::write(&st, json!({"tip_id": 1000, "tip_hash": "00".repeat(32)}).to_string()).unwrap();
        let key0 = tmp.join("attestation-0.key");
        let c0 = call(
            certify,
            json!({"channel_key_file": dev.join("seq-0/channel_signing_key"),
            "att_pub": call(pubkey, json!({"key_file": key0}))["public_key"], "deployment": dep}),
        );
        let behind = call(
            attest,
            json!({"deployment": dep, "cert": c0["cert"], "att_key_file": key0,
            "answers_dir": answers, "lez_dir": tmp, "state_file": st}),
        );
        assert_eq!(behind["verdict"], "refused", "{behind}");
        // Without the LEZ node's answers nothing is final there: refused.
        let k1 = tmp.join("attestation-1.key");
        let c1 = call(
            certify,
            json!({"channel_key_file": dev.join("seq-1/channel_signing_key"),
            "att_pub": call(pubkey, json!({"key_file": k1}))["public_key"], "deployment": dep}),
        );
        let no_lez = call(
            attest,
            json!({"deployment": dep, "cert": c1["cert"], "att_key_file": k1,
            "answers_dir": answers}),
        );
        assert_eq!(no_lez["verdict"], "refused", "{no_lez}");
        let _ = fs::remove_dir_all(&tmp);
    }

    /// A Logos Blockchain node over HTTP serving saved answers (`GET` only: what `NodeAnswers::fetch` asks).
    fn serve_node(answers: &Path) -> String {
        use std::io::{BufRead as _, BufReader, Write as _};
        let info = fs::read(answers.join("info.json")).unwrap();
        let channel = fs::read(answers.join("channel.json")).unwrap();
        let blocks = NodeAnswers::load(answers).unwrap().blocks;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { continue };
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut line = String::new();
                if r.read_line(&mut line).is_err() {
                    continue;
                }
                loop {
                    let mut h = String::new();
                    if r.read_line(&mut h).is_err() || h == "\r\n" || h.is_empty() {
                        break;
                    }
                }
                let target = line.split_whitespace().nth(1).unwrap_or("").to_string();
                let body = if target == "/cryptarchia/info" {
                    info.clone()
                } else if target.starts_with("/channel/") {
                    channel.clone()
                } else if let Some(q) = target.strip_prefix("/cryptarchia/blocks?") {
                    let get = |k: &str| -> u64 {
                        q.split('&').find_map(|kv| kv.strip_prefix(&format!("{k}="))).and_then(|v| v.parse().ok()).unwrap_or(0)
                    };
                    let (from, to) = (get("slot_from"), get("slot_to"));
                    let sel: Vec<&Value> =
                        blocks.iter().filter(|b| (from..=to).contains(&b["header"]["slot"].as_u64().unwrap_or(0))).collect();
                    serde_json::to_vec(&sel).unwrap()
                } else {
                    b"null".to_vec()
                };
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(head.as_bytes()).and_then(|()| s.write_all(&body));
            }
        });
        url
    }

    /// Review L4: against live nodes, `attest` requires a state file (the CLI requires `--state` with `--node`),
    /// and a LEZ node that does not answer is an `error` (the daemon's and the CLI's classification), not a refusal.
    #[test]
    fn a_live_attest_keeps_a_state_and_a_silent_lez_node_is_an_error() {
        let zone = repo().join("checkpoint/attestor/runs/zone");
        let dev = repo().join("../upstream/lez-v0.4.0-rc1/lez/configs/docker-all-in-one/devnet");
        if !dev.exists() || !zone.exists() {
            eprintln!("skipped: {} or {} missing", dev.display(), zone.display());
            return;
        }
        let tmp = std::env::temp_dir().join(format!("ferrata-attestor-calls-l4-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let answers = zone.join("answers");
        let key = tmp.join("attestation-0.key");
        let k = call(keygen, json!({"out": key, "seed_label": "zone-attestation-0"}));
        let c = call(
            certify,
            json!({"channel_key_file": dev.join("seq-0/channel_signing_key"), "att_pub": k["public_key"],
            "deployment_file": zone.join("deployment.json")}),
        );
        assert_eq!(c["verdict"], "certified", "{c}");
        let from = NodeAnswers::load(&answers).unwrap().blocks.iter().filter_map(|b| b["header"]["slot"].as_u64()).min().unwrap();
        let node = serve_node(&answers);
        // Port 1 on loopback: nothing listens, curl's connection is refused.
        let live = |save: &str, state: Option<&Path>| {
            let mut r = json!({"deployment_file": zone.join("deployment.json"), "cert": c["cert"], "att_key_file": key,
                "node": node, "lez": "http://127.0.0.1:1", "save_dir": tmp.join(save), "slot_from": from});
            if let Some(s) = state {
                r["state_file"] = json!(s);
            }
            call(attest, r)
        };
        let no_state = live("save-1", None);
        assert_eq!(no_state["verdict"], "invalid_request", "{no_state}");
        assert!(no_state["error"].as_str().unwrap().contains("state_file is required with node"), "{no_state}");
        let state = tmp.join("state.json");
        let silent = live("save-2", Some(&state));
        assert_eq!(silent["verdict"], "error", "{silent}");
        assert!(silent["error"].as_str().unwrap().contains("LEZ node http://127.0.0.1:1"), "{silent}");
        assert!(!state.exists(), "nothing signed, nothing recorded");
        let _ = fs::remove_dir_all(&tmp);
    }
}
