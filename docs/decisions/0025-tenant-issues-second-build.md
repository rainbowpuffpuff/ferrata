# ADR 0025: The two tenant issues of 18 November are fixed in a second build of the bridge program, not by redeploying now

- **Date:** 18 November 2026
- **Status:** accepted; the fix built and tested, not deployed
- **Requirements:** RFP-022 F.16, Security #3

## Context

A malicious-tenant test on the deployed program found two issues: whoever sees a plaintext claim's secret can freeze
that deposit through a second configuration of the same vault, and a filled public destination can no longer be
credited. No theft and no double mint.

## Options considered

1. Redeploy the program at once (every configuration and the live runs would move).
2. Key nullifier and holder accounts by the configuration in a second build (`lez/guests/bridge-next`), tested,
   deployed with milestone B1; document the issue in the trust document meanwhile.

## Decision

The second.

## Rationale and trade-offs

The deployed images stay reproducible and the live round trips checkable; the issue is visible before anyone deposits
(a private claim, the default, does not publish the secret). The cost: until B1 the limitation stands.

## Sources

- docs/design/bridge.md §3, "Known issues, 18 November 2026"
- lez/results/bridge-next.md
- EVIDENCE rows 136, 143
