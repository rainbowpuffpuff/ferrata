//! `seqnet-chain`: the two chains for `ferrata_bridge_module` inside Logos Core, on a running LEZ sequencer and
//! a local anvil (`e2e/seqnet/`, the mini-app's run). The module decides everything that is the bridge's:
//! the commitment and the vault call of a deposit, which imported root a claim goes under and the claim's
//! message and accounts, the burn note and its message, the release rule and journal, the wallet's recovery
//! and every position's state, with its own code (`ferrata-wallet`, the bridge core). This service only reads
//! the chains' state and signs and sends, as Logos Core's own modules would (`eth_rpc_module` and
//! `keystore_module`/`tx_sender_module` on Ethereum, `lez_core` on LEZ), which are not available here:
//!
//! - Ethereum: the vault's public reads (the `ferrata` tool's `eth.rs`), the user's deposit signed with its
//!   keystore, and a release handed to the relayer (`ferrata-relayer`, one round) after the stand-in prover
//!   (the vault tests' mock verifier accepts the exact journal: the Groth16 wrap is not run here);
//! - LEZ: LEZ's wallet library through the tool's own ledger (`ferrata_cli::lez::Sequencer`): actor states,
//!   the zone's applied bridge calls and the calls private transactions make in the clear (`lez::history_of`,
//!   `lez::visible_calls`), the wallet's own private accounts after a sync, and the module's messages sent
//!   as public transactions or as private ones executed and proven by the wallet.
//!
//! The method names and shapes are those of the mini-app devnet's `module.*` methods
//! (`cli/examples/miniapp_devnet.rs`), so the module's code runs over either. HTTP on 127.0.0.1:
//! `POST {"method", "params"}` -> `{"ok": true, "result"}` or `{"ok": false, "error"}`; every call is
//! appended to `--log` (one JSON line: method, params, reply, milliseconds).
//!
//!   seqnet-chain --listen 127.0.0.1:18700 --eth-rpc URL --chain-id N --vault ADDR --from-block N
//!     --keystore FILE --password-file FILE --sequencer URL --seed FILE --lez-wallet DIR --lez-password-file FILE
//!     --bridge-account HEX --bridge-program FILE --relayer-bin FILE --relayer ADDR --relayer-state DIR
//!     --relayer-inbox DIR --mock ADDR --prover ADDR --release-image-id HEX --log FILE [--proofs dev|real]

use std::{
    io::{BufRead as _, BufReader, Read as _, Write as _},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::Command,
    time::Instant,
};

use alloy::{
    network::TransactionBuilder as _,
    primitives::{Address, Bytes, B256, U256},
    providers::Provider as _,
    rpc::types::TransactionRequest,
};
use anyhow::{anyhow, bail, ensure, Context as _, Result};
use borsh::BorshDeserialize as _;
use clap::Parser;
use common::transaction::LeeTransaction;
use ferrata_bridge_core::{inbound, outbound::BurnNote, sha256};
use ferrata_cli::{
    eth::{self, IVault},
    flow, h32, hx,
    lez::{self, Context, Ledger, LezCall, Mention, Sequencer},
    seed::{Seed, DEFAULT_GAP},
};
use ferrata_lez_bridge_wire::{self as bw, config_actor, token_actor, BridgeMessage};
use lee::AccountId;
use lee_core::{account::Actor, program::PdaSeed};
use serde_json::{json, Value};
use wallet::WalletSequencerClient as _;

alloy::sol! {
    #[allow(missing_docs)]
    interface IMock {
        function accept(bytes32 imageId, bytes32 journalDigest) external;
    }
}

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "127.0.0.1:18700")]
    listen: String,
    #[arg(long)]
    eth_rpc: String,
    #[arg(long)]
    chain_id: u64,
    #[arg(long)]
    vault: Address,
    #[arg(long, default_value_t = 0)]
    from_block: u64,
    #[arg(long)]
    keystore: PathBuf,
    #[arg(long)]
    password_file: PathBuf,
    #[arg(long)]
    sequencer: String,
    #[arg(long)]
    seed: PathBuf,
    #[arg(long)]
    lez_wallet: PathBuf,
    #[arg(long)]
    lez_password_file: PathBuf,
    #[arg(long)]
    bridge_account: String,
    #[arg(long)]
    bridge_program: PathBuf,
    #[arg(long)]
    relayer_bin: PathBuf,
    #[arg(long)]
    relayer: Address,
    #[arg(long)]
    relayer_state: PathBuf,
    #[arg(long)]
    relayer_inbox: PathBuf,
    #[arg(long)]
    mock: Address,
    #[arg(long)]
    prover: Address,
    #[arg(long)]
    release_image_id: String,
    #[arg(long)]
    log: PathBuf,
    #[arg(long, default_value = "dev")]
    proofs: String,
}

struct Chain {
    a: Args,
    eth: flow::Eth,
    seq: Sequencer,
    ctx: Context,
    seed: Seed,
    user: Address,
}

fn hex_bytes(v: &Value, what: &str) -> Result<Vec<u8>> {
    hex::decode(v.as_str().with_context(|| format!("{what}: a hex string"))?.trim_start_matches("0x"))
        .with_context(|| format!("{what}: hex"))
}

fn actor_of(v: &Value) -> Result<Actor> {
    let id = |k: &str| -> Result<AccountId> { Ok(AccountId::new(h32(v[k].as_str().context("an account id")?)?)) };
    Ok(Actor::new(id("account")?, id("program")?))
}

fn note_value(n: &BurnNote) -> Value {
    json!({ "r": hx(&n.r), "memo": hx(&n.memo), "mac": hx(&n.mac), "token": hx(&n.token), "amount": hx(&n.amount), "c_out": hx(&n.c_out) })
}

fn token_of(params: &Value) -> Result<[u8; 20]> {
    Ok(match params["token"].as_str() {
        Some(t) => <[u8; 20]>::try_from(hex::decode(t.trim_start_matches("0x"))?.as_slice()).context("token: 20 bytes")?,
        None => [0; 20],
    })
}

impl Chain {
    async fn state_hex(&self, actor: Actor) -> Result<String> {
        Ok(hx(&self.seq.actor_state(actor).await?))
    }

    async fn eth_time(&self) -> Result<u64> {
        eth::now(&self.eth.p).await
    }

    async fn call(&mut self, method: &str, params: &Value) -> Result<Value> {
        match method {
            "module.session" => self.session(),
            "module.info" => self.info().await,
            "module.registry" => self.registry(params).await,
            "module.price" => self.price(params).await,
            "module.deposit_inputs" => self.deposit_inputs().await,
            "module.send_deposit" => self.send_deposit(params).await,
            "module.inputs" => self.inputs(params).await,
            "module.private" => self.private(params).await,
            "module.submit" => self.submit(params).await,
            "module.vault_roots" => self.vault_roots(params).await,
            "module.vault_release" => self.vault_release(params).await,
            "module.stop" => Ok(json!({ "stopping": true })),
            m => bail!("unknown method {m}"),
        }
    }

    /// What the wallet hands the module: its bridge seed (kAuth and kView are derived from it in the module),
    /// its accounts, and the configuration (the vault's `lezConfigId`, the bridge program's account).
    fn session(&self) -> Result<Value> {
        Ok(json!({
            "bridge_seed": hx(&self.seed.bridge_seed()),
            "chain_id": self.eth.chain_id,
            "vault": hx(&self.ctx.vault),
            "config_id": hx(&self.ctx.cid),
            "program": hx(self.ctx.program.value()),
            "depositor": hx(self.user.as_slice()),
            "gap": DEFAULT_GAP,
            "public_accounts": self.seed.public_accounts(DEFAULT_GAP)?.iter().map(|a| hx(a.id.value())).collect::<Vec<_>>(),
            "private_accounts": self.seed.private_account_ids(DEFAULT_GAP)?.iter().map(|a| hx(a.value())).collect::<Vec<_>>(),
            "proofs": self.a.proofs,
            "eth_rpc": self.a.eth_rpc,
            "sequencer": self.a.sequencer,
        }))
    }

    async fn info(&self) -> Result<Value> {
        let head = self.seq.wallet.client().get_last_block_id().await.map_err(|e| anyhow!("get_last_block_id: {e}"))?;
        Ok(json!({
            "eth_block": self.eth.p.get_block_number().await?,
            "eth_time": self.eth_time().await?,
            "eth_balance_wei": self.eth.p.get_balance(self.user).await?.to_string(),
            "lez_head": head,
            "relayer": self.a.relayer.to_string(),
        }))
    }

    /// The vault's side of the registry (the module adds the bridge program's from `token(cid, token)`).
    async fn registry(&self, params: &Value) -> Result<Value> {
        let token = Address::from(token_of(params)?);
        let (p, v) = (&self.eth.p, self.eth.vault);
        let cfg = eth::view(p, v, IVault::tokenConfigCall { token }, None).await?;
        let fee_state = eth::view(p, v, IVault::feeStateCall { token }, None).await?;
        let mut denominations = Vec::new();
        for (d, amount) in cfg.denominations_.iter().enumerate() {
            let f = eth::view(p, v, IVault::feesCall { token, index: U256::from(fee_state.current), denomIndex: d as u8 }, None).await?;
            denominations.push(json!({ "index": d, "amount": amount.to_string(), "mint_fee": f.mintFee.to_string(),
                "relay_fee": f.relayFee.to_string(), "burn_fee": f.burnFee.to_string(), "checkpoint_fee": f.checkpointFee.to_string() }));
        }
        let cap = |c: IVault::tokenCapReturn| json!({"capacity": c.capacity.to_string(), "refill_per_second": c.refillPerSecond.to_string(), "available": c.available.to_string()});
        let deposit_cap = eth::view(p, v, IVault::tokenCapCall { token, direction: 0 }, None).await?;
        let release_cap = eth::view(p, v, IVault::tokenCapCall { token, direction: 1 }, None).await?;
        Ok(json!({
            "token": {
                "token": token.to_string(),
                "registered": cfg.registered,
                "deposits_enabled": cfg.depositsEnabled,
                "lez_unit": cfg.lezUnit.to_string(),
                "fee_schedule": fee_state.current,
                "denominations": denominations,
                "escrowed": eth::view(p, v, IVault::escrowedCall { token }, None).await?.to_string(),
                "vault_deposit_cap": cap(deposit_cap),
                "vault_release_cap": cap(release_cap),
            },
            "vault": {
                "deposits_frozen_until": eth::view(p, v, IVault::depositsFrozenUntilCall {}, None).await?.to_string(),
                "releases_frozen_until": eth::view(p, v, IVault::releasesFrozenUntilCall {}, None).await?.to_string(),
                "retired": eth::view(p, v, IVault::retiredCall {}, None).await?,
                "deposits": eth::view(p, v, IVault::nextLeafIndexCall {}, None).await?,
                "checkpoints": eth::view(p, v, IVault::checkpointCountCall {}, None).await?.to_string(),
            },
        }))
    }

    async fn price(&self, params: &Value) -> Result<Value> {
        let d = params["denomination"].as_u64().unwrap_or(0) as u8;
        let price = eth::deposit_price(&self.eth.p, self.eth.vault, Address::from(token_of(params)?), d).await?;
        Ok(json!({ "denomination": price.denomination.to_string(), "mint_fee": price.mint_fee.to_string(),
                   "relay_fee": price.relay_fee.to_string(), "total": price.total.to_string(), "schedule": price.schedule }))
    }

    async fn deposit_inputs(&self) -> Result<Value> {
        let (p, v) = (&self.eth.p, self.eth.vault);
        Ok(json!({
            "j": eth::view(p, v, IVault::depositCountCall { depositor: self.user }, None).await?,
            "next_leaf": eth::view(p, v, IVault::nextLeafIndexCall {}, None).await?,
        }))
    }

    /// Signs the module's vault call with the user's keystore and sends it (an `eth_call` first).
    async fn send_deposit(&self, params: &Value) -> Result<Value> {
        let to: Address = params["to"].as_str().context("to")?.parse().context("to: an address")?;
        ensure!(to == self.eth.vault, "the deposit's call goes to the vault {}, not {to}", self.eth.vault);
        let data = Bytes::from(hex_bytes(&params["data"], "data")?);
        let value: U256 = params["value"].as_str().context("value")?.parse().context("value: a decimal amount")?;
        let signer = eth::keystore_signer(&self.a.keystore, &self.a.password_file)?;
        let sp = eth::signing_provider(&self.a.eth_rpc, signer)?;
        let receipt = eth::send(&sp, self.user, to, data, value, 1).await?;
        let event = eth::deposit_of_receipt(&receipt).context("the receipt has no Deposit event")?;
        Ok(json!({
            "tx_hash": receipt.transaction_hash.to_string(),
            "block": receipt.block_number,
            "status": receipt.status(),
            "gas_used": receipt.gas_used,
            "leaf_index": event.leafIndex,
            "j": event.index,
            "commitment": event.commitment.to_string(),
            "signed_by": "the user's keystore (stand-in for Logos Core's keystore_module and tx_sender_module)",
        }))
    }

    /// The chains' state a module request needs (the devnet's `module.inputs`): the zone's configuration,
    /// token and root states and its history (imports, published burns, private claims' public part), the
    /// vault's deposit list and released nullifiers, Ethereum's time.
    async fn inputs(&self, params: &Value) -> Result<Value> {
        let program = self.ctx.program;
        let token = token_of(params)?;
        let history = lez::history_of(&self.seq, &self.ctx).await?;
        let (p, v, from, chunk) = (&self.eth.p, self.eth.vault, self.eth.from_block, self.eth.chunk);
        let list = eth::deposit_list(p, v, from, chunk).await?;
        let leaves: Vec<String> = list
            .deposits
            .iter()
            .map(|d| hx(&inbound::deposit_leaf(d.leaf_index, &d.depositor, &d.token, &d.amount, &d.commitment)))
            .collect();
        let deposits: Vec<Value> = list
            .deposits
            .iter()
            .zip(&list.blocks)
            .map(|(d, b)| json!({ "leaf_index": d.leaf_index, "depositor": hx(&d.depositor), "j": d.j, "token": hx(&d.token),
                                  "amount": hx(&d.amount), "commitment": hx(&d.commitment), "block": b }))
            .collect();
        let mut touched: Vec<String> = history.touched_privately.iter().map(|a| hx(a.value())).collect();
        touched.sort();
        let mut out = json!({
            "program": hx(program.value()),
            "config_id": hx(&self.ctx.cid),
            "chain_id": self.ctx.chain_id,
            "vault": hx(&self.ctx.vault),
            "config_state": self.state_hex(config_actor(program, &self.ctx.cid)).await?,
            "token_state": self.state_hex(token_actor(program, &self.ctx.cid, &token)).await?,
            "imported": history.roots,
            "leaves": leaves,
            "deposits": deposits,
            "deposit_root": list.root.to_string(),
            "eth_block": list.block,
            "burns": history.burns.iter().map(|b| json!({ "burn_index": b.burn_index, "note": note_value(&b.note) })).collect::<Vec<_>>(),
            "touched_privately": touched,
            "claimed_plaintext": history.claimed.iter().map(|n| hx(n)).collect::<Vec<_>>(),
            "eth_time": self.eth_time().await?,
            "released_nullifiers": eth::released_nullifiers(p, v, from, chunk).await?.iter().map(|n| hx(n)).collect::<Vec<_>>(),
            "refunded_leaves": eth::refunded_leaves(p, v, from, chunk).await?.into_iter().collect::<Vec<_>>(),
        });
        let n = params["leaf_count"].as_u64().or_else(|| history.roots.last().copied());
        if let Some(n) = n {
            out["leaf_count"] = json!(n);
            out["root_state"] = json!(self.state_hex(bw::root_actor(program, &self.ctx.cid, n)).await?);
        }
        if let Some(nf) = params["nullifier"].as_str() {
            let nf = h32(nf)?;
            out["nullifier_state"] =
                json!(self.state_hex(bw::nullifier_actor(program, self.ctx.chain_id, &self.ctx.vault, &nf)).await?);
        }
        if let Some(holders) = params["public_holders"].as_array() {
            let mut states = Vec::new();
            for h in holders {
                let id = AccountId::new(h32(h.as_str().context("a public holder")?)?);
                states.push(json!({ "account": hx(id.value()), "state": self.state_hex(bw::holder_actor(program, id)).await? }));
            }
            out["public_holder_states"] = json!(states);
        }
        Ok(out)
    }

    /// The wallet's own view of one of its private accounts (LEZ's wallet decrypts it after a sync).
    async fn private(&mut self, params: &Value) -> Result<Value> {
        let id = AccountId::new(h32(params["account"].as_str().context("account")?)?);
        let account = self.seq.private_account(id).await?;
        Ok(match account {
            Some(a) => json!({
                "known": true,
                "native_balance": a.data.native_balance().map(|b| b.to_string()).unwrap_or_default(),
                "bridge_state": hx(a.data.actor_state(self.ctx.program)),
            }),
            None => json!({ "known": false }),
        })
    }

    /// Sends the module's message: a public transaction signed by the wallet, or, when the call's root is
    /// a private account of the wallet, a private transaction executed and proven by LEZ's wallet; then
    /// waits for the block that applies it.
    async fn submit(&mut self, params: &Value) -> Result<Value> {
        let bytes = hex_bytes(&params["message"], "message")?;
        let message = BridgeMessage::try_from_slice(&bytes).context("message: not a BridgeMessage")?;
        let accounts = params["accounts"].as_array().context("accounts")?;
        let root_index = params["root_index"].as_u64().unwrap_or(0) as usize;
        let mut mentions = Vec::new();
        for a in accounts {
            let actor = actor_of(a)?;
            mentions.push(match a["kind"].as_str().unwrap_or("") {
                "pda" => Mention::Pda { actor, seed: PdaSeed::new(h32(a["seed"].as_str().context("seed")?)?) },
                "signer" => Mention::Signer(actor),
                "private_self" => Mention::PrivateSelf(actor),
                "public" => Mention::Other(actor, None),
                k => bail!("account kind {k}"),
            });
        }
        let what = params["what"].as_str().unwrap_or("module call").to_string();
        let private = matches!(mentions.first(), Some(Mention::PrivateSelf(_)));
        let since = self.seq.wallet.client().get_last_block_id().await.map_err(|e| anyhow!("get_last_block_id: {e}"))?;
        let call = LezCall { what: what.clone(), mentions, root_index, message };
        let t0 = Instant::now();
        let hash = self.seq.submit(&call).await?;
        let send_ms = t0.elapsed().as_millis() as u64;
        let (block, reverted, tx) = self.wait_applied(&hash, since).await?;
        let tx_bytes = borsh::to_vec(&tx).map(|b| b.len()).unwrap_or(0);
        // The receipt's size: a private transaction's witness set is (signatures, the proof), Borsh; with no
        // signature it is [0u32][proof length u32][proof].
        let receipt_bytes = match &tx {
            LeeTransaction::PrivacyPreserving(t) => borsh::to_vec(&t.witness_set).ok().and_then(|w| {
                (w.len() >= 8 && w[..4] == [0, 0, 0, 0]).then(|| u32::from_le_bytes([w[4], w[5], w[6], w[7]]) as usize)
            }),
            LeeTransaction::Public(_) => None,
        };
        Ok(json!({
            "settled": !reverted,
            "private": private,
            "what": what,
            "tx_hash": hash,
            "block": block,
            "reverted": reverted,
            "tx_bytes": tx_bytes,
            "proof": if private { json!({ "mode": self.a.proofs, "prove_ms": send_ms, "receipt_bytes": receipt_bytes, "tx_bytes": tx_bytes,
                "by": "LEZ's wallet library (send_privacy_preserving_tx_with_pre_check): executed and proven on this machine, verified by the sequencer" }) } else { Value::Null },
        }))
    }

    async fn wait_applied(&self, hash: &str, since: u64) -> Result<(u64, bool, LeeTransaction)> {
        let client = self.seq.wallet.client();
        let t0 = Instant::now();
        let mut from = since.max(1);
        loop {
            let last = client.get_last_block_id().await.map_err(|e| anyhow!("get_last_block_id: {e}"))?;
            if last >= from {
                for block in client.get_block_range(from, last).await.map_err(|e| anyhow!("get_block_range: {e}"))? {
                    for (i, tx) in block.body.transactions.iter().enumerate() {
                        if tx.hash().to_string() == hash {
                            return Ok((block.header.block_id, block.body.reverted.contains(&(i as u64)), tx.clone()));
                        }
                    }
                }
                from = last + 1;
            }
            ensure!(t0.elapsed().as_secs() < 600, "transaction {hash} not applied within 10 min");
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }

    async fn vault_roots(&self, params: &Value) -> Result<Value> {
        let token = Address::from(token_of(params)?);
        let roots = eth::accepted_roots(&self.eth.p, self.eth.vault, self.eth.from_block, self.eth.chunk).await?;
        let mut out = Vec::new();
        for r in &roots {
            let delay = eth::view(&self.eth.p, self.eth.vault, IVault::releaseDelayForCall { token, checkpointIndex: U256::from(r.index) }, None).await?;
            out.push(json!({ "root": hx(&r.root.0), "count": r.count, "index": r.index, "finalized_at": r.finalized_at,
                             "delay": u64::try_from(delay).unwrap_or(u64::MAX) }));
        }
        Ok(json!({ "roots": out, "now": self.eth_time().await? }))
    }

    /// Hands the module's release to the relayer: the stand-in prover (the mock verifier accepts the exact
    /// journal: the Groth16 wrap is not run here), the job in the relayer's inbox, one round of
    /// `ferrata-relayer`, which checks it and sends `release` from its own account.
    async fn vault_release(&self, params: &Value) -> Result<Value> {
        let rel = &params["release"];
        let journal = hex_bytes(&rel["journal"], "release.journal")?;
        let recipient: Address = Address::from_slice(&hex_bytes(&rel["public"]["recipient"], "release.public.recipient")?);
        let image = B256::from(h32(&self.a.release_image_id)?);
        let data = {
            use alloy::sol_types::SolCall as _;
            IMock::acceptCall { imageId: image, journalDigest: B256::from(sha256(&journal)) }.abi_encode()
        };
        let tx = TransactionRequest::default().with_from(self.a.prover).with_to(self.a.mock).with_input(Bytes::from(data));
        let receipt = self.eth.p.send_transaction(tx).await?.get_receipt().await?;
        ensure!(receipt.status(), "the stand-in prover's accept reverted");
        std::fs::create_dir_all(&self.a.relayer_inbox)?;
        let job = self.a.relayer_inbox.join(format!("release-{}.json", hx(&sha256(&journal))[2..18].to_string()));
        std::fs::write(&job, json!({ "kind": "release", "journal": hx(&journal), "seal": "0x5ea15ea1" }).to_string())?;
        let before = self.eth.p.get_balance(recipient).await?;
        let out = Command::new(&self.a.relayer_bin)
            .args(["--eth-rpc", &self.a.eth_rpc, "--vault", &self.eth.vault.to_string(), "--chain-id", &self.eth.chain_id.to_string(),
                   "--unlocked", &self.a.relayer.to_string(), "--from-block", &self.eth.from_block.to_string()])
            .arg("--state").arg(&self.a.relayer_state)
            .args(["run", "--poll-secs", "1", "--rounds", "1"])
            .arg("--inbox").arg(&self.a.relayer_inbox)
            .output()?;
        let lines = String::from_utf8_lossy(&out.stdout).to_string();
        let events: Vec<Value> = lines.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        let paid = self.eth.p.get_balance(recipient).await? - before;
        let done = events.iter().find(|e| e["event"] == "job-done").cloned().unwrap_or(Value::Null);
        let tx_hash = done["result"]
            .as_object()
            .and_then(|o| o.iter().find(|(k, v)| (k.contains("tx") || k.contains("hash")) && v.is_string()).map(|(_, v)| v.clone()))
            .unwrap_or(Value::Null);
        let mut block = Value::Null;
        if let Some(h) = tx_hash.as_str() {
            if let Ok(h) = h.parse::<B256>() {
                if let Ok(Some(r)) = self.eth.p.get_transaction_receipt(h).await {
                    block = json!(r.block_number);
                }
            }
        }
        Ok(json!({
            "settled": paid > U256::ZERO,
            "tx_hash": tx_hash,
            "block": block,
            "relayer_result": done["result"],
            "recipient": recipient.to_string(),
            "paid_wei": paid.to_string(),
            "sent_by": format!("the relayer {} (ferrata-relayer, one round)", self.a.relayer),
            "relayer_events": events,
            "relayer_exit": out.status.code(),
            "stand_in_prover": { "tx": receipt.transaction_hash.to_string(), "what": "the mock verifier accepts the release journal (no Groth16 wrap here)" },
        }))
    }
}

fn read_request(stream: &mut TcpStream) -> Result<Vec<u8>> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut length = 0usize;
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h)? == 0 || h == "\r\n" || h == "\n" {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                length = v.trim().parse().unwrap_or(0);
            }
        }
    }
    ensure!(length <= 4 << 20, "request too large");
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    Ok(body)
}

fn respond(stream: &mut TcpStream, body: &str) {
    let head = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
    let _ = stream.flush();
}

fn main() -> Result<()> {
    let a = Args::parse();
    let rt = tokio::runtime::Runtime::new()?;
    let mut chain = rt.block_on(async {
        let p = eth::provider(&a.eth_rpc)?;
        let chain_id = p.get_chain_id().await?;
        ensure!(chain_id == a.chain_id, "the Ethereum RPC serves chain {chain_id}, not {}", a.chain_id);
        let eth = flow::Eth { p, vault: a.vault, chain_id, from_block: a.from_block, chunk: 10_000 };
        let seed = Seed::from_file(&a.seed)?;
        let wallet = lez::open_wallet(&a.lez_wallet, &a.sequencer, &a.lez_password_file, &seed)?;
        let program = AccountId::new(h32(&a.bridge_account)?);
        let cid = eth::view(&eth.p, eth.vault, IVault::lezConfigIdCall {}, None).await?;
        let ctx = Context { program, cid: cid.0, chain_id, vault: eth.vault.into_array() };
        let seq = Sequencer { wallet, payer: None, bridge_program: Some(a.bridge_program.clone()), synced: false };
        let user = eth::keystore_signer(&a.keystore, &a.password_file)?.address();
        anyhow::Ok(Chain { a, eth, seq, ctx, seed, user })
    })?;
    let listener = TcpListener::bind(&chain.a.listen)?;
    println!("seqnet-chain: http://{}/ (vault {}, configId {}, user {})", chain.a.listen, chain.eth.vault, hx(&chain.ctx.cid), chain.user);
    let mut log = std::fs::OpenOptions::new().create(true).append(true).open(&chain.a.log)?;
    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };
        let body = match read_request(&mut stream) {
            Ok(b) => b,
            Err(e) => {
                respond(&mut stream, &json!({"ok": false, "error": e.to_string()}).to_string());
                continue;
            }
        };
        let req: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
        let method = req["method"].as_str().unwrap_or("").to_string();
        let t0 = Instant::now();
        let reply = match rt.block_on(chain.call(&method, &req["params"])) {
            Ok(v) => json!({"ok": true, "result": v}),
            Err(e) => json!({"ok": false, "error": format!("{e:#}")}),
        };
        let ms = t0.elapsed().as_millis() as u64;
        let _ = writeln!(log, "{}", json!({"method": method, "params": req["params"], "reply": reply, "ms": ms}));
        respond(&mut stream, &reply.to_string());
        if method == "module.stop" {
            break;
        }
    }
    Ok(())
}
