# ADR 0016: No escrow-moving failsafe: a stop that only RISC Zero's proof of unsoundness trips, lapsing freezes, keyless retirement and refunds

- **Date:** 10–11 November 2026
- **Status:** accepted; built
- **Requirements:** RFP-022 Security #1, #5, #8, #9

## Context

Security #9 asks for a failsafe that recovers funds only by the owner's own proof, never by an admin redirecting
funds; Security #8 forbids changing what the bridge accepts.

## Options considered

1. An admin rescue or upgrade path (a backdoor).
2. A stop-only failsafe: RISC Zero's unmodified emergency stop around the base Groth16 verifier (never the router,
   whose admin can add verifiers), its owner renounced at deployment; freezes of at most 30 days; anyone retires after
   30 days without checkpoint progress; refunds to the depositor named in the leaf.

## Decision

The second.

## Rationale and trade-offs

No role can move escrow; the walkaway test passes. The cost: after a stop, unreleased burns and unrefunded deposits
stay in the vault; a wrong checkpoint not revoked within the window halts the vault for good.

## Sources

- docs/design/bridge.md §14–§16
- docs/trust.md §6
- vault/README.md, "Verifier and stop"
