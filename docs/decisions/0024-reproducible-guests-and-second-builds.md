# ADR 0024: Guest builds independent of the build path; changes to a deployed program go into a second build

- **Date:** 11 November 2026
- **Status:** accepted; built
- **Requirements:** RFP-021 VS.3, S.9; RFP-022 Security #8

## Context

RISC Zero image ids embedded absolute paths (0 of 6 deployed ids reproduced elsewhere), and once a vault pins an image
id no compiled byte of that guest may change.

## Options considered

1. Rebuild in place and redeploy.
2. `scripts/guest-rustc` remaps every path and replaces cargo's path-dependent metadata for the zkVM target; later
   changes live in a second build (its own workspace, a copied lockfile, a feature off by default), leaving the
   deployed sources byte for byte.

## Decision

The second.

## Rationale and trade-offs

Anyone rebuilds the deployed ids (14 of 14 guests byte-identical at two paths). The cost: bugs in pinned programs
become documented limits with their fix in a second build, deployed only by a new configuration (ADR 0025).

## Sources

- docs/image-ids.md
- docs/guides/verify.md §2
- EVIDENCE rows 37, 82
