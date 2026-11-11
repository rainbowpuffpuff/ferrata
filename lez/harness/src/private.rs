//! Private transactions through LEZ v0.4.0-rc1: proving with `lee::execute_and_prove` (the
//! privacy-preserving circuit of LEZ's artifacts), settlement with LEZ's private fee path
//! ([`crate::settle::settle_private_transaction`]), and a dump of what the transaction makes
//! public.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    io::Read,
    time::{Duration, Instant},
};

use fee_core::BlockFeeSummary;
use ferrata_lez_wire::{attest::Attestation, Message};
use lee::{
    privacy_preserving_transaction::{circuit::ProgramCatalog, Message as PrivateMessage, WitnessSet},
    test_utils::{no_seal, update_witness, TestPrivateKeys},
    Account, AccountId, Actor, PrivacyPreservingTransaction, ProvingInput, PublicExecutionContext, Recipient,
    RecipientEncryption, SenderPresentation, Simulation,
};
use lee_core::{
    encryption::{Ciphertext, EncryptedNote, MlKem768EncapsulationKey},
    execution_state::{BoundaryStep, Delivery, TransactionEntry},
    native_token,
    program::{StateReply, TransactionEvent},
    AuthorizationSecretKey, Commitment, DummyInput, DummyOutput, EphemeralSecretKey, FeeTransfer, Nullifier,
    NullifierPublicKey, NullifierSecretKey, PrivateAccountKind, ProgramImageClaim, RootCall, SharedSecretKey,
};

use crate::{
    consumer_account, ferrata_account,
    settle::{classify, opening_fee_state, settle_private_transaction, Charge, LeeTransaction},
    Chain,
};

/// A private account with a native balance, holding the keys to spend it.
pub struct PrivateAccount {
    pub keys: TestPrivateKeys,
    pub account: Account,
}

impl PrivateAccount {
    pub fn id(&self) -> AccountId {
        self.keys.account_id()
    }
}

/// The opening under which private senders present themselves (`SenderPresentation::Blinded`).
pub const ALIAS_OPENING: [u8; 32] = [9; 32];

/// The per-transaction randomness of a private transaction's public outputs: the opening of the
/// sender's alias (`SenderPresentation::Blinded`) and the witness's random seed.
#[derive(Clone, Copy, Debug)]
pub struct Freshness {
    pub alias_opening: [u8; 32],
    pub random_seed: [u8; 32],
}

impl Freshness {
    /// Drawn fresh, as LEZ's wallet draws them for every transaction: the witness's seed
    /// (`random_seed: random_bytes()`, `lez/wallet/src/account_manager.rs` line 1151) and the
    /// opening of the fee payer's alias (`let opening = account_manager::random_bytes()`,
    /// `lez/wallet/src/lib.rs` line 1692).
    pub fn random() -> Self {
        Self { alias_opening: random_bytes(), random_seed: random_bytes() }
    }
}

/// The private-action count that LEZ v0.4.0-rc1's wallet pads every private transaction up to
/// with dummy inputs (`AccountManager::PADDED_PRIVATE_ACTIONS`, `lez/wallet/src/account_manager.rs`
/// line 387).
pub const PADDED_PRIVATE_ACTIONS: usize = 7;

/// The size the wallet pads every note's plaintext to (`CIPHERTEXT_PAD_SIZE`,
/// `lez/wallet/src/account_manager.rs` line 35; `ciphertext_padding: Some(CIPHERTEXT_PAD_SIZE)`,
/// `lez/wallet/src/lib.rs` line 1585) and the size of a dummy note's random ciphertext.
pub const CIPHERTEXT_PAD_SIZE: u32 = 512;

/// The public shape of a private transaction this harness builds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// As this harness built every private transaction until 11 November 2026: the sender's
    /// account the only private action, its note at its own length, no recovery note, the unspent
    /// reserve refunded to the public payer account, and the sender presented under
    /// [`ALIAS_OPENING`] with a zero witness seed (the constants of LEZ's own tests).
    Bare,
    /// As LEZ v0.4.0-rc1's wallet builds every private transaction (`send_proven`,
    /// `lez/wallet/src/lib.rs` lines 1484-1745): dummy inputs up to [`PADDED_PRIVATE_ACTIONS`]
    /// (`dummy_inputs: acc_manager.dummy_inputs_default()`, line 1584), every note padded to
    /// [`CIPHERTEXT_PAD_SIZE`] (line 1585), the fee paid with `refund_to: None` (lines 510, 1681,
    /// 1740) and a recovery note binding the payer's alias for the refund
    /// (`input.recoveries.push(acc_manager.fee_recovery(payer, opening)?)`, lines 1692-1695), the
    /// payer's native actor presented under that alias and its other actors under a fresh one each
    /// (`AccountManager::presenter`, account_manager.rs lines 731-748), fresh randomness
    /// ([`Freshness::random`]).
    Wallet,
}

impl Shape {
    /// `wallet` or `bare`, as the binaries take it on their command line.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "wallet" => Some(Self::Wallet),
            "bare" => Some(Self::Bare),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Wallet => "wallet",
            Self::Bare => "bare",
        }
    }
}

fn random_vec(len: usize) -> Vec<u8> {
    let mut bytes = vec![0; len];
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes)).expect("/dev/urandom");
    bytes
}

/// `account_manager::random_bytes` (`lez/wallet/src/account_manager.rs` lines 1318-1322): 32 bytes
/// of the operating system's randomness (`OsRng` there, `/dev/urandom` here).
pub fn random_bytes() -> [u8; 32] {
    random_vec(32).try_into().expect("32 bytes")
}

/// `account_manager::dummy_output` and `random_dummy_note` (`lez/wallet/src/account_manager.rs`
/// lines 1337-1356): a fresh commitment seed and a dummy note, [`CIPHERTEXT_PAD_SIZE`] random
/// bytes with a real ML-KEM-768 epk toward a throwaway key.
pub fn wallet_dummy_output() -> DummyOutput {
    let throwaway = MlKem768EncapsulationKey::from_seed(&random_bytes(), &random_bytes());
    let (_, epk) = SharedSecretKey::encapsulate(&throwaway);
    DummyOutput {
        commitment_seed: random_bytes(),
        note: EncryptedNote { ciphertext: Ciphertext::from_inner(random_vec(CIPHERTEXT_PAD_SIZE as usize)), epk },
    }
}

/// `AccountManager::dummy_inputs_default` (`lez/wallet/src/account_manager.rs` lines 656-670)
/// over `dummy_inputs` (lines 644-652): the dummy inputs that pad `action_count` private actions
/// (one per private account, one for a received message, one per claim) up to
/// [`PADDED_PRIVATE_ACTIONS`], each against `commitment_root`, the root its accounts are proven
/// under.
pub fn wallet_dummy_inputs(action_count: usize, commitment_root: [u8; 32]) -> Vec<DummyInput> {
    std::iter::repeat_with(|| DummyInput {
        nullifier_seed: random_bytes(),
        commitment_root,
        output: wallet_dummy_output(),
    })
    .take(PADDED_PRIVATE_ACTIONS.saturating_sub(action_count))
    .collect()
}

/// `AccountManager::fee_recovery` (`lez/wallet/src/account_manager.rs` lines 791-829) for a
/// regular account holding its authorization key (`RegularKey::Authorized`, the kind
/// `update_witness` gives): recovery material that binds `payer`'s alias under `opening`, so a
/// refund published there is found again by the payer's wallet.
pub fn wallet_fee_recovery(payer: &PrivateAccount, opening: [u8; 32]) -> RecipientEncryption {
    RecipientEncryption {
        recipient: Recipient {
            npk: NullifierPublicKey::from(&NullifierSecretKey::from(&payer.keys.ask)),
            vpk: payer.keys.vpk(),
            kind: PrivateAccountKind::Regular,
            opening: Some(opening),
        },
        esk: EphemeralSecretKey(random_bytes()),
    }
}

/// What a private transaction cost and showed.
pub struct PrivateRun {
    pub tx: PrivacyPreservingTransaction,
    pub charge: Charge,
    pub events: Vec<TransactionEvent>,
    pub prove_time: Duration,
    /// The borsh size of the transaction (`LeeTransaction`).
    pub tx_bytes: usize,
    pub proof_bytes: usize,
}

impl Chain {
    /// Add a private account funded with `balance` to the state (its commitment and the
    /// nullifier of its initialization), as LEZ's own settlement tests do.
    pub fn add_private_account(&mut self, seed: u8, balance: u128) -> PrivateAccount {
        let keys = TestPrivateKeys {
            ask: AuthorizationSecretKey([seed; 32]),
            d: [seed.wrapping_add(1); 32],
            z: [seed.wrapping_add(2); 32],
        };
        self.add_private_account_with_keys(keys, balance)
    }

    /// [`Chain::add_private_account`] with given keys.
    pub fn add_private_account_with_keys(&mut self, keys: TestPrivateKeys, balance: u128) -> PrivateAccount {
        let account = Account::funded(balance);
        let id = keys.account_id();
        let state = std::mem::take(&mut self.state);
        self.state = state.with_private_accounts([(
            Commitment::new(&id, &account),
            Nullifier::for_account_initialization(&id, &keys.nsk()),
        )]);
        PrivateAccount { keys, account }
    }

    /// Prove (`RISC0_DEV_MODE` decides whether for real) a transaction whose root is `message` to
    /// `root`, an actor of the private account `sender`, with `public_actors` declared public and
    /// a fee of `reserve` paid from `sender`'s native balance inside the proof, then settle it as
    /// the sequencer does.
    #[allow(clippy::too_many_arguments)]
    pub fn prove_and_settle_private(
        &mut self,
        sender: &PrivateAccount,
        root: Actor,
        message: Vec<u8>,
        public_actors: Vec<Actor>,
        programs: &ProgramCatalog,
        reserve: u128,
        timestamp_ms: u64,
    ) -> Result<PrivateRun, String> {
        self.prove_and_settle_private_with(sender, root, message, public_actors, programs, reserve, timestamp_ms, None)
    }

    /// As [`Chain::prove_and_settle_private`]; with `predicted`, the proof assumes those public
    /// replies instead of simulating the public part first (`execute_and_prove_with_cross_messages`),
    /// so a transaction whose public part will fail can still be proven and submitted.
    #[allow(clippy::too_many_arguments)]
    pub fn prove_and_settle_private_with(
        &mut self,
        sender: &PrivateAccount,
        root: Actor,
        message: Vec<u8>,
        public_actors: Vec<Actor>,
        programs: &ProgramCatalog,
        reserve: u128,
        timestamp_ms: u64,
        predicted: Option<lee::PredictedCrossMessages>,
    ) -> Result<PrivateRun, String> {
        let fixed = Freshness { alias_opening: ALIAS_OPENING, random_seed: [0; 32] };
        self.prove_and_settle_private_fresh(
            sender,
            root,
            message,
            public_actors,
            programs,
            reserve,
            timestamp_ms,
            predicted,
            fixed,
        )
    }

    /// As [`Chain::prove_and_settle_private_with`], with the sender's alias opening and the
    /// witness's random seed (which keys the encrypted note's ephemeral key) chosen by the caller,
    /// as a wallet would draw them fresh for every transaction. The other entry points use the
    /// constants of LEZ's own tests ([`ALIAS_OPENING`] and a zero seed), which make a private
    /// account's alias and ephemeral keys a function of the account and its nonce.
    #[allow(clippy::too_many_arguments)]
    pub fn prove_and_settle_private_fresh(
        &mut self,
        sender: &PrivateAccount,
        root: Actor,
        message: Vec<u8>,
        public_actors: Vec<Actor>,
        programs: &ProgramCatalog,
        reserve: u128,
        timestamp_ms: u64,
        predicted: Option<lee::PredictedCrossMessages>,
        freshness: Freshness,
    ) -> Result<PrivateRun, String> {
        self.prove_and_settle_private_shaped(
            sender,
            root,
            message,
            public_actors,
            programs,
            reserve,
            timestamp_ms,
            predicted,
            freshness,
            Shape::Bare,
        )
    }

    /// As [`Chain::prove_and_settle_private`], in the given [`Shape`]: [`Shape::Bare`] is that
    /// function's transaction exactly; [`Shape::Wallet`] pads it as LEZ's wallet pads every private
    /// transaction, with fresh randomness.
    #[allow(clippy::too_many_arguments)]
    pub fn prove_and_settle_private_as(
        &mut self,
        sender: &PrivateAccount,
        root: Actor,
        message: Vec<u8>,
        public_actors: Vec<Actor>,
        programs: &ProgramCatalog,
        reserve: u128,
        timestamp_ms: u64,
        shape: Shape,
    ) -> Result<PrivateRun, String> {
        let freshness = match shape {
            Shape::Bare => Freshness { alias_opening: ALIAS_OPENING, random_seed: [0; 32] },
            Shape::Wallet => Freshness::random(),
        };
        self.prove_and_settle_private_shaped(
            sender,
            root,
            message,
            public_actors,
            programs,
            reserve,
            timestamp_ms,
            None,
            freshness,
            shape,
        )
    }

    /// As [`Chain::prove_and_settle_private_fresh`], in the given [`Shape`].
    #[allow(clippy::too_many_arguments)]
    pub fn prove_and_settle_private_shaped(
        &mut self,
        sender: &PrivateAccount,
        root: Actor,
        message: Vec<u8>,
        public_actors: Vec<Actor>,
        programs: &ProgramCatalog,
        reserve: u128,
        timestamp_ms: u64,
        predicted: Option<lee::PredictedCrossMessages>,
        freshness: Freshness,
        shape: Shape,
    ) -> Result<PrivateRun, String> {
        let wallet = shape == Shape::Wallet;
        let inbox = Actor::native_balance(system_accounts::fee_inbox_account_id());
        let mut public_actors = public_actors;
        public_actors.push(inbox);
        let public_actor_states: HashMap<Actor, lee::ActorState> =
            public_actors.iter().map(|a| (*a, self.actor_state(*a))).collect();
        let membership = self
            .state
            .get_proof_for_commitment(&Commitment::new(&sender.id(), &sender.account))
            .ok_or("the sender's commitment is not in the state")?;
        let input = ProvingInput {
            root: TransactionEntry::Sync(RootCall { to: root, message }),
            claims: Vec::new(),
            fee: Some(FeeTransfer {
                payer: sender.id(),
                recipient: system_accounts::fee_inbox_account_id(),
                amount: reserve,
                // Bare: the unspent reserve goes back to the public payer account. Wallet: to the
                // payer's alias, which the recovery note below binds (`refund_to: None`).
                refund_to: if wallet { None } else { Some(self.payer) },
            }),
            public_context: PublicExecutionContext::new(public_actors, []),
            private_witnesses: vec![{
                let mut witness = update_witness(&sender.keys, sender.account.clone(), membership);
                witness.random_seed = freshness.random_seed;
                witness
            }],
            // One private account, a root call (no received message) and no claims: one private
            // action, padded with six dummies to the wallet's seven.
            dummy_inputs: if wallet { wallet_dummy_inputs(1, self.state.commitment_root()) } else { Vec::new() },
            ciphertext_padding: wallet.then_some(CIPHERTEXT_PAD_SIZE),
            recoveries: if wallet { vec![wallet_fee_recovery(sender, freshness.alias_opening)] } else { Vec::new() },
            private_promotions: BTreeSet::new(),
        };
        let simulation = Simulation { public_actor_states, admitted_accounts: None };
        let started = Instant::now();
        let opening = freshness.alias_opening;
        let (payer_actor, sender_id) = (Actor::native_balance(sender.id()), sender.id());
        let presentation = move |actor: Actor| match shape {
            Shape::Bare => SenderPresentation::Blinded(opening),
            Shape::Wallet if actor == payer_actor => SenderPresentation::Blinded(opening),
            Shape::Wallet if actor.account_id == sender_id => SenderPresentation::Blinded(random_bytes()),
            Shape::Wallet => SenderPresentation::Canonical,
        };
        let (output, proof) = match predicted {
            None => lee::execute_and_prove(input, &simulation, programs, presentation, |_, _| false, no_seal),
            Some(predicted) => lee::execute_and_prove_with_cross_messages(
                input,
                predicted,
                programs,
                presentation,
                |_, _| false,
                no_seal,
            ),
        }
        .map_err(|e| format!("proving failed: {e}"))?;
        let prove_time = started.elapsed();
        let proof_bytes = proof.clone().into_inner().len();
        let message = PrivateMessage::from_circuit_output(BTreeMap::new(), output);
        let tx = PrivacyPreservingTransaction::new(message, WitnessSet::from_raw_parts(vec![], proof));
        let lee_tx = LeeTransaction::PrivacyPreserving(tx.clone());
        let tx_bytes = borsh::to_vec(&lee_tx).unwrap().len();
        let view = classify(&lee_tx)?;
        let opening = opening_fee_state(&self.state);
        let mut summary = BlockFeeSummary::default();
        let (events, charge) = settle_private_transaction(
            &lee_tx,
            &view,
            &mut self.state,
            &opening,
            self.block_id,
            timestamp_ms,
            &mut summary,
        )?;
        self.block_id += 1;
        Ok(PrivateRun { tx, charge, events, prove_time, tx_bytes, proof_bytes })
    }
}

/// The programs a private transaction of these tests may run privately.
pub fn catalog() -> ProgramCatalog {
    ProgramCatalog::from([
        (ferrata_account(), crate::ferrata_program()),
        (consumer_account(), crate::consumer_program()),
    ])
}

fn actor_name(actor: &Actor) -> String {
    let program = if actor.program_account_id == ferrata_account() {
        "ferrata".to_string()
    } else if actor.program_account_id == consumer_account() {
        "consumer".to_string()
    } else if actor.program_account_id == native_token::NATIVE_TOKEN_PROGRAM_ID {
        "native".to_string()
    } else {
        format!("program {}", actor.program_account_id)
    };
    format!("{program}@{}", actor.account_id)
}

fn describe_message(delivery: &Delivery<Actor>) -> String {
    let bytes = &delivery.envelope.message;
    let to = delivery.envelope.to.program_account_id;
    let body = if to == ferrata_account() {
        match borsh::from_slice::<Message>(bytes) {
            Ok(Message::ApplyVerified { instance_id, job }) => {
                format!("ApplyVerified {{ instance_id: {}, job: {} bytes }}", hex::encode(&instance_id[..6]), job.len())
            }
            Ok(Message::Read) => "Read".into(),
            Ok(Message::ReadPadded { padding }) => format!("ReadPadded {{ padding: 0x{} }}", hex::encode(&padding)),
            Ok(Message::Attest { request }) => format!(
                "Attest {{ instance_id: {}, chain_id: {}, hops: {}, accounts: {}, logs: {}, leaves: {}, relations: {} }}",
                hex::encode(&request.instance_id[..6]),
                request.chain_id,
                request.hops.len(),
                request.accounts.len(),
                request.logs.len(),
                request.leaves.len(),
                request.relations.len()
            ),
            Ok(other) => format!("{:.80}", format!("{other:?}")),
            Err(_) => "(undecodable)".into(),
        }
    } else if to == native_token::NATIVE_TOKEN_PROGRAM_ID {
        format!("{:?}", borsh::from_slice::<native_token::Message>(bytes).ok())
    } else if let Ok(attestation) = borsh::from_slice::<Attestation>(bytes) {
        format!(
            "Attestation {{ block: {}, statements: {} }}",
            attestation.block_number,
            attestation.statements.iter().map(|s| hex::encode(&s.id[..6])).collect::<Vec<_>>().join(", ")
        )
    } else if let Ok(reply) = borsh::from_slice::<StateReply>(bytes) {
        format!("StateReply {{ state: {} bytes }}", reply.state.len())
    } else {
        format!("{} bytes", bytes.len())
    };
    let seeds = if delivery.pda_seeds.is_empty() {
        String::new()
    } else {
        format!(", {} PDA seed(s)", delivery.pda_seeds.len())
    };
    format!(
        "from {} to {}: {body} ({} bytes{seeds})",
        actor_name(&delivery.envelope.from),
        actor_name(&delivery.envelope.to),
        bytes.len()
    )
}

/// Everything the transaction publishes, field by field.
pub fn describe_public_outputs(tx: &PrivacyPreservingTransaction) -> String {
    let m = &tx.message;
    let e = &m.execution;
    let mut out = String::new();
    out.push_str(&format!(
        "public_context.actors ({}): {}\n",
        m.public_context.actors.len(),
        m.public_context.actors.iter().map(actor_name).collect::<Vec<_>>().join(", ")
    ));
    out.push_str(&format!("public_context.authorized_accounts: {:?}\n", m.public_context.authorized_accounts));
    out.push_str(&format!("execution.public_root: {:?}\n", e.public_root.as_ref().map(|r| actor_name(&r.to))));
    out.push_str(&format!("execution.boundary ({} steps):\n", e.boundary.len()));
    for step in &e.boundary {
        let line = match step {
            BoundaryStep::PrivateToPublic(d) => format!("PrivateToPublic {}", describe_message(d)),
            BoundaryStep::PublicToPrivate(d) => format!("PublicToPrivate {}", describe_message(d)),
            BoundaryStep::EndPrivateSubtree => "EndPrivateSubtree".into(),
            BoundaryStep::EndPublicSubtree => "EndPublicSubtree".into(),
        };
        out.push_str(&format!("  - {line}\n"));
    }
    out.push_str(&format!("execution.program_image_claims ({}):\n", e.program_image_claims.len()));
    for claim in &e.program_image_claims {
        let line = match claim {
            ProgramImageClaim::Disclosed { account_id, image_id } => format!(
                "Disclosed {{ program: {}, image_id: {} }}",
                if *account_id == ferrata_account() {
                    "ferrata".to_string()
                } else if *account_id == consumer_account() {
                    "consumer".to_string()
                } else {
                    account_id.to_string()
                },
                crate::image_id_hex(*image_id)
            ),
            ProgramImageClaim::Undisclosed { root } => {
                format!("Undisclosed {{ commitment root: {} }}", hex::encode(root))
            }
        };
        out.push_str(&format!("  - {line}\n"));
    }
    out.push_str(&format!(
        "execution.private_actions ({}): each a nullifier, a commitment-tree root, a new commitment and an encrypted note ({} bytes)\n",
        e.private_actions.len(),
        e.private_actions.iter().map(|a| borsh::to_vec(&a.encrypted_post_state).unwrap().len()).sum::<usize>()
    ));
    out.push_str(&format!("execution.async_messages: {}\n", e.async_messages.len()));
    out.push_str(&format!("execution.recovery_bindings: {}\n", e.recovery_bindings.len()));
    out.push_str(&format!("execution.validity: {:?}\n", e.validity));
    out.push_str(&format!(
        "execution.fee: {:?}\n",
        e.fee.as_ref().map(|f| format!("refund_to {:?}, fallback {}", f.refund_to, f.fallback.is_some()))
    ));
    out.push_str(&format!("nonces: {}, admission evidence: {}\n", m.nonces.len(), m.admission_evidence.len()));
    out
}
