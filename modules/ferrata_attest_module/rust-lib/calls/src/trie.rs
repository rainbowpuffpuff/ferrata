//! A block's receipts trie, rebuilt from `eth_getBlockReceipts`, and the proof of one receipt in
//! `eth_getProof`'s format (the root node first). Adapted from the reference trie of the core's
//! test crate (`crates/ferrata-spectests/src/execkit.rs`: RLP, hex-prefix, `c(I, i)` of the Yellow
//! Paper's Appendix D), with the core's Keccak-256. The program never trusts this code: it checks
//! the proof against the receipts root its record holds.

use std::collections::BTreeMap;

use ferrata_core::keccak::keccak256;
use serde_json::Value;

use super::{text, unhex};

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
    pub fn bytes(b: &[u8]) -> Vec<u8> {
        if b.len() == 1 && b[0] < 0x80 {
            return b.to_vec();
        }
        let mut out = length(b.len(), 0x80);
        out.extend_from_slice(b);
        out
    }
    pub fn list(items: &[Vec<u8>]) -> Vec<u8> {
        let payload: Vec<u8> = items.concat();
        let mut out = length(payload.len(), 0xc0);
        out.extend(payload);
        out
    }
    pub fn u64(n: u64) -> Vec<u8> {
        let be = n.to_be_bytes();
        let start = be.iter().position(|b| *b != 0).unwrap_or(be.len());
        bytes(&be[start..])
    }
}

fn nibbles(key: &[u8]) -> Vec<u8> {
    key.iter().flat_map(|b| [b >> 4, b & 0x0f]).collect()
}

fn hex_prefix(nibbles: &[u8], leaf: bool) -> Vec<u8> {
    let odd = nibbles.len() % 2 == 1;
    let flag = (if leaf { 2 } else { 0 }) + u8::from(odd);
    let (mut out, rest) =
        if odd { (vec![(flag << 4) | nibbles[0]], &nibbles[1..]) } else { (vec![flag << 4], nibbles) };
    out.extend(rest.chunks(2).map(|p| (p[0] << 4) | p[1]));
    out
}

struct Built {
    node: Node,
    enc: Vec<u8>,
}

enum Node {
    Leaf,
    Extension { path: Vec<u8>, child: Box<Built> },
    Branch { children: Vec<Option<Built>> },
}

impl Built {
    fn reference(&self) -> Vec<u8> {
        if self.enc.len() < 32 {
            self.enc.clone()
        } else {
            enc::bytes(&keccak256(&self.enc))
        }
    }
}

fn build(entries: &[(Vec<u8>, Vec<u8>)], i: usize) -> Built {
    if entries.len() == 1 {
        let (key, value) = &entries[0];
        let enc = enc::list(&[enc::bytes(&hex_prefix(&key[i..], true)), enc::bytes(value)]);
        return Built { node: Node::Leaf, enc };
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
    Built { node: Node::Branch { children }, enc: enc::list(&items) }
}

/// A trie over byte keys.
pub struct Trie {
    root: Option<Built>,
}

impl Trie {
    pub fn new<I: IntoIterator<Item = (Vec<u8>, Vec<u8>)>>(pairs: I) -> Self {
        let map: BTreeMap<Vec<u8>, Vec<u8>> = pairs.into_iter().map(|(k, v)| (nibbles(&k), v)).collect();
        let entries: Vec<(Vec<u8>, Vec<u8>)> = map.into_iter().collect();
        Trie { root: (!entries.is_empty()).then(|| build(&entries, 0)) }
    }

    pub fn root(&self) -> [u8; 32] {
        keccak256(self.root.as_ref().map_or(&[0x80][..], |r| &r.enc))
    }

    /// The proof of `key`: the root node, then each node on the key's path that its parent
    /// references by hash.
    pub fn proof(&self, key: &[u8]) -> Vec<Vec<u8>> {
        let k = nibbles(key);
        let mut out = Vec::new();
        let Some(mut node) = self.root.as_ref() else { return out };
        out.push(node.enc.clone());
        let mut i = 0;
        loop {
            let next = match &node.node {
                Node::Leaf => None,
                Node::Extension { path, child } => {
                    let follows = k.len() >= i + path.len() && k[i..i + path.len()] == path[..];
                    i += path.len();
                    follows.then_some(child.as_ref())
                }
                Node::Branch { children } => {
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

/// A JSON-RPC quantity (`0x`-hex).
pub fn quantity(v: &Value, what: &str) -> Result<u64, String> {
    let s = text(v, what)?;
    u64::from_str_radix(s.trim_start_matches("0x"), 16).map_err(|_| format!("{what}: not a quantity: {s}"))
}

fn bytes_of(v: &Value, what: &str) -> Result<Vec<u8>, String> {
    unhex(text(v, what)?).map_err(|e| format!("{what}: {e}"))
}

/// The consensus encoding of one `eth_getBlockReceipts` entry: `[status, cumulative gas used,
/// logs bloom, logs]`, prefixed by the transaction type unless legacy.
pub fn receipt_rlp(r: &Value) -> Result<Vec<u8>, String> {
    let logs = r["logs"].as_array().ok_or("receipt: no logs array")?;
    let mut encoded_logs = Vec::with_capacity(logs.len());
    for (i, l) in logs.iter().enumerate() {
        let topics = l["topics"].as_array().ok_or_else(|| format!("log {i}: no topics"))?;
        let mut t = Vec::with_capacity(topics.len());
        for (j, topic) in topics.iter().enumerate() {
            t.push(enc::bytes(&bytes_of(topic, &format!("log {i} topic {j}"))?));
        }
        encoded_logs.push(enc::list(&[
            enc::bytes(&bytes_of(&l["address"], &format!("log {i} address"))?),
            enc::list(&t),
            enc::bytes(&bytes_of(&l["data"], &format!("log {i} data"))?),
        ]));
    }
    let body = enc::list(&[
        enc::u64(quantity(&r["status"], "receipt status")?),
        enc::u64(quantity(&r["cumulativeGasUsed"], "receipt cumulativeGasUsed")?),
        enc::bytes(&bytes_of(&r["logsBloom"], "receipt logsBloom")?),
        enc::list(&encoded_logs),
    ]);
    Ok(match quantity(&r["type"], "receipt type")? {
        0 => body,
        t => [vec![t as u8], body].concat(),
    })
}

/// The receipts trie of a block from its `eth_getBlockReceipts` entries, in transaction order.
pub fn receipts_trie(receipts: &[Value]) -> Result<Trie, String> {
    let mut pairs = Vec::with_capacity(receipts.len());
    for (i, r) in receipts.iter().enumerate() {
        pairs.push((enc::u64(i as u64), receipt_rlp(r).map_err(|e| format!("receipt {i}: {e}"))?));
    }
    Ok(Trie::new(pairs))
}

/// The key of transaction `tx_index`'s receipt: `RLP(tx_index)`.
pub fn receipt_key(tx_index: u64) -> Vec<u8> {
    enc::u64(tx_index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_single() {
        // The empty trie's root is Keccak-256(RLP("")).
        assert_eq!(
            super::super::hex(&Trie::new(Vec::new()).root()),
            "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
        );
        let t = Trie::new(vec![(receipt_key(0), vec![0xaa; 40])]);
        assert_eq!(t.proof(&receipt_key(0)).len(), 1);
        assert_eq!(keccak256(&t.proof(&receipt_key(0))[0]), t.root());
    }
}
