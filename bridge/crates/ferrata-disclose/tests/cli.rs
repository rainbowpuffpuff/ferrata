mod common;

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use common::*;
use ferrata_bridge_core::inbound::DestKind;
use ferrata_disclose::{ClaimSecret, Disclosure};
use serde_json::{json, Value};

static NEXT_SCRATCH: AtomicUsize = AtomicUsize::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR"));
        let sequence = NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed);
        let path = root.join(format!("disclose-cli-{label}-{}-{sequence}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn write_json(path: &Path, value: &impl serde::Serialize) {
    let mut bytes = serde_json::to_vec_pretty(value).unwrap();
    bytes.push(b'\n');
    fs::write(path, bytes).unwrap();
}

/// The wallet's own destination candidates (`make --destinations`), as the test world's `dest_of` derives them.
fn destinations_json() -> Value {
    let mut destinations = Vec::new();
    for kind in [DestKind::Private, DestKind::Public] {
        for account in 0..GAP {
            destinations
                .push(json!({ "kind": kind as u8, "account": account, "dest": hex::encode(dest_of(kind, account)) }));
        }
    }
    json!({ "destinations": destinations })
}

/// Public chain data only (`VIEW.json`).
fn view_json(world: &World) -> Value {
    let mut spent_nullifiers: Vec<_> = world.view.spent_nullifiers.iter().map(hex::encode).collect();
    spent_nullifiers.sort();
    json!({
        "chainId": hex::encode(world.view.chain_id),
        "vault": hex::encode(world.view.vault),
        "configId": hex::encode(world.view.config_id),
        "deposits": world.view.deposits.iter().map(|d| json!({
            "leafIndex": d.leaf_index,
            "j": d.j,
            "depositor": hex::encode(d.depositor),
            "token": hex::encode(d.token),
            "amount": hex::encode(d.amount),
            "commitment": hex::encode(d.commitment),
        })).collect::<Vec<_>>(),
        "spentNullifiers": spent_nullifiers,
        "burns": world.view.burns.iter().map(|b| json!({
            "burnIndex": b.burn_index,
            "r": hex::encode(b.note.r),
            "memo": hex::encode(b.note.memo),
            "mac": hex::encode(b.note.mac),
            "token": hex::encode(b.note.token),
            "amount": hex::encode(b.note.amount),
            "cOut": hex::encode(b.note.c_out),
        })).collect::<Vec<_>>(),
    })
}

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ferrata-disclose"))
}

/// `make` with the seed in a file beside the view (never an argument) and the wallet's destinations.
fn make(seed: &[u8; 32], view: &Path, out: &Path) -> Output {
    let dir = view.parent().unwrap();
    let (seed_file, destinations) = (dir.join("seed.hex"), dir.join("destinations.json"));
    fs::write(&seed_file, format!("{}\n", hex::encode(seed))).unwrap();
    write_json(&destinations, &destinations_json());
    cli()
        .arg("make")
        .arg("--bridge-seed-file")
        .arg(&seed_file)
        .arg("--destinations")
        .arg(&destinations)
        .arg("--view")
        .arg(view)
        .arg("--out")
        .arg(out)
        .output()
        .unwrap()
}

fn verify(disclosure: &Path, view: &Path) -> Output {
    cli().arg("verify").arg("--disclosure").arg(disclosure).arg("--view").arg(view).output().unwrap()
}

fn stdout_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("stdout is not JSON: {e}\n{}", String::from_utf8_lossy(&output.stdout)))
}

#[test]
fn round_trip_lists_alices_activity_and_nothing_of_bobs() {
    let world = world();
    let scratch = Scratch::new("round-trip");
    let view = scratch.join("view.json");
    let disclosure = scratch.join("disclosure.json");
    write_json(&view, &view_json(&world));

    let output = make(&world.alice_seed, &view, &disclosure);
    assert!(output.status.success(), "make: {}", String::from_utf8_lossy(&output.stderr));
    let document: Disclosure = serde_json::from_slice(&fs::read(&disclosure).unwrap()).unwrap();
    assert_eq!(document, alice_disclosure(&world));

    let output = verify(&disclosure, &view);
    assert_eq!(output.status.code(), Some(0), "verify: {}", String::from_utf8_lossy(&output.stderr));
    let report = stdout_json(&output);
    let deposits = report["deposits"].as_array().unwrap();
    assert_eq!(deposits.len(), world.alice.len());
    for (reported, fact) in deposits.iter().zip(&world.alice) {
        assert_eq!(reported["leafIndex"], fact.leaf_index);
        assert_eq!(reported["verified"], fact.claimed);
        assert_eq!(reported["dest"], hex::encode(dest_of(fact.kind, fact.account)));
        assert_eq!(
            reported["nullifier"],
            fact.claimed.then_some(fact.nullifier).map(hex::encode).map_or(Value::Null, Value::from)
        );
    }
    let burns = report["burns"].as_array().unwrap();
    assert_eq!(burns.len(), world.alice_burns.len());
    for (reported, fact) in burns.iter().zip(&world.alice_burns) {
        assert_eq!(reported["i"], fact.i);
        assert_eq!(reported["burnIndex"], fact.burn_index);
        assert_eq!(reported["recipient"], hex::encode(fact.recipient));
    }
    let bob_leaves: HashSet<u64> = world.bob.iter().map(|d| d.leaf_index).collect();
    assert!(deposits.iter().all(|d| !bob_leaves.contains(&d["leafIndex"].as_u64().unwrap())));
    assert!(burns
        .iter()
        .all(|b| !world.bob_burns.iter().any(|bob| bob.burn_index == b["burnIndex"].as_u64().unwrap())));
}

fn mutated(base: &Disclosure, change: impl FnOnce(&mut Disclosure)) -> Disclosure {
    let mut disclosure = base.clone();
    change(&mut disclosure);
    disclosure
}

fn assert_refusal(
    disclosure: &Disclosure,
    view: &Path,
    scratch: &Scratch,
    kind: &str,
    leaf: Option<u64>,
    field: Option<&str>,
) {
    static NEXT_DOCUMENT: AtomicUsize = AtomicUsize::new(0);
    let path = scratch.join(&format!("tampered-{}.json", NEXT_DOCUMENT.fetch_add(1, Ordering::Relaxed)));
    write_json(&path, disclosure);
    let output = verify(&path, view);
    assert_eq!(output.status.code(), Some(1), "{disclosure:?}: {}", String::from_utf8_lossy(&output.stderr));
    let error = stdout_json(&output)["error"].clone();
    assert_eq!(error["kind"], kind);
    assert_eq!(error["leafIndex"], leaf.map_or(Value::Null, Value::from));
    assert_eq!(error["field"], field.map_or(Value::Null, Value::from));
    assert!(error["message"].as_str().unwrap().len() > 10);
}

#[test]
fn every_library_refusal_is_exit_one_with_its_named_error() {
    let world = world();
    let scratch = Scratch::new("tampering");
    let view = scratch.join("view.json");
    write_json(&view, &view_json(&world));
    let base = alice_disclosure(&world);
    let mut cases = vec![
        (mutated(&base, |d| d.deposits[0].dest[9] ^= 1), "Mismatch", Some(1), Some("dest")),
        (mutated(&base, |d| d.deposits[0].kind = DestKind::Public as u8), "Mismatch", Some(1), Some("kind")),
        (mutated(&base, |d| d.deposits[0].kind = 7), "Mismatch", Some(1), Some("kind")),
        (
            mutated(&base, |d| d.deposits[0].leaf_index = world.bob[0].leaf_index),
            "Mismatch",
            Some(2),
            Some("depositor"),
        ),
        (mutated(&base, |d| d.deposits[0].leaf_index = 99), "UnknownLeaf", Some(99), None),
        (
            mutated(&base, |d| d.deposits[0].claim.as_mut().unwrap().nullifier = h("unspent")),
            "Mismatch",
            Some(1),
            Some("nullifier"),
        ),
        (
            mutated(&base, |d| d.deposits[0].claim.as_mut().unwrap().s = world.alice[1].s),
            "Mismatch",
            Some(1),
            Some("s"),
        ),
        (mutated(&base, |d| d.deposits[0].depositor[0] ^= 1), "Mismatch", Some(1), Some("depositor")),
        (mutated(&base, |d| d.deposits[0].token[0] ^= 1), "Mismatch", Some(1), Some("token")),
        (mutated(&base, |d| d.deposits[0].amount[31] ^= 1), "Mismatch", Some(1), Some("amount")),
        (mutated(&base, |d| d.deposits[0].commitment[0] ^= 1), "Mismatch", Some(1), Some("commitment")),
        (mutated(&base, |d| d.chain_id[31] ^= 1), "ContextMismatch", None, Some("chainId")),
        (mutated(&base, |d| d.vault[0] ^= 1), "ContextMismatch", None, Some("vault")),
    ];
    let forged = world.alice[2].nullifier;
    let secret = world.alice[2].s;
    cases.push((
        mutated(&base, |d| d.deposits[2].claim = Some(ClaimSecret { s: secret, nullifier: forged })),
        "UnspentNullifier",
        Some(7),
        None,
    ));
    cases.push((mutated(&base, |d| d.deposits.push(d.deposits[0].clone())), "Mismatch", Some(1), Some("leafIndex")));
    for (document, kind, leaf, field) in cases {
        assert_refusal(&document, &view, &scratch, kind, leaf, field);
    }
}

#[test]
fn another_vaults_view_is_refused() {
    let mut other = world();
    let scratch = Scratch::new("other-vault");
    let view = scratch.join("view.json");
    let disclosure = scratch.join("disclosure.json");
    let document = alice_disclosure(&other);
    other.view.vault[0] ^= 1;
    write_json(&view, &view_json(&other));
    write_json(&disclosure, &document);
    let output = verify(&disclosure, &view);
    assert_eq!(output.status.code(), Some(1));
    let error = stdout_json(&output);
    assert_eq!(error["error"]["kind"], "ContextMismatch");
    assert_eq!(error["error"]["field"], "vault");
}

#[test]
fn bad_arguments_exit_two() {
    let scratch = Scratch::new("arguments");
    let view = scratch.join("view.json");
    write_json(&view, &view_json(&world()));
    let commands: Vec<Vec<&str>> = vec![
        vec![],
        vec!["unknown"],
        vec!["make"],
        vec!["verify", "--view"],
        vec!["verify", "--what", "x", "--view", "y"],
        vec!["make", "--bridge-seed-hex", "00", "--view", "unused", "--out", "unused"],
        vec![
            "make",
            "--bridge-seed-file",
            "missing",
            "--destinations",
            "missing",
            "--view",
            "unused",
            "--out",
            "unused",
        ],
        vec!["verify", "--view", "a", "--view", "b"],
    ];
    for arguments in commands {
        let output = cli().args(arguments).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{output:?}");
    }
    // The public view carries public chain data only: a view with the wallet's destinations in it, or any other
    // unknown field, is refused; a seed file that is not 64 hex digits is refused (exit 2, nothing written).
    let world = world();
    let mut leaky = view_json(&world);
    leaky["destinations"] = destinations_json()["destinations"].clone();
    let leaky_view = scratch.join("leaky-view.json");
    write_json(&leaky_view, &leaky);
    let output = verify(&scratch.join("unused.json"), &leaky_view);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let out = scratch.join("never-written.json");
    assert_eq!(make(&world.alice_seed, &leaky_view, &out).status.code(), Some(2));
    let short_seed = scratch.join("short-seed.hex");
    fs::write(&short_seed, "00ff").unwrap();
    let output = cli()
        .args(["make", "--bridge-seed-file"])
        .arg(&short_seed)
        .arg("--destinations")
        .arg(scratch.join("destinations.json"))
        .arg("--view")
        .arg(&view)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(!out.exists(), "nothing written on a refused make");
}

fn replay_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/privacy/disclosure-replay/crown-rc3-rehearsal-1")
}

#[test]
fn recorded_run_replay_verifies_and_one_claim_byte_tamper_fails() {
    let view = replay_dir().join("VIEW.json");
    let disclosure = replay_dir().join("DISCLOSURE.json");
    let output = verify(&disclosure, &view);
    assert_eq!(output.status.code(), Some(0), "replay: {}", String::from_utf8_lossy(&output.stderr));
    let report = stdout_json(&output);
    assert_eq!(report["deposits"].as_array().unwrap().len(), 2);
    assert_eq!(report["burns"].as_array().unwrap().len(), 2);
    assert!(report["deposits"].as_array().unwrap().iter().all(|d| d["verified"] == true));
    assert_eq!(report["deposits"][0]["nullifier"], "0799ff698b9537d31f511eea9c15da7c8055b38b36ea7b225c39701f0d872d7f");
    assert_eq!(report["deposits"][1]["nullifier"], "717e58e631b37439f4b368a7d216804b74948108a05cb5931659246f92080844");

    // Change exactly one byte in the first disclosed claim secret's hexadecimal text.
    let original = fs::read(&disclosure).unwrap();
    let marker = b"\"s\": \"";
    let start = original.windows(marker.len()).position(|w| w == marker).unwrap() + marker.len();
    let mut tampered = original.clone();
    tampered[start] = if tampered[start] == b'0' { b'1' } else { b'0' };
    let scratch = Scratch::new("replay-tamper");
    let tampered_path = scratch.join("disclosure.json");
    fs::write(&tampered_path, tampered).unwrap();
    let output = verify(&tampered_path, &view);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stdout_json(&output)["error"]["kind"], "Mismatch");
}
