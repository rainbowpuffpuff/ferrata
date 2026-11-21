#!/usr/bin/env bash
# Build what ferrata_attest_module needs inside a headless Logos Core for the U.1 and F.9 runs (scripts/logos-run-hook.sh),
# on the Modal build box with Nix (spike/modal-build/setup/<worktree>.sh installs it); never on a laptop.
#
#   bash modules/ferrata_attest_module/scripts/logos-build.sh OUT
#
# 1. ferrata_attest_module's dev .lgx and the dev logosctl (modules/scripts/check.sh: the module's native tests and calls
#    first; no portable build, no end-to-end runs), logos-module-builder 5f74cdb as the module pins it;
# 2. Logos Core's LEZ module, lez_core, at its merged v0.4 state (logos-execution-zone-module cc73ebf) with
#    modules/patches/lez_core-cc73ebf-authorizes-public-pda.patch, from the clone beside the repository;
# 3. Logos Core's beacon_api_module (logos-evm-beacon-api-module aca138e) with builder 5f74cdb's own locks
#    (modules/scripts/graft_builder_lock.py, as verified-proxy.sh build-logos-graft does);
# 4. /nix saved to LOGOS_NIX_SNAPSHOT (default OUT/nix-store.tar.zst; none: not saved), for a box's setup script
#    (spike/modal-build/setup/<worktree>.sh) to restore in a new container: set it to the path that script names.
# A step whose output exists is not run again. OUT/steps.md lists each step, its result and seconds.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
spike=$(cd "$repo/.." && pwd)
out=${1:?usage: logos-build.sh OUT}
mkdir -p "$out/logs"
if [ "$(ulimit -s)" = unlimited ]; then ulimit -s 1048576; fi
LEZ_CORE_REV=cc73ebfe0857705cfc1c3b29743fb6c8c6aebd82
BEACON_REV=aca138e5c211a370654700f4f8f2779ea5081c86
failures=0
step() { # name command...
  local name="$1" t0 rc; shift
  t0=$(date +%s)
  printf '%s %s\n' "$(date -u +%H:%M:%S)" "$name"
  if "$@" > "$out/logs/$name.log" 2>&1; then rc=ok; else rc="FAILED (exit $?)"; failures=$((failures + 1)); fi
  printf '| %s | %s | %s |\n' "$name" "$rc" "$(( $(date +%s) - t0 ))" | tee -a "$out/steps.md"
  [ "$rc" = ok ]
}
# nix build, retried on the box's known transient failures (as verified-proxy.sh).
nixb() {
  local attempt log="$out/nix-build-attempt.log"
  for attempt in $(seq 1 20); do
    if [ "$(id -u)" = 0 ] && [ -d /homeless-shelter ]; then rm -rf /homeless-shelter; fi
    if nix build --cores "${NIX_CORES:-16}" --max-jobs 2 "$@" > >(tee "$log") 2>&1; then return 0; fi
    sleep 2
    grep -q -E 'reading a line: Input/output error|unexpected EOF reading a line|/homeless-shelter. exists|unable to download|HTTP error|timed out' "$log" || return 1
    echo "logos-build.sh: nix build $* stopped on a box or network error (attempt $attempt); retrying"
  done
  return 1
}
echo "| step | result | s |" > "$out/steps.md"; echo "|---|---|---|" >> "$out/steps.md"

# 1. The module and logosctl.
if [ ! -e "$out/check/lgx-ferrata_attest_module" ]; then
  step check env MODULES=ferrata_attest_module PORTABLE=0 E2E=0 LOGOS_TMPDIR="$spike/tmp/mrb" RISC0_DEV_MODE=1 \
    bash "$repo/modules/scripts/check.sh" "$out/check" || tail -n 30 "$out/logs/check.log"
fi

# 2. lez_core, patched.
if [ ! -e "$out/lgx-lez_core-patched" ]; then
  src="$out/src-lez_core"
  step src-lez_core bash -c "rm -rf '$src' && git clone -q '$spike/upstream/logos-execution-zone-module' '$src' \
    && git -C '$src' checkout -q $LEZ_CORE_REV && git -C '$src' apply '$repo/modules/patches/lez_core-cc73ebf-authorizes-public-pda.patch' \
    && git -C '$src' add -A && git -C '$src' diff --cached --stat" \
    && step build-lez_core nixb "git+file://$src#lgx" -o "$out/lgx-lez_core-patched" || tail -n 30 "$out/logs/build-lez_core.log"
fi

# 3. beacon_api_module with builder 5f74cdb's locks.
if [ ! -e "$out/lgx-beacon_api_module" ]; then
  src="$out/src-beacon_api_module"
  step src-beacon_api_module bash -c "rm -rf '$src' && git clone -q '$spike/upstream/logos-evm-beacon-api-module' '$src' \
    && git -C '$src' checkout -q $BEACON_REV \
    && python3 -I '$repo/modules/scripts/graft_builder_lock.py' '$src/flake.lock' '$repo/modules/ferrata_bridge_module/flake.lock' \
       > '$src/flake.lock.new' && mv '$src/flake.lock.new' '$src/flake.lock' && git -C '$src' add -A" \
    && step build-beacon_api_module nixb "git+file://$src#lgx" -o "$out/lgx-beacon_api_module" \
    || tail -n 30 "$out/logs/build-beacon_api_module.log"
fi

# 4. /nix for the next container.
snap=${LOGOS_NIX_SNAPSHOT:-$out/nix-store.tar.zst}
if [ "$snap" != none ]; then
  mkdir -p "$(dirname "$snap")"
  step save-nix bash -c "tar -C / -cf - nix | zstd -q -T0 -3 -o '$snap.part' -f && mv '$snap.part' '$snap' && ls -la '$snap'"
fi

( cd "$out" && for f in check/lgx-ferrata_attest_module/*.lgx lgx-lez_core-patched/*.lgx lgx-beacon_api_module/*.lgx; do
    [ -e "$f" ] && sha256sum "$f"; done; [ -e check/logos/bin/logosctl ] && check/logos/bin/logosctl --version 2>&1 | head -1 ) \
  > "$out/VERSIONS" 2>&1
cat "$out/VERSIONS"
echo "logos-build.sh: $failures failure(s)"
[ "$failures" = 0 ]
