//! The decision: what a watcher does with a candidate, given its own nodes' views (pure; no I/O).
//!
//! SPEC-v3 D4, D11 and D13 (`docs/design/spec/bridge-spec-v3-changes.md`):
//! - rule (B): a candidate's tip must already be in the watcher's final tier when it is submitted;
//!   the monitor allows its own node a short lag (`b_lag_grace`) and then cancels;
//! - rule (A): the tip must be final by `submittedAt + Δ_g`;
//! - both rules: a candidate whose key set is not the live one at its tip is cancelled (D13: under
//!   (A) a stale, smaller set would shorten the wait; under (B) the attestors sign their live set);
//!   a final block at the tip's id with another hash is a conflicting history and is cancelled at once;
//!   when the node computes the checkpoint fields at the tip, every field must match.
//!
//! A cancellation is decided no later than the candidate's deadline minus a margin, so that the
//! transaction can still land before the wait ends.

use attestor_core::{Deployment, SignedAttestation, VerifyError, SIGNED_ATTESTATION_LEN};
use serde::{Deserialize, Serialize};

use crate::node::{NodeView, TipStatus, H32};

/// The vault's finality rule (its immutable `mode`: 0 is A, 1 is B).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rule {
    A,
    B,
}

/// A pending candidate as the vault published it (`CandidateSubmitted`) and stores it (`candidates`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub new_state: H32,
    pub parent_state: H32,
    pub burn_root: H32,
    pub burn_count: u64,
    pub imported_leaf_count: u64,
    pub sunset: bool,
    pub delta_at_submit: u64,
    pub tip_id: u64,
    pub tip_hash: H32,
    pub n: u32,
    pub t: u32,
    /// `candidates(newState).submittedAt`, the block time of the submission.
    pub submitted_at: u64,
}

impl Candidate {
    /// When it can be finalised: `submittedAt + deltaAtSubmit + (t − 1) × turnSeconds` (D4; turnSeconds is 0 under (B)).
    pub fn deadline(&self, turn_seconds: u64) -> u64 {
        self.submitted_at + self.delta_at_submit + u64::from(self.t.saturating_sub(1)) * turn_seconds
    }
}

/// The watcher's parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Policy {
    pub rule: Rule,
    /// The vault's `turnSeconds` (0 under (B)).
    pub turn_seconds: u64,
    /// Rule (B): how long after the submission the watcher's own node may still lack the tip in its
    /// final tier before the candidate is cancelled (node lag; D13 checks at submission).
    pub b_lag_grace: u64,
    /// Rule (A): Δ_g, the time by which an honest tip is final in every honest node.
    pub delta_g: u64,
    /// Seconds before the deadline by which a cancellation must be sent.
    pub margin: u64,
}

/// Why a candidate is cancelled (or a finalised checkpoint should be revoked).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Reason {
    /// The node's final block at the tip's id has another hash.
    #[serde(rename_all = "camelCase")]
    TipConflict { node: String, tip_id: u64, final_hash: H32 },
    /// The tip is not in the node's final tier by the time the rule allows.
    #[serde(rename_all = "camelCase")]
    TipNotFinal { node: String, tip_id: u64, final_height: u64, checked_at: u64 },
    /// `n` or `t` is not the live key set's at the tip.
    #[serde(rename_all = "camelCase")]
    KeySetNotLive { node: String, live_n: u32, live_t: u32, candidate_n: u32, candidate_t: u32 },
    /// A checkpoint field differs from the node's own computation at the tip.
    #[serde(rename_all = "camelCase")]
    StateMismatch { node: String, field: String, node_value: String, candidate_value: String },
}

/// What to do with a candidate now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "camelCase")]
pub enum Verdict {
    /// Every node holds the tip final with its hash, under the live key set: leave it alone.
    Pass,
    /// Not decided: look again; cancel if still undecided at `until`.
    Wait { until: u64, why: String },
    /// Cancel now.
    Cancel { reason: Reason },
    /// No node view could be read: the watcher cannot check.
    Blind,
}

/// The checks that refuse a candidate at once, for one node; `None` if none applies.
fn contradiction(c: &Candidate, v: &NodeView) -> Option<Reason> {
    match v.tip_status(c.tip_id, &c.tip_hash) {
        TipStatus::Conflict(h) => {
            return Some(Reason::TipConflict { node: v.node.clone(), tip_id: c.tip_id, final_hash: h });
        }
        TipStatus::Final => {}
        // The key set at a tip the node has not finalised could still change; checked once it is final.
        TipStatus::NotFinal | TipStatus::Missing => return None,
    }
    if let Some(ks) = v.live_key_set(c.tip_id) {
        if ks.n() != c.n || ks.t() != c.t {
            return Some(Reason::KeySetNotLive {
                node: v.node.clone(),
                live_n: ks.n(),
                live_t: ks.t(),
                candidate_n: c.n,
                candidate_t: c.t,
            });
        }
    }
    if let Some(st) = v.states.get(&c.tip_id) {
        let mismatch = |field: &str, node_value: String, candidate_value: String| {
            Some(Reason::StateMismatch { node: v.node.clone(), field: field.into(), node_value, candidate_value })
        };
        if let Some(ns) = st.new_state {
            if ns != c.new_state {
                return mismatch("newState", ns.to_string(), c.new_state.to_string());
            }
        }
        if st.burn_root != c.burn_root {
            return mismatch("burnRoot", st.burn_root.to_string(), c.burn_root.to_string());
        }
        if st.burn_count != c.burn_count {
            return mismatch("burnCount", st.burn_count.to_string(), c.burn_count.to_string());
        }
        if st.imported_leaf_count != c.imported_leaf_count {
            return mismatch(
                "importedLeafCount",
                st.imported_leaf_count.to_string(),
                c.imported_leaf_count.to_string(),
            );
        }
        if st.sunset != c.sunset {
            return mismatch("sunset", st.sunset.to_string(), c.sunset.to_string());
        }
    }
    None
}

/// The verdict on a pending candidate at chain time `now`, from every node view the watcher could read.
pub fn evaluate(p: &Policy, c: &Candidate, views: &[NodeView], now: u64) -> Verdict {
    if views.is_empty() {
        return Verdict::Blind;
    }
    if let Some(reason) = views.iter().find_map(|v| contradiction(c, v)) {
        return Verdict::Cancel { reason };
    }
    let Some(lagging) = views.iter().find(|v| v.tip_status(c.tip_id, &c.tip_hash) != TipStatus::Final) else {
        return Verdict::Pass;
    };
    let allowed = match p.rule {
        Rule::B => c.submitted_at + p.b_lag_grace,
        Rule::A => c.submitted_at + p.delta_g,
    };
    let latest = c.deadline(p.turn_seconds).saturating_sub(p.margin);
    let until = allowed.min(latest);
    if now < until {
        return Verdict::Wait {
            until,
            why: format!("tip {} not yet final in {} (final height {})", c.tip_id, lagging.node, lagging.final_height),
        };
    }
    Verdict::Cancel {
        reason: Reason::TipNotFinal {
            node: lagging.node.clone(),
            tip_id: c.tip_id,
            final_height: lagging.final_height,
            checked_at: now,
        },
    }
}

/// A checkpoint the vault has finalised, checked after the fact (the watcher was down, or late): the
/// wait has passed, so an honest tip is final in every node. `Some` means it should be revoked.
pub fn audit_finalized(c: &Candidate, views: &[NodeView], now: u64) -> Option<Reason> {
    if let Some(reason) = views.iter().find_map(|v| contradiction(c, v)) {
        return Some(reason);
    }
    views.iter().find(|v| v.tip_status(c.tip_id, &c.tip_hash) != TipStatus::Final).map(|v| Reason::TipNotFinal {
        node: v.node.clone(),
        tip_id: c.tip_id,
        final_height: v.final_height,
        checked_at: now,
    })
}

/// What the monitor found in one published final-tier attestation (rule (B)).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "finding", rename_all = "camelCase")]
pub enum AttestationFinding {
    /// Certificate and message verify under the live key set, and the tip is final in the node.
    #[serde(rename_all = "camelCase")]
    Good { channel_key: H32, tip_id: u64 },
    /// Not 264 bytes of hex.
    Malformed { detail: String },
    /// The channel key is not in the key set live at the tip: its certificate is dead.
    #[serde(rename_all = "camelCase")]
    ChannelKeyNotLive { channel_key: H32, tip_id: u64 },
    /// The channel key did not certify this attestation key for this deployment.
    #[serde(rename_all = "camelCase")]
    BadCertificate { channel_key: H32, att_key: H32 },
    /// The attestation key's signature does not verify over the final-tier message v2 with the live
    /// key set (a stale key set, another deployment, or a forgery).
    #[serde(rename_all = "camelCase")]
    BadSignature { channel_key: H32, att_key: H32, tip_id: u64 },
    /// A certified key signed a tip that conflicts with the node's final tier: a corrupted attestor.
    #[serde(rename_all = "camelCase")]
    ConflictingTip { channel_key: H32, att_key: H32, tip_id: u64, signed_hash: H32, final_hash: H32 },
    /// A certified key signed a tip the node has not finalised (early, or the node lags).
    #[serde(rename_all = "camelCase")]
    TipNotFinal { channel_key: H32, tip_id: u64, final_height: u64 },
}

impl AttestationFinding {
    /// Whether the finding is evidence against an attestor (alerted as critical).
    pub fn is_misbehaviour(&self) -> bool {
        matches!(self, Self::ConflictingTip { .. })
    }
    pub fn is_good(&self) -> bool {
        matches!(self, Self::Good { .. })
    }
}

/// Checks each published attestation's certificate and final-tier message with the attestor core,
/// against the node's live key set and final tier.
pub fn audit_attestations(view: &NodeView, d: &Deployment) -> Vec<AttestationFinding> {
    view.attestations.iter().map(|hexed| audit_one(view, d, hexed)).collect()
}

fn audit_one(view: &NodeView, d: &Deployment, hexed: &str) -> AttestationFinding {
    let raw = match hex::decode(hexed.strip_prefix("0x").unwrap_or(hexed)) {
        Ok(b) => b,
        Err(e) => return AttestationFinding::Malformed { detail: e.to_string() },
    };
    let Ok(bytes) = <[u8; SIGNED_ATTESTATION_LEN]>::try_from(raw.as_slice()) else {
        return AttestationFinding::Malformed { detail: format!("{} bytes, not {SIGNED_ATTESTATION_LEN}", raw.len()) };
    };
    let sa = SignedAttestation::decode(&bytes);
    let channel_key = H32(sa.channel_key);
    let att_key = H32(sa.att_key);
    let tip_id = sa.tip.tip_id;
    let Some(ks) = view.live_key_set(tip_id).filter(|ks| ks.keys.contains(&channel_key)) else {
        return AttestationFinding::ChannelKeyNotLive { channel_key, tip_id };
    };
    match sa.verify(d, &ks.hash()) {
        Err(VerifyError::Certificate) => return AttestationFinding::BadCertificate { channel_key, att_key },
        Err(VerifyError::Attestation) => return AttestationFinding::BadSignature { channel_key, att_key, tip_id },
        Ok(()) => {}
    }
    let signed_hash = H32(sa.tip.tip_hash);
    match view.tip_status(tip_id, &signed_hash) {
        TipStatus::Final => AttestationFinding::Good { channel_key, tip_id },
        TipStatus::Conflict(final_hash) => {
            AttestationFinding::ConflictingTip { channel_key, att_key, tip_id, signed_hash, final_hash }
        }
        TipStatus::NotFinal | TipStatus::Missing => {
            AttestationFinding::TipNotFinal { channel_key, tip_id, final_height: view.final_height }
        }
    }
}

/// Valid attestations of exactly this tip (distinct channel keys) in a view: what a rule (B) proof
/// of it needed, as far as this node can see.
pub fn attested_by(view: &NodeView, d: &Deployment, tip_id: u64, tip_hash: &H32) -> u32 {
    let mut keys: Vec<H32> = Vec::new();
    for hexed in &view.attestations {
        if let AttestationFinding::Good { channel_key, tip_id: id } = audit_one(view, d, hexed) {
            let raw = hex::decode(hexed.strip_prefix("0x").unwrap_or(hexed)).unwrap_or_default();
            let same_hash = raw.len() == SIGNED_ATTESTATION_LEN && raw[136..168] == tip_hash.0;
            if id == tip_id && same_hash && !keys.contains(&channel_key) {
                keys.push(channel_key);
            }
        }
    }
    u32::try_from(keys.len()).unwrap_or(u32::MAX)
}
