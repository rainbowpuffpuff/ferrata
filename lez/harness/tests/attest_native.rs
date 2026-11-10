//! The attestation interface on the host, without LEZ: the function the program's `Attest`
//! handler and a private consumer both run (`ferrata_lez_wire::attest::evaluate`), and the
//! reference consumer's acceptance rule (`ferrata_lez_wire::consumer::Pool::accept`).
//!
//! * **Accumulator leaves** have no real data to be proven on: no deployed contract keeps a
//!   SHA-256 accumulator root of this shape in a slot (the bridge's vault would). The leaves and
//!   paths are the shared vectors (`vectors/acc-leaf.json`, written by an independent Python
//!   implementation); the state they are proven under is synthetic, built here with the reference
//!   trie: one contract whose slot 0 holds the vectors' root. This runs the request path natively
//!   only; it does not run through the program on LEZ.
//! * **The pool's rule** is checked on attestations built by hand, since the program never
//!   produces the ones a pool must refuse.

use std::{fs, path::PathBuf};

use ferrata_core::{
    types::BeaconBlockHeader,
    witness::{ExecutionSummary, HeaderRecord},
    Root,
};
use ferrata_lez_wire::{
    attest::{evaluate, AccountQuery, Attestation, Fact, LeafQuery, Request, SlotQuery, Statement},
    code,
    consumer::{address_word, code as consumer_code, Deposit, Pool, TRANSFER_TOPIC},
};
use ferrata_spectests::execkit::{account_rlp, enc, header_rlp, keccak, Trie};
use lee::AccountId;
use serde_json::Value;

const INSTANCE: Root = [0x1d; 32];
const ADDRESS: [u8; 20] = [0x11; 20];

fn vectors() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vectors/acc-leaf.json");
    serde_json::from_slice(&fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))).unwrap()
}

fn h32(v: &Value) -> Root {
    hex::decode(v.as_str().unwrap().trim_start_matches("0x")).unwrap().try_into().unwrap()
}

fn path(v: &Value) -> Vec<Root> {
    v.as_array().unwrap().iter().map(h32).collect()
}

/// A synthetic block whose state holds one contract at [`ADDRESS`] with `root` in slot `slot`,
/// among other accounts; and the request that proves that slot.
fn block_holding(root: &Root, slot: &Root) -> (HeaderRecord, Request) {
    let storage = Trie::new([(keccak(slot).to_vec(), enc::uint(root))]);
    let mut accounts = vec![(ADDRESS, account_rlp(1, 0, storage.root(), keccak(b"code")))];
    // Forty other accounts, none at the contract's address.
    for i in 0x40..0x68u8 {
        accounts.push(([i; 20], account_rlp(u64::from(i), 1_000 + u128::from(i), keccak(&[0x80]), keccak(b""))));
    }
    let state = Trie::new(accounts.iter().map(|(a, rlp)| (keccak(a).to_vec(), rlp.clone())));
    let recorded = HeaderRecord {
        beacon: BeaconBlockHeader { slot: 9_000_000, ..Default::default() },
        execution_root: [3; 32],
        execution: ExecutionSummary::Payload {
            block_number: 100,
            block_hash: [9; 32],
            parent_hash: [8; 32],
            state_root: state.root(),
            receipts_root: [7; 32],
            timestamp: 1_700_000_000,
        },
    };
    let request = Request {
        instance_id: INSTANCE,
        chain_id: 1,
        accounts: vec![AccountQuery {
            address: ADDRESS,
            proof: state.proof(&keccak(&ADDRESS)),
            conditions: vec![],
            slots: vec![SlotQuery { slot: *slot, proof: storage.proof(&keccak(slot)), conditions: vec![] }],
        }],
        ..Request::default()
    };
    (recorded, request)
}

#[test]
fn accumulator_leaves_of_the_shared_vectors_through_a_request() {
    let v = vectors();
    let slot = h32(&v["slot"]);
    assert_eq!(hex::decode(v["address"].as_str().unwrap().trim_start_matches("0x")).unwrap(), ADDRESS);
    assert_eq!(v["chainId"].as_u64(), Some(1));
    let mut proven = 0;
    for snapshot in v["snapshots"].as_array().unwrap() {
        let root = h32(&snapshot["root"]);
        let (recorded, base) = block_holding(&root, &slot);
        // Every leaf of the snapshot in one request, each against the storage fact at position 1.
        let leaves: Vec<LeafQuery> = snapshot["leaves"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| LeafQuery {
                root: 1,
                leaf_index: e["leafIndex"].as_u64().unwrap(),
                leaf: h32(&e["leaf"]),
                path: path(&e["path"]),
            })
            .collect();
        let request = Request { leaves: leaves.clone(), ..base.clone() };
        let attestation = evaluate(&INSTANCE, 1, &recorded, &request).expect("the leaves verify");
        assert_eq!(attestation.statements.len(), 2 + leaves.len(), "the account, its slot, then the leaves");
        assert_eq!(attestation.statements[1].fact, Fact::Storage { address: ADDRESS, slot, value: root });
        for (i, e) in snapshot["leaves"].as_array().unwrap().iter().enumerate() {
            let expected = Statement {
                id: h32(&e["id"]),
                fact: Fact::AccumulatorLeaf {
                    address: ADDRESS,
                    slot,
                    leaf_index: leaves[i].leaf_index,
                    leaf: leaves[i].leaf,
                    root,
                },
            };
            assert_eq!(attestation.statements[2 + i], expected, "the vectors' identifier");
            proven += 1;
        }
    }
    assert_eq!(proven, 14, "both snapshots");

    // The vectors' refusals, each with the core's code for it.
    let mut refused = 0;
    for case in v["invalid"].as_array().unwrap() {
        let (recorded, base) = block_holding(&h32(&case["root"]), &slot);
        let leaf = LeafQuery {
            root: 1,
            leaf_index: case["leafIndex"].as_u64().unwrap(),
            leaf: h32(&case["leaf"]),
            path: path(&case["path"]),
        };
        let code = match case["error"].as_str().unwrap() {
            "AccumulatorPathInvalid" => 160,
            "AccumulatorIndexOutOfRange" => 161,
            "AccumulatorLeafEmpty" => 162,
            "AccumulatorRootMismatch" => 163,
            other => panic!("unknown error {other}"),
        };
        let request = Request { leaves: vec![leaf], ..base };
        assert_eq!(evaluate(&INSTANCE, 1, &recorded, &request), Err(code), "{}", case["case"]);
        refused += 1;
    }
    assert_eq!(refused, 7);

    // A leaf must name a storage fact of the same request.
    let snapshot = &v["snapshots"][0];
    let e = &snapshot["leaves"][0];
    let (recorded, base) = block_holding(&h32(&snapshot["root"]), &slot);
    let leaf = |root: u32| LeafQuery {
        root,
        leaf_index: e["leafIndex"].as_u64().unwrap(),
        leaf: h32(&e["leaf"]),
        path: path(&e["path"]),
    };
    for (what, position) in [("the account fact", 0), ("a position past the last fact", 2)] {
        let request = Request { leaves: vec![leaf(position)], ..base.clone() };
        assert_eq!(evaluate(&INSTANCE, 1, &recorded, &request), Err(code::BAD_QUERY), "{what}");
    }
    // And the request is bound like every other: instance, then chain.
    let request = Request { leaves: vec![leaf(1)], ..base.clone() };
    assert!(evaluate(&INSTANCE, 1, &recorded, &request).is_ok());
    assert_eq!(evaluate(&[0x2e; 32], 1, &recorded, &request), Err(code::WRONG_INSTANCE));
    assert_eq!(evaluate(&INSTANCE, 5, &recorded, &request), Err(140), "ChainMismatch");
    // A record that binds a block hash only (the Gloas layout) is attested with that block's RLP
    // header, from which the roots are read: here the header of a real Sepolia block with the
    // synthetic state root put in, so the same request proves the same facts.
    let ExecutionSummary::Payload { state_root, .. } = recorded.execution else { unreachable!() };
    let served: Value = serde_json::from_slice(&harness::sepolia::gloas_file("block.json")).unwrap();
    let mut block = served["result"].clone();
    block["stateRoot"] = Value::String(format!("0x{}", hex::encode(state_root)));
    let rlp = header_rlp(&block);
    let gloas = HeaderRecord { execution: ExecutionSummary::BlockHash(keccak(&rlp)), ..recorded.clone() };
    assert_eq!(evaluate(&INSTANCE, 1, &gloas, &request), Err(code::HEADER_REQUIRED), "no header");
    let with_header = Request { header: Some(rlp.clone()), ..request.clone() };
    let by_header = evaluate(&INSTANCE, 1, &gloas, &with_header).unwrap();
    let by_payload = evaluate(&INSTANCE, 1, &recorded, &request).unwrap();
    assert_eq!(by_header.statements, by_payload.statements, "the same facts and identifiers");
    let number = u64::from_str_radix(block["number"].as_str().unwrap().trim_start_matches("0x"), 16).unwrap();
    assert_eq!((by_header.block_number, by_header.block_hash), (number, keccak(&rlp)), "from the header");
    assert_eq!(evaluate(&INSTANCE, 1, &recorded, &with_header), Err(code::BAD_QUERY), "a header for a payload record");
    let header = |h: Vec<u8>| Request { header: Some(h), ..request.clone() };
    let mut tampered = rlp.clone();
    tampered[100] ^= 1;
    assert_eq!(evaluate(&INSTANCE, 1, &gloas, &header(tampered)), Err(120), "tampered: HeaderHashMismatch");
    assert_eq!(evaluate(&INSTANCE, 1, &gloas, &header(rlp[..rlp.len() - 1].to_vec())), Err(120), "truncated");
    assert_eq!(evaluate(&INSTANCE, 1, &gloas, &header([rlp.as_slice(), &[0]].concat())), Err(120), "over-long");
    // Another block's header: the real one, whose state root is not the recorded block's.
    assert_eq!(evaluate(&INSTANCE, 1, &gloas, &header(header_rlp(&served["result"]))), Err(120), "another block");
    // No execution block to read: a zero block hash (a header from before the merge), the Altair
    // layout.
    let zero = HeaderRecord { execution: ExecutionSummary::BlockHash([0; 32]), ..recorded.clone() };
    assert_eq!(evaluate(&INSTANCE, 1, &zero, &with_header), Err(141), "BindingMismatch");
    let altair = HeaderRecord { execution: ExecutionSummary::None, ..recorded };
    assert_eq!(evaluate(&INSTANCE, 1, &altair, &request), Err(141), "BindingMismatch");
}

fn pool() -> Pool {
    Pool {
        ferrata: AccountId::new([0xfe; 32]),
        instance_id: INSTANCE,
        chain_id: 1,
        token: [0xa0; 20],
        recipient: [0xbb; 20],
    }
}

/// An attestation of one `Transfer` of the pool's token from `[0xcc; 20]` to the pool's recipient.
fn transfer(amount: u128) -> Attestation {
    let pool = pool();
    let mut data = vec![0u8; 16];
    data.extend_from_slice(&amount.to_be_bytes());
    Attestation {
        instance_id: pool.instance_id,
        chain_id: pool.chain_id,
        slot: 9_000_000,
        block_number: 100,
        block_hash: [9; 32],
        hops: 0,
        statements: vec![Statement {
            id: [0x51; 32],
            fact: Fact::Log {
                tx_index: 4,
                log_index: 2,
                address: pool.token,
                topics: vec![TRANSFER_TOPIC, address_word(&[0xcc; 20]), address_word(&pool.recipient)],
                data,
            },
        }],
    }
}

/// `attestation` with its one log changed by `change`.
fn with_log(
    mut attestation: Attestation,
    change: impl FnOnce(&mut [u8; 20], &mut Vec<Root>, &mut Vec<u8>),
) -> Attestation {
    let Fact::Log { address, topics, data, .. } = &mut attestation.statements[0].fact else { unreachable!() };
    change(address, topics, data);
    attestation
}

#[test]
fn the_pool_accepts_exactly_its_deposit() {
    let pool = pool();
    let good = transfer(1_000_000);
    assert_eq!(
        pool.accept(&good),
        Ok(Deposit { statement_id: [0x51; 32], block_number: 100, owner: [0xcc; 20], amount: 1_000_000 })
    );
    assert_eq!(pool.accept(&transfer(u128::MAX)).map(|d| d.amount), Ok(u128::MAX));

    // The binding: another instance, another chain.
    let binding = Err(consumer_code::BINDING_MISMATCH);
    assert_eq!(pool.accept(&Attestation { instance_id: [0x2e; 32], ..good.clone() }), binding);
    assert_eq!(pool.accept(&Attestation { chain_id: 11_155_111, ..good.clone() }), binding);

    // Exactly one statement, and it is the pool's kind of fact.
    let unexpected = Err(consumer_code::UNEXPECTED_FACT);
    assert_eq!(pool.accept(&Attestation { statements: vec![], ..good.clone() }), unexpected);
    let twice = [good.statements.clone(), good.statements.clone()].concat();
    assert_eq!(pool.accept(&Attestation { statements: twice, ..good.clone() }), unexpected);
    let storage =
        Statement { id: [0x51; 32], fact: Fact::Storage { address: pool.token, slot: [0; 32], value: [1; 32] } };
    assert_eq!(pool.accept(&Attestation { statements: vec![storage], ..good.clone() }), unexpected);
    assert_eq!(
        pool.accept(&with_log(good.clone(), |address, _, _| *address = [0xa1; 20])),
        unexpected,
        "another token"
    );
    assert_eq!(
        pool.accept(&with_log(good.clone(), |_, topics, _| topics[0] = [0xdd; 32])),
        unexpected,
        "another event"
    );
    assert_eq!(
        pool.accept(&with_log(good.clone(), |_, topics, _| topics[2] = address_word(&[0xbc; 20]))),
        unexpected,
        "another recipient"
    );
    assert_eq!(
        pool.accept(&with_log(good.clone(), |_, topics, _| topics[1][0] = 1)),
        unexpected,
        "a sender wider than an address"
    );
    assert_eq!(pool.accept(&with_log(good.clone(), |_, topics, _| topics.truncate(2))), unexpected, "two topics");
    assert_eq!(
        pool.accept(&with_log(good.clone(), |_, topics, _| topics.push([0; 32]))),
        unexpected,
        "four topics (an ERC-721 transfer)"
    );
    assert_eq!(pool.accept(&with_log(good.clone(), |_, _, data| data.push(0))), unexpected, "33 bytes of data");
    assert_eq!(pool.accept(&with_log(good.clone(), |_, _, data| data.clear())), unexpected, "no data");

    // An amount of 2^128 or more is refused, not truncated.
    assert_eq!(pool.accept(&with_log(good, |_, _, data| data[15] = 1)), Err(consumer_code::AMOUNT_TOO_LARGE));

    // The pool's identity: any field changes the id, and with it every actor's address.
    let id = pool.id();
    assert_ne!(Pool { ferrata: AccountId::new([0xfd; 32]), ..pool.clone() }.id(), id);
    assert_ne!(Pool { instance_id: [0x2e; 32], ..pool.clone() }.id(), id);
    assert_ne!(Pool { chain_id: 11_155_111, ..pool.clone() }.id(), id);
    assert_ne!(Pool { token: [0xa1; 20], ..pool.clone() }.id(), id);
    assert_ne!(Pool { recipient: [0xbc; 20], ..pool }.id(), id);
}
