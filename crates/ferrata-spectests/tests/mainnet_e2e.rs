//! Real mainnet, end to end (`fixtures/mainnet-e2e-2026-11-09`; its SOURCE.md lists every
//! request): a bootstrap and a finality update of sync-committee period 1906, verified with real
//! BLS signatures; the finalized header's execution payload as a [`HeaderRecord`]; accounts and
//! storage slots under it, equal to what JSON-RPC reports (`eth_getProof`, `eth_call`); the
//! receipts trie rebuilt with alloy-trie and every log of the block attested; and EIP-2935 hops to
//! the parent block (with three failed transactions), to a block 16 back, and from there to the
//! oldest block it can reach, with storage facts under the derived records. Tampering with any
//! kind of fixture byte is refused with an exact code.

use std::{collections::BTreeMap, fs, path::PathBuf};

use alloy_primitives::B256;
use alloy_trie::{proof::ProofRetainer, HashBuilder, Nibbles};
use ferrata_core::{
    attest::{
        account_id, attest_account, attest_batch, attest_log, attest_storage, history_hop, log_id, storage_id,
        AccountQuery, Batch, Binding, Cmp, Condition, Fact, Field, HeaderRecord, LogQuery, Predicate, SlotQuery,
        HISTORY_CODE_HASH, HISTORY_SERVE_WINDOW, HISTORY_STORAGE_ADDRESS,
    },
    bls::{Aggregation, BlsBackend, Blst, Zkcrypto},
    exec::{decode_header, Account, HeaderShape, Receipt},
    fork::{ChainSpec, LcFork, Preset, FAR_FUTURE_EPOCH},
    keccak::keccak256,
    merkle::ZERO,
    policy::{Adoption, FinalizedStore},
    rlp::encode_u64,
    types::{LightClientBootstrap, LightClientHeader, LightClientUpdate, SyncAggregate, SyncCommittee},
    validate::Context,
    Error, Root,
};
use ferrata_spectests::execkit::{
    account_proof, header_rlp, hexb, quantity, receipt_rlp, word, AccountProof, Trie, TRANSFER,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

fn mainnet() -> ChainSpec {
    ChainSpec {
        preset: Preset::MAINNET,
        versions: [
            [0, 0, 0, 0],
            [1, 0, 0, 0],
            [2, 0, 0, 0],
            [3, 0, 0, 0],
            [4, 0, 0, 0],
            [5, 0, 0, 0],
            [6, 0, 0, 0],
            [7, 0, 0, 0],
            [8, 0, 0, 0],
        ],
        epochs: [0, 74_240, 144_896, 194_048, 269_568, 364_032, 411_392, FAR_FUTURE_EPOCH, FAR_FUTURE_EPOCH],
    }
}

fn root(hex_str: &str) -> Root {
    hex::decode(hex_str.trim_start_matches("0x")).unwrap().try_into().unwrap()
}

fn address(hex_str: &str) -> [u8; 20] {
    hex::decode(hex_str.trim_start_matches("0x")).unwrap().try_into().unwrap()
}

const CHAIN_ID: u64 = 1;
const GENESIS_VALIDATORS_ROOT: &str = "0x4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95";
/// The checkpoint the instance trusts: the first slot of period 1906.
const BOOTSTRAP_ROOT: &str = "0xdab6b45fd34b9da264b15ad3e85dad586f836f3b11f5922e4c116b53574aefd3";
const FINALIZED_SLOT: u64 = 15_619_456;
/// The finalized execution block `n`, its parent `k` (with failed transactions), `m1 = n - 16`
/// and `m2 = m1 - 8191`.
const N: u64 = 26_379_842;
const N_HASH: &str = "0xa0a6c70dc174ba93e011f8990eda677686c1bdceee85e581c3b922223928cc27";
const K: u64 = 26_379_841;
const M1: u64 = 26_379_826;
const M2: u64 = 26_371_635;
const WETH: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
const USDC: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
const ABSENT: &str = "0x3e05d7d15f0d45265117bcd6e6682f138ee60e60";
/// `balanceOf[holder]` of WETH and USDC for the Uniswap v2 USDC/WETH pair, and an empty WETH slot.
const WETH_SLOT: &str = "0xb374801ace2c02f5db0425ab5920a2b7ed1d5a00abbcd395fda7530ba1d666c0";
const USDC_SLOT: &str = "0x28c0947f463362aab09a2d7896ccc847fb7e52633a16b978fb2caf3db1a9b8f8";
const EMPTY_SLOT: &str = "0x00000000000000000000000000000000000000000000000000000000000000ff";

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/mainnet-e2e-2026-11-09")
}

/// A fixture file, checked against the pinned `SHA256SUMS`.
fn fixture(name: &str) -> Vec<u8> {
    let bytes = fs::read(dir().join(name)).unwrap();
    let sums = fs::read_to_string(dir().join("SHA256SUMS")).unwrap();
    let want = sums.lines().find(|l| l.ends_with(&format!("  {name}"))).unwrap().split_whitespace().next().unwrap();
    assert_eq!(hex::encode(Sha256::digest(&bytes)), want, "{name} does not match SHA256SUMS");
    bytes
}

fn json(name: &str) -> Value {
    serde_json::from_slice::<Value>(&fixture(name)).unwrap()["result"].clone()
}

fn proof(name: &str) -> AccountProof {
    account_proof(&fixture(name))
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
        finality_branch: b[8..232].chunks_exact(32).map(|c| c.try_into().unwrap()).collect(),
        sync_aggregate: SyncAggregate::decode(&b[232..392], 512).unwrap(),
        signature_slot: u64::from_le_bytes(b[392..400].try_into().unwrap()),
    }
}

fn verified_store(bls: &dyn BlsBackend, adoption: Adoption) -> Result<FinalizedStore, Error> {
    let spec = mainnet();
    let bootstrap = LightClientBootstrap::decode(&fixture("bootstrap.ssz"), LcFork::Electra, 512).unwrap();
    let update = finality_update(&fixture("finality_update.ssz"));
    let mut store =
        FinalizedStore::initialize_with(&root(BOOTSTRAP_ROOT), &bootstrap, LcFork::Electra, &spec, bls, adoption)?;
    let ctx = Context { spec: &spec, genesis_validators_root: root(GENESIS_VALIDATORS_ROOT), bls };
    store.process_update(&update, update.signature_slot + 1, &ctx)?;
    Ok(store)
}

/// The record of block `n`, from the light client.
fn record_n() -> HeaderRecord {
    HeaderRecord::from_payload_header(
        &verified_store(&Blst::new(Aggregation::Auto), Adoption::OnChain).unwrap(),
        CHAIN_ID,
    )
    .unwrap()
}

fn storage_proof<'a>(p: &'a AccountProof, slot: &Root) -> &'a ferrata_spectests::execkit::StorageProof {
    p.storage.iter().find(|s| s.key == *slot).expect("slot in the eth_getProof response")
}

fn slot_of(n: u64) -> Root {
    let mut s = [0u8; 32];
    s[24..].copy_from_slice(&(n % HISTORY_SERVE_WINDOW).to_be_bytes());
    s
}

/// One EIP-2935 hop from `record` to block `m`, with the history proofs fetched at `record`'s block.
fn hop(record: &HeaderRecord, history: &str, m: u64) -> Result<HeaderRecord, Error> {
    let p = proof(history);
    let sp = storage_proof(&p, &slot_of(m));
    history_hop(record, &p.proof, &sp.proof, &fixture(&format!("header_{m}.rlp")))
}

#[test]
fn light_client_with_both_libraries_and_both_adoption_checks() {
    let canonical = json_root("beacon_header_15619456_second.json");
    for (name, bls) in
        [("blst", &Blst::new(Aggregation::Auto) as &dyn BlsBackend), ("zkcrypto", &Zkcrypto::new(Aggregation::Auto))]
    {
        for adoption in [Adoption::Full, Adoption::OnChain] {
            let store = verified_store(bls, adoption).unwrap_or_else(|e| panic!("{name} {adoption:?}: {e}"));
            let header = &store.core().finalized_header.beacon;
            assert_eq!((header.slot, header.hash_tree_root()), (FINALIZED_SLOT, canonical), "{name}");
        }
    }
}

fn json_root(name: &str) -> Root {
    let v: Value = serde_json::from_slice(&fixture(name)).unwrap();
    root(v["data"]["root"].as_str().unwrap())
}

#[test]
fn the_record_is_the_finalized_execution_block() {
    let record = record_n();
    let block = json(&format!("block_{N}.json"));
    let second = json(&format!("block_{N}_second.json"));
    assert_eq!((record.chain_id(), record.anchor_slot(), record.number()), (CHAIN_ID, FINALIZED_SLOT, N));
    assert_eq!(record.binding(), Binding::PayloadHeader);
    assert_eq!(record.block_hash(), root(N_HASH));
    for b in [&block, &second] {
        assert_eq!(record.block_hash(), root(b["hash"].as_str().unwrap()));
        assert_eq!(record.state_root(), root(b["stateRoot"].as_str().unwrap()));
        assert_eq!(record.receipts_root(), root(b["receiptsRoot"].as_str().unwrap()));
        assert_eq!(record.parent_hash(), root(b["parentHash"].as_str().unwrap()));
        assert_eq!(record.timestamp(), quantity(&b["timestamp"]));
    }
    // The RLP header (rebuilt by the fetch script, and again here) hashes to the bound block hash
    // and decodes to the record's fields.
    let rlp = fixture(&format!("header_{N}.rlp"));
    assert_eq!(header_rlp(&block), rlp);
    assert_eq!(keccak256(&rlp), record.block_hash());
    let h = decode_header(&rlp).unwrap();
    assert_eq!(h.shape, HeaderShape::Prague);
    assert_eq!(
        (h.number, h.state_root, h.receipts_root, h.parent_hash),
        (N, record.state_root(), record.receipts_root(), record.parent_hash())
    );
    assert_eq!(h.requests_hash, Some(root(block["requestsHash"].as_str().unwrap())));
    // The record survives its SSZ encoding.
    assert_eq!(HeaderRecord::from_ssz(&record.to_ssz()), Ok(record));
}

#[test]
fn accounts_and_storage_equal_json_rpc() {
    let record = record_n();
    let weth = proof(&format!("proof_weth_{N}.json"));
    let a = attest_account(&record, CHAIN_ID, &address(WETH), &weth.proof).unwrap();
    assert_eq!((a.fact.account.nonce, a.fact.account.balance), (weth.nonce, weth.balance));
    assert_eq!((a.fact.account.storage_root, a.fact.account.code_hash), (weth.storage_hash, weth.code_hash));
    assert_eq!(a.id, account_id(CHAIN_ID, &address(WETH), &a.fact.account));
    // The holder's balance slot: equal to the proof's value and to `balanceOf` via eth_call.
    let s = storage_proof(&weth, &root(WETH_SLOT));
    let v = attest_storage(&record, CHAIN_ID, &address(WETH), &weth.proof, &root(WETH_SLOT), &s.proof).unwrap();
    assert_eq!(v.fact.value, s.value);
    assert_eq!(v.fact.value, word(json(&format!("call_weth_balance_{N}.json")).as_str().unwrap()));
    assert_ne!(v.fact.value, [0; 32]);
    assert_eq!(v.id, storage_id(CHAIN_ID, &address(WETH), &root(WETH_SLOT), &v.fact.value));
    // An empty slot: a real exclusion proof.
    let e = storage_proof(&weth, &root(EMPTY_SLOT));
    let z = attest_storage(&record, CHAIN_ID, &address(WETH), &weth.proof, &root(EMPTY_SLOT), &e.proof).unwrap();
    assert_eq!((z.fact.value, e.value), ([0; 32], [0; 32]));
    // USDC (a proxy: the balance lives in the proxy's storage).
    let usdc = proof(&format!("proof_usdc_{N}.json"));
    let u = storage_proof(&usdc, &root(USDC_SLOT));
    let uv = attest_storage(&record, CHAIN_ID, &address(USDC), &usdc.proof, &root(USDC_SLOT), &u.proof).unwrap();
    assert_eq!(uv.fact.value, u.value);
    assert_eq!(uv.fact.value, word(json(&format!("call_usdc_balance_{N}.json")).as_str().unwrap()));
    // An address without an account: all-zero fields, as eth_getProof reports, and an empty
    // storage proven by the empty proof.
    let absent = proof(&format!("proof_absent_{N}.json"));
    let x = attest_account(&record, CHAIN_ID, &address(ABSENT), &absent.proof).unwrap();
    assert_eq!(x.fact.account, Account::ABSENT);
    assert_eq!((absent.nonce, absent.balance, absent.storage_hash, absent.code_hash), (0, [0; 32], [0; 32], [0; 32]));
    let slot0 = &absent.storage[0];
    assert!(slot0.proof.is_empty());
    let x0 = attest_storage(&record, CHAIN_ID, &address(ABSENT), &absent.proof, &slot0.key, &slot0.proof).unwrap();
    assert_eq!(x0.fact.value, [0; 32]);
    // The history contract holds the code EIP-2935 specifies.
    let history = proof(&format!("proof_history_{N}.json"));
    let h = attest_account(&record, CHAIN_ID, &HISTORY_STORAGE_ADDRESS, &history.proof).unwrap();
    assert_eq!(h.fact.account.code_hash, HISTORY_CODE_HASH);
    assert_eq!(keccak256(&hexb(json(&format!("code_history_{N}.json")).as_str().unwrap())), HISTORY_CODE_HASH);
}

#[test]
fn a_compound_statement_over_two_tokens_in_one_call() {
    let record = record_n();
    let weth = proof(&format!("proof_weth_{N}.json"));
    let usdc = proof(&format!("proof_usdc_{N}.json"));
    let (ws, us) = (storage_proof(&weth, &root(WETH_SLOT)), storage_proof(&usdc, &root(USDC_SLOT)));
    // "The pair holds at least 4,000 WETH and at least 9,000,000 USDC."
    let at_least = |n: u128| [Condition { field: Field::Value, predicate: Predicate { cmp: Cmp::Ge, rhs: u256(n) } }];
    let (weth_floor, usdc_floor) = (at_least(4_000 * 10u128.pow(18)), at_least(9_000_000 * 10u128.pow(6)));
    let wq = [SlotQuery { slot: root(WETH_SLOT), proof: ws.proof.as_slice(), conditions: &weth_floor }];
    let uq = [SlotQuery { slot: root(USDC_SLOT), proof: us.proof.as_slice(), conditions: &usdc_floor }];
    let accounts = [
        AccountQuery { address: address(WETH), proof: weth.proof.as_slice(), conditions: &[], slots: &wq },
        AccountQuery { address: address(USDC), proof: usdc.proof.as_slice(), conditions: &[], slots: &uq },
    ];
    let logs: [LogQuery<'_, Vec<u8>>; 0] = [];
    let facts = attest_batch(&record, CHAIN_ID, &Batch { accounts: &accounts, logs: &logs, relations: &[] }).unwrap();
    assert_eq!(facts.len(), 4);
    assert!(matches!(facts[1].fact, Fact::Storage(s) if s.value == ws.value));
    assert!(matches!(facts[3].fact, Fact::Storage(s) if s.value == us.value));
    // Raise one floor above the real balance: the whole statement fails.
    let too_high = at_least(10_000_000 * 10u128.pow(6));
    let uq2 = [SlotQuery { conditions: &too_high, ..uq[0] }];
    let accounts2 = [accounts[0], AccountQuery { slots: &uq2, ..accounts[1] }];
    assert_eq!(
        attest_batch(&record, CHAIN_ID, &Batch { accounts: &accounts2, logs: &logs, relations: &[] }),
        Err(Error::PredicateFailed)
    );
}

fn u256(n: u128) -> Root {
    let mut w = [0u8; 32];
    w[16..].copy_from_slice(&n.to_be_bytes());
    w
}

/// Every receipt of a block, its consensus encoding, and its trie (reference and alloy).
struct Block {
    receipts: Vec<Value>,
    trie: Trie,
    entries: BTreeMap<Vec<u8>, Vec<u8>>,
}

fn block_receipts(number: u64) -> Block {
    let receipts = json(&format!("receipts_{number}.json")).as_array().unwrap().clone();
    let entries: BTreeMap<Vec<u8>, Vec<u8>> = receipts
        .iter()
        .map(|r| {
            let (k, l) = encode_u64(quantity(&r["transactionIndex"]));
            (k[..l].to_vec(), receipt_rlp(r))
        })
        .collect();
    let trie = Trie::new(entries.iter().map(|(k, v)| (k.clone(), v.clone())));
    Block { receipts, trie, entries }
}

impl Block {
    /// The receipts root alloy-trie computes, and its proof of `tx_index` in `eth_getProof`'s
    /// convention.
    fn alloy(&self, tx_index: u64) -> (Root, Vec<Vec<u8>>) {
        let (k, l) = encode_u64(tx_index);
        let target = Nibbles::unpack(&k[..l]);
        let mut hb = HashBuilder::default().with_proof_retainer(ProofRetainer::new(vec![target]));
        for (key, value) in &self.entries {
            hb.add_leaf(Nibbles::unpack(key), value);
        }
        let root: B256 = hb.root();
        let proof = hb
            .take_proof_nodes()
            .matching_nodes_sorted(&target)
            .into_iter()
            .filter(|(path, node)| path.is_empty() || node.len() >= 32)
            .map(|(_, node)| node.to_vec())
            .collect();
        (root.0, proof)
    }

    fn proof(&self, tx_index: u64) -> Vec<Vec<u8>> {
        let (k, l) = encode_u64(tx_index);
        self.trie.proof(&k[..l])
    }
}

#[test]
fn every_log_of_the_finalized_block_is_attested() {
    let record = record_n();
    let block = block_receipts(N);
    assert_eq!(block.receipts.len(), 245);
    let (alloy_root, _) = block.alloy(0);
    assert_eq!(alloy_root, record.receipts_root(), "alloy-trie rebuilds the receipts root");
    assert_eq!(block.trie.root(), record.receipts_root(), "and so does the reference trie");
    let usdc = address(USDC);
    let (mut logs, mut types, mut usdc_transfers) = (0, BTreeMap::new(), 0);
    for r in &block.receipts {
        let i = quantity(&r["transactionIndex"]);
        let proof = block.proof(i);
        if i % 40 == 0 {
            assert_eq!(block.alloy(i).1, proof, "alloy and the reference trie give the same proof");
        }
        let receipt_value = &block.entries[&encode_u64(i).0[..encode_u64(i).1]];
        let receipt = Receipt::decode(receipt_value).unwrap();
        assert_eq!(u64::from(receipt.tx_type), quantity(&r["type"]));
        assert_eq!(
            (u64::from(receipt.status), receipt.cumulative_gas_used),
            (quantity(&r["status"]), quantity(&r["cumulativeGasUsed"]))
        );
        *types.entry(receipt.tx_type).or_insert(0) += 1;
        let json_logs = r["logs"].as_array().unwrap();
        assert_eq!(receipt.log_count(), json_logs.len());
        for (pos, jl) in json_logs.iter().enumerate() {
            let a = attest_log(&record, CHAIN_ID, i, pos as u64, &proof).unwrap();
            let log = a.fact.log;
            assert_eq!(log.address, address(jl["address"].as_str().unwrap()));
            let topics: Vec<Root> =
                jl["topics"].as_array().unwrap().iter().map(|t| root(t.as_str().unwrap())).collect();
            assert_eq!(log.topics(), topics.as_slice());
            assert_eq!(log.data, hexb(jl["data"].as_str().unwrap()).as_slice());
            assert_eq!(a.id, log_id(CHAIN_ID, N, i, pos as u64, &log));
            if log.address == usdc && log.topics().first() == Some(&TRANSFER) {
                usdc_transfers += 1;
                // The ERC-20 statement a consumer would check: emitter, event, recipient, amount.
                let conditions = [
                    Condition { field: Field::Emitter, predicate: Predicate { cmp: Cmp::Eq, rhs: left_pad(&usdc) } },
                    Condition { field: Field::Topic(0), predicate: Predicate { cmp: Cmp::Eq, rhs: TRANSFER } },
                    Condition { field: Field::Topic(2), predicate: Predicate { cmp: Cmp::Eq, rhs: topics[2] } },
                    Condition { field: Field::DataWord(0), predicate: Predicate { cmp: Cmp::Ge, rhs: [0; 32] } },
                ];
                assert_eq!(Fact::Log(a.fact).check(&conditions), Ok(()));
            }
            logs += 1;
        }
        // One past the last log.
        assert_eq!(
            attest_log(&record, CHAIN_ID, i, json_logs.len() as u64, &proof).unwrap_err(),
            Error::LogIndexOutOfRange
        );
    }
    assert_eq!(
        types.keys().copied().collect::<Vec<u8>>(),
        [0, 2, 3, 4],
        "legacy and EIP-1559, -4844 and -7702 receipts"
    );
    assert_eq!((logs, usdc_transfers), (870, 54));
    assert_eq!(types, BTreeMap::from([(0, 12), (2, 227), (3, 2), (4, 4)]));
    println!(
        "block {N}: {} receipts {types:?}, {logs} logs attested, {usdc_transfers} USDC transfers",
        block.receipts.len()
    );
    // No receipt past the last transaction.
    assert_eq!(attest_log(&record, CHAIN_ID, 245, 0, &block.proof(245)).unwrap_err(), Error::ReceiptNotIncluded);
}

fn left_pad(a: &[u8; 20]) -> Root {
    let mut w = [0u8; 32];
    w[12..].copy_from_slice(a);
    w
}

#[test]
fn history_hop_to_the_parent_block_and_its_failed_transactions() {
    let record = record_n();
    let parent = hop(&record, &format!("proof_history_{N}.json"), K).unwrap();
    assert_eq!(
        (parent.number(), parent.binding(), parent.anchor_slot()),
        (K, Binding::History { hops: 1 }, FINALIZED_SLOT)
    );
    assert_eq!(parent.block_hash(), record.parent_hash(), "n's parent hash, now proven through EIP-2935");
    let second = json(&format!("block_{K}_second.json"));
    assert_eq!(parent.receipts_root(), root(second["receiptsRoot"].as_str().unwrap()));
    let block = block_receipts(K);
    assert_eq!(block.trie.root(), parent.receipts_root());
    let failed: Vec<u64> = block
        .receipts
        .iter()
        .filter(|r| quantity(&r["status"]) == 0)
        .map(|r| quantity(&r["transactionIndex"]))
        .collect();
    assert_eq!(failed, [67, 142, 250]);
    for i in failed {
        assert_eq!(attest_log(&parent, CHAIN_ID, i, 0, &block.proof(i)).unwrap_err(), Error::ReceiptFailed, "tx {i}");
    }
    // A successful transaction of the same block attests.
    let ok = block
        .receipts
        .iter()
        .find(|r| quantity(&r["status"]) == 1 && !r["logs"].as_array().unwrap().is_empty())
        .unwrap();
    let i = quantity(&ok["transactionIndex"]);
    assert!(attest_log(&parent, CHAIN_ID, i, 0, &block.proof(i)).is_ok());
    // That receipt under the record of block n is refused.
    assert_eq!(attest_log(&record, CHAIN_ID, i, 0, &block.proof(i)).unwrap_err(), Error::ProofNodeMismatch);
}

#[test]
fn two_history_hops_with_storage_facts_under_each_record() {
    let record = record_n();
    let weth = address(WETH);
    let slot = root(WETH_SLOT);
    let fact_at = |r: &HeaderRecord, n: u64| {
        let p = proof(&format!("proof_weth_{n}.json"));
        let s = storage_proof(&p, &slot);
        let a = attest_storage(r, CHAIN_ID, &weth, &p.proof, &slot, &s.proof).unwrap();
        assert_eq!(a.fact.value, s.value, "block {n}: the proof's value");
        assert_eq!(
            a.fact.value,
            word(json(&format!("call_weth_balance_{n}.json")).as_str().unwrap()),
            "block {n}: balanceOf"
        );
        a
    };
    let at_n = fact_at(&record, N);
    // One hop: n to m1 = n - 16.
    let m1 = hop(&record, &format!("proof_history_{N}.json"), M1).unwrap();
    assert_eq!((m1.number(), m1.binding()), (M1, Binding::History { hops: 1 }));
    for b in [json(&format!("block_{M1}.json")), json(&format!("block_{M1}_second.json"))] {
        assert_eq!(m1.block_hash(), root(b["hash"].as_str().unwrap()));
        assert_eq!(m1.state_root(), root(b["stateRoot"].as_str().unwrap()));
    }
    let at_m1 = fact_at(&m1, M1);
    // The holder's balance did not change between m1 and n: one fact, one identifier.
    assert_eq!((at_m1.fact.value, at_m1.id), (at_n.fact.value, at_n.id));
    assert_ne!((at_m1.block_number, at_m1.block_hash), (at_n.block_number, at_n.block_hash));
    // Hops chain: m1 to m2 = m1 - 8191, the oldest block m1 reaches.
    let m2 = hop(&m1, &format!("proof_history_{M1}.json"), M2).unwrap();
    assert_eq!((m2.number(), m2.binding(), m2.anchor_slot()), (M2, Binding::History { hops: 2 }, FINALIZED_SLOT));
    assert_eq!(M1 - M2, HISTORY_SERVE_WINDOW);
    assert_eq!(m2.state_root(), root(json(&format!("block_{M2}_second.json"))["stateRoot"].as_str().unwrap()));
    let at_m2 = fact_at(&m2, M2);
    assert_ne!(at_m2.fact.value, at_n.fact.value);
    assert_ne!(at_m2.id, at_n.id);
    // The same history slot holds m1's hash at n and m2's hash at m1.
    assert_eq!(slot_of(M1), slot_of(M2));
    let at = |name: &str| storage_proof(&proof(name), &slot_of(M1)).value;
    assert_eq!(at(&format!("proof_history_{N}.json")), m1.block_hash());
    assert_eq!(at(&format!("proof_history_{M1}.json")), m2.block_hash());
    // One block further back is outside m1's window (refused before any proof is read), and m2
    // is outside n's.
    let p = proof(&format!("proof_history_{M1}.json"));
    let older = fixture(&format!("header_{}.rlp", M2 - 1));
    assert_eq!(
        history_hop(&m1, &p.proof, &storage_proof(&p, &slot_of(M2)).proof, &older),
        Err(Error::HistoryOutOfWindow)
    );
    assert_eq!(hop(&record, &format!("proof_history_{N}.json"), M2), Err(Error::HistoryOutOfWindow));
}

#[test]
fn tampering_with_any_kind_of_fixture_byte_is_refused() {
    let blst = Blst::new(Aggregation::Auto);
    let spec = mainnet();
    let ctx = Context { spec: &spec, genesis_validators_root: root(GENESIS_VALIDATORS_ROOT), bls: &blst };
    let bootstrap = LightClientBootstrap::decode(&fixture("bootstrap.ssz"), LcFork::Electra, 512).unwrap();
    let update = finality_update(&fixture("finality_update.ssz"));
    let fresh =
        || FinalizedStore::initialize(&root(BOOTSTRAP_ROOT), &bootstrap, LcFork::Electra, &spec, &blst).unwrap();
    // Light-client data: the bootstrap's committee and header, the update's finalized execution
    // payload, finality branch, signer bits and signature.
    let mut b = bootstrap.clone();
    b.current_sync_committee.pubkeys[7][5] ^= 1;
    assert_eq!(
        FinalizedStore::initialize(&root(BOOTSTRAP_ROOT), &b, LcFork::Electra, &spec, &blst).unwrap_err(),
        Error::CurrentCommitteeBranchInvalid
    );
    let mut b = bootstrap.clone();
    b.header.beacon.proposer_index += 1;
    assert_eq!(
        FinalizedStore::initialize(&root(BOOTSTRAP_ROOT), &b, LcFork::Electra, &spec, &blst).unwrap_err(),
        Error::CheckpointMismatch
    );
    let refuse = |u: &LightClientUpdate| fresh().process_update(u, u.signature_slot + 1, &ctx).unwrap_err();
    let mut u = update.clone();
    if let ferrata_core::types::Execution::Payload { header, .. } = &mut u.finalized_header.execution {
        header.state_root[0] ^= 1;
    }
    assert_eq!(refuse(&u), Error::FinalizedHeaderInvalid, "finalized state root");
    let mut u = update.clone();
    u.finality_branch[3][0] ^= 1;
    assert_eq!(refuse(&u), Error::FinalityBranchInvalid);
    let mut u = update.clone();
    u.sync_aggregate.bits[100] = !u.sync_aggregate.bits[100];
    assert_eq!(refuse(&u), Error::SignatureInvalid, "signer bit");
    let mut u = update.clone();
    u.sync_aggregate.signature[50] ^= 1;
    assert!(matches!(refuse(&u), Error::SignatureInvalid | Error::SignatureMalformed), "signature");

    let record = record_n();
    // The record's roots.
    let weth = proof(&format!("proof_weth_{N}.json"));
    let ws = storage_proof(&weth, &root(WETH_SLOT));
    let mut bytes = record.to_ssz();
    bytes[96] ^= 1; // state_root
    let tampered = HeaderRecord::from_ssz(&bytes).unwrap();
    let storage = |r: &HeaderRecord, ap: &[Vec<u8>], sp: &[Vec<u8>]| {
        attest_storage(r, CHAIN_ID, &address(WETH), ap, &root(WETH_SLOT), sp)
    };
    assert_eq!(storage(&tampered, &weth.proof, &ws.proof).unwrap_err(), Error::ProofNodeMismatch, "record state root");
    // Account and storage proof nodes, at every depth; truncated; extended.
    for depth in 0..weth.proof.len() {
        let mut p = weth.proof.clone();
        p[depth][10] ^= 1;
        assert_eq!(
            storage(&record, &p, &ws.proof).unwrap_err(),
            Error::ProofNodeMismatch,
            "account proof depth {depth}"
        );
    }
    for depth in 0..ws.proof.len() {
        let mut p = ws.proof.clone();
        p[depth][10] ^= 1;
        assert_eq!(
            storage(&record, &weth.proof, &p).unwrap_err(),
            Error::ProofNodeMismatch,
            "storage proof depth {depth}"
        );
    }
    assert_eq!(storage(&record, &weth.proof[..weth.proof.len() - 1], &ws.proof).unwrap_err(), Error::ProofIncomplete);
    assert_eq!(storage(&record, &weth.proof, &ws.proof[..ws.proof.len() - 1]).unwrap_err(), Error::ProofIncomplete);
    let mut longer = ws.proof.clone();
    longer.push(ws.proof[0].clone());
    assert_eq!(storage(&record, &weth.proof, &longer).unwrap_err(), Error::ProofTrailingNodes);
    // The USDC storage proof replayed under WETH's account.
    let usdc = proof(&format!("proof_usdc_{N}.json"));
    let us = storage_proof(&usdc, &root(USDC_SLOT));
    assert_eq!(storage(&record, &weth.proof, &us.proof).unwrap_err(), Error::ProofNodeMismatch);
    // A WETH proof from block m1 under block n's record.
    let weth_m1 = proof(&format!("proof_weth_{M1}.json"));
    assert_eq!(storage(&record, &weth_m1.proof, &ws.proof).unwrap_err(), Error::ProofNodeMismatch);
    // Receipt proof bytes: a byte of the receipt itself (the leaf).
    let block = block_receipts(N);
    let i = 138;
    let mut p = block.proof(i);
    let last = p.len() - 1;
    let at = p[last].len() - 5;
    p[last][at] ^= 1;
    assert_eq!(attest_log(&record, CHAIN_ID, i, 0, &p).unwrap_err(), Error::ProofNodeMismatch, "receipt bytes");
    // The RLP header of a hop: a byte of its state root.
    let history = proof(&format!("proof_history_{N}.json"));
    let hs = storage_proof(&history, &slot_of(M1));
    let mut header = fixture(&format!("header_{M1}.rlp"));
    let state_root_at = header
        .windows(32)
        .position(|w| w == json(&format!("block_{M1}.json"))["stateRoot"].as_str().map(root).unwrap())
        .unwrap();
    header[state_root_at] ^= 1;
    assert_eq!(
        history_hop(&record, &history.proof, &hs.proof, &header),
        Err(Error::HistoryHashMismatch),
        "header bytes"
    );
    // History proof nodes.
    let mut p = hs.proof.clone();
    p[1][7] ^= 1;
    assert_eq!(
        history_hop(&record, &history.proof, &p, &fixture(&format!("header_{M1}.rlp"))),
        Err(Error::ProofNodeMismatch)
    );
    // Another chain's consumer.
    assert_eq!(storage(&record, &weth.proof, &ws.proof).map(|_| ()), Ok(()));
    assert_eq!(
        attest_storage(&record, 11155111, &address(WETH), &weth.proof, &root(WETH_SLOT), &ws.proof).unwrap_err(),
        Error::ChainMismatch
    );
}

#[test]
fn block_wide_log_indices_convert_to_receipt_positions() {
    use ferrata_core::exec::{block_log_index, locate_log};
    let record = record_n();
    let block = block_receipts(N);
    let counts: Vec<u64> = block.receipts.iter().map(|r| r["logs"].as_array().unwrap().len() as u64).collect();
    let total: u64 = counts.iter().sum();
    assert_eq!(total, 870);
    for r in &block.receipts {
        let tx = quantity(&r["transactionIndex"]);
        for (position, l) in r["logs"].as_array().unwrap().iter().enumerate() {
            let block_wide = quantity(&l["logIndex"]);
            assert_eq!(locate_log(&counts, block_wide), Some((tx, position as u64)), "logIndex {block_wide}");
            assert_eq!(block_log_index(&counts, tx, position as u64), Some(block_wide));
        }
    }
    // The first log of the block's largest receipt, located from JSON-RPC's logIndex and attested.
    let largest = block.receipts.iter().max_by_key(|r| r["logs"].as_array().unwrap().len()).unwrap();
    let l = &largest["logs"][0];
    let (tx, position) = locate_log(&counts, quantity(&l["logIndex"])).unwrap();
    let proof = block.proof(tx);
    let a = attest_log(&record, CHAIN_ID, tx, position, &proof).unwrap();
    assert_eq!(a.fact.log.data, hexb(l["data"].as_str().unwrap()).as_slice());
    // Past the last log, past the last transaction, past a receipt's logs.
    assert_eq!(locate_log(&counts, total), None);
    assert_eq!(block_log_index(&counts, counts.len() as u64, 0), None);
    let empty = counts.iter().position(|c| *c == 0).unwrap() as u64;
    assert_eq!(block_log_index(&counts, empty, 0), None);
    assert_eq!(block_log_index(&counts, tx, counts[tx as usize]), None);
}

#[test]
fn the_same_balance_at_two_blocks_under_each_identifier() {
    let record = record_n();
    let m1 = hop(&record, &format!("proof_history_{N}.json"), M1).unwrap();
    let slot = root(WETH_SLOT);
    let at = |r: &HeaderRecord, n: u64| {
        let p = proof(&format!("proof_weth_{n}.json"));
        let s = storage_proof(&p, &slot);
        attest_storage(r, CHAIN_ID, &address(WETH), &p.proof, &slot, &s.proof).unwrap()
    };
    let (a, b) = (at(&record, N), at(&m1, M1));
    assert_eq!(a.fact.value, b.fact.value);
    assert_eq!(a.id, b.id, "the default identifier names the value");
    assert_ne!(a.id_at_block(), b.id_at_block(), "the block-bound one names the value at a block");
}

#[test]
fn a_relation_between_two_real_balances() {
    use ferrata_core::attest::{Operand, Relation};
    let record = record_n();
    let weth = proof(&format!("proof_weth_{N}.json"));
    let usdc = proof(&format!("proof_usdc_{N}.json"));
    let (ws, us) = (storage_proof(&weth, &root(WETH_SLOT)), storage_proof(&usdc, &root(USDC_SLOT)));
    let wq = [SlotQuery { slot: root(WETH_SLOT), proof: ws.proof.as_slice(), conditions: &[] }];
    let uq = [SlotQuery { slot: root(USDC_SLOT), proof: us.proof.as_slice(), conditions: &[] }];
    let accounts = [
        AccountQuery { address: address(WETH), proof: weth.proof.as_slice(), conditions: &[], slots: &wq },
        AccountQuery { address: address(USDC), proof: usdc.proof.as_slice(), conditions: &[], slots: &uq },
    ];
    let logs: [LogQuery<'_, Vec<u8>>; 0] = [];
    // Facts: 0 WETH account, 1 the pair's WETH balance, 2 USDC account, 3 the pair's USDC balance.
    // In raw units (wei against 10^-6 USDC) the WETH balance is the larger.
    let relation = |cmp| {
        [Relation {
            left: Operand { fact: 1, field: Field::Value },
            cmp,
            right: Operand { fact: 3, field: Field::Value },
        }]
    };
    let gt = relation(Cmp::Gt);
    assert!(attest_batch(&record, CHAIN_ID, &Batch { accounts: &accounts, logs: &logs, relations: &gt }).is_ok());
    let lt = relation(Cmp::Lt);
    assert_eq!(
        attest_batch(&record, CHAIN_ID, &Batch { accounts: &accounts, logs: &logs, relations: &lt }),
        Err(Error::PredicateFailed)
    );
}
