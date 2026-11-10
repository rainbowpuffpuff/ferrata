//! Execution objects and the attestation API, on synthetic data: every decoder branch (headers
//! of each shape, accounts, storage values, receipts of each type, logs), predicates, record
//! constructors and encoding, statement identifiers (pinned against an independent
//! computation), single and batched attestations.

use ferrata_core::{
    attest::{
        account_id, attest_account, attest_batch, attest_log, attest_storage, log_hash, log_id, storage_id,
        AccountQuery, Batch, Binding, Cmp, Condition, Fact, Field, HeaderRecord, LogQuery, Predicate, SlotQuery,
    },
    bls::{Aggregation, Blst},
    exec::{decode_header, decode_header_with_hash, decode_storage_value, Account, HeaderShape, Receipt, KECCAK_EMPTY},
    fork::LcFork,
    keccak::keccak256,
    merkle::ZERO,
    mpt::EMPTY_ROOT,
    policy::FinalizedStore,
    store::StoreCore,
    types::{BeaconBlockHeader, Execution, LightClientHeader, SyncCommittee},
    Error,
};
use ferrata_spectests::{
    execkit::{
        account_rlp, enc, header, header_items, keccak, log_rlp, receipt_with, record_ssz, w, World, NOBODY, PLAIN,
        SMALL, TOKEN, TRANSFER,
    },
    testkit::{fabricated_header, fake_committee},
};

#[test]
fn keccak_matches_two_other_implementations() {
    assert_eq!(hex::encode(keccak256(b"")), "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470");
    assert_eq!(keccak256(b""), KECCAK_EMPTY);
    assert_eq!(keccak256(&[0x80]), EMPTY_ROOT);
    // Around the 136-byte rate, and long inputs.
    for len in [1, 31, 32, 33, 135, 136, 137, 271, 272, 273, 1000, 10_000] {
        let data: Vec<u8> = (0..len).map(|i| (i * 7 + 3) as u8).collect();
        assert_eq!(keccak256(&data), keccak(&data), "sha3, {len} bytes");
        assert_eq!(keccak256(&data), alloy_primitives::keccak256(&data).0, "alloy, {len} bytes");
    }
}

#[test]
fn header_shapes() {
    let (s, r) = ([1; 32], [2; 32]);
    let cancun = decode_header(&header(10, s, r, 20)).unwrap();
    assert_eq!(
        (cancun.shape, cancun.requests_hash, cancun.block_access_list_hash, cancun.slot_number),
        (HeaderShape::Cancun, None, None, None)
    );
    assert_eq!((cancun.number, cancun.state_root, cancun.receipts_root), (10, s, r));
    assert_eq!(cancun.timestamp, 1_700_000_000 + 120);
    assert_eq!((cancun.gas_limit, cancun.gas_used, cancun.blob_gas_used), (36_000_000, 12_345_678, 131_072));
    assert_eq!((cancun.beneficiary, cancun.nonce, cancun.parent_beacon_block_root), ([0x11; 20], [0; 8], [0x55; 32]));
    assert_eq!((cancun.difficulty, cancun.base_fee_per_gas), ([0; 32], w(7)));
    let prague = decode_header(&header(10, s, r, 21)).unwrap();
    assert_eq!(
        (prague.shape, prague.requests_hash, prague.block_access_list_hash),
        (HeaderShape::Prague, Some([0x66; 32]), None)
    );
    let amsterdam = decode_header(&header(10, s, r, 23)).unwrap();
    assert_eq!(amsterdam.shape, HeaderShape::Amsterdam);
    assert_eq!((amsterdam.block_access_list_hash, amsterdam.slot_number), (Some([0x77; 32]), Some(15_000_000)));
    // Pre-Cancun shapes (London 16, Shanghai 17), Amsterdam without slot_number (22), and any
    // unknown trailing field (24) are refused.
    for fields in [15, 16, 17, 19, 22, 24, 30] {
        assert_eq!(decode_header(&header(10, s, r, fields)), Err(Error::HeaderUnsupported), "{fields} fields");
    }
}

fn with_item(fields: usize, i: usize, item: Vec<u8>) -> Vec<u8> {
    let mut items = header_items(10, [1; 32], [2; 32], fields);
    items[i] = item;
    enc::list(&items)
}

#[test]
fn header_fields_are_strict() {
    let cases: [(usize, Vec<u8>, Error, &str); 16] = [
        (0, enc::bytes(&[1; 31]), Error::RlpMalformed, "parent_hash 31 bytes"),
        (2, enc::bytes(&[1; 21]), Error::RlpMalformed, "beneficiary 21 bytes"),
        (3, enc::list(&[]), Error::RlpMalformed, "state_root a list"),
        (6, enc::bytes(&[0; 255]), Error::RlpMalformed, "logs_bloom 255 bytes"),
        (7, enc::bytes(&[1; 33]), Error::RlpMalformed, "difficulty above 256 bits"),
        (8, enc::bytes(&[0, 10]), Error::RlpNonCanonical, "number with a leading zero"),
        (8, enc::bytes(&[1; 9]), Error::RlpMalformed, "number above 64 bits"),
        (11, enc::list(&[]), Error::RlpMalformed, "timestamp a list"),
        (12, enc::bytes(&[7; 33]), Error::RlpMalformed, "extra_data 33 bytes"),
        (13, enc::bytes(&[1; 33]), Error::RlpMalformed, "mix_hash 33 bytes"),
        (14, enc::bytes(&[0; 7]), Error::RlpMalformed, "nonce 7 bytes"),
        (15, enc::bytes(&[0, 7]), Error::RlpNonCanonical, "base fee with a leading zero"),
        (17, enc::bytes(&[1; 9]), Error::RlpMalformed, "blob_gas_used above 64 bits"),
        (19, enc::bytes(&[1; 31]), Error::RlpMalformed, "parent_beacon_block_root 31 bytes"),
        (20, enc::bytes(&[1; 31]), Error::RlpMalformed, "requests_hash 31 bytes"),
        (21, enc::bytes(&[1; 33]), Error::RlpMalformed, "block_access_list_hash 33 bytes"),
    ];
    for (i, item, code, what) in cases {
        assert_eq!(decode_header(&with_item(23, i, item)), Err(code), "{what}");
    }
    assert_eq!(decode_header(&with_item(23, 22, enc::bytes(&[1; 9]))), Err(Error::RlpMalformed), "slot_number");
    assert!(decode_header(&with_item(23, 12, enc::bytes(&[7; 32]))).is_ok(), "extra_data of 32 bytes");
    let good = header(10, [1; 32], [2; 32], 21);
    assert_eq!(decode_header(&[good.clone(), vec![0x00]].concat()), Err(Error::RlpMalformed), "trailing byte");
    assert_eq!(decode_header(&enc::bytes(&good)), Err(Error::RlpMalformed), "a string, not a list");
    // The hash is checked first, against the whole encoding.
    let hash = keccak(&good);
    assert!(decode_header_with_hash(&good, &hash).is_ok());
    let mut other = hash;
    other[31] ^= 1;
    assert_eq!(decode_header_with_hash(&good, &other), Err(Error::HeaderHashMismatch));
    assert_eq!(decode_header_with_hash(&good[..good.len() - 1], &hash), Err(Error::HeaderHashMismatch));
}

#[test]
fn accounts_and_storage_values() {
    let a = Account::decode(&account_rlp(3, 1_000, [7; 32], [8; 32])).unwrap();
    assert_eq!((a.nonce, a.balance, a.storage_root, a.code_hash), (3, w(1000), [7; 32], [8; 32]));
    assert!(a.exists() && !Account::ABSENT.exists());
    let item = |i: usize, v: Vec<u8>| {
        let mut items = vec![enc::u64(3), enc::u64(1000), enc::bytes(&[7; 32]), enc::bytes(&[8; 32])];
        items[i] = v;
        Account::decode(&enc::list(&items))
    };
    assert_eq!(item(0, enc::bytes(&[1; 9])), Err(Error::RlpMalformed), "nonce above 2^64");
    assert_eq!(item(0, vec![0x00]), Err(Error::RlpNonCanonical), "nonce 0x00");
    assert_eq!(item(1, enc::bytes(&[1; 33])), Err(Error::RlpMalformed), "balance above 2^256");
    assert_eq!(item(1, enc::bytes(&[0, 1])), Err(Error::RlpNonCanonical), "balance with a leading zero");
    assert_eq!(item(2, enc::bytes(&[7; 31])), Err(Error::RlpMalformed), "storage root 31 bytes");
    assert_eq!(item(3, enc::bytes(&[8; 33])), Err(Error::RlpMalformed), "code hash 33 bytes");
    assert_eq!(item(2, enc::bytes(&[0; 32])), Err(Error::RlpMalformed), "zero storage root");
    assert_eq!(item(3, enc::bytes(&[0; 32])), Err(Error::RlpMalformed), "zero code hash");
    let three = enc::list(&[enc::u64(3), enc::u64(1000), enc::bytes(&[7; 32])]);
    assert_eq!(Account::decode(&three), Err(Error::RlpMalformed));
    let five = enc::list(&[enc::u64(3), enc::u64(1000), enc::bytes(&[7; 32]), enc::bytes(&[8; 32]), enc::u64(0)]);
    assert_eq!(Account::decode(&five), Err(Error::RlpMalformed));

    assert_eq!(decode_storage_value(&[0x05]), Ok(w(5)));
    assert_eq!(decode_storage_value(&enc::uint(&[0xff; 32])), Ok([0xff; 32]));
    assert_eq!(decode_storage_value(&[0x80]), Err(Error::RlpMalformed), "zero is never stored");
    assert_eq!(decode_storage_value(&[0x00]), Err(Error::RlpNonCanonical));
    assert_eq!(decode_storage_value(&enc::bytes(&[0, 5])), Err(Error::RlpNonCanonical));
    assert_eq!(decode_storage_value(&enc::bytes(&[1; 33])), Err(Error::RlpMalformed));
    assert_eq!(decode_storage_value(&enc::list(&[vec![0x05]])), Err(Error::RlpMalformed));
    assert_eq!(decode_storage_value(&[0x05, 0x05]), Err(Error::RlpMalformed));
}

fn a_log(topics: usize) -> Vec<u8> {
    log_rlp([0x11; 20], &vec![TRANSFER; topics], &w(1000))
}

#[test]
fn receipts_of_every_type_and_their_refusals() {
    for tx_type in 0u8..=4 {
        let r = receipt_with(tx_type, enc::u64(1), 50_000, &[a_log(3), a_log(0)]);
        let receipt = Receipt::decode(&r).unwrap();
        assert_eq!((receipt.tx_type, receipt.status, receipt.cumulative_gas_used), (tx_type, true, 50_000));
        assert_eq!((receipt.log_count(), receipt.logs_bloom.len()), (2, 256));
        let log = receipt.log(0).unwrap();
        assert_eq!((log.address, log.topics(), log.data), ([0x11; 20], &[TRANSFER; 3][..], &w(1000)[..]));
        assert!(receipt.log(1).unwrap().topics().is_empty());
        assert_eq!(receipt.log(2), Err(Error::LogIndexOutOfRange));
        assert_eq!(receipt.log(u64::MAX), Err(Error::LogIndexOutOfRange));
    }
    assert!(!Receipt::decode(&receipt_with(2, enc::u64(0), 1, &[])).unwrap().status);
    let body = || receipt_with(0, enc::u64(1), 1, &[]);
    for prefix in [0x00u8, 0x05, 0x7f, 0x80, 0xb7] {
        assert_eq!(Receipt::decode(&[vec![prefix], body()].concat()), Err(Error::ReceiptUnsupported), "{prefix:#x}");
    }
    assert_eq!(Receipt::decode(&[]), Err(Error::RlpMalformed));
    // Pre-Byzantium: an intermediate state root where the status belongs.
    assert_eq!(Receipt::decode(&receipt_with(0, enc::bytes(&[9; 32]), 1, &[])), Err(Error::ReceiptUnsupported));
    assert_eq!(Receipt::decode(&receipt_with(0, enc::u64(2), 1, &[])), Err(Error::RlpMalformed), "status 2");
    assert_eq!(Receipt::decode(&receipt_with(2, vec![0x00], 1, &[])), Err(Error::RlpNonCanonical), "status 0x00");
    assert_eq!(Receipt::decode(&receipt_with(2, enc::bytes(&[1; 31]), 1, &[])), Err(Error::RlpMalformed));
    let fields = |items: Vec<Vec<u8>>| Receipt::decode(&[vec![2], enc::list(&items)].concat()).map(|_| ());
    let ok = || vec![enc::u64(1), enc::u64(1), enc::bytes(&[0; 256]), enc::list(&[])];
    let mut three = ok();
    three.pop();
    assert_eq!(fields(three), Err(Error::RlpMalformed), "three fields");
    let mut five = ok();
    five.push(enc::u64(0));
    assert_eq!(fields(five), Err(Error::RlpMalformed), "five fields");
    let mut bloom = ok();
    bloom[2] = enc::bytes(&[0; 255]);
    assert_eq!(fields(bloom), Err(Error::RlpMalformed), "bloom 255 bytes");
    let mut gas = ok();
    gas[1] = enc::bytes(&[1; 9]);
    assert_eq!(fields(gas), Err(Error::RlpMalformed), "gas above 64 bits");
    let mut logs = ok();
    logs[3] = enc::bytes(&[]);
    assert_eq!(fields(logs), Err(Error::RlpMalformed), "logs a string");
    assert_eq!(Receipt::decode(&[receipt_with(2, enc::u64(1), 1, &[]), vec![0]].concat()), Err(Error::RlpMalformed));
    // Every log is checked, even ones nobody asks for.
    let bad_logs: [(Vec<u8>, &str); 5] = [
        (a_log(5), "five topics"),
        (enc::list(&[enc::bytes(&[0x11; 19]), enc::list(&[]), enc::bytes(&[])]), "address 19 bytes"),
        (enc::list(&[enc::bytes(&[0x11; 20]), enc::list(&[enc::bytes(&[1; 31])]), enc::bytes(&[])]), "topic 31 bytes"),
        (enc::list(&[enc::bytes(&[0x11; 20]), enc::list(&[])]), "two fields"),
        (enc::list(&[enc::bytes(&[0x11; 20]), enc::list(&[]), enc::list(&[])]), "data a list"),
    ];
    for (log, what) in bad_logs {
        assert_eq!(
            Receipt::decode(&receipt_with(2, enc::u64(1), 1, &[a_log(1), log])),
            Err(Error::RlpMalformed),
            "{what}"
        );
    }
    assert!(Receipt::decode(&receipt_with(2, enc::u64(1), 1, &[a_log(4)])).is_ok(), "four topics");
}

#[test]
fn predicates_compare_unsigned_256_bit_words() {
    let p = |cmp, rhs: u128| Predicate { cmp, rhs: w(rhs) };
    let table = [
        (Cmp::Eq, [false, true, false]),
        (Cmp::Ne, [true, false, true]),
        (Cmp::Lt, [true, false, false]),
        (Cmp::Le, [true, true, false]),
        (Cmp::Gt, [false, false, true]),
        (Cmp::Ge, [false, true, true]),
    ];
    for (cmp, want) in table {
        for (value, expected) in [99u128, 100, 101].into_iter().zip(want) {
            assert_eq!(p(cmp, 100).holds(&w(value)), expected, "{value} {cmp:?} 100");
        }
    }
    // Big-endian words compare as integers, across the full width.
    let max = Predicate { cmp: Cmp::Gt, rhs: [0xff; 32] };
    assert!(!max.holds(&[0xff; 32]));
    let mut high = [0u8; 32];
    high[0] = 1;
    assert!(Predicate { cmp: Cmp::Gt, rhs: [0; 32] }.holds(&high));
    assert!(Predicate { cmp: Cmp::Lt, rhs: high }.holds(&w(u128::MAX)));
}

#[test]
fn fact_fields_and_conditions() {
    let account = Fact::Account(ferrata_core::attest::AccountFact {
        address: [1; 20],
        account: Account { nonce: 7, balance: w(500), storage_root: [2; 32], code_hash: [3; 32] },
    });
    assert_eq!(account.field(Field::Nonce), Ok(w(7)));
    assert_eq!(account.field(Field::Balance), Ok(w(500)));
    assert_eq!(account.field(Field::StorageRoot), Ok([2; 32]));
    assert_eq!(account.field(Field::CodeHash), Ok([3; 32]));
    for f in [Field::Value, Field::Emitter, Field::Topic(0), Field::DataWord(0)] {
        assert_eq!(account.field(f), Err(Error::QueryInvalid), "{f:?}");
    }
    let storage = Fact::Storage(ferrata_core::attest::StorageFact { address: [1; 20], slot: w(1), value: w(9) });
    assert_eq!(storage.field(Field::Value), Ok(w(9)));
    assert_eq!(storage.field(Field::Balance), Err(Error::QueryInvalid));
    let r = receipt_with(
        2,
        enc::u64(1),
        1,
        &[log_rlp([0x11; 20], &[TRANSFER, w(5)], &[w(1).as_slice(), &w(2), &[9; 5]].concat())],
    );
    let receipt = Receipt::decode(&r).unwrap();
    let log = Fact::Log(ferrata_core::attest::LogFact { tx_index: 0, log_index: 0, log: receipt.log(0).unwrap() });
    let mut emitter = [0u8; 32];
    emitter[12..].copy_from_slice(&[0x11; 20]);
    assert_eq!(log.field(Field::Emitter), Ok(emitter));
    assert_eq!(log.field(Field::Topic(0)), Ok(TRANSFER));
    assert_eq!(log.field(Field::Topic(1)), Ok(w(5)));
    assert_eq!(log.field(Field::Topic(2)), Err(Error::PredicateFailed));
    assert_eq!(log.field(Field::DataWord(0)), Ok(w(1)));
    assert_eq!(log.field(Field::DataWord(1)), Ok(w(2)));
    assert_eq!(log.field(Field::DataWord(2)), Err(Error::PredicateFailed), "only 5 bytes left");
    assert_eq!(log.field(Field::DataWord(u32::MAX)), Err(Error::PredicateFailed));
    assert_eq!(log.field(Field::Nonce), Err(Error::QueryInvalid));
    let cond = |field, cmp, rhs| Condition { field, predicate: Predicate { cmp, rhs } };
    assert_eq!(log.check(&[cond(Field::Topic(0), Cmp::Eq, TRANSFER), cond(Field::DataWord(0), Cmp::Ge, w(1))]), Ok(()));
    assert_eq!(
        log.check(&[cond(Field::Topic(0), Cmp::Eq, TRANSFER), cond(Field::DataWord(0), Cmp::Gt, w(1))]),
        Err(Error::PredicateFailed)
    );
    assert_eq!(log.check(&[]), Ok(()));
}

#[test]
fn record_encoding_round_trips_and_is_strict() {
    let bytes = record_ssz(1, 99, 1234, 5678, [1; 32], [3; 32], [4; 32], 2, 3);
    assert_eq!(bytes.len(), HeaderRecord::SSZ_LEN);
    let r = HeaderRecord::from_ssz(&bytes).unwrap();
    assert_eq!((r.chain_id(), r.anchor_slot(), r.number(), r.timestamp()), (1, 99, 1234, 5678));
    assert_eq!((r.block_hash(), r.state_root(), r.receipts_root()), ([1; 32], [3; 32], [4; 32]));
    assert_eq!(r.parent_hash(), keccak(&1233u64.to_be_bytes()));
    assert_eq!(r.binding(), Binding::History { hops: 3 });
    assert_eq!(r.to_ssz().to_vec(), bytes);
    for (tag, hops) in [(0u8, 0u32), (1, 0)] {
        let b = record_ssz(1, 99, 1234, 5678, [1; 32], [3; 32], [4; 32], tag, hops);
        assert_eq!(HeaderRecord::from_ssz(&b).unwrap().to_ssz().to_vec(), b);
    }
    for (tag, hops) in [(0u8, 1u32), (1, 1), (2, 0), (3, 0), (0xff, 0)] {
        let b = record_ssz(1, 99, 1234, 5678, [1; 32], [3; 32], [4; 32], tag, hops);
        assert_eq!(HeaderRecord::from_ssz(&b), Err(Error::Malformed), "tag {tag} hops {hops}");
    }
    assert_eq!(HeaderRecord::from_ssz(&bytes[1..]), Err(Error::Malformed));
    assert_eq!(HeaderRecord::from_ssz(&[bytes.clone(), vec![0]].concat()), Err(Error::Malformed));
}

fn store_with(execution: Execution, fork: LcFork) -> FinalizedStore {
    let fake = fake_committee(32, 9);
    let core = StoreCore {
        fork,
        finalized_header: LightClientHeader {
            beacon: BeaconBlockHeader { slot: 777, ..Default::default() },
            execution,
        },
        current_sync_committee: fake.committee,
        next_sync_committee: SyncCommittee::empty(32),
    };
    FinalizedStore::from_core(core, &Blst::new(Aggregation::Auto)).unwrap()
}

#[test]
fn records_come_from_the_finalized_header() {
    // Capella to Fulu: the payload header's fields.
    let electra = fabricated_header(777, 5);
    let Execution::Payload { header: payload, .. } = &electra.execution else { unreachable!() };
    let store = store_with(electra.execution.clone(), LcFork::Electra);
    let r = HeaderRecord::from_payload_header(&store, 1).unwrap();
    assert_eq!(
        (r.chain_id(), r.anchor_slot(), r.number(), r.timestamp()),
        (1, 777, payload.block_number, payload.timestamp)
    );
    assert_eq!((r.block_hash(), r.parent_hash()), (payload.block_hash, payload.parent_hash));
    assert_eq!(
        (r.state_root(), r.receipts_root(), r.binding()),
        (payload.state_root, payload.receipts_root, Binding::PayloadHeader)
    );
    assert_eq!(HeaderRecord::from_block_hash(&store, 1, &[]), Err(Error::BindingMismatch));
    // A Capella-layout header before Capella carries an empty payload.
    let empty = LightClientHeader::empty(LcFork::Electra);
    let store = store_with(empty.execution, LcFork::Electra);
    assert_eq!(HeaderRecord::from_payload_header(&store, 1), Err(Error::BindingMismatch));
    // Gloas: the RLP header that hashes to the bound block hash.
    let rlp = header(4242, [1; 32], [2; 32], 23);
    let branch = vec![ZERO; LcFork::Gloas.execution_branch_len()];
    let store = store_with(Execution::BlockHash { block_hash: keccak(&rlp), branch: branch.clone() }, LcFork::Gloas);
    let r = HeaderRecord::from_block_hash(&store, 11155111, &rlp).unwrap();
    assert_eq!((r.chain_id(), r.anchor_slot(), r.number(), r.block_hash()), (11155111, 777, 4242, keccak(&rlp)));
    assert_eq!((r.state_root(), r.receipts_root(), r.binding()), ([1; 32], [2; 32], Binding::BlockHash));
    assert_eq!(r.parent_hash(), keccak(&4242u64.to_be_bytes()));
    assert_eq!(
        HeaderRecord::from_block_hash(&store, 1, &header(4242, [9; 32], [2; 32], 23)),
        Err(Error::HeaderHashMismatch)
    );
    assert_eq!(HeaderRecord::from_payload_header(&store, 1), Err(Error::BindingMismatch));
    // A bound hash that is a 22-field header's: the shape is refused after the hash matches.
    let odd = header(4242, [1; 32], [2; 32], 22);
    let store = store_with(Execution::BlockHash { block_hash: keccak(&odd), branch: branch.clone() }, LcFork::Gloas);
    assert_eq!(HeaderRecord::from_block_hash(&store, 1, &odd), Err(Error::HeaderUnsupported));
    let store = store_with(Execution::BlockHash { block_hash: ZERO, branch }, LcFork::Gloas);
    assert_eq!(HeaderRecord::from_block_hash(&store, 1, &[0x80]), Err(Error::BindingMismatch));
    // Altair: no execution data at all.
    let store = store_with(Execution::None, LcFork::Altair);
    assert_eq!(HeaderRecord::from_payload_header(&store, 1), Err(Error::BindingMismatch));
    assert_eq!(HeaderRecord::from_block_hash(&store, 1, &rlp), Err(Error::BindingMismatch));
}

#[test]
fn identifiers_are_pinned_and_depend_only_on_the_fact() {
    // Values computed by an independent implementation of the documented derivation.
    let account = Account { nonce: 5, balance: w(1000), storage_root: [0x22; 32], code_hash: [0x33; 32] };
    assert_eq!(
        hex::encode(account_id(1, &[0x11; 20], &account)),
        "0d8ca291e33d155b51078a942cdb128354cc58653e08cb462ee0d888bf444ffa"
    );
    assert_eq!(
        hex::encode(account_id(1, &[0x11; 20], &Account::ABSENT)),
        "530955dc34541c6f596477461ad3ae9cffe3a8d76eaa810a8db546bb55fb9fff"
    );
    assert_eq!(
        hex::encode(storage_id(1, &[0x11; 20], &w(3), &w(42))),
        "115130e2fa2bf7c57d10ca71e93935fb9ed4a9b8d9784a8fab1b67ec11ff5284"
    );
    assert_eq!(
        hex::encode(storage_id(11155111, &[0x11; 20], &w(3), &w(42))),
        "1d7cc305a243165e57df5ce92d639bb81976ba3f4f725574dcf475904aad62ce"
    );
    let r = receipt_with(2, enc::u64(1), 1, &[log_rlp([0x11; 20], &[TRANSFER, w(1)], &w(1000))]);
    let receipt = Receipt::decode(&r).unwrap();
    let log = receipt.log(0).unwrap();
    assert_eq!(hex::encode(log_hash(&log)), "77ea482acc0d40edea6812ad2b6b3fb0a39453328b3a884b7b57a669935ecc87");
    assert_eq!(
        hex::encode(log_id(1, 100, 2, 1, &log)),
        "fa95ababc4724fabf11720f60cbe7366b175553662f8db0854eaf3f6224e4e29"
    );
    // Each field changes the identifier.
    let base = storage_id(1, &[0x11; 20], &w(3), &w(42));
    for other in [
        storage_id(2, &[0x11; 20], &w(3), &w(42)),
        storage_id(1, &[0x12; 20], &w(3), &w(42)),
        storage_id(1, &[0x11; 20], &w(4), &w(42)),
        storage_id(1, &[0x11; 20], &w(3), &w(43)),
    ] {
        assert_ne!(base, other);
    }
    let lid = log_id(1, 100, 2, 1, &log);
    for other in
        [log_id(5, 100, 2, 1, &log), log_id(1, 101, 2, 1, &log), log_id(1, 100, 3, 1, &log), log_id(1, 100, 2, 0, &log)]
    {
        assert_ne!(lid, other);
    }
}

#[test]
fn attestations_on_a_synthetic_block() {
    let world = World::new(1);
    let record = world.record(1, 500);
    // A storage slot, and the same fact read under another record (another block number and
    // binding): one identifier.
    let (slot, value) = world.slots[&TOKEN][3];
    let (ap, sp) = world.proofs(&TOKEN, &slot);
    let s = attest_storage(&record, 1, &TOKEN, &ap, &slot, &sp).unwrap();
    assert_eq!((s.fact.value, s.block_number, s.chain_id), (value, 500, 1));
    assert_eq!(s.id, storage_id(1, &TOKEN, &slot, &value));
    let later = HeaderRecord::from_ssz(&record_ssz(1, 1, 900, 0, [9; 32], world.state.root(), [0; 32], 2, 4)).unwrap();
    assert_eq!(attest_storage(&later, 1, &TOKEN, &ap, &slot, &sp).unwrap().id, s.id);
    // A slot holding a one-byte value (an embedded-size leaf value) and an absent slot.
    let (ap2, sp2) = world.proofs(&SMALL, &w(0));
    assert_eq!(attest_storage(&record, 1, &SMALL, &ap2, &w(0), &sp2).unwrap().fact.value, w(0x2a));
    let (_, absent_proof) = world.proofs(&TOKEN, &w(5));
    let z = attest_storage(&record, 1, &TOKEN, &ap, &w(5), &absent_proof).unwrap();
    assert_eq!(z.fact.value, [0; 32]);
    // An account without storage, and an address without an account (its slots are zero,
    // proven by the empty proof).
    let plain = attest_account(&record, 1, &PLAIN, &world.state.proof(&keccak(&PLAIN))).unwrap();
    assert_eq!((plain.fact.account.nonce, plain.fact.account.code_hash), (42, KECCAK_EMPTY));
    let nobody_proof = world.state.proof(&keccak(&NOBODY));
    let nobody = attest_account(&record, 1, &NOBODY, &nobody_proof).unwrap();
    assert_eq!(nobody.fact.account, Account::ABSENT);
    let empty: Vec<Vec<u8>> = vec![];
    assert_eq!(attest_storage(&record, 1, &NOBODY, &nobody_proof, &w(1), &empty).unwrap().fact.value, [0; 32]);
    assert_eq!(
        attest_storage(&record, 1, &PLAIN, &world.state.proof(&keccak(&PLAIN)), &w(1), &empty).unwrap().fact.value,
        [0; 32]
    );
    // Logs of every receipt type.
    for i in [1u64, 2, 3, 5, 6, 11, 13, 14] {
        let proof = world.receipt_proof(i);
        let l = attest_log(&record, 1, i, 0, &proof).unwrap();
        assert_eq!((l.fact.tx_index, l.fact.log_index, l.fact.log.address), (i, 0, TOKEN));
        assert_eq!(l.fact.log.topics()[0], TRANSFER);
        assert_eq!(l.id, log_id(1, 500, i, 0, &l.fact.log));
    }
    // Chain binding.
    assert_eq!(attest_storage(&record, 5, &TOKEN, &ap, &slot, &sp).unwrap_err(), Error::ChainMismatch);
    assert_eq!(
        attest_account(&record, 5, &PLAIN, &world.state.proof(&keccak(&PLAIN))).unwrap_err(),
        Error::ChainMismatch
    );
    assert_eq!(attest_log(&record, 5, 1, 0, &world.receipt_proof(1)).unwrap_err(), Error::ChainMismatch);
}

#[test]
fn a_batch_is_one_compound_statement() {
    let world = World::new(2);
    let record = world.record(1, 600);
    let cond = |field, cmp, rhs| Condition { field, predicate: Predicate { cmp, rhs } };
    let token_proof = world.state.proof(&keccak(&TOKEN));
    let small_proof = world.state.proof(&keccak(&SMALL));
    let (slot, value) = world.slots[&TOKEN][10];
    let slot_proof = world.storage[&TOKEN].proof(&keccak(&slot));
    let small_slot_proof = world.storage[&SMALL].proof(&keccak(&w(0)));
    let log_proof = world.receipt_proof(3);
    let small_ok = [cond(Field::Balance, Cmp::Ge, w(5))];
    let value_ok = [cond(Field::Value, Cmp::Eq, value)];
    let small_value = [cond(Field::Value, Cmp::Gt, w(0x29)), cond(Field::Value, Cmp::Lt, w(0x2b))];
    let transfer = [
        cond(Field::Topic(0), Cmp::Eq, TRANSFER),
        cond(Field::Emitter, Cmp::Eq, {
            let mut e = [0u8; 32];
            e[12..].copy_from_slice(&TOKEN);
            e
        }),
    ];
    let token_slots = [SlotQuery { slot, proof: slot_proof.as_slice(), conditions: &value_ok }];
    let small_slots = [SlotQuery { slot: w(0), proof: small_slot_proof.as_slice(), conditions: &small_value }];
    let accounts = [
        AccountQuery { address: TOKEN, proof: token_proof.as_slice(), conditions: &[], slots: &token_slots },
        AccountQuery { address: SMALL, proof: small_proof.as_slice(), conditions: &small_ok, slots: &small_slots },
    ];
    let logs = [LogQuery { tx_index: 3, log_index: 2, proof: log_proof.as_slice(), conditions: &transfer }];
    let facts = attest_batch(&record, 1, &Batch { accounts: &accounts, logs: &logs, relations: &[] }).unwrap();
    assert_eq!(facts.len(), 5);
    assert!(matches!(facts[0].fact, Fact::Account(a) if a.address == TOKEN));
    assert!(matches!(facts[1].fact, Fact::Storage(s) if s.value == value));
    assert!(matches!(facts[3].fact, Fact::Storage(s) if s.value == w(0x2a)));
    assert!(matches!(facts[4].fact, Fact::Log(l) if l.tx_index == 3 && l.log_index == 2));
    assert_eq!(facts[1].id, storage_id(1, &TOKEN, &slot, &value));
    // Any failing condition, anywhere, fails the whole statement.
    let too_much = [cond(Field::Balance, Cmp::Gt, w(5))];
    let accounts2 = [AccountQuery { conditions: &too_much, ..accounts[1] }];
    assert_eq!(
        attest_batch(&record, 1, &Batch { accounts: &accounts2, logs: &logs, relations: &[] }),
        Err(Error::PredicateFailed)
    );
    let wrong_field = [cond(Field::Nonce, Cmp::Eq, w(0))];
    let logs2 = [LogQuery { conditions: &wrong_field, ..logs[0] }];
    assert_eq!(
        attest_batch(&record, 1, &Batch { accounts: &accounts, logs: &logs2, relations: &[] }),
        Err(Error::QueryInvalid)
    );
    let none: [AccountQuery<'_, Vec<u8>>; 0] = [];
    let no_logs: [LogQuery<'_, Vec<u8>>; 0] = [];
    assert_eq!(
        attest_batch(&record, 1, &Batch { accounts: &none, logs: &no_logs, relations: &[] }),
        Err(Error::QueryInvalid)
    );
    assert_eq!(
        attest_batch(&record, 2, &Batch { accounts: &accounts, logs: &logs, relations: &[] }),
        Err(Error::ChainMismatch)
    );
}

#[test]
fn block_bound_identifiers() {
    use ferrata_core::attest::{account_id_at_block, storage_id_at_block};
    // Values computed by an independent implementation of the documented derivation.
    let account = Account { nonce: 5, balance: w(1000), storage_root: [0x22; 32], code_hash: [0x33; 32] };
    assert_eq!(
        hex::encode(account_id_at_block(1, 100, &[0x11; 20], &account)),
        "bb4010e4fa410154ab4ba8e91379f07269800bfc470ba98aa41a9f1d7bf99f4a"
    );
    assert_eq!(
        hex::encode(storage_id_at_block(1, 100, &[0x11; 20], &w(3), &w(42))),
        "f66c650b0ce7f3a7ef2d299d5f7ee913b53178658f1ee7049a02294d0b633293"
    );
    // The default identifier is the same at two blocks; the block-bound one is not, and neither
    // collides with the other kind.
    let world = World::new(6);
    let (slot, _) = world.slots[&TOKEN][4];
    let (ap, sp) = world.proofs(&TOKEN, &slot);
    let at = |n: u64| attest_storage(&world.record(1, n), 1, &TOKEN, &ap, &slot, &sp).unwrap();
    let (a, b) = (at(500), at(501));
    assert_eq!(a.id, b.id);
    assert_ne!(a.id_at_block(), b.id_at_block());
    assert_ne!(a.id, a.id_at_block());
    assert_eq!(a.id_at_block(), storage_id_at_block(1, 500, &TOKEN, &slot, &a.fact.value));
    let acc = attest_account(&world.record(1, 500), 1, &PLAIN, &world.state.proof(&keccak(&PLAIN))).unwrap();
    assert_eq!(acc.id_at_block(), account_id_at_block(1, 500, &PLAIN, &acc.fact.account));
    assert_ne!(
        acc.id_at_block(),
        attest_account(&world.record(1, 501), 1, &PLAIN, &world.state.proof(&keccak(&PLAIN))).unwrap().id_at_block()
    );
    // In a batch, a log's block-bound identifier is its identifier.
    let log_proof = world.receipt_proof(3);
    let logs = [LogQuery { tx_index: 3, log_index: 0, proof: log_proof.as_slice(), conditions: &[] }];
    let none: [AccountQuery<'_, Vec<u8>>; 0] = [];
    let facts =
        attest_batch(&world.record(1, 500), 1, &Batch { accounts: &none, logs: &logs, relations: &[] }).unwrap();
    assert_eq!(facts[0].id_at_block(), facts[0].id);
}

#[test]
fn relations_compare_two_attested_values() {
    use ferrata_core::attest::{Operand, Relation};
    let world = World::new(7);
    let record = world.record(1, 600);
    let (s1, v1) = world.slots[&TOKEN][1];
    let (s2, v2) = world.slots[&TOKEN][2];
    let (p1, p2) = (world.storage[&TOKEN].proof(&keccak(&s1)), world.storage[&TOKEN].proof(&keccak(&s2)));
    let token_proof = world.state.proof(&keccak(&TOKEN));
    let plain_proof = world.state.proof(&keccak(&PLAIN));
    let slots = [
        SlotQuery { slot: s1, proof: p1.as_slice(), conditions: &[] },
        SlotQuery { slot: s2, proof: p2.as_slice(), conditions: &[] },
    ];
    let accounts = [
        AccountQuery { address: TOKEN, proof: token_proof.as_slice(), conditions: &[], slots: &slots },
        AccountQuery { address: PLAIN, proof: plain_proof.as_slice(), conditions: &[], slots: &[] },
    ];
    // Facts: 0 token account, 1 slot s1, 2 slot s2, 3 the plain account.
    let rel = |l: (usize, Field), cmp, r: (usize, Field)| Relation {
        left: Operand { fact: l.0, field: l.1 },
        cmp,
        right: Operand { fact: r.0, field: r.1 },
    };
    let (lo, hi, ord) = if v1 < v2 { (1, 2, Cmp::Lt) } else { (2, 1, Cmp::Gt) };
    let run = |relations: &[Relation]| {
        attest_batch(&record, 1, &Batch { accounts: &accounts, logs: &[] as &[LogQuery<'_, Vec<u8>>], relations })
            .map(|f| f.len())
    };
    assert_eq!(run(&[rel((1, Field::Value), ord, (2, Field::Value))]), Ok(4));
    assert_eq!(
        run(&[
            rel((lo, Field::Value), Cmp::Le, (hi, Field::Value)),
            rel((lo, Field::Value), Cmp::Ne, (hi, Field::Value))
        ]),
        Ok(4)
    );
    assert_eq!(run(&[rel((hi, Field::Value), Cmp::Lt, (lo, Field::Value))]), Err(Error::PredicateFailed));
    assert_eq!(run(&[rel((1, Field::Value), Cmp::Eq, (1, Field::Value))]), Ok(4), "a value equals itself");
    // An account field against a storage value, and against another account's field.
    assert_eq!(run(&[rel((3, Field::Balance), Cmp::Gt, (0, Field::Balance))]), Ok(4), "10^20 against 0");
    assert_eq!(run(&[rel((3, Field::Nonce), Cmp::Ge, (0, Field::Nonce))]), Ok(4));
    // A fact that is not there, or a field its fact does not have.
    assert_eq!(run(&[rel((4, Field::Value), Cmp::Eq, (1, Field::Value))]), Err(Error::QueryInvalid));
    assert_eq!(run(&[rel((1, Field::Value), Cmp::Eq, (0, Field::Value))]), Err(Error::QueryInvalid));
}
