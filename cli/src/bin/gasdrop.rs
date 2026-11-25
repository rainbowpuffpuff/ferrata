//! `ferrata-gasdrop`: the gas drop's two off-chain steps (bridge.md §6, v3 D13; M0 check 2), prototype, unaudited.
//!
//! - `prove` (the claimant): proves the gas-drop guest for real (RISC Zero's prover through r0vm, succinct) over a
//!   1324-byte `GasDropInput` (the claim witness under an imported root, the one-time address, the relayer), checks
//!   the receipt against the guest's image id and writes it as JSON (the receipt a relayer is shown).
//! - `relay` (the relayer): runs the relayer's own gas drop, [`ferrata_cli::relayer::gas_drop`] (the receipt checked
//!   by [`ferrata_cli::relayer::check_gas_drop`] against the image id, this relayer, the configuration, the imported
//!   roots and the claimed nullifiers; one drop per `N`, recorded in the drop book before the credit is sent), with
//!   the native credit sent by an external command. The command is how a relayer whose zone runs another LEZ version
//!   than this tool links sends it (lez/crown/driver `gas-drop-credit` on LEZ v0.4.0-rc3): `{to}` and `{amount}` in
//!   it are replaced by the one-time address (hex) and the amount; the last line of its standard output is the
//!   credit's transaction.
//!
//! Neither step reads a chain: `relay` takes the zone's imported roots and claimed nullifiers as files (hex, one per
//! line), which the caller reads from the bridge program's accounts. Each prints one JSON object.
//!
//! ```text
//! ferrata-gasdrop prove --elf GASDROP_BIN --input INPUT_HEX_FILE --out RECEIPT_JSON [--image-id HEX]
//! ferrata-gasdrop relay --receipt RECEIPT_JSON --image-id HEX --me HEX --chain-id N --vault HEX --roots FILE
//!                       --claimed FILE --book FILE --amount N --credit-command CMD [--now UNIX_S]
//! ferrata-gasdrop check-job --image-id HEX --relayer HEX --chain-id N --vault HEX [--accept-dev-mode-receipts] < JOB
//! ```
//!
//! - `check-job` (a network's gateway, deploy/testnet/host/bin/ferrata-gateway, before a posted job counts against its
//!   caps): the job verified offline ([`ferrata_cli::relayer::check_drop_job`]: the receipt against the image id, this
//!   relayer, chain and vault; the one-time keys deriving the receipt's one-time address); not the imported roots or the
//!   claimed nullifiers, which the relayer checks. Prints `{"ok": true, "nullifier", "root"}` (exit 0) or `{"ok":
//!   false, "reason"}` (exit 1). Under RISC0_DEV_MODE it refuses every job unless `--accept-dev-mode-receipts`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{anyhow, ensure, Context as _, Result};
use clap::{Parser, Subcommand};
use ferrata_bridge_core::gasdrop::{verify_gas_drop, GasDropInput, GasDropJournal, GAS_DROP_INPUT_LEN};
use ferrata_bridge_core::Hash;
use ferrata_cli::relayer::{self, DropBook, DropContext, DropRefusal};
use serde_json::{json, Value};

#[derive(Parser)]
#[command(name = "ferrata-gasdrop", about = "The gas drop's receipt (claimant) and the relayer's drop (prototype)")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Prove the gas-drop guest for real over INPUT (1324 bytes, hex) and write the receipt (JSON).
    Prove {
        #[arg(long)]
        elf: PathBuf,
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        out: PathBuf,
        /// Refuse an ELF whose image id is not this one.
        #[arg(long)]
        image_id: Option<String>,
    },
    /// A gateway's check of a gas-drop job (standard input, at most 4 MiB) before it counts against its caps.
    CheckJob {
        #[arg(long)]
        image_id: String,
        /// The relayer the network publishes (its LEZ account, 32-byte hex): the receipt must name it.
        #[arg(long)]
        relayer: String,
        #[arg(long)]
        chain_id: u64,
        #[arg(long)]
        vault: String,
        /// Accept RISC Zero dev-mode receipts (a local devnet whose proofs are fake too; only with RISC0_DEV_MODE set).
        #[arg(long)]
        accept_dev_mode_receipts: bool,
    },
    /// The relayer's gas drop for a receipt: check it, record it, send the credit through CMD.
    Relay {
        #[arg(long)]
        receipt: PathBuf,
        #[arg(long)]
        image_id: String,
        /// This relayer's LEZ account id (the one the receipt must name).
        #[arg(long)]
        me: String,
        #[arg(long)]
        chain_id: u64,
        #[arg(long)]
        vault: String,
        /// The roots LEZ imported for the configuration (hex, one per line).
        #[arg(long)]
        roots: PathBuf,
        /// The nullifiers the bridge program records as claimed (hex, one per line; may be empty).
        #[arg(long)]
        claimed: PathBuf,
        /// The drop book (one drop per N, kept across runs).
        #[arg(long)]
        book: PathBuf,
        #[arg(long)]
        amount: u128,
        #[arg(long)]
        credit_command: String,
        #[arg(long)]
        now: Option<u64>,
    },
}

fn h32(text: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(text.trim().trim_start_matches("0x")).with_context(|| format!("not hex: {text}"))?;
    bytes.try_into().map_err(|b: Vec<u8>| anyhow!("{} bytes, not 32: {text}", b.len()))
}

fn h20(text: &str) -> Result<[u8; 20]> {
    let bytes = hex::decode(text.trim().trim_start_matches("0x")).with_context(|| format!("not hex: {text}"))?;
    bytes.try_into().map_err(|b: Vec<u8>| anyhow!("{} bytes, not 20: {text}", b.len()))
}

fn hx(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

fn word(n: u64) -> [u8; 32] {
    let mut w = [0u8; 32];
    w[24..].copy_from_slice(&n.to_be_bytes());
    w
}

fn hashes(path: &Path) -> Result<HashSet<Hash>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    text.lines().map(str::trim).filter(|l| !l.is_empty()).map(h32).collect()
}

fn journal_json(j: &GasDropJournal) -> Value {
    json!({
        "root": hx(&j.root), "chainId": hx(&j.chain_id), "vault": hx(&j.vault), "nullifier": hx(&j.nullifier),
        "oneTimeAddress": hx(&j.one_time_address), "relayer": hx(&j.relayer),
    })
}

fn prove(elf: &Path, input: &Path, out: &Path, want: Option<&str>) -> Result<Value> {
    ensure!(
        std::env::var_os("RISC0_DEV_MODE").is_none_or(|v| v.is_empty() || v == "0"),
        "RISC0_DEV_MODE is set: this step makes a real receipt only"
    );
    let elf = std::fs::read(elf).with_context(|| format!("reading {}", elf.display()))?;
    let image = risc0_zkvm::compute_image_id(&elf).map_err(|e| anyhow!("{e}"))?;
    let image_hex = hx(image.as_bytes());
    if let Some(want) = want {
        ensure!(h32(want)? == *image.as_bytes(), "the ELF's image id is {image_hex}, not {want}");
    }
    let bytes = hex::decode(std::fs::read_to_string(input)?.trim().trim_start_matches("0x"))?;
    let bytes: [u8; GAS_DROP_INPUT_LEN] =
        bytes.try_into().map_err(|b: Vec<u8>| anyhow!("the input is {} bytes, not {GAS_DROP_INPUT_LEN}", b.len()))?;
    let native = verify_gas_drop(&GasDropInput::decode(&bytes)).map_err(|e| anyhow!("the witness does not verify natively: {e:?}"))?;
    let env = risc0_zkvm::ExecutorEnv::builder().write_slice(&bytes).build().map_err(|e| anyhow!("{e}"))?;
    let started = Instant::now();
    let info = risc0_zkvm::default_prover()
        .prove_with_opts(env, &elf, &risc0_zkvm::ProverOpts::succinct())
        .map_err(|e| anyhow!("r0vm: {e}"))?;
    let seconds = started.elapsed().as_secs_f64();
    info.receipt.verify(image).map_err(|e| anyhow!("the receipt does not verify: {e}"))?;
    ensure!(info.receipt.journal.bytes == native.encode().to_vec(), "the journal is not the native statement's");
    let receipt_json = serde_json::to_vec(&info.receipt)?;
    std::fs::write(out, &receipt_json)?;
    Ok(json!({
        "step": "prove", "imageId": image_hex, "provenS": (seconds * 10.0).round() / 10.0,
        "userCycles": info.stats.user_cycles, "totalCycles": info.stats.total_cycles, "segments": info.stats.segments,
        "receiptJsonBytes": receipt_json.len(), "journalBytes": info.receipt.journal.bytes.len(),
        "journal": journal_json(&native), "verified": true,
    }))
}

/// The native credit through an external command (see the module's documentation).
struct CommandCredit {
    template: String,
}

impl relayer::Credit for CommandCredit {
    async fn credit(&mut self, to: Hash, _keys: Option<&relayer::OneTimeKeys>, amount: u128) -> Result<String> {
        let command = self.template.replace("{to}", &hex::encode(to)).replace("{amount}", &amount.to_string());
        let out = std::process::Command::new("bash")
            .arg("-c")
            .arg(&command)
            .stderr(std::process::Stdio::inherit())
            .output()
            .context("running the credit command")?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        eprint!("{stdout}");
        ensure!(out.status.success(), "the credit command failed ({})", out.status);
        let tx = stdout.lines().rev().map(str::trim).find(|l| !l.is_empty()).ok_or_else(|| anyhow!("no transaction"))?;
        Ok(tx.to_string())
    }
}

#[allow(clippy::too_many_arguments)]
async fn relay(
    receipt: &Path,
    image_id: &str,
    me: &str,
    chain_id: u64,
    vault: &str,
    roots: &Path,
    claimed: &Path,
    book_path: &Path,
    amount: u128,
    credit_command: &str,
    now: Option<u64>,
) -> Result<(Value, bool)> {
    let receipt_json = std::fs::read(receipt).with_context(|| format!("reading {}", receipt.display()))?;
    let imported: &'static HashSet<Hash> = Box::leak(Box::new(hashes(roots)?));
    let claimed = hashes(claimed)?;
    let ctx = DropContext { image_id: h32(image_id)?, me: h32(me)?, chain_id: word(chain_id), vault: h20(vault)?, imported_roots: imported };
    let mut book = DropBook::load(book_path)?;
    let now = match now {
        Some(t) => t,
        None => std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs(),
    };
    // The decision alone first (it is what gas_drop runs before it records anything), for the record.
    let decision = relayer::check_gas_drop(&receipt_json, &ctx, |n| claimed.contains(n), &book);
    if let Err(refusal) = &decision {
        let code = match refusal {
            DropRefusal::BadReceipt(_) => "BadReceipt",
            DropRefusal::NotGasDrop => "NotGasDrop",
            DropRefusal::OtherRelayer => "OtherRelayer",
            DropRefusal::OtherConfiguration => "OtherConfiguration",
            DropRefusal::RootNotImported => "RootNotImported",
            DropRefusal::AlreadyClaimed => "AlreadyClaimed",
            DropRefusal::AlreadyDropped => "AlreadyDropped",
            DropRefusal::KeysMismatch => "KeysMismatch",
        };
        return Ok((json!({ "step": "relay", "decision": "refused", "refusal": code, "reason": refusal.to_string(), "at": now }), false));
    }
    let mut credit = CommandCredit { template: credit_command.to_string() };
    let started = Instant::now();
    let (j, tx) = relayer::gas_drop(&receipt_json, None, &ctx, |n| claimed.contains(n), &mut book, book_path, &mut credit, amount, now).await?;
    Ok((
        json!({
            "step": "relay", "decision": "sent", "at": now, "amount": amount.to_string(), "tx": tx,
            "creditS": (started.elapsed().as_secs_f64() * 10.0).round() / 10.0, "journal": journal_json(&j),
            "book": DropBook::load(book_path)?.drops.get(&hx(&j.nullifier)).cloned(),
        }),
        true,
    ))
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Prove { elf, input, out, image_id } => {
            println!("{}", prove(&elf, &input, &out, image_id.as_deref())?);
        }
        Command::CheckJob { image_id, relayer, chain_id, vault, accept_dev_mode_receipts } => {
            let refuse = |reason: String| -> ! {
                println!("{}", json!({ "ok": false, "reason": reason }));
                std::process::exit(1)
            };
            if risc0_zkvm::VerifierContext::default().dev_mode() && !accept_dev_mode_receipts {
                refuse("RISC0_DEV_MODE is set: a fake receipt would verify; unset it (or, on a local devnet, add --accept-dev-mode-receipts)".into());
            }
            let mut job = Vec::new();
            std::io::Read::read_to_end(&mut std::io::Read::take(std::io::stdin(), 4 << 20), &mut job)?;
            let none: &'static HashSet<Hash> = Box::leak(Box::default());
            let ctx = DropContext { image_id: h32(&image_id)?, me: h32(&relayer)?, chain_id: word(chain_id), vault: h20(&vault)?, imported_roots: none };
            match relayer::check_drop_job(&job, &ctx) {
                Ok(j) => println!(
                    "{}",
                    json!({ "ok": true, "nullifier": format!("0x{}", hex::encode(j.nullifier)), "root": format!("0x{}", hex::encode(j.root)) })
                ),
                Err(r) => refuse(r.to_string()),
            }
        }
        Command::Relay { receipt, image_id, me, chain_id, vault, roots, claimed, book, amount, credit_command, now } => {
            let (value, sent) =
                relay(&receipt, &image_id, &me, chain_id, &vault, &roots, &claimed, &book, amount, &credit_command, now).await?;
            println!("{value}");
            if !sent {
                std::process::exit(3);
            }
        }
    }
    Ok(())
}
