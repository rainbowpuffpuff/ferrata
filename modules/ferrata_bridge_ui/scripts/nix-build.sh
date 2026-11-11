#!/usr/bin/env bash
# Packages the mini-app with logos-module-builder: `nix build .#lgx` (dev package) and, unless PORTABLE=0,
# `.#lgx-portable`. The flake is read as path:, so untracked files are seen. Retries only the two failures of the Modal build box that modules/scripts/check.sh
# names (a gVisor container: lost builder output, /homeless-shelter left by a root build).
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
out="${1:-$here/build}"
mkdir -p "$out"
cd "$here"
if [ "$(ulimit -s)" = unlimited ]; then ulimit -s 1048576; fi
nb() {
  local log="$out/nix-build-$1.log" attempt
  for attempt in 1 2 3 4 5; do
    if [ "$(id -u)" = 0 ] && [ -d /homeless-shelter ]; then rm -rf /homeless-shelter; fi
    if nix build "path:$here#$1" -L --print-out-paths -o "$out/result-$1" >"$log" 2>&1; then return 0; fi
    grep -q -E 'reading a line: Input/output error|unexpected EOF reading a line|/homeless-shelter. exists' "$log" || return 1
    echo "nix-build.sh: .#$1 stopped on a build-box error (attempt $attempt); retrying"
  done
  return 1
}
for target in lgx $([ "${PORTABLE:-1}" = 0 ] || echo lgx-portable); do
  t0=$(date +%s)
  if nb "$target"; then
    echo "BUILT .#$target in $(( $(date +%s) - t0 )) s: $(readlink -f "$out/result-$target")"
    find -L "$out/result-$target" -maxdepth 3 -type f -printf "%s %P\n" | sort -k2
    find -L "$out/result-$target" -maxdepth 3 -type f -name "*.lgx" -exec sha256sum {} +
  else
    echo "FAILED .#$target after $(( $(date +%s) - t0 )) s"; tail -n 25 "$out/nix-build-$target.log"; exit 1
  fi
done
