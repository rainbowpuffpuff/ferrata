//! LEZ checkpoint encodings (SPEC-v2 §4.1, §4.2): `S`, `keySetHash`, `burnFrontierHash`, `t(n)`.
//! (The journal's layout is in tests/encoding.rs.)

mod common;

use common::h;
use ferrata_bridge_core::checkpoint::{burn_frontier_hash, key_set_hash, threshold, LezState};
use ferrata_bridge_core::encoding::{burn_frontier_preimage, lez_state_preimage};
use ferrata_bridge_core::sha256;
use ferrata_bridge_core::tree::IncrementalTree;
use proptest::prelude::*;

#[test]
fn threshold_matches_the_formula_table() {
    // t(n) = max(1, min(ceil(2n/3), n - 1))
    let want = [(1, 1), (2, 1), (3, 2), (4, 3), (5, 4), (6, 4), (7, 5), (8, 6), (9, 6), (10, 7), (100, 67)];
    for (n, t) in want {
        assert_eq!(threshold(n), t, "t({n})");
    }
    assert_eq!(threshold(0), 1);
    assert_eq!(threshold(u32::MAX), 2_863_311_530); // ceil(2(2^32-1)/3), no overflow
}

#[test]
fn state_hash_is_sha256_of_the_preimage() {
    let s = LezState {
        tip_id: 1000,
        tip_hash: h("tip"),
        key_set_hash: key_set_hash(&[h("k1"), h("k2")]),
        config_tip_hash: h("cfg"),
        burn_count: 3,
        burn_frontier_hash: h("f"),
        imported_leaf_count: 5,
        sunset: false,
    };
    assert_eq!(s.hash(), sha256(&lez_state_preimage(&s)));
    assert_ne!(s.hash(), LezState { sunset: true, ..s }.hash());
}

#[test]
fn key_order_and_count_matter() {
    let (a, b) = (h("a"), h("b"));
    assert_ne!(key_set_hash(&[a, b]), key_set_hash(&[b, a]));
    assert_ne!(key_set_hash(&[a]), key_set_hash(&[a, a]));
    assert_ne!(key_set_hash(&[]), key_set_hash(&[[0u8; 32]]));
}

proptest! {
    /// The frontier hash is a function of the leaves: the same leaves give the same hash whether the
    /// tree was built in one go or resumed from a frontier; one more leaf changes it.
    #[test]
    fn frontier_hash_depends_on_the_leaves_only(leaves in prop::collection::vec(any::<[u8; 32]>(), 0..60), split in any::<prop::sample::Index>(), extra in any::<[u8; 32]>()) {
        let k = split.index(leaves.len() + 1);
        let mut whole = IncrementalTree::new();
        for leaf in &leaves {
            whole.append(*leaf).unwrap();
        }
        let mut first = IncrementalTree::new();
        for leaf in &leaves[..k] {
            first.append(*leaf).unwrap();
        }
        let mut resumed = IncrementalTree::from_frontier(k as u64, &first.frontier()).unwrap();
        for leaf in &leaves[k..] {
            resumed.append(*leaf).unwrap();
        }
        prop_assert_eq!(burn_frontier_hash(&whole), burn_frontier_hash(&resumed));
        prop_assert_eq!(burn_frontier_hash(&whole), sha256(&burn_frontier_preimage(&whole.frontier())));
        let before = burn_frontier_hash(&whole);
        whole.append(extra).unwrap();
        prop_assert_ne!(burn_frontier_hash(&whole), before);
    }

    #[test]
    fn every_state_field_changes_s(tip_id in any::<u64>(), burn_count in any::<u64>(), imported in any::<u64>(), sunset in any::<bool>(), at in 0usize..32, mask in 1u8..=255) {
        let s = LezState {
            tip_id, tip_hash: h("tip"), key_set_hash: h("ks"), config_tip_hash: h("cfg"), burn_count,
            burn_frontier_hash: h("f"), imported_leaf_count: imported, sunset,
        };
        let base = s.hash();
        let bump = u64::from(mask) << (8 * (at % 8));
        let mut tip_hash = s.tip_hash; tip_hash[at] ^= mask;
        let mut ks = s.key_set_hash; ks[at] ^= mask;
        let mut cfg = s.config_tip_hash; cfg[at] ^= mask;
        let mut f = s.burn_frontier_hash; f[at] ^= mask;
        for changed in [
            LezState { tip_id: tip_id ^ bump, ..s }, LezState { tip_hash, ..s }, LezState { key_set_hash: ks, ..s },
            LezState { config_tip_hash: cfg, ..s }, LezState { burn_count: burn_count ^ bump, ..s },
            LezState { burn_frontier_hash: f, ..s }, LezState { imported_leaf_count: imported ^ bump, ..s },
            LezState { sunset: !sunset, ..s },
        ] {
            prop_assert_ne!(changed.hash(), base);
        }
    }
}
