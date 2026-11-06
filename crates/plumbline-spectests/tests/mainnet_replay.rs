//! Real mainnet data: the period-1899 bootstrap and the best update of each of periods 1899 to
//! 1903 (Fulu, 512-member committees), verified with both BLS libraries through the policy store
//! and the spec store. The expected finalized roots are the canonical block roots reported by a
//! second, independent provider (see `fixtures/mainnet-p1899/SOURCE.md`).

use std::{fs, path::PathBuf, time::Instant};

use plumbline_core::{
    bls::{Aggregation, BlsBackend, Blst, Zkcrypto},
    fork::{ChainSpec, Fork, LcFork, Preset, FAR_FUTURE_EPOCH},
    policy::{Adoption, FinalizedStore},
    store::LightClientStore,
    types::{LightClientBootstrap, LightClientUpdate},
    validate::Context,
    Error, Root,
};
use plumbline_spectests::CaseConfig;
use sha2::{Digest, Sha256};

/// Mainnet fork schedule, from `configs/mainnet.yaml` at consensus-specs v1.7.0-beta.4.
fn mainnet() -> ChainSpec {
    ChainSpec {
        preset: Preset::MAINNET,
        versions: [
            [0, 0, 0, 0],
            [1, 0, 0, 0],
            [2, 0, 0, 0],
            [3, 0, 0, 0],
            [4, 0, 0, 0],
            [5, 0, 0, 0],
            [6, 0, 0, 0],
            [7, 0, 0, 0],
            [8, 0, 0, 0],
        ],
        epochs: [0, 74_240, 144_896, 194_048, 269_568, 364_032, 411_392, FAR_FUTURE_EPOCH, FAR_FUTURE_EPOCH],
    }
}

fn root(hex_str: &str) -> Root {
    hex::decode(hex_str.trim_start_matches("0x")).unwrap().try_into().unwrap()
}

const GENESIS_VALIDATORS_ROOT: &str = "0x4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95";
const TRUSTED_BLOCK_ROOT: &str = "0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac";

/// Finalized (slot, block root) after each update, as a second provider reports them.
const EXPECTED_FINALIZED: [(u64, &str); 5] = [
    (15_556_608, "0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac"),
    (15_564_800, "0x96949350abfe8a04e6f1ae22b3064fdcee177dec32133348a2a69d200151e09a"),
    (15_572_992, "0x7e2771b1939afa205540003849bca0b0aa6ee71dd5a767cca2e35e084a008a4c"),
    (15_581_184, "0xe7c0ae95dd6a82d9eca7d9dc4e1b42a4c8197fbb0e4cd880bc5f9631de2b992b"),
    (15_589_599, "0x5d1200f2c031f01aa2c3385c881ca6cac727c23d3080e0ac53169160c8ddd573"),
];

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/mainnet-p1899")
}

/// Read a fixture and check it against the pinned `SHA256SUMS`.
fn fixture(name: &str) -> Vec<u8> {
    let bytes = fs::read(fixture_dir().join(name)).unwrap();
    let sums = fs::read_to_string(fixture_dir().join("SHA256SUMS")).unwrap();
    let want = sums.lines().find(|l| l.ends_with(name)).unwrap().split_whitespace().next().unwrap();
    assert_eq!(hex::encode(Sha256::digest(&bytes)), want, "{name} does not match SHA256SUMS");
    bytes
}

fn updates() -> Vec<LightClientUpdate> {
    (0..5)
        .map(|i| LightClientUpdate::decode(&fixture(&format!("update_{i}.ssz")), LcFork::Electra, 512).unwrap())
        .collect()
}

#[test]
fn update_digest_is_mainnet_fulu() {
    let config = CaseConfig {
        spec: mainnet(),
        blob_schedule: vec![(412_672, 15), (419_072, 21)],
        max_blobs_per_block_electra: 9,
    };
    assert_eq!(config.fork_for_digest(&root(GENESIS_VALIDATORS_ROOT), [0x8c, 0x9f, 0x62, 0xfe]), Ok(Fork::Fulu));
}

#[test]
fn replays_five_mainnet_periods_with_both_libraries() {
    let spec = mainnet();
    let bootstrap = LightClientBootstrap::decode(&fixture("bootstrap.ssz"), LcFork::Electra, 512).unwrap();
    let updates = updates();
    let backends: [(&str, &dyn BlsBackend); 4] = [
        ("blst/auto", &Blst::new(Aggregation::Auto)),
        ("blst/additive", &Blst::new(Aggregation::Additive)),
        ("zkcrypto/auto", &Zkcrypto::new(Aggregation::Auto)),
        ("zkcrypto/additive", &Zkcrypto::new(Aggregation::Additive)),
    ];
    for ((name, bls), adoption) in backends.into_iter().flat_map(|b| [(b, Adoption::Full), (b, Adoption::OnChain)]) {
        let trusted = root(TRUSTED_BLOCK_ROOT);
        let mut policy =
            FinalizedStore::initialize_with(&trusted, &bootstrap, LcFork::Electra, &spec, bls, adoption).unwrap();
        let mut reference = LightClientStore::initialize(&trusted, &bootstrap, LcFork::Electra, &spec).unwrap();
        let ctx = Context { spec: &spec, genesis_validators_root: root(GENESIS_VALIDATORS_ROOT), bls };
        let mut handoffs = 0;
        for (i, update) in updates.iter().enumerate() {
            let current_slot = update.signature_slot + 1;
            let started = Instant::now();
            let outcome =
                policy.process_update(update, current_slot, &ctx).unwrap_or_else(|e| panic!("{name} update {i}: {e}"));
            let elapsed = started.elapsed();
            reference.process_update(update, current_slot, &ctx).unwrap();
            assert_eq!(policy.core(), &reference.core, "{name} update {i}: policy and spec store differ");
            handoffs += usize::from(outcome.rotated);
            let (slot, block_root) = EXPECTED_FINALIZED[i];
            let finalized = &policy.core().finalized_header.beacon;
            assert_eq!(finalized.slot, slot, "{name} update {i}");
            assert_eq!(finalized.hash_tree_root(), root(block_root), "{name} update {i}");
            println!(
                "{name:<18} {adoption:?} period {} signers {}/512 finalized slot {} handoff {} ({:.1} ms incl. committee check)",
                spec.period_at_slot(update.attested_header.beacon.slot),
                update.sync_aggregate.participants(),
                slot,
                outcome.rotated,
                elapsed.as_secs_f64() * 1e3,
            );
        }
        assert_eq!(handoffs, 4, "{name}: four committee handoffs, 1899 to 1903");
        // A replayed old update is refused and changes nothing.
        let before = policy.clone();
        let replay = policy.process_update(&updates[1], updates[4].signature_slot + 1, &ctx);
        assert!(
            matches!(replay, Err(Error::PeriodSkipped | Error::UpdateIrrelevant | Error::NotNewer)),
            "{name}: {replay:?}"
        );
        assert_eq!(policy, before);
    }
}

#[test]
fn mainnet_update_with_a_flipped_signer_bit_is_refused() {
    let spec = mainnet();
    let bootstrap = LightClientBootstrap::decode(&fixture("bootstrap.ssz"), LcFork::Electra, 512).unwrap();
    let mut update = updates().remove(0);
    let i = update.sync_aggregate.bits.iter().position(|b| *b).unwrap();
    update.sync_aggregate.bits[i] = false;
    let bls = Blst::new(Aggregation::Auto);
    let mut policy =
        FinalizedStore::initialize(&root(TRUSTED_BLOCK_ROOT), &bootstrap, LcFork::Electra, &spec, &bls).unwrap();
    let ctx = Context { spec: &spec, genesis_validators_root: root(GENESIS_VALIDATORS_ROOT), bls: &bls };
    assert_eq!(policy.process_update(&update, update.signature_slot + 1, &ctx), Err(Error::SignatureInvalid));
}

#[test]
fn split_verification_round_trips_through_576_bytes() {
    // The program's seam: the Miller-loop product is encoded, stored and finished later.
    use plumbline_core::{bls::FP12_BYTES, validate::sync_committee_signing_root};
    let spec = mainnet();
    let bootstrap = LightClientBootstrap::decode(&fixture("bootstrap.ssz"), LcFork::Electra, 512).unwrap();
    let updates = updates();
    let bls = Blst::new(Aggregation::Auto);
    let ctx = Context { spec: &spec, genesis_validators_root: root(GENESIS_VALIDATORS_ROOT), bls: &bls };
    let mut store =
        FinalizedStore::initialize(&root(TRUSTED_BLOCK_ROOT), &bootstrap, LcFork::Electra, &spec, &bls).unwrap();
    store.process_update(&updates[0], updates[0].signature_slot + 1, &ctx).unwrap();
    // update_1 is signed in period 1900 by the committee the store just learned.
    let u = &updates[1];
    let committee = &store.core().next_sync_committee;
    let signing_root = sync_committee_signing_root(u, &ctx);
    let (bits, sig) = (&u.sync_aggregate.bits, &u.sync_aggregate.signature);
    let product: [u8; FP12_BYTES] = bls.miller_half(committee, bits, &signing_root, sig).unwrap();
    assert_eq!(Blst::final_half(&product), Ok(()));
    assert_eq!(bls.verify_checked(committee, bits, &signing_root, sig), Ok(()));
    let mut tampered = product;
    tampered[100] ^= 1;
    assert_eq!(Blst::final_half(&tampered), Err(Error::SignatureInvalid));
    let mut other_root = signing_root;
    other_root[0] ^= 1;
    let wrong = bls.miller_half(committee, bits, &other_root, sig).unwrap();
    assert_eq!(Blst::final_half(&wrong), Err(Error::SignatureInvalid));
}
