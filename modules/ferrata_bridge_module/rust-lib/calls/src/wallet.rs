//! The paths that run the `ferrata` tool's own code (`ferrata-wallet`, which the tool links too) over
//! the chains' state given in the request, decoded with the bridge program's own types
//! (`ferrata-lez-bridge-wire`):
//!
//! * `claim` with LEZ state: the imported root a claim goes under, the program's checks, the accounts
//!   and the message, plaintext `Claim` or private `ClaimPrivately` (`ferrata_wallet::claim`, the
//!   tool's `lez::plan_claim`);
//! * `release` with the vault's accepted roots: the wallet's release rule, the newest accepted root
//!   whose release delay has passed (`ferrata_wallet::release`, the tool's `flow::pick_root`), and the
//!   witness and journal under it (the tool's `release::plan`);
//! * `prove_release`: the release guest run through r0vm on that input (the tool's `release --prove`);
//! * `import_root`: the relayer's `ImportRoot` from the vault's `eth_getProof` proofs, dry-run with the
//!   attestation and bridge programs' functions (`ferrata_wallet::import`);
//! * `registry`: `token(cid, token)`, `config(cid)` and `root(cid, n)` decoded, with the range and fees
//!   of a claim.

use borsh::BorshDeserialize as _;
use ferrata_bridge_core::{
    chain_id_u256,
    inbound::{self, DestKind, OwnedDeposit, VaultDeposit},
    outbound::{self, PublishedBurn},
    sha256, Address, Hash, U256,
};
use ferrata_lez_bridge_wire::{self as bw, BridgeConfig, BridgeState, ConfigState, RootState, TokenState};
use ferrata_lez_wire::RecordState;
use ferrata_wallet::{
    claim, import,
    release::{self, AcceptedRoot, ReleasePlan},
    AccountId, Context, Mention, Refusal,
};
use serde_json::{json, Value};

use crate::{bytes, chain, fixed, hex, keys, kind, kind_name, note_json, note_of, path_json, u256, uint};

/// How many unused burn counters end a burn scan (the tool's `lez::BURN_GAP`).
const BURN_GAP: u64 = 20;

// ---------------------------------------------------------------------------------------------
// Shared inputs and outputs
// ---------------------------------------------------------------------------------------------

fn account(v: &Value, what: &str) -> Result<AccountId, String> {
    Ok(AccountId::new(fixed::<32>(v, what)?))
}

/// The chain as a number: `chain_id`, or `evm_chain_id` when it fits in 64 bits.
fn chain_u64(req: &Value) -> Result<u64, String> {
    if !req["chain_id"].is_null() {
        return uint(&req["chain_id"], "chain_id");
    }
    let w = u256(&req["evm_chain_id"], "evm_chain_id")?;
    if w[..24].iter().any(|b| *b != 0) {
        return Err("evm_chain_id: above 2^64".into());
    }
    Ok(u64::from_be_bytes(w[24..].try_into().expect("8 bytes")))
}

/// A bridge state as LEZ holds it (`BridgeState`, Borsh, hex); `None` when absent or empty.
fn state(v: &Value, what: &str) -> Result<Option<BridgeState>, String> {
    if v.is_null() {
        return Ok(None);
    }
    let b = bytes(v, what)?;
    if b.is_empty() {
        return Ok(None);
    }
    BridgeState::try_from_slice(&b).map(Some).map_err(|e| format!("{what}: not a bridge state: {e}"))
}

fn word_u128(w: &U256, what: &str) -> Result<u128, String> {
    bw::u128_of(w).ok_or_else(|| format!("{what}: above 2^128"))
}

/// The name of a program's exit code: the bridge program's, else the attestation program's.
fn code_name(code: u8) -> &'static str {
    bw::code::ALL
        .iter()
        .chain(ferrata_lez_wire::code::ALL)
        .find(|(_, c)| *c == code)
        .map(|(n, _)| *n)
        .unwrap_or("UNKNOWN")
}

fn refused(r: Refusal) -> Value {
    match r {
        Refusal::Program { code, detail } => {
            json!({ "verdict": "refused", "code": code, "error": code_name(code), "detail": detail })
        }
        Refusal::Wallet(detail) => json!({ "verdict": "refused", "code": null, "error": detail }),
    }
}

pub(crate) fn mentions_json(mentions: &[Mention]) -> Value {
    Value::Array(
        mentions
            .iter()
            .map(|m| {
                let a = m.actor();
                let mut v = json!({
                    "account": hex(a.account_id.value()),
                    "program": hex(a.program_account_id.value()),
                    "kind": m.kind(),
                });
                if let Mention::Pda { seed, .. } = m {
                    v["seed"] = json!(hex(seed.as_bytes()));
                }
                v
            })
            .collect(),
    )
}

fn range_json(r: &bw::ScheduleRange) -> Value {
    json!({ "index": r.index, "from_leaf": r.from_leaf, "until_leaf": r.until_leaf })
}

fn leaves_of(req: &Value) -> Result<Vec<Hash>, String> {
    let leaves = req["leaves"].as_array().ok_or("the request needs `leaves` (the vault's deposit leaves, in order)")?;
    leaves.iter().enumerate().map(|(i, l)| fixed::<32>(l, &format!("leaves[{i}]"))).collect()
}

// ---------------------------------------------------------------------------------------------
// claim with LEZ state
// ---------------------------------------------------------------------------------------------

/// `claim` given `root_state` and `token_state`: the tool's claim (`ferrata_wallet::claim`).
pub(crate) fn claim_with_state(req: &Value) -> Result<Value, String> {
    let keys = keys(req)?;
    let chain_id = chain_u64(req)?;
    let chain = chain_id_u256(chain_id);
    let vault: Address = fixed(&req["vault"], "vault")?;
    let ctx = Context {
        program: account(&req["program"], "program")?,
        cid: fixed(&req["config_id"], "config_id")?,
        chain_id,
        vault,
    };
    let depositor: Address = fixed(&req["depositor"], "depositor")?;
    let j = uint(&req["j"], "j")?;
    let leaf_index = uint(&req["leaf_index"], "leaf_index")?;
    let token: Address = fixed(&req["token"], "token")?;
    let amount = u256(&req["amount"], "amount")?;
    let k = kind(&req["kind"])?;
    let dest: Hash = fixed(&req["dest"], "dest")?;
    let s = inbound::deposit_secret(&keys.auth, &chain, &vault, &depositor, j);
    let commitment = inbound::commitment(&s, k, &dest);
    let leaf = inbound::deposit_leaf(leaf_index, &depositor, &token, &amount, &commitment);
    let deposit_id = inbound::deposit_id(&chain, &vault, leaf_index, &leaf);
    let nullifier = inbound::nullifier(&s, &deposit_id);
    let owned = OwnedDeposit {
        deposit: VaultDeposit { depositor, j, leaf_index, token, amount, commitment },
        s,
        kind: k,
        account_index: 0,
        dest,
        leaf,
        nullifier,
    };
    let leaves = leaves_of(req)?;
    // The imported root it goes under: given, or the newest of the configuration's imports that covers it.
    let n = if !req["leaf_count"].is_null() {
        uint(&req["leaf_count"], "leaf_count")?
    } else {
        let imported = req["imported"]
            .as_array()
            .ok_or("the request needs `imported` (the leaf counts of the imported roots) or `leaf_count`")?
            .iter()
            .enumerate()
            .map(|(i, v)| uint(v, &format!("imported[{i}]")))
            .collect::<Result<Vec<u64>, String>>()?;
        match claim::pick_import(&imported, leaf_index) {
            Ok(n) => n,
            Err(r) => return Ok(refused(r)),
        }
    };
    let root = match state(&req["root_state"], "root_state")? {
        Some(BridgeState::Root(r)) => r,
        _ => return Ok(refused(Refusal::program(bw::code::NOT_FOUND, format!("root(cid, {n}) is not on the zone")))),
    };
    let token_state = match state(&req["token_state"], "token_state")? {
        Some(BridgeState::Token(t)) => t,
        _ => return Ok(refused(Refusal::program(bw::code::NOT_REGISTERED, "no token(cid, token) account"))),
    };
    let spent = state(&req["nullifier_state"], "nullifier_state")?.is_some() || req["claimed"].as_bool() == Some(true);
    let private = req["private"].as_bool().unwrap_or(k == DestKind::Private);
    let relayer = if req["relayer"].is_null() { None } else { Some(fixed::<32>(&req["relayer"], "relayer")?) };
    let prover = if req["prover"].is_null() { None } else { Some(account(&req["prover"], "prover")?) };
    let inputs = claim::ClaimInputs { leaves: &leaves, leaf_count: n, root: &root, token: &token_state, spent };
    Ok(match claim::build_claim(&ctx, &owned, &inputs, private, relayer, prover) {
        Ok(plan) => {
            let m = borsh::to_vec(&plan.message).expect("borsh");
            json!({
                "verdict": "claimable",
                "what": plan.what,
                "private": private,
                "variant": if private { "ClaimPrivately" } else { "Claim" },
                "message": hex(&m),
                "message_bytes": m.len(),
                "accounts": mentions_json(&plan.mentions),
                "root_index": plan.root_index,
                "leaf_count": plan.leaf_count,
                "root": hex(&root.root),
                "nullifier": hex(&plan.nullifier),
                "leaf": hex(&leaf),
                "commitment": hex(&commitment),
                "deposit_id": hex(&deposit_id),
                "kind": kind_name(k),
                "path": path_json(&plan.path),
                "range": range_json(&plan.range),
                "mint_fee": plan.mint_fee.to_string(),
                "relay_fee": plan.relay_fee.to_string(),
            })
        }
        Err(r) => refused(r),
    })
}

// ---------------------------------------------------------------------------------------------
// release by the wallet's rule, and the release guest
// ---------------------------------------------------------------------------------------------

/// The zone's published burns of the configuration, in order: `{burn_index, note}`.
fn burns_of(req: &Value) -> Result<Vec<PublishedBurn>, String> {
    let burns = req["burns"].as_array().ok_or("the request needs `burns` (the zone's published burns, in order)")?;
    burns
        .iter()
        .enumerate()
        .map(|(k, b)| {
            Ok(PublishedBurn {
                burn_index: uint(&b["burn_index"], &format!("burns[{k}].burn_index"))?,
                note: note_of(&b["note"], &format!("burns[{k}].note"))?,
            })
        })
        .collect()
}

fn roots_of(req: &Value) -> Result<Vec<(AcceptedRoot, u64)>, String> {
    let roots = req["roots"].as_array().ok_or("roots: expected an array")?;
    roots
        .iter()
        .enumerate()
        .map(|(k, r)| {
            let w = |f: &str| format!("roots[{k}].{f}");
            Ok((
                AcceptedRoot {
                    root: fixed(&r["root"], &w("root"))?,
                    count: uint(&r["count"], &w("count"))?,
                    index: uint(&r["index"], &w("index"))?,
                    finalized_at: uint(&r["finalized_at"], &w("finalized_at"))?,
                },
                uint(&r["delay"], &w("delay"))?,
            ))
        })
        .collect()
}

/// A release planned by the tool's code: `Ok((reply, plan))` or a refusal reply.
pub(crate) fn release_by_rule(req: &Value) -> Result<Result<(Value, ReleasePlan), Value>, String> {
    let keys = keys(req)?;
    let cid: Hash = fixed(&req["config_id"], "config_id")?;
    let chain = chain(req)?;
    let vault: Address = fixed(&req["vault"], "vault")?;
    let burns = burns_of(req)?;
    let gap = if req["gap_limit"].is_null() { BURN_GAP } else { uint(&req["gap_limit"], "gap_limit")? };
    let scan = outbound::recover_burns(&keys, &cid, &chain, &vault, &burns, gap);
    let i = uint(&req["i"], "i")?;
    let Some(owned) = scan.owned.iter().find(|o| o.i == i) else {
        return Ok(Err(json!({ "verdict": "refused", "code": null, "error": format!("no burn with counter {i} of this seed among the burns given") })));
    };
    let roots = roots_of(req)?;
    let now = uint(&req["now"], "now")?;
    let (root, delay) = match release::pick_root(&roots, owned.burn.burn_index, now) {
        Ok(x) => x,
        Err(r) => {
            let mut v = json!({ "verdict": "refused", "code": null, "error": r.name(), "burn_index": owned.burn.burn_index });
            if let release::RootRefusal::DelayNotElapsed { first_open } = r {
                v["first_open"] = json!(first_open);
            }
            return Ok(Err(v));
        }
    };
    let relayer: Address = if req["relayer"].is_null() { [0; 20] } else { fixed(&req["relayer"], "relayer")? };
    let relay_fee = if req["relay_fee"].is_null() { 0 } else { word_u128(&u256(&req["relay_fee"], "relay_fee")?, "relay_fee")? };
    let plan = match release::plan(&cid, &chain, &vault, &burns, owned, root.root, root.count, relayer, relay_fee) {
        Ok(p) => p,
        Err(r) => return Ok(Err(refused(r))),
    };
    let p = &plan.input.public;
    let reply = json!({
        "verdict": "releasable",
        "picked": {
            "root": hex(&root.root),
            "count": root.count,
            "index": root.index,
            "finalized_at": root.finalized_at,
            "delay": delay,
            "opens": release::RootRule::opens(&root, delay),
            "rule": "the newest accepted burn root that covers the burn and whose release delay has passed",
        },
        "i": owned.i,
        "burn_index": owned.burn.burn_index,
        "note": note_json(&owned.burn.note),
        "nullifier": hex(&plan.nullifier),
        "burn_root": hex(&plan.input.burn_root),
        "journal": hex(&plan.journal),
        "journal_sha256": hex(&sha256(&plan.journal)),
        "input": hex(&plan.input.encode()),
        "path": path_json(&plan.input.witness.path),
        "public": {
            "evm_chain_id": hex(&p.chain_id),
            "vault": hex(&p.vault),
            "config_id": hex(&p.config_id),
            "token": hex(&p.token),
            "amount": hex(&p.amount),
            "recipient": hex(&p.recipient),
            "relayer": hex(&p.relayer),
            "relay_fee": hex(&p.relay_fee),
        },
    });
    Ok(Ok((reply, plan)))
}

/// `prove_release(request)`: the release request, then the release guest (`program`: its ELF) run on
/// its input through r0vm (`ferrata_wallet::release::prove`, the tool's `release --prove`).
pub(crate) fn prove_release_of(req: &Value) -> Result<Value, String> {
    let (mut out, input, journal) = if !req["roots"].is_null() {
        match release_by_rule(req)? {
            Ok((v, plan)) => (v, plan.input.encode().to_vec(), plan.journal),
            Err(refusal) => return Ok(refusal),
        }
    } else {
        match crate::release_explicit(req)? {
            Ok((v, input, journal)) => (v, input, journal),
            Err(refusal) => return Ok(refusal),
        }
    };
    let program = crate::text(&req["program"], "program")?;
    let elf = std::fs::read(program).map_err(|e| format!("program: reading {program}: {e}"))?;
    let t0 = std::time::Instant::now();
    let proven = match release::prove(&elf, &input, &journal) {
        Ok(p) => p,
        Err(e) => return Ok(json!({ "verdict": "failed", "error": e, "journal_sha256": out["journal_sha256"].clone() })),
    };
    out["verdict"] = json!("proven");
    out["image_id"] = json!(hex(&proven.image_id));
    out["dev_mode"] = json!(proven.dev_mode);
    out["receipt"] = json!(if proven.dev_mode { "fake (RISC0_DEV_MODE: executed, not proven)" } else { "succinct, verified against the image id" });
    out["receipt_bytes"] = json!(proven.receipt.len());
    out["receipt_sha256"] = json!(hex(&sha256(&proven.receipt)));
    out["prove_ms"] = json!(t0.elapsed().as_millis() as u64);
    if !req["expected_image_id"].is_null() {
        out["image_id_matches"] = json!(fixed::<32>(&req["expected_image_id"], "expected_image_id")? == proven.image_id);
    }
    if !req["receipt_out"].is_null() {
        let path = crate::text(&req["receipt_out"], "receipt_out")?;
        std::fs::write(path, &proven.receipt).map_err(|e| format!("receipt_out: writing {path}: {e}"))?;
        out["receipt_out"] = json!(path);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// import_root
// ---------------------------------------------------------------------------------------------

/// A storage proof of `eth_getProof` for slot `slot`: (its proof nodes, its value as a word).
fn storage_entry(proofs: &[Value], slot: u64) -> Result<(Vec<Vec<u8>>, U256), String> {
    for (k, p) in proofs.iter().enumerate() {
        if u256(&p["key"], &format!("storageProof[{k}].key"))? == crate::word_u64(slot) {
            let nodes = node_list(&p["proof"], &format!("storageProof[{k}].proof"))?;
            return Ok((nodes, u256(&p["value"], &format!("storageProof[{k}].value"))?));
        }
    }
    Err(format!("get_proof: no storage proof of slot {slot}"))
}

fn node_list(v: &Value, what: &str) -> Result<Vec<Vec<u8>>, String> {
    v.as_array()
        .ok_or_else(|| format!("{what}: expected an array"))?
        .iter()
        .enumerate()
        .map(|(i, n)| bytes(n, &format!("{what}[{i}]")))
        .collect()
}

fn config_of(req: &Value) -> Result<(BridgeConfig, &'static str, Option<bool>), String> {
    match state(&req["config_state"], "config_state")? {
        Some(BridgeState::Config(c)) => {
            let (phase, sunset) = match &c {
                ConfigState::Pairing { .. } => ("pairing", None),
                ConfigState::Ready { sunset, .. } => ("ready", Some(*sunset)),
            };
            Ok((c.config().clone(), phase, sunset))
        }
        _ => Err("config_state: the request needs config(cid)'s state (a bridge configuration)".into()),
    }
}

fn root_state_json(r: &RootState) -> Value {
    json!({
        "config_id": hex(&r.config_id),
        "evm_chain_id": r.evm_chain_id,
        "vault": hex(&r.vault),
        "leaf_count": r.leaf_count,
        "root": hex(&r.root),
        "record_slot": r.record_slot,
        "block_number": r.block_number,
    })
}

/// `import_root(request)`: the relayer's `ImportRoot` (`ferrata_wallet::import`).
pub(crate) fn import_root_of(req: &Value) -> Result<Value, String> {
    let program = account(&req["program"], "program")?;
    let cid: Hash = fixed(&req["config_id"], "config_id")?;
    let (config, _, _) = config_of(req)?;
    let record_slot = uint(&req["record_slot"], "record_slot")?;
    // The vault's `eth_getProof` of slots 0 (the root) and 1 (the leaf count): given in the request
    // (`get_proof`), or read by the module at `block` from the vault the configuration names, through
    // Logos Core's eth_rpc_module (`eth`) or a plain http:// endpoint (`rpc_url`): rpc.rs.
    let mut read = None;
    let read_gp;
    let gp = if !req["get_proof"].is_null() {
        if req["get_proof"]["result"].is_object() { &req["get_proof"]["result"] } else { &req["get_proof"] }
    } else {
        let block = uint(&req["block"], "block (get_proof absent: the block the vault's proofs are read at)")?;
        let timeout = if req["timeout_ms"].is_null() { 20_000 } else { uint(&req["timeout_ms"], "timeout_ms")? };
        let mut rpc = crate::rpc::Rpc::from_request(req, timeout).map_err(|e| format!("get_proof absent: {e}"))?;
        let slots: Vec<String> = [0u64, 1].iter().map(|s| hex(&crate::word_u64(*s))).collect();
        read_gp = rpc.call("eth_getProof", json!([hex(&config.vault), slots, format!("0x{block:x}")]))?;
        let mut report = rpc.report();
        report["block"] = json!(block);
        report["address"] = json!(hex(&config.vault));
        read = Some(report);
        &read_gp
    };
    let account_proof = node_list(&gp["accountProof"], "get_proof.accountProof")?;
    let storage = gp["storageProof"].as_array().ok_or("get_proof.storageProof: expected an array")?;
    let (root_nodes, root_value) = storage_entry(storage, 0)?;
    let (count_nodes, count_value) = storage_entry(storage, 1)?;
    let root: Hash = if req["root"].is_null() { root_value } else { fixed(&req["root"], "root")? };
    let leaf_count = if req["leaf_count"].is_null() {
        bw::u64_of(&count_value).ok_or("get_proof: slot 1 (the leaf count) is above 2^64")?
    } else {
        uint(&req["leaf_count"], "leaf_count")?
    };
    let header = if req["header"].is_null() { None } else { Some(bytes(&req["header"], "header")?) };
    let proof = import::import_proof(account_proof, root_nodes, count_nodes, header);
    let call = import::import_call(program, &cid, &config, record_slot, root, leaf_count, proof.clone());
    let m = borsh::to_vec(&call.message).expect("borsh");
    let mut out = json!({
        "verdict": "built",
        "what": call.what,
        "variant": "ImportRoot",
        "message": hex(&m),
        "message_bytes": m.len(),
        "accounts": mentions_json(&call.mentions),
        "root_index": call.root_index,
        "root": hex(&root),
        "leaf_count": leaf_count,
        "record_slot": record_slot,
        "instance_id": hex(&config.instance_id),
        "vault": hex(&config.vault),
    });
    if let Some(r) = read {
        out["eth"] = r;
    }
    if !req["record_state"].is_null() {
        let b = bytes(&req["record_state"], "record_state")?;
        let record = RecordState::try_from_slice(&b).map_err(|e| format!("record_state: not a RecordState: {e}"))?;
        match import::dry_run_import(&cid, &config, &record, record_slot, root, leaf_count, &proof) {
            Ok(written) => {
                out["verdict"] = json!("importable");
                out["root_state"] = root_state_json(&written);
            }
            Err(code) => {
                out["verdict"] = json!("refused");
                out["code"] = json!(code);
                out["error"] = json!(code_name(code));
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// registry
// ---------------------------------------------------------------------------------------------

fn cap_json(c: &bw::CapParams) -> Value {
    json!({ "capacity": c.capacity.to_string(), "refill_per_second": c.refill_per_second.to_string() })
}

fn bucket_json(b: &bw::Bucket) -> Value {
    json!({
        "capacity": b.capacity.to_string(),
        "refill_per_second": b.refill_per_second.to_string(),
        "level": b.level.to_string(),
        "last_ms": b.last_ms,
    })
}

fn controls_json(c: &bw::Controls) -> Value {
    json!({
        "admin": hex(&c.admin),
        "freeze_authority": hex(&c.freeze_authority),
        "fee_recipient": hex(&c.fee_recipient),
        "floor": cap_json(&c.floor),
        "token_cap": cap_json(&c.token_cap),
    })
}

fn gate_json(g: &bw::Gate) -> Value {
    json!({
        "now_ms": g.now_ms,
        "mints_until": g.mints_until,
        "burns_until": g.burns_until,
        "mint": bucket_json(&g.mint),
        "burn": bucket_json(&g.burn),
    })
}

fn strings(v: &[u128]) -> Vec<String> {
    v.iter().map(u128::to_string).collect()
}

fn token_json(t: &TokenState) -> Value {
    let e = &t.entry;
    json!({
        "config_id": hex(&t.config_id),
        "evm_chain_id": t.evm_chain_id,
        "vault": hex(&t.vault),
        "attestation": hex(t.attestation.value()),
        "instance_id": hex(&t.instance_id),
        "entry": {
            "token": hex(&e.token),
            "lez_definition": hex(&e.lez_definition),
            "decimals": e.decimals,
            "lez_unit": e.lez_unit.to_string(),
            "unit_weight": e.unit_weight.to_string(),
            "denominations": strings(&e.denominations),
            "relay_fees": strings(&e.relay_fees),
        },
        "schedules": t.schedules.iter().map(|s| json!({
            "index": s.index,
            "from_leaf": s.from_leaf,
            "announced_at": s.announced_at,
            "effective_at": s.effective_at,
            "mint_fees": strings(&s.mint_fees),
            "burn_fees": strings(&s.burn_fees),
            "checkpoint_fees": strings(&s.checkpoint_fees),
        })).collect::<Vec<_>>(),
        "known_until": t.known_until,
        "controls": controls_json(&t.controls),
        "gate": gate_json(&t.gate),
        "pending_time": t.pending.is_some(),
    })
}

/// `registry(request)`: the bridge program's registry and configuration as LEZ holds them.
pub(crate) fn registry_of(req: &Value) -> Result<Value, String> {
    let mut out = json!({ "verdict": "ok" });
    let mut any = false;
    if !req["token_state"].is_null() {
        any = true;
        let t = match state(&req["token_state"], "token_state")? {
            Some(BridgeState::Token(t)) => t,
            other => return Err(format!("token_state: not a token(cid, token) state: {other:?}")),
        };
        out["token"] = token_json(&t);
        if !req["leaf_index"].is_null() {
            let leaf = uint(&req["leaf_index"], "leaf_index")?;
            let range = claim::range_for(&t, leaf);
            out["range"] = range_json(&range);
            if !req["amount"].is_null() {
                let amount = word_u128(&u256(&req["amount"], "amount")?, "amount")?;
                out["claim"] = match t.fees_for(amount, &range) {
                    Ok((mint_fee, relay_fee)) => json!({
                        "denomination_index": t.entry.index_of(amount),
                        "mint_fee": mint_fee.to_string(),
                        "relay_fee": relay_fee.to_string(),
                        "value": t.entry.value(amount).map(|v| v.to_string()),
                    }),
                    Err(code) => json!({ "refused": code, "error": code_name(code) }),
                };
            }
        }
    }
    if !req["config_state"].is_null() {
        any = true;
        let (c, phase, sunset) = config_of(req)?;
        let mut v = json!({
            "phase": phase,
            // The one-way retirement (`Sunset`): no mint after it.
            "sunset": sunset,
            "bedrock_genesis_id": hex(&c.bedrock_genesis_id),
            "channel_id": hex(&c.channel_id),
            "bridge_image_id": hex(&c.bridge_image_id),
            "attestation": hex(c.attestation.value()),
            "instance_id": hex(&c.instance_id),
            "evm_chain_id": c.evm_chain_id,
            "vault": hex(&c.vault),
            "controls": controls_json(&c.controls),
            "attestation_root_id": hex(&c.attestation_root_id()),
        });
        // `configId` as the program computes it, for the program's account (`bridge_header`).
        if !req["bridge_header"].is_null() {
            v["config_id"] = json!(hex(&c.id(account(&req["bridge_header"], "bridge_header")?)));
        }
        out["config"] = v;
    }
    if !req["root_state"].is_null() {
        any = true;
        out["root"] = match state(&req["root_state"], "root_state")? {
            Some(BridgeState::Root(r)) => root_state_json(&r),
            other => return Err(format!("root_state: not a root(cid, n) state: {other:?}")),
        };
    }
    if !any {
        return Err("the request needs `token_state`, `config_state` or `root_state`".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use ferrata_bridge_core::{
        outbound::{burn_leaf, BurnNote, ReleaseTarget},
        tree::Tree,
        BridgeKeys,
    };

    use super::*;

    /// The release rule through the module's JSON interface: three burns of one wallet, a root over
    /// the first two (finalised at 1000) and one over all three (at 1100), each with a 120 s delay.
    #[test]
    fn release_by_the_wallet_rule() {
        let seed = [7u8; 32];
        let keys = BridgeKeys::from_bridge_seed(&seed);
        let (cid, vault, recipient) = ([0xc1u8; 32], [0x11u8; 20], [0x22u8; 20]);
        let target = ReleaseTarget { chain_id: chain_id_u256(1), vault, recipient };
        let amount = bw::word_of_u128(100_000_000_000_000_000);
        let burns: Vec<PublishedBurn> = (0..3u64)
            .map(|i| PublishedBurn { burn_index: i, note: BurnNote::create(&keys, &cid, i, &target, [0; 20], amount) })
            .collect();
        let leaf = |b: &PublishedBurn| burn_leaf(&cid, b.burn_index, &b.note.token, &b.note.amount, &b.note.c_out, &b.note.r);
        let leaves: Vec<Hash> = burns.iter().map(leaf).collect();
        let root_at = |n: usize| Tree::from_leaves(leaves[..n].to_vec()).unwrap().root();
        let request = |i: u64, now: u64| {
            json!({
                "bridge_seed": hex(&seed), "config_id": hex(&cid), "chain_id": 1, "vault": hex(&vault), "i": i,
                "burns": burns.iter().map(|b| json!({ "burn_index": b.burn_index, "note": note_json(&b.note) })).collect::<Vec<_>>(),
                "roots": [
                    { "root": hex(&root_at(2)), "count": 2, "index": 0, "finalized_at": 1000, "delay": 120 },
                    { "root": hex(&root_at(3)), "count": 3, "index": 1, "finalized_at": 1100, "delay": 120 },
                ],
                "now": now,
            })
        };
        let call = |r: Value| -> Value { serde_json::from_str(&crate::release(&r.to_string())).unwrap() };
        // At 1150 the newest root (opens at 1220) is in its delay: burn 0 goes under the older one.
        let out = call(request(0, 1150));
        assert_eq!(out["verdict"], "releasable", "{out}");
        assert_eq!(out["picked"]["index"], 0);
        assert_eq!(out["burn_root"], json!(hex(&root_at(2))));
        // After 1220, the newest.
        let later = call(request(0, 1221));
        assert_eq!(later["picked"]["index"], 1);
        assert_eq!(later["burn_root"], json!(hex(&root_at(3))));
        // Burn 2 is under the newest root only: refused until it opens.
        let early = call(request(2, 1150));
        assert_eq!(early["error"], "ReleaseDelayNotElapsed", "{early}");
        assert_eq!(early["first_open"], 1220);
        assert_eq!(call(request(2, 1221))["picked"]["index"], 1);
        // The same release with the root given explicitly gives the same journal.
        let explicit = json!({
            "bridge_seed": hex(&seed), "config_id": hex(&cid), "chain_id": 1, "vault": hex(&vault), "i": 0,
            "burn_index": 0, "leaves": leaves[..2].iter().map(|l| hex(l)).collect::<Vec<_>>(),
            "token": hex(&[0u8; 20]), "amount": hex(&amount), "recipient": hex(&recipient),
        });
        let e = call(explicit);
        assert_eq!(e["verdict"], "releasable", "{e}");
        assert_eq!(e["journal"], out["journal"]);
        assert_eq!(e["nullifier"], out["nullifier"]);
        // No root covers a fourth burn.
        let mut none = request(0, 5000);
        none["roots"] = json!([]);
        assert_eq!(call(none)["error"], "BurnRootNotAccepted");
    }
}
