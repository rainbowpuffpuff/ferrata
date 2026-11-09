//! Runs the bench guests with the RISC Zero executor (no proving) and prints cycle counts.
//!
//! Usage: `host [CONFIG ...] [--modes MODE,...]`
//! CONFIG is one of the guest names in `CONFIGS` (default: all of them). MODEs: e2e, phases,
//! split, single, onchain, single-onchain, coresplit, steps (default: all; the last two and
//! `split` run for blst configs only).

use std::{fs, io::Write, path::PathBuf, time::Instant};

use ferrata_core::{
    bls::{Aggregation, Blst},
    fork::{ChainSpec, LcFork, Preset, FAR_FUTURE_EPOCH},
    types::LightClientUpdate,
    validate::{sync_committee_signing_root, Context},
};
use risc0_zkvm::{default_executor, ExecutorEnv, ExecutorEnvBuilder, SessionInfo};
use sha2::{Digest, Sha256};

const MODE_E2E: u32 = 0;
const MODE_PHASES: u32 = 1;
const MODE_SPLIT_ML: u32 = 2;
const MODE_SPLIT_FE: u32 = 3;
const MODE_SINGLE: u32 = 4;
const MODE_ONCHAIN: u32 = 5;
const MODE_SINGLE_ONCHAIN: u32 = 6;
const MODE_CORE_SPLIT_1: u32 = 7;
const MODE_CORE_SPLIT_2: u32 = 8;
const MODE_STEP_1: u32 = 9;
const MODE_STEP_2: u32 = 10;

const ALL_MODES: [&str; 8] = ["e2e", "phases", "split", "single", "onchain", "single-onchain", "coresplit", "steps"];

struct Config {
    name: &'static str,
    elf: &'static [u8],
    /// The split demonstration needs blst's raw Fp12.
    split: bool,
}

const CONFIGS: &[Config] = &[
    Config { name: "blst-accel", elf: methods::BLST_ACCEL_ELF, split: true },
    Config { name: "zkcrypto-accel", elf: methods::ZKCRYPTO_ACCEL_ELF, split: false },
    Config { name: "zkcrypto-plain", elf: methods::ZKCRYPTO_PLAIN_ELF, split: false },
    Config { name: "blst-accel-v0314", elf: methods::BLST_ACCEL_V0314_ELF, split: true },
    Config { name: "blst-plain", elf: methods::BLST_PLAIN_ELF, split: true },
    Config { name: "nopatch", elf: methods::NOPATCH_ELF, split: false },
];

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/ferrata-spectests/fixtures/mainnet-p1899")
}

/// Read a fixture and check it against the pinned SHA256SUMS.
fn fixture(name: &str) -> Vec<u8> {
    let bytes = fs::read(fixture_dir().join(name)).unwrap();
    let sums = fs::read_to_string(fixture_dir().join("SHA256SUMS")).unwrap();
    let want = sums.lines().find(|l| l.ends_with(name)).unwrap().split_whitespace().next().unwrap();
    assert_eq!(hex::encode(Sha256::digest(&bytes)), want, "{name} does not match SHA256SUMS");
    bytes
}

fn blob(b: &mut ExecutorEnvBuilder<'_>, bytes: &[u8]) {
    b.write(&(bytes.len() as u32)).unwrap();
    let mut padded = bytes.to_vec();
    padded.resize((bytes.len() + 3) & !3, 0);
    b.write_slice(&padded);
}

struct Run {
    rec: Vec<(String, u64)>,
    extra: Vec<u8>,
    info: SessionInfo,
    secs: f64,
}

fn execute(elf: &[u8], mode: u32, blobs: &[&[u8]]) -> Run {
    let mut b = ExecutorEnv::builder();
    b.write(&mode).unwrap();
    for x in blobs {
        blob(&mut b, x);
    }
    let env = b.build().unwrap();
    let started = Instant::now();
    let info = default_executor().execute(env, elf).unwrap();
    let secs = started.elapsed().as_secs_f64();
    let (rec, extra): (Vec<(String, u64)>, Vec<u8>) = info.journal.decode().unwrap();
    Run { rec, extra, info, secs }
}

fn report(out: &mut String, title: &str, run: &Run) {
    let user = run.info.cycles();
    let padded: u64 = run.info.segments.iter().map(|s| 1u64 << s.po2).sum();
    let po2s: Vec<u32> = run.info.segments.iter().map(|s| s.po2).collect();
    out.push_str(&format!("### {title}\n"));
    for (label, v) in &run.rec {
        if label.starts_with("config:") {
            out.push_str(&format!("{label}\n"));
        } else {
            out.push_str(&format!("{v:>14}  {label}\n"));
        }
    }
    out.push_str(&format!(
        "{user:>14}  EXECUTION user cycles (SessionInfo::cycles)\n{padded:>14}  EXECUTION total cycles incl. padding/paging (sum of 2^po2 over {} segments; po2 {:?})\n{:>14.2}  executor wall-clock seconds\n\n",
        run.info.segments.len(),
        po2s.iter().collect::<std::collections::BTreeSet<_>>(),
        run.secs
    ));
}

/// Mainnet fork schedule, as in ferrata-spectests/tests/mainnet_replay.rs.
fn mainnet() -> ChainSpec {
    ChainSpec {
        preset: Preset::MAINNET,
        versions: [[0, 0, 0, 0], [1, 0, 0, 0], [2, 0, 0, 0], [3, 0, 0, 0], [4, 0, 0, 0], [5, 0, 0, 0], [6, 0, 0, 0], [7, 0, 0, 0], [8, 0, 0, 0]],
        epochs: [0, 74_240, 144_896, 194_048, 269_568, 364_032, 411_392, FAR_FUTURE_EPOCH, FAR_FUTURE_EPOCH],
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut modes: Vec<String> = ALL_MODES.iter().map(|m| m.to_string()).collect();
    let mut names: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--modes" {
            modes = args[i + 1].split(',').map(str::to_string).collect();
            i += 2;
        } else {
            names.push(args[i].clone());
            i += 1;
        }
    }
    let selected: Vec<&Config> =
        CONFIGS.iter().filter(|c| names.is_empty() || names.iter().any(|n| n == c.name)).collect();
    assert!(!selected.is_empty(), "unknown config; known: {:?}", CONFIGS.iter().map(|c| c.name).collect::<Vec<_>>());

    let bootstrap = fixture("bootstrap.ssz");
    let u0 = fixture("update_0.ssz");
    let u1 = fixture("update_1.ssz");
    let d0 = LightClientUpdate::decode(&u0, LcFork::Electra, 512).unwrap();
    let d1 = LightClientUpdate::decode(&u1, LcFork::Electra, 512).unwrap();
    println!(
        "update_0: {} signers, update_1: {} signers",
        d0.sync_aggregate.participants(),
        d1.sync_aggregate.participants()
    );

    // update_1's next committee (period 1901) as 512 uncompressed keys, for check 10b.
    let mut unc = Vec::with_capacity(512 * 96);
    for pk in &d1.next_sync_committee.pubkeys {
        let p = blst::min_pk::PublicKey::uncompress(pk).unwrap();
        unc.extend_from_slice(&p.serialize());
    }
    // update_1's signing committee (period 1900, carried by update_0), as SSZ.
    let mut committee = Vec::with_capacity(513 * 48);
    for pk in &d0.next_sync_committee.pubkeys {
        committee.extend_from_slice(pk);
    }
    committee.extend_from_slice(&d0.next_sync_committee.aggregate_pubkey);

    // Store state for the one-update execution: the bootstrap header (the only variable field of
    // the bootstrap, after a fixed part of 4 + 24,624 + 192 bytes), the bootstrap committee and
    // the period-1900 committee.
    let header_offset = u32::from_le_bytes(bootstrap[..4].try_into().unwrap()) as usize;
    assert_eq!(header_offset, 4 + 24_624 + 192);
    let state_header = &bootstrap[header_offset..];
    let state_current = &bootstrap[4..4 + 24_624];
    // update_1 as a finality update: zero next_sync_committee and its branch in the SSZ
    // (fixed fields at [4, 24628) and [24628, 24820)).
    let mut u1_finality = u1.clone();
    u1_finality[4..4 + 24_624 + 192].fill(0);
    let check = LightClientUpdate::decode(&u1_finality, LcFork::Electra, 512).unwrap();
    assert!(check.next_sync_committee.is_empty() && check.next_sync_committee_branch.iter().all(|b| *b == [0u8; 32]));
    assert_eq!(check.finalized_header, d1.finalized_header);

    // Native reference for the split seam: the core's own miller_half on the host (crates.io
    // blst), which every guest-side Fp12 handed between executions must equal byte for byte.
    let spec = mainnet();
    let native = Blst::new(Aggregation::Auto);
    let gvr: [u8; 32] =
        hex::decode("4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95").unwrap().try_into().unwrap();
    let ctx = Context { spec: &spec, genesis_validators_root: gvr, bls: &native };
    let signing_root = sync_committee_signing_root(&d1, &ctx);
    let native_product = native
        .miller_half(&d0.next_sync_committee, &d1.sync_aggregate.bits, &signing_root, &d1.sync_aggregate.signature)
        .unwrap();
    Blst::final_half(&native_product).unwrap();

    let results = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../results");
    fs::create_dir_all(&results).unwrap();
    for cfg in selected {
        let mut out = String::new();
        out.push_str(&format!("## {} (ELF {} bytes)\n\n", cfg.name, cfg.elf.len()));
        if modes.iter().any(|m| m == "e2e") {
            let run = execute(cfg.elf, MODE_E2E, &[&bootstrap, &u0, &u1]);
            report(&mut out, "E2E: bootstrap -> update_0 -> update_1", &run);
        }
        if modes.iter().any(|m| m == "phases") {
            let run = execute(cfg.elf, MODE_PHASES, &[&bootstrap, &u0, &u1, &unc]);
            report(&mut out, "PHASES: update_1 against the post-update_0 store", &run);
        }
        if cfg.split && modes.iter().any(|m| m == "split") {
            let ml = execute(cfg.elf, MODE_SPLIT_ML, &[&u1, &committee]);
            report(&mut out, "SPLIT execution 1: up to and including the Miller loop (commits Fp12)", &ml);
            out.push_str(&format!("Fp12 handed over: {} bytes\n\n", ml.extra.len()));
            let fe = execute(cfg.elf, MODE_SPLIT_FE, &[&ml.extra]);
            report(&mut out, "SPLIT execution 2: final exponentiation + check from the Fp12 bytes", &fe);
        }
        if modes.iter().any(|m| m == "single") {
            let run = execute(cfg.elf, MODE_SINGLE, &[state_header, state_current, &committee, &u1]);
            report(&mut out, "SINGLE: one execution = restore store + decode + process_update(update_1 as served, handoff)", &run);
            let run = execute(cfg.elf, MODE_SINGLE, &[state_header, state_current, &committee, &u1_finality]);
            report(&mut out, "SINGLE: one execution = restore store + decode + process_update(update_1 as finality update)", &run);
        }
        if modes.iter().any(|m| m == "onchain") {
            let run = execute(cfg.elf, MODE_ONCHAIN, &[&bootstrap, &u0, &u1, state_header, state_current, &committee]);
            report(&mut out, "ONCHAIN: Adoption::OnChain store; update_1 with and without handoff; committee checks; restore paths", &run);
        }
        if modes.iter().any(|m| m == "single-onchain") {
            let run = execute(cfg.elf, MODE_SINGLE_ONCHAIN, &[state_header, state_current, &committee, &u1]);
            report(&mut out, "SINGLE-ONCHAIN: one execution = decode saved state + update, OnChain policy, no re-check (update_1 as served, handoff)", &run);
            let run = execute(cfg.elf, MODE_SINGLE_ONCHAIN, &[state_header, state_current, &committee, &u1_finality]);
            report(&mut out, "SINGLE-ONCHAIN: one execution = decode saved state + update, OnChain policy, no re-check (update_1 as finality update)", &run);
        }
        if cfg.split && modes.iter().any(|m| m == "coresplit") {
            let c1 = execute(cfg.elf, MODE_CORE_SPLIT_1, &[&u1, &committee]);
            report(&mut out, "CORESPLIT execution 1: finality checks + Blst::miller_half (commits 576 bytes)", &c1);
            assert_eq!(c1.extra, native_product.to_vec(), "guest miller_half differs from the native core");
            out.push_str("Fp12 handed over: 576 bytes, equal to the native core's miller_half\n\n");
            let c2 = execute(cfg.elf, MODE_CORE_SPLIT_2, &[&c1.extra]);
            report(&mut out, "CORESPLIT execution 2: Blst::final_half from the 576 bytes", &c2);
        }
        if cfg.split && modes.iter().any(|m| m == "steps") {
            let t1 = execute(cfg.elf, MODE_STEP_1, &[&u1, &committee]);
            report(&mut out, "STEP 1 of 3: everything before the Miller loops (finality update; hands over 480 bytes)", &t1);
            let t2 = execute(cfg.elf, MODE_STEP_2, &[&t1.extra]);
            report(&mut out, "STEP 2 of 3: Miller loops (hands over 576 bytes)", &t2);
            assert_eq!(t2.extra, native_product.to_vec(), "step 2 product differs from the native core's miller_half");
            out.push_str("Fp12 handed over: 576 bytes, equal to the native core's miller_half\n\n");
            let t3 = execute(cfg.elf, MODE_CORE_SPLIT_2, &[&t2.extra]);
            report(&mut out, "STEP 3 of 3: Blst::final_half (final exponentiation + check)", &t3);
        }
        print!("{out}");
        std::io::stdout().flush().unwrap();
        let tag = if modes.len() == ALL_MODES.len() { "all".to_string() } else { modes.join("+") };
        let file = results.join(format!("{}.{}.txt", cfg.name, tag));
        fs::write(&file, &out).unwrap();
    }
}
