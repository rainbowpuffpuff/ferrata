# ADR 0028: An operator rebate paid by a pool program for an advance landed through it

- **Date:** 21 November 2026
- **Status:** accepted; built (branch rfp-softs), not deployed
- **Requirements:** RFP-021 Soft.5, F.11, VS.6

## Context

RFP-021 soft requirement 5 asks for a mechanism that rewards whoever advances the consensus state without making
operating exclusive. The proposal's design note (§A7) paid a fixed rebate from the bridge's protocol fee to whoever
lands an update. Neither the attestation program nor the bridge program should change for it (ADR 0004).

## Options considered

1. A fixed rebate from the bridge's protocol fee to whoever lands an update (the proposal's design note §A7).
   *Recorded:* docs/traceability.md (RFP-021 Soft.5: "The bridge can pay a fixed rebate from its protocol fee to
   whoever lands an update … Design only").
2. A pool program that relays the update's last step (`Finish`) and pays out of its own account, only if the
   finalized slot advanced in that transaction, funded by ordinary transfers (the bridge's fee recipient by policy).

## Decision

The second.

## Rationale and trade-offs

Updating stays open to anyone, directly or through the pool; an empty pool pays nothing and blocks nothing; progress
made directly earns nobody a rebate, because the pool reads the instance before and after `Finish`; neither the
attestation program nor the bridge program changes. The limit kept: whoever lands the last step is paid, even for a
job another operator opened. LEZ v0.4's `custody_transfer` pays out of the pool's own account. Measured: `Land` 3,842,337 execution gas against 3,736,661 for a direct `Finish`.

## Sources

- branch rfp-softs, commit 53c281de: lez/rebate-wire/src/lib.rs, lez/guests/rebate, lez/harness/tests/rebate.rs
- branch rfp-softs: docs/guides/operator-rebate.md
- docs/traceability.md (RFP-021 Soft.5: the design note it replaces)
