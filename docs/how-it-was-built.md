# How Ferrata was built

Ferrata is one person's work, done with coding agents. This page says what that means for a reader of the
code: who drafted what, what the commit trailers say and do not say, which reviews there were and what they
are not, where the decisions are written down, and what the author has and has not reviewed yet. It is the
one place in the repository that tells this; the [README](../README.md) is about the product.

## Agents draft, the author decides

The code, the tests and the documents were drafted with coding agents (Claude Code; one set of CI changes with
Antigravity, below) under the author's design, review and acceptance tests, and the author's decision log;
every commit was made this way. Read each commit as agent-drafted, whether or not it carries an `Assisted-by`
trailer.

The author owns the design, the decisions and the acceptance tests: the specification and BLS vectors, the
real-chain replays, the differential oracles, the adversarial and mutation suites and the measurements each
directory documents. For the core, acceptance is mechanical (its
[README](../crates/ferrata-core/README.md#how-this-was-built) lists the tests); and every result in
[EVIDENCE.md](EVIDENCE.md) names where it ran, whether its proofs were real or in dev mode, and the command
that re-checks it, so that each result can be checked without trusting who or what wrote the code.

Most pieces were built on a branch of their own, each by an agent in its own git worktree, and merged into an
integration branch (`integration`, then `integration3` to `integration8`); the merge commits name the
branches. Several agents often worked at the same time on the same machines, so some timings were taken
under their load; the results files say which (for example [`bridge/RESULTS.md`](../bridge/RESULTS.md) and
[`checkpoint/RESULTS.md`](../checkpoint/RESULTS.md)).

## The `Assisted-by` trailers

Not every commit says how it was made. 185 of the 418 commits up to `2854e4e` carry an `Assisted-by`
trailer (`git log --grep=Assisted-by`), every commit made after 04:37 CET on 11 November among them; the
others, most of those of 10 and 11 November, should be read the same way.

At `b4ad05f` (the tip of `integration7`, 11 November 2026, 23:41 CET), 537 of the 770 commits carry the
trailer: 502 name `claude-opus-5-5` and 35 `claude-fable-5-2` (counted with
`git rev-list --count b4ad05f` and `git log --format=%B b4ad05f | grep '^Assisted-by' | sort | uniq -c`).
The two commits of 04:37 CET on 11 November are the last without it; every commit since carries one.

Some drafts were written with GLM 5.3 through Claude Code and reviewed by Opus; their commits carry
`Assisted-by: Claude Code (glm-5.3)`, beside the Opus trailer where Opus finished or rewrote them.

Two pieces merged on 23 November 2026 were drafted outside Opus. The CI changes (the sharded `lez-harness`
matrix and its coverage guard, the daily unsharded run in `lez-full.yml`, the concurrency groups, the step
timeouts, the macOS runner and the round trip's bounded forge deploy) were drafted by Gemini 4 Neon through
Antigravity; their commits carry `Assisted-by: Antigravity (gemini-4-neon-xhigh)`. The performance-records test
(`scripts/test_perf_records.py`) and the operator guide's section on stall detection (§3.5) were drafted by
GLM 5.3 through Claude Code (`Assisted-by: Claude Code (glm-5.3)`). Opus (`claude-opus-5-5`) reviewed each
twice in a separate context, its drafter fixed the findings, and Opus merged both and folded the status
changes they proposed into the status documents.

The Helios step-path oracle (RFP-021 R.6: `crates/ferrata-spectests/tests/helios_step_oracle.rs`,
`scripts/helios-oracle-full.sh`), merged on 25 November 2026, was drafted by Gemini 4 Neon through Antigravity and
reviewed twice by Opus in a separate context; it was merged as one commit, which carries both trailers.

## Reviews by agents

Agent reviews in a separate context were part of the work; none of them is an audit. No part of this
repository has been audited; the audit is planned for the code the proposal would deliver (README,
"Not audited. Not for production.").

- **The security review of 11 November 2026**: an adversarial reading of the bridge (the vault, the bridge
  program, the checkpoint and release guests, the attestation program and the core's gates it calls) at
  `integration4` (`bc4a2a6`) by Claude Code (`claude-opus-5-5`), headless and read-only, in a context
  separate from the agents that wrote the code
  ([`docs/reviews/security/review-2026-11-11.md`](reviews/security/review-2026-11-11.md)). It
  found 1 critical, 1 high, 3 medium, 4 low and 10 informational issues. Ten are fixed, each with a test
  that fails on `bc4a2a6` and passes after the fix; nine are not fixed, each with its reason
  ([`fixes-2026-11-11.md`](reviews/security/fixes-2026-11-11.md); EVIDENCE row 30).
- **The review of the core against the specifications** (branch `review-core`, merged on 11 November): an
  agent in a separate context read each function against its specification section and planted 24 mutants
  (R01–R13, and S01–S11 in `steps.rs`); 18 were killed by the suite, the other 6 by four new tests in
  `tests/review_core.rs` (EVIDENCE row 1; [REVIEW-core.md](REVIEW-core.md)). Where the repository calls this
  review independent, it means a separate agent context, not a second person.
- **Security tooling and fuzzing**: Slither, Aderyn, Halmos, cargo-audit, cargo-deny and the fuzz targets
  were run, and their findings triaged, on the agents' branches (EVIDENCE rows 35, 36, 40 and 78;
  [`tools-2026-11-11.md`](reviews/security/tools-2026-11-11.md)). They are tooling, not an audit.

## The decision log

The author keeps a working decision log, started on 9 November 2026. It is a private file, not in this
repository. The decisions that shape the code are written into the repository where they apply: the bridge
specification's changes D1 to D13 ([`bridge-spec-v3-changes.md`](design/spec/bridge-spec-v3-changes.md)),
the design notes ([`attestation.md`](design/attestation.md), [`bridge.md`](design/bridge.md)), the
"Decisions" items of [`lez/RESULTS.md`](../lez/RESULTS.md) §5, and each directory's README and RESULTS file.
Since 21 November 2026 the major ones are also gathered in [`decisions/`](decisions/README.md), one record per
decision with its context, options, choice and rationale (RFP-021 S.11, RFP-022 S.4), reconstructed from those
files, the five of 21 November written that day; the private log was not copied into it.
The proposal names the decision log among the ways one person's work stays checkable, beside weekly notes,
reproducible builds and a re-check command for every result.

## What the author reviews

[REVIEW-core.md](REVIEW-core.md) maps each function of the core to its specification section for the
author's line-by-line review; the author signed every row on 12 November 2026 (Carol Calin, after the agent
review of 11 November). The same maps for the bridge prototypes, [REVIEW-vault.md](REVIEW-vault.md) (the vault and
its library, 52 rows) and [REVIEW-guests.md](REVIEW-guests.md) (every guest's entry points and checks, 55 rows),
are drafted (12 November 2026, with an agent review in a separate context) and not yet signed. Under the proposal, consensus-critical and fund-holding code ships only after its vectors, its
adversarial and mutation suites and the author's line-by-line review, signed function by function in that
file: the core's 48 rows are signed (12 November 2026); the program's on-chain steps (`steps.rs`, `witness.rs`) follow by milestone A1, the vault and the guests by B1; agent reviews in a separate context
are kept with each milestone and never called an audit.
