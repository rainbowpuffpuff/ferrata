# Ferrata's module catalog (layout, not published)

The proposal (§B7) releases Ferrata's Logos modules from a catalog of its own with
`logos-modules-release-action`. This directory is that catalog's content, laid out as a fork of
[`logos-co/logos-modules-release-base`](https://github.com/logos-co/logos-modules-release-base)
(commit `33fb564`), the starting point Logos documents for running a catalog:

| File | What | From the base |
|---|---|---|
| `logos-repo.json` | how clients (`lgpd`, the `package_downloader` module, the package-manager UI) identify the catalog; `indexUrl` names the rolling `index` release | the base's template, filled in |
| `.github/workflows/_release-module.yml` | calls `logos-co/logos-modules-release-action/.github/workflows/release.yml` pinned to the commit its `v1` tag names (`bbdc528…`, with the variants list and `signing_mode: inline`, the secret the workflows pass named `signing_key` from the repository's `LOGOS_SIGNING_KEY`) | the base's, with signing on, the pin, and a header; least-privilege `permissions:` per job in every file |
| `.github/workflows/release-<module>.yml` (six: `ferrata_attest_module`, `ferrata_bridge_module`, `ferrata_attestor_module`, `ferrata_monitor_module`, `ferrata_updater_module`, `ferrata_relayer_module`) | one per module: `module_path: submodules/<module>` | the base's `release-module.yml.template`, as `scripts/add-module.sh` writes it |
| `.github/workflows/release-all.yml` | releases every submodule | the base's, with least-privilege `permissions:` per job and the checkout pinned to a commit |

In the catalog's own repository these files sit at its root (GitHub runs workflows only from the
root's `.github/`, so nothing here runs from this repository). Each module is a git submodule under
`submodules/`, pointing at the module's own repository; `modules/scripts/export.sh MODULE DEST`
writes that repository from this one (the module's directory with its staged crate committed, since
the builder copies only `rust-lib/` and a flake sees only tracked files).

**Not done:** no catalog repository, module repositories, release, tag or index has been created or
published (no push, no public network). The action itself has not run. Publishing is one owner step,
prepared by [`RELEASE.md`](RELEASE.md): `modules/scripts/release-publish.sh ORG CATALOG_REPO
SIGNING_KEY_FILE --yes` generates the published catalog's `logos-repo.json` (the signer's `did:jwk` under
`trustedSigners`, the `indexUrl`), exports the module repositories, assembles the catalog, and prints the
create/push/secret/trigger commands it then runs; its offline dry run (local bare repositories, `file://`
remotes) is recorded in [`dry-run-publish/README.md`](dry-run-publish/README.md).

## Dry run: the action's steps, run locally

`modules/scripts/release-local.sh OUT_DIR` does for the `linux-amd64` variant of each module (the five, then the relayer's on its own) what `release.yml` and
`rebuild-index.yml` do, publishing nothing: export each module (`export.sh`), `nix build .#lgx-portable`,
`lgx verify`, `lgx manifest --json`, the release sidecar with the action's fields, and the catalog's
`index.json` built by `logos-modules-release-tool`'s `index.py` (tag `v1.3`, the one
`rebuild-index.yml` checks out) from the packages' URLs, served on 127.0.0.1. With one variant the
action skips `lgx merge`. Its small outputs (sidecars, `index.json`, logs; no package binaries) are in
`dry-run/` for the five modules (EVIDENCE row 98) and in `dry-run-relayer/` for `ferrata_relayer_module`, released
on its own on 13 November (`MODULES=ferrata_relayer_module DRY_RUN_DIR=...`: its index holds that one package);
the runs are described in `modules/RESULTS.md`.

## Installed from the catalog

`modules/scripts/catalog-install.sh RELEASE_DIR LOGOSCTL WORK_DIR` serves the dry run's catalog over
HTTPS on 127.0.0.1 (a throwaway certificate; `logosctl` requires https) and has a headless Logos Core
add it, search it, install the five modules by name, load them and call them; with `CATALOG_E2E=1` (and
`CATALOG_E2E_MORE="updater-e2e.sh monitor-e2e.sh e2e.sh bridge_e2e.sh"`) the installed modules then do
their jobs: the three daemons' (`attestor-e2e.sh`, `updater-e2e.sh`, `monitor-e2e.sh`), the attestation
module's log end to end and `advance` (`e2e.sh`), and the bridge module on the mini-app's devnet
(`bridge_e2e.sh`, with `RELEASE_ELF`). Its log, with those runs' check lines, is in `install-test/`
(`modules/RESULTS.md`). `install-test-relayer/` is the relayer module's: installed by name from its own dry run's
catalog, loaded, called, and its root imports on the mini-app's devnet through the installed module
(`CATALOG_E2E_SCRIPTS=relayer-e2e.sh PARTS=roots`), 10 checks passed.
