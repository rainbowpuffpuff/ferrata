//! The private attestation on a sequencer that verifies receipts (`attest-private`).
//!
//! The transaction of `lez/harness/src/bin/prove_attest.rs` (EVIDENCE rows 5 and 32), sent to a
//! running sequencer instead of the in-process harness: the reference consumer, deployed as
//! `consumer`, executed at a private account's actor, reads the record of a finalized mainnet
//! block from an instance of the attestation program (the `main` deployment) and verifies an
//! ERC-20 `Transfer` log of that block against it (`ConsumerMessage::AttestPrivately`); only the
//! `Read` to the record is public. LEZ's wallet executes and proves it
//! (`WalletCore::send_privacy_preserving_tx_with_pre_check`, as `private.rs` does for the
//! private update), for real: `RISC0_DEV_MODE` must be unset.
//!
//! Before it, publicly: the consumer deployed, and an instance of the main deployment initialised
//! from the fixture's bootstrap (`crates/ferrata-spectests/fixtures/mainnet-attest-2026-11-10`,
//! the harness's Stage 4 fixture) that applies the fixture's finality update, so that the record
//! of the finalized block exists. The wallet's private account is LEZ's second test private
//! account (`--private-account 1`, the default here), so that the first stays free for the other
//! private transactions of a session.

use std::{borrow::Cow, fs, path::Path, time::Instant};

use anyhow::{anyhow, bail, ensure, Context, Result};
use ferrata_core::{
    fork::LcFork,
    merkle::ZERO,
    rlp::encode_u64,
    types::{LightClientBootstrap, LightClientHeader, LightClientUpdate, SyncAggregate, SyncCommittee},
    Root,
};
use ferrata_lez_wire::consumer::{ConsumerMessage, Pool};
use ferrata_spectests::execkit::{hexb, receipt_rlp, Trie};
use lee::{privacy_preserving_transaction::circuit::ProgramCatalog, program::Program, AccountId};
use serde_json::Value;
use sha2::{Digest, Sha256};
use wallet::AccountIdentity;

use crate::{
    live::{mention, Live, Sent},
    manifest::Row,
    plan::mainnet_config,
    private::{describe_private, private_wallet_at, TapClient},
    programs::GuestBinary,
    scenario::{apply, check_record, deploy, deployment_accounts, init, Harness, World},
};

/// What the private attestation needs from the fixture.
pub struct AttestFixture {
    pub chain_id: u64,
    pub checkpoint: Root,
    pub bootstrap: LightClientBootstrap,
    pub update: LightClientUpdate,
    pub finalized_slot: u64,
    pub block: u64,
    pub token: [u8; 20],
    pub recipient: [u8; 20],
    pub tx_index: u64,
    pub log_index: u64,
    pub receipts_root: Root,
    pub proof: Vec<Vec<u8>>,
}

pub(crate) fn read(dir: &Path, name: &str) -> Result<Vec<u8>> {
    fs::read(dir.join(name)).with_context(|| format!("reading {}", dir.join(name).display()))
}

/// Every file `SHA256SUMS` names holds its sum.
pub(crate) fn check_sums(dir: &Path) -> Result<()> {
    let sums = String::from_utf8(read(dir, "SHA256SUMS")?)?;
    let mut n = 0;
    for line in sums.lines().filter(|l| !l.trim().is_empty()) {
        let (sum, name) = line.split_once(char::is_whitespace).ok_or_else(|| anyhow!("SHA256SUMS: {line}"))?;
        let name = name.trim().trim_start_matches('*');
        ensure!(hex::encode(Sha256::digest(read(dir, name)?)) == sum, "{name} does not match SHA256SUMS");
        n += 1;
    }
    ensure!(n > 0, "SHA256SUMS names no file");
    Ok(())
}

fn bytes_of<const N: usize>(v: &Value) -> Result<[u8; N]> {
    let s = v.as_str().ok_or_else(|| anyhow!("not a hex string: {v}"))?;
    hexb(s).try_into().map_err(|_| anyhow!("{s}: not {N} bytes"))
}

/// `LightClientFinalityUpdate` (Electra layout) as the `LightClientUpdate` it stands for (as the
/// harness's `attest::finality_update`).
pub(crate) fn finality_update(b: &[u8]) -> Result<LightClientUpdate> {
    ensure!(b.len() > 400, "LightClientFinalityUpdate too short");
    let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as usize;
    let (att, fin) = (u32_at(0), u32_at(4));
    ensure!(att == 400 && b.len() > fin && fin > att, "LightClientFinalityUpdate offsets");
    let decode = |r: &[u8]| LightClientHeader::decode(r, LcFork::Electra).map_err(|e| anyhow!("header: {e:?}"));
    Ok(LightClientUpdate {
        attested_header: decode(&b[att..fin])?,
        next_sync_committee: SyncCommittee::empty(512),
        next_sync_committee_branch: vec![ZERO; LcFork::Electra.sync_committee_branch_len()],
        finalized_header: decode(&b[fin..])?,
        finality_branch: b[8..232].as_chunks::<32>().0.to_vec(),
        sync_aggregate: SyncAggregate::decode(&b[232..392], 512).map_err(|e| anyhow!("sync aggregate: {e:?}"))?,
        signature_slot: u64::from_le_bytes(b[392..400].try_into().unwrap()),
    })
}

impl AttestFixture {
    pub fn load(dir: &Path) -> Result<Self> {
        check_sums(dir)?;
        let manifest: Value = serde_json::from_slice(&read(dir, "fixture.json")?)?;
        let block = manifest["block"].as_u64().ok_or_else(|| anyhow!("fixture.json: block"))?;
        let receipts = serde_json::from_slice::<Value>(&read(dir, &format!("receipts_{block}.json"))?)?["result"]
            .as_array()
            .ok_or_else(|| anyhow!("receipts_{block}.json: no result array"))?
            .clone();
        // The receipts trie, rebuilt from the JSON: key RLP(index), value the receipt's consensus encoding.
        let trie = Trie::new(receipts.iter().enumerate().map(|(i, r)| {
            let (key, len) = encode_u64(i as u64);
            (key[..len].to_vec(), receipt_rlp(r))
        }));
        let t = &manifest["transfer"];
        let tx_index = t["tx_index"].as_u64().ok_or_else(|| anyhow!("fixture.json: transfer.tx_index"))?;
        let (key, len) = encode_u64(tx_index);
        let update = finality_update(&read(dir, "finality_update.ssz")?)?;
        let fx = Self {
            chain_id: manifest["chain_id"].as_u64().ok_or_else(|| anyhow!("fixture.json: chain_id"))?,
            checkpoint: bytes_of(&manifest["bootstrap_root"])?,
            bootstrap: LightClientBootstrap::decode(&read(dir, "bootstrap.ssz")?, LcFork::Electra, 512)
                .map_err(|e| anyhow!("bootstrap: {e:?}"))?,
            finalized_slot: manifest["finalized_slot"].as_u64().ok_or_else(|| anyhow!("fixture.json: finalized_slot"))?,
            block,
            token: bytes_of(&t["token"])?,
            recipient: bytes_of(&t["to"])?,
            tx_index,
            log_index: t["log_index"].as_u64().ok_or_else(|| anyhow!("fixture.json: transfer.log_index"))?,
            receipts_root: trie.root(),
            proof: trie.proof(&key[..len]),
            update,
        };
        ensure!(fx.update.finalized_header.beacon.slot == fx.finalized_slot, "the finality update finalizes another slot");
        let ferrata_core::types::Execution::Payload { header, .. } = &fx.update.finalized_header.execution else {
            bail!("the finalized header carries no execution payload header");
        };
        ensure!(
            header.receipts_root == fx.receipts_root,
            "the receipts rebuilt from the JSON do not hash to the finalized block's receipts root"
        );
        Ok(fx)
    }
}

/// Deploy the consumer, bring the attestation instance to the fixture's finalized block, then
/// the private attestation with a real proof, and record what the sequencer does with it.
pub async fn attest_private(
    live: &mut Live,
    program: &GuestBinary,
    consumer: &GuestBinary,
    fixture_dir: &Path,
    payer: AccountId,
    private_account: usize,
) -> Result<()> {
    ensure!(
        !matches!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1") | Ok("true")),
        "RISC0_DEV_MODE must be unset: the private attestation is proven for real"
    );
    let fx = AttestFixture::load(fixture_dir)?;
    let attest = deployment_accounts("main", program).0;
    let consumer_at = deploy(live, "consumer", consumer, payer).await?;
    println!(
        "[attest] consumer {consumer_at} (image {}); attestation program {attest} (image {}); mainnet block {}, slot {}, Transfer in transaction {} log {}",
        consumer.image_hex(),
        program.image_hex(),
        fx.block,
        fx.finalized_slot,
        fx.tx_index,
        fx.log_index
    );

    let mut world = World::with_checkpoint("attest-mainnet", attest, mainnet_config(2), fx.checkpoint, &fx.bootstrap, LcFork::Electra)?;
    init(live, &mut world, &fx.bootstrap, "attest instance Init", Harness::None, payer).await?;
    let run = apply(live, &mut world, &fx.update, "attest instance finality update", Harness::None, payer).await?;
    ensure!(run.outcome.is_some_and(|o| o.advanced), "the finality update finalizes the block's header");
    check_record(live, &world, fx.finalized_slot).await?;
    println!("[attest] instance {} holds the record of slot {}", hex::encode(world.inst.id), fx.finalized_slot);

    let pool = Pool { ferrata: attest, instance_id: world.inst.id, chain_id: fx.chain_id, token: fx.token, recipient: fx.recipient };
    let message = borsh::to_vec(&ConsumerMessage::AttestPrivately {
        pool,
        slot: fx.finalized_slot,
        tx_index: fx.tx_index,
        log_index: fx.log_index,
        proof: fx.proof.clone(),
        header: None,
    })?;
    let record = world.inst.header(fx.finalized_slot);

    let dir = live.opts.run_dir.join("private-attest");
    fs::create_dir_all(&dir)?;
    let transcript = dir.join("rpc-transcript.jsonl");
    let tx_out = dir.join("transaction.borsh");
    let _ = fs::remove_file(&tx_out);
    let client = TapClient::recording(live.wallet.client_owned(), &transcript, &tx_out)?;
    let (mut wallet, me) = private_wallet_at(&dir.join("wallet"), &live.opts.rpc, client, private_account)?;
    let tip = wallet.start_at_chain_tip().await?;
    let catalog = ProgramCatalog::from([
        (consumer_at, Program::new(Cow::Owned(consumer.bytes.clone()))?),
        (attest, Program::new(Cow::Owned(program.bytes.clone()))?),
    ]);
    let mentions = || vec![AccountIdentity::PrivateOwned(me).select_program_actor_state(consumer_at), mention(record)];
    println!("[attest] AttestPrivately at the private account {me}'s actor of {consumer_at}; wallet synced at block {tip}; a real proof");

    wallet.set_dry_run(true);
    match wallet.send_privacy_preserving_tx_with_pre_check(mentions(), 0, message.clone(), &catalog, None, |_| Ok(())).await {
        #[cfg(feature = "lez-rc2")]
        Err(wallet::ExecutionFailureKind::DryRun(_)) => {}
        #[cfg(not(feature = "lez-rc2"))]
        Err(wallet::ExecutionFailureKind::DryRun) => {}
        Err(error) => bail!("the wallet's dry run failed: {error}: {error:?}"),
        Ok(_) => bail!("a dry run sent a transaction"),
    }
    wallet.set_dry_run(false);

    let step = "private attestation (consumer AttestPrivately), real proof";
    live.rows.push(Row {
        phase: live.phase.clone(),
        step: step.to_string(),
        status: "unfinished".into(),
        note: "the proof was started and had not finished when the run was recorded".into(),
        ..Row::default()
    });
    live.save()?;
    let pending_index = live.rows.len() - 1;
    let started = Instant::now();
    let sent = wallet.send_privacy_preserving_tx_with_pre_check(mentions(), 0, message, &catalog, None, |_| Ok(())).await;
    let seconds = started.elapsed().as_secs_f64();
    let since_head = live.head().await?;
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
            println!("[attest] {step}: rejected: {}", row.note);
            live.rows.push(row);
            live.save()?;
            bail!("the private attestation was not sent");
        }
    };
    let described = describe_private(&tx_out)?;
    println!("[attest] proved and sent in {seconds:.1} s: {described}");
    let outcome = live
        .settle(Sent {
            step: step.to_string(),
            hash,
            sent_at: Instant::now(),
            since_head,
            payer: None,
            balance_before: 0,
            harness: None,
            note: format!("execute_and_prove and submission {seconds:.1} s; {described}"),
        })
        .await?;
    println!("[attest] {step}: {} in block {}", outcome.row.status, outcome.row.block);
    ensure!(outcome.applied(), "the private attestation was not applied: {} {}", outcome.row.status, outcome.row.note);
    check_record(live, &world, fx.finalized_slot).await?;
    live.save()
}
