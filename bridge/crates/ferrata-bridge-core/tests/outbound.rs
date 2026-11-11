//! Burn -> release (SPEC-v2 §5): the release statement and its journal, burn notes and their MAC, the
//! burn-tag registry, and burn recovery. Tests named `attack_*` are negative tests for specific
//! attacks.

mod common;

use std::collections::HashSet;

use common::{addr, chain, h, keys, release_case};
use ferrata_bridge_core::encoding::{u256_from_u128, ReleaseJournal};
use ferrata_bridge_core::outbound::{
    burn_commitment, burn_id, burn_leaf, burn_mac, burn_nullifier, burn_secret, burn_tag, memo_pad, recover_burns,
    register_burn_tag, release_journal, verify_release, view_burns, xor_memo, BurnNote, PublishedBurn, ReleasePublic,
    ReleaseTarget, ReleaseWitness, RepeatedTag,
};
use ferrata_bridge_core::tree::{Tree, MAX_LEAVES};
use ferrata_bridge_core::{derive_keys, BridgeKey, BridgeKeys, Hash, VerifyError};
use proptest::prelude::*;

const RELEASE_FIELDS: [&str; 14] = [
    "burn_root",
    "burn_index",
    "directions",
    "sibling",
    "s_out",
    "r",
    "chain_id",
    "vault",
    "config_id",
    "token",
    "amount",
    "recipient",
    "relayer",
    "relay_fee",
];

proptest! {
    #[test]
    fn release_verifies_and_commits_the_vault_journal(case in release_case()) {
        let (w, p) = (&case.witness, &case.public);
        let n = verify_release(&case.burn_root, w, p).unwrap();
        prop_assert_eq!(n, burn_nullifier(&w.s_out), "D13: NOut depends on sOut alone");
        let j = ReleaseJournal::decode(&release_journal(&case.burn_root, &n, p)).unwrap();
        prop_assert_eq!((j.burn_root, j.nullifier, j.chain_id, j.vault, j.lez_config_id), (case.burn_root, n, p.chain_id, p.vault, p.config_id));
        prop_assert_eq!((j.token, j.amount, j.recipient, j.relayer, j.relay_fee), (p.token, p.amount, p.recipient, p.relayer, p.relay_fee));
    }

    /// Changing any one field: everything that feeds the burn leaf or its path fails; the relayer
    /// and the fee, chosen at proving time, leave verification alone and change the journal.
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
            "burn_index" => { prop_assume!(other_index != w.burn_index); w.burn_index = other_index }
            "directions" => w.path.directions ^= 1 << (at % 32),
            "sibling" => w.path.siblings[height][at] ^= mask,
            "s_out" => w.s_out[at] ^= mask,
            "r" => w.r[at] ^= mask,
            "chain_id" => p.chain_id[at] ^= mask,
            "vault" => p.vault[at % 20] ^= mask,
            "config_id" => p.config_id[at] ^= mask,
            "token" => p.token[at % 20] ^= mask,
            "amount" => p.amount[at] ^= mask,
            "recipient" => p.recipient[at % 20] ^= mask,
            "relayer" => p.relayer[at % 20] ^= mask,
            "relay_fee" => p.relay_fee[at] ^= mask,
            _ => unreachable!(),
        }
        let got = verify_release(&root, &w, &p);
        match RELEASE_FIELDS[field] {
            "relayer" | "relay_fee" => {
                prop_assert_eq!(got, Ok(n));
                prop_assert_ne!(release_journal(&root, &n, &p), journal);
            }
            "burn_index" | "directions" => prop_assert_eq!(got, Err(VerifyError::DirectionsMismatch)),
            _ => prop_assert_eq!(got, Err(VerifyError::RootMismatch)),
        }
    }

    #[test]
    fn same_burn_same_nullifier_under_every_later_root(case in release_case(), later in prop::collection::vec(any::<[u8; 32]>(), 1..20)) {
        let (w, p) = (&case.witness, &case.public);
        let leaf = burn_leaf(&p.config_id, w.burn_index, &p.token, &p.amount, &burn_commitment(&w.s_out, &p.chain_id, &p.vault, &p.recipient), &w.r);
        let mut leaves: Vec<_> = (0..=w.burn_index).map(|i| if i == w.burn_index { leaf } else { h(&format!("x{i}")) }).collect();
        let early = Tree::from_leaves(leaves.clone()).unwrap();
        leaves.extend(later);
        let late = Tree::from_leaves(leaves).unwrap();
        let at = |t: &Tree| verify_release(&t.root(), &ReleaseWitness { path: t.path(w.burn_index).unwrap(), ..w.clone() }, p);
        prop_assert_eq!(at(&early).unwrap(), at(&late).unwrap());
    }

    /// A burn appended under another configuration (another configId, even with
    /// the same chain, vault, token, amount and recipient) cannot satisfy a release for this one.
    #[test]
    fn attack_burn_from_another_config_cannot_satisfy_a_release(case in release_case(), at in 0usize..32, mask in 1u8..=255) {
        let (w, p) = (&case.witness, &case.public);
        let mut other_config = p.config_id;
        other_config[at] ^= mask;
        // The other configuration's leaf, at the same index, in a tree whose root the vault accepted.
        let c_out = burn_commitment(&w.s_out, &p.chain_id, &p.vault, &p.recipient);
        let foreign = burn_leaf(&other_config, w.burn_index, &p.token, &p.amount, &c_out, &w.r);
        let mut leaves: Vec<Hash> = (0..w.burn_index).map(|i| h(&format!("x{i}"))).collect();
        leaves.push(foreign);
        let tree = Tree::from_leaves(leaves).unwrap();
        let w2 = ReleaseWitness { path: tree.path(w.burn_index).unwrap(), ..w.clone() };
        // Proven for this configuration it fails; it only verifies for the configuration it belongs to.
        prop_assert_eq!(verify_release(&tree.root(), &w2, p), Err(VerifyError::RootMismatch));
        let as_other = ReleasePublic { config_id: other_config, ..p.clone() };
        prop_assert!(verify_release(&tree.root(), &w2, &as_other).is_ok());
    }

    /// The leaf binds token and amount, so a burn of one denomination of one token
    /// cannot release another token or another amount.
    #[test]
    fn attack_wrong_token_or_amount_fails(case in release_case(), at in 0usize..32, mask in 1u8..=255) {
        let mut p = case.public.clone();
        p.token[at % 20] ^= mask;
        prop_assert_eq!(verify_release(&case.burn_root, &case.witness, &p), Err(VerifyError::RootMismatch));
        let mut p = case.public.clone();
        p.amount[at] ^= mask;
        prop_assert_eq!(verify_release(&case.burn_root, &case.witness, &p), Err(VerifyError::RootMismatch));
        let mut p = case.public.clone();
        p.token = [0u8; 20]; // even "the same denomination of ETH"
        prop_assume!(p.token != case.public.token);
        prop_assert_eq!(verify_release(&case.burn_root, &case.witness, &p), Err(VerifyError::RootMismatch));
    }

    /// A copier who puts a victim's `(r, memo, mac, COut)` on a burn of another
    /// token or amount fails the victim's MAC check, so the victim's wallet ignores the copy.
    #[test]
    fn attack_copied_note_with_other_token_or_amount_fails_the_mac(seed in any::<[u8; 32]>(), i in any::<u64>(), at in 0usize..32, mask in 1u8..=255) {
        let k = BridgeKeys::from_bridge_seed(&seed);
        let target = ReleaseTarget { chain_id: chain(1), vault: addr("vault"), recipient: addr("victim's recipient") };
        let note = BurnNote::create(&k, &h("config"), i, &target, addr("USDC"), u256_from_u128(1_000_000_000));
        prop_assert!(note.mac_verifies(&k.view));
        let mut rare_token = note;
        rare_token.token[at % 20] ^= mask;
        prop_assert!(!rare_token.mac_verifies(&k.view));
        let mut rare_amount = note;
        rare_amount.amount[at] ^= mask;
        prop_assert!(!rare_amount.mac_verifies(&k.view));
        let mut other_c_out = note;
        other_c_out.c_out[at] ^= mask;
        prop_assert!(!other_c_out.mac_verifies(&k.view));
        // And the MAC is the author's: another view key does not verify it.
        prop_assert!(!note.mac_verifies(&keys("someone else").view));
    }

    /// Every derivation changes when any one of its inputs changes.
    #[test]
    fn derivations_bind_every_input(seed in any::<[u8; 32]>(), config in any::<[u8; 32]>(), i in any::<u64>(),
                                    chain_id in any::<[u8; 32]>(), vault in any::<[u8; 20]>(), recipient in any::<[u8; 20]>(),
                                    burn_index in any::<u64>(), token in any::<[u8; 20]>(), amount in any::<[u8; 32]>(),
                                    at in 0usize..32, mask in 1u8..=255) {
        let (k_auth, k_view) = derive_keys(&seed);
        let (auth, view) = (BridgeKey::new(&k_auth), BridgeKey::new(&k_view));
        let mut seed2 = seed; seed2[at] ^= mask;
        let (k_auth2, k_view2) = derive_keys(&seed2);
        let (auth2, view2) = (BridgeKey::new(&k_auth2), BridgeKey::new(&k_view2));
        let bump = u64::from(mask) << (8 * (at % 8));
        let mut config2 = config; config2[at] ^= mask;

        let r = burn_tag(&view, &config, i);
        prop_assert_ne!(r, burn_tag(&view2, &config, i));
        prop_assert_ne!(r, burn_tag(&view, &config2, i));
        prop_assert_ne!(r, burn_tag(&view, &config, i ^ bump));
        prop_assert_ne!(r, burn_tag(&auth, &config, i)); // the tag is a kView value

        let mut r2 = r; r2[at] ^= mask;
        let s_out = burn_secret(&auth, &config, &r);
        prop_assert_ne!(s_out, burn_secret(&auth2, &config, &r));
        prop_assert_ne!(s_out, burn_secret(&auth, &config2, &r));
        prop_assert_ne!(s_out, burn_secret(&auth, &config, &r2));
        prop_assert_ne!(s_out, burn_secret(&view, &config, &r)); // sOut is a kAuth value

        let pad = memo_pad(&view, &r);
        prop_assert_ne!(pad, memo_pad(&view2, &r));
        prop_assert_ne!(pad, memo_pad(&view, &r2));

        let c_out = burn_commitment(&s_out, &chain_id, &vault, &recipient);
        let mut s2 = s_out; s2[at] ^= mask;
        let mut chain2 = chain_id; chain2[at] ^= mask;
        let mut vault2 = vault; vault2[at % 20] ^= mask;
        let mut recipient2 = recipient; recipient2[at % 20] ^= mask;
        prop_assert_ne!(c_out, burn_commitment(&s2, &chain_id, &vault, &recipient));
        prop_assert_ne!(c_out, burn_commitment(&s_out, &chain2, &vault, &recipient));
        prop_assert_ne!(c_out, burn_commitment(&s_out, &chain_id, &vault2, &recipient));
        prop_assert_ne!(c_out, burn_commitment(&s_out, &chain_id, &vault, &recipient2));

        let memo = recipient;
        let mac = burn_mac(&view, &r, &memo, &token, &amount, &c_out);
        let mut memo2 = memo; memo2[at % 20] ^= mask;
        let mut token2 = token; token2[at % 20] ^= mask;
        let mut amount2 = amount; amount2[at] ^= mask;
        let mut c2 = c_out; c2[at] ^= mask;
        prop_assert_ne!(mac, burn_mac(&view2, &r, &memo, &token, &amount, &c_out));
        prop_assert_ne!(mac, burn_mac(&view, &r2, &memo, &token, &amount, &c_out));
        prop_assert_ne!(mac, burn_mac(&view, &r, &memo2, &token, &amount, &c_out)); // D12: the memo is covered
        prop_assert_ne!(mac, burn_mac(&view, &r, &memo, &token2, &amount, &c_out));
        prop_assert_ne!(mac, burn_mac(&view, &r, &memo, &token, &amount2, &c_out));
        prop_assert_ne!(mac, burn_mac(&view, &r, &memo, &token, &amount, &c2));

        let leaf = burn_leaf(&config, burn_index, &token, &amount, &c_out, &r);
        prop_assert_ne!(leaf, burn_leaf(&config2, burn_index, &token, &amount, &c_out, &r));
        prop_assert_ne!(leaf, burn_leaf(&config, burn_index ^ bump, &token, &amount, &c_out, &r));
        prop_assert_ne!(leaf, burn_leaf(&config, burn_index, &token2, &amount, &c_out, &r));
        prop_assert_ne!(leaf, burn_leaf(&config, burn_index, &token, &amount2, &c_out, &r));
        prop_assert_ne!(leaf, burn_leaf(&config, burn_index, &token, &amount, &c2, &r));
        prop_assert_ne!(leaf, burn_leaf(&config, burn_index, &token, &amount, &c_out, &r2));

        let bid = burn_id(&config, burn_index, &leaf);
        let mut leaf2 = leaf; leaf2[at] ^= mask;
        prop_assert_ne!(bid, burn_id(&config2, burn_index, &leaf));
        prop_assert_ne!(bid, burn_id(&config, burn_index ^ bump, &leaf));
        prop_assert_ne!(bid, burn_id(&config, burn_index, &leaf2));

        let n = burn_nullifier(&s_out);
        prop_assert_ne!(n, burn_nullifier(&s2));
    }

    #[test]
    fn memo_round_trips(seed in any::<[u8; 32]>(), config in any::<[u8; 32]>(), i in any::<u64>(), recipient in any::<[u8; 20]>(), chain_id in any::<[u8; 32]>(), vault in any::<[u8; 20]>()) {
        let k = BridgeKeys::from_bridge_seed(&seed);
        let target = ReleaseTarget { chain_id, vault, recipient };
        let note = BurnNote::create(&k, &config, i, &target, addr("token"), u256_from_u128(1));
        prop_assert_eq!(xor_memo(&k.view, &note.r, &note.memo), recipient);
        prop_assert_eq!(note.c_out, burn_commitment(&burn_secret(&k.auth, &config, &note.r), &chain_id, &vault, &recipient));
    }
}

/// The bridge program refuses a repeated `r`, with this helper.
#[test]
fn attack_repeated_burn_tag_is_refused_by_the_registry() {
    let k = keys("seed");
    let target = ReleaseTarget { chain_id: chain(1), vault: addr("vault"), recipient: addr("recipient") };
    let config = h("config");
    let first = BurnNote::create(&k, &config, 0, &target, addr("USDC"), u256_from_u128(100));
    let reused_i = BurnNote::create(&k, &config, 0, &target, addr("DAI"), u256_from_u128(100));
    let next = BurnNote::create(&k, &config, 1, &target, addr("USDC"), u256_from_u128(100));
    let mut copied = next;
    copied.token = addr("rare token");
    let mut tags: HashSet<Hash> = HashSet::new();
    assert_eq!(register_burn_tag(&mut tags, &first), Ok(()));
    assert_eq!(register_burn_tag(&mut tags, &reused_i), Err(RepeatedTag)); // a wallet reusing i
    assert_eq!(register_burn_tag(&mut tags, &next), Ok(()));
    assert_eq!(register_burn_tag(&mut tags, &copied), Err(RepeatedTag)); // a copier reusing r
}

#[test]
fn recover_burns_follows_the_gap_limit_and_ignores_notes_it_did_not_author() {
    let k = keys("our seed");
    let other = keys("other seed");
    let config = h("lez config");
    let (chain_id, vault) = (chain(1), addr("vault"));
    let to = |who: &str| ReleaseTarget { chain_id, vault, recipient: addr(who) };
    let amount = u256_from_u128(100_000_000);

    // Ours at counters 0, 1, 2, 5 and 9; strangers' notes interleaved.
    let mut notes = Vec::new();
    let mut push = |note: BurnNote| {
        let burn_index = notes.len() as u64;
        notes.push(PublishedBurn { burn_index, note });
    };
    for (i, who) in [(0, "alice"), (1, "bob"), (2, "alice"), (5, "carol"), (9, "dave")] {
        push(BurnNote::create(&other, &config, i, &to("stranger"), addr("USDC"), amount));
        push(BurnNote::create(&k, &config, i, &to(who), addr("USDC"), amount));
    }
    // A front-run copy of our r_3 on a rare token (our own burn with r_3 then failed on the repeated
    // tag): the MAC fails, so it is ignored, but counter 3 counts as used.
    let mut copy = BurnNote::create(&k, &config, 3, &to("erin"), addr("USDC"), amount);
    copy.token = addr("rare token");
    push(copy);
    // A copy of r_1 with an identical token, amount, COut and MAC but a tampered memo: D12's MAC
    // covers the memo, so even the view-only scan drops it (under SPEC-v2's MAC it verified, the
    // view-only scan reported a wrong recipient, and only the full scan dropped it on COut).
    let mut tampered = BurnNote::create(&k, &config, 1, &to("bob"), addr("USDC"), amount);
    tampered.memo[0] ^= 1;
    push(tampered);

    let scan = recover_burns(&k, &config, &chain_id, &vault, &notes, 3);
    let got: Vec<(u64, [u8; 20])> = scan.owned.iter().map(|o| (o.i, o.recipient)).collect();
    assert_eq!(got, vec![(0, addr("alice")), (1, addr("bob")), (2, addr("alice")), (5, addr("carol"))]);
    assert_eq!(scan.next_i, 6);
    assert_eq!(recover_burns(&k, &config, &chain_id, &vault, &notes, 4).owned.len(), 5);

    let (viewed, next) = view_burns(&k.view, &config, &notes, 3);
    assert_eq!(next, 6);
    assert_eq!(viewed.len(), 4, "4 own; the tampered-memo copy's MAC does not verify (D12)");
    assert!(viewed.iter().all(|v| v.burn.note.token == addr("USDC")), "the rare-token copy is not viewed");

    // Each recovered burn releases with the recovered nullifier.
    let tree = Tree::from_leaves(
        notes
            .iter()
            .map(|b| burn_leaf(&config, b.burn_index, &b.note.token, &b.note.amount, &b.note.c_out, &b.note.r))
            .collect(),
    )
    .unwrap();
    for o in &scan.owned {
        let public = ReleasePublic {
            chain_id,
            vault,
            config_id: config,
            token: o.burn.note.token,
            amount: o.burn.note.amount,
            recipient: o.recipient,
            relayer: [0u8; 20],
            relay_fee: [0u8; 32],
        };
        let w = ReleaseWitness {
            burn_index: o.burn.burn_index,
            path: tree.path(o.burn.burn_index).unwrap(),
            s_out: o.s_out,
            r: o.burn.note.r,
        };
        assert_eq!(verify_release(&tree.root(), &w, &public), Ok(o.nullifier));
    }
    // Worthless to another chain, vault or configuration; a gap limit of 0 checks nothing.
    assert!(recover_burns(&k, &config, &chain(2), &vault, &notes, 4).owned.is_empty());
    assert!(recover_burns(&k, &config, &chain_id, &addr("other vault"), &notes, 4).owned.is_empty());
    assert!(recover_burns(&k, &h("other config"), &chain_id, &vault, &notes, 4).owned.is_empty());
    assert!(recover_burns(&k, &config, &chain_id, &vault, &notes, 0).owned.is_empty());
}
