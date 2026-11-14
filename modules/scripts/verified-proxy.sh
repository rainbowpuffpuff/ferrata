#!/usr/bin/env bash
# Logos Core's verified proxy beside Ferrata's modules, on Sepolia (modules/LOGOS-CORE-MODULES.md; results in
# modules/results/verified-proxy-2026-11-14/):
#
#   verified-proxy.sh build-logos OUT    Logos's own modules from their sources: verified_proxy_module (fa98479, with
#                                        libverifproxy, nimbus-eth1 v0.4.4's Nim library) and beacon_api_module
#                                        (aca138e), each with its own lock (both pin logos-module-builder a57dc7f, so the
#                                        proxy's generated client of beacon_api_module is the one that module answers);
#                                        eth_rpc_module (6965f32) as logos-io.sh builds it (builder 5f74cdb, its two
#                                        optional dependencies called by name: eth_rpc_by_name.py).
#   verified-proxy.sh build-ferrata OUT  check.sh for ferrata_attest_module and ferrata_updater_module (their dev
#                                        packages, logosctl 66d098d, every method called through Logos Core).
#   verified-proxy.sh run OUT            one headless Logos Core with all of them, on public Sepolia, read only:
#                                        session.sh (the trusted root, the proxy's sync, the reads, the comparisons).
#
# Needs what check.sh needs (Linux x86-64, Nix with flakes, cargo, the RISC Zero toolchain of lez/), network access to
# GitHub (sources) and, for `run`, to public Sepolia endpoints (reads only: nothing is signed or sent to any chain).
# LOGOS_TMPDIR: as check.sh (62 bytes at most). On the Modal build box /nix is saved to the volume after build-logos.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
cmd=${1:?build-logos, build-ferrata or run}
mkdir -p "${2:?OUT}"
out=$(cd "$2" && pwd)
BUILDER=github:logos-co/logos-module-builder/5f74cdb3cecacde054c68d72fcc0d2cfe2195900
VP_REV=fa984790c5c5d7b39f37ed42d7b66abfc9e72b67
BEACON_REV=aca138e5c211a370654700f4f8f2779ea5081c86
ETH_RPC_REV=6965f3252986eef02e9342198a0625fa725aad8d
export GIT_TERMINAL_PROMPT=0
if [ "$(ulimit -s)" = unlimited ]; then ulimit -s 1048576; fi
LOGOS_TMPDIR=${LOGOS_TMPDIR:-${TMPDIR:-/tmp}}
mkdir -p "$LOGOS_TMPDIR"
rep="$out/report"
mkdir -p "$rep/logs"
[ -f "$rep/steps.md" ] || printf '| step | result | seconds |\n|---|---|---:|\n' > "$rep/steps.md"
failures=0
step() {
  local name=$1 t0 rc
  shift
  t0=$(date +%s)
  printf '%s %s\n' "$(date -u +%H:%M:%S)" "$name"
  if "$@" > "$rep/logs/$name.log" 2>&1; then rc=ok; else rc="FAILED (exit $?)"; failures=$((failures + 1)); fi
  printf '| %s | %s | %s |\n' "$name" "$rc" "$(( $(date +%s) - t0 ))" >> "$rep/steps.md"
  [ "$rc" = ok ]
}
# nix build, one at a time (two builds at once on the Modal box race on /homeless-shelter), retried on that box's
# known failures and on a transient download error (as logos-io.sh).
nixb() {
  local attempt log="$out/nix-build-attempt.log"
  for attempt in $(seq 1 40); do
    if [ "$(id -u)" = 0 ] && [ -d /homeless-shelter ]; then rm -rf /homeless-shelter; fi
    if nix build "$@" > >(tee "$log") 2>&1; then return 0; fi
    sleep 2
    grep -q -E 'reading a line: Input/output error|unexpected EOF reading a line|/homeless-shelter. exists|unable to download|HTTP error|timed out' "$log" || return 1
    echo "verified-proxy.sh: nix build $* stopped on a box or network error (attempt $attempt); retrying"
  done
  return 1
}

case "$cmd" in
build-logos)
  cat > "$rep/logos-modules.tsv" <<M
verified_proxy_module	logos-verified-proxy-module	$VP_REV	own lock (builder a57dc7f)
beacon_api_module	logos-evm-beacon-api-module	$BEACON_REV	own lock (builder a57dc7f)
eth_rpc_module	logos-evm-eth-rpc-module	$ETH_RPC_REV	builder 5f74cdb (override), optional dependencies by name
M
  # The Nim library first, on its own (the long build: nimbus-eth1 v0.4.4 with its submodules, Nim 2.2.12 from
  # nimbus-build-system), with every core of the box.
  step build-libverifproxy nixb "github:logos-co/logos-verified-proxy-module/$VP_REV#libverifproxy" --option cores 0 \
    -o "$out/libverifproxy" || true
  step build-verified_proxy_module nixb "github:logos-co/logos-verified-proxy-module/$VP_REV#lgx" \
    -o "$out/lgx-verified_proxy_module" || true
  step build-beacon_api_module nixb "github:logos-co/logos-evm-beacon-api-module/$BEACON_REV#lgx" \
    -o "$out/lgx-beacon_api_module" || true
  ethsrc="$out/src-eth_rpc_module"
  rm -rf "$ethsrc"
  step src-eth_rpc_module bash -c "git clone -q https://github.com/logos-co/logos-evm-eth-rpc-module.git '$ethsrc' \
    && git -C '$ethsrc' checkout -q $ETH_RPC_REV && python3 -I '$here/eth_rpc_by_name.py' '$ethsrc' \
    && git -C '$ethsrc' add -A && git -C '$ethsrc' diff --cached --stat" || true
  step build-eth_rpc_module nixb "git+file://$ethsrc#lgx" --override-input logos-module-builder "$BUILDER" \
    -o "$out/lgx-eth_rpc_module" || true
  {
    for m in verified_proxy_module beacon_api_module eth_rpc_module; do
      for f in "$out/lgx-$m"/*.lgx; do [ -f "$f" ] && printf '%s  %s\n' "$(sha256sum < "$f" | cut -c1-64)" "$m/$(basename "$f")"; done
    done
    [ -f "$out/libverifproxy/lib/libverifproxy.a" ] && printf 'libverifproxy.a: %s bytes, built by %s\n' \
      "$(stat -c %s "$out/libverifproxy/lib/libverifproxy.a")" \
      "$(strings -a "$out/libverifproxy/lib/libverifproxy.a" | grep -om1 'nbs-nim-[0-9.]*' || echo '?')"
  } > "$rep/lgx-sha256.txt" 2>&1
  ;;
build-ferrata)
  for m in ferrata_attest_module ferrata_updater_module; do
    if [ -d "$repo/modules/$m/logos-rust-sdk-src" ] && [ ! -L "$repo/modules/$m/logos-rust-sdk-src" ]; then
      rm -rf "$repo/modules/$m/logos-rust-sdk-src"
    fi
  done
  # The updater's own tests (its JSON-to-SSZ converter against recorded provider answers, its configuration) and the
  # example the session's comparison runs.
  step updater-tests bash -c "cd '$repo/lez' && cargo test --locked -p ferrata-updater --lib --test config \
    && cargo build --locked -p ferrata-updater --example lc_json_compare" || true
  step check env MODULES="ferrata_attest_module ferrata_updater_module" PORTABLE=0 E2E=0 \
    LOGOS_TMPDIR="$LOGOS_TMPDIR" "$here/check.sh" "$out/check" || true
  cp "$out/check/report/summary.md" "$rep/check-summary.md" 2>/dev/null || true
  cp "$out/check/report/steps.md" "$rep/check-steps.md" 2>/dev/null || true
  ;;
run)
  step session env LOGOSCTL="$out/check/logos/bin/logosctl" OUT="$out" LOGOS_TMPDIR="$LOGOS_TMPDIR" \
    bash "$here/verified-proxy-session.sh" || true
  step lc-ssz-compare "$repo/lez/target/debug/examples/lc_json_compare" "$out/session" \
    "$repo/crates/ferrata-spectests/fixtures/sepolia-2026-11-12" || true
  step summary python3 -I "$here/verified_proxy.py" summary "$repo" "$out/session" || true
  cat "$out/session/summary.md" 2>/dev/null
  ;;
*) echo "verified-proxy.sh: $cmd: build-logos, build-ferrata or run" >&2; exit 2 ;;
esac
echo "verified-proxy.sh $cmd: $failures failed steps"
exit "$failures"
