//! What the monitor keeps across restarts: one JSON file, rewritten atomically (temporary file,
//! fsync, rename) after every poll and before and after every transaction.
//!
//! The vault is the source of truth. A restarted monitor rescans the vault's events from the block
//! after the last one it finished (less a reorganisation margin), or from `--start-block` without a
//! file, and re-reads every pending candidate from the vault before it acts, so a lost or stale file
//! costs a rescan, never a missed cancellation. The file adds what the chain cannot give back: the
//! cancellations per state (a candidate cancelled twice brings the checkpoint pause), the finalised
//! checkpoints already audited, and the alerts already raised.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Write as _;
use std::path::Path;

use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};

use crate::check::{Candidate, Verdict};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    /// In the vault, not yet decided or passed.
    Pending,
    /// Cancelled by this monitor.
    CancelledByUs,
    /// Gone from the vault without this monitor's cancellation (finalised, or cancelled by another).
    Gone,
    /// Built on a state that is no longer the vault's final state: it can never be finalised.
    Stale,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tracked {
    pub candidate: Candidate,
    pub status: Status,
    pub verdict: Option<Verdict>,
    /// Chain time the monitor first saw it.
    pub first_seen_at: u64,
    /// The cancellation this monitor sent, once mined.
    pub cancel_tx: Option<String>,
    pub cancelled_at: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Store {
    /// The vault this file belongs to (refused for another one).
    pub vault: String,
    /// The next Ethereum block to scan.
    pub next_block: u64,
    /// Candidates by `newState`, keyed `0x…` (a resubmission after a cancellation replaces the entry).
    pub candidates: BTreeMap<String, Tracked>,
    /// Cancellations seen on the vault (`CandidateCancelled`, by anyone), per `newState`: transaction
    /// -> block, so that a rescan counts each once and a replayed submission counts only earlier ones.
    pub cancel_events: BTreeMap<String, BTreeMap<String, u64>>,
    /// Finalised checkpoints by index, with the `newState` and the audit's outcome.
    pub finalized: BTreeMap<u64, String>,
    /// Finalised checkpoints waiting for their audit: index -> the candidate (journal fields).
    pub to_audit: BTreeMap<u64, Candidate>,
    /// Keys of alerts already raised (one alert per fact).
    pub alerted: BTreeSet<String>,
    /// The lowest final height across the node views, and the chain time it last rose (stall watch).
    pub final_height: u64,
    pub final_height_since: u64,
    /// Chain time of this monitor's last `pauseCheckpoints`.
    pub paused_at: Option<u64>,
    /// Polls completed, over the file's life.
    pub polls: u64,
}

impl Store {
    /// The file at `path`, or a new store scanning from `start_block`.
    pub fn load_or_new(path: &Path, vault: &str, start_block: u64) -> Result<(Self, bool)> {
        if !path.exists() {
            return Ok((Self { vault: vault.to_owned(), next_block: start_block, ..Self::default() }, false));
        }
        let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        let s: Self = serde_json::from_slice(&bytes).with_context(|| format!("{} does not parse", path.display()))?;
        ensure!(s.vault.eq_ignore_ascii_case(vault), "{} belongs to vault {}, not {vault}", path.display(), s.vault);
        Ok((s, true))
    }

    /// Atomic rewrite: a crash leaves either the old file or the new one, never a torn one.
    pub fn save(&self, path: &Path) -> Result<()> {
        let tmp = path.with_extension("tmp");
        {
            let mut f = File::create(&tmp).with_context(|| format!("creating {}", tmp.display()))?;
            f.write_all(&serde_json::to_vec_pretty(self)?)?;
            f.sync_all()?;
        }
        fs::rename(&tmp, path).with_context(|| format!("renaming {} to {}", tmp.display(), path.display()))?;
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            if let Ok(d) = File::open(dir) {
                let _ = d.sync_all();
            }
        }
        Ok(())
    }

    /// Cancellations of `new_state` seen in blocks before `block`.
    pub fn cancels_before(&self, new_state: &str, block: u64) -> usize {
        self.cancel_events.get(new_state).map_or(0, |m| m.values().filter(|&&b| b < block).count())
    }

    /// Records an alert key; true if it is new.
    pub fn first_time(&mut self, key: String) -> bool {
        self.alerted.insert(key)
    }
}
