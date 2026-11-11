//! End to end: the vault on a local anvil (chain id 1, never a public network), the zone in the
//! LEZ harness (LEZ's state transition with the bridge guest, `RISC0_DEV_MODE=1`).
//!
//! Ethereum side through the `ferrata` binary itself (deposits signed from a keystore file, the
//! scan from the anvil RPC) and the library; LEZ side through the library's calls, checked with the
//! program's own functions, then settled by the harness (`HarnessLedger` below: the harness signs
//! and assembles the transaction the way LEZ's wallet does on a sequencer).
//!
//! Stand-ins, named: the root import, the configuration's `Init` and the token's `Register` are
//! the relayer's and run through the harness's own calls, from a synthetic execution block that
//! holds the anvil vault's real storage (read with eth_getStorageAt); the checkpoint and the
//! Groth16 proofs are the vault tests' MockRiscZeroVerifier accepting the exact journals (the test
//! acts as the checkpoint prover and the wrapper, as `VaultTestBase._acceptRootOn` does).
//!
//! Runs only with `FERRATA_E2E=1`, `anvil` and `forge` on the path, and `vault/lib` installed
//! (`cli/tests/e2e.sh` does all of it).

use std::{
    collections::{HashMap, HashSet},
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
use ferrata_bridge_core::{inbound::DestKind, outbound::burn_leaf, sha256, tree::Tree, Hash};
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

/// BIP-39 test phrases (published test values, never wallets): the user, and another user.
const USER: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
const OTHER: &str = "legal winner thank year wave sausage worth useful legal winner thank yellow";
/// anvil's first default account (unlocked on anvil; it signs nothing of the user's).
const ANVIL0: Address = alloy::primitives::address!("f39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
const SEAL: [u8; 4] = [0x5e, 0xa1, 0x5e, 0xa1];

// ---- the zone: the harness behind the tool's `Ledger` ------------------------------------------

struct HarnessLedger {
    world: hb::World,
    applied: Vec<Applied>,
    keys: HashMap<AccountId, PrivateKey>,
}

impl HarnessLedger {
    /// Keeps the last settled transaction's root call if it applied.
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
        let signers: Vec<&PrivateKey> = call
            .mentions
            .iter()
            .filter_map(|m| match m {
                Mention::Signer(a) => Some(self.keys.get(&a.account_id).expect("the holder's key is the seed's")),
                _ => None,
            })
            .collect();
        let signers: Vec<PrivateKey> = signers.into_iter().cloned().collect();
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
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp/cli/e2e"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
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

/// An unlocked anvil account sends a transaction (stand-in roles: deployer, checkpoint relayer).
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

/// Runs the `ferrata` binary; returns its stdout.
fn ferrata(args: &[&str]) -> Result<String> {
    let out = Command::new(env!("CARGO_BIN_EXE_ferrata")).args(args).output()?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    ensure!(out.status.success(), "ferrata {}: {}\n{stdout}", args.join(" "), String::from_utf8_lossy(&out.stderr));
    Ok(stdout)
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

/// Stands in for the checkpoint prover and relayer: candidate, wait, beacon, finalisation.
async fn accept_burn_root(p: &DynProvider, vault: Address, cid: B256, burn_root: Hash, burn_count: u64) -> Result<()> {
    let chain_id = p.get_chain_id().await?;
    let mock_image = eth::view(p, vault, ICheckpoints::checkpointImageIdCall {}, None).await?;
    let verifier = std::env::var("FERRATA_E2E_VERIFIER")?.parse::<Address>()?;
    let parent = eth::view(p, vault, ICheckpoints::finalStateCall {}, None).await?;
    let mut c = ICheckpoints::Checkpoint {
        nMin: 3,
        tMin: 2,
        parentState: parent,
        newState: B256::from(sha256(&[b"cli e2e state".as_slice(), &burn_root].concat())),
        burnRoot: B256::from(burn_root),
        burnCount: burn_count,
        importedLeafCount: 0,
        sunset: false,
        echoNonce: B256::ZERO,
        tipId: 1000 + burn_count,
        tipHash: B256::from(sha256(&[b"cli e2e tip".as_slice(), &burn_root].concat())),
        n: 4,
        t: 3,
    };
    let accept =
        |j: &[u8]| IMock::acceptCall { imageId: mock_image, journalDigest: B256::from(sha256(j)) }.abi_encode();
    anvil_send(p, verifier, accept(&checkpoint_journal(chain_id, vault, cid, &c)), U256::ZERO).await?;
    anvil_send(
        p,
        vault,
        ICheckpoints::submitCandidateCall { c: c.clone(), seal: Bytes::from(SEAL.to_vec()) }.abi_encode(),
        U256::ZERO,
    )
    .await?;
    // W = delta + (t - 1) * turnSeconds = 600 + 2 * 2.
    p.raw_request::<_, serde_json::Value>("evm_increaseTime".into(), (700u64,)).await?;
    p.raw_request::<_, serde_json::Value>("evm_mine".into(), Vec::<u64>::new()).await?;
    anvil_send(p, vault, ICheckpoints::recordBeaconCall {}.abi_encode(), U256::ZERO).await?;
    let index = eth::view(p, vault, ICheckpoints::beaconCountCall {}, None).await? - U256::from(1);
    let beacon = eth::view(p, vault, ICheckpoints::beaconsCall { i: index }, None).await?;
    c.echoNonce = beacon.nonce;
    anvil_send(p, verifier, accept(&checkpoint_journal(chain_id, vault, cid, &c)), U256::ZERO).await?;
    anvil_send(
        p,
        vault,
        ICheckpoints::finalizeCall { beaconIndex: index, c, seal: Bytes::from(SEAL.to_vec()) }.abi_encode(),
        U256::ZERO,
    )
    .await?;
    // The token's release delay (120 s).
    p.raw_request::<_, serde_json::Value>("evm_increaseTime".into(), (200u64,)).await?;
    p.raw_request::<_, serde_json::Value>("evm_mine".into(), Vec::<u64>::new()).await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn deposit_scan_claim_burn_release_and_recovery() -> Result<()> {
    if std::env::var("FERRATA_E2E").is_err() {
        eprintln!("skipped: FERRATA_E2E is not set (cli/tests/e2e.sh runs it)");
        return Ok(());
    }
    let dir = scratch();
    let port = 18545u16;
    let rpc = format!("http://127.0.0.1:{port}");
    let _anvil = Anvil(
        Command::new("anvil")
            .args(["--port", &port.to_string(), "--chain-id", "1", "--silent", "--code-size-limit", "100000"])
            .stdout(Stdio::null())
            .spawn()
            .context("starting anvil")?,
    );
    let p = wait_for(&rpc).await?;

    // The zone, and the configuration of the vault anvil's first account is about to deploy.
    let mut zone = HarnessLedger { world: hb::World::new(), applied: Vec::new(), keys: HashMap::new() };
    let program = zone.world.program;
    // The deployer's contracts: the linked FerrataRegistry library, the mock, its emergency stop, the vault.
    let vault = ANVIL0.create(3);
    let config = hb::config(vault.into_array(), hb::controls());
    let cid = config.id(program);
    let out = Command::new("forge")
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../vault"))
        .args(["script", "script/CliLocal.s.sol:CliLocal", "--rpc-url", &rpc, "--broadcast", "--unlocked", "--sender"])
        .arg(ANVIL0.to_string())
        .env("FERRATA_CID", hx(&cid))
        .output()?;
    ensure!(out.status.success(), "forge script: {}", String::from_utf8_lossy(&out.stderr));
    ensure!(!p.get_code_at(vault).await?.is_empty(), "the vault is not at the predicted address");
    std::env::set_var("FERRATA_E2E_VERIFIER", ANVIL0.create(1).to_string());
    let registered_at = p.get_block(alloy::eips::BlockId::latest()).await?.expect("a block").header.timestamp;
    println!("anvil: vault {vault}, configId {}", hx(&cid));

    // The relayer's part on LEZ: Init and Register from the vault's own storage.
    let reg = registry_slot(&[0; 20]);
    let fee0 = fee_slot(&[0; 20], 0);
    let mut storage = Vec::new();
    for key in [word(2), reg, fee0] {
        storage.push((key, slot(&p, vault, key).await?));
    }
    let stored = eth::view(&p, vault, IVault::lezConfigIdCall {}, None).await?;
    ensure!(
        storage[0].1 == cid,
        "the vault's slot 2 is not the configuration's id: slot 2 {} lezConfigId {stored} cid {}",
        hx(&storage[0].1),
        hx(&cid)
    );
    let b0 = zone.world.add_block(hb::Block::with_storage(9_000_000, 9_000_000, vault.into_array(), &storage));
    let r = zone.world.init("Init (relayer)", &config, b0);
    ensure!(r.ok(), "Init: {:?}", r.exit_code);
    zone.record(&r);
    let entry = hb::eth_entry();
    let schedule = FeeSchedule {
        index: 0,
        from_leaf: 0,
        announced_at: registered_at,
        effective_at: registered_at,
        mint_fees: vec![0; 3],
        burn_fees: vec![0; 3],
        checkpoint_fees: vec![0; 3],
    };
    let r = zone.world.register_with("Register ETH (relayer)", &cid, b0, &entry, &schedule, &vault.into_array());
    ensure!(r.ok(), "Register: {:?}", r.exit_code);
    zone.record(&r);

    // Keystores: the user's depositor and another user's, funded by anvil's account.
    let password = dir.join("password.txt");
    std::fs::write(&password, "e2e test password\n")?;
    let mut rng = rand::thread_rng();
    let mut keystores = Vec::new();
    for name in ["user", "other"] {
        let path = dir.join(format!("keystore-{name}.json"));
        let _ = std::fs::remove_file(&path);
        let (signer, _) = PrivateKeySigner::new_keystore(
            &dir,
            &mut rng,
            "e2e test password",
            Some(&format!("keystore-{name}.json")),
        )?;
        anvil_send(&p, signer.address(), vec![], U256::from(100u64) * U256::from(10u64).pow(U256::from(18u64))).await?;
        keystores.push((path, signer.address()));
    }
    for (phrase, name) in [(USER, "user"), (OTHER, "other")] {
        std::fs::write(dir.join(format!("seed-{name}.txt")), phrase)?;
    }
    let seed_user = dir.join("seed-user.txt");
    let common = |seed: &Path| -> Vec<String> {
        vec![
            "--seed".into(),
            seed.display().to_string(),
            "--eth-rpc".into(),
            rpc.clone(),
            "--vault".into(),
            vault.to_string(),
        ]
    };
    let deposit = |seed: &Path, keystore: &Path, denom: u8, kind: &str, account: u32| -> Result<String> {
        let mut args = common(seed);
        args.extend(
            [
                "deposit",
                "--denomination",
                &denom.to_string(),
                "--kind",
                kind,
                "--account",
                &account.to_string(),
                "--keystore",
                &keystore.display().to_string(),
                "--password-file",
                &password.display().to_string(),
                "--send",
                "--yes",
            ]
            .map(String::from),
        );
        ferrata(&args.iter().map(String::as_str).collect::<Vec<_>>())
    };

    // Deposits, through the binary: the user's 10 ETH to public account /0 (claimed below), ten
    // more of 0.1 ETH to accounts of both kinds (left unclaimed), and five of another user's.
    let user_ks = &keystores[0].0;
    let other_ks = &keystores[1].0;
    let first = deposit(&seed_user, user_ks, 2, "public", 0)?;
    print!("{first}");
    let mut expected_unclaimed = Vec::new();
    for k in 0..10u32 {
        if k % 2 == 0 {
            deposit(&dir.join("seed-other.txt"), other_ks, 0, "private", k % 3)?;
        }
        let kind = if k % 2 == 0 { "private" } else { "public" };
        deposit(&seed_user, user_ks, 0, kind, k % 8)?;
        expected_unclaimed.push((kind, k % 8));
    }
    // The vault refuses a deposit with a stale j: the tool reports the vault's error.
    let refused = {
        let signer = eth::keystore_signer(user_ks, &password)?;
        let sp = eth::signing_provider(&rpc, signer)?;
        let price = eth::deposit_price(&p, vault, eth::ETH, 0).await?;
        let input = eth::deposit_calldata(eth::ETH, 0, B256::repeat_byte(7), 0);
        eth::send(&sp, keystores[0].1, vault, input, U256::from(price.total), 1).await.expect_err("a stale j")
    };
    println!("stale j: {refused}");
    ensure!(refused.to_string().contains("WrongDepositIndex"), "the vault's error is named");

    // Scan through the binary, Ethereum only (deposit list checked against depositRoot).
    let scan_out =
        ferrata(&[common(&seed_user), vec!["scan".into()]].concat().iter().map(String::as_str).collect::<Vec<_>>())?;
    print!("{scan_out}");
    ensure!(scan_out.contains("deposits of this seed: 11"), "the binary's scan finds the user's 11 deposits");

    // Status through the binary, Ethereum part: the registry, fees and caps in the token's units.
    let status_out =
        ferrata(&[common(&seed_user), vec!["status".into()]].concat().iter().map(String::as_str).collect::<Vec<_>>())?;
    print!("{status_out}");
    ensure!(
        status_out.contains("registered true") && status_out.contains("16 deposits"),
        "the binary's status reads the vault"
    );

    // The relayer imports the vault's root (its storage after the deposits) on LEZ.
    let n = eth::view(&p, vault, IVault::nextLeafIndexCall {}, None).await?;
    let root = eth::view(&p, vault, IVault::depositRootCall {}, None).await?;
    let mut storage2 = vec![(word(0), slot(&p, vault, word(0)).await?), (word(1), slot(&p, vault, word(1)).await?)];
    storage2.push((word(2), cid));
    ensure!(storage2[0].1 == root.0, "slot 0 is depositRoot");
    let b1 = zone.world.add_block(hb::Block::with_storage(9_000_064, 9_000_064, vault.into_array(), &storage2));
    let r = zone.world.import("ImportRoot (relayer)", &cid, b1, root.0, n, &vault.into_array());
    ensure!(r.ok(), "ImportRoot: {:?}", r.exit_code);
    zone.record(&r);
    // The relayer attests that ETH has no fee schedule 1 at this count: the known range grows to it.
    let r =
        zone.world.import_fees("ImportFees ETH 1 = none (relayer)", &cid, b1, [0; 20], 1, None, &vault.into_array());
    ensure!(r.ok(), "ImportFees: {:?}", r.exit_code);
    zone.record(&r);
    println!("zone: root({n}) imported; ETH fees known to leaf {n}");

    // The user: the tool's library on the zone.
    let user = Seed::from_phrase(USER)?;
    for a in user.public_accounts(8)? {
        zone.keys.insert(a.id, a.key.clone());
    }
    let ctx = Context { program, cid, chain_id: 1, vault: vault.into_array() };
    let list = eth::deposit_list(&p, vault, 0, 10_000).await?;
    let history = lez::history(&ctx, &zone.applied(program).await?);
    let rows = scan::deposits(&user, &ctx, &list, 8, &history, &HashSet::new())?;
    ensure!(rows.len() == 11, "11 deposits of the user, found {}", rows.len());
    let big = rows.iter().find(|r| r.owned.deposit.leaf_index == 0).expect("leaf 0 is the user's");
    ensure!(big.owned.kind == DestKind::Public && big.owned.account_index == 0, "leaf 0 is to public /0");
    let leaves: Vec<Hash> = list
        .deposits
        .iter()
        .map(|d| {
            ferrata_bridge_core::inbound::deposit_leaf(d.leaf_index, &d.depositor, &d.token, &d.amount, &d.commitment)
        })
        .collect();

    // The gas drop (bridge.md §6, v3 D13), with `FERRATA_GASDROP_ELF` (cli/tests/relayer.sh): the user
    // proves, for real (not dev mode), a receipt that leaf 0 has nullifier N under the imported root,
    // bound to a one-time address and the relayer's LEZ account; the relayer checks it against the zone
    // (imported roots, claimed nullifiers) and the vault's deposit list and sends one credit per N; a
    // replay to another relayer and a second drop are refused; the claim below names the relayer.
    let mut gas_drop: Option<(Vec<u8>, ferrata_cli::relayer::DropContext<'static>, [u8; 32])> = None;
    if let Ok(elf_path) = std::env::var("FERRATA_GASDROP_ELF") {
        use ferrata_bridge_core::encoding::ClaimInput;
        use ferrata_bridge_core::gasdrop::GasDropInput;
        use ferrata_bridge_core::inbound::ClaimWitness;
        use ferrata_cli::relayer::{self as rl, DropBook, DropContext, DropRefusal};
        struct Credits(Vec<([u8; 32], u128)>);
        impl rl::Credit for Credits {
            async fn credit(&mut self, to: [u8; 32], amount: u128) -> Result<String> {
                // Stand-in: the harness does not model LEZ's native token program; the sequencer
                // implementation (`impl Credit for lez::Sequencer`) sends a native transfer.
                self.0.push((to, amount));
                Ok(format!("native credit {} (harness stand-in)", self.0.len()))
            }
        }
        let relayer_lez = *Seed::from_phrase(OTHER)?.public_accounts(1)?[0].id.value();
        let d = &big.owned.deposit;
        let tree = Tree::from_leaves(leaves.clone()).map_err(|_| anyhow!("tree"))?;
        let input = GasDropInput {
            claim: ClaimInput {
                root: tree.root(),
                chain_id: word(1),
                vault: vault.into_array(),
                witness: ClaimWitness {
                    depositor: d.depositor,
                    token: d.token,
                    amount: d.amount,
                    leaf_index: d.leaf_index,
                    path: tree.path(d.leaf_index).context("a path")?,
                    s: big.owned.s,
                    kind: big.owned.kind,
                    dest: big.owned.dest,
                },
            },
            one_time_address: sha256(b"cli e2e: the claim's one-time address"),
            relayer: relayer_lez,
        };
        let elf = std::fs::read(&elf_path).with_context(|| elf_path.clone())?;
        let image = risc0_zkvm::compute_image_id(&elf).map_err(|e| anyhow!("{e}"))?;
        let env =
            risc0_zkvm::ExecutorEnv::builder().write_slice(&input.encode()).build().map_err(|e| anyhow!("{e}"))?;
        // The zone's guests run in dev mode; this receipt is a real one.
        std::env::remove_var("RISC0_DEV_MODE");
        let started = std::time::Instant::now();
        let proven = risc0_zkvm::default_prover().prove_with_opts(env, &elf, &risc0_zkvm::ProverOpts::succinct());
        std::env::set_var("RISC0_DEV_MODE", "1");
        let info = proven.map_err(|e| anyhow!("r0vm: {e}"))?;
        println!(
            "gas-drop receipt: real succinct proof in {:.1} s ({} user cycles)",
            started.elapsed().as_secs_f64(),
            info.stats.user_cycles
        );
        let receipt_json = serde_json::to_vec(&info.receipt)?;
        let history = lez::history_of(&zone, &ctx).await?;
        let roots: &'static HashSet<Hash> = Box::leak(Box::new(rl::imported_roots(&list.deposits, &history.roots)?));
        ensure!(roots.contains(&tree.root()), "the receipt's root is the zone's imported root");
        let mut image_id = [0u8; 32];
        image_id.copy_from_slice(image.as_bytes());
        let dctx = DropContext {
            image_id,
            me: relayer_lez,
            chain_id: word(1),
            vault: vault.into_array(),
            imported_roots: roots,
        };
        let book_path = dir.join("relayer-drops.json");
        let _ = std::fs::remove_file(&book_path);
        let mut book = DropBook::load(&book_path)?;
        let mut credits = Credits(Vec::new());
        let claimed = |n: &Hash| lez::is_claimed(&ctx, &history, n);
        let replay = rl::check_gas_drop(&receipt_json, &DropContext { me: [0x77; 32], ..dctx }, claimed, &book);
        ensure!(replay == Err(DropRefusal::OtherRelayer), "a receipt replayed to another relayer: {replay:?}");
        let (j, tx) =
            rl::gas_drop(&receipt_json, &dctx, claimed, &mut book, &book_path, &mut credits, 1_000_000, 0).await?;
        ensure!(j.nullifier == big.owned.nullifier, "the receipt's N is leaf 0's");
        let second =
            rl::gas_drop(&receipt_json, &dctx, claimed, &mut book, &book_path, &mut credits, 1_000_000, 1).await;
        ensure!(second.is_err() && credits.0.len() == 1, "one drop per N");
        println!(
            "GAS DROP: N {} -> one-time address {} ({tx}); replay to another relayer and a second drop refused",
            hx(&j.nullifier),
            hx(&j.one_time_address)
        );
        gas_drop = Some((receipt_json, dctx, relayer_lez));
    }

    // Claim leaf 0 (plaintext: a public destination, no proof).
    let plan =
        lez::plan_claim(&zone, &ctx, &leaves, &big.owned, &history, false, gas_drop.as_ref().map(|g| g.2)).await?;
    let sent = zone.submit(&plan.call).await?;
    println!("claim: {sent}; N {}", hx(&plan.nullifier));
    let holder = AccountId::new(big.owned.dest);
    ensure!(zone.world.balance(holder, &cid) == 10_000_000_000_000_000_000, "the holder holds 10 ETH on LEZ");
    if let Some((receipt_json, dctx, relayer_lez)) = &gas_drop {
        use ferrata_cli::relayer::{self as rl, DropBook, DropRefusal};
        let fee = zone.world.balance(AccountId::new(*relayer_lez), &cid);
        ensure!(
            fee == plan.relay_fee && fee > 0,
            "the claim minted the relay fee to the relayer: {fee} of {}",
            plan.relay_fee
        );
        let history = lez::history_of(&zone, &ctx).await?;
        let after =
            rl::check_gas_drop(receipt_json, dctx, |n| lez::is_claimed(&ctx, &history, n), &DropBook::default());
        ensure!(after == Err(DropRefusal::AlreadyClaimed), "a relayer with no record refuses a claimed N: {after:?}");
        println!("GAS DROP: the claim named the relayer, minted it the relay fee {fee}; afterwards any relayer refuses the receipt (claimed)");
    }

    // A second claim of the same deposit: the tool refuses it with the program's code, and the
    // program, sent the same call without the tool's check, exits with that code.
    let history = lez::history(&ctx, &zone.applied(program).await?);
    let again = lez::plan_claim(&zone, &ctx, &leaves, &big.owned, &history, false, None).await.expect_err("claimed");
    println!("second claim: {again}");
    ensure!(again.to_string().contains("250 ALREADY_CLAIMED"), "the tool names 250");
    let report = zone.settle(&plan.call);
    ensure!(report.exit_code == Some(250), "the program exits 250, got {:?}", report.exit_code);

    // Burns: eleven of 0.1 ETH from /0, released to the depositor's address.
    let keys = user.bridge_keys();
    let recipient = keystores[0].1;
    for _ in 0..11 {
        let history = lez::history(&ctx, &zone.applied(program).await?);
        let plan = lez::plan_burn(
            &zone,
            &ctx,
            &keys,
            holder,
            [0; 20],
            100_000_000_000_000_000,
            recipient.into_array(),
            &history,
        )
        .await?;
        zone.submit(&plan.call).await?;
    }
    ensure!(
        zone.world.balance(holder, &cid) == 10_000_000_000_000_000_000 - 1_100_000_000_000_000_000,
        "11 burns debited"
    );
    // More than the holding: refused before sending with 254.
    let history = lez::history(&ctx, &zone.applied(program).await?);
    let too_much = lez::plan_burn(
        &zone,
        &ctx,
        &keys,
        holder,
        [0; 20],
        10_000_000_000_000_000_000,
        recipient.into_array(),
        &history,
    )
    .await
    .expect_err("more than held");
    ensure!(too_much.to_string().contains("254 INSUFFICIENT_BALANCE"), "{too_much}");

    // The checkpoint of all eleven burns (stand-in), then the release of the first.
    let history = lez::history(&ctx, &zone.applied(program).await?);
    ensure!(history.burns.len() == 11, "11 burns published");
    let burn_leaves: Vec<Hash> = history
        .burns
        .iter()
        .map(|b| burn_leaf(&cid, b.burn_index, &b.note.token, &b.note.amount, &b.note.c_out, &b.note.r))
        .collect();
    let burn_root = Tree::from_leaves(burn_leaves).map_err(|_| anyhow!("tree"))?.root();
    // The relayer's burn watch (with FERRATA_GASDROP_ELF): 11 burns on the zone, none accepted, no candidate
    // live: a checkpoint is needed; once the root of all 11 is accepted, none is.
    let watch = ferrata_cli::relayer::Relayer { vault, me: ANVIL0, accept_unpaid: false };
    if gas_drop.is_some() {
        let accepted = eth::accepted_burn_roots(&p, vault, 0, 10_000).await?;
        let live = watch.candidates(&p, 0).await?.len();
        let need = ferrata_cli::relayer::checkpoint_needed(history.burns.len() as u64, &accepted, live);
        ensure!(need == Some((11, 0)), "the burn watch asks for a checkpoint of 11 burns: {need:?}");
    }
    accept_burn_root(&p, vault, B256::from(cid), burn_root, 11).await?;
    if gas_drop.is_some() {
        let accepted = eth::accepted_burn_roots(&p, vault, 0, 10_000).await?;
        let live = watch.candidates(&p, 0).await?.len();
        let need = ferrata_cli::relayer::checkpoint_needed(history.burns.len() as u64, &accepted, live);
        ensure!(need.is_none() && live == 0, "no checkpoint needed once 11 burns are accepted: {need:?}, {live} live");
        println!("BURN WATCH: 11 burns on the zone -> checkpoint needed (0 accepted); after acceptance -> none");
    }
    let released = eth::released_nullifiers(&p, vault, 0, 10_000).await?;
    let (burn_rows, next_i) = scan::burns(&user, &ctx, &history, &released);
    ensure!(burn_rows.len() == 11 && next_i == 11, "11 burns of the user, next counter 11");
    let roots = eth::accepted_burn_roots(&p, vault, 0, 10_000).await?;
    let (root_b, count) = *roots.last().expect("an accepted burn root");
    ensure!(root_b.0 == burn_root && count == 11, "the vault accepted the burn root");
    let rplan = release::plan(&ctx, &history, &burn_rows[0].owned, root_b.0, count, 0, [0; 20], 0)?;
    let release_image = eth::view(&p, vault, ICheckpoints::releaseImageIdCall {}, None).await?;
    let verifier: Address = std::env::var("FERRATA_E2E_VERIFIER")?.parse()?;
    anvil_send(
        &p,
        verifier,
        IMock::acceptCall { imageId: release_image, journalDigest: B256::from(sha256(&rplan.journal)) }.abi_encode(),
        U256::ZERO,
    )
    .await?;
    let before = p.get_balance(recipient).await?;
    let relayer_signer = eth::keystore_signer(other_ks, &password)?;
    let sp = eth::signing_provider(&rpc, relayer_signer)?;
    let receipt =
        eth::send(&sp, keystores[1].1, vault, release::calldata(&rplan, Bytes::from(SEAL.to_vec())), U256::ZERO, 1)
            .await?;
    println!("release: {} in block {:?}", receipt.transaction_hash, receipt.block_number);
    ensure!(
        p.get_balance(recipient).await? - before == U256::from(100_000_000_000_000_000u128),
        "the recipient received 0.1 ETH"
    );
    ensure!(
        eth::view(&p, vault, IVault::nullifierUsedCall { nullifier: B256::from(rplan.nullifier) }, None).await?,
        "NOut used"
    );
    let twice =
        eth::send(&sp, keystores[1].1, vault, release::calldata(&rplan, Bytes::from(SEAL.to_vec())), U256::ZERO, 1)
            .await
            .expect_err("released");
    ensure!(twice.to_string().contains("NullifierAlreadyUsed"), "{twice}");

    // Recovery: a wiped client with only the seed phrase.
    let wiped = Seed::from_phrase(&std::fs::read_to_string(&seed_user)?)?;
    let list = eth::deposit_list(&p, vault, 0, 10_000).await?;
    let history = lez::history(&ctx, &zone.applied(program).await?);
    let refunded = eth::refunded_leaves(&p, vault, 0, 10_000).await?;
    let rows = scan::deposits(&wiped, &ctx, &list, 8, &history, &refunded)?;
    let unclaimed: Vec<_> = rows
        .iter()
        .filter(|r| !matches!(r.state, scan::DepositState::Claimed | scan::DepositState::Refunded))
        .collect();
    let claimed = rows.iter().filter(|r| r.state == scan::DepositState::Claimed).count();
    let released = eth::released_nullifiers(&p, vault, 0, 10_000).await?;
    let (burns, _) = scan::burns(&wiped, &ctx, &history, &released);
    let unreleased = burns.iter().filter(|b| !b.released).count();
    let found: Vec<(String, u32)> = unclaimed
        .iter()
        .map(|r| {
            (if r.owned.kind == DestKind::Private { "private" } else { "public" }.to_string(), r.owned.account_index)
        })
        .collect();
    let want: Vec<(String, u32)> = expected_unclaimed.iter().map(|(k, a)| (k.to_string(), *a)).collect();
    println!(
        "RECOVERY: {} of {} unclaimed deposits ({} claimed), {} of 10 unreleased burns ({} released); {} deposits in the vault",
        unclaimed.len(),
        want.len(),
        claimed,
        unreleased,
        burns.len() - unreleased,
        list.deposits.len()
    );
    ensure!(found == want, "the unclaimed deposits found are the ten made: {found:?}");

    // Status, the zone's part (the library function `ferrata status --sequencer` prints).
    let zone_lines = lez::zone_status(&zone, &ctx, &history).await?;
    for line in &zone_lines {
        println!("{line}");
    }
    ensure!(
        zone_lines
            .iter()
            .any(|l| l.contains(&format!("newest imported root: root(cid, {n})")) && l.contains("from header(id, ")),
        "status names the newest import and the record it read"
    );
    ensure!(zone_lines.iter().any(|l| l == "  burns published: 11"), "status counts the burns");

    // A private claim names its nullifier account in its public part. The harness settles no private
    // transaction here, so the history is given one such account: the scan then marks that deposit
    // claimed, matched locally from the account id (no per-item query).
    let target = unclaimed[0];
    let mut seen = history.clone();
    seen.touched_privately.insert(
        ferrata_lez_bridge_wire::nullifier_actor(program, 1, &vault.into_array(), &target.owned.nullifier).account_id,
    );
    let rows2 = scan::deposits(&wiped, &ctx, &list, 8, &seen, &refunded)?;
    let claimed2 = rows2.iter().filter(|r| r.state == scan::DepositState::Claimed).count();
    println!(
        "private claim seen: leaf {} marked claimed once a private transaction names its nullifier account ({claimed2} claimed)",
        target.owned.deposit.leaf_index
    );
    ensure!(claimed2 == 2, "the nullifier account named privately marks its deposit claimed");
    ensure!(claimed == 1 && unreleased == 10 && burns.len() == 11, "recovery");

    // The relayer's release (cli/src/relayer.rs), with `FERRATA_GASDROP_ELF`: the holder of burn 1 opts in
    // with a release that pays the relayer the registered relay fee; the relayer checks and sends it
    // and is paid; its own changed calls (recipient redirected) are refused by the vault, and another
    // relayer refuses the job.
    if gas_drop.is_some() {
        use ferrata_cli::relayer::{self as rl, Relayer};
        let relayer_eth = keystores[1].1;
        let fee = eth::deposit_price(&p, vault, eth::ETH, 0).await?.relay_fee;
        let plan1 =
            release::plan(&ctx, &history, &burn_rows[1].owned, root_b.0, count, 0, relayer_eth.into_array(), fee)?;
        let verifier: Address = std::env::var("FERRATA_E2E_VERIFIER")?.parse()?;
        anvil_send(
            &p,
            verifier,
            IMock::acceptCall { imageId: release_image, journalDigest: B256::from(sha256(&plan1.journal)) }
                .abi_encode(),
            U256::ZERO,
        )
        .await?;
        let sp = eth::signing_provider(&rpc, eth::keystore_signer(other_ks, &password)?)?;
        let r = Relayer { vault, me: relayer_eth, accept_unpaid: false };
        let view = r.view(&sp).await?;
        let (req, input) = r.plan_release(&sp, &view, &plan1.journal, Bytes::from(SEAL.to_vec())).await?;
        let mut redirected = req.clone();
        redirected.recipient = relayer_eth;
        let refused = eth::send(
            &sp,
            relayer_eth,
            vault,
            eth::release_calldata(redirected, Bytes::from(SEAL.to_vec())),
            U256::ZERO,
            1,
        )
        .await
        .expect_err("a redirected release");
        let other = Relayer { vault, me: ANVIL0, accept_unpaid: false };
        let not_ours = other
            .plan_release(&p, &view, &plan1.journal, Bytes::from(SEAL.to_vec()))
            .await
            .expect_err("another relayer's");
        let before = p.get_balance(relayer_eth).await?;
        let rc = r.submit(&sp, input).await?;
        let (_, paid) = rl::relay_fee_paid(&rc, relayer_eth).context("a Release paying the relayer")?;
        let gas = U256::from(rc.gas_used) * U256::from(rc.effective_gas_price);
        let gained = p.get_balance(relayer_eth).await? + gas - before;
        ensure!(
            fee > 0 && paid == U256::from(fee) && gained == U256::from(fee),
            "the relayer was paid {gained}, the fee is {fee}"
        );
        println!("RELAYER: burn 1 released by the relayer ({} gas), relay fee {fee} wei paid to it; redirected recipient refused: {refused}; another relayer: {not_ours}", rc.gas_used);
    }
    Ok(())
}
