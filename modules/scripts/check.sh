#!/usr/bin/env bash
# Build ferrata_attest_module and ferrata_bridge_module the way Logos modules are built (dev and
# portable packages, what logos-modules-release-action builds), install both into a headless Logos
# Core (logosctl), load them and call each method. Every reply is compared with the same call made
# natively (the methods' crates, without Logos Core) and with what the request must produce
# (requests.py, bridge_requests.py). Then e2e.sh runs through the same daemon: a log fetched over
# JSON-RPC, attested by a transaction the module built and the LEZ harness executed.
# Needs Linux x86-64, Nix with flakes, cargo, python3 and jq.
#
#   modules/scripts/check.sh [OUT_DIR]     default modules/out (ignored by git)
#
# PORTABLE=0 skips the portable packages and the portable logosctl (built and run by default).
# E2E=0 skips e2e.sh through Logos Core.
# Writes OUT_DIR/report/: summary.md, the requests, every reply and one log per step.
set -euo pipefail

BUILDER=github:logos-co/logos-module-builder/5f74cdb3cecacde054c68d72fcc0d2cfe2195900
CLI=github:logos-co/logos-logoscore-cli/66d098d3af1367cf831b69c13a5aa8345edeb682
MODULES="ferrata_attest_module ferrata_bridge_module"
PORTABLE=${PORTABLE:-1}
E2E=${E2E:-1}

# On the build box (a container) the stack limit is "unlimited": Nix's evaluation of the module's flake
# overflowed its stack, and local builds failed below 1 GiB. With a finite 1 GiB limit both work.
if [ "$(ulimit -s)" = unlimited ]; then ulimit -s 1048576; fi

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
  python3 -I "$here/summary.py" "$rep" "$failures" || failures=$((failures + 1))
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

stage() { FERRATA_COMMIT=${FERRATA_COMMIT:-} "$here/stage.sh"; }
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
lockfiles() {
  local m
  for m in $MODULES; do
    if [ ! -f "$repo/modules/$m/rust-lib/Cargo.lock" ]; then (cd "$repo/modules/$m/rust-lib" && cargo generate-lockfile); fi
  done
}
calls_crate() { case "$1" in ferrata_attest_module) echo ferrata-attest-calls ;; *) echo ferrata-bridge-calls ;; esac; }
native_test() {
  local m
  for m in $MODULES; do (cd "$repo/modules/$m/rust-lib" && cargo test --release --locked -p "$(calls_crate "$m")"); done
}
native_calls() {
  local m name method file module
  for m in $MODULES; do
    (cd "$repo/modules/$m/rust-lib" && cargo build --release --locked -p "$(calls_crate "$m")" --example call)
  done
  "$repo/modules/ferrata_attest_module/rust-lib/target/release/examples/call" version > "$rep/replies/native/version.json"
  while IFS=$'\t' read -r name method file module; do
    if [ "$file" != - ]; then
      "$repo/modules/$module/rust-lib/target/release/examples/call" "$method" "$rep/requests/$file" < /dev/null > "$rep/replies/native/$name.json"
    else
      "$repo/modules/$module/rust-lib/target/release/examples/call" "$method" < /dev/null > "$rep/replies/native/$name.json"
    fi
  done < <(jq -r '.[] | [.name, .method, (if .request == "" then "-" else .request end), (.module // "ferrata_attest_module")] | @tsv' \
             "$rep/requests/calls.json")
}

# The flake sees only files git tracks, so each module is built from a copy in a fresh repository
# with the staged crate added (as the builder's doc-tests do).
prepare_build() {
  local m b
  for m in $MODULES; do
    b="$out/build-$m"
    rm -rf "$b"
    mkdir -p "$b"
    tar -C "$repo/modules/$m" --exclude=./out --exclude=./rust-lib/target --exclude=./logos-rust-sdk-src \
      --exclude='./result*' -cf - . | tar -C "$b" -xf -
    (
      cd "$b"
      git init -q
      git add -A
      for c in rust-lib/staged rust-lib/upstream; do if [ -d "$c" ]; then git add -f "$c"; fi; done
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
  step 1 "$flavour-daemon-start" env HOME="$out/home-$flavour" "$LOGOSCTL" --config-dir "$SESSION" daemon start --detach
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
  done < <(jq -r '[{name: "version", method: "version", request: "", module: "ferrata_attest_module"}] + . | .[]
                  | [.name, .method, (if .request == "" then "-" else .request end), (.module // "ferrata_attest_module")] | @tsv' \
             "$rep/requests/calls.json")
  if [ "$E2E" != 0 ]; then
    step 0 "$flavour-e2e" env CALL=logosctl LOGOSCTL="$LOGOSCTL" SESSION="$SESSION" "$here/e2e.sh" "$rep/e2e-$flavour" || true
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
step 1 stage stage
step 1 sdk-source sdk
step 1 cargo-lockfiles lockfiles
step 1 requests python3 -I "$here/requests.py" "$rep/requests"
step 1 bridge-requests python3 -I "$here/bridge_requests.py" "$rep/requests"
step 1 native-test native_test
step 1 native-calls native_calls
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
