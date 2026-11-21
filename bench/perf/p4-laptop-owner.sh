#!/usr/bin/env bash
# RFP-021 P.4 and RFP-022 P.4 on a real laptop: the owner's step. The same three proofs as the laptop-sized
# containers of lez/results/perf-p4-laptop.md (the private attestation, the private claim and the private burn, each
# padded as LEZ's wallet pads every private transaction), from the bundle bench/perf/p4-laptop-build.sh stages on the
# build box (prove_attest, prove_bridge, the attestation's mainnet fixture, r0vm 3.0.5). Then each transaction is
# settled (its receipt verified) as the containers did.
#
#   bash bench/perf/p4-laptop-owner.sh STAGE_DIR OUT_DIR
#   python3 -I bench/perf/summarize_p4.py --owner OUT_DIR     # its rows for lez/results/perf-p4-laptop.md
#
# STAGE_DIR holds attest-pad/, bridge-pad/ and r0vm/r0vm as staged. Run it on mains power with nothing else busy:
# each proof keeps every hardware thread busy for 10 to 25 minutes and holds about 9.3 GiB. x86-64 Linux only (the
# staged binaries); a Mac runs lez/results/p4-macos's procedure instead.
set -euo pipefail
stage="$(realpath "${1:?usage: p4-laptop-owner.sh STAGE_DIR OUT_DIR}")"
out="$(realpath -m "${2:?usage: p4-laptop-owner.sh STAGE_DIR OUT_DIR}")"
mkdir -p "$out"
r0vm="$stage/r0vm/r0vm"
chmod +x "$r0vm" "$stage/attest-pad/prove_attest" "$stage/bridge-pad/prove_bridge"
get() { python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))[sys.argv[2]])' "$1" "$2"; }
{
  echo "date: $(date -u +%FT%TZ)"
  uname -srm
  lscpu
  free -b
  echo "on mains power: $(cat /sys/class/power_supply/A*/online 2>/dev/null | head -1 || echo unknown)"
  echo "cpufreq governor: $(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null || echo unknown)"
  echo "r0vm: $("$r0vm" --version) $(sha256sum "$r0vm" | cut -c1-64)"
  for b in attest-pad/prove_attest bridge-pad/prove_bridge; do echo "$b: $(sha256sum "$stage/$b" | cut -c1-64)"; done
} > "$out/machine.txt" 2>&1

# Memory beside GNU time's peak: the whole machine's used memory and r0vm's RSS, every second.
sample() {
  while :; do
    used=$(awk '/MemTotal/ {t=$2} /MemAvailable/ {a=$2} END {print t - a}' /proc/meminfo)
    rss=$(ps -C r0vm -o rss= 2>/dev/null | awk '{s+=$1} END {print s + 0}')
    echo "$(date +%s) $used $rss"
    sleep 1
  done
}

run() {  # name binary prove-args... -- txs...
  local name=$1 bin=$2; shift 2
  local args=() txs=()
  while [ "$1" != "--" ]; do args+=("$1"); shift; done; shift
  txs=("$@")
  local d="$out/$name"
  rm -rf "$d"; mkdir -p "$d/tmp"
  sample > "$d/samples.txt" & local s=$!
  local t0; t0=$(date +%s.%N)
  ( cd "$d" && exec env RISC0_DEV_MODE=0 RISC0_SERVER_PATH="$r0vm" RISC0_INFO=1 RUST_LOG=info TMPDIR="$d/tmp" "${ENVS[@]}" \
      /usr/bin/time -v "$bin" "${args[@]}" ) > "$d/prove.log" 2>&1 && rc=0 || rc=$?
  kill "$s" 2>/dev/null; wait "$s" 2>/dev/null || true
  echo "exit $rc, wall $(awk -v a="$(date +%s.%N)" -v b="$t0" 'BEGIN { printf "%.1f", a - b }') s" > "$d/exit.txt"
  rm -rf "$d/tmp"
  if [ "$rc" = 0 ]; then
    ( cd "$d" && exec env RISC0_DEV_MODE=0 RISC0_SERVER_PATH="$r0vm" "${ENVS[@]}" "$bin" --settle "${txs[@]}" ) \
      > "$d/settle.log" 2>&1 && echo "settle exit 0" >> "$d/exit.txt" || echo "settle exit $?" >> "$d/exit.txt"
  fi
  echo "$name: $(cat "$d/exit.txt" | tr '\n' ' ')"
  grep -h 'REAL PROOF' "$d/prove.log" || true
}

fixture="$(basename "$(get "$stage/attest-pad/MANIFEST.json" fixtures_from)")"
ENVS=("FERRATA_EXPECT_CONSUMER_IMAGE_ID=$(get "$stage/attest-pad/MANIFEST.json" expected_consumer_image_id)"
      "FERRATA_ATTEST_FIXTURE_DIR=$stage/attest-pad/$fixture")
run attest-pad "$stage/attest-pad/prove_attest" --shape wallet --tx-out tx-attest.borsh -- tx-attest.borsh
ENVS=("FERRATA_EXPECT_BRIDGE_IMAGE_ID=$(get "$stage/bridge-pad/MANIFEST.json" expected_bridge_image_id)")
run bridge-pad "$stage/bridge-pad/prove_bridge" --shape wallet --claim-out tx-claim.borsh --burn-out tx-burn.borsh \
  -- tx-claim.borsh tx-burn.borsh
echo "records in $out"
