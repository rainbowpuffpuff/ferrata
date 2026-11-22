#!/usr/bin/env bash
# Builds what bench/perf/gate-sequencer.sh runs, on the build box (spike/modal-build's mrun; never on a laptop):
# crown-driver (lez/crown/driver: deploys the programs on a new zone), the zone's bundle (crown-live-3's LEZ bundle,
# repacked with this tree's crown-driver and session.sh, as e2e/seqnet/run.sh build does) and ferrata-updater with its
# sequencer backend (lez/updater/sequencer).
#
#   bash bench/perf/gate-build.sh BUNDLE      # BUNDLE: bundle-crown-live-3.tar.zst (sha256 0013bab1...)
#
# Output in $PERF_GATE_TMP (default spike/tmp/perf-gate): bin/crown-driver, bin/ferrata-updater, crown-lez/bundle.tar.zst.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
T="${PERF_GATE_TMP:-$REPO/../tmp/perf-gate}"
bundle="${1:?usage: gate-build.sh BUNDLE}"
export CARGO_TERM_COLOR=never PATH="$HOME/.cargo/bin:$PATH"
mkdir -p "$T/bin" "$T/crown-lez/bin" "$T/logs"
echo "[$(date -u +%T)] crown-driver"
( cd "$REPO/lez/crown/driver" && CARGO_TARGET_DIR="$T/target-lez" cargo build --release --locked ) > "$T/logs/driver.log" 2>&1 \
  || { grep -E -A7 '^error' "$T/logs/driver.log" | head -40; exit 1; }
cp "$T/target-lez/release/crown-driver" "$T/bin/" && cp "$T/bin/crown-driver" "$T/crown-lez/bin/crown-driver"
echo "[$(date -u +%T)] ferrata-updater (sequencer backend)"
( cd "$REPO/lez/updater/sequencer" && CARGO_TARGET_DIR="$T/target-updater" cargo build --release --locked ) > "$T/logs/updater.log" 2>&1 \
  || { grep -E -A7 '^error' "$T/logs/updater.log" | head -40; exit 1; }
cp "$T/target-updater/release/ferrata-updater" "$T/bin/"
echo "[$(date -u +%T)] bundle repack"
CROWN_LEZ_TMP="$T/crown-lez" bash "$REPO/lez/crown/build.sh" repack "$bundle" > "$T/logs/repack.log" 2>&1 \
  || { tail -n 20 "$T/logs/repack.log"; exit 1; }
tail -n 4 "$T/logs/repack.log"
( cd "$T/bin" && sha256sum crown-driver ferrata-updater )
