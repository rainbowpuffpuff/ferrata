# Stream ci-shards: sharded `lez-harness`, daily backstop, and `macos-15`, proposed row edits

For the integrator of `docs/EVIDENCE.md`, `docs/requirements-status.md`, `docs/traceability.md`, and `README.md`.
Per scope rules, none of those four files was edited on branch `ci-shards` (branched from `8c38d723`). Exact old and
new text for each proposed edit is given below.

No program, guest, image id, or dependency changed. Nothing was pushed; the real proof of the sharded workflow and of
`macos-15` is a GitHub Actions run started by the owner after review.

## What changed on branch `ci-shards`

1. **Sharded `lez-harness` matrix (`.github/workflows/ci.yml`, `scripts/ci/lez-harness-shards.json`, `scripts/ci/lez-harness.sh`).**
   Based on the measured per-binary timings from GitHub Actions run `45072883393` (job `137363580490`, 4,474 s /
   74.6 min total: setup 137 s, build 809 s, warm rebuild 59 s, harness 2,158 s, updater 1,217 s, privacy suite + gate
   88 s), `lez-harness` now runs as a 6-shard matrix on `push` and `pull_request` (`fail-fast: false`, job id
   `lez-harness` preserved): `harness-1`–`harness-4` (511.9–516.7 s / 8.5–8.6 min of tests each), `updater-gate`
   (`lez/updater/tests/recorded.rs` split with `--exact` for `gate_two_honest_one_adversary` + 4 heavy `recorded` tests
   running in parallel on the other 3 cores, derived 724.8 s / 12.1 min), and `updater-privacy` (remaining 10 updater
   binaries, `ferrata-updater --lib`, `recorded` with `--exact --skip` for those five names, and `scripts/privacy/run.sh`
   + `scripts/privacy/check-results.py`, derived ~554 s / 9.2 min plus ~15 s warm relink). Expected push/PR workflow
   wall time is derived at ~27 min on run `45072883393`'s runner speed (~22 min on run `45075900693`'s where the build
   was 512 s). Unsharded `scripts/ci/lez-harness.sh` and `scripts/ci/lez-harness.sh build` remain unchanged.
2. **Shard coverage guard (`scripts/ci/check-lez-shards.py`, `scripts/ci/test_lez_shards.py`) and static CI model (`scripts/ci/ci_model.py`, `scripts/ci/test_ci_model.py`).**
   Derives binaries from `lez/harness/Cargo.toml` and `lez/updater/Cargo.toml` (`[[test]]`, `autotests`) and `tests/`
   (`*.rs` and `*/main.rs`, ignoring module subdirs like `redteam_common/` and `security_common/`), enforces complete
   disjoint coverage, feature-gated honesty, exact/skip split partitions, and `ci.yml` matrix equality. Runs in `links`
   and at the start of every `lez-harness` shard.
3. **Daily backstop (`lez-harness-full` in `.github/workflows/ci.yml`).**
   Runs `scripts/ci/lez-harness.sh` unsharded once on `schedule` (`17 4 * * *`, daily 04:17 UTC) and
   `workflow_dispatch`, keeping `permissions: contents: read`. Sharded `lez-harness` is skipped on those triggers; the
   other 12 jobs still run.
4. **macOS runner pin (`.github/workflows/macos.yml`).**
   Replaced retired `runs-on: macos-14` with currently supported pinned `runs-on: macos-15` in `user-side` and `modules`.

## Proposed edits to `docs/EVIDENCE.md`

### 1. Amend row 207

Exact old text (`docs/EVIDENCE.md`, row 207):

```markdown
| 207 | **CI runs the LEZ suites and the watchers (RFP-022 R.11; RFP-021 and RFP-022 rows with a test CI runs)**: job `lez-harness` (`scripts/ci/lez-harness.sh`: every test file of `lez/harness/tests/` and `lez/updater/tests/` that builds without an optional feature, RISC Zero dev mode, then the privacy suite and its gate) and job `monitor` (its tests and the anvil rehearsal); the roundtrip job runs the relayer's library and gas-drop tests, the witness-independence job the bridge core's tests; the row checker counts a test only when a job that runs on push or pull request runs it with a command that can fail the job (what its static model does not catch is listed in `scripts/ci/README.md`) | the job's script once at 4 threads: build 51 s, harness 2,196 s, updater 1,113 s, privacy suite and gate 93 s, 3,453 s in all; 77 test binaries ok, 0 failed; the gate 33 of 33; rows with a test CI runs: RFP-021 54 of 59, RFP-022 64 of 68 (`check-row-tests.py --list`; `test_ci_model.py` 88 passed) | the Modal build box, 4 CPUs (`taskset -c 0-3`), dev mode; not run on GitHub yet | none (dev mode) | `bash scripts/ci/lez-harness.sh`; `python3 -I scripts/ci/check-row-tests.py --list` | [`docs/guides/ci.md`](guides/ci.md); [`scripts/ci/README.md`](../scripts/ci/README.md) |
```

Exact new text:

```markdown
| 207 | **CI runs the LEZ suites and the watchers (RFP-022 R.11; RFP-021 and RFP-022 rows with a test CI runs)**: job `lez-harness` (`scripts/ci/lez-harness.sh --shard "${{ matrix.shard }}"`, 6-shard matrix in `scripts/ci/lez-harness-shards.json`, guarded by `scripts/ci/check-lez-shards.py` in `links` and at the start of every shard; plus daily/manual unsharded backstop `lez-harness-full`: every test file of `lez/harness/tests/` and `lez/updater/tests/` that builds without an optional feature, RISC Zero dev mode, then the privacy suite and its gate) and job `monitor` (its tests and the anvil rehearsal); the roundtrip job runs the relayer's library and gas-drop tests, the witness-independence job the bridge core's tests; the row checker counts a test only when a job that runs on push or pull request runs it with a command that can fail the job (what its static model does not catch is listed in `scripts/ci/README.md`) | the job's script once at 4 threads on the build box: build 51 s, harness 2,196 s, updater 1,113 s, privacy suite and gate 93 s, 3,453 s in all; 77 test binaries ok, 0 failed; the gate 33 of 33; unsharded on GitHub Actions run `45072883393` (job `137363580490`, `ff0cb39a`): 4,474 s (74.6 min) passed (setup 137 s, build 809 s, warm rebuild 59 s, harness 2,158 s, updater 1,217 s, privacy + gate 88 s, 78 test runs ok, gate 33/33); sharded on branch `ci-shards` (not yet run on GitHub; derived ~27 min wall time at run `45072883393`'s runner speed): `check-lez-shards.py` ok, `test_lez_shards.py` 21 passed, `test_ci_model.py` 98 passed, rows with a test CI runs unchanged: RFP-021 54 of 59, RFP-022 64 of 68 (`check-row-tests.py --list`) | the Modal build box, 4 CPUs (`taskset -c 0-3`), dev mode; GitHub Actions `ubuntu-latest` (unsharded run `45072883393`; sharded workflow pending first GitHub run) | none (dev mode) | `bash scripts/ci/lez-harness.sh`; `python3 -I scripts/ci/check-lez-shards.py`; `python3 -I scripts/ci/check-row-tests.py --list` | [`docs/guides/ci.md`](guides/ci.md); [`scripts/ci/README.md`](../scripts/ci/README.md) |
```

### 2. Amend row 165 (`macos.yml` runner label)

Exact old text (`docs/EVIDENCE.md`, row 165):

```markdown
| 165 | **The macOS workflow's scripts, portable (RFP-022 S.2, in part)** (not run on macOS) | `.github/workflows/macos.yml` (the user-side crates' tests; five modules' `.lgx` built with Nix and verified); `scripts/ci/check-portable.py` (CI `links` job): nothing GNU-only or newer than bash 3.2 in what it runs (7 scripts, 16 steps); under a FreeBSD userland with bash 3.2.57 the five module exports equal GNU's byte for byte and the publish tests pass 68 of 68, the current 83 of 83 under bash 3.2.57; `fetch-vectors.sh` verifies the real archives with bsdtar 3.7.2 and GNU tar 1.35 | laptop container (FreeBSD userland); Modal build box; 21 Nov 2026 | none | `python3 -I scripts/ci/check-portable.py` | [`bsd-userland-2026-11-21/`](../modules/results/bsd-userland-2026-11-21/README.md), [`bash32-2026-11-21/`](../modules/results/bash32-2026-11-21/README.md) |
```

Exact new text:

```markdown
| 165 | **The macOS workflow's scripts, portable (RFP-022 S.2, in part)** (not yet green on macOS: earlier runs on retired `macos-14` were cancelled; moved to pinned `macos-15` on branch `ci-shards`) | `.github/workflows/macos.yml` (`runs-on: macos-15`: the user-side crates' tests; five modules' `.lgx` built with Nix and verified); `scripts/ci/check-portable.py` (CI `links` job): nothing GNU-only or newer than bash 3.2 in what it runs (7 scripts, 16 steps); under a FreeBSD userland with bash 3.2.57 the five module exports equal GNU's byte for byte and the publish tests pass 68 of 68, the current 83 of 83 under bash 3.2.57; `fetch-vectors.sh` verifies the real archives with bsdtar 3.7.2 and GNU tar 1.35 | laptop container (FreeBSD userland); Modal build box; 21 Nov 2026 (runner label updated 22 Nov 2026) | none | `python3 -I scripts/ci/check-portable.py` | [`bsd-userland-2026-11-21/`](../modules/results/bsd-userland-2026-11-21/README.md), [`bash32-2026-11-21/`](../modules/results/bash32-2026-11-21/README.md) |
```

## Proposed edits to `docs/requirements-status.md`

Status classifications and counts do **not** change (the sharded workflow and `macos-15` have not completed a green
GitHub Actions run yet).

### 1. RFP-021 R.12

Exact old text (`docs/requirements-status.md`, line 96):

```markdown
| R.12 | in part | Seven CI jobs passed on GitHub at `main` (15 and 18 November); the six jobs added since (`lez-harness`, `monitor`, `deps`, `modules-resolve`, `release-publish`, `image-ids`) have not run on GitHub: a green run of every job is still needed. | GitHub runs 43730879603 and 44172503761; `.github/workflows/ci.yml` |
```

Exact new text:

```markdown
| R.12 | in part | Seven CI jobs passed on GitHub at `main` (15 and 18 November); in run `45072883393` (`ff0cb39a`, 22 November), 12 of 13 jobs passed on GitHub (including `lez-harness`, `monitor`, `deps`, `modules-resolve`, `release-publish`, `image-ids`) while `links` failed on `test_release_manifest` (fixed in `8c38d723`). On branch `ci-shards`, `lez-harness` is split into a 6-shard matrix with a daily unsharded `lez-harness-full` backstop: a single green run of every job at `main` is still needed. | GitHub runs 43730879603, 44172503761, 45072883393; `.github/workflows/ci.yml` |
```

### 2. RFP-022 S.2

Exact old text (`docs/requirements-status.md`, line 228):

```markdown
| S.2 | in part | User-side proofs built and proven on macOS; a macOS workflow for the user-side crates and five modules is written and its scripts are portable (the same exports under a FreeBSD userland and bash 3.2), never run on macOS (no push); the modules and the catalog run on Linux x86-64 only. | 88, 59–62, 94–98, 165; `.github/workflows/macos.yml` |
```

Exact new text:

```markdown
| S.2 | in part | User-side proofs built and proven on macOS; a macOS workflow for the user-side crates and five modules is written (`runs-on: macos-15`, moved from retired `macos-14`) and its scripts are portable (the same exports under a FreeBSD userland and bash 3.2), not yet green on GitHub's macOS runner; the modules and the catalog run on Linux x86-64 only. | 88, 59–62, 94–98, 165; `.github/workflows/macos.yml` |
```

*(RFP-022 R.11 in `docs/requirements-status.md` line 206 needs no change: the job id `lez-harness` and the counts 64/68 and 54/59 are unchanged.)*

## Proposed edits to `docs/traceability.md`

### 1. RFP-021 R.12

Exact old text (`docs/traceability.md`, line 96):

```markdown
| R.12 | CI green on default branch. | Each milestone is a tagged release with CI passing; A4's gate requires CI on main with none skipped. | §A8; milestone A4 | `.github/workflows/ci.yml` defines the jobs; the eighth pass ran each on the build box ([EVIDENCE](EVIDENCE.md), "Suites at this commit"); on 15 November 2026 all seven jobs (`test`, `witness-independence`, `roundtrip`, `vault`, `links`, `idl`, `admin`) passed on GitHub at `main`, commit `3e002fd1` ([run 43730879603](https://github.com/rainbowpuffpuff/ferrata/actions/runs/43730879603)). Read again on 18 November 2026: the five most recent runs on `main` passed, the latest at `3c50dc01` on 17 November ([run 44172503761](https://github.com/rainbowpuffpuff/ferrata/actions/runs/44172503761)). The workflow has since gained six jobs (`lez-harness`, `monitor`, `deps`, `modules-resolve`, `release-publish`, `image-ids`; each run on the build box with the job's own commands) that have not run on GitHub: in part until a run of every job passes on the default branch. | `.github/workflows/ci.yml`; [`guides/ci.md`](guides/ci.md) | A1–A4, R |
```

Exact new text:

```markdown
| R.12 | CI green on default branch. | Each milestone is a tagged release with CI passing; A4's gate requires CI on main with none skipped. | §A8; milestone A4 | `.github/workflows/ci.yml` defines the jobs; the eighth pass ran each on the build box ([EVIDENCE](EVIDENCE.md), "Suites at this commit"); on 15 November 2026 all seven jobs (`test`, `witness-independence`, `roundtrip`, `vault`, `links`, `idl`, `admin`) passed on GitHub at `main`, commit `3e002fd1` ([run 43730879603](https://github.com/rainbowpuffpuff/ferrata/actions/runs/43730879603)). Read again on 18 November 2026: the five most recent runs on `main` passed, the latest at `3c50dc01` on 17 November ([run 44172503761](https://github.com/rainbowpuffpuff/ferrata/actions/runs/44172503761)). In run `45072883393` (`ff0cb39a`, 22 November 2026), 12 of the 13 jobs passed on GitHub (`lez-harness` in 74.6 min, `monitor`, `deps`, `modules-resolve`, `release-publish`, `image-ids` included) while `links` failed on `test_release_manifest` (fixed in `8c38d723`). On branch `ci-shards`, `lez-harness` runs as a 6-shard matrix on push/PR (guarded by `scripts/ci/check-lez-shards.py`) with a daily unsharded `lez-harness-full` backstop: in part until a single green run of every job passes on the default branch. | `.github/workflows/ci.yml`; [`guides/ci.md`](guides/ci.md) | A1–A4, R |
```

### 2. RFP-022 S.2

Exact old text (`docs/traceability.md`, line 226):

```markdown
| S.2 | Modules work on Linux and macOS. | Everything runs on Linux and macOS except Groth16 wrapping, which needs Linux x86; macOS users rely on a relayer or another x86 machine for the wrap. B4's gate: the catalog installs on Linux and macOS. | §B7 | User-side proofs built natively and proven under macOS 15.8 on Apple Silicon, CPU only (M1 Mac mini with 16 GB and M4; [EVIDENCE](EVIDENCE.md) row 88). The modules on macOS: a workflow (`.github/workflows/macos.yml`) for the user-side crates and five modules, its scripts portable (no GNU-only command or bash-4 construct; under a FreeBSD userland with bash 3.2.57 the same exports), not run on macOS (no push; row 165); built and installed on Linux x86-64 only (rows 59–62, 94–98). | [`lez/results/p4-macos/README.md`](../lez/results/p4-macos/README.md); `scripts/ci/check-portable.py` | B3, B4 |
```

Exact new text:

```markdown
| S.2 | Modules work on Linux and macOS. | Everything runs on Linux and macOS except Groth16 wrapping, which needs Linux x86; macOS users rely on a relayer or another x86 machine for the wrap. B4's gate: the catalog installs on Linux and macOS. | §B7 | User-side proofs built natively and proven under macOS 15.8 on Apple Silicon, CPU only (M1 Mac mini with 16 GB and M4; [EVIDENCE](EVIDENCE.md) row 88). The modules on macOS: a workflow (`.github/workflows/macos.yml`, `runs-on: macos-15`, moved from retired `macos-14`) for the user-side crates and five modules, its scripts portable (no GNU-only command or bash-4 construct; under a FreeBSD userland with bash 3.2.57 the same exports), not yet green on GitHub's macOS runner (row 165); built and installed on Linux x86-64 only (rows 59–62, 94–98). | [`lez/results/p4-macos/README.md`](../lez/results/p4-macos/README.md); `scripts/ci/check-portable.py` | B3, B4 |
```

*(All other `lez-harness` references in `docs/traceability.md` remain valid as-is because the job id `lez-harness` is preserved.)*

## `README.md` status lines

Checked `README.md`: no status line mentions `lez-harness`, `macos-14`, or `macos.yml`, and the requirement counts
(RFP-021 39/16/1/3 of 59; RFP-022 43/22/1/2 of 68) are unchanged. No `README.md` edit is needed.
