//! Strict RLP decoding (Yellow Paper, Appendix B), as zero-copy views over the input.
//!
//! Only the canonical encoding of a value is accepted, so every value has exactly one encoding
//! that decodes:
//!
//! * a single byte below `0x80` is its own encoding; wrapped as a one-byte string (`0x81 0x05`)
//!   it is refused;
//! * a payload of 0 to 55 bytes takes the short form; a long form with a length below 56 is
//!   refused, and so is a long-form length with a leading zero byte;
//! * an unsigned integer is its big-endian bytes without leading zeros (zero is the empty
//!   string), so `0x00` and `0x820001` are refused as integers;
//! * a length that runs past the end of the input, or past the end of the list that contains
//!   the item, is refused, and so are bytes after the last item ([`decode`]).
//!
//! Lists are decoded lazily: [`Item::list`] walks a list's payload one item at a time, and every
//! item is bounded by its enclosing list. Nothing here recurses, so deep nesting cannot exhaust
//! the stack; a caller descends one level per call.

use crate::Error;

/// One RLP item: a byte string or a list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Item<'a> {
    /// The item's whole encoding: prefix and payload.
    pub raw: &'a [u8],
    /// A string's bytes, or a list's concatenated item encodings.
    pub payload: &'a [u8],
    /// True for a list.
    pub is_list: bool,
}

impl Item<'static> {
    /// The empty string (a placeholder for fixed-size arrays of items).
    pub const EMPTY: Item<'static> = Item { raw: &[0x80], payload: &[], is_list: false };
}

/// Decode the item at the start of `input`, and return it with the bytes that follow it.
pub fn split_first(input: &[u8]) -> Result<(Item<'_>, &[u8]), Error> {
    let (&b0, after_prefix) = input.split_first().ok_or(Error::RlpMalformed)?;
    let (is_list, header, len) = match b0 {
        0x00..=0x7f => {
            let (raw, rest) = input.split_at(1);
            return Ok((Item { raw, payload: raw, is_list: false }, rest));
        }
        0x80..=0xb7 => (false, 1, usize::from(b0 - 0x80)),
        0xb8..=0xbf => {
            let n = usize::from(b0 - 0xb7);
            (false, 1 + n, long_length(after_prefix, n)?)
        }
        0xc0..=0xf7 => (true, 1, usize::from(b0 - 0xc0)),
        0xf8..=0xff => {
            let n = usize::from(b0 - 0xf7);
            (true, 1 + n, long_length(after_prefix, n)?)
        }
    };
    let end = header.checked_add(len).ok_or(Error::RlpMalformed)?;
    if end > input.len() {
        return Err(Error::RlpMalformed);
    }
    let (raw, rest) = input.split_at(end);
    let payload = &raw[header..];
    if !is_list && len == 1 && payload[0] < 0x80 {
        return Err(Error::RlpNonCanonical);
    }
    Ok((Item { raw, payload, is_list }, rest))
}

/// The payload length after a long-form prefix: `n` big-endian bytes (1 to 8) without a leading
/// zero, at least 56 (shorter payloads take the short form).
fn long_length(bytes: &[u8], n: usize) -> Result<usize, Error> {
    let be = bytes.get(..n).ok_or(Error::RlpMalformed)?;
    if be[0] == 0 {
        return Err(Error::RlpNonCanonical);
    }
    let mut len: usize = 0;
    for &b in be {
        len = len.checked_mul(256).and_then(|l| l.checked_add(usize::from(b))).ok_or(Error::RlpMalformed)?;
    }
    if len < 56 {
        return Err(Error::RlpNonCanonical);
    }
    Ok(len)
}

/// Decode exactly one item that spans all of `input`.
pub fn decode(input: &[u8]) -> Result<Item<'_>, Error> {
    let (item, rest) = split_first(input)?;
    if !rest.is_empty() {
        return Err(Error::RlpMalformed);
    }
    Ok(item)
}

impl<'a> Item<'a> {
    /// The bytes of a string.
    pub fn bytes(&self) -> Result<&'a [u8], Error> {
        if self.is_list {
            return Err(Error::RlpMalformed);
        }
        Ok(self.payload)
    }

    /// A string of exactly `N` bytes.
    pub fn fixed<const N: usize>(&self) -> Result<[u8; N], Error> {
        self.bytes()?.try_into().map_err(|_| Error::RlpMalformed)
    }

    /// A canonical unsigned integer of at most `max` bytes: its big-endian bytes.
    fn uint(&self, max: usize) -> Result<&'a [u8], Error> {
        let b = self.bytes()?;
        if b.first() == Some(&0) {
            return Err(Error::RlpNonCanonical);
        }
        if b.len() > max {
            return Err(Error::RlpMalformed);
        }
        Ok(b)
    }

    /// A canonical unsigned integer below 2^64.
    pub fn u64(&self) -> Result<u64, Error> {
        Ok(self.uint(8)?.iter().fold(0, |acc, &b| (acc << 8) | u64::from(b)))
    }

    /// A canonical unsigned integer below 2^256, as a big-endian 32-byte word.
    pub fn word(&self) -> Result<[u8; 32], Error> {
        let b = self.uint(32)?;
        let mut w = [0u8; 32];
        w[32 - b.len()..].copy_from_slice(b);
        Ok(w)
    }

    /// The items of a list.
    pub fn list(&self) -> Result<List<'a>, Error> {
        if !self.is_list {
            return Err(Error::RlpMalformed);
        }
        Ok(List { rest: self.payload })
    }

    /// The items of a list that has exactly `N` of them.
    pub fn items<const N: usize>(&self) -> Result<[Item<'a>; N], Error> {
        let mut out = [Item::EMPTY; N];
        let mut list = self.list()?;
        for slot in out.iter_mut() {
            *slot = list.next().ok_or(Error::RlpMalformed)??;
        }
        if list.next().is_some() {
            return Err(Error::RlpMalformed);
        }
        Ok(out)
    }
}

/// The items of a list, in order. Each is bounded by the list's payload; after an error the
/// iterator ends.
#[derive(Clone, Debug)]
pub struct List<'a> {
    rest: &'a [u8],
}

impl<'a> Iterator for List<'a> {
    type Item = Result<Item<'a>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.rest.is_empty() {
            return None;
        }
        match split_first(self.rest) {
            Ok((item, rest)) => {
                self.rest = rest;
                Some(Ok(item))
            }
            Err(e) => {
                self.rest = &[];
                Some(Err(e))
            }
        }
    }
}

/// The canonical encoding of an unsigned integer (receipt-trie keys are `RLP(index)`), as a
/// buffer and the number of bytes used.
pub fn encode_u64(n: u64) -> ([u8; 9], usize) {
    let mut out = [0u8; 9];
    if n == 0 {
        out[0] = 0x80;
        return (out, 1);
    }
    if n < 0x80 {
        out[0] = n as u8;
        return (out, 1);
    }
    let skip = (n.leading_zeros() / 8) as usize;
    let len = 8 - skip;
    out[0] = 0x80 + len as u8;
    out[1..=len].copy_from_slice(&n.to_be_bytes()[skip..]);
    (out, len + 1)
}
