//! Host for the SPEC-v2 guests. Builds the end-to-end scenario, executes, proves a succinct receipt,
//! wraps it to Groth16 (RISC Zero's own pipeline: r0vm 3.0.5, then Docker), verifies, and writes
//! `artifacts/` and the Foundry fixtures. Every number printed comes from the run it reports.
//!
//! ```text
//! host scenario                     write artifacts/scenario.json
//! host execute                      executor only: cycles of the three release cases and the claim guest, a bad witness
//! host invariance [n]               n random valid witnesses per guest: the cycle count must not change
//! host prove <eth|erc20|ethpull>    succinct receipt -> artifacts/<case>/receipt-succinct.bin
//! host p254 <case>                  identity_p254 alone (the STARK-to-BN254 recursion before Groth16)
//! host groth16 <case>               saved succinct receipt -> Groth16 receipt, seal, journal
//! host verify <case>                verify saved receipts, print selector and verifier parameters
//! host fixtures                     artifacts -> evm-verify/test/fixtures/v2/*.json
//! host claim-check [n]              claim guest vs. native verify_claim, n deposits
//! host prove-claim                  one succinct proof of the claim guest (timing)
//!
//! FERRATA_DIAG=1 host invariance prints popcount(direction bits) next to each release cycle count.
//! ```
//!
//! Proving runs in the r0vm 3.0.5 subprocess (risc0-zkvm `client` feature); measure peak memory
//! around this process with `/usr/bin/time -v` (scripts/prove-bench.sh), which includes r0vm.

use std::{collections::BTreeSet, fs, path::PathBuf, time::Instant};

use anyhow::{bail, ensure, Context, Result};
use ferrata_bridge_core::checkpoint::{burn_frontier_hash, key_set_hash, LezState};
use ferrata_bridge_core::encoding::{
    claim_journal, config_id_preimage, u256_from_u128, ClaimInput, ConfigIdentity, ReleaseInput, ReleaseJournal,
};
use ferrata_bridge_core::inbound::{self, ClaimWitness, DestKind};
use ferrata_bridge_core::outbound::{
    burn_commitment, burn_leaf, recover_burns, release_journal, verify_release, BurnNote, PublishedBurn, ReleasePublic,
    ReleaseTarget, ReleaseWitness,
};
use ferrata_bridge_core::tree::{root_from_path, IncrementalTree, MerklePath, Tree};
use ferrata_bridge_core::{chain_id_u256, sha256, Address, BridgeKeys, Hash, U256};
use methods::{CLAIM_ELF, CLAIM_ID, RELEASE_ELF, RELEASE_ID};
use risc0_zkvm::{
    default_executor, default_prover, ApiClient, Asset, AssetRequest, Bytes, Digest, ExecutorEnv,
    Groth16ReceiptVerifierParameters, InnerReceipt, ProverOpts, Receipt, SessionInfo, SuccinctReceipt,
};
use serde_json::json;

// ---- the end-to-end scenario --------------------------------------------------------------------
// Foundry and anvil default to chain id 31337. The deployer is anvil's well-known test account 0;
// evm-verify deploys, in this order, RiscZeroGroth16Verifier (nonce 0), RiscZeroVerifierEmergencyStop
// (nonce 1), the test token (nonce 2) and the v2 vault (nonce 3), and asserts these addresses.
const CHAIN: u64 = 31_337;
const DEPLOYER: &str = "f39fd6e51aad88f6f4ce6ab8827279cfffb92266";
const GROTH16: &str = "5fbdb2315678afecb367f032d93f642f64180aa3";
const ESTOP: &str = "e7f1725e7734ce288f8367e1bb143e90bb3f0512";
const TOKEN: &str = "9fe46736679d2d9a65f0992f2272de9f3c7fa6e0";
const VAULT: &str = "cf7ed3acca5a467e9e704c703e8d87f634fb0fc9";
const ETH: Address = [0u8; 20];
/// Deployment constants of the checkpoint rule (§4.2), in the checkpoint journal.
const N_MIN: u32 = 4;
const T_MIN: u32 = 3;
const CASES: [&str; 3] = ["eth", "erc20", "ethpull"];

fn address(hex_str: &str) -> Address {
    hex::decode(hex_str).unwrap().try_into().unwrap()
}

fn label(text: &str) -> Hash {
    sha256(text.as_bytes())
}

fn label_addr(text: &str) -> Address {
    label(text)[..20].try_into().unwrap()
}

fn config_identity() -> ConfigIdentity {
    ConfigIdentity {
        bedrock_genesis_id: label("e2e v2: bedrock genesis id"),
        channel_id: label("e2e v2: zone channel id"),
        bridge_header: label("e2e v2: bridge header account"),
        bridge_image_id: label("e2e v2: bridge program image id"),
        evm_chain_id: chain_id_u256(CHAIN),
        vault: address(VAULT),
        attestation_root_id: label("e2e v2: RFP-021 attestation root id"),
        salt: label("e2e v2: salt"),
    }
}

fn sequencer_keys() -> Vec<Hash> {
    (0..4).map(|i| label(&format!("e2e v2: accredited sequencer key {i}"))).collect()
}

struct Case {
    name: &'static str,
    input: ReleaseInput,
    nullifier: Hash,
    journal: Vec<u8>,
}

struct Scenario {
    config_id: Hash,
    notes: Vec<PublishedBurn>,
    burn_root: Hash,
    genesis: LezState,
    state: LezState,
    cases: Vec<Case>,
}

/// Twelve burns on LEZ for this configuration. Ours are counters 0, 1, 2 at burn indices 5, 6, 9:
/// 0.1 ETH to a plain recipient (no relayer, no fee), 100 TUSD to a recipient through relayer 1 with
/// the registered fee, and 0.1 ETH through relayer 2 to a recipient that rejects payment (evm-verify
/// puts a reverting contract there), which falls back to pull payment. The wallet finds its three
/// burns from its seed alone with `recover_burns`, then builds each release.
fn scenario() -> Scenario {
    let wallet = BridgeKeys::from_bridge_seed(&label("e2e v2: user bridge seed (test value)"));
    let config_id = sha256(&config_id_preimage(&config_identity()));
    let (chain_id, vault, token) = (chain_id_u256(CHAIN), address(VAULT), address(TOKEN));
    let tenth_eth = u256_from_u128(100_000_000_000_000_000);
    let hundred_tusd = u256_from_u128(100_000_000);
    // (counter, burn index, recipient, token, amount, relayer, relay fee)
    let ours = [
        (0u64, 5u64, label_addr("e2e v2: ETH recipient"), ETH, tenth_eth, [0u8; 20], [0u8; 32]),
        (
            1,
            6,
            label_addr("e2e v2: TUSD recipient"),
            token,
            hundred_tusd,
            label_addr("e2e v2: relayer 1"),
            u256_from_u128(250_000),
        ),
        (
            2,
            9,
            label_addr("e2e v2: rejecting recipient"),
            ETH,
            tenth_eth,
            label_addr("e2e v2: relayer 2"),
            u256_from_u128(500_000_000_000_000),
        ),
    ];

    let mut notes = Vec::new();
    for burn_index in 0..12u64 {
        let note = match ours.iter().find(|o| o.1 == burn_index) {
            Some(&(i, _, recipient, tok, amount, _, _)) => {
                BurnNote::create(&wallet, &config_id, i, &ReleaseTarget { chain_id, vault, recipient }, tok, amount)
            }
            None => {
                let other = BridgeKeys::from_bridge_seed(&label(&format!("e2e v2: other user {burn_index}")));
                let target = ReleaseTarget {
                    chain_id,
                    vault,
                    recipient: label_addr(&format!("e2e v2: other recipient {burn_index}")),
                };
                let (tok, amount) = if burn_index % 2 == 0 { (ETH, tenth_eth) } else { (token, hundred_tusd) };
                BurnNote::create(&other, &config_id, 0, &target, tok, amount)
            }
        };
        notes.push(PublishedBurn { burn_index, note });
    }
    let leaves: Vec<Hash> = notes
        .iter()
        .map(|b| burn_leaf(&config_id, b.burn_index, &b.note.token, &b.note.amount, &b.note.c_out, &b.note.r))
        .collect();
    let mut inc = IncrementalTree::new();
    for leaf in &leaves {
        inc.append(*leaf).unwrap();
    }
    let tree = Tree::from_leaves(leaves).unwrap();
    let burn_root = tree.root();
    assert_eq!(burn_root, inc.root());

    let scan = recover_burns(&wallet, &config_id, &chain_id, &vault, &notes, 20);
    assert_eq!(scan.owned.len(), 3, "the wallet must find its three burns");
    let cases = scan
        .owned
        .iter()
        .zip(CASES)
        .zip(ours)
        .map(|((owned, name), (_, burn_index, recipient, _, _, relayer, relay_fee))| {
            assert_eq!((owned.burn.burn_index, owned.recipient), (burn_index, recipient));
            let public = ReleasePublic {
                chain_id,
                vault,
                config_id,
                token: owned.burn.note.token,
                amount: owned.burn.note.amount,
                recipient,
                relayer,
                relay_fee,
            };
            let witness = ReleaseWitness {
                burn_index,
                path: tree.path(burn_index).unwrap(),
                s_out: owned.s_out,
                r: owned.burn.note.r,
            };
            let nullifier = verify_release(&burn_root, &witness, &public).unwrap();
            assert_eq!(nullifier, owned.nullifier);
            let journal = release_journal(&burn_root, &nullifier, &public).to_vec();
            Case { name, input: ReleaseInput { burn_root, witness, public }, nullifier, journal }
        })
        .collect();

    let key_set = key_set_hash(&sequencer_keys());
    let genesis = LezState {
        tip_id: 0,
        tip_hash: label("e2e v2: LEZ genesis block hash"),
        key_set_hash: key_set,
        config_tip_hash: label("e2e v2: channel config tip 0"),
        burn_count: 0,
        burn_frontier_hash: burn_frontier_hash(&IncrementalTree::new()),
        imported_leaf_count: 0,
        sunset: false,
    };
    let state = LezState {
        tip_id: 4242,
        tip_hash: label("e2e v2: LEZ block 4242 hash"),
        key_set_hash: key_set,
        config_tip_hash: label("e2e v2: channel config tip 0"),
        burn_count: inc.len(),
        burn_frontier_hash: burn_frontier_hash(&inc),
        imported_leaf_count: 3,
        sunset: false,
    };
    Scenario { config_id, notes, burn_root, genesis, state, cases }
}

fn case(name: &str) -> Result<Case> {
    scenario()
        .cases
        .into_iter()
        .find(|c| c.name == name)
        .with_context(|| format!("unknown case {name}; use one of {CASES:?}"))
}

// ---- helpers ----------------------------------------------------------------------------------------

/// `artifacts/`, or `$FERRATA_ARTIFACTS` (for timing runs whose receipts must not replace the
/// committed ones the Foundry fixtures were made from).
fn artifacts() -> PathBuf {
    std::env::var_os("FERRATA_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../artifacts"))
}

fn case_dir(name: &str) -> Result<PathBuf> {
    let dir = artifacts().join(name);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn hx(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

fn loadavg() -> String {
    fs::read_to_string("/proc/loadavg").unwrap_or_default().trim().to_string()
}

fn image_id_hex(id: [u32; 8]) -> String {
    hx(Digest::from(id).as_bytes())
}

fn env_with(bytes: &[u8]) -> Result<ExecutorEnv<'static>> {
    ExecutorEnv::builder().write_slice(bytes).build()
}

fn refuse_dev_mode() -> Result<()> {
    if std::env::var("RISC0_DEV_MODE").map(|v| !v.is_empty() && v != "0" && v != "false").unwrap_or(false) {
        bail!("RISC0_DEV_MODE is set: receipts would be fake");
    }
    Ok(())
}

/// The seal as `RiscZeroGroth16Verifier.verify` takes it: 4-byte selector (first bytes of the
/// verifier-parameters digest) and the 256-byte proof, as risc0-ethereum's `encode_seal` builds it.
fn eth_seal(receipt: &Receipt) -> Result<Vec<u8>> {
    let g = receipt.inner.groth16().context("not a Groth16 receipt")?;
    Ok([&g.verifier_parameters.as_bytes()[..4], g.seal.as_slice()].concat())
}

fn shape(info: &SessionInfo) -> (u64, Vec<u32>) {
    (info.cycles(), info.segments.iter().map(|s| s.po2).collect())
}

/// Deterministic pseudo-random bytes for the invariance runs.
fn rnd(tag: &str, i: u64) -> Hash {
    sha256(&[tag.as_bytes(), &i.to_be_bytes()].concat())
}

fn rnd_addr(tag: &str, i: u64) -> Address {
    rnd(tag, i)[..20].try_into().unwrap()
}

fn rnd_path(tag: &str, i: u64, directions: u32) -> MerklePath {
    MerklePath { directions, siblings: core::array::from_fn(|h| rnd(&format!("{tag} sibling {h}"), i)) }
}

/// Index `i` of the invariance runs: the extremes first, then pseudo-random 32-bit indices.
fn rnd_index(tag: &str, i: u64) -> u32 {
    match i {
        0 => 0,
        1 => u32::MAX,
        2 => 0x5555_5555,
        3 => 0xAAAA_AAAA,
        _ => u32::from_be_bytes(rnd(tag, i)[..4].try_into().unwrap()),
    }
}

/// A valid release input built from random values (its own root).
fn random_release_input(i: u64) -> ReleaseInput {
    let burn_index = rnd_index("release index", i);
    let public = ReleasePublic {
        chain_id: rnd("release chain", i),
        vault: rnd_addr("release vault", i),
        config_id: rnd("release config", i),
        token: rnd_addr("release token", i),
        amount: rnd("release amount", i),
        recipient: rnd_addr("release recipient", i),
        relayer: rnd_addr("release relayer", i),
        relay_fee: rnd("release fee", i),
    };
    let (s_out, r) = (rnd("release sOut", i), rnd("release r", i));
    let c_out = burn_commitment(&s_out, &public.chain_id, &public.vault, &public.recipient);
    let leaf = burn_leaf(&public.config_id, u64::from(burn_index), &public.token, &public.amount, &c_out, &r);
    let path = rnd_path("release", i, burn_index);
    ReleaseInput {
        burn_root: root_from_path(&leaf, &path),
        witness: ReleaseWitness { burn_index: u64::from(burn_index), path, s_out, r },
        public,
    }
}

/// A valid claim input built from random values (its own root).
fn random_claim_input(i: u64) -> ClaimInput {
    let leaf_index = rnd_index("claim index", i);
    let kind = if i % 2 == 0 { DestKind::Private } else { DestKind::Public };
    let (s, dest) = (rnd("claim s", i), rnd("claim dest", i));
    let (depositor, token, amount) =
        (rnd_addr("claim depositor", i), rnd_addr("claim token", i), rnd("claim amount", i));
    let leaf = inbound::deposit_leaf(
        u64::from(leaf_index),
        &depositor,
        &token,
        &amount,
        &inbound::commitment(&s, kind, &dest),
    );
    let path = rnd_path("claim", i, leaf_index);
    ClaimInput {
        root: root_from_path(&leaf, &path),
        chain_id: rnd("claim chain", i),
        vault: rnd_addr("claim vault", i),
        witness: ClaimWitness { depositor, token, amount, leaf_index: u64::from(leaf_index), path, s, kind, dest },
    }
}

// ---- commands --------------------------------------------------------------------------------------

fn cmd_scenario() -> Result<()> {
    let s = scenario();
    let state_json = |st: &LezState| {
        json!({"tip_id": st.tip_id, "tip_hash": hx(&st.tip_hash), "key_set_hash": hx(&st.key_set_hash),
               "config_tip_hash": hx(&st.config_tip_hash), "burn_count": st.burn_count,
               "burn_frontier_hash": hx(&st.burn_frontier_hash), "imported_leaf_count": st.imported_leaf_count,
               "sunset": st.sunset, "S": hx(&st.hash())})
    };
    let notes: Vec<_> = s
        .notes
        .iter()
        .map(|b| {
            json!({"burn_index": b.burn_index, "r": hx(&b.note.r), "memo": hx(&b.note.memo), "mac": hx(&b.note.mac),
                   "token": hx(&b.note.token), "amount": hx(&b.note.amount), "C_out": hx(&b.note.c_out)})
        })
        .collect();
    let cases: Vec<_> = s
        .cases
        .iter()
        .map(|c| {
            let p = &c.input.public;
            json!({"case": c.name, "burn_index": c.input.witness.burn_index, "token": hx(&p.token), "amount": hx(&p.amount),
                   "recipient": hx(&p.recipient), "relayer": hx(&p.relayer), "relay_fee": hx(&p.relay_fee),
                   "nullifier": hx(&c.nullifier), "journal": hx(&c.journal), "journal_sha256": hx(&sha256(&c.journal))})
        })
        .collect();
    let ci = config_identity();
    let value = json!({
        "description": "SPEC-v2 end-to-end scenario: 12 burn notes of one configuration; ours are burn indices 5 (0.1 ETH), 6 (100 TUSD via relayer 1) and 9 (0.1 ETH via relayer 2 to a recipient that rejects payment). Addresses are the deployer's CREATE addresses at nonces 0-3; evm-verify asserts them.",
        "chain_id": CHAIN, "deployer": hx(&address(DEPLOYER)), "groth16_verifier": hx(&address(GROTH16)),
        "emergency_stop": hx(&address(ESTOP)), "token": hx(&address(TOKEN)), "vault": hx(&address(VAULT)),
        "config_identity": {"bedrock_genesis_id": hx(&ci.bedrock_genesis_id), "channel_id": hx(&ci.channel_id),
            "bridge_header": hx(&ci.bridge_header), "bridge_image_id": hx(&ci.bridge_image_id), "evm_chain_id": hx(&ci.evm_chain_id),
            "vault": hx(&ci.vault), "attestation_root_id": hx(&ci.attestation_root_id), "salt": hx(&ci.salt)},
        "config_id": hx(&s.config_id), "n_min": N_MIN, "t_min": T_MIN,
        "sequencer_keys": sequencer_keys().iter().map(|k| hx(k)).collect::<Vec<_>>(),
        "genesis_state": state_json(&s.genesis), "checkpoint_state": state_json(&s.state),
        "burn_root": hx(&s.burn_root), "release_image_id": image_id_hex(RELEASE_ID), "claim_image_id": image_id_hex(CLAIM_ID),
        "burn_notes": notes, "cases": cases,
    });
    fs::create_dir_all(artifacts())?;
    fs::write(artifacts().join("scenario.json"), serde_json::to_string_pretty(&value)? + "\n")?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn cmd_execute() -> Result<()> {
    println!("release image id {}", image_id_hex(RELEASE_ID));
    println!("claim image id   {}", image_id_hex(CLAIM_ID));
    for c in scenario().cases {
        let start = Instant::now();
        let info = default_executor().execute(env_with(&c.input.encode())?, RELEASE_ELF)?;
        let secs = start.elapsed().as_secs_f64();
        ensure!(info.journal.bytes == c.journal, "journal differs from the native computation");
        let (cycles, po2s) = shape(&info);
        let padded: u64 = po2s.iter().map(|p| 1u64 << p).sum();
        println!(
            "[release {}] user cycles {cycles} | segments {} (po2 {po2s:?}, padded total {padded}) | exit {:?} | journal == native: yes | executor wall {secs:.3} s",
            c.name,
            po2s.len(),
            info.exit_code
        );
    }
    let claim = random_claim_input(7);
    let info = default_executor().execute(env_with(&claim.encode())?, CLAIM_ELF)?;
    let (cycles, po2s) = shape(&info);
    println!("[claim] user cycles {cycles} | segments {} (po2 {po2s:?}) | exit {:?}", po2s.len(), info.exit_code);

    let mut bad = case("eth")?.input;
    bad.witness.s_out[0] ^= 1;
    match default_executor().execute(env_with(&bad.encode())?, RELEASE_ELF) {
        Ok(info) => bail!("bad witness executed with exit {:?}", info.exit_code),
        Err(e) => println!("[bad sOut] execution fails as it must: {}", e.to_string().lines().next().unwrap_or("")),
    }
    let mut bad = case("eth")?.input;
    bad.witness.path.directions ^= 1;
    match default_executor().execute(env_with(&bad.encode())?, RELEASE_ELF) {
        Ok(info) => bail!("bad directions executed with exit {:?}", info.exit_code),
        Err(e) => {
            println!("[bad directions] execution fails as it must: {}", e.to_string().lines().next().unwrap_or(""))
        }
    }
    Ok(())
}

/// SPEC-v2 §5.2: the guest's cycle count must not depend on the witness.
fn cmd_invariance(n: u64) -> Result<()> {
    println!("loadavg {}", loadavg());
    for (name, elf) in [("release", RELEASE_ELF), ("claim", CLAIM_ELF)] {
        let mut shapes = BTreeSet::new();
        let start = Instant::now();
        for i in 0..n {
            let bytes = if name == "release" {
                random_release_input(i).encode().to_vec()
            } else {
                random_claim_input(i).encode().to_vec()
            };
            let info = default_executor().execute(env_with(&bytes)?, elf)?;
            if std::env::var_os("FERRATA_DIAG").is_some() && name == "release" {
                println!("diag directions popcount {:2} cycles {}", rnd_index("release index", i).count_ones(), info.cycles());
            }
            shapes.insert(shape(&info));
        }
        println!(
            "[{name}] {n} random valid witnesses (leaf indices 0, 2^32-1, 0x55555555, 0xAAAAAAAA, then random; random siblings, secrets and public fields): distinct (user cycles, segment po2s): {:?} ({:.1} s)",
            shapes,
            start.elapsed().as_secs_f64()
        );
        ensure!(shapes.len() == 1, "{name}: the cycle count depends on the witness");
    }
    println!("cycle counts are independent of the witness for both guests");
    Ok(())
}

fn cmd_prove(name: &str) -> Result<()> {
    refuse_dev_mode()?;
    let c = case(name)?;
    println!("[{name}] load before: {}", loadavg());
    let start = Instant::now();
    let info = default_prover().prove_with_opts(env_with(&c.input.encode())?, RELEASE_ELF, &ProverOpts::succinct())?;
    let secs = start.elapsed().as_secs_f64();
    println!("[{name}] load after:  {}", loadavg());
    let receipt = info.receipt;
    ensure!(matches!(receipt.inner, InnerReceipt::Succinct(_)), "expected a succinct receipt");
    ensure!(receipt.journal.bytes == c.journal, "journal differs from the native computation");
    receipt.verify(RELEASE_ID).context("succinct receipt does not verify")?;
    let s = &info.stats;
    let bytes = bincode::serialize(&receipt)?;
    println!(
        "[{name}] succinct proof: wall {secs:.2} s | segments {} | user cycles {} | total cycles {} (paging {}, reserved {})",
        s.segments, s.user_cycles, s.total_cycles, s.paging_cycles, s.reserved_cycles
    );
    println!(
        "[{name}] receipt: {} bytes bincode (seal {} bytes, journal {} bytes); verifies against the release image id",
        bytes.len(),
        receipt.inner.succinct()?.seal.len() * 4,
        receipt.journal.bytes.len()
    );
    let dir = case_dir(name)?;
    fs::write(dir.join("receipt-succinct.bin"), &bytes)?;
    fs::write(dir.join("journal.bin"), &receipt.journal.bytes)?;
    Ok(())
}

fn load_receipt(name: &str, file: &str) -> Result<Receipt> {
    let path = artifacts().join(name).join(file);
    let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    Ok(bincode::deserialize(&bytes)?)
}

fn cmd_p254(name: &str) -> Result<()> {
    refuse_dev_mode()?;
    let receipt = load_receipt(name, "receipt-succinct.bin")?;
    let succinct: &SuccinctReceipt<_> = receipt.inner.succinct()?;
    let asset = Asset::Inline(Bytes::from(bincode::serialize(succinct)?));
    println!("[{name}] load before: {}", loadavg());
    let start = Instant::now();
    let p254 = ApiClient::from_env()?.identity_p254(&ProverOpts::succinct(), asset, AssetRequest::Inline)?;
    let secs = start.elapsed().as_secs_f64();
    println!("[{name}] load after:  {}", loadavg());
    println!("[{name}] identity_p254: wall {secs:.2} s | p254 seal {} bytes", p254.get_seal_bytes().len());
    Ok(())
}

fn cmd_groth16(name: &str) -> Result<()> {
    refuse_dev_mode()?;
    let c = case(name)?;
    let succinct = load_receipt(name, "receipt-succinct.bin")?;
    println!("[{name}] load before: {}", loadavg());
    let start = Instant::now();
    let receipt = default_prover().compress(&ProverOpts::groth16(), &succinct)?;
    let secs = start.elapsed().as_secs_f64();
    println!("[{name}] load after:  {}", loadavg());
    ensure!(matches!(receipt.inner, InnerReceipt::Groth16(_)), "expected a Groth16 receipt");
    ensure!(receipt.journal.bytes == c.journal, "journal differs from the native computation");
    receipt.verify(RELEASE_ID).context("Groth16 receipt does not verify")?;
    let seal = eth_seal(&receipt)?;
    let bytes = bincode::serialize(&receipt)?;
    println!("[{name}] identity_p254 + Groth16 (Docker): wall {secs:.2} s");
    println!(
        "[{name}] Groth16 receipt: {} bytes bincode; on-chain seal {} bytes = selector {} + proof {}; verifies against the release image id",
        bytes.len(),
        seal.len(),
        hx(&seal[..4]),
        seal.len() - 4
    );
    let dir = case_dir(name)?;
    fs::write(dir.join("receipt-groth16.bin"), &bytes)?;
    fs::write(dir.join("seal.hex"), hx(&seal) + "\n")?;
    fs::write(dir.join("journal.hex"), hx(&receipt.journal.bytes) + "\n")?;
    Ok(())
}

fn cmd_verify(name: &str) -> Result<()> {
    let c = case(name)?;
    println!("release image id {}", image_id_hex(RELEASE_ID));
    let p = Groth16ReceiptVerifierParameters::default();
    let digest = risc0_zkvm::sha::Digestible::digest(&p);
    println!(
        "Groth16ReceiptVerifierParameters::default(): control_root {} bn254_control_id {} digest {} selector {}",
        hx(p.control_root.as_bytes()),
        hx(p.bn254_control_id.as_bytes()),
        hx(digest.as_bytes()),
        hx(&digest.as_bytes()[..4])
    );
    for file in ["receipt-succinct.bin", "receipt-groth16.bin"] {
        let receipt = load_receipt(name, file)?;
        receipt.verify(RELEASE_ID).with_context(|| format!("{file} does not verify"))?;
        ensure!(receipt.journal.bytes == c.journal, "{file}: journal differs");
        let size = bincode::serialize(&receipt)?.len();
        println!("[{name}] {file}: verifies; {size} bytes bincode");
        if receipt.inner.groth16().is_ok() {
            println!("[{name}]   seal selector {}", hx(&eth_seal(&receipt)?[..4]));
        }
    }
    let j = ReleaseJournal::decode(&c.journal).context("journal does not decode")?;
    println!(
        "[{name}] journal: burnRoot {} NOut {} sha256 {}",
        hx(&j.burn_root),
        hx(&j.nullifier),
        hx(&sha256(&c.journal))
    );
    Ok(())
}

fn cmd_fixtures() -> Result<()> {
    let s = scenario();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../evm-verify/test/fixtures/v2");
    fs::create_dir_all(&dir)?;
    for c in &s.cases {
        let receipt = load_receipt(c.name, "receipt-groth16.bin")?;
        ensure!(receipt.journal.bytes == c.journal, "saved receipt is for another journal");
        let p = &c.input.public;
        let value = json!({
            "chainId": CHAIN, "deployer": hx(&address(DEPLOYER)), "groth16Verifier": hx(&address(GROTH16)),
            "emergencyStop": hx(&address(ESTOP)), "testToken": hx(&address(TOKEN)), "vault": hx(&address(VAULT)),
            "lezConfigId": hx(&s.config_id), "releaseImageId": image_id_hex(RELEASE_ID), "burnRoot": hx(&s.burn_root),
            "nullifier": hx(&c.nullifier), "token": hx(&p.token), "amount": hx(&p.amount), "recipient": hx(&p.recipient),
            "relayer": hx(&p.relayer), "relayFee": hx(&p.relay_fee), "journal": hx(&c.journal),
            "journalDigest": hx(&sha256(&c.journal)), "seal": hx(&eth_seal(&receipt)?),
        });
        let path = dir.join(format!("release-{}.json", c.name));
        fs::write(&path, serde_json::to_string_pretty(&value)? + "\n")?;
        println!("wrote {}", path.display());
    }
    let checkpoint = json!({
        "description": "Checkpoint side of the v2 scenario (the checkpoint proof itself is mocked in evm-verify): genesisState and newState are S (SPEC-v2 §4.1) of the states in artifacts/scenario.json.",
        "chainId": CHAIN, "vault": hx(&address(VAULT)), "lezConfigId": hx(&s.config_id), "nMin": N_MIN, "tMin": T_MIN,
        "genesisState": hx(&s.genesis.hash()), "newState": hx(&s.state.hash()), "burnRoot": hx(&s.burn_root),
        "burnCount": s.state.burn_count, "importedLeafCount": s.state.imported_leaf_count, "sunset": s.state.sunset,
    });
    let path = dir.join("checkpoint.json");
    fs::write(&path, serde_json::to_string_pretty(&checkpoint)? + "\n")?;
    println!("wrote {}", path.display());
    Ok(())
}

/// The claim guest and the native claim of the same deposits give the same nullifiers.
fn cmd_claim_check(n: u64) -> Result<()> {
    let wallet = BridgeKeys::from_bridge_seed(&label("claim-check v2 seed"));
    let (chain_id, vault): (U256, Address) = (chain_id_u256(CHAIN), label_addr("claim-check vault"));
    let depositor = label_addr("claim-check depositor");
    let mut tree = Tree::new();
    let mut ours = Vec::new();
    for leaf_index in 0..(2 * n) {
        let token = if leaf_index % 3 == 0 { ETH } else { label_addr("claim-check token") };
        let amount = u256_from_u128(10u128.pow(6 + (leaf_index % 13) as u32));
        let c = if leaf_index % 2 == 1 {
            let j = leaf_index / 2;
            let kind = if j % 2 == 0 { DestKind::Private } else { DestKind::Public };
            let s = inbound::deposit_secret(&wallet.auth, &chain_id, &vault, &depositor, j);
            let dest = label(&format!("claim-check dest {j}"));
            ours.push((leaf_index, token, amount, s, kind, dest));
            inbound::commitment(&s, kind, &dest)
        } else {
            label(&format!("claim-check other C {leaf_index}"))
        };
        tree.append(inbound::deposit_leaf(leaf_index, &depositor, &token, &amount, &c)).unwrap();
    }
    let root = tree.root();
    let mut cycles = BTreeSet::new();
    for (leaf_index, token, amount, s, kind, dest) in ours {
        let witness =
            ClaimWitness { depositor, token, amount, leaf_index, path: tree.path(leaf_index).unwrap(), s, kind, dest };
        let public_n = inbound::verify_claim(&root, &witness, &chain_id, &vault).unwrap();
        let input = ClaimInput { root, chain_id, vault, witness };
        let info = default_executor().execute(env_with(&input.encode())?, CLAIM_ELF)?;
        ensure!(
            info.journal.bytes == claim_journal(&root, &chain_id, &vault, &public_n, &token, &amount),
            "the claim guest's journal (and so its nullifier) differs from the native claim"
        );
        cycles.insert(shape(&info));
    }
    println!(
        "claim-check: {n} deposits (both kinds), the claim guest's journal == native (root, chain, vault, N, token, amount) for all; distinct (user cycles, po2s): {cycles:?}"
    );
    Ok(())
}

fn cmd_prove_claim() -> Result<()> {
    refuse_dev_mode()?;
    let input = random_claim_input(7);
    println!("[claim] load before: {}", loadavg());
    let start = Instant::now();
    let info = default_prover().prove_with_opts(env_with(&input.encode())?, CLAIM_ELF, &ProverOpts::succinct())?;
    let secs = start.elapsed().as_secs_f64();
    println!("[claim] load after:  {}", loadavg());
    let start = Instant::now();
    info.receipt.verify(CLAIM_ID).context("claim receipt does not verify")?;
    let verify_ms = start.elapsed().as_secs_f64() * 1e3;
    let s = &info.stats;
    println!(
        "[claim] succinct proof: wall {secs:.2} s | user cycles {} | total cycles {} | receipt {} bytes | verify {verify_ms:.1} ms (what a relayer runs before a drop)",
        s.user_cycles,
        s.total_cycles,
        bincode::serialize(&info.receipt)?.len()
    );
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| args.get(i).map(String::as_str).context("missing argument");
    let num =
        |i: usize, default: u64| -> Result<u64> { Ok(args.get(i).map(|a| a.parse()).transpose()?.unwrap_or(default)) };
    match args.first().map(String::as_str) {
        Some("scenario") => cmd_scenario(),
        Some("execute") => cmd_execute(),
        Some("invariance") => cmd_invariance(num(1, 64)?),
        Some("prove") => cmd_prove(arg(1)?),
        Some("p254") => cmd_p254(arg(1)?),
        Some("groth16") => cmd_groth16(arg(1)?),
        Some("verify") => cmd_verify(arg(1)?),
        Some("fixtures") => cmd_fixtures(),
        Some("claim-check") => cmd_claim_check(num(1, 16)?),
        Some("prove-claim") => cmd_prove_claim(),
        _ => bail!("usage: host scenario | execute | invariance [n] | prove <case> | p254 <case> | groth16 <case> | verify <case> | fixtures | claim-check [n] | prove-claim"),
    }
}
