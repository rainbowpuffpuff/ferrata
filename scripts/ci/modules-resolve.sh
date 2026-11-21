#!/usr/bin/env bash
# Each Logos Core module's crate resolves, as the module builder sees it, with its committed Cargo.lock: the
# module's sources staged into its crate directory (modules/scripts/stage.sh), the logos-rust-sdk source at the
# revision the module's flake.lock pins linked beside it (logos-rust-sdk-src), then `cargo metadata --locked` in
# rust-lib/. A manifest change in a crate a module vendors (cli/, lez/wire, ...) that the module's staging or lock
# does not follow fails here, not first in the module builder: two new path dependencies of cli/ broke
# ferrata_relayer_module's build this way on 21 November 2026 (docs/guides/ci.md, job `modules`).
# ferrata_updater_module is left out: its staging needs the guests of lez/ built (stage-updater.sh).
#
#   scripts/ci/modules-resolve.sh [MODULE...]
#
# Needs the upstream sources (scripts/fetch-upstream.sh), Python 3, git, cargo with Rust 1.98.1 for the relayer (as
# modules/scripts/native.sh), and the network for the crates and the SDK. The CI job `modules` runs exactly this.
set -euo pipefail
repo="$(cd "$(dirname "$0")/../.." && pwd)"
up="$(cd "$repo/.." && pwd)/upstream"
if [ $# = 0 ]; then
  set -- ferrata_attest_module ferrata_bridge_module ferrata_attestor_module ferrata_monitor_module ferrata_relayer_module
fi
# The logos-rust-sdk revision the modules' builder pins: one for all of them, or the check stops.
rev=$(python3 -I - "$repo/modules" "$@" <<'PY'
import json, sys
revs = {json.load(open(f"{sys.argv[1]}/{m}/flake.lock"))["nodes"]["logos-rust-sdk"]["locked"]["rev"] for m in sys.argv[2:]}
if len(revs) != 1:
    sys.exit(f"modules-resolve.sh: the modules pin different logos-rust-sdk revisions: {sorted(revs)}")
print(revs.pop())
PY
)
sdk="$up/logos-rust-sdk-${rev:0:7}"
if [ ! -f "$sdk/Cargo.toml" ]; then
  rm -rf "$sdk"
  git init -q "$sdk"
  git -C "$sdk" fetch -q --depth 1 https://github.com/logos-co/logos-rust-sdk.git "$rev"
  git -C "$sdk" checkout -q FETCH_HEAD
fi
test "$(git -C "$sdk" rev-parse HEAD)" = "$rev" || { echo "modules-resolve.sh: $sdk is not at $rev" >&2; exit 1; }
"$repo/modules/scripts/stage.sh" "$@"
failed=0
for m in "$@"; do
  d="$repo/modules/$m"
  link="$d/logos-rust-sdk-src"
  if [ -L "$link" ] || [ ! -e "$link" ]; then
    ln -sfn "$sdk" "$link"
  else
    echo "modules-resolve.sh: $m: $link is not a link; using it as it is"
  fi
  case "$m" in ferrata_updater_module|ferrata_relayer_module) cargo=(cargo +1.98.1) ;; *) cargo=(cargo) ;; esac
  mkdir -p "$d/rust-lib/target"
  log="$d/rust-lib/target/modules-resolve.log"
  if (cd "$d/rust-lib" && "${cargo[@]}" metadata --locked --format-version 1 > /dev/null 2> "$log"); then
    echo "modules-resolve.sh: $m: resolves with its Cargo.lock"
  else
    echo "modules-resolve.sh: $m: FAILED (${cargo[*]} metadata --locked)"
    grep -v -E '^\s*(Downloaded|Downloading|Updating|Locking) ' "$log" | tail -n 15
    failed=$((failed + 1))
  fi
done
echo "modules-resolve.sh: $(( $# - failed )) of $# modules resolve (logos-rust-sdk ${rev:0:7})"
[ "$failed" = 0 ]
