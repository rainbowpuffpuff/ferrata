#!/usr/bin/env bash
# The ferrata CLI's end-to-end test: anvil (chain id 1, local only) for the vault, the LEZ harness for the
# zone (RISC0_DEV_MODE=1). Needs Foundry (anvil, forge) and the vault's libraries (installed if missing).
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="$HOME/.foundry/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
cd "$here/../vault"
[ -d lib/risc0-ethereum ] || forge install --no-git foundry-rs/forge-std@v1.10.0 OpenZeppelin/openzeppelin-contracts@v5.4.0 risc0/risc0-ethereum@v3.0.0
forge build --quiet
cd "$here"
FERRATA_E2E=1 RISC0_DEV_MODE=1 cargo test --test e2e -- --nocapture
