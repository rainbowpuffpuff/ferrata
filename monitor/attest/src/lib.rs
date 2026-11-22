//! The attestation side's `monitor` (RFP-021 S.7 and VS.4; `docs/design/attestation.md` §3.6, §8, §11.3).
//! Prototype, unaudited.
//!
//! It watches one instance of the attestation program on LEZ and compares what the instance holds with what
//! several beacon providers report:
//!
//! * **divergence** (`instance-diverged`, critical): the instance's finalized header is not the canonical block at
//!   its slot by the account of at least `diverge_at` providers (default 1: any one of them), whatever the others
//!   report: they have another canonical block there (final for them or not yet), or no block at a slot they have
//!   finalized. This is what a captured or colluding sync committee leaves behind: a forged handoff or finality the
//!   program could not tell apart (`docs/trust.md` §2). The program cannot see it; a consumer wires this alarm to its
//!   freeze authority (the alert command) and sizes its caps to what may pass before the freeze. A provider that
//!   reports the instance's root does not suppress the alarm: it may be colluding with the forger. With `n` providers
//!   counted (they have finalized the slot, or already hold another block there): `f` lying providers raise the alarm
//!   on a healthy instance when `f >= diverge_at`; `f` providers colluding with a forged instance hide it only when
//!   the honest ones counted are fewer than `diverge_at`, that is `f > n - diverge_at`. With `diverge_at` 2 and three
//!   or more providers, one provider can neither raise nor hide it;
//! * **ahead** (`instance-ahead`, critical): the instance claims a finality the chain cannot have, or that no
//!   provider has. `too-new`: the finalized header is younger, by the chain's clock, than the chain's finality delay
//!   (two epochs: Casper FFG finalizes a checkpoint two epochs after it at the earliest, so no honest instance can hold
//!   such a header), whatever the providers say. `ahead-of-providers`: no provider that answers has finalized the
//!   header's slot or holds the header there, although the header is older than the finality delay plus
//!   `ahead_grace_s`: either the instance is not on the providers' chain, or every provider lags by more than the
//!   grace. A forged header kept ahead of the providers' finality is caught by one of these or by the divergence
//!   alarm (`README.md`, "The rule");
//! * **stall** (`instance-stalled`, warning): the instance's finalized header lags the providers' finalized header
//!   by more than `stall_after_s`. Nobody is obliged to advance an instance (S.7): the alarm tells whoever cares that
//!   updates are outstanding. Anyone can submit them, and the instance resumes (R.7);
//! * **weak subjectivity** (`ws-half`, critical): the instance's finalized header is older, by the chain's clock, than
//!   half its weak-subjectivity period (`Config::ws_end_slot`), the point at which the design wants someone to act
//!   (§3.6); `ws-expired` (critical): past the period the program refuses every update and the instance must be
//!   replaced by a fresh one (F.10);
//! * the providers themselves: one that cannot be reached, answers for another chain (its genesis is not the
//!   instance's), or has not finalized the instance's slot (behind or syncing) is reported. One behind the slot is
//!   counted only when it already holds another canonical block there (a disagreement); its 404 there may be a block
//!   it has not seen, and its agreement is not final, so neither counts.
//!
//! [`assess`] is the whole decision, a pure function of the instance's state, the providers' answers and the clock;
//! [`source`] reads the instance (a LEZ sequencer's `getAccount`, a file or a command) and the providers (beacon API,
//! over `curl`); [`watch`] runs the decision as a daemon with alerts, a status file and a clean stop.

use anyhow::{anyhow, Result};
use ferrata_core::{witness::CompactStore, Root};
use ferrata_lez_wire::{Config, InstanceState};
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
    /// The provider's finalized slot (`/eth/v1/beacon/headers/finalized`). A provider behind the instance's finalized
    /// slot counts only when it already holds another canonical block there ([`judge`]).
    pub finalized_slot: Option<u64>,
}

/// The decision's parameters.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Policy {
    /// Agreeing providers needed (with none disagreeing) to confirm a header. Read as 1 when 0.
    pub min_agreeing: usize,
    /// Disagreeing providers that call the instance diverged (`instance-diverged`, critical), whatever the number that
    /// agree. Default 1: any provider that disagrees. 2, with three or more providers: no single provider can raise
    /// the alarm or hide it. Read as 1 when 0.
    pub diverge_at: usize,
    /// The lag of the instance's finalized header behind the providers' finalized header that raises the stall alarm.
    pub stall_after_s: u64,
    /// How long, past the chain's finality delay, the instance's finalized header may be ahead of every provider (none
    /// has finalized its slot or holds it there) before `instance-ahead` is raised: the lag allowed to the providers.
    pub ahead_grace_s: u64,
}

impl Default for Policy {
    fn default() -> Self {
        // Two missed sessions of the lab's two-hour cadence (operator guide §3), plus finality.
        Self { min_agreeing: 2, diverge_at: 1, stall_after_s: 4 * 3600 + 15 * 60, ahead_grace_s: 60 }
    }
}

/// A provider's standing in one poll.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Standing {
    /// Its canonical root at the slot is the instance's finalized root.
    Agrees,
    /// It reports another canonical root at the slot (final for it or not yet), or no block at a slot it has finalized.
    Disagrees,
    /// It has not finalized the slot and holds no other canonical block there: not counted.
    Behind,
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
    /// At least `diverge_at` providers disagree, whatever the number that agree: the instance is not on their chain.
    Diverged,
    /// Fewer than `diverge_at` providers disagree and some agree (only when `diverge_at` is above 1): no verdict.
    Contested,
    /// Too few usable answers for either verdict.
    Unconfirmed,
}

/// Why the instance's finalized header is ahead of the chain's finality (`instance-ahead`).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Ahead {
    /// Younger, by the chain's clock, than the chain's finality delay: no honest instance can hold it.
    TooNew,
    /// No provider that answers has finalized its slot or holds it there, past the finality delay and the grace.
    AheadOfProviders,
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
    /// The providers' finalized slot (the median of those counted, the lower of two), and the instance's lag behind it.
    pub chain_finalized_slot: Option<u64>,
    pub lag_s: Option<u64>,
    /// The weak-subjectivity period of the finalized header: the first slot outside it, and the halfway slot.
    pub ws_end_slot: Option<u64>,
    pub ws_half_slot: Option<u64>,
    pub divergence: Divergence,
    /// The chain's finality delay (two epochs), and whether the header is ahead of the chain's finality.
    pub finality_delay_s: u64,
    pub ahead: Option<Ahead>,
    /// Providers that answered for the slot on the instance's chain (a finalized slot and an answer there), and how
    /// many of them have not finalized it.
    pub providers_answering: usize,
    pub providers_behind: usize,
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

/// The providers' verdict on one finalized header: the instance's current one, or one it showed earlier.
#[derive(Clone, Debug)]
pub struct Judgement {
    pub divergence: Divergence,
    pub verdicts: Vec<ProviderVerdict>,
    /// Providers that answered for the slot on the instance's chain: a finalized slot and an answer at the slot.
    pub answering: usize,
    /// Of those, the ones that have not finalized the slot.
    pub behind: usize,
    /// Of those behind, the ones whose canonical block at the slot is the header (not final for them yet).
    pub holding: usize,
    /// Configured providers not counted (neither agreeing nor disagreeing: behind with nothing else at the slot,
    /// unavailable, on another chain). A verdict reached with one of them may change once it counts.
    pub uncounted: usize,
    /// The finalized slots of the providers that have finalized the slot (for the stall), and the highest finalized
    /// slot any provider on the instance's chain reported.
    pub finalized: Vec<u64>,
    pub highest_finalized: Option<u64>,
    /// What the providers themselves raised (unavailable, another chain, behind).
    pub provider_alerts: Vec<Alert>,
}

/// The providers' `views` of the header `root` at `slot` of an instance of chain `c`.
pub fn judge(c: &Config, slot: u64, root: &Root, views: &[ProviderView], p: &Policy) -> Judgement {
    let mut alerts = Vec::new();
    let mut verdicts = Vec::new();
    let mut finalized = Vec::new();
    let (mut answering, mut behind, mut holding) = (0, 0, 0);
    let mut highest_finalized: Option<u64> = None;
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
        if !on_chain(c, v) {
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
        let Some(f) = v.finalized_slot else {
            verdict.note = Some("no finalized slot".into());
            alerts.push(Alert::new(Level::Warn, "provider-unavailable", "a beacon provider gave no usable answer")
                .with("provider", &v.name)
                .with("error", "no finalized slot"));
            verdicts.push(verdict);
            continue;
        };
        highest_finalized = Some(highest_finalized.map_or(f, |h| h.max(f)));
        if v.root_at_slot.is_some() {
            answering += 1;
        }
        if f >= slot {
            finalized.push(f);
            match &v.root_at_slot {
                None => verdict.note = Some("no answer for the instance's slot".into()),
                Some(Some(r)) => {
                    verdict.root_at_slot = Some(hex0x(r));
                    verdict.standing = if r == root { Standing::Agrees } else { Standing::Disagrees };
                }
                Some(None) => {
                    verdict.standing = Standing::Disagrees;
                    verdict.note = Some("no canonical block at the instance's slot".into());
                }
            }
            verdicts.push(verdict);
            continue;
        }
        // Behind the slot. Its 404 there may be a block it has not seen yet, and its agreement is not final: neither
        // counts. Another canonical block already there does: an honest instance's finalized header is final on the
        // chain, so a provider whose chain holds another block at the slot is on another fork, or the instance is.
        if v.root_at_slot.is_some() {
            behind += 1;
        }
        alerts.push(
            Alert::new(Level::Warn, "provider-behind", "a beacon provider has not finalized the instance's slot; counted only if it holds another block there")
                .with("provider", &v.name)
                .with("finalized_slot", f)
                .with("slot", slot),
        );
        verdict.standing = Standing::Behind;
        match &v.root_at_slot {
            Some(Some(r)) if r != root => {
                verdict.root_at_slot = Some(hex0x(r));
                verdict.standing = Standing::Disagrees;
                verdict.note = Some(format!("finalized slot {f}, before the instance's {slot}; its canonical block at the slot is another"));
            }
            Some(Some(r)) => {
                verdict.root_at_slot = Some(hex0x(r));
                holding += 1;
                verdict.note = Some(format!("finalized slot {f}, before the instance's {slot}; holds the instance's block there, not final for it yet"));
            }
            Some(None) => verdict.note = Some(format!("finalized slot {f}, before the instance's {slot}; no canonical block there (yet)")),
            None => verdict.note = Some(format!("finalized slot {f}, before the instance's {slot}")),
        }
        verdicts.push(verdict);
    }
    let agree = verdicts.iter().filter(|v| v.standing == Standing::Agrees).count();
    let disagree = verdicts.iter().filter(|v| v.standing == Standing::Disagrees).count();
    let uncounted = uncounted(&verdicts);
    // Disagreement decides: an agreeing provider never cancels a disagreeing one (it may collude with a forger).
    let divergence = if disagree >= p.diverge_at.max(1) {
        Divergence::Diverged
    } else if disagree > 0 && agree > 0 {
        Divergence::Contested
    } else if disagree == 0 && agree >= p.min_agreeing.max(1) {
        Divergence::Confirmed
    } else {
        Divergence::Unconfirmed
    };
    Judgement { divergence, verdicts, answering, behind, holding, uncounted, finalized, highest_finalized, provider_alerts: alerts }
}

/// Whether a provider's genesis, as far as it gave one, is the instance's chain's.
fn on_chain(c: &Config, v: &ProviderView) -> bool {
    !(v.genesis_validators_root.is_some_and(|g| g != c.genesis_validators_root) || v.genesis_time.is_some_and(|t| t != c.genesis_time))
}

/// The providers in `views` that a judgement of a header at `slot` counts by what they have finalized: a usable answer,
/// the instance's chain, a finalized slot at or past `slot`.
pub fn finalizers(c: &Config, slot: u64, views: &[ProviderView]) -> Vec<String> {
    views
        .iter()
        .filter(|v| v.error.is_none() && on_chain(c, v) && v.finalized_slot.is_some_and(|f| f >= slot))
        .map(|v| v.name.clone())
        .collect()
}

/// The providers in `verdicts` that are not counted: neither agreeing nor disagreeing.
pub fn uncounted(verdicts: &[ProviderVerdict]) -> usize {
    verdicts.iter().filter(|v| !matches!(v.standing, Standing::Agrees | Standing::Disagrees)).count()
}

/// The chain's finality delay in seconds: two epochs (a checkpoint is finalized two epochs after it at the earliest).
pub fn finality_delay_s(c: &Config) -> u64 {
    2 * c.slots_per_epoch.max(1) * c.seconds_per_slot.max(1)
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
    let delay_s = finality_delay_s(c);

    let mut alerts = Vec::new();
    let id = hex0x(&state.id);
    if !state.active {
        alerts.push(Alert::new(Level::Warn, "instance-inactive", "the instance's initial committee is not adopted yet")
            .with("instance", &id));
    }

    let j = judge(c, slot, &root, views, p);
    alerts.extend(j.provider_alerts.iter().cloned());
    let divergence = j.divergence;
    let diverge_at = p.diverge_at.max(1);
    let names = |s: Standing| -> Vec<String> { j.verdicts.iter().filter(|v| v.standing == s).map(|v| v.name.clone()).collect() };
    let (dissent, assent) = (names(Standing::Disagrees), names(Standing::Agrees));
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
            .with("providers", serde_json::json!(dissent))
            .with("agreeing", serde_json::json!(assent))
            .with("diverge_at", diverge_at),
        ),
        Divergence::Contested => alerts.push(
            Alert::new(Level::Warn, "providers-disagree", "providers disagree about the instance's finalized header, fewer than --diverge-at against it")
                .with("slot", slot)
                .with("agree", assent.len())
                .with("disagree", serde_json::json!(dissent))
                .with("diverge_at", diverge_at),
        ),
        Divergence::Unconfirmed => alerts.push(
            Alert::new(Level::Warn, "instance-unconfirmed", "too few providers answered to confirm the instance's finalized header")
                .with("slot", slot)
                .with("agree", assent.len())
                .with("needed", p.min_agreeing.max(1)),
        ),
    }

    // Ahead of the chain's finality. By the clock alone, whatever any provider says: no honest instance holds a
    // finalized header younger than the finality delay. Or ahead of every provider that answers, past the grace.
    let ahead = if age_s < delay_s {
        Some(Ahead::TooNew)
    } else if j.answering > 0 && j.behind == j.answering && j.holding == 0 && age_s > delay_s.saturating_add(p.ahead_grace_s) {
        Some(Ahead::AheadOfProviders)
    } else {
        None
    };
    match ahead {
        Some(Ahead::TooNew) => alerts.push(
            Alert::new(
                Level::Critical,
                "instance-ahead",
                "the instance's finalized header is younger than the chain's finality delay: no honest instance can hold it; freeze what consumes this instance",
            )
            .with("instance", &id)
            .with("why", Ahead::TooNew)
            .with("slot", slot)
            .with("instance_root", hex0x(&root))
            .with("age_s", age_s)
            .with("finality_delay_s", delay_s)
            .with("now_slot", now_slot),
        ),
        Some(Ahead::AheadOfProviders) => alerts.push(
            Alert::new(
                Level::Critical,
                "instance-ahead",
                "no provider has finalized or holds the instance's finalized header, past the finality delay and the grace: the instance is not on their chain, or every provider lags; freeze what consumes this instance",
            )
            .with("instance", &id)
            .with("why", Ahead::AheadOfProviders)
            .with("slot", slot)
            .with("instance_root", hex0x(&root))
            .with("highest_provider_finalized_slot", j.highest_finalized)
            .with("age_s", age_s)
            .with("finality_delay_s", delay_s)
            .with("ahead_grace_s", p.ahead_grace_s)
            .with("providers", serde_json::json!(names(Standing::Behind).into_iter().chain(dissent.iter().cloned()).collect::<Vec<_>>())),
        ),
        None => {}
    }

    // The stall: against the finalized slot of the providers that have finalized the instance's, the median (the lower
    // of two): with three or more providers, one provider can neither raise nor hide it; with two, either can hide it.
    let mut finalized = j.finalized.clone();
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
        finality_delay_s: delay_s,
        ahead,
        providers_answering: j.answering,
        providers_behind: j.behind,
        providers: j.verdicts,
        alerts,
    })
}
