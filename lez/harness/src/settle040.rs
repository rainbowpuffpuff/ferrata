//! LEZ v0.4.0's per-transaction fee settlement, as the sequencer and the indexer run it (feature `lez-040`; the
//! release candidates' is `settle.rs`, which this module replaces under the same name and interface).
//!
//! Copied from logos-execution-zone v0.4.0 (commit a255b42c, MIT or Apache-2.0):
//! `lez/chain_state/src/apply.rs` (`settle_charged_transaction`, `settle_private_transaction`),
//! `lez/chain_state/src/classify.rs` (`classify`, `private_fee_reserve`) and `lez/common/src/transaction.rs`
//! (`LeeTransaction::charged_bytes`, `fee_reserve_invocation`, `fee_refund_invocation`,
//! `private_fee_refund_invocation`, `validate_private_fee_credit`, `validate_reward_target`, the restricted-account
//! guard). `chain_state` and `common` themselves are not used because they pull in zone-sdk and with it the Bedrock
//! node (libp2p, RocksDB); every call into `lee`, the state machine, is the same.
//!
//! What v0.4.0 changed from the release candidates, all here:
//! - receipts (LEZ #1107): settlement returns a receipt per transaction (success, the gas, base fee, tip and refund
//!   charged) in place of the block's list of reverted transactions: [`Charge::receipt`];
//! - fees by size (LEZ #1044): a private transaction pays storage gas for its bytes (`charged_bytes`, its proof
//!   counted at `market::PROOF_BYTES`) instead of a fixed `PRIVATE_GAS_STOR`; it may pay its producer a tip inside
//!   its proof (LEZ #1125);
//! - a public payer for a private transaction (LEZ #1080): a private transaction may declare a fee paid by a public
//!   signer, settled like a public transaction whose gas limit also covers the proof's verification;
//! - authority (LEZ #1144): the settlement's own invocations declare each actor's authority (the reserve marks the
//!   payer's native balance `Signer`).
//!
//! Changes from the original, none of which changes what is charged:
//! - errors are strings instead of `BlockIngestError`;
//! - the functions also return the charge (cycles, storage gas, fee) for the measurements;
//! - `classify` keeps only the user-transaction paths (no genesis, system injections or sequencer-stake operations,
//!   which these tests never send);
//! - `validate_user_state_modification` without its system-injection exception (no injection reaches this
//!   settlement): the clock/fee guard, the bridge escrow's (increase-only, public transactions only) and the
//!   `system_upgrader` registry's (no user transaction writes it), as upstream.

use std::collections::{BTreeMap, HashSet};

use borsh::{BorshDeserialize, BorshSerialize};
use fee_core::{
    assess::{fee_actual_base, fee_reserve, private_cycles_affordable, private_fee_required, FeeTxView},
    market,
    state::FeeState,
    validity::{accumulate_exec_gas, accumulate_stor_gas, validate_static_tx, FeeError},
    BlockFeeSummary,
};
use lee::{
    privacy_preserving_transaction::Message as PrivateMessage, AccountId, Actor, PrivacyPreservingTransaction,
    PublicTransaction, V03State, ValidatedStateDiff,
};
use lee_core::{
    account::Balance,
    execution_state::{Authority, BoundaryStep, Delivery},
    native_token,
    program::TransactionEvent,
    BlockId, Timestamp,
};

/// `common::transaction::LeeTransaction`: same variants, same borsh encoding.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[allow(clippy::large_enum_variant)]
pub enum LeeTransaction {
    Public(PublicTransaction),
    PrivacyPreserving(PrivacyPreservingTransaction),
}

impl LeeTransaction {
    /// `common::transaction::LeeTransaction::charged_bytes`: the serialized size, a private transaction's proof
    /// counted at the fixed `market::PROOF_BYTES` it may not exceed (LEZ #1044).
    pub fn charged_bytes(&self) -> Result<u64, String> {
        let size = u64::try_from(borsh::object_length(self).map_err(|e| e.to_string())?).expect("size fits u64");
        Ok(match self {
            Self::Public(_) => size,
            Self::PrivacyPreserving(tx) => {
                let proof = u64::try_from(tx.witness_set().proof().size()).expect("a proof's size fits u64");
                size.saturating_sub(proof).saturating_add(market::PROOF_BYTES)
            }
        })
    }

    /// `common::transaction::LeeTransaction::is_fee_authorized`.
    pub fn is_fee_authorized(&self) -> bool {
        match self {
            Self::Public(tx) => lee::is_fee_authorized(tx.message()),
            Self::PrivacyPreserving(tx) => lee::is_fee_authorized(tx.message()),
        }
    }
}

/// The receipt LEZ v0.4.0 keeps for every transaction (`common::block::TransactionReceipt`, LEZ #1107): whether
/// its action applied, and what it was charged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Receipt {
    pub success: bool,
    pub gas_exec: u64,
    pub gas_stor: u64,
    pub fee_base: Balance,
    pub tip: Balance,
    pub refund: Balance,
}

/// What settlement charged one transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Charge {
    /// Execution gas: the metered cycles, clamped to the gas limit (public) or to the budget the
    /// reserve pays for (private), plus `PRIVATE_VERIFY_GAS` for a private transaction.
    pub gas_exec: u64,
    /// The metered cycles of the action as `lee` reports them (before clamping).
    pub cycles: u64,
    /// Storage gas: the transaction's charged bytes (`LeeTransaction::charged_bytes`).
    pub gas_stor: u64,
    /// The base fee charged, at the block's opening base fees.
    pub fee_base: Balance,
    /// Why the action reverted, if it did (the fee is still charged).
    pub revert: Option<String>,
    /// The transaction's receipt (LEZ #1107).
    pub receipt: Receipt,
}

/// `chain_state::classify::classify` for user transactions (`Err` is a `ClassifyError`'s message). Upstream first
/// classifies genesis transactions, system injections (bridge deposits, system upgrades) and sequencer-stake
/// operations as fee-exempt; the harness sends none, and this function treats every public transaction as a user's:
/// one without a fee declaration is refused here, not settled as exempt.
pub fn classify(tx: &LeeTransaction) -> Result<FeeTxView, String> {
    match tx {
        LeeTransaction::PrivacyPreserving(private_tx) => {
            let execution = &private_tx.message().execution;
            let count = execution.private_actions.len();
            if count > market::MAX_PRIVATE_ACTIONS {
                return Err(format!(
                    "private transaction carries {count} private actions, more than the {} allowed",
                    market::MAX_PRIVATE_ACTIONS
                ));
            }
            let size = private_tx.witness_set().proof().size();
            if u64::try_from(size).map_or(true, |size| size > market::PROOF_BYTES) {
                return Err(format!(
                    "private transaction's proof is {size} bytes, more than the {} allowed",
                    market::PROOF_BYTES
                ));
            }
            // A public signer's declaration pays like a public transaction whose gas limit also covers the
            // proof's verification (LEZ #1080).
            if let Some(fee) = private_tx.message().fee {
                if execution.proven_fee.is_some() || inbox_crossings(private_tx.message()).next().is_some() {
                    return Err("private transaction pays its fee both in its proof and by declaration".into());
                }
                if fee.gas_limit < market::PRIVATE_VERIFY_GAS {
                    return Err(format!(
                        "private transaction declares gas limit {}, below the {} its proof's verification costs",
                        fee.gas_limit,
                        market::PRIVATE_VERIFY_GAS
                    ));
                }
                return Ok(declared_view(fee, tx.charged_bytes()?));
            }
            let Some((payer, reserve, crossing)) = private_fee_reserve(private_tx.message()) else {
                return Err("private transaction omits its required in-proof fee".into());
            };
            if let Some(fallback) = execution.proven_fee.as_ref().and_then(|fee| fee.fallback.as_ref()) {
                if fallback.boundary
                    != [BoundaryStep::PrivateToPublic(crossing.clone()), BoundaryStep::EndPublicSubtree]
                {
                    return Err("private fee fallback does not replay the fee transfer alone".into());
                }
            }
            if let Some(target) = execution.proven_fee.as_ref().and_then(|fee| fee.refund_to) {
                validate_reward_target(target)
                    .map_err(|reason| format!("private fee refund target is invalid: {reason}"))?;
            }
            Ok(FeeTxView::Private {
                payer,
                reserve,
                data_bytes: tx.charged_bytes()?,
                tip: execution.proven_fee.as_ref().map_or(0, |fee| fee.tip),
            })
        }
        LeeTransaction::Public(public_tx) => {
            let Some(fee) = public_tx.message().fee else {
                return Err("user public transaction omits its required fee declaration".into());
            };
            Ok(declared_view(fee, tx.charged_bytes()?))
        }
    }
}

/// The view a declared fee is priced and settled by.
const fn declared_view(fee: lee::FeeDeclaration, data_bytes: u64) -> FeeTxView {
    FeeTxView::Public { payer: fee.payer, gas_limit: fee.gas_limit, data_bytes, tip: fee.tip, max_fee: fee.max_fee }
}

fn inbox_crossings(message: &PrivateMessage) -> impl Iterator<Item = &Delivery<Actor>> {
    let inbox = Actor::native_balance(system_accounts::fee_inbox_account_id());
    message.execution.boundary.iter().filter_map(move |step| match step {
        BoundaryStep::PrivateToPublic(delivery) if delivery.envelope.to == inbox => Some(delivery),
        _ => None,
    })
}

fn private_fee_reserve(message: &PrivateMessage) -> Option<(AccountId, Balance, &Delivery<Actor>)> {
    let mut crossings = inbox_crossings(message);
    let delivery = crossings.next()?;
    if crossings.next().is_some() {
        return None;
    }
    match borsh::from_slice(&delivery.envelope.message).ok()? {
        native_token::Message::Credit(reserve) => Some((delivery.envelope.from.account_id, reserve, delivery)),
        native_token::Message::Transfer { .. } | native_token::Message::ReadState => None,
    }
}

/// `common::transaction::validate_reward_target`: not a restricted system account.
fn validate_reward_target(target: AccountId) -> Result<(), String> {
    let restricted = std::iter::once(system_accounts::clock_account_id())
        .chain(system_accounts::fee_account_ids())
        .chain(std::iter::once(system_accounts::bridge_account_id()))
        .any(|id| id == target);
    if restricted {
        Err(format!("reward target {target} is a restricted system account"))
    } else {
        Ok(())
    }
}

/// `chain_state::apply::opening_fee_state`.
pub fn opening_fee_state(state: &V03State) -> FeeState {
    FeeState::from_bytes(
        state
            .get_account_by_id(system_accounts::fee_state_account_id())
            .data
            .actor_state(system_accounts::fee_program_id()),
    )
}

fn fee_state_actor() -> Actor {
    Actor::new(system_accounts::fee_state_account_id(), programs::fee_account_id())
}

/// `common::transaction::fee_reserve_invocation`: the payer's native balance authorized by the fee declaration.
pub fn fee_reserve_invocation(payer: AccountId, amount: u128) -> lee::public_transaction::Message {
    let inbox = Actor::native_balance(system_accounts::fee_inbox_account_id());
    lee::public_transaction::Message::try_new(
        Actor::native_balance(payer),
        vec![(Actor::native_balance(payer), Authority::Signer), (inbox, Authority::None)],
        BTreeMap::new(),
        native_token::Message::Transfer { to: inbox.account_id, amount },
    )
    .expect("Fee reserve message should always be constructable")
}

fn refund_invocation(payer: AccountId, amount: u128, payee: Option<Actor>) -> lee::public_transaction::Message {
    let fee_state = fee_state_actor();
    lee::public_transaction::Message::try_new(
        fee_state,
        [fee_state, Actor::native_balance(system_accounts::fee_inbox_account_id())]
            .into_iter()
            .chain(payee)
            .map(|actor| (actor, Authority::None)),
        BTreeMap::new(),
        fee_core::Message::Refund { amount, payer },
    )
    .expect("Fee refund message should always be constructable")
}

/// `common::transaction::fee_refund_invocation`.
pub fn fee_refund_invocation(payer: AccountId, amount: u128) -> lee::public_transaction::Message {
    refund_invocation(payer, amount, Some(Actor::native_balance(payer)))
}

/// `common::transaction::private_fee_refund_invocation`.
pub fn private_fee_refund_invocation(payer: AccountId, amount: u128) -> lee::public_transaction::Message {
    refund_invocation(payer, amount, None)
}

/// `common::transaction::native_balance_only_increased`.
fn native_balance_only_increased(pre: &lee::Account, post: &lee::Account) -> bool {
    fn non_native(account: &lee::Account) -> impl Iterator<Item = (&lee::AccountId, &lee::ActorState)> {
        account.data.actor_states.iter().filter(|(program, _)| **program != native_token::NATIVE_TOKEN_PROGRAM_ID)
    }
    matches!(
        (pre.data.native_balance(), post.data.native_balance()),
        (Ok(before), Ok(after)) if before < after
    ) && pre.nonce == post.nonce
        && non_native(pre).eq(non_native(post))
}

/// The clock/fee part of `common::transaction::validate_user_state_modification`: the in-proof fee may only raise
/// the inbox, and only for a private transaction paying inside its proof.
fn validate_no_restricted_account_modification(
    tx: &LeeTransaction,
    state: &V03State,
    diff: &ValidatedStateDiff,
) -> Result<(), String> {
    let fee_inbox = system_accounts::fee_inbox_account_id();
    for account_id in std::iter::once(system_accounts::clock_account_id()).chain(system_accounts::fee_account_ids()) {
        let in_proof_fee = account_id == fee_inbox
            && matches!(tx, LeeTransaction::PrivacyPreserving(private_tx) if private_tx.message().fee.is_none())
            && diff
                .public_diff()
                .get(&account_id)
                .is_some_and(|post| native_balance_only_increased(&state.get_account_by_id(account_id), post));
        if !in_proof_fee
            && diff.public_diff().get(&account_id).is_some_and(|post| *post != state.get_account_by_id(account_id))
        {
            return Err(format!("Transaction modifies restricted system account {account_id}"));
        }
    }
    Ok(())
}

/// `common::transaction::validate_bridge_account_modification`: a user transaction changes the bridge escrow only
/// by a public transaction's pure native-balance increase.
fn validate_bridge_account_modification(
    state: &V03State,
    diff: &ValidatedStateDiff,
    is_public_tx: bool,
) -> Result<(), String> {
    let bridge = system_accounts::bridge_account_id();
    let pre = state.get_account_by_id(bridge);
    let Some(post) = diff.public_diff().get(&bridge) else {
        return Ok(());
    };
    if !is_public_tx {
        return Err(format!("Non-public transaction cannot modify system bridge account {bridge}"));
    }
    if native_balance_only_increased(&pre, post) {
        Ok(())
    } else {
        Err(format!("Transaction modifies restricted system bridge account {bridge}"))
    }
}

/// `system_upgrader_core::registry_account_id` (lez/programs/system_upgrader/core/src/lib.rs): the upgrader's PDA of
/// the registry seed. Mirrored: the crate is not a dependency of the harness.
fn system_registry_account_id() -> AccountId {
    AccountId::for_public_pda(
        &lee_core::program::SYSTEM_UPGRADER_ACCOUNT_ID,
        &lee_core::program::PdaSeed::new(*b"/LEZ/v0.3/SystemRegistry/0000000"),
    )
}

/// `common::transaction::validate_no_system_registry_modification`: no user transaction changes `system_upgrader`'s
/// registry.
fn validate_no_system_registry_modification(state: &V03State, diff: &ValidatedStateDiff) -> Result<(), String> {
    let registry = system_registry_account_id();
    let program = lee_core::program::SYSTEM_UPGRADER_ACCOUNT_ID;
    let modified = diff.public_diff().get(&registry).is_some_and(|post| {
        state.get_account_by_id(registry).data.actor_states.get(&program) != post.data.actor_states.get(&program)
    });
    if modified {
        Err("Transaction modifies system_upgrader's registry".to_owned())
    } else {
        Ok(())
    }
}

/// `common::transaction::validate_user_state_modification` for user transactions (no system injection reaches the
/// harness's settlement, so its exceptions for the bridge's and the upgrader's own injections never apply).
fn validate_user_state_modification(
    tx: &LeeTransaction,
    state: &V03State,
    diff: &ValidatedStateDiff,
) -> Result<(), String> {
    validate_no_restricted_account_modification(tx, state, diff)?;
    validate_bridge_account_modification(state, diff, matches!(tx, LeeTransaction::Public(_)))?;
    validate_no_system_registry_modification(state, diff)
}

/// `common::transaction::validate_private_fee_credit`.
fn validate_private_fee_credit(state: &V03State, diff: &ValidatedStateDiff, reserve: Balance) -> Result<(), String> {
    let fee_inbox = system_accounts::fee_inbox_account_id();
    let balance = |account: &lee::Account| account.data.native_balance().ok();
    let pre = state.get_account_by_id(fee_inbox);
    let credited = diff
        .public_diff()
        .get(&fee_inbox)
        .and_then(balance)
        .zip(balance(&pre))
        .and_then(|(after, before)| after.checked_sub(before));
    if credited == Some(reserve) {
        Ok(())
    } else {
        Err(format!("Private transaction credits the fee inbox {credited:?} instead of its reserve {reserve}"))
    }
}

/// `chain_state::apply::settle_charged_transaction`: reserve → action → refund, for a public transaction or a
/// private one whose fee a public signer declares (LEZ #1080).
#[allow(clippy::too_many_arguments)]
pub fn settle_charged_transaction(
    transaction: &LeeTransaction,
    view: &FeeTxView,
    state: &mut V03State,
    opening: &FeeState,
    block_id: BlockId,
    timestamp: Timestamp,
    summary: &mut BlockFeeSummary,
) -> Result<(Vec<TransactionEvent>, Charge), String> {
    let FeeTxView::Public { payer, .. } = view else {
        unreachable!("only a declared fee classifies as a public fee view");
    };
    let payer = *payer;

    validate_static_tx(view, opening).map_err(|err| err.to_string())?;
    if !transaction.is_fee_authorized() {
        return Err("designated payer's authorization is missing".into());
    }

    // Phase 1: Reserve
    let reserved = fee_reserve(view, opening);
    let reserve_msg = fee_reserve_invocation(payer, reserved);
    let payer_authorized = HashSet::from([payer]);
    let reserve_diff = ValidatedStateDiff::from_fee_settlement_invocation(
        reserve_msg.execution.root.to,
        &reserve_msg.execution.root.message,
        reserve_msg.public_context.actors.clone(),
        &payer_authorized,
        state,
        block_id,
        timestamp,
    )
    .map_err(|err| format!("fee reserve failed: {err}"))?;
    state.apply_state_diff(reserve_diff).map_err(|err| format!("fee reserve failed: {err}"))?;

    // Phase 2: Action (a private transaction's gas limit also covers its proof's verification)
    let gas_limit = view.gas_limit();
    let (charge, result, verify_gas) = match transaction {
        LeeTransaction::Public(public_tx) => {
            let (charge, result) =
                ValidatedStateDiff::from_public_transaction_metered(public_tx, state, block_id, timestamp, gas_limit);
            (charge, result, 0)
        }
        LeeTransaction::PrivacyPreserving(private_tx) => {
            let budget = gas_limit.saturating_sub(market::PRIVATE_VERIFY_GAS);
            let (charge, result) = match ValidatedStateDiff::from_privacy_preserving_transaction_metered(
                private_tx, state, block_id, timestamp, budget,
            ) {
                Ok((diff, charge)) => (charge, Ok(diff)),
                Err(err) => (lee::ExecutionCharge::FREE, Err(err)),
            };
            (charge, result, market::PRIVATE_VERIFY_GAS)
        }
    };
    let action_diff = result.map_err(|err| format!("fee action failed: {err}"))?;
    // r0 cycle count can overshoot, clamp to `gas_limit`
    let charged_cycles = charge.cycles.saturating_add(verify_gas).min(gas_limit);

    summary.gas_used_exec = accumulate_exec_gas(summary.gas_used_exec, charged_cycles).map_err(gas_cap)?;
    summary.gas_used_stor = accumulate_stor_gas(summary.gas_used_stor, view.gas_stor()).map_err(gas_cap)?;

    validate_user_state_modification(transaction, state, &action_diff)?;
    let action_events = state.apply_state_diff(action_diff).map_err(|err| format!("fee action failed: {err}"))?;

    // Phase 3: Refund
    let fee_base = fee_actual_base(charged_cycles, view, opening);
    let fee_total = fee_base.checked_add(u128::from(view.tip())).expect("fee_base + tip fits u128");
    let refund = reserved.checked_sub(fee_total).expect("the reserve prices gas_limit, which bounds the actual fee");
    if refund > 0 {
        let refund_msg = fee_refund_invocation(payer, refund);
        let refund_diff = ValidatedStateDiff::from_fee_settlement_invocation(
            refund_msg.execution.root.to,
            &refund_msg.execution.root.message,
            refund_msg.public_context.actors.clone(),
            &HashSet::new(),
            state,
            block_id,
            timestamp,
        )
        .map_err(|err| format!("fee refund failed: {err}"))?;
        state.apply_state_diff(refund_diff).map_err(|err| format!("fee refund failed: {err}"))?;
    }

    summary.revenue_base = summary.revenue_base.checked_add(fee_base).expect("block revenue fits u128");
    summary.revenue_tip = summary.revenue_tip.checked_add(u128::from(view.tip())).expect("block tips fit u128");
    let receipt = Receipt {
        success: charge.revert.is_none(),
        gas_exec: charged_cycles,
        gas_stor: view.gas_stor(),
        fee_base,
        tip: u128::from(view.tip()),
        refund,
    };
    Ok((
        action_events,
        Charge {
            gas_exec: charged_cycles,
            cycles: charge.cycles,
            gas_stor: view.gas_stor(),
            fee_base,
            revert: charge.revert,
            receipt,
        },
    ))
}

/// `chain_state::apply::settle_private_transaction`: against the reserve the proof paid; the surplus above
/// `market::MIN_PRIVATE_REFUND` is refunded, else kept as tip.
#[allow(clippy::too_many_arguments)]
pub fn settle_private_transaction(
    transaction: &LeeTransaction,
    view: &FeeTxView,
    state: &mut V03State,
    opening: &FeeState,
    block_id: BlockId,
    timestamp: Timestamp,
    summary: &mut BlockFeeSummary,
) -> Result<(Vec<TransactionEvent>, Charge), String> {
    let (
        LeeTransaction::PrivacyPreserving(private_tx),
        FeeTxView::Private { payer, reserve, data_bytes, tip: proven_tip },
    ) = (transaction, view)
    else {
        unreachable!("only private transactions classify as a private fee view");
    };

    validate_static_tx(view, opening).map_err(|err| err.to_string())?;

    // The verification cost is charged up front; storage gas is the transaction's charged bytes.
    let gas_used_exec = accumulate_exec_gas(summary.gas_used_exec, market::PRIVATE_VERIFY_GAS).map_err(gas_cap)?;
    let gas_used_stor = accumulate_stor_gas(summary.gas_used_stor, *data_bytes).map_err(gas_cap)?;
    let proven_tip = u128::from(*proven_tip);
    let cycle_budget = market::MAX_GAS_EXEC.checked_sub(gas_used_exec).expect("the accumulator is cap-checked").min(
        private_cycles_affordable(
            reserve.saturating_sub(proven_tip),
            *data_bytes,
            opening.base_fee_exec,
            opening.base_fee_stor,
        ),
    );
    let (diff, charge) = ValidatedStateDiff::from_privacy_preserving_transaction_metered(
        private_tx,
        state,
        block_id,
        timestamp,
        cycle_budget,
    )
    .map_err(|err| format!("{err:#}"))?;
    let charged_cycles = charge.cycles.min(cycle_budget);
    let gas_used_exec = accumulate_exec_gas(gas_used_exec, charged_cycles).map_err(gas_cap)?;

    let actual = private_fee_required(charged_cycles, *data_bytes, opening.base_fee_exec, opening.base_fee_stor);
    let required = actual.checked_add(proven_tip).expect("a fee and a u64 tip fit u128");
    let surplus = reserve
        .checked_sub(required)
        .ok_or_else(|| FeeError::PrivateFeeBelowRequired { required, reserve: *reserve }.to_string())?;

    validate_user_state_modification(transaction, state, &diff)
        .and_then(|()| validate_private_fee_credit(state, &diff, *reserve))?;
    let events = state.apply_state_diff(diff).map_err(|err| format!("private action failed: {err}"))?;

    let refund_to = private_tx.message().execution.proven_fee.as_ref().and_then(|fee| fee.refund_to);
    let refundable = refund_to.is_some() || state.recovery_binding(*payer).is_some();
    let (refund, tip) = if surplus > market::MIN_PRIVATE_REFUND && refundable {
        (surplus, proven_tip)
    } else {
        (0, proven_tip.checked_add(surplus).expect("the tip and surplus fit in the reserve"))
    };
    if refund > 0 {
        let refund_msg = refund_to.map_or_else(
            || private_fee_refund_invocation(*payer, refund),
            |target| fee_refund_invocation(target, refund),
        );
        let refund_diff = ValidatedStateDiff::from_fee_settlement_invocation(
            refund_msg.execution.root.to,
            &refund_msg.execution.root.message,
            refund_msg.public_context.actors.clone(),
            &HashSet::new(),
            state,
            block_id,
            timestamp,
        )
        .map_err(|err| format!("private fee refund failed: {err}"))?;
        state.apply_state_diff(refund_diff).map_err(|err| format!("private fee refund failed: {err}"))?;
    }

    summary.gas_used_exec = gas_used_exec;
    summary.gas_used_stor = gas_used_stor;
    summary.revenue_base = summary.revenue_base.checked_add(actual).expect("block revenue fits u128");
    summary.revenue_tip = summary.revenue_tip.checked_add(tip).expect("block tips fit u128");
    let gas_exec = market::PRIVATE_VERIFY_GAS.checked_add(charged_cycles).expect("the accumulator is cap-checked");
    let receipt =
        Receipt { success: charge.revert.is_none(), gas_exec, gas_stor: *data_bytes, fee_base: actual, tip, refund };
    Ok((
        events,
        Charge {
            gas_exec,
            cycles: charge.cycles,
            gas_stor: *data_bytes,
            fee_base: actual,
            revert: charge.revert,
            receipt,
        },
    ))
}

fn gas_cap(err: FeeError) -> String {
    format!("gas cap exceeded: {err}")
}
