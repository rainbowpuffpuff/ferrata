# The update's dry run: the mini-app and `ferrata_bridge_module` 0.1.1 added to the published catalog

`landclimber/ferrata-modules` was published on 24 November 2026 with six modules at `v0.1.0`
([`../RELEASE.md`](../RELEASE.md)). Its releases are immutable, its `main` carries the index commits of
`rebuild-index.yml` and refuses force-pushes. So the mini-app (`ferrata_bridge_ui` 0.1.0) and the bridge module's new
version (0.1.1; the module's network session came after 0.1.0) are added by fast-forwards, with
`release-publish.sh --update` ([`../RELEASE.md`](../RELEASE.md), "Changing the published catalog"). This is its dry run
of 25 November 2026, against the live repositories, from a source commit of this work before its
merge (not in the public history: the merge was squashed):

```sh
modules/scripts/release-publish.sh landclimber ferrata-modules - --update \
  --modules 'ferrata_bridge_module ferrata_bridge_ui' --dry-run <scratch>/dry-live          # exit 0, 8 s, laptop
```

It read the live repositories with git (`ls-remote` and `fetch`, read-only; no `gh`, nothing sent), started a local bare
repository from each (`ferrata_bridge_ui` does not exist on GitHub: an empty one stood for it), made the update and
pushed it into them, fast-forwards only, the catalog's bare repository refusing non-fast-forwards and deletions as the
`main` ruleset does. No key was given (`-`): the signer is the live `logos-repo.json`'s,
`did:jwk:eyJjcnYiOiJFZDI1NTE5Iiwia3R5IjoiT0tQIiwieCI6ImNLVmpDSnFiWEFqYTBtenc1VFBOODhqWUs2QXkwTmtpRWNldUduekN6bGsifQ`.

## What it read

| Repository | `main` on GitHub |
|---|---|
| `landclimber/ferrata-modules` | `9738dd099435979df64eb746905ab73344d2cfc7` (the bot's index commit, 24 Nov 18:00Z) |
| `landclimber/ferrata_bridge_module` | `1728fa13777d92de8e2fcb9dfff1eea20f558a54` (= its tag `v0.1.0`, 0.1.0) |
| `landclimber/ferrata_bridge_ui` | none (the repository does not exist) |

## What it would push

| Repository | New `main` | Tag | How |
|---|---|---|---|
| `ferrata_bridge_module` | `59b232800e619df0b6a72b44a699a85a206362c7` | `v0.1.1` (annotated, `6af16acf…`) | a fast-forward of `1728fa13`: the export's tree (0.1.1) on top of 0.1.0, export.sh's identity and the source commit's date; 14 files changed |
| `ferrata_bridge_ui` | `216cce2d266383d80ffbbd6288db0b73ef27d988` | `v0.1.0` (annotated, `82f79c35…`) | a new repository: the export's tree, 29 files (the module's directory without its recorded results) |
| `ferrata-modules` | `0ecfa84d81d6ab2b1b3d0aaf4e4d9b1a66f18d45` | — | one commit on `9738dd09`: below |

Each module commit's message names the source commit (`ferrata_bridge_ui 0.1.0, exported from the Ferrata repository
at <the source commit>`); the bridge module's `rust-lib/staged/STAGED.json` names it too. The
catalog commit changes five paths and nothing else (`git diff --stat 9738dd09 0ecfa84d`):

```
 .github/workflows/release-ferrata_bridge_ui.yml | 36 +++++++++++++++++++++++++   (the template's, as the six)
 .gitmodules                                     |  3 +++                         (submodules/ferrata_bridge_ui, its GitHub URL)
 README.md                                       |  1 +                           (the mini-app's install line)
 submodules/ferrata_bridge_module                |  2 +-                          (1728fa13 -> 59b23280)
 submodules/ferrata_bridge_ui                    |  1 +                           (216cce2d)
```

The live README is otherwise the one this repository's template generates with the live signer, and the live
workflows and `logos-repo.json` identity equal the template's: the script found nothing it would leave behind. A clone
of the dry run's catalog, its two submodules initialised one at a time as the action's checkout does
(`git submodule update --init -- submodules/<module>`, GitHub's URLs mapped to the bare repositories), checks out
`ferrata_bridge_module` 0.1.1 and `ferrata_bridge_ui` 0.1.0 (`type: ui_qml`) at those commits.

A second run from the same commit into another directory, with `SOURCE_DATE_EPOCH=315532800` inherited (ignored, as
the script says), gave the same three commits and two tags. The commits depend on the source commit and on the live
`main`s: the publication is made from the commit public `main` has once this branch is merged, and its dry run, run
then, prints its own. `--yes` then pushes exactly what that dry run printed (`test-release-publish.sh` checks the
equality), unless a `main` moved in between (a release triggers an index commit), in which case the push is refused
and a rerun makes the update again on the new `main`.

## After the push (the owner's steps the script prints)

1. Immutable releases on `landclimber/ferrata_bridge_ui` (as on the other seven repositories).
2. The two releases, one workflow each: `gh workflow run release-ferrata_bridge_module.yml --repo
   landclimber/ferrata-modules` and `gh workflow run release-ferrata_bridge_ui.yml --repo landclimber/ferrata-modules`;
   each release dispatches `rebuild-index.yml`, which commits the index (seven packages, eight versions: the six of 0.1.0,
   the bridge module's 0.1.1 beside its 0.1.0, the mini-app's 0.1.0) to `main`.
3. The release-integrity manifest's new rows, and the install test from the published catalog:
   [`../install-test-plus/`](../install-test-plus/README.md).

What the action does with the two modules, for `linux-amd64`, was run on the build box beforehand:
[`../dry-run-plus/`](../dry-run-plus/).
