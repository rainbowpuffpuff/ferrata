#!/usr/bin/env bash
# Write MODULE as a standalone module repository at DEST: what a catalog's submodule points to
# (logos-modules-release-base: one git submodule per module, built with `nix build .#lgx-portable`).
# The staged crate (stage.sh) is committed there, since the builder copies only rust-lib/ and a
# flake sees only tracked files.
#
#   modules/scripts/export.sh ferrata_attest_module|ferrata_bridge_module DEST
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
m=$1
dest=$2
[ -f "$repo/modules/$m/metadata.json" ] || { echo "export.sh: no module $m" >&2; exit 2; }
"$here/stage.sh" > /dev/null
rm -rf "$dest"
mkdir -p "$dest"
tar -C "$repo/modules/$m" --exclude=./out --exclude=./rust-lib/target --exclude=./logos-rust-sdk-src \
  --exclude='./result*' -cf - . | tar -C "$dest" -xf -
cd "$dest"
# In the standalone repository the staged crate is source, not a build copy.
sed -i -e '/stage.sh before a build/d' -e '\#^rust-lib/ferrata-core/$#d' -e '\#^rust-lib/ferrata-bridge-core/$#d' .gitignore
git init -q
git add -A
git -c user.name=ferrata-export -c user.email=export@localhost commit -q -m "$m $(jq -r .version metadata.json), exported from the Ferrata repository"
echo "export.sh: $m -> $dest ($(git ls-files | wc -l) files, commit $(git rev-parse --short HEAD))"
