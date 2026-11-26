#!/usr/bin/env bash
# day1.sh's daemon commands against a stand-in daemon (a process holding the pid file; no network, no tools):
#   without a URL they refuse and name the running daemon; with it they find it; daemon-stop stops it, and kills one
#   that ignores SIGTERM after FERRATA_DAY1_STOP_SECONDS. (review-testnet04, finding 2: without a URL they used to look
#   in state/none and say "not running" while the daemon ran.) Writes only in a directory of its own under $TMPDIR (or
#   the repository's .scratch/), given to day1.sh as FERRATA_DAY1_HOME and removed at exit with every stand-in it
#   started. Exit 0 when every case holds.
set -u
HERE="$(cd "$(dirname "$0")/.." && pwd)"; REPO="$(cd "$HERE/../.." && pwd)"
D="$HERE/day1.sh"; URL="http://day1-test.invalid:1"; SLUG="day1-test.invalid_1"
base="${TMPDIR:-$REPO/.scratch}"
mkdir -p "$base" && H="$(mktemp -d "$base/day1-test.XXXXXX")" || { echo "FAIL cannot make a directory under $base"; exit 1; }
export FERRATA_DAY1_HOME="$H"
S="$H/state/$SLUG"
cleanup() { local p; for p in "$H"/state/*/daemon/pid; do [ -s "$p" ] && kill -9 "$(cat "$p")" 2>/dev/null; done; rm -rf "$H"; }
trap cleanup EXIT
fail=0; n=0
check() { n=$((n+1)); if [ "$1" = 0 ]; then echo "ok   $2"; else echo "FAIL $2"; fail=1; fi; }
standin() { # standin [ignore-term]: a process in its own session playing the daemon, its pid in STATE/daemon/pid
  rm -rf "$S"; mkdir -p "$S/daemon" || { echo "FAIL cannot make $S/daemon"; exit 1; }
  printf 'rpc = "%s"\n' "$URL" > "$S/daemon/updater.toml"
  # the stand-in holds none of this script's descriptors: a caller reading its output is not kept waiting for it
  if [ "${1:-}" = ignore-term ]; then
    setsid bash -c 'trap "" TERM; echo $$ > "$0"; exec sleep 600' "$S/daemon/pid" < /dev/null > /dev/null 2>&1 &
  else
    setsid bash -c 'echo $$ > "$0"; exec sleep 600' "$S/daemon/pid" < /dev/null > /dev/null 2>&1 &
  fi
  for _ in $(seq 1 50); do [ -s "$S/daemon/pid" ] && break; sleep 0.1; done
  [ -s "$S/daemon/pid" ] || { echo "FAIL the stand-in daemon did not start"; exit 1; }
}
export FERRATA_DAY1_DAEMON_PORT=1   # no /health there
unset FERRATA_DAY1_URL FERRATA_DAY1_STATE

standin
pid="$(cat "$S/daemon/pid")"
out="$("$D" daemon-status 2>&1)"; rc=$?
[ "$rc" != 0 ] && printf '%s' "$out" | grep -q "usage: day1.sh daemon-status URL" && printf '%s' "$out" | grep -q "running: pid $pid, URL $URL"
check $? "daemon-status without a URL refuses and names the running daemon (pid $pid, $URL)"
out="$("$D" daemon-stop 2>&1)"; rc=$?
[ "$rc" != 0 ] && kill -0 "$pid" 2>/dev/null
check $? "daemon-stop without a URL refuses and leaves the daemon running"
out="$("$D" daemon-status "$URL" 2>&1)"
printf '%s' "$out" | grep -q "daemon: running, pid $pid"
check $? "daemon-status URL finds it"
out="$(FERRATA_DAY1_URL="$URL" "$D" daemon-status 2>&1)"
printf '%s' "$out" | grep -q "daemon: running, pid $pid"
check $? "daemon-status with FERRATA_DAY1_URL finds it"
out="$(FERRATA_DAY1_STATE="$S" "$D" daemon-status 2>&1)"
printf '%s' "$out" | grep -q "daemon: running, pid $pid"
check $? "daemon-status with FERRATA_DAY1_STATE finds it"
out="$("$D" daemon-stop "$URL" 2>&1)"
printf '%s' "$out" | grep -q "daemon: stopped" && ! kill -0 "$pid" 2>/dev/null
check $? "daemon-stop URL stops it (SIGTERM)"
out="$("$D" daemon-status "$URL" 2>&1)"
printf '%s' "$out" | grep -q "daemon: not running"
check $? "daemon-status URL then says not running"

standin ignore-term
pid="$(cat "$S/daemon/pid")"
out="$(FERRATA_DAY1_STOP_SECONDS=2 "$D" daemon-stop "$URL" 2>&1)"
printf '%s' "$out" | grep -q "daemon: killed (it did not stop in 2 s)" && ! kill -0 "$pid" 2>/dev/null
check $? "daemon-stop kills one that ignores SIGTERM after FERRATA_DAY1_STOP_SECONDS, and says what that may cost"

echo "$((n)) cases, $([ "$fail" = 0 ] && echo "all hold" || echo "FAILED")"
exit "$fail"
