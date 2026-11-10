//! Host for the checkpoint guests. Every number it prints comes from the run it reports.
//!
//! ```text
//! host ids                       image ids of the three guests
//! host micro [--plain]           execute every fixtures/micro-* input; marginal costs per component
//!                                (--plain: the guest built without the sha2/curve25519 patches)
//! host bench [name…]             execute checkpoint/segment fixtures (default: all of them); user
//!                                cycles, segments, padded cycles, wall time; journal == native
//! host compose                   composition: segment receipt -> checkpoint that verifies it
//!                                (uses RISC0_DEV_MODE receipts unless --real is given)
//! host prove <name>              succinct proof of a fixture -> artifacts/<name>/
//! host groth16 <name>            saved succinct receipt -> Groth16 (r0vm identity_p254 + Docker)
//! host verify <name>             verify saved receipts
//! ```
//! Results are printed and appended as JSON lines to results/<command>.jsonl.

use std::fs;
use std::io::Write as _;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{bail, ensure, Context as _, Result};
use methods::{CHECKPOINT_ELF, CHECKPOINT_ID, MICRO_ELF, MICRO_ID, SEGMENT_ELF, SEGMENT_ID};
use risc0_zkvm::{
    default_executor, default_prover, Digest, ExecutorEnv, InnerReceipt, ProverOpts, Receipt, SessionInfo,
};
use serde_json::{json, Value};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn fixture(name: &str) -> Result<(Vec<u8>, Value)> {
    let dir = root().join("fixtures");
    let input =
        fs::read(dir.join(format!("{name}.input"))).with_context(|| format!("fixture {name} (run fidelity's gen)"))?;
    let meta: Value = serde_json::from_slice(&fs::read(dir.join(format!("{name}.json")))?)?;
    Ok((input, meta))
}

fn fixture_names(prefix: &str) -> Result<Vec<String>> {
    let mut names: Vec<String> = fs::read_dir(root().join("fixtures"))?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter_map(|f| f.strip_suffix(".json").map(str::to_string))
        .filter(|n| n.starts_with(prefix))
        .collect();
    names.sort();
    Ok(names)
}

fn loadavg() -> String {
    fs::read_to_string("/proc/loadavg").unwrap_or_default().trim().to_string()
}

fn record(command: &str, value: &Value) -> Result<()> {
    let dir = root().join("results");
    fs::create_dir_all(&dir)?;
    let mut f = fs::OpenOptions::new().create(true).append(true).open(dir.join(format!("{command}.jsonl")))?;
    writeln!(f, "{value}")?;
    Ok(())
}

fn hx(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

fn image_hex(id: [u32; 8]) -> String {
    hx(Digest::from(id).as_bytes())
}

/// The guest reads a little-endian u32 length, then the bytes.
fn env_for(input: &[u8]) -> Result<ExecutorEnv<'static>> {
    let len = u32::try_from(input.len())?.to_le_bytes();
    ExecutorEnv::builder().write_slice(&len).write_slice(input).build()
}

fn dev_mode() -> bool {
    std::env::var("RISC0_DEV_MODE").map(|v| !v.is_empty() && v != "0" && v != "false").unwrap_or(false)
}

struct Run {
    info: SessionInfo,
    secs: f64,
}

impl Run {
    fn padded(&self) -> u64 {
        self.info.segments.iter().map(|s| 1u64 << s.po2).sum()
    }
    fn po2s(&self) -> String {
        let mut counts = std::collections::BTreeMap::new();
        for s in &self.info.segments {
            *counts.entry(s.po2).or_insert(0u32) += 1;
        }
        counts.iter().map(|(p, c)| format!("{c}x2^{p}")).collect::<Vec<_>>().join("+")
    }
}

fn execute(elf: &[u8], env: ExecutorEnv<'_>) -> Result<Run> {
    let start = Instant::now();
    let info = default_executor().execute(env, elf)?;
    Ok(Run { info, secs: start.elapsed().as_secs_f64() })
}

fn guest_elf(kind: &str) -> Result<(&'static [u8], [u32; 8])> {
    Ok(match kind {
        "checkpoint" => (CHECKPOINT_ELF, CHECKPOINT_ID),
        "segment" => (SEGMENT_ELF, SEGMENT_ID),
        "micro" => (MICRO_ELF, MICRO_ID),
        other => bail!("unknown fixture kind {other}"),
    })
}

// ---- commands ----------------------------------------------------------------------------------------

fn cmd_ids() {
    println!("checkpoint {}", image_hex(CHECKPOINT_ID));
    println!("segment    {}", image_hex(SEGMENT_ID));
    println!("micro      {}", image_hex(MICRO_ID));
}

/// `--plain`: the same micro-benchmarks on the guest built without RISC Zero's sha2 and
/// curve25519-dalek patches (A/B), for the modes those patches affect.
fn cmd_micro_plain() -> Result<()> {
    println!("unpatched micro guest {} ; load before: {}", image_hex(methods_plain::MICRO_PLAIN_ID), loadavg());
    for name in fixture_names("micro-")? {
        if !["micro-sha256-", "micro-ed25519-", "micro-insc-1empty", "micro-insc-6empty"]
            .iter()
            .any(|p| name.starts_with(p))
        {
            continue;
        }
        let (input, meta) = fixture(&name)?;
        let run = execute(methods_plain::MICRO_PLAIN_ELF, env_for(&input)?)?;
        let c = run.info.cycles();
        println!(
            "{name:<24} k {:>9} | user cycles {:>11} | segments {:>3} ({}) | {:.2} s  [unpatched]",
            meta["k"],
            c,
            run.info.segments.len(),
            run.po2s(),
            run.secs
        );
        record(
            "micro-plain",
            &json!({"name": name, "k": meta["k"], "user_cycles": c, "segments": run.info.segments.len(),
                                       "padded_cycles": run.padded(), "wall_s": run.secs, "loadavg": loadavg()}),
        )?;
    }
    Ok(())
}

fn cmd_micro() -> Result<()> {
    println!("load before: {}", loadavg());
    let mut cycles = std::collections::BTreeMap::new();
    for name in fixture_names("micro-")? {
        let (input, meta) = fixture(&name)?;
        let run = execute(MICRO_ELF, env_for(&input)?)?;
        let c = run.info.cycles();
        println!(
            "{name:<24} k {:>9} | user cycles {:>11} | segments {:>3} ({}) | {:.2} s",
            meta["k"],
            c,
            run.info.segments.len(),
            run.po2s(),
            run.secs
        );
        record(
            "micro",
            &json!({"name": name, "k": meta["k"], "user_cycles": c, "segments": run.info.segments.len(),
                                 "padded_cycles": run.padded(), "wall_s": run.secs, "loadavg": loadavg()}),
        )?;
        cycles.insert(name.trim_start_matches("micro-").to_string(), (c as f64, meta["k"].as_f64().unwrap_or(0.0)));
    }
    let c = |k: &str| cycles.get(k).map(|x| x.0).context(k.to_string());
    let bytes = |k: &str| cycles.get(k).map(|x| x.1).context(k.to_string());
    let read = (c("read-1000000")? - c("read-10000")?) / 990_000.0;
    let sha = (c("sha256-1000000")? - c("sha256-10000")?) / 990_000.0 - read;
    let blake = (c("blake2b-1000000")? - c("blake2b-10000")?) / 990_000.0 - read;
    let sha_native = (c("sha256native-1000000")? - c("sha256native-10000")?) / 990_000.0 - read;
    let ed = (c("ed25519-6")? - c("ed25519-1")?) / 5.0;
    let ed200 = (c("ed25519msg200-6")? - c("ed25519msg200-1")?) / 5.0;
    let tree = (c("tree-101")? - c("tree-1")?) / 100.0;
    let parse_empty = (c("parse-110empty")? - c("parse-10empty")?) / 100.0;
    let hash_empty =
        ((c("hash-110empty")? - c("hash-10empty")?) - (c("parse-110empty")? - c("parse-10empty")?)) / 100.0;
    let empty_bytes = (bytes("parse-110empty")? - bytes("parse-10empty")?) / 100.0;
    let insc_empty = (c("insc-6empty")? - c("insc-1empty")?) / 5.0;
    let pp_bytes = bytes("parse-pp4")?;
    println!("\nmarginal costs (differences of the runs above; 'read' is the cost of reading input bytes):");
    println!("  input read                      {read:>10.3} cycles/byte");
    println!("  SHA-256 (accelerated)           {sha:>10.3} cycles/byte (on top of reading)");
    println!(
        "  SHA-256 via risc0_zkvm::sha     {sha_native:>10.3} cycles/byte (on top of reading; the zkVM's own API)"
    );
    println!("  BLAKE2b-256 (plain RV32IM)      {blake:>10.3} cycles/byte (on top of reading)");
    println!("  Ed25519 verify_strict           {ed:>10.0} cycles each (32-byte message)");
    println!("  Ed25519 verify_strict, 200 B    {ed200:>10.0} cycles each (200-byte message)");
    println!("  burn leaf + frontier append     {tree:>10.0} cycles each");
    println!("  empty block ({empty_bytes:.0} bytes): parse {parse_empty:.0}, hash {hash_empty:.0} cycles");
    println!(
        "  inscription check, empty block  {insc_empty:>10.0} cycles each (BLAKE2b of the tx + Ed25519 + framing)"
    );
    println!(
        "  block with 4 PP txs ({pp_bytes:.0} bytes): parse {:.0} cycles ({:.4}/byte), parse+hash {:.0}, inscription check {:.0}",
        c("parse-pp4")?,
        c("parse-pp4")? / pp_bytes,
        c("hash-pp4")?,
        c("insc-pp4")?
    );
    record(
        "micro-summary",
        &json!({"read_per_byte": read, "sha256_per_byte": sha, "sha256_native_per_byte": sha_native, "blake2b_per_byte": blake, "ed25519_each": ed, "ed25519_msg200_each": ed200,
                "burn_append_each": tree, "empty_block_bytes": empty_bytes, "empty_block_parse": parse_empty,
                "empty_block_hash": hash_empty, "inscription_check_empty_block": insc_empty, "pp4_block_bytes": pp_bytes,
                "pp4_parse": c("parse-pp4")?, "pp4_parse_hash": c("hash-pp4")?, "pp4_inscription_check": c("insc-pp4")?,
                "loadavg": loadavg()}),
    )?;
    Ok(())
}

fn cmd_bench(names: &[String]) -> Result<()> {
    let names = if names.is_empty() {
        let mut all = fixture_names("cp-")?;
        all.extend(fixture_names("seg-")?);
        all
    } else {
        names.to_vec()
    };
    println!("load before: {}", loadavg());
    for name in names {
        let (input, meta) = fixture(&name)?;
        if meta["segments"].as_u64().unwrap_or(0) > 0 {
            println!("{name}: continues from segments; run `host compose` for it");
            continue;
        }
        let (elf, _) = guest_elf(meta["kind"].as_str().unwrap_or(""))?;
        let run = execute(elf, env_for(&input)?)?;
        let want = meta["journal"].as_str().context("journal")?;
        let got = hex::encode(&run.info.journal.bytes);
        ensure!(got == want, "{name}: guest journal differs from the native run: guest {got}, fixture {want}");
        println!(
            "{name:<22} blocks {:>4} ({:>9} B) | user cycles {:>11} | segments {:>3} ({}) padded {:>11} | {:>6.2} s | journal == native",
            meta["blocks"],
            meta["block_bytes"],
            run.info.cycles(),
            run.info.segments.len(),
            run.po2s(),
            run.padded(),
            run.secs
        );
        record(
            "bench",
            &json!({"name": name, "blocks": meta["blocks"], "blocks_to_t": meta["blocks_to_t"],
                                 "block_bytes": meta["block_bytes"], "block_bytes_to_t": meta["block_bytes_to_t"],
                                 "support_entries": meta["support_entries"], "user_cycles": run.info.cycles(),
                                 "segments": run.info.segments.len(), "po2": run.po2s(), "padded_cycles": run.padded(),
                                 "wall_s": run.secs, "loadavg": loadavg(), "dev_mode": dev_mode()}),
        )?;
    }
    Ok(())
}

/// Segment `seg-empty-180` then checkpoint `cp-after-seg-180`, which verifies the segment's
/// receipt; the final journal must equal the one-piece `cp-empty-360`.
fn cmd_compose(real: bool) -> Result<()> {
    ensure!(real != dev_mode(), "use RISC0_DEV_MODE=1 for the dev-mode demo, unset with --real");
    let (seg_input, seg_meta) = fixture("seg-empty-180")?;
    let (cp_input, cp_meta) = fixture("cp-after-seg-180")?;
    let (_, whole_meta) = fixture("cp-empty-360")?;
    println!("segment image {} ; checkpoint image {}", image_hex(SEGMENT_ID), image_hex(CHECKPOINT_ID));
    println!("load before: {}", loadavg());

    let start = Instant::now();
    let opts = ProverOpts::succinct();
    let seg = default_prover().prove_with_opts(env_for(&seg_input)?, SEGMENT_ELF, &opts)?;
    let seg_secs = start.elapsed().as_secs_f64();
    ensure!(hex::encode(&seg.receipt.journal.bytes) == seg_meta["journal"].as_str().unwrap(), "segment journal");
    seg.receipt.verify(SEGMENT_ID)?;
    println!(
        "piece 1 (segment, 180 blocks): {} | user cycles {} | segments {} | {:.1} s | receipt verifies against the segment image",
        if real { "succinct receipt" } else { "dev-mode (fake) receipt" },
        seg.stats.user_cycles,
        seg.stats.segments,
        seg_secs
    );

    // Executing the second piece needs the first piece's claim as an assumption.
    let len = u32::try_from(cp_input.len())?.to_le_bytes();
    let env =
        ExecutorEnv::builder().write_slice(&len).write_slice(&cp_input).add_assumption(seg.receipt.clone()).build()?;
    let run = execute(CHECKPOINT_ELF, env)?;
    println!(
        "piece 2 (checkpoint, 180 blocks + 3 support, verifies piece 1): user cycles {} | segments {} | {:.2} s (executor)",
        run.info.cycles(),
        run.info.segments.len(),
        run.secs
    );
    let start = Instant::now();
    let env =
        ExecutorEnv::builder().write_slice(&len).write_slice(&cp_input).add_assumption(seg.receipt.clone()).build()?;
    let cp = default_prover().prove_with_opts(env, CHECKPOINT_ELF, &opts)?;
    let cp_secs = start.elapsed().as_secs_f64();
    cp.receipt.verify(CHECKPOINT_ID)?;
    let journal = hex::encode(&cp.receipt.journal.bytes);
    ensure!(journal == cp_meta["journal"].as_str().unwrap(), "checkpoint journal");
    ensure!(journal == whole_meta["journal"].as_str().unwrap(), "two pieces must prove cp-empty-360's journal");
    let kind = match &cp.receipt.inner {
        InnerReceipt::Succinct(_) => "succinct (assumption resolved)",
        InnerReceipt::Fake(_) => "fake (dev mode)",
        _ => "other",
    };
    println!(
        "piece 2 proved: {kind} | {:.1} s | verifies against the checkpoint image | journal == one-piece cp-empty-360",
        cp_secs
    );
    let claim = cp.receipt.claim()?.value()?;
    let assumptions = claim.output.value()?.map(|o| o.assumptions.value().map(|a| a.len()).unwrap_or(0));
    println!("final receipt claim: unresolved assumptions {:?}", assumptions);

    // The guest, not only the core, enforces the composition: piece 2 without piece 1's receipt,
    // and with a receipt of the segment guest over other blocks (1..=179), must both fail.
    let without = ExecutorEnv::builder().write_slice(&len).write_slice(&cp_input).build()?;
    match execute(CHECKPOINT_ELF, without) {
        Ok(r) => bail!("piece 2 executed without piece 1's receipt (exit {:?})", r.info.exit_code),
        Err(e) => {
            println!("piece 2 without piece 1's receipt: refused ({})", e.to_string().lines().next().unwrap_or(""))
        }
    }
    let mut short = checkpoint_core::input::SegmentInput::decode(&seg_input).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    short.blocks.pop();
    let other = default_prover().prove_with_opts(env_for(&short.encode())?, SEGMENT_ELF, &opts)?;
    ensure!(other.receipt.journal.bytes != seg.receipt.journal.bytes, "a different segment journal");
    let wrong =
        ExecutorEnv::builder().write_slice(&len).write_slice(&cp_input).add_assumption(other.receipt).build()?;
    match execute(CHECKPOINT_ELF, wrong) {
        Ok(r) => bail!("piece 2 executed with another segment's receipt (exit {:?})", r.info.exit_code),
        Err(e) => println!(
            "piece 2 with a receipt of another segment (blocks 1..=179): refused ({})",
            e.to_string().lines().next().unwrap_or("")
        ),
    }
    record(
        "compose",
        &json!({"real": real, "segment_user_cycles": seg.stats.user_cycles, "segment_segments": seg.stats.segments,
                               "segment_prove_s": seg_secs, "checkpoint_user_cycles": run.info.cycles(),
                               "checkpoint_segments": run.info.segments.len(), "checkpoint_prove_s": cp_secs,
                               "receipt": kind, "loadavg": loadavg()}),
    )?;
    Ok(())
}

fn artifacts(name: &str) -> Result<PathBuf> {
    let dir = root().join("artifacts").join(name);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn refuse_dev_mode() -> Result<()> {
    ensure!(!dev_mode(), "RISC0_DEV_MODE is set: receipts would be fake");
    Ok(())
}

fn cmd_prove(name: &str) -> Result<()> {
    refuse_dev_mode()?;
    let (input, meta) = fixture(name)?;
    let (elf, id) = guest_elf(meta["kind"].as_str().unwrap_or(""))?;
    println!("[{name}] load before: {}", loadavg());
    let start = Instant::now();
    let info = default_prover().prove_with_opts(env_for(&input)?, elf, &ProverOpts::succinct())?;
    let secs = start.elapsed().as_secs_f64();
    println!("[{name}] load after:  {}", loadavg());
    let receipt = info.receipt;
    ensure!(matches!(receipt.inner, InnerReceipt::Succinct(_)), "expected a succinct receipt");
    ensure!(hex::encode(&receipt.journal.bytes) == meta["journal"].as_str().unwrap(), "journal differs");
    receipt.verify(id)?;
    let s = &info.stats;
    let bytes = bincode::serialize(&receipt)?;
    println!(
        "[{name}] succinct proof: wall {secs:.2} s | segments {} | user cycles {} | total cycles {} (paging {}, reserved {}) | receipt {} bytes",
        s.segments, s.user_cycles, s.total_cycles, s.paging_cycles, s.reserved_cycles, bytes.len()
    );
    fs::write(artifacts(name)?.join("receipt-succinct.bin"), &bytes)?;
    record(
        "prove",
        &json!({"name": name, "wall_s": secs, "segments": s.segments, "user_cycles": s.user_cycles,
                             "total_cycles": s.total_cycles, "paging_cycles": s.paging_cycles,
                             "reserved_cycles": s.reserved_cycles, "receipt_bytes": bytes.len(), "loadavg": loadavg()}),
    )?;
    Ok(())
}

fn load_receipt(name: &str, file: &str) -> Result<Receipt> {
    Ok(bincode::deserialize(&fs::read(root().join("artifacts").join(name).join(file))?)?)
}

fn cmd_groth16(name: &str) -> Result<()> {
    refuse_dev_mode()?;
    let (_, meta) = fixture(name)?;
    let (_, id) = guest_elf(meta["kind"].as_str().unwrap_or(""))?;
    let succinct = load_receipt(name, "receipt-succinct.bin")?;
    println!("[{name}] load before: {}", loadavg());
    let start = Instant::now();
    let receipt = default_prover().compress(&ProverOpts::groth16(), &succinct)?;
    let secs = start.elapsed().as_secs_f64();
    println!("[{name}] load after:  {}", loadavg());
    ensure!(matches!(receipt.inner, InnerReceipt::Groth16(_)), "expected a Groth16 receipt");
    receipt.verify(id)?;
    let g = receipt.inner.groth16()?;
    let seal = [&g.verifier_parameters.as_bytes()[..4], g.seal.as_slice()].concat();
    let bytes = bincode::serialize(&receipt)?;
    println!(
        "[{name}] identity_p254 + Groth16 (Docker): wall {secs:.2} s | receipt {} bytes | on-chain seal {} bytes, selector {}",
        bytes.len(),
        seal.len(),
        hx(&seal[..4])
    );
    let dir = artifacts(name)?;
    fs::write(dir.join("receipt-groth16.bin"), &bytes)?;
    fs::write(dir.join("seal.hex"), hx(&seal) + "\n")?;
    fs::write(dir.join("journal.hex"), hx(&receipt.journal.bytes) + "\n")?;
    record(
        "groth16",
        &json!({"name": name, "wall_s": secs, "seal_bytes": seal.len(), "selector": hx(&seal[..4]),
                               "loadavg": loadavg()}),
    )?;
    Ok(())
}

fn cmd_verify(name: &str) -> Result<()> {
    let (_, meta) = fixture(name)?;
    let (_, id) = guest_elf(meta["kind"].as_str().unwrap_or(""))?;
    for file in ["receipt-succinct.bin", "receipt-groth16.bin"] {
        match load_receipt(name, file) {
            Ok(r) => {
                r.verify(id).with_context(|| file.to_string())?;
                ensure!(hex::encode(&r.journal.bytes) == meta["journal"].as_str().unwrap(), "{file}: journal");
                println!("[{name}] {file}: verifies; journal == native");
            }
            Err(e) => println!("[{name}] {file}: {e}"),
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| args.get(i).map(String::as_str).context("missing argument");
    match args.first().map(String::as_str) {
        Some("ids") => {
            cmd_ids();
            Ok(())
        }
        Some("micro") if args.iter().any(|a| a == "--plain") => cmd_micro_plain(),
        Some("micro") => cmd_micro(),
        Some("bench") => cmd_bench(&args[1..]),
        Some("compose") => cmd_compose(args.iter().any(|a| a == "--real")),
        Some("prove") => cmd_prove(arg(1)?),
        Some("groth16") => cmd_groth16(arg(1)?),
        Some("verify") => cmd_verify(arg(1)?),
        _ => bail!("usage: host ids | micro | bench [name…] | compose [--real] | prove <name> | groth16 <name> | verify <name>"),
    }
}
