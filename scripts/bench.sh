#!/usr/bin/env bash
# `just bench`: regenerate the proposal's performance numbers (§B8, docs/performance.md) with real proving, and record
# the hardware each number was measured on. Every stage writes into OUT (default bench/report/<UTC date>-<commit>/).
#
#   scripts/bench.sh hw            OUT   the machine: CPU model and flags, threads, cgroup CPU and memory limits, memory,
#                                        kernel, toolchains (rustc, r0vm, forge, solc)            -> OUT/hardware.json
#   scripts/bench.sh gas           OUT   the vault's gas on Foundry's schedule (forge test --isolate GasScenarios)
#                                                                                                 -> OUT/gas/
#   scripts/bench.sh cycles        OUT   light-client cycles in the zkVM executor (bench/, blst-accel: the on-chain,
#                                        single, core-split and step modes)                       -> OUT/cycles/
#   scripts/bench.sh release-proof OUT   a REAL succinct proof of the release program (RISC0_DEV_MODE=0) on this
#                                        machine's CPU, for the crown rehearsal's private burn; verified
#                                                                                                 -> OUT/release-proof/
#   scripts/bench.sh user-stage    OUT   build and stage the user-side provers (prove_attest, prove_bridge, padded as
#                                        LEZ's wallet pads) and r0vm for the minimum-hardware runs -> OUT/stage/
#   scripts/bench.sh user-prove    OUT   (from the laptop: Modal) the private attestation, claim and burn and the
#                                        release, proven for real in containers held to BENCH_CPUS (default 16) CPUs
#                                        and 16 GiB (lez/results/p4-minimum/minimum.py)           -> OUT/user/
#   scripts/bench.sh report        OUT   OUT/REPORT.md: every number with its hardware and command
#
# The build box runs hw, gas, cycles, release-proof and user-stage (spike/modal-build/mrun; `just bench` does it);
# user-prove starts Modal containers of its own (profile BENCH_MODAL_PROFILE) and needs OUT/stage from user-stage.
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
SPIKE=$(cd "$ROOT/.." && pwd)
step=${1:-}; OUT=${2:-}
[ -n "$step" ] && [ -n "$OUT" ] || { sed -n '2,22p' "$0"; exit 2; }
mkdir -p "$OUT"; OUT=$(cd "$OUT" && pwd)
export TMPDIR=${TMPDIR:-$SPIKE/tmp}
ts() { date -u +%FT%TZ; }

hw_() {
  python3 -I - "$OUT/hardware.json" <<'PY'
import json, os, platform, re, subprocess, sys
def sh(*c):
    try: return subprocess.run(c, capture_output=True, text=True, timeout=30).stdout.strip()
    except Exception as e: return f"unavailable: {e}"
def read(p):
    try: return open(p).read().strip()
    except OSError: return None
cpu = read("/proc/cpuinfo") or ""
models = sorted(set(re.findall(r"model name\s*:\s*(.*)", cpu)))
flags = set((re.search(r"flags\s*:\s*(.*)", cpu) or [None, ""])[1].split())
mem = {l.split(":")[0]: l.split(":")[1].strip() for l in (read("/proc/meminfo") or "").splitlines()[:3]}
h = {
    "measured_utc": sh("date", "-u", "+%FT%TZ"),
    "where": "Modal build box (spike/modal-build/mrun)" if os.environ.get("MODAL_TASK_ID") else platform.node() and "local",
    "cpu_models": models, "logical_cpus_visible": os.cpu_count(), "nproc": sh("nproc"),
    "sched_affinity": len(os.sched_getaffinity(0)),
    "cgroup_cpu_max": read("/sys/fs/cgroup/cpu.max"), "cgroup_cfs_quota_us": read("/sys/fs/cgroup/cpu/cpu.cfs_quota_us"),
    "cgroup_memory_max": read("/sys/fs/cgroup/memory.max") or read("/sys/fs/cgroup/memory/memory.limit_in_bytes"),
    "meminfo": mem, "flags_of_interest": sorted(flags & {"avx2", "avx512f", "sha_ni", "adx", "bmi2"}),
    "kernel": platform.release(), "machine": platform.machine(),
    "modal": {k: os.environ[k] for k in ("MODAL_REGION", "MODAL_CLOUD_PROVIDER", "MODAL_TASK_ID") if k in os.environ},
    "toolchains": {"rustc": sh("rustc", "--version"), "r0vm": sh("r0vm", "--version"),
                   "forge": sh("forge", "--version").splitlines()[0] if sh("forge", "--version") else None,
                   "cargo-risczero": sh("cargo", "risczero", "--version")},
}
json.dump(h, open(sys.argv[1], "w"), indent=1)
print(json.dumps({k: h[k] for k in ("cpu_models", "logical_cpus_visible", "nproc", "cgroup_cpu_max", "cgroup_memory_max")}))
PY
}

gas_() {
  mkdir -p "$OUT/gas"
  ( cd "$ROOT/vault" && forge test --isolate --match-contract GasScenarios -vv > "$OUT/gas/gas-scenarios.log" 2>&1 )
  echo "gas exit $? $(grep -E 'Suite result|passed|failed' "$OUT/gas/gas-scenarios.log" | tail -1)"
}

cycles_() {
  mkdir -p "$OUT/cycles"
  ( cd "$ROOT/bench" && /usr/bin/time -v ./run.sh blst-accel --modes onchain,single-onchain,coresplit,steps \
      > "$OUT/cycles/run.log" 2>&1 )
  local rc=$?
  cp "$ROOT"/bench/results/blst-accel.*.txt "$ROOT/bench/results/PROVENANCE.txt" "$OUT/cycles/" 2>/dev/null
  python3 -I "$ROOT/bench/tools/table.py" "$OUT/cycles" --headline > "$OUT/cycles/headline.md" 2>"$OUT/cycles/table.err"
  echo "cycles exit $rc"
}

release_proof_() {
  # Logs and records go to OUT/release-proof; the program, witness and receipt (binary) to a work directory outside
  # the repository (BENCH_WORK, default $TMPDIR/bench-work).
  local o=$OUT/release-proof d=${BENCH_WORK:-$TMPDIR/bench-work}/release-proof R=$ROOT/e2e/runs/crown-rehearsal-3
  mkdir -p "$o" "$d"
  # The release program: the release build's (crown build path, the crown's image id) if there is one, else built here.
  local E=${FERRATA_E2E:-} P=${FERRATA_RELEASE_BIN:-}
  if [ -z "$E" ]; then
    ( cd "$ROOT/bridge" && cargo build --release --locked -p methods -p ferrata-e2e > "$o/build.log" 2>&1 ) ||
      { echo "release-proof: build failed"; tail -n 20 "$o/build.log"; return 1; }
    E=$ROOT/bridge/target/release/ferrata-e2e
  fi
  if [ -z "$P" ]; then
    local elf
    elf=$(find "$ROOT/bridge/target/riscv-guest" -path '*release*' -name release -type f 2>/dev/null | grep -v '/deps/\|/build/' | head -n 1)
    [ -n "$elf" ] || { echo "release-proof: no release guest ELF"; return 1; }
    "$E" program-binary "$elf" "$d/release.bin" > "$o/image-id.json" || return 1
    P=$d/release.bin
  else
    cp "$P" "$d/release.bin"; "$E" image-id "$d/release.bin" > "$o/image-id.json"
  fi
  local id
  id=$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["image_id"])' "$o/image-id.json")
  "$E" crown-release --identity "$R/identity.json" --burns "$R/checkpoint/input/burns.json" \
    --notes "$R/lez/session-b/burn-notes.json" --out-dir "$d/witness" > "$o/witness.log" 2>&1 ||
    { echo "release-proof: witness failed"; tail -n 5 "$o/witness.log"; return 1; }
  local w=$d/witness/release-private-1
  RISC0_INFO=1 RUST_LOG=info "$E" exec --program "$d/release.bin" --input "$w.input" --meta "$w.json" > "$o/exec.log" 2>&1
  echo "exec exit $?: $(grep -E '^\{' "$o/exec.log" | tail -1 | cut -c1-300)"
  RISC0_DEV_MODE=0 RISC0_INFO=1 RUST_LOG=info /usr/bin/time -v "$E" prove --program "$d/release.bin" --input "$w.input" \
    --meta "$w.json" --out-dir "$d/prove" > "$o/prove.log" 2>&1
  local rc=$?
  echo "prove exit $rc: $(grep -E '^\{' "$o/prove.log" | tail -1 | cut -c1-300)"
  grep -E 'Elapsed \(wall|Maximum resident|Percent of CPU' "$o/prove.log"
  if [ $rc = 0 ]; then
    RISC0_DEV_MODE=0 "$E" verify --receipt "$d/prove/receipt-succinct.bin" --image-id "$id" --meta "$w.json" \
      > "$o/verify.log" 2>&1
    echo "verify exit $? (image id $id); receipt $(stat -c %s "$d/prove/receipt-succinct.bin") bytes, sha256 $(sha256sum "$d/prove/receipt-succinct.bin" | cut -c1-64)"
  fi
  return $rc
}

user_stage_() {
  local S=$OUT/stage
  mkdir -p "$S/stripped" "$S/bundles/r0vm"
  ( cd "$ROOT/lez" && RISC0_HOME=$SPIKE/risc0-home-lez RISC0_BUILD_LOCKED=1 /usr/bin/time -v \
      cargo build -p harness --locked --bin prove_attest --bin prove_bridge > "$S/build.log" 2>&1 ) ||
    { echo "user-stage: build failed"; grep -E '^error' -A8 "$S/build.log" | head -30; return 1; }
  grep -E 'Elapsed \(wall' "$S/build.log"
  local b
  for b in prove_attest prove_bridge; do strip -o "$S/stripped/$b" "$ROOT/lez/target/debug/$b"; done
  ( cd "$ROOT/lez" && FERRATA_HEAD=$(git -C "$ROOT" rev-parse HEAD 2>/dev/null) \
      scripts/stage-prove-attest.sh "$S/bundles/attest-pad" "$S/stripped/prove_attest" > "$S/stage-attest.log" 2>&1 &&
    scripts/stage-prove-bridge.sh "$S/bundles/bridge-pad" "$S/stripped/prove_bridge" > "$S/stage-bridge.log" 2>&1 ) ||
    { echo "user-stage: staging failed"; tail -n 5 "$S"/stage-*.log; return 1; }
  cp "$(readlink -f "$(command -v r0vm)")" "$S/bundles/r0vm/r0vm"
  "$S/bundles/attest-pad/prove_attest" --image-ids | head -2
  du -sh "$S/bundles"/*
}

user_prove_() {
  local P=${BENCH_MODAL_PROFILE:?BENCH_MODAL_PROFILE: the Modal workspace for the proving containers}
  local tag=${BENCH_TAG:-bench-$(basename "$OUT")} cpus=${BENCH_CPUS:-16} vol=${BENCH_VOLUME:-p4-minimum}
  local B=$OUT/stage/bundles M=$ROOT/lez/results/p4-minimum
  [ -d "$B/attest-pad" ] || { echo "user-prove: no $B (run user-stage on the box and fetch it)"; return 1; }
  mkdir -p "$OUT/user"
  export MODAL_PROFILE=$P P4_VOLUME=$vol P4_CPU=$cpus P4_MEM_MIB=16384 P4_APP=ferrata-bench
  local j
  for j in attest-pad bridge-pad r0vm; do modal volume put -f "$vol" "$B/$j" "/bundle/$j" > /dev/null || return 1; done
  ( cd "$M" && modal run minimum.py::main --job attest-pad --runs 1 --tag "$tag" > "$OUT/user/attest-pad.log" 2>&1 ) & local a=$!
  ( cd "$M" && modal run minimum.py::main --job bridge-pad --runs 1 --tag "$tag" > "$OUT/user/bridge-pad.log" 2>&1 ) & local c=$!
  wait $a; echo "attest-pad exit $?"; wait $c; echo "bridge-pad exit $?"
  modal volume get "$vol" "/results/$tag" "$OUT/user/" > /dev/null 2>&1; echo "results in $OUT/user/$tag"
}

report_() { python3 -I "$ROOT/scripts/bench_report.py" "$OUT"; }

case "$step" in
  hw) hw_ ;;
  gas) gas_ ;;
  cycles) cycles_ ;;
  release-proof) release_proof_ ;;
  user-stage) user_stage_ ;;
  user-prove) user_prove_ ;;
  report) report_ ;;
  box) hw_; gas_; release_proof_; cycles_ ;;  # the build box's part of `just bench`, in one call
  *) sed -n '2,22p' "$0"; exit 2 ;;
esac
