#!/usr/bin/env bash
# e2e/sepolia-live.sh run in the cloud (the Modal app ferrata-sepolia, e2e/cloud/app.py): once started it
# needs nothing from this machine until the results are fetched. e2e/README.md, "Running it in the cloud".
#
#   e2e/sepolia-cloud.sh setup                       deploy the app; create or update its Secret (idempotent)
#   e2e/sepolia-cloud.sh start [--fork] [--yes] [run]  upload what the run needs, start the runner detached, return
#   e2e/sepolia-cloud.sh status [run]                 the runner's state, the stages done, the last lines of the log
#   e2e/sepolia-cloud.sh fetch [run]                  the run directory from the cloud into e2e/runs/<run>/
#   e2e/sepolia-cloud.sh stop                         cancel running runners and stop the app
#
# [run] is a directory under e2e/runs/ (default sepolia-live). `start` signs on Sepolia only after a typed
# yes (or --yes); with --fork the runner rehearses on anvil forking Sepolia, impersonating the deployer.
# Settings come from e2e/sepolia-live.env (or $FERRATA_ENV_FILE), the file e2e/sepolia-live.sh reads:
# MODAL_PROFILE names the Modal workspace (always given per command, never activated); `setup` reads the
# keystore file and the password file named by ETH_KEYSTORE and ETH_PASSWORD and puts their contents, with
# SEPOLIA_RPC_URL and ETH_FROM, into the Secret ferrata-sepolia-key. FERRATA_MRUN and FERRATA_MWRAP are not
# used: everything runs in the cloud.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/.." && pwd)"
envfile="${FERRATA_ENV_FILE:-$here/sepolia-live.env}"
if [ -f "$envfile" ]; then set -a; . "$envfile"; set +a; fi
: "${MODAL_PROFILE:?set MODAL_PROFILE (in $envfile): the profile of the Modal workspace that runs the app}"
export MODAL_PROFILE FERRATA_REPO="$repo" TMPDIR="${TMPDIR:-$repo/../tmp}"
mkdir -p "$TMPDIR/sepolia-cloud"
ctl() { (cd "$repo" && python3 "$here/cloud/ctl.py" "$@"); }

cmd="${1:-}"
[ $# -gt 0 ] && shift
case "$cmd" in
setup) ctl setup ;;
start)
  mode=live yes=()
  while [ $# -gt 0 ]; do
    case "$1" in
    --fork) mode=fork ;;
    --selftest) mode=selftest ;;  # the image, the tree and the hooks only; no build, nothing sent
    --yes) yes=(--yes) ;;
    *) break ;;
    esac
    shift
  done
  ctl start "${1:-sepolia-live}" "$mode" "${yes[@]}"
  ;;
status | fetch) ctl "$cmd" "${1:-sepolia-live}" ;;
stop) ctl stop ;;
*) sed -n '2,10p' "$0" >&2; exit 2 ;;
esac
