# ADR 0022: A gas drop funds a first private claim's fee without linking it

- **Date:** 10 November 2026 (D13); run on LEZ v0.4.0-rc3 on 16 November 2026
- **Status:** accepted; built and run with LEZ's wallet; not yet in the relayer daemon
- **Requirements:** RFP-022 U.5, Privacy #6

## Context

A private transaction's fee is paid in native LEZ from a private account of the prover; a first claim funded from a
faucet or a purchase links to that source.

## Options considered

1. Accept the link.
2. A gas-drop receipt binding `(N, oneTimeAddress, relayer)`; the relayer sends one credit per `N` to a fresh one-time
   private account; the claim sweeps it and pays its fee from it.

## Decision

The second.

## Rationale and trade-offs

A claimant with no native LEZ claimed privately (M0 check 2). The cost: the relayer learns `N`, a one-time address and
a time, not which deposit; the daemon's own credit is still a public transfer.

## Sources

- docs/design/bridge.md §6
- docs/guides/operator-guide.md §6.4
- EVIDENCE rows 47, 48, 124
