#!/usr/bin/env bash
# The Sepolia side of the crown run (e2e/crown/README.md), one stage per call, every transaction recorded.
#
#   e2e/crown/sepolia.sh <run dir> deploy | deposit <j> | submit | finalize | release <i> | status
#
# deploy   vault/script/deploy.sh with FERRATA_MODE=B and FERRATA_GAS_RULE=eip8037: the stop, the library, the vault,
#          the test token, ETH and the token registered with fees, the stop's owner renounced (one transaction per
#          forge run, each limit the node's estimate x margin), the Groth16 verifier reused (<run>/identity.json
#          .groth16Verifier; without one, DeployVault deploys it first); then the deployed vault is checked against
#          <run>/identity.json.
# deposit, submit, finalize, release: vault/script/Crown.s.sol, one transaction each; forge asks the node for
#          eth_estimateGas right before it signs (--skip-simulation: Foundry's EVM does not price Sepolia's state gas)
#          and the limit is that estimate x FERRATA_GAS_MARGIN_PCT / 100; a stage the node cannot price sends nothing.
#
# Environment: SEPOLIA_RPC_URL, ETH_FROM; the key through Foundry's own ETH_KEYSTORE + ETH_PASSWORD (files), or
# FERRATA_UNLOCKED=1 on a local fork that impersonates ETH_FROM. FERRATA_GAS_MARGIN_PCT (default 125).
# <run> must be under e2e/runs/ (forge reads params.json and crown.json there: vault/foundry.toml fs_permissions);
# DeployVault's record is written to vault/deployments/crown-<run name>.json and copied to <run>/deploy.json.
# Inputs in <run>: identity.json (vault, configId, genesisState, ...), params.json (DeployVault's), crown.json
# (deposits, candidate, releases). Records: deploy.json, manifest.json (every transaction: stage, hash, nonce,
# block, gasLimit, gasUsed, status), logs/, net/ (the network journal: every signed transaction before it left).
#
# The network (e2e/NETWORK.md): forge and cast talk only to a local proxy (scripts/pubnet.sh, pubnet.py) in front of
# SEPOLIA_RPC_URL and the free public Sepolia endpoints (scripts/pubnet-endpoints.json): reads at "latest" pinned a
# few blocks behind the head (never before the deployer's last transaction), failover and backoff on transient errors,
# the nonce checked on two endpoints and the signed bytes journaled before a broadcast to every endpoint. A failed run
# is run again only after the fate of everything it may have sent is known, and only if it failed for a transient
# reason, within PUBNET_STAGE_BUDGET_S (1200 s). Exit status: 0 done, 1 a permanent stop (a human looks), 75 a
# transient stop with nothing in flight (e2e/crown/cloud/drive.py resumes those by itself).
set -euo pipefail
here="$(cd "$(dirname "$0")/../.." && pwd)"
run="$(cd "${1:?run directory}" && pwd)"; step="${2:?stage}"; arg="${3:-}"
: "${SEPOLIA_RPC_URL:?set SEPOLIA_RPC_URL}" "${ETH_FROM:?set ETH_FROM}"
export FOUNDRY_DISABLE_NIGHTLY_WARNING=1 TMPDIR="${TMPDIR:-$here/../tmp}"
mkdir -p "$TMPDIR"
margin="${FERRATA_GAS_MARGIN_PCT:-125}"
id="$run/identity.json"; manifest="$run/manifest.json"; mkdir -p "$run/logs"
export PUBNET_MANIFEST="$manifest" # where resolve records a transaction marked gone that shows up mined after all
say() { echo "[$(date -u +%FT%TZ)] $*"; }
die() { say "STOP: $*"; exit 1; }
die_transient() { say "STOP (transient, nothing in flight): $*"; exit 75; }
# shellcheck source=../../scripts/pubnet.sh
. "$here/scripts/pubnet.sh"
pubnet_up sepolia "$run/net" "$ETH_FROM" "$SEPOLIA_RPC_URL" || die_transient "the network proxy did not start"
rpc=(--rpc-url "$PUBNET_URL/$step${arg:+-$arg}")
rd() { local rc=0; pubnet_read "$@" || rc=$?; [ "$rc" = 0 ] || { [ "$rc" = 75 ] && die_transient "a read failed: $*"; die "a read failed: $*"; }; }
chain="$(rd cast chain-id "${rpc[@]}")"
[ "$chain" = "$(jq -r .chainId "$id")" ] || die "chain id $chain is not the run's"
vault="$(jq -r .vault "$id")"
call() { rd cast call "$vault" "$@" "${rpc[@]}"; }
signer=(); [ "${FERRATA_UNLOCKED:-}" = 1 ] && signer=(--unlocked)

# Appends the stage's mined transactions, from the network journal, to the manifest under the stage's name (each hash
# once); a transaction that did not succeed stops the run.
record() {
  local name="$1" rows
  [ -f "$manifest" ] || echo '{"chainId": '"$chain"', "stages": {}}' > "$manifest"
  local acked
  rows="$(python3 -I "$PUBNET_PY" rows --journal "$PUBNET_DIR/journal.jsonl" --stage "$name")"
  acked="$(python3 -I "$PUBNET_PY" acked --journal "$PUBNET_DIR/journal.jsonl")"
  # A transaction the operator acknowledged (pubnet.py ack: a reverted one, say) leaves the stage's rows.
  jq --arg s "$name" --argjson rows "$rows" --argjson acked "$acked" '.stages[$s] = ([(.stages[$s] // [])[]
      | select(.hash as $h | ($acked | index($h)) == null)] as $old | $old
      + [$rows[] | select(.hash as $h | ($old | map(.hash) | index($h)) == null)])
      | if .stages[$s] == [] then del(.stages[$s]) else . end' "$manifest" > "$manifest.tmp"
  mv "$manifest.tmp" "$manifest"
  [ "$rows" != '[]' ] || { say "$name: nothing sent (already done)"; return 0; }
  jq -e 'all(.status == "0x1")' <<<"$rows" > /dev/null || die "$name: a transaction failed: $rows"
  say "$name: $(jq length <<<"$rows") transaction(s) mined, in the manifest: $(jq -r 'map(.hash) | join(" ")' <<<"$rows")"
}

crown() { # crown <signature> [args]: one transaction of Crown.s.sol (state-checked: a step already on chain is skipped)
  local sig="$1"; shift
  local name="${sig%%(*}${1:+-$1}" rc=0
  # The fate of everything sent so far first; then, one transaction per stage: if the journal holds this stage's
  # mined transaction, the stage is done whatever an endpoint answers now (PUBNET_NO_JOURNAL_SKIP=1, for tests only:
  # plan again, on the reads alone).
  pubnet_resolve || { rc=$?; [ "$rc" = 3 ] && die "$name: an earlier transaction did not succeed (above): a human looks, then pubnet.py ack"
    die_transient "$name: an earlier transaction's fate is still unknown"; }
  # Only a transaction that succeeded (status 0x1) makes the stage done: after a reverted one and the operator's
  # `pubnet.py ack`, the stage runs again.
  if [ "${PUBNET_NO_JOURNAL_SKIP:-}" != 1 ] && python3 -I "$PUBNET_PY" rows --journal "$PUBNET_DIR/journal.jsonl" \
      --stage "$name" | jq -e 'any(.status == "0x1")' > /dev/null; then
    say "$name: the journal holds this stage's successful transaction; not run again"
    record "$name"
    return 0
  fi
  pubnet_stage "$run/logs/$name.log" env -u ETH_FROM FERRATA_CROWN="$run/crown.json" bash -c 'cd "$0/vault" && exec "$@"' \
      "$here" forge script script/Crown.s.sol:Crown --sig "$sig" "$@" --rpc-url "$PUBNET_URL/$name" --sender "$ETH_FROM" \
      "${signer[@]}" --broadcast --skip-simulation --slow --gas-estimate-multiplier "$margin" || rc=$?
  record "$name"
  [ "$rc" = 0 ] || { [ "$rc" = 75 ] && die_transient "$name"; die "$name"; }
}

case "$step" in
  status)
    say "chain $chain block $(rd cast block-number "${rpc[@]}") (pinned) deployer nonce $(rd cast nonce "$ETH_FROM" "${rpc[@]}")"
    if [ "$(rd cast code "$vault" "${rpc[@]}")" = 0x ]; then say "vault $vault: not deployed"; exit 0; fi
    say "vault $vault: finalState $(call 'finalState()(bytes32)') leaves $(call 'nextLeafIndex()(uint64)')" \
      "checkpoints $(call 'checkpointCount()(uint256)')"
    ;;
  deploy)
    # identity.json without .groth16Verifier (a fresh local chain: checkpoint/attestor/zone-bridge.sh): DeployVault deploys one.
    # deploy.sh uses this proxy (PUBNET_URL): its runs are journaled with the other stages', one transaction per run.
    gv="$(jq -r '.groth16Verifier // empty' "$id")"
    rc=0
    ( cd "$here/vault" && if [ -n "$gv" ]; then export FERRATA_GROTH16_VERIFIER="$gv"; fi && FERRATA_MODE=B FERRATA_GAS_RULE=eip8037 FERRATA_GAS_MARGIN_PCT="$margin" \
        FERRATA_RPC_URL="$PUBNET_URL/deploy" FERRATA_PARAMS="$run/params.json" FERRATA_OUT="deployments/crown-$(basename "$run").json" \
        FERRATA_DELTA="$(jq -r .delta "$id")" \
        FERRATA_LOG_DIR="$run/logs/deploy" script/deploy.sh ) > "$run/logs/deploy.log" 2>&1 || rc=$?
    if [ "$rc" != 0 ]; then
      tail -n 30 "$run/logs/deploy.log" | sed "s,$SEPOLIA_RPC_URL,<endpoint>,g"
      [ "$rc" = 75 ] && die_transient deploy
      die deploy
    fi
    cp "$here/vault/deployments/crown-$(basename "$run").json" "$run/deploy.json"
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
    crown "$step(uint256)" "$arg"
    ;;
  submit|finalize)
    sig=submit; [ "$step" = finalize ] && sig=finalizeAttested
    crown "$sig()"
    ;;
  *) sed -n 2,29p "$0"; exit 2 ;;
esac
