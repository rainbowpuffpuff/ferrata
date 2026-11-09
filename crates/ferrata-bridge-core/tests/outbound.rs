//! Burn -> release: the release statement, its journal, burn notes and burn recovery.

mod common;

use common::{addr, h, own_burn, release_case};
use ferrata_bridge_core::encoding::{u256_from_u128, ReleaseJournal};
use ferrata_bridge_core::outbound::{
    burn_commitment, burn_id, burn_leaf, burn_nullifier, burn_secret, burn_tag, memo_pad, recover_burns,
    release_journal, verify_release, xor_memo, BurnNote, PublishedBurn, ReleaseTarget, ReleaseWitness,
};
use ferrata_bridge_core::tree::{Tree, MAX_LEAVES};
use ferrata_bridge_core::{BridgeKey, VerifyError};
use proptest::prelude::*;

const RELEASE_FIELDS: [&str; 12] = [
    "burn_root",
    "leaf_index",
    "path",
    "s_out",
    "chain_id",
    "vault",
    "lez_config_id",
    "token",
    "amount",
    "recipient",
    "fee_recipient",
    "relay_fee",
];

proptest! {
    #[test]
    fn release_verifies_and_commits_the_vault_journal(case in release_case()) {
        let (w, p) = (&case.witness, &case.public);
        let n = verify_release(&case.burn_root, w, p).unwrap();
        let leaf = burn_leaf(w.leaf_index, &p.token, &p.amount, &burn_commitment(&w.s_out, p.chain_id, &p.vault, &p.recipient));
        prop_assert_eq!(n, burn_nullifier(&w.s_out, &burn_id(&p.lez_config_id, w.leaf_index, &leaf)));
        let journal = ReleaseJournal::decode(&release_journal(&case.burn_root, &n, p)).unwrap();
        prop_assert_eq!((journal.burn_root, journal.nullifier, journal.chain_id, journal.vault), (case.burn_root, n, p.chain_id, p.vault));
        prop_assert_eq!((journal.token, journal.amount, journal.recipient), (p.token, p.amount, p.recipient));
        prop_assert_eq!((journal.fee_recipient, journal.relay_fee, journal.lez_config_id), (p.fee_recipient, p.relay_fee, p.lez_config_id));
    }

    /// Changing any one field: everything that feeds the burn leaf or its path fails verification;
    /// `lez_config_id` (it feeds only the burn id) changes `N_out`; the two fee fields, chosen at
    /// proving time, leave verification alone. Every public field changes the journal.
    #[test]
    fn every_single_field_change_fails_or_changes_the_journal(
        case in release_case(), field in 0usize..RELEASE_FIELDS.len(), at in 0usize..32, mask in 1u8..=255,
        height in 0usize..32, other_index in 0u64..MAX_LEAVES,
    ) {
        let n = verify_release(&case.burn_root, &case.witness, &case.public).unwrap();
        let journal = release_journal(&case.burn_root, &n, &case.public);
        let (mut root, mut w, mut p) = (case.burn_root, case.witness.clone(), case.public.clone());
        match RELEASE_FIELDS[field] {
            "burn_root" => root[at] ^= mask,
            "leaf_index" => { prop_assume!(other_index != w.leaf_index); w.leaf_index = other_index }
            "path" => w.path[height][at] ^= mask,
            "s_out" => w.s_out[at] ^= mask,
            "chain_id" => p.chain_id ^= u64::from(mask) << (8 * (at % 8)),
            "vault" => p.vault[at % 20] ^= mask,
            "lez_config_id" => p.lez_config_id[at] ^= mask,
            "token" => p.token[at % 20] ^= mask,
            "amount" => p.amount[at] ^= mask,
            "recipient" => p.recipient[at % 20] ^= mask,
            "fee_recipient" => p.fee_recipient[at % 20] ^= mask,
            "relay_fee" => p.relay_fee[at] ^= mask,
            _ => unreachable!(),
        }
        let got = verify_release(&root, &w, &p);
        match RELEASE_FIELDS[field] {
            "lez_config_id" => prop_assert!(matches!(got, Ok(n2) if n2 != n)),
            "fee_recipient" | "relay_fee" => prop_assert_eq!(got, Ok(n)),
            _ => prop_assert_eq!(got, Err(VerifyError::RootMismatch)),
        }
        // For the fields that still verify, the journal (what the vault checks) differs.
        if let Ok(n2) = got {
            prop_assert_ne!(release_journal(&root, &n2, &p), journal);
        }
    }

    /// The same burn proven under two different (successive) burn roots has one nullifier.
    #[test]
    fn same_burn_same_nullifier_under_every_later_root(case in release_case(), later in prop::collection::vec(any::<[u8; 32]>(), 1..20)) {
        let (w, p) = (&case.witness, &case.public);
        let leaf = burn_leaf(w.leaf_index, &p.token, &p.amount, &burn_commitment(&w.s_out, p.chain_id, &p.vault, &p.recipient));
        let mut leaves: Vec<_> = (0..=w.leaf_index).map(|i| if i == w.leaf_index { leaf } else { h(&format!("x{i}")) }).collect();
        let early = Tree::from_leaves(leaves.clone()).unwrap();
        leaves.extend(later);
        let late = Tree::from_leaves(leaves).unwrap();
        let at = |t: &Tree| verify_release(&t.root(), &ReleaseWitness { path: t.path(w.leaf_index).unwrap(), ..w.clone() }, p);
        prop_assert_eq!(at(&early).unwrap(), at(&late).unwrap());
    }

    /// Every derivation changes when any one of its inputs changes.
    #[test]
    fn derivations_bind_every_input(k in any::<[u8; 32]>(), config in any::<[u8; 32]>(), i in any::<u64>(),
                                    chain_id in any::<u64>(), vault in any::<[u8; 20]>(), recipient in any::<[u8; 20]>(),
                                    leaf_index in any::<u64>(), token in any::<[u8; 20]>(), amount in any::<[u8; 32]>(),
                                    at in 0usize..32, mask in 1u8..=255) {
        let key = BridgeKey::new(&k);
        let mut k2 = k; k2[at] ^= mask;
        let key2 = BridgeKey::new(&k2);
        let bump = u64::from(mask) << (8 * (at % 8));
        let mut config2 = config; config2[at] ^= mask;

        let r = burn_tag(&key, &config, i);
        prop_assert_ne!(r, burn_tag(&key2, &config, i));
        prop_assert_ne!(r, burn_tag(&key, &config2, i));
        prop_assert_ne!(r, burn_tag(&key, &config, i ^ bump));

        let mut r2 = r; r2[at] ^= mask;
        let s_out = burn_secret(&key, &r);
        prop_assert_ne!(s_out, burn_secret(&key2, &r));
        prop_assert_ne!(s_out, burn_secret(&key, &r2));
        let pad = memo_pad(&key, &r);
        prop_assert_ne!(pad, memo_pad(&key2, &r));
        prop_assert_ne!(pad, memo_pad(&key, &r2));
        prop_assert_ne!(&pad[..], &s_out[..20]);

        let c_out = burn_commitment(&s_out, chain_id, &vault, &recipient);
        let mut s2 = s_out; s2[at] ^= mask;
        let mut vault2 = vault; vault2[at % 20] ^= mask;
        let mut recipient2 = recipient; recipient2[at % 20] ^= mask;
        prop_assert_ne!(c_out, burn_commitment(&s2, chain_id, &vault, &recipient));
        prop_assert_ne!(c_out, burn_commitment(&s_out, chain_id ^ bump, &vault, &recipient));
        prop_assert_ne!(c_out, burn_commitment(&s_out, chain_id, &vault2, &recipient));
        prop_assert_ne!(c_out, burn_commitment(&s_out, chain_id, &vault, &recipient2));

        let leaf = burn_leaf(leaf_index, &token, &amount, &c_out);
        let mut token2 = token; token2[at % 20] ^= mask;
        let mut amount2 = amount; amount2[at] ^= mask;
        let mut c2 = c_out; c2[at] ^= mask;
        prop_assert_ne!(leaf, burn_leaf(leaf_index ^ bump, &token, &amount, &c_out));
        prop_assert_ne!(leaf, burn_leaf(leaf_index, &token2, &amount, &c_out));
        prop_assert_ne!(leaf, burn_leaf(leaf_index, &token, &amount2, &c_out));
        prop_assert_ne!(leaf, burn_leaf(leaf_index, &token, &amount, &c2));
        // A burn leaf is never the deposit leaf of the same fields (different tag).
        prop_assert_ne!(leaf, ferrata_bridge_core::inbound::deposit_leaf(leaf_index, &token, &amount, &c_out));

        let bid = burn_id(&config, leaf_index, &leaf);
        let mut leaf2 = leaf; leaf2[at] ^= mask;
        prop_assert_ne!(bid, burn_id(&config2, leaf_index, &leaf));
        prop_assert_ne!(bid, burn_id(&config, leaf_index ^ bump, &leaf));
        prop_assert_ne!(bid, burn_id(&config, leaf_index, &leaf2));

        let n = burn_nullifier(&s_out, &bid);
        let mut bid2 = bid; bid2[at] ^= mask;
        prop_assert_ne!(n, burn_nullifier(&s2, &bid));
        prop_assert_ne!(n, burn_nullifier(&s_out, &bid2));
    }

    #[test]
    fn memo_round_trips_and_hides_the_recipient(k in any::<[u8; 32]>(), config in any::<[u8; 32]>(), i in any::<u64>(),
                                                recipient in any::<[u8; 20]>(), chain_id in any::<u64>(), vault in any::<[u8; 20]>()) {
        let key = BridgeKey::new(&k);
        let target = ReleaseTarget { chain_id, vault, recipient };
        let note = BurnNote::create(&key, &config, i, &target, addr("token"), u256_from_u128(1));
        prop_assert_eq!(xor_memo(&key, &note.r, &note.memo), recipient);
        prop_assert_ne!(note.memo, recipient);
        prop_assert_eq!(note.c_out, burn_commitment(&burn_secret(&key, &note.r), chain_id, &vault, &recipient));
    }
}

/// Two burns with the same `C_out` (a reused burn counter) are distinct leaves with distinct
/// nullifiers; each releases once.
#[test]
fn duplicate_burn_commitments_get_distinct_nullifiers() {
    let key = BridgeKey::new(&h("key"));
    let (config, token, amount) = (h("config"), addr("token"), u256_from_u128(1_000_000));
    let target = ReleaseTarget { chain_id: 1, vault: addr("vault"), recipient: addr("recipient") };
    let note = own_burn(&key, &config, 0, &target, token, amount);
    let mut tree = Tree::new();
    let a = tree.append(burn_leaf(0, &token, &amount, &note.c_out)).unwrap();
    tree.append(h("someone else")).unwrap();
    let b = tree.append(burn_leaf(2, &token, &amount, &note.c_out)).unwrap();
    let public = ferrata_bridge_core::outbound::ReleasePublic {
        chain_id: 1,
        vault: target.vault,
        lez_config_id: config,
        token,
        amount,
        recipient: target.recipient,
        fee_recipient: [0u8; 20],
        relay_fee: [0u8; 32],
    };
    let s_out = burn_secret(&key, &note.r);
    let release = |i| {
        verify_release(&tree.root(), &ReleaseWitness { leaf_index: i, path: tree.path(i).unwrap(), s_out }, &public)
    };
    assert_ne!(release(a).unwrap(), release(b).unwrap());
}

#[test]
fn recover_burns_regenerates_tags_up_to_the_gap_limit() {
    let key = BridgeKey::new(&h("our key"));
    let other = BridgeKey::new(&h("other key"));
    let config = h("lez config");
    let (chain_id, vault) = (1u64, addr("vault"));
    let to = |who: &str| ReleaseTarget { chain_id, vault, recipient: addr(who) };
    let amount = u256_from_u128(100_000_000);

    // Our burns use counters 0, 1, 2, 5 and 9; strangers' notes are interleaved.
    let mut notes = Vec::new();
    let mut push = |note: BurnNote| {
        let leaf_index = notes.len() as u64;
        notes.push(PublishedBurn { leaf_index, note });
    };
    for (i, who) in [(0, "alice"), (1, "bob"), (2, "alice"), (5, "carol"), (9, "dave")] {
        push(own_burn(&other, &config, i, &to("stranger"), addr("USDC"), amount));
        push(own_burn(&key, &config, i, &to(who), addr("USDC"), amount));
    }
    // A copy of our r_1 with a forged memo and C_out: recognised as used, not as ours.
    let mut forged = own_burn(&key, &config, 1, &to("mallory"), addr("USDC"), amount);
    forged.memo = xor_memo(&key, &forged.r, &addr("mallory"));
    forged.c_out = h("forged c_out");
    push(forged);

    // Gap limit 3: finds 0, 1, 2 and 5 (gaps of 2 after 2), then stops at 6, 7, 8.
    let scan = recover_burns(&key, &config, chain_id, &vault, &notes, 3);
    let got: Vec<(u64, [u8; 20])> = scan.owned.iter().map(|o| (o.i, o.recipient)).collect();
    assert_eq!(got, vec![(0, addr("alice")), (1, addr("bob")), (2, addr("alice")), (5, addr("carol"))]);
    assert_eq!(scan.next_i, 6);

    // Gap limit 4 reaches 9.
    let scan = recover_burns(&key, &config, chain_id, &vault, &notes, 4);
    assert_eq!(scan.owned.len(), 5);
    assert_eq!(scan.next_i, 10);

    // Each recovered burn releases, with the recovered nullifier, under the tree of all notes.
    let tree = Tree::from_leaves(
        notes.iter().map(|b| burn_leaf(b.leaf_index, &b.note.token, &b.note.amount, &b.note.c_out)).collect(),
    )
    .unwrap();
    for o in &scan.owned {
        let public = ferrata_bridge_core::outbound::ReleasePublic {
            chain_id,
            vault,
            lez_config_id: config,
            token: o.burn.note.token,
            amount: o.burn.note.amount,
            recipient: o.recipient,
            fee_recipient: [0u8; 20],
            relay_fee: [0u8; 32],
        };
        let w = ReleaseWitness {
            leaf_index: o.burn.leaf_index,
            path: tree.path(o.burn.leaf_index).unwrap(),
            s_out: o.s_out,
        };
        assert_eq!(verify_release(&tree.root(), &w, &public), Ok(o.nullifier));
    }

    // The same notes are worthless to a vault on another chain or address: C_out does not open.
    assert!(recover_burns(&key, &config, 2, &vault, &notes, 4).owned.is_empty());
    assert!(recover_burns(&key, &config, chain_id, &addr("other vault"), &notes, 4).owned.is_empty());
    // A gap limit of 0 checks nothing.
    assert!(recover_burns(&key, &config, chain_id, &vault, &notes, 0).owned.is_empty());
}
