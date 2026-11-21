#!/usr/bin/env bash
# eth_estimateGas (read-only: nothing is signed or sent) of the owner's commands, from the vaults' admin, on Sepolia.
# Usage: estimate.sh RPC_URL OUTFILE (at the latest block). Exits 1 if any value is missing or an estimate failed.
set -u
RPC="$1"; OUT="$2"; : > "$OUT"
ADMIN=0x79A95aE9A3f9000eb41F7CBAB6a6F97826895C07
ETH=0x0000000000000000000000000000000000000000
W=0x4444444444444444444444444444444444444444; FA=0x3333333333333333333333333333333333333333
est() { local label="$1"; shift; local v; v=$(cast estimate --rpc-url "$RPC" --from $ADMIN "$@" 2>&1) || v="ERROR: $v"; printf '%s\t%s\n' "$label" "$(echo "$v" | tr '\n' ' ')" >> "$OUT"; sleep 0.3; }
echo -e "block\t$(cast block-number --rpc-url "$RPC")" >> "$OUT"
echo -e "baseFee\t$(cast base-fee --rpc-url "$RPC")" >> "$OUT"
echo -e "gasPrice\t$(cast gas-price --rpc-url "$RPC")" >> "$OUT"
echo -e "maxPriorityFee\t$(cast rpc --rpc-url "$RPC" eth_maxPriorityFeePerGas 2>&1)" >> "$OUT"
for pair in live3:0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c live1:0xfE3199fa2634413CbAC317371076Cae3EB2d9E54 live4:0x80555Fe53CCcEf46951B23A3c75643a4C9283470 v3:0xd0B5fcC0B47D8E6f514f9D246Bb0b416b2048AF7; do
  n=${pair%%:*}; V=${pair#*:}
  est $n.setGlobalCap.release $V "setGlobalCap(uint8,uint256,uint256)" 1 0 0
  est $n.setGlobalCap.deposit $V "setGlobalCap(uint8,uint256,uint256)" 0 0 0
  est $n.setDepositsEnabled.ETH $V "setDepositsEnabled(address,bool)" $ETH false
  [ $n = v3 ] || est $n.proposeRoles $V "proposeRoles(address[3],address)" "[$W,$ETH,$ETH]" $FA
done
FAILED=$(grep -c -E $'\t(ERROR|Error| ?$)' "$OUT")
[ "$FAILED" -eq 0 ] || { echo "estimate.sh: $FAILED lines of $OUT have no value or an error" >&2; exit 1; }
