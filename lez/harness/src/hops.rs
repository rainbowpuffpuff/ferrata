//! Chains of EIP-2935 hops on real data, for `tests/history_hops.rs`.
//!
//! The fixtures (`crates/ferrata-spectests/fixtures/sepolia-hops-2026-11-18` and
//! `mainnet-hops-2026-11-18`, fetched on 18 November 2026 by `lez/scripts/fetch-hop-chain.py`; each
//! SOURCE.md lists every request) hold chains of hops from the block of a record the program's
//! instance wrote itself in the other fixtures (Sepolia: the vault fixture's Gloas record of block
//! 12,112,773; mainnet: the attestation fixture's record of block 26,384,851): the history
//! contract's `eth_getProof` at every block a hop starts from and every block's RLP header; at a
//! chain's end block, receipts and a storage value. A hop's proofs are in the state of the block
//! the hop starts from, so a chain of `k` hops reaches `8,191 k` blocks back at most.

use std::path::PathBuf;

use ferrata_lez_wire::attest::{AccountQuery, Address, Hop, Word};
use ferrata_core::rlp::encode_u64;
use ferrata_spectests::execkit::{account_proof, hexb, receipt_rlp, AccountProof, Trie};
use serde_json::Value;

use crate::{
    attest::{account_query, u256},
    sepolia::pinned,
};

/// EIP-2935's `HISTORY_SERVE_WINDOW`: one hop reaches at most this many blocks back.
pub const WINDOW: u64 = 8191;

/// The Sepolia chains' directory under `crates/ferrata-spectests/fixtures`.
pub const SEPOLIA_HOPS: &str = "sepolia-hops-2026-11-18";
/// The mainnet chain's directory under `crates/ferrata-spectests/fixtures`.
pub const MAINNET_HOPS: &str = "mainnet-hops-2026-11-18";
const SEPOLIA_SUMS: &str = include_str!("../../../crates/ferrata-spectests/fixtures/sepolia-hops-2026-11-18/SHA256SUMS");
const MAINNET_SUMS: &str = include_str!("../../../crates/ferrata-spectests/fixtures/mainnet-hops-2026-11-18/SHA256SUMS");

/// A hop-chain fixture.
pub struct HopFixture {
    pub dir: PathBuf,
    sums: &'static str,
    /// `fixture.json`.
    pub m: Value,
    /// The record's block, where every chain starts.
    pub record: u64,
}

impl HopFixture {
    fn load(name: &str, sums: &'static str) -> Self {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/ferrata-spectests/fixtures").join(name);
        let m: Value = serde_json::from_slice(&pinned(dir.clone(), sums, "fixture.json")).unwrap();
        let record = m["record"].as_u64().unwrap();
        assert_eq!(m["window"].as_u64(), Some(WINDOW));
        Self { dir, sums, m, record }
    }

    pub fn sepolia() -> Self {
        Self::load(SEPOLIA_HOPS, SEPOLIA_SUMS)
    }

    pub fn mainnet() -> Self {
        Self::load(MAINNET_HOPS, MAINNET_SUMS)
    }

    /// A file of the fixture, checked against its SHA-256 sum compiled into the harness.
    pub fn file(&self, name: &str) -> Vec<u8> {
        pinned(self.dir.clone(), self.sums, name)
    }

    /// Chain `i` of `fixture.json`: the blocks it reaches, newest first, from the record's block.
    pub fn chain(&self, i: usize) -> Vec<u64> {
        self.m["chains"][i].as_array().unwrap().iter().map(|n| n.as_u64().unwrap()).collect()
    }

    /// The history contract's `eth_getProof` at `block`.
    pub fn history(&self, block: u64) -> AccountProof {
        account_proof(&self.file(&format!("proof_history_{block}.json")))
    }

    /// Block `n`'s RLP header.
    pub fn header(&self, n: u64) -> Vec<u8> {
        self.file(&format!("header_{n}.rlp"))
    }

    /// The hop from block `from` to block `to`: the history contract's account proof and the proof
    /// of slot `to mod 8191`, both in `from`'s state, and `to`'s header.
    pub fn hop(&self, from: u64, to: u64) -> Hop {
        self.hop_with_proofs_at(from, to)
    }

    /// The hop to `to` with the history contract's proofs taken in the state of `at` (a broken
    /// link when `at` is not the block the hop starts from).
    pub fn hop_with_proofs_at(&self, at: u64, to: u64) -> Hop {
        let history = self.history(at);
        let key = u256(u128::from(to % WINDOW));
        let slot = history.storage.iter().find(|s| s.key == key).expect("the slot of the block hopped to");
        Hop { account_proof: history.proof.clone(), storage_proof: slot.proof.clone(), header: self.header(to) }
    }

    /// The hops along `chain` (newest first) from the record's block.
    pub fn hops(&self, chain: &[u64]) -> Vec<Hop> {
        let mut from = self.record;
        chain
            .iter()
            .map(|&to| {
                let hop = self.hop(from, to);
                from = to;
                hop
            })
            .collect()
    }

    /// The history contract's account at `block`, as a query (its proof is the one the next hop
    /// from `block` starts with): a fact of the same size at every block of a chain.
    pub fn history_account(&self, block: u64) -> AccountQuery {
        account_query(&self.history(block), vec![], vec![])
    }

    /// `eth_getBlockReceipts` of `block`.
    pub fn receipts(&self, block: u64) -> Vec<Value> {
        let served: Value = serde_json::from_slice(&self.file(&format!("receipts_{block}.json"))).unwrap();
        served["result"].as_array().expect("receipts").clone()
    }

    /// The proof of transaction `tx_index`'s receipt in `block`, from the block's receipts trie
    /// rebuilt from `eth_getBlockReceipts`.
    pub fn receipt_proof(&self, block: u64, tx_index: u64) -> Vec<Vec<u8>> {
        let receipts = self.receipts(block);
        let trie = Trie::new(receipts.iter().enumerate().map(|(i, r)| {
            let (key, len) = encode_u64(i as u64);
            (key[..len].to_vec(), receipt_rlp(r))
        }));
        let (key, len) = encode_u64(tx_index);
        trie.proof(&key[..len])
    }

    /// The ERC-20 `Transfer` the script chose in `block`: transaction index, position in the
    /// receipt, and the log as `eth_getBlockReceipts` gives it.
    pub fn transfer(&self, block: u64) -> (u64, u64, Value) {
        let t = &self.m["transfers"][block.to_string()];
        let (tx, i) = (t["tx_index"].as_u64().unwrap(), t["log_index"].as_u64().unwrap());
        let log = self.receipts(block)[tx as usize]["logs"][i as usize].clone();
        assert_eq!(log["logIndex"].as_str().map(|s| u64::from_str_radix(&s[2..], 16).unwrap()), t["block_log_index"].as_u64());
        (tx, i, log)
    }

    /// Storage value `i` of `fixture.json`: its block, the account's proof, the slot and the value.
    pub fn storage(&self, i: usize) -> (u64, AccountProof, Word, Word) {
        let s = &self.m["storage"][i];
        let block = s["block"].as_u64().unwrap();
        let address: Address = hexb(s["address"].as_str().unwrap()).try_into().unwrap();
        let proof = account_proof(&self.file(&format!("proof_0x{}_{block}.json", hex::encode(address))));
        let slot: Word = hexb(s["slot"].as_str().unwrap()).try_into().unwrap();
        let value: Word = hexb(s["value"].as_str().unwrap()).try_into().unwrap();
        (block, proof, slot, value)
    }

    /// The block hash both providers gave for block `n`.
    pub fn block_hash(&self, n: u64) -> [u8; 32] {
        hexb(self.m["headers"][n.to_string()].as_str().unwrap()).try_into().unwrap()
    }
}
