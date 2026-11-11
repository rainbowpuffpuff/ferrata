#!/usr/bin/env bash
# Stands in for RISC Zero's Groth16 prover image in cli/tests/relayer_fork.rs (the build box has no Docker): called
# as `wrap-stub.sh <input.json> <proof.json>` by ferrata-relayer --wrap-command, it returns the proof.json that the
# prover image produced for this release on Modal (e2e/runs/relayer-fork/paid/proof.json, FERRATA_WRAP_STUB_PROOF).
# identity_p254 before it and the Groth16 receipt after it run in the daemon, which verifies the result.
set -euo pipefail
[ -s "$1" ] || { echo "wrap-stub: no input.json" >&2; exit 1; }
cp "${FERRATA_WRAP_STUB_PROOF:?}" "$2"
