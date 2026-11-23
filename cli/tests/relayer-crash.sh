#!/usr/bin/env bash
# ferrata-relayer killed at each of its transactions, killed at random, and run behind failing endpoints
# and on its --send-via logos route (cli/tests/relayer_crash.rs; RFP-022 R.06). Local anvil only. Needs Foundry (anvil, forge), python3, the
# vault's libraries (installed if missing) and LEZ's toolchain as for tests/e2e.sh (the crate links LEZ's
# wallet). Results: e2e/runs/relayer-crash/results.json unless FERRATA_CRASH_RESULTS names another file.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="$HOME/.foundry/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
if [ -z "${RISC0_HOME:-}" ] && [ -d "$here/../../risc0-home-lez" ]; then export RISC0_HOME="$(cd "$here/../../risc0-home-lez" && pwd)"; fi
cd "$here/../vault"
[ -d lib/risc0-ethereum ] || forge install --no-git foundry-rs/forge-std@v1.10.0 OpenZeppelin/openzeppelin-contracts@v5.4.0 risc0/risc0-ethereum@v3.1.0
forge build --quiet
cd "$here"
mkdir -p "$here/../e2e/runs/relayer-crash"
# Part 5's daemon: the relayer with a stand-in for Logos Core's tx_sender_module (examples/relayer_logos_stub.rs).
cargo build --example relayer_logos_stub
FERRATA_RELAYER_CRASH=1 FERRATA_CRASH_RESULTS="${FERRATA_CRASH_RESULTS:-$here/../e2e/runs/relayer-crash/results.json}" \
  cargo test --test relayer_crash -- --nocapture
