//! `bridge03`: drives the Ferrata bridge program for LEZ v0.3.1 on a running sequencer (a local
//! standalone one, or the public testnet) through LEZ v0.3.1's own wallet (`lez/wallet`): its key
//! storage, its program loader, its public sends and its privacy-preserving transaction, which the
//! wallet proves itself (`execute_and_prove`, real proofs unless `RISC0_DEV_MODE` is set).
//! EXPERIMENTAL: see `../README.md`. Used by `../testnet.sh`.
//!
//! `bridge03 <run dir> <command> [key value]...`; everything a run makes is kept in `<run dir>`
//! (`run.json`, the wallet's storage under `wallet/`), never in the repository. Each sending step
//! records its transaction hash and block in `run.json` and is skipped when run again (resumable).

#[allow(dead_code)]
#[path = "../../harness/src/deposits.rs"]
mod deposits;
#[allow(dead_code)]
#[path = "../../harness/src/eth.rs"]
mod eth;

use std::{collections::HashMap, path::PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use ferrata_bridge_core::{inbound::DestKind, sha256};
use ferrata_lez03_bridge_core::{
    self as bc, burn_account, burns_account, config_account, header_account, holder_account, nullifier_account,
    root_account, BridgeConfig, BridgeState, BurnFrom, Denomination, Instruction, LcRecord, Note, RecordRef, Root,
    RootState, ETH,
};
use lee::{privacy_preserving_transaction::circuit::ProgramWithDependencies, program::Program, AccountId, PrivateKey};
use serde_json::{json, Value};
use wallet::{program_facades::program_loader::ProgramLoader, AccDecodeData, AccountIdentity, AccountMention, WalletCore};

const BRIDGE_ELF: &[u8] = methods03::FERRATA_BRIDGE03_ELF;
const RECORDS_ELF: &[u8] = methods03::FERRATA_RECORDS03_ELF;

fn image_id(words: [u32; 8]) -> Root {
    let mut out = [0u8; 32];
    for (i, w) in words.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&w.to_le_bytes());
    }
    out
}

fn h(bytes: &[u8]) -> String {
    hex::encode(bytes)
}

fn unhex<const N: usize>(s: &str) -> Result<[u8; N]> {
    let v = hex::decode(s.trim_start_matches("0x")).with_context(|| format!("hex: {s}"))?;
    v.try_into().map_err(|_| anyhow!("{s}: not {N} bytes"))
}

fn account(s: &str) -> Result<AccountId> {
    s.parse().map_err(|e| anyhow!("account id {s}: {e:?}"))
}

/// `run.json` and the wallet of one run.
struct Run {
    dir: PathBuf,
    rec: Value,
}

impl Run {
    fn open(dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("run.json");
        let rec = if path.exists() { serde_json::from_slice(&std::fs::read(&path)?)? } else { json!({}) };
        Ok(Self { dir, rec })
    }

    fn save(&self) -> Result<()> {
        std::fs::write(self.dir.join("run.json"), serde_json::to_vec_pretty(&self.rec)?)?;
        Ok(())
    }

    fn get(&self, key: &str) -> Result<&str> {
        self.rec[key].as_str().ok_or_else(|| anyhow!("run.json has no {key}"))
    }

    fn acc(&self, key: &str) -> Result<AccountId> {
        account(self.get(key)?)
    }

    /// The wallet home: the run's own (`<run>/wallet`), or an existing one named at setup
    /// (`--wallet-home`, e.g. the queued sender's, so that one home signs every transaction).
    fn wallet_paths(&self) -> (PathBuf, PathBuf) {
        let w = match self.rec["wallet_home"].as_str() {
            Some(home) => PathBuf::from(home),
            None => self.dir.join("wallet"),
        };
        (w.join("wallet_config.json"), w.join("storage.json"))
    }

    fn wallet(&self) -> Result<WalletCore> {
        let (config, storage) = self.wallet_paths();
        WalletCore::new_update_chain(config, storage, None).context("the run's wallet")
    }

    fn config(&self) -> Result<BridgeConfig> {
        Ok(borsh::from_slice(&hex::decode(self.get("config")?)?)?)
    }

    fn config_id(&self) -> Result<Root> {
        unhex(self.get("config_id")?)
    }

    fn done(&self, step: &str) -> bool {
        self.rec["steps"][step]["block"].is_u64()
    }

    fn record_step(&mut self, step: &str, value: Value) -> Result<()> {
        if !self.rec["steps"].is_object() {
            self.rec["steps"] = json!({});
        }
        self.rec["steps"][step] = value;
        self.save()
    }
}

/// `key value` pairs after the command.
fn args(rest: &[String]) -> Result<HashMap<String, String>> {
    if rest.len() % 2 != 0 {
        bail!("arguments come in `key value` pairs");
    }
    Ok(rest.chunks(2).map(|kv| (kv[0].trim_start_matches("--").to_owned(), kv[1].clone())).collect())
}

fn arg<'a>(a: &'a HashMap<String, String>, key: &str) -> Result<&'a str> {
    a.get(key).map(String::as_str).ok_or_else(|| anyhow!("missing --{key}"))
}

#[tokio::main]
async fn main() -> Result<()> {
    let argv: Vec<String> = std::env::args().collect();
    if argv.get(1).map(String::as_str) == Some("ids") {
        return ids();
    }
    if argv.len() < 3 {
        bail!("usage: bridge03 ids | bridge03 <run dir> <command> [--key value]...");
    }
    let mut run = Run::open(PathBuf::from(&argv[1]))?;
    let a = args(&argv[3..])?;
    match argv[2].as_str() {
        "setup" => setup(&mut run, &a).await,
        "deploy" => deploy(&mut run, arg(&a, "program")?).await,
        "config" => config(&mut run, &a),
        "deposits" => deposits_cmd(&mut run, &a),
        "record" => record(&mut run, &a).await,
        "init" => init(&mut run, &a).await,
        "import" => import(&mut run, &a).await,
        "claim" => claim(&mut run, &a, false).await,
        "claim-private" => claim(&mut run, &a, true).await,
        "burn" => burn(&mut run, &a, false).await,
        "burn-private" => burn(&mut run, &a, true).await,
        "state" => state(&run).await,
        "batch" => batch(&mut run, &a),
        "layout" => layout(&run),
        "burns" => burns_file(&run),
        other => bail!("unknown command {other}"),
    }
}

fn ids() -> Result<()> {
    let b = Program::new(BRIDGE_ELF.to_vec().into())?;
    let r = Program::new(RECORDS_ELF.to_vec().into())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "bridge_image_id": h(&image_id(methods03::FERRATA_BRIDGE03_ID)),
            "bridge_program_id": format!("{:?}", b.id()),
            "bridge_elf_bytes": BRIDGE_ELF.len(),
            "records_image_id": h(&image_id(methods03::FERRATA_RECORDS03_ID)),
            "records_program_id": format!("{:?}", r.id()),
            "records_elf_bytes": RECORDS_ELF.len(),
            "privacy_preserving_circuit_id": format!("{:?}", lee::PRIVACY_PRESERVING_CIRCUIT_ID),
        }))?
    );
    Ok(())
}

/// The wallet (created once: config naming the sequencer, storage under a password from
/// `FERRATA_WALLET_PASSWORD`), the payer imported from a key file (32 bytes, hex) or, on a local
/// sequencer, LEZ's published public test account A (`--payer testnet-a`), and the run's accounts:
/// the programs' header and segment accounts, a public holder (it signs its burn) and a private
/// holder (a private account of the wallet).
async fn setup(run: &mut Run, a: &HashMap<String, String>) -> Result<()> {
    if run.rec["payer"].is_string() {
        println!("setup: already done ({})", run.dir.display());
        return Ok(());
    }
    let sequencer = arg(a, "sequencer")?;
    if let Some(home) = a.get("wallet-home") {
        // An existing wallet home (its config names its network): this run's accounts are made there, and the
        // payer is one of its public accounts (`--payer-id`).
        run.rec["wallet_home"] = json!(home);
        let mut w = run.wallet()?;
        let payer = account(arg(a, "payer-id")?)?;
        w.get_account_public_signing_key(payer).ok_or_else(|| anyhow!("the payer is not an account of {home}"))?;
        return accounts(run, &mut w, sequencer, payer);
    }
    let (config_path, storage_path) = run.wallet_paths();
    std::fs::create_dir_all(config_path.parent().unwrap())?;
    std::fs::write(
        &config_path,
        serde_json::to_vec_pretty(&json!({
            "sequencer": { "sequencer_addr": sequencer },
            "seq_poll_timeout": "30s",
            "seq_tx_poll_max_blocks": 40,
            "seq_poll_max_retries": 20,
            "seq_block_poll_max_amount": 100,
        }))?,
    )?;
    // A new wallet's password: the caller's; a public default only for a loopback (local) sequencer (finding 13).
    // No userinfo: in `http://127.0.0.1:1@HOST` the client connects to HOST.
    let local = (sequencer.starts_with("http://127.0.0.1:") || sequencer.starts_with("http://localhost:"))
        && !sequencer.split("://").nth(1).unwrap_or("").split(['/', '?', '#']).next().unwrap_or("").contains('@');
    let password = match std::env::var("FERRATA_WALLET_PASSWORD") {
        Ok(p) if !p.is_empty() => p,
        _ if local => "ferrata-v03-bridge-local".into(),
        _ => bail!("FERRATA_WALLET_PASSWORD is required for a new wallet of a non-local sequencer ({sequencer})"),
    };
    let (mut w, _mnemonic) = WalletCore::new_init_storage(config_path, storage_path, None, &password)?;
    let payer_key = match arg(a, "payer")? {
        "testnet-a" => testnet_initial_state::initial_pub_accounts_private_keys()[0].pub_sign_key.clone(),
        file => {
            let bytes: [u8; 32] = unhex(std::fs::read_to_string(file)?.trim())?;
            PrivateKey::try_new(bytes)?
        }
    };
    let payer = AccountId::from(&lee::PublicKey::new_from_private_key(&payer_key));
    w.storage_mut().key_chain_mut().add_imported_public_account(payer_key);
    accounts(run, &mut w, sequencer, payer)
}

/// The run's accounts, made in the wallet `w`: the programs' header and segment accounts, a public holder (it signs
/// its burn) and a private holder.
fn accounts(run: &mut Run, w: &mut WalletCore, sequencer: &str, payer: AccountId) -> Result<()> {
    let w = &mut *w;
    let new_public = |w: &mut WalletCore| w.create_new_account_public(None).0.to_string();
    let segments = |w: &mut WalletCore, elf: &[u8]| -> Vec<String> {
        (0..elf.len().div_ceil(program_loader_segment())).map(|_| w.create_new_account_public(None).0.to_string()).collect()
    };
    let bridge_header = new_public(w);
    let bridge_segments = segments(w, &program_user_elf(BRIDGE_ELF)?);
    let records_header = new_public(w);
    let records_segments = segments(w, &program_user_elf(RECORDS_ELF)?);
    let holder_public = new_public(w);
    let holder_private = w.create_new_account_private(None).0.to_string();
    w.store_persistent_data()?;
    run.rec["sequencer"] = json!(sequencer);
    run.rec["payer"] = json!(payer.to_string());
    run.rec["bridge"] = json!(bridge_header);
    run.rec["bridge_segments"] = json!(bridge_segments);
    run.rec["records"] = json!(records_header);
    run.rec["records_segments"] = json!(records_segments);
    run.rec["holder_public"] = json!(holder_public);
    run.rec["holder_private"] = json!(holder_private);
    run.save()?;
    println!("{}", serde_json::to_string_pretty(&run.rec)?);
    Ok(())
}

fn program_loader_segment() -> usize {
    96 * 1024
}

/// The user ELF the loader stores (the program binary's).
fn program_user_elf(elf: &[u8]) -> Result<Vec<u8>> {
    Ok(elf.to_vec())
}

async fn deploy(run: &mut Run, which: &str) -> Result<()> {
    let step = format!("deploy-{which}");
    if run.done(&step) {
        println!("{step}: done");
        return Ok(());
    }
    let (elf, header_key, segments_key) = match which {
        "bridge" => (BRIDGE_ELF, "bridge", "bridge_segments"),
        "records" => (RECORDS_ELF, "records", "records_segments"),
        _ => bail!("--program bridge | records"),
    };
    let header = run.acc(header_key)?;
    let segments: Vec<AccountId> = run.rec[segments_key]
        .as_array()
        .ok_or_else(|| anyhow!("no segments"))?
        .iter()
        .map(|s| account(s.as_str().unwrap()))
        .collect::<Result<_>>()?;
    let payer = run.acc("payer")?;
    let w = run.wallet()?;
    let t0 = std::time::Instant::now();
    let at = ProgramLoader(&w).deploy(header, &segments, elf.to_vec(), true, Some(payer)).await?;
    let block = w.get_last_block_id().await?;
    run.record_step(&step, json!({ "header": at.to_string(), "block": block, "seconds": t0.elapsed().as_secs_f64() }))?;
    println!("{step}: program at {at}, last block {block}");
    Ok(())
}

/// The configuration: this run's bridge, the light client (`--lc`, default this run's stand-in
/// records program), its instance, the vault and the registry (ETH in `--eth` wei, and optionally
/// one ERC-20 `--token 0x…:units`).
fn config(run: &mut Run, a: &HashMap<String, String>) -> Result<()> {
    let bridge = run.acc("bridge")?;
    let lc = match a.get("lc") {
        Some(lc) => account(lc)?,
        None => run.acc("records")?,
    };
    let mut registry = vec![Denomination { token: ETH, amount: arg(a, "eth")?.parse()? }];
    if let Some(t) = a.get("token") {
        let (token, units) = t.split_once(':').ok_or_else(|| anyhow!("--token 0x…:units"))?;
        registry.push(Denomination { token: unhex(token)?, amount: units.parse()? });
    }
    let image: Root = match a.get("image") {
        Some(i) => unhex(i)?,
        None => image_id(methods03::FERRATA_BRIDGE03_ID),
    };
    let config = BridgeConfig {
        bedrock_genesis_id: match a.get("bedrock-genesis") {
            Some(g) => unhex(g)?,
            None => sha256(b"ferrata/lez-v03-bridge/bedrock-genesis/unset"),
        },
        channel_id: match a.get("channel") {
            Some(c) => unhex(c)?,
            None => [1; 32],
        },
        bridge_image_id: image,
        attestation: lc,
        instance_id: unhex(arg(a, "instance")?)?,
        evm_chain_id: arg(a, "chain")?.parse()?,
        vault: unhex(arg(a, "vault")?)?,
        registry,
    };
    config.check().map_err(|c| anyhow!("registry refused: {c}"))?;
    let cid = config.id(bridge);
    run.rec["config"] = json!(h(&borsh::to_vec(&config)?));
    run.rec["config_id"] = json!(h(&cid));
    run.rec["config_json"] = json!({
        "bridge": bridge.to_string(), "attestation": lc.to_string(), "instance": h(&config.instance_id),
        "chain": config.evm_chain_id, "vault": h(&config.vault), "bridge_image_id": h(&image),
        "registry": config.registry.iter().map(|d| json!({"token": h(&d.token), "amount": d.amount.to_string()})).collect::<Vec<_>>(),
    });
    run.save()?;
    println!("0x{}", h(&cid));
    Ok(())
}

/// Two deposits' openings: deposit `public` (public kind, to the public holder) and deposit
/// `private` (private kind, to the private holder), with the wallet's secrets from `--seed`, for
/// `--depositor` at its deposit counters `--j0` and `--j0 + 1`, leaves `--leaf0`, `--leaf0 + 1`.
fn deposits_cmd(run: &mut Run, a: &HashMap<String, String>) -> Result<()> {
    let config = run.config()?;
    let keys = deposits::keys(arg(a, "seed")?);
    let depositor: [u8; 20] = unhex(arg(a, "depositor")?)?;
    let j0: u64 = a.get("j0").map_or(Ok(0), |s| s.parse())?;
    let leaf0: u64 = a.get("leaf0").map_or(Ok(0), |s| s.parse())?;
    let amount = config.registry[0].amount;
    let public = *run.acc("holder_public")?.value();
    let private = *run.acc("holder_private")?.value();
    let mut out = Vec::new();
    for (k, (kind, dest)) in [(DestKind::Public, public), (DestKind::Private, private)].into_iter().enumerate() {
        let k = k as u64;
        let d = deposits::Deposit::new(&keys, config.evm_chain_id, &config.vault, depositor, j0 + k, leaf0 + k, ETH, amount, kind, dest);
        out.push(json!({
            "leaf_index": d.leaf_index, "kind": kind as u8, "dest": h(&d.dest), "s": h(&d.s),
            "commitment": h(&d.commitment()), "leaf": h(&d.leaf()), "amount": amount.to_string(),
            "depositor": h(&depositor),
        }));
    }
    run.rec["deposits"] = json!(out);
    run.save()?;
    println!("{}", serde_json::to_string_pretty(&run.rec["deposits"])?);
    Ok(())
}

fn deposit(run: &Run, k: usize) -> Result<deposits::Deposit> {
    let d = &run.rec["deposits"][k];
    let s = |key: &str| d[key].as_str().ok_or_else(|| anyhow!("deposit {k}: no {key}"));
    Ok(deposits::Deposit {
        leaf_index: d["leaf_index"].as_u64().unwrap(),
        depositor: unhex(s("depositor")?)?,
        token: ETH,
        amount: bc::word_of_u128(s("amount")?.parse()?),
        s: unhex(s("s")?)?,
        kind: DestKind::from_byte(d["kind"].as_u64().unwrap() as u8).unwrap(),
        dest: unhex(s("dest")?)?,
    })
}

/// A capture: `--capture <dir>` with `header.rlp` and the vault's `eth_getProof` answer `--proof`,
/// recorded under the light client's record of beacon slot `--slot`.
fn captured(a: &HashMap<String, String>) -> Result<(eth::Captured, u64)> {
    let slot: u64 = arg(a, "slot")?.parse()?;
    Ok((eth::Captured::load(&PathBuf::from(arg(a, "capture")?), arg(a, "proof")?, slot), slot))
}

fn record_ref(run: &Run, captured: &eth::Captured, slot: u64) -> Result<RecordRef> {
    let config = run.config()?;
    let state = LcRecord { instance_id: config.instance_id, chain_id: config.evm_chain_id, record: captured.record.encode() };
    Ok(RecordRef { slot, state: borsh::to_vec(&state)? })
}

fn mention(id: AccountId, program: AccountId) -> AccountMention {
    AccountIdentity::PublicNoSign(id).select_program_shard(program)
}

/// Send a public transaction paid by the run's payer, wait for its block, record it.
async fn public(run: &mut Run, step: &str, accounts: Vec<AccountMention>, program: AccountId, instruction: Vec<u8>) -> Result<()> {
    if run.done(step) {
        println!("{step}: done (block {})", run.rec["steps"][step]["block"]);
        return Ok(());
    }
    let payer = run.acc("payer")?;
    let w = run.wallet()?;
    let before = w.get_account_balance(payer).await?;
    let hash = w.send_pub_tx_paid_by(accounts, instruction, program, Some(payer)).await.map_err(|e| anyhow!("{e}"))?;
    let (tx, block) = w.poll_transaction(hash).await?;
    let after = w.get_account_balance(payer).await?;
    let bytes = borsh::to_vec(&tx)?.len();
    run.record_step(step, json!({ "tx": h(&hash.0), "block": block, "bytes": bytes, "fee": (before - after).to_string() }))?;
    println!("{step}: tx {} in block {block}, {bytes} bytes, fee {}", h(&hash.0), before - after);
    Ok(())
}

/// The stand-in light client writes the capture's record (local runs only).
async fn record(run: &mut Run, a: &HashMap<String, String>) -> Result<()> {
    let (c, slot) = captured(a)?;
    let config = run.config()?;
    let r = record_ref(run, &c, slot)?;
    let lc = config.attestation;
    let at = header_account(lc, &config.instance_id, slot);
    #[derive(borsh::BorshSerialize)]
    struct Write {
        instance_id: Root,
        slot: u64,
        state: Vec<u8>,
    }
    let i = borsh::to_vec(&Write { instance_id: config.instance_id, slot, state: r.state })?;
    public(run, &format!("record-{slot}"), vec![mention(at, lc)], lc, i).await
}

async fn init(run: &mut Run, a: &HashMap<String, String>) -> Result<()> {
    let (c, slot) = captured(a)?;
    let config = run.config()?;
    let bridge = run.acc("bridge")?;
    let cid = run.config_id()?;
    let r = record_ref(run, &c, slot)?;
    let accounts = vec![
        mention(config_account(bridge, &cid), bridge),
        mention(header_account(config.attestation, &config.instance_id, slot), config.attestation),
    ];
    let i = Instruction::Init { config, record: r, proof: c.pairing_proof() };
    let step = a.get("step").cloned().unwrap_or_else(|| "init".into());
    run.rec["record_slot"] = json!(slot);
    public(run, &step, accounts, bridge, borsh::to_vec(&i)?).await
}

async fn import(run: &mut Run, a: &HashMap<String, String>) -> Result<()> {
    let (c, slot) = captured(a)?;
    let config = run.config()?;
    let bridge = run.acc("bridge")?;
    let cid = run.config_id()?;
    let n: u64 = arg(a, "n")?.parse()?;
    let r = record_ref(run, &c, slot)?;
    let root = c.value(&bc::SLOT_ROOT);
    let accounts = vec![
        mention(config_account(bridge, &cid), bridge),
        mention(header_account(config.attestation, &config.instance_id, slot), config.attestation),
        mention(root_account(bridge, &cid, n), bridge),
    ];
    let i = Instruction::ImportRoot { config, record: r, root, leaf_count: n, proof: c.import_proof() };
    public(run, &format!("import-{n}"), accounts, bridge, borsh::to_vec(&i)?).await
}

/// The root state `root(cid, n)` holds, read from the sequencer.
async fn root_state(run: &Run, n: u64) -> Result<RootState> {
    let bridge = run.acc("bridge")?;
    let w = run.wallet()?;
    let acc = w.get_account_public(root_account(bridge, &run.config_id()?, n)).await?;
    match BridgeState::decode(acc.data.shard(bridge)).map_err(|c| anyhow!("root(cid, {n}): {c}"))? {
        BridgeState::Root(r) => Ok(r),
        other => bail!("root(cid, {n}) holds {other:?}"),
    }
}

/// The root state `ImportRoot` of the capture writes at `root(cid, n)` (the same fields, computed here).
fn local_root(run: &Run, a: &HashMap<String, String>, n: u64) -> Result<RootState> {
    let (c, slot) = captured(a)?;
    let config = run.config()?;
    let number = ferrata_core::exec::decode_header(&c.header).map_err(|e| anyhow!("header: {e:?}"))?.number;
    Ok(RootState {
        config_id: run.config_id()?,
        evm_chain_id: config.evm_chain_id,
        vault: config.vault,
        leaf_count: n,
        root: c.value(&bc::SLOT_ROOT),
        record_slot: slot,
        block_number: number,
    })
}

/// The leaves of the vault, as `deposits` recorded them (`--leaves` more before them are given as
/// hex, comma-separated, if the vault had earlier deposits).
fn leaves(run: &Run, a: &HashMap<String, String>) -> Result<Vec<Root>> {
    let mut out: Vec<Root> = match a.get("leaves-before") {
        Some(l) if !l.is_empty() => l.split(',').map(unhex).collect::<Result<_>>()?,
        _ => Vec::new(),
    };
    for k in 0..2 {
        out.push(deposit(run, k)?.leaf());
    }
    Ok(out)
}

async fn claim(run: &mut Run, a: &HashMap<String, String>, private: bool) -> Result<()> {
    let k: usize = arg(a, "deposit")?.parse()?;
    let n: u64 = arg(a, "n")?.parse()?;
    let config = run.config()?;
    let bridge = run.acc("bridge")?;
    // From the capture (the state `ImportRoot` writes, so a claim can be queued behind its import in one block),
    // or as the sequencer holds it.
    let root = match a.get("capture") {
        Some(_) => local_root(run, a, n)?,
        None => root_state(run, n).await?,
    };
    let d = deposit(run, k)?;
    let mut w = d.witness(&leaves(run, a)?, n);
    if let Some(dest) = a.get("dest") {
        w.dest = *account(dest)?.value(); // a claim to another destination (refused)
    }
    let nullifier = deposits::nullifier_of(&w, root.evm_chain_id, &root.vault);
    let step = a.get("step").cloned().unwrap_or_else(|| format!("claim-{k}"));
    let at_root = mention(root_account(bridge, &root.config_id, n), bridge);
    let at_nullifier = mention(nullifier_account(bridge, &root.config_id, root.evm_chain_id, &root.vault, &nullifier), bridge);
    if !private {
        let accounts = vec![at_root, at_nullifier, mention(holder_account(bridge, &root.config_id, &w.dest), bridge)];
        let i = Instruction::Claim { config, root, witness: w };
        return public(run, &step, accounts, bridge, borsh::to_vec(&i)?).await;
    }
    if run.done(&step) {
        println!("{step}: done");
        return Ok(());
    }
    let dest = run.acc("holder_private")?;
    let accounts = vec![AccountIdentity::PrivateOwned(dest).select_program_shard(bridge), at_root, at_nullifier];
    let i = Instruction::ClaimPrivately { config, root, witness: w };
    private_tx(run, &step, accounts, dest, borsh::to_vec(&i)?).await
}

/// A privacy-preserving transaction proven by the wallet, sent, waited for, and its result
/// decoded into the wallet's storage (the private holder's new state).
async fn private_tx(run: &mut Run, step: &str, accounts: Vec<AccountMention>, holder: AccountId, instruction: Vec<u8>) -> Result<()> {
    let bridge = run.acc("bridge")?;
    let mut w = run.wallet()?;
    let program = Program::new(BRIDGE_ELF.to_vec().into())?;
    let deps = ProgramWithDependencies::new(program, bridge, HashMap::new());
    let t0 = std::time::Instant::now();
    let (hash, secrets) =
        w.send_privacy_preserving_tx(accounts, instruction, &deps).await.map_err(|e| anyhow!("{e}"))?;
    let proven_and_sent = t0.elapsed().as_secs_f64();
    let (tx, block) = w.poll_transaction(hash).await?;
    let bytes = borsh::to_vec(&tx)?.len();
    let common::transaction::LeeTransaction::PrivacyPreserving(ptx) = &tx else { bail!("not a private transaction") };
    let decode: Vec<AccDecodeData> = secrets.into_iter().map(|s| AccDecodeData::Decode(s, holder)).take(1).collect();
    w.decode_insert_privacy_preserving_transaction_results(ptx, &decode)?;
    w.store_persistent_data()?;
    let dev = std::env::var("RISC0_DEV_MODE").map(|v| v != "0" && !v.is_empty()).unwrap_or(false);
    run.record_step(
        step,
        json!({ "tx": h(&hash.0), "block": block, "bytes": bytes, "proven_and_sent_seconds": proven_and_sent, "dev_mode": dev }),
    )?;
    println!("{step}: tx {} in block {block}, {bytes} bytes, proven and sent in {proven_and_sent:.1} s (dev mode {dev})", h(&hash.0));
    Ok(())
}

async fn burn(run: &mut Run, a: &HashMap<String, String>, private: bool) -> Result<()> {
    let config = run.config()?;
    let bridge = run.acc("bridge")?;
    let cid = run.config_id()?;
    let index: u64 = arg(a, "index")?.parse()?;
    let recipient: [u8; 20] = unhex(arg(a, "recipient")?)?;
    let keys = deposits::keys(arg(a, "seed")?);
    let note: Note = deposits::note(&keys, &cid, index, config.evm_chain_id, config.vault, recipient, ETH, config.registry[0].amount);
    let step = format!("burn-{}", if private { "private" } else { "public" });
    let at_record = mention(burn_account(bridge, &cid, &note.hash()), bridge);
    let at_burns = mention(burns_account(bridge, &cid), bridge);
    let sent = match run.rec["notes"][&step]["sent"].as_u64() {
        Some(i) => i,
        None => run.rec["notes"].as_object().map_or(0, |m| m.len()) as u64,
    };
    run.rec["notes"][&step] = json!({ "r": h(&note.r), "memo": h(&note.memo), "mac": h(&note.mac), "c_out": h(&note.c_out),
        "token": h(&note.token), "amount": h(&note.amount), "hash": h(&note.hash()), "sent": sent });
    if private {
        if run.done(&step) {
            println!("{step}: done");
            return Ok(());
        }
        let holder = run.acc("holder_private")?;
        let accounts = vec![AccountIdentity::PrivateOwned(holder).select_program_shard(bridge), at_record, at_burns];
        let i = Instruction::Burn { config, note, from: BurnFrom::Own };
        return private_tx(run, &step, accounts, holder, borsh::to_vec(&i)?).await;
    }
    let owner = run.acc("holder_public")?;
    let accounts = vec![
        AccountIdentity::Public(owner).select_program_shard(bridge),
        mention(holder_account(bridge, &cid, owner.value()), bridge),
        at_record,
        at_burns,
    ];
    let i = Instruction::Burn { config, note, from: BurnFrom::Holder };
    public(run, &step, accounts, bridge, borsh::to_vec(&i)?).await
}

/// Every account the run touched, read back from the sequencer: its id, the bridge shard's bytes and
/// their decoding. `state.json` of a run is this output; `../verify.py` compares the bytes again.
async fn state(run: &Run) -> Result<()> {
    let bridge = run.acc("bridge")?;
    let cid = run.config_id()?;
    let config = run.config()?;
    let w = run.wallet()?;
    let mut ids: Vec<(String, AccountId, AccountId)> = vec![
        ("config".into(), config_account(bridge, &cid), bridge),
        ("burns".into(), burns_account(bridge, &cid), bridge),
    ];
    for n in [1u64, 2] {
        ids.push((format!("root_{n}"), root_account(bridge, &cid, n), bridge));
    }
    if run.rec["deposits"].is_array() {
        for k in 0..2 {
            let d = deposit(run, k)?;
            let nf = d.nullifier(config.evm_chain_id, &config.vault);
            ids.push((format!("nullifier_{k}"), nullifier_account(bridge, &cid, config.evm_chain_id, &config.vault, &nf), bridge));
        }
    }
    let public_holder = run.acc("holder_public")?;
    ids.push(("holder_public".into(), holder_account(bridge, &cid, public_holder.value()), bridge));
    if let Some(n) = run.rec["notes"].as_object() {
        for (step, note) in n {
            let hash: Root = unhex(note["hash"].as_str().unwrap_or_default())?;
            ids.push((format!("note_{step}"), burn_account(bridge, &cid, &hash), bridge));
        }
    }
    if let Some(slot) = run.rec["record_slot"].as_u64() {
        ids.push(("record".into(), header_account(config.attestation, &config.instance_id, slot), config.attestation));
    }
    let mut accounts = serde_json::Map::new();
    for (name, id, program) in ids {
        let acc = w.get_account_public(id).await?;
        let data = acc.data.shard(program);
        let decoded = if data.is_empty() {
            Value::Null
        } else if program == bridge {
            json!(format!("{:?}", BridgeState::decode(data).map_err(|c| anyhow!("{name}: {c}"))?))
        } else {
            json!("a light client record")
        };
        accounts.insert(name, json!({ "id": id.to_string(), "program": program.to_string(), "shard": h(data), "decoded": decoded }));
    }
    let mut out = json!({ "config_id": h(&cid), "bridge": bridge.to_string(), "last_block": w.get_last_block_id().await?, "accounts": accounts });
    if let Some(p) = w.get_account_private(run.acc("holder_private")?) {
        let data = p.data.shard(bridge);
        out["holder_private (the wallet's decoded private state; on chain only its commitment)"] =
            if data.is_empty() { Value::Null } else { json!(format!("{:?}", BridgeState::decode(data).map_err(|c| anyhow!("{c}"))?)) };
    }
    if let Some(Ok(BridgeState::Burns(t))) =
        accounts_shard(&out, "burns").map(|b| BridgeState::decode(&b))
    {
        out["burn_tree"] = json!({ "count": t.count, "root": h(&t.root().unwrap_or_default()) });
    }
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

fn accounts_shard(out: &Value, name: &str) -> Option<Vec<u8>> {
    let s = out["accounts"][name]["shard"].as_str()?;
    if s.is_empty() {
        return None;
    }
    hex::decode(s).ok()
}

/// Declared gas limits of the public batch: the largest cycles measured through LEZ v0.3.1's state machine (results/:
/// the synthetic and the real-Sepolia runs) times 1.50 to 1.84, rounded up. A block admits a
/// transaction only if the gas charged so far plus its declared limit stays within 10,000,000
/// (testnet-ops's notes, outside the repository); a refused one is charged its cycles, a panic its whole limit.
// The largest cycles measured (results/, image d75d06ee…): Init 217,196 (Sepolia's real proofs, the plan's whole check),
// ImportRoot 285,133 (Sepolia), Claim 156,336 (synthetic), public Burn 280,003 (synthetic): 1.84, 1.58, 1.60, 1.50 times.
const GAS_INIT: u64 = 400_000;
const GAS_IMPORT: u64 = 450_000;
const GAS_CLAIM: u64 = 250_000;
const GAS_BURN: u64 = 420_000;

/// The public steps of a run as one batch for the queued sender (`ferrata_ops batch`, format of
/// testnet-ops's notes, outside the repository), all for one block, in order: the bridge's deployment (if not done),
/// `Init`, `ImportRoot`, the public claim of deposit 0, the public holder's burn (`--burn yes`, default; the claim
/// must have landed first only for the holder to hold it: in one block, in order, it has). The private claim and the
/// private burn are privacy-preserving transactions, proven and sent by `claim-private` and `burn-private` with the
/// same wallet home (fee-exempt; the private burn after the private claim's block). Writes the program's ELF
/// beside it. Nothing is sent.
fn batch(run: &mut Run, a: &HashMap<String, String>) -> Result<()> {
    let out = PathBuf::from(arg(a, "out")?);
    let config = run.config()?;
    let bridge = run.acc("bridge")?;
    let cid = run.config_id()?;
    let (c, slot) = captured(a)?;
    let n: u64 = arg(a, "n")?.parse()?;
    let r = record_ref(run, &c, slot)?;
    let sh = |id: AccountId, p: AccountId| json!([id.to_string(), p.to_string()]);
    let header = header_account(config.attestation, &config.instance_id, slot);
    let call = |label: &str, shards: Vec<Value>, signers: Vec<String>, data: Vec<u8>, gas: u64| {
        json!({ "call": { "label": label, "program": bridge.to_string(), "shards": shards, "signers": signers,
                          "data_hex": h(&data), "gas_limit": gas } })
    };
    let mut steps = Vec::new();
    if !run.done("deploy-bridge") {
        let elf = run.dir.join("ferrata_bridge03.bin");
        std::fs::write(&elf, BRIDGE_ELF)?;
        steps.push(json!({ "deploy": { "elf": elf.display().to_string(), "header": run.get("bridge")?,
                                        "segments": run.rec["bridge_segments"], "immutable": true } }));
    }
    let init = Instruction::Init { config: config.clone(), record: r.clone(), proof: c.pairing_proof() };
    steps.push(call(
        "init",
        vec![sh(config_account(bridge, &cid), bridge), sh(header, config.attestation)],
        vec![],
        borsh::to_vec(&init)?,
        GAS_INIT,
    ));
    let root = c.value(&bc::SLOT_ROOT);
    let import = Instruction::ImportRoot { config: config.clone(), record: r, root, leaf_count: n, proof: c.import_proof() };
    steps.push(call(
        &format!("import-{n}"),
        vec![sh(config_account(bridge, &cid), bridge), sh(header, config.attestation), sh(root_account(bridge, &cid, n), bridge)],
        vec![],
        borsh::to_vec(&import)?,
        GAS_IMPORT,
    ));
    let root_state = local_root(run, a, n)?;
    let d = deposit(run, 0)?;
    let w = d.witness(&leaves(run, a)?, n);
    let nf = deposits::nullifier_of(&w, root_state.evm_chain_id, &root_state.vault);
    let claim = Instruction::Claim { config: config.clone(), root: root_state, witness: w.clone() };
    steps.push(call(
        "claim-0",
        vec![
            sh(root_account(bridge, &cid, n), bridge),
            sh(nullifier_account(bridge, &cid, root_state.evm_chain_id, &root_state.vault, &nf), bridge),
            sh(holder_account(bridge, &cid, &w.dest), bridge),
        ],
        vec![],
        borsh::to_vec(&claim)?,
        GAS_CLAIM,
    ));
    if a.get("burn").map_or(true, |b| b == "yes") {
        let keys = deposits::keys(arg(a, "seed")?);
        let recipient: [u8; 20] = unhex(arg(a, "recipient")?)?;
        let note: Note = deposits::note(&keys, &cid, 0, config.evm_chain_id, config.vault, recipient, ETH, config.registry[0].amount);
        let owner = run.acc("holder_public")?;
        // The note, as `burn` records it: `state` reads its account, `burns` its leaf (first burn sent).
        let sent = run.rec["notes"]["burn-public"]["sent"].as_u64().unwrap_or(0);
        run.rec["notes"]["burn-public"] = json!({ "r": h(&note.r), "memo": h(&note.memo), "mac": h(&note.mac),
            "c_out": h(&note.c_out), "token": h(&note.token), "amount": h(&note.amount), "hash": h(&note.hash()), "sent": sent });
        run.save()?;
        let burn = Instruction::Burn { config: config.clone(), note, from: BurnFrom::Holder };
        steps.push(call(
            "burn-public",
            vec![
                sh(owner, bridge),
                sh(holder_account(bridge, &cid, owner.value()), bridge),
                sh(burn_account(bridge, &cid, &note.hash()), bridge),
                sh(burns_account(bridge, &cid), bridge),
            ],
            vec![owner.to_string()],
            borsh::to_vec(&burn)?,
            GAS_BURN,
        ));
    }
    let declared: u64 = steps.iter().filter_map(|s| s["call"]["gas_limit"].as_u64()).sum();
    let b = json!({ "payer": run.get("payer")?, "gas_limit": GAS_IMPORT, "steps": steps });
    std::fs::write(&out, serde_json::to_vec_pretty(&b)?)?;
    println!("batch: {} steps, declared execution gas {declared} (of 10,000,000 per block), written to {}", b["steps"].as_array().map_or(0, Vec::len), out.display());
    Ok(())
}

/// The burn accumulator's layout for the release leg's stand-in attestor (`lez-v03-return/INTERFACE.md` of branch
/// testnet-return, §1): `burns(cid)`'s bridge shard is borsh `BridgeState::Burns(BurnTree)`: tag 5 at 0, `config_id`
/// at 1 (32), `count` at 33 (u64 little-endian), the frontier's length at 41 (u32, 32), the canonical frontier at 45
/// (32 x 32 bytes, f[0] first); 1,069 bytes; no root (the frontier gives it).
fn layout(run: &Run) -> Result<()> {
    let bridge = run.acc("bridge")?;
    let cid = run.config_id()?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "program": bridge.to_string(),
            "account": burns_account(bridge, &cid).to_string(),
            "count_endian": "little",
            "burn_count": { "offset": 33, "length": 8 },
            "burn_frontier": { "offset": 45, "length": 1024 },
            "config_id": { "offset": 1, "length": 32 },
        }))?
    );
    Ok(())
}

/// `burns.json` (the same interface, §2) from this run's notes, in the order they were sent: the order the
/// accumulator holds them if they landed in that order (the comparison with the node's root decides).
fn burns_file(run: &Run) -> Result<()> {
    let cid = run.config_id()?;
    let mut notes: Vec<(&String, &Value)> = run.rec["notes"].as_object().map(|m| m.iter().collect()).unwrap_or_default();
    notes.sort_by_key(|(_, n)| n["sent"].as_u64().unwrap_or(0));
    let mut tree = bc::BurnTree::new(cid);
    let mut out = Vec::new();
    for (i, (step, n)) in notes.iter().enumerate() {
        let s = |k: &str| n[k].as_str().unwrap_or_default().to_owned();
        let note = Note {
            r: unhex(&s("r"))?,
            memo: unhex(&s("memo"))?,
            mac: unhex(&s("mac"))?,
            token: unhex(&s("token"))?,
            amount: unhex(&s("amount"))?,
            c_out: unhex(&s("c_out"))?,
        };
        tree.append(&note).ok_or_else(|| anyhow!("tree"))?;
        out.push(json!({ "burn_index": i, "token": format!("0x{}", s("token")), "amount": format!("0x{}", s("amount")),
            "c_out": format!("0x{}", s("c_out")), "r": format!("0x{}", s("r")), "leaf": format!("0x{}", h(&note.leaf(&cid, i as u64))),
            "block": run.rec["steps"][step.as_str()]["block"], "origin": step }));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({ "config_id": format!("0x{}", h(&cid)), "burn_count": out.len(),
            "burn_root": format!("0x{}", h(&tree.root().unwrap_or_default())), "burns": out }))?
    );
    Ok(())
}
