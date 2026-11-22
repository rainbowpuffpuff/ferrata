# ADR 0014: Fixed denominations, at most eight per token

- **Date:** 9 November 2026 (SPEC-v2)
- **Status:** accepted; built
- **Requirements:** RFP-022 F.8, Priv.1, Priv.2

## Context

Amount is one of the three signals the privacy requirement allows; free amounts would make most deposits unique.

## Options considered

1. Any amount the depositor chooses. *Reconstructed for contrast* (not recorded as an option at the time); the reason
   against it is docs/design/bridge.md §1's: fixed denominations, "so that an observer of both chains cannot tell which
   deposit paid which mint".
2. Fixed denominations per token (1 to 8, default 3), each an exact multiple of a power-of-ten LEZ unit, registered
   write-once.

## Decision

The second.

## Rationale and trade-offs

Each claim hides among the deposits of its token and denomination; the adversary in the privacy suite pairs at chance.
The cost: users split larger amounts, and a rare denomination is a small anonymity set.

## Sources

- docs/design/bridge.md §13, §17
- EVIDENCE rows 23, 89
