//! Several private reads in one block of a running sequencer (`attest-block`; RFP-021 R.11:
//! "statement submissions are independent of one another, so that multiple private transactions can
//! submit statements in the same block, and a private transaction can compute its resulting state
//! before inclusion"). LEZ v0.4.0-rc3 only (its wallet's private accounts and native transfers).
//!
//! The in-process test `lez/harness/tests/security_privacy.rs` (`r11_several_private_reads_in_one_block`)
//! settles four private attestations in a block loop of its own; here the block is the sequencer's:
//!
//! 1. Publicly: the reference consumer deployed; instance S of the main deployment initialised from
//!    the harness's Stage 4 fixture and its finality update applied (the record of mainnet block
//!    26,384,851); instance E initialised from the mainnet end-to-end fixture (period 1906).
//! 2. Four new private accounts of one wallet, each funded by a native transfer from a public test
//!    account (as LEZ's own `funded_private_account` does).
//! 3. Against one snapshot of the chain: each of the four proves the reference consumer's
//!    `AttestPrivately` of another ERC-20 `Transfer` log of block 26,384,851, and two more wallets
//!    holding LEZ's test private account 0 prove one each (the control). The wallet builds and
//!    proves every transaction and the client holds it (`TapClient::hold`): none is sent yet.
//! 4. Between proving and inclusion: the control's first transaction is sent and included (the
//!    commitment tree moves past the root the others were proven against), and instance E verifies
//!    its finality update and writes a record (the attestation program's state moves).
//! 5. Right after the sequencer makes a block, the four are sent back to back; each must be applied,
//!    all in one block, with nothing re-proven. The record they read is unchanged; every commitment
//!    they computed before inclusion is in the sequencer's tree after it.
//! 6. The control's second transaction, proven against the same snapshot as the first: refused.
//!
//! Proofs are as `RISC0_DEV_MODE` says: with it set (and the sequencer started with it, as LEZ's
//! Justfile documents for development), fake receipts; without it, real ones. The phase writes
//! `r11-one-block.json` into `--out` and every held transaction into `private-block/` of the run.

use std::{
    borrow::Cow,
    collections::BTreeSet,
    fs,
    path::Path,
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, ensure, Result};
use common::transaction::LeeTransaction;
use ferrata_core::{fork::LcFork, rlp::encode_u64, types::LightClientBootstrap, types::LightClientUpdate, Root};
use ferrata_lez_wire::consumer::{ConsumerMessage, Pool};
use ferrata_spectests::execkit::{hexb, receipt_rlp, Trie};
use lee::{privacy_preserving_transaction::circuit::ProgramCatalog, program::Program, AccountId, PrivateKey};
use serde_json::{json, Value};
use wallet::{
    program_facades::{native_token_transfer::NativeTokenTransfer, CreditDelivery},
    AccountIdentity, WalletCore, WalletSequencerClient,
};

use crate::{
    attest::{check_sums, finality_update, read, AttestFixture},
    keys::account_of,
    live::{check_send, mention, Live, Sent},
    plan::mainnet_config,
    private::{describe_private, empty_wallet, private_wallet_at, TapClient},
    programs::GuestBinary,
    scenario::{apply, check_record, deploy, deployment_accounts, init, Harness, World},
};

/// How many private reads go into the one block: LEZ's most per block (its block storage cap,
/// `MAX_GAS_STOR` 1,000,000, over the flat `PRIVATE_GAS_STOR` 224,063 of every private transaction).
const READERS: usize = 4;
/// What each new private account is given: far more than a private attestation's fee.
const FUNDING: u128 = 1_000_000_000;
/// The end-to-end fixture's checkpoint (its `SOURCE.md`: the bootstrap's block root, period 1906).
const E2E_CHECKPOINT: &str = "dab6b45fd34b9da264b15ad3e85dad586f836f3b11f5922e4c116b53574aefd3";
/// keccak256("Transfer(address,address,uint256)").
const TRANSFER_TOPIC: &str = "ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";

/// One ERC-20 `Transfer` log of the fixture's block that the reference consumer accepts.
struct Transfer {
    tx_index: u64,
    log_index: u64,
    token: [u8; 20],
    recipient: [u8; 20],
    proof: Vec<Vec<u8>>,
}

/// The first acceptable `Transfer` log of each of the block's first `n` transactions that have
/// one: three topics, the sender and recipient address words, one data word that fits 128 bits
/// (what `Pool::accept` takes).
fn transfers(dir: &Path, fx: &AttestFixture, n: usize) -> Result<Vec<Transfer>> {
    let receipts = serde_json::from_slice::<Value>(&read(dir, &format!("receipts_{}.json", fx.block))?)?["result"]
        .as_array()
        .ok_or_else(|| anyhow!("receipts_{}.json: no result array", fx.block))?
        .clone();
    let trie = Trie::new(receipts.iter().enumerate().map(|(i, r)| {
        let (key, len) = encode_u64(i as u64);
        (key[..len].to_vec(), receipt_rlp(r))
    }));
    ensure!(trie.root() == fx.receipts_root, "the receipts do not hash to the record's receipts root");
    let topic = hexb(TRANSFER_TOPIC);
    let mut out = Vec::new();
    for (i, receipt) in receipts.iter().enumerate() {
        let logs = receipt["logs"].as_array().cloned().unwrap_or_default();
        let found = logs.iter().enumerate().find_map(|(j, log)| {
            let topics: Vec<Vec<u8>> =
                log["topics"].as_array()?.iter().map(|t| t.as_str().map(hexb)).collect::<Option<_>>()?;
            let data = hexb(log["data"].as_str()?);
            let address = hexb(log["address"].as_str()?);
            let ok = topics.len() == 3
                && topics.iter().all(|t| t.len() == 32)
                && topics[0] == topic
                && topics[1][..12] == [0u8; 12]
                && topics[2][..12] == [0u8; 12]
                && data.len() == 32
                && data[..16] == [0u8; 16]
                && address.len() == 20;
            ok.then(|| (j, address, topics[2][12..].to_vec()))
        });
        if let Some((j, token, recipient)) = found {
            let (key, len) = encode_u64(i as u64);
            out.push(Transfer {
                tx_index: i as u64,
                log_index: j as u64,
                token: token.try_into().expect("20 bytes"),
                recipient: recipient.try_into().expect("20 bytes"),
                proof: trie.proof(&key[..len]),
            });
            if out.len() == n {
                return Ok(out);
            }
        }
    }
    bail!("block {} has {} acceptable Transfer logs in distinct transactions, not {n}", fx.block, out.len())
}

/// The end-to-end fixture: a bootstrap and a finality update of period 1906.
fn other_fixture(dir: &Path) -> Result<(Root, LightClientBootstrap, LightClientUpdate)> {
    check_sums(dir)?;
    let checkpoint: Root = hexb(E2E_CHECKPOINT).try_into().map_err(|_| anyhow!("checkpoint"))?;
    let bootstrap = LightClientBootstrap::decode(&read(dir, "bootstrap.ssz")?, LcFork::Electra, 512)
        .map_err(|e| anyhow!("bootstrap: {e:?}"))?;
    ensure!(bootstrap.header.beacon.hash_tree_root() == checkpoint, "the bootstrap is not the checkpoint's");
    let update = finality_update(&read(dir, "finality_update.ssz")?)?;
    Ok((checkpoint, bootstrap, update))
}

/// The commitment root the sequencer holds now, and which of `commitments` are in its tree.
async fn tree<C: WalletSequencerClient>(
    client: &C,
    commitments: Vec<lee_core::Commitment>,
) -> Result<(String, Vec<bool>)>
where
    C::Error: std::fmt::Display,
{
    let (proofs, _, root) =
        client.get_proofs_and_root(commitments, Vec::new()).await.map_err(|e| anyhow!("getProofsAndRoot: {e}"))?;
    Ok((hex::encode(borsh::to_vec(&root)?), proofs.iter().map(Option::is_some).collect()))
}

fn private_tx(path: &Path) -> Result<(LeeTransaction, lee::PrivacyPreservingTransaction)> {
    let tx: LeeTransaction = borsh::from_slice(&fs::read(path)?).map_err(|e| anyhow!("{}: {e}", path.display()))?;
    let LeeTransaction::PrivacyPreserving(inner) = &tx else { bail!("{}: not a private transaction", path.display()) };
    let inner = inner.clone();
    Ok((tx, inner))
}

fn root_of(tx: &lee::PrivacyPreservingTransaction) -> Result<String> {
    let roots: BTreeSet<Vec<u8>> =
        tx.message.execution.private_actions.iter().map(|a| borsh::to_vec(&a.root)).collect::<Result<_, _>>()?;
    ensure!(roots.len() == 1, "a transaction proven against {} commitment roots", roots.len());
    Ok(hex::encode(roots.into_iter().next().unwrap()))
}

#[allow(clippy::too_many_arguments)]
pub async fn attest_block(
    live: &mut Live,
    program: &GuestBinary,
    consumer: &GuestBinary,
    fixture_dir: &Path,
    other_dir: &Path,
    payer: AccountId,
    funder_key: &PrivateKey,
) -> Result<()> {
    let dev_mode = matches!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1") | Ok("true"));
    let proofs = if dev_mode { "fake receipts (RISC0_DEV_MODE)" } else { "real proofs" };
    let fx = AttestFixture::load(fixture_dir)?;
    let reads = transfers(fixture_dir, &fx, READERS)?;
    let (e_checkpoint, e_bootstrap, e_update) = other_fixture(other_dir)?;
    let attest = deployment_accounts("main", program).0;
    let funder = account_of(funder_key);
    let mut summary = json!({ "proofs": proofs, "readers": READERS });

    // 1. The consumer, instance S with its record, instance E.
    let consumer_at = deploy(live, "consumer", consumer, payer).await?;
    let mut s = World::with_checkpoint(
        "S (attest fixture)",
        attest,
        mainnet_config(2),
        fx.checkpoint,
        &fx.bootstrap,
        LcFork::Electra,
    )?;
    init(live, &mut s, &fx.bootstrap, "instance S Init", Harness::None, payer).await?;
    let run = apply(live, &mut s, &fx.update, "instance S finality update", Harness::None, payer).await?;
    ensure!(run.outcome.is_some_and(|o| o.advanced), "instance S's finality update finalizes the block's header");
    check_record(live, &s, fx.finalized_slot).await?;
    let mut e = World::with_checkpoint(
        "E (end-to-end fixture)",
        attest,
        mainnet_config(2),
        e_checkpoint,
        &e_bootstrap,
        LcFork::Electra,
    )?;
    init(live, &mut e, &e_bootstrap, "instance E Init", Harness::None, payer).await?;
    let record = s.inst.header(fx.finalized_slot);
    println!(
        "[block] consumer {consumer_at}; instance S {} holds the record of slot {} (block {}); instance E {} initialised; {} reads: {}",
        hex::encode(s.inst.id),
        fx.finalized_slot,
        fx.block,
        hex::encode(e.inst.id),
        READERS,
        reads.iter().map(|t| format!("tx {} log {}", t.tx_index, t.log_index)).collect::<Vec<_>>().join(", ")
    );

    // 2. Four new private accounts, funded.
    let dir = live.opts.run_dir.join("private-block");
    fs::create_dir_all(&dir)?;
    let client = TapClient::recording(
        live.wallet.client_owned(),
        &dir.join("rpc-transcript.jsonl"),
        &dir.join("funding.borsh"),
    )?;
    let mut wallet: WalletCore<TapClient> = empty_wallet(&dir.join("wallet"), &live.opts.rpc, client.clone())?;
    wallet.storage_mut().key_chain_mut().add_imported_public_account(crate::live::wallet_key(&funder_key));
    wallet.start_at_chain_tip().await?;
    let accounts: Vec<AccountId> = (0..READERS).map(|_| wallet.create_new_account_private(None).0).collect();
    for (i, account) in accounts.iter().enumerate() {
        let since_head = live.head().await?;
        let (hash, _) = NativeTokenTransfer(&wallet)
            .transfer(
                AccountIdentity::Public(funder),
                AccountIdentity::PrivateOwned(*account),
                FUNDING,
                CreditDelivery::Automatic,
            )
            .await
            .map_err(|e| anyhow!("funding reader {i}: {e}: {e:?}"))?;
        let outcome = live
            .settle(Sent {
                step: format!("fund reader {i}'s new private account ({FUNDING} from a public test account)"),
                hash,
                sent_at: Instant::now(),
                since_head,
                payer: None,
                balance_before: 0,
                harness: None,
                note: format!("LEZ's NativeTokenTransfer, public {funder} to a new private account; {proofs}"),
            })
            .await?;
        ensure!(outcome.applied(), "funding reader {i}: {} {}", outcome.row.status, outcome.row.note);
    }
    wallet.sync_to_latest_block().await?;
    let funded: Vec<lee_core::Commitment> = accounts
        .iter()
        .map(|a| wallet.get_private_account_commitment(*a).ok_or_else(|| anyhow!("the wallet does not know {a}")))
        .collect::<Result<_>>()?;
    let (_, present) = tree(live.wallet.client(), funded).await?;
    ensure!(present.iter().all(|p| *p), "a funded account's note is not in the sequencer's tree: {present:?}");

    // 3. One snapshot: every transaction built and proven, none sent.
    let catalog = || -> Result<ProgramCatalog> {
        Ok(ProgramCatalog::from([
            (consumer_at, Program::new(Cow::Owned(consumer.bytes.clone()))?),
            (attest, Program::new(Cow::Owned(program.bytes.clone()))?),
        ]))
    };
    let message = |t: &Transfer| -> Result<Vec<u8>> {
        let pool = Pool {
            ferrata: attest,
            instance_id: s.inst.id,
            chain_id: fx.chain_id,
            token: t.token,
            recipient: t.recipient,
        };
        Ok(borsh::to_vec(&ConsumerMessage::AttestPrivately {
            pool,
            slot: fx.finalized_slot,
            tx_index: t.tx_index,
            log_index: t.log_index,
            proof: t.proof.clone(),
            header: None,
        })?)
    };
    let snapshot_head = live.head().await?;
    let (snapshot_root, _) = tree(live.wallet.client(), Vec::new()).await?;
    let record_before = live.actor_state(record.actor()).await?;
    ensure!(!record_before.is_empty(), "no record to read");
    client.hold(true);
    let started = Instant::now();
    let mut held = Vec::new();
    for (i, (account, t)) in accounts.iter().zip(&reads).enumerate() {
        let path = dir.join(format!("reader-{i}.borsh"));
        client.set_tx_out(&path);
        let mentions =
            vec![AccountIdentity::PrivateOwned(*account).select_program_actor_state(consumer_at), mention(record)];
        let (hash, _) = wallet
            .send_privacy_preserving_tx_with_pre_check(mentions, 0, message(t)?, &catalog()?, None, |_| Ok(()))
            .await
            .map_err(|e| anyhow!("reader {i}'s private attestation: {e}: {e:?}"))?;
        let (tx, inner) = private_tx(&path)?;
        ensure!(tx.hash() == hash, "reader {i}: the held transaction is not the one the wallet built");
        held.push((tx, inner, describe_private(&path)?));
    }
    // The control: LEZ's test private account 0, in two wallets that know nothing of each other.
    let mut control = Vec::new();
    for k in 0..2 {
        let cdir = dir.join(format!("control-{k}"));
        let path = dir.join(format!("control-{k}.borsh"));
        fs::create_dir_all(&cdir)?;
        let cclient = TapClient::recording(live.wallet.client_owned(), &cdir.join("rpc-transcript.jsonl"), &path)?;
        cclient.hold(true);
        let (mut cwallet, me) = private_wallet_at(&cdir.join("wallet"), &live.opts.rpc, cclient, 0)?;
        cwallet.start_at_chain_tip().await?;
        let mentions = vec![AccountIdentity::PrivateOwned(me).select_program_actor_state(consumer_at), mention(record)];
        let (hash, _) = cwallet
            .send_privacy_preserving_tx_with_pre_check(mentions, 0, message(&reads[0])?, &catalog()?, None, |_| Ok(()))
            .await
            .map_err(|e| anyhow!("control {k}: {e}: {e:?}"))?;
        let (tx, inner) = private_tx(&path)?;
        ensure!(tx.hash() == hash, "control {k}: the held transaction is not the one the wallet built");
        control.push((tx, inner, describe_private(&path)?));
    }
    let proven_in = started.elapsed().as_secs_f64();
    let proven_head = live.head().await?;
    let (root_after_proving, _) = tree(live.wallet.client(), Vec::new()).await?;
    ensure!(root_after_proving == snapshot_root, "the commitment tree moved while the transactions were proven");
    for (i, (_, inner, _)) in held.iter().chain(&control).enumerate() {
        ensure!(
            root_of(inner)? == snapshot_root,
            "transaction {i} is not proven against the snapshot's commitment root"
        );
    }
    println!(
        "[block] {} transactions proven in {proven_in:.1} s against the snapshot (blocks {snapshot_head}..={proven_head}, commitment root {snapshot_root}), none sent",
        held.len() + control.len()
    );
    summary["snapshot"] = json!({
        "head_before_proving": snapshot_head,
        "head_after_proving": proven_head,
        "commitment_root": snapshot_root,
        "proving_seconds": (proven_in * 10.0).round() / 10.0,
    });

    // 4. Between proving and inclusion: the control's first transaction; instance E's update.
    let first = live
        .send_raw(
            "control: LEZ's test private account 0, first transaction of the snapshot",
            control[0].0.clone(),
            format!("AttestPrivately proven against the snapshot; {}", control[0].2),
        )
        .await?;
    ensure!(first.applied(), "the control's first transaction: {} {}", first.row.status, first.row.note);
    let run = apply(
        live,
        &mut e,
        &e_update,
        "instance E finality update, between proving and inclusion",
        Harness::None,
        payer,
    )
    .await?;
    ensure!(run.outcome.is_some_and(|o| o.advanced), "instance E's finality update finalizes its header");
    check_record(live, &e, e_update.finalized_header.beacon.slot).await?;
    let (root_before_block, _) = tree(live.wallet.client(), Vec::new()).await?;
    ensure!(root_before_block != snapshot_root, "the commitment tree did not move between proving and inclusion");
    ensure!(live.actor_state(record.actor()).await? == record_before, "the record changed before the block");
    summary["between"] = json!({
        "control_first": { "hash": first.row.tx_hash, "block": first.row.block, "status": first.row.status },
        "instance_e_finality_update": { "finalized_slot": e_update.finalized_header.beacon.slot, "job_transactions": run.outcomes.len(),
            "blocks": run.outcomes.iter().map(|o| o.row.block.clone()).collect::<Vec<_>>() },
        "commitment_root_before_the_block": root_before_block,
    });

    // 5. The four, back to back, just after the sequencer made a block.
    check_send(&live.opts.rpc)?;
    live.sync().await?;
    let h0 = live.head().await?;
    let deadline = Instant::now() + Duration::from_secs(120);
    while live.head().await? == h0 {
        ensure!(Instant::now() < deadline, "the sequencer made no block in 120 s");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let since_head = live.head().await?;
    let send_started = Instant::now();
    let mut sent = Vec::new();
    for (i, (tx, _, _)) in held.iter().enumerate() {
        let returned = live
            .wallet
            .client()
            .send_transaction(tx.clone())
            .await
            .map_err(|e| anyhow!("reader {i}: the sequencer's RPC refused it: {e}"))?;
        ensure!(returned == tx.hash(), "reader {i}: the sequencer returned another hash");
        sent.push((returned, Instant::now()));
    }
    let send_ms = send_started.elapsed().as_millis();
    let mut blocks = BTreeSet::new();
    let mut rows = Vec::new();
    for (i, ((hash, at), (_, _, described))) in sent.into_iter().zip(&held).enumerate() {
        let t = &reads[i];
        let outcome = live
            .settle(Sent {
                step: format!(
                    "reader {i}: AttestPrivately of tx {} log {}, proven against the snapshot",
                    t.tx_index, t.log_index
                ),
                hash,
                sent_at: at,
                since_head,
                payer: None,
                balance_before: 0,
                harness: None,
                note: format!("sent {send_ms} ms burst after block {since_head}; {described}"),
            })
            .await?;
        ensure!(outcome.applied(), "reader {i}: {} {}", outcome.row.status, outcome.row.note);
        blocks.insert(outcome.row.block.clone());
        rows.push(json!({
            "reader": i, "tx_index": t.tx_index, "log_index": t.log_index, "token": hex::encode(t.token),
            "hash": outcome.row.tx_hash, "block": outcome.row.block, "position": outcome.row.position,
            "exec_gas": outcome.row.exec_gas, "stor_gas": outcome.row.stor_gas, "fee": outcome.row.fee,
        }));
    }
    ensure!(blocks.len() == 1, "the {READERS} transactions landed in blocks {blocks:?}, not in one");
    let block = blocks.into_iter().next().unwrap();
    ensure!(live.actor_state(record.actor()).await? == record_before, "the record changed: a read wrote");
    let made: Vec<lee_core::Commitment> = held
        .iter()
        .flat_map(|(_, inner, _)| inner.message.execution.private_actions.iter().map(|a| a.commitment.clone()))
        .collect();
    let (root_after_block, present) = tree(live.wallet.client(), made).await?;
    ensure!(present.iter().all(|p| *p), "a commitment computed before inclusion is not in the tree after the block");
    println!("[block] the {READERS} private reads applied, all in block {block}; {} commitments computed before inclusion are in the tree", present.len());
    summary["block"] = json!({
        "block": block, "sent_after_block": since_head, "send_burst_ms": send_ms as u64, "transactions": rows,
        "record_unchanged": true, "commitments_in_tree": present.len(), "commitment_root_after": root_after_block,
    });

    // 6. The control's second transaction, from the same snapshot as the first.
    let second = live
        .send_raw(
            "control: LEZ's test private account 0, second transaction of the same snapshot",
            control[1].0.clone(),
            format!("AttestPrivately proven against the snapshot, after the first was included; {}", control[1].2),
        )
        .await?;
    ensure!(!second.applied(), "the second transaction of one account from one snapshot was applied");
    println!("[block] control: {} ({})", second.row.status, second.row.note);
    summary["control_second"] =
        json!({ "hash": second.row.tx_hash, "status": second.row.status, "note": second.row.note });

    fs::create_dir_all(&live.opts.out_dir)?;
    fs::write(live.opts.out_dir.join("r11-one-block.json"), serde_json::to_string_pretty(&summary)? + "\n")?;
    live.save()
}
