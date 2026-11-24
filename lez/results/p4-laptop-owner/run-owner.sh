#!/usr/bin/env bash
# p4-laptop: bench/perf/p4-laptop-owner.sh with P4_CPUS=8 on this laptop, behind a gate and under a watchdog.
#   bash run-owner.sh MAX_WAIT_S
# Gate (checked every 30 s, up to MAX_WAIT_S): on mains power, MemAvailable >= 18 GiB, and the CPU demand (busy hardware
# threads from /proc/stat) at most 3 on average over the last 600 s. Watchdog (every 5 s while the run lasts): stop the
# whole run if MemAvailable < 8 GiB, if the CPU demand stays above 10 threads for 120 s, or if the laptop is on battery
# for 60 s. Files here: gate.log, watch.log (one line per sample), owner.log, verdict.txt.
set -u
W=<REPO>
T=<SCRATCH>
max_wait=${1:?MAX_WAIT_S}
NCPU=$(nproc --all)
ac() { cat /sys/class/power_supply/ACAD/online 2> /dev/null || echo 1; }
mem() { awk '/MemAvailable/ {print $2}' /proc/meminfo; }
# busy and total jiffies: all CPUs, and CPUs 8-15 (the ones the provers are not held to)
stat() { awk '/^cpu / {b=$2+$3+$4+$7+$8+$9; printf "%d %d ", b, b+$5+$6}
              /^cpu([89]|1[0-5]) / {b2+=$2+$3+$4+$7+$8+$9; t2+=$2+$3+$4+$5+$6+$7+$8+$9}
              END {printf "%d %d\n", b2, t2}' /proc/stat; }
: > "$T/verdict.txt"

# the gate
start=$(date +%s); q=()
read -r b0 t0 _ _ < <(stat)
while :; do
  sleep 30
  read -r b1 t1 _ _ < <(stat)
  busy=$(awk -v db=$((b1 - b0)) -v dt=$((t1 - t0)) -v n="$NCPU" 'BEGIN {printf "%.2f", dt ? db / dt * n : 0}')
  b0=$b1 t0=$t1
  q+=("$busy"); [ ${#q[@]} -gt ${GATE_N:-20} ] && q=("${q[@]:1}")
  avg=$(printf '%s\n' "${q[@]}" | awk '{s+=$1} END {printf "%.2f", s / NR}')
  m=$(mem) a=$(ac)
  echo "$(date +%FT%T) ac=$a mem_avail_kib=$m busy_threads_30s=$busy busy_avg_600s=$avg load=$(cut -d' ' -f1-3 /proc/loadavg)" >> "$T/gate.log"
  if [ "$a" = 1 ] && [ "$m" -ge $((18 * 1024 * 1024)) ] && [ ${#q[@]} -ge ${GATE_N:-20} ] && awk -v x="$avg" 'BEGIN {exit !(x <= 3)}'; then
    break
  fi
  if [ $(( $(date +%s) - start )) -ge "$max_wait" ]; then
    echo "NOT STARTED after ${max_wait}s: last gate sample: ac=$a mem_avail_kib=$m busy_avg_600s=$avg" >> "$T/verdict.txt"
    exit 3
  fi
done

echo "$(date +%FT%T) gate passed; starting" >> "$T/gate.log"
rm -rf "$T/out"
cd "$W"
setsid env P4_CPUS=8 P4_JOBS="${P4_JOBS:-}" bash bench/perf/p4-laptop-owner.sh "$T/stage" "$T/out" > "$T/owner.log" 2>&1 &
pid=$!
echo "$pid" > "$T/owner.pid"
sleep 1
pgid=$(ps -o pgid= -p "$pid" | tr -d ' ')
reason=""
hi=0 bat=0
read -r b0 t0 c0 d0 < <(stat)
while kill -0 "$pid" 2> /dev/null; do
  sleep 5
  read -r b1 t1 c1 d1 < <(stat)
  busy=$(awk -v db=$((b1 - b0)) -v dt=$((t1 - t0)) -v n="$NCPU" 'BEGIN {printf "%.2f", dt ? db / dt * n : 0}')
  busy2=$(awk -v db=$((c1 - c0)) -v dt=$((d1 - d0)) 'BEGIN {printf "%.2f", dt ? db / dt * 8 : 0}')
  b0=$b1 t0=$t1 c0=$c1 d0=$d1
  m=$(mem) a=$(ac)
  thr=$(ps -C r0vm -o nlwp= 2> /dev/null | awk '{s+=$1} END {print s+0}')
  echo "$(date +%s) ac=$a mem_avail_kib=$m load1=$(cut -d' ' -f1 /proc/loadavg) busy_threads=$busy busy_cpu8_15=$busy2 r0vm_threads=$thr" >> "$T/watch.log"
  if [ "$m" -lt $((8 * 1024 * 1024)) ]; then reason="MemAvailable $m kB < 8 GiB"; fi
  if awk -v x="$busy" 'BEGIN {exit !(x > 10)}'; then hi=$((hi + 1)); else hi=0; fi
  [ "$hi" -ge 24 ] && reason="CPU demand above 10 hardware threads for 120 s (last $busy)"
  if [ "$a" = 0 ]; then bat=$((bat + 1)); else bat=0; fi
  [ "$bat" -ge 12 ] && reason="on battery for 60 s"
  if [ -n "$reason" ]; then
    echo "STOPPED $(date +%FT%T): $reason" >> "$T/verdict.txt"
    kill -TERM -- "-$pgid" 2> /dev/null; sleep 5; kill -KILL -- "-$pgid" 2> /dev/null
    break
  fi
done
wait "$pid"; rc=$?
echo "owner exit $rc $(date +%FT%T)" >> "$T/verdict.txt"
exit "$rc"
