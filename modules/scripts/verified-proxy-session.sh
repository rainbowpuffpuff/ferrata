#!/usr/bin/env bash
# verified-proxy.sh run: one headless Logos Core (logosctl) with Logos's beacon_api_module, verified_proxy_module and
# eth_rpc_module beside ferrata_attest_module and ferrata_updater_module, on public Sepolia. Reads only: nothing is
# signed or sent to any chain. Every request goes to a public endpoint without an API key (User-Agent
# ferrata-verified-proxy for the plain reads; the modules' own clients otherwise).
#
#   1. A trusted block root: Lodestar's finalized header, cross-checked against publicnode (the block it holds canonical
#      at that slot, and its finalized epoch) and Nimbus (its header of that root).
#   2. verified_proxy_module configured for Sepolia from that root (execution and archive URL: ethpandaops, which keeps
#      every proof; beacon providers from beacon_api_module), started, and its status sampled while it syncs.
#   3. eth_rpc_module with Sepolia in `required` mode: the live runs' deposits (fetch_log_proof), the capture blocks'
#      headers and the vault's storage at them, eth_getProof, verify_checkpoint's execution check; then the same with
#      the mode `off` (eth_rpc_module straight to the endpoint), and plain reads of a second provider.
#   4. beacon_api_module.lc_request: the fixture's bootstrap, the updates from its period, the finality update, beside
#      the providers' own answers (JSON and SSZ); ferrata_updater_module run with its sources through beacon_api_module.
#   5. verified_proxy.py summary: every comparison (summary.md, summary.json).
#
# Env: LOGOSCTL, OUT (verified-proxy.sh's OUT: lgx-* links and check/), LOGOS_TMPDIR.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
: "${LOGOSCTL:?}" "${OUT:?}" "${LOGOS_TMPDIR:?}"
W="$OUT/session"
rm -rf "$W"
mkdir -p "$W/rep" "$W/plain" "$W/req"
SESSION="$LOGOS_TMPDIR/vps"
rm -rf "$SESSION"
CHAIN=11155111
EL=https://rpc.sepolia.ethpandaops.io
PN_EL=https://ethereum-sepolia-rpc.publicnode.com
LODESTAR=https://lodestar-sepolia.chainsafe.io
PN_BEACON=https://ethereum-sepolia-beacon-api.publicnode.com
NIMBUS=http://unstable.sepolia.beacon-api.nimbus.team
PROXY_PORT=18645
UA=ferrata-verified-proxy
log="$W/session.log"
note() { printf '%s %s\n' "$(date -u +%H:%M:%S)" "$*" | tee -a "$log"; }
ctl() { "$LOGOSCTL" --config-dir "$SESSION" "$@"; }
# call MODULE METHOD NAME [ARGS...]: rep/NAME.raw (logosctl's output), rep/NAME.json (the method's reply, a string
# result parsed as JSON when it is JSON), one line in calls.tsv.
call() {
  local m=$1 f=$2 name=$3 t0 rc=0
  shift 3
  t0=$(date +%s.%N)
  ctl --timeout "${CALL_MS:-400000}" call "$m" "$f" "$@" --json < /dev/null > "$W/rep/$name.raw" 2>&1 || rc=$?
  jq -c 'if (.result | type) == "string" then (.result | fromjson? // .result) else .result end' "$W/rep/$name.raw" \
    > "$W/rep/$name.json" 2>/dev/null || echo "{\"call_failed\":$rc}" > "$W/rep/$name.json"
  printf '%s\t%s\t%s\t%s\t%s\n' "$name" "$m" "$f" "$rc" "$(awk -v a="$t0" -v b="$(date +%s.%N)" 'BEGIN { printf "%.2f", b - a }')" \
    >> "$W/calls.tsv"
}
get() { curl -sS -m 60 -A "$UA" -H "accept: ${3:-application/json}" -o "$2" -w '%{http_code}' "$1"; }
rpc() { curl -sS -m 60 -A "$UA" -H 'content-type: application/json' --data-binary "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$2\",\"params\":$3}" -o "$4" "$1"; }
printf 'name\tmodule\tmethod\texit\tseconds\n' > "$W/calls.tsv"
note "session: logosctl $("$LOGOSCTL" --version 2>/dev/null | head -n 1)"
python3 -I "$here/verified_proxy.py" plan "$repo" "$W"

# ---- 1. the trusted root --------------------------------------------------------------------------------------------
note "1. trusted root"
for attempt in 1 2 3; do
  get "$LODESTAR/eth/v1/beacon/headers/finalized" "$W/plain/lodestar-finalized.json" > /dev/null
  root=$(jq -r .data.root "$W/plain/lodestar-finalized.json")
  slot=$(jq -r .data.header.message.slot "$W/plain/lodestar-finalized.json")
  get "$PN_BEACON/eth/v1/beacon/headers/$slot" "$W/plain/publicnode-header-at-slot.json" > /dev/null
  get "$PN_BEACON/eth/v1/beacon/states/head/finality_checkpoints" "$W/plain/publicnode-finality-checkpoints.json" > /dev/null
  get "$NIMBUS/eth/v1/beacon/headers/$root" "$W/plain/nimbus-header-of-root.json" > /dev/null
  python3 -I - "$W" "$root" "$slot" <<'PY' && break
import json, sys
w, root, slot = sys.argv[1], sys.argv[2], int(sys.argv[3])
ld = lambda n: json.load(open(f"{w}/plain/{n}.json"))
pn, fc, nb = ld("publicnode-header-at-slot"), ld("publicnode-finality-checkpoints"), ld("nimbus-header-of-root")
pn_root, pn_canon = pn["data"]["root"], pn["data"]["canonical"]
nb_root, nb_slot = nb["data"]["root"], int(nb["data"]["header"]["message"]["slot"])
fin_epoch = int(fc["data"]["finalized"]["epoch"])
agree = pn_root == root and pn_canon is True and nb_root == root and nb_slot == slot and fin_epoch * 32 >= slot
json.dump({"root": root, "slot": slot, "epoch": slot // 32, "lodestar": "finalized header",
           "publicnode": {"root_at_slot": pn_root, "canonical": pn_canon, "finalized_epoch": fin_epoch},
           "nimbus": {"root": nb_root, "slot": nb_slot}, "agree": agree}, open(f"{w}/trusted-root.json", "w"), indent=1)
sys.exit(0 if agree else 1)
PY
  note "the providers did not agree on $root (attempt $attempt); again in 30 s"
  sleep 30
done
root=$(jq -r .root "$W/trusted-root.json")
note "trusted root $root (slot $(jq -r .slot "$W/trusted-root.json")), agree=$(jq -r .agree "$W/trusted-root.json")"

# ---- 2. Logos Core and the proxy ------------------------------------------------------------------------------------
note "2. Logos Core"
mkdir -p "$W/home"
trap 'ctl daemon stop > /dev/null 2>&1 || true' EXIT
env HOME="$W/home" RISC0_DEV_MODE=1 "$LOGOSCTL" --config-dir "$SESSION" daemon start --detach >> "$log" 2>&1
# VP_VARIANT=graft: the two built with builder 5f74cdb (verified-proxy.sh build-logos-graft) instead of their own.
for m in beacon_api_module verified_proxy_module eth_rpc_module; do
  d="$OUT/lgx-$m"
  if [ "${VP_VARIANT:-own}" = graft ] && [ -d "$OUT/lgx-graft-$m" ]; then d="$OUT/lgx-graft-$m"; fi
  echo "$m: $(basename "$d")" >> "$W/variants.txt"
  ctl install "$d"/*.lgx -y >> "$log" 2>&1 || note "install $m failed"
done
for m in ferrata_attest_module ferrata_updater_module; do
  ctl install "$OUT/check/lgx-$m"/*.lgx -y >> "$log" 2>&1 || note "install $m failed"
done
for m in beacon_api_module verified_proxy_module eth_rpc_module ferrata_attest_module ferrata_updater_module; do
  ctl module load "$m" >> "$log" 2>&1 || note "load $m failed"
done
ctl module list --json > "$W/rep/modules-loaded.json" 2>&1 || true
for m in beacon_api_module verified_proxy_module eth_rpc_module; do ctl module show "$m" --json > "$W/rep/show-$m.json" 2>&1 || true; done

call beacon_api_module init_defaults bam-init_defaults
call beacon_api_module list_providers bam-providers "$CHAIN"
call beacon_api_module provider_health bam-health-lodestar "$CHAIN" lodestar
call beacon_api_module provider_health bam-health-nimbus "$CHAIN" nimbus-unstable
call verified_proxy_module supportedNetworks vp-networks
cat > "$W/req/vp-config.json" <<J
{"network": "sepolia", "trustedBlockRoot": "$root", "executionApiUrls": ["$EL"], "archiveUrls": ["$EL"],
 "beaconSource": "module", "callTimeoutMs": 240000, "queueTimeoutMs": 60000, "startTimeoutMs": 300000,
 "keepAlive": "interval", "httpServer": {"enabled": true, "host": "127.0.0.1", "port": $PROXY_PORT}}
J
call verified_proxy_module configure vp-configure "json:@$W/req/vp-config.json"
call verified_proxy_module getConfig vp-config
note "verified_proxy_module.start (the light client bootstraps from the root)"
call verified_proxy_module start vp-start
call verified_proxy_module status vp-status-0
rpc "$EL" eth_blockNumber '[]' "$W/plain/el-blocknumber-0.json"
for i in 1 2 3; do
  sleep 40
  call verified_proxy_module status "vp-status-$i"
  rpc "$EL" eth_blockNumber '[]' "$W/plain/el-blocknumber-$i.json"
done
call verified_proxy_module ethBlockNumber vp-blocknumber
call verified_proxy_module localEndpoint vp-endpoint

# ---- 3. eth_rpc_module, required mode, then off ---------------------------------------------------------------------
note "3. eth_rpc_module"
call eth_rpc_module set_network_scope rpc-scope both
call eth_rpc_module set_chain_config rpc-chain "$CHAIN" \
  "{\"endpoint\":\"$EL\",\"name\":\"sepolia\",\"testnet\":true,\"timeoutSecs\":60,\"verifiedTimeoutSecs\":240}"
call eth_rpc_module set_verified_proxy_mode rpc-mode-required "$CHAIN" required
call eth_rpc_module get_chain_config rpc-chain-config "$CHAIN"
for i in $(seq 1 12); do
  call eth_rpc_module verified_proxy_status rpc-gate "$CHAIN"
  [ "$(jq -r .usable "$W/rep/rpc-gate.json" 2>/dev/null)" = true ] && break
  sleep 5
done
call eth_rpc_module raw_rpc rpc-chainid "$CHAIN" eth_chainId '[]'

# The deposits' block-wide log index, from a plain read of the receipt (the vault's first log in it).
jq -c '.deposits[]' "$W/plan.json" | while read -r d; do
  id=$(jq -r .id <<< "$d"); tx=$(jq -r .tx <<< "$d"); vault=$(jq -r .vault <<< "$d")
  rpc "$EL" eth_getTransactionReceipt "[\"$tx\"]" "$W/plain/receipt-$id.json"
  li=$(jq -r --arg v "$vault" '[.result.logs[] | select((.address | ascii_downcase) == $v)][0].logIndex' "$W/plain/receipt-$id.json")
  echo "$id $(( li ))" >> "$W/log-indices.txt"
done

reads() { # PHASE: V (required) or D (off)
  local P=$1 id block li d cap vault i n
  jq -c '.deposits[]' "$W/plan.json" | while read -r d; do
    id=$(jq -r .id <<< "$d"); block=$(jq -r .block <<< "$d")
    li=$(awk -v k="$id" '$1 == k { print $2 }' "$W/log-indices.txt")
    jq -n --argjson b "$block" --argjson l "$li" --argjson c "$CHAIN" \
      '{block: $b, block_log_index: $l, eth: {via: "eth_rpc_module", chain_id: $c}, timeout_ms: 300000}' > "$W/req/flp-$id.json"
    call ferrata_attest_module fetch_log_proof "$P-flp-$id" "@$W/req/flp-$id.json"
    call eth_rpc_module raw_rpc "$P-hdr-$id" "$CHAIN" eth_getBlockByNumber "[\"$(printf '0x%x' "$block")\",false]"
  done
  jq -c '.captures[]' "$W/plan.json" | while read -r cap; do
    id=$(jq -r .id <<< "$cap"); block=$(printf '0x%x' "$(jq -r .block <<< "$cap")"); vault=$(jq -r .vault <<< "$cap")
    call eth_rpc_module raw_rpc "$P-hdr-$id" "$CHAIN" eth_getBlockByNumber "[\"$block\",false]"
    n=$(jq '.slots | length' <<< "$cap")
    for i in $(seq 0 $((n - 1))); do
      call eth_rpc_module raw_rpc "$P-sto-$id-$i" "$CHAIN" eth_getStorageAt \
        "[\"$vault\",\"$(jq -r ".slots[$i].slot" <<< "$cap")\",\"$block\"]"
    done
    call eth_rpc_module raw_rpc "$P-proof-$id" "$CHAIN" eth_getProof "[\"$vault\",$(jq -c '[.slots[].slot]' <<< "$cap"),\"$block\"]"
  done
  jq --argjson c "$CHAIN" '.verify_checkpoint | .execution = {name: "eth_rpc_module", via: "eth_rpc_module", chain_id: $c}' \
    "$W/plan.json" > "$W/req/verify_checkpoint-$P.json"
  call ferrata_attest_module verify_checkpoint "$P-verify_checkpoint" "@$W/req/verify_checkpoint-$P.json"
}
note "3a. required mode"
reads V
call verified_proxy_module status vp-status-after-reads
call eth_rpc_module verified_proxy_status rpc-gate-after "$CHAIN"
# verify_checkpoint's execution check against the proxy's own JSON-RPC endpoint (what the design names), read with curl.
jq --arg u "http://127.0.0.1:$PROXY_PORT" '.verify_checkpoint | .execution = {name: "verified_proxy_module-http", url: $u}' \
  "$W/plan.json" > "$W/req/verify_checkpoint-H.json"
call ferrata_attest_module verify_checkpoint H-verify_checkpoint "@$W/req/verify_checkpoint-H.json"
note "3b. mode off"
call eth_rpc_module set_verified_proxy_mode rpc-mode-off "$CHAIN" off
reads D
# Plain reads of a second provider (publicnode), outside Logos Core.
jq -r '.deposits[].block, .captures[].block' "$W/plan.json" | sort -u | while read -r b; do
  rpc "$PN_EL" eth_getBlockByNumber "[\"$(printf '0x%x' "$b")\",false]" "$W/plain/pn-hdr-$b.json"
done

# ---- 4. light-client data through beacon_api_module; the updater module on it ----------------------------------------
note "4. beacon_api_module and the updater"
croot=$(jq -r .checkpoint_root "$W/plan.json")
call beacon_api_module lc_request B-bootstrap "$CHAIN" getLightClientBootstrap "{\"block_root\":\"$croot\"}" "" 60000
get "$NIMBUS/eth/v1/beacon/light_client/bootstrap/$croot" "$W/plain/nimbus-bootstrap.json" > "$W/plain/nimbus-bootstrap.json.code"
get "$NIMBUS/eth/v1/beacon/light_client/bootstrap/$croot" "$W/plain/nimbus-bootstrap.ssz" application/octet-stream > "$W/plain/nimbus-bootstrap.ssz.code"
get "$LODESTAR/eth/v1/beacon/light_client/bootstrap/$croot" "$W/plain/lodestar-bootstrap.txt" application/octet-stream > "$W/plain/lodestar-bootstrap.code"
call beacon_api_module lc_request B-updates "$CHAIN" getLightClientUpdatesByRange '{"start_period":1411,"count":3}' lodestar 60000
get "$LODESTAR/eth/v1/beacon/light_client/updates?start_period=1411&count=3" "$W/plain/lodestar-updates.json" > /dev/null
get "$LODESTAR/eth/v1/beacon/light_client/updates?start_period=1411&count=3" "$W/plain/lodestar-updates.ssz" application/octet-stream > /dev/null
for attempt in 1 2 3 4 5; do
  call beacon_api_module lc_request B-finality "$CHAIN" getLightClientFinalityUpdate '{}' lodestar 60000
  get "$LODESTAR/eth/v1/beacon/light_client/finality_update" "$W/plain/lodestar-finality.json" > /dev/null
  get "$LODESTAR/eth/v1/beacon/light_client/finality_update" "$W/plain/lodestar-finality.ssz" application/octet-stream > /dev/null
  a=$(jq -r '.body | fromjson | .data.signature_slot' "$W/rep/B-finality.json" 2>/dev/null)
  b=$(jq -r .data.signature_slot "$W/plain/lodestar-finality.json" 2>/dev/null)
  [ -n "$a" ] && [ "$a" = "$b" ] && break
  sleep 2
done
# The updater: the fixture's checkpoint (period 1411), both sources through beacon_api_module (Lodestar first, Nimbus
# first), two needed to agree, on the in-process LEZ chain (the harness, dev mode).
cat > "$W/req/updater.toml" <<T
[daemon]
name = "verified-proxy-session"
state_dir = "$W/updater-state"
poll_secs = 0
ledger = "harness"

[keys]
payer_key_file = "$repo/lez/updater/dev/payer.key"

[policy]
defer_window = 1
min_agreeing = 2
max_periods_per_request = 16

[[chain]]
name = "sepolia"
preset = "sepolia"
layout = "gloas"
checkpoint = "${croot#0x}"
adoption_parts = 2

  [[chain.source]]
  name = "beacon_api_module-lodestar"
  via = "beacon_api_module"
  chain_id = $CHAIN
  provider = "lodestar"
  timeout_secs = 60

  [[chain.source]]
  name = "beacon_api_module-nimbus"
  via = "beacon_api_module"
  chain_id = $CHAIN
  provider = "nimbus-unstable"
  timeout_secs = 60
T
jq -n --arg c "$W/req/updater.toml" '{config_file: $c}' > "$W/req/updater-check.json"
call ferrata_updater_module check_config U-check_config "@$W/req/updater-check.json"
jq -n --arg c "$W/req/updater.toml" '{config_file: $c, rounds: 80, until_up_to_date: true, quiet: false}' > "$W/req/updater-run.json"
CALL_MS=1800000 call ferrata_updater_module run U-run "@$W/req/updater-run.json"
# The instance's finalized header, against publicnode's canonical block at that slot.
fslot=$(jq -r '[.. | objects | select(has("finalized_slot")) | .finalized_slot][0] // empty' "$W/rep/U-run.json")
froot=$(jq -r '[.. | objects | select(has("finalized_root")) | .finalized_root][0] // empty' "$W/rep/U-run.json")
if [ -n "$fslot" ]; then
  get "$PN_BEACON/eth/v1/beacon/headers/$fslot" "$W/plain/publicnode-header-updater.json" > /dev/null
  pr=$(jq -r .data.root "$W/plain/publicnode-header-updater.json")
  jq -n --arg s "$fslot" --arg r "$froot" --arg p "$pr" \
    '{slot: $s, instance_root: $r, publicnode_root: $p, agree: (($r | ascii_downcase | ltrimstr("0x")) == ($p | ascii_downcase | ltrimstr("0x")))}' \
    > "$W/plain/updater-finalized-check.json"
fi

call verified_proxy_module status vp-status-end
call verified_proxy_module stop vp-stop
cp "$SESSION/logs/daemon.log" "$W/daemon.log" 2>/dev/null || true
ctl daemon stop >> "$log" 2>&1 || true
note "session done"
