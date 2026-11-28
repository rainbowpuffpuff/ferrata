//! An interrupted user-side operation consumes, corrupts and exposes nothing (RFP-022
//! Reliability #7: "An interrupted user-side operation does not consume, corrupt, or expose the
//! user's entitlement"; `docs/requirements-status.md`, RFP-022 R.7, in part today: "no test of a
//! client killed between proof and sending").
//!
//! What is tested, at the protocol level, in RISC Zero dev mode (the zkVM executes the guests; a
//! proof is a dev-mode receipt), on the private leg, where a client really proves before it sends.
//! The claim is vault A's leaf 5 (1 ETH, private kind): the only deposit a private account can
//! claim as its own root call.
//!
//! * *Claim proven, then dropped* — what a client killed between its proof and its sending
//!   leaves: the deposit's nullifier is unspent, no account changed, nothing of the claim is in
//!   the zone. A fresh client holding only its seeds (the LEZ account seed and the bridge seed)
//!   rediscovers the deposit — the recovery RFP-022 Reliability #5 asks for, the bridge core's
//!   `inbound::recover` — claims it, and is accepted once; a second claim is refused with the
//!   double-claim code (250). The dropped transaction, sent late after that claim landed, is
//!   refused by LEZ's commitment/nullifier re-use checks before anything runs, and changes
//!   nothing: an old proof cannot double-spend or corrupt.
//! * *Burn proven, then dropped*: the private holding is unchanged and no burn record exists;
//!   burning again later (the same counter, the same note, since nothing was published) is
//!   accepted once, the note written once at `burn(cid, r)`; a third attempt of the same
//!   note is refused; the dropped proof, sent late, is refused and changes nothing.
//! * *A burn sent, its client killed before it wrote the note down locally*: a fresh client from
//!   the seed alone finds the burn note on chain — in the zone's blocks, as the checkpoint
//!   program reads them, and at `burn(cid, r)` — and computes everything a release needs
//!   (the burn counter, the recipient, `sOut`, the burn leaf at its collected index, `NOut`; the
//!   bridge core's `outbound::recover_burns`). The entitlement is intact.
//!
//! Writes `results/interrupted-operations-claim.md` and `results/interrupted-operations-burn.md`.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::PathBuf,
};

#[cfg(not(feature = "lez-040"))]
use checkpoint_core::lez::{parse_block, Actor as CpActor, Origin, TxKind, Visitor};
#[cfg(feature = "lez-040")]
use checkpoint_core::lez_0_4_0::{parse_block, Actor as CpActor, Origin, TxKind, Visitor};
#[cfg(feature = "lez-040")]
mod lez_block040;
use checkpoint_core::{
    wire::BridgeMessage as CpMessage,
};
use fee_core::BlockFeeSummary;
use ferrata_bridge_core::{
    chain_id_u256,
    inbound::{recover, DestKind, VaultDeposit},
    outbound::{
        burn_commitment, burn_leaf, burn_nullifier, burn_secret, burn_tag, recover_burns, xor_memo, BurnNote,
        PublishedBurn,
    },
    BridgeKeys, Hash,
};
use ferrata_lez_bridge_wire::{
    burn_actor, burn_mac_v2, code, nf_out_v2, own_actor, word_of_u128, BridgeMessage, BridgeState, BurnRecord,
    HolderState, Holding, Note, NullifierState,
};
use harness::{
    attest::execute,
    bridge::{
        bridge_program, catalog, fee_recipient, holder_of, private_account_id, private_burn_transition, private_keys,
        private_post_state, relayer, Deposit, World, CHAIN_ID, DENOMINATIONS, ETH, RELAY_FEES,
    },
    compat,
    private::{PrivateAccount, ALIAS_OPENING},
    settle::{classify, opening_fee_state, settle_private_transaction, Charge, LeeTransaction},
    Chain,
};
use lee::{
    privacy_preserving_transaction::{circuit::ProgramCatalog, Message as PrivateMessage, WitnessSet},
    test_utils::{no_seal, update_witness},
    Actor, PrivacyPreservingTransaction, ProvingInput, PublicExecutionContext, SenderPresentation, Simulation,
};
use lee_core::{execution_state::TransactionEntry, program::TransactionEvent, Commitment, RootCall};

/// The private-fee reserve every proof here works out of, as the other bridge suites' (`bridge_burn.rs`).
const RESERVE: u128 = 100_000_000;
/// The wallet that deposited vault A's leaves 5–7 and burns here: its bridge seed
/// (`harness/src/bridge.rs`, `World::new`, derives their `s` from `BridgeKeys::from_bridge_seed`).
const WALLET_BRIDGE_SEED: [u8; 32] = [0x5a; 32];
/// The burner's bridge seed (`bridge_burn.rs`'s wallet).
const BURNER_BRIDGE_SEED: [u8; 32] = [0x77; 32];
/// Where the burned ETH goes on Ethereum (`bridge_burn.rs`'s recipient).
const RECIPIENT: [u8; 20] = [0xe1; 20];

fn require_dev_mode() {
    assert_eq!(std::env::var("RISC0_DEV_MODE").as_deref(), Ok("1"), "run these tests with RISC0_DEV_MODE=1");
}

fn results_path(name: &str) -> PathBuf {
    harness::results_dir().join(name)
}

/// Prove (dev mode) one private transaction and return it **without settling**: what a client
/// holds between proving and sending. The construction is `harness::private`'s
/// `prove_and_settle_private` (its bare shape: no padding, the unspent reserve refunded to the
/// public payer, fixed alias opening) with the settlement taken out — no harness function proves
/// without settling, so the two halves are placed here again; nothing library-side had to change.
fn prove_dropped(
    chain: &Chain,
    sender: &PrivateAccount,
    root: Actor,
    message: Vec<u8>,
    public_actors: Vec<Actor>,
    programs: &ProgramCatalog,
    reserve: u128,
) -> Result<LeeTransaction, String> {
    let mut public_actors = public_actors;
    public_actors.push(Actor::native_balance(system_accounts::fee_inbox_account_id()));
    let public_actor_states: HashMap<Actor, lee::ActorState> =
        public_actors.iter().map(|a| (*a, chain.actor_state(*a))).collect();
    let membership = chain
        .state
        .get_proof_for_commitment(&Commitment::new(&sender.id(), &sender.account))
        .ok_or("the sender's commitment is not in the state")?;
    let input = ProvingInput {
        root: TransactionEntry::Sync(RootCall { to: root, message }),
        claims: Vec::new(),
        fee: Some(compat::fee_transfer(
            sender.id(),
            system_accounts::fee_inbox_account_id(),
            reserve,
            Some(chain.payer),
        )),
        public_context: PublicExecutionContext::new(compat::declared(public_actors, []), []),
        private_witnesses: vec![{
            let witness = update_witness(&sender.keys, sender.account.clone(), membership);
            let mut witness = compat::authorize_root(witness, root);
            witness.random_seed = [0; 32];
            witness
        }],
        dummy_inputs: Vec::new(),
        ciphertext_padding: None,
        recoveries: Vec::new(),
        private_promotions: BTreeSet::new(),
    };
    let simulation = Simulation { public_actor_states, admitted_accounts: None };
    let presentation = |_: Actor| SenderPresentation::Blinded(ALIAS_OPENING);
    let (output, proof) = lee::execute_and_prove(input, &simulation, programs, presentation, |_, _| false, no_seal)
        .map_err(|e| format!("proving failed: {e}"))?;
    let message = PrivateMessage::from_circuit_output(BTreeMap::new(), output);
    let tx = PrivacyPreservingTransaction::new(message, WitnessSet::from_raw_parts(vec![], proof));
    Ok(LeeTransaction::PrivacyPreserving(tx))
}

/// Settle a kept private transaction as the sequencer does when a late submission reaches it
/// (what `harness::private` runs after proving). Settlement moves the state only after every
/// check passes, so a refusal leaves the chain exactly as it was.
fn settle_late(w: &mut World, tx: &LeeTransaction) -> Result<(Vec<TransactionEvent>, Charge), String> {
    let view = classify(tx)?;
    let opening = opening_fee_state(&w.chain.state);
    let mut summary = BlockFeeSummary::default();
    let settled =
        settle_private_transaction(tx, &view, &mut w.chain.state, &opening, w.chain.block_id, w.time, &mut summary);
    if settled.is_ok() {
        w.chain.block_id += 1;
    }
    settled
}

/// The wallet's `i`-th burn note under `cid`, for `RECIPIENT`: as `bridge_burn.rs`'s `note` makes
/// one (D12's MAC over the memo, the core's key derivations). Also returns `sOut`.
fn wallet_note(keys: &BridgeKeys, cid: &Hash, vault: &[u8; 20], i: u64, token: [u8; 20], amount: u128) -> (Note, Hash) {
    let r = burn_tag(&keys.view, cid, i);
    let s_out = burn_secret(&keys.auth, cid, &r);
    let c_out = burn_commitment(&s_out, &chain_id_u256(CHAIN_ID), vault, &RECIPIENT);
    let memo = xor_memo(&keys.view, &r, &RECIPIENT);
    let amount = word_of_u128(amount);
    let mac = burn_mac_v2(&keys.view, &r, &memo, &token, &amount, &c_out);
    (Note { r, memo, mac, token, amount, c_out }, s_out)
}

/// A LEZ block (`borsh(Block)`) holding `txs` (`borsh(LeeTransaction)` each), as `bridge_burn.rs`
/// builds one for the checkpoint's parser: what the zone shows a fresh client.
#[cfg(not(feature = "lez-040"))]
fn block(txs: &[&[u8]], reverted: &[u64]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&7u64.to_le_bytes());
    b.extend_from_slice(&[0u8; 32]);
    b.extend_from_slice(&[0u8; 32]);
    b.extend_from_slice(&1_700_000_000_000u64.to_le_bytes());
    b.extend_from_slice(&(txs.len() as u32).to_le_bytes());
    for t in txs {
        b.extend_from_slice(t);
    }
    b.extend_from_slice(&(reverted.len() as u32).to_le_bytes());
    for r in reverted {
        b.extend_from_slice(&r.to_le_bytes());
    }
    b.push(2);
    b
}

/// [`block`] in LEZ v0.4.0's layout (tests/lez_block040).
#[cfg(feature = "lez-040")]
fn block(txs: &[&[u8]], reverted: &[u64]) -> Vec<u8> {
    lez_block040::block(7, 1_700_000_000_000, txs, reverted)
}

/// The checkpoint's view of one block (`bridge_burn.rs`'s collector): every call to the bridge
/// program the block shows in the clear.
struct Calls<'a> {
    bridge: Hash,
    calls: Vec<(u32, Origin, &'a [u8])>,
}

impl<'a> Visitor<'a> for Calls<'a> {
    fn tx(&mut self, _index: u32, _kind: TxKind, _bytes: &'a [u8]) {}
    fn call(&mut self, index: u32, origin: Origin, _from: Option<CpActor<'a>>, to: CpActor<'a>, message: &'a [u8]) {
        if *to.program_account_id == self.bridge {
            self.calls.push((index, origin, message));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The claim
// ---------------------------------------------------------------------------------------------

#[test]
fn a_claim_interrupted_between_its_proof_and_its_sending() {
    require_dev_mode();
    let mut w = World::with_roots();
    let (cid, va) = (w.cid_a, w.vault_a.address);
    let seed = w.private_seed;
    // The private account the deposit names: the harness keeps the account seed and the bridge
    // seed apart; a wallet holds both.
    let account = w.chain.add_private_account(seed, 1_000_000_000_000);
    let id = account.id();
    let me = own_actor(w.program, id);
    assert_eq!(id.value(), &w.private_deposit.dest, "leaf 5 is this account's deposit");

    // A fresh client holding only the seed: the bridge keys from the bridge seed
    // (`ferrata_bridge_core::derive_keys`), `s` re-derived per published deposit, the commitment
    // recomputed over the wallet's candidate destinations (bridge core `inbound::recover`).
    let vault_deposits = |w: &World| -> Vec<VaultDeposit> {
        // The deposits this wallet could own: the three of `World::new`'s own depositor, whose
        // counters j are 0, 1 and 2 (`harness/src/bridge.rs`, `own`). Vault A's first five leaves
        // are the shared vector's, another depositor's: no seed-derived `s` matches them.
        [&w.private_deposit, &w.payer_deposit, &w.odd_deposit]
            .iter()
            .map(|d| VaultDeposit {
                depositor: d.depositor,
                j: d.leaf_index - 5,
                leaf_index: d.leaf_index,
                token: d.token,
                amount: d.amount,
                commitment: d.commitment(),
            })
            .collect()
    };
    let dest_of = |kind: DestKind, index: u32| match (kind, index) {
        (DestKind::Private, 0) => *private_account_id(seed).value(),
        (DestKind::Public, 0) => *w.chain.payer.value(),
        _ => unreachable!("the wallet looks one account of each kind ahead (gap 1)"),
    };
    let owned = recover(
        &BridgeKeys::from_bridge_seed(&WALLET_BRIDGE_SEED).auth,
        &chain_id_u256(CHAIN_ID),
        &va,
        &vault_deposits(&w),
        1,
        dest_of,
    );
    assert_eq!(owned.len(), 3, "the seed finds its three deposits of vault A");
    let leaf5 = owned.iter().find(|o| o.deposit.leaf_index == w.private_deposit.leaf_index).expect("leaf 5");
    assert_eq!(leaf5.s, w.private_deposit.s, "the deposit secret, from the seed");
    assert_eq!(leaf5.deposit.commitment, w.private_deposit.commitment());
    assert_eq!((leaf5.kind, leaf5.dest), (DestKind::Private, *id.value()), "the destination, from the seed");
    let leaf6 = owned.iter().find(|o| o.deposit.leaf_index == w.payer_deposit.leaf_index).expect("leaf 6");
    assert_eq!((leaf6.kind, leaf6.dest), (DestKind::Public, *w.chain.payer.value()));

    // --- Stage 1: the client proves the claim and is killed --------------------------------
    let deposit = Deposit {
        leaf_index: leaf5.deposit.leaf_index,
        depositor: leaf5.deposit.depositor,
        token: leaf5.deposit.token,
        amount: leaf5.deposit.amount,
        s: leaf5.s,
        kind: leaf5.kind,
        dest: leaf5.dest,
    };
    let n = leaf5.nullifier;
    let witness = w.vault_a.witness(&deposit, 8);
    let message = w.claim_privately(&cid, 8, witness);
    let publics = w.private_claim_actors(&cid, 8, &va, &deposit);
    // What the zone holds before anything is sent.
    let (root_before, token_before, config_before) =
        (w.root_state(&cid, 8), w.token_state(&cid, &ETH), w.config_state(&cid));
    let dropped =
        prove_dropped(&w.chain, &account, me, borsh::to_vec(&message).unwrap(), publics.clone(), &catalog(), RESERVE)
            .expect("the claim proves while the deposit is unspent");
    // Nothing consumed, nothing exposed: the zone is exactly what it was.
    assert_eq!(w.nullifier(&cid, &va, &n), None, "the deposit's N is still unspent");
    assert!(w.holder(&cid, id).is_none(), "no holder state appeared at the destination");
    assert_eq!((w.balance(relayer(), &cid), w.balance(fee_recipient(), &cid)), (0, 0), "no fee was minted");
    assert_eq!(w.root_state(&cid, 8), root_before, "the root the claim reads is untouched");
    assert_eq!(w.token_state(&cid, &ETH), token_before, "the token's gate is untouched");
    assert_eq!(w.config_state(&cid), config_before, "the configuration's gate is untouched");
    assert!(
        w.chain.state.get_proof_for_commitment(&Commitment::new(&id, &account.account)).is_some(),
        "the client's whole account — balance, nonce, states — is exactly what it was"
    );

    // --- Stage 2: a fresh client, from the seed alone, rediscovers and claims ----------------
    let rediscovered = recover(
        &BridgeKeys::from_bridge_seed(&WALLET_BRIDGE_SEED).auth,
        &chain_id_u256(CHAIN_ID),
        &va,
        &vault_deposits(&w),
        1,
        |kind, index| match (kind, index) {
            (DestKind::Private, 0) => *private_account_id(seed).value(),
            (DestKind::Public, 0) => *w.chain.payer.value(),
            _ => unreachable!("gap 1"),
        },
    );
    assert_eq!(rediscovered, owned, "a restarted client finds exactly the same deposits");
    let run = w
        .chain
        .prove_and_settle_private(
            &account,
            me,
            borsh::to_vec(&message).unwrap(),
            publics.clone(),
            &catalog(),
            RESERVE,
            w.time,
        )
        .expect("the fresh client's claim settles");
    assert!(run.charge.revert.is_none());
    // Accepted once, with exactly the accounts it should touch and nothing else: the nullifier
    // record (`bridge_private.rs` for these values), the private holding, the relay fee of
    // schedule 0 (leaf 5's), and no public credit at the destination.
    assert_eq!(
        w.nullifier(&cid, &va, &n),
        Some(NullifierState { config_id: cid, token: ETH, leaf_count: 8, amount: DENOMINATIONS[1] })
    );
    assert_eq!(w.balance(relayer(), &cid), RELAY_FEES[1]);
    assert_eq!(w.balance(fee_recipient(), &cid), 0, "leaf 5 is under schedule 0: no mint fee");
    assert!(w.holder(&cid, id).is_none(), "the credit is private: still no public holder state");
    let holding =
        HolderState { holdings: vec![Holding { config_id: cid, token: ETH, amount: DENOMINATIONS[1] }], pending: None };
    let claimed = private_post_state(&w.chain, &private_keys(seed), &account.account, RESERVE, &holding)
        .expect("the chain holds the claimed account: 1 ETH, privately");

    // --- Stage 3: a second claim is refused with the double-claim code ----------------------
    // As `bridge_private.rs`'s replay: the public part of the proof refuses (250), so nothing can
    // be proven and nothing is sent.
    let again = w.chain.prove_and_settle_private(
        &account,
        me,
        borsh::to_vec(&message).unwrap(),
        publics,
        &catalog(),
        RESERVE,
        w.time,
    );
    let err = again.err().expect("the claimed deposit cannot be claimed again");
    println!("second claim: {err}");
    assert!(
        err.contains(&u32::from(code::ALREADY_CLAIMED).to_string()),
        "the refusal carries the double-claim code ({}): {err}",
        u32::from(code::ALREADY_CLAIMED)
    );

    // --- Stage 4: the dropped transaction, sent late -----------------------------------------
    // LEZ checks a private transaction against the live state before it runs anything: the
    // landed claim has already taken the dropped one's commitment and nullifier (both are
    // deterministic in the account's pre-state), so the re-use check fires ("… already seen";
    // `validated_state_diff/mod.rs`, commitment freshness then nullifier uniqueness, v0.4.0-rc1)
    // and settlement never reaches the state it would have applied.
    let (nullifier_after, relayer_after) = (w.nullifier(&cid, &va, &n), w.balance(relayer(), &cid));
    let err =
        settle_late(&mut w, &dropped).expect_err("the proof of an already-spent account state cannot be settled again");
    println!("the dropped claim, sent late: {err}");
    assert!(err.contains("already seen"), "refused at LEZ's re-use check, before anything runs: {err}");
    assert_eq!(w.nullifier(&cid, &va, &n), nullifier_after, "no second spend");
    assert_eq!(w.balance(relayer(), &cid), relayer_after, "no second fee");
    private_post_state(&w.chain, &private_keys(seed), &claimed, 0, &holding)
        .expect("the landed claim's private state is untouched by the refused late transaction");

    let table = "| stage | interrupted at | outcome |\n|---|---|---|\n".to_string()
        + "| seed recovery | — | the fresh client finds all three of its deposits of vault A; leaf 5: `s`, kind, \
           destination, leaf and N, all equal to the world's own (bridge core `inbound::recover`) |\n"
        + "| claim proven, dropped | between the proof and the sending | N unspent, no holder state, no fee minted, \
           root/token/configuration untouched, the client's committed account exactly what it was: nothing consumed, \
           nothing exposed |\n"
        + "| fresh client claims | — | accepted once: N spent with exactly (A, ETH, 8 leaves, 1 ETH), the holding \
           private, the relay fee public |\n"
        + "| second claim | — | refused with the double-claim code 250 in the proving error, as `bridge_private.rs`'s \
           replay |\n"
        + "| the dropped transaction, sent late | after the fresh client's claim landed | refused at LEZ's \
           commitment/nullifier re-use check before anything runs; every state exactly as after the accepted claim: an \
           old proof cannot double-spend or corrupt |\n";
    println!("{table}");
    let dropped_bytes = borsh::to_vec(&dropped).unwrap().len();
    fs::write(
        results_path("interrupted-operations-claim.md"),
        format!(
            "# A claim interrupted between its proof and its sending \
             (`harness/tests/interrupted_operations.rs`), RISC Zero dev mode\n\n\
             RFP-022 Reliability #7. Bridge program image `{}`. Vault A's leaf 5 (1 ETH, private kind), claimed as the \
             private account it names.\n\n{table}\n\
             The dropped transaction: {} bytes. The claim that landed: execution gas {}, storage gas {}, \
             base fee {}.\n",
            harness::image_id_hex(harness::bridge::BRIDGE_ID),
            harness::grouped(dropped_bytes as u64),
            harness::grouped(run.charge.gas_exec),
            harness::grouped(run.charge.gas_stor),
            harness::grouped(run.charge.fee_base as u64),
        ),
    )
    .unwrap();
}

// ---------------------------------------------------------------------------------------------
// The burn
// ---------------------------------------------------------------------------------------------

#[test]
fn a_burn_interrupted_and_a_burn_whose_note_was_never_recorded() {
    require_dev_mode();
    let mut w = World::with_roots();
    let (cid, va) = (w.cid_a, w.vault_a.address);
    let seed = w.private_seed;
    let program = w.program;
    let account = w.chain.add_private_account(seed, 1_000_000_000_000);
    let id = account.id();
    let me = own_actor(program, id);

    // The holding the wallet will burn: its private claim of vault A's leaf 5 (1 ETH).
    let deposit = w.private_deposit.clone();
    let claim = w.claim_privately(&cid, 8, w.vault_a.witness(&deposit, 8));
    let claim_publics = w.private_claim_actors(&cid, 8, &va, &deposit);
    let claim_run = w
        .chain
        .prove_and_settle_private(
            &account,
            me,
            borsh::to_vec(&claim).unwrap(),
            claim_publics,
            &catalog(),
            RESERVE,
            w.time,
        )
        .expect("the private claim settles");
    assert!(claim_run.charge.revert.is_none());
    let holding =
        HolderState { holdings: vec![Holding { config_id: cid, token: ETH, amount: DENOMINATIONS[1] }], pending: None };
    let claimed = private_post_state(&w.chain, &private_keys(seed), &account.account, RESERVE, &holding)
        .expect("the claimed account's commitment");
    let holder = PrivateAccount { keys: private_keys(seed), account: claimed.clone() };

    // The wallet's 0th burn note under A (its counter is 0: it published nothing yet).
    let keys = BridgeKeys::from_bridge_seed(&BURNER_BRIDGE_SEED);
    let (note, s_out) = wallet_note(&keys, &cid, &va, 0, ETH, DENOMINATIONS[1]);
    let message = BridgeMessage::burn(cid, &note);
    let record = burn_actor(program, &cid, &note.r);

    // --- Stage 1: the wallet proves the burn and is killed ----------------------------------
    // The executor's own view of the burn (the proof's content: the debit, the delivery), then
    // the complete proven transaction, never sent.
    let executed = private_burn_transition(&w, id, claimed.data.actor_state(program).clone(), &message);
    assert_eq!(executed.exit_code, 0, "the burn runs: the debit and the delivery to the token");
    let burn_publics = w.burn_actors(me, &cid, &note)[1..].to_vec();
    let dropped = prove_dropped(
        &w.chain,
        &holder,
        me,
        borsh::to_vec(&message).unwrap(),
        burn_publics.clone(),
        &catalog(),
        RESERVE,
    )
    .expect("the burn proves against the intact holding");
    // The entitlement is unchanged: the committed account is still the claimed one, and the zone
    // holds no burn record.
    private_post_state(&w.chain, &private_keys(seed), &claimed, 0, &holding)
        .expect("the holding is unchanged: the dropped proof consumed nothing");
    assert!(w.chain.actor_state(record).is_empty(), "no burn record: nothing reached the zone");

    // --- Stage 2: the client restarts and burns again ----------------------------------------
    // Nothing was published, so its counter is still 0 and it makes the same note again.
    let retry = w
        .chain
        .prove_and_settle_private(
            &holder,
            me,
            borsh::to_vec(&message).unwrap(),
            burn_publics,
            &catalog(),
            RESERVE,
            w.time,
        )
        .expect("burning again later is accepted");
    assert!(retry.charge.revert.is_none());
    assert_eq!(
        borsh::from_slice::<BridgeState>(&w.chain.actor_state(record)).unwrap(),
        BridgeState::Burn(BurnRecord { config_id: cid, note }),
        "accepted once: the note, written once at burn(cid, r)"
    );
    let emptied = HolderState::default();
    let burned = private_post_state(&w.chain, &private_keys(seed), &claimed, RESERVE, &emptied)
        .expect("the holding is gone, exactly once");

    // --- Stage 3: the same note a third time is refused --------------------------------------
    // The holding is gone, so the root call's own debit fails and it forwards to the public
    // `holder(account)`, which refuses INSUFFICIENT_BALANCE (254) — `guests/bridge`, the
    // `holder(owner)` arm of `burn`. It cannot reach the token, so the zone is not touched.
    let first = private_burn_transition(&w, id, burned.data.actor_state(program).clone(), &message);
    assert_eq!(first.exit_code, 0, "the root call forwards to the public holder (nothing to debit privately)");
    let forwarded = compat::receive_input(
        holder_of(program, &cid, id),
        Some(me),
        true,
        Default::default(), // holder(the private account) holds nothing, and never did
        borsh::to_vec(&message).unwrap(),
    );
    let at_holder = execute("the forwarded burn at holder(the private account)", bridge_program().elf(), &forwarded);
    assert_eq!(at_holder.exit_code, u32::from(code::INSUFFICIENT_BALANCE), "the note cannot burn twice");

    // --- Stage 4: the dropped proof, sent late -------------------------------------------------
    // The retry that landed already took the dropped one's commitment and nullifier (both are
    // deterministic in the claimed account's state), so LEZ's re-use check fires before anything
    // runs and the state is not applied.
    let err = settle_late(&mut w, &dropped).expect_err("the proof of an already-spent holding cannot be settled again");
    println!("the dropped burn, sent late: {err}");
    assert!(err.contains("already seen"), "refused at LEZ's re-use check, before anything runs: {err}");
    assert_eq!(
        borsh::from_slice::<BridgeState>(&w.chain.actor_state(record)).unwrap(),
        BridgeState::Burn(BurnRecord { config_id: cid, note }),
        "the burn record is exactly what the retry wrote"
    );
    private_post_state(&w.chain, &private_keys(seed), &claimed, RESERVE, &emptied)
        .expect("the emptied account is still the committed one");

    // --- Stage 5: the burn landed; the wallet died before it wrote the note down -------------
    // A fresh client holding only the seed scans the zone's blocks as the checkpoint program
    // does (`checkpoint-core`'s parser — the one that collects burns, and whose collection index
    // is the burn's leaf index: `rule.rs` appends at `state.burns.count`), finds its note in the
    // clear, and then reads everything a release needs out of the seed. The zone here: the
    // world's public transactions, the private claim, the private burn, each in its own block
    // (the harness settles every transaction in a block of its own).
    let mut zone: Vec<Vec<u8>> = w.txs.iter().map(|(_, bytes)| bytes.clone()).collect();
    zone.push(borsh::to_vec(&LeeTransaction::PrivacyPreserving(claim_run.tx.clone())).unwrap());
    zone.push(borsh::to_vec(&LeeTransaction::PrivacyPreserving(retry.tx.clone())).unwrap());
    let mut collected = 0u64;
    let mut found: Option<PublishedBurn> = None;
    for tx in &zone {
        let bytes = block(&[tx.as_slice()], &[]);
        let mut visitor = Calls { bridge: *program.value(), calls: Vec::new() };
        let summary = parse_block(&bytes, &mut visitor).expect("the checkpoint parses the block");
        for (index, origin, message) in &visitor.calls {
            if !origin.is_public_execution() || summary.is_reverted(*index) {
                continue;
            }
            if let Ok(Some(CpMessage::Burn(b))) = CpMessage::decode(message) {
                assert_eq!(b.config_id, cid, "this zone holds burns of one configuration only");
                let scanned =
                    BurnNote { r: b.r, memo: b.memo, mac: b.mac, token: b.token, amount: b.amount, c_out: b.c_out };
                found = Some(PublishedBurn { burn_index: collected, note: scanned });
                collected += 1;
            }
        }
    }
    assert_eq!(collected, 1, "exactly one burn of the configuration is in the zone");
    let published = found.expect("the burn note is on chain, in the clear");
    assert_eq!(
        (
            published.note.r,
            published.note.memo,
            published.note.mac,
            published.note.token,
            published.note.amount,
            published.note.c_out
        ),
        (note.r, note.memo, note.mac, note.token, note.amount, note.c_out),
        "the note the zone carries is the one the killed wallet made"
    );
    assert_eq!(
        borsh::from_slice::<BridgeState>(&w.chain.actor_state(record)).unwrap(),
        BridgeState::Burn(BurnRecord { config_id: cid, note }),
        "and `burn(cid, r)` holds the same note, the on-chain record of it"
    );
    // The recovery itself (RFP-022 Reliability #5's, the burn half): `kView` verifies the MAC and
    // decrypts the recipient, `kAuth` re-derives `sOut`, and `COut` authenticates the recipient.
    let fresh = BridgeKeys::from_bridge_seed(&BURNER_BRIDGE_SEED);
    let scan = recover_burns(&fresh, &cid, &chain_id_u256(CHAIN_ID), &va, &[published], 10);
    assert_eq!(scan.owned.len(), 1, "the seed finds its burn");
    let recovered = &scan.owned[0];
    assert_eq!(recovered.i, 0, "the burn counter, never advanced past the burn that landed");
    assert_eq!(scan.next_i, 1, "the next burn's counter");
    assert_eq!(recovered.recipient, RECIPIENT, "the recipient, decrypted from the memo (the MAC covers the memo)");
    assert_eq!(recovered.s_out, s_out, "the release secret, from the seed and the tag");
    assert_eq!(recovered.nullifier, burn_nullifier(&s_out), "D13's NOut");
    assert_eq!(recovered.nullifier, nf_out_v2(&s_out), "the release program's NOut");
    assert_eq!(
        recovered.leaf,
        burn_leaf(&cid, published.burn_index, &note.token, &note.amount, &note.c_out, &note.r),
        "the burn leaf, at the index the checkpoint collects the burn at"
    );
    assert_eq!(
        recovered.leaf,
        checkpoint_core::tree::burn_leaf(
            &cid,
            published.burn_index,
            &published.note.token,
            &published.note.amount,
            &published.note.c_out,
            &published.note.r
        ),
        "the checkpoint's own leaf of this collected burn"
    );

    let table = "| stage | interrupted at | outcome |\n|---|---|---|\n".to_string()
        + "| burn proven, dropped | between the proof and the sending | the holding unchanged (the committed account \
           still the claimed one), no `burn(cid, r)` record: nothing consumed, nothing exposed |\n"
        + "| client restarts, burns again | — | accepted once: the note written once at `burn(cid, r)`, the \
           holding gone exactly once |\n"
        + "| the note a third time | — | refused before the zone is touched: the private debit fails, the public \
           `holder(account)` refuses 254 (INSUFFICIENT_BALANCE) |\n"
        + "| the dropped proof, sent late | after the retry landed | refused at LEZ's commitment/nullifier \
           re-use check before anything runs; the zone exactly as after the accepted burn |\n"
        + "| burn sent, note never recorded | between the sending and the wallet's own record | a fresh client \
           from the seed alone finds the note in the zone's blocks and at `burn(cid, r)`, and recovers the \
           counter (0), the recipient, `sOut`, the burn leaf and `NOut` (bridge core `outbound::recover_burns`): \
           everything a release needs except the burn root and path, which a published checkpoint gives; the \
           entitlement is intact |\n";
    println!("{table}");
    let dropped_bytes = borsh::to_vec(&dropped).unwrap().len();
    fs::write(
        results_path("interrupted-operations-burn.md"),
        format!(
            "# A burn interrupted, and a burn whose note was never recorded \
             (`harness/tests/interrupted_operations.rs`), RISC Zero dev mode\n\n\
             RFP-022 Reliability #7. Bridge program image `{}`. The private claim of vault A's leaf 5 gives the \
             account its 1 ETH holding; the burn is the wallet's 0th note for {} on Ethereum.\n\n{table}\n\
             The dropped transaction: {} bytes. The burn that landed: execution gas {}, storage gas {}.\n\n\
             NOut of the note: `{}`.\n",
            harness::image_id_hex(harness::bridge::BRIDGE_ID),
            hex::encode(RECIPIENT),
            harness::grouped(dropped_bytes as u64),
            harness::grouped(retry.charge.gas_exec),
            harness::grouped(retry.charge.gas_stor),
            hex::encode(recovered.nullifier),
        ),
    )
    .unwrap();
}
