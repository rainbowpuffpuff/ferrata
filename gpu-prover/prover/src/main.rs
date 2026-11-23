//! Proves ferrata-checkpoint fixtures with an uploaded guest program (on a GPU with `--features cuda`)
//! and verifies receipts. The guest is never rebuilt here: the program binary is the `.bin` that
//! risc0-build wrote in `checkpoint/target/riscv-guest/…`, i.e. the bytes the
//! ferrata-checkpoint host embeds, so the image id is the local build's.
//!
//! ```text
//! ferrata-modal image-id <program.bin>…
//!     image id, size and sha256 of each guest program binary
//! ferrata-modal prove <name> <program.bin> <fixture.input> <fixture.json> <out-dir>   (--features cuda)
//!     execute, then prove a succinct receipt in-process; writes <out-dir>/receipt-succinct.bin,
//!     journal.hex and prove.json, and prints prove.json as one line
//! ferrata-modal verify <receipt.bin> <image id, hex> <fixture.json>
//!     the receipt must be a real (succinct or Groth16) proof, verify against the image id, and carry
//!     the journal of the native run (the fixture's "journal")
//! ```
//!
//! `prove` is what `default_prover().prove_with_opts(env, elf, &ProverOpts::succinct())` does with the
//! in-process prover (`RISC0_PROVER=local`): `LocalProver::prove_with_ctx` calls
//! `get_prover_server(opts)?.prove_with_ctx(env, ctx, elf)`, which is `ExecutorImpl::from_elf(env,
//! elf)?.run()` followed by `prove_session(ctx, &session)` (risc0-zkvm 3.0.5,
//! `host/client/prove/local.rs`, `host/server/prove/prover_impl.rs`). The two steps are called here
//! one after the other so that execution, each segment's proof and the recursion (lift, join, final
//! check) can be timed separately. The input is written exactly as the ferrata-checkpoint host does:
//! a little-endian u32 length, then the bytes.

use std::fs;
use std::path::Path;

use anyhow::{bail, ensure, Context as _, Result};
use risc0_zkvm::sha::Digestible as _;
use risc0_zkvm::{compute_image_id, Digest, InnerReceipt, Receipt};
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn image_hex(id: &Digest) -> String {
    format!("0x{}", hex::encode(id.as_bytes()))
}

fn parse_image_id(text: &str) -> Result<Digest> {
    let bytes = hex::decode(text.trim().trim_start_matches("0x")).context("image id: hex")?;
    let words: [u8; 32] = bytes.try_into().map_err(|_| anyhow::anyhow!("image id: 32 bytes expected"))?;
    Ok(Digest::from_bytes(words))
}

fn dev_mode() -> bool {
    std::env::var("RISC0_DEV_MODE").map(|v| !v.is_empty() && v != "0" && v != "false").unwrap_or(false)
}

fn native_journal(meta_path: &Path) -> Result<String> {
    let meta: Value = serde_json::from_slice(&fs::read(meta_path)?).context("fixture json")?;
    Ok(meta["journal"].as_str().context("fixture json: no \"journal\"")?.to_string())
}

fn cmd_image_id(paths: &[String]) -> Result<()> {
    ensure!(!paths.is_empty(), "usage: ferrata-modal image-id <program.bin>…");
    for path in paths {
        let program = fs::read(path).with_context(|| path.clone())?;
        let id = compute_image_id(&program)?;
        println!("{}", json!({"program": path, "image_id": image_hex(&id), "bytes": program.len(), "sha256": sha256_hex(&program)}));
    }
    Ok(())
}

fn receipt_kind(receipt: &Receipt) -> &'static str {
    match &receipt.inner {
        InnerReceipt::Succinct(_) => "succinct",
        InnerReceipt::Groth16(_) => "groth16",
        InnerReceipt::Composite(_) => "composite",
        InnerReceipt::Fake(_) => "fake",
        _ => "other",
    }
}

fn cmd_verify(receipt_path: &str, image_id: &str, meta_path: &str) -> Result<()> {
    ensure!(!dev_mode(), "RISC0_DEV_MODE is set: refusing to verify (fake receipts would pass)");
    let bytes = fs::read(receipt_path).with_context(|| receipt_path.to_string())?;
    let receipt: Receipt = bincode::deserialize(&bytes).context("receipt: bincode")?;
    let kind = receipt_kind(&receipt);
    ensure!(kind == "succinct" || kind == "groth16", "{receipt_path}: a {kind} receipt is not a real proof here");
    let id = parse_image_id(image_id)?;
    receipt.verify(id).with_context(|| format!("{receipt_path}: does not verify against {}", image_hex(&id)))?;
    let journal = hex::encode(&receipt.journal.bytes);
    ensure!(journal == native_journal(Path::new(meta_path))?, "{receipt_path}: journal differs from the native run");
    let claim = receipt.claim()?.value()?;
    let pre = claim.pre.digest();
    ensure!(pre == id, "claim pre-state digest differs from the image id");
    println!(
        "{}",
        json!({"receipt": receipt_path, "kind": kind, "bytes": bytes.len(), "image_id": image_hex(&id),
               "verifies": true, "exit_code": format!("{:?}", claim.exit_code),
               "journal_bytes": receipt.journal.bytes.len(), "journal_sha256": sha256_hex(&receipt.journal.bytes),
               "journal_equals_native": true})
    );
    Ok(())
}

#[cfg(any(feature = "cuda", feature = "prove"))]
mod prove {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Instant;

    use risc0_zkvm::{
        get_prover_server, ExecutorEnv, ExecutorImpl, ProverOpts, Segment, SessionEvents, VerifierContext,
    };

    use super::*;

    #[derive(Default)]
    struct SegmentTimes {
        started: Option<Instant>,
        done: Vec<Value>,
    }

    /// Times each segment's proof (the session calls it just before and after `prove_segment`).
    struct Timer(Rc<RefCell<SegmentTimes>>);

    impl SessionEvents for Timer {
        fn on_pre_prove_segment(&self, _segment: &Segment) {
            self.0.borrow_mut().started = Some(Instant::now());
        }
        fn on_post_prove_segment(&self, segment: &Segment) {
            let mut times = self.0.borrow_mut();
            let secs = times.started.take().map(|t| t.elapsed().as_secs_f64());
            times.done.push(json!({"index": segment.index, "po2": segment.po2(), "prove_s": secs}));
        }
    }

    /// `groth16`: after the succinct receipt, the Groth16 receipt the vault takes, made here as well (identity_p254 and,
    /// with the `cuda` feature, RISC Zero's CUDA Groth16 prover with the rzup component `risc0-groth16` 0.2.0, the
    /// trusted setup of RISC Zero 3.1; without it, risc0-groth16's Docker prover image), written as receipt-groth16.bin
    /// and seal.hex (selector ‖ proof, as `RiscZeroGroth16Verifier.verify` takes it).
    pub fn cmd_prove(name: &str, program_path: &str, input_path: &str, meta_path: &str, out: &str, groth16: bool) -> Result<()> {
        ensure!(!dev_mode(), "RISC0_DEV_MODE is set: receipts would be fake");
        let program = fs::read(program_path).with_context(|| program_path.to_string())?;
        let input = fs::read(input_path).with_context(|| input_path.to_string())?;
        let want = native_journal(Path::new(meta_path))?;
        // A checkpoint's input is framed as its guest reads it (a little-endian u32 length, then the bytes); a
        // release's is written as is (bridge/e2e `job`).
        let meta: Value = serde_json::from_str(&fs::read_to_string(meta_path)?)?;
        let framed = meta["kind"].as_str() != Some("release");
        let id = compute_image_id(&program)?;
        println!("[{name}] image id {} (program {} bytes, sha256 {})", image_hex(&id), program.len(), sha256_hex(&program));

        let len = u32::try_from(input.len())?.to_le_bytes();
        let mut builder = ExecutorEnv::builder();
        if framed {
            builder.write_slice(&len);
        }
        let env = builder.write_slice(&input).build()?;
        let start = Instant::now();
        let mut session = ExecutorImpl::from_elf(env, &program)?.run()?;
        let exec_s = start.elapsed().as_secs_f64();
        let journal = session.journal.as_ref().map(|j| hex::encode(&j.bytes)).unwrap_or_default();
        ensure!(journal == want, "[{name}] guest journal differs from the native run");
        println!(
            "[{name}] executed: {} segments, {} user cycles, {exec_s:.2} s; journal == native",
            session.segments.len(),
            session.user_cycles
        );

        let times = Rc::new(RefCell::new(SegmentTimes::default()));
        session.add_hook(Timer(times.clone()));
        let prover = get_prover_server(&ProverOpts::succinct())?;
        let proving = Instant::now();
        let info = prover.prove_session(&VerifierContext::default(), &session)?;
        let prove_s = proving.elapsed().as_secs_f64();
        let total_s = start.elapsed().as_secs_f64();
        drop(session);

        let receipt = info.receipt;
        ensure!(matches!(receipt.inner, InnerReceipt::Succinct(_)), "expected a succinct receipt");
        receipt.verify(id).context("receipt does not verify against the image id")?;
        ensure!(hex::encode(&receipt.journal.bytes) == want, "receipt journal differs from the native run");
        let bytes = bincode::serialize(&receipt)?;
        let dir = Path::new(out);
        fs::create_dir_all(dir)?;
        fs::write(dir.join("receipt-succinct.bin"), &bytes)?;
        fs::write(dir.join("journal.hex"), format!("0x{}\n", hex::encode(&receipt.journal.bytes)))?;

        let times = times.borrow();
        let segments_s: f64 = times.done.iter().filter_map(|s| s["prove_s"].as_f64()).sum();
        let s = &info.stats;
        let record = json!({
            "name": name, "image_id": image_hex(&id), "program_bytes": program.len(),
            "program_sha256": sha256_hex(&program), "input_bytes": input.len(), "input_sha256": sha256_hex(&input),
            "risc0_zkvm": risc0_zkvm::VERSION, "prover": if cfg!(feature = "cuda") { "in-process (LocalProver path), risc0-zkvm feature cuda" } else { "in-process (LocalProver path), risc0-zkvm feature prove (CPU)" },
            "receipt_kind": "succinct", "receipt_bytes": bytes.len(), "receipt_sha256": sha256_hex(&bytes),
            "verifies": true, "journal_equals_native": true,
            "segments": s.segments, "user_cycles": s.user_cycles, "total_cycles": s.total_cycles,
            "paging_cycles": s.paging_cycles, "reserved_cycles": s.reserved_cycles,
            "execute_s": exec_s, "prove_session_s": prove_s, "segment_proofs_s": segments_s,
            "recursion_s": prove_s - segments_s, "total_s": total_s, "segment_times": times.done,
        });
        fs::write(dir.join("prove.json"), serde_json::to_string_pretty(&record)? + "\n")?;
        println!(
            "[{name}] succinct proof: total {total_s:.2} s (execute {exec_s:.2} s, {} segment proofs {segments_s:.2} s, \
             lift/join/check {:.2} s) | receipt {} bytes | verifies; journal == native",
            s.segments,
            prove_s - segments_s,
            bytes.len()
        );
        println!("{record}");
        if groth16 {
            let t = Instant::now();
            let wrapped = prover.compress(&ProverOpts::groth16(), &receipt)?;
            let wrap_s = t.elapsed().as_secs_f64();
            let g = wrapped.inner.groth16().context("not a Groth16 receipt")?;
            wrapped.verify(id).context("the Groth16 receipt does not verify against the image id")?;
            ensure!(wrapped.journal.bytes == receipt.journal.bytes, "the journal changed in the wrap");
            let seal = [&g.verifier_parameters.as_bytes()[..4], g.seal.as_slice()].concat();
            ensure!(seal.len() == 260, "seal of {} bytes", seal.len());
            let gbytes = bincode::serialize(&wrapped)?;
            fs::write(dir.join("receipt-groth16.bin"), &gbytes)?;
            fs::write(dir.join("seal.hex"), format!("0x{}\n", hex::encode(&seal)))?;
            let g16 = json!({
                "name": name, "image_id": image_hex(&id), "selector": format!("0x{}", hex::encode(&seal[..4])),
                "seal_bytes": seal.len(), "receipt_bytes": gbytes.len(), "verifies": true,
                "wrap_s": wrap_s, "prover": if cfg!(feature = "cuda") { "identity_p254 and Groth16 on the GPU (risc0-groth16 cuda)" } else { "identity_p254 on the CPU, Groth16 in RISC Zero's Docker image" },
            });
            fs::write(dir.join("groth16.json"), serde_json::to_string_pretty(&g16)? + "\n")?;
            println!("[{name}] Groth16: {wrap_s:.2} s, selector 0x{}, verifies", hex::encode(&seal[..4]));
            println!("{g16}");
        }
        Ok(())
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| args.get(i).map(String::as_str).context("missing argument");
    match args.first().map(String::as_str) {
        Some("image-id") => cmd_image_id(&args[1..]),
        Some("verify") => cmd_verify(arg(1)?, arg(2)?, arg(3)?),
        #[cfg(any(feature = "cuda", feature = "prove"))]
        Some("prove") => prove::cmd_prove(arg(1)?, arg(2)?, arg(3)?, arg(4)?, arg(5)?, args.get(6).map(String::as_str) == Some("--groth16")),
        #[cfg(not(any(feature = "cuda", feature = "prove")))]
        Some("prove") => bail!("built without the `cuda` or `prove` feature: no prover"),
        _ => bail!("usage: ferrata-modal image-id <program.bin>… | prove <name> <program.bin> <input> <fixture.json> <out-dir> [--groth16] | verify <receipt.bin> <image-id> <fixture.json>"),
    }
}
