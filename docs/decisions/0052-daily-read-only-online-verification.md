# ADR 0052: Run public-state evidence checks daily in a separate read-only workflow, not in the offline gate

- **Date:** 26 November 2026
- **Status:** accepted; built; scheduled daily and by hand
- **Requirements:** RFP-021 S.1; RFP-022 S.1

## Context

This record was written on 27 November 2026 from the commits, after the fact. Commit `3e5a658e` added the public-state
checks, and `ac2d222e` took the workflow decision.

Committed records and offline re-checks can show that a report is internally consistent, but cannot show that a vault,
program, website or checkpoint head still has the claimed state. Conversely, making every push and pull request wait
on unrelated public services would couple code verification to endpoint availability and request limits. The decision
was how to refresh public facts without giving CI credentials or transaction authority.

## Options considered

1. Keep only the offline replay of committed evidence. *Reconstructed for contrast*. It is deterministic and quick,
   but cannot detect that a deployment has changed, a supposedly closed cap has reopened or public data has aged.
2. Make public read-backs part of every push and pull-request gate. *Reconstructed for contrast*. It gives the freshest
   result per change, but request limits and unavailable third-party services can block unrelated changes.
3. Keep strict offline verification in ordinary CI and run a separate daily and manual online tier that permits only
   reads, uses no secrets and reports endpoint failures rather than converting them into matches.

## Decision

The third. `.github/workflows/verify-online.yml` runs the registry's online checks daily and on demand with a read-only
repository token. The checks re-read prior Ethereum round trips, closed vault caps and verifier stops, test-network
programs and heads, applied Ethereum headers, the site's served manifest, published package signatures and cited CI
outcomes. No transaction is signed or sent and no secret is configured.

The clients admit only their documented read calls: Ethereum calls, code, receipts and logs; LEZ block and account
reads; and ordinary public GET requests for site, beacon and CI metadata. An endpoint that cannot answer produces a
reported skip, not evidence of a match; a contradictory answer fails. The ordinary offline gate remains independent
of this workflow.

## Rationale and trade-offs

A recurring read-back can expose drift between the records and live public state without granting automation any
ability to move funds or administer a deployment. Separating it from offline CI preserves a deterministic change gate
while still publishing current online results. Reusing the registry binds each read-back to the same requirements and
evidence rows as its offline counterpart.

The cost is dependence on public endpoints, their rate limits and their availability. A skip is less assurance than a
successful read, and a successful read confirms the selected facts only at observation time; it neither re-executes
all historical proofs nor supplies an independent audit. Some historical reads require archive data, and private or
unpublished state cannot be checked this way. Scheduling supplies freshness without promising continuous monitoring.

## Sources

- Commits `3e5a658e` and `ac2d222e`; scripts/verifylib/checks_online.py (the public-state checks and read-only calls)
- `.github/workflows/verify-online.yml` (daily and manual triggers, permissions and absence of secrets)
- `scripts/verifylib/core.py` (online mode and pass, fail and skip semantics)
- docs/VERIFY.md §2 and docs/guides/ci.md (the checks, read-only boundary and endpoint limitations)
