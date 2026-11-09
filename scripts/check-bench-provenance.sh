#!/usr/bin/env bash
# The bench (bench/RESULTS.md) measured the core of commit f030475, under the project's former
# name (crates/plumbline-core). This checks that the core at REV (default HEAD) differs from it only
# by the rename: every file of f030475, with plumbline/Plumbline/PLUMBLINE replaced by
# ferrata/Ferrata/FERRATA, must equal the file at REV byte for byte, and no file may be added or
# removed. Covered: crates/ferrata-core (Cargo.toml, src/) and the root Cargo.toml, whose
# [workspace.package] the core inherits. Exits non-zero on any other difference.
#
# Usage: scripts/check-bench-provenance.sh [REV]
set -euo pipefail
cd "$(dirname "$0")/.."

base=f03047544d0c8e624020852dbc70616b7473c733
rev=${1:-HEAD}
rename() { sed -e 's/plumbline/ferrata/g' -e 's/Plumbline/Ferrata/g' -e 's/PLUMBLINE/FERRATA/g'; }

old_files=$(git ls-tree -r --name-only "$base" -- Cargo.toml crates/plumbline-core)
new_files=$(git ls-tree -r --name-only "$rev" -- Cargo.toml crates/ferrata-core)
echo "base $base (crates/plumbline-core), rev $(git rev-parse --short "$rev") (crates/ferrata-core)"
if [ -z "$new_files" ] || [ "$(printf '%s\n' "$old_files" | rename)" != "$new_files" ]; then
  echo "FAIL: the file lists differ (< f030475 renamed, > $rev)"
  diff <(printf '%s\n' "$old_files" | rename) <(printf '%s\n' "$new_files") || true
  exit 1
fi

fail=0 total=0 renamed=0
while IFS= read -r old; do
  new=$(printf '%s\n' "$old" | rename)
  total=$((total + 1))
  if ! cmp -s <(git cat-file blob "$base:$old" | rename) <(git cat-file blob "$rev:$new"); then
    echo "DIFFERS  $new"
    fail=1
  elif cmp -s <(git cat-file blob "$base:$old") <(git cat-file blob "$rev:$new"); then
    echo "same     $new"
  else
    echo "renamed  $new"
    renamed=$((renamed + 1))
  fi
done < <(printf '%s\n' "$old_files")

if [ "$fail" -ne 0 ]; then
  echo "FAIL: the core differs from f030475 by more than the rename"
  exit 1
fi
echo "OK: $total files; $renamed differ from f030475 only by the rename, $((total - renamed)) are byte-identical"
