# Stream fix-supply: proposed changes for the integrator

Branch `fix-supply` (from `42b14ca4`). This stream fixes the release-supply-chain and image-id-tooling findings of the
ops code review of 21 November 2026 (review-code-ops, at `5cf1e8e7`: M1, M2, M4, L1, L2, L3, L4, L6, L9 in part, L11,
L12) and, in a second pass, the findings of the branch's own review (review-fix-supply, at `a85da7db`: M-1, M-2, M-3
and the Lows). It owns no requirement row and moves none; it asks for one new EVIDENCE row, a README sentence and CI
changes that only the CI stream may make. Files it changed that other streams may also touch: `deny.toml`,
`gpu-prover/deny.toml` (new), the root `Cargo.toml` (`rust-version`, `exclude`), `justfile`,
`scripts/deploy-testnet.sh` (one check before the EVM step), `docs/guides/verify.md` (one sentence on `compare`),
`gpu-prover/README.md` (one dated sentence).

## Requirement rows

No status changes. Two rows rest partly on what this branch hardens, with no change of status or reason proposed:

| Row | Now | Effect of this branch |
|---|---|---|
| RFP-021 VS.3 ("image ids reproducible") | in part | `scripts/reproduce-image-ids.sh compare` fails closed now (an unreadable vault, an id `r0vm` did not compute, conflicting stale build outputs, a missing guest count as differences; it checks LEZ's commit before building). The earlier recorded comparisons (rows 82, 92 and the crown runs) were made with fresh target directories and a readable vault, so they stand. A release build of the branch tip on the build box gives 6 of 6 crown ids and 14 of 14 guests (fix-supply-2 box run 2b). |
| RFP-021 R.12 | shown | Unchanged here; the CI jobs below would add gates to the green run it cites (the integrator's call, with the CI stream). |

## New EVIDENCE row (number: the next free one, 155 at `42b14ca4`)

To paste into `docs/EVIDENCE.md`'s table (same columns; links written for that file):

| 155 | The release's supply chain and licences, and fail-closed image-id tooling (review-code-ops M1, M2, M4, L1–L4, L6, L11, L12; review-fix-supply M-1–M-3 and its Lows; branch `fix-supply`) | **measured**, Modal build box and laptop, 22 November 2026. The CLI's two prebuilt native archives pinned by sha256 in [`scripts/native-deps.json`](../scripts/native-deps.json) (rapidsnark-linux-x86_64-pic-v0.0.8.zip 585,355 B `59bdd709…cd831`, GitHub's digest and two downloads a day apart, built by the workflow at the tag's commit `8f51c339`; logos-blockchain-circuits-v0.5.7-linux-x86_64.tar.gz 30,023,192 B `92a0c6ad…0f7f`, GitHub's digest and the circuits repository's own Nix pin), fetched and checked by [`scripts/fetch_native.py`](../scripts/fetch_native.py). A release is made only from a build whose every step passed: `release.sh build` writes `BUILD-OK` only then, the CLI it ships is the binary `fetch_native.py check-build` passed (built `--locked` from the checked archives, cargo reporting success, no build script linking native code from elsewhere) with that record, and `package` and `verify` refuse anything else (the review's case on a real build directory: a CLI step marked failed and another `cli/ferrata` is refused; a failing CLI build leaves no `BUILD-OK`, no CLI and no native lines). Every release carries `THIRD-PARTY-NOTICES.md` (the native code: as a combined work the binary is conveyed under the GPL version 3), and `THIRD-PARTY-CRATES.md` generated from the build's own graph by [`scripts/crate_notices.py`](../scripts/crate_notices.py): 638 third-party crates and 27 of the standard library, each with its licence and the licence and notice files of its package (AWS-LC's, ring's, RocksDB's and NVTX's included, with the SPDX identifiers and copyright lines of their C sources); `verify --strict` checks the notices against the release's lockfile and dependency set and the crates in the binary's symbols (235 of the set and 14 of the standard library seen, 0 unlisted). The branch tip's release on the box: build, package, `verify --strict` pass; its six image ids equal the crown run's table, 14 of 14 guests equal `docs/image-ids.md`. `cargo deny` over every workspace with [`scripts/deny_all.py`](../scripts/deny_all.py): 39 of 51 pass bans, licences and sources with advisories against the per-workspace baseline, 12 skipped (LEZ v0.3.1 and spel sources absent; six unstaged module libraries), 0 fail; a workspace whose dependencies cannot be fetched now fails (empty cargo home, offline: 3 of 3). Tests (stdlib, laptop and box): 22 native fetch, 24 manifest and crate notices, 13 release scripts (the release path end to end with stubs), 13 cargo-deny runner, 18 image-id tooling, 8 vault libraries. | [`docs/release-engineering.md`](release-engineering.md) §3; [`releases/third-party/`](../releases/third-party/THIRD-PARTY-NOTICES.md); [`deny.toml`](../deny.toml) |

## README (review-fix-supply L-9)

`README.md:250-251` ("MIT OR Apache-2.0 … for every directory, the imported ones included") is true of the source but
not of the released binary, which links GPL-3.0 and LGPL-3.0 native code. Proposed replacement for the section's text:

> The source is MIT OR Apache-2.0, at your option ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)), for
> every directory, the imported ones included. The `ferrata` binary of a release links native code under the GPL and
> LGPL version 3 (circom's runtime, GMP, rapidsnark, ffiasm, circomlib), so as a combined work it is conveyed under the
> GPL version 3; every release carries its notices ([releases/third-party/](releases/third-party/THIRD-PARTY-NOTICES.md),
> and `THIRD-PARTY-CRATES.md` for the Rust crates in it).

## CI changes the integrator must add (this branch does not edit `.github/workflows/` or `scripts/ci/`)

1. **`deps` job (M4).** Install cargo-deny 0.20.2 from its release asset, checked:
   `cargo-deny-0.20.2-x86_64-unknown-linux-musl.tar.gz`, sha256
   `9f12ed4c49936e09b48bf862b595cde2fe64fcbd9d74dfacac6131ca824c8d5f` (= the asset's `.sha256` and GitHub's digest);
   `scripts/fetch-upstream.sh`; then `python3 -I scripts/deny_all.py --advisories baseline`. It needs Rust 1.93.0 and
   1.98.1 (for `cargo metadata`). Without `../upstream/lez-v0.3.1` and `../upstream/spel`, the five `lez-v03*`
   workspaces and `idl/spel-check` are skipped, and the six module libraries are skipped unless staged (12 = 5 + 1 +
   6); `--require-all` once CI provides them. A workspace whose dependencies cannot be fetched fails the job. A new
   advisory, a newly yanked crate, or a listed advisory reaching another workspace turns the job red until it is
   triaged and added to `scripts/deny-advisories-baseline.txt` (one line per advisory, with its workspaces).
2. **Python tests.** `python3 -I` on `scripts/reproduce/test_image_id_tools.py`, `scripts/test_fetch_native.py`,
   `scripts/test_release_manifest.py`, `scripts/test_release_scripts.py`, `scripts/test_vault_libs.py`,
   `scripts/test_deny_all.py` (standard library, Python 3.11 or later; bash, git, tar and gzip; about 20 s together).
3. **Native archives in every CI build that links them (M1).** Three builds link rust-rapidsnark and the circuits:
   checkpoint's `cargo +1.98.1 build --release --locked -p fidelity --bin ci_zone` (`e2e/ci/run.sh:48`), the CLI's
   `cargo test --locked --test roundtrip_b` (`e2e/ci/run.sh:53,57`), and `cli/tests/e2e.sh:14`
   (`cargo test --test e2e`, which also lacks `--locked`). Before each: `vars=$(python3 -I scripts/fetch_native.py env)
   && eval "$vars"` (not `eval "$(…)"`, which loses a failing exit status); build with
   `--message-format=json-render-diagnostics` and the output saved, then `python3 -I scripts/fetch_native.py
   check-build MESSAGES --target-dir <that workspace's target>`. Cache `~/.cache/ferrata/native` keyed on the hash of
   `scripts/native-deps.json`.
4. **Vault libraries (L6).** After each `forge install --no-git …` (`ci.yml:147`, `e2e/ci/run.sh:32`,
   `cli/tests/e2e.sh:11`, `cli/tests/relayer.sh:24` if it runs in CI): `python3 -I scripts/vault_libs.py check` (or
   replace the install with `python3 -I scripts/vault_libs.py install`).
5. **Image ids (M4).** In the jobs that build guests (`witness-independence`, `roundtrip`), after their builds, with
   `r0vm` 3.0.5 on PATH: `python3 -I scripts/reproduce/guest_ids.py . ids.tsv`, then
   `python3 -I scripts/reproduce/compare_ids.py --out . --guests release,checkpoint_b` (the guests the job built; the
   subset mode of `156e1450` needs no LEZ programs, crown table or other guests). The job fails on DIFFER, MISSING or
   CONFLICT. (After the move to RISC Zero 3.1, `docs/image-ids.md` changes with the ids.)
6. **MSRV (L4).** A step `cargo +1.85.0 check --workspace --locked` at the root keeps `rust-version = "1.85"` true.
7. **From the review, not done here:** L5 (pin actions by commit, `permissions: contents: read`), L6's cache key
   (key the upstream cache on `scripts/fetch-upstream.sh`'s hash: rc3 is missing from it), L9's
   `docs/guides/ci.md` line on path-dependent ids.

## Findings and commits

| Finding | Status | Commit(s) | Test or run |
|---|---|---|---|
| review-code-ops M1 release CLI unlocked, native downloads unchecked | fixed | `e709601f`, `a9a17562`, `88c25ae3` | `scripts/test_fetch_native.py` (22); box runs 4, 2b, 3 |
| review-code-ops M2 LGPL/GPL native code without notices; cargo-deny blind to it | fixed (owner to approve the written offer before publishing a release) | `bbd49dd9`, `a9a17562`, `5b2d7615`, `d05320c6`, `88c25ae3`, `5f03a030` | `scripts/test_release_manifest.py` (24); box run 3 |
| review-code-ops M4 deny.toml fails on the guest workspaces; no script over every workspace | fixed (CI job: integrator) | `bbd49dd9`, `07fd4d85`, `e9864e56` | `scripts/test_deny_all.py` (13); box runs 2a, 2b |
| review-code-ops L1, L2 image-id tooling fails open | fixed | `50f8572f` | `scripts/reproduce/test_image_id_tools.py` |
| review-code-ops L3 provenance written without being measured | fixed | `a9a17562`, `68298533`, `88c25ae3` | `scripts/test_release_scripts.py` |
| review-code-ops L4 false MSRV | fixed (1.85, measured) | `e4271790` | box runs 2–3 |
| review-code-ops L6 weak pins | fixed: Logos Blockchain full SHA, vault libraries by tree hash; the CI cache key is the integrator's | `8816471b`, `39fedec6`, `098ddb08`, `00fd2f8c` | `scripts/test_vault_libs.py` (8) |
| review-code-ops L9 stale docs | in part (`gpu-prover/README.md`); `docs/guides/ci.md` and `docs/image-ids.md` belong to other streams | `24aacc6f` | — |
| review-code-ops L11, L12 | fixed | `bbd49dd9`, `68298533` | box runs 3, 4 |
| review-fix-supply M-1 no notices for the Rust crates, AWS-LC, ring | fixed: generated per build, checked by `verify --strict` (lockfile, dependency set, binary symbols); the standard library's crates too | `88c25ae3`, `5f03a030`, `0f09c8eb` | `scripts/test_release_manifest.py`, `scripts/test_release_scripts.py`; box runs 1, 2b, 3 |
| review-fix-supply M-2 a failed build can be released | fixed | `88c25ae3` | `scripts/test_release_scripts.py` (the review's case, end to end; the old scripts fail all 7 such tests); box run 2b on a real build directory |
| review-fix-supply M-3 deny_all skips unfetchable workspaces | fixed | `07fd4d85`, `e9864e56` | `scripts/test_deny_all.py` (old script fails its 4 end-to-end tests); box run 2a (offline: 3 FAIL) |
| review-fix-supply L-1 rapidsnark's build commit | fixed (`8f51c339`) | `d05320c6` | GitHub API (tag ref, asset dates, compare) |
| review-fix-supply L-2 nlohmann/json in the circuits; ffiasm in librapidsnark.a | fixed | `d05320c6` | box run 1 (`ar t`, nlohmann symbols); Launchpad (jammy 3.10.5-2) |
| review-fix-supply L-3 record text | fixed | `d05320c6` | — |
| review-fix-supply L-4 deny.toml scope | fixed (NCSA for libfuzzer-sys; gpu-prover's own policy; clarifications by version) | `07fd4d85` | box run 1 (`cargo deny list`, `check licenses`); `scripts/test_deny_all.py` |
| review-fix-supply L-5 global advisories baseline | fixed (per workspace) | `07fd4d85` | box run 2b: 39 pass, 0 fail |
| review-fix-supply L-6 symlinked directories | fixed | `00fd2f8c` | `scripts/test_vault_libs.py` |
| review-fix-supply L-7 sources asset not measured or verified | fixed | `88c25ae3` | `scripts/test_release_manifest.py` |
| review-fix-supply L-8 CI proposals | fixed (items 1, 3, 4, 5 above; `compare_ids.py --guests`) | `156e1450`, this file | `scripts/reproduce/test_image_id_tools.py` (18) |
| review-fix-supply L-9 docs | fixed (`docs/release-engineering.md`; README sentence proposed above) | `bb429640` | — |
| review-fix-supply L-10 source offer wording | **owner decision**: the offer's text commits the project; a complete source asset (cargo vendor of `cli/Cargo.lock`, LEZ, Logos Blockchain, the native sources and workflows) is recommended before a release is published | — | — |
| review-fix-supply L-11 `CARGO_TARGET_DIR` | fixed | `88c25ae3` | `scripts/test_release_scripts.py` (`test_good_release`) |
| review-fix-supply L-12 what check-build cannot see | fixed (docstring) | `88c25ae3` | — |
| review-fix-supply nits (`commit` outside git, member modes, the licence set) | fixed | `88c25ae3` | `scripts/test_release_scripts.py` |
