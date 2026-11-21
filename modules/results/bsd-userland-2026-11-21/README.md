# The macOS workflow's scripts under a BSD userland and bash 3.2 (RFP-022 Supportability #2)

21 November 2026, laptop (light: a container, no build). `.github/workflows/macos.yml` has not run on GitHub
(nothing is pushed), and no macOS machine is available here, so its scripts were run where their two macOS
hazards can be reproduced: **BSD userland** and **bash 3.2**. GitHub's macOS runners have BSD tar
(libarchive), BSD sed and find, no GNU coreutils (no `sha256sum`), and `/bin/bash` 3.2.57.

## Where

- **BSD side:** a container of Chimera Linux (`docker.io/chimeralinux/chimera`, digest
  `sha256:ef2b5165b3a43474a212caf7d3d9d1505bf9f7bc835d776d5ad687e3d12e3902`), whose userland is FreeBSD's
  (sed, find, xargs, sort, cut, wc...), `tar` being bsdtar 3.8.10 (libarchive); with `bsdtar git jq python
  perl openssl curl` added by `apk`; `sha256sum` and its siblings deleted from it, as on macOS, so the
  scripts fall back to `shasum -a 256`; and **bash 3.2.57** first on `PATH` (the binary of
  `docker.io/library/bash:3.2`, digest `sha256:b5570cc1b07ce06db45ba48b5077611385e5606116cfb2de0e7b1c3097521158`,
  musl, as Chimera), so every `#!/usr/bin/env bash` script ran under it.
- **GNU side:** the laptop (Fedora: GNU tar 1.35, GNU sed 4.10, bfs 4.1.1 as `find`, bash 5.3.15).
- **Input:** one snapshot of this branch's tree (tracked files and the new ones), made a git repository with
  one fixed-date commit, the same for both sides, with LEZ v0.4.0-rc1 (`1806e87`) beside it. Script:
  [`bsd-run.sh`](bsd-run.sh); its whole output: [`run.log`](run.log).

## Results

| What | BSD userland, bash 3.2 | GNU, bash 5.3 |
|---|---|---|
| `bash -n` of the eight scripts (fetch-vectors, fetch-upstream, export, stage, stage-relayer, stage-updater, release-publish, test-release-publish) | 8 of 8 parse | — |
| `modules/scripts/test-release-publish.sh` (the publish step against a stub `gh`, every path) | **68 passed, 0 failed** | 68 passed, 0 failed |
| `export.sh ferrata_attest_module` | tree `132c9aaa…`, commit `fd9bc9b6…`, 66 files | the same tree and commit |
| `export.sh ferrata_bridge_module` | tree `c847a60f…`, commit `bf984c73…`, 79 files | the same |
| `export.sh ferrata_attestor_module` | tree `1645e21e…`, commit `bd3e1be1…`, 36 files | the same |
| `export.sh ferrata_monitor_module` | tree `f82a2bc5…`, commit `56d92684…`, 25 files | the same |
| `export.sh ferrata_relayer_module` (the LEZ tree vendored) | tree `bcd729a8…`, commit `20a5e5bf…`, 770 files | the same |
| `fetch-vectors.sh`'s extraction (list the files, extract by name) on a tarball shaped like `minimal.tar.gz` (5 forks × 3 light-client suites × 2 cases, and decoys) | 40 files | 40 files, the same list as the old `--wildcards` form |

The five exports are **byte-identical across the two userlands** (same git trees, and the same commits:
`export.sh` fixes identity and dates), so the staged sources, the `.gitignore` edit, the exclusions and
every `STAGED.json` checksum (`shasum -a 256` on one side, `sha256sum` on the other) agree. Measured, both
sides; full ids in [`run.log`](run.log).

What this found on the way: the first version of the new vector extraction listed directories too, and GNU
tar then reports the files under a listed directory as "Not found in archive" (exit 2); it lists files
only now. The same comparison against the scripts as they were before (`git show 292799f0:…`) is what
`scripts/ci/check-portable.py` reports on them: `tar --wildcards`, `sed -i` (3), `find -printf`,
`sha256sum` (6), `declare -A`.

## What it does not show

- macOS itself: Nix on macOS, the `.lgx` builds (`nix build .#lgx-portable`), Apple's toolchain and
  `lgx verify` there. The first run of `macos.yml` is the owner's push.
- `fetch-vectors.sh` on the real consensus-spec archive under bsdtar (the synthetic one above has its shape):
  the build box run is recorded in [`../../../scripts/ci/README.md`](../../../scripts/ci/README.md), "check-portable.py".
- `ferrata_updater_module`, which `macos.yml` leaves out (linux-x86_64 circuits and rapidsnark).
