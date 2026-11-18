#!/usr/bin/env bash
# EXPERIMENTAL, UNAUDITED (README.md). The Ferrata bridge for LEZ v0.3.1: a vault on an EVM chain, the bridge
# program on a LEZ v0.3.1 sequencer, the inbound leg and the burns.
#
#   lez-v03-bridge/testnet.sh plan     the steps, their transactions and costs; read-only RPC calls only
#   lez-v03-bridge/testnet.sh prepare  (testnet) the wallet, accounts, configuration id and deposits' commitments that
#                                      sepolia.sh needs; nothing sent
#   lez-v03-bridge/testnet.sh run      sends; refused unless FERRATA_TESTNET_SEND=yes; resumable (each step is
#                                      recorded in the run directory and skipped when done)
#   lez-v03-bridge/testnet.sh verify   re-reads every verdict from both chains' RPCs against the run's record
#
# FERRATA_NET=local (default): a local standalone LEZ v0.3.1 sequencer (LEZ_RPC, default http://127.0.0.1:3040),
#   a plain local anvil (ETH_RPC, default http://127.0.0.1:8545) where the script deploys the vault itself
#   (vault/script/deploy.sh, mode B, anvil's account 0), and the STAND-IN light client (guests/records: it writes
#   whatever record it is given, at the light client's address layout), whose record of the anvil block the
#   captures read is written by the script. Payer: LEZ's published test account A (the debug genesis funds it).
# FERRATA_NET=testnet: LEZ_RPC=https://testnet.lez.logos.co and ETH_RPC=<a Sepolia endpoint>. Nothing is deployed on
#   Sepolia here: the vault and its two deposits are sepolia.sh's (the owner's command, REPORT.md), whose record
#   gives FERRATA_VAULT, the deposits' leaf indices and FERRATA_CAPTURE (a capture of the finalized block that
#   holds them, e2e/crown/capture.py). The light client is lez-v03's (branch testnet-v03), deployed and advanced on
#   the testnet by its own testnet.sh: FERRATA_LC_PROGRAM, FERRATA_LC_INSTANCE and FERRATA_LC_SLOT (the slot of its
#   record of the captured block). The public steps go as one batch for one block through testnet-ops's queued
#   sender (FERRATA_OPS), signed by the wallet home FERRATA_WALLET_HOME whose funded public account FERRATA_PAYER_ID
#   pays (prepare makes this run's accounts there); the private claim and burn follow, proven by the wallet.
#
# The run directory (FERRATA_RUN, default spike/tmp/testnet-v03-bridge/<net>-run beside the repository) holds the
# keys, the wallet's storage and run.json: never inside the repository. results/<net>/ gets the record without keys.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/.." && pwd)"
spike="$(cd "$repo/.." && pwd)"
cmd="${1:-plan}"
net="${FERRATA_NET:-local}"
run="${FERRATA_RUN:-$spike/tmp/testnet-v03-bridge/$net-run}"
case "$run" in "$repo"/*) echo "testnet.sh: FERRATA_RUN must be outside the repository" >&2; exit 1 ;; esac
# The deposits' and burns' wallet seed. A local run uses a public test value; on any other network it must be a secret
# the caller supplies (review finding 2: from a public seed anyone computes every deposit's `s`, can spend its nullifier
# first under a configuration of their own, and links the private transactions to their deposits).
if [ "$net" = local ]; then
  seed="${FERRATA_DEPOSIT_SEED:-ferrata/lez-v03-bridge: local user bridge seed (test value)}"
  instance="${FERRATA_LC_INSTANCE:-$(printf 'ferrata/lez-v03-bridge/local-instance' | sha256sum | cut -c1-64)}"
else
  seed="${FERRATA_DEPOSIT_SEED:-}"; instance="${FERRATA_LC_INSTANCE:-}"
fi
need_secrets() { # a non-local network: the caller's secret seed and the light client's real instance, no defaults
  [ "$net" = local ] && return 0
  [ -n "$seed" ] || { echo "testnet.sh: FERRATA_DEPOSIT_SEED must be a secret you supply on $net (no default)" >&2; exit 2; }
  [ -n "$instance" ] || { echo "testnet.sh: FERRATA_LC_INSTANCE (the light client's instance id) is required on $net" >&2; exit 2; }
}
eth_denomination=1000000000000000      # 0.001 ETH, the vault's ETH denomination 0 (deploy.sh)
eth_total=1002000000000000             # with the 0.2% relay fee of schedule 0
recipient="${FERRATA_RECIPIENT:-0x00000000000000000000000000000000000Fe77a}"
results="$here/results/$net"
case "$net" in
  local)
    lez="${LEZ_RPC:-http://127.0.0.1:3040}"; eth="${ETH_RPC:-http://127.0.0.1:8545}"
    case "$eth$lez" in *@*) echo "testnet.sh: a userinfo in ETH_RPC or LEZ_RPC is refused (the client would connect to the host after the @)" >&2; exit 1 ;; esac
    case "$eth" in http://127.0.0.1:*|http://localhost:*) ;; *) echo "testnet.sh: local ETH_RPC must be a loopback anvil" >&2; exit 1 ;; esac
    case "$lez" in http://127.0.0.1:*|http://localhost:*) ;; *) echo "testnet.sh: local LEZ_RPC must be a loopback sequencer" >&2; exit 1 ;; esac
    ;;
  testnet)
    lez="${LEZ_RPC:-https://testnet.lez.logos.co}"; eth="${ETH_RPC:?ETH_RPC: a Sepolia endpoint}"
    ;;
  *) echo "testnet.sh: FERRATA_NET=local or testnet" >&2; exit 1 ;;
esac
drv="${BRIDGE03:-$here/target/release/bridge03}"
export SUPPRESS_VERBOSE_PRINTS=1   # LEZ's wallet would print every transaction in full
b() { "$drv" "$run" "$@"; }
log() { printf '== %s %s\n' "$(date -u +%T)" "$*"; }

rpc() { # rpc URL METHOD PARAMS-JSON  (read-only calls)
  curl -s --max-time 30 -H 'Content-Type: application/json' \
    --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$2\",\"params\":$3}" "$1"
}

plan() {
  log "net $net: LEZ $lez, EVM $eth, run $run"
  log "LEZ fee state (getFeeState): $(rpc "$lez" getFeeState '[]' | head -c 400)"
  log "LEZ last block: $(rpc "$lez" getLastBlockId '[]' | head -c 200)"
  log "EVM chain id: $(rpc "$eth" eth_chainId '[]' | head -c 200)"
  cat <<EOF
Steps of 'run' (each one transaction unless said; gas from results/synthetic.md and results/local/, the testnet's
base fees from getFeeState above; a privacy-preserving transaction is fee-exempt on v0.3.1):
  LEZ  setup          the wallet, the payer, the accounts (no transaction)
  LEZ  deploy bridge  program_loader: one transaction per 96 KiB segment and one for the header (README.md)
$( [ "$net" = local ] && echo "  LEZ  deploy records (STAND-IN light client, local only)" )
  EVM  vault          $( [ "$net" = local ] && echo "deploy.sh on anvil (mode B)" || echo "sepolia.sh (the owner's): deploy.sh, mode B, and two deposits" )
  EVM  deposits       depositETH x 2: 0.001 ETH + relay fee each; deposit 0 public kind, deposit 1 private kind
  EVM  capture        eth_getProof of the vault's slots 0, 1, 2 at a block holding both deposits, and its header
$( [ "$net" = local ] && echo "  LEZ  record         the stand-in writes its record of that block" || echo "  LEZ  (none)         the light client's record of that block: lez-v03/testnet.sh, slot FERRATA_LC_SLOT" )
  LEZ  Init           the vault's slot 2 = configId under the record
  LEZ  ImportRoot     n = 2
  LEZ  Claim          deposit 0 to the public holder; then its replay and a claim to another destination (refused)
  LEZ  ClaimPrivately deposit 1 to the private holder: a privacy-preserving transaction, REAL proof by the wallet
  LEZ  Burn           public holder, signed; then the private holder's (privacy-preserving, real proof)
  both verify         verify.py against the record
EOF
}

need_send() {
  [ "${FERRATA_TESTNET_SEND:-}" = yes ] || { echo "testnet.sh run: refused (set FERRATA_TESTNET_SEND=yes to send)" >&2; exit 2; }
}

evm_local() { # the vault on a plain local anvil, its two deposits, one capture
  local from=0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266   # anvil's account 0 (chain 31337 only)
  local vault
  vault="$(jq -r '.vault // empty' "$run/evm.json" 2>/dev/null || true)"
  if [ -z "$vault" ]; then
    local nonce; nonce="$(cast nonce "$from" --rpc-url "$eth")"
    [ "$nonce" = 0 ] || { echo "testnet.sh: anvil's account 0 has nonce $nonce; start a fresh anvil" >&2; exit 1; }
    vault="$(cast compute-address "$from" --nonce 3 | awk '{print $NF}')"   # verifier, stop, library, vault
    printf '{"vault":"%s","deployer":"%s"}\n' "$vault" "$from" > "$run/evm.json"
  fi
  b config --chain 31337 --vault "$vault" --instance "$instance" --eth "$eth_denomination" > "$run/config_id.txt"
  local cid; cid="$(cat "$run/config_id.txt")"
  # forge reads and writes only under vault/deployments/ (vault/foundry.toml fs_permissions); fork-* is ignored there.
  local dep="$repo/vault/deployments/fork-v03bridge-$(basename "$run")"
  mkdir -p "$repo/vault/deployments"
  if [ "$(jq -r '.complete // false' "$run/deploy-vault.json" 2>/dev/null)" != true ]; then
    jq --arg cid "$cid" '.lezConfigId = $cid | ._source = "lez-v03-bridge/testnet.sh: sepolia.json with this run'"'"'s v0.3.1 configuration id; the checkpoint and release image ids are the v0.4 programs'"'"' (no v0.3.1 checkpoint guest: the return leg is not built, RETURN.md)"' \
      "$repo/vault/script/params/sepolia.json" > "$dep-params.json"
    ( cd "$repo/vault" && FERRATA_MODE=B ETH_FROM="$from" FERRATA_UNLOCKED=1 FERRATA_RPC_URL="$eth" \
        FERRATA_PARAMS="$dep-params.json" FERRATA_OUT="$dep.json" FERRATA_LOG_DIR="$run/deploy-logs" \
        script/deploy.sh ) > "$run/deploy-vault.log" 2>&1 || { tail -20 "$run/deploy-vault.log"; exit 1; }
    cp "$dep.json" "$run/deploy-vault.json"; cp "$dep-params.json" "$run/params.json"
  fi
  [ "$(jq -r .vault "$run/deploy-vault.json" | tr A-F a-f)" = "$(echo "$vault" | tr A-F a-f)" ] || { echo "vault address differs" >&2; exit 1; }
  [ "$(cast call "$vault" 'storedLezConfigId()(bytes32)' --rpc-url "$eth")" = "$cid" ] || { echo "slot 2 differs" >&2; exit 1; }
  b deposits --seed "$seed" --depositor "$from" --j0 0 --leaf0 0 > "$run/deposits.json"
  for k in 0 1; do
    if [ ! -f "$run/deposit-$k.json" ]; then
      local c; c="$(jq -r ".[$k].commitment" "$run/deposits.json")"
      cast send "$vault" 'depositETH(uint8,bytes32,uint64)' 0 "0x$c" "$k" --value "$eth_total" \
        --from "$from" --unlocked --rpc-url "$eth" --json > "$run/deposit-$k.json"
    fi
  done
  if [ ! -d "$run/capture" ]; then
    python3 -I "$repo/lez/crown/capture-anvil.py" --rpc "$eth" --vault "$vault" --from-block 0 \
      --slot 0x0 --slot 0x1 --slot 0x2 "$run/capture" > "$run/capture.log" 2>&1
  fi
}

capture_args() { # --capture DIR --proof FILE --slot N for the run's capture
  local dir="${FERRATA_CAPTURE:-$run/capture}"
  local proof; proof="$(cd "$dir" && ls proof_0x*_*.json | head -1)"
  local block; block="$(echo "$proof" | sed 's/.*_\([0-9]*\)\.json/\1/')"
  local slot="${FERRATA_LC_SLOT:-$block}"   # the stand-in keys its record of anvil block n by n
  echo "--capture $dir --proof $proof --slot $slot"
}

# The public testnet: blocks hours apart, rewinds (spike/tmp/testnet-ops/FOR-SIBLINGS.md). So the public steps go as
# ONE batch for one block, through testnet-ops's queued sender (FERRATA_OPS: ferrata_ops; explicit nonces, tight
# declared gas), from the wallet home `prepare` made the accounts in; then the private claim (real proof, after the
# batch's block, which wrote the root it is checked against at settlement) and the private burn (after the claim's
# block: the wallet needs the private account's new commitment). Each step is recorded and skipped when done.
run_testnet() {
  need_send
  need_secrets
  jq -e .config "$run/run.json" > /dev/null 2>&1 || { echo "testnet.sh run: testnet.sh prepare first" >&2; exit 1; }
  : "${FERRATA_LC_SLOT:?FERRATA_LC_SLOT: the slot of the light client record of the captured block}" "${FERRATA_CAPTURE:?FERRATA_CAPTURE}"
  local n=2 cap="$FERRATA_CAPTURE" block home
  block="$(cd "$cap" && ls proof_0x*_*.json | head -1 | sed 's/.*_\([0-9]*\)\.json/\1/')"
  jq --argjson b "$block" --argjson n "$n" '.capture_block = $b | .capture_leaves = $n' "$run/run.json" > "$run/run.json.new" && mv "$run/run.json.new" "$run/run.json"
  home="$(jq -r '.wallet_home // empty' "$run/run.json")"; home="${home:-$run/wallet}"
  # Built and queued once (finding 12): the marker is written BEFORE ferrata_ops sends, so a re-run never rebuilds or
  # re-queues it; after a rewind or an interrupted send, re-send the same signed bytes with
  # `ferrata_ops resend --record $run/batch-record.json`.
  if [ -f "$run/batch.queued" ]; then
    log "the public batch was queued at $(cat "$run/batch.queued"): not rebuilt, not sent again"
  else
    local ops="${FERRATA_OPS:?FERRATA_OPS: testnet-ops ferrata_ops binary}"
    b batch $(capture_args) --n "$n" --seed "$seed" --recipient "$recipient" --out "$run/batch.json"
    log "the public batch (deploy, Init, ImportRoot, Claim of deposit 0, the public burn) through ferrata_ops"
    date -u +%FT%TZ > "$run/batch.queued"
    LEE_WALLET_HOME_DIR="$home" "$ops" batch --file "$run/batch.json" \
      --record "$run/batch-record.json" --wait-secs "${FERRATA_WAIT_SECS:-43200}"
  fi
  log "ClaimPrivately deposit 1 (real proof)"
  /usr/bin/time -v -o "$run/claim-private.time" "$drv" "$run" claim-private --deposit 1 --n "$n" $(capture_args)
  log "Burn (private holder; real proof)"
  /usr/bin/time -v -o "$run/burn-private.time" "$drv" "$run" burn-private --index 1 --recipient "$recipient" --seed "$seed"
  log "state"; b state > "$run/state.json"
  record
  [ -f "$run/batch-record.json" ] && cp "$run/batch-record.json" "$results/batch-record.json"
  verify
}

run_all() {
  [ "$net" = testnet ] && { run_testnet; return; }
  need_send
  mkdir -p "$run" "$results"
  [ -x "$drv" ] || { echo "testnet.sh: build the driver first (cargo build --release -p driver03)" >&2; exit 1; }
  log "setup"; b setup --sequencer "$lez" --payer "${FERRATA_PAYER_KEY:-testnet-a}" > "$run/setup.log"
  log "deploy bridge"; b deploy --program bridge
  if [ "$net" = local ]; then
    log "deploy the stand-in light client"; b deploy --program records
    log "vault, deposits, capture (anvil)"; evm_local
    log "the stand-in's record"; b record $(capture_args)
  else
    : "${FERRATA_VAULT:?FERRATA_VAULT}" "${FERRATA_LC_PROGRAM:?FERRATA_LC_PROGRAM}" "${FERRATA_LC_SLOT:?FERRATA_LC_SLOT}" "${FERRATA_CAPTURE:?FERRATA_CAPTURE}"
    b config --chain 11155111 --vault "$FERRATA_VAULT" --lc "$FERRATA_LC_PROGRAM" --instance "$instance" --eth "$eth_denomination" > "$run/config_id.txt"
    b deposits --seed "$seed" --depositor "${FERRATA_DEPOSITOR:?FERRATA_DEPOSITOR}" --j0 "${FERRATA_J0:-0}" --leaf0 "${FERRATA_LEAF0:-0}" > "$run/deposits.json"
  fi
  local n=$(( ${FERRATA_LEAF0:-0} + 2 ))
  local cap="${FERRATA_CAPTURE:-$run/capture}" block
  block="$(cd "$cap" && ls proof_0x*_*.json | head -1 | sed 's/.*_\([0-9]*\)\.json/\1/')"
  jq --argjson b "$block" --argjson n "$n" '.capture_block = $b | .capture_leaves = $n' "$run/run.json" > "$run/run.json.new" && mv "$run/run.json.new" "$run/run.json"
  log "Init"; b init $(capture_args)
  log "ImportRoot n = $n"; b import $(capture_args) --n "$n"
  log "Claim deposit 0 (public)"; b claim --deposit 0 --n "$n"
  log "its replay (refused)"; b claim --deposit 0 --n "$n" --step claim-0-replay
  log "deposit 0 to another destination (refused)"; b claim --deposit 0 --n "$n" --step claim-0-other-dest --dest "$(jq -r .payer "$run/run.json")"
  log "ClaimPrivately deposit 1 (real proof unless RISC0_DEV_MODE)"
  /usr/bin/time -v -o "$run/claim-private.time" "$drv" "$run" claim-private --deposit 1 --n "$n"
  log "Burn (public holder)"; b burn --index 0 --recipient "$recipient" --seed "$seed"
  log "Burn (private holder; real proof unless RISC0_DEV_MODE)"
  /usr/bin/time -v -o "$run/burn-private.time" "$drv" "$run" burn-private --index 1 --recipient "$recipient" --seed "$seed"
  log "state"; b state > "$run/state.json"
  record
  verify
}

record() { # results/<net>/: no key and no wallet; on a non-local network no secret-derived value either
  mkdir -p "$results"
  # A local run's seed is a public test value; elsewhere the deposits' s, dest and kind, the private holder and the
  # nullifier accounts (functions of s) stay in the run directory (review finding 2).
  local strip='del(.config, .wallet_home)'
  [ "$net" = local ] || strip='del(.config, .wallet_home, .holder_private) | if .deposits then .deposits |= map(del(.s, .dest, .kind)) else . end'
  jq "$strip" "$run/run.json" > "$results/run.json"
  if [ "$net" = local ]; then cp "$run/state.json" "$results/state.json"; else
    jq 'with_entries(select(.key | startswith("holder_private") | not)) | .accounts |= with_entries(select(.key | startswith("nullifier_") | not))' \
      "$run/state.json" > "$results/state.json"
  fi
  [ -f "$run/evm.json" ] && cp "$run/evm.json" "$results/evm.json"
  [ -f "$run/deposits.json" ] && { [ "$net" = local ] && cp "$run/deposits.json" "$results/deposits.json" || jq 'map(del(.s, .dest, .kind))' "$run/deposits.json" > "$results/deposits.json"; }
  for k in 0 1; do [ -f "$run/deposit-$k.json" ] && cp "$run/deposit-$k.json" "$results/deposit-$k.json"; done
  [ -f "$run/deploy-vault.json" ] && cp "$run/deploy-vault.json" "$results/deploy-vault.json"
  [ -f "$run/capture.log" ] && cp -r "${FERRATA_CAPTURE:-$run/capture}" "$results/capture"
  for f in claim-private.time burn-private.time; do
    [ -f "$run/$f" ] && grep -E "Elapsed|Maximum resident" "$run/$f" > "$results/$f.txt" || true
  done
  # No machine path in a committed file (finding 9): the run's and the repository's directories written relative.
  { grep -rlI -e "$spike/" "$results" 2>/dev/null || true; } | while read -r f; do sed -i "s#$spike/#spike/#g" "$f"; done
  log "record written to $results"
}

prepare() { # the public run's LEZ wallet, accounts and configuration, and the deposits' commitments: nothing sent
  [ "$net" = testnet ] || { echo "testnet.sh prepare: FERRATA_NET=testnet only (local runs do it in run)" >&2; exit 1; }
  need_secrets
  mkdir -p "$run"
  if [ -n "${FERRATA_WALLET_HOME:-}" ]; then   # the queued sender's wallet home: one home signs every transaction
    b setup --sequencer "$lez" --wallet-home "$FERRATA_WALLET_HOME" --payer-id "${FERRATA_PAYER_ID:?FERRATA_PAYER_ID}" > "$run/setup.log"
  else
    b setup --sequencer "$lez" --payer "${FERRATA_PAYER_KEY:?FERRATA_PAYER_KEY or FERRATA_WALLET_HOME}" > "$run/setup.log"
  fi
  b config --chain 11155111 --vault "${FERRATA_VAULT:?FERRATA_VAULT: sepolia.sh identity}" \
    --lc "${FERRATA_LC_PROGRAM:?FERRATA_LC_PROGRAM: the light client program account on the testnet}" \
    --instance "$instance" --eth "$eth_denomination" > "$run/config_id.txt"
  b deposits --seed "$seed" --depositor "${FERRATA_DEPOSITOR:?FERRATA_DEPOSITOR: the Sepolia depositor}" --j0 0 --leaf0 0 > "$run/deposits.json"
  log "configuration id $(cat "$run/config_id.txt") (the vault's lezConfigId); deposits' commitments in $run/deposits.json"
}

verify() {
  local any=(); [ "$net" = local ] && any=(--any-status)   # a standalone sequencer mocks Bedrock: no finality
  python3 -I "$here/verify.py" "${any[@]}" --lez "$lez" --eth "$eth" "$results"
}

case "$cmd" in
  plan) plan ;;
  prepare) prepare ;;
  run) run_all ;;
  verify) verify ;;
  *) echo "usage: testnet.sh plan | prepare | run | verify" >&2; exit 1 ;;
esac
