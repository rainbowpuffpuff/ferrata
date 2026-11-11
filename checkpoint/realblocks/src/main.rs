//! `realblocks`: the checkpoint program's own parser, block re-hashing and inscription check over the
//! REAL blocks of the lab's LEZ v0.4.0-rc1 sequencer chain (`lez/lab`), and the rule (B) input over a
//! span of them, attested by a key the lab sequencer's channel key certified.
//!
//! Sources (all read-only copies; the running lab is never touched):
//! - `--node-blocks DIR`: the immutable (final) blocks of the lab's Logos Blockchain node, as its
//!   `/cryptarchia/blocks` API serves them from a copy of a saved generation (`extract.sh`);
//! - `--channel FILE`: the node's `/channel/<id>` answer from the same copy;
//! - `--record DIR`: the lab's record (`lez/lab/results/record`: `blocks.borsh`, `chain.tsv`,
//!   `segments.tsv`), the blocks as the sequencer's RPC served them;
//! - `--txhashes FILE`: the inscription transaction hashes the sequencer logged (`txhashes.py`);
//! - `--channel-key FILE`: the 32-byte channel signing key from the copied sequencer home;
//! - `--genesis FILE`: the node's deployment settings (their SHA-256 is the configuration's
//!   `bedrockGenesisId`).
//!
//! Every block the node holds an inscription of is parsed strictly and re-hashed by
//! `checkpoint-core` exactly as the guest does, and every inscription is framed, hashed and
//! signature-checked by the same code. Then `--from A --to B` builds the rule (B) input: parent tip
//! `A - 1`, blocks `A..=B` as inscribed, `T = B`, one attestation (n = 1, t = 1).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, ensure, Context as _, Result};
use attestor_core::{sign_certificate, Deployment, FinalTip, SignedAttestation};
use checkpoint_core::hash::{sha256_parts, threshold};
use checkpoint_core::input::CheckpointInputB;
use checkpoint_core::lez::{hash_parsed_block, parse_block, NoVisitor};
use checkpoint_core::mantle::{inscription_msg_id, tx_hash, ElidedInscription, OP_CHANNEL_INSCRIBE, OP_TRANSFER};
use checkpoint_core::rule_b::run_checkpoint_b;
use checkpoint_core::state::{key_set_hash, Config, State};
use checkpoint_core::{Address, Hash};
use ed25519_dalek::SigningKey;
use serde_json::{json, Value};

const STATUS: [&str; 3] = ["Pending", "Safe", "Finalized"];

fn args() -> Result<BTreeMap<String, String>> {
    let v: Vec<String> = std::env::args().skip(1).collect();
    ensure!(v.len().is_multiple_of(2), "arguments come in --key value pairs");
    v.chunks(2)
        .map(|p| Ok((p[0].strip_prefix("--").context("expected --key")?.to_string(), p[1].clone())))
        .collect()
}

fn hexa<const N: usize>(v: &Value, what: &str) -> Result<[u8; N]> {
    let b = hex::decode(v.as_str().with_context(|| format!("{what}: not a string"))?)?;
    b.try_into().map_err(|_| anyhow!("{what}: not {N} bytes"))
}

/// The canonical encoding of a transaction's ops (`checkpoint-core::mantle`'s module doc), from the
/// node's JSON. `reverse_fr`: whether 32-byte field elements (note ids, public keys of outputs) are
/// served in the opposite byte order to the wire's; the transaction hash decides which is right.
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
            c => bail!("opcode {c:#04x}: not one the lab's sequencer was expected to send"),
        }
    }
    Ok(out)
}

/// One inscription of a LEZ block, as the node holds it.
struct Ins {
    node_block: String,
    slot: u64,
    ops: Vec<u8>,
    sig: [u8; 64],
    tx_hash: Hash,
    field_order: &'static str,
}

impl Ins {
    fn elided(&self) -> Result<ElidedInscription<'_>> {
        ElidedInscription::split(&self.ops, 0, &self.sig).map_err(|e| anyhow!("not an inscription: {e:?}"))
    }
    fn payload(&self) -> Result<&[u8]> {
        let e = self.elided()?;
        Ok(&self.ops[e.prefix.len()..self.ops.len() - e.suffix.len()])
    }
}

fn load_inscriptions(dir: &Path, channel: &Hash) -> Result<(BTreeMap<u64, Ins>, usize)> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)?.map(|e| Ok(e?.path())).collect::<Result<_>>()?;
    files.sort_by_key(|p| p.file_name().and_then(|n| n.to_str()).and_then(|n| n.split('-').next()?.parse::<u64>().ok()));
    let (mut out, mut node_blocks) = (BTreeMap::new(), 0usize);
    for f in files {
        let blocks: Vec<Value> = serde_json::from_slice(&fs::read(&f)?).with_context(|| format!("{}", f.display()))?;
        for b in blocks {
            node_blocks += 1;
            for tx in b["transactions"].as_array().context("transactions")? {
                let ops = tx["mantle_tx"]["ops"].as_array().context("ops")?;
                let served: Hash = hexa(&tx["mantle_tx"]["hash"], "tx hash")?;
                ensure!(ops.first().and_then(|o| o["opcode"].as_u64()) == Some(u64::from(OP_CHANNEL_INSCRIBE)),
                    "node block {}: a transaction whose first op is not an inscription", b["header"]["id"]);
                ensure!(hexa::<32>(&ops[0]["payload"]["channel_id"], "channel")? == *channel, "another channel");
                // The byte order of field elements: whichever reproduces the node's transaction hash.
                let mut found = None;
                for (rev, name) in [(false, "as served"), (true, "reversed")] {
                    let bytes = encode_ops(ops, rev)?;
                    if tx_hash(&bytes) == served {
                        found = Some((bytes, name));
                        break;
                    }
                }
                let (bytes, field_order) = found.with_context(|| format!("tx {}: no encoding reproduces its hash", hex::encode(served)))?;
                let sig: [u8; 64] = hexa(&tx["ops_proofs"][0]["Ed25519Sig"], "inscription signature")?;
                let ins = Ins {
                    node_block: b["header"]["id"].as_str().context("header id")?.to_string(),
                    slot: b["header"]["slot"].as_u64().context("slot")?,
                    ops: bytes,
                    sig,
                    tx_hash: served,
                    field_order,
                };
                let payload = ins.payload()?;
                ensure!(payload.len() >= 8, "short payload");
                let id = u64::from_le_bytes(payload[..8].try_into()?);
                ensure!(out.insert(id, ins).is_none(), "LEZ block {id} inscribed twice");
            }
        }
    }
    Ok((out, node_blocks))
}

fn tsv(path: &Path) -> Result<Vec<Vec<String>>> {
    Ok(fs::read_to_string(path)?.lines().skip(1).map(|l| l.split('\t').map(str::to_string).collect()).collect())
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?.windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

fn main() -> Result<()> {
    let a = args()?;
    let get = |k: &str| a.get(k).with_context(|| format!("--{k} is required"));
    let out_dir = PathBuf::from(get("out")?);
    fs::create_dir_all(&out_dir)?;
    let channel_json: Value = serde_json::from_slice(&fs::read(get("channel")?)?)?;
    let channel_id: Hash = hexa(&json!("0101010101010101010101010101010101010101010101010101010101010101"), "id")?;

    // ---- every inscription the node holds, framed and checked by checkpoint-core ----------------
    let (ins, node_blocks) = load_inscriptions(Path::new(get("node-blocks")?), &channel_id)?;
    let logged: BTreeMap<String, String> =
        tsv(Path::new(get("txhashes")?))?.into_iter().map(|r| (r[2].clone(), r[0].clone())).collect();
    // The header line of txhashes.tsv is data (it has none): put it back.
    let logged_all: Vec<String> = fs::read_to_string(get("txhashes")?)?.lines().map(|l| l.split('\t').nth(2).unwrap_or("").to_string()).collect();
    let chain = tsv(&Path::new(get("record")?).join("chain.tsv"))?;
    let segments = tsv(&Path::new(get("record")?).join("segments.tsv"))?;
    let rec = fs::read(Path::new(get("record")?).join("blocks.borsh"))?;
    let n = u64::try_from(chain.len())?;
    ensure!(u32::from_le_bytes(rec[..4].try_into()?) as u64 == n, "blocks.borsh holds another number of blocks");
    ensure!(ins.keys().copied().eq(1..=n), "the node holds inscriptions of blocks {:?}..{:?}, the record 1..={n}",
        ins.keys().next(), ins.keys().last());

    let mut rows = vec!["block\thash\tinscribed_bytes\ttransactions\tstatus_inscribed\tstatus_served\trecord\tnode_block\tnode_slot\ttx_hash\tlogged\tmsg_id\tparent_msg_id\tfield_order".to_string()];
    let mut lines = Vec::new();
    let (mut prev_hash, mut prev_msg, mut off) = ([0u8; 32], [0u8; 32], 4usize);
    let (mut equal, mut elided, mut statuses) = (0, 0, BTreeMap::<String, u32>::new());
    let mut msg_ids = BTreeMap::new();
    for k in 1..=n {
        let i = &ins[&k];
        let e = i.elided()?;
        let payload = i.payload()?;
        // The block as inscribed: strict parse, re-hash, id and link (the guest's steps 3).
        let s = parse_block(payload, &mut NoVisitor).map_err(|e| anyhow!("block {k}: parser refused it: {e:?}"))?;
        ensure!(s.header.block_id == k, "block {k}: id {}", s.header.block_id);
        let h = hash_parsed_block(payload).map_err(|e| anyhow!("{e:?}"))?;
        ensure!(h == s.header.hash, "block {k}: re-hash {} != carried {}", hex::encode(h), hex::encode(s.header.hash));
        ensure!(k == 1 || s.header.prev_block_hash == prev_hash, "block {k}: not linked to {}", k - 1);
        ensure!(chain[(k - 1) as usize][0] == k.to_string() && chain[(k - 1) as usize][1] == hex::encode(h), "block {k}: chain.tsv");
        // The inscription: framing, BLAKE2b transaction hash, verify_strict by the signer it names.
        let c = e.check(payload, &channel_id).map_err(|e| anyhow!("block {k}: inscription refused: {e:?}"))?;
        ensure!(c.tx_hash == i.tx_hash, "block {k}: transaction hash");
        ensure!(k == 1 || c.parent == prev_msg, "block {k}: inscription parent is not block {}'s message", k - 1);
        let msg = inscription_msg_id(&channel_id, payload, &c.parent, &c.signer);
        // The record: the same block as the sequencer's RPC served it.
        let end = if k < n {
            let mut next = (k + 1).to_le_bytes().to_vec();
            next.extend(h);
            find(&rec, &next, off + 80).with_context(|| format!("record: block {} not found after {k}", k + 1))?
        } else {
            rec.len()
        };
        let served = &rec[off..end];
        off = end;
        let ss = parse_block(served, &mut NoVisitor).map_err(|e| anyhow!("record block {k}: {e:?}"))?;
        let chunks = segments.iter().filter(|r| r[0] == k.to_string()).count();
        let record = if chunks == 0 {
            ensure!(served.len() == payload.len() && served[..served.len() - 1] == payload[..payload.len() - 1],
                "block {k}: the served block differs from the inscribed one beyond the status byte");
            equal += 1;
            "equal but status"
        } else {
            ensure!(served[..80] == payload[..80], "block {k}: header differs");
            elided += 1;
            "bytecode elided in record"
        };
        let st = |x: u8| STATUS.get(x as usize).copied().unwrap_or("?");
        *statuses.entry(format!("{}->{}", st(s.status), st(ss.status))).or_default() += 1;
        let txh = hex::encode(c.tx_hash);
        let was_logged = if logged.contains_key(&txh) || logged_all.contains(&txh) { "yes" } else { "no" };
        rows.push(format!("{k}\t{}\t{}\t{}\t{}\t{}\t{record}\t{}\t{}\t{txh}\t{was_logged}\t{}\t{}\t{}",
            hex::encode(h), payload.len(), s.tx_count, st(s.status), st(ss.status), i.node_block, i.slot,
            hex::encode(msg), hex::encode(c.parent), i.field_order));
        lines.push(json!({"block": k, "node_block": i.node_block, "slot": i.slot, "tx_hash": txh,
            "prefix": hex::encode(e.prefix), "suffix": hex::encode(e.suffix), "signature": hex::encode(i.sig),
            "payload_bytes": payload.len(), "payload_sha256": hex::encode(sha256_parts(&[payload])), "msg_id": hex::encode(msg)}).to_string());
        ensure!(c.signer == hexa::<32>(&channel_json["accredited_keys"][0], "key")?, "block {k}: signer is not the channel's key");
        msg_ids.insert(k, msg);
        prev_hash = h;
        prev_msg = msg;
    }
    ensure!(off == rec.len(), "record: trailing bytes");
    let tip_message: Hash = hexa(&channel_json["tip_message"], "tip_message")?;
    ensure!(prev_msg == tip_message, "the channel's tip message is not block {n}'s inscription");
    fs::write(out_dir.join("blocks.tsv"), rows.join("\n") + "\n")?;
    fs::write(out_dir.join("inscriptions.jsonl"), lines.join("\n") + "\n")?;
    let logged_count = rows.iter().filter(|r| r.contains("\tyes\t")).count();
    println!("node: {node_blocks} immutable blocks, {} inscriptions of LEZ blocks 1..={n}", ins.len());
    println!("parser + re-hash + link (checkpoint-core): {n} of {n} blocks accepted; inscriptions framed, hashed, verify_strict by 3b14…: {n} of {n}");
    println!("record: {equal} blocks byte-equal to the inscribed ones but the status byte, {elided} with bytecode elided (header equal); status inscribed->served {statuses:?}");
    println!("inscription hashes in the sequencer's logs: {logged_count} of {n}; the channel's tip message = block {n}'s inscription");

    // ---- the rule (B) input over the span ---------------------------------------------------------
    let from: u64 = get("from")?.parse()?;
    let to: u64 = get("to")?.parse()?;
    ensure!(from >= 2 && from <= to && to <= n, "span");
    let parent_payload = ins[&(from - 1)].payload()?;
    let channel_sk_bytes: [u8; 32] = fs::read(get("channel-key")?)?.try_into().map_err(|_| anyhow!("channel key: not 32 bytes"))?;
    let channel_sk = SigningKey::from_bytes(&channel_sk_bytes);
    let channel_pk = channel_sk.verifying_key().to_bytes();
    ensure!(channel_pk == hexa::<32>(&channel_json["accredited_keys"][0], "key")?, "the copied channel key is not the channel's accredited key");
    ensure!(channel_json["accredited_keys"].as_array().map(Vec::len) == Some(1), "the channel holds more than one key");
    let posting_timeframe = u32::try_from(channel_json["posting_timeframe"].as_u64().context("posting_timeframe")?)?;
    let config_tip_hash: Hash = hexa(&channel_json["config_tip_hash"], "config_tip_hash")?;
    let label = |s: &str| sha256_parts(&[b"ferrata/realblocks/stand-in/", s.as_bytes()]);
    let mut evm_chain_id = [0u8; 32];
    evm_chain_id[24..].copy_from_slice(&31337u64.to_be_bytes());
    let vault: Address = label("vault")[..20].try_into()?;
    let config = Config {
        bedrock_genesis_id: sha256_parts(&[fs::read(get("genesis")?)?.as_slice()]),
        channel_id,
        bridge_header: label("bridge header"),
        bridge_image_id: label("bridge image id"),
        evm_chain_id,
        vault,
        attestation_root_id: label("attestation root id"),
        salt: label("salt"),
        n_min: 1,
        t_min: 1,
    };
    let parent = State {
        tip_id: from - 1,
        tip_hash: parent_payload[40..72].try_into()?,
        keys: vec![channel_pk],
        posting_timeframe,
        config_tip_hash,
        burns: Default::default(),
        imported_leaf_count: 0,
        sunset: false,
    };
    // The attestor: the channel key certifies an attestation key for this deployment; the attestation
    // key signs the final tier's tip T = `to`, final because the node holds its inscription in an
    // immutable block (the only blocks /cryptarchia/blocks serves) and the lab recorded it Finalized.
    ensure!(chain[(to - 1) as usize].last().map(String::as_str) == Some("Finalized"), "T not recorded Finalized");
    let att_sk = SigningKey::from_bytes(&sha256_parts(&[b"ferrata/realblocks/attestation-key/v1"]));
    let att_pk = att_sk.verifying_key().to_bytes();
    let d = Deployment { evm_chain_id, vault, lez_config_id: config.config_id(), channel_id };
    let certificate = sign_certificate(&channel_sk, &d, &att_pk);
    let ksh = key_set_hash(&parent.keys, parent.posting_timeframe);
    let t_payload = ins[&to].payload()?;
    let tip = FinalTip { tip_id: to, tip_hash: t_payload[40..72].try_into()?, final_msg_id: msg_ids[&to] };
    let attestation = SignedAttestation::new(&att_sk, channel_pk, certificate, &d, &ksh, tip);
    let blocks: Vec<&[u8]> = (from..=to).map(|k| ins[&k].payload()).collect::<Result<_>>()?;
    let input = CheckpointInputB {
        config: config.clone(),
        parent: parent.clone(),
        config_changes: Vec::new(),
        blocks: blocks.clone(),
        t_index: u32::try_from(to - from)?,
        attestations: vec![attestation],
        segments: Vec::new(),
    };
    let outcome = run_checkpoint_b(&input).map_err(|e| anyhow!("run_checkpoint_b refused the real span: {e:?}"))?;
    let journal = outcome.journal.encode();
    let encoded = input.encode();
    ensure!(CheckpointInputB::decode(&encoded).is_ok(), "the input does not decode");
    let name = format!("real-lab-{from}-{to}");
    let bytes: usize = blocks.iter().map(|b| b.len()).sum();
    let txs: u32 = blocks.iter().map(|b| parse_block(b, &mut NoVisitor).map(|s| s.tx_count)).sum::<Result<u32, _>>().map_err(|e| anyhow!("{e:?}"))?;
    fs::write(out_dir.join(format!("{name}.input")), &encoded)?;
    let meta = json!({
        "kind": "checkpoint-b",
        "description": format!("rule (B) over the lab's real LEZ blocks {from}..={to} (parent tip {}), T = {to}, one certified attestation (n = 1, t = 1)", from - 1),
        "journal": hex::encode(journal.as_slice()),
        "journal_sha256": hex::encode(sha256_parts(&[journal.as_slice()])),
        "blocks": blocks.len(), "transactions": txs, "block_bytes": bytes, "input_bytes": encoded.len(),
        "first_block": from, "tip": to, "tip_hash": hex::encode(tip.tip_hash), "final_msg_id": hex::encode(tip.final_msg_id),
        "parent_state": hex::encode(parent.digest()), "n": 1, "t": threshold(1),
        "lez_config_id": hex::encode(config.config_id()), "evm_chain_id": 31337, "vault": format!("0x{}", hex::encode(vault)),
    });
    fs::write(out_dir.join(format!("{name}.json")), serde_json::to_string_pretty(&meta)? + "\n")?;
    let att = json!({
        "what": "The attestor on the lab's data: the lab sequencer's channel key (throwaway devnet key) certifies an attestation key (derived from the public label ferrata/realblocks/attestation-key/v1, throwaway); the attestation key signs the final-tier message v2 for T.",
        "channel_key": hex::encode(channel_pk), "attestation_key": hex::encode(att_pk),
        "certificate_message": hex::encode(d.certificate_message(&att_pk)), "certificate": hex::encode(certificate),
        "key_set_hash": hex::encode(ksh), "posting_timeframe": posting_timeframe, "config_tip_hash": hex::encode(config_tip_hash),
        "tip_id": to, "tip_hash": hex::encode(tip.tip_hash), "final_msg_id": hex::encode(tip.final_msg_id),
        "tip_node_block": ins[&to].node_block, "tip_node_slot": ins[&to].slot, "tip_recorded": chain[(to - 1) as usize].last(),
        "signature": hex::encode(attestation.signature),
        "deployment": {"evm_chain_id": 31337, "vault": format!("0x{}", hex::encode(vault)), "lez_config_id": hex::encode(config.config_id()), "channel_id": hex::encode(channel_id)},
    });
    fs::write(out_dir.join("attestor.json"), serde_json::to_string_pretty(&att)? + "\n")?;
    println!("{name}: {} blocks, {txs} transactions, {bytes} B of blocks, input {} B; journal sha256 {}; parent state {}",
        blocks.len(), encoded.len(), hex::encode(sha256_parts(&[journal.as_slice()])), hex::encode(parent.digest()));
    Ok(())
}
