//! Strict RLP (`ferrata_core::rlp`) against alloy-rlp 0.3.17, on arbitrary bytes.
//!
//! * The whole item tree, walked iteratively on both sides, is accepted by both or refused by
//!   both, with the same shape and the same string bytes.
//! * Every item, string or list, decodes as `u64` and as a 256-bit word exactly when alloy's
//!   `u64` / `U256` decoders accept it, to the same value.
//! * `encode_u64` is alloy's encoding of the same number and decodes back to it.
#![no_main]
use alloy_primitives::U256;
use alloy_rlp::{Decodable, Encodable, Header};
use ferrata_core::rlp;
use libfuzzer_sys::fuzz_target;

type Event = (usize, bool, Vec<u8>);

fn walk_ferrata(input: &[u8]) -> Option<(Vec<Event>, Vec<rlp::Item<'_>>)> {
    let top = rlp::decode(input).ok()?;
    let (mut events, mut items) = (Vec::new(), Vec::new());
    let mut stack = vec![(0usize, top)];
    while let Some((depth, item)) = stack.pop() {
        // The item's encoding is its prefix followed by its payload.
        assert!(item.raw.ends_with(item.payload));
        assert_eq!(rlp::decode(item.raw).ok(), Some(item));
        items.push(item);
        if item.is_list {
            events.push((depth, true, Vec::new()));
            let kids = item.list().ok()?.collect::<Result<Vec<_>, _>>().ok()?;
            stack.extend(kids.into_iter().rev().map(|k| (depth + 1, k)));
        } else {
            events.push((depth, false, item.payload.to_vec()));
        }
    }
    Some((events, items))
}

fn alloy_item<'a>(buf: &mut &'a [u8]) -> Option<(bool, &'a [u8])> {
    let h = Header::decode(buf).ok()?;
    let (payload, rest) = buf.split_at(h.payload_length);
    *buf = rest;
    Some((h.list, payload))
}

fn walk_alloy(input: &[u8]) -> Option<Vec<Event>> {
    let mut buf = input;
    let (list, payload) = alloy_item(&mut buf)?;
    if !buf.is_empty() {
        return None;
    }
    let mut events = Vec::new();
    let mut stack = vec![(0usize, list, payload)];
    while let Some((depth, list, payload)) = stack.pop() {
        if list {
            events.push((depth, true, Vec::new()));
            let mut q = payload;
            let mut kids = Vec::new();
            while !q.is_empty() {
                kids.push(alloy_item(&mut q)?);
            }
            stack.extend(kids.into_iter().rev().map(|(l, p)| (depth + 1, l, p)));
        } else {
            events.push((depth, false, payload.to_vec()));
        }
    }
    Some(events)
}

fn exact<T: Decodable>(raw: &[u8]) -> Option<T> {
    let mut r = raw;
    let v = T::decode(&mut r).ok()?;
    r.is_empty().then_some(v)
}

fuzz_target!(|data: &[u8]| {
    let ours = walk_ferrata(data);
    let theirs = walk_alloy(data);
    assert_eq!(ours.as_ref().map(|(e, _)| e), theirs.as_ref(), "tree differs from alloy-rlp");
    if let Some((_, items)) = &ours {
        for item in items {
            assert_eq!(item.u64().ok(), exact::<u64>(item.raw), "u64 differs");
            assert_eq!(item.word().ok(), exact::<U256>(item.raw).map(|u| u.to_be_bytes::<32>()), "word differs");
        }
    }
    // The first item alone (split_first) agrees with alloy's header on the prefix length.
    if let Ok((item, rest)) = rlp::split_first(data) {
        assert_eq!(item.raw.len() + rest.len(), data.len());
        let mut buf = data;
        let (list, payload) = alloy_item(&mut buf).expect("alloy refuses an item ferrata splits");
        assert_eq!((list, payload), (item.is_list, item.payload));
    }
    let n = u64::from_le_bytes(core::array::from_fn(|i| data.get(i).copied().unwrap_or(0)));
    let (buf, len) = rlp::encode_u64(n);
    let mut reference = Vec::new();
    n.encode(&mut reference);
    assert_eq!(&buf[..len], &reference[..]);
    assert_eq!(rlp::decode(&buf[..len]).and_then(|i| i.u64()), Ok(n));
});
