//! `privacy-blocks`: the checkpoint program's own checks over the privacy suite's LEZ blocks
//! (`lez/harness/tests/privacy_suite.rs`): can `checkpoint_b` prove the burn root that the suite's real
//! release proofs bind? (`docs/privacy/real/ROOT.md`.)
//!
//! ```text
//! privacy-blocks --lez lez.json --scenario scenario.json --out check.json
//! ```
//!
//! Over universe A's blocks (variant `private`: the deposit-root import, the 20 private claims and the 20
//! burns, in block order), with checkpoint-core's own functions, the ones the guest runs:
//! 1. per block, what the rule's `link_block` checks: the strict parse, the id after its predecessor's,
//!    the stored `prev_block_hash` against the predecessor's hash, the stored `hash` against
//!    `hash_parsed_block`;
//! 2. `run_checkpoint_b` with `T` = the last block: (a) with what the harness has (no channel key set,
//!    no inscription of `T`, no attestation); (b) the same with a placeholder key set of three label keys,
//!    so that the rule reaches the blocks; (c) as (b) over copies whose `hash` and `prev_block_hash` are
//!    filled with `hash_parsed_block` (a change to the blocks; diagnostic only, to see the next refusal).
//!    None of these inputs is a checkpoint; nothing is proven;
//! 3. the burns the rule would count (bridge calls executed in public by non-reverted transactions,
//!    `Burn` for the suite's configId), their tree's root and count against the scenario's.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{anyhow, ensure, Context as _, Result};
use checkpoint_core::hash::sha256_parts;
use checkpoint_core::input::CheckpointInputB;
use checkpoint_core::lez::{hash_parsed_block, parse_block, Actor, Origin, Visitor};
use checkpoint_core::mantle::ElidedInscription;
use checkpoint_core::rule_b::run_checkpoint_b;
use checkpoint_core::state::{Config, State};
use checkpoint_core::tree::{burn_leaf, zero_hashes, Frontier};
use checkpoint_core::wire::BridgeMessage;
use checkpoint_core::Hash;
use serde_json::{json, Value};

/// The suite's vault floors (`vault/test/utils/VaultTestBase.sol`: `N_MIN`, `T_MIN`).
const N_MIN: u32 = 3;
const T_MIN: u32 = 2;
/// The harness's configuration fields (`lez/harness/src/bridge.rs`, `config`).
const BEDROCK_GENESIS_ID: Hash = [0x0b; 32];
const CHANNEL_ID: Hash = [0x01; 32];
const NO_SIGNATURE: [u8; 64] = [0u8; 64];

fn args() -> Result<BTreeMap<String, String>> {
    let v: Vec<String> = std::env::args().skip(1).collect();
    ensure!(v.len().is_multiple_of(2), "arguments come in --key value pairs");
    v.chunks(2).map(|p| Ok((p[0].strip_prefix("--").context("expected --key")?.to_string(), p[1].clone()))).collect()
}

fn hx(b: &[u8]) -> String {
    format!("0x{}", hex::encode(b))
}

fn h<const N: usize>(v: &Value, what: &str) -> Result<[u8; N]> {
    let s = v.as_str().with_context(|| format!("{what}: not a string"))?;
    let b = hex::decode(s.strip_prefix("0x").unwrap_or(s)).with_context(|| format!("{what}: not hex"))?;
    b.try_into().map_err(|_| anyhow!("{what}: not {N} bytes"))
}

/// The guest's filter (`rule.rs`, `CallsTo`): calls to the bridge program executed in public.
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

fn run(config: &Config, parent: &State, blocks: &[&[u8]]) -> String {
    let input = CheckpointInputB {
        config: config.clone(),
        parent: parent.clone(),
        config_changes: Vec::new(),
        blocks: blocks.to_vec(),
        t_index: u32::try_from(blocks.len() - 1).expect("few blocks"),
        producer: ElidedInscription { prefix: &[], suffix: &[], signature: &NO_SIGNATURE },
        attestations: Vec::new(),
        segments: Vec::new(),
    };
    match run_checkpoint_b(&input) {
        Ok(o) => format!("accepted (burn root {})", hx(&o.journal.burn_root)),
        Err(e) => format!("refused: {e:?}"),
    }
}

fn main() -> Result<()> {
    let a = args()?;
    let (lez_path, sc_path, out) = (
        a.get("lez").context("--lez")?,
        a.get("scenario").context("--scenario")?,
        a.get("out").context("--out")?,
    );
    let (lez_bytes, sc_bytes) = (fs::read(lez_path)?, fs::read(sc_path)?);
    let lez: Value = serde_json::from_slice(&lez_bytes)?;
    let sc: Value = serde_json::from_slice(&sc_bytes)?;
    let program: Hash = h(&lez["program"], "program")?;
    let config_id: Hash = h(&sc["config_id"], "scenario config_id")?;
    ensure!(h::<32>(&lez["config_id"], "config_id")? == config_id, "lez.json and scenario.json disagree on configId");
    let want_root: Hash = h(&sc["burn_root"], "burn_root")?;
    let want_count = sc["burn_count"].as_u64().context("burn_count")?;

    // Universe A, in block order.
    let mut recs: Vec<(u64, String, Vec<u8>)> = Vec::new();
    for r in lez["records"].as_array().context("records")? {
        if r["variant"] != "private" {
            continue;
        }
        let block = hex::decode(r["block"].as_str().context("block")?)?;
        recs.push((r["block_id"].as_u64().context("block_id")?, r["id"].as_str().unwrap_or("?").to_string(), block));
    }
    recs.sort_by_key(|r| r.0);
    ensure!(!recs.is_empty(), "no private-variant records");

    // 1. What `link_block` checks, block by block.
    let mut rows = Vec::new();
    let mut prev: Option<(u64, Hash)> = None;
    let (mut hash_ok, mut link_ok, mut id_ok) = (0u32, 0u32, 0u32);
    for (bid, id, bytes) in &recs {
        let mut v = Calls { program: &program, calls: Vec::new() };
        let s = parse_block(bytes, &mut v).map_err(|e| anyhow!("{id}: parse_block refused it: {e:?}"))?;
        ensure!(s.header.block_id == *bid, "{id}: header id {} is not the record's {bid}", s.header.block_id);
        let hash = hash_parsed_block(bytes).map_err(|e| anyhow!("{id}: {e:?}"))?;
        let hash_matches = hash == s.header.hash;
        let links = prev.map(|(_, ph)| s.header.prev_block_hash == ph);
        let follows = prev.map(|(pid, _)| s.header.block_id == pid + 1);
        hash_ok += u32::from(hash_matches);
        link_ok += u32::from(links == Some(true));
        id_ok += u32::from(follows == Some(true));
        rows.push(json!({
            "block_id": s.header.block_id, "record": id, "bytes": bytes.len(), "txs": s.tx_count, "status": s.status,
            "timestamp_ms": s.header.timestamp, "stored_hash": hx(&s.header.hash), "hash_parsed_block": hx(&hash),
            "hash_matches": hash_matches, "stored_prev_block_hash": hx(&s.header.prev_block_hash),
            "predecessor_hash": prev.map(|(_, ph)| hx(&ph)), "links": links, "id_follows": follows,
            "public_bridge_calls": v.calls.len(),
        }));
        prev = Some((s.header.block_id, hash));
    }
    let n = recs.len();
    let blocks: Vec<&[u8]> = recs.iter().map(|r| r.2.as_slice()).collect();

    // 2. The rule itself.
    let mut evm_chain_id = [0u8; 32];
    evm_chain_id[24..].copy_from_slice(&sc["chain_id"].as_u64().context("chain_id")?.to_be_bytes());
    let config = Config {
        bedrock_genesis_id: BEDROCK_GENESIS_ID,
        channel_id: CHANNEL_ID,
        bridge_header: program,
        bridge_image_id: h(&lez["bridge_image_id"], "bridge_image_id")?,
        evm_chain_id,
        vault: h(&sc["vault"], "vault")?,
        // Not in the suite's output (the bridge wire derives them from the attestation account and the
        // controls); every run below refuses before a journal, and the burns of step 3 use the scenario's id.
        attestation_root_id: [0u8; 32],
        salt: [0u8; 32],
        n_min: N_MIN,
        t_min: T_MIN,
    };
    // The parent: the tip before the first block. The harness never hashed block `first - 1`; zero is
    // what the first block stores as its predecessor's hash.
    let first = recs[0].0;
    let parent = State {
        tip_id: first - 1,
        tip_hash: [0u8; 32],
        keys: Vec::new(),
        posting_timeframe: 0,
        config_tip_hash: [0u8; 32],
        burns: Frontier::default(),
        imported_leaf_count: 0,
        sunset: false,
    };
    let as_provided = run(&config, &parent, &blocks);
    let placeholder_keys: Vec<Hash> =
        (0u8..3).map(|i| sha256_parts(&[b"ferrata/privacy-root/placeholder-key/v1", &[i]])).collect();
    let with_keys = State { keys: placeholder_keys.clone(), ..parent.clone() };
    let keys_only = run(&config, &with_keys, &blocks);
    let mut relinked: Vec<Vec<u8>> = recs.iter().map(|r| r.2.clone()).collect();
    let mut prev_hash = parent.tip_hash;
    for b in &mut relinked {
        b[8..40].copy_from_slice(&prev_hash);
        prev_hash = hash_parsed_block(b).map_err(|e| anyhow!("{e:?}"))?;
        b[40..72].copy_from_slice(&prev_hash);
    }
    let relinked_refs: Vec<&[u8]> = relinked.iter().map(Vec::as_slice).collect();
    let relinked_run = run(&config, &with_keys, &relinked_refs);

    // 3. The burns the rule would count.
    let (mut frontier, mut burns, mut imports) = (Frontier::default(), Vec::new(), Vec::new());
    for (bid, _, bytes) in &recs {
        let mut v = Calls { program: &program, calls: Vec::new() };
        let s = parse_block(bytes, &mut v).map_err(|e| anyhow!("{e:?}"))?;
        for (tx, origin, message) in v.calls {
            let Ok(Some(msg)) = BridgeMessage::decode(message) else { continue };
            if s.is_reverted(tx) || msg.config_id() != Some(&config_id) {
                continue;
            }
            match msg {
                BridgeMessage::Burn(b) => {
                    let leaf = burn_leaf(&config_id, frontier.count, &b.token, &b.amount, &b.c_out, &b.r);
                    burns.push(json!({"burn_index": frontier.count, "block_id": bid, "origin": format!("{origin:?}"),
                        "leaf": hx(&leaf)}));
                    frontier.append(leaf).map_err(|e| anyhow!("{e:?}"))?;
                }
                BridgeMessage::ImportRoot(m) => imports.push(json!({"block_id": bid, "leaf_count": m.leaf_count})),
                _ => {}
            }
        }
    }
    let root = frontier.root(&zero_hashes());

    let name = |p: &str| Path::new(p).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    let report = json!({
        "inputs": {
            "lez": {"file": name(lez_path), "sha256": hx(&sha256_parts(&[&lez_bytes])), "bytes": lez_bytes.len()},
            "scenario": {"file": name(sc_path), "sha256": hx(&sha256_parts(&[&sc_bytes])), "bytes": sc_bytes.len()},
            "proofs": lez["proofs"], "config_id": hx(&config_id), "bridge_program": hx(&program),
        },
        "blocks": {
            "variant": "private", "count": n, "first_id": first, "last_id": recs[n - 1].0,
            "parsed_strictly": n, "hash_matches": hash_ok, "links_to_predecessor": link_ok, "links_checked": n - 1,
            "ids_follow": id_ok, "bytes_total": recs.iter().map(|r| r.2.len()).sum::<usize>(),
        },
        "run_checkpoint_b": {
            "a_as_the_harness_provides": as_provided,
            "b_placeholder_key_set": keys_only,
            "c_placeholder_key_set_blocks_relinked_copies": relinked_run,
            "placeholder_keys": placeholder_keys.iter().map(|k| hx(k)).collect::<Vec<_>>(),
            "config_id_of_the_config_used": hx(&config.config_id()),
        },
        "burns": {
            "counted": frontier.count, "scenario_burn_count": want_count, "root": hx(&root),
            "scenario_burn_root": hx(&want_root), "root_equal": root == want_root,
            "count_equal": frontier.count == want_count, "imports": imports, "leaves": burns,
        },
        "per_block": rows,
    });
    fs::write(out, serde_json::to_string_pretty(&report)? + "\n")?;
    println!(
        "{n} blocks {first}..={}: parsed {n}, hash matches {hash_ok}/{n}, links {link_ok}/{}, ids follow {id_ok}/{}",
        recs[n - 1].0,
        n - 1,
        n - 1
    );
    println!("run_checkpoint_b (a) {as_provided}\nrun_checkpoint_b (b) {keys_only}\nrun_checkpoint_b (c) {relinked_run}");
    println!(
        "burns counted {} (scenario {want_count}), root {} (scenario {}): equal {}",
        frontier.count,
        hx(&root),
        hx(&want_root),
        root == want_root
    );
    Ok(())
}
