#!/usr/bin/env bash
# What the mini-app needs to run inside Logos Basecamp with its core modules (Linux x86-64, Nix with flakes; the Modal
# build box): modules/ferrata_bridge_ui/results-basecamp-roundtrip/README.md.
#
#   basecamp-build.sh OUT [LGX_DIR...]
#
#   1. Logos Basecamp's `logos-basecamp-dev` (Basecamp's standalone mode with its QML inspector, logos-qt-mcp, and its
#      package manager's lgpm), from source at BASECAMP_REV (default: the pin of scripts/basecamp-ui-tests.sh):
#        nix build github:logos-co/logos-basecamp/$BASECAMP_REV#dev
#   2. the mini-app's install root (what `nix run .` hands Basecamp as --module): nix build path:<this module>#install
#   3. a root for Basecamp's --modules-dir: each LGX_DIR's .lgx installed by Basecamp's own lgpm (the core modules the
#      view calls: ferrata_bridge_module's .lgx, lez_core's), as Basecamp's dev command installs a --dep-package.
#
# OUT gets: basecamp-dev, ui-install (links into the Nix store), modules-root/, VERSIONS, timings.txt.
set -euo pipefail
OUT="${1:?usage: basecamp-build.sh OUT [LGX_DIR...]}"; shift
here="$(cd "$(dirname "$0")/.." && pwd)"
BASECAMP_REV="${BASECAMP_REV:-b642a9aba732b80343a3a7b622dad8c65e534bdb}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
# Nix's evaluation and local builds need a finite stack of 1 GiB on the box (modules/scripts/check.sh).
if [ "$(ulimit -s)" = unlimited ]; then ulimit -s 1048576; fi
NB=(nix build -L --cores "${NIX_CORES:-30}" --max-jobs "${NIX_JOBS:-2}")
step() { # NAME CMD...: timed, logged to OUT/NAME.log
  local name="$1" t0 rc; shift; t0=$(date +%s)
  echo "+ $*" > "$OUT/$name.log"
  "$@" >> "$OUT/$name.log" 2>&1; rc=$?
  echo "$name exit $rc $(( $(date +%s) - t0 )) s ($(date -u +%FT%TZ)): $*" | tee -a "$OUT/timings.txt"
  return $rc
}
for attempt in 1 2 3; do  # the box's two known Nix failures (modules/scripts/check.sh) are retried
  if [ "$(id -u)" = 0 ] && [ -d /homeless-shelter ]; then rm -rf /homeless-shelter; fi
  step basecamp-dev "${NB[@]}" "github:logos-co/logos-basecamp/$BASECAMP_REV#dev" -o "$OUT/basecamp-dev" && break
  grep -q -E 'reading a line: Input/output error|unexpected EOF reading a line|/homeless-shelter. exists' "$OUT/basecamp-dev.log" || exit 1
done
step ui-install "${NB[@]}" "path:$here#install" -o "$OUT/ui-install"
# Basecamp's lgpm: the one its dev command installs --dep-package with (in the closure of logos-basecamp-dev).
LGPM=""
for p in $(nix-store -qR "$(readlink -f "$OUT/basecamp-dev")"); do [ -x "$p/bin/lgpm" ] && LGPM="$p/bin/lgpm"; done
[ -n "$LGPM" ] || { echo "no lgpm in logos-basecamp-dev's closure" >&2; exit 1; }
rm -rf "$OUT/modules-root"; mkdir -p "$OUT/modules-root/modules" "$OUT/modules-root/plugins"
for d in "$@"; do
  for f in "$d"/*.lgx; do
    step "lgpm-$(basename "$f" .lgx)" "$LGPM" --modules-dir "$OUT/modules-root/modules" --ui-plugins-dir "$OUT/modules-root/plugins" install --file "$f"
  done
done
{
  echo "logos-basecamp $BASECAMP_REV: $(readlink -f "$OUT/basecamp-dev")"
  echo "ferrata_bridge_ui install root: $(readlink -f "$OUT/ui-install")"
  echo "lgpm: $LGPM"
  for d in "$@"; do (cd "$d" && sha256sum ./*.lgx); done
  ls "$OUT/modules-root/modules" "$OUT/modules-root/plugins"
} > "$OUT/VERSIONS"
cat "$OUT/VERSIONS"
