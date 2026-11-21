#!/usr/bin/env bash
# RFP-021 P.7, P.9 and RFP-022 P.7 (the LEZ side): writes lez/results/perf-state.md from
# lez/harness/src/bin/perf_state.rs, on the build box (mrun; dev mode is not needed: public transactions only).
#   bash bench/perf/state.sh            # from the repository root
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO/lez"
export RISC0_HOME="${RISC0_HOME:-$REPO/../risc0-home-lez}"
cargo build -p harness --locked --bin perf_state > /dev/null 2>&1 || cargo build -p harness --locked --bin perf_state
out="$REPO/lez/results/perf-state.md"
{
  sed -n '2,$p' "$REPO/bench/perf/state-header.md"
  echo
  echo "Measured $(date -u +%F) on $(grep -m1 "model name" /proc/cpuinfo | cut -d: -f2 | sed "s/^ //") ($(nproc --all) CPUs visible), commit ${FERRATA_HEAD:-$(git -C "$REPO" rev-parse --short HEAD 2>/dev/null || echo unknown)}."
  echo
  target/debug/perf_state | sed '1d'
} > "$out"
echo "wrote $out"
