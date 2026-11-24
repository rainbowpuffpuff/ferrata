//! An execution source's rule (`kind = "execution"`, `source::execution_canonical`): an update's finalized header is
//! confirmed by an execution endpoint only if the endpoint holds the block the header binds canonical at its number.

use ferrata_updater::source::{execution_canonical, rpc_result};
use serde_json::{json, Value};

#[test]
fn an_execution_block_counts_only_where_the_endpoint_holds_it_canonical() {
    let h = [0xab; 32];
    let hx = format!("0x{}", hex::encode(h));
    let other = format!("0x{}", "cd".repeat(32));
    let by_hash = |number: &str| json!({ "hash": hx, "number": number });
    // Held, and the block at its number: canonical.
    let mut calls = Vec::new();
    let got = execution_canonical(
        |m, p| {
            calls.push((m.to_string(), p.clone()));
            Ok(if m == "eth_getBlockByHash" { by_hash("0x10") } else { json!({ "hash": hx.to_uppercase().replace("0X", "0x") }) })
        },
        &h,
    )
    .unwrap();
    assert_eq!(got, Some(16));
    assert_eq!(calls[0], ("eth_getBlockByHash".to_string(), json!([hx, false])));
    assert_eq!(calls[1], ("eth_getBlockByNumber".to_string(), json!(["0x10", false])));
    // Not held at all.
    assert_eq!(execution_canonical(|_, _| Ok(Value::Null), &h).unwrap(), None);
    // Held, but another block at its number (a fork the endpoint left, or another chain).
    let fork = execution_canonical(|m, _| Ok(if m == "eth_getBlockByHash" { by_hash("0x10") } else { json!({ "hash": other }) }), &h);
    assert_eq!(fork.unwrap(), None);
    // No block at that number either.
    let none = execution_canonical(|m, _| Ok(if m == "eth_getBlockByHash" { by_hash("0x10") } else { Value::Null }), &h);
    assert_eq!(none.unwrap(), None);
    // An answer about another block, or without a number: an error, not a confirmation.
    assert!(execution_canonical(|_, _| Ok(json!({ "hash": other, "number": "0x10" })), &h).is_err());
    assert!(execution_canonical(|_, _| Ok(json!({ "hash": hx })), &h).is_err());
    assert!(execution_canonical(|_, _| Ok(json!({ "hash": hx, "number": "zz" })), &h).is_err());
    // A call that fails is an error.
    assert!(execution_canonical(|_, _| anyhow::bail!("down"), &h).is_err());
}

#[test]
fn json_rpc_answers() {
    assert_eq!(rpc_result(r#"{"jsonrpc":"2.0","id":1,"result":{"hash":"0x01"}}"#).unwrap(), json!({ "hash": "0x01" }));
    assert_eq!(rpc_result(r#"{"jsonrpc":"2.0","id":1,"result":null}"#).unwrap(), Value::Null);
    assert!(rpc_result(r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"no"}}"#).is_err());
    assert!(rpc_result(r#"{"jsonrpc":"2.0","id":1}"#).is_err());
    assert!(rpc_result("not json").is_err());
}
