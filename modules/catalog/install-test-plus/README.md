# Installing `ferrata_bridge_module` 0.1.1 and the mini-app from the catalog

[`box-install.sh`](box-install.sh) installs the two packages the catalog update adds
([`../dry-run-update/`](../dry-run-update/README.md)) into a headless Logos Core from the catalog, as a client does:
`logosctl` of logos-logoscore-cli `66d098d` (the portable bundle, built with Nix), `signature_policy: require`, the
catalog's signer DID the only key in its keyring. It is the published catalog's install test of 24 November
([`../install-test-landclimber-2026-11-24/`](../install-test-landclimber-2026-11-24/README.md), 56 of 56 for the six)
with these changes:

- the refusals (before the key is trusted; an unsigned copy, one signed by another key under the release key's signer
  name, one with a bit of its `linux-amd64` plugin flipped) are made from `ferrata_bridge_module`'s package;
- `package show` must name each package's version among those the catalog offers (the published catalog offers
  `ferrata_bridge_module` 0.1.0 and 0.1.1), and `install <name>` must install the newest;
- a core module is loaded and its `version` call must name the release's version;
- a ui module is not loaded (a headless Logos Core has no view to show it in: Basecamp is the host of ui modules) but
  must be installed as one, in the session's `plugins/` (`package ls --type ui`).

```sh
# after the two releases (on Linux x86-64 with Nix; the DID and the rootHashes from the releases' sidecars):
modules/catalog/install-test-plus/box-install.sh landclimber ferrata-modules <DID> \
  ferrata_bridge_module-v0.1.1/ferrata_bridge_module-0.1.1.lgx/<rootHash> \
  ferrata_bridge_ui-v0.1.0/ferrata_bridge_ui-0.1.0.lgx/<rootHash>
```

`CATALOG_URL`, `PKG_BASE` and `REC` point it at another catalog and record directory (a rehearsal). The record:
`steps.tsv` (name, expected, exit code, milliseconds, pass, output), `steps/`, `log.txt`, `versions.txt`,
`daemon.log`; absolute paths shortened: the repository as `<repo>`, the directory above it as `<top>`
(`rehearse.sh`'s WORK as `<work>`).

[`basecamp-open.sh`](basecamp-open.sh) then opens the mini-app in Logos Basecamp, the client that hosts ui modules:
Basecamp `b642a9a` (the mini-app's test pin), its portable standalone launcher (`#dev-portable`: the portable build
users run, which loads only portable variants, as the catalog's packages are), the two packages installed as they are
by Basecamp's own `lgpm`, the mini-app as `--module`, the bridge module in `--modules-dir`; then the mini-app's probe
([`../../ferrata_bridge_ui/basecamp/probe.mjs`](../../ferrata_bridge_ui/basecamp/probe.mjs), headless): the view opens,
the Module Inspector lists the bridge module loaded, the view's call reaches it. The signatures are checked by Logos
Core in `box-install.sh`, not here.

```sh
# after the release, with the two .lgx downloaded from their releases:
modules/catalog/install-test-plus/basecamp-open.sh REC ferrata_bridge_ui-0.1.0.lgx ferrata_bridge_module-0.1.1.lgx
```

On a box like the build box the portable Basecamp needs the host's OpenGL, xkbcommon, fontconfig, harfbuzz, fribidi
and ALSA libraries (Ubuntu: `libgl1 libegl1 libopengl0 libglx-mesa0 libxkbcommon0 libfontconfig1 libharfbuzz0b
libfribidi0 libasound2t64`, as the earlier Basecamp boxes installed them); without them it does not start.

## The rehearsal, before the release

Both scripts have since run against the published catalog, after the release of 25 November 2026:
[`../install-test-landclimber-2026-11-25/`](../install-test-landclimber-2026-11-25/README.md) (`box-install.sh` with all
seven packages, 74 of 74; `basecamp-open.sh` on the published packages, 3 of 3). Before the release, both
scripts ran, as committed, against a catalog served on the build box: [`rehearse.sh`](rehearse.sh), 25 November 2026,
Modal build box (Linux x86-64 under gVisor, Nix 2.28.3). Every part from a source commit of this work before its merge
(263 s, the Nix store of the earlier runs restored); then `install` and `basecamp` again from the next one, whose
`box-install.sh` reads the version from the module's reply exactly (58 s): the record here is that run's. Both
commits were squashed into the merge, so the record shows the staged source commit as `<source commit>`.

```sh
FERRATA_COMMIT=<source commit> SOURCE_DATE_EPOCH=<its date> \
  modules/catalog/install-test-plus/rehearse.sh <scratch> modules/catalog/install-test-plus/rehearsal-2026-11-25
modules/catalog/install-test-plus/rehearse.sh <scratch> modules/catalog/install-test-plus/rehearsal-2026-11-25 install basecamp
```

| Part | Result |
|---|---|
| `build` | the action's steps for `linux-amd64` on both packages ([`../dry-run-plus/`](../dry-run-plus/README.md)) |
| `sign` | both signed by the action's command (`lgx sign --key release --keys-dir`) with a throwaway key made on the box, `lgx verify` passing; the bridge module's contract in both packages equal to `modules/ferrata_bridge_ui/lidl/ferrata_bridge_module.lidl`; the published `ferrata_bridge_module` 0.1.0 downloaded, its sha256 equal to its sidecar's |
| `catalog` | served over HTTPS on 127.0.0.1 (a throwaway certificate): `logos-repo.json` with the template's identity and the throwaway DID (`index.py validate-repo`: OK), `index.json` by `index.py` v1.3: `ferrata_bridge_module` 0.1.1 and 0.1.0, `ferrata_bridge_ui` 0.1.0 with its icon under `icons/` ([`rehearsal-2026-11-25/catalog/`](rehearsal-2026-11-25/catalog/)) |
| `install` | **34 of 34 steps as expected** in 47 s ([`rehearsal-2026-11-25/steps.tsv`](rehearsal-2026-11-25/steps.tsv)): both downloaded and `lgx verify`d against the DID alone; the catalog's bridge package refused before the key was trusted (`Package signed by untrusted key`); with the key, the unsigned, other-signer and tampered copies refused (`Package is unsigned`, `Package signed by untrusted key`, `Content hash mismatch`) and nothing installed; `package show` naming 0.1.1 and 0.1.0; `install ferrata_bridge_module` installing 0.1.1, `install ferrata_bridge_ui` 0.1.0, each installed `rootHash` equal to its sidecar's; the bridge module listed as `core`, loaded, its `version` reply `{"module":"ferrata_bridge_module","version":"0.1.1",...}` with the staged source commit; the mini-app listed as a `ui_qml` package installed into the session's `plugins/` |
| `basecamp` | **the probe 3 of 3** in 6 s ([`rehearsal-2026-11-25/basecamp/`](rehearsal-2026-11-25/basecamp/)): the view opened (`p01-view-opened.png`); Basecamp loaded `ferrata_bridge_module` by itself, as the mini-app's optional dependency, when it opened the view (its log: `Module loaded: ferrata_bridge_module`), and the Module Inspector listed it loaded; the view's `logos.callModule("ferrata_bridge_module", "wallet", info)` got the module's own answer, `no wallet session: open it first` (`call-info.json`). The pictures of the Module Inspector (`p02`, `p03`) show Basecamp's Settings page: the inspector's rows are not in the grabbed widget; its state is the probe's check (`events.jsonl`). |

What a rehearsal cannot show: the packages the action builds on GitHub's runners (the three variants merged), signed by
the catalog's key, served by GitHub; that is the run after the release,
[`../install-test-landclimber-2026-11-25/`](../install-test-landclimber-2026-11-25/README.md).
