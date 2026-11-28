#!/usr/bin/env bash
# The `ferrata` CLI (cli/) and its wallet library (cli/wallet/) built against LEZ v0.4.0 (a255b42c), beside the build
# the repository commits (LEZ v0.4.0-rc1) and the test network's (v0.4.0-rc3, deploy/testnet/cli-rc3/build.sh). It
# rewrites manifests, so it runs on a copy of the tree, never on a working tree:
#
#   cli/v040/build.sh SRC [MODE [CARGO ARGS...]]
#       SRC   a copy of the tree (`git archive HEAD | tar -x -C SRC`), with LEZ v0.4.0 at SRC/../upstream/lez-v0.4.0
#             (scripts/fetch-upstream.sh fetches it)
#       MODE  check (the library and binaries, the default), build (release: ferrata, ferrata-relayer,
#             ferrata-gasdrop), test (the library's unit tests and the named integration tests in CARGO ARGS),
#             monitor-test (monitor/attest, the attestation side's monitor: its tests, on v0.4.0's lee_core)
#
# 1. lez/scripts/lez-select.sh v0.4.0: lez/'s manifests, the `lez-040` feature, lez/lockfiles/v0.4.0.patch;
# 2. cli/ and cli/wallet/: every `upstream/lez-v0.4.0-rc1/` path to `upstream/lez-v0.4.0/`, and the RISC Zero client
#    pin `risc0-zkvm =3.0.5` to LEZ v0.4.0's `=3.1.0` (LEZ #1124);
# 3. cli/Cargo.lock: cli/lockfiles/Cargo.lock.v0.4.0 (v0.4.0's own lockfile) and `--locked`; without that file cargo
#    resolves and the result is written to SRC/cli/lockfiles/Cargo.lock.v0.4.0 for committing (monitor/attest: its
#    LEZ path, monitor/attest/lockfiles/Cargo.lock.v0.4.0 likewise);
# 4. cargo MODE --features lez-040 (the calls whose LEZ API differs on v0.4.0; it implies `lez-rc3`).
# NO_HARNESS=1 (test mode, optional): without the dev-dependency lez/harness, which only the end-to-end tests and the
# mini-app example use (tests/{e2e,both_halves,roundtrip_b,attest_faults}.rs, examples/miniapp_devnet.rs): a quicker
# build of the library's and the other integration tests (lez/harness builds on v0.4.0 since lez-040-core c36a60c4);
# the lockfile is then pruned by cargo (no --locked).
set -euo pipefail
SRC="$(cd "${1:?usage: $0 SRC [check|build|test] [cargo args]}" && pwd)"; MODE="${2:-check}"; shift $(( $# > 1 ? 2 : 1 ))
[ -f "$SRC/../upstream/lez-v0.4.0/Cargo.toml" ] || { echo "LEZ v0.4.0 not found at $SRC/../upstream/lez-v0.4.0" >&2; exit 1; }
[ ! -e "$SRC/.git" ] || { echo "refused: $SRC is a git working tree; run on a copy (the manifests are rewritten)" >&2; exit 1; }
( cd "$SRC/lez" && { [ "$(scripts/lez-select.sh --show)" = v0.4.0 ] || scripts/lez-select.sh v0.4.0; } )
for m in cli/Cargo.toml cli/wallet/Cargo.toml monitor/attest/Cargo.toml; do
  sed -i -E 's#upstream/lez-v0\.4\.0-rc[0-9]+/#upstream/lez-v0.4.0/#g; s#(risc0-zkvm = \{ version = )"=3\.0\.5"#\1"=3.1.0"#' "$SRC/$m"
  ! grep -vE '^\s*#' "$SRC/$m" | grep -qE 'upstream/lez-v0\.4\.0-rc[0-9]+/|risc0-zkvm = \{ version = "=3\.0\.5"' \
    || { echo "build.sh: $m still names an rc of LEZ or risc0 3.0.5" >&2; exit 1; }
done
dir=cli; [ "$MODE" != monitor-test ] || dir=monitor/attest
locked=()
if [ -f "$SRC/$dir/lockfiles/Cargo.lock.v0.4.0" ]; then install -m 644 "$SRC/$dir/lockfiles/Cargo.lock.v0.4.0" "$SRC/$dir/Cargo.lock"; locked=(--locked); fi
if [ "${NO_HARNESS:-0}" = 1 ] && [ "$MODE" = test ]; then
  sed -i '/^harness = { path = "\.\.\/lez\/harness" }$/d' "$SRC/cli/Cargo.toml"
  ! grep -q '^harness = ' "$SRC/cli/Cargo.toml" || { echo "build.sh: the harness dev-dependency is still there" >&2; exit 1; }
  locked=()
fi
cd "$SRC/$dir"
case "$MODE" in
  check) cargo check "${locked[@]}" --features lez-040 --lib --bins "$@" ;;
  build) cargo build --release "${locked[@]}" --features lez-040 --bin ferrata --bin ferrata-relayer --bin ferrata-gasdrop "$@" ;;
  test) cargo test "${locked[@]}" --features lez-040 "$@" ;;
  monitor-test) cargo test "${locked[@]}" "$@" ;;
  *) echo "unknown mode $MODE (check, build, test)" >&2; exit 2 ;;
esac
[ ${#locked[@]} -gt 0 ] || [ "${NO_HARNESS:-0}" = 1 ] || { mkdir -p lockfiles && cp Cargo.lock lockfiles/Cargo.lock.v0.4.0; echo "wrote $dir/lockfiles/Cargo.lock.v0.4.0"; }
