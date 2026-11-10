#!/usr/bin/env bash
# The return leg on Sepolia (or on a local fork of it), one stage per call, with a manifest of every
# transaction. The stages and the waits between them are in vault/README.md ("Sepolia round trip").
#
#   script/roundtrip.sh deploy | submit | beacon | finalize | release | status
#
# Environment:
#   SEPOLIA_RPC_URL   the RPC endpoint
#   ETH_FROM          the sender's address: the fixture's deployer
#   the key, through Foundry's own variables, never on a command line and never in this repository:
#     ETH_KEYSTORE + ETH_PASSWORD   a keystore file and a file holding its password, or
#     ETH_KEYSTORE_ACCOUNT          a name under ~/.foundry/keystores (forge asks for the password)
#   FERRATA_UNLOCKED=1  no key: the endpoint is a local fork that impersonates ETH_FROM
#   FERRATA_FIXTURE   the run's fixture.json (default ../e2e/runs/sepolia/fixture.json)
#   FERRATA_MANIFEST  where transactions are recorded (default manifest.json beside the fixture)
#   FERRATA_DELTA, FERRATA_RELEASE_DELAY, FERRATA_REVOKE_WINDOW, FERRATA_MAX_FREEZE,
#   FERRATA_MIN_RELEASE_DELAY, FERRATA_FREEZE_AUTHORITY   the testnet configuration (deploy only;
#     defaults 600 s, 120 s, 1 day, 30 days, 0, an address without a key)
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
cd "$here"
export TMPDIR="${TMPDIR:-$here/../../tmp}" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
mkdir -p "$TMPDIR"
step="${1:?stage: deploy | submit | beacon | finalize | release | status}"
: "${SEPOLIA_RPC_URL:?set SEPOLIA_RPC_URL}"
export FERRATA_FIXTURE="${FERRATA_FIXTURE:-$here/../e2e/runs/sepolia/fixture.json}"
fixture="$FERRATA_FIXTURE"
manifest="${FERRATA_MANIFEST:-$(dirname "$fixture")/manifest.json}"
rpc=(--rpc-url "$SEPOLIA_RPC_URL")
chain="$(cast chain-id "${rpc[@]}")"
[ "$chain" = "$(jq -r .chainId "$fixture")" ] || { echo "chain id $chain is not the fixture's" >&2; exit 1; }
vault="$(jq -r .vault "$fixture")"
call() { cast call "$vault" "$@" "${rpc[@]}"; }

if [ "$step" = status ]; then
  now="$(cast block latest --field timestamp "${rpc[@]}")"
  echo "chain $chain, block $(cast block-number "${rpc[@]}"), time $now"
  if [ "$(cast code "$vault" "${rpc[@]}")" = 0x ]; then echo "vault $vault: not deployed"; exit 0; fi
  state="$(jq -r .newState "$fixture")"
  echo "vault $vault: finalState $(call 'finalState()(bytes32)'), $(call 'beaconCount()(uint256)') beacon(s), $(call 'checkpointCount()(uint256)') checkpoint(s)"
  read -r submitted delta <<<"$(call 'candidates(bytes32)(bytes32,bytes32,uint64,uint64,bool,uint32,uint64,bytes32,uint256,uint256,uint256)' "$state" | sed -n '9p;10p' | awk '{print $1}' | tr '\n' ' ')"
  if [ "${submitted:-0}" != 0 ]; then
    over=$((submitted + delta + 2 * $(call 'turnSeconds()(uint256)' | awk '{print $1}')))
    echo "candidate $state: submitted at $submitted, wait over at $over ($((over - now)) s from now)"
  fi
  echo "burn root $(jq -r .burnRoot "$fixture"): $(call 'burnRootStatus(bytes32)(bool,uint256)' "$(jq -r .burnRoot "$fixture")" | tr '\n' ' ')"
  exit 0
fi

: "${ETH_FROM:?set ETH_FROM to the deployer address}"
signer=()
[ "${FERRATA_UNLOCKED:-}" = 1 ] && signer=(--unlocked)
# `record <stage>` only rewrites the manifest entry of a stage from forge's broadcast log.
if [ "$step" = record ]; then
  step="${2:?stage}"
else
  case "$step" in deploy | submit | beacon | finalize | release) ;; *) echo "unknown stage $step" >&2; exit 2 ;; esac
  forge script script/RoundTrip.s.sol:RoundTrip --sig "$step()" "${rpc[@]}" --sender "$ETH_FROM" "${signer[@]}" \
    --broadcast --slow
fi

# The manifest: what forge broadcast and the receipts it got back.
log="broadcast/RoundTrip.s.sol/$chain/$step-latest.json"
[ -f "$manifest" ] || jq -n --argjson chain "$chain" --arg vault "$vault" --arg from "$ETH_FROM" \
  '{chainId: $chain, vault: $vault, sender: $from, stages: {}}' > "$manifest"
jq --arg step "$step" --slurpfile b "$log" '
  .stages[$step] = ([$b[0].transactions, $b[0].receipts] | transpose | map({
    hash: .[0].hash,
    what: ([.[0].contractName, .[0].function, .[0].transactionType] | map(select(. != null)) | join(" ")),
    contractAddress: (.[1].contractAddress // null), to: (.[1].to // null),
    blockNumber: .[1].blockNumber, gasUsed: .[1].gasUsed, effectiveGasPrice: .[1].effectiveGasPrice,
    status: .[1].status}))' "$manifest" > "$manifest.tmp" && mv "$manifest.tmp" "$manifest"
jq -r --arg step "$step" '.stages[$step][] | "\(.hash) \(.what) gas \(.gasUsed) status \(.status)"' "$manifest"

# After the candidate: when its wait is over, from the block that mined it.
if [ "$step" = submit ]; then "$0" status; fi

if [ "$step" = beacon ]; then
  # The block that recorded the beacon: its number and prevrandao give the nonce the finalisation
  # proof must echo. Written beside the fixture for `e2e/run.sh inputs`.
  index=$(($(call 'beaconCount()(uint256)' | awk '{print $1}') - 1))
  nonce="$(call 'beacons(uint256)(uint256,bytes32)' "$index" | sed -n 2p)"
  block="$(jq -r '.stages.beacon[-1].blockNumber' "$manifest")"
  jq -n --argjson number "$(cast to-dec "$block")" --argjson index "$index" --arg nonce "$nonce" --arg vault "$vault" \
    --arg randao "$(cast block "$block" --field mixHash "${rpc[@]}")" \
    --argjson time "$(call 'beacons(uint256)(uint256,bytes32)' "$index" | sed -n 1p | awk '{print $1}')" \
    '{blockNumber: $number, prevrandao: $randao, nonce: $nonce, vault: $vault, index: $index, timestamp: $time}' \
    > "$(dirname "$fixture")/beacon.json"
  cat "$(dirname "$fixture")/beacon.json"
  echo "next: prove the finalisation for this nonce (e2e/README.md), then: script/roundtrip.sh finalize"
fi
