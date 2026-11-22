#!/usr/bin/env bash
# RFP-021 P.2 with RFP-020's method: a real RISC Zero proof (CPU, r0vm 3.0.5) of every program
# action's guest execution, input for input as LEZ v0.4.0-rc1 runs it (lez/harness/src/bin/perf_prove.rs).
# Each proof is its own process under GNU time, so its peak memory is its own.
#
#   bash bench/perf/prove-actions.sh OUT_DIR [N ...]      # from the repository root, on the build box
#   python3 -I bench/perf/summarize_proofs.py OUT_DIR > lez/results/perf-proofs.md
#
# PERF_DUMP=DIR reuses a dump (inputs, guests, index.tsv); PERF_WITH_VERIFY_ALL=1 adds the finality
# update in one public transaction (VerifyAll, about 9.1M cycles). Finished proofs are skipped on a rerun.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
out="$(realpath -m "${1:?usage: prove-actions.sh OUT_DIR [N ...]}")"
shift
dump="${PERF_DUMP:-$out/dump}"
mkdir -p "$out"
cd "$root/lez"
export RISC0_HOME="${RISC0_HOME:-$root/../risc0-home-lez}"
cargo build -p harness --locked --bin perf_prove > "$out/build.log" 2>&1 || { tail -30 "$out/build.log"; exit 1; }
bin="$root/lez/target/debug/perf_prove"
if [ ! -f "$dump/index.tsv" ]; then
  "$bin" --dump "$dump" ${PERF_WITH_VERIFY_ALL:+--with-verify-all} > "$out/dump.log" 2>&1 || { tail -30 "$out/dump.log"; exit 1; }
fi
cp "$dump/index.tsv" "$out/index.tsv"
r0vm_path="$(command -v r0vm || true)"
{
  echo "date: $(date -u +%FT%TZ)"
  echo "nproc: $(nproc)"
  grep -m1 'model name' /proc/cpuinfo || true
  grep MemTotal /proc/meminfo
  echo "cgroup cpu.max: $(cat /sys/fs/cgroup/cpu.max 2>/dev/null || echo n/a)"
  echo "cgroup memory.max: $(cat /sys/fs/cgroup/memory.max 2>/dev/null || echo n/a)"
  echo "r0vm: $r0vm_path $(r0vm --version 2>/dev/null || true)"
  [ -n "$r0vm_path" ] && sha256sum "$r0vm_path"
  echo "head: ${FERRATA_HEAD:-$(git -C "$root" rev-parse HEAD 2>/dev/null || echo unknown)}"
} > "$out/machine.txt"
if [ "$#" -gt 0 ]; then ns="$*"; else ns="$(tail -n +2 "$dump/index.tsv" | sort -t$'\t' -k9,9n | cut -f1)"; fi
for n in $ns; do
  log="$out/prove-$(printf %03d "$n").log"
  if [ -f "$log" ] && grep -q '^RESULT' "$log"; then continue; fi
  RISC0_DEV_MODE=0 /usr/bin/time -v "$bin" --prove "$dump" "$n" > "$log" 2>&1 || echo "proof $n failed (see $log)"
  grep '^RESULT' "$log" || true
done
