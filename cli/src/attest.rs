//! `ferrata attest`: the attestation side's commands against a running LEZ v0.4 sequencer (RFP-021 U.2).
//!
//! The same commands as the harness's `ferrata-attest` (`lez/ATTEST-CLI.md`), on a real zone instead of the in-process
//! chain: read an instance's configuration and state, create an instance (`init`), submit a consensus-state update
//! (`advance`: the job `Open`, `Miller`, `Adopt` parts, `Finish`, planned with the updater's own code, `lez/advance`),
//! and attest a storage slot, an account or a log for a `(chain, contract, slot or log, predicate)` tuple, with the
//! proofs fetched from an Ethereum JSON-RPC endpoint.
//!
//! Every command that sends is a **dry run** unless `--send --yes`. The dry run gives:
//! * the **verdict**: the program's own function natively (`ferrata_lez_wire::attest::evaluate` for an attestation,
//!   `ferrata_advance::evaluate` and the program's step code for an update, `steps::initialize` for an instance), with
//!   the program's code and its name;
//! * **the zone's execution**: the exact signed transaction, settled by LEZ's own `settle_transaction` on a scratch
//!   copy of the zone's state, which this tool keeps by following the sequencer's blocks with LEZ's block application
//!   (`lez/live/driver/src/follower.rs`, included here: the sequencer's RPC reports no outcome, so the follower is
//!   how anyone reads one): exit code, execution and storage gas, events.
//!
//! With `--send --yes` the same transactions are sent to the sequencer, and once a block that includes each is final
//! on Bedrock (`--wait-final`, default) the follower's receipt is compared with the dry run: exit code, execution
//! gas, storage gas, events. Exit status: 0 applied (or would be); 3 refused, as the dry run said; 1 the dry run and
//! the zone disagree, or the verdict and the dry run; 2 a usage error.

#![allow(clippy::too_many_arguments)]

#[path = "../../lez/live/driver/src/follower.rs"]
#[allow(dead_code)]
mod follower;
#[path = "../../lez/live/driver/src/plan.rs"]
#[allow(dead_code, unused_imports)]
mod plan;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, ensure, Context as _, Result};
use chain_state::apply::{opening_fee_state, settle_transaction};
use common::{block::BedrockStatus, block::Block, transaction::LeeTransaction};
use fee_core::BlockFeeSummary;
use ferrata_core::{
    fork::LcFork,
    rlp::encode_u64,
    steps,
    types::{LightClientBootstrap, SyncCommittee},
    witness::{uncompressed_keys, CompactBootstrap, CompactStore},
    Root,
};
use ferrata_lez_wire::{
    attest::{self as wa, AccountQuery, Cmp, Condition, Field, LogQuery, Request, SlotQuery},
    consumer::address_word,
    header_actor, instance_actor, instance_id, instance_seed, job_actor, job_id, InstanceState, Message, RecordState,
};
use ferrata_spectests::execkit::{account_proof, receipt_rlp, AccountProof, Trie};
use lee::{
    public_transaction::{Message as PublicMessage, WitnessSet},
    AccountId, Actor, FeeDeclaration, PrivateKey, PublicAccountEvidence, PublicKey, PublicTransaction, ValidatedStateDiff,
};
use sequencer_service_rpc::{SequencerClient, SequencerClientBuilder};
use serde_json::{json, Value};
use wallet::WalletSequencerClient as _;

use follower::{exit_code_of, Follower};

/// The program's codes, names and meanings, from the IDL (`idl/ferrata_attestation.idl.json`).
const IDL: &str = include_str!("../../idl/ferrata_attestation.idl.json");

fn code_name(code: u32) -> String {
    let idl: Value = serde_json::from_str(IDL).unwrap_or(Value::Null);
    idl["errors"]
        .as_array()
        .and_then(|e| e.iter().find(|x| x["code"].as_u64() == Some(u64::from(code))))
        .map(|x| format!("{} ({})", x["name"].as_str().unwrap_or("?"), x["msg"].as_str().unwrap_or("")))
        .unwrap_or_else(|| plan::exit_name(code))
}

#[derive(clap::Args)]
pub struct AttestArgs {
    /// The LEZ sequencer's JSON-RPC (default: the global --sequencer, else http://127.0.0.1:3040).
    #[arg(long, env = "FERRATA_LEZ_RPC")]
    lez_rpc: Option<String>,
    /// The attestation program's account (hex).
    #[arg(long, env = "FERRATA_ATTEST_PROGRAM")]
    program: String,
    /// The fee payer: a file holding its private key in hex (needed to dry-run or send a transaction).
    #[arg(long, env = "FERRATA_LEZ_KEY")]
    key: Option<PathBuf>,
    /// Where the follower keeps the zone's state between runs (default: none, the chain is followed from genesis).
    #[arg(long)]
    state_dir: Option<PathBuf>,
    /// Apply only blocks final on Bedrock (`yes`), or the sequencer's head (`no`).
    #[arg(long, default_value = "yes")]
    wait_final: String,
    /// How long to wait for a sent transaction's block to be final, in seconds.
    #[arg(long, default_value_t = 900)]
    settle_timeout: u64,
    /// The gas limit each transaction declares.
    #[arg(long, default_value_t = 10_000_000)]
    gas_limit: u64,
    /// Send what the dry run shows (with --yes); may follow the command.
    #[arg(long, global = true)]
    send: bool,
    #[arg(long, global = true)]
    yes: bool,
    /// Append the run's record (JSON lines: every dry run and every receipt) to this file.
    #[arg(long)]
    record: Option<PathBuf>,
    #[command(subcommand)]
    cmd: AttestCmd,
}

#[derive(clap::Subcommand)]
pub enum AttestCmd {
    /// An instance's configuration as the zone holds it, its id derived again (with --checkpoint).
    Config {
        #[arg(long)]
        instance: String,
        #[arg(long)]
        checkpoint: Option<String>,
    },
    /// An instance's state: its store, the newest record, the jobs applied.
    Status {
        #[arg(long)]
        instance: String,
    },
    /// Create an instance: `Init` from a bootstrap, then the adoption of its committee in --parts transactions.
    Init {
        /// mainnet or sepolia (the configuration of `lez/live/driver/src/plan.rs`).
        #[arg(long)]
        preset: String,
        #[arg(long)]
        bootstrap: PathBuf,
        /// The trusted block root the bootstrap is for.
        #[arg(long)]
        checkpoint: String,
        /// The bootstrap's light-client layout: electra or gloas.
        #[arg(long, default_value = "electra")]
        layout: String,
        #[arg(long, default_value_t = 2)]
        parts: u8,
    },
    /// Submit a consensus-state update: a period update (SSZ of an `updates` entry) or, with --finality, a finality update.
    Advance {
        #[arg(long)]
        instance: String,
        #[arg(long)]
        update: PathBuf,
        #[arg(long)]
        finality: bool,
        /// The full committees the store summarises: a bootstrap (its current committee) or an update (its next
        /// committee), SSZ, as `bootstrap:FILE` or `update:FILE`; repeat as needed.
        #[arg(long = "committee")]
        committees: Vec<String>,
    },
    /// An actor state of the program as the zone holds it (hex of its Borsh encoding): `instance(id)`, or with --slot
    /// the record `header(id, slot)`.
    State {
        #[arg(long)]
        instance: String,
        #[arg(long)]
        slot: Option<u64>,
    },
    /// Write a LEZ wallet (LEZ's own storage, encrypted under the password) holding the payer's key (--key), as Logos
    /// Core's `lez_core` opens one (`open(config, storage, password, force_rpc)`); no zone is read.
    Wallet {
        #[arg(long)]
        home: PathBuf,
        #[arg(long)]
        password_file: PathBuf,
    },
    /// The zone's receipts of transactions (sent by anyone, for instance through Logos Core's `lez_core`), from the
    /// follower: applied or refused with the program's code, execution and storage gas, events.
    Receipt {
        /// Transaction hashes (hex); repeat.
        #[arg(long = "tx", required = true)]
        txs: Vec<String>,
    },
    /// Attest under the record `header(instance, --record-slot)`: storage slots, accounts, logs, each with its predicate.
    Attest {
        #[arg(long)]
        instance: String,
        #[arg(long)]
        record_slot: u64,
        /// An Ethereum JSON-RPC endpoint (proofs are checked by the program against the record, never trusted).
        #[arg(long)]
        eth: String,
        /// The record's execution block number.
        #[arg(long)]
        block: u64,
        /// `ADDRESS:SLOT` or `ADDRESS:SLOT:CMP:VALUE` (CMP: eq, ne, ge, gt, le, lt).
        #[arg(long)]
        storage: Vec<String>,
        /// `ADDRESS` or `ADDRESS:codehash` (the code hash at the block, as a condition).
        #[arg(long)]
        account: Vec<String>,
        /// `TX:LOG` (the log's position in its receipt) or `TX:LOG:emitter=ADDRESS`.
        #[arg(long)]
        log: Vec<String>,
        /// The block's RLP header, for a record that binds the block hash only (Gloas layout).
        #[arg(long)]
        header_rlp: Option<PathBuf>,
    },
}

/// The zone: the sequencer's RPC, and LEZ's block application over its blocks.
struct Zone {
    rpc: String,
    client: SequencerClient,
    follower: Follower,
    wait_final: bool,
    state_dir: Option<PathBuf>,
}

impl Zone {
    async fn open(rpc: &str, wait_final: bool, state_dir: Option<PathBuf>) -> Result<Self> {
        let client = SequencerClientBuilder::default().build(rpc).with_context(|| format!("the sequencer at {rpc}"))?;
        let follower = match &state_dir {
            Some(d) if d.join("follower.borsh").exists() => {
                let snapshot = std::fs::read(d.join("follower.borsh"))?;
                let blocks: Vec<Block> = borsh::from_slice(&std::fs::read(d.join("blocks.borsh"))?)?;
                Follower::restore(&snapshot, blocks)?
            }
            _ => Follower::new(),
        };
        let mut z = Self { rpc: rpc.to_string(), client, follower, wait_final, state_dir };
        z.sync().await?;
        Ok(z)
    }

    /// Apply the sequencer's blocks after the follower's tip: to its head, or to its last block final on Bedrock.
    async fn sync(&mut self) -> Result<()> {
        let head = self.client.get_last_block_id().await.map_err(|e| anyhow!("getLastBlockId on {}: {e}", self.rpc))?;
        while self.follower.height() < head {
            let id = self.follower.height() + 1;
            let Some(block) = self.client.get_block(id).await.map_err(|e| anyhow!("getBlock {id}: {e}"))? else { break };
            if self.wait_final && !matches!(block.bedrock_status, BedrockStatus::Finalized) {
                break;
            }
            self.follower.apply(block)?;
        }
        if let Some(d) = &self.state_dir {
            std::fs::create_dir_all(d)?;
            std::fs::write(d.join("follower.borsh"), self.follower.snapshot()?)?;
            std::fs::write(d.join("blocks.borsh"), borsh::to_vec(&self.follower.blocks)?)?;
        }
        Ok(())
    }

    /// `actor`'s state as the follower holds it, checked against the sequencer's `getAccount`.
    async fn actor_state(&self, actor: Actor) -> Result<Vec<u8>> {
        let followed = self.follower.state.get_account_by_id(actor.account_id).data.actor_state(actor.program_account_id).to_vec();
        let served = self
            .client
            .get_account(actor.account_id)
            .await
            .map_err(|e| anyhow!("getAccount: {e}"))?
            .map(|a| a.data.actor_state(actor.program_account_id).to_vec())
            .unwrap_or_default();
        // The sequencer's head can be ahead of the last final block; equal states are expected whenever no block in
        // between touched the account.
        if served != followed {
            eprintln!("note: the sequencer's head and the last final block differ on {} (a later block touched it)", actor.account_id);
        }
        Ok(followed)
    }

    fn tip(&self) -> (u64, u64) {
        (self.follower.height(), self.follower.blocks.last().map_or(0, |b| b.header.timestamp))
    }
}

/// What LEZ's settlement did, or would do, with one transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Outcome {
    reverted: bool,
    exit_code: Option<u32>,
    gas_exec: u64,
    gas_stor: u64,
    events: u64,
    event_data: Vec<Vec<u8>>,
}

impl Outcome {
    fn show(&self) -> String {
        let verdict = match (self.reverted, self.exit_code) {
            (false, _) => "applied".to_string(),
            (true, Some(c)) => format!("refused with {c} {}", code_name(c)),
            (true, None) => "reverted without a code".to_string(),
        };
        format!("{verdict}; execution gas {}, storage gas {}, {} event(s)", self.gas_exec, self.gas_stor, self.events)
    }
    /// The program's code: 0 when applied.
    fn code(&self) -> u32 {
        if self.reverted {
            self.exit_code.unwrap_or(u32::MAX)
        } else {
            0
        }
    }
}

/// Settle `tx` on `state` as the next block would (LEZ's own `settle_transaction`), returning what it did.
fn settle(state: &mut lee::V03State, tx: &LeeTransaction, block_id: u64, ts: u64) -> Result<Outcome> {
    let opening = opening_fee_state(state);
    let mut summary = BlockFeeSummary::default();
    let pre = state.clone();
    let (events, reverted) =
        settle_transaction(tx, state, &opening, block_id, ts, 0, &mut summary).map_err(|e| anyhow!("settlement rejects it: {e}"))?;
    let reason = match (reverted, tx) {
        (true, LeeTransaction::Public(p)) => {
            let budget = p.message().execution.fee.map(|f| f.gas_limit).unwrap_or(fee_core::market::MAX_GAS_EXEC);
            ValidatedStateDiff::from_public_transaction_metered(p, &pre, block_id, ts, budget).0.revert
        }
        _ => None,
    };
    Ok(Outcome {
        reverted,
        exit_code: reason.as_deref().and_then(exit_code_of),
        gas_exec: summary.gas_used_exec,
        gas_stor: summary.gas_used_stor,
        events: events.len() as u64,
        event_data: events.iter().map(|e| e.event.data.clone()).collect(),
    })
}

struct Payer {
    key: PrivateKey,
    id: AccountId,
}

fn payer(path: &Option<PathBuf>) -> Result<Payer> {
    let path = path.as_ref().ok_or_else(|| anyhow!("--key (the fee payer's private key file) is needed to build a transaction"))?;
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let bytes: [u8; 32] = hex::decode(text.trim().trim_start_matches("0x"))?.try_into().map_err(|_| anyhow!("not 32 bytes"))?;
    let key = PrivateKey::try_new(bytes).map_err(|e| anyhow!("not a private key: {e:?}"))?;
    let id = AccountId::from(&PublicKey::new_from_private_key(&key));
    Ok(Payer { key, id })
}

/// One public transaction of the program: its root actor, message and actors, signed by the payer at `nonce`.
fn build(p: &Payer, nonce: u128, root: Actor, message: Vec<u8>, actors: Vec<Actor>, evidence: Vec<PublicAccountEvidence>, gas_limit: u64) -> LeeTransaction {
    let fee = FeeDeclaration::new(p.id, gas_limit, 0, wallet::max_fee_for(gas_limit));
    let nonces = BTreeMap::from([(p.id, lee_core::account::Nonce(nonce))]);
    let msg = PublicMessage::new(root, message, actors, nonces, Some(fee), evidence);
    let witness = WitnessSet::for_message(&msg, &[&p.key]);
    LeeTransaction::Public(PublicTransaction::new(msg, witness))
}

struct Step {
    label: String,
    root: Actor,
    message: Vec<u8>,
    actors: Vec<Actor>,
    evidence: Vec<PublicAccountEvidence>,
}

struct Run {
    record: Option<PathBuf>,
}

impl Run {
    fn log(&self, entry: Value) {
        if let Some(p) = &self.record {
            use std::io::Write as _;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
                let _ = writeln!(f, "{entry}");
            }
        }
    }
}

/// Dry-run `steps` in order on one scratch copy of the zone's state, then (with `send`) send each and compare the
/// zone's receipt with its dry run. `expected` is the verdict's code for the last step (0: applied), checked against
/// the dry run. Returns the exit status.
async fn transact(z: &mut Zone, a: &AttestArgs, run: &Run, what: &str, steps: Vec<Step>, expected: Option<u32>) -> Result<i32> {
    let p = payer(&a.key)?;
    let (tip, ts) = z.tip();
    let mut scratch = z.follower.state.clone();
    let start_nonce = scratch.get_account_by_id(p.id).nonce.0;
    let mut txs = Vec::new();
    let mut dry = Vec::new();
    println!("{what}: dry run of {} transaction(s) on the zone's state at block {tip} (payer {})", steps.len(), p.id);
    for (i, s) in steps.iter().enumerate() {
        let tx = build(&p, start_nonce + i as u128, s.root, s.message.clone(), s.actors.clone(), s.evidence.clone(), a.gas_limit);
        let o = settle(&mut scratch, &tx, tip + 1 + i as u64, ts + 10_000 * (1 + i as u64))?;
        println!("  {:<28} {} bytes; would be {}", s.label, borsh::to_vec(&tx)?.len(), o.show());
        run.log(json!({ "what": what, "step": s.label, "phase": "dry-run", "tx": tx.hash().to_string(), "bytes": borsh::to_vec(&tx)?.len(),
            "reverted": o.reverted, "exit_code": o.exit_code, "gas_exec": o.gas_exec, "gas_stor": o.gas_stor, "events": o.events }));
        let stop = o.reverted;
        txs.push(tx);
        dry.push(o);
        if stop {
            break;
        }
    }
    let last = dry.last().unwrap().clone();
    let mut status = if last.reverted { 3 } else { 0 };
    if let Some(e) = expected {
        if last.code() != e {
            println!("  DISAGREE: the verdict says {e}, the zone's execution {}", last.code());
            status = 1;
        } else {
            println!("  the verdict and the zone's execution agree: {}", if e == 0 { "applied".into() } else { format!("{e} {}", code_name(e)) });
        }
    }
    if !(a.send && a.yes) {
        if a.send {
            println!("  (--send needs --yes)");
        }
        return Ok(status);
    }
    for (i, (tx, d)) in txs.iter().zip(&dry).enumerate() {
        let label = &steps[i].label;
        let hash = z.client.send_transaction(tx.clone()).await.map_err(|e| anyhow!("{label}: the sequencer refused it: {e}"))?;
        ensure!(hash == tx.hash(), "{label}: the sequencer returned another hash");
        let started = Instant::now();
        let receipt = loop {
            z.sync().await?;
            if let Some(r) = z.follower.receipt(&hash) {
                break r.clone();
            }
            ensure!(started.elapsed() < Duration::from_secs(a.settle_timeout), "{label}: {hash} not in a final block after {} s", a.settle_timeout);
            tokio::time::sleep(Duration::from_secs(1)).await;
        };
        let zone = Outcome { reverted: receipt.reverted, exit_code: receipt.exit_code, gas_exec: receipt.gas_exec, gas_stor: receipt.gas_stor, events: receipt.events, event_data: vec![] };
        let equal = zone.reverted == d.reverted && zone.exit_code == d.exit_code && zone.gas_exec == d.gas_exec && zone.gas_stor == d.gas_stor && zone.events == d.events;
        println!(
            "  {:<28} block {} ({:.0} s): {}; {}",
            label,
            receipt.block_id,
            started.elapsed().as_secs_f64(),
            zone.show(),
            if equal { "equal to its dry run" } else { "DIFFERS from its dry run" }
        );
        run.log(json!({ "what": what, "step": label, "phase": "sent", "tx": hash.to_string(), "block": receipt.block_id,
            "reverted": zone.reverted, "exit_code": zone.exit_code, "gas_exec": zone.gas_exec, "gas_stor": zone.gas_stor,
            "events": zone.events, "equal_to_dry_run": equal, "base_fee": receipt.fee_base }));
        if !equal {
            status = 1;
        }
        if zone.reverted {
            break;
        }
    }
    Ok(status)
}

fn root_arg(s: &str) -> Result<Root> {
    hex::decode(s.trim().trim_start_matches("0x"))?.try_into().map_err(|_| anyhow!("{s}: not 32 bytes"))
}

fn addr(s: &str) -> Result<[u8; 20]> {
    hex::decode(s.trim().trim_start_matches("0x"))?.try_into().map_err(|_| anyhow!("{s}: not 20 bytes"))
}

fn word(s: &str) -> Result<[u8; 32]> {
    let d = s.trim().trim_start_matches("0x");
    let padded = format!("{d:0>64}");
    hex::decode(padded)?.try_into().map_err(|_| anyhow!("{s}: more than 32 bytes"))
}

fn cmp(s: &str) -> Result<Cmp> {
    Ok(match s {
        "eq" => Cmp::Eq,
        "ne" => Cmp::Ne,
        "ge" => Cmp::Ge,
        "gt" => Cmp::Gt,
        "le" => Cmp::Le,
        "lt" => Cmp::Lt,
        other => bail!("{other}: eq, ne, ge, gt, le or lt"),
    })
}

fn hx(b: &[u8]) -> String {
    format!("0x{}", hex::encode(b))
}

async fn instance_state(z: &Zone, program: AccountId, id: &Root) -> Result<InstanceState> {
    let bytes = z.actor_state(instance_actor(program, id)).await?;
    ensure!(!bytes.is_empty(), "no instance {} on the zone (program {program})", hx(id));
    let s: InstanceState = borsh::from_slice(&bytes).map_err(|e| anyhow!("instance state: {e}"))?;
    ensure!(s.id == *id, "the account holds another instance");
    Ok(s)
}

fn eth_call(url: &str, method: &str, params: Value) -> Result<Vec<u8>> {
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string();
    let out = std::process::Command::new("curl")
        .args(["-sS", "-m", "60", "-A", "ferrata-cli", "-H", "Content-Type: application/json", "--data-binary", &body, url])
        .output()
        .context("running curl")?;
    ensure!(out.status.success(), "{method} on {url}: {}", String::from_utf8_lossy(&out.stderr).trim());
    let v: Value = serde_json::from_slice(&out.stdout).with_context(|| format!("{method}: not JSON"))?;
    ensure!(v.get("error").is_none(), "{method}: {}", v["error"]);
    Ok(out.stdout)
}

/// `sequencer` is the tool's global `--sequencer`; `chain_id` its global `--chain-id`, the chain id a statement must
/// carry (default: the record's).
pub async fn run(a: AttestArgs, sequencer: Option<String>, chain_id: Option<u64>) -> Result<i32> {
    let program = AccountId::new(root_arg(&a.program)?);
    let run = Run { record: a.record.clone() };
    let rpc = a.lez_rpc.clone().or(sequencer).unwrap_or_else(|| "http://127.0.0.1:3040".to_string());
    if let AttestCmd::Wallet { home, password_file } = &a.cmd {
        let p = payer(&a.key)?;
        let password = std::fs::read_to_string(password_file)?;
        let password = password.trim_end_matches(['\n', '\r']);
        std::fs::create_dir_all(home)?;
        let config = home.join("wallet_config.json");
        let storage = home.join("storage.json");
        std::fs::write(
            &config,
            format!(
                "{{\n  \"sequencer\": {{ \"sequencer_addr\": \"{rpc}\" }},\n  \"seq_poll_timeout\": \"1s\",\n  \
                 \"seq_tx_poll_max_blocks\": 120,\n  \"seq_poll_max_retries\": 2,\n  \"seq_block_poll_max_amount\": 100,\n  \
                 \"gas_limit\": {}\n}}\n",
                a.gas_limit
            ),
        )?;
        let (mut w, _mnemonic) = wallet::WalletCore::new_init_storage(config.clone(), storage.clone(), None, password)?;
        w.storage_mut().key_chain_mut().add_imported_public_account(p.key);
        w.store_persistent_data()?;
        println!("wallet {}: config {}, storage {}, public account {} (the payer)", home.display(), config.display(), storage.display(), p.id);
        run.log(json!({ "what": "wallet", "home": home.display().to_string(), "account": p.id.to_string() }));
        return Ok(0);
    }
    let mut z = Zone::open(&rpc, a.wait_final != "no", a.state_dir.clone()).await?;
    let (tip, _) = z.tip();
    println!("zone {rpc}: followed to block {tip}{}", if z.wait_final { " (final on Bedrock)" } else { "" });
    match &a.cmd {
        AttestCmd::Config { instance, checkpoint } => {
            let id = root_arg(instance)?;
            let s = instance_state(&z, program, &id).await?;
            let c = &s.config;
            println!("instance {} (root id {}, parent {})", hx(&s.id), hx(&s.root_id), hx(&s.parent_id));
            println!("  chain id {}, committee {}, {} slots/epoch, {} epochs/period, {} s/slot, genesis time {}, genesis_validators_root {}",
                c.chain_id, c.sync_committee_size, c.slots_per_epoch, c.epochs_per_sync_committee_period, c.seconds_per_slot, c.genesis_time, hx(&c.genesis_validators_root));
            println!("  fork epochs {:?}; layout at Init {}; adoption parts {}; weak-subjectivity period {} epochs", c.fork_epochs, c.store_fork, c.adoption_parts, c.ws_period_epochs);
            println!("  active {}, jobs applied {}", s.active, s.applied);
            let mut derived = Value::Null;
            if let Some(cp) = checkpoint {
                let again = instance_id(c, &root_arg(cp)?);
                println!("  id derived again from the configuration and checkpoint {cp}: {} ({})", hx(&again), if again == id { "equal" } else { "DIFFERENT" });
                derived = json!(hx(&again));
                if again != id {
                    return Ok(1);
                }
            }
            run.log(json!({ "what": "config", "instance": hx(&id), "chain_id": c.chain_id, "applied": s.applied, "active": s.active, "derived_id": derived, "block": tip }));
            Ok(0)
        }
        AttestCmd::Status { instance } => {
            let id = root_arg(instance)?;
            let s = instance_state(&z, program, &id).await?;
            let store = CompactStore::decode(&s.store).map_err(|e| anyhow!("store: {e:?}"))?;
            let spec = s.config.spec();
            println!("instance {}: store layout {:?}, finalized slot {} (period {}), root {}, next committee {}, jobs applied {}",
                hx(&id), store.fork, store.finalized.slot, spec.period_at_slot(store.finalized.slot), hx(&store.finalized.hash_tree_root()),
                if store.next.is_some() { "known" } else { "unknown" }, s.applied);
            let rec = z.actor_state(header_actor(program, &id, store.finalized.slot)).await?;
            if !rec.is_empty() {
                let r: RecordState = borsh::from_slice(&rec).map_err(|e| anyhow!("record: {e}"))?;
                let h = r.header().map_err(|e| anyhow!("record: {e:?}"))?;
                println!("  newest record header({}, {}): chain id {}, execution {:?}", hx(&id), store.finalized.slot, r.chain_id, h.execution);
            }
            run.log(json!({ "what": "status", "instance": hx(&id), "finalized_slot": store.finalized.slot, "root": hx(&store.finalized.hash_tree_root()), "applied": s.applied, "block": tip }));
            Ok(0)
        }
        AttestCmd::Init { preset, bootstrap, checkpoint, layout, parts } => {
            let fork = match layout.as_str() {
                "electra" => LcFork::Electra,
                "gloas" => LcFork::Gloas,
                other => bail!("--layout {other}: electra or gloas"),
            };
            let mut config = match preset.as_str() {
                "mainnet" => plan::mainnet_config(*parts),
                "sepolia" => plan::sepolia_config(*parts),
                other => bail!("--preset {other}: mainnet or sepolia"),
            };
            config.store_fork = match fork {
                LcFork::Gloas => 4,
                _ => 3,
            };
            let checkpoint = root_arg(checkpoint)?;
            let b = LightClientBootstrap::decode(&std::fs::read(bootstrap)?, fork, config.sync_committee_size as usize)
                .map_err(|e| anyhow!("the bootstrap does not decode: {e:?}"))?;
            let id = instance_id(&config, &checkpoint);
            let compact = CompactBootstrap::of(&b);
            let spec = config.spec();
            let (store, job) = match steps::initialize(&checkpoint, &compact, fork, &spec, *parts) {
                Ok(x) => x,
                Err(e) => {
                    println!("init: the program's Init check refuses the bootstrap: {e:?}");
                    return Ok(3);
                }
            };
            let jid = job_id(&id, &job.store_digest, &job.update_digest);
            let job_a = job_actor(program, &id, &jid);
            println!("init: instance {} (chain {}), finalized slot {}, job {}", hx(&id), config.chain_id, store.finalized.slot, hx(&jid));
            let keys = uncompressed_keys(&b.current_sync_committee).map_err(|e| anyhow!("{e:?}"))?;
            let mut list = vec![Step {
                label: "Init".into(),
                root: instance_actor(program, &id),
                message: borsh::to_vec(&Message::Init { config: config.clone(), checkpoint, bootstrap: compact.encode(fork) })?,
                actors: vec![instance_actor(program, &id), job_a],
                evidence: vec![PublicAccountEvidence::Pda { program, seed: instance_seed(&id) }],
            }];
            for p in 0..*parts {
                let s = ferrata_advance::adopt_step(program, &id, *parts, job_a, &keys, p, Some(store.finalized.slot));
                list.push(Step { label: format!("Adopt {}/{}", p + 1, parts), root: s.root, message: s.message, actors: s.actors, evidence: vec![] });
            }
            transact(&mut z, &a, &run, "init", list, Some(0)).await
        }
        AttestCmd::Advance { instance, update, finality, committees } => {
            let id = root_arg(instance)?;
            let s = instance_state(&z, program, &id).await?;
            let store = CompactStore::decode(&s.store).map_err(|e| anyhow!("store: {e:?}"))?;
            let bytes = std::fs::read(update)?;
            let (u, _) = if *finality { ferrata_advance::decode_finality_update(&bytes, &s.config)? } else { ferrata_advance::decode_update(&bytes, &s.config)? };
            let mut candidates: Vec<SyncCommittee> = Vec::new();
            for c in committees {
                let (kind, path) = c.split_once(':').ok_or_else(|| anyhow!("--committee {c}: bootstrap:FILE or update:FILE"))?;
                let b = std::fs::read(path)?;
                candidates.push(match kind {
                    "bootstrap" => {
                        let fork = s.config.fork().unwrap_or(LcFork::Electra);
                        let boot = [fork, LcFork::Electra, LcFork::Gloas]
                            .into_iter()
                            .find_map(|f| LightClientBootstrap::decode(&b, f, s.config.sync_committee_size as usize).ok())
                            .ok_or_else(|| anyhow!("{path}: not a bootstrap"))?;
                        boot.current_sync_committee
                    }
                    "update" => ferrata_advance::decode_update(&b, &s.config)?.0.next_sync_committee,
                    other => bail!("--committee {other}: bootstrap or update"),
                });
            }
            println!("advance: update attested at slot {}, signed at slot {}, finalizing slot {}; the store is at slot {}",
                u.attested_header.beacon.slot, u.signature_slot, u.finalized_header.beacon.slot, store.finalized.slot);
            let mut verdict = None;
            for c in &candidates {
                match ferrata_advance::evaluate(&s.config, &id, &store, &u, Some(c), Some(c)) {
                    ferrata_advance::Verdict::Useful(p) => {
                        verdict = Some(Ok(p));
                        break;
                    }
                    ferrata_advance::Verdict::NoChange => verdict = Some(Err("no change: it would not move the store (duplicate, stale or of an earlier period)".to_string())),
                    ferrata_advance::Verdict::Unverifiable => {
                        verdict.get_or_insert(Err("unverifiable: its signing committee is not known to the store".to_string()));
                    }
                    ferrata_advance::Verdict::Invalid(e) => {
                        verdict.get_or_insert(Err(format!("invalid: {e}")));
                    }
                }
            }
            let planned = match verdict {
                Some(Ok(p)) => p,
                Some(Err(e)) => {
                    println!("advance: the program's checks, natively: {e}");
                    // Send its Open anyway in a dry run, so that the zone's code is shown beside the verdict.
                    let layout = ferrata_advance::layout_for(&s.config, store.fork, &u).unwrap_or(store.fork);
                    let compact = ferrata_core::witness::CompactUpdate::of(&u);
                    let witness = candidates.first().map(|c| ferrata_advance::signer_witness(c, &u)).unwrap_or_default();
                    let store_u = steps::store_for_update(&store, layout, u.attested_header.beacon.slot, &s.config.spec()).unwrap_or(store.clone());
                    let jid = job_id(&id, &store_u.digest(), &compact.digest(layout));
                    // The program's step 1 natively (what `Open` runs): its code is the verdict the zone must match.
                    let native = steps::step1(&store_u, &compact, &witness, u.signature_slot, &s.config.spec(),
                        &s.config.genesis_validators_root, s.config.adoption_parts)
                        .err()
                        .map(|e| u32::from(e.code()));
                    match native {
                        Some(c) => println!("advance: the program's step 1, natively: refused with {c} {}", code_name(c)),
                        None => println!("advance: the program's step 1, natively: passes (a later step or the policy refuses)"),
                    }
                    let open = Step {
                        label: "Open".into(),
                        root: instance_actor(program, &id),
                        message: borsh::to_vec(&Message::Open { update: compact.encode(layout), signers: witness.encode() })?,
                        actors: vec![instance_actor(program, &id), job_actor(program, &id, &jid)],
                        evidence: vec![],
                    };
                    let st = transact(&mut z, &a, &run, "advance", vec![open], native).await?;
                    return Ok(if st == 0 { 1 } else { st });
                }
                None => bail!("--committee: at least one full committee the store summarises"),
            };
            println!("advance: useful: job {}, layout {:?}, {}", hx(&planned.jid), planned.layout,
                format!("advanced {}, adopted {}, rotated {}", planned.outcome.advanced, planned.outcome.adopted, planned.outcome.rotated));
            let mut list = Vec::new();
            for step in ferrata_advance::job_steps(&s.config, &planned) {
                let t = ferrata_advance::job_step(program, &id, &s.config, &planned, step)?;
                list.push(Step { label: step.name(), root: t.root, message: t.message, actors: t.actors, evidence: vec![] });
            }
            let st = transact(&mut z, &a, &run, "advance", list, Some(0)).await?;
            if a.send && a.yes && st == 0 {
                let after = instance_state(&z, program, &id).await?;
                let st2 = CompactStore::decode(&after.store).map_err(|e| anyhow!("store: {e:?}"))?;
                println!("advance: the instance's finalized slot is now {} (jobs applied {})", st2.finalized.slot, after.applied);
                run.log(json!({ "what": "advance", "phase": "after", "finalized_slot": st2.finalized.slot, "applied": after.applied }));
            }
            Ok(st)
        }
        AttestCmd::Wallet { .. } => unreachable!("handled before the zone is opened"),
        AttestCmd::State { instance, slot } => {
            let id = root_arg(instance)?;
            let actor = match slot {
                Some(s) => header_actor(program, &id, *s),
                None => instance_actor(program, &id),
            };
            let bytes = z.actor_state(actor).await?;
            println!("{}", hx(&bytes));
            Ok(if bytes.is_empty() { 1 } else { 0 })
        }
        AttestCmd::Receipt { txs } => {
            // Wait (up to --settle-timeout) until every transaction is in a block the follower has applied.
            let wanted: Vec<String> = txs.iter().map(|t| t.trim().trim_start_matches("0x").to_lowercase()).collect();
            let started = Instant::now();
            while !wanted.iter().all(|w| z.follower.receipts.keys().any(|h| h.trim_start_matches("0x").to_lowercase() == *w))
                && started.elapsed() < Duration::from_secs(a.settle_timeout)
            {
                tokio::time::sleep(Duration::from_secs(2)).await;
                z.sync().await?;
            }
            let (tip, _) = z.tip();
            let mut status = 0;
            for t in txs {
                let want = t.trim().trim_start_matches("0x").to_lowercase();
                match z.follower.receipts.iter().find(|(h, _)| h.trim_start_matches("0x").to_lowercase() == want) {
                    Some((h, r)) => {
                        let o = Outcome { reverted: r.reverted, exit_code: r.exit_code, gas_exec: r.gas_exec, gas_stor: r.gas_stor, events: r.events, event_data: vec![] };
                        println!("receipt {h}: block {}: {}", r.block_id, o.show());
                        run.log(json!({ "what": "receipt", "tx": h, "block": r.block_id, "reverted": r.reverted, "exit_code": r.exit_code,
                                        "gas_exec": r.gas_exec, "gas_stor": r.gas_stor, "events": r.events }));
                    }
                    None => {
                        println!("receipt {t}: not in a block the follower applied (to block {tip})");
                        status = 1;
                    }
                }
            }
            Ok(status)
        }
        AttestCmd::Attest { instance, record_slot, eth, block, storage, account, log, header_rlp } => {
            let id = root_arg(instance)?;
            let header = header_actor(program, &id, *record_slot);
            let rec_bytes = z.actor_state(header).await?;
            ensure!(!rec_bytes.is_empty(), "no record header({}, {record_slot}) on the zone", hx(&id));
            let record: RecordState = borsh::from_slice(&rec_bytes).map_err(|e| anyhow!("record: {e}"))?;
            let bh = format!("0x{block:x}");
            let mut req = Request { instance_id: id, chain_id: chain_id.unwrap_or(record.chain_id), ..Request::default() };
            if let Some(h) = header_rlp {
                req.header = Some(std::fs::read(h)?);
            }
            let mut proofs: BTreeMap<[u8; 20], AccountProof> = BTreeMap::new();
            let mut wanted: BTreeMap<[u8; 20], Vec<[u8; 32]>> = BTreeMap::new();
            for s in storage {
                let parts: Vec<&str> = s.split(':').collect();
                ensure!(parts.len() == 2 || parts.len() == 4, "--storage {s}: ADDRESS:SLOT[:CMP:VALUE]");
                wanted.entry(addr(parts[0])?).or_default().push(word(parts[1])?);
            }
            for s in account {
                wanted.entry(addr(s.split(':').next().unwrap())?).or_default();
            }
            for (address, slots) in &wanted {
                let params = json!([hx(address), slots.iter().map(|k| hx(k)).collect::<Vec<_>>(), bh]);
                proofs.insert(*address, account_proof(&eth_call(eth, "eth_getProof", params)?));
            }
            let mut by_account: BTreeMap<[u8; 20], AccountQuery> = BTreeMap::new();
            for s in account {
                let parts: Vec<&str> = s.split(':').collect();
                let a0 = addr(parts[0])?;
                let p = &proofs[&a0];
                let conditions = if parts.get(1) == Some(&"codehash") { vec![Condition { field: Field::CodeHash, cmp: Cmp::Eq, rhs: p.code_hash }] } else { vec![] };
                by_account.insert(a0, AccountQuery { address: a0, proof: p.proof.clone(), conditions, slots: vec![] });
            }
            for s in storage {
                let parts: Vec<&str> = s.split(':').collect();
                let a0 = addr(parts[0])?;
                let slot = word(parts[1])?;
                let p = &proofs[&a0];
                let sp = p.storage.iter().find(|x| x.key == slot).ok_or_else(|| anyhow!("no proof for slot {} of {}", hx(&slot), hx(&a0)))?;
                let conditions = if parts.len() == 4 { vec![Condition { field: Field::Value, cmp: cmp(parts[2])?, rhs: word(parts[3])? }] } else { vec![] };
                by_account
                    .entry(a0)
                    .or_insert_with(|| AccountQuery { address: a0, proof: p.proof.clone(), conditions: vec![], slots: vec![] })
                    .slots
                    .push(SlotQuery { slot, proof: sp.proof.clone(), conditions });
            }
            req.accounts = by_account.into_values().collect();
            if !log.is_empty() {
                let v: Value = serde_json::from_slice(&eth_call(eth, "eth_getBlockReceipts", json!([bh]))?)?;
                let receipts = v["result"].as_array().ok_or_else(|| anyhow!("eth_getBlockReceipts: no receipts"))?.clone();
                let trie = Trie::new(receipts.iter().enumerate().map(|(i, r)| {
                    let (k, n) = encode_u64(i as u64);
                    (k[..n].to_vec(), receipt_rlp(r))
                }));
                println!("attest: {} receipts of block {block} rebuild the receipts root {}", receipts.len(), hx(&trie.root()));
                for l in log {
                    let parts: Vec<&str> = l.split(':').collect();
                    ensure!(parts.len() >= 2, "--log {l}: TX:LOG[:emitter=ADDRESS]");
                    let (tx, li): (u64, u64) = (parts[0].parse()?, parts[1].parse()?);
                    let mut conditions = Vec::new();
                    if let Some(e) = parts.get(2).and_then(|x| x.strip_prefix("emitter=")) {
                        conditions.push(Condition { field: Field::Emitter, cmp: Cmp::Eq, rhs: address_word(&addr(e)?) });
                    }
                    let (k, n) = encode_u64(tx);
                    req.logs.push(LogQuery { tx_index: tx, log_index: li, proof: trie.proof(&k[..n]), conditions });
                }
            }
            let h = record.header().map_err(|e| anyhow!("record: {e:?}"))?;
            let verdict = wa::evaluate(&record.instance_id, record.chain_id, &h, &req);
            let expected = match &verdict {
                Ok(at) => {
                    println!("attest: verdict (the program's function, natively): {} statement(s) under block {} ({})", at.statements.len(), at.block_number, hx(&at.block_hash));
                    for st in &at.statements {
                        println!("  statement id {}", hx(&st.id));
                    }
                    0
                }
                Err(c) => {
                    println!("attest: verdict (the program's function, natively): refused with {c} {}", code_name(u32::from(*c)));
                    u32::from(*c)
                }
            };
            run.log(json!({ "what": "attest", "phase": "verdict", "code": expected, "statements": verdict.as_ref().map(|at| at.statements.iter().map(|s| hx(&s.id)).collect::<Vec<_>>()).ok() }));
            let message = borsh::to_vec(&Message::Attest { request: req })?;
            let step = Step { label: "Attest".into(), root: header, message, actors: vec![header], evidence: vec![] };
            transact(&mut z, &a, &run, "attest", vec![step], Some(expected)).await
        }
    }
}

