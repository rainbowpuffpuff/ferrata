//! Strict RLP decoding: every decoder branch, a corpus of non-canonical and malformed
//! encodings (each refused by ferrata with an exact code, and by alloy-rlp), random round trips
//! against alloy-rlp, and a differential on randomly mutated encodings.

use alloy_rlp::{Decodable, Encodable, Header};
use ferrata_core::{
    rlp::{decode, encode_u64, split_first, Item},
    Error,
};
use ferrata_spectests::execkit::{enc, Rng};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Tree {
    Bytes(Vec<u8>),
    List(Vec<Tree>),
}

fn ferrata_tree(item: Item<'_>) -> Result<Tree, Error> {
    if !item.is_list {
        return Ok(Tree::Bytes(item.bytes()?.to_vec()));
    }
    item.list()?.map(|i| ferrata_tree(i?)).collect::<Result<Vec<_>, _>>().map(Tree::List)
}

fn ferrata_decode(input: &[u8]) -> Result<Tree, Error> {
    ferrata_tree(decode(input)?)
}

/// A strict generic decoder built on alloy-rlp's `Header`.
fn alloy_tree(buf: &mut &[u8]) -> Result<Tree, alloy_rlp::Error> {
    let h = Header::decode(buf)?;
    let (payload, rest) = buf.split_at(h.payload_length);
    *buf = rest;
    if !h.list {
        return Ok(Tree::Bytes(payload.to_vec()));
    }
    let mut p = payload;
    let mut items = Vec::new();
    while !p.is_empty() {
        items.push(alloy_tree(&mut p)?);
    }
    Ok(Tree::List(items))
}

fn alloy_decode(input: &[u8]) -> Result<Tree, alloy_rlp::Error> {
    let mut buf = input;
    let t = alloy_tree(&mut buf)?;
    if !buf.is_empty() {
        return Err(alloy_rlp::Error::UnexpectedLength);
    }
    Ok(t)
}

fn encode(t: &Tree) -> Vec<u8> {
    match t {
        Tree::Bytes(b) => enc::bytes(b),
        Tree::List(items) => enc::list(&items.iter().map(encode).collect::<Vec<_>>()),
    }
}

fn alloy_encode(t: &Tree) -> Vec<u8> {
    let mut out = Vec::new();
    match t {
        Tree::Bytes(b) => b.as_slice().encode(&mut out),
        Tree::List(items) => {
            let payload: Vec<u8> = items.iter().flat_map(alloy_encode).collect();
            Header { list: true, payload_length: payload.len() }.encode(&mut out);
            out.extend(payload);
        }
    }
    out
}

#[test]
fn short_and_long_strings_and_lists() {
    // Single bytes are their own encoding.
    for b in [[0x00u8], [0x01], [0x7f]] {
        let item = decode(&b).unwrap();
        assert_eq!((item.payload, item.is_list, item.raw), (&b[..], false, &b[..]));
    }
    assert_eq!(decode(&[0x80]).unwrap().payload, b"");
    assert_eq!(decode(&[0x81, 0x80]).unwrap().payload, [0x80]);
    assert_eq!(decode(&[0x83, b'd', b'o', b'g']).unwrap().payload, b"dog");
    let s55 = [7u8; 55];
    assert_eq!(decode(&enc::bytes(&s55)).unwrap().payload, s55);
    assert_eq!(enc::bytes(&s55)[0], 0xb7);
    let s56 = [7u8; 56];
    assert_eq!(enc::bytes(&s56)[..2], [0xb8, 56]);
    assert_eq!(decode(&enc::bytes(&s56)).unwrap().payload, s56);
    let s1024 = vec![9u8; 1024];
    assert_eq!(enc::bytes(&s1024)[..3], [0xb9, 0x04, 0x00]);
    assert_eq!(decode(&enc::bytes(&s1024)).unwrap().payload, s1024);
    // Lists.
    let empty = decode(&[0xc0]).unwrap();
    assert!(empty.is_list && empty.payload.is_empty());
    let cat_dog = [0xc8, 0x83, b'c', b'a', b't', 0x83, b'd', b'o', b'g'];
    let items: Vec<_> = decode(&cat_dog).unwrap().list().unwrap().map(|i| i.unwrap().payload).collect();
    assert_eq!(items, [b"cat", b"dog"]);
    let long_list = enc::list(&vec![enc::bytes(&[1, 2, 3]); 20]);
    assert_eq!(long_list[..2], [0xf8, 80]);
    assert_eq!(decode(&long_list).unwrap().list().unwrap().count(), 20);
    // split_first leaves the rest.
    let (first, rest) = split_first(&[0x82, 1, 2, 0xc0]).unwrap();
    assert_eq!((first.payload, rest), (&[1, 2][..], &[0xc0][..]));
}

/// Encodings the Yellow Paper rules out, with the code ferrata gives; alloy-rlp refuses each too.
const CORPUS: &[(&[u8], Error, &str)] = &[
    (&[], Error::RlpMalformed, "empty input"),
    (&[0x81, 0x00], Error::RlpNonCanonical, "single byte 0x00 wrapped as a string"),
    (&[0x81, 0x7f], Error::RlpNonCanonical, "single byte 0x7f wrapped as a string"),
    (&[0xb8, 0x00], Error::RlpNonCanonical, "long-form length with a leading zero"),
    (&[0xb8, 0x01, 0xaa], Error::RlpNonCanonical, "long form for a 1-byte string"),
    (&[0xb8, 0x37], Error::RlpNonCanonical, "long form for a 55-byte string"),
    (&[0xb9, 0x00, 0x38], Error::RlpNonCanonical, "two length bytes with a leading zero"),
    (&[0xf8, 0x00], Error::RlpNonCanonical, "long-form list length with a leading zero"),
    (&[0xf8, 0x02, 0x01, 0x02], Error::RlpNonCanonical, "long form for a 2-byte list"),
    (&[0x82, 0x01], Error::RlpMalformed, "string shorter than its length"),
    (&[0xb8], Error::RlpMalformed, "long form without its length"),
    (&[0xb9, 0x01], Error::RlpMalformed, "long form with half its length"),
    (&[0xb8, 0x38], Error::RlpMalformed, "56-byte string with no payload"),
    (&[0xbf, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff], Error::RlpMalformed, "length 2^64 - 1"),
    (&[0xc1], Error::RlpMalformed, "list shorter than its length"),
    (&[0xc2, 0x83, 0x01], Error::RlpMalformed, "item overruns its list"),
    (&[0xc1, 0x00, 0x00], Error::RlpMalformed, "bytes after the item"),
    (&[0x80, 0x80], Error::RlpMalformed, "two items"),
    (&[0xc2, 0x81, 0x05], Error::RlpNonCanonical, "non-canonical item inside a list"),
    (&[0xc3, 0xc2, 0x83, 0x01], Error::RlpMalformed, "item two levels down overruns its list"),
    (&[0xc4, 0xc1, 0x83, 0x01, 0x02], Error::RlpMalformed, "item overruns its list but not the outer one"),
];

#[test]
fn corpus_of_non_canonical_and_malformed_encodings_is_refused_by_both() {
    for (input, code, what) in CORPUS {
        assert_eq!(ferrata_decode(input), Err(*code), "{what}");
        assert!(alloy_decode(input).is_err(), "alloy accepts: {what}");
    }
}

#[test]
fn integers_are_canonical() {
    let int = |bytes: &[u8]| decode(bytes).unwrap().u64();
    assert_eq!(int(&[0x80]), Ok(0));
    assert_eq!(int(&[0x05]), Ok(5));
    assert_eq!(int(&[0x81, 0x80]), Ok(0x80));
    assert_eq!(int(&[0x88, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]), Ok(u64::MAX));
    assert_eq!(int(&[0x00]), Err(Error::RlpNonCanonical), "zero as 0x00");
    assert_eq!(int(&[0x82, 0x00, 0x01]), Err(Error::RlpNonCanonical), "leading zero");
    assert_eq!(int(&[0x89, 1, 0, 0, 0, 0, 0, 0, 0, 0]), Err(Error::RlpMalformed), "2^64");
    assert_eq!(decode(&[0xc0]).unwrap().u64(), Err(Error::RlpMalformed), "a list");
    // alloy agrees on every refusal.
    for bad in [&[0x00][..], &[0x82, 0x00, 0x01], &[0x89, 1, 0, 0, 0, 0, 0, 0, 0, 0]] {
        assert!(u64::decode(&mut &bad[..]).is_err());
    }
    // 256-bit words.
    let w = decode(&enc::uint(&[0xff; 32])).unwrap().word().unwrap();
    assert_eq!(w, [0xff; 32]);
    let mut five = [0u8; 32];
    five[31] = 5;
    assert_eq!(decode(&[0x05]).unwrap().word(), Ok(five));
    assert_eq!(decode(&[0x80]).unwrap().word(), Ok([0; 32]));
    assert_eq!(decode(&enc::bytes(&[1; 33])).unwrap().word(), Err(Error::RlpMalformed), "33 bytes");
    assert_eq!(decode(&enc::bytes(&[0, 1])).unwrap().word(), Err(Error::RlpNonCanonical));
}

#[test]
fn typed_accessors() {
    let s = decode(&[0x82, 1, 2]).unwrap();
    assert_eq!(s.fixed::<2>(), Ok([1, 2]));
    assert_eq!(s.fixed::<3>(), Err(Error::RlpMalformed));
    assert_eq!(s.list().err(), Some(Error::RlpMalformed));
    let l = decode(&[0xc2, 0x01, 0x02]).unwrap();
    assert_eq!(l.bytes(), Err(Error::RlpMalformed));
    assert_eq!(l.fixed::<2>(), Err(Error::RlpMalformed));
    let [a, b] = l.items::<2>().unwrap();
    assert_eq!((a.payload, b.payload), (&[1][..], &[2][..]));
    assert_eq!(l.items::<1>().err(), Some(Error::RlpMalformed), "too many items");
    assert_eq!(l.items::<3>().err(), Some(Error::RlpMalformed), "too few items");
    // The iterator stops after an error.
    let bad = [0xc3, 0x01, 0x82, 0x01];
    let mut it = split_first(&bad).map(|(i, _)| i).unwrap().list().unwrap();
    assert_eq!(it.next(), Some(Ok(decode(&[0x01]).unwrap())));
    assert_eq!(it.next(), Some(Err(Error::RlpMalformed)));
    assert_eq!(it.next(), None);
}

#[test]
fn encode_u64_matches_alloy() {
    for n in [0u64, 1, 0x7f, 0x80, 0xff, 0x100, 0xffff, 0x1_0000, u64::MAX >> 8, u64::MAX] {
        let (buf, len) = encode_u64(n);
        let mut want = Vec::new();
        n.encode(&mut want);
        assert_eq!(&buf[..len], want.as_slice(), "{n}");
        assert_eq!(decode(&buf[..len]).unwrap().u64(), Ok(n));
    }
}

#[test]
fn deep_nesting_is_decoded_one_level_at_a_time() {
    // 100,000 nested lists: decoding is lazy, so no level is visited until asked for, and a
    // descent never recurses.
    let depth = 100_000;
    let mut bytes = vec![0xc0];
    for _ in 0..depth {
        bytes = enc::list(&[bytes]);
    }
    let mut item = decode(&bytes).unwrap();
    let mut levels = 0;
    while let Some(inner) = item.list().unwrap().next() {
        item = inner.unwrap();
        levels += 1;
    }
    assert_eq!(levels, depth);
    // A list deep inside that claims more bytes than its parent holds is refused when reached.
    let mut broken = vec![0xc5, 0x01];
    for _ in 0..50 {
        broken = enc::list(&[broken]);
    }
    let mut item = decode(&broken).unwrap();
    let err = loop {
        match item.list().unwrap().next() {
            Some(Ok(inner)) => item = inner,
            Some(Err(e)) => break e,
            None => panic!("reached the bottom"),
        }
    };
    assert_eq!(err, Error::RlpMalformed);
}

fn random_tree(rng: &mut Rng, depth: usize) -> Tree {
    if depth == 0 || rng.below(3) > 0 {
        let len = match rng.below(6) {
            0 => 0,
            1 => 1,
            2 => rng.below(56),
            3 => 55 + rng.below(3),
            4 => 56 + rng.below(300),
            _ => rng.below(8),
        };
        let mut b = rng.bytes(len);
        if len == 1 && rng.below(2) == 0 {
            b[0] &= 0x7f;
        }
        Tree::Bytes(b)
    } else {
        Tree::List((0..rng.below(6)).map(|_| random_tree(rng, depth - 1)).collect())
    }
}

#[test]
fn random_round_trips_agree_with_alloy() {
    let mut rng = Rng::new(0x5eed_0001);
    for _ in 0..3_000 {
        let t = random_tree(&mut rng, 4);
        let bytes = encode(&t);
        assert_eq!(bytes, alloy_encode(&t), "encoders differ");
        assert_eq!(ferrata_decode(&bytes).as_ref(), Ok(&t));
        assert_eq!(alloy_decode(&bytes).as_ref(), Ok(&t));
    }
}

#[test]
fn random_mutations_get_the_same_verdict_from_both() {
    let mut rng = Rng::new(0x5eed_0002);
    let (mut accepted, mut refused) = (0, 0);
    for _ in 0..20_000 {
        let mut bytes = encode(&random_tree(&mut rng, 3));
        match rng.below(5) {
            0 => {
                let i = rng.below(bytes.len());
                bytes[i] ^= 1 << rng.below(8);
            }
            1 => {
                let i = rng.below(bytes.len());
                bytes[i] = rng.next_u64() as u8;
            }
            2 => {
                let i = rng.below(bytes.len() + 1);
                bytes.insert(i, rng.next_u64() as u8);
            }
            3 => {
                bytes.remove(rng.below(bytes.len()));
            }
            _ => bytes.truncate(rng.below(bytes.len())),
        }
        let (ours, theirs) = (ferrata_decode(&bytes), alloy_decode(&bytes));
        assert_eq!(ours.is_ok(), theirs.is_ok(), "{bytes:02x?}: ferrata {ours:?}, alloy {theirs:?}");
        if let (Ok(a), Ok(b)) = (&ours, &theirs) {
            assert_eq!(a, b);
            assert_eq!(encode(a), bytes, "an accepted encoding is the canonical one");
            accepted += 1;
        } else {
            refused += 1;
        }
    }
    println!("mutated encodings: {accepted} accepted, {refused} refused, by both");
    assert!(accepted > 1_000 && refused > 1_000);
}
