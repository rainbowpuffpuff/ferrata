#!/usr/bin/env bash
# ferrata_bridge_module's Ethereum side checked natively against the real vault on a local anvil (nothing public):
# the module's vault reads (calls/src/evm.rs, through `admin` with `what`) and its admin calls (calls/src/admin.rs,
# built and checked with eth_call from the role's account) compared with Foundry's `cast` on the same chain. The
# calls the module builds are sent here with `cast` from anvil's unlocked accounts; through Logos Core's send stack
# they are sent by modules/scripts/bridge-send-stack.sh. Box only (spike/modal-build/mrun); Foundry 1.3.6.
#
#   modules/scripts/bridge-eth-native.sh OUT     (after MODULES=ferrata_bridge_module modules/scripts/native.sh)
#
# Accounts: anvil's well-known test accounts 1 (admin, the deployer), 2 (freeze authority), 3 (the user).
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
OUT=$(mkdir -p "${1:?OUT}" && cd "$1" && pwd)
export PATH="$HOME/.foundry/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
CALL="$repo/modules/ferrata_bridge_module/rust-lib/target/release/examples/call"
[ -x "$CALL" ] || { echo "no $CALL: run native.sh first"; exit 2; }
RPC=http://127.0.0.1:18546
K1=59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d
A1=0x70997970c51812dc3a010c7d01b50e0d17dc79c8
A2=0x3c44cdddb6a900fa2b585dd299e03d12fa4293bc
A3=0x90f79bf6eb2c4f870365e785982e1f101e93b906
ETH=0x0000000000000000000000000000000000000000
pass=0; fail=0
check() { # NAME EXPECTED ACTUAL
  if [ "$2" = "$3" ]; then pass=$((pass + 1)); echo "PASS $1"; else fail=$((fail + 1)); echo "FAIL $1: expected [$2], got [$3]"; fi
  printf '%s\t%s\t%s\t%s\n' "$([ "$2" = "$3" ] && echo PASS || echo FAIL)" "$1" "$2" "$3" >> "$OUT/checks.tsv"
}
: > "$OUT/checks.tsv"; : > "$OUT/calls.jsonl"
# m REQUEST_JSON: the module's `admin` natively; the reply on stdout, both in calls.jsonl.
m() { echo "$1" > "$OUT/req.json"; local r; r=$("$CALL" admin "$OUT/req.json"); jq -c -n --argjson q "$1" --argjson a "$r" '{request: $q, reply: $a}' >> "$OUT/calls.jsonl"; echo "$r"; }
anvil --port 18546 --chain-id 11155111 > "$OUT/anvil.log" 2>&1 & ANVIL=$!
trap 'kill $ANVIL 2>/dev/null' EXIT
for _ in $(seq 60); do cast chain-id --rpc-url $RPC > /dev/null 2>&1 && break; sleep 0.5; done
( cd "$repo/vault" && { [ -d lib/risc0-ethereum ] || forge install --no-git foundry-rs/forge-std@v1.10.0 OpenZeppelin/openzeppelin-contracts@v5.4.0 risc0/risc0-ethereum@v3.0.0; } \
  && FERRATA_FREEZE_AUTHORITY=$A2 forge script script/LogosSendStack.s.sol:LogosSendStack --rpc-url $RPC --broadcast --private-key $K1 ) > "$OUT/deploy.log" 2>&1 \
  || { tail -n 30 "$OUT/deploy.log"; exit 1; }
V=$(grep -E '^\s+vault 0x' "$OUT/deploy.log" | awk '{print $2}' | tr 'A-F' 'a-f')
T=$(grep -E '^\s+token 0x' "$OUT/deploy.log" | awk '{print $2}' | tr 'A-F' 'a-f')
echo "vault $V token $T" | tee "$OUT/addresses.txt"
E="{\"via\":\"http\",\"rpc_url\":\"$RPC\"}"
low() { tr 'A-F' 'a-f'; }

# 1. status: the roles and ETH's registry entry, as cast reads them.
S=$(m "{\"action\":\"status\",\"vault\":\"$V\",\"eth\":$E,\"tokens\":[\"$ETH\",\"$T\"]}")
check "status: admin" "$(cast call $V 'admin()(address)' --rpc-url $RPC | low)" "$(echo "$S" | jq -r .roles.admin)"
check "status: freeze authority" "$(cast call $V 'freezeAuthority()(address)' --rpc-url $RPC | low)" "$(echo "$S" | jq -r .roles.freeze_authority)"
check "status: ETH denominations" "100000000000000000,1000000000000000000,10000000000000000000" "$(echo "$S" | jq -r '[.registry[0].token.denominations[].amount] | join(",")')"
check "status: ETH relay fee 0" "$(cast call $V 'fees(address,uint256,uint8)(uint256,uint256,uint256,uint256)' $ETH 0 0 --rpc-url $RPC | sed -n 2p | awk '{print $1}')" "$(echo "$S" | jq -r '.registry[0].token.denominations[0].relay_fee')"
check "status: test token not registered" "false" "$(echo "$S" | jq -r '.registry[1].token.registered')"
check "status: reads through http" "http" "$(echo "$S" | jq -r .eth.via)"

# 2. the test token registered by the admin: the module builds and checks it, cast sends it.
REG="{\"token\":\"$T\",\"denominations\":[\"1000000\",\"10000000\"],\"relay_fees\":[\"1000\",\"10000\"],\"lez_unit\":\"1\",\"unit_weight\":\"1\",\
\"deposit_cap\":{\"capacity\":\"1000000000000\",\"refill_per_second\":\"1000\"},\"release_cap\":{\"capacity\":\"1000000000000\",\"refill_per_second\":\"1000\"},\
\"release_delay\":\"120\",\"lez_definition\":\"0x$(printf '%064d' 0)\",\"decimals\":6,\"checkpoint_fees\":[\"0\",\"0\"]}"
B=$(m "{\"action\":\"register_token\",\"vault\":\"$V\",\"eth\":$E,\"params\":$REG,\"send\":{\"from\":\"$A1\"}}")
check "register_token: built and checked from the admin" "built $A1" "$(echo "$B" | jq -r '.verdict + " " + .checked_from')"
cast send $V "$(echo "$B" | jq -r .call.data)" --from $A1 --unlocked --rpc-url $RPC > "$OUT/send-register.log" 2>&1
check "register_token: the vault holds it" "true" "$(cast call $V 'tokenConfig(address)(bool,bool,uint256,uint256,uint256[])' $T --rpc-url $RPC | head -1)"
S=$(m "{\"action\":\"read\",\"what\":\"registry\",\"token\":\"$T\",\"vault\":\"$V\",\"eth\":$E}")
check "register_token: denominations read back" "1000000,10000000" "$(echo "$S" | jq -r '[.value.token.denominations[].amount] | join(",")')"
check "register_token: deposit cap read back" "1000000000000" "$(echo "$S" | jq -r '.value.token.vault_deposit_cap.capacity')"
R=$(m "{\"action\":\"register_token\",\"vault\":\"$V\",\"eth\":$E,\"params\":$REG,\"send\":{\"from\":\"$A1\"}}")
check "register_token again: the vault's error named" "TokenAlreadyRegistered" "$(echo "$R" | jq -r '.error' | grep -o 'TokenAlreadyRegistered' | head -1)"
R=$(m "{\"action\":\"set_token_cap\",\"vault\":\"$V\",\"eth\":$E,\"params\":{\"token\":\"$T\",\"direction\":\"deposit\",\"capacity\":\"5\",\"refill_per_second\":\"1\"},\"send\":{\"from\":\"$A3\"}}")
check "set_token_cap from the user: refused before sending" "yes" "$(echo "$R" | jq -r '.error' | grep -q 'is not the vault.s admin' && echo yes)"

# 3. the admin's cap and the freeze authority's freeze.
C=$(m "{\"action\":\"set_token_cap\",\"vault\":\"$V\",\"eth\":$E,\"params\":{\"token\":\"$T\",\"direction\":\"deposit\",\"capacity\":\"2000000000000\",\"refill_per_second\":\"5000\"},\"send\":{\"from\":\"$A1\"}}")
cast send $V "$(echo "$C" | jq -r .call.data)" --from $A1 --unlocked --rpc-url $RPC > "$OUT/send-cap.log" 2>&1
check "set_token_cap: cast reads the new capacity" "2000000000000" "$(cast call $V 'tokenCap(address,uint8)(uint256,uint256,uint256)' $T 0 --rpc-url $RPC | head -1 | awk '{print $1}')"
F=$(m "{\"action\":\"set_token_deposits_frozen\",\"vault\":\"$V\",\"eth\":$E,\"params\":{\"token\":\"$ETH\",\"frozen\":true},\"send\":{\"from\":\"$A2\"}}")
check "freeze: built and checked from the freeze authority" "built" "$(echo "$F" | jq -r .verdict)"
cast send $V "$(echo "$F" | jq -r .call.data)" --from $A2 --unlocked --rpc-url $RPC > "$OUT/send-freeze.log" 2>&1
S=$(m "{\"action\":\"read\",\"what\":\"registry\",\"token\":\"$ETH\",\"vault\":\"$V\",\"eth\":$E}")
check "freeze: the module reads the vault's until" "$(cast call $V 'tokenDepositsFrozenUntil(address)(uint256)' $ETH --rpc-url $RPC | awk '{print $1}')" "$(echo "$S" | jq -r .value.token.deposits_frozen_until)"
check "freeze: it froze (until later than the latest block's time)" "yes" "$(u=$(echo "$S" | jq -r .value.token.deposits_frozen_until); n=$(cast block latest --field timestamp --rpc-url $RPC); [ "$u" -gt "$n" ] && echo yes || echo "no: until $u, now $n")"
F=$(m "{\"action\":\"set_token_deposits_frozen\",\"vault\":\"$V\",\"eth\":$E,\"params\":{\"token\":\"$ETH\",\"frozen\":false},\"send\":{\"from\":\"$A2\"}}")
cast send $V "$(echo "$F" | jq -r .call.data)" --from $A2 --unlocked --rpc-url $RPC > "$OUT/send-unfreeze.log" 2>&1
check "unfreeze: lifted" "0" "$(cast call $V 'tokenDepositsFrozenUntil(address)(uint256)' $ETH --rpc-url $RPC | awk '{print $1}')"

# 4. deposits (sent with cast here) and the module's deposit list, count and price.
P=$(m "{\"action\":\"read\",\"what\":\"price\",\"token\":\"$ETH\",\"denomination\":0,\"vault\":\"$V\",\"eth\":$E}")
TOTAL=$(echo "$P" | jq -r .value.total)
check "price: denomination + fees" "100000000000000000" "$(echo "$P" | jq -r .value.denomination)"
C1=0x$(printf 'c1%.0s' $(seq 32))
cast send $V 'depositETH(uint8,bytes32,uint64)' 0 $C1 0 --value $TOTAL --from $A3 --unlocked --rpc-url $RPC > "$OUT/send-deposit-eth.log" 2>&1
cast send $T 'mint(address,uint256)' $A3 100000000 --from $A3 --unlocked --rpc-url $RPC > /dev/null 2>&1
PT=$(m "{\"action\":\"read\",\"what\":\"price\",\"token\":\"$T\",\"denomination\":0,\"vault\":\"$V\",\"eth\":$E}")
TT=$(echo "$PT" | jq -r .value.total)
cast send $T 'approve(address,uint256)' $V $TT --from $A3 --unlocked --rpc-url $RPC > /dev/null 2>&1
C2=0x$(printf 'c2%.0s' $(seq 32))
cast send $V 'deposit(address,uint8,bytes32,uint64)' $T 0 $C2 1 --from $A3 --unlocked --rpc-url $RPC > "$OUT/send-deposit-erc20.log" 2>&1
L=$(m "{\"action\":\"read\",\"what\":\"deposit_list\",\"vault\":\"$V\",\"eth\":$E}")
check "deposit list: two deposits, checked against depositRoot" "2" "$(echo "$L" | jq -r '.value.deposits | length')"
check "deposit list: the root is the vault's" "$(cast call $V 'depositRoot()(bytes32)' --rpc-url $RPC)" "$(echo "$L" | jq -r .value.root)"
check "deposit list: leaf 0 is the user's ETH" "0 $A3 100000000000000000 $C1" "$(echo "$L" | jq -r '.value.deposits[0] | "\(.leaf_index) \(.depositor) \(.amount) \(.commitment)"')"
check "deposit list: leaf 1 the test token's" "1 $T 1000000 $C2" "$(echo "$L" | jq -r '.value.deposits[1] | "\(.leaf_index) \(.token) \(.amount) \(.commitment)"')"
N=$(m "{\"action\":\"read\",\"what\":\"deposit_count\",\"depositor\":\"$A3\",\"vault\":\"$V\",\"eth\":$E}")
check "deposit count of the user" "2" "$(echo "$N" | jq -r .value)"
for w in roots released refunded; do
  X=$(m "{\"action\":\"read\",\"what\":\"$w\",\"vault\":\"$V\",\"eth\":$E}")
  check "read $w: answered (nothing yet)" "0" "$(echo "$X" | jq -r '.value | if type == "object" then (.roots | length) else length end')"
done
L2=$(m "{\"action\":\"lez_set_cap\",\"program\":\"0x$(printf '22%.0s' $(seq 32))\",\"config_id\":\"0x$(printf '33%.0s' $(seq 32))\",\"params\":{\"token\":null,\"mints\":true,\"capacity\":\"11000000000\",\"refill_per_second\":\"10000000\"}}")
check "LEZ SetCap message built (variant 18)" "built 0x12" "$(echo "$L2" | jq -r '.verdict + " " + (.message[0:4])')"
echo "checks: $pass passed, $fail failed" | tee "$OUT/summary.txt"
[ "$fail" -eq 0 ]
