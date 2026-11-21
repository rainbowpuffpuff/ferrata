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
//! RISC0_DEV_MODE=1 ferrata-attest [--follow-to P] [--message-out DIR] COMMAND [ARGS] [--send] [--then COMMAND ...]
//!   status                                  the instance, its record, what following cost
//!   config                                  the instance's configuration, read from the zone
//!   advance [--period P | --finality]       submit a light-client update (Open, Miller, Adopt, Finish)
//!   slot    [--address A] [--slot S] [--expect-eq V]   a storage slot (default: USDC totalSupply_)
//!   account [--address A] [--expect-code-hash H]       an account (default: Multicall3)
//!   log     [--tx I --log J | --block-log-index K]     a log (default: the block's first Transfer)
//!   receipt --tx I                          every log of transaction I's receipt
//! ```
//! `--follow-to P` follows Sepolia only to period P (1378-1409; default 1409, and then the
//! attestation fixture's finality update too), so that `advance` has updates left to submit.
//! Exit status: 0 attested or advanced (or would be), 3 refused as the dry run predicted, 1 the dry
//! run and the execution disagree, 2 a usage error.

use std::{collections::BTreeMap, path::PathBuf, process::ExitCode};

use fee_core::{market, BlockFeeSummary};
use ferrata_core::{
    exec::locate_log,
    steps,
    witness::{uncompressed_keys, CompactUpdate},
};
use ferrata_lez_wire::{
    attest::{AccountQuery, Attestation, Cmp, Condition, Fact, Field, LogQuery, Request},
    code, header_actor, job_actor, job_id, Message, ATTESTED_SELECTOR,
};
use ferrata_spectests::execkit::AccountProof;
use harness::{
    plan_job,
    sepolia::{replay, sepolia_update, SepoliaFacts, Stage5, FIRST_PERIOD, LAST_PERIOD, SEPOLIA_CHAIN_ID},
    settle::{classify, opening_fee_state, settle_charged_transaction, Charge, LeeTransaction},
    trace::trace_public,
    Instance,
};
use lee::{
    public_transaction::{Message as PublicMessage, WitnessSet},
    AccountId, Actor, FeeDeclaration, PrivateKey, PublicKey, PublicTransaction, V03State,
};

type Word = [u8; 32];
type Address = [u8; 20];

#[derive(Debug)]
enum Op {
    Status,
    Config,
    Slot {
        address: Option<Address>,
        slot: Option<Word>,
        expect_eq: Option<Word>,
    },
    Account {
        address: Option<Address>,
        expect_code_hash: Option<Word>,
    },
    Log {
        tx: Option<u64>,
        log: Option<u64>,
        block_log_index: Option<u64>,
    },
    Receipt {
        tx: u64,
    },
    /// `None`: the attestation fixture's finality update.
    Advance {
        period: Option<u64>,
    },
}

#[derive(Debug)]
struct Command {
    op: Op,
    send: bool,
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: ferrata-attest [--follow-to P] [--message-out DIR] COMMAND [ARGS] [--send] [--then COMMAND ...]\n\
         commands: status | config | advance [--period P | --finality] | slot [--address A] [--slot S] [--expect-eq V]\n\
         \x20         | account [--address A] [--expect-code-hash H] | log [--tx I --log J | --block-log-index K] | receipt --tx I\n\
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

struct Globals {
    message_out: Option<PathBuf>,
    follow_to: u64,
}

fn parse(args: &[String]) -> Result<(Globals, Vec<Command>), String> {
    let mut args = args.to_vec();
    let mut g = Globals { message_out: None, follow_to: LAST_PERIOD };
    loop {
        match args.first().map(String::as_str) {
            Some("--message-out") => {
                g.message_out = Some(PathBuf::from(args.get(1).ok_or("--message-out needs a directory")?));
                args.drain(..2);
            }
            Some("--follow-to") => {
                g.follow_to = u64_arg(args.get(1).ok_or("--follow-to needs a period")?)?;
                if !(FIRST_PERIOD..=LAST_PERIOD).contains(&g.follow_to) {
                    return Err(format!("--follow-to: the fixture's periods are {FIRST_PERIOD}-{LAST_PERIOD}"));
                }
                args.drain(..2);
            }
            _ => break,
        }
    }
    let mut commands = Vec::new();
    for group in args.split(|a| a == "--then") {
        let (name, rest) = group.split_first().ok_or("an empty command")?;
        let mut send = false;
        let mut finality = false;
        let mut opts: BTreeMap<&str, &str> = BTreeMap::new();
        let mut i = 0;
        while i < rest.len() {
            match rest[i].as_str() {
                "--send" => send = true,
                "--finality" => finality = true,
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
            "status" | "config" => &[],
            "slot" => &["--address", "--slot", "--expect-eq"],
            "account" => &["--address", "--expect-code-hash"],
            "log" => &["--tx", "--log", "--block-log-index"],
            "receipt" => &["--tx"],
            "advance" => &["--period"],
            other => return Err(format!("unknown command {other}")),
        };
        if finality && name != "advance" {
            return Err(format!("{name}: unknown option --finality"));
        }
        if let Some(k) = opts.keys().find(|k| !allowed.contains(k)) {
            return Err(format!("{name}: unknown option {k}"));
        }
        let op = match name.as_str() {
            "status" => Op::Status,
            "config" => Op::Config,
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
            "advance" => {
                let period = get("--period").map(u64_arg).transpose()?;
                if period.is_some() == finality {
                    return Err("advance: give --period P or --finality".into());
                }
                Op::Advance { period }
            }
            _ => unreachable!(),
        };
        if send && matches!(op, Op::Status | Op::Config) {
            return Err(format!("{name} sends nothing"));
        }
        commands.push(Command { op, send });
    }
    Ok((g, commands))
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
        1..=61 | 100..=199 => "the core",
        62..=72 => "the recorded-statement programs",
        73..=79 => "the rebate program",
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

fn account_query(
    proof: &AccountProof,
    conditions: Vec<Condition>,
    slots: Vec<(Word, Vec<Condition>)>,
) -> Result<AccountQuery, String> {
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
    let base = Request {
        instance_id: s.w.inst.id,
        chain_id: SEPOLIA_CHAIN_ID,
        header: Some(s.fx.header.clone()),
        ..Request::default()
    };
    match op {
        Op::Status | Op::Config | Op::Advance { .. } => unreachable!(),
        Op::Slot { address, slot, expect_eq } => {
            let address = address.unwrap_or(s.fx.contract);
            let slot = slot.unwrap_or(s.fx.slot);
            let proof = find_proof(s, &address)?;
            let conditions =
                expect_eq.map(|v| vec![Condition { field: Field::Value, cmp: Cmp::Eq, rhs: v }]).unwrap_or_default();
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
            let conditions = expect_code_hash
                .map(|v| vec![Condition { field: Field::CodeHash, cmp: Cmp::Eq, rhs: v }])
                .unwrap_or_default();
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
                if conditions.is_empty() {
                    String::new()
                } else {
                    " (the block's first ERC-20 Transfer: emitter and recipient checked)".into()
                }
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
            let logs = (0..n)
                .map(|l| LogQuery { tx_index: *tx, log_index: l, proof: proof.clone(), conditions: vec![] })
                .collect();
            Ok((
                Request { logs, ..base },
                format!("every log of transaction {tx}'s receipt ({n} logs, status 1 required)"),
            ))
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

/// One transaction, signed as the harness's fee payer signs (`Chain::new`: a fixed key), traced
/// and settled by LEZ's settlement on `state`, which the caller passes as a scratch copy.
#[allow(clippy::too_many_arguments)]
fn settle_on(
    state: &mut V03State,
    payer: AccountId,
    block_id: u64,
    time: u64,
    root: Actor,
    message: &[u8],
    actors: Vec<Actor>,
) -> Result<DryRun, String> {
    let payer_key = PrivateKey::try_new([0x11; 32]).unwrap();
    if AccountId::from(&PublicKey::new_from_private_key(&payer_key)) != payer {
        return Err("the harness's fee payer is not the expected key".into());
    }
    let gas_limit = market::MAX_GAS_EXEC;
    let fee = FeeDeclaration::new(payer, gas_limit, 0, u128::MAX >> 8);
    let nonces = BTreeMap::from([(payer, state.get_account_by_id(payer).nonce)]);
    let msg = PublicMessage::new(root, message.to_vec(), actors, nonces, Some(fee), vec![]);
    let witness_set = WitnessSet::for_message(&msg, &[&payer_key]);
    let tx = PublicTransaction::new(msg, witness_set);
    let (transitions, _) = trace_public(&tx, &*state, gas_limit);
    let lee_tx = LeeTransaction::Public(tx);
    let tx_bytes = borsh::to_vec(&lee_tx).map_err(|e| e.to_string())?.len();
    let view = classify(&lee_tx)?;
    let opening = opening_fee_state(&*state);
    let mut summary = BlockFeeSummary::default();
    let (events, charge) = settle_charged_transaction(&lee_tx, &view, state, &opening, block_id, time, &mut summary)
        .map_err(|e| format!("LEZ would reject the transaction: {e}"))?;
    Ok(DryRun {
        tx_bytes,
        cycles: transitions.iter().map(|t| t.cycles).sum(),
        exit_code: transitions.iter().map(|t| t.exit_code).find(|c| *c != 0),
        charge,
        events: events.iter().map(|e| (e.event.selector.to_vec(), e.event.data.clone())).collect(),
    })
}

fn dry_run(s: &Stage5, root: Actor, message: &[u8]) -> Result<DryRun, String> {
    let chain = &s.w.chain;
    let mut scratch = chain.state.clone();
    settle_on(&mut scratch, chain.payer, chain.block_id, s.time, root, message, vec![root])
}

// ------------------------------------------------------------------------------------------
// Advancing the instance: one light-client update as the public job
// ------------------------------------------------------------------------------------------

struct Step {
    label: String,
    root: Actor,
    message: Vec<u8>,
    actors: Vec<Actor>,
}

/// The job's transactions as `Instance::update_in` sends them, the native verdict of step 1 and
/// the job's planned outcome, and the time the transactions are stamped with.
#[allow(clippy::type_complexity)]
fn job_plan(
    s: &Stage5,
    update: &ferrata_core::types::LightClientUpdate,
) -> Result<(Vec<Step>, Result<Option<ferrata_core::store::ApplyOutcome>, u16>, u64), String> {
    let w = &s.w;
    let inst = &w.inst;
    let spec = inst.config.spec();
    let stored = inst.store(&w.chain);
    let layout = inst.layout_for(&stored, update);
    let mut next = w.native.clone();
    if layout > next.core().fork {
        next.upgrade(layout, &w.spec).map_err(|e| format!("the native store does not upgrade: {e:?}"))?;
    }
    let before = next.core().clone();
    let store = steps::store_for_update(&stored, layout, update.attested_header.beacon.slot, &spec)
        .unwrap_or_else(|_| stored.clone());
    let compact = CompactUpdate::of(update);
    let witness = Instance::signer_witness(&before, update, &spec);
    let verdict = steps::step1(
        &store,
        &compact,
        &witness,
        update.signature_slot,
        &spec,
        &inst.config.genesis_validators_root,
        inst.config.adoption_parts,
    );
    let (planned, outcome) = plan_job(&store, &compact, &witness, update, &inst.config);
    let jid = job_id(&inst.id, &store.digest(), &compact.digest(layout));
    let job = job_actor(inst.program, &inst.id, &jid);
    let enc = |m: &Message| borsh::to_vec(m).expect("borsh");
    let mut out = vec![
        Step {
            label: "Open (step 1)".into(),
            root: inst.actor(),
            message: enc(&Message::Open { update: compact.encode(layout), signers: witness.encode() }),
            actors: vec![inst.actor(), job],
        },
        Step { label: "Miller (step 2)".into(), root: job, message: enc(&Message::Miller), actors: vec![job] },
    ];
    if planned.as_ref().is_some_and(|j| j.adoption.is_some()) {
        let keys = uncompressed_keys(&update.next_sync_committee).map_err(|e| format!("{e:?}"))?;
        let parts = inst.config.adoption_parts;
        let chunk = keys.len() / parts as usize;
        for p in 0..parts {
            let part = keys[p as usize * chunk..(p as usize + 1) * chunk].to_vec();
            out.push(Step {
                label: format!("Adopt {}/{parts}", p + 1),
                root: job,
                message: enc(&Message::Adopt { part: p, keys: part }),
                actors: vec![job],
            });
        }
    }
    let mut actors = vec![job, inst.actor()];
    if outcome.is_some_and(|o| o.advanced) {
        actors.push(header_actor(inst.program, &inst.id, update.finalized_header.beacon.slot));
    }
    out.push(Step { label: "Finish (step 3)".into(), root: job, message: enc(&Message::Finish), actors });
    let verdict = verdict.map(|_| outcome).map_err(|e| e.code());
    Ok((out, verdict, inst.config.slot_start_ms(update.signature_slot) + 12_000))
}

fn advance(
    s: &mut Stage5,
    period: Option<u64>,
    send: bool,
    names: &BTreeMap<u32, (String, String)>,
) -> Result<u8, String> {
    let (update, what) = match period {
        Some(p) if (FIRST_PERIOD..=LAST_PERIOD).contains(&p) => {
            (sepolia_update(p), format!("the update of period {p}"))
        }
        Some(p) => return Err(format!("the fixture's updates are of periods {FIRST_PERIOD}-{LAST_PERIOD}, not {p}")),
        None => (s.fx.update.clone(), "the finality update of the attestation fixture".into()),
    };
    let before = s.w.inst.store(&s.w.chain);
    println!("{} advance with {what}", if send { "send:" } else { "dry run:" });
    println!(
        "  instance           {} (store layout {:?}, finalized slot {}), {} update jobs submitted",
        h(&s.w.inst.id),
        before.fork,
        before.finalized.slot,
        s.w.runs.len()
    );
    println!(
        "  update             attested slot {}, finalized slot {}, signature slot {}, next committee {}",
        update.attested_header.beacon.slot,
        update.finalized_header.beacon.slot,
        update.signature_slot,
        if update.next_sync_committee_branch.iter().any(|b| *b != [0; 32]) {
            "carried"
        } else {
            "absent (a finality update)"
        }
    );
    let (plan, verdict, time) = job_plan(s, &update)?;
    match &verdict {
        Ok(Some(o)) => println!(
            "  verdict            step 1 accepts (ferrata_core::steps::step1, the program's own function); the job {} the finalized header{}{}",
            if o.advanced { "advances" } else { "does not advance" },
            if o.adopted { ", adopts the next committee" } else { "" },
            if o.rotated { ", rotates to the next period" } else { "" }
        ),
        Ok(None) => println!("  verdict            step 1 accepts; a later step refuses (the job is not applied)"),
        Err(c) => println!("  verdict            refused at step 1 with {}", describe(u32::from(*c), names)),
    }
    // The dry run: the job's transactions, one after the other, on a scratch copy of the state.
    let chain = &s.w.chain;
    let mut scratch = chain.state.clone();
    let mut dry = Vec::new();
    for (i, step) in plan.iter().enumerate() {
        let run = settle_on(
            &mut scratch,
            chain.payer,
            chain.block_id + i as u64,
            time,
            step.root,
            &step.message,
            step.actors.clone(),
        )?;
        let refused_open = i == 0 && run.charge.revert.is_some();
        dry.push(run);
        if refused_open {
            break; // as Instance::update_in: no job, nothing more to send
        }
    }
    let (mut gas, mut stor) = (0u64, 0u64);
    for (step, run) in plan.iter().zip(&dry) {
        gas += run.charge.gas_exec;
        stor += run.charge.gas_stor;
        println!(
            "  {:<18} message {:>7} bytes: execution gas {:>9} (cycles {}), storage gas {:>6}, base fee {}{}",
            step.label,
            step.message.len(),
            run.charge.gas_exec,
            run.charge.cycles,
            run.charge.gas_stor,
            run.charge.fee_base,
            match (&run.charge.revert, run.exit_code) {
                (None, _) => String::new(),
                (Some(_), Some(c)) => format!("; reverts with {}", describe(c, names)),
                (Some(r), None) => format!("; reverts: {r}"),
            }
        );
    }
    println!("  cost               {} transaction(s): execution gas {gas}, storage gas {stor}", dry.len());
    // The verdict and the zone's execution must agree.
    let all_ok = dry.iter().all(|d| d.charge.revert.is_none());
    let agree = match &verdict {
        Ok(Some(_)) => all_ok,
        Ok(None) => !all_ok,
        Err(c) => dry.len() == 1 && dry[0].exit_code == Some(u32::from(*c)),
    };
    if !agree {
        return Err("the native verdict and the zone's dry run disagree".into());
    }
    if !send {
        println!("  nothing was sent (add --send to settle it in the harness's zone)");
        return Ok(if all_ok { 0 } else { 3 });
    }
    let run = s.w.apply(&update, &format!("ferrata-attest advance ({what})"));
    let same = run.reports.len() == dry.len()
        && run.reports.iter().zip(&dry).all(|(r, d)| {
            r.exit_code == d.exit_code
                && r.charge.cycles == d.charge.cycles
                && r.charge.gas_exec == d.charge.gas_exec
                && r.charge.gas_stor == d.charge.gas_stor
                && r.charge.fee_base == d.charge.fee_base
                && r.tx_bytes == d.tx_bytes
        });
    let after = s.w.inst.store(&s.w.chain);
    println!(
        "  result             {} transaction(s) settled, {}; finalized slot {} -> {}; equal to the dry run: {}",
        run.reports.len(),
        if run.reports.iter().all(|r| r.ok()) { "all applied" } else { "refused" },
        before.finalized.slot,
        after.finalized.slot,
        if same { "yes" } else { "NO" }
    );
    if !same {
        return Err("the settled job differs from the dry run".into());
    }
    if run.reports.iter().all(|r| r.ok()) {
        s.time = s.time.max(time);
        Ok(0)
    } else {
        Ok(3)
    }
}

fn print_charge(prefix: &str, c: &Charge, tx_bytes: usize) {
    println!(
        "{prefix}execution gas {} (cycles metered {}), storage gas {} (the transaction's {} bytes), base fee {}",
        c.gas_exec, c.cycles, c.gas_stor, tx_bytes, c.fee_base
    );
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
            Fact::AccumulatorLeaf { address, leaf_index, leaf, .. } => {
                format!("accumulator leaf {leaf_index} of {} = {}", h(address), h(leaf))
            }
            // A fact kind added to the wire later is printed as it is, rather than breaking this build.
            #[allow(unreachable_patterns)]
            other => format!("{other:?}"),
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
    let slot = store.finalized.slot;
    let header = s.header(slot);
    println!("status");
    println!("  program            {}", inst.program);
    println!("  instance           {} (chain {}, store layout {:?})", h(&inst.id), inst.config.chain_id, store.fork);
    println!(
        "  followed           Init and {} update job(s) submitted through the program: {} transactions, all applied: {}",
        s.w.runs.len(),
        reports.len(),
        reports.iter().all(|r| r.ok())
    );
    println!("                     execution gas {gas} in all, storage gas {stor} in all");
    match inst.record(&s.w.chain, slot).map(|r| r.header()) {
        Some(Ok(record)) => {
            println!("  record             header(id, {slot}) = {}: {:?}", header.account_id, record.execution)
        }
        _ => println!("  record             none at slot {slot}"),
    }
}

/// The module's "read its configuration": the instance's state as the zone holds it, and its id
/// derived again from the configuration and the checkpoint (`ferrata_lez_wire::instance_id`).
fn config(s: &Stage5) -> Result<u8, String> {
    let inst = &s.w.inst;
    let state = inst.state(&s.w.chain).ok_or("no instance at the instance's actor")?;
    let c = &state.config;
    let derived = ferrata_lez_wire::instance_id(c, &inst.checkpoint);
    println!("config (instance(id)'s state on the zone)");
    println!("  instance actor     {} of program {}", inst.actor().account_id, inst.program);
    println!(
        "  instance id        {} (SHA-256 of the domain, borsh(config) and the checkpoint: {})",
        h(&state.id),
        if derived == state.id { "derived again, equal" } else { "DIFFERS" }
    );
    println!("  checkpoint         {} (the trusted beacon block root of Init)", h(&inst.checkpoint));
    println!("  chain id           {}", c.chain_id);
    println!(
        "  sync committee     {} keys, at least {} signing; {} slots per epoch, {} epochs per period",
        c.sync_committee_size, c.min_sync_committee_participants, c.slots_per_epoch, c.epochs_per_sync_committee_period
    );
    println!(
        "  genesis            time {}, {} s per slot, validators root {}",
        c.genesis_time,
        c.seconds_per_slot,
        h(&c.genesis_validators_root)
    );
    for (i, (version, epoch)) in c.fork_versions.iter().zip(c.fork_epochs).enumerate() {
        let at = if epoch == u64::MAX { "not scheduled".to_string() } else { format!("from epoch {epoch}") };
        println!("  fork {i}             version {}, {at}", h(version));
    }
    println!(
        "  layout at Init     {} ({:?}; the store upgrades itself at later forks); a committee adopted in {} parts",
        c.store_fork,
        c.fork(),
        c.adoption_parts
    );
    println!(
        "  state              active {}, {} jobs applied, store {} bytes",
        state.active,
        state.applied,
        state.store.len()
    );
    if derived != state.id {
        return Err("the instance id is not derived from its configuration".into());
    }
    Ok(0)
}

fn run(
    s: &mut Stage5,
    cmd: &Command,
    names: &BTreeMap<u32, (String, String)>,
    out: Option<&PathBuf>,
    n: usize,
) -> Result<u8, String> {
    if let Op::Status = cmd.op {
        status(s);
        return Ok(0);
    }
    if let Op::Config = cmd.op {
        return config(s);
    }
    if let Op::Advance { period } = cmd.op {
        return advance(s, period, cmd.send, names);
    }
    let slot = s.fx.finalized_slot;
    if s.w.inst.record(&s.w.chain, slot).is_none() {
        return Err(format!(
            "no record at slot {slot} (block {}): follow to {LAST_PERIOD} (the default), or run `advance --finality --send` first",
            s.fx.block
        ));
    }
    let (req, what) = request(s, &cmd.op)?;
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
        println!(
            "  written            {} (borsh, hex) and {} (its Rust Debug text)",
            stem.with_extension("hex").display(),
            stem.with_extension("debug").display()
        );
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
        (None, _) => println!(
            "  events             {}",
            dry.events
                .iter()
                .map(|(sel, d)| format!("{} ({} bytes)", String::from_utf8_lossy(sel), d.len()))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        (Some(_), Some(c)) => println!(
            "  outcome            the transaction would revert with {} (the fee is charged)",
            describe(c, names)
        ),
        (Some(r), None) => println!("  outcome            the transaction would revert: {r}"),
    }
    // The traced transitions must sum to what LEZ metered (the harness's own check).
    if (dry.charge.revert.is_none() || dry.exit_code.is_some()) && dry.cycles != dry.charge.cycles {
        return Err(format!("traced {} cycles, LEZ metered {}", dry.cycles, dry.charge.cycles));
    }
    // The verdict and the zone's execution must agree.
    let expected_code = verdict.as_ref().err().map(|c| u32::from(*c));
    if dry.exit_code != expected_code {
        return Err(format!(
            "the native verdict ({expected_code:?}) and the zone's dry run ({:?}) disagree",
            dry.exit_code
        ));
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
    let events: Vec<(Vec<u8>, Vec<u8>)> =
        report.events.iter().map(|e| (e.event.selector.to_vec(), e.event.data.clone())).collect();
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
    let (g, commands) = match parse(&args) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ferrata-attest: {e}");
            return usage();
        }
    };
    let names = code_names();
    let started = std::time::Instant::now();
    let full = g.follow_to == LAST_PERIOD;
    eprintln!(
        "following Sepolia in the harness's zone: Init, the updates of periods {FIRST_PERIOD}-{}{} ...",
        g.follow_to,
        if full { ", then the attestation fixture's finality update" } else { "" }
    );
    let mut s =
        if full { Stage5::new() } else { Stage5 { w: replay(2, g.follow_to), fx: SepoliaFacts::load(), time: 0 } };
    eprintln!("followed in {:.1} s", started.elapsed().as_secs_f64());
    let mut status = 0u8;
    for (n, cmd) in commands.iter().enumerate() {
        match run(&mut s, cmd, &names, g.message_out.as_ref(), n + 1) {
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
