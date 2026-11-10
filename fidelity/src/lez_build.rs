//! LEZ v0.4.0-rc1 blocks and transactions built with LEZ's own crates.
//!
//! [`Chain`] starts from LEZ's testnet initial state and genesis block and appends blocks the way
//! LEZ's tests and sequencer do (`settled_block` in `lez/chain_state/src/apply.rs` tests): the user
//! transactions, then the forced fee transaction (`fee_invocation`) carrying the settled summary
//! and payout, then the clock transaction (`clock_invocation`). Two modes:
//! - [`Chain::push_valid`]: the block is accepted by LEZ's `apply_block` (hash, id, link, every
//!   transaction executed, the fee transaction byte-compared, `reverted` compared);
//! - [`Chain::push_stand_in`]: for transactions that cannot execute here (calls to the bridge
//!   program, which does not exist; privacy-preserving transactions with stand-in proofs). The
//!   state advances by applying the equivalent empty block, whose fee and clock transactions the
//!   real block then carries; the block itself is real-format but not executable.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context as _, Result};
use chain_state::apply::{apply_block, derive_block_summary, Tip};
use common::block::{Block, HashableBlockData};
use common::transaction::{clock_invocation, fee_invocation, FeePayee, LeeTransaction};
use common::HashType;
use lee::privacy_preserving_transaction::{circuit::Proof, Message as PpMessage, WitnessSet as PpWitnessSet};
use lee::public_transaction::{Message as PublicMessage, WitnessSet as PublicWitnessSet};
use lee::{
    AccountId, Actor, Boundary, BoundaryStep, Delivery, FeeDeclaration, MessageEnvelope, PrivacyPreservingTransaction,
    PrivateKey, PublicKey, PublicTransaction, RootCall, V03State,
};
use lee_core::account::{Account, Nonce};
use lee_core::encryption::{Ciphertext, EphemeralPublicKey};
use lee_core::program::{ValidityWindow, ValidityWindows};
use lee_core::{
    Commitment, EncryptedNote, FeeFallback, Nullifier, PrivateAction, ProgramImageClaim, ProvenExecution, ProvenFee,
    RecoveryBinding, SealedMessage,
};

/// Wall-clock milliseconds of the genesis block, and LEZ's testnet block interval (10 s,
/// `lez/sequencer/service/networks/testnet.json`). Timestamps are producer-chosen u64s; only their
/// width matters for the format.
pub const GENESIS_TIMESTAMP_MS: u64 = 1_762_000_000_000;
pub const BLOCK_INTERVAL_MS: u64 = 10_000;

/// The succinct receipt carried by every privacy-preserving transaction
/// (`PROOF_BYTES`, `lez/programs/fee/core/src/market.rs`).
pub const PROOF_BYTES: usize = 223_551;

pub fn producer() -> FeePayee {
    FeePayee::Present(common::test_utils::producer_account_for_testing())
}

pub struct Chain {
    pub state: V03State,
    pub blocks: Vec<Block>,
    pub tip: Tip,
    /// Whether `apply_block` accepted the block (false for stand-in blocks).
    pub executed: Vec<bool>,
}

impl Chain {
    /// LEZ's testnet initial state with the producer's reward account claimed (as LEZ's own
    /// `applies_transfers_and_advances_state` test does: "simulating the stake a real sequencer
    /// holds before producing"), and its genesis block (id 1).
    pub fn genesis() -> Result<Self> {
        let mut state =
            testnet_initial_state::initial_state(true).with_public_accounts([common::test_utils::producer_seed()]);
        let genesis = settled(&state, 1, HashType::default(), Vec::new())?;
        apply_block(None, &genesis, &mut state).map_err(|e| anyhow::anyhow!("genesis: {e:?}"))?;
        Ok(Self { state, tip: Tip::from(&genesis), blocks: vec![genesis], executed: vec![true] })
    }

    fn next_id(&self) -> u64 {
        self.tip.block_id + 1
    }

    /// Appends a block of `txs` that LEZ's `apply_block` must accept.
    pub fn push_valid(&mut self, txs: Vec<LeeTransaction>) -> Result<&Block> {
        let block = settled(&self.state, self.next_id(), self.tip.hash, txs)?;
        apply_block(Some(&self.tip), &block, &mut self.state)
            .map_err(|e| anyhow::anyhow!("block {}: {e:?}", block.header.block_id))?;
        self.tip = Tip::from(&block);
        self.blocks.push(block);
        self.executed.push(true);
        Ok(self.blocks.last().unwrap())
    }

    /// Appends a real-format block of `txs` (with `reverted` as given) that cannot be executed
    /// here; see the module docs.
    pub fn push_stand_in(&mut self, txs: Vec<LeeTransaction>, reverted: Vec<u64>) -> Result<&Block> {
        let id = self.next_id();
        let empty = settled(&self.state, id, self.tip.hash, Vec::new())?;
        apply_block(Some(&self.tip), &empty, &mut self.state).map_err(|e| anyhow::anyhow!("empty {id}: {e:?}"))?;
        let mut transactions = txs;
        transactions.extend(empty.body.transactions);
        let block = HashableBlockData {
            block_id: id,
            prev_block_hash: self.tip.hash,
            timestamp: empty.header.timestamp,
            transactions,
            reverted,
        }
        .into_pending_block();
        self.tip = Tip::from(&block);
        self.blocks.push(block);
        self.executed.push(false);
        Ok(self.blocks.last().unwrap())
    }

    /// `borsh(Block)` of block `i`: the bytes LEZ's sequencer inscribes (`block_inscription`,
    /// `lez/sequencer/actors/bedrock/src/actor.rs`).
    pub fn inscribed(&self, i: usize) -> Vec<u8> {
        borsh::to_vec(&self.blocks[i]).expect("borsh")
    }
}

/// A block of `txs` plus the forced fee and clock transactions, settled against `state`.
fn settled(state: &V03State, id: u64, prev: HashType, mut txs: Vec<LeeTransaction>) -> Result<Block> {
    let timestamp = GENESIS_TIMESTAMP_MS + id * BLOCK_INTERVAL_MS;
    let (summary, payout, reverted) = derive_block_summary(state, &txs, id, timestamp)
        .map_err(|e| anyhow::anyhow!("settle block {id}: {e:?}"))
        .context("derive_block_summary")?;
    txs.push(LeeTransaction::Public(fee_invocation(summary, payout, producer())));
    txs.push(LeeTransaction::Public(clock_invocation(id, timestamp)));
    Ok(HashableBlockData { block_id: id, prev_block_hash: prev, timestamp, transactions: txs, reverted }
        .into_pending_block())
}

// ---- transactions ----------------------------------------------------------------------------------

pub fn private_key(seed: u8) -> PrivateKey {
    PrivateKey::try_new([seed; 32]).expect("valid secp256k1 scalar")
}

pub fn account_of(key: &PrivateKey) -> AccountId {
    AccountId::from(&PublicKey::new_from_private_key(key))
}

/// A signed, fee-declaring public transaction whose root call sends `message` (already
/// borsh-encoded) to the actor `(account, program)`, as a wallet builds one.
pub fn public_call(signer: &PrivateKey, nonce: u128, to: Actor, message: Vec<u8>) -> LeeTransaction {
    let me = account_of(signer);
    let message = PublicMessage::new_preserialized(
        to,
        message,
        [to, Actor::native_balance(me)],
        BTreeMap::from([(me, Nonce(nonce))]),
        Some(FeeDeclaration::new(me, 2_000_000, 0, u128::MAX >> 1)),
    );
    let witness = PublicWitnessSet::for_message(&message, &[signer]);
    LeeTransaction::Public(PublicTransaction::new(message, witness))
}

/// A native transfer as LEZ's test utilities build one (executes on the testnet initial state for
/// the genesis accounts).
pub fn native_transfer(signer: &PrivateKey, nonce: u128, to: AccountId, amount: u128) -> LeeTransaction {
    common::test_utils::create_transaction_native_token_transfer(account_of(signer), nonce, to, amount, signer)
}

/// One boundary delivery of a stand-in: `(private→public?, from, to, message)`.
pub type DeliverySpec = (bool, Actor, Actor, Vec<u8>);

/// What a stand-in privacy-preserving transaction carries in the clear.
#[derive(Clone)]
pub struct PpSpec {
    /// The boundary, one entry per delivery.
    pub deliveries: Vec<DeliverySpec>,
    pub public_root: Option<(Actor, Vec<u8>)>,
    /// Private actions and the ciphertext length of each note.
    pub actions: usize,
    pub note_len: usize,
    pub recovery_bindings: usize,
    pub sealed_messages: usize,
    /// A fee fallback whose boundary carries these deliveries.
    pub fallback: Option<Vec<DeliverySpec>>,
    pub claims: Vec<ProgramImageClaim>,
    pub proof_len: usize,
}

impl Default for PpSpec {
    /// Sizes after LEZ's wallet: 7 private actions (it pads to 7), ML-KEM-768 notes with a
    /// 1088-byte `epk`, one recovery binding for the fee refund, and a full-size proof.
    fn default() -> Self {
        Self {
            deliveries: Vec::new(),
            public_root: None,
            actions: 7,
            note_len: 300,
            recovery_bindings: 1,
            sealed_messages: 0,
            fallback: None,
            claims: Vec::new(),
            proof_len: PROOF_BYTES,
        }
    }
}

fn note(len: usize, salt: u8) -> EncryptedNote {
    EncryptedNote {
        ciphertext: Ciphertext::from_inner(vec![salt; len]),
        epk: EphemeralPublicKey(vec![salt ^ 0x5a; lee_core::ML_KEM_768_CIPHERTEXT_LEN]),
    }
}

fn boundary(deliveries: &[DeliverySpec]) -> Boundary {
    let mut steps = Vec::new();
    for (private_to_public, from, to, message) in deliveries {
        let delivery = Delivery {
            envelope: MessageEnvelope { from: *from, to: *to, message: message.clone() },
            inherited_authorizations: BTreeSet::new(),
            inherits_entry_authorizations: false,
            pda_seeds: BTreeSet::new(),
        };
        steps.push(if *private_to_public {
            BoundaryStep::PrivateToPublic(delivery)
        } else {
            BoundaryStep::PublicToPrivate(delivery)
        });
        steps.push(if *private_to_public { BoundaryStep::EndPublicSubtree } else { BoundaryStep::EndPrivateSubtree });
    }
    steps
}

fn private_action(i: usize, note_len: usize) -> PrivateAction {
    let seed = [u8::try_from(i % 251).unwrap(); 32];
    PrivateAction {
        nullifier: Nullifier::for_dummy(&seed),
        root: [0x42; 32],
        commitment: Commitment::new(&AccountId::new(seed), &Account::default()),
        encrypted_post_state: note(note_len, u8::try_from(i % 251).unwrap()),
    }
}

/// A privacy-preserving transaction built from LEZ's own types with stand-in contents: the
/// message structure is real, the proof is `proof_len` filler bytes, so the transaction is
/// real-format but its proof does not verify.
pub fn pp_stand_in(spec: &PpSpec, salt: u8) -> LeeTransaction {
    let execution = ProvenExecution {
        boundary: boundary(&spec.deliveries),
        async_messages: (0..spec.sealed_messages)
            .map(|i| SealedMessage { commitment: private_action(i, 1).commitment, note: note(spec.note_len, salt) })
            .collect(),
        recovery_bindings: (0..spec.recovery_bindings)
            .map(|i| RecoveryBinding {
                address: AccountId::new([salt ^ u8::try_from(i).unwrap(); 32]),
                note: note(64, salt),
            })
            .collect(),
        public_root: spec.public_root.clone().map(|(to, message)| RootCall { to, message }),
        private_actions: (0..spec.actions).map(|i| private_action(i + usize::from(salt) * 8, spec.note_len)).collect(),
        validity: ValidityWindows {
            blocks: ValidityWindow::try_from((Some(1u64), Some(1_000_000u64))).unwrap(),
            timestamps: ValidityWindow::try_from((None, Some(u64::MAX))).unwrap(),
        },
        program_image_claims: spec.claims.clone(),
        fee: Some(ProvenFee {
            refund_to: Some(AccountId::new([salt; 32])),
            fallback: spec
                .fallback
                .as_ref()
                .map(|d| FeeFallback { action: private_action(999, spec.note_len), boundary: boundary(d) }),
        }),
    };
    let message = PpMessage {
        public_context: lee::PublicExecutionContext::default(),
        execution,
        nonces: BTreeMap::new(),
        admission_evidence: Vec::new(),
    };
    let witness = PpWitnessSet::from_raw_parts(Vec::new(), Proof::from_inner(vec![salt; spec.proof_len]));
    LeeTransaction::PrivacyPreserving(PrivacyPreservingTransaction::new(message, witness))
}

/// A real privacy-preserving transaction (from LEZ's committed sequencer dump) with its dev-mode
/// receipt replaced by `proof_len` filler bytes: the message part as LEZ's wallet made it, the
/// proof at its real size.
pub fn resize_proof(tx: &LeeTransaction, proof_len: usize) -> Option<LeeTransaction> {
    let LeeTransaction::PrivacyPreserving(pp) = tx else { return None };
    let (sigs, _) = pp.witness_set().clone().into_raw_parts();
    let witness = PpWitnessSet::from_raw_parts(sigs, Proof::from_inner(vec![0xA5; proof_len]));
    Some(LeeTransaction::PrivacyPreserving(PrivacyPreservingTransaction::new(pp.message().clone(), witness)))
}
