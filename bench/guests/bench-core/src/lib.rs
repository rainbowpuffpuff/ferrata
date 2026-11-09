//! Guest-side cycle measurements for ferrata-core on real mainnet data.
//!
//! Every number recorded here is a delta of `env::cycle_count()`, which in zkVM 3.0.5 is the
//! session's *user* cycle counter (executed instructions plus accelerator cycles; no paging,
//! no power-of-two padding). The host adds the per-execution totals from `SessionInfo`.
//!
//! Modes (first word of the input):
//! * `MODE_E2E`: decode bootstrap, `FinalizedStore::initialize`, then `process_update` on
//!   update_0 and update_1, each step timed.
//! * `MODE_PHASES`: the phase breakdown (1-10) for update_1 against the store state after
//!   update_0, plus the ferrata backend calls and `process_update` with and without the
//!   committee handoff.
//! * `MODE_SPLIT_ML` / `MODE_SPLIT_FE`: the two halves of a split verification (blst only):
//!   everything up to the Miller loop, committing the Fp12 value; then the final
//!   exponentiation and the check, from those bytes. `MODE_SINGLE`: one update per execution (`fn single`).

use std::hint::black_box;

use ferrata_core::{
    bls::{Aggregation, BlsBackend},
    fork::{ChainSpec, LcFork, Preset, FAR_FUTURE_EPOCH},
    merkle::is_valid_normalized_merkle_branch,
    policy::{Adoption, FinalizedStore},
    store::{apply_core, ApplyOutcome, StoreCore},
    types::{LightClientBootstrap, LightClientHeader, LightClientUpdate, SyncCommittee},
    validate::{
        finalized_root_gindex_at_slot, is_finality_update, is_valid_light_client_header,
        next_sync_committee_gindex_at_slot, sync_committee_signing_root, validate_light_client_update, Context,
        SignaturePath,
    },
    Root,
};
use risc0_zkvm::guest::env;

#[cfg(feature = "blst")]
mod lib_blst;
#[cfg(feature = "blst")]
use lib_blst as bls;
#[cfg(feature = "zkcrypto")]
mod lib_zk;
#[cfg(feature = "zkcrypto")]
use lib_zk as bls;

pub const MODE_E2E: u32 = 0;
pub const MODE_PHASES: u32 = 1;
pub const MODE_SPLIT_ML: u32 = 2;
pub const MODE_SPLIT_FE: u32 = 3;
pub const MODE_SINGLE: u32 = 4;
pub const MODE_ONCHAIN: u32 = 5;
pub const MODE_SINGLE_ONCHAIN: u32 = 6;
pub const MODE_CORE_SPLIT_1: u32 = 7;
pub const MODE_CORE_SPLIT_2: u32 = 8;
pub const MODE_STEP_1: u32 = 9;
pub const MODE_STEP_2: u32 = 10;

const N: usize = 512;
const FORK: LcFork = LcFork::Electra; // Fulu uses the Electra light-client layout.

const GENESIS_VALIDATORS_ROOT: &str = "4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95";
const TRUSTED_BLOCK_ROOT: &str = "64b56a372f8c4ccc274d9f42c9b4a1d9a18966a20188992bcac15b1c6e7d91ac";
/// Finalized block root after update_1 (slot 15,564,800), per a second provider.
const FINALIZED_AFTER_UPDATE_1: &str = "96949350abfe8a04e6f1ae22b3064fdcee177dec32133348a2a69d200151e09a";

/// Cycle recorder.
pub struct Rec(pub Vec<(String, u64)>);

impl Rec {
    /// Run `f` and record its user-cycle cost under `label`.
    pub fn time<T>(&mut self, label: &str, f: impl FnOnce() -> T) -> T {
        let start = env::cycle_count();
        let out = black_box(f());
        let end = env::cycle_count();
        self.0.push((label.to_string(), end - start));
        out
    }

    /// Record a plain value (a count, or a derived number).
    pub fn note(&mut self, label: &str, value: u64) {
        self.0.push((label.to_string(), value));
    }
}

/// Read a length-prefixed byte blob (the host pads it to a multiple of 4 bytes).
fn read_blob() -> Vec<u8> {
    let len: u32 = env::read();
    let mut buf = vec![0u8; (len as usize + 3) & !3];
    env::read_slice(&mut buf);
    buf.truncate(len as usize);
    buf
}

fn h32(s: &str) -> Root {
    let b = s.as_bytes();
    let nib = |c: u8| match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        _ => panic!("bad hex"),
    };
    let mut out = [0u8; 32];
    for (i, o) in out.iter_mut().enumerate() {
        *o = (nib(b[2 * i]) << 4) | nib(b[2 * i + 1]);
    }
    out
}

/// Mainnet fork schedule, as in ferrata-spectests/tests/mainnet_replay.rs.
fn mainnet() -> ChainSpec {
    ChainSpec {
        preset: Preset::MAINNET,
        versions: [[0, 0, 0, 0], [1, 0, 0, 0], [2, 0, 0, 0], [3, 0, 0, 0], [4, 0, 0, 0], [5, 0, 0, 0], [6, 0, 0, 0], [7, 0, 0, 0], [8, 0, 0, 0]],
        epochs: [0, 74_240, 144_896, 194_048, 269_568, 364_032, 411_392, FAR_FUTURE_EPOCH, FAR_FUTURE_EPOCH],
    }
}

/// Guest entry point shared by every configuration.
pub fn run() {
    let mode: u32 = env::read();
    let mut rec = Rec(Vec::new());
    rec.note(&format!("config: {}", bls::NAME), 0);
    let mut extra: Vec<u8> = Vec::new();
    match mode {
        MODE_E2E => e2e(&mut rec),
        MODE_PHASES => phases(&mut rec),
        MODE_SPLIT_ML => extra = split_ml(&mut rec),
        MODE_SPLIT_FE => split_fe(&mut rec),
        MODE_SINGLE => single(&mut rec),
        MODE_ONCHAIN => onchain(&mut rec),
        MODE_SINGLE_ONCHAIN => single_onchain(&mut rec),
        MODE_CORE_SPLIT_1 => extra = core_split_1(&mut rec),
        MODE_CORE_SPLIT_2 => core_split_2(&mut rec),
        MODE_STEP_1 => extra = step_1(&mut rec),
        MODE_STEP_2 => extra = step_2(&mut rec),
        _ => panic!("unknown mode {mode}"),
    }
    env::commit(&(rec.0, extra));
}

/// Bootstrap, then update_0 and update_1 through Ferrata's policy store (Aggregation::Auto).
fn e2e(rec: &mut Rec) {
    let boot_bytes = read_blob();
    let u0_bytes = read_blob();
    let u1_bytes = read_blob();
    let spec = mainnet();
    let backend = bls::backend(Aggregation::Auto);
    let ctx = Context { spec: &spec, genesis_validators_root: h32(GENESIS_VALIDATORS_ROOT), bls: &backend };
    let trusted = h32(TRUSTED_BLOCK_ROOT);

    let t0 = env::cycle_count();
    let bootstrap =
        rec.time("E1 decode bootstrap (SSZ)", || LightClientBootstrap::decode(&boot_bytes, FORK, N).unwrap());
    let mut store = rec.time("E2 FinalizedStore::initialize (incl. check_committee on 512 keys)", || {
        FinalizedStore::initialize(&trusted, &bootstrap, FORK, &spec, &backend).unwrap()
    });
    let u0 = rec.time("E3 decode update_0 (SSZ)", || LightClientUpdate::decode(&u0_bytes, FORK, N).unwrap());
    let o0 = rec.time("E4 process_update(update_0): current committee signs, adopts next committee", || {
        store.process_update(&u0, u0.signature_slot + 1, &ctx).unwrap()
    });
    let u1 = rec.time("E5 decode update_1 (SSZ)", || LightClientUpdate::decode(&u1_bytes, FORK, N).unwrap());
    let o1 = rec.time("E6 process_update(update_1): handoff, next committee signs, rotates, adopts", || {
        store.process_update(&u1, u1.signature_slot + 1, &ctx).unwrap()
    });
    let t1 = env::cycle_count();
    rec.note("E  sum E1..E6 (one cycle_count delta)", t1 - t0);

    assert!(!o0.rotated && o0.adopted && o1.rotated && o1.advanced && o1.adopted);
    assert_eq!(store.core().finalized_header.beacon.hash_tree_root(), h32(FINALIZED_AFTER_UPDATE_1));
    rec.note("info: update_0 signers", u0.sync_aggregate.participants());
    rec.note("info: update_1 signers", u1.sync_aggregate.participants());
}

/// The phase breakdown for update_1 (signed by the period-1900 committee, carrying 1901's).
fn phases(rec: &mut Rec) {
    let boot_bytes = read_blob();
    let u0_bytes = read_blob();
    let u1_bytes = read_blob();
    let unc_bytes = read_blob();
    let spec = mainnet();
    let gvr = h32(GENESIS_VALIDATORS_ROOT);

    // Store state after update_0 (update_0 supplies the period-1900 committee and leaves the
    // finalized header at the bootstrap block). Built directly; E2E mode runs the real path.
    let bootstrap = LightClientBootstrap::decode(&boot_bytes, FORK, N).unwrap();
    let u0 = LightClientUpdate::decode(&u0_bytes, FORK, N).unwrap();
    let core = StoreCore {
        fork: FORK,
        finalized_header: bootstrap.header.clone(),
        current_sync_committee: bootstrap.current_sync_committee.clone(),
        next_sync_committee: u0.next_sync_committee.clone(),
    };
    drop(bootstrap);
    drop(u0);

    // 1
    let u = rec.time("1  SSZ decode of update_1 (26,906 bytes)", || {
        LightClientUpdate::decode(&u1_bytes, FORK, N).unwrap()
    });
    let attested_slot = u.attested_header.beacon.slot;
    let state_root = u.attested_header.beacon.state_root;

    // 2
    rec.time("2a header validity, attested (payload header root + 4-deep execution branch)", || {
        assert!(is_valid_light_client_header(&u.attested_header, FORK, &spec))
    });
    rec.time("2b header validity, finalized", || assert!(is_valid_light_client_header(&u.finalized_header, FORK, &spec)));

    // 3
    let sc_root = rec.time("3a SyncCommittee hash_tree_root (512 keys + aggregate)", || {
        u.next_sync_committee.hash_tree_root()
    });
    rec.time("3b next-committee branch check (6 deep)", || {
        assert!(is_valid_normalized_merkle_branch(
            &sc_root,
            &u.next_sync_committee_branch,
            next_sync_committee_gindex_at_slot(attested_slot, FORK, &spec),
            &state_root,
        ))
    });
    rec.time("3c finality: finalized beacon header root + branch check (7 deep)", || {
        let r = u.finalized_header.beacon.hash_tree_root();
        assert!(is_valid_normalized_merkle_branch(
            &r,
            &u.finality_branch,
            finalized_root_gindex_at_slot(attested_slot, FORK, &spec),
            &state_root,
        ))
    });

    // 4
    let backend_sub = bls::backend(Aggregation::Subtractive);
    let backend_add = bls::backend(Aggregation::Additive);
    let backend_auto = bls::backend(Aggregation::Auto);
    let ctx = Context { spec: &spec, genesis_validators_root: gvr, bls: &backend_sub };
    let signing_root = rec.time("4  signing root (attested header root, fork version, domain)", || {
        sync_committee_signing_root(&u, &ctx)
    });

    // 5-9, raw library calls, against the signing committee (the store's next committee).
    let committee = &core.next_sync_committee;
    let bits = &u.sync_aggregate.bits;
    let sig = &u.sync_aggregate.signature;
    let signers = u.sync_aggregate.participants();
    rec.note("info: update_1 signers", signers);
    rec.note("info: update_1 non-signers", N as u64 - signers);
    bls::bls_phases(rec, committee, bits, &signing_root, sig);

    // The ferrata backend, as the stores call it.
    rec.time("B1 backend verify_checked, subtractive (ferrata code, policy store path)", || {
        backend_sub.verify_checked(committee, bits, &signing_root, sig).unwrap()
    });
    rec.time("B2 backend verify_checked, additive (ferrata code)", || {
        backend_add.verify_checked(committee, bits, &signing_root, sig).unwrap()
    });
    rec.time("B3 backend fast_aggregate_verify (spec path: KeyValidate on every signer key)", || {
        backend_sub.fast_aggregate_verify(committee, bits, &signing_root, sig).unwrap()
    });

    // 10, on the committee update_1 hands off to (period 1901).
    let unc: Vec<[u8; 96]> = unc_bytes.chunks_exact(96).map(|c| c.try_into().unwrap()).collect();
    assert_eq!(unc.len(), N);
    bls::adoption_phases(rec, &u.next_sync_committee, &unc);
    rec.time("10a' backend check_committee (ferrata code)", || {
        backend_sub.check_committee(&u.next_sync_committee).unwrap()
    });

    // Whole-update numbers, from the post-update_0 store, Aggregation::Auto (production).
    let ctx_auto = Context { spec: &spec, genesis_validators_root: gvr, bls: &backend_auto };
    rec.time("V  validate_light_client_update(update_1), CheckedCommittee path, Auto (= subtractive)", || {
        validate_light_client_update(&core, &u, u.signature_slot + 1, &ctx_auto, SignaturePath::CheckedCommittee).unwrap()
    });
    // Untimed: from_core re-checks both committees, as at adoption.
    let store = FinalizedStore::from_core(core.clone(), &backend_auto).unwrap();
    let mut with_handoff = store.clone();
    let o1 = rec.time("P1 process_update(update_1) as served: finality + handoff (adopts next committee)", || {
        with_handoff.process_update(&u, u.signature_slot + 1, &ctx_auto).unwrap()
    });
    assert!(o1.rotated && o1.advanced && o1.adopted);

    let mut finality_only = u.clone();
    finality_only.next_sync_committee = SyncCommittee::empty(N);
    finality_only.next_sync_committee_branch = vec![[0u8; 32]; u.next_sync_committee_branch.len()];
    let mut without_handoff = store.clone();
    let o2 = rec.time("P2 process_update(update_1 stripped to a finality update): no committee adoption", || {
        without_handoff.process_update(&finality_only, finality_only.signature_slot + 1, &ctx_auto).unwrap()
    });
    assert!(o2.advanced && !o2.adopted);
    assert_eq!(with_handoff.core().finalized_header, without_handoff.core().finalized_header);
    assert_eq!(with_handoff.core().finalized_header.beacon.hash_tree_root(), h32(FINALIZED_AFTER_UPDATE_1));
}

/// First half of a split verification: everything up to the Miller loop. Commits the Fp12.
fn split_ml(rec: &mut Rec) -> Vec<u8> {
    let u1_bytes = read_blob();
    let committee_bytes = read_blob();
    let spec = mainnet();
    let t0 = env::cycle_count();
    let u = rec.time("S1 decode update_1 (SSZ)", || LightClientUpdate::decode(&u1_bytes, FORK, N).unwrap());
    let committee =
        rec.time("S2 decode signing committee (SSZ, 513 keys)", || SyncCommittee::decode(&committee_bytes, N).unwrap());
    let attested_slot = u.attested_header.beacon.slot;
    rec.time("S3 header validity x2 + finality branch", || {
        assert!(is_valid_light_client_header(&u.attested_header, FORK, &spec));
        assert!(is_valid_light_client_header(&u.finalized_header, FORK, &spec));
        let r = u.finalized_header.beacon.hash_tree_root();
        assert!(is_valid_normalized_merkle_branch(
            &r,
            &u.finality_branch,
            finalized_root_gindex_at_slot(attested_slot, FORK, &spec),
            &u.attested_header.beacon.state_root,
        ));
    });
    let backend = bls::backend(Aggregation::Subtractive);
    let ctx = Context { spec: &spec, genesis_validators_root: h32(GENESIS_VALIDATORS_ROOT), bls: &backend };
    let signing_root = rec.time("S4 signing root", || sync_committee_signing_root(&u, &ctx));
    let f = bls::split_ml(rec, &committee, &u.sync_aggregate.bits, &signing_root, &u.sync_aggregate.signature);
    let t1 = env::cycle_count();
    rec.note("S  sum (one cycle_count delta, excl. input read and journal commit)", t1 - t0);
    f
}

/// Second half: final exponentiation and the check, from the committed Fp12 bytes.
fn split_fe(rec: &mut Rec) {
    let f = read_blob();
    let t0 = env::cycle_count();
    let ok = bls::split_fe(rec, &f);
    let t1 = env::cycle_count();
    assert!(ok, "pairing check failed");
    rec.note("F  sum (one cycle_count delta)", t1 - t0);
}

/// One update per execution, as a program would run it: restore the store from its state
/// (finalized header + both committees, SSZ), decode the update, `process_update`, commit the
/// new finalized root. The only way the current API rebuilds a `FinalizedStore` from saved
/// state is `from_core`, which re-runs `check_committee` on both committees; it is timed
/// separately (R2) so a program that trusts its own state can subtract it.
fn single(rec: &mut Rec) {
    let header_bytes = read_blob();
    let current_bytes = read_blob();
    let next_bytes = read_blob();
    let update_bytes = read_blob();
    let spec = mainnet();
    let backend = bls::backend(Aggregation::Auto);
    let ctx = Context { spec: &spec, genesis_validators_root: h32(GENESIS_VALIDATORS_ROOT), bls: &backend };

    let t0 = env::cycle_count();
    let core = rec.time("R1 decode store state (finalized header + 2 committees, SSZ)", || StoreCore {
        fork: FORK,
        finalized_header: LightClientHeader::decode(&header_bytes, FORK).unwrap(),
        current_sync_committee: SyncCommittee::decode(&current_bytes, N).unwrap(),
        next_sync_committee: SyncCommittee::decode(&next_bytes, N).unwrap(),
    });
    let mut store =
        rec.time("R2 FinalizedStore::from_core (re-checks both committees)", || FinalizedStore::from_core(core, &backend).unwrap());
    let u = rec.time("R3 decode update (SSZ)", || LightClientUpdate::decode(&update_bytes, FORK, N).unwrap());
    let outcome = rec.time("R4 process_update", || store.process_update(&u, u.signature_slot + 1, &ctx).unwrap());
    let root = store.core().finalized_header.beacon.hash_tree_root();
    let t1 = env::cycle_count();
    rec.note("R  sum R1..R4 + new root (one cycle_count delta)", t1 - t0);
    rec.note("info: adopted a committee (1 = yes)", u64::from(outcome.adopted));
    assert!(outcome.advanced);
    assert_eq!(root, h32(FINALIZED_AFTER_UPDATE_1));
}

/// `update` with `next_sync_committee` and its branch zeroed: the same update as a finality
/// update (same signature, same finalized header, nothing to adopt).
fn as_finality_update(update: &LightClientUpdate) -> LightClientUpdate {
    let mut u = update.clone();
    u.next_sync_committee = SyncCommittee::empty(N);
    u.next_sync_committee_branch = vec![[0u8; 32]; update.next_sync_committee_branch.len()];
    u
}

/// Decode a saved store state: finalized header, current and next committee (SSZ).
fn decode_state(header: &[u8], current: &[u8], next: &[u8]) -> StoreCore {
    StoreCore {
        fork: FORK,
        finalized_header: LightClientHeader::decode(header, FORK).unwrap(),
        current_sync_committee: SyncCommittee::decode(current, N).unwrap(),
        next_sync_committee: SyncCommittee::decode(next, N).unwrap(),
    }
}

/// `FinalizedStore::process_update` with `Adoption::OnChain`, line for line, on a `StoreCore`
/// built from saved state. The core has no constructor that restores a `FinalizedStore` without
/// re-checking both committees (`from_core` runs the Full check and fixes `Adoption::Full`), so a
/// program that keeps its state in accounts would run these public building blocks itself.
fn process_update_onchain(core: &mut StoreCore, update: &LightClientUpdate, current_slot: u64, ctx: &Context<'_>) -> ApplyOutcome {
    assert!(is_finality_update(update), "NotFinalized");
    let n = ctx.spec.preset.sync_committee_size as u64;
    assert!(update.sync_aggregate.participants() * 3 >= n * 2, "BelowSupermajority");
    validate_light_client_update(core, update, current_slot, ctx, SignaturePath::CheckedCommittee).unwrap();
    assert!(core.update_advances(update, ctx.spec), "NotNewer");
    let mut next = core.clone();
    let outcome = apply_core(&mut next, update, ctx.spec).unwrap();
    if outcome.adopted {
        ctx.bls.check_committee_onchain(&next.next_sync_committee).unwrap();
    }
    *core = next;
    outcome
}

/// The on-chain adoption policy (`Adoption::OnChain`) on update_1, plus the restore paths.
fn onchain(rec: &mut Rec) {
    let boot_bytes = read_blob();
    let u0_bytes = read_blob();
    let u1_bytes = read_blob();
    let header_bytes = read_blob();
    let current_bytes = read_blob();
    let next_bytes = read_blob();
    let spec = mainnet();
    let backend = bls::backend(Aggregation::Auto);
    let ctx = Context { spec: &spec, genesis_validators_root: h32(GENESIS_VALIDATORS_ROOT), bls: &backend };
    let trusted = h32(TRUSTED_BLOCK_ROOT);

    let bootstrap =
        rec.time("O1 decode bootstrap (SSZ)", || LightClientBootstrap::decode(&boot_bytes, FORK, N).unwrap());
    let mut store = rec.time("O2 initialize_with(Adoption::OnChain): bootstrap checks + check_committee_onchain", || {
        FinalizedStore::initialize_with(&trusted, &bootstrap, FORK, &spec, &backend, Adoption::OnChain).unwrap()
    });
    let u0 = rec.time("O3 decode update_0 (SSZ)", || LightClientUpdate::decode(&u0_bytes, FORK, N).unwrap());
    let o0 = rec.time("O4 process_update(update_0), OnChain: adopts the period-1900 committee", || {
        store.process_update(&u0, u0.signature_slot + 1, &ctx).unwrap()
    });
    assert!(o0.adopted && !o0.rotated);
    let u1 = rec.time("O5 decode update_1 (SSZ)", || LightClientUpdate::decode(&u1_bytes, FORK, N).unwrap());
    let finality = as_finality_update(&u1);

    let mut with_handoff = store.clone();
    let o1 = rec.time("A1 process_update(update_1), OnChain: finality + handoff, check_committee_onchain on period 1901", || {
        with_handoff.process_update(&u1, u1.signature_slot + 1, &ctx).unwrap()
    });
    assert!(o1.rotated && o1.advanced && o1.adopted);
    let mut without = store.clone();
    let o2 = rec.time("A2 process_update(update_1 as finality update), OnChain store: no adoption", || {
        without.process_update(&finality, finality.signature_slot + 1, &ctx).unwrap()
    });
    assert!(o2.advanced && !o2.adopted);
    assert_eq!(with_handoff.core().finalized_header.beacon.hash_tree_root(), h32(FINALIZED_AFTER_UPDATE_1));
    assert_eq!(without.core().finalized_header.beacon.hash_tree_root(), h32(FINALIZED_AFTER_UPDATE_1));

    let committee = &u1.next_sync_committee;
    rec.time("B1 check_committee_onchain(period-1901 committee) alone", || backend.check_committee_onchain(committee).unwrap());
    rec.time("B2 check_committee (Full: KeyValidate per key) alone, for comparison", || backend.check_committee(committee).unwrap());
    rec.time("B3 the rest of a handoff's extra work: committee hash_tree_root + next-committee branch", || {
        let root = committee.hash_tree_root();
        assert!(is_valid_normalized_merkle_branch(
            &root,
            &u1.next_sync_committee_branch,
            next_sync_committee_gindex_at_slot(u1.attested_header.beacon.slot, FORK, &spec),
            &u1.attested_header.beacon.state_root,
        ))
    });

    // Restoring the post-update_0 store from saved state.
    let core = rec.time("R1 decode saved store state (finalized header + 2 committees, SSZ)", || {
        decode_state(&header_bytes, &current_bytes, &next_bytes)
    });
    assert_eq!(&core, store.core(), "saved state differs from the OnChain store after update_0");
    let restored = rec.time("R2 FinalizedStore::from_core: the only constructor from saved state (Full check on both committees; adoption fixed to Full)", || {
        FinalizedStore::from_core(core.clone(), &backend).unwrap()
    });
    assert_eq!(restored.core(), store.core());
    let mut manual = core.clone();
    let o3 = rec.time("M1 OnChain policy on the decoded StoreCore (public building blocks, no re-check), update_1 as served", || {
        process_update_onchain(&mut manual, &u1, u1.signature_slot + 1, &ctx)
    });
    assert_eq!(o3, o1);
    assert_eq!(&manual, with_handoff.core(), "manual OnChain policy differs from FinalizedStore");
    let mut manual = core.clone();
    let o4 = rec.time("M2 OnChain policy on the decoded StoreCore, finality update", || {
        process_update_onchain(&mut manual, &finality, finality.signature_slot + 1, &ctx)
    });
    assert_eq!(o4, o2);
    assert_eq!(&manual, without.core(), "manual OnChain policy differs from FinalizedStore");
}

/// One update per execution with the on-chain policy and no committee re-check on restore:
/// decode saved state, decode the update, run the OnChain policy, commit the new root.
fn single_onchain(rec: &mut Rec) {
    let header_bytes = read_blob();
    let current_bytes = read_blob();
    let next_bytes = read_blob();
    let update_bytes = read_blob();
    let spec = mainnet();
    let backend = bls::backend(Aggregation::Auto);
    let ctx = Context { spec: &spec, genesis_validators_root: h32(GENESIS_VALIDATORS_ROOT), bls: &backend };

    let t0 = env::cycle_count();
    let mut core = rec.time("Q1 decode saved store state (SSZ)", || decode_state(&header_bytes, &current_bytes, &next_bytes));
    let u = rec.time("Q2 decode update (SSZ)", || LightClientUpdate::decode(&update_bytes, FORK, N).unwrap());
    let outcome = rec.time("Q3 OnChain policy: validate (verify_checked), apply, check_committee_onchain if adopting", || {
        process_update_onchain(&mut core, &u, u.signature_slot + 1, &ctx)
    });
    let root = core.finalized_header.beacon.hash_tree_root();
    let t1 = env::cycle_count();
    rec.note("Q  sum Q1..Q3 + new root (one cycle_count delta)", t1 - t0);
    rec.note("info: adopted a committee (1 = yes)", u64::from(outcome.adopted));
    assert!(outcome.advanced);
    assert_eq!(root, h32(FINALIZED_AFTER_UPDATE_1));
}

/// Shared by the split executions: decode update_1 and the signing committee (saved state),
/// then the non-BLS checks of a finality update that cost anything and the signing root.
/// `validate_light_client_update` has no entry point that stops before the signature, so a split
/// program runs these pieces itself.
fn split_prelude(rec: &mut Rec, step: &str) -> (LightClientUpdate, SyncCommittee, Root) {
    let u1_bytes = read_blob();
    let committee_bytes = read_blob();
    let spec = mainnet();
    let u = rec.time(&format!("{step}a decode update_1 (SSZ)"), || LightClientUpdate::decode(&u1_bytes, FORK, N).unwrap());
    let committee = rec.time(&format!("{step}b decode the signing committee from saved state (SSZ)"), || {
        SyncCommittee::decode(&committee_bytes, N).unwrap()
    });
    rec.time(&format!("{step}c supermajority + header validity x2 + finality branch"), || {
        assert!(u.sync_aggregate.participants() * 3 >= N as u64 * 2);
        assert!(is_valid_light_client_header(&u.attested_header, FORK, &spec));
        assert!(is_valid_light_client_header(&u.finalized_header, FORK, &spec));
        let r = u.finalized_header.beacon.hash_tree_root();
        assert!(is_valid_normalized_merkle_branch(
            &r,
            &u.finality_branch,
            finalized_root_gindex_at_slot(u.attested_header.beacon.slot, FORK, &spec),
            &u.attested_header.beacon.state_root,
        ));
    });
    let backend = bls::backend(Aggregation::Auto);
    let ctx = Context { spec: &spec, genesis_validators_root: h32(GENESIS_VALIDATORS_ROOT), bls: &backend };
    let root = rec.time(&format!("{step}d signing root"), || sync_committee_signing_root(&u, &ctx));
    (u, committee, root)
}

/// The core's split, execution 1: everything for a finality update, ending in `Blst::miller_half`.
fn core_split_1(rec: &mut Rec) -> Vec<u8> {
    let t0 = env::cycle_count();
    let (u, committee, root) = split_prelude(rec, "C1");
    let f = bls::core_miller_half(rec, &committee, &u.sync_aggregate.bits, &root, &u.sync_aggregate.signature);
    let t1 = env::cycle_count();
    rec.note("C1 sum (one cycle_count delta, excl. input read and journal commit)", t1 - t0);
    f
}

/// The core's split, execution 2 (also step 3 of the 3-step job): `Blst::final_half`.
fn core_split_2(rec: &mut Rec) {
    let f = read_blob();
    let t0 = env::cycle_count();
    assert!(bls::core_final_half(rec, &f), "final_half refused the product");
    let t1 = env::cycle_count();
    rec.note("C2 sum (one cycle_count delta)", t1 - t0);
}

/// 3-step job, step 1: everything before the Miller loops; hands over H(m), -key, signature.
fn step_1(rec: &mut Rec) -> Vec<u8> {
    let t0 = env::cycle_count();
    let (u, committee, root) = split_prelude(rec, "T1");
    let out = bls::step1(rec, &committee, &u.sync_aggregate.bits, &root, &u.sync_aggregate.signature);
    let t1 = env::cycle_count();
    rec.note("T1 sum (one cycle_count delta, excl. input read and journal commit)", t1 - t0);
    out
}

/// 3-step job, step 2: the Miller loops; hands over the 576-byte product.
fn step_2(rec: &mut Rec) -> Vec<u8> {
    let points = read_blob();
    let t0 = env::cycle_count();
    let out = bls::step2(rec, &points);
    let t1 = env::cycle_count();
    rec.note("T2 sum (one cycle_count delta, excl. input read and journal commit)", t1 - t0);
    out
}
