# ADR 0017: A release delay inside a revocation window, and a release nullifier that does not depend on the burn's index

- **Date:** 10 November 2026 (D13); the guest on 11 November 2026 (security review M-3)
- **Status:** accepted; built
- **Requirements:** RFP-022 R.3, Sec.4, Sec.6, Soft.3

## Context

An accepted wrong checkpoint should be revocable before it pays; after a revocation the canonical branch may collect a
burn at another index than the revoked checkpoint did.

## Options considered

1. SPEC-v2: a per-token release delay with no lower bound (0 allowed, so a release could pay before its checkpoint
   stops being revocable), and `NOut = H("ferrata/nf-out/v1" ‖ sOut ‖ burnId)`, which changes with the burn's index.
   *Recorded:* docs/design/spec/bridge-spec-v2.md (the release's `releaseDelay(token)`, `revokeFrom`, `NOut`);
   bridge/RESULTS.md ("v2 allows 0").
2. A per-token release delay between `minReleaseDelay` (immutable; recommended 24 h outside testnets) and the
   revocation window (immutable; 7 days as the design's default, 1 day in the deploy script's testnet configuration),
   in which the freeze authority may `revokeFrom`;
   `NOut = H("ferrata/nf-out/v2" ‖ sOut)`.

## Decision

The second.

## Rationale and trade-offs

A burn collected twice is released once; used nullifiers stay used across a revocation. The cost: a release waits at
least the delay after its checkpoint.

The delay is bounded on both sides, `minReleaseDelay <= delay <= revokeWindow` with the minimum immutable, so a
checkpoint can be revoked for as long as its releases are delayed (D5); and the window's boundary is one second exact:
`revokeFrom` runs while `block.timestamp <= finalizedAt + revokeWindow`, refunds and releases whose delay equals
`revokeWindow` open only after it (D11). The release and claim guests' cycle counts are witness-independent, a CI gate
of 64 random witnesses against one count, which caught the first release guest leaking the burn index's popcount
through its cycle count (D12).

## Sources

- docs/design/spec/bridge-spec-v3-changes.md D5, D11, D12, D13
- docs/trust.md §3.5
- docs/design/spec/bridge-spec-v2.md
- bridge/RESULTS.md
- bridge/crates/ferrata-bridge-core/src/encoding.rs (`TAG_NF_OUT`)
