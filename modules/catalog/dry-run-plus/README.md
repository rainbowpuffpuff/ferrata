# The action's steps for `ferrata_bridge_module` 0.1.1 and the mini-app, run on the build box

Can `logos-modules-release-action` at the catalog's pin (`bbdc5287`, v1.8) build the mini-app, a QML-only ui module?
Its `release.yml` reads only `name` and `version` from the module's `metadata.json`, builds each variant on its runner
with `nix build .#lgx-portable` in the module's checkout (`.#packages.x86_64-windows.lgx-portable` for Windows, not
used here), merges, runs `lgx verify`, signs (`lgx sign --key release --keys-dir DIR`, then `lgx verify`), checks that
the package's manifest names the same module and version, and writes the sidecar; nothing depends on the module's
type. logos-module-builder's `mkLogosQmlModule` (`5f74cdb3`, the mini-app's pin) gives a QML-only module a
`packages.<system>.lgx-portable`, as `mkLogosModule` gives a core one. So the answer turns on whether that build
works for this module, which is what this run shows for `linux-amd64`.

[`release-local.sh`](../../scripts/release-local.sh) did, for the `linux-amd64` variant and publishing nothing, what
`release.yml` and the catalog's `rebuild-index.yml` do (each module exported as its own repository by `export.sh`,
`nix build .#lgx-portable` in it, `lgx verify`, `lgx manifest --json` and its name and version checked, the sidecar
with the action's fields, the index by `index.py` of logos-modules-release-tool `94462e66` (v1.3)), here for the two
packages the catalog update adds, on 25 November 2026 on the Modal build box (Linux x86-64, Nix 2.28.3), from a source
commit of this work before its merge (`FERRATA_COMMIT` and `SOURCE_DATE_EPOCH` given, the box's copy having no git
metadata), through [`../install-test-plus/rehearse.sh`](../install-test-plus/rehearse.sh)'s `build` part:

```sh
MODULES='ferrata_bridge_module ferrata_bridge_ui' DRY_RUN_DIR=modules/catalog/dry-run-plus modules/scripts/release-local.sh OUT
```

| Package | Exported repository | `nix build .#lgx-portable` | `lgx verify` | Manifest | rootHash | Size |
|---|---|---|---|---|---|---|
| `ferrata_bridge_module` 0.1.1 | 86 files, tree `11e5499` | 173 s (its first build on the box: 511 s) | valid (unsigned) | `ferrata_bridge_module` 0.1.1, `core` | `c20b41d2…` | 12,348,286 B |
| `ferrata_bridge_ui` 0.1.0 | 29 files, tree `44798d1` | 5 s | valid (unsigned) | `ferrata_bridge_ui` 0.1.0, `ui_qml` | `0dbd91ca…` | 51,201 B |

The mini-app's package holds its view (`variants/linux-amd64/qml/`: `Main.qml`, `qmldir`, `js/`), its icon, its
`metadata.json`, and the contract of the module it calls (`assets/lidl/ferrata_bridge_module.lidl`); the bridge
module's holds its plugin and the libraries it links, and the same contract. `index.py build` listed both
([`index.json`](index.json), the sidecars beside it), and `index.py validate-repo` passed on the catalog's
`logos-repo.json` ([`log/validate-repo.txt`](log/validate-repo.txt)). Logs: [`log/`](log/).

The rest of the action's release job, on the same packages (`rehearse.sh`'s `sign` part, its record in
[`../install-test-plus/rehearsal-2026-11-25/`](../install-test-plus/rehearsal-2026-11-25/)): both signed by the action's
own command with a throwaway key and verified; the bridge module's contract in its package equal to the mini-app's
copy (`modules/ferrata_bridge_ui/lidl/`) and to the one in the mini-app's package; the index over both and the
published `ferrata_bridge_module` 0.1.0 listing the bridge module's two versions and the mini-app with its icon.

The mini-app's package has the same `rootHash` as in the first box run (from an earlier commit of this work, whose
tree differs from this one in the mini-app's `README.md` only, which the package does not carry). The bridge module's
differs between runs: its staged `STAGED.json` names the source commit, and `version()` reports it.

**Not run here:** `darwin-arm64` (a `macos-latest` runner) and `linux-arm64` (`ubuntu-24.04-arm`), and the merge of
several variants (`lgx merge`, which the action runs when more than one leg builds). In the published release of 24
November four of the six core modules built on both; the mini-app's legs have never run. A leg that fails fails alone, and the release
then carries the variants that built (`missingVariants` in its sidecar).
