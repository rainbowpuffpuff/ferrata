//! The independent review's tests of 18 November 2026 (review-history-hops), adopted as written:
//! adversarial chains of EIP-2935 hops on the recorded Sepolia fixture (natively and through the
//! deployed program: the window's edges, a hop reused, a loop back up, the order reversed, a padded
//! header, the wrong slot, another block's account proof), and the Leaf class's pin: one leaf under
//! two slots of one contract has two Leaf identifiers, so a consumer pins the contract and the slot
//! (`docs/design/attestation.md` §5.7). Writes `results/history-hops-review-*.md`.

use ferrata_lez_wire::attest::{evaluate, AccountQuery, Attestation, Fact, Hop, LeafQuery, Request, SlotQuery};
use ferrata_spectests::execkit::{account_rlp, enc, header_rlp, keccak, Trie};
use harness::{
    hops::{HopFixture, WINDOW},
    sepolia::SEPOLIA_CHAIN_ID,
    vault::{self, RealVault},
};
use serde_json::Value;

fn code(r: &Result<Attestation, u8>) -> String {
    match r {
        Ok(a) => format!("Ok(block {}, hops {})", a.block_number, a.hops),
        Err(c) => format!("Err({c})"),
    }
}

#[test]
fn hop_chain_edges_reuse_loops_order_and_padding() {
    let mut v = RealVault::load();
    let fx = HopFixture::sepolia();
    let r = fx.record;
    let (id, record, base, actor, time) = (v.w.inst.id, v.record.clone(), v.base(), v.actor(), v.time);
    let a = fx.chain(0); // [12111574, 12111477]
    let far = fx.chain(1); // [R - 8191, R - 2*8191, ...]
    assert_eq!(far[0], r - WINDOW);

    // n mod 8191 on real data: in R's state, slot R mod 8191 holds the hash of R - 8191 (the
    // window's first block, inclusive), never R's own; slot a1 mod 8191 holds a1's hash.
    let h = fx.history(r);
    let at = |n: u64| h.storage.iter().find(|s| s.key == harness::attest::u256(u128::from(n % WINDOW))).map(|s| s.value);
    assert_eq!(r % WINDOW, (r - WINDOW) % WINDOW);
    assert_eq!(at(r - WINDOW), Some(fx.block_hash(r - WINDOW)), "slot R mod 8191 = hash(R - 8191)");
    assert_ne!(at(r), Some(keccak(&fx.header(r))), "the record's own block is not in its state");
    assert_eq!(at(a[0]), Some(fx.block_hash(a[0])));

    let mut padded = fx.header(a[0]);
    padded.push(0x80);
    let cases: Vec<(&str, Vec<Hop>, Option<u8>, u64)> = vec![
        ("control: R -> a1 -> a2", fx.hops(&a), None, a[1]),
        ("control: R -> R-8191 (first block of the window)", vec![fx.hop(r, far[0])], None, far[0]),
        ("reused hop: R -> a1 twice", vec![fx.hop(r, a[0]), fx.hop(r, a[0])], Some(150), r),
        ("reused hop later: R -> a1 -> a2 -> a2", vec![fx.hop(r, a[0]), fx.hop(a[0], a[1]), fx.hop(a[0], a[1])], Some(150), r),
        (
            "loop back up: R -> a1 -> a2 -> a1 (a1's header, proofs of a1's state)",
            vec![fx.hop(r, a[0]), fx.hop(a[0], a[1]), Hop { header: fx.header(a[0]), ..fx.hop(a[0], a[1]) }],
            Some(150),
            r,
        ),
        ("reversed order: a1 -> a2 first, then R -> a1", vec![fx.hop(a[0], a[1]), fx.hop(r, a[0])], None, r),
        ("padded header: a1's header with one byte appended", vec![Hop { header: padded, ..fx.hop(r, a[0]) }], None, r),
        (
            "right state, wrong slot: a1's header with the proof of slot (R-8191) mod 8191",
            vec![Hop { header: fx.header(a[0]), ..fx.hop(r, far[0]) }],
            None,
            r,
        ),
        (
            "right slot proof, another block's account proof (R-8191's state)",
            vec![Hop { account_proof: fx.history(far[0]).proof.clone(), ..fx.hop(r, a[0]) }],
            None,
            r,
        ),
        ("hop past the window from a hop record: R -> R-8191 -> (R-2*8191 is the edge) ok", fx.hops(&far[..2]), None, far[1]),
    ];
    let mut out = String::from("| case | native | program |\n|---|---|---|\n");
    for (label, hops, want, end) in cases {
        let request = Request { hops, accounts: vec![fx.history_account(end)], ..base.clone() };
        let native = evaluate(&id, SEPOLIA_CHAIN_ID, &record, &request);
        let report = vault::attest_root(&mut v.w.chain, label, actor, &request, time);
        let program = match report.exit_code {
            None if report.ok() => "applied".to_string(),
            c => format!("exit {c:?}"),
        };
        match (&native, label.starts_with("control") || label.ends_with(" ok")) {
            (Ok(_), true) => assert!(report.ok(), "{label}"),
            (Err(c), false) => {
                assert_eq!(report.exit_code, Some(u32::from(*c)), "{label}: program = native");
                if let Some(w) = want {
                    assert_eq!(*c, w, "{label}");
                }
            }
            _ => panic!("{label}: unexpected {}", code(&native)),
        }
        out.push_str(&format!("| {label} | {} | {program} |\n", code(&native)));
    }
    println!("{out}");
    std::fs::write(harness::results_dir().join("history-hops-review-hops.md"), out).unwrap();
}

/// The Leaf identifier is over (chain, contract, slot, index, leaf): one leaf under a second slot of
/// the same contract has a second identifier, both class Leaf. "Pin the contract" is not enough;
/// the consumer must pin the slot too (finding 2 of the review).
#[test]
fn one_leaf_under_two_slots_of_one_contract_has_two_leaf_identifiers() {
    let fx = HopFixture::sepolia();
    let syn = vault::synthetic();
    let leaf = syn.leaves[0];
    let (root0, path0) = vault::tree(&[leaf], 0);
    let (root7, path7) = vault::tree(&[syn.leaves[1], leaf], 1);
    let slot = |n: u8| vault::slot_word(n);
    let storage = Trie::new([(keccak(&slot(0)).to_vec(), enc::uint(&root0)), (keccak(&slot(7)).to_vec(), enc::uint(&root7))]);
    let contract = [0x77u8; 20];
    let code_hash = keccak(b"a contract with a second root slot");
    let state = Trie::new([(keccak(&contract).to_vec(), account_rlp(1, 0, storage.root(), code_hash))]);
    let served: Value = serde_json::from_slice(&fx.file(&format!("block_{}.json", fx.record))).unwrap();
    let mut block = served["result"].clone();
    block["stateRoot"] = Value::String(format!("0x{}", hex::encode(state.root())));
    let header = header_rlp(&block);
    let instance = [0x49u8; 32];
    let record = ferrata_core::witness::HeaderRecord {
        beacon: ferrata_core::types::BeaconBlockHeader { slot: 11_556_700, ..Default::default() },
        execution_root: [0; 32],
        execution: ferrata_core::witness::ExecutionSummary::BlockHash(keccak(&header)),
    };
    let request = Request {
        instance_id: instance,
        chain_id: SEPOLIA_CHAIN_ID,
        header: Some(header),
        accounts: vec![AccountQuery {
            address: contract,
            proof: state.proof(&keccak(&contract)),
            conditions: vec![],
            slots: [0u8, 7]
                .iter()
                .map(|&n| SlotQuery { slot: slot(n), proof: storage.proof(&keccak(&slot(n))), conditions: vec![] })
                .collect(),
        }],
        leaves: vec![
            LeafQuery { root: 1, leaf_index: 0, leaf, path: path0 },
            LeafQuery { root: 2, leaf_index: 1, leaf, path: path7 },
        ],
        ..Request::default()
    };
    let a = evaluate(&instance, SEPOLIA_CHAIN_ID, &record, &request).expect("both leaves attested");
    let leaves: Vec<_> = a.statements.iter().filter(|s| matches!(s.fact, Fact::AccumulatorLeaf { .. })).collect();
    assert_eq!(leaves.len(), 2);
    assert_ne!(leaves[0].id, leaves[1].id, "one leaf, one contract, two identifiers");
    #[cfg(feature = "program-class")]
    {
        use ferrata_lez_wire_class::attest::{evaluate as classed, StatementClass};
        let c = classed(&instance, SEPOLIA_CHAIN_ID, &record, &request).unwrap();
        let classes: Vec<_> = c.statements.iter().map(|s| s.class).collect();
        assert_eq!(classes, [StatementClass::Value, StatementClass::Value, StatementClass::Value, StatementClass::Leaf, StatementClass::Leaf]);
        assert!(c.statements[3].class.is_nullifiable() && c.statements[4].class.is_nullifiable());
    }
    let out = format!(
        "one leaf 0x{}… of contract 0x77…77: slot 0 index 0 -> id 0x{}…, slot 7 index 1 -> id 0x{}…; both Leaf, both is_nullifiable()\n",
        &hex::encode(leaf)[..8],
        &hex::encode(leaves[0].id)[..8],
        &hex::encode(leaves[1].id)[..8]
    );
    println!("{out}");
    std::fs::write(harness::results_dir().join("history-hops-review-leaf-slots.md"), out).unwrap();
}
