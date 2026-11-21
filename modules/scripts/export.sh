#!/usr/bin/env bash
# Write MODULE as a standalone module repository at DEST: what a catalog's submodule points to
# (logos-modules-release-base: one git submodule per module, built with `nix build .#lgx-portable`).
# The staged crate (stage.sh) is committed there, since the builder copies only rust-lib/ and a
# flake sees only tracked files.
#
# Reproducible: the commit's author, committer and dates are fixed - the dates to this repository's
# HEAD commit, or to SOURCE_DATE_EPOCH when it is set - and the user's git configuration (templates,
# global excludes, hooks, signing) is kept out, so the same source (the same commit of this repository,
# clean, with the same upstream trees beside it for the modules that vendor them) gives the same commit
# SHA on any machine. Portable to macOS (BSD tar, sed and find; bash 3.2).
#
#   modules/scripts/export.sh MODULE DEST      (MODULE: one of the directories modules/ferrata_*_module)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
[ $# = 2 ] || { echo "usage: modules/scripts/export.sh MODULE DEST" >&2; exit 2; }
m=$1
dest=$2
[ -f "$repo/modules/$m/metadata.json" ] || { echo "export.sh: no module $m" >&2; exit 2; }
epoch=${SOURCE_DATE_EPOCH:-$(git -C "$repo" log -1 --format=%ct HEAD 2> /dev/null || echo 0)}
case "$epoch" in ''|*[!0-9]*) echo "export.sh: SOURCE_DATE_EPOCH must be a number of seconds" >&2; exit 2 ;; esac
"$here/stage.sh" "$m" > /dev/null
rm -rf "$dest"
mkdir -p "$dest"
dest=$(cd "$dest" && pwd)
# The module's directory without its build outputs, which sit at its top level: the list is made with
# find, because tar's --exclude patterns differ (BSD tar matches them at any depth, GNU tar does not).
(cd "$repo/modules/$m" \
  && find . \( -path ./out -o -path ./rust-lib/target -o -path ./logos-rust-sdk-src -o -path './result*' \) -prune \
       -o -print0 \
  | tar --null --no-recursion -T - -cf -) | tar -C "$dest" -xf -
cd "$dest"
# In the standalone repository the staged crate is source, not a build copy.
if [ -f .gitignore ]; then
  sed -e '/before a build/d' -e '\#^rust-lib/staged/$#d' -e '\#^rust-lib/upstream/$#d' \
    -e '\#^rust-lib/attestor/$#d' -e '\#^rust-lib/crates/$#d' -e '\#^rust-lib/monitor/$#d' -e '\#^rust-lib/checkpoint/$#d' \
    -e '\#^rust-lib/vendor/$#d' .gitignore > .gitignore.export
  mv .gitignore.export .gitignore
fi
git() { command git -c core.excludesFile=/dev/null -c core.hooksPath=/dev/null -c commit.gpgSign=false \
  -c core.autocrlf=false -c core.fileMode=true "$@"; }
git init -q --template=
git add -A
# The staged trees whole, whatever .gitignore files they carry (the LEZ tree's own).
for c in rust-lib/staged rust-lib/upstream rust-lib/attestor rust-lib/crates rust-lib/monitor rust-lib/checkpoint rust-lib/vendor; do
  if [ -d "$c" ]; then git add -f "$c"; fi
done
GIT_AUTHOR_NAME=ferrata-export GIT_AUTHOR_EMAIL=export@localhost GIT_AUTHOR_DATE="$epoch +0000" \
GIT_COMMITTER_NAME=ferrata-export GIT_COMMITTER_EMAIL=export@localhost GIT_COMMITTER_DATE="$epoch +0000" \
  git commit -q -m "$m $(jq -r .version metadata.json), exported from the Ferrata repository"
echo "export.sh: $m -> $dest ($(git ls-files | wc -l | tr -d ' ') files, commit $(git rev-parse --short HEAD), tree $(git rev-parse --short 'HEAD^{tree}'))"
