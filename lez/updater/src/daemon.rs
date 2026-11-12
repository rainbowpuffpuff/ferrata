//! The daemon's set-up and its rounds, shared by the binary (`src/main.rs`) and the Logos module
//! (`modules/ferrata_updater_module`): one configuration file, one ledger, one updater per configured
//! chain, a round that ticks each of them, and the health and metrics a round leaves.

use std::{fs, path::Path, path::PathBuf};

use anyhow::{anyhow, bail, Context as _, Result};
use lee::{AccountId, PrivateKey};

use crate::{
    chain::{preset, root},
    config::File,
    ledger::Ledger,
    source::{BeaconApi, Clock, Recorded, Source},
    updater::{Instance, Plan, Updater},
};

/// The chain to write to, and the Ferrata program's account on it.
pub fn open_ledger(kind: &str, payer: &PrivateKey) -> Result<(Box<dyn Ledger>, AccountId)> {
    match kind {
        #[cfg(feature = "harness")]
        "harness" => {
            let l = crate::ledger::HarnessLedger::new(std::slice::from_ref(payer));
            Ok((Box::new(l), crate::ledger::HarnessLedger::program()))
        }
        #[cfg(not(feature = "harness"))]
        "harness" => {
            let _ = payer;
            bail!("daemon.ledger = \"harness\" needs a build with --features harness")
        }
        "sequencer" => {
            bail!(
                "daemon.ledger = \"sequencer\": the LEZ sequencer backend is not in this build (lez/updater/README.md)"
            )
        }
        other => bail!("daemon.ledger = {other:?}: harness or sequencer"),
    }
}

/// Everything a configuration file sets up: the ledger and one updater per chain.
pub struct Daemon {
    pub file: File,
    pub ledger: Box<dyn Ledger>,
    pub updaters: Vec<Updater>,
    pub round: u64,
}

impl Daemon {
    /// Reads the configuration (relative paths in it are relative to its directory), the payer key,
    /// opens the ledger and the sources, and makes the state directory.
    pub fn open(path: &Path, verbose: bool) -> Result<Self> {
        let file = File::load(path)?;
        let base = path.parent().map(PathBuf::from).unwrap_or_default();
        let at = |p: &str| if PathBuf::from(p).is_absolute() { PathBuf::from(p) } else { base.join(p) };

        let key_hex = fs::read_to_string(at(&file.keys.payer_key_file)).context("keys.payer_key_file")?;
        let key_bytes: [u8; 32] = hex::decode(key_hex.trim().trim_start_matches("0x"))?
            .try_into()
            .map_err(|_| anyhow!("the payer key is not 32 bytes"))?;
        let payer = PrivateKey::try_new(key_bytes).map_err(|e| anyhow!("payer key: {e:?}"))?;

        let (ledger, program) = open_ledger(&file.daemon.ledger, &payer)?;

        let state_dir = at(&file.daemon.state_dir);
        fs::create_dir_all(&state_dir)?;
        let mut updaters = Vec::new();
        for c in &file.chains {
            let config = preset(&c.preset, c.adoption_parts)?;
            let mut sources: Vec<Box<dyn Source>> = Vec::new();
            for s in &c.sources {
                match (&s.url, &s.dir) {
                    (Some(url), _) => sources.push(Box::new(BeaconApi {
                        name: s.name.clone(),
                        base: url.clone(),
                        timeout_secs: s.timeout_secs,
                    })),
                    (_, Some(dir)) => {
                        sources.push(Box::new(Recorded::open(&s.name, &at(dir), &config, Clock::new(usize::MAX))?))
                    }
                    _ => unreachable!("checked by config"),
                }
            }
            let inst = Instance::new(&c.name, program, config, root(&c.checkpoint)?);
            let mut u = Updater::new(
                &file.daemon.name,
                inst,
                payer.clone(),
                file.fees.clone(),
                file.policy.clone(),
                sources,
                Some(state_dir.clone()),
            )?;
            u.verbose = verbose;
            updaters.push(u);
        }
        Ok(Self { file, ledger, updaters, round: 0 })
    }

    /// One round: each updater's tick. Returns what the binary prints for it (waits, stuck jobs, errors).
    pub fn round(&mut self) -> Vec<String> {
        self.round += 1;
        let mut notes = Vec::new();
        for u in self.updaters.iter_mut() {
            match u.tick(self.ledger.as_mut()) {
                Ok(Plan::Send(_)) | Ok(Plan::UpToDate) | Ok(Plan::Deferred(_)) => {}
                Ok(Plan::Waiting(w)) | Ok(Plan::Stuck(w)) => notes.push(format!("[{} {}] {w}", u.label, u.inst.name)),
                Err(e) => notes.push(format!("[{} {}] round {}: {e:#}", u.label, u.inst.name, self.round)),
            }
        }
        notes
    }

    /// `/metrics` (Prometheus text) as a round leaves it.
    pub fn metrics(&self) -> String {
        self.updaters.iter().map(|u| u.render_metrics()).collect()
    }

    /// `/health`: one JSON object per instance; and whether no instance is down.
    pub fn health(&self) -> (Vec<serde_json::Value>, bool) {
        (self.updaters.iter().map(|u| u.health_json()).collect(), self.updaters.iter().all(|u| u.health().0 != "down"))
    }

    /// Whether every updater found its instance up to date in the last round.
    pub fn up_to_date(&self) -> bool {
        self.updaters.iter().all(|u| u.metrics.last_plan == "up-to-date")
    }

    /// Each instance as the chain holds it: finalized slot and header root, period, jobs applied.
    pub fn instances(&self) -> Vec<serde_json::Value> {
        self.updaters
            .iter()
            .map(|u| {
                let store = u.inst.store(self.ledger.as_ref());
                let state = u.inst.state(self.ledger.as_ref());
                match (store, state) {
                    (Ok(Some(s)), Ok(st)) => serde_json::json!({
                        "chain": u.inst.name, "active": true, "finalized_slot": s.finalized.slot,
                        "finalized_root": hex::encode(s.finalized.hash_tree_root()),
                        "period": u.metrics.period, "applied": st.map_or(0, |x| x.applied),
                    }),
                    (Ok(None), _) => serde_json::json!({ "chain": u.inst.name, "active": false }),
                    (Err(e), _) | (_, Err(e)) => serde_json::json!({ "chain": u.inst.name, "error": format!("{e:#}") }),
                }
            })
            .collect()
    }
}
