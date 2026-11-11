#!/usr/bin/env bash
# The relayer's tests (cli/src/relayer.rs, cli/src/bin/relayer.rs):
#  - tests/gasdrop.rs: the gas-drop checks on a real receipt (bridge/artifacts/gasdrop/), no dev mode;
#  - tests/relayer_fork.rs: the ferrata-relayer daemon on an anvil fork of Sepolia at the block before
#    crown-live-1's submitCandidate, with that run's real checkpoint and release proofs; a misbehaving
#    relayer refused. It reads public Sepolia state (FERRATA_FORK_URL, or the endpoints in the test) and
#    sends nothing to Sepolia. Results: e2e/runs/relayer-fork/results.json.
# Needs Foundry's anvil, and LEZ's toolchain as for tests/e2e.sh (the crate links LEZ's wallet).
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="$HOME/.foundry/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
if [ -z "${RISC0_HOME:-}" ] && [ -d "$here/../../risc0-home-lez" ]; then export RISC0_HOME="$(cd "$here/../../risc0-home-lez" && pwd)"; fi
cd "$here"
unset RISC0_DEV_MODE
cargo test --test gasdrop -- --nocapture
mkdir -p "$here/../e2e/runs/relayer-fork"
FERRATA_RELAYER_FORK=1 FERRATA_RELAYER_RESULTS="${FERRATA_RELAYER_RESULTS:-$here/../e2e/runs/relayer-fork/results.json}" \
  cargo test --test relayer_fork -- --nocapture
