//! Differential oracle comparing `helios-consensus-core` 0.12.0, source unmodified (`a16z/helios`
//! rev `2af9b41887e069144ee56cd710f3ddca195473f8`, pure-Rust `bls12_381` 0.8.0, built with this
//! repo's lockfile), against **Ferrata's program step path** — the exact verification,
//! compact-codec, witness, Miller/final-exp split, multi-part committee adoption, and in-place
//! fork-upgrade code that `lez/guests/program/src/main.rs` runs (`ferrata_core::steps` and
//! `ferrata_core::witness`, using `blst`) — alongside Ferrata's native policy store
//! (`FinalizedStore`, `Adoption::OnChain`) and spec store (`LightClientStore`).
//!
//! What this suite covers:
//! * `helios_pin_is_unmodified_0_12_0`: checks `Cargo.lock`, workspace manifests, and
//!   `.cargo/config.toml` to confirm unmodified `helios-consensus-core` 0.12.0 source at rev
//!   `2af9b41887e069144ee56cd710f3ddca195473f8`, no `[patch]` override, and that Helios links
//!   `bls12_381` (not `blst`).
//! * `every_recorded_real_update_in_repo_agrees_with_helios_on_step_path`: replays **every**
//!   recorded real mainnet and Sepolia update across all 19 `.ssz` directories in the repository
//!   (all 13 bootstraps and 319 update files / 321 updates, Electra and Gloas, period and finality,
//!   including the Electra→Gloas fork upgrade) and asserts 100% of non-vector `.ssz` files
//!   are covered (or explicitly listed with a reason in `NON_REPLAY_SSZ`).
//! * `official_sync_vectors_agree_with_helios_on_step_path`: replays all 33 Helios-decodable
//!   official `consensus-specs` v1.7.0-beta.4 `light_client/sync` vectors through Helios and the
//!   program's step path (96 updates compared verdict & state against Helios; 15 post-`force_update`
//!   updates checked one way, all refused).
//! * `every_field_mutation_agrees_with_helios_on_step_path`: deterministic structured mutations of
//!   **every field** of real Electra and Gloas `LightClientUpdate` and `LightClientFinalityUpdate`
//!   containers (every beacon header field, all 17 Electra payload fields + `extra_data` overflow,
//!   Gloas block hash, every single node of execution/finality/next-committee Merkle branches,
//!   committee keys and aggregate, sync aggregate bits and signatures including G2 non-subgroup /
//!   infinity / wrong-message, slot/current-slot/period/fork edges, SSZ framing via host-side
//!   decoders), plus 17 bootstrap/store mutations judged against Helios and 18 step-path-only
//!   `CompactUpdate`, `SignerWitness`, and `Job` step-order/digest refusals.
//! * `adopted_keys_must_match_committee_key_root_on_step_path`: step-path-only guard verifying
//!   that `Job::adopt` refuses an adopted committee whose keys do not match `pubkeys_root` (two
//!   distinct keys swapped, keeping the G1 aggregate sum identical) with `CommitteeInvalid` across
//!   both multi-part public jobs and single-part `VerifyAll`, for updates and bootstraps.
//! * `resigned_semantic_edges_agree_with_helios_on_step_path`: re-signed fabricated updates (32-key
//!   Electra, 32-key Electra→Gloas fork boundary, and 512-key mainnet-sized 341 vs 342 threshold)
//!   exercising every post-signature semantic boundary (participation thresholds, slot/period/fork
//!   `signature_slot - 1` edges, relevance, finality, committee contradiction, malformed adopted
//!   committees) and pinning Helios's known upstream spec departures (#871, #872, #751) on the
//!   program's step path.
//! * `ci_fast_random_mutations_agree_with_helios_on_step_path`: fast CI subset (400 seeded random
//!   mutations across Electra period, Electra finality, Gloas period, and Gloas finality, half at
//!   fixed `current_slot` and half at `mutated_sig_slot + 1`) judged on the program's step path.
//! * `full_random_mutations_across_all_layouts_agree_with_helios_on_step_path` (`#[ignore]`): full
//!   run (default 10,000 seeded random mutations across the same 4 base updates/layouts/kinds with
//!   a distinct seed, fixed and dynamic `current_slot`), run by `scripts/helios-oracle-full.sh`.
//!
//! Any unexpected disagreement fails with the complete case written out (and dumped to
//! `$FERRATA_ORACLE_FAILURES_DIR` when set).

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use alloy_primitives::{FixedBytes, B256};
use blst::{blst_p2_affine, blst_p2_affine_in_g2, blst_p2_uncompress, BLST_ERROR};
use ferrata_core::{
    bls::{Aggregation, BlsBackend, Blst},
    fork::{ChainSpec, Fork, LcFork, Preset, FAR_FUTURE_EPOCH},
    merkle::{CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA, ZERO},
    policy::{Adoption, FinalizedStore},
    steps::{self, HeaderRecord, Job, StepError},
    store::{ApplyOutcome, LightClientStore},
    types::{
        BeaconBlockHeader, BlsPubkey, Execution, ExecutionPayloadHeader, LightClientBootstrap, LightClientHeader,
        LightClientUpdate, SyncAggregate, SyncCommittee,
    },
    upgrade::{upgrade_bootstrap, upgrade_update},
    validate::{get_lc_execution_root, sync_committee_signing_root, Context},
    witness::{
        encode_light_client_header, encode_sync_aggregate, uncompress_key, uncompressed_keys, CommitteeSummary,
        CompactBootstrap, CompactStore, CompactUpdate, ExecutionSummary, SignerWitness, IDENTITY_UNCOMPRESSED,
    },
    Error, Root,
};
use ferrata_spectests::{
    cases,
    execkit::Rng,
    parse_config, read_ssz_snappy,
    testkit::{
        fabricated_header, fabricated_update, fake_committee, sign, sparse_tree, with_wrong_aggregate, Fabrication,
    },
    CaseConfig,
};
use helios_consensus_core::{
    apply_bootstrap, apply_generic_update,
    consensus_spec::{ConsensusSpec as HeliosSpec, MainnetConsensusSpec, MinimalConsensusSpec},
    force_update as helios_force_update,
    types::{
        Bootstrap as HBootstrap, FinalityUpdate as HFinalityUpdate, Fork as HFork, Forks, GenericUpdate,
        LightClientHeader as HHeader, LightClientStore as HStore, Update as HUpdate,
    },
    verify_bootstrap, verify_generic_update,
};
use serde_yaml::Value;
use sha2::{Digest, Sha256};
use ssz::Decode;
use tree_hash::TreeHash;

static BLST: Blst = Blst::new(Aggregation::Auto);

const HELIOS_VERSION: &str = "0.12.0";
const HELIOS_REV: &str = "2af9b41887e069144ee56cd710f3ddca195473f8";
const SLOTS_PER_PERIOD: u64 = 32 * 256;

const MAINNET_GVR: &str = "4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95";
const SEPOLIA_GVR: &str = "d8ea171f3c94aea21ebc42a1ed61052acf3f9209c00e4efbaaddac09ed9b8078";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

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

fn sepolia() -> ChainSpec {
    let v = |b: u8| [0x90, 0x00, 0x00, b];
    ChainSpec {
        preset: Preset::MAINNET,
        versions: [v(0x69), v(0x70), v(0x71), v(0x72), v(0x73), v(0x74), v(0x75), v(0x76), [0; 4]],
        epochs: [0, 50, 100, 56_832, 132_608, 222_464, 272_640, 353_024, FAR_FUTURE_EPOCH],
    }
}

fn forks(spec: &ChainSpec) -> Forks {
    let f =
        |fork: Fork| HFork { epoch: spec.fork_epoch(fork), fork_version: FixedBytes::from(spec.fork_version(fork)) };
    Forks {
        genesis: f(Fork::Phase0),
        altair: f(Fork::Altair),
        bellatrix: f(Fork::Bellatrix),
        capella: f(Fork::Capella),
        deneb: f(Fork::Deneb),
        electra: f(Fork::Electra),
        fulu: f(Fork::Fulu),
        gloas: f(Fork::Gloas),
    }
}

fn root(s: &str) -> Root {
    hex::decode(s.trim().trim_start_matches("0x")).unwrap().try_into().unwrap()
}

fn b256(r: &Root) -> B256 {
    B256::from(*r)
}

fn read_repo(rel: &str) -> Vec<u8> {
    fs::read(repo_root().join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

// ---------------------------------------------------------------------------------------------
// SSZ encoding & decoding (Electra and Gloas, period updates, finality updates, bootstraps)
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UpdateKind {
    Period,
    Finality,
}

/// Strict SSZ decoder for `LightClientFinalityUpdate` (Electra or Gloas layout), returning `Err`
/// instead of panicking on malformed bytes so mutations exercise the full decode boundary.
fn decode_finality_update(b: &[u8], layout: LcFork, n: usize) -> Result<LightClientUpdate, Error> {
    let (attested, finalized, rest) = match layout {
        LcFork::Electra => {
            let branch_bytes = 32 * layout.finality_branch_len();
            let agg_bytes = SyncAggregate::ssz_len(n);
            let fixed = 8 + branch_bytes + agg_bytes + 8;
            if b.len() < fixed {
                return Err(Error::Malformed);
            }
            let att = u32::from_le_bytes(b[0..4].try_into().unwrap()) as usize;
            let fin = u32::from_le_bytes(b[4..8].try_into().unwrap()) as usize;
            if att != fixed || fin < att || fin > b.len() {
                return Err(Error::Malformed);
            }
            (&b[att..fin], &b[fin..], &b[8..att])
        }
        LcFork::Gloas => {
            const H: usize = 112 + 32 + 11 * 32;
            let branch_bytes = 32 * layout.finality_branch_len();
            let agg_bytes = SyncAggregate::ssz_len(n);
            let total = 2 * H + branch_bytes + agg_bytes + 8;
            if b.len() != total {
                return Err(Error::Malformed);
            }
            (&b[..H], &b[H..2 * H], &b[2 * H..])
        }
        _ => return Err(Error::ForkMismatch),
    };
    let branch_bytes = 32 * layout.finality_branch_len();
    let agg_bytes = SyncAggregate::ssz_len(n);
    if rest.len() != branch_bytes + agg_bytes + 8 {
        return Err(Error::Malformed);
    }
    Ok(LightClientUpdate {
        attested_header: LightClientHeader::decode(attested, layout)?,
        next_sync_committee: SyncCommittee::empty(n),
        next_sync_committee_branch: vec![ZERO; layout.sync_committee_branch_len()],
        finalized_header: LightClientHeader::decode(finalized, layout)?,
        finality_branch: rest[..branch_bytes].chunks_exact(32).map(|c| c.try_into().unwrap()).collect(),
        sync_aggregate: SyncAggregate::decode(&rest[branch_bytes..branch_bytes + agg_bytes], n)?,
        signature_slot: u64::from_le_bytes(rest[branch_bytes + agg_bytes..].try_into().unwrap()),
    })
}

fn decode_ssz_update(b: &[u8], kind: UpdateKind, layout: LcFork, n: usize) -> Result<LightClientUpdate, Error> {
    match kind {
        UpdateKind::Period => LightClientUpdate::decode(b, layout, n),
        UpdateKind::Finality => decode_finality_update(b, layout, n),
    }
}

fn encode_committee(c: &SyncCommittee) -> Vec<u8> {
    let mut out = Vec::with_capacity((c.pubkeys.len() + 1) * 48);
    for pk in &c.pubkeys {
        out.extend_from_slice(pk);
    }
    out.extend_from_slice(&c.aggregate_pubkey);
    out
}

fn encode_bootstrap(b: &LightClientBootstrap, layout: LcFork) -> Vec<u8> {
    let header = encode_light_client_header(&b.header, layout);
    let comm = encode_committee(&b.current_sync_committee);
    let branch: Vec<u8> = b.current_sync_committee_branch.iter().flat_map(|r| r.iter().copied()).collect();
    match layout {
        LcFork::Capella | LcFork::Deneb | LcFork::Electra => {
            let fixed = 4 + comm.len() + branch.len();
            [&(fixed as u32).to_le_bytes()[..], &comm, &branch, &header].concat()
        }
        LcFork::Gloas => [&header[..], &comm, &branch].concat(),
        LcFork::Altair => [&header[..], &comm, &branch].concat(),
    }
}

fn encode_update(u: &LightClientUpdate, kind: UpdateKind, layout: LcFork) -> Vec<u8> {
    let att = encode_light_client_header(&u.attested_header, layout);
    let fin = encode_light_client_header(&u.finalized_header, layout);
    let fin_branch: Vec<u8> = u.finality_branch.iter().flat_map(|r| r.iter().copied()).collect();
    let agg = encode_sync_aggregate(&u.sync_aggregate);
    let sig_slot = u.signature_slot.to_le_bytes();
    match (kind, layout) {
        (UpdateKind::Period, LcFork::Capella | LcFork::Deneb | LcFork::Electra) => {
            let comm = encode_committee(&u.next_sync_committee);
            let next_branch: Vec<u8> = u.next_sync_committee_branch.iter().flat_map(|r| r.iter().copied()).collect();
            let fixed = 4 + comm.len() + next_branch.len() + 4 + fin_branch.len() + agg.len() + 8;
            [
                &(fixed as u32).to_le_bytes()[..],
                &comm,
                &next_branch,
                &((fixed + att.len()) as u32).to_le_bytes(),
                &fin_branch,
                &agg,
                &sig_slot,
                &att,
                &fin,
            ]
            .concat()
        }
        (UpdateKind::Finality, LcFork::Capella | LcFork::Deneb | LcFork::Electra) => {
            let fixed = 4 + 4 + fin_branch.len() + agg.len() + 8;
            [
                &(fixed as u32).to_le_bytes()[..],
                &((fixed + att.len()) as u32).to_le_bytes(),
                &fin_branch,
                &agg,
                &sig_slot,
                &att,
                &fin,
            ]
            .concat()
        }
        (UpdateKind::Period, LcFork::Gloas) => {
            let comm = encode_committee(&u.next_sync_committee);
            let next_branch: Vec<u8> = u.next_sync_committee_branch.iter().flat_map(|r| r.iter().copied()).collect();
            [&att[..], &comm, &next_branch, &fin, &fin_branch, &agg, &sig_slot].concat()
        }
        (UpdateKind::Finality, LcFork::Gloas) => [&att[..], &fin, &fin_branch, &agg, &sig_slot].concat(),
        _ => panic!("unsupported encoding ({kind:?}, {layout:?})"),
    }
}

fn helios_decode_update<S: HeliosSpec>(bytes: &[u8]) -> Result<GenericUpdate<S>, String> {
    if let Ok(u) = HUpdate::<S>::from_ssz_bytes(bytes) {
        return Ok(GenericUpdate::from(&u));
    }
    match HFinalityUpdate::<S>::from_ssz_bytes(bytes) {
        Ok(u) => Ok(GenericUpdate::from(&u)),
        Err(e) => Err(format!("Helios decode failed: {e:?}")),
    }
}

// ---------------------------------------------------------------------------------------------
// Comparable views across Helios, Ferrata native stores, and Ferrata's program step path
// ---------------------------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq, Clone)]
struct FullView {
    finalized: (u64, Root),
    block: (Option<u64>, Root),
    execution_root: Root,
    current: Root,
    next: Option<Root>,
    optimistic: (u64, Root),
}

#[derive(Debug, PartialEq, Eq, Clone)]
struct FinalizedView {
    finalized: (u64, Root),
    block: (Option<u64>, Root),
    execution_root: Root,
    current: Root,
    next: Option<Root>,
}

impl FullView {
    fn finalized_only(&self) -> FinalizedView {
        FinalizedView {
            finalized: self.finalized,
            block: self.block,
            execution_root: self.execution_root,
            current: self.current,
            next: self.next,
        }
    }
}

fn ferrata_full_view(store: &LightClientStore, spec: &ChainSpec) -> FullView {
    let core = &store.core;
    let header = &core.finalized_header;
    let block = match &header.execution {
        Execution::Payload { header, .. } => (Some(header.block_number), header.block_hash),
        Execution::BlockHash { block_hash, .. } => (None, *block_hash),
        Execution::None => (None, ZERO),
    };
    FullView {
        finalized: (header.beacon.slot, header.beacon.hash_tree_root()),
        block,
        execution_root: get_lc_execution_root(header, core.fork, spec),
        current: core.current_sync_committee.hash_tree_root(),
        next: (!core.next_sync_committee.is_empty()).then(|| core.next_sync_committee.hash_tree_root()),
        optimistic: (store.optimistic_header.beacon.slot, store.optimistic_header.beacon.hash_tree_root()),
    }
}

fn helios_full_view<S: HeliosSpec>(store: &HStore<S>) -> FullView {
    let header = &store.finalized_header;
    let block = match header {
        HHeader::Gloas(g) => (None, g.execution_block_hash.0),
        other => match other.execution() {
            Ok(p) => (Some(*p.block_number()), p.block_hash().0),
            Err(_) => (None, ZERO),
        },
    };
    FullView {
        finalized: (header.beacon().slot, header.beacon().tree_hash_root().0),
        block,
        execution_root: header.execution_root().0,
        current: store.current_sync_committee.tree_hash_root().0,
        next: store.next_sync_committee.as_ref().map(|c| c.tree_hash_root().0),
        optimistic: (store.optimistic_header.beacon().slot, store.optimistic_header.beacon().tree_hash_root().0),
    }
}

fn moved_flags(before: &FinalizedView, after: &FinalizedView, spec: &ChainSpec) -> (bool, bool, bool) {
    let rotated = spec.period_at_slot(after.finalized.0) > spec.period_at_slot(before.finalized.0);
    let advanced = after.finalized.0 > before.finalized.0;
    let adopted = if rotated { after.next.is_some() } else { before.next.is_none() && after.next.is_some() };
    (rotated, advanced, adopted)
}

fn finalized_views_match(ferrata: &FinalizedView, helios: &FinalizedView, ferrata_gloas_from_pre_gloas: bool) -> bool {
    let block_num_ok = ferrata.block.0 == helios.block.0 || (ferrata_gloas_from_pre_gloas && ferrata.block.0.is_none());
    ferrata.finalized == helios.finalized
        && block_num_ok
        && ferrata.block.1 == helios.block.1
        && ferrata.execution_root == helios.execution_root
        && ferrata.current == helios.current
        && ferrata.next == helios.next
}

fn full_views_match(ferrata: &FullView, helios: &FullView, upgraded_to_gloas_without_advance: bool) -> bool {
    finalized_views_match(&ferrata.finalized_only(), &helios.finalized_only(), upgraded_to_gloas_without_advance)
        && ferrata.optimistic == helios.optimistic
}

// ---------------------------------------------------------------------------------------------
// The program's step path, called natively via the exact entry points of `lez/guests/program`
// ---------------------------------------------------------------------------------------------

/// State held by a program instance (`InstanceState.store` encoded bytes + latest `HeaderRecord`),
/// together with the off-chain host's committee cache used solely to build `SignerWitness` bytes.
///
/// Cross-reference: this is a native mirror of the handler sequence in
/// `lez/guests/program/src/main.rs` (`init`, `open`, `miller`, `adopt`, `finish`, `apply_job`,
/// `verify_all`, `apply_verified`). Keep the call order here in step if `main.rs` changes.
#[derive(Clone)]
struct ProgramStepState {
    store_bytes: Vec<u8>,
    last_record_bytes: Vec<u8>,
    current_committee: SyncCommittee,
    next_committee: Option<SyncCommittee>,
    parts: u8,
}

impl ProgramStepState {
    /// Exact native execution of `lez/guests/program/src/main.rs::init` + `adopt` + `apply_job`.
    fn initialize(
        trusted: &Root,
        bootstrap: &LightClientBootstrap,
        layout: LcFork,
        spec: &ChainSpec,
        parts: u8,
    ) -> Result<Self, StepError> {
        let n = spec.preset.sync_committee_size;
        let boot_bytes = CompactBootstrap::of(bootstrap).encode(layout);
        let compact_boot = CompactBootstrap::decode(&boot_bytes, layout)?;
        let (store, job) = steps::initialize(trusted, &compact_boot, layout, spec, parts)?;
        let mut job_bytes = job.encode();
        let keys = uncompressed_keys(&bootstrap.current_sync_committee).map_err(|_| Error::CommitteeInvalid)?;
        let chunk = n / parts as usize;
        for p in 0..parts {
            let mut j = Job::decode(&job_bytes)?;
            j.adopt(p, &keys[p as usize * chunk..(p as usize + 1) * chunk], n)?;
            job_bytes = j.encode();
        }
        let j = Job::decode(&job_bytes)?;
        if !j.is_verified() {
            return Err(StepError::AdoptionIncomplete);
        }
        let store_bytes = store.encode();
        let stored = CompactStore::decode(&store_bytes)?;
        let (active, _) = steps::apply(&stored, &j, spec)?;
        let record_bytes = j.pending.record().encode();
        let _ = HeaderRecord::decode(&record_bytes)?;
        Ok(Self {
            store_bytes: active.encode(),
            last_record_bytes: record_bytes,
            current_committee: bootstrap.current_sync_committee.clone(),
            next_committee: None,
            parts,
        })
    }

    fn compact_store(&self) -> CompactStore {
        CompactStore::decode(&self.store_bytes).expect("stored CompactStore decodes")
    }

    fn last_record(&self) -> HeaderRecord {
        HeaderRecord::decode(&self.last_record_bytes).expect("stored HeaderRecord decodes")
    }

    fn view(&self) -> FinalizedView {
        let s = self.compact_store();
        let r = self.last_record();
        let block = match r.execution {
            ExecutionSummary::Payload { block_number, block_hash, .. } => (Some(block_number), block_hash),
            ExecutionSummary::BlockHash(h) => (None, h),
            ExecutionSummary::None => (None, ZERO),
        };
        FinalizedView {
            finalized: (s.finalized.slot, s.finalized.hash_tree_root()),
            block,
            execution_root: r.execution_root,
            current: s.current.root(),
            next: s.next.map(|c| c.root()),
        }
    }

    fn build_witness(&self, update: &LightClientUpdate, spec: &ChainSpec) -> SignerWitness {
        let s = self.compact_store();
        let committee = if spec.period_at_slot(update.signature_slot) == spec.period_at_slot(s.finalized.slot) {
            Some(&self.current_committee)
        } else {
            self.next_committee.as_ref()
        };
        committee.and_then(|c| SignerWitness::build(c, &update.sync_aggregate.bits).ok()).unwrap_or_default()
    }

    /// Exact native execution of the guest's public job path:
    /// `Open` (`CompactStore::decode`, `CompactUpdate::layout_of`, `CompactUpdate::decode`,
    /// `steps::store_for_update`, `SignerWitness::decode`, `steps::step1`) -> `Miller` (`step2`)
    /// -> `Adopt` (`parts` parts) -> `Finish` (`!adoption_pending`, `step3`) -> `Apply`
    /// (`steps::apply` against the pre-upgrade store) -> `CompactStore::encode` + `HeaderRecord`.
    fn run_public_job(
        &self,
        update: &LightClientUpdate,
        layout: LcFork,
        current_slot: u64,
        spec: &ChainSpec,
        gvr: &Root,
    ) -> Result<(Self, ApplyOutcome, HeaderRecord), StepError> {
        let n = spec.preset.sync_committee_size;
        let update_bytes = CompactUpdate::of(update).encode(layout);
        let witness_bytes = self.build_witness(update, spec).encode();
        self.run_public_job_raw(&update_bytes, &witness_bytes, &update.next_sync_committee, current_slot, spec, gvr, n)
    }

    #[allow(clippy::too_many_arguments)]
    fn run_public_job_raw(
        &self,
        update_bytes: &[u8],
        witness_bytes: &[u8],
        next_committee_for_adoption: &SyncCommittee,
        current_slot: u64,
        spec: &ChainSpec,
        gvr: &Root,
        n: usize,
    ) -> Result<(Self, ApplyOutcome, HeaderRecord), StepError> {
        // Open
        let stored = CompactStore::decode(&self.store_bytes)?;
        let layout = CompactUpdate::layout_of(update_bytes)?;
        let cu = CompactUpdate::decode(update_bytes, layout, n)?;
        let store_u = steps::store_for_update(&stored, layout, cu.attested_header.beacon.slot, spec)?;
        let sw = SignerWitness::decode(witness_bytes, n)?;
        let job = steps::step1(&store_u, &cu, &sw, current_slot, spec, gvr, self.parts)?;
        let job_bytes = job.encode();

        // Miller
        let mut job = Job::decode(&job_bytes)?;
        job.step2()?;
        let mut job_bytes = job.encode();

        // Adopt (in `self.parts` parts, matching the public job sequence before Finish)
        let pending_adopt = Job::decode(&job_bytes)?.adoption_pending();
        if pending_adopt {
            let keys = uncompressed_keys(next_committee_for_adoption).map_err(|_| Error::CommitteeInvalid)?;
            let chunk = n / self.parts as usize;
            for p in 0..self.parts {
                let mut job = Job::decode(&job_bytes)?;
                job.adopt(p, &keys[p as usize * chunk..(p as usize + 1) * chunk], n)?;
                job_bytes = job.encode();
            }
        }

        // Finish
        let mut job = Job::decode(&job_bytes)?;
        if job.adoption_pending() {
            return Err(StepError::AdoptionIncomplete);
        }
        job.step3()?;
        let job_bytes = job.encode();

        // Apply (applied to the stored pre-upgrade CompactStore, exactly as `apply_job` does)
        let job = Job::decode(&job_bytes)?;
        let stored_again = CompactStore::decode(&self.store_bytes)?;
        let (next_store, outcome) = steps::apply(&stored_again, &job, spec)?;
        let next_store_bytes = next_store.encode();
        let _ = CompactStore::decode(&next_store_bytes)?;

        let job_record_bytes = job.pending.record().encode();
        let job_record = HeaderRecord::decode(&job_record_bytes)?;
        let (next_cur, next_nxt) = Self::advance_committees(
            &self.current_committee,
            self.next_committee.as_ref(),
            next_committee_for_adoption,
            outcome,
        );
        let last_record_bytes = if outcome.advanced { job_record_bytes } else { self.last_record_bytes.clone() };
        Ok((
            Self {
                store_bytes: next_store_bytes,
                last_record_bytes,
                current_committee: next_cur,
                next_committee: next_nxt,
                parts: self.parts,
            },
            outcome,
            job_record,
        ))
    }

    /// Exact native execution of `lez/guests/program/src/main.rs::verify_all` + `apply_verified`
    /// (`parts = 1`, `step1` -> `step2` -> `step3` -> `adopt(0)` -> `apply`).
    fn run_verify_all(
        &self,
        update: &LightClientUpdate,
        layout: LcFork,
        current_slot: u64,
        spec: &ChainSpec,
        gvr: &Root,
    ) -> Result<(Self, ApplyOutcome, HeaderRecord), StepError> {
        let n = spec.preset.sync_committee_size;
        let update_bytes = CompactUpdate::of(update).encode(layout);
        let witness_bytes = self.build_witness(update, spec).encode();

        let stored = CompactStore::decode(&self.store_bytes)?;
        let dec_layout = CompactUpdate::layout_of(&update_bytes)?;
        let cu = CompactUpdate::decode(&update_bytes, dec_layout, n)?;
        let store_u = steps::store_for_update(&stored, dec_layout, cu.attested_header.beacon.slot, spec)?;
        let sw = SignerWitness::decode(&witness_bytes, n)?;
        let mut job = steps::step1(&store_u, &cu, &sw, current_slot, spec, gvr, 1)?;
        job.step2()?;
        job.step3()?;
        if job.adoption_pending() {
            let keys = uncompressed_keys(&update.next_sync_committee).map_err(|_| Error::CommitteeInvalid)?;
            job.adopt(0, &keys, n)?;
        }
        let job_bytes = job.encode();
        let job = Job::decode(&job_bytes)?;
        let stored_again = CompactStore::decode(&self.store_bytes)?;
        let (next_store, outcome) = steps::apply(&stored_again, &job, spec)?;
        let next_store_bytes = next_store.encode();
        let _ = CompactStore::decode(&next_store_bytes)?;
        let job_record_bytes = job.pending.record().encode();
        let job_record = HeaderRecord::decode(&job_record_bytes)?;
        let (next_cur, next_nxt) = Self::advance_committees(
            &self.current_committee,
            self.next_committee.as_ref(),
            &update.next_sync_committee,
            outcome,
        );
        let last_record_bytes = if outcome.advanced { job_record_bytes } else { self.last_record_bytes.clone() };
        Ok((
            Self {
                store_bytes: next_store_bytes,
                last_record_bytes,
                current_committee: next_cur,
                next_committee: next_nxt,
                parts: self.parts,
            },
            outcome,
            job_record,
        ))
    }

    fn advance_committees(
        current: &SyncCommittee,
        next: Option<&SyncCommittee>,
        update_next: &SyncCommittee,
        outcome: ApplyOutcome,
    ) -> (SyncCommittee, Option<SyncCommittee>) {
        let adopted_comm = if outcome.adopted { Some(update_next.clone()) } else { None };
        if outcome.rotated {
            (next.expect("rotated requires known next committee").clone(), adopted_comm)
        } else if outcome.adopted {
            (current.clone(), adopted_comm)
        } else {
            (current.clone(), next.cloned())
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Four-way differential harness: Helios 0.12.0 + program step path + policy store + spec store
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KnownDivergence {
    /// Expected full agreement on every invariant.
    None,
    /// a16z/helios#872: finalized header empty or slot 0 while finality branch is non-zero.
    /// Ferrata spec & step path refuse; Helios accepts optimistically and does not move finalized.
    Helios872EmptyFinalizedWithBranch,
    /// Re-signed fabricated update adopting a malformed `next_sync_committee` (bad aggregate,
    /// identity key, or off-curve key): spec & Helios accept and store the committee without
    /// validating its keys, while Ferrata's on-chain adoption (`Job::adopt` / `FinalizedStore`)
    /// refuses before adopting it.
    StricterOnChainAdoption,
}

#[derive(Clone)]
struct Quad<S: HeliosSpec> {
    chain_name: String,
    spec: ChainSpec,
    gvr: Root,
    steps: ProgramStepState,
    policy: FinalizedStore,
    spec_store: LightClientStore,
    helios: HStore<S>,
}

#[derive(Debug)]
struct StepReport {
    helios_valid: bool,
    helios_moved: (bool, bool, bool),
    steps_ok: bool,
    steps_err: Option<StepError>,
}

impl<S: HeliosSpec> Quad<S> {
    fn from_bootstrap(
        chain_name: &str,
        spec: ChainSpec,
        gvr: Root,
        trusted: Root,
        boot_bytes: &[u8],
        layout: LcFork,
        parts: u8,
    ) -> Self {
        let n = spec.preset.sync_committee_size;
        let boot = LightClientBootstrap::decode(boot_bytes, layout, n).expect("ferrata bootstrap decode");
        let steps = ProgramStepState::initialize(&trusted, &boot, layout, &spec, parts).expect("steps::initialize");
        let policy = FinalizedStore::initialize_with(&trusted, &boot, layout, &spec, &BLST, Adoption::OnChain).unwrap();
        let spec_store = LightClientStore::initialize(&trusted, &boot, layout, &spec).unwrap();
        let hboot = HBootstrap::<S>::from_ssz_bytes(boot_bytes).expect("helios bootstrap decode");
        verify_bootstrap(&hboot, b256(&trusted), &forks(&spec)).expect("helios verify_bootstrap");
        let mut helios = HStore::<S>::default();
        apply_bootstrap(&mut helios, &hboot);
        let q = Self { chain_name: chain_name.to_string(), spec, gvr, steps, policy, spec_store, helios };
        assert_eq!(ferrata_full_view(&q.spec_store, &q.spec), helios_full_view(&q.helios), "{chain_name}: bootstrap");
        assert_eq!(q.steps.view(), helios_full_view(&q.helios).finalized_only(), "{chain_name}: bootstrap steps");
        q
    }

    /// Judge one update (given as exact SSZ bytes) on a cloned or live state, comparing all four
    /// implementations and panicking with the complete serialized case on any disagreement.
    #[allow(clippy::too_many_arguments)]
    fn judge(
        &mut self,
        case_id: &str,
        bytes: &[u8],
        kind: UpdateKind,
        served_layout: LcFork,
        current_slot: u64,
        commit_if_ok: bool,
        expected_div: KnownDivergence,
    ) -> StepReport {
        let n = self.spec.preset.sync_committee_size;
        let ctx = Context { spec: &self.spec, genesis_validators_root: self.gvr, bls: &BLST };

        // 1. Helios 0.12.0 (unmodified)
        let h_before = helios_full_view(&self.helios);
        let mut h_after_store = self.helios.clone();
        let helios_res: Result<(), String> = match helios_decode_update::<S>(bytes) {
            Ok(hu) => {
                match verify_generic_update(&hu, current_slot, &h_after_store, b256(&self.gvr), &forks(&self.spec)) {
                    Ok(()) => {
                        apply_generic_update(&mut h_after_store, &hu);
                        Ok(())
                    }
                    Err(e) => Err(format!("verify_generic_update: {e}")),
                }
            }
            Err(e) => Err(e),
        };
        let helios_valid = helios_res.is_ok();
        let h_after = helios_full_view(&h_after_store);
        let helios_moved = moved_flags(&h_before.finalized_only(), &h_after.finalized_only(), &self.spec);
        let helios_finalized_moved = helios_moved.0 || helios_moved.1 || helios_moved.2;

        // 2. Ferrata SSZ decode
        let decoded = decode_ssz_update(bytes, kind, served_layout, n);

        // 3. Ferrata spec store, policy store, and program step path (public job + VerifyAll)
        let mut spec_after = self.spec_store.clone();
        let mut policy_after = self.policy.clone();

        let (spec_res, policy_res, steps_res, verify_all_res) = match &decoded {
            Err(e) => (Err(*e), Err(*e), Err(StepError::Core(*e)), Err(StepError::Core(*e))),
            Ok(u) => {
                // Native stores require explicit fork upgrade when an update arrives in a later
                // scheduled layout; the program's step path upgrades itself via `store_for_update`.
                let mut native_upgrade_ok = true;
                let u_native = if served_layout > spec_after.core.fork {
                    let scheduled =
                        self.spec.fork_at_epoch(self.spec.epoch_at_slot(u.attested_header.beacon.slot)).lc_fork();
                    if scheduled == Some(served_layout) {
                        spec_after.upgrade(served_layout, &self.spec).unwrap();
                        policy_after.upgrade(served_layout, &self.spec).unwrap();
                        Ok(u.clone())
                    } else {
                        native_upgrade_ok = false;
                        Err(Error::ForkMismatch)
                    }
                } else if served_layout < spec_after.core.fork {
                    upgrade_update(u.clone(), served_layout, spec_after.core.fork, &self.spec)
                } else {
                    Ok(u.clone())
                };
                let spec_r = match &u_native {
                    Ok(un) if native_upgrade_ok => spec_after.process_update(un, current_slot, &ctx),
                    Err(e) => Err(*e),
                    _ => Err(Error::ForkMismatch),
                };
                let policy_r = match &u_native {
                    Ok(un) if native_upgrade_ok => policy_after.process_update(un, current_slot, &ctx),
                    Err(e) => Err(*e),
                    _ => Err(Error::ForkMismatch),
                };
                // Program step path: NEVER told to upgrade; passes `served_layout` directly into
                // `CompactUpdate::encode(served_layout)` -> `layout_of` -> `store_for_update`.
                let steps_r = self.steps.run_public_job(u, served_layout, current_slot, &self.spec, &self.gvr);
                let va_r = self.steps.run_verify_all(u, served_layout, current_slot, &self.spec, &self.gvr);
                (spec_r, policy_r, steps_r, va_r)
            }
        };

        let spec_ok = spec_res.is_ok();
        let policy_ok = policy_res.is_ok();
        let steps_ok = steps_res.is_ok();
        let va_ok = verify_all_res.is_ok();

        let mut problems: Vec<String> = Vec::new();

        // Program step path (public job) vs VerifyAll vs FinalizedStore(OnChain) must always agree.
        if steps_ok != va_ok {
            problems.push(format!("step path public job ({steps_ok}) != VerifyAll ({va_ok})"));
        }
        if steps_ok != policy_ok {
            // Note: on a store that already upgraded to a later layout receiving an older-layout
            // update, `store_for_update` refuses with `ForkMismatch` whereas native `upgrade_update`
            // upgrades the update first; outside that explicit case they must match.
            let downgrade_input = served_layout < self.steps.compact_store().fork;
            if !downgrade_input {
                problems.push(format!(
                    "step path ({steps_ok}, {:?}) != FinalizedStore(OnChain) ({policy_ok}, {:?})",
                    steps_res.as_ref().err(),
                    policy_res.as_ref().err()
                ));
            }
        }
        if let (Ok((st_pub, out_pub, rec_pub)), Ok((st_va, out_va, rec_va))) = (&steps_res, &verify_all_res) {
            if st_pub.store_bytes != st_va.store_bytes || out_pub != out_va || rec_pub != rec_va {
                problems.push("public job and VerifyAll produced different post-states/outcomes/records".into());
            }
        }

        match expected_div {
            KnownDivergence::None => {
                if spec_ok != helios_valid {
                    problems.push(format!("spec store ({spec_ok}) != Helios 0.12.0 ({helios_valid})"));
                }
                if spec_ok && helios_valid {
                    let s_view = ferrata_full_view(&spec_after, &self.spec);
                    // Spec `upgrade_lc_store_to_gloas` converts an existing pre-Gloas finalized
                    // header to Gloas (`block_number = None`, keeping `block_hash` and
                    // `execution_root` identical), whereas Helios stores per-header fork variants
                    // so an optimistic-only Gloas update leaves Helios's pre-Gloas `block_number`
                    // intact. Every other field must match bit for bit.
                    let upgraded_to_gloas_without_advance = spec_after.core.fork >= LcFork::Gloas && !helios_moved.1;
                    if !full_views_match(&s_view, &h_after, upgraded_to_gloas_without_advance) {
                        problems.push(format!(
                            "spec store view != Helios view:\n  ferrata: {s_view:?}\n  helios:  {h_after:?}"
                        ));
                    }
                }
                let expected_steps_ok = helios_valid && helios_finalized_moved;
                if steps_ok != expected_steps_ok {
                    problems.push(format!(
                        "program step path ({steps_ok}) != (Helios valid {helios_valid} && Helios finalized moved {helios_finalized_moved} {helios_moved:?})"
                    ));
                }
                if let Ok((st_after, out, rec)) = &steps_res {
                    let out_tuple = (out.rotated, out.advanced, out.adopted);
                    if out_tuple != helios_moved {
                        problems.push(format!("step outcome {out_tuple:?} != Helios moved {helios_moved:?}"));
                    }
                    if let Ok(pol_out) = &policy_res {
                        if *out != *pol_out {
                            problems.push(format!("step outcome {out:?} != policy outcome {pol_out:?}"));
                        }
                        if st_after.compact_store() != CompactStore::of(policy_after.core()) {
                            problems.push("step CompactStore != CompactStore::of(policy)".into());
                        }
                    }
                    let st_view = st_after.view();
                    if st_view != h_after.finalized_only() {
                        problems.push(format!(
                            "program step view != Helios finalized view:\n  steps:  {st_view:?}\n  helios: {:?}",
                            h_after.finalized_only()
                        ));
                    }
                    if out.advanced && rec.beacon.hash_tree_root() != h_after.finalized.1 {
                        problems.push("HeaderRecord beacon root != Helios finalized beacon root".into());
                    }
                }
            }
            KnownDivergence::Helios872EmptyFinalizedWithBranch => {
                if spec_ok || steps_ok || !helios_valid || helios_finalized_moved {
                    problems.push(format!(
                        "expected helios#872 (spec=false, steps=false, helios_valid=true, helios_fin_moved=false), got spec={spec_ok} steps={steps_ok} helios_valid={helios_valid} helios_fin_moved={helios_finalized_moved}"
                    ));
                }
            }
            KnownDivergence::StricterOnChainAdoption => {
                if !spec_ok || !helios_valid || steps_ok || policy_ok {
                    problems.push(format!(
                        "expected stricter on-chain adoption (spec=true, helios=true, steps=false, policy=false), got spec={spec_ok} helios={helios_valid} steps={steps_ok} policy={policy_ok}"
                    ));
                }
            }
        }

        if !problems.is_empty() {
            let report = format_disagreement(
                case_id,
                &self.chain_name,
                kind,
                served_layout,
                current_slot,
                &h_before,
                bytes,
                &helios_res,
                helios_moved,
                &h_after,
                &spec_res,
                &policy_res,
                &steps_res.as_ref().map(|(_, o, _)| *o),
                &verify_all_res.as_ref().map(|(_, o, _)| *o),
                &problems,
            );
            dump_failure_case(case_id, &report, bytes);
            panic!("{report}");
        }

        let steps_err = match steps_res {
            Ok((st_after, _, _)) => {
                if commit_if_ok && helios_valid {
                    self.steps = st_after;
                    self.policy = policy_after;
                }
                None
            }
            Err(e) => Some(e),
        };
        if commit_if_ok && helios_valid {
            self.helios = h_after_store;
            self.spec_store = spec_after;
        }

        StepReport { helios_valid, helios_moved, steps_ok, steps_err }
    }
}

#[allow(clippy::too_many_arguments)]
fn format_disagreement(
    case_id: &str,
    chain: &str,
    kind: UpdateKind,
    layout: LcFork,
    current_slot: u64,
    before: &FullView,
    bytes: &[u8],
    helios_res: &Result<(), String>,
    helios_moved: (bool, bool, bool),
    helios_after: &FullView,
    spec_res: &Result<(), Error>,
    policy_res: &Result<ApplyOutcome, Error>,
    steps_res: &Result<ApplyOutcome, &StepError>,
    va_res: &Result<ApplyOutcome, &StepError>,
    problems: &[String],
) -> String {
    let sha = hex::encode(Sha256::digest(bytes));
    let preview = hex::encode(&bytes[..bytes.len().min(128)]);
    format!(
        "=== HELIOS STEP-PATH ORACLE DISAGREEMENT ===\n\
         case:          {case_id}\n\
         chain:         {chain}\n\
         kind/layout:   {kind:?} / {layout:?}\n\
         current_slot:  {current_slot}\n\
         store_before:  finalized_slot={} root=0x{} next_known={}\n\
         update_bytes:  {} bytes, sha256={sha}, head128=0x{preview}\n\
         helios 0.12.0: {helios_res:?}, moved={helios_moved:?}, after_fin=({}, 0x{})\n\
         ferrata spec:  {spec_res:?}\n\
         ferrata policy:{policy_res:?}\n\
         program steps: {steps_res:?}\n\
         verify_all:    {va_res:?}\n\
         problems:\n  - {}\n",
        before.finalized.0,
        hex::encode(before.finalized.1),
        before.next.is_some(),
        bytes.len(),
        helios_after.finalized.0,
        hex::encode(helios_after.finalized.1),
        problems.join("\n  - "),
    )
}

fn dump_failure_case(case_id: &str, report: &str, bytes: &[u8]) {
    if let Ok(dir) = std::env::var("FERRATA_ORACLE_FAILURES_DIR") {
        let dir = PathBuf::from(dir);
        let _ = fs::create_dir_all(&dir);
        let safe: String =
            case_id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
        let _ = fs::write(dir.join(format!("{safe}.txt")), report);
        let _ = fs::write(dir.join(format!("{safe}.ssz")), bytes);
    }
}

// ---------------------------------------------------------------------------------------------
// 1. Unmodified Helios 0.12.0 pin & BLS library verification
// ---------------------------------------------------------------------------------------------

const HELIOS_SOURCE_SHA256: &str = "17be8945b6af08d5b07da5f84d734c09c9375d86f982713beaf1a9cdfa410a20";
const BLS12_381_0_8_0_CHECKSUM: &str = "d7bc6d6292be3a19e6379786dac800f551e5865a5bb51ebbe3064ab80433f403";
const BLST_0_3_17_CHECKSUM: &str = "c20659f9bbee16cbbd2f7393e40ab6309f5a98f76a2eb57a995ec508b72387fe";

fn find_helios_checkout() -> Option<PathBuf> {
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cargo")))?;
    let checkouts = cargo_home.join("git").join("checkouts");
    let entries = fs::read_dir(checkouts).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with("helios-") {
            continue;
        }
        let candidate = entry.path().join(&HELIOS_REV[..7]).join("ethereum").join("consensus-core");
        if candidate.join("Cargo.toml").is_file() {
            return Some(candidate);
        }
    }
    None
}

fn hash_helios_crate_tree(base: &Path) -> (usize, String) {
    fn walk_rs(dir: &Path, out: &mut Vec<PathBuf>) {
        if let Ok(rd) = fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk_rs(&p, out);
                } else if p.extension().and_then(|s| s.to_str()) == Some("rs") {
                    out.push(p);
                }
            }
        }
    }
    let mut files = vec![base.join("Cargo.toml")];
    walk_rs(&base.join("src"), &mut files);
    let mut rel_files: Vec<(String, PathBuf)> =
        files.into_iter().map(|p| (p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/"), p)).collect();
    rel_files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut h = Sha256::new();
    for (rel, path) in &rel_files {
        h.update(rel.as_bytes());
        h.update([0u8]);
        h.update(fs::read(path).unwrap());
    }
    (rel_files.len(), hex::encode(h.finalize()))
}

#[test]
fn helios_pin_is_unmodified_0_12_0() {
    let root_dir = repo_root();
    let lock = fs::read_to_string(root_dir.join("Cargo.lock")).unwrap();
    let pkg = |name: &str| -> &str {
        lock.split("[[package]]")
            .find(|b| b.contains(&format!("name = \"{name}\"")))
            .unwrap_or_else(|| panic!("{name} missing from Cargo.lock"))
    };
    let block = pkg("helios-consensus-core");
    assert!(block.contains(&format!("version = \"{HELIOS_VERSION}\"")), "unexpected Helios version:\n{block}");
    let want_src = format!("source = \"git+https://github.com/a16z/helios?rev={HELIOS_REV}#{HELIOS_REV}\"");
    assert!(block.contains(&want_src), "unexpected Helios source pin:\n{block}");
    // Confirm which BLS crate Helios 0.12.0 links: pure-Rust `bls12_381`, not `blst`.
    assert!(block.contains("\"bls12_381\""), "Helios 0.12.0 should depend on bls12_381:\n{block}");
    assert!(!block.contains("\"blst"), "Helios 0.12.0 unexpectedly depends on blst:\n{block}");

    let bls12 = pkg("bls12_381");
    assert!(
        bls12.contains("version = \"0.8.0\"") && bls12.contains(&format!("checksum = \"{BLS12_381_0_8_0_CHECKSUM}\"")),
        "unexpected bls12_381 pin:\n{bls12}"
    );
    let blst_pkg = pkg("blst");
    assert!(
        blst_pkg.contains("version = \"0.3.17\"")
            && blst_pkg.contains(&format!("checksum = \"{BLST_0_3_17_CHECKSUM}\"")),
        "unexpected blst pin:\n{blst_pkg}"
    );

    // Confirm no [patch] section overrides Helios in the root workspace manifests or Cargo config,
    // and core does not link Helios.
    for rel in
        ["Cargo.toml", ".cargo/config.toml", "crates/ferrata-spectests/Cargo.toml", "crates/ferrata-core/Cargo.toml"]
    {
        let manifest = fs::read_to_string(root_dir.join(rel)).unwrap();
        assert!(!manifest.contains("[patch"), "{rel} must not patch dependencies");
    }
    let core_toml = fs::read_to_string(root_dir.join("crates/ferrata-core/Cargo.toml")).unwrap();
    assert!(!core_toml.contains("helios"), "ferrata-core must not depend on helios");
    let spectests_toml = fs::read_to_string(root_dir.join("crates/ferrata-spectests/Cargo.toml")).unwrap();
    assert!(
        spectests_toml.contains(&format!(
            "helios-consensus-core = {{ git = \"https://github.com/a16z/helios\", rev = \"{HELIOS_REV}\" }}"
        )),
        "exact Helios git rev pin missing from crates/ferrata-spectests/Cargo.toml"
    );

    let source_check = if let Some(dir) = find_helios_checkout() {
        let (count, digest) = hash_helios_crate_tree(&dir);
        assert_eq!(count, 11, "unexpected file count in helios-consensus-core checkout");
        assert_eq!(digest, HELIOS_SOURCE_SHA256, "helios-consensus-core source tree SHA-256 mismatch");
        format!("source sha256={digest} ({count} files)")
    } else {
        "checkout not under CARGO_HOME; Cargo.lock git rev pin checked".to_string()
    };
    println!(
        "pin verified: unmodified helios-consensus-core {HELIOS_VERSION} rev {HELIOS_REV} ({source_check}; BLS: bls12_381 0.8.0 vs Ferrata steps blst 0.3.17)"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. Every recorded real update in the repository (mainnet + Sepolia, all 19 .ssz directories)
// ---------------------------------------------------------------------------------------------

/// Explicit exclusions for tracked `.ssz` files that are intentionally NOT replayable mainnet or
/// Sepolia light-client fixtures (for example synthetic/corrupted fixtures or non-light-client SSZ
/// files). Add `(repo_relative_path, reason)` entries here only when a file cannot be replayed in
/// `every_recorded_real_update_in_repo_agrees_with_helios_on_step_path`.
const NON_REPLAY_SSZ: &[(&str, &str)] = &[
    ("e2e/runs/ci-sequencer-attest-v040/fresh/attest/finality_update.ssz", V040_REHEARSAL),
    ("e2e/runs/ci-sequencer-attest-v040/fresh/checkpoint/bootstrap.ssz", V040_REHEARSAL),
    ("e2e/runs/ci-sequencer-attest-v040/fresh/checkpoint/finality_update.ssz", V040_REHEARSAL),
    ("e2e/runs/ci-sequencer-attest-v040/fresh/checkpoint/update_1425.ssz", V040_REHEARSAL),
];
const V040_REHEARSAL: &str =
    "the LEZ v0.4.0 port's sequencer-attest rehearsal (28 Nov 2026): Sepolia data it fetched, after the differential's corpus and not yet replayed through Helios (scripts/verifylib/checks_lc.py POST_CORPUS)";

fn format_uncovered_ssz_message(uncovered: &[&String]) -> String {
    let list = uncovered.iter().map(|p| format!("  - {p}")).collect::<Vec<_>>().join("\n");
    format!(
        "found {} uncovered .ssz file(s) in the repository working tree:\n\
         {list}\n\
         \n\
         Why this failed:\n\
         `every_recorded_real_update_in_repo_agrees_with_helios_on_step_path` in\n\
         `crates/ferrata-spectests/tests/helios_step_oracle.rs` verifies that every non-vector\n\
         `.ssz` fixture in the repository is replayed through Helios 0.12.0 and Ferrata's\n\
         program step path.\n\
         \n\
         What to do:\n\
         1. New real mainnet/Sepolia light-client bootstrap or update fixture:\n\
         \x20  add a replay block in `every_recorded_real_update_in_repo_agrees_with_helios_on_step_path`\n\
         \x20  in `crates/ferrata-spectests/tests/helios_step_oracle.rs` (using `t.record_bootstrap(...)`\n\
         \x20  and `t.run_update(...)`). Note: files added under\n\
         \x20  `lez/lab/results/record/net/{{mainnet,sepolia}}/updates/` are picked up automatically.\n\
         2. Intentional non-replay `.ssz` file (synthetic, malformed/negative test data, or a\n\
         \x20  non-light-client SSZ object):\n\
         \x20  add `(repo_relative_path, reason)` to `NON_REPLAY_SSZ` right above\n\
         \x20  `collect_all_repo_ssz()` in `crates/ferrata-spectests/tests/helios_step_oracle.rs`.\n\
         3. Untracked local scratch `.ssz` file:\n\
         \x20  remove it or move it outside the repository working tree (this check walks the working\n\
         \x20  tree, skipping only `.git/`, `target/`, and top-level `vectors/`).",
        uncovered.len()
    )
}

fn collect_all_repo_ssz() -> BTreeSet<String> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeSet<String>) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        for entry in rd.flatten() {
            let p = entry.path();
            let name = entry.file_name();
            if name == ".git" || name == "target" || (dir == base && name == "vectors") {
                continue;
            }
            if p.is_dir() {
                walk(base, &p, out);
            } else if p.extension().and_then(|e| e.to_str()) == Some("ssz") {
                out.insert(p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let root_dir = repo_root();
    let mut out = BTreeSet::new();
    walk(&root_dir, &root_dir, &mut out);
    out
}

/// Split a beacon-API SSZ stream (`u64 le_len || 4-byte fork_digest || update_ssz`) into chunks.
fn split_update_stream(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < bytes.len() {
        let len = u64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap()) as usize;
        pos += 8;
        assert!(len > 4 && pos + len <= bytes.len(), "malformed update stream at {pos}");
        out.push(bytes[pos + 4..pos + len].to_vec());
        pos += len;
    }
    out
}

fn layout_from_name(s: &str) -> LcFork {
    match s {
        "electra" => LcFork::Electra,
        "gloas" => LcFork::Gloas,
        other => panic!("unexpected layout {other}"),
    }
}

#[derive(Default, Debug)]
struct RealReplayTally {
    bootstraps: usize,
    mainnet_updates: usize,
    sepolia_updates: usize,
    period_updates: usize,
    finality_updates: usize,
    accepted_by_steps: usize,
    optimistic_only_not_newer: usize,
    mainnet_periods: BTreeSet<u64>,
    sepolia_periods: BTreeSet<u64>,
    unique_update_hashes: BTreeSet<[u8; 32]>,
    covered_files: BTreeSet<String>,
}

impl RealReplayTally {
    fn record_bootstrap(&mut self, rel: &str) {
        self.bootstraps += 1;
        self.covered_files.insert(rel.to_string());
    }

    #[allow(clippy::too_many_arguments)]
    fn run_update(
        &mut self,
        quad: &mut Quad<MainnetConsensusSpec>,
        rel: &str,
        case_suffix: &str,
        bytes: &[u8],
        kind: UpdateKind,
        layout: LcFork,
        is_mainnet: bool,
        commit_if_ok: bool,
    ) {
        let u = decode_ssz_update(bytes, kind, layout, 512).unwrap_or_else(|e| panic!("decode {rel}: {e}"));
        let label = if case_suffix.is_empty() { rel.to_string() } else { format!("{rel}#{case_suffix}") };
        let rep = quad.judge(&label, bytes, kind, layout, u.signature_slot + 1, commit_if_ok, KnownDivergence::None);
        assert!(rep.helios_valid, "{label}: recorded real update must be valid");
        if rep.steps_ok {
            self.accepted_by_steps += 1;
        } else {
            // Valid update that did not move finalized state (e.g. finality_update at bootstrap slot).
            assert_eq!(rep.steps_err, Some(StepError::Core(Error::NotNewer)), "{label}");
            self.optimistic_only_not_newer += 1;
        }
        self.covered_files.insert(rel.to_string());
        self.unique_update_hashes.insert(Sha256::digest(bytes).into());
        if is_mainnet {
            self.mainnet_updates += 1;
            self.mainnet_periods.insert(u.signature_slot / SLOTS_PER_PERIOD);
        } else {
            self.sepolia_updates += 1;
            self.sepolia_periods.insert(u.signature_slot / SLOTS_PER_PERIOD);
        }
        match kind {
            UpdateKind::Period => self.period_updates += 1,
            UpdateKind::Finality => self.finality_updates += 1,
        }
    }
}

#[test]
fn every_recorded_real_update_in_repo_agrees_with_helios_on_step_path() {
    let mut t = RealReplayTally::default();
    let m_spec = mainnet();
    let s_spec = sepolia();
    let m_gvr = root(MAINNET_GVR);
    let s_gvr = root(SEPOLIA_GVR);

    // 1. Mainnet periods 1899-1903 (fixtures/mainnet-p1899)
    {
        let boot_rel = "crates/ferrata-spectests/fixtures/mainnet-p1899/bootstrap.ssz";
        t.record_bootstrap(boot_rel);
        let mut q = Quad::<MainnetConsensusSpec>::from_bootstrap(
            "mainnet-p1899",
            m_spec.clone(),
            m_gvr,
            root("0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac"),
            &read_repo(boot_rel),
            LcFork::Electra,
            2,
        );
        for i in 0..5 {
            let rel = format!("crates/ferrata-spectests/fixtures/mainnet-p1899/update_{i}.ssz");
            t.run_update(&mut q, &rel, "", &read_repo(&rel), UpdateKind::Period, LcFork::Electra, true, true);
        }
    }

    // 2. Mainnet e2e fixture (period 1906)
    {
        let boot_rel = "crates/ferrata-spectests/fixtures/mainnet-e2e-2026-11-09/bootstrap.ssz";
        t.record_bootstrap(boot_rel);
        let mut q = Quad::<MainnetConsensusSpec>::from_bootstrap(
            "mainnet-e2e",
            m_spec.clone(),
            m_gvr,
            root("0xdab6b45fd34b9da264b15ad3e85dad586f836f3b11f5922e4c116b53574aefd3"),
            &read_repo(boot_rel),
            LcFork::Electra,
            2,
        );
        let rel = "crates/ferrata-spectests/fixtures/mainnet-e2e-2026-11-09/finality_update.ssz";
        t.run_update(&mut q, rel, "", &read_repo(rel), UpdateKind::Finality, LcFork::Electra, true, true);
    }

    // 3. Mainnet attest fixture (period 1907)
    {
        let boot_rel = "crates/ferrata-spectests/fixtures/mainnet-attest-2026-11-10/bootstrap.ssz";
        t.record_bootstrap(boot_rel);
        let mut q = Quad::<MainnetConsensusSpec>::from_bootstrap(
            "mainnet-attest",
            m_spec.clone(),
            m_gvr,
            root("0x7c279a5e7aee53514ad2ce373139ff36b50e26563ff53134e46cf57b57382433"),
            &read_repo(boot_rel),
            LcFork::Electra,
            2,
        );
        let rel = "crates/ferrata-spectests/fixtures/mainnet-attest-2026-11-10/finality_update.ssz";
        t.run_update(&mut q, rel, "", &read_repo(rel), UpdateKind::Finality, LcFork::Electra, true, true);
    }

    // 4. Lab mainnet record (all 108 updates, periods 1907-1914) + client-diversity mainnet answers (8 files)
    {
        let boot_rel = "lez/lab/results/record/net/mainnet/bootstrap.ssz";
        t.record_bootstrap(boot_rel);
        let cp = root(&fs::read_to_string(repo_root().join("lez/lab/results/record/net/mainnet/checkpoint")).unwrap());
        let mut q = Quad::<MainnetConsensusSpec>::from_bootstrap(
            "lab-mainnet",
            m_spec.clone(),
            m_gvr,
            cp,
            &read_repo(boot_rel),
            LcFork::Electra,
            2,
        );
        let mut names: Vec<String> = fs::read_dir(repo_root().join("lez/lab/results/record/net/mainnet/updates"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|n| n.ends_with(".ssz"))
            .collect();
        names.sort();
        // Snapshots: before adopting period `p`'s next committee, and just after entering period 1914.
        let mut pre_period_snap: BTreeMap<u64, Quad<MainnetConsensusSpec>> = BTreeMap::new();
        let mut p1914_entry_snap: Option<Quad<MainnetConsensusSpec>> = None;
        for name in &names {
            let parts: Vec<&str> = name.trim_end_matches(".ssz").split('.').collect();
            let kind = if parts[1] == "period" { UpdateKind::Period } else { UpdateKind::Finality };
            let layout = layout_from_name(parts[2]);
            let rel = format!("lez/lab/results/record/net/mainnet/updates/{name}");
            let bytes = read_repo(&rel);
            let u = decode_ssz_update(&bytes, kind, layout, 512).unwrap();
            let u_period = u.signature_slot / SLOTS_PER_PERIOD;
            if kind == UpdateKind::Period {
                pre_period_snap.entry(u_period).or_insert_with(|| q.clone());
            }
            t.run_update(&mut q, &rel, "", &bytes, kind, layout, true, true);
            if p1914_entry_snap.is_none() && q.steps.compact_store().finalized.slot / SLOTS_PER_PERIOD == 1914 {
                p1914_entry_snap = Some(q.clone());
            }
        }
        // Client-diversity probe mainnet period updates (1910-1914) and round0 finality updates (3)
        for p in 1910..=1914u64 {
            let rel =
                format!("lez/updater/results/client-diversity-2026-11-18/probe/mainnet/answers/mainnet-period{p}.ssz");
            let mut snap = pre_period_snap.get(&p).unwrap_or_else(|| panic!("no snapshot for mainnet p{p}")).clone();
            t.run_update(&mut snap, &rel, "", &read_repo(&rel), UpdateKind::Period, LcFork::Electra, true, false);
        }
        let p1914_base = p1914_entry_snap.expect("mainnet reached period 1914");
        for fname in [
            "mainnet-lodestar-mainnet.chainsafe.io-finality-round0.ssz",
            "mainnet-testing.mainnet.beacon-api.nimbus.team-finality-round0.ssz",
            "mainnet-unstable.mainnet.beacon-api.nimbus.team-finality-round0.ssz",
        ] {
            let rel = format!("lez/updater/results/client-diversity-2026-11-18/probe/mainnet/answers/{fname}");
            let mut snap = p1914_base.clone();
            t.run_update(&mut snap, &rel, "", &read_repo(&rel), UpdateKind::Finality, LcFork::Electra, true, false);
        }
    }

    // 5. Sepolia through Gloas fixture (1378-1409 + finality) + sepolia-attest-2026-11-10
    {
        let boot_rel = "crates/ferrata-spectests/fixtures/sepolia-gloas-2026-11-10/bootstrap.ssz";
        t.record_bootstrap(boot_rel);
        let mut q = Quad::<MainnetConsensusSpec>::from_bootstrap(
            "sepolia-gloas",
            s_spec.clone(),
            s_gvr,
            root("0x5f71d7f650f9252b48749b5f062c0c234e01afd69f364ab600e2b6051494b20d"),
            &read_repo(boot_rel),
            LcFork::Electra,
            2,
        );
        for p in 1378..=1409u64 {
            let layout = if p < 1379 { LcFork::Electra } else { LcFork::Gloas };
            let rel = format!("crates/ferrata-spectests/fixtures/sepolia-gloas-2026-11-10/update_{p}.ssz");
            t.run_update(&mut q, &rel, "", &read_repo(&rel), UpdateKind::Period, layout, false, true);
        }
        let fu_rel = "crates/ferrata-spectests/fixtures/sepolia-gloas-2026-11-10/finality_update.ssz";
        t.run_update(&mut q, fu_rel, "", &read_repo(fu_rel), UpdateKind::Finality, LcFork::Gloas, false, true);
        let att_rel = "crates/ferrata-spectests/fixtures/sepolia-attest-2026-11-10/finality_update.ssz";
        t.run_update(&mut q, att_rel, "", &read_repo(att_rel), UpdateKind::Finality, LcFork::Gloas, false, true);
    }

    // 6. Lab Sepolia record (all 138 updates, periods 1378-1417) + inbound/vault-v5 + client-diversity (7 files)
    {
        let boot_rel = "lez/lab/results/record/net/sepolia/bootstrap.ssz";
        t.record_bootstrap(boot_rel);
        let cp = root(&fs::read_to_string(repo_root().join("lez/lab/results/record/net/sepolia/checkpoint")).unwrap());
        let mut q = Quad::<MainnetConsensusSpec>::from_bootstrap(
            "lab-sepolia",
            s_spec.clone(),
            s_gvr,
            cp,
            &read_repo(boot_rel),
            LcFork::Electra,
            2,
        );
        let mut names: Vec<String> = fs::read_dir(repo_root().join("lez/lab/results/record/net/sepolia/updates"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|n| n.ends_with(".ssz"))
            .collect();
        names.sort();
        let mut pre_period_snap: BTreeMap<u64, Quad<MainnetConsensusSpec>> = BTreeMap::new();
        let mut period_entry_snap: BTreeMap<u64, Quad<MainnetConsensusSpec>> = BTreeMap::new();
        for name in &names {
            let parts: Vec<&str> = name.trim_end_matches(".ssz").split('.').collect();
            let kind = if parts[1] == "period" { UpdateKind::Period } else { UpdateKind::Finality };
            let layout = layout_from_name(parts[2]);
            let rel = format!("lez/lab/results/record/net/sepolia/updates/{name}");
            let bytes = read_repo(&rel);
            let u = decode_ssz_update(&bytes, kind, layout, 512).unwrap();
            let u_period = u.signature_slot / SLOTS_PER_PERIOD;
            if kind == UpdateKind::Period {
                pre_period_snap.entry(u_period).or_insert_with(|| q.clone());
            }
            t.run_update(&mut q, &rel, "", &bytes, kind, layout, false, true);
            let cur_p = q.steps.compact_store().finalized.slot / SLOTS_PER_PERIOD;
            period_entry_snap.entry(cur_p).or_insert_with(|| q.clone());
        }
        for rel in [
            "crates/ferrata-spectests/fixtures/sepolia-inbound-2026-11-11-1/finality_update.ssz",
            "crates/ferrata-spectests/fixtures/sepolia-vault-v5-2026-11-11/finality_update.ssz",
        ] {
            let mut snap = period_entry_snap.get(&1410).expect("sepolia reached p1410").clone();
            t.run_update(&mut snap, rel, "", &read_repo(rel), UpdateKind::Finality, LcFork::Gloas, false, false);
        }
        for p in 1412..=1416u64 {
            let rel =
                format!("lez/updater/results/client-diversity-2026-11-18/probe/sepolia/answers/sepolia-period{p}.ssz");
            let mut snap = pre_period_snap.get(&p).unwrap_or_else(|| panic!("no snapshot for sepolia p{p}")).clone();
            t.run_update(&mut snap, &rel, "", &read_repo(&rel), UpdateKind::Period, LcFork::Gloas, false, false);
        }
        let p1416_base = period_entry_snap.get(&1416).expect("sepolia reached p1416");
        for fname in [
            "sepolia-lodestar-sepolia.chainsafe.io-finality-round0.ssz",
            "sepolia-unstable.sepolia.beacon-api.nimbus.team-finality-round0.ssz",
        ] {
            let rel = format!("lez/updater/results/client-diversity-2026-11-18/probe/sepolia/answers/{fname}");
            let mut snap = p1416_base.clone();
            t.run_update(&mut snap, &rel, "", &read_repo(&rel), UpdateKind::Finality, LcFork::Gloas, false, false);
        }
    }

    // 7. Sepolia 2026-11-12 (Gloas bootstrap at p1411, older-than-bootstrap adoption update, same-slot finality)
    {
        let boot_rel = "crates/ferrata-spectests/fixtures/sepolia-2026-11-12/bootstrap.ssz";
        t.record_bootstrap(boot_rel);
        let mut q = Quad::<MainnetConsensusSpec>::from_bootstrap(
            "sepolia-2026-11-12",
            s_spec.clone(),
            s_gvr,
            root("0x3b901f5e6853b5cf5eb04d7ccfe1272e66c38291cb3a11dd159cacd1117ccf7c"),
            &read_repo(boot_rel),
            LcFork::Gloas,
            2,
        );
        let u_rel = "crates/ferrata-spectests/fixtures/sepolia-2026-11-12/update_1411.ssz";
        t.run_update(&mut q, u_rel, "", &read_repo(u_rel), UpdateKind::Period, LcFork::Gloas, false, true);
        let f_rel = "crates/ferrata-spectests/fixtures/sepolia-2026-11-12/finality_update.ssz";
        // Finalized slot equals bootstrap slot (11,562,112): valid optimistically in Helios/spec,
        // refused with NotNewer by step path (exact match with `helios_finalized_moved == false`).
        t.run_update(&mut q, f_rel, "", &read_repo(f_rel), UpdateKind::Finality, LcFork::Gloas, false, false);
    }

    // 8. Sepolia 2026-11-18 (Gloas bootstrap at p1416 + update_1416 + finality)
    {
        let boot_rel = "crates/ferrata-spectests/fixtures/sepolia-2026-11-18/bootstrap.ssz";
        t.record_bootstrap(boot_rel);
        let mut q = Quad::<MainnetConsensusSpec>::from_bootstrap(
            "sepolia-2026-11-18",
            s_spec.clone(),
            s_gvr,
            root("0x3256972bbbec2328a3ef603bde6ffa247e8d1f68fbb1635cc1dc42513ac87ebd"),
            &read_repo(boot_rel),
            LcFork::Gloas,
            2,
        );
        let u_rel = "crates/ferrata-spectests/fixtures/sepolia-2026-11-18/update_1416.ssz";
        t.run_update(&mut q, u_rel, "", &read_repo(u_rel), UpdateKind::Period, LcFork::Gloas, false, true);
        let f_rel = "crates/ferrata-spectests/fixtures/sepolia-2026-11-18/finality_update.ssz";
        t.run_update(&mut q, f_rel, "", &read_repo(f_rel), UpdateKind::Finality, LcFork::Gloas, false, true);
    }

    // 9. CI sequencer run 2026-11-24 (Sepolia p1422 bootstrap + update_1422 + 2 finality updates)
    {
        let boot_rel = "e2e/runs/ci-sequencer-2026-11-24/attest/fresh/checkpoint/bootstrap.ssz";
        t.record_bootstrap(boot_rel);
        let mut q = Quad::<MainnetConsensusSpec>::from_bootstrap(
            "ci-sequencer-2026-11-24",
            s_spec.clone(),
            s_gvr,
            root("0xa5f802a576c15165dbf01b65bfe2b97a2327ea8638dd3a366f880aca1b20d006"),
            &read_repo(boot_rel),
            LcFork::Gloas,
            2,
        );
        let u_rel = "e2e/runs/ci-sequencer-2026-11-24/attest/fresh/checkpoint/update_1422.ssz";
        t.run_update(&mut q, u_rel, "", &read_repo(u_rel), UpdateKind::Period, LcFork::Gloas, false, true);
        let f1_rel = "e2e/runs/ci-sequencer-2026-11-24/attest/fresh/checkpoint/finality_update.ssz";
        t.run_update(&mut q, f1_rel, "", &read_repo(f1_rel), UpdateKind::Finality, LcFork::Gloas, false, true);
        let f2_rel = "e2e/runs/ci-sequencer-2026-11-24/attest/fresh/attest/finality_update.ssz";
        t.run_update(&mut q, f2_rel, "", &read_repo(f2_rel), UpdateKind::Finality, LcFork::Gloas, false, true);
    }

    // 10. Verified proxy session (Sepolia p1411 bootstrap + 3-chunk stream p1411..1413 + finality)
    //     and lc-json-sepolia (bootstrap p1411 + update_1411 + p1413 finality)
    {
        let boot_rel = "modules/results/verified-proxy-2026-11-14/session/plain/nimbus-bootstrap.ssz";
        t.record_bootstrap(boot_rel);
        let mut q = Quad::<MainnetConsensusSpec>::from_bootstrap(
            "verified-proxy",
            s_spec.clone(),
            s_gvr,
            root("0x3b901f5e6853b5cf5eb04d7ccfe1272e66c38291cb3a11dd159cacd1117ccf7c"),
            &read_repo(boot_rel),
            LcFork::Gloas,
            2,
        );
        let stream_rel = "modules/results/verified-proxy-2026-11-14/session/plain/lodestar-updates.ssz";
        let chunks = split_update_stream(&read_repo(stream_rel));
        assert_eq!(chunks.len(), 3, "verified-proxy stream holds 3 period updates (1411..=1413)");
        for (i, chunk) in chunks.iter().enumerate() {
            t.run_update(
                &mut q,
                stream_rel,
                &format!("chunk{i}"),
                chunk,
                UpdateKind::Period,
                LcFork::Gloas,
                false,
                true,
            );
        }
        // Snapshot at period 1413 (before finality at 11577568) also verifies lc-json-sepolia's
        // period-1413 finality_update.ssz (fin=11577184).
        let mut q_lc_json_p1413 = q.clone();
        let vp_fin = "modules/results/verified-proxy-2026-11-14/session/plain/lodestar-finality.ssz";
        t.run_update(&mut q, vp_fin, "", &read_repo(vp_fin), UpdateKind::Finality, LcFork::Gloas, false, true);

        let lcj_boot = "lez/updater/tests/fixtures/lc-json-sepolia/bootstrap.ssz";
        t.record_bootstrap(lcj_boot);
        let mut q_lcj = Quad::<MainnetConsensusSpec>::from_bootstrap(
            "lc-json-sepolia",
            s_spec.clone(),
            s_gvr,
            root("0x3b901f5e6853b5cf5eb04d7ccfe1272e66c38291cb3a11dd159cacd1117ccf7c"),
            &read_repo(lcj_boot),
            LcFork::Gloas,
            2,
        );
        let lcj_u = "lez/updater/tests/fixtures/lc-json-sepolia/update_1411.ssz";
        t.run_update(&mut q_lcj, lcj_u, "", &read_repo(lcj_u), UpdateKind::Period, LcFork::Gloas, false, true);
        let lcj_f = "lez/updater/tests/fixtures/lc-json-sepolia/finality_update.ssz";
        t.run_update(
            &mut q_lc_json_p1413,
            lcj_f,
            "",
            &read_repo(lcj_f),
            UpdateKind::Finality,
            LcFork::Gloas,
            false,
            true,
        );
    }

    // 11 & 12. lez-v03 local and public runs (Sepolia p1416 bootstraps + 2 finality updates each)
    for (label, dir, cp, updates) in [
        (
            "lez-v03-local",
            "lez-v03/results/local",
            "0xa3dbfb9dcb8fde50b3efe4ad407b6305119b78d8534f5996b0e89444bbcba641",
            ["updates/000-finality-11605600.ssz", "updates/001-finality-11605632.ssz"],
        ),
        (
            "lez-v03-public",
            "lez-v03/results/public",
            "0x8ef0115979d741ba168ce2e8d6dbc5efc4f41d571473b624145f9fb8465462c1",
            ["updates/000-finality-11606368.ssz", "updates/001-finality-11606400.ssz"],
        ),
    ] {
        let boot_rel = format!("{dir}/bootstrap.ssz");
        t.record_bootstrap(&boot_rel);
        let mut q = Quad::<MainnetConsensusSpec>::from_bootstrap(
            label,
            s_spec.clone(),
            s_gvr,
            root(cp),
            &read_repo(&boot_rel),
            LcFork::Gloas,
            2,
        );
        for u in updates {
            let rel = format!("{dir}/{u}");
            t.run_update(&mut q, &rel, "", &read_repo(&rel), UpdateKind::Finality, LcFork::Gloas, false, true);
        }
    }

    // Completeness check: EVERY non-vector .ssz file in the repository must be covered (or
    // explicitly excluded with a reason in `NON_REPLAY_SSZ`).
    let all_repo = collect_all_repo_ssz();
    let excluded: BTreeSet<String> = NON_REPLAY_SSZ.iter().map(|(p, _)| (*p).to_string()).collect();
    for (path, _) in NON_REPLAY_SSZ {
        assert!(all_repo.contains(*path), "stale NON_REPLAY_SSZ entry (file no longer exists): {path}");
    }
    let uncovered: Vec<&String> = all_repo.difference(&t.covered_files).filter(|p| !excluded.contains(*p)).collect();
    assert!(uncovered.is_empty(), "{}", format_uncovered_ssz_message(&uncovered));

    let total_updates = t.mainnet_updates + t.sepolia_updates;
    println!(
        "real-update step-path oracle: {} bootstraps, {} updates ({} mainnet, {} Sepolia; {} period, {} finality; \
         {} unique update payloads by sha256; {} applied by step path, {} valid optimistic-only NotNewer; \
         mainnet periods {:?}, Sepolia periods {:?}; {}/{} repo .ssz files covered); 0 disagreements",
        t.bootstraps,
        total_updates,
        t.mainnet_updates,
        t.sepolia_updates,
        t.period_updates,
        t.finality_updates,
        t.unique_update_hashes.len(),
        t.accepted_by_steps,
        t.optimistic_only_not_newer,
        t.mainnet_periods,
        t.sepolia_periods,
        t.covered_files.len(),
        all_repo.len(),
    );
    assert!(total_updates >= 321, "expected at least 321 recorded updates, got {total_updates}");
}

// ---------------------------------------------------------------------------------------------
// 3. Official consensus-specs light_client/sync vectors through Helios + program step path
// ---------------------------------------------------------------------------------------------

fn yaml(path: &Path) -> Value {
    serde_yaml::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn hex32(v: &Value) -> Root {
    hex::decode(v.as_str().unwrap().trim_start_matches("0x")).unwrap().try_into().unwrap()
}

fn digest(v: &Value) -> [u8; 4] {
    hex::decode(v.as_str().unwrap().trim_start_matches("0x")).unwrap().try_into().unwrap()
}

fn version(v: &Value) -> [u8; 4] {
    match v {
        Value::Number(n) => (n.as_u64().unwrap() as u32).to_be_bytes(),
        other => digest(other),
    }
}

#[test]
fn official_sync_vectors_agree_with_helios_on_step_path() {
    let mut compared_cases = 0usize;
    let mut compared_updates = 0usize;
    let mut direct_compared = 0usize;
    let mut direct_applied = 0usize;
    let mut direct_policy_refused = 0usize;
    let mut forced_cases = 0usize;
    let mut post_force_checked = 0usize;
    let mut post_force_applied = 0usize;
    let mut post_force_refused = 0usize;
    let mut self_upgrades = 0usize;

    for (fork, dir) in cases("sync").unwrap() {
        let case_name = format!("{}/{}", fork.name(), dir.file_name().unwrap().to_string_lossy());
        let meta = yaml(&dir.join("meta.yaml"));
        let cfg: CaseConfig = parse_config(&dir.join("config.yaml")).unwrap();
        let spec = &cfg.spec;
        let n = spec.preset.sync_committee_size;
        let gvr = hex32(&meta["genesis_validators_root"]);
        let trusted = hex32(&meta["trusted_block_root"]);
        let lc = |f: Fork| f.lc_fork().unwrap();
        let boot_fork = lc(cfg.fork_for_digest(&gvr, digest(&meta["bootstrap_fork_digest"])).unwrap());
        let store_fork = lc(cfg.fork_for_version(version(&meta["store_fork_version"])).unwrap());
        let boot_raw = read_ssz_snappy(&dir.join("bootstrap.ssz_snappy")).unwrap();
        let Ok(hboot) = HBootstrap::<MinimalConsensusSpec>::from_ssz_bytes(&boot_raw) else {
            // Altair-layout bootstrap Helios cannot decode (19 of 52 cases).
            continue;
        };
        let boot = upgrade_bootstrap(
            LightClientBootstrap::decode(&boot_raw, boot_fork, n).unwrap(),
            boot_fork,
            store_fork,
            spec,
        )
        .unwrap();
        let hforks = forks(spec);
        verify_bootstrap(&hboot, b256(&trusted), &hforks).unwrap();
        let mut hstore = HStore::<MinimalConsensusSpec>::default();
        apply_bootstrap(&mut hstore, &hboot);
        let mut steps = ProgramStepState::initialize(&trusted, &boot, store_fork, spec, 2).unwrap();
        let mut forced_diverged = false;
        let mut helios_seen_finalized: BTreeSet<Root> = BTreeSet::from([helios_full_view(&hstore).finalized.1]);

        for (i, step) in yaml(&dir.join("steps.yaml")).as_sequence().unwrap().iter().enumerate() {
            let (kind, body) = step.as_mapping().and_then(|m| m.iter().next()).unwrap();
            match kind.as_str().unwrap() {
                "process_update" => {
                    let current_slot = body["current_slot"].as_u64().unwrap();
                    let name = body["update"].as_str().unwrap();
                    let bytes = read_ssz_snappy(&dir.join(format!("{name}.ssz_snappy"))).unwrap();
                    let u_fork = lc(cfg.fork_for_digest(&gvr, digest(&body["update_fork_digest"])).unwrap());
                    let hu = helios_decode_update::<MinimalConsensusSpec>(&bytes).unwrap();
                    let h_before = helios_full_view(&hstore);
                    let h_ok = verify_generic_update(&hu, current_slot, &hstore, b256(&gvr), &hforks).is_ok();
                    if h_ok {
                        apply_generic_update(&mut hstore, &hu);
                    }
                    let h_after = helios_full_view(&hstore);
                    helios_seen_finalized.insert(h_after.finalized.1);
                    let h_moved = moved_flags(&h_before.finalized_only(), &h_after.finalized_only(), spec);
                    let h_fin_moved = h_moved.0 || h_moved.1 || h_moved.2;

                    // Program step path takes update in its served layout (or current store layout
                    // if already ahead), self-upgrading via `store_for_update`.
                    let raw_u = LightClientUpdate::decode(&bytes, u_fork, n).unwrap();
                    let target_layout = u_fork.max(steps.compact_store().fork);
                    let u = if target_layout != u_fork {
                        upgrade_update(raw_u, u_fork, target_layout, spec).unwrap()
                    } else {
                        raw_u
                    };
                    if target_layout > steps.compact_store().fork {
                        self_upgrades += 1;
                    }
                    let r_pub = steps.run_public_job(&u, target_layout, current_slot, spec, &gvr);
                    let r_va = steps.run_verify_all(&u, target_layout, current_slot, spec, &gvr);
                    assert_eq!(r_pub.is_ok(), r_va.is_ok(), "{case_name} step {i}: public vs VerifyAll");

                    if !forced_diverged {
                        direct_compared += 1;
                        assert_eq!(
                            r_pub.is_ok(),
                            h_ok && h_fin_moved,
                            "{case_name} step {i}: steps {:?} vs helios ok={h_ok} moved={h_moved:?}",
                            r_pub.as_ref().err()
                        );
                        if let Ok((next_st, out, _)) = r_pub {
                            assert_eq!((out.rotated, out.advanced, out.adopted), h_moved, "{case_name} step {i}");
                            let gloas_from_pre = target_layout >= LcFork::Gloas && u_fork < LcFork::Gloas;
                            assert!(
                                finalized_views_match(&next_st.view(), &h_after.finalized_only(), gloas_from_pre),
                                "{case_name} step {i}: steps {:?} vs helios {:?}",
                                next_st.view(),
                                h_after.finalized_only()
                            );
                            steps = next_st;
                            direct_applied += 1;
                        } else {
                            direct_policy_refused += 1;
                        }
                    } else {
                        post_force_checked += 1;
                        if let Ok((next_st, _, _)) = r_pub {
                            // Even after Helios forces an unfinalized update, the step path must never
                            // accept a header Helios did not finalize.
                            assert!(
                                helios_seen_finalized.contains(&next_st.compact_store().finalized.hash_tree_root()),
                                "{case_name} step {i}: step path finalized a header Helios never finalized"
                            );
                            steps = next_st;
                            post_force_applied += 1;
                        } else {
                            post_force_refused += 1;
                        }
                    }
                    compared_updates += 1;
                }
                "force_update" => {
                    let current_slot = body["current_slot"].as_u64().unwrap();
                    let before = helios_full_view(&hstore).finalized_only();
                    helios_force_update(&mut hstore, current_slot);
                    let after = helios_full_view(&hstore).finalized_only();
                    helios_seen_finalized.insert(after.finalized.1);
                    if after != before {
                        if !forced_diverged {
                            forced_cases += 1;
                        }
                        forced_diverged = true;
                    }
                }
                "upgrade_store" => {
                    // Self-upgrading CompactStore ignores explicit upgrade_store until an update
                    // arrives in the new layout, matching `lez/guests/program/src/main.rs`.
                }
                other => panic!("unknown step {other}"),
            }
        }
        compared_cases += 1;
    }

    println!(
        "consensus-specs step-path oracle: {compared_cases} cases, {compared_updates} updates ({direct_compared} compared \
         verdict & state against Helios [{direct_applied} applied, {direct_policy_refused} policy-refused]; \
         {post_force_checked} post-force_update across {forced_cases} cases checked one way [{post_force_applied} applied, \
         {post_force_refused} refused]; {self_upgrades} in-place fork upgrades); 0 disagreements"
    );
    assert_eq!(compared_cases, 33);
    assert_eq!(compared_updates, 111);
    assert_eq!(direct_compared, 96);
    assert_eq!(forced_cases, 5);
    assert_eq!(post_force_checked, 15);
    assert_eq!(post_force_applied, 0);
}

// ---------------------------------------------------------------------------------------------
// 4. Exhaustive field-by-field mutations (Electra & Gloas, period & finality, plus store/witness/job)
// ---------------------------------------------------------------------------------------------

/// A valid compressed G2 point that is on the curve but NOT in the prime-order subgroup.
fn g2_not_in_subgroup() -> [u8; 96] {
    for x in 1u8..=255 {
        let mut bytes = [0u8; 96];
        bytes[0] = 0x80;
        bytes[47] = x;
        let mut affine = blst_p2_affine::default();
        // SAFETY: valid pointers and sizes.
        let rc = unsafe { blst_p2_uncompress(&mut affine, bytes.as_ptr()) };
        if rc == BLST_ERROR::BLST_SUCCESS && !unsafe { blst_p2_affine_in_g2(&affine) } {
            return bytes;
        }
    }
    panic!("failed to locate a non-subgroup G2 point")
}

/// A compressed G1 point that is not on the BLS12-381 curve.
fn g1_off_curve() -> BlsPubkey {
    for x in 1u8..=255 {
        let mut k = [0u8; 48];
        k[0] = 0x80;
        k[47] = x;
        if blst::min_pk::PublicKey::uncompress(&k).is_err() {
            return k;
        }
    }
    panic!("failed to locate an off-curve G1 encoding")
}

fn identity_g1() -> BlsPubkey {
    let mut k = [0u8; 48];
    k[0] = 0xc0;
    k
}

fn identity_g2() -> [u8; 96] {
    let mut s = [0u8; 96];
    s[0] = 0xc0;
    s
}

fn pick_beacon(u: &mut LightClientUpdate, is_attested: bool) -> &mut BeaconBlockHeader {
    if is_attested {
        &mut u.attested_header.beacon
    } else {
        &mut u.finalized_header.beacon
    }
}

fn pick_electra_payload(u: &mut LightClientUpdate, is_attested: bool) -> &mut ExecutionPayloadHeader {
    let h = if is_attested { &mut u.attested_header } else { &mut u.finalized_header };
    match &mut h.execution {
        Execution::Payload { header, .. } => header,
        _ => panic!("expected Electra payload"),
    }
}

fn pick_electra_branch(u: &mut LightClientUpdate, is_attested: bool) -> &mut Vec<Root> {
    let h = if is_attested { &mut u.attested_header } else { &mut u.finalized_header };
    match &mut h.execution {
        Execution::Payload { branch, .. } => branch,
        _ => panic!("expected Electra payload"),
    }
}

fn pick_gloas_hash(u: &mut LightClientUpdate, is_attested: bool) -> &mut Root {
    let h = if is_attested { &mut u.attested_header } else { &mut u.finalized_header };
    match &mut h.execution {
        Execution::BlockHash { block_hash, .. } => block_hash,
        _ => panic!("expected Gloas block hash"),
    }
}

fn pick_gloas_branch(u: &mut LightClientUpdate, is_attested: bool) -> &mut Vec<Root> {
    let h = if is_attested { &mut u.attested_header } else { &mut u.finalized_header };
    match &mut h.execution {
        Execution::BlockHash { branch, .. } => branch,
        _ => panic!("expected Gloas block hash"),
    }
}

type UpdateMutator = (String, Box<dyn Fn(&mut LightClientUpdate)>);
type BootstrapMutator = (&'static str, Root, Box<dyn Fn(&mut LightClientBootstrap)>);

fn push_beacon_mutations(prefix: &'static str, is_attested: bool, out: &mut Vec<UpdateMutator>) {
    out.push((format!("{prefix}.beacon.slot +1"), Box::new(move |u| pick_beacon(u, is_attested).slot += 1)));
    out.push((format!("{prefix}.beacon.slot -1"), Box::new(move |u| pick_beacon(u, is_attested).slot -= 1)));
    out.push((
        format!("{prefix}.beacon.slot u64::MAX"),
        Box::new(move |u| pick_beacon(u, is_attested).slot = u64::MAX),
    ));
    out.push((
        format!("{prefix}.beacon.proposer_index flip"),
        Box::new(move |u| pick_beacon(u, is_attested).proposer_index ^= 1),
    ));
    out.push((
        format!("{prefix}.beacon.proposer_index u64::MAX"),
        Box::new(move |u| pick_beacon(u, is_attested).proposer_index = u64::MAX),
    ));
    out.push((
        format!("{prefix}.beacon.parent_root[0]"),
        Box::new(move |u| pick_beacon(u, is_attested).parent_root[0] ^= 0x01),
    ));
    out.push((
        format!("{prefix}.beacon.parent_root[31]"),
        Box::new(move |u| pick_beacon(u, is_attested).parent_root[31] ^= 0x80),
    ));
    out.push((
        format!("{prefix}.beacon.state_root[0]"),
        Box::new(move |u| pick_beacon(u, is_attested).state_root[0] ^= 0x01),
    ));
    out.push((
        format!("{prefix}.beacon.state_root zero"),
        Box::new(move |u| pick_beacon(u, is_attested).state_root = ZERO),
    ));
    out.push((
        format!("{prefix}.beacon.body_root[0]"),
        Box::new(move |u| pick_beacon(u, is_attested).body_root[0] ^= 0x01),
    ));
    out.push((
        format!("{prefix}.beacon.body_root zero"),
        Box::new(move |u| pick_beacon(u, is_attested).body_root = ZERO),
    ));
}

fn push_electra_exec_mutations(prefix: &'static str, is_attested: bool, out: &mut Vec<UpdateMutator>) {
    out.push((
        format!("{prefix}.exec.parent_hash"),
        Box::new(move |u| pick_electra_payload(u, is_attested).parent_hash[0] ^= 1),
    ));
    out.push((
        format!("{prefix}.exec.fee_recipient"),
        Box::new(move |u| pick_electra_payload(u, is_attested).fee_recipient[0] ^= 1),
    ));
    out.push((
        format!("{prefix}.exec.state_root"),
        Box::new(move |u| pick_electra_payload(u, is_attested).state_root[15] ^= 1),
    ));
    out.push((
        format!("{prefix}.exec.receipts_root"),
        Box::new(move |u| pick_electra_payload(u, is_attested).receipts_root[31] ^= 1),
    ));
    out.push((
        format!("{prefix}.exec.logs_bloom[0]"),
        Box::new(move |u| pick_electra_payload(u, is_attested).logs_bloom[0] ^= 1),
    ));
    out.push((
        format!("{prefix}.exec.logs_bloom[255]"),
        Box::new(move |u| pick_electra_payload(u, is_attested).logs_bloom[255] ^= 1),
    ));
    out.push((
        format!("{prefix}.exec.prev_randao"),
        Box::new(move |u| pick_electra_payload(u, is_attested).prev_randao[0] ^= 1),
    ));
    out.push((
        format!("{prefix}.exec.block_number"),
        Box::new(move |u| pick_electra_payload(u, is_attested).block_number ^= 1),
    ));
    out.push((
        format!("{prefix}.exec.gas_limit"),
        Box::new(move |u| pick_electra_payload(u, is_attested).gas_limit ^= 1),
    ));
    out.push((
        format!("{prefix}.exec.gas_used"),
        Box::new(move |u| pick_electra_payload(u, is_attested).gas_used ^= 1),
    ));
    out.push((
        format!("{prefix}.exec.timestamp"),
        Box::new(move |u| pick_electra_payload(u, is_attested).timestamp ^= 1),
    ));
    out.push((
        format!("{prefix}.exec.extra_data flip/push"),
        Box::new(move |u| {
            let e = &mut pick_electra_payload(u, is_attested).extra_data;
            if e.is_empty() {
                e.push(0x42);
            } else {
                e[0] ^= 1;
            }
        }),
    ));
    out.push((
        format!("{prefix}.exec.extra_data >32B"),
        Box::new(move |u| pick_electra_payload(u, is_attested).extra_data = vec![0xaa; 33]),
    ));
    out.push((
        format!("{prefix}.exec.base_fee_per_gas"),
        Box::new(move |u| pick_electra_payload(u, is_attested).base_fee_per_gas[0] ^= 1),
    ));
    out.push((
        format!("{prefix}.exec.block_hash"),
        Box::new(move |u| pick_electra_payload(u, is_attested).block_hash[0] ^= 1),
    ));
    out.push((
        format!("{prefix}.exec.transactions_root"),
        Box::new(move |u| pick_electra_payload(u, is_attested).transactions_root[0] ^= 1),
    ));
    out.push((
        format!("{prefix}.exec.withdrawals_root"),
        Box::new(move |u| pick_electra_payload(u, is_attested).withdrawals_root[0] ^= 1),
    ));
    out.push((
        format!("{prefix}.exec.blob_gas_used"),
        Box::new(move |u| pick_electra_payload(u, is_attested).blob_gas_used ^= 1),
    ));
    out.push((
        format!("{prefix}.exec.excess_blob_gas"),
        Box::new(move |u| pick_electra_payload(u, is_attested).excess_blob_gas ^= 1),
    ));
    for idx in 0..4 {
        out.push((
            format!("{prefix}.exec.branch[{idx}]"),
            Box::new(move |u| pick_electra_branch(u, is_attested)[idx][0] ^= 1),
        ));
    }
    out.push((format!("{prefix}.exec.branch zero"), Box::new(move |u| pick_electra_branch(u, is_attested).fill(ZERO))));
}

fn push_gloas_exec_mutations(prefix: &'static str, is_attested: bool, out: &mut Vec<UpdateMutator>) {
    out.push((format!("{prefix}.exec.block_hash flip"), Box::new(move |u| pick_gloas_hash(u, is_attested)[0] ^= 1)));
    out.push((format!("{prefix}.exec.block_hash zero"), Box::new(move |u| *pick_gloas_hash(u, is_attested) = ZERO)));
    for idx in 0..11 {
        out.push((
            format!("{prefix}.exec.branch[{idx}]"),
            Box::new(move |u| pick_gloas_branch(u, is_attested)[idx][0] ^= 1),
        ));
    }
    out.push((format!("{prefix}.exec.branch zero"), Box::new(move |u| pick_gloas_branch(u, is_attested).fill(ZERO))));
}

#[derive(Default, Debug)]
struct FieldSuiteTally {
    total: usize,
    accepted_by_both: usize,
    refused_by_both: usize,
    helios_optimistic_only: usize,
    helios_872_pins: usize,
}

impl FieldSuiteTally {
    fn add(&mut self, other: Self) {
        self.total += other.total;
        self.accepted_by_both += other.accepted_by_both;
        self.refused_by_both += other.refused_by_both;
        self.helios_optimistic_only += other.helios_optimistic_only;
        self.helios_872_pins += other.helios_872_pins;
    }

    fn record_normal(&mut self, rep: &StepReport) {
        self.total += 1;
        match (rep.helios_valid, rep.steps_ok) {
            (true, true) => self.accepted_by_both += 1,
            (false, false) => self.refused_by_both += 1,
            (true, false) => self.helios_optimistic_only += 1,
            (false, true) => unreachable!("judge() forbids steps_ok without helios_valid"),
        }
    }
}

fn run_field_suite(
    suite_name: &str,
    base: &Quad<MainnetConsensusSpec>,
    original_bytes: &[u8],
    kind: UpdateKind,
    layout: LcFork,
    other_valid_sig: [u8; 96],
) -> FieldSuiteTally {
    let orig = decode_ssz_update(original_bytes, kind, layout, 512).unwrap();
    // Verify encoder reproduces exact real bytes before mutating!
    assert_eq!(encode_update(&orig, kind, layout), original_bytes, "{suite_name}: encoder roundtrip");

    let mut cases: Vec<UpdateMutator> = Vec::new();
    push_beacon_mutations("attested", true, &mut cases);
    push_beacon_mutations("finalized", false, &mut cases);
    match layout {
        LcFork::Electra => {
            push_electra_exec_mutations("attested", true, &mut cases);
            push_electra_exec_mutations("finalized", false, &mut cases);
        }
        LcFork::Gloas => {
            push_gloas_exec_mutations("attested", true, &mut cases);
            push_gloas_exec_mutations("finalized", false, &mut cases);
        }
        _ => unreachable!(),
    }

    // Cross-header slot relations
    cases.push((
        "attested.slot == finalized.slot".into(),
        Box::new(|u| u.attested_header.beacon.slot = u.finalized_header.beacon.slot),
    ));
    cases.push((
        "attested.slot == finalized.slot - 1".into(),
        Box::new(|u| u.attested_header.beacon.slot = u.finalized_header.beacon.slot - 1),
    ));
    cases.push((
        "finalized.slot == attested.slot + 1".into(),
        Box::new(|u| u.finalized_header.beacon.slot = u.attested_header.beacon.slot + 1),
    ));

    // Finality branch: every node + zeroed branch (with non-empty header) + both emptied (optimistic only)
    let fin_len = layout.finality_branch_len();
    for idx in 0..fin_len {
        cases.push((format!("finality_branch[{idx}]"), Box::new(move |u| u.finality_branch[idx][idx % 32] ^= 1)));
    }
    cases.push(("finality_branch zeroed (header non-empty)".into(), Box::new(|u| u.finality_branch.fill(ZERO))));
    cases.push((
        "optimistic-only (finalized_header & branch zeroed)".into(),
        Box::new(move |u| {
            u.finalized_header = LightClientHeader::empty(layout);
            u.finality_branch.fill(ZERO);
        }),
    ));

    // Next sync committee & branch (for period updates)
    if kind == UpdateKind::Period {
        let sc_len = layout.sync_committee_branch_len();
        cases.push(("next_committee.pubkeys[0] flip".into(), Box::new(|u| u.next_sync_committee.pubkeys[0][10] ^= 1)));
        cases.push((
            "next_committee.pubkeys[255] flip".into(),
            Box::new(|u| u.next_sync_committee.pubkeys[255][47] ^= 1),
        ));
        cases.push((
            "next_committee.pubkeys[511] flip".into(),
            Box::new(|u| u.next_sync_committee.pubkeys[511][0] ^= 1),
        ));
        cases.push((
            "next_committee.pubkeys[3] identity".into(),
            Box::new(|u| u.next_sync_committee.pubkeys[3] = identity_g1()),
        ));
        let off = g1_off_curve();
        cases.push((
            "next_committee.pubkeys[4] off-curve".into(),
            Box::new(move |u| u.next_sync_committee.pubkeys[4] = off),
        ));
        cases.push(("next_committee.pubkeys[5] zero".into(), Box::new(|u| u.next_sync_committee.pubkeys[5] = [0; 48])));
        cases.push((
            "next_committee.aggregate_pubkey flip".into(),
            Box::new(|u| u.next_sync_committee.aggregate_pubkey[0] ^= 1),
        ));
        cases.push((
            "next_committee.aggregate_pubkey identity".into(),
            Box::new(|u| u.next_sync_committee.aggregate_pubkey = identity_g1()),
        ));
        cases.push((
            "next_committee.aggregate_pubkey zero".into(),
            Box::new(|u| u.next_sync_committee.aggregate_pubkey = [0; 48]),
        ));
        for idx in 0..sc_len {
            cases.push((
                format!("next_sync_committee_branch[{idx}]"),
                Box::new(move |u| u.next_sync_committee_branch[idx][0] ^= 1),
            ));
        }
        cases.push((
            "next_committee zeroed (branch kept)".into(),
            Box::new(|u| u.next_sync_committee = SyncCommittee::empty(512)),
        ));
        cases.push((
            "next_committee_branch zeroed (committee kept)".into(),
            Box::new(|u| u.next_sync_committee_branch.fill(ZERO)),
        ));
        cases.push((
            "stripped to finality update (committee & branch zeroed)".into(),
            Box::new(|u| {
                u.next_sync_committee = SyncCommittee::empty(512);
                u.next_sync_committee_branch.fill(ZERO);
            }),
        ));
    }

    // Sync aggregate bits & signature
    let first_signer = orig.sync_aggregate.bits.iter().position(|b| *b).unwrap();
    let first_absent = orig.sync_aggregate.bits.iter().position(|b| !*b);
    cases.push(("sync_bits clear one signer".into(), Box::new(move |u| u.sync_aggregate.bits[first_signer] = false)));
    if let Some(a) = first_absent {
        cases.push(("sync_bits set one non-signer".into(), Box::new(move |u| u.sync_aggregate.bits[a] = true)));
    }
    cases.push(("sync_bits all zero (0 signers)".into(), Box::new(|u| u.sync_aggregate.bits.fill(false))));
    cases.push(("sync_bits all set (512 signers)".into(), Box::new(|u| u.sync_aggregate.bits.fill(true))));
    cases.push((
        "sync_bits exactly 341 of 512 (below 2/3)".into(),
        Box::new(|u| {
            u.sync_aggregate.bits = (0..512).map(|i| i < 341).collect();
        }),
    ));
    cases.push((
        "sync_bits exactly 342 of 512 (exact 2/3)".into(),
        Box::new(|u| {
            u.sync_aggregate.bits = (0..512).map(|i| i < 342).collect();
        }),
    ));

    let bad_sub = g2_not_in_subgroup();
    cases.push(("signature[0] clear compression flag".into(), Box::new(|u| u.sync_aggregate.signature[0] &= 0x7f)));
    cases.push(("signature[0] flip sign bit".into(), Box::new(|u| u.sync_aggregate.signature[0] ^= 0x20)));
    cases.push(("signature[0] set infinity flag".into(), Box::new(|u| u.sync_aggregate.signature[0] |= 0x40)));
    cases.push(("signature[48] flip".into(), Box::new(|u| u.sync_aggregate.signature[48] ^= 0x01)));
    cases.push(("signature[95] flip".into(), Box::new(|u| u.sync_aggregate.signature[95] ^= 0x80)));
    cases.push(("signature all zero".into(), Box::new(|u| u.sync_aggregate.signature = [0; 96])));
    cases.push(("signature G2 infinity".into(), Box::new(|u| u.sync_aggregate.signature = identity_g2())));
    cases.push(("signature G2 not in subgroup".into(), Box::new(move |u| u.sync_aggregate.signature = bad_sub)));
    cases.push((
        "signature from another real update".into(),
        Box::new(move |u| u.sync_aggregate.signature = other_valid_sig),
    ));

    let orig_current = orig.signature_slot + 1;
    let mut tally = FieldSuiteTally::default();

    for (name, mutator) in &cases {
        let mut u = orig.clone();
        mutator(&mut u);
        let bytes = encode_update(&u, kind, layout);
        let mut q = base.clone();
        let rep =
            q.judge(&format!("{suite_name}::{name}"), &bytes, kind, layout, orig_current, false, KnownDivergence::None);
        tally.record_normal(&rep);
    }

    // Slot, current_slot, and period edges (evaluated at specific `current_slot` values!)
    let sig_p = orig.signature_slot / SLOTS_PER_PERIOD;
    let period_last = (sig_p + 1) * SLOTS_PER_PERIOD - 1;
    let next_period_first = (sig_p + 1) * SLOTS_PER_PERIOD;
    let plus2_period_first = (sig_p + 2) * SLOTS_PER_PERIOD;
    let slot_edges: Vec<(&str, u64, u64)> = vec![
        ("sig_slot=orig at current=orig (exact valid edge)", orig.signature_slot, orig.signature_slot),
        ("sig_slot=orig at current=orig-1 (future by 1 slot)", orig.signature_slot, orig.signature_slot - 1),
        ("sig_slot=orig+1 at fixed current=orig (future)", orig.signature_slot + 1, orig.signature_slot),
        ("sig_slot=attested (must be strictly greater)", orig.attested_header.beacon.slot, orig_current),
        ("sig_slot=attested-1", orig.attested_header.beacon.slot - 1, orig_current),
        ("sig_slot=attested+1 (valid edge)", orig.attested_header.beacon.slot + 1, orig_current),
        ("sig_slot=0", 0, orig_current),
        ("sig_slot=last_slot_of_period at current=sig", period_last, period_last),
        ("sig_slot=first_slot_of_next_period at current=sig", next_period_first, next_period_first),
        ("sig_slot=first_slot_of_period+2 at current=sig", plus2_period_first, plus2_period_first),
        ("sig_slot=u64::MAX at fixed current", u64::MAX, orig_current),
        ("sig_slot=u64::MAX at current=u64::MAX", u64::MAX, u64::MAX),
    ];
    for (name, sig_slot, cur_slot) in slot_edges {
        let mut u = orig.clone();
        u.signature_slot = sig_slot;
        let bytes = encode_update(&u, kind, layout);
        let mut q = base.clone();
        let rep =
            q.judge(&format!("{suite_name}::{name}"), &bytes, kind, layout, cur_slot, false, KnownDivergence::None);
        tally.record_normal(&rep);
    }

    // Pin a16z/helios#872 on BOTH Electra and Gloas, period and finality:
    // finalized_header emptied or slot set to 0 while finality_branch is kept non-zero.
    for (name, mutate_fn) in [
        (
            "helios#872 finalized_header emptied (branch kept)",
            Box::new(move |u: &mut LightClientUpdate| u.finalized_header = LightClientHeader::empty(layout))
                as Box<dyn Fn(&mut LightClientUpdate)>,
        ),
        (
            "helios#872 finalized_header.slot=0 (branch kept)",
            Box::new(|u: &mut LightClientUpdate| u.finalized_header.beacon.slot = 0),
        ),
    ] {
        let mut u = orig.clone();
        mutate_fn(&mut u);
        let bytes = encode_update(&u, kind, layout);
        let mut q = base.clone();
        q.judge(
            &format!("{suite_name}::{name}"),
            &bytes,
            kind,
            layout,
            orig_current,
            false,
            KnownDivergence::Helios872EmptyFinalizedWithBranch,
        );
        tally.total += 1;
        tally.helios_872_pins += 1;
    }

    // Raw SSZ framing mutations (host-side decoders): truncated, trailing byte, empty, and
    // (for Electra) bad offsets.
    let mut wire_cases: Vec<(&str, Vec<u8>)> = vec![
        ("ssz truncated 1B", original_bytes[..original_bytes.len() - 1].to_vec()),
        ("ssz trailing 1B", [original_bytes, &[0x00]].concat()),
        ("ssz empty", Vec::new()),
    ];
    if layout == LcFork::Electra {
        let mut bad_att = original_bytes.to_vec();
        bad_att[0] ^= 0x04;
        wire_cases.push(("ssz electra bad attested offset", bad_att));
        let fin_off_pos = match kind {
            UpdateKind::Period => 4 + 513 * 48 + 6 * 32,
            UpdateKind::Finality => 4,
        };
        let mut bad_fin = original_bytes.to_vec();
        bad_fin[fin_off_pos] ^= 0x04;
        wire_cases.push(("ssz electra bad finalized offset", bad_fin));
    }
    for (name, bytes) in wire_cases {
        let mut q = base.clone();
        let rep =
            q.judge(&format!("{suite_name}::{name}"), &bytes, kind, layout, orig_current, false, KnownDivergence::None);
        assert!(!rep.steps_ok && !rep.helios_valid, "{suite_name}::{name} must be refused");
        tally.record_normal(&rep);
    }

    tally
}

#[test]
fn every_field_mutation_agrees_with_helios_on_step_path() {
    let m_spec = mainnet();
    let s_spec = sepolia();
    let m_gvr = root(MAINNET_GVR);
    let s_gvr = root(SEPOLIA_GVR);

    // Other real signatures (valid G2 subgroup points over other headers)
    let other_mainnet_sig = LightClientUpdate::decode(
        &read_repo("crates/ferrata-spectests/fixtures/mainnet-p1899/update_2.ssz"),
        LcFork::Electra,
        512,
    )
    .unwrap()
    .sync_aggregate
    .signature;
    let other_sepolia_sig = LightClientUpdate::decode(
        &read_repo("crates/ferrata-spectests/fixtures/sepolia-gloas-2026-11-10/update_1380.ssz"),
        LcFork::Gloas,
        512,
    )
    .unwrap()
    .sync_aggregate
    .signature;

    let mut tally = FieldSuiteTally::default();

    // A. Mainnet Electra LightClientUpdate (period 1900 after update_0, so store knows next committee)
    let m_boot_bytes = read_repo("crates/ferrata-spectests/fixtures/mainnet-p1899/bootstrap.ssz");
    let m_trusted = root("0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac");
    let mut q_m = Quad::<MainnetConsensusSpec>::from_bootstrap(
        "mainnet-p1899",
        m_spec.clone(),
        m_gvr,
        m_trusted,
        &m_boot_bytes,
        LcFork::Electra,
        2,
    );
    let u0_bytes = read_repo("crates/ferrata-spectests/fixtures/mainnet-p1899/update_0.ssz");
    let u0 = LightClientUpdate::decode(&u0_bytes, LcFork::Electra, 512).unwrap();
    q_m.judge(
        "setup-m-u0",
        &u0_bytes,
        UpdateKind::Period,
        LcFork::Electra,
        u0.signature_slot + 1,
        true,
        KnownDivergence::None,
    );
    let u1_bytes = read_repo("crates/ferrata-spectests/fixtures/mainnet-p1899/update_1.ssz");
    tally.add(run_field_suite(
        "electra-period",
        &q_m,
        &u1_bytes,
        UpdateKind::Period,
        LcFork::Electra,
        other_mainnet_sig,
    ));

    // B. Mainnet Electra LightClientFinalityUpdate (mainnet-attest-2026-11-10)
    let q_mf = Quad::<MainnetConsensusSpec>::from_bootstrap(
        "mainnet-attest",
        m_spec.clone(),
        m_gvr,
        root("0x7c279a5e7aee53514ad2ce373139ff36b50e26563ff53134e46cf57b57382433"),
        &read_repo("crates/ferrata-spectests/fixtures/mainnet-attest-2026-11-10/bootstrap.ssz"),
        LcFork::Electra,
        2,
    );
    let mf_bytes = read_repo("crates/ferrata-spectests/fixtures/mainnet-attest-2026-11-10/finality_update.ssz");
    tally.add(run_field_suite(
        "electra-finality",
        &q_mf,
        &mf_bytes,
        UpdateKind::Finality,
        LcFork::Electra,
        other_mainnet_sig,
    ));

    // C. Sepolia Gloas LightClientUpdate RIGHT ACROSS THE ELECTRA->GLOAS FORK UPGRADE (period 1379!)
    //    Notice `q_s_fork.steps` is still in Electra layout, so EVERY case tests `steps::store_for_update`!
    let mut q_s_fork = Quad::<MainnetConsensusSpec>::from_bootstrap(
        "sepolia-fork",
        s_spec.clone(),
        s_gvr,
        root("0x5f71d7f650f9252b48749b5f062c0c234e01afd69f364ab600e2b6051494b20d"),
        &read_repo("crates/ferrata-spectests/fixtures/sepolia-gloas-2026-11-10/bootstrap.ssz"),
        LcFork::Electra,
        2,
    );
    let s1378_bytes = read_repo("crates/ferrata-spectests/fixtures/sepolia-gloas-2026-11-10/update_1378.ssz");
    let s1378 = LightClientUpdate::decode(&s1378_bytes, LcFork::Electra, 512).unwrap();
    q_s_fork.judge(
        "setup-s-1378",
        &s1378_bytes,
        UpdateKind::Period,
        LcFork::Electra,
        s1378.signature_slot + 1,
        true,
        KnownDivergence::None,
    );
    assert_eq!(q_s_fork.steps.compact_store().fork, LcFork::Electra);
    let s1379_bytes = read_repo("crates/ferrata-spectests/fixtures/sepolia-gloas-2026-11-10/update_1379.ssz");
    tally.add(run_field_suite(
        "gloas-period-fork-upgrade",
        &q_s_fork,
        &s1379_bytes,
        UpdateKind::Period,
        LcFork::Gloas,
        other_sepolia_sig,
    ));

    // Exact Sepolia Electra->Gloas fork-slot boundary on update_1378 (slot 11_296_768 = epoch 353_024 * 32):
    // sig_slot = 11_296_767 (fork_version_slot 11_296_766, period 1378, Fulu) -> valid!
    // sig_slot = 11_296_768 (fork_version_slot 11_296_767, period 1379, Fulu) -> PeriodSkipped (next unknown)!
    {
        let q_pre_fork = Quad::<MainnetConsensusSpec>::from_bootstrap(
            "sepolia-pre-fork",
            s_spec.clone(),
            s_gvr,
            root("0x5f71d7f650f9252b48749b5f062c0c234e01afd69f364ab600e2b6051494b20d"),
            &read_repo("crates/ferrata-spectests/fixtures/sepolia-gloas-2026-11-10/bootstrap.ssz"),
            LcFork::Electra,
            2,
        );
        for (label, sig_slot, want_ok) in [
            ("sepolia-fork-edge-11296767", 11_296_767u64, true),
            ("sepolia-fork-edge-11296768", 11_296_768u64, false),
            ("sepolia-fork-edge-11296769", 11_296_769u64, false),
        ] {
            let mut u = s1378.clone();
            u.signature_slot = sig_slot;
            let bytes = encode_update(&u, UpdateKind::Period, LcFork::Electra);
            let mut q = q_pre_fork.clone();
            let rep =
                q.judge(label, &bytes, UpdateKind::Period, LcFork::Electra, sig_slot, false, KnownDivergence::None);
            assert_eq!(rep.steps_ok, want_ok, "{label}");
            tally.record_normal(&rep);
        }
    }

    // D. Sepolia Gloas LightClientFinalityUpdate (sepolia-2026-11-18)
    let q_sf = Quad::<MainnetConsensusSpec>::from_bootstrap(
        "sepolia-2026-11-18",
        s_spec.clone(),
        s_gvr,
        root("0x3256972bbbec2328a3ef603bde6ffa247e8d1f68fbb1635cc1dc42513ac87ebd"),
        &read_repo("crates/ferrata-spectests/fixtures/sepolia-2026-11-18/bootstrap.ssz"),
        LcFork::Gloas,
        2,
    );
    let sf_bytes = read_repo("crates/ferrata-spectests/fixtures/sepolia-2026-11-18/finality_update.ssz");
    tally.add(run_field_suite(
        "gloas-finality",
        &q_sf,
        &sf_bytes,
        UpdateKind::Finality,
        LcFork::Gloas,
        other_sepolia_sig,
    ));

    // E. Bootstrap mutations (Helios verify_bootstrap vs steps::initialize + adopt + apply)
    let mut diff_non_update = 0usize;
    let mut step_only_refusals = 0usize;
    for (label, boot_raw, trusted, spec, layout) in [
        ("mainnet-electra-boot", &m_boot_bytes, m_trusted, &m_spec, LcFork::Electra),
        (
            "sepolia-gloas-boot",
            &read_repo("crates/ferrata-spectests/fixtures/sepolia-2026-11-18/bootstrap.ssz"),
            root("0x3256972bbbec2328a3ef603bde6ffa247e8d1f68fbb1635cc1dc42513ac87ebd"),
            &s_spec,
            LcFork::Gloas,
        ),
    ] {
        let orig_b = LightClientBootstrap::decode(boot_raw, layout, 512).unwrap();
        assert_eq!(encode_bootstrap(&orig_b, layout), *boot_raw, "{label} roundtrip");
        let hforks = forks(spec);
        let boot_muts: Vec<BootstrapMutator> = vec![
            (
                "wrong trusted checkpoint",
                {
                    let mut r = trusted;
                    r[0] ^= 1;
                    r
                },
                Box::new(|_| {}),
            ),
            ("header.beacon.slot flip", trusted, Box::new(|b| b.header.beacon.slot ^= 1)),
            ("header.beacon.state_root flip", trusted, Box::new(|b| b.header.beacon.state_root[0] ^= 1)),
            ("header.beacon.body_root flip", trusted, Box::new(|b| b.header.beacon.body_root[0] ^= 1)),
            ("committee key flip", trusted, Box::new(|b| b.current_sync_committee.pubkeys[10][0] ^= 1)),
            ("committee aggregate flip", trusted, Box::new(|b| b.current_sync_committee.aggregate_pubkey[0] ^= 1)),
            ("committee branch[0] flip", trusted, Box::new(|b| b.current_sync_committee_branch[0][0] ^= 1)),
        ];
        for (mname, tr, mutate) in boot_muts {
            let mut b = orig_b.clone();
            mutate(&mut b);
            let encoded = encode_bootstrap(&b, layout);
            let helios_ok = HBootstrap::<MainnetConsensusSpec>::from_ssz_bytes(&encoded)
                .ok()
                .is_some_and(|hb| verify_bootstrap(&hb, b256(&tr), &hforks).is_ok());
            let steps_ok = ProgramStepState::initialize(&tr, &b, layout, spec, 2).is_ok();
            assert!(!helios_ok && !steps_ok, "{label}::{mname}: helios={helios_ok} steps={steps_ok}");
            diff_non_update += 1;
        }
    }

    // F. Store-state mutations (not just update bytes!): wrong committee, missing next committee,
    //    wrong genesis_validators_root, wrong store fork layout.
    {
        // Wrong committee in store (period 1900 committee placed as current for period 1899)
        let mut q_wrong = Quad::<MainnetConsensusSpec>::from_bootstrap(
            "wrong-store-committee",
            m_spec.clone(),
            m_gvr,
            m_trusted,
            &m_boot_bytes,
            LcFork::Electra,
            2,
        );
        let wrong_comm = u0.next_sync_committee.clone();
        let mut s = q_wrong.steps.compact_store();
        s.current = CommitteeSummary::of(&wrong_comm);
        q_wrong.steps.store_bytes = s.encode();
        q_wrong.steps.current_committee = wrong_comm.clone();
        let mut core = q_wrong.policy.core().clone();
        core.current_sync_committee = wrong_comm;
        q_wrong.policy = FinalizedStore::from_core(core.clone(), &BLST).unwrap();
        q_wrong.spec_store.core = core;
        q_wrong.helios.current_sync_committee =
            helios_decode_update::<MainnetConsensusSpec>(&u0_bytes).unwrap().next_sync_committee.unwrap();
        let rep = q_wrong.judge(
            "store-wrong-committee",
            &u0_bytes,
            UpdateKind::Period,
            LcFork::Electra,
            u0.signature_slot + 1,
            false,
            KnownDivergence::None,
        );
        assert!(!rep.steps_ok && !rep.helios_valid);
        diff_non_update += 1;

        // Missing next committee in store when update_1 is signed in period 1900
        let mut q_no_next = q_m.clone();
        let mut s = q_no_next.steps.compact_store();
        s.next = None;
        q_no_next.steps.store_bytes = s.encode();
        q_no_next.steps.next_committee = None;
        let mut core = q_no_next.policy.core().clone();
        core.next_sync_committee = SyncCommittee::empty(512);
        q_no_next.policy = FinalizedStore::from_core(core.clone(), &BLST).unwrap();
        q_no_next.spec_store.core = core;
        q_no_next.helios.next_sync_committee = None;
        let u1 = LightClientUpdate::decode(&u1_bytes, LcFork::Electra, 512).unwrap();
        let rep = q_no_next.judge(
            "store-missing-next-committee",
            &u1_bytes,
            UpdateKind::Period,
            LcFork::Electra,
            u1.signature_slot + 1,
            false,
            KnownDivergence::None,
        );
        assert!(!rep.steps_ok && !rep.helios_valid);
        diff_non_update += 1;

        // Wrong genesis_validators_root
        let mut q_gvr = q_m.clone();
        q_gvr.gvr[0] ^= 1;
        let rep = q_gvr.judge(
            "store-wrong-gvr",
            &u1_bytes,
            UpdateKind::Period,
            LcFork::Electra,
            u1.signature_slot + 1,
            false,
            KnownDivergence::None,
        );
        assert!(!rep.steps_ok && !rep.helios_valid);
        diff_non_update += 1;
    }

    // G. Step-path-only wire, witness & job refusals (`CompactUpdate`, `SignerWitness`, `Job`;
    //    no Helios side)
    {
        let u1 = LightClientUpdate::decode(&u1_bytes, LcFork::Electra, 512).unwrap();
        let good_cu = CompactUpdate::of(&u1).encode(LcFork::Electra);
        let good_sw = q_m.steps.build_witness(&u1, &m_spec);
        assert!(!good_sw.keys.is_empty(), "mainnet update_1 has absent signers");
        let good_sw_bytes = good_sw.encode();
        assert!(q_m
            .steps
            .run_public_job_raw(
                &good_cu,
                &good_sw_bytes,
                &u1.next_sync_committee,
                u1.signature_slot + 1,
                &m_spec,
                &m_gvr,
                512
            )
            .is_ok());

        // CompactUpdate layout byte tampered (e.g. Gloas on mainnet where Gloas is unscheduled, or invalid byte)
        for (name, mut_cu) in [
            ("compact-update bad layout byte 99", {
                let mut b = good_cu.clone();
                b[0] = 99;
                b
            }),
            ("compact-update unscheduled Gloas byte 4", {
                let mut b = good_cu.clone();
                b[0] = 4;
                b
            }),
            ("compact-update trailing byte", {
                let mut b = good_cu.clone();
                b.push(0);
                b
            }),
            ("compact-update truncated", good_cu[..good_cu.len() - 1].to_vec()),
        ] {
            assert!(
                q_m.steps
                    .run_public_job_raw(
                        &mut_cu,
                        &good_sw_bytes,
                        &u1.next_sync_committee,
                        u1.signature_slot + 1,
                        &m_spec,
                        &m_gvr,
                        512
                    )
                    .is_err(),
                "{name}"
            );
            step_only_refusals += 1;
        }

        // SignerWitness mutations
        let signer_idx = u1.sync_aggregate.bits.iter().position(|b| *b).unwrap();
        let signing_comm = q_m.steps.next_committee.as_ref().unwrap();
        let mut sw_fewer = good_sw.clone();
        sw_fewer.keys.pop();
        sw_fewer.branches.pop();
        let mut sw_bad_branch = good_sw.clone();
        sw_bad_branch.branches[0][0][0] ^= 1;
        let mut sw_wrong_key = good_sw.clone();
        sw_wrong_key.keys[0] = uncompress_key(&signing_comm.pubkeys[signer_idx]).unwrap();
        let mut sw_identity = good_sw.clone();
        sw_identity.keys[0] = IDENTITY_UNCOMPRESSED;
        let mut sw_compressed = good_sw.clone();
        sw_compressed.keys[0][0] |= 0x80;
        for (name, sw_bytes) in [
            ("witness fewer non-signers", sw_fewer.encode()),
            ("witness bad Merkle branch", sw_bad_branch.encode()),
            ("witness signer key instead of absent", sw_wrong_key.encode()),
            ("witness identity point", sw_identity.encode()),
            ("witness compressed flag set", sw_compressed.encode()),
            ("witness trailing byte", {
                let mut b = good_sw_bytes.clone();
                b.push(0);
                b
            }),
        ] {
            assert!(
                q_m.steps
                    .run_public_job_raw(
                        &good_cu,
                        &sw_bytes,
                        &u1.next_sync_committee,
                        u1.signature_slot + 1,
                        &m_spec,
                        &m_gvr,
                        512
                    )
                    .is_err(),
                "{name}"
            );
            step_only_refusals += 1;
        }

        // Job step-ordering, incomplete adoption, and wrong store_digest refusals
        let stored = q_m.steps.compact_store();
        let cu = CompactUpdate::decode(&good_cu, LcFork::Electra, 512).unwrap();
        let mut job = steps::step1(&stored, &cu, &good_sw, u1.signature_slot + 1, &m_spec, &m_gvr, 2).unwrap();
        assert_eq!(job.clone().step3(), Err(StepError::Phase), "skip Miller");
        assert_eq!(steps::apply(&stored, &job, &m_spec).unwrap_err(), StepError::Phase, "apply before Miller");
        job.step2().unwrap();
        assert_eq!(job.clone().step2(), Err(StepError::Phase), "replay Miller");
        assert!(job.adoption_pending());
        assert_eq!(steps::apply(&stored, &job, &m_spec).unwrap_err(), StepError::Phase, "apply before Finish");
        let keys = uncompressed_keys(&u1.next_sync_committee).unwrap();
        assert_eq!(
            job.clone().adopt(1, &keys[256..], 512),
            Err(StepError::Core(Error::Malformed)),
            "adopt out of order"
        );
        job.adopt(0, &keys[..256], 512).unwrap();
        assert!(job.adoption_pending(), "half adoption still pending");
        let mut job_half = job.clone();
        job_half.step3().unwrap();
        assert_eq!(
            steps::apply(&stored, &job_half, &m_spec).unwrap_err(),
            StepError::AdoptionIncomplete,
            "apply incomplete adoption"
        );
        job.adopt(1, &keys[256..], 512).unwrap();
        job.step3().unwrap();
        assert_eq!(job.clone().step3(), Err(StepError::Phase), "replay Finish");
        let mut wrong_store = stored.clone();
        wrong_store.finalized.slot += 1;
        assert_eq!(
            steps::apply(&wrong_store, &job, &m_spec).unwrap_err(),
            StepError::StoreChanged,
            "wrong store digest"
        );
        step_only_refusals += 8;
    }

    println!(
        "every-field step-path oracle: {} update mutations ({} accepted by both, {} refused by both, \
         {} Helios optimistic-only / step-refused, {} #872 pins), {} bootstrap/store mutations judged against Helios, \
         {} step-path-only wire/witness/job refusals; 0 disagreements",
        tally.total,
        tally.accepted_by_both,
        tally.refused_by_both,
        tally.helios_optimistic_only,
        tally.helios_872_pins,
        diff_non_update,
        step_only_refusals,
    );
    assert_eq!(tally.total, 467);
    assert_eq!(tally.helios_872_pins, 8);
    assert_eq!(diff_non_update, 17);
    assert_eq!(step_only_refusals, 18);
}

#[test]
fn adopted_keys_must_match_committee_key_root_on_step_path() {
    let m_spec = mainnet();
    let m_gvr = root(MAINNET_GVR);
    let m_boot_bytes = read_repo("crates/ferrata-spectests/fixtures/mainnet-p1899/bootstrap.ssz");
    let m_trusted = root("0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac");
    let boot = LightClientBootstrap::decode(&m_boot_bytes, LcFork::Electra, 512).unwrap();

    // 1. Bootstrap adoption (`steps::initialize` + `Job::adopt`, 2-part and 1-part): swapping two
    //    distinct keys preserves the G1 aggregate sum while changing `pubkeys_root`, so only the
    //    Merkle key-root check in `AdoptionProgress::add_part` refuses it.
    let boot_keys = uncompressed_keys(&boot.current_sync_committee).unwrap();
    assert_ne!(boot_keys[0], boot_keys[1], "distinct bootstrap keys for within-part swap");
    assert_ne!(boot_keys[0], boot_keys[256], "distinct bootstrap keys for cross-part swap");
    let boot_summary = CommitteeSummary::of(&boot.current_sync_committee);
    let compact_boot = CompactBootstrap::of(&boot);
    let mut checked = 0usize;

    for (label, i, j) in [("within-part", 0usize, 1usize), ("cross-part", 0, 256)] {
        let mut swapped_comm = boot.current_sync_committee.clone();
        swapped_comm.pubkeys.swap(i, j);
        assert_eq!(BLST.check_committee_onchain(&swapped_comm), Ok(()), "bootstrap sum preserved ({label})");
        assert_ne!(
            CommitteeSummary::of(&swapped_comm).pubkeys_root,
            boot_summary.pubkeys_root,
            "bootstrap root changed ({label})"
        );
        let swapped = uncompressed_keys(&swapped_comm).unwrap();

        let (_, mut job2) = steps::initialize(&m_trusted, &compact_boot, LcFork::Electra, &m_spec, 2).unwrap();
        job2.adopt(0, &swapped[..256], 512).unwrap();
        assert_eq!(
            job2.adopt(1, &swapped[256..], 512),
            Err(StepError::Core(Error::CommitteeInvalid)),
            "bootstrap 2-part adopt swap ({label})"
        );
        checked += 1;

        let (_, mut job1) = steps::initialize(&m_trusted, &compact_boot, LcFork::Electra, &m_spec, 1).unwrap();
        assert_eq!(
            job1.adopt(0, &swapped, 512),
            Err(StepError::Core(Error::CommitteeInvalid)),
            "bootstrap 1-part adopt swap ({label})"
        );
        checked += 1;
    }

    // 2. Update next-committee adoption (full `run_public_job_raw` 2-part job and 1-part VerifyAll
    //    on real mainnet `update_1`): honest keys succeed; two distinct keys swapped (same sum,
    //    wrong `pubkeys_root`) must be refused with `CommitteeInvalid`.
    let mut q_m = Quad::<MainnetConsensusSpec>::from_bootstrap(
        "mainnet-p1899-adopt-root",
        m_spec.clone(),
        m_gvr,
        m_trusted,
        &m_boot_bytes,
        LcFork::Electra,
        2,
    );
    let u0_bytes = read_repo("crates/ferrata-spectests/fixtures/mainnet-p1899/update_0.ssz");
    let u0 = LightClientUpdate::decode(&u0_bytes, LcFork::Electra, 512).unwrap();
    q_m.judge(
        "adopt-root-setup-u0",
        &u0_bytes,
        UpdateKind::Period,
        LcFork::Electra,
        u0.signature_slot + 1,
        true,
        KnownDivergence::None,
    );
    let u1_bytes = read_repo("crates/ferrata-spectests/fixtures/mainnet-p1899/update_1.ssz");
    let u1 = LightClientUpdate::decode(&u1_bytes, LcFork::Electra, 512).unwrap();
    let good_cu = CompactUpdate::of(&u1).encode(LcFork::Electra);
    let good_sw = q_m.steps.build_witness(&u1, &m_spec);
    let good_sw_bytes = good_sw.encode();
    assert!(
        q_m.steps
            .run_public_job_raw(
                &good_cu,
                &good_sw_bytes,
                &u1.next_sync_committee,
                u1.signature_slot + 1,
                &m_spec,
                &m_gvr,
                512
            )
            .is_ok(),
        "control: honest next_sync_committee keys succeed"
    );

    let next_summary = CommitteeSummary::of(&u1.next_sync_committee);
    let stored = q_m.steps.compact_store();
    let cu = CompactUpdate::decode(&good_cu, LcFork::Electra, 512).unwrap();
    for (label, i, j) in [("within-part", 0usize, 1usize), ("cross-part", 0, 256)] {
        assert_ne!(u1.next_sync_committee.pubkeys[i], u1.next_sync_committee.pubkeys[j]);
        let mut swapped_comm = u1.next_sync_committee.clone();
        swapped_comm.pubkeys.swap(i, j);
        assert_eq!(BLST.check_committee_onchain(&swapped_comm), Ok(()), "next committee sum preserved ({label})");
        assert_ne!(
            CommitteeSummary::of(&swapped_comm).pubkeys_root,
            next_summary.pubkeys_root,
            "next committee root changed ({label})"
        );

        // Full 2-part public job path (`Open -> Miller -> Adopt(0) -> Adopt(1)`).
        assert_eq!(
            q_m.steps
                .run_public_job_raw(
                    &good_cu,
                    &good_sw_bytes,
                    &swapped_comm,
                    u1.signature_slot + 1,
                    &m_spec,
                    &m_gvr,
                    512
                )
                .err(),
            Some(StepError::Core(Error::CommitteeInvalid)),
            "public job adopt swap ({label})"
        );
        checked += 1;

        // Single-part `VerifyAll` path (`step1 -> step2 -> step3 -> adopt(0)`).
        let swapped_keys = uncompressed_keys(&swapped_comm).unwrap();
        let mut va_job = steps::step1(&stored, &cu, &good_sw, u1.signature_slot + 1, &m_spec, &m_gvr, 1).unwrap();
        va_job.step2().unwrap();
        va_job.step3().unwrap();
        assert_eq!(
            va_job.adopt(0, &swapped_keys, 512),
            Err(StepError::Core(Error::CommitteeInvalid)),
            "VerifyAll adopt swap ({label})"
        );
        checked += 1;
    }

    println!(
        "adoption key-root binding (step-path-only): {checked} swapped-key adoption cases (bootstrap & update, \
         2-part public job & 1-part VerifyAll, within-part & cross-part; aggregate sum preserved) all refused with CommitteeInvalid"
    );
    assert_eq!(checked, 8);
}

// ---------------------------------------------------------------------------------------------
// 5. Re-signed semantic edge cases on fabricated chains (32-key & 512-key + fork boundary)
// ---------------------------------------------------------------------------------------------

fn minimal_electra_spec() -> ChainSpec {
    let mut versions = [[0u8; 4]; 9];
    for (i, v) in versions.iter_mut().enumerate() {
        *v = [i as u8, 0, 0, 9];
    }
    ChainSpec {
        preset: Preset::MINIMAL,
        versions,
        epochs: [0, 0, 0, 0, 0, 0, FAR_FUTURE_EPOCH, FAR_FUTURE_EPOCH, FAR_FUTURE_EPOCH],
    }
}

fn minimal_bootstrap(committee: &SyncCommittee, slot: u64) -> (LightClientBootstrap, Root) {
    let mut header = fabricated_header(slot, 3);
    let (state_root, branches) =
        sparse_tree(&[(CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA, committee.hash_tree_root())], 11);
    header.beacon.state_root = state_root;
    let boot = LightClientBootstrap {
        header,
        current_sync_committee: committee.clone(),
        current_sync_committee_branch: branches[0].clone(),
    };
    let trusted = boot.header.beacon.hash_tree_root();
    (boot, trusted)
}

#[test]
fn resigned_semantic_edges_agree_with_helios_on_step_path() {
    let mut cases_run = 0usize;

    // A. 32-key Electra fabricated chain (minimal preset: 8 slots/epoch, 8 epochs/period = 64 slots/period)
    let fake_cur = fake_committee(32, 0x5a);
    let fake_next = fake_committee(32, 0x6b);
    let fake_other = fake_committee(32, 0x7c);
    let spec = minimal_electra_spec();
    let gvr = [0x42; 32];
    let ctx = Context { spec: &spec, genesis_validators_root: gvr, bls: &BLST };
    let (boot, trusted) = minimal_bootstrap(&fake_cur.committee, 64); // start of period 1
    let boot_bytes = encode_bootstrap(&boot, LcFork::Electra);
    let base = Quad::<MinimalConsensusSpec>::from_bootstrap(
        "fab-32",
        spec.clone(),
        gvr,
        trusted,
        &boot_bytes,
        LcFork::Electra,
        2,
    );

    // A1. Participation sweep across every threshold edge (0..=32; 2/3 of 32 is 22)
    for signers in [0usize, 1, 15, 16, 17, 20, 21, 22, 23, 31, 32] {
        let mut u = fabricated_update(
            &Fabrication {
                finalized_slot: Some(72),
                attested_slot: 80,
                signature_slot: 81,
                signers: signers.max(1),
                next_committee: None,
            },
            &fake_cur,
            &ctx,
        );
        if signers == 0 {
            u.sync_aggregate.bits = vec![false; 32];
        }
        let bytes = encode_update(&u, UpdateKind::Period, LcFork::Electra);
        let mut q = base.clone();
        let rep = q.judge(
            &format!("part-{signers}-of-32"),
            &bytes,
            UpdateKind::Period,
            LcFork::Electra,
            82,
            false,
            KnownDivergence::None,
        );
        assert_eq!(rep.helios_valid, signers > 0, "signers={signers}");
        assert_eq!(rep.steps_ok, signers >= 22, "signers={signers}");
        cases_run += 1;
    }

    // A2. Re-signed slot, relevance, finality, and period boundaries
    // Store is at slot 64 (period 1, slots 64..127), next committee unknown.
    let edge_specs: Vec<(&str, Fabrication<'_>, u64, bool, bool)> = vec![
        (
            "att==fin (72,72,73)",
            Fabrication {
                finalized_slot: Some(72),
                attested_slot: 72,
                signature_slot: 73,
                signers: 32,
                next_committee: None,
            },
            73,
            true,
            true,
        ),
        (
            "att==fin-1 (72,71,73)",
            Fabrication {
                finalized_slot: Some(72),
                attested_slot: 71,
                signature_slot: 73,
                signers: 32,
                next_committee: None,
            },
            73,
            false,
            false,
        ),
        (
            "sig==att (72,80,80)",
            Fabrication {
                finalized_slot: Some(72),
                attested_slot: 80,
                signature_slot: 80,
                signers: 32,
                next_committee: None,
            },
            81,
            false,
            false,
        ),
        (
            "current==sig (72,80,81 @81)",
            Fabrication {
                finalized_slot: Some(72),
                attested_slot: 80,
                signature_slot: 81,
                signers: 32,
                next_committee: None,
            },
            81,
            true,
            true,
        ),
        (
            "current==sig-1 (72,80,81 @80)",
            Fabrication {
                finalized_slot: Some(72),
                attested_slot: 80,
                signature_slot: 81,
                signers: 32,
                next_committee: None,
            },
            80,
            false,
            false,
        ),
        (
            "fin==store.fin no next (64,80,81)",
            Fabrication {
                finalized_slot: Some(64),
                attested_slot: 80,
                signature_slot: 81,
                signers: 32,
                next_committee: None,
            },
            81,
            true,
            false,
        ),
        (
            "fin==store.fin WITH next (64,80,81)",
            Fabrication {
                finalized_slot: Some(64),
                attested_slot: 80,
                signature_slot: 81,
                signers: 32,
                next_committee: Some(&fake_next.committee),
            },
            81,
            true,
            true,
        ),
        (
            "fin<store.fin WITH next (56,64,65)",
            Fabrication {
                finalized_slot: Some(56),
                attested_slot: 64,
                signature_slot: 65,
                signers: 32,
                next_committee: Some(&fake_next.committee),
            },
            65,
            true,
            false,
        ),
        (
            "fin<store.fin att==store.fin WITH next (64,64,65)",
            Fabrication {
                finalized_slot: Some(64),
                attested_slot: 64,
                signature_slot: 65,
                signers: 32,
                next_committee: Some(&fake_next.committee),
            },
            65,
            true,
            true,
        ),
        (
            "att==store.fin no next (irrelevant)",
            Fabrication {
                finalized_slot: Some(64),
                attested_slot: 64,
                signature_slot: 65,
                signers: 32,
                next_committee: None,
            },
            65,
            false,
            false,
        ),
        (
            "fin==store.fin+1 (65,66,67)",
            Fabrication {
                finalized_slot: Some(65),
                attested_slot: 66,
                signature_slot: 67,
                signers: 32,
                next_committee: None,
            },
            67,
            true,
            true,
        ),
        (
            "optimistic only (None,80,81)",
            Fabrication {
                finalized_slot: None,
                attested_slot: 80,
                signature_slot: 81,
                signers: 32,
                next_committee: None,
            },
            81,
            true,
            false,
        ),
        (
            "sig at last slot of period 1 (127)",
            Fabrication {
                finalized_slot: Some(72),
                attested_slot: 126,
                signature_slot: 127,
                signers: 32,
                next_committee: None,
            },
            127,
            true,
            true,
        ),
        (
            "sig at first slot of period 2 (128, next unknown)",
            Fabrication {
                finalized_slot: Some(72),
                attested_slot: 127,
                signature_slot: 128,
                signers: 32,
                next_committee: None,
            },
            128,
            false,
            false,
        ),
    ];
    for (name, fab, cur, want_h, want_s) in edge_specs {
        let u = fabricated_update(&fab, &fake_cur, &ctx);
        let bytes = encode_update(&u, UpdateKind::Period, LcFork::Electra);
        let mut q = base.clone();
        let rep = q.judge(
            &format!("edge-{name}"),
            &bytes,
            UpdateKind::Period,
            LcFork::Electra,
            cur,
            false,
            KnownDivergence::None,
        );
        assert_eq!((rep.helios_valid, rep.steps_ok), (want_h, want_s), "edge-{name}");
        cases_run += 1;
    }

    // A3. After adopting `fake_next`: committee contradiction, period-2 rotation, and period-3 skip
    let mut q_with_next = base.clone();
    let adopt_u = fabricated_update(
        &Fabrication {
            finalized_slot: Some(72),
            attested_slot: 80,
            signature_slot: 81,
            signers: 32,
            next_committee: Some(&fake_next.committee),
        },
        &fake_cur,
        &ctx,
    );
    let rep = q_with_next.judge(
        "adopt-next",
        &encode_update(&adopt_u, UpdateKind::Period, LcFork::Electra),
        UpdateKind::Period,
        LcFork::Electra,
        81,
        true,
        KnownDivergence::None,
    );
    assert!(rep.steps_ok && rep.helios_moved == (false, true, true));
    cases_run += 1;

    // Contradicting next committee in same period -> refused by both
    let contra_u = fabricated_update(
        &Fabrication {
            finalized_slot: Some(88),
            attested_slot: 96,
            signature_slot: 97,
            signers: 32,
            next_committee: Some(&fake_other.committee),
        },
        &fake_cur,
        &ctx,
    );
    let rep = q_with_next.clone().judge(
        "contradict-next",
        &encode_update(&contra_u, UpdateKind::Period, LcFork::Electra),
        UpdateKind::Period,
        LcFork::Electra,
        97,
        false,
        KnownDivergence::None,
    );
    assert!(!rep.helios_valid && !rep.steps_ok);
    cases_run += 1;

    // Attested in period 2 (slot 136), finalized still in period 1 (slot 120), signed by next committee at 137 -> advances finalized without rotating!
    let cross_no_rot = fabricated_update(
        &Fabrication {
            finalized_slot: Some(120),
            attested_slot: 136,
            signature_slot: 137,
            signers: 32,
            next_committee: Some(&fake_other.committee),
        },
        &fake_next,
        &ctx,
    );
    let rep = q_with_next.clone().judge(
        "cross-period-no-rotate",
        &encode_update(&cross_no_rot, UpdateKind::Period, LcFork::Electra),
        UpdateKind::Period,
        LcFork::Electra,
        137,
        false,
        KnownDivergence::None,
    );
    assert!(rep.steps_ok && rep.helios_moved == (false, true, false));
    cases_run += 1;

    // Full rotation into period 2 (finalized 128, attested 136, signed by next at 137, adopting period 3's committee)
    let rotate_u = fabricated_update(
        &Fabrication {
            finalized_slot: Some(128),
            attested_slot: 136,
            signature_slot: 137,
            signers: 32,
            next_committee: Some(&fake_other.committee),
        },
        &fake_next,
        &ctx,
    );
    let rep = q_with_next.clone().judge(
        "rotate-to-period-2",
        &encode_update(&rotate_u, UpdateKind::Period, LcFork::Electra),
        UpdateKind::Period,
        LcFork::Electra,
        137,
        false,
        KnownDivergence::None,
    );
    assert!(rep.steps_ok && rep.helios_moved == (true, true, true));
    cases_run += 1;

    // Period skipped: signed at period 3 (slot 192) -> refused by both
    let skip_u = fabricated_update(
        &Fabrication {
            finalized_slot: Some(128),
            attested_slot: 191,
            signature_slot: 192,
            signers: 32,
            next_committee: None,
        },
        &fake_next,
        &ctx,
    );
    let rep = q_with_next.clone().judge(
        "skip-to-period-3",
        &encode_update(&skip_u, UpdateKind::Period, LcFork::Electra),
        UpdateKind::Period,
        LcFork::Electra,
        192,
        false,
        KnownDivergence::None,
    );
    assert!(!rep.helios_valid && !rep.steps_ok);
    cases_run += 1;

    // A4. Fork-version `signature_slot.saturating_sub(1)` exact 1-slot boundary (Fulu activates at epoch 10 = slot 80)
    {
        let mut fork_spec = minimal_electra_spec();
        fork_spec.epochs[Fork::Fulu as usize] = 10; // slot 80
        let fork_ctx = Context { spec: &fork_spec, genesis_validators_root: gvr, bls: &BLST };
        let q_fork = Quad::<MinimalConsensusSpec>::from_bootstrap(
            "fork-ver-edge",
            fork_spec.clone(),
            gvr,
            trusted,
            &boot_bytes,
            LcFork::Electra,
            2,
        );
        // signature_slot = 80 -> fork_version_slot = 79 -> Electra version; signature_slot = 81 -> fork_version_slot = 80 -> Fulu version!
        for (name, sig_slot, sign_under_wrong_fork) in [
            ("sig=80 (fork_slot, Electra domain)", 80u64, false),
            ("sig=81 (fork_slot+1, Fulu domain)", 81u64, false),
            ("sig=80 signed with Fulu domain (must refuse)", 80u64, true),
            ("sig=81 signed with Electra domain (must refuse)", 81u64, true),
        ] {
            let mut u = fabricated_update(
                &Fabrication {
                    finalized_slot: Some(72),
                    attested_slot: 79,
                    signature_slot: sig_slot,
                    signers: 32,
                    next_committee: None,
                },
                &fake_cur,
                &fork_ctx,
            );
            if sign_under_wrong_fork {
                // Re-sign under the opposite fork version!
                let mut alt = u.clone();
                alt.signature_slot = if sig_slot == 80 { 81 } else { 80 };
                u.sync_aggregate.signature =
                    sign(&fake_cur, &u.sync_aggregate.bits, &sync_committee_signing_root(&alt, &fork_ctx));
            }
            let bytes = encode_update(&u, UpdateKind::Period, LcFork::Electra);
            let mut q = q_fork.clone();
            let rep = q.judge(
                &format!("fork-domain-{name}"),
                &bytes,
                UpdateKind::Period,
                LcFork::Electra,
                sig_slot,
                false,
                KnownDivergence::None,
            );
            assert_eq!((rep.helios_valid, rep.steps_ok), (!sign_under_wrong_fork, !sign_under_wrong_fork), "{name}");
            cases_run += 1;
        }
    }

    // A5. Malformed adopted `next_sync_committee` (re-signed with valid branch!):
    //     Ferrata's on-chain adoption (`Job::adopt`) refuses before storing the committee;
    //     Helios & spec store do not validate `next_sync_committee` keys during adoption.
    {
        let bad_agg = with_wrong_aggregate(&fake_next.committee);
        let mut with_id = fake_next.committee.clone();
        with_id.pubkeys[4] = identity_g1();
        let mut with_off = fake_next.committee.clone();
        with_off.pubkeys[4] = g1_off_curve();
        for (name, bad_c) in
            [("adopt-wrong-aggregate", bad_agg), ("adopt-identity-key", with_id), ("adopt-off-curve-key", with_off)]
        {
            let u = fabricated_update(
                &Fabrication {
                    finalized_slot: Some(72),
                    attested_slot: 80,
                    signature_slot: 81,
                    signers: 32,
                    next_committee: Some(&bad_c),
                },
                &fake_cur,
                &ctx,
            );
            let mut q = base.clone();
            q.judge(
                name,
                &encode_update(&u, UpdateKind::Period, LcFork::Electra),
                UpdateKind::Period,
                LcFork::Electra,
                81,
                false,
                KnownDivergence::StricterOnChainAdoption,
            );
            cases_run += 1;
        }
    }

    // A6. Pin Helios #871 (identity key) and #751 (wrong aggregate) directly against program init/step path
    {
        let mut with_id = fake_cur.committee.clone();
        with_id.pubkeys[5] = identity_g1();
        with_id.aggregate_pubkey = {
            use blst::min_pk::{AggregatePublicKey, PublicKey};
            let keys: Vec<PublicKey> = with_id
                .pubkeys
                .iter()
                .filter(|k| **k != identity_g1())
                .map(|k| PublicKey::from_bytes(k).unwrap())
                .collect();
            AggregatePublicKey::aggregate(&keys.iter().collect::<Vec<_>>(), false).unwrap().to_public_key().compress()
        };
        let (b_id, tr_id) = minimal_bootstrap(&with_id, 64);
        assert_eq!(
            ProgramStepState::initialize(&tr_id, &b_id, LcFork::Electra, &spec, 2).err(),
            Some(StepError::Core(Error::CommitteeInvalid))
        );
        // Helios accepts it at bootstrap and accepts an update where member 5's bit is set (#871).
        let hb = HBootstrap::<MinimalConsensusSpec>::from_ssz_bytes(&encode_bootstrap(&b_id, LcFork::Electra)).unwrap();
        verify_bootstrap(&hb, b256(&tr_id), &forks(&spec)).unwrap();
        let mut hs = HStore::<MinimalConsensusSpec>::default();
        apply_bootstrap(&mut hs, &hb);
        let mut spec_s = LightClientStore::initialize(&tr_id, &b_id, LcFork::Electra, &spec).unwrap();
        let mut u = fabricated_update(
            &Fabrication {
                finalized_slot: Some(72),
                attested_slot: 80,
                signature_slot: 81,
                signers: 32,
                next_committee: None,
            },
            &fake_cur,
            &ctx,
        );
        let mut others = vec![true; 32];
        others[5] = false;
        u.sync_aggregate.signature = sign(&fake_cur, &others, &sync_committee_signing_root(&u, &ctx));
        assert_eq!(spec_s.process_update(&u, 82, &ctx), Err(Error::PubkeyInvalid));
        let hu = helios_decode_update::<MinimalConsensusSpec>(&encode_update(&u, UpdateKind::Period, LcFork::Electra))
            .unwrap();
        assert!(verify_generic_update(&hu, 82, &hs, b256(&gvr), &forks(&spec)).is_ok(), "pin helios#871");
        cases_run += 1;

        let wrong_agg = with_wrong_aggregate(&fake_cur.committee);
        let (b_wa, tr_wa) = minimal_bootstrap(&wrong_agg, 64);
        assert_eq!(
            ProgramStepState::initialize(&tr_wa, &b_wa, LcFork::Electra, &spec, 2).err(),
            Some(StepError::Core(Error::CommitteeInvalid))
        );
        let hb = HBootstrap::<MinimalConsensusSpec>::from_ssz_bytes(&encode_bootstrap(&b_wa, LcFork::Electra)).unwrap();
        verify_bootstrap(&hb, b256(&tr_wa), &forks(&spec)).unwrap();
        let mut hs = HStore::<MinimalConsensusSpec>::default();
        apply_bootstrap(&mut hs, &hb);
        let mut spec_s = LightClientStore::initialize(&tr_wa, &b_wa, LcFork::Electra, &spec).unwrap();
        let u_all = fabricated_update(
            &Fabrication {
                finalized_slot: Some(72),
                attested_slot: 80,
                signature_slot: 81,
                signers: 32,
                next_committee: None,
            },
            &fake_cur,
            &ctx,
        );
        assert_eq!(spec_s.process_update(&u_all, 82, &ctx), Ok(()));
        let hu =
            helios_decode_update::<MinimalConsensusSpec>(&encode_update(&u_all, UpdateKind::Period, LcFork::Electra))
                .unwrap();
        assert!(verify_generic_update(&hu, 82, &hs, b256(&gvr), &forks(&spec)).is_err(), "pin helios#751");
        cases_run += 1;
    }

    // B. 512-key mainnet-sized fabricated chain: exact 341 vs 342 of 512 supermajority edge!
    {
        let fake512 = fake_committee(512, 0x91);
        let m_spec = ChainSpec {
            preset: Preset::MAINNET,
            versions: mainnet().versions,
            epochs: [0, 0, 0, 0, 0, 0, FAR_FUTURE_EPOCH, FAR_FUTURE_EPOCH, FAR_FUTURE_EPOCH],
        };
        let m_ctx = Context { spec: &m_spec, genesis_validators_root: gvr, bls: &BLST };
        let (b512, tr512) = minimal_bootstrap(&fake512.committee, SLOTS_PER_PERIOD); // slot 8192 (period 1)
        let b512_bytes = encode_bootstrap(&b512, LcFork::Electra);
        let q512 = Quad::<MainnetConsensusSpec>::from_bootstrap(
            "fab-512",
            m_spec.clone(),
            gvr,
            tr512,
            &b512_bytes,
            LcFork::Electra,
            2,
        );
        for (signers, want_steps_ok) in
            [(0usize, false), (1, false), (256, false), (341, false), (342, true), (512, true)]
        {
            let mut u = fabricated_update(
                &Fabrication {
                    finalized_slot: Some(8224),
                    attested_slot: 8256,
                    signature_slot: 8257,
                    signers: signers.max(1),
                    next_committee: None,
                },
                &fake512,
                &m_ctx,
            );
            if signers == 0 {
                u.sync_aggregate.bits = vec![false; 512];
            }
            let bytes = encode_update(&u, UpdateKind::Period, LcFork::Electra);
            let mut q = q512.clone();
            let rep = q.judge(
                &format!("mainnet-512-signers-{signers}"),
                &bytes,
                UpdateKind::Period,
                LcFork::Electra,
                8258,
                false,
                KnownDivergence::None,
            );
            assert_eq!(rep.helios_valid, signers > 0, "512 signers={signers}");
            assert_eq!(rep.steps_ok, want_steps_ok, "512 signers={signers}");
            cases_run += 1;
        }
    }

    println!("re-signed semantic edge oracle: {cases_run} cases (32-key + fork domain + 512-key 341/342); 0 unexpected disagreements");
    assert!(cases_run >= 40);
}

// ---------------------------------------------------------------------------------------------
// 6. Seeded random mutations across ALL 4 layouts/kinds on the program's step path (CI + Full)
// ---------------------------------------------------------------------------------------------

struct CorpusBase {
    label: &'static str,
    kind: UpdateKind,
    layout: LcFork,
    quad: Quad<MainnetConsensusSpec>,
    update_bytes: Vec<u8>,
    orig_sig_slot: u64,
    regions: Vec<(&'static str, usize, usize)>,
}

fn regions_for(bytes: &[u8], kind: UpdateKind, layout: LcFork) -> Vec<(&'static str, usize, usize)> {
    let u32_at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap()) as usize;
    match (kind, layout) {
        (UpdateKind::Period, LcFork::Electra) => {
            let next_c = 4usize;
            let next_b = next_c + 513 * 48;
            let fin_off = next_b + 6 * 32;
            let fin_b = fin_off + 4;
            let bits = fin_b + 7 * 32;
            let sig = bits + 64;
            let sig_slot = sig + 96;
            let att = u32_at(0);
            let fin = u32_at(fin_off);
            vec![
                ("offset of attested header", 0, 4),
                ("next committee", next_c, 513 * 48),
                ("next-committee branch", next_b, 6 * 32),
                ("offset of finalized header", fin_off, 4),
                ("finality branch", fin_b, 7 * 32),
                ("participation bits", bits, 64),
                ("signature", sig, 96),
                ("signature slot", sig_slot, 8),
                ("attested beacon header", att, 112),
                ("attested execution branch & offset", att + 112, 132),
                ("attested payload", att + 244, fin - att - 244),
                ("finalized beacon header", fin, 112),
                ("finalized execution branch & offset", fin + 112, 132),
                ("finalized payload", fin + 244, bytes.len() - fin - 244),
            ]
        }
        (UpdateKind::Finality, LcFork::Electra) => {
            let fin_b = 8usize;
            let bits = fin_b + 7 * 32;
            let sig = bits + 64;
            let sig_slot = sig + 96;
            let att = u32_at(0);
            let fin = u32_at(4);
            vec![
                ("offset of attested header", 0, 4),
                ("offset of finalized header", 4, 4),
                ("finality branch", fin_b, 7 * 32),
                ("participation bits", bits, 64),
                ("signature", sig, 96),
                ("signature slot", sig_slot, 8),
                ("attested beacon header", att, 112),
                ("attested execution branch & offset", att + 112, 132),
                ("attested payload", att + 244, fin - att - 244),
                ("finalized beacon header", fin, 112),
                ("finalized execution branch & offset", fin + 112, 132),
                ("finalized payload", fin + 244, bytes.len() - fin - 244),
            ]
        }
        (UpdateKind::Period, LcFork::Gloas) => {
            const H: usize = 112 + 32 + 11 * 32;
            let next_c = H;
            let next_b = next_c + 513 * 48;
            let fin = next_b + 11 * 32;
            let fin_b = fin + H;
            let bits = fin_b + 9 * 32;
            let sig = bits + 64;
            let sig_slot = sig + 96;
            vec![
                ("attested beacon header", 0, 112),
                ("attested execution block hash", 112, 32),
                ("attested execution branch", 144, 11 * 32),
                ("next committee", next_c, 513 * 48),
                ("next-committee branch", next_b, 11 * 32),
                ("finalized beacon header", fin, 112),
                ("finalized execution block hash", fin + 112, 32),
                ("finalized execution branch", fin + 144, 11 * 32),
                ("finality branch", fin_b, 9 * 32),
                ("participation bits", bits, 64),
                ("signature", sig, 96),
                ("signature slot", sig_slot, 8),
            ]
        }
        (UpdateKind::Finality, LcFork::Gloas) => {
            const H: usize = 112 + 32 + 11 * 32;
            let fin = H;
            let fin_b = 2 * H;
            let bits = fin_b + 9 * 32;
            let sig = bits + 64;
            let sig_slot = sig + 96;
            vec![
                ("attested beacon header", 0, 112),
                ("attested execution block hash", 112, 32),
                ("attested execution branch", 144, 11 * 32),
                ("finalized beacon header", fin, 112),
                ("finalized execution block hash", fin + 112, 32),
                ("finalized execution branch", fin + 144, 11 * 32),
                ("finality branch", fin_b, 9 * 32),
                ("participation bits", bits, 64),
                ("signature", sig, 96),
                ("signature slot", sig_slot, 8),
            ]
        }
        _ => unreachable!(),
    }
}

fn build_random_corpus_bases() -> Vec<CorpusBase> {
    let m_spec = mainnet();
    let s_spec = sepolia();
    let m_gvr = root(MAINNET_GVR);
    let s_gvr = root(SEPOLIA_GVR);

    // 1. Mainnet Electra period updates (p1899 -> p1900)
    let mut q_mp = Quad::<MainnetConsensusSpec>::from_bootstrap(
        "mainnet-p1899",
        m_spec.clone(),
        m_gvr,
        root("0x64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac"),
        &read_repo("crates/ferrata-spectests/fixtures/mainnet-p1899/bootstrap.ssz"),
        LcFork::Electra,
        2,
    );
    let mu0 = read_repo("crates/ferrata-spectests/fixtures/mainnet-p1899/update_0.ssz");
    let mu0_sig = LightClientUpdate::decode(&mu0, LcFork::Electra, 512).unwrap().signature_slot;
    q_mp.judge("base-mu0", &mu0, UpdateKind::Period, LcFork::Electra, mu0_sig + 1, true, KnownDivergence::None);
    let mu1 = read_repo("crates/ferrata-spectests/fixtures/mainnet-p1899/update_1.ssz");
    let mu1_sig = LightClientUpdate::decode(&mu1, LcFork::Electra, 512).unwrap().signature_slot;

    // 2. Mainnet Electra finality update (mainnet-attest-2026-11-10)
    let q_mf = Quad::<MainnetConsensusSpec>::from_bootstrap(
        "mainnet-attest",
        m_spec,
        m_gvr,
        root("0x7c279a5e7aee53514ad2ce373139ff36b50e26563ff53134e46cf57b57382433"),
        &read_repo("crates/ferrata-spectests/fixtures/mainnet-attest-2026-11-10/bootstrap.ssz"),
        LcFork::Electra,
        2,
    );
    let mf = read_repo("crates/ferrata-spectests/fixtures/mainnet-attest-2026-11-10/finality_update.ssz");
    let mf_sig = decode_finality_update(&mf, LcFork::Electra, 512).unwrap().signature_slot;

    // 3. Sepolia Gloas period update (right across the Electra->Gloas fork transition!)
    let mut q_sp = Quad::<MainnetConsensusSpec>::from_bootstrap(
        "sepolia-gloas-fork",
        s_spec.clone(),
        s_gvr,
        root("0x5f71d7f650f9252b48749b5f062c0c234e01afd69f364ab600e2b6051494b20d"),
        &read_repo("crates/ferrata-spectests/fixtures/sepolia-gloas-2026-11-10/bootstrap.ssz"),
        LcFork::Electra,
        2,
    );
    let su0 = read_repo("crates/ferrata-spectests/fixtures/sepolia-gloas-2026-11-10/update_1378.ssz");
    let su0_sig = LightClientUpdate::decode(&su0, LcFork::Electra, 512).unwrap().signature_slot;
    q_sp.judge("base-su1378", &su0, UpdateKind::Period, LcFork::Electra, su0_sig + 1, true, KnownDivergence::None);
    let su1 = read_repo("crates/ferrata-spectests/fixtures/sepolia-gloas-2026-11-10/update_1379.ssz");
    let su1_sig = LightClientUpdate::decode(&su1, LcFork::Gloas, 512).unwrap().signature_slot;

    // 4. Sepolia Gloas finality update (sepolia-2026-11-18)
    let q_sf = Quad::<MainnetConsensusSpec>::from_bootstrap(
        "sepolia-2026-11-18",
        s_spec,
        s_gvr,
        root("0x3256972bbbec2328a3ef603bde6ffa247e8d1f68fbb1635cc1dc42513ac87ebd"),
        &read_repo("crates/ferrata-spectests/fixtures/sepolia-2026-11-18/bootstrap.ssz"),
        LcFork::Gloas,
        2,
    );
    let sf = read_repo("crates/ferrata-spectests/fixtures/sepolia-2026-11-18/finality_update.ssz");
    let sf_sig = decode_finality_update(&sf, LcFork::Gloas, 512).unwrap().signature_slot;

    vec![
        CorpusBase {
            label: "electra-period",
            kind: UpdateKind::Period,
            layout: LcFork::Electra,
            quad: q_mp,
            regions: regions_for(&mu1, UpdateKind::Period, LcFork::Electra),
            update_bytes: mu1,
            orig_sig_slot: mu1_sig,
        },
        CorpusBase {
            label: "electra-finality",
            kind: UpdateKind::Finality,
            layout: LcFork::Electra,
            quad: q_mf,
            regions: regions_for(&mf, UpdateKind::Finality, LcFork::Electra),
            update_bytes: mf,
            orig_sig_slot: mf_sig,
        },
        CorpusBase {
            label: "gloas-period-fork",
            kind: UpdateKind::Period,
            layout: LcFork::Gloas,
            quad: q_sp,
            regions: regions_for(&su1, UpdateKind::Period, LcFork::Gloas),
            update_bytes: su1,
            orig_sig_slot: su1_sig,
        },
        CorpusBase {
            label: "gloas-finality",
            kind: UpdateKind::Finality,
            layout: LcFork::Gloas,
            quad: q_sf,
            regions: regions_for(&sf, UpdateKind::Finality, LcFork::Gloas),
            update_bytes: sf,
            orig_sig_slot: sf_sig,
        },
    ]
}

fn run_random_step_oracle(seed: u64, per_class: usize) {
    let bases = build_random_corpus_bases();
    let mut rng = Rng::new(seed);
    // (class, region) -> [total, refused_all, accepted_all_with_same_heads]
    let mut by_class: BTreeMap<&'static str, [usize; 4]> = BTreeMap::new();
    let mut total = 0usize;
    let mut total_refused = 0usize;
    let mut total_accepted_fixed = 0usize;
    let mut total_accepted_dynamic = 0usize;

    for base in &bases {
        let entry = by_class.entry(base.label).or_default();
        for idx in 0..per_class {
            let (region, start, len) = base.regions[rng.below(base.regions.len())];
            let mut bytes = base.update_bytes.clone();
            let count = 1 + rng.below(3);
            let mut flips = Vec::with_capacity(count);
            while flips.len() < count {
                let flip = (start + rng.below(len), rng.below(8));
                if !flips.contains(&flip) {
                    bytes[flip.0] ^= 1 << flip.1;
                    flips.push(flip);
                }
            }
            // Alternate fixed `current_slot = orig_sig_slot + 1` (catches future signature_slot)
            // and dynamic `current_slot = mutated_sig_slot + 1` (reaches valid re-dated slots).
            let fixed_clock = idx % 2 == 0;
            let current_slot = if fixed_clock {
                base.orig_sig_slot + 1
            } else {
                decode_ssz_update(&bytes, base.kind, base.layout, 512)
                    .map(|u| u.signature_slot.saturating_add(1))
                    .unwrap_or(base.orig_sig_slot + 1)
            };
            let mut q = base.quad.clone();
            let cid = format!(
                "{}#{idx}({region}, {}, flips={flips:?})",
                base.label,
                if fixed_clock { "fixed-clock" } else { "dynamic-clock" }
            );
            let rep = q.judge(&cid, &bytes, base.kind, base.layout, current_slot, false, KnownDivergence::None);
            total += 1;
            entry[0] += 1;
            if !rep.helios_valid && !rep.steps_ok {
                total_refused += 1;
                entry[1] += 1;
            } else if rep.helios_valid && rep.steps_ok {
                if fixed_clock {
                    total_accepted_fixed += 1;
                    entry[2] += 1;
                } else {
                    total_accepted_dynamic += 1;
                    entry[3] += 1;
                }
            }
        }
    }

    println!(
        "seeded step-path random oracle (seed {seed:#x}, {per_class}/class, {total} total): \
         {total_refused} refused by all, {total_accepted_fixed} accepted at fixed clock, \
         {total_accepted_dynamic} accepted at dynamic clock; 0 disagreements"
    );
    println!("class | total | refused_all | accepted_fixed_clock | accepted_dynamic_clock");
    for (cls, c) in &by_class {
        println!("{cls} | {} | {} | {} | {}", c[0], c[1], c[2], c[3]);
    }
}

#[test]
fn ci_fast_random_mutations_agree_with_helios_on_step_path() {
    run_random_step_oracle(0x2611_2401, 100);
}

#[test]
#[ignore = "full 10,000-mutation step-path differential run; invoke via scripts/helios-oracle-full.sh"]
fn full_random_mutations_across_all_layouts_agree_with_helios_on_step_path() {
    let per_class =
        std::env::var("FERRATA_ORACLE_MUTATIONS_PER_CLASS").ok().and_then(|s| s.parse::<usize>().ok()).unwrap_or(2_500);
    run_random_step_oracle(0x2611_24ff, per_class);
}
