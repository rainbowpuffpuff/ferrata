//! `realblocks-crown`: the attestor and the rule (B) checkpoint input of the crown run
//! (`e2e/crown/README.md`, stage 7), from the devnet's own data at the end of LEZ session B.
//!
//! ```text
//! realblocks-crown --node DIR --channel-key FILE --genesis FILE --identity FILE
//!                  --parent T0 --tip T1 --out DIR [--name NAME] [--record DIR]
//! ```
//!
//! - `--node DIR`: the Logos Blockchain node's answers (`crown-extract.sh`): `info.json`
//!   (`/cryptarchia/info`), `channel.json` (`/channel/<id>`), `blocks/*.json` (`/cryptarchia/blocks`,
//!   which serves immutable blocks only: the node's own finality);
//! - `--channel-key FILE`: the sequencer's 32-byte channel signing key (never written anywhere by this
//!   tool; the attestation key is derived from it, see below);
//! - `--genesis FILE`: the node's deployment settings (SHA-256 = `bedrockGenesisId`);
//! - `--identity FILE`: the run's configuration (JSON, hex `0x…`): `bedrockGenesisId`, `channelId`,
//!   `bridgeHeader`, `bridgeImageId`, `chainId` (number), `vault`, `attestationRootId`, `salt`, `nMin`,
//!   `tMin`, `configId` (the bridge wire's `BridgeConfig::id`, checked against checkpoint-core's), and
//!   optionally `genesisState` (the vault's, checked against the state at `T0`);
//!   Instead of `--identity`, the LEZ side's own files (`lez/crown/session.sh`, spike interface lez.md):
//!   `--session-a session-a.json` (configuration fields, `tip` = T0) with `--vault 0x… --config-id 0x…` (or
//!   the file's own `vault`/`configId`) and optionally `--genesis-state 0x…`; `--session-b session-b.json`
//!   gives `tipT1` = T1 (its hash is checked);
//! - `--parent T0 --tip T1`: the parent tip (the vault's genesis state is the state at `T0`) and the
//!   attested tip; the input holds blocks `T0+1..=T1` as inscribed;
//! - `--record DIR`: optional `blocks.borsh` of the blocks as the sequencer served them (u32 count, then
//!   the blocks back to back, `lez/lab` and live-driver export format); compared, not used.
//! - A zone of several sequencers (`checkpoint/attestor/zone-bridge.sh`): `--att A.json,B.json,...` (records of
//!   `ferrata_attestor attest`, one per sequencer's attestor) instead of `--channel-key`, with `--n-min N --t-min T`
//!   (the vault's floors; default 1 and 1): the key set is every key the channel accredits, each inscription must be
//!   signed by one of them, and the input carries the given attestations (each checked for this deployment, key set
//!   and tip before the rule runs); the rule needs t(n) of them. A channel configuration change the node holds
//!   between T0 and T (a rotation the zone's committee proposed and Bedrock applied: a `ChannelConfigOp` with its
//!   `ChannelMultiSigProof`) goes into the input as it was inscribed (the transaction re-encoded and checked
//!   against its hash), the parent's key set and configuration tip being session A's (`--session-a`); each block's
//!   signer is checked against the key set in force at its node slot, and the attestations are over the set after it.
//!
//! A zone on Logos Blockchain 0.3.5-rc.1 (LEZ v0.4.0-rc3): its node serves every transaction with its fork digest
//! (`mantle_tx.fork_digest`). The tool then reads the transactions in that layout (`fork_digest ‖ ops`), requires every
//! one to carry the same digest and `bedrockGenesisId` (session A's, `crown-driver crown-a --fork-digest`) to be it in
//! place of the deployment settings' SHA-256, checks them with checkpoint-core's second build
//! (`checkpoint_core::bedrock_0_3_5`, the code of the `checkpoint_b_0_3_5` guest) and runs that build's rule (B).
//!
//! Every inscription the node holds for the channel is parsed strictly and re-hashed by
//! `checkpoint-core` exactly as the guest does, framed, BLAKE2b-hashed and signature-checked, and the
//! MsgId chain is followed. The attestor: the channel key certifies an attestation key whose secret
//! is `sha256("ferrata/crown/attestation-key/v1" ‖ channel secret)` (as secret as the channel key);
//! the attestation key signs the final-tier message for `T1`, which must be final by the node's own
//! rule (its inscription is in an immutable node block, at or below the node's LIB slot).
//!
//! Outputs in `--out`: `<name>.input` / `<name>.json` (the gpu-prover fixture pair, `kind:
//! checkpoint-b`), `attestor.json`, `burns.json` (every burn the checkpoint counts, in order, with the
//! burn tree's leaves; checked against the guest's own outcome), `blocks.tsv`, `inscriptions.jsonl`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, ensure, Context as _, Result};
use attestor_core::{sign_certificate, Deployment, FinalTip, SignedAttestation};
use checkpoint_core::hash::{sha256_parts, threshold};
use checkpoint_core::input::CheckpointInputB;
use checkpoint_core::lez::{hash_parsed_block, parse_block, Actor, NoVisitor, Origin, Visitor};
use checkpoint_core::mantle::{
    inscription_msg_id, tx_hash, ConfigChange, ElidedInscription, OP_CHANNEL_CONFIG, OP_CHANNEL_INSCRIBE, OP_TRANSFER,
};
use checkpoint_core::bedrock_0_3_5;
use checkpoint_core::rule_b::run_checkpoint_b;
use checkpoint_core::state::{key_set_hash, Config, State};
use checkpoint_core::tree::{burn_leaf, zero_hashes, Frontier};
use checkpoint_core::wire::BridgeMessage;
use checkpoint_core::{Address, Hash};
use ed25519_dalek::SigningKey;
use serde_json::{json, Value};

const STATUS: [&str; 3] = ["Pending", "Safe", "Finalized"];
const CHANNEL_ID: Hash = [1u8; 32];

fn args() -> Result<BTreeMap<String, String>> {
    let v: Vec<String> = std::env::args().skip(1).collect();
    ensure!(v.len().is_multiple_of(2), "arguments come in --key value pairs");
    v.chunks(2).map(|p| Ok((p[0].strip_prefix("--").context("expected --key")?.to_string(), p[1].clone()))).collect()
}

fn hexa<const N: usize>(v: &Value, what: &str) -> Result<[u8; N]> {
    let s = v.as_str().with_context(|| format!("{what}: not a string"))?;
    let b = hex::decode(s.strip_prefix("0x").unwrap_or(s)).with_context(|| format!("{what}: not hex"))?;
    b.try_into().map_err(|_| anyhow!("{what}: not {N} bytes"))
}

fn hx(b: &[u8]) -> String {
    format!("0x{}", hex::encode(b))
}

// ---- the node's transactions (as checkpoint/realblocks/src/main.rs reads them) ---------------------

/// The fork digest a node of Logos Blockchain 0.3.5-rc.1 serves with a transaction (`mantle_tx.fork_digest`), or none.
fn fork_digest_of(tx: &Value) -> Result<Option<Hash>> {
    match &tx["mantle_tx"]["fork_digest"] {
        Value::Null => Ok(None),
        v => Ok(Some(hexa::<32>(v, "fork digest")?)),
    }
}

/// The whole transaction's bytes: its fork digest (0.3.5-rc.1), then its operations.
fn encode_tx(fork_digest: Option<Hash>, ops: &[Value], reverse_fr: bool) -> Result<Vec<u8>> {
    let mut out = fork_digest.map(|d| d.to_vec()).unwrap_or_default();
    out.extend(encode_ops(ops, reverse_fr)?);
    Ok(out)
}

/// The canonical encoding of a transaction's ops from the node's JSON; `reverse_fr`: whether 32-byte
/// field elements are served in the opposite byte order (the transaction hash decides).
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
            OP_CHANNEL_CONFIG => {
                out.extend(bytes32(&p["channel"], "channel")?);
                out.extend(bytes32(&p["parent"], "parent")?);
                let keys = p["keys"].as_array().context("keys")?;
                out.extend(u16::try_from(keys.len())?.to_le_bytes());
                for k in keys {
                    out.extend(bytes32(k, "key")?);
                }
                out.extend(u32::try_from(number(&p["posting_timeframe"], "posting_timeframe")?)?.to_le_bytes());
                out.extend(u32::try_from(number(&p["posting_timeout"], "posting_timeout")?)?.to_le_bytes());
                out.extend(
                    u16::try_from(number(&p["configuration_threshold"], "configuration_threshold")?)?.to_le_bytes(),
                );
                out.extend(u16::try_from(number(&p["transfer_threshold"], "transfer_threshold")?)?.to_le_bytes());
            }
            c => bail!("opcode {c:#04x}: not one a LEZ sequencer was expected to send"),
        }
    }
    Ok(out)
}

/// 32 bytes served as hex (with or without 0x) or as an array of numbers.
fn bytes32(v: &Value, what: &str) -> Result<[u8; 32]> {
    match v {
        Value::String(_) => hexa::<32>(v, what),
        Value::Array(a) => a
            .iter()
            .map(|x| x.as_u64().and_then(|n| u8::try_from(n).ok()).context("byte"))
            .collect::<Result<Vec<u8>>>()?
            .try_into()
            .map_err(|_| anyhow!("{what}: not 32 bytes")),
        _ => bail!("{what}: neither hex nor bytes: {v}"),
    }
}

/// A number served as a number, a decimal string or a one-field wrapper.
fn number(v: &Value, what: &str) -> Result<u64> {
    match v {
        Value::Number(n) => n.as_u64().with_context(|| format!("{what}: {n}")),
        Value::String(x) => x.parse().with_context(|| format!("{what}: {x}")),
        Value::Object(m) if m.len() == 1 => number(m.values().next().context("value")?, what),
        _ => bail!("{what}: not a number: {v}"),
    }
}

/// A `ChannelMultiSigProof` as the node serves it: (key index, signature) pairs, ascending.
fn multisig(v: &Value) -> Result<Vec<(u16, [u8; 64])>> {
    let v = v.get("ChannelMultiSigProof").unwrap_or(v);
    let sigs = v.get("signatures").unwrap_or(v);
    let sig = |x: &Value| -> Result<[u8; 64]> {
        let x = match x {
            Value::Object(m) if m.len() == 1 => m.values().next().context("signature")?,
            _ => x,
        };
        hexa::<64>(x, "signature")
    };
    let mut out: Vec<(u16, [u8; 64])> = match sigs {
        Value::Object(m) => {
            m.iter().map(|(k, x)| Ok((k.parse::<u16>().context("key index")?, sig(x)?))).collect::<Result<_>>()?
        }
        Value::Array(a) => a
            .iter()
            .map(|e| match e {
                Value::Array(p) if p.len() == 2 => Ok((u16::try_from(number(&p[0], "key index")?)?, sig(&p[1])?)),
                Value::Object(m) => {
                    let i = m.get("index").or(m.get("key_index")).context("key index")?;
                    Ok((u16::try_from(number(i, "key index")?)?, sig(m.get("signature").context("signature")?)?))
                }
                _ => bail!("a multisig entry of an unknown shape: {e}"),
            })
            .collect::<Result<_>>()?,
        _ => bail!("a ChannelMultiSigProof of an unknown shape: {v}"),
    };
    out.sort_by_key(|(i, _)| *i);
    Ok(out)
}

/// A channel configuration transaction the node holds as final.
struct Cfg {
    node_block: String,
    slot: u64,
    ops: Vec<u8>,
    op_index: u8,
    sigs: Vec<(u16, [u8; 64])>,
    tx_hash: Hash,
    fork_digest: Option<Hash>,
}

impl Cfg {
    fn change(&self) -> ConfigChange<'_> {
        ConfigChange {
            ops: &self.ops,
            op_index: self.op_index,
            signatures: self.sigs.iter().map(|(i, s)| (*i, s)).collect(),
        }
    }
}

/// Every `ChannelConfigOp` of `channel` in the node's immutable blocks, in slot order.
fn load_configs(dir: &Path, channel: &Hash) -> Result<Vec<Cfg>> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)?.map(|e| Ok(e?.path())).collect::<Result<_>>()?;
    files
        .sort_by_key(|p| p.file_name().and_then(|n| n.to_str()).and_then(|n| n.split('-').next()?.parse::<u64>().ok()));
    let mut out = Vec::new();
    for f in files {
        let blocks: Vec<Value> = serde_json::from_slice(&fs::read(&f)?).with_context(|| format!("{}", f.display()))?;
        for b in blocks {
            for tx in b["transactions"].as_array().context("transactions")? {
                let ops = tx["mantle_tx"]["ops"].as_array().context("ops")?;
                let Some(op_index) = ops.iter().position(|o| {
                    o["opcode"].as_u64() == Some(u64::from(OP_CHANNEL_CONFIG))
                        && bytes32(&o["payload"]["channel"], "channel").ok().as_ref() == Some(channel)
                }) else {
                    continue;
                };
                let served: Hash = hexa(&tx["mantle_tx"]["hash"], "tx hash")?;
                let fork_digest = fork_digest_of(tx)?;
                let bytes = [false, true]
                    .into_iter()
                    .map(|rev| encode_tx(fork_digest, ops, rev))
                    .collect::<Result<Vec<_>>>()?
                    .into_iter()
                    .find(|bytes| tx_hash(bytes) == served)
                    .with_context(|| {
                        format!("configuration tx {}: no encoding reproduces its hash", hex::encode(served))
                    })?;
                let proof = &tx["ops_proofs"][op_index];
                let sigs = multisig(proof)
                    .with_context(|| format!("configuration tx {}: its proof {proof}", hex::encode(served)))?;
                out.push(Cfg {
                    node_block: b["header"]["id"].as_str().context("header id")?.to_string(),
                    slot: b["header"]["slot"].as_u64().context("slot")?,
                    ops: bytes,
                    op_index: u8::try_from(op_index)?,
                    sigs,
                    tx_hash: served,
                    fork_digest,
                });
            }
        }
    }
    out.sort_by_key(|c| c.slot);
    Ok(out)
}

struct Ins {
    node_block: String,
    slot: u64,
    ops: Vec<u8>,
    sig: [u8; 64],
    tx_hash: Hash,
    fork_digest: Option<Hash>,
}

impl Ins {
    fn elided(&self) -> Result<ElidedInscription<'_>> {
        match self.fork_digest {
            None => ElidedInscription::split(&self.ops, 0, &self.sig).map_err(|e| anyhow!("not an inscription: {e:?}")),
            Some(_) => {
                bedrock_0_3_5::mantle::split(&self.ops, 0, &self.sig).map_err(|e| anyhow!("not an inscription: {e:?}"))
            }
        }
    }
    fn payload(&self) -> Result<&[u8]> {
        let e = self.elided()?;
        Ok(&self.ops[e.prefix.len()..self.ops.len() - e.suffix.len()])
    }
}

/// Every inscription of `channel` in the node's immutable blocks, by LEZ block id.
fn load_inscriptions(dir: &Path, channel: &Hash) -> Result<(BTreeMap<u64, Ins>, usize, usize)> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)?.map(|e| Ok(e?.path())).collect::<Result<_>>()?;
    files
        .sort_by_key(|p| p.file_name().and_then(|n| n.to_str()).and_then(|n| n.split('-').next()?.parse::<u64>().ok()));
    let (mut out, mut node_blocks, mut other) = (BTreeMap::new(), 0usize, 0usize);
    for f in files {
        let blocks: Vec<Value> = serde_json::from_slice(&fs::read(&f)?).with_context(|| format!("{}", f.display()))?;
        for b in blocks {
            node_blocks += 1;
            for tx in b["transactions"].as_array().context("transactions")? {
                let ops = tx["mantle_tx"]["ops"].as_array().context("ops")?;
                let first_is_ours = ops.first().and_then(|o| o["opcode"].as_u64())
                    == Some(u64::from(OP_CHANNEL_INSCRIBE))
                    && hexa::<32>(&ops[0]["payload"]["channel_id"], "channel")? == *channel;
                if !first_is_ours {
                    other += 1;
                    continue;
                }
                let served: Hash = hexa(&tx["mantle_tx"]["hash"], "tx hash")?;
                let fork_digest = fork_digest_of(tx)?;
                let mut found = None;
                for rev in [false, true] {
                    let bytes = encode_tx(fork_digest, ops, rev)?;
                    if tx_hash(&bytes) == served {
                        found = Some(bytes);
                        break;
                    }
                }
                let bytes =
                    found.with_context(|| format!("tx {}: no encoding reproduces its hash", hex::encode(served)))?;
                let ins = Ins {
                    node_block: b["header"]["id"].as_str().context("header id")?.to_string(),
                    slot: b["header"]["slot"].as_u64().context("slot")?,
                    ops: bytes,
                    sig: hexa(&tx["ops_proofs"][0]["Ed25519Sig"], "inscription signature")?,
                    tx_hash: served,
                    fork_digest,
                };
                let payload = ins.payload()?;
                ensure!(payload.len() >= 8, "short payload");
                let id = u64::from_le_bytes(payload[..8].try_into()?);
                ensure!(out.insert(id, ins).is_none(), "LEZ block {id} inscribed twice");
            }
        }
    }
    Ok((out, node_blocks, other))
}

// ---- the burns the checkpoint counts (checkpoint-core rule.rs `extend` / `apply_message`) -------------

/// Calls to the bridge program executed by a public actor, with their transaction and origin.
struct Calls<'p, 'a> {
    program: &'p Hash,
    calls: Vec<(u32, Origin, &'a [u8])>,
}

impl<'a> Visitor<'a> for Calls<'_, 'a> {
    fn call(&mut self, index: u32, origin: Origin, _from: Option<Actor<'a>>, to: Actor<'a>, message: &'a [u8]) {
        if to.program_account_id == self.program && origin.is_public_execution() {
            self.calls.push((index, origin, message));
        }
    }
}

/// The identity file, T0, T1 and the session's (block, hash) pairs.
type Identity = (Value, u64, u64, Vec<(u64, Hash)>);

/// The run's configuration and the span: `--identity` + `--parent` + `--tip`, or the LEZ side's
/// `--session-a` (+ `--vault`, `--config-id`, `--genesis-state`) and `--session-b` (or `--tip`).
/// Returns the identity in `--identity`'s shape, T0, T1 and the tip hashes the session files name.
fn identity(a: &BTreeMap<String, String>) -> Result<Identity> {
    let read = |k: &str| -> Result<Value> {
        let p = a.get(k).with_context(|| format!("--{k} is required"))?;
        serde_json::from_slice(&fs::read(p).with_context(|| p.clone())?).with_context(|| p.clone())
    };
    let Some(_) = a.get("session-a") else {
        let id = read("identity")?;
        let t0: u64 = a.get("parent").context("--parent")?.parse()?;
        let t1: u64 = a.get("tip").context("--tip")?.parse()?;
        return Ok((id, t0, t1, Vec::new()));
    };
    let sa = read("session-a")?;
    let pick = |arg: &str, key: &str| -> Value {
        a.get(arg).map(|v| json!(v)).unwrap_or_else(|| sa.get(key).cloned().unwrap_or(Value::Null))
    };
    let id = json!({
        "label": sa.get("label"), "bedrockGenesisId": sa["bedrockGenesisId"], "channelId": sa["channel"]["id"],
        "bridgeHeader": sa["bridge"]["header"], "bridgeImageId": sa["bridge"]["imageId"], "chainId": sa["evmChainId"],
        "vault": pick("vault", "vault"), "attestationRootId": sa["attestationRootId"], "salt": sa["salt"],
        "nMin": a.get("n-min").map_or(Ok(1), |v| v.parse::<u64>())?,
        "tMin": a.get("t-min").map_or(Ok(1), |v| v.parse::<u64>())?,
        "configId": pick("config-id", "configId"),
        "genesisState": a.get("genesis-state").map(|v| json!(v)).unwrap_or(Value::Null),
        "parentKeys": sa["channel"]["keys"], "parentConfigTipHash": sa["channel"]["configTipHash"],
    });
    ensure!(!id["configId"].is_null(), "--config-id (or session-a.json configId) is required with --session-a");
    let t0 = sa["tip"]["id"].as_u64().context("session-a.json tip.id")?;
    let mut hashes = vec![(t0, hexa::<32>(&sa["tip"]["hash"], "session-a tip.hash")?)];
    let t1 = match a.get("session-b") {
        Some(_) => {
            let sb = read("session-b")?;
            ensure!(sb["tipT0"]["id"].as_u64() == Some(t0), "session-b.json tipT0 is not session-a.json's tip");
            let t1 = sb["tipT1"]["id"].as_u64().context("session-b.json tipT1.id")?;
            hashes.push((t1, hexa::<32>(&sb["tipT1"]["hash"], "session-b tipT1.hash")?));
            t1
        }
        None => a.get("tip").context("--tip or --session-b")?.parse()?,
    };
    Ok((id, t0, t1, hashes))
}

fn main() -> Result<()> {
    let a = args()?;
    let get = |k: &str| a.get(k).with_context(|| format!("--{k} is required"));
    let out_dir = PathBuf::from(get("out")?);
    fs::create_dir_all(&out_dir)?;
    let node = PathBuf::from(get("node")?);
    let channel_json: Value = serde_json::from_slice(&fs::read(node.join("channel.json"))?).context("channel.json")?;
    let info_path = if node.join("info.json").exists() { node.join("info.json") } else { node.join("info-start.json") };
    let info: Value = serde_json::from_slice(&fs::read(info_path)?).context("info.json")?;
    let lib_slot =
        info["lib_slot"].as_u64().or(info["cryptarchia_info"]["lib_slot"].as_u64()).context("info.json: lib_slot")?;
    let (id, t0, t1, tip_hashes) = identity(&a)?;
    ensure!(t0 >= 1 && t1 > t0, "need 1 <= T0 < T1");

    // ---- the run's configuration, checked against the node and the bridge wire's configId ------------
    let (ins, node_blocks, other_txs) = load_inscriptions(&node.join("blocks"), &CHANNEL_ID)?;
    // Logos Blockchain 0.3.5-rc.1: one fork digest on every transaction, and bedrockGenesisId is it.
    let fork_digest = ins.values().next().and_then(|i| i.fork_digest);
    ensure!(
        ins.values().all(|i| i.fork_digest == fork_digest),
        "the channel's inscriptions carry different fork digests (or some none)"
    );
    let bedrock_genesis_id = match fork_digest {
        Some(d) => {
            ensure!(
                d == hexa::<32>(&id["bedrockGenesisId"], "bedrockGenesisId")?,
                "identity.bedrockGenesisId is not the zone's fork digest {}",
                hx(&d)
            );
            println!("Logos Blockchain 0.3.5-rc.1 layout: every transaction carries fork digest {} (= bedrockGenesisId)", hx(&d));
            d
        }
        None => {
            let genesis_bytes = fs::read(get("genesis")?)?;
            let g = sha256_parts(&[genesis_bytes.as_slice()]);
            ensure!(
                g == hexa::<32>(&id["bedrockGenesisId"], "bedrockGenesisId")?,
                "identity.bedrockGenesisId is not the SHA-256 of these deployment settings"
            );
            g
        }
    };
    ensure!(hexa::<32>(&id["channelId"], "channelId")? == CHANNEL_ID, "identity.channelId is not 0x01…01");
    let chain_id = id["chainId"].as_u64().context("identity.chainId")?;
    let mut evm_chain_id = [0u8; 32];
    evm_chain_id[24..].copy_from_slice(&chain_id.to_be_bytes());
    let vault: Address = hexa(&id["vault"], "vault")?;
    let config = Config {
        bedrock_genesis_id,
        channel_id: CHANNEL_ID,
        bridge_header: hexa(&id["bridgeHeader"], "bridgeHeader")?,
        bridge_image_id: hexa(&id["bridgeImageId"], "bridgeImageId")?,
        evm_chain_id,
        vault,
        attestation_root_id: hexa(&id["attestationRootId"], "attestationRootId")?,
        salt: hexa(&id["salt"], "salt")?,
        n_min: u32::try_from(id["nMin"].as_u64().context("nMin")?)?,
        t_min: u32::try_from(id["tMin"].as_u64().context("tMin")?)?,
    };
    let config_id = config.config_id();
    // `"configId": null` only for a stand-in identity (a rehearsal): then nothing pins it to the wire's.
    let config_id_checked = !id["configId"].is_null();
    if config_id_checked {
        ensure!(
            config_id == hexa::<32>(&id["configId"], "configId")?,
            "checkpoint-core's configId {} is not identity.configId (the bridge wire's)",
            hx(&config_id)
        );
    } else {
        eprintln!(
            "WARNING: identity.configId is null: configId {} is checkpoint-core's alone (stand-in)",
            hx(&config_id)
        );
    }

    // ---- every inscription of the channel the node holds as final, checked by checkpoint-core --------
    let (first, last) = (*ins.keys().next().context("no inscriptions")?, *ins.keys().last().context("none")?);
    ensure!(
        ins.keys().copied().eq(first..=last),
        "the node's final inscriptions are not contiguous in {first}..={last}"
    );
    ensure!(first <= t0 && t1 <= last, "the node holds final inscriptions of blocks {first}..={last}, not {t0}..={t1}");
    // The key set: the channel's one key (the crown run), or every key the channel accredits (a zone, --att).
    let zone = a.contains_key("att");
    let channel_keys: Vec<Hash> = channel_json["accredited_keys"]
        .as_array()
        .context("channel.json: accredited_keys")?
        .iter()
        .map(|k| hexa::<32>(k, "accredited key"))
        .collect::<Result<_>>()?;
    ensure!(!channel_keys.is_empty(), "the channel accredits no key");
    ensure!(zone || channel_keys.len() == 1, "the channel holds more than one key (a zone: give --att)");
    let channel_key = channel_keys[0];
    let mut signed_by: BTreeMap<Hash, u32> = BTreeMap::new();
    // The parent's key set and configuration tip: session A's (a zone), else the channel's as the node reports it.
    let parent_keys: Vec<Hash> = match id.get("parentKeys").and_then(Value::as_array).filter(|_| zone) {
        Some(k) => k.iter().map(|x| hexa::<32>(x, "session-a channel key")).collect::<Result<_>>()?,
        None => channel_keys.clone(),
    };
    let parent_config_tip: Hash = match id.get("parentConfigTipHash").filter(|v| zone && !v.is_null()) {
        Some(v) => hexa(v, "session-a configTipHash")?,
        None => hexa(&channel_json["config_tip_hash"], "config_tip_hash")?,
    };
    // Configuration changes after T0's inscription: applied in order natively (the program does it again).
    let t0_slot = ins.get(&t0).context("no final inscription of T0")?.slot;
    let all_configs = if zone { load_configs(&node.join("blocks"), &CHANNEL_ID)? } else { Vec::new() };
    let configs: Vec<&Cfg> = all_configs.iter().filter(|c| c.slot > t0_slot).collect();
    let mut sets: Vec<(u64, Vec<Hash>)> = vec![(0, parent_keys.clone())];
    let mut cth = parent_config_tip;
    for c in &configs {
        let (op, h) = match fork_digest {
            None => c.change().op().map_err(|e| anyhow!("configuration tx {}: {e:?}", hx(&c.tx_hash)))?,
            Some(d) => {
                ensure!(c.fork_digest == Some(d), "configuration tx {}: another fork digest", hx(&c.tx_hash));
                bedrock_0_3_5::mantle::config_op(&c.change(), &d)
                    .map_err(|e| anyhow!("configuration tx {}: {e:?}", hx(&c.tx_hash)))?
            }
        };
        ensure!(h == c.tx_hash, "configuration tx: re-hash");
        ensure!(
            op.parent == &cth,
            "configuration tx {}: its parent is not the configuration tip before it",
            hx(&c.tx_hash)
        );
        cth = op.id();
        sets.push((c.slot, op.keys.iter().map(|k| **k).collect()));
        println!(
            "configuration change in node block {} (slot {}): {} keys, threshold {}, posting timeframe {}, signed by \
                  key indices {:?} of the set before it; new configuration tip {}",
            c.node_block,
            c.slot,
            op.keys.len(),
            op.configuration_threshold,
            op.posting_timeframe,
            c.sigs.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
            hx(&cth)
        );
    }
    let final_keys = sets.last().map(|(_, k)| k.clone()).context("key sets")?;
    if zone {
        ensure!(
            final_keys == channel_keys,
            "the key set after the configuration changes is not the channel's as the node reports it"
        );
        ensure!(
            cth == hexa::<32>(&channel_json["config_tip_hash"], "config_tip_hash")?,
            "the configuration tip after the changes is not the channel's"
        );
    }
    // The key set in force for an inscription at node slot `slot` (a change takes effect after its own node block).
    fn set_at(sets: &[(u64, Vec<Hash>)], slot: u64) -> &[Hash] {
        &sets.iter().rev().find(|(from, _)| *from < slot || *from == 0).expect("the parent's set").1
    }
    let mut record: Option<(Vec<u8>, usize)> = match a.get("record") {
        Some(d) => {
            let rec = fs::read(Path::new(d).join("blocks.borsh"))?;
            Some((rec, 4))
        }
        None => None,
    };
    let mut rows = vec!["block\thash\tinscribed_bytes\ttransactions\tstatus_inscribed\tnode_block\tnode_slot\ttx_hash\tmsg_id\tparent_msg_id\trecord".to_string()];
    let mut lines = Vec::new();
    let (mut prev_hash, mut prev_msg) = ([0u8; 32], [0u8; 32]);
    let mut msg_ids = BTreeMap::new();
    let mut record_equal = 0u32;
    for k in first..=last {
        let i = &ins[&k];
        let e = i.elided()?;
        let payload = i.payload()?;
        let s = parse_block(payload, &mut NoVisitor).map_err(|e| anyhow!("block {k}: parser refused it: {e:?}"))?;
        ensure!(s.header.block_id == k, "block {k}: id {}", s.header.block_id);
        let h = hash_parsed_block(payload).map_err(|e| anyhow!("{e:?}"))?;
        ensure!(h == s.header.hash, "block {k}: re-hash {} != carried {}", hex::encode(h), hex::encode(s.header.hash));
        ensure!(k == first || s.header.prev_block_hash == prev_hash, "block {k}: not linked to {}", k - 1);
        let c = match fork_digest {
            None => e.check(payload, &CHANNEL_ID).map_err(|e| anyhow!("block {k}: inscription refused: {e:?}"))?,
            Some(d) => bedrock_0_3_5::mantle::check(&e, payload, &CHANNEL_ID, &d)
                .map_err(|e| anyhow!("block {k}: inscription refused: {e:?}"))?,
        };
        ensure!(c.tx_hash == i.tx_hash, "block {k}: transaction hash");
        ensure!(k == first || c.parent == prev_msg, "block {k}: inscription parent is not block {}'s message", k - 1);
        ensure!(
            set_at(&sets, i.slot).contains(&c.signer),
            "block {k}: signer is not one of the channel's keys at node slot {}",
            i.slot
        );
        *signed_by.entry(c.signer).or_default() += 1;
        let msg = inscription_msg_id(&CHANNEL_ID, payload, &c.parent, &c.signer);
        // Optional: the same block as the sequencer served it (equal but the status byte).
        let mut rec_note = "-";
        if let Some((rec, off)) = record.as_mut() {
            if let Some(served) = rec.get(*off..*off + payload.len()) {
                if served[..served.len() - 1] == payload[..payload.len() - 1] && served[..8] == k.to_le_bytes() {
                    *off += payload.len();
                    record_equal += 1;
                    rec_note = "equal but status";
                } else {
                    rec_note = "differs";
                }
            }
        }
        let st = STATUS.get(s.status as usize).copied().unwrap_or("?");
        rows.push(format!(
            "{k}\t{}\t{}\t{}\t{st}\t{}\t{}\t{}\t{}\t{}\t{rec_note}",
            hex::encode(h),
            payload.len(),
            s.tx_count,
            i.node_block,
            i.slot,
            hex::encode(c.tx_hash),
            hex::encode(msg),
            hex::encode(c.parent)
        ));
        lines.push(
            json!({"block": k, "node_block": i.node_block, "slot": i.slot, "tx_hash": hex::encode(c.tx_hash),
            "signature": hex::encode(i.sig), "payload_bytes": payload.len(),
            "payload_sha256": hex::encode(sha256_parts(&[payload])), "msg_id": hex::encode(msg)})
            .to_string(),
        );
        msg_ids.insert(k, msg);
        prev_hash = h;
        prev_msg = msg;
    }
    fs::write(out_dir.join("blocks.tsv"), rows.join("\n") + "\n")?;
    fs::write(out_dir.join("inscriptions.jsonl"), lines.join("\n") + "\n")?;
    let channel_tip: Hash = hexa(&channel_json["tip_message"], "tip_message")?;
    let by: Vec<String> = signed_by.iter().map(|(k, n)| format!("{}… {n}", &hex::encode(k)[..8])).collect();
    println!("node: {node_blocks} immutable blocks ({other_txs} other transactions), final inscriptions of LEZ blocks {first}..={last}: \
              parse + re-hash + link + framing + BLAKE2b + verify_strict by the channel's {} key(s) ({}): all {} accepted; \
              channel tip message = block {last}'s: {}",
        channel_keys.len(), by.join(", "), last - first + 1, channel_tip == prev_msg);
    if record.is_some() {
        println!("record: {record_equal} blocks equal to the inscribed ones but the status byte");
    }

    for (k, h) in &tip_hashes {
        ensure!(ins[k].payload()?[40..72] == h[..], "block {k}: its hash is not the one the session files name");
    }

    // ---- finality of T1 by the node's own rule ------------------------------------------------------
    let tip_ins = &ins[&t1];
    ensure!(tip_ins.slot <= lib_slot, "T1's inscription is in node block slot {} > LIB {lib_slot}", tip_ins.slot);

    // ---- the parent state at T0 and the attestor(s) ---------------------------------------------------
    let posting_timeframe = u32::try_from(channel_json["posting_timeframe"].as_u64().context("posting_timeframe")?)?;
    let config_tip_hash: Hash = parent_config_tip;
    let parent = State {
        tip_id: t0,
        tip_hash: ins[&t0].payload()?[40..72].try_into()?,
        keys: parent_keys.clone(),
        posting_timeframe,
        config_tip_hash,
        burns: Default::default(),
        imported_leaf_count: 0,
        sunset: false,
    };
    if let Some(g) = id.get("genesisState").filter(|v| !v.is_null()) {
        ensure!(
            parent.digest() == hexa::<32>(g, "genesisState")?,
            "the state at T0 {} is not identity.genesisState (the vault's)",
            hx(&parent.digest())
        );
    }
    let d = Deployment { evm_chain_id, vault, lez_config_id: config_id, channel_id: CHANNEL_ID };
    // Attestations are over the key set at T: after the configuration changes.
    let ksh = key_set_hash(&final_keys, parent.posting_timeframe);
    let t_payload = tip_ins.payload()?;
    let tip = FinalTip { tip_id: t1, tip_hash: t_payload[40..72].try_into()?, final_msg_id: msg_ids[&t1] };
    let (attestations, att_record) = if let Some(files) = a.get("att") {
        // A zone: the attestors' own records; each must verify for this deployment, key set and tip.
        let mut atts = Vec::new();
        let mut records = Vec::new();
        for f in files.split(',') {
            let v: Value =
                serde_json::from_slice(&fs::read(f).with_context(|| f.to_string())?).with_context(|| f.to_string())?;
            ensure!(
                v["version"].as_str() == Some("ferrata-attestation/v1"),
                "{f}: not a ferrata-attestation/v1 record"
            );
            let bytes: [u8; attestor_core::SIGNED_ATTESTATION_LEN] =
                hex::decode(v["encoded"].as_str().context("encoded")?)?
                    .try_into()
                    .map_err(|_| anyhow!("{f}: encoded: wrong length"))?;
            let att = SignedAttestation::decode(&bytes);
            ensure!(
                final_keys.contains(&att.channel_key),
                "{f}: channel key {} is not in the key set",
                hx(&att.channel_key)
            );
            ensure!(
                att.tip == tip,
                "{f}: attests tip {} {}, not T1 = {t1} {}",
                att.tip.tip_id,
                hx(&att.tip.tip_hash),
                hx(&tip.tip_hash)
            );
            att.verify(&d, &ksh).map_err(|e| anyhow!("{f}: the attestation does not verify: {e:?}"))?;
            records.push(json!({"file": Path::new(f).file_name().map(|n| n.to_string_lossy().to_string()),
                "channel_key": hx(&att.channel_key), "attestation_key": hx(&att.att_key), "certificate": hx(&att.certificate),
                "signature": hx(&att.signature), "tip_id": att.tip.tip_id}));
            atts.push(att);
        }
        let rec = json!({
            "what": "zone attestors: each sequencer's channel key certified one attestation key (ferrata_attestor certify); each attestor read the node's final tier and signed the final-tier message v2 for T1 (ferrata_attestor attest); the records were checked here for this deployment, key set and tip",
            "key_set": channel_keys.iter().map(|k| hx(k)).collect::<Vec<_>>(), "n": channel_keys.len(),
            "t": threshold(u32::try_from(channel_keys.len())?), "attestations": records,
            "key_set_hash": hx(&ksh), "posting_timeframe": posting_timeframe, "config_tip_hash": hx(&config_tip_hash),
            "tip_id": t1, "tip_hash": hx(&tip.tip_hash), "final_msg_id": hx(&tip.final_msg_id),
            "tip_node_block": tip_ins.node_block, "tip_node_slot": tip_ins.slot, "lib_slot": lib_slot,
            "channel_tip_is_last_final_block": channel_tip == prev_msg, "last_final_block": last,
            "deployment": {"evm_chain_id": chain_id, "vault": hx(&vault), "lez_config_id": hx(&config_id), "channel_id": hx(&CHANNEL_ID)},
        });
        (atts, rec)
    } else {
        let channel_sk_bytes: [u8; 32] =
            fs::read(get("channel-key")?)?.try_into().map_err(|_| anyhow!("channel key: not 32 bytes"))?;
        let channel_sk = SigningKey::from_bytes(&channel_sk_bytes);
        let channel_pk = channel_sk.verifying_key().to_bytes();
        ensure!(channel_pk == channel_key, "the channel key file is not the channel's accredited key");
        let att_sk = SigningKey::from_bytes(&sha256_parts(&[b"ferrata/crown/attestation-key/v1", &channel_sk_bytes]));
        let att_pk = att_sk.verifying_key().to_bytes();
        let certificate = sign_certificate(&channel_sk, &d, &att_pk);
        let attestation = SignedAttestation::new(&att_sk, channel_pk, certificate, &d, &ksh, tip);
        attestation.verify(&d, &ksh).map_err(|e| anyhow!("the attestation does not verify: {e:?}"))?;
        let rec = json!({
            "what": "crown attestor: the devnet sequencer's channel key certifies an attestation key (secret = sha256(\"ferrata/crown/attestation-key/v1\" || channel secret), not written); the attestation key signs the final-tier message v2 for T1, final by the node's rule (inscription in an immutable node block, slot <= LIB)",
            "channel_key": hx(&channel_pk), "attestation_key": hx(&att_pk),
            "certificate_message": hx(&d.certificate_message(&att_pk)), "certificate": hx(&certificate),
            "key_set_hash": hx(&ksh), "posting_timeframe": posting_timeframe, "config_tip_hash": hx(&config_tip_hash),
            "tip_id": t1, "tip_hash": hx(&tip.tip_hash), "final_msg_id": hx(&tip.final_msg_id),
            "tip_node_block": tip_ins.node_block, "tip_node_slot": tip_ins.slot, "lib_slot": lib_slot,
            "tip_status_inscribed": STATUS.get(parse_block(t_payload, &mut NoVisitor).map_err(|e| anyhow!("{e:?}"))?.status as usize),
            "channel_tip_is_last_final_block": channel_tip == prev_msg, "last_final_block": last,
            "signature": hx(&attestation.signature),
            "deployment": {"evm_chain_id": chain_id, "vault": hx(&vault), "lez_config_id": hx(&config_id), "channel_id": hx(&CHANNEL_ID)},
        });
        (vec![attestation], rec)
    };
    let n_keys = u32::try_from(channel_keys.len())?;

    // ---- the input, the guest's rule run natively, and the burns it counts -----------------------------
    let blocks: Vec<&[u8]> = ((t0 + 1)..=t1).map(|k| ins[&k].payload()).collect::<Result<_>>()?;
    let input = CheckpointInputB {
        config: config.clone(),
        parent: parent.clone(),
        config_changes: configs.iter().map(|c| c.change()).collect(),
        blocks: blocks.clone(),
        t_index: u32::try_from(t1 - t0 - 1)?,
        producer: tip_ins.elided()?,
        attestations: attestations.clone(),
        segments: Vec::new(),
    };
    let outcome = match fork_digest {
        None => run_checkpoint_b(&input).map_err(|e| anyhow!("run_checkpoint_b refused the span: {e:?}"))?,
        Some(_) => bedrock_0_3_5::rule::run_checkpoint_b(&input)
            .map_err(|e| anyhow!("the 0.3.5-rc.1 build's run_checkpoint_b refused the span: {e:?}"))?,
    };
    let journal = outcome.journal.encode();
    let encoded = input.encode();
    ensure!(CheckpointInputB::decode(&encoded).is_ok(), "the input does not decode");

    let (mut frontier, mut burns, mut imports, mut sunset) = (Frontier::default(), Vec::new(), Vec::new(), false);
    let mut txs = 0u32;
    for (k, bytes) in ((t0 + 1)..=t1).zip(&blocks) {
        let mut v = Calls { program: &config.bridge_header, calls: Vec::new() };
        let s = parse_block(bytes, &mut v).map_err(|e| anyhow!("block {k}: {e:?}"))?;
        txs += s.tx_count;
        for (tx, origin, message) in v.calls {
            let reverted = s.is_reverted(tx);
            let Ok(Some(msg)) = BridgeMessage::decode(message) else { continue };
            if reverted || msg.config_id() != Some(&config_id) {
                continue;
            }
            match msg {
                BridgeMessage::Burn(b) => {
                    let index = frontier.count;
                    let leaf = burn_leaf(&config_id, index, &b.token, &b.amount, &b.c_out, &b.r);
                    frontier.append(leaf).map_err(|e| anyhow!("{e:?}"))?;
                    burns.push(json!({"burn_index": index, "block": k, "tx": tx, "origin": format!("{origin:?}"),
                        "r": hx(&b.r), "memo": hx(&b.memo), "mac": hx(&b.mac), "token": hx(&b.token),
                        "amount": hx(&b.amount), "c_out": hx(&b.c_out), "leaf": hx(&leaf)}));
                }
                BridgeMessage::ImportRoot(m) if !sunset => {
                    imports.push(json!({"block": k, "tx": tx, "leaf_count": m.leaf_count}))
                }
                BridgeMessage::Sunset { .. } => sunset = true,
                _ => {}
            }
        }
    }
    let zeros = zero_hashes();
    ensure!(frontier == outcome.new_state.burns, "the burns collected here are not the guest rule's frontier");
    ensure!(frontier.root(&zeros) == outcome.journal.burn_root, "burn root");
    let name = a.get("name").cloned().unwrap_or_else(|| format!("crown-{t0}-{t1}"));
    let bytes: usize = blocks.iter().map(|b| b.len()).sum();
    fs::write(out_dir.join(format!("{name}.input")), &encoded)?;
    let meta = json!({
        "kind": "checkpoint-b",
        "description": if zone {
            format!("zone: rule (B) over the zone's blocks {}..={t1} (parent tip {t0}), T = {t1}, {} certified attestations of n = {n_keys} (t = {})",
                t0 + 1, attestations.len(), threshold(n_keys))
        } else {
            format!("crown run: rule (B) over the devnet's blocks {}..={t1} (parent tip {t0}), T = {t1}, one certified attestation (n = 1, t = 1)", t0 + 1)
        },
        "journal": hex::encode(journal),
        "journal_sha256": hex::encode(sha256_parts(&[journal.as_slice()])),
        "blocks": blocks.len(), "transactions": txs, "block_bytes": bytes, "input_bytes": encoded.len(),
        "first_block": t0 + 1, "tip": t1, "tip_hash": hx(&tip.tip_hash), "final_msg_id": hx(&tip.final_msg_id),
        "parent_state": hx(&parent.digest()), "new_state": hx(&outcome.journal.new_state), "n": n_keys, "t": threshold(n_keys),
        "attestations": attestations.len(), "n_min": config.n_min, "t_min": config.t_min,
        "parent_keys": parent_keys.iter().map(|k| hx(k)).collect::<Vec<_>>(),
        "config_changes": configs.iter().map(|c| json!({"node_block": c.node_block, "node_slot": c.slot, "tx_hash": hx(&c.tx_hash),
            "op_index": c.op_index, "signer_indices": c.sigs.iter().map(|(i, _)| *i).collect::<Vec<_>>()})).collect::<Vec<_>>(),
        "keys_after": final_keys.iter().map(|k| hx(k)).collect::<Vec<_>>(),
        "burn_root": hx(&outcome.journal.burn_root), "burn_count": outcome.journal.burn_count,
        "imported_leaf_count": outcome.journal.imported_leaf_count, "sunset": outcome.journal.sunset,
        "lez_config_id": hx(&config_id), "config_id_checked_against_identity": config_id_checked,
        "identity_label": id.get("label"), "evm_chain_id": chain_id, "vault": hx(&vault),
    });
    fs::write(out_dir.join(format!("{name}.json")), serde_json::to_string_pretty(&meta)? + "\n")?;
    fs::write(out_dir.join("attestor.json"), serde_json::to_string_pretty(&att_record)? + "\n")?;
    let burns_json = json!({"config_id": hx(&config_id), "parent_tip": t0, "tip": t1, "burn_count": frontier.count,
        "burn_root": hx(&frontier.root(&zeros)), "burns": burns, "imports": imports, "sunset": sunset});
    fs::write(out_dir.join("burns.json"), serde_json::to_string_pretty(&burns_json)? + "\n")?;
    println!("{name}: {} blocks, {txs} transactions, {bytes} B of blocks, input {} B; burns {} (root {}), imported leaf count {}, \
              sunset {}; journal sha256 {}; parent state {}; new state {}; configId {}",
        blocks.len(), encoded.len(), frontier.count, hx(&frontier.root(&zeros)), outcome.journal.imported_leaf_count,
        outcome.journal.sunset, hex::encode(sha256_parts(&[journal.as_slice()])), hx(&parent.digest()),
        hx(&outcome.journal.new_state), hx(&config_id));
    Ok(())
}
