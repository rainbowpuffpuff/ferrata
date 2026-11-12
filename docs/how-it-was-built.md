# How Ferrata was built

Ferrata is one person's work, done with coding agents. This page says what that means for a reader of the
code: who drafted what, what the commit trailers say and do not say, which reviews there were and what they
are not, where the decisions are written down, and what the author has and has not reviewed yet. It is the
one place in the repository that tells this; the [README](../README.md) is about the product.

## Agents draft, the author decides

The code, the tests and the documents were drafted with coding agents (Claude Code) under the author's
design, review and acceptance tests, and the author's decision log; every commit was made this way. Read
each commit as agent-drafted, whether or not it carries an `Assisted-by` trailer.

The author owns the design, the decisions and the acceptance tests: the specification and BLS vectors, the
real-chain replays, the differential oracles, the adversarial and mutation suites and the measurements each
directory documents. For the core, acceptance is mechanical (its
[README](../crates/ferrata-core/README.md#how-this-was-built) lists the tests); and every result in
[EVIDENCE.md](EVIDENCE.md) names where it ran, whether its proofs were real or in dev mode, and the command
that re-checks it, so that each result can be checked without trusting who or what wrote the code.

Most pieces were built on a branch of their own, each by an agent in its own git worktree, and merged into an
integration branch (`integration`, then `integration3` to `integration7`); the merge commits name the
branches. Several agents often worked at the same time on the same machines, so some timings were taken
under their load; the results files say which (for example [`bridge/RESULTS.md`](../bridge/RESULTS.md) and
[`checkpoint/RESULTS.md`](../checkpoint/RESULTS.md)).

## The `Assisted-by` trailers

Not every commit says how it was made. 185 of the 418 commits up to `0b135f7` carry an `Assisted-by`
trailer (`git log --grep=Assisted-by`), every commit made after 04:37 CET on 11 November among them; the
others, most of those of 10 and 11 November, should be read the same way.

At `d81a855` (the tip of `integration7`, 11 November 2026, 23:41 CET), 537 of the 770 commits carry the
trailer: 502 name `claude-opus-5-5` and 35 `claude-fable-5-2` (counted with
`git rev-list --count d81a855` and `git log --format=%B d81a855 | grep '^Assisted-by' | sort | uniq -c`).
The two commits of 04:37 CET on 11 November are the last without it; every commit since carries one.

## Reviews by agents

Agent reviews in a separate context were part of the work; none of them is an audit. No part of this
repository has been audited; the audit is planned for the code the proposal would deliver (README,
"Not audited. Not for production.").

- **The security review of 11 November 2026**: an adversarial reading of the bridge (the vault, the bridge
  program, the checkpoint and release guests, the attestation program and the core's gates it calls) at
  `integration4` (`1bb20d5`) by Claude Code (`claude-opus-5-5`), headless and read-only, in a context
  separate from the agents that wrote the code
  ([`notes/reviews/security/review-2026-11-11.md`](../notes/reviews/security/review-2026-11-11.md)). It
  found 1 critical, 1 high, 3 medium, 4 low and 10 informational issues. Ten are fixed, each with a test
  that fails on `1bb20d5` and passes after the fix; nine are not fixed, each with its reason
  ([`fixes-2026-11-11.md`](../notes/reviews/security/fixes-2026-11-11.md); EVIDENCE row 30).
- **The review of the core against the specifications** (branch `review-core`, merged on 11 November): an
  agent in a separate context read each function against its specification section and planted 24 mutants
  (R01–R13, and S01–S11 in `steps.rs`); 18 were killed by the suite, the other 6 by four new tests in
  `tests/review_core.rs` (EVIDENCE row 1; [REVIEW-core.md](REVIEW-core.md)). Where the repository calls this
  review independent, it means a separate agent context, not a second person.
- **Security tooling and fuzzing**: Slither, Aderyn, Halmos, cargo-audit, cargo-deny and the fuzz targets
  were run, and their findings triaged, on the agents' branches (EVIDENCE rows 35, 36, 40 and 78;
  [`tools-2026-11-11.md`](../notes/reviews/security/tools-2026-11-11.md)). They are tooling, not an audit.

## The decision log

The author keeps a working decision log, started on 9 November 2026. It is a private file, not in this
repository. The decisions that shape the code are written into the repository where they apply: the bridge
specification's changes D1 to D13 ([`bridge-spec-v3-changes.md`](design/spec/bridge-spec-v3-changes.md)),
the design notes ([`attestation.md`](design/attestation.md), [`bridge.md`](design/bridge.md)), the
"Decisions" items of [`lez/RESULTS.md`](../lez/RESULTS.md) §5, and each directory's README and RESULTS file.
The proposal names the decision log among the ways one person's work stays checkable, beside weekly notes,
reproducible builds and a re-check command for every result.

## What the author reviews

[REVIEW-core.md](REVIEW-core.md) maps each function of the core to its specification section for the
author's line-by-line review; its sign-off column is not filled in yet, and the bridge prototypes have no
such map. Under the proposal, consensus-critical and fund-holding code ships only after its vectors, its
adversarial and mutation suites and the author's line-by-line review, signed function by function in that
file: the core first, the vault and the guests by milestones A1 and B1; agent reviews in a separate context
are kept with each milestone and never called an audit.
