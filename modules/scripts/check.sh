#!/usr/bin/env bash
# Build Ferrata's Logos modules the way Logos modules are built (dev and portable packages, what
# logos-modules-release-action builds), install them into a headless Logos Core (logosctl), load them
# and call each method. Every reply is compared with the same call made natively (the methods' crates,
# without Logos Core) and with what the request must produce (requests.py, bridge_requests.py,
# daemon_requests.py). Then, through the same daemon: e2e.sh (a log fetched over JSON-RPC, attested by
# a transaction the module built and the LEZ harness executed) and the three daemons doing their jobs
# as modules: attestor-e2e.sh (the zone of four's records), updater-e2e.sh (the in-process LEZ chain
# with the lab's recorded beacon data), monitor-e2e.sh (monitor/tests/anvil-e2e.sh: an anvil chain with a v5 vault),
# each first run natively (the calls crate, or the monitor binary) as the reference.
# Needs Linux x86-64, Nix with flakes, cargo, python3 and jq; for the daemons also the RISC Zero
# toolchain of lez/ (the guests the updater module embeds), r0vm, and Foundry (anvil, forge, cast) with
# vault/lib.
#
#   modules/scripts/check.sh [OUT_DIR]     default modules/out (ignored by git)
#
# MODULES="..." restricts the modules (default all five). PORTABLE=0 skips the portable packages and
# the portable logosctl (built and run by default). E2E=0 skips the end-to-end runs through Logos Core;
# DAEMONS_E2E_IN="dev portable" names the flavours the daemons' runs go through (default both).
# Writes OUT_DIR/report/: summary.md, the requests, every reply and one log per step.
set -euo pipefail

BUILDER=github:logos-co/logos-module-builder/5f74cdb3cecacde054c68d72fcc0d2cfe2195900
CLI=github:logos-co/logos-logoscore-cli/66d098d3af1367cf831b69c13a5aa8345edeb682
MODULES=${MODULES:-"ferrata_attest_module ferrata_bridge_module ferrata_attestor_module ferrata_monitor_module ferrata_updater_module"}
PORTABLE=${PORTABLE:-1}
E2E=${E2E:-1}
DAEMONS_E2E_IN=${DAEMONS_E2E_IN:-"dev portable"}
has() { case " $MODULES " in *" $1 "*) return 0 ;; esac; return 1; }

# On the build box (a container) the stack limit is "unlimited": Nix's evaluation of the module's flake
# overflowed its stack, and local builds failed below 1 GiB. With a finite 1 GiB limit both work.
if [ "$(ulimit -s)" = unlimited ]; then ulimit -s 1048576; fi
# Logos Core keeps each module's socket in TMPDIR (logos_<module>_<id>). A Unix socket path holds at most
# 107 bytes; with a longer one every module's host process dies at load (SIGSEGV after "failed to listen",
# met on the build box). LOGOS_TMPDIR (default TMPDIR) must leave room for the longest module name.
LOGOS_TMPDIR=${LOGOS_TMPDIR:-${TMPDIR:-/tmp}}
if [ $(( ${#LOGOS_TMPDIR} + 45 )) -gt 107 ]; then
  echo "$(basename "$0"): LOGOS_TMPDIR=$LOGOS_TMPDIR is too long for Logos Core's sockets (62 bytes at most); set a shorter one" >&2
  exit 2
fi
mkdir -p "$LOGOS_TMPDIR"
export TMPDIR="$LOGOS_TMPDIR"

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
mkdir -p "${1:-$repo/modules/out}"
out=$(cd "${1:-$repo/modules/out}" && pwd)
rep="$out/report"
rm -rf "$rep"
mkdir -p "$rep/logs" "$rep/replies/native"
steps="$rep/steps.md"
printf '| step | result | seconds |\n|---|---|---:|\n' > "$steps"
failures=0

now() { date +%s.%N; }
since() { awk -v a="$1" -v b="$(now)" 'BEGIN { printf "%.1f", b - a }'; }

# step FATAL NAME CMD...: run CMD with its output in logs/NAME.log and record its time.
step() {
  local fatal=$1 name=$2 t0
  shift 2
  t0=$(now)
  printf '%s %s\n' "$(date -u +%H:%M:%S)" "$name"
  if "$@" > "$rep/logs/$name.log" 2>&1; then
    printf '| %s | ok | %s |\n' "$name" "$(since "$t0")" >> "$steps"
  else
    local rc=$?
    printf '| %s | FAILED (exit %s) | %s |\n' "$name" "$rc" "$(since "$t0")" >> "$steps"
    tail -n 30 "$rep/logs/$name.log" | sed "s/^/  [$name] /"
    failures=$((failures + 1))
    if [ "$fatal" = 1 ]; then finish; fi
    return 1
  fi
}

ctl() { "$LOGOSCTL" --config-dir "$SESSION" "$@"; }

stop_daemon() { if [ -n "${SESSION:-}" ]; then ctl daemon stop > /dev/null 2>&1 || true; fi; }
trap stop_daemon EXIT

finish() {
  stop_daemon
  MODULES="$MODULES" E2E="$E2E" DAEMONS_E2E_IN="$DAEMONS_E2E_IN" python3 -I "$here/summary.py" "$rep" "$failures" || failures=$((failures + 1))
  exit $(( failures > 0 ? 1 : 0 ))
}

versions() {
  uname -srm
  echo "cpus $(nproc)"
  free -g | sed -n 2p
  nix --version
  cargo --version
  python3 --version
  jq --version
}

# The guests risc0-build makes for lez/methods: the updater module embeds them (stage-updater.sh).
guests() { (cd "$repo/lez" && RISC0_HOME=${RISC0_HOME:-$repo/../risc0-home-lez} cargo build --locked -p methods); }
stage() { FERRATA_COMMIT=${FERRATA_COMMIT:-} "$here/stage.sh" $MODULES; }
# nix build, retried on the two failures the Modal build box (a gVisor container, builds as root
# without a sandbox) causes: a builder whose terminal output is lost ("reading a line: Input/output
# error", "unexpected EOF reading a line"), and /homeless-shelter left behind by a build that wrote
# to $HOME (Nix then refuses to start further builds). Each attempt resumes from what was built.
# Any other failure is final.
nixb() {
  local attempt log="$out/nix-build-attempt.log"
  for attempt in 1 2 3 4 5 6 7 8 9 10 11 12; do
    if [ "$(id -u)" = 0 ] && [ -d /homeless-shelter ]; then rm -rf /homeless-shelter; fi
    if nix build "$@" > >(tee "$log") 2>&1; then return 0; fi
    sleep 1
    grep -q -E 'reading a line: Input/output error|unexpected EOF reading a line|/homeless-shelter. exists' "$log" || return 1
    echo "check.sh: nix build $* stopped on a build-box error (attempt $attempt); retrying"
  done
  return 1
}

sdk() {
  local m
  for m in $MODULES; do nixb "$BUILDER#rust-sdk-src" -o "$repo/modules/$m/logos-rust-sdk-src"; done
}
# A missing lockfile is written from the daemon's own (the versions it is tested with), else fresh.
lock_seed() { case "$1" in ferrata_attestor_module) echo checkpoint/Cargo.lock ;; ferrata_monitor_module) echo monitor/Cargo.lock ;;
  ferrata_updater_module) echo lez/Cargo.lock ;; esac; }
lockfiles() {
  local m seed
  for m in $MODULES; do
    if [ ! -f "$repo/modules/$m/rust-lib/Cargo.lock" ]; then
      seed=$(lock_seed "$m")
      if [ -n "$seed" ]; then
        cp "$repo/$seed" "$repo/modules/$m/rust-lib/Cargo.lock"
        (cd "$repo/modules/$m/rust-lib" && $(cargo_for "$m") metadata --format-version 1 > /dev/null)
      else
        (cd "$repo/modules/$m/rust-lib" && cargo generate-lockfile)
      fi
    fi
  done
}
calls_crate() { case "$1" in ferrata_attest_module) echo ferrata-attest-calls ;; ferrata_bridge_module) echo ferrata-bridge-calls ;;
  ferrata_attestor_module) echo ferrata-attestor-calls ;; ferrata_monitor_module) echo ferrata-monitor-calls ;;
  ferrata_updater_module) echo ferrata-updater-calls ;; esac; }
# The updater module's crates are LEZ's: the toolchain lez/ builds with (lez/rust-toolchain.toml).
cargo_for() { case "$1" in ferrata_updater_module) echo "cargo +1.98.1" ;; *) echo cargo ;; esac; }
native_test() {
  local m
  for m in $MODULES; do (cd "$repo/modules/$m/rust-lib" && RISC0_DEV_MODE=1 $(cargo_for "$m") test --release --locked -p "$(calls_crate "$m")"); done
}
native_calls() {
  local m name method file module
  for m in $MODULES; do
    (cd "$repo/modules/$m/rust-lib" && $(cargo_for "$m") build --release --locked -p "$(calls_crate "$m")" --example call)
  done
  if has ferrata_attest_module; then
    "$repo/modules/ferrata_attest_module/rust-lib/target/release/examples/call" version > "$rep/replies/native/version.json"
  fi
  while IFS=$'\t' read -r name method file module; do
    if [ "$file" != - ]; then
      "$repo/modules/$module/rust-lib/target/release/examples/call" "$method" "$rep/requests/$file" < /dev/null > "$rep/replies/native/$name.json"
    else
      "$repo/modules/$module/rust-lib/target/release/examples/call" "$method" < /dev/null > "$rep/replies/native/$name.json"
    fi
  done < <(jq -r --arg mods " $MODULES " '.[] | select((.module // "ferrata_attest_module") as $m | $mods | contains(" " + $m + " "))
                  | [.name, .method, (if .request == "" then "-" else .request end), (.module // "ferrata_attest_module")] | @tsv' \
             "$rep/requests/calls.json")
}
# The daemons' end-to-end runs natively: the reference the runs through Logos Core are held to.
native_daemons() {
  local rc=0
  if has ferrata_attestor_module; then "$here/attestor-e2e.sh" "$rep/daemons-native/attestor" || rc=1; fi
  if has ferrata_updater_module; then "$here/updater-e2e.sh" "$rep/daemons-native/updater" || rc=1; fi
  if has ferrata_monitor_module; then "$here/monitor-e2e.sh" "$rep/daemons-native/monitor" || rc=1; fi
  return $rc
}

# The flake sees only files git tracks, so each module is built from a copy in a fresh repository
# with the staged crate added (as the builder's doc-tests do).
prepare_build() {
  local m b
  for m in $MODULES; do
    # LogosModule.cmake reads metadata.json into a CMake string unquoted: a ';' splits it and the JSON
    # no longer parses ("logos_module: NAME is required"), after the whole Rust build.
    if grep -q ';' "$repo/modules/$m/metadata.json"; then echo "check.sh: $m/metadata.json contains ';'" >&2; return 1; fi
    b="$out/build-$m"
    rm -rf "$b"
    mkdir -p "$b"
    tar -C "$repo/modules/$m" --exclude=./out --exclude=./rust-lib/target --exclude=./logos-rust-sdk-src \
      --exclude='./result*' -cf - . | tar -C "$b" -xf -
    (
      cd "$b"
      git init -q
      git add -A
      for c in rust-lib/ferrata-core rust-lib/ferrata-bridge-core rust-lib/attestor rust-lib/crates rust-lib/monitor \
               rust-lib/checkpoint rust-lib/vendor; do if [ -d "$c" ]; then git add -f "$c"; fi; done
      git add -f rust-lib/Cargo.lock
      if [ ! -f flake.lock ]; then nix flake lock; git add flake.lock; fi
    )
  done
}
build() { local m; for m in $MODULES; do (cd "$out/build-$m" && nixb ".#$1" -o "$out/$1-$m" -L); done; }
logosctl_build() { nixb "$CLI#$1" -o "$out/$2"; }

# core FLAVOUR LOGOSCTL PACKAGE_KIND: install both modules' packages into a fresh session, load,
# call, run e2e.sh through the daemon, stop.
core() {
  local flavour=$1 kind=$3 name method file module t0 raw m
  LOGOSCTL=$2
  SESSION="$out/session-$flavour"
  rm -rf "$SESSION" "$out/home-$flavour"
  mkdir -p "$out/home-$flavour" "$rep/replies/$flavour"
  # The daemon's package downloader starts a storage node in $HOME: keep it out of the real one.
  # RISC0_DEV_MODE: the updater module's in-process chain runs as the updater's tests do.
  step 1 "$flavour-daemon-start" env HOME="$out/home-$flavour" RISC0_DEV_MODE=1 "$LOGOSCTL" --config-dir "$SESSION" daemon start --detach
  for m in $MODULES; do
    step 1 "$flavour-install-$m" ctl install "$out/$kind-$m"/*.lgx -y
    step 1 "$flavour-load-$m" ctl module load "$m"
    step 0 "$flavour-show-$m" ctl module show "$m" --json || true
  done
  printf 'name\tseconds\texit\n' > "$rep/replies/$flavour/timings.tsv"
  while IFS=$'\t' read -r name method file module; do
    raw="$rep/replies/$flavour/$name.out"
    t0=$(now)
    if [ "$file" != - ]; then
      ctl call "$module" "$method" "@$rep/requests/$file" --json < /dev/null > "$raw" 2>&1 && rc=0 || rc=$?
    else
      ctl call "$module" "$method" --json < /dev/null > "$raw" 2>&1 && rc=0 || rc=$?
    fi
    printf '%s\t%s\t%s\n' "$name" "$(since "$t0")" "$rc" >> "$rep/replies/$flavour/timings.tsv"
    # The reply is the method's string, itself JSON.
    jq -c 'if (.result | type) == "string" then (.result | fromjson? // .result) else .result end' \
      "$raw" > "$rep/replies/$flavour/$name.json" 2>/dev/null || true
  done < <(jq -r --arg mods " $MODULES " '[{name: "version", method: "version", request: "", module: "ferrata_attest_module"}] + . | .[]
                  | select((.module // "ferrata_attest_module") as $m | $mods | contains(" " + $m + " "))
                  | [.name, .method, (if .request == "" then "-" else .request end), (.module // "ferrata_attest_module")] | @tsv' \
             "$rep/requests/calls.json")
  if [ "$E2E" != 0 ] && has ferrata_attest_module && has ferrata_bridge_module; then
    step 0 "$flavour-e2e" env CALL=logosctl LOGOSCTL="$LOGOSCTL" SESSION="$SESSION" "$here/e2e.sh" "$rep/e2e-$flavour" || true
  fi
  if [ "$E2E" != 0 ] && case " $DAEMONS_E2E_IN " in *" $flavour "*) true ;; *) false ;; esac; then
    local d="$rep/daemons-$flavour"
    if has ferrata_attestor_module; then
      step 0 "$flavour-attestor-e2e" env CALL=logosctl LOGOSCTL="$LOGOSCTL" SESSION="$SESSION" "$here/attestor-e2e.sh" "$d/attestor" || true
    fi
    if has ferrata_updater_module; then
      step 0 "$flavour-updater-e2e" env CALL=logosctl LOGOSCTL="$LOGOSCTL" SESSION="$SESSION" "$here/updater-e2e.sh" "$d/updater" || true
    fi
    if has ferrata_monitor_module; then
      step 0 "$flavour-monitor-e2e" env CALL=logosctl LOGOSCTL="$LOGOSCTL" SESSION="$SESSION" "$here/monitor-e2e.sh" "$d/monitor" || true
    fi
  fi
  step 0 "$flavour-module-ls" ctl module ls --json || true
  step 0 "$flavour-daemon-stop" ctl daemon stop || true
  SESSION=
}

sizes() {
  local f m
  for f in "$out"/lgx-*/*.lgx "$out"/lgx-portable-*/*.lgx; do
    [ -e "$f" ] && printf '%s\t%s bytes\n' "${f#"$out"/}" "$(stat -L -c %s "$f")"
  done
  for m in $MODULES; do
    [ -e "$out/lgx-$m" ] && printf '%s dev package closure\t%s\n' "$m" "$(nix path-info -S "$(readlink -f "$out/lgx-$m")" | awk '{print $2}')"
  done
  [ -e "$out/logos" ] && printf 'logosctl (dev) closure\t%s\n' "$(nix path-info -S "$(readlink -f "$out/logos")" | awk '{print $2}')"
  for f in "$out"/lgx-*/*.lgx "$out"/lgx-portable-*/*.lgx; do [ -e "$f" ] && { echo "contents of ${f#"$out"/}:"; tar -tvzf "$f" 2>/dev/null || file -L "$f"; }; done
  for m in $MODULES; do
    jq -r '.nodes | to_entries[] | select(.value.locked.rev) | "\(.key)\t\(.value.locked.rev)"' "$out/build-$m/flake.lock"
    for f in "$out/lgx-$m"/*.lgx; do [ -e "$f" ] && { echo "== contract in ${f#"$out"/}:"; tar -xzOf "$f" --wildcards '*.lidl' 2>/dev/null || true; }; done
  done
  "$out/logos/bin/logosctl" --version 2>&1 | head -3 || true
}

step 1 versions versions
if has ferrata_updater_module; then step 1 guests guests; fi
step 1 stage stage
step 1 sdk-source sdk
step 1 cargo-lockfiles lockfiles
step 1 requests python3 -I "$here/requests.py" "$rep/requests"
step 1 bridge-requests python3 -I "$here/bridge_requests.py" "$rep/requests"
step 1 daemon-requests python3 -I "$here/daemon_requests.py" "$rep/requests"
step 1 native-test native_test
step 1 native-calls native_calls
if [ "$E2E" != 0 ]; then step 0 native-daemons native_daemons || true; fi
step 1 prepare-build prepare_build
cd "$repo"
step 1 build-lgx build lgx
step 1 build-logosctl logosctl_build ctl logos
core dev "$out/logos/bin/logosctl" lgx
if [ "$PORTABLE" != 0 ]; then
  if step 0 build-lgx-portable build lgx-portable && step 0 build-logosctl-portable logosctl_build ctl-bundle-dir logos-portable; then
    core portable "$out/logos-portable/bin/logosctl" lgx-portable || true
  fi
fi
step 0 sizes sizes || true
finish
