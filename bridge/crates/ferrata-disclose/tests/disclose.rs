mod common;

use std::collections::HashSet;

use common::*;
use ferrata_bridge_core::inbound::DestKind;
use ferrata_bridge_core::{derive_keys, Hash};
use ferrata_disclose::{verify, ClaimSecret, Disclosure, DisclosureError};

/// A disclosure agrees with the wallet's own facts and verifies against the public view.
fn check_disclosure(facts: &[DepositFact], disclosure: &Disclosure, w: &World) {
    // The document itself: one entry per own deposit, `s` and `N` only for the claimed ones.
    assert_eq!(disclosure.deposits.len(), facts.len());
    for (entry, fact) in disclosure.deposits.iter().zip(facts) {
        assert_eq!(entry.leaf_index, fact.leaf_index);
        assert_eq!(entry.kind, fact.kind as u8);
        assert_eq!(entry.dest, dest_of(fact.kind, fact.account));
        if fact.claimed {
            let claim = entry.claim.as_ref().expect("a claimed deposit carries s and N");
            assert_eq!((claim.s, claim.nullifier), (fact.s, fact.nullifier));
        } else {
            assert!(entry.claim.is_none(), "an unclaimed deposit carries no s");
        }
    }
    // The third party's report: exactly the same deposits, with destinations and claim nullifiers.
    let report = verify(disclosure, &w.view).expect("an honest disclosure verifies");
    assert_eq!(report.deposits.len(), facts.len());
    for (r, fact) in report.deposits.iter().zip(facts) {
        assert_eq!(r.leaf_index, fact.leaf_index);
        assert_eq!(r.kind, fact.kind);
        assert_eq!(r.dest, dest_of(fact.kind, fact.account));
        assert_eq!(r.verified, fact.claimed);
        assert_eq!(r.nullifier, fact.claimed.then_some(fact.nullifier));
    }
}

// Requirement 1: Alice's disclosure verifies; the third party lists exactly Alice's deposits with
// their destinations, the claim (N) of each claimed one, and Alice's burns with their recipients.
#[test]
fn alices_disclosure_lists_her_deposits_claims_and_burns() {
    let w = world();
    let disclosure = alice_disclosure(&w);
    check_disclosure(&w.alice, &disclosure, &w);

    let report = verify(&disclosure, &w.view).unwrap();
    assert_eq!(report.chain_id, w.view.chain_id);
    assert_eq!(report.vault, w.view.vault);
    // Her burns, not Bob's and not the stranger's.
    assert_eq!(report.burns.len(), w.alice_burns.len());
    for (r, fact) in report.burns.iter().zip(&w.alice_burns) {
        assert_eq!(
            (r.i, r.burn_index, r.token, r.amount, r.recipient),
            (fact.i, fact.burn_index, fact.token, fact.amount, fact.recipient)
        );
    }
}

// Requirement 2: with Alice's disclosure, the third party learns nothing about Bob beyond the
// public view: none of Bob's deposits gets a destination or a claim link, and none of Bob's burns
// opens with Alice's kView.
#[test]
fn nothing_of_bob_beyond_the_public_view() {
    let w = world();
    let report = verify(&alice_disclosure(&w), &w.view).unwrap();

    let bob_leafs: HashSet<u64> = w.bob.iter().map(|d| d.leaf_index).collect();
    let alice_leafs: HashSet<u64> = w.alice.iter().map(|d| d.leaf_index).collect();
    assert!(report.deposits.iter().all(|d| alice_leafs.contains(&d.leaf_index)), "no deposit but Alice's is listed");
    assert!(
        !report.deposits.iter().any(|d| bob_leafs.contains(&d.leaf_index)),
        "none of Bob's deposits gets a destination or a claim link"
    );

    let bob_nullifiers: HashSet<Hash> = w.bob.iter().map(|d| d.nullifier).collect();
    assert!(
        report.deposits.iter().all(|d| d.nullifier.is_none_or(|n| !bob_nullifiers.contains(&n))),
        "no claim of Bob's appears"
    );

    let bob_recipients: HashSet<[u8; 20]> = w.bob_burns.iter().map(|b| b.recipient).collect();
    let bob_burn_index: HashSet<u64> = w.bob_burns.iter().map(|b| b.burn_index).collect();
    assert!(
        !report.burns.iter().any(|b| bob_recipients.contains(&b.recipient) || bob_burn_index.contains(&b.burn_index)),
        "none of Bob's burns opens with Alice's kView"
    );
    assert_eq!(report.burns.len(), w.alice_burns.len(), "exactly Alice's burns");

    // And the mirror image: Bob's disclosure says nothing of Alice's.
    let bob_report = verify(&bob_disclosure(&w), &w.view).unwrap();
    assert!(!bob_report.deposits.iter().any(|d| alice_leafs.contains(&d.leaf_index)));
    assert_eq!(bob_report.deposits.len(), w.bob.len());
    assert_eq!(bob_report.burns.len(), w.bob_burns.len());
}

// Requirement 3: tampering is refused, each with a named error.
#[test]
fn tampering_is_refused_with_named_errors() {
    let w = world();
    let disclosure = alice_disclosure(&w);
    // Tamper a received document: serialise, parse, mutate.
    let json = serde_json::to_string(&disclosure).unwrap();
    let doc: Disclosure = serde_json::from_str(&json).unwrap();

    // Changed dest of a claimed deposit (leaf 1).
    let mut d = doc.clone();
    d.deposits[0].dest[9] ^= 1;
    assert_eq!(verify(&d, &w.view), Err(DisclosureError::Mismatch { leaf_index: 1, field: "dest" }));

    // Changed kind of a claimed deposit (leaf 1, private declared public).
    let mut d = doc.clone();
    d.deposits[0].kind = DestKind::Public as u8;
    assert_eq!(verify(&d, &w.view), Err(DisclosureError::Mismatch { leaf_index: 1, field: "kind" }));
    // ... and an invalid kind byte is refused the same way.
    let mut d = doc.clone();
    d.deposits[0].kind = 0x07;
    assert_eq!(verify(&d, &w.view), Err(DisclosureError::Mismatch { leaf_index: 1, field: "kind" }));

    // Another user's leafIndex (Bob's claimed leaf 2) on Alice's entry.
    let mut d = doc.clone();
    d.deposits[0].leaf_index = w.bob[0].leaf_index;
    assert_eq!(verify(&d, &w.view), Err(DisclosureError::Mismatch { leaf_index: 2, field: "depositor" }));
    // A leaf index no deposit sits at.
    let mut d = doc.clone();
    d.deposits[0].leaf_index = w.view.deposits.len() as u64;
    assert_eq!(verify(&d, &w.view), Err(DisclosureError::UnknownLeaf { leaf_index: 9 }));

    // An N that is not spent: another nullifier under Alice's honest s (leaf 1).
    let mut d = doc.clone();
    d.deposits[0].claim.as_mut().unwrap().nullifier = h("nobody spent this");
    assert_eq!(verify(&d, &w.view), Err(DisclosureError::Mismatch { leaf_index: 1, field: "nullifier" }));

    // An entry marked claimed that is not: a consistent, forged claim on Alice's unclaimed deposit
    // (leaf 7). The s and N are right, but the nullifier is not spent.
    let mut d = doc.clone();
    let unclaimed = d.deposits.iter_mut().find(|e| e.leaf_index == w.alice[2].leaf_index).unwrap();
    unclaimed.claim = Some(ClaimSecret { s: w.alice[2].s, nullifier: w.alice[2].nullifier });
    assert_eq!(verify(&d, &w.view), Err(DisclosureError::UnspentNullifier { leaf_index: 7 }));

    // The s of another deposit (Alice's second claimed deposit's s, leaf 4) under leaf 1's N.
    let mut d = doc.clone();
    d.deposits[0].claim.as_mut().unwrap().s = w.alice[1].s;
    assert_eq!(verify(&d, &w.view), Err(DisclosureError::Mismatch { leaf_index: 1, field: "s" }));

    // Another context entirely.
    let mut d = doc.clone();
    d.chain_id[31] ^= 1;
    assert_eq!(verify(&d, &w.view), Err(DisclosureError::ContextMismatch { field: "chainId" }));
    let mut d = doc.clone();
    d.vault[0] ^= 1;
    assert_eq!(verify(&d, &w.view), Err(DisclosureError::ContextMismatch { field: "vault" }));

    // A duplicated entry.
    let mut d = doc;
    d.deposits.push(d.deposits[0].clone());
    assert_eq!(verify(&d, &w.view), Err(DisclosureError::Mismatch { leaf_index: 1, field: "leafIndex" }));

    // The untampered document still verifies.
    assert!(verify(
        &serde_json::from_str::<Disclosure>(&serde_json::to_string(&disclosure).unwrap()).unwrap(),
        &w.view
    )
    .is_ok());
}

// Requirement 4: no capability leaks. The JSON carries neither kAuth, nor the bridge seed, nor any
// sOut of Alice's burns; her unclaimed deposit has no s.
#[test]
fn no_capability_leaks_in_the_document() {
    let w = world();
    let json = serde_json::to_string(&alice_disclosure(&w)).unwrap();

    // kView is there (the burns need it) ...
    let (k_auth, k_view) = derive_keys(&w.alice_seed);
    assert!(json.contains(&hex(&k_view)), "kView is the disclosed viewing key");
    // ... and nothing that authorises.
    assert!(!json.contains(&hex(&k_auth)), "kAuth never appears");
    assert!(!json.contains(&hex(&w.alice_seed)), "the bridge seed never appears");
    for burn in &w.alice_burns {
        assert!(!json.contains(&hex(&burn.s_out)), "no sOut of Alice's burns appears");
    }
    // The only secrets present are the s of claimed deposits, which grant nothing after the claim.
    for fact in w.alice.iter().filter(|d| d.claimed) {
        assert!(json.contains(&hex(&fact.s)), "a claimed deposit's s is disclosed by design");
    }
    assert!(!json.contains(&hex(&w.alice[2].s)), "the unclaimed deposit's s is not disclosed");

    // Alice's unclaimed deposit has no s, in the parsed document and in the text.
    let parsed: Disclosure = serde_json::from_str(&json).unwrap();
    let unclaimed = parsed.deposits.iter().find(|d| d.leaf_index == w.alice[2].leaf_index).unwrap();
    assert!(unclaimed.claim.is_none());
    let unclaimed_json = serde_json::to_string(unclaimed).unwrap();
    assert!(!unclaimed_json.contains("\"s\":"), "the unclaimed entry carries no s");
}

// Requirement 5: JSON round trip: serialise, parse, verify gives the same report.
#[test]
fn json_round_trip_gives_the_same_report() {
    let w = world();
    let disclosure = alice_disclosure(&w);
    let json = serde_json::to_string(&disclosure).unwrap();
    let parsed: Disclosure = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, disclosure, "the document survives the round trip unchanged");
    assert_eq!(verify(&parsed, &w.view), verify(&disclosure, &w.view), "the same report");
    // And Bob's, too.
    let bob = bob_disclosure(&w);
    let parsed: Disclosure = serde_json::from_str(&serde_json::to_string(&bob).unwrap()).unwrap();
    assert_eq!(parsed, bob);
    assert_eq!(verify(&parsed, &w.view), verify(&bob, &w.view));
}
