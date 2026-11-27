//! Writes the seed corpora of the fuzz targets from the committed vectors: the
//! consensus-spec-tests light-client sync cases (minimal preset, every fork) and the mainnet
//! and Sepolia fixtures of `crates/ferrata-spectests/fixtures` (headers, receipts, proofs,
//! bootstraps, finality updates).
//!
//! The `bls` target's seeds come from the official BLS vectors (`ethereum/bls12-381-tests`:
//! `fast_aggregate_verify`, `verify`, `deserialization_G1`, `deserialization_G2`; consensus-specs
//! `eth_fast_aggregate_verify`, `eth_aggregate_pubkeys`), from every signed update of the spec sync
//! cases with the committee that signed it, and from the signed updates of the mainnet and Sepolia
//! fixtures (512-member committees, written condensed and, once per network, whole).
//!
//! `cargo run --release --example seeds -- <out dir>` (from `fuzz/`); the spec vectors are read
//! from `FERRATA_VECTORS` or the default path of `ferrata-spectests`, the BLS vectors from
//! `FERRATA_BLS_VECTORS` or its default path.
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use bls12_381::{G1Affine, G1Projective};
use ferrata_core::{
    bls::{BlsBackend, Blst},
    fork::{ChainSpec, LcFork, Preset, FAR_FUTURE_EPOCH},
    store::StoreCore,
    types::{BlsPubkey, BlsSignature, LightClientBootstrap, LightClientUpdate, SyncCommittee},
    upgrade::{upgrade_bootstrap, upgrade_update},
    validate::{sync_committee_signing_root, Context},
    witness::{CompactBootstrap, CompactStore, CompactUpdate, HeaderRecord},
    Root,
};
use ferrata_fuzz::{
    bls::{self, Edit},
    encode_raw_proof, lc_fork_byte, LcInput,
};
use ferrata_spectests::{bls_vectors_dir, cases, execkit, general_vectors_dir, load_sync_case, read_ssz_snappy};
use serde_yaml::Value;

const FORKS: [LcFork; 5] = [LcFork::Altair, LcFork::Capella, LcFork::Deneb, LcFork::Electra, LcFork::Gloas];

fn put(out: &Path, target: &str, name: &str, bytes: &[u8]) {
    let dir = out.join(target);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(name), bytes).unwrap();
}

fn main() {
    let out = std::env::args().nth(1).expect("output directory");
    let out = Path::new(&out);
    let mut count = 0usize;

    // Spec sync cases.
    for (_, dir) in cases("sync").expect("spec vectors") {
        let case = load_sync_case(&dir).expect("case");
        let spec = &case.config.spec;
        let n = spec.preset.sync_committee_size;
        let tag = format!(
            "{}-{}",
            dir.parent()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_string_lossy(),
            dir.file_name().unwrap().to_string_lossy()
        );
        let raw_bootstrap = read_ssz_snappy(&dir.join("bootstrap.ssz_snappy")).unwrap();
        let bootstrap_fork = FORKS
            .into_iter()
            .find(|&f| {
                LightClientBootstrap::decode(&raw_bootstrap, f, n).ok().and_then(|b| {
                    if f == case.store_fork {
                        Some(b)
                    } else {
                        upgrade_bootstrap(b, f, case.store_fork, spec).ok()
                    }
                }) == Some(case.bootstrap.clone())
            })
            .expect("bootstrap fork");
        let nb = [lc_fork_byte(case.store_fork), u8::from(n != 32)];
        put(
            out,
            "ssz",
            &format!("{tag}-bootstrap"),
            &[&[lc_fork_byte(bootstrap_fork), nb[1]][..], &raw_bootstrap].concat(),
        );
        let cb = CompactBootstrap::of(&case.bootstrap);
        put(out, "witness", &format!("{tag}-cbootstrap"), &[&nb[..], &cb.encode(case.store_fork)].concat());
        if let Ok(core) = StoreCore::from_bootstrap(&case.trusted_block_root, &case.bootstrap, case.store_fork, spec) {
            put(out, "witness", &format!("{tag}-cstore"), &[&nb[..], &CompactStore::of(&core).encode()].concat());
        }
        put(
            out,
            "witness",
            &format!("{tag}-record"),
            &[&nb[..], &HeaderRecord::of(&case.bootstrap.header, case.store_fork, spec).encode()].concat(),
        );
        let mut files: Vec<_> = fs::read_dir(&dir).unwrap().filter_map(|e| e.ok().map(|e| e.path())).collect();
        files.sort();
        for path in files {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if !name.starts_with("update") || !name.ends_with(".ssz_snappy") {
                continue;
            }
            let raw = read_ssz_snappy(&path).unwrap();
            for f in FORKS {
                let Ok(u) = LightClientUpdate::decode(&raw, f, n) else { continue };
                let up: Option<LightClientUpdate> =
                    if f == case.store_fork { Some(u) } else { upgrade_update(u, f, case.store_fork, spec).ok() };
                let Some(up) = up else { continue };
                let Some((_, slot)) = case.updates.iter().find(|(x, _)| *x == up) else { continue };
                let stem = name.trim_end_matches(".ssz_snappy");
                put(out, "ssz", &format!("{tag}-{stem}"), &[&[lc_fork_byte(f), nb[1]][..], &raw].concat());
                let cu = CompactUpdate::of(&up);
                put(out, "witness", &format!("{tag}-c{stem}"), &[&nb[..], &cu.encode(case.store_fork)].concat());
                let lc = LcInput {
                    spec: spec.clone(),
                    bootstrap_fork,
                    store_fork: case.store_fork,
                    update_fork: f,
                    current_slot: *slot,
                    genesis_validators_root: case.genesis_validators_root,
                    bootstrap: &raw_bootstrap,
                    update: &raw,
                };
                put(out, "lc", &format!("{tag}-{stem}"), &lc.encode());
                count += 1;
                break;
            }
        }
    }

    // Mainnet and Sepolia fixtures.
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/ferrata-spectests/fixtures");
    for set in fs::read_dir(&fixtures).unwrap().filter_map(|e| e.ok().map(|e| e.path())) {
        let set_name = set.file_name().unwrap().to_string_lossy().to_string();
        for path in fs::read_dir(&set).unwrap().filter_map(|e| e.ok().map(|e| e.path())) {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let tag = format!("{set_name}-{name}");
            let bytes = fs::read(&path).unwrap();
            if name.ends_with(".rlp") {
                put(out, "exec", &tag, &bytes);
                put(out, "rlp", &tag, &bytes);
            } else if name.ends_with(".ssz") {
                for (fb, f) in FORKS.iter().enumerate() {
                    let _ = f;
                    put(out, "ssz", &format!("{tag}-{fb}"), &[&[fb as u8, 1][..], &bytes].concat());
                }
            } else if name.starts_with("receipts_") && name.ends_with(".json") {
                let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                let Some(list) = v.get("result").unwrap_or(&v).as_array() else { continue };
                let receipts: Vec<Vec<u8>> = list.iter().map(execkit::receipt_rlp).collect();
                if receipts.is_empty() {
                    continue;
                }
                for (k, r) in receipts.iter().enumerate().take(12) {
                    put(out, "exec", &format!("{tag}-{k}"), r);
                    put(out, "rlp", &format!("{tag}-{k}"), r);
                }
                let trie = execkit::Trie::new(receipts.iter().enumerate().map(|(k, r)| {
                    let (b, l) = ferrata_core::rlp::encode_u64(k as u64);
                    (b[..l].to_vec(), r.clone())
                }));
                for k in [0u64, 1, 7, 127, 128, receipts.len() as u64 - 1, receipts.len() as u64] {
                    let (b, l) = ferrata_core::rlp::encode_u64(k);
                    let proof = trie.proof(&b[..l]);
                    put(out, "mpt", &format!("{tag}-receipt-{k}"), &encode_raw_proof(&b[..l], &proof));
                }
            } else if name.starts_with("proof_") && name.ends_with(".json") {
                let Ok(p) = std::panic::catch_unwind(|| execkit::account_proof(&bytes)) else { continue };
                let key = execkit::keccak(&p.address);
                put(out, "mpt", &format!("{tag}-account"), &encode_raw_proof(&key, &p.proof));
                for (k, node) in p.proof.iter().enumerate() {
                    put(out, "rlp", &format!("{tag}-node{k}"), node);
                }
                let account = execkit::account_rlp(p.nonce, 0, p.storage_hash, p.code_hash);
                put(out, "exec", &format!("{tag}-account"), &account);
                for (s, sp) in p.storage.iter().enumerate() {
                    put(
                        out,
                        "mpt",
                        &format!("{tag}-storage{s}"),
                        &encode_raw_proof(&execkit::keccak(&sp.key), &sp.proof),
                    );
                }
            }
        }
    }
    eprintln!("seeds: {count} spec updates written to {}", out.display());
    let n = bls_seeds(out);
    eprintln!("seeds: {n} bls inputs written to {}", out.display());
}

// The `bls` target.

const MAINNET_GVR: &str = "4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95";
const SEPOLIA_GVR: &str = "d8ea171f3c94aea21ebc42a1ed61052acf3f9209c00e4efbaaddac09ed9b8078";

/// Mainnet's fork schedule (consensus-specs `configs/mainnet.yaml`, as `mainnet_replay.rs`).
fn mainnet() -> ChainSpec {
    ChainSpec {
        preset: Preset::MAINNET,
        versions: [
            [0, 0, 0, 0],
            [1, 0, 0, 0],
            [2, 0, 0, 0],
            [3, 0, 0, 0],
            [4, 0, 0, 0],
            [5, 0, 0, 0],
            [6, 0, 0, 0],
            [7, 0, 0, 0],
            [8, 0, 0, 0],
        ],
        epochs: [0, 74_240, 144_896, 194_048, 269_568, 364_032, 411_392, FAR_FUTURE_EPOCH, FAR_FUTURE_EPOCH],
    }
}

/// Sepolia's fork schedule (as `sepolia_steps.rs`).
fn sepolia() -> ChainSpec {
    let v = |b: u8| [0x90, 0x00, 0x00, b];
    ChainSpec {
        preset: Preset::MAINNET,
        versions: [v(0x69), v(0x70), v(0x71), v(0x72), v(0x73), v(0x74), v(0x75), v(0x76), [0; 4]],
        epochs: [0, 50, 100, 56_832, 132_608, 222_464, 272_640, 353_024, FAR_FUTURE_EPOCH],
    }
}

fn sorted_dir(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    out.sort();
    out
}

fn yaml(path: &Path) -> Value {
    serde_yaml::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

/// A hex value, cut or zero-padded to `N` bytes.
fn fit<const N: usize>(v: &Value) -> Option<[u8; N]> {
    let b = hex::decode(v.as_str()?.trim_start_matches("0x")).ok()?;
    let mut out = [0u8; N];
    let l = b.len().min(N);
    out[..l].copy_from_slice(&b[..l]);
    Some(out)
}

fn g1(k: &BlsPubkey) -> Option<G1Projective> {
    Option::<G1Affine>::from(G1Affine::from_compressed_unchecked(k)).map(G1Projective::from)
}

/// The sum of the keys that decode.
fn sum(keys: &[BlsPubkey]) -> BlsPubkey {
    G1Affine::from(keys.iter().filter_map(g1).fold(G1Projective::identity(), |a, k| a + k)).to_compressed()
}

/// A committee of at most 33 members that signs as `c` does under `bits`: its first 16 signers'
/// keys, the other signers' keys summed in 16 groups, and the non-signers' keys summed into one
/// member that does not sign. Its keys sum to the same aggregate key and its signers to the same
/// signing key, so the recorded signature verifies under it on every path.
fn condensed(c: &SyncCommittee, bits: &[bool]) -> (SyncCommittee, Vec<bool>) {
    let signers: Vec<BlsPubkey> = c.pubkeys.iter().zip(bits).filter(|(_, b)| **b).map(|(k, _)| *k).collect();
    let absent: Vec<BlsPubkey> = c.pubkeys.iter().zip(bits).filter(|(_, b)| !**b).map(|(k, _)| *k).collect();
    let mut keys: Vec<BlsPubkey> = signers.iter().take(16).copied().collect();
    let rest = &signers[signers.len().min(16)..];
    if !rest.is_empty() {
        keys.extend(rest.chunks(rest.len().div_ceil(16)).map(sum));
    }
    let mut out_bits = vec![true; keys.len()];
    if !absent.is_empty() {
        keys.push(sum(&absent));
        out_bits.push(false);
    }
    (SyncCommittee { pubkeys: keys, aggregate_pubkey: c.aggregate_pubkey }, out_bits)
}

fn bls_seeds(out: &Path) -> usize {
    let mut written = HashSet::new();
    let mut put_bls = |name: &str, bytes: Vec<u8>| {
        if written.insert(bytes.clone()) {
            put(out, "bls", name, &bytes);
        }
    };
    let all = |n: usize| vec![true; n];

    // ethereum/bls12-381-tests: fast_aggregate_verify and verify as committees whose members all
    // sign, then each key of deserialization_G1 alone, and each signature of deserialization_G2
    // under a valid single-key case.
    let dir = bls_vectors_dir();
    let mut valid_single: Option<(BlsPubkey, Root, BlsSignature)> = None;
    for sub in ["fast_aggregate_verify", "verify"] {
        for path in sorted_dir(&dir.join(sub)) {
            let v = yaml(&path);
            let input = &v["input"];
            let mut keys: Vec<BlsPubkey> = match input.get("pubkeys") {
                Some(Value::Sequence(list)) => list.iter().filter_map(fit).collect(),
                _ => input.get("pubkey").and_then(fit).into_iter().collect(),
            };
            let (Some(message), Some(signature)) = (fit(&input["message"]), fit(&input["signature"])) else { continue };
            if keys.is_empty() {
                keys.push(bls::G1_IDENTITY);
            }
            if valid_single.is_none() && keys.len() == 1 && v["output"].as_bool() == Some(true) {
                valid_single = Some((keys[0], message, signature));
            }
            let c = SyncCommittee { aggregate_pubkey: sum(&keys), pubkeys: keys };
            let name = format!("vec-{}", path.file_stem().unwrap().to_string_lossy());
            put_bls(&name, bls::encode(&c, &all(c.pubkeys.len()), &message, &signature, &[]));
        }
    }
    let (pk, message, signature) = valid_single.expect("a valid single-key case");
    for path in sorted_dir(&dir.join("deserialization_G1")) {
        let Some(key) = fit::<48>(&yaml(&path)["input"]["pubkey"]) else { continue };
        let c = SyncCommittee { pubkeys: vec![key], aggregate_pubkey: key };
        let name = format!("vec-g1-{}", path.file_stem().unwrap().to_string_lossy());
        put_bls(&name, bls::encode(&c, &[true], &message, &signature, &[]));
    }
    for path in sorted_dir(&dir.join("deserialization_G2")) {
        let Some(sig) = fit::<96>(&yaml(&path)["input"]["signature"]) else { continue };
        let c = SyncCommittee { pubkeys: vec![pk], aggregate_pubkey: pk };
        let name = format!("vec-g2-{}", path.file_stem().unwrap().to_string_lossy());
        put_bls(&name, bls::encode(&c, &[true], &message, &sig, &[]));
    }
    // consensus-specs: eth_fast_aggregate_verify, and eth_aggregate_pubkeys as committee checks.
    let general = general_vectors_dir().join("altair/bls");
    for case in sorted_dir(&general.join("eth_fast_aggregate_verify/bls")) {
        let v = yaml(&case.join("data.yaml"));
        let input = &v["input"];
        let mut keys: Vec<BlsPubkey> = input["pubkeys"].as_sequence().into_iter().flatten().filter_map(fit).collect();
        let (Some(message), Some(signature)) = (fit(&input["message"]), fit(&input["signature"])) else { continue };
        if keys.is_empty() {
            keys.push(bls::G1_IDENTITY);
        }
        let c = SyncCommittee { aggregate_pubkey: sum(&keys), pubkeys: keys };
        let name = format!("spec-{}", case.file_name().unwrap().to_string_lossy());
        put_bls(&name, bls::encode(&c, &all(c.pubkeys.len()), &message, &signature, &[]));
    }
    for case in sorted_dir(&general.join("eth_aggregate_pubkeys/bls")) {
        let v = yaml(&case.join("data.yaml"));
        let keys: Vec<BlsPubkey> = v["input"].as_sequence().into_iter().flatten().filter_map(fit).collect();
        if keys.is_empty() {
            continue;
        }
        let c = SyncCommittee { aggregate_pubkey: fit(&v["output"]).unwrap_or([0u8; 48]), pubkeys: keys };
        let name = format!("spec-{}", case.file_name().unwrap().to_string_lossy());
        put_bls(&name, bls::encode(&c, &all(c.pubkeys.len()), &message, &signature, &[]));
    }

    // Spec sync cases: every update that verifies, with the committee of its signature period.
    let mut first: Option<(SyncCommittee, Vec<bool>, Root, BlsSignature)> = None;
    let (mut members, mut known) = (0usize, 0usize);
    let blst = Blst::default();
    for (_, dir) in cases("sync").expect("spec vectors") {
        let case = load_sync_case(&dir).expect("case");
        let spec = &case.config.spec;
        let ctx = Context { spec, genesis_validators_root: case.genesis_validators_root, bls: &blst };
        let mut committees: HashMap<u64, SyncCommittee> = HashMap::new();
        committees.insert(
            spec.period_at_slot(case.bootstrap.header.beacon.slot),
            case.bootstrap.current_sync_committee.clone(),
        );
        for (u, _) in &case.updates {
            if !u.next_sync_committee.is_empty() {
                committees
                    .entry(spec.period_at_slot(u.attested_header.beacon.slot) + 1)
                    .or_insert_with(|| u.next_sync_committee.clone());
            }
        }
        let tag = dir.iter().rev().take(5).map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>();
        for (k, (u, _)) in case.updates.iter().enumerate() {
            let Some(c) = committees.get(&spec.period_at_slot(u.signature_slot)) else { continue };
            let root = sync_committee_signing_root(u, &ctx);
            let (bits, sig) = (&u.sync_aggregate.bits, &u.sync_aggregate.signature);
            if blst.fast_aggregate_verify(c, bits, &root, sig).is_err() {
                continue;
            }
            let bytes = bls::encode(c, bits, &root, sig, &[]);
            let parsed = bls::parse(&bytes);
            members += parsed.secrets.len();
            known += parsed.secrets.iter().filter(|s| s.is_some()).count();
            put_bls(&format!("sync-{}-{}-{k}", tag[4], tag[0]), bytes);
            if first.is_none() {
                first = Some((c.clone(), bits.clone(), root, *sig));
            }
        }
    }
    eprintln!("seeds: bls: {known} of {members} members of the spec committees are pyspec test keys");

    // Each edit, on the first signed spec update (a committee of pyspec keys, whose secrets the
    // harness knows).
    let (c, bits, root, sig) = first.expect("a signed spec update");
    let mut demos: Vec<(String, Vec<Edit>)> = vec![
        ("resign".into(), vec![Edit::Sign]),
        ("all-sign".into(), vec![Edit::Fill(0xff), Edit::Sign]),
        ("every-eighth".into(), vec![Edit::Fill(0x01), Edit::Sign]),
        ("none".into(), vec![Edit::Fill(0x00)]),
        ("bit".into(), vec![Edit::Bit(3), Edit::Sign]),
        ("secret".into(), vec![Edit::Secret(0, 12_345), Edit::SumAggregate, Edit::Sign]),
        ("secret-zero".into(), vec![Edit::Secret(1, 0), Edit::SumAggregate, Edit::Sign]),
        ("copy".into(), vec![Edit::Copy(2, 5), Edit::SumAggregate, Edit::Sign]),
        ("negate".into(), vec![Edit::Negate(4), Edit::SumAggregate, Edit::Sign]),
        (
            "cancel".into(),
            vec![
                Edit::Copy(1, 0),
                Edit::Negate(1),
                Edit::Truncate(2),
                Edit::Fill(0x03),
                Edit::SumAggregate,
                Edit::Sign,
            ],
        ),
        ("truncate".into(), vec![Edit::Truncate(7), Edit::SumAggregate, Edit::Sign]),
        ("single".into(), vec![Edit::Truncate(0), Edit::Fill(0x01), Edit::SumAggregate, Edit::Sign]),
    ];
    for t in 0..7u8 {
        demos.push((format!("key-torsion-{t}"), vec![Edit::KeyTorsion(2, t), Edit::SumAggregate, Edit::Sign]));
        demos.push((format!("aggregate-torsion-{t}"), vec![Edit::AggregateTorsion(t)]));
    }
    for w in 0..12u8 {
        demos.push((format!("key-special-{w}"), vec![Edit::KeySpecial(2, w), Edit::SumAggregate, Edit::Sign]));
        demos.push((format!("aggregate-special-{w}"), vec![Edit::AggregateSpecial(w)]));
    }
    for t in 0..8u8 {
        demos.push((format!("signature-torsion-{t}"), vec![Edit::SignatureTorsion(t)]));
    }
    for w in 0..14u8 {
        demos.push((format!("signature-special-{w}"), vec![Edit::SignatureSpecial(w)]));
    }
    for (name, edits) in &demos {
        put_bls(&format!("edit-{name}"), bls::encode(&c, &bits, &root, &sig, edits));
    }
    put_bls("edit-other-root", bls::encode(&c, &bits, &[0x42; 32], &sig, &[Edit::Sign]));

    // Mainnet and Sepolia fixtures: every update whose committee the set holds.
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/ferrata-spectests/fixtures");
    let mut whole = HashSet::new();
    for set in sorted_dir(&fixtures) {
        let set_name = set.file_name().unwrap().to_string_lossy().to_string();
        let network = if set_name.starts_with("mainnet") { "mainnet" } else { "sepolia" };
        let (spec, gvr) = if network == "mainnet" { (mainnet(), MAINNET_GVR) } else { (sepolia(), SEPOLIA_GVR) };
        let gvr: Root = hex::decode(gvr).unwrap().try_into().unwrap();
        let mut committees: HashMap<u64, SyncCommittee> = HashMap::new();
        if let Ok(b) = fs::read(set.join("bootstrap.ssz")) {
            if let Some(bs) = FORKS.iter().find_map(|f| LightClientBootstrap::decode(&b, *f, 512).ok()) {
                committees.insert(spec.period_at_slot(bs.header.beacon.slot), bs.current_sync_committee);
            }
        }
        let mut updates = Vec::new();
        for path in sorted_dir(&set) {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if !(name.starts_with("update_") && name.ends_with(".ssz")) {
                continue;
            }
            let bytes = fs::read(&path).unwrap();
            if let Some(u) = FORKS.iter().find_map(|f| LightClientUpdate::decode(&bytes, *f, 512).ok()) {
                updates.push((name.trim_end_matches(".ssz").to_string(), u));
            }
        }
        for (_, u) in &updates {
            if !u.next_sync_committee.is_empty() {
                committees
                    .entry(spec.period_at_slot(u.attested_header.beacon.slot) + 1)
                    .or_insert_with(|| u.next_sync_committee.clone());
            }
        }
        let ctx = Context { spec: &spec, genesis_validators_root: gvr, bls: &blst };
        for (name, u) in &updates {
            let Some(c) = committees.get(&spec.period_at_slot(u.signature_slot)) else { continue };
            let root = sync_committee_signing_root(u, &ctx);
            let (bits, sig) = (&u.sync_aggregate.bits, &u.sync_aggregate.signature);
            if blst.fast_aggregate_verify(c, bits, &root, sig).is_err() {
                eprintln!("seeds: bls: {set_name}/{name} does not verify under the set's committee; skipped");
                continue;
            }
            let (small, small_bits) = condensed(c, bits);
            put_bls(&format!("fixture-{set_name}-{name}"), bls::encode(&small, &small_bits, &root, sig, &[]));
            if whole.insert(network) {
                put_bls(&format!("fixture-{set_name}-{name}-whole"), bls::encode(c, bits, &root, sig, &[]));
            }
        }
    }
    written.len()
}
