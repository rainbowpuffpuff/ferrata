//! The outbound findings of the security review of 11 November 2026 (`docs/reviews/security`).
//!
//! **M-3.** A checkpoint finalised over a LEZ branch that later loses can be revoked after releases
//! were made against it; used nullifiers stay used. On the canonical branch the same burn lands at
//! another index. D13's `NOut = H("ferrata/nf-out/v2" ‖ sOut)` does not depend on the index, so the
//! second release proves the nullifier the vault already holds and is refused; SPEC-v2's
//! index-dependent `NOut` let the same burn be released twice.
//!
//! **I-4.** D12's MAC covers the memo (`ferrata/burn-mac/v2`), so a `kView`-only viewer drops a copied
//! note whose memo (the encrypted recipient) was changed.

use std::collections::HashSet;

use ferrata_bridge_core::encoding::u256_from_u128;
use ferrata_bridge_core::outbound::{
    burn_leaf, burn_secret, recover_burns, release_journal, verify_release, view_burns, BurnNote, PublishedBurn,
    ReleasePublic, ReleaseTarget, ReleaseWitness,
};
use ferrata_bridge_core::tree::Tree;
use ferrata_bridge_core::{chain_id_u256, sha256, BridgeKeys, Hash};

const CID: Hash = [0xc1; 32];

fn target() -> ReleaseTarget {
    ReleaseTarget { chain_id: chain_id_u256(11_155_111), vault: [0x0a; 20], recipient: [0xe1; 20] }
}

/// The burn tree of one branch: `before` other burns, then ours at index `before`.
fn branch(note: &BurnNote, before: u64, tag: u8) -> Tree {
    let mut tree = Tree::new();
    for i in 0..before {
        let other = burn_leaf(&CID, i, &note.token, &note.amount, &[tag ^ i as u8; 32], &[tag; 32]);
        tree.append(other).unwrap();
    }
    tree.append(burn_leaf(&CID, before, &note.token, &note.amount, &note.c_out, &note.r)).unwrap();
    tree
}

#[test]
fn m3_a_reindexed_burn_is_released_once() {
    let keys = BridgeKeys::from_bridge_seed(&[0x33; 32]);
    let t = target();
    let (token, amount) = ([0u8; 20], u256_from_u128(1_000_000_000_000_000_000));
    let note = BurnNote::create(&keys, &CID, 0, &t, token, amount);
    let s_out = burn_secret(&keys.auth, &CID, &note.r);
    let public = ReleasePublic {
        chain_id: t.chain_id,
        vault: t.vault,
        config_id: CID,
        token,
        amount,
        recipient: t.recipient,
        relayer: [0; 20],
        relay_fee: [0; 32],
    };
    // The revoked checkpoint's branch collected the burn at index 1; the canonical one at index 3.
    let release = |tree: &Tree, k: u64| {
        let w = ReleaseWitness { burn_index: k, path: tree.path(k).unwrap(), s_out, r: note.r };
        let nullifier = verify_release(&tree.root(), &w, &public).expect("a valid release under this root");
        (nullifier, release_journal(&tree.root(), &nullifier, &public))
    };
    let (revoked, canonical) = (branch(&note, 1, 0x71), branch(&note, 3, 0x72));
    assert_ne!(revoked.root(), canonical.root());
    let (n1, j1) = release(&revoked, 1);
    let (n2, j2) = release(&canonical, 3);
    assert_eq!(n1, sha256(&[&b"ferrata/nf-out/v2"[..], &s_out].concat()), "D13's NOut");
    assert_eq!(n1, n2, "one burn, one nullifier, whatever index a checkpoint collected it at");
    assert_ne!(j1, j2, "the journals differ in their burn roots only");
    // The vault keys replay protection on NOut alone (FerrataVault.sol `nullifierUsed`), and a
    // revocation leaves used nullifiers used: the second release is refused.
    let mut used = HashSet::new();
    assert!(used.insert(n1), "the release against the revoked checkpoint");
    assert!(!used.insert(n2), "the release against the canonical checkpoint is a replay");
    // The wallet recovers the same NOut on both branches.
    for (index, nullifier) in [(1u64, n1), (3u64, n2)] {
        let notes = [PublishedBurn { burn_index: index, note }];
        let scan = recover_burns(&keys, &CID, &t.chain_id, &t.vault, &notes, 4);
        assert_eq!(scan.owned.len(), 1);
        assert_eq!(scan.owned[0].nullifier, nullifier, "the wallet's NOut is the guest's");
    }
}

#[test]
fn i4_the_mac_covers_the_memo() {
    let keys = BridgeKeys::from_bridge_seed(&[0x34; 32]);
    let note = BurnNote::create(&keys, &CID, 0, &target(), [0u8; 20], u256_from_u128(1_000));
    let message = [&b"ferrata/burn-mac/v2"[..], &note.r, &note.memo, &note.token, &note.amount, &note.c_out].concat();
    assert_eq!(note.mac[..], keys.view.prf(&message)[..16], "D12's MAC");
    assert!(note.mac_verifies(&keys.view));
    // A copied note whose memo names another recipient.
    let copied = BurnNote { memo: [note.memo[0] ^ 0xff; 20], ..note };
    assert!(!copied.mac_verifies(&keys.view), "the copied note's MAC does not verify");
    let notes = [PublishedBurn { burn_index: 0, note }, PublishedBurn { burn_index: 1, note: copied }];
    let (viewed, _) = view_burns(&keys.view, &CID, &notes, 4);
    assert_eq!(viewed.len(), 1, "a kView-only viewer sees the burn once, with its own memo");
    assert_eq!(viewed[0].recipient, target().recipient);
}
