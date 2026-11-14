//! Light-client data in the beacon API's JSON form (`{"version": …, "data": …}`, what a beacon node answers with
//! `Accept: application/json`) turned into the SSZ the updater reads (what the same node answers with
//! `Accept: application/octet-stream`, without the `updates` stream's length and fork-digest prefixes).
//!
//! Logos Core's `beacon_api_module` answers `lc_request` with the provider's JSON body, verbatim
//! (`{ok, provider, body, attempts}`), so a source that reads through it needs this (`source::BeaconApiModule`).
//! Every layout from Altair to Gloas (`ferrata_core::fork::LcFork`; Bellatrix is Altair's, Fulu Electra's), named by
//! `version`. The fields are taken by name and the lengths checked against the layout; nothing is trusted: the
//! updater decodes the bytes and runs the program's own checks on them, as for any source. The tests compare the
//! output with the bytes providers served as SSZ for the same requests.

use anyhow::{anyhow, bail, ensure, Context as _, Result};
use ferrata_core::{
    fork::{Fork, LcFork},
    types::{BeaconBlockHeader, BlsPubkey, Execution, ExecutionPayloadHeader, LightClientHeader, SyncAggregate},
    witness::{encode_light_client_header, encode_sync_aggregate},
    Root,
};
use serde_json::Value;

/// `GET …/light_client/bootstrap/{root}` as JSON → `LightClientBootstrap` SSZ.
pub fn bootstrap(v: &Value) -> Result<Vec<u8>> {
    let (fork, d) = versioned(v)?;
    let header = header(&d["header"], fork).context("bootstrap.header")?;
    let committee = committee(&d["current_sync_committee"]).context("bootstrap.current_sync_committee")?;
    let branch = branch(&d["current_sync_committee_branch"], fork.sync_committee_branch_len())
        .context("bootstrap.current_sync_committee_branch")?;
    Ok(container(&[(header_variable(fork), header), (false, committee), (false, branch)]))
}

/// One `LightClientUpdate` (`{"version", "data"}`) → SSZ.
pub fn update(v: &Value) -> Result<Vec<u8>> {
    let (fork, d) = versioned(v)?;
    let attested = header(&d["attested_header"], fork).context("update.attested_header")?;
    let next = committee(&d["next_sync_committee"]).context("update.next_sync_committee")?;
    let next_branch = branch(&d["next_sync_committee_branch"], fork.sync_committee_branch_len())
        .context("update.next_sync_committee_branch")?;
    let finalized = header(&d["finalized_header"], fork).context("update.finalized_header")?;
    let finality_branch =
        branch(&d["finality_branch"], fork.finality_branch_len()).context("update.finality_branch")?;
    let n = next.len() / 48 - 1;
    let aggregate = sync_aggregate(&d["sync_aggregate"], n).context("update.sync_aggregate")?;
    let slot = uint(&d["signature_slot"]).context("update.signature_slot")?;
    let var = header_variable(fork);
    Ok(container(&[
        (var, attested),
        (false, next),
        (false, next_branch),
        (var, finalized),
        (false, finality_branch),
        (false, aggregate),
        (false, slot.to_le_bytes().to_vec()),
    ]))
}

/// `GET …/light_client/updates?start_period&count` as JSON (an array of `{"version", "data"}`) → one SSZ update each.
pub fn updates(v: &Value) -> Result<Vec<Vec<u8>>> {
    let list = v.as_array().ok_or_else(|| anyhow!("updates: not a JSON array"))?;
    list.iter().enumerate().map(|(i, u)| update(u).with_context(|| format!("updates[{i}]"))).collect()
}

/// `GET …/light_client/finality_update` as JSON → `LightClientFinalityUpdate` SSZ.
pub fn finality_update(v: &Value) -> Result<Vec<u8>> {
    let (fork, d) = versioned(v)?;
    let attested = header(&d["attested_header"], fork).context("finality_update.attested_header")?;
    let finalized = header(&d["finalized_header"], fork).context("finality_update.finalized_header")?;
    let finality_branch =
        branch(&d["finality_branch"], fork.finality_branch_len()).context("finality_update.finality_branch")?;
    let bits = d["sync_aggregate"]["sync_committee_bits"].as_str().unwrap_or("");
    let n = 4 * bits.trim_start_matches("0x").len();
    let aggregate = sync_aggregate(&d["sync_aggregate"], n).context("finality_update.sync_aggregate")?;
    let slot = uint(&d["signature_slot"]).context("finality_update.signature_slot")?;
    let var = header_variable(fork);
    Ok(container(&[
        (var, attested),
        (var, finalized),
        (false, finality_branch),
        (false, aggregate),
        (false, slot.to_le_bytes().to_vec()),
    ]))
}

/// The layout `version` names, and `data`.
fn versioned(v: &Value) -> Result<(LcFork, &Value)> {
    let name = v["version"].as_str().ok_or_else(|| anyhow!("no \"version\": not a versioned light-client object"))?;
    let fork = Fork::from_name(name)
        .and_then(Fork::lc_fork)
        .ok_or_else(|| anyhow!("version {name:?}: no light-client layout for it"))?;
    ensure!(v["data"].is_object(), "no \"data\" object");
    Ok((fork, &v["data"]))
}

/// Capella to Electra headers hold a variable-size execution payload header: the container keeps an offset for them.
fn header_variable(fork: LcFork) -> bool {
    matches!(fork, LcFork::Capella | LcFork::Deneb | LcFork::Electra)
}

/// An SSZ container: fixed parts in place, variable ones as 4-byte offsets with their bytes after the fixed part.
fn container(parts: &[(bool, Vec<u8>)]) -> Vec<u8> {
    let fixed: usize = parts.iter().map(|(var, b)| if *var { 4 } else { b.len() }).sum();
    let (mut out, mut tail) = (Vec::new(), Vec::new());
    for (var, b) in parts {
        if *var {
            out.extend_from_slice(&u32::try_from(fixed + tail.len()).expect("small").to_le_bytes());
            tail.extend_from_slice(b);
        } else {
            out.extend_from_slice(b);
        }
    }
    out.extend_from_slice(&tail);
    out
}

fn header(h: &Value, fork: LcFork) -> Result<Vec<u8>> {
    ensure!(h.is_object(), "missing");
    let b = &h["beacon"];
    let beacon = BeaconBlockHeader {
        slot: uint(&b["slot"]).context("beacon.slot")?,
        proposer_index: uint(&b["proposer_index"]).context("beacon.proposer_index")?,
        parent_root: bytes32(&b["parent_root"]).context("beacon.parent_root")?,
        state_root: bytes32(&b["state_root"]).context("beacon.state_root")?,
        body_root: bytes32(&b["body_root"]).context("beacon.body_root")?,
    };
    let execution = match fork {
        LcFork::Altair => Execution::None,
        LcFork::Capella | LcFork::Deneb | LcFork::Electra => Execution::Payload {
            header: payload_header(&h["execution"], fork >= LcFork::Deneb).context("execution")?,
            branch: roots(&h["execution_branch"], fork.execution_branch_len()).context("execution_branch")?,
        },
        LcFork::Gloas => Execution::BlockHash {
            block_hash: bytes32(&h["execution_block_hash"]).context("execution_block_hash")?,
            branch: roots(&h["execution_branch"], fork.execution_branch_len()).context("execution_branch")?,
        },
    };
    Ok(encode_light_client_header(&LightClientHeader { beacon, execution }, fork))
}

fn payload_header(e: &Value, deneb: bool) -> Result<ExecutionPayloadHeader> {
    ensure!(e.is_object(), "missing");
    let extra_data = bytes(&e["extra_data"]).context("extra_data")?;
    ensure!(extra_data.len() <= ferrata_core::types::MAX_EXTRA_DATA_BYTES, "extra_data: {} bytes", extra_data.len());
    Ok(ExecutionPayloadHeader {
        parent_hash: bytes32(&e["parent_hash"]).context("parent_hash")?,
        fee_recipient: fixed(&e["fee_recipient"]).context("fee_recipient")?,
        state_root: bytes32(&e["state_root"]).context("state_root")?,
        receipts_root: bytes32(&e["receipts_root"]).context("receipts_root")?,
        logs_bloom: fixed(&e["logs_bloom"]).context("logs_bloom")?,
        prev_randao: bytes32(&e["prev_randao"]).context("prev_randao")?,
        block_number: uint(&e["block_number"]).context("block_number")?,
        gas_limit: uint(&e["gas_limit"]).context("gas_limit")?,
        gas_used: uint(&e["gas_used"]).context("gas_used")?,
        timestamp: uint(&e["timestamp"]).context("timestamp")?,
        extra_data,
        base_fee_per_gas: uint256_le(&e["base_fee_per_gas"]).context("base_fee_per_gas")?,
        block_hash: bytes32(&e["block_hash"]).context("block_hash")?,
        transactions_root: bytes32(&e["transactions_root"]).context("transactions_root")?,
        withdrawals_root: bytes32(&e["withdrawals_root"]).context("withdrawals_root")?,
        blob_gas_used: if deneb { uint(&e["blob_gas_used"]).context("blob_gas_used")? } else { 0 },
        excess_blob_gas: if deneb { uint(&e["excess_blob_gas"]).context("excess_blob_gas")? } else { 0 },
    })
}

/// `SyncCommittee` SSZ: the public keys, then the aggregate key.
fn committee(c: &Value) -> Result<Vec<u8>> {
    let keys = c["pubkeys"].as_array().ok_or_else(|| anyhow!("pubkeys: not an array"))?;
    ensure!(!keys.is_empty(), "pubkeys: empty");
    let mut out = Vec::with_capacity(48 * (keys.len() + 1));
    for (i, k) in keys.iter().enumerate() {
        out.extend_from_slice(&fixed::<48>(k).with_context(|| format!("pubkeys[{i}]"))?);
    }
    let aggregate: BlsPubkey = fixed(&c["aggregate_pubkey"]).context("aggregate_pubkey")?;
    out.extend_from_slice(&aggregate);
    Ok(out)
}

/// `SyncAggregate` SSZ for a committee of `n`.
fn sync_aggregate(a: &Value, n: usize) -> Result<Vec<u8>> {
    let raw = bytes(&a["sync_committee_bits"]).context("sync_committee_bits")?;
    ensure!(n > 0 && raw.len() == n.div_ceil(8), "sync_committee_bits: {} bytes for a committee of {n}", raw.len());
    let bits = (0..n).map(|i| (raw[i / 8] >> (i % 8)) & 1 == 1).collect();
    let signature = fixed(&a["sync_committee_signature"]).context("sync_committee_signature")?;
    Ok(encode_sync_aggregate(&SyncAggregate { bits, signature }))
}

fn branch(v: &Value, len: usize) -> Result<Vec<u8>> {
    Ok(roots(v, len)?.concat())
}

fn roots(v: &Value, len: usize) -> Result<Vec<Root>> {
    let list = v.as_array().ok_or_else(|| anyhow!("not an array"))?;
    ensure!(list.len() == len, "{} roots, the layout has {len}", list.len());
    list.iter().map(bytes32).collect()
}

/// A decimal string (the beacon API's form of every integer), or a JSON number.
fn uint(v: &Value) -> Result<u64> {
    match v {
        Value::String(s) => s.parse().map_err(|_| anyhow!("{s:?}: not a decimal integer")),
        Value::Number(n) => n.as_u64().ok_or_else(|| anyhow!("{n}: not a non-negative integer")),
        _ => bail!("missing"),
    }
}

/// A decimal `uint256` (`base_fee_per_gas`) as 32 little-endian bytes.
fn uint256_le(v: &Value) -> Result<[u8; 32]> {
    let s = v.as_str().ok_or_else(|| anyhow!("missing"))?;
    ensure!(!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()), "{s:?}: not a decimal integer");
    let mut out = [0u8; 32];
    for d in s.bytes() {
        let mut carry = u32::from(d - b'0');
        for byte in out.iter_mut() {
            let x = u32::from(*byte) * 10 + carry;
            *byte = (x & 0xff) as u8;
            carry = x >> 8;
        }
        ensure!(carry == 0, "{s}: more than 256 bits");
    }
    Ok(out)
}

fn bytes(v: &Value) -> Result<Vec<u8>> {
    let s = v.as_str().ok_or_else(|| anyhow!("missing"))?;
    let h = s.strip_prefix("0x").ok_or_else(|| anyhow!("{s:?}: not 0x hex"))?;
    hex::decode(h).map_err(|e| anyhow!("{s:?}: {e}"))
}

fn fixed<const N: usize>(v: &Value) -> Result<[u8; N]> {
    let b = bytes(v)?;
    b.as_slice().try_into().map_err(|_| anyhow!("{} bytes, {N} expected", b.len()))
}

fn bytes32(v: &Value) -> Result<Root> {
    fixed::<32>(v)
}

#[cfg(test)]
mod tests {
    //! Against bytes providers served as SSZ for the same requests (`tests/fixtures/lc-json-sepolia/SOURCE.md`): the
    //! JSON a provider answered, turned into SSZ here, equals the SSZ it (or another provider) served.
    use super::*;

    const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/lc-json-sepolia");

    fn json(name: &str) -> Value {
        serde_json::from_slice(&std::fs::read(format!("{DIR}/{name}")).unwrap()).unwrap()
    }
    fn ssz(name: &str) -> Vec<u8> {
        std::fs::read(format!("{DIR}/{name}")).unwrap()
    }

    #[test]
    fn bootstrap_json_is_the_served_ssz() {
        // Nimbus's JSON for the root of crates/ferrata-spectests/fixtures/sepolia-2026-11-12, against that fixture's
        // bootstrap.ssz (Lodestar's SSZ of 12 Nov).
        assert_eq!(bootstrap(&json("bootstrap.json")).unwrap(), ssz("bootstrap.ssz"));
    }

    #[test]
    fn updates_json_is_the_served_ssz() {
        let out = updates(&json("updates_1411.json")).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], ssz("update_1411.ssz"));
        // The SSZ stream of the same request, split as the updater splits it.
        assert_eq!(crate::chain::split_chunks(&ssz("updates_1411.stream")), out);
    }

    #[test]
    fn finality_update_json_is_the_served_ssz() {
        assert_eq!(finality_update(&json("finality_update.json")).unwrap(), ssz("finality_update.ssz"));
    }

    #[test]
    fn a_wrong_shape_is_refused() {
        let mut v = json("finality_update.json");
        v["data"]["finality_branch"].as_array_mut().unwrap().pop();
        assert!(finality_update(&v).is_err());
        let mut v = json("finality_update.json");
        v["version"] = "deneb".into();
        assert!(finality_update(&v).is_err(), "a Gloas object read as Deneb has no execution payload header");
        assert!(bootstrap(&serde_json::json!({"data": {}})).is_err());
        assert!(updates(&serde_json::json!({"version": "gloas"})).is_err());
    }

    #[test]
    fn uint256_is_little_endian() {
        let v = uint256_le(&Value::String("4660".into())).unwrap();
        assert_eq!((v[0], v[1], v[2]), (0x34, 0x12, 0));
        let max = "115792089237316195423570985008687907853269984665640564039457584007913129639935";
        assert_eq!(uint256_le(&Value::String(max.into())).unwrap(), [0xff; 32]);
        assert!(uint256_le(&Value::String(format!("{max}0"))).is_err());
    }
}
