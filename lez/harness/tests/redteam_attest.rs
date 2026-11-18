//! Red team, phase 2 (`vectors/redteam-2026-11-18`): the blind vectors of the areas `attest`
//! (AT-*, 63) and `consumer` (CON-*, 15), each run through the attestation program and the
//! reference consumer on LEZ v0.4's state machine (in-process harness, dev mode) and judged by
//! `redteam_common` against the expectation the specification implies.
//!
//! Baselines. B-AT-MAIN is [`Stage4`] (fixture `mainnet-attest-2026-11-10`, the record of block
//! 26,384,851) and, where a vector needs data that fixture lacks (a hop to the parent block, the
//! EIP-2935 window's edge, USDC's storage, a slot unchanged across a hop), an instance of the same
//! program initialised from `mainnet-e2e-2026-11-09` ([`E2e`], the record of block 26,379,842, the
//! fixture `docs/design/attestation.md` §5.4 describes). B-AT-GLOAS is [`Stage5`]. B-AT-ACC is
//! FerrataVault v5 on Sepolia ([`RealVault`]) and, for two roots, the vault's tree vectors in
//! synthetic states. B-CON is the reference consumer's pool over [`Stage4`], B-CON-VAULT its
//! `VaultPool` over [`RealVault`]. Each vector's code is marked with its id; every observation's
//! note says where the construction departs from the vector's text, and why.
//!
//! Each vector starts from a copy of its fixture's world, so no vector sees another's state.
//! The decisive transaction is always sent through LEZ's settlement; the native `evaluate` is run
//! beside it only for the note.

mod redteam_common;

use std::{fs, path::PathBuf};

use ferrata_core::{
    fork::{Fork, LcFork},
    merkle::ZERO,
    rlp::encode_u64,
    types::{BeaconBlockHeader, LightClientBootstrap, LightClientHeader, LightClientUpdate, SyncAggregate, SyncCommittee},
    witness::{ExecutionSummary, HeaderRecord},
    Root,
};
use ferrata_lez_wire::{
    attest::{
        evaluate, AccountQuery, Attestation, Cmp, Condition, Fact, Field, Hop, LeafQuery, LogQuery, Operand,
        Relation, Request, SlotQuery, Word,
    },
    consumer::{
        address_word, balance_actor, balance_seed, claim_actor, claim_seed, holder_actor, holder_seed, nullifier,
        secret_nullifier, ConsumerMessage, ConsumerState, Pool, VaultPool,
    },
    header_actor, header_seed, instance_actor, instance_id, successor_id, Message, RecordState, ATTESTED_SELECTOR,
};
use ferrata_spectests::execkit::{
    account_proof, account_rlp, enc, header_rlp, hexb, keccak, quantity, receipt_rlp, word, AccountProof,
    StorageProof, Trie,
};
use harness::{
    attest::{account_query, condition, consumer_state, fixture_file, log_statement_id, u256, Stage4},
    consumer_account, ferrata_account, mainnet_config,
    private::{catalog, PrivateRun},
    scenarios::World,
    sepolia::{self, SepoliaFacts, Stage5, SEPOLIA_CHAIN_ID},
    settle::LeeTransaction,
    vault::{self, RealVault},
    Chain, Instance, TxReport, TWIN,
};
use lee::{Account, AccountId, Actor, PublicAccountEvidence};
use lee_core::{account::ActorState, program::PdaSeed};
use redteam_common::*;
use serde_json::Value;
use sha2::{Digest, Sha256};

const FILE: &str = "redteam_attest";

/// 0.001 ETH, each FerrataVault v5 deposit's amount.
const VAULT_AMOUNT: u128 = 1_000_000_000_000_000;

/// The query a private consumer keeps (AT-PRV-01): what it means to answer from the record.
const QUERY: &[u8] = b"ferrata red team AT-PRV-01: WETH 0xc02aaa39 balanceOf[0xb4e16d01] >= 1 at the record of slot 15,624,480";

// ---------------------------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------------------------

fn occurs(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

/// `w + 1` as a 256-bit big-endian word.
fn plus_one(w: Word) -> Word {
    let mut out = w;
    for b in out.iter_mut().rev() {
        let (v, carry) = b.overflowing_add(1);
        *b = v;
        if !carry {
            break;
        }
    }
    out
}

fn rlp_index(i: u64) -> Vec<u8> {
    let (key, len) = encode_u64(i);
    key[..len].to_vec()
}

/// The receipts trie of an `eth_getBlockReceipts` result.
fn receipt_trie(receipts: &[Value]) -> Trie {
    Trie::new(receipts.iter().enumerate().map(|(i, r)| (rlp_index(i as u64), receipt_rlp(r))))
}

fn native_note(result: &Result<Attestation, u8>) -> String {
    match result {
        Ok(a) => format!("native evaluate: accepted, {} statements", a.statements.len()),
        Err(code) => format!("native evaluate: refused {}", code_name(u32::from(*code))),
    }
}

fn describe(report: &TxReport) -> String {
    match (report.exit_code, &report.charge.revert) {
        (Some(code), _) => format!("refused {}", code_name(code)),
        (None, Some(revert)) => format!("reverted by LEZ: {revert}"),
        (None, None) => "applied".to_string(),
    }
}

/// `Attest` at `actor` as the root of a transaction.
fn attest_root(
    chain: &mut Chain,
    label: &str,
    actor: Actor,
    request: &Request,
    evidence: Vec<PublicAccountEvidence>,
    time: u64,
) -> TxReport {
    let message = borsh::to_vec(&Message::Attest { request: request.clone() }).unwrap();
    chain.submit(label, actor, message, vec![actor], evidence, time)
}

/// The data of the attestation event of a root `Attest`.
fn event_data(report: &TxReport) -> Option<Vec<u8>> {
    report.events.iter().find(|e| e.event.selector == ATTESTED_SELECTOR).map(|e| e.event.data.clone())
}

/// The statement identifiers the event names.
fn event_ids(report: &TxReport) -> Result<Vec<Root>, String> {
    let data = event_data(report).ok_or_else(|| "no attestation event".to_string())?;
    if data.len() < 48 || (data.len() - 48) % 32 != 0 {
        return Err(format!("attestation event of {} bytes", data.len()));
    }
    Ok(data[48..].chunks(32).map(|c| c.try_into().unwrap()).collect())
}

/// The block number the event names.
fn event_block(report: &TxReport) -> Result<u64, String> {
    let data = event_data(report).ok_or_else(|| "no attestation event".to_string())?;
    if data.len() < 48 {
        return Err(format!("attestation event of {} bytes", data.len()));
    }
    Ok(u64::from_le_bytes(data[40..48].try_into().unwrap()))
}

/// The observation of an accepted transaction whose effects `check` verifies; a refusal as is.
fn with_effects(report: &TxReport, note: &str, check: impl FnOnce(&TxReport) -> Result<String, String>) -> Observed {
    match Observed::of_report(report, note) {
        Observed::Accepted { .. } => match check(report) {
            Ok(more) => Observed::Accepted { note: format!("{note}; {more} [{}]", report.label) },
            Err(why) => Observed::EffectMismatch { note: format!("{note}: {why} [{}]", report.label) },
        },
        other => other,
    }
}

/// A public actor of the consumer calls `Attest` at `header(request.instance_id, slot)` of
/// `program` (`CallAttest`) and keeps the reply: the reply the program gave, if applied.
fn call_attest(
    chain: &mut Chain,
    label: &str,
    program: AccountId,
    slot: u64,
    request: &Request,
    time: u64,
) -> (TxReport, Option<Attestation>) {
    let consumer = consumer_account();
    let seed = PdaSeed::new([0x44; 32]);
    let reader = Actor::new(AccountId::for_public_pda(&consumer, &seed), consumer);
    let header = header_actor(program, &request.instance_id, slot);
    let message = ConsumerMessage::CallAttest { ferrata: program, slot, request: request.clone() };
    let report = chain.submit(
        label,
        reader,
        borsh::to_vec(&message).unwrap(),
        vec![reader, header],
        vec![PublicAccountEvidence::Pda { program: consumer, seed }],
        time,
    );
    let reply = match consumer_state(chain, reader) {
        Some(ConsumerState::Reader(state)) if report.ok() => {
            state.attestation.and_then(|bytes| borsh::from_slice(&bytes).ok())
        }
        _ => None,
    };
    (report, reply)
}

/// The exit code a failed private proving names (`... non-zero code N ...`).
fn error_code(error: &str) -> Option<u32> {
    error.match_indices("code ").find_map(|(i, _)| {
        let digits: String = error[i + 5..].chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    })
}

/// The observation of a private transaction: settled (then `check`), or not provable with an exit
/// code (a transition refused: no receipt, nothing to submit), or refused by LEZ.
fn observe_private(
    result: Result<PrivateRun, String>,
    note: &str,
    check: impl FnOnce(&PrivateRun) -> Result<String, String>,
) -> Observed {
    match result {
        Ok(run) => match &run.charge.revert {
            Some(revert) => Observed::Lez { revert: revert.clone(), note: format!("{note} (settled, reverted)") },
            None => match check(&run) {
                Ok(more) => Observed::Accepted { note: format!("{note}; {more}") },
                Err(why) => Observed::EffectMismatch { note: format!("{note}: {why}") },
            },
        },
        Err(error) => match error_code(&error) {
            Some(code) => Observed::Code { code, note: format!("{note}; not provable: {error}") },
            None => Observed::Lez { revert: error, note: format!("{note}; not provable") },
        },
    }
}

fn tx_bytes(run: &PrivateRun) -> Vec<u8> {
    borsh::to_vec(&LeeTransaction::PrivacyPreserving(run.tx.clone())).unwrap()
}

/// The accumulator-leaf identifier as the specification writes it (`docs/design/attestation.md`
/// §5.2, with the widths of `vectors/acc-leaf.json` and `bridge/README.md`): `SHA-256("ferrata/
/// fact/acc-leaf/v1" ‖ chain id (32 bytes) ‖ address ‖ slot ‖ leaf index (8 bytes) ‖ leaf)`.
fn acc_leaf_spec_id(chain_id: u64, address: &[u8; 20], slot: &Word, leaf_index: u64, leaf: &Root) -> Root {
    Sha256::new()
        .chain_update(b"ferrata/fact/acc-leaf/v1")
        .chain_update(u256(u128::from(chain_id)))
        .chain_update(address)
        .chain_update(slot)
        .chain_update(leaf_index.to_be_bytes())
        .chain_update(leaf)
        .finalize()
        .into()
}

/// The formula above against the independent Python implementation's identifiers.
fn self_check_acc_leaf_formula() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vectors/acc-leaf.json");
    let v: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    for x in v["identifierVariants"].as_array().unwrap() {
        let address: [u8; 20] = hexb(x["address"].as_str().unwrap()).try_into().unwrap();
        let id = acc_leaf_spec_id(
            x["chainId"].as_u64().unwrap(),
            &address,
            &word(x["slot"].as_str().unwrap()),
            x["leafIndex"].as_u64().unwrap(),
            &word(x["leaf"].as_str().unwrap()),
        );
        assert_eq!(id, word(x["id"].as_str().unwrap()), "the spec formula against acc-leaf.json");
    }
}

/// A fixture builder whose world each vector starts from afresh.
trait HasWorld {
    fn world(&mut self) -> &mut World;
}
impl HasWorld for Stage4 {
    fn world(&mut self) -> &mut World {
        &mut self.w
    }
}
impl HasWorld for Stage5 {
    fn world(&mut self) -> &mut World {
        &mut self.w
    }
}
impl HasWorld for RealVault {
    fn world(&mut self) -> &mut World {
        &mut self.w
    }
}

/// The fixture of `cell` (built on first use), its world reset to the one it was built with.
fn fresh<T: HasWorld>(cell: &mut Option<(T, World)>, make: impl FnOnce() -> T) -> &mut T {
    let entry = cell.get_or_insert_with(|| {
        let mut t = make();
        let w = t.world().clone();
        (t, w)
    });
    *entry.0.world() = entry.1.clone();
    &mut entry.0
}

// ---------------------------------------------------------------------------------------------
// B-AT-MAIN (Stage 4)
// ---------------------------------------------------------------------------------------------

fn weth_storage(s: &Stage4, conditions: Vec<Condition>) -> Request {
    let weth = s.fx.weth_proof();
    Request { accounts: vec![account_query(&weth, vec![], vec![(s.fx.weth_slot, conditions)])], ..s.request() }
}

fn weth_account(s: &Stage4, conditions: Vec<Condition>) -> Request {
    let weth = s.fx.weth_proof();
    Request { accounts: vec![account_query(&weth, conditions, vec![])], ..s.request() }
}

/// `Attest` at the record of the finalized block, as a transaction's root.
fn main_attest(s: &mut Stage4, label: &str, request: &Request, note: &str) -> Observed {
    let slot = s.fx.finalized_slot;
    let native = native_note(&s.expected(slot, request));
    let report = s.attest(label, slot, request);
    Observed::of_report(&report, &format!("{note}; {native}"))
}

fn s4(cell: &mut Option<(Stage4, World)>) -> &mut Stage4 {
    fresh(cell, Stage4::new)
}

// ---------------------------------------------------------------------------------------------
// B-AT-MAIN, second fixture: `mainnet-e2e-2026-11-09` through the program
// ---------------------------------------------------------------------------------------------

const E2E: &str = "mainnet-e2e-2026-11-09";
const E2E_SUMS: &str = include_str!("../../../crates/ferrata-spectests/fixtures/mainnet-e2e-2026-11-09/SHA256SUMS");
const E2E_CHECKPOINT: &str = "dab6b45fd34b9da264b15ad3e85dad586f836f3b11f5922e4c116b53574aefd3";
const E2E_SLOT: u64 = 15_619_456;
/// The record's block `n`, its parent `k` (3 failed transactions), `m1 = n − 16`, `m2 = m1 − 8191`.
const E2E_N: u64 = 26_379_842;
const E2E_K: u64 = 26_379_841;
const E2E_M1: u64 = 26_379_826;
const E2E_M2: u64 = 26_371_635;
const WETH_SLOT: &str = "0xb374801ace2c02f5db0425ab5920a2b7ed1d5a00abbcd395fda7530ba1d666c0";
const USDC_SLOT: &str = "0x28c0947f463362aab09a2d7896ccc847fb7e52633a16b978fb2caf3db1a9b8f8";

fn e2e_file(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/ferrata-spectests/fixtures").join(E2E).join(name);
    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let want = E2E_SUMS
        .lines()
        .find(|l| l.ends_with(&format!("  {name}")))
        .unwrap_or_else(|| panic!("{name} is not in SHA256SUMS"))
        .split_whitespace()
        .next()
        .unwrap();
    assert_eq!(hex::encode(Sha256::digest(&bytes)), want, "{name} does not match SHA256SUMS");
    bytes
}

fn e2e_json(name: &str) -> Value {
    serde_json::from_slice::<Value>(&e2e_file(name)).unwrap()["result"].clone()
}

fn e2e_receipts(block: u64) -> Vec<Value> {
    e2e_json(&format!("receipts_{block}.json")).as_array().expect("receipts").clone()
}

/// `LightClientFinalityUpdate` (Electra layout) as the `LightClientUpdate` it stands for.
fn electra_finality_update(b: &[u8]) -> LightClientUpdate {
    let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as usize;
    let (att, fin) = (u32_at(0), u32_at(4));
    assert_eq!((att, b.len() > fin), (400, true), "LightClientFinalityUpdate offsets");
    LightClientUpdate {
        attested_header: LightClientHeader::decode(&b[att..fin], LcFork::Electra).unwrap(),
        next_sync_committee: SyncCommittee::empty(512),
        next_sync_committee_branch: vec![ZERO; LcFork::Electra.sync_committee_branch_len()],
        finalized_header: LightClientHeader::decode(&b[fin..], LcFork::Electra).unwrap(),
        finality_branch: b[8..232].chunks_exact(32).map(|c| c.try_into().unwrap()).collect(),
        sync_aggregate: SyncAggregate::decode(&b[232..392], 512).unwrap(),
        signature_slot: u64::from_le_bytes(b[392..400].try_into().unwrap()),
    }
}

/// A mainnet instance initialised from the e2e fixture's bootstrap that verified its finality
/// update through the public job: `header(id, 15,619,456)` holds the record of block `n`.
#[derive(Clone)]
struct E2e {
    w: World,
    time: u64,
}

impl E2e {
    fn load() -> Self {
        let bootstrap = LightClientBootstrap::decode(&e2e_file("bootstrap.ssz"), LcFork::Electra, 512).unwrap();
        let checkpoint = bootstrap.header.beacon.hash_tree_root();
        assert_eq!(hex::encode(checkpoint), E2E_CHECKPOINT);
        let update = electra_finality_update(&e2e_file("finality_update.ssz"));
        let mut w = World::at(ferrata_account(), mainnet_config(2), checkpoint, bootstrap, vec![update.clone()]);
        let run = w.apply(&update, "e2e finality update");
        assert_eq!(run.outcome.map(|o| o.advanced), Some(true), "the e2e update finalizes a new header");
        assert_eq!(update.finalized_header.beacon.slot, E2E_SLOT);
        let record = w.inst.record(&w.chain, E2E_SLOT).expect("the record").header().unwrap();
        let ExecutionSummary::Payload { block_number, .. } = record.execution else { panic!("a payload record") };
        assert_eq!(block_number, E2E_N);
        let time = w.inst.config.slot_start_ms(update.signature_slot) + 12_000;
        Self { w, time }
    }

    fn header(&self) -> Actor {
        header_actor(self.w.inst.program, &self.w.inst.id, E2E_SLOT)
    }

    fn request(&self) -> Request {
        Request { instance_id: self.w.inst.id, chain_id: 1, ..Request::default() }
    }

    fn proof(&self, name: &str) -> AccountProof {
        account_proof(&e2e_file(name))
    }

    /// A hop from block `from`: the history contract's proofs in `from`'s state for the slot of
    /// block `slot_of`, with the RLP header of block `header_of`.
    fn hop(&self, from: u64, slot_of: u64, header_of: u64) -> Hop {
        let history = self.proof(&format!("proof_history_{from}.json"));
        let key = u256(u128::from(slot_of % 8191));
        let slot = history.storage.iter().find(|s| s.key == key).expect("the history slot's proof");
        Hop {
            account_proof: history.proof.clone(),
            storage_proof: slot.proof.clone(),
            header: e2e_file(&format!("header_{header_of}.rlp")),
        }
    }

    fn expected(&self, request: &Request) -> Result<Attestation, u8> {
        let state = self.w.inst.record(&self.w.chain, E2E_SLOT).expect("the record");
        evaluate(&state.instance_id, state.chain_id, &state.header().unwrap(), request)
    }

    fn attest(&mut self, label: &str, request: &Request) -> TxReport {
        let header = self.header();
        attest_root(&mut self.w.chain, label, header, request, vec![], self.time)
    }

    fn observe(&mut self, label: &str, request: &Request, note: &str) -> Observed {
        let native = native_note(&self.expected(request));
        let report = self.attest(label, request);
        Observed::of_report(&report, &format!("{note}; {native}"))
    }

    /// The WETH balance slot at block `block` (its account and the slot, no conditions).
    fn weth_at(&self, block: u64) -> AccountQuery {
        account_query(&self.proof(&format!("proof_weth_{block}.json")), vec![], vec![(word(WETH_SLOT), vec![])])
    }
}

/// The e2e fixture's world, built on first use, a copy each time.
fn e2e(cell: &mut Option<E2e>) -> E2e {
    cell.get_or_insert_with(E2e::load).clone()
}

/// The first log of the first successful transaction with logs in the parent block `k`.
fn k_log() -> LogQuery {
    let receipts = e2e_receipts(E2E_K);
    let i = receipts
        .iter()
        .position(|r| quantity(&r["status"]) == 1 && !r["logs"].as_array().unwrap().is_empty())
        .expect("a successful transaction with a log") as u64;
    LogQuery { tx_index: i, log_index: 0, proof: receipt_trie(&receipts).proof(&rlp_index(i)), conditions: vec![] }
}

// ---------------------------------------------------------------------------------------------
// B-AT-GLOAS (Stage 5)
// ---------------------------------------------------------------------------------------------

/// The Sepolia attestation fixture's storage request (USDC's `totalSupply_`), with the header.
fn usdc_request(fx: &SepoliaFacts, instance: Root, chain_id: u64, header: Option<Vec<u8>>) -> Request {
    let usdc = fx.storage_proof();
    Request {
        instance_id: instance,
        chain_id,
        header,
        accounts: vec![account_query(&usdc, vec![], vec![(fx.slot, vec![condition(Field::Value, Cmp::Eq, fx.value)])])],
        ..Request::default()
    }
}

/// [`Stage5`] from a replayed world: the attestation fixture's finality update applied.
fn stage5_from(mut w: World) -> Stage5 {
    let fx = SepoliaFacts::load();
    let update = fx.update.clone();
    let run = w.apply(&update, "finality update (attestation fixture)");
    assert_eq!(run.outcome.map(|o| o.advanced), Some(true));
    let time = w.inst.config.slot_start_ms(update.signature_slot) + 12_000;
    Stage5 { w, fx, time }
}

fn gloas_attest(s5: &mut Stage5, label: &str, request: &Request, note: &str) -> Observed {
    let slot = s5.fx.finalized_slot;
    let native = native_note(&s5.expected(slot, request));
    let report = s5.attest(label, slot, request);
    Observed::of_report(&report, &format!("{note}; {native}"))
}

// ---------------------------------------------------------------------------------------------
// B-AT-ACC
// ---------------------------------------------------------------------------------------------

/// The vault's account and slot 0 (its deposit root, `= root`), then `leaves` against fact 1.
fn vault_leaf_request(v: &RealVault, leaves: Vec<LeafQuery>) -> Request {
    let slot0 = vault::slot_word(0);
    let proof0 = v.proof.storage.iter().find(|p| p.key == slot0).expect("slot 0").proof.clone();
    Request {
        accounts: vec![AccountQuery {
            address: v.proof.address,
            proof: v.proof.proof.clone(),
            conditions: vec![],
            slots: vec![SlotQuery { slot: slot0, proof: proof0, conditions: vec![condition(Field::Value, Cmp::Eq, v.root)] }],
        }],
        leaves,
        ..v.base()
    }
}

fn leaf_query(v: &RealVault, i: usize) -> LeafQuery {
    let (leaf_index, leaf, path) = v.leaf(i);
    LeafQuery { root: 1, leaf_index, leaf, path }
}

fn vault_attest(v: &mut RealVault, label: &str, request: &Request, note: &str) -> Observed {
    let native = native_note(&evaluate(&v.w.inst.id, SEPOLIA_CHAIN_ID, &v.record, request));
    let (actor, time) = (v.actor(), v.time);
    let report = attest_root(&mut v.w.chain, label, actor, request, vec![], time);
    Observed::of_report(&report, &format!("{note}; {native}"))
}

/// A block whose state holds the vault (`vault::synthetic`'s construction) with the root of
/// `leaves` in slot 0 and their count in slot 1; a Gloas record of it placed at `beacon_slot`.
/// Returns the record's actor and the request for leaf `i` against slot 0.
fn synthetic_snapshot(chain: &mut Chain, leaves: &[Root], i: usize, beacon_slot: u64) -> (Actor, Request) {
    let (root, path) = vault::tree(leaves, i);
    let count = u256(leaves.len() as u128);
    let storage = Trie::new([
        (keccak(&vault::slot_word(0)).to_vec(), enc::uint(&root)),
        (keccak(&vault::slot_word(1)).to_vec(), enc::uint(&count)),
    ]);
    let address = vault::vault_address();
    let code_hash = keccak(b"FerrataVault");
    let mut accounts = vec![(address, account_rlp(1, 0, storage.root(), code_hash))];
    for b in 0x40..0x68u8 {
        accounts.push(([b; 20], account_rlp(u64::from(b), 1_000 + u128::from(b), keccak(&[0x80]), keccak(b""))));
    }
    let state = Trie::new(accounts.iter().map(|(a, rlp)| (keccak(a).to_vec(), rlp.clone())));
    let served: Value = serde_json::from_slice(&sepolia::gloas_file("block.json")).unwrap();
    let mut block = served["result"].clone();
    block["stateRoot"] = Value::String(format!("0x{}", hex::encode(state.root())));
    let header = header_rlp(&block);
    let record = HeaderRecord {
        beacon: BeaconBlockHeader { slot: beacon_slot, ..Default::default() },
        execution_root: [0; 32],
        execution: ExecutionSummary::BlockHash(keccak(&header)),
    };
    let actor = vault::place_record(chain, vault::SYNTHETIC_INSTANCE, SEPOLIA_CHAIN_ID, &record);
    let slot0 = vault::slot_word(0);
    let request = Request {
        instance_id: vault::SYNTHETIC_INSTANCE,
        chain_id: SEPOLIA_CHAIN_ID,
        header: Some(header),
        accounts: vec![AccountQuery {
            address,
            proof: state.proof(&keccak(&address)),
            conditions: vec![],
            slots: vec![SlotQuery {
                slot: slot0,
                proof: storage.proof(&keccak(&slot0)),
                conditions: vec![condition(Field::Value, Cmp::Eq, root)],
            }],
        }],
        leaves: vec![LeafQuery { root: 1, leaf_index: i as u64, leaf: leaves[i], path }],
        ..Request::default()
    };
    (actor, request)
}

// =============================================================================================
// Binding
// =============================================================================================

#[test]
fn bind() {
    let mut r = Results::new(FILE, "bind");
    let mut c4: Option<(Stage4, World)> = None;
    let other_id = |s: &Stage4| instance_id(&mainnet_config(4), &s.fx.checkpoint);
    let other_note = "the id of another instance of the program (mainnet's configuration with 4 adoption parts, the same \
                      checkpoint, chain id 1)";

    // AT-BIND-01: request bound to another instance, chain id right.
    r.run("AT-BIND-01", || {
        let s = s4(&mut c4);
        let request = Request { instance_id: other_id(s), ..weth_storage(s, vec![]) };
        main_attest(s, "AT-BIND-01", &request, &format!("WETH storage request bound to {other_note}"))
    });

    // AT-BIND-02: the mainnet record asked as a Sepolia statement.
    r.run("AT-BIND-02", || {
        let s = s4(&mut c4);
        let request = Request { chain_id: SEPOLIA_CHAIN_ID, ..weth_storage(s, vec![]) };
        main_attest(s, "AT-BIND-02", &request, "WETH storage request, chain id 11155111, at the mainnet record")
    });

    // AT-BIND-04: wrong instance and wrong chain together.
    r.run("AT-BIND-04", || {
        let s = s4(&mut c4);
        let request = Request { instance_id: other_id(s), chain_id: SEPOLIA_CHAIN_ID, ..weth_storage(s, vec![]) };
        main_attest(s, "AT-BIND-04", &request, &format!("bound to {other_note} and chain id 11155111"))
    });

    // The Sepolia instance (B-LC-SEP) after period 1409's update, shared by AT-BIND-03 and -05.
    let mut replay: Option<World> = None;

    // AT-BIND-03: a Sepolia (Gloas) record asked as a mainnet statement, right header and proofs.
    r.run("AT-BIND-03", || {
        let w = replay.get_or_insert_with(|| sepolia::replay(2, sepolia::LAST_PERIOD)).clone();
        let mut s5 = stage5_from(w);
        let request = usdc_request(&s5.fx, s5.w.inst.id, 1, Some(s5.fx.header.clone()));
        gloas_attest(
            &mut s5,
            "AT-BIND-03",
            &request,
            "Sepolia Gloas record of block 12,105,936 (Stage 5), its RLP header and USDC's slot proof, chain id 1",
        )
    });

    // AT-BIND-05: a record written by a successor, the request bound to the root id.
    r.run("AT-BIND-05", || {
        let mut w = replay.get_or_insert_with(|| sepolia::replay(2, sepolia::LAST_PERIOD)).clone();
        let fx = SepoliaFacts::load();
        let parent = w.inst.clone();
        let root_id = parent.state(&w.chain).expect("the root instance").root_id;
        let store = parent.store(&w.chain);
        assert_eq!(store.fork, LcFork::Gloas, "the parent's store is in the Gloas layout");
        let checkpoint = store.finalized.hash_tree_root();
        let mut config = sepolia::sepolia_config(2);
        config.fork_epochs[Fork::Heze as usize] = 400_000;
        config.store_fork = 4;
        let id = successor_id(&root_id, &parent.id, &config, &checkpoint);
        let now = parent.config.slot_start_ms(fx.update.signature_slot) + 12_000;
        let succeed = w.chain.submit(
            "AT-BIND-05 setup: Succeed (Heze scheduled)",
            parent.actor(),
            borsh::to_vec(&Message::Succeed { config: config.clone(), checkpoint }).unwrap(),
            vec![parent.actor(), instance_actor(parent.program, &id)],
            vec![],
            now,
        );
        assert!(succeed.ok(), "Succeed: {}", describe(&succeed));
        let successor = Instance { program: parent.program, config: config.clone(), checkpoint, id };
        let mut sw = World {
            chain: w.chain,
            inst: successor,
            native: w.native.clone(),
            spec: config.spec(),
            bootstrap: w.bootstrap.clone(),
            updates: vec![],
            runs: vec![],
        };
        let update = fx.update.clone();
        let run = sw.apply(&update, "AT-BIND-05 setup: the successor takes the attestation fixture's finality update");
        assert_eq!(run.outcome.map(|o| o.advanced), Some(true));
        let slot = fx.finalized_slot;
        let record = sw.inst.record(&sw.chain, slot).expect("the successor's record");
        assert_eq!((record.instance_id, record.root_id), (id, root_id), "the successor's record carries the root id");
        let request = usdc_request(&fx, root_id, SEPOLIA_CHAIN_ID, Some(fx.header.clone()));
        let native = native_note(&evaluate(&root_id, record.chain_id, &record.header().unwrap(), &request));
        let header = header_actor(sw.inst.program, &id, slot);
        let report = attest_root(&mut sw.chain, "AT-BIND-05", header, &request, vec![], now);
        let note = format!(
            "root instance {} (Sepolia, replayed to period 1409), successor {} created by Succeed (Heze scheduled), which \
             wrote the record of slot {slot}; USDC storage request bound to the root id, sent to the successor's record; {native}",
            hex::encode(&root_id[..6]),
            hex::encode(&id[..6])
        );
        with_effects(&report, &note, |rep| {
            let data = event_data(rep).ok_or_else(|| "no attestation event".to_string())?;
            if data.len() >= 32 && data[..32] == root_id {
                Ok("the reply's instance id (event bytes 0..32) is the root id".into())
            } else {
                Err(format!("the reply names instance {} (root id {})", hex::encode(&data[..32.min(data.len())]), hex::encode(root_id)))
            }
        })
    });

    r.finish(&["attest"], &["AT-BIND-"]);
}

// =============================================================================================
// The record's execution header
// =============================================================================================

#[test]
fn hdr() {
    let mut r = Results::new(FILE, "hdr");
    let mut c5: Option<(Stage5, World)> = None;
    let storage = |s5: &Stage5, header: Option<Vec<u8>>| usdc_request(&s5.fx, s5.w.inst.id, SEPOLIA_CHAIN_ID, header);

    // AT-HDR-01: no block header for a Gloas record.
    r.run("AT-HDR-01", || {
        let s5 = fresh(&mut c5, Stage5::new);
        let request = storage(s5, None);
        gloas_attest(s5, "AT-HDR-01", &request, "USDC storage request without the header, Gloas record (Stage 5)")
    });

    // AT-HDR-02: another block's header.
    r.run("AT-HDR-02", || {
        let s5 = fresh(&mut c5, Stage5::new);
        let request = storage(s5, Some(sepolia::bound_header()));
        gloas_attest(
            s5,
            "AT-HDR-02",
            &request,
            "the record binds block 12,105,936; no fixture holds its parent's (12,105,935) header, so another real \
             block's header is sent: 12,103,422 (sepolia-gloas header.rlp)",
        )
    });

    // AT-HDR-03: the header truncated by one byte.
    r.run("AT-HDR-03", || {
        let s5 = fresh(&mut c5, Stage5::new);
        let mut header = s5.fx.header.clone();
        header.pop();
        let request = storage(s5, Some(header));
        gloas_attest(s5, "AT-HDR-03", &request, "the record block's RLP header without its last byte")
    });

    // AT-HDR-04: the header with one zero byte appended.
    r.run("AT-HDR-04", || {
        let s5 = fresh(&mut c5, Stage5::new);
        let header = [s5.fx.header.as_slice(), &[0u8]].concat();
        let request = storage(s5, Some(header));
        gloas_attest(s5, "AT-HDR-04", &request, "the record block's RLP header with 0x00 appended")
    });

    // AT-HDR-05: the header re-encoded with another state root (the fabricated-state attack).
    r.run("AT-HDR-05", || {
        let s5 = fresh(&mut c5, Stage5::new);
        let served: Value = serde_json::from_slice(&sepolia::attest_file("block.json")).unwrap();
        let mut block = served["result"].clone();
        assert_eq!(header_rlp(&block), s5.fx.header, "block.json rebuilds the record's header");
        let usdc = s5.fx.storage_proof();
        let slot = s5.fx.slot;
        let forged_value = plus_one(s5.fx.value);
        let storage_trie = Trie::new([(keccak(&slot).to_vec(), enc::uint(&forged_value))]);
        let balance = u128::from_be_bytes(usdc.balance[16..].try_into().unwrap());
        let mut accounts = vec![(usdc.address, account_rlp(usdc.nonce, balance, storage_trie.root(), usdc.code_hash))];
        for b in 0x40..0x68u8 {
            accounts.push(([b; 20], account_rlp(u64::from(b), 1_000 + u128::from(b), keccak(&[0x80]), keccak(b""))));
        }
        let state = Trie::new(accounts.iter().map(|(a, rlp)| (keccak(a).to_vec(), rlp.clone())));
        block["stateRoot"] = Value::String(format!("0x{}", hex::encode(state.root())));
        let forged_header = header_rlp(&block);
        let forged = AccountProof {
            address: usdc.address,
            nonce: usdc.nonce,
            balance: usdc.balance,
            storage_hash: storage_trie.root(),
            code_hash: usdc.code_hash,
            proof: state.proof(&keccak(&usdc.address)),
            storage: vec![StorageProof { key: slot, value: forged_value, proof: storage_trie.proof(&keccak(&slot)) }],
        };
        let request = Request {
            header: Some(forged_header),
            accounts: vec![account_query(&forged, vec![], vec![(slot, vec![condition(Field::Value, Cmp::Eq, forged_value)])])],
            ..s5.request()
        };
        gloas_attest(
            s5,
            "AT-HDR-05",
            &request,
            "the record block's header re-encoded with the state root of a state in which USDC's totalSupply_ is the \
             real value + 1 (a state built for the attack, with consistent proofs of that value), asking Eq that value",
        )
    });

    // AT-HDR-06: a payload (Fulu) record with a header in the request.
    let mut c4: Option<(Stage4, World)> = None;
    r.run("AT-HDR-06", || {
        let s = s4(&mut c4);
        let header = fixture_file(&format!("header_{}.rlp", s.fx.block));
        let request = Request { header: Some(header), ..weth_storage(s, vec![]) };
        main_attest(s, "AT-HDR-06", &request, "WETH storage request with the record block's real RLP header, payload record")
    });

    r.finish(&["attest"], &["AT-HDR-"]);
}

// =============================================================================================
// Accounts
// =============================================================================================

#[test]
fn acc() {
    let mut r = Results::new(FILE, "acc");
    let mut c4: Option<(Stage4, World)> = None;

    // AT-ACC-01: one byte of the second node of WETH's account proof flipped.
    r.run("AT-ACC-01", || {
        let s = s4(&mut c4);
        let mut request = weth_account(s, vec![]);
        let node = &mut request.accounts[0].proof[1];
        let at = node.len() / 2;
        node[at] ^= 0xff;
        main_attest(s, "AT-ACC-01", &request, &format!("WETH's account proof, node 1 byte {at} xor 0xff"))
    });

    // AT-ACC-02: WETH's account proof without its leaf.
    r.run("AT-ACC-02", || {
        let s = s4(&mut c4);
        let mut request = weth_account(s, vec![]);
        request.accounts[0].proof.pop();
        main_attest(s, "AT-ACC-02", &request, "WETH's account proof without its last (leaf) node")
    });

    // AT-ACC-03: a trailing node (the storage proof's first) appended.
    r.run("AT-ACC-03", || {
        let s = s4(&mut c4);
        let weth = s.fx.weth_proof();
        let extra = weth.storage.iter().find(|p| p.key == s.fx.weth_slot).unwrap().proof[0].clone();
        let mut request = weth_account(s, vec![]);
        request.accounts[0].proof.push(extra);
        main_attest(s, "AT-ACC-03", &request, "WETH's account proof with the balance-slot proof's first node appended")
    });

    // AT-ACC-04: USDC's address with WETH's account proof.
    r.run("AT-ACC-04", || {
        let s = s4(&mut c4);
        let usdc = s.fx.transfer.token;
        let mut request = weth_account(s, vec![]);
        request.accounts[0].address = usdc;
        main_attest(s, "AT-ACC-04", &request, &format!("address 0x{} (USDC) with WETH's account proof", hex::encode(usdc)))
    });

    // AT-ACC-05: WETH's proof truncated before the leaf, asking the absent reading.
    r.run("AT-ACC-05", || {
        let s = s4(&mut c4);
        let absent = vec![
            condition(Field::Nonce, Cmp::Eq, ZERO),
            condition(Field::Balance, Cmp::Eq, ZERO),
            condition(Field::StorageRoot, Cmp::Eq, ZERO),
            condition(Field::CodeHash, Cmp::Eq, ZERO),
        ];
        let mut request = weth_account(s, absent);
        request.accounts[0].proof.pop();
        main_attest(
            s,
            "AT-ACC-05",
            &request,
            "WETH's address, its account proof without the leaf, conditions nonce = balance = storage root = code hash = 0",
        )
    });

    // AT-ACC-06: an address without an account, proven absent.
    r.run("AT-ACC-06", || {
        let s = s4(&mut c4);
        let absent = s.fx.absent_proof();
        let request = Request { accounts: vec![account_query(&absent, vec![], vec![])], ..s.request() };
        let slot = s.fx.finalized_slot;
        let native = native_note(&s.expected(slot, &request));
        let report = s.attest("AT-ACC-06", slot, &request);
        let note = format!("address 0x{} with its exclusion proof (no conditions); {native}", hex::encode(absent.address));
        let address = absent.address;
        // The fact's fields, from the program's reply to a caller (CallAttest), the same request.
        let (call, reply) = s.call("AT-ACC-06 (reply via CallAttest)", slot, &request);
        with_effects(&report, &note, |_| {
            let reply = reply.ok_or_else(|| format!("no reply to CallAttest ({})", describe(&call)))?;
            let zero = Fact::Account { address, nonce: 0, balance: ZERO, storage_root: ZERO, code_hash: ZERO };
            match reply.statements.as_slice() {
                [st] if st.fact == zero => Ok("the reply's fact is Account with every field zero".into()),
                other => Err(format!("the reply's facts are {other:?}")),
            }
        })
    });

    // AT-ACC-07: a balance condition that does not hold.
    r.run("AT-ACC-07", || {
        let s = s4(&mut c4);
        let weth = s.fx.weth_proof();
        let request = weth_account(s, vec![condition(Field::Balance, Cmp::Ge, plus_one(weth.balance))]);
        main_attest(s, "AT-ACC-07", &request, "condition Balance >= WETH's real balance + 1")
    });

    // AT-ACC-08: a condition on a field accounts do not have.
    r.run("AT-ACC-08", || {
        let s = s4(&mut c4);
        let request = weth_account(s, vec![condition(Field::Topic(0), Cmp::Eq, ZERO)]);
        main_attest(s, "AT-ACC-08", &request, "condition Topic(0) = 0 on WETH's account")
    });

    // AT-ACC-09: an empty request.
    r.run("AT-ACC-09", || {
        let s = s4(&mut c4);
        let request = s.request();
        main_attest(s, "AT-ACC-09", &request, "no accounts, logs, leaves, hops or relations")
    });

    r.finish(&["attest"], &["AT-ACC-"]);
}

// =============================================================================================
// Storage
// =============================================================================================

#[test]
fn sto() {
    let mut r = Results::new(FILE, "sto");
    let mut c4: Option<(Stage4, World)> = None;

    // AT-STO-01: one byte of a node of the balance-slot proof flipped.
    r.run("AT-STO-01", || {
        let s = s4(&mut c4);
        let mut request = weth_storage(s, vec![]);
        let node = &mut request.accounts[0].slots[0].proof[1];
        let at = node.len() / 2;
        node[at] ^= 0xff;
        main_attest(s, "AT-STO-01", &request, &format!("WETH balance-slot proof, node 1 byte {at} xor 0xff"))
    });

    // AT-STO-02: another holder's balance slot asked with the proof of slot S.
    r.run("AT-STO-02", || {
        let s = s4(&mut c4);
        let manifest: Value = serde_json::from_slice(&fixture_file("fixture.json")).unwrap();
        let holder: [u8; 20] = hexb(manifest["holder"].as_str().unwrap()).try_into().unwrap();
        let mapping = (0u128..16)
            .find(|m| keccak(&[address_word(&holder), u256(*m)].concat()) == s.fx.weth_slot)
            .expect("WETH's balanceOf mapping slot");
        let other = s.fx.transfer.to;
        let other_slot = keccak(&[address_word(&other), u256(mapping)].concat());
        let mut request = weth_storage(s, vec![]);
        request.accounts[0].slots[0].slot = other_slot;
        main_attest(
            s,
            "AT-STO-02",
            &request,
            &format!(
                "slot keccak(0x{} ‖ {mapping}) (balanceOf of another holder) with the proof of the fixture holder's slot",
                hex::encode(other)
            ),
        )
    });

    // AT-STO-03: Value Eq (real value + 1).
    r.run("AT-STO-03", || {
        let s = s4(&mut c4);
        let value = s.fx.weth_proof().storage.iter().find(|p| p.key == s.fx.weth_slot).unwrap().value;
        let request = weth_storage(s, vec![condition(Field::Value, Cmp::Eq, plus_one(value))]);
        main_attest(s, "AT-STO-03", &request, "condition Value = real value + 1 on WETH's balance slot")
    });

    // AT-STO-04: WETH's account proof with USDC's slot proof (same slot key). The attestation
    // fixture has no USDC proof; the e2e fixture has one at its record's block.
    let mut ce: Option<E2e> = None;
    r.run("AT-STO-04", || {
        let mut e = e2e(&mut ce);
        let weth = e.proof(&format!("proof_weth_{E2E_N}.json"));
        let usdc = e.proof(&format!("proof_usdc_{E2E_N}.json"));
        let slot = word(USDC_SLOT);
        let usdc_slot = usdc.storage.iter().find(|p| p.key == slot).expect("USDC's balance slot").proof.clone();
        let request = Request {
            accounts: vec![AccountQuery {
                address: weth.address,
                proof: weth.proof.clone(),
                conditions: vec![],
                slots: vec![SlotQuery { slot, proof: usdc_slot, conditions: vec![] }],
            }],
            ..e.request()
        };
        e.observe(
            "AT-STO-04",
            &request,
            "B-AT-MAIN = the e2e fixture's record (block 26,379,842; the attestation fixture has no USDC proof): WETH's \
             account proof with USDC's proof of its balance slot 0x28c0…f8, asked as WETH's slot 0x28c0…f8",
        )
    });

    // AT-STO-05: Ne 0 on a non-zero slot.
    r.run("AT-STO-05", || {
        let s = s4(&mut c4);
        let request = weth_storage(s, vec![condition(Field::Value, Cmp::Ne, ZERO)]);
        main_attest(s, "AT-STO-05", &request, "condition Value != 0 on WETH's (non-zero) balance slot")
    });

    // AT-STO-06: Le at equality.
    r.run("AT-STO-06", || {
        let s = s4(&mut c4);
        let value = s.fx.weth_proof().storage.iter().find(|p| p.key == s.fx.weth_slot).unwrap().value;
        let request = weth_storage(s, vec![condition(Field::Value, Cmp::Le, value)]);
        main_attest(s, "AT-STO-06", &request, "condition Value <= real value")
    });

    // AT-STO-07: Lt at equality.
    r.run("AT-STO-07", || {
        let s = s4(&mut c4);
        let value = s.fx.weth_proof().storage.iter().find(|p| p.key == s.fx.weth_slot).unwrap().value;
        let request = weth_storage(s, vec![condition(Field::Value, Cmp::Lt, value)]);
        main_attest(s, "AT-STO-07", &request, "condition Value < real value")
    });

    // AT-STO-08: a never-written slot reads zero.
    r.run("AT-STO-08", || {
        let s = s4(&mut c4);
        let weth = s.fx.weth_proof();
        let request = Request {
            accounts: vec![account_query(&weth, vec![], vec![(s.fx.empty_slot, vec![condition(Field::Value, Cmp::Eq, ZERO)])])],
            ..s.request()
        };
        main_attest(s, "AT-STO-08", &request, "WETH slot 0xff with its exclusion proof, condition Value = 0")
    });

    r.finish(&["attest"], &["AT-STO-"]);
}

// =============================================================================================
// Logs
// =============================================================================================

#[test]
fn log() {
    let mut r = Results::new(FILE, "log");
    let mut c4: Option<(Stage4, World)> = None;

    // AT-LOG-01: a log index past the receipt's logs.
    r.run("AT-LOG-01", || {
        let s = s4(&mut c4);
        let t = s.fx.transfer.clone();
        let request = s.log_request(t.tx_index, t.logs_in_receipt, vec![]);
        main_attest(s, "AT-LOG-01", &request, &format!("tx {} has {} logs; log index {}", t.tx_index, t.logs_in_receipt, t.logs_in_receipt))
    });

    // AT-LOG-02: tx index = transaction count, with the last receipt's proof.
    r.run("AT-LOG-02", || {
        let s = s4(&mut c4);
        let n = s.fx.receipts.len() as u64;
        let request = Request {
            logs: vec![LogQuery { tx_index: n, log_index: 0, proof: s.fx.receipt_proof(n - 1), conditions: vec![] }],
            ..s.request()
        };
        main_attest(s, "AT-LOG-02", &request, &format!("tx index {n} (= the block's {n} transactions) with the proof of receipt {}", n - 1))
    });

    // AT-LOG-03: one byte of a receipt-proof node flipped.
    r.run("AT-LOG-03", || {
        let s = s4(&mut c4);
        let t = s.fx.transfer.clone();
        let mut request = s.log_request(t.tx_index, t.log_index, vec![]);
        let node = &mut request.logs[0].proof[1];
        let at = node.len() / 2;
        node[at] ^= 0xff;
        main_attest(s, "AT-LOG-03", &request, &format!("receipt proof of tx {}, node 1 byte {at} xor 0xff", t.tx_index))
    });

    // AT-LOG-04: a log of a failed transaction.
    r.run("AT-LOG-04", || {
        let s = s4(&mut c4);
        let failed = s.fx.failed_tx;
        let status = quantity(&s.fx.receipts[failed as usize]["status"]);
        let request = s.log_request(failed, 0, vec![]);
        main_attest(
            s,
            "AT-LOG-04",
            &request,
            &format!("log 0 of tx {failed} of the record's own block (receipt status {status}); the vector's parenthesis names the e2e fixture's parent block, the attestation fixture's block holds a failed transaction itself"),
        )
    });

    // AT-LOG-05: Topic(2) = another address on an ERC-20 Transfer.
    r.run("AT-LOG-05", || {
        let s = s4(&mut c4);
        let t = s.fx.transfer.clone();
        let other = s.fx.other_recipient.to;
        let request = s.log_request(t.tx_index, t.log_index, vec![condition(Field::Topic(2), Cmp::Eq, address_word(&other))]);
        main_attest(s, "AT-LOG-05", &request, &format!("USDC Transfer to 0x{}, condition Topic(2) = 0x{}", hex::encode(t.to), hex::encode(other)))
    });

    // AT-LOG-06: Topic(5) on a three-topic log.
    r.run("AT-LOG-06", || {
        let s = s4(&mut c4);
        let t = s.fx.transfer.clone();
        let request = s.log_request(t.tx_index, t.log_index, vec![condition(Field::Topic(5), Cmp::Eq, ZERO)]);
        main_attest(s, "AT-LOG-06", &request, "condition Topic(5) = 0 on a three-topic Transfer")
    });

    // AT-LOG-07: DataWord(100) on a one-word log.
    r.run("AT-LOG-07", || {
        let s = s4(&mut c4);
        let t = s.fx.transfer.clone();
        let request = s.log_request(t.tx_index, t.log_index, vec![condition(Field::DataWord(100), Cmp::Eq, ZERO)]);
        main_attest(s, "AT-LOG-07", &request, "condition DataWord(100) = 0 on a Transfer with one data word")
    });

    // AT-LOG-08: Emitter = WETH on a USDC Transfer.
    r.run("AT-LOG-08", || {
        let s = s4(&mut c4);
        let t = s.fx.transfer.clone();
        let weth = s.fx.weth;
        let request = s.log_request(t.tx_index, t.log_index, vec![condition(Field::Emitter, Cmp::Eq, address_word(&weth))]);
        main_attest(s, "AT-LOG-08", &request, &format!("Transfer emitted by 0x{} (USDC), condition Emitter = WETH", hex::encode(t.token)))
    });

    // AT-LOG-09: the receipt proof of tx i presented as tx i + 1.
    r.run("AT-LOG-09", || {
        let s = s4(&mut c4);
        let t = s.fx.transfer.clone();
        let mut request = s.log_request(t.tx_index, t.log_index, vec![]);
        request.logs[0].tx_index = t.tx_index + 1;
        main_attest(s, "AT-LOG-09", &request, &format!("tx index {} with the proof of receipt {}", t.tx_index + 1, t.tx_index))
    });

    r.finish(&["attest"], &["AT-LOG-"]);
}

// =============================================================================================
// Relations
// =============================================================================================

#[test]
fn rel() {
    let mut r = Results::new(FILE, "rel");
    let mut c4: Option<(Stage4, World)> = None;

    // AT-REL-01: fact0.Balance > fact1.Value where it is false.
    r.run("AT-REL-01", || {
        let s = s4(&mut c4);
        let history = account_proof(&fixture_file(&format!("proof_history_{}.json", s.fx.block)));
        let held = history.storage.iter().find(|p| p.key == s.fx.history_slot).unwrap().value;
        let request = Request {
            accounts: vec![account_query(&history, vec![], vec![(s.fx.history_slot, vec![])])],
            relations: vec![Relation {
                left: Operand { fact: 0, field: Field::Balance },
                cmp: Cmp::Gt,
                right: Operand { fact: 1, field: Field::Value },
            }],
            ..s.request()
        };
        main_attest(
            s,
            "AT-REL-01",
            &request,
            &format!(
                "fact 0 = the EIP-2935 history contract's account (balance 0x{}), fact 1 = its slot 0x658 (a block hash, 0x{}…); \
                 relation fact0.Balance > fact1.Value is false",
                hex::encode(history.balance),
                hex::encode(&held[..6])
            ),
        )
    });

    // AT-REL-02: a relation naming fact 9 of a two-fact request.
    r.run("AT-REL-02", || {
        let s = s4(&mut c4);
        let request = Request {
            relations: vec![Relation {
                left: Operand { fact: 9, field: Field::Value },
                cmp: Cmp::Eq,
                right: Operand { fact: 1, field: Field::Value },
            }],
            ..weth_storage(s, vec![])
        };
        main_attest(s, "AT-REL-02", &request, "WETH account and balance slot (2 facts), relation fact9.Value = fact1.Value")
    });

    // AT-REL-03: relations that hold (both readings of the vector, one transaction each).
    r.run("AT-REL-03", || {
        let s = s4(&mut c4);
        let weth = s.fx.weth_proof();
        let base = Request {
            accounts: vec![account_query(&weth, vec![], vec![(s.fx.weth_slot, vec![]), (s.fx.empty_slot, vec![])])],
            ..s.request()
        };
        let same = Request {
            relations: vec![Relation {
                left: Operand { fact: 1, field: Field::Value },
                cmp: Cmp::Eq,
                right: Operand { fact: 1, field: Field::Value },
            }],
            ..base.clone()
        };
        let across = Request {
            relations: vec![Relation {
                left: Operand { fact: 1, field: Field::Value },
                cmp: Cmp::Gt,
                right: Operand { fact: 2, field: Field::Value },
            }],
            ..base
        };
        let slot = s.fx.finalized_slot;
        let n1 = native_note(&s.expected(slot, &same));
        let n2 = native_note(&s.expected(slot, &across));
        let a = s.attest("AT-REL-03 same slot", slot, &same);
        let b = s.attest("AT-REL-03 across two slots", slot, &across);
        Observed::of_reports(
            &[a.clone(), b.clone()],
            &format!(
                "(a) fact1.Value = fact1.Value (the balance slot with itself): {} ({n1}); (b) fact1.Value > fact2.Value \
                 (balance slot > slot 0xff): {} ({n2})",
                describe(&a),
                describe(&b)
            ),
        )
    });

    r.finish(&["attest"], &["AT-REL-"]);
}

// =============================================================================================
// History hops (the e2e fixture: its hops reach the parent block and the window's edge)
// =============================================================================================

#[test]
fn hop() {
    let mut r = Results::new(FILE, "hop");
    let mut ce: Option<E2e> = None;
    let fixture_note = "B-AT-MAIN = the e2e fixture's record of block n = 26,379,842 (the attestation fixture has \
                        neither the parent block nor the window's edge)";

    // AT-HOP-01: a hop to the block one past the window (R − 8192).
    r.run("AT-HOP-01", || {
        let mut e = e2e(&mut ce);
        let first = e.hop(E2E_N, E2E_M1, E2E_M1);
        let past = e.hop(E2E_M1, E2E_M2, E2E_M2 - 1);
        let request = Request { hops: vec![first, past], accounts: vec![e.weth_at(E2E_M2)], ..e.request() };
        e.observe(
            "AT-HOP-01",
            &request,
            &format!(
                "{fixture_note}. No fixture has a header of n − 8192; the hop is taken from R = m1 = n − 16 (reached by a \
                 valid first hop) to block R − 8192 = 26,371,634 (its real header). The slot proof of (R − 8192) mod 8191 \
                 = 0x12c5 was not fetched; the hop carries the only history slot proof at m1, 0x12c6 (the slot of R − 8191)"
            ),
        )
    });

    // AT-HOP-02: a hop to the record's own block.
    r.run("AT-HOP-02", || {
        let mut e = e2e(&mut ce);
        let own = Hop { header: e2e_file(&format!("header_{E2E_N}.rlp")), ..e.hop(E2E_N, E2E_K, E2E_K) };
        let request = Request { hops: vec![own], accounts: vec![e.weth_at(E2E_N)], ..e.request() };
        e.observe(
            "AT-HOP-02",
            &request,
            &format!("{fixture_note}: hop with block n's own header and the history proofs of the parent's slot, then WETH's slot at n"),
        )
    });

    // AT-HOP-03: a hop to the parent with one byte of its header's extra data changed.
    r.run("AT-HOP-03", || {
        let mut e = e2e(&mut ce);
        let mut block = e2e_json(&format!("block_{E2E_K}.json"));
        assert_eq!(header_rlp(&block), e2e_file(&format!("header_{E2E_K}.rlp")), "block.json rebuilds k's header");
        let mut extra = hexb(block["extraData"].as_str().unwrap());
        assert!(!extra.is_empty(), "the parent's extra data is not empty");
        extra[0] ^= 0x01;
        block["extraData"] = Value::String(format!("0x{}", hex::encode(&extra)));
        let tampered = header_rlp(&block);
        let hop = Hop { header: tampered, ..e.hop(E2E_N, E2E_K, E2E_K) };
        let request = Request { hops: vec![hop], logs: vec![k_log()], ..e.request() };
        e.observe(
            "AT-HOP-03",
            &request,
            &format!("{fixture_note}: hop to the parent k = n − 1 whose header's extra data has its first byte xor 0x01 (same length), then a log of k"),
        )
    });

    // AT-HOP-04: a hop through WETH's account instead of the history contract's.
    r.run("AT-HOP-04", || {
        let mut e = e2e(&mut ce);
        let weth = e.proof(&format!("proof_weth_{E2E_N}.json"));
        let hop = Hop { account_proof: weth.proof.clone(), ..e.hop(E2E_N, E2E_K, E2E_K) };
        let request = Request { hops: vec![hop], logs: vec![k_log()], ..e.request() };
        e.observe(
            "AT-HOP-04",
            &request,
            &format!("{fixture_note}: hop to k with WETH's account proof at n in place of the history contract's (history slot proof and k's header unchanged)"),
        )
    });

    // AT-HOP-05: one hop to the parent block, then a fact there.
    r.run("AT-HOP-05", || {
        let mut e = e2e(&mut ce);
        let hop = e.hop(E2E_N, E2E_K, E2E_K);
        let request = Request { hops: vec![hop], logs: vec![k_log()], ..e.request() };
        let native = native_note(&e.expected(&request));
        let report = e.attest("AT-HOP-05", &request);
        let (program, time) = (e.w.inst.program, e.time);
        let (call, reply) = call_attest(&mut e.w.chain, "AT-HOP-05 (reply via CallAttest)", program, E2E_SLOT, &request, time);
        let note = format!(
            "{fixture_note}: hop to k = n − 1 with the history contract's proofs; no fixture has a storage proof at k, so the \
             fact there is a log of k (receipts of k); {native}"
        );
        with_effects(&report, &note, |rep| {
            let block = event_block(rep)?;
            let reply = reply.ok_or_else(|| format!("no reply to CallAttest ({})", describe(&call)))?;
            if (block, reply.block_number, reply.hops) == (E2E_K, E2E_K, 1) {
                Ok(format!("event and reply name block {E2E_K} = R − 1, reply.hops = 1"))
            } else {
                Err(format!("event block {block}, reply block {} hops {}", reply.block_number, reply.hops))
            }
        })
    });

    // AT-HOP-06: a valid hop to R − 1, then a storage proof taken at R.
    r.run("AT-HOP-06", || {
        let mut e = e2e(&mut ce);
        let hop = e.hop(E2E_N, E2E_K, E2E_K);
        let request = Request { hops: vec![hop], accounts: vec![e.weth_at(E2E_N)], ..e.request() };
        e.observe(
            "AT-HOP-06",
            &request,
            &format!("{fixture_note}: valid hop to k = n − 1, then WETH's account and balance-slot proofs taken at n"),
        )
    });

    r.finish(&["attest"], &["AT-HOP-"]);
}

// =============================================================================================
// Accumulator leaves (FerrataVault v5 on Sepolia, the record the instance wrote)
// =============================================================================================

#[test]
fn leaf() {
    let mut r = Results::new(FILE, "leaf");
    let mut cv: Option<(RealVault, World)> = None;
    let base_note = "FerrataVault v5's account and slot 0 (= its deposit root) under the Gloas record of block 12,112,773, \
                     then the leaf against fact 1";

    // AT-LEAF-01: a leaf whose root names no storage fact.
    r.run("AT-LEAF-01", || {
        let v = fresh(&mut cv, RealVault::load);
        let request = vault_leaf_request(v, vec![LeafQuery { root: 5, ..leaf_query(v, 1) }]);
        vault_attest(v, "AT-LEAF-01", &request, &format!("{base_note}: leaf 1 with LeafQuery.root = 5 (two facts)"))
    });

    // AT-LEAF-02: leaf index 2^32 + i.
    r.run("AT-LEAF-02", || {
        let v = fresh(&mut cv, RealVault::load);
        let request = vault_leaf_request(v, vec![LeafQuery { leaf_index: (1u64 << 32) + 1, ..leaf_query(v, 1) }]);
        vault_attest(v, "AT-LEAF-02", &request, &format!("{base_note}: leaf 1 and its path at index 2^32 + 1"))
    });

    // AT-LEAF-03: the empty leaf at an unused index.
    r.run("AT-LEAF-03", || {
        let v = fresh(&mut cv, RealVault::load);
        let unused = v.leaves.len();
        let path = vault::tree(&v.leaves, unused).1;
        let request =
            vault_leaf_request(v, vec![LeafQuery { root: 1, leaf_index: unused as u64, leaf: [0; 32], path }]);
        vault_attest(
            v,
            "AT-LEAF-03",
            &request,
            &format!("{base_note}: the zero leaf at unused index {unused} with that position's path in the tree (its sibling subtree empty); no count condition"),
        )
    });

    // AT-LEAF-04: a path of 31 siblings.
    r.run("AT-LEAF-04", || {
        let v = fresh(&mut cv, RealVault::load);
        let mut q = leaf_query(v, 1);
        q.path.pop();
        let request = vault_leaf_request(v, vec![q]);
        vault_attest(v, "AT-LEAF-04", &request, &format!("{base_note}: leaf 1 with its path's last sibling dropped (31)"))
    });

    // AT-LEAF-05: one bit of sibling 0 flipped.
    r.run("AT-LEAF-05", || {
        let v = fresh(&mut cv, RealVault::load);
        let mut q = leaf_query(v, 1);
        q.path[0][31] ^= 0x01;
        let request = vault_leaf_request(v, vec![q]);
        vault_attest(v, "AT-LEAF-05", &request, &format!("{base_note}: leaf 1, the last bit of sibling 0 flipped"))
    });

    // AT-LEAF-06: leaf index i ^ 1 with leaf i's path.
    r.run("AT-LEAF-06", || {
        let v = fresh(&mut cv, RealVault::load);
        let request = vault_leaf_request(v, vec![LeafQuery { leaf_index: 0, ..leaf_query(v, 1) }]);
        vault_attest(v, "AT-LEAF-06", &request, &format!("{base_note}: leaf 1 and its path at index 1 ^ 1 = 0"))
    });

    // AT-LEAF-07: a real deposit leaf under the attested root.
    r.run("AT-LEAF-07", || {
        self_check_acc_leaf_formula();
        let v = fresh(&mut cv, RealVault::load);
        let q = leaf_query(v, 1);
        let request = vault_leaf_request(v, vec![q.clone()]);
        let native = native_note(&evaluate(&v.w.inst.id, SEPOLIA_CHAIN_ID, &v.record, &request));
        let (actor, time, program, slot) = (v.actor(), v.time, v.w.inst.program, v.slot);
        let report = attest_root(&mut v.w.chain, "AT-LEAF-07", actor, &request, vec![], time);
        let (call, reply) = call_attest(&mut v.w.chain, "AT-LEAF-07 (reply via CallAttest)", program, slot, &request, time);
        let (address, root) = (v.proof.address, v.root);
        let spec = acc_leaf_spec_id(SEPOLIA_CHAIN_ID, &address, &vault::slot_word(0), q.leaf_index, &q.leaf);
        with_effects(&report, &format!("{base_note}: deposit leaf 1 with its path; {native}"), |rep| {
            let ids = event_ids(rep)?;
            let reply = reply.ok_or_else(|| format!("no reply to CallAttest ({})", describe(&call)))?;
            let want = Fact::AccumulatorLeaf { address, slot: vault::slot_word(0), leaf_index: 1, leaf: q.leaf, root };
            let last = reply.statements.last().ok_or_else(|| "an empty reply".to_string())?;
            if last.fact != want {
                return Err(format!("the reply's last fact is {:?}", last.fact));
            }
            if ids.last() != Some(&spec) || last.id != spec {
                return Err(format!(
                    "identifier: event {:?}, reply {}, spec formula {}",
                    ids.last().map(hex::encode),
                    hex::encode(last.id),
                    hex::encode(spec)
                ));
            }
            Ok(format!("AccumulatorLeaf fact; its identifier {} equals the spec formula (checked against acc-leaf.json's independent ids)", hex::encode(&spec[..8])))
        })
    });

    r.finish(&["attest"], &["AT-LEAF-"]);
}

// =============================================================================================
// Identifiers
// =============================================================================================

#[test]
fn id() {
    let mut r = Results::new(FILE, "id");

    // AT-ID-01: a storage identifier does not depend on the block.
    let mut ce: Option<E2e> = None;
    r.run("AT-ID-01", || {
        let mut e = e2e(&mut ce);
        let at_r = Request { accounts: vec![e.weth_at(E2E_N)], ..e.request() };
        let back = Request { hops: vec![e.hop(E2E_N, E2E_M1, E2E_M1)], accounts: vec![e.weth_at(E2E_M1)], ..e.request() };
        let a = e.attest("AT-ID-01 at R", &at_r);
        let b = e.attest("AT-ID-01 one hop back", &back);
        let note = "e2e fixture: WETH's balance slot of the Uniswap v2 pair, the same value (0xee633e5bbb4f863523) at R = \
                    26,379,842 and one hop back at m1 = R − 16 (no fixture has a storage proof at R − 1)";
        match Observed::of_reports(&[a.clone(), b.clone()], note) {
            Observed::Accepted { .. } => match (event_ids(&a), event_ids(&b)) {
                (Ok(x), Ok(y)) if x.len() == 2 && y.len() == 2 && x[1] == y[1] => {
                    Observed::Accepted { note: format!("{note}; both slot identifiers are {}", hex::encode(&x[1][..8])) }
                }
                (x, y) => Observed::EffectMismatch { note: format!("{note}: identifiers {x:?} and {y:?}") },
            },
            other => other,
        }
    });

    // AT-ID-02: log identifiers equal the formula, two logs two identifiers.
    let mut c4: Option<(Stage4, World)> = None;
    r.run("AT-ID-02", || {
        let s = s4(&mut c4);
        let slot = s.fx.finalized_slot;
        let (x, y) = (s.fx.transfer.clone(), s.fx.other_recipient.clone());
        let a = s.attest("AT-ID-02 Transfer (tx 0, log 0)", slot, &s.log_request(x.tx_index, x.log_index, vec![]));
        let b = s.attest("AT-ID-02 Transfer (tx 0, log 4)", slot, &s.log_request(y.tx_index, y.log_index, vec![]));
        let (fx, fy) = (s.transfer_id(&x), s.transfer_id(&y));
        let note = "two USDC Transfer logs of the record's block; each identifier against SHA-256 over §5.2's fields (chain \
                    id, block, tx index, log position, log hash) derived from the JSON-RPC log by harness::attest::log_statement_id";
        match Observed::of_reports(&[a.clone(), b.clone()], note) {
            Observed::Accepted { .. } => match (event_ids(&a), event_ids(&b)) {
                (Ok(ia), Ok(ib)) if ia == [fx] && ib == [fy] && fx != fy => Observed::Accepted {
                    note: format!("{note}; ids {} and {} equal the formula and differ", hex::encode(&fx[..6]), hex::encode(&fy[..6])),
                },
                (ia, ib) => Observed::EffectMismatch {
                    note: format!("{note}: event ids {ia:?} / {ib:?}, formula {} / {}", hex::encode(fx), hex::encode(fy)),
                },
            },
            other => other,
        }
    });

    // AT-ID-03: the accumulator-leaf identifier is the same under two roots.
    r.run("AT-ID-03", || {
        let leaves = vault::synthetic().leaves;
        let i = 2usize;
        let mut chain = Chain::new();
        let (actor_a, request_a) = synthetic_snapshot(&mut chain, &leaves[..5], i, vault::SYNTHETIC_SLOT + 100);
        let (actor_b, request_b) = synthetic_snapshot(&mut chain, &leaves, i, vault::SYNTHETIC_SLOT + 101);
        let time = 1_800_000_000_000;
        let a = attest_root(&mut chain, "AT-ID-03 under the root of 5 leaves", actor_a, &request_a, vec![], time);
        let b = attest_root(&mut chain, "AT-ID-03 under the root of 8 leaves", actor_b, &request_b, vec![], time);
        let spec = acc_leaf_spec_id(SEPOLIA_CHAIN_ID, &vault::vault_address(), &vault::slot_word(0), i as u64, &leaves[i]);
        let note = "B-AT-ACC, synthetic (as vault::synthetic): two blocks whose vault slot 0 holds the root of the first 5 and \
                    of all 8 deposits of the vault's tree vectors, each under a Gloas record placed in LEZ's state; leaf 2 \
                    attested under each";
        match Observed::of_reports(&[a.clone(), b.clone()], note) {
            Observed::Accepted { .. } => match (event_ids(&a), event_ids(&b)) {
                (Ok(x), Ok(y)) if x.last().is_some() && x.last() == y.last() => Observed::Accepted {
                    note: format!(
                        "{note}; one identifier {} under both roots (spec formula: {})",
                        hex::encode(&x.last().unwrap()[..8]),
                        if x.last() == Some(&spec) { "equal" } else { "DIFFERENT" }
                    ),
                },
                (x, y) => Observed::EffectMismatch { note: format!("{note}: identifiers {x:?} and {y:?}") },
            },
            other => other,
        }
    });

    r.finish(&["attest"], &["AT-ID-"]);
}

// =============================================================================================
// Read-only, privacy
// =============================================================================================

#[test]
fn ro_prv() {
    let mut r = Results::new(FILE, "ro_prv");
    let mut c4: Option<(Stage4, World)> = None;

    // AT-RO-01: Attest changes no state.
    r.run("AT-RO-01", || {
        let s = s4(&mut c4);
        let slot = s.fx.finalized_slot;
        let header = s.header(slot);
        let before = s.w.chain.state.get_account_by_id(header.account_id).clone();
        let refused_req = weth_storage(s, vec![condition(Field::Value, Cmp::Lt, u256(1))]);
        let accepted_req = weth_storage(s, vec![]);
        let refused = s.attest("AT-RO-01 refused", slot, &refused_req);
        let accepted = s.attest("AT-RO-01 accepted", slot, &accepted_req);
        let after = s.w.chain.state.get_account_by_id(header.account_id).clone();
        let note = format!("first an Attest that is refused (balance < 1: {}), then an accepted one", describe(&refused));
        with_effects(&accepted, &note, |_| {
            if before == after && refused.exit_code.is_some() {
                Ok("the record's account (its state, nonce, everything) is identical before and after".into())
            } else if before != after {
                Err("the record's account changed".into())
            } else {
                Err(format!("the first Attest was not refused: {}", describe(&refused)))
            }
        })
    });

    // AT-RO-02: Attest as the transaction root emits the identifiers only.
    r.run("AT-RO-02", || {
        let s = s4(&mut c4);
        let request = weth_storage(s, vec![condition(Field::Value, Cmp::Ge, u256(1))]);
        let slot = s.fx.finalized_slot;
        let native = native_note(&s.expected(slot, &request));
        let report = s.attest("AT-RO-02", slot, &request);
        let mut needles: Vec<(String, Vec<u8>)> = Vec::new();
        for (i, node) in request.accounts[0].proof.iter().enumerate() {
            needles.push((format!("account proof node {i}"), node[..32.min(node.len())].to_vec()));
        }
        for (i, node) in request.accounts[0].slots[0].proof.iter().enumerate() {
            needles.push((format!("storage proof node {i}"), node[..32.min(node.len())].to_vec()));
        }
        needles.push(("the slot key".into(), s.fx.weth_slot.to_vec()));
        needles.push(("WETH's address".into(), s.fx.weth.to_vec()));
        with_effects(&report, &format!("WETH account and balance slot (k = 2 statements); {native}"), |rep| {
            let data = event_data(rep).ok_or_else(|| "no attestation event".to_string())?;
            if data.len() != 48 + 32 * 2 {
                return Err(format!("event data of {} bytes, not 48 + 32·2", data.len()));
            }
            for e in &rep.events {
                for (what, needle) in &needles {
                    if occurs(&e.event.data, needle) {
                        return Err(format!("{what} occurs in an event's data"));
                    }
                }
            }
            Ok(format!("event data {} bytes = 48 + 32·2; no proof node, slot key or address in any of the {} event(s)", data.len(), rep.events.len()))
        })
    });

    // AT-RO-03: Attest at an actor with no record.
    r.run("AT-RO-03", || {
        let s = s4(&mut c4);
        let slot = s.fx.finalized_slot + 1;
        let actor = header_actor(s.w.inst.program, &s.w.inst.id, slot);
        let evidence = vec![PublicAccountEvidence::Pda { program: s.w.inst.program, seed: header_seed(&s.w.inst.id, slot) }];
        let request = weth_storage(s, vec![]);
        let report = s.attest_at("AT-RO-03", actor, &request, evidence);
        Observed::of_report(&report, &format!("header(id, {slot}) (no record), admitted by its PDA evidence so that the program decides"))
    });

    // AT-RO-04: Attest at the instance actor.
    r.run("AT-RO-04", || {
        let s = s4(&mut c4);
        let actor = s.w.inst.actor();
        let request = weth_storage(s, vec![]);
        let report = s.attest_at("AT-RO-04", actor, &request, vec![]);
        Observed::of_report(&report, "instance(id) of the mainnet instance")
    });

    // AT-RO-05: a record stays valid after later updates. No mainnet fixture has a record with
    // execution proofs followed by two more updates, so the Sepolia lineage is used.
    r.run("AT-RO-05", || {
        let mut w = sepolia::replay(2, 1408);
        let fx = SepoliaFacts::load();
        let slot = fx.finalized_slot;
        let update = fx.update.clone();
        let run = w.apply(&update, "AT-RO-05 setup: the record of slot S");
        assert_eq!(run.outcome.map(|o| o.advanced), Some(true));
        let u1409 = sepolia::sepolia_update(1409);
        let a = w.apply(&u1409, "AT-RO-05 setup: period 1409's update (adopts period 1410's committee)");
        let finality = sepolia::gloas_finality_update(&vault::vault_file("finality_update.ssz"));
        let b = w.apply(&finality, "AT-RO-05 setup: a finality update of period 1410");
        assert!(a.reports.iter().chain(&b.reports).all(TxReport::ok), "the two later updates are applied");
        let finalized = w.inst.store(&w.chain).finalized.slot;
        assert!(finalized > slot);
        let record = w.inst.record(&w.chain, slot).expect("the record of slot S");
        let request = usdc_request(&fx, w.inst.id, SEPOLIA_CHAIN_ID, Some(fx.header.clone()));
        let native = native_note(&evaluate(&record.instance_id, record.chain_id, &record.header().unwrap(), &request));
        let header = header_actor(w.inst.program, &w.inst.id, slot);
        let time = w.inst.config.slot_start_ms(finality.signature_slot) + 12_000;
        let report = attest_root(&mut w.chain, "AT-RO-05", header, &request, vec![], time);
        Observed::of_report(
            &report,
            &format!(
                "B-LC-SEP in place of B-LC-MAIN (no mainnet record with proofs is followed by two updates in any fixture): \
                 the Sepolia instance replayed to period 1408 takes the attestation fixture's finality update (record of \
                 slot S = {slot}, block 12,105,936), then period 1409's update ({} jobs, committee only) and the vault \
                 fixture's finality update (finalized slot {finalized}); USDC's totalSupply_ attested under S afterwards; {native}",
                a.reports.len()
            ),
        )
    });

    // AT-PRV-01: a private read publishes no byte of the query.
    r.run("AT-PRV-01", || {
        let s = s4(&mut c4);
        let slot = s.fx.finalized_slot;
        let header = s.header(slot);
        let account = s.w.chain.add_private_account(0x71, 1_000_000_000_000);
        let read = ConsumerMessage::ReadRecord {
            ferrata: s.w.inst.program,
            instance_id: s.w.inst.id,
            slot,
            query: QUERY.to_vec(),
        };
        let result = s.w.chain.prove_and_settle_private(
            &account,
            Actor::new(account.id(), consumer_account()),
            borsh::to_vec(&read).unwrap(),
            vec![header],
            &catalog(),
            100_000_000,
            s.time,
        );
        observe_private(result, &format!("a private consumer reads header(id, {slot}) keeping a {}-byte query", QUERY.len()), |run| {
            let bytes = tx_bytes(run);
            let windows: Vec<&[u8]> = QUERY.windows(12).step_by(6).collect();
            match windows.iter().position(|w| occurs(&bytes, w)) {
                None if !occurs(&bytes, QUERY) => Ok(format!(
                    "neither the query nor any 12-byte window of it is in the {} transaction bytes",
                    bytes.len()
                )),
                found => Err(format!("query bytes found in the transaction (window {found:?})")),
            }
        })
    });

    // AT-PRV-02: a private call publishes its query (documented leak).
    r.run("AT-PRV-02", || {
        let s = s4(&mut c4);
        let slot = s.fx.finalized_slot;
        let header = s.header(slot);
        let request = weth_storage(s, vec![condition(Field::Value, Cmp::Ge, u256(1))]);
        let account = s.w.chain.add_private_account(0x72, 1_000_000_000_000);
        let call = ConsumerMessage::CallAttest { ferrata: s.w.inst.program, slot, request: request.clone() };
        let result = s.w.chain.prove_and_settle_private(
            &account,
            Actor::new(account.id(), consumer_account()),
            borsh::to_vec(&call).unwrap(),
            vec![header],
            &catalog(),
            100_000_000,
            s.time,
        );
        let query = borsh::to_vec(&request).unwrap();
        observe_private(result, "a private consumer calls Attest (CallAttest) with a WETH storage request", |run| {
            let bytes = tx_bytes(run);
            if occurs(&bytes, &query) {
                Ok(format!("the whole {}-byte request is in the {} transaction bytes", query.len(), bytes.len()))
            } else {
                Err("the request's bytes are not in the transaction".into())
            }
        })
    });

    r.finish(&["attest"], &["AT-RO-", "AT-PRV-"]);
}

// =============================================================================================
// The reference consumer
// =============================================================================================

/// `ClaimDeposit` of deposit `i` at `claim(pool, n)` (as `tests/vault_consumer.rs`).
#[allow(clippy::too_many_arguments)]
fn vault_claim(v: &mut RealVault, label: &str, pool: &VaultPool, i: usize, s: Root, kind: u8, dest: Root, n: Root) -> TxReport {
    let program = consumer_account();
    let pool_id = pool.id();
    let at = claim_actor(program, &pool_id, &n);
    let holder = holder_actor(program, &pool_id, &dest);
    let header = vault::header_of(pool, v.slot);
    let message = ConsumerMessage::ClaimDeposit { pool: pool.clone(), slot: v.slot, deposit: v.deposit_proof(i), s, kind, dest };
    let time = v.time;
    v.w.chain.submit(
        label,
        at,
        borsh::to_vec(&message).unwrap(),
        vec![at, header, holder],
        vec![PublicAccountEvidence::Pda { program, seed: claim_seed(&pool_id, &n) }],
        time,
    )
}

#[test]
fn consumer() {
    let mut r = Results::new(FILE, "consumer");
    let mut c4: Option<(Stage4, World)> = None;
    let program = consumer_account();

    // CON-01: the reference consumer claims a real Transfer once.
    r.run("CON-01", || {
        let s = s4(&mut c4);
        let (slot, t, pool) = (s.fx.finalized_slot, s.fx.transfer.clone(), s.pool());
        let id = s.transfer_id(&t);
        let report = s.claim("CON-01", &pool, slot, &t, &id);
        let credited = s.balance(&pool, &t.from).map(|b| (b.amount, b.credits));
        with_effects(&report, "Claim of the fixture's USDC Transfer (tx 0, log 0) at claim(pool, N)", |_| {
            if credited == Some((t.amount(), 1)) {
                Ok(format!("balance(pool, 0x{}) = {} (the amount), 1 credit", hex::encode(t.from), t.amount()))
            } else {
                Err(format!("balance(pool, sender) is {credited:?}, want ({}, 1)", t.amount()))
            }
        })
    });

    // CON-02: the same Transfer claimed twice.
    r.run("CON-02", || {
        let s = s4(&mut c4);
        let (slot, t, pool) = (s.fx.finalized_slot, s.fx.transfer.clone(), s.pool());
        let id = s.transfer_id(&t);
        let first = s.claim("CON-02 setup: CON-01's claim", &pool, slot, &t, &id);
        let again = s.claim("CON-02", &pool, slot, &t, &id);
        Observed::of_report(&again, &format!("after CON-01's claim ({}), the same Claim again", describe(&first)))
    });

    // CON-03: a Transfer of the same token to another recipient.
    r.run("CON-03", || {
        let s = s4(&mut c4);
        let (slot, pool, wrong) = (s.fx.finalized_slot, s.pool(), s.fx.other_recipient.clone());
        let id = s.transfer_id(&wrong);
        let report = s.claim("CON-03", &pool, slot, &wrong, &id);
        Observed::of_report(&report, &format!("USDC Transfer (tx 0, log 4) to 0x{}, not the pool's recipient", hex::encode(wrong.to)))
    });

    // CON-04: a pool bound to another instance, claim at the mainnet record's slot.
    r.run("CON-04", || {
        let s = s4(&mut c4);
        let (slot, t) = (s.fx.finalized_slot, s.fx.transfer.clone());
        let other = instance_id(&mainnet_config(4), &s.fx.checkpoint);
        let pool = Pool { instance_id: other, ..s.pool() };
        let id = s.transfer_id(&t);
        let pool_id = pool.id();
        let n = nullifier(&pool_id, &id);
        let claim = claim_actor(program, &pool_id, &n);
        let balance = balance_actor(program, &pool_id, &t.from);
        let header = header_actor(pool.ferrata, &other, slot);
        let message = ConsumerMessage::Claim {
            pool: pool.clone(),
            slot,
            tx_index: t.tx_index,
            log_index: t.log_index,
            proof: s.fx.receipt_proof(t.tx_index),
            header: None,
        };
        let evidence = vec![
            PublicAccountEvidence::Pda { program, seed: claim_seed(&pool_id, &n) },
            PublicAccountEvidence::Pda { program: pool.ferrata, seed: header_seed(&other, slot) },
        ];
        let time = s.time;
        let report = s.w.chain.submit("CON-04", claim, borsh::to_vec(&message).unwrap(), vec![claim, header, balance], evidence, time);
        Observed::of_report(
            &report,
            &format!(
                "Pool.instance_id = another instance's id (mainnet config, 4 adoption parts), Claim at slot {slot} (the mainnet \
                 record's). The consumer addresses the record from the pool, header(pool.instance_id, slot), so the call goes \
                 to header({}…, {slot}); the transaction lists that actor (admitted by PDA evidence) so the programs decide",
                hex::encode(&other[..6])
            ),
        )
    });

    // CON-05: Claim sent to claim(pool, N') for another N'.
    r.run("CON-05", || {
        let s = s4(&mut c4);
        let (slot, t, pool) = (s.fx.finalized_slot, s.fx.transfer.clone(), s.pool());
        let other_id = s.transfer_id(&s.fx.other_token.clone());
        let report = s.claim("CON-05", &pool, slot, &t, &other_id);
        Observed::of_report(&report, "the fixture's Transfer claimed at claim(pool, N') with N' the nullifier of another log's identifier (tx 0, log 1)")
    });

    // CON-13: Credit sent by an outside account.
    r.run("CON-13", || {
        let s = s4(&mut c4);
        let (t, pool) = (s.fx.transfer.clone(), s.pool());
        let pool_id = pool.id();
        let n = nullifier(&pool_id, &s.transfer_id(&t));
        let balance = balance_actor(program, &pool_id, &t.from);
        let credit = ConsumerMessage::Credit { pool_id, nullifier: n, owner: t.from, amount: t.amount() };
        let evidence = vec![PublicAccountEvidence::Pda { program, seed: balance_seed(&pool_id, &t.from) }];
        let time = s.time;
        let report = s.w.chain.submit("CON-13", balance, borsh::to_vec(&credit).unwrap(), vec![balance], evidence, time);
        Observed::of_report(&report, "a transaction signed by an ordinary key whose root message is Credit at balance(pool, owner)")
    });

    // CON-14: the claim's attestation routed to a copy of the program holding a forged record.
    r.run("CON-14", || {
        let s = s4(&mut c4);
        let (slot, pool) = (s.fx.finalized_slot, s.pool());
        let attacker = [0xee; 20];
        let mut receipt = s.fx.receipts[0].clone();
        receipt["logs"][0]["topics"][1] = Value::String(format!("0x{}", hex::encode(address_word(&attacker))));
        receipt["logs"][0]["data"] = Value::String(format!("0x{}", hex::encode(u256(10u128.pow(30)))));
        let trie = Trie::new([(rlp_index(0), receipt_rlp(&receipt))]);
        let mut forged = s.record(slot).header().unwrap();
        if let ExecutionSummary::Payload { receipts_root, .. } = &mut forged.execution {
            *receipts_root = trie.root();
        }
        let twin_header = header_actor(TWIN, &s.w.inst.id, slot);
        let state = RecordState { instance_id: s.w.inst.id, chain_id: 1, record: forged.encode(), root_id: s.w.inst.id };
        s.w.chain.state.force_insert_account(
            twin_header.account_id,
            Account::default().with_actor_state(TWIN, ActorState::from(borsh::to_vec(&state).unwrap())),
        );
        let forged_id = log_statement_id(1, s.fx.block, 0, 0, &receipt["logs"][0]);
        let pool_id = pool.id();
        let n = nullifier(&pool_id, &forged_id);
        let claim = claim_actor(program, &pool_id, &n);
        let balance = balance_actor(program, &pool_id, &attacker);
        let message = ConsumerMessage::Claim { pool, slot, tx_index: 0, log_index: 0, proof: trie.proof(&rlp_index(0)), header: None };
        let time = s.time;
        let report = s.w.chain.submit(
            "CON-14",
            claim,
            borsh::to_vec(&message).unwrap(),
            vec![claim, twin_header, balance],
            vec![PublicAccountEvidence::Pda { program, seed: claim_seed(&pool_id, &n) }],
            time,
        );
        Observed::of_report(
            &report,
            "the same image at a second program account (harness::TWIN) holds, at its header(id, slot), a forged record \
             (the real one with the receipts root of a receipt whose Transfer pays 10^30 to the pool's recipient from 0xee…ee); \
             Pool.ferrata = the real program; the Claim of the forged log lists the copy's record actor in place of the real one",
        )
    });

    // CON-15: a refused claim can be retried.
    r.run("CON-15", || {
        let s = s4(&mut c4);
        let (slot, t, pool) = (s.fx.finalized_slot, s.fx.transfer.clone(), s.pool());
        let id = s.transfer_id(&t);
        let pool_id = pool.id();
        let n = nullifier(&pool_id, &id);
        let claim = claim_actor(program, &pool_id, &n);
        let balance = balance_actor(program, &pool_id, &t.from);
        let header = s.header(slot);
        let mut proof = s.fx.receipt_proof(t.tx_index);
        let last = proof.len() - 1;
        let middle = proof[last].len() / 2;
        proof[last][middle] ^= 0x01;
        let bad = ConsumerMessage::Claim { pool: pool.clone(), slot, tx_index: t.tx_index, log_index: t.log_index, proof, header: None };
        let time = s.time;
        let first = s.w.chain.submit(
            "CON-15 corrupted proof",
            claim,
            borsh::to_vec(&bad).unwrap(),
            vec![claim, header, balance],
            vec![PublicAccountEvidence::Pda { program, seed: claim_seed(&pool_id, &n) }],
            time,
        );
        let retry = s.claim("CON-15 retry", &pool, slot, &t, &id);
        let credited = s.balance(&pool, &t.from).map(|b| (b.amount, b.credits));
        if first.ok() {
            return Observed::EffectMismatch { note: "the Claim with a corrupted receipt proof was accepted".into() };
        }
        with_effects(&retry, &format!("Claim with one bit of the receipt leaf flipped: {}; then the correct Claim", describe(&first)), |_| {
            if credited == Some((t.amount(), 1)) {
                Ok("credited once after the retry".into())
            } else {
                Err(format!("balance after the retry {credited:?}"))
            }
        })
    });

    // B-CON-VAULT: FerrataVault v5's real deposits.
    let mut cv: Option<(RealVault, World)> = None;

    // CON-06: a wrong opening (s changed in one byte).
    r.run("CON-06", || {
        let v = fresh(&mut cv, RealVault::load);
        let (p, open) = (vault::eth_pool(v), vault::crown_openings(v));
        let (mut s0, kind0, dest0) = open[0];
        s0[0] ^= 0x01;
        let n = secret_nullifier(&s0, &v.deposit_id(0));
        let report = vault_claim(v, "CON-06", &p, 0, s0, kind0, dest0, n);
        Observed::of_report(&report, "ClaimDeposit of public-kind deposit 0 with s's first byte xor 0x01, at claim(pool, H(s' ‖ id))")
    });

    // CON-07: public claim of a private-kind deposit, correct opening.
    r.run("CON-07", || {
        let v = fresh(&mut cv, RealVault::load);
        let (p, open) = (vault::eth_pool(v), vault::crown_openings(v));
        let (s1, kind1, dest1) = open[1];
        let n = secret_nullifier(&s1, &v.deposit_id(1));
        let report = vault_claim(v, "CON-07", &p, 1, s1, kind1, dest1, n);
        Observed::of_report(&report, &format!("ClaimDeposit of deposit 1 (kind 0x{kind1:02x}, private) with its correct opening"))
    });

    // CON-08: a kind byte that is no kind.
    r.run("CON-08", || {
        let v = fresh(&mut cv, RealVault::load);
        let (p, open) = (vault::eth_pool(v), vault::crown_openings(v));
        let (s0, _, dest0) = open[0];
        let n = secret_nullifier(&s0, &v.deposit_id(0));
        let report = vault_claim(v, "CON-08", &p, 0, s0, 0x03, dest0, n);
        Observed::of_report(&report, "ClaimDeposit of deposit 0 with kind = 0x03")
    });

    // CON-09: claimed publicly, then privately.
    r.run("CON-09", || {
        let v = fresh(&mut cv, RealVault::load);
        let (p, open) = (vault::eth_pool(v), vault::crown_openings(v));
        let (s0, kind0, dest0) = open[0];
        let n0 = secret_nullifier(&s0, &v.deposit_id(0));
        let public = vault_claim(v, "CON-09 setup: the public claim", &p, 0, s0, kind0, dest0, n0);
        if !public.ok() {
            return Observed::of_report(&public, "the public claim (setup) was refused");
        }
        let account = v.w.chain.add_private_account(0x9a, 1_000_000_000_000);
        let message =
            ConsumerMessage::ClaimDepositPrivately { pool: p.clone(), slot: v.slot, deposit: v.deposit_proof(0), s: s0, kind: kind0, dest: dest0 };
        let publics = vec![v.actor(), claim_actor(program, &p.id(), &n0), holder_actor(program, &p.id(), &dest0)];
        let time = v.time;
        let result = v.w.chain.prove_and_settle_private(
            &account,
            Actor::new(account.id(), program),
            borsh::to_vec(&message).unwrap(),
            publics,
            &catalog(),
            100_000_000,
            time,
        );
        observe_private(result, "public-kind deposit 0 claimed publicly (applied), then ClaimDepositPrivately of it", |_| Ok("settled".into()))
    });

    // CON-10: a private-kind deposit claimed privately by another account.
    r.run("CON-10", || {
        let v = fresh(&mut cv, RealVault::load);
        let (p, open) = (vault::eth_pool(v), vault::crown_openings(v));
        let (s1, kind1, dest1) = open[1];
        let n1 = secret_nullifier(&s1, &v.deposit_id(1));
        let account = v.w.chain.add_private_account(0x9b, 1_000_000_000_000);
        assert_ne!(account.id().value(), &dest1);
        let message =
            ConsumerMessage::ClaimDepositPrivately { pool: p.clone(), slot: v.slot, deposit: v.deposit_proof(1), s: s1, kind: kind1, dest: dest1 };
        let publics = vec![v.actor(), claim_actor(program, &p.id(), &n1)];
        let time = v.time;
        let result = v.w.chain.prove_and_settle_private(
            &account,
            Actor::new(account.id(), program),
            borsh::to_vec(&message).unwrap(),
            publics,
            &catalog(),
            100_000_000,
            time,
        );
        observe_private(result, "ClaimDepositPrivately of private-kind deposit 1 (correct opening) run by a private account that is not its destination", |_| Ok("settled".into()))
    });

    // CON-11: Spend sent by an outside account.
    r.run("CON-11", || {
        let v = fresh(&mut cv, RealVault::load);
        let (p, open) = (vault::eth_pool(v), vault::crown_openings(v));
        let (s0, _, dest0) = open[0];
        let n0 = secret_nullifier(&s0, &v.deposit_id(0));
        let pool_id = p.id();
        let at = claim_actor(program, &pool_id, &n0);
        let spend = ConsumerMessage::Spend { pool_id, nullifier: n0, credit: Some((dest0, VAULT_AMOUNT)) };
        let time = v.time;
        let report = v.w.chain.submit(
            "CON-11",
            at,
            borsh::to_vec(&spend).unwrap(),
            vec![at, holder_actor(program, &pool_id, &dest0)],
            vec![PublicAccountEvidence::Pda { program, seed: claim_seed(&pool_id, &n0) }],
            time,
        );
        Observed::of_report(&report, "a transaction signed by an ordinary key whose root message is Spend{pool, N0, credit} at claim(pool, N0)")
    });

    // CON-12: CreditHolder sent by an outside account.
    r.run("CON-12", || {
        let v = fresh(&mut cv, RealVault::load);
        let (p, open) = (vault::eth_pool(v), vault::crown_openings(v));
        let (s0, _, dest0) = open[0];
        let n0 = secret_nullifier(&s0, &v.deposit_id(0));
        let pool_id = p.id();
        let holder = holder_actor(program, &pool_id, &dest0);
        let message = ConsumerMessage::CreditHolder { pool_id, nullifier: n0, dest: dest0, amount: VAULT_AMOUNT };
        let time = v.time;
        let report = v.w.chain.submit(
            "CON-12",
            holder,
            borsh::to_vec(&message).unwrap(),
            vec![holder],
            vec![PublicAccountEvidence::Pda { program, seed: holder_seed(&pool_id, &dest0) }],
            time,
        );
        Observed::of_report(&report, "a transaction signed by an ordinary key whose root message is CreditHolder at holder(pool, dest)")
    });

    r.finish(&["consumer"], &["CON-"]);
}
