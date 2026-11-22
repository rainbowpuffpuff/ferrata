#!/usr/bin/env bash
# RFP-022 P.7, the Ethereum side: storage slots each vault operation creates (vault/test/StateGrowth.t.sol, Foundry's
# state-diff recording), on the build box. Writes lez/results/perf-state-ethereum.txt.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
export PATH="$HOME/.foundry/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
cd "$REPO/vault"
[ -d lib/risc0-ethereum ] || forge install --no-git foundry-rs/forge-std@v1.10.0 OpenZeppelin/openzeppelin-contracts@v5.4.0 risc0/risc0-ethereum@v3.0.0 > /dev/null 2>&1
{ forge --version | head -1; forge test --match-contract StateGrowthTest --match-test test_stateGrowth -vv; } > "$REPO/lez/results/perf-state-ethereum.txt" 2>&1
grep -E -A2 'GROWTH|PASS|FAIL' "$REPO/lez/results/perf-state-ethereum.txt" | head -60
