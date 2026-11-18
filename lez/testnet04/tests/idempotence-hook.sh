#!/usr/bin/env bash
# The idempotence of `day1.sh run` against chain state and the key directory (review-testnet04, finding 3), on our own
# LEZ zone (run as lez/live/run.sh's hook, as ../rehearse.sh): (a) and (b) applied once; then `run` again with the state
# directory lost, with the stored Init and the instance file lost (only the kept checkpoint left), and with another
# spelling of the URL (another state directory): every time nothing new may be signed or sent, and the payer's nonce
# must stay what (a) and (b) made it. Records under REHEARSAL_OUT. No public network.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${REHEARSAL_OUT:?}"; mkdir -p "$OUT"
: "${ZONE_RPC:?run as the hook of lez/live/run.sh}"
EV="$OUT/events.tsv"; printf 'utc\tevent\tdetail\n' > "$EV"
ev() { printf '%s\t%s\t%s\n' "$(date -u +%FT%TZ)" "$1" "${2:-}" | tee -a "$EV"; }
RPC_PORT="${ZONE_RPC##*:}"
IP="$(python3 -I -c 'import socket; s=socket.socket(socket.AF_INET, socket.SOCK_DGRAM); s.connect(("10.255.255.255", 1)); print(s.getsockname()[0])' 2>/dev/null || true)"
case "$IP" in ""|127.*) echo "no non-loopback address on this machine" >&2; exit 1 ;; esac
PORT=$(( RPC_PORT + 1 ))
python3 -I "$HERE/forward.py" "$IP" "$PORT" 127.0.0.1 "$RPC_PORT" > "$OUT/forward.log" 2>&1 &
FWD=$!; sleep 1
URL="http://$IP:$PORT"
D="$HERE/day1.sh"; KEYS="${FERRATA_DAY1_KEYS:?}"; S0="${FERRATA_DAY1_STATE:?}"
unset FERRATA_DAY1_BRIDGE
nonce() { FERRATA_DAY1_STATE="$1" "$D" discover "$2" > /dev/null 2>&1
  python3 -I -c 'import json,sys; d=json.load(open(sys.argv[1])); print(d["payer"]["nonce"])' "$1/inspect-discover.json"; }
run_until_b() { # run_until_b NAME STATE URL
  FERRATA_DAY1_STATE="$2" FERRATA_TESTNET_SEND=yes FERRATA_DAY1_UNTIL=b "$D" run "$3" > "$OUT/$1.log" 2>&1
  local rc=$?; ev "$1-exit" "$rc; $(grep -E '^\[|\(a\)|\(b\)|signed|exists' "$OUT/$1.log" | tr '\n' ' ' | cut -c1-300)"; return $rc; }

ev url "$URL"
FERRATA_DAY1_STATE="$S0" "$D" plan "$URL" > "$OUT/plan.log" 2>&1; ev plan-exit "$?"
FERRATA_DAY1_REHEARSAL=yes FERRATA_TESTNET_SEND=yes FERRATA_SEQUENCER_REMOTE="$URL" \
  "${FERRATA_DAY1_BIN:?}/ferrata-day1" fund --keys "$KEYS" --state "$S0" --amount 50000000000 --rpc "$URL" > "$OUT/fund.log" 2>&1
ev fund-exit "$?"
run_until_b run-1-first "$S0" "$URL"
n1="$(nonce "$S0" "$URL")"; ev nonce-after-first "$n1"
rm -rf "$S0"; ev state-lost "$S0 removed"
run_until_b run-2-state-lost "$S0" "$URL"
n2="$(nonce "$S0" "$URL")"; ev nonce "$n2"
mv "$KEYS/tx/init" "$OUT/init-stage-set-aside"; mv "$KEYS/instance.json" "$OUT/instance-set-aside.json"; rm -rf "$S0"
ev init-lost "KEYS/tx/init and KEYS/instance.json set aside, KEYS/init (the checkpoint) kept; state removed"
run_until_b run-3-init-lost "$S0" "$URL"
n3="$(nonce "$S0" "$URL")"; ev nonce "$n3"
S2="$S0-other-spelling"; rm -rf "$S2"
run_until_b run-4-other-url "$S2" "$URL/"
n4="$(nonce "$S2" "$URL/")"; ev nonce "$n4"
[ -d "$KEYS/tx/init" ] && ev init-resigned "KEYS/tx/init exists again (it must not)" || ev init-not-resigned "no new Init stage in KEYS"
ok=1; for n in "$n2" "$n3" "$n4"; do [ "$n" = "$n1" ] || ok=0; done
[ "$ok" = 1 ] && [ ! -d "$KEYS/tx/init" ] && ev RESULT "pass: the payer's nonce stayed $n1 through three repeated runs; no second Init" || ev RESULT "FAIL: nonces $n1 $n2 $n3 $n4"
kill "$FWD" 2>/dev/null
