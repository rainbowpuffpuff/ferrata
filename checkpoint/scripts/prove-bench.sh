#!/usr/bin/env bash
# Measured proving of one fixture. Writes results/logs/<UTC time>-<step>-<name>.log with the
# machine, the load average before and after, the host's output and GNU time's report (its
# "Maximum resident set size" covers the host and the r0vm child it waits for). During `groth16`
# the Docker prover container is polled every 0.2 s for its cgroup memory.peak and the largest
# VmHWM of its processes (lower bounds, within one poll). Adapted from bridge/scripts/prove-bench.sh.
#
# usage: scripts/prove-bench.sh <prove|groth16|compose> <name|--real>
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
export TMPDIR="$here/../../tmp"         # tmp/ beside the repository, as bench/ uses; not /tmp
export CARGO_TARGET_DIR="$here/target"
unset RISC0_DEV_MODE
mkdir -p "$TMPDIR" "$here/results/logs"
IMAGE=risczero/risc0-groth16-prover:v2025-04-03.1
step="${1:?step}"; name="${2:?name}"
log="$here/results/logs/$(date -u +%Y%m%dT%H%M%SZ)-$step-${name#--}.log"

machine() {
  echo "date            $(date -Is)"
  echo "host            $(uname -srm)"
  echo "cpu             $(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2 | sed 's/^ //'), $(nproc) threads"
  echo "memory          $(free -g | awk '/Mem:/ {print $2 " GiB total, " $7 " GiB available"}')"
  echo "r0vm            $(r0vm --version)"
  echo "docker image    $(docker image inspect "$IMAGE" --format '{{index .RepoDigests 0}}' 2>/dev/null || echo absent)"
  echo "RAYON_NUM_THREADS ${RAYON_NUM_THREADS:-unset (all hardware threads)}"
  echo "loadavg         $(cat /proc/loadavg)"
}

monitor() {
  local out="$1"
  while [ ! -e "$out.stop" ]; do
    for id in $(docker ps -q --no-trunc --filter "ancestor=$IMAGE"); do
      cg="/sys/fs/cgroup/system.slice/docker-$id.scope"
      peak=$(cat "$cg/memory.peak" 2>/dev/null || echo 0)
      hwm=0; pname=-
      for pid in $(cat "$cg/cgroup.procs" 2>/dev/null); do
        v=$(awk '/^VmHWM:/ {print $2}' "/proc/$pid/status" 2>/dev/null || true)
        if [ "${v:-0}" -gt "$hwm" ]; then hwm=$v; pname=$(awk '/^Name:/ {print $2}' "/proc/$pid/status" 2>/dev/null || echo -); fi
      done
      echo "$(date +%s.%N) ${id:0:12} $peak $hwm $pname" >> "$out"
    done
    sleep 0.2
  done
}

{
  echo "== $step $name"
  machine
  if [ "$step" = groth16 ]; then
    work="$here/artifacts/work/$name"; rm -rf "$work" 2>/dev/null || true; mkdir -p "$work"
    export RISC0_WORK_DIR="$work"
    mon="$here/artifacts/work/$name.monitor"; rm -f "$mon" "$mon.stop"
    monitor "$mon" & mpid=$!
  fi
  set +e
  /usr/bin/time -v "$here/target/release/host" "$step" "$name" 2>&1
  status=$?
  set -e
  if [ "$step" = groth16 ]; then
    touch "$mon.stop"; wait "$mpid" || true
    if [ -s "$mon" ]; then
      awk 'NR==1 {t0=$1} {t1=$1; if ($3 > peak) peak=$3; if ($4 > hwm) {hwm=$4; n=$5}} END {printf "docker container: seen for %.1f s (first..last poll); cgroup memory.peak (max over polls) %.2f GiB (incl. page cache); largest process peak RSS (VmHWM) %.2f GiB (%s)\n", t1-t0, peak/1073741824, hwm/1048576, n}' "$mon"
    else
      echo "docker container: never seen by the monitor"
    fi
  fi
  echo "loadavg after   $(cat /proc/loadavg)"
  exit $status
} 2>&1 | tee "$log"
echo "log: $log"
