#!/usr/bin/env bash
# The Sepolia side of the crown run (e2e/crown/README.md), one stage per call, every transaction recorded.
#
#   e2e/crown/sepolia.sh <run dir> deploy | deposit <j> | submit | finalize | release <i> | status
#
# deploy   vault/script/deploy.sh with FERRATA_MODE=B and FERRATA_GAS_RULE=eip8037: the stop, the library, the vault,
#          the test token, ETH and the token registered with fees, the stop's owner renounced (one transaction per
#          forge run, each limit the node's estimate x margin), the Groth16 verifier reused (<run>/identity.json
#          .groth16Verifier); then the deployed vault is checked against <run>/identity.json.
# deposit, submit, finalize, release: vault/script/Crown.s.sol, one transaction each; forge asks the node for
#          eth_estimateGas right before it signs (--skip-simulation: Foundry's EVM does not price Sepolia's state gas)
#          and the limit is that estimate x FERRATA_GAS_MARGIN_PCT / 100; a stage the node cannot price sends nothing.
#
# Environment: SEPOLIA_RPC_URL, ETH_FROM; the key through Foundry's own ETH_KEYSTORE + ETH_PASSWORD (files), or
# FERRATA_UNLOCKED=1 on a local fork that impersonates ETH_FROM. FERRATA_GAS_MARGIN_PCT (default 125).
# Inputs in <run>: identity.json (vault, configId, genesisState, ...), params.json (DeployVault's), crown.json
# (deposits, candidate, releases). Records: deploy.json, manifest.json (every transaction: stage, hash, nonce,
# block, gasLimit, gasUsed, status), logs/.
set -euo pipefail
here="$(cd "$(dirname "$0")/../.." && pwd)"
run="$(cd "${1:?run directory}" && pwd)"; step="${2:?stage}"; arg="${3:-}"
: "${SEPOLIA_RPC_URL:?set SEPOLIA_RPC_URL}" "${ETH_FROM:?set ETH_FROM}"
export FOUNDRY_DISABLE_NIGHTLY_WARNING=1 TMPDIR="${TMPDIR:-$here/../tmp}"
margin="${FERRATA_GAS_MARGIN_PCT:-125}"
rpc=(--rpc-url "$SEPOLIA_RPC_URL")
id="$run/identity.json"; manifest="$run/manifest.json"; mkdir -p "$run/logs"
say() { echo "[$(date -u +%FT%TZ)] $*"; }
die() { say "STOP: $*"; exit 1; }
chain="$(cast chain-id "${rpc[@]}")"
[ "$chain" = "$(jq -r .chainId "$id")" ] || die "chain id $chain is not the run's"
vault="$(jq -r .vault "$id")"
call() { cast call "$vault" "$@" "${rpc[@]}"; }
signer=(); [ "${FERRATA_UNLOCKED:-}" = 1 ] && signer=(--unlocked)

# Appends the receipts of a forge broadcast file to the manifest under the stage's name.
record() {
  local name="$1" bfile="$2" rows='[]' h r
  [ -f "$manifest" ] || echo '{"chainId": '"$chain"', "stages": {}}' > "$manifest"
  for h in $(jq -r '.transactions[].hash' "$bfile"); do
    r="$(cast receipt "$h" --json "${rpc[@]}")"
    rows="$(jq -c --argjson r "$r" --arg h "$h" '. + [{hash: $h, blockNumber: ($r.blockNumber), status: ($r.status),
      gasUsed: ($r.gasUsed), to: ($r.to), contractAddress: ($r.contractAddress)}]' <<<"$rows")"
    [ "$(jq -r .status <<<"$r")" = 0x1 ] || die "$name: transaction $h failed (status $(jq -r .status <<<"$r"))"
  done
  jq --arg s "$name" --argjson rows "$rows" '.stages[$s] = ((.stages[$s] // []) + $rows)' "$manifest" > "$manifest.tmp"
  mv "$manifest.tmp" "$manifest"
}

crown() { # crown <signature> [args]: one transaction of Crown.s.sol
  local sig="$1"; shift
  local name="${sig%%(*}${1:+-$1}" log="$run/logs/${sig%%(*}${1:+-$1}.log"
  ( cd "$here/vault" && FERRATA_CROWN="$run/crown.json" forge script script/Crown.s.sol:Crown --sig "$sig" "$@" \
      "${rpc[@]}" --sender "$ETH_FROM" "${signer[@]}" --broadcast --skip-simulation --slow \
      --gas-estimate-multiplier "$margin" ) > "$log" 2>&1 || { tail -n 25 "$log" | sed "s,$SEPOLIA_RPC_URL,<endpoint>,g"; die "$name"; }
  local b="$here/vault/broadcast/Crown.s.sol/$chain/${sig%%(*}-latest.json"
  if [ -f "$b" ] && [ "$(find "$b" -newer "$run/logs/.mark-$name" 2>/dev/null)" ]; then
    record "$name" "$b"
  else
    say "$name: nothing sent (already done)"
  fi
}

case "$step" in
  status)
    say "chain $chain block $(cast block-number "${rpc[@]}") deployer nonce $(cast nonce "$ETH_FROM" "${rpc[@]}")"
    if [ "$(cast code "$vault" "${rpc[@]}")" = 0x ]; then say "vault $vault: not deployed"; exit 0; fi
    say "vault $vault: finalState $(call 'finalState()(bytes32)') leaves $(call 'nextLeafIndex()(uint64)')" \
      "checkpoints $(call 'checkpointCount()(uint256)')"
    ;;
  deploy)
    [ "$(cast nonce "$ETH_FROM" "${rpc[@]}")" = "$(cast nonce "$ETH_FROM" --block pending "${rpc[@]}")" ] ||
      die "the deployer has pending transactions"
    ( cd "$here/vault" && FERRATA_MODE=B FERRATA_GAS_RULE=eip8037 FERRATA_GAS_MARGIN_PCT="$margin" \
        FERRATA_RPC_URL="$SEPOLIA_RPC_URL" FERRATA_PARAMS="$run/params.json" FERRATA_OUT="$run/deploy.json" \
        FERRATA_GROTH16_VERIFIER="$(jq -r .groth16Verifier "$id")" FERRATA_DELTA="$(jq -r .delta "$id")" \
        FERRATA_LOG_DIR="$run/logs/deploy" script/deploy.sh ) > "$run/logs/deploy.log" 2>&1 ||
      { tail -n 30 "$run/logs/deploy.log" | sed "s,$SEPOLIA_RPC_URL,<endpoint>,g"; die deploy; }
    [ "$(jq -r .complete "$run/deploy.json")" = true ] || die "deploy.json is not complete"
    got="$(jq -r .vault "$run/deploy.json")"
    [ "${got,,}" = "${vault,,}" ] || die "the vault is at $got, the configuration names $vault"
    [ "$(call 'storedLezConfigId()(bytes32)')" = "$(jq -r .configId "$id")" ] || die "storedLezConfigId"
    [ "$(call 'genesisState()(bytes32)')" = "$(jq -r .genesisState "$id")" ] || die "genesisState"
    [ "$(call 'mode()(uint8)')" = 1 ] || die "the vault is not rule (B)"
    say "deploy complete: vault $vault, configId and genesisState as the run's identity"
    ;;
  deposit|release)
    [ -n "$arg" ] || die "$step needs an index"
    touch "$run/logs/.mark-$step-$arg"
    crown "$step(uint256)" "$arg"
    ;;
  submit|finalize)
    sig=submit; [ "$step" = finalize ] && sig=finalizeAttested
    touch "$run/logs/.mark-$sig"
    crown "$sig()"
    ;;
  *) sed -n 2,20p "$0"; exit 2 ;;
esac
