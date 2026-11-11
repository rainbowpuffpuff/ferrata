#!/usr/bin/env bash
# Copy the crates each module links into the module's crate directory (ignored by git), where cargo
# and the module builder find them: the builder copies only rust-lib/.
#   ferrata_attest_module: crates/ferrata-core        -> rust-lib/ferrata-core
#   ferrata_bridge_module: bridge/crates/ferrata-bridge-core -> rust-lib/ferrata-bridge-core
# Records what was copied in STAGED.json beside the copy, which each module's version() reports.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
commit=${FERRATA_COMMIT:-$(git -C "$repo" rev-parse HEAD 2>/dev/null || echo unknown)}

# The copy inherits these from the module's rust-lib/Cargo.toml; they must equal its source workspace's.
fields() { sed -n '/^\[workspace.package\]/,/^\[/p' "$1" | grep -E '^(version|edition|license|rust-version|repository) = '; }

# stage MODULE SOURCE_CRATE SOURCE_WORKSPACE_TOML DEST_NAME VERSION
stage() {
  local mod="$repo/modules/$1" src="$repo/$2" ws="$repo/$3" dst="$repo/modules/$1/rust-lib/$4" version=$5 sum
  if ! diff <(fields "$ws") <(fields "$mod/rust-lib/Cargo.toml") >/dev/null; then
    echo "stage.sh: [workspace.package] of $1/rust-lib/Cargo.toml differs from $3's" >&2
    exit 1
  fi
  rm -rf "$dst"
  mkdir -p "$dst"
  cp -r "$src/Cargo.toml" "$src/README.md" "$src/src" "$dst/" 2>/dev/null || cp -r "$src/Cargo.toml" "$src/src" "$dst/"
  sum=$(cd "$src" && find Cargo.toml src -type f | LC_ALL=C sort | xargs sha256sum | sha256sum | cut -d' ' -f1)
  printf '{"version":"%s","commit":"%s","source_sha256":"%s"}\n' "$version" "$commit" "$sum" > "$dst/STAGED.json"
  echo "stage.sh: $4 $version into $1, commit $commit, sources sha256 $sum"
}

core_version=$(fields "$repo/Cargo.toml" | sed -n 's/^version = "\(.*\)"/\1/p')
bridge_version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$repo/bridge/crates/ferrata-bridge-core/Cargo.toml" | head -n 1)
stage ferrata_attest_module crates/ferrata-core Cargo.toml ferrata-core "$core_version"
stage ferrata_bridge_module bridge/crates/ferrata-bridge-core bridge/Cargo.toml ferrata-bridge-core "$bridge_version"
