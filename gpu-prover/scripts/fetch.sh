#!/usr/bin/env bash
# Downloads the Volume's results/ (receipts, journals, timing JSON, logs, GPU samples) into results/
# and the CUDA-free verifier binary built in the image into bin/. With a run name, only that run's
# results/<run>/ and no binaries. The Modal profile is taken from MODAL_PROFILE on each command (never
# `modal profile activate`).
#
# usage: scripts/fetch.sh [RUN]
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
: "${MODAL_PROFILE:?set MODAL_PROFILE to the profile of the workspace that holds the Volume}"
export MODAL_PROFILE
mkdir -p "$here/results" "$here/bin"
if [ $# -ge 1 ]; then
  modal volume get --force ferrata-prover "results/$1" "$here/results/"
else
  modal volume get --force ferrata-prover results "$here/"
  modal volume get --force ferrata-prover bin "$here/"
  chmod +x "$here/bin/ferrata-modal-verify"
fi
find "$here/results" -type f -newer "$here/modal_app.py" | sort | sed "s#^$here/##"
