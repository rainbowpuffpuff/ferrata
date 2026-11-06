//! The official BLS vectors through both libraries and both verification paths.
//!
//! * consensus-specs v1.7.0-beta.4 `general/altair/bls`: `eth_fast_aggregate_verify` (12 cases)
//!   and `eth_aggregate_pubkeys` (8 cases);
//! * `ethereum/bls12-381-tests` v0.1.2: `fast_aggregate_verify` (12) and `verify` (29).
//!
//! `fast_aggregate_verify` must match every expected output. Wherever the committee passes
//! `check_committee`, `verify_checked` must match too, under every signing-key composition.

use std::{fs, path::Path};

use blst::min_pk::{AggregatePublicKey, PublicKey};
use plumbline_core::{
    bls::{Aggregation, BlsBackend, Blst, Zkcrypto},
    types::{BlsPubkey, BlsSignature, SyncCommittee},
    Error, Root,
};
use plumbline_spectests::{bls_vectors_dir, general_vectors_dir};
use serde_yaml::Value;

fn backends() -> Vec<(&'static str, Box<dyn BlsBackend>)> {
    vec![
        ("blst/auto", Box::new(Blst::new(Aggregation::Auto))),
        ("blst/additive", Box::new(Blst::new(Aggregation::Additive))),
        ("blst/subtractive", Box::new(Blst::new(Aggregation::Subtractive))),
        ("zkcrypto/auto", Box::new(Zkcrypto::new(Aggregation::Auto))),
        ("zkcrypto/additive", Box::new(Zkcrypto::new(Aggregation::Additive))),
        ("zkcrypto/subtractive", Box::new(Zkcrypto::new(Aggregation::Subtractive))),
    ]
}

fn yaml(path: &Path) -> Value {
    serde_yaml::from_str(&fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))).unwrap()
}

fn bytes<const N: usize>(v: &Value) -> [u8; N] {
    let s = v.as_str().unwrap();
    hex::decode(s.trim_start_matches("0x")).unwrap().try_into().unwrap()
}

/// The aggregate of whatever keys decode, so `check_committee` decides on validity alone.
fn aggregate(keys: &[BlsPubkey]) -> BlsPubkey {
    let pks: Vec<PublicKey> = keys.iter().filter_map(|k| PublicKey::uncompress(k).ok()).collect();
    let refs: Vec<&PublicKey> = pks.iter().collect();
    AggregatePublicKey::aggregate(&refs, false).map(|a| a.to_public_key().compress()).unwrap_or([0u8; 48])
}

struct Case {
    name: String,
    keys: Vec<BlsPubkey>,
    message: Root,
    signature: BlsSignature,
    expected: bool,
}

fn files(dir: &Path, data_yaml: bool) -> Vec<std::path::PathBuf> {
    let mut out: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e} (run scripts/fetch-vectors.sh)", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .map(|p| if data_yaml { p.join("data.yaml") } else { p })
        .collect();
    out.sort();
    out
}

fn load(paths: &[std::path::PathBuf]) -> Vec<Case> {
    paths
        .iter()
        .map(|path| {
            let v = yaml(path);
            let input = &v["input"];
            let keys = match input.get("pubkeys") {
                Some(Value::Sequence(list)) => list.iter().map(bytes::<48>).collect(),
                _ => vec![bytes::<48>(&input["pubkey"])],
            };
            Case {
                name: path.display().to_string(),
                keys,
                message: bytes::<32>(&input["message"]),
                signature: bytes::<96>(&input["signature"]),
                expected: v["output"].as_bool().unwrap(),
            }
        })
        .collect()
}

fn run(cases: &[Case]) -> (usize, usize) {
    let (mut checked_runs, mut eth_empty) = (0, 0);
    for case in cases {
        let committee = SyncCommittee { pubkeys: case.keys.clone(), aggregate_pubkey: aggregate(&case.keys) };
        let bits = vec![true; case.keys.len()];
        // `eth_fast_aggregate_verify` accepts no keys with the identity signature. The light
        // client calls `FastAggregateVerify`, which never does, and it requires at least one
        // participant before it gets here.
        let expected = if case.keys.is_empty() {
            eth_empty += 1;
            false
        } else {
            case.expected
        };
        for (name, bls) in backends() {
            let spec = bls.fast_aggregate_verify(&committee, &bits, &case.message, &case.signature);
            assert_eq!(spec.is_ok(), expected, "{name} fast_aggregate_verify {}: {spec:?}", case.name);
            if case.keys.is_empty() {
                assert_eq!(spec, Err(Error::ParticipationLow));
            }
            if bls.check_committee(&committee).is_ok() {
                let checked = bls.verify_checked(&committee, &bits, &case.message, &case.signature);
                assert_eq!(checked, spec, "{name} verify_checked vs fast_aggregate_verify {}", case.name);
                checked_runs += 1;
            }
        }
    }
    (checked_runs, eth_empty)
}

#[test]
fn consensus_specs_eth_fast_aggregate_verify() {
    let dir = general_vectors_dir().join("altair/bls/eth_fast_aggregate_verify/bls");
    let cases = load(&files(&dir, true));
    assert_eq!(cases.len(), 12);
    let (checked, empty) = run(&cases);
    println!("eth_fast_aggregate_verify: {} cases, {checked} checked-path runs, {empty} empty-key cases", cases.len());
}

#[test]
fn bls12_381_tests_fast_aggregate_verify_and_verify() {
    let fav = load(&files(&bls_vectors_dir().join("fast_aggregate_verify"), false));
    let verify = load(&files(&bls_vectors_dir().join("verify"), false));
    assert_eq!((fav.len(), verify.len()), (12, 29));
    let (a, _) = run(&fav);
    let (b, _) = run(&verify);
    println!(
        "bls12-381-tests: {} fast_aggregate_verify + {} verify cases, {} checked-path runs",
        fav.len(),
        verify.len(),
        a + b
    );
}

#[test]
fn consensus_specs_eth_aggregate_pubkeys_through_check_committee() {
    let dir = general_vectors_dir().join("altair/bls/eth_aggregate_pubkeys/bls");
    let paths = files(&dir, true);
    assert_eq!(paths.len(), 8);
    for path in &paths {
        let v = yaml(path);
        let keys: Vec<BlsPubkey> = v["input"].as_sequence().unwrap().iter().map(bytes::<48>).collect();
        let expected = match &v["output"] {
            Value::Null => None,
            out => Some(bytes::<48>(out)),
        };
        let committee = SyncCommittee { pubkeys: keys.clone(), aggregate_pubkey: expected.unwrap_or([0u8; 48]) };
        for (name, bls) in backends() {
            let result = bls.check_committee(&committee);
            match expected {
                Some(_) => assert_eq!(result, Ok(()), "{name} {}", path.display()),
                None => assert_eq!(result, Err(Error::CommitteeInvalid), "{name} {}", path.display()),
            }
        }
    }
}
