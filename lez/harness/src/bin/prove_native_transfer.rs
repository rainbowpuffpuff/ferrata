//! LEZ's own yardstick for P.4: the cheapest private transaction the wallet of LEZ v0.4.0-rc1
//! makes, proven the way that wallet proves it. A deshielded native transfer: a private account,
//! which also pays the fee (the wallet requires a private fee payer), sends 5,000 native units to a
//! public account. No program runs privately (the native token runs inside LEZ's
//! privacy-preserving circuit), so LEZ's circuit is the only session proven.
//!
//! The wallet (`lez/wallet/src/lib.rs`, `send_proven`; `account_manager.rs`) builds every private
//! transaction with: dummy inputs up to `PADDED_PRIVATE_ACTIONS` = 7 private actions (here one
//! real account and six dummies, each with a real ML-KEM-768 epk toward a throwaway key and a
//! random 512-byte ciphertext), every note padded to `CIPHERTEXT_PAD_SIZE` = 512 bytes, the fee
//! paid inside the proof from the private payer with the unspent reserve refunded to the payer's
//! alias (`refund_to: None` and a recovery note, `fee_recovery`), and the payer presented blinded.
//! The recipient's public actor and LEZ's fee inbox are the public actors; nothing is signed.
//!
//! * `prove_native_transfer [--shape wallet|bare] [--dummies N] [--tx-out FILE] [--allow-dev-mode]`:
//!   prove (`RISC0_DEV_MODE` unset or 0: for real, through r0vm), then settle through LEZ's
//!   private settlement. `wallet` (the default) is the wallet's transaction above; `bare` is the
//!   same transfer as this repository's harness builds its private transactions
//!   (`Chain::prove_and_settle_private_fresh`): no dummy inputs, no padding, no recovery note, the
//!   refund to a public account. `--dummies N`, `--padding N|none` and `--recovery yes|no` override
//!   the shape's number of dummy inputs, note padding and fee recovery note (to tell their costs
//!   apart; the refund's destination stays the shape's).
//! * `prove_native_transfer --settle FILE`: rebuild the same state and settle a transaction
//!   written by `--tx-out`; LEZ's settlement verifies its receipt (a fake one only in dev mode).
//! * `--image-ids`: print the circuit's image id and stop.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    io::Read,
    time::Instant,
};

use fee_core::BlockFeeSummary;
use harness::{
    private::*,
    settle::{classify, opening_fee_state, settle_private_transaction, LeeTransaction},
    *,
};
use lee::{
    privacy_preserving_transaction::{circuit::ProgramCatalog, Message as PrivateMessage, WitnessSet},
    test_utils::{no_seal, update_witness},
    Account, AccountId, Actor, PrivacyPreservingTransaction, ProvingInput, PublicExecutionContext, Recipient,
    RecipientEncryption, SenderPresentation, Simulation,
};
use lee_core::{
    encryption::{Ciphertext, EncryptedNote, MlKem768EncapsulationKey},
    execution_state::TransactionEntry,
    native_token, Commitment, DummyInput, DummyOutput, EphemeralSecretKey, FeeTransfer, NullifierPublicKey,
    NullifierSecretKey, PrivateAccountKind, RootCall, SharedSecretKey,
};

/// The wallet's `PADDED_PRIVATE_ACTIONS` and `CIPHERTEXT_PAD_SIZE` (LEZ v0.4.0-rc1).
const PADDED_PRIVATE_ACTIONS: usize = 7;
const CIPHERTEXT_PAD_SIZE: u32 = 512;
const RECIPIENT: AccountId = AccountId::new([0x42; 32]);
const AMOUNT: u128 = 5_000;
const BALANCE: u128 = 1_000_000_000_000;
const RESERVE: u128 = 100_000_000;
const TIME_MS: u64 = 1_762_819_200_000;

fn usage() -> ! {
    eprintln!(
        "usage: prove_native_transfer [--shape wallet|bare] [--dummies N] [--padding N|none] [--recovery yes|no] \
         [--tx-out FILE] [--allow-dev-mode] \
         | --settle FILE | --image-ids"
    );
    std::process::exit(2)
}

fn random_vec(len: usize) -> Vec<u8> {
    let mut bytes = vec![0; len];
    fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes)).expect("/dev/urandom");
    bytes
}

fn random_bytes() -> [u8; 32] {
    random_vec(32).try_into().unwrap()
}

/// `account_manager::dummy_output`: a fresh commitment seed and a dummy note, random bytes of the
/// pad's size with a real ML-KEM epk toward a throwaway key.
fn dummy_output() -> DummyOutput {
    let throwaway = MlKem768EncapsulationKey::from_seed(&random_bytes(), &random_bytes());
    let (_, epk) = SharedSecretKey::encapsulate(&throwaway);
    DummyOutput {
        commitment_seed: random_bytes(),
        note: EncryptedNote { ciphertext: Ciphertext::from_inner(random_vec(CIPHERTEXT_PAD_SIZE as usize)), epk },
    }
}

/// The state the transfer is proven against and settled on: the harness's chain, the recipient as
/// an existing empty public account, and the funded private sender. Deterministic.
fn setup() -> (Chain, PrivateAccount) {
    let mut chain = Chain::new();
    chain.state = std::mem::take(&mut chain.state).with_public_accounts([(RECIPIENT, Account::default())]);
    let sender = chain.add_private_account(0x91, BALANCE);
    (chain, sender)
}

fn recipient_balance(chain: &Chain) -> u128 {
    native_token::decode_balance(&chain.actor_state(Actor::native_balance(RECIPIENT)).into_inner()).unwrap()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mut tx_out, mut settle_from, mut allow_dev_mode, mut only_ids) = (None, None, false, false);
    let (mut shape, mut dummies, mut padding, mut recovery) = ("wallet".to_string(), None, None, None);
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--tx-out" => tx_out = Some(it.next().unwrap_or_else(|| usage()).clone()),
            "--settle" => settle_from = Some(it.next().unwrap_or_else(|| usage()).clone()),
            "--shape" => shape = it.next().unwrap_or_else(|| usage()).clone(),
            "--dummies" => {
                dummies = Some(it.next().unwrap_or_else(|| usage()).parse::<usize>().unwrap_or_else(|_| usage()))
            }
            "--padding" => {
                padding = Some(match it.next().unwrap_or_else(|| usage()).as_str() {
                    "none" => None,
                    n => Some(n.parse::<u32>().unwrap_or_else(|_| usage())),
                })
            }
            "--recovery" => {
                recovery = Some(match it.next().unwrap_or_else(|| usage()).as_str() {
                    "yes" => true,
                    "no" => false,
                    _ => usage(),
                })
            }
            "--allow-dev-mode" => allow_dev_mode = true,
            "--image-ids" => only_ids = true,
            _ => usage(),
        }
    }
    if !matches!(shape.as_str(), "wallet" | "bare") || (settle_from.is_some() && (tx_out.is_some() || allow_dev_mode)) {
        usage();
    }
    let dev_mode = matches!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1") | Ok("true"));
    println!(
        "LEZ privacy-preserving circuit image {} ({} ELF bytes); RISC0_DEV_MODE {}",
        image_id_hex(lee::PRIVACY_PRESERVING_CIRCUIT_ID),
        lee::PRIVACY_PRESERVING_CIRCUIT_ELF.len(),
        if dev_mode { "on" } else { "off" }
    );
    if only_ids {
        return;
    }
    assert!(
        settle_from.is_some() || !dev_mode || allow_dev_mode,
        "unset RISC0_DEV_MODE (or set it to 0) for a real proof; --allow-dev-mode tries the commands with a fake receipt"
    );
    let (mut chain, sender) = setup();
    if let Some(path) = &settle_from {
        settle(&mut chain, path);
        return;
    }

    let wallet = shape == "wallet";
    let dummies = dummies.unwrap_or(if wallet { PADDED_PRIVATE_ACTIONS - 1 } else { 0 });
    let padding = padding.unwrap_or(wallet.then_some(CIPHERTEXT_PAD_SIZE));
    let recovery = recovery.unwrap_or(wallet);
    let sender_actor = Actor::native_balance(sender.id());
    let inbox = system_accounts::fee_inbox_account_id();
    let public_actors = vec![Actor::native_balance(RECIPIENT), Actor::native_balance(inbox)];
    let public_actor_states: HashMap<Actor, lee::ActorState> =
        public_actors.iter().map(|a| (*a, chain.actor_state(*a))).collect();
    let membership = chain
        .state
        .get_proof_for_commitment(&Commitment::new(&sender.id(), &sender.account))
        .expect("the sender's commitment is in the state");
    let mut witness = update_witness(&sender.keys, sender.account.clone(), membership);
    witness.random_seed = random_bytes();
    let opening = random_bytes();
    let commitment_root = chain.state.commitment_root();
    let input = ProvingInput {
        root: TransactionEntry::Sync(RootCall {
            to: sender_actor,
            message: borsh::to_vec(&native_token::Message::Transfer { to: RECIPIENT, amount: AMOUNT }).unwrap(),
        }),
        claims: Vec::new(),
        fee: Some(FeeTransfer {
            payer: sender.id(),
            recipient: inbox,
            amount: RESERVE,
            refund_to: if wallet { None } else { Some(chain.payer) },
        }),
        public_context: PublicExecutionContext::new(public_actors, []),
        private_witnesses: vec![witness],
        dummy_inputs: (0..dummies)
            .map(|_| DummyInput { nullifier_seed: random_bytes(), commitment_root, output: dummy_output() })
            .collect(),
        ciphertext_padding: padding,
        recoveries: if recovery {
            vec![RecipientEncryption {
                recipient: Recipient {
                    npk: NullifierPublicKey::from(&NullifierSecretKey::from(&sender.keys.ask)),
                    vpk: sender.keys.vpk(),
                    kind: PrivateAccountKind::Regular,
                    opening: Some(opening),
                },
                esk: EphemeralSecretKey(random_bytes()),
            }]
        } else {
            Vec::new()
        },
        private_promotions: BTreeSet::new(),
    };
    println!(
        "native Transfer of {AMOUNT} from a private account to a public one; shape {shape}: 1 private account, \
         {dummies} dummy inputs, ciphertext padding {:?}, {} recovery note(s), refund to {}",
        input.ciphertext_padding,
        input.recoveries.len(),
        if wallet { "the payer's alias" } else { "a public account" }
    );
    let simulation = Simulation { public_actor_states, admitted_accounts: None };
    let sender_id = sender.id();
    let presentation = move |actor: Actor| {
        if actor.account_id == sender_id {
            SenderPresentation::Blinded(opening)
        } else {
            SenderPresentation::Canonical
        }
    };
    let started = Instant::now();
    let (output, proof) =
        lee::execute_and_prove(input, &simulation, &ProgramCatalog::default(), presentation, |_, _| false, no_seal)
            .expect("proves");
    let prove_time = started.elapsed();
    let proof_bytes = proof.clone().into_inner().len();
    let message = PrivateMessage::from_circuit_output(BTreeMap::new(), output);
    let tx = PrivacyPreservingTransaction::new(message, WitnessSet::from_raw_parts(vec![], proof));
    let private_actions = tx.message.execution.private_actions.len();
    let lee_tx = LeeTransaction::PrivacyPreserving(tx);
    let bytes = borsh::to_vec(&lee_tx).unwrap();
    let (what, receipt) = if dev_mode {
        ("DRY RUN, NOT A PROOF (RISC0_DEV_MODE=1)", "fake receipt")
    } else {
        ("REAL PROOF", "succinct InnerReceipt, borsh")
    };
    println!(
        "{what}: execute_and_prove {:.1} s; receipt ({receipt}) {proof_bytes} bytes; transaction {} bytes; \
         {private_actions} private actions",
        prove_time.as_secs_f64(),
        bytes.len()
    );
    if let Some(path) = &tx_out {
        fs::write(path, &bytes).unwrap_or_else(|e| panic!("writing {path}: {e}"));
        println!("wrote the proven transaction to {path} ({} bytes)", bytes.len());
    }
    let view = classify(&lee_tx).expect("classifies");
    let opening_state = opening_fee_state(&chain.state);
    let mut summary = BlockFeeSummary::default();
    let (_, charge) = settle_private_transaction(
        &lee_tx,
        &view,
        &mut chain.state,
        &opening_state,
        chain.block_id,
        TIME_MS,
        &mut summary,
    )
    .expect("settlement accepts the transaction");
    assert!(charge.revert.is_none(), "the public part reverted: {:?}", charge.revert);
    assert_eq!(recipient_balance(&chain), AMOUNT, "the recipient is credited");
    if let LeeTransaction::PrivacyPreserving(tx) = &lee_tx {
        println!("{}", describe_public_outputs(tx));
    }
    println!(
        "settled: execution gas {} (PRIVATE_VERIFY_GAS {} + public part {} cycles), storage gas {}, base fee {}",
        grouped(charge.gas_exec),
        grouped(fee_core::market::PRIVATE_VERIFY_GAS),
        grouped(charge.cycles),
        grouped(charge.gas_stor),
        grouped(charge.fee_base as u64)
    );
}

/// Settle the transaction in `path` on the rebuilt state, as the proving run settles its own.
fn settle(chain: &mut Chain, path: &str) {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("reading {path}: {e}"));
    let lee_tx: LeeTransaction = borsh::from_slice(&bytes).expect("a borsh LeeTransaction");
    let LeeTransaction::PrivacyPreserving(tx) = &lee_tx else { panic!("{path} holds a public transaction") };
    let view = classify(&lee_tx).expect("classifies");
    let opening = opening_fee_state(&chain.state);
    let mut summary = BlockFeeSummary::default();
    let started = Instant::now();
    let (_, charge) =
        settle_private_transaction(&lee_tx, &view, &mut chain.state, &opening, chain.block_id, TIME_MS, &mut summary)
            .expect("settlement accepts the transaction");
    let proof_bytes = tx.witness_set.proof().clone().into_inner().len();
    let verified = if matches!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1") | Ok("true")) {
        "RISC0_DEV_MODE on: a fake receipt passes too"
    } else {
        "receipt verified"
    };
    println!(
        "SETTLED FROM {path} ({verified}): receipt {proof_bytes} bytes, transaction {} bytes, settled in {:.1} s",
        bytes.len(),
        started.elapsed().as_secs_f64()
    );
    assert!(charge.revert.is_none(), "the public part reverted: {:?}", charge.revert);
    assert_eq!(recipient_balance(chain), AMOUNT, "the recipient is credited");
}
