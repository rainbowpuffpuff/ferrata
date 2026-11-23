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
//! the zone disagree, or the verdict and the dry run; 2 a usage error; 4 the Ethereum endpoint failed (refused,
//! timed out, cut its answer, answered with an error, or serves another chain than the record's) before anything was
//! sent.
//!
//! **Interrupted and run again (RFP-021 R.5).** Before it sends, a command writes every transaction it is about to
//! send, signed, to a journal in `--state-dir` (`sent/<intent>.json`, keyed by what the command asks for as parsed, not
//! by its spelling or the endpoints that serve it), and records each attempt and receipt there. Run again after an
//! interruption, the same command signs nothing new: a transaction the zone applied is reported from its receipt, one
//! in a block not yet final is waited for, one in no block yet is sent again byte for byte (same hash and nonce: LEZ
//! applies it at most once), unless its window has ended or is about to (then nothing is sent, and the journal of a
//! command nothing of which was applied is cleared); one whose nonce another transaction took is reported not applied.
//! Run again after it finished (or after the sequencer turned a first submission away, which ends a run; a resend
//! turned away leaves it unfinished, as the earlier copy may still be pending), it reports what the earlier run did and
//! sends nothing (`--again` sends a new one). One run at a time holds a state directory (its file `lock`); the
//! follower's state and the journals are written whole or not at all, the directory synced. An Ethereum endpoint that
//! fails is retried (`--eth-retries`, each request bounded by `--eth-timeout`), then reported with nothing sent.
//!
//! **A stale checkpoint (RFP-021 U.5).** LEZ refuses a transaction executed outside the timestamp validity window its
//! program gave it with its own text ("Execution outside of the validity window"). `Init` and `Open` carry the window
//! that ends with the weak-subjectivity period of the checkpoint (for `Open`, the instance's newest finalized header);
//! past that end, the tool shows the refusal as code 53's message, naming the checkpoint, the time its period ended
//! and the remedy (a new instance from a recent bootstrap), and sends no step whose window has ended, ends within a
//! minute of the clock that judges it (the later of the zone's newest block's time and this machine's clock,
//! `FERRATA_ATTEST_CLOCK_MS` in tests), or has not opened yet by the dry run (made at the zone's clock, plus 10 s). A
//! window not yet open is LEZ's text as it is, with the window's start.

#![allow(clippy::too_many_arguments)]

#[path = "../../lez/live/driver/src/follower.rs"]
#[allow(dead_code)]
mod follower;
#[path = "../../lez/live/driver/src/plan.rs"]
#[allow(dead_code, unused_imports)]
mod plan;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, ensure, Context as _, Result};
use chain_state::apply::{opening_fee_state, settle_transaction};
use common::{block::BedrockStatus, block::Block, transaction::LeeTransaction};
use fee_core::BlockFeeSummary;
use ferrata_core::{
    fork::LcFork,
    rlp::encode_u64,
    steps,
    types::{LightClientBootstrap, SyncCommittee},
    witness::{uncompressed_keys, CompactBootstrap, CompactStore, ExecutionSummary},
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
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};
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

/// LEZ's text for a transaction executed outside the timestamp validity window its program gave it
/// (`LeeError::OutOfValidityWindow`, LEZ v0.4.0-rc1 `lee/state_machine/src/error.rs`): charged the whole gas limit.
pub const WINDOW_REFUSAL: &str = "Execution outside of the validity window";

/// The program's code for a store past its weak-subjectivity period (`OutsideWeakSubjectivityPeriod`).
pub const OUTSIDE_WS_PERIOD: u32 = 53;

/// The timestamp validity window the program gives a step (`Init`: until the end of the checkpoint's
/// weak-subjectivity period; `Open`: from the update's signature slot to the end of the period of the instance's newest
/// finalized header), as `ferrata_advance::init_window` and `open_window` compute it, and what it is the window of.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowNote {
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
    /// What went stale when the window ends, for the message: `checkpoint 0x… (slot S)` or `the instance's newest
    /// finalized header 0x… (slot S)`.
    pub checkpoint: String,
}

/// Unix milliseconds as `YYYY-MM-DD HH:MM:SS UTC`.
pub fn utc(ms: u64) -> String {
    let secs = ms / 1000;
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Where a block's time fell against a step's window, when LEZ refused the step for its window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowSide {
    /// At or after the window's end: the checkpoint is stale by the zone's clock (code 53's message).
    Ended,
    /// Before the window's start.
    NotOpen,
    /// Inside the window as this tool computes it (LEZ's text as it is).
    Inside,
    /// A step without a window this tool knows.
    Unknown,
}

/// Code 53's message for a step whose window ended at `end`: the checkpoint is stale. `at_ms` is the time it was judged
/// at and `clock` what that time is (a block's, a dry run's); `lez` what LEZ says or would say.
fn stale_message(w: &WindowNote, end: u64, at_ms: u64, clock: &str, lez: &str) -> String {
    format!(
        "refused with {OUTSIDE_WS_PERIOD} {}: the {} is stale: its weak-subjectivity period ended at {}, and {clock} is {} \
         ({lez}). Nothing can extend an instance from it any more: recover from a new bootstrap, a new instance from a \
         recent finalized checkpoint with `ferrata attest init` (operator guide §10, \"Instance past its \
         weak-subjectivity bound\")",
        code_name(OUTSIDE_WS_PERIOD),
        w.checkpoint,
        utc(end),
        utc(at_ms)
    )
}

/// What to tell the user when LEZ refused a step with `reason` at `at_ms` (`clock`: what that time is, a block's or a
/// dry run's), given the step's window: `None` unless `reason` is LEZ's window refusal ([`WINDOW_REFUSAL`]). Past the
/// window's end the checkpoint is stale: code 53 (`OutsideWeakSubjectivityPeriod`) and its message, the checkpoint, when
/// its period ended, and the remedy. Before the window's start (an update sent before its signature slot), or inside
/// the window as this tool computes it, LEZ's text is shown as it is, with the window: no code is put on it.
pub fn window_refusal(
    reason: &str,
    window: Option<&WindowNote>,
    at_ms: u64,
    clock: &str,
) -> Option<(WindowSide, String)> {
    if !reason.contains(WINDOW_REFUSAL) {
        return None;
    }
    let Some(w) = window else {
        return Some((WindowSide::Unknown, format!("refused by LEZ: \"{reason}\" (no window of it is known here)")));
    };
    if let Some(end) = w.end_ms.filter(|e| at_ms >= *e) {
        return Some((WindowSide::Ended, stale_message(w, end, at_ms, clock, &format!("LEZ's refusal: \"{reason}\""))));
    }
    if let Some(start) = w.start_ms.filter(|s| at_ms < *s) {
        return Some((
            WindowSide::NotOpen,
            format!(
                "refused by LEZ: \"{reason}\": its window opens at {} (the update's signature slot), and {clock} is {}; \
                 it can be sent from then (with --again once a run has sent it)",
                utc(start),
                utc(at_ms)
            ),
        ));
    }
    Some((
        WindowSide::Inside,
        format!(
            "refused by LEZ: \"{reason}\" (the window this tool computes, {} to {}, holds {clock}, {})",
            w.start_ms.map_or("open".into(), utc),
            w.end_ms.map_or("open".into(), utc),
            utc(at_ms)
        ),
    ))
}

/// How long before a window's end a step is no longer sent: the time a transaction may take to reach a block (a
/// sequencer's block interval, a retry), as `ferrata-updater`'s `WINDOW_MARGIN_MS`.
pub const WINDOW_MARGIN_MS: u64 = 60_000;

/// The variable that sets this machine's clock for tests and rehearsals.
pub const CLOCK_VAR: &str = "FERRATA_ATTEST_CLOCK_MS";

/// `FERRATA_ATTEST_CLOCK_MS`: `None` when unset; else the time it sets, Unix milliseconds, or why it is refused (not a
/// whole number of milliseconds). A time before the zone's clock (`0`, for one) leaves a window's end to the zone's
/// clock alone, which is what the tests do; `run` refuses a bad value before anything is read and names any value it
/// takes.
fn clock_override() -> Option<std::result::Result<u64, String>> {
    match std::env::var(CLOCK_VAR) {
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(_)) => Some(Err(format!("{CLOCK_VAR} is not text"))),
        Ok(v) => Some(v.trim().parse::<u64>().map_err(|_| format!("{CLOCK_VAR}={v:?} is not a time in milliseconds"))),
    }
}

/// This machine's clock, Unix milliseconds: the system clock, or the time `FERRATA_ATTEST_CLOCK_MS` sets (checked by
/// `run` before any use).
fn machine_ms() -> u64 {
    match clock_override() {
        Some(Ok(ms)) => ms,
        _ => SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64),
    }
}

/// The time a window's end is judged at, and what that time is: the later of the zone's clock (its newest block's time)
/// and this machine's, as `ferrata-updater` judges its windows. A zone that has stamped no block since a window ended
/// (a sequencer back from a stall, before its first new block) is caught by this machine's clock; a skew is covered by
/// the [`WINDOW_MARGIN_MS`] margin. A window's start is judged at the zone's clock only: a machine ahead of the zone
/// must not send a step the zone's next block would refuse.
fn end_clock(zone_ms: u64) -> (u64, &'static str) {
    let machine = machine_ms();
    if machine > zone_ms {
        (machine, "this machine's clock (the zone has stamped no later block)")
    } else {
        (zone_ms, "the zone's clock (its newest block's time)")
    }
}

/// Whether a step with `window` may be sent when the clock that judges its end (`clock`: which one, see [`end_clock`])
/// reads `now_ms`: `Err` when the window has ended (code 53's message) or ends within [`WINDOW_MARGIN_MS`] (a
/// transaction sent now would likely reach a block after it, be refused and be charged the whole gas limit). A window
/// not open yet is judged by the dry run, at the zone's clock.
pub fn send_check(
    window: Option<&WindowNote>,
    now_ms: u64,
    clock: &str,
) -> std::result::Result<(), (WindowSide, String)> {
    let Some(w) = window else { return Ok(()) };
    let Some(end) = w.end_ms else { return Ok(()) };
    if now_ms >= end {
        let lez = format!("LEZ refuses a transaction executed then: \"{WINDOW_REFUSAL}\", charged the whole gas limit");
        return Err((WindowSide::Ended, stale_message(w, end, now_ms, clock, &lez)));
    }
    if now_ms.saturating_add(WINDOW_MARGIN_MS) >= end {
        return Err((
            WindowSide::Inside,
            format!(
                "the weak-subjectivity period of the {} ends at {}, less than {} s after {clock}, {}: a transaction \
                 sent now would likely reach a block after it, be refused (\"{WINDOW_REFUSAL}\") and be charged the \
                 whole gas limit; recover from a new bootstrap (`ferrata attest init`, operator guide §10)",
                w.checkpoint,
                utc(end),
                WINDOW_MARGIN_MS / 1000,
                utc(now_ms)
            ),
        ));
    }
    Ok(())
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
    /// Send what the dry run shows (with --yes); may follow the command. Needs --state-dir, where the journal of what
    /// is sent is kept.
    #[arg(long, global = true)]
    send: bool,
    #[arg(long, global = true)]
    yes: bool,
    /// Send it again although an earlier run of the same command already did (its journal in --state-dir says so).
    #[arg(long, global = true)]
    again: bool,
    /// Seconds one request to the Ethereum endpoint (`attest --eth`) may take.
    #[arg(long, global = true, default_value_t = 60)]
    eth_timeout: u64,
    /// Retries of an Ethereum request whose connection failed, timed out, was cut or answered HTTP 5xx or 429.
    #[arg(long, global = true, default_value_t = 2)]
    eth_retries: u32,
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
    /// The follower's height when its state was last written to `state_dir`.
    saved_height: Option<u64>,
}

/// The follower's state in `--state-dir`: `borsh(snapshot) ‖ borsh(blocks)` in one file, replaced whole.
pub const FOLLOWER_STATE: &str = "follower.state";

/// Write `bytes` to `path` whole or not at all: a temporary file of this process beside it, synced, renamed over it,
/// and the directory synced so that the rename survives a power loss.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write as _;
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let mut f = std::fs::File::create(&tmp).with_context(|| format!("writing {}", tmp.display()))?;
    f.write_all(bytes)?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, path).with_context(|| format!("renaming {} to {}", tmp.display(), path.display()))?;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::File::open(dir).and_then(|d| d.sync_all()).with_context(|| format!("syncing {}", dir.display()))?;
    }
    Ok(())
}

/// An exclusive lock on `dir/lock`, held while the returned file lives: one run of the tool at a time per state
/// directory (two would interleave their writes of the follower's state and of the journals). `None` if another
/// run holds it.
pub fn lock_state_dir(dir: &Path) -> Result<Option<std::fs::File>> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join("lock");
    let f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .with_context(|| format!("opening {}", path.display()))?;
    match f.try_lock() {
        Ok(()) => Ok(Some(f)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(e)) => Err(anyhow!("locking {}: {e}", path.display())),
    }
}

/// The follower kept in `dir`: [`FOLLOWER_STATE`], or the two files earlier versions wrote (`follower.borsh`,
/// `blocks.borsh`), or none.
fn load_follower(dir: Option<&Path>) -> Result<Follower> {
    use borsh::BorshDeserialize;
    let Some(d) = dir else { return Ok(Follower::new()) };
    if d.join(FOLLOWER_STATE).exists() {
        let bytes = std::fs::read(d.join(FOLLOWER_STATE))?;
        let mut rest = &bytes[..];
        let snapshot = <Vec<u8> as BorshDeserialize>::deserialize(&mut rest).context("the follower's saved state")?;
        let blocks = <Vec<Block> as BorshDeserialize>::deserialize(&mut rest).context("the follower's saved blocks")?;
        ensure!(rest.is_empty(), "the follower's saved state has trailing bytes");
        return Follower::restore(&snapshot, blocks);
    }
    if d.join("follower.borsh").exists() {
        let snapshot = std::fs::read(d.join("follower.borsh"))?;
        let blocks: Vec<Block> = borsh::from_slice(&std::fs::read(d.join("blocks.borsh"))?)?;
        return Follower::restore(&snapshot, blocks);
    }
    Ok(Follower::new())
}

impl Zone {
    async fn open(rpc: &str, wait_final: bool, state_dir: Option<PathBuf>) -> Result<Self> {
        let client = SequencerClientBuilder::default().build(rpc).with_context(|| format!("the sequencer at {rpc}"))?;
        let follower = load_follower(state_dir.as_deref())?;
        let saved_height = state_dir.as_ref().filter(|d| d.join(FOLLOWER_STATE).exists()).map(|_| follower.height());
        let mut z = Self { rpc: rpc.to_string(), client, follower, wait_final, state_dir, saved_height };
        z.sync().await?;
        Ok(z)
    }

    /// The time of the sequencer's newest block (final or not), Unix milliseconds.
    async fn head_time(&self) -> Result<u64> {
        let head = self.client.get_last_block_id().await.map_err(|e| anyhow!("getLastBlockId on {}: {e}", self.rpc))?;
        let block = self.client.get_block(head).await.map_err(|e| anyhow!("getBlock {head}: {e}"))?;
        Ok(block.map_or(0, |b| b.header.timestamp))
    }

    /// The zone's clock as a client can read it: the newest block's time (the sequencer stamps one every block
    /// interval), or the followed tip's if that is later. The sequencer's RPC serves no other clock.
    async fn clock(&self) -> Result<u64> {
        Ok(self.tip().1.max(self.head_time().await?))
    }

    /// The payer's nonce at the sequencer's head (it counts transactions in blocks not yet final).
    async fn head_nonce(&self, id: AccountId) -> Result<u128> {
        let a = self.client.get_account(id).await.map_err(|e| anyhow!("getAccount: {e}"))?;
        Ok(a.map_or(0, |a| a.nonce.0))
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
        // Written when the follower moved (not at every poll while a transaction is awaited).
        if let Some(d) = &self.state_dir {
            if self.saved_height != Some(self.follower.height()) {
                std::fs::create_dir_all(d)?;
                let mut bytes = borsh::to_vec(&self.follower.snapshot()?)?;
                bytes.extend(borsh::to_vec(&self.follower.blocks)?);
                write_atomic(&d.join(FOLLOWER_STATE), &bytes)?;
                self.saved_height = Some(self.follower.height());
            }
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Outcome {
    reverted: bool,
    exit_code: Option<u32>,
    /// LEZ's message for a revert (for a program's refusal: "Program exited with non-zero code …").
    #[serde(default)]
    reason: Option<String>,
    gas_exec: u64,
    gas_stor: u64,
    events: u64,
    #[serde(skip)]
    event_data: Vec<Vec<u8>>,
    /// The time of the block it ran in (a dry run: the time it was dry-run at), Unix milliseconds.
    #[serde(default)]
    at_ms: u64,
    /// A dry run's outcome (its time is the dry run's), not a block's.
    #[serde(default)]
    dry: bool,
}

impl Outcome {
    /// LEZ's validity-window refusal of a step with `window`, as [`window_refusal`] words it.
    fn window(&self, window: Option<&WindowNote>) -> Option<(WindowSide, String)> {
        match (self.reverted, self.exit_code, &self.reason) {
            (true, None, Some(r)) => {
                let clock = if self.dry { "the dry run's time" } else { "the zone's clock (the block's time)" };
                window_refusal(r, window, self.at_ms, clock)
            }
            _ => None,
        }
    }
    fn show_for(&self, window: Option<&WindowNote>) -> String {
        let verdict = match (self.reverted, self.exit_code, self.window(window)) {
            (false, _, _) => "applied".to_string(),
            (true, Some(c), _) => format!("refused with {c} {}", code_name(c)),
            (true, None, Some((_, message))) => message,
            (true, None, None) => match &self.reason {
                Some(r) => format!("refused by LEZ: \"{r}\""),
                None => "reverted without a code".to_string(),
            },
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
    /// Refused by LEZ because the step's window had ended (the checkpoint is stale by the zone's clock).
    fn stale(&self, window: Option<&WindowNote>) -> bool {
        matches!(self.window(window), Some((WindowSide::Ended, _)))
    }
    /// Refused by LEZ for the block's time: the window had ended or was not open yet.
    fn timed_out_of_window(&self, window: Option<&WindowNote>) -> bool {
        matches!(self.window(window), Some((WindowSide::Ended | WindowSide::NotOpen, _)))
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
        reason,
        gas_exec: summary.gas_used_exec,
        gas_stor: summary.gas_used_stor,
        events: events.len() as u64,
        event_data: events.iter().map(|e| e.event.data.clone()).collect(),
        at_ms: ts,
        dry: true,
    })
}

/// The zone's receipt of a transaction, from the follower.
fn outcome_of(r: &follower::Receipt) -> Outcome {
    Outcome {
        reverted: r.reverted,
        exit_code: r.exit_code,
        reason: r.reason.clone(),
        gas_exec: r.gas_exec,
        gas_stor: r.gas_stor,
        events: r.events,
        event_data: vec![],
        at_ms: r.timestamp_ms,
        dry: false,
    }
}

/// The receipt matches the dry run: exit code, execution and storage gas, events. A revert's reason is not compared:
/// LEZ's window refusal depends on the block's time.
fn same(zone: &Outcome, dry: &Outcome) -> bool {
    zone.reverted == dry.reverted
        && zone.exit_code == dry.exit_code
        && zone.gas_exec == dry.gas_exec
        && zone.gas_stor == dry.gas_stor
        && zone.events == dry.events
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
    /// The timestamp validity window the program gives it (`Init`, `Open`), for LEZ's window refusal.
    window: Option<WindowNote>,
}

// ---- the journal of what a command sends ---------------------------------------------------------------------------

/// One transaction a command sent, or is about to send: the signed bytes, the dry run, and the zone's receipt once read.
#[derive(Clone, Serialize, Deserialize)]
struct Sent {
    label: String,
    /// `0x` + borsh of the signed `LeeTransaction`.
    tx: String,
    hash: String,
    /// The payer's nonce it is signed at (decimal).
    nonce: String,
    dry_run: Outcome,
    #[serde(default)]
    window: Option<WindowNote>,
    /// Set, and written, just before it is handed to the sequencer: it may have been sent.
    #[serde(default)]
    attempted: bool,
    #[serde(default)]
    receipt: Option<Receipt>,
    /// Never applied: the payer's nonce it was signed at was used by another transaction in a final block.
    #[serde(default)]
    superseded: bool,
    /// Never applied: the sequencer refused it at submission, with this answer (it was not admitted).
    #[serde(default)]
    refused: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Receipt {
    block: u64,
    outcome: Outcome,
}

/// What a sending command sent, kept in `--state-dir/sent/<intent>.json` and written before anything is sent.
#[derive(Clone, Serialize, Deserialize)]
struct Journal {
    version: u32,
    /// `init`, `advance` or `attest`.
    what: String,
    /// What the command asks for, as its key was derived ([`intent`]).
    intent: String,
    payer: String,
    /// The verdict's code for the last step (0: applied), as the command computed it.
    expected: Option<u32>,
    steps: Vec<Sent>,
    /// The exit status once every receipt was read; `None` while the run is unfinished.
    status: Option<i32>,
}

/// Where a command's journal lives, and the text its key is the hash of.
#[derive(Clone)]
struct Slot {
    path: PathBuf,
    intent: String,
}

impl Slot {
    fn load(&self) -> Result<Option<Journal>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let j: Journal = serde_json::from_slice(&std::fs::read(&self.path)?)
            .with_context(|| format!("the journal {}", self.path.display()))?;
        ensure!(j.intent == self.intent, "the journal {} is of another command: {}", self.path.display(), j.intent);
        Ok(Some(j))
    }
    fn save(&self, j: &Journal) -> Result<()> {
        if let Some(d) = self.path.parent() {
            std::fs::create_dir_all(d)?;
        }
        write_atomic(&self.path, &serde_json::to_vec_pretty(j)?)
    }
}

/// The text naming what a sending command asks for, from its parsed arguments (not their spelling: an address or a
/// word with or without `0x`, padded or not, in any case, statements in any order), files by their SHA-256, and not the
/// endpoints serving them; and its key, the text's SHA-256. `None` for a command that sends nothing.
fn intent(a: &AttestArgs, program: AccountId, chain_id: Option<u64>) -> Result<Option<(String, String)>> {
    let file = |p: &PathBuf| -> Result<String> {
        let b = std::fs::read(p).with_context(|| format!("reading {}", p.display()))?;
        Ok(format!("sha256:{}", hex::encode(Sha256::digest(&b))))
    };
    let text = match &a.cmd {
        AttestCmd::Init { preset, bootstrap, checkpoint, layout, parts } => format!(
            "init program={program} preset={} layout={} parts={parts} checkpoint={} bootstrap={}",
            preset.trim().to_lowercase(),
            layout.trim().to_lowercase(),
            hx(&root_arg(checkpoint)?),
            file(bootstrap)?
        ),
        AttestCmd::Advance { instance, update, finality, .. } => format!(
            "advance program={program} instance={} finality={finality} update={}",
            hx(&root_arg(instance)?),
            file(update)?
        ),
        AttestCmd::Attest { instance, record_slot, block, storage, account, log, header_rlp, .. } => {
            let mut statements = Vec::new();
            for s in storage {
                let p: Vec<&str> = s.split(':').collect();
                ensure!(p.len() == 2 || p.len() == 4, "--storage {s}: ADDRESS:SLOT[:CMP:VALUE]");
                let mut t = format!("storage {} {}", hx(&addr(p[0])?), hx(&word(p[1])?));
                if p.len() == 4 {
                    cmp(p[2])?;
                    t.push_str(&format!(" {} {}", p[2], hx(&word(p[3])?)));
                }
                statements.push(t);
            }
            for s in account {
                let p: Vec<&str> = s.split(':').collect();
                let flag = if p.get(1) == Some(&"codehash") { " codehash" } else { "" };
                statements.push(format!("account {}{flag}", hx(&addr(p[0])?)));
            }
            for l in log {
                let p: Vec<&str> = l.split(':').collect();
                ensure!(p.len() >= 2, "--log {l}: TX:LOG[:emitter=ADDRESS]");
                let (tx, li): (u64, u64) = (p[0].parse()?, p[1].parse()?);
                let emitter = match p.get(2).and_then(|x| x.strip_prefix("emitter=")) {
                    Some(e) => format!(" emitter {}", hx(&addr(e)?)),
                    None => String::new(),
                };
                statements.push(format!("log {tx} {li}{emitter}"));
            }
            statements.sort();
            format!(
                "attest program={program} instance={} record_slot={record_slot} block={block} chain_id={} \
                 statements=[{}] header_rlp={}",
                hx(&root_arg(instance)?),
                chain_id.map_or("record".into(), |c| c.to_string()),
                statements.join("; "),
                header_rlp.as_ref().map(file).transpose()?.unwrap_or_else(|| "none".into())
            )
        }
        _ => return Ok(None),
    };
    let key = hex::encode(Sha256::digest(text.as_bytes()));
    Ok(Some((key, text)))
}

/// The exit status of a run whose last outcome is `last`, given the verdict's code for it and whether every receipt
/// equalled its dry run. LEZ's refusal past a checkpoint's weak-subjectivity period is a refusal (3): the program's
/// native checks do not see the clock, and a dry run made at an earlier time could not either.
fn status_of(last: &Outcome, window: Option<&WindowNote>, expected: Option<u32>, all_equal: bool) -> i32 {
    if last.timed_out_of_window(window) {
        return 3;
    }
    if !all_equal || expected.is_some_and(|e| last.code() != e) {
        return 1;
    }
    if last.reverted {
        3
    } else {
        0
    }
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
/// the dry run. With `slot`, every transaction is written to the journal, signed, before the first is sent, and each
/// receipt as it is read. Returns the exit status.
async fn transact(
    z: &mut Zone,
    a: &AttestArgs,
    run: &Run,
    what: &str,
    steps: Vec<Step>,
    expected: Option<u32>,
    slot: Option<&Slot>,
) -> Result<i32> {
    let p = payer(&a.key)?;
    let (tip, _) = z.tip();
    // The zone's clock: the newest block's time, which can be ahead of the last final block the follower holds.
    let now = z.clock().await?;
    let mut scratch = z.follower.state.clone();
    let start_nonce = scratch.get_account_by_id(p.id).nonce.0;
    let mut txs = Vec::new();
    let mut dry = Vec::new();
    println!(
        "{what}: dry run of {} transaction(s) on the zone's state at block {tip}, the zone's clock {} (payer {})",
        steps.len(),
        utc(now),
        p.id
    );
    for (i, s) in steps.iter().enumerate() {
        let tx = build(&p, start_nonce + i as u128, s.root, s.message.clone(), s.actors.clone(), s.evidence.clone(), a.gas_limit);
        let o = settle(&mut scratch, &tx, tip + 1 + i as u64, now + 10_000 * (1 + i as u64))?;
        println!("  {:<28} {} bytes; would be {}", s.label, borsh::to_vec(&tx)?.len(), o.show_for(s.window.as_ref()));
        run.log(json!({ "what": what, "step": s.label, "phase": "dry-run", "tx": tx.hash().to_string(), "bytes": borsh::to_vec(&tx)?.len(),
            "reverted": o.reverted, "exit_code": o.exit_code, "reason": o.reason, "gas_exec": o.gas_exec, "gas_stor": o.gas_stor, "events": o.events }));
        let stop = o.reverted;
        txs.push(tx);
        dry.push(o);
        if stop {
            break;
        }
    }
    let last = dry.last().unwrap().clone();
    let last_window = steps[dry.len() - 1].window.clone();
    let status = status_of(&last, last_window.as_ref(), expected, true);
    if let Some(e) = expected {
        if last.timed_out_of_window(last_window.as_ref()) {
            println!(
                "  the program's checks pass natively ({}); LEZ refuses it for the block's time",
                if e == 0 { "applied".into() } else { format!("{e}") }
            );
        } else if last.code() != e {
            println!("  DISAGREE: the verdict says {e}, the zone's execution {}", last.code());
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
    // A step the dry run, made at the zone's clock, shows refused for its window is not sent: LEZ would charge it the
    // whole gas limit. Past the window's end it is refused by every later block too; before its start, until then.
    // Nor is a step whose window ends within the margin.
    if last.stale(last_window.as_ref()) {
        println!(
            "  not sent: LEZ refuses it at any later time too, and would charge it the whole gas limit ({})",
            a.gas_limit
        );
        run.log(json!({ "what": what, "phase": "held", "reason": last.reason, "code": OUTSIDE_WS_PERIOD }));
        return Ok(3);
    }
    if last.timed_out_of_window(last_window.as_ref()) {
        let start = last_window.as_ref().and_then(|w| w.start_ms).map_or("its window opens".into(), utc);
        println!(
            "  not sent: LEZ refuses it until {start}, and would charge it the whole gas limit ({}); run the command \
             again from then",
            a.gas_limit
        );
        run.log(json!({ "what": what, "phase": "held", "reason": last.reason, "until": start }));
        return Ok(3);
    }
    let (end_now, end_by) = end_clock(now);
    for s in steps.iter().take(txs.len()) {
        if let Err((_, message)) = send_check(s.window.as_ref(), end_now, end_by) {
            println!("  {:<28} not sent: {message}", s.label);
            run.log(json!({ "what": what, "phase": "held", "step": s.label, "message": message }));
            return Ok(3);
        }
    }
    let mut journal = Journal {
        version: 1,
        what: what.to_string(),
        intent: slot.map(|s| s.intent.clone()).unwrap_or_default(),
        payer: p.id.to_string(),
        expected,
        steps: txs
            .iter()
            .zip(&dry)
            .enumerate()
            .map(|(i, (tx, d))| -> Result<Sent> {
                Ok(Sent {
                    label: steps[i].label.clone(),
                    tx: hx(&borsh::to_vec(tx)?),
                    hash: tx.hash().to_string(),
                    nonce: (start_nonce + i as u128).to_string(),
                    dry_run: d.clone(),
                    window: steps[i].window.clone(),
                    attempted: false,
                    receipt: None,
                    superseded: false,
                    refused: None,
                })
            })
            .collect::<Result<_>>()?,
        status: None,
    };
    // Written before anything is sent: a run interrupted from here on is finished by running the same command again.
    if let Some(s) = slot {
        s.save(&journal)?;
    }
    send_journal(z, a, run, &p, &mut journal, slot, false).await
}

/// Send the journal's transactions that have no receipt yet, in order, and read each receipt; stop at the first refused.
/// `resuming`: an earlier run may have sent them already, so a transaction is sent (again, byte for byte) only while
/// the sequencer's head shows its nonce unused, and otherwise waited for. Writes each receipt and the status to the
/// journal. Returns the exit status.
async fn send_journal(
    z: &mut Zone,
    a: &AttestArgs,
    run: &Run,
    p: &Payer,
    j: &mut Journal,
    slot: Option<&Slot>,
    resuming: bool,
) -> Result<i32> {
    let what = j.what.clone();
    let mut all_equal = true;
    let mut last: Option<(Outcome, Option<WindowNote>)> = None;
    for i in 0..j.steps.len() {
        let s = j.steps[i].clone();
        if let Some(r) = &s.receipt {
            let equal = same(&r.outcome, &s.dry_run);
            println!(
                "  {:<28} block {} (sent and read by an earlier run): {}",
                s.label,
                r.block,
                r.outcome.show_for(s.window.as_ref())
            );
            all_equal &= equal;
            last = Some((r.outcome.clone(), s.window.clone()));
            if r.outcome.reverted {
                break;
            }
            continue;
        }
        let tx: LeeTransaction = borsh::from_slice(&hex::decode(s.tx.trim_start_matches("0x"))?)?;
        let hash = tx.hash();
        ensure!(
            hash.to_string() == s.hash,
            "{}: the journal's transaction does not hash to its recorded hash",
            s.label
        );
        let nonce: u128 = s.nonce.parse()?;
        let started = Instant::now();
        let mut sent_now = false;
        if resuming {
            z.sync().await?;
            if z.follower.receipt(&hash).is_some() {
                println!(
                    "  {:<28} applied or refused in a final block since the earlier run sent it: nothing sent",
                    s.label
                );
            } else if z.follower.state.get_account_by_id(p.id).nonce.0 > nonce {
                // Handled by the wait below: its nonce is used in a final block, by another transaction.
            } else if z.head_nonce(p.id).await? > nonce {
                println!("  {:<28} in a block of the sequencer not yet final: waiting for it, nothing sent", s.label);
            } else {
                // To be sent (again): not when its window has ended or is about to (it would be charged the gas limit).
                let zone_now = z.clock().await?;
                let (end_now, end_by) = end_clock(zone_now);
                if let Err((side, message)) = send_check(s.window.as_ref(), end_now, end_by) {
                    println!("  {:<28} not sent again: {message}", s.label);
                    run.log(json!({ "what": what, "step": s.label, "phase": "held-on-resume", "message": message }));
                    let applied_before =
                        j.steps[..i].iter().any(|x| x.receipt.as_ref().is_some_and(|r| !r.outcome.reverted));
                    // Past the end by the zone's own clock, a copy still pending is refused too; by this machine's clock
                    // alone (ahead of the zone's) it may still land.
                    let zone_past_end = s.window.as_ref().and_then(|w| w.end_ms).is_some_and(|e| zone_now >= e);
                    match slot {
                        // Nothing of the command was applied and nothing of it can be any more: the journal is cleared.
                        Some(sl) if !applied_before && side == WindowSide::Ended && (zone_past_end || !s.attempted) => {
                            std::fs::remove_file(&sl.path)
                                .with_context(|| format!("removing {}", sl.path.display()))?;
                            println!("  the journal {} is cleared: nothing of this command was applied, and its transactions can never be", sl.path.display());
                        }
                        // The earlier run handed it to the sequencer: a copy may still be pending and land before the
                        // end. The run stays unfinished (no --again while a copy may land).
                        Some(_) if s.attempted => println!(
                            "  the earlier run's submission may still be pending: the run stays unfinished; run the \
                             same command again to read what became of it"
                        ),
                        Some(sl) => {
                            j.status = Some(3);
                            sl.save(j)?;
                        }
                        None => {}
                    }
                    return Ok(3);
                }
                if s.attempted {
                    println!("  {:<28} in no block yet: sending the same signed transaction again (hash {hash}, nonce {nonce}; LEZ applies it at most once)", s.label);
                } else {
                    println!("  {:<28} not sent by the earlier run: sending it as that run signed it (hash {hash}, nonce {nonce})", s.label);
                }
                sent_now = true;
            }
        } else {
            sent_now = true;
        }
        if sent_now {
            j.steps[i].attempted = true;
            if let Some(sl) = slot {
                sl.save(j)?;
            }
            match z.client.send_transaction(tx.clone()).await {
                Ok(got) => ensure!(got == hash, "{}: the sequencer returned another hash", s.label),
                // A resend turned away: the earlier run's submission of these same bytes may still be pending (admitted
                // before the pool filled, for one), so nothing is recorded as refused and the run stays unfinished. Run
                // again, it is found applied, its nonce taken, or in no block (then sent again).
                Err(sequencer_service_rpc::ClientError::Call(e)) if resuming && s.attempted => {
                    let why = format!("{} (code {})", e.message(), e.code());
                    println!("  {:<28} the sequencer answered the resend with: {why}; the earlier run's submission of the same transaction may still be pending, so the run stays unfinished and nothing more is sent: run the same command again to read what became of it", s.label);
                    run.log(json!({ "what": what, "step": s.label, "phase": "resend-turned-away", "tx": s.hash, "answer": why }));
                    return Ok(1);
                }
                // A first submission turned away (too large, its fee, a full pool): it was not admitted and cannot be
                // applied later. The run ends here, recorded as finished: run again, the command reports it; with --again,
                // it is planned and signed anew.
                Err(sequencer_service_rpc::ClientError::Call(e)) => {
                    let why = format!("{} (code {})", e.message(), e.code());
                    println!("  {:<28} REFUSED by the sequencer at submission: {why}; it was not admitted, nothing of it can be applied, and nothing more is sent", s.label);
                    run.log(json!({ "what": what, "step": s.label, "phase": "refused-at-submission", "tx": s.hash, "answer": why }));
                    j.steps[i].refused = Some(why);
                    j.status = Some(1);
                    if let Some(sl) = slot {
                        sl.save(j)?;
                    }
                    return Ok(1);
                }
                // No answer (unreachable, timed out, cut): it may have been admitted. The journal keeps it unfinished, and
                // the same command run again sends it again byte for byte or finds it applied.
                Err(e) => bail!(
                    "{}: no answer from the sequencer ({e}); it may have been admitted: run the same command again to finish",
                    s.label
                ),
            }
        }
        let receipt = loop {
            z.sync().await?;
            if let Some(r) = z.follower.receipt(&hash) {
                break r.clone();
            }
            // Its nonce used in a final block by another transaction: this one can never be applied.
            if z.follower.state.get_account_by_id(p.id).nonce.0 > nonce {
                println!("  {:<28} NOT APPLIED: the payer's nonce {nonce} was used by another transaction in a final block; nothing more is sent", s.label);
                run.log(
                    json!({ "what": what, "step": s.label, "phase": "superseded", "tx": s.hash, "nonce": s.nonce }),
                );
                j.steps[i].superseded = true;
                j.status = Some(1);
                if let Some(sl) = slot {
                    sl.save(j)?;
                }
                return Ok(1);
            }
            ensure!(
                started.elapsed() < Duration::from_secs(a.settle_timeout),
                "{}: {hash} not in a final block after {} s",
                s.label,
                a.settle_timeout
            );
            tokio::time::sleep(Duration::from_secs(1)).await;
        };
        let zone = outcome_of(&receipt);
        let equal = same(&zone, &s.dry_run);
        let explained = zone.stale(s.window.as_ref()) && !s.dry_run.reverted;
        println!(
            "  {:<28} block {} ({:.0} s): {}; {}",
            s.label,
            receipt.block_id,
            started.elapsed().as_secs_f64(),
            zone.show_for(s.window.as_ref()),
            if equal {
                "equal to its dry run"
            } else if explained {
                "its dry run, at the followed tip's earlier time, was inside the window"
            } else {
                "DIFFERS from its dry run"
            }
        );
        run.log(json!({ "what": what, "step": s.label, "phase": "sent", "tx": s.hash, "block": receipt.block_id,
            "reverted": zone.reverted, "exit_code": zone.exit_code, "reason": zone.reason, "gas_exec": zone.gas_exec, "gas_stor": zone.gas_stor,
            "events": zone.events, "equal_to_dry_run": equal, "base_fee": receipt.fee_base, "resumed": resuming }));
        all_equal &= equal || explained;
        j.steps[i].receipt = Some(Receipt { block: receipt.block_id, outcome: zone.clone() });
        if let Some(sl) = slot {
            sl.save(j)?;
        }
        last = Some((zone.clone(), s.window.clone()));
        if zone.reverted {
            break;
        }
    }
    let status = match &last {
        Some((o, w)) => status_of(o, w.as_ref(), j.expected, all_equal),
        None => 1,
    };
    j.status = Some(status);
    if let Some(sl) = slot {
        sl.save(j)?;
    }
    Ok(status)
}

/// A command whose journal says an earlier run sent it: report what that run did and send nothing (it finished), or
/// finish it with the transactions it signed (it was interrupted). Signs nothing. Returns the exit status.
async fn resume(z: &mut Zone, a: &AttestArgs, run: &Run, slot: &Slot, mut j: Journal) -> Result<i32> {
    let p = payer(&a.key)?;
    if let Some(status) = j.status {
        println!("{}: already sent by an earlier run (journal {}): nothing sent", j.what, slot.path.display());
        for s in &j.steps {
            match (&s.receipt, s.superseded, &s.refused) {
                (Some(r), _, _) => {
                    println!("  {:<28} block {}: {}", s.label, r.block, r.outcome.show_for(s.window.as_ref()))
                }
                (None, true, _) => {
                    println!("  {:<28} not applied: its nonce {} was used by another transaction", s.label, s.nonce)
                }
                (None, false, Some(why)) => println!("  {:<28} refused by the sequencer at submission: {why}", s.label),
                (None, false, None) => println!("  {:<28} not sent", s.label),
            }
        }
        println!("  exit status of that run: {status} (--again sends the command once more)");
        run.log(json!({ "what": j.what, "phase": "already-sent", "journal": slot.path.display().to_string(), "status": status }));
        return Ok(status);
    }
    ensure!(
        j.payer == p.id.to_string(),
        "the interrupted run's transactions are signed by {}, not by --key's account {}: finish it with that key",
        j.payer,
        p.id
    );
    println!(
        "{}: an earlier run was interrupted (journal {}): finishing its {} transaction(s), none signed again",
        j.what,
        slot.path.display(),
        j.steps.len()
    );
    run.log(json!({ "what": j.what, "phase": "resume", "journal": slot.path.display().to_string() }));
    send_journal(z, a, run, &p, &mut j, Some(slot), true).await
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

/// An Ethereum JSON-RPC endpoint (`attest --eth`), read with `curl`: each request bounded by `timeout` seconds, and
/// retried `retries` times after a failure that may pass (no connection, a timeout, an answer cut or empty, HTTP 5xx or
/// 429), one second more between each. An answer that is an error, or not the expected shape, is not retried. Nothing
/// it serves is trusted: the program checks every proof against the record.
pub struct Eth {
    pub url: String,
    pub timeout: u64,
    pub retries: u32,
}

/// One failed request: whether trying again may help, and why it failed.
struct EthFailure {
    transient: bool,
    why: String,
}

impl Eth {
    /// `method(params)`'s whole answer (a JSON-RPC response with a non-null `result`), as bytes.
    pub fn call(&self, method: &str, params: Value) -> Result<Vec<u8>> {
        let mut attempt = 0;
        loop {
            match self.once(method, &params) {
                Ok(body) => return Ok(body),
                Err(f) if f.transient && attempt < self.retries => {
                    attempt += 1;
                    eprintln!("note: {method}: {}; trying again ({attempt} of {})", f.why, self.retries);
                    std::thread::sleep(Duration::from_secs(u64::from(attempt)));
                }
                Err(f) => {
                    let tries =
                        if f.transient { format!(" (after {} attempt(s))", attempt + 1) } else { String::new() };
                    bail!("{method}: {}{tries}", f.why)
                }
            }
        }
    }

    fn once(&self, method: &str, params: &Value) -> std::result::Result<Vec<u8>, EthFailure> {
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string();
        let timeout = self.timeout.max(1).to_string();
        let out = std::process::Command::new("curl")
            .args([
                "-sS",
                "-m",
                &timeout,
                "-A",
                "ferrata-cli",
                "-H",
                "Content-Type: application/json",
                "--data-binary",
                &body,
            ])
            .args(["-w", "\n%{http_code}", &self.url])
            .output()
            .map_err(|e| EthFailure { transient: false, why: format!("running curl: {e}") })?;
        if !out.status.success() {
            let code = out.status.code().unwrap_or(-1);
            let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
            // curl's exit codes: 6 host not resolved, 7 connection failed, 18 answer cut short, 28 timed out, 52 empty
            // answer, 55 and 56 send or receive failed, 92 HTTP/2 stream error.
            let (transient, kind) = match code {
                7 => (true, "the endpoint refused the connection or is unreachable"),
                28 => (true, "the endpoint did not answer in time"),
                18 => (true, "the endpoint's answer was cut short"),
                52 => (true, "the endpoint closed the connection without answering"),
                55 | 56 | 92 => (true, "the connection failed mid-answer"),
                6 => (true, "the endpoint's host name did not resolve"),
                _ => (false, "curl failed"),
            };
            return Err(EthFailure { transient, why: format!("{kind} (curl exit {code}: {why})") });
        }
        let text = out.stdout;
        let split = text.iter().rposition(|b| *b == b'\n').unwrap_or(0);
        let (body, status) =
            (&text[..split], String::from_utf8_lossy(&text[(split + 1).min(text.len())..]).trim().to_string());
        let http: u16 = status.parse().unwrap_or(0);
        if http == 429 || (500..600).contains(&http) {
            return Err(EthFailure { transient: true, why: format!("the endpoint answered HTTP {http}") });
        }
        if !(200..300).contains(&http) {
            return Err(EthFailure { transient: false, why: format!("the endpoint answered HTTP {http}") });
        }
        let v: Value = serde_json::from_slice(body).map_err(|e| EthFailure {
            // A body cut where the server closed the connection without a length reads as broken JSON.
            transient: true,
            why: format!("the answer is not JSON ({e}; {} bytes): cut short?", body.len()),
        })?;
        if let Some(e) = v.get("error") {
            return Err(EthFailure { transient: false, why: format!("the endpoint answered with an error: {e}") });
        }
        if v.get("result").is_none_or(Value::is_null) {
            return Err(EthFailure { transient: false, why: "the endpoint has no answer for it (result null)".into() });
        }
        Ok(body.to_vec())
    }
}

/// What `attest` reads from the Ethereum endpoint: each account's proof, and the block's receipts when a log is asked.
type Fetched = (BTreeMap<[u8; 20], AccountProof>, Option<Vec<Value>>);

/// `f` over data an endpoint served, which may not have the shape `f` expects: a panic is an error, quietly.
fn untrusted<T>(what: &str, f: impl FnOnce() -> T + std::panic::UnwindSafe) -> Result<T> {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let r = std::panic::catch_unwind(f);
    std::panic::set_hook(hook);
    r.map_err(|_| anyhow!("{what}: the answer does not have the expected fields"))
}

/// The endpoint serves the record's chain and block: its `eth_chainId` is the record's, and its block `number` has the
/// hash the record binds. Otherwise its proofs could not verify under the record, whatever else it serves.
fn check_endpoint(eth: &Eth, chain_id: u64, number: u64, bound: &Root) -> Result<()> {
    let v: Value = serde_json::from_slice(&eth.call("eth_chainId", json!([]))?)?;
    let served = v["result"].as_str().and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok());
    ensure!(
        served == Some(chain_id),
        "the endpoint serves chain {} (eth_chainId), the record is of chain {chain_id}: it serves the wrong chain",
        served.map_or_else(|| v["result"].to_string(), |c| c.to_string())
    );
    let v: Value = serde_json::from_slice(&eth.call("eth_getBlockByNumber", json!([format!("0x{number:x}"), false]))?)?;
    let hash = v["result"]["hash"].as_str().unwrap_or_default().to_lowercase();
    ensure!(
        hash == hx(bound),
        "the endpoint's block {number} has hash {hash}, the record binds {}: another chain, or a block it does not hold \
         as the record's",
        hx(bound)
    );
    Ok(())
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
    // A sending command's journal: what it sends is written there before it is sent (RFP-021 R.5).
    let slot = match intent(&a, program, chain_id)? {
        Some((key, text)) => {
            if a.send && a.yes && a.state_dir.is_none() {
                println!(
                    "--send --yes needs --state-dir: the tool writes there what it is about to send, so that the same \
                     command run again after an interruption sends nothing twice"
                );
                return Ok(2);
            }
            a.state_dir.as_ref().map(|d| Slot { path: d.join("sent").join(format!("{key}.json")), intent: text })
        }
        None => None,
    };
    // This machine's clock set for a test or a rehearsal: refused if it is not a time, said when it is taken.
    match clock_override() {
        Some(Err(why)) => {
            println!("{why}: unset it (it is for tests and rehearsals), or set it to a time in milliseconds");
            return Ok(2);
        }
        Some(Ok(ms)) => eprintln!(
            "note: {CLOCK_VAR} sets this machine's clock to {} for judging windows' ends (tests and rehearsals; a time \
             before the zone's clock leaves them to the zone's clock alone)",
            utc(ms)
        ),
        None => {}
    }
    // One run at a time per state directory: its follower state and journals are written as a run goes.
    let _lock = match &a.state_dir {
        Some(d) => match lock_state_dir(d)? {
            Some(f) => Some(f),
            None => {
                println!(
                    "another run of `ferrata attest` holds --state-dir {} (its file `lock`): run this one when that one \
                     has ended",
                    d.display()
                );
                return Ok(2);
            }
        },
        None => None,
    };
    let mut z = Zone::open(&rpc, a.wait_final != "no", a.state_dir.clone()).await?;
    let (tip, _) = z.tip();
    println!("zone {rpc}: followed to block {tip}{}", if z.wait_final { " (final on Bedrock)" } else { "" });
    if let Some(s) = &slot {
        if let Some(j) = s.load()? {
            if a.send && a.yes && !a.again {
                return resume(&mut z, &a, &run, s, j).await;
            }
            if a.again && j.status.is_none() {
                println!(
                    "--again: an earlier run of this command was interrupted (journal {}); run it again without --again \
                     to finish it first",
                    s.path.display()
                );
                return Ok(2);
            }
            println!(
                "note: an earlier run sent this command (journal {}, {}){}",
                s.path.display(),
                j.status.map_or("unfinished".into(), |st| format!("exit status {st}")),
                match (a.again, a.send && a.yes, j.status) {
                    (true, true, _) => "; --again: sending it once more",
                    (true, false, _) => "; with --send --yes --again it would be sent once more",
                    (false, _, Some(_)) => "; with --send --yes nothing more is sent",
                    (false, _, None) => "; with --send --yes it is finished with the transactions that run signed",
                }
            );
        }
    }
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
            // `Init`'s window: until the end of the weak-subjectivity period of the checkpoint's header.
            let w = ferrata_advance::init_window(&config, store.finalized.slot);
            let window = WindowNote {
                start_ms: w.start_ms,
                end_ms: w.end_ms,
                checkpoint: format!("checkpoint {} (its header at slot {})", hx(&checkpoint), store.finalized.slot),
            };
            let mut list = vec![Step {
                label: "Init".into(),
                root: instance_actor(program, &id),
                message: borsh::to_vec(&Message::Init { config: config.clone(), checkpoint, bootstrap: compact.encode(fork) })?,
                actors: vec![instance_actor(program, &id), job_a],
                evidence: vec![PublicAccountEvidence::Pda { program, seed: instance_seed(&id) }],
                window: Some(window),
            }];
            for p in 0..*parts {
                let s = ferrata_advance::adopt_step(program, &id, *parts, job_a, &keys, p, Some(store.finalized.slot));
                list.push(Step {
                    label: format!("Adopt {}/{}", p + 1, parts),
                    root: s.root,
                    message: s.message,
                    actors: s.actors,
                    evidence: vec![],
                    window: None,
                });
            }
            transact(&mut z, &a, &run, "init", list, Some(0), slot.as_ref()).await
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
            // `Open`'s window: from the update's signature slot to the end of the weak-subjectivity period of the
            // instance's newest finalized header (none if it is signed after that end: the program refuses it, 53).
            let open_window =
                ferrata_advance::open_window(&s.config, store.finalized.slot, u.signature_slot).map(|w| WindowNote {
                    start_ms: w.start_ms,
                    end_ms: w.end_ms,
                    checkpoint: format!(
                        "instance's newest finalized header {} (slot {})",
                        hx(&store.finalized.hash_tree_root()),
                        store.finalized.slot
                    ),
                });
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
                        window: open_window.clone(),
                    };
                    let st = transact(&mut z, &a, &run, "advance", vec![open], native, slot.as_ref()).await?;
                    return Ok(if st == 0 { 1 } else { st });
                }
                None => bail!("--committee: at least one full committee the store summarises"),
            };
            println!("advance: useful: job {}, layout {:?}, {}", hx(&planned.jid), planned.layout,
                format!("advanced {}, adopted {}, rotated {}", planned.outcome.advanced, planned.outcome.adopted, planned.outcome.rotated));
            let mut list = Vec::new();
            for step in ferrata_advance::job_steps(&s.config, &planned) {
                let t = ferrata_advance::job_step(program, &id, &s.config, &planned, step)?;
                let window = if step == ferrata_advance::Step::Open { open_window.clone() } else { None };
                list.push(Step {
                    label: step.name(),
                    root: t.root,
                    message: t.message,
                    actors: t.actors,
                    evidence: vec![],
                    window,
                });
            }
            let st = transact(&mut z, &a, &run, "advance", list, Some(0), slot.as_ref()).await?;
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
                        let o = outcome_of(r);
                        let window = window_of(&z, program, h, r.block_id, r.timestamp_ms);
                        println!("receipt {h}: block {}: {}", r.block_id, o.show_for(window.as_ref()));
                        run.log(json!({ "what": "receipt", "tx": h, "block": r.block_id, "reverted": r.reverted, "exit_code": r.exit_code, "reason": r.reason,
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
            let h = record.header().map_err(|e| anyhow!("record: {e:?}"))?;
            // The block the record binds: its hash, and --block must be its number where the record names it.
            let bound = match &h.execution {
                ExecutionSummary::Payload { block_number, block_hash, .. } => {
                    if block_number != block {
                        println!(
                            "--block {block}: the record header({}, {record_slot}) binds block {block_number}",
                            hx(&id)
                        );
                        return Ok(2);
                    }
                    *block_hash
                }
                ExecutionSummary::BlockHash(hash) => *hash,
                ExecutionSummary::None => {
                    bail!("the record header({}, {record_slot}) binds no execution block", hx(&id))
                }
            };
            let bh = format!("0x{block:x}");
            let mut req = Request { instance_id: id, chain_id: chain_id.unwrap_or(record.chain_id), ..Request::default() };
            if let Some(h) = header_rlp {
                req.header = Some(std::fs::read(h)?);
            }
            let mut wanted: BTreeMap<[u8; 20], Vec<[u8; 32]>> = BTreeMap::new();
            for s in storage {
                let parts: Vec<&str> = s.split(':').collect();
                ensure!(parts.len() == 2 || parts.len() == 4, "--storage {s}: ADDRESS:SLOT[:CMP:VALUE]");
                wanted.entry(addr(parts[0])?).or_default().push(word(parts[1])?);
            }
            for s in account {
                wanted.entry(addr(s.split(':').next().unwrap())?).or_default();
            }
            for l in log {
                ensure!(l.split(':').count() >= 2, "--log {l}: TX:LOG[:emitter=ADDRESS]");
            }
            // Every Ethereum read comes first: an endpoint that fails leaves nothing built and nothing sent.
            let eth = Eth { url: eth.clone(), timeout: a.eth_timeout, retries: a.eth_retries };
            let fetched = (|| -> Result<Fetched> {
                check_endpoint(&eth, record.chain_id, *block, &bound)?;
                let mut proofs = BTreeMap::new();
                for (address, slots) in &wanted {
                    let params = json!([hx(address), slots.iter().map(|k| hx(k)).collect::<Vec<_>>(), bh]);
                    let body = eth.call("eth_getProof", params)?;
                    proofs.insert(*address, untrusted("eth_getProof", || account_proof(&body))?);
                }
                let receipts = if log.is_empty() {
                    None
                } else {
                    let v: Value = serde_json::from_slice(&eth.call("eth_getBlockReceipts", json!([bh]))?)?;
                    Some(
                        v["result"]
                            .as_array()
                            .ok_or_else(|| anyhow!("eth_getBlockReceipts: the answer is not a list"))?
                            .clone(),
                    )
                };
                Ok((proofs, receipts))
            })();
            let (proofs, receipts) = match fetched {
                Ok(x) => x,
                Err(e) => {
                    let endpoint = redacted(&eth.url);
                    println!("attest: the Ethereum endpoint {endpoint}: {e:#}");
                    println!(
                        "  nothing was sent, and the follower's state is kept: run the same command again once the endpoint \
                         answers, on the record's chain"
                    );
                    run.log(json!({ "what": "attest", "phase": "endpoint-failed", "endpoint": endpoint, "error": format!("{e:#}") }));
                    return Ok(4);
                }
            };
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
            if let Some(receipts) = receipts {
                let trie = untrusted("eth_getBlockReceipts", || {
                    Trie::new(receipts.iter().enumerate().map(|(i, r)| {
                        let (k, n) = encode_u64(i as u64);
                        (k[..n].to_vec(), receipt_rlp(r))
                    }))
                })?;
                println!("attest: {} receipts of block {block} rebuild the receipts root {}", receipts.len(), hx(&trie.root()));
                for l in log {
                    let parts: Vec<&str> = l.split(':').collect();
                    let (tx, li): (u64, u64) = (parts[0].parse()?, parts[1].parse()?);
                    let mut conditions = Vec::new();
                    if let Some(e) = parts.get(2).and_then(|x| x.strip_prefix("emitter=")) {
                        conditions.push(Condition { field: Field::Emitter, cmp: Cmp::Eq, rhs: address_word(&addr(e)?) });
                    }
                    let (k, n) = encode_u64(tx);
                    req.logs.push(LogQuery { tx_index: tx, log_index: li, proof: trie.proof(&k[..n]), conditions });
                }
            }
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
            // `Attest` carries no validity window.
            let step = Step {
                label: "Attest".into(),
                root: header,
                message,
                actors: vec![header],
                evidence: vec![],
                window: None,
            };
            transact(&mut z, &a, &run, "attest", vec![step], Some(expected), slot.as_ref()).await
        }
    }
}

/// For `receipt`: the window of a transaction the follower holds, when LEZ refused it at `at_ms` past the window's end
/// (`None` otherwise, and LEZ's text is shown as it is). An `Init`'s window is its message's; an `Open`'s end is taken
/// from the instance's newest finalized header as the zone holds it now: a later header only moves that end later, so
/// a time past it was past the end the `Open` carried.
fn window_of(z: &Zone, program: AccountId, hash: &str, block_id: u64, at_ms: u64) -> Option<WindowNote> {
    let want = hash.trim().trim_start_matches("0x").to_lowercase();
    let block = z.follower.blocks.iter().find(|b| b.header.block_id == block_id)?;
    let tx = block
        .body
        .transactions
        .iter()
        .find(|t| t.hash().to_string().trim_start_matches("0x").to_lowercase() == want)?;
    let LeeTransaction::Public(p) = tx else { return None };
    let root = &p.message().execution.root;
    if root.to.program_account_id != program {
        return None;
    }
    let note = match borsh::from_slice::<Message>(&root.message).ok()? {
        Message::Init { config, checkpoint, bootstrap } => {
            let slot = CompactBootstrap::decode(&bootstrap, config.fork()?).ok()?.header.beacon.slot;
            let w = ferrata_advance::init_window(&config, slot);
            WindowNote {
                start_ms: w.start_ms,
                end_ms: w.end_ms,
                checkpoint: format!("checkpoint {} (its header at slot {slot})", hx(&checkpoint)),
            }
        }
        Message::Open { .. } => {
            let state = z.follower.state.get_account_by_id(root.to.account_id).data.actor_state(program).to_vec();
            let s: InstanceState = borsh::from_slice(&state).ok()?;
            let store = CompactStore::decode(&s.store).ok()?;
            WindowNote {
                start_ms: None,
                end_ms: s.config.ws_end_ms(store.finalized.slot),
                checkpoint: format!(
                    "instance's newest finalized header {} (slot {})",
                    hx(&store.finalized.hash_tree_root()),
                    store.finalized.slot
                ),
            }
        }
        _ => return None,
    };
    note.end_ms.is_some_and(|e| at_ms >= e).then_some(note)
}

/// An endpoint's URL without its path, query or credentials, for messages and records (a provider's key often sits in
/// the path).
fn redacted(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(u) => format!(
            "{}://{}{}",
            u.scheme(),
            u.host_str().unwrap_or("?"),
            u.port().map(|p| format!(":{p}")).unwrap_or_default()
        ),
        Err(_) => "(an endpoint whose URL does not parse)".into(),
    }
}

#[cfg(test)]
mod tests {
    //! LEZ's validity-window refusal mapped to code 53's message (RFP-021 U.5), on the refusal a real LEZ v0.4.0-rc1
    //! sequencer gave: the `twin Init` of `e2e/runs/ci-sequencer-attest-2026-11-21` (its `run.tsv` row, its block in
    //! `records/blocks.borsh`). Run with `cargo test --lib attest` (CI's `roundtrip` job).

    use super::*;
    use ferrata_lez_wire::Config;

    const RUN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../e2e/runs/ci-sequencer-attest-2026-11-21/records");

    /// The recorded refusal: the `twin Init` row of `run.tsv` (its status, gas and LEZ's note), and its transaction's
    /// block from `blocks.borsh` (its time, and the `Init` message it carried).
    fn recorded() -> (Outcome, Config, Root, u64) {
        let tsv = std::fs::read_to_string(format!("{RUN}/run.tsv")).unwrap();
        let header: Vec<&str> = tsv.lines().next().unwrap().split('\t').collect();
        let col = |name: &str| header.iter().position(|h| *h == name).unwrap();
        let row: Vec<&str> = tsv.lines().find(|l| l.contains("\ttwin Init Init\t")).unwrap().split('\t').collect();
        assert_eq!(row[col("status")], "reverted");
        assert_eq!(row[col("exit")], "-", "refused by LEZ, not by the program: no exit code");
        assert_eq!(row[col("exec_gas")], row[col("gas_limit")], "LEZ charged the whole gas limit");
        let (hash, block_id): (&str, u64) = (row[col("tx_hash")], row[col("block")].parse().unwrap());
        let blocks: Vec<Block> = borsh::from_slice(&std::fs::read(format!("{RUN}/blocks.borsh")).unwrap()).unwrap();
        let block = blocks.iter().find(|b| b.header.block_id == block_id).expect("the refused transaction's block");
        let tx =
            block.body.transactions.iter().find(|t| t.hash().to_string() == hash).expect("the refused transaction");
        let LeeTransaction::Public(p) = tx else { panic!("a public transaction") };
        let Message::Init { config, checkpoint, bootstrap } =
            borsh::from_slice(&p.message().execution.root.message).unwrap()
        else {
            panic!("an Init")
        };
        let slot = CompactBootstrap::decode(&bootstrap, config.fork().unwrap()).unwrap().header.beacon.slot;
        let outcome = Outcome {
            reverted: true,
            exit_code: None,
            reason: Some(row[col("note")].to_string()),
            gas_exec: row[col("exec_gas")].parse().unwrap(),
            gas_stor: row[col("stor_gas")].parse().unwrap(),
            events: 0,
            event_data: vec![],
            at_ms: block.header.timestamp,
            dry: false,
        };
        (outcome, config, checkpoint, slot)
    }

    /// The window `init` gives the step, as the `init` command computes it.
    fn init_note(config: &Config, checkpoint: &Root, slot: u64) -> WindowNote {
        let w = ferrata_advance::init_window(config, slot);
        WindowNote {
            start_ms: w.start_ms,
            end_ms: w.end_ms,
            checkpoint: format!("checkpoint {} (its header at slot {})", hx(checkpoint), slot),
        }
    }

    #[test]
    fn lez_text_is_the_constant() {
        // The constant is LEZ's own text: a change in LEZ breaks this test, not the mapping silently.
        assert_eq!(lee::error::LeeError::OutOfValidityWindow.to_string(), WINDOW_REFUSAL);
    }

    #[test]
    fn recorded_refusal_of_a_stale_checkpoint_maps_to_code_53() {
        let (outcome, config, checkpoint, slot) = recorded();
        assert_eq!(outcome.reason.as_deref(), Some(WINDOW_REFUSAL), "the recorded note is LEZ's text");
        let note = init_note(&config, &checkpoint, slot);
        let end = note.end_ms.unwrap();
        assert!(
            outcome.at_ms >= end,
            "the recorded block ({}) is past the checkpoint's period ({})",
            utc(outcome.at_ms),
            utc(end)
        );
        let (side, message) = outcome.window(Some(&note)).expect("LEZ's window refusal");
        assert_eq!(side, WindowSide::Ended);
        for part in [
            "refused with 53 OutsideWeakSubjectivityPeriod",
            "recover from a new bootstrap",
            &hx(&checkpoint),
            &format!("slot {slot}"),
            &utc(end),
            &utc(outcome.at_ms),
            "ferrata attest init",
            WINDOW_REFUSAL,
        ] {
            assert!(message.contains(part), "{part:?} missing from: {message}");
        }
        assert!(outcome.stale(Some(&note)));
        assert!(outcome.show_for(Some(&note)).starts_with("refused with 53 OutsideWeakSubjectivityPeriod"));
        // A refusal (3), not a disagreement, though the program's checks pass natively (expected 0).
        assert_eq!(status_of(&outcome, Some(&note), Some(0), true), 3);
        // Without the step's window the text stays LEZ's, with no code put on it.
        assert!(!outcome.show_for(None).contains("OutsideWeakSubjectivityPeriod"), "{}", outcome.show_for(None));
    }

    #[test]
    fn a_window_not_open_yet_is_not_code_53() {
        let (mut outcome, config, checkpoint, slot) = recorded();
        let mut note = init_note(&config, &checkpoint, slot);
        // An update's window opens at its signature slot: the same refusal before that start.
        note.start_ms = Some(outcome.at_ms + 12_000);
        note.end_ms = Some(outcome.at_ms + 1_000_000);
        let (side, message) = outcome.window(Some(&note)).unwrap();
        assert_eq!(side, WindowSide::NotOpen);
        assert!(!message.contains("OutsideWeakSubjectivityPeriod") && !message.contains("bootstrap"), "{message}");
        assert!(message.contains(WINDOW_REFUSAL) && message.contains(&utc(outcome.at_ms + 12_000)), "{message}");
        assert_eq!(status_of(&outcome, Some(&note), Some(0), true), 3);
        // Inside the window as computed: LEZ's text as it is, and a disagreement with the program's verdict.
        note.start_ms = Some(outcome.at_ms - 1);
        let (side, message) = outcome.window(Some(&note)).unwrap();
        assert_eq!(side, WindowSide::Inside);
        assert!(!message.contains("OutsideWeakSubjectivityPeriod") && message.contains(WINDOW_REFUSAL), "{message}");
        assert_eq!(status_of(&outcome, Some(&note), Some(0), true), 1);
        // Other refusals are not touched: the gas limit, and a program's own code (here 53 itself, by the program).
        outcome.reason = Some("Execution exceeded its cycle budget".into());
        assert!(outcome.window(Some(&note)).is_none());
        assert_eq!(
            outcome.show_for(Some(&note)).split(';').next().unwrap(),
            "refused by LEZ: \"Execution exceeded its cycle budget\""
        );
        outcome.exit_code = Some(53);
        outcome.reason = Some("Program exited with non-zero code 53 after 100 cycles".into());
        assert!(outcome.window(Some(&note)).is_none());
        assert!(outcome.show_for(Some(&note)).starts_with("refused with 53 OutsideWeakSubjectivityPeriod"));
    }

    #[test]
    fn a_step_is_not_sent_past_or_near_its_window_end() {
        let (outcome, config, checkpoint, slot) = recorded();
        let note = init_note(&config, &checkpoint, slot);
        let end = note.end_ms.unwrap();
        // Past the end: code 53's message (the run's clock is its newest block's time).
        let zone = "the zone's clock (its newest block's time)";
        let (side, message) = send_check(Some(&note), end + 60_000, zone).unwrap_err();
        assert_eq!(side, WindowSide::Ended);
        for part in [
            "refused with 53 OutsideWeakSubjectivityPeriod",
            &hx(&checkpoint),
            &utc(end),
            "recover from a new bootstrap",
        ] {
            assert!(message.contains(part), "{part:?} missing from: {message}");
        }
        // Within the margin: not sent, no code put on it.
        let (side, message) = send_check(Some(&note), end - WINDOW_MARGIN_MS / 2, zone).unwrap_err();
        assert_eq!(side, WindowSide::Inside);
        assert!(message.contains("less than 60 s") && !message.contains("OutsideWeakSubjectivityPeriod"), "{message}");
        // Earlier, or a step without a window: sent.
        assert!(send_check(Some(&note), end - WINDOW_MARGIN_MS - 1, zone).is_ok());
        assert!(send_check(None, end + 60_000, zone).is_ok());
        // The recorded refusal's block was past the end: a resumed run would not have sent it.
        assert!(send_check(Some(&note), outcome.at_ms, zone).is_err());
    }

    #[derive(clap::Parser)]
    struct Cli {
        #[command(flatten)]
        a: AttestArgs,
    }

    fn key(args: &[&str]) -> String {
        use clap::Parser as _;
        let program = "0x".to_string() + &"11".repeat(32);
        let mut v = vec!["ferrata-attest", "--program", program.as_str()];
        v.extend_from_slice(args);
        let c = Cli::try_parse_from(v).unwrap();
        intent(&c.a, AccountId::new([0x11; 32]), None).unwrap().unwrap().0
    }

    #[test]
    fn the_journal_key_is_the_command_as_parsed() {
        let id = "ab".repeat(32);
        let base =
            ["attest", "--instance", &id, "--record-slot", "15624480", "--eth", "http://a", "--block", "26384851"];
        let with = |extra: &[&str]| key(&[&base[..], extra].concat());
        let weth = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
        let slot = "0xb374801ace2c02f5db0425ab5920a2b7ed1d5a00abbcd395fda7530ba1d666c0";
        let one = with(&["--storage", &format!("{weth}:{slot}:eq:0x01")]);
        // Without 0x, in capitals, a value padded, another endpoint, the instance with 0x: the same command.
        assert_eq!(one, with(&["--storage", &format!("{}:{}:eq:0x0001", &weth[2..], &slot[2..])]));
        assert_eq!(one, with(&["--storage", &format!("{}:{slot}:eq:1", weth.to_uppercase().replace("0X", "0x"))]));
        let mut other = base;
        other[6] = "http://b";
        assert_eq!(one, key(&[&other[..], &["--storage", &format!("{weth}:{slot}:eq:0x01")]].concat()));
        let id0x = format!("0x{id}");
        let mut with0x = base;
        with0x[2] = &id0x;
        assert_eq!(one, key(&[&with0x[..], &["--storage", &format!("{weth}:{slot}:eq:0x01")]].concat()));
        // Two statements of one kind in either order (the request keeps the command line's order; only the key's sort
        // makes them one command): the same.
        let empty = "0x00000000000000000000000000000000000000000000000000000000000000ff";
        assert_eq!(
            with(&["--storage", &format!("{weth}:{slot}"), "--storage", &format!("{weth}:{empty}")]),
            with(&["--storage", &format!("{weth}:{empty}"), "--storage", &format!("{weth}:{slot}")])
        );
        // Statements of two kinds in another order: the same.
        let log = "0:0:emitter=0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
        assert_eq!(
            with(&["--storage", &format!("{weth}:{slot}"), "--log", log]),
            with(&[
                "--log",
                "0:0:emitter=A0B86991C6218B36C1D19D4A2E9EB0CE3606EB48",
                "--storage",
                &format!("{weth}:{slot}")
            ])
        );
        // Another value, comparison, slot, record or chain: another command.
        assert_ne!(one, with(&["--storage", &format!("{weth}:{slot}:eq:0x02")]));
        assert_ne!(one, with(&["--storage", &format!("{weth}:{slot}:ge:0x01")]));
        assert_ne!(one, with(&["--storage", &format!("{weth}:{slot}")]));
        let mut later = base;
        later[4] = "15624481";
        assert_ne!(one, key(&[&later[..], &["--storage", &format!("{weth}:{slot}:eq:0x01")]].concat()));
    }

    #[test]
    fn utc_dates() {
        assert_eq!(utc(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(utc(1_795_278_711_972), "2026-11-21 16:31:51 UTC");
        assert_eq!(utc(951_782_400_000), "2000-02-29 00:00:00 UTC");
        assert_eq!(utc(4_102_444_800_000), "2100-01-01 00:00:00 UTC");
    }

    #[test]
    fn redacted_drops_path_and_query() {
        assert_eq!(redacted("https://user:pw@eth.example:8545/v3/KEY?x=1"), "https://eth.example:8545");
        assert_eq!(redacted("http://127.0.0.1:8645"), "http://127.0.0.1:8645");
    }
}
