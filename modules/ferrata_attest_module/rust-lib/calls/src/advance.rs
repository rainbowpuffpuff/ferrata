//! `advance`: one light-client update planned the way `ferrata-updater` plans it, with the updater's
//! own code (`ferrata-advance`, which the updater links too): the program's checks run natively
//! (`ferrata_core::steps`, BLS by blst), the job the update opens, and the transactions of its steps,
//! `Open` at `instance(id)`, then `Miller`, the committee's `Adopt` parts and `Finish` at
//! `job(id, job_id)`, each with the program's own message type (`ferrata_lez_wire::Message`).
//!
//! The instance's state comes in the request as LEZ holds it at `instance(id)`; the update as a beacon
//! node serves it (SSZ: an `updates` entry, or a finality update); the full sync committees the store
//! summarises as a bootstrap (its current committee) or as lists of keys. Nothing is signed or sent.

use ferrata_advance::{self as adv, Step, Verdict};
use ferrata_core::{
    types::{LightClientBootstrap, SyncCommittee},
    witness::{CommitteeSummary, CompactStore},
};
use ferrata_lez_wire::InstanceState;
use lee_core::account::{AccountId, Actor};
use serde_json::{json, Value};

use crate::{fixed, hex, text, unhex};

fn bytes(v: &Value, what: &str) -> Result<Vec<u8>, String> {
    unhex(text(v, what)?).map_err(|e| format!("{what}: {e}"))
}

fn actor_json(a: &Actor) -> Value {
    json!({ "account": hex(a.account_id.value()), "program": hex(a.program_account_id.value()) })
}

/// The committees the request gives: `bootstrap` (its current committee) and `committees` (each the
/// keys then the aggregate key, 48 bytes each, as the updater keeps them).
fn committees(req: &Value, n: usize) -> Result<Vec<SyncCommittee>, String> {
    let mut out = Vec::new();
    if !req["bootstrap"].is_null() {
        let b = bytes(&req["bootstrap"], "bootstrap")?;
        let layouts = [ferrata_core::fork::LcFork::Electra, ferrata_core::fork::LcFork::Gloas];
        let boot = layouts
            .iter()
            .find_map(|l| LightClientBootstrap::decode(&b, *l, n).ok())
            .ok_or("bootstrap: not a LightClientBootstrap (Electra or Gloas layout)")?;
        out.push(boot.current_sync_committee);
    }
    if let Some(list) = req["committees"].as_array() {
        for (i, c) in list.iter().enumerate() {
            let b = bytes(c, &format!("committees[{i}]"))?;
            out.push(adv::decode_committee(&b).map_err(|e| format!("committees[{i}]: {e}"))?);
        }
    }
    Ok(out)
}

pub(crate) fn advance_of(request: &str) -> Result<Value, String> {
    let req: Value = serde_json::from_str(request).map_err(|e| format!("request is not JSON: {e}"))?;
    let program = AccountId::new(fixed::<32>(&req["program_id"], "program_id")?);
    let state: InstanceState = borsh::from_slice(&bytes(&req["instance_state"], "instance_state")?)
        .map_err(|e| format!("instance_state: not an InstanceState: {e}"))?;
    let config = &state.config;
    if !state.active {
        return Ok(json!({
            "verdict": "inactive",
            "error": "the instance's committee is not adopted yet (its initialization job is open)",
            "instance_id": hex(&state.id),
        }));
    }
    let store = CompactStore::decode(&state.store).map_err(|e| format!("instance_state: its store does not decode: {e:?}"))?;
    let (update, served) = match (&req["update"], &req["finality_update"]) {
        (u, _) if !u.is_null() => adv::decode_update(&bytes(u, "update")?, config).map_err(|e| format!("update: {e}"))?,
        (_, f) if !f.is_null() => {
            adv::decode_finality_update(&bytes(f, "finality_update")?, config).map_err(|e| format!("finality_update: {e}"))?
        }
        _ => return Err("the request needs `update` or `finality_update` (SSZ, hex)".into()),
    };
    // The full committees, accepted only where they match the summaries the store holds.
    let known = committees(&req, config.sync_committee_size as usize)?;
    let matches = |root: &[u8; 32]| known.iter().find(|c| CommitteeSummary::of(c).pubkeys_root == *root);
    let current = matches(&store.current.pubkeys_root);
    let next = store.next.as_ref().and_then(|s| matches(&s.pubkeys_root));
    let base = json!({
        "instance_id": hex(&state.id),
        "store_finalized_slot": store.finalized.slot,
        "update": {
            "served_layout": format!("{served:?}"),
            "attested_slot": update.attested_header.beacon.slot,
            "finalized_slot": update.finalized_header.beacon.slot,
            "signature_slot": update.signature_slot,
            "signers": update.sync_aggregate.bits.iter().filter(|b| **b).count(),
            "carries_next_committee": !update.next_sync_committee.is_empty(),
        },
        "committees": { "current_known": current.is_some(), "next_known": next.is_some() },
    });
    let mut out = base;
    let planned = match adv::evaluate(config, &state.id, &store, &update, current, next) {
        Verdict::Useful(p) => p,
        Verdict::NoChange => {
            out["verdict"] = json!("no_change");
            out["detail"] = json!("valid or not, it would not move the instance's store");
            return Ok(out);
        }
        Verdict::Unverifiable => {
            out["verdict"] = json!("unverifiable");
            out["detail"] = json!("the committee that signs it is not known: give it (`bootstrap` or `committees`)");
            return Ok(out);
        }
        Verdict::Invalid(reason) => {
            out["verdict"] = json!("invalid");
            out["detail"] = json!(reason);
            return Ok(out);
        }
    };
    // The steps still to send: all of a fresh job, or from the job's state on the chain.
    let steps: Vec<Step> = if req["job_state"].is_null() {
        adv::job_steps(config, &planned)
    } else {
        let job = adv::job_of_state(&bytes(&req["job_state"], "job_state")?).map_err(|e| format!("job_state: {e}"))?;
        let first = adv::next_step(job.as_ref());
        adv::job_steps(config, &planned).into_iter().filter(|s| *s >= first).collect()
    };
    let mut txs = Vec::new();
    for step in &steps {
        let t = adv::job_step(program, &state.id, config, &planned, *step).map_err(|e| format!("{}: {e}", step.name()))?;
        txs.push(json!({
            "step": step.name(),
            "root": actor_json(&t.root),
            "accounts": t.actors.iter().map(actor_json).collect::<Vec<_>>(),
            "message": hex(&t.message),
            "message_bytes": t.message.len(),
        }));
    }
    out["verdict"] = json!("useful");
    out["job_id"] = json!(hex(&planned.jid));
    out["layout"] = json!(format!("{:?}", planned.layout));
    out["outcome"] = json!({
        "advanced": planned.outcome.advanced,
        "adopted": planned.outcome.adopted,
        "rotated": planned.outcome.rotated,
        "finalized_slot": planned.update.finalized_header.beacon.slot,
    });
    out["steps"] = Value::Array(txs);
    Ok(out)
}
