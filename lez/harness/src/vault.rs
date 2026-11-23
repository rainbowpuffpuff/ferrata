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

use std::{fs, path::PathBuf};

use ferrata_core::rlp::encode_u64;
use ferrata_core::{
    accumulator::{accumulator_leaf_id, accumulator_node},
    fork::LcFork,
    types::{BeaconBlockHeader, LightClientUpdate},
    witness::{ExecutionSummary, HeaderRecord},
    Root,
};
use ferrata_lez_wire::{
    attest::{
        evaluate, AccountQuery, Address, Attestation, Cmp, Fact, Field, Hop, LeafQuery, LogQuery, Request, SlotQuery,
        Word,
    },
    consumer::DepositProof,
    header_actor, Message, RecordState, ATTESTED_SELECTOR,
};
use ferrata_spectests::execkit::{
    account_proof, account_rlp, enc, header_rlp, hexb, keccak, receipt_rlp, word, AccountProof, StorageProof, Trie,
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
        leaves: leaves
            .iter()
            .map(|(i, leaf, path)| LeafQuery { root: 1, leaf_index: *i, leaf: *leaf, path: path.clone() })
            .collect(),
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
    let state = RecordState { instance_id, chain_id, record: record.encode(), root_id: instance_id };
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
    // What the header costs: the same request under a record of the same block that holds the
    // payload's fields (the Capella-to-Fulu layouts), which takes no header.
    let h = ferrata_core::exec::decode_header_with_hash(&s.header, &s.block_hash).unwrap();
    let payload = HeaderRecord {
        beacon: BeaconBlockHeader { slot: SYNTHETIC_SLOT + 1, ..Default::default() },
        execution_root: [0; 32],
        execution: ExecutionSummary::Payload {
            block_number: h.number,
            block_hash: s.block_hash,
            parent_hash: h.parent_hash,
            state_root: h.state_root,
            receipts_root: h.receipts_root,
            timestamp: h.timestamp,
        },
    };
    let at_payload = place_record(&mut chain, SYNTHETIC_INSTANCE, SEPOLIA_CHAIN_ID, &payload);
    let (report, a) = run(
        &mut chain,
        (&SYNTHETIC_INSTANCE, SEPOLIA_CHAIN_ID, &payload),
        "the same with one leaf, under a record that holds the payload's fields (no header)",
        at_payload,
        &Request { header: None, ..one.clone() },
        time,
    );
    let by_payload = a.expect("accepted");
    let by_header = evaluate(&SYNTHETIC_INSTANCE, SEPOLIA_CHAIN_ID, &s.record, &one).unwrap();
    assert_eq!(by_payload.statements, by_header.statements, "the same facts and identifiers");
    accepted.push(report);
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
// The vault on Sepolia: FerrataVault v5 of the live round trip (`e2e/runs/crown-live-1`)
// ---------------------------------------------------------------------------------------------

/// FerrataVault v5 on Sepolia (`e2e/runs/crown-live-1`: deployed in block 12,111,048, two ETH
/// deposits, in blocks 12,111,477 and 12,111,574).
pub const VAULT_V5: &str = "fe3199fa2634413cbac317371076cae3eb2d9e54";
/// The fixture of `lez/scripts/fetch-attest-fixture.py --network sepolia --storage
/// VAULT_V5:0=depositRoot(),1=nextLeafIndex() --deposits VAULT_V5:12111048 --updates-from 1410
/// --hops --absent`, fetched on 11 November 2026: finalized slot 11,556,608 binds block
/// 12,112,773.
pub const VAULT_FIXTURE: &str = "sepolia-vault-v5-2026-11-11";
const VAULT_SUMS: &str =
    include_str!("../../../crates/ferrata-spectests/fixtures/sepolia-vault-v5-2026-11-11/SHA256SUMS");

/// `FERRATA_VAULT_FIXTURE_DIR` if set, else the fixture of this checkout.
pub fn vault_fixture_dir() -> PathBuf {
    std::env::var_os("FERRATA_VAULT_FIXTURE_DIR").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/ferrata-spectests/fixtures").join(VAULT_FIXTURE)
    })
}

/// A file of the vault fixture, checked against its SHA-256 sum compiled into the harness.
pub fn vault_file(name: &str) -> Vec<u8> {
    sepolia::pinned(vault_fixture_dir(), VAULT_SUMS, name)
}

/// One deposit's log as `eth_getLogs` reports it, and where it sits.
#[derive(Clone, Debug)]
pub struct DepositLog {
    pub deposit: Deposit,
    pub block: u64,
    pub tx_index: u64,
    /// The log's position in its receipt (JSON-RPC's `logIndex` is block-wide).
    pub log_index: u64,
    pub json: Value,
}

/// The vault on Sepolia under the record of the fixture's finalized block, which the Sepolia
/// instance wrote itself: it follows the chain from the period-1378 bootstrap through the replay
/// fixture and the fixture's finality update.
pub struct RealVault {
    pub w: crate::scenarios::World,
    pub vault: Address,
    pub block: u64,
    pub slot: u64,
    pub record: HeaderRecord,
    pub header: Vec<u8>,
    pub proof: AccountProof,
    pub root: Root,
    pub count: u64,
    pub deposits: Vec<DepositLog>,
    pub leaves: Vec<Root>,
    pub time: u64,
    pub m: Value,
}

impl RealVault {
    pub fn load() -> Self {
        let m: Value = serde_json::from_slice(&vault_file("fixture.json")).unwrap();
        let mut w = sepolia::replay(2, sepolia::LAST_PERIOD);
        for p in m["updates"].as_array().unwrap() {
            let p = p.as_u64().unwrap();
            let update =
                LightClientUpdate::decode(&vault_file(&format!("update_{p}.ssz")), LcFork::Gloas, 512).unwrap();
            w.apply(&update, &format!("period {p}"));
        }
        let finality = sepolia::gloas_finality_update(&vault_file("finality_update.ssz"));
        let run_ = w.apply(&finality, "finality update (vault fixture)");
        assert_eq!(run_.outcome.map(|o| o.advanced), Some(true));
        let slot = m["finalized_slot"].as_u64().unwrap();
        let record = w.inst.record(&w.chain, slot).expect("the record").header().unwrap();
        // What `placed` places is the record the instance wrote.
        assert_eq!(
            record,
            HeaderRecord::of(&finality.finalized_header, LcFork::Gloas, &w.spec),
            "RealVault::placed's record"
        );
        Self::on(m, w, record, &finality)
    }

    /// The same vault under the same record without the replay: the record placed (`place_record`)
    /// on a new chain beside the Sepolia instance's id (the instance is not on that chain), as
    /// `HeaderRecord::of` gives it for the fixture's finalized header in the Gloas layout, which is
    /// what the replayed instance writes. For measurements of what reads the record and not the
    /// instance, at a fraction of [`RealVault::load`]'s cost (`tests/costs_attestation.rs` pins the
    /// reference consumer's charges on it to the figures measured on the replayed world).
    pub fn placed() -> Self {
        use ferrata_core::policy::{Adoption, FinalizedStore};
        let m: Value = serde_json::from_slice(&vault_file("fixture.json")).unwrap();
        let config = sepolia::sepolia_config(2);
        let spec = config.spec();
        let checkpoint = crate::root(sepolia::SEPOLIA_CHECKPOINT);
        let bootstrap = sepolia::sepolia_bootstrap();
        let inst = crate::Instance::new(crate::ferrata_account(), config, checkpoint);
        let native = FinalizedStore::initialize_with(
            &checkpoint,
            &bootstrap,
            inst.fork(),
            &spec,
            &crate::scenarios::BLS,
            Adoption::OnChain,
        )
        .unwrap();
        let mut w = crate::scenarios::World {
            chain: Chain::new(),
            inst,
            native,
            spec,
            bootstrap,
            updates: vec![],
            runs: vec![],
        };
        let finality = sepolia::gloas_finality_update(&vault_file("finality_update.ssz"));
        let record = HeaderRecord::of(&finality.finalized_header, LcFork::Gloas, &w.spec);
        assert_eq!(Some(record.beacon.slot), m["finalized_slot"].as_u64(), "the finalized header's record");
        place_record(&mut w.chain, w.inst.id, SEPOLIA_CHAIN_ID, &record);
        Self::on(m, w, record, &finality)
    }

    /// The vault's data from the fixture, under `record` on `w`'s chain.
    fn on(m: Value, w: crate::scenarios::World, record: HeaderRecord, finality: &LightClientUpdate) -> Self {
        assert_eq!(m["chain_id"].as_u64(), Some(SEPOLIA_CHAIN_ID));
        let vault: Address = hexb(VAULT_V5).try_into().unwrap();
        let storage = &m["storage"][0];
        assert_eq!(hexb(storage["address"].as_str().unwrap()), vault.to_vec(), "the fixture is the vault's");
        assert_eq!(m["deposits"]["vault"].as_str().map(hexb), Some(vault.to_vec()));
        let block = m["block"].as_u64().unwrap();
        let slot = m["finalized_slot"].as_u64().unwrap();
        let header = vault_file("header.rlp");
        let proof = account_proof(&vault_file(&format!("proof_0x{VAULT_V5}_{block}.json")));
        let value = |n: u8| proof.storage.iter().find(|s| s.key == slot_word(n)).expect("slot").value;
        let (root, count) = (value(0), value(1));
        let logs: Vec<Value> = serde_json::from_slice(&vault_file(&format!("deposits_{block}.json"))).unwrap();
        let deposits: Vec<DepositLog> = logs
            .iter()
            .map(|l| {
                let block =
                    u64::from_str_radix(l["blockNumber"].as_str().unwrap().trim_start_matches("0x"), 16).unwrap();
                let tx_index =
                    u64::from_str_radix(l["transactionIndex"].as_str().unwrap().trim_start_matches("0x"), 16).unwrap();
                let receipts = receipts_of(block);
                let receipt = &receipts[tx_index as usize];
                let log_index = receipt["logs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .position(|x| x["logIndex"] == l["logIndex"])
                    .expect("the log is in its receipt") as u64;
                DepositLog { deposit: Deposit::from_log(l), block, tx_index, log_index, json: l.clone() }
            })
            .collect();
        for (i, d) in deposits.iter().enumerate() {
            assert_eq!(d.deposit.leaf_index, i as u64, "the deposits in leaf order, none missing");
        }
        assert_eq!(u256(deposits.len() as u128), count, "slot 1 counts the deposits");
        let leaves: Vec<Root> = deposits.iter().map(|d| d.deposit.leaf()).collect();
        assert_eq!(tree(&leaves, 0).0, root, "slot 0 is the root of the deposits' leaves");
        let time = w.inst.config.slot_start_ms(finality.signature_slot) + 12_000;
        let count = deposits.len() as u64;
        Self { w, vault, block, slot, record, header, proof, root, count, deposits, leaves, time, m }
    }

    /// Leaf `i` with its path.
    pub fn leaf(&self, i: usize) -> (u64, Root, Vec<Root>) {
        (i as u64, self.leaves[i], tree(&self.leaves, i).1)
    }

    /// A request to the record: its instance, Sepolia, and the record block's RLP header.
    pub fn base(&self) -> Request {
        Request {
            instance_id: self.w.inst.id,
            chain_id: SEPOLIA_CHAIN_ID,
            header: Some(self.header.clone()),
            ..Request::default()
        }
    }

    /// `header(id, slot)` of the record.
    pub fn actor(&self) -> Actor {
        header_actor(self.w.inst.program, &self.w.inst.id, self.slot)
    }

    /// The EIP-2935 hop from the record's block to `block`.
    pub fn hop(&self, block: u64) -> Hop {
        let history = account_proof(&vault_file(&format!("proof_history_{}.json", self.block)));
        let key = u256(u128::from(block % 8191));
        let slot = history.storage.iter().find(|s| s.key == key).expect("the history slot of the block");
        Hop {
            account_proof: history.proof.clone(),
            storage_proof: slot.proof.clone(),
            header: vault_file(&format!("header_{block}.rlp")),
        }
    }

    /// The proof of transaction `tx_index`'s receipt in `block`.
    pub fn receipt_proof(&self, block: u64, tx_index: u64) -> Vec<Vec<u8>> {
        let receipts = receipts_of(block);
        let trie = Trie::new(receipts.iter().enumerate().map(|(i, r)| {
            let (key, len) = encode_u64(i as u64);
            (key[..len].to_vec(), receipt_rlp(r))
        }));
        let (key, len) = encode_u64(tx_index);
        trie.proof(&key[..len])
    }

    /// Deposit `i`'s log, one hop back from the record's block.
    pub fn deposit_proof(&self, i: usize) -> DepositProof {
        let d = &self.deposits[i];
        DepositProof {
            header: Some(self.header.clone()),
            hops: vec![self.hop(d.block)],
            tx_index: d.tx_index,
            log_index: d.log_index,
            proof: self.receipt_proof(d.block, d.tx_index),
        }
    }

    /// Deposit `i`'s statement identifier, from the JSON-RPC fields alone.
    pub fn deposit_id(&self, i: usize) -> Root {
        let d = &self.deposits[i];
        crate::attest::log_statement_id(SEPOLIA_CHAIN_ID, d.block, d.tx_index, d.log_index, &d.json)
    }

    /// One attestation at the record, checked against the native evaluation.
    pub fn run(&mut self, label: &str, request: &Request) -> (TxReport, Result<Attestation, u8>) {
        let (id, record, actor, time) = (self.w.inst.id, self.record.clone(), self.actor(), self.time);
        run(&mut self.w.chain, (&id, SEPOLIA_CHAIN_ID, &record), label, actor, request, time)
    }
}

/// `header(id, slot)` of the program and instance `pool` is bound to.
pub fn header_of(pool: &ferrata_lez_wire::consumer::VaultPool, slot: u64) -> Actor {
    header_actor(pool.ferrata, &pool.instance_id, slot)
}

/// `eth_getBlockReceipts` of `block` from the vault fixture.
fn receipts_of(block: u64) -> Vec<Value> {
    let served: Value = serde_json::from_slice(&vault_file(&format!("receipts_{block}.json"))).unwrap();
    served["result"].as_array().expect("receipts").clone()
}

/// The vault's leaves on real data, through the program, under the record the instance wrote:
/// slots 0 and 1 and each deposit leaf; an address without an account (exclusion); each
/// deposit's `Deposit` log one EIP-2935 hop back; then the refusals. Returns the vault (for
/// more) and the accepted and refused reports.
pub fn real() -> (RealVault, Vec<TxReport>, Vec<TxReport>) {
    let mut v = RealVault::load();
    let (root, count) = (v.root, v.count);
    let mut accepted = Vec::new();
    for (label, leaves) in [
        ("the vault v5: slots 0 and 1 and deposit leaf 0 (real Sepolia state)", vec![v.leaf(0)]),
        ("the vault v5: slots 0 and 1 and deposit leaf 1", vec![v.leaf(1)]),
        ("the vault v5: slots 0 and 1 and both deposit leaves", vec![v.leaf(0), v.leaf(1)]),
    ] {
        let request = vault_request(v.base(), &v.proof, &root, &leaves);
        let (report, a) = v.run(label, &request);
        check_vault_attestation(&a.expect("accepted"), &root, count, &leaves);
        accepted.push(report);
    }
    // Exclusion under the Gloas record: an address without an account.
    let absent = account_proof(&vault_file(&format!("proof_absent_{}.json", v.block)));
    let request = Request {
        accounts: vec![AccountQuery {
            address: absent.address,
            proof: absent.proof.clone(),
            conditions: vec![condition(Field::Nonce, Cmp::Eq, [0; 32])],
            slots: vec![],
        }],
        ..v.base()
    };
    let (report, a) = v.run("an address without an account (exclusion), under the Gloas record", &request);
    let a = a.expect("accepted");
    assert_eq!(
        a.statements[0].fact,
        Fact::Account {
            address: absent.address,
            nonce: 0,
            balance: [0; 32],
            storage_root: [0; 32],
            code_hash: [0; 32]
        }
    );
    accepted.push(report);
    // Each deposit's event, one EIP-2935 hop back from the record's block.
    for i in 0..v.deposits.len() {
        let p = v.deposit_proof(i);
        let request = Request {
            hops: p.hops.clone(),
            logs: vec![LogQuery {
                tx_index: p.tx_index,
                log_index: p.log_index,
                proof: p.proof.clone(),
                conditions: vec![],
            }],
            ..v.base()
        };
        let label = format!(
            "deposit {i}'s Deposit log, block {}, one EIP-2935 hop under the Gloas record",
            v.deposits[i].block
        );
        let (report, a) = v.run(&label, &request);
        let a = a.expect("accepted");
        assert_eq!((a.block_number, a.hops), (v.deposits[i].block, 1));
        assert_eq!(a.statements[0].id, v.deposit_id(i), "the identifier from the JSON-RPC fields alone");
        accepted.push(report);
    }

    let one = vault_request(v.base(), &v.proof, &root, &[v.leaf(1)]);
    let mut refused = Vec::new();
    let mut wrong_leaf = one.clone();
    wrong_leaf.leaves[0].leaf = v.leaves[0];
    let mut wrong_index = one.clone();
    wrong_index.leaves[0].leaf_index = 0;
    let mut against_count = one.clone();
    against_count.leaves[0].root = 2;
    let past_count = vault_request(v.base(), &v.proof, &root, &[(2, [0; 32], tree(&v.leaves, 2).1)]);
    let mut tampered_sibling = one.clone();
    tampered_sibling.leaves[0].path[0][0] ^= 1;
    let p0 = v.deposit_proof(0);
    let hop_log = |hops: Vec<Hop>| Request {
        hops,
        logs: vec![LogQuery {
            tx_index: p0.tx_index,
            log_index: p0.log_index,
            proof: p0.proof.clone(),
            conditions: vec![],
        }],
        ..v.base()
    };
    let other_header = Hop { header: vault_file(&format!("header_{}.rlp", v.deposits[1].block)), ..p0.hops[0].clone() };
    let false_predicate = Request {
        logs: vec![LogQuery {
            conditions: vec![condition(
                Field::Topic(2),
                Cmp::Eq,
                ferrata_lez_wire::consumer::address_word(&[0x11; 20]),
            )],
            ..hop_log(p0.hops.clone()).logs[0].clone()
        }],
        ..hop_log(p0.hops.clone())
    };
    for (label, request) in [
        ("a leaf that is not at its index", wrong_leaf),
        ("a leaf proven at another index", wrong_index),
        ("a leaf against slot 1 (the count, not the root)", against_count),
        ("an empty position past the count (slot 1 condition)", past_count),
        ("a leaf with one sibling changed", tampered_sibling),
        ("the vault's facts asked as mainnet statements", Request { chain_id: 1, ..one.clone() }),
        ("the vault's facts without the block header", Request { header: None, ..one.clone() }),
        ("deposit 0's log through a hop that carries deposit 1's block header", hop_log(vec![other_header])),
        ("deposit 0's log without the hop (proof against the record's block)", hop_log(vec![])),
        ("deposit 0's log with a false predicate (token topic = 0x11…11)", false_predicate),
    ] {
        let (report, result) = v.run(label, &request);
        assert!(result.is_err(), "{label}: refused");
        refused.push(report);
    }
    (v, accepted, refused)
}

/// The live run's openings of its two deposits' commitments, `(s, kind, dest)`, checked against
/// the commitments on chain. `s = HMAC(kAuth, "ferrata/s/v1" ‖ chain ‖ vault ‖ depositor ‖ j)` from
/// the run's test seed `sha256("e2e: user bridge seed (test value)")` (`e2e/crown/identity.py`);
/// deposit 0 is public-kind, to LEZ's public test account 0, deposit 1 private-kind, to LEZ's
/// private test account 0.
pub fn crown_openings(v: &RealVault) -> Vec<(Root, u8, Root)> {
    use ferrata_bridge_core::{inbound::deposit_secret, sha256, BridgeKeys};
    use ferrata_lez_wire::consumer::{commitment, KIND_PRIVATE, KIND_PUBLIC};
    let keys = BridgeKeys::from_bridge_seed(&sha256(b"e2e: user bridge seed (test value)"));
    let chain = ferrata_bridge_core::chain_id_u256(SEPOLIA_CHAIN_ID);
    let dests = [
        (KIND_PUBLIC, word("0x54d662b664519cb8aef608ae84a63e041cc16019e4a07c34828311996f0c817a")),
        (KIND_PRIVATE, word("0x2d6cb849edc6d8b1599c558ecd87bc09b4420bdc60f1b3be85547a5834b153fc")),
    ];
    v.deposits
        .iter()
        .zip(dests)
        .enumerate()
        .map(|(j, (d, (kind, dest)))| {
            let s = deposit_secret(&keys.auth, &chain, &v.vault, &d.deposit.depositor, j as u64);
            assert_eq!(commitment(&s, kind, &dest), d.deposit.commitment, "deposit {j}: the run's opening of C");
            (s, kind, dest)
        })
        .collect()
}

/// The native-ETH pool of the vault, bound to the instance.
pub fn eth_pool(v: &RealVault) -> ferrata_lez_wire::consumer::VaultPool {
    ferrata_lez_wire::consumer::VaultPool {
        ferrata: v.w.inst.program,
        instance_id: v.w.inst.id,
        chain_id: SEPOLIA_CHAIN_ID,
        vault: v.vault,
        token: [0; 20],
    }
}

/// The keys of LEZ v0.4.0-rc1's testnet private account 0 (`testnet_initial_state`, a published
/// test constant), as the harness's `TestPrivateKeys`: `ask = sha256("/LEE-Keys/v1/Authorization/
/// Secret" ‖ ssk ‖ 0u32)` and the viewing seed `d ‖ z = HMAC-SHA512(key "/LEE-Keys/v1/Viewing/Seed",
/// "/LEE-Keys/v1/Viewing/Secret" ‖ ssk ‖ 0u32)` (`lee/key_protocol`, `SecretSpendingKey`). The live
/// run's private deposit is to this account.
pub fn lez_testnet_private_account_0() -> lee::test_utils::TestPrivateKeys {
    use sha2::Sha512;
    const SSK: [u8; 32] = [
        93, 13, 190, 240, 250, 33, 108, 195, 176, 40, 144, 61, 4, 28, 58, 112, 53, 161, 42, 238, 155, 27, 23, 176, 208,
        121, 15, 229, 165, 180, 99, 143,
    ];
    let ask: [u8; 32] = Sha256::new()
        .chain_update(b"/LEE-Keys/v1/Authorization/Secret")
        .chain_update(SSK)
        .chain_update(0u32.to_be_bytes())
        .finalize()
        .into();
    let message = [&b"/LEE-Keys/v1/Viewing/Secret"[..], &SSK, &0u32.to_be_bytes()].concat();
    // HMAC-SHA512 (RFC 2104, 128-byte blocks) with a key shorter than a block.
    let key = b"/LEE-Keys/v1/Viewing/Seed";
    let mut block = [0u8; 128];
    block[..key.len()].copy_from_slice(key);
    let pad = |byte: u8| block.map(|b| b ^ byte);
    let inner = Sha512::new().chain_update(pad(0x36)).chain_update(&message).finalize();
    let seed = Sha512::new().chain_update(pad(0x5c)).chain_update(inner).finalize();
    lee::test_utils::TestPrivateKeys {
        ask: lee_core::AuthorizationSecretKey(ask),
        d: seed[..32].try_into().unwrap(),
        z: seed[32..].try_into().unwrap(),
    }
}
