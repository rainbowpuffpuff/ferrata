# ADR 0019: Bound what a captured committee or a forged root can take, instead of pricing the attack

- **Date:** 18 November 2026 (measured); the caps from 10 November 2026
- **Status:** accepted; caps and freezes built; bonded backing design only
- **Requirements:** RFP-021 VS.4; RFP-022 Security #4

## Context

A forged finality update needs 342 of 512 committee seats, and nothing of their stake is slashable for it (EIP-7657 is
stagnant), so a bribe has no floor.

## Options considered

1. Claim an attack cost the protocol does not enforce.
2. Size per-token and global caps, the release delay and freezes so that what passes before a freeze is what a
   deployment accepts to lose; record bonded backing and FFG anchoring as future options.

## Decision

The second.

## Rationale and trade-offs

Measured under a verifier that accepts every seal: releases stayed within the cap bound in every window. The cost: the
testnet scripts leave caps unlimited, so a production deployment must set them (`docs/guides/deployer-guide.md` §2,
§11).

## Sources

- docs/trust.md §2.2
- docs/forgery-cost/README.md
- EVIDENCE row 138
