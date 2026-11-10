#!/usr/bin/env bash
# The return leg on Sepolia (or on a local fork of it), one stage per call, with a manifest of every
# transaction. The stages and the waits between them are in vault/README.md ("Sepolia round trip").
#
#   script/roundtrip.sh deploy | deposit0 | deposit1 | deposit2 | submit | beacon | finalize | release | status
#
# Gas: every transaction's gas limit is the node's own estimate (eth_estimateGas, taken by forge right before the
# transaction is signed, after the previous one's receipt) times FERRATA_GAS_MARGIN_PCT / 100. Foundry's EVM does not
# price Sepolia's state gas (EIP-8037, Glamsterdam), so forge's own simulation is skipped. Before a stage sends
# anything, e2e/gasplan.py plays the stage on a fork and has the node price each of its transactions on that state;
# if one has no estimate (it fails, or its execution gas is over the 2^24 cap) or its limit is over the cap, the
# stage stops with nothing sent. The record: gas/<stage>.json beside the fixture.
#
# Environment:
#   SEPOLIA_RPC_URL   the RPC endpoint
#   ETH_FROM          the sender's address: the fixture's deployer
#   the key, through Foundry's own variables, never on a command line and never in this repository:
#     ETH_KEYSTORE + ETH_PASSWORD   a keystore file and a file holding its password, or
#     ETH_KEYSTORE_ACCOUNT          a name under ~/.foundry/keystores (forge asks for the password)
#   FERRATA_UNLOCKED=1  no key: the endpoint is a local fork that impersonates ETH_FROM
#   FERRATA_RESUME=1    finish a stage that was interrupted while it was sending (forge --resume)
#   FERRATA_SETTLE      seconds between a stage's last receipt and the next read of the chain (default 15;
#                       not on a fork)
#   FERRATA_GAS_RPC   the node whose gas counts (default SEPOLIA_RPC_URL; on a fork, the real Sepolia endpoint, or
#                     the fork itself prices the stages as Foundry does)
#   FERRATA_GAS_MARGIN_PCT  the gas limit over the node's estimate, in percent (default 125)
#   FERRATA_GAS_FORK_PORT   the port of the throwaway fork the gas check plays a stage on (default 8547)
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
  read -r t submitted delta <<<"$(call 'candidates(bytes32)(bytes32,bytes32,uint64,uint64,bool,uint32,uint64,bytes32,uint256,uint256,uint256)' "$state" | sed -n '6p;9p;10p' | awk '{print $1}' | tr '\n' ' ')"
  if [ "${submitted:-0}" != 0 ]; then
    over=$((submitted + delta + (t - 1) * $(call 'turnSeconds()(uint256)' | awk '{print $1}')))
    echo "candidate $state: submitted at $submitted, wait over at $over ($((over - now)) s from now)"
  fi
  echo "burn root $(jq -r .burnRoot "$fixture"): $(call 'burnRootStatus(bytes32)(bool,uint256)' "$(jq -r .burnRoot "$fixture")" | tr '\n' ' ')"
  exit 0
fi

: "${ETH_FROM:?set ETH_FROM to the deployer address}"
signer=()
[ "${FERRATA_UNLOCKED:-}" = 1 ] && signer=(--unlocked)
# FERRATA_RESUME=1: a stage whose transactions were only partly sent; forge sends the rest of what it
# recorded, in the recorded order, without running the script again.
[ "${FERRATA_RESUME:-}" = 1 ] && signer+=(--resume)
# The stage's transactions as forge would send them (a dry run, nothing signed), priced by the node on a fork that
# holds the chain's state: the fork the run uses for a rehearsal (put back afterwards), otherwise a throwaway fork of
# the endpoint, started here.
gas_check() {
  local dry="broadcast/RoundTrip.s.sol/$chain/dry-run/$step-latest.json" out pid="" state="$SEPOLIA_RPC_URL" rc=0
  out="$(dirname "$fixture")/gas"
  mkdir -p "$out"
  rm -f "$dry"
  forge script script/RoundTrip.s.sol:RoundTrip --sig "$step()" "${rpc[@]}" --sender "$ETH_FROM" > "$out/$step.forge.log" 2>&1 ||
    { tail -n 20 "$out/$step.forge.log" >&2; return 1; }
  [ -f "$dry" ] || { echo "forge wrote no dry run of $step" >&2; return 1; }
  python3 "$here/../e2e/gasplan.py" plan "$dry" --out "$out/$step.plan.json" > /dev/null || return 1
  if [ "${FERRATA_UNLOCKED:-}" != 1 ]; then
    # Its log stays out of the run directory: anvil's banner names the endpoint, which may hold a key.
    local port="${FERRATA_GAS_FORK_PORT:-8547}" alog="$TMPDIR/gas-fork-$step.log" attempt line
    state="http://127.0.0.1:$port"
    for attempt in 1 2 3; do
      anvil --fork-url "$SEPOLIA_RPC_URL" --host 127.0.0.1 --port "$port" --auto-impersonate > "$alog" 2>&1 &
      pid=$!
      for _ in $(seq 60); do
        cast chain-id --rpc-url "$state" > /dev/null 2>&1 && break 2
        kill -0 "$pid" 2> /dev/null || break
        sleep 1
      done
      kill "$pid" 2> /dev/null; wait "$pid" 2> /dev/null || true; pid=""
      tail -n 3 "$alog" | while IFS= read -r line; do echo "anvil, attempt $attempt: ${line//"$SEPOLIA_RPC_URL"/<endpoint>}" >&2; done
      sleep 10
    done
    [ -n "$pid" ] || { echo "no fork of the endpoint in three attempts: the gas check cannot run" >&2; return 1; }
  fi
  echo "gas check of $step: the node $(echo "${FERRATA_GAS_RPC:-$SEPOLIA_RPC_URL}" | sed 's,^\([a-z]*://[^/?]*\).*,\1,') prices each transaction on the state of $state"
  python3 "$here/../e2e/gasplan.py" run --node "${FERRATA_GAS_RPC:-$SEPOLIA_RPC_URL}" --state "$state" \
    --plan "$out/$step.plan.json" --margin "$(awk -v m="$margin" 'BEGIN {print m / 100}')" --out "$out/$step.json" \
    --accounts "$(jq -r '[.groth16Verifier, .emergencyStop, .token, .vault] | join(",")' "$fixture")" || rc=$?
  if [ -n "$pid" ]; then kill "$pid" 2> /dev/null; wait "$pid" 2> /dev/null || true; fi
  return "$rc"
}

# `record <stage>` only rewrites the manifest entry of a stage from forge's broadcast log.
if [ "$step" = record ]; then
  step="${2:?stage}"
else
  case "$step" in deploy | deposit[0-9] | submit | beacon | finalize | release) ;; *) echo "unknown stage $step" >&2; exit 2 ;; esac
  margin="${FERRATA_GAS_MARGIN_PCT:-125}"
  if [ "${FERRATA_RESUME:-}" != 1 ]; then gas_check || { echo "STOP before $step: the node's gas check failed; nothing of $step was sent" >&2; exit 1; }; fi
  forge script script/RoundTrip.s.sol:RoundTrip --sig "$step()" "${rpc[@]}" --sender "$ETH_FROM" "${signer[@]}" \
    --broadcast --slow --skip-simulation --gas-estimate-multiplier "$margin"
  # A public endpoint is several nodes: give the others a block to catch up before the chain is read again.
  [ "${FERRATA_UNLOCKED:-}" = 1 ] || sleep "${FERRATA_SETTLE:-15}"
fi

# The manifest: the transactions forge sent (its broadcast log names them), each with the receipt the
# chain gives now. So a stage whose forge was stopped between sending and saving a receipt is still
# recorded whole.
log="broadcast/RoundTrip.s.sol/$chain/$step-latest.json"
[ -f "$manifest" ] || jq -n --argjson chain "$chain" --arg vault "$vault" --arg from "$ETH_FROM" \
  '{chainId: $chain, vault: $vault, sender: $from, stages: {}}' > "$manifest"
jq -c '.transactions[] | {hash, what: ([.contractName, .function, .transactionType] | map(select(. != null)) | join(" "))}' "$log" |
  while read -r sent; do
    hash="$(jq -r '.hash // empty' <<< "$sent")"
    [ -n "$hash" ] || { echo "forge's log of the $step stage holds a transaction that was not sent" >&2; exit 1; }
    receipt=""
    for _ in $(seq 12); do # tried for a minute: a load-balanced endpoint can answer from a node one block behind
      receipt="$(cast receipt "$hash" --async --json "${rpc[@]}" 2> /dev/null < /dev/null)" && break
      sleep 5
    done
    [ -n "$receipt" ] ||
      { echo "the chain has no receipt for $hash, which forge's log of the $step stage names" >&2; exit 1; }
    limit="$(cast tx "$hash" gas "${rpc[@]}" 2> /dev/null || echo 0)" # the gas limit it was sent with
    jq -c --argjson sent "$sent" --argjson limit "${limit:-0}" '$sent + {contractAddress: (.contractAddress // null),
      to: (.to // null), blockNumber, gasLimit: $limit, gasUsed, effectiveGasPrice, status}' <<< "$receipt"
  done | jq -s . > "$manifest.stage"
jq --arg step "$step" --slurpfile sent "$manifest.stage" '.stages[$step] = $sent[0]' "$manifest" > "$manifest.tmp" &&
  mv "$manifest.tmp" "$manifest"
rm -f "$manifest.stage"
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
