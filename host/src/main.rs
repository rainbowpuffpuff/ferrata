//! Host for the release guest. Builds the end-to-end scenario, executes, proves a succinct
//! receipt, wraps it to Groth16 (RISC Zero's own pipeline: r0vm 3.0.5, then Docker), verifies, and
//! writes `artifacts/` and the Foundry fixtures. Every number printed comes from the run it reports.
//!
//! ```text
//! host scenario                 write artifacts/scenario.json
//! host execute                  executor only: cycles and segments of both release cases, a bad witness
//! host prove <eth|erc20>        succinct receipt -> artifacts/<case>/receipt-succinct.bin
//! host p254 <eth|erc20>         identity_p254 alone (the STARK-to-BN254 recursion step before Groth16)
//! host groth16 <eth|erc20>      saved succinct receipt -> Groth16 receipt, seal, journal
//! host verify <eth|erc20>       verify saved receipts, print selector and verifier parameters
//! host fixtures                 artifacts -> evm-verify/test/fixtures/release-<case>.json
//! host claim-check [n]          claim guest in the executor vs. the native claim, n deposits
//! ```
//!
//! Proving runs in the r0vm 3.0.5 subprocess (risc0-zkvm `client` feature); measure peak memory
//! around this process with `/usr/bin/time -v` (scripts/prove-bench.sh), which includes r0vm.

use std::{fs, path::PathBuf, time::Instant};

use anyhow::{bail, ensure, Context, Result};
use ferrata_bridge_core::encoding::{u256_from_u128, ClaimInput, ReleaseInput, ReleaseJournal, CLAIM_JOURNAL_LEN};
use ferrata_bridge_core::inbound::{self, ClaimWitness};
use ferrata_bridge_core::outbound::{
    burn_leaf, recover_burns, release_journal, verify_release, BurnNote, PublishedBurn, ReleasePublic, ReleaseTarget,
    ReleaseWitness,
};
use ferrata_bridge_core::tree::Tree;
use ferrata_bridge_core::{sha256, Address, BridgeKey, Hash};
use methods::{CLAIM_ELF, CLAIM_ID, RELEASE_ELF, RELEASE_ID};
use risc0_zkvm::{
    default_executor, default_prover, ApiClient, Asset, AssetRequest, Bytes, Digest, ExecutorEnv,
    Groth16ReceiptVerifierParameters, InnerReceipt, ProverOpts, Receipt, SuccinctReceipt,
};
use serde_json::json;

// ---- the end-to-end scenario --------------------------------------------------------------------
// Foundry and anvil both default to chain id 31337. The deployer is anvil's well-known test account
// 0; evm-verify deploys, in this order, the Groth16 verifier (nonce 0), the test token (nonce 1) and
// the vault (nonce 2), and asserts that the addresses equal these.
const CHAIN_ID: u64 = 31_337;
const DEPLOYER: &str = "f39fd6e51aad88f6f4ce6ab8827279cfffb92266";
const VERIFIER: &str = "5fbdb2315678afecb367f032d93f642f64180aa3";
const TOKEN: &str = "e7f1725e7734ce288f8367e1bb143e90bb3f0512";
const VAULT: &str = "9fe46736679d2d9a65f0992f2272de9f3c7fa6e0";
const ETH: Address = [0u8; 20];

fn address(hex_str: &str) -> Address {
    hex::decode(hex_str).unwrap().try_into().unwrap()
}

fn label(text: &str) -> Hash {
    sha256(text.as_bytes())
}

fn label_addr(text: &str) -> Address {
    label(text)[..20].try_into().unwrap()
}

struct Case {
    name: &'static str,
    input: ReleaseInput,
    nullifier: Hash,
    journal: Vec<u8>,
}

struct Scenario {
    lez_config_id: Hash,
    notes: Vec<PublishedBurn>,
    burn_root: Hash,
    cases: Vec<Case>,
}

/// Eight burns on LEZ; ours are counters 0 (0.1 ETH) and 1 (100 TUSD, 6 decimals) at leaves 5 and
/// 6. The wallet finds them from its key alone with `recover_burns`, then builds each release.
fn scenario() -> Scenario {
    let key = BridgeKey::new(&label("ferrata-bridge e2e: user bridge key (test value)"));
    let lez_config_id = label("ferrata-bridge e2e: LEZ configuration");
    let (vault, token) = (address(VAULT), address(TOKEN));
    let eth_amount = u256_from_u128(100_000_000_000_000_000); // 0.1 ether
    let tusd_amount = u256_from_u128(100_000_000); // 100 TUSD
    let ours = [
        (0u64, label_addr("e2e: ETH recipient"), ETH, eth_amount),
        (1u64, label_addr("e2e: TUSD recipient"), token, tusd_amount),
    ];

    let mut notes = Vec::new();
    for leaf_index in 0..8u64 {
        let note = match leaf_index {
            5 | 6 => {
                let (i, recipient, tok, amount) = ours[(leaf_index - 5) as usize];
                let target = ReleaseTarget { chain_id: CHAIN_ID, vault, recipient };
                BurnNote::create(&key, &lez_config_id, i, &target, tok, amount)
            }
            _ => {
                let other = BridgeKey::new(&label(&format!("e2e: other user {leaf_index}")));
                let target = ReleaseTarget {
                    chain_id: CHAIN_ID,
                    vault,
                    recipient: label_addr(&format!("e2e: other {leaf_index}")),
                };
                let (tok, amount) = if leaf_index % 2 == 0 { (ETH, eth_amount) } else { (token, tusd_amount) };
                BurnNote::create(&other, &lez_config_id, 0, &target, tok, amount)
            }
        };
        notes.push(PublishedBurn { leaf_index, note });
    }
    let tree = Tree::from_leaves(
        notes.iter().map(|b| burn_leaf(b.leaf_index, &b.note.token, &b.note.amount, &b.note.c_out)).collect(),
    )
    .unwrap();
    let burn_root = tree.root();

    let scan = recover_burns(&key, &lez_config_id, CHAIN_ID, &vault, &notes, 20);
    assert_eq!(scan.owned.len(), 2, "the wallet must find its two burns");
    let fees = [([0u8; 20], [0u8; 32]), (label_addr("e2e: relayer"), u256_from_u128(250_000))];
    let cases = scan
        .owned
        .iter()
        .zip(["eth", "erc20"])
        .zip(fees)
        .map(|((owned, name), (fee_recipient, relay_fee))| {
            let public = ReleasePublic {
                chain_id: CHAIN_ID,
                vault,
                lez_config_id,
                token: owned.burn.note.token,
                amount: owned.burn.note.amount,
                recipient: owned.recipient,
                fee_recipient,
                relay_fee,
            };
            let witness = ReleaseWitness {
                leaf_index: owned.burn.leaf_index,
                path: tree.path(owned.burn.leaf_index).unwrap(),
                s_out: owned.s_out,
            };
            let nullifier = verify_release(&burn_root, &witness, &public).unwrap();
            assert_eq!(nullifier, owned.nullifier);
            let journal = release_journal(&burn_root, &nullifier, &public).to_vec();
            Case { name, input: ReleaseInput { burn_root, witness, public }, nullifier, journal }
        })
        .collect();
    Scenario { lez_config_id, notes, burn_root, cases }
}

fn case(name: &str) -> Result<Case> {
    scenario()
        .cases
        .into_iter()
        .find(|c| c.name == name)
        .with_context(|| format!("unknown case {name}; use eth or erc20"))
}

// ---- helpers ----------------------------------------------------------------------------------------

/// `artifacts/`, or `$FERRATA_ARTIFACTS` (used for timing re-runs, so that the committed receipts,
/// which the Foundry fixtures were made from, are not replaced by new randomised proofs).
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

fn release_env(input: &ReleaseInput) -> Result<ExecutorEnv<'static>> {
    ExecutorEnv::builder().write_slice(&input.encode()).build()
}

/// `RISC0_DEV_MODE` makes r0vm return fake receipts; refuse to measure anything under it.
fn refuse_dev_mode() -> Result<()> {
    if std::env::var("RISC0_DEV_MODE").map(|v| !v.is_empty() && v != "0" && v != "false").unwrap_or(false) {
        bail!("RISC0_DEV_MODE is set: receipts would be fake");
    }
    Ok(())
}

/// The seal in the form `RiscZeroGroth16Verifier.verify` takes: the 4-byte selector (first bytes of
/// the receipt's verifier-parameters digest) followed by the 256-byte Groth16 proof, as
/// risc0-ethereum's `encode_seal` builds it.
fn eth_seal(receipt: &Receipt) -> Result<Vec<u8>> {
    let g = receipt.inner.groth16().context("not a Groth16 receipt")?;
    Ok([&g.verifier_parameters.as_bytes()[..4], g.seal.as_slice()].concat())
}

fn print_verifier_parameters() {
    let p = Groth16ReceiptVerifierParameters::default();
    let digest = risc0_zkvm::sha::Digestible::digest(&p);
    println!("  Groth16ReceiptVerifierParameters::default() (risc0-zkvm 3.0.5):");
    println!("    control_root       {}", hx(p.control_root.as_bytes()));
    println!("    bn254_control_id   {}", hx(p.bn254_control_id.as_bytes()));
    println!("    digest             {}", hx(digest.as_bytes()));
    println!("    selector           {}", hx(&digest.as_bytes()[..4]));
}

// ---- commands --------------------------------------------------------------------------------------

fn cmd_scenario() -> Result<()> {
    let s = scenario();
    let notes: Vec<_> = s
        .notes
        .iter()
        .map(|b| {
            json!({"leaf_index": b.leaf_index, "r": hx(&b.note.r), "memo": hx(&b.note.memo), "token": hx(&b.note.token),
                   "amount": hx(&b.note.amount), "C_out": hx(&b.note.c_out)})
        })
        .collect();
    let cases: Vec<_> = s
        .cases
        .iter()
        .map(|c| {
            let p = &c.input.public;
            json!({"case": c.name, "leaf_index": c.input.witness.leaf_index, "token": hx(&p.token), "amount": hx(&p.amount),
                   "recipient": hx(&p.recipient), "fee_recipient": hx(&p.fee_recipient), "relay_fee": hx(&p.relay_fee),
                   "nullifier": hx(&c.nullifier), "journal": hx(&c.journal), "journal_sha256": hx(&sha256(&c.journal))})
        })
        .collect();
    let value = json!({
        "description": "End-to-end scenario: 8 burn notes on LEZ; ours are leaves 5 (0.1 ETH) and 6 (100 TUSD). The vault and token addresses are the CREATE addresses of the deployer at nonces 2 and 1; evm-verify asserts them.",
        "chain_id": CHAIN_ID, "deployer": hx(&address(DEPLOYER)), "verifier": hx(&address(VERIFIER)),
        "token": hx(&address(TOKEN)), "vault": hx(&address(VAULT)), "lez_config_id": hx(&s.lez_config_id),
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
    for c in scenario().cases {
        println!("[{}] load before: {}", c.name, loadavg());
        let start = Instant::now();
        let info = default_executor().execute(release_env(&c.input)?, RELEASE_ELF)?;
        let secs = start.elapsed().as_secs_f64();
        ensure!(info.journal.bytes == c.journal, "journal differs from the native computation");
        let po2s: Vec<u32> = info.segments.iter().map(|s| s.po2).collect();
        let padded: u64 = info.segments.iter().map(|s| 1u64 << s.po2).sum();
        println!(
            "[{}] user cycles {} | segments {} (po2 {:?}, padded total {}) | exit {:?} | executor wall {:.3} s",
            c.name,
            info.cycles(),
            info.segments.len(),
            po2s,
            padded,
            info.exit_code,
            secs
        );
        println!("[{}] journal == native release_journal(): yes ({} bytes)", c.name, info.journal.bytes.len());
    }
    // A witness with the wrong secret: the guest panics, so there is nothing to prove.
    let mut bad = case("eth")?.input;
    bad.witness.s_out[0] ^= 1;
    match default_executor().execute(release_env(&bad)?, RELEASE_ELF) {
        Ok(info) => bail!("bad witness executed with exit {:?}", info.exit_code),
        Err(e) => println!("[bad s_out] execution fails as it must: {}", e.to_string().lines().next().unwrap_or("")),
    }
    Ok(())
}

fn cmd_prove(name: &str) -> Result<()> {
    refuse_dev_mode()?;
    let c = case(name)?;
    println!("[{name}] load before: {}", loadavg());
    let start = Instant::now();
    let info = default_prover().prove_with_opts(release_env(&c.input)?, RELEASE_ELF, &ProverOpts::succinct())?;
    let secs = start.elapsed().as_secs_f64();
    println!("[{name}] load after:  {}", loadavg());
    let receipt = info.receipt;
    ensure!(matches!(receipt.inner, InnerReceipt::Succinct(_)), "expected a succinct receipt");
    ensure!(receipt.journal.bytes == c.journal, "journal differs from the native computation");
    receipt.verify(RELEASE_ID).context("succinct receipt does not verify")?;
    let s = &info.stats;
    let bytes = bincode::serialize(&receipt)?;
    let seal_bytes = receipt.inner.succinct()?.seal.len() * 4;
    println!(
        "[{name}] succinct proof: wall {secs:.2} s | segments {} | user cycles {} | total cycles {} (paging {}, reserved {})",
        s.segments, s.user_cycles, s.total_cycles, s.paging_cycles, s.reserved_cycles
    );
    println!("[{name}] receipt: {} bytes bincode (seal {seal_bytes} bytes, journal {} bytes); verifies against the release image id", bytes.len(), receipt.journal.bytes.len());
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
    print_verifier_parameters();
    for file in ["receipt-succinct.bin", "receipt-groth16.bin"] {
        let receipt = load_receipt(name, file)?;
        receipt.verify(RELEASE_ID).with_context(|| format!("{file} does not verify"))?;
        ensure!(receipt.journal.bytes == c.journal, "{file}: journal differs");
        let size = bincode::serialize(&receipt)?.len();
        println!("[{name}] {file}: verifies; {size} bytes bincode");
        if let Ok(g) = receipt.inner.groth16() {
            println!("[{name}]   receipt verifier_parameters {}", hx(g.verifier_parameters.as_bytes()));
            println!("[{name}]   seal selector             {}", hx(&eth_seal(&receipt)?[..4]));
        }
    }
    let j = ReleaseJournal::decode(&c.journal).context("journal does not decode")?;
    println!(
        "[{name}] journal: burn_root {} nullifier {} sha256 {}",
        hx(&j.burn_root),
        hx(&j.nullifier),
        hx(&sha256(&c.journal))
    );
    Ok(())
}

fn cmd_fixtures() -> Result<()> {
    let s = scenario();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../evm-verify/test/fixtures");
    fs::create_dir_all(&dir)?;
    for c in &s.cases {
        let receipt = load_receipt(c.name, "receipt-groth16.bin")?;
        ensure!(receipt.journal.bytes == c.journal, "saved receipt is for another journal");
        let p = &c.input.public;
        let value = json!({
            "chainId": CHAIN_ID, "deployer": hx(&address(DEPLOYER)), "verifier": hx(&address(VERIFIER)),
            "token": hx(&p.token), "testToken": hx(&address(TOKEN)), "vault": hx(&address(VAULT)),
            "lezConfigId": hx(&s.lez_config_id), "releaseImageId": image_id_hex(RELEASE_ID),
            "burnRoot": hx(&s.burn_root), "nullifier": hx(&c.nullifier), "amount": hx(&p.amount),
            "recipient": hx(&p.recipient), "feeRecipient": hx(&p.fee_recipient), "relayFee": hx(&p.relay_fee),
            "journal": hx(&c.journal), "journalDigest": hx(&sha256(&c.journal)), "seal": hx(&eth_seal(&receipt)?),
        });
        let path = dir.join(format!("release-{}.json", c.name));
        fs::write(&path, serde_json::to_string_pretty(&value)? + "\n")?;
        println!("wrote {}", path.display());
    }
    Ok(())
}

/// Claims made privately (claim guest, executor) and publicly (native `verify_claim`) of the same
/// deposits give the same nullifiers.
fn cmd_claim_check(n: u64) -> Result<()> {
    let key = BridgeKey::new(&label("ferrata-bridge claim-check key"));
    let (vault, depositor) = (label_addr("claim-check vault"), label_addr("claim-check depositor"));
    let mut tree = Tree::new();
    let mut ours = Vec::new();
    for leaf_index in 0..(2 * n) {
        let token = if leaf_index % 3 == 0 { ETH } else { label_addr("claim-check token") };
        let amount = u256_from_u128(10u128.pow(6 + (leaf_index % 13) as u32));
        let c = if leaf_index % 2 == 1 {
            let j = leaf_index / 2;
            let s = inbound::deposit_secret(&key, CHAIN_ID, &vault, &depositor, j);
            let dest = label(&format!("claim-check dest {j}"));
            ours.push((leaf_index, token, amount, s, dest));
            inbound::commitment(&s, &dest)
        } else {
            label(&format!("claim-check other C {leaf_index}"))
        };
        tree.append(inbound::deposit_leaf(leaf_index, &token, &amount, &c)).unwrap();
    }
    let root = tree.root();
    let mut cycles = Vec::new();
    for (leaf_index, token, amount, s, dest) in ours {
        let witness = ClaimWitness { token, amount, leaf_index, path: tree.path(leaf_index).unwrap(), s, dest };
        let public_n = inbound::verify_claim(&root, &witness, CHAIN_ID, &vault).unwrap();
        let input = ClaimInput { root, chain_id: CHAIN_ID, vault, witness };
        let env = ExecutorEnv::builder().write_slice(&input.encode()).build()?;
        let info = default_executor().execute(env, CLAIM_ELF)?;
        ensure!(info.journal.bytes.len() == CLAIM_JOURNAL_LEN, "claim journal length");
        ensure!(info.journal.bytes[..32] == root, "claim journal root");
        ensure!(info.journal.bytes[32..64] == public_n, "nullifier differs between the zkVM and the native claim");
        cycles.push(info.cycles());
    }
    println!(
        "claim-check: {} deposits, nullifier from the claim guest (zkVM executor) == native verify_claim for all; user cycles {:?}",
        cycles.len(),
        cycles
    );
    println!("claim image id {}", image_id_hex(CLAIM_ID));
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| args.get(i).map(String::as_str).context("missing argument");
    match args.first().map(String::as_str) {
        Some("scenario") => cmd_scenario(),
        Some("execute") => cmd_execute(),
        Some("prove") => cmd_prove(arg(1)?),
        Some("p254") => cmd_p254(arg(1)?),
        Some("groth16") => cmd_groth16(arg(1)?),
        Some("verify") => cmd_verify(arg(1)?),
        Some("fixtures") => cmd_fixtures(),
        Some("claim-check") => cmd_claim_check(args.get(1).map(|a| a.parse()).transpose()?.unwrap_or(16)),
        _ => bail!("usage: host scenario | execute | prove <case> | p254 <case> | groth16 <case> | verify <case> | fixtures | claim-check [n]"),
    }
}
