# Ferrata's module catalog (layout, not published)

The proposal (§B7) releases Ferrata's Logos modules from a catalog of its own with
`logos-modules-release-action`. This directory is that catalog's content, laid out as a fork of
[`logos-co/logos-modules-release-base`](https://github.com/logos-co/logos-modules-release-base)
(commit `33fb564`), the starting point Logos documents for running a catalog:

| File | What | From the base |
|---|---|---|
| `logos-repo.json` | how clients (`lgpd`, the `package_downloader` module, the package-manager UI) identify the catalog; `indexUrl` names the rolling `index` release | the base's template, filled in |
| `.github/workflows/_release-module.yml` | calls `logos-co/logos-modules-release-action/.github/workflows/release.yml@v1` with the catalog's variants and signing (`signing_mode: none`) | unchanged but for a header |
| `.github/workflows/release-<module>.yml` (five: `ferrata_attest_module`, `ferrata_bridge_module`, `ferrata_attestor_module`, `ferrata_monitor_module`, `ferrata_updater_module`) | one per module: `module_path: submodules/<module>` | the base's `release-module.yml.template`, as `scripts/add-module.sh` writes it |
| `.github/workflows/release-all.yml` | releases every submodule | unchanged |

In the catalog's own repository these files sit at its root (GitHub runs workflows only from the
root's `.github/`, so nothing here runs from this repository). Each module is a git submodule under
`submodules/`, pointing at the module's own repository; `modules/scripts/export.sh MODULE DEST`
writes that repository from this one (the module's directory with its staged crate committed, since
the builder copies only `rust-lib/` and a flake sees only tracked files).

**Not done:** no catalog repository, module repositories, release, tag or index has been created or
published (no push, no public network). The action itself has not run.

## Dry run: the action's steps, run locally

`modules/scripts/release-local.sh OUT_DIR` does for the `linux-amd64` variant what `release.yml` and
`rebuild-index.yml` do, publishing nothing: export each module (`export.sh`), `nix build .#lgx-portable`,
`lgx verify`, `lgx manifest --json`, the release sidecar with the action's fields, and the catalog's
`index.json` built by `logos-modules-release-tool`'s `index.py` (tag `v1.3`, the one
`rebuild-index.yml` checks out) from the packages' URLs, served on 127.0.0.1. With one variant the
action skips `lgx merge`. Its small outputs (sidecars, `index.json`, logs; no package binaries) are in
`dry-run/`, and the run is described in `modules/RESULTS.md`.

## Installed from the catalog

`modules/scripts/catalog-install.sh RELEASE_DIR LOGOSCTL WORK_DIR` serves the dry run's catalog over
HTTPS on 127.0.0.1 (a throwaway certificate; `logosctl` requires https) and has a headless Logos Core
add it, search it, install both modules by name, load them and call them. Both installed and answered;
its log is in `install-test/` (`modules/RESULTS.md`, "Installed from the catalog").
