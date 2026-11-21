#!/usr/bin/env bash
# RFP-021 R.9 and R.10 on a running sequencer, and P.6 (the updater daemon's CPU, memory and bandwidth): two honest
# ferrata-updater daemons and an adversary follow the lab's recorded mainnet chain on one LEZ v0.4.0-rc1 zone
# (sequencer + Logos Blockchain node, LEZ's local configuration), on the build box (spike/modal-build's mrun; never on
# a laptop; nothing is sent to any public network: the beacon data is bench/perf/beacon_replay.py's replay of the
# lab's record on loopback). After bench/perf/gate-build.sh.
#
#   bash bench/perf/gate-sequencer.sh NAME          # records in bench/perf/results/gate-NAME/
#   python3 -I bench/perf/summarize_gate.py bench/perf/results/gate-NAME
#
# Environment: GATE_ENTRIES (recorded updates replayed, default 24), GATE_STEP (seconds between them, 150),
# GATE_WINDOW (the honest daemons' deferral window in rounds, 8), GATE_POLL (their round, seconds, 4),
# GATE_ADVERSARY (1: a third daemon that never defers and polls twice as often; 0: none), GATE_DEADLINE_MIN (150).
#
# 1. LEZ session A (lez/crown/session.sh a, CROWN_CHAIN=stand-in): a new zone with the final attestation and bridge
#    programs deployed (crown-driver crown-a); saved. 2. session.sh x with bench/perf/gate-hook.sh while the zone runs.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
T="${PERF_GATE_TMP:-$REPO/../tmp/rfp-C/gate}"
NAME="${1:?usage: gate-sequencer.sh NAME}"
W="$T/w-$NAME"; VOL="$W/vol"; OUT="$REPO/bench/perf/results/gate-$NAME"
say() { echo "[$(date -u +%H:%M:%S)] $*"; }
[ -x "$T/bin/ferrata-updater" ] && [ -f "$T/crown-lez/bundle.tar.zst" ] || { say "run bench/perf/gate-build.sh first"; exit 1; }
rm -rf "$W" "$OUT"; mkdir -p "$VOL/inputs" "$OUT"
cp "$T/crown-lez/bundle.tar.zst" "$VOL/bundle.tar.zst"
{
  echo "date: $(date -u +%FT%TZ)"; echo "nproc --all: $(nproc --all)"; grep -m1 'model name' /proc/cpuinfo; grep MemTotal /proc/meminfo
  echo "bundle sha256 $(sha256sum "$VOL/bundle.tar.zst" | cut -c1-64)"
  echo "ferrata-updater sha256 $(sha256sum "$T/bin/ferrata-updater" | cut -c1-64)"
  echo "GATE_ENTRIES=${GATE_ENTRIES:-24} GATE_STEP=${GATE_STEP:-150} GATE_WINDOW=${GATE_WINDOW:-8} GATE_POLL=${GATE_POLL:-4} GATE_ADVERSARY=${GATE_ADVERSARY:-1}"
} > "$OUT/machine.txt"
export CROWN_LEZ_VOL="$VOL" CROWN_LEZ_WORK="$W/zone" CROWN_CHAIN=stand-in
say "session A: a new zone, the programs deployed"
bash "$REPO/lez/crown/session.sh" a > "$W/session-a.log" 2>&1 || { tail -n 30 "$W/session-a.log"; exit 1; }
cp "$VOL/session-a/session-a.json" "$OUT/" 2>/dev/null || true
say "session X: the daemons (hook bench/perf/gate-hook.sh)"
xrc=0
GATE_OUT="$OUT" GATE_UPDATER_BIN="$T/bin/ferrata-updater" CROWN_HOOK="$REPO/bench/perf/gate-hook.sh" \
  CROWN_DEADLINE_MIN=$(( ${GATE_DEADLINE_MIN:-150} + 15 )) bash "$REPO/lez/crown/session.sh" x > "$W/session-x.log" 2>&1 || xrc=$?
cp "$VOL/session-x/logs/hook.log" "$OUT/hook.log" 2>/dev/null || true
cp "$VOL/session-x/logs/session.log" "$OUT/session-x.log" 2>/dev/null || true
cp "$VOL/session-a/logs/session.log" "$OUT/session-a.log" 2>/dev/null || true
say "session X rc=$xrc"
tail -n 6 "$OUT/hook.log" 2>/dev/null || tail -n 30 "$W/session-x.log"
