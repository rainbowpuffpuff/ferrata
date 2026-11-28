//! LEZ v0.4.0 user-state guards as the harness applies them to settled user transactions (feature `lez-040`).
//! Each case has its own chain. The public cases sign and settle directly because `Chain::submit` expects acceptance;
//! the private bridge case needs dev mode. On the release candidates this file is empty.
//! The harness's genesis does not hold the clock, the bridge escrow or the registry account, and v0.4.0 refuses an
//! action on an account that is neither in the state nor shown public by admission evidence (an unadmitted actor,
//! displayed as "Program violated execution rules"), so the recipient is put in the state first (`admit`).
#![cfg(feature = "lez-040")]

use std::collections::BTreeMap;

use fee_core::{assess::fee_reserve, market, BlockFeeSummary};
use harness::{
    compat,
    settle::{classify, opening_fee_state, settle_charged_transaction, Charge, LeeTransaction},
    Chain,
};
use lee::{
    privacy_preserving_transaction::circuit::ProgramCatalog, public_transaction::Message as PublicMessage, Account,
    AccountId, Actor, PublicTransaction,
};
use lee_core::{native_token, program::PdaSeed};

const SENDER_BALANCE: u128 = 1 << 100;
const TRANSFER_AMOUNT: u128 = 1_000;
const PRIVATE_RESERVE: u128 = 100_000_000;

fn require_dev_mode() {
    assert_eq!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1"), "run these tests with RISC0_DEV_MODE=1");
}

/// Puts `account` in the state as an empty public account when the harness's genesis does not hold it (an account the
/// state lacks reads as the default account): LEZ v0.4.0 refuses an action whose actor's account is neither in the
/// state nor shown public by the transaction's admission evidence.
fn admit(chain: &mut Chain, account: AccountId) {
    if chain.state.get_account_by_id(account) == Account::default() {
        chain.state = std::mem::take(&mut chain.state).with_public_accounts([(account, Account::default())]);
    }
}

/// A funded public signer transferring native balance to `recipient`. This follows `Chain::submit_signed`, but keeps
/// settlement separate so a guard's `Err` can be inspected instead of becoming that method's panic.
fn public_transfer_to(recipient: AccountId) -> (Chain, AccountId, PublicTransaction) {
    let mut chain = Chain::new();
    let key = compat::account_key([0x61; 32]);
    let sender = compat::account_of(&key);
    chain.state = std::mem::take(&mut chain.state).with_public_accounts([(sender, Account::funded(SENDER_BALANCE))]);
    admit(&mut chain, recipient);

    let root = Actor::native_balance(sender);
    let nonces = BTreeMap::from([(sender, chain.state.get_account_by_id(sender).nonce)]);
    let actors = compat::declared([root, Actor::native_balance(recipient)], [sender]);
    let fee = lee::FeeDeclaration::new(sender, market::MAX_GAS_EXEC, 0, u128::MAX);
    let message = PublicMessage::new(
        root,
        borsh::to_vec(&native_token::Message::Transfer { to: recipient, amount: TRANSFER_AMOUNT }).unwrap(),
        actors,
        nonces,
        Some(fee),
        Vec::new(),
    );
    let witnesses = compat::sign(&message, &[&key]);
    (chain, sender, PublicTransaction::new(message, witnesses))
}

/// The reserve settlement takes before the action, and the settlement's result.
fn settle_public(chain: &mut Chain, transaction: PublicTransaction) -> (u128, Result<Charge, String>) {
    let transaction = LeeTransaction::Public(transaction);
    let view = classify(&transaction).expect("the signed transfer classifies as a public-fee transaction");
    let opening = opening_fee_state(&chain.state);
    let reserved = fee_reserve(&view, &opening);
    let result = settle_charged_transaction(
        &transaction,
        &view,
        &mut chain.state,
        &opening,
        chain.block_id,
        0,
        &mut BlockFeeSummary::default(),
    )
    .map(|(_, charge)| charge);
    (reserved, result)
}

fn balance(chain: &Chain, account: AccountId) -> u128 {
    chain.state.get_account_by_id(account).data.native_balance().expect("a native balance actor")
}

/// The guard refuses the transfer's whole action diff: the exact error, the sender's nonce not advanced and the
/// restricted recipient untouched. In LEZ such a refusal makes the whole block invalid (`BlockIngestError`,
/// lez/chain_state/src/apply.rs, `settle_charged_transaction`): a correct sequencer never includes the transaction. The
/// harness returns the error and keeps the reserve it took before the action in its state; that is not LEZ behaviour
/// and is not asserted here.
fn assert_restricted_public_transfer_refused(recipient: AccountId) {
    let (mut chain, sender, transaction) = public_transfer_to(recipient);
    let sender_nonce_before = chain.state.get_account_by_id(sender).nonce;
    let recipient_before = chain.state.get_account_by_id(recipient).clone();

    let (_, result) = settle_public(&mut chain, transaction);
    let error = result.expect_err("the restricted-account guard refuses the transfer");
    assert_eq!(error, format!("Transaction modifies restricted system account {recipient}"));
    assert_eq!(
        chain.state.get_account_by_id(sender).nonce,
        sender_nonce_before,
        "the refused action does not advance the nonce"
    );
    // The fee inbox also received the harness's reserve (above), so only the other accounts are compared whole.
    if recipient != system_accounts::fee_inbox_account_id() {
        assert_eq!(chain.state.get_account_by_id(recipient), recipient_before, "the restricted recipient is untouched");
    }
}

#[test]
fn a_public_transfer_to_the_clock_is_refused() {
    assert_restricted_public_transfer_refused(system_accounts::clock_account_id());
}

#[test]
fn a_public_transfer_to_each_fee_account_is_refused_including_the_fee_inbox() {
    for recipient in system_accounts::fee_account_ids() {
        // A fresh chain for every fee account: in particular, the inbox starts without another case's reserve.
        assert_restricted_public_transfer_refused(recipient);
    }
}

#[test]
fn a_public_native_balance_increase_of_the_bridge_escrow_is_accepted() {
    let bridge = system_accounts::bridge_account_id();
    let (mut chain, _, transaction) = public_transfer_to(bridge);
    let bridge_before = balance(&chain, bridge);
    let (_, result) = settle_public(&mut chain, transaction);
    let charge = result.expect("a pure public native-balance increase is the bridge guard's allowed shape");
    assert!(charge.receipt.success, "the native transfer itself applied: {:?}", charge.revert);
    assert_eq!(balance(&chain, bridge), bridge_before + TRANSFER_AMOUNT);
}

#[test]
fn a_private_native_balance_increase_of_the_bridge_escrow_is_refused() {
    require_dev_mode();
    let mut chain = Chain::new();
    let sender = chain.add_private_account(0x71, SENDER_BALANCE);
    let bridge = system_accounts::bridge_account_id();
    admit(&mut chain, bridge);
    let bridge_before = chain.state.get_account_by_id(bridge).clone();
    let inbox_before = chain.state.get_account_by_id(system_accounts::fee_inbox_account_id()).clone();
    let message = native_token::Message::Transfer { to: bridge, amount: TRANSFER_AMOUNT };
    let result = chain.prove_and_settle_private(
        &sender,
        Actor::native_balance(sender.id()),
        borsh::to_vec(&message).unwrap(),
        vec![Actor::native_balance(bridge)],
        &ProgramCatalog::default(),
        PRIVATE_RESERVE,
        0,
    );
    let error = result.map(|_| ()).expect_err("even a pure private increase of the bridge is forbidden");
    assert_eq!(error, format!("Non-public transaction cannot modify system bridge account {bridge}"));
    assert_eq!(chain.state.get_account_by_id(bridge), bridge_before, "the refused bridge diff is not applied");
    assert_eq!(
        chain.state.get_account_by_id(system_accounts::fee_inbox_account_id()),
        inbox_before,
        "the rejected private diff, including its in-proof fee credit, is not applied"
    );
}

#[test]
fn a_public_native_balance_increase_does_not_modify_the_system_registry_actor_state() {
    // `system_upgrader_core` is not a direct harness dependency (and `programs` does not re-export this helper), so
    // derive the expected ID from the exact seed used by upstream `registry_account_id` rather than naming that crate.
    const REGISTRY_SEED: [u8; 32] = *b"/LEZ/v0.3/SystemRegistry/0000000";
    let registry =
        AccountId::for_public_pda(&lee_core::program::SYSTEM_UPGRADER_ACCOUNT_ID, &PdaSeed::new(REGISTRY_SEED));

    let (mut chain, _, transaction) = public_transfer_to(registry);
    let upgrader_state_before = chain
        .state
        .get_account_by_id(registry)
        .data
        .actor_states
        .get(&lee_core::program::SYSTEM_UPGRADER_ACCOUNT_ID)
        .cloned();
    let registry_balance_before = balance(&chain, registry);
    let (_, result) = settle_public(&mut chain, transaction);
    let charge = result.expect("the registry guard compares only the system-upgrader actor state, not native balance");
    assert!(charge.receipt.success, "the native transfer itself applied: {:?}", charge.revert);
    assert_eq!(balance(&chain, registry), registry_balance_before + TRANSFER_AMOUNT);
    assert_eq!(
        chain.state.get_account_by_id(registry).data.actor_states.get(&lee_core::program::SYSTEM_UPGRADER_ACCOUNT_ID),
        upgrader_state_before.as_ref(),
        "only the native actor changed; the upgrader's registry actor state did not"
    );
}
