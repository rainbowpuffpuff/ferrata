//! Execution-layer objects from arbitrary bytes: block headers, accounts, storage values,
//! receipts and their logs, and the log-index arithmetic.
//!
//! * Accounts, storage values, receipts and logs are compared with reference decoders written
//!   with alloy-rlp's primitives (`Header`, `u64`, `U256`, `bool`): the same accept/reject and
//!   the same fields, apart from ferrata's documented extra rules (an account's zero storage
//!   root or code hash, a zero storage value, a 32-byte pre-Byzantium status, receipt types
//!   above 4, more than four topics), each excluded by its own predicate.
//! * A header decodes with its own hash exactly when it decodes, to the same header, and never
//!   with another hash.
//! * `block_log_index` and `locate_log` are inverse on any counts.
#![no_main]
use alloy_primitives::U256;
use alloy_rlp::{Decodable, Header};
use ferrata_core::{
    exec::{
        block_log_index, decode_header, decode_header_with_hash, decode_storage_value, locate_log, Account, Receipt,
    },
    keccak::keccak256,
};
use libfuzzer_sys::fuzz_target;

fn exact<T: Decodable>(buf: &mut &[u8]) -> Option<T> {
    T::decode(buf).ok()
}

fn bytes<'a>(buf: &mut &'a [u8]) -> Option<&'a [u8]> {
    Header::decode_bytes(buf, false).ok()
}

fn list<'a>(buf: &mut &'a [u8]) -> Option<&'a [u8]> {
    Header::decode_bytes(buf, true).ok()
}

fn fixed<const N: usize>(buf: &mut &[u8]) -> Option<[u8; N]> {
    bytes(buf)?.try_into().ok()
}

/// (nonce, balance, storage root, code hash), alloy-rlp's reading.
type RefAccount = (u64, [u8; 32], [u8; 32], [u8; 32]);

fn ref_account(enc: &[u8]) -> Option<RefAccount> {
    let mut buf = enc;
    let mut p = list(&mut buf)?;
    let out = (exact::<u64>(&mut p)?, exact::<U256>(&mut p)?.to_be_bytes::<32>(), fixed(&mut p)?, fixed(&mut p)?);
    (buf.is_empty() && p.is_empty()).then_some(out)
}

type RefLog = ([u8; 20], Vec<[u8; 32]>, Vec<u8>);
type RefReceipt = (u8, bool, u64, Vec<u8>, Vec<RefLog>);

fn ref_receipt(enc: &[u8]) -> Option<RefReceipt> {
    let (&first, rest) = enc.split_first()?;
    let (ty, mut buf) = match first {
        0x01..=0x04 => (first, rest),
        0xc0..=0xff => (0, enc),
        _ => return None,
    };
    let mut p = list(&mut buf)?;
    if !buf.is_empty() {
        return None;
    }
    let status = exact::<bool>(&mut p)?;
    let gas = exact::<u64>(&mut p)?;
    let bloom = fixed::<256>(&mut p)?.to_vec();
    let mut logs_payload = list(&mut p)?;
    if !p.is_empty() {
        return None;
    }
    let mut logs = Vec::new();
    while !logs_payload.is_empty() {
        let mut l = list(&mut logs_payload)?;
        let address = fixed::<20>(&mut l)?;
        let mut t = list(&mut l)?;
        let mut topics = Vec::new();
        while !t.is_empty() {
            topics.push(fixed::<32>(&mut t)?);
        }
        let data = bytes(&mut l)?.to_vec();
        if !l.is_empty() {
            return None;
        }
        logs.push((address, topics, data));
    }
    Some((ty, status, gas, bloom, logs))
}

/// The receipt carries a feature ferrata refuses on purpose (see the module comment).
fn ferrata_refuses_on_purpose(enc: &[u8], r: &RefReceipt) -> bool {
    let _ = enc;
    r.4.iter().any(|(_, topics, _)| topics.len() > 4)
}

fuzz_target!(|data: &[u8]| {
    // Headers.
    let h = decode_header(data);
    let hash = keccak256(data);
    assert_eq!(decode_header_with_hash(data, &hash).ok(), h.clone().ok());
    let mut other = hash;
    other[0] ^= 1;
    assert!(decode_header_with_hash(data, &other).is_err());

    // Accounts.
    let ours = Account::decode(data).ok().map(|a| (a.nonce, a.balance, a.storage_root, a.code_hash));
    let theirs = ref_account(data).filter(|a| a.2 != [0; 32] && a.3 != [0; 32]);
    assert_eq!(ours, theirs, "account differs from alloy-rlp");

    // Storage values.
    let ours = decode_storage_value(data).ok();
    let mut buf = data;
    let theirs = exact::<U256>(&mut buf).filter(|v| buf.is_empty() && !v.is_zero()).map(|v| v.to_be_bytes::<32>());
    assert_eq!(ours, theirs, "storage value differs from alloy-rlp");

    // Receipts and logs.
    let ours = Receipt::decode(data);
    let theirs = ref_receipt(data);
    match (&ours, &theirs) {
        (Ok(r), Some(t)) => {
            assert_eq!(
                (r.tx_type, r.status, r.cumulative_gas_used, r.logs_bloom.to_vec()),
                (t.0, t.1, t.2, t.3.clone())
            );
            assert_eq!(r.log_count(), t.4.len());
            for (k, (address, topics, d)) in t.4.iter().enumerate() {
                let log = r.log(k as u64).expect("a counted log is readable");
                assert_eq!((&log.address, log.topics(), log.data), (address, &topics[..], &d[..]));
            }
            assert!(r.log(r.log_count() as u64).is_err());
            assert!(r.log(u64::MAX).is_err());
        }
        (Err(_), None) => {}
        (Err(e), Some(t)) => {
            assert!(ferrata_refuses_on_purpose(data, t), "ferrata refuses a receipt alloy reads: {e:?}")
        }
        (Ok(r), None) => panic!("ferrata reads a receipt alloy refuses: {r:?}"),
    }

    // Log-index arithmetic on counts taken from the input.
    let counts: Vec<u64> = data.chunks(2).take(16).map(|c| u64::from(c[0]) % 5 * u64::from(c.len() as u8)).collect();
    let g = u64::from(data.first().copied().unwrap_or(0));
    if let Some((tx, pos)) = locate_log(&counts, g) {
        assert_eq!(block_log_index(&counts, tx, pos), Some(g));
    }
    let tx = u64::from(data.get(1).copied().unwrap_or(0) % 20);
    let pos = u64::from(data.get(2).copied().unwrap_or(0) % 6);
    if let Some(g) = block_log_index(&counts, tx, pos) {
        assert_eq!(locate_log(&counts, g), Some((tx, pos)));
    }
    let big: Vec<u64> = data
        .chunks(8)
        .take(8)
        .map(|c| u64::from_le_bytes(core::array::from_fn(|i| c.get(i).copied().unwrap_or(0))))
        .collect();
    let _ = locate_log(&big, g.wrapping_mul(0x0101_0101_0101_0101));
    let _ = block_log_index(&big, tx, u64::MAX - pos);
});
