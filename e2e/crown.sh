#!/usr/bin/env bash
# The crown run (e2e/crown/README.md) in the cloud: the Modal app ferrata-crown (e2e/crown/cloud/app.py) on the
# your Modal workspace. Once started it needs nothing from this machine until the results are fetched.
#
#   e2e/crown.sh setup [--inputs rc1|rc3] [--bundle FILE] [--programs DIR]
#                                     deploy the app; the LEZ bundle and the programs to its Volume, under the input
#                                     set's directory (e2e/crown/cloud/inputs.json: rc1 = inputs/, crown-live-1 to -3's;
#                                     rc3 = inputs-rc3/: LEZ v0.4.0-rc3, its node 0.3.5-rc.1, the second build of the
#                                     checkpoint guests); the other sets stay as they are
#   e2e/crown.sh start [--fork] [--inputs rc1|rc3] [--yes] [--halt-after STAGE] [--lez-chain sepolia] [run]
#                                     --inputs: a new run's input set (default rc1; a resume keeps the run's own);
#                                     live (default run crown-live-3; crown-live-1 is done, crown-live-2 stopped at lez-a: its
#                                     checkpoint was outside the weak-subjectivity period): signs on Sepolia only after a typed yes;
#                                     --fork (default run crown-rehearsal-4): an anvil fork of Sepolia, nothing sent;
#                                     --fork --lez-chain sepolia --halt-after identity: session A on the bundle's real
#                                     Sepolia checkpoint (the live run's), ending after identity
#   e2e/crown.sh status [run]         the runner, the stage, the stages done, the last lines of the log
#   e2e/crown.sh fetch [--lez] [run]  the run directory into e2e/runs/<run>/ (--lez: also the LEZ sessions' records)
#   e2e/crown.sh stop [--app]         cancel the runner (and a queued GPU proof); `start` resumes; --app also stops the app
#   e2e/crown.sh verify [run]         e2e/crown/verify.py over a fetched live run, from this machine (chain reads only)
#   e2e/crown.sh updater RUN --bin FILE|--module DIR [--minutes N] [--restart-min M] [--poll-s S]
#                                     session A on a new zone, then ferrata-updater (lez/updater/sequencer) on it for N
#                                     minutes, reading the public Sepolia beacon APIs (lez/updater/README.md); --module
#                                     DIR: the same daemon as ferrata_updater_module in a headless Logos Core (DIR: the
#                                     module's portable *.lgx and logos-portable.tar.zst); also
#                                     updater-stop RUN, updater-status RUN, updater-fetch RUN DIR
#
# Settings: e2e/sepolia-live.env (or $FERRATA_ENV_FILE) for SEPOLIA_RPC_URL and ETH_FROM (read-only checks of `start`;
# the run itself signs with the Secret ferrata-sepolia-key that `e2e/sepolia-cloud.sh setup` made). The Modal
# workspace is CROWN_MODAL_PROFILE (required: yModal), given per command, never activated.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/.." && pwd)"
envfile="${FERRATA_ENV_FILE:-$here/sepolia-live.env}"
if [ -f "$envfile" ]; then set -a; . "$envfile"; set +a; fi
MODAL_PROFILE="${CROWN_MODAL_PROFILE:?set CROWN_MODAL_PROFILE to your Modal profile}"
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
    --inputs) opts+=(--inputs "$2"); shift ;;
    *) break ;;
    esac
    shift
  done
  def=crown-live-3; [ "$mode" = live ] || def=crown-rehearsal-4
  ctl start "${1:-$def}" "$mode" "${opts[@]}"
  ;;
status) ctl status "${1:-crown-live-3}" ;;
fetch)
  lez=(); [ "${1:-}" = --lez ] && { lez=(--lez); shift; }
  ctl fetch "${1:-crown-live-3}" "${lez[@]}"
  ;;
stop) ctl stop "$@" ;;
verify)
  run="$repo/e2e/runs/${1:-crown-live-3}"
  : "${SEPOLIA_RPC_URL:=https://ethereum-sepolia-rpc.publicnode.com}"
  SEPOLIA_RPC_URL="$SEPOLIA_RPC_URL" python3 -I "$here/crown/verify.py" "$run" --lez "$run/lez"
  ;;
updater|updater-stop|updater-status|updater-fetch) ctl "$cmd" "$@" ;;
*) sed -n '2,30p' "$0" >&2; exit 2 ;;
esac
