//! `ferrata attest` against failing Ethereum endpoints and interrupted mid-command (RFP-021 Reliability #5: "Temporary
//! RPC or connectivity failure on the Ethereum side leaves any off-chain component in a recoverable state, able to
//! resume without duplicating work already done"), and LEZ's refusal of a checkpoint stale by the zone's clock shown
//! as code 53's message (RFP-021 Usability #5).
//!
//! The zone is a stand-in for a LEZ v0.4.0-rc1 sequencer: LEZ's own state machine and block application
//! (`chain_state::apply`: `settle_transaction` and `derive_block_summary` to build a block as LEZ's block builder does,
//! dropping a transaction it rejects; `apply_block` to apply it), the attestation program deployed at genesis as LEZ's
//! tests deploy programs (`with_genesis_programs`, the program of `lez/harness`), served on 127.0.0.1 over the four
//! JSON-RPC methods the tool calls (`getLastBlockId`, `getBlock`, `getAccount`, `sendTransaction`), with a clock, block
//! production and Bedrock finality the test sets. The tool's follower starts from the zone's genesis state (written
//! into its `--state-dir`, as a run that followed the zone earlier leaves it). The Ethereum endpoint is a scripted
//! JSON-RPC server on 127.0.0.1 answering from the recorded mainnet responses of `mainnet-attest-2026-11-10`
//! (`eth_chainId`, `eth_getBlockByNumber`, `eth_getProof`, `eth_getBlockReceipts`), failing on demand. The tool is the
//! `ferrata` binary, run as a process and SIGKILLed where a case says. Nothing leaves 127.0.0.1.
//!
//! R.5, on one zone, every command of `ferrata attest` that reads Ethereum (`attest`; `init` and `advance` read files,
//! `config`, `status`, `state` and `receipt` only the zone):
//! * the endpoint refuses connections, never answers, cuts its answer (the proof; the receipts), answers HTTP 503,
//!   answers with an error, serves another chain (its chain id; a block hash other than the record's): exit 4, nothing
//!   sent, and the same command run once the endpoint answers is applied once; an answer cut once is retried;
//! * killed while the endpoint holds its request: nothing sent; run again, applied once;
//! * killed with its transaction sent but in no block: run again, the same signed transaction is sent again (one hash),
//!   applied once; killed with it in a block not yet final: run again, it waits and sends nothing;
//! * `advance` killed after its `Open` (a job of three transactions): run again, it finishes with `Miller` and
//!   `Finish`, no second `Open`; a finished command run again sends nothing.
//!
//! Checked: every transaction the zone included was submitted once (or resubmitted byte for byte), none applied twice;
//! no `Attest` request and no job step twice.
//!
//! R.5, the journal's guards (another zone): the same command spelled otherwise (no `0x`, capitals, a value unpadded)
//! is "already sent"; a submission the sequencer turns away ends the run, recorded (reported when run again; `--again`
//! then applies it once); `--send --yes` without `--state-dir` and `--again` on an interrupted run are refused with
//! nothing sent; a transaction whose nonce another took is reported not applied (exit 1); a second run on the same
//! state directory is refused while the first holds it.
//!
//! U.5, six more zones, with LEZ's own text produced by LEZ's state machine on the zone's blocks:
//! * `init` with the zone's clock past the checkpoint's weak-subjectivity period: the dry run's refusal shown as code 53's
//!   message naming the checkpoint and the remedy, and nothing sent;
//! * the same with the dry run inside the window and the zone's block past it (the receipt's refusal): code 53's message,
//!   and from `receipt` too (how a caller of the Logos module reads an outcome);
//! * `advance` sent before the end of the period of the instance's newest finalized header, its block after it: code
//!   53's message naming that header, from the run and from `receipt`;
//! * `advance` before its update's signature slot: LEZ's text with the window's start, not code 53;
//! * the followed final tip behind the zone's newest block, that block past the end: the dry run, made at the zone's
//!   clock, holds `init`;
//! * the first review's case: `init` killed with its transaction in the pool, the pool lost, the zone's clock past the
//!   end: run again, nothing is sent (no gas limit charged) and the journal is cleared.
//!
//! Records (each command's output): `$CARGO_TARGET_TMPDIR/attest-faults/`. Run: `cargo test --locked --test
//! attest_faults -- --nocapture` in `cli/` (CI's `roundtrip` job).

use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead as _, BufReader, Read as _, Write as _},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use chain_state::apply::{apply_block, derive_block_summary, opening_fee_state, settle_transaction, Tip};
use common::{
    block::{BedrockStatus, Block, HashableBlockData},
    transaction::{clock_invocation, fee_invocation, FeePayee, LeeTransaction},
    HashType,
};
use ferrata_core::witness::CompactStore;
use ferrata_lez_wire::{header_actor, instance_actor, InstanceState, Message};
use lee::{AccountId, PrivateKey, PublicKey, V03State};
use lee_core::account::Account;
use serde_json::{json, Value};

#[path = "../../lez/live/driver/src/follower.rs"]
#[allow(dead_code)]
mod follower;

const FIXTURE: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/ferrata-spectests/fixtures/mainnet-attest-2026-11-10");
const WETH: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
const USDC: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
const WETH_SLOT: &str = "0xb374801ace2c02f5db0425ab5920a2b7ed1d5a00abbcd395fda7530ba1d666c0";
const BLOCK: u64 = 26_384_851;
const RECORD_SLOT: u64 = 15_624_480;
const BOOTSTRAP_SLOT: u64 = 15_622_144;
const SIGNATURE_SLOT: u64 = 15_624_545;
const MAINNET_GENESIS_TIME: u64 = 1_606_824_023;

fn slot_ms(slot: u64) -> u64 {
    (MAINNET_GENESIS_TIME + slot * 12) * 1000
}

fn fixture(name: &str) -> PathBuf {
    Path::new(FIXTURE).join(name)
}

fn payer_key() -> PrivateKey {
    PrivateKey::try_new([0x5a; 32]).unwrap()
}

fn payer() -> AccountId {
    AccountId::from(&PublicKey::new_from_private_key(&payer_key()))
}

// ---- the zone: LEZ's block application behind the sequencer's JSON-RPC methods ------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// A transaction sent goes into a block at once, final at once.
    Auto,
    /// A transaction sent goes into a block at once, which stays not final until the test finalizes it.
    Hold,
    /// A transaction sent waits in the pool: no block.
    Queue,
}

struct Zone {
    state: V03State,
    blocks: Vec<Block>,
    tip: Tip,
    pending: Vec<LeeTransaction>,
    /// The hash of every `sendTransaction`, in order.
    submitted: Vec<String>,
    /// Every user transaction in a block: its hash, block, whether it reverted, and the transaction.
    included: Vec<(String, u64, bool, LeeTransaction)>,
    final_height: u64,
    clock_ms: u64,
    mode: Mode,
    /// When set, `sendTransaction` answers with this error and admits nothing, as LEZ's sequencer turns a transaction
    /// away at submission (too large, its fee, a full pool).
    refuse: Option<String>,
    /// The hash of every transaction turned away so.
    turned_away: Vec<String>,
}

/// The fee program's payee: LEZ's test block producer (claimed in the genesis state, as LEZ's own tests do).
fn producer() -> FeePayee {
    FeePayee::Present(common::test_utils::producer_account_for_testing())
}

impl Zone {
    /// LEZ's pre-genesis state of a local network, the producer's reward account, the payer funded, and the attestation
    /// program deployed at genesis; then its genesis block (id 1), stamped `clock_ms`.
    fn genesis(clock_ms: u64) -> Self {
        let mut state = testnet_initial_state::initial_state(false)
            .with_public_accounts([common::test_utils::producer_seed(), (payer(), Account::funded(1 << 100))])
            .with_genesis_programs([(harness::ferrata_program(), true)]);
        let genesis = Self::block(&state, 1, HashType::default(), clock_ms, Vec::new()).0;
        apply_block(None, &genesis, &mut state).expect("genesis");
        Self {
            state,
            tip: Tip::from(&genesis),
            blocks: vec![genesis],
            pending: Vec::new(),
            submitted: Vec::new(),
            included: Vec::new(),
            final_height: 1,
            clock_ms,
            mode: Mode::Auto,
            refuse: None,
            turned_away: Vec::new(),
        }
    }

    /// A block of `pending` on `state`, as LEZ's block builder makes one: each transaction settled on a scratch copy in
    /// turn, one it rejects dropped (a nonce already used, for one); then the forced fee and clock transactions.
    fn block(
        state: &V03State,
        id: u64,
        prev: HashType,
        ts: u64,
        pending: Vec<LeeTransaction>,
    ) -> (Block, Vec<LeeTransaction>) {
        let opening = opening_fee_state(state);
        let mut scratch = state.clone();
        let mut summary = fee_core::BlockFeeSummary::default();
        let mut kept = Vec::new();
        for tx in pending {
            let (mut s, mut sum) = (scratch.clone(), summary);
            if settle_transaction(&tx, &mut s, &opening, id, ts, kept.len() as u64, &mut sum).is_ok() {
                (scratch, summary) = (s, sum);
                kept.push(tx);
            }
        }
        let (summary, payout, reverted) = derive_block_summary(state, &kept, id, ts).expect("block summary");
        let mut txs = kept.clone();
        txs.push(LeeTransaction::Public(fee_invocation(summary, payout, producer())));
        txs.push(LeeTransaction::Public(clock_invocation(id, ts)));
        (
            HashableBlockData { block_id: id, prev_block_hash: prev, timestamp: ts, transactions: txs, reverted }
                .into_pending_block(),
            kept,
        )
    }

    /// The next block, of every pending transaction, stamped with the clock (at least 1 ms after the tip).
    fn produce(&mut self) {
        let id = self.tip.block_id + 1;
        let ts = self.clock_ms.max(self.blocks.last().unwrap().header.timestamp + 1);
        let (block, kept) = Self::block(&self.state, id, self.tip.hash, ts, std::mem::take(&mut self.pending));
        apply_block(Some(&self.tip), &block, &mut self.state).expect("the zone applies its own block");
        for (i, tx) in kept.into_iter().enumerate() {
            self.included.push((tx.hash().to_string(), id, block.body.reverted.contains(&(i as u64)), tx));
        }
        self.tip = Tip::from(&block);
        self.blocks.push(block);
    }

    /// The zone's clock moves to `ms` and it stamps a block (empty, final), as a sequencer does every block interval.
    fn tick(&mut self, ms: u64) {
        self.clock_ms = ms;
        self.produce();
        self.final_height = self.tip.block_id;
    }

    /// One JSON-RPC call: its result, or a JSON-RPC error (code, message).
    fn rpc(&mut self, method: &str, params: &Value) -> Result<Value, (i64, String)> {
        let p0 = params.get(0).cloned().unwrap_or(Value::Null);
        let err = |e: serde_json::Error| (-32602, e.to_string());
        match method {
            "getLastBlockId" => Ok(json!(self.tip.block_id)),
            "getBlock" => {
                let id: u64 = serde_json::from_value(p0).map_err(err)?;
                match id.checked_sub(1).and_then(|i| self.blocks.get(i as usize)) {
                    Some(b) => {
                        let mut b = b.clone();
                        b.bedrock_status =
                            if id <= self.final_height { BedrockStatus::Finalized } else { BedrockStatus::Pending };
                        serde_json::to_value(&b).map_err(err)
                    }
                    None => Ok(Value::Null),
                }
            }
            "getAccount" => {
                let id: AccountId = serde_json::from_value(p0).map_err(err)?;
                serde_json::to_value(Some(self.state.get_account_by_id(id).clone())).map_err(err)
            }
            "sendTransaction" => {
                let tx: LeeTransaction = serde_json::from_value(p0).map_err(err)?;
                let hash = tx.hash();
                if let Some(why) = &self.refuse {
                    self.turned_away.push(hash.to_string());
                    return Err((-32602, why.clone()));
                }
                self.submitted.push(hash.to_string());
                self.pending.push(tx);
                match self.mode {
                    Mode::Auto => {
                        self.produce();
                        self.final_height = self.tip.block_id;
                    }
                    Mode::Hold => self.produce(),
                    Mode::Queue => {}
                }
                serde_json::to_value(hash).map_err(err)
            }
            other => Err((-32601, format!("{other}: not served by this stand-in"))),
        }
    }

    /// The program's messages the zone included from the payer: (hash, block, reverted, message).
    fn messages(&self, program: AccountId) -> Vec<(String, u64, bool, Message)> {
        self.included
            .iter()
            .filter_map(|(h, b, r, tx)| {
                let LeeTransaction::Public(p) = tx else { return None };
                let root = &p.message().execution.root;
                (root.to.program_account_id == program)
                    .then(|| borsh::from_slice::<Message>(&root.message).ok())
                    .flatten()
                    .map(|m| (h.clone(), *b, *r, m))
            })
            .collect()
    }
}

/// One HTTP/1.1 request on `r`: its head (lower-cased) and body; `None` at the end of the connection.
fn read_request(r: &mut BufReader<TcpStream>) -> std::io::Result<Option<(String, Vec<u8>)>> {
    let mut head = String::new();
    loop {
        let mut line = String::new();
        if r.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        head.push_str(&line.to_lowercase());
    }
    let len = head
        .lines()
        .find_map(|l| l.strip_prefix("content-length:").and_then(|v| v.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    if head.contains("expect: 100-continue") {
        r.get_mut().write_all(b"HTTP/1.1 100 Continue\r\n\r\n")?;
    }
    let mut body = vec![0; len];
    r.read_exact(&mut body)?;
    Ok(Some((head, body)))
}

fn respond(w: &mut TcpStream, status: &str, body: &[u8], close: bool) -> std::io::Result<()> {
    write!(
        w,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: {}\r\n\r\n",
        body.len(),
        if close { "close" } else { "keep-alive" }
    )?;
    w.write_all(body)?;
    w.flush()
}

/// The zone's JSON-RPC server on 127.0.0.1: its URL.
fn serve_zone(zone: Arc<Mutex<Zone>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let zone = zone.clone();
            std::thread::spawn(move || {
                let mut r = BufReader::new(stream.try_clone().unwrap());
                let mut w = stream;
                while let Ok(Some((head, body))) = read_request(&mut r) {
                    let req: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                    let answer = zone.lock().unwrap().rpc(req["method"].as_str().unwrap_or(""), &req["params"]);
                    let reply = match answer {
                        Ok(result) => json!({ "jsonrpc": "2.0", "id": req["id"], "result": result }),
                        Err((code, e)) => {
                            json!({ "jsonrpc": "2.0", "id": req["id"], "error": { "code": code, "message": e } })
                        }
                    };
                    let close = head.contains("connection: close");
                    if respond(&mut w, "200 OK", reply.to_string().as_bytes(), close).is_err() || close {
                        break;
                    }
                }
            });
        }
    });
    url
}

// ---- the Ethereum endpoint: recorded mainnet answers, failing on demand ---------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fault {
    None,
    /// Takes the request and never answers.
    Hang,
    /// Sends the head and half the body, then closes.
    Cut,
    /// `Cut` for the first matching request, then answers.
    CutOnce,
    Http503,
    /// A JSON-RPC error.
    RpcError,
    /// `eth_chainId` answers Sepolia's.
    WrongChain,
    /// `eth_getBlockByNumber` answers the block with another hash.
    WrongHash,
}

struct Eth {
    fault: Fault,
    /// The method the fault applies to (all if none).
    only: Option<&'static str>,
    /// Every request's method, in order.
    calls: Vec<String>,
    /// Requests held without an answer.
    hung: usize,
}

fn result_of(path: &Path) -> Value {
    let v: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    if v.get("jsonrpc").is_some() {
        v["result"].clone()
    } else {
        v
    }
}

/// The recorded answer to `method(params)`, as `modules/scripts/recorded_rpc.py` gives it.
fn recorded(method: &str, params: &Value) -> Result<Value, String> {
    match method {
        "eth_chainId" => Ok(json!("0x1")),
        "eth_getBlockByNumber" if params[0] == json!(format!("0x{BLOCK:x}")) => {
            Ok(result_of(&fixture(&format!("block_{BLOCK}.json"))))
        }
        "eth_getBlockReceipts" if params[0] == json!(format!("0x{BLOCK:x}")) => {
            Ok(result_of(&fixture(&format!("receipts_{BLOCK}.json"))))
        }
        "eth_getProof" if params[2] == json!(format!("0x{BLOCK:x}")) => {
            let address = params[0].as_str().unwrap_or_default().to_lowercase();
            let keys: Vec<String> =
                params[1].as_array().into_iter().flatten().filter_map(|k| k.as_str()).map(str::to_lowercase).collect();
            for entry in std::fs::read_dir(FIXTURE).unwrap().flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !(name.starts_with("proof_") && name.ends_with(&format!("_{BLOCK}.json"))) {
                    continue;
                }
                let r = result_of(&entry.path());
                if r["address"].as_str().unwrap_or_default().to_lowercase() != address {
                    continue;
                }
                let have: BTreeMap<String, Value> = r["storageProof"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|e| (e["key"].as_str().unwrap_or_default().to_lowercase(), e.clone()))
                    .collect();
                if keys.iter().all(|k| have.contains_key(k)) {
                    let mut r = r.clone();
                    r["storageProof"] = Value::Array(keys.iter().map(|k| have[k].clone()).collect());
                    return Ok(r);
                }
            }
            Err("no recorded proof".into())
        }
        _ => Err(format!("{method}: not recorded")),
    }
}

fn serve_eth(eth: Arc<Mutex<Eth>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let eth = eth.clone();
            std::thread::spawn(move || {
                let mut r = BufReader::new(stream.try_clone().unwrap());
                let mut w = stream;
                let Ok(Some((_, body))) = read_request(&mut r) else { return };
                let req: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                let method = req["method"].as_str().unwrap_or("").to_string();
                let fault = {
                    let mut e = eth.lock().unwrap();
                    e.calls.push(method.clone());
                    let applies = e.only.is_none_or(|m| m == method);
                    let f = if applies { e.fault } else { Fault::None };
                    if f == Fault::CutOnce {
                        e.fault = Fault::None;
                    }
                    if f == Fault::Hang {
                        e.hung += 1;
                    }
                    f
                };
                let answer = |result: Result<Value, String>| match result {
                    Ok(v) => json!({ "jsonrpc": "2.0", "id": req["id"], "result": v }),
                    Err(e) => json!({ "jsonrpc": "2.0", "id": req["id"], "error": { "code": -32000, "message": e } }),
                };
                let body = match fault {
                    Fault::Hang => {
                        std::thread::sleep(Duration::from_secs(90));
                        return;
                    }
                    Fault::Http503 => {
                        let _ = respond(&mut w, "503 Service Unavailable", b"busy", true);
                        return;
                    }
                    Fault::RpcError => answer(Err("missing trie node".into())),
                    Fault::WrongChain if method == "eth_chainId" => answer(Ok(json!("0xaa36a7"))),
                    Fault::WrongHash if method == "eth_getBlockByNumber" => {
                        let mut b = recorded(&method, &req["params"]).unwrap();
                        b["hash"] = json!(format!("0x{}", "ab".repeat(32)));
                        answer(Ok(b))
                    }
                    _ => answer(recorded(&method, &req["params"])),
                };
                let bytes = body.to_string().into_bytes();
                if matches!(fault, Fault::Cut | Fault::CutOnce) {
                    let _ = write!(
                        w,
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                        bytes.len()
                    );
                    let _ = w.write_all(&bytes[..bytes.len() / 2]);
                    let _ = w.flush();
                    return;
                }
                let _ = respond(&mut w, "200 OK", &bytes, true);
            });
        }
    });
    url
}

// ---- the tool --------------------------------------------------------------------------------------------------------

struct Tool {
    dir: PathBuf,
    lez: String,
    program: AccountId,
    key: PathBuf,
    state: PathBuf,
    n: usize,
}

impl Tool {
    /// The tool for `zone`, its follower starting from the zone's genesis state in a fresh `--state-dir`.
    fn new(dir: &Path, name: &str, lez: &str, zone: &Zone) -> Self {
        let dir = dir.join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let key = dir.join("payer.key");
        std::fs::write(&key, hex::encode(payer_key().value())).unwrap();
        let state = dir.join("follower");
        std::fs::create_dir_all(&state).unwrap();
        let f = follower::Follower {
            state: zone.state.clone(),
            tip: Some(Tip { block_id: zone.tip.block_id, hash: zone.tip.hash }),
            blocks: zone.blocks.clone(),
            chain: Vec::new(),
            receipts: BTreeMap::new(),
        };
        let mut bytes = borsh::to_vec(&f.snapshot().unwrap()).unwrap();
        bytes.extend(borsh::to_vec(&f.blocks).unwrap());
        std::fs::write(state.join(ferrata_cli::attest::FOLLOWER_STATE), bytes).unwrap();
        Self { dir, lez: lez.to_string(), program: harness::ferrata_account(), key, state, n: 0 }
    }

    fn command(&mut self, name: &str, args: &[&str]) -> (Command, PathBuf) {
        self.n += 1;
        let out = self.dir.join(format!("{:02}-{name}.txt", self.n));
        let mut c = Command::new(env!("CARGO_BIN_EXE_ferrata"));
        c.arg("attest")
            .args(["--lez-rpc", &self.lez, "--program", &format!("0x{}", hex::encode(self.program.value()))])
            .args([
                "--key",
                self.key.to_str().unwrap(),
                "--state-dir",
                self.state.to_str().unwrap(),
                "--settle-timeout",
                "120",
            ])
            .args(args);
        std::fs::write(&out, format!("$ ferrata attest ... {}\n", args.join(" "))).unwrap();
        (c, out)
    }

    /// Run to the end: the exit status and the output (stdout, then stderr).
    fn run(&mut self, name: &str, args: &[&str]) -> (i32, String) {
        let (mut c, out) = self.command(name, args);
        let o = c.output().unwrap();
        let text = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
        let code = o.status.code().unwrap_or(-1);
        std::fs::write(&out, format!("{}{text}[exit {code}]\n", std::fs::read_to_string(&out).unwrap())).unwrap();
        println!("  {name}: exit {code}");
        (code, text)
    }

    /// Run to the end without `--state-dir`: the exit status and the output.
    fn run_without_state_dir(&mut self, name: &str, args: &[&str]) -> (i32, String) {
        self.n += 1;
        let out = self.dir.join(format!("{:02}-{name}.txt", self.n));
        let o = Command::new(env!("CARGO_BIN_EXE_ferrata"))
            .arg("attest")
            .args(["--lez-rpc", &self.lez, "--program", &format!("0x{}", hex::encode(self.program.value()))])
            .args(["--key", self.key.to_str().unwrap()])
            .args(args)
            .output()
            .unwrap();
        let text = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
        let code = o.status.code().unwrap_or(-1);
        std::fs::write(&out, format!("$ ferrata attest ... (no --state-dir) {}\n{text}[exit {code}]\n", args.join(" "))).unwrap();
        println!("  {name}: exit {code}");
        (code, text)
    }

    /// Start, output to its record file.
    fn spawn(&mut self, name: &str, args: &[&str]) -> (Child, PathBuf) {
        let (mut c, out) = self.command(name, args);
        let f = std::fs::OpenOptions::new().append(true).open(&out).unwrap();
        let child = c.stdout(Stdio::from(f.try_clone().unwrap())).stderr(Stdio::from(f)).spawn().unwrap();
        (child, out)
    }
}

fn wait_for(what: &str, secs: u64, mut done: impl FnMut() -> bool) {
    let t = Instant::now();
    while !done() {
        assert!(t.elapsed() < Duration::from_secs(secs), "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn kill(mut child: Child, out: &Path) {
    child.kill().unwrap(); // SIGKILL
    let status = child.wait().unwrap();
    std::fs::write(out, format!("{}[killed: {status}]\n", std::fs::read_to_string(out).unwrap())).unwrap();
    println!("  killed: {status}");
}

fn has(text: &str, part: &str) {
    assert!(text.contains(part), "{part:?} not in:\n{text}");
}

fn records() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("attest-faults");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn instance_of(init_output: &str) -> String {
    init_output.split_whitespace().skip_while(|w| *w != "instance").nth(1).expect("init names the instance").to_string()
}

const BOOT: &str = "bootstrap.ssz";
const CHECKPOINT: &str = "0x7c279a5e7aee53514ad2ce373139ff36b50e26563ff53134e46cf57b57382433";

fn init_args() -> Vec<String> {
    ["init", "--preset", "mainnet", "--bootstrap", fixture(BOOT).to_str().unwrap(), "--checkpoint", CHECKPOINT]
        .iter()
        .map(|s| s.to_string())
        .chain(["--layout", "electra", "--parts", "2"].iter().map(|s| s.to_string()))
        .collect()
}

fn strs(v: &[String]) -> Vec<&str> {
    v.iter().map(String::as_str).collect()
}

/// A failing endpoint's case: its name, the fault and the method it applies to, the statement's arguments, and a part
/// of the message expected.
type Case<'a> = (&'a str, Fault, Option<&'static str>, Vec<&'a str>, &'a str);

/// R.5: failing endpoints and interruptions, nothing done twice.
#[test]
fn failing_endpoint_and_interruptions_do_nothing_twice() {
    let dir = records();
    // The zone's clock: ten minutes after the finality update's signature slot, inside every window of this data.
    let zone = Arc::new(Mutex::new(Zone::genesis(slot_ms(SIGNATURE_SLOT) + 600_000)));
    let lez = serve_zone(zone.clone());
    let eth = Arc::new(Mutex::new(Eth { fault: Fault::None, only: None, calls: Vec::new(), hung: 0 }));
    let eth_url = serve_eth(eth.clone());
    let mut t = Tool::new(&dir, "r5", &lez, &zone.lock().unwrap());
    let program = t.program;
    let submitted = || zone.lock().unwrap().submitted.len();
    let set_eth = |fault: Fault, only: Option<&'static str>| {
        let mut e = eth.lock().unwrap();
        (e.fault, e.only) = (fault, only);
    };

    // An instance and the record attestations read under.
    let (code, out) = t.run("init", &strs(&[init_args(), vec!["--send".into(), "--yes".into()]].concat()));
    assert_eq!(code, 0, "{out}");
    let id = instance_of(&out);
    let update = fixture("finality_update.ssz");
    let committee = format!("bootstrap:{}", fixture(BOOT).display());
    let advance = [
        "advance",
        "--instance",
        &id,
        "--update",
        update.to_str().unwrap(),
        "--finality",
        "--committee",
        &committee,
        "--send",
        "--yes",
    ];

    // `advance` killed after its `Open`, in a block not yet final; run again: `Miller` and `Finish`, no second `Open`.
    zone.lock().unwrap().mode = Mode::Hold;
    let before = submitted();
    let (child, out_path) = t.spawn("advance-killed-after-open", &advance);
    wait_for("the Open", 300, || submitted() > before);
    kill(child, &out_path);
    {
        let mut z = zone.lock().unwrap();
        z.final_height = z.tip.block_id;
        z.mode = Mode::Auto;
    }
    let (code, out) = t.run("advance-run-again", &advance);
    assert_eq!(code, 0, "{out}");
    has(&out, "an earlier run was interrupted");
    has(&out, "applied or refused in a final block since the earlier run sent it: nothing sent");
    has(&out, "not sent by the earlier run: sending it as that run signed it");
    assert_eq!(submitted() - before, 3, "Open once, then Miller and Finish");
    let (code, out) = t.run("advance-finished-run-again", &advance);
    assert_eq!(code, 0, "{out}");
    has(&out, "already sent by an earlier run");
    assert_eq!(submitted() - before, 3, "nothing more sent");
    {
        let z = zone.lock().unwrap();
        let id_bytes: [u8; 32] = hex::decode(id.trim_start_matches("0x")).unwrap().try_into().unwrap();
        let record = z.state.get_account_by_id(header_actor(program, &id_bytes, RECORD_SLOT).account_id).clone();
        assert!(!record.data.actor_state(program).is_empty(), "the record header(id, {RECORD_SLOT}) is written");
        let inst: InstanceState = borsh::from_slice(
            z.state.get_account_by_id(instance_actor(program, &id_bytes).account_id).data.actor_state(program),
        )
        .unwrap();
        assert_eq!(CompactStore::decode(&inst.store).unwrap().finalized.slot, RECORD_SLOT);
        let opens = z.messages(program).iter().filter(|(_, _, _, m)| matches!(m, Message::Open { .. })).count();
        assert_eq!(opens, 1, "one Open");
    }

    // `attest` under a failing endpoint: exit 4, nothing sent; the follower's state stays usable.
    let bal = result_of(&fixture(&format!("call_weth_balance_{BLOCK}.json"))).as_str().unwrap().to_string();
    let storage_eq = format!("{WETH}:{WETH_SLOT}:eq:{bal}");
    let attest = |eth: &str, extra: &[&str]| -> Vec<String> {
        let mut v: Vec<String> = [
            "attest",
            "--instance",
            &id,
            "--record-slot",
            &RECORD_SLOT.to_string(),
            "--eth",
            eth,
            "--block",
            &BLOCK.to_string(),
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        v.extend(extra.iter().map(|s| s.to_string()));
        v.extend(["--send", "--yes", "--eth-timeout", "2", "--eth-retries", "1"].iter().map(|s| s.to_string()));
        v
    };
    let closed = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}", l.local_addr().unwrap())
    };
    let s0 = submitted();
    let log_spec = format!("0:0:emitter={USDC}");
    let (st, lg) = (storage_eq.as_str(), log_spec.as_str());
    let cases: Vec<Case> = vec![
        ("refused", Fault::None, None, vec!["--storage", st], "refused the connection"),
        ("hang", Fault::Hang, Some("eth_getProof"), vec!["--storage", st], "did not answer in time"),
        ("cut-proof", Fault::Cut, Some("eth_getProof"), vec!["--storage", st], "cut short"),
        ("http-503", Fault::Http503, Some("eth_getProof"), vec!["--storage", st], "HTTP 503"),
        ("rpc-error", Fault::RpcError, Some("eth_getProof"), vec!["--storage", st], "answered with an error"),
        ("wrong-chain-id", Fault::WrongChain, None, vec!["--storage", st], "serves chain 11155111"),
        ("wrong-block-hash", Fault::WrongHash, None, vec!["--storage", st], "the record binds"),
        ("cut-receipts", Fault::Cut, Some("eth_getBlockReceipts"), vec!["--log", lg], "cut short"),
    ];
    for (name, fault, only, extra, expect) in &cases {
        set_eth(*fault, *only);
        eth.lock().unwrap().calls.clear();
        let url = if *name == "refused" { closed.as_str() } else { eth_url.as_str() };
        let (code, out) = t.run(&format!("attest-{name}"), &strs(&attest(url, extra)));
        assert_eq!(code, 4, "{name}: {out}");
        has(&out, expect);
        has(&out, "nothing was sent");
        assert_eq!(submitted(), s0, "{name}: nothing sent");
        let calls = eth.lock().unwrap().calls.clone();
        match *name {
            // Retried once (--eth-retries 1) where trying again may help; not after an error answer or another chain.
            "hang" | "cut-proof" | "http-503" => {
                assert_eq!(calls.iter().filter(|m| *m == "eth_getProof").count(), 2, "{name}: {calls:?}")
            }
            "rpc-error" => assert_eq!(calls.iter().filter(|m| *m == "eth_getProof").count(), 1, "{calls:?}"),
            "wrong-chain-id" => assert_eq!(calls, ["eth_chainId"]),
            "wrong-block-hash" => assert_eq!(calls, ["eth_chainId", "eth_getBlockByNumber"]),
            _ => {}
        }
    }
    // An answer cut once is retried: applied, once.
    set_eth(Fault::CutOnce, Some("eth_getProof"));
    let (code, out) = t.run("attest-cut-once", &strs(&attest(&eth_url, &["--storage", st])));
    assert_eq!(code, 0, "{out}");
    has(&out, "trying again (1 of 1)");
    assert_eq!(submitted(), s0 + 1);
    // The commands that failed, run again with the endpoint answering: the receipts one applied once; the storage one
    // (the same as the cut-once run) already sent, so nothing.
    set_eth(Fault::None, None);
    let (code, out) = t.run("attest-receipts-again", &strs(&attest(&eth_url, &["--log", lg])));
    assert_eq!(code, 0, "{out}");
    assert_eq!(submitted(), s0 + 2);
    let (code, out) = t.run("attest-storage-again", &strs(&attest(&eth_url, &["--storage", st])));
    assert_eq!(code, 0, "{out}");
    has(&out, "already sent by an earlier run");
    assert_eq!(submitted(), s0 + 2, "nothing sent twice");

    // Killed while the endpoint holds its request: nothing sent; run again, applied once.
    set_eth(Fault::Hang, Some("eth_getProof"));
    let account = format!("{WETH}:codehash");
    let mut slow = attest(&eth_url, &["--account", &account]);
    let at = slow.iter().position(|a| a == "--eth-timeout").unwrap();
    slow[at + 1] = "60".into();
    let hung_before = eth.lock().unwrap().hung;
    let (child, out_path) = t.spawn("attest-killed-reading-eth", &strs(&slow));
    wait_for("the held request", 120, || eth.lock().unwrap().hung > hung_before);
    kill(child, &out_path);
    assert_eq!(submitted(), s0 + 2, "killed while reading Ethereum: nothing sent");
    set_eth(Fault::None, None);
    let (code, out) = t.run("attest-account-run-again", &strs(&slow));
    assert_eq!(code, 0, "{out}");
    assert_eq!(submitted(), s0 + 3);

    // Killed with its transaction sent and in no block: run again, the same signed transaction sent again, applied once.
    zone.lock().unwrap().mode = Mode::Queue;
    let ge_spec = format!("{WETH}:{WETH_SLOT}:ge:0x01");
    let ge = attest(&eth_url, &["--storage", &ge_spec]);
    let (child, out_path) = t.spawn("attest-killed-in-pool", &strs(&ge));
    wait_for("the Attest", 300, || submitted() > s0 + 3);
    kill(child, &out_path);
    let queued = zone.lock().unwrap().submitted.last().unwrap().clone();
    zone.lock().unwrap().mode = Mode::Auto;
    let (code, out) = t.run("attest-in-pool-run-again", &strs(&ge));
    assert_eq!(code, 0, "{out}");
    has(&out, "in no block yet: sending the same signed transaction again");
    {
        let z = zone.lock().unwrap();
        assert_eq!(z.submitted.len(), s0 + 5);
        assert_eq!(z.submitted[s0 + 4], queued, "the same transaction, byte for byte");
        assert_eq!(z.included.iter().filter(|(h, ..)| *h == queued).count(), 1, "applied once");
    }

    // Killed with its transaction in a block not yet final: run again, it waits and sends nothing.
    zone.lock().unwrap().mode = Mode::Hold;
    let plain_spec = format!("{WETH}:{WETH_SLOT}");
    let plain = attest(&eth_url, &["--storage", &plain_spec]);
    let (child, out_path) = t.spawn("attest-killed-in-block", &strs(&plain));
    wait_for("the Attest", 300, || submitted() > s0 + 5);
    kill(child, &out_path);
    let (mut rerun, rerun_out) = t.spawn("attest-in-block-run-again", &strs(&plain));
    wait_for("the run waiting for the block", 300, || {
        std::fs::read_to_string(&rerun_out).unwrap_or_default().contains("not yet final: waiting for it, nothing sent")
    });
    {
        let mut z = zone.lock().unwrap();
        z.final_height = z.tip.block_id;
        z.mode = Mode::Auto;
    }
    let status = rerun.wait().unwrap();
    let out = std::fs::read_to_string(&rerun_out).unwrap();
    std::fs::write(&rerun_out, format!("{out}[exit {}]\n", status.code().unwrap_or(-1))).unwrap();
    assert_eq!(status.code(), Some(0), "{out}");
    assert_eq!(submitted(), s0 + 6, "nothing sent by the second run");
    let (code, out) = t.run("attest-in-block-finished-run-again", &strs(&plain));
    assert_eq!(code, 0, "{out}");
    has(&out, "already sent by an earlier run");
    assert_eq!(submitted(), s0 + 6);

    // Nothing twice: every included transaction submitted once, except the one resent byte for byte; every Attest
    // request and every job step once; none reverted.
    let z = zone.lock().unwrap();
    let mut seen = BTreeSet::new();
    for (h, ..) in &z.included {
        assert!(seen.insert(h.clone()), "{h} included twice");
    }
    let distinct: BTreeSet<&String> = z.submitted.iter().collect();
    assert_eq!(distinct.len(), z.included.len(), "every distinct submission included once");
    assert_eq!(z.submitted.len(), z.included.len() + 1, "one resubmission: the transaction left in the pool");
    let mut bodies = BTreeSet::new();
    for (h, _, reverted, m) in z.messages(program) {
        assert!(!reverted, "{h} reverted");
        assert!(bodies.insert(borsh::to_vec(&m).unwrap()), "a message applied twice: {h}");
    }
    let attests = z.messages(program).iter().filter(|(_, _, _, m)| matches!(m, Message::Attest { .. })).count();
    assert_eq!(attests, 5, "five attestations, each once");
    assert!(t.state.join(ferrata_cli::attest::FOLLOWER_STATE).exists());
    println!(
        "R.5: {} submissions, {} transactions included, none twice; records in {}",
        z.submitted.len(),
        z.included.len(),
        dir.display()
    );
}

/// R.5, the journal's guards: the same command spelled otherwise is recognised (the first review's L10 case); a
/// submission the sequencer turns away ends the run, recorded (no journal to delete); `--send --yes` needs
/// `--state-dir`; `--again` does not replace an interrupted run; a transaction whose nonce another took is reported not
/// applied; one run at a time per state directory.
#[test]
fn the_journal_guards() {
    let dir = records();
    let zone = Arc::new(Mutex::new(Zone::genesis(slot_ms(SIGNATURE_SLOT) + 600_000)));
    let lez = serve_zone(zone.clone());
    let eth = Arc::new(Mutex::new(Eth { fault: Fault::None, only: None, calls: Vec::new(), hung: 0 }));
    let eth_url = serve_eth(eth.clone());
    let mut t = Tool::new(&dir, "r5-guards", &lez, &zone.lock().unwrap());
    let program = t.program;
    let submitted = || zone.lock().unwrap().submitted.len();
    let (code, out) = t.run("init", &strs(&[init_args(), vec!["--send".into(), "--yes".into()]].concat()));
    assert_eq!(code, 0, "{out}");
    let id = instance_of(&out);
    let update = fixture("finality_update.ssz");
    let committee = format!("bootstrap:{}", fixture(BOOT).display());
    let (code, out) = t.run(
        "advance",
        &["advance", "--instance", &id, "--update", update.to_str().unwrap(), "--finality", "--committee", &committee, "--send", "--yes"],
    );
    assert_eq!(code, 0, "{out}");
    let (rs, bs) = (RECORD_SLOT.to_string(), BLOCK.to_string());
    let attest = |extra: &[&str]| -> Vec<String> {
        let mut v: Vec<String> = ["attest", "--instance", &id, "--record-slot", &rs, "--eth", &eth_url, "--block", &bs]
            .iter()
            .map(|s| s.to_string())
            .collect();
        v.extend(extra.iter().map(|s| s.to_string()));
        v.extend(["--send", "--yes"].iter().map(|s| s.to_string()));
        v
    };
    let bal = result_of(&fixture(&format!("call_weth_balance_{BLOCK}.json"))).as_str().unwrap().to_string();
    let s0 = submitted();

    // The same command spelled otherwise: no 0x, capitals, the value unpadded. Applied once, then "already sent".
    let st = format!("{WETH}:{WETH_SLOT}:eq:{bal}");
    let unpadded = bal.trim_start_matches("0x").trim_start_matches('0');
    let st_other = format!("{}:{}:eq:{unpadded}", WETH.trim_start_matches("0x").to_uppercase(), WETH_SLOT.trim_start_matches("0x"));
    let (code, out) = t.run("attest-spelled-with-0x", &strs(&attest(&["--storage", &st])));
    assert_eq!(code, 0, "{out}");
    assert_eq!(submitted(), s0 + 1);
    let (code, out) = t.run("attest-spelled-otherwise", &strs(&attest(&["--storage", &st_other])));
    assert_eq!(code, 0, "{out}");
    has(&out, "already sent by an earlier run");
    assert_eq!(submitted(), s0 + 1, "the same command: nothing sent");

    // `--send --yes` without `--state-dir`: a usage error, nothing sent.
    let acct = format!("{WETH}:codehash");
    let (code, out) = t.run_without_state_dir("attest-without-state-dir", &strs(&attest(&["--account", &acct])));
    assert_eq!(code, 2, "{out}");
    has(&out, "--send --yes needs --state-dir");
    assert_eq!(submitted(), s0 + 1);

    // A submission the sequencer turns away (as LEZ's "Transaction too large"): the run ends, recorded; run again,
    // reported, nothing sent; with --again, planned anew and applied once.
    zone.lock().unwrap().refuse = Some("Transaction too large: size 4357, max 4096".into());
    let (code, out) = t.run("attest-turned-away", &strs(&attest(&["--account", &acct])));
    assert_eq!(code, 1, "{out}");
    has(&out, "REFUSED by the sequencer at submission: Transaction too large");
    assert_eq!((submitted(), zone.lock().unwrap().turned_away.len()), (s0 + 1, 1));
    zone.lock().unwrap().refuse = None;
    let (code, out) = t.run("attest-turned-away-run-again", &strs(&attest(&["--account", &acct])));
    assert_eq!(code, 1, "{out}");
    has(&out, "already sent by an earlier run");
    has(&out, "refused by the sequencer at submission: Transaction too large");
    assert_eq!(submitted(), s0 + 1);
    let (code, out) = t.run("attest-turned-away-again", &strs(&[attest(&["--account", &acct]), vec!["--again".into()]].concat()));
    assert_eq!(code, 0, "{out}");
    assert_eq!(submitted(), s0 + 2);

    // --again does not replace an interrupted run: refused, nothing sent; run without it, the run is finished.
    zone.lock().unwrap().mode = Mode::Queue;
    let ge_spec = format!("{WETH}:{WETH_SLOT}:ge:0x01");
    let ge = attest(&["--storage", &ge_spec]);
    let (child, out_path) = t.spawn("attest-killed-in-pool", &strs(&ge));
    wait_for("the Attest", 300, || submitted() > s0 + 2);
    kill(child, &out_path);
    let (code, out) = t.run("attest-interrupted-again", &strs(&[ge.clone(), vec!["--again".into()]].concat()));
    assert_eq!(code, 2, "{out}");
    has(&out, "was interrupted");
    assert_eq!(submitted(), s0 + 3, "--again on an interrupted run: nothing sent");
    zone.lock().unwrap().mode = Mode::Auto;
    let (code, out) = t.run("attest-interrupted-finished", &strs(&ge));
    assert_eq!(code, 0, "{out}");
    has(&out, "in no block yet: sending the same signed transaction again");
    assert_eq!(submitted(), s0 + 4);

    // A transaction whose nonce another took (the pool lost, then another command sent at that nonce): not applied,
    // exit 1, recorded; run again, reported; with --again, applied once.
    zone.lock().unwrap().mode = Mode::Queue;
    let plain_spec = format!("{WETH}:{WETH_SLOT}");
    let plain = attest(&["--storage", &plain_spec]);
    let (child, out_path) = t.spawn("attest-killed-then-pool-lost", &strs(&plain));
    wait_for("the Attest", 300, || submitted() > s0 + 4);
    kill(child, &out_path);
    {
        let mut z = zone.lock().unwrap();
        z.pending.clear(); // the sequencer lost its pool (a restart)
        z.mode = Mode::Auto;
    }
    let log_spec = format!("0:0:emitter={USDC}");
    let (code, out) = t.run("attest-takes-the-nonce", &strs(&attest(&["--log", &log_spec])));
    assert_eq!(code, 0, "{out}");
    assert_eq!(submitted(), s0 + 6);
    let (code, out) = t.run("attest-nonce-taken", &strs(&plain));
    assert_eq!(code, 1, "{out}");
    has(&out, "NOT APPLIED: the payer's nonce");
    assert_eq!(submitted(), s0 + 6, "nothing sent");
    let (code, out) = t.run("attest-nonce-taken-run-again", &strs(&plain));
    assert_eq!(code, 1, "{out}");
    has(&out, "not applied: its nonce");
    let (code, out) = t.run("attest-nonce-taken-again", &strs(&[plain.clone(), vec!["--again".into()]].concat()));
    assert_eq!(code, 0, "{out}");
    assert_eq!(submitted(), s0 + 7);

    // One run at a time per state directory: a status beside a run awaiting its transaction is refused.
    zone.lock().unwrap().mode = Mode::Hold;
    let bare = WETH.to_string();
    let (mut child, out_path) = t.spawn("attest-holding-the-lock", &strs(&attest(&["--account", &bare])));
    wait_for("the Attest", 300, || submitted() > s0 + 7);
    let (code, out) = t.run("status-beside-it", &["status", "--instance", &id]);
    assert_eq!(code, 2, "{out}");
    has(&out, "another run of `ferrata attest` holds --state-dir");
    {
        let mut z = zone.lock().unwrap();
        z.final_height = z.tip.block_id;
        z.mode = Mode::Auto;
    }
    let status = child.wait().unwrap();
    let text = std::fs::read_to_string(&out_path).unwrap();
    std::fs::write(&out_path, format!("{text}[exit {}]\n", status.code().unwrap_or(-1))).unwrap();
    assert_eq!(status.code(), Some(0), "{text}");

    // Nothing twice: every included transaction once, every message once, none reverted.
    let z = zone.lock().unwrap();
    let mut seen = BTreeSet::new();
    for (h, ..) in &z.included {
        assert!(seen.insert(h.clone()), "{h} included twice");
    }
    let mut bodies = BTreeSet::new();
    for (h, _, reverted, m) in z.messages(program) {
        assert!(!reverted, "{h} reverted");
        assert!(bodies.insert(borsh::to_vec(&m).unwrap()), "a message applied twice: {h}");
    }
    println!("R.5 guards: {} submissions, {} turned away, {} included, none twice", z.submitted.len(), z.turned_away.len(), z.included.len());
}

/// U.5: LEZ's refusal of a checkpoint stale by the zone's clock, as code 53's message; a window not open, as LEZ's text.
#[test]
fn stale_checkpoint_shown_as_code_53() {
    let dir = records();
    // The instance's configuration, as `init` makes it (mainnet, two adoption parts, the Electra layout).
    let config = ferrata_lez_wire_config();
    let end = ferrata_advance::init_window(&config, BOOTSTRAP_SLOT).end_ms.expect("a period that ends");
    let init = [init_args(), vec!["--send".into(), "--yes".into()]].concat();

    // The zone's clock past the end: the dry run shows code 53's message, and nothing is sent.
    let zone = Arc::new(Mutex::new(Zone::genesis(end + 60_000)));
    let lez = serve_zone(zone.clone());
    let mut t = Tool::new(&dir, "u5-past-end", &lez, &zone.lock().unwrap());
    let (code, out) = t.run("init-past-end", &strs(&init));
    assert_eq!(code, 3, "{out}");
    for part in [
        "refused with 53 OutsideWeakSubjectivityPeriod",
        CHECKPOINT,
        "recover from a new bootstrap",
        "ferrata attest init",
        "\"Execution outside of the validity window\"",
        "not sent",
    ] {
        has(&out, part);
    }
    assert_eq!(zone.lock().unwrap().submitted.len(), 0, "nothing sent");

    // The dry run an hour before the end, the zone's block after it: the receipt's refusal, as code 53's message.
    let zone = Arc::new(Mutex::new(Zone::genesis(end - 3_600_000)));
    zone.lock().unwrap().clock_ms = end + 60_000;
    let lez = serve_zone(zone.clone());
    let mut t = Tool::new(&dir, "u5-crossing-end", &lez, &zone.lock().unwrap());
    let (code, out) = t.run("init-crossing-end", &strs(&init));
    assert_eq!(code, 3, "{out}");
    has(&out, "would be applied");
    has(&out, "refused with 53 OutsideWeakSubjectivityPeriod");
    has(&out, "its dry run, at the followed tip's earlier time, was inside the window");
    has(&out, CHECKPOINT);
    let init_hash = {
        let z = zone.lock().unwrap();
        assert_eq!(z.submitted.len(), 1, "the Init only: the adoption parts are not sent after a refusal");
        assert!(z.included[0].2, "the zone refused it");
        z.submitted[0].clone()
    };
    // `receipt`, as a caller of the Logos module reads a transaction's outcome: the same message.
    let (code, out) = t.run("receipt-of-the-refused-init", &["receipt", "--tx", &init_hash]);
    assert_eq!(code, 0, "{out}");
    has(&out, "refused with 53 OutsideWeakSubjectivityPeriod");
    has(&out, CHECKPOINT);

    // An update sent before the end of the period of the instance's newest finalized header, its block after it.
    let zone = Arc::new(Mutex::new(Zone::genesis(end - 3_600_000)));
    let lez = serve_zone(zone.clone());
    let mut t = Tool::new(&dir, "u5-update-crossing-end", &lez, &zone.lock().unwrap());
    let (code, out) = t.run("init", &strs(&init));
    assert_eq!(code, 0, "{out}");
    let id = instance_of(&out);
    zone.lock().unwrap().clock_ms = end + 60_000;
    let update = fixture("finality_update.ssz");
    let committee = format!("bootstrap:{}", fixture(BOOT).display());
    let advance = [
        "advance",
        "--instance",
        &id,
        "--update",
        update.to_str().unwrap(),
        "--finality",
        "--committee",
        &committee,
        "--send",
        "--yes",
    ];
    let (code, out) = t.run("advance-crossing-end", &advance);
    assert_eq!(code, 3, "{out}");
    has(&out, "refused with 53 OutsideWeakSubjectivityPeriod");
    has(&out, "instance's newest finalized header 0x");
    has(&out, &format!("(slot {BOOTSTRAP_SLOT}) is stale"));
    let open_hash = {
        let z = zone.lock().unwrap();
        assert_eq!(z.submitted.len(), 4, "the instance's three, then the Open only");
        let last = z.included.last().unwrap();
        assert!(last.2, "the zone refused the Open");
        z.submitted[3].clone()
    };
    let (code, out) = t.run("receipt-of-the-refused-open", &["receipt", "--tx", &open_hash]);
    assert_eq!(code, 0, "{out}");
    has(&out, "refused with 53 OutsideWeakSubjectivityPeriod");
    has(&out, &format!("(slot {BOOTSTRAP_SLOT}) is stale"));

    // An update before its signature slot: LEZ's text with the window's start; not code 53.
    let zone = Arc::new(Mutex::new(Zone::genesis(slot_ms(BOOTSTRAP_SLOT) + 600_000)));
    let lez = serve_zone(zone.clone());
    let mut t = Tool::new(&dir, "u5-not-open", &lez, &zone.lock().unwrap());
    let (code, out) = t.run("init", &strs(&init));
    assert_eq!(code, 0, "{out}");
    let id = instance_of(&out);
    let update = fixture("finality_update.ssz");
    let committee = format!("bootstrap:{}", fixture(BOOT).display());
    let (code, out) = t.run(
        "advance-before-signature-slot",
        &["advance", "--instance", &id, "--update", update.to_str().unwrap(), "--finality", "--committee", &committee],
    );
    assert_eq!(code, 3, "{out}");
    let open = out.lines().find(|l| l.trim_start().starts_with("open ")).expect("the Open's dry run").to_string();
    has(&open, "\"Execution outside of the validity window\": its window opens at");
    has(&open, &ferrata_cli::attest::utc(slot_ms(SIGNATURE_SLOT)));
    assert!(
        !open.contains("OutsideWeakSubjectivityPeriod") && !open.contains("refused with 53"),
        "not code 53: {open}"
    );
    // The followed final tip behind the zone's newest block (finality lagging), that block past the end: the dry run is
    // made at the zone's clock, and nothing is sent.
    let zone = Arc::new(Mutex::new(Zone::genesis(end - 3_600_000)));
    {
        let mut z = zone.lock().unwrap();
        z.clock_ms = end + 60_000;
        z.produce(); // not final
    }
    let lez = serve_zone(zone.clone());
    let mut t = Tool::new(&dir, "u5-final-tip-lagging", &lez, &zone.lock().unwrap());
    let (code, out) = t.run("init-final-tip-lagging", &strs(&init));
    assert_eq!(code, 3, "{out}");
    has(&out, "refused with 53 OutsideWeakSubjectivityPeriod");
    has(&out, "not sent");
    assert_eq!(zone.lock().unwrap().submitted.len(), 0, "nothing sent");

    // The first review's L1 case: `init` killed with its `Init` in the pool, the pool lost, the zone's clock 60 s past
    // the end (and a block stamped then, as a sequencer stamps one every interval): run again, the earlier run's
    // transaction is not sent again (it would be charged the whole gas limit), the journal is cleared; run once more, a
    // fresh run's dry run holds it.
    let zone = Arc::new(Mutex::new(Zone::genesis(end - 3_600_000)));
    zone.lock().unwrap().mode = Mode::Queue;
    let lez = serve_zone(zone.clone());
    let mut t = Tool::new(&dir, "u5-resume-past-end", &lez, &zone.lock().unwrap());
    let (child, out_path) = t.spawn("init-killed-in-pool", &strs(&init));
    wait_for("the Init", 300, || !zone.lock().unwrap().submitted.is_empty());
    kill(child, &out_path);
    {
        let mut z = zone.lock().unwrap();
        z.pending.clear(); // the sequencer lost its pool (a restart)
        z.tick(end + 60_000);
        z.mode = Mode::Auto;
    }
    let (code, out) = t.run("init-run-again-past-end", &strs(&init));
    assert_eq!(code, 3, "{out}");
    has(&out, "not sent again: refused with 53 OutsideWeakSubjectivityPeriod");
    has(&out, CHECKPOINT);
    has(&out, "is cleared");
    {
        let z = zone.lock().unwrap();
        assert_eq!(z.submitted.len(), 1, "the killed run's submission only");
        assert!(z.included.is_empty(), "nothing applied, nothing charged");
    }
    let journals = std::fs::read_dir(t.state.join("sent")).map(|d| d.count()).unwrap_or(0);
    assert_eq!(journals, 0, "the journal is cleared");
    let (code, out) = t.run("init-once-more", &strs(&init));
    assert_eq!(code, 3, "{out}");
    has(&out, "not sent: LEZ refuses it at any later time too");
    assert_eq!(zone.lock().unwrap().submitted.len(), 1, "nothing sent");
    println!("U.5: records in {}", dir.display());
}

/// The configuration `ferrata attest init --preset mainnet --parts 2 --layout electra` gives an instance.
fn ferrata_lez_wire_config() -> ferrata_lez_wire::Config {
    let mut c = plan::mainnet_config(2);
    c.store_fork = 3;
    c
}

#[path = "../../lez/live/driver/src/plan.rs"]
#[allow(dead_code, unused_imports)]
mod plan;
