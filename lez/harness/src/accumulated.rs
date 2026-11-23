//! RFP-021 performance #9, "No action's cycle count or gas cost scales with accumulated on-chain
//! storage", for the actions not covered by `src/bin/perf_state.rs` §2–§3 (five statement types
//! and every update step): each runs on two copies of one world, one of which also holds K more of
//! the accounts that accumulate with use, and must be charged the same in both — every handler's
//! cycles, the execution gas and the storage gas. Shared by `src/bin/perf_state.rs` (`perf_state
//! actions`, K from 0 to 100,000, written to `lez/results/perf-state-actions.md` by
//! `bench/perf/state-actions.sh`) and `tests/costs_attestation.rs` (K = 1,000, pinned in CI).
//!
//! The actions:
//! - `Init` and its two adoption transactions, on a new chain ([`init_on`]);
//! - `VerifyAll` in a public transaction, with the `ApplyVerified` and `Record` it sends
//!   ([`verify_all_public`]), and in a private one, the proven update path
//!   ([`verify_all_private`]: the settled charge, and the private execution's own cycles in
//!   RISC Zero's executor, which is what the sender's device proves);
//! - `Succeed` and the `Spawn` it sends ([`succeed`]);
//! - the eight statement types of `lez/results/stage4-attest.md`, among them the absent account,
//!   the absent slot and the four facts ([`statement_requests`]);
//! - `Read` and `ReadPadded`, the read idiom of LEZ v0.4 (what a private consumer's transaction
//!   runs publicly), sent by a public reader of the consumer ([`read_record`]);
//! - the accumulator-leaf statement and the vault's slots under a record that binds a block hash
//!   (the `Request.header` path), on the synthetic vault ([`LeafWorld`]);
//! - the reference consumer: `CallAttest` with an empty reader and with a reader holding the
//!   previous reply ([`call_attest`]), `Claim` for a sender credited for the first time and for one
//!   credited before ([`claim`], [`place_balance`]), and its private read `AttestPrivately`
//!   ([`attest_privately`]);
//! - the reference consumer's vault pattern on FerrataVault v5's real deposits ([`VaultWorld`]):
//!   `ClaimDeposit` (→ `Attest` → reply → `CreditHolder`) for a first credit and to a holder
//!   credited before, `ClaimDepositPrivately` (its public part `Read` → `Spend` → `CreditHolder`),
//!   and the private claim's private transitions with the claimant's private `holdings` list
//!   grown ([`private_kind_claim`]): the reference consumer's cost that grows with use (avoidable,
//!   by one private account per pool).
//!
//! With `Open`, `JobOpen`, `Miller`, `Adopt`, `Finish`, `Apply` and `Record` (perf_state §3,
//! `tests/costs_attestation.rs`'s replay), that is every message the attestation program handles,
//! and every message of the reference consumer (`lez/guests/consumer`).
//!
//! What K adds: copies of real accounts the programs wrote, at keys no measured transaction names.
//! For the attestation program, `header(id, slot)` records at slots from 2^40 (as perf_state §2),
//! and for `Init` and `Succeed`, which each create an instance, `instance(id)` accounts too; for the
//! reference consumer, spent `claim(pool, N)` nullifiers, `balance(pool, owner)` or
//! `holder(pool, dest)` accounts and callers' reader states, beside the records. Not copied: job
//! accounts (one per `Open` never finished; no action but its own job's steps names one). For the
//! private transactions, K more commitments in LEZ's commitment set ([`add_commitments`]): every
//! private transaction's circuit verifies its fee payer's membership path in that set's tree, one
//! hash per level, so its cycles grow with log2 of all commitments ever made. Measured: the charge
//! stays the same while the path grows ([`membership_path_levels`]); the circuit's own cycles,
//! which LEZ's API drops with the prover's statistics, are read from r0vm's session log
//! (`RISC0_INFO=1 RUST_LOG=risc0_zkvm=info`) by `bench/perf/state-actions.sh`: 1,935 a level. A LEZ transaction sees
//! only the accounts it names, so equal charges show that the metering does not depend on the rest
//! of the state. The accounts an action does read are fixed in size or replaced rather than
//! appended to, with one exception in the reference consumer's own private state; the ones that
//! change with use are measured as they change: a reader holding a previous reply or record (and
//! again after another call), a balance or holder credited once or K + 1 times before, and a
//! claimant's `holdings` list (one entry per pool it holds privately).

use ferrata_core::{
    fork::{Fork, LcFork},
    merkle::ZERO,
    types::LightClientUpdate,
    witness::{uncompressed_keys, CompactUpdate},
    Root,
};
use ferrata_lez_wire::{
    attest::{Cmp, Field, LogQuery, Operand, Relation, Request},
    consumer::{
        balance_actor, claim_actor, holder_actor, nullifier, secret_nullifier, BalanceState, ClaimantState,
        ConsumerMessage, ConsumerState, HolderState, VaultPool,
    },
    header_actor, instance_actor, successor_id, Config, Message,
};
use lee::{test_utils::TestPrivateKeys, Account, AccountId, Actor, PublicAccountEvidence, V03State};
use lee_core::{
    account::ActorState,
    program::{PdaSeed, ReceiveInput, StateReply},
    AuthorizationSecretKey, Commitment, Nullifier,
};

use crate::{
    as_finality_update,
    attest::{account_query, condition, execute, transfer_conditions, u256, Stage4},
    consumer_account, consumer_program, ferrata_account, grouped, mainnet_bootstrap, mainnet_config,
    private::{catalog, PrivateAccount},
    root,
    scenarios::World,
    sepolia::SEPOLIA_CHAIN_ID,
    vault, Chain, Instance, TxReport, TRUSTED_BLOCK_ROOT,
};

/// Slots of the copied records: far above any real slot, so no request reads one (perf_state §2).
pub const SYNTHETIC_SLOT: u64 = 1 << 40;

/// The Gloas epoch of the successor [`succeed`] creates: a fork the mainnet instance's schedule
/// does not have, after its checkpoint's epoch (486,144), as `tests/successor.rs` adds Gloas to a
/// Sepolia instance's.
pub const SUCCESSOR_GLOAS_EPOCH: u64 = 600_000;

/// One transaction's charge as the comparison sees it: each handler's cycles in order, and the
/// execution and storage gas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Charged {
    pub action: String,
    pub handlers: Vec<(String, u64)>,
    pub gas_exec: u64,
    pub gas_stor: u64,
}

impl Charged {
    /// The charge of an applied transaction.
    pub fn of(action: &str, r: &TxReport) -> Self {
        assert!(r.ok(), "{action}: {:?} (exit {:?})", r.charge.revert, r.exit_code);
        Self {
            action: action.to_string(),
            handlers: r.transitions.iter().map(|t| (t.handler.clone(), t.cycles)).collect(),
            gas_exec: r.charge.gas_exec,
            gas_stor: r.charge.gas_stor,
        }
    }

    /// The cycles of `handler`'s only execution.
    pub fn cycles(&self, handler: &str) -> u64 {
        let mut runs = self.handlers.iter().filter(|(h, _)| h == handler);
        let (_, cycles) = runs.next().unwrap_or_else(|| panic!("{}: no {handler} ran", self.action));
        assert!(runs.next().is_none(), "{}: {handler} ran more than once", self.action);
        *cycles
    }

    /// `Init 191,616 → JobOpen 19,050`.
    pub fn handlers_text(&self) -> String {
        self.handlers.iter().map(|(h, c)| format!("{h} {}", grouped(*c))).collect::<Vec<_>>().join(" → ")
    }
}

/// `charged` at every K equals `charged` at the first: the assertion the measurements rest on.
pub fn assert_flat(base: &[Charged], at_k: &[Charged], k: usize) {
    assert_eq!(base.len(), at_k.len(), "the same transactions with {k} more accounts");
    for (b, c) in base.iter().zip(at_k) {
        assert_eq!(c, b, "{}: the charge moved with {k} more accounts", b.action);
    }
}

// ---------------------------------------------------------------------------------------------
// Accumulated accounts
// ---------------------------------------------------------------------------------------------

/// A key no request names: `tag` repeated, with `i` in the first eight bytes.
pub fn synthetic_root(tag: u8, i: usize) -> Root {
    let mut b = [tag; 32];
    b[..8].copy_from_slice(&(i as u64).to_le_bytes());
    b
}

/// The account at `id`, which must exist.
pub fn account(state: &V03State, id: AccountId) -> Account {
    state.get_account_by_id_ref(id).cloned().unwrap_or_else(|| panic!("no account {id}"))
}

/// `k` copies of `template` at `id_of(0..k)`.
pub fn copy_to(state: &mut V03State, template: &Account, k: usize, id_of: impl Fn(usize) -> AccountId) {
    for i in 0..k {
        state.force_insert_account(id_of(i), template.clone());
    }
}

/// `k` copies of `record` as records of instance `id` at slots from [`SYNTHETIC_SLOT`].
pub fn add_records(state: &mut V03State, id: &Root, record: &Account, k: usize) {
    let program = ferrata_account();
    copy_to(state, record, k, |i| header_actor(program, id, SYNTHETIC_SLOT + i as u64).account_id);
}

/// `k` copies of `instance` at instance ids no request names.
pub fn add_instances(state: &mut V03State, instance: &Account, k: usize) {
    let program = ferrata_account();
    copy_to(state, instance, k, |i| instance_actor(program, &synthetic_root(0x1a, i)).account_id);
}

// ---------------------------------------------------------------------------------------------
// The attestation program: Init, VerifyAll (public and private), Succeed
// ---------------------------------------------------------------------------------------------

/// The mainnet instance after `Init` and update_0, which supplies the next committee, so that
/// update_1 stripped to a finality update verifies in one call (`scenarios::one_transaction_update`,
/// and the proven path of `tests/costs_attestation.rs`); with the instance's record and instance
/// accounts, the templates of the copies.
pub struct UpdateWorld {
    pub base: World,
    pub record: Account,
    pub instance: Account,
}

impl Default for UpdateWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl UpdateWorld {
    pub fn new() -> Self {
        let mut base = World::new(2);
        let u0 = base.updates[0].clone();
        let run = base.apply(&u0, "update_0");
        assert!(run.reports.iter().all(TxReport::ok), "update_0 applies");
        let slot = base.inst.store(&base.chain).finalized.slot;
        let record = account(&base.chain.state, header_actor(base.inst.program, &base.inst.id, slot).account_id);
        let instance = account(&base.chain.state, base.inst.actor().account_id);
        Self { base, record, instance }
    }

    /// A copy of the base world with `k` more records of its instance and `k` more instances.
    pub fn grown(&self, k: usize) -> World {
        let mut w = self.base.clone();
        add_records(&mut w.chain.state, &w.inst.id, &self.record, k);
        add_instances(&mut w.chain.state, &self.instance, k);
        w
    }

    /// A new chain holding `k` records of another instance and `k` other instances, for `Init`.
    pub fn chain_for_init(&self, k: usize) -> Chain {
        let mut chain = Chain::new();
        add_records(&mut chain.state, &synthetic_root(0x0f, 0), &self.record, k);
        add_instances(&mut chain.state, &self.instance, k);
        chain
    }
}

/// `Init` of the mainnet instance and its committee's adoption in two parts, on `chain`: the
/// three transactions of `scenarios::World::new(2)`.
pub fn init_on(chain: &mut Chain) -> Vec<Charged> {
    let inst = Instance::new(ferrata_account(), mainnet_config(2), root(TRUSTED_BLOCK_ROOT));
    let run = inst.init(chain, &mainnet_bootstrap(), "Init");
    assert_eq!(run.reports.len(), 3, "Init, then two adoption transactions");
    let state = inst.state(chain).expect("the instance exists");
    assert!(state.active, "the instance activates with its last adoption part");
    let actions = ["Init (Init → JobOpen)", "Init: adoption 1/2", "Init: adoption 2/2 (Adopt → Apply → Record)"];
    run.reports.iter().zip(actions).map(|(r, a)| Charged::of(a, r)).collect()
}

/// update_1 stripped to a finality update: what [`verify_all_public`] and [`verify_all_private`]
/// verify, and its block time.
pub fn finality_update(w: &World) -> (LightClientUpdate, u64) {
    let update = as_finality_update(&w.updates[1]);
    let time = w.inst.config.slot_start_ms(update.signature_slot) + 12_000;
    (update, time)
}

/// `VerifyAll` for `update` against the instance's current store (as `tests/private_placement.rs`
/// builds it).
pub fn verify_all_message(w: &World, update: &LightClientUpdate) -> Vec<u8> {
    let store = w.inst.store(&w.chain);
    let witness = Instance::signer_witness(w.native.core(), update, &w.spec);
    let adoption = if update.next_sync_committee.is_empty() {
        vec![]
    } else {
        uncompressed_keys(&update.next_sync_committee).unwrap()
    };
    borsh::to_vec(&Message::VerifyAll {
        config: w.inst.config.clone(),
        checkpoint: w.inst.checkpoint,
        store: store.encode(),
        update: CompactUpdate::of(update).encode(LcFork::Electra),
        signers: witness.encode(),
        adoption,
    })
    .unwrap()
}

/// The finality update verified in one public transaction: `VerifyAll` at a public actor of the
/// program (the payer's account), which sends `ApplyVerified` to the instance, which writes the
/// record (`scenarios::one_transaction_update`).
pub fn verify_all_public(w: &mut World) -> Charged {
    let (update, time) = finality_update(w);
    let message = verify_all_message(w, &update);
    let verifier = Actor::new(w.chain.payer, ferrata_account());
    let header = header_actor(w.inst.program, &w.inst.id, update.finalized_header.beacon.slot);
    let r = w.chain.submit(
        "finality update in one transaction (VerifyAll)",
        verifier,
        message,
        vec![verifier, w.inst.actor(), header],
        vec![],
        time,
    );
    let charged = Charged::of("VerifyAll, public (→ ApplyVerified → Record)", &r);
    assert!(w.inst.record(&w.chain, update.finalized_header.beacon.slot).is_some(), "the record is written");
    charged
}

/// A private transaction's measured charge, and the length of its fee payer's membership path in
/// LEZ's commitment tree (the levels its circuit hashes).
#[derive(Clone, Debug)]
pub struct Proven {
    pub charged: Charged,
    pub path_levels: usize,
}

/// `k` more commitments in LEZ's commitment set: new private accounts that no transaction names,
/// added as LEZ's own settlement tests add them (`Chain::add_private_account`), so that the
/// commitment tree, and the membership path a private transaction's circuit verifies for its fee
/// payer, is deeper.
pub fn add_commitments(chain: &mut Chain, k: usize) {
    let accounts: Vec<(Commitment, Nullifier)> = (0..k)
        .map(|i| {
            let keys = TestPrivateKeys {
                ask: AuthorizationSecretKey(synthetic_root(0xa5, i)),
                d: synthetic_root(0xd5, i),
                z: synthetic_root(0x25, i),
            };
            let id = keys.account_id();
            (Commitment::new(&id, &Account::default()), Nullifier::for_account_initialization(&id, &keys.nsk()))
        })
        .collect();
    let state = std::mem::take(&mut chain.state);
    chain.state = state.with_private_accounts(accounts);
}

/// The length of `sender`'s membership path in LEZ's commitment tree: the levels the circuit hashes
/// for it (`compute_digest_for_path`, one SHA-256 per level).
pub fn membership_path_levels(chain: &Chain, sender: &PrivateAccount) -> usize {
    let (_, path) = chain
        .state
        .get_proof_for_commitment(&Commitment::new(&sender.id(), &sender.account))
        .expect("the sender's commitment is in the set");
    path.len()
}

/// The finality update verified in one private transaction (the proven path, RISC Zero dev mode):
/// a new private account runs `VerifyAll` at its own actor, which sends `ApplyVerified` to the
/// public instance. The charge is LEZ's flat receipt gas plus the public part's cycles, and the
/// private transaction's flat storage gas; the private execution, which LEZ does not meter (the
/// sender proves it), is run again in RISC Zero's executor for its cycles. Returned as two
/// "handlers": the private execution's cycles, then the public part's. `VerifyAll` carries the
/// instance's store in its message, so its private execution reads no chain state: its cycles
/// cannot vary with K, and are not a measurement against accumulation.
pub fn verify_all_private(w: &mut World) -> Proven {
    let (update, _) = finality_update(w);
    // The block time of the private placement's own test (`tests/costs_attestation.rs`).
    let time = w.inst.config.slot_start_ms(w.updates[2].signature_slot) + 12_000;
    let message = verify_all_message(w, &update);
    let sender = w.chain.add_private_account(0x31, 1_000_000_000_000);
    let root_actor = Actor::new(sender.id(), ferrata_account());
    let private = execute(
        "VerifyAll (private execution)",
        methods::FERRATA_LEZ_ELF,
        &ReceiveInput {
            receiver: root_actor,
            from: None,
            is_authorized: true,
            pre_state: Default::default(),
            message: message.clone(),
        },
    );
    assert_eq!(private.exit_code, 0, "the private execution verifies the update");
    let path_levels = membership_path_levels(&w.chain, &sender);
    let header = header_actor(w.inst.program, &w.inst.id, update.finalized_header.beacon.slot);
    let run = w
        .chain
        .prove_and_settle_private(
            &sender,
            root_actor,
            message,
            vec![w.inst.actor(), header],
            &catalog(),
            100_000_000,
            time,
        )
        .expect("settles");
    assert!(run.charge.revert.is_none(), "{:?}", run.charge.revert);
    assert!(w.inst.record(&w.chain, update.finalized_header.beacon.slot).is_some(), "the record is written");
    let charged = Charged {
        action: "VerifyAll, private (the proven path; public part ApplyVerified → Record)".to_string(),
        handlers: vec![
            ("VerifyAll, private execution (executor)".to_string(), private.cycles),
            ("public part".to_string(), run.charge.cycles),
        ],
        gas_exec: run.charge.gas_exec,
        gas_stor: run.charge.gas_stor,
    };
    Proven { charged, path_levels }
}

/// The successor's configuration: the instance's, with Gloas scheduled at
/// [`SUCCESSOR_GLOAS_EPOCH`].
pub fn successor_config(w: &World) -> Config {
    let mut config = w.inst.config.clone();
    config.fork_epochs[Fork::Gloas as usize] = SUCCESSOR_GLOAS_EPOCH;
    config
}

/// `Succeed` at the instance, an hour after its latest record, creating a successor whose
/// checkpoint is that record; the instance sends `Spawn` to the successor (`tests/successor.rs`).
pub fn succeed(w: &mut World) -> Charged {
    let store = w.inst.store(&w.chain);
    let checkpoint = store.finalized.hash_tree_root();
    let config = successor_config(w);
    let root_id = w.inst.state(&w.chain).expect("the instance").root_id;
    let id = successor_id(&root_id, &w.inst.id, &config, &checkpoint);
    let successor = instance_actor(w.inst.program, &id);
    let time = w.inst.config.slot_start_ms(store.finalized.slot) + 3_600_000;
    let r = w.chain.submit(
        "Succeed",
        w.inst.actor(),
        borsh::to_vec(&Message::Succeed { config, checkpoint }).unwrap(),
        vec![w.inst.actor(), successor],
        vec![],
        time,
    );
    let charged = Charged::of("Succeed (→ Spawn)", &r);
    let state = borsh::from_slice::<ferrata_lez_wire::InstanceState>(&w.chain.actor_state(successor))
        .expect("the successor exists");
    assert!(state.active && state.parent_id == w.inst.id, "an active successor of the instance");
    charged
}

// ---------------------------------------------------------------------------------------------
// Statements and the reference consumer, on the chain of `Stage4`
// ---------------------------------------------------------------------------------------------

/// The eight requests of `harness::attest::accepted` (`lez/results/stage4-attest.md`), with the
/// labels that record gives them, built as that function builds them.
pub fn statement_requests(s: &Stage4) -> Vec<(String, Request)> {
    let weth = s.fx.weth_proof();
    let (weth_slot, empty_slot) = (s.fx.weth_slot, s.fx.empty_slot);
    let absent = s.fx.absent_proof();
    let t = s.fx.transfer.clone();
    let big = s.fx.in_largest.clone();
    let older = s.fx.weth_proof_at_hop();
    vec![
        (
            "account: the WETH contract (nonce, balance, storage root, code hash)".to_string(),
            Request {
                accounts: vec![account_query(
                    &weth,
                    vec![
                        condition(Field::CodeHash, Cmp::Eq, weth.code_hash),
                        condition(Field::Nonce, Cmp::Ge, u256(1)),
                        condition(Field::Balance, Cmp::Eq, weth.balance),
                    ],
                    vec![],
                )],
                ..s.request()
            },
        ),
        (
            "storage slot: a WETH balance, value >= 1".to_string(),
            Request {
                accounts: vec![account_query(
                    &weth,
                    vec![],
                    vec![(weth_slot, vec![condition(Field::Value, Cmp::Ge, u256(1))])],
                )],
                ..s.request()
            },
        ),
        (
            "storage slot: an absent slot is zero".to_string(),
            Request {
                accounts: vec![account_query(
                    &weth,
                    vec![],
                    vec![(empty_slot, vec![condition(Field::Value, Cmp::Eq, ZERO)])],
                )],
                ..s.request()
            },
        ),
        (
            "account: an address without an account".to_string(),
            Request {
                accounts: vec![account_query(&absent, vec![condition(Field::CodeHash, Cmp::Eq, ZERO)], vec![])],
                ..s.request()
            },
        ),
        (
            format!("event log: an ERC-20 Transfer (receipt with {} logs)", t.logs_in_receipt),
            s.log_request(t.tx_index, t.log_index, transfer_conditions(&t.token, &t.to)),
        ),
        (
            format!("event log: a Transfer in the block's largest receipt ({} logs)", big.logs_in_receipt),
            s.log_request(big.tx_index, big.log_index, transfer_conditions(&big.token, &big.to)),
        ),
        (
            "storage slot 16 blocks back: one EIP-2935 hop, then the slot".to_string(),
            Request {
                hops: vec![s.fx.hop()],
                accounts: vec![account_query(&older, vec![], vec![(weth_slot, vec![])])],
                ..s.request()
            },
        ),
        (
            "four facts in one call (account, two slots, a log) and a relation".to_string(),
            Request {
                accounts: vec![account_query(&weth, vec![], vec![(weth_slot, vec![]), (empty_slot, vec![])])],
                logs: vec![LogQuery {
                    tx_index: t.tx_index,
                    log_index: t.log_index,
                    proof: s.fx.receipt_proof(t.tx_index),
                    conditions: vec![],
                }],
                relations: vec![Relation {
                    left: Operand { fact: 1, field: Field::Value },
                    cmp: Cmp::Gt,
                    right: Operand { fact: 2, field: Field::Value },
                }],
                ..s.request()
            },
        ),
    ]
}

/// Each of `requests` attested at the finalized block's record, as the root of a transaction.
pub fn attest_all(s: &mut Stage4, requests: &[(String, Request)]) -> Vec<Charged> {
    let slot = s.fx.finalized_slot;
    requests
        .iter()
        .map(|(label, request)| {
            let (r, _) = s.accept(label, slot, request);
            assert_eq!(r.transitions.len(), 1, "{label}: one handler execution");
            Charged::of(label, &r)
        })
        .collect()
}

/// The Stage 4 chain (the mainnet instance after the fixture's finality update), with the reference
/// consumer's accounts as a first `CallAttest` and a first claim leave them: the templates of the
/// consumer's copies.
pub struct ConsumerWorld {
    pub base: Stage4,
    pub record: Account,
    pub reader: Account,
    pub claim: Account,
    pub balance: Account,
}

impl Default for ConsumerWorld {
    fn default() -> Self {
        Self::new()
    }
}

/// The request [`call_attest`] sends: the WETH balance slot (row 1 of
/// `lez/results/stage4-attest-consumer.md`).
pub fn call_request(s: &Stage4) -> Request {
    let weth = s.fx.weth_proof();
    Request {
        accounts: vec![account_query(
            &weth,
            vec![],
            vec![(s.fx.weth_slot, vec![condition(Field::Value, Cmp::Ge, u256(1))])],
        )],
        ..s.request()
    }
}

impl ConsumerWorld {
    pub fn new() -> Self {
        let base = Stage4::new();
        let slot = base.fx.finalized_slot;
        let record = account(&base.w.chain.state, base.header(slot).account_id);
        // The templates, from a scratch copy: a reader holding a reply, a spent claim, a balance.
        let mut scratch = base.clone();
        let request = call_request(&scratch);
        let (r, reply) = scratch.call("a public actor calls Attest", slot, &request);
        assert!(r.ok() && reply.is_some(), "{:?}", r.charge.revert);
        let reader = account(&scratch.w.chain.state, scratch.reader().0.account_id);
        let (pool, t) = (scratch.pool(), scratch.fx.transfer.clone());
        let id = scratch.transfer_id(&t);
        let r = scratch.claim("reference consumer: claim", &pool, slot, &t, &id);
        assert!(r.ok(), "{:?} (exit {:?})", r.charge.revert, r.exit_code);
        let program = consumer_account();
        let pool_id = pool.id();
        let claim =
            account(&scratch.w.chain.state, claim_actor(program, &pool_id, &nullifier(&pool_id, &id)).account_id);
        let balance = account(&scratch.w.chain.state, balance_actor(program, &pool_id, &t.from).account_id);
        // The balance as `place_balance` builds it is the account the program wrote.
        assert_eq!(balance_account(&scratch, 1), balance, "place_balance builds the program's balance account");
        Self { base, record, reader, claim, balance }
    }

    /// A copy of the base chain with `k` more records of the instance, and `k` more spent claims,
    /// balances (in the measured pool, of other owners) and readers of the consumer.
    pub fn grown(&self, k: usize) -> Stage4 {
        let mut s = self.base.clone();
        let program = consumer_account();
        let pool_id = s.pool().id();
        let state = &mut s.w.chain.state;
        add_records(state, &s.w.inst.id, &self.record, k);
        copy_to(state, &self.claim, k, |i| claim_actor(program, &pool_id, &synthetic_root(0xc1, i)).account_id);
        copy_to(state, &self.balance, k, |i| {
            let mut owner = [0xba; 20];
            owner[..8].copy_from_slice(&(i as u64).to_le_bytes());
            balance_actor(program, &pool_id, &owner).account_id
        });
        copy_to(state, &self.reader, k, |i| {
            AccountId::for_public_pda(&program, &PdaSeed::new(synthetic_root(0x4e, i)))
        });
        s
    }
}

/// `CallAttest` of [`call_request`] three times from the same public reader: with an empty reader,
/// then twice with the reader holding the previous reply (which each call replaces).
pub fn call_attest(s: &mut Stage4) -> Vec<Charged> {
    let slot = s.fx.finalized_slot;
    let request = call_request(s);
    let expected = s.expected(slot, &request).expect("accepted natively");
    ["CallAttest, empty reader", "CallAttest, reader holding the previous reply", "CallAttest, once more"]
        .iter()
        .map(|action| {
            let (r, reply) = s.call(action, slot, &request);
            assert_eq!(reply.as_ref(), Some(&expected), "{action}: the reply is the whole attestation");
            Charged::of(action, &r)
        })
        .collect()
}

/// The public reader of [`read_record`]: the consumer's actor at the seed of
/// `scenarios::shell_costs`, another than [`call_attest`]'s.
pub fn read_actor() -> (Actor, Vec<PublicAccountEvidence>) {
    let program = consumer_account();
    let seed = PdaSeed::new([0x33; 32]);
    (
        Actor::new(AccountId::for_public_pda(&program, &seed), program),
        vec![PublicAccountEvidence::Pda { program, seed }],
    )
}

/// The finalized block's record read by a public reader of the consumer (`ReadRecord`: the
/// consumer sends `Read`, the record replies with its state, which the reader keeps): with an empty
/// reader, then with the reader holding the record it read; then `ReadPadded` with 1,024 bytes the
/// program ignores (`scenarios::shell_costs`).
pub fn read_record(s: &mut Stage4) -> Vec<Charged> {
    let slot = s.fx.finalized_slot;
    let (reader, evidence) = read_actor();
    let header = s.header(slot);
    let (ferrata, instance_id) = (s.w.inst.program, s.w.inst.id);
    let read = ConsumerMessage::ReadRecord { ferrata, instance_id, slot, query: vec![] };
    let padded = ConsumerMessage::ReadPadded { ferrata, instance_id, slot, padding: vec![0xab; 1024] };
    [
        ("ReadRecord (→ Read → reply), empty reader", &read),
        ("ReadRecord, reader holding the record read before", &read),
        ("ReadPadded, 1,024 bytes of padding", &padded),
    ]
    .into_iter()
    .map(|(action, message)| {
        let r = s.w.chain.submit(
            action,
            reader,
            borsh::to_vec(message).unwrap(),
            vec![reader, header],
            evidence.clone(),
            s.time,
        );
        Charged::of(action, &r)
    })
    .collect()
}

/// The sender's `balance(pool, owner)` account as the reference consumer writes it after `credits`
/// credits of the fixture's transfer.
pub fn balance_account(s: &Stage4, credits: u64) -> Account {
    let (pool, t) = (s.pool(), &s.fx.transfer);
    let state = ConsumerState::Balance(BalanceState {
        pool_id: pool.id(),
        owner: t.from,
        amount: t.amount() * u128::from(credits),
        credits,
    });
    Account::default().with_actor_state(consumer_account(), ActorState::from(borsh::to_vec(&state).unwrap()))
}

/// Place the sender's balance as if it had been credited `credits` times before.
pub fn place_balance(s: &mut Stage4, credits: u64) {
    let id = balance_actor(consumer_account(), &s.pool().id(), &s.fx.transfer.from).account_id;
    let balance = balance_account(s, credits);
    s.w.chain.state.force_insert_account(id, balance);
}

/// The reference consumer's claim of the fixture's `Transfer` (`Claim → Attest → reply →
/// Credit`), which credits its sender; `credits_before` is what the sender's balance held.
pub fn claim(s: &mut Stage4, action: &str, credits_before: u64) -> Charged {
    let slot = s.fx.finalized_slot;
    let (pool, t) = (s.pool(), s.fx.transfer.clone());
    let id = s.transfer_id(&t);
    let r = s.claim(action, &pool, slot, &t, &id);
    let charged = Charged::of(action, &r);
    assert_eq!(r.transitions.len(), 4, "{action}: Claim, Attest, the reply, Credit");
    let balance = s.balance(&pool, &t.from).expect("the sender is credited");
    assert_eq!(balance.credits, credits_before + 1, "{action}: credited once more");
    charged
}

// ---------------------------------------------------------------------------------------------
// The accumulator leaf: the synthetic vault
// ---------------------------------------------------------------------------------------------

/// The block time of `vault::synthetic_attestations`.
pub const LEAF_TIME: u64 = 1_800_000_000_000;

/// The synthetic vault (`vault::synthetic`): a record of `vault::SYNTHETIC_INSTANCE` that binds the
/// block hash of a block whose state holds the vault with the eight deposits of the tree vectors,
/// and a record of the same block that holds the payload's fields, placed on a new chain as
/// `vault::synthetic_attestations` places them (`lez/results/stage5-vault-synthetic.md`).
pub struct LeafWorld {
    pub base: Chain,
    pub syn: vault::Synthetic,
    /// `header(id, slot)` of the record that binds the block hash, and its account.
    pub actor: Actor,
    pub record: Account,
    /// The record that holds the payload's fields, and its actor.
    pub payload: ferrata_core::witness::HeaderRecord,
    pub at_payload: Actor,
}

impl Default for LeafWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl LeafWorld {
    pub fn new() -> Self {
        use ferrata_core::{
            types::BeaconBlockHeader,
            witness::{ExecutionSummary, HeaderRecord},
        };
        let syn = vault::synthetic();
        let mut base = Chain::new();
        let actor = vault::place_record(&mut base, vault::SYNTHETIC_INSTANCE, SEPOLIA_CHAIN_ID, &syn.record);
        let record = account(&base.state, actor.account_id);
        let h = ferrata_core::exec::decode_header_with_hash(&syn.header, &syn.block_hash).unwrap();
        let payload = HeaderRecord {
            beacon: BeaconBlockHeader { slot: vault::SYNTHETIC_SLOT + 1, ..Default::default() },
            execution_root: [0; 32],
            execution: ExecutionSummary::Payload {
                block_number: h.number,
                block_hash: syn.block_hash,
                parent_hash: h.parent_hash,
                state_root: h.state_root,
                receipts_root: h.receipts_root,
                timestamp: h.timestamp,
            },
        };
        let at_payload = vault::place_record(&mut base, vault::SYNTHETIC_INSTANCE, SEPOLIA_CHAIN_ID, &payload);
        Self { base, syn, actor, record, payload, at_payload }
    }

    /// A copy of the chain with `k` more records of the synthetic instance (slots from 2^40).
    pub fn grown(&self, k: usize) -> Chain {
        let mut chain = self.base.clone();
        add_records(&mut chain.state, &vault::SYNTHETIC_INSTANCE, &self.record, k);
        chain
    }
}

/// The vault's requests of `stage5-vault-synthetic.md` rows 1–5, each attested at its record as the
/// root of a transaction and checked against the native evaluation: slots 0 and 1 alone, with one,
/// two and all eight deposit leaves (under the record that binds the block hash, the header
/// travelling with the request), and with one leaf under the record that holds the payload's fields.
pub fn attest_leaves(lw: &LeafWorld, chain: &mut Chain) -> Vec<Charged> {
    let s = &lw.syn;
    let base = Request {
        instance_id: vault::SYNTHETIC_INSTANCE,
        chain_id: SEPOLIA_CHAIN_ID,
        header: Some(s.header.clone()),
        ..Request::default()
    };
    let n = s.leaves.len();
    let leaf = |i: usize| (i as u64, s.leaves[i], vault::tree(&s.leaves, i).1);
    let by_hash = (&vault::SYNTHETIC_INSTANCE, SEPOLIA_CHAIN_ID, &s.record);
    let by_payload = (&vault::SYNTHETIC_INSTANCE, SEPOLIA_CHAIN_ID, &lw.payload);
    let one = vault::vault_request(base.clone(), &s.proof, &s.root, &[leaf(n - 1)]);
    let requests = [
        (
            "vault slots 0 and 1, no leaf (synthetic state, Gloas record)",
            by_hash,
            lw.actor,
            vault::vault_request(base.clone(), &s.proof, &s.root, &[]),
        ),
        ("vault slots 0 and 1 and one deposit leaf", by_hash, lw.actor, one.clone()),
        (
            "vault slots 0 and 1 and two deposit leaves",
            by_hash,
            lw.actor,
            vault::vault_request(base.clone(), &s.proof, &s.root, &[leaf(0), leaf(n - 1)]),
        ),
        (
            "vault slots 0 and 1 and all eight deposit leaves",
            by_hash,
            lw.actor,
            vault::vault_request(base.clone(), &s.proof, &s.root, &(0..n).map(leaf).collect::<Vec<_>>()),
        ),
        (
            "the same with one leaf, under a record that holds the payload's fields (no header)",
            by_payload,
            lw.at_payload,
            Request { header: None, ..one },
        ),
    ];
    requests
        .into_iter()
        .map(|(label, native, actor, request)| {
            let (r, a) = vault::run(chain, native, label, actor, &request, LEAF_TIME);
            assert!(a.is_ok(), "{label}: accepted");
            Charged::of(label, &r)
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// The reference consumer's private read, and its vault pattern on FerrataVault v5's deposits
// ---------------------------------------------------------------------------------------------

/// The reference consumer's private read (`AttestPrivately` of the fixture's `Transfer` in its
/// pool), by a new private account, proven in one private transaction (RISC Zero dev mode) and
/// settled: its two private transitions' cycles in the executor (the root call, which sends `Read`;
/// the record's reply, in which the fact is verified), then the public part (`Read`).
pub fn attest_privately(s: &mut Stage4) -> Proven {
    let slot = s.fx.finalized_slot;
    let (pool, t) = (s.pool(), s.fx.transfer.clone());
    let header = s.header(slot);
    let account = s.w.chain.add_private_account(0x81, 1_000_000_000_000);
    let message = s.private_message(&pool, slot, &t);
    let executed = s.private_transitions(account.id(), slot, &message);
    assert_eq!(executed.iter().map(|e| e.exit_code).collect::<Vec<_>>(), [0, 0], "both private transitions pass");
    let path_levels = membership_path_levels(&s.w.chain, &account);
    let run =
        s.w.chain
            .prove_and_settle_private(
                &account,
                Actor::new(account.id(), consumer_account()),
                borsh::to_vec(&message).unwrap(),
                vec![header],
                &catalog(),
                100_000_000,
                s.time,
            )
            .expect("settles");
    assert!(run.charge.revert.is_none(), "{:?}", run.charge.revert);
    let charged = Charged {
        action: "AttestPrivately, private (public part Read)".to_string(),
        handlers: vec![
            ("AttestPrivately, private (executor)".to_string(), executed[0].cycles),
            ("the record's reply, private (executor)".to_string(), executed[1].cycles),
            ("public part".to_string(), run.charge.cycles),
        ],
        gas_exec: run.charge.gas_exec,
        gas_stor: run.charge.gas_stor,
    };
    Proven { charged, path_levels }
}

/// FerrataVault v5's two real deposits under the Sepolia instance's record (`vault::RealVault`):
/// the reference consumer's vault pattern (`tests/vault_consumer.rs`). Deposit 0 is public-kind (to
/// LEZ's public test account 0), deposit 1 private-kind (to LEZ's private test account 0). With the
/// accounts a first public claim of deposit 0 leaves, the templates of the copies.
pub struct VaultWorld {
    pub v: vault::RealVault,
    pub pool: VaultPool,
    /// Each deposit's opening `(s, kind, dest)` (`vault::crown_openings`).
    pub openings: Vec<(Root, u8, Root)>,
    pub record: Account,
    pub spent: Account,
    pub holder: Account,
}

impl Default for VaultWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl VaultWorld {
    /// On the Sepolia instance's own record, after the replay (`vault::RealVault::load`).
    pub fn new() -> Self {
        Self::on(vault::RealVault::load())
    }

    /// On the same record placed without the replay (`vault::RealVault::placed`): what CI runs.
    pub fn placed() -> Self {
        Self::on(vault::RealVault::placed())
    }

    fn on(v: vault::RealVault) -> Self {
        let pool = vault::eth_pool(&v);
        let openings = vault::crown_openings(&v);
        let record = account(&v.w.chain.state, v.actor().account_id);
        let mut vw = Self { v, pool, openings, record: record.clone(), spent: record.clone(), holder: record };
        let mut scratch = vw.v.w.clone();
        claim_deposit(&vw, &mut scratch, "the template claim", 0);
        let (s0, _, dest0) = vw.openings[0];
        let program = consumer_account();
        let pool_id = vw.pool.id();
        let n0 = secret_nullifier(&s0, &vw.v.deposit_id(0));
        vw.spent = account(&scratch.chain.state, claim_actor(program, &pool_id, &n0).account_id);
        vw.holder = account(&scratch.chain.state, holder_actor(program, &pool_id, &dest0).account_id);
        assert_eq!(holder_account(&vw, 1), vw.holder, "place_holder builds the program's holder account");
        vw
    }

    /// A copy of the Sepolia world with `k` more records of its instance, and `k` more spent claims
    /// and holders (same pool, other destinations) of the consumer.
    pub fn grown(&self, k: usize) -> World {
        let mut w = self.v.w.clone();
        let program = consumer_account();
        let pool_id = self.pool.id();
        let state = &mut w.chain.state;
        add_records(state, &w.inst.id, &self.record, k);
        copy_to(state, &self.spent, k, |i| claim_actor(program, &pool_id, &synthetic_root(0xc2, i)).account_id);
        copy_to(state, &self.holder, k, |i| holder_actor(program, &pool_id, &synthetic_root(0x40, i)).account_id);
        w
    }
}

/// `ClaimDeposit` of deposit 0 (public kind) at `claim(pool, N)` on `w`: the public claim (`→
/// Attest → reply → CreditHolder`); `credits_before` is what its destination's holder held.
pub fn claim_deposit(vw: &VaultWorld, w: &mut World, action: &str, credits_before: u64) -> Charged {
    let (s0, kind0, dest0) = vw.openings[0];
    let program = consumer_account();
    let pool_id = vw.pool.id();
    let n0 = secret_nullifier(&s0, &vw.v.deposit_id(0));
    let at = claim_actor(program, &pool_id, &n0);
    let holder = holder_actor(program, &pool_id, &dest0);
    let message = ConsumerMessage::ClaimDeposit {
        pool: vw.pool.clone(),
        slot: vw.v.slot,
        deposit: vw.v.deposit_proof(0),
        s: s0,
        kind: kind0,
        dest: dest0,
    };
    let r = w.chain.submit(
        action,
        at,
        borsh::to_vec(&message).unwrap(),
        vec![at, vw.v.actor(), holder],
        vec![PublicAccountEvidence::Pda { program, seed: ferrata_lez_wire::consumer::claim_seed(&pool_id, &n0) }],
        vw.v.time,
    );
    let charged = Charged::of(action, &r);
    let held = match crate::attest::consumer_state(&w.chain, holder) {
        Some(ConsumerState::Holder(h)) => h,
        other => panic!("{action}: the holder holds {other:?}"),
    };
    assert_eq!(held.credits, credits_before + 1, "{action}: credited once more");
    charged
}

/// Deposit 0's destination's holder as the consumer writes it after `credits` credits of 0.001 ETH.
pub fn holder_account(vw: &VaultWorld, credits: u64) -> Account {
    let (_, _, dest0) = vw.openings[0];
    let amount = vw.v.deposits[0].deposit.amount;
    let state = ConsumerState::Holder(HolderState {
        pool_id: vw.pool.id(),
        dest: dest0,
        amount: u128::from_be_bytes(amount[16..].try_into().unwrap()) * u128::from(credits),
        credits,
    });
    Account::default().with_actor_state(consumer_account(), ActorState::from(borsh::to_vec(&state).unwrap()))
}

/// Place deposit 0's destination's holder as if credited `credits` times before.
pub fn place_holder(vw: &VaultWorld, w: &mut World, credits: u64) {
    let (_, _, dest0) = vw.openings[0];
    let id = holder_actor(consumer_account(), &vw.pool.id(), &dest0).account_id;
    w.chain.state.force_insert_account(id, holder_account(vw, credits));
}

/// The consumer's two private transitions for a private claim, in the executor: the root call at
/// `account`'s actor holding `pre_state` (it sends `Read`), then the record's reply, in which the
/// deposit is verified and the claim's commitment opened (it sends `Spend`).
fn private_claim_transitions(
    vw: &VaultWorld,
    account: AccountId,
    pre_state: ActorState,
    message: &ConsumerMessage,
) -> (u64, u64, ActorState) {
    let elf = consumer_program();
    let actor = Actor::new(account, consumer_account());
    let call = ReceiveInput {
        receiver: actor,
        from: None,
        is_authorized: true,
        pre_state,
        message: borsh::to_vec(message).unwrap(),
    };
    let first = execute("ClaimDepositPrivately (sends Read)", elf.elf(), &call);
    let transition = first.transition.as_ref().expect("the root call passes");
    let header = vw.v.actor();
    let reply = ReceiveInput {
        receiver: actor,
        from: Some(header),
        is_authorized: true,
        pre_state: transition.response.post_state.clone().expect("the claim is kept for the reply"),
        message: borsh::to_vec(&StateReply { state: vw.v.w.chain.actor_state(header) }).unwrap(),
    };
    let second = execute("the record's reply (verifies the deposit, sends Spend)", elf.elf(), &reply);
    assert_eq!(second.exit_code, 0, "the reply verifies the deposit");
    let kept = second.transition.expect("a transition").response.post_state.expect("the claimant keeps its state");
    (first.cycles, second.cycles, kept)
}

/// `ClaimDepositPrivately` of deposit 0 (public kind, so its holder is credited publicly), by a new
/// private account, proven in one private transaction (dev mode) and settled: the two private
/// transitions' cycles in the executor, then the public part (`Read`, `Spend`, `CreditHolder`).
pub fn claim_deposit_privately(vw: &VaultWorld, w: &mut World) -> Proven {
    let (s0, kind0, dest0) = vw.openings[0];
    let program = consumer_account();
    let pool_id = vw.pool.id();
    let n0 = secret_nullifier(&s0, &vw.v.deposit_id(0));
    let account = w.chain.add_private_account(0x92, 1_000_000_000_000);
    let message = ConsumerMessage::ClaimDepositPrivately {
        pool: vw.pool.clone(),
        slot: vw.v.slot,
        deposit: vw.v.deposit_proof(0),
        s: s0,
        kind: kind0,
        dest: dest0,
    };
    let (root_call, reply, _) = private_claim_transitions(vw, account.id(), ActorState::default(), &message);
    let path_levels = membership_path_levels(&w.chain, &account);
    let publics = vec![vw.v.actor(), claim_actor(program, &pool_id, &n0), holder_actor(program, &pool_id, &dest0)];
    let run = w
        .chain
        .prove_and_settle_private(
            &account,
            Actor::new(account.id(), program),
            borsh::to_vec(&message).unwrap(),
            publics,
            &catalog(),
            100_000_000,
            vw.v.time,
        )
        .expect("settles");
    assert!(run.charge.revert.is_none(), "{:?}", run.charge.revert);
    let charged = Charged {
        action: "ClaimDepositPrivately, private (public part Read → Spend → CreditHolder)".to_string(),
        handlers: vec![
            ("ClaimDepositPrivately, private (executor)".to_string(), root_call),
            ("the record's reply, private (executor)".to_string(), reply),
            ("public part".to_string(), run.charge.cycles),
        ],
        gas_exec: run.charge.gas_exec,
        gas_stor: run.charge.gas_stor,
    };
    Proven { charged, path_levels }
}

/// The private claim of deposit 1 (private kind: credited in the claimant's own private state, its
/// `holdings`, one `(pool id, amount)` entry per pool) by its destination, LEZ's private test account
/// 0, in the executor, with the claimant's state already holding `pools` other pools' entries: the
/// two private transitions' cycles. The claimant's private state is the one list an action reads
/// that grows with use (`lez/wire/src/lib.rs`, `ClaimantState`); LEZ charges the transaction the same
/// (its public part, `Read` and `Spend`, does not see it), the claimant's own proof grows: here its
/// two private transitions; LEZ's circuit, which hashes and encrypts the larger state, is not measured.
pub fn private_kind_claim(vw: &VaultWorld, pools: usize) -> (u64, u64) {
    let (s1, kind1, dest1) = vw.openings[1];
    let keys = vault::lez_testnet_private_account_0();
    assert_eq!(keys.account_id().value(), &dest1, "the deposit's destination is LEZ's private test account 0");
    let holdings: Vec<(Root, u128)> = (0..pools).map(|i| (synthetic_root(0x9f, i), 1)).collect();
    let before = ClaimantState { awaiting: None, pending: None, holdings: holdings.clone(), claims: pools as u64 };
    let pre_state = ActorState::from(borsh::to_vec(&ConsumerState::Claimant(before)).unwrap());
    let message = ConsumerMessage::ClaimDepositPrivately {
        pool: vw.pool.clone(),
        slot: vw.v.slot,
        deposit: vw.v.deposit_proof(1),
        s: s1,
        kind: kind1,
        dest: dest1,
    };
    let (root_call, reply, kept) = private_claim_transitions(vw, keys.account_id(), pre_state, &message);
    let ConsumerState::Claimant(after) = borsh::from_slice(&kept).expect("a claimant's state") else {
        panic!("a claimant's state")
    };
    assert_eq!(after.holdings.len(), pools + 1, "one more pool held");
    assert_eq!(after.claims, pools as u64 + 1, "one more claim");
    (root_call, reply)
}
