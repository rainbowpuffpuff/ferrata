//! Merkle-Patricia proof verification (Yellow Paper, Appendix D), for proofs in the format of
//! `eth_getProof` (EIP-1186): the trie nodes on the key's path, root first.
//!
//! A node enters the walk only through the reference its parent holds: the root hash for the
//! first node, a child's 32-byte hash, or the child node itself when it is embedded (its encoding
//! is shorter than 32 bytes). Embedded nodes therefore never appear as proof elements; a node
//! referenced by hash must be the next proof element, and its Keccak-256 must equal the
//! reference. The walk follows the key one nibble per branch and one path per extension, and
//! ends at a value (inclusion), or where the trie shows the key is absent (exclusion): an empty
//! branch slot, an extension or leaf whose path diverges from the key, or a branch whose value
//! slot is empty when the key ends there. Every proof element must be used.
//!
//! Callers hash secure-trie keys themselves (`keccak256(address)`, `keccak256(slot)`); the
//! receipts trie uses `RLP(index)` as its key.

use crate::{
    keccak::keccak256,
    rlp::{self, Item},
    Error, Root,
};

/// A constant from hexadecimal digits, at compile time.
pub(crate) const fn hex<const N: usize>(s: &str) -> [u8; N] {
    const fn digit(c: u8) -> u8 {
        match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            _ => panic!("not a lower-case hex digit"),
        }
    }
    let b = s.as_bytes();
    assert!(b.len() == 2 * N);
    let mut out = [0u8; N];
    let mut i = 0;
    while i < N {
        out[i] = (digit(b[2 * i]) << 4) | digit(b[2 * i + 1]);
        i += 1;
    }
    out
}

/// The root of the empty trie, `KEC(RLP(""))`.
pub const EMPTY_ROOT: Root = hex("56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421");

/// How a node is referenced by its parent.
#[derive(Clone, Copy)]
enum Child<'a> {
    /// By the Keccak-256 of its encoding: the node is the next proof element.
    Hash(Root),
    /// Embedded: the node's encoding (shorter than 32 bytes) itself.
    Embedded(&'a [u8]),
}

/// The nibbles of a hex-prefix encoded path.
struct Path<'a> {
    encoded: &'a [u8],
    /// Index of the first path nibble in `encoded` (1 for odd paths, 2 for even ones).
    start: usize,
    len: usize,
}

fn nibble(bytes: &[u8], i: usize) -> u8 {
    let b = bytes[i / 2];
    if i % 2 == 0 {
        b >> 4
    } else {
        b & 0x0f
    }
}

impl Path<'_> {
    /// True if the key's nibbles from `pos` start with this path.
    fn is_prefix_of(&self, key: &[u8], pos: usize) -> bool {
        self.len <= 2 * key.len() - pos
            && (0..self.len).all(|j| nibble(key, pos + j) == nibble(self.encoded, self.start + j))
    }
}

/// Hex-prefix decoding (Yellow Paper, Appendix C): the flag nibble says leaf (2, 3) or extension
/// (0, 1) and odd (1, 3) or even (0, 2) length; an even path pads the flag byte with a zero
/// nibble.
fn hex_prefix(encoded: &[u8]) -> Result<(bool, Path<'_>), Error> {
    let first = *encoded.first().ok_or(Error::ProofNodeInvalid)?;
    let (leaf, odd) = match first >> 4 {
        0 => (false, false),
        1 => (false, true),
        2 => (true, false),
        3 => (true, true),
        _ => return Err(Error::ProofNodeInvalid),
    };
    if !odd && first & 0x0f != 0 {
        return Err(Error::ProofNodeInvalid);
    }
    let start = if odd { 1 } else { 2 };
    Ok((leaf, Path { encoded, start, len: 2 * encoded.len() - start }))
}

/// A child reference: empty (no child), a 32-byte hash, or an embedded node shorter than 32
/// bytes. Anything else is not a canonical node.
fn child(item: Item<'_>) -> Result<Option<Child<'_>>, Error> {
    if item.is_list {
        if item.raw.len() >= 32 {
            return Err(Error::ProofNodeInvalid);
        }
        return Ok(Some(Child::Embedded(item.raw)));
    }
    match item.payload.len() {
        0 => Ok(None),
        32 => Ok(Some(Child::Hash(item.payload.try_into().map_err(|_| Error::ProofNodeInvalid)?))),
        _ => Err(Error::ProofNodeInvalid),
    }
}

/// A string item: a path or a value.
fn string(item: Item<'_>) -> Result<&[u8], Error> {
    item.bytes().map_err(|_| Error::ProofNodeInvalid)
}

// A branch holds its 16 decoded child references; nodes live only for one step of the walk.
#[allow(clippy::large_enum_variant)]
enum Node<'a> {
    Branch { children: [Option<Child<'a>>; 16], value: Option<&'a [u8]> },
    Extension { path: Path<'a>, child: Child<'a> },
    Leaf { path: Path<'a>, value: &'a [u8] },
}

/// Decode a trie node: a canonical RLP list of 17 items (branch) or 2 (extension or leaf).
fn decode_node(encoded: &[u8]) -> Result<Node<'_>, Error> {
    let list = rlp::decode(encoded)?.list().map_err(|_| Error::ProofNodeInvalid)?;
    let mut items = [Item::EMPTY; 17];
    let mut count = 0;
    for item in list {
        let slot = items.get_mut(count).ok_or(Error::ProofNodeInvalid)?;
        *slot = item?;
        count += 1;
    }
    match count {
        17 => {
            let mut children = [None; 16];
            for (c, item) in children.iter_mut().zip(items) {
                *c = child(item)?;
            }
            let value = string(items[16])?;
            Ok(Node::Branch { children, value: (!value.is_empty()).then_some(value) })
        }
        2 => {
            let (leaf, path) = hex_prefix(string(items[0])?)?;
            if leaf {
                let value = string(items[1])?;
                // A key whose value is empty is absent from the trie: it has no leaf.
                if value.is_empty() {
                    return Err(Error::ProofNodeInvalid);
                }
                return Ok(Node::Leaf { path, value });
            }
            if path.len == 0 {
                return Err(Error::ProofNodeInvalid);
            }
            let child = child(items[1])?.ok_or(Error::ProofNodeInvalid)?;
            Ok(Node::Extension { path, child })
        }
        _ => Err(Error::ProofNodeInvalid),
    }
}

/// Verify `proof` for `key` in the trie with root `root`. Returns the value stored at `key`
/// (inclusion) or `None` (exclusion). The empty trie is proven by the empty proof.
///
/// Refuses ([`Error`] codes): a node whose hash differs from its reference
/// (`ProofNodeMismatch`); a proof that ends before the walk does (`ProofIncomplete`); elements
/// the walk did not reach (`ProofTrailingNodes`); a node that is not canonical RLP (`Rlp*`); a
/// node that is not a canonical trie node, including one shorter than 32 bytes referenced by
/// hash (`ProofNodeInvalid`).
pub fn verify<'p, P: AsRef<[u8]>>(root: &Root, key: &[u8], proof: &'p [P]) -> Result<Option<&'p [u8]>, Error> {
    if proof.is_empty() && *root == EMPTY_ROOT {
        return Ok(None);
    }
    let total = 2 * key.len();
    let mut pos = 0;
    let mut used = 0;
    let mut next = Child::Hash(*root);
    let found = loop {
        let encoded: &'p [u8] = match next {
            Child::Hash(hash) => {
                let node = proof.get(used).ok_or(Error::ProofIncomplete)?.as_ref();
                if keccak256(node) != hash {
                    return Err(Error::ProofNodeMismatch);
                }
                // Only the root may be shorter than 32 bytes and still be referenced by hash.
                if used > 0 && node.len() < 32 {
                    return Err(Error::ProofNodeInvalid);
                }
                used += 1;
                node
            }
            Child::Embedded(node) => node,
        };
        match decode_node(encoded)? {
            Node::Branch { children, value } => {
                if pos == total {
                    break value;
                }
                match children[usize::from(nibble(key, pos))] {
                    None => break None,
                    Some(c) => {
                        pos += 1;
                        next = c;
                    }
                }
            }
            Node::Extension { path, child } => {
                if !path.is_prefix_of(key, pos) {
                    break None;
                }
                pos += path.len;
                next = child;
            }
            Node::Leaf { path, value } => {
                break (path.len == total - pos && path.is_prefix_of(key, pos)).then_some(value);
            }
        }
    };
    if used != proof.len() {
        return Err(Error::ProofTrailingNodes);
    }
    Ok(found)
}
