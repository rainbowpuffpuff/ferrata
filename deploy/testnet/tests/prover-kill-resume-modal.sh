#!/usr/bin/env bash
# The bridge's prover client (host/bin/ferrata-bridge-prover, unchanged) against Modal itself, not a stand-in (RFP-022
# R.6: "Temporary RPC or connectivity failure ... leaves any off-chain component in a recoverable state, able to resume
# without duplicating work already done"). Its `watch` loop is given proof requests; as soon as it has spawned the first
# proof on Modal's GPU and recorded the call, it is killed (SIGKILL) and started again: the second client must wait for
# the same Modal call (event "resumed", the same call id) and deliver its job once, never spawning a second proof of
# that request. The other requests then run without a kill. Run from the repository root, with a Modal profile whose
# workspace has the app `ferrata-prover` (gpu-prover/modal_app.py) deployed with bin-31/, and the RISC Zero 3.1
# programs of docs/image-ids.md in FERRATA_PROGRAMS_31:
#
#   MODAL_PROFILE=<workspace> FERRATA_PROGRAMS_31=<dir> deploy/testnet/tests/prover-kill-resume-modal.sh OUT REQ.json...
#
# Each REQ.json is a request as `watch` reads it ({"kind": "checkpoint-b"|"release", "input": FILE, "meta": FILE}); the
# first is the one killed. OUT gets: events.jsonl (both clients' stdout, in order), done/ (the answers), inbox/ (the
# jobs the relayer would send), calls.tsv (every request: its call id when spawned and when answered, attempts,
# resumed) and summary.txt; it prints PROVER KILL-RESUME OK when every request was answered ok, the killed one by the
# same call it spawned, each job written once. Nothing here is sent to any chain; Modal bills the proofs (about one
# minute of an L4 each).
set -euo pipefail
OUT="$1"; shift
[ "$#" -ge 1 ] || { sed -n 2,20p "$0"; exit 2; }
: "${MODAL_PROFILE:?MODAL_PROFILE names the workspace with the ferrata-prover app}" "${FERRATA_PROGRAMS_31:?the RISC Zero 3.1 programs}"
CLIENT="$(cd "$(dirname "$0")/.." && pwd)/host/bin/ferrata-bridge-prover"
PY="${FERRATA_PROVER_PYTHON:-python3}"
mkdir -p "$OUT"; OUT="$(cd "$OUT" && pwd)"
R="$OUT/requests"; rm -rf "$R" "$OUT/inbox" "$OUT/done"; mkdir -p "$R" "$OUT/inbox"
first=""
for q in "$@"; do
  n="$(basename "$q" .json)"; [ -n "$first" ] || first="$n"
  cp "$q" "$R/$n.json"
done
: > "$OUT/events.jsonl"
say() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$OUT/summary.txt"; }
: > "$OUT/summary.txt"
start() { "$PY" "$CLIENT" watch --requests "$R" --inbox "$OUT/inbox" --poll 5 >> "$OUT/events.jsonl" 2> "$OUT/client-$1.err" & echo $!; }

say "client 1 started on $# requests (Modal workspace from MODAL_PROFILE; the first, $first, is killed once it is spawned)"
p1="$(start 1)"
for _ in $(seq 1 600); do
  [ -s "$R/calls/$first.json" ] && jq -e .call_id "$R/calls/$first.json" > /dev/null 2>&1 && break
  kill -0 "$p1" 2>/dev/null || { cat "$OUT/client-1.err" >&2; say "STOP: client 1 ended before spawning"; exit 1; }
  sleep 1
done
spawned="$(jq -r .call_id "$R/calls/$first.json")"
[ -n "$spawned" ] && [ "$spawned" != null ] || { say "STOP: no call recorded for $first"; exit 1; }
sleep "${FERRATA_KILL_AFTER_S:-5}"
kill -KILL "$p1"; wait "$p1" 2>/dev/null || true
say "client 1 killed (SIGKILL) while it waited for call $spawned ($first)"
p2="$(start 2)"
for _ in $(seq 1 2400); do
  left=$(find "$R" -maxdepth 1 -name '*.json' | wc -l)
  [ "$left" = 0 ] && break
  kill -0 "$p2" 2>/dev/null || { cat "$OUT/client-2.err" >&2; say "STOP: client 2 ended early"; exit 1; }
  sleep 2
done
kill -TERM "$p2" 2>/dev/null || true; wait "$p2" 2>/dev/null || true
cp -r "$R/done" "$OUT/done" 2>/dev/null || true
[ -d "$R/failed" ] && [ -n "$(ls -A "$R/failed")" ] && { cp -r "$R/failed" "$OUT/failed"; say "STOP: failed requests: $(ls "$R/failed")"; exit 1; }
printf 'request\tcall id spawned\tcall id answered\tattempts\tresumed\tok\tseconds\n' > "$OUT/calls.tsv"
ok=1
for d in "$OUT"/done/*.json; do
  n="$(basename "$d" .json)"
  sp="$(jq -r --arg n "$n" 'select(.event == "spawned" and .name == $n) | .call_id' "$OUT/events.jsonl" | paste -sd, -)"
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$n" "$sp" "$(jq -r .call_id "$d")" "$(jq -r .attempts "$d")" "$(jq -r .resumed "$d")" \
    "$(jq -r .ok "$d")" "$(jq -r .seconds "$d")" >> "$OUT/calls.tsv"
  [ "$(jq -r .ok "$d")" = true ] || ok=0
  [ "$(echo "$sp" | tr ',' '\n' | grep -c .)" = 1 ] || { say "$n: spawned $(echo "$sp" | tr ',' '\n' | grep -c .) times"; ok=0; }
  [ -s "$OUT/inbox/$n.json" ] || { say "$n: no job in the inbox"; ok=0; }
done
[ "$(ls "$OUT"/done/*.json | wc -l)" = "$#" ] || { say "STOP: $(ls "$OUT"/done/*.json | wc -l) answers for $# requests"; ok=0; }
k="$OUT/done/$first.json"
if [ "$(jq -r .call_id "$k")" = "$spawned" ] && [ "$(jq -r .resumed "$k")" = true ] && [ "$(jq -r .attempts "$k")" = 1 ]; then
  say "$first: answered by the call client 1 spawned ($spawned), resumed by client 2, one attempt"
else
  say "$first: NOT the killed client's call (call $(jq -r .call_id "$k"), resumed $(jq -r .resumed "$k"), attempts $(jq -r .attempts "$k"))"; ok=0
fi
column -t -s $'\t' "$OUT/calls.tsv" | tee -a "$OUT/summary.txt"
[ "$ok" = 1 ] || { say "PROVER KILL-RESUME FAILED"; exit 1; }
say "PROVER KILL-RESUME OK"
