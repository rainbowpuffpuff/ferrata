//! `ferrata-attest`: the attestation side's command-line tool (milestone A3), on the harness.
//!
//! It follows Sepolia with the attestation program, then attests a storage slot, an account, a
//! log or a whole receipt of a finalized Sepolia block, end to end: the request is built from
//! recorded `eth_getProof` and `eth_getBlockReceipts` responses, the program's verdict is
//! computed with the function the program runs, and the transaction is priced by LEZ's own
//! settlement on a scratch copy of the zone's state. Nothing is sent unless `--send` is given;
//! with it, the transaction is settled and must match the dry run byte for byte.
//!
//! The zone is the harness's (LEZ v0.4.0-rc1's state transition in process, the guests in RISC
//! Zero dev mode); the data is the fixtures `sepolia-gloas-2026-11-10` (light-client updates of
//! periods 1378-1409, through the Gloas fork) and `sepolia-attest-2026-11-10` (a finality update
//! of period 1409, the block it finalizes, its receipts and two accounts' proofs), all real
//! Sepolia data checked against pinned SHA-256 sums. See `lez/ATTEST-CLI.md`.
//!
//! ```text
//! RISC0_DEV_MODE=1 ferrata-attest [--message-out DIR] COMMAND [ARGS] [--send] [--then COMMAND ...]
//!   status                                  the instance, its record, what following cost
//!   slot    [--address A] [--slot S] [--expect-eq V]   a storage slot (default: USDC totalSupply_)
//!   account [--address A] [--expect-code-hash H]       an account (default: Multicall3)
//!   log     [--tx I --log J | --block-log-index K]     a log (default: the block's first Transfer)
//!   receipt --tx I                          every log of transaction I's receipt
//! ```
//! Exit status: 0 attested (or would be), 3 refused as the dry run predicted, 1 the dry run and
//! the execution disagree, 2 a usage error.

use std::{collections::BTreeMap, path::PathBuf, process::ExitCode};

use fee_core::{market, BlockFeeSummary};
use ferrata_core::exec::locate_log;
use ferrata_lez_wire::{
    attest::{AccountQuery, Attestation, Cmp, Condition, Fact, Field, LogQuery, Request},
    code, Message, ATTESTED_SELECTOR,
};
use ferrata_spectests::execkit::AccountProof;
use harness::{
    sepolia::{Stage5, FIRST_PERIOD, LAST_PERIOD, SEPOLIA_CHAIN_ID},
    settle::{classify, opening_fee_state, settle_charged_transaction, Charge, LeeTransaction},
    trace::trace_public,
};
use lee::{
    public_transaction::{Message as PublicMessage, WitnessSet},
    AccountId, Actor, FeeDeclaration, PrivateKey, PublicKey, PublicTransaction,
};

type Word = [u8; 32];
type Address = [u8; 20];

#[derive(Debug)]
enum Op {
    Status,
    Slot { address: Option<Address>, slot: Option<Word>, expect_eq: Option<Word> },
    Account { address: Option<Address>, expect_code_hash: Option<Word> },
    Log { tx: Option<u64>, log: Option<u64>, block_log_index: Option<u64> },
    Receipt { tx: u64 },
}

#[derive(Debug)]
struct Command {
    op: Op,
    send: bool,
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: ferrata-attest [--message-out DIR] COMMAND [ARGS] [--send] [--then COMMAND ...]\n\
         commands: status | slot [--address A] [--slot S] [--expect-eq V] | account [--address A] [--expect-code-hash H]\n\
         \x20         | log [--tx I --log J | --block-log-index K] | receipt --tx I\n\
         Without --send nothing is sent: the dry run prints the message, the program's verdict and LEZ's charge."
    );
    ExitCode::from(2)
}

fn hex_bytes(s: &str) -> Result<Vec<u8>, String> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    let s = if s.len() % 2 == 1 { format!("0{s}") } else { s.to_string() };
    hex::decode(&s).map_err(|e| format!("{s}: {e}"))
}

fn word_arg(s: &str) -> Result<Word, String> {
    let b = hex_bytes(s)?;
    if b.len() > 32 {
        return Err(format!("{s}: more than 32 bytes"));
    }
    let mut w = [0u8; 32];
    w[32 - b.len()..].copy_from_slice(&b);
    Ok(w)
}

fn address_arg(s: &str) -> Result<Address, String> {
    hex_bytes(s)?.try_into().map_err(|_| format!("{s}: not a 20-byte address"))
}

fn u64_arg(s: &str) -> Result<u64, String> {
    s.replace([',', '_'], "").parse().map_err(|e| format!("{s}: {e}"))
}

fn parse(args: &[String]) -> Result<(Option<PathBuf>, Vec<Command>), String> {
    let mut args = args.to_vec();
    let mut message_out = None;
    if args.first().map(String::as_str) == Some("--message-out") {
        message_out = Some(PathBuf::from(args.get(1).ok_or("--message-out needs a directory")?));
        args.drain(..2);
    }
    let mut commands = Vec::new();
    for group in args.split(|a| a == "--then") {
        let (name, rest) = group.split_first().ok_or("an empty command")?;
        let mut send = false;
        let mut opts: BTreeMap<&str, &str> = BTreeMap::new();
        let mut i = 0;
        while i < rest.len() {
            match rest[i].as_str() {
                "--send" => send = true,
                "--dry-run" => {}
                flag if flag.starts_with("--") => {
                    let value = rest.get(i + 1).ok_or(format!("{flag} needs a value"))?;
                    opts.insert(flag, value.as_str());
                    i += 1;
                }
                other => return Err(format!("unexpected argument {other}")),
            }
            i += 1;
        }
        let get = |k: &str| opts.get(k).copied();
        let allowed: &[&str] = match name.as_str() {
            "status" => &[],
            "slot" => &["--address", "--slot", "--expect-eq"],
            "account" => &["--address", "--expect-code-hash"],
            "log" => &["--tx", "--log", "--block-log-index"],
            "receipt" => &["--tx"],
            other => return Err(format!("unknown command {other}")),
        };
        if let Some(k) = opts.keys().find(|k| !allowed.contains(k)) {
            return Err(format!("{name}: unknown option {k}"));
        }
        let op = match name.as_str() {
            "status" => Op::Status,
            "slot" => Op::Slot {
                address: get("--address").map(address_arg).transpose()?,
                slot: get("--slot").map(word_arg).transpose()?,
                expect_eq: get("--expect-eq").map(word_arg).transpose()?,
            },
            "account" => Op::Account {
                address: get("--address").map(address_arg).transpose()?,
                expect_code_hash: get("--expect-code-hash").map(word_arg).transpose()?,
            },
            "log" => {
                let op = Op::Log {
                    tx: get("--tx").map(u64_arg).transpose()?,
                    log: get("--log").map(u64_arg).transpose()?,
                    block_log_index: get("--block-log-index").map(u64_arg).transpose()?,
                };
                if let Op::Log { tx, log, block_log_index } = &op {
                    if tx.is_some() != log.is_some() || (tx.is_some() && block_log_index.is_some()) {
                        return Err("log: give --tx and --log together, or --block-log-index".into());
                    }
                }
                op
            }
            "receipt" => Op::Receipt { tx: u64_arg(get("--tx").ok_or("receipt: --tx is required")?)? },
            _ => unreachable!(),
        };
        if send && matches!(op, Op::Status) {
            return Err("status sends nothing".into());
        }
        commands.push(Command { op, send });
    }
    Ok((message_out, commands))
}

// ------------------------------------------------------------------------------------------
// Names of the program's codes, read from the IDL
// ------------------------------------------------------------------------------------------

fn code_names() -> BTreeMap<u32, (String, String)> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../idl/ferrata_attestation.idl.json");
    let mut out = BTreeMap::new();
    if let Ok(text) = std::fs::read_to_string(&path) {
        let idl: serde_json::Value = serde_json::from_str(&text).expect("the IDL is JSON");
        for e in idl["errors"].as_array().into_iter().flatten() {
            let msg = e["msg"].as_str().unwrap_or("").to_string();
            out.insert(e["code"].as_u64().unwrap() as u32, (e["name"].as_str().unwrap().to_string(), msg));
        }
    }
    // The program's own codes are also in the wire crate: they must agree with the IDL.
    for (name, c) in code::ALL {
        if let Some((n, _)) = out.get(&u32::from(*c)) {
            assert_eq!(n, name, "the IDL names code {c} {n}, the wire {name}");
        }
    }
    out
}

fn describe(code: u32, names: &BTreeMap<u32, (String, String)>) -> String {
    let owner = match code {
        1..=79 | 100..=199 => "the core",
        80..=99 => "the step API",
        200..=219 => "the attestation program",
        _ => "unassigned",
    };
    match names.get(&code) {
        Some((name, msg)) => format!("exit code {code} {name} ({owner}): {msg}"),
        None => format!("exit code {code} ({owner})"),
    }
}

// ------------------------------------------------------------------------------------------
// Requests from the recorded Sepolia data
// ------------------------------------------------------------------------------------------

fn h(b: &[u8]) -> String {
    format!("0x{}", hex::encode(b))
}

fn padded(a: &Address) -> Word {
    let mut w = [0u8; 32];
    w[12..].copy_from_slice(a);
    w
}

fn proofs(s: &Stage5) -> Vec<AccountProof> {
    vec![s.fx.storage_proof(), s.fx.same_address_proof()]
}

fn find_proof(s: &Stage5, address: &Address) -> Result<AccountProof, String> {
    proofs(s).into_iter().find(|p| &p.address == address).ok_or_else(|| {
        let have: Vec<String> = proofs(s).iter().map(|p| h(&p.address)).collect();
        format!("the recorded data holds eth_getProof for {} only, not {}", have.join(" and "), h(address))
    })
}

fn account_query(proof: &AccountProof, conditions: Vec<Condition>, slots: Vec<(Word, Vec<Condition>)>) -> Result<AccountQuery, String> {
    let mut queries = Vec::new();
    for (slot, conditions) in slots {
        let p = proof.storage.iter().find(|p| p.key == slot).ok_or_else(|| {
            let have: Vec<String> = proof.storage.iter().map(|p| h(&p.key)).collect();
            format!("the recorded proof of {} covers slots {} only", h(&proof.address), have.join(", "))
        })?;
        queries.push(ferrata_lez_wire::attest::SlotQuery { slot, proof: p.proof.clone(), conditions });
    }
    Ok(AccountQuery { address: proof.address, proof: proof.proof.clone(), conditions, slots: queries })
}

fn log_counts(s: &Stage5) -> Vec<u64> {
    s.fx.receipts.iter().map(|r| r["logs"].as_array().map_or(0, |l| l.len() as u64)).collect()
}

/// The request and a one-line description of what it asks.
fn request(s: &Stage5, op: &Op) -> Result<(Request, String), String> {
    let base = Request { instance_id: s.w.inst.id, chain_id: SEPOLIA_CHAIN_ID, header: Some(s.fx.header.clone()), ..Request::default() };
    match op {
        Op::Status => unreachable!(),
        Op::Slot { address, slot, expect_eq } => {
            let address = address.unwrap_or(s.fx.contract);
            let slot = slot.unwrap_or(s.fx.slot);
            let proof = find_proof(s, &address)?;
            let conditions = expect_eq.map(|v| vec![Condition { field: Field::Value, cmp: Cmp::Eq, rhs: v }]).unwrap_or_default();
            let what = format!(
                "storage slot {} of {}{}",
                h(&slot),
                h(&address),
                expect_eq.map(|v| format!(", condition value == {}", h(&v))).unwrap_or_default()
            );
            Ok((Request { accounts: vec![account_query(&proof, vec![], vec![(slot, conditions)])?], ..base }, what))
        }
        Op::Account { address, expect_code_hash } => {
            let address = address.unwrap_or(s.fx.same_address);
            let proof = find_proof(s, &address)?;
            let conditions =
                expect_code_hash.map(|v| vec![Condition { field: Field::CodeHash, cmp: Cmp::Eq, rhs: v }]).unwrap_or_default();
            let what = format!(
                "account {}{}",
                h(&address),
                expect_code_hash.map(|v| format!(", condition code hash == {}", h(&v))).unwrap_or_default()
            );
            Ok((Request { accounts: vec![account_query(&proof, conditions, vec![])?], ..base }, what))
        }
        Op::Log { tx, log, block_log_index } => {
            let (tx, log, conditions) = match (tx, log, block_log_index) {
                (Some(t), Some(l), None) => (*t, *l, vec![]),
                (None, None, Some(k)) => {
                    let (t, l) = locate_log(&log_counts(s), *k).ok_or(format!("the block has no log {k}"))?;
                    (t, l, vec![])
                }
                _ => {
                    let t = &s.fx.transfer;
                    let conditions = vec![
                        Condition { field: Field::Emitter, cmp: Cmp::Eq, rhs: padded(&t.token) },
                        Condition { field: Field::Topic(2), cmp: Cmp::Eq, rhs: padded(&t.to) },
                    ];
                    (t.tx_index, t.log_index, conditions)
                }
            };
            let receipts = s.fx.receipts.len() as u64;
            if tx >= receipts {
                return Err(format!("the block has {receipts} transactions"));
            }
            let what = format!(
                "log {log} of transaction {tx}'s receipt{}",
                if conditions.is_empty() { String::new() } else { " (the block's first ERC-20 Transfer: emitter and recipient checked)".into() }
            );
            let query = LogQuery { tx_index: tx, log_index: log, proof: s.fx.receipt_proof(tx), conditions };
            Ok((Request { logs: vec![query], ..base }, what))
        }
        Op::Receipt { tx } => {
            let counts = log_counts(s);
            let n = *counts.get(*tx as usize).ok_or(format!("the block has {} transactions", counts.len()))?;
            if n == 0 {
                return Err(format!("transaction {tx}'s receipt has no log: a receipt is attested through its logs"));
            }
            let proof = s.fx.receipt_proof(*tx);
            let logs = (0..n).map(|l| LogQuery { tx_index: *tx, log_index: l, proof: proof.clone(), conditions: vec![] }).collect();
            Ok((Request { logs, ..base }, format!("every log of transaction {tx}'s receipt ({n} logs, status 1 required)")))
        }
    }
}

// ------------------------------------------------------------------------------------------
// The dry run: LEZ's settlement on a scratch copy of the state
// ------------------------------------------------------------------------------------------

struct DryRun {
    tx_bytes: usize,
    cycles: u64,
    exit_code: Option<u32>,
    charge: Charge,
    events: Vec<(Vec<u8>, Vec<u8>)>,
}

fn dry_run(s: &Stage5, root: Actor, message: &[u8]) -> Result<DryRun, String> {
    let chain = &s.w.chain;
    // The harness's fee payer (`Chain::new`): its key is fixed, so the dry run signs as it would.
    let payer_key = PrivateKey::try_new([0x11; 32]).unwrap();
    if AccountId::from(&PublicKey::new_from_private_key(&payer_key)) != chain.payer {
        return Err("the harness's fee payer is not the expected key".into());
    }
    let gas_limit = market::MAX_GAS_EXEC;
    let fee = FeeDeclaration::new(chain.payer, gas_limit, 0, u128::MAX >> 8);
    let nonces = BTreeMap::from([(chain.payer, chain.state.get_account_by_id(chain.payer).nonce)]);
    let msg = PublicMessage::new(root, message.to_vec(), vec![root], nonces, Some(fee), vec![]);
    let witness_set = WitnessSet::for_message(&msg, &[&payer_key]);
    let tx = PublicTransaction::new(msg, witness_set);
    let (transitions, _) = trace_public(&tx, &chain.state, gas_limit);
    let lee_tx = LeeTransaction::Public(tx);
    let tx_bytes = borsh::to_vec(&lee_tx).map_err(|e| e.to_string())?.len();
    let view = classify(&lee_tx)?;
    let opening = opening_fee_state(&chain.state);
    let mut scratch = chain.state.clone();
    let mut summary = BlockFeeSummary::default();
    let (events, charge) =
        settle_charged_transaction(&lee_tx, &view, &mut scratch, &opening, chain.block_id, s.time, &mut summary)
            .map_err(|e| format!("LEZ would reject the transaction: {e}"))?;
    Ok(DryRun {
        tx_bytes,
        cycles: transitions.iter().map(|t| t.cycles).sum(),
        exit_code: transitions.iter().map(|t| t.exit_code).find(|c| *c != 0),
        charge,
        events: events.iter().map(|e| (e.event.selector.to_vec(), e.event.data.clone())).collect(),
    })
}

fn print_charge(prefix: &str, c: &Charge, tx_bytes: usize) {
    println!("{prefix}execution gas {} (cycles metered {}), storage gas {} (the transaction's {} bytes), base fee {}", c.gas_exec, c.cycles, c.gas_stor, tx_bytes, c.fee_base);
}

fn print_attestation(a: &Attestation) {
    println!(
        "    proven at block {} ({}), {} hop(s), record slot {}, chain {}",
        a.block_number,
        h(&a.block_hash),
        a.hops,
        a.slot,
        a.chain_id
    );
    for (i, st) in a.statements.iter().enumerate() {
        let fact = match &st.fact {
            Fact::Account { address, nonce, balance, storage_root, code_hash } => format!(
                "account {} nonce {nonce} balance {} storage root {} code hash {}",
                h(address),
                h(balance),
                h(storage_root),
                h(code_hash)
            ),
            Fact::Storage { address, slot, value } => format!("storage {} slot {} = {}", h(address), h(slot), h(value)),
            Fact::Log { tx_index, log_index, address, topics, data } => format!(
                "log tx {tx_index} log {log_index} emitter {} topic0 {} ({} topics, {} data bytes)",
                h(address),
                topics.first().map(|t| h(t)).unwrap_or_default(),
                topics.len(),
                data.len()
            ),
            Fact::AccumulatorLeaf { address, leaf_index, leaf, .. } => format!("accumulator leaf {leaf_index} of {} = {}", h(address), h(leaf)),
        };
        println!("    [{i}] {fact}\n        statement id {}", h(&st.id));
    }
}

fn status(s: &Stage5) {
    let inst = &s.w.inst;
    let reports = &s.w.chain.reports;
    let gas: u64 = reports.iter().map(|r| r.charge.gas_exec).sum();
    let stor: u64 = reports.iter().map(|r| r.charge.gas_stor).sum();
    let store = inst.store(&s.w.chain);
    let slot = s.fx.finalized_slot;
    let header = s.header(slot);
    println!("status");
    println!("  program            {}", inst.program);
    println!("  instance           {} (chain {}, store layout {:?})", h(&inst.id), inst.config.chain_id, store.fork);
    println!(
        "  followed           Init, then the updates of periods {FIRST_PERIOD}-{LAST_PERIOD} and the fixture's finality update: {} transactions, all applied: {}",
        reports.len(),
        reports.iter().all(|r| r.ok())
    );
    println!("                     execution gas {gas} in all, storage gas {stor} in all");
    println!("  finalized slot     {} (store), the record's: {slot}", store.finalized.slot);
    match inst.record(&s.w.chain, slot).map(|r| r.header()) {
        Some(Ok(record)) => println!(
            "  record             header(id, {slot}) = {}: block {} execution {:?}",
            header.account_id,
            s.fx.block,
            record.execution
        ),
        _ => println!("  record             none at slot {slot}"),
    }
}

fn run(s: &mut Stage5, cmd: &Command, names: &BTreeMap<u32, (String, String)>, out: Option<&PathBuf>, n: usize) -> Result<u8, String> {
    if let Op::Status = cmd.op {
        status(s);
        return Ok(0);
    }
    let (req, what) = request(s, &cmd.op)?;
    let slot = s.fx.finalized_slot;
    let root = s.header(slot);
    let msg = Message::Attest { request: req.clone() };
    let message = borsh::to_vec(&msg).map_err(|e| e.to_string())?;
    println!("{} {what}", if cmd.send { "send:" } else { "dry run:" });
    println!("  message            Attest (variant 7 of ferrata_lez_wire::Message), {} bytes borsh", message.len());
    println!(
        "  to                 header(instance, slot {slot}) = {} of program {}",
        root.account_id, root.program_account_id
    );
    println!(
        "  request            chain {}, instance {}, the block's RLP header ({} bytes: the Gloas record binds the hash only), {} account(s), {} slot(s), {} log(s)",
        req.chain_id,
        h(&req.instance_id),
        req.header.as_ref().map_or(0, Vec::len),
        req.accounts.len(),
        req.accounts.iter().map(|a| a.slots.len()).sum::<usize>(),
        req.logs.len()
    );
    if let Some(dir) = out {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let stem = dir.join(format!("message-{n}"));
        std::fs::write(stem.with_extension("hex"), hex::encode(&message) + "\n").map_err(|e| e.to_string())?;
        std::fs::write(stem.with_extension("debug"), format!("{msg:?}\n")).map_err(|e| e.to_string())?;
        println!("  written            {} (borsh, hex) and {} (its Rust Debug text)", stem.with_extension("hex").display(), stem.with_extension("debug").display());
    }
    // 1. The verdict: the program's handler function, natively, on the record the zone holds.
    let verdict = s.expected(slot, &req);
    match &verdict {
        Ok(a) => {
            println!("  verdict            attested: {} statement(s) (ferrata_lez_wire::attest::evaluate, the program's own function)", a.statements.len());
            print_attestation(a);
        }
        Err(c) => println!("  verdict            refused with {}", describe(u32::from(*c), names)),
    }
    // 2. The cost: LEZ's settlement on a scratch copy of the zone's state.
    let dry = dry_run(s, root, &message)?;
    print_charge("  cost               ", &dry.charge, dry.tx_bytes);
    match (&dry.charge.revert, dry.exit_code) {
        (None, _) => println!("  events             {}", dry.events.iter().map(|(sel, d)| format!("{} ({} bytes)", String::from_utf8_lossy(sel), d.len())).collect::<Vec<_>>().join(", ")),
        (Some(_), Some(c)) => println!("  outcome            the transaction would revert with {} (the fee is charged)", describe(c, names)),
        (Some(r), None) => println!("  outcome            the transaction would revert: {r}"),
    }
    // The traced transitions must sum to what LEZ metered (the harness's own check).
    if (dry.charge.revert.is_none() || dry.exit_code.is_some()) && dry.cycles != dry.charge.cycles {
        return Err(format!("traced {} cycles, LEZ metered {}", dry.cycles, dry.charge.cycles));
    }
    // The verdict and the zone's execution must agree.
    let expected_code = verdict.as_ref().err().map(|c| u32::from(*c));
    if dry.exit_code != expected_code {
        return Err(format!("the native verdict ({expected_code:?}) and the zone's dry run ({:?}) disagree", dry.exit_code));
    }
    if let Ok(a) = &verdict {
        let event = dry.events.iter().find(|(sel, _)| sel.as_slice() == ATTESTED_SELECTOR.as_slice());
        if event.map(|(_, d)| d) != Some(&a.event_data()) {
            return Err("the dry run's FRT-ATT1 event does not name the verdict's identifiers".into());
        }
    }
    if !cmd.send {
        println!("  nothing was sent (add --send to settle it in the harness's zone)");
        return Ok(if verdict.is_ok() { 0 } else { 3 });
    }
    // 3. Send: settle it on the zone's state, and compare with the dry run.
    let report = s.w.chain.submit(&format!("ferrata-attest {n}"), root, message, vec![root], vec![], s.time);
    print_charge("  settled            ", &report.charge, report.tx_bytes);
    let events: Vec<(Vec<u8>, Vec<u8>)> = report.events.iter().map(|e| (e.event.selector.to_vec(), e.event.data.clone())).collect();
    let same = report.exit_code == dry.exit_code
        && report.charge.cycles == dry.charge.cycles
        && report.charge.gas_exec == dry.charge.gas_exec
        && report.charge.gas_stor == dry.charge.gas_stor
        && report.charge.fee_base == dry.charge.fee_base
        && report.tx_bytes == dry.tx_bytes
        && events == dry.events;
    println!(
        "  result             {} in block {}; exit {:?}; {} event(s); equal to the dry run: {}",
        if report.ok() { "applied" } else { "reverted" },
        s.w.chain.block_id - 1,
        report.exit_code,
        events.len(),
        if same { "yes" } else { "NO" }
    );
    if !same {
        return Err("the settled transaction differs from the dry run".into());
    }
    Ok(if report.ok() { 0 } else { 3 })
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
        return usage();
    }
    let (out, commands) = match parse(&args) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ferrata-attest: {e}");
            return usage();
        }
    };
    let names = code_names();
    let started = std::time::Instant::now();
    eprintln!("following Sepolia in the harness's zone: Init, the updates of periods {FIRST_PERIOD}-{LAST_PERIOD}, then the attestation fixture's finality update ...");
    let mut s = Stage5::new();
    eprintln!("followed in {:.1} s", started.elapsed().as_secs_f64());
    let mut status = 0u8;
    for (n, cmd) in commands.iter().enumerate() {
        match run(&mut s, cmd, &names, out.as_ref(), n + 1) {
            Ok(code) => status = status.max(code),
            Err(e) => {
                println!("  error              {e}");
                return ExitCode::from(1);
            }
        }
        println!();
    }
    ExitCode::from(status)
}
