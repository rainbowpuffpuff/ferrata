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
# outside it. Run it on mains power with other work closed. The two proving runs keep every hardware thread busy (at
# nice 10, so the desktop stays usable) and hold about 9.3 GiB each: about 50-55 minutes in all at 8 threads (the
# attestation 19-21 minutes of proving, the claim and the burn 31-34; measured in containers of that shape, the
# rehearsal 50 minutes in all), less with 16. P4_CPUS=N holds the provers to the first N hardware threads (taskset,
# and RAYON_NUM_THREADS=N), recorded in OUT_DIR/machine.txt; the circuits' C++ kernels start one thread per hardware
# thread whatever RAYON_NUM_THREADS says, so the affinity is what holds them. x86-64 Linux only (the staged binaries);
# a Mac runs lez/results/p4-macos's procedure instead. No network beyond a loopback socket to its own r0vm, no keys,
# no sudo; it writes only inside OUT_DIR (and sets +x on the staged binaries). It exits 1 if a proof or a settlement
# fails, 2 if it refuses to start.
#
# If STAGE_DIR also holds release/ (p4-laptop-build.sh with P4_RELEASE_BIN), RFP-022's third user-device proof runs
# last: the release's succinct proof (the holder's: its input holds the burn secret; the Groth16 wrap is any relayer's),
# ferrata-e2e with release_31 and its own r0vm (RISC Zero 3.1.0, the client ferrata-e2e is built with), the crown
# rehearsal 3's burn private-1 as witness; about a minute at 8 threads, 1.5 GiB. Its "settlement" is ferrata-e2e
# verify: the receipt against release_31's image id and the journal against the native one.
set -euo pipefail
usage="usage: p4-laptop-owner.sh STAGE_DIR OUT_DIR"
stage="$(realpath "${1:?$usage}")"
out="$(realpath -m "${2:?$usage}")"
die() { echo "p4-laptop-owner: $*" >&2; exit 2; }
cpus="${P4_CPUS:-}"
pin=()
if [ -n "$cpus" ]; then
  case "$cpus" in *[!0-9]*|0*) die "P4_CPUS must be a whole number of hardware threads, 1 or more";; esac
  [ "$cpus" -le "$(nproc --all)" ] || die "P4_CPUS=$cpus: this machine has $(nproc --all) hardware threads"
  command -v taskset > /dev/null || die "P4_CPUS needs taskset (util-linux)"
  pin=(taskset -c "0-$((cpus - 1))" env "RAYON_NUM_THREADS=$cpus")
fi

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
jobs=(attest-pad bridge-pad)
if [ -d "$stage/release" ]; then
  jobs+=(release)
  for f in ferrata-e2e r0vm release.bin MANIFEST.json witness/release-private-1.input witness/release-private-1.json; do
    [ -f "$stage/release/$f" ] || die "missing $stage/release/$f (release/ as p4-laptop-build.sh stages it)"
  done
fi
# Every staged file against the sha256 its manifest recorded (the bundle as the build box staged it).
python3 -I - "$stage" "${jobs[@]}" <<'EOF' || die "the bundle does not match its manifests"
import hashlib, json, pathlib, sys
stage = pathlib.Path(sys.argv[1])
bad = 0
for job in sys.argv[2:]:
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
[ -d "$stage/release" ] && chmod +x "$stage/release/ferrata-e2e" "$stage/release/r0vm"
get() { python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))[sys.argv[2]])' "$1" "$2"; }
{
  echo "date: $(date -u +%FT%TZ)"
  uname -srm
  lscpu
  free -b
  echo "on mains power: $(cat /sys/class/power_supply/A*/online 2>/dev/null | head -1 || echo unknown)"
  echo "cpufreq governor: $(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null || echo unknown)"
  echo "energy performance preference: $(cat /sys/devices/system/cpu/cpu0/cpufreq/energy_performance_preference 2>/dev/null || echo unknown)"
  echo "platform profile: $(cat /sys/firmware/acpi/platform_profile 2>/dev/null || echo unknown)"
  echo "battery: $(cat /sys/class/power_supply/BAT*/status 2>/dev/null | head -1 || echo none) $(cat /sys/class/power_supply/BAT*/capacity 2>/dev/null | head -1)"
  echo "prover niceness: 10"
  if [ -n "$cpus" ]; then echo "prover CPUs: $cpus of $(nproc --all) (P4_CPUS: taskset -c 0-$((cpus - 1)), RAYON_NUM_THREADS=$cpus)"
  else echo "prover CPUs: all $(nproc --all)"; fi
  echo "r0vm: $("$r0vm" --version) $(sha256sum "$r0vm" | cut -c1-64)"
  for b in attest-pad/prove_attest bridge-pad/prove_bridge; do echo "$b: $(sha256sum "$stage/$b" | cut -c1-64)"; done
  if [ -d "$stage/release" ]; then
    echo "release/r0vm: $("$stage/release/r0vm" --version) $(sha256sum "$stage/release/r0vm" | cut -c1-64)"
    for b in release/ferrata-e2e release/release.bin; do echo "$b: $(sha256sum "$stage/$b" | cut -c1-64)"; done
  fi
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
  ( cd "$d" && exec nice -n 10 ${pin[@]+"${pin[@]}"} env RISC0_DEV_MODE=0 RISC0_SERVER_PATH="$r0vm" RISC0_INFO=1 RUST_LOG=info \
      TMPDIR="$d/tmp" "${ENVS[@]}" /usr/bin/time -v "$bin" "${args[@]}" ) > "$d/prove.log" 2>&1 && rc=0 || rc=$?
  kill "$s" 2> /dev/null || true; wait "$s" 2> /dev/null || true
  echo "exit $rc, wall $(awk -v a="$(date +%s.%N)" -v b="$t0" 'BEGIN { printf "%.1f", a - b }') s" > "$d/exit.txt"
  rm -rf "$d/tmp"
  if [ "$rc" = 0 ]; then
    ( cd "$d" && exec nice -n 10 ${pin[@]+"${pin[@]}"} env RISC0_DEV_MODE=0 RISC0_SERVER_PATH="$r0vm" "${ENVS[@]}" "$bin" --settle "${txs[@]}" ) \
      > "$d/settle.log" 2>&1 && echo "settle exit 0" >> "$d/exit.txt" || echo "settle exit $?" >> "$d/exit.txt"
  fi
  echo "$(date +%T) $name: $(tr '\n' ' ' < "$d/exit.txt")"
  grep -h 'REAL PROOF' "$d/prove.log" || true
}

run_release() {  # the release's succinct proof, then its verification (the settlement's place)
  local rs="$stage/release" d="$out/release" id
  id=$(get "$rs/MANIFEST.json" image_id)
  local io=(--program "$rs/release.bin" --input "$rs/witness/release-private-1.input" --meta "$rs/witness/release-private-1.json")
  local renv=(RISC0_DEV_MODE=0 RISC0_SERVER_PATH="$rs/r0vm" RISC0_INFO=1 RUST_LOG=info TMPDIR="$d/tmp")
  rm -rf "$d"; mkdir -p "$d/tmp"
  echo "$(date +%T) release: executing, then proving (log $d/prove.log)"
  ( cd "$d" && exec nice -n 10 ${pin[@]+"${pin[@]}"} env "${renv[@]}" "$rs/ferrata-e2e" exec "${io[@]}" ) > "$d/exec.log" 2>&1 || true
  sample > "$d/samples.txt" & local s=$!
  local t0; t0=$(date +%s.%N)
  ( cd "$d" && exec nice -n 10 ${pin[@]+"${pin[@]}"} env "${renv[@]}" /usr/bin/time -v "$rs/ferrata-e2e" prove "${io[@]}" \
      --out-dir "$d/prove" ) > "$d/prove.log" 2>&1 && rc=0 || rc=$?
  kill "$s" 2> /dev/null || true; wait "$s" 2> /dev/null || true
  echo "exit $rc, wall $(awk -v a="$(date +%s.%N)" -v b="$t0" 'BEGIN { printf "%.1f", a - b }') s" > "$d/exit.txt"
  rm -rf "$d/tmp"
  if [ "$rc" = 0 ]; then
    ( cd "$d" && exec env "${renv[@]}" "$rs/ferrata-e2e" verify --receipt "$d/prove/receipt-succinct.bin" --image-id "$id" \
        --meta "$rs/witness/release-private-1.json" ) > "$d/settle.log" 2>&1 \
      && echo "settle exit 0" >> "$d/exit.txt" || echo "settle exit $?" >> "$d/exit.txt"
  fi
  echo "$(date +%T) release: $(tr '\n' ' ' < "$d/exit.txt")"
  grep -h '^{' "$d/prove.log" | tail -1 | cut -c1-240 || true
}

fixture="$(basename "$(get "$stage/attest-pad/MANIFEST.json" fixtures_from)")"
ENVS=("FERRATA_EXPECT_CONSUMER_IMAGE_ID=$(get "$stage/attest-pad/MANIFEST.json" expected_consumer_image_id)"
      "FERRATA_ATTEST_FIXTURE_DIR=$stage/attest-pad/$fixture")
run attest-pad "$stage/attest-pad/prove_attest" --shape wallet --tx-out tx-attest.borsh -- tx-attest.borsh
ENVS=("FERRATA_EXPECT_BRIDGE_IMAGE_ID=$(get "$stage/bridge-pad/MANIFEST.json" expected_bridge_image_id)")
run bridge-pad "$stage/bridge-pad/prove_bridge" --shape wallet --claim-out tx-claim.borsh --burn-out tx-burn.borsh \
  -- tx-claim.borsh tx-burn.borsh
[ -d "$stage/release" ] && run_release
fail=0
for job in "${jobs[@]}"; do
  grep -q '^exit 0,' "$out/$job/exit.txt" && grep -qx 'settle exit 0' "$out/$job/exit.txt" \
    || { echo "p4-laptop-owner: $job did not prove and settle: $(tr '\n' ' ' < "$out/$job/exit.txt")" >&2; fail=1; }
done
echo "records in $out; next: python3 -I bench/perf/summarize_p4.py --owner $out"
exit "$fail"
