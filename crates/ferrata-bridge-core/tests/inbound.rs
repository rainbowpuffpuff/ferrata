//! Deposit -> mint (SPEC-v2 §2, §3): keys, the claim statement, its nullifier, the nullifier
//! registry, and wallet recovery. Tests named `attack_*` are the negative tests for the reviewed
//! attacks.

mod common;

use std::collections::HashSet;

use common::{addr, chain, claim_case, dest_of, h, keys, synthetic_deposits};
use ferrata_bridge_core::encoding::u256_from_u128;
use ferrata_bridge_core::inbound::{
    commitment, deposit_id, deposit_leaf, deposit_secret, nullifier, recover, register_claim, verify_claim,
    AlreadyClaimed, ClaimWitness, DestKind, NullifierKey,
};
use ferrata_bridge_core::tree::{Tree, MAX_LEAVES};
use ferrata_bridge_core::{derive_keys, hmac_sha256, BridgeKey, VerifyError};
use proptest::prelude::*;

const CLAIM_FIELDS: [&str; 12] = [
    "root",
    "depositor",
    "token",
    "amount",
    "leaf_index",
    "directions",
    "sibling",
    "s",
    "kind",
    "dest",
    "chain_id",
    "vault",
];

#[test]
fn keys_are_split_and_derived_from_the_seed() {
    let seed = h("seed");
    let (k_auth, k_view) = derive_keys(&seed);
    assert_eq!(k_auth, hmac_sha256(&seed, b"ferrata/auth/v1"));
    assert_eq!(k_view, hmac_sha256(&seed, b"ferrata/view/v1"));
    assert_ne!(k_auth, k_view);
    let other = derive_keys(&h("other seed"));
    assert_ne!(other.0, k_auth);
    assert_ne!(other.1, k_view);
    // `s` comes from kAuth: computed with kView instead, it differs.
    let (vault, depositor) = (addr("vault"), addr("depositor"));
    assert_ne!(
        deposit_secret(&BridgeKey::new(&k_auth), &chain(1), &vault, &depositor, 0),
        deposit_secret(&BridgeKey::new(&k_view), &chain(1), &vault, &depositor, 0)
    );
}

proptest! {
    #[test]
    fn claim_verifies(case in claim_case()) {
        let w = &case.witness;
        let n = verify_claim(&case.root, w, &case.chain_id, &case.vault).unwrap();
        let leaf = deposit_leaf(w.leaf_index, &w.depositor, &w.token, &w.amount, &commitment(&w.s, w.kind, &w.dest));
        prop_assert_eq!(n, nullifier(&w.s, &deposit_id(&case.chain_id, &case.vault, w.leaf_index, &leaf)));
    }

    /// Changing any one input either fails verification (everything that feeds the leaf or its path)
    /// or changes `N` (chain id and vault, which feed only `fid`: they must be configuration
    /// constants, never claimant input).
    #[test]
    fn every_single_field_change_fails_or_changes_the_nullifier(
        case in claim_case(), field in 0usize..CLAIM_FIELDS.len(), at in 0usize..32, mask in 1u8..=255,
        height in 0usize..32, other_index in 0u64..MAX_LEAVES,
    ) {
        let n = verify_claim(&case.root, &case.witness, &case.chain_id, &case.vault).unwrap();
        let (mut root, mut w, mut chain_id, mut vault) = (case.root, case.witness.clone(), case.chain_id, case.vault);
        match CLAIM_FIELDS[field] {
            "root" => root[at] ^= mask,
            "depositor" => w.depositor[at % 20] ^= mask,
            "token" => w.token[at % 20] ^= mask,
            "amount" => w.amount[at] ^= mask,
            "leaf_index" => { prop_assume!(other_index != w.leaf_index); w.leaf_index = other_index }
            "directions" => w.path.directions ^= 1 << (at % 32),
            "sibling" => w.path.siblings[height][at] ^= mask,
            "s" => w.s[at] ^= mask,
            "kind" => w.kind = if w.kind == DestKind::Private { DestKind::Public } else { DestKind::Private },
            "dest" => w.dest[at] ^= mask,
            "chain_id" => chain_id[at] ^= mask,
            "vault" => vault[at % 20] ^= mask,
            _ => unreachable!(),
        }
        let got = verify_claim(&root, &w, &chain_id, &vault);
        match CLAIM_FIELDS[field] {
            "chain_id" | "vault" => prop_assert!(matches!(got, Ok(n2) if n2 != n)),
            "leaf_index" | "directions" => prop_assert_eq!(got, Err(VerifyError::DirectionsMismatch)),
            _ => prop_assert_eq!(got, Err(VerifyError::RootMismatch)),
        }
    }

    /// Two deposits with the same commitment are separate leaves: both claimable, distinct `N`.
    #[test]
    fn duplicate_commitments_get_distinct_nullifiers(s in any::<[u8; 32]>(), dest in any::<[u8; 32]>(), gap in 0usize..5, amount in any::<u128>()) {
        let (token, vault, depositor, chain_id) = (addr("token"), addr("vault"), addr("depositor"), chain(1));
        let amount = u256_from_u128(amount);
        let c = commitment(&s, DestKind::Public, &dest);
        let mut tree = Tree::new();
        let first = tree.append(deposit_leaf(0, &depositor, &token, &amount, &c)).unwrap();
        for k in 0..gap {
            tree.append(h(&format!("other {k}"))).unwrap();
        }
        let second = tree.append(deposit_leaf(tree.leaves().len() as u64, &depositor, &token, &amount, &c)).unwrap();
        let claim = |leaf_index| ClaimWitness {
            depositor, token, amount, leaf_index, path: tree.path(leaf_index).unwrap(), s, kind: DestKind::Public, dest,
        };
        let n1 = verify_claim(&tree.root(), &claim(first), &chain_id, &vault).unwrap();
        let n2 = verify_claim(&tree.root(), &claim(second), &chain_id, &vault).unwrap();
        prop_assert_ne!(n1, n2);
    }

    /// Review r2 §9: the attested statement is the root; a nullifier keyed on it would allow one
    /// mint per root. `fid` does not depend on the root, so the same deposit claimed under root n and
    /// a later root n+1 gives the same `N`, and the registry refuses the second claim.
    #[test]
    fn attack_same_deposit_under_two_roots_gives_same_nullifier(case in claim_case(), later in prop::collection::vec(any::<[u8; 32]>(), 1..20)) {
        let w = &case.witness;
        let leaf = deposit_leaf(w.leaf_index, &w.depositor, &w.token, &w.amount, &commitment(&w.s, w.kind, &w.dest));
        let mut leaves: Vec<_> = (0..=w.leaf_index).map(|i| if i == w.leaf_index { leaf } else { h(&format!("x{i}")) }).collect();
        let root_n = Tree::from_leaves(leaves.clone()).unwrap();
        leaves.extend(later);
        let root_n1 = Tree::from_leaves(leaves).unwrap();
        prop_assert_ne!(root_n.root(), root_n1.root());
        let claim_under = |t: &Tree| verify_claim(&t.root(), &ClaimWitness { path: t.path(w.leaf_index).unwrap(), ..w.clone() }, &case.chain_id, &case.vault);
        let (n_a, n_b) = (claim_under(&root_n).unwrap(), claim_under(&root_n1).unwrap());
        prop_assert_eq!(n_a, n_b);
        let mut registry: HashSet<NullifierKey> = HashSet::new();
        prop_assert_eq!(register_claim(&mut registry, &case.chain_id, &case.vault, &n_a), Ok(()));
        prop_assert_eq!(register_claim(&mut registry, &case.chain_id, &case.vault, &n_b), Err(AlreadyClaimed));
    }

    /// Review r2 §7: a commitment made for a private destination cannot be opened as a public one
    /// (nor the other way round), even with the same 32-byte destination id.
    #[test]
    fn attack_private_kind_commitment_cannot_be_opened_as_public(case in claim_case()) {
        let mut w = case.witness.clone();
        prop_assert!(verify_claim(&case.root, &w, &case.chain_id, &case.vault).is_ok());
        w.kind = match w.kind { DestKind::Private => DestKind::Public, DestKind::Public => DestKind::Private };
        prop_assert_eq!(verify_claim(&case.root, &w, &case.chain_id, &case.vault), Err(VerifyError::RootMismatch));
        prop_assert_ne!(commitment(&w.s, DestKind::Private, &w.dest), commitment(&w.s, DestKind::Public, &w.dest));
    }

    /// Every derivation changes when any one of its inputs changes.
    #[test]
    fn derivations_bind_every_input(seed in any::<[u8; 32]>(), chain_id in any::<[u8; 32]>(), vault in any::<[u8; 20]>(),
                                    depositor in any::<[u8; 20]>(), j in any::<u64>(), dest in any::<[u8; 32]>(),
                                    leaf_index in any::<u64>(), token in any::<[u8; 20]>(), amount in any::<[u8; 32]>(),
                                    at in 0usize..32, mask in 1u8..=255) {
        let (k_auth, _) = derive_keys(&seed);
        let auth = BridgeKey::new(&k_auth);
        let mut seed2 = seed; seed2[at] ^= mask;
        let auth2 = BridgeKey::new(&derive_keys(&seed2).0);
        let mut chain2 = chain_id; chain2[at] ^= mask;
        let mut vault2 = vault; vault2[at % 20] ^= mask;
        let mut depositor2 = depositor; depositor2[at % 20] ^= mask;
        let bump = u64::from(mask) << (8 * (at % 8));

        let s = deposit_secret(&auth, &chain_id, &vault, &depositor, j);
        prop_assert_ne!(s, deposit_secret(&auth2, &chain_id, &vault, &depositor, j));
        prop_assert_ne!(s, deposit_secret(&auth, &chain2, &vault, &depositor, j));
        prop_assert_ne!(s, deposit_secret(&auth, &chain_id, &vault2, &depositor, j));
        prop_assert_ne!(s, deposit_secret(&auth, &chain_id, &vault, &depositor2, j));
        prop_assert_ne!(s, deposit_secret(&auth, &chain_id, &vault, &depositor, j ^ bump));

        let c = commitment(&s, DestKind::Private, &dest);
        let mut s2 = s; s2[at] ^= mask;
        let mut dest2 = dest; dest2[at] ^= mask;
        prop_assert_ne!(c, commitment(&s2, DestKind::Private, &dest));
        prop_assert_ne!(c, commitment(&s, DestKind::Public, &dest));
        prop_assert_ne!(c, commitment(&s, DestKind::Private, &dest2));

        let leaf = deposit_leaf(leaf_index, &depositor, &token, &amount, &c);
        let mut token2 = token; token2[at % 20] ^= mask;
        let mut amount2 = amount; amount2[at] ^= mask;
        let mut c2 = c; c2[at] ^= mask;
        prop_assert_ne!(leaf, deposit_leaf(leaf_index ^ bump, &depositor, &token, &amount, &c));
        prop_assert_ne!(leaf, deposit_leaf(leaf_index, &depositor2, &token, &amount, &c));
        prop_assert_ne!(leaf, deposit_leaf(leaf_index, &depositor, &token2, &amount, &c));
        prop_assert_ne!(leaf, deposit_leaf(leaf_index, &depositor, &token, &amount2, &c));
        prop_assert_ne!(leaf, deposit_leaf(leaf_index, &depositor, &token, &amount, &c2));

        let fid = deposit_id(&chain_id, &vault, leaf_index, &leaf);
        let mut leaf2 = leaf; leaf2[at] ^= mask;
        prop_assert_ne!(fid, deposit_id(&chain2, &vault, leaf_index, &leaf));
        prop_assert_ne!(fid, deposit_id(&chain_id, &vault2, leaf_index, &leaf));
        prop_assert_ne!(fid, deposit_id(&chain_id, &vault, leaf_index ^ bump, &leaf));
        prop_assert_ne!(fid, deposit_id(&chain_id, &vault, leaf_index, &leaf2));

        let n = nullifier(&s, &fid);
        let mut fid2 = fid; fid2[at] ^= mask;
        prop_assert_ne!(n, nullifier(&s2, &fid));
        prop_assert_ne!(n, nullifier(&s, &fid2));
    }
}

/// The registry is one namespace keyed by (chain, vault, N): the same N at another vault or chain is
/// a different account.
#[test]
fn nullifier_registry_is_keyed_by_chain_vault_and_n() {
    let mut registry: HashSet<NullifierKey> = HashSet::new();
    let n = h("N");
    assert_eq!(register_claim(&mut registry, &chain(1), &addr("vault A"), &n), Ok(()));
    assert_eq!(register_claim(&mut registry, &chain(1), &addr("vault A"), &n), Err(AlreadyClaimed));
    assert_eq!(register_claim(&mut registry, &chain(1), &addr("vault B"), &n), Ok(()));
    assert_eq!(register_claim(&mut registry, &chain(2), &addr("vault A"), &n), Ok(()));
}

#[test]
fn recover_finds_exactly_the_own_deposits_inside_the_window_among_100k() {
    let k = keys("our bridge seed");
    let (chain_id, vault) = (chain(1), addr("vault"));
    let (deposits, planted) = synthetic_deposits(&k.auth, &chain_id, &vault, 100_000);
    assert_eq!(deposits.len(), 100_000);
    assert_eq!(planted.len(), 8);

    let found = recover(&k.auth, &chain_id, &vault, &deposits, 5, dest_of);
    let want: Vec<(u64, DestKind, u32)> =
        planted.iter().filter(|p| p.account < 5).map(|p| (p.leaf_index, p.kind, p.account)).collect();
    let got: Vec<(u64, DestKind, u32)> =
        found.iter().map(|o| (o.deposit.leaf_index, o.kind, o.account_index)).collect();
    assert_eq!(got, want, "the 7 inside the gap-5 window, both kinds");
    // A wider window finds the eighth (account 7).
    assert_eq!(recover(&k.auth, &chain_id, &vault, &deposits, 8, dest_of).len(), 8);

    // Each recovered deposit claims with the recovered nullifier.
    let tree = Tree::from_leaves(
        deposits.iter().map(|d| deposit_leaf(d.leaf_index, &d.depositor, &d.token, &d.amount, &d.commitment)).collect(),
    )
    .unwrap();
    let root = tree.root();
    for o in &found {
        let d = &o.deposit;
        let w = ClaimWitness {
            depositor: d.depositor,
            token: d.token,
            amount: d.amount,
            leaf_index: d.leaf_index,
            path: tree.path(d.leaf_index).unwrap(),
            s: o.s,
            kind: o.kind,
            dest: o.dest,
        };
        assert_eq!(verify_claim(&root, &w, &chain_id, &vault), Ok(o.nullifier));
    }
    // Another seed recognises none; nor does the view key used as if it were the auth key.
    assert!(recover(&keys("someone else").auth, &chain_id, &vault, &deposits, 5, dest_of).is_empty());
    assert!(recover(&k.view, &chain_id, &vault, &deposits, 5, dest_of).is_empty());
}
