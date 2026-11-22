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
# STAGE_DIR holds attest-pad/, bridge-pad/ and r0vm/ (r0vm and its SHA256SUMS) as staged; OUT_DIR is a new directory
# outside it. Run it on mains power with nothing else busy. The two proving runs keep every hardware thread busy (at
# nice 10, so the desktop stays usable) and hold about 9.3 GiB each: about 21 minutes for the attestation and 34 for
# the claim and the burn at 8 threads (measured in a container of that shape), less with 16. x86-64 Linux only (the
# staged binaries); a Mac runs lez/results/p4-macos's procedure instead. No network beyond a loopback socket to its
# own r0vm, no keys, no sudo; it writes only inside OUT_DIR (and sets +x on the staged binaries). It exits 1 if a
# proof or a settlement fails, 2 if it refuses to start.
set -euo pipefail
usage="usage: p4-laptop-owner.sh STAGE_DIR OUT_DIR"
stage="$(realpath "${1:?$usage}")"
out="$(realpath -m "${2:?$usage}")"
die() { echo "p4-laptop-owner: $*" >&2; exit 2; }

# Refuse what would fail an hour in, or delete the bundle: each job's directory OUT_DIR/<job> is emptied first.
[ "$(uname -s)/$(uname -m)" = "Linux/x86_64" ] || die "needs x86-64 Linux (this is $(uname -s)/$(uname -m))"
for c in python3 awk ps lscpu free sha256sum nice; do command -v "$c" > /dev/null || die "missing command: $c"; done
/usr/bin/time -v true > /dev/null 2>&1 || die "missing GNU time at /usr/bin/time (Fedora: dnf install time; Debian, Ubuntu: apt install time)"
case "$out/" in "$stage/"*) die "OUT_DIR must lie outside STAGE_DIR";; esac
case "$stage/" in "$out/attest-pad/"*|"$out/bridge-pad/"*) die "OUT_DIR would hold STAGE_DIR";; esac
r0vm="$stage/r0vm/r0vm"
for f in "$r0vm" "$stage/r0vm/SHA256SUMS" "$stage/attest-pad/prove_attest" "$stage/attest-pad/MANIFEST.json" \
         "$stage/bridge-pad/prove_bridge" "$stage/bridge-pad/MANIFEST.json"; do
  [ -f "$f" ] || die "missing $f (the bundle holds attest-pad/, bridge-pad/ and r0vm/ with r0vm and SHA256SUMS)"
done
# Every staged file against the sha256 its manifest recorded (the bundle as the build box staged it).
python3 -I - "$stage" <<'EOF' || die "the bundle does not match its manifests"
import hashlib, json, pathlib, sys
stage = pathlib.Path(sys.argv[1])
bad = 0
for job in ("attest-pad", "bridge-pad"):
    for name, want in json.load(open(stage / job / "MANIFEST.json"))["files"].items():
        got = hashlib.sha256((stage / job / name).read_bytes()).hexdigest()
        if got != want["sha256"]:
            print(f"{job}/{name}: sha256 {got}, manifest {want['sha256']}", file=sys.stderr); bad += 1
want = (stage / "r0vm" / "SHA256SUMS").read_text().split()[0]
got = hashlib.sha256((stage / "r0vm" / "r0vm").read_bytes()).hexdigest()
if got != want:
    print(f"r0vm/r0vm: sha256 {got}, SHA256SUMS {want}", file=sys.stderr); bad += 1
sys.exit(1 if bad else 0)
EOF
mkdir -p "$out"
chmod +x "$r0vm" "$stage/attest-pad/prove_attest" "$stage/bridge-pad/prove_bridge"
get() { python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))[sys.argv[2]])' "$1" "$2"; }
{
  echo "date: $(date -u +%FT%TZ)"
  uname -srm
  lscpu
  free -b
  echo "on mains power: $(cat /sys/class/power_supply/A*/online 2>/dev/null | head -1 || echo unknown)"
  echo "cpufreq governor: $(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null || echo unknown)"
  echo "prover niceness: 10"
  echo "r0vm: $("$r0vm" --version) $(sha256sum "$r0vm" | cut -c1-64)"
  for b in attest-pad/prove_attest bridge-pad/prove_bridge; do echo "$b: $(sha256sum "$stage/$b" | cut -c1-64)"; done
} > "$out/machine.txt" 2>&1

# Stop the memory sampler if the script ends early (an error, Ctrl-C).
trap 'kill $(jobs -p) 2> /dev/null || true' EXIT

# Memory beside GNU time's peak: the whole machine's used memory and r0vm's RSS, every second. A sample must not end
# the loop: `ps -C r0vm` fails while no r0vm runs (always at the first sample).
sample() {
  set +e +o pipefail
  while :; do
    used=$(awk '/MemTotal/ {t=$2} /MemAvailable/ {a=$2} END {print t - a}' /proc/meminfo)
    rss=$(ps -C r0vm -o rss= 2> /dev/null | awk '{s+=$1} END {print s + 0}')
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
  echo "$(date +%T) $name: proving (no output until it ends; log $d/prove.log)"
  sample > "$d/samples.txt" & local s=$!
  local t0; t0=$(date +%s.%N)
  ( cd "$d" && exec nice -n 10 env RISC0_DEV_MODE=0 RISC0_SERVER_PATH="$r0vm" RISC0_INFO=1 RUST_LOG=info \
      TMPDIR="$d/tmp" "${ENVS[@]}" /usr/bin/time -v "$bin" "${args[@]}" ) > "$d/prove.log" 2>&1 && rc=0 || rc=$?
  kill "$s" 2> /dev/null || true; wait "$s" 2> /dev/null || true
  echo "exit $rc, wall $(awk -v a="$(date +%s.%N)" -v b="$t0" 'BEGIN { printf "%.1f", a - b }') s" > "$d/exit.txt"
  rm -rf "$d/tmp"
  if [ "$rc" = 0 ]; then
    ( cd "$d" && exec nice -n 10 env RISC0_DEV_MODE=0 RISC0_SERVER_PATH="$r0vm" "${ENVS[@]}" "$bin" --settle "${txs[@]}" ) \
      > "$d/settle.log" 2>&1 && echo "settle exit 0" >> "$d/exit.txt" || echo "settle exit $?" >> "$d/exit.txt"
  fi
  echo "$(date +%T) $name: $(tr '\n' ' ' < "$d/exit.txt")"
  grep -h 'REAL PROOF' "$d/prove.log" || true
}

fixture="$(basename "$(get "$stage/attest-pad/MANIFEST.json" fixtures_from)")"
ENVS=("FERRATA_EXPECT_CONSUMER_IMAGE_ID=$(get "$stage/attest-pad/MANIFEST.json" expected_consumer_image_id)"
      "FERRATA_ATTEST_FIXTURE_DIR=$stage/attest-pad/$fixture")
run attest-pad "$stage/attest-pad/prove_attest" --shape wallet --tx-out tx-attest.borsh -- tx-attest.borsh
ENVS=("FERRATA_EXPECT_BRIDGE_IMAGE_ID=$(get "$stage/bridge-pad/MANIFEST.json" expected_bridge_image_id)")
run bridge-pad "$stage/bridge-pad/prove_bridge" --shape wallet --claim-out tx-claim.borsh --burn-out tx-burn.borsh \
  -- tx-claim.borsh tx-burn.borsh
fail=0
for job in attest-pad bridge-pad; do
  grep -q '^exit 0,' "$out/$job/exit.txt" && grep -qx 'settle exit 0' "$out/$job/exit.txt" \
    || { echo "p4-laptop-owner: $job did not prove and settle: $(tr '\n' ' ' < "$out/$job/exit.txt")" >&2; fail=1; }
done
echo "records in $out; next: python3 -I bench/perf/summarize_p4.py --owner $out"
exit "$fail"
