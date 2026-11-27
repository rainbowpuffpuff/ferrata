# ADR 0053: Generate the audit inventory from an explicit production allowlist, tracked files and lockfiles

- **Date:** 26 November 2026
- **Status:** accepted; built and checked in CI; no independent audit claimed
- **Requirements:** RFP-021 S.8, R.6; RFP-022 Sec.1

## Context

This record was written on 27 November 2026 from the commits, after the fact. Commit `42dc111a` took the decision.

RFP-021 S.8 and RFP-022 Sec.1 require independent audits before their respective deployment milestones. Preparing the
scope required a precise quotation boundary for the attestation, bridge, checkpoint, release and vault sources, their
entry points and reused dependencies. Hand-maintained file tables and version pins would drift as programs changed;
scanning every source file would mix production artifacts with tools, tests and experimental programs. No independent
audit is claimed by this scope.

## Options considered

1. Maintain the source inventory, line totals and dependency pins directly in prose. *Reconstructed for contrast*. It
   permits arbitrary curation, but a source or lockfile edit can leave the quotation stale without any failing check.
2. Discover every Rust and Solidity file on the filesystem. *Reconstructed for contrast*. It needs no allowlist, but
   can include untracked, generated, test and off-chain files and does not define the deployed boundary.
3. Select tracked production files through explicit prefixes, generate both requirement inventories and dependency
   pins with a standard-library tool, and require the document to carry those tables exactly.

## Decision

The third. `scripts/audit_scope.py` obtains tracked paths from git and selects only the allowlisted production prefixes.
Separate views cover the RFP-021 attestation surface and the broader RFP-022 surface; shared files intentionally occur
in both and their totals must not be added. The generator classifies physical lines as code, comments or blank, handles
nested comments and quoted strings, and parses the named lockfiles for reused-component versions.

The document lists entry surfaces and audit properties in prose, while CI checks that each generated table appears
exactly and that no corresponding hand-substituted table differs. Tools, UIs, tests, evidence and upstream internals
are outside the quoted source totals. That exclusion defines the requested engagement; it does not declare those
components harmless or remove the need to review integration and trust assumptions.

## Rationale and trade-offs

An explicit allowlist makes scope changes visible in code review, while tracked-file selection and lockfile parsing
make the inventory reproducible at the commit supplied to an auditor. Exact table comparison prevents silent drift.
The two overlapping views answer the two RFP engagements without disguising their shared attestation code.

The generator supplies a boundary and size measures, not a security verdict or an effort estimate. New production
prefixes must be added deliberately, and an omitted prefix is not discovered automatically. Physical line counts do
not measure assurance, and excluded off-chain and upstream code can still be security-critical. The document remains
project-authored preparation: the independent engagements and published reports required by the RFPs are still open.

## Sources

- Commit `42dc111a`; scripts/audit_scope.py (production groups, tracked-file selection, lexer and lockfile parsing)
- `scripts/test_audit_scope.py` (line-classification cases and exact generated-table comparison)
- docs/security/audit-scope.md §1 (the generated inventory, entry points and explicit exclusions)
- docs/security/audit-scope.md §4 (what the scope does not claim)
- `.github/workflows/ci.yml` (the generated-inventory check)
