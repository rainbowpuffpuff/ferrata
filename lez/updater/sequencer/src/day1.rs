//! `ferrata-day1`: the transactions of the day-one deployment on a LEZ v0.4 network, signed once, kept, and sent
//! again byte for byte (`lez/testnet04/day1.sh` drives it; `lez/testnet04/README.md`).
//!
//! Every transaction is built here as the updater's `SequencerLedger` builds its own (`ledger.rs`: actors, admission
//! evidence, explicit nonces, a fee declaration with a declared gas limit and maximum fee, the signers' signatures) and
//! written to `STATE/tx/<stage>/NN.tx` (the borsh `LeeTransaction`) with `NN.json` (its hash, signers and nonces,
//! limits) **before** it is sent. A stage is signed once; `send` sends the stored bytes again until each is in a block,
//! so a restart never builds a second transaction that could be applied beside the first: a stored transaction is
//! re-signed only with the same nonces (both can never apply), never with new ones.
//!
//! **The store** is where the signed stages and the instance's identity are kept: the keys' directory (`--keys`), or
//! `--store` (the plan's scratch). It lasts as long as the keys: a run whose `--state` (the follower, fetched data)
//! is lost or named otherwise (another spelling of the URL) still finds every stage it signed and sends those bytes
//! again; `Init` is signed only from the checkpoint first kept there (`STORE/init/`), so a second signing is for the
//! same instance, and not at all when that instance exists on chain.
//!
//! Commands (`--rpc URL` everywhere; read-only unless named `send` or `fund`):
//!
//!   keygen   --keys DIR --bin PROGRAM.bin [--bridge-bin BRIDGE.bin]      fresh keys (mode 600), `accounts.json`
//!   keygen   --keys DIR --program ACCOUNT                                  the same for an instance on a program deployed
//!                                                                        already (its account only: no program keys)
//!   sign     --keys DIR --state DIR [--store DIR] --stage deploy-program|deploy-bridge|init|attest [--bin F] [--fetch DIR]
//!            --stage bridge-init --identity F --bridge-config F --fetch DIR   the bridge program's `Init` (pairing: the
//!            vault's slot 2 holds the configId) and ETH's `Register` (registry entry, schedules 0 and 1, leaf count)
//!            under the instance's newest record, from `eth_getProof` there (deploy/testnet: ferrata-bridge-lez-init)
//!            --stage bridge-import --identity F --bridge-config F --fetch DIR   the bridge's `ImportRoot`: the vault's
//!            deposit root and leaf count n (slots 0 and 1) under the newest record, written to root(cid, n)
//!            (deploy/testnet: ferrata-bridge-lez-import)
//!            --stage bridge-fees ...   ETH's `ImportFees` with no new schedule: known_until up to the vault's count
//!            (a claim's leaf must lie below it; registration set it to the count then)
//!   send     --state DIR --stage S                                       send the stage's stored bytes, settle each
//!   status   --state DIR --stage S                                       each stored transaction: applied / pending / dead
//!   target   --keys DIR --state DIR                                      the instance's newest record: slot, block hash
//!   inspect  --keys DIR --state DIR [--bin F] [--bridge-bin F]           balances, nonces, program, instance (JSON)
//!   verify   --keys DIR --state DIR --bin F [--bridge-bin F]            re-read every verdict; exit 0 only if all match
//!   fund     --keys DIR --amount N                                       rehearsal only: LEZ's published local test
//!                                                                        account pays the payer and the daemon key
//!   transfer --from-key F --to ACCOUNT [--to-pk HEX] --amount N --state DIR [--wait S] [--record FILE]
//!                                                                        a native-token transfer from the key's
//!                                                                        public account (the testnet's faucet,
//!                                                                        deploy/testnet); one JSON line with its
//!                                                                        receipt (state applied | reverted |
//!                                                                        pending), no key; --record: its hash, bytes
//!                                                                        and nonce written to FILE before it is sent
//!   transfer-status --record FILE --state DIR                            applied | reverted | dropped | pending
//!   transfer-resend --record FILE --state DIR                            the same bytes again (same nonce)
//!
//! Reading a sequencer that is not on this machine needs `FERRATA_SEQUENCER_REMOTE=<that exact URL>`; sending to it
//! needs `FERRATA_TESTNET_SEND=yes` as well (`live.rs`, `check_loopback` and `check_send`, checked before every send).

use std::{
    collections::BTreeMap,
    fs,
    io::{Read as _, Write as _},
    os::unix::fs::OpenOptionsExt as _,
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::{anyhow, bail, ensure, Context as _, Result};
use common::transaction::LeeTransaction;
use ferrata_core::{
    steps,
    types::{BeaconBlockHeader, LightClientBootstrap},
    witness::{CompactBootstrap, CompactStore, ExecutionSummary, HeaderRecord},
    Root,
};
use ferrata_lez_bridge_wire as bw;
use ferrata_lez_wire::{
    attest, header_actor, header_seed, instance_actor, instance_id, instance_seed, job_actor, job_id, Config,
    InstanceState, Message, RecordState,
};
use lee::{
    public_transaction::{Message as PublicMessage, WitnessSet},
    AccountId, Actor, FeeDeclaration, PrivateKey, PublicAccountEvidence, PublicKey, PublicTransaction,
};
use lee_core::{account::Nonce, program::PROGRAM_LOADER_ACCOUNT_ID};
use program_loader_core::Message as LoaderMessage;
use serde_json::{json, Value};

use crate::{
    live::{Live, Options},
    programs::{image_id_hex, sha256_hex, GuestBinary},
};

/// Declared execution gas of a loader transaction (the wallet's default; the live driver deployed the final programs
/// with it on rc1, rc2 and rc3).
const LOADER_GAS: u64 = 2_000_000;
/// Declared execution gas of `Init` (the updater's default for every step, `ferrata-updater.example.toml`).
const INIT_GAS: u64 = 4_800_000;
/// Declared execution gas of the attestation (a slot attestation measured 276,607: `lez/ATTEST-CLI.md`).
const ATTEST_GAS: u64 = 2_000_000;
/// The base fee each maximum fee is sized against, at least: the wallet's own assumption (8 times the genesis minimum,
/// `lez/wallet/src/lib.rs` `ASSUMED_BASE_FEE`), or 8 times the fee state's when that is higher.
const ASSUMED_BASE_FEE: u64 = 64;
/// The chain the instance follows: Sepolia, the Gloas layout (as the updater's example and the crown run's instance).
pub const PRESET: &str = "sepolia";
pub const LAYOUT: &str = "gloas";
pub const ADOPTION_PARTS: u8 = 2;
const SEPOLIA_CHAIN_ID: u64 = 11_155_111;
/// The bridge's `Init` and `Register`: an attestation of the vault's slots chained from the bridge program.
const BRIDGE_GAS: u64 = 3_000_000;
/// ETH's registry entry and fee schedule 0 as the vault's DeployVault registers them (lez/crown/driver's `eth_entry`,
/// `eth_schedule_zero`): checked against the vault's own registry and fee slots before anything is signed.
const ETH: [u8; 20] = [0; 20];
const ETH_DENOMINATIONS: [u128; 3] = [1_000_000_000_000_000, 10_000_000_000_000_000, 100_000_000_000_000_000];
const ETH_RELAY_FEES: [u128; 3] = [2_000_000_000_000, 20_000_000_000_000, 200_000_000_000_000];
const ETH_CHECKPOINT_FEES: [u128; 3] = [1_000_000_000_000, 10_000_000_000_000, 100_000_000_000_000];
const ETH_LEZ_UNIT: u128 = 1_000_000_000_000;
const ETH_UNIT_WEIGHT: u128 = 3000;

// ---- arguments ---------------------------------------------------------------------------------------------------

struct Args {
    cmd: String,
    opts: BTreeMap<String, String>,
}

impl Args {
    fn parse() -> Result<Self> {
        let mut it = std::env::args().skip(1);
        let cmd = it.next().ok_or_else(|| anyhow!("usage: ferrata-day1 COMMAND [--name value ...] (see src/day1.rs)"))?;
        let mut opts = BTreeMap::new();
        while let Some(a) = it.next() {
            let k = a.strip_prefix("--").ok_or_else(|| anyhow!("unexpected argument {a}"))?;
            let v = it.next().ok_or_else(|| anyhow!("--{k} needs a value"))?;
            opts.insert(k.to_string(), v);
        }
        Ok(Self { cmd, opts })
    }
    fn get(&self, k: &str) -> Option<&str> {
        self.opts.get(k).map(String::as_str)
    }
    fn need(&self, k: &str) -> Result<&str> {
        self.get(k).ok_or_else(|| anyhow!("--{k} is required for {}", self.cmd))
    }
    fn path(&self, k: &str) -> Result<PathBuf> {
        self.need(k).map(PathBuf::from)
    }
}

// ---- keys ----------------------------------------------------------------------------------------------------------

fn fresh_key() -> Result<PrivateKey> {
    let mut f = fs::File::open("/dev/urandom").context("/dev/urandom")?;
    loop {
        let mut b = [0u8; 32];
        f.read_exact(&mut b)?;
        if let Ok(k) = PrivateKey::try_new(b) {
            return Ok(k);
        }
    }
}

fn write_key(path: &Path, key: &PrivateKey) -> Result<()> {
    if let Some(d) = path.parent() {
        fs::create_dir_all(d)?;
    }
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("{}: exists already (keys are never overwritten)", path.display()))?;
    writeln!(f, "{}", hex::encode(key.value()))?;
    Ok(())
}

fn read_key(path: &Path) -> Result<PrivateKey> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let bytes: [u8; 32] = hex::decode(text.trim())?.try_into().map_err(|_| anyhow!("{}: not 32 bytes", path.display()))?;
    PrivateKey::try_new(bytes).map_err(|e| anyhow!("{}: not a key: {e}", path.display()))
}

fn id_of(key: &PrivateKey) -> AccountId {
    AccountId::from(&PublicKey::new_from_private_key(key))
}

/// The keys of one deployment: the header's and one per segment. Or, for a program another deployment made (a second
/// instance on a network's published program), its account alone: `DIR/account` (base58), no keys, nothing to deploy.
struct Deployment {
    header: Option<PrivateKey>,
    account: AccountId,
    segments: Vec<PrivateKey>,
}

impl Deployment {
    fn load(dir: &Path) -> Result<Self> {
        if !dir.join("header.key").exists() && dir.join("account").exists() {
            let text = fs::read_to_string(dir.join("account"))?;
            let account = text.trim().parse().map_err(|e| anyhow!("{}: not an account: {e:?}", dir.join("account").display()))?;
            return Ok(Self { header: None, account, segments: Vec::new() });
        }
        let header = read_key(&dir.join("header.key"))?;
        let mut segments = Vec::new();
        while dir.join(format!("segment-{:02}.key", segments.len())).exists() {
            segments.push(read_key(&dir.join(format!("segment-{:02}.key", segments.len())))?);
        }
        Ok(Self { account: id_of(&header), header: Some(header), segments })
    }
    /// A deployment of these keys, or the program another deployment made (`DIR/account`).
    fn known(dir: &Path) -> bool {
        dir.join("header.key").exists() || dir.join("account").exists()
    }
    fn program(&self) -> AccountId {
        self.account
    }
    fn header_key(&self) -> Result<&PrivateKey> {
        self.header.as_ref().ok_or_else(|| anyhow!("{}: deployed by another deployment (its account only): nothing to sign for it", self.account))
    }
    fn json(&self) -> Value {
        json!({"header": self.program().to_string(), "keys": self.header.is_some(),
               "segments": self.segments.iter().map(|k| id_of(k).to_string()).collect::<Vec<_>>()})
    }
}

fn keygen(a: &Args) -> Result<()> {
    let dir = a.path("keys")?;
    ensure!(!dir.join("payer.key").exists(), "{}: keys exist already; a new deployment needs a new directory", dir.display());
    // `--program ACCOUNT`: an instance on a program already deployed (a network's published one): its account, no keys.
    // Checked before anything is written, so that a refused call leaves no directory behind.
    let program: Option<AccountId> = match a.get("program") {
        Some(account) => {
            ensure!(a.get("bin").is_none(), "--program names a program deployed already: no --bin");
            Some(account.trim().parse().map_err(|e| anyhow!("--program {account:?}: {e:?}"))?)
        }
        None => None,
    };
    fs::create_dir_all(&dir)?;
    fs::set_permissions(&dir, std::os::unix::fs::PermissionsExt::from_mode(0o700))?;
    write_key(&dir.join("payer.key"), &fresh_key()?)?;
    write_key(&dir.join("daemon.key"), &fresh_key()?)?;
    if let Some(id) = program {
        fs::create_dir_all(dir.join("program"))?;
        fs::write(dir.join("program").join("account"), format!("{id}\n"))?;
    }
    for (name, opt) in [("program", "bin"), ("bridge", "bridge-bin")] {
        let Some(bin) = a.get(opt) else { continue };
        let binary = GuestBinary::load(name, Path::new(bin))?;
        write_key(&dir.join(name).join("header.key"), &fresh_key()?)?;
        for i in 0..binary.chunks().len() {
            write_key(&dir.join(name).join(format!("segment-{i:02}.key")), &fresh_key()?)?;
        }
    }
    let accounts = accounts_json(&dir)?;
    fs::write(dir.join("accounts.json"), serde_json::to_string_pretty(&accounts)? + "\n")?;
    println!("{}", serde_json::to_string_pretty(&accounts)?);
    Ok(())
}

fn accounts_json(dir: &Path) -> Result<Value> {
    let mut v = json!({
        "payer": id_of(&read_key(&dir.join("payer.key"))?).to_string(),
        "payer_public_key": hex::encode(PublicKey::new_from_private_key(&read_key(&dir.join("payer.key"))?).value()),
        "daemon": id_of(&read_key(&dir.join("daemon.key"))?).to_string(),
        "daemon_public_key": hex::encode(PublicKey::new_from_private_key(&read_key(&dir.join("daemon.key"))?).value()),
    });
    for name in ["program", "bridge"] {
        if Deployment::known(&dir.join(name)) {
            v[name] = Deployment::load(&dir.join(name))?.json();
        }
    }
    Ok(v)
}

// ---- the stored transactions ---------------------------------------------------------------------------------------

/// What `NN.json` holds beside `NN.tx`.
fn tx_meta(label: &str, tx: &LeeTransaction, signers: &[(AccountId, u128)], gas_limit: u64, max_fee: u128) -> Value {
    json!({
        "label": label,
        "hash": tx.hash().to_string(),
        "bytes": borsh::to_vec(tx).map(|b| b.len()).unwrap_or(0),
        "signers": signers.iter().map(|(a, n)| json!({"account": a.to_string(), "nonce": n.to_string()})).collect::<Vec<_>>(),
        "gas_limit": gas_limit,
        "max_fee": max_fee.to_string(),
        "signed_utc": crate::utc(),
    })
}

/// The store (module comment): `--store`, or the keys' directory.
fn store_of(a: &Args) -> Result<PathBuf> {
    match a.get("store") {
        Some(s) => Ok(PathBuf::from(s)),
        None => a.path("keys").map_err(|_| anyhow!("--keys (or --store) is required for {}: the signed stages are kept beside the keys", a.cmd)),
    }
}

fn stage_dir(state: &Path, stage: &str) -> PathBuf {
    state.join("tx").join(stage)
}

/// The stage's stored transactions, in order: (index, bytes, meta).
fn stored(state: &Path, stage: &str) -> Result<Vec<(usize, LeeTransaction, Value)>> {
    let dir = stage_dir(state, stage);
    let mut out = Vec::new();
    for i in 0.. {
        let tx_path = dir.join(format!("{i:02}.tx"));
        if !tx_path.exists() {
            break;
        }
        let tx: LeeTransaction = borsh::from_slice(&fs::read(&tx_path)?)?;
        let meta: Value = serde_json::from_str(&fs::read_to_string(dir.join(format!("{i:02}.json")))?)?;
        ensure!(meta["hash"].as_str() == Some(tx.hash().to_string().as_str()), "{}: hash mismatch", tx_path.display());
        out.push((i, tx, meta));
    }
    Ok(out)
}

/// Write the stage's transactions, all or nothing (a `.part` directory renamed at the end).
fn store_stage(state: &Path, stage: &str, txs: &[(LeeTransaction, Value)]) -> Result<()> {
    let dir = stage_dir(state, stage);
    ensure!(!dir.exists(), "{}: the stage is signed already (its bytes are sent again by `send`)", dir.display());
    let part = dir.with_extension("part");
    let _ = fs::remove_dir_all(&part);
    fs::create_dir_all(&part)?;
    for (i, (tx, meta)) in txs.iter().enumerate() {
        fs::write(part.join(format!("{i:02}.tx")), borsh::to_vec(tx)?)?;
        fs::write(part.join(format!("{i:02}.json")), serde_json::to_string_pretty(meta)? + "\n")?;
    }
    fs::rename(&part, &dir)?;
    Ok(())
}

fn public_tx(
    root: Actor,
    message: Vec<u8>,
    actors: Vec<Actor>,
    evidence: Vec<PublicAccountEvidence>,
    signers: &[(&PrivateKey, u128)],
    payer: &PrivateKey,
    gas_limit: u64,
    max_fee: u128,
) -> (LeeTransaction, Vec<(AccountId, u128)>) {
    let nonces: BTreeMap<AccountId, Nonce> = signers.iter().map(|(k, n)| (id_of(k), Nonce(*n))).collect();
    let msg = PublicMessage::new(root, message, actors, nonces, Some(FeeDeclaration::new(id_of(payer), gas_limit, 0, max_fee)), evidence);
    let keys: Vec<&PrivateKey> = signers.iter().map(|(k, _)| *k).collect();
    let ws = WitnessSet::for_message(&msg, &keys);
    let tx = LeeTransaction::Public(PublicTransaction::new(msg, ws));
    (tx, signers.iter().map(|(k, n)| (id_of(k), *n)).collect())
}

/// The maximum fee of a transaction: its declared gas and its bytes at the larger of the wallet's assumed base fee
/// and 8 times the fee state's.
fn max_fee(gas_limit: u64, bytes: usize, quote: (u64, u64)) -> u128 {
    let exec = ASSUMED_BASE_FEE.max(quote.0.saturating_mul(8));
    let stor = ASSUMED_BASE_FEE.max(quote.1.saturating_mul(8));
    u128::from(gas_limit) * u128::from(exec) + (bytes as u128 + 2_048) * u128::from(stor)
}

// ---- the connection ------------------------------------------------------------------------------------------------

async fn connect(a: &Args, state: &Path) -> Result<Live> {
    let run_dir = state.join("ledger");
    let opts = Options {
        rpc: a.need("rpc")?.to_string(),
        out_dir: run_dir.clone(),
        run_dir,
        harness_results: PathBuf::from("/nonexistent"),
        sequencer_log: a.get("sequencer-log").map(PathBuf::from),
        gas_limit: INIT_GAS,
        wait_final: a.get("wait-final") != Some("no"),
    };
    let mut live = Live::open(opts, &[]).await?;
    live.phase = "day1".into();
    Ok(live)
}

async fn nonce_of(live: &Live, account: AccountId) -> Result<u128> {
    Ok(live.wallet.get_accounts_nonces(&[account]).await?.first().ok_or_else(|| anyhow!("no nonce served"))?.0)
}

async fn quote(live: &Live) -> Result<(u64, u64)> {
    use wallet::WalletSequencerClient as _;
    let q = live.wallet.client().get_fee_state().await?;
    Ok((q.base_fee_exec, q.base_fee_stor))
}

// ---- stage: deploy --------------------------------------------------------------------------------------------------

/// `WriteSegment` for every chunk of the user ELF, last first (each links to the next, which must exist), then
/// `CreateHeader` (immutable); the payer co-signs each with consecutive nonces, each segment's own fresh key signs its
/// write (LEZ v0.4's loader: the target must be authorized, `program_loader/core/src/lib.rs`).
fn sign_deploy(payer: &PrivateKey, payer_nonce: u128, d: &Deployment, binary: &GuestBinary, quote: (u64, u64)) -> Result<Vec<(LeeTransaction, Value)>> {
    let chunks = binary.chunks();
    ensure!(chunks.len() == d.segments.len(), "{}: {} chunks, {} segment keys", binary.name, chunks.len(), d.segments.len());
    let seg_ids: Vec<AccountId> = d.segments.iter().map(id_of).collect();
    let mut out = Vec::new();
    let mut n = payer_nonce;
    for i in (0..chunks.len()).rev() {
        let msg = LoaderMessage::WriteSegment { bytecode: chunks[i].to_vec(), next_segment: seg_ids.get(i + 1).copied() };
        let actor = Actor::new(seg_ids[i], PROGRAM_LOADER_ACCOUNT_ID);
        let bytes_guess = chunks[i].len() + 512;
        let fee = max_fee(LOADER_GAS, bytes_guess, quote);
        let (tx, signers) = public_tx(actor, borsh::to_vec(&msg)?, vec![actor], vec![], &[(&d.segments[i], 0), (payer, n)], payer, LOADER_GAS, fee);
        let label = format!("deploy {}: WriteSegment {} of {} ({} bytes)", binary.name, i + 1, chunks.len(), chunks[i].len());
        out.push((tx.clone(), tx_meta(&label, &tx, &signers, LOADER_GAS, fee)));
        n += 1;
    }
    let msg = LoaderMessage::CreateHeader { first_segment: seg_ids[0], immutable: true };
    let actor = Actor::new(d.program(), PROGRAM_LOADER_ACCOUNT_ID);
    let fee = max_fee(LOADER_GAS, 1_024, quote);
    let (tx, signers) = public_tx(actor, borsh::to_vec(&msg)?, vec![actor], vec![], &[(d.header_key()?, 0), (payer, n)], payer, LOADER_GAS, fee);
    let label = format!("deploy {}: CreateHeader (immutable) at {}", binary.name, d.program());
    out.push((tx.clone(), tx_meta(&label, &tx, &signers, LOADER_GAS, fee)));
    Ok(out)
}

// ---- stage: init ----------------------------------------------------------------------------------------------------

/// The instance's configuration and checkpoint, from `FETCH/chain.json` (written by `lez-v03/scripts/fetch.py
/// checkpoint`: Lodestar's bootstrap, its root checked against publicnode's header at the same slot).
fn instance_of(fetch: &Path) -> Result<(Config, Root, Vec<u8>, u64)> {
    let chain: Value = serde_json::from_str(&fs::read_to_string(fetch.join("chain.json"))?)?;
    ensure!(chain["layout"].as_str() == Some(LAYOUT), "chain.json: layout {} (expected {LAYOUT})", chain["layout"]);
    let checkpoint = ferrata_updater::chain::root(chain["checkpoint"].as_str().ok_or_else(|| anyhow!("chain.json: no checkpoint"))?)?;
    let slot = chain["bootstrap_slot"].as_u64().ok_or_else(|| anyhow!("chain.json: no bootstrap_slot"))?;
    let config = ferrata_updater::chain::instance_config(PRESET, ADOPTION_PARTS, Some(LAYOUT))?;
    let bootstrap = fs::read(fetch.join("bootstrap.ssz"))?;
    Ok((config, checkpoint, bootstrap, slot))
}

fn sign_init(payer: &PrivateKey, n: u128, program: AccountId, store: &Path, fetch: &Path, quote: (u64, u64)) -> Result<(Vec<(LeeTransaction, Value)>, Value)> {
    // The checkpoint is the store's once kept there: a second signing (the store's `tx/init` lost) is for the same
    // instance, never a new one.
    let kept = store.join("init");
    if !kept.join("chain.json").exists() {
        let part = store.join("init.part");
        let _ = fs::remove_dir_all(&part);
        fs::create_dir_all(&part)?;
        fs::copy(fetch.join("chain.json"), part.join("chain.json"))?;
        fs::copy(fetch.join("bootstrap.ssz"), part.join("bootstrap.ssz"))?;
        fs::rename(&part, &kept)?;
    }
    let (config, checkpoint, bytes, slot) = instance_of(&kept)?;
    let fork = config.fork().ok_or_else(|| anyhow!("config: no layout"))?;
    let b = LightClientBootstrap::decode(&bytes, fork, config.sync_committee_size as usize)
        .map_err(|e| anyhow!("bootstrap.ssz does not decode: {e:?}"))?;
    ensure!(b.header.beacon.slot == slot, "bootstrap.ssz is not chain.json's slot");
    let compact = CompactBootstrap::of(&b);
    // The program's own `Init` checks, natively: the bootstrap's header is the checkpoint, its committee branch holds.
    let (store, job) = steps::initialize(&checkpoint, &compact, fork, &config.spec(), config.adoption_parts)
        .map_err(|e| anyhow!("the program's Init checks refuse this bootstrap: {e:?}"))?;
    // The weak-subjectivity bound: every transaction that creates or extends the instance is valid only before the
    // period of its finalized header ends (`Config::ws_end_ms`, by the sequencer's clock). Refuse a checkpoint within a
    // day of that end.
    let ws_end_ms = config.ws_end_ms(store.finalized.slot).ok_or_else(|| anyhow!("no weak-subjectivity end"))?;
    let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis() as u64;
    ensure!(ws_end_ms > now_ms + 86_400_000, "the checkpoint's weak-subjectivity period ends within a day ({ws_end_ms} ms): fetch a fresh one");
    let id = instance_id(&config, &checkpoint);
    let jid = job_id(&id, &job.store_digest, &job.update_digest);
    let root = instance_actor(program, &id);
    let message = borsh::to_vec(&Message::Init { config: config.clone(), checkpoint, bootstrap: compact.encode(fork) })?;
    let bytes_guess = message.len() + 512;
    let fee = max_fee(INIT_GAS, bytes_guess, quote);
    let (tx, signers) = public_tx(
        root,
        message,
        vec![root, job_actor(program, &id, &jid)],
        vec![PublicAccountEvidence::Pda { program, seed: instance_seed(&id) }],
        &[(payer, n)],
        payer,
        INIT_GAS,
        fee,
    );
    let info = json!({
        "instance_id": hex::encode(id), "checkpoint": hex::encode(checkpoint), "checkpoint_slot": slot,
        "preset": PRESET, "layout": LAYOUT, "adoption_parts": ADOPTION_PARTS, "chain_id": config.chain_id,
        "ws_period_epochs": config.ws_period_epochs, "ws_end_ms": ws_end_ms, "program": program.to_string(),
        "init_job_id": hex::encode(jid),
    });
    Ok((vec![(tx.clone(), tx_meta("Init (Sepolia, Gloas layout)", &tx, &signers, INIT_GAS, fee))], info))
}

// ---- stage: attest --------------------------------------------------------------------------------------------------

/// An actor's state as of the newest block final on Bedrock: the follower's, replayed from the endpoint's blocks with
/// LEZ's own block application. (`Live::actor_state` also requires the sequencer's head state to equal it, which fails
/// while transactions are in flight, as when the daemon sends.)
fn final_state(live: &Live, actor: Actor) -> Vec<u8> {
    live.follower.state.get_account_by_id(actor.account_id).data.actor_state(actor.program_account_id).to_vec()
}

async fn instance_state(live: &Live, program: AccountId, id: &Root) -> Result<Option<InstanceState>> {
    let bytes = final_state(live, instance_actor(program, id));
    if bytes.is_empty() {
        return Ok(None);
    }
    Ok(Some(borsh::from_slice(&bytes)?))
}

fn block_hash_of(r: &HeaderRecord) -> Result<Root> {
    match &r.execution {
        ExecutionSummary::BlockHash(h) => Ok(*h),
        ExecutionSummary::Payload { block_hash, .. } => Ok(*block_hash),
        ExecutionSummary::None => bail!("the record has no execution block"),
    }
}

fn header_ssz(h: &BeaconBlockHeader) -> Vec<u8> {
    let mut v = Vec::with_capacity(112);
    v.extend_from_slice(&h.slot.to_le_bytes());
    v.extend_from_slice(&h.proposer_index.to_le_bytes());
    v.extend_from_slice(&h.parent_root);
    v.extend_from_slice(&h.state_root);
    v.extend_from_slice(&h.body_root);
    v
}

/// The instance's newest record: `header(id, finalized slot)`.
async fn newest_record(live: &Live, program: AccountId, id: &Root) -> Result<(u64, HeaderRecord, InstanceState)> {
    let state = instance_state(live, program, id).await?.ok_or_else(|| anyhow!("no instance {} yet", hex::encode(id)))?;
    ensure!(state.active, "the instance is not active yet (its committee adoption is pending)");
    let store = CompactStore::decode(&state.store).map_err(|e| anyhow!("store: {e:?}"))?;
    let slot = store.finalized.slot;
    let bytes = final_state(live, header_actor(program, id, slot));
    ensure!(!bytes.is_empty(), "no record at header({}, {slot})", hex::encode(id));
    let rec: RecordState = borsh::from_slice(&bytes)?;
    let hr = rec.header().map_err(|c| anyhow!("record does not decode ({c})"))?;
    Ok((slot, hr, state))
}

fn unhex(v: &Value, what: &str) -> Result<Vec<u8>> {
    let s = v.as_str().ok_or_else(|| anyhow!("{what}: not a string"))?;
    hex::decode(s.trim_start_matches("0x")).with_context(|| format!("{what}: not hex"))
}

fn word(v: &Value, what: &str) -> Result<[u8; 32]> {
    let s = v.as_str().ok_or_else(|| anyhow!("{what}: not a string"))?.trim_start_matches("0x");
    let s = if s.len() % 2 == 1 { format!("0{s}") } else { s.to_string() };
    let b = hex::decode(&s)?;
    ensure!(b.len() <= 32, "{what}: longer than a word");
    let mut w = [0u8; 32];
    w[32 - b.len()..].copy_from_slice(&b);
    Ok(w)
}

fn nodes(v: &Value, what: &str) -> Result<Vec<Vec<u8>>> {
    v.as_array().ok_or_else(|| anyhow!("{what}: not a list"))?.iter().map(|n| unhex(n, what)).collect()
}

/// The request: the vault's account and its slots, under the record, from `eth_getProof` at the record's block
/// (`FETCH/proofs/<slot>/proof.json`) and the block's RLP header (`header.rlp`, rebuilt and checked by fetch.py).
fn attest_request(id: &Root, dir: &Path) -> Result<(attest::Request, Value)> {
    let proof: Value = serde_json::from_str(&fs::read_to_string(dir.join("proof.json"))?)?;
    let p = if proof.get("result").is_some() { &proof["result"] } else { &proof };
    let address: [u8; 20] = unhex(&p["address"], "address")?.try_into().map_err(|_| anyhow!("address: not 20 bytes"))?;
    let mut slots = Vec::new();
    let mut values = Vec::new();
    for e in p["storageProof"].as_array().ok_or_else(|| anyhow!("no storageProof"))? {
        slots.push(attest::SlotQuery { slot: word(&e["key"], "key")?, proof: nodes(&e["proof"], "storage proof")?, conditions: vec![] });
        values.push(json!({"slot": e["key"], "value": e["value"]}));
    }
    let request = attest::Request {
        instance_id: *id,
        chain_id: SEPOLIA_CHAIN_ID,
        header: Some(fs::read(dir.join("header.rlp"))?),
        hops: vec![],
        accounts: vec![attest::AccountQuery { address, proof: nodes(&p["accountProof"], "account proof")?, conditions: vec![], slots }],
        logs: vec![],
        leaves: vec![],
        relations: vec![],
    };
    Ok((request, json!({"address": p["address"], "slots": values, "balance": p["balance"], "nonce": p["nonce"],
                         "storage_hash": p["storageHash"], "code_hash": p["codeHash"]})))
}

fn attestation_json(at: &attest::Attestation) -> Value {
    let statements: Vec<Value> = at
        .statements
        .iter()
        .map(|s| {
            let fact = match &s.fact {
                attest::Fact::Account { address, nonce, balance, storage_root, code_hash } => json!({"account": format!("0x{}", hex::encode(address)),
                    "nonce": nonce, "balance": format!("0x{}", hex::encode(balance)), "storage_root": format!("0x{}", hex::encode(storage_root)),
                    "code_hash": format!("0x{}", hex::encode(code_hash))}),
                attest::Fact::Storage { address, slot, value } => json!({"storage": format!("0x{}", hex::encode(address)),
                    "slot": format!("0x{}", hex::encode(slot)), "value": format!("0x{}", hex::encode(value))}),
                other => json!({"other": format!("{other:?}")}),
            };
            json!({"id": format!("0x{}", hex::encode(s.id)), "fact": fact})
        })
        .collect();
    json!({"instance_id": hex::encode(at.instance_id), "chain_id": at.chain_id, "slot": at.slot, "block_number": at.block_number,
           "block_hash": format!("0x{}", hex::encode(at.block_hash)), "hops": at.hops, "statements": statements,
           "event_data": hex::encode(at.event_data())})
}

// ---- sending -----------------------------------------------------------------------------------------------------

/// A stored transaction's state on the chain: in a block (applied or reverted), still applicable (every signer's nonce
/// not yet used: its bytes are sent again), or dead (a signer's nonce used by another transaction: these bytes can
/// never apply).
async fn tx_state(live: &Live, tx: &LeeTransaction, meta: &Value) -> Result<(&'static str, Option<u64>)> {
    if let Some((_, block)) = live.wallet.get_transaction(tx.hash()).await? {
        return Ok(("in-block", Some(block)));
    }
    for s in meta["signers"].as_array().into_iter().flatten() {
        let account: AccountId = s["account"].as_str().unwrap_or_default().parse().map_err(|e| anyhow!("{e:?}"))?;
        let nonce: u128 = s["nonce"].as_str().unwrap_or("0").parse()?;
        if nonce_of(live, account).await? > nonce {
            return Ok(("dead", None));
        }
    }
    Ok(("pending", None))
}

async fn send_stage(live: &mut Live, state: &Path, stage: &str) -> Result<bool> {
    use wallet::WalletSequencerClient as _;
    let txs = stored(state, stage)?;
    ensure!(!txs.is_empty(), "stage {stage}: nothing signed (run `sign` first)");
    let mut all_applied = true;
    for (i, tx, meta) in txs {
        let result_path = stage_dir(state, stage).join(format!("{i:02}.result.json"));
        if result_path.exists() {
            let r: Value = serde_json::from_str(&fs::read_to_string(&result_path)?)?;
            println!("[{stage}] {i:02} {}: {} in block {} (recorded)", meta["label"].as_str().unwrap_or(""), r["status"], r["block"]);
            all_applied &= r["status"] == "applied";
            continue;
        }
        let label = meta["label"].as_str().unwrap_or("").to_string();
        let mut attempts = 0;
        loop {
            let (st, block) = tx_state(live, &tx, &meta).await?;
            match st {
                "dead" => bail!("[{stage}] {i:02} {label}: dead (a signer's nonce was used by another transaction; it never applied). Stop: decide by hand (`status`)"),
                "pending" => {
                    crate::live::check_send(&live.opts.rpc)?;
                    attempts += 1;
                    ensure!(attempts <= 6, "[{stage}] {i:02} {label}: sent {attempts} times, never included");
                    let since_head = live.head().await?;
                    match live.wallet.client().send_transaction(tx.clone()).await {
                        Ok(h) => ensure!(h == tx.hash(), "the sequencer returned another hash"),
                        Err(e) => println!("[{stage}] {i:02} {label}: the sequencer answered {e} (attempt {attempts}); waiting"),
                    }
                    let (included, seen) = live.wait_settled(tx.hash(), since_head).await?;
                    if included.is_none() {
                        println!("[{stage}] {i:02} {label}: not included within {} blocks; the same bytes again", crate::live::drop_after_blocks());
                        continue;
                    }
                    if seen.len() > 1 {
                        println!("[{stage}] {i:02} {label}: included again after a rewind (blocks {seen:?})");
                    }
                }
                _ => {
                    // In a block: wait until the follower holds it (final on Bedrock).
                    let head = live.head().await?;
                    live.wait_settled(tx.hash(), head).await?;
                    let _ = block;
                }
            }
            let receipt = live.follower.receipt(&tx.hash()).cloned().ok_or_else(|| anyhow!("no receipt after settling"))?;
            let r = json!({"hash": tx.hash().to_string(), "block": receipt.block_id, "status": receipt.status(),
                           "exit_code": receipt.exit_code, "reason": receipt.reason, "gas_exec": receipt.gas_exec,
                           "gas_stor": receipt.gas_stor, "fee_base": receipt.fee_base, "events": receipt.events,
                           "tx_bytes": receipt.tx_bytes, "settled_utc": crate::utc()});
            fs::write(&result_path, serde_json::to_string_pretty(&r)? + "\n")?;
            println!("[{stage}] {i:02} {label}: {} in block {} (exec gas {}, storage gas {}, fee {})", r["status"], r["block"], r["gas_exec"], r["gas_stor"], r["fee_base"]);
            all_applied &= receipt.status() == "applied";
            live.save()?;
            break;
        }
    }
    Ok(all_applied)
}

/// A segment's loader actor state: tagged from LEZ v0.4.0-rc2 on (#943), untagged on rc1 (as `programs.rs`).
#[cfg(feature = "lez-rc2")]
fn segment_of(s: &[u8]) -> Option<program_loader_core::ProgramSegment> {
    program_loader_core::ProgramSegment::from_loader_actor_state(s)
}
#[cfg(not(feature = "lez-rc2"))]
fn segment_of(s: &[u8]) -> Option<program_loader_core::ProgramSegment> {
    program_loader_core::ProgramSegment::from_bytes(s)
}

// ---- commands --------------------------------------------------------------------------------------------------

// ---- the bridge's configuration on this zone (deploy/testnet: the Sepolia bridge demo) --------------------------------

fn eth_entry() -> bw::TokenEntry {
    bw::TokenEntry {
        token: ETH,
        lez_definition: [0; 32],
        decimals: 18,
        lez_unit: ETH_LEZ_UNIT,
        unit_weight: ETH_UNIT_WEIGHT,
        denominations: ETH_DENOMINATIONS.to_vec(),
        relay_fees: ETH_RELAY_FEES.to_vec(),
    }
}

fn eth_schedule_zero(t: u64) -> bw::FeeSchedule {
    bw::FeeSchedule {
        index: 0,
        from_leaf: 0,
        announced_at: t,
        effective_at: t,
        mint_fees: vec![0; 3],
        burn_fees: vec![0; 3],
        checkpoint_fees: ETH_CHECKPOINT_FEES.to_vec(),
    }
}

fn cap_params(v: &Value, what: &str) -> Result<bw::CapParams> {
    let n = |k: &str| -> Result<u128> {
        v[k].as_str().ok_or_else(|| anyhow!("{what}.{k}: not a string"))?.parse().with_context(|| format!("{what}.{k}"))
    };
    Ok(bw::CapParams { capacity: n("capacity")?, refill_per_second: n("refillPerSecond")? })
}

/// The configuration `crown-driver config-id` printed for this zone's identity (deploy/testnet: ferrata-bridge-identity,
/// ferrata-bridge-configure), rebuilt and held to its configId: the bridge's own encoding, so what `Init` writes is
/// what the vault pins.
fn bridge_config(identity: &Value, cfg: &Value) -> Result<(bw::BridgeConfig, AccountId, Root)> {
    let vault: [u8; 20] = unhex(&cfg["vault"], "vault")?.try_into().map_err(|_| anyhow!("vault: not 20 bytes"))?;
    let c = &cfg["controls"];
    let controls = bw::Controls {
        admin: word(&c["admin"], "controls.admin")?,
        freeze_authority: word(&c["freezeAuthority"], "controls.freezeAuthority")?,
        fee_recipient: word(&c["feeRecipient"], "controls.feeRecipient")?,
        floor: cap_params(&c["floor"], "controls.floor")?,
        token_cap: cap_params(&c["tokenCap"], "controls.tokenCap")?,
        global_cap: cap_params(&c["globalCap"], "controls.globalCap")?,
    };
    controls.check().map_err(|code| anyhow!("the controls would be refused by Init (code {code})"))?;
    let bridge = AccountId::new(word(&identity["bridge"]["header"], "bridge.header")?);
    let config = bw::BridgeConfig {
        bedrock_genesis_id: word(&identity["bedrockGenesisId"], "bedrockGenesisId")?,
        channel_id: word(&identity["channel"]["id"], "channel.id")?,
        bridge_image_id: word(&identity["bridge"]["imageId"], "bridge.imageId")?,
        attestation: AccountId::new(word(&identity["attestation"]["account"], "attestation.account")?),
        instance_id: word(&identity["instance"]["id"], "instance.id")?,
        evm_chain_id: SEPOLIA_CHAIN_ID,
        vault,
        controls,
    };
    let cid = config.id(bridge);
    ensure!(cid == word(&cfg["configId"], "configId")?, "the rebuilt configuration's id 0x{} is not the configId {}", hex::encode(cid), cfg["configId"]);
    Ok((config, bridge, cid))
}

async fn cmd_sign(a: &Args) -> Result<()> {
    let keys = a.path("keys")?;
    let state = a.path("state")?;
    let store = store_of(a)?;
    let stage = a.need("stage")?;
    let payer = read_key(&keys.join("payer.key"))?;
    let live = connect(a, &state).await?;
    let q = quote(&live).await?;
    let n = nonce_of(&live, id_of(&payer)).await?;
    let (txs, info) = match stage {
        "deploy-program" | "deploy-bridge" => {
            let name = if stage == "deploy-program" { "program" } else { "bridge" };
            let d = Deployment::load(&keys.join(name))?;
            let binary = GuestBinary::load(name, &a.path("bin")?)?;
            (sign_deploy(&payer, n, &d, &binary, q)?, json!({"program": d.program().to_string(), "image_id": binary.image_hex(), "sha256": binary.sha256()}))
        }
        "init" => {
            let d = Deployment::load(&keys.join("program"))?;
            let (txs, info) = sign_init(&payer, n, d.program(), &store, &a.path("fetch")?, q)?;
            // Read the chain before signing: an instance with this id exists already (an earlier run's `Init`, or
            // anyone's for the same configuration and checkpoint): nothing to sign.
            let id = ferrata_updater::chain::root(info["instance_id"].as_str().unwrap_or_default())?;
            if instance_state(&live, d.program(), &id).await?.is_some() {
                fs::write(store.join("instance.json"), serde_json::to_string_pretty(&info)? + "\n")?;
                bail!("the instance {} exists on chain already: no Init signed (STORE/instance.json written)", hex::encode(id));
            }
            (txs, info)
        }
        "attest" => {
            let d = Deployment::load(&keys.join("program"))?;
            let inst: Value = serde_json::from_str(&fs::read_to_string(store.join("instance.json"))?)?;
            let id: Root = ferrata_updater::chain::root(inst["instance_id"].as_str().unwrap_or_default())?;
            let (slot, hr, _) = newest_record(&live, d.program(), &id).await?;
            let dir = a.path("fetch")?.join("proofs").join(slot.to_string());
            ensure!(dir.join("proof.json").exists(), "no proofs for slot {slot} in {} (fetch them first)", dir.display());
            let (request, sources) = attest_request(&id, &dir)?;
            // The verdict, natively: the function the program runs (ferrata_lez_wire::attest::evaluate) on the record
            // the zone holds.
            let verdict = attest::evaluate(&id, SEPOLIA_CHAIN_ID, &hr, &request).map_err(|c| anyhow!("the program would refuse with code {c}"))?;
            let root = header_actor(d.program(), &id, slot);
            let message = borsh::to_vec(&Message::Attest { request })?;
            let fee = max_fee(ATTEST_GAS, message.len() + 512, q);
            let (tx, signers) = public_tx(root, message, vec![root], vec![PublicAccountEvidence::Pda { program: d.program(), seed: header_seed(&id, slot) }], &[(&payer, n)], &payer, ATTEST_GAS, fee);
            let info = json!({"record_slot": slot, "block_hash": format!("0x{}", hex::encode(block_hash_of(&hr)?)), "verdict": attestation_json(&verdict), "eth_getProof": sources});
            (vec![(tx.clone(), tx_meta(&format!("Attest: the vault's slots 0 and 1 under header({}, {slot})", &hex::encode(id)[..8]), &tx, &signers, ATTEST_GAS, fee))], info)
        }
        "bridge-init" => {
            let identity: Value = serde_json::from_str(&fs::read_to_string(a.path("identity")?)?)?;
            let cfg: Value = serde_json::from_str(&fs::read_to_string(a.path("bridge-config")?)?)?;
            let (config, bridge, cid) = bridge_config(&identity, &cfg)?;
            let (attest_program, id) = (config.attestation, config.instance_id);
            let config_actor = bw::config_actor(bridge, &cid);
            let token = bw::token_actor(bridge, &cid, &ETH);
            // Init and Register ETH, or Register alone when an Init applied and its Register did not (the pair is two
            // transactions: the second can be refused or lost on its own).
            let paired = !final_state(&live, config_actor).is_empty();
            ensure!(
                !(paired && !final_state(&live, token).is_empty()),
                "config(0x{}) and token(cid, ETH) exist on the zone already: nothing to sign",
                hex::encode(cid)
            );
            let (slot, hr, _) = newest_record(&live, attest_program, &id).await?;
            let dir = a.path("fetch")?.join("proofs").join(slot.to_string());
            ensure!(dir.join("proof.json").exists(), "no proofs for slot {slot} in {} (fetch them first)", dir.display());
            let (request, sources) = attest_request(&id, &dir)?;
            let acct = request.accounts.first().ok_or_else(|| anyhow!("no account in the proofs"))?;
            ensure!(acct.address == config.vault, "the proofs are of 0x{}, not the vault 0x{}", hex::encode(acct.address), hex::encode(config.vault));
            // The attestation program's own check, natively, on the record the zone holds: the header and every proof.
            attest::evaluate(&id, SEPOLIA_CHAIN_ID, &hr, &request).map_err(|c| anyhow!("the attestation program would refuse these proofs (code {c})"))?;
            let value = |key: &bw::Word| -> Result<[u8; 32]> {
                let e = sources["slots"].as_array().into_iter().flatten()
                    .find(|e| word(&e["slot"], "slot").map(|w| w == *key).unwrap_or(false))
                    .ok_or_else(|| anyhow!("no proof of slot 0x{}", hex::encode(key)))?;
                word(&e["value"], "value")
            };
            let proof_of = |key: &bw::Word| -> Result<Vec<Vec<u8>>> {
                acct.slots.iter().find(|q| q.slot == *key).map(|q| q.proof.clone()).ok_or_else(|| anyhow!("no proof of slot 0x{}", hex::encode(key)))
            };
            ensure!(value(&bw::SLOT_CONFIG_ID)? == cid, "the vault's slot 2 is 0x{}, not the configId 0x{}", hex::encode(value(&bw::SLOT_CONFIG_ID)?), hex::encode(cid));
            let entry = eth_entry();
            ensure!(value(&bw::registry_slot(&ETH))? == entry.hash(), "the vault's ETH registry entry is not the one this registers (other denominations or fees?)");
            let fee0 = value(&bw::fee_slot(&ETH, 0))?;
            ensure!(fee0 != [0; 32], "the vault's ETH fee schedule 0 is empty: ETH is not registered yet");
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs();
            let t = (now.saturating_sub(30 * 24 * 3600)..=now).rev().find(|t| eth_schedule_zero(*t).hash(&ETH) == fee0)
                .ok_or_else(|| anyhow!("no time in the last 30 days gives the vault's ETH schedule 0 (other fees?)"))?;
            let account = bw::with_header(fs::read(dir.join("header.rlp"))?, acct.proof.clone());
            let init = bw::BridgeMessage::Init { config: config.clone(), record_slot: slot, proof: bw::PairingProof { account: account.clone(), config_slot: proof_of(&bw::SLOT_CONFIG_ID)? } };
            let register = bw::BridgeMessage::Register {
                config_id: cid,
                entry,
                schedule: eth_schedule_zero(t),
                record_slot: slot,
                proof: bw::VaultProof { account, slots: bw::register_slots(&ETH).iter().map(&proof_of).collect::<Result<_>>()? },
            };
            let header = header_actor(attest_program, &id, slot);
            let ev_config = PublicAccountEvidence::Pda { program: bridge, seed: bw::config_seed(&cid) };
            let ev_header = PublicAccountEvidence::Pda { program: attest_program, seed: header_seed(&id, slot) };
            let ev_token = PublicAccountEvidence::Pda { program: bridge, seed: bw::token_seed(&cid, &ETH) };
            let mut txs = Vec::new();
            let mut nonce = n;
            if !paired {
                let m1 = borsh::to_vec(&init)?;
                let f1 = max_fee(BRIDGE_GAS, m1.len() + 512, q);
                let (tx1, s1) = public_tx(config_actor, m1, vec![config_actor, header], vec![ev_config.clone(), ev_header.clone()], &[(&payer, nonce)], &payer, BRIDGE_GAS, f1);
                txs.push((tx1.clone(), tx_meta(&format!("bridge Init: config 0x{} paired with the vault's slot 2 under header({}, {slot})", &hex::encode(cid)[..8], &hex::encode(id)[..8]), &tx1, &s1, BRIDGE_GAS, f1)));
                nonce += 1;
            }
            let m2 = borsh::to_vec(&register)?;
            let f2 = max_fee(BRIDGE_GAS, m2.len() + 512, q);
            let (tx2, s2) = public_tx(config_actor, m2, vec![config_actor, header, token], vec![ev_config, ev_header, ev_token], &[(&payer, nonce)], &payer, BRIDGE_GAS, f2);
            txs.push((tx2.clone(), tx_meta("bridge Register ETH: registry entry, schedules 0 and 1, leaf count", &tx2, &s2, BRIDGE_GAS, f2)));
            let info = json!({"config_id": format!("0x{}", hex::encode(cid)), "vault": format!("0x{}", hex::encode(config.vault)),
                "record_slot": slot, "block_hash": format!("0x{}", hex::encode(block_hash_of(&hr)?)), "eth_schedule0_at": t,
                "eth_entry_hash": format!("0x{}", hex::encode(eth_entry().hash())), "eth_getProof": sources,
                "register_only": paired});
            (txs, info)
        }
        "bridge-import" => {
            let identity: Value = serde_json::from_str(&fs::read_to_string(a.path("identity")?)?)?;
            let cfg: Value = serde_json::from_str(&fs::read_to_string(a.path("bridge-config")?)?)?;
            let (config, bridge, cid) = bridge_config(&identity, &cfg)?;
            let (attest_program, id) = (config.attestation, config.instance_id);
            let config_actor = bw::config_actor(bridge, &cid);
            ensure!(!final_state(&live, config_actor).is_empty(), "config(0x{}) is not on the zone: Init first", hex::encode(cid));
            let (slot, hr, _) = newest_record(&live, attest_program, &id).await?;
            let dir = a.path("fetch")?.join("proofs").join(slot.to_string());
            ensure!(dir.join("proof.json").exists(), "no proofs for slot {slot} in {} (fetch them first)", dir.display());
            let (request, sources) = attest_request(&id, &dir)?;
            let acct = request.accounts.first().ok_or_else(|| anyhow!("no account in the proofs"))?;
            ensure!(acct.address == config.vault, "the proofs are of 0x{}, not the vault 0x{}", hex::encode(acct.address), hex::encode(config.vault));
            attest::evaluate(&id, SEPOLIA_CHAIN_ID, &hr, &request).map_err(|c| anyhow!("the attestation program would refuse these proofs (code {c})"))?;
            let value = |key: &bw::Word| -> Result<[u8; 32]> {
                let e = sources["slots"].as_array().into_iter().flatten()
                    .find(|e| word(&e["slot"], "slot").map(|w| w == *key).unwrap_or(false))
                    .ok_or_else(|| anyhow!("no proof of slot 0x{}", hex::encode(key)))?;
                word(&e["value"], "value")
            };
            let proof_of = |key: &bw::Word| -> Result<Vec<Vec<u8>>> {
                acct.slots.iter().find(|q| q.slot == *key).map(|q| q.proof.clone()).ok_or_else(|| anyhow!("no proof of slot 0x{}", hex::encode(key)))
            };
            let root = value(&bw::SLOT_ROOT)?;
            let count = value(&bw::SLOT_COUNT)?;
            ensure!(count[..24].iter().all(|b| *b == 0), "the vault's leaf count does not fit 64 bits");
            let leaves = u64::from_be_bytes(count[24..].try_into()?);
            ensure!(leaves > 0, "no deposit in the vault under record slot {slot} (leaf count 0)");
            let root_actor = bw::root_actor(bridge, &cid, leaves);
            ensure!(final_state(&live, root_actor).is_empty(), "root(cid, {leaves}) is on the zone already: nothing to import under record slot {slot}");
            let account = bw::with_header(fs::read(dir.join("header.rlp"))?, acct.proof.clone());
            let message = bw::BridgeMessage::ImportRoot {
                config_id: cid,
                record_slot: slot,
                root,
                leaf_count: leaves,
                proof: bw::ImportProof { account, root_slot: proof_of(&bw::SLOT_ROOT)?, count_slot: proof_of(&bw::SLOT_COUNT)? },
            };
            let header = header_actor(attest_program, &id, slot);
            let m = borsh::to_vec(&message)?;
            let f = max_fee(BRIDGE_GAS, m.len() + 512, q);
            let (tx, s1) = public_tx(config_actor, m, vec![config_actor, header, root_actor], vec![
                PublicAccountEvidence::Pda { program: bridge, seed: bw::config_seed(&cid) },
                PublicAccountEvidence::Pda { program: attest_program, seed: header_seed(&id, slot) },
                PublicAccountEvidence::Pda { program: bridge, seed: bw::root_seed(&cid, leaves) },
            ], &[(&payer, n)], &payer, BRIDGE_GAS, f);
            let info = json!({"config_id": format!("0x{}", hex::encode(cid)), "vault": format!("0x{}", hex::encode(config.vault)),
                "record_slot": slot, "block_hash": format!("0x{}", hex::encode(block_hash_of(&hr)?)), "leaf_count": leaves,
                "deposit_root": format!("0x{}", hex::encode(root)), "eth_getProof": sources});
            (vec![(tx.clone(), tx_meta(&format!("bridge ImportRoot n={leaves}: deposit root 0x{} under header({}, {slot})", &hex::encode(root)[..8], &hex::encode(id)[..8]), &tx, &s1, BRIDGE_GAS, f))], info)
        }
        "bridge-fees" => {
            // A claim's leaf must lie below the token's `known_until`, which registration set to the vault's leaf count
            // then; `ImportFees` with no new schedule (the vault's next fee slot empty) extends it to the count now.
            let identity: Value = serde_json::from_str(&fs::read_to_string(a.path("identity")?)?)?;
            let cfg: Value = serde_json::from_str(&fs::read_to_string(a.path("bridge-config")?)?)?;
            let (config, bridge, cid) = bridge_config(&identity, &cfg)?;
            let (attest_program, id) = (config.attestation, config.instance_id);
            let config_actor = bw::config_actor(bridge, &cid);
            let token_actor = bw::token_actor(bridge, &cid, &ETH);
            let tbytes = final_state(&live, token_actor);
            ensure!(!tbytes.is_empty(), "token(cid, ETH) is not on the zone: Register first");
            let bw::BridgeState::Token(token) = borsh::from_slice::<bw::BridgeState>(&tbytes)? else { bail!("token(cid, ETH) holds another state") };
            let index = token.schedules.len() as u64;
            let (slot, hr, _) = newest_record(&live, attest_program, &id).await?;
            let dir = a.path("fetch")?.join("proofs").join(slot.to_string());
            ensure!(dir.join("proof.json").exists(), "no proofs for slot {slot} in {} (fetch them first)", dir.display());
            let (request, sources) = attest_request(&id, &dir)?;
            let acct = request.accounts.first().ok_or_else(|| anyhow!("no account in the proofs"))?;
            ensure!(acct.address == config.vault, "the proofs are of 0x{}, not the vault 0x{}", hex::encode(acct.address), hex::encode(config.vault));
            attest::evaluate(&id, SEPOLIA_CHAIN_ID, &hr, &request).map_err(|c| anyhow!("the attestation program would refuse these proofs (code {c})"))?;
            let value = |key: &bw::Word| -> Result<[u8; 32]> {
                let e = sources["slots"].as_array().into_iter().flatten()
                    .find(|e| word(&e["slot"], "slot").map(|w| w == *key).unwrap_or(false))
                    .ok_or_else(|| anyhow!("no proof of slot 0x{} (fetch fee slot {index})", hex::encode(key)))?;
                word(&e["value"], "value")
            };
            let proof_of = |key: &bw::Word| -> Result<Vec<Vec<u8>>> {
                acct.slots.iter().find(|q| q.slot == *key).map(|q| q.proof.clone()).ok_or_else(|| anyhow!("no proof of slot 0x{}", hex::encode(key)))
            };
            let fees_key = bw::fee_slot(&ETH, index);
            ensure!(value(&fees_key)? == [0; 32], "the vault has an ETH fee schedule {index}: import it with its schedule (not done here)");
            let count = value(&bw::SLOT_COUNT)?;
            ensure!(count[..24].iter().all(|b| *b == 0), "the vault's leaf count does not fit 64 bits");
            let leaves = u64::from_be_bytes(count[24..].try_into()?);
            ensure!(leaves > token.known_until, "token(cid, ETH) is known until leaf {} already (vault count {leaves}): nothing to extend", token.known_until);
            let account = bw::with_header(fs::read(dir.join("header.rlp"))?, acct.proof.clone());
            let message = bw::BridgeMessage::ImportFees {
                config_id: cid,
                token: ETH,
                index,
                schedule: None,
                record_slot: slot,
                proof: bw::VaultProof { account, slots: vec![proof_of(&fees_key)?, proof_of(&bw::SLOT_COUNT)?] },
            };
            let header = header_actor(attest_program, &id, slot);
            let m = borsh::to_vec(&message)?;
            let f = max_fee(BRIDGE_GAS, m.len() + 512, q);
            let (tx, s1) = public_tx(config_actor, m, vec![config_actor, header, token_actor], vec![
                PublicAccountEvidence::Pda { program: bridge, seed: bw::config_seed(&cid) },
                PublicAccountEvidence::Pda { program: attest_program, seed: header_seed(&id, slot) },
                PublicAccountEvidence::Pda { program: bridge, seed: bw::token_seed(&cid, &ETH) },
            ], &[(&payer, n)], &payer, BRIDGE_GAS, f);
            let info = json!({"config_id": format!("0x{}", hex::encode(cid)), "record_slot": slot, "index": index,
                "known_until_before": token.known_until, "leaf_count": leaves, "eth_getProof": sources});
            (vec![(tx.clone(), tx_meta(&format!("bridge ImportFees ETH: schedule {index} absent, known until leaf {leaves} (was {})", token.known_until), &tx, &s1, BRIDGE_GAS, f))], info)
        }
        other => bail!("unknown stage {other}"),
    };
    if stage == "init" {
        // Before the stage: the instance's identity is in the store even if the stage's writing is cut.
        fs::write(store.join("instance.json"), serde_json::to_string_pretty(&info)? + "\n")?;
    }
    store_stage(&store, stage, &txs)?;
    // The follower as far as it read: the stage's `send` (and every later command on this state) resumes from it.
    live.save()?;
    fs::write(stage_dir(&store, stage).join("stage.json"), serde_json::to_string_pretty(&json!({"stage": stage, "payer": id_of(&payer).to_string(),
        "first_nonce": n.to_string(), "fee_state": [q.0, q.1], "info": info}))? + "\n")?;
    let mut reserve: u128 = 0;
    let mut expected: u128 = 0;
    for (tx, meta) in &txs {
        let bytes = borsh::to_vec(tx)?.len() as u128;
        let mf: u128 = meta["max_fee"].as_str().unwrap_or("0").parse()?;
        reserve = reserve.max(mf);
        expected += u128::from(meta["gas_limit"].as_u64().unwrap_or(0)) * u128::from(q.0) + bytes * u128::from(q.1);
        println!("{} {:>8} B gas limit {:>9} max fee {:>14} {}", meta["hash"].as_str().unwrap_or(""), bytes, meta["gas_limit"], mf, meta["label"].as_str().unwrap_or(""));
    }
    println!("stage {stage}: {} transactions signed for payer {} from nonce {n}; at the current base fees ({}, {}) at most {expected} if every one used its whole gas limit; the largest reserve {reserve}", txs.len(), id_of(&payer), q.0, q.1);
    Ok(())
}

async fn cmd_send(a: &Args) -> Result<()> {
    let state = a.path("state")?;
    let store = store_of(a)?;
    let stage = a.need("stage")?;
    let mut live = connect(a, &state).await?;
    let t0 = Instant::now();
    let ok = send_stage(&mut live, &store, stage).await?;
    live.save()?;
    println!("stage {stage}: {} in {:.1} s", if ok { "every transaction applied" } else { "NOT every transaction applied" }, t0.elapsed().as_secs_f64());
    ensure!(ok, "stage {stage}: a transaction was not applied");
    Ok(())
}

async fn cmd_status(a: &Args) -> Result<()> {
    let state = a.path("state")?;
    let store = store_of(a)?;
    let stage = a.need("stage")?;
    let live = connect(a, &state).await?;
    for (i, tx, meta) in stored(&store, stage)? {
        let (st, block) = tx_state(&live, &tx, &meta).await?;
        println!("{i:02} {} {st}{} {}", tx.hash(), block.map(|b| format!(" (block {b})")).unwrap_or_default(), meta["label"].as_str().unwrap_or(""));
    }
    Ok(())
}

async fn cmd_target(a: &Args) -> Result<()> {
    let keys = a.path("keys")?;
    let state = a.path("state")?;
    let store = store_of(a)?;
    let d = Deployment::load(&keys.join("program"))?;
    let inst: Value = serde_json::from_str(&fs::read_to_string(store.join("instance.json"))?)?;
    let id: Root = ferrata_updater::chain::root(inst["instance_id"].as_str().unwrap_or_default())?;
    let live = connect(a, &state).await?;
    let (slot, hr, st) = newest_record(&live, d.program(), &id).await?;
    live.save()?;
    println!("{}", json!({"slot": slot, "block_hash": format!("0x{}", hex::encode(block_hash_of(&hr)?)), "applied": st.applied}));
    Ok(())
}

/// Everything `verify` and `run` read: balances, nonces, the deployments' headers and segments, the instance.
async fn inspect(a: &Args, live: &Live) -> Result<Value> {
    let keys = a.path("keys")?;
    let mut v = json!({"rpc": a.need("rpc")?, "head": live.head().await?, "followed": live.follower.height(), "read_utc": crate::utc()});
    for who in ["payer", "daemon"] {
        let id = id_of(&read_key(&keys.join(format!("{who}.key")))?);
        v[who] = json!({"account": id.to_string(), "balance": live.balance(id).await?.to_string(), "nonce": nonce_of(live, id).await?.to_string()});
    }
    for (name, opt) in [("program", "bin"), ("bridge", "bridge-bin")] {
        if !Deployment::known(&keys.join(name)) {
            continue;
        }
        let d = Deployment::load(&keys.join(name))?;
        let header_state = final_state(live, Actor::new(d.program(), PROGRAM_LOADER_ACCOUNT_ID));
        let header = crate::programs::program_header(&header_state);
        // The segments: this deployment's keys, or (a program known by its account) the chain the header starts.
        let seg_ids: Vec<AccountId> = if d.header.is_some() {
            d.segments.iter().map(id_of).collect()
        } else {
            let mut ids = Vec::new();
            let mut next = header.as_ref().map(|h| h.program_first_segment);
            while let Some(id) = next {
                if ids.len() >= 64 || ids.contains(&id) {
                    break;
                }
                ids.push(id);
                next = segment_of(&final_state(live, Actor::new(id, PROGRAM_LOADER_ACCOUNT_ID))).and_then(|seg| seg.next_segment);
            }
            ids
        };
        let mut segs = Vec::new();
        for id in &seg_ids {
            let s = final_state(live, Actor::new(*id, PROGRAM_LOADER_ACCOUNT_ID));
            segs.push(match segment_of(&s) {
                Some(seg) => json!({"account": id.to_string(), "bytes": seg.bytecode.len(), "sha256": sha256_hex(&seg.bytecode),
                                    "next": seg.next_segment.map(|n| n.to_string())}),
                None => json!({"account": id.to_string(), "empty": s.is_empty()}),
            });
        }
        let mut entry = json!({"header": d.program().to_string(), "segments": segs,
            "deployed": header.as_ref().map(|h| json!({"image_id": image_id_hex(h.image_id), "immutable": h.immutable, "first_segment": h.program_first_segment.to_string()}))});
        if let Some(bin) = a.get(opt) {
            let binary = GuestBinary::load(name, Path::new(bin))?;
            let expected: Vec<String> = binary.chunks().iter().map(|c| sha256_hex(c)).collect();
            let matches = header.as_ref().is_some_and(|h| h.image_id == binary.image_id && h.immutable)
                && segs.iter().zip(&expected).all(|(s, e)| s["sha256"].as_str() == Some(e.as_str()))
                && segs.len() == expected.len();
            entry["binary"] = json!({"image_id": binary.image_hex(), "sha256": binary.sha256(), "chunks": expected.len(), "on_chain_equal": matches});
        }
        v[name] = entry;
    }
    let store = store_of(a)?;
    if store.join("instance.json").exists() {
        let inst: Value = serde_json::from_str(&fs::read_to_string(store.join("instance.json"))?)?;
        let id: Root = ferrata_updater::chain::root(inst["instance_id"].as_str().unwrap_or_default())?;
        let program: AccountId = inst["program"].as_str().unwrap_or_default().parse().map_err(|e| anyhow!("{e:?}"))?;
        let account = AccountId::for_public_pda(&program, &instance_seed(&id)).to_string();
        v["instance"] = match instance_state(live, program, &id).await? {
            None => json!({"id": hex::encode(id), "account": account, "exists": false}),
            Some(s) => {
                let store = CompactStore::decode(&s.store).map_err(|e| anyhow!("store: {e:?}"))?;
                let config_ok = s.config == ferrata_updater::chain::instance_config(PRESET, ADOPTION_PARTS, Some(LAYOUT))?;
                json!({"id": hex::encode(id), "account": account, "exists": true, "id_matches": s.id == id, "config_matches": config_ok, "active": s.active,
                       "applied": s.applied, "finalized_slot": store.finalized.slot, "finalized_header_ssz": hex::encode(header_ssz(&store.finalized)),
                       "root_id": hex::encode(s.root_id)})
            }
        };
    }
    Ok(v)
}

async fn cmd_inspect(a: &Args) -> Result<()> {
    let live = connect(a, &a.path("state")?).await?;
    live.save()?;
    println!("{}", serde_json::to_string_pretty(&inspect(a, &live).await?)?);
    Ok(())
}

/// Re-read every verdict from the endpoint and compare it with what the run recorded: exit 0 only if all match.
async fn cmd_verify(a: &Args) -> Result<()> {
    let state = a.path("state")?;
    let store = store_of(a)?;
    let mut live = connect(a, &state).await?;
    live.sync().await?;
    let v = inspect(a, &live).await?;
    let mut checks: Vec<(String, bool, String)> = Vec::new();
    let mut check = |name: &str, ok: bool, detail: String| checks.push((name.to_string(), ok, detail));
    check("program deployed byte for byte (image id, every segment, immutable)", v["program"]["binary"]["on_chain_equal"] == true,
          format!("{} at {}", v["program"]["deployed"]["image_id"], v["program"]["header"]));
    if a.get("bridge-bin").is_some() {
        check("bridge deployed byte for byte", v["bridge"]["binary"]["on_chain_equal"] == true, format!("{}", v["bridge"]["deployed"]["image_id"]));
    }
    for stage in ["deploy-program", "deploy-bridge", "init", "attest"] {
        let Ok(txs) = stored(&store, stage) else { continue };
        for (i, tx, meta) in txs {
            let recorded: Option<Value> = fs::read_to_string(stage_dir(&store, stage).join(format!("{i:02}.result.json"))).ok().and_then(|t| serde_json::from_str(&t).ok());
            let served = live.wallet.get_transaction(tx.hash()).await?.map(|(_, b)| b);
            let receipt = live.follower.receipt(&tx.hash()).cloned();
            let ok = match (&recorded, served, &receipt) {
                (Some(r), Some(b), Some(rc)) => r["block"] == json!(b) && rc.block_id == b && r["status"] == json!(rc.status()) && rc.status() == "applied",
                _ => false,
            };
            check(&format!("{stage} {i:02} {}", meta["label"].as_str().unwrap_or("")), ok,
                  format!("{} block {:?}, now {}", tx.hash(), served, receipt.as_ref().map(|r| r.status()).unwrap_or("not followed")));
        }
    }
    if v["instance"].is_object() {
        let i = &v["instance"];
        check("instance exists with the recorded id and configuration", i["exists"] == true && i["id_matches"] == true && i["config_matches"] == true, format!("{}", i["id"]));
        check("instance active (committee adopted)", i["active"] == true, format!("applied {}", i["applied"]));
        let min_applied = a.get("min-applied").and_then(|s| s.parse::<u64>().ok()).unwrap_or(1);
        check(&format!("instance applied at least {min_applied} jobs (the daemon follows Sepolia)"), i["applied"].as_u64().unwrap_or(0) >= min_applied, format!("applied {}, finalized slot {}", i["applied"], i["finalized_slot"]));
    }
    // The attestation: the settled transaction emitted its event, and its verdict (the program's function on the
    // record) holds the values eth_getProof served for the vault's slots at that block.
    if let Ok(stage) = fs::read_to_string(stage_dir(&store, "attest").join("stage.json")) {
        let s: Value = serde_json::from_str(&stage)?;
        let verdict = &s["info"]["verdict"];
        let got: Vec<&Value> = verdict["statements"].as_array().into_iter().flatten().filter_map(|st| st["fact"].get("value")).collect();
        let want: Vec<String> = s["info"]["eth_getProof"]["slots"].as_array().into_iter().flatten()
            .map(|e| format!("0x{}", hex::encode(word(&e["value"], "value").unwrap_or([0; 32])))).collect();
        let values_ok = got.len() == want.len() && got.iter().zip(&want).all(|(g, w)| g.as_str() == Some(w.as_str()));
        let events_ok = stored(&store, "attest").ok().and_then(|t| t.first().map(|(_, tx, _)| tx.hash())).and_then(|h| live.follower.receipt(&h).cloned()).is_some_and(|r| r.events >= 1 && r.exit_code.is_none());
        // What is compared: the verdict the program's own function computed natively on the record (kept when the
        // stage was signed) against eth_getProof's values; from the chain, only that the transaction applied with an
        // event (the follower keeps an event count, not the event's data).
        check("attestation: the verdict computed natively on the record equals eth_getProof's values; the transaction applied with an event (its content not compared)", values_ok && events_ok,
              format!("block {} {}: {:?}", verdict["block_number"], verdict["block_hash"], got));
    }
    let failed = checks.iter().filter(|c| !c.1).count();
    for (name, ok, detail) in &checks {
        println!("[{}] {name}: {detail}", if *ok { "ok" } else { "FAIL" });
    }
    let report = json!({"checks": checks.iter().map(|(n, ok, d)| json!({"check": n, "ok": ok, "detail": d})).collect::<Vec<_>>(), "inspect": v, "verified_utc": crate::utc()});
    if let Some(out) = a.get("out") {
        fs::write(out, serde_json::to_string_pretty(&report)? + "\n")?;
    }
    println!("verify: {} of {} checks match", checks.len() - failed, checks.len());
    ensure!(failed == 0 && !checks.is_empty(), "verify: {failed} check(s) do not match");
    Ok(())
}

/// Rehearsal only (`FERRATA_DAY1_REHEARSAL=yes`): LEZ's published local test account (funded by the genesis of LEZ's
/// `local.json`, `lez/live/driver/src/keys.rs`) pays the payer and the daemon key, as the owner's deposit would on a
/// public network.
async fn cmd_fund(a: &Args) -> Result<()> {
    use wallet::program_facades::{native_token_transfer::NativeTokenTransfer, CreditDelivery};
    ensure!(std::env::var("FERRATA_DAY1_REHEARSAL").as_deref() == Ok("yes"), "fund is the rehearsal's stand-in for the owner's deposit: FERRATA_DAY1_REHEARSAL=yes");
    let keys = a.path("keys")?;
    let amount: u128 = a.need("amount")?.parse()?;
    let state = a.path("state")?;
    let faucet = crate::keys_local_test()?;
    let run_dir = state.join("faucet");
    let opts = Options {
        rpc: a.need("rpc")?.to_string(),
        out_dir: run_dir.clone(),
        run_dir,
        harness_results: PathBuf::from("/nonexistent"),
        sequencer_log: None,
        gas_limit: INIT_GAS,
        wait_final: true,
    };
    let live = Live::open(opts, &[faucet.clone()]).await?;
    crate::live::check_send(&live.opts.rpc)?;
    for who in ["payer", "daemon"] {
        let key = read_key(&keys.join(format!("{who}.key")))?;
        let to = id_of(&key);
        let before = live.balance(to).await?;
        let (hash, _) = NativeTokenTransfer(&live.wallet)
            .transfer(wallet::AccountIdentity::Public(id_of(&faucet)), wallet::AccountIdentity::PublicForeign(PublicKey::new_from_private_key(&key)), amount, CreditDelivery::Automatic)
            .await
            .map_err(|e| anyhow!("the transfer to the {who} was not sent: {e:?}"))?;
        let t0 = Instant::now();
        loop {
            let now = live.balance(to).await?;
            if now >= before + amount {
                println!("fund: {who} {to} holds {now} ({hash}, {:.1} s)", t0.elapsed().as_secs_f64());
                break;
            }
            ensure!(t0.elapsed().as_secs() < 600, "fund: {who} holds {now} after 600 s");
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }
    Ok(())
}

/// The testnet's faucet (`deploy/testnet`, the gateway's `/faucet`): the public account of `--from-key` pays `--amount`
/// to the public account `--to` with LEZ's own native-token transfer (as `fund` does), through the wallet alone: no
/// follower, so a claim does not replay the chain. Waits for the transfer's own receipt (at most `--wait` seconds,
/// default 120) and prints one JSON line (hash, recipient, amount, state: applied, reverted or pending, block,
/// credited). With `--record FILE` the signed transaction is written to FILE before it is sent (`RecordingClient`).
/// The key is read from its file and never printed. Sending follows `live.rs`'s rules (`check_send`).
async fn cmd_transfer(a: &Args) -> Result<()> {
    use std::str::FromStr as _;
    let key = read_key(&a.path("from-key")?)?;
    let amount: u128 = a.need("amount")?.parse().context("--amount")?;
    ensure!(amount > 0, "--amount must be positive");
    // A public account never used before is credited only with admission evidence: its public key
    // (`--to-pk`, 64 hex digits, as LEZ's wallet prints it); `--to` alone reaches an account that exists.
    let to_pk = match a.get("to-pk") {
        Some(h) => {
            let b: [u8; 32] = hex::decode(h.trim().trim_start_matches("0x"))
                .context("--to-pk: not hex")?
                .try_into()
                .map_err(|_| anyhow!("--to-pk: not 32 bytes"))?;
            Some(PublicKey::try_new(b).map_err(|e| anyhow!("--to-pk: not a public key ({e:?})"))?)
        }
        None => None,
    };
    let to = match (a.get("to"), &to_pk) {
        (Some(t), pk) => {
            let t = t.trim();
            let t = t.strip_prefix("Public/").unwrap_or(t);
            let to = AccountId::from_str(t).map_err(|e| anyhow!("--to {t}: not a LEZ public account id ({e:?})"))?;
            if let Some(pk) = pk {
                ensure!(AccountId::from(pk) == to, "--to-pk is not the key of --to");
            }
            to
        }
        (None, Some(pk)) => AccountId::from(pk),
        (None, None) => bail!("--to ACCOUNT or --to-pk HEX is required"),
    };
    let from = id_of(&key);
    ensure!(to != from, "--to is the paying account itself");
    let rpc = a.need("rpc")?.to_string();
    crate::live::check_send(&rpc)?;
    let wait: u64 = a.get("wait").unwrap_or("120").parse().context("--wait")?;
    let home = a.path("state")?.join("wallet");
    let dest = match to_pk {
        Some(pk) => wallet::AccountIdentity::PublicForeign(pk),
        None => wallet::AccountIdentity::PublicNoSign(to),
    };
    match a.get("record") {
        Some(record) => {
            let w = open_recording_wallet(&home, &rpc, key, PathBuf::from(record), from)?;
            send_and_wait(&w, from, to, dest, amount, wait).await
        }
        None => send_and_wait(&crate::live::open_wallet(&home, &rpc, &[key])?, from, to, dest, amount, wait).await,
    }
}

/// The transfer, then its receipt polled for `wait` seconds: its own hash in a block, and whether its action reverted
/// there (`applied`, `reverted`; `pending` when not in a block yet). After the send a failed read is retried, never an
/// error: the caller must always learn the hash.
async fn send_and_wait<C: wallet::WalletSequencerClient>(
    w: &wallet::WalletCore<C>,
    from: AccountId,
    to: AccountId,
    dest: wallet::AccountIdentity,
    amount: u128,
    wait: u64,
) -> Result<()> {
    use wallet::program_facades::{native_token_transfer::NativeTokenTransfer, CreditDelivery};
    let (hash, _) = NativeTokenTransfer(w)
        .transfer(wallet::AccountIdentity::Public(from), dest, amount, CreditDelivery::Automatic)
        .await
        .map_err(|e| anyhow!("the transfer was not sent: {e:?}"))?;
    let t0 = Instant::now();
    let (mut state, mut block) = ("pending", None);
    while t0.elapsed().as_secs() < wait {
        if let Ok(Some((s, b))) = receipt(w.client(), &hash.0).await {
            (state, block) = (s, Some(b));
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    println!(
        "{}",
        json!({
            "hash": hex::encode(hash.0), "from": from.to_string(), "to": to.to_string(), "amount": amount.to_string(),
            "state": state, "block": block, "credited": state == "applied", "seconds": t0.elapsed().as_secs_f64(),
        })
    );
    Ok(())
}

/// The receipt of the transaction `hash`: `applied` or `reverted` (its action, in the block that holds it) and the
/// block's id, or None while no block holds it.
async fn receipt<C: wallet::WalletSequencerClient>(c: &C, hash: &[u8; 32]) -> Result<Option<(&'static str, u64)>> {
    let Some((_, id)) = c.get_transaction(common::HashType(*hash)).await.map_err(|e| anyhow!("getTransaction: {e}"))? else {
        return Ok(None);
    };
    let block = c.get_block(id).await.map_err(|e| anyhow!("getBlock {id}: {e}"))?.ok_or_else(|| anyhow!("block {id} is not served"))?;
    let i = block.body.transactions.iter().position(|t| t.hash().0 == *hash).ok_or_else(|| anyhow!("block {id} does not hold it"))?;
    Ok(Some((if block.body.reverted.contains(&(i as u64)) { "reverted" } else { "applied" }, id)))
}

/// The client of the faucet's transfers (`transfer --record FILE`): before a transaction leaves, its record (its hash,
/// its bytes and the sender's nonce it spends) is written whole to FILE (synced, renamed into place), and only then is it
/// sent. Whoever loses the tool's answer looks the transfer up by its own hash (`transfer-status`) and may send the very
/// same bytes again (`transfer-resend`: the same nonce, so at most one of them is ever applied).
#[derive(Clone)]
struct RecordingClient {
    inner: sequencer_service_rpc::SequencerClient,
    record: PathBuf,
    from: AccountId,
}

impl RecordingClient {
    fn write(&self, tx: &LeeTransaction) -> std::result::Result<(), String> {
        let nonce = match tx {
            LeeTransaction::Public(p) => p.message.nonces.get(&self.from).map(|n| n.0),
            LeeTransaction::PrivacyPreserving(_) => None,
        }
        .ok_or("not a public transaction the faucet account signs")?;
        let bytes = borsh::to_vec(tx).map_err(|e| e.to_string())?;
        let body = json!({"hash": hex::encode(tx.hash().0), "from": self.from.to_string(), "nonce": nonce.to_string(),
                          "tx": hex::encode(bytes)});
        let tmp = self.record.with_extension("partial");
        let mut f = fs::File::create(&tmp).map_err(|e| e.to_string())?;
        f.write_all(body.to_string().as_bytes()).and_then(|()| f.sync_all()).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &self.record).map_err(|e| e.to_string())?;
        if let Some(dir) = self.record.parent() {
            fs::File::open(dir).and_then(|d| d.sync_all()).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

impl wallet::WalletSequencerClient for RecordingClient {
    type Error = sequencer_service_rpc::ClientError;

    async fn send_transaction(&self, tx: LeeTransaction) -> std::result::Result<common::HashType, Self::Error> {
        self.write(&tx).map_err(|e| sequencer_service_rpc::ClientError::Custom(format!("not sent: its record: {e}")))?;
        <sequencer_service_rpc::SequencerClient as wallet::WalletSequencerClient>::send_transaction(&self.inner, tx).await
    }
    async fn get_block(&self, block_id: lee_core::BlockId) -> std::result::Result<Option<common::block::Block>, Self::Error> {
        <sequencer_service_rpc::SequencerClient as wallet::WalletSequencerClient>::get_block(&self.inner, block_id).await
    }
    async fn get_block_range(&self, start: lee_core::BlockId, end: lee_core::BlockId) -> std::result::Result<Vec<common::block::Block>, Self::Error> {
        <sequencer_service_rpc::SequencerClient as wallet::WalletSequencerClient>::get_block_range(&self.inner, start, end).await
    }
    async fn get_last_block_id(&self) -> std::result::Result<lee_core::BlockId, Self::Error> {
        <sequencer_service_rpc::SequencerClient as wallet::WalletSequencerClient>::get_last_block_id(&self.inner).await
    }
    async fn get_account_balance(&self, account_id: AccountId) -> std::result::Result<u128, Self::Error> {
        <sequencer_service_rpc::SequencerClient as wallet::WalletSequencerClient>::get_account_balance(&self.inner, account_id).await
    }
    async fn get_transaction(&self, tx_hash: common::HashType) -> std::result::Result<Option<(LeeTransaction, lee_core::BlockId)>, Self::Error> {
        <sequencer_service_rpc::SequencerClient as wallet::WalletSequencerClient>::get_transaction(&self.inner, tx_hash).await
    }
    async fn get_accounts_nonces(&self, account_ids: Vec<AccountId>) -> std::result::Result<Vec<Nonce>, Self::Error> {
        <sequencer_service_rpc::SequencerClient as wallet::WalletSequencerClient>::get_accounts_nonces(&self.inner, account_ids).await
    }
    async fn get_account_view(&self, actor_state_selector: lee_core::account::Actor) -> std::result::Result<Option<lee::Account>, Self::Error> {
        <sequencer_service_rpc::SequencerClient as wallet::WalletSequencerClient>::get_account_view(&self.inner, actor_state_selector).await
    }
    async fn get_proofs_and_root(
        &self,
        commitments: Vec<lee_core::Commitment>,
        message_positions: Vec<lee_core::CommitmentPosition>,
    ) -> std::result::Result<(Vec<Option<lee_core::MembershipProof>>, Vec<Option<lee_core::MembershipProof>>, lee_core::CommitmentSetDigest), Self::Error> {
        <sequencer_service_rpc::SequencerClient as wallet::WalletSequencerClient>::get_proofs_and_root(&self.inner, commitments, message_positions).await
    }
    async fn get_fee_state(&self) -> std::result::Result<sequencer_service_rpc::FeeStateQuote, Self::Error> {
        <sequencer_service_rpc::SequencerClient as wallet::WalletSequencerClient>::get_fee_state(&self.inner).await
    }
    async fn get_account(&self, account_id: AccountId) -> std::result::Result<Option<lee::Account>, Self::Error> {
        <sequencer_service_rpc::SequencerClient as wallet::WalletSequencerClient>::get_account(&self.inner, account_id).await
    }
    async fn get_program_ids(&self) -> std::result::Result<std::collections::BTreeMap<String, lee::ProgramId>, Self::Error> {
        <sequencer_service_rpc::SequencerClient as wallet::WalletSequencerClient>::get_program_ids(&self.inner).await
    }
    async fn get_publications(
        &self,
        from_position: lee_core::CommitmentPosition,
        limit: common::PageLimit,
    ) -> std::result::Result<Vec<(lee_core::CommitmentPosition, lee_core::program::Publication)>, Self::Error> {
        <sequencer_service_rpc::SequencerClient as wallet::WalletSequencerClient>::get_publications(&self.inner, from_position, limit).await
    }
    async fn get_recovery_binding(&self, address: AccountId) -> std::result::Result<Option<lee::EncryptedNote>, Self::Error> {
        <sequencer_service_rpc::SequencerClient as wallet::WalletSequencerClient>::get_recovery_binding(&self.inner, address).await
    }
}

/// The faucet's wallet over the recording client: LEZ's wallet as `live::open_wallet` makes it (its configuration, its
/// storage with the faucet's key), reopened over the client that records each transaction before it leaves. A send
/// whose action reverts in the wallet's simulation is not made (no fee spent on a transfer that would not credit).
fn open_recording_wallet(home: &Path, rpc: &str, key: PrivateKey, record: PathBuf, from: AccountId) -> Result<wallet::WalletCore<RecordingClient>> {
    let inner = crate::live::open_wallet(home, rpc, &[key])?.client_owned();
    wallet::WalletCore::new_update_chain_with_client(
        home.join("wallet_config.json"),
        home.join("storage.json"),
        None,
        "",
        RecordingClient { inner, record, from },
    )
}

fn read_record(path: &Path) -> Result<([u8; 32], AccountId, u128, Vec<u8>)> {
    use std::str::FromStr as _;
    let r: Value = serde_json::from_str(&fs::read_to_string(path).with_context(|| format!("{}", path.display()))?)?;
    let hash: [u8; 32] = hex::decode(r["hash"].as_str().context("record: hash")?)?.try_into().map_err(|_| anyhow!("record: hash"))?;
    let from = AccountId::from_str(r["from"].as_str().context("record: from")?).map_err(|e| anyhow!("record: from ({e:?})"))?;
    let nonce: u128 = r["nonce"].as_str().context("record: nonce")?.parse().context("record: nonce")?;
    let tx = hex::decode(r["tx"].as_str().context("record: tx")?)?;
    Ok((hash, from, nonce, tx))
}

/// `transfer-status --record FILE --rpc URL --state DIR`: what became of a recorded transfer, by its own hash:
/// `applied` or `reverted` (in a block), `dropped` (in no block, and the sender's nonce has moved past it: it never
/// can be), or `pending` (in no block yet, its nonce not spent: it may still be applied; `transfer-resend` sends it again).
async fn cmd_transfer_status(a: &Args) -> Result<()> {
    use wallet::WalletSequencerClient as _;
    let (hash, from, nonce, _) = read_record(&a.path("record")?)?;
    let rpc = a.need("rpc")?.to_string();
    let w = crate::live::open_wallet(&a.path("state")?.join("wallet"), &rpc, &[])?;
    // The sender's nonce first, then the receipt: a transaction applied before the nonce was read is asked for after
    // it; one not found while the nonce has moved past it is asked for once more, after a pause, before it is called
    // dropped.
    let now = w.client().get_accounts_nonces(vec![from]).await.map_err(|e| anyhow!("getAccountsNonces: {e}"))?;
    let now = now.first().map(|n| n.0).ok_or_else(|| anyhow!("getAccountsNonces: no answer"))?;
    let mut found = receipt(w.client(), &hash).await?;
    if found.is_none() && now > nonce {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        found = receipt(w.client(), &hash).await?;
    }
    let out = match found {
        Some((state, block)) => json!({"hash": hex::encode(hash), "state": state, "block": block}),
        None => json!({"hash": hex::encode(hash), "state": if now > nonce { "dropped" } else { "pending" },
                       "nonce": nonce.to_string(), "account_nonce": now.to_string()}),
    };
    println!("{out}");
    Ok(())
}

/// `transfer-resend --record FILE --rpc URL --state DIR`: the recorded transfer's very bytes sent again (its hash and
/// nonce unchanged: at most one of the two is ever applied).
async fn cmd_transfer_resend(a: &Args) -> Result<()> {
    use wallet::WalletSequencerClient as _;
    let (hash, _, _, bytes) = read_record(&a.path("record")?)?;
    let tx: LeeTransaction = borsh::from_slice(&bytes).context("record: tx")?;
    ensure!(tx.hash().0 == hash, "record: its bytes are not its hash");
    let rpc = a.need("rpc")?.to_string();
    crate::live::check_send(&rpc)?;
    let w = crate::live::open_wallet(&a.path("state")?.join("wallet"), &rpc, &[])?;
    let sent = w.client().send_transaction(tx).await.map_err(|e| anyhow!("not resent: {e}"))?;
    println!("{}", json!({"hash": hex::encode(sent.0), "resent": true}));
    Ok(())
}

pub fn main() -> Result<()> {
    let a = Args::parse()?;
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    rt.block_on(async {
        match a.cmd.as_str() {
            "keygen" => keygen(&a),
            "accounts" => {
                println!("{}", serde_json::to_string_pretty(&accounts_json(&a.path("keys")?)?)?);
                Ok(())
            }
            "sign" => cmd_sign(&a).await,
            "send" => cmd_send(&a).await,
            "status" => cmd_status(&a).await,
            "target" => cmd_target(&a).await,
            "inspect" => cmd_inspect(&a).await,
            "verify" => cmd_verify(&a).await,
            "fund" => cmd_fund(&a).await,
            "transfer" => cmd_transfer(&a).await,
            "transfer-status" => cmd_transfer_status(&a).await,
            "transfer-resend" => cmd_transfer_resend(&a).await,
            other => bail!("unknown command {other}: keygen accounts sign send status target inspect verify fund transfer transfer-status transfer-resend"),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A second instance on a network's published program: the keys' directory names the program by its account
    /// (`keygen --program`), and nothing can be signed for the program itself.
    #[test]
    fn a_program_known_by_its_account_has_no_keys_to_sign_with() {
        let dir = std::env::temp_dir().join(format!("day1-deployment-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let k = fresh_key().unwrap();
        write_key(&dir.join("own").join("header.key"), &k).unwrap();
        let own = Deployment::load(&dir.join("own")).unwrap();
        assert_eq!(own.program(), id_of(&k));
        assert!(own.header_key().is_ok());
        fs::create_dir_all(dir.join("theirs")).unwrap();
        fs::write(dir.join("theirs").join("account"), format!("{}\n", id_of(&k))).unwrap();
        let theirs = Deployment::load(&dir.join("theirs")).unwrap();
        assert_eq!(theirs.program(), id_of(&k));
        assert!(theirs.header_key().is_err());
        assert_eq!(theirs.json()["keys"], json!(false));
        assert!(Deployment::known(&dir.join("own")) && Deployment::known(&dir.join("theirs")));
        assert!(!Deployment::known(&dir.join("none")));
        fs::write(dir.join("theirs").join("account"), "not an account\n").unwrap();
        assert!(Deployment::load(&dir.join("theirs")).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }
}
