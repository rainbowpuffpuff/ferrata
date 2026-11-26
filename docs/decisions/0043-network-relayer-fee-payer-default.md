# ADR 0043: Use the network's gas-drop relayer by default and fall back to the user's fee account only on 404 or 410

- **Date:** 25 November 2026
- **Status:** accepted; built, the test-network relayer deployed
- **Requirements:** RFP-022 Priv.7, U.5

## Context

On LEZ v0.4 the prover's private account pays a privacy-preserving transaction's native fee inside its proof. Asking
the user to fund that account independently adds a setup step and exposes where its native LEZ came from. The existing
gas drop avoided this by having a relayer seal one credit to a fresh one-time account for the claim; the same drop can
pay a later burn. Making that protection the normal path required deciding which relayer a client uses and when it is
safe to use the user's own account instead.

## Options considered

1. Leave the gas drop opt-in and otherwise pay from the proving account. *Recorded:* ADR 0022 records the gas drop
   built into the wallet and tool, while cli/src/gasdrop_net.rs now replaces that client default. This preserves the
   funding and correlation burden for every user who does not discover and configure the option.
2. Try a published relayer, but pay from the user's own account whenever its terms cannot be fetched or understood.
   *Reconstructed for contrast* cli/src/gasdrop_net.rs's tests enumerate 400, 429, 500, 503, malformed answers,
   timeouts and unavailable connections as errors instead. Such behaviour would silently select the weaker fee payer
   when the network or relayer failed.
3. Unless the user explicitly opts out or names another relayer, use an HTTP(S) sequencer RPC's origin to read the
   network's relayer from `/relayer/gas-drop` and its sealed credit for the claim and subsequent burn. Only HTTP 404 or
   410 means no relayer is published and permits paying from the user's account without `--no-gas-drop`; every other
   answer or transport failure stops with the opt-out named. A non-HTTP RPC has no HTTP origin to discover one from.

## Decision

The third. The network terms identify the relayer, inbox, gas-drop guest and amount. The claim names that relayer, so
the deposit's registered relay fee goes to the party funding the drop. A burn uses the drop's remainder and asks for
no new drop; if none remains while a relayer is published, paying from the user's account also needs explicit opt-out.
`--gas-drop-inbox` and the corresponding environment setting select a known relayer instead, and `--no-gas-drop` or its
environment setting deliberately retains the proving-account fee payer.

The gateway's semantics are part of the decision: only a positive absence answer triggers fallback. Paused service,
rate limiting, malformed terms and network failures fail closed.

## Rationale and trade-offs

The default gives a user with no native LEZ the private fee path without extra configuration, and the remainder covers
the natural next operation. Restricting silent fallback to 404 or 410 prevents an outage, an attack on availability or
a malformed response from turning a private-by-default claim into one paid by a correlatable pre-funded account. The
recorded run covered the full client path with the user holding no native LEZ before or after the claim and burn, and
its tests separately exercised each refusal to fall back.

The cost is availability and an explicit trust choice. A published but unusable relayer stops the operation even when
the user could pay, until the user supplies `--no-gas-drop`; where no relayer is published, a first claim still needs
native LEZ from elsewhere and retains that correlation. Terms from another machine require HTTPS, with only local HTTP
allowed. The client default was exercised against a local sequencer and gateway in dev mode; the deployment record
covers the test network's relayer, not a claim that every client artifact has been replaced in production.

## Sources

- Commits `c994e951`, `2d88fe4c` and `a1ce545b`; cli/README.md, "The gas drop" under the claim instructions
- cli/src/gasdrop_net.rs (network discovery, the 404/410 rule, HTTPS restriction and opt-out errors)
- docs/guides/user-guide.md §5, "Fees" (the private fee mechanism, default relayer and remaining correlation)
- deploy/testnet/README.md, "`ferrata-gasdrop`" and "The gas-drop relayer: deploying it"
- docs/EVIDENCE.md row 258 (the claim and burn with no gas-drop setting and failure cases)
