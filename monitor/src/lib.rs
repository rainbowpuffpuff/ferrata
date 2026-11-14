//! `monitor`: the watchers' node-side program for the Ferrata vault (SPEC-v3 D4, D11, D13;
//! `docs/guides/operator-guide.md` §7). Prototype, unaudited.
//!
//! For each candidate checkpoint submitted to the vault, the monitor checks it against its own
//! nodes' views ([`node`]) and cancels it when the design says so ([`check`]): under rule (B) when its
//! tip is not already in the watcher's final tier (after a short allowance for node lag) or conflicts
//! with it; under rule (A) when its tip is not final by `submittedAt + Δ_g`; under both when its key
//! set is not the live one at its tip, or a checkpoint field differs from the node's own computation.
//! A state cancelled and submitted again brings the watcher's 12-hour checkpoint pause; a wrong
//! checkpoint that was finalised anyway (the watcher was down) raises the revocation alert, and is
//! revoked when the monitor's key is the freeze authority's. Under rule (B) it also audits the
//! attestors' published final-tier messages and certificates with the attestor core.
//!
//! Restart safety ([`store`]): the vault is the source of truth; the state file only adds counts and
//! deduplication, and is rewritten atomically.

pub mod alert;
pub mod chain;
pub mod check;
pub mod cli;
pub mod daemon;
pub mod logos_send;
pub mod node;
pub mod store;
