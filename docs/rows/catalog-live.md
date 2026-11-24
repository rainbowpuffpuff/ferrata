# The module catalog, published and installed from: proposals for RFP-021 S.9, S.10 and RFP-022 U.10, S.3 (for the integrator)

Branch `catalog-live` (from `eac802f5`). The owner published the catalog once, from public `main` `98375f82`, as
[`landclimber/ferrata-modules`](https://github.com/landclimber/ferrata-modules) (24 November 2026). This branch reads
it back and installs from it:

- [`releases/RELEASE-INTEGRITY.md`](../../releases/RELEASE-INTEGRITY.md): the release-integrity manifest, each value
  the output of the template's command for it (transcript:
  [`integrity-commands.log`](../../modules/catalog/install-test-landclimber-2026-11-24/integrity-commands.log));
- [`modules/catalog/install-test-landclimber-2026-11-24/`](../../modules/catalog/install-test-landclimber-2026-11-24/README.md):
  the read-back (123 checks, 0 failed, one "unknown") and the install from the published catalog into a headless Logos
  Core with signatures required (56 of 56 steps);
- the repository's catalog template and scripts now generate what is live (`aea28fec`, `cf4a0eca`): `rebuild-index.yml`
  commits `index/index.json` to the catalog's `main`, `indexUrl` names it, lgx pinned to `4bdd9041`; the generated
  workflows equal the live catalog's byte for byte (9 of 9); `test-release-publish.sh` 85 of 85.

This stream edits neither `docs/EVIDENCE.md` nor `docs/requirements-status.md` nor `docs/traceability.md`; the
integrator applies the texts below.

**Status moves:** RFP-021 S.9 and S.10, in part → **shown**. RFP-022 U.10 and S.3 stay **in part**, advanced: the
six core modules are published and installed from, but RFP-022 makes every interface a Logos ui module (U.1) and asks
for a mini-app (U.2), so its "resulting modules" include `ferrata_bridge_ui`, which is not in the catalog. Counts:
RFP-021 45/10/1/3 → **47/8/1/3**, and in the by-section table RFP-021 Supportability 5/4/1/1 → **7/2/1/1** (of 11);
RFP-022 unchanged, 42/23/1/2.

## The new EVIDENCE rows (the next free numbers; 251 and 252 at `eac802f5`)

| N1 | **The module catalog, published by its owner (RFP-021 S.9, S.10; RFP-022 U.10, S.3)**: `landclimber/ferrata-modules`, assembled by `release-publish.sh` from public `main` `98375f82` (catalog commit `c9bdb664`; each module repository's `main` = its `v0.1.0` tag = the offline dry run's commit at the same source). `release-all.yml` (run 45438763159) called `logos-modules-release-action` `bbdc5287` (v1.8) on GitHub's runners: six immutable releases `<module>-v0.1.0`, each `.lgx` signed in the release job (`lgx sign`, the `LOGOS_SIGNING_KEY` secret) by the DID the catalog's `trustedSigners` declares, each with its sidecar (`rootHash`, `sha256`); four with `darwin-arm64`, `linux-amd64` and `linux-arm64`, `ferrata_monitor_module` without `darwin-arm64` (its macOS leg never got a GitHub-hosted runner and was cancelled after 37 minutes), `ferrata_updater_module` `linux-amd64` alone by design (its other legs fail linking the vendored x86-64 objects), so the run ended "failure" with all six published. The index: `index/index.json` on the catalog's `main`, six packages, `rootHash` and `sha256` equal to the sidecars', committed by the catalog's own `rebuild-index.yml` (run 45454593821, commit `9738dd09`), because the base catalog's rolling `index` release turned immutable at its first rebuild and can never hold an asset (`modules/catalog/RELEASE.md`, rule 2). Immutable releases on all seven repositories; on the catalog, a ruleset on all tags and one on `main` (the six module repositories have none: the catalog pins their commits by SHA); organization 2FA. Read back: 123 checks passed, 0 failed; the release-integrity manifest filled from its commands. | `releases/RELEASE-INTEGRITY.md`; `modules/catalog/install-test-landclimber-2026-11-24/read-back-checks.txt`, `integrity-commands.log` |

| N2 | **Installed from the published catalog, signatures required (RFP-021 S.9; RFP-022 U.10)**: a headless Logos Core (logosctl `66d098d`, portable, built from source with Nix) on the build box (Linux x86-64): `catalog add` the raw `logos-repo.json` URL, `refresh`, `search` (the six), `signature_policy: require`; the catalog's package refused while the keyring was empty (untrusted key); with the DID added, a copy of a published package without its signature (unsigned), one signed by another key under the same signer name (untrusted key) and one with a bit flipped (content hash mismatch) refused, nothing installed; then all six installed by name from the catalog, each installed `rootHash` equal to its release sidecar's and its `signerDid` the DID, loaded, `version` called; `lgx verify` (logos-package `4bdd9041`) of each published `.lgx` against the DID alone. 56 of 56 steps, 24 Nov 2026. Not shown: macOS, `linux-arm64`, Basecamp's package manager. | `modules/catalog/install-test-landclimber-2026-11-24/README.md` |

Row 163, its title: "(not published)" → "(published on 24 November 2026: rows N1, N2)".

## Rows

### RFP-021 S.9 — in part → **shown**

*Publish the resulting modules in a module catalog of the team's own, built from the Logos module catalog template
(logos-modules-release-base), so the modules are installable by Logos clients.* Every clause, and what it rests on:

- *Publish the resulting modules*: all six modules are published as signed, immutable releases (N1). RFP-021 asks for
  no ui module (its U.1 asks for a core module "so that different Logos ui modules can be built on it"); every module
  it names is among the six.
- *a module catalog of the team's own*: `landclimber/ferrata-modules`, in the owner's organization `landclimber`
  (2FA required, N1).
- *built from the Logos module catalog template*: the catalog's files are `logos-modules-release-base@33fb564`'s
  (`logos-repo.json` from its template, `_release-module.yml`, `release-all.yml`, and one `release-<module>.yml` per
  module as the base's `scripts/add-module.sh` writes it from `release-module.yml.template`), with the action
  pinned, signing on and least-privilege `permissions`; the README is our own (the install and trust instructions).
  `index.py validate-repo` passes on the published `logos-repo.json`. Against the base, every difference
  ([`modules/catalog/RELEASE.md`](../../modules/catalog/RELEASE.md), "The base's files"):
  - *replaced*: `rebuild-index.yml` is the catalog's own, because the base's calls the action's, whose last step
    cannot write the rolling `index` release under immutable releases; ours runs the same index build (icons under
    `index/icons/`) and commits the result to `main` (`RELEASE.md`, rule 2);
  - *left out*: `.github/workflows/unpublish.yml`, which deletes releases and their tags: the release-integrity
    rules freeze every published release (a fix ships as a new version, rule 4) and the tag ruleset forbids deleting
    tags; `.github/workflows/validate-repo.yml`, which checks a hand-edited `logos-repo.json` and `includes.json` on
    each push: ours is generated by `release-publish.sh`, includes no other catalog, and its check, `index.py
    validate-repo`, passes on the generated file (the dry run) and on the published one (the read-back); the
    maintainer's `scripts/` (`add-module.sh`, which `release-publish.sh` replaces by writing the same submodules
    and workflows, and `catalog.sh`, which dispatches the workflows, as the `gh workflow run` commands
    `release-publish.sh` prints do); and the inputs and placeholders whose outputs are there
    (`release-module.yml.template`, `submodules/.gitkeep`).
- *so the modules are installable by Logos clients*: a headless Logos Core installed, loaded and called all six from
  the published URL, signatures required (N2). On Linux x86-64 only: no macOS or `linux-arm64` client installed from
  the catalog here. Of the published packages, four carry `darwin-arm64`; the monitor module does not (its macOS leg
  got no runner) and the updater module is `linux-amd64` alone, by design (it vendors x86-64 builds). The row asks
  for installability, not for every platform; platforms are RFP-022 S.2's, unchanged here.

- Current: "Catalog laid out from the template and installed from 127.0.0.1; publishing is one owner command, every
  path tested against a stub GitHub with no network (83 of 83), its dry run reproducible; not published. The release
  key's signing ran for real on 13 packages, published as files with their signatures, not as a catalog, and Logos
  Core enforced it under `signature_policy: require` (row 241)."
- Proposed: "Published by the owner at `landclimber/ferrata-modules`, from the template (its rolling index release
  replaced by an index on `main`, which immutable releases require; `unpublish.yml`, which would delete frozen
  releases, `validate-repo.yml`, for a hand-edited file ours is not, and the maintainer's `scripts/` left out):
  six signed, immutable releases. A headless Logos
  Core installed, loaded and called all six from the published URL with signatures required, after refusing the
  catalog's package before its key was trusted and unsigned, other-signed and tampered copies. Installed on Linux
  x86-64; the monitor module's package has no `darwin-arm64` and the updater module's is `linux-amd64` alone (by
  design)."
- Evidence: `61, 98, 163, 241, N1, N2; modules/catalog/RELEASE.md; releases/RELEASE-INTEGRITY.md`.
- `traceability.md` row S.9, the evidence cell: replace from "Publishing is one owner command" to the end with
  "Published by the owner at [`landclimber/ferrata-modules`](https://github.com/landclimber/ferrata-modules) from
  `98375f82` and installed from there by a headless Logos Core with signatures required (rows N1, N2;
  `releases/RELEASE-INTEGRITY.md`, linked from `docs/` as `../releases/RELEASE-INTEGRITY.md`). Remains: nothing for
  this row."; its files cell: add `modules/catalog/install-test-landclimber-2026-11-24/`.

### RFP-021 S.10 — in part → **shown**

*The modules must be built and released to that catalog using logos-modules-release-action.*

- *built and released … using logos-modules-release-action*: `release-all.yml` → `_release-module.yml` →
  `logos-modules-release-action/.github/workflows/release.yml@bbdc5287` (v1.8) on GitHub's runners built each module
  with Nix per variant, merged, `lgx verify`ed, signed and published all six releases with their sidecars (run
  45438763159: each module's setup and release jobs succeeded). The failed jobs are variant legs the action lets fail
  alone (the updater module's `linux-arm64` and `darwin-arm64`, by design; the monitor module's `darwin-arm64`, which
  never got a runner); the releases are partial for those two, and say so in `missingVariants`.
- *to that catalog*: the releases are the catalog repository's; its index lists all six (N1).
- The index is not the action's: its own index workflow could not write under immutable releases (six runs, each
  refused at the upload), so the catalog's `rebuild-index.yml` runs the action's index build (`index.py`, the
  action's v1.3, by commit; lgx `4bdd9041`) and commits the result (run 45454593821). The clause is about the
  modules, which the action built and released.

- Current: "The release action's steps ran locally for linux-amd64; the catalog's workflows call the action pinned
  (v1.8); nothing published, the action not run on a release. Its signing (`lgx sign`, `lgx verify`) ran for real on
  13 packages with a release key, Logos Core refusing unsigned, tampered and other-signed copies (row 241)."
- Proposed: "Built, signed and released by `logos-modules-release-action` (pinned v1.8) on GitHub's runners: six
  immutable releases with sidecars at `landclimber/ferrata-modules`, four with all three variants, the monitor
  module's without `darwin-arm64` (no macOS runner picked its leg up), the updater module's `linux-amd64` alone (by
  design). The index is the catalog's own workflow's, with the action's index tool: the action's index release
  cannot be written under immutable releases."
- Evidence: `61, 163, 241, N1; releases/RELEASE-INTEGRITY.md`.
- `traceability.md` row S.10, the evidence cell: replace "Remains: the action run on a release." with "The action ran
  on the release of `landclimber/ferrata-modules` (row N1, run 45438763159): six signed, immutable releases. Remains:
  nothing for this row."; its files cell: add `releases/RELEASE-INTEGRITY.md`.

### RFP-022 U.10 — stays **in part** (advanced)

Same clause as RFP-021 S.9. Every clause is shown for the six core modules, the bridge module (U.1's core module) and
the relayer, monitor and updater modules among them (N1, N2). **Not shown:** the mini-app. RFP-022 U.1 makes "every
user and admin interface" a Logos ui module and U.2 asks for the mini-app, so `ferrata_bridge_ui` is one of "the
resulting modules", and it is not in the catalog: it was signed and published only as a file on the test network
(row 241). The template does not exclude ui modules (Logos's own catalog, `logos-co/logos-modules-release`, carries
them beside core modules). What remains: export `ferrata_bridge_ui` as a module repository (`export.sh` is written for the
core modules and untested on a `ui_qml` one), add it to the catalog with its `release-ferrata_bridge_ui.yml`, and the
owner's release of it; then this row and S.3 can move.

- Current: "Catalog laid out from the template and installed from 127.0.0.1; publishing is one owner command, every
  path tested against a stub GitHub (83 of 83); not published. The release key's signing ran for real on 13 packages,
  published as files with their signatures, not as a catalog, and Logos Core enforced it under `signature_policy:
  require` (row 241)."
- Proposed: "Published by the owner at `landclimber/ferrata-modules` from the template, and installed from it by a
  headless Logos Core with signatures required: the six core modules, the bridge module among them (as RFP-021 S.9).
  The mini-app (`ferrata_bridge_ui`, U.2's ui module) is not in the catalog: it is published, signed, only as a file
  on the test network (row 241)."
- Evidence: `61, 98, 107, 163, 241, N1, N2`.
- `traceability.md` row U.10, the evidence cell: replace from "Publishing is one owner command" to the end with
  "Published at `landclimber/ferrata-modules` and installed from there, signatures required (rows N1, N2). Remains:
  the mini-app (`ferrata_bridge_ui`) in the catalog."

### RFP-022 S.3 — stays **in part** (advanced)

*The modules must be built and released to the team's module catalog (Usability #10) using
logos-modules-release-action.* Shown for the six core modules (as RFP-021 S.10, N1); not for the mini-app, which the
action has not built or released: its signed packages come from our own release script (row 241).

- Current: "The release action's steps ran locally; the catalog's workflows call it pinned (v1.8); nothing published,
  the action not run on a release. Its signing (`lgx sign`, `lgx verify`) ran for real on 13 packages with a release
  key, Logos Core refusing unsigned, tampered and other-signed copies (row 241)."
- Proposed: "The six core modules built, signed and released to `landclimber/ferrata-modules` by
  `logos-modules-release-action` (pinned v1.8) on GitHub's runners, as RFP-021 S.10. The mini-app is not: it is not in
  the catalog, and its signed packages come from our own release script (row 241)."
- Evidence: `61, 163, 241, N1`.
- `traceability.md` row S.3, the evidence cell: "As RFP-021 S.10: the six core modules released by the action to
  `landclimber/ferrata-modules` (row N1). Remains: the mini-app, built and released by the action."

## Also to update

- `README.md`, "Specified but not built", the Logos modules item: "…, a published catalog." → "…, the mini-app in
  the published catalog."; `docs/not-built.md` (the same item, "and a published catalog (laid out, dry-run and installed
  from on 127.0.0.1 in `modules/catalog/`)") → "and the mini-app in the published catalog (`landclimber/ferrata-modules`
  holds the six core modules)".
- Lambda-prize rule 2 asks for the signer DID to be declared in the project README and the solution PR, beside the
  catalog URL and the rootHashes (rule 6): the values are in `releases/RELEASE-INTEGRITY.md`; the README and the PR
  are the owner's to update.
- RFP-022 S.2 (Linux and macOS): no change proposed. N1 is data for it: the published `darwin-arm64` variants of four
  modules were built on GitHub's macOS runners; none was installed on macOS here.
- `scripts/ci/row-tests.toml`: RFP-021 S.9 and S.10 can name `releases/RELEASE-INTEGRITY.md` and the install record as
  `record`s; their test stays `release-publish` (the publish script against a stub GitHub), as no CI job reads the
  live catalog.

## What was run, and where

All GitHub access was read-only (`gh` GETs, `gh release view`/`download`, `git ls-remote`, clones, raw files); nothing
was written to GitHub. The read-back and the manifest's commands ran on the laptop; the install ran on the Modal build
box (Linux x86-64): the Nix builds of `logosctl` and `lgx` once (1,165 s and 30 s, then saved to the box's volume),
then the install (63 s), and the committed script once more, unmodified, on a fresh container (327 s with the
store restored): 56 of 56 each time; the first install run failed (its downloads did not follow redirects; the
install record's README says how). `test-release-publish.sh`: 85 passed, 0 failed (laptop, bash 5.3).
