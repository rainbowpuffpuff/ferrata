//! SSZ containers of the light client from arbitrary bytes, for a fuzzer-chosen fork and
//! committee size: beacon and execution headers, light-client headers, sync committees and
//! aggregates, bootstraps and updates.
//!
//! * No decoder panics, and neither does anything run on what it accepts: hash-tree roots,
//!   `is_valid_light_client_header`, `get_lc_execution_root`, the fork upgrades, the update
//!   predicates.
//! * Where ferrata has an encoder, decode-then-encode returns the input (SSZ has one encoding
//!   per value, so a decoder that accepts anything else is not strict).
//! * The compact witness forms of a decoded bootstrap or update round-trip.
#![no_main]
use ferrata_core::{
    fork::{ChainSpec, Preset},
    types::{
        BeaconBlockHeader, ExecutionPayloadHeader, LightClientBootstrap, LightClientHeader, LightClientUpdate,
        SyncAggregate, SyncCommittee,
    },
    upgrade::{upgrade_bootstrap, upgrade_header, upgrade_update},
    validate::{get_lc_execution_root, is_finality_update, is_sync_committee_update, is_valid_light_client_header},
    witness::{
        encode_beacon_header, encode_light_client_header, encode_payload_header, encode_sync_aggregate,
        CompactBootstrap, CompactUpdate, HeaderRecord,
    },
};
use ferrata_fuzz::{committee_size, lc_fork, Input};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut i = Input(data);
    let fork = lc_fork(i.u8());
    let n = committee_size(i.u8());
    let b = i.rest();
    let preset = if n == 32 { Preset::MINIMAL } else { Preset::MAINNET };
    // Every fork at epoch 0, so every upgrade path is open.
    let spec = ChainSpec { preset, versions: [[0, 0, 0, 1]; 9], epochs: [0; 9] };

    if let Ok(h) = BeaconBlockHeader::decode(b) {
        assert_eq!(&encode_beacon_header(&h)[..], b);
        let _ = h.hash_tree_root();
    }
    for deneb in [false, true] {
        if let Ok(p) = ExecutionPayloadHeader::decode(b, deneb) {
            assert_eq!(encode_payload_header(&p, deneb), b);
            let _ = (p.hash_tree_root_capella(), p.hash_tree_root_deneb(), p.block_hash_proof(deneb));
        }
    }
    if let Ok(a) = SyncAggregate::decode(b, n) {
        assert_eq!(encode_sync_aggregate(&a), b);
        assert!(a.participants() <= n as u64);
    }
    if let Ok(c) = SyncCommittee::decode(b, n) {
        let _ = c.hash_tree_root();
    }
    if let Ok(h) = LightClientHeader::decode(b, fork) {
        assert_eq!(encode_light_client_header(&h, fork), b);
        assert!(h.has_layout(fork));
        let _ = (is_valid_light_client_header(&h, fork, &spec), get_lc_execution_root(&h, fork, &spec));
        let _ = HeaderRecord::of(&h, fork, &spec).encode();
        let mut to = fork;
        while let Some(next) = to.next() {
            if let Ok(u) = upgrade_header(h.clone(), fork, next, &spec) {
                let _ = is_valid_light_client_header(&u, next, &spec);
            }
            to = next;
        }
    }
    if let Ok(bs) = LightClientBootstrap::decode(b, fork, n) {
        let c = CompactBootstrap::of(&bs);
        assert_eq!(CompactBootstrap::decode(&c.encode(fork), fork).as_ref(), Ok(&c));
        if let Some(next) = fork.next() {
            let _ = upgrade_bootstrap(bs, fork, next, &spec);
        }
    }
    if let Ok(u) = LightClientUpdate::decode(b, fork, n) {
        let _ = (is_sync_committee_update(&u), is_finality_update(&u), u.sync_aggregate.participants());
        let c = CompactUpdate::of(&u);
        let enc = c.encode(fork);
        let _ = CompactUpdate::layout_of(&enc);
        assert_eq!(CompactUpdate::decode(&enc, fork, n).as_ref(), Ok(&c));
        if let Some(next) = fork.next() {
            let _ = upgrade_update(u, fork, next, &spec);
        }
    }
});
