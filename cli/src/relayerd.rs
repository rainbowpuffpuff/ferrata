//! `ferrata-relayer`: the relayer daemon for the Ferrata bridge (prototype, unaudited). The binary
//! (`src/bin/relayer.rs`) and `ferrata_relayer_module` run this code: the same arguments, rounds and
//! JSON lines; the binary prints the lines and stops on SIGTERM or SIGINT, the module keeps them and
//! stops when it is asked to.
//!
//! One loop, every `--poll-secs`:
//! 0. **Root imports** (with `--import-roots` and a zone), on a fixed cadence (bridge.md §5.4): in the first
//!    round of each `--import-every` period of Ethereum time, the newest record of the configuration's
//!    light-client instance on LEZ, the vault's slots 0 and 1 at its block (`eth_getProof`), and, when it
//!    covers deposits LEZ has not imported, `ImportRoot`, built and dry-run with `ferrata_wallet::import`
//!    ([`relayer::plan_root_import`]) and sent; the import is confirmed from `root(cid, n)`. A period whose
//!    attempt waits (no newer record yet) or fails is tried again in its next rounds; at most one import is
//!    sent per period, whenever the deposits came (the period done is kept in `<state>/root-import.json`).
//! 1. **Inbox.** Each `<inbox>/*.json` job ([`relayer::Job`]) handed over by a user who opts in, or by
//!    the checkpoint prover: a release (journal and seal), a rule (B) checkpoint (journal and seal) or
//!    a gas drop (receipt). Checked ([`relayer`]), then sent; the file moves to `<state>/done/` or
//!    `<state>/refused/`, and one JSON line says what happened and why.
//! 2. **Candidates on the vault.** Every live candidate (anyone's, rebuilt from its event and checked
//!    against the digest the vault stored) whose wait is over is finalised with `finalizeAttested`.
//! 3. **Fees.** Whatever the vault credited this relayer (checkpoint fees; relay fees whose push
//!    failed) is withdrawn with `claimOwed`.
//! 4. **Burns on LEZ** (with a zone): the configuration's burns past the newest accepted burn
//!    count; if there are any and no live candidate, `<state>/needs-checkpoint.json` tells the
//!    checkpoint prover (a separate process: `e2e/crown`, stage 7) to cut one.
//!
//! Keys: an Ethereum keystore file (`--keystore`, `--password-file`), or, on a local anvil only,
//! `--unlocked` (the node signs). The LEZ side: a sequencer through LEZ's wallet with the relayer's own
//! seed (`--sequencer`, feature `lez-wallet`), or the mini-app devnet's zone (`--lez-devnet`, a stand-in
//! for tests: the devnet signs and settles). The relayer holds no user secret: see `cli/src/relayer.rs`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use alloy::network::TransactionBuilder as _;
use alloy::primitives::{Address, Bytes};
use alloy::providers::{DynProvider, Provider};
use alloy::rpc::types::{TransactionReceipt, TransactionRequest};
use alloy::sol_types::SolCall;
use anyhow::{anyhow, bail, Context as _, Result};
use clap::{Parser, Subcommand};
use ferrata_lez_bridge_wire::BridgeState;
use serde_json::{json, Value};
use sha2::Digest as _;

use crate::eth::{self, IVault};
use crate::lez::{self, Context, Ledger as _, Zone};
use crate::relayer::{self, DropBook, DropContext, IRelayVault, Job, Relayer, RootImport};
use crate::{account, h32, hx};

#[derive(Parser, Clone, Debug)]
#[command(name = "ferrata-relayer", version, about = "The Ferrata relayer daemon (prototype, unaudited)")]
pub struct Cli {
    /// Any Ethereum JSON-RPC endpoint.
    #[arg(long, env = "FERRATA_ETH_RPC", global = true)]
    pub eth_rpc: Option<String>,
    /// The vault's address.
    #[arg(long, env = "FERRATA_VAULT", global = true)]
    pub vault: Option<Address>,
    /// Refuse to run unless the endpoint serves this chain id.
    #[arg(long, env = "FERRATA_CHAIN_ID", global = true)]
    pub chain_id: Option<u64>,
    /// The relayer's state directory (drop book, candidates, done and refused jobs).
    #[arg(long, env = "FERRATA_RELAYER_STATE", default_value = "relayer-state", global = true)]
    pub state: PathBuf,
    /// An Ethereum keystore file (Web3 Secret Storage) for the relayer's account.
    #[arg(long, env = "FERRATA_KEYSTORE", global = true)]
    pub keystore: Option<PathBuf>,
    /// A file holding the keystore's password.
    #[arg(long, env = "FERRATA_PASSWORD_FILE", global = true)]
    pub password_file: Option<PathBuf>,
    /// Local anvil only: send from this address and let the node sign (no key here).
    #[arg(long, global = true)]
    pub unlocked: Option<Address>,
    /// Submit releases whose proof pays no relay fee.
    #[arg(long, global = true)]
    pub accept_unpaid: bool,
    /// First block of the vault's events (its deployment block).
    #[arg(long, env = "FERRATA_FROM_BLOCK", default_value_t = 0, global = true)]
    pub from_block: u64,
    /// The LEZ sequencer's RPC endpoint (root imports, gas drops and the burn watch).
    #[arg(long, env = "FERRATA_SEQUENCER", global = true)]
    pub sequencer: Option<String>,
    /// The mini-app devnet's zone (`cli/examples/miniapp_devnet.rs`, `http://127.0.0.1:PORT/rpc`) in place of a
    /// sequencer: a stand-in for tests (reads and calls over the devnet's HTTP methods; the devnet signs and
    /// settles them in its LEZ harness). Loopback addresses only.
    #[arg(long, env = "FERRATA_LEZ_DEVNET", global = true)]
    pub lez_devnet: Option<String>,
    /// The relayer's own seed file (its LEZ wallet's mnemonic).
    #[arg(long, env = "FERRATA_SEED", global = true)]
    pub seed: Option<PathBuf>,
    /// The LEZ wallet's home.
    #[arg(long, env = "FERRATA_LEZ_WALLET", global = true)]
    pub lez_wallet: Option<PathBuf>,
    /// A file holding the LEZ wallet's password.
    #[arg(long, env = "FERRATA_LEZ_PASSWORD_FILE", global = true)]
    pub lez_password_file: Option<PathBuf>,
    /// The bridge program's account id on LEZ (32-byte hex).
    #[arg(long, env = "FERRATA_BRIDGE_ACCOUNT", global = true)]
    pub bridge_account: Option<String>,
    /// The relayer's LEZ public account (32-byte hex): pays gas drops, named in gas-drop receipts.
    #[arg(long, env = "FERRATA_RELAYER_LEZ_ACCOUNT", global = true)]
    pub lez_account: Option<String>,
    /// The gas-drop guest's image id (32-byte hex).
    #[arg(long, env = "FERRATA_GASDROP_IMAGE_ID", global = true)]
    pub gasdrop_image_id: Option<String>,
    /// Native LEZ sent per gas drop.
    #[arg(long, default_value_t = 0, global = true)]
    pub drop_amount: u128,
    /// Import the vault's deposit roots on LEZ: each round, the bridge program's `ImportRoot` under the newest
    /// record of the configuration's light-client instance, when that record covers deposits LEZ has not
    /// imported (built and dry-run with ferrata-wallet's `import`, the code ferrata_bridge_module's
    /// `import_root` runs). Needs a zone (--sequencer or --lez-devnet) and --bridge-account.
    #[arg(long, global = true)]
    pub import_roots: bool,
    /// The root imports' period in seconds of Ethereum time: at most one import per period, tried from the first
    /// round of each (a fixed cadence, never in response to a deposit). 0: every round (tests).
    #[arg(long, default_value_t = 3600, global = true)]
    pub import_every: u64,
    /// The Groth16 prover, run as `<command> <input.json> <proof.json>` (RISC Zero's prover image: Docker on
    /// x86, or a remote prover); with it the relayer wraps holders' succinct release receipts.
    #[arg(long, env = "FERRATA_WRAP_COMMAND", global = true)]
    pub wrap_command: Option<PathBuf>,
    /// Seconds a job may wait in the inbox for a temporary condition (root not final, delay, freeze, cap).
    #[arg(long, default_value_t = 7 * 24 * 3600, global = true)]
    pub max_job_age: u64,
    /// Where the relayer's Ethereum transactions go: `plain` (signed here, or by a local anvil with --unlocked, and
    /// sent over --eth-rpc; the default) or `logos` (Logos Core's `tx_sender_module`, signed by `keystore_module`
    /// through `signer_manager_module`; only when the relayer runs as `ferrata_relayer_module`). With `logos` and a
    /// key of its own as well, a transaction `tx_sender_module` did not broadcast goes the plain way (the fallback).
    #[arg(long, default_value = "plain", value_parser = ["plain", "logos"], global = true)]
    pub send_via: String,
    /// With --send-via logos: the keystore account that sends (the relayer's address; its key's address when it
    /// has one here, which must then be the same).
    #[arg(long, global = true)]
    pub from: Option<Address>,
    /// With --send-via logos: seconds a transaction may wait for its approval before it is withdrawn.
    #[arg(long, default_value_t = 30, global = true)]
    pub approval_timeout: u64,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Clone, Debug)]
pub enum Command {
    /// The daemon.
    Run {
        /// The job inbox.
        #[arg(long, default_value = "relayer-inbox")]
        inbox: PathBuf,
        /// Seconds between rounds.
        #[arg(long, default_value_t = 12)]
        poll_secs: u64,
        /// Stop after this many rounds (0: run until stopped).
        #[arg(long, default_value_t = 0)]
        rounds: u64,
    },
    /// The vault's final state, live candidates, this relayer's balance and the drop book.
    Status,
}

/// Where the daemon's JSON lines go: the binary prints each on standard output, the module keeps them.
pub type Sink = Arc<dyn Fn(Value) + Send + Sync>;

/// One JSON line: the time, the event, its fields.
pub fn line(event: &str, fields: Value) -> Value {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let mut line = json!({ "t": t, "event": event });
    if let (Some(obj), Some(extra)) = (line.as_object_mut(), fields.as_object()) {
        obj.extend(extra.clone());
    }
    line
}

fn log(sink: &Sink, event: &str, fields: Value) {
    sink(line(event, fields));
}

struct Daemon {
    p: DynProvider,
    r: Relayer,
    state: PathBuf,
    from_block: u64,
    zone: Option<(Zone, Context)>,
    gasdrop_image_id: Option<[u8; 32]>,
    lez_me: Option<[u8; 32]>,
    drop_amount: u128,
    /// Tokens whose credited fees the relayer collects (ETH, and every token of a release it sent).
    tokens: std::collections::BTreeSet<Address>,
    /// The vault runs rule (B) (`mode() == 1`): candidates are finalised with `finalizeAttested`.
    rule_b: bool,
    /// A job deferred for longer than this (seconds) is refused.
    max_job_age: u64,
    /// The Groth16 prover command (wrapping), if any.
    wrap_command: Option<PathBuf>,
    /// `--import-roots`.
    import_roots: bool,
    /// `--import-every` (seconds; 0: every round).
    import_every: u64,
    /// The period whose import is done (sent, or nothing to send).
    import_done: Option<u64>,
    /// `--send-via logos`: the Ethereum transactions go through Logos Core's `tx_sender_module`.
    logos: Option<crate::logos_send::Route>,
    sink: Sink,
}

impl Daemon {
    /// One transaction to the vault: [`Relayer::submit`] (the plain path), or through `tx_sender_module` after the
    /// same `eth_call`, the receipt then read on the relayer's own node; what `tx_sender_module` did not broadcast
    /// goes the plain way when the relayer has a key here too (the fallback).
    async fn send(&self, input: Bytes, label: &'static str) -> Result<(TransactionReceipt, crate::logos_send::Via)> {
        use crate::logos_send::{Outcome, Via};
        let Some(route) = self.logos.clone() else {
            return Ok((self.r.submit(&self.p, input).await?, Via::Plain));
        };
        let fallback = route.fallback;
        let tx = TransactionRequest::default().with_from(self.r.me).with_to(self.r.vault).with_input(input.clone());
        if let Err(e) = self.p.call(tx).await {
            match eth::refusal_of(&e, "eth_call before sending") {
                Some(r) => bail!(r),
                None => bail!("the vault would revert: {e}"),
            }
        }
        let (vault, sent) = (self.r.vault, input.clone());
        let outcome = tokio::task::spawn_blocking(move || route.send(vault, &sent, label))
            .await
            .map_err(|e| anyhow!("the send through Logos Core did not finish: {e}"))?;
        match outcome {
            Outcome::Broadcast { hash, request_id, polls, millis } => {
                let t0 = std::time::Instant::now();
                let receipt = loop {
                    if let Some(r) = self.p.get_transaction_receipt(hash).await? {
                        break r;
                    }
                    if t0.elapsed() > Duration::from_secs(600) {
                        bail!("{hash:#x} (sent by tx_sender_module, request {request_id}) not mined within 600 s");
                    }
                    tokio::time::sleep(Duration::from_millis(250)).await;
                };
                anyhow::ensure!(receipt.status(), "the transaction {} reverted", receipt.transaction_hash);
                Ok((receipt, Via::Logos { request_id, polls, millis }))
            }
            Outcome::NotSent(reason) if fallback => {
                Ok((self.r.submit(&self.p, input).await?, Via::Fallback { reason }))
            }
            Outcome::NotSent(reason) => bail!("not sent: {reason}"),
            Outcome::Unknown(reason) => bail!("{reason}"),
        }
    }

    async fn round(&mut self, inbox: &Path) -> Result<()> {
        let s = self.sink.clone();
        let v = self.r.view(&self.p).await?;
        // 0. Root imports, once per period: before the inbox, whose gas drops check the imported roots.
        let period = if self.import_every == 0 { None } else { Some(v.now / self.import_every) };
        if self.import_roots && (period.is_none() || period != self.import_done) {
            match self.import_root().await {
                Ok(true) => {
                    if let Some(p) = period {
                        // Kept in the state directory: a restart does not import twice in one period.
                        std::fs::write(
                            self.state.join("root-import.json"),
                            serde_json::to_vec(&json!({ "every": self.import_every, "period": p }))?,
                        )?;
                    }
                    self.import_done = period;
                }
                Ok(false) => {}
                Err(e) => log(&s, "root-import-failed", json!({ "reason": format!("{e:#}") })),
            }
        }
        // 1. The inbox, in name order.
        let mut files: Vec<PathBuf> = match std::fs::read_dir(inbox) {
            Ok(dir) => dir
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .collect(),
            Err(_) => Vec::new(),
        };
        files.sort();
        for file in files {
            let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let outcome = self.job(&file, &v).await;
            let age = std::fs::metadata(&file)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let dir = match &outcome {
                Ok(_) => "done",
                // A root not final yet, a delay, a freeze, a cap, a transport failure: retried next round,
                // until the job is older than --max-job-age.
                Err(e) if relayer::temporary(&format!("{e:#}")) && age < self.max_job_age => {
                    log(&s, "job-deferred", json!({ "job": name, "reason": format!("{e:#}"), "ageSecs": age }));
                    continue;
                }
                Err(_) => "refused",
            };
            std::fs::create_dir_all(self.state.join(dir))?;
            std::fs::rename(&file, self.state.join(dir).join(&name))?;
            match outcome {
                Ok(fields) => log(&s, "job-done", json!({ "job": name, "result": fields })),
                Err(e) => log(&s, "job-refused", json!({ "job": name, "reason": format!("{e:#}") })),
            }
        }
        // 2. Candidates whose wait is over.
        let v = self.r.view(&self.p).await?;
        for (c, ready_at) in self.r.candidates(&self.p, self.from_block).await? {
            if !self.rule_b {
                // Rule (A) finalises with a second proof echoing a beacon: the checkpoint prover's job.
                log(
                    &s,
                    "candidate-not-finalized",
                    json!({ "newState": c.newState, "reason": "rule (A) vault: finalize needs a beacon proof" }),
                );
                continue;
            }
            if v.now < ready_at {
                log(&s, "candidate-waiting", json!({ "newState": c.newState, "readyAt": ready_at, "now": v.now }));
                continue;
            }
            let input: Bytes = IRelayVault::finalizeAttestedCall { c: c.clone() }.abi_encode().into();
            match self.send(input, "finalizeAttested").await {
                Ok((rc, via)) => log(
                    &s,
                    "finalized",
                    json!({ "newState": c.newState, "burnRoot": c.burnRoot, "burnCount": c.burnCount, "tx": rc.transaction_hash, "gasUsed": rc.gas_used, "via": via.to_json() }),
                ),
                Err(e) => log(&s, "finalize-refused", json!({ "newState": c.newState, "reason": format!("{e:#}") })),
            }
        }
        // 3. Fees credited to this relayer (checkpoint fees always; a relay fee whose push failed).
        for token in self.tokens.clone() {
            let owed =
                eth::view(&self.p, self.r.vault, IRelayVault::owedCall { payee: self.r.me, token }, None).await?;
            if owed.is_zero() {
                continue;
            }
            let input: Bytes = IRelayVault::claimOwedCall { token }.abi_encode().into();
            match self.send(input, "claimOwed").await {
                Ok((rc, via)) => log(
                    &s,
                    "fees-collected",
                    json!({ "token": token, "amount": owed.to_string(), "tx": rc.transaction_hash, "gasUsed": rc.gas_used, "via": via.to_json() }),
                ),
                Err(e) => log(&s, "fees-refused", json!({ "token": token, "reason": format!("{e:#}") })),
            }
        }
        // 4. Burns on LEZ past the newest accepted burn count.
        if let Some((zone, ctx)) = &self.zone {
            let history = lez::history_of(zone, ctx).await?;
            let accepted = eth::accepted_burn_roots(&self.p, self.r.vault, self.from_block, 10_000).await?;
            let live = self.r.candidates(&self.p, self.from_block).await?;
            let path = self.state.join("needs-checkpoint.json");
            if let Some((burns, covered)) =
                relayer::checkpoint_needed(history.burns.len() as u64, &accepted, live.len())
            {
                std::fs::write(
                    &path,
                    serde_json::to_vec_pretty(
                        &json!({ "burns": burns, "accepted": covered, "finalState": v.final_state }),
                    )?,
                )?;
                log(&s, "checkpoint-needed", json!({ "burns": burns, "accepted": covered }));
            } else if path.exists() {
                std::fs::remove_file(&path)?;
            }
        }
        Ok(())
    }

    /// Step 0: the vault's newest deposit root on LEZ, under the newest record of the configuration's instance.
    /// `Ok(true)` when the period's import is done (sent and applied, or nothing to import, or closed).
    async fn import_root(&mut self) -> Result<bool> {
        let s = self.sink.clone();
        let Some((zone, ctx)) = self.zone.as_mut() else { return Ok(true) };
        let history = lez::history_of(&*zone, ctx).await?;
        let vault_count = eth::view(&self.p, self.r.vault, IVault::nextLeafIndexCall {}, None).await?;
        let imported = history.roots.iter().copied().max().unwrap_or(0);
        let done = match relayer::plan_root_import(&*zone, &self.p, ctx, &history, vault_count).await {
            Ok(RootImport::UpToDate { .. }) => true,
            Ok(RootImport::Waiting { reason, vault_count, imported }) => {
                log(
                    &s,
                    "root-import-waiting",
                    json!({ "reason": reason, "vaultCount": vault_count, "imported": imported }),
                );
                false
            }
            Ok(RootImport::Closed { reason }) => {
                log(&s, "root-import-closed", json!({ "reason": reason }));
                true
            }
            Ok(RootImport::Planned(plan)) => {
                let mut fields = json!({
                    "leafCount": plan.leaf_count, "root": hx(&plan.root), "recordSlot": plan.record_slot,
                    "block": plan.block_number, "blockHash": hx(&plan.block_hash), "previous": plan.imported,
                    "vaultCount": vault_count, "accounts": plan.call.mentions.len(),
                });
                match zone.submit(&plan.call).await {
                    Ok(tx) => {
                        // Applied when root(cid, n) holds what the dry run said the import writes.
                        let at = ferrata_lez_bridge_wire::root_actor(ctx.program, &ctx.cid, plan.leaf_count);
                        let applied =
                            matches!(lez::read(&*zone, at).await?, Some(BridgeState::Root(r)) if r == plan.writes);
                        fields["tx"] = json!(tx);
                        fields["applied"] = json!(applied);
                        log(&s, if applied { "root-imported" } else { "root-import-unconfirmed" }, fields);
                        applied
                    }
                    Err(e) => {
                        fields["reason"] = json!(format!("{e:#}"));
                        log(&s, "root-import-failed", fields);
                        false
                    }
                }
            }
            Err(e) => {
                log(
                    &s,
                    "root-import-refused",
                    json!({ "reason": format!("{e:#}"), "vaultCount": vault_count, "imported": imported }),
                );
                false
            }
        };
        Ok(done)
    }

    async fn job(&mut self, file: &Path, v: &relayer::VaultView) -> Result<serde_json::Value> {
        let job: Job = serde_json::from_slice(&std::fs::read(file)?).context("not a relayer job")?;
        match job {
            Job::Release { journal, seal } => {
                let (req, input) =
                    self.r.plan_release(&self.p, v, &relayer::unhex(&journal)?, relayer::unhex(&seal)?.into()).await?;
                let (rc, via) = self.send(input, "release").await?;
                self.tokens.insert(req.token);
                let paid = relayer::relay_fee_paid(&rc, self.r.me);
                Ok(json!({
                    "kind": "release", "nullifier": req.nullifier, "recipient": req.recipient, "token": req.token,
                    "relayFee": req.relayFee.to_string(), "paidTo": self.r.me, "paid": paid.map(|(_, f)| f.to_string()),
                    "tx": rc.transaction_hash, "gasUsed": rc.gas_used, "via": via.to_json(),
                }))
            }
            Job::ReleaseSuccinct { receipt } => {
                let Some(cmd) = self.wrap_command.clone() else {
                    bail!("refused: this relayer does not wrap receipts (no --wrap-command)");
                };
                let receipt: risc0_zkvm::Receipt = serde_json::from_value(receipt).context("not a receipt")?;
                let journal = receipt.journal.bytes.clone();
                // The relayer's own checks first: nothing is wrapped for a release it would not send.
                self.r.plan_release(&self.p, v, &journal, Bytes::new()).await?;
                let image = eth::view(&self.p, self.r.vault, IRelayVault::releaseImageIdCall {}, None).await?;
                let work = self.state.join("wrap").join(hex::encode(&sha2::Sha256::digest(&journal)[..8]));
                let started = std::time::Instant::now();
                let seal = tokio::task::block_in_place(|| relayer::wrap_release(&receipt, image.0, &cmd, &work))?;
                let wrap_s = started.elapsed().as_secs_f64();
                let (req, input) = self.r.plan_release(&self.p, v, &journal, seal.into()).await?;
                let (rc, via) = self.send(input, "release").await?;
                self.tokens.insert(req.token);
                let paid = relayer::relay_fee_paid(&rc, self.r.me);
                Ok(json!({
                    "kind": "release", "wrapped": true, "wrapSecs": wrap_s, "nullifier": req.nullifier,
                    "recipient": req.recipient, "token": req.token, "relayFee": req.relayFee.to_string(),
                    "paidTo": self.r.me, "paid": paid.map(|(_, f)| f.to_string()), "tx": rc.transaction_hash,
                    "gasUsed": rc.gas_used, "via": via.to_json(),
                }))
            }
            Job::Checkpoint { journal, seal } => {
                let journal = relayer::unhex(&journal)?;
                let input = self.r.plan_candidate(&self.p, v, &journal, relayer::unhex(&seal)?.into()).await?;
                let (rc, via) = self.send(input, "submitCandidate").await?;
                let c = relayer::checkpoint_from_journal(&journal)?.checkpoint;
                Ok(
                    json!({ "kind": "candidate", "newState": c.newState, "burnRoot": c.burnRoot, "burnCount": c.burnCount, "tx": rc.transaction_hash, "gasUsed": rc.gas_used, "via": via.to_json() }),
                )
            }
            Job::GasDrop { receipt } => {
                let (Some((zone, ctx)), Some(image_id), Some(me)) =
                    (self.zone.as_mut(), self.gasdrop_image_id, self.lez_me)
                else {
                    bail!(
                        "refused: this relayer runs no gas drops (no --sequencer, --gasdrop-image-id or --lez-account)"
                    );
                };
                let history = lez::history_of(&*zone, ctx).await?;
                let list = eth::deposit_list(&self.p, self.r.vault, self.from_block, 10_000).await?;
                let roots = relayer::imported_roots(&list.deposits, &history.roots)?;
                let mut chain_id = [0u8; 32];
                chain_id[24..].copy_from_slice(&ctx.chain_id.to_be_bytes());
                let dctx = DropContext { image_id, me, chain_id, vault: ctx.vault, imported_roots: &roots };
                let book_path = self.state.join("drops.json");
                let mut book = DropBook::load(&book_path)?;
                let ctx2 = ctx.clone();
                let claimed = |n: &[u8; 32]| lez::is_claimed(&ctx2, &history, n);
                let (j, tx) = relayer::gas_drop(
                    &serde_json::to_vec(&receipt)?,
                    &dctx,
                    claimed,
                    &mut book,
                    &book_path,
                    zone,
                    self.drop_amount,
                    v.now,
                )
                .await?;
                Ok(
                    json!({ "kind": "gas-drop", "nullifier": hx(&j.nullifier), "oneTimeAddress": hx(&j.one_time_address), "amount": self.drop_amount.to_string(), "tx": tx }),
                )
            }
        }
    }
}

/// The sequencer zone: LEZ's wallet with the relayer's seed.
#[cfg(feature = "lez-wallet")]
fn sequencer(cli: &Cli, rpc: &str) -> Result<Zone> {
    let need = |o: &Option<PathBuf>, f: &str| o.clone().ok_or_else(|| anyhow!("{f} is required with --sequencer"));
    let seed = crate::seed::Seed::from_file(&need(&cli.seed, "--seed")?)?;
    let wallet = lez::open_wallet(
        &need(&cli.lez_wallet, "--lez-wallet")?,
        rpc,
        &need(&cli.lez_password_file, "--lez-password-file")?,
        &seed,
    )?;
    let payer = cli.lez_account.as_deref().map(account).transpose()?;
    Ok(Zone::Sequencer(lez::Sequencer { wallet, payer, bridge_program: None, synced: false }))
}

#[cfg(not(feature = "lez-wallet"))]
fn sequencer(_cli: &Cli, _rpc: &str) -> Result<Zone> {
    bail!("this build has no LEZ wallet (feature `lez-wallet`): --sequencer is not available here, --lez-devnet is")
}

/// The daemon: `ferrata-relayer ARGS` until its rounds are done, or until `stop` holds a reason (the
/// binary's SIGTERM or SIGINT, the module's `stop`), after the round in progress. Every line goes to `sink`.
pub async fn run(cli: Cli, sink: Sink, mut stop: tokio::sync::watch::Receiver<Option<String>>) -> Result<()> {
    let url = cli.eth_rpc.clone().ok_or_else(|| anyhow!("--eth-rpc is required"))?;
    let vault = cli.vault.ok_or_else(|| anyhow!("--vault is required"))?;
    let logos = cli.send_via == "logos";
    let (p, me) = match (&cli.keystore, &cli.password_file, cli.unlocked) {
        (None, None, None) if logos => {
            let me = cli.from.ok_or_else(|| anyhow!("--send-via logos needs --from (the keystore account)"))?;
            (eth::provider(&url)?, me)
        }
        (Some(ks), Some(pw), None) => {
            let signer = eth::keystore_signer(ks, pw)?;
            let me = signer.address();
            (eth::signing_provider(&url, signer)?, me)
        }
        (None, None, Some(me)) => {
            let p = eth::provider(&url)?;
            let accounts = p.get_accounts().await.unwrap_or_default();
            let chain = p.get_chain_id().await?;
            // anvil answers anvil_nodeInfo; a public node does not, and must never be asked to sign.
            let anvil = p.raw_request::<_, serde_json::Value>("anvil_nodeInfo".into(), Vec::<u64>::new()).await.is_ok();
            if !anvil {
                bail!("--unlocked needs a local anvil (chain {chain}; {} unlocked accounts)", accounts.len());
            }
            (p, me)
        }
        _ => bail!("give --keystore and --password-file, or --unlocked on a local anvil, or --send-via logos --from"),
    };
    let r = Relayer { vault, me, accept_unpaid: cli.accept_unpaid };
    let v = r.view(&p).await?;
    relayer::require_chain(&v, cli.chain_id)?;
    let logos = if logos {
        let call = crate::logos_send::installed()
            .ok_or_else(|| anyhow!("--send-via logos needs Logos Core: run the relayer as ferrata_relayer_module"))?;
        if let Some(f) = cli.from {
            anyhow::ensure!(f == me, "--from {f} is not the relayer key's address {me}");
        }
        Some(crate::logos_send::Route {
            call,
            chain_id: v.chain_id,
            from: me,
            purpose: "Ferrata relayer",
            fallback: cli.keystore.is_some() || cli.unlocked.is_some(),
            approval_timeout: Duration::from_secs(cli.approval_timeout),
        })
    } else {
        None
    };
    std::fs::create_dir_all(&cli.state)?;

    let zone = match (&cli.sequencer, &cli.lez_devnet) {
        (None, None) => None,
        (Some(_), Some(_)) => bail!("give --sequencer or --lez-devnet, not both"),
        (seq, devnet) => {
            let zone = match (seq, devnet) {
                (Some(rpc), _) => sequencer(&cli, rpc)?,
                (_, Some(url)) => Zone::Devnet(lez::DevnetZone::new(url)?),
                (None, None) => unreachable!(),
            };
            let program =
                account(cli.bridge_account.as_deref().ok_or_else(|| anyhow!("--bridge-account is required"))?)?;
            let ctx = Context { program, cid: v.cid.0, chain_id: v.chain_id, vault: vault.into_array() };
            Some((zone, ctx))
        }
    };
    if cli.import_roots && zone.is_none() {
        bail!("--import-roots needs a zone: --sequencer, or --lez-devnet for the devnet");
    }
    let p_mode = p.clone();
    let mut d = Daemon {
        p,
        r,
        state: cli.state.clone(),
        from_block: cli.from_block,
        zone,
        gasdrop_image_id: cli.gasdrop_image_id.as_deref().map(h32).transpose()?,
        lez_me: cli.lez_account.as_deref().map(h32).transpose()?,
        drop_amount: cli.drop_amount,
        tokens: [Address::ZERO].into_iter().collect(),
        rule_b: eth::view(&p_mode, vault, IRelayVault::modeCall {}, None).await? == 1,
        max_job_age: cli.max_job_age,
        wrap_command: cli.wrap_command.clone(),
        import_roots: cli.import_roots,
        import_every: cli.import_every,
        import_done: std::fs::read(cli.state.join("root-import.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .filter(|v| v["every"].as_u64() == Some(cli.import_every))
            .and_then(|v| v["period"].as_u64()),
        logos,
        sink: sink.clone(),
    };
    let zone_kind = d.zone.as_ref().map(|(z, _)| z.kind());
    log(
        &sink,
        "start",
        json!({ "vault": vault, "relayer": me, "chainId": v.chain_id, "configId": v.cid, "finalState": v.final_state,
                "gasDrops": zone_kind == Some("sequencer"), "zone": zone_kind, "importRoots": cli.import_roots,
                "importEvery": cli.import_every,
                "sendVia": d.logos.as_ref().map_or_else(|| json!("plain"), |r| json!({ "path": "logos",
                    "module": crate::logos_send::TX_SENDER, "from": r.from, "fallback": r.fallback })) }),
    );

    match cli.command {
        Command::Status => {
            let candidates = d.r.candidates(&d.p, d.from_block).await?;
            let balance = d.p.get_balance(me).await?;
            let owed = eth::view(&d.p, vault, IRelayVault::owedCall { payee: me, token: Address::ZERO }, None).await?;
            let book = DropBook::load(&d.state.join("drops.json"))?;
            let finalized = eth::view(&d.p, vault, IVault::checkpointCountCall {}, None).await?;
            log(
                &sink,
                "status",
                json!({
                    "finalState": v.final_state, "checkpoints": finalized.to_string(),
                    "candidates": candidates.iter().map(|(c, at)| json!({ "newState": c.newState, "readyAt": at })).collect::<Vec<_>>(),
                    "balanceWei": balance.to_string(), "owedWei": owed.to_string(), "drops": book.drops.len(),
                }),
            );
        }
        Command::Run { inbox, poll_secs, rounds } => {
            std::fs::create_dir_all(&inbox)?;
            let stop_reason = |s: &tokio::sync::watch::Receiver<Option<String>>| s.borrow().clone();
            let mut n = 0u64;
            let mut reason = "rounds".to_string();
            loop {
                if let Err(e) = d.round(&inbox).await {
                    log(&sink, "round-failed", json!({ "reason": format!("{e:#}") }));
                }
                n += 1;
                if rounds != 0 && n >= rounds {
                    break;
                }
                // A stop asked for during the round ends the daemon now.
                if let Some(r) = stop_reason(&stop) {
                    reason = r;
                    break;
                }
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(poll_secs)) => {}
                    changed = stop.changed() => {
                        if changed.is_err() {
                            // Nobody can ask for a stop any more: the poll goes on.
                            tokio::time::sleep(Duration::from_secs(poll_secs)).await;
                        }
                    }
                }
                if let Some(r) = stop_reason(&stop) {
                    reason = r;
                    break;
                }
            }
            log(&sink, "stop", json!({ "rounds": n, "reason": reason }));
        }
    }
    Ok(())
}
