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

#![allow(dead_code)] // Each integration-test target uses a different subset of this shared world.

use std::collections::HashSet;

use ferrata_bridge_core::encoding::u256_from_u128;
use ferrata_bridge_core::inbound::{commitment, deposit_id, deposit_leaf, deposit_secret, nullifier, DestKind};
use ferrata_bridge_core::outbound::{burn_secret, BurnNote, PublishedBurn, ReleaseTarget};
use ferrata_bridge_core::{chain_id_u256, sha256, BridgeKeys, Hash, U256};
use ferrata_disclose::{make_disclosure, Disclosure, PublicDeposit, PublicView, WalletKeys};

/// A labelled 32-byte value: `sha256(label)`.
pub fn h(label: &str) -> Hash {
    sha256(label.as_bytes())
}

/// A labelled address: the first 20 bytes of `sha256(label)`.
fn addr(label: &str) -> [u8; 20] {
    h(label)[..20].try_into().unwrap()
}

/// Stand-in for the wallet's seed-derived LEZ account ids: account `a` of kind `kind`. Both Alice
/// and Bob use this scheme; their keys differ, so their commitments differ.
pub fn dest_of(kind: DestKind, a: u32) -> Hash {
    sha256(&[b"disclose test seed-derived account".as_slice(), &[kind as u8], &a.to_be_bytes()].concat())
}

/// A whole number as the `uint256` every preimage carries.
pub fn u256(units: u128) -> U256 {
    u256_from_u128(units)
}

/// Lowercase hex of `bytes`, as the document spells its fields.
pub fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        write!(text, "{b:02x}").unwrap();
    }
    text
}

/// Native ETH.
pub const ETH: [u8; 20] = [0u8; 20];

/// A deposit the test planted for a known user.
pub struct DepositFact {
    pub leaf_index: u64,
    pub kind: DestKind,
    pub account: u32,
    pub claimed: bool,
    pub s: Hash,
    pub nullifier: Hash,
}

/// A burn the test planted for a known user.
pub struct BurnFact {
    pub i: u64,
    pub burn_index: u64,
    pub token: [u8; 20],
    pub amount: U256,
    pub recipient: [u8; 20],
    pub s_out: Hash,
}

/// The world: the public view and the facts the tests assert against.
pub struct World {
    pub view: PublicView,
    pub alice: Vec<DepositFact>,
    pub bob: Vec<DepositFact>,
    pub alice_burns: Vec<BurnFact>,
    pub bob_burns: Vec<BurnFact>,
    pub alice_seed: Hash,
    pub bob_seed: Hash,
}
/// A deposit by `keys`, committed to `dest_of(kind, account)`: appended to the vault's public
/// leaves, and, when `claimed`, its nullifier recorded as spent.
#[allow(clippy::too_many_arguments)]
pub fn push_deposit(
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
pub fn push_burn(
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
pub fn world() -> World {
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
pub const GAP: u32 = 4;

/// Alice's honest disclosure, as her wallet would write it.
pub fn alice_disclosure(w: &World) -> Disclosure {
    make_disclosure(&WalletKeys::from_bridge_seed(&w.alice_seed), &w.view, GAP, dest_of)
}

/// Bob's honest disclosure.
pub fn bob_disclosure(w: &World) -> Disclosure {
    make_disclosure(&WalletKeys::from_bridge_seed(&w.bob_seed), &w.view, GAP, dest_of)
}
