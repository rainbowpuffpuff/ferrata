#!/usr/bin/env bash
# The crown run (e2e/crown/README.md) in the cloud: the Modal app ferrata-crown (e2e/crown/cloud/app.py) on the
# workspace ws-3. Once started it needs nothing from this machine until the results are fetched.
#
#   e2e/crown.sh setup [--bundle FILE] [--programs DIR]   deploy the app; the LEZ bundle and the programs to its Volume
#   e2e/crown.sh start [--fork] [--yes] [--halt-after STAGE] [run]
#                                     live (default run crown-live-1): signs on Sepolia only after a typed yes;
#                                     --fork (default run crown-rehearsal-1): an anvil fork of Sepolia, nothing sent
#   e2e/crown.sh status [run]         the runner, the stage, the stages done, the last lines of the log
#   e2e/crown.sh fetch [--lez] [run]  the run directory into e2e/runs/<run>/ (--lez: also the LEZ sessions' records)
#   e2e/crown.sh stop [--app]         cancel the runner (and a queued GPU proof); `start` resumes; --app also stops the app
#   e2e/crown.sh verify [run]         e2e/crown/verify.py over a fetched live run, from this machine (chain reads only)
#
# Settings: e2e/sepolia-live.env (or $FERRATA_ENV_FILE) for SEPOLIA_RPC_URL and ETH_FROM (read-only checks of `start`;
# the run itself signs with the Secret ferrata-sepolia-key that `e2e/sepolia-cloud.sh setup` made). The Modal
# workspace is CROWN_MODAL_PROFILE (default ws-3), given per command, never activated.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/.." && pwd)"
envfile="${FERRATA_ENV_FILE:-$here/sepolia-live.env}"
if [ -f "$envfile" ]; then set -a; . "$envfile"; set +a; fi
MODAL_PROFILE="${CROWN_MODAL_PROFILE:-ws-3}"
[ "$MODAL_PROFILE" != ws-2 ] || { echo "crown: not on ws-2 (retired)" >&2; exit 1; }
export MODAL_PROFILE FERRATA_REPO="$repo" TMPDIR="${TMPDIR:-$repo/../tmp}"
mkdir -p "$TMPDIR/crown-cloud"
ctl() { (cd "$repo" && python3 "$here/crown/cloud/ctl.py" "$@"); }

cmd="${1:-}"
[ $# -gt 0 ] && shift
case "$cmd" in
setup) ctl setup "$@" ;;
start)
  mode=live opts=()
  while [ $# -gt 0 ]; do
    case "$1" in
    --fork) mode=fork ;;
    --selftest) mode=selftest ;;  # the image, the tree, the inputs, a ping: no build, nothing sent
    --yes) opts+=(--yes) ;;
    --halt-after) opts+=(--halt-after "$2"); shift ;;
    *) break ;;
    esac
    shift
  done
  def=crown-live-1; [ "$mode" = live ] || def=crown-rehearsal-1
  ctl start "${1:-$def}" "$mode" "${opts[@]}"
  ;;
status) ctl status "${1:-crown-live-1}" ;;
fetch)
  lez=(); [ "${1:-}" = --lez ] && { lez=(--lez); shift; }
  ctl fetch "${1:-crown-live-1}" "${lez[@]}"
  ;;
stop) ctl stop "$@" ;;
verify)
  run="$repo/e2e/runs/${1:-crown-live-1}"
  : "${SEPOLIA_RPC_URL:=https://ethereum-sepolia-rpc.publicnode.com}"
  SEPOLIA_RPC_URL="$SEPOLIA_RPC_URL" python3 -I "$here/crown/verify.py" "$run" --lez "$run/lez"
  ;;
*) sed -n '2,19p' "$0" >&2; exit 2 ;;
esac
