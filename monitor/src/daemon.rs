//! The monitor's loop: scan the vault's events, read the node views, decide, act, persist.

use std::path::PathBuf;
use std::time::Duration;

use alloy::primitives::Address;
use anyhow::{bail, Result};
use attestor_core::Deployment;
use serde_json::json;

use crate::alert::{Alerts, Level};
use crate::chain::{Chain, Sent, VaultEvent};
use crate::check::{attested_by, audit_attestations, audit_finalized, evaluate, Policy, Rule, Verdict};
use crate::node::{NodeSource, NodeView, H32};
use crate::store::{Status, Store, Tracked};

fn wall_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default()
}

/// The watcher's 12-hour checkpoint pause (the vault's `WATCHER_PAUSE`).
pub const WATCHER_PAUSE: u64 = 12 * 3600;

#[derive(Clone, Debug)]
pub struct Config {
    pub vault: Address,
    pub nodes: Vec<NodeSource>,
    pub state: PathBuf,
    pub start_block: u64,
    pub poll: Duration,
    /// Stop after this many polls (tests and one-shot runs); `None` runs until a signal.
    pub max_polls: Option<u64>,
    pub b_lag_grace: u64,
    pub delta_g: u64,
    pub margin: u64,
    /// Blocks behind the head the scan stays (0 on a local chain).
    pub confirmations: u64,
    /// Blocks rescanned after a restart.
    pub reorg_margin: u64,
    pub log_chunk: u64,
    /// The pause follows a cancellation of a state cancelled this many times (D11: a persistent attacker).
    pub pause_after_cancels: u32,
    /// Pause (rule (A)) and alert when the nodes' final tier has not advanced for this long (0: off).
    pub stall_pause_after: u64,
    /// Decide and alert, send nothing.
    pub dry_run: bool,
    /// The channel id, for the audit of published attestations (rule (B)); `None` skips it.
    pub channel_id: Option<H32>,
    pub chain_id: u64,
}

struct Ctx {
    cfg: Config,
    chain: Chain,
    alerts: Alerts,
    store: Store,
    policy: Policy,
    revoke_window: u64,
    is_watcher: bool,
    is_freeze: bool,
    deployment: Option<Deployment>,
}

fn key(h: &H32) -> String {
    h.to_string()
}

impl Ctx {
    fn save(&self) -> Result<()> {
        self.store.save(&self.cfg.state)
    }

    fn alert_once(&mut self, dedup: String, level: Level, kind: &str, now: u64, fields: serde_json::Value) {
        if self.store.first_time(dedup) {
            self.alerts.emit(level, kind, now, fields);
        }
    }

    fn can_cancel(&self) -> bool {
        !self.cfg.dry_run && (self.is_watcher || self.is_freeze)
    }

    fn apply(&mut self, ev: VaultEvent, now: u64) {
        match ev {
            VaultEvent::Submitted { candidate, block } => {
                let k = key(&candidate.new_state);
                let earlier = self.store.cancels_before(&k, block);
                if earlier > 0 {
                    self.alert_once(
                        format!("resubmitted:{k}:{block}"),
                        Level::Warn,
                        "candidate-resubmitted",
                        now,
                        json!({ "newState": k, "block": block, "cancelledBefore": earlier }),
                    );
                }
                self.store.candidates.insert(
                    k,
                    Tracked {
                        candidate,
                        status: Status::Pending,
                        verdict: None,
                        first_seen_at: now,
                        cancel_tx: None,
                        cancelled_at: None,
                    },
                );
            }
            VaultEvent::Cancelled { new_state, block, tx } => {
                let k = key(&new_state);
                let id = tx.map_or_else(|| format!("block-{block}"), |t| format!("{t:#x}"));
                self.store.cancel_events.entry(k.clone()).or_default().insert(id.clone(), block);
                if let Some(t) = self.store.candidates.get_mut(&k) {
                    if t.cancel_tx.as_deref() != Some(id.as_str()) && t.status == Status::Pending {
                        t.status = Status::Gone;
                    }
                }
            }
            VaultEvent::Finalized { index, candidate, .. } => {
                let k = key(&candidate.new_state);
                if let Some(t) = self.store.candidates.get_mut(&k) {
                    if t.status == Status::Pending {
                        t.status = Status::Gone;
                    }
                }
                if !self.store.finalized.contains_key(&index) {
                    self.store.to_audit.insert(index, candidate);
                }
            }
            VaultEvent::Revoked { from_index, to_index, block } => {
                for i in from_index..=to_index {
                    self.store.to_audit.remove(&i);
                    self.store.finalized.insert(i, "revoked".into());
                }
                self.alert_once(
                    format!("revoked:{from_index}:{block}"),
                    Level::Info,
                    "checkpoints-revoked",
                    now,
                    json!({ "fromIndex": from_index, "toIndex": to_index, "block": block }),
                );
            }
        }
    }

    async fn pause(&mut self, now: u64, why: &str) -> Result<()> {
        if !self.is_watcher || self.cfg.dry_run {
            self.alert_once(
                format!("would-pause:{now}"),
                Level::Critical,
                "would-pause",
                now,
                json!({ "why": why, "note": "only a watcher can pause; the freeze authority can setCheckpointsFrozen" }),
            );
            return Ok(());
        }
        let until = self.chain.checkpoints_frozen_until().await?;
        if until > now + 3600 {
            self.alerts.emit(Level::Info, "already-paused", now, json!({ "why": why, "frozenUntil": until }));
            return Ok(());
        }
        match self.chain.pause().await {
            Ok(Sent::Mined { tx, block, time, via }) => {
                self.store.paused_at = Some(time);
                self.save()?;
                self.alerts.emit(
                    Level::Critical,
                    "paused",
                    now,
                    json!({ "why": why, "tx": format!("{tx:#x}"), "block": block, "until": time + WATCHER_PAUSE,
                            "via": via.to_json(),
                            "next": "the freeze authority decides: setCheckpointsFrozen, or let the pause lapse" }),
                );
            }
            Ok(Sent::Moot) => {}
            Err(e) => {
                self.alerts.emit(Level::Critical, "pause-failed", now, json!({ "why": why, "error": e.to_string() }))
            }
        }
        Ok(())
    }

    async fn poll(&mut self) -> Result<()> {
        let (head, now) = self.chain.head().await?;
        let to = head.saturating_sub(self.cfg.confirmations);
        if self.store.next_block <= to {
            let events = self.chain.events(self.store.next_block, to, self.cfg.log_chunk).await?;
            for ev in events {
                self.apply(ev, now);
            }
            self.store.next_block = to + 1;
            self.save()?;
        }

        let mut views: Vec<NodeView> = Vec::new();
        let mut unreadable = Vec::new();
        for src in &self.cfg.nodes {
            match src.read() {
                Ok(v) => views.push(v),
                Err(e) => unreadable.push((src.label(), format!("{e:#}"))),
            }
        }
        // One alert per node and hour of chain time; a candidate checked meanwhile uses the other nodes,
        // and with none the verdict is `Blind` (critical).
        for (node, error) in unreadable {
            self.alert_once(
                format!("node-unreadable:{node}:{}", now / 3600),
                Level::Warn,
                "node-unreadable",
                now,
                json!({ "node": node, "error": error }),
            );
        }
        self.watch_stall(&views, now).await?;
        self.audit_attestations(&views, now);
        self.check_pending(&views, now).await?;
        self.audit_finalized(&views, now).await?;
        Ok(())
    }

    async fn watch_stall(&mut self, views: &[NodeView], now: u64) -> Result<()> {
        let Some(height) = views.iter().map(|v| v.final_height).min() else { return Ok(()) };
        if height > self.store.final_height || self.store.final_height_since == 0 {
            self.store.final_height = height;
            self.store.final_height_since = now;
            return Ok(());
        }
        let after = self.cfg.stall_pause_after;
        if after == 0 || now < self.store.final_height_since + after {
            return Ok(());
        }
        let since = self.store.final_height_since;
        self.alert_once(
            format!("stall:{height}:{since}"),
            Level::Critical,
            "final-tier-stalled",
            now,
            json!({ "finalHeight": height, "since": since, "seconds": now - since }),
        );
        let recently = self.store.paused_at.is_some_and(|t| now < t + WATCHER_PAUSE);
        if self.policy.rule == Rule::A && !recently {
            self.pause(now, "final tier stalled").await?;
        }
        Ok(())
    }

    fn audit_attestations(&mut self, views: &[NodeView], now: u64) {
        let Some(d) = self.deployment else { return };
        for v in views {
            for f in audit_attestations(v, &d) {
                if f.is_good() {
                    continue;
                }
                let level = if f.is_misbehaviour() { Level::Critical } else { Level::Warn };
                let dedup = format!("attestation:{}:{}", v.node, serde_json::to_string(&f).unwrap_or_default());
                self.alert_once(dedup, level, "attestation-finding", now, json!({ "node": v.node, "finding": f }));
            }
        }
    }

    async fn check_pending(&mut self, views: &[NodeView], now: u64) -> Result<()> {
        let pending: Vec<String> =
            self.store.candidates.iter().filter(|(_, t)| t.status == Status::Pending).map(|(k, _)| k.clone()).collect();
        if pending.is_empty() {
            return Ok(());
        }
        let final_state = self.chain.final_state().await?;
        for k in pending {
            let Some(mut t) = self.store.candidates.get(&k).cloned() else { continue };
            let Some(onchain) = self.chain.pending(t.candidate.new_state).await? else {
                t.status = Status::Gone;
                self.store.candidates.insert(k, t);
                continue;
            };
            t.candidate.submitted_at = onchain.submitted_at;
            t.candidate.delta_at_submit = onchain.delta_at_submit;
            if t.candidate.parent_state != final_state {
                t.status = Status::Stale;
                self.store.candidates.insert(k, t);
                continue;
            }
            let c = t.candidate.clone();
            let deadline = c.deadline(self.policy.turn_seconds);
            let verdict = evaluate(&self.policy, &c, views, now);
            let base = json!({ "newState": k, "tipId": c.tip_id, "tipHash": c.tip_hash, "n": c.n, "t": c.t,
                               "submittedAt": c.submitted_at, "deadline": deadline });
            match &verdict {
                Verdict::Pass => {
                    let attested: Vec<_> = match self.deployment {
                        Some(d) => views
                            .iter()
                            .map(|v| json!({ "node": v.node, "attestedBy": attested_by(v, &d, c.tip_id, &c.tip_hash) }))
                            .collect(),
                        None => Vec::new(),
                    };
                    let mut f = base.clone();
                    f["attestations"] = json!(attested);
                    self.alert_once(format!("pass:{k}:{}", c.submitted_at), Level::Info, "candidate-passed", now, f);
                }
                Verdict::Wait { until, why } => {
                    let mut f = base.clone();
                    f["until"] = json!(until);
                    f["why"] = json!(why);
                    self.alert_once(format!("wait:{k}:{}", c.submitted_at), Level::Info, "candidate-waiting", now, f);
                }
                Verdict::Blind => {
                    self.alert_once(
                        format!("blind:{k}:{}", c.submitted_at),
                        Level::Critical,
                        "blind",
                        now,
                        base.clone(),
                    );
                }
                Verdict::Cancel { reason } => {
                    let mut f = base.clone();
                    f["reason"] = json!(reason);
                    if now >= deadline {
                        self.alert_once(
                            format!("late:{k}:{}", c.submitted_at),
                            Level::Critical,
                            "late",
                            now,
                            f.clone(),
                        );
                    }
                    if !self.can_cancel() {
                        self.alert_once(
                            format!("would-cancel:{k}:{}", c.submitted_at),
                            Level::Critical,
                            "would-cancel",
                            now,
                            f,
                        );
                    } else {
                        let earlier = self.store.cancels_before(&k, u64::MAX);
                        match self.chain.cancel(c.new_state).await {
                            Ok(Sent::Mined { tx, block, time, via }) => {
                                t.status = Status::CancelledByUs;
                                t.cancel_tx = Some(format!("{tx:#x}"));
                                t.cancelled_at = Some(time);
                                t.verdict = Some(verdict.clone());
                                self.store.candidates.insert(k.clone(), t.clone());
                                self.save()?;
                                f["tx"] = json!(format!("{tx:#x}"));
                                f["block"] = json!(block);
                                f["cancelledAt"] = json!(time);
                                f["secondsAfterSubmission"] = json!(time.saturating_sub(c.submitted_at));
                                f["secondsBeforeDeadline"] = json!(i128::from(deadline) - i128::from(time));
                                f["via"] = via.to_json();
                                self.alerts.emit(Level::Critical, "cancelled", now, f);
                                if u32::try_from(earlier + 1).unwrap_or(u32::MAX) >= self.cfg.pause_after_cancels {
                                    self.alerts.emit(
                                        Level::Critical,
                                        "persistent-candidate",
                                        now,
                                        json!({ "newState": k, "cancellations": earlier + 1,
                                                "next": "freeze authority: setCheckpointsFrozen(true)" }),
                                    );
                                    self.pause(now, "a cancelled state was submitted again").await?;
                                }
                                continue;
                            }
                            Ok(Sent::Moot) => {
                                t.status = Status::Gone;
                            }
                            Err(e) => {
                                // Retried at every poll; paged once per 5 minutes of chain time.
                                f["error"] = json!(format!("{e:#}"));
                                self.alert_once(
                                    format!("cancel-failed:{k}:{}", now / 300),
                                    Level::Critical,
                                    "cancel-failed",
                                    now,
                                    f,
                                );
                            }
                        }
                    }
                }
            }
            t.verdict = Some(verdict);
            self.store.candidates.insert(k, t);
        }
        Ok(())
    }

    async fn audit_finalized(&mut self, views: &[NodeView], now: u64) -> Result<()> {
        if views.is_empty() {
            return Ok(());
        }
        let todo: Vec<u64> = self.store.to_audit.keys().copied().collect();
        for index in todo {
            let Some(c) = self.store.to_audit.get(&index).cloned() else { continue };
            let Some(reason) = audit_finalized(&c, views, now) else {
                self.store.to_audit.remove(&index);
                self.store.finalized.insert(index, "ok".into());
                continue;
            };
            let (revoked, finalized_at) = self.chain.checkpoint(index).await?;
            if revoked {
                self.store.to_audit.remove(&index);
                self.store.finalized.insert(index, "revoked".into());
                continue;
            }
            let revoke_by = finalized_at + self.revoke_window;
            let f = json!({ "index": index, "newState": key(&c.new_state), "tipId": c.tip_id, "tipHash": c.tip_hash,
                            "reason": reason, "finalizedAt": finalized_at, "revokeBy": revoke_by,
                            "action": format!("revokeFrom({index}) by the freeze authority before {revoke_by}") });
            self.alert_once(
                format!("finalized-wrong:{index}:{finalized_at}"),
                Level::Critical,
                "finalized-wrong",
                now,
                f.clone(),
            );
            if !self.is_freeze || self.cfg.dry_run {
                continue; // the multisig acts on the alert; audited again next poll until revoked
            }
            match self.chain.revoke_from(index).await {
                Ok(Sent::Mined { tx, block, time, via }) => {
                    self.store.to_audit.remove(&index);
                    self.store.finalized.insert(index, "revoked".into());
                    self.save()?;
                    let mut g = f;
                    g["tx"] = json!(format!("{tx:#x}"));
                    g["block"] = json!(block);
                    g["revokedAt"] = json!(time);
                    g["via"] = via.to_json();
                    self.alerts.emit(Level::Critical, "revoked", now, g);
                }
                Ok(Sent::Moot) => {
                    self.store.to_audit.remove(&index);
                    self.store.finalized.insert(index, "revoked".into());
                }
                Err(e) => self.alert_once(
                    format!("revoke-failed:{index}:{}", now / 300),
                    Level::Critical,
                    "revoke-failed",
                    now,
                    json!({ "index": index, "error": format!("{e:#}") }),
                ),
            }
        }
        Ok(())
    }
}

/// Runs the monitor until `max_polls` or a signal (SIGINT, SIGTERM); the state file is current at every exit.
/// Runs until SIGTERM or SIGINT (or `max_polls`): the binary.
pub async fn run(cfg: Config, chain: Chain, alerts: Alerts) -> Result<()> {
    run_with(cfg, chain, alerts, None).await
}

/// [`run`]; with `stop`, the loop ends when `stop` turns true (or its sender is dropped) instead of on
/// a signal, and no signal handler is installed: the Logos module, whose process's signals are the
/// host's.
pub async fn run_with(
    cfg: Config,
    chain: Chain,
    alerts: Alerts,
    mut stop: Option<tokio::sync::watch::Receiver<bool>>,
) -> Result<()> {
    let rule = chain.rule().await?;
    let turn_seconds = chain.turn_seconds().await?;
    let revoke_window = chain.revoke_window().await?;
    let freeze = chain.freeze_authority().await?;
    let watchers = chain.watchers().await?;
    let me = chain.me;
    let is_watcher = me.is_some_and(|m| watchers.contains(&m));
    let is_freeze = me == Some(freeze);
    let deployment = match cfg.channel_id {
        Some(channel) => {
            let mut evm_chain_id = [0u8; 32];
            evm_chain_id[24..].copy_from_slice(&cfg.chain_id.to_be_bytes());
            Some(Deployment {
                evm_chain_id,
                vault: cfg.vault.into_array(),
                lez_config_id: chain.lez_config_id().await?.0,
                channel_id: channel.0,
            })
        }
        None => None,
    };
    let vault_label = format!("{:#x}", cfg.vault);
    // One monitor per state file (and so per key): an OS lock, released when the process ends, even by SIGKILL.
    let lock_path = cfg.state.with_extension("lock");
    let lock = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(&lock_path)?;
    if let Err(e) = lock.try_lock() {
        bail!("{} is held by another monitor ({e:?})", lock_path.display());
    }
    let (mut store, resumed) = Store::load_or_new(&cfg.state, &vault_label, cfg.start_block)?;
    if resumed {
        store.next_block = store.next_block.saturating_sub(cfg.reorg_margin).max(cfg.start_block);
    }
    let policy = Policy { rule, turn_seconds, b_lag_grace: cfg.b_lag_grace, delta_g: cfg.delta_g, margin: cfg.margin };
    let (_, now) = chain.head().await?;
    alerts.emit(
        Level::Info,
        "started",
        now,
        json!({ "vault": vault_label, "rule": rule, "me": me.map(|m| format!("{m:#x}")), "isWatcher": is_watcher,
                "isFreezeAuthority": is_freeze, "dryRun": cfg.dry_run, "resumed": resumed, "nextBlock": store.next_block,
                "pendingKnown": store.candidates.values().filter(|t| t.status == Status::Pending).count(),
                "policy": policy, "nodes": cfg.nodes.iter().map(NodeSource::label).collect::<Vec<_>>(),
                "sendVia": chain.logos.as_ref().map_or_else(
                    || json!("plain"),
                    |r| json!({ "path": "logos", "module": crate::logos_send::TX_SENDER, "from": format!("{:#x}", r.from),
                                "fallback": r.fallback }),
                ) }),
    );
    if !is_watcher && !is_freeze {
        alerts.emit(
            Level::Warn,
            "not-a-watcher",
            now,
            json!({ "me": me.map(|m| format!("{m:#x}")), "note": "decides and alerts only" }),
        );
    }
    let mut ctx = Ctx { cfg, chain, alerts, store, policy, revoke_window, is_watcher, is_freeze, deployment };
    ctx.save()?;

    let mut term = match stop {
        None => Some(tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?),
        Some(_) => None,
    };
    let mut polls = 0u64;
    let mut failing_since: Option<u64> = None;
    loop {
        match ctx.poll().await {
            Ok(()) => failing_since = None,
            Err(e) => {
                // Wall clock here: the chain may be what fails. One alert per 10 minutes; critical once
                // the polls have failed for 5 minutes (the watcher cannot see new candidates).
                let wall = wall_secs();
                let since = *failing_since.get_or_insert(wall);
                let level = if wall - since >= 300 { Level::Critical } else { Level::Warn };
                let (_, now) = ctx.chain.head().await.unwrap_or((0, 0));
                ctx.alert_once(
                    format!("poll-failed:{}:{level:?}", wall / 600),
                    level,
                    "poll-failed",
                    now,
                    json!({ "error": format!("{e:#}"), "failingForSeconds": wall - since }),
                );
            }
        }
        ctx.store.polls += 1;
        ctx.save()?;
        polls += 1;
        if ctx.cfg.max_polls.is_some_and(|m| polls >= m) {
            break;
        }
        match (stop.as_mut(), term.as_mut()) {
            (Some(rx), _) => tokio::select! {
                () = tokio::time::sleep(ctx.cfg.poll) => {}
                _ = rx.wait_for(|stopped| *stopped) => break,
            },
            (None, Some(term)) => tokio::select! {
                () = tokio::time::sleep(ctx.cfg.poll) => {}
                _ = tokio::signal::ctrl_c() => break,
                _ = term.recv() => break,
            },
            (None, None) => unreachable!("a signal handler is installed when there is no stop channel"),
        }
    }
    let (_, now) = ctx.chain.head().await.unwrap_or((0, 0));
    ctx.alerts.emit(Level::Info, "stopped", now, json!({ "polls": polls, "nextBlock": ctx.store.next_block }));
    let saved = ctx.save();
    drop(lock);
    saved
}
