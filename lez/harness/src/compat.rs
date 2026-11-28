//! The LEZ calls whose API differs between the release candidates (v0.4.0-rc1 to rc3) and v0.4.0 (feature
//! `lez-040`), behind one interface, so that the rest of the harness and its tests read the same on every tag.
//!
//! What v0.4.0 changed here:
//! - public accounts (LEZ #1135): an account is its secret, from which the key at each nonce derives and rotates,
//!   and its id is the hash of its nonce-0 key; a release candidate's account is a Schnorr key whose public key is
//!   the id. [`AccountKey`], [`account_key`], [`account_of`], [`sign`];
//! - authority (LEZ #1144): a transaction declares each actor with its authority, and a signature authorizes only
//!   the actors it marks `Signer`, on the transaction's root call; a handler sees the grants it holds
//!   (`ReceiveInput::grants`) instead of a flag. [`declared`], [`receive_input`], [`signer_accounts`];
//! - genesis (LEZ #946): programs are seeded through `with_programs_at` (the test builders behind `lee`'s
//!   `test-utils`); the loader's entries moved to `program_loader_core`. [`named_programs`], [`genesis_programs`],
//!   [`program_header`].

#[cfg(feature = "lez-040")]
use std::collections::BTreeSet;

use lee::{program::Program, public_transaction::Message as PublicMessage, public_transaction::WitnessSet};
use lee::{AccountId, Actor, PublicTransaction, V03State};
use lee_core::{
    account::ActorState,
    program::{ProgramHeader, ReceiveInput},
};

/// A public account's key: a Schnorr private key on the release candidates.
#[cfg(not(feature = "lez-040"))]
pub type AccountKey = lee::PrivateKey;
/// A public account's key: from v0.4.0 the account's secret, from which the key at each nonce derives (LEZ #1135).
#[cfg(feature = "lez-040")]
pub type AccountKey = lee::signature::PublicAccountSecret;

/// The account key made of `bytes` (a nonzero scalar on the release candidates).
#[cfg(not(feature = "lez-040"))]
pub fn account_key(bytes: [u8; 32]) -> AccountKey {
    lee::PrivateKey::try_new(bytes).expect("a valid private key")
}

/// The account key made of `bytes`: the account's secret.
#[cfg(feature = "lez-040")]
pub fn account_key(bytes: [u8; 32]) -> AccountKey {
    lee::signature::PublicAccountSecret::new(bytes)
}

/// The public account `key` controls: its public key on the release candidates.
#[cfg(not(feature = "lez-040"))]
pub fn account_of(key: &AccountKey) -> AccountId {
    AccountId::from(&lee::PublicKey::new_from_private_key(key))
}

/// The public account `key` controls: the hash of its nonce-0 key (LEZ #1135).
#[cfg(feature = "lez-040")]
pub fn account_of(key: &AccountKey) -> AccountId {
    lee::signature::public_account_id(key)
}

/// The witness set of `message` signed by `keys`.
#[cfg(not(feature = "lez-040"))]
pub fn sign(message: &PublicMessage, keys: &[&AccountKey]) -> WitnessSet {
    WitnessSet::for_message(message, keys)
}

/// The witness set of `message` signed by `keys`, each with its key at the nonce the message gives its account,
/// rotating to the next nonce's key of the same type (Schnorr, the only type of v0.4.0).
#[cfg(feature = "lez-040")]
pub fn sign(message: &PublicMessage, keys: &[&AccountKey]) -> WitnessSet {
    use lee::signature::Signer;
    use lee_core::signing_key::KeyType;
    let signers: Vec<Signer> = keys
        .iter()
        .map(|key| {
            let account = account_of(key);
            let nonce = *message.nonces.get(&account).expect("every signer has a nonce in the message");
            Signer::new(key, nonce, KeyType::Schnorr, KeyType::Schnorr)
        })
        .collect();
    WitnessSet::sign(message, &signers)
}

/// The declared actors of a public message: bare actors on the release candidates.
#[cfg(not(feature = "lez-040"))]
pub type Declared = Vec<Actor>;
/// The declared actors of a public message, each with its authority (LEZ #1144).
#[cfg(feature = "lez-040")]
pub type Declared = Vec<(Actor, lee_core::execution_state::Authority)>;

/// `actors` as a public message declares them when the accounts `signers` sign it: on v0.4.0 every actor of a
/// signing account is marked `Signer` (`lee::test_utils::signed_by`'s rule, which is what a release candidate's
/// signature authorized), the others `None`.
#[cfg(not(feature = "lez-040"))]
pub fn declared(actors: impl IntoIterator<Item = Actor>, _signers: impl IntoIterator<Item = AccountId>) -> Declared {
    actors.into_iter().collect()
}

/// `actors` as a public message declares them when the accounts `signers` sign it: on v0.4.0 every actor of a
/// signing account is marked `Signer` (`lee::test_utils::signed_by`'s rule, which is what a release candidate's
/// signature authorized), the others `None`.
#[cfg(feature = "lez-040")]
pub fn declared(actors: impl IntoIterator<Item = Actor>, signers: impl IntoIterator<Item = AccountId>) -> Declared {
    lee::test_utils::signed_by(actors, signers)
}

/// The declared actors without their authorities.
#[cfg(not(feature = "lez-040"))]
pub fn declared_actors(actors: &std::collections::BTreeSet<Actor>) -> Vec<Actor> {
    actors.iter().copied().collect()
}

/// The declared actors without their authorities.
#[cfg(feature = "lez-040")]
pub fn declared_actors(actors: &std::collections::BTreeMap<Actor, lee_core::execution_state::Authority>) -> Vec<Actor> {
    actors.keys().copied().collect()
}

/// The input of one delivery to `receiver`. `authorized`: on the release candidates the input's flag ("signed by the
/// receiver or authorized by owning program"); on v0.4.0 whether the delivery carries a grant over the receiver.
#[cfg(not(feature = "lez-040"))]
pub fn receive_input(
    receiver: Actor,
    from: Option<Actor>,
    authorized: bool,
    pre_state: ActorState,
    message: Vec<u8>,
) -> ReceiveInput {
    ReceiveInput { receiver, from, is_authorized: authorized, pre_state, message }
}

/// The input of one delivery to `receiver`. `authorized`: on the release candidates the input's flag ("signed by the
/// receiver or authorized by owning program"); on v0.4.0 whether the delivery carries a grant over the receiver.
#[cfg(feature = "lez-040")]
pub fn receive_input(
    receiver: Actor,
    from: Option<Actor>,
    authorized: bool,
    pre_state: ActorState,
    message: Vec<u8>,
) -> ReceiveInput {
    let grants = if authorized { BTreeSet::from([receiver]) } else { BTreeSet::new() };
    ReceiveInput { receiver, from, grants, pre_state, message }
}

/// A reply (a `StateReply` a read sent back) to `receiver`, in a private execution whose root `receiver` is: on the
/// release candidates authorized like every delivery to the account that signed; on v0.4.0 it carries no grant (the
/// replier attaches none, LEZ #1144).
pub fn reply_input(receiver: Actor, from: Actor, pre_state: ActorState, message: Vec<u8>) -> ReceiveInput {
    receive_input(receiver, Some(from), !cfg!(feature = "lez-040"), pre_state, message)
}

/// A private transaction's fee transfer inside its proof: `amount` from `payer` to `recipient`, the unspent part to
/// `refund_to` (or back to the payer as presented). v0.4.0 adds the producer's tip (LEZ #1125): none here.
#[cfg(not(feature = "lez-040"))]
pub fn fee_transfer(
    payer: AccountId,
    recipient: AccountId,
    amount: u128,
    refund_to: Option<AccountId>,
) -> lee_core::FeeTransfer {
    lee_core::FeeTransfer { payer, recipient, amount, refund_to }
}

/// A private transaction's fee transfer inside its proof: `amount` from `payer` to `recipient`, the unspent part to
/// `refund_to` (or back to the payer as presented). v0.4.0 adds the producer's tip (LEZ #1125): none here.
#[cfg(feature = "lez-040")]
pub fn fee_transfer(
    payer: AccountId,
    recipient: AccountId,
    amount: u128,
    refund_to: Option<AccountId>,
) -> lee_core::FeeTransfer {
    lee_core::FeeTransfer { payer, recipient, amount, refund_to, tip: 0 }
}

/// What a private transaction's proof commits about its fee: the refund target and whether it carries a fallback
/// (`ProvenExecution::fee` on the release candidates).
#[cfg(not(feature = "lez-040"))]
pub fn proven_fee(execution: &lee_core::ProvenExecution) -> Option<(Option<AccountId>, bool)> {
    execution.fee.as_ref().map(|f| (f.refund_to, f.fallback.is_some()))
}

/// What a private transaction's proof commits about its fee: the refund target and whether it carries a fallback
/// (`ProvenExecution::proven_fee` from v0.4.0, which also commits the tip).
#[cfg(feature = "lez-040")]
pub fn proven_fee(execution: &lee_core::ProvenExecution) -> Option<(Option<AccountId>, bool)> {
    execution.proven_fee.as_ref().map(|f| (f.refund_to, f.fallback.is_some()))
}

/// The account a public account's admission evidence shows (v0.4.0: `None` for evidence that names no account).
#[cfg(not(feature = "lez-040"))]
pub fn evidence_account(evidence: &lee::PublicAccountEvidence) -> Option<AccountId> {
    Some(evidence.account_id())
}

/// The account a public account's admission evidence shows (v0.4.0: `None` for evidence that names no account).
#[cfg(feature = "lez-040")]
pub fn evidence_account(evidence: &lee::PublicAccountEvidence) -> Option<AccountId> {
    evidence.account_id()
}

/// A private account's witness that authorizes the transaction's root call `root` to its actor: on the release
/// candidates the account's key authorized every actor on it.
#[cfg(not(feature = "lez-040"))]
pub fn authorize_root(witness: lee_core::PrivateWitness, _root: Actor) -> lee_core::PrivateWitness {
    witness
}

/// A private account's witness that authorizes the transaction's root call `root` to its actor: from v0.4.0 a witness
/// names the programs whose actor on its account it authorizes (LEZ #1144: `signer_programs`; LEE's test witnesses
/// name only the native token's), as LEZ's wallet marks a mention that authorizes.
#[cfg(feature = "lez-040")]
pub fn authorize_root(witness: lee_core::PrivateWitness, root: Actor) -> lee_core::PrivateWitness {
    witness.with_signer_programs([root.program_account_id])
}

/// The accounts whose signatures `tx` carries.
#[cfg(not(feature = "lez-040"))]
pub fn signer_accounts(tx: &PublicTransaction) -> Vec<AccountId> {
    tx.witness_set().signatures_and_public_keys().iter().map(|(_, key)| AccountId::from(key)).collect()
}

/// The accounts whose signatures `tx` carries: one witness entry per account with a nonce in the message.
#[cfg(feature = "lez-040")]
pub fn signer_accounts(tx: &PublicTransaction) -> Vec<AccountId> {
    tx.message().nonces.keys().copied().collect()
}

/// `state` with `programs` deployed, immutable, at the given accounts.
#[cfg(not(feature = "lez-040"))]
pub fn named_programs(state: V03State, programs: impl IntoIterator<Item = (AccountId, Program)>) -> V03State {
    state.with_named_programs(programs)
}

/// `state` with `programs` deployed, immutable, at the given accounts.
#[cfg(feature = "lez-040")]
pub fn named_programs(state: V03State, programs: impl IntoIterator<Item = (AccountId, Program)>) -> V03State {
    state.with_programs_at(programs.into_iter().map(|(account, program)| (account, program, true)))
}

/// `state` with `programs` deployed at the accounts their image ids derive (`AccountId::from_builtin_program`),
/// immutable or not.
#[cfg(not(feature = "lez-040"))]
pub fn genesis_programs(state: V03State, programs: impl IntoIterator<Item = (Program, bool)>) -> V03State {
    state.with_genesis_programs(programs)
}

/// `state` with `programs` deployed at the accounts their image ids derive (`AccountId::from_builtin_program`),
/// immutable or not.
#[cfg(feature = "lez-040")]
pub fn genesis_programs(state: V03State, programs: impl IntoIterator<Item = (Program, bool)>) -> V03State {
    state.with_programs_at(
        programs
            .into_iter()
            .map(|(program, immutable)| (AccountId::from_builtin_program(program.id()), program, immutable)),
    )
}

/// The program header a loader actor state holds: untagged on v0.4.0-rc1.
#[cfg(not(feature = "lez-rc2"))]
pub fn program_header(loader_actor_state: &[u8]) -> Option<ProgramHeader> {
    ProgramHeader::from_bytes(loader_actor_state)
}

/// The program header a loader actor state holds: a tagged `LoaderEntry` from v0.4.0-rc2 on (LEZ #943).
#[cfg(all(feature = "lez-rc2", not(feature = "lez-040")))]
pub fn program_header(loader_actor_state: &[u8]) -> Option<ProgramHeader> {
    ProgramHeader::from_loader_actor_state(loader_actor_state)
}

/// The program header a loader actor state holds: v0.4.0 moved `LoaderEntry` to `program_loader_core` (LEZ #946)
/// with the same encoding, mirrored here (a header is variant 0).
#[cfg(feature = "lez-040")]
pub fn program_header(loader_actor_state: &[u8]) -> Option<ProgramHeader> {
    #[derive(borsh::BorshDeserialize)]
    #[allow(dead_code)]
    enum LoaderEntry {
        Header(ProgramHeader),
        Segment { bytecode: Vec<u8>, next_segment: Option<AccountId> },
    }
    match borsh::from_slice(loader_actor_state).ok()? {
        LoaderEntry::Header(header) => Some(header),
        LoaderEntry::Segment { .. } => None,
    }
}

// ---- Helpers for the tests and binaries ----

/// The loader segment fields a test reads, independent of the crate that owns the loader types.
#[derive(borsh::BorshDeserialize)]
pub struct ProgramSegment {
    pub bytecode: Vec<u8>,
    pub next_segment: Option<AccountId>,
}

/// Decode a loader segment: before v0.4.0 `ProgramSegment` lives in `lee_core::program`.
#[cfg(not(feature = "lez-rc2"))]
pub fn program_segment(loader_actor_state: &[u8]) -> Option<ProgramSegment> {
    let segment = lee_core::program::ProgramSegment::from_bytes(loader_actor_state)?;
    Some(ProgramSegment { bytecode: segment.bytecode, next_segment: segment.next_segment })
}

/// Decode a tagged loader segment: before v0.4.0 `ProgramSegment` lives in `lee_core::program`.
#[cfg(all(feature = "lez-rc2", not(feature = "lez-040")))]
pub fn program_segment(loader_actor_state: &[u8]) -> Option<ProgramSegment> {
    let segment = lee_core::program::ProgramSegment::from_loader_actor_state(loader_actor_state)?;
    Some(ProgramSegment { bytecode: segment.bytecode, next_segment: segment.next_segment })
}

/// Decode a loader segment: v0.4.0 moved `ProgramSegment` and `LoaderEntry` to `program_loader_core`
/// (LEZ #946), which the harness does not depend on; mirror their unchanged borsh encoding.
#[cfg(feature = "lez-040")]
pub fn program_segment(loader_actor_state: &[u8]) -> Option<ProgramSegment> {
    #[derive(borsh::BorshDeserialize)]
    #[allow(dead_code)]
    enum LoaderEntry {
        Header(ProgramHeader),
        Segment(ProgramSegment),
    }
    match borsh::from_slice(loader_actor_state).ok()? {
        LoaderEntry::Segment(segment) => Some(segment),
        LoaderEntry::Header(_) => None,
    }
}

/// A private transaction's storage gas: the release candidates' flat `PRIVATE_GAS_STOR`.
#[cfg(not(feature = "lez-040"))]
pub fn private_storage_gas(_transaction: &crate::settle::LeeTransaction) -> u64 {
    fee_core::market::PRIVATE_GAS_STOR
}

/// A private transaction's storage gas: v0.4.0 charges its canonical bytes, with the proof counted
/// at `market::PROOF_BYTES`, instead of a flat amount (LEZ #1044).
#[cfg(feature = "lez-040")]
pub fn private_storage_gas(transaction: &crate::settle::LeeTransaction) -> u64 {
    transaction.charged_bytes().expect("a private transaction's charged size")
}

/// A public message's declared fee: on the release candidates it belongs to its public execution.
#[cfg(not(feature = "lez-040"))]
pub fn public_fee(message: &PublicMessage) -> Option<lee::FeeDeclaration> {
    message.execution.fee
}

/// A public message's declared fee: v0.4.0 moved it to the transaction message (LEZ #1080).
#[cfg(feature = "lez-040")]
pub fn public_fee(message: &PublicMessage) -> Option<lee::FeeDeclaration> {
    message.fee
}

/// The fee fallback in the shape the release candidates publish, for assertions that compare its
/// action and boundary. `encoded_len` remains the size of the actual version-specific fallback.
pub struct ProvenFallback {
    pub action: lee_core::PrivateAction,
    pub boundary: lee_core::execution_state::Boundary,
    pub encoded_len: usize,
}

/// The fields of a proof's fee commitment that the tests inspect.
pub struct ProvenFeeDetails {
    pub refund_to: Option<AccountId>,
    pub fallback: Option<ProvenFallback>,
}

/// Read a proof's fee commitment: the release candidates store the fallback's whole private action.
#[cfg(not(feature = "lez-040"))]
pub fn proven_fee_details(execution: &lee_core::ProvenExecution) -> Option<ProvenFeeDetails> {
    execution.fee.as_ref().map(|fee| ProvenFeeDetails {
        refund_to: fee.refund_to,
        fallback: fee.fallback.as_ref().map(|fallback| ProvenFallback {
            action: fallback.action.clone(),
            boundary: fallback.boundary.clone(),
            encoded_len: borsh::to_vec(fallback).expect("a fee fallback's borsh encoding").len(),
        }),
    })
}

/// Read a proof's fee commitment: v0.4.0 splits the fallback action into nullifier, root,
/// commitment and ciphertext and reconstructs its ephemeral key from the payer's action (LEZ #1146).
#[cfg(feature = "lez-040")]
pub fn proven_fee_details(execution: &lee_core::ProvenExecution) -> Option<ProvenFeeDetails> {
    execution.proven_fee.as_ref().map(|fee| ProvenFeeDetails {
        refund_to: fee.refund_to,
        fallback: fee.fallback.as_ref().map(|fallback| ProvenFallback {
            action: fallback
                .action(&execution.private_actions)
                .expect("a fee fallback names its payer's private action"),
            boundary: fallback.boundary.clone(),
            encoded_len: borsh::to_vec(fallback).expect("a fee fallback's borsh encoding").len(),
        }),
    })
}

/// Replace the aligned fields of a release-candidate fallback's private action.
#[cfg(not(feature = "lez-040"))]
pub fn set_proven_fallback_action(execution: &mut lee_core::ProvenExecution, action: &lee_core::PrivateAction) {
    execution.fee.as_mut().expect("a proven fee").fallback.as_mut().expect("a fee fallback").action = action.clone();
}

/// Replace the aligned fields of a v0.4.0 fallback: its action was split into nullifier, root,
/// commitment and ciphertext (LEZ #1146); the ephemeral key still belongs to the payer's action.
#[cfg(feature = "lez-040")]
pub fn set_proven_fallback_action(execution: &mut lee_core::ProvenExecution, action: &lee_core::PrivateAction) {
    let fallback = execution.proven_fee.as_mut().expect("a proven fee").fallback.as_mut().expect("a fee fallback");
    fallback.nullifier = action.nullifier;
    fallback.root = action.root;
    fallback.commitment = action.commitment;
    fallback.ciphertext = action.encrypted_post_state.ciphertext.clone();
}

/// Borrow a release-candidate fallback's boundary for alignment.
#[cfg(not(feature = "lez-040"))]
pub fn proven_fallback_boundary_mut(
    execution: &mut lee_core::ProvenExecution,
) -> Option<&mut lee_core::execution_state::Boundary> {
    execution.fee.as_mut()?.fallback.as_mut().map(|fallback| &mut fallback.boundary)
}

/// Borrow a v0.4.0 fallback's boundary: the boundary remains whole although its private action was
/// split into separate fields (LEZ #1146).
#[cfg(feature = "lez-040")]
pub fn proven_fallback_boundary_mut(
    execution: &mut lee_core::ProvenExecution,
) -> Option<&mut lee_core::execution_state::Boundary> {
    execution.proven_fee.as_mut()?.fallback.as_mut().map(|fallback| &mut fallback.boundary)
}

/// Encode a release-candidate proof's complete fee commitment.
#[cfg(not(feature = "lez-040"))]
pub fn proven_fee_bytes(execution: &lee_core::ProvenExecution) -> Vec<u8> {
    borsh::to_vec(&execution.fee).expect("a proven fee's borsh encoding")
}

/// Encode a v0.4.0 proof's complete fee commitment, renamed `proven_fee` and carrying a tip (LEZ
/// #1146 and #1044).
#[cfg(feature = "lez-040")]
pub fn proven_fee_bytes(execution: &lee_core::ProvenExecution) -> Vec<u8> {
    borsh::to_vec(&execution.proven_fee).expect("a proven fee's borsh encoding")
}
