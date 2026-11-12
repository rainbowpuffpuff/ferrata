#!/usr/bin/env bash
# Copy the crates each module links into the module's crate directory (ignored by git), where cargo
# and the module builder find them: the builder copies only rust-lib/.
#   ferrata_attest_module:   crates/ferrata-core               -> rust-lib/ferrata-core
#   ferrata_bridge_module:   bridge/crates/ferrata-bridge-core -> rust-lib/ferrata-bridge-core
#   ferrata_attestor_module: checkpoint/attestor and checkpoint/crates/{checkpoint-core,attestor-core}
#                            -> rust-lib/attestor, rust-lib/crates/... (checkpoint/'s layout, so the
#                            path dependencies between them resolve unchanged)
#   ferrata_monitor_module:  monitor (src/main.rs and the `[workspace]` line left out: it is a member
#                            of the module's workspace) and checkpoint/crates/attestor-core
#                            -> rust-lib/monitor, rust-lib/checkpoint/crates/attestor-core
#   ferrata_updater_module:  see stage_updater below (rust-lib/vendor/)
# Records what was copied in STAGED.json beside the (first) copy, which each module's version() reports.
#
#   modules/scripts/stage.sh [MODULE...]      (default: every module)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
commit=${FERRATA_COMMIT:-$(git -C "$repo" rev-parse --verify -q HEAD 2>/dev/null || echo unknown)}

# The copy inherits these from the module's rust-lib/Cargo.toml; they must equal its source workspace's.
fields() { sed -n '/^\[workspace.package\]/,/^\[/p' "$1" | grep -E '^(version|edition|license|rust-version|repository) = '; }
same_fields() { # MODULE SOURCE_WORKSPACE_TOML
  if ! diff <(fields "$repo/$2") <(fields "$repo/modules/$1/rust-lib/Cargo.toml") >/dev/null; then
    echo "stage.sh: [workspace.package] of $1/rust-lib/Cargo.toml differs from $2's" >&2
    exit 1
  fi
}
# A crate's sources as one sha256: Cargo.toml and src/ of one crate (as the first two modules have
# always recorded it), or of several, each path prefixed with its directory.
srcsum() {
  if [ $# = 1 ]; then
    (cd "$repo/$1" && find Cargo.toml src -type f | LC_ALL=C sort | xargs sha256sum | sha256sum | cut -d' ' -f1)
  else
    local d
    for d in "$@"; do (cd "$repo/$d" && find Cargo.toml src -type f | LC_ALL=C sort | sed "s#^#$d/#"); done \
      | (cd "$repo" && xargs sha256sum) | sha256sum | cut -d' ' -f1
  fi
}
# copy SOURCE_CRATE DEST_DIR: its Cargo.toml, README.md, build.rs and src/.
copy() {
  local src="$repo/$1" dst=$2
  rm -rf "$dst"
  mkdir -p "$dst"
  cp -r "$src/Cargo.toml" "$src/src" "$dst/"
  for f in README.md build.rs; do if [ -f "$src/$f" ]; then cp "$src/$f" "$dst/"; fi; done
}
staged() { # DEST_DIR VERSION SUM WHAT
  printf '{"version":"%s","commit":"%s","source_sha256":"%s"}\n' "$2" "$commit" "$3" > "$1/STAGED.json"
  echo "stage.sh: $4 $2, commit $commit, sources sha256 $3"
}

stage_attest() {
  local m=ferrata_attest_module
  same_fields $m Cargo.toml
  copy crates/ferrata-core "$repo/modules/$m/rust-lib/ferrata-core"
  staged "$repo/modules/$m/rust-lib/ferrata-core" "$(fields "$repo/Cargo.toml" | sed -n 's/^version = "\(.*\)"/\1/p')" \
    "$(srcsum crates/ferrata-core)" "ferrata-core into $m"
}

stage_bridge() {
  local m=ferrata_bridge_module
  same_fields $m bridge/Cargo.toml
  copy bridge/crates/ferrata-bridge-core "$repo/modules/$m/rust-lib/ferrata-bridge-core"
  staged "$repo/modules/$m/rust-lib/ferrata-bridge-core" \
    "$(sed -n 's/^version = "\(.*\)"/\1/p' "$repo/bridge/crates/ferrata-bridge-core/Cargo.toml" | head -n 1)" \
    "$(srcsum bridge/crates/ferrata-bridge-core)" "ferrata-bridge-core into $m"
}

stage_attestor() {
  local m=ferrata_attestor_module r="$repo/modules/ferrata_attestor_module/rust-lib"
  same_fields $m checkpoint/Cargo.toml
  rm -rf "$r/crates"
  copy checkpoint/attestor "$r/attestor"
  copy checkpoint/crates/checkpoint-core "$r/crates/checkpoint-core"
  copy checkpoint/crates/attestor-core "$r/crates/attestor-core"
  staged "$r/attestor" "$(sed -n 's/^version = "\(.*\)"/\1/p' "$repo/checkpoint/attestor/Cargo.toml" | head -n 1)" \
    "$(srcsum checkpoint/attestor checkpoint/crates/checkpoint-core checkpoint/crates/attestor-core)" \
    "checkpoint/attestor (ferrata-attestor) with checkpoint-core and attestor-core into $m"
}

stage_monitor() {
  local m=ferrata_monitor_module r="$repo/modules/ferrata_monitor_module/rust-lib"
  rm -rf "$r/checkpoint"
  copy monitor "$r/monitor"
  # A member of the module's workspace: without its own `[workspace]` table; its library only.
  sed -i '/^\[workspace\]$/d' "$r/monitor/Cargo.toml"
  rm -f "$r/monitor/src/main.rs"
  sed -i '/^\[\[bin\]\]$/,/^$/d' "$r/monitor/Cargo.toml"
  copy checkpoint/crates/attestor-core "$r/checkpoint/crates/attestor-core"
  staged "$r/monitor" "$(sed -n 's/^version = "\(.*\)"/\1/p' "$repo/monitor/Cargo.toml" | head -n 1)" \
    "$(srcsum monitor checkpoint/crates/attestor-core)" "monitor (ferrata-monitor) with attestor-core into $m"
}

stage_one() {
  case "$1" in
    ferrata_attest_module) stage_attest ;;
    ferrata_bridge_module) stage_bridge ;;
    ferrata_attestor_module) stage_attestor ;;
    ferrata_monitor_module) stage_monitor ;;
    ferrata_updater_module) "$here/stage-updater.sh" ;;
    *) echo "stage.sh: no module $1" >&2; exit 2 ;;
  esac
}

if [ $# = 0 ]; then
  set -- ferrata_attest_module ferrata_bridge_module ferrata_attestor_module ferrata_monitor_module ferrata_updater_module
fi
for m in "$@"; do stage_one "$m"; done
