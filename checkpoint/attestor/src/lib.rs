//! `ferrata_attestor`: the rule (B) signer that runs beside each LEZ sequencer (SPEC-v3 D7 and D13;
//! `docs/design/bridge.md` §8.3), and the collector that puts `t` of `n` attestations into a
//! `checkpoint_b` input. The signed bytes, the certificate and the key-set hash are
//! [`attestor_core`]'s; this crate is the program around them.
//!
//! **Reading the final tier.** The attestor asks its own Logos Blockchain node three things, by the
//! node's HTTP API (or from saved answers of the same endpoints, laid out as `realblocks/extract.sh`
//! writes them): `/cryptarchia/info` (the last irreversible block's slot, `lib_slot`),
//! `/channel/<id>` (the channel's accredited keys and posting timeframe: the live key set `K`) and
//! `/cryptarchia/blocks` (the node's immutable blocks). It takes this channel's inscriptions from
//! blocks at or below `lib_slot`, and checks each the way the checkpoint program does: the
//! transaction's canonical encoding reproduces the node's hash, the inscription is well framed and
//! its signer's `verify_strict` signature covers the transaction hash, the inscribed LEZ block parses
//! strictly and re-hashes to the hash it carries, and consecutive blocks link (id + 1,
//! `prev_block_hash`, the inscription's `parent` = the previous inscription's message id). The final
//! tip is the last block of that linked run; its signer must be in `K`.
//!
//! **Signing.** Before it signs, the attestor checks that its certificate certifies its attestation
//! key for this deployment, that its channel key is in `K` (a certificate dies with its key), and
//! that the tip does not contradict what it signed before (a lower tip, or another hash at a height
//! it signed, is refused: an attestor never signs two histories). It signs the final-tier message v2
//! over `(deployment, keySetHash(K), tip, finalMsgId)`.
//!
//! **Collecting.** [`collect`] takes the published attestations, keeps those whose channel key is in
//! the parent state's key set, whose certificate and signature verify for the input's deployment and
//! key set, and which repeat no key, and refuses when fewer than `t(n)` remain. The checkpoint
//! program checks all of it again; the collector only avoids proving an input it would refuse.
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, ensure, Context as _, Result};
use attestor_core::{key_set_hash, verify_certificate, Deployment, FinalTip, SignedAttestation, VerifyError};
use checkpoint_core::hash::{sha256_parts, threshold};
use checkpoint_core::input::CheckpointInputB;
use checkpoint_core::lez::{hash_parsed_block, parse_block, NoVisitor};
use checkpoint_core::mantle::{inscription_msg_id, tx_hash, ElidedInscription, OP_CHANNEL_INSCRIBE, OP_TRANSFER};
use checkpoint_core::rule_b::run_checkpoint_b;
use checkpoint_core::state::Config;
use checkpoint_core::Hash;
use ed25519_dalek::SigningKey;
use serde_json::{json, Value};

/// The version tag of what an attestor publishes ([`attestation_json`]).
pub const RECORD_VERSION: &str = "ferrata-attestation/v1";

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

impl NodeAnswers {
    /// Saved answers: `channel.json`, `info.json` (or `info-start.json`, as `realblocks/extract.sh`
    /// names it), and `blocks/<from>-<to>.json` windows.
    pub fn load(dir: &Path) -> Result<Self> {
        let read = |p: PathBuf| -> Result<Value> {
            serde_json::from_slice(&fs::read(&p).with_context(|| format!("{}", p.display()))?)
                .with_context(|| format!("{}", p.display()))
        };
        let channel = read(dir.join("channel.json"))?;
        let info_path =
            if dir.join("info.json").exists() { dir.join("info.json") } else { dir.join("info-start.json") };
        let info = read(info_path)?;
        let lib_slot = info["cryptarchia_info"]["lib_slot"]
            .as_u64()
            .or_else(|| info["lib_slot"].as_u64())
            .context("cryptarchia info: no lib_slot")?;
        let mut files: Vec<PathBuf> =
            fs::read_dir(dir.join("blocks"))?.map(|e| Ok(e?.path())).collect::<Result<_>>()?;
        files.sort_by_key(|p| {
            p.file_name().and_then(|n| n.to_str()).and_then(|n| n.split('-').next()?.parse::<u64>().ok())
        });
        let mut blocks = Vec::new();
        for f in files {
            match read(f.clone())? {
                Value::Array(window) => blocks.extend(window),
                _ => bail!("{}: not an array of blocks", f.display()),
            }
        }
        Ok(Self { channel, lib_slot, blocks })
    }

    /// Asks a running node (with `curl`, as the lab's scripts do) and saves the answers in `dir` in
    /// the layout [`NodeAnswers::load`] reads, so that every signature can be checked afterwards
    /// against exactly what the attestor saw.
    pub fn fetch(node: &str, channel_id: &Hash, slot_from: u64, dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir.join("blocks"))?;
        let get = |url: String, out: PathBuf| -> Result<()> {
            let status = Command::new("curl").args(["-fsS", "-m", "60", "-o"]).arg(&out).arg(&url).status()?;
            ensure!(status.success(), "GET {url}: curl exit {status}");
            Ok(())
        };
        let node = node.trim_end_matches('/');
        get(format!("{node}/cryptarchia/info"), dir.join("info.json"))?;
        get(format!("{node}/channel/{}", hex::encode(channel_id)), dir.join("channel.json"))?;
        let info: Value = serde_json::from_slice(&fs::read(dir.join("info.json"))?)?;
        let lib_slot = info["cryptarchia_info"]["lib_slot"].as_u64().context("cryptarchia info: no lib_slot")?;
        let mut from = slot_from;
        while from <= lib_slot {
            let to = (from + 499).min(lib_slot);
            get(
                format!("{node}/cryptarchia/blocks?slot_from={from}&slot_to={to}"),
                dir.join("blocks").join(format!("{from}-{to}.json")),
            )?;
            from = to + 1;
        }
        Self::load(dir)
    }
}

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
                let inputs = p["inputs"].as_array().context("inputs")?;
                out.push(u8::try_from(inputs.len())?);
                for i in inputs {
                    out.extend(fr(i, "note id")?);
                }
                let outputs = p["outputs"].as_array().context("outputs")?;
                out.push(u8::try_from(outputs.len())?);
                for o in outputs {
                    out.extend(o["value"].as_u64().context("value")?.to_le_bytes());
                    out.extend(fr(&o["pk"], "pk")?);
                }
            }
            c => bail!("opcode {c:#04x} in an inscription transaction of this channel: not one LEZ sends"),
        }
    }
    Ok(out)
}

/// One inscription of a LEZ block, as the node holds it.
pub struct Inscription {
    pub node_block: String,
    pub slot: u64,
    pub ops: Vec<u8>,
    pub sig: [u8; 64],
    pub tx_hash: Hash,
}

impl Inscription {
    pub fn elided(&self) -> Result<ElidedInscription<'_>> {
        ElidedInscription::split(&self.ops, 0, &self.sig).map_err(|e| anyhow!("not an inscription: {e:?}"))
    }
    pub fn payload(&self) -> Result<&[u8]> {
        let e = self.elided()?;
        Ok(&self.ops[e.prefix.len()..self.ops.len() - e.suffix.len()])
    }
}

/// This channel's inscriptions in the node's final blocks (slot <= `lib_slot`), by LEZ block id.
/// Other channels' transactions are skipped; a LEZ block inscribed twice in the final tier refuses.
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
            let served: Hash = hexa(&tx["mantle_tx"]["hash"], "tx hash")?;
            let mut found = None;
            for rev in [false, true] {
                let bytes = encode_ops(ops, rev)?;
                if tx_hash(&bytes) == served {
                    found = Some(bytes);
                    break;
                }
            }
            let ops = found.with_context(|| format!("tx {}: no encoding reproduces its hash", hex::encode(served)))?;
            let ins = Inscription {
                node_block: b["header"]["id"].as_str().context("node block: id")?.to_string(),
                slot,
                ops,
                sig: hexa(&tx["ops_proofs"][0]["Ed25519Sig"], "inscription signature")?,
                tx_hash: served,
            };
            let payload = ins.payload()?;
            ensure!(payload.len() >= 8, "tx {}: payload shorter than a block id", hex::encode(served));
            let id = u64::from_le_bytes(payload[..8].try_into()?);
            ensure!(out.insert(id, ins).is_none(), "LEZ block {id} inscribed twice in the final tier");
        }
    }
    Ok(out)
}

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
    /// The linked run of LEZ blocks checked, first id and hashes by id.
    pub first_block: u64,
    pub hashes: BTreeMap<u64, Hash>,
}

/// Reads and checks the final tier (module docs).
pub fn final_tier(answers: &NodeAnswers, channel_id: &Hash) -> Result<FinalTier> {
    let keys: Vec<Hash> = answers.channel["accredited_keys"]
        .as_array()
        .context("channel: accredited_keys")?
        .iter()
        .map(|k| hexa::<32>(k, "accredited key"))
        .collect::<Result<_>>()?;
    ensure!(!keys.is_empty(), "the channel accredits no key");
    let posting_timeframe =
        u32::try_from(answers.channel["posting_timeframe"].as_u64().context("channel: posting_timeframe")?)?;
    let ins = final_inscriptions(answers, channel_id)?;
    ensure!(!ins.is_empty(), "no inscription of channel {} in the node's final tier", hex::encode(channel_id));

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
        let c =
            i.elided()?.check(payload, channel_id).map_err(|e| anyhow!("block {id}: inscription refused: {e:?}"))?;
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
    })
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

/// What the attestor signed last (its `--state` file): it never signs a lower tip or another hash at
/// a height it signed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LastSigned {
    pub tip_id: u64,
    pub tip_hash: Hash,
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
        if let Some(last) = last {
            ensure!(
                tier.tip.tip_id >= last.tip_id,
                "refused: the node's final tip {} is below the tip {} this attestor signed",
                tier.tip.tip_id,
                last.tip_id
            );
            if let Some(h) = tier.hashes.get(&last.tip_id) {
                ensure!(
                    *h == last.tip_hash,
                    "refused: block {} is {} in the node's final tier, but this attestor signed {}",
                    last.tip_id,
                    hex::encode(h),
                    hex::encode(last.tip_hash)
                );
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
                 "blocks_checked": tier.hashes.len()},
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
        Some(change) => change.op().map_err(|e| anyhow!("rotation: {e:?}"))?.0.keys.iter().map(|k| **k).collect(),
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

/// The real-blocks input `realblocks/out/real-lab-186-202.input` was written (b21dd13) before rule (B)
/// carried `T`'s inscription (91c0df8): `config ‖ parent ‖ changes ‖ blocks ‖ tIndex ‖ attestations(1) ‖
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
    let tier = final_tier(answers, channel_id)?;
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
            let c = i.elided()?.check(i.payload()?, channel_id).map_err(|e| anyhow!("block {id}: {e:?}"))?;
            Ok((*id, c.signer))
        })
        .collect()
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
    match (run_checkpoint_b(&input), expect) {
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
            let got = format!("{e:?}");
            ensure!(got.starts_with(want), "{name}: native run refused with {got}, expected {want}");
            meta["expect_refusal"] = json!(got);
            meta["native"] = json!(format!("refused: {got}"));
        }
        (Ok(_), Some(want)) => bail!("{name}: native run accepted an input expected to be refused ({want})"),
        (Err(e), None) => bail!("{name}: native run refused: {e:?}"),
    }
    Ok(meta)
}
