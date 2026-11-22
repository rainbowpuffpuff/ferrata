//! The relayer daemon killed at each of its Ethereum transactions, killed at random, and run behind failing
//! endpoints (RFP-022 R.06): one effect per job, every job booked, no transaction sent twice.
//!
//! A local anvil (chain id 31337, never a public network) with FerrataVault v5 in mode (B) from
//! `vault/script/MonitorFixture.s.sol` (its accept-all verifier: no proof is checked here, R.06 is not about
//! proving), ETH registered with a relay fee (and, for its second denomination, a checkpoint fee the relayer
//! then withdraws with `claimOwed`), and deposits escrowing what the releases pay out. The jobs are what a
//! checkpoint prover and holders hand a relayer: a rule (B) checkpoint over a burn root, and releases under it
//! (journals encoded as the guests encode them; any seal). The daemon is the `ferrata-relayer` binary, signing
//! with a keystore.
//!
//! Part 1, a kill at each kind of transaction. anvil's automine is off, so a sent transaction waits in the pool
//! until the test mines it; the daemon is SIGKILLed while its transaction is pending, then
//!   P  restarted while the transaction is still pending, and the block mined afterwards;
//!   M  the block mined, then restarted;
//!   D  the transaction dropped from the pool (`anvil_dropTransaction`), then restarted;
//! submitCandidate (P), finalizeAttested (M), a release (D), a release (P), a release with a checkpoint fee
//! (M) and the claimOwed after it (D).
//! Part 2, random kills: automine on, twenty releases in the inbox, the daemon SIGKILLed 0 to 40 ms after its
//! start-up line and started again until every job is booked. Where the kills land is chance: the recorded runs
//! show which windows they hit (`inflight-*` lines); the two windows chance rarely hits are part 4's.
//! Part 3, failing endpoints: `e2e/netfault/faultnet.py` in front of the anvil, the daemon on one of its endpoints
//! at a time (flaky, sends, lagging), four releases each, a block mined every second. Each endpoint's faults are drawn
//! from its seed (rates, not guarantees): the faults that actually fired are counted per part (`faultsSeen`).
//! Part 4, the record's two narrow windows, made to happen (`e2e/netfault/holdproxy.py` in front of the anvil, automine
//! on): a send whose answer is lost (the transaction mined, its hash never known: found under its nonce,
//! `inflight-found`); a send forwarded with its answer held and the daemon killed (the same, after a restart); the daemon
//! killed with its record written and nothing sent (restarted: `inflight-lost`, the same call with the same nonce).
//! Part 5, the `--send-via logos` route killed in each of its windows (`cli/examples/relayer_logos_stub.rs`, the daemon
//! with Logos Core's call replaced by a client of `e2e/netfault/txsender_stub.py`, a stand-in for `tx_sender_module`;
//! no key in the daemon): after its request was made and before any broadcast; after the request was made, before its
//! id was known; after the broadcast, before its hash was known. Restarted, the daemon finds its request and drives it
//! to its end: one request per job, never a second one. The stand-in survives each of these kills.
//! Part 6, what `tx_sender_module` cannot account for (the second review's M2; the stand-in with a durable history, as
//! the module's): (a) the module restarted after it broadcast, the answer never delivered, the transaction pending
//! (automine off): the restarted daemon reads the request's row in the module's history and waits for the transaction,
//! no request made; (b) the module restarted after the request was made, before any broadcast: no row, so nothing left,
//! and the call is planned again (one request); (c) `stuck` (a broadcast whose answer never came) with the transaction
//! out: found mined under the request's nonce; (d) `stuck` with nothing sent: lost, sent again through the module with
//! the nonce pinned (one transaction for two requests).
//!
//! Checked after each part: one `CandidateSubmitted` and one `CheckpointFinalized`, one `Release` per
//! nullifier, every job in `done/` (none refused) with at least one `job-done` line, all naming one
//! transaction; the relayer's nonce equal to the number of its effects; none of its transactions reverted.
//! Results: `FERRATA_CRASH_RESULTS` (JSON).
//!
//! Runs only with `FERRATA_RELAYER_CRASH=1` (`cli/tests/relayer-crash.sh`), Foundry's anvil and forge, and
//! `vault/lib` installed.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use alloy::{
    network::TransactionBuilder as _,
    primitives::{Address, B256, U256},
    providers::{DynProvider, Provider},
    rpc::types::{Filter, TransactionRequest},
    signers::local::PrivateKeySigner,
    sol,
    sol_types::{SolCall, SolEvent},
};
use anyhow::{anyhow, bail, ensure, Context as _, Result};
use ferrata_bridge_core::{encoding::ReleaseJournal, sha256};
use ferrata_cli::{
    eth::{self, IVault},
    relayer::{self, IRelayVault},
};
use serde_json::{json, Value};

sol! {
    #[allow(missing_docs)]
    interface IAdmin {
        struct CapParams { uint256 capacity; uint256 refillPerSecond; }
        struct TokenParams {
            uint256[] denominations; uint256[] relayFees; uint256 lezUnit; uint256 unitWeight; CapParams depositCap;
            CapParams releaseCap; uint256 releaseDelay; bytes32 lezDefinition; uint8 decimals; uint256[] checkpointFees;
        }
        function registerToken(address token, TokenParams p) external;
        event OwedClaimed(address indexed payee, address indexed token, uint256 amount);
    }
}

/// anvil's default accounts 0-3 (unlocked on anvil): the deployer and admin, the freeze authority, the watcher,
/// the fee recipient.
const ANVIL: [Address; 4] = [
    alloy::primitives::address!("f39Fd6e51aad88F6F4ce6aB8827279cffFb92266"),
    alloy::primitives::address!("70997970C51812dc3A010C7d01b50e0d17dc79C8"),
    alloy::primitives::address!("3C44CdDdB6a900fa2b585dd299e03d12FA4293BC"),
    alloy::primitives::address!("90F79bf6EB2c4f870365E785982E1f101E93b906"),
];
const SEAL: &str = "0x5ea15ea1";
const DENOMS: [u128; 2] = [100_000_000_000_000_000, 1_000_000_000_000_000_000];
const RELAY_FEES: [u128; 2] = [100_000_000_000_000, 1_000_000_000_000_000];
const CHECKPOINT_FEES: [u128; 2] = [0, 10_000_000_000_000];
const PASSWORD: &str = "relayer crash test password";

struct Proc(Child);

impl Drop for Proc {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Everything the parts share.
struct World {
    dir: PathBuf,
    rpc: String,
    p: DynProvider,
    vault: Address,
    chain_id: u64,
    cid: B256,
    relayer: Address,
    keystore: PathBuf,
    password: PathBuf,
    state: PathBuf,
    inbox: PathBuf,
    /// Daemon runs so far (each writes `logs/<n>.jsonl`).
    runs: usize,
    /// Every job handed over: file name -> (key of its effect, nullifier for a release).
    jobs: BTreeMap<String, Option<B256>>,
    /// Part 5: the stand-in for tx_sender_module (`HOST:PORT`); the daemon is then the example with --send-via logos.
    logos_stub: Option<String>,
    /// The relayer's effects the vault must show: transactions it must have mined.
    effects: u64,
    /// The checkpoint's new state.
    new_state: B256,
    burn_root: B256,
    releases: u64,
}

/// The `ferrata-relayer` binary cargo built for this test, found beside the test's own binary (as
/// `relayer_fork.rs` does: `CARGO_BIN_EXE_ferrata-relayer` does not reach rustc through this tree's rustc wrapper).
/// `FERRATA_RELAYER_CMD` runs another command with the binary's arguments and behaviour in its place
/// (`modules/scripts/relayer-module.sh`: the daemon as `ferrata_relayer_module` in a Logos Core).
fn relayer_bin() -> PathBuf {
    if let Ok(cmd) = std::env::var("FERRATA_RELAYER_CMD") {
        return PathBuf::from(cmd);
    }
    if let Some(p) = option_env!("CARGO_BIN_EXE_ferrata-relayer") {
        return PathBuf::from(p);
    }
    let exe = std::env::current_exe().expect("the test binary's path");
    exe.parent().and_then(Path::parent).expect("target/<profile>").join("ferrata-relayer")
}

/// The example `relayer_logos_stub` (part 5), built beside the binaries (`cargo build --example relayer_logos_stub`).
fn relayer_logos_stub_bin() -> PathBuf {
    let exe = std::env::current_exe().expect("the test binary's path");
    exe.parent().and_then(Path::parent).expect("target/<profile>").join("examples").join("relayer_logos_stub")
}

/// The test's scratch directory: `FERRATA_CRASH_DIR`, or cargo's directory for integration tests (inside `target/`).
fn scratch() -> PathBuf {
    let dir = std::env::var("FERRATA_CRASH_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("relayer-crash"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("logs")).unwrap();
    dir
}

async fn rpc(p: &DynProvider, method: &'static str, params: Value) -> Result<Value> {
    p.raw_request::<_, Value>(method.into(), params).await.map_err(|e| anyhow!("{method}: {e}"))
}

async fn from_anvil(p: &DynProvider, from: Address, to: Address, input: Vec<u8>, value: U256) -> Result<()> {
    let tx = TransactionRequest::default().with_from(from).with_to(to).with_input(input).with_value(value);
    let receipt = p.send_transaction(tx).await?.get_receipt().await?;
    ensure!(receipt.status(), "a set-up transaction reverted");
    Ok(())
}

impl World {
    /// The daemon, started: `ferrata-relayer ... run`, its lines in `logs/<n>.jsonl`.
    fn start(&mut self, rpc: Option<&str>) -> Result<Proc> {
        self.runs += 1;
        let log = std::fs::File::create(self.dir.join("logs").join(format!("{:03}.jsonl", self.runs)))?;
        let err = std::fs::File::create(self.dir.join("logs").join(format!("{:03}.err", self.runs)))?;
        let mut cmd = match &self.logos_stub {
            None => {
                let mut c = Command::new(relayer_bin());
                c.args(["--eth-rpc", rpc.unwrap_or(&self.rpc), "--vault", &self.vault.to_string()])
                    .arg("--keystore")
                    .arg(&self.keystore)
                    .arg("--password-file")
                    .arg(&self.password);
                c
            }
            Some(stub) => {
                let mut c = Command::new(relayer_logos_stub_bin());
                c.env("FERRATA_LOGOS_STUB", stub)
                    .args(["--eth-rpc", rpc.unwrap_or(&self.rpc), "--vault", &self.vault.to_string()])
                    .args(["--send-via", "logos", "--from", &self.relayer.to_string(), "--approval-timeout", "20"]);
                c
            }
        };
        let child = cmd
            .arg("--state")
            .arg(&self.state)
            .args(["--receipt-timeout", "8", "--startup-wait", "300", "run", "--poll-secs", "1", "--inbox"])
            .arg(&self.inbox)
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(err))
            .spawn()
            .context("starting ferrata-relayer")?;
        Ok(Proc(child))
    }

    /// SIGKILL (Child::kill on Unix), and wait for it.
    fn kill(&self, mut d: Proc) -> Result<()> {
        d.0.kill()?;
        d.0.wait()?;
        Ok(())
    }

    /// Every line every daemon run wrote, in order.
    fn lines(&self) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        for n in 1..=self.runs {
            let text = std::fs::read_to_string(self.dir.join("logs").join(format!("{n:03}.jsonl"))).unwrap_or_default();
            out.extend(text.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()));
        }
        Ok(out)
    }

    fn last_run_has(&self, event: &str) -> bool {
        let text =
            std::fs::read_to_string(self.dir.join("logs").join(format!("{:03}.jsonl", self.runs))).unwrap_or_default();
        text.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).any(|v| v["event"] == event)
    }

    async fn wait_last_run(&self, what: &str, f: impl Fn(&Value) -> bool, secs: u64) -> Result<()> {
        let t0 = Instant::now();
        loop {
            let text = std::fs::read_to_string(self.dir.join("logs").join(format!("{:03}.jsonl", self.runs)))
                .unwrap_or_default();
            if text.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).any(|v| f(&v)) {
                return Ok(());
            }
            ensure!(t0.elapsed() < Duration::from_secs(secs), "no {what} within {secs} s (run {})", self.runs);
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    async fn nonces(&self) -> Result<(u64, u64)> {
        Ok((
            self.p.get_transaction_count(self.relayer).latest().await?,
            self.p.get_transaction_count(self.relayer).pending().await?,
        ))
    }

    /// The relayer's one pending transaction (hash, nonce, selector), waited for.
    async fn pending(&self, secs: u64) -> Result<(B256, u64, [u8; 4])> {
        let t0 = Instant::now();
        loop {
            let pool = rpc(&self.p, "txpool_content", json!([])).await?;
            let mine = pool["pending"]
                .as_object()
                .and_then(|m| m.iter().find(|(a, _)| a.parse::<Address>().ok() == Some(self.relayer)))
                .map(|(_, txs)| txs.clone());
            if let Some(txs) = mine.and_then(|t| t.as_object().cloned()) {
                ensure!(txs.len() == 1, "the relayer has {} transactions in the pool: {txs:?}", txs.len());
                let tx = txs.values().next().expect("one");
                let hash: B256 = tx["hash"].as_str().context("hash")?.parse()?;
                let nonce = u64::from_str_radix(tx["nonce"].as_str().context("nonce")?.trim_start_matches("0x"), 16)?;
                let input = tx["input"].as_str().or(tx["data"].as_str()).context("input")?;
                let sel = alloy::hex::decode(input)?.get(..4).context("selector")?.try_into()?;
                return Ok((hash, nonce, sel));
            }
            ensure!(t0.elapsed() < Duration::from_secs(secs), "no pending relayer transaction within {secs} s");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    async fn mine(&self) -> Result<()> {
        rpc(&self.p, "evm_mine", json!([])).await.map(|_| ())
    }

    async fn automine(&self, on: bool) -> Result<()> {
        rpc(&self.p, "evm_setAutomine", json!([on])).await.map(|_| ())
    }

    /// anvil's call trace of a transaction (for a diagnosis when one reverts).
    async fn trace(&self, tx: &str) -> String {
        match rpc(&self.p, "debug_traceTransaction", json!([tx, { "tracer": "callTracer" }])).await {
            Ok(t) => json!({ "error": t["error"], "revertReason": t["revertReason"], "output": t["output"],
                             "gas": t["gas"], "gasUsed": t["gasUsed"] })
            .to_string(),
            Err(e) => format!("no trace: {e}"),
        }
    }

    async fn wait_done(&self, job: &str, secs: u64) -> Result<()> {
        let t0 = Instant::now();
        while !self.state.join("done").join(job).exists() {
            if self.state.join("refused").join(job).exists() {
                let lines: Vec<Value> = self.lines()?.into_iter().filter(|v| v["job"] == job).collect();
                let mut traces = Vec::new();
                for v in &lines {
                    if let Some(tx) = v["tx"].as_str() {
                        traces.push(self.trace(tx).await);
                    }
                }
                bail!(
                    "{job} was refused: {}\ntraces: {}",
                    lines.iter().map(|v| v.to_string()).collect::<Vec<_>>().join("\n"),
                    traces.join("\n")
                );
            }
            ensure!(t0.elapsed() < Duration::from_secs(secs), "{job} not done within {secs} s");
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        Ok(())
    }

    fn put_job(&mut self, name: &str, job: Value, nullifier: Option<B256>) -> Result<()> {
        let tmp = self.inbox.join(format!("{name}.part"));
        std::fs::write(&tmp, serde_json::to_vec_pretty(&job)?)?;
        std::fs::rename(&tmp, self.inbox.join(name))?;
        self.jobs.insert(name.to_string(), nullifier);
        Ok(())
    }

    /// A release job of denomination `d` under the checkpoint's burn root, paying this relayer its fee.
    fn release_job(&mut self, d: usize) -> Result<String> {
        self.releases += 1;
        let i = self.releases;
        let nullifier = sha256(format!("relayer crash test: release {i}").as_bytes());
        let recipient: [u8; 20] = sha256(format!("relayer crash test: recipient {i}").as_bytes())[12..].try_into()?;
        let mut chain = [0u8; 32];
        chain[24..].copy_from_slice(&self.chain_id.to_be_bytes());
        let j = ReleaseJournal {
            chain_id: chain,
            vault: self.vault.into_array(),
            lez_config_id: self.cid.0,
            burn_root: self.burn_root.0,
            nullifier,
            token: [0; 20],
            amount: U256::from(DENOMS[d]).to_be_bytes(),
            recipient,
            relayer: self.relayer.into_array(),
            relay_fee: U256::from(RELAY_FEES[d]).to_be_bytes(),
        };
        let name = format!("release-{i:02}.json");
        let job = json!({ "kind": "release", "journal": format!("0x{}", hex::encode(j.encode())), "seal": SEAL });
        self.put_job(&name, job, Some(B256::from(nullifier)))?;
        Ok(name)
    }

    async fn count_logs(&self, topic0: B256, topic1: Option<B256>) -> Result<usize> {
        let mut f = Filter::new().address(self.vault).event_signature(topic0).from_block(0u64);
        if let Some(t) = topic1 {
            f = f.topic1(t);
        }
        Ok(self.p.get_logs(&f).await?.len())
    }

    /// The invariants of every part (module documentation); returns a summary.
    async fn check(&self, part: &str) -> Result<Value> {
        let candidates = self.count_logs(IRelayVault::CandidateSubmitted::SIGNATURE_HASH, None).await?;
        let finals = self.count_logs(IVault::CheckpointFinalized::SIGNATURE_HASH, None).await?;
        ensure!(candidates == 1 && finals == 1, "{part}: {candidates} CandidateSubmitted, {finals} CheckpointFinalized");
        let lines = self.lines()?;
        let mut releases = 0;
        for (job, nullifier) in &self.jobs {
            ensure!(self.state.join("done").join(job).exists(), "{part}: {job} is not in done/");
            ensure!(!self.state.join("refused").join(job).exists(), "{part}: {job} is in refused/");
            ensure!(!self.inbox.join(job).exists(), "{part}: {job} is still in the inbox");
            let booked: Vec<&Value> = lines.iter().filter(|v| v["event"] == "job-done" && v["job"] == *job).collect();
            ensure!(!booked.is_empty(), "{part}: {job} done without a job-done line");
            let txs: BTreeSet<String> = booked
                .iter()
                .filter_map(|v| v["result"]["tx"].as_str().map(str::to_string))
                .collect();
            ensure!(txs.len() <= 1, "{part}: {job} booked with two transactions: {txs:?}");
            if let Some(n) = nullifier {
                let n_logs = self.count_logs(IVault::Release::SIGNATURE_HASH, Some(*n)).await?;
                ensure!(n_logs == 1, "{part}: {n_logs} Release events for {n} ({job})");
                ensure!(
                    eth::view(&self.p, self.vault, IVault::nullifierUsedCall { nullifier: *n }, None).await?,
                    "{part}: {n} not used"
                );
                releases += 1;
            }
        }
        let (latest, pending) = self.nonces().await?;
        ensure!(latest == pending, "{part}: {} transactions of the relayer still pending", pending - latest);
        ensure!(latest == self.effects, "{part}: the relayer sent {latest} transactions for {} effects", self.effects);
        // Every transaction of the relayer mined with status 1.
        let head = self.p.get_block_number().await?;
        let mut mined = 0u64;
        for n in 0..=head {
            use alloy::network::TransactionResponse as _;
            let Some(b) = self.p.get_block_by_number(n.into()).full().await? else { continue };
            for tx in b.transactions.txns() {
                if tx.from() == self.relayer {
                    mined += 1;
                    let rc = self.p.get_transaction_receipt(tx.tx_hash()).await?.context("receipt")?;
                    ensure!(rc.status(), "{part}: the relayer's transaction {} reverted", tx.tx_hash());
                }
            }
        }
        ensure!(mined == latest, "{part}: {mined} mined transactions of the relayer, nonce {latest}");
        let recovered = lines.iter().filter(|v| v["recovered"] == true).count();
        let events: BTreeMap<String, usize> = lines.iter().fold(BTreeMap::new(), |mut m, v| {
            *m.entry(v["event"].as_str().unwrap_or("?").to_string()).or_default() += 1;
            m
        });
        let s = json!({
            "part": part, "daemonRuns": self.runs, "jobs": self.jobs.len(), "releases": releases,
            "relayerTransactions": latest, "reverted": 0, "candidateSubmitted": candidates, "checkpointFinalized": finals,
            "linesRecovered": recovered, "events": events,
        });
        println!("CHECK {s}");
        Ok(s)
    }
}

/// One kill at the relayer's next transaction: the case (P, M, D) and the selector it must have.
async fn kill_at(w: &mut World, case: char, want: [u8; 4], what: &str) -> Result<Value> {
    let d = w.start(None)?;
    let (hash, nonce, sel) = w.pending(60).await.with_context(|| format!("{what}: the first send"))?;
    ensure!(sel == want, "{what}: the pending transaction is {} (expected {})", hex::encode(sel), hex::encode(want));
    w.kill(d)?;
    let mut out = json!({ "what": what, "case": case.to_string(), "killedWithPending": hash, "nonce": nonce });
    match case {
        'P' => {
            let d = w.start(None)?;
            tokio::time::sleep(Duration::from_secs(4)).await;
            let (latest, pending) = w.nonces().await?;
            ensure!(pending == latest + 1, "{what} P: {} transactions pending after the restart", pending - latest);
            let (h2, _, _) = w.pending(1).await?;
            ensure!(h2 == hash, "{what} P: another transaction in the pool after the restart");
            ensure!(w.last_run_has("inflight-pending"), "{what} P: the restarted daemon did not wait for it");
            w.mine().await?;
            w.wait_last_run("the booking", |v| v["recovered"] == true, 30).await?;
            out["restartedSawPending"] = json!(true);
            w.kill(d)?;
        }
        'M' => {
            w.mine().await?;
            let d = w.start(None)?;
            w.wait_last_run("the booking", |v| v["recovered"] == true, 30).await?;
            tokio::time::sleep(Duration::from_secs(2)).await;
            out["minedBeforeRestart"] = json!(true);
            // A claimOwed may follow a booked release in the same round (part 1's last step); left to the caller.
            out["pendingAfter"] = json!(w.nonces().await?.1 - w.nonces().await?.0);
            w.kill(d)?;
        }
        'D' => {
            rpc(&w.p, "anvil_dropTransaction", json!([hash])).await?;
            let (latest, pending) = w.nonces().await?;
            ensure!(latest == pending, "{what} D: the pool still holds the transaction");
            let d = w.start(None)?;
            let (h2, n2, s2) = w.pending(60).await.with_context(|| format!("{what} D: the second send"))?;
            ensure!(n2 == nonce && s2 == want, "{what} D: sent again with nonce {n2} (first {nonce})");
            ensure!(w.last_run_has("inflight-lost"), "{what} D: no inflight-lost line");
            w.mine().await?;
            w.wait_last_run("the booking", |v| v["recovered"] == true, 30).await?;
            out["resentWithSameNonce"] = json!(h2);
            w.kill(d)?;
        }
        _ => bail!("case {case}"),
    }
    w.effects += 1;
    println!("KILL {out}");
    Ok(out)
}

#[tokio::test(flavor = "multi_thread")]
async fn relayer_killed_and_cut_off_does_each_job_once() -> Result<()> {
    if std::env::var("FERRATA_RELAYER_CRASH").is_err() {
        eprintln!("skipped: FERRATA_RELAYER_CRASH is not set (cli/tests/relayer-crash.sh runs it)");
        return Ok(());
    }
    let dir = scratch();
    let port = 18645u16;
    let url = format!("http://127.0.0.1:{port}");
    let _anvil = Proc(
        Command::new("anvil")
            .args(["--port", &port.to_string(), "--silent", "--code-size-limit", "100000"])
            .stdout(Stdio::null())
            .spawn()
            .context("starting anvil")?,
    );
    let p = eth::provider(&url)?;
    for _ in 0..100 {
        if p.get_block_number().await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // The vault: MonitorFixture (mode B, accept-all verifier), ETH registered, deposits.
    let vault_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../vault");
    let genesis = B256::from(sha256(b"relayer crash test: genesis"));
    let out = Command::new("forge")
        .current_dir(&vault_dir)
        .args(["script", "script/MonitorFixture.s.sol:MonitorFixture", "--rpc-url", &url, "--broadcast", "--unlocked"])
        .args(["--sender", &ANVIL[0].to_string()])
        .env("FIXTURE_GENESIS", genesis.to_string())
        .env("FIXTURE_ADMIN", ANVIL[0].to_string())
        .env("FIXTURE_FREEZER", ANVIL[1].to_string())
        .env("FIXTURE_WATCHER", ANVIL[2].to_string())
        .env("FIXTURE_FEES", ANVIL[3].to_string())
        .output()?;
    ensure!(out.status.success(), "forge script: {}", String::from_utf8_lossy(&out.stderr));
    let broadcast: Value = serde_json::from_slice(&std::fs::read(
        vault_dir.join("broadcast/MonitorFixture.s.sol/31337/run-latest.json"),
    )?)?;
    let vault: Address = broadcast["transactions"]
        .as_array()
        .context("transactions")?
        .iter()
        .rev()
        .find(|t| t["contractName"] == "FerrataVault")
        .and_then(|t| t["contractAddress"].as_str())
        .context("no FerrataVault in the broadcast")?
        .parse()?;
    let unlimited = IAdmin::CapParams { capacity: U256::MAX, refillPerSecond: U256::ZERO };
    let params = IAdmin::TokenParams {
        denominations: DENOMS.iter().map(|d| U256::from(*d)).collect(),
        relayFees: RELAY_FEES.iter().map(|d| U256::from(*d)).collect(),
        lezUnit: U256::from(1_000_000_000u64),
        unitWeight: U256::from(1u64),
        depositCap: unlimited.clone(),
        releaseCap: unlimited,
        releaseDelay: U256::ZERO,
        lezDefinition: B256::ZERO,
        decimals: 18,
        checkpointFees: CHECKPOINT_FEES.iter().map(|d| U256::from(*d)).collect(),
    };
    from_anvil(&p, ANVIL[0], vault, IAdmin::registerTokenCall { token: Address::ZERO, p: params }.abi_encode(), U256::ZERO)
        .await?;
    // Deposits from anvil's account 3: 24 of 0.1 ETH and 3 of 1 ETH, 5.4 ETH: what the 45 releases of parts 1-6 pay out
    // (44 of 0.1 ETH, 1 of 1 ETH); a part with more releases needs more deposits.
    let depositor = ANVIL[3];
    let mut j = 0u64;
    for (d, count) in [(0u8, 24), (1u8, 3)] {
        for _ in 0..count {
            let price = eth::deposit_price(&p, vault, eth::ETH, d).await?;
            let c = B256::from(sha256(format!("relayer crash test: deposit {j}").as_bytes()));
            let input = eth::deposit_calldata(eth::ETH, d, c, j).to_vec();
            from_anvil(&p, depositor, vault, input, U256::from(price.total)).await?;
            j += 1;
        }
    }
    let chain_id = p.get_chain_id().await?;
    let cid = eth::view(&p, vault, IVault::lezConfigIdCall {}, None).await?;
    ensure!(eth::view(&p, vault, IRelayVault::finalStateCall {}, None).await? == genesis, "genesis");

    // The relayer's key.
    let password = dir.join("password.txt");
    std::fs::write(&password, format!("{PASSWORD}\n"))?;
    let (signer, _) =
        PrivateKeySigner::new_keystore(&dir, &mut rand::thread_rng(), PASSWORD, Some("keystore-relayer.json"))?;
    let relayer = signer.address();
    rpc(&p, "anvil_setBalance", json!([relayer, "0x56bc75e2d63100000"])).await?; // 100 ETH, local only
    let mut w = World {
        dir: dir.clone(),
        rpc: url.clone(),
        p: p.clone(),
        vault,
        chain_id,
        cid,
        relayer,
        keystore: dir.join("keystore-relayer.json"),
        password,
        state: dir.join("state"),
        inbox: dir.join("inbox"),
        runs: 0,
        jobs: BTreeMap::new(),
        logos_stub: None,
        effects: 0,
        new_state: B256::from(sha256(b"relayer crash test: state 1")),
        burn_root: B256::from(sha256(b"relayer crash test: burns")),
        releases: 0,
    };
    std::fs::create_dir_all(&w.inbox)?;
    println!("anvil: vault {vault}, chain {chain_id}, relayer {relayer}");
    let mut results = json!({ "vault": vault, "chainId": chain_id, "relayer": relayer, "part1": [], "parts": [] });

    // ---- part 1: a kill at each kind of transaction --------------------------------------------------------
    let c = IRelayVault::Checkpoint {
        nMin: 3,
        tMin: 2,
        parentState: genesis,
        newState: w.new_state,
        burnRoot: w.burn_root,
        burnCount: 64,
        importedLeafCount: 0,
        sunset: false,
        echoNonce: B256::ZERO,
        tipId: 1,
        tipHash: B256::from(sha256(b"relayer crash test: tip")),
        n: 4,
        t: 3,
    };
    let journal = relayer::checkpoint_journal(chain_id, vault, cid, &c);
    w.put_job(
        "checkpoint-1.json",
        json!({ "kind": "checkpoint", "journal": format!("0x{}", hex::encode(&journal)), "seal": SEAL }),
        None,
    )?;
    let sel = |s: [u8; 4]| s;
    w.automine(false).await?;
    let mut part1 = Vec::new();
    part1.push(kill_at(&mut w, 'P', sel(IRelayVault::submitCandidateCall::SELECTOR), "submitCandidate").await?);
    w.wait_done("checkpoint-1.json", 1).await?;
    // The wait (Δ = 1 h) let pass on anvil's clock.
    rpc(&p, "evm_increaseTime", json!([3601])).await?;
    w.mine().await?;
    part1.push(kill_at(&mut w, 'M', sel(IRelayVault::finalizeAttestedCall::SELECTOR), "finalizeAttested").await?);
    ensure!(eth::view(&p, vault, IRelayVault::finalStateCall {}, None).await? == w.new_state, "finalised");
    let r1 = w.release_job(0)?;
    part1.push(kill_at(&mut w, 'D', sel(IVault::releaseCall::SELECTOR), "release (denomination 0)").await?);
    w.wait_done(&r1, 1).await?;
    let r2 = w.release_job(0)?;
    part1.push(kill_at(&mut w, 'P', sel(IVault::releaseCall::SELECTOR), "release (denomination 0)").await?);
    w.wait_done(&r2, 1).await?;
    // A release with a checkpoint fee: the relayer, which submitted the checkpoint, is owed the fee and withdraws
    // it with claimOwed in the round after the release (the kill falls on each).
    let r3 = w.release_job(1)?;
    part1.push(kill_at(&mut w, 'M', sel(IVault::releaseCall::SELECTOR), "release with a checkpoint fee").await?);
    w.wait_done(&r3, 1).await?;
    // The M restart above was killed with the claimOwed in the pool, if it got that far: drop it so the next
    // kill sees one fresh send; either way the claimOwed below must be exactly one mined transaction.
    if let Ok((h, _, s)) = w.pending(1).await {
        ensure!(s == IRelayVault::claimOwedCall::SELECTOR, "an unexpected pending transaction");
        rpc(&p, "anvil_dropTransaction", json!([h])).await?;
    }
    part1.push(kill_at(&mut w, 'D', sel(IRelayVault::claimOwedCall::SELECTOR), "claimOwed").await?);
    let owed = eth::view(&p, vault, IRelayVault::owedCall { payee: relayer, token: Address::ZERO }, None).await?;
    ensure!(owed.is_zero(), "the relayer is still owed {owed}");
    let claimed = w.count_logs(IAdmin::OwedClaimed::SIGNATURE_HASH, None).await?;
    ensure!(claimed == 1, "{claimed} OwedClaimed events");
    w.automine(true).await?;
    results["part1"] = json!(part1);
    results["parts"].as_array_mut().expect("array").push(w.check("part 1 (kills at each transaction)").await?);

    // ---- part 2: random kills ------------------------------------------------------------------------------
    let names: Vec<String> = (0..20).map(|_| w.release_job(0)).collect::<Result<_>>()?;
    w.effects += names.len() as u64;
    let mut rng = rand::thread_rng();
    let mut kills = 0;
    while names.iter().any(|n| !w.state.join("done").join(n).exists()) {
        ensure!(kills < 400, "part 2: jobs not done after {kills} kills");
        let d = w.start(None)?;
        w.wait_last_run("the start-up line", |v| v["event"] == "start", 60).await?;
        let ms = rand::Rng::gen_range(&mut rng, 0..40);
        tokio::time::sleep(Duration::from_millis(ms)).await;
        w.kill(d)?;
        kills += 1;
    }
    // One last start lets a killed daemon's record be booked; then it is stopped.
    let d = w.start(None)?;
    tokio::time::sleep(Duration::from_secs(4)).await;
    w.kill(d)?;
    ensure!(!Path::new(&relayerd_inflight(&w.state)).exists(), "part 2: a record left in flight");
    let mut s2 = w.check("part 2 (random kills)").await?;
    s2["kills"] = json!(kills);
    results["parts"].as_array_mut().expect("array").push(s2);

    // ---- part 3: failing endpoints ---------------------------------------------------------------------------
    let faultnet = Path::new(env!("CARGO_MANIFEST_DIR")).join("../e2e/netfault/faultnet.py");
    // A block every second: a lagging node catches up with the relayer's own transactions.
    let ticker = {
        let p = p.clone();
        tokio::spawn(async move {
            loop {
                let _ = p.raw_request::<_, Value>("evm_mine".into(), json!([])).await;
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        })
    };
    for (seed, kind) in [(11u64, "flaky"), (16, "sends"), (12, "lagging")] {
        let ports = dir.join(format!("faultnet-{seed}.ports"));
        let flog = dir.join(format!("faultnet-{seed}.jsonl"));
        let fout = dir.join(format!("faultnet-{seed}.out"));
        let _fn = Proc(
            Command::new("python3")
                .arg("-I")
                .arg(&faultnet)
                .args(["--upstream", &url, "--seed", &seed.to_string()])
                .arg("--port-file")
                .arg(&ports)
                .arg("--log")
                .arg(&flog)
                .stdout(Stdio::from(std::fs::File::create(&fout)?))
                .stderr(Stdio::null())
                .spawn()
                .context("starting faultnet")?,
        );
        let t0 = Instant::now();
        let desc: Value = loop {
            if let Some(line) = std::fs::read_to_string(&fout).ok().and_then(|t| t.lines().next().map(str::to_string)) {
                break serde_json::from_str(&line)?;
            }
            ensure!(t0.elapsed() < Duration::from_secs(20), "faultnet did not start");
            tokio::time::sleep(Duration::from_millis(100)).await;
        };
        let ep = desc["endpoints"]
            .as_array()
            .context("endpoints")?
            .iter()
            .find(|e| e["kind"] == kind)
            .context("no such endpoint")?
            .clone();
        let ep_url = format!("http://127.0.0.1:{}", ep["port"]);
        let names: Vec<String> = (0..4).map(|_| w.release_job(0)).collect::<Result<_>>()?;
        w.effects += names.len() as u64;
        let d = w.start(Some(&ep_url))?;
        let t0 = Instant::now();
        while names.iter().any(|n| !w.state.join("done").join(n).exists()) {
            for n in &names {
                ensure!(!w.state.join("refused").join(n).exists(), "part 3 ({kind}): {n} refused");
            }
            ensure!(t0.elapsed() < Duration::from_secs(400), "part 3 ({kind}): jobs not done within 400 s");
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        let secs = t0.elapsed().as_secs();
        // The daemon may still hold a record it cannot see mined yet (a lagging node): give it rounds to clear.
        let t1 = Instant::now();
        while Path::new(&relayerd_inflight(&w.state)).exists() && t1.elapsed() < Duration::from_secs(120) {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        w.kill(d)?;
        let faults: BTreeMap<String, usize> = std::fs::read_to_string(&flog)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .filter(|v| v["ep"] == ep["ep"] || v["fault"] == "outage")
            .fold(BTreeMap::new(), |mut m, v| {
                *m.entry(v["fault"].as_str().unwrap_or("?").to_string()).or_default() += 1;
                m
            });
        let mut s3 = w.check(&format!("part 3 ({kind} endpoint, seed {seed})")).await?;
        s3["endpoint"] = ep;
        s3["outagesS"] = desc["outages_s"].clone();
        s3["faultsSeen"] = json!(faults);
        s3["secondsToDone"] = json!(secs);
        results["parts"].as_array_mut().expect("array").push(s3);
    }
    ticker.abort();
    w.automine(true).await?;

    // ---- part 4: the record's narrow windows, made to happen ---------------------------------------------------------
    let hp = Helper::start(&dir, "holdproxy", "e2e/netfault/holdproxy.py", &url)?;
    let proxy = hp.url().await?;
    let mut part4 = Vec::new();
    // E1: the send's answer lost (mined: automine); no kill: the next round finds it under its nonce.
    hp.arm("lose eth_sendRawTransaction")?;
    let r = w.release_job(0)?;
    w.effects += 1;
    let d = w.start(Some(&proxy))?;
    w.wait_done(&r, 120).await?;
    ensure!(w.last_run_has("inflight-found"), "part 4 E1: no inflight-found line");
    w.kill(d)?;
    part4.push(json!({ "case": "answer lost", "fired": hp.hits(), "booked": "inflight-found" }));
    // E2: the send forwarded, its answer held, the daemon killed; restarted on the anvil: found under its nonce.
    let h = hp.hits();
    hp.arm("hold eth_sendRawTransaction 30")?;
    let r = w.release_job(0)?;
    w.effects += 1;
    let d = w.start(Some(&proxy))?;
    hp.wait_hit(h, 120).await?;
    w.kill(d)?;
    let d = w.start(None)?;
    w.wait_done(&r, 120).await?;
    ensure!(w.last_run_has("inflight-found"), "part 4 E2: no inflight-found line after the restart");
    w.kill(d)?;
    part4.push(json!({ "case": "answer held, killed", "fired": hp.hits(), "booked": "inflight-found" }));
    // E3: the daemon killed with its record written and nothing sent; restarted: the same call, the same nonce.
    let h = hp.hits();
    hp.arm("trap eth_sendRawTransaction")?;
    let r = w.release_job(0)?;
    w.effects += 1;
    let d = w.start(Some(&proxy))?;
    hp.wait_hit(h, 120).await?;
    let (latest, pending) = w.nonces().await?;
    ensure!(latest == pending && latest == w.effects - 1, "part 4 E3: something was sent ({latest}, {pending})");
    let recorded = ferrata_cli::relayerd::Inflight::load(&w.state)?.context("part 4 E3: no record")?;
    ensure!(recorded.hash.is_none(), "part 4 E3: the record has a hash");
    w.kill(d)?;
    let d = w.start(None)?;
    w.wait_done(&r, 120).await?;
    ensure!(
        w.last_run_has("inflight-lost") && w.last_run_has("inflight-resent"),
        "part 4 E3: not resent as lost after the restart"
    );
    w.kill(d)?;
    part4.push(json!({ "case": "killed with the record written, nothing sent", "fired": hp.hits(), "nonce": recorded.nonce,
                       "booked": "inflight-lost, inflight-resent (same nonce)" }));
    drop(hp);
    let mut s4 = w.check("part 4 (lost answer, held answer and kill, kill before the send)").await?;
    s4["cases"] = json!(part4);
    results["parts"].as_array_mut().expect("array").push(s4);

    // ---- part 5: the --send-via logos route killed in each of its windows ----------------------------------------------
    rpc(&p, "anvil_impersonateAccount", json!([relayer])).await?; // the stand-in has anvil sign for it (local only)
    let ts = Helper::start(&dir, "txsender", "e2e/netfault/txsender_stub.py", &url)?;
    w.logos_stub = Some(ts.addr().await?);
    let mut part5 = Vec::new();
    for (fault, what, line) in [
        ("stall send_status", "request made, nothing broadcast", "inflight-request-driven"),
        ("hold send 30", "request made, its id unknown", "inflight-request-found"),
        ("hold send_status 30", "broadcast, its hash unknown", "inflight-request-driven"),
    ] {
        let h = ts.hits();
        ts.arm(fault)?;
        let r = w.release_job(0)?;
        w.effects += 1;
        let d = w.start(None)?;
        ts.wait_hit(h, 120).await?;
        let (latest, _) = w.nonces().await?;
        w.kill(d)?;
        let d = w.start(None)?;
        w.wait_done(&r, 120).await?;
        ensure!(w.last_run_has(line), "part 5 ({fault}): no {line} line after the restart");
        ensure!(w.last_run_has("inflight-request-driven"), "part 5 ({fault}): the request was not driven to its end");
        w.kill(d)?;
        part5.push(json!({ "fault": fault, "window": what, "minedBeforeRestart": latest == w.effects, "booked": line }));
    }
    let made = ts.log_count(|v| v["method"] == "send" && v["requestId"].is_string() && !v["action"].as_str().unwrap_or("").contains("not delivered"));
    let jobs5 = part5.len();
    ensure!(made == jobs5, "part 5: {made} requests made at tx_sender_module's stand-in for {jobs5} jobs");
    drop(ts);
    w.logos_stub = None;
    let mut s5 = w.check("part 5 (the logos route killed in each window)").await?;
    s5["cases"] = json!(part5);
    s5["requestsMade"] = json!(made);
    results["parts"].as_array_mut().expect("array").push(s5);

    // ---- part 6: what tx_sender_module cannot account for (the second review's M2) ---------------------------
    let history = dir.join("txsender6.history.jsonl").to_string_lossy().into_owned();
    let with_history = ["--history".to_string(), history];
    let requests = |ts: &Helper| ts.log_count(|v| v["method"] == "send" && v["requestId"].is_string());
    let mut part6 = Vec::new();
    let mut ts = Helper::start_with(&dir, "txsender6a", "e2e/netfault/txsender_stub.py", &url, &with_history)?;
    w.logos_stub = Some(ts.addr().await?);
    // (a) broadcast, its answer held, the daemon killed; the module restarted; the transaction still pending.
    w.automine(false).await?;
    let h = ts.hits();
    ts.arm("hold send_status 30")?;
    let (l0, _) = w.nonces().await?;
    let r = w.release_job(0)?;
    w.effects += 1;
    let d = w.start(None)?;
    ts.wait_hit(h, 120).await?;
    let (_, pending_at_kill) = w.nonces().await?;
    ensure!(pending_at_kill == l0 + 1, "part 6a: the transaction is not pending at the kill ({l0}, {pending_at_kill})");
    w.kill(d)?;
    drop(ts);
    ts = Helper::start_with(&dir, "txsender6b", "e2e/netfault/txsender_stub.py", &url, &with_history)?;
    w.logos_stub = Some(ts.addr().await?);
    let d = w.start(None)?;
    w.wait_last_run("the transaction waited for", |v| v["event"] == "inflight-pending", 60).await?;
    ensure!(w.last_run_has("inflight-request-forgotten"), "part 6a: no inflight-request-forgotten line");
    ensure!(!w.state.join("refused").join(&r).exists(), "part 6a: {r} refused while its transaction is pending");
    w.mine().await?;
    w.wait_done(&r, 60).await?;
    w.kill(d)?;
    w.automine(true).await?;
    let made_a = requests(&ts);
    ensure!(made_a == 0, "part 6a: {made_a} requests made at the restarted module");
    part6.push(json!({ "case": "module restarted after the broadcast, transaction pending", "requestsAfterRestart": made_a,
                       "booked": "inflight-request-forgotten (history row), inflight-pending, then the receipt" }));
    // (b) the request made, nothing broadcast, the daemon killed; the module restarted: no row, nothing left.
    let h = ts.hits();
    ts.arm("stall send_status")?;
    let r = w.release_job(0)?;
    w.effects += 1;
    let d = w.start(None)?;
    ts.wait_hit(h, 120).await?;
    w.kill(d)?;
    drop(ts);
    ts = Helper::start_with(&dir, "txsender6c", "e2e/netfault/txsender_stub.py", &url, &with_history)?;
    w.logos_stub = Some(ts.addr().await?);
    let d = w.start(None)?;
    w.wait_done(&r, 120).await?;
    ensure!(w.last_run_has("inflight-request-forgotten"), "part 6b: no inflight-request-forgotten line");
    w.kill(d)?;
    let made_b = requests(&ts);
    ensure!(made_b == 1, "part 6b: {made_b} requests at the restarted module (the call planned again once)");
    part6.push(json!({ "case": "module restarted after the request, before any broadcast", "requestsAfterRestart": made_b,
                       "booked": "inflight-request-forgotten (no row), planned again" }));
    // (c) stuck, the transaction out (mined): found under the request's nonce; no second request.
    let (h, m0) = (ts.hits(), requests(&ts));
    ts.arm("stuck send_status")?;
    let r = w.release_job(0)?;
    w.effects += 1;
    let d = w.start(None)?;
    w.wait_done(&r, 120).await?;
    ensure!(ts.hits() > h, "part 6c: the fault did not fire");
    ensure!(w.last_run_has("inflight-request-unknown") && w.last_run_has("inflight-found"), "part 6c: not found under its nonce");
    w.kill(d)?;
    let made_c = requests(&ts) - m0;
    ensure!(made_c == 1, "part 6c: {made_c} requests for one stuck call");
    part6.push(json!({ "case": "stuck, the transaction out", "requests": made_c, "booked": "inflight-request-unknown, inflight-found" }));
    // (d) stuck, nothing sent: lost, sent again through the module with the nonce pinned.
    let (h, m0) = (ts.hits(), requests(&ts));
    ts.arm("stuck-unsent send_status")?;
    let r = w.release_job(0)?;
    w.effects += 1;
    let d = w.start(None)?;
    w.wait_done(&r, 120).await?;
    ensure!(ts.hits() > h, "part 6d: the fault did not fire");
    ensure!(w.last_run_has("inflight-lost") && w.last_run_has("inflight-resent"), "part 6d: not sent again as lost");
    w.kill(d)?;
    let made_d = requests(&ts) - m0;
    let pinned = ts.log_count(|v| v["method"] == "send" && v["pinned"] == true);
    ensure!(made_d == 2 && pinned == 1, "part 6d: {made_d} requests, {pinned} pinned (expected 2, 1)");
    part6.push(json!({ "case": "stuck, nothing sent", "requests": made_d, "pinned": pinned,
                       "booked": "inflight-request-unknown, inflight-lost, inflight-resent (pinned nonce)" }));
    drop(ts);
    w.logos_stub = None;
    let mut s6 = w.check("part 6 (what tx_sender_module cannot account for)").await?;
    s6["cases"] = json!(part6);
    results["parts"].as_array_mut().expect("array").push(s6);

    if let Ok(path) = std::env::var("FERRATA_CRASH_RESULTS") {
        std::fs::write(&path, serde_json::to_vec_pretty(&results)?)?;
    }
    println!("RESULTS {}", serde_json::to_string(&results)?);
    Ok(())
}

/// A Python helper of e2e/netfault (holdproxy.py, txsender_stub.py) in front of the anvil, its one-shot faults armed
/// through its control file; stopped when dropped.
struct Helper {
    proc: Proc,
    ctl: PathBuf,
    port: PathBuf,
    log: PathBuf,
}

impl Helper {
    fn start(dir: &Path, name: &str, script: &str, upstream: &str) -> Result<Self> {
        Self::start_with(dir, name, script, upstream, &[])
    }

    fn start_with(dir: &Path, name: &str, script: &str, upstream: &str, extra: &[String]) -> Result<Self> {
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(script);
        let (ctl, port, log) =
            (dir.join(format!("{name}.ctl")), dir.join(format!("{name}.port")), dir.join(format!("{name}.jsonl")));
        std::fs::write(&ctl, "pass\n")?;
        let child = Command::new("python3")
            .arg("-I")
            .arg(&script)
            .args(["--upstream", upstream])
            .arg("--port-file")
            .arg(&port)
            .arg("--ctl")
            .arg(&ctl)
            .arg("--log")
            .arg(&log)
            .args(extra)
            .stdout(Stdio::null())
            .stderr(Stdio::from(std::fs::File::create(dir.join(format!("{name}.err")))?))
            .spawn()
            .with_context(|| format!("starting {}", script.display()))?;
        Ok(Self { proc: Proc(child), ctl, port, log })
    }

    async fn addr(&self) -> Result<String> {
        let t0 = Instant::now();
        loop {
            if let Some(p) = std::fs::read_to_string(&self.port).ok().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()) {
                return Ok(format!("127.0.0.1:{p}"));
            }
            ensure!(t0.elapsed() < Duration::from_secs(20), "{} did not start", self.port.display());
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    async fn url(&self) -> Result<String> {
        Ok(format!("http://{}", self.addr().await?))
    }

    fn arm(&self, fault: &str) -> Result<()> {
        std::fs::write(&self.ctl, format!("{fault}\n")).map_err(Into::into)
    }

    fn hits(&self) -> usize {
        std::fs::read_to_string(self.ctl.with_extension("ctl.hit")).map_or(0, |t| t.lines().count())
    }

    async fn wait_hit(&self, before: usize, secs: u64) -> Result<()> {
        let t0 = Instant::now();
        while self.hits() <= before {
            ensure!(t0.elapsed() < Duration::from_secs(secs), "{}: the fault did not fire within {secs} s", self.ctl.display());
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        // The fault fired; the call it caught is held now.
        tokio::time::sleep(Duration::from_millis(300)).await;
        Ok(())
    }

    fn log_count(&self, f: impl Fn(&Value) -> bool) -> usize {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .filter(|v| f(v))
            .count()
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        let _ = self.proc.0.kill();
    }
}

fn relayerd_inflight(state: &Path) -> PathBuf {
    ferrata_cli::relayerd::Inflight::path(state)
}
