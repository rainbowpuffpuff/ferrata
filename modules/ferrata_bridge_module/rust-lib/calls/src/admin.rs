//! `admin(request)`: the bridge's admin and freeze-authority functions, in the module (RFP-022 Usability #1: "core
//! functionalities for both users and admin in a Logos core module ... reading and administering the
//! supported-token registry and its permitted amounts").
//!
//! - **Reading** (`status`): the vault's roles, registry entry (registration, denominations = the permitted amounts,
//!   fees, caps and their use, freezes) and global caps, read through Logos Core's `eth_rpc_module` (or a plain
//!   endpoint), with LEZ's side decoded from the bridge program's states when the request gives them (`registry`).
//! - **Administering the vault**: the admin's `registerToken`, `setDepositsEnabled` (delisting), `setTokenCap`,
//!   `setGlobalCap`, `setReleaseDelay`, `announceFees`, `raiseDelta`, `transferAdmin`, `proposeRoles`; the freeze
//!   authority's `setDepositsFrozen`, `setReleasesFrozen`, `setCheckpointsFrozen`, `setTokenDepositsFrozen`,
//!   `setTokenReleasesFrozen`, `revokeFrom`; anyone's `acceptAdmin` (the pending admin), `applyRoles`, `applyFees`,
//!   `retire`. The module builds the call, checks the role and runs it with `eth_call` from the sender (a refusal names
//!   the vault's error before anything is signed), and with `send` hands it to Logos Core's send stack
//!   (`tx_sender_module`; the admin approves in the Logos signer; `send.rs`). Without `send` it answers the call for any
//!   other signer (a multisig, `cast`).
//! - **Administering the bridge program on LEZ**: the admin's `SetCap`, the freeze authority's `Freeze` (and lifting
//!   it) and `Sunset`, a watcher's `Pause`, anyone's `Tick`: the program's Borsh message and the accounts of the public
//!   transaction that carries it, as the program takes it (`lez/guests/bridge/src/main.rs` `role_message`): a role's
//!   message is a top-level call at the role's OWN actor (`own_actor(program, signer)`, signed by the role's key), which
//!   forwards it to `config(cid)`, which checks the role and forwards it to `token(cid, token)` when it names one; a
//!   `Freeze` or `Pause` also names the attestation program's `header(instance, slot)` it reads its time from. A
//!   message sent to `config(cid)` or `token(cid, token)` directly is refused (`NOT_AUTHORIZED`). `Tick` is anyone's
//!   top-level call at the gate it advances. The module builds; the role's LEZ wallet signs and sends.

use std::time::Duration;

use ferrata_bridge_core::{Address, Hash};
use ferrata_lez_bridge_wire::{self as bw, BridgeMessage, BridgeState, CapParams, ConfigState, TimeRecord};
use ferrata_wallet::{AccountId, Mention};
use serde_json::{json, Value};

use crate::evm::{self, as_address, as_u128, calldata, dec, selector, Abi, Vault};
use crate::send::{self, Call};
use crate::{fixed, hex, u256, uint, unhex};

/// The vault's custom errors (`vault/src/FerrataVault.sol`, all without parameters), to name a revert.
const VAULT_ERRORS: &str = "NotAdmin NotFreezeAuthority NotPendingAdmin ZeroAddress RolesMustDiffer VerifierHasNoCode \
InvalidImageIds InvalidLezConfigId InvalidGenesisState InvalidCommitteeFloor InvalidDelta InvalidRevokeWindow \
InvalidMaxFreeze InvalidTurnSeconds InvalidMinReleaseDelay VerifierNotStop TokenAlreadyRegistered TokenNotRegistered \
TokenHasNoCode InvalidDenominations InvalidLezUnit DenominationNotMultipleOfLezUnit InvalidUnitWeight RelayFeeTooHigh \
RelayFeeCountMismatch FeeTooHigh FeeCountMismatch FeeNotMultipleOfLezUnit ReleaseDelayOutOfRange CapBelowFloor \
DeltaCanOnlyRise DeltaTooLong VaultRetired AlreadyRetired RetireNotOpen DepositsAreFrozen TokenDepositsAreFrozen \
DepositsDisabled UseDepositETH InvalidDenominationIndex ZeroCommitment WrongDepositIndex WrongEthAmount \
TransferAmountMismatch TreeFull TokenDepositCapExceeded GlobalDepositCapExceeded CheckpointsAreFrozen WrongMode \
WaitNotOver ImportedLeafCountTooHigh VerifierStopped NotWatcher NoProposal TimelockNotOver ParentNotFinal \
EchoNonceNotZero WrongCommitteeFloor KeySetBelowFloor ImportedLeafCountDecreased SunsetIsFinal CandidateAlreadySubmitted \
UnknownCandidate StaleCandidate CandidateMismatch UnknownBeacon BeaconTooEarly EchoNonceMismatch UnknownCheckpoint \
CheckpointAlreadyRevoked RevokeWindowPassed NotRetired NotRefundable RefundNotYetOpen UnknownLeaf AlreadyRefunded \
InvalidMerklePath ReleasesAreFrozen TokenReleasesAreFrozen BurnRootNotAccepted ReleaseDelayNotElapsed \
NullifierAlreadyUsed InvalidRelayFee ZeroRecipient InsufficientEscrow TokenReleaseCapExceeded GlobalReleaseCapExceeded \
EthTransferFailed NothingOwed DirectEthNotAccepted FunctionNotFound";

/// The vault error a revert's data starts with, by selector.
pub(crate) fn vault_error(data: &[u8]) -> Option<&'static str> {
    let sel = data.get(..4)?;
    VAULT_ERRORS.split_whitespace().find(|name| selector(&format!("{name}()")) == sel)
}

/// Who may send a vault call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Admin,
    FreezeAuthority,
    PendingAdmin,
    Anyone,
}

impl Role {
    fn name(self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::FreezeAuthority => "freeze_authority",
            Role::PendingAdmin => "pending_admin",
            Role::Anyone => "anyone",
        }
    }
}

fn address(v: &Value, what: &str) -> Result<Address, String> {
    fixed::<20>(v, what)
}

fn amount(v: &Value, what: &str) -> Result<Abi, String> {
    Ok(Abi::W(u256(v, what)?))
}

fn amounts(v: &Value, what: &str) -> Result<Abi, String> {
    let list = v.as_array().ok_or_else(|| format!("{what}: a list of amounts"))?;
    Ok(Abi::Arr(list.iter().enumerate().map(|(i, x)| amount(x, &format!("{what}[{i}]"))).collect::<Result<_, _>>()?))
}

fn cap(v: &Value, what: &str) -> Result<Abi, String> {
    Ok(Abi::Tup(vec![amount(&v["capacity"], &format!("{what}.capacity"))?, amount(&v["refill_per_second"], &format!("{what}.refill_per_second"))?]))
}

fn flag(v: &Value, what: &str) -> Result<bool, String> {
    v.as_bool().ok_or_else(|| format!("{what}: true or false"))
}

/// `deposit` (0) or `release` (1), as the vault's `Direction`.
fn direction(v: &Value) -> Result<u128, String> {
    match v {
        Value::String(s) if s == "deposit" => Ok(0),
        Value::String(s) if s == "release" => Ok(1),
        Value::Number(n) if n.as_u64() == Some(0) || n.as_u64() == Some(1) => Ok(u128::from(n.as_u64().unwrap())),
        _ => Err("direction: \"deposit\" or \"release\"".into()),
    }
}

/// The vault call of an admin `action` with its `params`: (function signature, ABI arguments, who may send it, what
/// it does in one line).
fn vault_action(action: &str, p: &Value) -> Result<(&'static str, Vec<Abi>, Role, String), String> {
    let token = || address(&p["token"], "params.token");
    Ok(match action {
        "register_token" => {
            let t = token()?;
            let params = Abi::Tup(vec![
                amounts(&p["denominations"], "params.denominations")?,
                amounts(&p["relay_fees"], "params.relay_fees")?,
                amount(&p["lez_unit"], "params.lez_unit")?,
                amount(&p["unit_weight"], "params.unit_weight")?,
                cap(&p["deposit_cap"], "params.deposit_cap")?,
                cap(&p["release_cap"], "params.release_cap")?,
                amount(&p["release_delay"], "params.release_delay")?,
                Abi::W(fixed::<32>(&p["lez_definition"], "params.lez_definition")?),
                Abi::uint(u128::from(uint(&p["decimals"], "params.decimals")?)),
                amounts(&p["checkpoint_fees"], "params.checkpoint_fees")?,
            ]);
            let n = p["denominations"].as_array().map_or(0, Vec::len);
            (
                "registerToken(address,(uint256[],uint256[],uint256,uint256,(uint256,uint256),(uint256,uint256),uint256,bytes32,uint8,uint256[]))",
                vec![Abi::addr(&t), params],
                Role::Admin,
                format!("register token {} with {n} denominations (the permitted amounts)", hex(&t)),
            )
        }
        "set_deposits_enabled" => {
            let (t, on) = (token()?, flag(&p["enabled"], "params.enabled")?);
            ("setDepositsEnabled(address,bool)", vec![Abi::addr(&t), Abi::boolean(on)], Role::Admin,
             format!("{} deposits of {}", if on { "enable" } else { "disable (delist)" }, hex(&t)))
        }
        "set_token_cap" => {
            let (t, d) = (token()?, direction(&p["direction"])?);
            ("setTokenCap(address,uint8,uint256,uint256)",
             vec![Abi::addr(&t), Abi::uint(d), amount(&p["capacity"], "params.capacity")?, amount(&p["refill_per_second"], "params.refill_per_second")?],
             Role::Admin, format!("set the {} cap of {}", if d == 0 { "deposit" } else { "release" }, hex(&t)))
        }
        "set_global_cap" => {
            let d = direction(&p["direction"])?;
            ("setGlobalCap(uint8,uint256,uint256)",
             vec![Abi::uint(d), amount(&p["capacity"], "params.capacity")?, amount(&p["refill_per_second"], "params.refill_per_second")?],
             Role::Admin, format!("set the global {} cap", if d == 0 { "deposit" } else { "release" }))
        }
        "set_release_delay" => {
            let t = token()?;
            ("setReleaseDelay(address,uint256)", vec![Abi::addr(&t), amount(&p["delay"], "params.delay")?], Role::Admin,
             format!("set the release delay of {}", hex(&t)))
        }
        "announce_fees" => {
            let t = token()?;
            ("announceFees(address,uint256[],uint256[],uint256[])",
             vec![Abi::addr(&t), amounts(&p["mint_fees"], "params.mint_fees")?, amounts(&p["burn_fees"], "params.burn_fees")?,
                  amounts(&p["checkpoint_fees"], "params.checkpoint_fees")?],
             Role::Admin, format!("announce a fee schedule for {} (in force seven days later)", hex(&t)))
        }
        "apply_fees" => {
            let t = token()?;
            ("applyFees(address)", vec![Abi::addr(&t)], Role::Anyone, format!("put the announced fees of {} in force", hex(&t)))
        }
        "raise_delta" => ("raiseDelta(uint256)", vec![amount(&p["delta"], "params.delta")?], Role::Admin, "raise the checkpoint wait".into()),
        "set_deposits_frozen" | "set_releases_frozen" | "set_checkpoints_frozen" => {
            let on = flag(&p["frozen"], "params.frozen")?;
            let (sig, what) = match action {
                "set_deposits_frozen" => ("setDepositsFrozen(bool)", "deposits"),
                "set_releases_frozen" => ("setReleasesFrozen(bool)", "releases"),
                _ => ("setCheckpointsFrozen(bool)", "checkpoints"),
            };
            (sig, vec![Abi::boolean(on)], Role::FreezeAuthority, format!("{} all {what}", if on { "freeze" } else { "unfreeze" }))
        }
        "set_token_deposits_frozen" | "set_token_releases_frozen" => {
            let (t, on) = (token()?, flag(&p["frozen"], "params.frozen")?);
            let (sig, what) = if action == "set_token_deposits_frozen" {
                ("setTokenDepositsFrozen(address,bool)", "deposits")
            } else {
                ("setTokenReleasesFrozen(address,bool)", "releases")
            };
            (sig, vec![Abi::addr(&t), Abi::boolean(on)], Role::FreezeAuthority,
             format!("{} {what} of {}", if on { "freeze" } else { "unfreeze" }, hex(&t)))
        }
        "revoke_from" => ("revokeFrom(uint256)", vec![Abi::uint(u128::from(uint(&p["index"], "params.index")?))], Role::FreezeAuthority,
                          "revoke checkpoints from an index".into()),
        "transfer_admin" => {
            let a = address(&p["new_admin"], "params.new_admin")?;
            ("transferAdmin(address)", vec![Abi::addr(&a)], Role::Admin, format!("offer the admin role to {}", hex(&a)))
        }
        "accept_admin" => ("acceptAdmin()", vec![], Role::PendingAdmin, "accept the admin role".into()),
        "propose_roles" => {
            let w = p["watchers"].as_array().filter(|w| w.len() == 3).ok_or("params.watchers: three addresses")?;
            let watchers = w.iter().map(|x| address(x, "params.watchers[]").map(|a| Abi::addr(&a))).collect::<Result<Vec<_>, _>>()?;
            let fa = address(&p["freeze_authority"], "params.freeze_authority")?;
            ("proposeRoles(address[3],address)", vec![Abi::Tup(watchers), Abi::addr(&fa)], Role::Admin,
             "propose a new freeze authority and watchers (time-locked)".into())
        }
        "apply_roles" => ("applyRoles()", vec![], Role::Anyone, "apply the proposed roles after their time lock".into()),
        "retire" => ("retire()", vec![], Role::Anyone, "retire the vault (deposits stop forever)".into()),
        other => return Err(format!("unknown vault action {other}")),
    })
}

/// Who may send the vault `action` (`admin`, `freeze_authority`, `pending_admin` or `anyone`).
pub(crate) fn role_of(action: &str, params: &Value) -> Result<&'static str, String> {
    Ok(vault_action(action, params)?.2.name())
}

/// Is `from` the role's holder on the vault (read through the request's Ethereum path)?
fn role_holder(v: &mut Vault, role: Role) -> Result<Option<Address>, String> {
    Ok(match role {
        Role::Admin => Some(as_address(&v.getter("admin")?)),
        Role::FreezeAuthority => Some(as_address(&v.getter("freezeAuthority")?)),
        Role::PendingAdmin => Some(as_address(&v.getter("pendingAdmin")?)),
        Role::Anyone => None,
    })
}

/// `eth_call` of the vault call from `from`: `Ok` or the vault's error.
fn preflight(v: &mut Vault, from: &Address, data: &[u8]) -> Result<(), String> {
    let vault = v.vault;
    match v.rpc.call("eth_call", json!([{ "from": hex(from), "to": hex(&vault), "data": hex(data) }, "latest"])) {
        Ok(_) => Ok(()),
        Err(e) => {
            // A node answers a revert with its data in the error; name the vault's error when it is one.
            let named = e
                .split(|c: char| !c.is_ascii_hexdigit() && c != 'x')
                .filter(|s| s.starts_with("0x") && s.len() >= 10)
                .find_map(|s| unhex(s).ok().and_then(|b| vault_error(&b)));
            Err(match named {
                Some(name) => format!("refused: vault error {name}() (checked with eth_call before sending)"),
                None => format!("refused: the vault would revert (eth_call before sending): {e}"),
            })
        }
    }
}

/// A LEZ admin call as the bridge program takes it.
struct LezAdmin {
    message: BridgeMessage,
    /// The transaction's accounts, the root (index 0) first.
    mentions: Vec<Mention>,
    role: &'static str,
    signer: Option<AccountId>,
    what: String,
    /// Whether `signer` holds `role` in the configuration's state (when the request gives it).
    holds: Option<bool>,
    holder: Value,
}

/// The configuration's state, when the request gives it (`config_state`, hex of the bridge program's state).
fn config_state(req: &Value) -> Result<Option<ConfigState>, String> {
    let Some(text) = req["config_state"].as_str().filter(|t| t.len() > 2) else { return Ok(None) };
    match borsh::from_slice::<BridgeState>(&unhex(text)?) {
        Ok(BridgeState::Config(c)) => Ok(Some(c)),
        _ => Err("config_state: not a bridge configuration's state".into()),
    }
}

/// The LEZ admin call of `action` (`lez_set_cap`, `lez_freeze`, `lez_pause`, `lez_tick`, `lez_sunset`), from `signer`
/// (the role's LEZ account, `req.signer`), in the configuration of `req.config_id` of the program at `req.program`.
fn lez_action(action: &str, req: &Value) -> Result<LezAdmin, String> {
    let p = &req["params"];
    let program = AccountId::new(fixed::<32>(&req["program"], "program (the bridge program's account)")?);
    let cid: Hash = fixed(&req["config_id"], "config_id")?;
    let token: Option<Address> = if p["token"].is_null() { None } else { Some(fixed(&p["token"], "params.token")?) };
    let state = config_state(req)?;
    let signer = if req["signer"].is_null() { None } else { Some(AccountId::new(fixed::<32>(&req["signer"], "signer (the role's LEZ account)")?)) };
    let record = || -> Result<TimeRecord, String> {
        let r = &p["record"];
        Ok(TimeRecord {
            record_slot: uint(&r["record_slot"], "params.record.record_slot")?,
            header: if r["header"].is_null() { None } else { Some(unhex(r["header"].as_str().ok_or("params.record.header: hex")?)?) },
        })
    };
    // The attestation program's record a `Freeze`, `Pause` or `Tick` reads its time from.
    let header = |slot: u64| -> Result<Mention, String> {
        let (attestation, instance) = match &state {
            Some(c) => (c.config().attestation, c.config().instance_id),
            None => (AccountId::new(fixed::<32>(&req["attestation"], "attestation (or config_state)")?), fixed::<32>(&req["instance_id"], "instance_id (or config_state)")?),
        };
        Ok(Mention::Public(ferrata_lez_wire::header_actor(attestation, &instance, slot)))
    };
    let u128_of = |v: &Value, what: &str| -> Result<u128, String> { as_u128(&u256(v, what)?) };
    let (message, role, what) = match action {
        "lez_set_cap" => {
            let mints = flag(&p["mints"], "params.mints")?;
            let cap = CapParams { capacity: u128_of(&p["capacity"], "params.capacity")?, refill_per_second: u128_of(&p["refill_per_second"], "params.refill_per_second")? };
            let what = format!("SetCap: {} cap {} LEZ units, refill {}/s", if mints { "mint" } else { "burn" }, cap.capacity, cap.refill_per_second);
            (BridgeMessage::SetCap { config_id: cid, token, mints, cap }, "admin", what)
        }
        "lez_freeze" => {
            let until_ms = uint(&p["until_ms"], "params.until_ms (0 lifts the freeze)")?;
            let (mints, burns) = (flag(&p["mints"], "params.mints")?, flag(&p["burns"], "params.burns")?);
            let what = if until_ms == 0 { "Freeze lifted".to_string() } else { format!("Freeze until {until_ms} ms") };
            (BridgeMessage::Freeze { config_id: cid, token, mints, burns, until_ms, record: record()? }, "freeze_authority", what)
        }
        "lez_pause" => (
            BridgeMessage::Pause { config_id: cid, token, mints: flag(&p["mints"], "params.mints")?, burns: flag(&p["burns"], "params.burns")?, record: record()? },
            "watcher",
            "Pause (12 hours)".to_string(),
        ),
        "lez_tick" => (BridgeMessage::Tick { config_id: cid, token, record: record()? }, "anyone", "Tick".to_string()),
        "lez_sunset" => (BridgeMessage::Sunset { config_id: cid }, "freeze_authority", "Sunset".to_string()),
        other => return Err(format!("unknown LEZ action {other}")),
    };
    let config_at = bw::config_actor(program, &cid);
    let gate = |t: &Option<Address>| match t {
        Some(t) => (bw::token_actor(program, &cid, t), bw::token_seed(&cid, t)),
        None => (config_at, bw::config_seed(&cid)),
    };
    let mut mentions = Vec::new();
    if role == "anyone" {
        // `Tick`: a top-level call at the gate it advances, then the record.
        let (at, seed) = gate(&token);
        mentions.push(Mention::Pda { actor: at, seed });
    } else {
        let signer = signer.ok_or_else(|| format!("signer: the LEZ account of the configuration's {role} (the message is sent at its own actor)"))?;
        mentions.push(Mention::Signer(bw::own_actor(program, signer)));
        mentions.push(Mention::Public(config_at));
        if let Some(t) = &token {
            mentions.push(Mention::Public(bw::token_actor(program, &cid, t)));
        }
    }
    if let BridgeMessage::Freeze { record, .. } | BridgeMessage::Pause { record, .. } | BridgeMessage::Tick { record, .. } = &message {
        mentions.push(header(record.record_slot)?);
    }
    // Whether the signer holds the role, from the configuration's state (the program's own checks decide).
    let (holds, holder) = match (&state, signer) {
        (Some(ConfigState::Ready { config, roles, .. }), Some(s)) => match role {
            "admin" => (Some(config.controls.admin == *s.value()), json!(hex(&config.controls.admin))),
            "freeze_authority" => (Some(roles.freeze_authority.single(s.value())),
                                   json!(roles.freeze_authority.members.iter().map(|m| hex(m)).collect::<Vec<_>>())),
            "watcher" => (Some(roles.watchers.contains(s.value())), json!(roles.watchers.iter().map(|m| hex(m)).collect::<Vec<_>>())),
            _ => (None, Value::Null),
        },
        _ => (None, Value::Null),
    };
    Ok(LezAdmin { message, mentions, role, signer, what, holds, holder })
}

fn timeout(send: &Value) -> Duration {
    Duration::from_secs(send["timeout_s"].as_u64().unwrap_or(600))
}

/// One Ethereum admin action: built, role and `eth_call` checked from `from`, then (with `send`) through Logos Core's
/// send stack.
pub(crate) fn vault_admin(v: &mut Vault, action: &str, p: &Value, sender: &Value, chain_id: u64) -> Result<Value, String> {
    if sender["via"].as_str() == Some(send::TX_SENDER) {
        // Only a deployment the person named (send.rs `pinned`), before the vault's roles are read.
        send::pinned(chain_id, &v.vault)?;
    }
    let (signature, args, role, what) = vault_action(action, p)?;
    let data = calldata(signature, &args);
    let call = json!({ "to": hex(&v.vault), "function": signature, "data": hex(&data), "value": "0", "role": role.name(), "what": what });
    let from = if sender["from"].is_null() { None } else { Some(address(&sender["from"], "send.from")?) };
    if let Some(from) = from {
        if let Some(holder) = role_holder(v, role)? {
            if holder != from {
                return Err(format!("refused: {} is not the vault's {} ({}); checked before sending", hex(&from), role.name(), hex(&holder)));
            }
        }
        preflight(v, &from, &data)?;
    }
    if sender["via"].as_str() != Some(send::TX_SENDER) {
        return Ok(json!({ "verdict": "built", "call": call, "checked_from": from.map(|a| hex(&a)),
                          "note": "not sent: give send {via: tx_sender_module, from, chain_id} to send it through Logos Core's send stack" }));
    }
    let from = from.ok_or("send.from: the role's Ethereum account (held by keystore_module)")?;
    let mut c = Call::new(v.vault, "0", data.clone(), format!("Ferrata vault: {what}"));
    c.gas_limit = send::given_limit(sender)?;
    let wait_for_it = sender["wait"] != false;
    let s = send::through_stack(v, chain_id, &hex(&from), &[c], &format!("Ferrata bridge admin ({}): {what}", role.name()),
                                wait_for_it, timeout(sender), send::receipt_wait(sender), json!({ "role": role.name() }))?;
    if !wait_for_it {
        return Ok(json!({ "verdict": "awaiting_approval", "call": call, "send": s.record, "approve_in": send::APPROVE_IN }));
    }
    if let Some(why) = s.unconfirmed_text("this admin call") {
        return Ok(json!({ "verdict": "unconfirmed", "settled": false, "pending": true, "what": why, "error": why, "call": call,
                          "tx_hash": s.last_hash(), "send": s.record, "eth": v.report() }));
    }
    let receipt = s.last().cloned().ok_or("tx_sender_module broadcast no transaction")?;
    let (ok, block, _) = evm::receipt_summary(&receipt, &v.vault)?;
    let settled = ok && s.reverted.is_none();
    let mut out = json!({
        "verdict": if settled { "settled" } else { "reverted" },
        "settled": settled,
        "call": call,
        "tx_hash": s.last_hash(),
        "block": block,
        "gas_used": receipt["gasUsed"],
        "send": s.record,
        "eth": v.report(),
    });
    if !settled {
        out["error"] = json!(s.reverted.unwrap_or_else(|| format!("reverted: the vault call was mined in block {block} and reverted")));
    }
    Ok(out)
}

/// `admin(request)`.
pub(crate) fn admin_of(req: &Value) -> Result<Value, String> {
    let action = req["action"].as_str().ok_or("action: status, a vault action or a LEZ action (lez_*)")?;
    if action.starts_with("lez_") {
        let l = lez_action(action, req)?;
        let bytes = borsh::to_vec(&l.message).map_err(|e| e.to_string())?;
        return Ok(json!({
            "verdict": "built",
            "what": l.what,
            "message": hex(&bytes),
            "accounts": crate::wallet::mentions_json(&l.mentions),
            "root_index": 0,
            "program": req["program"],
            "role": l.role,
            "signer": l.signer.map(|s| hex(s.value())),
            "signer_holds_role": l.holds,
            "role_holder": l.holder,
            "note": if l.role == "anyone" {
                "a public transaction rooted at accounts[0] (the gate), sent by anyone"
            } else {
                "a public transaction rooted at accounts[0], the role's own actor, signed by `signer`: the program forwards it to config(cid), which checks the role"
            },
        }));
    }
    let vault: Address = fixed(&req["vault"], "vault")?;
    let mut v = Vault::new(&req["eth"], vault, req["from_block"].as_u64().unwrap_or(0))?;
    if action == "status" {
        let mut out = json!({ "roles": v.roles()?, "eth": Value::Null });
        let tokens = req["tokens"].as_array().cloned().unwrap_or_else(|| vec![json!(hex(&[0u8; 20]))]);
        let mut entries = Vec::new();
        for t in &tokens {
            entries.push(v.registry(&address(t, "tokens[]")?)?);
        }
        out["registry"] = json!(entries);
        if !req["config_state"].is_null() || !req["token_state"].is_null() {
            out["lez"] = crate::wallet::registry_of(req)?;
        }
        out["eth"] = v.report();
        return Ok(out);
    }
    if action == "read" {
        // The wallet session's own vault reads (evm.rs), one by name: what the session decides with on the `eth` path.
        let token: Address = if req["token"].is_null() { [0u8; 20] } else { address(&req["token"], "token")? };
        let what = req["what"].as_str().ok_or("what: deposit_list, price, roots, released, refunded, registry, roles, deposit_count")?;
        let out = match what {
            "deposit_list" => {
                let (block, deposits, root) = v.deposit_list()?;
                json!({ "block": block, "root": hex(&root), "deposits": deposits.iter().map(|d| json!({
                    "leaf_index": d.leaf_index, "depositor": hex(&d.depositor), "j": d.j, "token": hex(&d.token),
                    "amount": dec(&d.amount), "commitment": hex(&d.commitment), "block": d.block })).collect::<Vec<_>>() })
            }
            "price" => v.price(&token, uint(&req["denomination"], "denomination")? as u8)?,
            "roots" => v.roots_for_release(&token)?,
            "released" => json!(v.released_nullifiers()?.iter().map(|n| hex(n)).collect::<Vec<_>>()),
            "refunded" => json!(v.refunded_leaves()?),
            "registry" => v.registry(&token)?,
            "roles" => v.roles()?,
            "deposit_count" => json!(v.deposit_count(&address(&req["depositor"], "depositor")?)?),
            other => return Err(format!("unknown read {other}")),
        };
        return Ok(json!({ "read": what, "value": out, "eth": v.report() }));
    }
    let chain_id = req["send"]["chain_id"].as_u64().or_else(|| req["eth"]["chain_id"].as_u64()).unwrap_or(0);
    vault_admin(&mut v, action, &req["params"], &req["send"], chain_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vault_error_is_named_by_its_selector() {
        let sel = selector("NotAdmin()");
        assert_eq!(vault_error(&sel), Some("NotAdmin"));
        assert_eq!(vault_error(&selector("CapBelowFloor()")), Some("CapBelowFloor"));
        assert_eq!(vault_error(&[0, 0, 0, 0]), None);
    }

    #[test]
    fn register_token_is_encoded_as_the_vault_declares_it() {
        let p = json!({
            "token": "0x00000000000000000000000000000000000000aa",
            "denominations": ["1000000", "10000000"], "relay_fees": ["1000", "10000"],
            "lez_unit": "1", "unit_weight": "1",
            "deposit_cap": {"capacity": "100000000", "refill_per_second": "1000"},
            "release_cap": {"capacity": "100000000", "refill_per_second": "1000"},
            "release_delay": "3600", "lez_definition": format!("0x{}", "11".repeat(32)), "decimals": 6,
            "checkpoint_fees": ["0", "0"],
        });
        let (sig, args, role, _) = vault_action("register_token", &p).unwrap();
        assert_eq!(role, Role::Admin);
        let data = calldata(sig, &args);
        // selector, then: token, the offset of the dynamic TokenParams tuple (0x40).
        assert_eq!(&data[..4], &selector(sig));
        let body = &data[4..];
        let mut aa = [0u8; 20];
        aa[19] = 0xaa;
        assert_eq!(as_address(&evm::word(body, 0).unwrap()), aa);
        assert_eq!(evm::as_u64(&evm::word(body, 1).unwrap()).unwrap(), 0x40);
        // Inside the tuple (from word 2): offsets of the three arrays, the two static caps inline (2 words each).
        let t = &body[64..];
        // Head: denominations off, relayFees off, lezUnit, unitWeight, depositCap (2), releaseCap (2), releaseDelay,
        // lezDefinition, decimals, checkpointFees off = 12 words; the first array starts right after.
        assert_eq!(evm::as_u64(&evm::word(t, 0).unwrap()).unwrap(), 12 * 32);
        assert_eq!(evm::as_u64(&evm::word(t, 12).unwrap()).unwrap(), 2);
        assert_eq!(evm::as_u64(&evm::word(t, 13).unwrap()).unwrap(), 1_000_000);
        assert_eq!(evm::as_u64(&evm::word(t, 10).unwrap()).unwrap(), 6);
        assert_eq!(dec(&evm::word(t, 4).unwrap()), "100000000");
    }

    #[test]
    fn freeze_actions_need_the_freeze_authority_and_a_flag() {
        let (sig, args, role, what) =
            vault_action("set_token_deposits_frozen", &json!({ "token": "0x00000000000000000000000000000000000000aa", "frozen": true })).unwrap();
        assert_eq!((sig, role), ("setTokenDepositsFrozen(address,bool)", Role::FreezeAuthority));
        assert!(what.starts_with("freeze deposits"));
        assert_eq!(evm::as_u64(&evm::word(&calldata(sig, &args)[4..], 1).unwrap()).unwrap(), 1);
        assert!(vault_action("set_deposits_frozen", &json!({})).is_err());
        assert!(vault_action("propose_roles", &json!({ "watchers": ["0x01"], "freeze_authority": "0x02" })).is_err());
        assert!(vault_action("nonsense", &json!({})).is_err());
    }

    /// The program's rule (main.rs `role_message`): the root is the role's own actor (account = the signer, owned by
    /// the program), never `config(cid)` or `token(cid, t)`; config(cid) comes next, then the token, then the record.
    #[test]
    fn lez_role_messages_are_rooted_at_the_roles_own_actor() {
        let program = AccountId::new([0x22; 32]);
        let signer = [0x44u8; 32];
        let attestation = [0x55u8; 32];
        let base = json!({
            "program": format!("0x{}", "22".repeat(32)), "config_id": format!("0x{}", "33".repeat(32)),
            "signer": format!("0x{}", "44".repeat(32)),
            "attestation": format!("0x{}", "55".repeat(32)), "instance_id": format!("0x{}", "66".repeat(32)),
        });
        let mut cap = base.clone();
        cap["action"] = json!("lez_set_cap");
        cap["params"] = json!({ "token": format!("0x{}", "00".repeat(20)), "mints": true, "capacity": "11000000000", "refill_per_second": "10000000" });
        let out = admin_of(&cap).unwrap();
        let acc = out["accounts"].as_array().unwrap();
        assert_eq!(acc.len(), 3);
        assert_eq!((acc[0]["kind"].as_str(), acc[0]["account"].as_str()), (Some("signer"), Some(format!("0x{}", "44".repeat(32)).as_str())));
        assert_eq!(acc[0]["program"], json!(format!("0x{}", "22".repeat(32))), "the actor of the bridge program's state in the signer's account");
        assert_ne!(acc[0]["account"], acc[1]["account"]);
        assert_eq!(acc[1]["kind"], "public");
        assert_eq!(acc[1]["account"], json!(hex(bw::config_actor(program, &[0x33; 32]).account_id.value())));
        assert_eq!(acc[2]["account"], json!(hex(bw::token_actor(program, &[0x33; 32], &[0u8; 20]).account_id.value())));
        assert_eq!(out["root_index"], 0);
        let m: BridgeMessage = borsh::from_slice(&unhex(out["message"].as_str().unwrap()).unwrap()).unwrap();
        assert!(matches!(m, BridgeMessage::SetCap { token: Some(_), mints: true, cap: CapParams { capacity: 11_000_000_000, .. }, .. }));
        // Without the role's account there is nothing to sign at: refused, not built at the configuration.
        let mut unsigned = cap.clone();
        unsigned["signer"] = Value::Null;
        assert!(admin_of(&unsigned).unwrap_err().starts_with("signer:"));
        // A freeze also names the attestation program's record its time is read from (the last account).
        let mut fr = base.clone();
        fr["action"] = json!("lez_freeze");
        fr["params"] = json!({ "token": format!("0x{}", "00".repeat(20)), "mints": true, "burns": false, "until_ms": 0, "record": { "record_slot": 9 } });
        let out = admin_of(&fr).unwrap();
        let acc = out["accounts"].as_array().unwrap();
        assert_eq!(acc.len(), 4);
        assert_eq!(acc[3]["account"], json!(hex(ferrata_lez_wire::header_actor(AccountId::new(attestation), &[0x66; 32], 9).account_id.value())));
        assert_eq!(acc[3]["program"], json!(format!("0x{}", "55".repeat(32))));
        assert_eq!(out["role"], "freeze_authority");
        let _ = signer;
        // Tick: anyone's call at the gate (a PDA root with its seed), no signer.
        let mut tick = base.clone();
        tick["action"] = json!("lez_tick");
        tick["signer"] = Value::Null;
        tick["params"] = json!({ "token": null, "record": { "record_slot": 9 } });
        let out = admin_of(&tick).unwrap();
        assert_eq!(out["accounts"][0]["kind"], "pda");
        assert_eq!(out["accounts"][0]["account"], json!(hex(bw::config_actor(program, &[0x33; 32]).account_id.value())));
    }

    /// A watcher's Pause (with its record) and the freeze authority's Sunset (no token, no record: it ends the
    /// configuration for good) are built at the role's own actor too.
    #[test]
    fn pause_and_sunset_are_rooted_at_the_roles_own_actor() {
        let program = AccountId::new([0x22; 32]);
        let base = json!({
            "program": format!("0x{}", "22".repeat(32)), "config_id": format!("0x{}", "33".repeat(32)),
            "signer": format!("0x{}", "44".repeat(32)),
            "attestation": format!("0x{}", "55".repeat(32)), "instance_id": format!("0x{}", "66".repeat(32)),
        });
        let mut pause = base.clone();
        pause["action"] = json!("lez_pause");
        pause["params"] = json!({ "token": format!("0x{}", "00".repeat(20)), "mints": true, "burns": true, "record": { "record_slot": 11 } });
        let out = admin_of(&pause).unwrap();
        assert_eq!(out["role"], "watcher");
        let acc = out["accounts"].as_array().unwrap();
        assert_eq!(acc.len(), 4, "own actor, config(cid), token(cid, ETH), the record");
        assert_eq!((acc[0]["kind"].as_str(), acc[0]["account"].as_str()), (Some("signer"), Some(format!("0x{}", "44".repeat(32)).as_str())));
        assert_eq!(acc[1]["account"], json!(hex(bw::config_actor(program, &[0x33; 32]).account_id.value())));
        assert_eq!(acc[2]["account"], json!(hex(bw::token_actor(program, &[0x33; 32], &[0u8; 20]).account_id.value())));
        assert_eq!(acc[3]["account"], json!(hex(ferrata_lez_wire::header_actor(AccountId::new([0x55; 32]), &[0x66; 32], 11).account_id.value())));
        let m: BridgeMessage = borsh::from_slice(&unhex(out["message"].as_str().unwrap()).unwrap()).unwrap();
        assert!(matches!(m, BridgeMessage::Pause { token: Some(_), mints: true, burns: true, record: TimeRecord { record_slot: 11, .. }, .. }));
        let mut sunset = base.clone();
        sunset["action"] = json!("lez_sunset");
        sunset["params"] = json!({});
        let out = admin_of(&sunset).unwrap();
        assert_eq!(out["role"], "freeze_authority");
        let acc = out["accounts"].as_array().unwrap();
        assert_eq!(acc.len(), 2, "own actor and config(cid): no token, no record");
        assert_eq!(acc[0]["kind"], "signer");
        assert_eq!(acc[1]["account"], json!(hex(bw::config_actor(program, &[0x33; 32]).account_id.value())));
        let m: BridgeMessage = borsh::from_slice(&unhex(out["message"].as_str().unwrap()).unwrap()).unwrap();
        assert_eq!(m, BridgeMessage::Sunset { config_id: [0x33; 32] });
        let mut unsigned = sunset.clone();
        unsigned["signer"] = Value::Null;
        assert!(admin_of(&unsigned).unwrap_err().starts_with("signer:"), "never built without the role's account");
    }
}
