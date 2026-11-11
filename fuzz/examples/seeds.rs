//! Writes the seed corpora of the fuzz targets from the committed vectors: the
//! consensus-spec-tests light-client sync cases (minimal preset, every fork) and the mainnet
//! and Sepolia fixtures of `crates/ferrata-spectests/fixtures` (headers, receipts, proofs,
//! bootstraps, finality updates).
//!
//! `cargo run --release --example seeds -- <out dir>` (from `fuzz/`); the spec vectors are read
//! from `FERRATA_VECTORS` or the default path of `ferrata-spectests`.
use std::{fs, path::Path};

use ferrata_core::{
    fork::LcFork,
    store::StoreCore,
    types::{LightClientBootstrap, LightClientUpdate},
    upgrade::{upgrade_bootstrap, upgrade_update},
    witness::{CompactBootstrap, CompactStore, CompactUpdate, HeaderRecord},
};
use ferrata_fuzz::{encode_raw_proof, lc_fork_byte, LcInput};
use ferrata_spectests::{cases, execkit, load_sync_case, read_ssz_snappy};

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
        let tag = format!("{}-{}", dir.parent().unwrap().parent().unwrap().parent().unwrap().parent().unwrap().file_name().unwrap().to_string_lossy(), dir.file_name().unwrap().to_string_lossy());
        let raw_bootstrap = read_ssz_snappy(&dir.join("bootstrap.ssz_snappy")).unwrap();
        let bootstrap_fork = FORKS
            .into_iter()
            .find(|&f| {
                LightClientBootstrap::decode(&raw_bootstrap, f, n)
                    .ok()
                    .and_then(|b| if f == case.store_fork { Some(b) } else { upgrade_bootstrap(b, f, case.store_fork, spec).ok() })
                    == Some(case.bootstrap.clone())
            })
            .expect("bootstrap fork");
        let nb = [lc_fork_byte(case.store_fork), u8::from(n != 32)];
        put(out, "ssz", &format!("{tag}-bootstrap"), &[&[lc_fork_byte(bootstrap_fork), nb[1]][..], &raw_bootstrap].concat());
        let cb = CompactBootstrap::of(&case.bootstrap);
        put(out, "witness", &format!("{tag}-cbootstrap"), &[&nb[..], &cb.encode(case.store_fork)].concat());
        if let Ok(core) = StoreCore::from_bootstrap(&case.trusted_block_root, &case.bootstrap, case.store_fork, spec) {
            put(out, "witness", &format!("{tag}-cstore"), &[&nb[..], &CompactStore::of(&core).encode()].concat());
        }
        put(out, "witness", &format!("{tag}-record"), &[&nb[..], &HeaderRecord::of(&case.bootstrap.header, case.store_fork, spec).encode()].concat());
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
                    put(out, "mpt", &format!("{tag}-storage{s}"), &encode_raw_proof(&execkit::keccak(&sp.key), &sp.proof));
                }
            }
        }
    }
    eprintln!("seeds: {count} spec updates written to {}", out.display());
}
