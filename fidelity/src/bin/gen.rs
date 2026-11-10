//! Writes the bench fixtures to `fixtures/`: `<name>.input` (the exact bytes the guest reads,
//! without the 4-byte length the host prepends) and `<name>.json` (what it is and the journal the
//! native run of the same rule produced, which the host compares with the guest's).
//!
//! Every block is built with LEZ v0.4.0-rc1's crates and every inscription with
//! logos-blockchain-core d480efc6 (see `fidelity` docs for what is executed and what is stand-in).
//!
//! usage: gen [all|checkpoints|empty|busy|support|micro|rejournal]

use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context as _, Result};
use checkpoint_core::input::{CheckpointInput, SegmentInput};
use checkpoint_core::rule::{run_checkpoint, run_segment};
use common::transaction::LeeTransaction;
use fidelity::bedrock::Sequencer;
use fidelity::dump::dump_blocks;
use fidelity::lez_build::{pp_stand_in, private_key, public_call, resize_proof, Chain, PpSpec, PROOF_BYTES};
use fidelity::scenario::{checkpoint_input, label, Deployment, Inscribed, OwnedInput};
use lee::{AccountId, Actor};
use serde_json::json;

fn out_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures")
}

fn write(name: &str, input: &[u8], meta: serde_json::Value) -> Result<()> {
    let dir = out_dir();
    fs::create_dir_all(&dir)?;
    fs::write(dir.join(format!("{name}.input")), input)?;
    fs::write(dir.join(format!("{name}.json")), serde_json::to_string_pretty(&meta)? + "\n")?;
    println!("wrote {name}: {} input bytes", input.len());
    Ok(())
}

/// Runs the rule natively on `input` and writes it with its expected journal.
fn write_checkpoint(name: &str, input: &OwnedInput, description: &str) -> Result<()> {
    let bytes = input.encode();
    let out =
        run_checkpoint(&CheckpointInput::decode(&bytes).unwrap()).map_err(|e| anyhow::anyhow!("{name}: {e:?}"))?;
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
    write(name, &bytes, meta)
}

/// The privacy-preserving transactions of LEZ's committed sequencer dump (made by LEZ's wallet),
/// each with its dev-mode receipt replaced by a full-size proof.
fn real_pp_txs() -> Result<Vec<LeeTransaction>> {
    let mut out = Vec::new();
    for (_, block) in dump_blocks()? {
        for tx in &block.body.transactions {
            if let Some(big) = resize_proof(tx, PROOF_BYTES) {
                out.push(big);
            }
        }
    }
    anyhow::ensure!(!out.is_empty(), "no privacy-preserving transaction in the dump");
    Ok(out)
}

/// A 360-block hour after genesis, with `txs_at(block index)` in the blocks that have any, then
/// `extra` empty blocks for support. Blocks with transactions are stand-ins (module docs).
fn hour(extra: usize, mut txs_at: impl FnMut(usize) -> Vec<LeeTransaction>) -> Result<Chain> {
    let mut chain = Chain::genesis()?;
    for i in 1..=360 + extra {
        let txs = if i <= 360 { txs_at(i) } else { Vec::new() };
        if txs.is_empty() {
            chain.push_valid(Vec::new())?;
        } else {
            chain.push_stand_in(txs, Vec::new())?;
        }
    }
    Ok(chain)
}

fn empty_hours() -> Result<()> {
    let dep = Deployment::new(4, 3, 2);
    let user = private_key(7);
    let start = Instant::now();

    // ---- empty blocks: 1, 60, 360, and the composition pieces --------------------------------
    let empty = hour(4, |_| Vec::new())?;
    println!(
        "empty chain: {} blocks, all accepted by apply_block ({:.1} s)",
        empty.blocks.len(),
        start.elapsed().as_secs_f64()
    );
    let ins = Inscribed::new(&dep, &empty);
    let genesis = dep.genesis_state(&empty);
    for n in [1usize, 60, 360] {
        let i = checkpoint_input(&dep, &ins, genesis.clone(), 0, n, n, n + 3, [0; 32], 3);
        write_checkpoint(
            &format!("cp-empty-{n}"),
            &i,
            &format!("{n} empty LEZ blocks up to T, then 3 support blocks inscribed by the other 3 of 4 keys (t = 3)"),
        )?;
    }
    // Finalisation flavour: the echo is in a block 2 after T (block 362 carries it instead of
    // being empty, so this chain is built separately).
    {
        let echo = label("beacon nonce");
        let mut chain = Chain::genesis()?;
        for i in 1..=366usize {
            if i == 362 {
                chain.push_stand_in(
                    vec![public_call(
                        &user,
                        0,
                        dep.bridge(),
                        checkpoint_core::wire::BridgeMessage::Echo { nonce: echo }.encode(),
                    )],
                    vec![],
                )?;
            } else {
                chain.push_valid(Vec::new())?;
            }
        }
        let ins = Inscribed::new(&dep, &chain);
        let i = checkpoint_input(&dep, &ins, dep.genesis_state(&chain), 0, 360, 362, 366, echo, 3);
        write_checkpoint(
            "cp-echo-360",
            &i,
            "360 empty blocks up to T; Y = T+2 carries Echo(nonce); 3 support blocks after Y",
        )?;
    }
    // Composition: a segment over blocks 1..=180, then a checkpoint over 181..=360 (+ support)
    // that verifies it. Same parent and T as cp-empty-360, so the journal must be the same.
    {
        let blocks: Vec<&[u8]> = ins.bytes[1..=180].iter().map(Vec::as_slice).collect();
        let seg_bytes = SegmentInput { config: dep.config.clone(), parent: genesis.clone(), blocks }.encode();
        let (seg, mid) =
            run_segment(&SegmentInput::decode(&seg_bytes).unwrap()).map_err(|e| anyhow::anyhow!("{e:?}"))?;
        write(
            "seg-empty-180",
            &seg_bytes,
            json!({"kind": "segment", "description": "segment over empty blocks 1..=180 (no support check)", "blocks": 180,
                   "block_bytes": ins.bytes[1..=180].iter().map(Vec::len).sum::<usize>(), "journal": hex::encode(seg.encode())}),
        )?;
        let mut second = checkpoint_input(&dep, &ins, mid, 180, 360, 360, 363, [0; 32], 3);
        second.segments = vec![seg];
        write_checkpoint("cp-after-seg-180", &second, "checkpoint over blocks 181..=360 (+3 support) continuing from the segment seg-empty-180, which it verifies with env::verify; same journal as cp-empty-360")?;
    }

    println!("empty-hour fixtures in {:.1} s", start.elapsed().as_secs_f64());
    Ok(())
}

/// Busy hours. Transactions sit in blocks away from T, so T and the support blocks stay empty:
/// the cost of busy blocks *as the tip or as support* is measured on its own (`cp-busytip-t3`).
fn busy_hours() -> Result<()> {
    let dep = Deployment::new(4, 3, 2);
    let user = private_key(7);
    let start = Instant::now();
    let pps = real_pp_txs()?;
    for (name, count) in [("cp-pp5-360", 5usize), ("cp-pp50-360", 50)] {
        // Spread evenly; at most 4 per block (LEZ's per-block limit), here at most 1.
        let every = 360 / count;
        let mut k = 0usize;
        let chain = hour(4, |i| {
            if i % every == every / 2 && k < count {
                k += 1;
                vec![pps[k % pps.len()].clone()]
            } else {
                Vec::new()
            }
        })?;
        let ins = Inscribed::new(&dep, &chain);
        let i = checkpoint_input(&dep, &ins, dep.genesis_state(&chain), 0, 360, 360, 363, [0; 32], 3);
        write_checkpoint(name, &i, &format!("360 blocks with {count} privacy-preserving transactions (LEZ-wallet message part from LEZ's sequencer dump, proof resized to {PROOF_BYTES} bytes), 3 support blocks"))?;
    }
    {
        let mut k = 0u64;
        let chain = hour(4, |i| {
            if i % 36 == 18 && k < 10 {
                k += 1;
                vec![public_call(&user, u128::from(k), dep.bridge(), dep.burn(k, None).encode())]
            } else {
                Vec::new()
            }
        })?;
        let ins = Inscribed::new(&dep, &chain);
        let i = checkpoint_input(&dep, &ins, dep.genesis_state(&chain), 0, 360, 360, 363, [0; 32], 3);
        write_checkpoint("cp-burns10-360", &i, "360 blocks, 10 of them with one public burn (a signed public transaction calling the bridge), 3 support blocks")?;
    }
    {
        let token = Actor::new(AccountId::new([0xA1; 32]), AccountId::new(label("token program")));
        let fee_inbox = Actor::native_balance(AccountId::new(label("fee inbox")));
        let mut k = 0u64;
        let chain = hour(4, |i| {
            if i % 36 == 18 && k < 10 {
                k += 1;
                let spec = PpSpec {
                    deliveries: vec![
                        (true, token, dep.bridge(), dep.burn(k, None).encode()),
                        (true, token, fee_inbox, vec![0; 64]),
                    ],
                    ..PpSpec::default()
                };
                vec![pp_stand_in(&spec, u8::try_from(k).unwrap())]
            } else {
                Vec::new()
            }
        })?;
        let ins = Inscribed::new(&dep, &chain);
        let i = checkpoint_input(&dep, &ins, dep.genesis_state(&chain), 0, 360, 360, 363, [0; 32], 3);
        write_checkpoint("cp-privburns10-360", &i, "360 blocks, 10 of them with one private burn (a privacy-preserving transaction whose boundary sends Burn to the bridge; stand-in contents, full-size proof), 3 support blocks")?;
    }

    // The worst case for support: T and every support block carry 4 privacy-preserving
    // transactions (LEZ's per-block maximum), so each inscription check hashes ~0.95 MB.
    {
        let mut chain = Chain::genesis()?;
        for _ in 0..4 {
            chain.push_stand_in((0..4).map(|k| pps[k % pps.len()].clone()).collect(), Vec::new())?;
        }
        let ins = Inscribed::new(&dep, &chain);
        let i = checkpoint_input(&dep, &ins, dep.genesis_state(&chain), 0, 1, 1, 4, [0; 32], 3);
        write_checkpoint("cp-busytip-t3", &i, "T and its 3 support blocks each carry 4 privacy-preserving transactions (LEZ-wallet message part, full-size proof): the support check over busy blocks, n = 4, t = 3")?;
    }
    println!("busy-hour fixtures in {:.1} s", start.elapsed().as_secs_f64());
    Ok(())
}

fn support_only() -> Result<()> {
    // ---- support alone: t = 2, 3, 5 (n = 3, 4, 7) ----------------------------------------------
    for (n, t) in [(3usize, 2usize), (4, 3), (7, 5)] {
        let dep = Deployment::new(n, 1, 1);
        let mut chain = Chain::genesis()?;
        for _ in 0..=t {
            chain.push_valid(Vec::new())?;
        }
        let ins = Inscribed::new(&dep, &chain);
        let i = checkpoint_input(&dep, &ins, dep.genesis_state(&chain), 0, 1, 1, 1 + t, [0; 32], t);
        write_checkpoint(
            &format!("cp-support-t{t}"),
            &i,
            &format!("1 empty block T and {t} support blocks, n = {n}, t = {t}"),
        )?;
    }
    Ok(())
}

/// Micro-benchmark inputs: `[mode] ‖ payload`, two sizes per mode.
fn micro() -> Result<()> {
    let put = |name: &str, mode: u8, payload: &[u8], what: &str, k: u64| -> Result<()> {
        let mut bytes = vec![mode];
        bytes.extend_from_slice(payload);
        write(&format!("micro-{name}"), &bytes, json!({"kind": "micro", "mode": mode, "k": k, "description": what}))
    };
    let filler = |n: usize| (0..n).map(|i| (i * 2_654_435_761 % 251) as u8).collect::<Vec<u8>>();
    for n in [10_000usize, 1_000_000] {
        put(&format!("read-{n}"), 0, &filler(n), "read the input only", n as u64)?;
        put(&format!("sha256-{n}"), 1, &filler(n), "SHA-256 of the payload", n as u64)?;
        put(&format!("blake2b-{n}"), 2, &filler(n), "BLAKE2b-256 of the payload", n as u64)?;
        put(&format!("sha256native-{n}"), 8, &filler(n), "SHA-256 of the payload via risc0_zkvm::sha::Impl", n as u64)?;
    }
    for k in [1usize, 6] {
        let mut p = Vec::new();
        for i in 0..k {
            let s = Sequencer::from_seed(1000 + i as u64);
            let msg = label(&format!("msg {i}"));
            p.extend_from_slice(&s.public());
            p.extend_from_slice(&msg);
            p.extend_from_slice(&s.0.sign_payload(&msg).to_bytes());
        }
        put(&format!("ed25519-{k}"), 3, &p, "Ed25519 verify_strict over a 32-byte message", k as u64)?;
    }
    // Ed25519 over a ~200-byte message (SPEC-v3 D7 mode (B) attestations are about that size).
    for k in [1usize, 6] {
        let mut p = Vec::new();
        for i in 0..k {
            let s = Sequencer::from_seed(2000 + i as u64);
            let msg: Vec<u8> = (0..200u32).map(|j| (j as u8) ^ (i as u8)).collect();
            p.extend_from_slice(&s.public());
            p.extend_from_slice(&s.0.sign_payload(&msg).to_bytes());
            p.extend_from_slice(&u32::try_from(msg.len()).unwrap().to_le_bytes());
            p.extend_from_slice(&msg);
        }
        put(&format!("ed25519msg200-{k}"), 9, &p, "Ed25519 verify_strict over a 200-byte message", k as u64)?;
    }
    for k in [1u64, 101] {
        put(&format!("tree-{k}"), 4, &k.to_le_bytes(), "burn leaf + frontier append, then one root", k)?;
    }
    // Blocks for parse/hash, and inscriptions for the support check: from an empty chain and one
    // block with 4 privacy-preserving transactions (LEZ's per-block maximum).
    let dep = Deployment::new(4, 1, 1);
    let mut chain = Chain::genesis()?;
    for _ in 0..110 {
        chain.push_valid(Vec::new())?;
    }
    let pps = real_pp_txs()?;
    chain.push_stand_in((0..4).map(|k| pps[k % pps.len()].clone()).collect(), Vec::new())?;
    let ins = Inscribed::new(&dep, &chain);
    let blocks = |range: std::ops::Range<usize>| {
        let mut p = Vec::new();
        for i in range {
            p.extend_from_slice(&u32::try_from(ins.bytes[i].len()).unwrap().to_le_bytes());
            p.extend_from_slice(&ins.bytes[i]);
        }
        p
    };
    for (name, range) in [("10empty", 1..11), ("110empty", 1..111), ("pp4", 111..112)] {
        let bytes: usize = ins.bytes[range.clone()].iter().map(Vec::len).sum();
        put(&format!("parse-{name}"), 5, &blocks(range.clone()), "parse LEZ blocks (no hash)", bytes as u64)?;
        put(&format!("hash-{name}"), 6, &blocks(range.clone()), "parse and hash LEZ blocks", bytes as u64)?;
    }
    let inscriptions = |range: std::ops::Range<usize>| {
        let mut p = dep.config.channel_id.to_vec();
        for i in range {
            let tx = &ins.inscriptions[i];
            let e = checkpoint_core::mantle::ElidedInscription::split(&tx.ops, 0, &tx.signature).unwrap();
            for part in [ins.bytes[i].as_slice(), e.prefix, e.suffix] {
                p.extend_from_slice(&u32::try_from(part.len()).unwrap().to_le_bytes());
                p.extend_from_slice(part);
            }
            p.extend_from_slice(&tx.signature);
        }
        p
    };
    for (name, range) in [("insc-1empty", 1..2), ("insc-6empty", 1..7), ("insc-pp4", 111..112)] {
        let bytes: usize = ins.bytes[range.clone()].iter().map(Vec::len).sum();
        put(
            name,
            7,
            &inscriptions(range),
            "inscription checks (BLAKE2b tx hash over the block, Ed25519)",
            bytes as u64,
        )?;
    }
    Ok(())
}

/// Re-runs the rule natively on every checkpoint fixture's input and rewrites the expected
/// journal (and the fields derived from the run) in its JSON; the inputs are unchanged. Used when
/// the journal format changes (SPEC-v3: 13 -> 17 words) without rebuilding the chains.
fn rejournal() -> Result<()> {
    let mut names: Vec<String> = fs::read_dir(out_dir())?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter_map(|f| f.strip_suffix(".json").map(str::to_string))
        .filter(|n| n.starts_with("cp-"))
        .collect();
    names.sort();
    for name in names {
        let input = fs::read(out_dir().join(format!("{name}.input")))?;
        let mut meta: serde_json::Value = serde_json::from_slice(&fs::read(out_dir().join(format!("{name}.json")))?)?;
        let out =
            run_checkpoint(&CheckpointInput::decode(&input).unwrap()).map_err(|e| anyhow::anyhow!("{name}: {e:?}"))?;
        meta["journal"] = json!(hex::encode(out.journal.encode()));
        meta["threshold"] = json!(out.threshold);
        meta["keys"] = json!(out.new_state.keys.len());
        meta["burn_count"] = json!(out.journal.burn_count);
        meta["y_index"] = json!(out.y_index);
        fs::write(out_dir().join(format!("{name}.json")), serde_json::to_string_pretty(&meta)? + "\n")?;
        println!("rejournaled {name}: {} journal bytes", out.journal.encode().len());
    }
    Ok(())
}

fn main() -> Result<()> {
    let what = std::env::args().nth(1).unwrap_or_else(|| "all".into());
    fs::create_dir_all(out_dir()).context("fixtures dir")?;
    if what == "rejournal" {
        return rejournal();
    }
    if what == "all" || what == "micro" {
        micro()?;
    }
    if what == "all" || what == "checkpoints" || what == "empty" {
        empty_hours()?;
    }
    if what == "all" || what == "checkpoints" || what == "busy" {
        busy_hours()?;
    }
    if what == "all" || what == "checkpoints" || what == "support" {
        support_only()?;
    }
    Ok(())
}
