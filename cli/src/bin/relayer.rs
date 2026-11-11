//! `ferrata-relayer`: the relayer daemon for the Ferrata bridge (prototype, unaudited).
//!
//! One loop, every `--poll-secs`:
//! 1. **Inbox.** Each `<inbox>/*.json` job ([`relayer::Job`]) handed over by a user who opts in, or by
//!    the checkpoint prover: a release (journal and seal), a rule (B) checkpoint (journal and seal) or
//!    a gas drop (receipt). Checked ([`relayer`]), then sent; the file moves to `<state>/done/` or
//!    `<state>/refused/`, and one JSON line says what happened and why.
//! 2. **Candidates on the vault.** Every live candidate (anyone's, rebuilt from its event and checked
//!    against the digest the vault stored) whose wait is over is finalised with `finalizeAttested`.
//! 3. **Fees.** Whatever the vault credited this relayer (checkpoint fees; relay fees whose push
//!    failed) is withdrawn with `claimOwed`.
//! 4. **Burns on LEZ** (with `--sequencer`): the configuration's burns past the newest accepted burn
//!    count; if there are any and no live candidate, `<state>/needs-checkpoint.json` tells the
//!    checkpoint prover (a separate process: `e2e/crown`, stage 7) to cut one.
//!
//! Keys: an Ethereum keystore file (`--keystore`, `--password-file`), or, on a local anvil only,
//! `--unlocked` (the node signs). The LEZ side (gas drops, burns) uses LEZ's wallet with the
//! relayer's own seed. The relayer holds no user secret: see `cli/src/relayer.rs`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use alloy::primitives::{Address, Bytes};
use alloy::providers::{DynProvider, Provider};
use alloy::sol_types::SolCall;
use anyhow::{anyhow, bail, Context as _, Result};
use clap::{Parser, Subcommand};
use ferrata_cli::eth::{self, IVault};
use ferrata_cli::lez::{self, Context, Sequencer};
use ferrata_cli::relayer::{self, DropBook, DropContext, IRelayVault, Job, Relayer};
use ferrata_cli::seed::Seed;
use ferrata_cli::{account, h32, hx};
use serde_json::json;
use sha2::Digest as _;

#[derive(Parser)]
#[command(name = "ferrata-relayer", version, about = "The Ferrata relayer daemon (prototype, unaudited)")]
struct Cli {
    /// Any Ethereum JSON-RPC endpoint.
    #[arg(long, env = "FERRATA_ETH_RPC", global = true)]
    eth_rpc: Option<String>,
    /// The vault's address.
    #[arg(long, env = "FERRATA_VAULT", global = true)]
    vault: Option<Address>,
    /// Refuse to run unless the endpoint serves this chain id.
    #[arg(long, env = "FERRATA_CHAIN_ID", global = true)]
    chain_id: Option<u64>,
    /// The relayer's state directory (drop book, candidates, done and refused jobs).
    #[arg(long, env = "FERRATA_RELAYER_STATE", default_value = "relayer-state", global = true)]
    state: PathBuf,
    /// An Ethereum keystore file (Web3 Secret Storage) for the relayer's account.
    #[arg(long, env = "FERRATA_KEYSTORE", global = true)]
    keystore: Option<PathBuf>,
    /// A file holding the keystore's password.
    #[arg(long, env = "FERRATA_PASSWORD_FILE", global = true)]
    password_file: Option<PathBuf>,
    /// Local anvil only: send from this address and let the node sign (no key here).
    #[arg(long, global = true)]
    unlocked: Option<Address>,
    /// Submit releases whose proof pays no relay fee.
    #[arg(long, global = true)]
    accept_unpaid: bool,
    /// First block of the vault's events (its deployment block).
    #[arg(long, env = "FERRATA_FROM_BLOCK", default_value_t = 0, global = true)]
    from_block: u64,
    /// The LEZ sequencer's RPC endpoint (gas drops and the burn watch).
    #[arg(long, env = "FERRATA_SEQUENCER", global = true)]
    sequencer: Option<String>,
    /// The relayer's own seed file (its LEZ wallet's mnemonic).
    #[arg(long, env = "FERRATA_SEED", global = true)]
    seed: Option<PathBuf>,
    /// The LEZ wallet's home.
    #[arg(long, env = "FERRATA_LEZ_WALLET", global = true)]
    lez_wallet: Option<PathBuf>,
    /// A file holding the LEZ wallet's password.
    #[arg(long, env = "FERRATA_LEZ_PASSWORD_FILE", global = true)]
    lez_password_file: Option<PathBuf>,
    /// The bridge program's account id on LEZ (32-byte hex).
    #[arg(long, env = "FERRATA_BRIDGE_ACCOUNT", global = true)]
    bridge_account: Option<String>,
    /// The relayer's LEZ public account (32-byte hex): pays gas drops, named in gas-drop receipts.
    #[arg(long, env = "FERRATA_RELAYER_LEZ_ACCOUNT", global = true)]
    lez_account: Option<String>,
    /// The gas-drop guest's image id (32-byte hex).
    #[arg(long, env = "FERRATA_GASDROP_IMAGE_ID", global = true)]
    gasdrop_image_id: Option<String>,
    /// Native LEZ sent per gas drop.
    #[arg(long, default_value_t = 0, global = true)]
    drop_amount: u128,
    /// The Groth16 prover, run as `<command> <input.json> <proof.json>` (RISC Zero's prover image: Docker on
    /// x86, or a remote prover); with it the relayer wraps holders' succinct release receipts.
    #[arg(long, env = "FERRATA_WRAP_COMMAND", global = true)]
    wrap_command: Option<PathBuf>,
    /// Seconds a job may wait in the inbox for a temporary condition (root not final, delay, freeze, cap).
    #[arg(long, default_value_t = 7 * 24 * 3600, global = true)]
    max_job_age: u64,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
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

fn log(event: &str, fields: serde_json::Value) {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let mut line = json!({ "t": t, "event": event });
    if let (Some(obj), Some(extra)) = (line.as_object_mut(), fields.as_object()) {
        obj.extend(extra.clone());
    }
    println!("{line}");
}

struct Daemon {
    p: DynProvider,
    r: Relayer,
    state: PathBuf,
    from_block: u64,
    zone: Option<(Sequencer, Context)>,
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
}

impl Daemon {
    async fn round(&mut self, inbox: &Path) -> Result<()> {
        let v = self.r.view(&self.p).await?;
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
                    log("job-deferred", json!({ "job": name, "reason": format!("{e:#}"), "ageSecs": age }));
                    continue;
                }
                Err(_) => "refused",
            };
            std::fs::create_dir_all(self.state.join(dir))?;
            std::fs::rename(&file, self.state.join(dir).join(&name))?;
            match outcome {
                Ok(fields) => log("job-done", json!({ "job": name, "result": fields })),
                Err(e) => log("job-refused", json!({ "job": name, "reason": format!("{e:#}") })),
            }
        }
        // 2. Candidates whose wait is over.
        let v = self.r.view(&self.p).await?;
        for (c, ready_at) in self.r.candidates(&self.p, self.from_block).await? {
            if !self.rule_b {
                // Rule (A) finalises with a second proof echoing a beacon: the checkpoint prover's job.
                log(
                    "candidate-not-finalized",
                    json!({ "newState": c.newState, "reason": "rule (A) vault: finalize needs a beacon proof" }),
                );
                continue;
            }
            if v.now < ready_at {
                log("candidate-waiting", json!({ "newState": c.newState, "readyAt": ready_at, "now": v.now }));
                continue;
            }
            let input: Bytes = IRelayVault::finalizeAttestedCall { c: c.clone() }.abi_encode().into();
            match self.r.submit(&self.p, input).await {
                Ok(rc) => log(
                    "finalized",
                    json!({ "newState": c.newState, "burnRoot": c.burnRoot, "burnCount": c.burnCount, "tx": rc.transaction_hash, "gasUsed": rc.gas_used }),
                ),
                Err(e) => log("finalize-refused", json!({ "newState": c.newState, "reason": format!("{e:#}") })),
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
            match self.r.submit(&self.p, input).await {
                Ok(rc) => log(
                    "fees-collected",
                    json!({ "token": token, "amount": owed.to_string(), "tx": rc.transaction_hash }),
                ),
                Err(e) => log("fees-refused", json!({ "token": token, "reason": format!("{e:#}") })),
            }
        }
        // 4. Burns on LEZ past the newest accepted burn count.
        if let Some((seq, ctx)) = &self.zone {
            let history = lez::history_of(seq, ctx).await?;
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
                log("checkpoint-needed", json!({ "burns": burns, "accepted": covered }));
            } else if path.exists() {
                std::fs::remove_file(&path)?;
            }
        }
        Ok(())
    }

    async fn job(&mut self, file: &Path, v: &relayer::VaultView) -> Result<serde_json::Value> {
        let job: Job = serde_json::from_slice(&std::fs::read(file)?).context("not a relayer job")?;
        match job {
            Job::Release { journal, seal } => {
                let (req, input) =
                    self.r.plan_release(&self.p, v, &relayer::unhex(&journal)?, relayer::unhex(&seal)?.into()).await?;
                let rc = self.r.submit(&self.p, input).await?;
                self.tokens.insert(req.token);
                let paid = relayer::relay_fee_paid(&rc, self.r.me);
                Ok(json!({
                    "kind": "release", "nullifier": req.nullifier, "recipient": req.recipient, "token": req.token,
                    "relayFee": req.relayFee.to_string(), "paidTo": self.r.me, "paid": paid.map(|(_, f)| f.to_string()),
                    "tx": rc.transaction_hash, "gasUsed": rc.gas_used,
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
                let rc = self.r.submit(&self.p, input).await?;
                self.tokens.insert(req.token);
                let paid = relayer::relay_fee_paid(&rc, self.r.me);
                Ok(json!({
                    "kind": "release", "wrapped": true, "wrapSecs": wrap_s, "nullifier": req.nullifier,
                    "recipient": req.recipient, "token": req.token, "relayFee": req.relayFee.to_string(),
                    "paidTo": self.r.me, "paid": paid.map(|(_, f)| f.to_string()), "tx": rc.transaction_hash,
                    "gasUsed": rc.gas_used,
                }))
            }
            Job::Checkpoint { journal, seal } => {
                let journal = relayer::unhex(&journal)?;
                let input = self.r.plan_candidate(&self.p, v, &journal, relayer::unhex(&seal)?.into()).await?;
                let rc = self.r.submit(&self.p, input).await?;
                let c = relayer::checkpoint_from_journal(&journal)?.checkpoint;
                Ok(
                    json!({ "kind": "candidate", "newState": c.newState, "burnRoot": c.burnRoot, "burnCount": c.burnCount, "tx": rc.transaction_hash, "gasUsed": rc.gas_used }),
                )
            }
            Job::GasDrop { receipt } => {
                let (Some((seq, ctx)), Some(image_id), Some(me)) =
                    (self.zone.as_mut(), self.gasdrop_image_id, self.lez_me)
                else {
                    bail!(
                        "refused: this relayer runs no gas drops (no --sequencer, --gasdrop-image-id or --lez-account)"
                    );
                };
                let history = lez::history_of(&*seq, ctx).await?;
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
                    seq,
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

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let url = cli.eth_rpc.clone().ok_or_else(|| anyhow!("--eth-rpc is required"))?;
    let vault = cli.vault.ok_or_else(|| anyhow!("--vault is required"))?;
    let (p, me) = match (&cli.keystore, &cli.password_file, cli.unlocked) {
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
        _ => bail!("give --keystore and --password-file, or --unlocked on a local anvil"),
    };
    let r = Relayer { vault, me, accept_unpaid: cli.accept_unpaid };
    let v = r.view(&p).await?;
    relayer::require_chain(&v, cli.chain_id)?;
    std::fs::create_dir_all(&cli.state)?;

    let zone = match &cli.sequencer {
        Some(rpc) => {
            let need =
                |o: &Option<PathBuf>, f: &str| o.clone().ok_or_else(|| anyhow!("{f} is required with --sequencer"));
            let seed = Seed::from_file(&need(&cli.seed, "--seed")?)?;
            let wallet = lez::open_wallet(
                &need(&cli.lez_wallet, "--lez-wallet")?,
                rpc,
                &need(&cli.lez_password_file, "--lez-password-file")?,
                &seed,
            )?;
            let program =
                account(cli.bridge_account.as_deref().ok_or_else(|| anyhow!("--bridge-account is required"))?)?;
            let payer = cli.lez_account.as_deref().map(account).transpose()?;
            let ctx = Context { program, cid: v.cid.0, chain_id: v.chain_id, vault: vault.into_array() };
            Some((Sequencer { wallet, payer, bridge_program: None }, ctx))
        }
        None => None,
    };
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
    };
    log(
        "start",
        json!({ "vault": vault, "relayer": me, "chainId": v.chain_id, "configId": v.cid, "finalState": v.final_state, "gasDrops": d.zone.is_some() }),
    );

    match cli.command {
        Command::Status => {
            let candidates = d.r.candidates(&d.p, d.from_block).await?;
            let balance = d.p.get_balance(me).await?;
            let owed = eth::view(&d.p, vault, IRelayVault::owedCall { payee: me, token: Address::ZERO }, None).await?;
            let book = DropBook::load(&d.state.join("drops.json"))?;
            let finalized = eth::view(&d.p, vault, IVault::checkpointCountCall {}, None).await?;
            log(
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
            // Clean shutdown: SIGTERM or SIGINT ends the daemon after the round in progress (installed before the
            // first round, so a signal during a round waits for it to finish).
            use tokio::signal::unix::{signal, SignalKind};
            let (mut term, mut int) = (signal(SignalKind::terminate())?, signal(SignalKind::interrupt())?);
            let mut n = 0u64;
            let mut reason = "rounds";
            loop {
                if let Err(e) = d.round(&inbox).await {
                    log("round-failed", json!({ "reason": format!("{e:#}") }));
                }
                n += 1;
                if rounds != 0 && n >= rounds {
                    break;
                }
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(poll_secs)) => {}
                    _ = term.recv() => { reason = "SIGTERM"; break; }
                    _ = int.recv() => { reason = "SIGINT"; break; }
                }
            }
            log("stop", json!({ "rounds": n, "reason": reason }));
        }
    }
    Ok(())
}
