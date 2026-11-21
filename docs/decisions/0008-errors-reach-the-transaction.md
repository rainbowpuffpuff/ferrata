# ADR 0008: Typed errors reach the transaction's outcome, not the consumer; the client maps each code

- **Date:** 10 November 2026 (Stage 4)
- **Status:** accepted; built
- **Requirements:** RFP-021 U.2, U.5, S.4 (rules 3 and 4)

## Context

The Recommended Consumer Pattern asks a consumer to handle each typed code. On LEZ v0.4 a non-zero exit of any handler
reverts the whole transaction (`check_exit_code`), so a consumer's code never runs after a failed attestation.

## Options considered

1. Return error values to the consumer (impossible on v0.4 without changing LEZ).
2. Let the code be the transaction's outcome; give clients the same codes before sending (dry runs) and a table of
   what to do for each.

## Decision

The second; every code has one owner (a test fails on overlap), and the IDL names each.

## Rationale and trade-offs

Rule 4 (no action without an attestation) holds by construction, and a refused attestation consumes nothing. The cost:
handling is a client's job, which `docs/guides/consumer-guide.md` §8.4 tabulates code by code.

## Sources

- lez/RESULTS.md §5, "Decisions" item 4
- lez/harness/tests/exit_codes.rs
- idl/ferrata_attestation.idl.json
