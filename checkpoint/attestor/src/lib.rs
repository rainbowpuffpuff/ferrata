//! `ferrata_attestor`: the rule (B) signer that runs beside each LEZ sequencer (SPEC-v3 D7 and D13;
//! `docs/design/bridge.md` §8.3), and the collector that puts `t` of `n` attestations into a
//! `checkpoint_b` input. The signed bytes, the certificate and the key-set hash are
//! [`attestor_core`]'s; this crate is the program around them.
//!
//! **What it signs.** Under rule (B) the checkpoint program executes nothing: a block's validity rests on
//! the attestations. So an attestor signs only a tip that **its own LEZ node holds in LEZ's final tier**:
//! applied, not parked. Bedrock admits any payload from the key whose turn it is (it checks only the
//! inscription's parent and signer); LEZ re-executes every finalized inscription and parks a block that
//! does not apply, keeping its final tip (`lez/chain_state/src/chain.rs`, `apply_final_block`). A LEZ
//! node stores only blocks that applied, and marks `Finalized` exactly the ids up to its final tip
//! (the sequencer's storage actor, `stage_new_blocks`). The attestor asks its LEZ node `getBlock(id)` and
//! takes a block as final in LEZ only when the node serves it with status `Finalized` and byte for byte
//! the block that was inscribed (all but the status byte, which lies outside the hash).
//!
//! **Reading.** The attestor asks its own Logos Blockchain node, by its HTTP API (or from saved answers of
//! the same endpoints, laid out as `realblocks/extract.sh` writes them): `/cryptarchia/info` (the last
//! irreversible block's slot, `lib_slot`), `/channel/<id>` (the channel's accredited keys and posting
//! timeframe: the live key set `K`) and `/cryptarchia/blocks` (the node's immutable blocks). It takes
//! every inscription op of this channel from blocks at or below `lib_slot` (any op of a transaction), and
//! reads each payload as a LEZ block (strict parse, re-hash). What does not parse, does not link or
//! repeats a block is skipped and noted, never a reason to stop: one turn holder must not be able to halt
//! every attestor. The chain is followed the way LEZ folds it, from the tip this attestor signed last
//! (its state): each step takes a block whose id is one more and whose `prev_block_hash` is the current
//! hash; where two inscriptions offer such a block (a parked one and its honest replacement), the LEZ node
//! decides. The end of the run is confirmed against the LEZ node; since LEZ's final tier is a hash chain,
//! a block it holds final confirms every block below it on the run.
//!
//! **The tip's inscription** is checked the way the checkpoint program checks `T`'s: the transaction's
//! canonical encoding reproduces the node's hash (inscriptions, transfers, channel withdrawals and channel
//! transfers: what LEZ's sequencer sends, atomic withdrawals included), the inscription is well framed,
//! its signer's `verify_strict` signature covers the transaction hash, and the signer is in `K`. A tip
//! whose transaction does not check is passed over for the block below it.
//!
//! **State.** After signing, the attestor records the tip, its message id, its inscription's slot and its
//! signer in its state file, atomically and before the attestation is published. The next round reads the
//! node from that slot on (the work per round does not grow with the channel's age), starts the chain at
//! that tip, and first asks the LEZ node whether it still holds that tip final: if it holds another block
//! there, the round refuses (two histories). Every signed tip therefore descends from the one before; an
//! attestor never signs two histories, across restarts too. A state file of the first version (tip id and
//! hash only) is read; the node is then read from `--slot-from`.
//!
//! **Logos Blockchain 0.3.5-rc.1.** A node of that release serves each transaction with its fork digest
//! (`mantle_tx.fork_digest`); its bytes are then `fork_digest ‖ ops` and its hash covers the digest. The attestor reads
//! either layout from the node's answers ([`Inscription::fork_digest`]) and checks each inscription in its own layout
//! (`checkpoint_core::bedrock_0_3_5` for 0.3.5-rc.1). Inputs built from such answers are run natively with the second
//! build's rule ([`run_b`]).
//!
//! **Signing.** Before it signs, the attestor checks that its certificate certifies its attestation
//! key for this deployment, that its channel key is in `K` (a certificate dies with its key), that the tier
//! was checked against a LEZ node, and that the tip descends from what it signed before (a lower tip, or a
//! tier that does not hold the signed tip, is refused). It signs the final-tier message v2 over
//! `(deployment, keySetHash(K), tip, finalMsgId)`.
//!
//! **Collecting.** [`collect`] takes the published attestations, keeps those whose channel key is in
//! the parent state's key set, whose certificate and signature verify for the input's deployment and
//! key set, and which repeat no key, and refuses when fewer than `t(n)` remain. The checkpoint
//! program checks all of it again; the collector only avoids proving an input it would refuse.
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, ensure, Context as _, Result};
use attestor_core::{key_set_hash, verify_certificate, Deployment, FinalTip, SignedAttestation, VerifyError};
use checkpoint_core::bedrock_0_3_5;
use checkpoint_core::hash::{sha256_parts, threshold};
use checkpoint_core::input::CheckpointInputB;
use checkpoint_core::lez::{hash_parsed_block, parse_block, NoVisitor};
use checkpoint_core::mantle::{
    inscription_msg_id, tx_hash, ChannelConfigOp, CheckedInscription, ConfigChange, ElidedInscription,
    OP_CHANNEL_INSCRIBE, OP_CHANNEL_TRANSFER, OP_CHANNEL_WITHDRAW, OP_TRANSFER,
};
use checkpoint_core::rule_b::{run_checkpoint_b, OutcomeB};
use checkpoint_core::state::Config;
use checkpoint_core::Hash;
use ed25519_dalek::SigningKey;
use serde_json::{json, Value};

/// The version tag of what an attestor publishes ([`attestation_json`]).
pub const RECORD_VERSION: &str = "ferrata-attestation/v1";

/// The version tag of the attestor's state file ([`write_state`]).
pub const STATE_VERSION: &str = "ferrata-attestor-state/v2";

/// `BedrockStatus::Finalized`, the last byte of a LEZ block as its node serves it (outside the hash).
pub const LEZ_FINALIZED: u8 = 2;

pub fn hexa<const N: usize>(v: &Value, what: &str) -> Result<[u8; N]> {
    let s = v.as_str().with_context(|| format!("{what}: not a string"))?;
    let b = hex::decode(s.strip_prefix("0x").unwrap_or(s)).with_context(|| format!("{what}: not hex"))?;
    b.try_into().map_err(|_| anyhow!("{what}: not {N} bytes"))
}

pub fn hex_str<const N: usize>(s: &str, what: &str) -> Result<[u8; N]> {
    hexa(&json!(s), what)
}

/// A 32-byte secret key file: the raw bytes, as LEZ keeps a sequencer's `channel_signing_key`.
pub fn read_key(path: &Path) -> Result<SigningKey> {
    let bytes: [u8; 32] = fs::read(path)
        .with_context(|| format!("{}", path.display()))?
        .try_into()
        .map_err(|_| anyhow!("{}: not a 32-byte key", path.display()))?;
    Ok(SigningKey::from_bytes(&bytes))
}

/// Writes a new secret key file, readable by its owner only (mode 0600); an existing file is never
/// overwritten (the check and the creation are one `create_new` open).
pub fn write_secret(path: &Path, secret: &[u8; 32]) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt as _;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("{}: a key is never overwritten", path.display()))?;
    f.write_all(secret)?;
    f.sync_all()?;
    Ok(())
}

/// Writes `bytes` to `path` atomically: a new temporary file beside it, synced, renamed over it, the
/// directory synced. A reader sees the old content or the new, never a part. The temporary file has a name of
/// its own (process, time, a counter) and is created with `create_new`, which never follows a symlink and never
/// opens a file that exists: two writers never share one, and a link planted beside `path` is not written through.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let name = path.file_name().with_context(|| format!("{}: not a file path", path.display()))?;
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let tmp = dir.join(format!(
        ".{}.{}.{nanos}.{}.tmp",
        name.to_string_lossy(),
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let written = (|| -> Result<()> {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .with_context(|| tmp.display().to_string())?;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&tmp, path).with_context(|| path.display().to_string())
    })();
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written?;
    fs::File::open(dir)?.sync_all()?;
    Ok(())
}

// ---- the deployment --------------------------------------------------------------------------------

pub fn deployment_of(config: &Config) -> Deployment {
    Deployment {
        evm_chain_id: config.evm_chain_id,
        vault: config.vault,
        lez_config_id: config.config_id(),
        channel_id: config.channel_id,
    }
}

pub fn deployment_json(d: &Deployment) -> Value {
    json!({
        "evm_chain_id": hex::encode(d.evm_chain_id),
        "vault": format!("0x{}", hex::encode(d.vault)),
        "lez_config_id": hex::encode(d.lez_config_id),
        "channel_id": hex::encode(d.channel_id),
    })
}

pub fn deployment_from_json(v: &Value) -> Result<Deployment> {
    Ok(Deployment {
        evm_chain_id: hexa(&v["evm_chain_id"], "evm_chain_id")?,
        vault: hexa(&v["vault"], "vault")?,
        lez_config_id: hexa(&v["lez_config_id"], "lez_config_id")?,
        channel_id: hexa(&v["channel_id"], "channel_id")?,
    })
}

// ---- the node's answers ----------------------------------------------------------------------------

/// What the attestor reads from its node: `/channel/<id>`, `/cryptarchia/info`, `/cryptarchia/blocks`.
pub struct NodeAnswers {
    pub channel: Value,
    pub lib_slot: u64,
    /// The node's blocks, all windows, in the order served.
    pub blocks: Vec<Value>,
}

fn read_json_file(p: &Path) -> Result<Value> {
    serde_json::from_slice(&fs::read(p).with_context(|| format!("{}", p.display()))?)
        .with_context(|| format!("{}", p.display()))
}

fn read_window(p: &Path) -> Result<Vec<Value>> {
    match read_json_file(p)? {
        Value::Array(window) => Ok(window),
        _ => bail!("{}: not an array of blocks", p.display()),
    }
}

fn lib_slot_of(info: &Value) -> Result<u64> {
    info["cryptarchia_info"]["lib_slot"]
        .as_u64()
        .or_else(|| info["lib_slot"].as_u64())
        .context("cryptarchia info: no lib_slot")
}

/// Whether `dir` is missing or holds no entry.
fn empty_dir(dir: &Path) -> Result<bool> {
    Ok(!dir.exists() || fs::read_dir(dir)?.next().is_none())
}

impl NodeAnswers {
    /// Saved answers: `channel.json`, `info.json` (or `info-start.json`, as `realblocks/extract.sh`
    /// names it), and `blocks/<from>-<to>.json` windows.
    pub fn load(dir: &Path) -> Result<Self> {
        let channel = read_json_file(&dir.join("channel.json"))?;
        let info_path =
            if dir.join("info.json").exists() { dir.join("info.json") } else { dir.join("info-start.json") };
        let lib_slot = lib_slot_of(&read_json_file(&info_path)?)?;
        let mut files: Vec<PathBuf> =
            fs::read_dir(dir.join("blocks"))?.map(|e| Ok(e?.path())).collect::<Result<_>>()?;
        files.sort_by_key(|p| {
            p.file_name().and_then(|n| n.to_str()).and_then(|n| n.split('-').next()?.parse::<u64>().ok())
        });
        let mut blocks = Vec::new();
        for f in files {
            blocks.extend(read_window(&f)?);
        }
        Ok(Self { channel, lib_slot, blocks })
    }

    /// Asks a running node (with `curl`, as the lab's scripts do) for its blocks from `slot_from` to its
    /// last irreversible slot, and saves the answers in `dir` in the layout [`NodeAnswers::load`] reads, so
    /// that every signature can be checked afterwards against exactly what the attestor saw. `dir` must
    /// hold no earlier answers (a window of another run would be read with these on a replay).
    pub fn fetch(node: &str, channel_id: &Hash, slot_from: u64, dir: &Path) -> Result<Self> {
        ensure!(
            empty_dir(&dir.join("blocks"))? && empty_dir(&dir.join("lez"))?,
            "{}: holds earlier answers; the attestor saves each read into a fresh directory",
            dir.display()
        );
        fs::create_dir_all(dir.join("blocks"))?;
        let get = |url: String, out: PathBuf| -> Result<()> {
            let status = Command::new("curl").args(["-fsS", "-m", "60", "-o"]).arg(&out).arg(&url).status()?;
            ensure!(status.success(), "GET {url}: curl exit {status}");
            Ok(())
        };
        let node = node.trim_end_matches('/');
        get(format!("{node}/cryptarchia/info"), dir.join("info.json"))?;
        get(format!("{node}/channel/{}", hex::encode(channel_id)), dir.join("channel.json"))?;
        let info = read_json_file(&dir.join("info.json"))?;
        let lib_slot = info["cryptarchia_info"]["lib_slot"].as_u64().context("cryptarchia info: no lib_slot")?;
        let channel = read_json_file(&dir.join("channel.json"))?;
        let mut blocks = Vec::new();
        let mut from = slot_from;
        while from <= lib_slot {
            let to = from.saturating_add(499).min(lib_slot);
            let out = dir.join("blocks").join(format!("{from}-{to}.json"));
            get(format!("{node}/cryptarchia/blocks?slot_from={from}&slot_to={to}"), out.clone())?;
            blocks.extend(read_window(&out)?);
            from = to + 1;
        }
        Ok(Self { channel, lib_slot, blocks })
    }
}

// ---- the LEZ node ----------------------------------------------------------------------------------

/// A node that did not answer, or answered what is not an answer: a round that fails on it failed for a
/// reason other than the answers (the daemon counts it as an error, not a refusal).
#[derive(Debug)]
pub struct NodeFault(pub String);

impl std::fmt::Display for NodeFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for NodeFault {}

/// The attestor's own LEZ node, asked block by block.
pub trait LezNode {
    /// The node's block `id` as it serves it (`borsh(Block)`, the bedrock status as its last byte), or
    /// none when it holds no block at that id.
    fn block(&mut self, id: u64) -> Result<Option<Vec<u8>>>;
}

/// A map of answers (tests, stand-ins): an id not in the map is a block the node does not hold.
impl LezNode for BTreeMap<u64, Option<Vec<u8>>> {
    fn block(&mut self, id: u64) -> Result<Option<Vec<u8>>> {
        Ok(self.get(&id).cloned().flatten())
    }
}

/// The path of a saved `getBlock` answer.
fn lez_answer_path(dir: &Path, id: u64) -> PathBuf {
    dir.join("lez").join(format!("getBlock-{id}.json"))
}

/// A LEZ node's JSON-RPC (`getBlock`, as LEZ's sequencer serves it), asked with `curl`; every answer is
/// saved in `<dir>/lez/getBlock-<id>.json`, beside the Bedrock node's answers.
pub struct LezRpc {
    url: String,
    dir: PathBuf,
}

impl LezRpc {
    pub fn new(url: &str, dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir.join("lez"))?;
        Ok(Self { url: url.to_string(), dir: dir.to_path_buf() })
    }
}

impl LezNode for LezRpc {
    fn block(&mut self, id: u64) -> Result<Option<Vec<u8>>> {
        let out = lez_answer_path(&self.dir, id);
        let body = format!(r#"{{"jsonrpc":"2.0","id":1,"method":"getBlock","params":[{id}]}}"#);
        let fault = |e: anyhow::Error| anyhow::Error::new(NodeFault(format!("LEZ node {}: {e:#}", self.url)));
        let status = Command::new("curl")
            .args(["-fsS", "-m", "30", "-H", "content-type: application/json", "-d"])
            .arg(&body)
            .arg("-o")
            .arg(&out)
            .arg(&self.url)
            .status()
            .map_err(|e| fault(e.into()))?;
        if !status.success() {
            return Err(fault(anyhow!("getBlock({id}): curl exit {status}")));
        }
        read_json_file(&out).and_then(|v| lez_reply(&v, id)).map_err(fault)
    }
}

/// Saved `getBlock` answers (`<dir>/lez/getBlock-<id>.json`): a replay. An answer that was not saved
/// reads as a block the node does not hold, so a replay never confirms more than the live run saw.
pub struct LezSaved {
    dir: PathBuf,
}

impl LezSaved {
    pub fn new(dir: &Path) -> Self {
        Self { dir: dir.to_path_buf() }
    }
}

impl LezNode for LezSaved {
    fn block(&mut self, id: u64) -> Result<Option<Vec<u8>>> {
        let p = lez_answer_path(&self.dir, id);
        if !p.exists() {
            return Ok(None);
        }
        lez_reply(&read_json_file(&p)?, id)
    }
}

/// A `getBlock` reply: `result` is base64 of `borsh(Block)` (LEZ's `borsh_base64`) or null.
fn lez_reply(v: &Value, id: u64) -> Result<Option<Vec<u8>>> {
    if let Some(e) = v.get("error") {
        bail!("LEZ getBlock({id}): {e}");
    }
    match v.get("result").with_context(|| format!("LEZ getBlock({id}): no result"))? {
        Value::Null => Ok(None),
        Value::String(s) => Ok(Some(base64_decode(s).with_context(|| format!("LEZ getBlock({id})"))?)),
        other => bail!("LEZ getBlock({id}): result is {other}, not base64 or null"),
    }
}

/// Standard base64 (RFC 4648 §4), padding optional.
pub fn base64_decode(s: &str) -> Result<Vec<u8>> {
    let b = s.trim_end_matches('=').as_bytes();
    ensure!(b.len() % 4 != 1, "not base64: length");
    let val = |c: u8| -> Result<u32> {
        Ok(u32::from(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => bail!("not base64: byte {c:#04x}"),
        }))
    };
    let mut out = Vec::with_capacity(b.len() * 3 / 4);
    for chunk in b.chunks(4) {
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= val(c)? << (18 - 6 * i);
        }
        let n = n.to_be_bytes();
        out.extend(&n[1..chunk.len()]);
    }
    Ok(out)
}

/// Standard base64 with padding (the encoding LEZ serves blocks in).
pub fn base64_encode(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(A[(n >> (18 - 6 * i) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Whether the LEZ node's `served` block is `inscribed` (same bytes but the status byte, outside the
/// hash) and final in its LEZ node.
fn holds_final(served: &[u8], inscribed: &[u8]) -> bool {
    match (served.split_last(), inscribed.split_last()) {
        (Some((&status, a)), Some((_, b))) => status == LEZ_FINALIZED && a == b,
        _ => false,
    }
}

/// The hash of a block the LEZ node serves as final: parsed strictly, re-hashed, status `Finalized`.
fn final_hash(served: &[u8]) -> Option<Hash> {
    let s = parse_block(served, &mut NoVisitor).ok()?;
    (s.status == LEZ_FINALIZED && hash_parsed_block(served).ok()? == s.header.hash).then_some(s.header.hash)
}

/// The LEZ node's answers of one read, each id asked once.
struct LezAsked<'a> {
    node: &'a mut dyn LezNode,
    asked: BTreeMap<u64, Option<Vec<u8>>>,
}

impl LezAsked<'_> {
    fn get(&mut self, id: u64) -> Result<Option<&[u8]>> {
        if !self.asked.contains_key(&id) {
            let b = self.node.block(id)?;
            self.asked.insert(id, b);
        }
        Ok(self.asked[&id].as_deref())
    }

    /// Whether the node holds a block at `id` with status `Finalized`.
    fn final_at(&mut self, id: u64) -> Result<bool> {
        Ok(self.get(id)?.is_some_and(|b| b.last() == Some(&LEZ_FINALIZED)))
    }

    /// Whether the node holds exactly `inscribed` (a block of id `id`) final.
    fn holds(&mut self, id: u64, inscribed: &[u8]) -> Result<bool> {
        Ok(self.get(id)?.is_some_and(|b| holds_final(b, inscribed)))
    }
}

/// Devnet stand-in for a LEZ node over saved answers: every block inscribed on `channel_id` in the node's
/// final blocks, served as final. It assumes what the attestor exists to check (that every inscribed block
/// applied), so it is only for replays of recorded honest runs that have no LEZ node's answers (the lab's
/// sequencer served all of its blocks `Finalized`, measured).
pub fn devnet_lez_standin(answers: &NodeAnswers, channel_id: &Hash) -> Result<BTreeMap<u64, Option<Vec<u8>>>> {
    let (entries, _) = channel_entries(answers, channel_id)?;
    let mut out = BTreeMap::new();
    for e in &entries {
        if let Ok(s) = parse_block(&e.payload, &mut NoVisitor) {
            let mut b = e.payload.clone();
            *b.last_mut().expect("a parsed block is not empty") = LEZ_FINALIZED;
            out.entry(s.header.block_id).or_insert(Some(b));
        }
    }
    Ok(out)
}

/// Writes `answers` as saved `getBlock` replies under `<dir>/lez/` (what [`LezSaved`] reads).
pub fn save_lez_answers(dir: &Path, answers: &BTreeMap<u64, Option<Vec<u8>>>) -> Result<()> {
    fs::create_dir_all(dir.join("lez"))?;
    for (id, b) in answers {
        let result = b.as_deref().map(base64_encode);
        fs::write(lez_answer_path(dir, *id), json!({"jsonrpc": "2.0", "id": 1, "result": result}).to_string())?;
    }
    Ok(())
}

// ---- transactions ----------------------------------------------------------------------------------

/// The canonical encoding of a transaction's ops (`checkpoint-core::mantle`'s module doc) from the
/// node's JSON (as `realblocks` does). `reverse_fr`: whether 32-byte field elements are served in the
/// opposite byte order to the wire's; the transaction hash decides which is right.
fn encode_ops(ops: &[Value], reverse_fr: bool) -> Result<Vec<u8>> {
    let fr = |v: &Value, what: &str| -> Result<[u8; 32]> {
        let mut b = hexa::<32>(v, what)?;
        if reverse_fr {
            b.reverse();
        }
        Ok(b)
    };
    // `Inputs` = `u8 n ‖ NoteId·n`; `Outputs` = `u8 n ‖ (value u64 ‖ pk)·n`.
    let inputs = |p: &Value, out: &mut Vec<u8>| -> Result<()> {
        let inputs = p["inputs"].as_array().context("inputs")?;
        out.push(u8::try_from(inputs.len())?);
        for i in inputs {
            out.extend(fr(i, "note id")?);
        }
        Ok(())
    };
    let outputs = |p: &Value, out: &mut Vec<u8>| -> Result<()> {
        let outputs = p["outputs"].as_array().context("outputs")?;
        out.push(u8::try_from(outputs.len())?);
        for o in outputs {
            out.extend(o["value"].as_u64().context("value")?.to_le_bytes());
            out.extend(fr(&o["pk"], "pk")?);
        }
        Ok(())
    };
    let mut out = vec![u8::try_from(ops.len())?];
    for op in ops {
        let code = u8::try_from(op["opcode"].as_u64().context("opcode")?)?;
        let p = &op["payload"];
        out.push(code);
        match code {
            OP_CHANNEL_INSCRIBE => {
                out.extend(hexa::<32>(&p["channel_id"], "channel_id")?);
                let ins = hex::decode(p["inscription"].as_str().context("inscription")?)?;
                out.extend(u32::try_from(ins.len())?.to_le_bytes());
                out.extend(&ins);
                out.extend(hexa::<32>(&p["parent"], "parent")?);
                out.extend(hexa::<32>(&p["signer"], "signer")?);
            }
            OP_TRANSFER => {
                inputs(p, &mut out)?;
                outputs(p, &mut out)?;
            }
            // `ChannelWithdrawOp { channel_id, inputs }` and `ChannelTransferOp { channel_id, inputs,
            // outputs }`: LEZ's `publish_atomic_withdraw` puts them after a block's inscription.
            OP_CHANNEL_WITHDRAW => {
                out.extend(hexa::<32>(&p["channel_id"], "channel_id")?);
                inputs(p, &mut out)?;
            }
            OP_CHANNEL_TRANSFER => {
                out.extend(hexa::<32>(&p["channel_id"], "channel_id")?);
                inputs(p, &mut out)?;
                outputs(p, &mut out)?;
            }
            c => bail!("opcode {c:#04x}: not one this attestor encodes (it encodes 0x00, 0x11, 0x13 and 0x14)"),
        }
    }
    Ok(out)
}

/// One inscription of a LEZ block, as the node holds it.
pub struct Inscription {
    pub node_block: String,
    pub slot: u64,
    /// The whole transaction: its operations, after its fork digest on Logos Blockchain 0.3.5-rc.1.
    pub ops: Vec<u8>,
    /// Which operation of the transaction is the inscription.
    pub op_index: usize,
    pub sig: [u8; 64],
    pub tx_hash: Hash,
    /// The fork digest the transaction carries (a 0.3.5-rc.1 node's answer), or none (the `d480efc6` layout).
    pub fork_digest: Option<Hash>,
}

impl Inscription {
    /// The transaction of `tx` (the node's JSON), encoded so that it reproduces the node's hash, with its
    /// inscription op `op_index` and that op's signature.
    pub fn from_node(tx: &Value, op_index: usize, node_block: &str, slot: u64) -> Result<Self> {
        let ops = tx["mantle_tx"]["ops"].as_array().context("ops")?;
        let fork_digest = match &tx["mantle_tx"]["fork_digest"] {
            Value::Null => None,
            v => Some(hexa::<32>(v, "fork digest")?),
        };
        let served: Hash = hexa(&tx["mantle_tx"]["hash"], "tx hash")?;
        let mut found = None;
        for rev in [false, true] {
            let mut bytes = fork_digest.map(|d| d.to_vec()).unwrap_or_default();
            bytes.extend(encode_ops(ops, rev)?);
            if tx_hash(&bytes) == served {
                found = Some(bytes);
                break;
            }
        }
        let ops = found.with_context(|| format!("tx {}: no encoding reproduces its hash", hex::encode(served)))?;
        Ok(Inscription {
            node_block: node_block.to_string(),
            slot,
            ops,
            op_index,
            sig: hexa(&tx["ops_proofs"][op_index]["Ed25519Sig"], "inscription signature")?,
            tx_hash: served,
            fork_digest,
        })
    }

    pub fn elided(&self) -> Result<ElidedInscription<'_>> {
        match self.fork_digest {
            None => ElidedInscription::split(&self.ops, self.op_index, &self.sig)
                .map_err(|e| anyhow!("not an inscription: {e:?}")),
            Some(_) => bedrock_0_3_5::mantle::split(&self.ops, self.op_index, &self.sig)
                .map_err(|e| anyhow!("not an inscription: {e:?}")),
        }
    }
    /// The inscription of its own payload on `channel_id`, checked as the checkpoint program checks it, in its layout
    /// (on 0.3.5-rc.1 with the digest it carries; the program also requires that digest to be the deployment's).
    pub fn check(&self, channel_id: &Hash) -> Result<CheckedInscription> {
        let e = self.elided()?;
        let payload = self.payload()?;
        match self.fork_digest {
            None => e.check(payload, channel_id).map_err(|e| anyhow!("{e:?}")),
            Some(d) => bedrock_0_3_5::mantle::check(&e, payload, channel_id, &d).map_err(|e| anyhow!("{e:?}")),
        }
    }
    pub fn payload(&self) -> Result<&[u8]> {
        let e = self.elided()?;
        Ok(&self.ops[e.prefix.len()..self.ops.len() - e.suffix.len()])
    }
}

/// This channel's inscriptions in the node's final blocks (slot <= `lib_slot`), by LEZ block id, for
/// devnet set-up over honest recorded answers ([`bedrock_tier`], [`retip`], [`base_from_answers`]):
/// only transactions whose first op is the inscription; a LEZ block inscribed twice refuses. The signer
/// does not use this ([`final_tier`]).
pub fn final_inscriptions(answers: &NodeAnswers, channel_id: &Hash) -> Result<BTreeMap<u64, Inscription>> {
    let mut out = BTreeMap::new();
    for b in &answers.blocks {
        let slot = b["header"]["slot"].as_u64().context("node block: slot")?;
        if slot > answers.lib_slot {
            continue;
        }
        for tx in b["transactions"].as_array().context("node block: transactions")? {
            let ops = tx["mantle_tx"]["ops"].as_array().context("ops")?;
            let Some(first) = ops.first() else { continue };
            if first["opcode"].as_u64() != Some(u64::from(OP_CHANNEL_INSCRIBE))
                || hexa::<32>(&first["payload"]["channel_id"], "channel")? != *channel_id
            {
                continue;
            }
            let ins = Inscription::from_node(tx, 0, b["header"]["id"].as_str().context("node block: id")?, slot)?;
            let payload = ins.payload()?;
            ensure!(payload.len() >= 8, "tx {}: payload shorter than a block id", hex::encode(ins.tx_hash));
            let id = u64::from_le_bytes(payload[..8].try_into()?);
            ensure!(out.insert(id, ins).is_none(), "LEZ block {id} inscribed twice in the final tier");
        }
    }
    Ok(out)
}

/// One inscription op of this channel in the node's final blocks, as served.
pub struct Entry<'a> {
    pub slot: u64,
    pub node_block: String,
    /// The transaction's JSON and the position of the inscription among its ops.
    pub tx: &'a Value,
    pub op_index: usize,
    pub payload: Vec<u8>,
}

/// Every inscription op on `channel_id` in the node's blocks at or below `lib_slot`, in the order
/// served (any op of a transaction). An op whose JSON does not read is noted and skipped; a node block
/// without a slot, id or transaction list is the node's fault and refuses.
pub fn channel_entries<'a>(answers: &'a NodeAnswers, channel_id: &Hash) -> Result<(Vec<Entry<'a>>, Vec<String>)> {
    let (mut entries, mut notes) = (Vec::new(), Vec::new());
    for b in &answers.blocks {
        let slot = b["header"]["slot"].as_u64().context("node block: slot")?;
        if slot > answers.lib_slot {
            continue;
        }
        let node_block = b["header"]["id"].as_str().context("node block: id")?;
        for (t, tx) in b["transactions"].as_array().context("node block: transactions")?.iter().enumerate() {
            let Some(ops) = tx["mantle_tx"]["ops"].as_array() else {
                notes.push(format!("slot {slot} tx {t}: no ops"));
                continue;
            };
            for (k, op) in ops.iter().enumerate() {
                if op["opcode"].as_u64() != Some(u64::from(OP_CHANNEL_INSCRIBE)) {
                    continue;
                }
                let p = &op["payload"];
                match hexa::<32>(&p["channel_id"], "channel") {
                    Ok(c) if c == *channel_id => {}
                    Ok(_) => continue,
                    Err(e) => {
                        notes.push(format!("slot {slot} tx {t} op {k}: {e}"));
                        continue;
                    }
                }
                match p["inscription"].as_str().map(hex::decode) {
                    Some(Ok(payload)) => {
                        entries.push(Entry { slot, node_block: node_block.to_string(), tx, op_index: k, payload })
                    }
                    _ => notes.push(format!("slot {slot} tx {t} op {k}: the inscription is not hex")),
                }
            }
        }
    }
    Ok((entries, notes))
}

/// A LEZ block an entry carries: parsed strictly and re-hashed.
struct Candidate {
    id: u64,
    prev: Hash,
    hash: Hash,
    entry: usize,
}

/// The blocks the entries carry, one per hash (a block inscribed again keeps its first inscription);
/// what does not parse or re-hash is noted and skipped.
fn candidates(entries: &[Entry<'_>], notes: &mut Vec<String>) -> Vec<Candidate> {
    let mut seen: HashMap<Hash, usize> = HashMap::new();
    let mut out = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        let parsed = parse_block(&e.payload, &mut NoVisitor).map_err(|err| format!("{err:?}")).and_then(|s| {
            match hash_parsed_block(&e.payload) {
                Ok(h) if h == s.header.hash => Ok(s),
                Ok(h) => Err(format!("re-hash {} != carried {}", hex::encode(h), hex::encode(s.header.hash))),
                Err(err) => Err(format!("{err:?}")),
            }
        });
        match parsed {
            Ok(s) => {
                if seen.contains_key(&s.header.hash) {
                    continue;
                }
                seen.insert(s.header.hash, out.len());
                out.push(Candidate {
                    id: s.header.block_id,
                    prev: s.header.prev_block_hash,
                    hash: s.header.hash,
                    entry: i,
                });
            }
            Err(err) => notes.push(format!(
                "slot {} tx in node block {} op {}: not a LEZ block ({} bytes): {err}",
                e.slot,
                e.node_block,
                e.op_index,
                e.payload.len()
            )),
        }
    }
    out
}

// ---- the final tier ---------------------------------------------------------------------------------

/// The node's final tier as the attestor signs it.
#[derive(Clone, Debug)]
pub struct FinalTier {
    /// `K`: the channel's accredited keys, in the node's order.
    pub keys: Vec<Hash>,
    pub posting_timeframe: u32,
    pub key_set_hash: Hash,
    pub tip: FinalTip,
    /// The channel key that inscribed the tip.
    pub tip_signer: Hash,
    pub tip_node_block: String,
    pub tip_slot: u64,
    pub lib_slot: u64,
    /// The run of LEZ blocks followed, first id and hashes by id (up to the tip).
    pub first_block: u64,
    pub hashes: BTreeMap<u64, Hash>,
    /// The fork digest the tip's inscription carries (Logos Blockchain 0.3.5-rc.1), or none.
    pub fork_digest: Option<Hash>,
    /// Whether the tip was checked against the attestor's LEZ node ([`final_tier`]); [`bedrock_tier`]'s
    /// tiers are not, and [`Attestor::attest`] refuses them.
    pub lez_checked: bool,
    /// What the read passed over and why: entries that are not blocks, blocks the LEZ node does not hold
    /// final, transactions that do not check.
    pub notes: Vec<String>,
}

fn channel_keys(answers: &NodeAnswers) -> Result<(Vec<Hash>, u32)> {
    let keys: Vec<Hash> = answers.channel["accredited_keys"]
        .as_array()
        .context("channel: accredited_keys")?
        .iter()
        .map(|k| hexa::<32>(k, "accredited key"))
        .collect::<Result<_>>()?;
    ensure!(!keys.is_empty(), "the channel accredits no key");
    let posting_timeframe =
        u32::try_from(answers.channel["posting_timeframe"].as_u64().context("channel: posting_timeframe")?)?;
    Ok((keys, posting_timeframe))
}

/// The Bedrock node's view alone, for devnet set-up and diagnostics (`tier`, [`base_from_answers`]): the
/// linked run of this channel's inscriptions from the first final one, each checked as the program checks
/// an inscription, strict (an entry that does not check refuses). Not checked against a LEZ node, so not
/// signable.
pub fn bedrock_tier(answers: &NodeAnswers, channel_id: &Hash) -> Result<FinalTier> {
    let (keys, posting_timeframe) = channel_keys(answers)?;
    let ins = final_inscriptions(answers, channel_id)?;
    ensure!(!ins.is_empty(), "no inscription of channel {} in the node's final tier", hex::encode(channel_id));
    let fork_digest = ins.values().next().and_then(|i| i.fork_digest);
    ensure!(
        ins.values().all(|i| i.fork_digest == fork_digest),
        "the final tier's inscriptions carry different fork digests (or some none)"
    );

    let mut hashes = BTreeMap::new();
    let mut prev: Option<(u64, Hash, Hash)> = None; // id, block hash, message id
    let mut tip = None;
    for (&id, i) in &ins {
        if let Some((pid, _, _)) = prev {
            if id != pid + 1 {
                break; // the final tier is the linked run from the first final inscription
            }
        }
        let payload = i.payload()?;
        let c = i.check(channel_id).map_err(|e| anyhow!("block {id}: inscription refused: {e}"))?;
        ensure!(c.tx_hash == i.tx_hash, "block {id}: the inscription's hash is not the node's");
        let s = parse_block(payload, &mut NoVisitor).map_err(|e| anyhow!("block {id}: parser refused it: {e:?}"))?;
        ensure!(s.header.block_id == id, "block {id}: carries id {}", s.header.block_id);
        let h = hash_parsed_block(payload).map_err(|e| anyhow!("block {id}: {e:?}"))?;
        ensure!(h == s.header.hash, "block {id}: re-hash {} != carried {}", hex::encode(h), hex::encode(s.header.hash));
        if let Some((pid, phash, pmsg)) = prev {
            ensure!(s.header.prev_block_hash == phash, "block {id}: does not link to block {pid}");
            ensure!(c.parent == pmsg, "block {id}: its inscription's parent is not block {pid}'s message");
        }
        let msg = inscription_msg_id(channel_id, payload, &c.parent, &c.signer);
        hashes.insert(id, h);
        prev = Some((id, h, msg));
        tip = Some((FinalTip { tip_id: id, tip_hash: h, final_msg_id: msg }, c.signer, i.node_block.clone(), i.slot));
    }
    let (tip, tip_signer, tip_node_block, tip_slot) = tip.context("no final tip")?;
    ensure!(
        keys.contains(&tip_signer),
        "the final tip {} was inscribed by {}, which the channel does not accredit now",
        tip.tip_id,
        hex::encode(tip_signer)
    );
    Ok(FinalTier {
        key_set_hash: key_set_hash(&keys, posting_timeframe),
        keys,
        posting_timeframe,
        tip,
        tip_signer,
        tip_node_block,
        tip_slot,
        lib_slot: answers.lib_slot,
        first_block: *ins.keys().next().unwrap_or(&0),
        hashes,
        fork_digest,
        lez_checked: false,
        notes: Vec::new(),
    })
}

/// The first read's starting block (no state): the highest block that the LEZ node holds final and the
/// Bedrock node's final tier holds. A LEZ node marks `Finalized` the ids up to its final tip, so over the
/// inscribed ids (sorted) "final in the LEZ node" holds up to some id and not above it: probing down from the
/// highest id in doubling steps finds a final one, and bisection the highest. (A block a LEZ node adopted from
/// another sequencer keeps the status inscribed with it, LEZ #1053; a status out of order only moves where the
/// walk below starts, and the candidates are inscriptions at or below the Bedrock node's LIB.) From there the
/// walk goes down the inscribed ids, every one, to the first whose inscribed block the LEZ node holds byte for
/// byte: blocks inscribed with made-up ids, above or below the chain, cost one question each (bounded by the
/// candidates of the read) and never stop the read (review L5: a cap of 16 ids let 16 junk ids stop it).
fn first_root(cands: &[Candidate], entries: &[Entry<'_>], lez: &mut LezAsked<'_>) -> Result<usize> {
    let mut ids: Vec<u64> = cands.iter().map(|c| c.id).collect();
    ids.sort_unstable();
    ids.dedup();
    ensure!(!ids.is_empty(), "no LEZ block among the node's final inscriptions");
    let n = ids.len();
    let (mut good, mut bad) = (None, n); // final at ids[good]; not final at ids[bad] (n: none known)
    let (mut i, mut step) = (n - 1, 1usize);
    loop {
        if lez.final_at(ids[i])? {
            good = Some(i);
            break;
        }
        bad = i;
        if i == 0 {
            break;
        }
        i = i.saturating_sub(step);
        step = step.saturating_mul(2);
    }
    let Some(mut good) = good else {
        bail!(
            "refused: the LEZ node holds none of the blocks inscribed in the node's final tier final (ids {}..={}): \
             it is not synced, follows another channel, or the read starts beyond its final tip",
            ids[0],
            ids[n - 1]
        )
    };
    while bad - good > 1 {
        let mid = good + (bad - good) / 2;
        if lez.final_at(ids[mid])? {
            good = mid;
        } else {
            bad = mid;
        }
    }
    let mut by_id: BTreeMap<u64, Vec<usize>> = BTreeMap::new();
    for (i, c) in cands.iter().enumerate() {
        by_id.entry(c.id).or_default().push(i);
    }
    for (&id, at) in by_id.range(..=ids[good]).rev() {
        for &i in at {
            if lez.holds(id, &entries[cands[i].entry].payload)? {
                return Ok(i);
            }
        }
    }
    bail!(
        "refused: the LEZ node's final blocks up to {} are none of the blocks inscribed in the node's final tier",
        ids[good]
    )
}

/// A block of the run: its id and hash, and the candidate that carries it (none for the signed tip
/// when its inscription is before the read).
#[derive(Clone, Copy)]
struct Step {
    id: u64,
    hash: Hash,
    cand: Option<usize>,
}

/// The tip's inscription, checked: what the attestation and the state record of it.
struct CheckedTip {
    tip: FinalTip,
    signer: Hash,
    node_block: String,
    slot: u64,
    fork_digest: Option<Hash>,
}

/// The full check of the inscription that carries a block: the transaction reproduces the node's hash,
/// the inscription is well framed and signed, and inscribes exactly the entry's payload.
fn check_entry(e: &Entry<'_>, channel_id: &Hash) -> Result<CheckedTip> {
    let ins = Inscription::from_node(e.tx, e.op_index, &e.node_block, e.slot)?;
    ensure!(ins.payload()? == e.payload.as_slice(), "the transaction inscribes another payload than its op shows");
    let c = ins.check(channel_id).map_err(|err| anyhow!("inscription refused: {err}"))?;
    ensure!(c.tx_hash == ins.tx_hash, "the inscription's hash is not the node's");
    let s = parse_block(&e.payload, &mut NoVisitor).map_err(|err| anyhow!("{err:?}"))?;
    Ok(CheckedTip {
        tip: FinalTip {
            tip_id: s.header.block_id,
            tip_hash: s.header.hash,
            final_msg_id: inscription_msg_id(channel_id, &e.payload, &c.parent, &c.signer),
        },
        signer: c.signer,
        node_block: e.node_block.clone(),
        slot: e.slot,
        fork_digest: ins.fork_digest,
    })
}

/// Reads the final tier the attestor signs (module docs): the run from `state`'s tip (or, with no state,
/// from the highest block the LEZ node holds final), each step decided as LEZ folds, its end confirmed by
/// the LEZ node `lez`, and the highest confirmed block whose inscription checks and whose signer is in `K`
/// as the tip. Refuses when the LEZ node does not hold the signed tip final.
pub fn final_tier(
    answers: &NodeAnswers,
    lez: &mut dyn LezNode,
    channel_id: &Hash,
    state: Option<&AttestorState>,
) -> Result<FinalTier> {
    let (keys, posting_timeframe) = channel_keys(answers)?;
    let (entries, mut notes) = channel_entries(answers, channel_id)?;
    let cands = candidates(&entries, &mut notes);
    let by_hash: HashMap<Hash, usize> = cands.iter().enumerate().map(|(i, c)| (c.hash, i)).collect();
    let mut children: HashMap<(u64, Hash), Vec<usize>> = HashMap::new();
    for (i, c) in cands.iter().enumerate() {
        if let Some(parent_id) = c.id.checked_sub(1) {
            children.entry((parent_id, c.prev)).or_default().push(i);
        }
    }
    let mut lez = LezAsked { node: lez, asked: BTreeMap::new() };

    // The start: the signed tip, still final in the LEZ node; or the first read's root.
    let root = match state {
        Some(s) => {
            let (id, want) = (s.last.tip_id, s.last.tip_hash);
            match lez.get(id)? {
                Some(b) if final_hash(b) == Some(want) => {}
                Some(b) => bail!(
                    "refused: the LEZ node's block {id} is {} (status {}), but this attestor signed {}: another history, \
                     or the LEZ node is not synced",
                    parse_block(b, &mut NoVisitor).map(|s| hex::encode(s.header.hash)).unwrap_or_else(|_| "unparsable".into()),
                    b.last().copied().unwrap_or(0),
                    hex::encode(want)
                ),
                None => bail!("refused: the LEZ node holds no block {id}, which this attestor signed: it is not synced"),
            }
            Step { id, hash: want, cand: by_hash.get(&want).copied() }
        }
        None => {
            let i = first_root(&cands, &entries, &mut lez)?;
            Step { id: cands[i].id, hash: cands[i].hash, cand: Some(i) }
        }
    };

    // Follow the run: one candidate child is taken, several are decided by the LEZ node.
    let mut run = vec![root];
    let mut confirmed = 0usize; // index of the highest step known final in the LEZ node
    loop {
        let cur = *run.last().expect("the run starts with its root");
        let Some(next) = cur.id.checked_add(1) else { break };
        let kids = children.get(&(cur.id, cur.hash)).map(Vec::as_slice).unwrap_or(&[]);
        match kids {
            [] => break,
            [k] => run.push(Step { id: next, hash: cands[*k].hash, cand: Some(*k) }),
            _ => {
                let mut chosen = None;
                for &k in kids {
                    if lez.holds(next, &entries[cands[k].entry].payload)? {
                        chosen = Some(k);
                        break;
                    }
                }
                match chosen {
                    Some(k) => {
                        run.push(Step { id: next, hash: cands[k].hash, cand: Some(k) });
                        confirmed = run.len() - 1;
                        notes.push(format!(
                            "block {next}: {} blocks inscribed on {}; the LEZ node holds {} final, the others are passed over",
                            kids.len(),
                            hex::encode(cur.hash),
                            hex::encode(cands[k].hash)
                        ));
                    }
                    None => {
                        notes.push(format!(
                            "block {next}: {} blocks inscribed on {}, the LEZ node holds none of them final",
                            kids.len(),
                            hex::encode(cur.hash)
                        ));
                        break;
                    }
                }
            }
        }
    }

    // Confirm the end of the run: the highest step the LEZ node holds final (a hash chain: monotone).
    let last = run.len() - 1;
    let holds = |lez: &mut LezAsked<'_>, s: &Step| -> Result<bool> {
        match s.cand {
            Some(c) => lez.holds(s.id, &entries[cands[c].entry].payload),
            None => Ok(true), // the signed tip, checked above
        }
    };
    let top = if confirmed == last || holds(&mut lez, &run[last])? {
        last
    } else {
        let (mut good, mut bad) = (confirmed, last);
        while bad - good > 1 {
            let mid = good + (bad - good) / 2;
            if holds(&mut lez, &run[mid])? {
                good = mid;
            } else {
                bad = mid;
            }
        }
        good
    };
    if top < last {
        notes.push(format!(
            "blocks {}..={}: inscribed, but not final in the LEZ node (parked, or not final there yet)",
            run[top + 1].id,
            run[last].id
        ));
    }

    // The tip: the highest confirmed block whose inscription checks and whose signer is in K.
    let mut chosen = None;
    for s in run[..=top].iter().rev() {
        let Some(c) = s.cand else { continue };
        match check_entry(&entries[cands[c].entry], channel_id) {
            Ok(t) if keys.contains(&t.signer) => {
                chosen = Some((s.id, t));
                break;
            }
            Ok(t) => notes.push(format!(
                "block {}: inscribed by {}, which the channel does not accredit now; passed over",
                s.id,
                hex::encode(t.signer)
            )),
            Err(e) => notes.push(format!("block {}: its inscription does not check ({e:#}); passed over", s.id)),
        }
    }
    let tip = match (chosen, state.and_then(|s| s.record.as_ref().map(|r| (s.last, r)))) {
        (Some((_, t)), _) => t,
        // Nothing above the signed tip checks: the signed tip again, as recorded.
        (None, Some((last, r))) => {
            ensure!(
                keys.contains(&r.tip_signer),
                "refused: the signed tip {} was inscribed by {}, which the channel does not accredit now, and no block \
                 above it checks",
                last.tip_id,
                hex::encode(r.tip_signer)
            );
            CheckedTip {
                tip: FinalTip { tip_id: last.tip_id, tip_hash: last.tip_hash, final_msg_id: r.final_msg_id },
                signer: r.tip_signer,
                node_block: r.tip_node_block.clone(),
                slot: r.tip_slot,
                fork_digest: r.fork_digest,
            }
        }
        _ => bail!(
            "refused: no block of the read is final in the LEZ node with an inscription that checks ({})",
            notes.join("; ")
        ),
    };
    let hashes: BTreeMap<u64, Hash> =
        run.iter().take_while(|s| s.id <= tip.tip.tip_id).map(|s| (s.id, s.hash)).collect();
    Ok(FinalTier {
        key_set_hash: key_set_hash(&keys, posting_timeframe),
        keys,
        posting_timeframe,
        tip: tip.tip,
        tip_signer: tip.signer,
        tip_node_block: tip.node_block,
        tip_slot: tip.slot,
        lib_slot: answers.lib_slot,
        first_block: root.id,
        hashes,
        fork_digest: tip.fork_digest,
        lez_checked: true,
        notes,
    })
}

// ---- the state ---------------------------------------------------------------------------------------

/// What the attestor signed last: it never signs a lower tip, nor a tip that does not descend from it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LastSigned {
    pub tip_id: u64,
    pub tip_hash: Hash,
}

/// The rest of the signed tip's record (state v2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TipRecord {
    pub final_msg_id: Hash,
    pub tip_signer: Hash,
    /// The slot of the node block that holds the tip's inscription: where the next read starts.
    pub tip_slot: u64,
    pub tip_node_block: String,
    pub fork_digest: Option<Hash>,
}

/// The attestor's state file: the tip it signed last, and (state v2) its record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttestorState {
    pub last: LastSigned,
    /// None for a state file of the first version (tip id and hash only).
    pub record: Option<TipRecord>,
}

impl AttestorState {
    /// The state after signing `tier`'s tip.
    pub fn of(tier: &FinalTier) -> Self {
        Self {
            last: LastSigned { tip_id: tier.tip.tip_id, tip_hash: tier.tip.tip_hash },
            record: Some(TipRecord {
                final_msg_id: tier.tip.final_msg_id,
                tip_signer: tier.tip_signer,
                tip_slot: tier.tip_slot,
                tip_node_block: tier.tip_node_block.clone(),
                fork_digest: tier.fork_digest,
            }),
        }
    }
}

/// Where a read starts: the signed tip's slot (state v2), else `slot_from`.
pub fn scan_from(state: Option<&AttestorState>, slot_from: u64) -> u64 {
    state.and_then(|s| s.record.as_ref()).map_or(slot_from, |r| r.tip_slot)
}

pub fn state_json(channel_id: &Hash, s: &AttestorState) -> Value {
    let mut v = json!({"version": STATE_VERSION, "channel_id": hex::encode(channel_id),
                       "tip_id": s.last.tip_id, "tip_hash": hex::encode(s.last.tip_hash)});
    if let Some(r) = &s.record {
        v["final_msg_id"] = json!(hex::encode(r.final_msg_id));
        v["tip_signer"] = json!(hex::encode(r.tip_signer));
        v["tip_slot"] = json!(r.tip_slot);
        v["tip_node_block"] = json!(r.tip_node_block);
        v["fork_digest"] = json!(r.fork_digest.map(hex::encode));
    }
    v
}

pub fn state_from_json(channel_id: &Hash, v: &Value) -> Result<AttestorState> {
    let last = LastSigned {
        tip_id: v["tip_id"].as_u64().context("state: tip_id")?,
        tip_hash: hexa(&v["tip_hash"], "state: tip_hash")?,
    };
    match v["version"].as_str() {
        None => Ok(AttestorState { last, record: None }), // the first version: {tip_id, tip_hash}
        Some(STATE_VERSION) => {
            let ch: Hash = hexa(&v["channel_id"], "state: channel_id")?;
            ensure!(
                ch == *channel_id,
                "the state file is of channel {}, not of {}",
                hex::encode(ch),
                hex::encode(channel_id)
            );
            Ok(AttestorState {
                last,
                record: Some(TipRecord {
                    final_msg_id: hexa(&v["final_msg_id"], "state: final_msg_id")?,
                    tip_signer: hexa(&v["tip_signer"], "state: tip_signer")?,
                    tip_slot: v["tip_slot"].as_u64().context("state: tip_slot")?,
                    tip_node_block: v["tip_node_block"].as_str().context("state: tip_node_block")?.to_string(),
                    fork_digest: match &v["fork_digest"] {
                        Value::Null => None,
                        d => Some(hexa(d, "state: fork_digest")?),
                    },
                }),
            })
        }
        Some(other) => bail!("state file version {other}: not one this attestor reads"),
    }
}

/// The state file at `path`, or none when it does not exist yet.
pub fn read_state(path: &Path, channel_id: &Hash) -> Result<Option<AttestorState>> {
    if !path.exists() {
        return Ok(None);
    }
    state_from_json(channel_id, &read_json_file(path)?).with_context(|| path.display().to_string()).map(Some)
}

/// Records `s` in the state file, atomically. Called before the attestation is published, so that a
/// published signature is always one the state knows.
pub fn write_state(path: &Path, channel_id: &Hash, s: &AttestorState) -> Result<()> {
    write_atomic(path, (serde_json::to_string_pretty(&state_json(channel_id, s))? + "\n").as_bytes())
}

// ---- the attestor ----------------------------------------------------------------------------------

/// One sequencer's attestor: its channel's public key, the certificate that key gave its attestation
/// key, the attestation key, and the deployment it serves.
pub struct Attestor {
    pub deployment: Deployment,
    pub channel_key: Hash,
    pub certificate: [u8; 64],
    pub att_key: SigningKey,
}

impl Attestor {
    pub fn attest(&self, tier: &FinalTier, last: Option<LastSigned>) -> Result<SignedAttestation> {
        let att_pub = self.att_key.verifying_key().to_bytes();
        ensure!(
            verify_certificate(&self.channel_key, &self.deployment, &att_pub, &self.certificate),
            "refused: the certificate does not certify attestation key {} by channel key {} for this deployment",
            hex::encode(att_pub),
            hex::encode(self.channel_key)
        );
        ensure!(
            tier.keys.contains(&self.channel_key),
            "refused: channel key {} is not in the channel's live key set (its certificate is dead)",
            hex::encode(self.channel_key)
        );
        ensure!(
            tier.lez_checked,
            "refused: this final tier was not checked against a LEZ node (a Bedrock-only view is for devnet set-up)"
        );
        if let Some(last) = last {
            ensure!(
                tier.tip.tip_id >= last.tip_id,
                "refused: the node's final tip {} is below the tip {} this attestor signed",
                tier.tip.tip_id,
                last.tip_id
            );
            match tier.hashes.get(&last.tip_id) {
                Some(h) if *h == last.tip_hash => {}
                Some(h) => bail!(
                    "refused: block {} is {} in the node's final tier, but this attestor signed {}",
                    last.tip_id,
                    hex::encode(h),
                    hex::encode(last.tip_hash)
                ),
                None => bail!(
                    "refused: the run read (blocks {}..={}) does not hold block {}, which this attestor signed: it does \
                     not show that the tip descends from it",
                    tier.first_block,
                    tier.tip.tip_id,
                    last.tip_id
                ),
            }
        }
        Ok(SignedAttestation::new(
            &self.att_key,
            self.channel_key,
            self.certificate,
            &self.deployment,
            &tier.key_set_hash,
            tier.tip,
        ))
    }
}

/// Why a round signed nothing.
#[derive(Debug)]
pub enum RoundError {
    /// A node did not answer (or answered nonsense): try again.
    Node(anyhow::Error),
    /// The state file could not be read or written.
    State(anyhow::Error),
    /// The answers were read and the attestor refuses to sign them.
    Refused(anyhow::Error),
}

impl std::fmt::Display for RoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoundError::Node(e) => write!(f, "reading the nodes: {e:#}"),
            RoundError::State(e) => write!(f, "the state file: {e:#}"),
            RoundError::Refused(e) => write!(f, "{e:#}"),
        }
    }
}

/// One signed round.
pub struct Round {
    pub tier: FinalTier,
    pub attestation: SignedAttestation,
    /// The slot the Bedrock node was read from.
    pub scanned_from: u64,
    /// The state before the round.
    pub before: Option<AttestorState>,
}

/// One round against live nodes, as the CLI's `attest --node` and the module's daemon run it: read the
/// state, ask the Bedrock node from the signed tip's slot (or `slot_from`), read the final tier against the
/// LEZ node, sign, and record the state (atomically) before returning; the caller publishes. The answers
/// are saved in `save` (a fresh directory).
pub fn round(
    attestor: &Attestor,
    node: &str,
    lez_url: &str,
    save: &Path,
    state_path: &Path,
    slot_from: u64,
) -> std::result::Result<Round, RoundError> {
    let channel = attestor.deployment.channel_id;
    let before = read_state(state_path, &channel).map_err(RoundError::State)?;
    let scanned_from = scan_from(before.as_ref(), slot_from);
    let answers = NodeAnswers::fetch(node, &channel, scanned_from, save).map_err(RoundError::Node)?;
    let mut lez = LezRpc::new(lez_url, save).map_err(RoundError::Node)?;
    let tier = final_tier(&answers, &mut lez, &channel, before.as_ref()).map_err(|e| {
        // A LEZ node that does not answer is a node error; everything else is a refusal.
        if e.chain().any(|c| c.is::<NodeFault>()) {
            RoundError::Node(e)
        } else {
            RoundError::Refused(e)
        }
    })?;
    let attestation = attestor.attest(&tier, before.as_ref().map(|s| s.last)).map_err(RoundError::Refused)?;
    write_state(state_path, &channel, &AttestorState::of(&tier)).map_err(RoundError::State)?;
    Ok(Round { tier, attestation, scanned_from, before })
}

/// What an attestor publishes: the attestation (also as the checkpoint input's 264 bytes), the key
/// set and the node facts it signed over.
pub fn attestation_json(d: &Deployment, a: &SignedAttestation, tier: &FinalTier) -> Value {
    json!({
        "version": RECORD_VERSION,
        "deployment": deployment_json(d),
        "channel_key": hex::encode(a.channel_key),
        "attestation_key": hex::encode(a.att_key),
        "certificate": hex::encode(a.certificate),
        "key_set": tier.keys.iter().map(hex::encode).collect::<Vec<_>>(),
        "posting_timeframe": tier.posting_timeframe,
        "key_set_hash": hex::encode(tier.key_set_hash),
        "tip_id": a.tip.tip_id,
        "tip_hash": hex::encode(a.tip.tip_hash),
        "final_msg_id": hex::encode(a.tip.final_msg_id),
        "signature": hex::encode(a.signature),
        "encoded": hex::encode(a.encode()),
        "node": {"lib_slot": tier.lib_slot, "tip_node_block": tier.tip_node_block, "tip_slot": tier.tip_slot,
                 "tip_signer": hex::encode(tier.tip_signer), "first_block": tier.first_block,
                 "blocks_checked": tier.hashes.len(), "fork_digest": tier.fork_digest.map(hex::encode)},
    })
}

pub fn attestation_from_json(v: &Value) -> Result<SignedAttestation> {
    ensure!(v["version"].as_str() == Some(RECORD_VERSION), "not a {RECORD_VERSION} record");
    let bytes = hex::decode(v["encoded"].as_str().context("encoded")?)?;
    let bytes: [u8; attestor_core::SIGNED_ATTESTATION_LEN] =
        bytes.try_into().map_err(|_| anyhow!("encoded: wrong length"))?;
    Ok(SignedAttestation::decode(&bytes))
}

// ---- the collector ---------------------------------------------------------------------------------

/// Why the collector left an attestation out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dropped {
    /// Its channel key is not in the parent state's key set.
    NotInKeySet,
    /// Its channel key or attestation key was already counted.
    Repeated,
    /// The certificate does not certify the attestation key for this deployment.
    Certificate,
    /// The attestation key did not sign this deployment, key set and tip.
    Signature,
}

#[derive(Debug)]
pub struct Collected {
    pub input: Vec<u8>,
    pub kept: Vec<usize>,
    pub dropped: Vec<(usize, Dropped)>,
    pub n: u32,
    pub t: u32,
}

/// Puts the valid attestations into `base` (a `checkpoint_b` input with no attestations needed) and
/// refuses when fewer than `t(n)` are valid. The pre-check uses the parent state's key set, so it
/// needs an input that applies no key rotation.
pub fn collect(base: &[u8], attestations: &[SignedAttestation]) -> Result<Collected> {
    let mut input = CheckpointInputB::decode(base).map_err(|e| anyhow!("base input: {e:?}"))?;
    let deployment = deployment_of(&input.config);
    // K: the parent's set, or the set of the input's last rotation (whose signatures and floors the
    // program checks; the collector only needs the set the attestations must be over).
    let keys: Vec<Hash> = match input.config_changes.last() {
        None => input.parent.keys.clone(),
        Some(change) => config_op(&input, change)?.keys.iter().map(|k| **k).collect(),
    };
    let keys = &keys;
    let ksh = key_set_hash(keys, input.parent.posting_timeframe);
    let n = u32::try_from(keys.len())?;
    let t = threshold(n);
    let (mut kept, mut dropped, mut chosen) = (Vec::new(), Vec::new(), Vec::new());
    for (i, a) in attestations.iter().enumerate() {
        let why = if !keys.contains(&a.channel_key) {
            Some(Dropped::NotInKeySet)
        } else if chosen.iter().any(|c: &SignedAttestation| c.channel_key == a.channel_key || c.att_key == a.att_key) {
            Some(Dropped::Repeated)
        } else {
            match a.verify(&deployment, &ksh) {
                Ok(()) => None,
                Err(VerifyError::Certificate) => Some(Dropped::Certificate),
                Err(VerifyError::Attestation) => Some(Dropped::Signature),
            }
        };
        match why {
            Some(w) => dropped.push((i, w)),
            None => {
                kept.push(i);
                chosen.push(*a);
            }
        }
    }
    let have = u32::try_from(chosen.len())?;
    ensure!(have >= t, "too few attestations: {have} valid, t(n = {n}) = {t} needed; dropped {dropped:?}");
    input.attestations = chosen;
    Ok(Collected { input: input.encode(), kept, dropped, n, t })
}

/// `base` with exactly `attestations`, unchecked: for inputs the checkpoint program must refuse.
pub fn assemble_unchecked(base: &[u8], attestations: &[SignedAttestation]) -> Result<Vec<u8>> {
    let mut input = CheckpointInputB::decode(base).map_err(|e| anyhow!("base input: {e:?}"))?;
    input.attestations = attestations.to_vec();
    Ok(input.encode())
}

/// The real-blocks input `realblocks/out/real-lab-186-202.input` was written (31fc7ae) before rule (B)
/// carried `T`'s inscription (490b5a4): `config ‖ parent ‖ changes ‖ blocks ‖ tIndex ‖ attestations(1) ‖
/// segments(0)`. This inserts `producer` after `tIndex` (the old tail is the one attestation, its count and
/// the empty segment list: 4 + 264 + 4 bytes) and checks that the result decodes whole.
pub fn upgrade_pre_producer_input(old: &[u8], producer: &ElidedInscription<'_>) -> Result<Vec<u8>> {
    let tail = 4 + attestor_core::SIGNED_ATTESTATION_LEN + 4;
    ensure!(old.len() > tail, "too short");
    let (head, rest) = old.split_at(old.len() - tail);
    ensure!(
        rest[..4] == 1u32.to_le_bytes() && rest[tail - 4..] == 0u32.to_le_bytes(),
        "not the one-attestation layout"
    );
    let mut w = checkpoint_core::input::Writer::default();
    w.inscription(producer);
    let out = [head, w.0.as_slice(), rest].concat();
    let input = CheckpointInputB::decode(&out).map_err(|e| anyhow!("the upgraded input does not decode: {e:?}"))?;
    ensure!(input.attestations.len() == 1 && input.t_index as usize + 1 == input.blocks.len(), "unexpected shape");
    Ok(out)
}

/// `base` with another `T` among its blocks: `tIndex` moved to block `tip` and the producer's inscription
/// replaced by the node's final inscription of that block. The blocks after `T` stay as the hash-linked
/// path along which attestations of descendants of `T` are checked.
pub fn retip(base: &[u8], answers: &NodeAnswers, tip: u64) -> Result<Vec<u8>> {
    let mut input = CheckpointInputB::decode(base).map_err(|e| anyhow!("base input: {e:?}"))?;
    let first = parse_block(input.blocks[0], &mut NoVisitor).map_err(|e| anyhow!("{e:?}"))?.header.block_id;
    let index = tip.checked_sub(first).context("the tip is before the input's first block")?;
    ensure!((index as usize) < input.blocks.len(), "the tip is after the input's last block");
    let ins = final_inscriptions(answers, &input.config.channel_id)?;
    let t = ins.get(&tip).with_context(|| format!("the node holds no final inscription of block {tip}"))?;
    input.t_index = u32::try_from(index)?;
    input.producer = t.elided()?;
    Ok(input.encode())
}

/// Devnet set-up, not part of an attestor: `base` re-keyed to the stand-in key set `keys` (in this
/// order) with the deployment floors `n_min`, `t_min`; its attestations removed. The producer's
/// inscription is kept, so `keys` must contain `T`'s signer for the program to accept it.
pub fn rekey_base(base: &[u8], keys: &[Hash], n_min: u32, t_min: u32) -> Result<Vec<u8>> {
    rekey_base_with_timeframe(base, keys, n_min, t_min, None)
}

/// [`rekey_base`], also replacing the parent's posting timeframe (a rotation needs a non-zero one,
/// SPEC-v3 D3; the lab's one-sequencer channel has 0).
pub fn rekey_base_with_timeframe(
    base: &[u8],
    keys: &[Hash],
    n_min: u32,
    t_min: u32,
    posting_timeframe: Option<u32>,
) -> Result<Vec<u8>> {
    let mut input = CheckpointInputB::decode(base).map_err(|e| anyhow!("base input: {e:?}"))?;
    ensure!(input.config_changes.is_empty(), "the base input rotates keys");
    input.parent.keys = keys.to_vec();
    if let Some(p) = posting_timeframe {
        input.parent.posting_timeframe = p;
    }
    input.config.n_min = n_min;
    input.config.t_min = t_min;
    input.attestations = Vec::new();
    Ok(input.encode())
}

/// A Mantle transaction with one `ChannelConfigOp` (`checkpoint-core::mantle`'s encoding: channel,
/// parent configuration, `u16` key count and keys, posting timeframe and timeout, configuration and
/// transfer thresholds), as the channel's keys would sign it to rotate the set. Devnet set-up: on a
/// running zone the sequencers' committee proposes and inscribes this transaction.
pub fn channel_config_tx(
    channel: &Hash,
    parent: &Hash,
    keys: &[Hash],
    posting_timeframe: u32,
    configuration_threshold: u16,
) -> Result<Vec<u8>> {
    let mut ops = vec![1u8, checkpoint_core::mantle::OP_CHANNEL_CONFIG];
    ops.extend(channel);
    ops.extend(parent);
    ops.extend(u16::try_from(keys.len())?.to_le_bytes());
    for k in keys {
        ops.extend(k);
    }
    ops.extend(posting_timeframe.to_le_bytes());
    ops.extend(posting_timeframe.to_le_bytes()); // posting timeout: one turn
    ops.extend(configuration_threshold.to_le_bytes());
    ops.extend(1u16.to_le_bytes()); // transfer threshold
    Ok(ops)
}

/// `base` with one rotation: the configuration transaction `ops` and its signatures over the
/// transaction hash, each `(index of the signing key in the parent's set, signing key)`.
pub fn rotate_base(base: &[u8], ops: &[u8], signers: &[(u16, &SigningKey)]) -> Result<Vec<u8>> {
    use ed25519_dalek::Signer as _;
    let mut input = CheckpointInputB::decode(base).map_err(|e| anyhow!("base input: {e:?}"))?;
    let hash = tx_hash(ops);
    let sigs: Vec<(u16, [u8; 64])> = signers.iter().map(|(i, k)| (*i, k.sign(&hash).to_bytes())).collect();
    input.config_changes = vec![checkpoint_core::mantle::ConfigChange {
        ops,
        op_index: 0,
        signatures: sigs.iter().map(|(i, s)| (*i, s)).collect(),
    }];
    Ok(input.encode())
}

/// Devnet set-up: a `checkpoint_b` input over the node's final blocks `from..=to` (`T = to`) built from the
/// node's answers alone. The parent state is block `from - 1` as inscribed, with the channel's key set,
/// posting timeframe and configuration tip as the node reports them; the configuration's bridge values are
/// stand-ins (SHA-256 of labels, chain 31337, a label-derived vault), `bedrockGenesisId` is `genesis_id`.
pub fn base_from_answers(
    answers: &NodeAnswers,
    channel_id: &Hash,
    genesis_id: Hash,
    from: u64,
    to: u64,
    n_min: u32,
    t_min: u32,
) -> Result<Vec<u8>> {
    let tier = bedrock_tier(answers, channel_id)?;
    ensure!(from >= 2 && from <= to && to <= tier.tip.tip_id, "span {from}..={to} not within 2..={}", tier.tip.tip_id);
    let ins = final_inscriptions(answers, channel_id)?;
    let payload = |k: u64| -> Result<&[u8]> {
        ins.get(&k).with_context(|| format!("no final inscription of block {k}"))?.payload()
    };
    let parent_header =
        parse_block(payload(from - 1)?, &mut NoVisitor).map_err(|e| anyhow!("block {}: {e:?}", from - 1))?.header;
    let label = |s: &str| sha256_parts(&[b"ferrata/attestor/zone/stand-in/", s.as_bytes()]);
    let mut evm_chain_id = [0u8; 32];
    evm_chain_id[24..].copy_from_slice(&31337u64.to_be_bytes());
    let config = Config {
        bedrock_genesis_id: genesis_id,
        channel_id: *channel_id,
        bridge_header: label("bridge header"),
        bridge_image_id: label("bridge image id"),
        evm_chain_id,
        vault: label("vault")[..20].try_into()?,
        attestation_root_id: label("attestation root id"),
        salt: label("salt"),
        n_min,
        t_min,
    };
    let parent = checkpoint_core::state::State {
        tip_id: from - 1,
        tip_hash: parent_header.hash,
        keys: tier.keys.clone(),
        posting_timeframe: tier.posting_timeframe,
        config_tip_hash: hexa(&answers.channel["config_tip_hash"], "config_tip_hash")?,
        burns: Default::default(),
        imported_leaf_count: 0,
        sunset: false,
    };
    let blocks: Vec<&[u8]> = (from..=to).map(payload).collect::<Result<_>>()?;
    let input = CheckpointInputB {
        config,
        parent,
        config_changes: Vec::new(),
        blocks,
        t_index: u32::try_from(to - from)?,
        producer: ins[&to].elided()?,
        attestations: Vec::new(),
        segments: Vec::new(),
    };
    Ok(input.encode())
}

/// Which channel key inscribed each final block (the checked signer of its inscription).
pub fn signers(answers: &NodeAnswers, channel_id: &Hash) -> Result<BTreeMap<u64, Hash>> {
    final_inscriptions(answers, channel_id)?
        .iter()
        .map(|(id, i)| {
            let c = i.check(channel_id).map_err(|e| anyhow!("block {id}: {e}"))?;
            Ok((*id, c.signer))
        })
        .collect()
}

// ---- the two layouts of an input ------------------------------------------------------------------

/// Whether `input`'s transactions are in Logos Blockchain 0.3.5-rc.1's layout: its `T` inscription starts with the
/// configuration's `bedrockGenesisId`, which for such a zone is the fork digest (`checkpoint_core::bedrock_0_3_5`). A
/// `d480efc6` transaction starts with its operation count, never with a 32-byte digest equal to that id.
pub fn is_bedrock_0_3_5(input: &CheckpointInputB<'_>) -> bool {
    input.producer.prefix.get(..32) == Some(input.config.bedrock_genesis_id.as_slice())
}

/// The native run of the program `input` is for: rule (B) of the default build, or of the second build for an input
/// in the 0.3.5-rc.1 layout; a refusal as the guest would print it (`Debug`).
pub fn run_b(input: &CheckpointInputB<'_>) -> std::result::Result<OutcomeB, String> {
    if is_bedrock_0_3_5(input) {
        bedrock_0_3_5::rule::run_checkpoint_b(input).map_err(|e| format!("{e:?}"))
    } else {
        run_checkpoint_b(input).map_err(|e| format!("{e:?}"))
    }
}

/// A rotation's `ChannelConfigOp`, read in `input`'s layout.
fn config_op<'a>(input: &CheckpointInputB<'a>, change: &ConfigChange<'a>) -> Result<ChannelConfigOp<'a>> {
    if is_bedrock_0_3_5(input) {
        Ok(bedrock_0_3_5::mantle::config_op(change, &input.config.bedrock_genesis_id)
            .map_err(|e| anyhow!("rotation: {e:?}"))?
            .0)
    } else {
        Ok(change.op().map_err(|e| anyhow!("rotation: {e:?}"))?.0)
    }
}

// ---- fixtures for the checkpoint host ---------------------------------------------------------------

/// The native run of the rule and the fixture meta the checkpoint host reads (`host bench`, `prove`):
/// with `expect`, the run must refuse with an error whose name starts with it.
pub fn fixture_meta(name: &str, bytes: &[u8], description: &str, expect: Option<&str>) -> Result<Value> {
    let input = CheckpointInputB::decode(bytes).map_err(|e| anyhow!("input: {e:?}"))?;
    let blocks = input.blocks.len();
    let block_bytes: usize = input.blocks.iter().map(|b| b.len()).sum();
    let txs: u32 = input
        .blocks
        .iter()
        .map(|b| parse_block(b, &mut NoVisitor).map(|s| s.tx_count))
        .sum::<Result<u32, _>>()
        .map_err(|e| anyhow!("{e:?}"))?;
    let t_index = input.t_index as usize;
    let tip = parse_block(input.blocks[t_index], &mut NoVisitor).map_err(|e| anyhow!("{e:?}"))?.header;
    let mut meta = json!({
        "kind": "checkpoint-b", "name": name, "description": description,
        "blocks": blocks, "block_bytes": block_bytes, "transactions": txs, "input_bytes": bytes.len(),
        "blocks_to_t": t_index + 1, "segments": 0,
        "n": input.parent.keys.len(), "t": checkpoint_core::hash::threshold(u32::try_from(input.parent.keys.len())?),
        "n_min": input.config.n_min, "t_min": input.config.t_min,
        "keys": input.parent.keys.iter().map(hex::encode).collect::<Vec<_>>(),
        "posting_timeframe": input.parent.posting_timeframe,
        "config_tip_hash": hex::encode(input.parent.config_tip_hash),
        "parent_tip_id": input.parent.tip_id, "parent_tip_hash": hex::encode(input.parent.tip_hash),
        "parent_state": hex::encode(input.parent.digest()),
        "tip": tip.block_id, "tip_hash": hex::encode(tip.hash),
        "attestations": input.attestations.iter().map(|x| json!({"channel_key": hex::encode(x.channel_key),
            "attestation_key": hex::encode(x.att_key), "tip_id": x.tip.tip_id})).collect::<Vec<_>>(),
        "deployment": deployment_json(&deployment_of(&input.config)),
    });
    match (run_b(&input), expect) {
        (Ok(outcome), None) => {
            let journal = outcome.journal.encode();
            meta["journal"] = json!(hex::encode(journal.as_slice()));
            meta["journal_sha256"] = json!(hex::encode(sha256_parts(&[journal.as_slice()])));
            meta["native"] = json!(format!(
                "accepted: n = {}, t = {}, {} attestations counted, producer {}",
                outcome.journal.n,
                outcome.threshold,
                outcome.attestors,
                hex::encode(outcome.producer)
            ));
        }
        (Err(e), Some(want)) => {
            let got = e;
            // The second build wraps the default build's refusals in `Core(..)` (checkpoint_core::bedrock_0_3_5::Error).
            ensure!(
                got.starts_with(want) || got.starts_with(&format!("Core({want}")),
                "{name}: native run refused with {got}, expected {want}"
            );
            meta["expect_refusal"] = json!(got);
            meta["native"] = json!(format!("refused: {got}"));
        }
        (Ok(_), Some(want)) => bail!("{name}: native run accepted an input expected to be refused ({want})"),
        (Err(e), None) => bail!("{name}: native run refused: {e}"),
    }
    Ok(meta)
}
