//! Accumulator leaves through the program, and Ferrata's vault on Sepolia.
//!
//! The vault (`0xdfedf9A73Cfb337277b95a9a1C941c3496A4385F` on Sepolia) keeps the root of its
//! SHA-256 deposit tree in storage slot 0 and the number of leaves in slot 1 (`vault/README.md`,
//! "Storage-slot contract"). A deposit's leaf is `sha256(0x00 ‖ LEAF_TAG ‖ leafIndex:u64 ‖
//! depositor ‖ token ‖ amount:u256 ‖ C)` with `LEAF_TAG = sha256("ferrata/vault/leaf/v2")`, and the
//! tree is the core's accumulator (`ferrata_core::accumulator`). [`vault_request`] asks in one
//! request what a bridge program needs before it mints against a deposit: the vault's account,
//! slots 0 and 1, and deposit leaves proven against the root in slot 0.
//!
//! [`synthetic`]: the same request on a synthetic state, until the vault is deployed. The vault's
//! eight deposits of `vectors/vault-tree-v2.json` (made on the Solidity vault by its own tests)
//! sit in a block whose state holds an account at the vault's address with their root in slot 0
//! and their count in slot 1; its header is a real Sepolia header with that state root put in, and
//! a record in the Gloas layout binds its hash. The record is placed in LEZ's state at the
//! program's record address with `V03State::force_insert_account` (LEZ's test utilities): it is
//! not one the program wrote. From there the path is the program's own, in the guest.
//!
//! [`real`]: the fixture of `lez/scripts/fetch-attest-fixture.py --network sepolia --storage
//! VAULT:0=depositRoot(),1=nextLeafIndex() --deposits VAULT:FROM --updates-from 1410`, once the
//! vault exists: the Sepolia instance follows the chain to the fixture's record, and the request
//! goes to it.

use std::{
    fs,
    path::{Path, PathBuf},
};

use ferrata_core::{
    accumulator::{accumulator_leaf_id, accumulator_node},
    fork::LcFork,
    types::{BeaconBlockHeader, LightClientUpdate},
    witness::{ExecutionSummary, HeaderRecord},
    Root,
};
use ferrata_lez_wire::{
    attest::{evaluate, AccountQuery, Address, Attestation, Cmp, Fact, Field, LeafQuery, Request, SlotQuery, Word},
    header_actor, Message, RecordState, ATTESTED_SELECTOR,
};
use ferrata_spectests::execkit::{
    account_proof, account_rlp, enc, header_rlp, hexb, keccak, word, AccountProof, StorageProof, Trie,
};
use lee::{Account, Actor};
use lee_core::account::ActorState;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    attest::{condition, u256},
    ferrata_account,
    sepolia::{self, gloas_file, SEPOLIA_CHAIN_ID},
    Chain, TxReport,
};

/// The vault's address on Sepolia.
pub const VAULT: &str = "dfedf9a73cfb337277b95a9a1c941c3496a4385f";
/// `sha256("ferrata/vault/leaf/v2")` (`vault/README.md`).
pub const LEAF_TAG: &str = "17b9508a0d0cbf61207c0fcb66f06ed92a6aeec110a54860479f0bf3cd6aff0b";
/// The root of the empty tree (`vault/README.md`).
pub const EMPTY_ROOT: &str = "782d35b1fdad7d54e7a1b36a2ab1021e872c7692bb80fdd12bfc321e9e420409";
/// The instance id of the synthetic record: no instance of the program has it.
pub const SYNTHETIC_INSTANCE: Root = [0x5e; 32];
/// The beacon slot the synthetic record names.
pub const SYNTHETIC_SLOT: u64 = 11_549_696;

pub fn vault_address() -> Address {
    hexb(VAULT).try_into().unwrap()
}

/// Storage slot `n` as a word.
pub fn slot_word(n: u8) -> Word {
    let mut w = [0u8; 32];
    w[31] = n;
    w
}

/// One deposit, as the vault's `Deposit` event reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Deposit {
    pub depositor: Address,
    pub leaf_index: u64,
    pub token: Address,
    pub amount: Word,
    pub commitment: Root,
}

impl Deposit {
    /// The vault's leaf: `sha256(0x00 ‖ LEAF_TAG ‖ leafIndex ‖ depositor ‖ token ‖ amount ‖ C)`.
    pub fn leaf(&self) -> Root {
        Sha256::new()
            .chain_update([0u8])
            .chain_update(hexb(LEAF_TAG))
            .chain_update(self.leaf_index.to_be_bytes())
            .chain_update(self.depositor)
            .chain_update(self.token)
            .chain_update(self.amount)
            .chain_update(self.commitment)
            .finalize()
            .into()
    }

    /// One log of `Deposit(address indexed depositor, uint64 index, uint64 leafIndex, address
    /// indexed token, uint256 amount, bytes32 commitment)` from `eth_getLogs`.
    pub fn from_log(v: &Value) -> Self {
        let topics = v["topics"].as_array().unwrap();
        let data = hexb(v["data"].as_str().unwrap());
        assert_eq!((topics.len(), data.len()), (3, 4 * 32), "a Deposit log");
        let w = |i: usize| -> [u8; 32] { data[32 * i..32 * (i + 1)].try_into().unwrap() };
        let topic = |i: usize| -> Address { hexb(topics[i].as_str().unwrap())[12..].try_into().unwrap() };
        Self {
            depositor: topic(1),
            leaf_index: u64::from_be_bytes(w(1)[24..].try_into().unwrap()),
            token: topic(2),
            amount: w(2),
            commitment: w(3),
        }
    }
}

/// The depth-32 accumulator over `leaves` (leaf `i` at index `i`): its root, and the path of the
/// leaf at `index`.
pub fn tree(leaves: &[Root], index: usize) -> (Root, Vec<Root>) {
    let mut zero = [0u8; 32];
    let mut level = leaves.to_vec();
    let mut at = index;
    let mut path = Vec::with_capacity(32);
    for _ in 0..32 {
        path.push(level.get(at ^ 1).copied().unwrap_or(zero));
        level = level.chunks(2).map(|p| accumulator_node(&p[0], p.get(1).unwrap_or(&zero))).collect();
        zero = accumulator_node(&zero, &zero);
        at /= 2;
    }
    (level.first().copied().unwrap_or(zero), path)
}

/// The vault's facts in one request: its account, slot 0 (equal to `root`), slot 1 (above the
/// highest leaf index asked), and each leaf `(index, leaf, path)` against the root in slot 0 (the
/// request's fact at position 1).
pub fn vault_request(base: Request, proof: &AccountProof, root: &Root, leaves: &[(u64, Root, Vec<Root>)]) -> Request {
    let slot = |n: u8, conditions| SlotQuery {
        slot: slot_word(n),
        proof: proof.storage.iter().find(|s| s.key == slot_word(n)).expect("the slot's proof").proof.clone(),
        conditions,
    };
    let highest = leaves.iter().map(|l| l.0).max().unwrap_or(0);
    Request {
        accounts: vec![AccountQuery {
            address: proof.address,
            proof: proof.proof.clone(),
            conditions: vec![],
            slots: vec![
                slot(0, vec![condition(Field::Value, Cmp::Eq, *root)]),
                slot(1, vec![condition(Field::Value, Cmp::Gt, u256(u128::from(highest)))]),
            ],
        }],
        leaves: leaves.iter().map(|(i, leaf, path)| LeafQuery { root: 1, leaf_index: *i, leaf: *leaf, path: path.clone() }).collect(),
        ..base
    }
}

/// Check an accepted vault attestation: the account, its two slots, then one leaf fact per leaf,
/// each named by the core's identifier.
pub fn check_vault_attestation(a: &Attestation, root: &Root, count: u64, leaves: &[(u64, Root, Vec<Root>)]) {
    let vault = a.statements[0].fact.clone();
    let Fact::Account { address, .. } = vault else { panic!("the vault's account first") };
    assert_eq!(a.statements[1].fact, Fact::Storage { address, slot: slot_word(0), value: *root });
    assert_eq!(a.statements[2].fact, Fact::Storage { address, slot: slot_word(1), value: u256(u128::from(count)) });
    assert_eq!(a.statements.len(), 3 + leaves.len());
    for (s, (index, leaf, _)) in a.statements[3..].iter().zip(leaves) {
        assert_eq!(
            s.fact,
            Fact::AccumulatorLeaf { address, slot: slot_word(0), leaf_index: *index, leaf: *leaf, root: *root }
        );
        assert_eq!(s.id, accumulator_leaf_id(a.chain_id, &address, &slot_word(0), *index, leaf));
    }
}

/// `Attest` at `actor` as the root of a transaction.
pub fn attest_root(chain: &mut Chain, label: &str, actor: Actor, request: &Request, time: u64) -> TxReport {
    let message = borsh::to_vec(&Message::Attest { request: request.clone() }).unwrap();
    chain.submit(label, actor, message, vec![actor], vec![], time)
}

/// Place `record` of `instance_id` on chain `chain_id` at the program's record address, as the
/// program writes it (`RecordState`).
pub fn place_record(chain: &mut Chain, instance_id: Root, chain_id: u64, record: &HeaderRecord) -> Actor {
    let actor = header_actor(ferrata_account(), &instance_id, record.beacon.slot);
    let state = RecordState { instance_id, chain_id, record: record.encode() };
    let account =
        Account::default().with_actor_state(actor.program_account_id, ActorState::from(borsh::to_vec(&state).unwrap()));
    chain.state.force_insert_account(actor.account_id, account);
    actor
}

/// One attestation at a record: accepted with the natively evaluated event, or refused with the
/// native code. `native` is the record's (instance id, chain id, record).
pub fn run(
    chain: &mut Chain,
    native: (&Root, u64, &HeaderRecord),
    label: &str,
    actor: Actor,
    request: &Request,
    time: u64,
) -> (TxReport, Result<Attestation, u8>) {
    let expected = evaluate(native.0, native.1, native.2, request);
    let report = attest_root(chain, label, actor, request, time);
    match &expected {
        Ok(a) => {
            assert!(report.ok(), "{label}: {:?} (exit {:?})", report.charge.revert, report.exit_code);
            let event = report.events.iter().find(|e| e.event.selector == ATTESTED_SELECTOR).expect("an event");
            assert_eq!(event.event.data, a.event_data(), "{label}: the event names the expected identifiers");
        }
        Err(code) => {
            assert_eq!(report.exit_code, Some(u32::from(*code)), "{label}: {:?}", report.charge.revert);
            assert!(report.events.is_empty(), "{label}: a refusal emits nothing");
        }
    }
    (report, expected)
}

// ---------------------------------------------------------------------------------------------
// The synthetic vault
// ---------------------------------------------------------------------------------------------

/// The vault's tree vectors (`vault/test/vectors/tree-v2.json` of the vault's branch).
pub fn tree_vectors() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vectors/vault-tree-v2.json");
    serde_json::from_slice(&fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))).unwrap()
}

fn number(v: &Value) -> u128 {
    v.as_u64().map(u128::from).unwrap_or_else(|| v.as_str().unwrap().parse().unwrap())
}

/// A block whose state holds the vault with the deposits of the tree vectors.
pub struct Synthetic {
    pub deposits: Vec<Deposit>,
    pub leaves: Vec<Root>,
    pub root: Root,
    /// The vault's account and slots 0 and 1, as `eth_getProof` gives them.
    pub proof: AccountProof,
    /// The block's RLP header and hash.
    pub header: Vec<u8>,
    pub block_hash: Root,
    /// A record in the Gloas layout that binds the block's hash.
    pub record: HeaderRecord,
}

pub fn synthetic() -> Synthetic {
    let v = tree_vectors();
    assert_eq!(v["leafTag"].as_str().unwrap().trim_start_matches("0x"), LEAF_TAG);
    let deposits: Vec<Deposit> = v["deposits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| Deposit {
            depositor: hexb(d["depositor"].as_str().unwrap()).try_into().unwrap(),
            leaf_index: number(&d["leafIndex"]) as u64,
            token: hexb(d["token"].as_str().unwrap()).try_into().unwrap(),
            amount: u256(number(&d["amount"])),
            commitment: word(d["commitment"].as_str().unwrap()),
        })
        .collect();
    let leaves: Vec<Root> = deposits.iter().map(Deposit::leaf).collect();
    for (d, leaf) in v["deposits"].as_array().unwrap().iter().zip(&leaves) {
        assert_eq!(*leaf, word(d["leaf"].as_str().unwrap()), "the vault's own leaf");
    }
    let (root, _) = tree(&leaves, 0);
    assert_eq!(root, word(v["finalRoot"].as_str().unwrap()), "the vault's own root");
    assert_eq!(tree(&[], 0).0, word(EMPTY_ROOT), "the empty tree");
    let count = u256(leaves.len() as u128);
    let storage = Trie::new([
        (keccak(&slot_word(0)).to_vec(), enc::uint(&root)),
        (keccak(&slot_word(1)).to_vec(), enc::uint(&count)),
    ]);
    let vault = vault_address();
    let code_hash = keccak(b"FerrataVault");
    let mut accounts = vec![(vault, account_rlp(1, 0, storage.root(), code_hash))];
    for i in 0x40..0x68u8 {
        accounts.push(([i; 20], account_rlp(u64::from(i), 1_000 + u128::from(i), keccak(&[0x80]), keccak(b""))));
    }
    let state = Trie::new(accounts.iter().map(|(a, rlp)| (keccak(a).to_vec(), rlp.clone())));
    let proof = AccountProof {
        address: vault,
        nonce: 1,
        balance: [0; 32],
        storage_hash: storage.root(),
        code_hash,
        proof: state.proof(&keccak(&vault)),
        storage: [(0u8, root), (1, count)]
            .iter()
            .map(|(n, value)| StorageProof {
                key: slot_word(*n),
                value: *value,
                proof: storage.proof(&keccak(&slot_word(*n))),
            })
            .collect(),
    };
    // A real Sepolia header (the replay fixture's bound block) with the synthetic state root.
    let served: Value = serde_json::from_slice(&gloas_file("block.json")).unwrap();
    let mut block = served["result"].clone();
    block["stateRoot"] = Value::String(format!("0x{}", hex::encode(state.root())));
    let header = header_rlp(&block);
    let block_hash = keccak(&header);
    let record = HeaderRecord {
        beacon: BeaconBlockHeader { slot: SYNTHETIC_SLOT, ..Default::default() },
        execution_root: [0; 32],
        execution: ExecutionSummary::BlockHash(block_hash),
    };
    Synthetic { deposits, leaves, root, proof, header, block_hash, record }
}

/// The vault's request through the program on the synthetic state: slots 0 and 1 alone, with one
/// leaf, with every leaf; then refusals. Returns the accepted and the refused reports.
pub fn synthetic_attestations() -> (Vec<TxReport>, Vec<TxReport>) {
    let s = synthetic();
    let mut chain = Chain::new();
    let actor = place_record(&mut chain, SYNTHETIC_INSTANCE, SEPOLIA_CHAIN_ID, &s.record);
    let native = (&SYNTHETIC_INSTANCE, SEPOLIA_CHAIN_ID, &s.record);
    let time = 1_800_000_000_000;
    let base = Request {
        instance_id: SYNTHETIC_INSTANCE,
        chain_id: SEPOLIA_CHAIN_ID,
        header: Some(s.header.clone()),
        ..Request::default()
    };
    let n = s.leaves.len();
    let leaf = |i: usize| (i as u64, s.leaves[i], tree(&s.leaves, i).1);
    let mut accepted = Vec::new();
    for (label, leaves) in [
        ("vault slots 0 and 1, no leaf (synthetic state, Gloas record)", vec![]),
        ("vault slots 0 and 1 and one deposit leaf", vec![leaf(n - 1)]),
        ("vault slots 0 and 1 and two deposit leaves", vec![leaf(0), leaf(n - 1)]),
        ("vault slots 0 and 1 and all eight deposit leaves", (0..n).map(leaf).collect()),
    ] {
        let request = vault_request(base.clone(), &s.proof, &s.root, &leaves);
        let (report, a) = run(&mut chain, native, label, actor, &request, time);
        check_vault_attestation(&a.expect("accepted"), &s.root, n as u64, &leaves);
        accepted.push(report);
    }
    let one = vault_request(base.clone(), &s.proof, &s.root, &[leaf(n - 1)]);
    let mut refused = Vec::new();
    let mut wrong_leaf = one.clone();
    wrong_leaf.leaves[0].leaf = s.leaves[0];
    let mut wrong_index = one.clone();
    wrong_index.leaves[0].leaf_index = 0;
    let mut against_count = one.clone();
    against_count.leaves[0].root = 2;
    let past_count = vault_request(base.clone(), &s.proof, &s.root, &[(n as u64, [0; 32], tree(&s.leaves, n).1)]);
    for (label, request) in [
        ("a leaf that is not at its index", wrong_leaf),
        ("a leaf proven at another index", wrong_index),
        ("a leaf against slot 1 (the count, not the root)", against_count),
        ("an empty position past the count (slot 1 condition)", past_count),
        ("the vault's facts asked as mainnet statements", Request { chain_id: 1, ..one.clone() }),
        ("the vault's facts without the block header", Request { header: None, ..one }),
    ] {
        let (report, result) = run(&mut chain, native, label, actor, &request, time);
        assert!(result.is_err(), "{label}: refused");
        refused.push(report);
    }
    (accepted, refused)
}

// ---------------------------------------------------------------------------------------------
// The vault on Sepolia
// ---------------------------------------------------------------------------------------------

/// The vault fixture: `FERRATA_VAULT_FIXTURE_DIR`, else `fixtures/sepolia-vault` of this checkout,
/// if it exists.
pub fn vault_fixture_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("FERRATA_VAULT_FIXTURE_DIR").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/ferrata-spectests/fixtures/sepolia-vault")
    });
    dir.is_dir().then_some(dir)
}

/// A file of the vault fixture, checked against the fixture's own `SHA256SUMS`.
fn vault_file(dir: &Path, name: &str) -> Vec<u8> {
    let sums = String::from_utf8(fs::read(dir.join("SHA256SUMS")).expect("SHA256SUMS")).unwrap();
    sepolia::pinned(dir.to_path_buf(), &sums, name)
}

/// The vault's slots 0 and 1 and its newest deposit leaf, under the record of the fixture's
/// finalized header, through the program: the Sepolia instance follows the chain from the
/// period-1378 bootstrap through the replay fixture, the fixture's own updates and its finality
/// update. Returns the reports of the attestation.
pub fn real(dir: &Path) -> Vec<TxReport> {
    let m: Value = serde_json::from_slice(&vault_file(dir, "fixture.json")).unwrap();
    let vault = vault_address();
    let storage = &m["storage"][0];
    assert_eq!(hexb(storage["address"].as_str().unwrap()), vault.to_vec(), "the fixture is the vault's");
    let block = m["block"].as_u64().unwrap();
    let mut w = sepolia::replay(2, sepolia::LAST_PERIOD);
    for p in m["updates"].as_array().unwrap() {
        let p = p.as_u64().unwrap();
        let update =
            LightClientUpdate::decode(&vault_file(dir, &format!("update_{p}.ssz")), LcFork::Gloas, 512).unwrap();
        w.apply(&update, &format!("period {p}"));
    }
    let finality = sepolia::gloas_finality_update(&vault_file(dir, "finality_update.ssz"));
    let run_ = w.apply(&finality, "finality update (vault fixture)");
    assert_eq!(run_.outcome.map(|o| o.advanced), Some(true));
    let slot = m["finalized_slot"].as_u64().unwrap();
    let state = w.inst.record(&w.chain, slot).expect("the record");
    let record = state.header().unwrap();

    let proof = account_proof(&vault_file(dir, &format!("proof_0x{VAULT}_{block}.json")));
    let value = |n: u8| proof.storage.iter().find(|s| s.key == slot_word(n)).expect("slot").value;
    let (root, count) = (value(0), value(1));
    let logs: Vec<Value> = serde_json::from_slice(&vault_file(dir, &format!("deposits_{block}.json"))).unwrap();
    let deposits: Vec<Deposit> = logs.iter().map(Deposit::from_log).collect();
    for (i, d) in deposits.iter().enumerate() {
        assert_eq!(d.leaf_index, i as u64, "the deposits in leaf order, none missing");
    }
    assert_eq!(u256(deposits.len() as u128), count, "slot 1 counts the deposits");
    assert!(!deposits.is_empty(), "the vault has a deposit");
    let leaves: Vec<Root> = deposits.iter().map(Deposit::leaf).collect();
    let newest = leaves.len() - 1;
    let (computed, path) = tree(&leaves, newest);
    assert_eq!(computed, root, "slot 0 is the root of the deposits' leaves");
    let leaf = [(newest as u64, leaves[newest], path)];
    let base = Request {
        instance_id: w.inst.id,
        chain_id: SEPOLIA_CHAIN_ID,
        header: Some(vault_file(dir, "header.rlp")),
        ..Request::default()
    };
    let request = vault_request(base, &proof, &root, &leaf);
    let actor = header_actor(w.inst.program, &w.inst.id, slot);
    let time = w.inst.config.slot_start_ms(finality.signature_slot) + 12_000;
    let id = w.inst.id;
    let (report, a) =
        run(&mut w.chain, (&id, SEPOLIA_CHAIN_ID, &record), "the vault: slots 0, 1 and its newest deposit leaf", actor, &request, time);
    check_vault_attestation(&a.expect("accepted"), &root, deposits.len() as u64, &leaf);
    vec![report]
}
