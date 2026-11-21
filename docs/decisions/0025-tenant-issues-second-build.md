# ADR 0025: The two tenant issues of 18 November are fixed in a second build of the bridge program, not by redeploying now

- **Date:** 18 November 2026
- **Status:** accepted; the fix built and tested, not deployed
- **Requirements:** RFP-022 F.16, Sec.3

## Context

An independent review of the bridge's LEZ v0.3.1 port found two properties that the deployed v0.4 bridge program
shares, and a malicious-tenant test reproduced them on it: whoever sees a plaintext claim's secret can freeze that
deposit through a second configuration of the same vault, and a filled public destination can no longer be credited.
No theft and no double mint.

## Options considered

1. Redeploy the program at once (every configuration and the live runs would move). *Reconstructed for contrast* (not
   recorded as an option at the time).
2. Key nullifier and holder accounts by the configuration in a second build (`lez/guests/bridge-next`), tested,
   deployed with milestone B1; document the issue in the trust document meanwhile.

## Decision

The second.

## Rationale and trade-offs

The deployed images stay reproducible and the live round trips checkable; the issue is visible before anyone deposits
(a private claim, the default, does not publish the secret). The second build also refuses the red team's finding
F1, a configuration whose freeze authority is its admin (docs/image-ids.md). The cost: until B1 the limitation
stands.

## Sources

- docs/design/bridge.md §3, "Known issues, 18 November 2026"
- lez/results/bridge-next.md
- docs/image-ids.md
- EVIDENCE rows 136, 143
