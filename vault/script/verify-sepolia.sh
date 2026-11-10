#!/usr/bin/env bash
# Re-checks a round trip's manifest against any RPC endpoint of its chain: every transaction is mined
# and succeeded with the recorded gas, the contracts are wired as the design says, the vault holds
# the fixture's configuration and state, every seal still verifies through the emergency stop, and
# the releases reached their recipients. Reads only.
#
#   SEPOLIA_RPC_URL=<endpoint> script/verify-sepolia.sh [manifest.json] [fixture.json]
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
export TMPDIR="${TMPDIR:-$here/../../tmp}" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
: "${SEPOLIA_RPC_URL:?set SEPOLIA_RPC_URL}"
manifest="${1:-$here/../e2e/runs/sepolia/manifest.json}"
fixture="${2:-$(dirname "$manifest")/fixture.json}"
rpc=(--rpc-url "$SEPOLIA_RPC_URL")
fail=0
check() { if [ "$2" = "$3" ]; then echo "ok    $1"; else echo "FAIL  $1: got $2, want $3"; fail=1; fi; }
lower() { tr 'A-F' 'a-f'; }
f() { jq -r "$1" "$fixture"; }

check "chain id" "$(cast chain-id "${rpc[@]}")" "$(jq -r .chainId "$manifest")"
check "manifest and fixture name the same vault" "$(jq -r .vault "$manifest" | lower)" "$(f .vault | lower)"
vault="$(f .vault)"; estop="$(f .emergencyStop)"; groth16="$(f .groth16Verifier)"; token="$(f .token)"

while IFS=$'\t' read -r stage hash block gas status; do
  receipt="$(cast receipt "$hash" --json "${rpc[@]}")"
  check "$stage $hash mined in block $(cast to-dec "$block")" "$(jq -r .blockNumber <<<"$receipt")" "$block"
  check "$stage $hash succeeded" "$(jq -r .status <<<"$receipt")" "$status"
  check "$stage $hash gas $(cast to-dec "$gas")" "$(jq -r .gasUsed <<<"$receipt")" "$gas"
done < <(jq -r '.stages | to_entries[] | .key as $s | .value[] | [$s, .hash, .blockNumber, .gasUsed, .status] | @tsv' "$manifest")

for a in "$groth16" "$estop" "$token" "$vault"; do
  [ "$(cast code "$a" "${rpc[@]}")" != 0x ] && echo "ok    code at $a" || { echo "FAIL  no code at $a"; fail=1; }
done
call() { cast call "$@" "${rpc[@]}"; }
check "vault.verifier is the emergency stop" "$(call "$vault" 'verifier()(address)' | lower)" "$(echo "$estop" | lower)"
check "the stop wraps the Groth16 verifier" "$(call "$estop" 'verifier()(address)' | lower)" "$(echo "$groth16" | lower)"
check "the stop is not paused" "$(call "$estop" 'paused()(bool)')" false
check "Groth16 verifier selector" "$(call "$groth16" 'SELECTOR()(bytes4)')" 0x73c457ba
check "checkpointImageId" "$(call "$vault" 'checkpointImageId()(bytes32)')" "$(f .checkpointImageId)"
check "releaseImageId" "$(call "$vault" 'releaseImageId()(bytes32)')" "$(f .releaseImageId)"
check "lezConfigId" "$(call "$vault" 'lezConfigId()(bytes32)')" "$(f .lezConfigId)"
check "slot 2 holds lezConfigId" "$(cast storage "$vault" 2 "${rpc[@]}")" "$(f .lezConfigId)"
check "genesisState" "$(call "$vault" 'genesisState()(bytes32)')" "$(f .genesisState)"
check "deposit root (slot 0)" "$(cast storage "$vault" 0 "${rpc[@]}")" "$(f .depositRoot)"

# Every seal, checked again by the deployed verifier through the stop (a view call).
verify() { call "$estop" 'verify(bytes,bytes32,bytes32)' "$2" "$3" "$4" > /dev/null 2>&1 && echo "ok    $1 seal verifies on chain" || { echo "FAIL  $1 seal does not verify"; fail=1; }; }
[ "$(f '.candidate.seal // empty')" ] && verify candidate "$(f .candidate.seal)" "$(f .checkpointImageId)" "$(f .candidate.journalDigest)"
[ "$(f '.finalize.seal // empty')" ] && verify finalisation "$(f .finalize.seal)" "$(f .checkpointImageId)" "$(f .finalize.journalDigest)"

if jq -e '.stages.finalize' "$manifest" > /dev/null; then
  check "finalState is the checkpoint's new state" "$(call "$vault" 'finalState()(bytes32)')" "$(f .newState)"
  check "the burn root is accepted" "$(call "$vault" 'burnRootStatus(bytes32)(bool,uint256)' "$(f .burnRoot)" | head -1)" true
fi
for i in $(seq 0 $(($(f .releaseCount) - 1))); do
  r=".releases[$i]"; name="$(f "$r.name")"
  [ "$(f "$r.seal // empty")" ] && verify "release $name" "$(f "$r.seal")" "$(f .releaseImageId)" "$(f "$r.journalDigest")"
  jq -e '.stages.release' "$manifest" > /dev/null || continue
  check "release $name: nullifier used" "$(call "$vault" 'nullifierUsed(bytes32)(bool)' "$(f "$r.nullifier")")" true
  paid=$(($(cast to-dec "$(f "$r.amount")") - $(cast to-dec "$(f "$r.relayFee")")))
  if [ "$(f "$r.token")" = 0x0000000000000000000000000000000000000000 ]; then
    have="$(cast balance "$(f "$r.recipient")" "${rpc[@]}")"
  else
    have="$(call "$(f "$r.token")" 'balanceOf(address)(uint256)' "$(f "$r.recipient")" | awk '{print $1}')"
  fi
  [ "$have" -ge "$paid" ] && echo "ok    release $name: the recipient holds $have (paid $paid)" || { echo "FAIL  release $name: recipient holds $have, paid $paid"; fail=1; }
done
[ "$fail" = 0 ] && echo "ALL CHECKS PASSED" || { echo "SOME CHECKS FAILED"; exit 1; }
