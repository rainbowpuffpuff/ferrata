# ADR 0006: `Attest` replies with statements, `Read` with the record; one request may hold several facts

- **Date:** 10 November 2026 (Stage 4)
- **Status:** accepted; built
- **Requirements:** RFP-021 F.3, F.4, F.7, soft #4

## Context

The design's table had `Attest` reply with the record's state; consumers need verified facts, and compound conditions
over several facts of one block.

## Options considered

1. Reply with the record and let each consumer verify.
2. Reply with an `Attestation` (instance, chain, block, each fact with its identifier), keep `Read` for the raw
   record, and let one request carry accounts, slots, logs, leaves, conditions, relations and hops.

## Decision

The second.

## Rationale and trade-offs

A public consumer acts in its reply handler on verified statements within the same transaction; a private consumer
uses `Read` (ADR 0007). The cost: the proofs travel in the transaction, so a log in a very large receipt may not fit
the execution target.

## Sources

- lez/RESULTS.md §5, "Decisions" items 3, 5, 9
- docs/guides/consumer-guide.md §1, §3
