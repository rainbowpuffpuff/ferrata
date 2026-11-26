//! RFP-022 F.7: `attestor-refusals --real`, the refusal cases built over a recorded run's real attestations
//! (`checkpoint/attestor/zone-bridge.sh input` runs it on the zone's `final.json` files), run as the CLI on the lab's
//! real blocks re-keyed to {lab, k1, k2} (n = 3, t = 2), with attestations made by the devnet's seeded keys standing in
//! for the run's: every case written is refused by the checkpoint program's rule with the error its meta names, the same
//! attestations of the same tip alone reach t and are accepted, and the inputs the tool must not build are refused by it.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use attestor_core::{sign_certificate, Deployment, FinalTip, SignedAttestation};
use checkpoint_core::hash::sha256_parts;
use checkpoint_core::input::CheckpointInputB;
use checkpoint_core::rule_b::run_checkpoint_b;
use checkpoint_core::{Error, Hash};
use ed25519_dalek::SigningKey;
use ferrata_attestor::{
    attestation_json, bedrock_tier, collect, deployment_of, devnet_lez_standin, final_inscriptions, final_tier, hexa,
    rekey_base, upgrade_pre_producer_input, Attestor, FinalTier, NodeAnswers,
};
use serde_json::json;

/// The tool, built beside this test (cargo builds a package's binaries for its integration tests).
fn bin() -> PathBuf {
    if let Some(p) = option_env!("CARGO_BIN_EXE_attestor-refusals") {
        return PathBuf::from(p);
    }
    let exe = std::env::current_exe().unwrap();
    exe.parent().and_then(Path::parent).unwrap().join("attestor-refusals")
}
const LAB_KEY: &str = "3b14e3980d05dd3651ff5956758dd4cfecc33b837775c9e3d90279d34d3fb0a2";
const CHANNEL: &str = "0101010101010101010101010101010101010101010101010101010101010101";

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn h(s: &str) -> Hash {
    hexa(&json!(s), "hex").unwrap()
}

fn key(label: &str) -> SigningKey {
    SigningKey::from_bytes(&sha256_parts(&[b"ferrata/attestor/devnet-key/", label.as_bytes()]))
}

fn pk(k: &SigningKey) -> Hash {
    k.verifying_key().to_bytes()
}

fn lab_answers() -> NodeAnswers {
    NodeAnswers::load(&dir().join("vectors/lab-node")).unwrap()
}

/// The lab chain's real span, re-keyed to {lab, k1, k2} with nMin 3, tMin 2 (as tests/attestor.rs's base_n3).
fn base_n3() -> Vec<u8> {
    let old = fs::read(dir().join("../realblocks/out/real-lab-186-202.input")).unwrap();
    let ins = final_inscriptions(&lab_answers(), &h(CHANNEL)).unwrap();
    let real = upgrade_pre_producer_input(&old, &ins[&202].elided().unwrap()).unwrap();
    rekey_base(&real, &[h(LAB_KEY), pk(&key("channel-1")), pk(&key("channel-2"))], 3, 2).unwrap()
}

/// The node's final tier with the stand-in key set, read against an honest LEZ node.
fn tier_n3() -> FinalTier {
    let mut answers = lab_answers();
    answers.channel["accredited_keys"] =
        json!([LAB_KEY, hex::encode(pk(&key("channel-1"))), hex::encode(pk(&key("channel-2")))]);
    let mut lez = devnet_lez_standin(&lab_answers(), &h(CHANNEL)).unwrap();
    final_tier(&answers, &mut lez, &h(CHANNEL), None).unwrap()
}

fn attestor(i: u32, d: &Deployment) -> Attestor {
    let channel = key(&format!("channel-{i}"));
    let att = key(&format!("attestation-{i}"));
    Attestor {
        deployment: *d,
        channel_key: pk(&channel),
        certificate: sign_certificate(&channel, d, &pk(&att)),
        att_key: att,
    }
}

fn scratch(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("real-refusals-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn write_att(path: &Path, d: &Deployment, a: &SignedAttestation, tier: &FinalTier) {
    fs::write(path, serde_json::to_vec_pretty(&attestation_json(d, a, tier)).unwrap()).unwrap();
}

fn run(bytes: &[u8]) -> Result<checkpoint_core::rule_b::OutcomeB, Error> {
    run_checkpoint_b(&CheckpointInputB::decode(bytes).unwrap())
}

/// The run's base input, two real attestations of its final tip and a third attestor's earlier round (block 190).
struct Setup {
    d: PathBuf,
    base: Vec<u8>,
    a1: SignedAttestation,
}

fn setup(name: &str) -> Setup {
    let d = scratch(name);
    let base = base_n3();
    let dep = deployment_of(&CheckpointInputB::decode(&base).unwrap().config);
    let tier = tier_n3();
    let a1 = attestor(1, &dep).attest(&tier, None).unwrap();
    let a2 = attestor(2, &dep).attest(&tier, None).unwrap();
    // attestor 2's signature of an older tip of the same chain: what an earlier round of it published
    let at2 = attestor(2, &dep);
    let older = FinalTip {
        tip_id: 190,
        tip_hash: bedrock_tier(&lab_answers(), &h(CHANNEL)).unwrap().hashes[&190],
        final_msg_id: [0x90; 32],
    };
    let stale2 =
        SignedAttestation::new(&at2.att_key, at2.channel_key, at2.certificate, &dep, &tier.key_set_hash, older);
    let stale1 = SignedAttestation::new(
        &attestor(1, &dep).att_key,
        pk(&key("channel-1")),
        attestor(1, &dep).certificate,
        &dep,
        &tier.key_set_hash,
        older,
    );
    fs::write(d.join("base.input"), &base).unwrap();
    write_att(&d.join("a1.json"), &dep, &a1, &tier);
    write_att(&d.join("a2.json"), &dep, &a2, &tier);
    write_att(&d.join("stale2.json"), &dep, &stale2, &tier);
    write_att(&d.join("stale1.json"), &dep, &stale1, &tier);
    // The control: the two real attestations together are t of n, collected and accepted.
    let c = collect(&base, &[a1, a2]).unwrap();
    run(&c.input).unwrap();
    Setup { d, base, a1 }
}

fn cli(s: &Setup, args: &[&str]) -> std::process::Output {
    Command::new(bin()).arg("--base").arg(s.d.join("base.input")).args(args).current_dir(&s.d).output().unwrap()
}

#[test]
fn real_mode_writes_each_case_and_the_program_refuses_each_with_its_error() {
    let s = setup("cases");
    let out = cli(&s, &["--real", "a1.json,a2.json", "--stale", "stale2.json", "--out-dir", "out", "--prefix", "t"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let expected: [(&str, Error, usize); 4] = [
        ("below-t", Error::InsufficientAttestations { have: 1, need: 2 }, 1),
        ("outsider", Error::NotAccredited, 2),
        ("repeated", Error::DuplicateAttestor, 2),
        ("stale-tip", Error::NotADescendant { tip_id: 190 }, 2),
    ];
    let written = fs::read_dir(s.d.join("out")).unwrap().count();
    assert_eq!(written, 2 * expected.len(), "an input and a meta per case, nothing else");
    for (case, err, count) in expected {
        let input = fs::read(s.d.join(format!("out/t-{case}.input"))).unwrap();
        let meta: serde_json::Value =
            serde_json::from_slice(&fs::read(s.d.join(format!("out/t-{case}.json"))).unwrap()).unwrap();
        let decoded = CheckpointInputB::decode(&input).unwrap();
        assert_eq!(decoded.attestations.len(), count, "{case}");
        assert_eq!(decoded.attestations[0], s.a1, "{case}: the kept real attestation comes first");
        assert_eq!(run(&input).unwrap_err(), err, "{case}");
        assert!(meta["native"].as_str().unwrap().starts_with("refused: "), "{case}: {}", meta["native"]);
        // Only the attestations differ from the run's base input.
        let mut again = CheckpointInputB::decode(&input).unwrap();
        again.attestations = CheckpointInputB::decode(&s.base).unwrap().attestations;
        assert_eq!(again.encode(), CheckpointInputB::decode(&s.base).unwrap().encode(), "{case}");
    }
    // The outsider's channel key is outside the live key set; its certificate and signature are valid for this
    // deployment, so the program refuses it for the key alone.
    let outsider =
        CheckpointInputB::decode(&fs::read(s.d.join("out/t-outsider.input")).unwrap()).unwrap().attestations[1];
    assert!(!CheckpointInputB::decode(&s.base).unwrap().parent.keys.contains(&outsider.channel_key));
    let dep = deployment_of(&CheckpointInputB::decode(&s.base).unwrap().config);
    assert_eq!(outsider.verify(&dep, &tier_n3().key_set_hash), Ok(()));
}

#[test]
fn real_mode_refuses_inputs_it_must_not_build() {
    let s = setup("refused");
    // A stale round by an attestor among the t - 1 kept would be a repeated key, not a stale tip.
    let out = cli(&s, &["--real", "a1.json,a2.json", "--stale", "stale1.json", "--out-dir", "o1"]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("among the kept"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Real attestations of two different tips.
    let out = cli(&s, &["--real", "a1.json,stale2.json", "--out-dir", "o2"]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("different tips"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Fewer real attestations than t: no case can keep t - 1 and be one short of t.
    let out = cli(&s, &["--real", "a1.json", "--out-dir", "o3"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("real attestations, t(n = 3) = 2"));
    // No attestation named at all (an empty list, or commas only): an error, not a panic.
    for (list, o) in [("", "o4"), (",", "o5")] {
        let out = cli(&s, &["--real", list, "--out-dir", o]);
        assert_eq!(out.status.code(), Some(1), "{}", String::from_utf8_lossy(&out.stderr));
        assert!(String::from_utf8_lossy(&out.stderr).contains("names no attestation file"));
    }
    for o in ["o1", "o2", "o3", "o4", "o5"] {
        assert_eq!(fs::read_dir(s.d.join(o)).map(|r| r.count()).unwrap_or(0), 0, "{o}: nothing written");
    }
}
