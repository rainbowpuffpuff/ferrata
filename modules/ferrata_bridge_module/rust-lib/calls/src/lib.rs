//! The methods of `ferrata_bridge_module` as plain functions over JSON strings, so that they run and
//! are tested without the module scaffold. Every function returns one JSON object. A request that
//! cannot be read returns `{"verdict":"invalid_request","error":...}`.
//!
//! The cryptography is `ferrata-bridge-core`'s (specification v2), the crate the bridge program, the
//! release guest and the CLI use, so commitments, nullifiers, paths and journals cannot drift. The
//! bridge program's messages and states are its own types (`ferrata-lez-bridge-wire`, staged with
//! LEZ's `lee_core` by `scripts/stage.py`). The claim with LEZ state, the release rule and the release
//! guest run, the root import and the registry run the `ferrata` tool's own code (`ferrata-wallet`,
//! `wallet.rs`). The vault's calls are ABI-encoded here.
//!
//! Secrets: a request carries the wallet's `bridge_seed` (or the LEZ wallet's 64-byte `lez_seed`, from
//! which the bridge seed is derived as the CLI derives it). Replies never contain `kAuth`, `kView`, a
//! deposit secret `s` or a burn secret `sOut`, except where the bridge program needs it: the
//! plaintext `Claim` message carries `s`, as the program's public claim does, and the `ClaimPrivately`
//! message carries it as the private execution's input (the LEZ wallet runs it privately; it is not
//! sent in the clear).

use std::fmt::Write as _;

mod admin;
mod evm;
mod jobs;
mod lez;
mod rpc;
mod send;
mod session;
mod user;
mod wallet;

pub use lez::{set_lez_core, LezCoreCall};
pub use rpc::{set_eth_rpc_module, EthRpcModuleCall};
pub use send::{set_module_call, ModuleCall};

use ferrata_bridge_core::{
    chain_id_u256,
    encoding::ReleaseInput,
    hmac_sha256,
    inbound::{self, ClaimWitness, DestKind, VaultDeposit},
    outbound::{self, BurnNote, PublishedBurn, ReleasePublic, ReleaseTarget, ReleaseWitness},
    sha256,
    tree::{MerklePath, Tree, TREE_DEPTH},
    Address, BridgeKeys, Hash, VerifyError, U256,
};
use ferrata_lez_bridge_wire::{self as bw, BridgeMessage};
use serde_json::{json, Value};

/// What `scripts/stage.py` recorded about the crates staged beside this one (the commit, LEZ's, each
/// crate's version and sources' SHA-256).
const STAGED: &str = include_str!("../../staged/STAGED.json");

/// The label of the bridge seed under the LEZ wallet's seed (`cli/src/seed.rs`).
pub const BRIDGE_SEED_LABEL: &[u8] = b"ferrata/bridge-seed/v1";


/// `version()`: the module's version and the bridge core it was built from.
pub fn version() -> String {
    json!({
        "module": "ferrata_bridge_module",
        "version": env!("CARGO_PKG_VERSION"),
        "staged": serde_json::from_str::<Value>(STAGED).unwrap_or(Value::Null),
        "spec": "v2",
        "tool_code": "ferrata-wallet (claim with LEZ state, release rule, release guest run, root import)",
        "methods": ["version", "deposit", "claim", "burn", "release", "recover", "status", "prove_release", "import_root", "registry", "wallet", "admin"],
        "ethereum": "reads through Logos Core's eth_rpc_module (or a plain http:// endpoint); sends through Logos Core's tx_sender_module (one approval in the Logos signer per send); the module holds no Ethereum key",
    })
    .to_string()
}

/// `deposit(request)`: the commitment and the vault call of a deposit.
/// With `send`, the deposit is sent through Logos Core's send stack from the depositor's account (`user.rs`).
pub fn deposit(request: &str) -> String {
    reply(parse(request).and_then(|r| if r["send"].is_null() { deposit_of(&r) } else { user::deposit_and_send(&r) }))
}

/// `claim(request)`: the claim of one deposit against a deposit root.
pub fn claim(request: &str) -> String {
    reply(parse(request).and_then(|r| {
        if r["root_state"].is_null() && r["token_state"].is_null() {
            claim_of(&r)
        } else {
            wallet::claim_with_state(&r)
        }
    }))
}

/// `burn(request)`: a burn note and the bridge program's `Burn` message.
pub fn burn(request: &str) -> String {
    reply(parse(request).and_then(|r| burn_of(&r)))
}

/// `release(request)`: the release of one burn against a checkpointed burn root.
/// With `send` (and the proof's `seal`), a releasable one is sent through Logos Core's send stack (`user.rs`).
pub fn release(request: &str) -> String {
    reply(parse(request).and_then(|r| {
        let made = if r["roots"].is_null() {
            release_explicit(&r).map(|x| x.map(|(v, _, _)| v))
        } else {
            wallet::release_by_rule(&r).map(|x| x.map(|(v, _)| v))
        }?;
        match made {
            Ok(v) if !r["send"].is_null() => user::release_and_send(&r, &v),
            Ok(v) => Ok(v),
            Err(refusal) => Ok(refusal),
        }
    }))
}

/// `prove_release(request)`: a release (as `release`), then the release guest run on its input
/// through r0vm, its journal checked against the native one.
pub fn prove_release(request: &str) -> String {
    reply(parse(request).and_then(|r| wallet::prove_release_of(&r)))
}

/// `import_root(request)`: the relayer's `ImportRoot` of a deposit root from the vault's proofs,
/// dry-run with the programs' own functions when the record is given.
pub fn import_root(request: &str) -> String {
    reply(parse(request).and_then(|r| wallet::import_root_of(&r)))
}

/// `registry(request)`: the bridge program's token, configuration and root states, decoded.
pub fn registry(request: &str) -> String {
    reply(parse(request).and_then(|r| wallet::registry_of(&r)))
}

/// `recover(request)`: the wallet's deposits and burns among public ones.
pub fn recover(request: &str) -> String {
    reply(parse(request).and_then(|r| recover_of(&r)))
}

/// `status(request)`: where each deposit and burn stands.
pub fn status(request: &str) -> String {
    reply(parse(request).and_then(|r| status_of(&r)))
}

/// `admin(request)`: the bridge's admin and freeze-authority functions (`admin.rs`): `status` (both sides' roles,
/// registry and controls), a vault action built, checked and (with `send`) sent through Logos Core's send stack, or a
/// LEZ action's message (`lez_*`).
pub fn admin(request: &str) -> String {
    reply(parse(request).and_then(|r| admin::admin_of(&r)))
}

/// `wallet(request)`: one call of the mini-app's view (`{"method", "params"}`), run with this module's
/// methods over the chains a chain service reads and sends to (`session.rs`); `{"ok", "result"|"error"}`.
pub fn wallet(request: &str) -> String {
    session::wallet(request)
}

fn parse(request: &str) -> Result<Value, String> {
    serde_json::from_str(request).map_err(|e| format!("request is not JSON: {e}"))
}

fn reply(result: Result<Value, String>) -> String {
    result.unwrap_or_else(|error| json!({ "verdict": "invalid_request", "error": error })).to_string()
}

fn verify_error(e: VerifyError) -> &'static str {
    match e {
        VerifyError::DirectionsMismatch => "DirectionsMismatch",
        VerifyError::RootMismatch => "RootMismatch",
    }
}

// ---------------------------------------------------------------------------------------------
// Inputs shared by the methods
// ---------------------------------------------------------------------------------------------

/// The wallet's keys: `bridge_seed`, or `lez_seed` (the BIP-39 seed of the LEZ wallet).
pub(crate) fn keys(req: &Value) -> Result<BridgeKeys, String> {
    if !req["bridge_seed"].is_null() {
        let seed = bytes(&req["bridge_seed"], "bridge_seed")?;
        return Ok(BridgeKeys::from_bridge_seed(&seed));
    }
    if !req["lez_seed"].is_null() {
        let seed = bytes(&req["lez_seed"], "lez_seed")?;
        return Ok(BridgeKeys::from_bridge_seed(&hmac_sha256(&seed, BRIDGE_SEED_LABEL)));
    }
    Err("the request needs `bridge_seed` or `lez_seed`".into())
}

/// `evm_chain_id` (a 32-byte word) or `chain_id` (a number).
pub(crate) fn chain(req: &Value) -> Result<U256, String> {
    if !req["evm_chain_id"].is_null() {
        return u256(&req["evm_chain_id"], "evm_chain_id");
    }
    Ok(chain_id_u256(uint(&req["chain_id"], "chain_id")?))
}

pub(crate) fn kind(v: &Value) -> Result<DestKind, String> {
    let k = match v {
        Value::String(s) if s == "private" => 1,
        Value::String(s) if s == "public" => 2,
        _ => uint(v, "kind")?,
    };
    u8::try_from(k).ok().and_then(DestKind::from_byte).ok_or_else(|| format!("kind: {k} is not 1 (private) or 2 (public)"))
}

pub(crate) fn kind_name(k: DestKind) -> &'static str {
    match k {
        DestKind::Private => "private",
        DestKind::Public => "public",
    }
}

/// A Merkle path: `path` given (`directions`, 32 `siblings`), or built from all the tree's
/// `leaves`. Returns the path and, when built, the tree's root.
fn path_of(req: &Value, leaf_index: u64) -> Result<(MerklePath, Option<Hash>), String> {
    if let Some(leaves) = req["leaves"].as_array() {
        let mut ls = Vec::with_capacity(leaves.len());
        for (i, l) in leaves.iter().enumerate() {
            ls.push(fixed::<32>(l, &format!("leaves[{i}]"))?);
        }
        let tree = Tree::from_leaves(ls).map_err(|_| "leaves: more than 2^32 leaves")?;
        let path = tree
            .path(leaf_index)
            .ok_or_else(|| format!("leaf index {leaf_index} is not below the {} leaves given", leaves.len()))?;
        return Ok((path, Some(tree.root())));
    }
    let p = &req["path"];
    if p.is_null() {
        return Err("the request needs `leaves` (the whole tree) or `path`".into());
    }
    let siblings = p["siblings"].as_array().ok_or("path.siblings: expected an array")?;
    if siblings.len() != TREE_DEPTH {
        return Err(format!("path.siblings: expected {TREE_DEPTH}, got {}", siblings.len()));
    }
    let mut s = [[0u8; 32]; TREE_DEPTH];
    for (i, h) in siblings.iter().enumerate() {
        s[i] = fixed::<32>(h, &format!("path.siblings[{i}]"))?;
    }
    let directions = u32::try_from(uint(&p["directions"], "path.directions")?).map_err(|_| "path.directions: above 2^32")?;
    Ok((MerklePath { directions, siblings: s }, None))
}

pub(crate) fn path_json(p: &MerklePath) -> Value {
    json!({ "directions": p.directions, "siblings": p.siblings.iter().map(|s| hex(s)).collect::<Vec<_>>() })
}

/// The root to check against: `root` (or `burn_root`) if given, else the root of the `leaves`.
fn root_of(req: &Value, key: &str, built: Option<Hash>) -> Result<Hash, String> {
    match (&req[key], built) {
        (v, Some(b)) if !v.is_null() => {
            let given = fixed::<32>(v, key)?;
            if given != b {
                return Err(format!("{key} {} is not the root of the leaves given ({})", hex(&given), hex(&b)));
            }
            Ok(given)
        }
        (v, None) if !v.is_null() => fixed::<32>(v, key),
        (_, Some(b)) => Ok(b),
        _ => Err(format!("the request needs `{key}` or `leaves`")),
    }
}

// ---------------------------------------------------------------------------------------------
// deposit
// ---------------------------------------------------------------------------------------------

fn selector(signature: &str) -> [u8; 4] {
    use tiny_keccak::{Hasher, Keccak};
    let mut k = Keccak::v256();
    k.update(signature.as_bytes());
    let mut out = [0u8; 32];
    k.finalize(&mut out);
    [out[0], out[1], out[2], out[3]]
}

pub(crate) fn word_u64(n: u64) -> [u8; 32] {
    let mut w = [0u8; 32];
    w[24..].copy_from_slice(&n.to_be_bytes());
    w
}

fn word_address(a: &Address) -> [u8; 32] {
    let mut w = [0u8; 32];
    w[12..].copy_from_slice(a);
    w
}

/// The vault's `deposit(token, denomIndex, commitment, index)`, or `depositETH(denomIndex,
/// commitment, index)` for the zero token, as `cli/src/eth.rs` calls them.
fn vault_call(token: &Address, denom_index: u8, c: &Hash, j: u64) -> (String, Vec<u8>) {
    let eth = token.iter().all(|b| *b == 0);
    let signature =
        if eth { "depositETH(uint8,bytes32,uint64)" } else { "deposit(address,uint8,bytes32,uint64)" };
    let mut data = selector(signature).to_vec();
    if !eth {
        data.extend_from_slice(&word_address(token));
    }
    data.extend_from_slice(&word_u64(u64::from(denom_index)));
    data.extend_from_slice(c);
    data.extend_from_slice(&word_u64(j));
    (signature.to_string(), data)
}

fn deposit_of(req: &Value) -> Result<Value, String> {
    let keys = keys(req)?;
    let chain_id = chain(req)?;
    let vault: Address = fixed(&req["vault"], "vault")?;
    let depositor: Address = fixed(&req["depositor"], "depositor")?;
    let j = uint(&req["j"], "j")?;
    let kind = kind(&req["kind"])?;
    let dest: Hash = fixed(&req["dest"], "dest")?;
    let token: Address = fixed(&req["token"], "token")?;
    let denom_index = if req["denom_index"].is_null() { 0 } else { uint(&req["denom_index"], "denom_index")? };
    let denom_index = u8::try_from(denom_index).map_err(|_| "denom_index: above 255")?;

    let s = inbound::deposit_secret(&keys.auth, &chain_id, &vault, &depositor, j);
    let c = inbound::commitment(&s, kind, &dest);
    let (function, data) = vault_call(&token, denom_index, &c, j);
    let mut out = json!({
        "verdict": "ok",
        "commitment": hex(&c),
        "kind": kind_name(kind),
        "vault_call": {
            "to": hex(&vault),
            "from": hex(&depositor),
            "function": function,
            "data": hex(&data),
            "value": if token.iter().all(|b| *b == 0) {
                "the denomination plus the mint and relay fees (vault.fees(token, schedule, denomIndex))"
            } else {
                "0 (approve the vault for the denomination plus fees first)"
            },
        },
    });
    // With the leaf's place in the vault's tree (from the vault's `Deposit` event), what the
    // wallet keeps to claim it and to recognise the claim.
    if !req["leaf_index"].is_null() {
        let leaf_index = uint(&req["leaf_index"], "leaf_index")?;
        let amount = u256(&req["amount"], "amount")?;
        let leaf = inbound::deposit_leaf(leaf_index, &depositor, &token, &amount, &c);
        let fid = inbound::deposit_id(&chain_id, &vault, leaf_index, &leaf);
        out["leaf_index"] = json!(leaf_index);
        out["leaf"] = json!(hex(&leaf));
        out["deposit_id"] = json!(hex(&fid));
        out["nullifier"] = json!(hex(&inbound::nullifier(&s, &fid)));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// claim
// ---------------------------------------------------------------------------------------------

/// Borsh of the bridge program's `Claim` (or, `private`, `ClaimPrivately`) with this witness: the
/// program's own type (`BridgeMessage`).
fn claim_message(
    config_id: &Hash,
    leaf_count: u64,
    w: &ClaimWitness,
    range: [u64; 3],
    relayer: Option<Hash>,
    private: bool,
) -> Vec<u8> {
    let witness = bw::ClaimWitness {
        depositor: w.depositor,
        token: w.token,
        amount: w.amount,
        leaf_index: w.leaf_index,
        path: w.path.siblings.to_vec(),
        s: w.s,
        kind: w.kind as u8,
        dest: w.dest,
    };
    let range = bw::ScheduleRange { index: range[0], from_leaf: range[1], until_leaf: range[2] };
    let message = if private {
        BridgeMessage::ClaimPrivately { config_id: *config_id, leaf_count, witness, range, relayer }
    } else {
        BridgeMessage::Claim { config_id: *config_id, leaf_count, witness, range, relayer }
    };
    borsh::to_vec(&message).expect("borsh")
}

fn claim_of(req: &Value) -> Result<Value, String> {
    let keys = keys(req)?;
    let chain_id = chain(req)?;
    let vault: Address = fixed(&req["vault"], "vault")?;
    let depositor: Address = fixed(&req["depositor"], "depositor")?;
    let j = uint(&req["j"], "j")?;
    let leaf_index = uint(&req["leaf_index"], "leaf_index")?;
    let (path, built) = path_of(req, leaf_index)?;
    let root = root_of(req, "root", built)?;
    let w = ClaimWitness {
        depositor,
        token: fixed(&req["token"], "token")?,
        amount: u256(&req["amount"], "amount")?,
        leaf_index,
        path,
        s: inbound::deposit_secret(&keys.auth, &chain_id, &vault, &depositor, j),
        kind: kind(&req["kind"])?,
        dest: fixed(&req["dest"], "dest")?,
    };
    let c = inbound::commitment(&w.s, w.kind, &w.dest);
    let leaf = inbound::deposit_leaf(leaf_index, &w.depositor, &w.token, &w.amount, &c);
    let mut out = match inbound::verify_claim(&root, &w, &chain_id, &vault) {
        Ok(n) => json!({
            "verdict": "claimable",
            "nullifier": hex(&n),
            "root": hex(&root),
            "leaf": hex(&leaf),
            "commitment": hex(&c),
            "deposit_id": hex(&inbound::deposit_id(&chain_id, &vault, leaf_index, &leaf)),
            "kind": kind_name(w.kind),
            "path": path_json(&w.path),
        }),
        Err(e) => {
            return Ok(json!({
                "verdict": "refused",
                "error": verify_error(e),
                "root": hex(&root),
                "leaf": hex(&leaf),
            }))
        }
    };
    // The bridge program's claim, sent at `root(cid, n)`: plaintext `Claim` for a public destination,
    // `ClaimPrivately` (run in a private execution of the LEZ wallet) for a private one or with
    // `private`. With LEZ's state (`root_state`, `token_state`) the claim goes through the tool's code
    // instead (`wallet::claim_with_state`): the program's checks, the fees and the accounts.
    if !req["config_id"].is_null() {
        let private = req["private"].as_bool().unwrap_or(w.kind != DestKind::Public);
        if !private && w.kind != DestKind::Public {
            out["message_note"] = json!("a private destination is claimed with ClaimPrivately (`private`)");
        } else {
            let config_id: Hash = fixed(&req["config_id"], "config_id")?;
            let leaf_count = match (&req["leaf_count"], req["leaves"].as_array()) {
                (v, _) if !v.is_null() => uint(v, "leaf_count")?,
                (_, Some(ls)) => ls.len() as u64,
                _ => return Err("the request needs `leaf_count` (the imported root's leaf count) or `leaves`".into()),
            };
            let r = &req["range"];
            let range = if r.is_null() {
                [0, 0, u64::MAX]
            } else {
                [uint(&r["index"], "range.index")?, uint(&r["from_leaf"], "range.from_leaf")?, uint(&r["until_leaf"], "range.until_leaf")?]
            };
            let relayer = if req["relayer"].is_null() { None } else { Some(fixed::<32>(&req["relayer"], "relayer")?) };
            let m = claim_message(&config_id, leaf_count, &w, range, relayer, private);
            out["variant"] = json!(if private { "ClaimPrivately" } else { "Claim" });
            out["message"] = json!(hex(&m));
            out["message_bytes"] = json!(m.len());
            out["leaf_count"] = json!(leaf_count);
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// burn
// ---------------------------------------------------------------------------------------------

/// Borsh of the bridge program's `Burn` of note `n` (`BridgeMessage::burn`, the program's own type).
fn burn_message(config_id: &Hash, n: &BurnNote) -> Vec<u8> {
    let note = bw::Note { r: n.r, memo: n.memo, mac: n.mac, token: n.token, amount: n.amount, c_out: n.c_out };
    borsh::to_vec(&BridgeMessage::burn(*config_id, &note)).expect("borsh")
}

fn burn_of(req: &Value) -> Result<Value, String> {
    let keys = keys(req)?;
    let config_id: Hash = fixed(&req["config_id"], "config_id")?;
    let i = uint(&req["i"], "i")?;
    let target = ReleaseTarget {
        chain_id: chain(req)?,
        vault: fixed(&req["vault"], "vault")?,
        recipient: fixed(&req["recipient"], "recipient")?,
    };
    let note = BurnNote::create(&keys, &config_id, i, &target, fixed(&req["token"], "token")?, u256(&req["amount"], "amount")?);
    let m = burn_message(&config_id, &note);
    let mut out = json!({
        "verdict": "ok",
        "i": i,
        "note": note_json(&note),
        "message": hex(&m),
        "message_bytes": m.len(),
    });
    // With the burn's index in the burn tree (from the program's burn record), the leaf, the burn
    // id and the nullifier the vault will spend.
    if !req["burn_index"].is_null() {
        let burn_index = uint(&req["burn_index"], "burn_index")?;
        let leaf = outbound::burn_leaf(&config_id, burn_index, &note.token, &note.amount, &note.c_out, &note.r);
        let s_out = outbound::burn_secret(&keys.auth, &config_id, &note.r);
        out["burn_index"] = json!(burn_index);
        out["burn_leaf"] = json!(hex(&leaf));
        out["burn_id"] = json!(hex(&outbound::burn_id(&config_id, burn_index, &leaf)));
        out["nullifier"] = json!(hex(&outbound::burn_nullifier(&s_out)));
    }
    Ok(out)
}

pub(crate) fn note_json(n: &BurnNote) -> Value {
    json!({
        "r": hex(&n.r),
        "memo": hex(&n.memo),
        "mac": hex(&n.mac),
        "token": hex(&n.token),
        "amount": hex(&n.amount),
        "c_out": hex(&n.c_out),
    })
}

pub(crate) fn note_of(v: &Value, what: &str) -> Result<BurnNote, String> {
    Ok(BurnNote {
        r: fixed(&v["r"], &format!("{what}.r"))?,
        memo: fixed(&v["memo"], &format!("{what}.memo"))?,
        mac: fixed::<16>(&v["mac"], &format!("{what}.mac"))?,
        token: fixed(&v["token"], &format!("{what}.token"))?,
        amount: u256(&v["amount"], &format!("{what}.amount"))?,
        c_out: fixed(&v["c_out"], &format!("{what}.c_out"))?,
    })
}

// ---------------------------------------------------------------------------------------------
// release
// ---------------------------------------------------------------------------------------------

/// `release` with the burn root given (`burn_root`, with `path` or `leaves`): the reply, and for a
/// releasable burn the release guest's input (encoded) and the journal; or a refusal reply.
#[allow(clippy::type_complexity)]
pub(crate) fn release_explicit(req: &Value) -> Result<Result<(Value, Vec<u8>, Vec<u8>), Value>, String> {
    let keys = keys(req)?;
    let config_id: Hash = fixed(&req["config_id"], "config_id")?;
    let i = uint(&req["i"], "i")?;
    let burn_index = uint(&req["burn_index"], "burn_index")?;
    let (path, built) = path_of(req, burn_index)?;
    let burn_root = root_of(req, "burn_root", built)?;
    let r = outbound::burn_tag(&keys.view, &config_id, i);
    let s_out = outbound::burn_secret(&keys.auth, &config_id, &r);
    let p = ReleasePublic {
        chain_id: chain(req)?,
        vault: fixed(&req["vault"], "vault")?,
        config_id,
        token: fixed(&req["token"], "token")?,
        amount: u256(&req["amount"], "amount")?,
        recipient: fixed(&req["recipient"], "recipient")?,
        relayer: if req["relayer"].is_null() { [0; 20] } else { fixed(&req["relayer"], "relayer")? },
        relay_fee: if req["relay_fee"].is_null() { [0; 32] } else { u256(&req["relay_fee"], "relay_fee")? },
    };
    let w = ReleaseWitness { burn_index, path, s_out, r };
    Ok(match outbound::verify_release(&burn_root, &w, &p) {
        Ok(n) => {
            let journal = outbound::release_journal(&burn_root, &n, &p);
            let input = ReleaseInput { burn_root, witness: w.clone(), public: p.clone() }.encode().to_vec();
            Ok((json!({
                "verdict": "releasable",
                "nullifier": hex(&n),
                "burn_root": hex(&burn_root),
                "r": hex(&r),
                "journal": hex(&journal),
                "journal_sha256": hex(&sha256(&journal)),
                "path": path_json(&w.path),
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
            }), input, journal.to_vec()))
        }
        Err(e) => Err(json!({ "verdict": "refused", "error": verify_error(e), "burn_root": hex(&burn_root), "r": hex(&r) })),
    })
}

// ---------------------------------------------------------------------------------------------
// recover
// ---------------------------------------------------------------------------------------------

fn recover_of(req: &Value) -> Result<Value, String> {
    let keys = keys(req)?;
    let chain_id = chain(req)?;
    let vault: Address = fixed(&req["vault"], "vault")?;
    let gap = if req["gap"].is_null() { 8 } else { uint(&req["gap"], "gap")? };
    let gap = u32::try_from(gap).map_err(|_| "gap: above 2^32")?;

    // The wallet's candidate destinations: (kind, account index) -> account id.
    let mut dests: Vec<(DestKind, u32, Hash)> = Vec::new();
    for (i, d) in req["destinations"].as_array().into_iter().flatten().enumerate() {
        let index = u32::try_from(uint(&d["index"], &format!("destinations[{i}].index"))?).map_err(|_| "index above 2^32")?;
        dests.push((kind(&d["kind"])?, index, fixed(&d["dest"], &format!("destinations[{i}].dest"))?));
    }
    let mut deposits = Vec::new();
    for (i, d) in req["deposits"].as_array().into_iter().flatten().enumerate() {
        let at = |k: &str| format!("deposits[{i}].{k}");
        deposits.push(VaultDeposit {
            depositor: fixed(&d["depositor"], &at("depositor"))?,
            j: uint(&d["j"], &at("j"))?,
            leaf_index: uint(&d["leaf_index"], &at("leaf_index"))?,
            token: fixed(&d["token"], &at("token"))?,
            amount: u256(&d["amount"], &at("amount"))?,
            commitment: fixed(&d["commitment"], &at("commitment"))?,
        });
    }
    let owned = inbound::recover(&keys.auth, &chain_id, &vault, &deposits, gap, |k, a| {
        dests.iter().find(|(dk, di, _)| *dk == k && *di == a).map(|(_, _, d)| *d).unwrap_or([0xff; 32])
    });
    let mut out = json!({
        "verdict": "ok",
        "deposits_scanned": deposits.len(),
        "deposits": owned.iter().map(|o| json!({
            "leaf_index": o.deposit.leaf_index,
            "j": o.deposit.j,
            "depositor": hex(&o.deposit.depositor),
            "token": hex(&o.deposit.token),
            "amount": hex(&o.deposit.amount),
            "kind": kind_name(o.kind),
            "account_index": o.account_index,
            "dest": hex(&o.dest),
            "leaf": hex(&o.leaf),
            "nullifier": hex(&o.nullifier),
        })).collect::<Vec<_>>(),
    });
    if let Some(burns) = req["burns"].as_array() {
        let config_id: Hash = fixed(&req["config_id"], "config_id")?;
        let gap_limit = if req["gap_limit"].is_null() { 20 } else { uint(&req["gap_limit"], "gap_limit")? };
        let mut notes = Vec::with_capacity(burns.len());
        for (i, b) in burns.iter().enumerate() {
            notes.push(PublishedBurn {
                burn_index: uint(&b["burn_index"], &format!("burns[{i}].burn_index"))?,
                note: note_of(b, &format!("burns[{i}]"))?,
            });
        }
        let scan = outbound::recover_burns(&keys, &config_id, &chain_id, &vault, &notes, gap_limit);
        out["burns_scanned"] = json!(notes.len());
        out["next_i"] = json!(scan.next_i);
        out["burns"] = json!(scan
            .owned
            .iter()
            .map(|o| json!({
                "i": o.i,
                "burn_index": o.burn.burn_index,
                "recipient": hex(&o.recipient),
                "token": hex(&o.burn.note.token),
                "amount": hex(&o.burn.note.amount),
                "leaf": hex(&o.leaf),
                "nullifier": hex(&o.nullifier),
            }))
            .collect::<Vec<_>>());
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// status
// ---------------------------------------------------------------------------------------------

fn hash_set(v: &Value, what: &str) -> Result<Vec<Hash>, String> {
    let mut out = Vec::new();
    for (i, h) in v.as_array().into_iter().flatten().enumerate() {
        out.push(fixed::<32>(h, &format!("{what}[{i}]"))?);
    }
    Ok(out)
}

/// Deposits: `claimed` once their nullifier is spent on LEZ, else `claimable` once a root imported
/// on LEZ covers their leaf, else `awaiting_root`. Burns: `released` once the vault has spent their
/// nullifier, else `releasable` once an accepted checkpoint covers their index, else
/// `awaiting_checkpoint`.
fn status_of(req: &Value) -> Result<Value, String> {
    let imported = if req["imported_leaf_count"].is_null() { 0 } else { uint(&req["imported_leaf_count"], "imported_leaf_count")? };
    let checkpointed =
        if req["checkpointed_burn_count"].is_null() { 0 } else { uint(&req["checkpointed_burn_count"], "checkpointed_burn_count")? };
    let spent = hash_set(&req["spent_nullifiers"], "spent_nullifiers")?;
    let released = hash_set(&req["released_nullifiers"], "released_nullifiers")?;
    let mut deposits = Vec::new();
    for (i, d) in req["deposits"].as_array().into_iter().flatten().enumerate() {
        let n: Hash = fixed(&d["nullifier"], &format!("deposits[{i}].nullifier"))?;
        let leaf_index = uint(&d["leaf_index"], &format!("deposits[{i}].leaf_index"))?;
        let state = if spent.contains(&n) {
            "claimed"
        } else if leaf_index < imported {
            "claimable"
        } else {
            "awaiting_root"
        };
        deposits.push(json!({ "leaf_index": leaf_index, "nullifier": hex(&n), "status": state }));
    }
    let mut burns = Vec::new();
    for (i, b) in req["burns"].as_array().into_iter().flatten().enumerate() {
        let n: Hash = fixed(&b["nullifier"], &format!("burns[{i}].nullifier"))?;
        let burn_index = uint(&b["burn_index"], &format!("burns[{i}].burn_index"))?;
        let state = if released.contains(&n) {
            "released"
        } else if burn_index < checkpointed {
            "releasable"
        } else {
            "awaiting_checkpoint"
        };
        burns.push(json!({ "burn_index": burn_index, "nullifier": hex(&n), "status": state }));
    }
    Ok(json!({
        "verdict": "ok",
        "imported_leaf_count": imported,
        "checkpointed_burn_count": checkpointed,
        "deposits": deposits,
        "burns": burns,
    }))
}

// ---------------------------------------------------------------------------------------------
// JSON helpers
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

pub(crate) fn text<'a>(v: &'a Value, what: &str) -> Result<&'a str, String> {
    match v {
        Value::String(s) => Ok(s),
        Value::Null => Err(format!("{what}: missing")),
        _ => Err(format!("{what}: expected a string")),
    }
}

pub(crate) fn bytes(v: &Value, what: &str) -> Result<Vec<u8>, String> {
    unhex(text(v, what)?).map_err(|e| format!("{what}: {e}"))
}

/// A byte string of exactly `N` bytes, in hex.
pub(crate) fn fixed<const N: usize>(v: &Value, what: &str) -> Result<[u8; N], String> {
    let b = bytes(v, what)?;
    let len = b.len();
    b.try_into().map_err(|_| format!("{what}: expected {N} bytes, got {len}"))
}

/// An unsigned 64-bit integer: a JSON number, a decimal string or a `0x` quantity.
pub(crate) fn uint(v: &Value, what: &str) -> Result<u64, String> {
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

/// A `uint256` as a 32-byte big-endian word: `0x` hex of at most 32 bytes (left-padded), a decimal
/// string, or a JSON number.
pub(crate) fn u256(v: &Value, what: &str) -> Result<U256, String> {
    let mut w = [0u8; 32];
    match v {
        Value::Number(_) => w[24..].copy_from_slice(&uint(v, what)?.to_be_bytes()),
        Value::String(s) if s.starts_with("0x") || s.starts_with("0X") => {
            let h = strip0x(s);
            let padded = if h.len() % 2 == 1 { format!("0{h}") } else { h.to_string() };
            let b = unhex(&padded).map_err(|e| format!("{what}: {e}"))?;
            if b.len() > 32 {
                return Err(format!("{what}: more than 32 bytes"));
            }
            w[32 - b.len()..].copy_from_slice(&b);
        }
        Value::String(s) => {
            if s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit()) {
                return Err(format!("{what}: not a decimal integer: {s}"));
            }
            for c in s.bytes() {
                // w = w * 10 + digit, big-endian, refusing overflow.
                let mut carry = u32::from(c - b'0');
                for byte in w.iter_mut().rev() {
                    let x = u32::from(*byte) * 10 + carry;
                    *byte = (x & 0xff) as u8;
                    carry = x >> 8;
                }
                if carry != 0 {
                    return Err(format!("{what}: above 2^256"));
                }
            }
        }
        Value::Null => return Err(format!("{what}: missing")),
        _ => return Err(format!("{what}: expected a string or a number")),
    }
    Ok(w)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts() {
        let a = u256(&json!("1000000000000000000"), "a").unwrap();
        assert_eq!(hex(&a[24..]), "0x0de0b6b3a7640000");
        assert_eq!(u256(&json!("0x01"), "a").unwrap()[31], 1);
        assert_eq!(u256(&json!(258), "a").unwrap()[30..], [1, 2]);
        assert!(u256(&json!("1e18"), "a").is_err());
        let max = "115792089237316195423570985008687907853269984665640564039457584007913129639935";
        assert_eq!(u256(&json!(max), "a").unwrap(), [0xff; 32]);
        assert!(u256(&json!("115792089237316195423570985008687907853269984665640564039457584007913129639936"), "a").is_err());
    }

    #[test]
    fn selectors() {
        // keccak256("transfer(address,uint256)") starts a9059cbb.
        assert_eq!(selector("transfer(address,uint256)"), [0xa9, 0x05, 0x9c, 0xbb]);
        let (f, data) = vault_call(&[0; 20], 1, &[7; 32], 3);
        assert_eq!((f.as_str(), data.len()), ("depositETH(uint8,bytes32,uint64)", 4 + 3 * 32));
        let (_, data) = vault_call(&[9; 20], 1, &[7; 32], 3);
        assert_eq!(data.len(), 4 + 4 * 32);
    }

    #[test]
    fn deposit_claim_status_round_trip() {
        // One wallet deposits twice; the claim of the second against the two-leaf tree verifies,
        // recovery finds both and only them, and status follows the chain's state.
        let seed = hex(&[0x11; 32]);
        let base = json!({ "bridge_seed": seed, "chain_id": 11155111, "vault": hex(&[0xaa; 20]),
                           "depositor": hex(&[0xbb; 20]), "token": hex(&[0; 20]), "amount": "1000",
                           "kind": "public", "dest": hex(&[0xcc; 32]) });
        let mut leaves = Vec::new();
        let mut owned = Vec::new();
        for j in 0..2u64 {
            let mut r = base.clone();
            r["j"] = json!(j);
            r["leaf_index"] = json!(j);
            let out: Value = serde_json::from_str(&deposit(&r.to_string())).unwrap();
            assert_eq!(out["verdict"], "ok", "{out}");
            leaves.push(out["leaf"].clone());
            owned.push((out["commitment"].clone(), out["nullifier"].clone()));
        }
        let mut c = base.clone();
        c["j"] = json!(1);
        c["leaf_index"] = json!(1);
        c["leaves"] = json!(leaves);
        c["config_id"] = json!(hex(&[0x01; 32]));
        let out: Value = serde_json::from_str(&claim(&c.to_string())).unwrap();
        assert_eq!(out["verdict"], "claimable", "{out}");
        assert_eq!(out["nullifier"], owned[1].1);
        assert_eq!(out["message_bytes"], 1 + 32 + 8 + 20 + 20 + 32 + 8 + 4 + 32 * 32 + 32 + 1 + 32 + 24 + 1);
        // A wrong amount gives another leaf: refused.
        c["amount"] = json!("999");
        c.as_object_mut().unwrap().remove("leaves");
        c["path"] = out["path"].clone();
        c["root"] = out["root"].clone();
        let bad: Value = serde_json::from_str(&claim(&c.to_string())).unwrap();
        assert_eq!(bad["verdict"], "refused");
        assert_eq!(bad["error"], "RootMismatch");

        let deposits: Vec<Value> = (0..2)
            .map(|j| json!({ "depositor": hex(&[0xbb; 20]), "j": j, "leaf_index": j, "token": hex(&[0; 20]),
                              "amount": "1000", "commitment": owned[j as usize].0 }))
            .chain(std::iter::once(json!({ "depositor": hex(&[0xbb; 20]), "j": 2, "leaf_index": 2,
                              "token": hex(&[0; 20]), "amount": "1000", "commitment": hex(&[0x77; 32]) })))
            .collect();
        let r = json!({ "bridge_seed": seed, "chain_id": 11155111, "vault": hex(&[0xaa; 20]),
                        "destinations": [{ "kind": 2, "index": 0, "dest": hex(&[0xcc; 32]) }],
                        "deposits": deposits });
        let found: Value = serde_json::from_str(&recover(&r.to_string())).unwrap();
        assert_eq!(found["deposits"].as_array().unwrap().len(), 2, "{found}");

        let s = json!({ "imported_leaf_count": 1, "spent_nullifiers": [owned[0].1],
                        "deposits": [{ "leaf_index": 0, "nullifier": owned[0].1 },
                                     { "leaf_index": 1, "nullifier": owned[1].1 }] });
        let st: Value = serde_json::from_str(&status(&s.to_string())).unwrap();
        assert_eq!(st["deposits"][0]["status"], "claimed");
        assert_eq!(st["deposits"][1]["status"], "awaiting_root");
    }

    #[test]
    fn burn_release_round_trip() {
        let seed = hex(&[0x22; 32]);
        let cid = hex(&[0x0c; 32]);
        let common = json!({ "bridge_seed": seed, "config_id": cid, "chain_id": 11155111,
                             "vault": hex(&[0xaa; 20]), "recipient": hex(&[0xdd; 20]),
                             "token": hex(&[0; 20]), "amount": "5000" });
        let mut leaves = vec![json!(hex(&[0x99; 32]))];
        let mut b = common.clone();
        b["i"] = json!(0);
        b["burn_index"] = json!(1);
        let out: Value = serde_json::from_str(&burn(&b.to_string())).unwrap();
        assert_eq!(out["message_bytes"], 1 + 32 + 32 + 20 + 16 + 20 + 32 + 32);
        leaves.push(out["burn_leaf"].clone());
        let mut r = common.clone();
        r["i"] = json!(0);
        r["burn_index"] = json!(1);
        r["leaves"] = json!(leaves);
        let rel: Value = serde_json::from_str(&release(&r.to_string())).unwrap();
        assert_eq!(rel["verdict"], "releasable", "{rel}");
        assert_eq!(rel["nullifier"], out["nullifier"]);
        assert_eq!(rel["journal"].as_str().unwrap().len(), 2 + 2 * 352);
        // Another recipient is another commitment: refused.
        r["recipient"] = json!(hex(&[0xde; 20]));
        let bad: Value = serde_json::from_str(&release(&r.to_string())).unwrap();
        assert_eq!(bad["verdict"], "refused");
        // Recovery from the published note.
        let rec = json!({ "bridge_seed": seed, "config_id": cid, "chain_id": 11155111, "vault": hex(&[0xaa; 20]),
                          "burns": [{ "burn_index": 1, "r": out["note"]["r"], "memo": out["note"]["memo"],
                                      "mac": out["note"]["mac"], "token": out["note"]["token"],
                                      "amount": out["note"]["amount"], "c_out": out["note"]["c_out"] }] });
        let found: Value = serde_json::from_str(&recover(&rec.to_string())).unwrap();
        assert_eq!(found["burns"][0]["recipient"], json!(hex(&[0xdd; 20])), "{found}");
        assert_eq!(found["burns"][0]["nullifier"], out["nullifier"]);
    }

    #[test]
    fn unreadable_requests() {
        for f in [deposit, claim, burn, release, recover, status] {
            let out: Value = serde_json::from_str(&f("not json")).unwrap();
            assert_eq!(out["verdict"], "invalid_request");
        }
        let out: Value = serde_json::from_str(&deposit("{}")).unwrap();
        assert_eq!(out["error"], "the request needs `bridge_seed` or `lez_seed`");
    }
}
