#!/usr/bin/env bash
# Rehearses script/deploy.sh on a local anvil fork of Sepolia, then a deposit path and a refund path. No key is used
# and nothing is sent to Sepolia: anvil reads Sepolia's state over RPC and keeps every transaction to itself; the
# accounts are impersonated on the fork.
#   1. rule (A), RISC Zero's real Groth16 verifier behind its stop, gas rule eip7825: deploy; ETH and token deposits
#      with their fees.
#   2. a refusal: the next step with an own gas cap below its estimate fails and sends nothing.
#   3. rule (B), gas rule eip8037, an accept-all TEST verifier behind the stop (no proof exists for a vault at a fresh
#      address without a GPU prover): deploy; deposits; retire and a sunset candidate; finalizeAttested after the wait;
#      refunds after the revoke window, each returning the denomination and the fees it escrowed.
#
#   script/fork-rehearsal.sh [<log directory>]       FERRATA_FORK_URL: a public Sepolia RPC (default publicnode)
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
cd "$here"
export FOUNDRY_DISABLE_NIGHTLY_WARNING=1
logs="${1:-broadcast/fork-rehearsal}"
mkdir -p "$logs"
fork_url="${FERRATA_FORK_URL:-https://ethereum-sepolia-rpc.publicnode.com}"
port="${FERRATA_FORK_PORT:-8545}"
rpc="http://127.0.0.1:$port"
anvil --fork-url "$fork_url" --port "$port" > "$logs/anvil.log" 2>&1 &
anvil_pid=$!
trap 'kill "$anvil_pid" 2>/dev/null || true' EXIT
for _ in $(seq 1 90); do cast chain-id --rpc-url "$rpc" > /dev/null 2>&1 && break; sleep 1; done
chain="$(cast chain-id --rpc-url "$rpc")"
echo "fork of chain $chain at block $(cast block-number --rpc-url "$rpc")"

addr() { local h; h="$(cast keccak "$1")"; cast to-check-sum-address "0x${h:26:40}"; }
deployer="$(addr 'ferrata fork rehearsal: deployer')"
depositor="$(addr 'ferrata fork rehearsal: depositor')"
relayer="$(addr 'ferrata fork rehearsal: relayer')"
freeze="$(addr 'ferrata testnet: freeze authority (no key)')" # DeployVault's default freeze authority
for a in "$deployer" "$depositor" "$relayer" "$freeze"; do
  cast rpc --rpc-url "$rpc" anvil_impersonateAccount "$a" > /dev/null
  cast rpc --rpc-url "$rpc" anvil_setBalance "$a" 0x3635c9adc5dea00000 > /dev/null # 1000 ETH, on the fork only
done
export FERRATA_RPC_URL="$rpc" FERRATA_UNLOCKED=1 ETH_FROM="$deployer" FERRATA_LOG_DIR="$logs"
export FERRATA_DELTA=600 FERRATA_REVOKE_WINDOW=86400
rm -f "deployments/fork-$chain-A.json" "deployments/fork-$chain-B.json"

rehearse() { # <signature> [args...] [forge flags...]
  local sig="$1" log="$logs/$2"
  shift 2
  if ! env -u ETH_FROM forge script script/ForkRehearsal.s.sol:ForkRehearsal --sig "$sig" "$@" --rpc-url "$rpc" -vv > "$log" 2>&1; then
    tail -n 25 "$log" >&2
    echo "fork-rehearsal.sh: $sig failed (log $log)" >&2
    exit 1
  fi
  grep -h -o 'FERRATA_OK.*' "$log" || true
}
warp() { cast rpc --rpc-url "$rpc" evm_increaseTime "$1" > /dev/null && cast rpc --rpc-url "$rpc" evm_mine > /dev/null; }

echo "== 1. rule (A), real verifier, gas rule eip7825"
FERRATA_MODE=A FERRATA_GAS_RULE=eip7825 FERRATA_OUT="deployments/fork-$chain-A.json" script/deploy.sh
export FERRATA_OUT="deployments/fork-$chain-A.json"
rehearse "deposits()" A-deposits.log --unlocked --sender "$depositor" --broadcast
rehearse "checkDeposits(address)" A-check-deposits.log "$depositor"

echo "== 2. a refusal: an own cap of 100,000 gas under the next creation's estimate"
nonce="$(cast nonce "$deployer" --rpc-url "$rpc")"
rm -f "deployments/fork-$chain-refusal.json"
if FERRATA_MODE=A FERRATA_TX_GAS_CAP=100000 FERRATA_OUT="deployments/fork-$chain-refusal.json" \
  FERRATA_LOG_DIR="$logs/refusal" script/deploy.sh > "$logs/refusal.txt" 2>&1; then
  echo "fork-rehearsal.sh: the refusal did not happen" >&2
  exit 1
fi
grep -h -o -E "FERRATA_GAS .*|is above the cap[^\"]*" "$logs/refusal/"*.log > "$logs/refusal-lines.txt" || true
head -n 2 "$logs/refusal-lines.txt"
[ "$(cast nonce "$deployer" --rpc-url "$rpc")" = "$nonce" ] || { echo "fork-rehearsal.sh: something was sent" >&2; exit 1; }
echo "refused; the deployer's nonce is still $nonce"

echo "== 3. rule (B), test verifier, gas rule eip8037"
env -u ETH_FROM forge create script/ForkRehearsal.s.sol:RehearsalAcceptAllVerifier --rpc-url "$rpc" --unlocked --from "$deployer" \
  --broadcast > "$logs/test-verifier.log" 2>&1
test_verifier="$(awk '/Deployed to:/ {print $3}' "$logs/test-verifier.log")"
echo "test verifier $test_verifier"
FERRATA_MODE=B FERRATA_GAS_RULE=eip8037 FERRATA_GROTH16_VERIFIER="$test_verifier" \
  FERRATA_OUT="deployments/fork-$chain-B.json" script/deploy.sh
export FERRATA_OUT="deployments/fork-$chain-B.json"
rehearse "deposits()" B-deposits.log --unlocked --sender "$depositor" --broadcast
rehearse "checkDeposits(address)" B-check-deposits.log "$depositor"
rehearse "sunset()" B-sunset.log --unlocked --sender "$freeze" --broadcast
warp $((FERRATA_DELTA + 1))
rehearse "finalizeSunset()" B-finalize.log --unlocked --sender "$relayer" --broadcast
warp $((FERRATA_REVOKE_WINDOW + 1))
before="$(cast balance "$depositor" --rpc-url "$rpc")"
rehearse "refunds(address)" B-refunds.log "$depositor" --unlocked --sender "$relayer" --broadcast
rehearse "checkRefunds(address,uint256)" B-check-refunds.log "$depositor" "$before"
cp "deployments/fork-$chain-A.json" "deployments/fork-$chain-B.json" "$logs/"
echo "FORK REHEARSAL OK"
