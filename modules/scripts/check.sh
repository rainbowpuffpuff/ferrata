#!/usr/bin/env bash
# Build ferrata_attest_module the way Logos modules are built, install it into a headless Logos Core
# (logosctl), load it and call each method. Every reply is compared with the same call made natively
# (the methods' crate, without Logos Core) and with what the request must produce (requests.py).
# Needs Linux x86-64, Nix with flakes, cargo, python3 and jq.
#
#   modules/scripts/check.sh [OUT_DIR]     default modules/ferrata_attest_module/out (ignored by git)
#
# PORTABLE=0 skips the portable package and the portable logosctl (built and run by default).
# Writes OUT_DIR/report/: summary.md, the requests, every reply and one log per step.
set -euo pipefail

BUILDER=github:logos-co/logos-module-builder/5f74cdb3cecacde054c68d72fcc0d2cfe2195900
CLI=github:logos-co/logos-logoscore-cli/66d098d3af1367cf831b69c13a5aa8345edeb682
MODULE=ferrata_attest_module
PORTABLE=${PORTABLE:-1}

# On the build box (a container) the stack limit is "unlimited", and Nix's evaluation of the module's
# flake overflowed its stack; with a finite limit of 1 GiB it does not.
if [ "$(ulimit -s)" = unlimited ]; then ulimit -s 1048576; fi

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
mod="$repo/modules/$MODULE"
mkdir -p "${1:-$mod/out}"
out=$(cd "${1:-$mod/out}" && pwd)
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
sdk() { nix build "$BUILDER#rust-sdk-src" -o "$mod/logos-rust-sdk-src"; }
lockfile() { (cd "$mod/rust-lib" && cargo generate-lockfile); }
native_test() { (cd "$mod/rust-lib" && cargo test --release --locked -p ferrata-attest-calls); }
native_calls() {
  (cd "$mod/rust-lib" && cargo build --release --locked -p ferrata-attest-calls --example call)
  local bin="$mod/rust-lib/target/release/examples/call" name method file
  "$bin" version > "$rep/replies/native/version.json"
  while IFS=$'\t' read -r name method file; do
    "$bin" "$method" "$rep/requests/$file" < /dev/null > "$rep/replies/native/$name.json"
  done < <(jq -r '.[] | [.name, .method, .request] | @tsv' "$rep/requests/calls.json")
}

# The flake sees only files git tracks, so the module is built from a copy in a fresh repository
# with the staged ferrata-core added (as the builder's doc-tests do).
prepare_build() {
  rm -rf "$out/build"
  mkdir -p "$out/build"
  tar -C "$mod" --exclude=./out --exclude=./rust-lib/target --exclude=./logos-rust-sdk-src \
    --exclude='./result*' -cf - . | tar -C "$out/build" -xf -
  cd "$out/build"
  git init -q
  git add -A
  git add -f rust-lib/ferrata-core rust-lib/Cargo.lock
  if [ ! -f flake.lock ]; then nix flake lock; git add flake.lock; fi
}
build() { (cd "$out/build" && nix build ".#$1" -o "$out/$1" -L); }
logosctl_build() { nix build "$CLI#$1" -o "$out/$2"; }

# core FLAVOUR LOGOSCTL PACKAGE_DIR: install the package into a fresh session, load, call, stop.
core() {
  local flavour=$1 name method file t0 raw
  LOGOSCTL=$2
  SESSION="$out/session-$flavour"
  rm -rf "$SESSION" "$out/home-$flavour"
  mkdir -p "$out/home-$flavour" "$rep/replies/$flavour"
  # The daemon's package downloader starts a storage node in $HOME: keep it out of the real one.
  step 1 "$flavour-daemon-start" env HOME="$out/home-$flavour" "$LOGOSCTL" --config-dir "$SESSION" daemon start --detach
  step 1 "$flavour-install" ctl install "$3"/*.lgx -y
  step 1 "$flavour-load" ctl module load "$MODULE"
  step 0 "$flavour-show" ctl module show "$MODULE" --json || true
  printf 'name\tseconds\texit\n' > "$rep/replies/$flavour/timings.tsv"
  while IFS=$'\t' read -r name method file; do
    raw="$rep/replies/$flavour/$name.out"
    t0=$(now)
    if [ "$name" = version ]; then
      ctl call "$MODULE" version --json < /dev/null > "$raw" 2>&1 && rc=0 || rc=$?
    else
      ctl call "$MODULE" "$method" "@$rep/requests/$file" --json < /dev/null > "$raw" 2>&1 && rc=0 || rc=$?
    fi
    printf '%s\t%s\t%s\n' "$name" "$(since "$t0")" "$rc" >> "$rep/replies/$flavour/timings.tsv"
    # The reply is the method's string, itself JSON.
    jq -c 'if (.result | type) == "string" then (.result | fromjson? // .result) else .result end' \
      "$raw" > "$rep/replies/$flavour/$name.json" 2>/dev/null || true
  done < <(jq -r '[{name: "version", method: "version", request: ""}] + . | .[] | [.name, .method, .request] | @tsv' \
             "$rep/requests/calls.json")
  step 0 "$flavour-module-ls" ctl module ls --json || true
  step 0 "$flavour-daemon-stop" ctl daemon stop || true
  SESSION=
}

sizes() {
  local f
  for f in "$out"/lgx/*.lgx "$out"/lgx-portable/*.lgx; do
    [ -e "$f" ] && printf '%s\t%s bytes\n' "${f#"$out"/}" "$(stat -L -c %s "$f")"
  done
  [ -e "$out/lgx" ] && printf 'dev package closure\t%s\n' "$(nix path-info -S "$(readlink -f "$out/lgx")" | awk '{print $2}')"
  [ -e "$out/logos" ] && printf 'logosctl (dev) closure\t%s\n' "$(nix path-info -S "$(readlink -f "$out/logos")" | awk '{print $2}')"
  for f in "$out"/lgx/*.lgx; do [ -e "$f" ] && { echo "contents of ${f#"$out"/}:"; tar -tvzf "$f" 2>/dev/null || file -L "$f"; }; done
  jq -r '.nodes | to_entries[] | select(.value.locked.rev) | "\(.key)\t\(.value.locked.rev)"' "$out/build/flake.lock"
  "$out/logos/bin/logosctl" --version 2>&1 | head -3 || true
}

step 1 versions versions
step 1 stage stage
step 1 sdk-source sdk
if [ ! -f "$mod/rust-lib/Cargo.lock" ]; then step 1 cargo-lockfile lockfile; fi
step 1 requests python3 -I "$here/requests.py" "$rep/requests"
step 1 native-test native_test
step 1 native-calls native_calls
step 1 prepare-build prepare_build
cd "$repo"
step 1 build-lgx build lgx
if [ ! -f "$mod/flake.lock" ]; then cp "$out/build/flake.lock" "$mod/flake.lock"; fi
step 1 build-logosctl logosctl_build ctl logos
core dev "$out/logos/bin/logosctl" "$out/lgx"
if [ "$PORTABLE" != 0 ]; then
  if step 0 build-lgx-portable build lgx-portable && step 0 build-logosctl-portable logosctl_build ctl-bundle-dir logos-portable; then
    core portable "$out/logos-portable/bin/logosctl" "$out/lgx-portable" || true
  fi
fi
step 0 sizes sizes || true
finish
