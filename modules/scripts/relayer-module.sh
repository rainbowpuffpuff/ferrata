#!/usr/bin/env bash
# `ferrata-relayer` through ferrata_relayer_module, loaded in a running headless Logos Core (logosctl daemon): what
# cli/tests/relayer_fork.rs and relayer-e2e.sh run in place of the binary when FERRATA_RELAYER_CMD (RELAYER) names
# this script. The same arguments and the same JSON lines on standard output:
#
#   relayer-module.sh ARGS... run --rounds N ...   the module's `run` (blocks until the rounds are done)
#   relayer-module.sh ARGS... status               the module's `run` too
#   relayer-module.sh ARGS... run ...              (no --rounds) the module's `start`; this script prints the
#                                                  daemon's lines as they come (`status` with `since`) and, on
#                                                  SIGTERM or SIGINT, calls `stop` with that signal as the reason
#                                                  (the binary's clean stop) and exits with the daemon's status
#
# Exit status and standard error as the binary's: 0, or 1 with `Error: <message>` on standard error. Paths in the
# arguments are passed as given: the module's process reads them (Logos Core runs on this machine).
# Environment: LOGOSCTL (the logosctl binary), SESSION (its --config-dir).
set -uo pipefail
: "${LOGOSCTL:?}" "${SESSION:?}"
M=ferrata_relayer_module
ctl() { "$LOGOSCTL" --config-dir "$SESSION" "$@"; }

# call METHOD REQUEST_JSON: the method's reply (itself JSON) on standard output.
call() {
  local req raw rc
  req=$(mktemp "$SESSION/req.XXXXXX")
  printf '%s' "$2" > "$req"
  raw=$(ctl --timeout 1800000 call "$M" "$1" "@$req" --json < /dev/null 2>&1); rc=$?
  rm -f "$req"
  if [ $rc != 0 ]; then echo "Error: logosctl call $M $1 failed (exit $rc): $raw" >&2; return 1; fi
  jq -c 'if (.result | type) == "string" then (.result | fromjson? // .result) else .result end' <<< "$raw"
}
lines() { jq -c '.events[]?' <<< "$1"; }
ends() { # REPLY: exit as the binary would (0, or 1 with the error)
  if [ "$(jq -r '.verdict' <<< "$1")" = invalid_request ] || [ "$(jq -r '.exit // 0' <<< "$1")" != 0 ] \
     || [ "$(jq -r '.verdict' <<< "$1")" = error ]; then
    echo "Error: $(jq -r '.error // .' <<< "$1")" >&2
    exit 1
  fi
  exit 0
}

req=$(jq -n -c '{args: $ARGS.positional}' --args -- "$@")
# Bounded: `status`, or `run` with a non-zero --rounds.
bounded=0
prev=""
for a in "$@"; do
  [ "$a" = status ] && [ "$prev" != --inbox ] && [ "$prev" != --state ] && bounded=1
  if [ "$prev" = --rounds ] && [ "$a" != 0 ]; then bounded=1; fi
  case "$a" in --rounds=*) [ "${a#--rounds=}" != 0 ] && bounded=1 ;; esac
  prev=$a
done
if [ $bounded = 1 ]; then
  r=$(call run "$req") || exit 1
  lines "$r"
  ends "$r"
fi

# The binary installs its handlers before its first round: a signal while the daemon starts is kept for the loop.
stopping=""
trap 'stopping=SIGTERM' TERM
trap 'stopping=SIGINT' INT
r=$(call start "$req") || exit 1
lines "$r"
[ "$(jq -r .verdict <<< "$r")" = started ] || ends "$r"
seen=$(jq -r '.events_total // 0' <<< "$r")
while :; do
  if [ -n "$stopping" ]; then
    r=$(call stop "$(jq -n -c --arg reason "$stopping" --argjson since "$seen" '{reason: $reason, since: $since}')") || exit 1
    lines "$r"
    ends "$r"
  fi
  r=$(call status "$(jq -n -c --argjson since "$seen" '{since: $since}')") || exit 1
  lines "$r"
  seen=$(jq -r '.events_total // 0' <<< "$r")
  if [ "$(jq -r .running <<< "$r")" != true ]; then ends "$r"; fi
  sleep 0.3 & wait $!
done
