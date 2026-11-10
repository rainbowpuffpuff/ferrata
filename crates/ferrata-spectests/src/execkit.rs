//! Building blocks for the execution-layer tests: a deterministic PRNG, an RLP encoder, a
//! reference Merkle-Patricia trie that writes proofs in `eth_getProof`'s format, a helper that
//! re-links a modified proof to a new root, and readers for the JSON-RPC fixtures.
//!
//! The reference trie is independent of `ferrata-core`: it hashes with RustCrypto's `sha3`
//! Keccak-256 and builds nodes from the Yellow Paper's definition (Appendix D, `c(I, i)`), so it
//! can stand beside the verifier as a second implementation. Its roots are checked against the
//! official `TrieTests` and against `alloy-trie`.

use serde_json::Value;
use sha3::{Digest, Keccak256};

/// Keccak-256 (RustCrypto `sha3`, independent of the core's `tiny-keccak`).
pub fn keccak(data: &[u8]) -> [u8; 32] {
    Keccak256::digest(data).into()
}

/// SplitMix64: a small deterministic generator, so every random test is reproducible.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    /// Uniform in `0..n` (n > 0).
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
    pub fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next_u64() as u8).collect()
    }
}

/// RLP encoding (canonical).
pub mod enc {
    fn length(len: usize, offset: u8) -> Vec<u8> {
        if len < 56 {
            return vec![offset + len as u8];
        }
        let be: Vec<u8> = (len as u64).to_be_bytes().into_iter().skip_while(|b| *b == 0).collect();
        let mut out = vec![offset + 55 + be.len() as u8];
        out.extend(be);
        out
    }
    /// A byte string.
    pub fn bytes(b: &[u8]) -> Vec<u8> {
        if b.len() == 1 && b[0] < 0x80 {
            return b.to_vec();
        }
        let mut out = length(b.len(), 0x80);
        out.extend_from_slice(b);
        out
    }
    /// A list of already encoded items.
    pub fn list(items: &[Vec<u8>]) -> Vec<u8> {
        let payload: Vec<u8> = items.concat();
        let mut out = length(payload.len(), 0xc0);
        out.extend(payload);
        out
    }
    /// An unsigned integer given by its big-endian bytes (leading zeros are stripped).
    pub fn uint(be: &[u8]) -> Vec<u8> {
        let start = be.iter().position(|b| *b != 0).unwrap_or(be.len());
        bytes(&be[start..])
    }
    pub fn u64(n: u64) -> Vec<u8> {
        uint(&n.to_be_bytes())
    }
}

/// The nibbles of `key`.
pub fn nibbles(key: &[u8]) -> Vec<u8> {
    key.iter().flat_map(|b| [b >> 4, b & 0x0f]).collect()
}

/// Hex-prefix encoding (Yellow Paper, Appendix C).
pub fn hex_prefix(nibbles: &[u8], leaf: bool) -> Vec<u8> {
    let odd = nibbles.len() % 2 == 1;
    let flag = (if leaf { 2 } else { 0 }) + u8::from(odd);
    let (mut out, rest) =
        if odd { (vec![(flag << 4) | nibbles[0]], &nibbles[1..]) } else { (vec![flag << 4], nibbles) };
    out.extend(rest.chunks(2).map(|p| (p[0] << 4) | p[1]));
    out
}

/// A trie node with its encoding.
pub struct Built {
    pub node: Node,
    pub enc: Vec<u8>,
}

pub enum Node {
    Leaf { path: Vec<u8> },
    Extension { path: Vec<u8>, child: Box<Built> },
    Branch { children: Vec<Option<Built>>, has_value: bool },
}

impl Built {
    /// How a parent refers to this node: embedded below 32 bytes, else by hash.
    pub fn reference(&self) -> Vec<u8> {
        if self.enc.len() < 32 {
            self.enc.clone()
        } else {
            enc::bytes(&keccak(&self.enc))
        }
    }
}

/// `c(I, i)` for a non-empty, sorted set of (nibble key, value) whose keys agree up to `i`.
fn build(entries: &[(Vec<u8>, Vec<u8>)], i: usize) -> Built {
    if entries.len() == 1 {
        let (key, value) = &entries[0];
        let path = key[i..].to_vec();
        let enc = enc::list(&[enc::bytes(&hex_prefix(&path, true)), enc::bytes(value)]);
        return Built { node: Node::Leaf { path }, enc };
    }
    let (first, last) = (&entries[0].0, &entries[entries.len() - 1].0);
    let mut l = 0;
    while i + l < first.len() && i + l < last.len() && first[i + l] == last[i + l] {
        l += 1;
    }
    if l > 0 {
        let path = first[i..i + l].to_vec();
        let child = build(entries, i + l);
        let enc = enc::list(&[enc::bytes(&hex_prefix(&path, false)), child.reference()]);
        return Built { node: Node::Extension { path, child: Box::new(child) }, enc };
    }
    // Sorted: a key that ends here comes first and is the branch's value.
    let has_value = first.len() == i;
    let rest = if has_value { &entries[1..] } else { entries };
    let mut items = Vec::new();
    let mut children = Vec::new();
    for nibble in 0..16u8 {
        let sub: Vec<(Vec<u8>, Vec<u8>)> = rest.iter().filter(|(k, _)| k[i] == nibble).cloned().collect();
        if sub.is_empty() {
            items.push(enc::bytes(&[]));
            children.push(None);
        } else {
            let child = build(&sub, i + 1);
            items.push(child.reference());
            children.push(Some(child));
        }
    }
    items.push(enc::bytes(if has_value { &entries[0].1 } else { &[] }));
    Built { node: Node::Branch { children, has_value }, enc: enc::list(&items) }
}

/// A reference trie over byte keys.
pub struct Trie {
    entries: Vec<(Vec<u8>, Vec<u8>)>,
    root: Option<Built>,
}

impl Trie {
    /// The trie holding `pairs` (keys in bytes). Later pairs replace earlier ones with the same
    /// key, and an empty value deletes the key, as in the `TrieTests`.
    pub fn new<I: IntoIterator<Item = (Vec<u8>, Vec<u8>)>>(pairs: I) -> Self {
        let mut map = std::collections::BTreeMap::new();
        for (k, v) in pairs {
            if v.is_empty() {
                map.remove(&nibbles(&k));
            } else {
                map.insert(nibbles(&k), v);
            }
        }
        let entries: Vec<(Vec<u8>, Vec<u8>)> = map.into_iter().collect();
        let root = (!entries.is_empty()).then(|| build(&entries, 0));
        Trie { entries, root }
    }

    pub fn root(&self) -> [u8; 32] {
        keccak(self.root.as_ref().map_or(&[0x80][..], |r| &r.enc))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The value at `key`, if any.
    pub fn get(&self, key: &[u8]) -> Option<&[u8]> {
        let k = nibbles(key);
        self.entries.iter().find(|(e, _)| *e == k).map(|(_, v)| v.as_slice())
    }

    /// The keys (in bytes; every key here has an even number of nibbles).
    pub fn keys(&self) -> Vec<Vec<u8>> {
        self.entries.iter().map(|(k, _)| k.chunks(2).map(|p| (p[0] << 4) | p[1]).collect()).collect()
    }

    /// The proof of `key` in `eth_getProof`'s format: the root, then each node on the key's path
    /// that its parent references by hash.
    pub fn proof(&self, key: &[u8]) -> Vec<Vec<u8>> {
        let k = nibbles(key);
        let mut out = Vec::new();
        let Some(mut node) = self.root.as_ref() else { return out };
        out.push(node.enc.clone());
        let mut i = 0;
        loop {
            let next = match &node.node {
                Node::Leaf { .. } => None,
                Node::Extension { path, child } => {
                    let follows = k.len() >= i + path.len() && k[i..i + path.len()] == path[..];
                    i += path.len();
                    follows.then_some(child.as_ref())
                }
                Node::Branch { children, .. } => {
                    let child = k.get(i).and_then(|n| children[usize::from(*n)].as_ref());
                    i += 1;
                    child
                }
            };
            match next {
                None => return out,
                Some(child) => {
                    if child.enc.len() >= 32 {
                        out.push(child.enc.clone());
                    }
                    node = child;
                }
            }
        }
    }
}

/// Replace `proof[index]` with `node` and update every ancestor's hash reference, so the proof
/// is consistent again; returns the new root. Used to forge nodes that pass the hash check, to
/// test the structural checks behind it.
pub fn relink(proof: &mut [Vec<u8>], index: usize, node: Vec<u8>) -> [u8; 32] {
    let mut old = keccak(&proof[index]);
    proof[index] = node;
    let mut new = keccak(&proof[index]);
    for parent in (0..index).rev() {
        let before = keccak(&proof[parent]);
        let p = &mut proof[parent];
        let at = p.windows(32).position(|w| w == old).expect("parent references the node by hash");
        p[at..at + 32].copy_from_slice(&new);
        old = before;
        new = keccak(&proof[parent]);
    }
    keccak(&proof[0])
}

/// Bytes from `0x`-prefixed hex.
pub fn hexb(s: &str) -> Vec<u8> {
    hex::decode(s.trim_start_matches("0x")).expect("hex")
}

/// A 32-byte word from `0x`-prefixed hex of at most 32 bytes (left-padded), as JSON-RPC
/// quantities are written.
pub fn word(s: &str) -> [u8; 32] {
    let digits = s.trim_start_matches("0x");
    let padded = format!("{digits:0>64}");
    hex::decode(padded).expect("hex").try_into().expect("at most 32 bytes")
}

/// A JSON-RPC quantity.
pub fn quantity(v: &Value) -> u64 {
    u64::from_str_radix(v.as_str().expect("string").trim_start_matches("0x"), 16).expect("quantity")
}

/// One `storageProof` entry of `eth_getProof`.
pub struct StorageProof {
    pub key: [u8; 32],
    pub value: [u8; 32],
    pub proof: Vec<Vec<u8>>,
}

/// The result of `eth_getProof`.
pub struct AccountProof {
    pub address: [u8; 20],
    pub nonce: u64,
    pub balance: [u8; 32],
    pub storage_hash: [u8; 32],
    pub code_hash: [u8; 32],
    pub proof: Vec<Vec<u8>>,
    pub storage: Vec<StorageProof>,
}

fn nodes(v: &Value) -> Vec<Vec<u8>> {
    v.as_array().expect("array").iter().map(|n| hexb(n.as_str().expect("hex"))).collect()
}

/// Read an `eth_getProof` response (`{"jsonrpc", "id", "result"}`).
pub fn account_proof(response: &[u8]) -> AccountProof {
    let v: Value = serde_json::from_slice(response).expect("json");
    let r = &v["result"];
    AccountProof {
        address: hexb(r["address"].as_str().unwrap()).try_into().unwrap(),
        nonce: quantity(&r["nonce"]),
        balance: word(r["balance"].as_str().unwrap()),
        storage_hash: word(r["storageHash"].as_str().unwrap()),
        code_hash: word(r["codeHash"].as_str().unwrap()),
        proof: nodes(&r["accountProof"]),
        storage: r["storageProof"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| StorageProof {
                key: word(s["key"].as_str().unwrap()),
                value: word(s["value"].as_str().unwrap()),
                proof: nodes(&s["proof"]),
            })
            .collect(),
    }
}

/// The consensus encoding of one `eth_getBlockReceipts` entry: `[status, cumulative_gas_used,
/// logs_bloom, logs]`, prefixed by the type byte unless legacy.
pub fn receipt_rlp(r: &Value) -> Vec<u8> {
    let logs: Vec<Vec<u8>> = r["logs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            let topics: Vec<Vec<u8>> =
                l["topics"].as_array().unwrap().iter().map(|t| enc::bytes(&hexb(t.as_str().unwrap()))).collect();
            enc::list(&[
                enc::bytes(&hexb(l["address"].as_str().unwrap())),
                enc::list(&topics),
                enc::bytes(&hexb(l["data"].as_str().unwrap())),
            ])
        })
        .collect();
    let body = enc::list(&[
        enc::u64(quantity(&r["status"])),
        enc::u64(quantity(&r["cumulativeGasUsed"])),
        enc::bytes(&hexb(r["logsBloom"].as_str().unwrap())),
        enc::list(&logs),
    ]);
    match quantity(&r["type"]) {
        0 => body,
        t => [vec![t as u8], body].concat(),
    }
}

/// The RLP header of an `eth_getBlockByNumber` result, from its fields in header order.
pub fn header_rlp(block: &Value) -> Vec<u8> {
    const FIELDS: [(&str, bool); 23] = [
        ("parentHash", false),
        ("sha3Uncles", false),
        ("miner", false),
        ("stateRoot", false),
        ("transactionsRoot", false),
        ("receiptsRoot", false),
        ("logsBloom", false),
        ("difficulty", true),
        ("number", true),
        ("gasLimit", true),
        ("gasUsed", true),
        ("timestamp", true),
        ("extraData", false),
        ("mixHash", false),
        ("nonce", false),
        ("baseFeePerGas", true),
        ("withdrawalsRoot", false),
        ("blobGasUsed", true),
        ("excessBlobGas", true),
        ("parentBeaconBlockRoot", false),
        ("requestsHash", false),
        ("blockAccessListHash", false),
        ("slotNumber", true),
    ];
    let items: Vec<Vec<u8>> = FIELDS
        .iter()
        .map_while(|(name, int)| {
            let s = block.get(*name)?.as_str()?;
            Some(if *int { enc::uint(&word(s)) } else { enc::bytes(&hexb(s)) })
        })
        .collect();
    enc::list(&items)
}

/// The RLP of an account.
pub fn account_rlp(nonce: u64, balance: u128, storage_root: [u8; 32], code_hash: [u8; 32]) -> Vec<u8> {
    enc::list(&[enc::u64(nonce), enc::uint(&balance.to_be_bytes()), enc::bytes(&storage_root), enc::bytes(&code_hash)])
}

/// The RLP of a log.
pub fn log_rlp(address: [u8; 20], topics: &[[u8; 32]], data: &[u8]) -> Vec<u8> {
    let topics: Vec<Vec<u8>> = topics.iter().map(|t| enc::bytes(t)).collect();
    enc::list(&[enc::bytes(&address), enc::list(&topics), enc::bytes(data)])
}

/// The consensus encoding of a receipt with the given status item (an encoded integer, or a
/// 32-byte post-state root for pre-Byzantium receipts).
pub fn receipt_with(tx_type: u8, status: Vec<u8>, cumulative_gas_used: u64, logs: &[Vec<u8>]) -> Vec<u8> {
    let body = enc::list(&[status, enc::u64(cumulative_gas_used), enc::bytes(&[0u8; 256]), enc::list(logs)]);
    if tx_type == 0 {
        body
    } else {
        [vec![tx_type], body].concat()
    }
}

/// The header items of a block (20 for Cancun, 21 for Prague/Osaka, 23 for Amsterdam, or any
/// other count for refusal tests).
pub fn header_items(number: u64, state_root: [u8; 32], receipts_root: [u8; 32], fields: usize) -> Vec<Vec<u8>> {
    let mut items = vec![
        enc::bytes(&keccak(&number.to_be_bytes())), // parent_hash
        enc::bytes(&keccak(&[0xc0])),               // ommers_hash
        enc::bytes(&[0x11; 20]),                    // beneficiary
        enc::bytes(&state_root),
        enc::bytes(&[0x22; 32]), // transactions_root
        enc::bytes(&receipts_root),
        enc::bytes(&[0u8; 256]), // logs_bloom
        enc::u64(0),             // difficulty
        enc::u64(number),
        enc::u64(36_000_000),
        enc::u64(12_345_678),
        enc::u64(1_700_000_000 + 12 * number),
        enc::bytes(b"ferrata"),  // extra_data
        enc::bytes(&[0x33; 32]), // mix_hash
        enc::bytes(&[0u8; 8]),   // nonce
        enc::u64(7),             // base_fee_per_gas
        enc::bytes(&[0x44; 32]), // withdrawals_root
        enc::u64(131_072),       // blob_gas_used
        enc::u64(0),             // excess_blob_gas
        enc::bytes(&[0x55; 32]), // parent_beacon_block_root
        enc::bytes(&[0x66; 32]), // requests_hash
        enc::bytes(&[0x77; 32]), // block_access_list_hash
        enc::u64(15_000_000),    // slot_number
    ];
    items.resize_with(fields, || enc::bytes(&[0x88; 32]));
    items
}

/// [`header_items`], encoded.
pub fn header(number: u64, state_root: [u8; 32], receipts_root: [u8; 32], fields: usize) -> Vec<u8> {
    enc::list(&header_items(number, state_root, receipts_root, fields))
}

/// The SSZ bytes of a header record, written field by field from its documented layout.
#[allow(clippy::too_many_arguments)]
pub fn record_ssz(
    chain_id: u64,
    slot: u64,
    number: u64,
    timestamp: u64,
    block_hash: [u8; 32],
    state_root: [u8; 32],
    receipts_root: [u8; 32],
    binding: u8,
    hops: u32,
) -> Vec<u8> {
    let parent_hash = keccak(&number.wrapping_sub(1).to_be_bytes());
    [
        &chain_id.to_le_bytes()[..],
        &slot.to_le_bytes(),
        &number.to_le_bytes(),
        &timestamp.to_le_bytes(),
        &block_hash,
        &parent_hash,
        &state_root,
        &receipts_root,
        &[binding],
        &hops.to_le_bytes(),
    ]
    .concat()
}

/// A storage word from a small integer.
pub fn w(n: u128) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[16..].copy_from_slice(&n.to_be_bytes());
    out
}

/// (slot, value) pairs of one account's storage.
pub type Slots = Vec<([u8; 32], [u8; 32])>;

/// A synthetic block: a state trie with three accounts (a contract with storage, a contract
/// with one slot, an account without storage), and a receipts trie of every type.
pub struct World {
    pub state: Trie,
    pub storage: std::collections::BTreeMap<[u8; 20], Trie>,
    pub slots: std::collections::BTreeMap<[u8; 20], Slots>,
    pub receipts: Trie,
    pub receipt_values: Vec<Vec<u8>>,
}

pub const TOKEN: [u8; 20] = [0xaa; 20];
pub const SMALL: [u8; 20] = [0xbb; 20];
pub const PLAIN: [u8; 20] = [0xcc; 20];
pub const NOBODY: [u8; 20] = [0xdd; 20];
/// `Transfer(address,address,uint256)`.
pub const TRANSFER: [u8; 32] = [
    0xdd, 0xf2, 0x52, 0xad, 0x1b, 0xe2, 0xc8, 0x9b, 0x69, 0xc2, 0xb0, 0x68, 0xfc, 0x37, 0x8d, 0xaa, 0x95, 0x2b, 0xa7,
    0xf1, 0x63, 0xc4, 0xa1, 0x16, 0x28, 0xf5, 0x5a, 0x4d, 0xf5, 0x23, 0xb3, 0xef,
];
/// The index of the failed transaction in [`World::receipts`].
pub const FAILED_TX: u64 = 7;

impl World {
    pub fn new(seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let mut storage = std::collections::BTreeMap::new();
        let mut slots = std::collections::BTreeMap::new();
        let token_slots: Vec<([u8; 32], [u8; 32])> = (0..60u128)
            .map(|i| {
                let value = if i % 5 == 0 { w(1 + i) } else { w(u128::from(rng.next_u64()) << 40 | 1) };
                (w(1_000 + i), value)
            })
            .collect();
        let small_slots = vec![(w(0), w(0x2a))];
        for (address, s) in [(TOKEN, token_slots), (SMALL, small_slots)] {
            let trie = Trie::new(s.iter().map(|(k, v)| (keccak(k).to_vec(), enc::uint(v))));
            storage.insert(address, trie);
            slots.insert(address, s);
        }
        let code = keccak(b"code");
        let mut accounts = vec![
            (TOKEN, account_rlp(1, 0, storage[&TOKEN].root(), code)),
            (SMALL, account_rlp(1, 5, storage[&SMALL].root(), code)),
            (PLAIN, account_rlp(42, 10u128.pow(20), keccak(&[0x80]), keccak(b""))),
        ];
        for _ in 0..200 {
            let a: [u8; 20] = rng.bytes(20).try_into().unwrap();
            accounts
                .push((a, account_rlp(rng.next_u64() % 100, u128::from(rng.next_u64()), keccak(&[0x80]), keccak(b""))));
        }
        let state = Trie::new(accounts.iter().map(|(a, rlp)| (keccak(a).to_vec(), rlp.clone())));
        let mut receipt_values = Vec::new();
        for i in 0..40u64 {
            let tx_type = (i % 5) as u8;
            let logs: Vec<Vec<u8>> = (0..(i % 4))
                .map(|j| {
                    let mut from = [0u8; 32];
                    from[12..].copy_from_slice(&[0x01; 20]);
                    let mut to = [0u8; 32];
                    to[12..].copy_from_slice(&[0x02; 20]);
                    log_rlp(TOKEN, &[TRANSFER, from, to], &w(u128::from(1_000 * i + j)))
                })
                .collect();
            let status = if i == FAILED_TX { enc::u64(0) } else { enc::u64(1) };
            receipt_values.push(receipt_with(tx_type, status, 21_000 * (i + 1), &logs));
        }
        let receipts = Trie::new(receipt_values.iter().enumerate().map(|(i, v)| {
            let (k, l) = ferrata_core::rlp::encode_u64(i as u64);
            (k[..l].to_vec(), v.clone())
        }));
        World { state, storage, slots, receipts, receipt_values }
    }

    /// `eth_getProof`-style proofs: the account proof and the proof of `slot`.
    pub fn proofs(&self, address: &[u8; 20], slot: &[u8; 32]) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
        let account = self.state.proof(&keccak(address));
        let storage = self.storage.get(address).map(|t| t.proof(&keccak(slot))).unwrap_or_default();
        (account, storage)
    }

    /// The receipt proof of transaction `i`.
    pub fn receipt_proof(&self, i: u64) -> Vec<Vec<u8>> {
        let (k, l) = ferrata_core::rlp::encode_u64(i);
        self.receipts.proof(&k[..l])
    }

    /// A record of this block (restored from its SSZ bytes).
    pub fn record(&self, chain_id: u64, number: u64) -> ferrata_core::attest::HeaderRecord {
        let bytes = record_ssz(
            chain_id,
            9_000_000,
            number,
            1_700_000_000,
            keccak(&number.to_le_bytes()),
            self.state.root(),
            self.receipts.root(),
            0,
            0,
        );
        ferrata_core::attest::HeaderRecord::from_ssz(&bytes).expect("record")
    }
}
