#!/usr/bin/env bash
# EXPERIMENTAL (README.md). The Sepolia side of the public-testnet run: a FerrataVault v5 (vault/, as it is) whose
# storedLezConfigId is the id of lez-v03-bridge's testnet configuration, and its two 0.001 ETH deposits (deposit 0
# public kind, deposit 1 private kind). One stage per call; every receipt recorded in <run>/sepolia.json.
#
#   lez-v03-bridge/sepolia.sh <run> identity      the deployer's nonce, the vault's address (CREATE), nothing sent
#   lez-v03-bridge/sepolia.sh <run> deploy        vault/script/deploy.sh, mode B, eip8037 (needs <run>/config_id.txt)
#   lez-v03-bridge/sepolia.sh <run> deposit 0|1   depositETH(0, C, j) (needs <run>/deposits.json)
#   lez-v03-bridge/sepolia.sh <run> status        the vault's slots 0, 1, 2 and depositCount, read-only
#   lez-v03-bridge/sepolia.sh --fork <run> all    the REHEARSAL: all of it on a local anvil fork of Sepolia
#                                                 (the deployer impersonated, nothing reaches Sepolia)
#
# Owner's mode (not --fork): FERRATA_SEPOLIA_SEND=yes, SEPOLIA_RPC_URL, ETH_FROM, and the key through Foundry's own
# ETH_KEYSTORE + ETH_PASSWORD (files; this script never reads them). Mode B behind RISC Zero's Groth16 verifier and
# the emergency stop, as crown-live-3: nothing can release or refund without a real proof, and no v0.3.1 checkpoint
# guest exists (RETURN.md), so the deposits stay escrowed until a return leg exists; an accept-all test verifier would
# let anyone release them. The Groth16 verifier, the FerrataRegistry library and the test token of crown-live-3
# (e2e/runs/crown-live-3/deploy.json) are reused: deploy.sh counts a recorded address that has the expected code,
# so it sends the stop, the vault, the two registrations and the stop's renounce only.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"; repo="$(cd "$here/.." && pwd)"
fork=0; [ "${1:-}" = --fork ] && { fork=1; shift; }
# The rehearsal's stages run as children of `--fork <run> all`, against its loopback fork.
# Only for the anvil fork this script started itself (review finding 13): its process id, alive, an anvil forking on
# port 8547; a loopback endpoint that is anything else (a node, a tunnel) keeps the send guard.
own_fork() {
  local pid="${FERRATA_SEPOLIA_FORK_PID:-}" cmd
  [ -n "$pid" ] && [ -r "/proc/$pid/cmdline" ] || return 1
  cmd="$(tr '\0' ' ' < "/proc/$pid/cmdline")"
  case "$cmd" in *anvil*--fork-url*--port\ 8547*) ;; *) return 1 ;; esac
  [ "${SEPOLIA_RPC_URL:-}" = http://127.0.0.1:8547 ]
}
[ "${FERRATA_SEPOLIA_FORK:-}" = 1 ] && own_fork && fork=2
run="${1:?run directory}"; stage="${2:?stage}"; arg="${3:-}"
mkdir -p "$run"; run="$(cd "$run" && pwd)"
case "$run" in "$repo"/*) echo "sepolia.sh: the run directory must be outside the repository" >&2; exit 1 ;; esac
crown="$repo/e2e/runs/crown-live-3/deploy.json"
total=1002000000000000   # 0.001 ETH and the 0.2% relay fee of schedule 0
say() { echo "[$(date -u +%FT%TZ)] $*"; }
die() { say "STOP: $*"; exit 1; }
rec() { # rec STAGE RECEIPT-JSON: append to <run>/sepolia.json
  local f="$run/sepolia.json"; [ -f "$f" ] || echo '{}' > "$f"
  jq --arg s "$1" --argjson r "$2" '.[$s] = $r' "$f" > "$f.new" && mv "$f.new" "$f"
}

if [ "$fork" = 1 ]; then
  upstream="${SEPOLIA_RPC_URL:?SEPOLIA_RPC_URL: the Sepolia endpoint the fork reads from}"
  export ETH_FROM="${ETH_FROM:-0x79A95aE9A3f9000eb41F7CBAB6a6F97826895C07}"   # crown-live-3's deployer, impersonated
  export SEPOLIA_RPC_URL=http://127.0.0.1:8547 FERRATA_UNLOCKED=1 FERRATA_SEPOLIA_FORK=1
  anvil --fork-url "$upstream" --port 8547 --auto-impersonate --silent > "$run/anvil-fork.log" 2>&1 &
  apid=$!; trap 'kill $apid 2>/dev/null || true' EXIT
  export FERRATA_SEPOLIA_FORK_PID=$apid
  for _ in $(seq 1 60); do cast chain-id --rpc-url "$SEPOLIA_RPC_URL" > /dev/null 2>&1 && break; sleep 1; done
  cast rpc anvil_setBalance "$ETH_FROM" 0x56BC75E2D63100000 --rpc-url "$SEPOLIA_RPC_URL" > /dev/null   # 100 ETH, fork only
  say "fork of Sepolia at block $(cast block-number --rpc-url "$SEPOLIA_RPC_URL"), deployer $ETH_FROM impersonated"
  if [ "$stage" = all ]; then
    "$here/sepolia.sh" "$run" identity
    if [ ! -f "$run/config_id.txt" ]; then
      # The rehearsal's own LEZ side, no network: a configuration of a bridge account the driver makes, a placeholder
      # light client (FERRATA_LC_PROGRAM, default: that account) and instance, this vault; and the deposits' openings.
      drv="${BRIDGE03:-$here/target/release/bridge03}"; [ -x "$drv" ] || die "build the driver (cargo build --release -p driver03)"
      head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n' > "$run/payer.key"
      # A throwaway wallet (no LEZ node is contacted): a random password, never written anywhere.
      FERRATA_WALLET_PASSWORD="$(head -c 16 /dev/urandom | od -An -tx1 | tr -d ' \n')" \
        "$drv" "$run" setup --sequencer https://testnet.lez.logos.co --payer "$run/payer.key" > "$run/setup.log"
      "$drv" "$run" config --chain 11155111 --vault "$(jq -r .vault "$run/identity.json")" \
        --lc "${FERRATA_LC_PROGRAM:-$(jq -r .bridge "$run/run.json")}" \
        --instance "${FERRATA_LC_INSTANCE:-$(printf 'ferrata/lez-v03-bridge/fork-rehearsal' | sha256sum | cut -c1-64)}" \
        --eth 1000000000000000 > "$run/config_id.txt"
      "$drv" "$run" deposits --seed "ferrata/lez-v03-bridge: fork rehearsal seed (test value)" --depositor "$ETH_FROM" > "$run/deposits.json"
    fi
    for s in deploy "deposit 0" "deposit 1" status; do "$here/sepolia.sh" "$run" $s; done
    exit 0
  fi
elif [ "$fork" = 0 ]; then
  : "${SEPOLIA_RPC_URL:?SEPOLIA_RPC_URL}" "${ETH_FROM:?ETH_FROM}"
  case "$stage" in deploy|deposit) [ "${FERRATA_SEPOLIA_SEND:-}" = yes ] || die "set FERRATA_SEPOLIA_SEND=yes to send" ;; esac
fi
rpc=(--rpc-url "$SEPOLIA_RPC_URL")
chain="$(cast chain-id "${rpc[@]}")"
[ "$chain" = 11155111 ] || die "chain $chain is not Sepolia"
signer=(); [ "${FERRATA_UNLOCKED:-}" = 1 ] && signer=(--unlocked)

case "$stage" in
  identity)
    [ -f "$run/identity.json" ] && { cat "$run/identity.json"; exit 0; }
    nonce="$(cast nonce "$ETH_FROM" "${rpc[@]}")"
    pending="$(cast nonce "$ETH_FROM" --block pending "${rpc[@]}")"
    [ "$nonce" = "$pending" ] || die "a pending transaction (nonce $nonce, pending $pending)"
    vault="$(cast compute-address "$ETH_FROM" --nonce $((nonce + 1)) | awk '{print $NF}')"   # the stop, then the vault
    jq -n --arg d "$ETH_FROM" --argjson n "$nonce" --arg v "$vault" '{deployer: $d, nonce: $n, vault: $v, chainId: 11155111}' > "$run/identity.json"
    cat "$run/identity.json"
    ;;
  deploy)
    cid="$(cat "$run/config_id.txt")"; vault="$(jq -r .vault "$run/identity.json")"
    # forge reads and writes only under vault/deployments/ (vault/foundry.toml fs_permissions): fork-* (vault/.gitignore)
    # for the rehearsal, v03bridge-* (the repository's .gitignore) for Sepolia; both copied into the run directory.
    dep="$repo/vault/deployments/$([ "$fork" != 0 ] && echo fork-)v03bridge-$(basename "$run")"
    mkdir -p "$repo/vault/deployments"
    jq --arg cid "$cid" '.lezConfigId = $cid | ._source = "lez-v03-bridge/sepolia.sh: vault/script/params/sepolia.json with lez-v03-bridge'"'"'s testnet configuration id; the checkpoint and release image ids are the v0.4 programs'"'"' (no v0.3.1 checkpoint guest exists: RETURN.md)"' \
      "$repo/vault/script/params/sepolia.json" > "$dep-params.json"
    out="$dep.json"
    [ -f "$out" ] || jq -n --slurpfile c "$crown" --arg d "$ETH_FROM" \
      '{chainId: 11155111, mode: "B", deployer: $d, groth16Verifier: $c[0].groth16Verifier, registry: $c[0].registry, token: $c[0].token}' > "$out"
    ( cd "$repo/vault" && FERRATA_MODE=B FERRATA_GAS_RULE=eip8037 FERRATA_RPC_URL="$SEPOLIA_RPC_URL" FERRATA_PARAMS="$dep-params.json" \
        FERRATA_OUT="$out" FERRATA_LOG_DIR="$run/deploy-logs" FERRATA_GROTH16_VERIFIER="$(jq -r .groth16Verifier "$crown")" \
        script/deploy.sh ) 2>&1 | tee "$run/deploy-vault.log"
    [ "$(jq -r .vault "$out" | tr A-F a-f)" = "$(echo "$vault" | tr A-F a-f)" ] || die "the vault is not at the identity's address"
    [ "$(cast call "$vault" 'storedLezConfigId()(bytes32)' "${rpc[@]}")" = "$cid" ] || die "slot 2 is not the configuration id"
    cp "$out" "$run/deploy-vault.json"; cp "$dep-params.json" "$run/params.json"
    rec deploy "$(jq -c . "$out")"
    ;;
  deposit)
    k="${arg:?deposit 0 or 1}"; vault="$(jq -r .vault "$run/identity.json")"
    [ -n "$(jq -r ".\"deposit-$k\" // empty" "$run/sepolia.json" 2>/dev/null)" ] && { say "deposit $k: done"; exit 0; }
    c="0x$(jq -r ".[$k].commitment" "$run/deposits.json")"
    # A deposit whose send may have gone out is never sent again (review finding 4): the marker is written before the
    # send, and a depositor counter other than the one the record expects stops the script (the vault takes any C at
    # index == depositCount, so a second send would escrow again and shift every later leaf).
    [ -f "$run/deposit-$k.sending" ] && die "deposit $k: a send began at $(cat "$run/deposit-$k.sending") and has no receipt here: check the depositor's depositCount and its transactions on chain, then record it or remove the marker"
    j="$(cast call "$vault" 'depositCount(address)(uint64)' "$ETH_FROM" "${rpc[@]}" | awk '{print $1}')"
    want="$(jq -r ".[$k].leaf_index" "$run/deposits.json")"
    [ "$j" = "$want" ] || die "deposit $k: the depositor's counter is $j, the record expects $want: nothing sent"
    date -u +%FT%TZ > "$run/deposit-$k.sending"
    r="$(cast send "$vault" 'depositETH(uint8,bytes32,uint64)' 0 "$c" "$j" --value "$total" --from "$ETH_FROM" \
        "${signer[@]}" "${rpc[@]}" --json)"
    rm -f "$run/deposit-$k.sending"
    rec "deposit-$k" "$(echo "$r" | jq -c '{transactionHash, blockNumber, gasUsed, effectiveGasPrice, status}')"
    say "deposit $k: $(echo "$r" | jq -r '.transactionHash + " block " + .blockNumber + " gas " + .gasUsed')"
    ;;
  status)
    vault="$(jq -r .vault "$run/identity.json")"
    for s in 0 1 2; do echo "slot $s: $(cast storage "$vault" $s "${rpc[@]}")"; done
    echo "depositCount($ETH_FROM): $(cast call "$vault" 'depositCount(address)(uint64)' "$ETH_FROM" "${rpc[@]}")"
    ;;
  *) die "unknown stage $stage" ;;
esac
