# ADR 0007: Private consumers read the record and verify inside their own proof; they never call `Attest`

- **Date:** 10 November 2026
- **Status:** accepted; built
- **Requirements:** RFP-021 F.8, R.11

## Context

On LEZ v0.4 every message from a private execution to a public actor is published in plaintext: its target, its bytes
and the sending program. A private consumer calling `Attest` would publish its whole request.

## Options considered

1. A private call to `Attest` (measured: the token, sender, amount and identifier appear in the transaction).
   *Recorded:* docs/design/attestation.md §6.3 ("Why not a private call").
2. A private read of the write-once record (`Read`, one byte), then `attest::evaluate` inside the consumer's own
   execution.

## Decision

The second; the consumer keeps only a fixed-size result.

## Rationale and trade-offs

Only the read, the record, the fee transfer, the consumer's program id and an alias are public; records never change,
so private transactions in one block never conflict. The cost: a read shows which record was read (read the newest,
hop inside the proof), and the consumer proves the verification itself.

## Sources

- docs/design/attestation.md §6.2–§6.4
- docs/design/lez-v0.4-findings.md §6
- lez/harness/tests/attest_private.rs
