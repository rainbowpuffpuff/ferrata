//! The methods of `ferrata_attest_module` as plain functions over JSON strings, so that they run and
//! are tested without the module scaffold. Every function returns one JSON object. A request that
//! cannot be read returns `{"verdict":"invalid_request","error":...}`, which carries no program code.
//!
//! The verification is ferrata-core's, the crate the attestation program runs: a dry run builds the
//! record the program's way and proves the fact with the same functions, so its verdict and code are
//! the program's (`lez/wire/src/lib.rs`: a core refusal's code is the program's exit code).

use std::fmt::Write as _;

use ferrata_core::{
    attest::{attest_log, attest_storage, HeaderRecord},
    exec::{Address, Word},
    fork::{ChainSpec, LcFork, Preset},
    merkle::ZERO,
    store::StoreCore,
    types::{BeaconBlockHeader, LightClientBootstrap},
    witness::{self, ExecutionSummary},
    Error, Root,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// What `scripts/stage.sh` recorded about the ferrata-core copy this crate was built against.
const STAGED: &str = include_str!("../../ferrata-core/STAGED.json");

/// `version()`: the module's version and the ferrata-core it was built from.
pub fn version() -> String {
    json!({
        "module": "ferrata_attest_module",
        "version": env!("CARGO_PKG_VERSION"),
        "ferrata_core": serde_json::from_str::<Value>(STAGED).unwrap_or(Value::Null),
        "bls_backend": "none",
        "methods": ["version", "dry_run_attest", "verify_checkpoint"],
    })
    .to_string()
}

/// `dry_run_attest(request)`: prove one storage slot or one log against a header record.
pub fn dry_run_attest(request: &str) -> String {
    reply(dry_run(request))
}

/// `verify_checkpoint(request)`: instance id, bootstrap and observed roots of a checkpoint.
pub fn verify_checkpoint(request: &str) -> String {
    reply(checkpoint(request))
}

fn reply(result: Result<Value, String>) -> String {
    result.unwrap_or_else(|error| json!({ "verdict": "invalid_request", "error": error })).to_string()
}

fn refused(error: Error, stage: &str) -> Value {
    json!({ "verdict": "refused", "code": error.code(), "error": format!("{error:?}"), "stage": stage })
}

// ---------------------------------------------------------------------------------------------
// dry_run_attest
// ---------------------------------------------------------------------------------------------

fn dry_run(request: &str) -> Result<Value, String> {
    let req: Value = serde_json::from_str(request).map_err(|e| format!("request is not JSON: {e}"))?;
    let chain_id = uint(&req["chain_id"], "chain_id")?;

    // The record as the program stores it for a finalized header (`header(id, slot)`): the beacon
    // header and execution root are not used by `from_recorded`, which reads the slot and the
    // execution payload fields, so they are left zero.
    let r = &req["record"];
    let instance_chain_id = uint(&r["chain_id"], "record.chain_id")?;
    let recorded = witness::HeaderRecord {
        beacon: BeaconBlockHeader {
            slot: uint(&r["anchor_slot"], "record.anchor_slot")?,
            proposer_index: 0,
            parent_root: ZERO,
            state_root: ZERO,
            body_root: ZERO,
        },
        execution_root: ZERO,
        execution: ExecutionSummary::Payload {
            block_number: uint(&r["block_number"], "record.block_number")?,
            block_hash: fixed(&r["block_hash"], "record.block_hash")?,
            parent_hash: fixed(&r["parent_hash"], "record.parent_hash")?,
            state_root: fixed(&r["state_root"], "record.state_root")?,
            receipts_root: fixed(&r["receipts_root"], "record.receipts_root")?,
            timestamp: uint(&r["timestamp"], "record.timestamp")?,
        },
    };
    let record = match HeaderRecord::from_recorded(&recorded, instance_chain_id) {
        Ok(record) => record,
        Err(e) => return Ok(refused(e, "record")),
    };

    let mut out = if !req["storage"].is_null() {
        storage(&record, chain_id, &req["storage"])?
    } else if !req["log"].is_null() {
        log(&record, chain_id, &req["log"])?
    } else {
        return Err("the request needs `storage` or `log`".into());
    };
    out["record"] = json!({
        "chain_id": record.chain_id(),
        "anchor_slot": record.anchor_slot(),
        "block_number": record.number(),
        "block_hash": hex(&record.block_hash()),
        "state_root": hex(&record.state_root()),
        "receipts_root": hex(&record.receipts_root()),
    });
    Ok(out)
}

/// `storage`: an `eth_getProof` result (`get_proof`, or its fields directly) and the `slot`.
fn storage(record: &HeaderRecord, chain_id: u64, s: &Value) -> Result<Value, String> {
    let p = if s["get_proof"].is_null() { s } else { &s["get_proof"] };
    let address: Address = fixed(&p["address"], "storage.address")?;
    let account_proof = nodes(&p["accountProof"], "storage.accountProof")?;
    let slot = word(&s["slot"], "storage.slot")?;
    let entries = p["storageProof"].as_array().ok_or("storage.storageProof: expected an array")?;
    let mut entry = None;
    for (i, e) in entries.iter().enumerate() {
        if word(&e["key"], &format!("storage.storageProof[{i}].key"))? == slot {
            entry = Some(e);
            break;
        }
    }
    let entry = entry.ok_or_else(|| format!("storage.storageProof has no entry for slot {}", hex(&slot)))?;
    let storage_proof = nodes(&entry["proof"], "storage.storageProof[].proof")?;
    // What the provider said the value is; the reply says whether the proof agrees.
    let claimed = word(&entry["value"], "storage.storageProof[].value").ok();

    Ok(match attest_storage(record, chain_id, &address, &account_proof, &slot, &storage_proof) {
        Ok(a) => json!({
            "verdict": "attested",
            "code": 0,
            "fact": {
                "kind": "storage",
                "address": hex(&a.fact.address),
                "slot": hex(&a.fact.slot),
                "value": hex(&a.fact.value),
            },
            "provider_value_matches": claimed.map(|c| c == a.fact.value),
            "chain_id": a.chain_id,
            "block_number": a.block_number,
            "block_hash": hex(&a.block_hash),
            "id": hex(&a.id),
            "id_at_block": hex(&a.id_at_block()),
        }),
        Err(e) => refused(e, "proof"),
    })
}

/// `log`: the transaction's index in the block, the log's index within its receipt, and the
/// receipt's proof in the block's receipts trie.
fn log(record: &HeaderRecord, chain_id: u64, l: &Value) -> Result<Value, String> {
    let tx_index = uint(&l["tx_index"], "log.tx_index")?;
    let log_index = uint(&l["log_index"], "log.log_index")?;
    let proof = nodes(&l["receipt_proof"], "log.receipt_proof")?;
    Ok(match attest_log(record, chain_id, tx_index, log_index, &proof) {
        Ok(a) => json!({
            "verdict": "attested",
            "code": 0,
            "fact": {
                "kind": "log",
                "tx_index": a.fact.tx_index,
                "log_index": a.fact.log_index,
                "address": hex(&a.fact.log.address),
                "topics": a.fact.log.topics().iter().map(|t| hex(t)).collect::<Vec<_>>(),
                "data": hex(a.fact.log.data),
            },
            "chain_id": a.chain_id,
            "block_number": a.block_number,
            "block_hash": hex(&a.block_hash),
            "id": hex(&a.id),
        }),
        Err(e) => refused(e, "proof"),
    })
}

// ---------------------------------------------------------------------------------------------
// verify_checkpoint
// ---------------------------------------------------------------------------------------------

/// The program's instance configuration (`Config` in `lez/wire/src/lib.rs`), field for field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub chain_id: u64,
    pub sync_committee_size: u32,
    pub slots_per_epoch: u64,
    pub epochs_per_sync_committee_period: u64,
    pub min_sync_committee_participants: u64,
    pub fork_versions: [[u8; 4]; 9],
    pub fork_epochs: [u64; 9],
    pub genesis_validators_root: Root,
    pub genesis_time: u64,
    pub seconds_per_slot: u64,
    pub store_fork: u8,
    pub adoption_parts: u8,
}

impl Config {
    /// From the JSON object of the same field names (byte strings in hex).
    pub fn from_json(c: &Value) -> Result<Self, String> {
        let versions = c["fork_versions"].as_array().ok_or("config.fork_versions: expected an array")?;
        let epochs = c["fork_epochs"].as_array().ok_or("config.fork_epochs: expected an array")?;
        if versions.len() != 9 || epochs.len() != 9 {
            return Err("config.fork_versions and config.fork_epochs need 9 entries each".into());
        }
        let mut fork_versions = [[0u8; 4]; 9];
        let mut fork_epochs = [0u64; 9];
        for i in 0..9 {
            fork_versions[i] = fixed(&versions[i], &format!("config.fork_versions[{i}]"))?;
            fork_epochs[i] = uint(&epochs[i], &format!("config.fork_epochs[{i}]"))?;
        }
        let narrow = |v: u64, what: &str, max: u64| {
            if v <= max { Ok(v) } else { Err(format!("config.{what}: at most {max}")) }
        };
        Ok(Self {
            chain_id: uint(&c["chain_id"], "config.chain_id")?,
            sync_committee_size: narrow(uint(&c["sync_committee_size"], "config.sync_committee_size")?, "sync_committee_size", u32::MAX as u64)? as u32,
            slots_per_epoch: uint(&c["slots_per_epoch"], "config.slots_per_epoch")?,
            epochs_per_sync_committee_period: uint(&c["epochs_per_sync_committee_period"], "config.epochs_per_sync_committee_period")?,
            min_sync_committee_participants: uint(&c["min_sync_committee_participants"], "config.min_sync_committee_participants")?,
            fork_versions,
            fork_epochs,
            genesis_validators_root: fixed(&c["genesis_validators_root"], "config.genesis_validators_root")?,
            genesis_time: uint(&c["genesis_time"], "config.genesis_time")?,
            seconds_per_slot: uint(&c["seconds_per_slot"], "config.seconds_per_slot")?,
            store_fork: narrow(uint(&c["store_fork"], "config.store_fork")?, "store_fork", 255)? as u8,
            adoption_parts: narrow(uint(&c["adoption_parts"], "config.adoption_parts")?, "adoption_parts", 255)? as u8,
        })
    }

    /// The Borsh encoding `#[derive(BorshSerialize)]` gives `Config`: the fields in order, integers
    /// little-endian, fixed-size arrays element by element with no length.
    pub fn borsh(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(194);
        out.extend_from_slice(&self.chain_id.to_le_bytes());
        out.extend_from_slice(&self.sync_committee_size.to_le_bytes());
        out.extend_from_slice(&self.slots_per_epoch.to_le_bytes());
        out.extend_from_slice(&self.epochs_per_sync_committee_period.to_le_bytes());
        out.extend_from_slice(&self.min_sync_committee_participants.to_le_bytes());
        for v in &self.fork_versions {
            out.extend_from_slice(v);
        }
        for e in &self.fork_epochs {
            out.extend_from_slice(&e.to_le_bytes());
        }
        out.extend_from_slice(&self.genesis_validators_root);
        out.extend_from_slice(&self.genesis_time.to_le_bytes());
        out.extend_from_slice(&self.seconds_per_slot.to_le_bytes());
        out.push(self.store_fork);
        out.push(self.adoption_parts);
        out
    }

    /// The core's chain configuration (`Config::spec` in the program).
    pub fn spec(&self) -> ChainSpec {
        ChainSpec {
            preset: Preset {
                sync_committee_size: self.sync_committee_size as usize,
                slots_per_epoch: self.slots_per_epoch,
                epochs_per_sync_committee_period: self.epochs_per_sync_committee_period,
                min_sync_committee_participants: self.min_sync_committee_participants,
            },
            versions: self.fork_versions,
            epochs: self.fork_epochs,
        }
    }

    /// The store's layout (`Config::fork` in the program).
    pub fn fork(&self) -> Option<LcFork> {
        Some(match self.store_fork {
            0 => LcFork::Altair,
            1 => LcFork::Capella,
            2 => LcFork::Deneb,
            3 => LcFork::Electra,
            4 => LcFork::Gloas,
            _ => return None,
        })
    }
}

/// `H(config ‖ checkpoint)`, as the program derives an instance id (`instance_id` in
/// `lez/wire/src/lib.rs`): SHA-256 over `ferrata-lez/v1/`, `instance-id`, Borsh of the config and
/// the checkpoint root.
pub fn instance_id(config: &Config, checkpoint: &Root) -> Root {
    let mut h = Sha256::new();
    h.update(b"ferrata-lez/v1/");
    h.update(b"instance-id");
    h.update(config.borsh());
    h.update(checkpoint);
    h.finalize().into()
}

/// Execution chain ids and the `genesis_validators_root` of their beacon chains (from this
/// repository: `lez/harness/src/lib.rs`, `crates/ferrata-spectests/tests/sepolia_gloas.rs`).
const KNOWN_CHAINS: [(&str, u64, &str); 2] = [
    ("mainnet", 1, "4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95"),
    ("sepolia", 11_155_111, "d8ea171f3c94aea21ebc42a1ed61052acf3f9209c00e4efbaaddac09ed9b8078"),
];

fn chain_pair(chain_id: u64, gvr: &Root) -> Value {
    let gvr_hex = hex(gvr);
    for (name, id, root) in KNOWN_CHAINS {
        let same_id = id == chain_id;
        let same_root = gvr_hex[2..] == *root;
        if same_id && same_root {
            return json!({ "status": "known", "network": name });
        }
        if same_id || same_root {
            return json!({ "status": "mismatch", "network": name });
        }
    }
    json!({ "status": "unknown" })
}

fn checkpoint(request: &str) -> Result<Value, String> {
    let req: Value = serde_json::from_str(request).map_err(|e| format!("request is not JSON: {e}"))?;
    let config = Config::from_json(&req["config"])?;
    let checkpoint: Root = fixed(&req["checkpoint"], "checkpoint")?;
    let id = instance_id(&config, &checkpoint);
    let mut failed: Vec<&str> = Vec::new();

    let expected = if req["expected_instance_id"].is_null() {
        Value::Null
    } else {
        let same = fixed::<32>(&req["expected_instance_id"], "expected_instance_id")? == id;
        if !same {
            failed.push("instance_id");
        }
        json!(same)
    };

    let pair = chain_pair(config.chain_id, &config.genesis_validators_root);
    if pair["status"] == "mismatch" {
        failed.push("chain_pair");
    }

    // The checks of the program's `Init` on the bootstrap, against this checkpoint.
    let bootstrap = if req["bootstrap"].is_null() {
        Value::Null
    } else {
        let bytes = unhex(text(&req["bootstrap"], "bootstrap")?).map_err(|e| format!("bootstrap: {e}"))?;
        let verdict = match config.fork() {
            None => refused(Error::ForkMismatch, "config"),
            Some(fork) => match LightClientBootstrap::decode(&bytes, fork, config.sync_committee_size as usize) {
                Err(e) => refused(e, "decode"),
                Ok(b) => {
                    let mut v = match StoreCore::from_bootstrap(&checkpoint, &b, fork, &config.spec()) {
                        Ok(_) => json!({ "verdict": "accepted", "code": 0 }),
                        Err(e) => refused(e, "init"),
                    };
                    v["slot"] = json!(b.header.beacon.slot);
                    v["header_root"] = json!(hex(&b.header.beacon.hash_tree_root()));
                    v
                }
            },
        };
        if verdict["code"] != 0 {
            failed.push("bootstrap");
        }
        verdict
    };

    // Roots that other sources report for the checkpoint, given in the call (no network here).
    let mut observed = Vec::new();
    let mut agreeing = 0u64;
    if let Some(list) = req["observed"].as_array() {
        for (i, o) in list.iter().enumerate() {
            let root: Root = fixed(&o["root"], &format!("observed[{i}].root"))?;
            agreeing += u64::from(root == checkpoint);
            observed.push(json!({ "source": o["source"].clone(), "root": hex(&root), "agrees": root == checkpoint }));
        }
    }
    let required = if req["required_agreeing"].is_null() {
        observed.len() as u64
    } else {
        uint(&req["required_agreeing"], "required_agreeing")?
    };
    if agreeing < required {
        failed.push("observed");
    }

    Ok(json!({
        "verdict": if failed.is_empty() { "ok" } else { "failed" },
        "failed": failed,
        "instance_id": hex(&id),
        "instance_id_matches": expected,
        "checkpoint": hex(&checkpoint),
        "chain_pair": pair,
        "bootstrap": bootstrap,
        "observed": observed,
        "observed_agreeing": agreeing,
        "required_agreeing": required,
    }))
}

// ---------------------------------------------------------------------------------------------
// JSON and hex
// ---------------------------------------------------------------------------------------------

pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(2 + 2 * bytes.len());
    s.push_str("0x");
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

fn strip0x(s: &str) -> &str {
    s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s)
}

pub fn unhex(s: &str) -> Result<Vec<u8>, String> {
    let t = strip0x(s.trim());
    if !t.is_ascii() || t.len() % 2 != 0 {
        return Err(format!("not an even-length hex string ({} characters)", t.len()));
    }
    (0..t.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&t[i..i + 2], 16).map_err(|_| format!("not hex at character {i}")))
        .collect()
}

fn text<'a>(v: &'a Value, what: &str) -> Result<&'a str, String> {
    match v {
        Value::String(s) => Ok(s),
        Value::Null => Err(format!("{what}: missing")),
        _ => Err(format!("{what}: expected a string")),
    }
}

/// A byte string of exactly `N` bytes, in hex.
fn fixed<const N: usize>(v: &Value, what: &str) -> Result<[u8; N], String> {
    let bytes = unhex(text(v, what)?).map_err(|e| format!("{what}: {e}"))?;
    let len = bytes.len();
    bytes.try_into().map_err(|_| format!("{what}: expected {N} bytes, got {len}"))
}

/// A 256-bit word in hex, at most 32 bytes, left-padded (`eth_getProof` quantities such as `0x0`).
fn word(v: &Value, what: &str) -> Result<Word, String> {
    let s = strip0x(text(v, what)?.trim());
    let padded = if s.len() % 2 == 1 { format!("0{s}") } else { s.to_string() };
    let bytes = unhex(&padded).map_err(|e| format!("{what}: {e}"))?;
    if bytes.len() > 32 {
        return Err(format!("{what}: more than 32 bytes"));
    }
    let mut w = [0u8; 32];
    w[32 - bytes.len()..].copy_from_slice(&bytes);
    Ok(w)
}

/// An unsigned 64-bit integer: a JSON number, a decimal string or a `0x` quantity.
fn uint(v: &Value, what: &str) -> Result<u64, String> {
    match v {
        Value::Number(n) => n.as_u64().ok_or_else(|| format!("{what}: not an unsigned 64-bit integer")),
        Value::String(s) => match s.strip_prefix("0x") {
            Some(h) => u64::from_str_radix(h, 16),
            None => s.parse::<u64>(),
        }
        .map_err(|_| format!("{what}: not an unsigned 64-bit integer: {s}")),
        Value::Null => Err(format!("{what}: missing")),
        _ => Err(format!("{what}: expected a number")),
    }
}

/// A list of trie nodes in hex.
fn nodes(v: &Value, what: &str) -> Result<Vec<Vec<u8>>, String> {
    v.as_array()
        .ok_or_else(|| format!("{what}: expected an array of hex strings"))?
        .iter()
        .enumerate()
        .map(|(i, n)| unhex(text(n, &format!("{what}[{i}]"))?).map_err(|e| format!("{what}[{i}]: {e}")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `mainnet_config(2)` of `lez/harness/src/lib.rs`.
    fn mainnet(parts: u8) -> Config {
        let mut fork_versions = [[0u8; 4]; 9];
        for (i, v) in fork_versions.iter_mut().enumerate() {
            v[0] = i as u8;
        }
        Config {
            chain_id: 1,
            sync_committee_size: 512,
            slots_per_epoch: 32,
            epochs_per_sync_committee_period: 256,
            min_sync_committee_participants: 1,
            fork_versions,
            fork_epochs: [0, 74_240, 144_896, 194_048, 269_568, 364_032, 411_392, u64::MAX, u64::MAX],
            genesis_validators_root: fixed(&json!(format!("0x{}", KNOWN_CHAINS[0].2)), "gvr").unwrap(),
            genesis_time: 1_606_824_023,
            seconds_per_slot: 12,
            store_fork: 3,
            adoption_parts: parts,
        }
    }

    #[test]
    fn config_borsh_is_194_bytes() {
        assert_eq!(mainnet(2).borsh().len(), 8 + 4 + 8 + 8 + 8 + 36 + 72 + 32 + 8 + 8 + 1 + 1);
    }

    /// The instance of `lez/harness` `World::new(2)` (mainnet periods 1899–1903), whose id the
    /// program wrote into the transactions of `lez/results/stage3-*-tx.hex` (prefix in
    /// `lez/results/prove-private-dry-run.log`).
    #[test]
    fn instance_id_equals_the_programs() {
        let checkpoint = fixed(&json!("0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac"), "c").unwrap();
        assert_eq!(
            hex(&instance_id(&mainnet(2), &checkpoint)),
            "0xce624027489c721108da1cbaeb5eb16990111f6908282949e05108e7cb2afdfb"
        );
    }

    #[test]
    fn hex_and_words() {
        assert_eq!(unhex("0x00ff").unwrap(), vec![0, 255]);
        assert!(unhex("0x0").is_err());
        assert!(unhex("0xzz").is_err());
        assert!(unhex("0xé1").is_err());
        assert_eq!(word(&json!("0x0"), "w").unwrap(), [0u8; 32]);
        assert_eq!(word(&json!("0x1ea6feb"), "w").unwrap()[28..], [0x01, 0xea, 0x6f, 0xeb]);
        assert_eq!(uint(&json!("0x6af31dd7"), "u").unwrap(), 0x6af3_1dd7);
        assert_eq!(uint(&json!("18446744073709551615"), "u").unwrap(), u64::MAX);
    }

    #[test]
    fn unreadable_requests_are_not_program_codes() {
        let v: Value = serde_json::from_str(&dry_run_attest("{")).unwrap();
        assert_eq!(v["verdict"], "invalid_request");
        let v: Value = serde_json::from_str(&verify_checkpoint("{}")).unwrap();
        assert_eq!(v["verdict"], "invalid_request");
    }

    #[test]
    fn chain_pairs() {
        let mainnet_gvr = mainnet(2).genesis_validators_root;
        assert_eq!(chain_pair(1, &mainnet_gvr)["status"], "known");
        assert_eq!(chain_pair(11_155_111, &mainnet_gvr)["status"], "mismatch");
        assert_eq!(chain_pair(5, &[7u8; 32])["status"], "unknown");
    }
}
