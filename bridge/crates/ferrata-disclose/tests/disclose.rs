//! Voluntary disclosure of a user's deposits, claims and burns (RFP-022 soft requirement 2).
//!
//! Everything here uses only public data and the user's own disclosure: the "public view" is
//! what anyone reads from the chains (the vault's `Deposit` leaves, the LEZ bridge's spent
//! nullifiers, the burn notes published on LEZ), and each disclosure is built by the user's own
//! wallet from its own keys. No guest, no LEZ program, no vault and no other protocol component
//! discloses anything: the only private input is the file the user chooses to hand over.
//!
//! The world: two users, Alice and Bob, each with seed-derived keys; deposits from both (some
//! claimed, one of Alice's unclaimed) and from strangers; burns from both and a stranger.

use std::collections::HashSet;

use ferrata_bridge_core::encoding::u256_from_u128;
use ferrata_bridge_core::inbound::{commitment, deposit_id, deposit_leaf, deposit_secret, nullifier, DestKind};
use ferrata_bridge_core::outbound::{burn_secret, BurnNote, PublishedBurn, ReleaseTarget};
use ferrata_bridge_core::{chain_id_u256, derive_keys, sha256, BridgeKeys, Hash, U256};
use ferrata_disclose::{
    make_disclosure, verify, ClaimSecret, Disclosure, DisclosureError, PublicDeposit, PublicView, WalletKeys,
};

/// A labelled 32-byte value: `sha256(label)`.
fn h(label: &str) -> Hash {
    sha256(label.as_bytes())
}

/// A labelled address: the first 20 bytes of `sha256(label)`.
fn addr(label: &str) -> [u8; 20] {
    h(label)[..20].try_into().unwrap()
}

/// Stand-in for the wallet's seed-derived LEZ account ids: account `a` of kind `kind`. Both Alice
/// and Bob use this scheme; their keys differ, so their commitments differ.
fn dest_of(kind: DestKind, a: u32) -> Hash {
    sha256(&[b"disclose test seed-derived account".as_slice(), &[kind as u8], &a.to_be_bytes()].concat())
}

/// A whole number as the `uint256` every preimage carries.
fn u256(units: u128) -> U256 {
    u256_from_u128(units)
}

/// Lowercase hex of `bytes`, as the document spells its fields.
fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        write!(text, "{b:02x}").unwrap();
    }
    text
}

/// Native ETH.
const ETH: [u8; 20] = [0u8; 20];

/// A deposit the test planted for a known user.
struct DepositFact {
    leaf_index: u64,
    kind: DestKind,
    account: u32,
    claimed: bool,
    s: Hash,
    nullifier: Hash,
}

/// A burn the test planted for a known user.
struct BurnFact {
    i: u64,
    burn_index: u64,
    token: [u8; 20],
    amount: U256,
    recipient: [u8; 20],
    s_out: Hash,
}

/// The world: the public view and the facts the tests assert against.
struct World {
    view: PublicView,
    alice: Vec<DepositFact>,
    bob: Vec<DepositFact>,
    alice_burns: Vec<BurnFact>,
    bob_burns: Vec<BurnFact>,
    alice_seed: Hash,
    bob_seed: Hash,
}

/// A deposit by `keys`, committed to `dest_of(kind, account)`: appended to the vault's public
/// leaves, and, when `claimed`, its nullifier recorded as spent.
#[allow(clippy::too_many_arguments)]
fn push_deposit(
    world: &mut World,
    keys: &BridgeKeys,
    depositor: [u8; 20],
    j: u64,
    kind: DestKind,
    account: u32,
    token: [u8; 20],
    amount: U256,
    claimed: bool,
) -> DepositFact {
    let leaf_index = world.view.deposits.len() as u64;
    let s = deposit_secret(&keys.auth, &world.view.chain_id, &world.view.vault, &depositor, j);
    let c = commitment(&s, kind, &dest_of(kind, account));
    world.view.deposits.push(PublicDeposit { leaf_index, depositor, j, token, amount, commitment: c });
    let leaf = deposit_leaf(leaf_index, &depositor, &token, &amount, &c);
    let nullifier = nullifier(&s, &deposit_id(&world.view.chain_id, &world.view.vault, leaf_index, &leaf));
    if claimed {
        world.view.spent_nullifiers.insert(nullifier);
    }
    DepositFact { leaf_index, kind, account, claimed, s, nullifier }
}

/// A stranger's deposit: public fields with a synthetic commitment.
fn push_stranger_deposit(world: &mut World, label: &str) {
    let leaf_index = world.view.deposits.len() as u64;
    world.view.deposits.push(PublicDeposit {
        leaf_index,
        depositor: addr(&format!("{label} depositor")),
        j: 0,
        token: addr(&format!("{label} token")),
        amount: u256(7),
        commitment: h(&format!("{label} commitment")),
    });
}

/// A burn note by `keys` at counter `i`: published on LEZ, in the public view's burn list.
fn push_burn(
    world: &mut World,
    keys: &BridgeKeys,
    i: u64,
    recipient: [u8; 20],
    token: [u8; 20],
    amount: U256,
) -> BurnFact {
    let target = ReleaseTarget { chain_id: world.view.chain_id, vault: world.view.vault, recipient };
    let note = BurnNote::create(keys, &world.view.config_id, i, &target, token, amount);
    let s_out = burn_secret(&keys.auth, &world.view.config_id, &note.r);
    let burn_index = world.view.burns.len() as u64;
    world.view.burns.push(PublishedBurn { burn_index, note });
    BurnFact { i, burn_index, token, amount, recipient, s_out }
}

/// The shared world. Leaf indices, in order: a stranger (0), Alice (1), Bob (2), a stranger (3),
/// Alice (4), Bob (5), a stranger (6), Alice (7, unclaimed), Bob (8, unclaimed). Burns: Alice's
/// counter 0 is leaf 0, Bob's counter 0 is leaf 1, a stranger's is leaf 2, Alice's counter 1 is
/// leaf 3.
fn world() -> World {
    let alice_seed = h("alice bridge seed");
    let bob_seed = h("bob bridge seed");
    let alice = BridgeKeys::from_bridge_seed(&alice_seed);
    let bob = BridgeKeys::from_bridge_seed(&bob_seed);
    let mut w = World {
        view: PublicView {
            chain_id: chain_id_u256(11155111),
            vault: addr("vault"),
            config_id: h("lez configuration"),
            deposits: vec![],
            spent_nullifiers: HashSet::new(),
            burns: vec![],
        },
        alice: vec![],
        bob: vec![],
        alice_burns: vec![],
        bob_burns: vec![],
        alice_seed,
        bob_seed,
    };

    push_stranger_deposit(&mut w, "stranger one");
    let d = push_deposit(
        &mut w,
        &alice,
        addr("alice depositor"),
        0,
        DestKind::Private,
        0,
        ETH,
        u256(1_000_000_000_000_000_000),
        true,
    );
    w.alice.push(d);
    let d =
        push_deposit(&mut w, &bob, addr("bob depositor"), 0, DestKind::Public, 1, addr("USDC"), u256(25_000_000), true);
    w.bob.push(d);
    push_stranger_deposit(&mut w, "stranger two");
    let d = push_deposit(&mut w, &alice, addr("alice depositor"), 1, DestKind::Public, 2, addr("DAI"), u256(500), true);
    w.alice.push(d);
    let d = push_deposit(
        &mut w,
        &bob,
        addr("bob depositor"),
        1,
        DestKind::Private,
        0,
        ETH,
        u256(2_000_000_000_000_000_000),
        true,
    );
    w.bob.push(d);
    push_stranger_deposit(&mut w, "stranger three");
    let d =
        push_deposit(&mut w, &alice, addr("alice depositor"), 2, DestKind::Private, 1, addr("WETH"), u256(3), false);
    w.alice.push(d);
    let d = push_deposit(&mut w, &bob, addr("bob depositor"), 2, DestKind::Public, 3, ETH, u256(9_000_000_000), false);
    w.bob.push(d);

    let stranger = BridgeKeys::from_bridge_seed(&h("stranger bridge seed"));
    let b = push_burn(&mut w, &alice, 0, addr("alice ETH recipient"), ETH, u256(1_000_000_000));
    w.alice_burns.push(b);
    let b = push_burn(&mut w, &bob, 0, addr("bob recipient"), addr("USDC"), u256(100));
    w.bob_burns.push(b);
    push_burn(&mut w, &stranger, 0, addr("stranger recipient"), ETH, u256(5));
    let b = push_burn(&mut w, &alice, 1, addr("alice USDC recipient"), addr("USDC"), u256(42));
    w.alice_burns.push(b);
    w
}

/// The gap covering every planted destination (account indices 0..4, both kinds).
const GAP: u32 = 4;

/// Alice's honest disclosure, as her wallet would write it.
fn alice_disclosure(w: &World) -> Disclosure {
    make_disclosure(&WalletKeys::from_bridge_seed(&w.alice_seed), &w.view, GAP, dest_of)
}

/// Bob's honest disclosure.
fn bob_disclosure(w: &World) -> Disclosure {
    make_disclosure(&WalletKeys::from_bridge_seed(&w.bob_seed), &w.view, GAP, dest_of)
}

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
