#!/usr/bin/env bash
# On the Modal build box (needs Nix): the catalog's two packages opened in Logos Basecamp, the client that hosts ui
# modules. Basecamp `b642a9a` (the pin of modules/ferrata_bridge_ui/scripts/basecamp-ui-tests.sh), its portable
# standalone launcher `logos-basecamp-dev-portable` (`#dev-portable`: the portable build users run, which loads only
# portable package variants, as the catalog's are); each package installed as it is by Basecamp's own lgpm (the one in
# that launcher's closure) into an install root, the mini-app's as `--module`, the bridge module's as `--modules-dir`;
# then the mini-app's probe (modules/ferrata_bridge_ui/basecamp/probe.mjs, headless, `--test`): the view opens, the
# Module Inspector lists and loads ferrata_bridge_module, and the view's call reaches it (no wallet session is open,
# and the module says so). Nothing is signed or checked here beyond what lgpm does: the signatures are box-install.sh's.
#
#   basecamp-open.sh REC UI_LGX CORE_LGX      (REC: the record's directory, made anew)
# Record: REC/steps.tsv (name, exit code, seconds), REC/probe/ (the probe's events.jsonl and shots/), REC/*.log,
# REC/versions.txt; absolute paths shortened (the repository as <repo>, the directory above it as <top>).
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
top=$(cd "$repo/.." && pwd)
[ $# = 3 ] || { echo "usage: basecamp-open.sh REC UI_LGX CORE_LGX" >&2; exit 2; }
rec=$1; ui=$(cd "$(dirname "$2")" && pwd)/$(basename "$2"); core=$(cd "$(dirname "$3")" && pwd)/$(basename "$3")
BASECAMP_REV=b642a9aba732b80343a3a7b622dad8c65e534bdb
rm -rf "$rec"; mkdir -p "$rec"; rec=$(cd "$rec" && pwd)
W="$top/tmp/cp-bc"; rm -rf "$W"; mkdir -p "$W"
# Logos Core puts each module's socket in TMPDIR (logos_<module>_<id>); a Unix socket path holds at most 107 bytes, and
# with a longer one the module's process dies at load ("failed to listen"), as modules/scripts/catalog-install.sh says.
S="$top/tmp/cb-s"; rm -rf "$S"; mkdir -p "$S"
[ $(( ${#S} + 45 )) -le 107 ] || { echo "basecamp-open.sh: $S is too long for Logos Core's sockets" >&2; exit 2; }
export TMPDIR="$S" LOGOS_TMPDIR="$S"
if [ "$(ulimit -s)" = unlimited ]; then ulimit -s 1048576; fi
: > "$rec/steps.tsv"
step() { # NAME CMD...: timed, logged to REC/NAME.log
  local name=$1 t0 rc; shift; t0=$(date +%s)
  "$@" > "$rec/$name.log" 2>&1; rc=$?
  printf '%s\t%s\t%s\n' "$name" "$rc" "$(( $(date +%s) - t0 ))" >> "$rec/steps.tsv"
  echo "$([ $rc = 0 ] && echo PASS || echo FAIL) $name (exit $rc, $(( $(date +%s) - t0 )) s)"
  return $rc
}
nixb() { local a; for a in 1 2 3 4 5 6; do [ -d /homeless-shelter ] && rm -rf /homeless-shelter; nix build -L "$@" && return 0; sleep 1; done; return 1; }
step basecamp-build nixb "github:logos-co/logos-basecamp/$BASECAMP_REV#dev-portable" -o "$W/bc" || exit 1
LGPM=""
for p in $(nix-store -qR "$(readlink -f "$W/bc")"); do [ -x "$p/bin/lgpm" ] && LGPM="$p/bin/lgpm"; done
[ -n "$LGPM" ] || { echo "no lgpm in logos-basecamp-dev-portable's closure" | tee "$rec/lgpm.log"; exit 1; }
mkdir -p "$W/ui/modules" "$W/ui/plugins" "$W/core/modules" "$W/core/plugins"
step lgpm-ui "$LGPM" --modules-dir "$W/ui/modules" --ui-plugins-dir "$W/ui/plugins" install --file "$ui"
step lgpm-core "$LGPM" --modules-dir "$W/core/modules" --ui-plugins-dir "$W/core/plugins" install --file "$core"
step roots find "$W/ui" "$W/core" -maxdepth 3
{ echo "logos-basecamp $BASECAMP_REV#dev-portable: $(readlink -f "$W/bc")"; echo "lgpm: $LGPM"
  sha256sum "$ui" "$core"; uname -srm; nix --version; date -u +%Y-%m-%dT%H:%M:%SZ; } > "$rec/versions.txt" 2>&1
mkdir -p "$rec/probe"
# The probe as the mini-app's earlier runs ran it (modules/ferrata_bridge_ui/results-basecamp-roundtrip/README.md,
# "Commands, in full"), on the portable launcher and the catalog's packages.
step probe env -C "$repo/modules/ferrata_bridge_ui/basecamp" BASECAMP_OUT="$rec/probe" BASECAMP_CORE_MODULES=ferrata_bridge_module \
  QT_QPA_PLATFORM="${QT_QPA_PLATFORM:-offscreen}" timeout 900 \
  "$W/bc/bin/logos-basecamp-dev-portable" --module "$W/ui" --modules-dir "$W/core" --test probe.mjs
rc=$?
grep -rlF "$top" "$rec" 2> /dev/null | while read -r f; do sed -i -e "s#$repo#<repo>#g" -e "s#$top#<top>#g" "$f"; done
rm -rf "$W" "$S"
echo "basecamp-open.sh: probe exit $rc"
exit $rc
