//! The chain backend for a running LEZ sequencer. A transaction is built exactly as the in-process `HarnessLedger`
//! builds it (`../../src/ledger.rs`: the job step's actors and evidence, the payer's nonce, a fee declaration with the
//! configured gas limit and maximum fee, the payer's signature), sent to the sequencer's RPC with LEZ's wallet client,
//! and settled as the live driver settles its own (`../../live/driver/src/live.rs`): the follower applies every block
//! the sequencer serves with LEZ's block application, only once the block is final on Bedrock, and reads the
//! transaction's status, exit code, execution and storage gas from LEZ's settlement; the payer's balance is read from
//! the sequencer before and after. Every transaction is a row of `<run_dir>/run.tsv` (the live driver's manifest).
//! The actor states the updater reads are the follower's, checked against what the sequencer serves.

use std::{cell::RefCell, collections::BTreeMap, time::Instant};

use anyhow::{anyhow, ensure, Context as _, Result};
use common::transaction::LeeTransaction;
use ferrata_updater::{
    ledger::{Ledger, Tx, TxOutcome},
    utc_now,
};
use lee::{
    public_transaction::{Message as PublicMessage, WitnessSet},
    AccountId, Actor, FeeDeclaration, PrivateKey, PublicKey, PublicTransaction,
};
use sequencer_service_rpc::ClientError;
use wallet::WalletSequencerClient as _;

use crate::{
    live::{Live, Options, Sent},
    manifest::Row,
};

pub struct SequencerLedger {
    rt: tokio::runtime::Runtime,
    live: RefCell<Live>,
}

impl SequencerLedger {
    /// Connect (a sequencer on this machine only), restore the follower from `run_dir` if it was saved there, and
    /// follow the chain up to its last final block.
    pub fn open(opts: Options, payer: &PrivateKey) -> Result<Self> {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
        let started = Instant::now();
        let run_dir = opts.run_dir.clone();
        let mut live = rt.block_on(Live::open(opts, &[])).context("the sequencer")?;
        live.phase = "updater".into();
        let payer_id = AccountId::from(&PublicKey::new_from_private_key(payer));
        let (head, balance) =
            rt.block_on(async { Ok::<_, anyhow::Error>((live.head().await?, live.balance(payer_id).await?)) })?;
        eprintln!(
            "{} [sequencer] {}: head block {head}, following to block {} (final on Bedrock: {}) in {:.1} s; payer {payer_id} \
             balance {balance}; records in {}",
            utc_now(),
            live.opts.rpc,
            live.follower.height(),
            live.opts.wait_final,
            started.elapsed().as_secs_f64(),
            run_dir.display()
        );
        Ok(Self { rt, live: RefCell::new(live) })
    }
}

impl Ledger for SequencerLedger {
    fn actor_state(&self, actor: Actor) -> Result<Vec<u8>> {
        let mut live = self.live.borrow_mut();
        self.rt.block_on(async {
            live.sync().await?;
            live.actor_state(actor).await
        })
    }

    fn submit(&mut self, payer: &PrivateKey, tx: &Tx, gas_limit: u64, max_fee: u128) -> Result<TxOutcome> {
        let live = self.live.get_mut();
        self.rt.block_on(submit(live, payer, tx, gas_limit, max_fee))
    }

    fn flush(&mut self) -> Result<()> {
        self.live.get_mut().save()
    }
}

async fn submit(live: &mut Live, payer: &PrivateKey, tx: &Tx, gas_limit: u64, max_fee: u128) -> Result<TxOutcome> {
    // A sequencer not on this machine: only the day-one deployment's, with the owner's FERRATA_TESTNET_SEND=yes.
    crate::live::check_send(&live.opts.rpc)?;
    live.sync().await?;
    let payer_id = AccountId::from(&PublicKey::new_from_private_key(payer));
    let nonce =
        *live.wallet.get_accounts_nonces(&[payer_id]).await?.first().ok_or_else(|| anyhow!("no nonce served"))?;
    let balance_before = live.balance(payer_id).await?;
    let since_head = live.head().await?;
    let msg = PublicMessage::new(
        tx.root,
        tx.message.clone(),
        tx.actors.clone(),
        BTreeMap::from([(payer_id, nonce)]),
        Some(FeeDeclaration::new(payer_id, gas_limit, 0, max_fee)),
        tx.evidence.clone(),
    );
    let witness_set = WitnessSet::for_message(&msg, &[payer]);
    let ltx = LeeTransaction::Public(PublicTransaction::new(msg, witness_set));
    let hash = ltx.hash();
    let sent_at = Instant::now();
    match live.wallet.client().send_transaction(ltx).await {
        Ok(returned) => ensure!(returned == hash, "the sequencer returned another hash for {}", tx.label),
        // The sequencer answered with an error: it took nothing in, charged nothing.
        Err(ClientError::Call(e)) => {
            let row = Row {
                phase: live.phase.clone(),
                step: tx.label.clone(),
                tx_hash: hash.to_string(),
                status: "rejected".into(),
                gas_limit: gas_limit.to_string(),
                note: format!("sequencer RPC: {e}"),
                ..Row::default()
            };
            println!("[{}] {:<58} rejected: {}", live.phase, row.step, row.note);
            live.rows.push(row);
            crate::manifest::write(&live.opts.run_dir.join("run.tsv"), &live.rows)?;
            return Ok(TxOutcome { rejected: true, note: format!("{hash}: sequencer RPC: {e}"), ..Default::default() });
        }
        // Not sent, or no answer: a transport failure (the updater retries, after reading the chain again).
        Err(e) => return Err(anyhow!("sequencer RPC: {e}")),
    }
    let note = format!("nonce {}, gas limit {gas_limit}, max fee {max_fee}", nonce.0);
    let out = live
        .settle(Sent {
            step: tx.label.clone(),
            hash,
            sent_at,
            since_head,
            payer: Some(payer_id),
            balance_before,
            harness: None,
            note,
        })
        .await?;
    let receipt = out.receipt.as_ref();
    let row = &out.row;
    let fee = row.payer_debit.parse::<u128>().ok().or_else(|| row.fee.parse().ok()).unwrap_or(0);
    Ok(TxOutcome {
        applied: out.applied(),
        exit_code: out.exit_code(),
        // Dropped by the sequencer (never in a block): nothing charged.
        rejected: receipt.is_none(),
        gas_exec: receipt.map_or(0, |r| r.gas_exec),
        gas_stor: receipt.map_or(0, |r| r.gas_stor),
        fee,
        block: receipt.map_or(0, |r| r.block_id),
        note: format!("{hash} {}", row.note),
    })
}
