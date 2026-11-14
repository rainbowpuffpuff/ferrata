//! LEZ v0.4.0-rc1's per-transaction fee settlement, as the sequencer and the indexer run it.
//!
//! Copied from logos-execution-zone v0.4.0-rc1 (commit 1806e873, MIT or Apache-2.0):
//! `lez/chain_state/src/apply.rs` (`settle_charged_transaction`, `settle_private_transaction`),
//! `lez/chain_state/src/classify.rs` (`classify`, `private_fee_reserve`) and
//! `lez/common/src/transaction.rs` (`LeeTransaction`, `fee_reserve_invocation`,
//! `fee_refund_invocation`, `private_fee_refund_invocation`, `validate_private_fee_credit`,
//! the restricted-account guard). `chain_state` and `common` themselves are not used because
//! they pull in the Bedrock node (libp2p, RocksDB, circuit builds); every call into `lee`, the
//! state machine, is the same.
//!
//! Changes from the original, none of which changes what is charged:
//! - errors are strings instead of `BlockIngestError`;
//! - the functions also return the charge (cycles, storage gas, fee) for the measurements;
//! - `classify` keeps only the user-transaction paths (no system injections, cross-zone locks or
//!   sequencer-stake operations, which these tests never send);
//! - the bridge and cross-zone-inbox guards of `validate_user_state_modification` are left out
//!   (they reject writes to accounts these tests never touch); the clock/fee guard is kept.
//!
//! Against v0.4.0-rc2 and rc3 the copied functions are unchanged, except that rc3 has one clock
//! account instead of three (LEZ #988): the guard takes the tag's (feature `lez-rc3`).

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
    execution_state::{BoundaryStep, Delivery},
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

/// What settlement charged one transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Charge {
    /// Execution gas: the metered cycles, clamped to the gas limit (public) or to the budget the
    /// reserve pays for (private), plus `PRIVATE_VERIFY_GAS` for a private transaction.
    pub gas_exec: u64,
    /// The metered cycles of the action as `lee` reports them (before clamping).
    pub cycles: u64,
    /// Storage gas: the transaction's borsh size (public) or `PRIVATE_GAS_STOR` (private).
    pub gas_stor: u64,
    /// The base fee charged, at the block's opening base fees.
    pub fee_base: Balance,
    /// Why the action reverted, if it did (the fee is still charged).
    pub revert: Option<String>,
}

/// `chain_state::classify::FeeClass` for the transactions these tests send.
pub fn classify(tx: &LeeTransaction) -> Result<FeeTxView, String> {
    match tx {
        LeeTransaction::PrivacyPreserving(private_tx) => {
            let execution = &private_tx.message().execution;
            let Some((payer, reserve, crossing)) = private_fee_reserve(private_tx.message()) else {
                return Err("private transaction omits its required in-proof fee".into());
            };
            if let Some(fallback) = execution.fee.as_ref().and_then(|fee| fee.fallback.as_ref()) {
                if fallback.boundary
                    != [BoundaryStep::PrivateToPublic(crossing.clone()), BoundaryStep::EndPublicSubtree]
                {
                    return Err("private fee fallback does not replay the fee transfer alone".into());
                }
            }
            let count = execution.private_actions.len();
            if count > market::MAX_PRIVATE_ACTIONS {
                return Err(format!("{count} private actions"));
            }
            Ok(FeeTxView::Private { payer, reserve })
        }
        LeeTransaction::Public(public_tx) => {
            let Some(fee) = public_tx.message().execution.fee else {
                return Err("user public transaction omits its required fee declaration".into());
            };
            let data_bytes =
                u64::try_from(borsh::to_vec(tx).map_err(|e| e.to_string())?.len()).expect("tx size fits u64");
            Ok(FeeTxView::Public {
                payer: fee.payer,
                gas_limit: fee.gas_limit,
                data_bytes,
                tip: fee.tip,
                max_fee: fee.max_fee,
            })
        }
    }
}

fn private_fee_reserve(message: &PrivateMessage) -> Option<(AccountId, Balance, &Delivery<Actor>)> {
    let inbox = Actor::native_balance(system_accounts::fee_inbox_account_id());
    let mut crossings = message.execution.boundary.iter().filter_map(|step| match step {
        BoundaryStep::PrivateToPublic(delivery) if delivery.envelope.to == inbox => Some(delivery),
        _ => None,
    });
    let delivery = crossings.next()?;
    if crossings.next().is_some() {
        return None;
    }
    match borsh::from_slice(&delivery.envelope.message).ok()? {
        native_token::Message::Credit(reserve) => Some((delivery.envelope.from.account_id, reserve, delivery)),
        native_token::Message::Transfer { .. } | native_token::Message::ReadState => None,
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

/// `common::transaction::fee_reserve_invocation`.
pub fn fee_reserve_invocation(payer: AccountId, amount: u128) -> lee::public_transaction::Message {
    let inbox = Actor::native_balance(system_accounts::fee_inbox_account_id());
    lee::public_transaction::Message::try_new(
        Actor::native_balance(payer),
        vec![Actor::native_balance(payer), inbox],
        BTreeMap::new(),
        native_token::Message::Transfer { to: inbox.account_id, amount },
    )
    .expect("Fee reserve message should always be constructable")
}

fn refund_invocation(payer: AccountId, amount: u128, payee: Option<Actor>) -> lee::public_transaction::Message {
    let fee_state = fee_state_actor();
    lee::public_transaction::Message::try_new(
        fee_state,
        [fee_state, Actor::native_balance(system_accounts::fee_inbox_account_id())].into_iter().chain(payee),
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

/// The clock accounts: three on v0.4.0-rc1 and rc2 (every 1, 10 and 50 blocks), one from v0.4.0-rc3 on
/// (LEZ #988).
#[cfg(not(feature = "lez-rc3"))]
fn clock_account_ids() -> impl Iterator<Item = AccountId> {
    system_accounts::clock_account_ids().into_iter()
}

/// The clock account (v0.4.0-rc3: one, read through the clock's actor, LEZ #988).
#[cfg(feature = "lez-rc3")]
fn clock_account_ids() -> impl Iterator<Item = AccountId> {
    std::iter::once(system_accounts::clock_account_id())
}

/// The clock/fee part of `common::transaction::validate_user_state_modification`.
fn validate_no_restricted_account_modification(
    tx: &LeeTransaction,
    state: &V03State,
    diff: &ValidatedStateDiff,
) -> Result<(), String> {
    let fee_inbox = system_accounts::fee_inbox_account_id();
    for account_id in clock_account_ids().chain(system_accounts::fee_account_ids()) {
        let in_proof_fee = account_id == fee_inbox
            && matches!(tx, LeeTransaction::PrivacyPreserving(_))
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

/// `chain_state::apply::settle_charged_transaction`: reserve → action → refund.
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
    let (LeeTransaction::Public(public_tx), FeeTxView::Public { payer, .. }) = (transaction, view) else {
        unreachable!("only public transactions classify as a public fee view");
    };
    let payer = *payer;

    validate_static_tx(view, opening).map_err(|err| err.to_string())?;
    if !lee::is_fee_authorized(public_tx.message(), public_tx.witness_set()) {
        return Err("designated payer's authorization is missing".into());
    }

    // Phase 1: Reserve
    let reserved = fee_reserve(view, opening);
    let reserve_msg = fee_reserve_invocation(payer, reserved);
    let payer_authorized = HashSet::from([payer]);
    let reserve_diff = ValidatedStateDiff::from_fee_settlement_invocation(
        reserve_msg.execution.root.to,
        &reserve_msg.execution.root.message,
        &reserve_msg.public_context.actors,
        &payer_authorized,
        state,
        block_id,
        timestamp,
    )
    .map_err(|err| format!("fee reserve failed: {err}"))?;
    state.apply_state_diff(reserve_diff).map_err(|err| format!("fee reserve failed: {err}"))?;

    // Phase 2: Action
    let gas_limit = view.gas_limit();
    let (charge, result) =
        ValidatedStateDiff::from_public_transaction_metered(public_tx, state, block_id, timestamp, gas_limit);
    let action_diff = result.map_err(|err| format!("fee action failed: {err}"))?;
    // r0 cycle count can overshoot, clamp to `gas_limit`
    let charged_cycles = charge.cycles.min(gas_limit);

    summary.gas_used_exec = accumulate_exec_gas(summary.gas_used_exec, charged_cycles).map_err(gas_cap)?;
    summary.gas_used_stor = accumulate_stor_gas(summary.gas_used_stor, view.gas_stor()).map_err(gas_cap)?;

    validate_no_restricted_account_modification(transaction, state, &action_diff)?;
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
            &refund_msg.public_context.actors,
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
    Ok((
        action_events,
        Charge {
            gas_exec: charged_cycles,
            cycles: charge.cycles,
            gas_stor: view.gas_stor(),
            fee_base,
            revert: charge.revert,
        },
    ))
}

/// `chain_state::apply::settle_private_transaction`.
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
    let (LeeTransaction::PrivacyPreserving(private_tx), FeeTxView::Private { payer, reserve }) = (transaction, view)
    else {
        unreachable!("only private transactions classify as a private fee view");
    };

    validate_static_tx(view, opening).map_err(|err| err.to_string())?;

    // The verification cost is charged up front.
    let gas_used_exec = accumulate_exec_gas(summary.gas_used_exec, market::PRIVATE_VERIFY_GAS).map_err(gas_cap)?;
    let gas_used_stor = accumulate_stor_gas(summary.gas_used_stor, market::PRIVATE_GAS_STOR).map_err(gas_cap)?;
    let cycle_budget = market::MAX_GAS_EXEC
        .checked_sub(gas_used_exec)
        .expect("the accumulator is cap-checked")
        .min(private_cycles_affordable(*reserve, opening.base_fee_exec, opening.base_fee_stor));
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

    let actual = private_fee_required(charged_cycles, opening.base_fee_exec, opening.base_fee_stor);
    let surplus = reserve
        .checked_sub(actual)
        .ok_or_else(|| FeeError::PrivateFeeBelowRequired { required: actual, reserve: *reserve }.to_string())?;

    validate_no_restricted_account_modification(transaction, state, &diff)
        .and_then(|()| validate_private_fee_credit(state, &diff, *reserve))?;
    let events = state.apply_state_diff(diff).map_err(|err| format!("private action failed: {err}"))?;

    let refund_to = private_tx.message().execution.fee.as_ref().and_then(|fee| fee.refund_to);
    let refundable = refund_to.is_some() || state.recovery_binding(*payer).is_some();
    let (refund, tip) = if surplus > market::MIN_PRIVATE_REFUND && refundable { (surplus, 0) } else { (0, surplus) };
    if refund > 0 {
        let refund_msg = refund_to.map_or_else(
            || private_fee_refund_invocation(*payer, refund),
            |target| fee_refund_invocation(target, refund),
        );
        let refund_diff = ValidatedStateDiff::from_fee_settlement_invocation(
            refund_msg.execution.root.to,
            &refund_msg.execution.root.message,
            &refund_msg.public_context.actors,
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
    Ok((
        events,
        Charge {
            gas_exec: market::PRIVATE_VERIFY_GAS + charged_cycles,
            cycles: charge.cycles,
            gas_stor: market::PRIVATE_GAS_STOR,
            fee_base: actual,
            revert: charge.revert,
        },
    ))
}

fn gas_cap(err: FeeError) -> String {
    format!("gas cap exceeded: {err}")
}
