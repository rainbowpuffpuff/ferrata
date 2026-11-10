#!/usr/bin/env bash
# Copy crates/ferrata-core into the module's crate directory (rust-lib/ferrata-core, ignored by git),
# where cargo and the module builder find it: the builder copies only rust-lib/. Records what was
# copied in rust-lib/ferrata-core/STAGED.json, which the module's version() reports.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
mod="$repo/modules/ferrata_attest_module"
src="$repo/crates/ferrata-core"
dst="$mod/rust-lib/ferrata-core"

# The copy inherits these from rust-lib/Cargo.toml; they must equal the repository's.
fields() { sed -n '/^\[workspace.package\]/,/^\[/p' "$1" | grep -E '^(version|edition|license|rust-version|repository) = '; }
if ! diff <(fields "$repo/Cargo.toml") <(fields "$mod/rust-lib/Cargo.toml") >/dev/null; then
  echo "stage.sh: [workspace.package] of rust-lib/Cargo.toml differs from the repository's" >&2
  exit 1
fi

rm -rf "$dst"
mkdir -p "$dst"
cp -r "$src/Cargo.toml" "$src/README.md" "$src/src" "$dst/"
sum=$(cd "$src" && find Cargo.toml src -type f | LC_ALL=C sort | xargs sha256sum | sha256sum | cut -d' ' -f1)
version=$(fields "$repo/Cargo.toml" | sed -n 's/^version = "\(.*\)"/\1/p')
commit=${FERRATA_COMMIT:-$(git -C "$repo" rev-parse HEAD 2>/dev/null || echo unknown)}
printf '{"version":"%s","commit":"%s","source_sha256":"%s"}\n' "$version" "$commit" "$sum" > "$dst/STAGED.json"
echo "stage.sh: ferrata-core $version, commit $commit, sources sha256 $sum"
