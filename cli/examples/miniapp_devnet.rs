//! The mini-app's devnet backend: the methods the mini-app calls (the shape `ferrata_bridge_module` is
//! to have: a method name and one JSON value in, one JSON value out), served over HTTP on 127.0.0.1
//! from a local devnet: the v5 vault on anvil (chain id 1, never a public network; the vault tests'
//! `MockRiscZeroVerifier` behind the emergency stop) and the zone in the LEZ harness (LEZ's own state
//! transition with the bridge guest, `RISC0_DEV_MODE=1`). Everything is the `ferrata` tool's library,
//! wired the way `cli/tests/e2e.rs` wires it; this file adds the methods and the HTTP loop.
//!
//! Stand-ins, named in every reply that uses them (`"stand_in"`): the relayer's `Init`, `Register`,
//! `ImportRoot` and `ImportFees` run through the harness's own calls from a synthetic execution block
//! holding the anvil vault's real storage; the checkpoint prover and the Groth16 wrap are the mock
//! verifier accepting the exact journals; the harness settles public LEZ transactions only, so a
//! private claim or burn is planned (the call the LEZ wallet would prove) and not settled here.
//!
//!   FERRATA_DEVNET_PORT=18600 RISC0_DEV_MODE=1 cargo run --example miniapp_devnet
//!   curl -s -d '{"method":"info","params":{}}' http://127.0.0.1:18600/rpc
//!
//! Needs what `cli/tests/e2e.sh` needs: anvil and forge on the path, `vault/lib` installed and built.

use std::{
    collections::HashMap,
    io::{BufRead as _, BufReader, Read as _, Write as _},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use alloy::{
    network::TransactionBuilder as _,
    primitives::{Address, Bytes, B256, U256},
    providers::{DynProvider, Provider},
    rpc::types::TransactionRequest,
    signers::local::PrivateKeySigner,
    sol,
    sol_types::SolCall,
};
use anyhow::{anyhow, bail, ensure, Context as _, Result};
use borsh::BorshDeserialize as _;
use ferrata_bridge_core::{
    chain_id_u256,
    inbound::{self, DestKind},
    outbound::burn_leaf,
    sha256,
    tree::Tree,
    Hash,
};
use ferrata_cli::{
    codes, eth,
    eth::IVault,
    hx,
    lez::{self, Applied, Context, Ledger, LezCall, Mention},
    release, scan,
    seed::Seed,
};
use ferrata_lez_bridge_wire::{fee_slot, registry_slot, token_actor, BridgeMessage, CapParams, FeeSchedule};
use harness::{bridge as hb, TxReport};
use lee::{AccountId, PrivateKey, PublicAccountEvidence};
use lee_core::account::Actor;
use serde_json::{json, Value};

sol! {
    #[allow(missing_docs)]
    interface IMock {
        function accept(bytes32 imageId, bytes32 journalDigest) external;
    }
    #[allow(missing_docs)]
    interface ICheckpoints {
        struct Checkpoint {
            uint32 nMin; uint32 tMin; bytes32 parentState; bytes32 newState; bytes32 burnRoot; uint64 burnCount;
            uint64 importedLeafCount; bool sunset; bytes32 echoNonce; uint64 tipId; bytes32 tipHash; uint32 n; uint32 t;
        }
        function submitCandidate(Checkpoint c, bytes seal) external;
        function recordBeacon() external returns (uint256);
        function beacons(uint256 i) external view returns (uint256 timestamp, bytes32 nonce);
        function beaconCount() external view returns (uint256);
        function finalize(uint256 beaconIndex, Checkpoint c, bytes seal) external;
        function finalState() external view returns (bytes32);
        function checkpointImageId() external view returns (bytes32);
        function releaseImageId() external view returns (bytes32);
    }
}

/// The devnet user's BIP-39 phrase: the published test phrase, never a wallet.
const USER: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
/// anvil's first default account (unlocked on anvil): the deployer and the stand-in prover.
const ANVIL0: Address = alloy::primitives::address!("f39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
const SEAL: [u8; 4] = [0x5e, 0xa1, 0x5e, 0xa1];
const ETH_TOKEN: [u8; 20] = [0; 20];
/// The recovery window the mini-app scans (the CLI's default `--gap`).
const GAP: u32 = 8;

// ---- the zone: the harness behind the tool's `Ledger` (as cli/tests/e2e.rs) ----------------------

struct HarnessLedger {
    world: hb::World,
    applied: Vec<Applied>,
    keys: HashMap<AccountId, PrivateKey>,
}

impl HarnessLedger {
    fn record(&mut self, report: &TxReport) {
        if !report.ok() {
            return;
        }
        let tx =
            common::transaction::LeeTransaction::try_from_slice(&self.world.chain.last_tx).expect("a LEZ transaction");
        if let common::transaction::LeeTransaction::Public(tx) = tx {
            let root = &tx.message.execution.root;
            self.applied.push(Applied { to: root.to, message: root.message.clone() });
        }
    }

    fn settle(&mut self, call: &LezCall) -> TxReport {
        let actors: Vec<Actor> = call.mentions.iter().map(Mention::actor).collect();
        let root = actors[call.root_index];
        let mut evidence = Vec::new();
        if let Mention::Pda { actor, seed } = &call.mentions[call.root_index] {
            if self.world.chain.state.get_account_by_id_ref(actor.account_id).is_none() {
                evidence.push(PublicAccountEvidence::Pda { program: actor.program_account_id, seed: *seed });
            }
        }
        for m in &call.mentions {
            if let Mention::Other(actor, Some(key)) = m {
                if self.world.chain.state.get_account_by_id_ref(actor.account_id).is_none() {
                    evidence.push(PublicAccountEvidence::Key(key.clone()));
                }
            }
        }
        let signers: Vec<PrivateKey> = call
            .mentions
            .iter()
            .filter_map(|m| match m {
                Mention::Signer(a) => self.keys.get(&a.account_id).cloned(),
                _ => None,
            })
            .collect();
        let refs: Vec<&PrivateKey> = signers.iter().collect();
        let report = self.world.submit_signed(&call.what, root, &call.message, actors, evidence, &refs);
        self.record(&report);
        report
    }
}

impl Ledger for HarnessLedger {
    async fn applied(&self, program: AccountId) -> Result<Vec<Applied>> {
        Ok(self.applied.iter().filter(|a| a.to.program_account_id == program).cloned().collect())
    }

    async fn actor_state(&self, actor: Actor) -> Result<Vec<u8>> {
        Ok(self.world.chain.actor_state(actor).to_vec())
    }

    async fn submit(&mut self, call: &LezCall) -> Result<String> {
        let report = self.settle(call);
        match (report.ok(), report.exit_code) {
            (true, _) => Ok(format!("{} (harness, block {})", call.what, self.world.chain.block_id - 1)),
            (false, Some(code)) => Err(anyhow!(codes::program_refusal(code, "returned by the program in the harness"))),
            (false, None) => Err(anyhow!("reverted without an exit code: {:?}", report.charge.revert)),
        }
    }
}

// ---- Ethereum: anvil and the vault -----------------------------------------------------------------

struct Anvil(Child);

impl Drop for Anvil {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

async fn wait_for(rpc: &str) -> Result<DynProvider> {
    let p = eth::provider(rpc)?;
    for _ in 0..100 {
        if p.get_block_number().await.is_ok() {
            return Ok(p);
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    bail!("anvil did not start")
}

async fn anvil_send(p: &DynProvider, to: Address, input: Vec<u8>, value: U256) -> Result<()> {
    let tx = TransactionRequest::default().with_from(ANVIL0).with_to(to).with_input(input).with_value(value);
    let receipt = p.send_transaction(tx).await?.get_receipt().await?;
    ensure!(receipt.status(), "a stand-in transaction reverted");
    Ok(())
}

async fn slot(p: &DynProvider, vault: Address, key: [u8; 32]) -> Result<[u8; 32]> {
    Ok(p.get_storage_at(vault, U256::from_be_bytes(key)).await?.to_be_bytes::<32>())
}

fn word(n: u64) -> [u8; 32] {
    let mut w = [0u8; 32];
    w[24..].copy_from_slice(&n.to_be_bytes());
    w
}

fn u128_of(w: &[u8; 32]) -> u128 {
    u128::from_be_bytes(w[16..].try_into().expect("16 bytes"))
}

fn u256_str(v: U256) -> String {
    v.to_string()
}

/// The checkpoint journal the vault rebuilds (VaultTestBase._checkpointJournalBytes).
fn checkpoint_journal(chain_id: u64, vault: Address, cid: B256, c: &ICheckpoints::Checkpoint) -> Vec<u8> {
    let mut j = Vec::new();
    j.extend_from_slice(&sha256(b"ferrata/vault/checkpoint/v3"));
    j.extend_from_slice(&word(chain_id));
    j.extend_from_slice(&[[0u8; 12].as_slice(), vault.as_slice()].concat());
    j.extend_from_slice(cid.as_slice());
    j.extend_from_slice(&word(u64::from(c.nMin)));
    j.extend_from_slice(&word(u64::from(c.tMin)));
    for h in [c.parentState, c.newState, c.burnRoot] {
        j.extend_from_slice(h.as_slice());
    }
    j.extend_from_slice(&word(c.burnCount));
    j.extend_from_slice(&word(c.importedLeafCount));
    j.extend_from_slice(&word(u64::from(c.sunset)));
    j.extend_from_slice(c.echoNonce.as_slice());
    j.extend_from_slice(&word(c.tipId));
    j.extend_from_slice(c.tipHash.as_slice());
    j.extend_from_slice(&word(u64::from(c.n)));
    j.extend_from_slice(&word(u64::from(c.t)));
    j
}

// ---- the devnet ------------------------------------------------------------------------------------

struct Devnet {
    _anvil: Anvil,
    p: DynProvider,
    rpc: String,
    vault: Address,
    verifier: Address,
    cid: Hash,
    ctx: Context,
    zone: HarnessLedger,
    user: Seed,
    user_ks: PathBuf,
    user_addr: Address,
    relayer_ks: PathBuf,
    relayer_addr: Address,
    password: PathBuf,
    /// Execution blocks added to the zone (the stand-in relayer's), for the next block's numbers.
    eth_blocks: u64,
    checkpoints: u64,
}

impl Devnet {
    async fn start(dir: &Path, anvil_port: u16) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let rpc = format!("http://127.0.0.1:{anvil_port}");
        let anvil = Anvil(
            Command::new("anvil")
                .args(["--port", &anvil_port.to_string(), "--chain-id", "1", "--silent", "--code-size-limit", "100000"])
                .stdout(Stdio::null())
                .spawn()
                .context("starting anvil")?,
        );
        let p = wait_for(&rpc).await?;
        let mut zone = HarnessLedger { world: hb::World::new(), applied: Vec::new(), keys: HashMap::new() };
        let program = zone.world.program;
        let vault = ANVIL0.create(3);
        let config = hb::config(vault.into_array(), hb::controls());
        let cid = config.id(program);
        let out = Command::new("forge")
            .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../vault"))
            .args([
                "script",
                "script/CliLocal.s.sol:CliLocal",
                "--rpc-url",
                &rpc,
                "--broadcast",
                "--unlocked",
                "--sender",
            ])
            .arg(ANVIL0.to_string())
            .env("FERRATA_CID", hx(&cid))
            .output()?;
        ensure!(out.status.success(), "forge script: {}", String::from_utf8_lossy(&out.stderr));
        ensure!(!p.get_code_at(vault).await?.is_empty(), "the vault is not at the predicted address");
        let verifier = ANVIL0.create(1);
        let registered_at = p.get_block(alloy::eips::BlockId::latest()).await?.expect("a block").header.timestamp;

        // The relayer's part on LEZ (stand-in): Init and Register from the vault's own storage.
        let mut storage = Vec::new();
        for key in [word(2), registry_slot(&ETH_TOKEN), fee_slot(&ETH_TOKEN, 0)] {
            storage.push((key, slot(&p, vault, key).await?));
        }
        ensure!(storage[0].1 == cid, "the vault's slot 2 is not the configuration's id");
        let b0 = zone.world.add_block(hb::Block::with_storage(9_000_000, 9_000_000, vault.into_array(), &storage));
        let r = zone.world.init("Init (relayer)", &config, b0);
        ensure!(r.ok(), "Init: {:?}", r.exit_code);
        zone.record(&r);
        let schedule = FeeSchedule {
            index: 0,
            from_leaf: 0,
            announced_at: registered_at,
            effective_at: registered_at,
            mint_fees: vec![0; 3],
            burn_fees: vec![0; 3],
            checkpoint_fees: vec![0; 3],
        };
        let r = zone.world.register_with(
            "Register ETH (relayer)",
            &cid,
            b0,
            &hb::eth_entry(),
            &schedule,
            &vault.into_array(),
        );
        ensure!(r.ok(), "Register: {:?}", r.exit_code);
        zone.record(&r);

        // The user's depositor key and a relayer's, in keystore files, funded by anvil's account.
        let password = dir.join("password.txt");
        std::fs::write(&password, "devnet test password\n")?;
        let mut rng = rand::thread_rng();
        let mut made = Vec::new();
        for name in ["user", "relayer"] {
            let file = format!("keystore-{name}.json");
            let _ = std::fs::remove_file(dir.join(&file));
            let (signer, _) = PrivateKeySigner::new_keystore(dir, &mut rng, "devnet test password", Some(&file))?;
            anvil_send(&p, signer.address(), vec![], U256::from(100u64) * U256::from(10u64).pow(U256::from(18u64)))
                .await?;
            made.push((dir.join(&file), signer.address()));
        }
        let user = Seed::from_phrase(USER)?;
        for a in user.public_accounts(GAP)? {
            zone.keys.insert(a.id, a.key.clone());
        }
        let ctx = Context { program, cid, chain_id: 1, vault: vault.into_array() };
        Ok(Self {
            _anvil: anvil,
            p,
            rpc,
            vault,
            verifier,
            cid,
            ctx,
            zone,
            user,
            user_ks: made[0].0.clone(),
            user_addr: made[0].1,
            relayer_ks: made[1].0.clone(),
            relayer_addr: made[1].1,
            password,
            eth_blocks: 1,
            checkpoints: 0,
        })
    }

    async fn history(&self) -> Result<lez::History> {
        Ok(lez::history(&self.ctx, &self.zone.applied(self.ctx.program).await?))
    }

    async fn eth_time(&self) -> Result<u64> {
        Ok(self.p.get_block(alloy::eips::BlockId::latest()).await?.expect("a block").header.timestamp)
    }

    async fn call(&mut self, method: &str, params: &Value) -> Result<Value> {
        match method {
            "info" => self.info().await,
            "registry" => self.registry().await,
            "price" => self.price(params).await,
            "deposit" => self.deposit(params).await,
            "relay" => self.relay().await,
            "scan" => self.scan().await,
            "claim" => self.claim(params).await,
            "burn" => self.burn(params).await,
            "checkpoint" => self.checkpoint().await,
            "release" => self.release(params).await,
            "advance_time" => self.advance_time(params).await,
            "set_endpoints" => self.set_endpoints(params).await,
            "admin.status" => self.admin_status().await,
            "admin.set_cap" => self.admin_set_cap(params).await,
            "admin.freeze" => self.admin_freeze(params).await,
            other => bail!("unknown method {other}"),
        }
    }

    async fn info(&self) -> Result<Value> {
        let publics: Vec<String> = self.user.public_accounts(GAP)?.iter().map(|a| hx(a.id.value())).collect();
        let privates: Vec<String> = self.user.private_account_ids(GAP)?.iter().map(|a| hx(a.value())).collect();
        Ok(json!({
            "network": "devnet: anvil (chain id 1, local) + the LEZ harness (in-process, RISC0_DEV_MODE)",
            "eth_rpc": self.rpc,
            "sequencer": "in-process LEZ harness (no sequencer RPC)",
            "chain_id": 1,
            "vault": self.vault.to_string(),
            "config_id": hx(&self.cid),
            "program": hx(self.ctx.program.value()),
            "eth_block": self.p.get_block_number().await?,
            "eth_time": self.eth_time().await?,
            "user": {
                "depositor": self.user_addr.to_string(),
                "eth_balance_wei": u256_str(self.p.get_balance(self.user_addr).await?),
                "public_accounts": publics,
                "private_accounts": privates,
            },
            "relayer": self.relayer_addr.to_string(),
            "stand_in": "the vault's verifier is the mock (no real proof); the zone is the harness",
        }))
    }

    async fn registry(&self) -> Result<Value> {
        let token = eth::address(ETH_TOKEN);
        let cfg = eth::view(&self.p, self.vault, IVault::tokenConfigCall { token }, None).await?;
        let fee_state = eth::view(&self.p, self.vault, IVault::feeStateCall { token }, None).await?;
        let mut denominations = Vec::new();
        for (d, amount) in cfg.denominations_.iter().enumerate() {
            let f = eth::view(
                &self.p,
                self.vault,
                IVault::feesCall { token, index: U256::from(fee_state.current), denomIndex: d as u8 },
                None,
            )
            .await?;
            denominations.push(json!({
                "index": d,
                "amount": u256_str(*amount),
                "mint_fee": u256_str(f.mintFee),
                "relay_fee": u256_str(f.relayFee),
                "burn_fee": u256_str(f.burnFee),
                "checkpoint_fee": u256_str(f.checkpointFee),
            }));
        }
        let cap = |c: IVault::tokenCapReturn| json!({"capacity": u256_str(c.capacity), "refill_per_second": u256_str(c.refillPerSecond), "available": u256_str(c.available)});
        let deposit_cap = eth::view(&self.p, self.vault, IVault::tokenCapCall { token, direction: 0 }, None).await?;
        let release_cap = eth::view(&self.p, self.vault, IVault::tokenCapCall { token, direction: 1 }, None).await?;
        let escrowed = eth::view(&self.p, self.vault, IVault::escrowedCall { token }, None).await?;
        let lez = self.zone.world.token_state(&self.cid, &ETH_TOKEN).map(|t| {
            json!({
                "now_ms": t.gate.now_ms,
                "mints_frozen_until_ms": t.gate.mints_until,
                "burns_frozen_until_ms": t.gate.burns_until,
                "mint_cap": {"capacity": t.gate.mint.capacity.to_string(), "refill_per_second": t.gate.mint.refill_per_second.to_string(), "level": t.gate.mint.level.to_string()},
                "burn_cap": {"capacity": t.gate.burn.capacity.to_string(), "refill_per_second": t.gate.burn.refill_per_second.to_string(), "level": t.gate.burn.level.to_string()},
                "known_until_leaf": t.known_until,
                "fee_schedules": t.schedules.len(),
            })
        });
        Ok(json!({
            "tokens": [{
                "symbol": "ETH",
                "token": token.to_string(),
                "decimals": 18,
                "registered": cfg.registered,
                "deposits_enabled": cfg.depositsEnabled,
                "lez_unit": u256_str(cfg.lezUnit),
                "fee_schedule": fee_state.current,
                "denominations": denominations,
                "escrowed": u256_str(escrowed),
                "vault_deposit_cap": cap(deposit_cap),
                "vault_release_cap": cap(release_cap),
                "lez": lez,
            }],
            "vault": {
                "deposits_frozen_until": u256_str(eth::view(&self.p, self.vault, IVault::depositsFrozenUntilCall {}, None).await?),
                "releases_frozen_until": u256_str(eth::view(&self.p, self.vault, IVault::releasesFrozenUntilCall {}, None).await?),
                "retired": eth::view(&self.p, self.vault, IVault::retiredCall {}, None).await?,
                "deposits": eth::view(&self.p, self.vault, IVault::nextLeafIndexCall {}, None).await?,
                "checkpoints": u256_str(eth::view(&self.p, self.vault, IVault::checkpointCountCall {}, None).await?),
            },
        }))
    }

    async fn price(&self, params: &Value) -> Result<Value> {
        let denomination = params["denomination"].as_u64().unwrap_or(0) as u8;
        let price = eth::deposit_price(&self.p, self.vault, eth::address(ETH_TOKEN), denomination).await?;
        Ok(json!({
            "denomination": price.denomination.to_string(),
            "mint_fee": price.mint_fee.to_string(),
            "relay_fee": price.relay_fee.to_string(),
            "total": price.total.to_string(),
            "schedule": price.schedule,
        }))
    }

    async fn deposit(&mut self, params: &Value) -> Result<Value> {
        let denomination = params["denomination"].as_u64().unwrap_or(0) as u8;
        let kind = match params["kind"].as_str().unwrap_or("private") {
            "private" => DestKind::Private,
            "public" => DestKind::Public,
            k => bail!("kind {k}: private or public"),
        };
        let a = params["account"].as_u64().unwrap_or(0) as u32;
        ensure!(a < GAP, "account /{a} is outside the recovery window ({GAP}): a wallet would not find it");
        let token = eth::address(ETH_TOKEN);
        let price = eth::deposit_price(&self.p, self.vault, token, denomination).await?;
        let j = eth::view(&self.p, self.vault, IVault::depositCountCall { depositor: self.user_addr }, None).await?;
        let keys = self.user.bridge_keys();
        let dests = self.user.destinations(GAP)?;
        let s = inbound::deposit_secret(
            &keys.auth,
            &chain_id_u256(1),
            &self.vault.into_array(),
            &self.user_addr.into_array(),
            j,
        );
        let commitment = inbound::commitment(&s, kind, &dests.of(kind, a));
        let leaf = eth::view(&self.p, self.vault, IVault::nextLeafIndexCall {}, None).await?;
        let input = eth::deposit_calldata(token, denomination, B256::from(commitment), j);
        let signer = eth::keystore_signer(&self.user_ks, &self.password)?;
        let sp = eth::signing_provider(&self.rpc, signer)?;
        let receipt = eth::send(&sp, self.user_addr, self.vault, input, U256::from(price.total), 1).await?;
        Ok(json!({
            "tx_hash": receipt.transaction_hash.to_string(),
            "block": receipt.block_number,
            "leaf_index": leaf,
            "j": j,
            "amount": price.denomination.to_string(),
            "total_paid": price.total.to_string(),
            "commitment": hx(&commitment),
            "kind": if kind == DestKind::Private { "private" } else { "public" },
            "account": a,
            "public_on_ethereum": ["the depositor address", "the token", "the denomination and fees", "the time", "the commitment C"],
            "hidden": ["the LEZ destination (C commits to it)"],
        }))
    }

    /// The stand-in relayer: imports the vault's current root on the zone (and the fee range).
    async fn relay(&mut self) -> Result<Value> {
        let n = eth::view(&self.p, self.vault, IVault::nextLeafIndexCall {}, None).await?;
        let root = eth::view(&self.p, self.vault, IVault::depositRootCall {}, None).await?;
        let mut storage = vec![
            (word(0), slot(&self.p, self.vault, word(0)).await?),
            (word(1), slot(&self.p, self.vault, word(1)).await?),
        ];
        storage.push((word(2), self.cid));
        ensure!(storage[0].1 == root.0, "slot 0 is depositRoot");
        let at = 9_000_000 + 64 * self.eth_blocks;
        self.eth_blocks += 1;
        let b = self.zone.world.add_block(hb::Block::with_storage(at, at, self.vault.into_array(), &storage));
        let r = self.zone.world.import("ImportRoot (relayer)", &self.cid, b, root.0, n, &self.vault.into_array());
        self.zone.record(&r);
        ensure!(r.ok(), "{}", codes::program_refusal(r.exit_code.unwrap_or(0), "ImportRoot, returned by the program"));
        let f = self.zone.world.import_fees(
            "ImportFees ETH 1 = none (relayer)",
            &self.cid,
            b,
            ETH_TOKEN,
            1,
            None,
            &self.vault.into_array(),
        );
        self.zone.record(&f);
        Ok(json!({
            "imported_leaf_count": n,
            "root": hx(&root.0),
            "fees_known": f.ok(),
            "fees_code": f.exit_code,
            "stand_in": "the relayer's ImportRoot from a synthetic execution block holding the vault's real storage",
        }))
    }

    async fn scan(&self) -> Result<Value> {
        let list = eth::deposit_list(&self.p, self.vault, 0, 10_000).await?;
        let history = self.history().await?;
        let refunded = eth::refunded_leaves(&self.p, self.vault, 0, 10_000).await?;
        let rows = scan::deposits(&self.user, &self.ctx, &list, GAP, &history, &refunded)?;
        let deposits: Vec<Value> = rows
            .iter()
            .map(|r| {
                let state = match &r.state {
                    scan::DepositState::Claimed => "claimed".to_string(),
                    scan::DepositState::Refunded => "refunded".to_string(),
                    scan::DepositState::Claimable { .. } => "claimable".to_string(),
                    scan::DepositState::NotImported => "waiting for the root import".to_string(),
                    #[allow(unreachable_patterns)]
                    other => format!("{other:?}"),
                };
                json!({
                    "leaf_index": r.owned.deposit.leaf_index,
                    "j": r.owned.deposit.j,
                    "amount": u128_of(&r.owned.deposit.amount).to_string(),
                    "kind": if r.owned.kind == DestKind::Private { "private" } else { "public" },
                    "account": r.owned.account_index,
                    "state": state,
                })
            })
            .collect();
        let released = eth::released_nullifiers(&self.p, self.vault, 0, 10_000).await?;
        let (burn_rows, next_i) = scan::burns(&self.user, &self.ctx, &history, &released);
        let burns: Vec<Value> = burn_rows
            .iter()
            .map(|b| {
                json!({
                    "burn_index": b.owned.burn.burn_index,
                    "i": b.owned.i,
                    "amount": u128_of(&b.owned.burn.note.amount).to_string(),
                    "recipient": hx(&b.owned.recipient),
                    "released": b.released,
                })
            })
            .collect();
        let mut holdings = Vec::new();
        for a in self.user.public_accounts(GAP)? {
            let bal = self.zone.world.balance(a.id, &self.cid);
            if bal > 0 {
                holdings.push(json!({"kind": "public", "account": a.index, "balance": bal.to_string()}));
            }
        }
        Ok(json!({
            "deposits": deposits,
            "burns": burns,
            "next_burn_counter": next_i,
            "holdings": holdings,
            "vault_deposits": list.deposits.len(),
            "imported_roots": history.roots,
            "how": "the whole deposit list from the RPC, checked against depositRoot, and the zone's applied calls: no per-item query",
        }))
    }

    async fn claim(&mut self, params: &Value) -> Result<Value> {
        let leaf = params["leaf_index"].as_u64().context("leaf_index")?;
        let private = params["private"].as_bool().unwrap_or(true);
        let list = eth::deposit_list(&self.p, self.vault, 0, 10_000).await?;
        let history = self.history().await?;
        let refunded = eth::refunded_leaves(&self.p, self.vault, 0, 10_000).await?;
        let rows = scan::deposits(&self.user, &self.ctx, &list, GAP, &history, &refunded)?;
        let row = rows.iter().find(|r| r.owned.deposit.leaf_index == leaf).context("not a deposit of this wallet")?;
        let leaves: Vec<Hash> = list
            .deposits
            .iter()
            .map(|d| inbound::deposit_leaf(d.leaf_index, &d.depositor, &d.token, &d.amount, &d.commitment))
            .collect();
        let plan = lez::plan_claim(&self.zone, &self.ctx, &leaves, &row.owned, &history, private, None).await?;
        if private {
            return Ok(json!({
                "settled": false,
                "planned": true,
                "private": true,
                "what": plan.call.what,
                "nullifier": hx(&plan.nullifier),
                "root_leaf_count": plan.leaf_count,
                "public_on_lez": ["the nullifier N (its account)", "the denomination", "the root read"],
                "hidden": ["the destination account", "the leaf index", "the depositor"],
                "not_settled_because": "this devnet's zone is the LEZ harness, which settles public transactions only: the private claim is the call the LEZ wallet proves and submits to a sequencer",
            }));
        }
        let sent = self.zone.submit(&plan.call).await?;
        let holder = AccountId::new(row.owned.dest);
        Ok(json!({
            "settled": true,
            "private": false,
            "what": sent,
            "nullifier": hx(&plan.nullifier),
            "root_leaf_count": plan.leaf_count,
            "balance": self.zone.world.balance(holder, &self.cid).to_string(),
            "public_on_lez": ["the nullifier N", "the denomination", "the destination account and its new balance", "the root read"],
        }))
    }

    async fn burn(&mut self, params: &Value) -> Result<Value> {
        if params["private"].as_bool().unwrap_or(false) {
            return Ok(json!({
                "settled": false,
                "planned": false,
                "private": true,
                "not_settled_because": "this devnet's holdings are public accounts (claimed in plaintext): the harness settles public transactions only, and a private holder's burn is proven by the LEZ wallet on a sequencer",
            }));
        }
        let a = params["account"].as_u64().unwrap_or(0) as u32;
        let amount: u128 = params["amount"].as_str().context("amount (base units, a string)")?.parse()?;
        let recipient: Address = match params["recipient"].as_str() {
            Some(s) if !s.is_empty() => s.parse().context("recipient: an Ethereum address")?,
            _ => self.user_addr,
        };
        let holder = self.user.public_accounts(GAP)?.into_iter().find(|x| x.index == a).context("account")?.id;
        let history = self.history().await?;
        let plan = lez::plan_burn(
            &self.zone,
            &self.ctx,
            &self.user.bridge_keys(),
            holder,
            ETH_TOKEN,
            amount,
            recipient.into_array(),
            &history,
        )
        .await?;
        let sent = self.zone.submit(&plan.call).await?;
        Ok(json!({
            "settled": true,
            "what": sent,
            "i": plan.i,
            "amount": amount.to_string(),
            "recipient": recipient.to_string(),
            "balance": self.zone.world.balance(holder, &self.cid).to_string(),
            "public_on_lez": ["the holder account (a public burn)", "the token and denomination", "the note: r, c_out (the recipient encrypted)"],
        }))
    }

    /// The stand-in checkpoint prover and relayer: candidate, wait, beacon, finalisation of a burn root
    /// over every published burn, then the token's release delay.
    async fn checkpoint(&mut self) -> Result<Value> {
        let history = self.history().await?;
        ensure!(!history.burns.is_empty(), "no burn to checkpoint yet");
        let burn_leaves: Vec<Hash> = history
            .burns
            .iter()
            .map(|b| burn_leaf(&self.cid, b.burn_index, &b.note.token, &b.note.amount, &b.note.c_out, &b.note.r))
            .collect();
        let count = burn_leaves.len() as u64;
        let burn_root = Tree::from_leaves(burn_leaves).map_err(|_| anyhow!("tree"))?.root();
        let (p, vault, cid) = (&self.p, self.vault, B256::from(self.cid));
        let chain_id = p.get_chain_id().await?;
        let mock_image = eth::view(p, vault, ICheckpoints::checkpointImageIdCall {}, None).await?;
        let parent = eth::view(p, vault, ICheckpoints::finalStateCall {}, None).await?;
        self.checkpoints += 1;
        let mut c = ICheckpoints::Checkpoint {
            nMin: 3,
            tMin: 2,
            parentState: parent,
            newState: B256::from(sha256(&[b"miniapp devnet state".as_slice(), &burn_root].concat())),
            burnRoot: B256::from(burn_root),
            burnCount: count,
            importedLeafCount: 0,
            sunset: false,
            echoNonce: B256::ZERO,
            tipId: 1000 + self.checkpoints,
            tipHash: B256::from(sha256(&[b"miniapp devnet tip".as_slice(), &burn_root].concat())),
            n: 4,
            t: 3,
        };
        let accept =
            |j: &[u8]| IMock::acceptCall { imageId: mock_image, journalDigest: B256::from(sha256(j)) }.abi_encode();
        anvil_send(p, self.verifier, accept(&checkpoint_journal(chain_id, vault, cid, &c)), U256::ZERO).await?;
        anvil_send(
            p,
            vault,
            ICheckpoints::submitCandidateCall { c: c.clone(), seal: Bytes::from(SEAL.to_vec()) }.abi_encode(),
            U256::ZERO,
        )
        .await?;
        p.raw_request::<_, Value>("evm_increaseTime".into(), (700u64,)).await?;
        p.raw_request::<_, Value>("evm_mine".into(), Vec::<u64>::new()).await?;
        anvil_send(p, vault, ICheckpoints::recordBeaconCall {}.abi_encode(), U256::ZERO).await?;
        let index = eth::view(p, vault, ICheckpoints::beaconCountCall {}, None).await? - U256::from(1);
        let beacon = eth::view(p, vault, ICheckpoints::beaconsCall { i: index }, None).await?;
        c.echoNonce = beacon.nonce;
        anvil_send(p, self.verifier, accept(&checkpoint_journal(chain_id, vault, cid, &c)), U256::ZERO).await?;
        anvil_send(
            p,
            vault,
            ICheckpoints::finalizeCall { beaconIndex: index, c, seal: Bytes::from(SEAL.to_vec()) }.abi_encode(),
            U256::ZERO,
        )
        .await?;
        p.raw_request::<_, Value>("evm_increaseTime".into(), (200u64,)).await?;
        p.raw_request::<_, Value>("evm_mine".into(), Vec::<u64>::new()).await?;
        Ok(json!({
            "burn_root": hx(&burn_root),
            "burn_count": count,
            "stand_in": "the checkpoint proof and its Groth16 wrap: the mock verifier accepts the exact journal; anvil's clock is moved past the challenge window and the release delay",
        }))
    }

    async fn release(&mut self, params: &Value) -> Result<Value> {
        let burn_index = params["burn_index"].as_u64().context("burn_index")?;
        let history = self.history().await?;
        let released = eth::released_nullifiers(&self.p, self.vault, 0, 10_000).await?;
        let (rows, _) = scan::burns(&self.user, &self.ctx, &history, &released);
        let row = rows.iter().find(|b| b.owned.burn.burn_index == burn_index).context("not a burn of this wallet")?;
        ensure!(
            !row.released,
            "already released: the vault's NullifierAlreadyUsed for this burn's NOut (checked before sending)"
        );
        let roots = eth::accepted_burn_roots(&self.p, self.vault, 0, 10_000).await?;
        let (root_b, count) = *roots
            .iter()
            .rev()
            .find(|(_, c)| *c > burn_index)
            .context("no accepted burn root covers this burn yet: wait for the next checkpoint")?;
        let plan = release::plan(&self.ctx, &history, &row.owned, root_b.0, count, 0, ETH_TOKEN, 0)?;
        let release_image = eth::view(&self.p, self.vault, ICheckpoints::releaseImageIdCall {}, None).await?;
        anvil_send(
            &self.p,
            self.verifier,
            IMock::acceptCall { imageId: release_image, journalDigest: B256::from(sha256(&plan.journal)) }.abi_encode(),
            U256::ZERO,
        )
        .await?;
        let recipient = eth::address(row.owned.recipient);
        let before = self.p.get_balance(recipient).await?;
        let signer = eth::keystore_signer(&self.relayer_ks, &self.password)?;
        let sp = eth::signing_provider(&self.rpc, signer)?;
        let receipt = eth::send(
            &sp,
            self.relayer_addr,
            self.vault,
            release::calldata(&plan, Bytes::from(SEAL.to_vec())),
            U256::ZERO,
            1,
        )
        .await?;
        let paid = self.p.get_balance(recipient).await? - before;
        Ok(json!({
            "tx_hash": receipt.transaction_hash.to_string(),
            "block": receipt.block_number,
            "recipient": recipient.to_string(),
            "paid_wei": u256_str(paid),
            "sent_by": format!("the relayer {} (pays the gas; the recipient needs no ETH)", self.relayer_addr),
            "public_on_ethereum": ["the recipient", "the token and amount", "NOut (the release nullifier)", "the burn root used", "the time"],
            "stand_in": "the release proof: the mock verifier accepts the exact journal",
        }))
    }

    /// Switches the Ethereum RPC every later call reads and sends through (U.09). The zone is the
    /// in-process harness: a sequencer endpoint other than `harness` is refused, with the reason.
    async fn set_endpoints(&mut self, params: &Value) -> Result<Value> {
        let sequencer = params["sequencer"].as_str().unwrap_or("harness");
        ensure!(
            sequencer.is_empty() || sequencer == "harness",
            "sequencer {sequencer}: this devnet's zone is the in-process LEZ harness, which has no RPC; a sequencer \
             endpoint is used by ferrata_bridge_module and by the CLI's --sequencer"
        );
        if let Some(rpc) = params["eth_rpc"].as_str().filter(|r| !r.is_empty()) {
            if rpc != self.rpc {
                self.p = eth::provider(rpc)?;
                self.rpc = rpc.to_string();
            }
        }
        let reached = self.p.get_chain_id().await;
        Ok(json!({
            "eth_rpc": self.rpc,
            "reachable": reached.is_ok(),
            "chain_id": reached.as_ref().ok(),
            "error": reached.err().map(|e| e.to_string()),
            "sequencer": "the in-process LEZ harness",
        }))
    }

    async fn advance_time(&mut self, params: &Value) -> Result<Value> {
        let seconds = params["seconds"].as_u64().unwrap_or(3600);
        self.p.raw_request::<_, Value>("evm_increaseTime".into(), (seconds,)).await?;
        self.p.raw_request::<_, Value>("evm_mine".into(), Vec::<u64>::new()).await?;
        Ok(json!({"eth_time": self.eth_time().await?}))
    }

    async fn admin_status(&self) -> Result<Value> {
        let controls = hb::controls();
        Ok(json!({
            "admin": hx(&controls.admin),
            "freeze_authority": hx(&controls.freeze_authority),
            "fee_recipient": hx(&controls.fee_recipient),
            "floor": {"capacity": controls.floor.capacity.to_string(), "refill_per_second": controls.floor.refill_per_second.to_string()},
            "token_cap_at_registration": {"capacity": controls.token_cap.capacity.to_string(), "refill_per_second": controls.token_cap.refill_per_second.to_string()},
            "devnet_signers": "the harness's role keys (admin, freeze authority); a deployment uses its multisig",
            "registry": self.registry().await?,
        }))
    }

    /// `SetCap` from the admin (the bridge program refuses a cap below the floor).
    async fn admin_set_cap(&mut self, params: &Value) -> Result<Value> {
        let mints = params["mints"].as_bool().unwrap_or(true);
        let capacity: u128 = params["capacity"].as_str().context("capacity (LEZ units, a string)")?.parse()?;
        let refill: u128 = params["refill_per_second"].as_str().unwrap_or("0").parse()?;
        let signer = params["signer"].as_str().unwrap_or("admin");
        let seed = if signer == "freeze_authority" { hb::FREEZE_SEED } else { hb::ADMIN_SEED };
        let msg = BridgeMessage::SetCap {
            config_id: self.cid,
            token: Some(ETH_TOKEN),
            mints,
            cap: CapParams { capacity, refill_per_second: refill },
        };
        let target = token_actor(self.ctx.program, &self.cid, &ETH_TOKEN);
        let r = self.zone.world.role("SetCap (admin view)", seed, &msg, target);
        self.zone.record(&r);
        ensure!(r.ok(), "{}", refusal_of(&r, "SetCap"));
        Ok(
            json!({"settled": true, "what": "SetCap", "mints": mints, "capacity": capacity.to_string(), "refill_per_second": refill.to_string()}),
        )
    }

    /// `Freeze` from the freeze authority, bounded against the newest record's attested time.
    async fn admin_freeze(&mut self, params: &Value) -> Result<Value> {
        let mints = params["mints"].as_bool().unwrap_or(true);
        let burns = params["burns"].as_bool().unwrap_or(false);
        let seconds = params["seconds"].as_u64().unwrap_or(3600);
        let signer = params["signer"].as_str().unwrap_or("freeze_authority");
        let seed = if signer == "admin" { hb::ADMIN_SEED } else { hb::FREEZE_SEED };
        // A record of Ethereum's current time (the stand-in relayer's newest finalized header), as the
        // bridge program's own tests freeze against `World::finalize`.
        let now = self.eth_time().await?;
        let newest = self.zone.world.finalize(now);
        let t = self.zone.world.blocks[newest].time_ms();
        let record = self.zone.world.blocks[newest].time_record();
        // `lift`: until 0, the freeze authority's unfreeze.
        let until_ms = if params["lift"].as_bool().unwrap_or(false) { 0 } else { t + seconds * 1000 };
        let msg = BridgeMessage::Freeze { config_id: self.cid, token: Some(ETH_TOKEN), mints, burns, until_ms, record };
        let target = token_actor(self.ctx.program, &self.cid, &ETH_TOKEN);
        let r = self.zone.world.role("Freeze (admin view)", seed, &msg, target);
        self.zone.record(&r);
        ensure!(r.ok(), "{}", refusal_of(&r, "Freeze"));
        Ok(json!({"settled": true, "what": "Freeze", "mints": mints, "burns": burns, "until_ms": until_ms}))
    }
}

/// A failed report as the program's code, or the revert when there is no code.
fn refusal_of(r: &TxReport, what: &str) -> String {
    match r.exit_code {
        Some(code) => codes::program_refusal(code, format!("{what}, returned by the program")).to_string(),
        None => format!("{what} reverted without an exit code: {:?}", r.charge.revert),
    }
}

// ---- HTTP: one request at a time, JSON in and out ----------------------------------------------------

fn read_request(stream: &mut TcpStream) -> Result<(String, String, Vec<u8>)> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
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
    ensure!(length <= 1 << 20, "request too large");
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    Ok((method, path, body))
}

fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: POST, GET, OPTIONS\r\nAccess-Control-Allow-Headers: content-type\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
    let _ = stream.flush();
}

fn main() -> Result<()> {
    let port: u16 = std::env::var("FERRATA_DEVNET_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(18600);
    let anvil_port: u16 = std::env::var("FERRATA_DEVNET_ANVIL_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(18545);
    let dir = std::env::var("FERRATA_DEVNET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp/miniapp/devnet"));
    let rt = tokio::runtime::Runtime::new()?;
    let mut devnet = rt.block_on(Devnet::start(&dir, anvil_port))?;
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    println!(
        "miniapp devnet: http://127.0.0.1:{port}/rpc (anvil {}, vault {}, configId {})",
        devnet.rpc,
        devnet.vault,
        hx(&devnet.cid)
    );
    let mut log = std::fs::File::create(dir.join("calls.jsonl"))?;
    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };
        let (method, path, body) = match read_request(&mut stream) {
            Ok(r) => r,
            Err(e) => {
                respond(&mut stream, "400 Bad Request", &json!({"ok": false, "error": e.to_string()}).to_string());
                continue;
            }
        };
        if method == "OPTIONS" {
            respond(&mut stream, "204 No Content", "");
            continue;
        }
        if method == "GET" {
            respond(
                &mut stream,
                "200 OK",
                &json!({"ok": true, "service": "ferrata miniapp devnet", "path": path}).to_string(),
            );
            continue;
        }
        let req: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
        let name = req["method"].as_str().unwrap_or("").to_string();
        if name == "shutdown" {
            respond(&mut stream, "200 OK", &json!({"ok": true, "result": "stopping"}).to_string());
            break;
        }
        let t0 = std::time::Instant::now();
        let reply = match rt.block_on(devnet.call(&name, &req["params"])) {
            Ok(v) => json!({"ok": true, "result": v}),
            Err(e) => json!({"ok": false, "error": format!("{e:#}")}),
        };
        let ms = t0.elapsed().as_millis();
        let _ = writeln!(log, "{}", json!({"method": name, "params": req["params"], "ms": ms, "reply": reply}));
        println!("{name} {ms} ms ok={}", reply["ok"]);
        respond(&mut stream, "200 OK", &reply.to_string());
    }
    println!("miniapp devnet: stopped");
    Ok(())
}
