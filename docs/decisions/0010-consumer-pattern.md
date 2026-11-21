# ADR 0010: The Recommended Consumer Pattern has seven rules: the RFP's four and a secret-keyed nullifier

- **Date:** 10 November 2026
- **Status:** accepted; built (rules 5–7 on the vault's real deposits: 11 November)
- **Requirements:** RFP-021 S.4, F.8; RFP-022 F.3

## Context

S.4 asks for a nullifier keyed on the statement identifier; F.8 forbids new correlation signals. A nullifier that is a
function of the identifier alone can be computed from public Ethereum data and matches each private action to its
deposit.

## Options considered

1. A nullifier on the identifier alone (as the reference consumer's public claim keeps it, with no private claim).
2. Three added rules: the fact commits to a depositor's secret; the nullifier hashes that secret with the identifier;
   exactly one kind of fact counts as the deposit.

## Decision

The second, for any consumer with a private claim; the bridge's vault event carries the commitment.

## Rationale and trade-offs

A deposit cannot be claimed twice (once on each path), and a private claim names no deposit. The cost: a plain ERC-20
`Transfer` carries no commitment, so such a consumer has no private claim.

## Sources

- docs/design/attestation.md §7
- docs/guides/consumer-guide.md §6
- lez/RESULTS.md §5, "Decisions" item 7
- EVIDENCE row 73
