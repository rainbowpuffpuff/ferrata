//! Stage 4: attestation through the program, on real mainnet data.
//!
//! The fixture (`crates/ferrata-spectests/fixtures/mainnet-attest-2026-11-10`, fetched by
//! `lez/scripts/fetch-attest-fixture.py`; its SOURCE.md lists every request) is one finalized
//! mainnet block: the bootstrap `Init` verifies, the finality update the public job verifies,
//! `eth_getProof` for accounts and storage slots at that block, the block's receipts, and the
//! proofs of one EIP-2935 hop. [`Stage4`] is a chain whose instance has verified that header, so
//! that `header(id, slot)` holds its record; everything here is then proven against that record
//! through LEZ's settlement, never against a root the test supplies.
//!
//! What the program must answer is computed three ways: by the program in the guest, by the same
//! function natively ([`Stage4::expected`]), and from the JSON-RPC responses alone (the values
//! `eth_getProof` and `eth_getBlockReceipts` report, and the identifiers re-derived here with
//! SHA-256 from those fields: [`log_statement_id`], [`storage_statement_id`],
//! [`account_statement_id`]).

use std::{fs, path::PathBuf};

use ferrata_core::{
    fork::LcFork,
    merkle::ZERO,
    rlp::encode_u64,
    types::{LightClientBootstrap, LightClientHeader, LightClientUpdate, SyncAggregate, SyncCommittee},
    Root,
};
use ferrata_lez_wire::{
    attest::{
        evaluate, AccountQuery, Address, Attestation, Cmp, Condition, Fact, Field, Hop, LogQuery, Operand, Relation,
        Request, SlotQuery, Word,
    },
    code,
    consumer::{
        balance_actor, claim_actor, claim_seed, code as consumer_code, nullifier, BalanceState, ClaimState,
        ConsumerMessage, ConsumerState, Pool,
    },
    header_actor, header_seed, Message, RecordState, ATTESTED_SELECTOR,
};
use ferrata_spectests::execkit::{account_proof, hexb, receipt_rlp, word, AccountProof, Trie};
use lee::{AccountId, Actor, PublicAccountEvidence};
use lee_core::{
    from_frame,
    program::{PdaSeed, ReceiveInput, StateReply, Transition},
};
use risc0_zkvm::{default_executor, ExecutorEnv, ExitCode};
use serde_json::Value;
use sha2::{Digest, Sha256};
use test_guest_core::Script;

use crate::{
    consumer_account, consumer_program, mainnet_config,
    scenarios::{Refusal, World},
    scripted_account, Chain, Instance, TxReport, TWIN,
};

// ---------------------------------------------------------------------------------------------
// The fixture
// ---------------------------------------------------------------------------------------------

/// The fixture's directory name under `crates/ferrata-spectests/fixtures`.
pub const FIXTURE: &str = "mainnet-attest-2026-11-10";

/// The fixture's pinned `SHA256SUMS`, compiled in.
const SUMS: &str = include_str!("../../../crates/ferrata-spectests/fixtures/mainnet-attest-2026-11-10/SHA256SUMS");

/// `FERRATA_ATTEST_FIXTURE_DIR` if set (to run a built binary elsewhere), else the fixture of
/// this checkout.
pub fn fixture_dir() -> PathBuf {
    std::env::var_os("FERRATA_ATTEST_FIXTURE_DIR").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/ferrata-spectests/fixtures").join(FIXTURE)
    })
}

/// A fixture file, checked against the pinned `SHA256SUMS`.
pub fn fixture_file(name: &str) -> Vec<u8> {
    let path = fixture_dir().join(name);
    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let want = SUMS
        .lines()
        .find(|l| l.ends_with(&format!("  {name}")))
        .unwrap_or_else(|| panic!("{name} is not in SHA256SUMS"))
        .split_whitespace()
        .next()
        .unwrap();
    assert_eq!(hex::encode(Sha256::digest(&bytes)), want, "{name} does not match SHA256SUMS");
    bytes
}

fn address(v: &Value) -> Address {
    hexb(v.as_str().expect("an address")).try_into().expect("20 bytes")
}

fn root_of(v: &Value) -> Root {
    hexb(v.as_str().expect("a root")).try_into().expect("32 bytes")
}

/// An ERC-20 `Transfer` log the fetch script chose in the finalized block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transfer {
    pub tx_index: u64,
    /// The log's position in its receipt.
    pub log_index: u64,
    /// JSON-RPC's block-wide `logIndex`.
    pub block_log_index: u64,
    pub logs_in_receipt: u64,
    pub token: Address,
    pub from: Address,
    pub to: Address,
    pub value: Word,
}

impl Transfer {
    pub(crate) fn read(v: &Value) -> Self {
        Self {
            tx_index: v["tx_index"].as_u64().unwrap(),
            log_index: v["log_index"].as_u64().unwrap(),
            block_log_index: v["block_log_index"].as_u64().unwrap(),
            logs_in_receipt: v["logs_in_receipt"].as_u64().unwrap(),
            token: address(&v["token"]),
            from: address(&v["from"]),
            to: address(&v["to"]),
            value: word(v["value"].as_str().unwrap()),
        }
    }

    /// The transferred amount, if it fits 128 bits.
    pub fn amount(&self) -> u128 {
        assert_eq!(self.value[..16], [0u8; 16], "the amount fits 128 bits");
        u128::from_be_bytes(self.value[16..].try_into().unwrap())
    }
}

/// The mainnet fixture: one finalized block and what is proven under it.
pub struct Fixture {
    pub chain_id: u64,
    /// The checkpoint `Init` trusts: the bootstrap's block root.
    pub checkpoint: Root,
    pub bootstrap: LightClientBootstrap,
    /// The finality update, as the update the job verifies (no next committee).
    pub update: LightClientUpdate,
    pub bootstrap_slot: u64,
    pub finalized_slot: u64,
    /// The finalized execution block `n`.
    pub block: u64,
    /// `m = n - 16`, one EIP-2935 hop from `n`.
    pub hop_block: u64,
    pub weth: Address,
    /// `balanceOf[holder]` in WETH's storage.
    pub weth_slot: Word,
    /// A slot WETH never writes.
    pub empty_slot: Word,
    /// An address without an account.
    pub absent: Address,
    /// The history contract's slot of `m`.
    pub history_slot: Word,
    /// The first `Transfer` whose recipient is a contract.
    pub transfer: Transfer,
    /// A `Transfer` of the same token to another recipient.
    pub other_recipient: Transfer,
    /// A `Transfer` of another token.
    pub other_token: Transfer,
    /// A `Transfer` in the receipt with the most logs.
    pub in_largest: Transfer,
    /// A transaction that failed.
    pub failed_tx: u64,
    /// `eth_getBlockReceipts` of `n`, in transaction order.
    pub receipts: Vec<Value>,
    receipts_trie: Trie,
}

/// `LightClientFinalityUpdate` (Electra layout) as the `LightClientUpdate` it stands for.
fn finality_update(b: &[u8]) -> LightClientUpdate {
    let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as usize;
    let (att, fin) = (u32_at(0), u32_at(4));
    assert_eq!((att, b.len() > fin), (400, true), "LightClientFinalityUpdate offsets");
    LightClientUpdate {
        attested_header: LightClientHeader::decode(&b[att..fin], LcFork::Electra).unwrap(),
        next_sync_committee: SyncCommittee::empty(512),
        next_sync_committee_branch: vec![ZERO; LcFork::Electra.sync_committee_branch_len()],
        finalized_header: LightClientHeader::decode(&b[fin..], LcFork::Electra).unwrap(),
        finality_branch: b[8..232].as_chunks::<32>().0.to_vec(),
        sync_aggregate: SyncAggregate::decode(&b[232..392], 512).unwrap(),
        signature_slot: u64::from_le_bytes(b[392..400].try_into().unwrap()),
    }
}

/// The receipts trie, rebuilt from the JSON: key `RLP(index)`, value the receipt's consensus
/// encoding.
fn receipts_trie(receipts: &[Value]) -> Trie {
    Trie::new(receipts.iter().enumerate().map(|(i, r)| {
        let (key, len) = encode_u64(i as u64);
        (key[..len].to_vec(), receipt_rlp(r))
    }))
}

/// A copy, its receipts trie rebuilt from the same JSON (the trie type is not `Clone`).
impl Clone for Fixture {
    fn clone(&self) -> Self {
        Self {
            chain_id: self.chain_id,
            checkpoint: self.checkpoint,
            bootstrap: self.bootstrap.clone(),
            update: self.update.clone(),
            bootstrap_slot: self.bootstrap_slot,
            finalized_slot: self.finalized_slot,
            block: self.block,
            hop_block: self.hop_block,
            weth: self.weth,
            weth_slot: self.weth_slot,
            empty_slot: self.empty_slot,
            absent: self.absent,
            history_slot: self.history_slot,
            transfer: self.transfer.clone(),
            other_recipient: self.other_recipient.clone(),
            other_token: self.other_token.clone(),
            in_largest: self.in_largest.clone(),
            failed_tx: self.failed_tx,
            receipts: self.receipts.clone(),
            receipts_trie: receipts_trie(&self.receipts),
        }
    }
}

impl Fixture {
    pub fn load() -> Self {
        let manifest: Value = serde_json::from_slice(&fixture_file("fixture.json")).unwrap();
        let block = manifest["block"].as_u64().unwrap();
        let receipts: Vec<Value> = serde_json::from_slice::<Value>(&fixture_file(&format!("receipts_{block}.json")))
            .unwrap()["result"]
            .as_array()
            .expect("receipts")
            .clone();
        let receipts_trie = receipts_trie(&receipts);
        Self {
            chain_id: manifest["chain_id"].as_u64().unwrap(),
            checkpoint: root_of(&manifest["bootstrap_root"]),
            bootstrap: LightClientBootstrap::decode(&fixture_file("bootstrap.ssz"), LcFork::Electra, 512).unwrap(),
            update: finality_update(&fixture_file("finality_update.ssz")),
            bootstrap_slot: manifest["bootstrap_slot"].as_u64().unwrap(),
            finalized_slot: manifest["finalized_slot"].as_u64().unwrap(),
            block,
            hop_block: manifest["hop_block"].as_u64().unwrap(),
            weth: address(&manifest["weth"]),
            weth_slot: word(manifest["weth_balance_slot"].as_str().unwrap()),
            empty_slot: word(manifest["empty_slot"].as_str().unwrap()),
            absent: address(&manifest["absent_account"]),
            history_slot: word(manifest["history_slot"].as_str().unwrap()),
            transfer: Transfer::read(&manifest["transfer"]),
            other_recipient: Transfer::read(&manifest["transfer_other_recipient"]),
            other_token: Transfer::read(&manifest["transfer_other_token"]),
            in_largest: Transfer::read(&manifest["transfer_in_largest_receipt"]),
            failed_tx: manifest["failed_tx"].as_u64().unwrap(),
            receipts,
            receipts_trie,
        }
    }

    /// The root of the receipts trie rebuilt from `eth_getBlockReceipts`.
    pub fn receipts_root(&self) -> Root {
        self.receipts_trie.root()
    }

    /// The proof of transaction `tx_index`'s receipt (or of its absence), in `eth_getProof`'s
    /// format.
    pub fn receipt_proof(&self, tx_index: u64) -> Vec<Vec<u8>> {
        let (key, len) = encode_u64(tx_index);
        self.receipts_trie.proof(&key[..len])
    }

    /// `eth_getProof` for WETH at `n`: the holder's balance slot and the empty slot.
    pub fn weth_proof(&self) -> AccountProof {
        account_proof(&fixture_file(&format!("proof_weth_{}.json", self.block)))
    }

    /// `eth_getProof` for WETH at `m`: the holder's balance slot.
    pub fn weth_proof_at_hop(&self) -> AccountProof {
        account_proof(&fixture_file(&format!("proof_weth_{}.json", self.hop_block)))
    }

    /// `eth_getProof` for the address without an account, at `n`.
    pub fn absent_proof(&self) -> AccountProof {
        account_proof(&fixture_file(&format!("proof_absent_{}.json", self.block)))
    }

    /// The hop from `n` to `m`: the history contract's proofs at `n` and `m`'s RLP header.
    pub fn hop(&self) -> Hop {
        let history = account_proof(&fixture_file(&format!("proof_history_{}.json", self.block)));
        let slot = history.storage.iter().find(|s| s.key == self.history_slot).expect("the history slot of m");
        Hop {
            account_proof: history.proof.clone(),
            storage_proof: slot.proof.clone(),
            header: fixture_file(&format!("header_{}.rlp", self.hop_block)),
        }
    }

    /// `eth_call balanceOf(holder)` on WETH at `block`.
    pub fn weth_balance_by_call(&self, block: u64) -> Word {
        let v: Value = serde_json::from_slice(&fixture_file(&format!("call_weth_balance_{block}.json"))).unwrap();
        word(v["result"].as_str().unwrap())
    }

    /// The JSON log at position `log_index` of transaction `tx_index`'s receipt.
    pub fn json_log(&self, tx_index: u64, log_index: u64) -> &Value {
        &self.receipts[tx_index as usize]["logs"][log_index as usize]
    }
}

/// An account with the slots `slots` of its storage, from one `eth_getProof` response.
pub fn account_query(
    proof: &AccountProof,
    conditions: Vec<Condition>,
    slots: Vec<(Word, Vec<Condition>)>,
) -> AccountQuery {
    AccountQuery {
        address: proof.address,
        proof: proof.proof.clone(),
        conditions,
        slots: slots
            .into_iter()
            .map(|(slot, conditions)| SlotQuery {
                slot,
                proof: proof.storage.iter().find(|s| s.key == slot).expect("the slot's proof").proof.clone(),
                conditions,
            })
            .collect(),
    }
}

/// `field <cmp> rhs`.
pub fn condition(field: Field, cmp: Cmp, rhs: Word) -> Condition {
    Condition { field, cmp, rhs }
}

/// A small integer as a 256-bit word.
pub fn u256(n: u128) -> Word {
    let mut out = [0u8; 32];
    out[16..].copy_from_slice(&n.to_be_bytes());
    out
}

// ---------------------------------------------------------------------------------------------
// Statement identifiers, re-derived from JSON-RPC fields
// ---------------------------------------------------------------------------------------------

fn statement(kind: u8, chain_id: u64) -> Sha256 {
    Sha256::new().chain_update(b"ferrata/statement/v1").chain_update([kind]).chain_update(chain_id.to_be_bytes())
}

/// `SHA-256("ferrata/statement/v1" ‖ 0x01 ‖ chain id ‖ address ‖ nonce ‖ balance ‖ storage root
/// ‖ code hash)`, from an `eth_getProof` response.
pub fn account_statement_id(chain_id: u64, proof: &AccountProof) -> Root {
    statement(1, chain_id)
        .chain_update(proof.address)
        .chain_update(proof.nonce.to_be_bytes())
        .chain_update(proof.balance)
        .chain_update(proof.storage_hash)
        .chain_update(proof.code_hash)
        .finalize()
        .into()
}

/// `SHA-256("ferrata/statement/v1" ‖ 0x02 ‖ chain id ‖ address ‖ slot ‖ value)`.
pub fn storage_statement_id(chain_id: u64, address: &Address, slot: &Word, value: &Word) -> Root {
    statement(2, chain_id).chain_update(address).chain_update(slot).chain_update(value).finalize().into()
}

/// `SHA-256("ferrata/statement/v1" ‖ 0x03 ‖ chain id ‖ block ‖ tx index ‖ log index ‖
/// SHA-256("ferrata/log/v1" ‖ address ‖ topic count ‖ topics ‖ data length ‖ data))`, from one log of
/// an `eth_getBlockReceipts` response.
pub fn log_statement_id(chain_id: u64, block: u64, tx_index: u64, log_index: u64, log: &Value) -> Root {
    let topics: Vec<Vec<u8>> = log["topics"].as_array().unwrap().iter().map(|t| hexb(t.as_str().unwrap())).collect();
    let data = hexb(log["data"].as_str().unwrap());
    let mut log_hash = Sha256::new()
        .chain_update(b"ferrata/log/v1")
        .chain_update(hexb(log["address"].as_str().unwrap()))
        .chain_update([topics.len() as u8]);
    for topic in &topics {
        log_hash.update(topic);
    }
    let log_hash = log_hash.chain_update((data.len() as u64).to_be_bytes()).chain_update(&data).finalize();
    statement(3, chain_id)
        .chain_update(block.to_be_bytes())
        .chain_update(tx_index.to_be_bytes())
        .chain_update(log_index.to_be_bytes())
        .chain_update(log_hash)
        .finalize()
        .into()
}

// ---------------------------------------------------------------------------------------------
// The chain
// ---------------------------------------------------------------------------------------------

/// A chain whose mainnet instance was initialized from the fixture's bootstrap and has verified
/// its finality update through the public job: `header(id, bootstrap slot)` and
/// `header(id, finalized slot)` hold their records. `Clone`, so that a measurement can run the same
/// transaction on copies of one chain (`accumulated`).
#[derive(Clone)]
pub struct Stage4 {
    pub w: World,
    pub fx: Fixture,
    /// A block time after the finality update's signature slot, for the transactions here
    /// (`Attest` carries no validity window).
    pub time: u64,
}

impl Default for Stage4 {
    fn default() -> Self {
        Self::new()
    }
}

/// The state of an actor of the consumer, if it has one.
pub fn consumer_state(chain: &Chain, actor: Actor) -> Option<ConsumerState> {
    let state = chain.actor_state(actor);
    (!state.is_empty()).then(|| borsh::from_slice(&state).expect("a consumer state"))
}

impl Stage4 {
    pub fn new() -> Self {
        Self::at(crate::ferrata_account())
    }

    /// The same chain for the deployment of the program at `program`.
    pub fn at(program: AccountId) -> Self {
        let fx = Fixture::load();
        let mut w = World::at(program, mainnet_config(2), fx.checkpoint, fx.bootstrap.clone(), vec![fx.update.clone()]);
        assert_eq!(w.bootstrap.header.beacon.slot, fx.bootstrap_slot);
        let update = fx.update.clone();
        let run = w.apply(&update, "finality update");
        assert_eq!(run.reports.len(), 3, "a finality update is three transactions");
        assert_eq!(run.outcome.map(|o| o.advanced), Some(true), "the update finalizes a new header");
        assert_eq!(w.inst.store(&w.chain).finalized.slot, fx.finalized_slot);
        let time = w.inst.config.slot_start_ms(update.signature_slot) + 12_000;
        Self { w, fx, time }
    }

    /// `header(id, slot)` of the instance.
    pub fn header(&self, slot: u64) -> Actor {
        header_actor(self.w.inst.program, &self.w.inst.id, slot)
    }

    /// The record the chain holds at `header(id, slot)`.
    pub fn record(&self, slot: u64) -> RecordState {
        self.w.inst.record(&self.w.chain, slot).expect("a record at this slot")
    }

    /// An empty request bound to the instance and its chain.
    pub fn request(&self) -> Request {
        Request { instance_id: self.w.inst.id, chain_id: self.fx.chain_id, ..Request::default() }
    }

    /// A request for one log of the finalized block.
    pub fn log_request(&self, tx_index: u64, log_index: u64, conditions: Vec<Condition>) -> Request {
        Request {
            logs: vec![LogQuery { tx_index, log_index, proof: self.fx.receipt_proof(tx_index), conditions }],
            ..self.request()
        }
    }

    /// What the program must answer: the function its handler runs, run here on the record the
    /// chain holds.
    pub fn expected(&self, slot: u64, request: &Request) -> Result<Attestation, u8> {
        let state = self.record(slot);
        evaluate(&state.instance_id, state.chain_id, &state.header().expect("a record"), request)
    }

    /// `Attest` at `header(id, slot)` as the root of a transaction.
    pub fn attest(&mut self, label: &str, slot: u64, request: &Request) -> TxReport {
        let header = self.header(slot);
        self.attest_at(label, header, request, vec![])
    }

    /// `Attest` at any actor as the root of a transaction.
    pub fn attest_at(
        &mut self,
        label: &str,
        actor: Actor,
        request: &Request,
        evidence: Vec<PublicAccountEvidence>,
    ) -> TxReport {
        let message = borsh::to_vec(&Message::Attest { request: request.clone() }).unwrap();
        self.w.chain.submit(label, actor, message, vec![actor], evidence, self.time)
    }

    /// An accepted root attestation: the transaction is applied, and the event names exactly the
    /// identifiers the native evaluation gives. Returns that evaluation.
    pub fn accept(&mut self, label: &str, slot: u64, request: &Request) -> (TxReport, Attestation) {
        let expected = self.expected(slot, request).unwrap_or_else(|code| panic!("{label}: refused natively ({code})"));
        let report = self.attest(label, slot, request);
        assert!(report.ok(), "{label}: {:?} (exit {:?})", report.charge.revert, report.exit_code);
        let event = report
            .events
            .iter()
            .find(|e| e.event.selector == ATTESTED_SELECTOR)
            .unwrap_or_else(|| panic!("{label}: no attestation event"));
        assert_eq!(event.account_id, self.w.inst.program, "{label}: the program emits the event");
        assert_eq!(event.event.data, expected.event_data(), "{label}: the event names the expected identifiers");
        (report, expected)
    }

    /// A refused root attestation: the transaction reverts with `code`, as the native evaluation
    /// refuses with the same code.
    pub fn refuse(&mut self, label: &str, slot: u64, request: &Request, code: u8) -> TxReport {
        assert_eq!(self.expected(slot, request).err(), Some(code), "{label}: the native evaluation");
        let report = self.attest(label, slot, request);
        assert_eq!(report.exit_code, Some(u32::from(code)), "{label}: {:?}", report.charge.revert);
        assert!(report.events.is_empty(), "{label}: a refusal emits nothing");
        report
    }

    /// A public actor of the consumer and the evidence that admits it.
    pub fn reader(&self) -> (Actor, Vec<PublicAccountEvidence>) {
        let seed = PdaSeed::new([0x44; 32]);
        let program = consumer_account();
        (
            Actor::new(AccountId::for_public_pda(&program, &seed), program),
            vec![PublicAccountEvidence::Pda { program, seed }],
        )
    }

    /// A public actor of the consumer calls `Attest` at `header(id, slot)` and keeps the reply
    /// (`CallAttest`). Returns the reply it stored, if the transaction was applied.
    pub fn call(&mut self, label: &str, slot: u64, request: &Request) -> (TxReport, Option<Attestation>) {
        let (reader, evidence) = self.reader();
        let header = self.header(slot);
        let message = ConsumerMessage::CallAttest { ferrata: self.w.inst.program, slot, request: request.clone() };
        let report = self.w.chain.submit(
            label,
            reader,
            borsh::to_vec(&message).unwrap(),
            vec![reader, header],
            evidence,
            self.time,
        );
        let reply = match consumer_state(&self.w.chain, reader) {
            Some(ConsumerState::Reader(state)) if report.ok() => {
                state.attestation.map(|bytes| borsh::from_slice(&bytes).expect("an attestation"))
            }
            _ => None,
        };
        (report, reply)
    }

    /// The pool of the reference consumer: this instance, mainnet, the fixture's token and
    /// recipient.
    pub fn pool(&self) -> Pool {
        Pool {
            ferrata: self.w.inst.program,
            instance_id: self.w.inst.id,
            chain_id: self.fx.chain_id,
            token: self.fx.transfer.token,
            recipient: self.fx.transfer.to,
        }
    }

    /// The identifier of `log` in the finalized block, from the receipts JSON alone.
    pub fn transfer_id(&self, log: &Transfer) -> Root {
        log_statement_id(
            self.fx.chain_id,
            self.fx.block,
            log.tx_index,
            log.log_index,
            self.fx.json_log(log.tx_index, log.log_index),
        )
    }

    /// `Claim` of `log` for `pool`, sent to the claim actor that `statement_id` names.
    pub fn claim(&mut self, label: &str, pool: &Pool, slot: u64, log: &Transfer, statement_id: &Root) -> TxReport {
        let program = consumer_account();
        let pool_id = pool.id();
        let spent = nullifier(&pool_id, statement_id);
        let claim = claim_actor(program, &pool_id, &spent);
        let balance = balance_actor(program, &pool_id, &log.from);
        let header = header_actor(pool.ferrata, &pool.instance_id, slot);
        let message = ConsumerMessage::Claim {
            pool: pool.clone(),
            slot,
            tx_index: log.tx_index,
            log_index: log.log_index,
            proof: self.fx.receipt_proof(log.tx_index),
            header: None,
        };
        self.w.chain.submit(
            label,
            claim,
            borsh::to_vec(&message).unwrap(),
            vec![claim, header, balance],
            vec![PublicAccountEvidence::Pda { program, seed: claim_seed(&pool_id, &spent) }],
            self.time,
        )
    }

    /// The claim actor of `statement_id` in `pool`, and what it holds.
    pub fn claim_state(&self, pool: &Pool, statement_id: &Root) -> Option<ClaimState> {
        let pool_id = pool.id();
        let actor = claim_actor(consumer_account(), &pool_id, &nullifier(&pool_id, statement_id));
        match consumer_state(&self.w.chain, actor) {
            Some(ConsumerState::Claim(state)) => Some(state),
            None => None,
            Some(other) => panic!("a claim actor holds {other:?}"),
        }
    }

    /// What `owner` was credited in `pool`.
    pub fn balance(&self, pool: &Pool, owner: &Address) -> Option<BalanceState> {
        match consumer_state(&self.w.chain, balance_actor(consumer_account(), &pool.id(), owner)) {
            Some(ConsumerState::Balance(state)) => Some(state),
            None => None,
            Some(other) => panic!("a balance actor holds {other:?}"),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Scenarios, shared by the tests (which assert more) and the `tables` binary
// ---------------------------------------------------------------------------------------------

/// `pool` with the token and recipient of `log`: the pool for which `log` is a deposit.
pub fn pool_for(pool: &Pool, log: &Transfer) -> Pool {
    Pool { token: log.token, recipient: log.to, ..pool.clone() }
}

/// The pool's rule for a deposit, as conditions on a log.
pub(crate) fn transfer_conditions(token: &Address, to: &Address) -> Vec<Condition> {
    Pool { ferrata: AccountId::new([0; 32]), instance_id: ZERO, chain_id: 0, token: *token, recipient: *to }
        .conditions()
}

/// One accepted attestation of each kind, each sent as the root of a transaction to the record of
/// the finalized block, and checked against JSON-RPC. Returns the chain and the reports.
pub fn accepted() -> (Stage4, Vec<TxReport>) {
    accepted_at(crate::ferrata_account())
}

/// [`accepted`] for the deployment of the program at `program` (the comparison build, which
/// verifies with software Keccak).
pub fn accepted_at(program: AccountId) -> (Stage4, Vec<TxReport>) {
    let mut s = Stage4::at(program);
    let chain_id = s.fx.chain_id;
    let slot = s.fx.finalized_slot;
    let mut out = Vec::new();
    // The record is the finalized block's, and the receipts trie rebuilt from JSON has its root.
    let record = s.record(slot).header().unwrap();
    let ferrata_core::witness::ExecutionSummary::Payload { block_number, receipts_root, .. } = record.execution else {
        panic!("a Fulu record holds payload fields");
    };
    assert_eq!((block_number, receipts_root), (s.fx.block, s.fx.receipts_root()));

    // 1. An account: the WETH contract. Its four fields are the ones eth_getProof reports.
    let weth = s.fx.weth_proof();
    let request = Request {
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
    };
    let (report, attestation) =
        s.accept("account: the WETH contract (nonce, balance, storage root, code hash)", slot, &request);
    assert_eq!(
        attestation.statements[0].fact,
        Fact::Account {
            address: weth.address,
            nonce: weth.nonce,
            balance: weth.balance,
            storage_root: weth.storage_hash,
            code_hash: weth.code_hash
        }
    );
    assert_eq!(attestation.statements[0].id, account_statement_id(chain_id, &weth));
    assert_eq!((attestation.block_number, attestation.slot, attestation.hops), (s.fx.block, slot, 0));
    out.push(report);

    // 2. A storage slot: WETH's balance of the holder, at least one wei. The value is the one
    //    eth_getProof reports and `balanceOf` returns.
    let balance = weth.storage.iter().find(|p| p.key == s.fx.weth_slot).unwrap().value;
    assert_eq!(balance, s.fx.weth_balance_by_call(s.fx.block));
    assert_ne!(balance, ZERO);
    let weth_slot = s.fx.weth_slot;
    let storage = Request {
        accounts: vec![account_query(
            &weth,
            vec![],
            vec![(weth_slot, vec![condition(Field::Value, Cmp::Ge, u256(1))])],
        )],
        ..s.request()
    };
    let (report, attestation) = s.accept("storage slot: a WETH balance, value >= 1", slot, &storage);
    assert_eq!(attestation.statements.len(), 2, "the account, then its slot");
    assert_eq!(
        attestation.statements[1].fact,
        Fact::Storage { address: weth.address, slot: weth_slot, value: balance }
    );
    assert_eq!(attestation.statements[1].id, storage_statement_id(chain_id, &weth.address, &weth_slot, &balance));
    out.push(report);

    // 3. A slot that is not there: value zero, proven by exclusion.
    let empty_slot = s.fx.empty_slot;
    let request = Request {
        accounts: vec![account_query(&weth, vec![], vec![(empty_slot, vec![condition(Field::Value, Cmp::Eq, ZERO)])])],
        ..s.request()
    };
    let (report, attestation) = s.accept("storage slot: an absent slot is zero", slot, &request);
    assert_eq!(attestation.statements[1].fact, Fact::Storage { address: weth.address, slot: empty_slot, value: ZERO });
    out.push(report);

    // 4. An address without an account: every field zero.
    let absent = s.fx.absent_proof();
    assert_eq!(absent.address, s.fx.absent);
    let request = Request {
        accounts: vec![account_query(&absent, vec![condition(Field::CodeHash, Cmp::Eq, ZERO)], vec![])],
        ..s.request()
    };
    let (report, attestation) = s.accept("account: an address without an account", slot, &request);
    assert_eq!(
        attestation.statements[0].fact,
        Fact::Account { address: absent.address, nonce: 0, balance: ZERO, storage_root: ZERO, code_hash: ZERO }
    );
    out.push(report);

    // 5. An event log: an ERC-20 Transfer to a contract, with the reference consumer's conditions.
    let t = s.fx.transfer.clone();
    let request = s.log_request(t.tx_index, t.log_index, transfer_conditions(&t.token, &t.to));
    let label = format!("event log: an ERC-20 Transfer (receipt with {} logs)", t.logs_in_receipt);
    let (report, attestation) = s.accept(&label, slot, &request);
    check_transfer(&s, &attestation.statements[0].fact, &attestation.statements[0].id, &t);
    out.push(report);

    // 6. The same in the block's largest receipt: the whole receipt is the trie leaf.
    let t = s.fx.in_largest.clone();
    let request = s.log_request(t.tx_index, t.log_index, transfer_conditions(&t.token, &t.to));
    let label = format!("event log: a Transfer in the block's largest receipt ({} logs)", t.logs_in_receipt);
    let (report, attestation) = s.accept(&label, slot, &request);
    check_transfer(&s, &attestation.statements[0].fact, &attestation.statements[0].id, &t);
    out.push(report);

    // 7. An older block: one EIP-2935 hop from n to m = n - 16, then the same WETH slot there.
    let older = s.fx.weth_proof_at_hop();
    let older_balance = older.storage.iter().find(|p| p.key == weth_slot).unwrap().value;
    assert_eq!(older_balance, s.fx.weth_balance_by_call(s.fx.hop_block));
    let request = Request {
        hops: vec![s.fx.hop()],
        accounts: vec![account_query(&older, vec![], vec![(weth_slot, vec![])])],
        ..s.request()
    };
    let (report, attestation) =
        s.accept("storage slot 16 blocks back: one EIP-2935 hop, then the slot", slot, &request);
    assert_eq!((attestation.block_number, attestation.hops, attestation.slot), (s.fx.hop_block, 1, slot));
    assert_eq!(
        attestation.statements[1].fact,
        Fact::Storage { address: weth.address, slot: weth_slot, value: older_balance }
    );
    assert_eq!(attestation.statements[1].id, storage_statement_id(chain_id, &weth.address, &weth_slot, &older_balance));
    out.push(report);

    // 8. Several facts in one call, with a relation between two of them: the WETH account, two of
    //    its slots, a log; the balance slot is greater than the empty one.
    let t = s.fx.transfer.clone();
    let request = Request {
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
    };
    let (report, attestation) =
        s.accept("four facts in one call (account, two slots, a log) and a relation", slot, &request);
    assert_eq!(attestation.statements.len(), 4);
    check_transfer(&s, &attestation.statements[3].fact, &attestation.statements[3].id, &t);
    out.push(report);
    (s, out)
}

/// `fact` is the transfer `t` as `eth_getBlockReceipts` reports it, and `id` its identifier
/// derived from that JSON.
pub fn check_transfer(s: &Stage4, fact: &Fact, id: &Root, t: &Transfer) {
    let log = s.fx.json_log(t.tx_index, t.log_index);
    assert_eq!(log["logIndex"].as_str().map(|i| u64::from_str_radix(&i[2..], 16).unwrap()), Some(t.block_log_index));
    let topics: Vec<Root> = log["topics"].as_array().unwrap().iter().map(root_of).collect();
    assert_eq!(
        *fact,
        Fact::Log {
            tx_index: t.tx_index,
            log_index: t.log_index,
            address: address(&log["address"]),
            topics: topics.clone(),
            data: hexb(log["data"].as_str().unwrap()),
        }
    );
    // The manifest's reading of the log is the JSON's.
    assert_eq!(address(&log["address"]), t.token);
    assert_eq!((topics[1][12..].to_vec(), topics[2][12..].to_vec()), (t.from.to_vec(), t.to.to_vec()));
    assert_eq!(hexb(log["data"].as_str().unwrap()), t.value.to_vec());
    assert_eq!(*id, s.transfer_id(t));
}

/// The reference consumer on the public path: a generic caller's `CallAttest` for a storage slot
/// and for a log (the reply it stores is the whole attestation), then a claim, which credits the
/// transfer's sender once, and its replay.
pub fn consumer_path() -> (Stage4, Vec<TxReport>) {
    let mut s = Stage4::new();
    let slot = s.fx.finalized_slot;
    let mut out = Vec::new();

    let weth = s.fx.weth_proof();
    let weth_slot = s.fx.weth_slot;
    let storage = Request {
        accounts: vec![account_query(
            &weth,
            vec![],
            vec![(weth_slot, vec![condition(Field::Value, Cmp::Ge, u256(1))])],
        )],
        ..s.request()
    };
    let (report, reply) = s.call("a public actor calls Attest: storage slot", slot, &storage);
    assert!(report.ok(), "{:?}", report.charge.revert);
    assert_eq!(reply, Some(s.expected(slot, &storage).unwrap()), "the reply is the whole attestation");
    out.push(report);

    let t = s.fx.transfer.clone();
    let log = s.log_request(t.tx_index, t.log_index, transfer_conditions(&t.token, &t.to));
    let (report, reply) = s.call("a public actor calls Attest: event log", slot, &log);
    assert!(report.ok(), "{:?}", report.charge.revert);
    assert_eq!(reply, Some(s.expected(slot, &log).unwrap()));
    out.push(report);

    // The claim: one transaction, four handler executions.
    let pool = s.pool();
    let id = s.transfer_id(&t);
    assert!(s.claim_state(&pool, &id).is_none() && s.balance(&pool, &t.from).is_none());
    let report =
        s.claim("reference consumer: claim an ERC-20 Transfer (credits its sender once)", &pool, slot, &t, &id);
    assert!(report.ok(), "{:?} (exit {:?})", report.charge.revert, report.exit_code);
    assert_eq!(report.transitions.len(), 4, "Claim, Attest, the reply, Credit");
    let Some(ClaimState::Spent(deposit)) = s.claim_state(&pool, &id) else { panic!("the nullifier is spent") };
    assert_eq!((deposit.statement_id, deposit.owner, deposit.amount), (id, t.from, t.amount()));
    assert_eq!(deposit.block_number, s.fx.block);
    let balance = s.balance(&pool, &t.from).expect("the sender is credited");
    assert_eq!((balance.pool_id, balance.owner, balance.amount, balance.credits), (pool.id(), t.from, t.amount(), 1));
    out.push(report);

    // The same deposit again: refused by its nullifier before anything is verified.
    let replay = s.claim("reference consumer: the same claim again (replay)", &pool, slot, &t, &id);
    assert_eq!(replay.exit_code, Some(u32::from(consumer_code::ALREADY_CLAIMED)));
    assert_eq!(replay.transitions.len(), 1, "refused in the claim actor itself");
    assert_eq!(s.balance(&pool, &t.from).map(|b| (b.amount, b.credits)), Some((t.amount(), 1)), "credited once");
    out.push(replay);
    (s, out)
}

/// Flip one bit of the first node of `proof` that is at least `at + 1` bytes long.
fn corrupt(proof: &mut [Vec<u8>], at: usize) {
    let node = proof.iter_mut().find(|n| n.len() > at).expect("a node to corrupt");
    node[at] ^= 0x01;
}

/// Every refusal of the attestation and of the reference consumer, each through LEZ's settlement
/// against the chain of [`Stage4::new`]. A refusal changes no state, so they share one chain.
pub fn refusals() -> Vec<Refusal> {
    let mut s = Stage4::new();
    let slot = s.fx.finalized_slot;
    let mut out = Vec::new();
    let mut push = |case: &'static str, report: TxReport| out.push(Refusal { case, report });

    let weth = s.fx.weth_proof();
    let weth_slot = s.fx.weth_slot;
    let empty_slot = s.fx.empty_slot;
    let storage = |s: &Stage4, conditions: Vec<Condition>| Request {
        accounts: vec![account_query(&weth, vec![], vec![(weth_slot, conditions)])],
        ..s.request()
    };
    let t = s.fx.transfer.clone();
    let log = s.log_request(t.tx_index, t.log_index, vec![]);
    // The requests the refusals are variations of are accepted as they are.
    s.accept("control: the storage request", slot, &storage(&s, vec![]));
    s.accept("control: the log request", slot, &log);

    // --- Chain and instance ---------------------------------------------------------------------
    // A consumer configured for another chain (Sepolia's id), asking mainnet's record.
    let request = Request { chain_id: 11_155_111, ..storage(&s, vec![]) };
    push("a request for another chain id", s.refuse("another chain id", slot, &request, 140));
    // A consumer bound to another instance.
    let request = Request { instance_id: [0x5a; 32], ..storage(&s, vec![]) };
    push("a request bound to another instance", s.refuse("another instance", slot, &request, code::WRONG_INSTANCE));
    // A second instance of the same program: the same checkpoint and committee, configured as
    // another chain. Its record of the same block is not the first instance's record: a request
    // bound to the first instance is refused there, and so is one that names the second instance
    // but expects mainnet's chain id.
    let mut config = mainnet_config(2);
    config.chain_id = 11_155_111;
    let other = Instance::new(s.w.inst.program, config, s.fx.checkpoint);
    assert_ne!(other.id, s.w.inst.id, "the chain id is part of the instance id");
    let init = other.init(&mut s.w.chain, &s.fx.bootstrap.clone(), "an instance configured as another chain");
    assert!(init.reports.iter().all(TxReport::ok));
    let other_header = header_actor(other.program, &other.id, s.fx.bootstrap_slot);
    let other_record = other.record(&s.w.chain, s.fx.bootstrap_slot).expect("the second instance's record");
    assert_eq!((other_record.instance_id, other_record.chain_id), (other.id, 11_155_111));
    let report = s.attest_at("another instance's record", other_header, &storage(&s, vec![]), vec![]);
    assert_eq!(report.exit_code, Some(u32::from(code::WRONG_INSTANCE)));
    push("a request bound to instance A, sent to instance B's record", report);
    let request = Request { instance_id: other.id, ..storage(&s, vec![]) };
    let report = s.attest_at("the other instance, mainnet's chain id", other_header, &request, vec![]);
    assert_eq!(report.exit_code, Some(140), "ChainMismatch");
    push("a request naming instance B but expecting mainnet's chain id", report);

    // --- Records --------------------------------------------------------------------------------
    // No record at this slot (the account is admitted by evidence, so that the program refuses).
    let empty = header_actor(s.w.inst.program, &s.w.inst.id, slot + 1);
    let evidence =
        vec![PublicAccountEvidence::Pda { program: s.w.inst.program, seed: header_seed(&s.w.inst.id, slot + 1) }];
    let report = s.attest_at("no record", empty, &storage(&s, vec![]), evidence);
    assert_eq!(report.exit_code, Some(u32::from(code::NO_RECORD)));
    push("an actor that holds no record (no header at this slot)", report);
    // The same image deployed at another address: its actors are not this program's records.
    let twin = header_actor(TWIN, &s.w.inst.id, slot);
    let evidence = vec![PublicAccountEvidence::Pda { program: TWIN, seed: header_seed(&s.w.inst.id, slot) }];
    let report = s.attest_at("twin deployment", twin, &storage(&s, vec![]), evidence);
    assert_eq!(report.exit_code, Some(u32::from(code::NO_RECORD)));
    push("header(id, slot) of another deployment of the same image", report);
    // An actor of the program whose state is not a record: the instance itself.
    let instance = s.w.inst.actor();
    let report = s.attest_at("the instance actor", instance, &storage(&s, vec![]), vec![]);
    assert_eq!(report.exit_code, Some(u32::from(code::BAD_STATE)));
    push("an actor of the program that is not a record (the instance)", report);
    // A proof that is valid for another header: the finalized block's facts against the record
    // of the bootstrap's block.
    let bootstrap_slot = s.fx.bootstrap_slot;
    push(
        "a storage proof valid for another header's block",
        s.refuse("storage, another record", bootstrap_slot, &storage(&s, vec![]), 110),
    );
    push(
        "a receipt proof valid for another header's block",
        s.refuse("log, another record", bootstrap_slot, &log, 110),
    );

    // --- Proofs ---------------------------------------------------------------------------------
    // One bit of a proof node, in the account proof and in the storage proof.
    let mut request = storage(&s, vec![]);
    corrupt(&mut request.accounts[0].proof, 40);
    push("a forged node in the account proof", s.refuse("forged account node", slot, &request, 110));
    let mut request = storage(&s, vec![]);
    let last = request.accounts[0].slots[0].proof.len() - 1;
    corrupt(&mut request.accounts[0].slots[0].proof[last..], 3);
    push("a forged node in the storage proof (its leaf)", s.refuse("forged storage leaf", slot, &request, 110));
    let mut request = log.clone();
    let last = request.logs[0].proof.len() - 1;
    let middle = request.logs[0].proof[last].len() / 2;
    corrupt(&mut request.logs[0].proof[last..], middle);
    push("a forged receipt (one bit of the trie leaf)", s.refuse("forged receipt", slot, &request, 110));
    // The proof of one slot presented for another, and of one account for another address.
    let mut request = storage(&s, vec![]);
    request.accounts[0].slots[0].slot = empty_slot;
    let wrong_slot = s.expected(slot, &request).expect_err("a proof of another slot");
    assert!([110, 111, 112].contains(&wrong_slot), "an inclusion-proof refusal, got {wrong_slot}");
    push("a storage proof presented for another slot", s.refuse("another slot", slot, &request, wrong_slot));
    let mut request = storage(&s, vec![]);
    request.accounts[0].address = s.fx.absent;
    let wrong_address = s.expected(slot, &request).expect_err("a proof of another account");
    assert!([110, 111, 112].contains(&wrong_address), "an inclusion-proof refusal, got {wrong_address}");
    push("an account proof presented for another address", s.refuse("another address", slot, &request, wrong_address));
    // A truncated proof, and one with a node appended.
    let mut request = storage(&s, vec![]);
    request.accounts[0].slots[0].proof.pop();
    push("a storage proof without its last node", s.refuse("truncated proof", slot, &request, 111));
    let mut request = storage(&s, vec![]);
    let extra = request.accounts[0].proof[0].clone();
    request.accounts[0].slots[0].proof.push(extra);
    push("a storage proof with a node appended", s.refuse("extended proof", slot, &request, 112));

    // --- Predicates and relations ---------------------------------------------------------------
    let request = storage(&s, vec![condition(Field::Value, Cmp::Lt, u256(1))]);
    push("a predicate that does not hold (a positive balance < 1)", s.refuse("false predicate", slot, &request, 130));
    let request = s.log_request(t.tx_index, t.log_index, transfer_conditions(&t.token, &s.fx.other_recipient.to));
    push("a log that is not a Transfer to the expected recipient", s.refuse("another recipient", slot, &request, 130));
    let request = storage(&s, vec![condition(Field::Topic(0), Cmp::Eq, ZERO)]);
    push("a condition on a field the fact does not have", s.refuse("field of another kind", slot, &request, 131));
    let mut request = storage(&s, vec![]);
    request.relations = vec![Relation {
        left: Operand { fact: 1, field: Field::Value },
        cmp: Cmp::Lt,
        right: Operand { fact: 0, field: Field::Nonce },
    }];
    push(
        "a relation that does not hold (a balance < the contract's nonce)",
        s.refuse("false relation", slot, &request, 130),
    );

    // --- Logs -----------------------------------------------------------------------------------
    let request = s.log_request(t.tx_index, t.logs_in_receipt, vec![]);
    push("a log index past the receipt's last log", s.refuse("absent log index", slot, &request, 125));
    let receipts = s.fx.receipts.len() as u64;
    let request = s.log_request(receipts, 0, vec![]);
    push("a transaction index past the block's last transaction", s.refuse("absent receipt", slot, &request, 122));
    let failed = s.fx.failed_tx;
    let request = s.log_request(failed, 0, vec![]);
    push("a log of a transaction that failed", s.refuse("failed transaction", slot, &request, 124));
    // The receipt of one transaction presented for another index.
    let mut request = log.clone();
    request.logs[0].tx_index = t.tx_index + 1;
    let wrong_tx = s.expected(slot, &request).expect_err("a receipt at another index");
    assert!([110, 111, 112].contains(&wrong_tx), "an inclusion-proof refusal, got {wrong_tx}");
    push(
        "a receipt proof presented for another transaction index",
        s.refuse("another index", slot, &request, wrong_tx),
    );

    // --- History hops ---------------------------------------------------------------------------
    let older = s.fx.weth_proof_at_hop();
    let hopped = |s: &Stage4, hop: Hop| Request {
        hops: vec![hop],
        accounts: vec![account_query(&older, vec![], vec![(weth_slot, vec![])])],
        ..s.request()
    };
    // The record's own header as the "older" block: not older.
    let own = Hop { header: fixture_file(&format!("header_{}.rlp", s.fx.block)), ..s.fx.hop() };
    push(
        "a hop to a block that is not older than the record's",
        s.refuse("hop out of window", slot, &hopped(&s, own), 150),
    );
    // A header the history contract does not hold: one bit of its last field.
    let mut forged = s.fx.hop();
    let end = forged.header.len() - 1;
    forged.header[end] ^= 0x01;
    push(
        "a hop with a header the history contract does not hold",
        s.refuse("hop hash", slot, &hopped(&s, forged), 151),
    );
    // The older block's proof without the hop: the newer block's state does not hold it.
    let request = Request { hops: vec![], ..hopped(&s, s.fx.hop()) };
    push("an older block's proof without its hop", s.refuse("no hop", slot, &request, 110));

    // --- Malformed input ------------------------------------------------------------------------
    push("an empty request", s.refuse("empty request", slot, &s.request(), 131));
    let mut request = storage(&s, vec![]);
    request.leaves =
        vec![ferrata_lez_wire::attest::LeafQuery { root: 0, leaf_index: 0, leaf: [1; 32], path: vec![ZERO; 32] }];
    push(
        "an accumulator leaf whose root is not a storage fact",
        s.refuse("leaf of an account", slot, &request, code::BAD_QUERY),
    );
    let mut request = storage(&s, vec![]);
    request.accounts[0].proof[0] = vec![0xc1];
    let bad_node = s.expected(slot, &request).expect_err("a node that is not the root's");
    push("a proof whose first node is replaced by two junk bytes", s.refuse("bad node", slot, &request, bad_node));
    let header = s.header(slot);
    let mut message = borsh::to_vec(&Message::Attest { request: storage(&s, vec![]) }).unwrap();
    message.truncate(message.len() - 1);
    let report = s.w.chain.submit("truncated message", header, message.clone(), vec![header], vec![], s.time);
    assert_eq!(report.exit_code, Some(u32::from(code::BAD_MESSAGE)));
    push("a message cut short by one byte", report);
    message.extend([0, 0]);
    let report = s.w.chain.submit("extended message", header, message, vec![header], vec![], s.time);
    assert_eq!(report.exit_code, Some(u32::from(code::BAD_MESSAGE)));
    push("a message with a byte appended", report);

    // --- The reference consumer -----------------------------------------------------------------
    let pool = s.pool();
    let id = s.transfer_id(&t);
    // A transfer to another recipient, and one of another token, claimed for the pool: the
    // program refuses the pool's predicates.
    let wrong = s.fx.other_recipient.clone();
    let report = s.claim("claim: another recipient", &pool, slot, &wrong, &s.transfer_id(&wrong));
    assert_eq!(report.exit_code, Some(130), "PredicateFailed, from the program");
    push("consumer: a Transfer of the token to another recipient", report);
    let wrong = s.fx.other_token.clone();
    let report = s.claim("claim: another token", &pool, slot, &wrong, &s.transfer_id(&wrong));
    assert_eq!(report.exit_code, Some(130));
    push("consumer: a Transfer of another token", report);
    // The right log claimed at a claim actor its identifier does not name.
    let report = s.claim("claim: wrong nullifier", &pool, slot, &t, &[0x77; 32]);
    assert_eq!(report.exit_code, Some(u32::from(consumer_code::NULLIFIER_MISMATCH)));
    push("consumer: a claim sent to a claim actor the statement does not name", report);
    // A pool bound to another chain id: the program refuses its request.
    let sepolia = Pool { chain_id: 11_155_111, ..pool.clone() };
    let report = s.claim("claim: pool of another chain", &sepolia, slot, &t, &id);
    assert_eq!(report.exit_code, Some(140), "ChainMismatch, from the program");
    push("consumer: a pool configured for another chain id", report);
    // A forged proof through the consumer.
    let mut forged = ConsumerMessage::Claim {
        pool: pool.clone(),
        slot,
        tx_index: t.tx_index,
        log_index: t.log_index,
        proof: s.fx.receipt_proof(t.tx_index),
        header: None,
    };
    if let ConsumerMessage::Claim { proof, .. } = &mut forged {
        let last = proof.len() - 1;
        let middle = proof[last].len() / 2;
        corrupt(&mut proof[last..], middle);
    }
    let program = consumer_account();
    let pool_id = pool.id();
    let spent = nullifier(&pool_id, &id);
    let claim = claim_actor(program, &pool_id, &spent);
    let balance = balance_actor(program, &pool_id, &t.from);
    let claim_evidence = vec![PublicAccountEvidence::Pda { program, seed: claim_seed(&pool_id, &spent) }];
    let report = s.w.chain.submit(
        "claim: forged receipt",
        claim,
        borsh::to_vec(&forged).unwrap(),
        vec![claim, header, balance],
        claim_evidence.clone(),
        s.time,
    );
    assert_eq!(report.exit_code, Some(110));
    push("consumer: a claim with a forged receipt", report);
    // Nothing above credited anyone, and the nullifier is still unspent.
    assert!(s.claim_state(&pool, &id).is_none() && s.balance(&pool, &t.from).is_none());

    // `Credit` from anyone but the claim actor: as a transaction's root, and from an actor of
    // another program.
    let credit = ConsumerMessage::Credit { pool_id, nullifier: spent, owner: t.from, amount: 1_000_000 };
    let balance_evidence =
        vec![PublicAccountEvidence::Pda { program, seed: ferrata_lez_wire::consumer::balance_seed(&pool_id, &t.from) }];
    let report = s.w.chain.submit(
        "Credit as root",
        balance,
        borsh::to_vec(&credit).unwrap(),
        vec![balance],
        balance_evidence.clone(),
        s.time,
    );
    assert_eq!(report.exit_code, Some(u32::from(consumer_code::WRONG_SENDER)));
    push("consumer: Credit sent as the transaction root", report);
    let seed = PdaSeed::new([7; 32]);
    let attacker = Actor::new(AccountId::for_public_pda(&scripted_account(), &seed), scripted_account());
    let attacker_evidence = PublicAccountEvidence::Pda { program: scripted_account(), seed };
    let script = Script::default().send_sync(balance, &credit);
    let report = s.w.chain.submit(
        "Credit from another program",
        attacker,
        borsh::to_vec(&script).unwrap(),
        vec![attacker, balance],
        [vec![attacker_evidence.clone()], balance_evidence].concat(),
        s.time,
    );
    assert_eq!(report.exit_code, Some(u32::from(consumer_code::WRONG_SENDER)));
    push("consumer: Credit from another program's actor", report);
    // An attestation nobody asked for, sent to a claim actor by another program's actor: it is
    // not a reply (the actor waits for nothing), so it is not even a message of the consumer.
    let genuine =
        s.expected(slot, &pool.request(t.tx_index, t.log_index, s.fx.receipt_proof(t.tx_index), None)).unwrap();
    let script = Script::default().send_sync(claim, &genuine);
    let report = s.w.chain.submit(
        "unsolicited attestation",
        attacker,
        borsh::to_vec(&script).unwrap(),
        vec![attacker, claim],
        [vec![attacker_evidence], claim_evidence].concat(),
        s.time,
    );
    assert_eq!(report.exit_code, Some(u32::from(consumer_code::BAD_MESSAGE)));
    push("consumer: an attestation it did not ask for, from another program's actor", report);
    assert!(s.claim_state(&pool, &id).is_none() && s.balance(&pool, &t.from).is_none());

    // The claim itself, then its replay.
    let report = s.claim("claim", &pool, slot, &t, &id);
    assert!(report.ok(), "{:?} (exit {:?})", report.charge.revert, report.exit_code);
    let report = s.claim("claim again", &pool, slot, &t, &id);
    assert_eq!(report.exit_code, Some(u32::from(consumer_code::ALREADY_CLAIMED)));
    push("consumer: the same deposit claimed again (replay)", report);
    assert_eq!(s.balance(&pool, &t.from).map(|b| (b.amount, b.credits)), Some((t.amount(), 1)));

    // Every refusal left the instance as it was.
    s.w.check_store();
    out
}

// ---------------------------------------------------------------------------------------------
// The private path, in the executor
// ---------------------------------------------------------------------------------------------

/// One handler execution in RISC Zero's executor (no proof): what a prover would have to prove.
#[derive(Clone, Debug)]
pub struct Executed {
    pub what: String,
    /// Size of the borsh `ReceiveInput` the guest reads.
    pub input_bytes: usize,
    pub journal_bytes: usize,
    /// User cycles (`SessionInfo::cycles`).
    pub cycles: u64,
    pub segments: usize,
    /// The sum of the segments' sizes, `Σ 2^po2`: the cycles a prover pays for, padding included.
    pub padded_cycles: u64,
    /// `Halted(code)`; non-zero is a refusal, which cannot be proven.
    pub exit_code: u32,
    /// The handler's output, if it succeeded.
    pub transition: Option<Transition>,
}

/// Run `elf` on `input` as LEZ runs a handler.
pub fn execute(what: &str, elf: &[u8], input: &ReceiveInput) -> Executed {
    let mut builder = ExecutorEnv::builder();
    lee::program::Program::write_receive_input(input, &mut builder).unwrap();
    let info = default_executor().execute(builder.build().unwrap(), elf).unwrap();
    let exit_code = match info.exit_code {
        ExitCode::Halted(code) => code,
        other => panic!("{what}: unexpected exit {other:?}"),
    };
    let transition = (exit_code == 0).then(|| {
        from_frame(&info.journal.bytes).and_then(|payload| borsh::from_slice(payload).ok()).expect("a journal")
    });
    Executed {
        what: what.to_string(),
        input_bytes: borsh::to_vec(input).unwrap().len(),
        journal_bytes: info.journal.bytes.len(),
        cycles: info.cycles(),
        segments: info.segments.len(),
        padded_cycles: info.segments.iter().map(|s| 1u64 << s.po2).sum(),
        exit_code,
        transition,
    }
}

impl Stage4 {
    /// `AttestPrivately` for `log` in `pool`.
    pub fn private_message(&self, pool: &Pool, slot: u64, log: &Transfer) -> ConsumerMessage {
        ConsumerMessage::AttestPrivately {
            pool: pool.clone(),
            slot,
            tx_index: log.tx_index,
            log_index: log.log_index,
            proof: self.fx.receipt_proof(log.tx_index),
            header: None,
        }
    }

    /// The consumer's two private transitions for `message`, in the executor alone: the root call
    /// at `account`'s actor (which sends `Read`), then the record's reply, in which the fact is
    /// verified. The second runs only if the first succeeds. The reply is the `StateReply` the
    /// program gives for the record the chain holds at `slot` (an empty one if there is none).
    pub fn private_transitions(&self, account: AccountId, slot: u64, message: &ConsumerMessage) -> Vec<Executed> {
        self.private_transitions_of(&consumer_program(), account, slot, message)
    }

    /// [`Stage4::private_transitions`] for another build of the consumer.
    pub fn private_transitions_of(
        &self,
        elf: &lee::program::Program,
        account: AccountId,
        slot: u64,
        message: &ConsumerMessage,
    ) -> Vec<Executed> {
        let actor = Actor::new(account, consumer_account());
        let call = ReceiveInput {
            receiver: actor,
            from: None,
            is_authorized: true,
            pre_state: Default::default(),
            message: borsh::to_vec(message).unwrap(),
        };
        let first = execute("AttestPrivately (sends Read)", elf.elf(), &call);
        let Some(transition) = &first.transition else { return vec![first] };
        let header = self.header(slot);
        let reply = ReceiveInput {
            receiver: actor,
            from: Some(header),
            is_authorized: true,
            pre_state: transition.response.post_state.clone().expect("the query is kept for the reply"),
            message: borsh::to_vec(&StateReply { state: self.w.chain.actor_state(header) }).unwrap(),
        };
        let second = execute("the record's reply (verifies the fact)", elf.elf(), &reply);
        vec![first, second]
    }
}

/// A markdown table of handler executions in the executor.
pub fn render_executed(rows: &[Executed]) -> String {
    let mut out = String::from(
        "| handler execution | input bytes (ReceiveInput) | journal bytes | user cycles | segments | cycles with padding (sum of 2^po2) | exit |\n\
         |---|---:|---:|---:|---:|---:|---:|\n",
    );
    for r in rows {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            r.what,
            crate::grouped(r.input_bytes as u64),
            crate::grouped(r.journal_bytes as u64),
            crate::grouped(r.cycles),
            r.segments,
            crate::grouped(r.padded_cycles),
            r.exit_code
        ));
    }
    out
}
