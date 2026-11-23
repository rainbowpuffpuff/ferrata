#!/usr/bin/env bash
# RFP-021 P.9: writes lez/results/perf-state-actions.md from `perf_state actions`
# (lez/harness/src/bin/perf_state.rs, lez/harness/src/accumulated.rs), on the build box (mrun). Every action that
# lez/results/perf-state.md (bash bench/perf/state.sh) does not sweep, charged with 0 to 100,000 more of each account
# that accumulates beside it. RISC0_DEV_MODE=1 for the private transactions only (a fake receipt; LEZ charges a
# private transaction the same whatever its proof); their circuits' cycles from r0vm's session log (RISC0_INFO=1).
#   bash bench/perf/state-actions.sh    # from the repository root
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO/lez"
export RISC0_HOME="${RISC0_HOME:-$REPO/../risc0-home-lez}"
cargo build -p harness --locked --bin perf_state > /dev/null 2>&1 || cargo build -p harness --locked --bin perf_state
out="$REPO/lez/results/perf-state-actions.md"
log="$REPO/lez/target/perf-state-actions-r0vm.log"
{
  sed -n '2,$p' "$REPO/bench/perf/state-actions-header.md"
  echo
  model="$(grep -m1 "model name" /proc/cpuinfo | cut -d: -f2 | sed "s/^ //")"
  [ "$model" = unknown ] && model="the build box (its CPU model is not reported inside the sandbox)"
  echo "Measured $(date -u +%F) on $model ($(nproc --all) CPUs visible), commit ${FERRATA_HEAD:-$(git -C "$REPO" rev-parse --short HEAD 2>/dev/null || echo unknown)}."
  echo
  # r0vm's session log (stderr) for the private transactions' circuits: bench/perf/circuit-cycles.py.
  RISC0_DEV_MODE=1 RISC0_INFO=1 RUST_LOG=risc0_zkvm=info target/debug/perf_state actions 2> "$log" | sed '1d'
  python3 -I "$REPO/bench/perf/circuit-cycles.py" "$log"
} > "$out"
echo "wrote $out"
