//! The LEZ side of the Logos modules' end-to-end test (`modules/scripts/e2e.sh`): the transactions
//! `ferrata_attest_module` and `ferrata_bridge_module` build, executed or decoded with the
//! programs' own types. The chain is Stage 4's (`harness::attest::Stage4`): the mainnet instance
//! initialized from the attestation fixture's bootstrap, its finality update verified through the
//! public job, all inside LEZ v0.4.0-rc1's state machine and fee settlement.
//!
//! ```text
//! module_e2e info                          what a client needs: program, instance, slot, the record,
//!                                          the instance's state
//! module_e2e attest MESSAGE_HEX ACCOUNT    decode the module's Attest message with the program's
//!                                          types, submit it at ACCOUNT, report the outcome
//! module_e2e bridge MESSAGE_HEX            decode a bridge module message with the bridge
//!                                          program's types (BridgeMessage)
//! module_e2e advance-info                  the same instance before its finality update: its state,
//!                                          the update and the bootstrap (SSZ), for `advance`
//! module_e2e advance REPLY_FILE            execute the steps of `advance`'s reply in order on that
//!                                          chain; compare with the harness's own job for the update
//! ```
//! Each prints one JSON object. MESSAGE_HEX and ACCOUNT are hex, as the modules return them.

use std::process::exit;

use ferrata_lez_bridge_wire::BridgeMessage;
use ferrata_lez_wire::{header_actor, instance_actor, Message, ATTESTED_SELECTOR};
use harness::{
    attest::{fixture_file, Fixture, Stage4},
    mainnet_config,
    scenarios::World,
};
use lee::{AccountId, Actor};
use serde_json::{json, Value};

fn unhex(s: &str) -> Vec<u8> {
    hex::decode(s.trim().trim_start_matches("0x")).unwrap_or_else(|e| {
        eprintln!("module_e2e: not hex: {e}");
        exit(2)
    })
}

fn h(b: &[u8]) -> String {
    format!("0x{}", hex::encode(b))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = match args.first().map(String::as_str) {
        Some("info") => info(),
        Some("attest") if args.len() == 3 => attest(&args[1], &args[2]),
        Some("bridge") if args.len() == 2 => bridge(&args[1]),
        Some("advance-info") => advance_info(),
        Some("advance") if args.len() == 2 => advance(&args[1]),
        _ => {
            eprintln!(
                "usage: module_e2e info | attest MESSAGE_HEX ACCOUNT_HEX | bridge MESSAGE_HEX | advance-info | advance REPLY_FILE"
            );
            exit(2)
        }
    };
    println!("{out}");
}

fn info() -> Value {
    let s = Stage4::new();
    let slot = s.fx.finalized_slot;
    let record = s.record(slot);
    json!({
        "program_id": h(s.w.inst.program.value()),
        "instance_id": h(&s.w.inst.id),
        "chain_id": s.fx.chain_id,
        "checkpoint": h(&s.w.inst.checkpoint),
        "finalized_slot": slot,
        "block": s.fx.block,
        "header_account": h(s.header(slot).account_id.value()),
        // The record as the program stores it at header(id, slot): what a client reads from LEZ.
        "record": { "chain_id": record.chain_id, "encoded": h(&record.record) },
        "record_instance_id": h(&record.instance_id),
        // The instance's state as the program stores it at instance(id) (`InstanceState`, Borsh).
        "instance_state": h(&s.w.chain.actor_state(instance_actor(s.w.inst.program, &s.w.inst.id))),
    })
}

fn attest(message_hex: &str, account_hex: &str) -> Value {
    let bytes = unhex(message_hex);
    let account = unhex(account_hex);
    let mut s = Stage4::new();
    let slot = s.fx.finalized_slot;
    let header = s.header(slot);
    // The program's own decoding of the module's bytes, and its canonical re-encoding.
    let request = match borsh::from_slice::<Message>(&bytes) {
        Ok(Message::Attest { request }) => request,
        Ok(other) => return json!({ "decoded": false, "error": format!("not Attest: {other:?}") }),
        Err(e) => return json!({ "decoded": false, "error": e.to_string() }),
    };
    let canonical = borsh::to_vec(&Message::Attest { request: request.clone() }).unwrap() == bytes;
    let account_matches = account.as_slice() == header.account_id.value().as_slice();
    let expected = s.expected(slot, &request);
    let report = s.w.chain.submit("module attest", header, bytes.clone(), vec![header], vec![], s.time);
    let event = report.events.iter().find(|e| e.event.selector == ATTESTED_SELECTOR);
    json!({
        "decoded": true,
        "canonical": canonical,
        "account_matches": account_matches,
        "facts": request.logs.len() + request.accounts.iter().map(|a| a.slots.len()).sum::<usize>(),
        "applied": report.ok(),
        "exit_code": report.exit_code,
        "revert": report.charge.revert.as_ref().map(|r| format!("{r:?}")),
        "gas_exec": report.charge.gas_exec,
        "gas_stor": report.charge.gas_stor,
        "cycles": report.charge.cycles,
        "tx_bytes": report.tx_bytes,
        "message_bytes": report.message_bytes,
        "event_emitter_is_program": event.map(|e| e.account_id == s.w.inst.program),
        "event_data": event.map(|e| h(&e.event.data)),
        "native": match &expected {
            Ok(a) => json!({
                "verdict": "attested",
                "event_data": h(&a.event_data()),
                "ids": a.statements.iter().map(|st| h(&st.id)).collect::<Vec<_>>(),
            }),
            Err(code) => json!({ "verdict": "refused", "code": code }),
        },
        "event_matches_native": match (&expected, event) {
            (Ok(a), Some(e)) => e.event.data == a.event_data(),
            _ => false,
        },
    })
}

fn bridge(message_hex: &str) -> Value {
    let bytes = unhex(message_hex);
    let message = match borsh::from_slice::<BridgeMessage>(&bytes) {
        Ok(m) => m,
        Err(e) => return json!({ "decoded": false, "error": e.to_string() }),
    };
    let canonical = borsh::to_vec(&message).unwrap() == bytes;
    let fields = match &message {
        BridgeMessage::Burn { config_id, r, memo, mac, token, amount, c_out } => json!({
            "variant": "Burn",
            "config_id": h(config_id), "r": h(r), "memo": h(memo), "mac": h(mac),
            "token": h(token), "amount": h(amount), "c_out": h(c_out),
        }),
        BridgeMessage::Claim { config_id, leaf_count, witness, range, relayer } => json!({
            "variant": "Claim",
            "config_id": h(config_id),
            "leaf_count": leaf_count,
            "depositor": h(&witness.depositor),
            "token": h(&witness.token),
            "amount": h(&witness.amount),
            "leaf_index": witness.leaf_index,
            "path_len": witness.path.len(),
            "kind": witness.kind,
            "dest": h(&witness.dest),
            "range": [range.index, range.from_leaf, range.until_leaf],
            "relayer": relayer.map(|r| h(&r)),
            "range_covers_leaf": range.covers(witness.leaf_index).is_ok(),
        }),
        other => {
            json!({ "variant": format!("{other:?}").split([' ', '{', '(']).next().unwrap_or("?").to_string() })
        }
    };
    json!({ "decoded": true, "canonical": canonical, "message_bytes": bytes.len(), "fields": fields })
}

/// Stage 4's instance before its finality update: initialized from the fixture's bootstrap, the
/// committee adopted, no update applied.
fn advance_world() -> (World, Fixture) {
    let fx = Fixture::load();
    let w = World::at(
        harness::ferrata_account(),
        mainnet_config(2),
        fx.checkpoint,
        fx.bootstrap.clone(),
        vec![fx.update.clone()],
    );
    (w, fx)
}

fn advance_info() -> Value {
    let (w, fx) = advance_world();
    json!({
        "program_id": h(w.inst.program.value()),
        "instance_id": h(&w.inst.id),
        "instance_state": h(&w.chain.actor_state(instance_actor(w.inst.program, &w.inst.id))),
        // As a beacon node serves them (the fixture's recorded files).
        "finality_update": h(&fixture_file("finality_update.ssz")),
        "bootstrap": h(&fixture_file("bootstrap.ssz")),
        "finalized_slot_before": w.inst.store(&w.chain).finalized.slot,
        "expected_finalized_slot": fx.finalized_slot,
    })
}

fn actor_of(v: &Value) -> Actor {
    let id = |k: &str| AccountId::new(unhex(v[k].as_str().unwrap_or_default()).try_into().unwrap_or([0; 32]));
    Actor::new(id("account"), id("program"))
}

/// The module's steps, executed in order on the chain before the update; then the same update run
/// by the harness's own job on a second copy of that chain, and the two instances compared.
fn advance(reply_file: &str) -> Value {
    let reply: Value = serde_json::from_slice(&std::fs::read(reply_file).unwrap_or_else(|e| {
        eprintln!("module_e2e: {reply_file}: {e}");
        exit(2)
    }))
    .unwrap_or(Value::Null);
    let (mut w, fx) = advance_world();
    let program = w.inst.program;
    let time = w.inst.config.slot_start_ms(fx.update.signature_slot) + 12_000;
    let mut steps = Vec::new();
    let mut all_applied = true;
    for step in reply["steps"].as_array().cloned().unwrap_or_default() {
        let bytes = unhex(step["message"].as_str().unwrap_or_default());
        let decoded = borsh::from_slice::<Message>(&bytes);
        let canonical = decoded.as_ref().map(|m| borsh::to_vec(m).unwrap() == bytes).unwrap_or(false);
        let variant = match &decoded {
            Ok(m) => format!("{m:?}").split([' ', '{', '(']).next().unwrap_or("?").to_string(),
            Err(e) => format!("undecodable: {e}"),
        };
        let root = actor_of(&step["root"]);
        let actors: Vec<Actor> =
            step["accounts"].as_array().map(|a| a.iter().map(actor_of).collect()).unwrap_or_default();
        let label = format!("module advance {}", step["step"].as_str().unwrap_or("?"));
        let report = w.chain.submit(&label, root, bytes.clone(), actors, vec![], time);
        all_applied &= report.ok();
        steps.push(json!({
            "step": step["step"],
            "variant": variant,
            "canonical": canonical,
            "root_is_program_actor": root.program_account_id == program,
            "applied": report.ok(),
            "exit_code": report.exit_code,
            "gas_exec": report.charge.gas_exec,
            "gas_stor": report.charge.gas_stor,
            "cycles": report.charge.cycles,
            "tx_bytes": report.tx_bytes,
            "message_bytes": report.message_bytes,
        }));
        if !report.ok() {
            break;
        }
    }
    let store = w.inst.store(&w.chain);
    let module_state = w.chain.actor_state(instance_actor(program, &w.inst.id)).to_vec();
    let record = w.chain.actor_state(header_actor(program, &w.inst.id, store.finalized.slot)).to_vec();
    // The harness's own job for the same update, on a second copy of the chain.
    let (mut w2, _) = advance_world();
    let run = w2.apply(&fx.update, "harness finality update");
    let harness_state = w2.chain.actor_state(instance_actor(program, &w2.inst.id)).to_vec();
    let harness_record = w2.chain.actor_state(header_actor(program, &w2.inst.id, store.finalized.slot)).to_vec();
    json!({
        "steps": steps,
        "all_applied": all_applied,
        "finalized_slot_after": store.finalized.slot,
        "expected_finalized_slot": fx.finalized_slot,
        "record_written": !record.is_empty(),
        "instance_state_after": h(&module_state),
        "harness_transactions": run.reports.len(),
        "harness_job_id": h(&run.job_id),
        "state_equals_harness_job": module_state == harness_state,
        "record_equals_harness_job": record == harness_record,
    })
}
