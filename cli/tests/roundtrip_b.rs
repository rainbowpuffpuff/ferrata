//! CI's round trip (`e2e/ci/README.md`): one deposit there and back, every step asserted, with
//! nothing outside the machine it runs on. Ethereum: the v5 vault under rule (B) on a local anvil
//! (chain id 1, never a public network) with the vault tests' `MockRiscZeroVerifier` behind the
//! emergency stop. LEZ: LEZ v0.4.0-rc1's state transition in the harness, with the attestation and
//! bridge guests (RISC Zero dev mode). The checkpoint: the rule (B) guest, executed in dev mode on
//! blocks that hold the transactions the harness executed (`checkpoint/fidelity` `ci_zone`). The
//! release: the release guest, executed in dev mode. The mock accepts exactly the journals the two
//! guests committed, as the vault's own tests accept theirs (`VaultTestBase`).
//!
//! 1. The zone's configuration and the zone's genesis checkpoint state; the vault deployed with
//!    both and the two guests' image ids.
//! 2. `Init` and `Register` (the bridge program attests the vault's slot 2, its registry entry and
//!    fee schedule 0 through the attestation program's records of a block holding the vault's own
//!    storage, read from anvil).
//! 3. A 1 ETH deposit on anvil to the test seed's public account /0.
//! 4. `ImportRoot` of the vault's root and count, `ImportFees` (no schedule 1), the plaintext claim:
//!    1 ETH minted on LEZ; a second claim refused (250).
//! 5. A burn of 0.1 ETH to a fresh Ethereum address.
//! 6. The rule (B) checkpoint over the zone's blocks: guest journal == native journal == the
//!    vault's own journal layout; a forged one (another burn root) refused; submitted, refused
//!    before its wait, finalised after it; the vault's accepted burn root equals the root the
//!    wallet computes from the zone.
//! 7. The release (guest in dev mode, journal == native): redirected to another address, refused;
//!    the recipient receives 0.1 ETH; a replay is refused.
//!
//! Runs only with `FERRATA_E2E=1` and the paths of the built tools in `FERRATA_CI_ZONE`,
//! `FERRATA_CI_CHECKPOINT_B_ELF` and `FERRATA_CI_RELEASE_ELF` (`e2e/ci/run.sh` sets them).
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Instant,
};

use alloy::{
    network::TransactionBuilder as _,
    primitives::{Address, Bytes, B256, U256},
    providers::{DynProvider, Provider},
    rpc::types::TransactionRequest,
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
use ferrata_lez_bridge_wire::{fee_slot, registry_slot, FeeSchedule};
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
    interface ICheckpointsB {
        struct Checkpoint {
            uint32 nMin; uint32 tMin; bytes32 parentState; bytes32 newState; bytes32 burnRoot; uint64 burnCount;
            uint64 importedLeafCount; bool sunset; bytes32 echoNonce; uint64 tipId; bytes32 tipHash; uint32 n; uint32 t;
        }
        function submitCandidate(Checkpoint c, bytes seal) external;
        function finalizeAttested(Checkpoint c) external;
        function finalState() external view returns (bytes32);
        function checkpointImageId() external view returns (bytes32);
        function releaseImageId() external view returns (bytes32);
        function mode() external view returns (uint8);
    }
}

/// BIP-39 test phrase (a published test value, never a wallet): the user.
const USER: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
/// anvil's first and second default accounts (unlocked on anvil): the deployer and stand-in
/// relayer, and the depositor.
const ANVIL0: Address = alloy::primitives::address!("f39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
const ANVIL1: Address = alloy::primitives::address!("70997970C51812dc3A010C7d01b50e0d17dc79C8");
/// The mock verifier's seal: it checks only that the (image id, journal digest) pair was accepted.
const SEAL: [u8; 4] = [0x5e, 0xa1, 0x5e, 0xa1];
const ONE_ETH: u128 = 1_000_000_000_000_000_000;
const TENTH_ETH: u128 = 100_000_000_000_000_000;

// ---- the zone: the harness behind the tool's `Ledger`, keeping every transaction it settled ----

struct HarnessLedger {
    world: hb::World,
    applied: Vec<Applied>,
    keys: HashMap<AccountId, PrivateKey>,
    /// `borsh(LeeTransaction)` of every applied transaction, in order: the zone's blocks.
    txs: Vec<Vec<u8>>,
}

impl HarnessLedger {
    fn record(&mut self, report: &TxReport) {
        if !report.ok() {
            return;
        }
        self.txs.push(self.world.chain.last_tx.clone());
        let tx =
            common::transaction::LeeTransaction::try_from_slice(&self.world.chain.last_tx).expect("a LEZ transaction");
        if let common::transaction::LeeTransaction::Public(tx) = tx {
            let root = &tx.message.execution.root;
            self.applied.push(Applied { to: root.to, message: root.message.clone() });
        }
    }

    /// Sends `call` without the tool's checks: what the program itself answers.
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
                Mention::Signer(a) => {
                    Some(self.keys.get(&a.account_id).expect("the holder's key is the seed's").clone())
                }
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

// ---- Ethereum: anvil and the vault ------------------------------------------------------------

struct Anvil(Child);

impl Drop for Anvil {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

fn scratch() -> PathBuf {
    let dir = std::env::var("FERRATA_E2E_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp/ci-roundtrip"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn tool(var: &str) -> Result<PathBuf> {
    let p = PathBuf::from(std::env::var(var).with_context(|| format!("{var} is not set (e2e/ci/run.sh sets it)"))?);
    ensure!(p.is_file(), "{var}: {} is not a file", p.display());
    Ok(p)
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

/// An unlocked anvil account sends a transaction; Err if it reverts. Only for transactions that must
/// succeed: the provider's nonce filler counts a transaction whose gas estimate fails as sent, and
/// the next one would then wait forever behind the gap (an expected refusal goes through [`reverts`]).
async fn anvil_send(p: &DynProvider, from: Address, to: Address, input: Vec<u8>, value: U256) -> Result<()> {
    let tx = TransactionRequest::default().with_from(from).with_to(to).with_input(input).with_value(value);
    let pending = p.send_transaction(tx).await?;
    let receipt = tokio::time::timeout(std::time::Duration::from_secs(60), pending.get_receipt())
        .await
        .map_err(|_| anyhow!("no receipt within 60 s"))??;
    ensure!(receipt.status(), "the transaction reverted");
    Ok(())
}

/// Whether the call reverts on the current state (`eth_call` from `from`): an expected refusal.
async fn reverts(p: &DynProvider, from: Address, to: Address, input: Vec<u8>) -> bool {
    p.call(TransactionRequest::default().with_from(from).with_to(to).with_input(input)).await.is_err()
}

async fn slot(p: &DynProvider, vault: Address, key: [u8; 32]) -> Result<[u8; 32]> {
    Ok(p.get_storage_at(vault, U256::from_be_bytes(key)).await?.to_be_bytes::<32>())
}

async fn advance(p: &DynProvider, seconds: u64) -> Result<()> {
    p.raw_request::<_, Value>("evm_increaseTime".into(), (seconds,)).await?;
    p.raw_request::<_, Value>("evm_mine".into(), Vec::<u64>::new()).await?;
    Ok(())
}

fn word(n: u64) -> [u8; 32] {
    let mut w = [0u8; 32];
    w[24..].copy_from_slice(&n.to_be_bytes());
    w
}

fn h32(v: &Value, key: &str) -> Result<B256> {
    let s = v[key].as_str().with_context(|| key.to_string())?;
    Ok(B256::from_slice(&hex::decode(s.trim_start_matches("0x"))?))
}

fn num(v: &Value, key: &str) -> Result<u64> {
    v[key].as_u64().with_context(|| key.to_string())
}

/// The checkpoint journal the vault rebuilds (VaultTestBase._checkpointJournalBytes).
fn checkpoint_journal(chain_id: u64, vault: Address, cid: B256, c: &ICheckpointsB::Checkpoint) -> Vec<u8> {
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

/// Executes the checkpoint guest in `elf` on `input` (RISC Zero dev mode: a fake receipt whose
/// journal is the guest's own). The guest reads a little-endian u32 length, then the bytes.
fn checkpoint_guest(elf: &Path, input: &[u8]) -> Result<(B256, Vec<u8>)> {
    use risc0_zkvm::{compute_image_id, default_prover, ExecutorEnv, ProverOpts};
    let elf = std::fs::read(elf)?;
    let image_id = compute_image_id(&elf).map_err(|e| anyhow!("not a guest program: {e}"))?;
    let len = u32::try_from(input.len())?.to_le_bytes();
    let env = ExecutorEnv::builder().write_slice(&len).write_slice(input).build().map_err(|e| anyhow!("{e}"))?;
    let info =
        default_prover().prove_with_opts(env, &elf, &ProverOpts::succinct()).map_err(|e| anyhow!("r0vm: {e}"))?;
    println!("checkpoint guest: {} user cycles (dev mode: executed, fake receipt)", info.stats.user_cycles);
    Ok((B256::from_slice(image_id.as_bytes()), info.receipt.journal.bytes))
}

fn ci_zone(args: &[&str]) -> Result<Value> {
    let out = Command::new(tool("FERRATA_CI_ZONE")?).args(args).output()?;
    ensure!(out.status.success(), "ci_zone {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr));
    Ok(serde_json::from_slice(&out.stdout)?)
}

fn step(t0: Instant, what: &str) {
    println!("[{:>6.1} s] ok: {what}", t0.elapsed().as_secs_f64());
}

#[tokio::test(flavor = "multi_thread")]
async fn deposit_attest_claim_burn_checkpoint_b_release() -> Result<()> {
    if std::env::var("FERRATA_E2E").is_err() {
        eprintln!("skipped: FERRATA_E2E is not set (e2e/ci/run.sh runs it)");
        return Ok(());
    }
    ensure!(std::env::var("RISC0_DEV_MODE").as_deref() == Ok("1"), "RISC0_DEV_MODE=1: the guests run in dev mode");
    let t0 = Instant::now();
    let dir = scratch();
    let checkpoint_elf = tool("FERRATA_CI_CHECKPOINT_B_ELF")?;
    let release_elf = tool("FERRATA_CI_RELEASE_ELF")?;
    let port: u16 = std::env::var("FERRATA_CI_ANVIL_PORT").ok().and_then(|s| s.parse().ok()).unwrap_or(18546);
    let rpc = format!("http://127.0.0.1:{port}");
    let _anvil = Anvil(
        Command::new("anvil")
            .args(["--port", &port.to_string(), "--chain-id", "1", "--silent", "--code-size-limit", "100000"])
            .stdout(Stdio::null())
            .spawn()
            .context("starting anvil")?,
    );
    let p = wait_for(&rpc).await?;
    let chain_id = p.get_chain_id().await?;
    ensure!(chain_id == 1, "anvil's chain id is the harness instance's (1)");

    // 1. The zone's configuration for the vault anvil's first account is about to deploy (its
    //    fourth contract: the linked FerrataRegistry library, the mock, its emergency stop, the vault).
    let mut zone =
        HarnessLedger { world: hb::World::new(), applied: Vec::new(), keys: HashMap::new(), txs: Vec::new() };
    let program = zone.world.program;
    let vault = ANVIL0.create(3);
    let config = hb::config(vault.into_array(), hb::controls());
    let cid = config.id(program);
    let identity = json!({
        "evm_chain_id": chain_id,
        "vault": hx(vault.as_slice()),
        "bedrock_genesis_id": hx(&config.bedrock_genesis_id),
        "channel_id": hx(&config.channel_id),
        "bridge_header": hx(program.value()),
        "bridge_image_id": hx(&config.bridge_image_id),
        "attestation_root_id": hx(&config.attestation_root_id()),
        "salt": hx(&config.salt()),
    });
    let identity_path = dir.join("identity.json");
    std::fs::write(&identity_path, serde_json::to_string_pretty(&identity)? + "\n")?;
    let identity_arg = identity_path.to_str().context("path")?;
    let genesis = ci_zone(&["genesis", "--identity", identity_arg])?;
    ensure!(h32(&genesis, "configId")?.0 == cid, "the checkpoint's configId is the bridge program's");
    let genesis_state = h32(&genesis, "genesisState")?;
    let checkpoint_image = {
        let elf = std::fs::read(&checkpoint_elf)?;
        B256::from_slice(risc0_zkvm::compute_image_id(&elf).map_err(|e| anyhow!("{e}"))?.as_bytes())
    };
    let release_image = {
        let elf = std::fs::read(&release_elf)?;
        B256::from_slice(risc0_zkvm::compute_image_id(&elf).map_err(|e| anyhow!("{e}"))?.as_bytes())
    };
    // One transaction at a time, each after the previous one's receipt (--slow), and at most 15 minutes: without it
    // anvil (1.3.6, automine) once left the deployer's next two transactions queued with no nonce gap while it mined
    // a block holding their predecessors, never mined (not even by evm_mine), and forge waited on (review of 22 Nov
    // 2026; the same as both_halves.rs's deployments).
    let out = Command::new("timeout")
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../vault"))
        .args(["900", "forge", "script", "script/CiRoundTrip.s.sol:CiRoundTrip", "--rpc-url", &rpc, "--broadcast"])
        .args(["--slow", "--unlocked"])
        .args(["--sender", &ANVIL0.to_string()])
        .env("FERRATA_CID", hx(&cid))
        .env("FERRATA_GENESIS_STATE", genesis_state.to_string())
        .env("FERRATA_CHECKPOINT_IMAGE_ID", checkpoint_image.to_string())
        .env("FERRATA_RELEASE_IMAGE_ID", release_image.to_string())
        .output()?;
    ensure!(out.status.success(), "forge script: {}", String::from_utf8_lossy(&out.stderr));
    ensure!(!p.get_code_at(vault).await?.is_empty(), "the vault is at the predicted address");
    let verifier = ANVIL0.create(1);
    ensure!(eth::view(&p, vault, ICheckpointsB::modeCall {}, None).await? == 1, "the vault runs rule (B)");
    ensure!(eth::view(&p, vault, ICheckpointsB::finalStateCall {}, None).await? == genesis_state, "genesisState");
    ensure!(eth::view(&p, vault, IVault::lezConfigIdCall {}, None).await?.0 == cid, "lezConfigId");
    let registered_at = p.get_block(alloy::eips::BlockId::latest()).await?.expect("a block").header.timestamp;
    println!("anvil: vault {vault} (rule B), configId {}, genesisState {genesis_state}", hx(&cid));
    println!("image ids: checkpoint (rule B) {checkpoint_image}, release {release_image}");
    step(t0, "1. zone configuration and genesis; vault deployed with both");

    // 2. Init and Register from the vault's own storage.
    let mut storage = Vec::new();
    for key in [word(2), registry_slot(&[0; 20]), fee_slot(&[0; 20], 0)] {
        storage.push((key, slot(&p, vault, key).await?));
    }
    ensure!(storage[0].1 == cid, "the vault's slot 2 holds the configuration's id");
    let b0 = zone.world.add_block(hb::Block::with_storage(9_000_000, 9_000_000, vault.into_array(), &storage));
    let r = zone.world.init("Init", &config, b0);
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
    let r = zone.world.register_with("Register ETH", &cid, b0, &hb::eth_entry(), &schedule, &vault.into_array());
    ensure!(r.ok(), "Register: {:?}", r.exit_code);
    zone.record(&r);
    step(t0, "2. Init and Register ETH on LEZ, attested from the vault's storage");

    // 3. The deposit: 1 ETH (denomination 1) from anvil's second account to the user's public /0.
    let user = Seed::from_phrase(USER)?;
    for a in user.public_accounts(8)? {
        zone.keys.insert(a.id, a.key.clone());
    }
    let keys = user.bridge_keys();
    let dest = user.destinations(8)?.of(DestKind::Public, 0);
    let j = eth::view(&p, vault, IVault::depositCountCall { depositor: ANVIL1 }, None).await?;
    let s = inbound::deposit_secret(&keys.auth, &chain_id_u256(chain_id), &vault.into_array(), &ANVIL1.into_array(), j);
    let commitment = inbound::commitment(&s, DestKind::Public, &dest);
    let price = eth::deposit_price(&p, vault, Address::ZERO, 1).await?;
    ensure!(price.denomination == ONE_ETH, "denomination 1 is 1 ETH");
    let calldata = eth::deposit_calldata(Address::ZERO, 1, B256::from(commitment), j);
    anvil_send(&p, ANVIL1, vault, calldata.to_vec(), U256::from(price.total)).await?;
    let n = eth::view(&p, vault, IVault::nextLeafIndexCall {}, None).await?;
    let root = eth::view(&p, vault, IVault::depositRootCall {}, None).await?;
    ensure!(n == 1, "one leaf in the vault's tree");
    step(t0, "3. deposit of 1 ETH on anvil");

    // 4. ImportRoot from the vault's storage after the deposit, ImportFees, the claim.
    let storage2 =
        vec![(word(0), slot(&p, vault, word(0)).await?), (word(1), slot(&p, vault, word(1)).await?), (word(2), cid)];
    ensure!(storage2[0].1 == root.0, "slot 0 is depositRoot");
    let b1 = zone.world.add_block(hb::Block::with_storage(9_000_064, 9_000_064, vault.into_array(), &storage2));
    let r = zone.world.import("ImportRoot", &cid, b1, root.0, n, &vault.into_array());
    ensure!(r.ok(), "ImportRoot: {:?}", r.exit_code);
    zone.record(&r);
    let r = zone.world.import_fees("ImportFees ETH 1 = none", &cid, b1, [0; 20], 1, None, &vault.into_array());
    ensure!(r.ok(), "ImportFees: {:?}", r.exit_code);
    zone.record(&r);
    let ctx = Context { program, cid, chain_id, vault: vault.into_array() };
    let list = eth::deposit_list(&p, vault, 0, 10_000).await?;
    let history = lez::history(&ctx, &zone.applied(program).await?);
    let rows = scan::deposits(&user, &ctx, &list, 8, &history, &HashSet::new())?;
    ensure!(rows.len() == 1, "the wallet finds its deposit, found {}", rows.len());
    let leaves: Vec<Hash> = list
        .deposits
        .iter()
        .map(|d| inbound::deposit_leaf(d.leaf_index, &d.depositor, &d.token, &d.amount, &d.commitment))
        .collect();
    let plan = lez::plan_claim(&zone, &ctx, &leaves, &rows[0].owned, &history, false, None, None).await?;
    zone.submit(&plan.call).await?;
    let holder = AccountId::new(dest);
    ensure!(zone.world.balance(holder, &cid) == ONE_ETH, "the holder holds 1 ETH on LEZ");
    let report = zone.settle(&plan.call);
    ensure!(report.exit_code == Some(250), "a second claim exits 250, got {:?}", report.exit_code);
    step(t0, "4. ImportRoot and ImportFees attested; claim minted 1 ETH on LEZ; second claim refused (250)");

    // 5. The burn: 0.1 ETH to a fresh address.
    let recipient = Address::from_slice(&sha256(b"ferrata ci round trip: recipient")[12..]);
    let history = lez::history(&ctx, &zone.applied(program).await?);
    let from = lez::BurnFrom::Public(holder);
    let bplan = lez::plan_burn(&zone, &ctx, &keys, from, [0; 20], TENTH_ETH, recipient.into_array(), &history).await?;
    zone.submit(&bplan.call).await?;
    ensure!(zone.world.balance(holder, &cid) == ONE_ETH - TENTH_ETH, "the burn debited 0.1 ETH");
    let history = lez::history(&ctx, &zone.applied(program).await?);
    ensure!(history.burns.len() == 1, "one burn published");
    let burn_leaves: Vec<Hash> = history
        .burns
        .iter()
        .map(|b| burn_leaf(&cid, b.burn_index, &b.note.token, &b.note.amount, &b.note.c_out, &b.note.r))
        .collect();
    let wallet_root = Tree::from_leaves(burn_leaves).map_err(|_| anyhow!("tree"))?.root();
    step(t0, "5. burn of 0.1 ETH on LEZ");

    // 6. The rule (B) checkpoint over the zone's blocks (one block per settled transaction).
    let blocks: Vec<Value> = zone.txs.iter().map(|t| json!([hx(t)])).collect();
    let txs_path = dir.join("txs.json");
    std::fs::write(&txs_path, serde_json::to_string(&json!({ "blocks": blocks }))?)?;
    let cp = ci_zone(&[
        "checkpoint",
        "--identity",
        identity_arg,
        "--txs",
        txs_path.to_str().context("path")?,
        "--out-dir",
        dir.to_str().context("path")?,
    ])?;
    let input = std::fs::read(dir.join("cp-ci-b.input"))?;
    let (image, journal) = checkpoint_guest(&checkpoint_elf, &input)?;
    ensure!(image == checkpoint_image, "the executed guest is the vault's checkpoint image");
    ensure!(hx(&journal) == cp["journal"].as_str().unwrap_or(""), "guest journal == native journal");
    let c = ICheckpointsB::Checkpoint {
        nMin: u32::try_from(num(&cp, "nMin")?)?,
        tMin: u32::try_from(num(&cp, "tMin")?)?,
        parentState: h32(&cp, "parentState")?,
        newState: h32(&cp, "newState")?,
        burnRoot: h32(&cp, "burnRoot")?,
        burnCount: num(&cp, "burnCount")?,
        importedLeafCount: num(&cp, "importedLeafCount")?,
        sunset: cp["sunset"].as_bool().context("sunset")?,
        echoNonce: h32(&cp, "echoNonce")?,
        tipId: num(&cp, "tipId")?,
        tipHash: h32(&cp, "tipHash")?,
        n: u32::try_from(num(&cp, "n")?)?,
        t: u32::try_from(num(&cp, "t")?)?,
    };
    ensure!(journal == checkpoint_journal(chain_id, vault, B256::from(cid), &c), "the guest's journal is the vault's");
    ensure!(c.parentState == genesis_state, "the checkpoint starts at the vault's genesis state");
    ensure!(c.burnCount == 1 && c.burnRoot.0 == wallet_root, "the checkpoint's burn root is the wallet's");
    ensure!(c.importedLeafCount == n, "the checkpoint saw the import of {n} leaves");
    println!("checkpoint: {} blocks, burn root {}, newState {}", num(&cp, "blocks")?, c.burnRoot, c.newState);
    let accept =
        |image: B256, j: &[u8]| IMock::acceptCall { imageId: image, journalDigest: B256::from(sha256(j)) }.abi_encode();
    anvil_send(&p, ANVIL0, verifier, accept(checkpoint_image, &journal), U256::ZERO).await?;
    // A checkpoint whose journal the guest did not commit (another burn root) is refused by the verifier.
    let mut forged = c.clone();
    forged.burnRoot = B256::from(sha256(b"ferrata ci round trip: a burn root nobody proved"));
    let forged = ICheckpointsB::submitCandidateCall { c: forged, seal: Bytes::from(SEAL.to_vec()) }.abi_encode();
    ensure!(reverts(&p, ANVIL0, vault, forged).await, "a checkpoint the guest did not produce is refused");
    let submit = ICheckpointsB::submitCandidateCall { c: c.clone(), seal: Bytes::from(SEAL.to_vec()) }.abi_encode();
    anvil_send(&p, ANVIL0, vault, submit, U256::ZERO).await?;
    let finalize = ICheckpointsB::finalizeAttestedCall { c: c.clone() }.abi_encode();
    ensure!(reverts(&p, ANVIL0, vault, finalize.clone()).await, "finalisation before the wait W is refused");
    advance(&p, 601).await?; // W = delta (600 s) under rule (B)
    anvil_send(&p, ANVIL0, vault, finalize, U256::ZERO).await?;
    ensure!(eth::view(&p, vault, ICheckpointsB::finalStateCall {}, None).await? == c.newState, "finalState moved");
    let roots = eth::accepted_burn_roots(&p, vault, 0, 10_000).await?;
    let (root_b, count) = *roots.last().context("an accepted burn root")?;
    ensure!(root_b.0 == wallet_root && count == 1, "the vault accepted the zone's burn root");
    step(t0, "6. rule (B) checkpoint: guest journal == native == vault layout; a forged one refused; refused early; finalised after W");

    // 7. The release, through the release guest (dev mode).
    advance(&p, 200).await?; // the token's release delay (120 s)
    let released = eth::released_nullifiers(&p, vault, 0, 10_000).await?;
    let (burn_rows, _) = scan::burns(&user, &ctx, &history, &released);
    ensure!(burn_rows.len() == 1, "the wallet finds its burn");
    let rplan = release::plan(&ctx, &history, &burn_rows[0].owned, root_b.0, count, 0, [0; 20], 0)?;
    let proven = release::prove(&release_elf, &rplan)?;
    ensure!(B256::from(proven.image_id) == release_image && proven.dev_mode, "the release guest ran in dev mode");
    anvil_send(&p, ANVIL0, verifier, accept(release_image, &rplan.journal), U256::ZERO).await?;
    // The same release paid to another address: not the journal the guest committed, refused.
    let mut other = rplan.request.clone();
    other.recipient = ANVIL0;
    let redirected = eth::release_calldata(other, Bytes::from(SEAL.to_vec()));
    ensure!(reverts(&p, ANVIL0, vault, redirected.to_vec()).await, "a release to another recipient is refused");
    let before = p.get_balance(recipient).await?;
    let call = release::calldata(&rplan, Bytes::from(SEAL.to_vec()));
    anvil_send(&p, ANVIL0, vault, call.to_vec(), U256::ZERO).await?;
    ensure!(p.get_balance(recipient).await? - before == U256::from(TENTH_ETH), "the recipient received 0.1 ETH");
    ensure!(
        eth::view(&p, vault, IVault::nullifierUsedCall { nullifier: B256::from(rplan.nullifier) }, None).await?,
        "NOut used"
    );
    ensure!(reverts(&p, ANVIL0, vault, call.to_vec()).await, "a replay is refused");
    step(t0, "7. release of 0.1 ETH on anvil; redirected and replayed releases refused");
    println!("round trip done in {:.1} s", t0.elapsed().as_secs_f64());
    Ok(())
}
