# ADR 0029: Voluntary disclosure of deposits and claims: the deposit secret only once its nullifier is spent

- **Date:** 21 November 2026
- **Status:** accepted; built, not deployed
- **Requirements:** RFP-022 Soft.2, Priv.4

## Context

RFP-022 soft requirement 2 asks for viewing keys with which a user voluntarily discloses their own bridge activity
to a chosen third party, without weakening anyone else's privacy and without a protocol-level disclosure capability.
`kView` opened burns only: a deposit's commitment and nullifier both derive from the secret `s`, which is what a claim
proves knowledge of (ADR 0013).

## Options considered

1. Keep `kView` burns-only. *Recorded:* docs/design/bridge.md §4 ("so soft #2 is met outbound only"); bridge/RESULTS.md
   ("SPEC-v2: what I believe is wrong, unsafe or underspecified", point 4).
2. A disclosure document from the wallet: `kView` for burns, and per deposit the declared destination, with `s` and
   `N` only once `N` is spent; the third party checks it against public chain data.

## Decision

The second.

## Rationale and trade-offs

Wallet-side only: no guest, program, vault or core change. Before the claim, `s` would let the third party submit
the claim itself (to the user's destination, but naming itself as relayer and taking the relay fee, and choosing the
timing); after it, `s` grants nothing (refunds need no `s`; releases use `sOut`). The cost: an unclaimed deposit's
destination is a declaration the third party cannot check until the claim; if the third party publishes a disclosed
deposit-to-claim pair, that pair leaves everyone else's candidate set.

## Sources

- bridge/crates/ferrata-disclose, docs/privacy/viewing-keys.md
- bridge/RESULTS.md ("SPEC-v2: what I believe is wrong, unsafe or underspecified", point 4)
- docs/design/bridge.md §4
