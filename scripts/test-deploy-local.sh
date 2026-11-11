#!/usr/bin/env bash
# The test of scripts/deploy-testnet.sh: a release deployed for real on a local EVM chain (anvil), the LEZ side
# planned, and the refusals. Nothing leaves this machine. Records go to vault/deployments/fork-test-* (ignored by git).
#
#   scripts/test-deploy-local.sh RELEASE_DIR WORK_DIR
#
# Checks, each printed as PASS or FAIL (exit 1 on any FAIL):
#   1. plan only (LEZ and EVM): exit 0, nothing deployed
#   2. --send on anvil (mode B): the vault deployed, its checkpoint and release image ids equal to the release's
#   3. a tampered release (one bit of a program flipped) is refused before anything is sent
#   4. a wrong --chain-id is refused
#   5. --chain-id 1 (Ethereum mainnet) is refused
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
REL=$(cd "${1:?RELEASE_DIR}" && pwd); W=${2:?WORK_DIR}
mkdir -p "$W"; W=$(cd "$W" && pwd)
D="$ROOT/scripts/deploy-testnet.sh"
PORT=${ANVIL_PORT:-18545}; RPC=http://127.0.0.1:$PORT
fails=0
check() { if [ "$1" = 0 ]; then echo "PASS $2"; else echo "FAIL $2"; fails=$((fails + 1)); fi; }

anvil --port "$PORT" --chain-id 31337 --auto-impersonate --silent > "$W/anvil.log" 2>&1 & A=$!
trap 'kill $A 2>/dev/null' EXIT
for i in $(seq 50); do cast chain-id --rpc-url "$RPC" >/dev/null 2>&1 && break; sleep 0.2; done
# anvil's first funded account (a well-known test key); impersonated, no key is used
export ETH_FROM=0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266 FERRATA_UNLOCKED=1

"$D" --release "$REL" --lez http://127.0.0.1:9 --evm-rpc "$RPC" --chain-id 31337 --name fork-test-plan > "$W/1-plan.log" 2>&1
r=$?; n=$(cast block-number --rpc-url "$RPC")
check $(( r != 0 || n != 0 )) "1. plan only: exit $r, block number still $n"

"$D" --release "$REL" --evm-rpc "$RPC" --chain-id 31337 --skip-lez --mode B --send --name fork-test-send > "$W/2-send.log" 2>&1
r=$?
ok=$(grep -c "= the release's" "$W/2-send.log")
check $(( r != 0 || ok != 2 )) "2. --send on anvil: exit $r, $ok of 2 image ids read back equal ($(grep -o 'vault 0x[0-9a-fA-F]*' "$W/2-send.log" | head -1))"

cp -r "$REL" "$W/tampered"
python3 -I - "$W/tampered/programs/release.bin" <<'PY'
import sys
p = sys.argv[1]; b = bytearray(open(p, "rb").read()); b[len(b) // 2] ^= 1; open(p, "wb").write(bytes(b))
PY
"$D" --release "$W/tampered" --evm-rpc "$RPC" --chain-id 31337 --skip-lez --send --name fork-test-tampered > "$W/3-tampered.log" 2>&1
r=$?
check $(( r == 0 )) "3. tampered release refused: exit $r, $(grep -m1 -E 'does not verify|FAILED|differs' "$W/3-tampered.log" | cut -c1-80)"

"$D" --release "$REL" --evm-rpc "$RPC" --chain-id 11155111 --skip-lez --send --name fork-test-wrong-chain > "$W/4-chain.log" 2>&1
r=$?
check $(( r == 0 )) "4. wrong chain id refused: exit $r, $(grep -m1 'chain id' "$W/4-chain.log" | cut -c1-90)"

"$D" --release "$REL" --evm-rpc "$RPC" --chain-id 1 --skip-lez --name fork-test-mainnet > "$W/5-mainnet.log" 2>&1
r=$?
check $(( r == 0 )) "5. chain id 1 refused: exit $r, $(grep -m1 'refused' "$W/5-mainnet.log" | cut -c1-90)"

echo "$fails failed"
[ "$fails" = 0 ]
