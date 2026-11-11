//! The zone side of CI's round trip (`e2e/ci/README.md`): a rule (B) checkpoint over blocks that
//! hold the transactions the LEZ harness executed (`cli/tests/roundtrip_b.rs`), in LEZ
//! v0.4.0-rc1's block format, inscribed with logos-blockchain-core d480efc6 by test sequencer keys.
//!
//! ```text
//! ci_zone genesis --identity <identity.json>
//!     the deployment the vault is built with: configId and genesisState (stdout, JSON)
//! ci_zone checkpoint --identity <identity.json> --txs <txs.json> --out-dir <dir>
//!     <dir>/cp-ci-b.input   the rule (B) checkpoint guest's input
//!     <dir>/cp-ci-b.json    the journal the rule gives natively on those bytes, and its fields
//! ```
//!
//! `identity.json`: `{"evm_chain_id": n, "vault": "0x…20", "bedrock_genesis_id", "channel_id",
//! "bridge_header", "bridge_image_id", "attestation_root_id", "salt"}` (each 0x + 32 bytes): the
//! eight values of configId's preimage. `txs.json`: `{"blocks": [["0x" + borsh(LeeTransaction),
//! …], …]}`, one array per block, in order.
//!
//! The chain: genesis (block 1), one block per entry of `blocks` (real format; the transactions
//! are the harness's own bytes, not executed again here), then `T`, an empty block. n = 4
//! sequencer keys (`nMin` = 3, `tMin` = 2), each block inscribed by key `id mod 4`; t = 3
//! attestation keys, each certified by one sequencer key, attest `T` (rule (B), D13). STAND-IN: no
//! Bedrock node and no sequencer ran; the keys are test keys. What is real: the block and
//! inscription formats, the transactions LEZ's state machine executed, and the checkpoint rule.
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{bail, ensure, Context as _, Result};
use attestor_core::{sign_certificate, Deployment as Attested, FinalTip, SignedAttestation};
use checkpoint_core::hash::{sha256_parts, threshold};
use checkpoint_core::input::CheckpointInputB;
use checkpoint_core::mantle::inscription_msg_id;
use checkpoint_core::rule_b::run_checkpoint_b;
use checkpoint_core::state::{key_set_hash, State};
use checkpoint_core::tree::zero_hashes;
use checkpoint_core::Hash;
use common::transaction::LeeTransaction;
use ed25519_dalek::SigningKey;
use fidelity::lez_build::Chain;
use fidelity::scenario::{checkpoint_input, Deployment, Inscribed};
use serde_json::{json, Value};

/// The posting timeframe of the stand-in channel, in slots (as `e2e deployment`'s default).
const POSTING_TIMEFRAME: u32 = 900;
const IDENTITY: [&str; 6] =
    ["bedrock_genesis_id", "channel_id", "bridge_header", "bridge_image_id", "attestation_root_id", "salt"];

fn hx(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

fn unhex(text: &str) -> Result<Vec<u8>> {
    hex::decode(text.trim_start_matches("0x")).with_context(|| format!("not hex: {text}"))
}

fn field<const N: usize>(v: &Value, key: &str) -> Result<[u8; N]> {
    let s = v[key].as_str().with_context(|| format!("missing {key}"))?;
    unhex(s)?.try_into().map_err(|_| anyhow::anyhow!("{key}: not {N} bytes"))
}

fn options(args: &[String]) -> Result<BTreeMap<String, String>> {
    let mut opts = BTreeMap::new();
    let mut it = args.iter();
    while let Some(k) = it.next() {
        let key = k.strip_prefix("--").with_context(|| format!("unexpected argument {k}"))?;
        opts.insert(key.to_string(), it.next().with_context(|| format!("--{key} wants a value"))?.clone());
    }
    Ok(opts)
}

fn read_json(path: &str) -> Result<Value> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("reading {path}"))?)
        .with_context(|| path.to_string())
}

/// The test deployment (n = 4, `nMin` = 3, `tMin` = 2) with the identity file's eight values.
fn deployment(identity: &Value) -> Result<Deployment> {
    let mut dep = Deployment::new(4, 3, 2);
    let chain_id = identity["evm_chain_id"].as_u64().context("evm_chain_id")?;
    dep.config.evm_chain_id = [0u8; 32];
    dep.config.evm_chain_id[24..].copy_from_slice(&chain_id.to_be_bytes());
    dep.config.vault = field(identity, "vault")?;
    let c = &mut dep.config;
    for key in IDENTITY {
        let value: Hash = field(identity, key)?;
        match key {
            "bedrock_genesis_id" => c.bedrock_genesis_id = value,
            "channel_id" => c.channel_id = value,
            "bridge_header" => c.bridge_header = value,
            "bridge_image_id" => c.bridge_image_id = value,
            "attestation_root_id" => c.attestation_root_id = value,
            "salt" => c.salt = value,
            _ => unreachable!(),
        }
    }
    Ok(dep)
}

fn genesis(dep: &Deployment, chain: &Chain) -> State {
    let mut state = dep.genesis_state(chain);
    state.posting_timeframe = POSTING_TIMEFRAME;
    state
}

/// Sequencer key `i` as ed25519-dalek holds it: the secret `fidelity::bedrock::Sequencer::from_seed`
/// gives logos-blockchain's key (checked against the deployment's public keys before use).
fn channel_key(dep: &Deployment, i: usize) -> Result<SigningKey> {
    let mut bytes = [0u8; 32];
    bytes[..8].copy_from_slice(&(i as u64).to_le_bytes());
    bytes[8..16].copy_from_slice(b"ferrata!");
    let key = SigningKey::from_bytes(&bytes);
    ensure!(key.verifying_key().to_bytes() == dep.keys()[i], "sequencer key {i}: not the deployment's");
    Ok(key)
}

/// Attestation key `i` (test key; certified by sequencer key `i`).
fn attestation_key(i: u32) -> SigningKey {
    SigningKey::from_bytes(&sha256_parts(&[b"ferrata/ci/attestation-key/", &i.to_be_bytes()]))
}

fn cmd_genesis(opts: &BTreeMap<String, String>) -> Result<()> {
    let dep = deployment(&read_json(opts.get("identity").context("--identity")?)?)?;
    let chain = Chain::genesis()?;
    let state = genesis(&dep, &chain);
    let n = u32::try_from(state.keys.len())?;
    let out = json!({
        "configId": hx(&dep.config_id()),
        "genesisState": hx(&state.digest()),
        "nMin": dep.config.n_min,
        "tMin": dep.config.t_min,
        "n": n,
        "t": threshold(n),
        "genesisBlock": {"id": chain.blocks[0].header.block_id, "hash": hx(&chain.blocks[0].header.hash.0)},
    });
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

fn cmd_checkpoint(opts: &BTreeMap<String, String>) -> Result<()> {
    let dep = deployment(&read_json(opts.get("identity").context("--identity")?)?)?;
    let txs = read_json(opts.get("txs").context("--txs")?)?;
    let dir = PathBuf::from(opts.get("out-dir").context("--out-dir")?);
    fs::create_dir_all(&dir)?;

    let mut chain = Chain::genesis()?;
    let parent = genesis(&dep, &chain);
    let mut carried = 0usize;
    for block in txs["blocks"].as_array().context("blocks")? {
        let mut list = Vec::new();
        for t in block.as_array().context("a block: an array of transactions")? {
            let bytes = unhex(t.as_str().context("a transaction: hex")?)?;
            list.push(borsh::from_slice::<LeeTransaction>(&bytes).context("borsh(LeeTransaction)")?);
        }
        carried += list.len();
        chain.push_stand_in(list, Vec::new())?;
    }
    ensure!(carried > 0, "no transactions");
    chain.push_valid(Vec::new())?; // T
    let t = chain.blocks.len() - 1;
    let ins = Inscribed::new(&dep, &chain);
    let owned = checkpoint_input(&dep, &ins, parent.clone(), 0, t, t, t, [0; 32], 0);
    let view = owned.view();

    // Rule (B): t certified attestations, all on T (as `host gen-b`'s rb-tonly fixtures).
    let n = u32::try_from(parent.keys.len())?;
    let threshold_t = threshold(n);
    let config = &view.config;
    let d = Attested {
        evm_chain_id: config.evm_chain_id,
        vault: config.vault,
        lez_config_id: config.config_id(),
        channel_id: config.channel_id,
    };
    let ksh = key_set_hash(&parent.keys, parent.posting_timeframe);
    let t_block = view.blocks[view.t_index as usize];
    let producer = view.producer;
    let parent_msg: Hash = producer.suffix[..32].try_into()?;
    let signer: Hash = producer.suffix[32..64].try_into()?;
    let tip = FinalTip {
        tip_id: u64::from_le_bytes(t_block[..8].try_into()?),
        tip_hash: t_block[40..72].try_into()?,
        final_msg_id: inscription_msg_id(&config.channel_id, t_block, &parent_msg, &signer),
    };
    let mut attestations = Vec::new();
    for i in 0..threshold_t {
        let ch = channel_key(&dep, i as usize)?;
        let at = attestation_key(i);
        let cert = sign_certificate(&ch, &d, &at.verifying_key().to_bytes());
        attestations.push(SignedAttestation::new(&at, ch.verifying_key().to_bytes(), cert, &d, &ksh, tip));
    }
    let b = CheckpointInputB {
        config: config.clone(),
        parent: parent.clone(),
        config_changes: Vec::new(),
        blocks: view.blocks.clone(),
        t_index: view.t_index,
        producer,
        attestations,
        segments: Vec::new(),
    };
    let bytes = b.encode();
    let decoded = CheckpointInputB::decode(&bytes).map_err(|e| anyhow::anyhow!("decode: {e:?}"))?;
    let out = run_checkpoint_b(&decoded).map_err(|e| anyhow::anyhow!("rule (B): {e:?}"))?;
    let j = &out.journal;
    if j.burn_count == 0 {
        bail!("the checkpoint collected no burn from the given transactions");
    }
    let meta = json!({
        "kind": "checkpoint-b",
        "description": format!("CI round trip: rule (B) over {} block(s) of harness transactions ({carried}) and T, n = {n}, t = {threshold_t}", t - 1),
        "journal": hx(&j.encode()),
        "evmChainId": hx(&j.evm_chain_id),
        "vault": hx(&j.vault),
        "lezConfigId": hx(&j.lez_config_id),
        "nMin": j.n_min,
        "tMin": j.t_min,
        "parentState": hx(&j.parent_state),
        "newState": hx(&j.new_state),
        "burnRoot": hx(&j.burn_root),
        "burnCount": j.burn_count,
        "importedLeafCount": j.imported_leaf_count,
        "sunset": j.sunset,
        "echoNonce": hx(&j.echo_nonce),
        "tipId": j.tip_id,
        "tipHash": hx(&j.tip_hash),
        "n": j.n,
        "t": j.t,
        "attestors": out.attestors,
        "burnRootCheck": hx(&out.new_state.burns.root(&zero_hashes())),
        "blocks": t,
        "inputBytes": bytes.len(),
    });
    fs::write(dir.join("cp-ci-b.input"), &bytes)?;
    fs::write(dir.join("cp-ci-b.json"), serde_json::to_string_pretty(&meta)? + "\n")?;
    println!("{}", serde_json::to_string(&meta)?);
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (cmd, rest) = args.split_first().context("ci_zone genesis | checkpoint")?;
    let opts = options(rest)?;
    match cmd.as_str() {
        "genesis" => cmd_genesis(&opts),
        "checkpoint" => cmd_checkpoint(&opts),
        other => bail!("unknown command {other}"),
    }
}
