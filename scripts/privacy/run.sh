#!/usr/bin/env bash
# The privacy suite end to end (proposal B3): the LEZ half (RISC Zero dev mode), the Ethereum half
# (Foundry, mock verifier), then the gate, the adversary, the field table and the recovery check.
#
#   scripts/privacy/run.sh [OUT_DIR]        # from anywhere; OUT_DIR defaults to docs/privacy/generated
#
# Needs what lez/ and vault/ need (RISC0_HOME for lez/, Foundry with vault/lib installed). The raw
# data (about 4 MB) goes to target/privacy/ (FERRATA_PRIVACY_DIR overrides) and is not committed.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
export FERRATA_PRIVACY_DIR="${FERRATA_PRIVACY_DIR:-$root/target/privacy}"
out="${1:-$root/docs/privacy/generated}"
rm -f "$FERRATA_PRIVACY_DIR"/{lez.json,scenario.json,truth.json,eth.jsonl}
(cd "$root/lez" && RISC0_DEV_MODE=1 cargo test -p harness --locked --test privacy_suite -- --nocapture)
(cd "$root/vault" && forge test --match-contract PrivacySuiteTest -vv)
python3 -I "$root/scripts/privacy/adversary.py" "$FERRATA_PRIVACY_DIR" "$out"
