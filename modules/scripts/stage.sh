#!/usr/bin/env bash
# Copy the crates each module links into the module's crate directory (ignored by git), where cargo
# and the module builder find them: the builder copies only rust-lib/.
#   ferrata_attest_module,   stage.py: the repository's crates under rust-lib/staged/ (ferrata-core,
#   ferrata_bridge_module:   lez/wire, lez/advance; ferrata-bridge-core, lez/bridge-wire, cli/wallet),
#                            LEZ's lee_core under rust-lib/upstream/, in their own layouts, manifests
#                            with their inherited fields resolved (rust-lib/staged/STAGED.json)
#   ferrata_attestor_module: checkpoint/attestor and checkpoint/crates/{checkpoint-core,attestor-core}
#                            -> rust-lib/attestor, rust-lib/crates/... (checkpoint/'s layout, so the
#                            path dependencies between them resolve unchanged)
#   ferrata_monitor_module:  monitor (src/main.rs and the `[workspace]` line left out: it is a member
#                            of the module's workspace) and checkpoint/crates/attestor-core
#                            -> rust-lib/monitor, rust-lib/checkpoint/crates/attestor-core
#   ferrata_updater_module:  stage-updater.sh (rust-lib/vendor/: lez/updater with lez/advance, the
#                            lez/ harness and the LEZ tree)
#   ferrata_relayer_module:  stage-relayer.sh (rust-lib/vendor/: cli/'s library without lez-wallet, cli/wallet,
#                            the crates they link and the LEZ tree)
# Each records what was copied in a STAGED.json, which the module's version() reports.
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
# Several crates' sources as one sha256: Cargo.toml and src/ of each, each path prefixed with its directory.
srcsum() {
  local d
  for d in "$@"; do (cd "$repo/$d" && find Cargo.toml src -type f | LC_ALL=C sort | sed "s#^#$d/#"); done \
    | (cd "$repo" && xargs sha256sum) | sha256sum | cut -d' ' -f1
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

if [ $# = 0 ]; then
  set -- ferrata_attest_module ferrata_bridge_module ferrata_attestor_module ferrata_monitor_module ferrata_updater_module \
    ferrata_relayer_module
fi
py=()
for m in "$@"; do
  case "$m" in
    ferrata_attest_module|ferrata_bridge_module) py+=("$m") ;;
    ferrata_attestor_module|ferrata_monitor_module|ferrata_updater_module|ferrata_relayer_module) ;;
    *) echo "stage.sh: no module $m" >&2; exit 2 ;;
  esac
done
if [ ${#py[@]} -gt 0 ]; then FERRATA_COMMIT="$commit" python3 -I "$here/stage.py" "${py[@]}"; fi
for m in "$@"; do
  case "$m" in
    ferrata_attestor_module) stage_attestor ;;
    ferrata_monitor_module) stage_monitor ;;
    ferrata_updater_module) FERRATA_COMMIT="$commit" "$here/stage-updater.sh" ;;
    ferrata_relayer_module) FERRATA_COMMIT="$commit" "$here/stage-relayer.sh" ;;
  esac
done
