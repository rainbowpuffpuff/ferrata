#!/usr/bin/env bash
# The hook of bench/perf/gate-sequencer.sh, run by lez/crown/session.sh x while a LEZ v0.4.0-rc1 zone runs (sequencer +
# Logos Blockchain node, LEZ's local configuration). Three ferrata-updater daemons (the build of lez/updater/sequencer)
# follow the lab's recorded mainnet chain (lez/lab/results/record/net/mainnet) on this zone's sequencer, each with its
# own payer (LEZ's published test keys of the local network's genesis accounts), its own beacon port and its own
# counting proxy to the sequencer's RPC: honest-1 and honest-2 with the deferral window GATE_WINDOW (blocks), and, unless
# GATE_ADVERSARY=0, an adversary that never defers, waits for no other job (join_wait_rounds = 0) and polls twice as
# often (GATE_ADV_POLL). GATE_POLLS sets the honest daemons' polls apart; GATE_COPIES=1 has honest-2's beacon port serve
# copies (beacon_replay.py --copies). bench/perf/beacon_replay.py reveals one recorded update every GATE_STEP seconds. Every daemon runs under GNU time and is sampled by proc_sample.py.
# Records go to $GATE_OUT. Environment from session.sh x: RPC_PORT, ZONE_LOG, ZONE_VOL, ZONE_WORK, ZONE_BIN.
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
OUT="${GATE_OUT:?}"; BIN="${GATE_UPDATER_BIN:?}"
ENTRIES="${GATE_ENTRIES:-24}"; STEP="${GATE_STEP:-150}"; WINDOW="${GATE_WINDOW:-8}"; POLL="${GATE_POLL:-4}"
POLLS="${GATE_POLLS:-$POLL,$POLL}"; POLL1="${POLLS%%,*}"; POLL2="${POLLS#*,}"; COPIES="${GATE_COPIES:-0}"
ADV="${GATE_ADVERSARY:-1}"; ADV_POLL="${GATE_ADV_POLL:-$(( POLL > 1 ? POLL / 2 : 1 ))}"; DEADLINE_MIN="${GATE_DEADLINE_MIN:-150}"
REC="$REPO/lez/lab/results/record/net/mainnet"
CHECKPOINT="$(tr -d ' \n' < "$REC/checkpoint")"
say() { echo "[$(date -u +%H:%M:%S)] $*"; }
mkdir -p "$OUT"
cp "$BIN" "$ZONE_WORK/ferrata-updater" && chmod +x "$ZONE_WORK/ferrata-updater"
say "updater sha256 $(sha256sum "$ZONE_WORK/ferrata-updater" | cut -c1-64); entries $ENTRIES, step ${STEP}s, window $WINDOW, poll ${POLL}s, polls ${POLLS}s, copies $COPIES, adversary $ADV, adversary poll ${ADV_POLL}s"
PROGRAM="$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["attestation"]["account"])' "$ZONE_VOL/session-a/session-a.json")"
say "attestation program $PROGRAM; sequencer 127.0.0.1:$RPC_PORT"

# Daemons: name, payer key (LEZ's published test keys: testnet_initial_state's two accounts and local.json's first),
# deferral window, poll seconds, beacon port, proxy port, metrics port.
names=(honest-1 honest-2); keys=(10a26a9aec7d34b82364eeae45c5294dbb0a764b000b94eeb9b58511dc487c4d 717940b1cc55e5d6b2066dbf1d9a3f26f212f4db08d02388177fcfedd8a9be1b)
windows=("$WINDOW" "$WINDOW"); polls=("$POLL1" "$POLL2"); joins=("" "")
if [ "$ADV" = 1 ]; then
  # The adversary as A3's: it never defers (window 1) and waits for no other party's job (join or wait off).
  names+=(adversary); keys+=(7f273098f25b71e6c005a9519f2678da8d1c7f01f6a27778e2d9948abdf901fb)
  windows+=(1); polls+=("$ADV_POLL"); joins+=("join_wait_rounds = 0")
fi
n=${#names[@]}
ports=""; for i in $(seq 0 $((n - 1))); do ports="$ports${ports:+,}$((9601 + i))"; done
copy_args=(); if [ "$COPIES" = 1 ]; then copy_args=(--copies 9602); fi

python3 -I "$HERE/beacon_replay.py" --dir "$REC" --ports "$ports" "${copy_args[@]}" --start 1 --step "$STEP" --last "$ENTRIES" \
  --reveals "$OUT/reveals.tsv" --stats "$OUT/beacon-stats.json" > "$OUT/beacon.log" 2>&1 &
BEACON=$!
pids=(); proxies=(); samp=()
for i in $(seq 0 $((n - 1))); do
  name=${names[$i]}; D="$ZONE_WORK/gate/$name"; mkdir -p "$D"
  python3 -I "$HERE/tcp_count.py" --listen $((13041 + i)) --to "127.0.0.1:$RPC_PORT" --stats "$OUT/rpc-$name.json" &
  proxies+=($!)
  printf '%s\n' "${keys[$i]}" > "$D/payer.key"
  printf '%s\n' "# bench/perf/gate-hook.sh: $name" \
    '[daemon]' "name = \"$name\"" 'state_dir = "state"' "poll_secs = ${polls[$i]}" "listen = \"127.0.0.1:$((9701 + i))\"" 'ledger = "sequencer"' '' \
    '[sequencer]' "rpc = \"http://127.0.0.1:$((13041 + i))\"" "program = \"$PROGRAM\"" 'wait_final = true' "log = \"$ZONE_LOG/sequencer.log\"" '' \
    '[keys]' 'payer_key_file = "payer.key"' '' \
    '[fees]' 'gas_limit = 4800000' 'max_fee_per_tx = 1000000000' 'budget = 1000000000000' '' \
    '[policy]' "defer_window = ${windows[$i]}" 'max_lag_slots = 64' 'min_agreeing = 1' 'retry_base_ms = 1000' 'retry_max_ms = 60000' \
    'max_attempts = 5' 'demote_rounds = 50' 'max_periods_per_request = 4' 'finish_own_jobs = true' ${joins[$i]:+"${joins[$i]}"} '' \
    '[[chain]]' 'name = "mainnet"' 'preset = "mainnet"' "checkpoint = \"$CHECKPOINT\"" 'adoption_parts = 2' '' \
    '  [[chain.source]]' '  name = "replay"' "  url = \"http://127.0.0.1:$((9601 + i))\"" '  timeout_secs = 30' > "$D/updater.toml"
  sed -e "s#$ZONE_LOG/#work/logs/#" "$D/updater.toml" > "$OUT/updater-$name.toml"
done
sleep 2
for i in $(seq 0 $((n - 1))); do
  name=${names[$i]}; D="$ZONE_WORK/gate/$name"
  ( cd "$D" && exec /usr/bin/time -v -o "$OUT/time-$name.txt" "$ZONE_WORK/ferrata-updater" --config "$D/updater.toml" ) > "$OUT/daemon-$name.log" 2>&1 &
  pids+=($!)
done
sleep 3
sargs=(); for i in $(seq 0 $((n - 1))); do c="$(pgrep -P "${pids[$i]}" | head -1)"; sargs+=(--pid "${names[$i]}=${c:-${pids[$i]}}"); done
python3 -I "$HERE/proc_sample.py" "${sargs[@]}" --out "$OUT/samples.tsv" --every 5 &
SAMPLER=$!
say "daemons ${pids[*]}; sampler ${sargs[*]}"

# Done when every entry has been revealed and the newest revealed finalized slot is applied by someone, or at the deadline.
t0=$(date +%s); done=0; tick=0
while [ $(( $(date +%s) - t0 )) -lt $(( DEADLINE_MIN * 60 )) ]; do
  sleep 30
  # Every minute, each daemon's /health and /metrics as it serves them (what it waits for, its counters).
  tick=$((tick + 1))
  if [ $((tick % 2)) = 0 ]; then
    for i in $(seq 0 $((n - 1))); do
      now=$(date +%s)
      h="$(curl -fsS -m 5 "http://127.0.0.1:$((9701 + i))/health" 2>/dev/null | tr -d '\n')"
      echo "{\"t\": $now, \"health\": ${h:-null}}" >> "$OUT/health-${names[$i]}.jsonl"
      { echo "# t=$now"; curl -fsS -m 5 "http://127.0.0.1:$((9701 + i))/metrics" 2>/dev/null | grep -v '^#'; } >> "$OUT/metrics-series-${names[$i]}.txt"
    done
  fi
  revealed=$(( $(wc -l < "$OUT/reveals.tsv") - 1 ))
  want="$(awk -F'\t' -v n="$ENTRIES" 'NR == n + 1 { print $5 }' "$OUT/reveals.tsv")"
  got="$(cat "$ZONE_WORK"/gate/*/state/*/journal.tsv 2>/dev/null | awk -F'\t' '$2 == "finish" && $5 == "applied" { if ($4 > m) m = $4 } END { print m + 0 }')"
  say "revealed $revealed/$ENTRIES; newest applied finalized slot $got (want ${want:-?})"
  if [ -n "$want" ] && [ "$got" -ge "$want" ]; then done=1; sleep $(( 4 * POLL + 20 )); break; fi
done
say "stopping (done=$done)"
for i in $(seq 0 $((n - 1))); do curl -fsS -m 5 "http://127.0.0.1:$((9701 + i))/metrics" > "$OUT/metrics-${names[$i]}.txt" 2>/dev/null || true; done
for i in $(seq 0 $((n - 1))); do c="$(pgrep -P "${pids[$i]}" | head -1)"; [ -n "$c" ] && kill -TERM "$c" 2>/dev/null; done
for p in "${pids[@]}"; do wait "$p" 2>/dev/null; done
kill -TERM "$SAMPLER" "$BEACON" "${proxies[@]}" 2>/dev/null; sleep 2
for name in "${names[@]}"; do
  D="$ZONE_WORK/gate/$name"
  mkdir -p "$OUT/state-$name"
  find "$D/state" -maxdepth 2 -name 'journal.tsv' -exec cp {} "$OUT/state-$name/" \;
  cp "$D/state/ledger/manifest.tsv" "$OUT/state-$name/" 2>/dev/null || true
done
cp "$ZONE_LOG/sequencer.log" "$OUT/sequencer.log" 2>/dev/null; gzip -f "$OUT/sequencer.log" 2>/dev/null
echo "done=$done" > "$OUT/done.txt"
say "records in $OUT"
