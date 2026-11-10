//! The private path on a sequencer that verifies receipts.
//!
//! 1. The stored real-proof transaction (`lez/results/stage2-private-tx.borsh`), submitted as it
//!    is: what the sequencer answers.
//! 2. The same private update (`VerifyAll` at a private account's actor, the instance applying
//!    the verified job) built for the live instance with LEZ's own wallet
//!    (`WalletCore::send_privacy_preserving_tx_with_pre_check`): once with `RISC0_DEV_MODE=1` in
//!    this process, which makes a fake receipt, and once with a real proof.
//!
//! Every RPC call the wallet makes while it builds the private update is written to a
//! transcript. With the wallet's keys (LEZ's published test account) and the program binary,
//! the transcript is all a prover needs: `private-update --replay` rebuilds the same proving
//! job from it without a sequencer and writes the proven transaction to a file.

use std::{
    borrow::Cow,
    collections::BTreeMap,
    fs,
    future::Future,
    io::Write as _,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};

use anyhow::{anyhow, bail, ensure, Context, Result};
use common::{block::Block, transaction::LeeTransaction, HashType, PageLimit};
use ferrata_core::{fork::LcFork, witness::CompactUpdate};
use ferrata_lez_wire::Message;
use lee::{
    privacy_preserving_transaction::circuit::ProgramCatalog, program::Program, Account, AccountId, Actor,
    EncryptedNote, ProgramId, Publication,
};
use lee_core::{
    account::Nonce, BlockId, Commitment, CommitmentPosition, CommitmentSetDigest, MembershipProof, ProgramImageClaim,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::value::RawValue;
use sequencer_service_rpc::{ClientError, FeeStateQuote, SequencerClient};
use wallet::{AccountIdentity, WalletCore, WalletSequencerClient};

use crate::{
    live::{check_loopback, mention, Live, Sent},
    manifest::Row,
    plan::{as_finality_update, plan_job, signer_witness, Fixtures},
    programs::{image_id_hex, GuestBinary},
    scenario::{check_record, check_store, store, world_b_after, World},
};

// ---------------------------------------------------------------------------------------------
// 1. The stored transaction
// ---------------------------------------------------------------------------------------------

fn describe_claims(tx: &LeeTransaction) -> String {
    let LeeTransaction::PrivacyPreserving(tx) = tx else { return "a public transaction".into() };
    let claims: Vec<String> = tx
        .message
        .execution
        .program_image_claims
        .iter()
        .map(|claim| match claim {
            ProgramImageClaim::Disclosed { account_id, image_id } => {
                format!("image {} at account {account_id}", image_id_hex(*image_id))
            }
            ProgramImageClaim::Undisclosed { root } => format!("an undisclosed image under root {}", hex::encode(root)),
        })
        .collect();
    format!(
        "receipt {} bytes; claims {}; {} private action(s); {} boundary step(s)",
        tx.witness_set.proof().clone().into_inner().len(),
        claims.join(", "),
        tx.message.execution.private_actions.len(),
        tx.message.execution.boundary.len()
    )
}

/// Submit a stored transaction (a borsh `LeeTransaction`) as it is.
pub async fn stored(live: &mut Live, path: &Path) -> Result<()> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let tx: LeeTransaction = borsh::from_slice(&bytes).map_err(|e| anyhow!("{}: {e}", path.display()))?;
    let note = format!("{} ({} bytes): {}", path.file_name().and_then(|n| n.to_str()).unwrap_or("?"), bytes.len(), describe_claims(&tx));
    println!("[private] stored transaction: {note}");
    let outcome = live.send_raw("the stored real-proof transaction, as proven for the harness state", tx, note).await?;
    println!("[private] -> {} {}", outcome.row.status, outcome.row.note);
    live.save()
}

// ---------------------------------------------------------------------------------------------
// The wallet's sequencer client, recorded or replayed
// ---------------------------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct Entry {
    method: String,
    params: Box<RawValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ok: Option<Box<RawValue>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    err: Option<String>,
}

struct Tape {
    path: PathBuf,
    /// Replay: the recorded entries not yet consumed.
    recorded: Vec<Entry>,
    /// Where `sendTransaction` writes the transaction (borsh `LeeTransaction`).
    tx_out: PathBuf,
}

/// LEZ's wallet talks to the sequencer through this: every call goes to the sequencer and is
/// appended to a transcript, or, in replay, is answered from a transcript.
#[derive(Clone)]
pub struct TapClient {
    inner: Option<SequencerClient>,
    tape: Arc<Mutex<Tape>>,
}

fn raw<T: Serialize>(value: &T) -> Box<RawValue> {
    RawValue::from_string(serde_json::to_string(value).expect("RPC types serialize")).expect("valid JSON")
}

impl TapClient {
    pub fn recording(inner: SequencerClient, transcript: &Path, tx_out: &Path) -> Result<Self> {
        fs::write(transcript, b"")?;
        Ok(Self {
            inner: Some(inner),
            tape: Arc::new(Mutex::new(Tape { path: transcript.to_path_buf(), recorded: Vec::new(), tx_out: tx_out.to_path_buf() })),
        })
    }

    pub fn replaying(transcript: &Path, tx_out: &Path) -> Result<Self> {
        let text = fs::read_to_string(transcript).with_context(|| format!("reading {}", transcript.display()))?;
        let recorded = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str::<Entry>(l).map_err(|e| anyhow!("transcript: {e}")))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            inner: None,
            tape: Arc::new(Mutex::new(Tape { path: transcript.to_path_buf(), recorded, tx_out: tx_out.to_path_buf() })),
        })
    }

    fn append(&self, entry: &Entry) {
        let tape = self.tape.lock().expect("tape");
        if let Ok(mut file) = fs::OpenOptions::new().append(true).open(&tape.path) {
            let _ = writeln!(file, "{}", serde_json::to_string(entry).expect("entry"));
        }
    }

    async fn tap<T, P, F, Fut>(&self, method: &str, params: &P, call: F) -> Result<T, ClientError>
    where
        T: Serialize + DeserializeOwned,
        P: Serialize,
        F: FnOnce(SequencerClient) -> Fut,
        Fut: Future<Output = Result<T, ClientError>>,
    {
        match &self.inner {
            Some(inner) => {
                let result = call(inner.clone()).await;
                let entry = match &result {
                    Ok(value) => Entry { method: method.into(), params: raw(params), ok: Some(raw(value)), err: None },
                    Err(error) => Entry { method: method.into(), params: raw(params), ok: None, err: Some(error.to_string()) },
                };
                self.append(&entry);
                result
            }
            None => {
                let entry = {
                    let mut tape = self.tape.lock().expect("tape");
                    let index = tape
                        .recorded
                        .iter()
                        .position(|e| e.method == method)
                        .ok_or_else(|| ClientError::Custom(format!("the transcript has no further {method} call")))?;
                    tape.recorded.remove(index)
                };
                let asked = raw(params);
                if asked.get() != entry.params.get() {
                    return Err(ClientError::Custom(format!(
                        "the wallet asks {method}({}) where the transcript recorded {method}({})",
                        asked.get(),
                        entry.params.get()
                    )));
                }
                match (entry.ok, entry.err) {
                    (Some(ok), _) => serde_json::from_str(ok.get())
                        .map_err(|e| ClientError::Custom(format!("transcript: the result of {method} does not decode: {e}"))),
                    (None, error) => Err(ClientError::Custom(error.unwrap_or_else(|| "recorded failure".into()))),
                }
            }
        }
    }
}

#[derive(Serialize)]
struct SentTransaction {
    hash: String,
    bytes: usize,
    file: String,
}

impl WalletSequencerClient for TapClient {
    type Error = ClientError;

    async fn send_transaction(&self, tx: LeeTransaction) -> Result<HashType, Self::Error> {
        let hash = tx.hash();
        let bytes = borsh::to_vec(&tx).expect("a transaction serializes");
        let (tx_out, file) = {
            let tape = self.tape.lock().expect("tape");
            (tape.tx_out.clone(), tape.tx_out.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string())
        };
        fs::write(&tx_out, &bytes).map_err(|e| ClientError::Custom(format!("writing {}: {e}", tx_out.display())))?;
        let params = SentTransaction { hash: hash.to_string(), bytes: bytes.len(), file };
        match &self.inner {
            Some(inner) => {
                let result = <SequencerClient as WalletSequencerClient>::send_transaction(inner, tx).await;
                let entry = match &result {
                    Ok(value) => Entry { method: "sendTransaction".into(), params: raw(&params), ok: Some(raw(value)), err: None },
                    Err(error) => Entry { method: "sendTransaction".into(), params: raw(&params), ok: None, err: Some(error.to_string()) },
                };
                self.append(&entry);
                result
            }
            // Replay: the transaction is written, not sent.
            None => Ok(hash),
        }
    }

    async fn get_block(&self, block_id: BlockId) -> Result<Option<Block>, Self::Error> {
        self.tap("getBlock", &block_id, |c| async move { <SequencerClient as WalletSequencerClient>::get_block(&c, block_id).await })
            .await
    }

    async fn get_block_range(&self, start_block_id: BlockId, end_block_id: BlockId) -> Result<Vec<Block>, Self::Error> {
        self.tap("getBlockRange", &(start_block_id, end_block_id), |c| async move {
            <SequencerClient as WalletSequencerClient>::get_block_range(&c, start_block_id, end_block_id).await
        })
        .await
    }

    async fn get_last_block_id(&self) -> Result<BlockId, Self::Error> {
        self.tap("getLastBlockId", &(), |c| async move { <SequencerClient as WalletSequencerClient>::get_last_block_id(&c).await })
            .await
    }

    async fn get_account_balance(&self, account_id: AccountId) -> Result<u128, Self::Error> {
        self.tap("getAccountBalance", &account_id, |c| async move {
            <SequencerClient as WalletSequencerClient>::get_account_balance(&c, account_id).await
        })
        .await
    }

    async fn get_transaction(&self, tx_hash: HashType) -> Result<Option<(LeeTransaction, BlockId)>, Self::Error> {
        self.tap("getTransaction", &tx_hash, |c| async move {
            <SequencerClient as WalletSequencerClient>::get_transaction(&c, tx_hash).await
        })
        .await
    }

    async fn get_accounts_nonces(&self, account_ids: Vec<AccountId>) -> Result<Vec<Nonce>, Self::Error> {
        let params = account_ids.clone();
        self.tap("getAccountsNonces", &params, |c| async move {
            <SequencerClient as WalletSequencerClient>::get_accounts_nonces(&c, account_ids).await
        })
        .await
    }

    async fn get_account_view(&self, actor_state_selector: Actor) -> Result<Option<Account>, Self::Error> {
        self.tap("getAccountView", &actor_state_selector, |c| async move {
            <SequencerClient as WalletSequencerClient>::get_account_view(&c, actor_state_selector).await
        })
        .await
    }

    async fn get_proofs_and_root(
        &self,
        commitments: Vec<Commitment>,
        message_positions: Vec<CommitmentPosition>,
    ) -> Result<(Vec<Option<MembershipProof>>, Vec<Option<MembershipProof>>, CommitmentSetDigest), Self::Error> {
        let params = (commitments.clone(), message_positions.clone());
        self.tap("getProofsAndRoot", &params, |c| async move {
            <SequencerClient as WalletSequencerClient>::get_proofs_and_root(&c, commitments, message_positions).await
        })
        .await
    }

    async fn get_fee_state(&self) -> Result<FeeStateQuote, Self::Error> {
        self.tap("getFeeState", &(), |c| async move { <SequencerClient as WalletSequencerClient>::get_fee_state(&c).await }).await
    }

    async fn get_account(&self, account_id: AccountId) -> Result<Option<Account>, Self::Error> {
        self.tap("getAccount", &account_id, |c| async move {
            <SequencerClient as WalletSequencerClient>::get_account(&c, account_id).await
        })
        .await
    }

    async fn get_program_ids(&self) -> Result<BTreeMap<String, ProgramId>, Self::Error> {
        self.tap("getProgramIds", &(), |c| async move { <SequencerClient as WalletSequencerClient>::get_program_ids(&c).await })
            .await
    }

    async fn get_publications(
        &self,
        from_position: CommitmentPosition,
        limit: PageLimit,
    ) -> Result<Vec<(CommitmentPosition, Publication)>, Self::Error> {
        self.tap("getPublications", &(from_position, limit), |c| async move {
            <SequencerClient as WalletSequencerClient>::get_publications(&c, from_position, limit).await
        })
        .await
    }

    async fn get_recovery_binding(&self, address: AccountId) -> Result<Option<EncryptedNote>, Self::Error> {
        self.tap("getRecoveryBinding", &address, |c| async move {
            <SequencerClient as WalletSequencerClient>::get_recovery_binding(&c, address).await
        })
        .await
    }
}

// ---------------------------------------------------------------------------------------------
// 2. The private update for the live instance
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Proof {
    /// `RISC0_DEV_MODE=1` in this process: a fake receipt.
    Fake,
    /// A real succinct receipt.
    Real,
}

fn dev_mode() -> bool {
    matches!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1") | Ok("true"))
}

pub struct PrivateJob {
    pub world: World,
    pub update: ferrata_core::types::LightClientUpdate,
    pub message: Vec<u8>,
    pub slot: u64,
}

/// The private update: update_2 of the fixtures as a finality update, for the twin deployment's
/// instance B once it has applied update_0 and update_1 (as the scenarios leave it). `store` is
/// the instance's store as the chain holds it.
pub fn private_job(twin: AccountId, fixtures: &Fixtures, on_chain: Option<&ferrata_core::witness::CompactStore>) -> Result<PrivateJob> {
    let updates = &fixtures.updates;
    let world = world_b_after(twin, fixtures, &[updates[0].clone(), as_finality_update(&updates[1]), updates[1].clone()])?;
    let native_store = ferrata_core::witness::CompactStore::of(world.native.core());
    if let Some(on_chain) = on_chain {
        ensure!(*on_chain == native_store, "instance B is not at update_1 (run the scenarios first)");
    }
    let update = as_finality_update(&updates[2]);
    let inst = &world.inst;
    let fork = LcFork::Electra;
    let compact = CompactUpdate::of(&update);
    let witness = signer_witness(world.native.core(), &update, &world.native.spec);
    let (_, outcome) = plan_job(&native_store, &compact, &witness, &update, &inst.config);
    ensure!(outcome.is_some_and(|o| o.advanced), "the update verifies natively and advances the header");
    let message = borsh::to_vec(&Message::VerifyAll {
        config: inst.config.clone(),
        checkpoint: inst.checkpoint,
        store: native_store.encode(),
        update: compact.encode(fork),
        signers: witness.encode(),
        adoption: vec![],
    })?;
    let slot = update.finalized_header.beacon.slot;
    Ok(PrivateJob { world, update, message, slot })
}

fn private_wallet(home: &Path, rpc: &str, client: TapClient) -> Result<(WalletCore<TapClient>, AccountId)> {
    check_loopback(rpc)?;
    fs::create_dir_all(home)?;
    let config_path = home.join("wallet_config.json");
    let storage_path = home.join("storage.json");
    let _ = fs::remove_file(&storage_path);
    fs::write(
        &config_path,
        format!(
            "{{\n  \"sequencer\": {{ \"sequencer_addr\": \"{rpc}\" }},\n  \"seq_poll_timeout\": \"1s\",\n  \
             \"seq_tx_poll_max_blocks\": 120,\n  \"seq_poll_max_retries\": 2,\n  \"seq_block_poll_max_amount\": 100\n}}\n"
        ),
    )?;
    let (mut wallet, _mnemonic) = WalletCore::new_init_storage_with_client(config_path, storage_path, None, "", client)?;
    // LEZ's first test private account: the genesis of its local network credits it, and its
    // keys are in LEZ's source (`testnet_initial_state`).
    let account = testnet_initial_state::initial_priv_accounts_private_keys()
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("LEZ's initial state has no private account"))?;
    let id = account.account_id();
    wallet.storage_mut().key_chain_mut().add_imported_private_account(account.key_chain, account.chain_index, account.account);
    Ok((wallet, id))
}

fn describe_private(tx_out: &Path) -> Result<String> {
    let bytes = fs::read(tx_out)?;
    let tx: LeeTransaction = borsh::from_slice(&bytes)?;
    Ok(format!("transaction {} bytes: {}", bytes.len(), describe_claims(&tx)))
}

/// Build the private update with LEZ's wallet, prove it (really or not), submit it, and record
/// what the sequencer does with it.
pub async fn private_update(
    live: &mut Live,
    twin: AccountId,
    fixtures: &Fixtures,
    program: &GuestBinary,
    proof: Proof,
    name: &str,
) -> Result<()> {
    ensure!(
        dev_mode() == (proof == Proof::Fake),
        "RISC0_DEV_MODE must be 1 for the fake receipt and unset for the real proof"
    );
    live.sync().await?;
    let on_chain = store(live, &world_b_after(twin, fixtures, &[])?.inst).await?;
    let PrivateJob { mut world, update, message, slot } = private_job(twin, fixtures, Some(&on_chain))?;
    let inst = world.inst.clone();

    let dir = live.opts.run_dir.join(format!("private-{name}"));
    fs::create_dir_all(&dir)?;
    let transcript = dir.join("rpc-transcript.jsonl");
    let tx_out = dir.join("transaction.borsh");
    let _ = fs::remove_file(&tx_out);
    let client = TapClient::recording(live.wallet.client_owned(), &transcript, &tx_out)?;
    let (mut wallet, payer) = private_wallet(&dir.join("wallet"), &live.opts.rpc, client)?;
    let tip = wallet.start_at_chain_tip().await?;
    let catalog = ProgramCatalog::from([(twin, Program::new(Cow::Owned(program.bytes.clone()))?)]);
    let mentions = || {
        vec![
            AccountIdentity::PrivateOwned(payer).select_program_actor_state(twin),
            mention(inst.pda()),
            mention(inst.header(slot)),
        ]
    };
    println!(
        "[private] update_2 as a finality update, verified by VerifyAll at the private account {payer}'s actor of {twin}; wallet synced at block {tip}; {}",
        if proof == Proof::Fake { "RISC0_DEV_MODE=1: a fake receipt" } else { "a real proof" }
    );

    // LEZ's own dry run: the simulated public part and the fee it would reserve.
    wallet.set_dry_run(true);
    match wallet.send_privacy_preserving_tx_with_pre_check(mentions(), 0, message.clone(), &catalog, None, |_| Ok(())).await {
        Err(wallet::ExecutionFailureKind::DryRun) => {}
        Err(error) => bail!("the wallet's dry run failed: {error}: {error:?}"),
        Ok(_) => bail!("a dry run sent a transaction"),
    }
    wallet.set_dry_run(false);

    let step = match proof {
        Proof::Fake => "private update for the live instance, fake receipt (RISC0_DEV_MODE=1 in the wallet)",
        Proof::Real => "private update for the live instance, real proof",
    };
    // A row before the proof starts, in case it is stopped.
    let pending = Row {
        phase: live.phase.clone(),
        step: step.to_string(),
        status: "unfinished".into(),
        note: "the proof was started and had not finished when the run was recorded".into(),
        ..Row::default()
    };
    live.rows.push(pending);
    live.save()?;
    let pending_index = live.rows.len() - 1;

    let since_head = live.head().await?;
    let started = Instant::now();
    let sent = wallet.send_privacy_preserving_tx_with_pre_check(mentions(), 0, message, &catalog, None, |_| Ok(())).await;
    let seconds = started.elapsed().as_secs_f64();
    live.rows.remove(pending_index);
    let hash = match sent {
        Ok((hash, _)) => hash,
        Err(error) => {
            let row = Row {
                phase: live.phase.clone(),
                step: step.to_string(),
                status: "rejected".into(),
                note: format!("after {seconds:.1} s: {error}: {error:?}"),
                ..Row::default()
            };
            println!("[private] {step}: rejected: {}", row.note);
            live.rows.push(row);
            return live.save();
        }
    };
    let described = describe_private(&tx_out)?;
    println!("[private] proved and sent in {seconds:.1} s: {described}");
    let outcome = live
        .settle(Sent {
            step: step.to_string(),
            hash,
            since_head,
            payer: None,
            balance_before: 0,
            harness: None,
            note: format!("execute_and_prove and submission {seconds:.1} s; {described}"),
        })
        .await?;
    match proof {
        Proof::Fake => ensure!(
            outcome.row.status == "dropped",
            "a sequencer that verifies receipts must not include a fake one; it {} it",
            outcome.row.status
        ),
        Proof::Real => {
            if outcome.applied() {
                world.native.process(&update).map_err(|e| anyhow!("native: {e:?}"))?;
                check_store(live, &world).await?;
                check_record(live, &world, slot).await?;
                println!("[private] the instance holds the native store after the update, and the record of slot {slot}");
            }
        }
    }
    live.save()
}

/// Without a sequencer: rebuild the proving job from a transcript and write the proven
/// transaction. `RISC0_DEV_MODE` decides whether the receipt is real.
pub async fn prove_from_transcript(
    twin: AccountId,
    fixtures: &Fixtures,
    program: &GuestBinary,
    transcript: &Path,
    tx_out: &Path,
    scratch: &Path,
) -> Result<()> {
    let PrivateJob { world, message, slot, .. } = private_job(twin, fixtures, None)?;
    let inst = world.inst.clone();
    let client = TapClient::replaying(transcript, tx_out)?;
    let (mut wallet, payer) = private_wallet(&scratch.join("wallet"), "http://127.0.0.1:0", client)?;
    wallet.start_at_chain_tip().await?;
    let catalog = ProgramCatalog::from([(twin, Program::new(Cow::Owned(program.bytes.clone()))?)]);
    let mentions = || {
        vec![
            AccountIdentity::PrivateOwned(payer).select_program_actor_state(twin),
            mention(inst.pda()),
            mention(inst.header(slot)),
        ]
    };
    // The recording made the wallet's dry run first; replay it so that the calls line up.
    wallet.set_dry_run(true);
    match wallet.send_privacy_preserving_tx_with_pre_check(mentions(), 0, message.clone(), &catalog, None, |_| Ok(())).await {
        Err(wallet::ExecutionFailureKind::DryRun) => {}
        Err(error) => bail!("the wallet's dry run failed: {error}: {error:?}"),
        Ok(_) => bail!("a dry run sent a transaction"),
    }
    wallet.set_dry_run(false);
    let started = Instant::now();
    let (hash, _) = wallet
        .send_privacy_preserving_tx_with_pre_check(mentions(), 0, message, &catalog, None, |_| Ok(()))
        .await
        .map_err(|e| anyhow!("proving: {e}: {e:?}"))?;
    println!(
        "proved in {:.1} s ({}); transaction {hash} written to {}: {}",
        started.elapsed().as_secs_f64(),
        if dev_mode() { "RISC0_DEV_MODE=1: NOT a proof" } else { "a real proof" },
        tx_out.display(),
        describe_private(tx_out)?
    );
    Ok(())
}
