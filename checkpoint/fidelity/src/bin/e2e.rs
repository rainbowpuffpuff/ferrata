//! The LEZ side of the end-to-end return-leg fixture (`e2e/` at the repository root): a stand-in
//! for a zone and its Logos Blockchain channel, built like the bench fixtures (`gen.rs`) with LEZ
//! v0.4.0-rc1's crates and logos-blockchain-core d480efc6, with test sequencer keys.
//!
//! ```text
//! e2e deployment --chain-id <n> --vault <0x…> [--posting-timeframe <slots>] --out <deployment.json>
//!     the configuration a vault is paired with: the preimage of configId, the sequencer keys, the
//!     genesis block and the genesis checkpoint state S (the vault's `genesisState`)
//! e2e chain --deployment <deployment.json> --wallet <wallet.json> [--echo-nonce <0x…>] --out-dir <dir>
//!     the blocks, their inscriptions and the checkpoint inputs:
//!       <dir>/cp-e2e-candidate.{input,json}   candidate checkpoint (echoNonce = 0)
//!       <dir>/cp-e2e-finalize.{input,json}    finalisation checkpoint (with --echo-nonce)
//!       <dir>/lez.json                        what an observer of the zone reads: every block's
//!                                             id, hash and messages to the bridge, the state at T
//! ```
//!
//! The chain (block id = position + 1, producer = id mod 4, n = 4 keys, t = 3):
//!
//! | block | contents |
//! |---|---|
//! | 1 | genesis |
//! | 2 | `ImportRoot` with the vault's deposit root and leaf count (from the wallet file) |
//! | 3 | burns 0 and 1 of the wallet file |
//! | 4 | a burn of another configuration (ignored), burn 2, a burn in a reverted transaction (ignored) |
//! | 5 | the remaining burns |
//! | 6 | empty: `T` |
//! | 7–9 | empty: the candidate's support (the three keys other than `T`'s producer) |
//! | 10–11 | empty (a real zone produces about W / 10 s blocks here) |
//! | 12 | `Echo{nonce}`: `Y` |
//! | 13–16 | empty: the finalisation's support (13, 15, 16; 14 is by `T`'s producer) |
//!
//! Blocks 10–16 exist only with `--echo-nonce`; blocks 1–9 are the same bytes either way, so the
//! candidate proven before the beacon is a prefix of what the finalisation proves. LEZ signs
//! transactions with fresh randomness, so two builds of the same zone differ in every block hash:
//! when `<dir>` already holds a candidate input, its blocks 2–5 are taken from it (their
//! transactions as they were signed then) and the rebuilt blocks 2–9 must equal the stored bytes.
//!
//! Stand-in, as everywhere in this prototype: no LEZ node and no bridge program run. Blocks that
//! carry calls to the bridge are real-format but not executed (`Chain::push_stand_in`).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context as _, Result};
use checkpoint_core::input::CheckpointInput;
use checkpoint_core::lez::{parse_block, Actor as CoreActor, Origin, Visitor};
use checkpoint_core::rule::{run_checkpoint, Outcome};
use checkpoint_core::state::{key_set_hash, State};
use checkpoint_core::tree::zero_hashes;
use checkpoint_core::wire::{BridgeMessage, Burn, ImportRoot};
use checkpoint_core::Hash;
use common::block::Block;
use common::transaction::LeeTransaction;
use fidelity::lez_build::{private_key, public_call, Chain};
use fidelity::scenario::{checkpoint_input, label, Deployment, Inscribed, OwnedInput};
use serde_json::{json, Value};

/// Position of `T` in the chain (block 6) and of the last candidate support block (block 9).
const T: usize = 5;
const CANDIDATE_END: usize = 8;
/// Position of the echo block `Y` (block 12) and of the last finalisation support block (block 16).
const Y: usize = 11;
const FINALIZE_END: usize = 15;

fn hx(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

fn unhex<const N: usize>(text: &str) -> Result<[u8; N]> {
    let bytes = hex::decode(text.trim_start_matches("0x")).with_context(|| format!("hex: {text}"))?;
    bytes.try_into().map_err(|b: Vec<u8>| anyhow::anyhow!("expected {N} bytes, got {}: {text}", b.len()))
}

fn field<const N: usize>(v: &Value, key: &str) -> Result<[u8; N]> {
    unhex(v[key].as_str().with_context(|| format!("missing \"{key}\""))?)
}

/// `--name value` pairs after the subcommand.
fn options(args: &[String]) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    let mut it = args.iter();
    while let Some(name) = it.next() {
        let name = name.strip_prefix("--").with_context(|| format!("unexpected argument {name}"))?;
        out.insert(name.to_string(), it.next().with_context(|| format!("--{name} needs a value"))?.clone());
    }
    Ok(out)
}

fn required<'a>(opts: &'a BTreeMap<String, String>, name: &str) -> Result<&'a str> {
    opts.get(name).map(String::as_str).with_context(|| format!("--{name} is required"))
}

/// The test deployment (n = 4 keys, `nMin` = 3, `tMin` = 2) for one vault on one chain.
fn deployment(chain_id: u64, vault: [u8; 20]) -> Deployment {
    let mut dep = Deployment::new(4, 3, 2);
    dep.config.evm_chain_id = [0u8; 32];
    dep.config.evm_chain_id[24..].copy_from_slice(&chain_id.to_be_bytes());
    dep.config.vault = vault;
    dep
}

/// The state at the genesis block, with the channel's posting timeframe as given.
fn genesis_state(dep: &Deployment, chain: &Chain, posting_timeframe: u32) -> State {
    let mut state = dep.genesis_state(chain);
    state.posting_timeframe = posting_timeframe;
    state
}

fn state_json(state: &State) -> Value {
    json!({
        "tip_id": state.tip_id,
        "tip_hash": hx(&state.tip_hash),
        "keys": state.keys.iter().map(|k| hx(k)).collect::<Vec<_>>(),
        "posting_timeframe": state.posting_timeframe,
        "key_set_hash": hx(&key_set_hash(&state.keys, state.posting_timeframe)),
        "config_tip_hash": hx(&state.config_tip_hash),
        "burn_count": state.burns.count,
        "burn_frontier": state.burns.nodes.iter().map(|n| hx(n)).collect::<Vec<_>>(),
        "burn_frontier_hash": hx(&state.burns.hash()),
        "burn_root": hx(&state.burns.root(&zero_hashes())),
        "imported_leaf_count": state.imported_leaf_count,
        "sunset": state.sunset,
        "S": hx(&state.digest()),
    })
}

fn cmd_deployment(opts: &BTreeMap<String, String>) -> Result<()> {
    let chain_id: u64 = required(opts, "chain-id")?.parse().context("--chain-id")?;
    let vault: [u8; 20] = unhex(required(opts, "vault")?)?;
    let posting_timeframe: u32 = opts.get("posting-timeframe").map_or(Ok(900), |s| s.parse()).context("timeframe")?;
    ensure!(posting_timeframe != 0, "a posting timeframe of 0 is refused by the deployment check (SPEC-v3 D10)");
    let dep = deployment(chain_id, vault);
    let chain = Chain::genesis()?;
    let genesis = genesis_state(&dep, &chain, posting_timeframe);
    let c = &dep.config;
    let n = u32::try_from(genesis.keys.len())?;
    let value = json!({
        "what": "Stand-in LEZ deployment of the end-to-end return-leg fixture: test sequencer keys, no node. \
                 config_id is the vault's lezConfigId; genesis.state.S is the vault's genesisState.",
        "evm_chain_id": chain_id,
        "vault": hx(&c.vault),
        "config_identity": {
            "bedrock_genesis_id": hx(&c.bedrock_genesis_id),
            "channel_id": hx(&c.channel_id),
            "bridge_header": hx(&c.bridge_header),
            "bridge_image_id": hx(&c.bridge_image_id),
            "evm_chain_id": hx(&c.evm_chain_id),
            "vault": hx(&c.vault),
            "attestation_root_id": hx(&c.attestation_root_id),
            "salt": hx(&c.salt),
        },
        "config_id": hx(&dep.config_id()),
        "n_min": c.n_min,
        "t_min": c.t_min,
        "n": n,
        "t": checkpoint_core::hash::threshold(n),
        "posting_timeframe": posting_timeframe,
        "genesis": {
            "block_id": chain.blocks[0].header.block_id,
            "block_hash": hx(&chain.blocks[0].header.hash.0),
            "state": state_json(&genesis),
        },
    });
    let out = PathBuf::from(required(opts, "out")?);
    fs::write(&out, serde_json::to_string_pretty(&value)? + "\n")?;
    println!("wrote {}: configId {} genesisState {}", out.display(), hx(&dep.config_id()), hx(&genesis.digest()));
    Ok(())
}

/// A burn note of the wallet file, for this deployment's configuration.
fn burn_message(dep: &Deployment, note: &Value) -> Result<BridgeMessage> {
    Ok(BridgeMessage::Burn(Burn {
        config_id: dep.config_id(),
        r: field(note, "r")?,
        memo: field(note, "memo")?,
        mac: field(note, "mac")?,
        token: field(note, "token")?,
        amount: field(note, "amount")?,
        c_out: field(note, "c_out")?,
    }))
}

/// The blocks of a checkpoint input an earlier run of this builder left in `dir` (from block 2 on),
/// with the nonce it echoes.
fn saved_blocks(dir: &Path, name: &str) -> Result<Option<(Hash, Vec<(Vec<u8>, Block)>)>> {
    let path = dir.join(format!("{name}.input"));
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path)?;
    let input = CheckpointInput::decode(&bytes).map_err(|e| anyhow::anyhow!("{}: {e:?}", path.display()))?;
    let blocks = input
        .blocks
        .iter()
        .map(|b| Ok((b.to_vec(), borsh::from_slice::<Block>(b).context("a stored block")?)))
        .collect::<Result<Vec<_>>>()?;
    Ok(Some((input.echo_nonce, blocks)))
}

/// The transactions a wallet or user sent in a stored block: all but its fee and clock transactions.
fn user_transactions(block: &Block) -> (Vec<LeeTransaction>, Vec<u64>) {
    let count = block.body.transactions.len().saturating_sub(2);
    (block.body.transactions[..count].to_vec(), block.body.reverted.clone())
}

/// The chain of the module docs. Every transaction is signed by its own test user; with `saved`
/// (the stored blocks from block 2 on), the blocks that carry transactions take those of the
/// stored blocks instead of newly signed ones, and every stored block must be rebuilt exactly.
fn build_chain(
    dep: &Deployment,
    wallet: &Value,
    echo: Option<Hash>,
    saved: Option<&[(Vec<u8>, Block)]>,
) -> Result<Chain> {
    let import = &wallet["import_root"];
    let leaf_count = import["leaf_count"].as_u64().context("import_root.leaf_count")?;
    let import = ImportRoot { config_id: dep.config_id(), record_slot: 0, root: field(import, "root")?, leaf_count };
    let burns: Vec<BridgeMessage> = wallet["burns"]
        .as_array()
        .context("burns")?
        .iter()
        .map(|note| burn_message(dep, note))
        .collect::<Result<_>>()?;
    ensure!(burns.len() >= 3, "the fixture wants at least three burns");
    let mut user = 20u8;
    let mut call = |message: &BridgeMessage| -> LeeTransaction {
        user += 1;
        public_call(&private_key(user), 0, dep.bridge(), message.encode())
    };

    // Blocks 2-5 as (transactions, reverted): the import, then the burns with the two ignored
    // messages in block 4.
    let busy: Vec<(Vec<LeeTransaction>, Vec<u64>)> = match saved {
        Some(blocks) => blocks[..4].iter().map(|(_, block)| user_transactions(block)).collect(),
        None => {
            let noise = [dep.burn(1001, Some(label("e2e: another configuration"))), dep.burn(1002, None)];
            vec![
                (vec![call(&BridgeMessage::ImportRoot(import))], vec![]),
                (burns[..2].iter().map(&mut call).collect(), vec![]),
                (vec![call(&noise[0]), call(&burns[2]), call(&noise[1])], vec![2]),
                (burns[3..].iter().map(&mut call).collect(), vec![]),
            ]
        }
    };
    let mut chain = Chain::genesis()?;
    for (txs, reverted) in busy {
        if txs.is_empty() {
            chain.push_valid(Vec::new())?;
        } else {
            chain.push_stand_in(txs, reverted)?;
        }
    }
    // Block 6 = T and blocks 7-9, the candidate's support.
    for _ in T..=CANDIDATE_END {
        chain.push_valid(Vec::new())?;
    }
    if let Some(nonce) = echo {
        // Blocks 10-11, block 12 = Y with the echo, blocks 13-16.
        chain.push_valid(Vec::new())?;
        chain.push_valid(Vec::new())?;
        let (txs, reverted) = match saved.and_then(|blocks| blocks.get(Y - 1)) {
            Some((_, block)) => user_transactions(block),
            None => (vec![call(&BridgeMessage::Echo { nonce })], vec![]),
        };
        chain.push_stand_in(txs, reverted)?;
        for _ in Y + 1..=FINALIZE_END {
            chain.push_valid(Vec::new())?;
        }
    }
    for (i, (bytes, _)) in saved.unwrap_or_default().iter().enumerate().take(chain.blocks.len() - 1) {
        ensure!(&chain.inscribed(i + 1) == bytes, "block {} could not be rebuilt as it was inscribed", i + 2);
    }
    ensure!(chain.blocks.len() == if echo.is_some() { FINALIZE_END + 1 } else { CANDIDATE_END + 1 }, "chain length");
    Ok(chain)
}

/// Runs the rule natively on the bytes the guest will read and writes them with the expected
/// journal, in the bench fixtures' format (`gen.rs`).
fn write_checkpoint(dir: &Path, name: &str, input: &OwnedInput, description: &str) -> Result<Outcome> {
    let bytes = input.encode();
    let decoded = CheckpointInput::decode(&bytes).map_err(|e| anyhow::anyhow!("{name}: {e:?}"))?;
    let out = run_checkpoint(&decoded).map_err(|e| anyhow::anyhow!("{name}: {e:?}"))?;
    let t = input.t_index as usize;
    let meta = json!({
        "kind": "checkpoint",
        "description": description,
        "blocks": input.blocks.len(),
        "blocks_to_t": t + 1,
        "block_bytes": input.block_bytes(),
        "block_bytes_to_t": input.blocks[..=t].iter().map(Vec::len).sum::<usize>(),
        "support_entries": input.support.len(),
        "threshold": out.threshold,
        "keys": out.new_state.keys.len(),
        "burn_count": out.journal.burn_count,
        "y_index": out.y_index,
        "segments": input.segments.len(),
        "journal": hex::encode(out.journal.encode()),
    });
    fs::write(dir.join(format!("{name}.input")), &bytes)?;
    fs::write(dir.join(format!("{name}.json")), serde_json::to_string_pretty(&meta)? + "\n")?;
    println!("wrote {name}: {} input bytes, journal {} bytes", bytes.len(), out.journal.encode().len());
    Ok(out)
}

/// The calls to one program that are executed publicly, with their transaction index.
struct CallsTo<'p, 'a> {
    program: &'p Hash,
    calls: Vec<(u32, &'a [u8])>,
}

impl<'a> Visitor<'a> for CallsTo<'_, 'a> {
    fn call(&mut self, index: u32, origin: Origin, _from: Option<CoreActor<'a>>, to: CoreActor<'a>, message: &'a [u8]) {
        if to.program_account_id == self.program && origin.is_public_execution() {
            self.calls.push((index, message));
        }
    }
}

fn message_json(message: &[u8]) -> Value {
    match BridgeMessage::decode(message) {
        Ok(Some(BridgeMessage::Burn(b))) => json!({
            "kind": "Burn", "config_id": hx(&b.config_id), "r": hx(&b.r), "memo": hx(&b.memo), "mac": hx(&b.mac),
            "token": hx(&b.token), "amount": hx(&b.amount), "c_out": hx(&b.c_out),
        }),
        Ok(Some(BridgeMessage::ImportRoot(m))) => json!({
            "kind": "ImportRoot", "config_id": hx(&m.config_id), "record_slot": m.record_slot, "root": hx(&m.root),
            "leaf_count": m.leaf_count,
        }),
        Ok(Some(BridgeMessage::Sunset { config_id })) => json!({"kind": "Sunset", "config_id": hx(&config_id)}),
        Ok(Some(BridgeMessage::Echo { nonce })) => json!({"kind": "Echo", "nonce": hx(&nonce)}),
        _ => json!({"kind": "other", "bytes": hx(message)}),
    }
}

/// What an observer reads from the inscribed blocks, through the core's own parser: id, hash,
/// who inscribed it, and every publicly executed message to the bridge with its transaction index
/// and whether that transaction is in the block's `reverted` list.
fn observed_blocks(dep: &Deployment, ins: &Inscribed) -> Result<Vec<Value>> {
    let mut out = Vec::new();
    for (i, bytes) in ins.bytes.iter().enumerate() {
        let mut visitor = CallsTo { program: &dep.config.bridge_header, calls: Vec::new() };
        let summary = parse_block(bytes, &mut visitor).map_err(|e| anyhow::anyhow!("block {i}: {e:?}"))?;
        let messages: Vec<Value> = visitor
            .calls
            .iter()
            .map(|(tx, message)| {
                let mut m = message_json(message);
                m["tx"] = json!(tx);
                m["reverted"] = json!(summary.is_reverted(*tx));
                m
            })
            .collect();
        out.push(json!({
            "id": summary.header.block_id,
            "hash": hx(&summary.header.hash),
            "bytes": bytes.len(),
            "transactions": summary.tx_count,
            "inscribed_by": hx(&dep.sequencers[ins.producer[i]].public()),
            "bridge_messages": messages,
        }));
    }
    Ok(out)
}

fn checkpoint_json(name: &str, input: &OwnedInput, out: &Outcome, first_block: u64) -> Value {
    json!({
        "fixture": name,
        "input_bytes": input.encode().len(),
        "first_block": first_block,
        "t_block": first_block + u64::from(input.t_index),
        "y_block": first_block + u64::from(out.y_index),
        "last_block": first_block + input.blocks.len() as u64 - 1,
        "support_blocks": input.support.iter().map(|(i, _)| first_block + u64::from(*i)).collect::<Vec<_>>(),
        "producer": hx(&out.producer),
        "support_keys": out.support_keys,
        "threshold": out.threshold,
        "echo_nonce": hx(&out.journal.echo_nonce),
        "journal": hx(&out.journal.encode()),
    })
}

fn cmd_chain(opts: &BTreeMap<String, String>) -> Result<()> {
    let read = |name: &str| -> Result<Value> {
        let path = required(opts, name)?;
        serde_json::from_slice(&fs::read(path).with_context(|| path.to_string())?).with_context(|| path.to_string())
    };
    let (d, wallet) = (read("deployment")?, read("wallet")?);
    let dir = PathBuf::from(required(opts, "out-dir")?);
    fs::create_dir_all(&dir)?;
    let echo: Option<Hash> = opts.get("echo-nonce").map(|s| unhex(s)).transpose()?;
    ensure!(echo != Some([0u8; 32]), "an echo nonce of 0 means no echo: leave --echo-nonce out");

    let chain_id = d["evm_chain_id"].as_u64().context("evm_chain_id")?;
    let dep = deployment(chain_id, field(&d, "vault")?);
    ensure!(hx(&dep.config_id()) == d["config_id"], "the deployment file is for another configuration");
    ensure!(field::<32>(&wallet, "config_id")? == dep.config_id(), "the wallet file is for another configuration");
    let posting_timeframe = u32::try_from(d["posting_timeframe"].as_u64().context("posting_timeframe")?)?;

    // An earlier run's zone: the finalisation input if it echoes this nonce, else the candidate's.
    let saved = match (saved_blocks(&dir, "cp-e2e-finalize")?, saved_blocks(&dir, "cp-e2e-candidate")?) {
        (Some((nonce, blocks)), _) if Some(nonce) == echo => Some(blocks),
        (_, Some((_, blocks))) => Some(blocks),
        _ => None,
    };
    if let Some(blocks) = &saved {
        println!("the zone's blocks 2-{} are those already inscribed in {}", blocks.len() + 1, dir.display());
    }
    let chain = build_chain(&dep, &wallet, echo, saved.as_deref())?;
    let ins = Inscribed::new(&dep, &chain);
    let genesis = genesis_state(&dep, &chain, posting_timeframe);
    ensure!(hx(&genesis.digest()) == d["genesis"]["state"]["S"], "the genesis state differs from the deployment's");
    let first_block = chain.blocks[1].header.block_id;

    let candidate = checkpoint_input(&dep, &ins, genesis.clone(), 0, T, T, CANDIDATE_END, [0; 32], 3);
    let cand = write_checkpoint(
        &dir,
        "cp-e2e-candidate",
        &candidate,
        "end-to-end return leg, candidate: ImportRoot and the wallet file's burns in blocks 2-5, T = block 6, \
         support by the other 3 of 4 keys in blocks 7-9 (n = 4, t = 3)",
    )?;
    let mut checkpoints = json!({"candidate": checkpoint_json("cp-e2e-candidate", &candidate, &cand, first_block)});
    if let Some(nonce) = echo {
        let finalize = checkpoint_input(&dep, &ins, genesis.clone(), 0, T, Y, FINALIZE_END, nonce, 3);
        let fin = write_checkpoint(
            &dir,
            "cp-e2e-finalize",
            &finalize,
            "end-to-end return leg, finalisation: the candidate's blocks 2-9, then Y = block 12 carrying \
             Echo(nonce), support by the other 3 of 4 keys in blocks 13, 15 and 16",
        )?;
        // The finalisation proves the candidate's state again; only the echo differs.
        let mut without_echo = fin.journal;
        without_echo.echo_nonce = [0u8; 32];
        ensure!(fin.new_state == cand.new_state, "the two checkpoints differ in state");
        ensure!(without_echo == cand.journal, "the two journals differ in more than the echo");
        ensure!(fin.journal.echo_nonce == nonce, "the finalisation does not echo the nonce");
        ensure!(candidate.blocks[..] == finalize.blocks[..candidate.blocks.len()], "the candidate is not a prefix");
        checkpoints["finalize"] = checkpoint_json("cp-e2e-finalize", &finalize, &fin, first_block);
    }

    let lez = json!({
        "what": "The stand-in zone as an observer reads it: the inscribed blocks (ids, hashes, publicly executed \
                 messages to the bridge), the checkpoint state at T and the checkpoints cut from it. Built by \
                 checkpoint/fidelity/src/bin/e2e.rs with test keys; no LEZ node ran.",
        "config_id": hx(&dep.config_id()),
        "channel_id": hx(&dep.config.channel_id),
        "bridge_header": hx(&dep.config.bridge_header),
        "parent_state": hx(&genesis.digest()),
        "blocks": observed_blocks(&dep, &ins)?,
        "state_at_t": state_json(&cand.new_state),
        "checkpoints": checkpoints,
    });
    fs::write(dir.join("lez.json"), serde_json::to_string_pretty(&lez)? + "\n")?;
    println!(
        "wrote lez.json: {} blocks, T = block {}, {} burns, burn root {}, newState {}",
        chain.blocks.len(),
        cand.journal.tip_id,
        cand.journal.burn_count,
        hx(&cand.journal.burn_root),
        hx(&cand.journal.new_state)
    );
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("deployment") => cmd_deployment(&options(&args[1..])?),
        Some("chain") => cmd_chain(&options(&args[1..])?),
        _ => bail!(
            "usage: e2e deployment --chain-id <n> --vault <0x…> [--posting-timeframe <slots>] --out <file>\n       \
             e2e chain --deployment <file> --wallet <file> [--echo-nonce <0x…>] --out-dir <dir>"
        ),
    }
}
