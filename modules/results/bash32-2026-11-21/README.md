# The publish tests under bash 3.2.57 (build box, 21 November 2026)

macOS's `/bin/bash` is 3.2.57, and `modules/scripts/release-publish.sh` and its tests are written for it
([`../../catalog/RELEASE.md`](../../catalog/RELEASE.md); `.github/workflows/macos.yml`). The earlier record,
[`../bsd-userland-2026-11-21/`](../bsd-userland-2026-11-21/README.md), ran the suite as it was then (68 checks)
under a FreeBSD userland with bash 3.2.57. This one runs the suite of commit `811d7b3f` (83 checks, after the
second review's fixes to the script) under bash 3.2.57 itself, on the build box (Ubuntu 24.04, GNU userland):

- bash 3.2.57 built from GNU's tarball, `https://ftp.gnu.org/gnu/bash/bash-3.2.57.tar.gz` (sha256
  `3fa9daf85ebf35068f090ce51283ddeeb3c75eb5bc70b1a4a7cb05868bfe06a4`), with gcc 13.3 (`-std=gnu89`, the old code's
  warnings not made errors) and its parser regenerated with bison 3.8.2 (`make YACC="bison -y"`: the tarball's
  `y.tab.c` predates its patched `parse.y`);
- that `bash` first in `PATH`, so that the tests' own `bash` calls run 3.2 too;
- in a throwaway git checkout of what the tests read (`.gitignore`, `modules/`, `checkpoint/`, `monitor/`, without
  build outputs, committed once): the box's copy of the worktree carries no git metadata, and `--yes` publishes a
  commit.

Result ([`summary.txt`](summary.txt), [`test-release-publish-bash3.2.log`](test-release-publish-bash3.2.log)):
`bash 3.2.57(2)-release`, exit 0, **83 passed, 0 failed** in 29 s. The same suite under the box's own bash
5.2.21: 83 passed, 0 failed in 30 s; on the laptop under bash 5.3: 83 passed, 0 failed.

```sh
curl -fsSLO https://ftp.gnu.org/gnu/bash/bash-3.2.57.tar.gz && tar -xzf bash-3.2.57.tar.gz && cd bash-3.2.57
CFLAGS="-O2 -std=gnu89 -Wno-error -Wno-implicit-function-declaration -Wno-implicit-int -Wno-incompatible-pointer-types -Wno-int-conversion" \
  ./configure --without-bash-malloc --disable-nls
make YACC="bison -y"
mkdir -p <bin> && ln -s "$PWD/bash" <bin>/bash
PATH="<bin>:$PATH" TMPDIR=<scratch> <bin>/bash <checkout>/modules/scripts/test-release-publish.sh <scratch>/trp
```
