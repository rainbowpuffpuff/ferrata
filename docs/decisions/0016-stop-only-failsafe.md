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

Refunds open only when the vault is retired, the sunset checkpoint's revocation window has passed — so a forged sunset
checkpoint can be revoked before any refund — and no global release freeze stands; the refund boundary is set by the
first non-revoked sunset checkpoint, never lowered by a later one, and revoking that checkpoint clears it, while
`revokeFrom` refuses an index that is already revoked (D5). From the v3 vault build: retirement must not depend on the
freeze key, so anyone may call `retire()` once a sunset checkpoint has been final for longer than `revokeWindow` —
since widened to anyone retiring after 30 days without a finalisation (D13, D11).

The limits kept and documented rather than fixed (D8): a tripped stop strands all escrow except owed balances (D8 kept refunds open for a sunset checkpoint finalised
first; as built under D13, refunds stop with the verifier too, so after a stop nothing but owed balances moves, shown by
the migration rehearsal's `vault/test/MigrationFailsafe.t.sol`, branch rfp-ci-release); the stop wrapper's owner is not moved by a freeze-authority handover, so it
is renounced at deployment instead; until the admin replaces it under the timelock, a compromised freeze key can renew
freezes, revoke honest checkpoints within the window and retire the vault; a payee that can never receive keeps its
`owed` balance forever; and retired keys stay counted until a finalised checkpoint carries the rotation.

## Sources

- docs/design/bridge.md §14–§16, §20
- docs/trust.md §6
- vault/README.md, "Verifier and stop"
- docs/design/spec/bridge-spec-v3-changes.md D5, D8, D11
