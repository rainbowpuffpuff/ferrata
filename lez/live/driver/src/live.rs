//! The connection to the sequencer: LEZ's wallet (which builds, signs and sends every
//! transaction), the follower that reads the result of each, and the manifest.

use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{bail, ensure, Result};
use common::{block::BedrockStatus, transaction::LeeTransaction, HashType};
use lee::{AccountId, Actor, PrivateKey};
use sequencer_service_rpc::SequencerClient;
use wallet::{AccountIdentity, AccountMention, WalletCore};

use crate::{
    blocklog,
    follower::{Follower, Receipt},
    manifest::{self, HarnessTables, Row},
    plan::{exit_name, Pda},
};

/// How many blocks after its submission a transaction may still appear in; one that has not is
/// recorded as dropped by the sequencer.
pub const DROP_AFTER_BLOCKS: u64 = 4;

/// `DROP_AFTER_BLOCKS`, or `FERRATA_DROP_AFTER_BLOCKS` from the environment: on a zone of several sequencers a
/// transaction waits for the turn of a sequencer that holds it (checkpoint/attestor/zone-bridge.sh sets 40).
pub fn drop_after_blocks() -> u64 {
    std::env::var("FERRATA_DROP_AFTER_BLOCKS").ok().and_then(|v| v.parse().ok()).unwrap_or(DROP_AFTER_BLOCKS)
}

/// How long a transaction that was seen in a block may take to be final on Bedrock. The
/// sequencer gives up on an inscription Bedrock does not mine after 300 slots (its
/// `stale_refund_slots`), rewinds its head and includes the transactions again.
pub const SETTLE_TIMEOUT: Duration = Duration::from_secs(3000); // two stalls of rc3's 0.3.5-rc.1 node (1,215 s each) fit

pub struct Options {
    pub rpc: String,
    pub run_dir: PathBuf,
    pub out_dir: PathBuf,
    pub harness_results: PathBuf,
    pub sequencer_log: Option<PathBuf>,
    /// Gas limit declared for the program's job steps.
    pub gas_limit: u64,
    /// Follow only blocks that are final on Bedrock, and treat a transaction as settled only
    /// then. Without a Bedrock (LEZ's mock) no block ever becomes final: follow the head.
    pub wait_final: bool,
}

pub struct Live {
    pub wallet: WalletCore<SequencerClient>,
    pub follower: Follower,
    pub rows: Vec<Row>,
    pub harness: HarnessTables,
    pub opts: Options,
    pub phase: String,
}

/// The URL guards' parser (standard library only; the updater's `chain.rs` includes it too): the host a URL names,
/// a userinfo refused.
#[path = "rpcurl.rs"]
pub mod rpcurl;

/// Only a sequencer on this machine: the wallet's own default is LEZ's public testnet. The one exception is the
/// day-one deployment (`lez/testnet04/day1.sh`): another sequencer when `FERRATA_SEQUENCER_REMOTE` names exactly that
/// URL (the script sets it to the URL it was given). Reading it is then allowed; sending to it needs
/// `FERRATA_TESTNET_SEND=yes` too ([`check_send`], at every place a transaction leaves: `send`, `send_mentions`,
/// `send_raw` here, the updater's `SequencerLedger::submit`, `ferrata-day1`'s `send` and `fund`, and at the drivers'
/// `open`). "On this machine" is decided on the URL's host read strictly ([`rpcurl`]: a userinfo is refused, so the
/// host is the one the client connects to); a loopback port forwarded elsewhere is beyond any URL check.
pub fn check_loopback(rpc: &str) -> Result<()> {
    if rpcurl::is_loopback(rpc) || remote_named(rpc) {
        return Ok(());
    }
    match rpcurl::parse(rpc) {
        Err(why) => bail!("refusing {why}"),
        Ok(u) => bail!(
            "refusing to talk to {}: only a local sequencer (another needs FERRATA_SEQUENCER_REMOTE={rpc})",
            u.host
        ),
    }
}

/// `rpc` is the sequencer the day-one deployment was given: `FERRATA_SEQUENCER_REMOTE` is exactly this URL, and the URL
/// reads strictly (no userinfo).
pub fn remote_named(rpc: &str) -> bool {
    std::env::var("FERRATA_SEQUENCER_REMOTE").is_ok_and(|named| !named.is_empty() && named == rpc)
        && rpcurl::parse(rpc).is_ok()
}

/// Before a transaction is sent: a sequencer on this machine, or the named one with `FERRATA_TESTNET_SEND=yes` (the
/// owner's decision; nothing here sets it).
pub fn check_send(rpc: &str) -> Result<()> {
    if rpcurl::is_loopback(rpc) {
        return Ok(());
    }
    ensure!(
        remote_named(rpc) && std::env::var("FERRATA_TESTNET_SEND").as_deref() == Ok("yes"),
        "refusing to send to {rpc}: a sequencer not on this machine needs FERRATA_TESTNET_SEND=yes and FERRATA_SEQUENCER_REMOTE={rpc} (and no userinfo in the URL)"
    );
    Ok(())
}

fn wallet_config_json(rpc: &str, gas_limit: u64) -> String {
    format!(
        "{{\n  \"sequencer\": {{ \"sequencer_addr\": \"{rpc}\" }},\n  \"seq_poll_timeout\": \"1s\",\n  \
         \"seq_tx_poll_max_blocks\": 120,\n  \"seq_poll_max_retries\": 2,\n  \"seq_block_poll_max_amount\": 100,\n  \
         \"gas_limit\": {gas_limit}\n}}\n"
    )
}

/// A wallet home under the run directory, created on first use with the given public keys
/// imported. LEZ's wallet keeps its config and storage there.
pub fn open_wallet(home: &Path, rpc: &str, keys: &[PrivateKey]) -> Result<WalletCore<SequencerClient>> {
    check_loopback(rpc)?;
    fs::create_dir_all(home)?;
    let config_path = home.join("wallet_config.json");
    let storage_path = home.join("storage.json");
    // Always written: LEZ's wallet would otherwise create a default config pointing at its
    // public testnet.
    fs::write(&config_path, wallet_config_json(rpc, wallet::DEFAULT_GAS_LIMIT))?;
    let mut wallet = if storage_path.exists() {
        WalletCore::new_update_chain(config_path, storage_path, None, "")?
    } else {
        let (wallet, _mnemonic) = WalletCore::new_init_storage(config_path, storage_path, None, "")?;
        wallet
    };
    for key in keys {
        wallet.storage_mut().key_chain_mut().add_imported_public_account(key.clone());
    }
    wallet.store_persistent_data()?;
    // From LEZ v0.4.0-rc2 the wallet simulates a public send and does not send one that reverts
    // (#1035); the scenario sends the program's and LEZ's refusals on purpose.
    #[cfg(feature = "lez-rc2")]
    wallet.set_allow_revert(true);
    Ok(wallet)
}

impl Live {
    pub async fn open(opts: Options, keys: &[PrivateKey]) -> Result<Self> {
        fs::create_dir_all(&opts.run_dir)?;
        let wallet = open_wallet(&opts.run_dir.join("wallet"), &opts.rpc, keys)?;
        let blocks_path = opts.run_dir.join("blocks.borsh");
        let state_path = opts.run_dir.join("follower.borsh");
        let follower = if blocks_path.exists() && state_path.exists() {
            Follower::restore(&fs::read(&state_path)?, blocklog::load(&blocks_path)?)?
        } else {
            Follower::new()
        };
        let rows_path = opts.run_dir.join("run.tsv");
        let rows = if rows_path.exists() { manifest::read(&rows_path)? } else { Vec::new() };
        let harness = HarnessTables::load(&opts.harness_results)?;
        let mut live = Self { wallet, follower, rows, harness, opts, phase: String::new() };
        live.sync().await?;
        Ok(live)
    }

    /// Save the follower, the unabridged blocks and the manifest, to resume in another process.
    pub fn save(&self) -> Result<()> {
        blocklog::save(&self.opts.run_dir.join("blocks.borsh"), &self.follower.blocks)?;
        fs::write(self.opts.run_dir.join("follower.borsh"), self.follower.snapshot()?)?;
        manifest::write(&self.opts.run_dir.join("run.tsv"), &self.rows)?;
        Ok(())
    }

    fn save_rows(&self) -> Result<()> {
        manifest::write(&self.opts.run_dir.join("run.tsv"), &self.rows)
    }

    pub async fn head(&self) -> Result<u64> {
        self.wallet.get_last_block_id().await
    }

    /// Bring the follower up to the sequencer's chain: to its head, or, with a Bedrock, to the
    /// last block that is final there. Blocks of the head that are not final yet can still be
    /// replaced (the sequencer rewinds its head when an inscription is not mined), so the
    /// follower never applies one.
    pub async fn sync(&mut self) -> Result<Vec<Receipt>> {
        let head = self.head().await?;
        let mut receipts = Vec::new();
        while self.follower.height() < head {
            let id = self.follower.height() + 1;
            // None: the head was rewound below `id` after `head` was read (on LEZ v0.4.0-rc3 with Logos
            // Blockchain 0.3.5-rc.1 the sequencer orphans unmined inscriptions and rewinds to the last final
            // block); the follower waits for the chain to come back.
            let Some(block) = self.wallet.get_block(id).await? else { break };
            if self.opts.wait_final && !matches!(block.bedrock_status, BedrockStatus::Finalized) {
                break;
            }
            receipts.extend(self.follower.apply(block)?);
        }
        Ok(receipts)
    }

    pub fn set_gas_limit(&mut self, gas_limit: u64) {
        let mut config = self.wallet.config().clone();
        config.gas_limit = gas_limit;
        self.wallet.set_config(config);
        // From LEZ v0.4.0-rc2 the wallet declares the gas it measured plus a margin unless the
        // limit is pinned (#1035); the scenario declares its own limits, as on v0.4.0-rc1.
        #[cfg(feature = "lez-rc2")]
        self.wallet.set_fee_params(wallet::FeeParams { gas_limit: Some(gas_limit), ..wallet::FeeParams::default() });
    }

    /// The state of `actor` as the sequencer serves it, checked against the follower's.
    pub async fn actor_state(&self, actor: Actor) -> Result<Vec<u8>> {
        let served = self
            .wallet
            .get_account_view(actor)
            .await?
            .map(|account| account.data.actor_state(actor.program_account_id).to_vec())
            .unwrap_or_default();
        let followed =
            self.follower.state.get_account_by_id(actor.account_id).data.actor_state(actor.program_account_id).to_vec();
        ensure!(
            served == followed,
            "the sequencer and the follower disagree on the state of {}@{}",
            actor.account_id,
            actor.program_account_id
        );
        Ok(served)
    }

    pub async fn balance(&self, account: AccountId) -> Result<u128> {
        self.wallet.get_account_balance(account).await
    }

    /// Lines of the sequencer's log that name `hash`.
    pub fn sequencer_lines(&self, hash: &str) -> Vec<String> {
        let Some(path) = &self.opts.sequencer_log else { return Vec::new() };
        let Ok(text) = fs::read_to_string(path) else { return Vec::new() };
        text.lines().filter(|l| l.contains(hash)).map(str::to_string).collect()
    }

    /// Wait until `hash` is settled: in a block the follower has applied (a block final on
    /// Bedrock, if there is one). `None` if the sequencer produced `drop_after_blocks()` blocks
    /// without ever including it. Also returns the head blocks it was seen in on the way: more
    /// than one means the sequencer rewound its head and included it again.
    pub async fn wait_settled(&mut self, hash: HashType, since_head: u64) -> Result<(Option<u64>, Vec<u64>)> {
        let started = std::time::Instant::now();
        let mut seen: Vec<u64> = Vec::new();
        loop {
            let found = self.wallet.get_transaction(hash).await?.map(|(_, block_id)| block_id);
            if let Some(block_id) = found {
                if seen.last() != Some(&block_id) {
                    if !seen.is_empty() {
                        println!("[{}] {hash} moved from block {} to block {block_id}: the head was rewound", self.phase, seen[seen.len() - 1]);
                    }
                    seen.push(block_id);
                }
                self.sync().await?;
                if self.follower.receipt(&hash).is_some() {
                    return Ok((Some(block_id), seen));
                }
            } else if seen.is_empty() && self.head().await? >= since_head + drop_after_blocks() {
                if self.wallet.get_transaction(hash).await?.is_none() {
                    return Ok((None, seen));
                }
            }
            ensure!(
                started.elapsed() < SETTLE_TIMEOUT,
                "{hash} was seen in block(s) {seen:?} but is not final after {} s",
                SETTLE_TIMEOUT.as_secs()
            );
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
    }

    fn harness_columns(&self, row: &mut Row, harness: &Option<(&'static str, String)>) {
        if let Some((table, label)) = harness {
            match self.harness.get(table, label) {
                Some(h) => {
                    row.harness_exec_gas = h.exec_gas.to_string();
                    row.harness_stor_gas = h.stor_gas.to_string();
                    row.harness_result = h.result.clone();
                }
                None => row.harness_result = format!("(no row \"{label}\" in {table}.md)"),
            }
        }
    }

    /// Record a transaction that was sent: wait for it, read its receipt from the follower.
    pub async fn settle(&mut self, sent: Sent) -> Result<Outcome> {
        let Sent { step, hash, since_head, payer, balance_before, harness, note, sent_at } = sent;
        let mut row = Row { phase: self.phase.clone(), step: step.clone(), tx_hash: hash.to_string(), note, ..Row::default() };
        self.harness_columns(&mut row, &harness);
        let (included, seen) = self.wait_settled(hash, since_head).await?;
        if seen.len() > 1 {
            let first = seen[..seen.len() - 1].iter().map(u64::to_string).collect::<Vec<_>>().join(", ");
            row.note = format!("first included in block {first} of a head the sequencer rewound; {}", row.note);
        }
        let receipt = self.follower.receipt(&hash).cloned();
        match (&included, &receipt) {
            (Some(block_id), Some(receipt)) => {
                ensure!(*block_id == receipt.block_id, "{step}: the sequencer and the follower disagree on the block");
                row = row.with_receipt(receipt, exit_name);
                if let Some(payer) = payer {
                    let after = self.balance(payer).await?;
                    row.payer_debit = balance_before.saturating_sub(after).to_string();
                }
            }
            (None, None) => {
                row.status = "dropped".to_string();
                let lines = self.sequencer_lines(&hash.to_string());
                let reason = lines.iter().map(|l| log_message(l)).collect::<Vec<_>>().join(" | ");
                row.note = if reason.is_empty() {
                    format!("not included within {} blocks; {}", drop_after_blocks(), row.note)
                } else {
                    format!("sequencer: {reason}; {}", row.note)
                };
            }
            _ => bail!("{step}: the sequencer and the follower disagree on whether {hash} is included"),
        }
        println!(
            "[{}] {:<58} {:<9} {:>4} gas {:>9} block {:>4} +{:>5.1}s {}",
            self.phase,
            row.step,
            row.status,
            row.exit,
            row.exec_gas,
            row.block,
            sent_at.elapsed().as_secs_f64(),
            row.tx_hash
        );
        self.rows.push(row.clone());
        self.save_rows()?;
        Ok(Outcome { row, receipt })
    }

    /// Send one public transaction through LEZ's wallet: `actors` are declared in order, the
    /// message goes to `actors[root]`, `payer` pays (co-signing if it is not an actor).
    pub async fn send(&mut self, tx: Tx) -> Result<Result<Sent, Outcome>> {
        check_send(&self.opts.rpc)?;
        let mentions: Vec<AccountMention> = tx.actors.iter().map(|pda| mention(*pda)).collect();
        self.send_mentions(tx, mentions).await
    }

    /// `tx` with these account mentions instead of its PDAs (for actors that are not PDAs).
    pub async fn send_mentions(&mut self, tx: Tx, mentions: Vec<AccountMention>) -> Result<Result<Sent, Outcome>> {
        check_send(&self.opts.rpc)?;
        self.sync().await?;
        let gas_limit = tx.gas_limit.unwrap_or(self.opts.gas_limit);
        self.set_gas_limit(gas_limit);
        let balance_before = self.balance(tx.payer).await?;
        let since_head = self.head().await?;
        match self.wallet.send_pub_tx_paid_by(mentions, tx.root, tx.message, Some(tx.payer)).await {
            Ok(hash) => Ok(Ok(Sent {
                step: tx.step,
                hash,
                sent_at: std::time::Instant::now(),
                since_head,
                payer: Some(tx.payer),
                balance_before,
                harness: tx.harness,
                note: tx.note,
            })),
            Err(error) => {
                let mut row = Row {
                    phase: self.phase.clone(),
                    step: tx.step,
                    status: "rejected".to_string(),
                    gas_limit: gas_limit.to_string(),
                    note: format!("{error}: {error:?}"),
                    ..Row::default()
                };
                self.harness_columns(&mut row, &tx.harness);
                println!("[{}] {:<58} rejected: {}", self.phase, row.step, row.note);
                self.rows.push(row.clone());
                self.save_rows()?;
                Ok(Err(Outcome { row, receipt: None }))
            }
        }
    }

    /// Send with explicit mentions and wait.
    pub async fn submit_mentions(&mut self, tx: Tx, mentions: Vec<AccountMention>) -> Result<Outcome> {
        match self.send_mentions(tx, mentions).await? {
            Ok(sent) => self.settle(sent).await,
            Err(outcome) => Ok(outcome),
        }
    }

    /// Send and wait.
    pub async fn submit(&mut self, tx: Tx) -> Result<Outcome> {
        match self.send(tx).await? {
            Ok(sent) => self.settle(sent).await,
            Err(outcome) => Ok(outcome),
        }
    }

    /// Send an already built transaction (not through the wallet's builders).
    pub async fn send_raw(&mut self, step: &str, tx: LeeTransaction, note: String) -> Result<Outcome> {
        use wallet::WalletSequencerClient as _;
        check_send(&self.opts.rpc)?;
        self.sync().await?;
        let since_head = self.head().await?;
        let hash = tx.hash();
        match self.wallet.client().send_transaction(tx).await {
            Ok(returned) => {
                ensure!(returned == hash, "the sequencer returned another hash");
                self.settle(Sent {
                    step: step.to_string(),
                    hash,
                    sent_at: std::time::Instant::now(),
                    since_head,
                    payer: None,
                    balance_before: 0,
                    harness: None,
                    note,
                })
                .await
            }
            Err(error) => {
                let row = Row {
                    phase: self.phase.clone(),
                    step: step.to_string(),
                    tx_hash: hash.to_string(),
                    status: "rejected".to_string(),
                    note: format!("sequencer RPC: {error}; {note}"),
                    ..Row::default()
                };
                println!("[{}] {:<58} rejected: {}", self.phase, row.step, row.note);
                self.rows.push(row.clone());
                self.save_rows()?;
                Ok(Outcome { row, receipt: None })
            }
        }
    }
}

/// The message part of a log line (after the logger's `[time level target]` prefix).
pub fn log_message(line: &str) -> String {
    match line.find("] ") {
        Some(i) => line[i + 2..].trim().to_string(),
        None => line.trim().to_string(),
    }
}

pub fn mention(pda: Pda) -> AccountMention {
    AccountIdentity::PublicPda { program: pda.program, seed: pda.seed }.select_program_actor_state(pda.program)
}

/// A public transaction to send.
pub struct Tx {
    pub step: String,
    /// (harness table, row label) of the same step in `lez/results/`.
    pub harness: Option<(&'static str, String)>,
    pub actors: Vec<Pda>,
    pub root: usize,
    pub message: Vec<u8>,
    pub payer: AccountId,
    /// `None`: the run's gas limit for job steps.
    pub gas_limit: Option<u64>,
    pub note: String,
}

/// A transaction the sequencer's RPC accepted.
pub struct Sent {
    pub step: String,
    pub hash: HashType,
    /// When the sequencer's RPC accepted it (the row's `+s` is the time to its inclusion and
    /// to the follower's reading of it).
    pub sent_at: std::time::Instant,
    pub since_head: u64,
    pub payer: Option<AccountId>,
    pub balance_before: u128,
    pub harness: Option<(&'static str, String)>,
    pub note: String,
}

pub struct Outcome {
    pub row: Row,
    pub receipt: Option<Receipt>,
}

impl Outcome {
    pub fn applied(&self) -> bool {
        self.row.status == "applied"
    }

    pub fn exit_code(&self) -> Option<u32> {
        self.receipt.as_ref().and_then(|r| r.exit_code)
    }

    pub fn expect_applied(self) -> Result<Self> {
        ensure!(self.applied(), "{}: expected to apply, got {} {} {}", self.row.step, self.row.status, self.row.exit, self.row.note);
        Ok(self)
    }

    pub fn expect_exit(self, code: u8) -> Result<Self> {
        ensure!(
            self.exit_code() == Some(u32::from(code)),
            "{}: expected exit {code}, got {} {} {}",
            self.row.step,
            self.row.status,
            self.row.exit,
            self.row.note
        );
        Ok(self)
    }
}
