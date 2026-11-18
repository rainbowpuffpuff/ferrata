//! Shared by the red-team tests (`tests/redteam_*.rs`): the blind vectors of
//! `vectors/redteam-2026-11-18/vectors.json` (written from the specification alone before any
//! implementation was read; `docs/reviews/redteam-2026-11-18.md`), the judge that compares what a
//! run showed with a vector's expectation, and the results file each test writes.
//!
//! A test runs each of its vectors in isolation (a panic of the test's own code is caught and
//! recorded as a harness error, never as a verdict), and records one [`Observed`] per vector. The
//! judge never reads the implementation's opinion: the expectation is the vector's, by the code
//! names it lists, resolved to numbers through the crates' own code tables.
#![allow(dead_code)]

use std::{
    collections::BTreeMap,
    fs,
    panic::{catch_unwind, AssertUnwindSafe},
    path::PathBuf,
};

use harness::TxReport;
use serde_json::{json, Value};

/// The vectors, compiled in: the expectations a test is judged against are the committed ones.
pub const VECTORS: &str = include_str!("../../../../vectors/redteam-2026-11-18/vectors.json");

/// One vector, as the judge needs it.
#[derive(Clone, Debug)]
pub struct Vector {
    pub id: String,
    pub area: String,
    pub target: String,
    pub kind: String,
    pub title: String,
    /// `accepted` or `refused`.
    pub outcome: String,
    /// Every code name the specification allows (`core:…`, `step:…`, `prog:…`, `cons:…`,
    /// `bridge:…`, `lez:validity-window`, `lez:any`, `evm:revert`, `any`).
    pub codes: Vec<String>,
}

/// Every vector, by id.
pub fn vectors() -> BTreeMap<String, Vector> {
    let doc: Value = serde_json::from_str(VECTORS).expect("vectors.json");
    doc["vectors"]
        .as_array()
        .expect("vectors")
        .iter()
        .map(|v| {
            let codes = v["expect"]["codes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c["name"].as_str().unwrap().to_string())
                .collect();
            let vector = Vector {
                id: v["id"].as_str().unwrap().to_string(),
                area: v["area"].as_str().unwrap().to_string(),
                target: v["target"].as_str().unwrap().to_string(),
                kind: v["kind"].as_str().unwrap().to_string(),
                title: v["title"].as_str().unwrap().to_string(),
                outcome: v["expect"]["outcome"].as_str().unwrap().to_string(),
                codes,
            };
            (vector.id.clone(), vector)
        })
        .collect()
}

/// The number of a code name, from the crates' own tables (`None` for the non-numeric names).
pub fn code_number(name: &str) -> Option<u32> {
    let (ns, n) = name.split_once(':').unwrap_or(("", name));
    match ns {
        "core" => harness::CORE_ERRORS.iter().find(|e| format!("{e:?}") == n).map(|e| u32::from(e.code())),
        "step" => match n {
            "Witness" => Some(80),
            "Phase" => Some(81),
            "StoreChanged" => Some(82),
            "AdoptionIncomplete" => Some(83),
            "NoAdoption" => Some(84),
            _ => None,
        },
        "prog" => ferrata_lez_wire::code::ALL.iter().find(|(k, _)| *k == n).map(|(_, v)| u32::from(*v)),
        "cons" => ferrata_lez_wire::consumer::code::ALL.iter().find(|(k, _)| *k == n).map(|(_, v)| u32::from(*v)),
        "bridge" => ferrata_lez_bridge_wire::code::ALL.iter().find(|(k, _)| *k == n).map(|(_, v)| u32::from(*v)),
        _ => None,
    }
}

/// The name of a numeric code, for the results (the same tables, reversed).
pub fn code_name(code: u32) -> String {
    if let Some(e) = harness::CORE_ERRORS.iter().find(|e| u32::from(e.code()) == code) {
        return format!("core:{e:?}");
    }
    let step = [(80, "Witness"), (81, "Phase"), (82, "StoreChanged"), (83, "AdoptionIncomplete"), (84, "NoAdoption")];
    if let Some((_, n)) = step.iter().find(|(c, _)| *c == code) {
        return format!("step:{n}");
    }
    for (ns, table) in [
        ("prog", ferrata_lez_wire::code::ALL),
        ("cons", ferrata_lez_wire::consumer::code::ALL),
        ("bridge", ferrata_lez_bridge_wire::code::ALL),
    ] {
        if let Some((n, _)) = table.iter().find(|(_, v)| u32::from(*v) == code) {
            return format!("{ns}:{n}");
        }
    }
    format!("unknown:{code}")
}

/// What running a vector showed.
#[derive(Clone, Debug)]
pub enum Observed {
    /// The decisive transaction (or native call) succeeded; `note` says what the test checked of
    /// the vector's effects.
    Accepted { note: String },
    /// Refused with an exit code (a program's, or the code a native call returned).
    Code { code: u32, note: String },
    /// Refused by LEZ itself, with no exit code: the revert string (a validity window, an
    /// unadmitted actor, a guest that aborted, a settlement rejection).
    Lez { revert: String, note: String },
    /// A native call refused with an error that has no code (`any` vectors).
    Refused { what: String, note: String },
    /// Accepted, but an effect the vector states (its `effects:` note) did not hold: what was
    /// expected and what was found.
    EffectMismatch { note: String },
    /// The vector cannot be built as written with the fixtures at hand.
    Unsupported { reason: String },
    /// The test's own code failed (a harness error: fix the test, not a verdict).
    HarnessError { message: String },
}

impl Observed {
    /// The observation of the last report of a run: accepted if every report applied.
    pub fn of_reports(reports: &[TxReport], note: &str) -> Self {
        match reports.iter().find(|r| !r.ok()) {
            None => Observed::Accepted { note: note.to_string() },
            Some(r) => Self::of_report(r, note),
        }
    }

    /// The observation of one report.
    pub fn of_report(r: &TxReport, note: &str) -> Self {
        match (r.exit_code, &r.charge.revert) {
            (Some(code), _) => Observed::Code { code, note: format!("{} [{}]", note, r.label) },
            (None, Some(revert)) => Observed::Lez { revert: revert.clone(), note: format!("{} [{}]", note, r.label) },
            (None, None) => Observed::Accepted { note: format!("{} [{}]", note, r.label) },
        }
    }

    fn describe(&self) -> Value {
        match self {
            Observed::Accepted { note } => json!({"outcome": "accepted", "note": note}),
            Observed::Code { code, note } => {
                json!({"outcome": "refused", "code": code, "code_name": code_name(*code), "note": note})
            }
            Observed::Lez { revert, note } => json!({"outcome": "refused", "lez_revert": revert, "note": note}),
            Observed::Refused { what, note } => json!({"outcome": "refused", "error": what, "note": note}),
            Observed::EffectMismatch { note } => json!({"outcome": "accepted", "effect_mismatch": note}),
            Observed::Unsupported { reason } => json!({"outcome": "unsupported", "reason": reason}),
            Observed::HarnessError { message } => json!({"outcome": "harness-error", "message": message}),
        }
    }
}

/// The verdict on one vector (`docs/reviews/redteam-2026-11-18.md`, "Protocol").
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verdict {
    Pass,
    Mismatch,
    Panic,
    Unsupported,
    HarnessError,
}

impl Verdict {
    pub fn name(self) -> &'static str {
        match self {
            Verdict::Pass => "PASS",
            Verdict::Mismatch => "MISMATCH",
            Verdict::Panic => "PANIC",
            Verdict::Unsupported => "UNSUPPORTED",
            Verdict::HarnessError => "HARNESS-ERROR",
        }
    }
}

/// Whether a LEZ revert string is LEZ's timestamp validity window.
fn is_validity_window(revert: &str) -> bool {
    revert.contains("validity window")
}

/// Judge `observed` against `v`.
pub fn judge(v: &Vector, observed: &Observed) -> Verdict {
    let allows = |name: &str| v.codes.iter().any(|c| c == name);
    match observed {
        Observed::Unsupported { .. } => Verdict::Unsupported,
        Observed::HarnessError { .. } => Verdict::HarnessError,
        Observed::EffectMismatch { .. } => Verdict::Mismatch,
        Observed::Accepted { .. } => {
            if v.outcome == "accepted" {
                Verdict::Pass
            } else {
                Verdict::Mismatch
            }
        }
        _ if v.outcome == "accepted" => match observed {
            Observed::Lez { revert, .. } if !is_validity_window(revert) => Verdict::Panic,
            _ => Verdict::Mismatch,
        },
        Observed::Code { code, .. } => {
            if allows("any") || v.codes.iter().any(|c| code_number(c) == Some(*code)) {
                Verdict::Pass
            } else {
                Verdict::Mismatch
            }
        }
        Observed::Lez { revert, .. } => {
            if allows("any") || allows("lez:any") || (allows("lez:validity-window") && is_validity_window(revert)) {
                Verdict::Pass
            } else if is_validity_window(revert) {
                Verdict::Mismatch
            } else {
                // Refused without a code where the specification names one: the program
                // aborted, or LEZ refused something the test sent (reviewed by hand).
                Verdict::Panic
            }
        }
        Observed::Refused { .. } => {
            if allows("any") || allows("evm:revert") {
                Verdict::Pass
            } else {
                Verdict::Mismatch
            }
        }
    }
}

/// One test's results: every vector it ran, judged.
pub struct Results {
    /// The test file's stem (`redteam_lc`) and the part of it this run covers (`sig`).
    pub file: &'static str,
    pub part: &'static str,
    all: BTreeMap<String, Vector>,
    pub rows: Vec<(Vector, Observed, Verdict)>,
}

impl Results {
    pub fn new(file: &'static str, part: &'static str) -> Self {
        Self { file, part, all: vectors(), rows: Vec::new() }
    }

    pub fn vector(&self, id: &str) -> Vector {
        self.all.get(id).unwrap_or_else(|| panic!("no vector {id}")).clone()
    }

    /// Run `f` for vector `id` and record what it observed; a panic of `f` is recorded as a
    /// harness error (or, if LEZ's settlement rejected a transaction, as LEZ's refusal).
    pub fn run(&mut self, id: &str, f: impl FnOnce() -> Observed) {
        let v = self.vector(id);
        let observed = match catch_unwind(AssertUnwindSafe(f)) {
            Ok(o) => o,
            Err(e) => {
                let message = e
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| "panic".to_string());
                if message.contains("settlement rejected the transaction") {
                    Observed::Lez { revert: message, note: "settlement rejected".into() }
                } else {
                    Observed::HarnessError { message }
                }
            }
        };
        let verdict = judge(&v, &observed);
        println!("{:<12} {:<13} {}", v.id, verdict.name(), serde_json::to_string(&observed.describe()).unwrap());
        self.rows.push((v, observed, verdict));
    }

    /// Record an unsupported vector.
    pub fn unsupported(&mut self, id: &str, reason: &str) {
        self.run(id, || Observed::Unsupported { reason: reason.to_string() });
    }

    /// Write `results/redteam-2026-11-18/<file>.<part>.json` and print the counts. Fails if a vector of
    /// `areas` (restricted to ids with one of `prefixes`, if given) was not recorded exactly once.
    pub fn finish(self, areas: &[&str], prefixes: &[&str]) -> Vec<(String, Verdict)> {
        let mut seen = BTreeMap::new();
        for (v, _, _) in &self.rows {
            *seen.entry(v.id.clone()).or_insert(0) += 1;
        }
        let expected: Vec<&Vector> = self
            .all
            .values()
            .filter(|v| areas.contains(&v.area.as_str()))
            .filter(|v| prefixes.is_empty() || prefixes.iter().any(|p| v.id.starts_with(p)))
            .collect();
        let missing: Vec<&str> = expected.iter().filter(|v| !seen.contains_key(&v.id)).map(|v| v.id.as_str()).collect();
        let twice: Vec<&String> = seen.iter().filter(|(_, n)| **n > 1).map(|(id, _)| id).collect();
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for (_, _, verdict) in &self.rows {
            *counts.entry(verdict.name()).or_insert(0) += 1;
        }
        let rows: Vec<Value> = self
            .rows
            .iter()
            .map(|(v, o, verdict)| {
                json!({
                    "id": v.id, "area": v.area, "kind": v.kind, "title": v.title,
                    "expected": {"outcome": v.outcome, "codes": v.codes},
                    "observed": o.describe(), "verdict": verdict.name(),
                })
            })
            .collect();
        let dir = harness::results_dir().join("redteam-2026-11-18");
        fs::create_dir_all(&dir).unwrap();
        let path: PathBuf = dir.join(format!("{}.{}.json", self.file, self.part));
        let doc = json!({
            "test": format!("lez/harness/tests/{}.rs", self.file), "part": self.part,
            "lez": harness::LEZ_TAG,
            "dev_mode": std::env::var("RISC0_DEV_MODE").unwrap_or_default(),
            "counts": counts, "results": rows,
        });
        fs::write(&path, serde_json::to_string_pretty(&doc).unwrap() + "\n").unwrap();
        println!("{}.{}: {:?} -> {}", self.file, self.part, counts, path.display());
        assert!(missing.is_empty(), "vectors not run: {missing:?}");
        assert!(twice.is_empty(), "vectors run twice: {twice:?}");
        self.rows.iter().map(|(v, _, verdict)| (v.id.clone(), *verdict)).collect()
    }
}
