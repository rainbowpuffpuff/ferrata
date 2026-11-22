//! RFP-021 P.2, with RFP-020's method: the RISC Zero proof time and proof size of every program
//! action, advancing the tracked state and each type of statement. LEZ v0.4 executes public
//! transactions without proving them; what is proven here is the same guest execution a prover of
//! the transaction would have to prove, input for input (the guest, its `ReceiveInput` and the
//! session limit `lee`'s public backend gives it, captured by `trace::TAPE`).
//!
//! * `perf_prove --dump DIR [--with-verify-all]`: run the scenarios of `lez/results/stage1-*.md`
//!   and `stage4-attest.md` (dev mode is not needed: public execution is never proven), keep the
//!   transitions of the actions measured, and write each one's input (`NNN.input`, borsh
//!   `ReceiveInput`), the executor's journal (`NNN.journal`), the guests (`elf-<image id>.bin`) and
//!   `index.tsv`.
//! * `perf_prove --prove DIR N`: prove transition `N` for real (`RISC0_DEV_MODE` must be unset or
//!   0): a composite receipt, then compressed to a succinct one; both verified against the image
//!   id, the journal compared with the executor's. Prints one `RESULT` line (tab-separated).
//!
//! * `perf_prove --exec DIR N [RUNS]`: execute transition `N` with the executor only (no proof),
//!   RUNS times (default 5); prints one `EXEC` line: user cycles and the median wall time (the
//!   executor's throughput, for the precompile model of RFP-021 P.8).
//!
//! `bench/perf/prove-actions.sh` runs both on the build box and `bench/perf/summarize_proofs.py` writes
//! `lez/results/perf-proofs.md`.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Instant,
};

use harness::{
    attest, scenarios,
    trace::{Taped, TAPE},
    TxReport,
};
use lee_core::program::ReceiveInput;
use risc0_zkvm::{compute_image_id, default_executor, default_prover, ExecutorEnv, ProverOpts};

fn usage() -> ! {
    eprintln!("usage: perf_prove --dump DIR [--with-verify-all] | --prove DIR N | --exec DIR N [RUNS]");
    std::process::exit(2)
}

/// Run `f` with the tape on; return its value and every transition traced meanwhile.
fn taped<T>(f: impl FnOnce() -> T) -> (T, Vec<Taped>) {
    *TAPE.lock().unwrap() = Some(Vec::new());
    let out = f();
    let tape = TAPE.lock().unwrap().take().expect("the tape was on");
    (out, tape)
}

/// Each report with its own taped transitions (the tape holds them in submission order).
fn by_report<'a>(reports: &'a [TxReport], tape: &'a [Taped]) -> Vec<(&'a TxReport, &'a [Taped])> {
    let mut at = 0;
    let mut out = Vec::new();
    for r in reports {
        let n = r.transitions.len();
        let mine = &tape[at..at + n];
        for (t, k) in r.transitions.iter().zip(mine) {
            assert_eq!(
                (t.handler.as_str(), t.cycles),
                (k.handler.as_str(), k.cycles),
                "{}: tape and trace agree",
                r.label
            );
        }
        out.push((r, mine));
        at += n;
    }
    assert_eq!(at, tape.len(), "every taped transition belongs to a report");
    out
}

struct Row {
    scenario: &'static str,
    action: String,
    tx_label: String,
    gas_exec: u64,
    gas_stor: u64,
    position: usize,
    taped: Taped,
}

fn keep(
    rows: &mut Vec<Row>,
    scenario: &'static str,
    pairs: &[(&TxReport, &[Taped])],
    pick: impl Fn(&str) -> Option<String>,
) {
    for (r, taped) in pairs {
        let Some(action) = pick(&r.label) else { continue };
        assert!(r.ok(), "{}: the measured action is applied", r.label);
        for (i, t) in taped.iter().enumerate() {
            rows.push(Row {
                scenario,
                action: action.clone(),
                tx_label: r.label.clone(),
                gas_exec: r.charge.gas_exec,
                gas_stor: r.charge.gas_stor,
                position: i + 1,
                taped: t.clone(),
            });
        }
    }
}

fn dump(dir: &Path, with_verify_all: bool) {
    fs::create_dir_all(dir).unwrap();
    let mut rows = Vec::new();

    // Bootstrap and a committee handoff: Init in two adoption parts, then mainnet update_0, which
    // rotates to and adopts the next committee (five transactions; stage1-replay.md).
    let (w, tape) = taped(|| scenarios::replay(2));
    let pairs = by_report(&w.chain.reports, &tape);
    keep(&mut rows, "replay", &pairs, |l| {
        if let Some(rest) = l.strip_prefix("Init ") {
            Some(format!("Init (bootstrap): {rest}"))
        } else {
            l.strip_prefix("update_0 ").map(|rest| format!("committee handoff (update_0): {rest}"))
        }
    });

    // A finality update: three transactions, no adoption (stage1-finality.md).
    let (w, tape) = taped(scenarios::finality_path);
    let pairs = by_report(&w.chain.reports, &tape);
    keep(&mut rows, "finality_path", &pairs, |l| {
        l.strip_prefix("update_1 as finality update ").map(|rest| format!("finality update: {rest}"))
    });

    // Every statement type of stage4-attest.md, one transaction each.
    let ((s, out), tape) = taped(attest::accepted);
    let labels: Vec<String> = out.iter().map(|r| r.label.clone()).collect();
    let pairs = by_report(&s.w.chain.reports, &tape);
    keep(&mut rows, "accepted", &pairs, |l| labels.iter().any(|x| x == l).then(|| format!("statement: {l}")));

    // The reference consumer's claim (Claim -> Attest -> reply -> Credit) and a public caller's Attest.
    let ((s, _), tape) = taped(attest::consumer_path);
    let pairs = by_report(&s.w.chain.reports, &tape);
    keep(&mut rows, "consumer_path", &pairs, |l| {
        (l.starts_with("reference consumer: claim an ERC-20 Transfer") || l.starts_with("a public actor calls Attest"))
            .then(|| format!("consumer: {l}"))
    });

    // Instance succession (lez/results/a1-successors-ws.md): `Succeed` at a parent, which `Spawn`s the successor.
    let (w, tape) = taped(succession);
    let pairs = by_report(&w.chain.reports, &tape);
    keep(&mut rows, "succession", &pairs, |l| {
        (l == "Succeed").then(|| "instance succession: Succeed (then Spawn at the successor)".to_string())
    });

    if with_verify_all {
        // The whole finality update in one public transaction (VerifyAll), stage1-one-transaction.md.
        let (reports, tape) = taped(scenarios::one_transaction_update);
        let all: Vec<TxReport> = reports;
        let n: usize = all.iter().map(|r| r.transitions.len()).sum();
        let tail = &tape[tape.len() - n..];
        let pairs = by_report(&all, tail);
        keep(&mut rows, "one_transaction", &pairs, |l| {
            l.contains("VerifyAll").then(|| format!("finality update in one transaction: {l}"))
        });
    }

    let mut index = fs::File::create(dir.join("index.tsv")).unwrap();
    writeln!(
        index,
        "n\tscenario\taction\ttx_label\tposition\thandler\timage_id\tsession_limit\texec_cycles\tinput_bytes\tjournal_bytes\ttx_gas_exec\ttx_gas_stor"
    )
    .unwrap();
    for (n, row) in rows.iter().enumerate() {
        let n = n + 1;
        let id = compute_image_id(&row.taped.elf).unwrap();
        let id_hex = hex::encode(id.as_bytes());
        let elf_path = dir.join(format!("elf-{id_hex}.bin"));
        if !elf_path.exists() {
            fs::write(&elf_path, &row.taped.elf).unwrap();
        }
        let input = borsh::to_vec(&row.taped.input).unwrap();
        fs::write(dir.join(format!("{n:03}.input")), &input).unwrap();
        fs::write(dir.join(format!("{n:03}.journal")), &row.taped.journal).unwrap();
        writeln!(
            index,
            "{n}\t{}\t{}\t{}\t{}\t{}\t{id_hex}\t{}\t{}\t{}\t{}\t{}\t{}",
            row.scenario,
            row.action,
            row.tx_label,
            row.position,
            row.taped.handler,
            row.taped.session_limit,
            row.taped.cycles,
            input.len(),
            row.taped.journal.len(),
            row.gas_exec,
            row.gas_stor
        )
        .unwrap();
    }
    println!("{} transitions written to {} (index.tsv)", rows.len(), dir.display());
}

/// The accepted `Succeed` of `tests/successor.rs`, alone: a parent initialised with Sepolia's schedule before Gloas was
/// scheduled follows period 1378; then `Succeed` names a successor with the schedule that has Gloas, at the parent's
/// latest record, and the successor's `Spawn` creates it.
fn succession() -> scenarios::World {
    use ferrata_core::fork::{Fork, FAR_FUTURE_EPOCH};
    use ferrata_lez_wire::{instance_actor, successor_id, Message};
    use harness::sepolia::{self, FIRST_PERIOD, GLOAS_PERIOD};
    let mut parent_config = sepolia::sepolia_config(2);
    parent_config.fork_epochs[Fork::Gloas as usize] = FAR_FUTURE_EPOCH;
    let mut w = sepolia::sepolia_world(parent_config);
    let u1378 = sepolia::sepolia_update(FIRST_PERIOD);
    let run = w.apply(&u1378, &format!("parent: period {FIRST_PERIOD}"));
    assert!(run.reports.iter().all(TxReport::ok));
    let checkpoint = w.inst.store(&w.chain).finalized.hash_tree_root();
    let now = w.inst.config.slot_start_ms(sepolia::sepolia_update(GLOAS_PERIOD).signature_slot) + 12_000;
    let config = sepolia::sepolia_config(2);
    let root_id = w.inst.state(&w.chain).unwrap().root_id;
    let successor = instance_actor(w.inst.program, &successor_id(&root_id, &w.inst.id, &config, &checkpoint));
    let message = borsh::to_vec(&Message::Succeed { config, checkpoint }).unwrap();
    let r = w.chain.submit("Succeed", w.inst.actor(), message, vec![w.inst.actor(), successor], vec![], now);
    assert!(r.ok(), "Succeed: {:?}", r.charge.revert);
    w
}

fn field(line: &str, i: usize) -> &str {
    line.split('\t').nth(i).unwrap_or_else(|| panic!("index.tsv: no column {i}"))
}

fn prove(dir: &Path, n: usize) {
    let dev = std::env::var("RISC0_DEV_MODE").is_ok_and(|v| !v.is_empty() && v != "0" && v != "false");
    if dev {
        eprintln!("RISC0_DEV_MODE is set: the receipts would be fake. Unset it.");
        std::process::exit(2);
    }
    let index = fs::read_to_string(dir.join("index.tsv")).unwrap();
    let line = index.lines().skip(1).find(|l| field(l, 0) == n.to_string()).expect("no such transition");
    let (image_hex, limit, exec_cycles) =
        (field(line, 6), field(line, 7).parse::<u64>().unwrap(), field(line, 8).parse::<u64>().unwrap());
    let elf = fs::read(dir.join(format!("elf-{image_hex}.bin"))).unwrap();
    let input: ReceiveInput = borsh::from_slice(&fs::read(dir.join(format!("{n:03}.input"))).unwrap()).unwrap();
    let journal = fs::read(dir.join(format!("{n:03}.journal"))).unwrap();
    let image_id = compute_image_id(&elf).unwrap();
    assert_eq!(hex::encode(image_id.as_bytes()), image_hex);

    let mut builder = ExecutorEnv::builder();
    builder.session_limit(Some(limit));
    lee::program::Program::write_receive_input(&input, &mut builder).unwrap();
    let env = builder.build().unwrap();
    let prover = default_prover();
    let start = Instant::now();
    let info = prover.prove_with_opts(env, &elf, &ProverOpts::composite()).expect("proving succeeds");
    let composite_s = start.elapsed().as_secs_f64();
    info.receipt.verify(image_id).expect("the composite receipt verifies against the image id");
    assert_eq!(info.receipt.journal.bytes, journal, "the proven journal is the executor's");
    assert_eq!(info.stats.user_cycles, exec_cycles, "the proof covers the cycles LEZ metered");
    let composite_bytes = borsh::to_vec(&info.receipt).unwrap().len();

    let start = Instant::now();
    let succinct = prover.compress(&ProverOpts::succinct(), &info.receipt).expect("compression succeeds");
    let succinct_s = start.elapsed().as_secs_f64();
    let start = Instant::now();
    succinct.verify(image_id).expect("the succinct receipt verifies against the image id");
    let verify_ms = start.elapsed().as_secs_f64() * 1000.0;
    assert!(matches!(succinct.inner, risc0_zkvm::InnerReceipt::Succinct(_)), "a succinct receipt");
    let succinct_inner_bytes = borsh::to_vec(&succinct.inner).unwrap().len();
    let succinct_bytes = borsh::to_vec(&succinct).unwrap().len();
    println!("image {image_hex} ({})", field(line, 5));
    println!(
        "RESULT\t{n}\t{}\t{}\t{}\t{composite_s:.2}\t{composite_bytes}\t{succinct_s:.2}\t{succinct_inner_bytes}\t{succinct_bytes}\t{verify_ms:.1}",
        info.stats.user_cycles, info.stats.total_cycles, info.stats.segments
    );
}

/// The guest, the input and the session limit of transition `n` of a dump.
fn load(dir: &Path, n: usize) -> (String, Vec<u8>, ReceiveInput, u64, u64) {
    let index = fs::read_to_string(dir.join("index.tsv")).unwrap();
    let line = index.lines().skip(1).find(|l| field(l, 0) == n.to_string()).expect("no such transition");
    let elf = fs::read(dir.join(format!("elf-{}.bin", field(line, 6)))).unwrap();
    let input: ReceiveInput = borsh::from_slice(&fs::read(dir.join(format!("{n:03}.input"))).unwrap()).unwrap();
    (line.to_string(), elf, input, field(line, 7).parse().unwrap(), field(line, 8).parse().unwrap())
}

fn exec(dir: &Path, n: usize, runs: usize) {
    let (line, elf, input, limit, exec_cycles) = load(dir, n);
    let mut times = Vec::new();
    for _ in 0..runs {
        let mut builder = ExecutorEnv::builder();
        builder.session_limit(Some(limit));
        lee::program::Program::write_receive_input(&input, &mut builder).unwrap();
        let env = builder.build().unwrap();
        let start = Instant::now();
        let info = default_executor().execute(env, &elf).expect("executes");
        times.push(start.elapsed().as_secs_f64());
        assert_eq!(info.cycles(), exec_cycles, "the same cycles as LEZ metered");
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = times[times.len() / 2];
    println!("EXEC\t{n}\t{}\t{exec_cycles}\t{median:.4}\t{:.2}", field(&line, 5), exec_cycles as f64 / median / 1e6);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--dump") => {
            let dir = PathBuf::from(args.get(1).unwrap_or_else(|| usage()));
            dump(&dir, args.iter().any(|a| a == "--with-verify-all"));
        }
        Some("--prove") => {
            let dir = PathBuf::from(args.get(1).unwrap_or_else(|| usage()));
            let n = args.get(2).and_then(|n| n.parse().ok()).unwrap_or_else(|| usage());
            prove(&dir, n);
        }
        Some("--exec") => {
            let dir = PathBuf::from(args.get(1).unwrap_or_else(|| usage()));
            let n = args.get(2).and_then(|n| n.parse().ok()).unwrap_or_else(|| usage());
            let runs = args.get(3).and_then(|n| n.parse().ok()).unwrap_or(5);
            exec(&dir, n, runs);
        }
        _ => usage(),
    }
}
