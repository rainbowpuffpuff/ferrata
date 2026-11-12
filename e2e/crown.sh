#!/usr/bin/env bash
# The crown run (e2e/crown/README.md) in the cloud: the Modal app ferrata-crown (e2e/crown/cloud/app.py) on the
# workspace ws-3. Once started it needs nothing from this machine until the results are fetched.
#
#   e2e/crown.sh setup [--bundle FILE] [--programs DIR]   deploy the app; the LEZ bundle and the programs to its Volume
#   e2e/crown.sh start [--fork] [--yes] [--halt-after STAGE] [--lez-chain sepolia] [run]
#                                     live (default run crown-live-2; crown-live-1 is done): signs on Sepolia only after a typed yes;
#                                     --fork (default run crown-rehearsal-4): an anvil fork of Sepolia, nothing sent;
#                                     --fork --lez-chain sepolia --halt-after identity: session A on the bundle's real
#                                     Sepolia checkpoint (the live run's), ending after identity
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
# A retired Modal workspace (named by the first 16 hex digits of its name's SHA-256) must not run the crown.
[ "$(printf %s "$MODAL_PROFILE" | sha256sum | cut -c1-16)" != 5e810fe35ab057e2 ] || { echo "crown: not on the retired workspace" >&2; exit 1; }
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
    --lez-chain) opts+=(--lez-chain "$2"); shift ;;
    *) break ;;
    esac
    shift
  done
  def=crown-live-2; [ "$mode" = live ] || def=crown-rehearsal-4
  ctl start "${1:-$def}" "$mode" "${opts[@]}"
  ;;
status) ctl status "${1:-crown-live-2}" ;;
fetch)
  lez=(); [ "${1:-}" = --lez ] && { lez=(--lez); shift; }
  ctl fetch "${1:-crown-live-2}" "${lez[@]}"
  ;;
stop) ctl stop "$@" ;;
verify)
  run="$repo/e2e/runs/${1:-crown-live-2}"
  : "${SEPOLIA_RPC_URL:=https://ethereum-sepolia-rpc.publicnode.com}"
  SEPOLIA_RPC_URL="$SEPOLIA_RPC_URL" python3 -I "$here/crown/verify.py" "$run" --lez "$run/lez"
  ;;
*) sed -n '2,18p' "$0" >&2; exit 2 ;;
esac
