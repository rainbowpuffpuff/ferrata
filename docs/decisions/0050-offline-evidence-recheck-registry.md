# ADR 0050: Re-derive committed evidence offline through one registry, with independent seal checks and tamper tests

- **Date:** 26 November 2026
- **Status:** accepted; built and run in CI
- **Requirements:** RFP-021 R.1; RFP-022 R.11

## Context

This record was written on 27 November 2026 from the commits, after the fact. Commits `57009d56` and `f08c8eab` took
the registry and CI decision; `be473f9d` and `e2fc6dbc` added independent light-client record and Groth16 checks.

The evidence index, requirement rows and run documents state figures and verdicts derived from committed records. A
reviewer could read those claims or rerun the original test and proving toolchains, but the former trusts the prose
and the latter takes hours and does not directly compare every published figure with its record. The missing layer was
a quick, uniform way to recompute the evidence itself and to show that each check fails when its input is altered.

## Options considered

1. Leave review to reading the evidence documents and manually selecting their cited scripts. *Reconstructed for contrast*.
   It adds no duplicate parser, but gives no uniform result or proof that a check can detect a changed
   record.
2. Rerun only the original requirement tests. *Recorded:* docs/VERIFY.md §3 retains this path and records its required
   toolchains and long running time. It rebuilds the original behavior, but is not a quick evidence-consistency pass.
3. Maintain one registry of offline checks bound to requirement and evidence rows; independently parse records,
   recompute figures, hashes and signatures, verify seals locally, and run mutation-style tamper tests in CI.

## Decision

The third. `scripts/verify.py` exposes a registry in which each check declares its family, offline or online mode,
requirement rows, evidence rows, inputs, tools and quick or full-test tier. It prints what was re-checked and against
what. The quick offline tier independently derives recorded figures and refusal counts, recomputes hashes and image
ids, checks release signatures, and verifies recorded RISC Zero 3.0.0 and 3.1.0 Groth16 seals against their journals
and image ids with a local implementation.

Every check receives the repository root explicitly, allowing its test to run against an altered copy. The tamper
suites change a seal, transaction, figure, signature or manifest entry and require failure. CI runs all quick offline
checks with strict handling of skips and then their tamper suites. The original toolchain tests remain separately
addressable through the same registry rather than being silently presented as quick checks.

## Rationale and trade-offs

Re-derivation checks the relationship between a claim and its underlying record, not merely that the original program
once exited successfully. A shared row map lets a reviewer select one requirement or evidence row, while fixed failure
and skip semantics make missing tools and no-op tests visible. Independent seal verification checks the mathematical
artifact without invoking the prover that produced it, and tamper tests reduce the chance of a vacuous checker.

The trade-off is a substantial second set of parsers, cryptographic code and expectations that must follow the
protocols and records. Offline verification proves only what the committed inputs encode; it does not establish that
those inputs came from the claimed public chain, cover every case, or constitute an audit. Some local checks need
external command-line tools and otherwise skip; CI's strict mode makes that a failure. The longer original suites and
the separate online read-back remain necessary.

## Sources

- Commits `57009d56` and `f08c8eab`; scripts/verifylib/core.py (the registry, result semantics and repository context)
- Commit `be473f9d`; scripts/verifylib/checks_lc.py and scripts/test_verify_lc.py (independent record derivation and
  its tamper tests)
- Commit `e2fc6dbc`; scripts/verifylib/groth16.py and scripts/verifylib/checks_seals.py (local seal verification)
- `scripts/test_verify.py` (registry and tamper checks) and docs/VERIFY.md §1 (the offline tier and its limits)
- `.github/workflows/ci.yml` (the strict offline verification job)
