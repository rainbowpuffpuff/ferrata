#!/usr/bin/env bash
# The day-one deployment rehearsed on our own LEZ v0.4.0-rc3 zone as if it were the public network (results in
# results/rehearsal-1/README.md). Run as lez/live/run.sh's hook (FERRATA_LIVE_PHASES=hook, FERRATA_LIVE_HOOK=this
# file), which starts rc3's sequencer and its Logos Blockchain node 0.3.5-rc.1 and gives the zone in ZONE_* variables:
#
#   1. the zone's RPC under this machine's non-loopback address (forward.py), the URL day1.sh is given;
#   2. day1.sh discover, plan (fresh keys);
#   3. the stand-in for the owner's funding: LEZ's published local test account pays the payer and the daemon key
#      (`ferrata-day1 fund`; on the public network: a deposit through the zone's bridge, NOTES.md);
#   4. day1.sh run (FERRATA_TESTNET_SEND=yes; the bridge stage on), with the sequencer stopped and started again while
#      the attestation program's segments are being deployed;
#   5. the daemon left following Sepolia for REHEARSAL_FOLLOW_MIN minutes from its start (default 35), its /health read
#      every minute, the sequencer stopped and started again half-way;
#   6. day1.sh verify; the daemon stopped; the records copied to REHEARSAL_OUT.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT="${REHEARSAL_OUT:?REHEARSAL_OUT: where the records go}"
FOLLOW_MIN="${REHEARSAL_FOLLOW_MIN:-35}"
: "${ZONE_RPC:?run as the hook of lez/live/run.sh}" "${ZONE_RUN_DIR:?}" "${ZONE_BIN:?}" "${ZONE_LOG:?}"
mkdir -p "$OUT"
EV="$OUT/events.tsv"; [ -f "$EV" ] || printf 'utc\tevent\tdetail\n' > "$EV"
ev() { printf '%s\t%s\t%s\n' "$(date -u +%FT%TZ)" "$1" "${2:-}" | tee -a "$EV"; }

RPC_PORT="${ZONE_RPC##*:}"
IP="$(python3 -I -c 'import socket; s=socket.socket(socket.AF_INET, socket.SOCK_DGRAM); s.connect(("10.255.255.255", 1)); print(s.getsockname()[0])' 2>/dev/null || true)"
case "$IP" in ""|127.*) IP=127.0.0.2 ;; esac
PORT=$(( RPC_PORT + 1 ))
python3 -I "$HERE/forward.py" "$IP" "$PORT" 127.0.0.1 "$RPC_PORT" > "$OUT/forward.log" 2>&1 &
FWD=$!
URL="http://$IP:$PORT"
sleep 1
ev url "$URL (forward.py to the zone's $ZONE_RPC)"

seq_pid() { ps -eo pid=,args= | awk -v d="$ZONE_RUN_DIR/sequencer_home" '$0 ~ "sequencer_service start " d { print $1; exit }'; }
head_block() { curl -sS -m 5 -H 'Content-Type: application/json' --data '{"jsonrpc":"2.0","id":1,"method":"getLastBlockId","params":[]}' "$ZONE_RPC" 2>/dev/null | sed -n 's/.*"result":\([0-9]*\).*/\1/p'; }
restart_sequencer() { # SIGTERM (as an operator's restart), down for $1 s, started again on the same home and port
  local pid; pid="$(seq_pid)"
  ev sequencer-stop "pid $pid, head $(head_block)"
  kill -TERM "$pid"; for _ in $(seq 1 80); do kill -0 "$pid" 2>/dev/null || break; sleep 0.5; done
  kill -0 "$pid" 2>/dev/null && kill -KILL "$pid"
  sleep "${1:-20}"
  echo "== restarted by rehearse.sh at $(date -u +%FT%TZ)" >> "$ZONE_LOG/sequencer.log"
  env -u RISC0_DEV_MODE RUST_LOG="${FERRATA_LIVE_RUST_LOG:-info,sequencer_core=debug,risc0_zkvm=warn,kameo=warn}" \
    "$ZONE_BIN/sequencer_service" start "$ZONE_RUN_DIR/sequencer_home" --rpc-listen-address "127.0.0.1:$RPC_PORT" \
    >> "$ZONE_LOG/sequencer.log" 2>&1 &
  local new=$!
  echo "$new" >> "$FERRATA_LIVE_TMP/run.pids"
  for _ in $(seq 1 120); do [ -n "$(head_block)" ] && break; sleep 1; done
  ev sequencer-start "pid $new, head $(head_block)"
}

D="$HERE/day1.sh"
export FERRATA_DAY1_BRIDGE=1
ev discover; "$D" discover "$URL" > "$OUT/discover.txt" 2>&1; ev discover-exit "$?"
ev plan; "$D" plan "$URL" > "$OUT/plan.log" 2>&1; ev plan-exit "$?"
STATE="${FERRATA_DAY1_STATE:?}"; KEYS="${FERRATA_DAY1_KEYS:?}"
ev fund "stand-in: LEZ's published local test account pays the payer and the daemon key"
FERRATA_DAY1_REHEARSAL=yes FERRATA_TESTNET_SEND=yes FERRATA_SEQUENCER_REMOTE="$URL" \
  "${FERRATA_DAY1_BIN:?}/ferrata-day1" fund --keys "$KEYS" --state "$STATE" --amount "${REHEARSAL_FUND:-50000000000}" --rpc "$URL" \
  > "$OUT/fund.log" 2>&1; ev fund-exit "$?"

ev run "day1.sh run (FERRATA_TESTNET_SEND=yes), in the background"
FERRATA_TESTNET_SEND=yes "$D" run "$URL" > "$OUT/run.log" 2>&1 &
RUN=$!
# The sequencer stopped and started again while the program's segments are deployed (after the third one settled).
for _ in $(seq 1 600); do [ -f "$STATE/tx/deploy-program/02.result.json" ] && break; kill -0 "$RUN" 2>/dev/null || break; sleep 1; done
if kill -0 "$RUN" 2>/dev/null; then restart_sequencer 20; else ev restart-skipped "day1.sh run had ended"; fi
wait "$RUN"; ev run-exit "$?"
"$D" status "$URL" > "$OUT/status-after-run.txt" 2>&1

# The daemon follows Sepolia; /health every minute; the sequencer restarted again half-way.
DSTART="$(stat -c %Y "$STATE/daemon/pid" 2>/dev/null || date +%s)"
ev follow "until $FOLLOW_MIN min after the daemon's start ($(date -u -d "@$DSTART" +%T))"
H="$OUT/health.tsv"; printf 'utc\thttp\thealth\n' > "$H"
restarted=0
while [ $(( $(date +%s) - DSTART )) -lt $(( FOLLOW_MIN * 60 )) ]; do
  body="$(curl -sS -m 5 -w '\t%{http_code}' http://127.0.0.1:${FERRATA_DAY1_DAEMON_PORT:-9464}/health 2>/dev/null | tr -d '\n')"
  printf '%s\t%s\t%s\n' "$(date -u +%FT%TZ)" "${body##*$'\t'}" "${body%$'\t'*}" >> "$H"
  if [ "$restarted" = 0 ] && [ $(( $(date +%s) - DSTART )) -ge $(( FOLLOW_MIN * 30 )) ]; then restart_sequencer 30; restarted=1; fi
  sleep 60
done
"$D" daemon-status "$URL" > "$OUT/daemon-status-end.txt" 2>&1
ev verify; FERRATA_DAY1_MIN_APPLIED="${REHEARSAL_MIN_APPLIED:-3}" "$D" verify "$URL" > "$OUT/verify.txt" 2>&1; ev verify-exit "$?"
"$D" daemon-stop "$URL" >> "$OUT/daemon-status-end.txt" 2>&1; ev daemon-stopped
kill "$FWD" 2>/dev/null
ev done
