#!/usr/bin/env bash
# Read-only: checks vault/deployments/sepolia-31-B.json and vault/script/params/sepolia-31.json against Sepolia.
# Sends nothing. Usage, from the repository root: RPC=<a Sepolia endpoint> vault/deployments/sepolia-31-B.check.sh .
set -uo pipefail
wt="${1:?worktree}"; rec="$wt/vault/deployments/sepolia-31-B.json"; par="$wt/vault/script/params/sepolia-31.json"
rpc=(--rpc-url "${RPC:?}")
fail=0; n=0
ok() { n=$((n+1)); if [ "$2" = "$3" ]; then echo "ok    $1 = $2"; else echo "FAIL  $1: chain $2, want $3"; fail=$((fail+1)); fi; }
low() { tr 'A-F' 'a-f'; }
r() { jq -r "$1" "$rec"; }; p() { jq -r "$1" "$par"; }
c() { cast call "$@" "${rpc[@]}" 2>&1 | sed 's/ \[.*\]$//'; }
num() { sed 's/ \[.*\]$//'; }
ETH=0x0000000000000000000000000000000000000000
v=$(r .vault); s=$(r .emergencyStop); g=$(r .groth16Verifier); reg=$(r .registry); t=$(r .token); d=$(r .deployer)

ok "chain id" "$(cast chain-id "${rpc[@]}")" "$(r .chainId)"
ok "record complete, lastStep" "$(r .complete) $(r .lastStep)" "true done"
ok "params vault = record vault" "$(p .lezConfig.vault | low)" "$(echo "$v" | low)"
ok "params evmChainId = record chainId" "$(p .lezConfig.evmChainId)" "$(r .chainId)"
# Brief's addresses (owner's message, 14:12Z)
ok "vault (brief)" "$(echo "$v" | low)" "$(echo 0x19499061CeC0BaC2a3E1869Ba5AE6B403aA82787 | low)"
ok "verifier (brief)" "$(echo "$g" | low)" "$(echo 0x43C858f02f209c6C10c06B64B4ab2Cefd8B8dc96 | low)"
ok "stop (brief)" "$(echo "$s" | low)" "$(echo 0xC15916BDC7b32951B0310e0E24F98764EbA4b314 | low)"
ok "registry (brief)" "$(echo "$reg" | low)" "$(echo 0x03953f8888f2666f5DF572551a3141b4B94e0C13 | low)"
ok "token (brief)" "$(echo "$t" | low)" "$(echo 0x5d4DdB1863d2EE85765499dd850a9AeedecA03bC | low)"
# Creation order from the deployer's nonce 57 (deploy_sepolia.py --dry: verifier N, stop N+1, library N+2, vault N+3)
i=57; for k in groth16Verifier emergencyStop registry vault; do
  ok "$k = create(deployer, nonce $i)" "$(cast compute-address "$d" --nonce $i | awk '{print $NF}' | low)" "$(r .$k | low)"; i=$((i+1)); done
for k in groth16Verifier emergencyStop registry vault token; do
  a=$(r .$k); code=$(cast code "$a" "${rpc[@]}"); ok "code at $k ($a)" "$([ ${#code} -gt 2 ] && echo yes || echo no)" yes
done
ok "vault has code at block 12196188" "$([ "$(cast code "$v" --block 12196188 "${rpc[@]}" | wc -c)" -gt 3 ] && echo yes || echo no)" yes
ok "vault no code at block 12196187" "$(cast code "$v" --block 12196187 "${rpc[@]}" 2>&1 | tr -d '\n')" 0x
# Verifier, stop
ok "verifier VERSION" "$(c "$g" 'VERSION()(string)' | tr -d '"')" 3.1.0
ok "verifier SELECTOR" "$(c "$g" 'SELECTOR()(bytes4)')" 0xe71e6620
ok "stop owner (renounced)" "$(c "$s" 'owner()(address)')" "$ETH"
ok "stop paused" "$(c "$s" 'paused()(bool)')" false
ok "stop wraps verifier" "$(c "$s" 'verifier()(address)' | low)" "$(echo "$g" | low)"
"$wt/vault/script/check-verifier.sh" "$v" "$RPC" && echo "ok    check-verifier.sh" || { echo "FAIL  check-verifier.sh"; fail=$((fail+1)); }
# Vault
ok "vault verifier = stop" "$(c "$v" 'verifier()(address)' | low)" "$(echo "$s" | low)"
ok "checkpointImageId = params B" "$(c "$v" 'checkpointImageId()(bytes32)')" "$(p .checkpointImageIdB)"
ok "releaseImageId" "$(c "$v" 'releaseImageId()(bytes32)')" "$(p .releaseImageId)"
ok "lezConfigId" "$(c "$v" 'lezConfigId()(bytes32)')" "$(p .lezConfigId)"
ok "genesisState" "$(c "$v" 'genesisState()(bytes32)')" "$(p .genesisState)"
ok "mode (1 = rule B; record $(r .mode))" "$(c "$v" 'mode()(uint8)')" 1
ok "nMin" "$(c "$v" 'nMin()(uint32)')" "$(p .nMin)"
ok "tMin" "$(c "$v" 'tMin()(uint32)')" "$(p .tMin)"
ok "admin = deployer" "$(c "$v" 'admin()(address)' | low)" "$(echo "$d" | low)"
ok "freezeAuthority" "$(c "$v" 'freezeAuthority()(address)' | low)" "$(r .freezeAuthority | low)"
for i in 0 1 2; do echo "info  watchers($i) = $(c "$v" 'watchers(uint256)(address)' $i)"; done
ok "delta" "$(c "$v" 'delta()(uint256)' | num)" "$(r .delta)"
ok "deltaFloor = designDelta" "$(c "$v" 'deltaFloor()(uint256)' | num)" "$(r .designDelta)"
ok "feeRecipient" "$(c "$v" 'feeRecipient()(address)' | low)" "$(r .feeRecipient | low)"
ok "releaseCapFloor = releaseFloorDailyValue" "$(c "$v" 'releaseCapFloor()(uint256)' | num)" "$(r .releaseFloorDailyValue)"
for m in 0 1; do
  ok "globalCap($m) capacity" "$(c "$v" 'globalCap(uint8)(uint256,uint256,uint256)' $m | head -1 | num)" "$(r .globalDailyValue)"
  ok "ETH cap($m) capacity" "$(c "$v" 'tokenCap(address,uint8)(uint256,uint256,uint256)' $ETH $m | head -1 | num)" "$(r .ethDailyCap)"
  ok "token cap($m) capacity" "$(c "$v" 'tokenCap(address,uint8)(uint256,uint256,uint256)' "$t" $m | head -1 | num)" "$(r .tokenDailyCap)"
done
for k in revokeWindow maxFreeze minReleaseDelay; do echo "info  $k = $(c "$v" "$k()(uint256)")"; done
ok "ETH registered" "$(c "$v" 'tokenConfig(address)(bool,bool,uint256,uint256,uint256[])' $ETH | head -1)" true
ok "token registered" "$(c "$v" 'tokenConfig(address)(bool,bool,uint256,uint256,uint256[])' "$t" | head -1)" true
echo "info  ETH tokenConfig: $(c "$v" 'tokenConfig(address)(bool,bool,uint256,uint256,uint256[])' $ETH | tr '\n' ' ')"
echo "info  token tokenConfig: $(c "$v" 'tokenConfig(address)(bool,bool,uint256,uint256,uint256[])' "$t" | tr '\n' ' ')"
echo "info  token name/symbol/decimals: $(c "$t" 'name()(string)') $(c "$t" 'symbol()(string)') $(c "$t" 'decimals()(uint8)')"
echo "info  vault ETH balance: $(cast balance "$v" "${rpc[@]}")"
echo "info  depositRoot slot0: $(cast storage "$v" 0 "${rpc[@]}")"
echo "info  block now: $(cast block-number "${rpc[@]}")"
echo "info  deployer nonce now: $(cast nonce "$d" "${rpc[@]}")"
echo "checks: $n, failed: $fail"
[ "$fail" = 0 ]
