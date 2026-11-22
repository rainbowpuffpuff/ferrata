//! The methods of `ferrata_attest_module` as plain functions over JSON strings, so that they run and
//! are tested without the module scaffold. Every function returns one JSON object. A request that
//! cannot be read returns `{"verdict":"invalid_request","error":...}`, which carries no program code.
//!
//! The verification is ferrata-core's, the crate the attestation program runs: a dry run builds the
//! record the program's way and proves the fact with the same functions, so its verdict and code are
//! the program's (`lez/wire/src/lib.rs`: a core refusal's code is the program's exit code).

use std::fmt::Write as _;

use ferrata_core::{
    attest::{attest_account, attest_storage, attest_log, HeaderRecord},
    bls::{Aggregation, BlsBackend, Blst},
    exec::{Address, Word},
    fork::{ChainSpec, LcFork, Preset},
    merkle::ZERO,
    store::StoreCore,
    types::{BeaconBlockHeader, Execution, LightClientBootstrap, SyncCommittee},
    witness::{self, ExecutionSummary},
    Error, Root,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

mod advance;
mod lez;
pub use lez::{set_lez_core, LezCoreCall};
pub mod providers;
mod rpc;
pub use rpc::{set_eth_rpc_module, EthRpcModuleCall};
mod trie;
mod tx;

/// What `scripts/stage.py` recorded about the crates staged beside this one (the commit, LEZ's, each
/// crate's version and sources' SHA-256).
const STAGED: &str = include_str!("../../staged/STAGED.json");

/// `version()`: the module's version and the ferrata-core it was built from.
pub fn version() -> String {
    json!({
        "module": "ferrata_attest_module",
        "version": env!("CARGO_PKG_VERSION"),
        "staged": serde_json::from_str::<Value>(STAGED).unwrap_or(Value::Null),
        "bls_backend": "blst (advance runs the program's steps natively)",
        "tool_code": "ferrata-advance (the updater's job planning)",
        "methods": [
            "version",
            "dry_run_attest",
            "verify_checkpoint",
            "fetch_log_proof",
            "build_attest_tx",
            "read_attestation",
            "read_instance",
            "advance",
            "lez",
            "lez_read",
            "lez_submit"
        ],
        "logos_core_modules_called": ["eth_rpc_module (raw_rpc)", "beacon_api_module (list_providers, raw_get)", "lez_core"],
    })
    .to_string()
}

/// `advance(request)`: a light-client update planned as the updater plans it: whether it moves the
/// instance, its job, and the transactions of the job's steps (`Open`, `Miller`, `Adopt`, `Finish`).
pub fn advance(request: &str) -> String {
    reply(advance::advance_of(request))
}

/// `dry_run_attest(request)`: prove one storage slot or one log against a header record.
pub fn dry_run_attest(request: &str) -> String {
    reply(dry_run(request))
}

/// `verify_checkpoint(request)`: a checkpoint's instance id, its bootstrap (the program's `Init` checks and the full
/// `KeyValidate` of its committee), the roots given in the request, and, fetched, what several beacon providers and an
/// execution endpoint hold for it (`providers.rs`).
pub fn verify_checkpoint(request: &str) -> String {
    reply(checkpoint(request, None))
}

/// [`verify_checkpoint`] over the given HTTP client (a recording, a replay) instead of `curl`.
pub fn verify_checkpoint_with(request: &str, http: &mut dyn providers::Http) -> String {
    reply(checkpoint(request, Some(http)))
}

/// `fetch_log_proof(request)`: read a block and its receipts from a JSON-RPC endpoint, rebuild the
/// receipts trie, check it against the block's `receiptsRoot`, and return one log's receipt proof
/// in the form `dry_run_attest` and `build_attest_tx` take.
pub fn fetch_log_proof(request: &str) -> String {
    reply(fetch_log(request))
}

/// `build_attest_tx(request)`: the attestation program's `Attest` message for a log (or a storage
/// slot) at `header(id, slot)`, and the account it is sent to.
pub fn build_attest_tx(request: &str) -> String {
    reply(build_tx(request))
}

/// `read_attestation(request)`: decode the program's `Attested` event and compare its statement
/// ids with the ones a dry run predicted.
pub fn read_attestation(request: &str) -> String {
    reply(read_event(request))
}

/// `read_instance(request)`: an instance's state as LEZ holds it at `instance(id)` (the program's
/// `InstanceState`, Borsh): its id, configuration, activation, store and jobs applied.
pub fn read_instance(request: &str) -> String {
    reply(instance(request))
}

/// `lez(request)`: open (or close) the path through Logos Core's own LEZ module, `lez_core` (`lez.rs`).
pub fn lez(request: &str) -> String {
    reply(serde_json::from_str::<Value>(request).map_err(|e| format!("request: {e}")).and_then(|r| lez::lez(&r)))
}

/// `lez_read(request)`: an instance, a record or a job read through `lez_core.get_account_public` (`lez.rs`).
pub fn lez_read(request: &str) -> String {
    reply(serde_json::from_str::<Value>(request).map_err(|e| format!("request: {e}")).and_then(|r| lez::lez_read(&r)))
}

/// `lez_submit(request)`: the steps of `advance`, or `build_attest_tx`'s transaction, sent through
/// `lez_core.send_generic_public_transaction`, each waited for with `poll_transaction_status` (`lez.rs`).
pub fn lez_submit(request: &str) -> String {
    reply(serde_json::from_str::<Value>(request).map_err(|e| format!("request: {e}")).and_then(|r| lez::lez_submit(&r)))
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
    // Either the record as the program stores it (`RecordState::record`, read from the chain), or
    // its fields.
    let recorded = if !r["encoded"].is_null() {
        let bytes = unhex(text(&r["encoded"], "record.encoded")?).map_err(|e| format!("record.encoded: {e}"))?;
        match witness::HeaderRecord::decode(&bytes) {
            Ok(h) => h,
            Err(e) => return Ok(refused(e, "record")),
        }
    } else {
        record_from_fields(r)?
    };
    let record = match HeaderRecord::from_recorded(&recorded, instance_chain_id) {
        Ok(record) => record,
        Err(e) => return Ok(refused(e, "record")),
    };

    let mut out = if !req["storage"].is_null() {
        // The proof given, or read at the record's block through the path the storage names.
        let (s, read) = storage_with_proof(&req["storage"], Some(record.number()))?;
        let mut o = storage(&record, chain_id, &s)?;
        if let Some(r) = read {
            o["eth"] = r;
        }
        o
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

/// The record's fields as JSON (`chain_id` aside): the slot and the execution payload's fields.
fn record_from_fields(r: &Value) -> Result<witness::HeaderRecord, String> {
    Ok(witness::HeaderRecord {
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
    })
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
    // The program attests an account query's account as a fact of its own, before its slots: the
    // statement ids it emits for this request are the account's, then the slot's.
    let account = attest_account(record, chain_id, &address, &account_proof).ok();

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
            "account_id": account.as_ref().map(|f| hex(&f.id)),
            "ids": account.iter().map(|f| hex(&f.id)).chain(std::iter::once(hex(&a.id))).collect::<Vec<_>>(),
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
            "ids": [hex(&a.id)],
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
    /// The weak-subjectivity period of the finalized header, in epochs (nonzero; part of the instance id).
    pub ws_period_epochs: u64,
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
            ws_period_epochs: uint(&c["ws_period_epochs"], "config.ws_period_epochs")?,
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
        out.extend_from_slice(&self.ws_period_epochs.to_le_bytes());
        out
    }

    /// The inverse of [`Config::borsh`].
    fn read(r: &mut Bytes) -> Result<Self, String> {
        let chain_id = r.u64()?;
        let sync_committee_size = r.u32()?;
        let slots_per_epoch = r.u64()?;
        let epochs_per_sync_committee_period = r.u64()?;
        let min_sync_committee_participants = r.u64()?;
        let mut fork_versions = [[0u8; 4]; 9];
        for v in fork_versions.iter_mut() {
            *v = r.array::<4>()?;
        }
        let mut fork_epochs = [0u64; 9];
        for e in fork_epochs.iter_mut() {
            *e = r.u64()?;
        }
        Ok(Self {
            chain_id,
            sync_committee_size,
            slots_per_epoch,
            epochs_per_sync_committee_period,
            min_sync_committee_participants,
            fork_versions,
            fork_epochs,
            genesis_validators_root: r.array::<32>()?,
            genesis_time: r.u64()?,
            seconds_per_slot: r.u64()?,
            store_fork: r.u8()?,
            adoption_parts: r.u8()?,
            ws_period_epochs: r.u64()?,
        })
    }

    /// The JSON object [`Config::from_json`] reads.
    pub fn to_json(&self) -> Value {
        json!({
            "chain_id": self.chain_id,
            "sync_committee_size": self.sync_committee_size,
            "slots_per_epoch": self.slots_per_epoch,
            "epochs_per_sync_committee_period": self.epochs_per_sync_committee_period,
            "min_sync_committee_participants": self.min_sync_committee_participants,
            "fork_versions": self.fork_versions.iter().map(|v| hex(v)).collect::<Vec<_>>(),
            "fork_epochs": self.fork_epochs.to_vec(),
            "genesis_validators_root": hex(&self.genesis_validators_root),
            "genesis_time": self.genesis_time,
            "seconds_per_slot": self.seconds_per_slot,
            "store_fork": self.store_fork,
            "adoption_parts": self.adoption_parts,
            "ws_period_epochs": self.ws_period_epochs,
        })
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

fn checkpoint(request: &str, http: Option<&mut dyn providers::Http>) -> Result<Value, String> {
    let req: Value = serde_json::from_str(request).map_err(|e| format!("request is not JSON: {e}"))?;
    let config = Config::from_json(&req["config"])?;
    let checkpoint: Root = fixed(&req["checkpoint"], "checkpoint")?;
    let id = instance_id(&config, &checkpoint);
    let mut failed: Vec<&str> = Vec::new();

    let expected_id = if req["expected_instance_id"].is_null() {
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

    // The checks of the program's `Init` on the bootstrap, against this checkpoint. An accepted bootstrap gives the
    // checkpoint's slot and execution block to the provider checks below.
    let mut accepted: Option<LightClientBootstrap> = None;
    let mut decoded_committee: Option<SyncCommittee> = None;
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
                    decoded_committee = Some(b.current_sync_committee.clone());
                    if v["code"] == 0 {
                        accepted = Some(b);
                    }
                    v
                }
            },
        };
        if verdict["code"] != 0 {
            failed.push("bootstrap");
        }
        verdict
    };

    // The full `KeyValidate` on the bootstrap's committee (every key canonical, on the curve, not the identity, in the
    // prime-order subgroup; their sum the aggregate), which the program leaves to off-chain tools (attestation.md
    // §4.3). On by default; `"key_validate": false` skips it.
    let committee = match (&decoded_committee, req["key_validate"].as_bool().unwrap_or(true)) {
        (Some(c), true) => {
            let v = committee_check(c);
            if v["code"] != 0 {
                failed.push("committee");
            }
            v
        }
        _ => Value::Null,
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

    // What several beacon providers hold for the checkpoint, fetched (`providers`: base URLs, or {name, url}); at
    // least `min_providers` of them (3 by default when any is given), on distinct hosts, must agree and none disagree.
    let timeout = req["timeout_secs"].as_u64().unwrap_or(20);
    let mut curl = providers::Curl { timeout_secs: timeout };
    let http: &mut dyn providers::Http = match http {
        Some(h) => h,
        None => &mut curl,
    };
    let expected = providers::Expected {
        checkpoint: &checkpoint,
        slot: accepted.as_ref().map(|b| b.header.beacon.slot),
        config: &config,
    };
    let mut provider_results = Vec::new();
    if let Some(list) = req["providers"].as_array() {
        for (i, p) in list.iter().enumerate() {
            let (name, url) = endpoint(p, &format!("providers[{i}]"))?;
            provider_results.push(providers::beacon(http, &name, &url, &expected));
        }
    } else if req["providers"]["via"].as_str() == Some("beacon_api_module") {
        // Through Logos Core's `beacon_api_module`: its providers for the chain (`list_providers`), or the ones `ids`
        // names, each asked pinned; a provider is counted by its endpoint's host, and one whose GETs another provider
        // answered is not counted.
        let p = &req["providers"];
        let chain_id = p["chain_id"].as_i64().ok_or("providers.chain_id: the chain beacon_api_module serves")?;
        let ms = timeout.saturating_mul(1000);
        let listed = beacon_api_call("list_providers", json!([chain_id]), ms)?;
        let all: Vec<(String, String)> = listed["providers"]
            .as_array()
            .ok_or_else(|| format!("beacon_api_module.list_providers: {listed}"))?
            .iter()
            .filter(|x| x["enabled"] != false)
            .filter_map(|x| Some((x["id"].as_str()?.to_owned(), x["endpoint"].as_str()?.to_owned())))
            .collect();
        let ids: Vec<String> = match p["ids"].as_array() {
            Some(a) => a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect(),
            None => all.iter().map(|(i, _)| i.clone()).collect(),
        };
        for id in ids {
            let endpoint = all
                .iter()
                .find(|(i, _)| *i == id)
                .map(|(_, e)| e.clone())
                .ok_or_else(|| format!("providers.ids: {id} is not an enabled provider of beacon_api_module's chain {chain_id}"))?;
            let base = format!("beacon_api_module:{id}");
            let mut h = BeaconApiModuleHttp { chain_id, provider: id.clone(), base: base.clone(), timeout_ms: ms, answered: vec![] };
            let mut v = providers::beacon(&mut h, &id, &base, &expected);
            v["via"] = json!("beacon_api_module.raw_get");
            v["endpoint"] = json!(endpoint);
            v["host"] = json!(providers::host_of(&endpoint));
            if h.answered.iter().any(|a| *a != id) {
                v["verdict"] = json!("unavailable");
                v["unavailable"] = json!(["another provider answered for it"]);
            }
            provider_results.push(v);
        }
    }
    let min_providers = if req["min_providers"].is_null() {
        if provider_results.is_empty() { 0 } else { 3 }
    } else {
        uint(&req["min_providers"], "min_providers")?
    };
    let (providers_agreeing, any_disagrees) = providers::tally(&provider_results);
    if providers_agreeing < min_providers || any_disagrees {
        failed.push("providers");
    }

    // The checkpoint's execution block, as an execution endpoint holds it (`execution`: the design's
    // `verified_proxy_module`, whose answers its own light client checks, or any node): the bootstrap header's block.
    // `{"via": "eth_rpc_module", "chain_id": N}` asks Logos Core's `eth_rpc_module` (rpc.rs): with chain N in its
    // verified mode (`required`), `verified_proxy_module` answers every call; anything else is a URL read with curl.
    let execution = if req["execution"].is_null() {
        Value::Null
    } else {
        let target = match accepted.as_ref().map(|b| &b.header.execution) {
            Some(Execution::Payload { header, .. }) => Ok((header.block_hash, Some(header.block_number))),
            Some(Execution::BlockHash { block_hash, .. }) => Ok((*block_hash, None)),
            Some(Execution::None) => Err("the bootstrap's layout (Altair) carries no execution block"),
            None => Err("needs an accepted bootstrap: the execution block is its header's"),
        };
        let unavailable =
            |name: &str, url: &str, why: &str| json!({ "name": name, "url": url, "verdict": "unavailable", "unavailable": [why] });
        let e = &req["execution"];
        let v = if e["via"].as_str() == Some("eth_rpc_module") {
            let mut module = EthRpcModuleHttp(rpc::Rpc::from_request(&json!({ "eth": e }), timeout.saturating_mul(1000))?);
            let name = e["name"].as_str().unwrap_or("eth_rpc_module").to_owned();
            let url = format!("eth_rpc_module:{}", e["chain_id"]);
            let mut v = match target {
                Ok((hash, number)) => providers::execution(&mut module, &name, &url, &hash, number, config.chain_id),
                Err(why) => unavailable(&name, &url, why),
            };
            v["eth"] = module.0.report();
            v
        } else {
            let (name, url) = endpoint(e, "execution")?;
            match target {
                Ok((hash, number)) => providers::execution(http, &name, &url, &hash, number, config.chain_id),
                Err(why) => unavailable(&name, &url, why),
            }
        };
        if v["verdict"] != "agrees" {
            failed.push("execution");
        }
        v
    };

    Ok(json!({
        "verdict": if failed.is_empty() { "ok" } else { "failed" },
        "failed": failed,
        "instance_id": hex(&id),
        "instance_id_matches": expected_id,
        "checkpoint": hex(&checkpoint),
        "chain_pair": pair,
        "bootstrap": bootstrap,
        "committee": committee,
        "observed": observed,
        "observed_agreeing": agreeing,
        "required_agreeing": required,
        "providers": provider_results,
        "providers_agreeing": providers_agreeing,
        "min_providers": min_providers,
        "execution": execution,
    }))
}

/// `KeyValidate` on every key of a committee and the sum (`BlsBackend::check_committee`, blst).
fn committee_check(c: &SyncCommittee) -> Value {
    let keys = c.pubkeys.len();
    match Blst::new(Aggregation::Auto).check_committee(c) {
        Ok(()) => json!({ "verdict": "accepted", "code": 0, "keys": keys,
                          "check": "KeyValidate on every key (subgroup included); the keys sum to aggregate_pubkey" }),
        Err(e) => {
            let mut v = refused(e, "committee");
            v["keys"] = json!(keys);
            v
        }
    }
}

/// `verify_checkpoint`'s execution endpoint as Logos Core's `eth_rpc_module`: each JSON-RPC request the check makes
/// goes to `raw_rpc(chain_id, method, params)` (rpc.rs) and comes back as a node's answer; a refusal (for instance
/// `verified_proxy_module` unable to verify) as a JSON-RPC error, which the check counts as unavailable.
struct EthRpcModuleHttp(rpc::Rpc);

impl providers::Http for EthRpcModuleHttp {
    fn get(&mut self, url: &str) -> providers::Reply {
        Err(format!("{url}: eth_rpc_module takes JSON-RPC requests only"))
    }
    fn post(&mut self, _url: &str, body: &str) -> providers::Reply {
        let req: Value = serde_json::from_str(body).map_err(|e| format!("the request is not JSON: {e}"))?;
        let method = req["method"].as_str().ok_or("the request names no method")?;
        let answer = match self.0.call(method, req["params"].clone()) {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": req["id"], "result": result }),
            Err(e) => json!({ "jsonrpc": "2.0", "id": req["id"], "error": { "code": -32000, "message": e } }),
        };
        Ok((200, answer.to_string()))
    }
}

/// How the module crate reaches Logos Core's `beacon_api_module` (logos-evm-beacon-api-module): (method, the arguments
/// as a JSON array, a timeout in milliseconds) -> the method's reply as text.
pub type BeaconApiModuleCall = fn(&str, &str, u64) -> Result<String, String>;

static BEACON_API_MODULE: std::sync::OnceLock<BeaconApiModuleCall> = std::sync::OnceLock::new();

/// Installed once by the module crate (inside Logos Core).
pub fn set_beacon_api_module(f: BeaconApiModuleCall) {
    let _ = BEACON_API_MODULE.set(f);
}

fn beacon_api_call(method: &str, args: Value, timeout_ms: u64) -> Result<Value, String> {
    let f = BEACON_API_MODULE.get().ok_or("beacon_api_module is reachable only inside Logos Core (the module crate installs the call)")?;
    let text = f(method, &args.to_string(), timeout_ms)?;
    serde_json::from_str(&text).map_err(|e| format!("beacon_api_module.{method}: not JSON ({e}): {text}"))
}

/// One beacon provider of Logos Core's `beacon_api_module`, pinned (the module does not fail over for a pinned
/// provider): each GET of `verify_checkpoint`'s beacon checks goes to `raw_get(chain_id, path, provider)` and comes back
/// as that provider's answer. `answered` keeps which provider the module says answered each GET.
struct BeaconApiModuleHttp {
    chain_id: i64,
    provider: String,
    base: String,
    timeout_ms: u64,
    answered: Vec<String>,
}

impl providers::Http for BeaconApiModuleHttp {
    fn get(&mut self, url: &str) -> providers::Reply {
        let path = url.strip_prefix(&self.base).ok_or_else(|| format!("{url}: not a path of {}", self.base))?;
        let v = beacon_api_call("raw_get", json!([self.chain_id, path, self.provider]), self.timeout_ms)?;
        if v["ok"] == true {
            self.answered.push(v["provider"].as_str().unwrap_or_default().to_owned());
            let body = match &v["result"] {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            Ok((200, body))
        } else {
            let e = v["error"].as_str().map(str::to_owned).unwrap_or_else(|| v.to_string());
            if e.contains("404") {
                Ok((404, "{}".to_owned()))
            } else {
                Err(format!("beacon_api_module.raw_get: {e}"))
            }
        }
    }
    fn post(&mut self, url: &str, _body: &str) -> providers::Reply {
        Err(format!("{url}: beacon_api_module serves GETs only"))
    }
}

/// An endpoint of the request: a URL, or `{name, url}`.
fn endpoint(v: &Value, what: &str) -> Result<(String, String), String> {
    match v {
        Value::String(u) => Ok((providers::host_of(u), u.clone())),
        _ => {
            let url = text(&v["url"], &format!("{what}.url"))?.to_owned();
            let name = v["name"].as_str().map(str::to_owned).unwrap_or_else(|| providers::host_of(&url));
            Ok((name, url))
        }
    }
}

// ---------------------------------------------------------------------------------------------
// JSON and hex
// ---------------------------------------------------------------------------------------------

// ---------------------------------------------------------------------------------------------
// fetch_log_proof, build_attest_tx, read_attestation
// ---------------------------------------------------------------------------------------------

fn fetch_log(request: &str) -> Result<Value, String> {
    let req: Value = serde_json::from_str(request).map_err(|e| format!("request is not JSON: {e}"))?;
    let timeout = if req["timeout_ms"].is_null() { 20_000 } else { uint(&req["timeout_ms"], "timeout_ms")? };
    let block = uint(&req["block"], "block")?;
    // `rpc_url` (plain http://) or `eth` (Logos Core's eth_rpc_module): rpc.rs.
    let mut rpc = rpc::Rpc::from_request(&req, timeout)?;
    let tag = format!("0x{block:x}");
    let header = rpc.call("eth_getBlockByNumber", json!([tag, false]))?;
    if header.is_null() {
        return Err(format!("block {block}: the node has no such block"));
    }
    let receipts_root: Root = fixed(&header["receiptsRoot"], "block.receiptsRoot")?;
    let block_hash: Root = fixed(&header["hash"], "block.hash")?;
    let receipts = rpc.call("eth_getBlockReceipts", json!([tag]))?;
    let receipts = receipts.as_array().ok_or("eth_getBlockReceipts: not an array")?;
    let trie = trie::receipts_trie(receipts)?;
    let rebuilt = trie.root();
    if rebuilt != receipts_root {
        // The provider's receipts and its block disagree: nothing here would pass the program.
        return Ok(json!({
            "verdict": "inconsistent",
            "error": "the receipts do not rebuild the block's receiptsRoot",
            "receipts_root": hex(&receipts_root),
            "rebuilt_root": hex(&rebuilt),
            "rpc_calls": rpc.calls,
            "eth": rpc.report(),
        }));
    }
    // Which log: `tx_index` and `log_index` (within the receipt), or JSON-RPC's block-wide
    // `logIndex` as `block_log_index`.
    let (tx_index, log_index) = if !req["block_log_index"].is_null() {
        locate(receipts, uint(&req["block_log_index"], "block_log_index")?)?
    } else {
        (uint(&req["tx_index"], "tx_index")?, uint(&req["log_index"], "log_index")?)
    };
    let receipt = receipts
        .get(tx_index as usize)
        .ok_or_else(|| format!("tx_index {tx_index}: the block has {} receipts", receipts.len()))?;
    let logs = receipt["logs"].as_array().ok_or("receipt: no logs array")?;
    let log = logs
        .get(log_index as usize)
        .ok_or_else(|| format!("log_index {log_index}: the receipt has {} logs", logs.len()))?;
    let proof = trie.proof(&trie::receipt_key(tx_index));
    Ok(json!({
        "verdict": "fetched",
        "block": block,
        "block_hash": hex(&block_hash),
        "receipts_root": hex(&receipts_root),
        "receipts": receipts.len(),
        "receipt_status": receipt["status"],
        "log": {
            "tx_index": tx_index,
            "log_index": log_index,
            "receipt_proof": proof.iter().map(|n| hex(n)).collect::<Vec<_>>(),
        },
        "provider_log": {
            "address": log["address"],
            "topics": log["topics"],
            "data": log["data"],
            "logIndex": log["logIndex"],
            "transactionHash": receipt["transactionHash"],
        },
        "rpc_calls": rpc.calls,
        "eth": rpc.report(),
    }))
}

/// `(tx_index, log_index within the receipt)` of the log with block-wide index `want`.
fn locate(receipts: &[Value], want: u64) -> Result<(u64, u64), String> {
    for (i, r) in receipts.iter().enumerate() {
        for (j, l) in r["logs"].as_array().into_iter().flatten().enumerate() {
            if trie::quantity(&l["logIndex"], "logIndex")? == want {
                return Ok((i as u64, j as u64));
            }
        }
    }
    Err(format!("block_log_index {want}: no such log in the block"))
}

fn build_tx(request: &str) -> Result<Value, String> {
    let req: Value = serde_json::from_str(request).map_err(|e| format!("request is not JSON: {e}"))?;
    let program: Root = fixed(&req["program_id"], "program_id")?;
    let instance_id: Root = fixed(&req["instance_id"], "instance_id")?;
    let chain_id = uint(&req["chain_id"], "chain_id")?;
    let slot = uint(&req["slot"], "slot")?;
    // `log` (one, as fetch_log_proof returns it) or `logs` (several); `storage` as dry_run_attest
    // takes it.
    let items: Vec<&Value> = match (&req["logs"], &req["log"]) {
        (Value::Array(a), _) => a.iter().collect(),
        (_, l) if !l.is_null() => vec![l],
        _ => vec![],
    };
    let mut logs = Vec::with_capacity(items.len());
    for (i, l) in items.iter().enumerate() {
        logs.push(tx::LogQuery {
            tx_index: uint(&l["tx_index"], &format!("logs[{i}].tx_index"))?,
            log_index: uint(&l["log_index"], &format!("logs[{i}].log_index"))?,
            proof: nodes(&l["receipt_proof"], &format!("logs[{i}].receipt_proof"))?,
        });
    }
    let mut accounts = Vec::new();
    let mut read = None;
    if !req["storage"].is_null() {
        // The proof given, or read at `storage.block` through the path the storage names.
        let (s, r) = storage_with_proof(&req["storage"], None)?;
        accounts.push(account_query(&s)?);
        read = r;
    }
    if logs.is_empty() && accounts.is_empty() {
        return Err("the request needs `log`, `logs` or `storage`".into());
    }
    // The program attests each account as a fact, then each of its slots, then each log.
    let facts = logs.len() + accounts.iter().map(|a| 1 + a.slots.len()).sum::<usize>();
    let message = tx::attest_message(&tx::Request { instance_id, chain_id, accounts, logs });
    let account = tx::header_account(&program, &instance_id, slot);
    let mut out = json!({
        "program_id": hex(&program),
        "instance_id": hex(&instance_id),
        "chain_id": chain_id,
        "slot": slot,
        "account": hex(&account),
        "accounts": [hex(&account)],
        "facts": facts,
        "message": hex(&message),
        "message_bytes": message.len(),
        "message_sha256": hex(&Sha256::digest(&message)),
    });
    if let Some(r) = read {
        out["eth"] = r;
    }
    Ok(out)
}

/// `storage` with its `eth_getProof` result: as given (`get_proof`, or its fields directly), or read
/// for `address` and `slot` at `block` (the record's for a dry run, else `storage.block`) through the
/// path the storage names (`eth` or `rpc_url`, rpc.rs). The storage with `get_proof` filled in, and
/// what answered when it was read.
fn storage_with_proof(s: &Value, block: Option<u64>) -> Result<(Value, Option<Value>), String> {
    let given = !s["get_proof"].is_null() || !s["accountProof"].is_null();
    if given || (s["eth"].is_null() && s["rpc_url"].is_null()) {
        return Ok((s.clone(), None));
    }
    let block = match block {
        Some(b) => b,
        None => uint(&s["block"], "storage.block (the block the proof is read at)")?,
    };
    let timeout = if s["timeout_ms"].is_null() { 20_000 } else { uint(&s["timeout_ms"], "storage.timeout_ms")? };
    let address: Address = fixed(&s["address"], "storage.address")?;
    let slot = word(&s["slot"], "storage.slot")?;
    let mut rpc = rpc::Rpc::from_request(s, timeout)?;
    let gp = rpc.call("eth_getProof", json!([hex(&address), [hex(&slot)], format!("0x{block:x}")]))?;
    let mut filled = s.clone();
    filled["get_proof"] = gp;
    let mut report = rpc.report();
    report["block"] = json!(block);
    Ok((filled, Some(report)))
}

/// One account and one slot from an `eth_getProof` result (`get_proof`, or its fields) and `slot`.
fn account_query(s: &Value) -> Result<tx::AccountQuery, String> {
    let p = if s["get_proof"].is_null() { s } else { &s["get_proof"] };
    let address: Address = fixed(&p["address"], "storage.address")?;
    let proof = nodes(&p["accountProof"], "storage.accountProof")?;
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
    let slot_proof = nodes(&entry["proof"], "storage.storageProof[].proof")?;
    Ok(tx::AccountQuery { address, proof, slots: vec![tx::SlotQuery { slot, proof: slot_proof }] })
}

fn read_event(request: &str) -> Result<Value, String> {
    let req: Value = serde_json::from_str(request).map_err(|e| format!("request is not JSON: {e}"))?;
    if !req["selector"].is_null() {
        let selector = unhex(text(&req["selector"], "selector")?).map_err(|e| format!("selector: {e}"))?;
        if selector != tx::ATTESTED_SELECTOR {
            return Ok(json!({ "verdict": "not_an_attestation", "selector": hex(&selector) }));
        }
    }
    let data = unhex(text(&req["event_data"], "event_data")?).map_err(|e| format!("event_data: {e}"))?;
    let a = tx::decode_attested(&data)?;
    let statements: Vec<String> = a.statements.iter().map(|s| hex(s)).collect();
    let mut out = json!({
        "verdict": "attested",
        "instance_id": hex(&a.instance_id),
        "chain_id": a.chain_id,
        "block_number": a.block_number,
        "statements": statements,
    });
    // `expected_ids`: what dry_run_attest returned as `id`, in the request's fact order.
    if let Some(expected) = req["expected_ids"].as_array() {
        let want: Vec<String> = expected.iter().map(|e| e.as_str().unwrap_or("").to_ascii_lowercase()).collect();
        out["matches_expected"] = json!(want == statements);
    }
    if !req["expected_instance_id"].is_null() {
        let want: Root = fixed(&req["expected_instance_id"], "expected_instance_id")?;
        out["instance_matches"] = json!(want == a.instance_id);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// read_instance
// ---------------------------------------------------------------------------------------------

/// A Borsh reader over a byte string.
struct Bytes<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Bytes<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).filter(|e| *e <= self.b.len()).ok_or("state: too short")?;
        let out = &self.b[self.at..end];
        self.at = end;
        Ok(out)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N], String> {
        Ok(self.take(N)?.try_into().unwrap())
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.array::<4>()?))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.array::<8>()?))
    }
}

fn instance(request: &str) -> Result<Value, String> {
    let req: Value = serde_json::from_str(request).map_err(|e| format!("request is not JSON: {e}"))?;
    let bytes = unhex(text(&req["state"], "state")?).map_err(|e| format!("state: {e}"))?;
    // `InstanceState { id, config, active, store: Vec<u8>, applied, root_id, parent_id }`.
    let mut r = Bytes { b: &bytes, at: 0 };
    let id: Root = r.array::<32>()?;
    let config = Config::read(&mut r)?;
    let active = match r.u8()? {
        0 => false,
        1 => true,
        x => return Err(format!("state: `active` is not a bool ({x})")),
    };
    let store_len = r.u32()? as usize;
    let store_bytes = r.take(store_len)?;
    let applied = r.u64()?;
    let root_id: Root = r.array::<32>()?;
    let parent_id: Root = r.array::<32>()?;
    if r.at != bytes.len() {
        return Err(format!("state: {} bytes after InstanceState", bytes.len() - r.at));
    }
    let mut out = json!({
        "verdict": "ok",
        "id": hex(&id),
        "config": config.to_json(),
        "active": active,
        "applied": applied,
        "root_id": hex(&root_id),
        "parent_id": hex(&parent_id),
    });
    out["store"] = match witness::CompactStore::decode(store_bytes) {
        Ok(st) => json!({
            "fork": format!("{:?}", st.fork),
            "finalized_slot": st.finalized.slot,
            "finalized_header": {
                "slot": st.finalized.slot,
                "proposer_index": st.finalized.proposer_index,
                "parent_root": hex(&st.finalized.parent_root),
                "state_root": hex(&st.finalized.state_root),
                "body_root": hex(&st.finalized.body_root),
            },
            "next_committee_known": st.next.is_some(),
            "digest": hex(&st.digest()),
        }),
        Err(e) => json!({ "error": format!("{e:?}"), "code": e.code() }),
    };
    // With the checkpoint the instance was created from, the id as `Init` derives it.
    if !req["checkpoint"].is_null() {
        let checkpoint: Root = fixed(&req["checkpoint"], "checkpoint")?;
        let recomputed = instance_id(&config, &checkpoint);
        out["instance_id_recomputed"] = json!(hex(&recomputed));
        out["id_matches_config"] = json!(recomputed == id);
    }
    if !req["expected_instance_id"].is_null() {
        let want: Root = fixed(&req["expected_instance_id"], "expected_instance_id")?;
        out["instance_matches"] = json!(want == id);
    }
    Ok(out)
}

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

pub(crate) fn text<'a>(v: &'a Value, what: &str) -> Result<&'a str, String> {
    match v {
        Value::String(s) => Ok(s),
        Value::Null => Err(format!("{what}: missing")),
        _ => Err(format!("{what}: expected a string")),
    }
}

/// A byte string of exactly `N` bytes, in hex.
pub(crate) fn fixed<const N: usize>(v: &Value, what: &str) -> Result<[u8; N], String> {
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
    /// The harness's weak-subjectivity period (`lez/harness` `test_ws_period_epochs()`).
    const WS_PERIOD_EPOCHS: u64 = 3_532;

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
            ws_period_epochs: WS_PERIOD_EPOCHS,
        }
    }

    #[test]
    fn config_borsh_is_202_bytes() {
        assert_eq!(mainnet(2).borsh().len(), 8 + 4 + 8 + 8 + 8 + 36 + 72 + 32 + 8 + 8 + 1 + 1 + 8);
    }

    /// The instance of `lez/harness` `World::new(2)` (mainnet periods 1899–1903), whose id the
    /// program wrote into the transactions of `lez/results/stage3-*-tx.hex` (prefix in
    /// `lez/results/prove-private-dry-run.log`).
    #[test]
    fn instance_id_equals_the_programs() {
        let checkpoint = fixed(&json!("0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac"), "c").unwrap();
        assert_eq!(
            hex(&instance_id(&mainnet(2), &checkpoint)),
            // With lc-a1's ws_period_epochs (3,532, the harness's) in the configuration: this encoding gives the merged
            // program's own id for the Stage 4 instance (module_e2e info, read_instance id_matches_config at integration7);
            // before it, at integration6, the id was 0xce624027…afdfb.
            "0x39860f39c97574b5eed493bdfce3429de1b8ff1b1f2a2452e72292460953b79c"
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

    // ---- verify_checkpoint's provider checks, on the responses recorded for the crown run's Sepolia checkpoint ----

    /// The crown run's Sepolia bootstrap (period 1411, slot 11,562,112).
    const SEPOLIA_BOOTSTRAP: &[u8] =
        include_bytes!("../../../../../crates/ferrata-spectests/fixtures/sepolia-2026-11-12/bootstrap.ssz");
    /// The request (without its bootstrap) and every provider response of its live run (`examples/call.rs` with
    /// `FERRATA_HTTP_RECORD`; `modules/results/verify-checkpoint-2026-11-13/`).
    const SEPOLIA_REQUEST: &str = include_str!("../tests/fixtures/verify-checkpoint-sepolia.json");
    const SEPOLIA_RECORD: &str = include_str!("../tests/fixtures/verify-checkpoint-sepolia.record.json");

    fn sepolia_request() -> Value {
        let mut r: Value = serde_json::from_str(SEPOLIA_REQUEST).unwrap();
        r["bootstrap"] = json!(hex(SEPOLIA_BOOTSTRAP));
        r
    }

    fn replay() -> providers::Replay {
        providers::Replay { log: serde_json::from_str(SEPOLIA_RECORD).unwrap() }
    }

    fn check(request: &Value, http: &mut providers::Replay) -> Value {
        serde_json::from_str(&verify_checkpoint_with(&request.to_string(), http)).unwrap()
    }

    /// Edit the recorded JSON response of the request whose URL starts with `base` and ends with `path` (and, for a
    /// JSON-RPC call, names `method`).
    fn edit(http: &mut providers::Replay, base: &str, path: &str, method: Option<&str>, f: impl Fn(&mut Value)) {
        let e = http
            .log
            .iter_mut()
            .find(|e| {
                let url = e["url"].as_str().unwrap_or_default();
                url.starts_with(base)
                    && url.ends_with(path)
                    && method.is_none_or(|m| e["body"].as_str().is_some_and(|b| b.contains(&format!("\"{m}\""))))
            })
            .unwrap_or_else(|| panic!("no recorded request {base}…{path} {method:?}"));
        let mut v: Value = serde_json::from_str(e["response"].as_str().unwrap()).unwrap();
        f(&mut v);
        e["response"] = json!(v.to_string());
    }

    fn provider<'a>(v: &'a Value, name: &str) -> &'a Value {
        v["providers"].as_array().unwrap().iter().find(|p| p["name"] == name).unwrap()
    }

    const LODESTAR: &str = "https://lodestar-sepolia.chainsafe.io";
    const PUBLICNODE: &str = "https://ethereum-sepolia-beacon-api.publicnode.com";
    const NIMBUS: &str = "http://unstable.sepolia.beacon-api.nimbus.team";
    const EXECUTION: &str = "https://ethereum-sepolia-rpc.publicnode.com";

    #[test]
    fn sepolia_checkpoint_agrees_with_three_providers() {
        let v = check(&sepolia_request(), &mut replay());
        assert_eq!(v["verdict"], "ok", "{v:#}");
        assert_eq!(v["instance_id_matches"], true);
        assert_eq!(v["bootstrap"]["code"], 0);
        assert_eq!(v["bootstrap"]["slot"], 11_562_112);
        assert_eq!(v["committee"]["verdict"], "accepted");
        assert_eq!(v["committee"]["keys"], 512);
        assert_eq!(v["providers_agreeing"], 3);
        for p in ["lodestar", "publicnode", "nimbus"] {
            assert_eq!(provider(&v, p)["verdict"], "agrees", "{p}: {:#}", provider(&v, p));
            assert_eq!(provider(&v, p)["slot"], 11_562_112);
        }
        // A checkpointz server serves the chain's genesis and spec, not the headers API: not counted.
        assert_eq!(provider(&v, "checkpointz-ethpandaops")["verdict"], "unavailable");
        assert_eq!(v["execution"]["verdict"], "agrees", "{:#}", v["execution"]);
    }

    /// Logos Core's `beacon_api_module` as a test double: its three providers for Sepolia, each `raw_get` answered
    /// from the live run's record of that provider (what the module would fetch from it).
    fn recorded_beacon_api_module(method: &str, args: &str, _t: u64) -> Result<String, String> {
        let a: Value = serde_json::from_str(args).unwrap();
        let bases = [("lodestar", LODESTAR), ("publicnode", PUBLICNODE), ("nimbus", NIMBUS)];
        match method {
            "list_providers" => Ok(json!({ "ok": true, "chainId": a[0],
                "providers": bases.iter().map(|(i, e)| json!({ "id": i, "endpoint": e, "enabled": true })).collect::<Vec<_>>() })
            .to_string()),
            "raw_get" => {
                let (path, id) = (a[1].as_str().unwrap(), a[2].as_str().unwrap());
                let base = bases.iter().find(|(i, _)| *i == id).ok_or("unknown provider")?.1;
                let log: Vec<Value> = serde_json::from_str(SEPOLIA_RECORD).unwrap();
                let url = format!("{base}{path}");
                Ok(match log.iter().find(|e| e["method"] == "GET" && e["url"] == url) {
                    Some(e) if e["status"] == 200 => json!({ "ok": true, "provider": id,
                        "result": serde_json::from_str::<Value>(e["response"].as_str().unwrap()).unwrap() }),
                    Some(e) => json!({ "ok": false, "error": format!("{id}: HTTP {}", e["status"]) }),
                    None => json!({ "ok": false, "error": format!("not recorded: {url}") }),
                }
                .to_string())
            }
            other => Err(format!("{other}: not in the double")),
        }
    }

    #[test]
    fn the_beacon_checks_through_beacon_api_module_give_the_plain_paths_verdicts() {
        set_beacon_api_module(recorded_beacon_api_module);
        let mut req = sepolia_request();
        req["providers"] = json!({ "via": "beacon_api_module", "chain_id": 11_155_111 });
        let v = check(&req, &mut replay());
        assert_eq!(v["verdict"], "ok", "{v:#}");
        assert_eq!(v["providers_agreeing"], 3);
        let plain = check(&sepolia_request(), &mut replay());
        for p in ["lodestar", "publicnode", "nimbus"] {
            assert_eq!(provider(&v, p)["verdict"], "agrees", "{p}: {:#}", provider(&v, p));
            assert_eq!(provider(&v, p)["via"], "beacon_api_module.raw_get");
            for k in ["slot", "verdict", "finalized_slot"] {
                assert_eq!(provider(&v, p)[k], provider(&plain, p)[k], "{p}: {k}");
            }
        }
        // Pinned to two of them: two agree, three are needed.
        req["providers"]["ids"] = json!(["lodestar", "nimbus"]);
        let v = check(&req, &mut replay());
        assert_eq!(v["providers_agreeing"], 2);
        assert!(v["failed"].as_array().unwrap().contains(&json!("providers")));
        // A provider beacon_api_module does not have is a request error.
        req["providers"]["ids"] = json!(["lodestar", "nope"]);
        let v = check(&req, &mut replay());
        assert_eq!(v["verdict"], "invalid_request", "{v:#}");
    }

    #[test]
    fn a_provider_with_another_block_at_the_slot_fails_the_check() {
        let mut http = replay();
        edit(&mut http, NIMBUS, "/eth/v1/beacon/headers/11562112", None, |r| {
            r["data"]["root"] = json!(format!("0x{}", "ab".repeat(32)));
        });
        let v = check(&sepolia_request(), &mut http);
        assert_eq!(v["verdict"], "failed");
        assert!(v["failed"].as_array().unwrap().contains(&json!("providers")));
        assert_eq!(provider(&v, "nimbus")["verdict"], "disagrees");
        assert_eq!(v["providers_agreeing"], 2);
    }

    #[test]
    fn a_provider_that_has_not_finalized_the_slot_disagrees() {
        let mut http = replay();
        edit(&mut http, LODESTAR, "/eth/v1/beacon/headers/finalized", None, |r| {
            r["data"]["header"]["message"]["slot"] = json!("11562080");
        });
        let v = check(&sepolia_request(), &mut http);
        assert_eq!(v["verdict"], "failed");
        let p = provider(&v, "lodestar");
        assert_eq!(p["verdict"], "disagrees");
        assert!(p["disagreements"][0].as_str().unwrap().starts_with("finalized slot"), "{p:#}");
    }

    #[test]
    fn a_provider_on_another_chain_disagrees() {
        // The chain id the light client cannot check, tied to genesis_validators_root by the provider's own spec.
        let mut http = replay();
        edit(&mut http, PUBLICNODE, "/eth/v1/config/spec", None, |r| r["data"]["DEPOSIT_CHAIN_ID"] = json!("1"));
        let v = check(&sepolia_request(), &mut http);
        assert_eq!(v["verdict"], "failed");
        assert!(provider(&v, "publicnode")["disagreements"][0].as_str().unwrap().starts_with("DEPOSIT_CHAIN_ID"));
        let mut http = replay();
        edit(&mut http, PUBLICNODE, "/eth/v1/beacon/genesis", None, |r| {
            r["data"]["genesis_validators_root"] = json!(format!("0x{}", KNOWN_CHAINS[0].2));
        });
        let v = check(&sepolia_request(), &mut http);
        assert_eq!(provider(&v, "publicnode")["verdict"], "disagrees");
    }

    #[test]
    fn a_non_canonical_block_disagrees() {
        let mut http = replay();
        let root = "/eth/v1/beacon/headers/0x3b901f5e6853b5cf5eb04d7ccfe1272e66c38291cb3a11dd159cacd1117ccf7c";
        edit(&mut http, LODESTAR, root, None, |r| r["data"]["canonical"] = json!(false));
        let v = check(&sepolia_request(), &mut http);
        assert_eq!(provider(&v, "lodestar")["verdict"], "disagrees");
        assert_eq!(v["verdict"], "failed");
    }

    #[test]
    fn an_unreachable_provider_is_not_counted_and_three_are_needed() {
        let mut http = replay();
        for e in http.log.iter_mut().filter(|e| e["url"].as_str().unwrap().starts_with(NIMBUS)) {
            *e = json!({ "method": e["method"], "url": e["url"], "body": e["body"], "error": "Connection refused" });
        }
        let v = check(&sepolia_request(), &mut http);
        assert_eq!(provider(&v, "nimbus")["verdict"], "unavailable");
        assert_eq!(v["providers_agreeing"], 2);
        assert_eq!(v["verdict"], "failed", "two of the three providers needed");
        let mut r = sepolia_request();
        r["min_providers"] = json!(2);
        assert_eq!(check(&r, &mut http)["verdict"], "ok");
    }

    #[test]
    fn providers_are_counted_by_host() {
        let mut r = sepolia_request();
        r["providers"] = json!([{"name": "a", "url": LODESTAR}, {"name": "b", "url": format!("{LODESTAR}/")}, {"name": "c", "url": PUBLICNODE}]);
        let v = check(&r, &mut replay());
        assert_eq!(v["providers_agreeing"], 2, "{v:#}");
        assert_eq!(v["verdict"], "failed");
    }

    #[test]
    fn the_execution_check_goes_through_eth_rpc_module_when_asked() {
        // Natively no eth_rpc_module is installed: the check is unavailable (and so failed), the beacon checks unchanged,
        // and the reply says which path was asked. Through Logos Core the same request reads verified_proxy_module's
        // answers when eth_rpc_module holds the chain in `required` mode (modules/scripts/verified-proxy.sh).
        let mut req = sepolia_request();
        req["execution"] = json!({ "via": "eth_rpc_module", "chain_id": 11_155_111 });
        let v = check(&req, &mut replay());
        assert_eq!(v["execution"]["verdict"], "unavailable", "{:#}", v["execution"]);
        assert_eq!(v["execution"]["url"], "eth_rpc_module:11155111");
        assert_eq!(v["execution"]["eth"]["via"], "eth_rpc_module");
        assert_eq!(v["execution"]["eth"]["chain_id"], 11_155_111);
        assert!(v["execution"]["unavailable"][0].as_str().unwrap().contains("only from inside Logos Core"), "{v:#}");
        assert_eq!(v["failed"], json!(["execution"]));
        assert_eq!(v["providers_agreeing"], 3);
        req["execution"] = json!({ "via": "eth_rpc_module" });
        assert_eq!(check(&req, &mut replay())["verdict"], "invalid_request");
    }

    #[test]
    fn the_execution_endpoint_must_hold_the_block_on_the_same_chain() {
        let mut http = replay();
        edit(&mut http, EXECUTION, "", Some("eth_chainId"), |r| r["result"] = json!("0x1"));
        let v = check(&sepolia_request(), &mut http);
        assert_eq!(v["execution"]["verdict"], "disagrees", "{:#}", v["execution"]);
        assert!(v["failed"].as_array().unwrap().contains(&json!("execution")));
        let mut http = replay();
        edit(&mut http, EXECUTION, "", Some("eth_getBlockByHash"), |r| r["result"] = Value::Null);
        assert_eq!(check(&sepolia_request(), &mut http)["execution"]["verdict"], "disagrees");
    }

    #[test]
    fn no_providers_no_fetching() {
        // The checks of row 59 alone: nothing fetched, nothing required of providers.
        let mut r = sepolia_request();
        for k in ["providers", "min_providers", "execution"] {
            r.as_object_mut().unwrap().remove(k);
        }
        let mut empty = providers::Replay { log: Vec::new() };
        let v = check(&r, &mut empty);
        assert_eq!(v["verdict"], "ok", "{v:#}");
        assert_eq!(v["min_providers"], 0);
        assert_eq!(v["providers"], json!([]));
    }

    #[test]
    fn key_validate_refuses_a_key_outside_the_subgroup() {
        let b = LightClientBootstrap::decode(SEPOLIA_BOOTSTRAP, LcFork::Gloas, 512).unwrap();
        assert_eq!(committee_check(&b.current_sync_committee)["code"], 0);
        // (0, 2), a point of order three on the curve (crates/ferrata-spectests/tests/adversarial.rs).
        let mut c = b.current_sync_committee.clone();
        c.pubkeys[0] = [0u8; 48];
        c.pubkeys[0][0] = 0x80;
        let v = committee_check(&c);
        assert_eq!(v["verdict"], "refused", "{v}");
        // ferrata-core's committee check reports a key that fails KeyValidate as the committee's refusal.
        assert_eq!(v["code"], Error::CommitteeInvalid.code(), "{v}");
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
