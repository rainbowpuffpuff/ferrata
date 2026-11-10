//! Runs the execution-layer guests with the RISC Zero executor (no proving) on the mainnet
//! end-to-end fixture, and writes `results/<config>.txt`.
//!
//! Usage: `exec-host [CONFIG ...]`, CONFIG `keccak-accel` or `keccak-plain` (default: both).
//! The record comes from the light client run natively (bootstrap and finality update, blst);
//! every expected result is computed natively and asserted by the guest.

use std::{fs, path::PathBuf, time::Instant};

use exec_methods::{KECCAK_ACCEL_ELF, KECCAK_ACCEL_ID, KECCAK_PLAIN_ELF, KECCAK_PLAIN_ID};
use ferrata_core::{
    attest::{attest_log, attest_storage, history_hop, HeaderRecord, HISTORY_SERVE_WINDOW},
    bls::{Aggregation, Blst},
    fork::{ChainSpec, LcFork, Preset, FAR_FUTURE_EPOCH},
    keccak::keccak256,
    merkle::ZERO,
    policy::FinalizedStore,
    rlp::encode_u64,
    types::{LightClientBootstrap, LightClientHeader, LightClientUpdate, SyncAggregate, SyncCommittee},
    validate::Context,
    Root,
};
use ferrata_spectests::execkit::{account_proof, quantity, receipt_rlp, AccountProof, Trie};
use risc0_zkvm::{default_executor, ExecutorEnv, ExecutorEnvBuilder, SessionInfo};
use serde_json::Value;
use sha2::{Digest, Sha256};

const MODE_STORAGE: u32 = 0;
const MODE_LOG: u32 = 1;
const MODE_HOP: u32 = 2;
const MODE_HOP_STORAGE: u32 = 3;
const MODE_PRIMITIVES: u32 = 4;

const N: u64 = 26_379_842;
const M1: u64 = 26_379_826;
const WETH: &str = "c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
const USDC: &str = "a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
const WETH_SLOT: &str = "b374801ace2c02f5db0425ab5920a2b7ed1d5a00abbcd395fda7530ba1d666c0";
const USDC_SLOT: &str = "28c0947f463362aab09a2d7896ccc847fb7e52633a16b978fb2caf3db1a9b8f8";
const TRANSFER: &str = "ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../crates/ferrata-spectests/fixtures/mainnet-e2e-2026-11-09")
}

/// A fixture file, checked against the pinned SHA256SUMS.
fn fixture(name: &str) -> Vec<u8> {
    let bytes = fs::read(dir().join(name)).unwrap();
    let sums = fs::read_to_string(dir().join("SHA256SUMS")).unwrap();
    let want = sums.lines().find(|l| l.ends_with(&format!("  {name}"))).unwrap().split_whitespace().next().unwrap();
    assert_eq!(hex::encode(Sha256::digest(&bytes)), want, "{name} does not match SHA256SUMS");
    bytes
}

fn h<const L: usize>(s: &str) -> [u8; L] {
    hex::decode(s.trim_start_matches("0x")).unwrap().try_into().unwrap()
}

fn mainnet() -> ChainSpec {
    ChainSpec {
        preset: Preset::MAINNET,
        versions: [[0, 0, 0, 0], [1, 0, 0, 0], [2, 0, 0, 0], [3, 0, 0, 0], [4, 0, 0, 0], [5, 0, 0, 0], [6, 0, 0, 0], [7, 0, 0, 0], [8, 0, 0, 0]],
        epochs: [0, 74_240, 144_896, 194_048, 269_568, 364_032, 411_392, FAR_FUTURE_EPOCH, FAR_FUTURE_EPOCH],
    }
}

/// The record of block n: bootstrap, finality update, `HeaderRecord::from_payload_header`.
fn record() -> HeaderRecord {
    let spec = mainnet();
    let b = fixture("finality_update.ssz");
    let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as usize;
    let (att, fin) = (u32_at(0), u32_at(4));
    let update = LightClientUpdate {
        attested_header: LightClientHeader::decode(&b[att..fin], LcFork::Electra).unwrap(),
        next_sync_committee: SyncCommittee::empty(512),
        next_sync_committee_branch: vec![ZERO; LcFork::Electra.sync_committee_branch_len()],
        finalized_header: LightClientHeader::decode(&b[fin..], LcFork::Electra).unwrap(),
        finality_branch: b[8..232].chunks_exact(32).map(|c| c.try_into().unwrap()).collect(),
        sync_aggregate: SyncAggregate::decode(&b[232..392], 512).unwrap(),
        signature_slot: u64::from_le_bytes(b[392..400].try_into().unwrap()),
    };
    let bootstrap = LightClientBootstrap::decode(&fixture("bootstrap.ssz"), LcFork::Electra, 512).unwrap();
    let bls = Blst::new(Aggregation::Auto);
    let trusted = h::<32>("dab6b45fd34b9da264b15ad3e85dad586f836f3b11f5922e4c116b53574aefd3");
    let mut store = FinalizedStore::initialize(&trusted, &bootstrap, LcFork::Electra, &spec, &bls).unwrap();
    let gvr = h::<32>("4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95");
    store.process_update(&update, update.signature_slot + 1, &Context { spec: &spec, genesis_validators_root: gvr, bls: &bls }).unwrap();
    let r = HeaderRecord::from_payload_header(&store, 1).unwrap();
    assert_eq!(r.number(), N);
    r
}

enum Input {
    Blob(Vec<u8>),
    U64(u64),
    Nodes(Vec<Vec<u8>>),
}

fn write_blob(b: &mut ExecutorEnvBuilder<'_>, bytes: &[u8]) {
    b.write(&(bytes.len() as u32)).unwrap();
    let mut padded = bytes.to_vec();
    padded.resize((bytes.len() + 3) & !3, 0);
    b.write_slice(&padded);
}

/// Keccak-f permutations to hash `len` bytes (rate 136, at least one padding byte).
fn perms(len: usize) -> u64 {
    (len / 136 + 1) as u64
}

fn node_perms(nodes: &[Vec<u8>]) -> u64 {
    nodes.iter().map(|n| perms(n.len())).sum()
}

struct Case {
    title: String,
    mode: u32,
    inputs: Vec<Input>,
    /// Keccak-f permutations the attested call performs.
    permutations: u64,
}

fn storage_case(title: &str, mode_prefix: Vec<Input>, rec: &HeaderRecord, p: &AccountProof, slot: &Root) -> (Vec<Input>, u64) {
    let s = p.storage.iter().find(|s| s.key == *slot).unwrap();
    let a = attest_storage(rec, 1, &p.address, &p.proof, slot, &s.proof).unwrap();
    assert_eq!(a.fact.value, s.value, "{title}");
    let mut inputs = mode_prefix;
    inputs.extend([
        Input::Blob(p.address.to_vec()),
        Input::Blob(slot.to_vec()),
        Input::Nodes(p.proof.clone()),
        Input::Nodes(s.proof.clone()),
        Input::Blob(a.fact.value.to_vec()),
        Input::Blob(a.id.to_vec()),
    ]);
    (inputs, 2 + node_perms(&p.proof) + node_perms(&s.proof))
}

fn cases(rec: &HeaderRecord) -> Vec<Case> {
    let mut out = Vec::new();
    let rbytes = rec.to_ssz().to_vec();
    for (name, token, slot) in [("WETH", WETH, WETH_SLOT), ("USDC", USDC, USDC_SLOT)] {
        let p = account_proof(&fixture(&format!("proof_{}_{N}.json", name.to_lowercase())));
        assert_eq!(p.address, h::<20>(token));
        let (inputs, permutations) = storage_case(name, vec![Input::Blob(rbytes.clone())], rec, &p, &h::<32>(slot));
        out.push(Case {
            title: format!("attest_storage: {name} balance slot at block n ({} + {} proof nodes)", p.proof.len(), p.storage[0].proof.len()),
            mode: MODE_STORAGE,
            inputs,
            permutations,
        });
    }
    // Logs: the first USDC Transfer in a one-log receipt, and in the block's largest receipt.
    let receipts: Value = serde_json::from_slice(&fixture(&format!("receipts_{N}.json"))).unwrap();
    let receipts = receipts["result"].as_array().unwrap();
    let trie = Trie::new(receipts.iter().map(|r| {
        let (k, l) = encode_u64(quantity(&r["transactionIndex"]));
        (k[..l].to_vec(), receipt_rlp(r))
    }));
    let usdc_transfer = |r: &Value| {
        r["logs"].as_array().unwrap().iter().position(|l| {
            l["address"].as_str().unwrap().trim_start_matches("0x") == USDC
                && l["topics"].as_array().unwrap().first().and_then(Value::as_str).map(|t| t.trim_start_matches("0x")) == Some(TRANSFER)
        })
    };
    let one_log = receipts.iter().find(|r| r["logs"].as_array().unwrap().len() == 1 && usdc_transfer(r).is_some()).unwrap();
    let largest = receipts.iter().filter(|r| usdc_transfer(r).is_some()).max_by_key(|r| r["logs"].as_array().unwrap().len()).unwrap();
    for (what, r) in [("a one-log receipt", one_log), ("the block's largest receipt", largest)] {
        let i = quantity(&r["transactionIndex"]);
        let pos = usdc_transfer(r).unwrap() as u64;
        let (k, l) = encode_u64(i);
        let proof = trie.proof(&k[..l]);
        let (emitter, id) = {
            let a = attest_log(rec, 1, i, pos, &proof).unwrap();
            (a.fact.log.address, a.id)
        };
        let logs = r["logs"].as_array().unwrap().len();
        let leaf = proof.last().unwrap().len();
        out.push(Case {
            title: format!("attest_log: USDC Transfer in {what} (tx {i}, {logs} logs, {} proof nodes, leaf {leaf} bytes)", proof.len()),
            mode: MODE_LOG,
            permutations: node_perms(&proof),
            inputs: vec![
                Input::Blob(rbytes.clone()),
                Input::U64(i),
                Input::U64(pos),
                Input::Nodes(proof),
                Input::Blob(emitter.to_vec()),
                Input::Blob(id.to_vec()),
            ],
        });
    }
    // One EIP-2935 hop (n to m1 = n - 16), alone and with a storage fact under the derived record.
    let history = account_proof(&fixture(&format!("proof_history_{N}.json")));
    let mut slot = [0u8; 32];
    slot[24..].copy_from_slice(&(M1 % HISTORY_SERVE_WINDOW).to_be_bytes());
    let hs = history.storage.iter().find(|s| s.key == slot).unwrap();
    let header = fixture(&format!("header_{M1}.rlp"));
    let older = history_hop(rec, &history.proof, &hs.proof, &header).unwrap();
    assert_eq!(older.number(), M1);
    let hop_perms = 2 + node_perms(&history.proof) + node_perms(&hs.proof) + perms(header.len());
    let hop_inputs = || {
        vec![
            Input::Blob(rbytes.clone()),
            Input::Nodes(history.proof.clone()),
            Input::Nodes(hs.proof.clone()),
            Input::Blob(header.clone()),
            Input::U64(M1),
        ]
    };
    out.push(Case {
        title: format!("history_hop: n to n - 16 ({} + {} proof nodes, {}-byte header)", history.proof.len(), hs.proof.len(), header.len()),
        mode: MODE_HOP,
        inputs: hop_inputs(),
        permutations: hop_perms,
    });
    let weth_m1 = account_proof(&fixture(&format!("proof_weth_{M1}.json")));
    let (inputs, storage_perms) = storage_case("WETH at m1", hop_inputs(), &older, &weth_m1, &h::<32>(WETH_SLOT));
    out.push(Case {
        title: "history_hop, then attest_storage (WETH) under the derived record".into(),
        mode: MODE_HOP_STORAGE,
        inputs,
        permutations: hop_perms + storage_perms,
    });
    let node = history.proof[0].clone();
    out.push(Case {
        title: format!("primitives: keccak256 of a {}-byte branch node; one storage identifier", node.len()),
        mode: MODE_PRIMITIVES,
        permutations: perms(node.len()),
        inputs: vec![Input::Blob(rbytes), Input::Blob(node.clone()), Input::Blob(keccak256(&node).to_vec())],
    });
    out
}

fn execute(elf: &[u8], case: &Case) -> (Vec<(String, u64)>, SessionInfo, f64) {
    let mut b = ExecutorEnv::builder();
    b.write(&case.mode).unwrap();
    for input in &case.inputs {
        match input {
            Input::Blob(x) => write_blob(&mut b, x),
            Input::U64(v) => {
                b.write(v).unwrap();
            }
            Input::Nodes(nodes) => {
                b.write(&(nodes.len() as u32)).unwrap();
                for n in nodes {
                    write_blob(&mut b, n);
                }
            }
        }
    }
    let env = b.build().unwrap();
    let started = Instant::now();
    let info = default_executor().execute(env, elf).unwrap();
    let secs = started.elapsed().as_secs_f64();
    (info.journal.decode().unwrap(), info, secs)
}

fn main() {
    let configs: [(&str, &[u8], [u32; 8]); 2] =
        [("keccak-accel", KECCAK_ACCEL_ELF, KECCAK_ACCEL_ID), ("keccak-plain", KECCAK_PLAIN_ELF, KECCAK_PLAIN_ID)];
    let wanted: Vec<String> = std::env::args().skip(1).collect();
    let rec = record();
    let cases = cases(&rec);
    let results = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../results");
    fs::create_dir_all(&results).unwrap();
    for (name, elf, id) in configs {
        if !wanted.is_empty() && !wanted.iter().any(|w| w == name) {
            continue;
        }
        let image: String = id.iter().flat_map(|w| w.to_le_bytes()).map(|b| format!("{b:02x}")).collect();
        let mut out = format!(
            "# {name}: ferrata-core execution layer, RISC Zero zkVM 3.0.5 executor, mainnet block {N}\n\nimage id {image}, ELF {} bytes\n\n",
            elf.len()
        );
        for case in &cases {
            let (rows, info, secs) = execute(elf, case);
            let user = info.cycles();
            let padded: u64 = info.segments.iter().map(|s| 1u64 << s.po2).sum();
            out.push_str(&format!("### {}\n", case.title));
            for (label, v) in rows {
                out.push_str(&format!("{v:>12}  {label}\n"));
            }
            out.push_str(&format!(
                "{:>12}  Keccak-f permutations in the call\n{user:>12}  EXECUTION user cycles (SessionInfo::cycles)\n{padded:>12}  EXECUTION total incl. padding/paging ({} segments)\n{secs:>12.2}  executor wall-clock seconds\n\n",
                case.permutations,
                info.segments.len()
            ));
            println!("{name}: {}", case.title);
        }
        fs::write(results.join(format!("{name}.txt")), &out).unwrap();
        println!("{out}");
    }
}
