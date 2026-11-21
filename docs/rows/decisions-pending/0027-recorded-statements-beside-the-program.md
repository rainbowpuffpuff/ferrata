# ADR 0027: Recorded statements by a separate program that calls the attestation program's own `Attest`

- **Date:** 21 November 2026
- **Status:** accepted; built (branch rfp-softs), not deployed
- **Requirements:** RFP-021 Soft.1, F.7, VS.3

## Context

RFP-021 soft requirement 1 asks for a consumption path that records a verified statement to a public account so that
many consumers share one verification, over a single verification core with the direct path (RFP-020's pattern), and
for documentation that a consumer of such an account trusts whoever wrote it. The attestation program is deployed
immutable (ADR 0004); a new instruction in it would be a new program and move its image id.

## Options considered

1. A `Record` instruction in the attestation program, writing a statement account itself.
2. A separate recorder program that sends the attestation program's own `Attest` and keeps the reply, keyed on the
   request without its proofs; consumers read it with `Read` and key their nullifiers on their whole trust
   configuration (recorder, attestation program, instance, chain).
3. An account any submitter writes, consumers checking a signature or trusting the submitter.

## Decision

The second.

## Rationale and trade-offs

It is RFP-020's push aggregator: a thin wrapper over the shared verifier writing a canonical account, so the audit
surface of the verification stays the attestation program's, and no deployed image id moves. Only the recorder's code
writes an entry, and only the attestation program's reply, so a consumer trusts the recorder's program id (fixed in
immutable state, VS.3's rule) and not a submitter; the third option would ask it to trust whoever wrote. The cost: one
more program to audit (small), and a consumer that wants recent state checks `block_number`, because an entry never
becomes more recent. Measured: a consumer's use 63,438 cycles against 432,128 for a direct attestation.

## Sources

- branch rfp-softs, commit e90b9249: lez/statements-wire/src/lib.rs, lez/guests/statements, lez/harness/tests/statements.rs
- branch rfp-softs: docs/guides/recorded-statements.md
