//! The attestation side's `monitor` (RFP-021 S.7 and VS.4; `docs/design/attestation.md` §3.6, §8, §11.3).
//! Prototype, unaudited.
//!
//! It watches one instance of the attestation program on LEZ and compares what the instance holds with what
//! several beacon providers report:
//!
//! * **divergence** (`instance-diverged`, critical): the instance's finalized header is not the canonical block at
//!   its slot by the providers' account (at least `min_agreeing` of them report another root, or no block, and none
//!   reports the instance's). This is what a captured or colluding sync committee leaves behind: a forged handoff or
//!   finality the program could not tell apart (`docs/trust.md` §2). The program cannot see it; a consumer wires this
//!   alarm to its freeze authority (the alert command) and sizes its caps to what may pass before the freeze;
//! * **stall** (`instance-stalled`, warning): the instance's finalized header lags the providers' finalized header
//!   by more than `stall_after_s`. Nobody is obliged to advance an instance (S.7): the alarm tells whoever cares that
//!   updates are outstanding. Anyone can submit them, and the instance resumes (R.7);
//! * **weak subjectivity** (`ws-half`, critical): the instance's finalized header is older, by the chain's clock, than
//!   half its weak-subjectivity period (`Config::ws_end_slot`), the point at which the design wants someone to act
//!   (§3.6); `ws-expired` (critical): past the period the program refuses every update and the instance must be
//!   replaced by a fresh one (F.10);
//! * the providers themselves: one that cannot be reached, answers for another chain (its genesis is not the
//!   instance's), or contradicts the others is reported, and is never counted towards a verdict.
//!
//! [`assess`] is the whole decision, a pure function of the instance's state, the providers' answers and the clock;
//! [`source`] reads the instance (a LEZ sequencer's `getAccount`, a file or a command) and the providers (beacon API,
//! over `curl`); [`watch`] runs the decision as a daemon with alerts, a status file and a clean stop.

use anyhow::{anyhow, Result};
use ferrata_core::{witness::CompactStore, Root};
use ferrata_lez_wire::InstanceState;
use serde::{Deserialize, Serialize};

pub mod alert;
pub mod source;
pub mod watch;

pub use alert::{Alert, Level};

/// What one beacon provider answered in one poll.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderView {
    pub name: String,
    /// Why the provider could not be used this poll (unreachable, an error status, an answer that does not parse).
    pub error: Option<String>,
    /// The provider's genesis (`/eth/v1/beacon/genesis`): its `genesis_validators_root` and `genesis_time`.
    pub genesis_validators_root: Option<Root>,
    pub genesis_time: Option<u64>,
    /// The canonical block root at the instance's finalized slot (`/eth/v1/beacon/headers/{slot}`): `Some(None)` when
    /// the provider has no canonical block at that slot (HTTP 404, or a header it marks `canonical: false`).
    pub root_at_slot: Option<Option<Root>>,
    /// The provider's finalized slot (`/eth/v1/beacon/headers/finalized`).
    pub finalized_slot: Option<u64>,
}

/// The decision's parameters.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Policy {
    /// Agreeing providers needed to confirm a header, and disagreeing ones (with none agreeing) to call it diverged.
    pub min_agreeing: usize,
    /// The lag of the instance's finalized header behind the providers' finalized header that raises the stall alarm.
    pub stall_after_s: u64,
}

impl Default for Policy {
    fn default() -> Self {
        // Two missed sessions of the lab's two-hour cadence (operator guide §3), plus finality.
        Self { min_agreeing: 2, stall_after_s: 4 * 3600 + 15 * 60 }
    }
}

/// A provider's standing in one poll.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Standing {
    /// Its canonical root at the slot is the instance's finalized root.
    Agrees,
    /// It reports another root, or no canonical block, at the slot.
    Disagrees,
    /// No usable answer.
    Unavailable,
    /// Its genesis is not the instance's: another chain; not counted.
    WrongChain,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderVerdict {
    pub name: String,
    pub standing: Standing,
    pub root_at_slot: Option<String>,
    pub finalized_slot: Option<u64>,
    pub note: Option<String>,
}

/// The verdict on the instance's finalized header.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Divergence {
    /// At least `min_agreeing` providers agree and none disagrees.
    Confirmed,
    /// At least `min_agreeing` providers disagree and none agrees: the instance is not on the providers' chain.
    Diverged,
    /// Providers agree and disagree at once: no verdict, the providers are at fault or the network is split.
    Contested,
    /// Too few usable answers for either verdict.
    Unconfirmed,
}

/// One poll's assessment of the instance.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Assessment {
    pub instance_id: String,
    pub chain_id: u64,
    pub active: bool,
    pub applied_jobs: u64,
    pub finalized_slot: u64,
    pub finalized_root: String,
    /// The sync-committee period of the finalized header, and whether the store knows the next committee.
    pub store_period: u64,
    pub next_committee_known: bool,
    /// The chain's clock (`genesis_time + slot × seconds_per_slot`) at the poll.
    pub now_s: u64,
    pub now_slot: u64,
    /// Age of the finalized header by the chain's clock.
    pub age_s: u64,
    /// The providers' finalized slot (the median of those that answered on the instance's chain), and the instance's
    /// lag behind it.
    pub chain_finalized_slot: Option<u64>,
    pub lag_s: Option<u64>,
    /// The weak-subjectivity period of the finalized header: the first slot outside it, and the halfway slot.
    pub ws_end_slot: Option<u64>,
    pub ws_half_slot: Option<u64>,
    pub divergence: Divergence,
    pub providers: Vec<ProviderVerdict>,
    pub alerts: Vec<Alert>,
}

impl Assessment {
    /// The highest level among the alerts, if any.
    pub fn level(&self) -> Option<Level> {
        self.alerts.iter().map(|a| a.level).max()
    }
}

pub fn hex0x(b: &[u8]) -> String {
    format!("0x{}", hex::encode(b))
}

/// `0x`-hex (or bare hex) of 32 bytes.
pub fn parse_root(s: &str) -> Result<Root> {
    let b = hex::decode(s.trim().trim_start_matches("0x")).map_err(|e| anyhow!("{s:?}: {e}"))?;
    b.try_into().map_err(|_| anyhow!("{s:?}: not 32 bytes"))
}

/// The instance's state from its Borsh encoding (as LEZ holds it at `instance(id)`).
pub fn decode_state(bytes: &[u8]) -> Result<InstanceState> {
    borsh::from_slice(bytes).map_err(|e| anyhow!("the instance state does not decode: {e}"))
}

/// The whole decision: the instance's `state`, the providers' `views`, the chain's clock `now_s` (UNIX seconds).
pub fn assess(state: &InstanceState, views: &[ProviderView], now_s: u64, p: &Policy) -> Result<Assessment> {
    let c = &state.config;
    let spec = c.spec();
    let store = CompactStore::decode(&state.store).map_err(|e| anyhow!("the instance's store does not decode: {e:?}"))?;
    let slot = store.finalized.slot;
    let root = store.finalized.hash_tree_root();
    let sps = c.seconds_per_slot.max(1);
    let now_slot = now_s.saturating_sub(c.genesis_time) / sps;
    let header_time = c.genesis_time + slot * sps;
    let age_s = now_s.saturating_sub(header_time);
    let ws_end_slot = c.ws_end_slot(slot);
    let ws_half_slot = ws_end_slot.map(|end| slot + (end - slot) / 2);

    let mut alerts = Vec::new();
    let id = hex0x(&state.id);
    if !state.active {
        alerts.push(Alert::new(Level::Warn, "instance-inactive", "the instance's initial committee is not adopted yet")
            .with("instance", &id));
    }

    // The providers, each on its own.
    let mut verdicts = Vec::new();
    let mut finalized = Vec::new();
    for v in views {
        let mut verdict =
            ProviderVerdict { name: v.name.clone(), standing: Standing::Unavailable, root_at_slot: None, finalized_slot: v.finalized_slot, note: None };
        if let Some(e) = &v.error {
            verdict.note = Some(e.clone());
            alerts.push(Alert::new(Level::Warn, "provider-unavailable", "a beacon provider gave no usable answer")
                .with("provider", &v.name)
                .with("error", e));
            verdicts.push(verdict);
            continue;
        }
        let wrong_genesis = v.genesis_validators_root.is_some_and(|g| g != c.genesis_validators_root)
            || v.genesis_time.is_some_and(|t| t != c.genesis_time);
        if wrong_genesis {
            verdict.standing = Standing::WrongChain;
            verdict.note = Some(format!(
                "genesis {} at {} is not the instance's {} at {}",
                v.genesis_validators_root.map(|g| hex0x(&g)).unwrap_or_default(),
                v.genesis_time.unwrap_or_default(),
                hex0x(&c.genesis_validators_root),
                c.genesis_time
            ));
            alerts.push(Alert::new(Level::Warn, "provider-wrong-chain", "a beacon provider answers for another chain; not counted")
                .with("provider", &v.name));
            verdicts.push(verdict);
            continue;
        }
        if let Some(f) = v.finalized_slot {
            finalized.push(f);
        }
        match &v.root_at_slot {
            None => {
                verdict.note = Some("no answer for the instance's slot".into());
            }
            Some(Some(r)) => {
                verdict.root_at_slot = Some(hex0x(r));
                verdict.standing = if *r == root { Standing::Agrees } else { Standing::Disagrees };
            }
            Some(None) => {
                verdict.standing = Standing::Disagrees;
                verdict.note = Some("no canonical block at the instance's slot".into());
            }
        }
        verdicts.push(verdict);
    }
    let agree = verdicts.iter().filter(|v| v.standing == Standing::Agrees).count();
    let disagree = verdicts.iter().filter(|v| v.standing == Standing::Disagrees).count();
    let divergence = match (agree, disagree) {
        (a, 0) if a >= p.min_agreeing => Divergence::Confirmed,
        (0, d) if d >= p.min_agreeing => Divergence::Diverged,
        (a, d) if a > 0 && d > 0 => Divergence::Contested,
        _ => Divergence::Unconfirmed,
    };
    let dissent: Vec<String> =
        verdicts.iter().filter(|v| v.standing == Standing::Disagrees).map(|v| v.name.clone()).collect();
    match divergence {
        Divergence::Confirmed => {}
        Divergence::Diverged => alerts.push(
            Alert::new(
                Level::Critical,
                "instance-diverged",
                "the instance's finalized header is not the canonical block at its slot: freeze what consumes this instance",
            )
            .with("instance", &id)
            .with("slot", slot)
            .with("instance_root", hex0x(&root))
            .with("providers", serde_json::json!(dissent)),
        ),
        Divergence::Contested => alerts.push(
            Alert::new(Level::Warn, "providers-disagree", "providers disagree about the instance's finalized header; no verdict")
                .with("slot", slot)
                .with("agree", agree)
                .with("disagree", serde_json::json!(dissent)),
        ),
        Divergence::Unconfirmed => alerts.push(
            Alert::new(Level::Warn, "instance-unconfirmed", "too few providers answered to confirm the instance's finalized header")
                .with("slot", slot)
                .with("agree", agree)
                .with("needed", p.min_agreeing),
        ),
    }

    // The stall: against the providers' finalized slot (the median, so that one provider cannot raise or hide it).
    finalized.sort_unstable();
    let chain_finalized_slot = (!finalized.is_empty()).then(|| finalized[(finalized.len() - 1) / 2]);
    let lag_s = chain_finalized_slot.map(|f| f.saturating_sub(slot) * sps);
    if let Some(lag) = lag_s {
        if lag > p.stall_after_s {
            alerts.push(
                Alert::new(Level::Warn, "instance-stalled", "the instance's finalized header lags the chain's: updates are outstanding, anyone may submit them")
                    .with("instance", &id)
                    .with("slot", slot)
                    .with("chain_finalized_slot", chain_finalized_slot.unwrap())
                    .with("lag_s", lag)
                    .with("stall_after_s", p.stall_after_s),
            );
        }
    }

    // Weak subjectivity, by the chain's clock.
    if let (Some(end), Some(half)) = (ws_end_slot, ws_half_slot) {
        if now_slot >= end {
            alerts.push(
                Alert::new(Level::Critical, "ws-expired", "the instance is past its weak-subjectivity period: it can no longer advance; replace it with a fresh instance (F.10)")
                    .with("instance", &id)
                    .with("slot", slot)
                    .with("ws_end_slot", end)
                    .with("now_slot", now_slot),
            );
        } else if now_slot >= half {
            alerts.push(
                Alert::new(Level::Critical, "ws-half", "the instance's finalized header is past half its weak-subjectivity period: submit the outstanding updates now")
                    .with("instance", &id)
                    .with("slot", slot)
                    .with("ws_half_slot", half)
                    .with("ws_end_slot", end)
                    .with("now_slot", now_slot)
                    .with("seconds_left", (end - now_slot) * sps),
            );
        }
    }

    Ok(Assessment {
        instance_id: id,
        chain_id: c.chain_id,
        active: state.active,
        applied_jobs: state.applied,
        finalized_slot: slot,
        finalized_root: hex0x(&root),
        store_period: spec.period_at_slot(slot),
        next_committee_known: store.next.is_some(),
        now_s,
        now_slot,
        age_s,
        chain_finalized_slot,
        lag_s,
        ws_end_slot,
        ws_half_slot,
        divergence,
        providers: verdicts,
        alerts,
    })
}
