//! The relayer daemon on an anvil fork of Sepolia (local; nothing is sent to Sepolia), the way the
//! crown rehearsals run: the fork is taken at the block before `crown-live-1`'s `submitCandidate`
//! (`e2e/runs/crown-live-1/manifest.json`), so the live FerrataVault v5, its two deposits and RISC
//! Zero's Groth16 verifier are there and no candidate is. The proofs are that run's real ones: the
//! rule (B) checkpoint (`checkpoint/receipt/`) and the two releases (`releases/`), checked by the
//! real verifier on the fork.
//!
//! The honest relayer is the `ferrata-relayer` binary itself, its jobs dropped into its inbox; it
//! sends from the address the release proofs name as relayer (impersonated on the fork: no key).
//! A misbehaving relayer sends forged and tampered calls straight to the vault and is refused, and a
//! second daemon refuses a release that pays another relayer.
//!
//! Runs only with `FERRATA_RELAYER_FORK=1` and `anvil` on the path; `FERRATA_FORK_URL` overrides the
//! public Sepolia endpoints tried (they must serve state 1,000+ blocks back). `cli/tests/relayer.sh`.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use alloy::network::TransactionBuilder;
use alloy::primitives::{address, Address, Bytes, B256, U256};
use alloy::providers::{DynProvider, Provider};
use alloy::rpc::types::TransactionRequest;
use alloy::sol_types::SolCall;
use anyhow::{bail, ensure, Context, Result};
use ferrata_cli::eth::{self, IVault};
use ferrata_cli::relayer::{self, IRelayVault};
use serde_json::{json, Value};

const VAULT: Address = address!("fe3199fa2634413cbac317371076cae3eb2d9e54");
/// The relayer (and recipient) the recorded release proofs name.
const NAMED: Address = address!("79a95ae9a3f9000eb41f7cbab6a6f97826895c07");
/// A relayer that misbehaves (any address; impersonated on the fork).
const MALLORY: Address = address!("00000000000000000000000000000000000bad01");
/// The block of crown-live-1's `submitCandidate` (0xb8cf82); the fork is the block before it.
const SUBMIT_BLOCK: u64 = 0xb8cf82;
/// Public endpoints that served Sepolia's state at the fork block on 11 Nov 2026 (probed with
/// eth_getStorageAt; publicnode and drpc's free plan did not).
const ENDPOINTS: [&str; 3] =
    ["https://rpc.sepolia.ethpandaops.io", "https://1rpc.io/sepolia", "https://sepolia.rpc.thirdweb.com"];

struct Anvil(Child);
impl Drop for Anvil {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn read_hex(path: &Path) -> Result<String> {
    Ok(std::fs::read_to_string(path).with_context(|| path.display().to_string())?.trim().to_string())
}

async fn rpc(p: &DynProvider, method: &'static str, params: Value) -> Result<Value> {
    Ok(p.raw_request::<_, Value>(method.into(), params).await?)
}

/// Sends a call from an impersonated address with a fixed gas limit (no estimate), so a refused call is
/// mined and its receipt shows the revert; returns (status, the eth_call error before it).
async fn mallory_sends(p: &DynProvider, input: Vec<u8>) -> Result<(bool, String)> {
    let tx = TransactionRequest::default().with_from(MALLORY).with_to(VAULT).with_input(Bytes::from(input));
    let call_err = match p.call(tx.clone()).await {
        Ok(_) => String::new(),
        Err(e) => e.to_string(),
    };
    let receipt = p.send_transaction(tx.with_gas_limit(3_000_000)).await?.get_receipt().await?;
    Ok((receipt.status(), call_err))
}

/// Runs the honest daemon for `rounds` rounds; returns its JSON log lines.
fn daemon(url: &str, state: &Path, inbox: &Path, from: Address, extra: &[&str]) -> Result<Vec<Value>> {
    let from_block = (SUBMIT_BLOCK - 1).to_string();
    let from_s = from.to_string();
    let mut args = vec![
        "--eth-rpc",
        url,
        "--vault",
        "0xfe3199fa2634413cbac317371076cae3eb2d9e54",
        "--chain-id",
        "11155111",
        "--from-block",
        &from_block,
        "--unlocked",
        &from_s,
    ];
    let state_s = state.display().to_string();
    let inbox_s = inbox.display().to_string();
    args.extend_from_slice(&["--state", &state_s]);
    args.extend_from_slice(extra);
    args.extend_from_slice(&["run", "--inbox", &inbox_s, "--rounds", "1", "--poll-secs", "0"]);
    let out = Command::new(env!("CARGO_BIN_EXE_ferrata-relayer")).args(&args).stderr(Stdio::inherit()).output()?;
    ensure!(out.status.success(), "the daemon failed: {}", String::from_utf8_lossy(&out.stdout));
    let lines: Vec<Value> =
        String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    for l in &lines {
        println!("daemon {l}");
    }
    Ok(lines)
}

fn events<'a>(lines: &'a [Value], name: &str) -> Vec<&'a Value> {
    lines.iter().filter(|l| l["event"] == name).collect()
}

fn job(inbox: &Path, name: &str, v: Value) -> Result<()> {
    std::fs::create_dir_all(inbox)?;
    std::fs::write(inbox.join(name), serde_json::to_vec_pretty(&v)?)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn relayer_on_a_sepolia_fork_with_crown_live_1_proofs() -> Result<()> {
    if std::env::var("FERRATA_RELAYER_FORK").as_deref() != Ok("1") {
        eprintln!("skipped: set FERRATA_RELAYER_FORK=1 (cli/tests/relayer.sh)");
        return Ok(());
    }
    let run = repo().join("e2e/runs/crown-live-1");
    let work = PathBuf::from(std::env::var("FERRATA_RELAYER_WORK").unwrap_or_else(|_| "target/relayer-fork".into()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)?;
    let fork_block = SUBMIT_BLOCK - 1;

    // A public endpoint that serves the vault's state at the fork block.
    let endpoints: Vec<String> = match std::env::var("FERRATA_FORK_URL") {
        Ok(u) => vec![u],
        Err(_) => ENDPOINTS.iter().map(|s| s.to_string()).collect(),
    };
    let mut chosen = None;
    for e in &endpoints {
        let Ok(p) = eth::provider(e) else { continue };
        // Historical state, not only code: anvil reads accounts and storage at the fork block.
        let slot = p.get_storage_at(VAULT, U256::ZERO).block_id(fork_block.into()).await;
        let bal = p.get_balance(address!("f39Fd6e51aad88F6F4ce6aB8827279cffFb92266")).block_id(fork_block.into()).await;
        match (slot, bal) {
            (Ok(s), Ok(_)) if !s.is_zero() => {
                chosen = Some(e.clone());
                break;
            }
            (s, b) => println!(
                "endpoint {e}: no state at {fork_block} ({:?} / {:?})",
                s.err().map(|x| x.to_string()),
                b.err().map(|x| x.to_string())
            ),
        }
    }
    let fork_url = chosen.context("no endpoint serves Sepolia state at the fork block")?;
    println!("fork: {fork_url} at block {fork_block}");

    let port = 8551u16;
    let _anvil = Anvil(
        Command::new("anvil")
            .args(["--fork-url", &fork_url, "--fork-block-number", &fork_block.to_string(), "--auto-impersonate"])
            .args(["--port", &port.to_string(), "--silent", "--no-rate-limit"])
            .spawn()
            .context("starting anvil")?,
    );
    let url = format!("http://127.0.0.1:{port}");
    let p = eth::provider(&url)?;
    let mut up = false;
    for _ in 0..120 {
        if p.get_block_number().await.is_ok() {
            up = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    ensure!(up, "anvil did not start");
    ensure!(p.get_chain_id().await? == 11155111, "the fork is not Sepolia");
    for who in [NAMED, MALLORY] {
        rpc(&p, "anvil_setBalance", json!([who, "0x56BC75E2D63100000"])).await?;
    }
    let mut results = serde_json::Map::new();
    results.insert("fork".into(), json!({ "endpoint": fork_url, "block": fork_block, "vault": VAULT }));

    // The recorded proofs.
    let cp_journal = read_hex(&run.join("checkpoint/receipt/journal.hex"))?;
    let cp_seal = read_hex(&run.join("checkpoint/receipt/seal.hex"))?;
    let cp = relayer::checkpoint_from_journal(&relayer::unhex(&cp_journal)?)?;
    let final_before = eth::view(&p, VAULT, IRelayVault::finalStateCall {}, None).await?;
    ensure!(cp.checkpoint.parentState == final_before, "the fork's final state is not the checkpoint's parent");
    let mut releases = Vec::new();
    for name in ["public-0", "private-1"] {
        let r: Value = serde_json::from_slice(&std::fs::read(run.join(format!("releases/release-{name}.json")))?)?;
        let journal = r["journal"].as_str().context("journal")?.to_string();
        let seal = read_hex(&run.join(format!("releases/{name}/receipt/seal.hex")))?;
        releases.push((name, journal, seal));
    }

    // 1. A misbehaving relayer forges the checkpoint's burn root under the real seal: refused.
    let mut forged = cp.checkpoint.clone();
    forged.burnRoot = B256::repeat_byte(0x66);
    let (ok, call_err) = mallory_sends(
        &p,
        IRelayVault::submitCandidateCall { c: forged, seal: relayer::unhex(&cp_seal)?.into() }.abi_encode(),
    )
    .await?;
    ensure!(!ok, "a forged checkpoint was accepted");
    println!("forged checkpoint: refused (status 0); eth_call: {}", call_err.lines().next().unwrap_or(""));
    results.insert("forgedCheckpoint".into(), json!({ "refused": true, "ethCall": call_err.lines().next() }));

    // 2. The honest daemon submits the real checkpoint from its inbox; a second copy is skipped.
    let (state, inbox) = (work.join("state"), work.join("inbox"));
    job(&inbox, "01-checkpoint.json", json!({ "kind": "checkpoint", "journal": cp_journal, "seal": cp_seal }))?;
    let log1 = daemon(&url, &state, &inbox, NAMED, &["--accept-unpaid"])?;
    let done = events(&log1, "job-done");
    ensure!(done.len() == 1 && done[0]["result"]["kind"] == "candidate", "the candidate was not submitted");
    ensure!(events(&log1, "candidate-waiting").len() == 1, "the candidate is not waiting");
    results.insert("candidate".into(), done[0]["result"].clone());
    job(&inbox, "02-checkpoint-again.json", json!({ "kind": "checkpoint", "journal": cp_journal, "seal": cp_seal }))?;
    let log2 = daemon(&url, &state, &inbox, NAMED, &["--accept-unpaid"])?;
    let refused = events(&log2, "job-refused");
    ensure!(refused.len() == 1 && refused[0]["reason"].as_str().unwrap_or("").contains("already a candidate"));

    // 3. Releases before the root is final: the daemon's eth_call reports the vault's refusal.
    let (name0, j0, s0) = &releases[0];
    job(&inbox, &format!("03-release-{name0}-early.json"), json!({ "kind": "release", "journal": j0, "seal": s0 }))?;
    let log3 = daemon(&url, &state, &inbox, NAMED, &["--accept-unpaid"])?;
    let early = events(&log3, "job-refused");
    ensure!(early.len() == 1, "an early release was not refused");
    println!("early release: {}", early[0]["reason"]);
    results.insert("earlyRelease".into(), early[0]["reason"].clone());

    // 4. The wait W (deltaAtSubmit), then the daemon finalises the candidate (anyone may).
    let cand = eth::view(&p, VAULT, IRelayVault::candidatesCall { newState: cp.checkpoint.newState }, None).await?;
    let wait = u64::from(cand.deltaAtSubmit);
    rpc(&p, "evm_increaseTime", json!([wait + 1])).await?;
    rpc(&p, "evm_mine", json!([])).await?;
    let log4 = daemon(&url, &state, &inbox, NAMED, &["--accept-unpaid"])?;
    let fin = events(&log4, "finalized");
    ensure!(fin.len() == 1, "the candidate was not finalised");
    ensure!(eth::view(&p, VAULT, IRelayVault::finalStateCall {}, None).await? == cp.checkpoint.newState);
    results.insert("finalized".into(), json!({ "wait": wait, "log": fin[0] }));

    // 5. The release delay, then: a misbehaving relayer tampers with each release; refused.
    let root = cp.checkpoint.burnRoot;
    let status = eth::view(&p, VAULT, IVault::burnRootStatusCall { burnRoot: root }, None).await?;
    ensure!(status.accepted, "the burn root is not accepted");
    rpc(&p, "evm_increaseTime", json!([600])).await?;
    rpc(&p, "evm_mine", json!([])).await?;
    let v = relayer::Relayer { vault: VAULT, me: NAMED, accept_unpaid: true };
    let view = v.view(&p).await?;
    let (req0, _) = v.plan_release(&p, &view, &relayer::unhex(j0)?, relayer::unhex(s0)?.into()).await?;
    let fee_now = eth::view(&p, VAULT, IVault::feeStateCall { token: req0.token }, None).await?;
    let fees = eth::view(
        &p,
        VAULT,
        IVault::feesCall { token: req0.token, index: U256::from(fee_now.current), denomIndex: 0 },
        None,
    )
    .await?;
    let mut tampered = Vec::new();
    for (what, mut r) in [
        ("relayer and fee redirected to the misbehaving relayer", req0.clone()),
        ("recipient redirected", req0.clone()),
        ("fee raised", req0.clone()),
    ] {
        match what {
            "relayer and fee redirected to the misbehaving relayer" => {
                r.relayer = MALLORY;
                r.relayFee = fees.relayFee;
            }
            "recipient redirected" => r.recipient = MALLORY,
            _ => r.relayFee = fees.relayFee,
        }
        let (ok, call_err) = mallory_sends(&p, eth::release_calldata(r, relayer::unhex(s0)?.into()).to_vec()).await?;
        ensure!(!ok, "tampered release ({what}) accepted");
        println!("tampered release ({what}): refused; eth_call: {}", call_err.lines().next().unwrap_or(""));
        tampered.push(json!({ "what": what, "refused": true, "ethCall": call_err.lines().next() }));
    }
    results.insert("tamperedReleases".into(), json!(tampered));

    // 6. Another daemon, not the relayer the proof names, refuses the job before sending.
    let (state_m, inbox_m) = (work.join("state-other"), work.join("inbox-other"));
    job(&inbox_m, "01-release.json", json!({ "kind": "release", "journal": j0, "seal": s0 }))?;
    let log_m = daemon(&url, &state_m, &inbox_m, MALLORY, &["--accept-unpaid"])?;
    let other = events(&log_m, "job-refused");
    ensure!(other.len() == 1 && other[0]["reason"].as_str().unwrap_or("").contains("names relayer"));
    results.insert("otherRelayerDaemon".into(), other[0]["reason"].clone());
    // ... and an honest daemon that is not paid refuses an unpaid release by default.
    let (state_u, inbox_u) = (work.join("state-unpaid"), work.join("inbox-unpaid"));
    job(&inbox_u, "01-release.json", json!({ "kind": "release", "journal": j0, "seal": s0 }))?;
    let log_u = daemon(&url, &state_u, &inbox_u, NAMED, &[])?;
    ensure!(events(&log_u, "job-refused").len() == 1, "an unpaid release was taken without --accept-unpaid");

    // 7. The honest daemon releases both burns with the real proofs, and collects the checkpoint fees.
    let before = p.get_balance(NAMED).await?;
    for (name, j, s) in &releases {
        job(&inbox, &format!("04-release-{name}.json"), json!({ "kind": "release", "journal": j, "seal": s }))?;
    }
    let log5 = daemon(&url, &state, &inbox, NAMED, &["--accept-unpaid"])?;
    let done = events(&log5, "job-done");
    ensure!(done.len() == 2, "both releases were not sent: {log5:?}");
    for (_, j, _) in &releases {
        let n = B256::from_slice(&relayer::unhex(j)?[160..192]);
        ensure!(eth::view(&p, VAULT, IVault::nullifierUsedCall { nullifier: n }, None).await?, "NOut not used");
    }
    let collected = events(&log5, "fees-collected");
    ensure!(collected.len() == 1, "the checkpoint fees were not collected");
    let after = p.get_balance(NAMED).await?;
    results.insert(
        "releases".into(),
        json!({ "done": done.iter().map(|d| d["result"].clone()).collect::<Vec<_>>(), "feesCollected": collected[0],
                "balanceBefore": before.to_string(), "balanceAfter": after.to_string() }),
    );

    // 8. Replays: the misbehaving relayer resends a release; the daemon is handed it again.
    let (ok, call_err) =
        mallory_sends(&p, eth::release_calldata(req0.clone(), relayer::unhex(s0)?.into()).to_vec()).await?;
    ensure!(!ok, "a replayed release was accepted");
    job(&inbox, "05-release-again.json", json!({ "kind": "release", "journal": j0, "seal": s0 }))?;
    let log6 = daemon(&url, &state, &inbox, NAMED, &["--accept-unpaid"])?;
    let again = events(&log6, "job-refused");
    ensure!(again.len() == 1 && again[0]["reason"].as_str().unwrap_or("").contains("already released"));
    results.insert("replay".into(), json!({ "vault": call_err.lines().next(), "daemon": again[0]["reason"] }));

    let out = std::env::var("FERRATA_RELAYER_RESULTS").map(PathBuf::from).unwrap_or_else(|_| work.join("results.json"));
    std::fs::write(&out, serde_json::to_vec_pretty(&Value::Object(results))?)?;
    println!("results: {}", out.display());
    if !out.exists() {
        bail!("no results written");
    }
    Ok(())
}
