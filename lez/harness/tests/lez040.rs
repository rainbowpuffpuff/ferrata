//! What LEZ v0.4.0 changed for Ferrata's programs, held on v0.4.0 (feature `lez-040`; `lez/scripts/lez-select.sh
//! v0.4.0`). Dev mode (`RISC0_DEV_MODE=1`) for the private transaction. On the release candidates this file is empty.
//!
//! - receipts (LEZ #1107): every settled transaction has a receipt that records what settlement charged, also for a
//!   transaction whose action reverted (it still pays);
//! - fees by size (LEZ #1044): storage gas is the transaction's charged bytes; a private transaction's proof is counted
//!   at `market::PROOF_BYTES`, whatever the proof's own size (a dev-mode receipt is shorter);
//! - a public payer for a private transaction (LEZ #1080): its declaration is authorized by the payer's signature and
//!   settled on the public fee view, including proof-verification gas, while the in-proof-fee path remains private;
//! - authority (LEZ #1144): a private root call is authorized only when its witness names the root's program
//!   (`compat::authorize_root`), and the bridge's cosigners' chain passes each next signer's grant along;
//! - system programs (LEZ #946): the fee program the settlement calls lives at the `system_upgrader` PDA of its name.
#![cfg(feature = "lez-040")]

use fee_core::{
    assess::{fee_actual_base, fee_reserve, FeeTxView},
    market, BlockFeeSummary,
};
use harness::{
    attest::Stage4,
    private::*,
    scenarios,
    settle::{classify, opening_fee_state, settle_charged_transaction, LeeTransaction},
    Chain,
};
use lee::{
    privacy_preserving_transaction::{circuit::ProgramCatalog, WitnessSet},
    test_utils::TestSigner,
    Account, AccountId, Actor, FeeDeclaration, PrivacyPreservingTransaction,
};
use lee_core::{encryption::EncryptionScheme, native_token, SharedSecretKey};

fn require_dev_mode() {
    assert_eq!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1"), "run these tests with RISC0_DEV_MODE=1");
}

#[test]
fn every_receipt_records_what_settlement_charged() {
    let world = scenarios::replay(2);
    assert!(!world.chain.reports.is_empty());
    for r in &world.chain.reports {
        let receipt = r.charge.receipt;
        assert!(receipt.success, "{}: applied", r.label);
        assert_eq!(receipt.success, r.ok(), "{}", r.label);
        assert_eq!(
            (receipt.gas_exec, receipt.gas_stor, receipt.fee_base),
            (r.charge.gas_exec, r.charge.gas_stor, r.charge.fee_base),
            "{}",
            r.label
        );
        assert_eq!(receipt.tip, 0, "{}: the harness declares no tip", r.label);
        assert!(receipt.fee_base > 0, "{}: a charged transaction pays a base fee", r.label);
        // The reserve priced the whole gas limit (`market::MAX_GAS_EXEC`); a transaction that used less is refunded.
        assert!(receipt.gas_exec < market::MAX_GAS_EXEC && receipt.refund > 0, "{}: refunded", r.label);
    }
    // A public transaction's storage gas is its serialized size: the last transaction, as it was serialized.
    let last = world.chain.reports.last().unwrap();
    assert_eq!(last.charge.receipt.gas_stor as usize, world.chain.last_tx.len(), "storage gas by size");
}

#[test]
fn a_refused_transaction_has_a_failed_receipt_and_still_pays() {
    // The negative cases of Stage 1 (some of them controls that pass): a refusal reverts, and its receipt says so
    // while still charging the fee.
    let refusals = scenarios::refusals();
    let reverted: Vec<_> = refusals.iter().filter(|r| !r.report.ok()).collect();
    assert!(reverted.len() >= 5, "the refusals revert: {} of {}", reverted.len(), refusals.len());
    for r in &refusals {
        let receipt = r.report.charge.receipt;
        assert_eq!(receipt.success, r.report.ok(), "{}: the receipt's success is the settlement's", r.case);
        assert!(receipt.fee_base > 0, "{}: the fee is charged whether or not the action applied", r.case);
        assert_eq!(receipt.gas_exec, r.report.charge.gas_exec, "{}", r.case);
    }
}

#[test]
fn a_private_transaction_pays_storage_for_its_size_with_the_proof_at_proof_bytes() {
    require_dev_mode();
    let mut s = Stage4::new();
    let slot = s.fx.finalized_slot;
    let t = s.fx.transfer.clone();
    let pool = s.pool();
    let header = s.header(slot);
    let account = s.w.chain.add_private_account(0x81, 1_000_000_000_000);
    let message = s.private_message(&pool, slot, &t);
    // The consumer's actor on the private account: the witness names the consumer's program (`authorize_root`), so
    // the root call holds the grant over its receiver.
    let root = Actor::new(account.id(), harness::consumer_account());
    let run =
        s.w.chain
            .prove_and_settle_private(
                &account,
                root,
                borsh::to_vec(&message).unwrap(),
                vec![header],
                &catalog(),
                100_000_000,
                s.time,
            )
            .expect("settles");
    assert!(run.charge.revert.is_none(), "the private attestation applies: {:?}", run.charge.revert);
    assert!(run.charge.receipt.success);
    let tx = LeeTransaction::PrivacyPreserving(run.tx.clone());
    let charged = tx.charged_bytes().unwrap();
    assert_eq!(run.charge.gas_stor, charged, "storage gas is the charged bytes");
    assert_eq!(run.charge.receipt.gas_stor, charged);
    // The dev-mode receipt is shorter than a real proof; it is charged as one of `PROOF_BYTES`.
    assert!((run.proof_bytes as u64) < market::PROOF_BYTES, "a dev-mode proof: {} bytes", run.proof_bytes);
    assert_eq!(charged, run.tx_bytes as u64 - run.proof_bytes as u64 + market::PROOF_BYTES);
    // Execution gas: the verification, then the metered cycles; the surplus of the reserve is refunded or kept as tip.
    assert!(run.charge.gas_exec > market::PRIVATE_VERIFY_GAS);
    let r = run.charge.receipt;
    assert!(r.refund > 0 || r.tip > 0, "a surplus over the fee: {r:?}");
}

#[test]
fn the_fee_program_is_the_system_upgraders_pda_of_its_name() {
    // LEZ #946: every system program's address is a PDA of `system_upgrader`; the settlement's invocations go there.
    let fee = programs::fee_account_id();
    let by_name = lee::AccountId::from_system_program_name(&fee_core::FEE_NAME);
    assert_eq!(fee, by_name);
    assert_eq!(system_accounts::fee_program_id(), fee);
}

const TRANSFER_AMOUNT: u128 = 1_000;
const SENDER_BALANCE: u128 = 1_000_000_000;
const PUBLIC_PAYER_BALANCE: u128 = 1_000_000_000;
const TIP: u64 = 123_456;

struct DeclaredTransfer {
    chain: Chain,
    sender: PrivateAccount,
    payer: TestSigner,
    recipient: AccountId,
    tx: PrivacyPreservingTransaction,
}

/// One private native transfer to a public recipient, paid by a public signer unrelated to the sender. No global
/// fixture: every test proves against its own chain.
fn declared_transfer(tip: u64) -> DeclaredTransfer {
    let mut chain = Chain::new();
    let payer = TestSigner::new(0x72);
    let recipient = AccountId::new([0x73; 32]);
    chain.state = std::mem::take(&mut chain.state).with_public_accounts([
        (payer.account_id(), Account::funded(PUBLIC_PAYER_BALANCE)),
        (recipient, Account::default()),
    ]);
    let sender = chain.add_private_account(0x71, SENDER_BALANCE);
    let root = Actor::native_balance(sender.id());
    let message = native_token::Message::Transfer { to: recipient, amount: TRANSFER_AMOUNT };
    let fee = FeeDeclaration::new(payer.account_id(), market::MAX_GAS_EXEC, tip, u128::MAX);
    let tx = chain
        .prove_private_with_declared_fee(
            &sender,
            root,
            borsh::to_vec(&message).unwrap(),
            vec![Actor::native_balance(recipient)],
            &ProgramCatalog::default(),
            &payer,
            fee,
        )
        .expect("the declared-fee transaction proves");
    DeclaredTransfer { chain, sender, payer, recipient, tx }
}

fn decrypted_sender_balance(tx: &PrivacyPreservingTransaction, sender: &PrivateAccount) -> u128 {
    let action = tx.message.execution.private_actions.first().expect("the sender's post-state note");
    assert_eq!(tx.message.execution.private_actions.len(), 1, "a bare transfer has no dummy actions");
    let shared = SharedSecretKey::decapsulate(&action.encrypted_post_state.epk, &sender.keys.d, &sender.keys.z)
        .expect("the sender can decapsulate its note");
    let (_, account) = EncryptionScheme::decrypt(&action.encrypted_post_state.ciphertext, &shared, &action.nullifier)
        .expect("the sender can decrypt its post-state");
    account.data.native_balance().expect("the private account's native balance")
}

/// The existing in-proof-fee path, with the same native action as `declared_transfer`.
fn in_proof_transfer() -> (Chain, PrivateAccount, PrivateRun) {
    let mut chain = Chain::new();
    let recipient = AccountId::new([0x73; 32]);
    chain.state = std::mem::take(&mut chain.state).with_public_accounts([(recipient, Account::default())]);
    let sender = chain.add_private_account(0x71, SENDER_BALANCE);
    let root = Actor::native_balance(sender.id());
    let message = native_token::Message::Transfer { to: recipient, amount: TRANSFER_AMOUNT };
    let run = chain
        .prove_and_settle_private(
            &sender,
            root,
            borsh::to_vec(&message).unwrap(),
            vec![Actor::native_balance(recipient)],
            &ProgramCatalog::default(),
            100_000_000,
            0,
        )
        .expect("the in-proof-fee control settles");
    (chain, sender, run)
}

#[test]
fn a_private_transaction_with_a_public_payer_settles_and_charges_only_that_payer() {
    require_dev_mode();
    let DeclaredTransfer { mut chain, sender, payer, recipient, tx } = declared_transfer(TIP);
    assert!(tx.message.execution.proven_fee.is_none(), "the declared fee has no in-proof twin");
    assert_eq!(tx.message.nonces.len(), 1, "only the public payer signs");
    assert_eq!(tx.witness_set.entries().len(), 1, "the payer's signature is carried by the witness set");
    let lee_tx = LeeTransaction::PrivacyPreserving(tx);
    let view = classify(&lee_tx).expect("a declared fee without a proven fee classifies");
    let FeeTxView::Public { payer: fee_payer, gas_limit, tip: declared_tip, .. } = view else {
        panic!("a public signer must produce the public fee view: {view:?}");
    };
    assert_eq!(fee_payer, payer.account_id());
    assert_eq!((gas_limit, declared_tip), (market::MAX_GAS_EXEC, TIP));

    let opening = opening_fee_state(&chain.state);
    let reserve = fee_reserve(&view, &opening);
    let payer_before = chain.state.get_account_by_id(payer.account_id()).data.native_balance().unwrap();
    let mut summary = BlockFeeSummary::default();
    let (_, charge) =
        settle_charged_transaction(&lee_tx, &view, &mut chain.state, &opening, chain.block_id, 0, &mut summary)
            .expect("the publicly paid private transaction settles");
    let receipt = charge.receipt;
    assert!(receipt.success, "the private transfer applies: {:?}", charge.revert);
    assert_eq!(receipt.gas_exec, charge.cycles.saturating_add(market::PRIVATE_VERIFY_GAS).min(gas_limit));
    assert!(receipt.gas_exec >= market::PRIVATE_VERIFY_GAS, "verification is included in execution gas");
    assert_eq!(receipt.gas_stor, lee_tx.charged_bytes().unwrap());
    assert_eq!(receipt.tip, u128::from(TIP), "the declared tip is charged");
    assert_eq!(receipt.fee_base, fee_actual_base(receipt.gas_exec, &view, &opening));
    assert_eq!(receipt.refund, reserve - receipt.fee_base - receipt.tip);
    let payer_after = chain.state.get_account_by_id(payer.account_id()).data.native_balance().unwrap();
    assert_eq!(payer_after, payer_before - receipt.fee_base - receipt.tip, "reserve less refund leaves fee plus tip");
    assert_eq!(
        decrypted_sender_balance(tx_message(&lee_tx), &sender),
        SENDER_BALANCE - TRANSFER_AMOUNT,
        "the private sender pays only its transfer, not the base fee or tip"
    );
    assert_eq!(
        chain.state.get_account_by_id(recipient).data.native_balance().unwrap(),
        TRANSFER_AMOUNT,
        "the private action's public transfer applied"
    );
}

fn tx_message(tx: &LeeTransaction) -> &PrivacyPreservingTransaction {
    let LeeTransaction::PrivacyPreserving(tx) = tx else { panic!("a private transaction") };
    tx
}

#[test]
fn a_declared_payer_absent_from_nonces_and_witness_is_not_fee_authorized() {
    require_dev_mode();
    let DeclaredTransfer { mut chain, payer, mut tx, .. } = declared_transfer(TIP);
    let before = chain.state.get_account_by_id(payer.account_id()).data.native_balance().unwrap();
    assert_eq!(
        tx.message.nonces.remove(&payer.account_id()),
        Some(chain.state.get_account_by_id(payer.account_id()).nonce)
    );
    assert_eq!(tx.witness_set.entries().len(), 1);
    tx.witness_set = WitnessSet::from_raw_parts(Vec::new(), tx.witness_set.proof().clone());
    assert!(tx.message.nonces.is_empty() && tx.witness_set.entries().is_empty());
    let lee_tx = LeeTransaction::PrivacyPreserving(tx);
    assert!(!lee_tx.is_fee_authorized());
    let view = classify(&lee_tx).expect("classification leaves signer authorization to settlement");
    let opening = opening_fee_state(&chain.state);
    let error = settle_charged_transaction(
        &lee_tx,
        &view,
        &mut chain.state,
        &opening,
        chain.block_id,
        0,
        &mut BlockFeeSummary::default(),
    )
    .expect_err("settlement refuses an unauthorized designated payer");
    assert_eq!(error, "designated payer's authorization is missing");
    assert_eq!(chain.state.get_account_by_id(payer.account_id()).data.native_balance().unwrap(), before);
}

#[test]
fn a_declared_fee_and_an_in_proof_fee_are_refused_as_a_double_fee() {
    require_dev_mode();
    let (chain, _, run) = in_proof_transfer();
    let mut tx = run.tx;
    assert!(tx.message.execution.proven_fee.is_some());
    tx.message.fee = Some(FeeDeclaration::new(chain.payer, market::MAX_GAS_EXEC, TIP, u128::MAX));
    let error = classify(&LeeTransaction::PrivacyPreserving(tx)).expect_err("two fee sources are invalid");
    assert_eq!(error, "private transaction pays its fee both in its proof and by declaration");
}

#[test]
fn a_declared_gas_limit_below_private_verification_is_refused() {
    require_dev_mode();
    let DeclaredTransfer { mut tx, .. } = declared_transfer(TIP);
    let fee = tx.message.fee.as_mut().expect("the declared fee");
    fee.gas_limit = market::PRIVATE_VERIFY_GAS - 1;
    let error = classify(&LeeTransaction::PrivacyPreserving(tx)).expect_err("the gas limit excludes verification");
    assert_eq!(
        error,
        format!(
            "private transaction declares gas limit {}, below the {} its proof's verification costs",
            market::PRIVATE_VERIFY_GAS - 1,
            market::PRIVATE_VERIFY_GAS
        )
    );
}

#[test]
fn an_in_proof_fee_still_classifies_as_a_private_fee_view() {
    require_dev_mode();
    let (_, sender, run) = in_proof_transfer();
    let lee_tx = LeeTransaction::PrivacyPreserving(run.tx);
    let view = classify(&lee_tx).expect("the existing private fee transaction classifies");
    let FeeTxView::Private { payer, reserve, .. } = view else {
        panic!("an in-proof fee must remain on the private fee view: {view:?}");
    };
    // The fee crossing's sender is the private sender as presented to the public side (`SenderPresentation::Blinded`
    // in the harness's proving): the view's payer is not the private account's id. (Which alias it is, is not checked.)
    assert_ne!(payer, sender.id(), "the in-proof fee does not name the private account");
    assert_eq!(reserve, 100_000_000);
    assert!(run.charge.receipt.success);
}
