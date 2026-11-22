//! The user's own Ethereum transactions from the module's methods, through Logos Core's send stack (`send.rs`):
//! `deposit` and `release` given `send: {"via": "tx_sender_module", "from", "chain_id", "wait"?}` with the Ethereum
//! path (`eth`) and the `vault`. The module builds the call (the commitment and `depositETH`/`deposit`, an ERC-20's
//! `approve` before it in the same bundle; or the release's `release(r, seal)`), reads what it needs through
//! `eth_rpc_module` (the depositor's count, the price, the token's denominations), checks it with `eth_call` from the
//! sender, and hands it to `tx_sender_module`: ONE approval in the Logos signer for the whole bundle, `keystore_module`
//! signs, `send_status` broadcasts. The module holds no Ethereum key. `wait: false` answers at the approval request
//! (`requestId`, `handle`); otherwise the call waits for the broadcast and reads the receipt back.

use std::time::Duration;

use ferrata_bridge_core::Address;
use serde_json::{json, Value};

use crate::admin::vault_error;
use crate::evm::{self, calldata, Abi, Vault};
use crate::send::{self, Call};
use crate::{fixed, hex, u256, unhex};

/// `eth_call` of `data` to `to` from `from` with `value`: `Ok`, or the refusal with the vault's error named.
pub(crate) fn preflight(v: &mut Vault, from: &Address, to: &Address, data: &[u8], value: &str) -> Result<(), String> {
    let value_hex = format!("0x{}", evm::dec_to_hex(value)?);
    match v.rpc.call("eth_call", json!([{ "from": hex(from), "to": hex(to), "data": hex(data), "value": value_hex }, "latest"])) {
        Ok(_) => Ok(()),
        Err(e) => {
            let named = e
                .split(|c: char| !c.is_ascii_hexdigit() && c != 'x')
                .filter(|s| s.starts_with("0x") && s.len() >= 10)
                .find_map(|s| unhex(s).ok().and_then(|b| vault_error(&b)));
            Err(match named {
                Some(name) => format!("refused: vault error {name}() (checked with eth_call before sending; nothing was signed)"),
                None => format!("refused: the call would revert (eth_call before sending; nothing was signed): {e}"),
            })
        }
    }
}

/// Sends `calls` from `from` through the send stack ([`send::through_stack`]); with `wait` (the default) waits for the
/// person's approval and the broadcast and reads every call's receipt.
fn send_bundle(v: &mut Vault, sender: &Value, from: &Address, calls: &[Call], purpose: &str) -> Result<send::Sent, String> {
    let chain_id = sender["chain_id"].as_u64().ok_or("send.chain_id: the chain tx_sender_module sends on")?;
    let timeout = Duration::from_secs(sender["timeout_s"].as_u64().unwrap_or(900));
    send::through_stack(v, chain_id, &hex(from), calls, purpose, sender["wait"] != false, timeout, send::receipt_wait(sender), json!({}))
}

/// An ERC-20 deposit's checks before the person is asked (its `deposit` cannot be run with `eth_call` before the
/// `approve` it follows is mined): the token registered, its deposits enabled and neither it nor the vault frozen,
/// and the depositor holding `total`. A refusal names the vault's error the deposit would revert with.
fn erc20_preflight(v: &mut Vault, from: &Address, token: &Address, total: &str) -> Result<(), String> {
    let refuse = |name: &str| -> Result<(), String> { Err(format!("refused: vault error {name}() (checked with the vault's views before sending; nothing was signed)")) };
    let (registered, enabled, _, _, _) = v.token_config(token)?;
    if !registered {
        return refuse("TokenNotRegistered");
    }
    if !enabled {
        return refuse("DepositsDisabled");
    }
    let now = u128::from(v.now()?);
    if evm::as_u128(&v.getter("depositsFrozenUntil")?)? > now {
        return refuse("DepositsAreFrozen");
    }
    let vault = v.vault;
    let until = evm::word(&v.call_at(&vault, &calldata("tokenDepositsFrozenUntil(address)", &[Abi::addr(token)]), None)?, 0)?;
    if evm::as_u128(&until)? > now {
        return refuse("TokenDepositsAreFrozen");
    }
    let balance = evm::word(&v.call_at(token, &calldata("balanceOf(address)", &[Abi::addr(from)]), None)?, 0)?;
    let want: u128 = total.parse().map_err(|_| format!("price.total {total}: not a decimal amount"))?;
    if evm::as_u128(&balance)? < want {
        return Err(format!("refused: the depositor holds {} base units of {}, the deposit costs {want} (nothing was signed)", evm::dec(&balance), hex(token)));
    }
    Ok(())
}

/// `deposit(request)` with `send`: the deposit built by the module and sent through the send stack.
pub(crate) fn deposit_and_send(req: &Value) -> Result<Value, String> {
    let sender = &req["send"];
    let vault: Address = fixed(&req["vault"], "vault")?;
    // Only a deployment the person named (send.rs `pinned`): refused before the vault is even read.
    send::pinned(sender["chain_id"].as_u64().ok_or("send.chain_id: the chain tx_sender_module sends on")?, &vault)?;
    let mut v = Vault::new(&req["eth"], vault, req["from_block"].as_u64().unwrap_or(0))?;
    let from: Address = fixed(&sender["from"], "send.from: the depositor's account (held by keystore_module)")?;
    let depositor: Address = fixed(&req["depositor"], "depositor")?;
    if from != depositor {
        return Err(format!("send.from {} is not the depositor {}: a deposit names its depositor", hex(&from), hex(&depositor)));
    }
    let token: Address = fixed(&req["token"], "token")?;
    let d = req["denom_index"].as_u64().unwrap_or(0);
    let mut r = req.clone();
    if r["j"].is_null() {
        r["j"] = json!(v.deposit_count(&depositor)?);
    }
    let price = v.price(&token, d as u8)?;
    let made = crate::deposit_of(&r)?;
    let data = unhex(made["vault_call"]["data"].as_str().ok_or("vault_call.data")?)?;
    let total = price["total"].as_str().ok_or("price.total")?.to_string();
    let eth = token.iter().all(|b| *b == 0);
    let mut calls = Vec::new();
    if !eth {
        erc20_preflight(&mut v, &from, &token, &total)?;
        let approve = calldata("approve(address,uint256)", &[Abi::addr(&vault), Abi::W(u256(&json!(total), "total")?)]);
        calls.push(Call::new(token, "0", approve, format!("Allow the Ferrata vault to take {total} base units")));
    } else {
        preflight(&mut v, &from, &vault, &data, &total)?;
    }
    let mut deposit = Call::new(vault, if eth { total.clone() } else { "0".into() }, data,
                                format!("Ferrata deposit: denomination {d} ({} base units) into the vault", price["denomination"].as_str().unwrap_or("?")));
    deposit.gas_limit = send::given_limit(sender)?;
    calls.push(deposit);
    let purpose = format!("Ferrata Bridge deposit of denomination {d}; the LEZ destination stays hidden in the commitment");
    let s = send_bundle(&mut v, sender, &from, &calls, &purpose)?;
    let mut out = made;
    out["price"] = price;
    if let Some(why) = s.unconfirmed_text("this deposit") {
        // Broadcast and not yet mined: not a failure (it may still be mined), and said so.
        out["verdict"] = json!("unconfirmed");
        out["settled"] = json!(false);
        out["pending"] = json!(true);
        out["what"] = json!(why);
        out["tx_hash"] = s.last_hash();
        out["send"] = s.record;
        out["eth"] = v.report();
        return Ok(out);
    }
    let Some(receipt) = s.last().cloned() else {
        out["verdict"] = json!("awaiting_approval");
        out["send"] = s.record;
        return Ok(out);
    };
    if let Some(why) = s.reverted.clone() {
        out["verdict"] = json!("reverted");
        out["settled"] = json!(false);
        out["error"] = json!(format!("{why}; nothing was deposited"));
        out["send"] = s.record;
        out["eth"] = v.report();
        return Ok(out);
    }
    let (ok, block, event) = evm::receipt_summary(&receipt, &vault)?;
    let event = event.ok_or("the receipt has no Deposit event of the vault")?;
    if hex(&event.commitment) != out["commitment"].as_str().unwrap_or("").to_lowercase() {
        return Err("the Deposit event's commitment is not the module's".into());
    }
    // The leaf, deposit id and nullifier, now that the leaf index is known.
    r["leaf_index"] = json!(event.leaf_index);
    r["amount"] = json!(hex(&event.amount));
    let with_leaf = crate::deposit_of(&r)?;
    for k in ["leaf", "deposit_id", "nullifier"] {
        out[k] = with_leaf[k].clone();
    }
    out["verdict"] = json!(if ok { "settled" } else { "reverted" });
    out["settled"] = json!(ok);
    out["sent"] = json!({ "tx_hash": s.last_hash(), "block": block, "gas_used": receipt["gasUsed"], "leaf_index": event.leaf_index, "j": event.j });
    out["send"] = s.record;
    out["eth"] = v.report();
    Ok(out)
}

/// The vault's `release(r, seal)` for a release the module computed (`r`: the `release` reply, explicit or by the
/// wallet's rule), the denomination index read from the vault's registry: `(calldata, recipient)`.
pub(crate) fn release_call(v: &mut Vault, r: &Value, seal: &[u8]) -> Result<(Vec<u8>, Address), String> {
    let p = &r["public"];
    let token: Address = fixed(&p["token"], "public.token")?;
    let recipient: Address = fixed(&p["recipient"], "public.recipient")?;
    let amount = evm::dec(&u256(&p["amount"], "public.amount")?);
    let (_, _, _, _, denominations) = v.token_config(&token)?;
    let d = denominations
        .iter()
        .position(|x| evm::dec(x) == amount)
        .ok_or_else(|| format!("{amount} is not a registered denomination of {}", hex(&token)))?;
    let request = Abi::Tup(vec![
        Abi::addr(&token),
        Abi::uint(d as u128),
        Abi::addr(&recipient),
        Abi::addr(&fixed(&p["relayer"], "public.relayer")?),
        Abi::W(u256(&p["relay_fee"], "public.relay_fee")?),
        Abi::W(fixed(&r["burn_root"], "burn_root")?),
        Abi::W(fixed(&r["nullifier"], "nullifier")?),
    ]);
    Ok((calldata("release((address,uint8,address,address,uint256,bytes32,bytes32),bytes)", &[request, Abi::Bytes(seal.to_vec())]), recipient))
}

/// `release(request)` with `send` and the proof's `seal`: the release sent through the send stack by `send.from`
/// (anyone may send a release; the relay fee goes to the request's relayer, the sender when it is zero).
pub(crate) fn release_and_send(req: &Value, r: &Value) -> Result<Value, String> {
    let sender = &req["send"];
    let vault: Address = fixed(&req["vault"], "vault")?;
    send::pinned(sender["chain_id"].as_u64().ok_or("send.chain_id: the chain tx_sender_module sends on")?, &vault)?;
    let mut v = Vault::new(&req["eth"], vault, req["from_block"].as_u64().unwrap_or(0))?;
    let from: Address = fixed(&sender["from"], "send.from: the sender's account (held by keystore_module)")?;
    let seal = unhex(req["seal"].as_str().ok_or("seal: the release proof's seal (hex)")?)?;
    let (data, recipient) = release_call(&mut v, r, &seal)?;
    preflight(&mut v, &from, &vault, &data, "0")?;
    let before = v.balance(&recipient)?;
    let mut c = Call::new(vault, "0", data, "Ferrata release to the burn's recipient");
    c.gas_limit = send::given_limit(sender)?;
    let s = send_bundle(&mut v, sender, &from, &[c], "Ferrata Bridge release: the vault pays a burn's recipient")?;
    let mut out = r.clone();
    if let Some(why) = s.unconfirmed_text("this release") {
        out["verdict"] = json!("unconfirmed");
        out["settled"] = json!(false);
        out["pending"] = json!(true);
        out["what"] = json!(why);
        out["tx_hash"] = s.last_hash();
        out["send"] = s.record;
        return Ok(out);
    }
    let Some(receipt) = s.last().cloned() else {
        out["verdict"] = json!("awaiting_approval");
        out["send"] = s.record;
        return Ok(out);
    };
    let (ok, block, _) = evm::receipt_summary(&receipt, &vault)?;
    let paid = evm::as_u128(&v.balance(&recipient)?)?.saturating_sub(evm::as_u128(&before)?);
    let settled = ok && s.reverted.is_none();
    out["verdict"] = json!(if settled { "settled" } else { "reverted" });
    out["settled"] = json!(settled);
    if let Some(why) = s.reverted.clone() {
        out["error"] = json!(format!("{why}; nothing was released"));
    }
    out["sent"] = json!({ "tx_hash": s.last_hash(), "block": block, "gas_used": receipt["gasUsed"], "paid_wei": paid.to_string() });
    out["send"] = s.record;
    out["eth"] = v.report();
    Ok(out)
}
