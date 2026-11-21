# ADR 0017: A release delay inside a revocation window, and a release nullifier that does not depend on the burn's index

- **Date:** 10 November 2026 (D13); the guest on 11 November 2026 (security review M-3)
- **Status:** accepted; built
- **Requirements:** RFP-022 R.3, Security #4, #6

## Context

An accepted wrong checkpoint should be revocable before it pays; after a revocation the canonical branch may collect a
burn at another index than the revoked checkpoint did.

## Options considered

1. Release at once on a final root; `NOut` over the burn's index (SPEC-v2).
2. A per-token release delay between `minReleaseDelay` (24 h outside testnets) and the 7-day revocation window, in
   which the freeze authority may `revokeFrom`; `NOut = H("ferrata/nf-out/v2" ‖ sOut)`.

## Decision

The second.

## Rationale and trade-offs

A burn collected twice is released once; used nullifiers stay used across a revocation. The cost: a release waits at
least the delay after its checkpoint.

## Sources

- docs/design/spec/bridge-spec-v3-changes.md D13
- docs/trust.md §3.5
- bridge/crates/ferrata-bridge-core/src/encoding.rs (`TAG_NF_OUT`)
