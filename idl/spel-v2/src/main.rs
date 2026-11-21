//! SPEL's LEZ v0.4 line reads Ferrata's programs (RFP-021 U.4, RFP-022 U.4).
//!
//! `spel-framework-core` at logos-co/spel PR #301 (head c90ca0b, LEZ v0.4.0-rc3):
//! 1. **generates** each program's IDL (version 2) from its core crate with `idl_gen::generate_idl`, the code behind
//!    `spel generate-idl` (the crate's source read with `syn`, no annotations): the attestation program from
//!    `lez/wire` (message enum `Message`; its three actor states `InstanceState`, `RecordState`, `JobState` have no
//!    enum over them, so the IDL is generated once per state and the types merged, each type required to be identical
//!    in every generation); the bridge program from `lez/bridge-wire` (`BridgeMessage`, state `BridgeState`);
//! 2. **compares** each generated IDL with the hand-written one (`idl/<name>.idl.json`, SPEL's earlier format): every
//!    message (name, tag, fields with their types) against the hand-written message enum's variants in order, and every
//!    generated type against the hand-written type or account type of the same name (kind, fields, variants), docs
//!    aside; the other way, every hand-written type must be in the generated IDL;
//! 3. **decodes** every committed vector (`idl/vectors/<name>.vectors.json`) with SPEL's own decoder
//!    (`decode::decode_named`, every byte consumed) on the generated IDL, and, given a file, every recorded message
//!    (`lez/live/driver`'s `idl-recorded` output);
//! 4. checks that a vector one byte short, one byte long, or with its tag out of range is refused by that decoder.
//!
//! `cargo run --locked -- OUT_DIR [RECORDED.json]` (from `idl/spel-v2`): writes `OUT_DIR/<name>.idl-v2.json`; exit 1
//! on any failure.

use std::{fs, path::PathBuf, process::ExitCode};

use serde_json::{json, Value};
use spel_framework_core::{
    decode::decode_named,
    idl::SpelIdl,
    idl_gen::{generate_idl, GenOptions},
};

struct Program {
    name: &'static str,
    crate_dir: &'static str,
    message: &'static str,
    states: &'static [&'static str],
}

const PROGRAMS: [Program; 2] = [
    Program {
        name: "ferrata_attestation",
        crate_dir: "lez/wire",
        message: "Message",
        // `Attestation` is no actor's state: it is the reply and the event payload. A generation with it as `state`
        // brings its types (`Statement`, `Fact`) in; the IDL keeps `InstanceState` as its `state`.
        states: &["InstanceState", "RecordState", "JobState", "Attestation"],
    },
    Program { name: "ferrata_bridge", crate_dir: "lez/bridge-wire", message: "BridgeMessage", states: &["BridgeState"] },
];

/// The hand-written IDL's names for the types SPEL synthesizes (with the tuple patch): `(Root, u128)` is the
/// bridge's `FeeShare`. The encoding is the same; only the name differs.
const RENAMES: [(&str, &str); 1] = [("Tuple_Root_u128", "FeeShare")];

fn rename(v: &Value) -> Value {
    match v {
        Value::String(s) => Value::String(RENAMES.iter().find(|(g, _)| g == s).map_or_else(|| s.clone(), |(_, h)| (*h).to_string())),
        Value::Object(m) => Value::Object(m.iter().map(|(k, v)| (k.clone(), rename(v))).collect()),
        Value::Array(a) => Value::Array(a.iter().map(rename).collect()),
        other => other.clone(),
    }
}

/// The IDL with its messages also as an enum type named `name`: a message is encoded as its tag byte, then its fields
/// (IDL v2, "messages"), which is how SPEL's decoder reads an enum; so `decode_named(bytes, name, ..)` decodes a
/// message with SPEL's own code.
fn with_message_enum(idl: &SpelIdl, name: &str) -> SpelIdl {
    let mut out = idl.clone();
    let variants: Vec<Value> = idl
        .messages
        .iter()
        .map(|m| json!({ "name": m.name, "fields": serde_json::to_value(&m.fields).unwrap() }))
        .collect();
    assert!(idl.messages.iter().enumerate().all(|(i, m)| usize::from(m.tag) == i), "tags are positional");
    out.types.push(serde_json::from_value(json!({ "name": name, "kind": "enum", "variants": variants })).unwrap());
    out
}

/// Strips `docs` and the hand-written IDL's `x-` annotations (`x-rust`: the Rust type a definition stands for)
/// everywhere, so that comparisons are about the encoding.
fn no_docs(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter().filter(|(k, _)| *k != "docs" && !k.starts_with("x-")).map(|(k, v)| (k.clone(), no_docs(v))).collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(no_docs).collect()),
        other => other.clone(),
    }
}

fn generate(p: &Program, repo: &PathBuf, failures: &mut Vec<String>) -> Option<SpelIdl> {
    let mut merged: Option<SpelIdl> = None;
    for state in p.states {
        let options = GenOptions {
            crate_dir: repo.join(p.crate_dir),
            message: p.message.to_string(),
            state: Some((*state).to_string()),
            events: None,
            namespace: Some(p.name.to_string()),
        };
        let idl = match generate_idl(&options, |w| println!("  {}: generator warning: {w}", p.name)) {
            Ok(idl) => idl,
            Err(e) => {
                failures.push(format!("{}: generate_idl (state {state}) failed: {e}", p.name));
                return None;
            }
        };
        match &mut merged {
            None => merged = Some(idl),
            Some(m) => {
                for t in idl.types {
                    match m.types.iter().find(|x| x.name == t.name) {
                        Some(existing) => {
                            if serde_json::to_value(existing).ok() != serde_json::to_value(&t).ok() {
                                failures.push(format!("{}: type {} differs between generations", p.name, t.name));
                            }
                        }
                        None => m.types.push(t),
                    }
                }
            }
        }
    }
    let mut idl = merged?;
    idl.types.sort_by(|a, b| a.name.cmp(&b.name));
    Some(idl)
}

/// The generated IDL against the hand-written one. Returns (items compared, differences).
fn compare(name: &str, generated: &Value, hand: &Value, failures: &mut Vec<String>) -> usize {
    let mut compared = 0;
    let hand_types: Vec<Value> = hand["types"]
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .chain(hand["accounts"].as_array().unwrap().iter().map(|a| a["type"].clone()))
        .map(|t| no_docs(&t))
        .collect();
    let find = |n: &str| hand_types.iter().find(|t| t["name"] == n).cloned();
    // Messages: the hand-written message enum's variants, in order.
    let instruction_type = hand["instruction_type"].as_str().unwrap().rsplit("::").next().unwrap().to_string();
    let enum_def = find(&instruction_type).expect("the hand-written IDL has its message enum");
    let variants = enum_def["variants"].as_array().unwrap();
    let messages = generated["messages"].as_array().unwrap();
    if messages.len() != variants.len() {
        failures.push(format!("{name}: {} generated messages, {} hand-written variants", messages.len(), variants.len()));
    }
    for (i, (m, v)) in messages.iter().zip(variants).enumerate() {
        compared += 1;
        let m = rename(&no_docs(m));
        let gen_fields = m.get("fields").cloned().unwrap_or(json!([]));
        let hand_fields = v.get("fields").cloned().unwrap_or(json!([]));
        if m["name"] != v["name"] || m["tag"].as_u64() != Some(i as u64) || gen_fields != hand_fields {
            failures.push(format!("{name}: message {i}: generated {m} vs hand-written {v}"));
        }
    }
    // Types, both ways.
    let gen_types: Vec<Value> = generated["types"].as_array().unwrap().iter().map(|t| rename(&no_docs(t))).collect();
    for t in &gen_types {
        let n = t["name"].as_str().unwrap();
        compared += 1;
        match find(n) {
            Some(h) if &h == t => {}
            Some(h) => failures.push(format!("{name}: type {n}: generated {t} vs hand-written {h}")),
            None => failures.push(format!("{name}: type {n} is generated but not in the hand-written IDL")),
        }
    }
    for h in &hand_types {
        let n = h["name"].as_str().unwrap();
        if n != instruction_type && !gen_types.iter().any(|t| t["name"] == n) {
            failures.push(format!("{name}: hand-written type {n} is not generated"));
        }
    }
    compared
}

fn decode_file(idl: &SpelIdl, label: &str, path: &PathBuf, failures: &mut Vec<String>) -> (usize, usize) {
    let file: Value = serde_json::from_str(&fs::read_to_string(path).expect("vectors")).expect("JSON");
    let (mut ok, mut n) = (0, 0);
    for v in file["vectors"].as_array().expect("vectors") {
        n += 1;
        let bytes = hex::decode(v["hex"].as_str().unwrap()).unwrap();
        let ty = v["type"].as_str().unwrap();
        let vname = v["name"].as_str().unwrap_or("?");
        match decode_named(&bytes, ty, idl) {
            Ok(json) => {
                ok += 1;
                if ok == 1 {
                    let text = json.to_string();
                    println!("    first: {vname} -> {}", &text[..text.len().min(160)]);
                }
                // A byte short, a byte long: refused. A tag out of range: refused (messages and state enums).
                let mut bad = Vec::new();
                if decode_named(&bytes[..bytes.len() - 1], ty, idl).is_ok() {
                    bad.push("a byte short");
                }
                if decode_named(&[bytes.as_slice(), &[0]].concat(), ty, idl).is_ok() {
                    bad.push("a byte long");
                }
                if ty.ends_with("Message") || ty == "BridgeState" {
                    let mut t = bytes.clone();
                    t[0] = 0xff;
                    if decode_named(&t, ty, idl).is_ok() {
                        bad.push("tag 255");
                    }
                }
                if !bad.is_empty() {
                    failures.push(format!("{label} {vname}: SPEL's decoder accepts it {}", bad.join(", ")));
                }
            }
            Err(e) => failures.push(format!("{label} {vname}: SPEL's decoder refuses it: {e}")),
        }
    }
    println!("    {label}: SPEL v2 decoded {ok} of {n} exactly; mutants refused");
    (ok, n)
}

fn main() -> ExitCode {
    let repo = std::env::var_os("FERRATA_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .canonicalize()
        .unwrap();
    let (mut out, mut recorded, mut only, mut expect_fail) = (None, None, None, None);
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--only" => only = args.next(),
            "--expect-generation-failure" => expect_fail = args.next(),
            _ if out.is_none() => out = Some(PathBuf::from(a)),
            _ => recorded = Some(PathBuf::from(a)),
        }
    }
    let out = out.expect("usage: OUT_DIR [RECORDED.json] [--only NAME] [--expect-generation-failure NAME]");
    fs::create_dir_all(&out).unwrap();
    let mut failures = Vec::new();
    for p in PROGRAMS.iter().filter(|p| only.as_deref().is_none_or(|o| o == p.name)) {
        println!("{}: generate_idl({}, message {}, states {:?})", p.name, p.crate_dir, p.message, p.states);
        let before = failures.len();
        let generated_idl = generate(p, &repo, &mut failures);
        if expect_fail.as_deref() == Some(p.name) {
            match generated_idl {
                None => {
                    println!("  expected: {}", failures.drain(before..).collect::<Vec<_>>().join("; "));
                    continue;
                }
                Some(_) => failures.push(format!("{}: generation was expected to fail and did not", p.name)),
            }
        }
        let Some(idl) = generated_idl else { continue };
        let generated = serde_json::to_value(&idl).unwrap();
        println!(
            "  generated: IDL version {}, {} messages, {} types",
            generated["version"],
            generated["messages"].as_array().map_or(0, Vec::len),
            generated["types"].as_array().map_or(0, Vec::len)
        );
        let path = out.join(format!("{}.idl-v2.json", p.name));
        fs::write(&path, serde_json::to_string_pretty(&generated).unwrap() + "\n").unwrap();
        println!("  wrote {}", path.display());
        let hand: Value =
            serde_json::from_str(&fs::read_to_string(repo.join(format!("idl/{}.idl.json", p.name))).unwrap()).unwrap();
        let before = failures.len();
        let compared = compare(p.name, &generated, &hand, &mut failures);
        println!(
            "  against the hand-written IDL: {compared} messages and types compared, {} difference(s)",
            failures.len() - before
        );
        let decoding = with_message_enum(&idl, p.message);
        decode_file(&decoding, "committed vectors", &repo.join(format!("idl/vectors/{}.vectors.json", p.name)), &mut failures);
        if let (Some(r), "ferrata_attestation") = (&recorded, p.name) {
            decode_file(&decoding, "recorded messages", r, &mut failures);
        }
    }
    for f in &failures {
        println!("FAIL {f}");
    }
    println!("SPEL v2 check: {}", if failures.is_empty() { "passed".to_string() } else { format!("{} failure(s)", failures.len()) });
    if failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
