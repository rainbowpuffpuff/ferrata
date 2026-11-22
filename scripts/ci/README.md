# CI's own checks

The checks that live under `scripts/ci/` and what runs them. `install-risc0.sh` and
`witness-independence.sh` predate this directory's other files; they are documented in
[`docs/guides/ci.md`](../../docs/guides/ci.md) and in ci.yml's own comments.

## check-row-tests.py

**Every hard requirement of RFP-021 (59 rows) and RFP-022 (68 rows) mapped to its tests, and which of them CI
really runs** (RFP-022 Reliability #11; the A4/B4 gates' "one named test per hard requirement, passing in CI"
of [`docs/traceability.md`](../../docs/traceability.md)). It parses the hard row IDs out of traceability.md's
matrices and checks the counts, then reads the map [`row-tests.toml`](row-tests.toml), where every entry is a
`test` (it runs and fails when the requirement breaks: a Rust or Foundry test file, a test script, a workflow
whose job is the test), a `record` (a run's results or log) or a `document`. It fails when a hard row has no
entry, a path is not tracked (entries on another branch are reported as pending), a kind is unknown, a test
path is nothing that can run, a test file is listed as a record, a box or local test has no `run` command,
or **a test is said to run in a CI job that does not run it**: the job's `run:` steps and the shell scripts
they call are read ([`ci_model.py`](ci_model.py)), and one command must select the file — a `cargo test`
whose directory, packages, targets, features, name filters and environment select it (a test that skips
itself unless, say, `FERRATA_E2E` is set needs the command to set it), a `forge test` whose project and
filters select it (inherited test functions included), the script itself invoked, or the job being defined
in the named workflow. A job that runs only when started by hand counts as manual, not as CI.

The summary gives, per RFP, the rows with a test CI runs and the others by what they have instead (a test in
a manual workflow; tests on the build box or by hand; records and documents only); `--list` prints every
row. `--require-ci RFP-022` (or `RFP-021`, `all`) also fails while any hard row of that RFP lacks a test CI
runs: that is RFP-022 R.11's gate, and it fails today. Standard library only.

```sh
python3 -I scripts/ci/check-row-tests.py                          # the map is consistent (CI)
python3 -I scripts/ci/check-row-tests.py --require-ci RFP-022     # the R.11 gate: exit 1 while rows lack a CI test
```

Run by CI's `links` job on every push and pull request, with its tests (below).
[`row-tests.toml`](row-tests.toml) is the map itself, one `[[row]]` per hard row (IDs as in traceability.md;
the rows of the two RFPs that are addressed on branch `rfp-docs-final` carry `branch = "rfp-docs-final"`,
reported as pending until that branch merges; their `ci = "links"` entries need the links job to run
`check-guides.py` and `check-decisions.py` by then, or the check fails).

## ci_model.py and test_ci_model.py

[`ci_model.py`](ci_model.py) is what the two checks above and below read the workflows with: a parser for the
YAML they are written in (standard library; `python3 -I scripts/ci/ci_model.py --compare-yq` compares it with
`yq` on all eleven workflows of the repository and the catalog: the same triggers, jobs, steps, commands,
directories and environments, 11 of 11), the trace of the commands a job runs (expanding `strategy.matrix`
combinations and tracing `scripts/ci/lez-harness.sh --shard NAME` from [`lez-harness-shards.json`](lez-harness-shards.json)), and the
cargo and forge selection rules. It reads statically. A command under a step's or job's `if:` (unless always
true, or a job-level `push || pull_request` event guard on a workflow triggered on both) or `continue-on-error`, in an `if`/`while`/`until` condition, run with `&`, whose failure `|| X` masks,
piped without pipefail, or not last in a text without `set -e` counts for nothing; tracing stops at a top-level `exit` and at `[ A = B ] && exit` when A and B resolve (a
script's arguments are `$1`, `$2` ...), so a step that runs `lez-harness.sh build` or `lez-harness.sh --shard NAME build` runs no test; `-- --skip P`
leaves out a file any of whose tests P could skip; a test that asserts an environment variable's value needs
the command to set it. Other conditions are read as if they held, and a directory it cannot resolve makes the
commands after it count nowhere.

**What it does not catch.** The model reads statically and covers the forms above, not every way of not running a
test. An independent review's mutants (22 November 2026) found these that still count as CI running a test; none is
in today's workflows, so the counts stand:
- in a workflow: a job that `needs:` a job that is skipped (it is skipped too); `on:` triggers whose `paths:` or
  `branches:` filters match nothing pushed (the workflow never runs);
- in a command or script: `--ignored` or `--list` given to the test binary (only ignored tests run, or none, listed);
  `exec` of another command before the test; an `exit` inside a shell function, a compound command
  (`if true; then exit 0; fi`) or an and-list (`true && exit 0`) before the test; `trap 'exit 0' EXIT` or
  `trap 'exit 0' ERR`, which turn a failure into success; the test in a subshell or a brace group whose failure
  `|| true` masks (`( cargo test ) || true`, `{ cargo test; } || true`); a bug inside `lez-harness.sh`'s own
  `--shard` dispatch loop (`ci_model.py` reads `lez-harness-shards.json` statically for `--shard` calls, so the loop
  itself is tested by `test_lez_shards.py`, `--dry-run`, and the daily unsharded `lez-harness-full` backstop);
- in Rust: `cfg` forms other than a bare `feature = "X"` on the file, the test or its `mod` block, such as
  `cfg(all(feature = "X", test))`, `cfg(any(feature = "X", feature = "Y"))` with both off, a file-level
  `#![cfg(all(feature = "X"))]`, `cfg_attr(not(feature = "X"), ignore)`, or another attribute between a `mod`
  block and its gate.

It errs the other way on two forms that do run the test: a step under `if: always()` and one with
`continue-on-error: false` are counted for nothing. A Rust test
compiled out by a feature the command leaves off (`#![cfg(feature = "X")]` on the file, `#[cfg(feature = "X")]`
on the test or its `mod` block) does not count, and a cargo name filter is matched against a test's whole path
(the module path of a file under `src/`, its inner modules, the function), as cargo matches it.
[`test_ci_model.py`](test_ci_model.py) tests both (YAML forms; what the real jobs run and do not run; made-up
`cargo test` and `forge test` commands; and the checker refusing a test claimed for a job that does not run
it, a test file listed as a record, an unknown kind). `python3 -I scripts/ci/ci_model.py --trace [JOB...]`
prints a job's commands with their directories.

```sh
python3 -I scripts/ci/test_ci_model.py
python3 -I scripts/ci/ci_model.py --compare-yq      # needs yq (mikefarah, v4)
```

## check-portable.py

**The shell that runs on macOS is portable** (RFP-022 Supportability #2): the `run:` steps of
`.github/workflows/macos.yml`, every shell script they run (followed through `ci_model.py`'s trace), and the
owner's publish script `modules/scripts/release-publish.sh` with what it calls, are checked line by line for
GNU-only commands and options (`sed -i`, `sed -r`, `find -printf`, `tar --wildcards`, `sha256sum` without the
`shasum` fallback, `readlink -f`, `stat -c`, `date -d`, `grep -P`, `xargs -r`, `nproc`, `timeout`...) and for
anything newer than macOS's bash 3.2 (`declare -A`, `mapfile`, `${x,,}`, `|&`...). On the branch's scripts
before the fixes of 21 November 2026 it reports eleven, the four of the review among them. It does not run
anything; the scripts themselves were run under a FreeBSD userland with bash 3.2, and their exports compared
with GNU's, byte for byte: [`modules/results/bsd-userland-2026-11-21/`](../../modules/results/bsd-userland-2026-11-21/README.md).
`scripts/fetch-vectors.sh` itself ran on the real archives on the build box, 21 November 2026, once with GNU tar
1.35 and once with bsdtar 3.7.2 (libarchive) as `tar`: exit 0 both times (23 s, 30 s), each ending "vectors
verified", that is, the extracted trees equal the pinned tree hashes.

```sh
python3 -I scripts/ci/check-portable.py
```

Run by CI's `links` job on every push and pull request.

## check-unaudited.sh

What the bid promises until the audit reports are published (RFP-021 S.8, RFP-022 Sec.1): **the code
is marked unaudited / not for production** in `README.md`, `SECURITY.md`, the catalog's description
(`modules/catalog/logos-repo.json`) and every `modules/*/metadata.json` description. The two audit rows
can have no functional test until the reports exist, so this is their entry in
[`row-tests.toml`](row-tests.toml). A marking must say it: "unaudited", "not audited", "has not been
audited", "not for production", "unfit for production" - "prototype" alone is a maturity label, not
this promise, and does not pass. Fails naming each file whose marking is missing. bash, grep, python3.

```sh
bash scripts/ci/check-unaudited.sh
```

Run by CI's `links` job on every push and pull request.

## check-actions-pinned.py

That **every third-party action a workflow uses is pinned to a full 40-character commit SHA**, in
`.github/workflows/*.yml` and `modules/catalog/.github/workflows/*.yml`. A moving tag in CI is a
supply-chain hole. Local workflow paths (no `@`) are reported separately; so is a tag followed by exactly the
comment `# TODO: pin to a commit SHA` (a TODO anywhere else on the line does not count); anything else
unpinned fails. The SHAs of most actions cannot be resolved offline here: the two pinned uses (the release
action at `bbdc5287`, `v1.8`, and the `actions/checkout` commit that action itself uses) were taken from the
local clone of `logos-modules-release-action`. On 21 November 2026 it counts 2 pinned, 7 local, 13
TODO-marked (`macos.yml`, `sequencer.yml`) and **21 unpinned** (all in `ci.yml`), so it fails today; it joins
CI's `links` job once the owner has run `pin-actions.py --write`. Standard library only.

```sh
python3 -I scripts/ci/check-actions-pinned.py
```

## pin-actions.py

The owner's one step that makes the check above pass: for every `uses: owner/repo@ref` that is not a commit SHA,
`git ls-remote https://github.com/owner/repo` resolves the tag or branch (an annotated tag to its commit) and the
line becomes `uses: owner/repo@<sha> # <ref>`. It reads the network, so it was tested here only with a stand-in
resolver (every rewrite as expected on `macos.yml`).

```sh
python3 scripts/ci/pin-actions.py            # what it would pin
python3 scripts/ci/pin-actions.py --write    # pin; then add check-actions-pinned.py to the links job
```

## install-risc0.sh

RISC Zero's toolchains for CI, installed with rzup only when missing: in `~/.risc0` cargo-risczero,
r0vm 3.0.5, guest Rust 1.88.0 and C++ 2024.1.5 (bridge/ and checkpoint/); the same with guest Rust
1.91.1 in `~/risc0-home-lez` (LEZ's own guest builds, [`lez/RESULTS.md`](../../lez/RESULTS.md) §8),
kept apart so it does not change `~/.risc0`'s default. Run by CI's `witness-independence` and
`roundtrip` jobs before their builds, with the caches above.

```sh
scripts/ci/install-risc0.sh
```

## witness-independence.sh

The zkVM step of [`bridge/scripts/check.sh`](../../bridge/README.md) in CI: N random valid witnesses
per guest (release, claim), executed in r0vm; fails if a guest's cycle count depends on its witness
(SPEC-v2 §5.2 - a witness-dependent count is a privacy leak, as the one this check found once was).

```sh
scripts/ci/witness-independence.sh 64
```

Run by CI's `witness-independence` job, which passes the witness count.

## check-lez-shards.py, test_lez_shards.py and lez-harness-shards.json

Guard and shard map for the 6-shard `lez-harness` matrix in [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml).
[`check-lez-shards.py`](check-lez-shards.py) derives integration-test binaries from `lez/harness/Cargo.toml` and
`lez/updater/Cargo.toml` (`[[test]]` entries and `package.autotests`) and their `tests/` trees (`tests/*.rs` and
`tests/*/main.rs`; helper directories without `main.rs` such as `redteam_common/` and `security_common/` are modules,
not binaries). It fails when any binary is run by no shard, run by more than one shard (including `privacy_suite`
alongside `privacy: true`), named in [`lez-harness-shards.json`](lez-harness-shards.json) without existing, listed as
feature-gated without a matching `required-features` / `#![cfg(feature = ...)]` gate, when a split binary's `--exact`
and `--skip` test-name lists disagree or name a nonexistent test, when `updater_lib` is missing while `src/` holds
unit tests, or when `ci.yml`'s `lez-harness` matrix disagrees with the shard map. Standard library only.

```sh
python3 -I scripts/ci/check-lez-shards.py    # validate shard map (links job and start of each shard)
python3 -I scripts/ci/test_lez_shards.py     # unit tests on scratch copies (links job)
```

## lez-harness.sh

CI's LEZ suites, run sharded by `lez-harness` (6-shard matrix on push and pull request) and unsharded by
`lez-harness-full` (daily `schedule` at 04:17 UTC and `workflow_dispatch`): every test file of `lez/harness/tests/`
and `lez/updater/tests/` that builds without an optional feature (plus `--lib` unit tests), in RISC Zero dev mode,
and the privacy suite end to end (`scripts/privacy/run.sh`) with its gate (`scripts/privacy/check-results.py`).
Needs Rust 1.98.1, RISC Zero's toolchains with guest Rust 1.91.1 (`install-risc0.sh`), the upstream sources
(`scripts/fetch-upstream.sh`) and Foundry with `vault/lib`.

```sh
scripts/ci/lez-harness.sh                         # build what is missing, then run everything (unsharded)
scripts/ci/lez-harness.sh build                   # build everything only
scripts/ci/lez-harness.sh --shard NAME build      # check shard map, build shard NAME only
scripts/ci/lez-harness.sh --shard NAME            # check shard map, run shard NAME
scripts/ci/lez-harness.sh --dry-run               # print exact commands for every shard (no RISC0_HOME needed)
scripts/ci/lez-harness.sh --dry-run --shard NAME  # print exact commands for shard NAME (add `build` for build commands)
```
