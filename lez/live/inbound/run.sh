#!/usr/bin/env bash
# The inbound crossing on a live LEZ v0.4.0-rc1 sequencer and Logos Blockchain node (README.md).
#
#   run.sh check-stand-in                  the stand-in chain natively (no zone)
#   run.sh stand-in                        the session on the stand-in chain (before the deposits)
#   run.sh check-real CAPTURE_1 CAPTURE_2 [VAULT]   the captures natively (no zone)
#   run.sh real CAPTURE_1 CAPTURE_2 [VAULT]         the session on Sepolia as captured after each
#                                          deposit's finality (sepolia-plan.json `capture`)
#
# The programs deployed are the frozen binaries of sepolia-plan.json (FERRATA_INBOUND_PROGRAMS,
# default tmp/inbound/programs beside the repository): ferrata_lez.bin (the attestation program,
# deployed as "attest") and ferrata_bridge.bin (deployed as "bridge"); they are checked by SHA-256
# and never rebuilt (the bridge's image id is part of configId). FERRATA_INBOUND_PRIVATE: real
# (default: the private claim proven for real) | fake (a fake receipt, which the sequencer must
# drop) | skip. Everything else as ../run.sh (build first: ../run.sh build, build-bedrock).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$HERE/../env.sh"
PROGRAMS="${FERRATA_INBOUND_PROGRAMS:-$BESIDE/tmp/inbound/programs}"
ATTEST_BIN="$PROGRAMS/ferrata_lez.bin"
BRIDGE_BIN="$PROGRAMS/ferrata_bridge.bin"
pinned() { [ -f "$1" ] && [ "$(sha256sum "$1" | cut -d' ' -f1)" = "$2" ] || die "$1 is not the frozen binary of sepolia-plan.json (sha256 $2)"; }
pinned "$ATTEST_BIN" f99871e715f63a206bd17720b01373a0fc865d7e803be04e9b19e7a0aa4e82bb
pinned "$BRIDGE_BIN" aff2205a55c6f496881019f8f25b8d754d2c83d623e86abcad504c7f2969605d
mode="${1:-}"; [ $# -gt 0 ] && shift
case "$mode" in
  check-stand-in|stand-in) chain="--chain stand-in"; out="$HERE/results/stand-in" ;;
  check-real|real)
    [ $# -ge 2 ] || die "run.sh $mode CAPTURE_1 CAPTURE_2 [VAULT]"
    for d in "$1" "$2"; do [ -f "$d/fixture.json" ] || die "$d: not a capture (no fixture.json)"; done
    chain="--chain sepolia --capture $(cd "$1" && pwd) --capture $(cd "$2" && pwd)${3:+ --vault $3}"; out="$HERE/results/sepolia" ;;
  *) die "run.sh check-stand-in | stand-in | check-real CAPTURE_1 CAPTURE_2 [VAULT] | real CAPTURE_1 CAPTURE_2 [VAULT]" ;;
esac
case "$mode" in
  check-*) exec "$BIN/live-driver" inbound-check --program-bin "$ATTEST_BIN" --bridge-bin "$BRIDGE_BIN" $chain ;;
esac
export FERRATA_PROGRAM_BIN="$ATTEST_BIN"
# ../run-live.sh wants LEZ's scripted test program too (not deployed here): the copy lez/methods builds.
export FERRATA_SCRIPTED_BIN="${FERRATA_SCRIPTED_BIN:-$REPO/lez/target/riscv-guest/methods/scripted/riscv32im-risc0-zkvm-elf/release/scripted.bin}"
export FERRATA_LIVE_DRIVER_EXTRA="--bridge-bin $BRIDGE_BIN $chain --private ${FERRATA_INBOUND_PRIVATE:-real}"
export FERRATA_LIVE_PHASES="${FERRATA_INBOUND_PHASES:-info inbound-deploy inbound export}"
export FERRATA_LIVE_OUT="$out"
exec "$HERE/../run.sh" run
