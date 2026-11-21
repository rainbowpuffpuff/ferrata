# ADR 0003: Two update paths with one verification: a public job of several transactions, and a proven path of one

- **Date:** 10 November 2026
- **Status:** accepted; built
- **Requirements:** RFP-021 F.1, F.11, F.12, P.2

## Context

A finality update verified in one public transaction (`VerifyAll`) costs 9.1M execution gas: under LEZ v0.4's 10M cap,
but 1.8 times its 5M per-block target, so one update fills nearly a block of the zone's public capacity; an update
that adopts a committee (about 16M) does not fit one transaction (docs/design/attestation.md §4.2, measured on mainnet
periods 1899–1903).

## Options considered

1. One public transaction per update (`VerifyAll`): fits a plain finality update but fills nearly a block; an update
   that adopts a committee does not fit. *Recorded:* docs/design/attestation.md §4.2 (its table's `VerifyAll` and
   adoption rows).
2. Only a proven path: the update verified in a private execution and applied by a small public transaction (needs a
   prover for every update). *Recorded:* docs/design/attestation.md §4.2 ("The proven path is far cheaper on chain
   but needs a prover; the public job keeps liveness from depending on anyone owning one").
3. Both: a public job (`Open`, `Miller`, `Finish`, plus `Adopt` parts for a new committee) that anyone runs with no
   prover, and the proven path (`VerifyAll`, then `ApplyVerified`) for whoever can prove 9M cycles.

## Decision

Both, over the same step functions. `VerifyAll` stays open to any actor, so a plain finality update may also go
in one public transaction.

## Rationale and trade-offs

No prover is a required party (F.12): the public job advances the instance alone. The cost: a finality update is three
transactions (9.2M execution gas in all) and a duplicate first step wastes about 2.2M cycles, which the updater's
deferral keeps rare (ADR 0011).

## Sources

- docs/design/attestation.md §4.2–§4.5
- docs/guides/operator-guide.md §3.2
- lez/updater/tests/every_party.rs (`provers_none_is_needed_on_the_public_job`)
