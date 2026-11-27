//! Both real halves at once (RFP-022 Sec.6, Sec.8, Sec.9, R.4, F.8, F.13): the v5 vault on a local anvil (chain id 1, never a
//! public network) and the bridge program in LEZ v0.4.0-rc1's state machine (the harness), with the rule (B) checkpoint
//! guest and the release guest executed in RISC Zero dev mode, as in CI's round trip (`roundtrip_b.rs`, whose helpers
//! this file repeats). The vault's verifier is the vault tests' mock behind RISC Zero's emergency stop; the test
//! accepts exactly the journals the two guests committed, so nothing is released that the guests did not prove.
//!
//! * `randomised_runs_keep_supply_within_holdings` (Sec.6, F.8): seeded runs of random steps over both halves:
//!   deposits of the three denominations from three depositors, root imports, claims, burns, one checkpoint, releases;
//!   and forgeries among them: claims with a tampered witness, replayed claims, releases no guest proved, replayed and
//!   redirected releases. After every step, read from the vault and the zone: the vault's balance is its escrow plus
//!   what it owes; the zone's wrapped supply is what it minted less what it burnt; and **the wrapped supply plus the
//!   unminted deposits plus the unreleased burns is the vault's escrow**, so the supply never exceeds the vault's
//!   holdings. For each claim, the anonymity set a private claim of its deposit would have at that moment (the client's
//!   `anonymity_set`) is recorded per denomination, twice: as run (every claim of these runs is plaintext and publishes
//!   its deposit, which leaves every later set, refused by the bucket or not) and as if every earlier claim had been
//!   private (nothing published). With `FERRATA_BOTH_HALVES_MINT_CAP_ETH` the admin first tightens ETH's mint bucket,
//!   so that claims are refused (255) and claimed later.
//! * `migration_rehearsal_honours_positions_in_flight` (Sec.8, R.4, Sec.9): an old vault and configuration with
//!   deposits imported or not, claimed or not, and burns before and after the retirement; the freeze authority retires
//!   the old vault; the zone attests it and runs the keyless `Sunset`; one sunset checkpoint; every burn released, every
//!   unimported deposit refunded to its depositor, every imported one still claimed on LEZ; a new vault and
//!   configuration take the new deposits. Then RISC Zero's emergency stop is tripped: releases and refunds stop on the
//!   old vault, nothing else moves (Sec.9's stop-only failsafe), and the new vault goes on.
//! * `fee_schedules_over_both_halves` (F.13, F.8): the admin announces protocol fees on the vault; seven days later
//!   they are in force and attested on LEZ (`ImportFees`). The vault credits the mint fee at deposit. Every claim
//!   mints only the denomination and the fixed relay fee; every release pays the denomination less the burn and
//!   checkpoint fees of the schedule in force, credited on the vault. The client's anonymity set of a claim counts
//!   equal-token, equal-amount deposits under the root on both sides of the fee change (before any claim, and at each
//!   plaintext claim, which publishes its deposit).
//!
//! Runs only with `FERRATA_E2E=1` and the built tools in `FERRATA_CI_ZONE`, `FERRATA_CI_CHECKPOINT_B_ELF` and
//! `FERRATA_CI_RELEASE_ELF` (`e2e/both-halves/run.sh` builds and sets them). Writes `e2e/both-halves/results/`.
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt::Write as _,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Instant,
};

use alloy::{
    network::TransactionBuilder as _,
    primitives::{keccak256, Address, Bytes, B256, U256},
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
    sha256,
    tree::Tree,
    Hash,
};
use ferrata_cli::{
    codes, eth,
    eth::IVault,
    hx,
    lez::{self, Applied, Context, Ledger, LezCall, Mention},
    release,
    scan::{self, DepositState},
    seed::Seed,
};
use ferrata_lez_bridge_wire::{
    config_actor, fee_slot, registry_slot, token_actor, BridgeConfig, BridgeMessage, CapParams, FeeSchedule,
};
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
    }
    #[allow(missing_docs)]
    interface IRetire {
        function retire() external;
        function refund(uint64 leafIndex, address depositor, address token, uint8 denomIndex, bytes32 commitment, bytes32[32] path) external;
        function totalOwed(address token) external view returns (uint256);
        function sunsetLeafCount() external view returns (uint64);
        function sunsetLeafCountSet() external view returns (bool);
        function refunded(uint64 leafIndex) external view returns (bool);
    }
    #[allow(missing_docs)]
    interface IFees {
        function announceFees(address token, uint256[] mintFees, uint256[] burnFees, uint256[] checkpointFees) external;
        function applyFees(address token) external;
    }
    #[allow(missing_docs)]
    interface IStop {
        function estop() external;
        function paused() external view returns (bool);
    }
}

/// BIP-39 test phrase (a published test value, never a wallet): the user.
const USER: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
/// anvil's default accounts (unlocked on anvil).
const ANVIL0: Address = alloy::primitives::address!("f39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
const ANVIL1: Address = alloy::primitives::address!("70997970C51812dc3A010C7d01b50e0d17dc79C8");
const ANVIL2: Address = alloy::primitives::address!("3C44CdDdB6a900fa2b585dd299e03d12FA4293BC");
const ANVIL3: Address = alloy::primitives::address!("90F79bf6EB2c4f870365E785982E1f101E93b906");
const ANVIL4: Address = alloy::primitives::address!("15d34AAf54267DB7D7c367839AAf71A00a2C6A65");
/// The mock verifier's seal: it checks only that the (image id, journal digest) pair was accepted.
const SEAL: [u8; 4] = [0x5e, 0xa1, 0x5e, 0xa1];
/// CiRoundTrip.s.sol's denominations of ETH.
const DENOMS: [u128; 3] = [100_000_000_000_000_000, 1_000_000_000_000_000_000, 10_000_000_000_000_000_000];

// ---- the zone: the harness behind the tool's `Ledger`, keeping every transaction it settled (as roundtrip_b.rs) ----

struct HarnessLedger {
    world: hb::World,
    applied: Vec<Applied>,
    refused: Vec<Applied>,
    keys: HashMap<AccountId, PrivateKey>,
    txs: Vec<Vec<u8>>,
}

impl HarnessLedger {
    /// Keeps the last settled transaction: its root call, applied or refused (LEZ keeps a refused transaction in its
    /// block, message and all: a refused plaintext claim publishes its witness), and the bytes of an applied one.
    fn record(&mut self, report: &TxReport) {
        let tx =
            common::transaction::LeeTransaction::try_from_slice(&self.world.chain.last_tx).expect("a LEZ transaction");
        if let common::transaction::LeeTransaction::Public(tx) = tx {
            let root = &tx.message.execution.root;
            let call = Applied { to: root.to, message: root.message.clone() };
            if !report.ok() {
                self.refused.push(call);
                return;
            }
            self.applied.push(call);
        }
        if report.ok() {
            self.txs.push(self.world.chain.last_tx.clone());
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

    fn keep(&mut self, report: TxReport, what: &str) -> Result<()> {
        ensure!(report.ok(), "{what}: exit {:?}, {:?}", report.exit_code, report.charge.revert);
        self.record(&report);
        Ok(())
    }
}

impl Ledger for HarnessLedger {
    async fn applied(&self, program: AccountId) -> Result<Vec<Applied>> {
        Ok(self.applied.iter().filter(|a| a.to.program_account_id == program).cloned().collect())
    }

    async fn included(&self, program: AccountId) -> Result<lez::Included> {
        let refused = self.refused.iter().filter(|a| a.to.program_account_id == program).cloned().collect();
        Ok(lez::Included { applied: self.applied(program).await?, refused, ..Default::default() })
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

// ---- Ethereum: anvil (as roundtrip_b.rs) --------------------------------------------------------------------------

struct Anvil(Child);

impl Drop for Anvil {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

fn scratch(name: &str) -> PathBuf {
    let base = std::env::var("FERRATA_E2E_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp/ci-roundtrip"));
    let dir = base.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn results_dir() -> PathBuf {
    let dir = std::env::var("FERRATA_BOTH_HALVES_OUT")
        .map(PathBuf::from)
        // This tree's programs (ADR 0046) record under results/f13/; results/*.md are the earlier program's records.
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../e2e/both-halves/results/f13"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn tool(var: &str) -> Result<PathBuf> {
    let p = PathBuf::from(std::env::var(var).with_context(|| format!("{var} is not set (e2e/both-halves/run.sh)"))?);
    ensure!(p.is_file(), "{var}: {} is not a file", p.display());
    Ok(p)
}

/// A local port nothing listens on now (the OS picks it): a fixed port can be held by one of the many short connections
/// an earlier run made, when the box's ephemeral range includes it (run 6's migration test: "Address already in use").
fn free_port() -> Result<u16> {
    Ok(std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

fn start_anvil(port: u16) -> Result<Anvil> {
    Ok(Anvil(
        Command::new("anvil")
            .args(["--port", &port.to_string(), "--chain-id", "1", "--silent", "--code-size-limit", "100000"])
            .stdout(Stdio::null())
            .spawn()
            .context("starting anvil")?,
    ))
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

async fn anvil_send(p: &DynProvider, from: Address, to: Address, input: Vec<u8>, value: U256) -> Result<()> {
    let tx = TransactionRequest::default().with_from(from).with_to(to).with_input(input).with_value(value);
    let pending = p.send_transaction(tx).await?;
    let receipt = tokio::time::timeout(std::time::Duration::from_secs(60), pending.get_receipt())
        .await
        .map_err(|_| anyhow!("no receipt within 60 s"))??;
    ensure!(receipt.status(), "the transaction reverted");
    Ok(())
}

/// Whether the call reverts on the current state (`eth_call` from `from`): an expected refusal, never sent.
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

fn checkpoint_guest(elf: &Path, input: &[u8]) -> Result<(B256, Vec<u8>)> {
    use risc0_zkvm::{compute_image_id, default_prover, ExecutorEnv, ProverOpts};
    let elf = std::fs::read(elf)?;
    let image_id = compute_image_id(&elf).map_err(|e| anyhow!("not a guest program: {e}"))?;
    let len = u32::try_from(input.len())?.to_le_bytes();
    let env = ExecutorEnv::builder().write_slice(&len).write_slice(input).build().map_err(|e| anyhow!("{e}"))?;
    let info =
        default_prover().prove_with_opts(env, &elf, &ProverOpts::succinct()).map_err(|e| anyhow!("r0vm: {e}"))?;
    Ok((B256::from_slice(image_id.as_bytes()), info.receipt.journal.bytes))
}

fn ci_zone(args: &[&str]) -> Result<Value> {
    let out = Command::new(tool("FERRATA_CI_ZONE")?).args(args).output()?;
    ensure!(out.status.success(), "ci_zone {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr));
    Ok(serde_json::from_slice(&out.stdout)?)
}

fn image_of(elf: &Path) -> Result<B256> {
    let elf = std::fs::read(elf)?;
    Ok(B256::from_slice(risc0_zkvm::compute_image_id(&elf).map_err(|e| anyhow!("{e}"))?.as_bytes()))
}

/// The CiRoundTrip vault's freeze authority (no key: on anvil it is impersonated) and the stop's owner.
fn freeze_authority() -> Address {
    Address::from_slice(&keccak256(b"ferrata ci round trip: freeze authority (no key)")[12..])
}

async fn impersonate(p: &DynProvider, who: Address) -> Result<()> {
    p.raw_request::<_, Value>("anvil_impersonateAccount".into(), (who,)).await?;
    p.raw_request::<_, Value>("anvil_setBalance".into(), (who, "0x56BC75E2D63100000")).await?; // 100 ETH
    Ok(())
}

fn denom_index(amount: u128) -> Result<u8> {
    DENOMS.iter().position(|d| *d == amount).map(|i| i as u8).ok_or_else(|| anyhow!("not a denomination: {amount}"))
}

// ---- one vault and its configuration ------------------------------------------------------------------------------

/// Beacon slots of the synthetic records this file adds (after `World::new`'s).
struct Clock(u64);

impl Clock {
    fn next(&mut self) -> u64 {
        self.0 += 32;
        self.0
    }
}

/// A vault on anvil, its configuration on the zone, and the books the invariants are checked against.
#[allow(dead_code)]
struct Side {
    name: &'static str,
    vault: Address,
    verifier: Address,
    stop: Address,
    config: BridgeConfig,
    ctx: Context,
    identity: PathBuf,
    genesis_state: B256,
    imported: u64,
    // The books, in wei: what the zone minted and burnt; deposits not yet minted (their whole escrow, by leaf);
    // burns not yet released (by burn index).
    minted: u128,
    burned: u128,
    unminted: BTreeMap<u64, u128>,
    unreleased: BTreeMap<u64, u128>,
    checked: u64,
}

/// Deploys CiRoundTrip from `deployer` and pairs the configuration on the zone (Init, Register ETH, attested from the
/// vault's own storage). The script creates the library, the mock, the stop and the vault, from the deployer's next
/// nonce; forge deploys the library once per chain (a second deployment on the same chain reuses it: `library_on_chain`),
/// so the vault is the deployer's third or fourth creation.
#[allow(clippy::too_many_arguments)]
async fn deploy(
    p: &DynProvider,
    zone: &mut HarnessLedger,
    clock: &mut Clock,
    rpc: &str,
    dir: &Path,
    name: &'static str,
    deployer: Address,
    images: (B256, B256),
    library_on_chain: bool,
) -> Result<Side> {
    let chain_id = p.get_chain_id().await?;
    let program = zone.world.program;
    let first = p.get_transaction_count(deployer).await? + u64::from(!library_on_chain);
    let vault = deployer.create(first + 2);
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
    let identity_path = dir.join(format!("identity-{name}.json"));
    std::fs::write(&identity_path, serde_json::to_string_pretty(&identity)? + "\n")?;
    let genesis = ci_zone(&["genesis", "--identity", identity_path.to_str().context("path")?])?;
    ensure!(h32(&genesis, "configId")?.0 == cid, "the checkpoint's configId is the bridge program's");
    let genesis_state = h32(&genesis, "genesisState")?;
    // One transaction at a time, each after the previous one's receipt (--slow), and at most 15 minutes: on the build
    // box (21 Nov 2026) the second deployment of the migration test once left its last two transactions pending on
    // anvil for an hour, never mined (not even by evm_mine), while forge waited for their receipts.
    let out = Command::new("timeout")
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../vault"))
        .args([
            "900",
            "forge",
            "script",
            "script/CiRoundTrip.s.sol:CiRoundTrip",
            "--rpc-url",
            rpc,
            "--broadcast",
            "--slow",
        ])
        .args(["--unlocked", "--sender", &deployer.to_string()])
        .env("FERRATA_CID", hx(&cid))
        .env("FERRATA_GENESIS_STATE", genesis_state.to_string())
        .env("FERRATA_CHECKPOINT_IMAGE_ID", images.0.to_string())
        .env("FERRATA_RELEASE_IMAGE_ID", images.1.to_string())
        .output()?;
    ensure!(out.status.success(), "forge script: {}", String::from_utf8_lossy(&out.stderr));
    ensure!(!p.get_code_at(vault).await?.is_empty(), "the vault is at the predicted address");
    ensure!(eth::view(p, vault, IVault::lezConfigIdCall {}, None).await?.0 == cid, "lezConfigId");
    let registered_at = p.get_block(alloy::eips::BlockId::latest()).await?.expect("a block").header.timestamp;
    let mut storage = Vec::new();
    for key in [word(2), registry_slot(&[0; 20]), fee_slot(&[0; 20], 0)] {
        storage.push((key, slot(p, vault, key).await?));
    }
    let s = clock.next();
    let b = zone.world.add_block(hb::Block::with_storage(s, s, vault.into_array(), &storage));
    let r = zone.world.init(&format!("{name}: Init"), &config, b);
    zone.keep(r, "Init")?;
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
        &format!("{name}: Register ETH"),
        &cid,
        b,
        &hb::eth_entry(),
        &schedule,
        &vault.into_array(),
    );
    zone.keep(r, "Register")?;
    Ok(Side {
        name,
        vault,
        verifier: deployer.create(first),
        stop: deployer.create(first + 1),
        config,
        ctx: Context { program, cid, chain_id, vault: vault.into_array() },
        identity: identity_path,
        genesis_state,
        imported: 0,
        minted: 0,
        burned: 0,
        unminted: BTreeMap::new(),
        unreleased: BTreeMap::new(),
        checked: 0,
    })
}

/// A deposit of denomination `denom` from `depositor` to the user's public account `/dest`.
async fn deposit(
    p: &DynProvider,
    side: &mut Side,
    user: &Seed,
    depositor: Address,
    dest: u32,
    denom: u8,
) -> Result<u64> {
    let keys = user.bridge_keys();
    let vault = side.vault;
    let j = eth::view(p, vault, IVault::depositCountCall { depositor }, None).await?;
    let s = inbound::deposit_secret(
        &keys.auth,
        &chain_id_u256(side.ctx.chain_id),
        &vault.into_array(),
        &depositor.into_array(),
        j,
    );
    let to = user.destinations(8)?.of(DestKind::Public, dest);
    let c = inbound::commitment(&s, DestKind::Public, &to);
    let price = eth::deposit_price(p, vault, Address::ZERO, denom).await?;
    anvil_send(
        p,
        depositor,
        vault,
        eth::deposit_calldata(Address::ZERO, denom, B256::from(c), j).to_vec(),
        U256::from(price.total),
    )
    .await?;
    let leaf = eth::view(p, vault, IVault::nextLeafIndexCall {}, None).await? - 1;
    // ADR 0046: the vault escrows the denomination and the relay fee; the mint fee is credited to the fee recipient
    // (owed) at the deposit, so it is not part of what the zone will mint.
    side.unminted.insert(leaf, price.total - price.mint_fee);
    Ok(leaf)
}

/// `ImportRoot` of the vault's root now (and the absence of fee schedule 1, so the schedules cover every leaf).
async fn import(p: &DynProvider, zone: &mut HarnessLedger, clock: &mut Clock, side: &mut Side) -> Result<bool> {
    let vault = side.vault;
    let n = eth::view(p, vault, IVault::nextLeafIndexCall {}, None).await?;
    if n == 0 || n <= side.imported {
        return Ok(false);
    }
    let root = eth::view(p, vault, IVault::depositRootCall {}, None).await?;
    let mut storage = Vec::new();
    for key in [word(0), word(1), word(2)] {
        storage.push((key, slot(p, vault, key).await?));
    }
    let s = clock.next();
    let b = zone.world.add_block(hb::Block::with_storage(s, s, vault.into_array(), &storage));
    let cid = side.ctx.cid;
    let r = zone.world.import(&format!("{}: ImportRoot n = {n}", side.name), &cid, b, root.0, n, &vault.into_array());
    zone.keep(r, "ImportRoot")?;
    let r = zone.world.import_fees(
        &format!("{}: ImportFees, no schedule 1", side.name),
        &cid,
        b,
        [0; 20],
        1,
        None,
        &vault.into_array(),
    );
    zone.keep(r, "ImportFees")?;
    side.imported = n;
    Ok(true)
}

/// The vault's leaves and the user's deposit rows.
async fn rows(
    p: &DynProvider,
    zone: &HarnessLedger,
    side: &Side,
    user: &Seed,
) -> Result<(eth::DepositList, Vec<Hash>, Vec<scan::DepositRow>)> {
    let list = eth::deposit_list(p, side.vault, 0, 10_000).await?;
    let history = lez::history_of(zone, &side.ctx).await?;
    let refunded = eth::refunded_leaves(p, side.vault, 0, 10_000).await?;
    let rows = scan::deposits(user, &side.ctx, &list, 8, &history, &refunded)?;
    let leaves = list
        .deposits
        .iter()
        .map(|d| inbound::deposit_leaf(d.leaf_index, &d.depositor, &d.token, &d.amount, &d.commitment))
        .collect();
    Ok((list, leaves, rows))
}

/// The program's exit code of a refusal the tool reports (`codes::Refusal`), if it is one.
fn refusal_code(e: &anyhow::Error) -> Option<u32> {
    e.downcast_ref::<codes::Refusal>().and_then(|r| r.code.parse().ok())
}

/// The plaintext claim of the user's deposit at `leaf`; with `forge`, a tampered copy (another `s`) first, refused
/// with 249, and after it the same claim again, refused (250, or 255 when the token's bucket is spent first: the
/// program admits the value before it reaches the nullifier). Returns the anonymity set a private claim of the deposit
/// would have at that moment, or `None` when the mint bucket refused it (255): the deposit stays claimable and a later
/// step claims it (R.4: a cap only delays), but the refused claim stays in its block and has published the deposit
/// (F.8). The plaintext claim itself shows the client's 1: it names its deposit.
async fn claim(
    p: &DynProvider,
    zone: &mut HarnessLedger,
    side: &mut Side,
    user: &Seed,
    leaf: u64,
    forge: bool,
    log: &mut Log,
) -> Result<Option<usize>> {
    let (list, leaves, rows) = rows(p, zone, side, user).await?;
    let row = rows.iter().find(|r| r.owned.deposit.leaf_index == leaf).context("the user's deposit")?;
    ensure!(matches!(row.state, DepositState::Claimable { .. }), "leaf {leaf} is claimable: {:?}", row.state);
    let history = lez::history_of(&*zone, &side.ctx).await?;
    let plan = lez::plan_claim(&*zone, &side.ctx, &leaves, &row.owned, &history, false, None, None).await?;
    let d = &row.owned.deposit;
    // F.8: the deposits of the same token and amount under the root, less those that earlier
    // plaintext claims published, applied or refused on chain (`History.opened` from the zone's blocks: its leaves among
    // the vault's deposits must be exactly those this test sent a plaintext claim of). A deposit whose own earlier claim
    // was refused is published already: the client shows 1 (`flow::claim`). Beside it, the set had every earlier claim
    // been private (nothing published, refused or not).
    let opened: BTreeSet<u64> = list
        .deposits
        .iter()
        .zip(&leaves)
        .filter(|(_, l)| history.opened.contains(*l))
        .map(|(d, _)| d.leaf_index)
        .collect();
    let published = log.published.get(&side.vault).cloned().unwrap_or_default();
    ensure!(opened == published, "the client's history opens {opened:?}; plaintext claims sent: {published:?}");
    let set = if published.contains(&leaf) {
        log.reclaimed += 1;
        1
    } else {
        ferrata_wallet::claim::anonymity_set(&list.deposits, &history.opened, &d.token, &d.amount, plan.leaf_count)
    };
    let set_private =
        ferrata_wallet::claim::anonymity_set(&list.deposits, &HashSet::new(), &d.token, &d.amount, plan.leaf_count);
    ensure!(1 <= set && set <= set_private, "the deposit's own set: {set}, {set_private}");
    if forge {
        let mut tampered = plan.call.clone();
        if let BridgeMessage::Claim { witness, .. } = &mut tampered.message {
            witness.s = sha256(&witness.s);
        }
        let r = zone.settle(&tampered);
        ensure!(r.exit_code == Some(249), "a claim with a tampered witness: {:?}", r.exit_code);
        log.forged += 1;
    }
    log.published.entry(side.vault).or_default().insert(leaf); // applied or refused, its witness is in a block now
    if let Err(e) = zone.submit(&plan.call).await {
        ensure!(refusal_code(&e) == Some(255), "the claim of leaf {leaf}: {e}");
        log.capped += 1;
        return Ok(None);
    }
    let amount = ferrata_cli::units::word_to_u128(&d.amount)?;
    side.minted += amount + plan.relay_fee;
    let escrow = side.unminted.remove(&leaf).context("an unminted deposit")?;
    ensure!(escrow == amount + plan.relay_fee, "the claim mints denomination + relay fee; the vault kept the mint fee");
    if forge {
        let r = zone.settle(&plan.call);
        ensure!(matches!(r.exit_code, Some(250 | 255)), "a replayed claim: {:?}", r.exit_code);
        log.forged += 1;
    }
    log.claims += 1;
    log.sets.entry(amount).or_default().push(set);
    log.sets_private.entry(amount).or_default().push(set_private);
    Ok(Some(set))
}

/// A burn of `amount` from the user's public account `/from` to `recipient`. Returns the burn index.
async fn burn(
    zone: &mut HarnessLedger,
    side: &mut Side,
    user: &Seed,
    from: AccountId,
    amount: u128,
    recipient: Address,
) -> Result<u64> {
    let keys = user.bridge_keys();
    let history = lez::history(&side.ctx, &zone.applied(side.ctx.program).await?);
    let index = history.burns.len() as u64;
    let plan = lez::plan_burn(
        &*zone,
        &side.ctx,
        &keys,
        lez::BurnFrom::Public(from),
        [0; 20],
        amount,
        recipient.into_array(),
        &history,
    )
    .await?;
    zone.submit(&plan.call).await?;
    side.burned += amount;
    side.unreleased.insert(index, amount);
    Ok(index)
}

/// The rule (B) checkpoint over every transaction the zone settled: guest in dev mode, journal accepted on the mock,
/// submitted, finalised after the wait. Returns (burn root, burn count, imported leaf count, sunset).
async fn checkpoint(
    p: &DynProvider,
    zone: &HarnessLedger,
    side: &Side,
    dir: &Path,
    elf: &Path,
    image: B256,
) -> Result<(B256, u64, u64, bool)> {
    let blocks: Vec<Value> = zone.txs.iter().map(|t| json!([hx(t)])).collect();
    let txs_path = dir.join(format!("txs-{}.json", side.name));
    std::fs::write(&txs_path, serde_json::to_string(&json!({ "blocks": blocks }))?)?;
    let cp = ci_zone(&[
        "checkpoint",
        "--identity",
        side.identity.to_str().context("path")?,
        "--txs",
        txs_path.to_str().context("path")?,
        "--out-dir",
        dir.to_str().context("path")?,
    ])?;
    let input = std::fs::read(dir.join("cp-ci-b.input"))?;
    let (got, journal) = checkpoint_guest(elf, &input)?;
    ensure!(got == image, "the executed guest is the vault's checkpoint image");
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
    ensure!(
        journal == checkpoint_journal(side.ctx.chain_id, side.vault, B256::from(side.ctx.cid), &c),
        "the vault's journal"
    );
    ensure!(c.parentState == side.genesis_state, "from the vault's genesis state");
    let accept = IMock::acceptCall { imageId: image, journalDigest: B256::from(sha256(&journal)) }.abi_encode();
    anvil_send(p, ANVIL0, side.verifier, accept, U256::ZERO).await?;
    let submit = ICheckpointsB::submitCandidateCall { c: c.clone(), seal: Bytes::from(SEAL.to_vec()) }.abi_encode();
    anvil_send(p, ANVIL0, side.vault, submit, U256::ZERO).await?;
    advance(p, 601).await?;
    anvil_send(p, ANVIL0, side.vault, ICheckpointsB::finalizeAttestedCall { c: c.clone() }.abi_encode(), U256::ZERO)
        .await?;
    ensure!(eth::view(p, side.vault, ICheckpointsB::finalStateCall {}, None).await? == c.newState, "finalState moved");
    Ok((c.burnRoot, c.burnCount, c.importedLeafCount, c.sunset))
}

/// The release of the user's burn `index` under the accepted root; with `forge`, a release no guest proved first
/// (refused), then after it a replay and a redirected copy (refused).
#[allow(clippy::too_many_arguments)]
async fn release_burn(
    p: &DynProvider,
    zone: &HarnessLedger,
    side: &mut Side,
    user: &Seed,
    index: u64,
    root: (B256, u64),
    elf: &Path,
    image: B256,
    forge: bool,
    log: &mut Log,
) -> Result<()> {
    release_paying(p, zone, side, user, index, root, (elf, image), forge, log, 0).await.map(|_| ())
}

/// `release_burn`, with the burn and checkpoint fees of the schedule in force, `fees` together: the recipient gets the
/// denomination less them. Returns what the recipient received.
#[allow(clippy::too_many_arguments)]
async fn release_paying(
    p: &DynProvider,
    zone: &HarnessLedger,
    side: &mut Side,
    user: &Seed,
    index: u64,
    root: (B256, u64),
    guest: (&Path, B256),
    forge: bool,
    log: &mut Log,
    fees: u128,
) -> Result<u128> {
    let (elf, image) = guest;
    let history = lez::history(&side.ctx, &zone.applied(side.ctx.program).await?);
    let released = eth::released_nullifiers(p, side.vault, 0, 10_000).await?;
    let (burn_rows, _) = scan::burns(user, &side.ctx, &history, &released);
    let row = burn_rows.iter().find(|r| r.owned.burn.burn_index == index).context("the user's burn")?;
    ensure!(!row.released, "burn {index} not released yet");
    let amount = ferrata_cli::units::word_to_u128(&row.owned.burn.note.amount)?;
    let rplan = release::plan(&side.ctx, &history, &row.owned, root.0 .0, root.1, denom_index(amount)?, [0; 20], 0)?;
    let call = release::calldata(&rplan, Bytes::from(SEAL.to_vec()));
    if forge {
        ensure!(reverts(p, ANVIL0, side.vault, call.to_vec()).await, "a release no guest proved is refused");
        log.forged += 1;
    }
    let proven = release::prove(elf, &rplan)?;
    ensure!(B256::from(proven.image_id) == image && proven.dev_mode, "the release guest ran in dev mode");
    let accept = IMock::acceptCall { imageId: image, journalDigest: B256::from(sha256(&rplan.journal)) }.abi_encode();
    anvil_send(p, ANVIL0, side.verifier, accept, U256::ZERO).await?;
    let recipient = Address::from(row.owned.recipient);
    let before = p.get_balance(recipient).await?;
    anvil_send(p, ANVIL0, side.vault, call.to_vec(), U256::ZERO).await?;
    let paid = p.get_balance(recipient).await? - before;
    ensure!(paid == U256::from(amount - fees), "the recipient received the denomination less the fees: {paid}");
    side.unreleased.remove(&index).context("an unreleased burn")?;
    if forge {
        ensure!(reverts(p, ANVIL0, side.vault, call.to_vec()).await, "a replayed release is refused");
        let mut other = rplan.request.clone();
        other.recipient = ANVIL4;
        let redirected = eth::release_calldata(other, Bytes::from(SEAL.to_vec()));
        ensure!(reverts(p, ANVIL0, side.vault, redirected.to_vec()).await, "a redirected release is refused");
        log.forged += 2;
    }
    log.releases += 1;
    Ok(amount - fees)
}

/// The zone's wrapped supply under `side`: the holdings of every account a claim of this file mints to (the user's
/// public accounts, the fee recipient, the relayer). The harness enumerates no accounts, so a mint to any other account
/// would show only as a claim minting less than its deposit's escrow, which `claim` refuses.
fn supply(zone: &HarnessLedger, side: &Side, user: &Seed) -> Result<u128> {
    let mut accounts: Vec<AccountId> = user.public_accounts(8)?.into_iter().map(|a| a.id).collect();
    accounts.extend([hb::fee_recipient(), hb::relayer()]);
    Ok(accounts.iter().map(|a| zone.world.balance(*a, &side.ctx.cid)).sum())
}

/// The cross-side invariants, read from the vault and the zone.
async fn invariants(p: &DynProvider, zone: &HarnessLedger, side: &mut Side, user: &Seed) -> Result<()> {
    let escrowed = eth::view(p, side.vault, IVault::escrowedCall { token: Address::ZERO }, None).await?;
    let owed = eth::view(p, side.vault, IRetire::totalOwedCall { token: Address::ZERO }, None).await?;
    let balance = p.get_balance(side.vault).await?;
    ensure!(balance == escrowed + owed, "{}: vault balance {balance} == escrowed {escrowed} + owed {owed}", side.name);
    let s = supply(zone, side, user)?;
    ensure!(
        s == side.minted - side.burned,
        "{}: zone supply {s} == minted {} - burnt {}",
        side.name,
        side.minted,
        side.burned
    );
    ensure!(U256::from(s) <= escrowed, "Sec.6, {}: supply {s} exceeds the vault's escrow {escrowed}", side.name);
    let unminted: u128 = side.unminted.values().sum();
    let unreleased: u128 = side.unreleased.values().sum();
    ensure!(
        U256::from(s + unminted + unreleased) == escrowed,
        "{}: supply {s} + unminted {unminted} + unreleased {unreleased} == escrowed {escrowed}",
        side.name
    );
    side.checked += 1;
    Ok(())
}

#[derive(Default)]
struct Log {
    steps: u64,
    deposits: u64,
    imports: u64,
    claims: u64,
    burns: u64,
    releases: u64,
    forged: u64,
    capped: u64,
    /// Claims of a deposit whose earlier plaintext claim the bucket refused (its set as run is 1).
    reclaimed: u64,
    /// Per vault, the leaves the test sent a plaintext claim of (applied or refused: each is in a block).
    published: BTreeMap<Address, BTreeSet<u64>>,
    /// Per denomination, each claim's set as run and had every earlier claim been private.
    sets: BTreeMap<u128, Vec<usize>>,
    sets_private: BTreeMap<u128, Vec<usize>>,
}

/// xorshift64*: deterministic per seed, no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn guard() -> Result<bool> {
    if std::env::var("FERRATA_E2E").is_err() {
        eprintln!("skipped: FERRATA_E2E is not set (e2e/both-halves/run.sh runs it)");
        return Ok(false);
    }
    ensure!(std::env::var("RISC0_DEV_MODE").as_deref() == Ok("1"), "RISC0_DEV_MODE=1: the guests run in dev mode");
    Ok(true)
}

fn new_zone(user: &Seed) -> Result<HarnessLedger> {
    let mut zone = HarnessLedger {
        world: hb::World::new(),
        applied: Vec::new(),
        refused: Vec::new(),
        keys: HashMap::new(),
        txs: Vec::new(),
    };
    for a in user.public_accounts(8)? {
        zone.keys.insert(a.id, a.key.clone());
    }
    Ok(zone)
}

// ---- Sec.6 and F.8: randomised runs over both halves ----------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn randomised_runs_keep_supply_within_holdings() -> Result<()> {
    if !guard()? {
        return Ok(());
    }
    let t0 = Instant::now();
    let checkpoint_elf = tool("FERRATA_CI_CHECKPOINT_B_ELF")?;
    let release_elf = tool("FERRATA_CI_RELEASE_ELF")?;
    let images = (image_of(&checkpoint_elf)?, image_of(&release_elf)?);
    let seeds: Vec<u64> = std::env::var("FERRATA_BOTH_HALVES_SEEDS")
        .unwrap_or_else(|_| "1,2,3".into())
        .split(',')
        .map(|s| s.trim().parse().expect("a seed"))
        .collect();
    let steps: u64 = std::env::var("FERRATA_BOTH_HALVES_STEPS").ok().and_then(|s| s.parse().ok()).unwrap_or(40);
    // FERRATA_BOTH_HALVES_MINT_CAP_ETH: after pairing, the admin sets ETH's mint bucket to that many ETH (`SetCap`,
    // refilled at the harness's rate, 0.01 ETH per second of attested time), so that claims meet a spent bucket (255):
    // each refused plaintext claim publishes its deposit all the same (F.8) and the deposit is claimed later (R.4).
    let mint_cap: Option<u128> =
        std::env::var("FERRATA_BOTH_HALVES_MINT_CAP_ETH").ok().map(|v| v.parse().expect("whole ETH"));
    let user = Seed::from_phrase(USER)?;
    let mut report = String::new();
    let mut all_sets: BTreeMap<u128, Vec<usize>> = BTreeMap::new();
    let mut all_private: BTreeMap<u128, Vec<usize>> = BTreeMap::new();
    for seed in &seeds {
        let port = free_port()?;
        let rpc = format!("http://127.0.0.1:{port}");
        let _anvil = start_anvil(port)?;
        let p = wait_for(&rpc).await?;
        ensure!(p.get_chain_id().await? == 1, "anvil's chain id is the harness instance's (1)");
        let dir = scratch(&format!("both-halves-{seed}"));
        let mut zone = new_zone(&user)?;
        let mut clock = Clock(9_200_000);
        let mut side = deploy(&p, &mut zone, &mut clock, &rpc, &dir, "A", ANVIL0, images, false).await?;
        if let Some(cap) = mint_cap {
            let cid = side.ctx.cid;
            let cap = CapParams { capacity: cap * 1_000_000_000, refill_per_second: hb::TOKEN_CAP.refill_per_second };
            let message = BridgeMessage::SetCap { config_id: cid, token: Some([0; 20]), mints: true, cap };
            let token = token_actor(side.ctx.program, &cid, &[0; 20]);
            let r = zone.world.role("A: SetCap ETH mints", hb::ADMIN_SEED, &message, token);
            zone.keep(r, "SetCap")?;
        }
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15 ^ seed);
        let mut log = Log::default();
        let mut root: Option<(B256, u64)> = None;
        let depositors = [ANVIL1, ANVIL2, ANVIL3];
        invariants(&p, &zone, &mut side, &user).await?;
        for step in 0..steps {
            log.steps += 1;
            let roll = rng.below(100);
            let half = step >= steps / 2;
            if roll < 30 {
                let denom = rng.below(3) as u8;
                let who = depositors[rng.below(3) as usize];
                let dest = rng.below(4) as u32;
                deposit(&p, &mut side, &user, who, dest, denom).await?;
                log.deposits += 1;
            } else if roll < 42 {
                if import(&p, &mut zone, &mut clock, &mut side).await? {
                    log.imports += 1;
                }
            } else if roll < 64 {
                let (_, _, rows) = rows(&p, &zone, &side, &user).await?;
                let open: Vec<u64> = rows
                    .iter()
                    .filter(|r| matches!(r.state, DepositState::Claimable { .. }))
                    .map(|r| r.owned.deposit.leaf_index)
                    .collect();
                if !open.is_empty() {
                    let leaf = open[rng.below(open.len() as u64) as usize];
                    let forge = rng.below(3) == 0;
                    claim(&p, &mut zone, &mut side, &user, leaf, forge, &mut log).await?;
                }
            } else if roll < 80 {
                let holders: Vec<(AccountId, u128)> = user
                    .public_accounts(4)?
                    .into_iter()
                    .map(|a| (a.id, zone.world.balance(a.id, &side.ctx.cid)))
                    .filter(|(_, b)| *b >= DENOMS[0])
                    .collect();
                if !holders.is_empty() {
                    let (from, held) = holders[rng.below(holders.len() as u64) as usize];
                    let fits: Vec<u128> = DENOMS.iter().copied().filter(|d| *d <= held).collect();
                    let amount = fits[rng.below(fits.len() as u64) as usize];
                    let recipient = Address::from_slice(
                        &sha256(&[b"recipient".as_slice(), &step.to_be_bytes(), &seed.to_be_bytes()].concat())[12..],
                    );
                    burn(&mut zone, &mut side, &user, from, amount, recipient).await?;
                    log.burns += 1;
                }
            } else if root.is_none() && half && !side.unreleased.is_empty() {
                let (r, count, _, _) = checkpoint(&p, &zone, &side, &dir, &checkpoint_elf, images.0).await?;
                advance(&p, 200).await?; // the token's release delay (120 s)
                root = Some((r, count));
            } else if let Some(root) = root {
                let ready: Vec<u64> = side.unreleased.keys().copied().filter(|i| *i < root.1).collect();
                if !ready.is_empty() {
                    let index = ready[rng.below(ready.len() as u64) as usize];
                    let forge = rng.below(3) == 0;
                    release_burn(&p, &zone, &mut side, &user, index, root, &release_elf, images.1, forge, &mut log)
                        .await?;
                }
            }
            invariants(&p, &zone, &mut side, &user).await?;
        }
        // Whatever is left in the books is what the vault still holds for it.
        invariants(&p, &zone, &mut side, &user).await?;
        let _ = writeln!(
            report,
            "| {seed} | {} | {} | {} | {} | {} ({}) | {} | {} | {} | {} | {} | {} |",
            log.steps,
            log.deposits,
            log.imports,
            log.claims,
            log.capped,
            log.reclaimed,
            log.burns,
            log.releases,
            log.forged,
            side.unminted.len(),
            side.unreleased.len(),
            side.checked
        );
        for (d, v) in log.sets {
            all_sets.entry(d).or_default().extend(v);
        }
        for (d, v) in log.sets_private {
            all_private.entry(d).or_default().extend(v);
        }
        println!(
            "[{:>6.1} s] seed {seed}: {} steps, invariants held {} times",
            t0.elapsed().as_secs_f64(),
            log.steps,
            side.checked
        );
    }
    let mut out = format!(
        "# Randomised runs over both real halves (`cli/tests/both_halves.rs`, RFP-022 Sec.6 and F.8)\n\n\
         Bridge program image `{}`; checkpoint (rule B) `{}`; release `{}`. The vault on anvil (chain id 1), the bridge \
         program in LEZ v0.4.0-rc1's state machine, the two guests executed in RISC Zero dev mode; the mock verifier \
         accepts only the journals the guests committed. {} s in all.\n\n\
         After every step: vault balance == escrowed + owed; zone supply == minted - burnt; supply <= escrowed; and \
         supply + unminted deposits + unreleased burns == escrowed.\n\n\
         {cap_note}| seed | steps | deposits | imports | claims | claims capped (of them claimed later) | burns | releases | forgeries refused | \
         unminted at the end | unreleased at the end | invariant checks |\n|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n{report}\n\
         ## Anonymity sets of the claims (F.8)\n\nFor each claim, the set a private claim of its deposit would have at \
         that moment (`ferrata_wallet::claim::anonymity_set`, what `ferrata claim` shows for a private claim: the \
         deposits of the same token and amount under the root, on either side of a fee change, less those a plaintext \
         claim has published, applied or refused on chain: a refused transaction stays in its block, witness and all), \
         by denomination over every run. Every claim of these runs is plaintext, for which the client shows 1 (it names \
         its deposit), and each leaves every later set, as does a claim the bucket refused, whose deposit's own later \
         claim shows 1: \"as run\". Beside it, the same claims had every earlier claim been private, the default, \
         which publishes no deposit, refused or not: \"all private\". \
         Median: the lower and the upper middle of the sorted sets (the same when the count is odd).\n\n\
         | denomination (ETH) | claims | as run: smallest | median | largest | sets of 1 | all private: smallest | \
         median | largest | sets of 1 |\n|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n",
        harness::image_id_hex(hb::BRIDGE_ID),
        images.0,
        images.1,
        t0.elapsed().as_secs(),
        cap_note = match mint_cap {
            Some(cap) => format!(
                "ETH's mint bucket set to {cap} ETH after pairing (`SetCap` by the admin; refilled at 0.01 ETH per second \
                 of attested time), so that claims meet a spent bucket.\n\n"
            ),
            None => String::new(),
        }
    );
    // Smallest, median (lower and upper middle), largest and how many are 1, of one list of sets.
    let stats = |v: &mut Vec<usize>| -> String {
        v.sort_unstable();
        let n = v.len();
        let (lo, hi) = if n == 0 { (0, 0) } else { (v[(n - 1) / 2], v[n / 2]) };
        let median = if lo == hi { lo.to_string() } else { format!("{lo}, {hi}") };
        format!(
            "{} | {median} | {} | {}",
            v.first().copied().unwrap_or(0),
            v.last().copied().unwrap_or(0),
            v.iter().filter(|s| **s == 1).count()
        )
    };
    let (mut ones_run, mut ones_private, mut shrink, mut claims) = (0, 0, 0, 0);
    for (d, mut v) in all_sets {
        let mut w = all_private.remove(&d).unwrap_or_default();
        ensure!(v.len() == w.len(), "one pair of sets per claim");
        shrink += v.iter().zip(&w).filter(|(a, b)| a < b).count();
        ones_run += v.iter().filter(|s| **s == 1).count();
        ones_private += w.iter().filter(|s| **s == 1).count();
        claims += v.len();
        let _ = writeln!(out, "| {} | {} | {} | {} |", d as f64 / 1e18, v.len(), stats(&mut v), stats(&mut w));
    }
    let _ = writeln!(
        out,
        "\nSets of 1 (the claim's amount alone links it): {ones_run} of {claims} claims as run, {ones_private} of \
         {claims} had every earlier claim been private; the plaintext claims made {shrink} of the {claims} sets smaller."
    );
    println!("{out}");
    let name = match mint_cap {
        Some(cap) => format!("randomised-cap-{cap}eth.md"),
        None => "randomised.md".into(),
    };
    std::fs::write(results_dir().join(name), out)?;
    Ok(())
}

// ---- Sec.8, R.4, Sec.9: a migration with positions in flight, then the stop ---------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn migration_rehearsal_honours_positions_in_flight() -> Result<()> {
    if !guard()? {
        return Ok(());
    }
    let t0 = Instant::now();
    let checkpoint_elf = tool("FERRATA_CI_CHECKPOINT_B_ELF")?;
    let release_elf = tool("FERRATA_CI_RELEASE_ELF")?;
    let images = (image_of(&checkpoint_elf)?, image_of(&release_elf)?);
    let port = free_port()?;
    let rpc = format!("http://127.0.0.1:{port}");
    let _anvil = start_anvil(port)?;
    let p = wait_for(&rpc).await?;
    let dir = scratch("migration");
    let user = Seed::from_phrase(USER)?;
    let mut zone = new_zone(&user)?;
    let mut clock = Clock(9_300_000);
    let mut out = String::from(
        "# Migration rehearsal over both real halves (`cli/tests/both_halves.rs`, RFP-022 Sec.8, R.4, Sec.9)\n\n",
    );
    let mut line = |s: String| {
        println!("[{:>6.1} s] {s}", t0.elapsed().as_secs_f64());
        let _ = writeln!(out, "- {s}");
    };
    let mut log = Log::default();
    // The old vault A and the new vault B, each with its configuration (their deployers' first contracts).
    let mut a = deploy(&p, &mut zone, &mut clock, &rpc, &dir, "A", ANVIL0, images, false).await?;
    let mut b = deploy(&p, &mut zone, &mut clock, &rpc, &dir, "B", ANVIL2, images, true).await?;
    line(format!(
        "old vault A {} (configId {}), new vault B {} (configId {})",
        a.vault,
        hx(&a.ctx.cid),
        b.vault,
        hx(&b.ctx.cid)
    ));

    // Positions on A: d0 (1 ETH to /0) imported and claimed; d1 (0.1 ETH to /1) imported, not claimed; d2, d3 and d4
    // (1 ETH, 10 ETH, 0.1 ETH) never imported (d4 is left unrefunded until the stop). Burn b0 (0.1 ETH of d0's) before
    // the retirement.
    let d0 = deposit(&p, &mut a, &user, ANVIL1, 0, 1).await?;
    let d1 = deposit(&p, &mut a, &user, ANVIL1, 1, 0).await?;
    ensure!(import(&p, &mut zone, &mut clock, &mut a).await?, "import of d0 and d1");
    let d2 = deposit(&p, &mut a, &user, ANVIL3, 2, 1).await?;
    let d3 = deposit(&p, &mut a, &user, ANVIL3, 3, 2).await?;
    let d4 = deposit(&p, &mut a, &user, ANVIL3, 4, 0).await?;
    ensure!(claim(&p, &mut zone, &mut a, &user, d0, true, &mut log).await?.is_some(), "d0 claimed (no cap reached)");
    let holder0 = AccountId::new(user.destinations(8)?.of(DestKind::Public, 0));
    let recipient = Address::from_slice(&sha256(b"migration: recipient")[12..]);
    let b0 = burn(&mut zone, &mut a, &user, holder0, DENOMS[0], recipient).await?;
    invariants(&p, &zone, &mut a, &user).await?;
    line(format!("A: deposits d{d0} (claimed), d{d1} (imported, unclaimed), d{d2}, d{d3}, d{d4} (not imported); burn {b0} of 0.1 ETH"));

    // Retirement: the freeze authority retires A (deposits stop for good); the zone attests it and anyone runs Sunset.
    let fa = freeze_authority();
    impersonate(&p, fa).await?;
    anvil_send(&p, fa, a.vault, IRetire::retireCall {}.abi_encode(), U256::ZERO).await?;
    let c = inbound::commitment(&[7; 32], DestKind::Public, &[8; 32]);
    let price = eth::deposit_price(&p, a.vault, Address::ZERO, 0).await;
    let late = eth::deposit_calldata(Address::ZERO, 0, B256::from(c), 2).to_vec();
    let refused = p
        .call(
            TransactionRequest::default()
                .with_from(ANVIL1)
                .with_to(a.vault)
                .with_input(late)
                .with_value(U256::from(price.map(|x| x.total).unwrap_or(1))),
        )
        .await
        .is_err();
    ensure!(refused, "a deposit on the retired vault is refused");
    let s = clock.next();
    let retired = slot(&p, a.vault, ferrata_lez_bridge_wire::SLOT_RETIRED).await?;
    let blk = zone.world.add_block(hb::Block::with_storage(
        s,
        s,
        a.vault.into_array(),
        &[(ferrata_lez_bridge_wire::SLOT_RETIRED, retired)],
    ));
    let r = zone.world.attest_retired("A: AttestRetired", &a.ctx.cid, blk, &a.vault.into_array());
    zone.keep(r, "AttestRetired")?;
    let at = config_actor(zone.world.program, &a.ctx.cid);
    let r = zone.world.send(
        "A: Sunset (keyless: the vault is attested retired)",
        at,
        &BridgeMessage::Sunset { config_id: a.ctx.cid },
        vec![at],
    );
    zone.keep(r, "Sunset")?;
    let imported_before = a.imported;
    let refused_import = {
        let n = eth::view(&p, a.vault, IVault::nextLeafIndexCall {}, None).await?;
        let root = eth::view(&p, a.vault, IVault::depositRootCall {}, None).await?;
        let mut storage = Vec::new();
        for key in [word(0), word(1), word(2)] {
            storage.push((key, slot(&p, a.vault, key).await?));
        }
        let s = clock.next();
        let blk = zone.world.add_block(hb::Block::with_storage(s, s, a.vault.into_array(), &storage));
        let r = zone.world.import("A: ImportRoot after Sunset", &a.ctx.cid, blk, root.0, n, &a.vault.into_array());
        r.exit_code
    };
    ensure!(refused_import == Some(242), "no root is imported after Sunset: {refused_import:?}");
    line(format!("retire() by A's freeze authority; a deposit on A refused; AttestRetired and the keyless Sunset applied; ImportRoot after it refused (242); A's imported count stays {imported_before}"));

    // After Sunset: the imported, unclaimed d1 is still claimed (R.4); holders still burn.
    ensure!(claim(&p, &mut zone, &mut a, &user, d1, false, &mut log).await?.is_some(), "d1 claimed (no cap reached)");
    let holder1 = AccountId::new(user.destinations(8)?.of(DestKind::Public, 1));
    let b1 = burn(&mut zone, &mut a, &user, holder1, DENOMS[0], recipient).await?;
    invariants(&p, &zone, &mut a, &user).await?;
    line(format!("after Sunset: d{d1} claimed on LEZ; burn {b1} of 0.1 ETH"));

    // The sunset checkpoint: both burns, the imported count, sunset = true.
    let (root, count, imported, sunset) = checkpoint(&p, &zone, &a, &dir, &checkpoint_elf, images.0).await?;
    ensure!(
        sunset && imported == imported_before && count == 2,
        "the sunset checkpoint: sunset {sunset}, imported {imported}, burns {count}"
    );
    ensure!(
        eth::view(&p, a.vault, IRetire::sunsetLeafCountCall {}, None).await? == imported,
        "the refund boundary is the imported count"
    );
    advance(&p, 200).await?;
    release_burn(&p, &zone, &mut a, &user, b0, (root, count), &release_elf, images.1, true, &mut log).await?;
    invariants(&p, &zone, &mut a, &user).await?;
    line(format!(
        "sunset checkpoint finalised (burns {count}, imported {imported}); burn {b0} released on the retired vault"
    ));

    // Refunds of the leaves LEZ never imported, after the sunset checkpoint's revocation window (1 day).
    let (list, leaves, _) = rows(&p, &zone, &a, &user).await?;
    let tree = Tree::from_leaves(leaves).map_err(|_| anyhow!("tree"))?;
    let refund_call = |leaf: u64| -> Result<Vec<u8>> {
        let d = &list.deposits[leaf as usize];
        let path = tree.path(leaf).context("a path")?;
        Ok(IRetire::refundCall {
            leafIndex: leaf,
            depositor: Address::from(d.depositor),
            token: Address::ZERO,
            denomIndex: denom_index(ferrata_cli::units::word_to_u128(&d.amount)?)?,
            commitment: B256::from(d.commitment),
            path: path.siblings.map(B256::from),
        }
        .abi_encode())
    };
    ensure!(reverts(&p, ANVIL4, a.vault, refund_call(d2)?).await, "no refund inside the revocation window");
    advance(&p, 24 * 3600 + 1).await?;
    ensure!(reverts(&p, ANVIL4, a.vault, refund_call(d0)?).await, "an imported (claimed) leaf is never refunded");
    ensure!(
        reverts(&p, ANVIL4, a.vault, refund_call(d1)?).await,
        "an imported (claimed after Sunset) leaf is never refunded"
    );
    for leaf in [d2, d3] {
        let before = p.get_balance(ANVIL3).await?;
        anvil_send(&p, ANVIL4, a.vault, refund_call(leaf)?, U256::ZERO).await?;
        let escrow = a.unminted.remove(&leaf).context("an unminted deposit")?;
        ensure!(
            p.get_balance(ANVIL3).await? - before == U256::from(escrow),
            "the depositor gets the leaf's whole escrow back"
        );
        ensure!(reverts(&p, ANVIL4, a.vault, refund_call(leaf)?).await, "a second refund is refused");
        log.forged += 1;
    }
    invariants(&p, &zone, &mut a, &user).await?;
    line(format!("after the window: d{d2} and d{d3} refunded to their depositor (the whole escrow); refunds of d{d0} and d{d1} refused; second refunds refused"));

    // The new configuration takes new deposits: a deposit on B, imported, claimed; a burn from it.
    let e0 = deposit(&p, &mut b, &user, ANVIL1, 0, 1).await?;
    ensure!(import(&p, &mut zone, &mut clock, &mut b).await?, "import on B");
    ensure!(claim(&p, &mut zone, &mut b, &user, e0, true, &mut log).await?.is_some(), "e0 claimed (no cap reached)");
    let e_burn = burn(&mut zone, &mut b, &user, holder0, DENOMS[0], recipient).await?;
    invariants(&p, &zone, &mut b, &user).await?;
    invariants(&p, &zone, &mut a, &user).await?;
    line(format!("B: deposit d{e0} imported and claimed under B's configuration; burn {e_burn} under B"));

    // Sec.9: RISC Zero's emergency stop on A's verifier, as a proof of unsoundness would trip it (here its owner, the
    // impersonated freeze authority). Burn b1 is in A's finalised root but not released: it now cannot be.
    anvil_send(&p, fa, a.stop, IStop::estopCall {}.abi_encode(), U256::ZERO).await?;
    ensure!(eth::view(&p, a.stop, IStop::pausedCall {}, None).await?, "A's stop is tripped");
    let history = lez::history(&a.ctx, &zone.applied(a.ctx.program).await?);
    let released = eth::released_nullifiers(&p, a.vault, 0, 10_000).await?;
    let (burn_rows, _) = scan::burns(&user, &a.ctx, &history, &released);
    let row = burn_rows.iter().find(|r| r.owned.burn.burn_index == b1).context("burn b1")?;
    let rplan = release::plan(&a.ctx, &history, &row.owned, root.0, count, 0, [0; 20], 0)?;
    let proven = release::prove(&release_elf, &rplan)?;
    ensure!(B256::from(proven.image_id) == images.1, "the release guest");
    anvil_send(
        &p,
        ANVIL0,
        a.verifier,
        IMock::acceptCall { imageId: images.1, journalDigest: B256::from(sha256(&rplan.journal)) }.abi_encode(),
        U256::ZERO,
    )
    .await?;
    let call = release::calldata(&rplan, Bytes::from(SEAL.to_vec()));
    ensure!(reverts(&p, ANVIL0, a.vault, call.to_vec()).await, "after the stop, a proven release is refused");
    ensure!(reverts(&p, ANVIL4, a.vault, refund_call(d4)?).await, "after the stop, a refund is refused");
    let escrow_a = eth::view(&p, a.vault, IVault::escrowedCall { token: Address::ZERO }, None).await?;
    invariants(&p, &zone, &mut a, &user).await?;
    // B, with its own stop, goes on.
    let e1 = deposit(&p, &mut b, &user, ANVIL1, 2, 0).await?;
    ensure!(import(&p, &mut zone, &mut clock, &mut b).await?, "import on B after A's stop");
    ensure!(claim(&p, &mut zone, &mut b, &user, e1, false, &mut log).await?.is_some(), "e1 claimed (no cap reached)");
    invariants(&p, &zone, &mut b, &user).await?;
    line(format!(
        "Sec.9: A's emergency stop tripped: the proven release of burn {b1} and the refund of d{d4} refused; A's escrow {escrow_a} wei stays (burn {b1} unreleased, d{d4} unrefunded; the books still balance); B goes on (d{e1} claimed)"
    ));
    let _ = writeln!(
        out,
        "\nInvariant checks: A {} times, B {} times (vault balance == escrowed + owed; supply == minted - burnt; supply + \
         unminted + unreleased == escrowed). Forgeries refused: {}. {} s in all.",
        a.checked,
        b.checked,
        log.forged,
        t0.elapsed().as_secs()
    );
    std::fs::write(results_dir().join("migration.md"), out)?;
    Ok(())
}

// ---- F.13 and F.8: a fee change, attested on LEZ, paid on both halves ---------------------------------------------

/// The latest block's timestamp (anvil mines each transaction in a block of its own).
async fn now(p: &DynProvider) -> Result<u64> {
    Ok(p.get_block(alloy::eips::BlockId::latest()).await?.context("a block")?.header.timestamp)
}

#[tokio::test(flavor = "multi_thread")]
async fn fee_schedules_over_both_halves() -> Result<()> {
    if !guard()? {
        return Ok(());
    }
    let t0 = Instant::now();
    let checkpoint_elf = tool("FERRATA_CI_CHECKPOINT_B_ELF")?;
    let release_elf = tool("FERRATA_CI_RELEASE_ELF")?;
    let images = (image_of(&checkpoint_elf)?, image_of(&release_elf)?);
    let port = free_port()?;
    let rpc = format!("http://127.0.0.1:{port}");
    let _anvil = start_anvil(port)?;
    let p = wait_for(&rpc).await?;
    let dir = scratch("fees");
    let user = Seed::from_phrase(USER)?;
    let mut zone = new_zone(&user)?;
    let mut clock = Clock(9_400_000);
    let mut log = Log::default();
    let mut a = deploy(&p, &mut zone, &mut clock, &rpc, &dir, "A", ANVIL0, images, false).await?;
    let eth_token = Address::ZERO;
    let mut out =
        String::from("# Fee schedules over both real halves (`cli/tests/both_halves.rs`, RFP-022 F.13, F.8)\n\n");

    // Schedule 0 (the registration's: no protocol fee, the relay fee 0.1 % of each denomination): three deposits.
    let mut leaves = Vec::new();
    for (k, who) in [ANVIL1, ANVIL2, ANVIL3].into_iter().enumerate() {
        leaves.push(deposit(&p, &mut a, &user, who, k as u32, 1).await?);
    }
    invariants(&p, &zone, &mut a, &user).await?;

    // Schedule 1, announced by the admin (the deployer): 0.5 % mint, 0.2 % burn, 0.1 % checkpoint of each denomination.
    let pct = |per_mille: u128| -> Vec<U256> { DENOMS.iter().map(|d| U256::from(d / 1000 * per_mille)).collect() };
    let (mint, burn_fees, cp_fees) = (pct(5), pct(2), pct(1));
    let announce = IFees::announceFeesCall {
        token: eth_token,
        mintFees: mint.clone(),
        burnFees: burn_fees.clone(),
        checkpointFees: cp_fees.clone(),
    };
    anvil_send(&p, ANVIL0, a.vault, announce.abi_encode(), U256::ZERO).await?;
    let announced_at = now(&p).await?;
    ensure!(
        reverts(
            &p,
            ANVIL1,
            a.vault,
            IFees::announceFeesCall {
                token: eth_token,
                mintFees: mint.clone(),
                burnFees: burn_fees.clone(),
                checkpointFees: cp_fees.clone()
            }
            .abi_encode()
        )
        .await,
        "only the admin announces fees"
    );
    advance(&p, 7 * 24 * 3600 + 1).await?;
    anvil_send(&p, ANVIL4, a.vault, IFees::applyFeesCall { token: eth_token }.abi_encode(), U256::ZERO).await?;
    let effective_at = now(&p).await?;
    let from_leaf = eth::view(&p, a.vault, IVault::nextLeafIndexCall {}, None).await?;
    let to_u128 = |v: &[U256]| -> Vec<u128> { v.iter().map(|x| x.to::<u128>()).collect() };
    let s1 = FeeSchedule {
        index: 1,
        from_leaf,
        announced_at,
        effective_at,
        mint_fees: to_u128(&mint),
        burn_fees: to_u128(&burn_fees),
        checkpoint_fees: to_u128(&cp_fees),
    };
    let fee_key = fee_slot(&[0; 20], 1);
    ensure!(
        slot(&p, a.vault, fee_key).await? == s1.hash(&[0; 20]),
        "the vault published schedule 1 as the LEZ side hashes it"
    );
    let _ = writeln!(
        out,
        "Schedule 1 announced at {announced_at}, in force at {effective_at} (seven days and {} s later), from leaf {from_leaf}: \
         mint {:?}, burn {:?}, checkpoint {:?} wei for the denominations 0.1, 1 and 10 ETH.\n",
        effective_at - announced_at - 7 * 24 * 3600,
        s1.mint_fees,
        s1.burn_fees,
        s1.checkpoint_fees
    );

    // Schedule 1: three more deposits; the vault credits the new mint fee and escrows denomination + relay fee.
    for (k, who) in [ANVIL1, ANVIL2, ANVIL3].into_iter().enumerate() {
        leaves.push(deposit(&p, &mut a, &user, who, 3 + k as u32, 1).await?);
    }
    invariants(&p, &zone, &mut a, &user).await?;

    // LEZ: the root of all six, schedule 1 attested from the vault's own slot, and no schedule 2 at this count.
    let n = eth::view(&p, a.vault, IVault::nextLeafIndexCall {}, None).await?;
    let root = eth::view(&p, a.vault, IVault::depositRootCall {}, None).await?;
    let mut storage = Vec::new();
    for key in [word(0), word(1), word(2), fee_key, fee_slot(&[0; 20], 2)] {
        storage.push((key, slot(&p, a.vault, key).await?));
    }
    let sl = clock.next();
    let b = zone.world.add_block(hb::Block::with_storage(sl, sl, a.vault.into_array(), &storage));
    let cid = a.ctx.cid;
    let r = zone.world.import("A: ImportRoot n = 6", &cid, b, root.0, n, &a.vault.into_array());
    zone.keep(r, "ImportRoot")?;
    let early = FeeSchedule { effective_at: announced_at + 7 * 24 * 3600 - 1, ..s1.clone() };
    let r = zone.world.import_fees(
        "A: ImportFees ETH schedule 1, a second early (forged)",
        &cid,
        b,
        [0; 20],
        1,
        Some(early),
        &a.vault.into_array(),
    );
    ensure!(r.exit_code == Some(247), "a schedule the vault does not hold is refused (247): {:?}", r.exit_code);
    let r = zone.world.import_fees(
        "A: ImportFees ETH schedule 1",
        &cid,
        b,
        [0; 20],
        1,
        Some(s1.clone()),
        &a.vault.into_array(),
    );
    zone.keep(r, "ImportFees 1")?;
    let r =
        zone.world.import_fees("A: ImportFees ETH, no schedule 2", &cid, b, [0; 20], 2, None, &a.vault.into_array());
    zone.keep(r, "ImportFees 2")?;
    a.imported = n;

    // The set a private claim of each deposit has once the six are imported and none is claimed (a private claim
    // publishes nothing, so this is every private claim's): every 1 ETH deposit under the root, across the fee change.
    let mut pre_sets: BTreeMap<u64, usize> = BTreeMap::new();
    {
        let (list, leaves_now, rows_now) = rows(&p, &zone, &a, &user).await?;
        let history = lez::history_of(&zone, &a.ctx).await?;
        ensure!(history.opened.is_empty(), "no claim yet");
        for leaf in &leaves {
            let row = rows_now.iter().find(|r| r.owned.deposit.leaf_index == *leaf).context("the user's deposit")?;
            let plan = lez::plan_claim(&zone, &a.ctx, &leaves_now, &row.owned, &history, false, None, None).await?;
            let d = &row.owned.deposit;
            let set = ferrata_wallet::claim::anonymity_set(
                &list.deposits,
                &history.opened,
                &d.token,
                &d.amount,
                plan.leaf_count,
            );
            pre_sets.insert(*leaf, set);
        }
    }

    ensure!(
        pre_sets.values().all(|&set| set == 6),
        "the fee change does not split the pre-claim anonymity set: {pre_sets:?}"
    );

    // Claims: each mints denomination + relay fee; the vault took the mint fee at deposit.
    let mut table = String::from(
        "| leaf | schedule at deposit | relay fee (wei) | set before any claim | set at this claim |\n|---:|---:|---:|---:|---:|\n",
    );
    let fee_holder = hb::fee_recipient();
    let mut by_schedule: BTreeMap<u64, Vec<(u128, u128, usize)>> = BTreeMap::new();
    for (k, leaf) in leaves.iter().enumerate() {
        let before = zone.world.balance(fee_holder, &cid);
        let set = claim(&p, &mut zone, &mut a, &user, *leaf, k % 3 == 0, &mut log).await?.context("no cap reached")?;
        let schedule = u64::from(*leaf >= from_leaf);
        let relay_fee = zone.world.balance(fee_holder, &cid) - before;
        ensure!(
            relay_fee == 1_000_000 * 1_000_000_000,
            "the relay fee of 1 ETH (to the fee recipient: no relayer named); no mint fee on LEZ"
        );
        by_schedule.entry(schedule).or_default().push((0, relay_fee, pre_sets[leaf]));
        let _ = writeln!(table, "| {leaf} | {schedule} | {relay_fee} | {} | {set} |", pre_sets[leaf]);
        invariants(&p, &zone, &mut a, &user).await?;
    }
    for (schedule, claims) in &by_schedule {
        ensure!(
            claims.windows(2).all(|w| w[0] == w[1]),
            "schedule {schedule}: every claim pays the same fees and, before any claim, has the same set"
        );
    }
    out.push_str("## Claims (plaintext, 1 ETH each)\n\n");
    out.push_str(&table);
    let _ = writeln!(
        out,
        "\nEvery claim of 1 ETH mints the same fixed relay fee and no mint fee. Before any claim, a private claim of any \
         deposit has the same anonymity set ({} on both sides of the change; {} deposits of 1 ETH under the root): the \
         fee change does not split the set at leaf {from_leaf}. These claims are plaintext, so each publishes its deposit \
         and the next claim has one fewer (\"set at this claim\"); private claims would leave it whole.\n",
        by_schedule.values().map(|v| v[0].2.to_string()).collect::<Vec<_>>().join(" and "),
        leaves.len()
    );

    // Burns of 1 ETH from three holders; one checkpoint; every release under schedule 1.
    let mut burns = Vec::new();
    for k in 0..3u32 {
        let holder = AccountId::new(user.destinations(8)?.of(DestKind::Public, k));
        let to = Address::from_slice(&sha256(&[b"fees: recipient".as_slice(), &k.to_be_bytes()].concat())[12..]);
        burns.push(burn(&mut zone, &mut a, &user, holder, DENOMS[1], to).await?);
    }
    invariants(&p, &zone, &mut a, &user).await?;
    let (broot, count, _, _) = checkpoint(&p, &zone, &a, &dir, &checkpoint_elf, images.0).await?;
    advance(&p, 200).await?;
    let owed_before = eth::view(&p, a.vault, IRetire::totalOwedCall { token: eth_token }, None).await?;
    let fees_each = s1.burn_fees[1] + s1.checkpoint_fees[1];
    let mut paid = Vec::new();
    for (k, index) in burns.iter().enumerate() {
        paid.push(
            release_paying(
                &p,
                &zone,
                &mut a,
                &user,
                *index,
                (broot, count),
                (release_elf.as_path(), images.1),
                k == 0,
                &mut log,
                fees_each,
            )
            .await?,
        );
        invariants(&p, &zone, &mut a, &user).await?;
    }
    let owed_after = eth::view(&p, a.vault, IRetire::totalOwedCall { token: eth_token }, None).await?;
    ensure!(
        owed_after - owed_before == U256::from(fees_each * burns.len() as u128),
        "the burn and checkpoint fees credited on the vault"
    );
    ensure!(paid.windows(2).all(|w| w[0] == w[1]), "every release of 1 ETH pays the same");
    let _ = writeln!(
        out,
        "## Releases (1 ETH each, under schedule 1)\n\nEach recipient received {} wei: 1 ETH less the burn fee {} and the \
         checkpoint fee {}, credited on the vault to the fee recipient and to the checkpoint's prover (the vault's \
         `totalOwed` rose by {} wei over {} releases).\n\nInvariant checks: {} (vault balance == escrowed + owed; supply == \
         minted - burnt; supply + unminted + unreleased == escrowed). Forgeries refused: {}. {} s in all.",
        paid[0],
        s1.burn_fees[1],
        s1.checkpoint_fees[1],
        owed_after - owed_before,
        burns.len(),
        a.checked,
        log.forged,
        t0.elapsed().as_secs()
    );
    println!("{out}");
    std::fs::write(results_dir().join("fees.md"), out)?;
    Ok(())
}
