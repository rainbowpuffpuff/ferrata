#!/usr/bin/env bash
# Deploys a test FerrataVault (rule A or B) with its linked library FerrataRegistry, ONE TRANSACTION PER FORGE RUN:
# script/DeployVault.s.sol does the next step that is not yet on chain; this loop runs it until its record says
# complete, and stops at the first run that fails.
#
#   FERRATA_MODE=A script/deploy.sh          (or FERRATA_MODE=B)
#
# Gas: every limit is the node's eth_estimateGas for that transaction, on the chain as the earlier ones left it, times
# FERRATA_GAS_MARGIN_PCT / 100; the script refuses (nothing is sent) when that is above the cap of FERRATA_GAS_RULE.
# Sepolia since 6 Oct 2026 (Glamsterdam, EIP-8037): FERRATA_GAS_RULE=eip8037 (the vault's creation needs ~39.5 M gas).
#
# Environment:
#   FERRATA_MODE        A | B
#   ETH_FROM            the deployer's address (the admin of the vault, the stop's owner until the last step)
#   FERRATA_RPC_URL     the endpoint (default http://127.0.0.1:8545, a local fork)
#   the key, through Foundry's own variables, never on a command line and never in this repository:
#     ETH_KEYSTORE + ETH_PASSWORD   a keystore file and a file holding its password (forge reads both itself)
#   FERRATA_UNLOCKED=1  no key: the endpoint is a local fork that impersonates ETH_FROM
#   FERRATA_GAS_RULE    eip7825 (default: every limit <= 2^24) | eip8037 (limit <= min(2^32 - 1, block gas limit))
#   FERRATA_GAS_MARGIN_PCT  the margin, 100..300 (default 125); FERRATA_TX_GAS_CAP  an own lower cap
#   FERRATA_OUT         the record (default deployments/<chain id>-<mode>.json)
#   FERRATA_PARAMS      programs, LEZ configuration and LEZ definitions (default script/params/sepolia.json)
#   FERRATA_GROTH16_VERIFIER  an existing Groth16 verifier to put behind the stop instead of deploying one
#   FERRATA_FREEZE_AUTHORITY, FERRATA_FEE_RECIPIENT, FERRATA_WATCHERS (comma-separated, at most 3), FERRATA_DELTA,
#   FERRATA_REVOKE_WINDOW, FERRATA_MAX_FREEZE, FERRATA_MIN_RELEASE_DELAY, FERRATA_RELEASE_DELAY: the testnet
#     configuration (defaults: an address nobody holds a key for, the deployer, none, 600 s, 1 day, 30 days, 0, 120 s)
#   FERRATA_LOG_DIR     one log per run (default broadcast/deploy-logs)
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
cd "$here"
export FOUNDRY_DISABLE_NIGHTLY_WARNING=1
: "${FERRATA_MODE:?FERRATA_MODE: A or B}"
: "${ETH_FROM:?ETH_FROM: the deployer address}"
rpc="${FERRATA_RPC_URL:-http://127.0.0.1:8545}"
export FERRATA_GAS_MARGIN_PCT="${FERRATA_GAS_MARGIN_PCT:-125}"
chain="$(cast chain-id --rpc-url "$rpc")"
[ "$chain" != 1 ] || { echo "deploy.sh: a test deployment, not on chain 1" >&2; exit 1; }
export FERRATA_OUT="${FERRATA_OUT:-deployments/$chain-$FERRATA_MODE.json}"
mkdir -p "$(dirname "$FERRATA_OUT")"
sign=()
if [ "${FERRATA_UNLOCKED:-0}" = 1 ]; then
  sign=(--unlocked)
else
  : "${ETH_KEYSTORE:?ETH_KEYSTORE: a keystore file (or FERRATA_UNLOCKED=1 against a local fork)}"
  : "${ETH_PASSWORD:?ETH_PASSWORD: a file holding the keystore password}"
  [ -f "$ETH_KEYSTORE" ] && [ -f "$ETH_PASSWORD" ] || { echo "deploy.sh: ETH_KEYSTORE, ETH_PASSWORD must be files" >&2; exit 1; }
fi
logs="${FERRATA_LOG_DIR:-broadcast/deploy-logs}"
mkdir -p "$logs"
# A load-balanced endpoint can announce a block that the node answering forge's next read does not have yet ("block
# not found", crown-live-1 on 11 Nov 2026, twice right after a transaction). Such a run is run again, at most 5 times,
# 24 s apart, only when the deployer's nonce (latest and pending) shows that nothing was sent; any later send uses that
# same nonce, so at most one transaction per step can land.
transient='-32001|block not found|header not found|failed to get account|failed to get storage|missing trie node'
last=""
for run in $(seq 1 12); do
  log="$logs/deploy-$chain-$FERRATA_MODE-$run.log"
  tries=0
  while :; do
    n0="$(cast nonce "$ETH_FROM" --rpc-url "$rpc")"
    # forge would also read ETH_FROM as --froms, which --unlocked refuses: the sender is given as --sender.
    if env -u ETH_FROM forge script script/DeployVault.s.sol:DeployVault --rpc-url "$rpc" --sender "$ETH_FROM" ${sign[@]+"${sign[@]}"} \
        --broadcast --skip-simulation --slow --gas-estimate-multiplier "$FERRATA_GAS_MARGIN_PCT" -vv > "$log" 2>&1; then
      break
    fi
    tail -n 25 "$log" >&2
    if [ "$tries" -lt 5 ] && grep -q -E -e "$transient" "$log" \
        && [ "$(cast nonce "$ETH_FROM" --rpc-url "$rpc")" = "$n0" ] \
        && [ "$(cast nonce "$ETH_FROM" --block pending --rpc-url "$rpc")" = "$n0" ]; then
      tries=$((tries + 1))
      mv "$log" "$log.transient-$tries"
      echo "deploy.sh: run $run: the endpoint failed a read and nothing was sent (nonce still $n0); again in 24 s ($tries of 5)" >&2
      sleep 24
      continue
    fi
    echo "deploy.sh: run $run failed; nothing more is sent (log $log)" >&2
    exit 1
  done
  step="$({ grep -o 'FERRATA_STEP [A-Za-z0-9]*' "$log" || true; } | tail -n 1 | cut -d' ' -f2)"
  grep -h -o -E 'FERRATA_(GAS|CREATED) .*' "$log" || true
  # The transaction as the node has it: forge must have sent it with the limit the script checked.
  limit="$({ grep -o 'FERRATA_GAS [A-Za-z0-9]* estimate [0-9]* limit [0-9]*' "$log" || true; } | tail -n 1 | awk '{print $6}')"
  if [ -n "$limit" ]; then
    hash="$(grep -o '"hash": *"0x[0-9a-fA-F]\{64\}"' "broadcast/DeployVault.s.sol/$chain/run-latest.json" | head -n 1 \
      | grep -o '0x[0-9a-fA-F]*')"
    sent="$(cast tx "$hash" gas --rpc-url "$rpc")"
    echo "FERRATA_SENT $step $hash gas limit $sent, used $(cast receipt "$hash" gasUsed --rpc-url "$rpc")"
    if [ "$sent" != "$limit" ]; then
      echo "deploy.sh: forge sent a gas limit of $sent, not the checked $limit; stopping (log $log)" >&2
      exit 1
    fi
  fi
  if grep -q FERRATA_COMPLETE "$log"; then
    echo "deploy.sh: complete; record $FERRATA_OUT"
    exit 0
  fi
  if [ -z "$step" ] || [ "$step" = "$last" ]; then
    echo "deploy.sh: no progress at step '$step' (log $log)" >&2
    exit 1
  fi
  last="$step"
done
echo "deploy.sh: not complete after 12 runs" >&2
exit 1
