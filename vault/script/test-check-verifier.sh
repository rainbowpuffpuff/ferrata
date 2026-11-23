#!/usr/bin/env bash
# script/check-verifier.sh on Sepolia's vaults of verifier 3.0.0 (capped at zero on 21 November 2026): each passes while
# both caps read 0, and fails once its admin has raised one. On a local anvil fork of Sepolia (a public endpoint, read
# only; nothing is sent to Sepolia: the cap is raised on the fork, the admin impersonated).
#
#   script/test-check-verifier.sh        FERRATA_FORK_URL: a Sepolia endpoint (default publicnode)
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
cd "$here"
export FOUNDRY_DISABLE_NIGHTLY_WARNING=1
fork_url="${FERRATA_FORK_URL:-https://ethereum-sepolia-rpc.publicnode.com}"
port="${FERRATA_FORK_PORT:-8548}"
rpc="http://127.0.0.1:$port"
head="$(cast block-number --rpc-url "$fork_url")"
anvil --fork-url "$fork_url" --fork-block-number "$((head - 5))" --port "$port" --silent &
anvil_pid=$!
trap 'kill "$anvil_pid" 2>/dev/null || true' EXIT
for _ in $(seq 1 90); do cast chain-id --rpc-url "$rpc" > /dev/null 2>&1 && break; sleep 1; done
echo "fork of Sepolia at block $((head - 5))"
# The four vaults of November 2026 (docs/security/risc0-groth16-advisory-2026-11-21/README.md).
for v in 0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c 0xfE3199fa2634413CbAC317371076Cae3EB2d9E54 \
         0x80555Fe53CCcEf46951B23A3c75643a4C9283470 0xd0B5fcC0B47D8E6f514f9D246Bb0b416b2048AF7; do
  script/check-verifier.sh "$v" "$rpc" || { echo "test-check-verifier: the capped vault $v did not pass"; exit 1; }
done
v=0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c
admin="$(cast call "$v" 'admin()(address)' --rpc-url "$rpc")"
cast rpc --rpc-url "$rpc" anvil_impersonateAccount "$admin" > /dev/null
cast rpc --rpc-url "$rpc" anvil_setBalance "$admin" 0xde0b6b3a7640000 > /dev/null
cast send --unlocked --from "$admin" --rpc-url "$rpc" "$v" 'setGlobalCap(uint8,uint256,uint256)' 1 1000000000000000000 0 > /dev/null
if script/check-verifier.sh "$v" "$rpc"; then echo "test-check-verifier: a raised release cap passed"; exit 1; fi
echo "TEST CHECK-VERIFIER OK: the four capped 3.0.0 vaults pass; one with its release cap raised (on the fork) fails"
