//! The inbound leg and the burns on synthetic vault storage, through LEZ v0.3.1's state machine
//! (dev-mode proofs for the private transactions): every refusal of the brief, the private claim's
//! public part searched for its private inputs, the burn tree checked against the bridge core's.
//! Writes `results/synthetic.md`.

use ferrata_bridge_core::{inbound::DestKind, sha256, tree::Tree};
use ferrata_lez03_bridge_core::{
    burn_account, code, lc_code, root_account, BridgeState, Denomination, Instruction, RootState, ETH,
};
use harness03::{
    account_of,
    deposits::{self, Deposit},
    eth::{vault_slots, Block},
    key, render, occurrences, PrivateHolder, World,
};
use lee::ProgramShardSelector;

const CHAIN: u64 = 11_155_111;
const VAULT: [u8; 20] = [0xAA; 20];
const TOKEN: [u8; 20] = [0x70; 20];
const ETH_DENOMINATION: u128 = 1_000_000_000_000_000; // 0.001 ETH
const TOKEN_DENOMINATION: u128 = 100_000_000; // 100 units of a 6-decimal token

fn registry() -> Vec<Denomination> {
    vec![
        Denomination { token: ETH, amount: ETH_DENOMINATION },
        Denomination { token: TOKEN, amount: TOKEN_DENOMINATION },
    ]
}

fn code_of(r: &harness03::TxReport) -> Option<u32> {
    r.code
}

#[test]
fn inbound_and_burns_on_synthetic_storage() {
    let mut w = World::new();
    let instance = sha256(b"ferrata/lez-v03-bridge/harness/instance");
    let config = w.config(CHAIN, VAULT, instance, registry());
    let cid = w.config_id(&config);
    // Another configuration of the same vault (another registry): the vault does not hold its id.
    let other = w.config(CHAIN, VAULT, instance, vec![Denomination { token: ETH, amount: 2 * ETH_DENOMINATION }]);

    let wallet = deposits::keys("ferrata/lez-v03-bridge/harness: depositor bridge seed (test value)");
    let alice = key(1); // a public LEZ account: public-kind deposits to it
    let bob = key(2);
    let mut private = PrivateHolder::new(1); // a private LEZ account: private-kind deposits to it
    let mut intruder = PrivateHolder::new(2);
    let alice_id = *account_of(&alice).value();
    let depositor = [0xD0; 20];
    let dep = |j: u64, token: [u8; 20], amount: u128, kind: DestKind, dest: [u8; 32]| {
        Deposit::new(&wallet, CHAIN, &VAULT, depositor, j, j, token, amount, kind, dest)
    };
    let deposits = vec![
        dep(0, ETH, ETH_DENOMINATION, DestKind::Public, alice_id),
        dep(1, ETH, ETH_DENOMINATION, DestKind::Private, *private.id().value()),
        dep(2, TOKEN, TOKEN_DENOMINATION, DestKind::Public, alice_id),
        dep(3, ETH, 5 * ETH_DENOMINATION, DestKind::Public, alice_id), // not a registered amount
        dep(4, ETH, ETH_DENOMINATION, DestKind::Public, alice_id),
    ];
    let leaves: Vec<_> = deposits.iter().map(Deposit::leaf).collect();

    // Block 1000 (beacon slot 100, Gloas layout): four deposits, the configuration's id in slot 2.
    let b4 = Block::gloas(100, 1000, VAULT, &vault_slots(deposits::root(&leaves, 4), 4, cid));
    let r4 = w.place_record(instance, CHAIN, &b4.record);
    // Block 1010 (slot 110, payload layout): five deposits.
    let b5 = Block::new(110, 1010, VAULT, &vault_slots(deposits::root(&leaves, 5), 5, cid));
    let r5 = w.place_record(instance, CHAIN, &b5.record);

    // Pairing.
    let t = w.init("Init without the block's header (Gloas record)", &config, &r4, b4.pairing_proof_without_header());
    assert_eq!(code_of(&t), Some(u32::from(lc_code::HEADER_REQUIRED)));
    let t = w.init("Init of another configuration of the vault (wrong pairing)", &other, &r4, b4.pairing_proof());
    assert_eq!(code_of(&t), Some(u32::from(code::PAIRING_MISMATCH)));
    let t = w.import_root("ImportRoot before Init", &config, &r4, deposits::root(&leaves, 4), 4, b4.import_proof());
    assert_eq!(code_of(&t), Some(u32::from(code::NOT_FOUND)));
    let t = w.init("Init (the vault's slot 2 holds configId)", &config, &r4, b4.pairing_proof());
    assert!(t.applied(), "{}", t.verdict());
    let t = w.init("Init again", &config, &r4, b4.pairing_proof());
    assert_eq!(code_of(&t), Some(u32::from(code::ALREADY_WRITTEN)));

    // Imports.
    let t = w.import_root("ImportRoot of a root the vault does not hold", &config, &r4, [9; 32], 4, b4.import_proof());
    assert_eq!(code_of(&t), Some(u32::from(code::IMPORT_MISMATCH)));
    let mut forged = r4.clone();
    forged.slot = 101; // no record at header(instance, 101); the record's own slot is 100
    let t = w.import_root("ImportRoot naming a record slot it is not keyed by", &config, &forged, deposits::root(&leaves, 4), 4, b4.import_proof());
    assert_eq!(code_of(&t), Some(u32::from(code::IMPORT_MISMATCH)));
    let t = w.import_root("ImportRoot n = 4", &config, &r4, deposits::root(&leaves, 4), 4, b4.import_proof());
    assert!(t.applied(), "{}", t.verdict());
    let t = w.import_root("ImportRoot n = 4 again", &config, &r4, deposits::root(&leaves, 4), 4, b4.import_proof());
    assert_eq!(code_of(&t), Some(u32::from(code::ALREADY_WRITTEN)));
    let root4 = w.root_state(&config, 4).expect("root(cid, 4)");
    assert_eq!(root4.root, deposits::root(&leaves, 4));
    assert_eq!(root4.block_number, 1000);

    // Public claims.
    let d0 = deposits[0].witness(&leaves, 4);
    let t = w.claim("Claim deposit 0 (public kind, 0.001 ETH to alice)", &config, &root4, &d0);
    assert!(t.applied(), "{}", t.verdict());
    assert_eq!(w.public_holding(&config, &alice_id, &ETH), ETH_DENOMINATION);
    let t = w.claim("the same claim again (replay)", &config, &root4, &d0);
    assert_eq!(code_of(&t), Some(u32::from(code::ALREADY_CLAIMED)));
    let mut to_bob = d0.clone();
    to_bob.dest = *account_of(&bob).value();
    let t = w.claim("deposit 0 claimed to another destination", &config, &root4, &to_bob);
    assert_eq!(code_of(&t), Some(u32::from(code::CLAIM_INVALID)));
    // The vault's root at five leaves, which nobody imported yet: the witness opens it, the guard
    // at root(cid, 5) refuses.
    let unknown = RootState { root: deposits::root(&leaves, 5), leaf_count: 5, ..root4 };
    let t = w.claim("deposit 2 under a root never imported (n = 5)", &config, &unknown, &deposits[2].witness(&leaves, 5));
    assert_eq!(code_of(&t), Some(u32::from(code::NOT_FOUND)));
    let made_up = RootState { root: [7; 32], ..root4 };
    let t = w.claim("deposit 2 under a made-up root", &config, &made_up, &deposits[2].witness(&leaves, 4));
    assert_eq!(code_of(&t), Some(u32::from(code::CLAIM_INVALID)));
    let t = w.claim("deposit 4 (leaf index 4) under root(cid, 4)", &config, &root4, &deposits[4].witness(&leaves, 5));
    assert_eq!(code_of(&t), Some(u32::from(code::LEAF_NOT_IMPORTED)));
    let t = w.claim("deposit 1 (private kind) by the plaintext path", &config, &root4, &deposits[1].witness(&leaves, 4));
    assert_eq!(code_of(&t), Some(u32::from(code::WRONG_DESTINATION)));
    let t = w.claim("deposit 3 (0.005 ETH, not registered)", &config, &root4, &deposits[3].witness(&leaves, 4));
    assert_eq!(code_of(&t), Some(u32::from(code::NOT_REGISTERED)));
    let t = w.claim("Claim deposit 2 (100 tokens to alice)", &config, &root4, &deposits[2].witness(&leaves, 4));
    assert!(t.applied(), "{}", t.verdict());
    assert_eq!(w.public_holding(&config, &alice_id, &TOKEN), TOKEN_DENOMINATION);

    // The private claim.
    let d1 = deposits[1].witness(&leaves, 4);
    let (t, _) = w.claim_privately("ClaimPrivately of deposit 1 by another private account", &config, &root4, &d1, &mut intruder);
    assert_eq!(code_of(&t), Some(u32::from(code::WRONG_DESTINATION)));
    assert_eq!(t.stage, "private part: no proof");
    let (t, tx) = w.claim_privately("ClaimPrivately of deposit 1 (private kind, to the private account)", &config, &root4, &d1, &mut private);
    assert!(t.applied(), "{}", t.verdict());
    match private.bridge_state(w.bridge) {
        Some(BridgeState::Holder(h)) => assert_eq!(h.holding(&cid, &ETH), ETH_DENOMINATION),
        other => panic!("the private account's bridge shard: {other:?}"),
    }
    // What the private claim's transaction shows.
    let tx = tx.unwrap();
    let bytes = borsh::to_vec(&tx).unwrap();
    let witness = &deposits[1];
    let mut needles: Vec<(&str, Vec<u8>)> = vec![
        ("the destination (the private account's id)", private.id().value().to_vec()),
        ("s", witness.s.to_vec()),
        ("C", witness.commitment().to_vec()),
        ("the deposit leaf", witness.leaf().to_vec()),
        ("the depositor", witness.depositor.to_vec()),
        ("the borsh witness", borsh::to_vec(&d1).unwrap()),
        ("N", witness.nullifier(CHAIN, &VAULT).to_vec()),
        ("the leaf index (8-byte LE, with the depositor)", [witness.depositor.as_slice(), &1u64.to_le_bytes()].concat()),
        ("configId", cid.to_vec()),
        ("the imported root", root4.root.to_vec()),
        ("root(cid, 4)'s account", root_account(w.bridge, &cid, 4).value().to_vec()),
    ];
    for (i, s) in d1.path.iter().enumerate().filter(|(_, s)| **s != [0u8; 32]) {
        needles.push((if i == 0 { "a path sibling" } else { "another path sibling" }, s.to_vec()));
    }
    let shown = occurrences(&bytes, &needles);
    for hidden in ["the destination (the private account's id)", "s", "C", "the deposit leaf", "the depositor", "the borsh witness", "N", "a path sibling", "another path sibling"] {
        assert!(!shown.contains(&hidden), "the private claim shows {hidden}");
    }
    assert!(shown.contains(&"configId") && shown.contains(&"root(cid, 4)'s account"));
    let privacy = format!(
        "{} bytes searched; present: {}; absent: {}",
        bytes.len(),
        shown.join(", "),
        needles.iter().map(|(n, _)| *n).filter(|n| !shown.contains(n)).collect::<Vec<_>>().join(", ")
    );
    let (t, _) = w.claim_privately("ClaimPrivately of deposit 1 again (replay)", &config, &root4, &d1, &mut private);
    assert_eq!(code_of(&t), Some(u32::from(code::ALREADY_CLAIMED)));
    assert_eq!(t.stage, "settlement");

    // A later root, and the leaf it adds.
    let t = w.import_root("ImportRoot n = 5 (payload record)", &config, &r5, deposits::root(&leaves, 5), 5, b5.import_proof());
    assert!(t.applied(), "{}", t.verdict());
    let root5 = w.root_state(&config, 5).unwrap();
    let t = w.claim("Claim deposit 4 under root(cid, 5)", &config, &root5, &deposits[4].witness(&leaves, 5));
    assert!(t.applied(), "{}", t.verdict());
    let t = w.claim("deposit 0 again under the later root", &config, &root5, &deposits[0].witness(&leaves, 5));
    assert_eq!(code_of(&t), Some(u32::from(code::ALREADY_CLAIMED)));
    assert_eq!(w.public_holding(&config, &alice_id, &ETH), 2 * ETH_DENOMINATION);

    // Burns.
    let recipient = [0xEE; 20];
    let note = |i: u64, token, amount| deposits::note(&wallet, &cid, i, CHAIN, VAULT, recipient, token, amount);
    let n0 = note(0, ETH, ETH_DENOMINATION);
    let t = w.burn_public("Burn 0.001 ETH (alice, public)", &config, &n0, &alice);
    assert!(t.applied(), "{}", t.verdict());
    let t = w.burn_public("the same note again (alice holds 0.001 ETH more)", &config, &n0, &alice);
    assert_eq!(code_of(&t), Some(u32::from(code::ALREADY_WRITTEN)));
    let t = w.burn_public("Burn 0.002 ETH (not a denomination)", &config, &note(1, ETH, 2 * ETH_DENOMINATION), &alice);
    assert_eq!(code_of(&t), Some(u32::from(code::NOT_REGISTERED)));
    let t = w.burn_public("Burn 100 tokens by bob, who holds none", &config, &note(2, TOKEN, TOKEN_DENOMINATION), &bob);
    assert_eq!(code_of(&t), Some(u32::from(code::INSUFFICIENT_BALANCE)));
    // Alice's holder account named, but the transaction not signed by alice.
    let n3 = note(3, ETH, ETH_DENOMINATION);
    let alice_acc = account_of(&alice);
    let selectors = vec![
        ProgramShardSelector::new(alice_acc, w.bridge),
        ProgramShardSelector::new(ferrata_lez03_bridge_core::holder_account(w.bridge, alice_acc.value()), w.bridge),
        ProgramShardSelector::new(burn_account(w.bridge, &cid, &n3.hash()), w.bridge),
        ProgramShardSelector::new(ferrata_lez03_bridge_core::burns_account(w.bridge, &cid), w.bridge),
    ];
    let i = Instruction::Burn { config: config.clone(), note: n3, from: ferrata_lez03_bridge_core::BurnFrom::Holder };
    let t = w.public("alice's burn sent without alice's signature", &i, selectors, &[]);
    assert_eq!(code_of(&t), Some(u32::from(code::NOT_AUTHORIZED)));
    let (t, burn_tx) = w.burn_private("Burn 0.001 ETH (the private account, private)", &config, &note(4, ETH, ETH_DENOMINATION), &mut private);
    assert!(t.applied(), "{}", t.verdict());
    let (t, _) = w.burn_private("a second private burn (the private account holds nothing)", &config, &note(5, ETH, ETH_DENOMINATION), &mut private);
    assert_eq!(code_of(&t), Some(u32::from(code::INSUFFICIENT_BALANCE)));
    assert_eq!(t.stage, "private part: no proof");
    let burn_bytes = borsh::to_vec(&burn_tx.unwrap()).unwrap();
    assert!(!burn_bytes.windows(32).any(|x| x == private.id().value()), "the private burn shows its holder");
    assert!(!burn_bytes.windows(20).any(|x| x == recipient), "a burn shows the Ethereum recipient");

    // The burn tree: two leaves, in order, as the bridge core's tree has them.
    let tree = w.burn_tree(&config).expect("burns(cid)");
    assert_eq!(tree.count, 2);
    let expected = Tree::from_leaves(vec![n0.leaf(&cid, 0), note(4, ETH, ETH_DENOMINATION).leaf(&cid, 1)]).unwrap();
    assert_eq!(tree.root().unwrap(), expected.root());
    assert_eq!(w.public_holding(&config, &alice_id, &ETH), ETH_DENOMINATION);

    let applied = w.reports.iter().filter(|r| r.applied()).count();
    let out = format!(
        "# Synthetic vault storage: the inbound leg and the burns on LEZ v0.3.1's state machine\n\n\
         Written by `harness/tests/synthetic.rs` (`cargo test -p harness03 --test synthetic`, RISC Zero dev mode \
         for the two private transactions' proofs). Bridge image id `{}`; configuration `{}`. {} transactions, \
         {} applied. Execution gas is LEZ's metered cycles (`ValidatedStateDiff::from_public_transaction_metered`), \
         storage gas the transaction's bytes; private transactions are fee-exempt on v0.3.1 \
         (`lez/chain_state/src/classify.rs`).\n\n{}\n\n**The private claim's transaction** (dev-mode proof): {}.\n\n\
         **Burn tree** at `burns(cid)`: {} leaves, root `{}` = the bridge core's `Tree` of the two notes' `burn_leaf`.\n",
        hex::encode(harness03::bridge_image_id()),
        hex::encode(cid),
        w.reports.len(),
        applied,
        render(&w.reports),
        privacy,
        tree.count,
        hex::encode(tree.root().unwrap()),
    );
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../results");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("synthetic.md"), out).unwrap();
}
