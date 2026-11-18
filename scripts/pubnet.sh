# shellcheck shell=bash
# pubnet.sh: sourced by bash scripts that talk to a public network (e2e/NETWORK.md; the Python side: scripts/pubnet.py).
#
#   pubnet_up <network> <state dir> [sender] [first endpoint]
#        Starts the local JSON-RPC proxy (pubnet.py proxy) in front of the network's endpoint list, with its journal in
#        <state dir>, and exports PUBNET_URL (forge and cast are given "$PUBNET_URL/<stage>"). A caller's proxy is
#        reused: if PUBNET_URL is set and answers, nothing is started. FERRATA_UNLOCKED=1 allows eth_sendTransaction
#        (a local fork only). PUBNET_LAG: how far behind the head reads at "latest" are pinned (default 3, 0 local).
#   pubnet_down
#   pubnet_stage <log> <command...>
#        Runs a command that may send (forge script ... --rpc-url "$PUBNET_URL/<stage>") until it succeeds. Before the
#        first run and after each, the fate of every transaction the journal holds (pubnet.py resolve: mined, replaced,
#        rejected), so nothing is sent again before that is known (a bad fate stops until an operator has looked:
#        pubnet.py ack); then a failed run is classified (pubnet.py classify): a transient
#        one runs again after an exponential backoff with jitter (15 s, 30 s, ... at most 300 s) while the stage's
#        budget lasts (PUBNET_STAGE_BUDGET_S, default 1200); a permanent one stops. Returns 0, 1 (permanent: a human
#        looks) or 75 (transient: the budget is spent with nothing in flight, or a fate is still unknown).
#   pubnet_read <command...>
#        A read-only command (cast ...), again after a transient failure, within PUBNET_READ_BUDGET_S (300).
PUBNET_PY="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/pubnet.py"
PUBNET_PID=""

_pubnet_say() { echo "[$(date -u +%FT%TZ)] pubnet: $*" >&2; }

pubnet_alive() {
  [ -n "${PUBNET_URL:-}" ] && curl -s -m 5 -o /dev/null -H 'Content-Type: application/json' \
    --data '{"jsonrpc":"2.0","id":1,"method":"eth_chainId","params":[]}' "$PUBNET_URL/alive"
}

pubnet_up() {
  local network="$1" dir="$2" sender="${3:-}" first="${4:-}" k
  export PUBNET_NETWORK="$network" PUBNET_DIR="${PUBNET_DIR:-$dir}" PUBNET_SENDER="${PUBNET_SENDER:-$sender}"
  export PUBNET_FIRST="${PUBNET_FIRST:-$first}"
  if [ -n "${PUBNET_URL:-}" ] && pubnet_alive; then return 0; fi
  mkdir -p "$PUBNET_DIR"
  rm -f "$PUBNET_DIR/port"
  local args=(proxy --network "$network" --journal "$PUBNET_DIR/journal.jsonl" --port-file "$PUBNET_DIR/port")
  [ -n "$PUBNET_SENDER" ] && args+=(--sender "$PUBNET_SENDER")
  [ -n "$PUBNET_FIRST" ] && args+=(--first "$PUBNET_FIRST")
  [ -n "${PUBNET_LAG:-}" ] && args+=(--lag "$PUBNET_LAG")
  [ "${FERRATA_UNLOCKED:-}" = 1 ] && args+=(--allow-unlocked)
  python3 -I "$PUBNET_PY" "${args[@]}" >> "$PUBNET_DIR/proxy.log" 2>&1 &
  PUBNET_PID=$!
  for k in $(seq 1 150); do
    [ -s "$PUBNET_DIR/port" ] && break
    kill -0 "$PUBNET_PID" 2>/dev/null || { tail -n 5 "$PUBNET_DIR/proxy.log" >&2; return 1; }
    sleep 0.2
  done
  [ -s "$PUBNET_DIR/port" ] || { _pubnet_say "the proxy did not start"; return 1; }
  export PUBNET_URL="http://127.0.0.1:$(cat "$PUBNET_DIR/port")"
  trap pubnet_down EXIT
}

pubnet_down() {
  if [ -n "$PUBNET_PID" ]; then kill "$PUBNET_PID" 2>/dev/null || true; wait "$PUBNET_PID" 2>/dev/null || true; fi
  PUBNET_PID=""
}

pubnet_resolve() { # exit 0 all mined with status 1, 3 a bad fate, 75 unknown
  [ -n "${PUBNET_SENDER:-}" ] || return 0
  python3 -I "$PUBNET_PY" resolve --network "$PUBNET_NETWORK" --journal "$PUBNET_DIR/journal.jsonl" \
    --sender "$PUBNET_SENDER" ${PUBNET_FIRST:+--first "$PUBNET_FIRST"} --budget "${1:-${PUBNET_RESOLVE_BUDGET_S:-600}}"
}

pubnet_stage() {
  local log="$1" t0 since attempt=0 rc cls pause budget="${PUBNET_STAGE_BUDGET_S:-1200}"
  shift
  t0="$(date +%s)"
  # Before the first run too: a stage stopped (or killed) after a broadcast sends nothing until that fate is known.
  pubnet_resolve || { local r0=$?; [ "$r0" = 3 ] && { _pubnet_say "an earlier transaction did not succeed (above): a human looks, then pubnet.py ack"; return 1; }
    _pubnet_say "an earlier transaction's fate is still unknown: nothing is sent"; return 75; }
  while :; do
    if [ -n "$PUBNET_PID" ] && ! kill -0 "$PUBNET_PID" 2>/dev/null; then
      _pubnet_say "the proxy is gone; starting it again"
      unset PUBNET_URL
      pubnet_up "$PUBNET_NETWORK" "$PUBNET_DIR" "$PUBNET_SENDER" "$PUBNET_FIRST" || return 75
    fi
    since="$(date +%s.%N)"
    rc=0
    # A run that hangs (forge waiting for a receipt no node will give) is ended after PUBNET_ATTEMPT_TIMEOUT_S: what
    # it sent is in the journal, and its fate is found below before anything else.
    timeout -k 30 "${PUBNET_ATTEMPT_TIMEOUT_S:-900}" "$@" > "$log" 2>&1 || rc=$?
    # Whatever happened, the fate of every journaled transaction first: nothing is sent while one is unknown.
    pubnet_resolve || { local r=$?; [ "$r" = 3 ] && return 1; _pubnet_say "a transaction's fate is still unknown"; return 75; }
    [ "$rc" = 0 ] && return 0
    cls="$(python3 -I "$PUBNET_PY" classify --events "$PUBNET_DIR/events.jsonl" --since "$since" "$log")"
    if [ "$rc" = 124 ] || [ "$rc" = 137 ]; then cls=transient; echo "pubnet: the run took longer than ${PUBNET_ATTEMPT_TIMEOUT_S:-900} s and was ended" >> "$log"; fi
    tail -n 12 "$log" | sed -e "s,${PUBNET_FIRST:-@@none@@},<endpoint>,g" >&2
    if [ "$cls" != transient ]; then
      _pubnet_say "the run failed for a permanent reason (log $log); nothing more is sent"
      return 1
    fi
    attempt=$((attempt + 1))
    pause=$(( ${PUBNET_BACKOFF_BASE_S:-15} << (attempt > 5 ? 5 : attempt - 1) )); [ "$pause" -le 300 ] || pause=300
    pause=$(( pause * (50 + RANDOM % 101) / 100 ))
    if [ $(( $(date +%s) + pause - t0 )) -ge "$budget" ]; then
      _pubnet_say "transient failures for $(( $(date +%s) - t0 )) s (budget $budget s); stopping with nothing in flight"
      return 75
    fi
    mv "$log" "$log.transient-$attempt"
    _pubnet_say "transient failure, nothing in flight; again in $pause s (attempt $attempt, log $log.transient-$attempt)"
    sleep "$pause"
  done
}

pubnet_read() {
  local t0 attempt=0 out err pause budget="${PUBNET_READ_BUDGET_S:-300}"
  t0="$(date +%s)"
  err="$(mktemp "${TMPDIR:-.}/pubnet-read.XXXXXX")"
  while :; do
    if out="$("$@" 2> "$err")"; then rm -f "$err"; printf '%s\n' "$out"; return 0; fi
    if [ "$(python3 -I "$PUBNET_PY" classify "$err")" != transient ]; then cat "$err" >&2; rm -f "$err"; return 1; fi
    attempt=$((attempt + 1))
    pause=$(( (2 << (attempt > 4 ? 4 : attempt - 1)) * (50 + RANDOM % 101) / 100 + 1 ))
    if [ $(( $(date +%s) + pause - t0 )) -ge "$budget" ]; then cat "$err" >&2; rm -f "$err"; return 75; fi
    sleep "$pause"
  done
}
