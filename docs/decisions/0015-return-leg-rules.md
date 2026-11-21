# ADR 0015: The return leg: a checkpoint program over LEZ blocks, finality by rule (B) attestations, rule (A) as the fallback

- **Date:** 10 November 2026 (SPEC-v3 D1, D3, D4, D7)
- **Status:** accepted; both built; (B) proposed to Logos
- **Requirements:** RFP-022 F.6, F.7, Security #2, #7

## Context

LEZ v0.4 blocks carry no state root and fees depend on cycle counts, so Ethereum cannot check a burn's validity or
finality from a receipt; the previous rule (SPEC-v2 §4.6) was shown to finalise a block LEZ later drops, with fewer
than `t` keys.

## Options considered

1. A bridge committee signing releases (a trusted party).
2. Rule (A): inferred support (`t` other keys' inscriptions after the tip) and a wait `W = Δ + (t − 1) × turn` with a
   random beacon, made twice.
3. Rule (B): `t` final-tier attestations by keys the channel keys certified, plus the tip's inscription check, one
   proof and a 1-hour wait.

## Decision

Both, chosen per vault (`mode`); (B) where the zone's sequencers run the attestor, (A) otherwise.

## Rationale and trade-offs

Under (B) every off-chain role is liveness-only and `n − t` silent sequencers are tolerated; (A) needs no change on
LEZ but trusts the watchers for safety outside its conditions. The cost: the accredited sequencers are trusted for the
return leg under both rules; the analysis and simulation are in `docs/design/finality-rule/`.

Specification v3 states what each rule needs as validity V1–V3 and finality F1–F4, where F2 bounds adversarial keys in
the channel's live accredited set, not only in the proof's `K`; outside testnets `nMin >= 3` and `tMin >= 2`, and rule
(A) needs a chain whose `prevrandao` is unpredictable (D6). What still breaks it is documented with the rule: `t` keys
can sign any history, and two honest sequencer nodes eclipsed for about `W` defeat the wait; outside the assumptions
the watchers are trusted for safety.

From the checkpoint prototype (D10): a key-set change needs exactly the channel's live `configuration_threshold`
signatures, the threshold at least `t(n)`, and no repeated keys; `allowSingleKey` holds only where the pinned
`nMin <= 1`; the deployment check — which sees what the program cannot — verifies `turnSeconds` against the slot
duration and refuses a zero `posting_timeframe`; and provers must read blocks from the channel's inscriptions, because
a block served by an RPC may carry a different `bedrock_status` byte. A burn reaches the bridge program only as a
public top-level call, the public part of a private transaction or a private-to-public boundary delivery to
`bridgeHeader`; anything else must be refused, or tokens would be destroyed without a burn leaf. The burn frontier
hash carried in the checkpoint state is the hash of a tagged, canonical frontier with unused levels zero, an encoding
specification v2 had left open (D12).

## Sources

- docs/design/spec/bridge-spec-v3-changes.md D1, D3, D4, D6, D7, D10, D12
- docs/design/bridge.md §7–§11
- docs/trust.md §3
