//! Runs the official `consensus-spec-tests` light-client vectors against `ferrata-core`.
//!
//! * `sync`: replays every step through the spec store ([`LightClientStore`]) and checks the
//!   finalized and optimistic headers after each one. In parallel it feeds the same updates to
//!   Ferrata's policy store ([`FinalizedStore`]) and requires its state to equal the spec
//!   store's until a forced update first changes the spec store (the policy never forces).
//! * `update_ranking`: checks `is_better_update` reproduces the canonical order.

pub mod execkit;
pub mod testkit;

use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use ferrata_core::{
    bls::BlsBackend,
    fork::{ChainSpec, Fork, LcFork, Preset, FAR_FUTURE_EPOCH},
    policy::{Adoption, FinalizedStore},
    store::LightClientStore,
    types::{LightClientBootstrap, LightClientHeader, LightClientUpdate},
    upgrade::{upgrade_bootstrap, upgrade_update},
    validate::{compute_fork_data_root, get_lc_execution_root, is_better_update, Context},
    Error, Root,
};
use serde_yaml::Value;
use sha2::{Digest, Sha256};

/// The pinned `consensus-specs` release whose vectors this harness runs.
pub const SPEC_TESTS_VERSION: &str = "v1.7.0-beta.4";

/// Directory holding `<fork>/light_client/...`: `$FERRATA_VECTORS`, or the pinned download
/// made by `scripts/fetch-vectors.sh`.
pub fn vectors_dir() -> PathBuf {
    std::env::var_os("FERRATA_VECTORS").map(PathBuf::from).unwrap_or_else(|| {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vectors").join(SPEC_TESTS_VERSION).join("tests/minimal")
    })
}

/// The consensus-spec-tests `general` directory of the same release (BLS vectors).
pub fn general_vectors_dir() -> PathBuf {
    vectors_dir().parent().map(|p| p.join("general")).unwrap_or_default()
}

/// The `ethereum/bls12-381-tests` vectors: `$FERRATA_BLS_VECTORS`, or the pinned download.
pub fn bls_vectors_dir() -> PathBuf {
    std::env::var_os("FERRATA_BLS_VECTORS")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vectors/bls12-381-tests-v0.1.2"))
}

/// Read and decompress an `.ssz_snappy` file (snappy block format).
pub fn read_ssz_snappy(path: &Path) -> Result<Vec<u8>, String> {
    let raw = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    snap::raw::Decoder::new().decompress_vec(&raw).map_err(|e| format!("{}: {e}", path.display()))
}

fn read_yaml(path: &Path) -> Result<Value, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_yaml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn hex_bytes<const N: usize>(v: &Value) -> Result<[u8; N], String> {
    let s = v.as_str().ok_or_else(|| format!("expected hex string, got {v:?}"))?;
    let bytes = hex::decode(s.trim().trim_start_matches("0x")).map_err(|e| format!("{s}: {e}"))?;
    bytes.try_into().map_err(|_| format!("{s}: expected {N} bytes"))
}

fn as_u64(v: &Value) -> Result<u64, String> {
    match v {
        Value::Number(n) => n.as_u64().ok_or_else(|| format!("not a u64: {n}")),
        Value::String(s) => s.trim().parse().map_err(|e| format!("{s}: {e}")),
        other => Err(format!("expected integer, got {other:?}")),
    }
}

fn as_version(v: &Value) -> Result<[u8; 4], String> {
    match v {
        // YAML 1.2 reads `0x01000001` as an integer.
        Value::Number(n) => n
            .as_u64()
            .and_then(|x| u32::try_from(x).ok())
            .map(u32::to_be_bytes)
            .ok_or_else(|| format!("bad version {n}")),
        other => hex_bytes::<4>(other),
    }
}

/// A test case's `config.yaml`, as far as the light client needs it.
#[derive(Clone, Debug)]
pub struct CaseConfig {
    pub spec: ChainSpec,
    /// `BLOB_SCHEDULE` as (epoch, max blobs), for Fulu fork digests.
    pub blob_schedule: Vec<(u64, u64)>,
    pub max_blobs_per_block_electra: u64,
}

/// Parse a `config.yaml`. Forks it does not mention are unscheduled.
pub fn parse_config(path: &Path) -> Result<CaseConfig, String> {
    let v = read_yaml(path)?;
    let preset = match v.get("PRESET_BASE").and_then(Value::as_str) {
        Some("minimal") => Preset::MINIMAL,
        Some("mainnet") => Preset::MAINNET,
        other => return Err(format!("{}: unknown PRESET_BASE {other:?}", path.display())),
    };
    let mut versions = [[0u8; 4]; 9];
    let mut epochs = [FAR_FUTURE_EPOCH; 9];
    versions[0] = as_version(&v["GENESIS_FORK_VERSION"])?;
    epochs[0] = 0;
    for fork in &Fork::ALL[1..] {
        let name = fork.name().to_uppercase();
        if let Some(version) = v.get(format!("{name}_FORK_VERSION")) {
            versions[*fork as usize] = as_version(version)?;
        }
        if let Some(epoch) = v.get(format!("{name}_FORK_EPOCH")) {
            epochs[*fork as usize] = as_u64(epoch)?;
        }
    }
    let mut blob_schedule = Vec::new();
    if let Some(Value::Sequence(entries)) = v.get("BLOB_SCHEDULE") {
        for e in entries {
            blob_schedule.push((as_u64(&e["EPOCH"])?, as_u64(&e["MAX_BLOBS_PER_BLOCK"])?));
        }
    }
    let max_blobs_per_block_electra = v.get("MAX_BLOBS_PER_BLOCK_ELECTRA").map(as_u64).transpose()?.unwrap_or(0);
    Ok(CaseConfig { spec: ChainSpec { preset, versions, epochs }, blob_schedule, max_blobs_per_block_electra })
}

impl CaseConfig {
    /// `get_blob_parameters(epoch)`.
    fn blob_parameters(&self, epoch: u64) -> (u64, u64) {
        let mut schedule = self.blob_schedule.clone();
        schedule.sort_by_key(|entry| std::cmp::Reverse(entry.0));
        schedule
            .into_iter()
            .find(|(e, _)| epoch >= *e)
            .unwrap_or((self.spec.fork_epoch(Fork::Electra), self.max_blobs_per_block_electra))
    }

    /// `compute_fork_digest(genesis_validators_root, epoch)` (Fulu form; earlier epochs reduce
    /// to the pre-Fulu digest).
    pub fn fork_digest(&self, gvr: &Root, epoch: u64) -> [u8; 4] {
        let base = compute_fork_data_root(self.spec.compute_fork_version(epoch), gvr);
        let mut digest = [base[0], base[1], base[2], base[3]];
        if epoch >= self.spec.fork_epoch(Fork::Fulu) {
            let (blob_epoch, max_blobs) = self.blob_parameters(epoch);
            let mut h = Sha256::new();
            h.update(blob_epoch.to_le_bytes());
            h.update(max_blobs.to_le_bytes());
            let mask = h.finalize();
            for i in 0..4 {
                digest[i] ^= mask[i];
            }
        }
        digest
    }

    /// The fork whose data carries `digest`. The digest changes at every scheduled fork and,
    /// from Fulu on, at every blob-parameter-only (BPO) epoch of `BLOB_SCHEDULE`.
    pub fn fork_for_digest(&self, gvr: &Root, digest: [u8; 4]) -> Result<Fork, String> {
        Fork::ALL
            .into_iter()
            .map(|f| self.spec.fork_epoch(f))
            .chain(self.blob_schedule.iter().map(|(epoch, _)| *epoch))
            .filter(|e| *e != FAR_FUTURE_EPOCH)
            .find(|e| self.fork_digest(gvr, *e) == digest)
            .map(|e| self.spec.fork_at_epoch(e))
            .ok_or_else(|| format!("no scheduled fork has digest 0x{}", hex::encode(digest)))
    }

    /// The fork with this version.
    pub fn fork_for_version(&self, version: [u8; 4]) -> Result<Fork, String> {
        Fork::ALL
            .into_iter()
            .rev()
            .find(|f| self.spec.fork_version(*f) == version)
            .ok_or_else(|| format!("no fork has version 0x{}", hex::encode(version)))
    }
}

fn lc_fork(fork: Fork) -> Result<LcFork, String> {
    fork.lc_fork().ok_or_else(|| format!("{} has no light-client data", fork.name()))
}

fn check_header(
    label: &str,
    header: &LightClientHeader,
    expected: &Value,
    fork: LcFork,
    spec: &ChainSpec,
) -> Result<(), String> {
    if expected.is_null() {
        return Ok(());
    }
    let slot = as_u64(&expected["slot"])?;
    if header.beacon.slot != slot {
        return Err(format!("{label}.slot: got {}, want {slot}", header.beacon.slot));
    }
    let beacon_root: Root = hex_bytes(&expected["beacon_root"])?;
    if header.beacon.hash_tree_root() != beacon_root {
        return Err(format!("{label}.beacon_root mismatch"));
    }
    if let Some(execution_root) = expected.get("execution_root") {
        let want: Root = hex_bytes(execution_root)?;
        if get_lc_execution_root(header, fork, spec) != want {
            return Err(format!("{label}.execution_root mismatch"));
        }
    }
    Ok(())
}

/// What one `sync` case exercised.
#[derive(Clone, Debug, Default)]
pub struct SyncOutcome {
    pub steps: usize,
    pub process_updates: usize,
    pub force_updates: usize,
    pub upgrades: usize,
    /// Upgrades of incoming data from an older fork's layout before processing.
    pub data_upgrades: usize,
    /// Steps after which the policy store's state was compared with the spec store's.
    pub policy_steps_compared: usize,
    /// Updates the policy store applied.
    pub policy_applied: usize,
    /// Updates the policy store refused, by error code.
    pub policy_refused: BTreeMap<u16, usize>,
    /// True if a forced update changed the spec store (state comparison stopped there).
    pub forced_divergence: bool,
    /// After that divergence the policy store is still fed every update: its refusals, by code.
    pub policy_refused_after_divergence: BTreeMap<u16, usize>,
    /// The policy store's finalized slot after the last step.
    pub policy_final_slot: u64,
    /// The spec store knows its next committee after the last step.
    pub final_next_committee_known: bool,
}

/// Run one `sync` test case directory, with the off-chain committee check.
pub fn run_sync_case(dir: &Path, bls: &dyn BlsBackend) -> Result<SyncOutcome, String> {
    run_sync_case_with(dir, bls, Adoption::Full)
}

/// Run one `sync` test case directory, with the policy store adopting committees by `adoption`.
pub fn run_sync_case_with(dir: &Path, bls: &dyn BlsBackend, adoption: Adoption) -> Result<SyncOutcome, String> {
    let meta = read_yaml(&dir.join("meta.yaml"))?;
    let cfg = parse_config(&dir.join("config.yaml"))?;
    let spec = &cfg.spec;
    let n = spec.preset.sync_committee_size;
    let gvr: Root = hex_bytes(&meta["genesis_validators_root"])?;
    let trusted: Root = hex_bytes(&meta["trusted_block_root"])?;
    let bootstrap_fork = lc_fork(cfg.fork_for_digest(&gvr, hex_bytes(&meta["bootstrap_fork_digest"])?)?)?;
    let store_fork = lc_fork(cfg.fork_for_version(as_version(&meta["store_fork_version"])?)?)?;

    let bootstrap =
        LightClientBootstrap::decode(&read_ssz_snappy(&dir.join("bootstrap.ssz_snappy"))?, bootstrap_fork, n)
            .map_err(|e| format!("bootstrap decode: {e}"))?;
    let bootstrap = upgrade_bootstrap(bootstrap, bootstrap_fork, store_fork, spec)
        .map_err(|e| format!("bootstrap upgrade: {e}"))?;
    let mut store =
        LightClientStore::initialize(&trusted, &bootstrap, store_fork, spec).map_err(|e| format!("initialize: {e}"))?;
    let mut policy = FinalizedStore::initialize_with(&trusted, &bootstrap, store_fork, spec, bls, adoption)
        .map_err(|e| format!("policy initialize: {e}"))?;
    let ctx = Context { spec, genesis_validators_root: gvr, bls };

    let mut out = SyncOutcome::default();
    let mut compare_policy = true;
    let mut spec_finalized: HashSet<Root> = HashSet::from([store.core.finalized_header.beacon.hash_tree_root()]);
    let steps = read_yaml(&dir.join("steps.yaml"))?;
    for (i, step) in steps.as_sequence().ok_or("steps.yaml is not a list")?.iter().enumerate() {
        let (kind, body) =
            step.as_mapping().and_then(|m| m.iter().next()).ok_or_else(|| format!("step {i}: not a mapping"))?;
        let kind = kind.as_str().unwrap_or_default();
        let at = |e: String| format!("step {i} ({kind}): {e}");
        match kind {
            "process_update" => {
                let update_fork = lc_fork(cfg.fork_for_digest(&gvr, hex_bytes(&body["update_fork_digest"])?)?)?;
                let name = body["update"].as_str().ok_or_else(|| at("missing update".into()))?.trim();
                let bytes = read_ssz_snappy(&dir.join(format!("{name}.ssz_snappy")))?;
                let update =
                    LightClientUpdate::decode(&bytes, update_fork, n).map_err(|e| at(format!("decode: {e}")))?;
                if update_fork != store.core.fork {
                    out.data_upgrades += 1;
                }
                let update = upgrade_update(update, update_fork, store.core.fork, spec)
                    .map_err(|e| at(format!("upgrade: {e}")))?;
                let current_slot = as_u64(&body["current_slot"])?;
                store
                    .process_update(&update, current_slot, &ctx)
                    .map_err(|e| at(format!("spec store refused: {e}")))?;
                match policy.process_update(&update, current_slot, &ctx) {
                    Ok(_) => out.policy_applied += 1,
                    Err(e @ (Error::NotFinalized | Error::BelowSupermajority | Error::NotNewer)) if compare_policy => {
                        *out.policy_refused.entry(e.code()).or_default() += 1;
                    }
                    Err(e) if compare_policy => {
                        return Err(at(format!("policy store refused an update the spec accepted: {e}")))
                    }
                    Err(e) => *out.policy_refused_after_divergence.entry(e.code()).or_default() += 1,
                }
                out.process_updates += 1;
            }
            "force_update" => {
                let before = store.core.clone();
                store.process_force_update(as_u64(&body["current_slot"])?, spec).map_err(|e| at(e.to_string()))?;
                if store.core != before {
                    compare_policy = false;
                    out.forced_divergence = true;
                }
                out.force_updates += 1;
            }
            "upgrade_store" => {
                let to = lc_fork(cfg.fork_for_version(as_version(&body["store_fork_version"])?)?)?;
                store.upgrade(to, spec).map_err(|e| at(e.to_string()))?;
                policy.upgrade(to, spec).map_err(|e| at(format!("policy: {e}")))?;
                out.upgrades += 1;
            }
            other => return Err(at(format!("unknown step kind {other}"))),
        }
        let checks = &body["checks"];
        check_header(
            "finalized_header",
            &store.core.finalized_header,
            &checks["finalized_header"],
            store.core.fork,
            spec,
        )
        .map_err(at)?;
        check_header(
            "optimistic_header",
            &store.optimistic_header,
            &checks["optimistic_header"],
            store.core.fork,
            spec,
        )
        .map_err(at)?;
        spec_finalized.insert(store.core.finalized_header.beacon.hash_tree_root());
        // Safety, at every step: the policy store only ever holds a header the spec store finalized.
        if !spec_finalized.contains(&policy.core().finalized_header.beacon.hash_tree_root()) {
            return Err(at("policy store holds a header the spec store never finalized".into()));
        }
        if compare_policy {
            if policy.core() != &store.core {
                return Err(at("policy store diverged from the spec store without a forced update".into()));
            }
            out.policy_steps_compared += 1;
        }
        out.steps += 1;
    }
    out.policy_final_slot = policy.core().finalized_header.beacon.slot;
    out.final_next_committee_known = store.core.is_next_sync_committee_known();
    Ok(out)
}

/// Run one `update_ranking` case for `fork`. Returns the number of updates ranked.
pub fn run_update_ranking_case(dir: &Path, fork: Fork) -> Result<usize, String> {
    let lc = lc_fork(fork)?;
    // Ranking depends only on the preset; these cases ship no config.yaml.
    let spec = ChainSpec { preset: Preset::MINIMAL, versions: [[0; 4]; 9], epochs: [0; 9] };
    let n = spec.preset.sync_committee_size;
    let count = as_u64(&read_yaml(&dir.join("meta.yaml"))?["updates_count"])? as usize;
    let updates = (0..count)
        .map(|i| {
            let bytes = read_ssz_snappy(&dir.join(format!("updates_{i}.ssz_snappy")))?;
            LightClientUpdate::decode(&bytes, lc, n).map_err(|e| format!("updates_{i}: {e}"))
        })
        .collect::<Result<Vec<_>, String>>()?;
    for i in 0..count {
        for j in i + 1..count {
            if j == i + 1 && !is_better_update(&updates[i], &updates[j], &spec) {
                return Err(format!("updates_{i} should rank above updates_{j}"));
            }
            if is_better_update(&updates[j], &updates[i], &spec) {
                return Err(format!("updates_{j} ranks above earlier updates_{i}"));
            }
        }
    }
    Ok(count)
}

/// Every case directory of `handler` (e.g. `sync`), by fork, in a stable order.
pub fn cases(handler: &str) -> Result<Vec<(Fork, PathBuf)>, String> {
    let root = vectors_dir();
    if !root.is_dir() {
        return Err(format!(
            "spec vectors not found at {} (run scripts/fetch-vectors.sh or set FERRATA_VECTORS)",
            root.display()
        ));
    }
    let mut out = Vec::new();
    for fork in Fork::ALL {
        let dir = root.join(fork.name()).join("light_client").join(handler).join("pyspec_tests");
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        let mut dirs: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_dir()).collect();
        dirs.sort();
        out.extend(dirs.into_iter().map(|d| (fork, d)));
    }
    Ok(out)
}

/// A `sync` case loaded for building other tests on real data.
pub struct LoadedCase {
    pub config: CaseConfig,
    pub genesis_validators_root: Root,
    pub trusted_block_root: Root,
    /// Upgraded to `store_fork`.
    pub bootstrap: LightClientBootstrap,
    pub store_fork: LcFork,
    /// The case's `process_update` steps as (update upgraded to `store_fork`, current slot),
    /// up to its first `upgrade_store` step.
    pub updates: Vec<(LightClientUpdate, u64)>,
}

/// Load a `sync` case for reuse.
pub fn load_sync_case(dir: &Path) -> Result<LoadedCase, String> {
    let meta = read_yaml(&dir.join("meta.yaml"))?;
    let config = parse_config(&dir.join("config.yaml"))?;
    let spec = &config.spec;
    let n = spec.preset.sync_committee_size;
    let gvr: Root = hex_bytes(&meta["genesis_validators_root"])?;
    let trusted: Root = hex_bytes(&meta["trusted_block_root"])?;
    let bootstrap_fork = lc_fork(config.fork_for_digest(&gvr, hex_bytes(&meta["bootstrap_fork_digest"])?)?)?;
    let store_fork = lc_fork(config.fork_for_version(as_version(&meta["store_fork_version"])?)?)?;
    let bootstrap =
        LightClientBootstrap::decode(&read_ssz_snappy(&dir.join("bootstrap.ssz_snappy"))?, bootstrap_fork, n)
            .map_err(|e| e.to_string())?;
    let bootstrap = upgrade_bootstrap(bootstrap, bootstrap_fork, store_fork, spec).map_err(|e| e.to_string())?;
    let mut updates = Vec::new();
    for step in read_yaml(&dir.join("steps.yaml"))?.as_sequence().ok_or("steps")? {
        let Some((kind, body)) = step.as_mapping().and_then(|m| m.iter().next()) else { continue };
        match kind.as_str() {
            Some("process_update") => {
                let fork = lc_fork(config.fork_for_digest(&gvr, hex_bytes(&body["update_fork_digest"])?)?)?;
                let name = body["update"].as_str().ok_or("update")?.trim();
                let update =
                    LightClientUpdate::decode(&read_ssz_snappy(&dir.join(format!("{name}.ssz_snappy")))?, fork, n)
                        .map_err(|e| e.to_string())?;
                let update = upgrade_update(update, fork, store_fork, spec).map_err(|e| e.to_string())?;
                updates.push((update, as_u64(&body["current_slot"])?));
            }
            Some("upgrade_store") => break,
            _ => {}
        }
    }
    Ok(LoadedCase { config, genesis_validators_root: gvr, trusted_block_root: trusted, bootstrap, store_fork, updates })
}
