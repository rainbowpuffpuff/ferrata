//! Weak subjectivity: `consensus-specs` v1.7.0-beta.4, `specs/phase0/weak-subjectivity.md` and
//! `specs/electra/weak-subjectivity.md`.
//!
//! A light client trusts the sync committee of its newest finalized header. Once that header is
//! older than the weak-subjectivity period, the validators behind that committee may have exited
//! and withdrawn, and their signatures no longer stand on slashable stake: a store that old must
//! not be advanced, whatever its committee signs. The recovery is a new bootstrap from a trusted
//! checkpoint inside the period, which is a new trust decision (`docs/design/attestation.md`
//! §3.6).
//!
//! The period depends on the total active balance of the checkpoint's beacon state, which no
//! light-client object commits to. It is computed off chain from that state with
//! [`compute_weak_subjectivity_period`] (Electra and later) or
//! [`compute_weak_subjectivity_period_phase0`] (before Electra), and fixed in an instance's
//! configuration. The check, [`is_within_weak_subjectivity_period`], compares the epoch of the
//! current slot with the epoch of the store's finalized header, which stands for the spec's
//! `ws_state`.
//!
//! **What a clock is needed for.** The spec's period rule keeps an update's signature slot within
//! two sync-committee periods of the store's finalized slot (`PeriodSkipped`), 512 epochs on
//! mainnet, and the period is at least `MIN_VALIDATOR_WITHDRAWABILITY_DELAY` (256 epochs), about
//! 3,500 on mainnet. So comparing an update's own signature slot with the store is vacuous on
//! mainnet: the bound needs the current slot from a clock. Off chain the caller passes its clock's
//! slot to [`crate::policy::FinalizedStore::process_update`]. The LEZ program has no trusted
//! clock in its guest; it makes each transaction that extends an instance valid only before
//! [`weak_subjectivity_end_slot`] of the instance's finalized header starts, by the sequencer's
//! clock (the transaction's timestamp validity window).

use core::cmp::max;

use crate::{fork::ChainSpec, Error};

/// `ETH_TO_GWEI`.
pub const ETH_TO_GWEI: u64 = 1_000_000_000;

/// `SAFETY_DECAY` (phase 0 weak-subjectivity configuration): the tolerated loss, in percent, of
/// the one-third safety margin of FFG finality.
pub const SAFETY_DECAY: u64 = 10;

/// The configuration and preset values the period is computed from (`configs/*.yaml`,
/// `presets/*/phase0.yaml` of the same release).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WsConfig {
    /// `MIN_VALIDATOR_WITHDRAWABILITY_DELAY`, epochs.
    pub min_validator_withdrawability_delay: u64,
    /// `CHURN_LIMIT_QUOTIENT`.
    pub churn_limit_quotient: u64,
    /// `MIN_PER_EPOCH_CHURN_LIMIT_ELECTRA`, Gwei.
    pub min_per_epoch_churn_limit_electra: u64,
    /// `MIN_PER_EPOCH_CHURN_LIMIT` (phase 0), validators.
    pub min_per_epoch_churn_limit: u64,
    /// `MAX_EFFECTIVE_BALANCE` (phase 0), Gwei.
    pub max_effective_balance: u64,
    /// `EFFECTIVE_BALANCE_INCREMENT`, Gwei.
    pub effective_balance_increment: u64,
    /// `MAX_DEPOSITS`.
    pub max_deposits: u64,
    /// `SLOTS_PER_EPOCH`.
    pub slots_per_epoch: u64,
    /// `SAFETY_DECAY`.
    pub safety_decay: u64,
}

impl WsConfig {
    /// `configs/mainnet.yaml` and `presets/mainnet` (also Sepolia's and Hoodi's values).
    pub const MAINNET: WsConfig = WsConfig {
        min_validator_withdrawability_delay: 256,
        churn_limit_quotient: 65_536,
        min_per_epoch_churn_limit_electra: 128_000_000_000,
        min_per_epoch_churn_limit: 4,
        max_effective_balance: 32_000_000_000,
        effective_balance_increment: 1_000_000_000,
        max_deposits: 16,
        slots_per_epoch: 32,
        safety_decay: SAFETY_DECAY,
    };

    /// `configs/minimal.yaml` and `presets/minimal`.
    pub const MINIMAL: WsConfig = WsConfig {
        min_validator_withdrawability_delay: 256,
        churn_limit_quotient: 32,
        min_per_epoch_churn_limit_electra: 64_000_000_000,
        min_per_epoch_churn_limit: 2,
        max_effective_balance: 32_000_000_000,
        effective_balance_increment: 1_000_000_000,
        max_deposits: 16,
        slots_per_epoch: 8,
        safety_decay: SAFETY_DECAY,
    };
}

/// `get_balance_churn_limit` (Electra), from the state's total active balance in Gwei. `None`
/// where the spec's arithmetic fails (a zero quotient or increment).
pub fn get_balance_churn_limit(total_active_balance: u64, c: &WsConfig) -> Option<u64> {
    let churn = max(c.min_per_epoch_churn_limit_electra, total_active_balance.checked_div(c.churn_limit_quotient)?);
    churn.checked_sub(churn.checked_rem(c.effective_balance_increment)?)
}

/// `compute_weak_subjectivity_period` as modified in Electra, from the state's total active
/// balance in Gwei: `MIN_VALIDATOR_WITHDRAWABILITY_DELAY + SAFETY_DECAY * t // (2 * delta * 100)`
/// with `delta = get_balance_churn_limit(state)`. `None` where the spec's `uint64` arithmetic
/// raises (an overflow or a division by zero).
pub fn compute_weak_subjectivity_period(total_active_balance: u64, c: &WsConfig) -> Option<u64> {
    let t = total_active_balance;
    let delta = get_balance_churn_limit(t, c)?;
    let epochs_for_validator_set_churn =
        c.safety_decay.checked_mul(t)?.checked_div(delta.checked_mul(2)?.checked_mul(100)?)?;
    c.min_validator_withdrawability_delay.checked_add(epochs_for_validator_set_churn)
}

/// `get_validator_churn_limit` (phase 0), from the number of active validators.
pub fn get_validator_churn_limit(active_validators: u64, c: &WsConfig) -> Option<u64> {
    Some(max(c.min_per_epoch_churn_limit, active_validators.checked_div(c.churn_limit_quotient)?))
}

/// Phase 0's `compute_weak_subjectivity_period` (the period of a state before Electra), from the
/// number of active validators `N` and their total active balance in Gwei. `None` where the
/// spec's `uint64` arithmetic raises.
pub fn compute_weak_subjectivity_period_phase0(
    active_validators: u64,
    total_active_balance: u64,
    c: &WsConfig,
) -> Option<u64> {
    let n = active_validators;
    let t = total_active_balance.checked_div(n)?.checked_div(ETH_TO_GWEI)?;
    let big_t = c.max_effective_balance.checked_div(ETH_TO_GWEI)?;
    let delta = get_validator_churn_limit(n, c)?;
    let big_delta = c.max_deposits.checked_mul(c.slots_per_epoch)?;
    let d = c.safety_decay;
    let lhs = t.checked_mul(d.checked_mul(12)?.checked_add(200)?)?;
    let rhs = big_t.checked_mul(d.checked_mul(3)?.checked_add(200)?)?;
    let extra = if lhs > rhs {
        let churn = n
            .checked_mul(lhs - rhs)?
            .checked_div(delta.checked_mul(600)?.checked_mul(t.checked_mul(2)?.checked_add(big_t)?)?)?;
        let top_ups = n.checked_mul(d.checked_mul(3)?.checked_add(200)?)?.checked_div(big_delta.checked_mul(600)?)?;
        max(churn, top_ups)
    } else {
        n.checked_mul(3)?
            .checked_mul(d)?
            .checked_mul(t)?
            .checked_div(big_delta.checked_mul(200)?.checked_mul(big_t.checked_sub(t)?)?)?
    };
    c.min_validator_withdrawability_delay.checked_add(extra)
}

/// `is_within_weak_subjectivity_period`, with the header at `ws_slot` (a store's finalized
/// header) standing for `ws_state`: `compute_epoch_at_slot(current_slot) <=
/// compute_epoch_at_slot(ws_slot) + ws_period`. The spec's integers are unbounded, so an end past
/// `u64::MAX` is never reached (and must not wrap around to an early one).
pub fn is_within_weak_subjectivity_period(ws_slot: u64, current_slot: u64, ws_period: u64, spec: &ChainSpec) -> bool {
    match spec.epoch_at_slot(ws_slot).checked_add(ws_period) {
        Some(last_epoch) => spec.epoch_at_slot(current_slot) <= last_epoch,
        None => true,
    }
}

/// The first slot outside the weak-subjectivity period of the header at `ws_slot`: the start of
/// epoch `compute_epoch_at_slot(ws_slot) + ws_period + 1`. `None` if that slot is past `u64::MAX`
/// (the period never ends). For every `current_slot`, [`is_within_weak_subjectivity_period`]
/// holds exactly when `current_slot` is below this slot.
pub fn weak_subjectivity_end_slot(ws_slot: u64, ws_period: u64, spec: &ChainSpec) -> Option<u64> {
    spec.epoch_at_slot(ws_slot).checked_add(ws_period)?.checked_add(1)?.checked_mul(spec.preset.slots_per_epoch)
}

/// The policy check: a store whose finalized header is at `finalized_slot` may be advanced at
/// `current_slot` only inside that header's weak-subjectivity period.
pub fn check_within(finalized_slot: u64, current_slot: u64, ws_period: u64, spec: &ChainSpec) -> Result<(), Error> {
    if is_within_weak_subjectivity_period(finalized_slot, current_slot, ws_period, spec) {
        Ok(())
    } else {
        Err(Error::OutsideWeakSubjectivityPeriod)
    }
}
