//! Review L3, L6 and L7: what the attestor writes to disk.
//!
//! - L3: `keygen` writes a key readable by its owner only (mode 0600) and never overwrites a file, nor writes
//!   through a link planted at its path (one `create_new` open).
//! - L6: `write_atomic` (the state file, the published attestation) writes a temporary file of its own beside
//!   the target, never one a link planted beside it points to, and two writers never share one.
//! - L7: the CLI records the signed tip in its state before it writes the attestation: when the attestation
//!   cannot be written, the state already holds the tip, and the next round signs the same tip again.
mod support;

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::process::Command;

use attestor_core::{sign_certificate, Deployment};
use checkpoint_core::hash::sha256_parts;
use ferrata_attestor::{deployment_json, read_state, save_lez_answers, write_atomic, write_secret};
use serde_json::{json, Value};
use support::*;

const BIN: &str = env!("CARGO_BIN_EXE_ferrata_attestor");

fn scratch(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("files-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn mode(p: &Path) -> u32 {
    fs::symlink_metadata(p).unwrap().permissions().mode() & 0o777
}

/// The names in `dir` that look like temporary files of [`write_atomic`].
fn temporaries(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with('.') && n.ends_with(".tmp"))
        .collect()
}

#[test]
fn l3_keygen_writes_an_owner_only_key_and_never_overwrites() {
    let d = scratch("keygen");
    let key = d.join("attestation.key");
    let out = Command::new(BIN).args(["keygen", "--out"]).arg(&key).args(["--seed-label", "l3"]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let secret = sha256_parts(&[b"ferrata/attestor/devnet-key/", b"l3"]);
    assert_eq!(fs::read(&key).unwrap(), secret);
    assert_eq!(mode(&key), 0o600);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        hex::encode(pk(&ed25519_dalek::SigningKey::from_bytes(&secret)))
    );
    // A second keygen to the same path is refused, and the key is unchanged.
    let again = Command::new(BIN).args(["keygen", "--out"]).arg(&key).output().unwrap();
    assert!(!again.status.success());
    assert!(String::from_utf8_lossy(&again.stderr).contains("a key is never overwritten"));
    assert_eq!(fs::read(&key).unwrap(), secret);
    // A link planted at the path (dangling or not) is not written through.
    let target = d.join("elsewhere");
    let link = d.join("planted.key");
    symlink(&target, &link).unwrap();
    assert!(write_secret(&link, &[7; 32]).is_err());
    assert!(!target.exists());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn l6_write_atomic_never_writes_through_a_planted_link() {
    let d = scratch("atomic-link");
    let (state, victim) = (d.join("state.json"), d.join("victim"));
    fs::write(&victim, b"victim").unwrap();
    // The old code's fixed temporary name, planted as a link to another file.
    symlink(&victim, d.join("state.json.tmp")).unwrap();
    write_atomic(&state, b"new").unwrap();
    assert_eq!(fs::read(&state).unwrap(), b"new");
    assert!(!fs::symlink_metadata(&state).unwrap().file_type().is_symlink());
    assert_eq!(fs::read(&victim).unwrap(), b"victim");
    // A link at the target itself is replaced, not written through.
    let (att, victim2) = (d.join("att.json"), d.join("victim2"));
    fs::write(&victim2, b"victim2").unwrap();
    symlink(&victim2, &att).unwrap();
    write_atomic(&att, b"published").unwrap();
    assert_eq!(fs::read(&att).unwrap(), b"published");
    assert_eq!(fs::read(&victim2).unwrap(), b"victim2");
    assert!(temporaries(&d).is_empty(), "{:?}", temporaries(&d));
    // A path whose directory does not exist fails and leaves nothing.
    assert!(write_atomic(&d.join("absent/x.json"), b"x").is_err());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn l6_two_writers_never_share_a_temporary_file() {
    let d = scratch("atomic-race");
    let p = d.join("latest.json");
    let contents: Vec<Vec<u8>> = (0..8u8).map(|w| vec![b'a' + w; 64 * 1024]).collect();
    std::thread::scope(|s| {
        for c in &contents {
            let p = &p;
            s.spawn(move || {
                for _ in 0..40 {
                    write_atomic(p, c).unwrap();
                }
            });
        }
    });
    let last = fs::read(&p).unwrap();
    assert!(contents.contains(&last), "the file holds one writer's bytes whole");
    assert!(temporaries(&d).is_empty(), "{:?}", temporaries(&d));
    let _ = fs::remove_dir_all(&d);
}

/// The lab node's answers, with the channel accrediting the stand-in key set n3, saved as a directory the CLI
/// reads (`--answers`), with a LEZ node's saved answers holding 186..=202 final (`lez/`).
fn answers_dir(d: &Path) -> PathBuf {
    let src = dir().join("vectors/lab-node");
    let a = d.join("answers");
    fs::create_dir_all(a.join("blocks")).unwrap();
    for e in fs::read_dir(&src).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_file() {
            fs::copy(e.path(), a.join(e.file_name())).unwrap();
        }
    }
    for e in fs::read_dir(src.join("blocks")).unwrap() {
        let e = e.unwrap();
        fs::copy(e.path(), a.join("blocks").join(e.file_name())).unwrap();
    }
    let mut channel: Value = serde_json::from_slice(&fs::read(a.join("channel.json")).unwrap()).unwrap();
    channel["accredited_keys"] = json!(n3_keys().iter().map(hex::encode).collect::<Vec<_>>());
    fs::write(a.join("channel.json"), channel.to_string()).unwrap();
    let lab = lab_answers();
    let lez: BTreeMap<u64, Option<Vec<u8>>> =
        (186..=202).map(|id| (id, Some(with_status(&payload_of(&lab, id), FINALIZED)))).collect();
    save_lez_answers(&a, &lez).unwrap();
    a
}

#[test]
fn l7_the_state_holds_the_tip_before_the_attestation_is_written() {
    let d = scratch("order");
    let answers = answers_dir(&d);
    let dep = Deployment { evm_chain_id: [0; 32], vault: [1; 20], lez_config_id: [2; 32], channel_id: channel() };
    let (ch, att) = (key("channel-1"), key("attestation-1"));
    fs::write(d.join("deployment.json"), deployment_json(&dep).to_string()).unwrap();
    let cert = json!({"channel_key": hex::encode(pk(&ch)), "certificate": hex::encode(sign_certificate(&ch, &dep, &pk(&att)))});
    fs::write(d.join("cert.json"), cert.to_string()).unwrap();
    write_secret(&d.join("att.key"), &att.to_bytes()).unwrap();
    let state = d.join("state.json");
    let attest = |out: &Path| {
        Command::new(BIN)
            .arg("attest")
            .arg("--deployment")
            .arg(d.join("deployment.json"))
            .arg("--cert")
            .arg(d.join("cert.json"))
            .arg("--att-key")
            .arg(d.join("att.key"))
            .arg("--answers")
            .arg(&answers)
            .arg("--state")
            .arg(&state)
            .arg("--out")
            .arg(out)
            .output()
            .unwrap()
    };
    // The attestation cannot be written (its directory does not exist): the round fails after the state
    // recorded the tip.
    let failed = attest(&d.join("no-such-dir/att.json"));
    assert!(!failed.status.success());
    let st = read_state(&state, &channel()).unwrap().expect("the state was written before the attestation");
    assert_eq!(st.last.tip_id, 202);
    // The next round signs the same tip again (nothing above it): the signature lost is the one re-made.
    let ok = attest(&d.join("att.json"));
    assert!(ok.status.success(), "{}", String::from_utf8_lossy(&ok.stderr));
    let published: Value = serde_json::from_slice(&fs::read(d.join("att.json")).unwrap()).unwrap();
    assert_eq!(
        (published["tip_id"].as_u64(), published["tip_hash"].as_str()),
        (Some(202), Some(hex::encode(st.last.tip_hash).as_str()))
    );
    assert_eq!(read_state(&state, &channel()).unwrap(), Some(st));
    let _ = fs::remove_dir_all(&d);
}
