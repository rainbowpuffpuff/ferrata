#!/usr/bin/env bash
# Measured proving steps for the release guest. Each step writes artifacts/logs/<UTC time>-<step>-<case>.log
# with the machine, the load average before and after, the host's output, and GNU time's report
# (its "Maximum resident set size" covers the host and the r0vm child it waits for).
# During `groth16`, the Docker prover container is polled every 0.2 s for its cgroup memory.peak;
# the last value read and the first/last time it was seen are logged (lower bounds, within one poll).
#
# usage: scripts/prove-bench.sh <prove|p254|groth16> <eth|erc20>
#        scripts/prove-bench.sh env
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
export TMPDIR="$here/../tmp"            # spike/tmp: never /tmp on this machine (RAM-backed)
export CARGO_TARGET_DIR="$here/target"
unset RISC0_DEV_MODE
mkdir -p "$TMPDIR" "$here/artifacts/logs"
IMAGE=risczero/risc0-groth16-prover:v2025-04-03.1

machine() {
  echo "date            $(date -Is)"
  echo "host            $(uname -srm)"
  echo "cpu             $(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2 | sed 's/^ //'), $(nproc) threads"
  echo "memory          $(free -g | awk '/Mem:/ {print $2 " GiB total, " $7 " GiB available"}')"
  echo "r0vm            $(r0vm --version)"
  echo "docker image    $(docker image inspect "$IMAGE" --format '{{index .RepoDigests 0}}' 2>/dev/null || echo absent)"
  echo "loadavg         $(cat /proc/loadavg)"
}

step="${1:?step}"
if [ "$step" = env ]; then machine; exit 0; fi
case_name="${2:?case}"
log="$here/artifacts/logs/$(date -u +%Y%m%dT%H%M%SZ)-$step-$case_name.log"

monitor() {
  local out="$1"
  while [ ! -e "$out.stop" ]; do
    for id in $(docker ps -q --no-trunc --filter "ancestor=$IMAGE"); do
      f="/sys/fs/cgroup/system.slice/docker-$id.scope/memory.peak"
      [ -r "$f" ] && echo "$(date +%s.%N) ${id:0:12} $(cat "$f" 2>/dev/null || echo gone)" >> "$out"
    done
    sleep 0.2
  done
}

{
  echo "== $step $case_name"
  machine
  if [ "$step" = groth16 ]; then
    work="$here/artifacts/work/$case_name"; rm -rf "$work"; mkdir -p "$work"
    export RISC0_WORK_DIR="$work"         # the Docker prover's /mnt; scratch, git-ignored
    mon="$here/artifacts/work/$case_name.monitor"; rm -f "$mon" "$mon.stop"
    monitor "$mon" & mpid=$!
  fi
  /usr/bin/time -v "$here/target/release/host" "$step" "$case_name" 2>&1
  status=$?
  if [ "$step" = groth16 ]; then
    touch "$mon.stop"; wait "$mpid" || true
    if [ -s "$mon" ]; then
      awk 'NR==1 {t0=$1} {t1=$1; peak=$3} END {printf "docker container: seen for %.1f s (first..last poll), memory.peak %.2f GiB (%d bytes)\n", t1-t0, peak/1073741824, peak}' "$mon"
    else
      echo "docker container: never seen by the monitor"
    fi
    echo "work dir after the run:"; ls -la "$work"
  fi
  echo "loadavg after   $(cat /proc/loadavg)"
  exit $status
} 2>&1 | tee "$log"
echo "log: $log"
