//! Deposit -> mint: the claim statement, its nullifier, and wallet recovery.

mod common;

use common::{addr, claim_case, dest_for, h, synthetic_deposits};
use ferrata_bridge_core::encoding::u256_from_u128;
use ferrata_bridge_core::inbound::{
    commitment, deposit_id, deposit_leaf, deposit_secret, nullifier, recover, verify_claim, ClaimWitness,
};
use ferrata_bridge_core::tree::{Tree, MAX_LEAVES};
use ferrata_bridge_core::{BridgeKey, VerifyError};
use proptest::prelude::*;

/// The fields of a claim, in the order the per-field tests mutate them.
const CLAIM_FIELDS: [&str; 9] = ["root", "token", "amount", "leaf_index", "path", "s", "dest", "chain_id", "vault"];

proptest! {
    #[test]
    fn claim_verifies(case in claim_case()) {
        let w = &case.witness;
        let n = verify_claim(&case.root, w, case.chain_id, &case.vault).unwrap();
        let leaf = deposit_leaf(w.leaf_index, &w.token, &w.amount, &commitment(&w.s, &w.dest));
        prop_assert_eq!(n, nullifier(&w.s, &deposit_id(case.chain_id, &case.vault, w.leaf_index, &leaf)));
    }

    /// Changing any one input either fails verification (everything that feeds the leaf or its
    /// path) or changes the nullifier (chain id and vault, which feed only `fid`; they must
    /// therefore come from the program's configuration, never from the claimant).
    #[test]
    fn every_single_field_change_fails_or_changes_the_nullifier(
        case in claim_case(), field in 0usize..CLAIM_FIELDS.len(), at in 0usize..32, mask in 1u8..=255,
        height in 0usize..32, other_index in 0u64..MAX_LEAVES,
    ) {
        let n = verify_claim(&case.root, &case.witness, case.chain_id, &case.vault).unwrap();
        let (mut root, mut w, mut chain_id, mut vault) = (case.root, case.witness.clone(), case.chain_id, case.vault);
        match CLAIM_FIELDS[field] {
            "root" => root[at] ^= mask,
            "token" => w.token[at % 20] ^= mask,
            "amount" => w.amount[at] ^= mask,
            "leaf_index" => { prop_assume!(other_index != w.leaf_index); w.leaf_index = other_index }
            "path" => w.path[height][at] ^= mask,
            "s" => w.s[at] ^= mask,
            "dest" => w.dest[at] ^= mask,
            "chain_id" => chain_id ^= u64::from(mask) << (8 * (at % 8)),
            "vault" => vault[at % 20] ^= mask,
            _ => unreachable!(),
        }
        let got = verify_claim(&root, &w, chain_id, &vault);
        match CLAIM_FIELDS[field] {
            "chain_id" | "vault" => prop_assert!(matches!(got, Ok(n2) if n2 != n)),
            _ => prop_assert_eq!(got, Err(VerifyError::RootMismatch)),
        }
    }

    /// Two deposits with the same commitment (a wallet reusing `C`, or anyone copying it) are
    /// separate leaves: both claimable, with different nullifiers.
    #[test]
    fn duplicate_commitments_get_distinct_nullifiers(s in any::<[u8; 32]>(), dest in any::<[u8; 32]>(),
                                                     gap in 0usize..5, amount in any::<u128>()) {
        let (token, vault, chain_id) = (addr("token"), addr("vault"), 1u64);
        let amount = u256_from_u128(amount);
        let c = commitment(&s, &dest);
        let mut tree = Tree::new();
        let first = tree.append(deposit_leaf(0, &token, &amount, &c)).unwrap();
        for k in 0..gap {
            tree.append(h(&format!("other {k}"))).unwrap();
        }
        let second = tree.append(deposit_leaf(tree.leaves().len() as u64, &token, &amount, &c)).unwrap();
        let claim = |leaf_index| ClaimWitness { token, amount, leaf_index, path: tree.path(leaf_index).unwrap(), s, dest };
        let n1 = verify_claim(&tree.root(), &claim(first), chain_id, &vault).unwrap();
        let n2 = verify_claim(&tree.root(), &claim(second), chain_id, &vault).unwrap();
        prop_assert_ne!(n1, n2);
    }

    /// The nullifier depends on the deposit, not on the root it is proven under: claiming the
    /// same deposit again under a later root yields the same `N` (so the program rejects it).
    #[test]
    fn same_deposit_same_nullifier_under_every_later_root(case in claim_case(), later in prop::collection::vec(any::<[u8; 32]>(), 1..20)) {
        let w = &case.witness;
        let leaf = deposit_leaf(w.leaf_index, &w.token, &w.amount, &commitment(&w.s, &w.dest));
        let mut leaves: Vec<_> = (0..=w.leaf_index).map(|i| if i == w.leaf_index { leaf } else { h(&format!("x{i}")) }).collect();
        let early = Tree::from_leaves(leaves.clone()).unwrap();
        leaves.extend(later);
        let late = Tree::from_leaves(leaves).unwrap();
        let at = |t: &Tree| verify_claim(&t.root(), &ClaimWitness { path: t.path(w.leaf_index).unwrap(), ..w.clone() }, case.chain_id, &case.vault);
        prop_assert_ne!(early.root(), late.root());
        prop_assert_eq!(at(&early).unwrap(), at(&late).unwrap());
    }

    /// Every derivation changes when any one of its inputs changes.
    #[test]
    fn derivations_bind_every_input(k in any::<[u8; 32]>(), chain_id in any::<u64>(), vault in any::<[u8; 20]>(),
                                    depositor in any::<[u8; 20]>(), j in any::<u64>(), dest in any::<[u8; 32]>(),
                                    leaf_index in any::<u64>(), token in any::<[u8; 20]>(), amount in any::<[u8; 32]>(),
                                    at in 0usize..32, mask in 1u8..=255) {
        let key = BridgeKey::new(&k);
        let s = deposit_secret(&key, chain_id, &vault, &depositor, j);
        let mut k2 = k; k2[at] ^= mask;
        let mut vault2 = vault; vault2[at % 20] ^= mask;
        let mut depositor2 = depositor; depositor2[at % 20] ^= mask;
        let bump = u64::from(mask) << (8 * (at % 8));
        prop_assert_ne!(s, deposit_secret(&BridgeKey::new(&k2), chain_id, &vault, &depositor, j));
        prop_assert_ne!(s, deposit_secret(&key, chain_id ^ bump, &vault, &depositor, j));
        prop_assert_ne!(s, deposit_secret(&key, chain_id, &vault2, &depositor, j));
        prop_assert_ne!(s, deposit_secret(&key, chain_id, &vault, &depositor2, j));
        prop_assert_ne!(s, deposit_secret(&key, chain_id, &vault, &depositor, j ^ bump));

        let c = commitment(&s, &dest);
        let mut s2 = s; s2[at] ^= mask;
        let mut dest2 = dest; dest2[at] ^= mask;
        prop_assert_ne!(c, commitment(&s2, &dest));
        prop_assert_ne!(c, commitment(&s, &dest2));

        let leaf = deposit_leaf(leaf_index, &token, &amount, &c);
        let mut token2 = token; token2[at % 20] ^= mask;
        let mut amount2 = amount; amount2[at] ^= mask;
        let mut c2 = c; c2[at] ^= mask;
        prop_assert_ne!(leaf, deposit_leaf(leaf_index ^ bump, &token, &amount, &c));
        prop_assert_ne!(leaf, deposit_leaf(leaf_index, &token2, &amount, &c));
        prop_assert_ne!(leaf, deposit_leaf(leaf_index, &token, &amount2, &c));
        prop_assert_ne!(leaf, deposit_leaf(leaf_index, &token, &amount, &c2));

        let fid = deposit_id(chain_id, &vault, leaf_index, &leaf);
        let mut leaf2 = leaf; leaf2[at] ^= mask;
        prop_assert_ne!(fid, deposit_id(chain_id ^ bump, &vault, leaf_index, &leaf));
        prop_assert_ne!(fid, deposit_id(chain_id, &vault2, leaf_index, &leaf));
        prop_assert_ne!(fid, deposit_id(chain_id, &vault, leaf_index ^ bump, &leaf));
        prop_assert_ne!(fid, deposit_id(chain_id, &vault, leaf_index, &leaf2));

        let n = nullifier(&s, &fid);
        let mut fid2 = fid; fid2[at] ^= mask;
        prop_assert_ne!(n, nullifier(&s2, &fid));
        prop_assert_ne!(n, nullifier(&s, &fid2));
    }
}

#[test]
fn recover_finds_exactly_the_seven_own_deposits_among_100k() {
    let key = BridgeKey::new(&h("our bridge key"));
    let (chain_id, vault) = (1u64, addr("vault"));
    let (deposits, ours) = synthetic_deposits(&key, chain_id, &vault);
    assert_eq!(deposits.len(), 100_000);

    let found = recover(&key, chain_id, &vault, &deposits, dest_for);
    let found_at: Vec<usize> = found.iter().map(|o| o.deposit.leaf_index as usize).collect();
    assert_eq!(found_at, ours);

    // Each recovered deposit claims (publicly, witness in the clear) with the recovered nullifier.
    let tree = Tree::from_leaves(
        deposits.iter().map(|d| deposit_leaf(d.leaf_index, &d.token, &d.amount, &d.commitment)).collect(),
    )
    .unwrap();
    let root = tree.root();
    for o in &found {
        let d = &o.deposit;
        let w = ClaimWitness {
            token: d.token,
            amount: d.amount,
            leaf_index: d.leaf_index,
            path: tree.path(d.leaf_index).unwrap(),
            s: o.s,
            dest: o.dest,
        };
        assert_eq!(verify_claim(&root, &w, chain_id, &vault), Ok(o.nullifier));
    }

    // Another key recognises none of them.
    assert!(recover(&BridgeKey::new(&h("someone else")), chain_id, &vault, &deposits, dest_for).is_empty());
}
