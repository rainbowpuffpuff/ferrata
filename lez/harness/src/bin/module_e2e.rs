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
//! ```
//! Each prints one JSON object. MESSAGE_HEX and ACCOUNT are hex, as the modules return them.

use std::process::exit;

use ferrata_lez_bridge_wire::BridgeMessage;
use ferrata_lez_wire::{instance_actor, Message, ATTESTED_SELECTOR};
use harness::attest::Stage4;
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
        _ => {
            eprintln!("usage: module_e2e info | attest MESSAGE_HEX ACCOUNT_HEX | bridge MESSAGE_HEX");
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
        other => json!({ "variant": format!("{other:?}").split(|c: char| c == ' ' || c == '{' || c == '(').next().unwrap_or("?").to_string() }),
    };
    json!({ "decoded": true, "canonical": canonical, "message_bytes": bytes.len(), "fields": fields })
}
