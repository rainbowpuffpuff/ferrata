# ADR 0013: Deposit and burn secrets derive from the LEZ wallet seed; recovery scans whole lists

- **Date:** 10 November 2026 (SPEC-v2)
- **Status:** accepted; built
- **Requirements:** RFP-022 F.2, F.5, F.9, R.5, Privacy #4

## Context

A user must recover every claimable deposit and unreleased burn from credentials alone, and no provider may learn
which items are the user's.

## Options considered

1. Keep per-deposit secrets in local state (lost with the machine).
2. Derive `kAuth` and `kView` from the wallet seed under labels disjoint from LEZ's own; `s`, `C`, `N`, burn tags,
   `sOut` and the memo from them; recover by downloading whole lists and matching.

## Decision

The second.

## Rationale and trade-offs

A wallet restored from the seed finds everything (10 of 10 deposits and burns in the privacy suite); providers see
whole-list downloads only. The cost: recovery scans every deposit (1.3 s for a million on one laptop thread).

## Sources

- docs/design/bridge.md §4, §5.3, §5.6, §12.1
- docs/guides/user-guide.md §2, §8
