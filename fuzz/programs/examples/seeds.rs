//! Writes the seed corpora (`seeds/<target>/`) from committed data: the lab's recorded rule (B)
//! inputs over real LEZ blocks (`checkpoint/realblocks/out/*.input`: their blocks, inscriptions,
//! configuration changes, attestations, and every visible call's message), and the bridge's
//! vectors (`bridge/vectors/`: the release and claim guests' inputs, the journals).
//! Usage (from fuzz/programs): `cargo run --release --example seeds -- seeds`
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use checkpoint_core::input::{CheckpointInput, CheckpointInputB, SegmentInput};
use checkpoint_core::lez::{parse_block, Actor, Origin, Visitor};
use checkpoint_core::mantle::ElidedInscription;
use ferrata_lez_bridge_wire::{BridgeMessage, Note};

struct Calls(Vec<Vec<u8>>);

impl<'a> Visitor<'a> for Calls {
    fn call(&mut self, _index: u32, _origin: Origin, _from: Option<Actor<'a>>, _to: Actor<'a>, message: &'a [u8]) {
        self.0.push(message.to_vec());
    }
}

fn hex_field(v: &serde_json::Value) -> Vec<u8> {
    let s = v.as_str().expect("a hex string");
    hex::decode(s.trim_start_matches("0x")).expect("hex")
}

fn with(prefix: &[u8], bytes: &[u8]) -> Vec<u8> {
    [prefix, bytes].concat()
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = root.join(std::env::args().nth(1).unwrap_or_else(|| "seeds".into()));
    let repo = root.join("../..");
    let mut seeds: BTreeMap<&str, BTreeSet<Vec<u8>>> = BTreeMap::new();
    let mut messages: BTreeSet<Vec<u8>> = BTreeSet::new();

    // The recorded inscriptions of blocks 1..=202 (prefix, suffix, signature, transaction hash).
    let rows: BTreeMap<u64, serde_json::Value> =
        fs::read_to_string(repo.join("checkpoint/realblocks/out/inscriptions.jsonl"))
            .expect("inscriptions.jsonl")
            .lines()
            .map(|l| serde_json::from_str::<serde_json::Value>(l).expect("json line"))
            .map(|v| (v["block"].as_u64().unwrap(), v))
            .collect();
    for (name, first) in [("real-lab-186-202", 186u64), ("real-lab-8-202", 8)] {
        let recorded = fs::read(repo.join(format!("checkpoint/realblocks/out/{name}.input"))).expect("recorded input");
        // Recorded before rule (B) took T's producer inscription (commit 91c0df8): insert it, from
        // the recorded inscription of T = 202, before the attestations (one, no segments).
        let row = &rows[&202];
        let (prefix, suffix, signature) =
            (hex_field(&row["prefix"]), hex_field(&row["suffix"]), hex_field(&row["signature"]));
        let mut producer = Vec::new();
        producer.extend_from_slice(&(prefix.len() as u32).to_le_bytes());
        producer.extend_from_slice(&prefix);
        producer.extend_from_slice(&(suffix.len() as u32).to_le_bytes());
        producer.extend_from_slice(&suffix);
        producer.extend_from_slice(&signature);
        let at = recorded.len() - (4 + attestor_core::SIGNED_ATTESTATION_LEN + 4);
        let bytes = [&recorded[..at], &producer[..], &recorded[at..]].concat();
        let input = CheckpointInputB::decode(&bytes).expect("the recorded input, with T's producer, decodes");
        for (j, block) in input.blocks.iter().enumerate() {
            let row = &rows[&(first + j as u64)];
            let ops = [hex_field(&row["prefix"]), block.to_vec(), hex_field(&row["suffix"])].concat();
            assert_eq!(
                checkpoint_core::mantle::tx_hash(&ops)[..],
                hex_field(&row["tx_hash"])[..],
                "block {}",
                first + j as u64
            );
            seeds.entry("mantle").or_default().insert(with(&[0, 0], &ops));
            for i in 0..usize::from(ops[0]) {
                if ElidedInscription::split(&ops, i, &[0; 64]).is_ok() {
                    seeds.entry("mantle").or_default().insert(with(&[1, i as u8], &ops));
                }
            }
        }
        for block in &input.blocks {
            let mut calls = Calls(Vec::new());
            parse_block(block, &mut calls).expect("a recorded block parses");
            messages.extend(calls.0);
            seeds.entry("lez_block").or_default().insert(block.to_vec());
        }
        for change in &input.config_changes {
            seeds.entry("mantle").or_default().insert(with(&[2, change.op_index], change.ops));
        }
        for a in &input.attestations {
            seeds.entry("release").or_default().insert(with(&[5], &a.encode()));
        }
        for s in &input.segments {
            seeds.entry("release").or_default().insert(with(&[4], &s.encode()));
        }
        if name != "real-lab-186-202" {
            continue; // 2.1 MB: its blocks only
        }
        let accepted = checkpoint_core::rule_b::run_checkpoint_b(&input).is_ok();
        println!("{name}: rule (B) over the recorded input with T's producer: accepted = {accepted}");
        seeds.entry("checkpoint_input").or_default().insert(with(&[1], &bytes));
        let a = CheckpointInput {
            config: input.config.clone(),
            parent: input.parent.clone(),
            config_changes: input.config_changes.clone(),
            blocks: input.blocks.clone(),
            t_index: input.t_index,
            echo_nonce: [0; 32],
            producer: input.producer,
            support: Vec::new(),
            segments: input.segments.clone(),
        };
        seeds.entry("checkpoint_input").or_default().insert(with(&[0], &a.encode()));
        for k in [1usize, 2, 4, 8, input.blocks.len()] {
            let k = k.min(input.blocks.len());
            let s = SegmentInput {
                config: input.config.clone(),
                parent: input.parent.clone(),
                blocks: input.blocks[..k].to_vec(),
            };
            seeds.entry("checkpoint_input").or_default().insert(with(&[2], &s.encode()));
        }
    }

    // The recorded privacy-preserving transactions of lez/results (bridge claim and burn, attestation),
    // each as the one transaction of a block: their visible calls, and the blocks themselves.
    for name in ["bridge-private-claim-tx", "bridge-private-burn-tx", "stage2-private-tx", "stage4-private-attest-tx"] {
        let tx = fs::read(repo.join(format!("lez/results/{name}.borsh"))).expect("recorded transaction");
        for body in [tx.clone(), with(&[1], &tx)] {
            let mut block = vec![0u8; 80];
            block.extend_from_slice(&1u32.to_le_bytes());
            block.extend_from_slice(&body);
            block.extend_from_slice(&0u32.to_le_bytes());
            block.push(0);
            let mut calls = Calls(Vec::new());
            if parse_block(&block, &mut calls).is_ok() {
                println!("{name}: {} visible calls", calls.0.len());
                messages.extend(calls.0);
                seeds.entry("lez_block").or_default().insert(block);
                break;
            }
        }
    }

    // Messages built from the program's own types, for the variants with plain fields, and
    // sequences that carry a state from one delivery to the next.
    let (cid, token, vault, holder, nf) = ([0xc1; 32], [0x7e; 20], [0x5a; 20], [0x4d; 32], [0x9f; 32]);
    let empty_proof = ferrata_lez_bridge_wire::ImportProof { account: vec![], root_slot: vec![], count_slot: vec![] };
    let mut amount = [0u8; 32];
    amount[31] = 100;
    let built = [
        BridgeMessage::Mint { nullifier: nf, config_id: cid, evm_chain_id: 1, vault, token, holder, amount: 100 },
        BridgeMessage::Spend {
            nullifier: nf,
            config_id: cid,
            evm_chain_id: 1,
            vault,
            token,
            leaf_count: 4,
            amount: 100,
            mint_to: Some(holder),
            fees: vec![([0x22; 32], 1)],
        },
        BridgeMessage::Admit { config_id: cid, token, mints: true, value: 100 },
        BridgeMessage::Burn { config_id: cid, r: [3; 32], memo: [0; 20], mac: [0; 16], token, amount, c_out: [4; 32] },
        BridgeMessage::ImportRoot { config_id: cid, record_slot: 64, root: [6; 32], leaf_count: 4, proof: empty_proof },
        BridgeMessage::Sunset { config_id: cid },
        BridgeMessage::Echo { nonce: holder },
        BridgeMessage::Read,
        BridgeMessage::ExtendFees { config_id: cid, token, index: 0, schedule: None, count: 1 },
    ];
    let encoded: Vec<Vec<u8>> = built.iter().map(|m| borsh::to_vec(m).unwrap()).collect();
    for m in &encoded {
        messages.insert(m.clone());
    }
    let step = |flags: u8, m: &[u8]| [&[flags][..], &(m.len() as u16).to_le_bytes(), m].concat();
    // Mint at holder(account) from the nullifier actor (mode 4, sender 5 = nullifier), then each message.
    for m in &encoded {
        let seed = [&[4u8][..], &step(8 | 5, &encoded[0]), &step(8 | 7, m), &step(8, m)].concat();
        seeds.entry("bridge_handle").or_default().insert(seed);
    }
    // Spend at the nullifier actor from the token actor (mode 3, sender 3 = token actor), twice.
    let seed = [&[3u8][..], &step(8 | 3, &encoded[1]), &step(8 | 3, &encoded[1])].concat();
    seeds.entry("bridge_handle").or_default().insert(seed);

    for m in &messages {
        seeds.entry("bridge_wire").or_default().insert(with(&[0], m));
        let Ok(decoded) = borsh::from_slice::<BridgeMessage>(m) else { continue };
        let len = u16::try_from(m.len()).unwrap_or(u16::MAX).to_le_bytes();
        for mode in 0u8..8 {
            for flags in [8u8, 8 | 2] {
                let seed = [&[mode, flags][..], &len, m].concat();
                seeds.entry("bridge_handle").or_default().insert(seed);
            }
        }
        let part = |selector: u8, bytes: Vec<u8>| with(&[selector], &bytes);
        let wire = seeds.entry("bridge_wire").or_default();
        match decoded {
            BridgeMessage::Burn { r, memo, mac, token, amount, c_out, .. } => {
                wire.insert(part(3, borsh::to_vec(&Note { r, memo, mac, token, amount, c_out }).unwrap()));
            }
            BridgeMessage::ImportRoot { proof, .. } => {
                wire.insert(part(13, borsh::to_vec(&proof).unwrap()));
            }
            BridgeMessage::Init { config, proof, .. } => {
                wire.insert(part(6, borsh::to_vec(&config).unwrap()));
                wire.insert(part(15, borsh::to_vec(&proof).unwrap()));
            }
            BridgeMessage::Claim { witness, .. } | BridgeMessage::ClaimPrivately { witness, .. } => {
                wire.insert(part(2, borsh::to_vec(&witness).unwrap()));
            }
            BridgeMessage::Register { entry, schedule, proof, .. } => {
                wire.insert(part(4, borsh::to_vec(&entry).unwrap()));
                wire.insert(part(5, borsh::to_vec(&schedule).unwrap()));
                wire.insert(part(14, borsh::to_vec(&proof).unwrap()));
            }
            BridgeMessage::Freeze { record, .. } | BridgeMessage::Tick { record, .. } => {
                wire.insert(part(7, borsh::to_vec(&record).unwrap()));
            }
            _ => {}
        }
    }

    let json = |path: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(repo.join(path)).expect("vector file")).expect("json")
    };
    let outbound = json("bridge/vectors/outbound-v2.json");
    seeds.entry("release").or_default().insert(with(&[0], &hex_field(&outbound["release"]["guest_input"])));
    let inbound = json("bridge/vectors/inbound-v2.json");
    seeds.entry("release").or_default().insert(with(&[1], &hex_field(&inbound["claim"]["guest_input"])));
    for case in json("bridge/vectors/release-journal-v2.json")["cases"].as_array().unwrap() {
        seeds.entry("release").or_default().insert(with(&[2], &hex_field(&case["journal"])));
    }
    for j in json("bridge/vectors/checkpoint-v2.json")["journals"].as_array().unwrap() {
        seeds.entry("release").or_default().insert(with(&[3], &hex_field(&j["journal"])));
    }

    // checkpoint_b edits the recorded rule (B) input itself: the empty edit list is that input.
    seeds.entry("checkpoint_b").or_default().insert(Vec::new());

    for (target, set) in &seeds {
        let dir = out.join(target);
        fs::create_dir_all(&dir).unwrap();
        for (i, seed) in set.iter().enumerate() {
            fs::write(dir.join(format!("seed-{i:05}")), seed).unwrap();
        }
        println!("{target}: {} seeds, {} bytes", set.len(), set.iter().map(Vec::len).sum::<usize>());
    }
    println!("visible calls: {} distinct messages", messages.len());
}
