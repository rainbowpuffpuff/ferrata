//! RFP-021 performance #9, "No action's cycle count or gas cost scales with accumulated on-chain
//! storage", for the actions not covered by `src/bin/perf_state.rs` §2–§3 (five statement types
//! and every update step): each runs on two copies of one world, one of which also holds K more of
//! the accounts that accumulate with use, and must be charged the same in both — every handler's
//! cycles, the execution gas and the storage gas. Shared by `src/bin/perf_state.rs` (§6, K from 0
//! to 100,000, written to `lez/results/perf-state.md`) and `tests/costs_attestation.rs` (K = 1,000,
//! pinned in CI).
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
//! - the reference consumer: `CallAttest` with an empty reader and with a reader holding the
//!   previous reply ([`call_attest`]), and `Claim` for a sender credited for the first time and for
//!   one credited before ([`claim`], [`place_balance`]).
//!
//! What K adds: copies of real accounts the programs wrote, at keys no measured transaction names.
//! For the attestation program, `header(id, slot)` records at slots from 2^40 (as perf_state §2),
//! and for `Init` and `Succeed`, which each create an instance, `instance(id)` accounts too; for the
//! reference consumer, spent `claim(pool, N)` nullifiers, `balance(pool, owner)` accounts and
//! callers' reader states, beside the records. A LEZ transaction sees only the accounts it names,
//! so equal charges show that the metering does not depend on the rest of the state. The accounts
//! an action does read are fixed in size or replaced rather than appended to; the two that change
//! with use are measured as they change: the reader holding a previous reply (and again after
//! another call), and the claimant's balance credited once or K + 1 times before.

use ferrata_core::{
    fork::{Fork, LcFork},
    merkle::ZERO,
    types::LightClientUpdate,
    witness::{uncompressed_keys, CompactUpdate},
    Root,
};
use ferrata_lez_wire::{
    attest::{Cmp, Field, LogQuery, Operand, Relation, Request},
    consumer::{balance_actor, claim_actor, nullifier, BalanceState, ConsumerState},
    header_actor, instance_actor, successor_id, Config, Message,
};
use lee::{Account, AccountId, Actor, V03State};
use lee_core::{
    account::ActorState,
    program::{PdaSeed, ReceiveInput},
};

use crate::{
    as_finality_update,
    attest::{account_query, condition, execute, transfer_conditions, u256, Stage4},
    consumer_account, ferrata_account, grouped, mainnet_bootstrap, mainnet_config,
    private::catalog,
    root,
    scenarios::World,
    Chain, Instance, TxReport, TRUSTED_BLOCK_ROOT,
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
    let adoption =
        if update.next_sync_committee.is_empty() { vec![] } else { uncompressed_keys(&update.next_sync_committee).unwrap() };
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

/// The finality update verified in one private transaction (the proven path, RISC Zero dev mode):
/// a new private account runs `VerifyAll` at its own actor, which sends `ApplyVerified` to the
/// public instance. The charge is LEZ's flat receipt gas plus the public part's cycles, and the
/// private transaction's flat storage gas; the private execution, which LEZ does not meter (the
/// sender proves it), is run again in RISC Zero's executor for its cycles. Returned as two
/// "handlers": the private execution's cycles, then the public part's.
pub fn verify_all_private(w: &mut World) -> Charged {
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
    Charged {
        action: "VerifyAll, private (the proven path; public part ApplyVerified → Record)".to_string(),
        handlers: vec![
            ("VerifyAll, private execution (executor)".to_string(), private.cycles),
            ("public part".to_string(), run.charge.cycles),
        ],
        gas_exec: run.charge.gas_exec,
        gas_stor: run.charge.gas_stor,
    }
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
        let claim = account(&scratch.w.chain.state, claim_actor(program, &pool_id, &nullifier(&pool_id, &id)).account_id);
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
