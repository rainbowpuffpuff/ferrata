#!/usr/bin/env bash
# Builds what the lab runs, on a build machine (spike/modal-build's mrun box), and packs it:
#
#   build.sh chain    LEZ's sequencer_service and the Bedrock node (both as ../live/run.sh builds
#                     them) and the program's guest image (lez/methods, as ../RESULTS.md §8)
#   build.sh driver   the lab driver (driver/)
#   build.sh all      chain and driver
#   build.sh pack     bundle.tar.zst in $LAB_TMP: bin/, node/, xdg-cache/, program/, and SHA256SUMS
#
# Everything goes to $LAB_TMP (default: tmp/lab beside the repository), nothing to /tmp.
set -euo pipefail
LAB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$LAB_DIR/../.." && pwd)"
BESIDE="$(cd "$REPO/.." && pwd)"
LAB_TMP="${LAB_TMP:-$BESIDE/tmp/lab}"
export FERRATA_LIVE_TMP="$LAB_TMP/live" FERRATA_LIVE_JOBS="${FERRATA_LIVE_JOBS:-16}" FERRATA_LIVE_NICE=0
mkdir -p "$LAB_TMP/logs" "$LAB_TMP/program"

guest() {
  # The program's guest, built as ../RESULTS.md §8 builds it (risc0-build, RISC Zero's Rust 1.91.1).
  ( cd "$REPO/lez" && RISC0_HOME="$BESIDE/risc0-home-lez" RISC0_BUILD_LOCKED=1 \
      CARGO_BUILD_JOBS="$FERRATA_LIVE_JOBS" cargo build -p methods --locked )
  local bin
  bin="$(find "$REPO/lez/target/riscv-guest" -path '*release*' -name 'ferrata_lez.bin' | head -n 1)"
  [ -n "$bin" ] || { echo "no ferrata_lez.bin under lez/target/riscv-guest" >&2; exit 1; }
  cp "$bin" "$LAB_TMP/program/ferrata_lez.bin"
  sha256sum "$LAB_TMP/program/ferrata_lez.bin"
}

case "${1:-}" in
  chain|all)
    # `all`: the driver after the sequencer (they share a target directory), the rest alongside.
    if [ "$1" = all ]; then
      { "$REPO/lez/live/run.sh" build-sequencer > "$LAB_TMP/logs/build-sequencer.log" 2>&1 \
          && "$0" driver > "$LAB_TMP/logs/build-driver.log" 2>&1; } &
    else
      "$REPO/lez/live/run.sh" build-sequencer > "$LAB_TMP/logs/build-sequencer.log" 2>&1 &
    fi
    seq=$!
    "$REPO/lez/live/run.sh" build-bedrock > "$LAB_TMP/logs/build-bedrock.log" 2>&1 &
    node=$!
    guest > "$LAB_TMP/logs/build-guest.log" 2>&1 &
    g=$!
    rc=0
    wait "$seq" || { echo "sequencer or driver build FAILED"; tail -n 20 "$LAB_TMP/logs/build-sequencer.log"; \
      { grep -E -A12 '^error' "$LAB_TMP/logs/build-driver.log" 2>/dev/null || true; } | sed -n 1,80p; rc=1; }
    wait "$node" || { echo "node build FAILED"; tail -n 20 "$LAB_TMP/logs/build-bedrock.log"; rc=1; }
    wait "$g" || { echo "guest build FAILED"; tail -n 20 "$LAB_TMP/logs/build-guest.log"; rc=1; }
    exit "$rc"
    ;;
  driver)
    # Shares the sequencer's target directory: the LEZ crates both link are built once.
    [ -f "$LAB_DIR/driver/Cargo.lock" ] || cp "$REPO/lez/live/driver/Cargo.lock" "$LAB_DIR/driver/Cargo.lock"
    ( cd "$LAB_DIR/driver" && CARGO_TARGET_DIR="$FERRATA_LIVE_TMP/target" CARGO_BUILD_JOBS="$FERRATA_LIVE_JOBS" \
        cargo build --release )
    mkdir -p "$FERRATA_LIVE_TMP/bin"
    cp "$FERRATA_LIVE_TMP/target/release/lab-driver" "$FERRATA_LIVE_TMP/bin/lab-driver"
    sha256sum "$FERRATA_LIVE_TMP/bin/lab-driver"
    ;;
  pack)
    b="$LAB_TMP/bundle"
    rm -rf "$b" && mkdir -p "$b/bin" "$b/node" "$b/program"
    cp "$FERRATA_LIVE_TMP/bin/sequencer_service" "$FERRATA_LIVE_TMP/bin/sequencer_service.toolchain" "$b/bin/"
    cp "$FERRATA_LIVE_TMP/bin/lab-driver" "$b/bin/"
    cp "$FERRATA_LIVE_TMP/bedrock/.resolved-host/logos-blockchain-node" "$FERRATA_LIVE_TMP/bedrock/.resolved-host/metadata.json" "$b/node/"
    cp -r "$FERRATA_LIVE_TMP/xdg-cache" "$b/xdg-cache"
    cp "$LAB_TMP/program/ferrata_lez.bin" "$b/program/"
    ( cd "$b" && find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS )
    tar -C "$b" -cf - . | zstd -q -T0 -9 -o "$LAB_TMP/bundle.tar.zst" -f
    ls -l "$LAB_TMP/bundle.tar.zst"; du -sh "$b"; cat "$b/SHA256SUMS"
    ;;
  *) sed -n '2,9p' "${BASH_SOURCE[0]}"; exit 2 ;;
esac
