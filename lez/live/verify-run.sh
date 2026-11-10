#!/usr/bin/env bash
# Re-check the recorded run in results/ (or in the directory given as --dir).
#
#   verify-run.sh [--dir DIR] [--rpc http://127.0.0.1:3040] [guest binary ...]
#
# Always: what needs no execution. The chain links and block hashes of the recorded blocks, the
# manifest (run.tsv) against the blocks (where each transaction is, whether its block lists it
# as reverted) and against the gas totals the sequencer wrote into each block's own fee
# transaction.
#
# With the guest binaries (the two .bin files the run deployed; README.md says how to obtain
# them; they are also picked up from programs/ here and from tmp/sequencer/program beside the
# repository): the program bytecode is put back into the blocks, every block hash then holds,
# the whole chain is replayed through LEZ's block application from the pre-genesis state, and
# every row of the manifest is compared with what LEZ's settlement charges. Needs r0vm 3.0.5.
#
# With --rpc: a running sequencer must serve the recorded chain (same block hashes, the
# manifest's transactions at the manifest's blocks).
#
# Builds the driver first if it is not built (run.sh build-driver: it links LEZ's crates).
set -euo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/env.sh"

DIR="$LIVE_DIR/results$([ "$LEZ_TAG" = v0.4.0-rc1 ] || echo "/$LEZ_TAG")"
RPC=""
BINARIES=()
while [ $# -gt 0 ]; do
  case "$1" in
    --dir) DIR="$2"; shift 2 ;;
    --rpc) RPC="$2"; shift 2 ;;
    -h|--help) sed -n '2,22p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) BINARIES+=("$1"); shift ;;
  esac
done
if [ ${#BINARIES[@]} -eq 0 ]; then
  for f in "$LIVE_DIR"/programs/*.bin "$FERRATA_LIVE_TMP"/program/*.bin; do
    [ -f "$f" ] && BINARIES+=("$f")
  done
fi

[ -x "$BIN/live-driver" ] || "$LIVE_DIR/run.sh" build-driver

args=(verify --dir "$DIR")
for f in "${BINARIES[@]}"; do args+=(--binary "$f"); done
env -u RISC0_DEV_MODE "$BIN/live-driver" "${args[@]}"

if [ -n "$RPC" ]; then
  [ ${#BINARIES[@]} -ge 2 ] || die "--rpc needs the two guest binaries (the driver derives the accounts of the run from them)"
  # The program binary is the larger of the two.
  program="$(ls -S "${BINARIES[@]}" | sed -n 1p)"; scripted="$(ls -S "${BINARIES[@]}" | tail -1)"
  env -u RISC0_DEV_MODE "$BIN/live-driver" check-rpc --dir "$DIR" --rpc "$RPC" \
    --run-dir "$FERRATA_LIVE_TMP/check-rpc" --program-bin "$program" --scripted-bin "$scripted"
fi
