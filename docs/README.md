# Documentation

Every document in this folder, one line each. Start with [EVIDENCE.md](EVIDENCE.md).

## Evidence and status

- [EVIDENCE.md](EVIDENCE.md): the index. One row per piece of evidence: its numbers, where it ran, whether its
  proofs were real, and the command that re-checks it.
- [VERIFY.md](VERIFY.md): the reviewer's path from `git clone` to every check `scripts/verify.py` makes of the
  evidence, offline then online, with the named tests and a round trip of your own.
- [cli-coverage.md](cli-coverage.md): every component and user-facing feature with the command line that exercises
  it, the test that runs that command, whether it is offline or online, and what is not reachable from a terminal.
- [traceability.md](traceability.md): every requirement of RFP-021 and RFP-022 with its design, the evidence that
  exists today and the milestone that would deliver it.
- [requirements-status.md](requirements-status.md): the status of every requirement row (shown, in part, not yet, waits on
  Logos) with its reason, and the counts; the project's own strict classification of 18 November 2026.
- [components.md](components.md): every component, where it ran, its status and its re-check command.
- [not-built.md](not-built.md): what the design notes specify and this repository does not build yet, in full.
- [sepolia.md](sepolia.md): the live runs on Sepolia, the three deployed vaults, and how to check them.
- [image-ids.md](image-ids.md): the image ids of the final programs, which any checkout rebuilds.
- [port-v0.4.0.md](port-v0.4.0.md): the port from LEZ v0.4.0-rc3 to v0.4.0: every LEZ change that touches
  Ferrata, the files it touches, who ports it and its status (in progress).
- [privacy/RESULTS.md](privacy/RESULTS.md): the privacy suite and its results.

## Design notes

- [docs/design/attestation.md](design/attestation.md): the attestation primitive (RFP-021).
- [docs/design/bridge.md](design/bridge.md): the bridge (RFP-022).
- [docs/design/lez-v0.4-findings.md](design/lez-v0.4-findings.md): what the LEZ v0.4 and Logos
  Blockchain sources say about bridging to Ethereum, with line links.
- [docs/design/finality-rule/](design/finality-rule/): [MODEL.md](design/finality-rule/MODEL.md),
  [LEMMA.md](design/finality-rule/LEMMA.md) and [RESULTS.md](design/finality-rule/RESULTS.md),
  the analysis of finality rule (A).
- [docs/design/spec/](design/spec/): the bridge specification,
  [v2](design/spec/bridge-spec-v2.md) and [the v3 changes](design/spec/bridge-spec-v3-changes.md),
  which the prototypes cite as SPEC-v2 and SPEC-v3.
- [docs/design/risc0-3.1-migration.md](design/risc0-3.1-migration.md): moving to RISC Zero 3.1 after the Groth16 advisories: what moves, in what order, what waits on Logos, the tests.
- [docs/traceability.md](traceability.md): every requirement of RFP-021 and RFP-022 with its design,
  the evidence that exists today and the milestone that would deliver it.
- [docs/REVIEW-core.md](REVIEW-core.md): each function of the core against its specification
  section, with deviations and the tests that pin it.
- [docs/REVIEW-vault.md](REVIEW-vault.md) and [docs/REVIEW-guests.md](REVIEW-guests.md): the same for
  the vault and its library (52 rows) and for every guest program's entry points and checks (55 rows); drafted on
  12 November 2026 with an agent review, not yet signed.

## Guides, trust and performance

The documents the two RFPs ask for, written from the results and design notes above and revised for delivery on
21 November 2026 (`performance.md`: its second draft the same day, revised for delivery on 22 November); every statement cites the file it rests
on, and each says what is not built.

- [docs/guides/consumer-guide.md](guides/consumer-guide.md): consuming attestations from a LEZ
  program (public call, private read), statement identifiers, the Recommended Consumer Pattern, error
  codes, chain binding, migration (RFP-021 S.3, S.4).
- [docs/guides/ci.md](guides/ci.md): how CI works: the jobs, their caches, and what the GitHub runner
  differs in from the box the jobs were proven on.
- [docs/guides/operator-guide.md](guides/operator-guide.md): the updater, the lab, the root
  importer, the checkpoint prover and relayer, watchers, keys, costs, failure modes, retirement
  (RFP-021 S.5, S.7; RFP-022 R.13).
- [docs/guides/deployer-guide.md](guides/deployer-guide.md): deploying the programs on LEZ and the
  vault on an EVM chain, gas limits under Glamsterdam's state gas, configuration and pairing
  (RFP-022 R.14).
- [docs/guides/user-guide.md](guides/user-guide.md): deposit, claim, burn, release, recovery from
  the seed, what each step reveals, defaults, and the developer integration journey (RFP-022 U.5–U.8,
  R.12).
- [docs/guides/verify.md](guides/verify.md): check it yourself: rebuild the crown run's programs to
  their image ids (`scripts/reproduce-image-ids.sh`) and check a Sepolia run from its records against any JSON-RPC
  endpoint (`scripts/verify-sepolia.py`), with what each check proves and what it still trusts.
- [docs/trust.md](trust.md): who is trusted for what, what each failure does and what bounds the
  loss, residual privacy correlations (RFP-021 S.6; RFP-022 Security).
- [docs/performance.md](performance.md): RFP-021 P.1–P.9 and RFP-022 P.1–P.7 with where each figure
  was measured, and the gaps.
- [guides/quickstart.md](guides/quickstart.md): every command of the quickstart, with the toolchains each needs.

## How the repository is made

- [how-it-was-built.md](how-it-was-built.md): how the code, the tests and the documents were drafted with coding
  agents, and what the commit trailers and the reviews do and do not cover.
- [release-engineering.md](release-engineering.md): one command per job (`just bench`, `just m0`, `just release`,
  `just deploy-testnet`).
- [reviews/](reviews/): the agents' reviews of the core, the vault and the guests, and the security review of
  11 November 2026 with its tool outputs. None is an audit.
- [security/](security/): dated security notes; 21 November 2026: RISC Zero's Groth16 advisories and the Sepolia vaults; 27 November 2026: the [internal review of the Ethereum half](security/internal-review-2026-11-27.md) (not an audit) and [what happens if LEZ's receipt check is unsound](security/lez-receipt-unsound.md) (Security #9).
- [history-rewrite.md](history-rewrite.md): why commit ids changed on 15 November 2026, with the table from old to
  new.
