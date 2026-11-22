# ADR 0009: Statement identifiers per kind of fact, independent of the instance; a class says which may be nullified

- **Date:** 10 November 2026; the class on 18 November 2026 (second build)
- **Status:** accepted; built (the class in a second build, not deployed)
- **Requirements:** RFP-021 F.6, S.4

## Context

Consumers key nullifiers on identifiers. A storage value can change and change back; one deposit lies under every
later root; the same fact may be proven under two instances or by different chains of hops.

## Options considered

1. One identifier for any fact. *Reconstructed for contrast* (not recorded as an option at the time).
2. Domain-separated identifiers per kind (log, storage, account, accumulator leaf), over fixed-width fields, without
   the instance; and a class (`Event`, `Leaf`, `Value`) telling which are safe to nullify.

## Decision

The second.

## Rationale and trade-offs

A once-per-event consumer keys on a log or a leaf; the same fact has the same identifier under a replacement instance,
which is what makes a migration's nullifier carry-over possible (ADR 0004). The cost: a `Value` identifier must never
be a nullifier alone, which the class makes explicit.

## Sources

- docs/design/attestation.md §5.2, §5.7
- docs/guides/consumer-guide.md §5
- EVIDENCE row 145
