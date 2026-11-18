//! The blind red-team vectors of area "checkpoint" (`vectors/redteam-2026-11-18/vectors_bridge.py`,
//! list CHECKPOINT; written from the specification alone before any implementation was read), run
//! natively on the checkpoint core: rule (A) (`run_checkpoint`) and rule (B) (`run_checkpoint_b`),
//! every input through its byte encoding first, as the guest receives it.
//!
//! B-CKP here: the core's own hand-written v0.4 block encoders (`tests/common`, checked against
//! LEZ's in `fidelity/`), `n = 4` channel keys (`t = 3`), `nMin = 3`, `tMin = 2`, the parent at
//! block 1, blocks 2..=6, `T` = block 4 (index 2); block 3 carries one burn of the configuration
//! and `T` two; channel key 0 inscribed `T`. Every vector first checks that its unmutated baseline
//! is accepted (otherwise the run is a harness error), then applies its one mutation. Writes
//! `lez/results/redteam-2026-11-18/checkpoint.json`.
mod common;

use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};

use attestor_core::{sign_certificate, Deployment, FinalTip, SignedAttestation};
use checkpoint_core::hash::sha256;
use checkpoint_core::input::{CheckpointInput, CheckpointInputB, Support};
use checkpoint_core::mantle::{tx_hash, ConfigChange, OP_CHANNEL_CONFIG};
use checkpoint_core::rule::{run_checkpoint, Outcome};
use checkpoint_core::rule_b::{run_checkpoint_b, OutcomeB};
use checkpoint_core::state::{key_set_hash, Config, State};
use checkpoint_core::tree::{burn_leaf, zero_hashes, Frontier};
use checkpoint_core::wire::{BridgeMessage, Burn};
use checkpoint_core::{Error, Hash};
use common::*;
use ed25519_dalek::{Signer as _, SigningKey};
use serde_json::{json, Value};

const VECTORS: &str = include_str!("../../../../vectors/redteam-2026-11-18/vectors.json");
const TIMEFRAME: u32 = 30;
const T_INDEX: u32 = 2;

// ---- recording and judging (README "Phase 2 rules") ----------------------------------------------

enum Obs {
    Accepted(String),
    Refused { error: String, note: String },
    EffectMismatch(String),
    Panic(String),
    Unsupported(String),
    HarnessError(String),
}

fn judge(outcome: &str, codes: &[String], o: &Obs) -> &'static str {
    let any = codes.iter().any(|c| c == "any");
    match o {
        Obs::Unsupported(_) => "UNSUPPORTED",
        Obs::HarnessError(_) => "HARNESS-ERROR",
        Obs::EffectMismatch(_) => "MISMATCH",
        Obs::Accepted(_) => if outcome == "accepted" { "PASS" } else { "MISMATCH" },
        Obs::Refused { .. } if outcome == "accepted" => "MISMATCH",
        Obs::Refused { .. } => if any { "PASS" } else { "MISMATCH" },
        Obs::Panic(_) => if outcome == "refused" && any { "PASS" } else { "PANIC" },
    }
}

fn describe(o: &Obs) -> Value {
    match o {
        Obs::Accepted(note) => json!({"outcome": "accepted", "note": note}),
        Obs::Refused { error, note } => json!({"outcome": "refused", "error": error, "note": note}),
        Obs::EffectMismatch(note) => json!({"outcome": "accepted", "effect_mismatch": note}),
        Obs::Panic(m) => json!({"outcome": "refused", "error": format!("panic: {m}"), "note": "the core panicked"}),
        Obs::Unsupported(reason) => json!({"outcome": "unsupported", "reason": reason}),
        Obs::HarnessError(m) => json!({"outcome": "harness-error", "message": m}),
    }
}

fn panic_message(e: Box<dyn std::any::Any + Send>) -> String {
    e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()
}

struct Results {
    vectors: BTreeMap<String, Value>,
    rows: Vec<Value>,
    counts: BTreeMap<&'static str, usize>,
}

impl Results {
    fn new() -> Self {
        let doc: Value = serde_json::from_str(VECTORS).unwrap();
        let vectors = doc["vectors"].as_array().unwrap().iter().map(|v| (v["id"].as_str().unwrap().to_string(), v.clone())).collect();
        Self { vectors, rows: Vec::new(), counts: BTreeMap::new() }
    }

    fn run(&mut self, id: &str, f: impl FnOnce() -> Obs) {
        let v = self.vectors[id].clone();
        let o = catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|e| Obs::HarnessError(panic_message(e)));
        let outcome = v["expect"]["outcome"].as_str().unwrap().to_string();
        let codes: Vec<String> = v["expect"]["codes"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap().to_string()).collect();
        let verdict = judge(&outcome, &codes, &o);
        *self.counts.entry(verdict).or_insert(0) += 1;
        println!("{id:<8} {verdict:<13} {}", describe(&o));
        self.rows.push(json!({
            "id": id, "area": v["area"], "kind": v["kind"], "title": v["title"],
            "expected": {"outcome": outcome, "codes": codes},
            "observed": describe(&o), "verdict": verdict,
        }));
    }

    fn finish(self, area: &str) {
        let expected: Vec<&String> = self.vectors.iter().filter(|(_, v)| v["area"] == area).map(|(id, _)| id).collect();
        assert_eq!(expected.len(), self.rows.len(), "every vector of {area} recorded once");
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../lez/results/redteam-2026-11-18");
        std::fs::create_dir_all(&dir).unwrap();
        let doc = json!({"test": "checkpoint/crates/checkpoint-core/tests/redteam.rs", "counts": self.counts, "results": self.rows});
        std::fs::write(dir.join("checkpoint.json"), serde_json::to_string_pretty(&doc).unwrap() + "\n").unwrap();
        println!("checkpoint: {:?}", self.counts);
        assert!(!self.counts.contains_key("HARNESS-ERROR"), "harness errors");
    }
}

/// Runs the core under a panic guard: `Err(message)` if the core itself panicked.
fn guard<T>(f: impl FnOnce() -> Result<T, Error>) -> Result<Result<T, Error>, String> {
    catch_unwind(AssertUnwindSafe(f)).map_err(panic_message)
}

/// The observation of a mutated input that should be refused.
fn refusal<T>(r: Result<Result<T, Error>, String>, note: &str) -> Obs {
    match r {
        Ok(Ok(_)) => Obs::Accepted(note.to_string()),
        Ok(Err(e)) => Obs::Refused { error: format!("{e:?}"), note: note.to_string() },
        Err(m) => Obs::Panic(format!("{m} ({note})")),
    }
}

/// Several mutations of one vector: refused only if every one is refused (first one reported).
fn all_refused(parts: Vec<(String, Obs)>) -> Obs {
    let summary: Vec<String> = parts
        .iter()
        .map(|(label, o)| match o {
            Obs::Refused { error, .. } => format!("{label}: refused {error}"),
            Obs::Accepted(_) => format!("{label}: ACCEPTED"),
            Obs::Panic(m) => format!("{label}: panic {m}"),
            _ => format!("{label}: ?"),
        })
        .collect();
    let note = summary.join("; ");
    if let Some((_, o)) = parts.iter().find(|(_, o)| !matches!(o, Obs::Refused { .. })) {
        return match o {
            Obs::Accepted(_) => Obs::Accepted(note),
            Obs::Panic(m) => Obs::Panic(format!("{m} [{note}]")),
            _ => Obs::HarnessError(note),
        };
    }
    match &parts[0].1 {
        Obs::Refused { error, .. } => Obs::Refused { error: error.clone(), note },
        _ => unreachable!(),
    }
}

// ---- the fixture -----------------------------------------------------------------------------------

fn uint256(x: u64) -> Hash {
    let mut w = [0u8; 32];
    w[24..].copy_from_slice(&x.to_be_bytes());
    w
}

fn config() -> Config {
    Config {
        bedrock_genesis_id: [1; 32],
        channel_id: [2; 32],
        bridge_header: [3; 32],
        bridge_image_id: [4; 32],
        evm_chain_id: uint256(11_155_111),
        vault: [5; 20],
        attestation_root_id: [6; 32],
        salt: [7; 32],
        n_min: 3,
        t_min: 2,
    }
}

fn pk(k: &SigningKey) -> Hash {
    k.verifying_key().to_bytes()
}
fn channel(i: u8) -> SigningKey {
    key(0x10 + i)
}
fn att(i: u8) -> SigningKey {
    key(0x80 + i)
}

fn parent() -> State {
    State {
        tip_id: 1,
        tip_hash: [9; 32],
        keys: (0..4).map(|i| pk(&channel(i))).collect(),
        posting_timeframe: TIMEFRAME,
        config_tip_hash: [8; 32],
        burns: Frontier::default(),
        imported_leaf_count: 0,
        sunset: false,
    }
}

fn burn(config_id: &Hash, i: u8) -> Burn {
    Burn {
        config_id: *config_id,
        r: sha256(&[0xB0, i]),
        memo: [i; 20],
        mac: [i; 16],
        token: [0x70 + i; 20],
        amount: uint256(1_000 + u64::from(i)),
        c_out: sha256(&[0xC0, i]),
    }
}

/// A public call by user `i` to the bridge program carrying burn `b`.
fn burn_tx(c: &Config, i: u8, b: &Burn) -> Vec<u8> {
    public_tx(&[0xA0 + i; 32], &c.bridge_header, &BridgeMessage::Burn(*b).encode())
}

/// Blocks 2.. chained on the parent's tip ([9; 32]), timestamps 1000 + id.
fn chain(contents: &[Vec<Vec<u8>>]) -> (Vec<Vec<u8>>, Vec<Hash>) {
    let mut prev = [9u8; 32];
    let (mut blocks, mut hashes) = (Vec::new(), Vec::new());
    for (i, txs) in contents.iter().enumerate() {
        let id = 2 + i as u64;
        let (b, h) = block(id, &prev, 1_000 + id, txs, &[]);
        blocks.push(b);
        hashes.push(h);
        prev = h;
    }
    (blocks, hashes)
}

struct World {
    c: Config,
    p: State,
    contents: Vec<Vec<Vec<u8>>>,
    blocks: Vec<Vec<u8>>,
    hashes: Vec<Hash>,
}

/// The standard blocks: 2 empty, 3 burn 1, 4 (= T) burns 2 and 3, 5 and 6 empty; with `foreign`,
/// block 3 first carries a burn of another configuration (same chain, vault and channel; another salt).
fn world(foreign: bool) -> World {
    let c = config();
    let id = c.config_id();
    let other = Config { salt: [0x77; 32], ..c.clone() }.config_id();
    let mut block3 = Vec::new();
    if foreign {
        block3.push(burn_tx(&c, 9, &burn(&other, 9)));
    }
    block3.push(burn_tx(&c, 1, &burn(&id, 1)));
    let contents = vec![vec![], block3, vec![burn_tx(&c, 2, &burn(&id, 2)), burn_tx(&c, 3, &burn(&id, 3))], vec![], vec![]];
    let (blocks, hashes) = chain(&contents);
    World { p: parent(), c, contents, blocks, hashes }
}

type Insc = (Vec<u8>, [u8; 64]);
type Rotation = (Vec<u8>, Vec<(u16, [u8; 64])>);

impl World {
    /// Channel key `k`'s inscription of block `index` (of `blocks`).
    fn ins(&self, k: u8, blocks: &[Vec<u8>], index: usize) -> Insc {
        inscription_tx(&channel(k), &self.c.channel_id, &blocks[index], &[6; 32])
    }
    fn d(&self) -> Deployment {
        Deployment { evm_chain_id: self.c.evm_chain_id, vault: self.c.vault, lez_config_id: self.c.config_id(), channel_id: self.c.channel_id }
    }
    fn tip(&self, index: usize) -> FinalTip {
        FinalTip { tip_id: 2 + index as u64, tip_hash: self.hashes[index], final_msg_id: [0x5e; 32] }
    }
    /// Attestor `i` (channel key `i`, attestation key `i`) over block `index`, under `ksh`.
    fn by(&self, i: u8, index: usize, ksh: &Hash) -> SignedAttestation {
        let d = self.d();
        SignedAttestation::new(&att(i), pk(&channel(i)), sign_certificate(&channel(i), &d, &pk(&att(i))), &d, ksh, self.tip(index))
    }
    fn ksh(&self) -> Hash {
        key_set_hash(&self.p.keys, TIMEFRAME)
    }
    /// The standard rule (A) support: keys 1 and 2 on block 5, key 3 on block 6.
    fn support(&self) -> Vec<(u32, Insc)> {
        vec![(3, self.ins(1, &self.blocks, 3)), (3, self.ins(2, &self.blocks, 3)), (4, self.ins(3, &self.blocks, 4))]
    }
    /// The standard rule (B) attestations: keys 0, 1, 2 over T, block 5 and block 6.
    fn attestations(&self) -> Vec<SignedAttestation> {
        let ksh = self.ksh();
        vec![self.by(0, 2, &ksh), self.by(1, 3, &ksh), self.by(2, 4, &ksh)]
    }

    fn run_a(&self, rotations: &[Rotation], blocks: &[Vec<u8>], t_index: u32, producer: &Insc, support: &[(u32, Insc)]) -> Result<Outcome, Error> {
        let input = CheckpointInput {
            config: self.c.clone(),
            parent: self.p.clone(),
            config_changes: changes(rotations),
            blocks: blocks.iter().map(Vec::as_slice).collect(),
            t_index,
            echo_nonce: [0; 32],
            producer: elide(&producer.0, &producer.1),
            support: support.iter().map(|(i, s)| Support { block_index: *i, inscription: elide(&s.0, &s.1) }).collect(),
            segments: Vec::new(),
        };
        let bytes = input.encode();
        run_checkpoint(&CheckpointInput::decode(&bytes)?)
    }

    fn run_b(&self, rotations: &[Rotation], blocks: &[Vec<u8>], producer: &Insc, attestations: Vec<SignedAttestation>) -> Result<OutcomeB, Error> {
        let input = CheckpointInputB {
            config: self.c.clone(),
            parent: self.p.clone(),
            config_changes: changes(rotations),
            blocks: blocks.iter().map(Vec::as_slice).collect(),
            t_index: T_INDEX,
            producer: elide(&producer.0, &producer.1),
            attestations,
            segments: Vec::new(),
        };
        let bytes = input.encode();
        run_checkpoint_b(&CheckpointInputB::decode(&bytes)?)
    }

    /// The standard rule (A) input, accepted.
    fn baseline_a(&self) -> Result<Outcome, String> {
        self.run_a(&[], &self.blocks, T_INDEX, &self.ins(0, &self.blocks, 2), &self.support()).map_err(|e| format!("rule (A) baseline refused: {e:?}"))
    }
    fn baseline_b(&self) -> Result<OutcomeB, String> {
        self.run_b(&[], &self.blocks, &self.ins(0, &self.blocks, 2), self.attestations()).map_err(|e| format!("rule (B) baseline refused: {e:?}"))
    }
}

fn changes(rotations: &[Rotation]) -> Vec<ConfigChange<'_>> {
    rotations.iter().map(|(ops, sigs)| ConfigChange { ops, op_index: 0, signatures: sigs.iter().map(|(i, s)| (*i, s)).collect() }).collect()
}

/// A Mantle transaction with one `ChannelConfigOp` on `parent_tip` setting `keys` and
/// `posting_timeframe`, signed over its hash by `signers` (index into the current list, key).
fn rotation(c: &Config, parent_tip: &Hash, keys: &[Hash], posting_timeframe: u32, signers: &[(u16, SigningKey)]) -> Rotation {
    let mut ops = vec![1u8, OP_CHANNEL_CONFIG];
    ops.extend_from_slice(&c.channel_id);
    ops.extend_from_slice(parent_tip);
    ops.extend_from_slice(&u16::try_from(keys.len()).unwrap().to_le_bytes());
    for k in keys {
        ops.extend_from_slice(k);
    }
    ops.extend_from_slice(&posting_timeframe.to_le_bytes());
    ops.extend_from_slice(&60u32.to_le_bytes()); // posting_timeout
    ops.extend_from_slice(&3u16.to_le_bytes()); // configuration_threshold
    ops.extend_from_slice(&3u16.to_le_bytes()); // transfer_threshold
    let h = tx_hash(&ops);
    (ops, signers.iter().map(|(i, k)| (*i, k.sign(&h).to_bytes())).collect())
}

// ---- the vectors -----------------------------------------------------------------------------------

/// CKP-01: one burn transaction removed from block T's bytes before proving.
fn ckp01() -> Obs {
    let w = world(false);
    match w.baseline_a() {
        Ok(o) if o.journal.burn_count == 3 => {}
        Ok(o) => return Obs::HarnessError(format!("baseline burn_count {}", o.journal.burn_count)),
        Err(e) => return Obs::HarnessError(e),
    }
    let producer = w.ins(0, &w.blocks, 2);
    // T rebuilt with burn 3 left out (same id, parent and timestamp).
    let (rebuilt, _) = block(4, &w.hashes[1], 1_004, &w.contents[2][..1], &[]);
    // (1) As written: T's bytes without the transaction, its stored header (hash) unchanged.
    let mut literal = w.blocks.clone();
    literal[2] = rebuilt.clone();
    literal[2][40..72].copy_from_slice(&w.hashes[2]);
    let r1 = refusal(guard(|| w.run_a(&[], &literal, T_INDEX, &producer, &w.support())), "");
    // (2) The prover also rewrites T's stored hash to match its new bytes.
    let mut rehashed = w.blocks.clone();
    rehashed[2] = rebuilt;
    let r2 = refusal(guard(|| w.run_a(&[], &rehashed, T_INDEX, &producer, &w.support())), "");
    all_refused(vec![("burn 3's transaction removed from T, header kept".into(), r1), ("same, T's stored hash recomputed".into(), r2)])
}

/// CKP-02: one block between P.tip and T omitted (block 3, which carries burn 1).
fn ckp02() -> Obs {
    let w = world(false);
    if let Err(e) = w.baseline_a() {
        return Obs::HarnessError(e);
    }
    let producer = w.ins(0, &w.blocks, 2);
    let blocks: Vec<Vec<u8>> = [0usize, 2, 3, 4].iter().map(|i| w.blocks[*i].clone()).collect();
    let support = vec![(2, w.ins(1, &w.blocks, 3)), (2, w.ins(2, &w.blocks, 3)), (3, w.ins(3, &w.blocks, 4))];
    refusal(guard(|| w.run_a(&[], &blocks, 1, &producer, &support)), "blocks 2, 4 (= T), 5, 6: block 3 omitted")
}

/// CKP-03..05: a key-set change (K' = keys 0..4, n = 5, t = 4, signed by t(4) = 3 current keys),
/// with `keys` and `timeframe` in place of K' and 30, under rule (A) (support by keys 1..4 after T)
/// and rule (B) (attestations by keys 0..3 under the key-set hash the op would make live).
fn key_set_change(keys: &[Hash], timeframe: u32, what: &str) -> Obs {
    let w = world(false);
    let signers = [(0u16, channel(0)), (1, channel(1)), (2, channel(2))];
    let k5: Vec<Hash> = (0..5).map(|i| pk(&channel(i))).collect();
    let support = vec![(3, w.ins(1, &w.blocks, 3)), (3, w.ins(2, &w.blocks, 3)), (4, w.ins(3, &w.blocks, 4)), (4, w.ins(4, &w.blocks, 4))];
    let producer = w.ins(0, &w.blocks, 2);
    let atts = |ksh: &Hash| vec![w.by(0, 2, ksh), w.by(1, 3, ksh), w.by(2, 4, ksh), w.by(3, 4, ksh)];
    // Baselines: the valid change is accepted under both rules.
    let good = vec![rotation(&w.c, &w.p.config_tip_hash, &k5, TIMEFRAME, &signers)];
    if let Err(e) = w.run_a(&good, &w.blocks, T_INDEX, &producer, &support) {
        return Obs::HarnessError(format!("rule (A) baseline with the valid change refused: {e:?}"));
    }
    if let Err(e) = w.run_b(&good, &w.blocks, &producer, atts(&key_set_hash(&k5, TIMEFRAME))) {
        return Obs::HarnessError(format!("rule (B) baseline with the valid change refused: {e:?}"));
    }
    let bad = vec![rotation(&w.c, &w.p.config_tip_hash, keys, timeframe, &signers)];
    let a = refusal(guard(|| w.run_a(&bad, &w.blocks, T_INDEX, &producer, &support)), "");
    let b = refusal(guard(|| w.run_b(&bad, &w.blocks, &producer, atts(&key_set_hash(keys, timeframe)))), "");
    all_refused(vec![(format!("rule (A), {what}"), a), (format!("rule (B), {what}"), b)])
}

fn ckp03() -> Obs {
    let mut keys: Vec<Hash> = (0..5).map(|i| pk(&channel(i))).collect();
    keys.push(pk(&channel(4)));
    key_set_change(&keys, TIMEFRAME, "new key list 0,1,2,3,4,4 (4 distinct supporters other than D would meet t(6) = 4)")
}

fn ckp04() -> Obs {
    let keys: Vec<Hash> = (0..5).map(|i| pk(&channel(i))).collect();
    key_set_change(&keys, TIMEFRAME + 1, "posting_timeframe 31 where the key-set hash binds 30")
}

fn ckp05() -> Obs {
    let keys: Vec<Hash> = (0..5).map(|i| pk(&channel(i))).collect();
    key_set_change(&keys, 0, "posting_timeframe 0")
}

/// Rule (B) on the standard world: the baseline must be accepted, then `atts`/`producer` replace it.
fn rule_b_vector(note: &str, mutate: impl FnOnce(&World) -> (Insc, Vec<SignedAttestation>)) -> Obs {
    let w = world(false);
    if let Err(e) = w.baseline_b() {
        return Obs::HarnessError(e);
    }
    let (producer, atts) = mutate(&w);
    refusal(guard(|| w.run_b(&[], &w.blocks, &producer, atts)), note)
}

fn ckp06() -> Obs {
    rule_b_vector("n = 4, t = 3; certified attestations by keys 0 and 1 only", |w| {
        let ksh = w.ksh();
        (w.ins(0, &w.blocks, 2), vec![w.by(0, 2, &ksh), w.by(1, 3, &ksh)])
    })
}

fn ckp07() -> Obs {
    rule_b_vector("key 2's certificate binds vault 0x42..42 instead of the deployment's (its attestation is for this deployment)", |w| {
        let ksh = w.ksh();
        let other = Deployment { vault: [0x42; 20], ..w.d() };
        let a2 = SignedAttestation::new(&att(2), pk(&channel(2)), sign_certificate(&channel(2), &other, &pk(&att(2))), &w.d(), &ksh, w.tip(4));
        (w.ins(0, &w.blocks, 2), vec![w.by(0, 2, &ksh), w.by(1, 3, &ksh), a2])
    })
}

fn ckp08() -> Obs {
    rule_b_vector("attestations by keys 0, 1, 1 (key 1's attestation twice: three entries for t = 3)", |w| {
        let ksh = w.ksh();
        (w.ins(0, &w.blocks, 2), vec![w.by(0, 2, &ksh), w.by(1, 3, &ksh), w.by(1, 3, &ksh)])
    })
}

fn ckp09() -> Obs {
    rule_b_vector("key 2's attestation key signs SHA-256 of the final-tier message v2 instead of the message", |w| {
        let ksh = w.ksh();
        let good = w.by(2, 4, &ksh);
        let m = w.d().attestation_message(&ksh, &good.tip);
        let a2 = SignedAttestation { signature: att(2).sign(&sha256(&m)).to_bytes(), ..good };
        (w.ins(0, &w.blocks, 2), vec![w.by(0, 2, &ksh), w.by(1, 3, &ksh), a2])
    })
}

fn ckp10() -> Obs {
    rule_b_vector("T inscribed by channel key 4, outside K; t valid attestations", |w| (w.ins(4, &w.blocks, 2), w.attestations()))
}

/// CKP-11: rule (A), n = 4, t = 3: support from D (key 0, T's producer) and keys 1 and 2 only.
fn ckp11() -> Obs {
    let w = world(false);
    if let Err(e) = w.baseline_a() {
        return Obs::HarnessError(e);
    }
    let support = vec![(3, w.ins(0, &w.blocks, 3)), (3, w.ins(1, &w.blocks, 3)), (4, w.ins(2, &w.blocks, 4))];
    let producer = w.ins(0, &w.blocks, 2);
    refusal(guard(|| w.run_a(&[], &w.blocks, T_INDEX, &producer, &support)), "support: D (key 0) on block 5, key 1 on block 5, key 2 on block 6")
}

/// CKP-12 (control): block 3 also carries a burn of another configuration; accepted, and that burn
/// changes neither burnCount nor the burn root.
fn ckp12() -> Obs {
    let base = match world(false).baseline_a() {
        Ok(o) => o,
        Err(e) => return Obs::HarnessError(e),
    };
    let w = world(true);
    let out = match guard(|| w.run_a(&[], &w.blocks, T_INDEX, &w.ins(0, &w.blocks, 2), &w.support())) {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => return Obs::Refused { error: format!("{e:?}"), note: "with the foreign burn in block 3".into() },
        Err(m) => return Obs::Panic(m),
    };
    // The root of burns 1, 2, 3 of this configuration, computed here.
    let id = w.c.config_id();
    let mut f = Frontier::default();
    for (index, i) in [(0u64, 1u8), (1, 2), (2, 3)] {
        let b = burn(&id, i);
        f.append(burn_leaf(&id, index, &b.token, &b.amount, &b.c_out, &b.r)).unwrap();
    }
    let root = f.root(&zero_hashes());
    let j = &out.journal;
    let note = format!(
        "accepted; burnCount {} (without the foreign burn {}), burn root {} the 3-burn root of this configuration",
        j.burn_count,
        base.journal.burn_count,
        if j.burn_root == root { "equals" } else { "DIFFERS FROM" }
    );
    if j.burn_count == base.journal.burn_count && j.burn_count == 3 && j.burn_root == root && base.journal.burn_root == root {
        Obs::Accepted(note)
    } else {
        Obs::EffectMismatch(note)
    }
}

#[test]
fn redteam_checkpoint() {
    let mut r = Results::new();
    r.run("CKP-01", ckp01);
    r.run("CKP-02", ckp02);
    r.run("CKP-03", ckp03);
    r.run("CKP-04", ckp04);
    r.run("CKP-05", ckp05);
    r.run("CKP-06", ckp06);
    r.run("CKP-07", ckp07);
    r.run("CKP-08", ckp08);
    r.run("CKP-09", ckp09);
    r.run("CKP-10", ckp10);
    r.run("CKP-11", ckp11);
    r.run("CKP-12", ckp12);
    r.finish("checkpoint");
}
