#!/usr/bin/env bash
# The inbound crossing on a live LEZ v0.4.0-rc1 sequencer and Logos Blockchain node (README.md).
#
#   run.sh check-stand-in                  the stand-in chain natively (no zone)
#   run.sh stand-in                        the session on the stand-in chain (before the deposits)
#   run.sh check-real CAPTURE_1 [CAPTURE_2] [VAULT]   the captures natively (no zone)
#   run.sh real CAPTURE_1 [CAPTURE_2] [VAULT]         the session on Sepolia as captured after each
#                                          deposit's finality (sepolia-plan.json `capture`)
#
# With CAPTURE_1 alone (deposit 0 finalized): the instance caught up to its finalized header, the
# bridge's Init (pairing), ImportRoot of root(1), the refusals that need one root (a leaf index at
# the count, a wrong opening), the plaintext claim of deposit 0 and its replay; records in
# results/sepolia-capture-1. With both captures: the same, then ImportRoot of root(2), deposit 0
# under the later root, deposit 1 with a wrong opening and its private claim with a real proof;
# records in results/sepolia. Capture k must hold the vault's first k deposits.
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
    captures=(); vault=""
    for a in "$@"; do
      if [[ "$a" =~ ^0x[0-9a-fA-F]{40}$ ]]; then [ -z "$vault" ] || die "two vault addresses"; vault="$a"
      elif [ -f "$a/fixture.json" ]; then captures+=("$(cd "$a" && pwd)")
      else die "$a: not a capture (no fixture.json) nor a vault address (0x and 40 hex digits)"; fi
    done
    [ ${#captures[@]} -ge 1 ] && [ ${#captures[@]} -le 2 ] || die "run.sh $mode CAPTURE_1 [CAPTURE_2] [VAULT]"
    chain="--chain sepolia"
    for c in "${captures[@]}"; do chain="$chain --capture $c"; done
    [ -z "$vault" ] || chain="$chain --vault $vault"
    if [ ${#captures[@]} = 1 ]; then out="$HERE/results/sepolia-capture-1"; else out="$HERE/results/sepolia"; fi ;;
  *) die "run.sh check-stand-in | stand-in | check-real CAPTURE_1 [CAPTURE_2] [VAULT] | real CAPTURE_1 [CAPTURE_2] [VAULT]" ;;
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
# Where the machine reports fewer CPUs than it may use (the build box: nproc 2), the prover's
# thread count (RAYON_NUM_THREADS through FERRATA_LIVE_PROOF_THREADS; default 32).
export FERRATA_LIVE_PROOF_THREADS="${FERRATA_LIVE_PROOF_THREADS:-32}"
exec "$HERE/../run.sh" run
