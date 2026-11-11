//! Light-client store initialization and update processing on attacker-chosen bootstraps and
//! updates, under a fuzzer-chosen chain spec (preset, fork versions and epochs), forks and
//! current slot: the spec store (`LightClientStore`: process_update, process_force_update,
//! upgrade) and Ferrata's policy store (`FinalizedStore`), with the blst backend.
//!
//! Properties: nothing panics; neither store applies an update the spec's validation refuses;
//! an applied update never moves the finalized slot backwards.
#![no_main]
use ferrata_core::{
    bls::Blst,
    policy::FinalizedStore,
    store::LightClientStore,
    types::{LightClientBootstrap, LightClientUpdate},
    upgrade::{upgrade_bootstrap, upgrade_update},
    validate::{validate_light_client_update, Context, SignaturePath},
};
use ferrata_fuzz::LcInput;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let x = LcInput::parse(data);
    let spec = &x.spec;
    let n = spec.preset.sync_committee_size;
    let Ok(mut bootstrap) = LightClientBootstrap::decode(x.bootstrap, x.bootstrap_fork, n) else { return };
    if x.bootstrap_fork != x.store_fork {
        let Ok(b) = upgrade_bootstrap(bootstrap, x.bootstrap_fork, x.store_fork, spec) else { return };
        bootstrap = b;
    }
    let trusted = bootstrap.header.beacon.hash_tree_root();
    let Ok(mut store) = LightClientStore::initialize(&trusted, &bootstrap, x.store_fork, spec) else { return };
    let bls = Blst::default();
    let ctx = Context { spec, genesis_validators_root: x.genesis_validators_root, bls: &bls };
    let Ok(mut update) = LightClientUpdate::decode(x.update, x.update_fork, n) else {
        let _ = store.process_force_update(x.current_slot, spec);
        return;
    };
    if x.update_fork != x.store_fork {
        let Ok(u) = upgrade_update(update, x.update_fork, x.store_fork, spec) else { return };
        update = u;
    }
    let valid = validate_light_client_update(&store.core, &update, x.current_slot, &ctx, SignaturePath::Spec).is_ok();
    let before = store.core.finalized_header.beacon.slot;
    let processed = store.process_update(&update, x.current_slot, &ctx);
    // Application may still refuse a valid update (the spec's assert in
    // apply_light_client_update), but nothing invalid is ever applied.
    if processed.is_ok() {
        assert!(valid, "process_update accepted an update validate refuses");
        assert!(store.core.finalized_header.beacon.slot >= before, "finalized slot moved back");
    }
    let _ = store.process_force_update(x.current_slot, spec);
    if let Some(next) = x.store_fork.next() {
        let _ = store.upgrade(next, spec);
    }

    if let Ok(mut policy) = FinalizedStore::initialize(&trusted, &bootstrap, x.store_fork, spec, &bls) {
        let finalized = policy.core().finalized_header.beacon.slot;
        if policy.process_update(&update, x.current_slot, &ctx).is_ok() {
            assert!(valid, "the policy store accepted an update the spec refuses");
            assert!(policy.core().finalized_header.beacon.slot >= finalized);
        }
    }
});
