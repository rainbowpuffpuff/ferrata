#!/usr/bin/env bash
# p4-laptop, second night: one job of bench/perf/p4-laptop-owner.sh (P4_JOBS=JOB, P4_CPUS=8) started as a transient user
# service, so that the script starts at niceness 0 and its provers run at 10 as it intends (this shell runs at 19), under
# a watchdog with the lead's rule of 25 November 2026, 00:35 CET: stop only if other work uses more than 6 busy threads
# on CPUs 8-15 (the prover is held to CPUs 0-7) for 5 minutes, or the 1-minute load average stays above 24 for 5
# minutes, or MemAvailable drops under 8 GB.
#   bash run-part.sh JOB OUT_DIR                 start the job and watch it
#   bash run-part.sh --attach UNIT JOB OUT_DIR   watch a job already started (if this runner was cut off)
# Files in this directory, named after the job: JOB.owner.log (+ .exit), JOB.watch.log, JOB.verdict.txt.
set -u
attach=""
if [ "${1:-}" = "--attach" ]; then attach=${2:?UNIT}; shift 2; fi
job=${1:?JOB} out=${2:?OUT_DIR}
W=<REPO>
T=<SCRATCH>
log=$T/$job.owner.log watch=$T/$job.watch.log verdict=$T/$job.verdict.txt
mem() { awk '/MemAvailable/ {print $2}' /proc/meminfo; }
stat() { awk '/^cpu / {b=$2+$3+$4+$7+$8+$9; printf "%d %d ", b, b+$5+$6}
              /^cpu([89]|1[0-5]) / {b2+=$2+$3+$4+$7+$8+$9; t2+=$2+$3+$4+$5+$6+$7+$8+$9}
              END {printf "%d %d\n", b2, t2}' /proc/stat; }
if [ -z "$attach" ]; then
  : > "$watch"; : > "$verdict"; rm -f "$log" "$log.exit"
  unit=p4-laptop-$job-$(date +%s)
  echo "$(date +%FT%T) starting $unit: load $(cut -d' ' -f1-3 /proc/loadavg), MemAvailable $(mem) kB" >> "$verdict"
  systemd-run --user --collect --quiet --unit="$unit" --working-directory="$W" \
    --setenv=PATH="$PATH" --setenv=P4_CPUS=8 --setenv=P4_JOBS="$job" \
    bash "$T/part-service.sh" "$T/stage" "$out" "$log" || { echo "systemd-run failed" >> "$verdict"; exit 2; }
  sleep 2
else
  unit=$attach
  echo "$(date +%FT%T) watching $unit again" >> "$verdict"
fi
reason="" hi_other=0 hi_load=0
read -r b0 t0 c0 d0 < <(stat)
while systemctl --user is-active --quiet "$unit"; do
  sleep 5
  read -r b1 t1 c1 d1 < <(stat)
  busy=$(awk -v db=$((b1 - b0)) -v dt=$((t1 - t0)) 'BEGIN {printf "%.2f", dt ? db / dt * 16 : 0}')
  busy2=$(awk -v db=$((c1 - c0)) -v dt=$((d1 - d0)) 'BEGIN {printf "%.2f", dt ? db / dt * 8 : 0}')
  b0=$b1 t0=$t1 c0=$c1 d0=$d1
  l1=$(cut -d' ' -f1 /proc/loadavg) m=$(mem)
  r0=$(ps -C r0vm -o nlwp=,ni= 2> /dev/null | awk '{t+=$1; n=$2} END {printf "%d %s", t, (n == "" ? "-" : n)}')
  echo "$(date +%s) ac=$(cat /sys/class/power_supply/ACAD/online 2> /dev/null) mem_avail_kib=$m load1=$l1 busy_threads=$busy busy_cpu8_15=$busy2 r0vm_threads_nice=${r0// /,}" >> "$watch"
  if [ "$m" -lt 7812500 ]; then reason="MemAvailable $m kB, under 8 GB"; fi
  if awk -v x="$busy2" 'BEGIN {exit !(x > 6)}'; then hi_other=$((hi_other + 1)); else hi_other=0; fi
  if awk -v x="$l1" 'BEGIN {exit !(x > 24)}'; then hi_load=$((hi_load + 1)); else hi_load=0; fi
  [ "$hi_other" -ge 60 ] && reason="other work above 6 busy threads on CPUs 8-15 for 5 minutes (last $busy2)"
  [ "$hi_load" -ge 60 ] && reason="1-minute load average above 24 for 5 minutes (last $l1)"
  if [ -n "$reason" ]; then
    echo "STOPPED $(date +%FT%T): $reason" >> "$verdict"
    systemctl --user stop "$unit"
    break
  fi
done
for i in 1 2 3 4 5 6; do [ -f "$log.exit" ] && break; sleep 5; done
echo "owner exit $(cat "$log.exit" 2> /dev/null || echo none) $(date +%FT%T)" >> "$verdict"
