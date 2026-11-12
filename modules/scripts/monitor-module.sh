#!/usr/bin/env bash
# `monitor` through ferrata_monitor_module, loaded in a running headless Logos Core (logosctl daemon):
# what monitor/tests/anvil-e2e.sh runs in place of the binary when MONITOR_CMD names this script.
#
#   monitor-module.sh run ARGS...       with --max-polls: the module's `run` (blocks until the polls
#                                       are done); without: the module's `start`, then this script waits
#                                       until the daemon ends, and on SIGTERM calls `stop` and exits with
#                                       the daemon's exit status (the binary's behaviour on SIGTERM)
#   monitor-module.sh check ARGS...     the module's `check` (monitor check's arguments)
#   monitor-module.sh node-view ARGS... the module's `node_view` (monitor node-view's arguments)
#   monitor-module.sh kill9             SIGKILL the module's host process (logos_host), as anvil-e2e.sh's
#                                       S5 kills the binary, then load the module again
#   monitor-module.sh host-pid          the module's host process id
#
# Exit status and standard error as the binary's: 0, or 1 with `Error: <message>` on standard error.
# The watcher key: `--key-env VAR` names a variable of this script's environment, not the module's, so
# it is written to a file only this user can read and passed as `--key-file` (the key never goes into
# the request). Environment: LOGOSCTL (the logosctl binary), SESSION (its --config-dir), KEY_DIR
# (where key files go; default $SESSION/keys).
set -uo pipefail
: "${LOGOSCTL:?}" "${SESSION:?}"
M=ferrata_monitor_module
KEY_DIR=${KEY_DIR:-$SESSION/keys}
ctl() { "$LOGOSCTL" --config-dir "$SESSION" "$@"; }

# call METHOD REQUEST_JSON: the method's reply (itself JSON) on standard output.
call() {
  local req raw rc
  req=$(mktemp "$SESSION/req.XXXXXX")
  printf '%s' "$2" > "$req"
  raw=$(ctl --timeout 900000 call "$M" "$1" "@$req" --json < /dev/null 2>&1); rc=$?
  rm -f "$req"
  if [ $rc != 0 ]; then echo "Error: logosctl call $M $1 failed (exit $rc): $raw" >&2; return 1; fi
  jq -c 'if (.result | type) == "string" then (.result | fromjson? // .result) else .result end' <<< "$raw"
}
call0() { # a method without a request
  local raw rc
  raw=$(ctl --timeout 900000 call "$M" "$1" --json < /dev/null 2>&1); rc=$?
  if [ $rc != 0 ]; then echo "Error: logosctl call $M $1 failed (exit $rc): $raw" >&2; return 1; fi
  jq -c 'if (.result | type) == "string" then (.result | fromjson? // .result) else .result end' <<< "$raw"
}
fail_on() { # REPLY OK_VERDICT...: exit 1 with the reply's error unless its verdict is one of these
  local reply=$1 v ok
  shift
  v=$(jq -r '.verdict // ""' <<< "$reply")
  for ok in "$@"; do [ "$v" = "$ok" ] && return 0; done
  echo "Error: $(jq -r '.error // .' <<< "$reply")" >&2
  exit 1
}

# The module's host process: `logos_host --name ferrata_monitor_module --path ... --instance-persistence-path
# $SESSION/data/...` (as logosctl 66d098d starts it), in this session.
host_pid() {
  local p a
  for p in $(pgrep -f logos_host 2>/dev/null); do
    a=$(tr '\0' ' ' < "/proc/$p/cmdline" 2>/dev/null) || continue
    case "$a" in *"--name $M "*"$SESSION/"*) echo "$p"; return 0 ;; esac
  done
  return 1
}

cmd=${1:-}; shift || true
case "$cmd" in
  run)
    args=()
    bounded=0
    while [ $# -gt 0 ]; do
      case "$1" in
        --key-env)
          mkdir -p "$KEY_DIR"; chmod 700 "$KEY_DIR"
          f="$KEY_DIR/$2.key"
          ( umask 077; printf '%s\n' "${!2:?$2 is not set}" > "$f" )
          args+=(--key-file "$f"); shift 2 ;;
        --max-polls) bounded=1; args+=("$1" "$2"); shift 2 ;;
        *) args+=("$1"); shift ;;
      esac
    done
    req=$(jq -n -c '{args: $ARGS.positional}' --args -- "${args[@]}")
    if [ $bounded = 1 ]; then
      r=$(call run "$req") || exit 1
      fail_on "$r" ok
      exit 0
    fi
    r=$(call start "$req") || exit 1
    fail_on "$r" started
    stopping=0
    trap 'stopping=1' TERM INT
    while :; do
      if [ $stopping = 1 ]; then
        r=$(call0 stop) || exit 1
        [ "$(jq -r '.exit // 0' <<< "$r")" = 0 ] || { echo "Error: $(jq -r .error <<< "$r")" >&2; exit 1; }
        exit 0
      fi
      r=$(call0 status) || exit 1
      if [ "$(jq -r .running <<< "$r")" != true ]; then
        [ "$(jq -r '.exit // 0' <<< "$r")" = 0 ] || { echo "Error: $(jq -r .error <<< "$r")" >&2; exit 1; }
        exit 0
      fi
      sleep 0.3 & wait $!
    done ;;
  check)
    rule=""; cand=""; now=""; nodes=(); extra='{}'
    while [ $# -gt 0 ]; do
      case "$1" in
        --rule) rule=$2; shift 2 ;;
        --candidate) cand=$2; shift 2 ;;
        --node) nodes+=("$2"); shift 2 ;;
        --now) now=$2; shift 2 ;;
        --turn-seconds|--b-lag-grace|--delta-g|--margin)
          k=$(sed -e 's/^--//' -e 's/-/_/g' <<< "$1"); extra=$(jq -c --arg k "$k" --argjson v "$2" '. + {($k): $v}' <<< "$extra"); shift 2 ;;
        *) echo "Error: monitor-module.sh check: unknown argument $1" >&2; exit 2 ;;
      esac
    done
    req=$(jq -n -c --arg rule "$rule" --arg cand "$(realpath "$cand")" --argjson now "$now" --argjson extra "$extra" \
      '{rule: $rule, candidate_file: $cand, now: $now, nodes: $ARGS.positional} + $extra' --args -- "${nodes[@]}")
    r=$(call check "$req") || exit 1
    fail_on "$r" pass wait cancel blind
    jq . <<< "$r" ;;
  node-view)
    tsv=""; label=lez; sets=""
    while [ $# -gt 0 ]; do
      case "$1" in
        --chain-tsv) tsv=$2; shift 2 ;;
        --label) label=$2; shift 2 ;;
        --key-sets) sets=$2; shift 2 ;;
        *) echo "Error: monitor-module.sh node-view: unknown argument $1" >&2; exit 2 ;;
      esac
    done
    req=$(jq -n -c --arg t "$(realpath "$tsv")" --arg l "$label" --arg s "${sets:+$(realpath "$sets")}" \
      '{chain_tsv_file: $t, label: $l} + (if $s == "" then {} else {key_sets_file: $s} end)')
    r=$(call node_view "$req") || exit 1
    fail_on "$r" ok
    jq '.view' <<< "$r" ;;
  kill9)
    p=$(host_pid) || { echo "Error: no logos_host process of $M" >&2; exit 1; }
    kill -9 "$p"
    for _ in $(seq 1 100); do kill -0 "$p" 2>/dev/null || break; sleep 0.1; done
    # The daemon sees its module crashed; load it again (a restarted monitor).
    for _ in $(seq 1 50); do
      st=$(ctl module ls --json 2>/dev/null | jq -r --arg m "$M" '.[]? | select(.name == $m) | .status' 2>/dev/null)
      [ "$st" = crashed ] || [ "$st" = stopped ] || [ "$st" = "" ] && break
      sleep 0.1
    done
    ctl module reload "$M" --json > /dev/null 2>&1 || ctl module load "$M" --json > /dev/null 2>&1 || true
    for _ in $(seq 1 100); do
      host_pid > /dev/null && call0 version > /dev/null 2>&1 && exit 0
      ctl module load "$M" --json > /dev/null 2>&1 || true
      sleep 0.2
    done
    echo "Error: $M did not come back after the kill" >&2; exit 1 ;;
  host-pid) host_pid ;;
  *) echo "usage: monitor-module.sh run|check|node-view ARGS... | kill9 | host-pid" >&2; exit 2 ;;
esac
