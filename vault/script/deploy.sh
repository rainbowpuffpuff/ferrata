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
#   FERRATA_LOG_DIR     one log per run, and the network journal in net/ (default broadcast/deploy-logs/<record
#                       name>: one journal per deployment record, so per chain and mode by default)
#
# The network (e2e/NETWORK.md): forge and cast talk to a local proxy (scripts/pubnet.sh) in front of FERRATA_RPC_URL
# (and, for a public one, the free public endpoints of scripts/pubnet-endpoints.json); a caller's proxy (PUBNET_URL,
# e2e/crown/sepolia.sh) is used as it is. Every signed transaction is journaled before it leaves (<log dir>/net/); a
# failed run is run again only once the fate of what it may have sent is known, and only for a transient reason.
# Exit status: 0 complete, 1 a permanent stop, 75 a transient stop with nothing in flight.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
cd "$here"
export FOUNDRY_DISABLE_NIGHTLY_WARNING=1
: "${FERRATA_MODE:?FERRATA_MODE: A or B}"
: "${ETH_FROM:?ETH_FROM: the deployer address}"
rpc="${FERRATA_RPC_URL:-http://127.0.0.1:8545}"
export FERRATA_GAS_MARGIN_PCT="${FERRATA_GAS_MARGIN_PCT:-125}"
# shellcheck source=../../scripts/pubnet.sh
. "$here/../scripts/pubnet.sh"
# The chain id first (a static answer, read before the proxy): it names the deployment record and with it the
# journal. One journal per deployment record (default deployments/<chain>-<mode>.json, so per chain and mode): a
# journal never holds two identical calls, and a second deployment's first step is the same call as the first's.
chain="$(pubnet_read cast chain-id --rpc-url "$rpc")"
[ "$chain" != 1 ] || { echo "deploy.sh: a test deployment, not on chain 1" >&2; exit 1; }
export FERRATA_OUT="${FERRATA_OUT:-deployments/$chain-$FERRATA_MODE.json}"
mkdir -p "$(dirname "$FERRATA_OUT")"
logs="${FERRATA_LOG_DIR:-broadcast/deploy-logs/$(basename "$FERRATA_OUT" .json)}"
mkdir -p "$logs"
pubnet_up sepolia "$logs/net" "$ETH_FROM" "$rpc" || { echo "deploy.sh: the network proxy did not start" >&2; exit 75; }
case "$rpc" in "$PUBNET_URL"*) ;; *) rpc="$PUBNET_URL/deploy" ;; esac
sign=()
if [ "${FERRATA_UNLOCKED:-0}" = 1 ]; then
  sign=(--unlocked)
else
  : "${ETH_KEYSTORE:?ETH_KEYSTORE: a keystore file (or FERRATA_UNLOCKED=1 against a local fork)}"
  : "${ETH_PASSWORD:?ETH_PASSWORD: a file holding the keystore password}"
  [ -f "$ETH_KEYSTORE" ] && [ -f "$ETH_PASSWORD" ] || { echo "deploy.sh: ETH_KEYSTORE, ETH_PASSWORD must be files" >&2; exit 1; }
fi
last=""
for run in $(seq 1 12); do
  log="$logs/deploy-$chain-$FERRATA_MODE-$run.log"
  rc=0
  # forge would also read ETH_FROM as --froms, which --unlocked refuses: the sender is given as --sender.
  pubnet_stage "$log" env -u ETH_FROM forge script script/DeployVault.s.sol:DeployVault --rpc-url "$rpc" --sender "$ETH_FROM" \
      ${sign[@]+"${sign[@]}"} --broadcast --skip-simulation --slow --gas-estimate-multiplier "$FERRATA_GAS_MARGIN_PCT" -vv || rc=$?
  if [ "$rc" != 0 ]; then
    echo "deploy.sh: run $run failed ($([ "$rc" = 75 ] && echo transient || echo permanent)); nothing more is sent (log $log)" >&2
    exit "$rc"
  fi
  step="$({ grep -o 'FERRATA_STEP [A-Za-z0-9]*' "$log" || true; } | tail -n 1 | cut -d' ' -f2)"
  grep -h -o -E 'FERRATA_(GAS|CREATED) .*' "$log" || true
  # The transaction as the node has it: forge must have sent it with the limit the script checked.
  limit="$({ grep -o 'FERRATA_GAS [A-Za-z0-9]* estimate [0-9]* limit [0-9]*' "$log" || true; } | tail -n 1 | awk '{print $6}')"
  if [ -n "$limit" ]; then
    hash="$(grep -o '"hash": *"0x[0-9a-fA-F]\{64\}"' "broadcast/DeployVault.s.sol/$chain/run-latest.json" | head -n 1 \
      | grep -o '0x[0-9a-fA-F]*')"
    sent="$(pubnet_read cast tx "$hash" gas --rpc-url "$rpc")"
    echo "FERRATA_SENT $step $hash gas limit $sent, used $(pubnet_read cast receipt "$hash" gasUsed --rpc-url "$rpc")"
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
