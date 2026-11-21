# ADR 0003: Two update paths with one verification: a public job of several transactions, and a proven path of one

- **Date:** 10 November 2026
- **Status:** accepted; built
- **Requirements:** RFP-021 F.1, F.11, F.12, P.1, P.2

## Context

A whole finality update costs about 10.2M cycles inside RISC Zero, above LEZ v0.4's 10M execution gas per transaction
and far above its 5M per-block target.

## Options considered

1. One transaction per update (does not fit).
2. Only a proven path: the update verified in a private execution and applied by a small public transaction (needs a
   prover for every update).
3. Both: a public job (`Open`, `Miller`, `Finish`, plus `Adopt` parts for a new committee) that anyone runs with no
   prover, and the proven path (`VerifyAll`, then `ApplyVerified`) for whoever can prove 9M cycles.

## Decision

Both, over the same step functions.

## Rationale and trade-offs

No prover is a required party (F.12): the public job advances the instance alone. The cost: a finality update is three
transactions (9.2M execution gas in all) and a duplicate first step wastes about 2.2M cycles, which the updater's
deferral keeps rare (ADR 0011).

## Sources

- docs/design/attestation.md §4.2–§4.5
- docs/guides/operator-guide.md §3.2
- lez/updater/tests/every_party.rs (`provers_none_is_needed_on_the_public_job`)
