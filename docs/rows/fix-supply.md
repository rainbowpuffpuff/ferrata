# Stream fix-supply: proposed changes for the integrator

Branch `fix-supply` (from `42b14ca4`). This stream fixes the release-supply-chain and image-id-tooling findings of the
ops code review of 21 November 2026 (review-code-ops, at `5cf1e8e7`: M1, M2, M4, L1, L2, L3, L4, L6, L9 in part, L11,
L12). It owns no requirement row and moves none; it asks for one new EVIDENCE row and for CI changes that only the
CI stream may make. Files it changed that other streams may also touch: `deny.toml`, the root `Cargo.toml`
(`rust-version`, `exclude`), `justfile`, `scripts/deploy-testnet.sh` (one check before the EVM step),
`docs/guides/verify.md` (one sentence on `compare`), `gpu-prover/README.md` (one dated sentence).

## Requirement rows

No status changes. Two rows rest partly on what this branch hardens, with no change of status or reason proposed:

| Row | Now | Effect of this branch |
|---|---|---|
| RFP-021 VS.3 ("image ids reproducible") | in part | `scripts/reproduce-image-ids.sh compare` fails closed now (an unreadable vault, an id `r0vm` did not compute, conflicting stale build outputs, a missing guest count as differences; it checks LEZ's commit before building). The earlier recorded comparisons (rows 82, 92 and the crown runs) were made with fresh target directories and a readable vault, so they stand. |
| RFP-021 R.12, RFP-022 R.12 | as they are | Unchanged here; the CI jobs below would add gates to the green run R.12 cites (the integrator's call, with the CI stream). |

## New EVIDENCE row (number: the next free one, 155 at `42b14ca4`)

To paste into `docs/EVIDENCE.md`'s table (same columns; links written for that file):

| 155 | The release's supply chain and licences, and fail-closed image-id tooling (review-code-ops M1, M2, M4, L1–L4, L6, L11, L12; branch `fix-supply`) | **measured**, Modal build box and laptop, 22 November 2026. The CLI's two prebuilt native archives pinned by sha256 in [`scripts/native-deps.json`](../scripts/native-deps.json) (rapidsnark-linux-x86_64-pic-v0.0.8.zip 585,355 B `59bdd709…cd831`, GitHub's digest and two downloads a day apart; logos-blockchain-circuits-v0.5.7-linux-x86_64.tar.gz 30,023,192 B `92a0c6ad…0f7f`, GitHub's digest and the circuits repository's own Nix pin), fetched and checked by [`scripts/fetch_native.py`](../scripts/fetch_native.py); `scripts/release.sh build` builds the CLI `--locked` from them and `fetch_native.py check-build` fails the build if a build script linked native code from anywhere else: a test release built, packaged and verified (`--strict`) on the build box, its six image ids equal to the crown run's table; the five crates built with the variables unset refused by `check-build`; a cache with one byte of an archive changed stops the build in 0 s. A release now carries `THIRD-PARTY-NOTICES.md`, the licence texts and the GMP sources (the binary links circom's GPL-3.0 runtime and LGPL-3.0 GMP, rapidsnark, ffiasm, circomlib, so as a combined work it is conveyed under the GPL version 3), and `release_manifest.py check` refuses incomplete notices. `cargo deny` over every workspace with [`scripts/deny_all.py`](../scripts/deny_all.py): 39 of 51 workspaces pass bans, licences and sources with advisories against the committed baseline, 12 skipped (LEZ v0.3.1 and spel sources absent; six unstaged module libraries), 0 fail; the old policy fails `bridge/guests` and `fuzz`. Tests (stdlib, laptop and box): 15 image-id tooling, 19 native fetch, 9 manifest, 8 release scripts, 6 vault libraries. | [`docs/release-engineering.md`](release-engineering.md) §3; [`releases/third-party/`](../releases/third-party/THIRD-PARTY-NOTICES.md); [`deny.toml`](../deny.toml) |

## CI changes the integrator must add (this branch does not edit `.github/workflows/` or `scripts/ci/`)

1. **`deps` job (M4).** Install cargo-deny 0.20.2 from its release asset, checked:
   `cargo-deny-0.20.2-x86_64-unknown-linux-musl.tar.gz`, sha256
   `9f12ed4c49936e09b48bf862b595cde2fe64fcbd9d74dfacac6131ca824c8d5f` (= the asset's `.sha256` and GitHub's digest);
   `scripts/fetch-upstream.sh`; then `python3 -I scripts/deny_all.py --advisories baseline`. It needs Rust 1.93.0 and
   1.98.1 (for `cargo metadata`). Without `../upstream/lez-v0.3.1` and `../upstream/spel`, the six `lez-v03*`
   workspaces and `idl/spel-check` are skipped, and the six module libraries are skipped unless staged; `--require-all`
   once CI provides them. A new advisory or newly yanked crate turns the job red until it is triaged and added to
   `scripts/deny-advisories-baseline.txt`.
2. **Python tests.** `python3 -I scripts/reproduce/test_image_id_tools.py`, `scripts/test_fetch_native.py`,
   `scripts/test_release_manifest.py`, `scripts/test_release_scripts.py`, `scripts/test_vault_libs.py` (standard
   library; the last two need bash and git; about 10 s together).
3. **Native archives in CI's CLI and sequencer builds (M1).** Before `cargo build --locked` of `cli/` and
   `lez/updater/sequencer` (`e2e/ci/run.sh`): `eval "$(python3 -I scripts/fetch_native.py env)"`, and after it, with
   `--message-format=json-render-diagnostics` output saved, `python3 -I scripts/fetch_native.py check-build MESSAGES
   --target-dir cli/target`. Cache `~/.cache/ferrata/native` keyed on the hash of `scripts/native-deps.json`.
4. **Vault libraries (L6).** After each `forge install --no-git …` (`ci.yml`, `e2e/ci/run.sh`, `cli/tests/e2e.sh`):
   `python3 -I scripts/vault_libs.py check` (or replace the install with `python3 -I scripts/vault_libs.py install`).
5. **Image ids (M4).** In `witness-independence` and `roundtrip`, after their guest builds:
   `python3 -I scripts/reproduce/guest_ids.py . ids.tsv` and a comparison with `docs/image-ids.md` (the job fails on
   DIFFER or CONFLICT).
6. **MSRV (L4).** A step `cargo +1.85.0 check --workspace --locked` at the root keeps `rust-version = "1.85"` true.
7. **From the review, not done here:** L5 (pin actions by commit, `permissions: contents: read`), L6's cache key
   (key the upstream cache on `scripts/fetch-upstream.sh`'s hash: rc3 is missing from it), L9's
   `docs/guides/ci.md` line on path-dependent ids.

## Findings and commits

| Finding | Status | Commit(s) | Test or run |
|---|---|---|---|
| M1 release CLI unlocked, native downloads unchecked | fixed | `e709601f`, `a9a17562` | `scripts/test_fetch_native.py` (19); `scripts/test_release_scripts.py` (corrupted cache stops `release.sh build`); box run 4 |
| M2 LGPL/GPL native code without notices; cargo-deny blind to it | fixed (owner to approve the written offer before publishing a release) | `bbd49dd9` (deny.toml), `a9a17562` | `scripts/test_release_manifest.py` (9); box run 3 (`cargo deny list`); box run 4 |
| M4 deny.toml fails on the guest workspaces; no script over every workspace | fixed (CI job: integrator) | `bbd49dd9` | box runs 2–4 |
| L1 stale `methods.rs`, r0vm column ignored | fixed | `50f8572f` | `scripts/reproduce/test_image_id_tools.py` |
| L2 `compare --vault` passes on an unreadable vault | fixed | `50f8572f` | same (JSON-RPC stub) |
| L3 provenance written without being measured | fixed | `a9a17562`, `68298533` | `scripts/test_release_scripts.py`, `scripts/test_release_manifest.py` |
| L4 false MSRV | fixed (1.85, measured) | `e4271790` | box runs 2–3 |
| L6 weak pins | fixed: Logos Blockchain full SHA, vault libraries by tree hash; the CI cache key is the integrator's | `8816471b`, `39fedec6` | `scripts/test_vault_libs.py`; three local installs agree |
| L9 stale docs | in part (`gpu-prover/README.md`); `docs/guides/ci.md` and `docs/image-ids.md` belong to other streams | `24aacc6f` | — |
| L11 gpu-prover not buildable in the tree | fixed | `bbd49dd9` | box run 3 (`cargo metadata` exit 0) |
| L12 justfile arguments unquoted | fixed | `68298533` | box run 4 (`just m0 'x \|\| echo INJECTED'`) |
