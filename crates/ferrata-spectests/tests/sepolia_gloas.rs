//! Real Sepolia data under Gloas (`fixtures/sepolia-gloas-2026-11-10`; its SOURCE.md lists every
//! request). Sepolia activated Gloas at epoch 353,024, the first slot of period 1379.
//!
//! The one public Sepolia node found serving light-client data (Lodestar) serves no bootstrap for a
//! Gloas-era checkpoint (SOURCE.md records the refusal), so the store starts from the bootstrap of
//! the last pre-Gloas period (1378, Electra layout) and follows the real chain: the period-1378
//! update, the store's upgrade to the Gloas layout, the update of every period from 1379 to 1409
//! (31 committee handoffs under Gloas), and the latest Gloas finality update. Policy store (both
//! committee checks) and spec store, with both BLS libraries. The finalized header binds the
//! bid's parent block hash; the record comes from that block's real RLP header and equals what two
//! execution providers report. Tampering with the real Gloas data is refused with exact codes.

use std::{fs, path::PathBuf};

use ferrata_core::{
    attest::{Binding, HeaderRecord},
    bls::{Aggregation, BlsBackend, Blst, Zkcrypto},
    exec::{decode_header, HeaderShape},
    fork::{ChainSpec, Fork, LcFork, Preset, FAR_FUTURE_EPOCH},
    keccak::keccak256,
    merkle::ZERO,
    policy::{Adoption, FinalizedStore},
    store::LightClientStore,
    types::{Execution, LightClientBootstrap, LightClientHeader, LightClientUpdate, SyncAggregate, SyncCommittee},
    validate::Context,
    Error, Root,
};
use ferrata_spectests::{execkit::header_rlp, CaseConfig};
use serde_json::Value;
use sha2::{Digest, Sha256};

const SEPOLIA: u64 = 11_155_111;
const GVR: &str = "0xd8ea171f3c94aea21ebc42a1ed61052acf3f9209c00e4efbaaddac09ed9b8078";
const BOOTSTRAP_ROOT: &str = "0x5f71d7f650f9252b48749b5f062c0c234e01afd69f364ab600e2b6051494b20d";
const FINALIZED_SLOT: u64 = 11_547_104;
const BLOCK_NUMBER: u64 = 12_103_422;
const BLOCK_HASH: &str = "0xcd9fa3661248bb2411636d3498a1dda97f0ee50d02b8ac894969e391f2978435";
const FIRST: u64 = 1378;
const LAST: u64 = 1409;

/// Sepolia's fork schedule (eth-clients/sepolia `metadata/config.yaml` at 7b1b9d9).
fn sepolia() -> ChainSpec {
    let v = |b: u8| [0x90, 0x00, 0x00, b];
    ChainSpec {
        preset: Preset::MAINNET,
        versions: [v(0x69), v(0x70), v(0x71), v(0x72), v(0x73), v(0x74), v(0x75), v(0x76), [0; 4]],
        epochs: [0, 50, 100, 56_832, 132_608, 222_464, 272_640, 353_024, FAR_FUTURE_EPOCH],
    }
}

fn root(s: &str) -> Root {
    hex::decode(s.trim_start_matches("0x")).unwrap().try_into().unwrap()
}

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/sepolia-gloas-2026-11-10")
}

/// A fixture file, checked against the pinned `SHA256SUMS`.
fn fixture(name: &str) -> Vec<u8> {
    let bytes = fs::read(dir().join(name)).unwrap();
    let sums = fs::read_to_string(dir().join("SHA256SUMS")).unwrap();
    let want = sums.lines().find(|l| l.ends_with(&format!("  {name}"))).unwrap().split_whitespace().next().unwrap();
    assert_eq!(hex::encode(Sha256::digest(&bytes)), want, "{name} does not match SHA256SUMS");
    bytes
}

fn json(name: &str) -> Value {
    serde_json::from_slice(&fixture(name)).unwrap()
}

/// The layout of a period's update: Electra before the fork, Gloas from period 1379.
fn layout(period: u64) -> LcFork {
    if period < 1379 {
        LcFork::Electra
    } else {
        LcFork::Gloas
    }
}

fn update(period: u64) -> LightClientUpdate {
    LightClientUpdate::decode(&fixture(&format!("update_{period}.ssz")), layout(period), 512).unwrap()
}

/// The Gloas `LightClientFinalityUpdate`: every field is fixed-size.
fn finality_update(b: &[u8]) -> LightClientUpdate {
    const H: usize = 112 + 32 + 11 * 32;
    assert_eq!(b.len(), 2 * H + 9 * 32 + 160 + 8);
    LightClientUpdate {
        attested_header: LightClientHeader::decode(&b[..H], LcFork::Gloas).unwrap(),
        next_sync_committee: SyncCommittee::empty(512),
        next_sync_committee_branch: vec![ZERO; LcFork::Gloas.sync_committee_branch_len()],
        finalized_header: LightClientHeader::decode(&b[H..2 * H], LcFork::Gloas).unwrap(),
        finality_branch: b[2 * H..2 * H + 288].chunks_exact(32).map(|c| c.try_into().unwrap()).collect(),
        sync_aggregate: SyncAggregate::decode(&b[2 * H + 288..2 * H + 448], 512).unwrap(),
        signature_slot: u64::from_le_bytes(b[2 * H + 448..].try_into().unwrap()),
    }
}

/// The canonical root a second beacon provider reports for `slot`.
fn canonical(slot: u64) -> Root {
    root(json(&format!("beacon_header_{slot}_second.json"))["data"]["root"].as_str().unwrap())
}

/// A policy store and a spec store, both following the real chain from the pre-Gloas bootstrap
/// into Gloas: returns them after the period-1409 update.
fn follow(bls: &dyn BlsBackend, adoption: Adoption) -> (FinalizedStore, LightClientStore) {
    let spec = sepolia();
    let ctx = Context { spec: &spec, genesis_validators_root: root(GVR), bls };
    let boot = LightClientBootstrap::decode(&fixture("bootstrap.ssz"), LcFork::Electra, 512).unwrap();
    let mut policy =
        FinalizedStore::initialize_with(&root(BOOTSTRAP_ROOT), &boot, LcFork::Electra, &spec, bls, adoption).unwrap();
    let mut reference = LightClientStore::initialize(&root(BOOTSTRAP_ROOT), &boot, LcFork::Electra, &spec).unwrap();
    let mut rotations = 0;
    for period in FIRST..=LAST {
        if period == 1379 {
            // The fork: both stores move to the Gloas layout.
            policy.upgrade(LcFork::Gloas, &spec).unwrap();
            reference.upgrade(LcFork::Gloas, &spec).unwrap();
        }
        let u = update(period);
        let outcome =
            policy.process_update(&u, u.signature_slot + 1, &ctx).unwrap_or_else(|e| panic!("period {period}: {e}"));
        reference.process_update(&u, u.signature_slot + 1, &ctx).unwrap();
        assert_eq!(policy.core(), &reference.core, "period {period}: policy and spec store differ");
        let finalized = &policy.core().finalized_header.beacon;
        assert_eq!(finalized.hash_tree_root(), canonical(finalized.slot), "period {period}");
        rotations += usize::from(outcome.rotated);
    }
    assert_eq!(rotations, 31, "a committee handoff in every Gloas period, 1379 to 1409");
    (policy, reference)
}

/// The fork digest Lodestar sent with each period's update (listed in SOURCE.md) is the one this
/// schedule computes for the update's attested slot: Fulu (with Sepolia's second blob-parameter
/// change) for period 1378, Gloas from 1379 on.
#[test]
fn every_served_fork_digest_is_the_sepolia_schedules() {
    let cfg = CaseConfig {
        spec: sepolia(),
        blob_schedule: vec![(274_176, 15), (275_712, 21)],
        max_blobs_per_block_electra: 9,
    };
    let source = fs::read_to_string(dir().join("SOURCE.md")).unwrap();
    let mut periods = Vec::new();
    for cells in source.lines().map(|l| l.split('|').map(str::trim).collect::<Vec<_>>()) {
        let Some(period) = cells.get(1).and_then(|c| c.parse::<u64>().ok()).filter(|p| (FIRST..=LAST).contains(p))
        else {
            continue;
        };
        let digest: [u8; 4] = hex::decode(cells[3].trim_start_matches("0x")).unwrap().try_into().unwrap();
        let attested = update(period).attested_header.beacon.slot;
        assert_eq!(attested.to_string(), cells[4], "period {period}");
        assert_eq!(cfg.fork_digest(&root(GVR), attested / 32), digest, "period {period}");
        let fork = cfg.fork_for_digest(&root(GVR), digest).unwrap();
        assert_eq!(fork, if period < 1379 { Fork::Fulu } else { Fork::Gloas });
        assert_eq!(fork.lc_fork(), Some(layout(period)));
        periods.push(period);
    }
    assert_eq!(periods, (FIRST..=LAST).collect::<Vec<_>>());
}

#[test]
fn the_real_chain_into_gloas_with_both_libraries_and_both_stores() {
    let spec = sepolia();
    let backends: [(&str, &dyn BlsBackend); 2] =
        [("blst", &Blst::new(Aggregation::Auto)), ("zkcrypto", &Zkcrypto::new(Aggregation::Auto))];
    let fu = finality_update(&fixture("finality_update.ssz"));
    for (name, bls) in backends {
        for adoption in [Adoption::Full, Adoption::OnChain] {
            let (mut policy, mut reference) = follow(bls, adoption);
            let ctx = Context { spec: &spec, genesis_validators_root: root(GVR), bls };
            policy
                .process_update(&fu, fu.signature_slot + 1, &ctx)
                .unwrap_or_else(|e| panic!("{name} {adoption:?}: {e}"));
            reference.process_update(&fu, fu.signature_slot + 1, &ctx).unwrap();
            assert_eq!(policy.core(), &reference.core);
            let f = &policy.core().finalized_header;
            assert_eq!(
                (f.beacon.slot, f.beacon.hash_tree_root()),
                (FINALIZED_SLOT, canonical(FINALIZED_SLOT)),
                "{name}"
            );
            let Execution::BlockHash { block_hash, .. } = &f.execution else { panic!("not the Gloas layout") };
            assert_eq!(*block_hash, root(BLOCK_HASH));
        }
    }
}

#[test]
fn the_gloas_record_is_the_bids_parent_block() {
    let spec = sepolia();
    let bls = Blst::new(Aggregation::Auto);
    let (mut policy, _) = follow(&bls, Adoption::OnChain);
    let fu = finality_update(&fixture("finality_update.ssz"));
    policy
        .process_update(
            &fu,
            fu.signature_slot + 1,
            &Context { spec: &spec, genesis_validators_root: root(GVR), bls: &bls },
        )
        .unwrap();
    let rlp = fixture("header.rlp");
    let block = json("block.json")["result"].clone();
    assert_eq!(header_rlp(&block), rlp, "the RLP header is rebuilt from the provider's fields here too");
    let record = HeaderRecord::from_block_hash(&policy, SEPOLIA, &rlp).unwrap();
    assert_eq!(
        (record.chain_id(), record.anchor_slot(), record.binding()),
        (SEPOLIA, FINALIZED_SLOT, Binding::BlockHash)
    );
    assert_eq!((record.number(), record.block_hash()), (BLOCK_NUMBER, root(BLOCK_HASH)));
    for b in [block.clone(), json("block_second.json")["result"].clone()] {
        assert_eq!(record.block_hash(), root(b["hash"].as_str().unwrap()));
        assert_eq!(record.state_root(), root(b["stateRoot"].as_str().unwrap()));
        assert_eq!(record.receipts_root(), root(b["receiptsRoot"].as_str().unwrap()));
        assert_eq!(record.parent_hash(), root(b["parentHash"].as_str().unwrap()));
    }
    // The bound hash is the parent named by the finalized block's bid (EIP-7732).
    let beacon_block = json("beacon_block_finalized_second.json");
    let bid = &beacon_block["data"]["message"]["body"]["signed_execution_payload_bid"]["message"];
    assert_eq!(root(bid["parent_block_hash"].as_str().unwrap()), record.block_hash());
    // An Amsterdam header: EIP-7928's block-access-list hash and EIP-7843's slot number.
    let h = decode_header(&rlp).unwrap();
    assert_eq!(h.shape, HeaderShape::Amsterdam);
    assert_eq!(h.block_access_list_hash, Some(root(block["blockAccessListHash"].as_str().unwrap())));
    assert_eq!(
        h.slot_number,
        Some(u64::from_str_radix(block["slotNumber"].as_str().unwrap().trim_start_matches("0x"), 16).unwrap())
    );
    assert_eq!(keccak256(&rlp), record.block_hash());
}

#[test]
fn tampering_with_the_real_gloas_data_is_refused() {
    let spec = sepolia();
    let bls = Blst::new(Aggregation::Auto);
    let ctx = Context { spec: &spec, genesis_validators_root: root(GVR), bls: &bls };
    let (store, _) = follow(&bls, Adoption::OnChain);
    let fu = finality_update(&fixture("finality_update.ssz"));
    let refuse = |u: &LightClientUpdate| store.clone().process_update(u, u.signature_slot + 1, &ctx).map(|_| ());
    assert_eq!(refuse(&fu), Ok(()));
    let with = |f: &dyn Fn(&mut LightClientUpdate)| {
        let mut u = fu.clone();
        f(&mut u);
        u
    };
    let hash = |h: &mut LightClientHeader| match &mut h.execution {
        Execution::BlockHash { block_hash, .. } => block_hash[0] ^= 1,
        _ => unreachable!(),
    };
    let branch = |h: &mut LightClientHeader| match &mut h.execution {
        Execution::BlockHash { branch, .. } => branch[10][0] ^= 1,
        _ => unreachable!(),
    };
    assert_eq!(
        refuse(&with(&|u| hash(&mut u.finalized_header))),
        Err(Error::FinalizedHeaderInvalid),
        "bound block hash"
    );
    assert_eq!(
        refuse(&with(&|u| branch(&mut u.finalized_header))),
        Err(Error::FinalizedHeaderInvalid),
        "execution branch"
    );
    assert_eq!(
        refuse(&with(&|u| hash(&mut u.attested_header))),
        Err(Error::AttestedHeaderInvalid),
        "attested block hash"
    );
    assert_eq!(refuse(&with(&|u| u.finality_branch[4][0] ^= 1)), Err(Error::FinalityBranchInvalid));
    assert_eq!(refuse(&with(&|u| u.sync_aggregate.bits[3] = !u.sync_aggregate.bits[3])), Err(Error::SignatureInvalid));
    let sig = refuse(&with(&|u| u.sync_aggregate.signature[60] ^= 1));
    assert!(matches!(sig, Err(Error::SignatureInvalid | Error::SignatureMalformed)), "{sig:?}");
    // Another chain's domain: mainnet's genesis validators root.
    let mainnet = Context {
        genesis_validators_root: root("0x4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95"),
        ..ctx
    };
    assert_eq!(
        store.clone().process_update(&fu, fu.signature_slot + 1, &mainnet).map(|_| ()),
        Err(Error::SignatureInvalid)
    );
    // A Gloas update handed to a store that was not upgraded at the fork.
    let boot = LightClientBootstrap::decode(&fixture("bootstrap.ssz"), LcFork::Electra, 512).unwrap();
    let mut electra =
        FinalizedStore::initialize_with(&root(BOOTSTRAP_ROOT), &boot, LcFork::Electra, &spec, &bls, Adoption::OnChain)
            .unwrap();
    let u1378 = update(1378);
    electra.process_update(&u1378, u1378.signature_slot + 1, &ctx).unwrap();
    let u1379 = update(1379);
    assert_eq!(electra.process_update(&u1379, u1379.signature_slot + 1, &ctx).map(|_| ()), Err(Error::ForkMismatch));
    // The first Gloas handoff with a tampered next-committee key.
    let mut electra_then_gloas = electra.clone();
    electra_then_gloas.upgrade(LcFork::Gloas, &spec).unwrap();
    let mut u = u1379.clone();
    u.next_sync_committee.pubkeys[9][7] ^= 1;
    assert_eq!(
        electra_then_gloas.process_update(&u, u.signature_slot + 1, &ctx).map(|_| ()),
        Err(Error::NextCommitteeBranchInvalid)
    );
    // The record: the real header with a byte flipped at five places or one byte short, and the
    // constructor of the payload-header layouts.
    let mut policy = store.clone();
    policy.process_update(&fu, fu.signature_slot + 1, &ctx).unwrap();
    let rlp = fixture("header.rlp");
    for at in [0, 5, 100, rlp.len() / 2, rlp.len() - 1] {
        let mut t = rlp.clone();
        t[at] ^= 1;
        assert_eq!(HeaderRecord::from_block_hash(&policy, SEPOLIA, &t), Err(Error::HeaderHashMismatch), "byte {at}");
    }
    assert_eq!(HeaderRecord::from_block_hash(&policy, SEPOLIA, &rlp[..rlp.len() - 1]), Err(Error::HeaderHashMismatch));
    assert_eq!(HeaderRecord::from_payload_header(&policy, SEPOLIA), Err(Error::BindingMismatch));
}
