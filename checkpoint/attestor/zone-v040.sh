#!/usr/bin/env bash
# LEZ v0.4.0 on the build box, beside attestor/zone.sh's zone of four (which runs v0.4.0 with
# ZONE_LEZ=../../upstream/lez-v0.4.0 and a bundle of attestor/zone-bundle.sh v0.4.0). Run from checkpoint/:
#
#   attestor/zone-v040.sh standalone <bundle> <out dir> [seconds]
#       LEZ v0.4.0's sequencer_service built with its `standalone` feature (its mock Bedrock actor: no node) on LEZ's
#       local.json, and LEZ's wallet of the same tag with its storage sealed under a password (LEZ PR 987, in rc3 already): check-health,
#       LEZ's published local test account imported by `--secret` (PR 1135), a new public account, a public transfer
#       between them, its receipt from the sequencer (getTransactionReceipt, PR 1107), the storage file checked to be
#       sealed; then (ZONE_PRIVATE=1, the default) a transfer into a new private account of the wallet, the privacy
#       circuit proven for real by the wallet and verified by the sequencer, and its receipt. With ZONE_PROGRAM=<elf>[:<image id>], that program is deployed by `wallet program-loader deploy` at fresh
#       accounts (immutable), paid by the test account, and its header read back; with ZONE_CALL=<program id>:<message
#       hex>:<to actor>, `wallet program call` sends that message. The sequencer runs at least <seconds> (default 120).
#   attestor/zone-v040.sh vote <bundle> <out dir>
#       The committee-voted LEZ parameters (PR 1089) on a Logos Blockchain 0.4.0-rc.2 node and four sequencers: upstream's
#       devnet configuration without its founding stakes, so sequencer 0 creates the channel and stakes its own key;
#       three more keys staked from this run's wallet (`sequencer_service stake`, whose ownership accounts are the
#       wallet's), funded by LEZ's published local test accounts; the channel accrediting four keys; then
#       `sequencer_service vote-params` by those three owners for a new posting timeframe: the first two votes change
#       nothing (t(4) = 3), the third replaces the params, and the Bedrock channel takes the new posting timeframe;
#       then (ZONE_UNSTAKE=1, the default) LEZ PR 1026's L1 finality: one of the three keys unstaked to a fresh (funded) account,
#       and the node's height recorded when the unstake lands, when the channel drops the key, and when the stake is paid.
#
# Scratch in $TMPDIR (default this tree's ignored .scratch/). ZONE_LEZ (default ../../upstream/lez-v0.4.0): LEZ's tree (its Justfile's published test secrets, its wallet
# configuration, its devnet configuration). The records (facts.json, the commands' outputs, the logs' tails) go to
# <out dir> (RECS), with this machine's paths scrubbed (scripts/scrub_records.py). Leader proofs in dev mode (as every LEZ devnet
# here: POL_PROOF_DEV_MODE=true); the sequencers verify real proofs (RISC0_DEV_MODE unset).
set -euo pipefail
MODE="${1:?standalone | vote}"; bundle="$(realpath "${2:?bundle}")"; RECS="$(realpath -m "${3:?out dir}")"; secs="${4:-120}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"; REPO="$(cd "$HERE/../.." && pwd)"; SPIKE="$(cd "$REPO/.." && pwd)"
. "$HERE/zone-lib.sh"
lez="$(realpath "${ZONE_LEZ:-$SPIKE/upstream/lez-v0.4.0}")"; TAG="$(zl_tag_of_tree "$lez")" || true
zl_is_v040 "$TAG" || { echo "zone-v040.sh: $lez is not LEZ v0.4.0"; exit 2; }
work="$(realpath -m "${TMPDIR:-$REPO/.scratch}")/zone-v040-$MODE"; rm -rf "$work"; mkdir -p "$work/bundle" "$work/logs" "$RECS"
zstd -q -d -c "$bundle" | tar -C "$work/bundle" -xf -
( cd "$work/bundle" && sha256sum -c --quiet SHA256SUMS )
[ "$(zl_tag_of_tree "$work/bundle/lez-config")" = v0.4.0 ] || { echo "zone-v040.sh: the bundle is not LEZ v0.4.0"; exit 2; }
BIN="$work/bundle/bin"; NODE="$work/bundle/node/logos-blockchain-node"; chmod +x "$NODE" "$BIN"/*
export PATH="$BIN:$PATH" RISC0_SERVER_PATH="$BIN/r0vm" XDG_CACHE_HOME="$work/bundle/xdg-cache"
FUND=2e03b2eff5a45478e7e79668d2a146cf2c5c7925bce927f2b1c67f2ab4fc0d26   # the dev node's funded key (compose default)
CHANNEL=0101010101010101010101010101010101010101010101010101010101010101
PORT=18080; API="http://127.0.0.1:$PORT"
FACTS="$work/facts.jsonl"; : > "$FACTS"
log() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$RECS/run.log"; }
die() { log "STOP: $*"; exit 1; }
fact() { python3 -I -c 'import json,sys; print(json.dumps({sys.argv[1]: json.loads(sys.argv[2])}))' "$1" "$2" >> "$FACTS"; }
fact_s() { fact "$1" "$(python3 -I -c 'import json,sys; print(json.dumps(sys.argv[1]))' "$2")"; }
rpc() { # url method [params json]: the JSON-RPC result (compact JSON), empty on an error
  curl -fsS -m 10 -H 'content-type: application/json' -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$2\",\"params\":${3:-[]}}" "$1" 2>/dev/null \
    | python3 -I -c 'import json,sys; d=json.load(sys.stdin); print(json.dumps(d["result"], separators=(",", ":")) if "result" in d else "")' 2>/dev/null || true
}
PIDS=()
cleanup() { # (the EXIT trap) the processes stopped, the records written and scrubbed; a scrub that refuses fails the run
  local rc=$?
  for p in "${PIDS[@]:-}"; do [ -n "$p" ] && kill -TERM "$p" 2>/dev/null || true; done; sleep 4
  for p in "${PIDS[@]:-}"; do [ -n "$p" ] && kill -KILL "$p" 2>/dev/null || true; done
  mkdir -p "$RECS/logs"
  for f in "$work"/logs/*.log; do [ -f "$f" ] && sed -e 's/\x1b\[[0-9;]*m//g' "$f" | tail -n 400 | cut -c1-2000 > "$RECS/logs/$(basename "$f")"; done
  python3 -I -c '
import json, sys
d = {}
for line in open(sys.argv[1]):
    if line.strip():
        d.update(json.loads(line))
json.dump(d, open(sys.argv[2], "w"), indent=1, sort_keys=True)' "$FACTS" "$RECS/facts.json" || true
  if ! python3 -I "$REPO/scripts/scrub_records.py" "$RECS" --map "$work/=work/" --map "$REPO/=" --scratch "$SPIKE/tmp=tmp/" \
      --map "$SPIKE/=spike/" --map "$HOME/=~/" > "$RECS/scrub.txt" 2>&1; then
    echo "zone-v040.sh: the records name this machine (scrub.txt)" >&2; [ "$rc" -ne 0 ] || rc=1
  fi
  # a run that failed without saying why (a command that failed under set -e) still ends with a STOP line
  if [ "$rc" -ne 0 ] && ! tail -n 1 "$RECS/run.log" 2>/dev/null | grep -q 'STOP:'; then
    echo "[$(date -u +%H:%M:%S)] STOP: exit $rc without a STOP line (a command failed; see logs/)" | tee -a "$RECS/run.log" >&2
  fi
  trap - EXIT; exit "$rc"
}
trap cleanup EXIT
fact_s lez "$TAG $(zl_lez_commit "$TAG")"; fact_s bundle_sha256 "$(sha256sum "$bundle" | cut -c1-64)"
for b in sequencer_service sequencer_service_standalone wallet r0vm; do fact_s "sha256_$b" "$(sha256sum "$BIN/$b" | cut -c1-64)"; done

# ---- the wallet: LEZ's own CLI of this tag, its storage sealed under a password made here (never written to the records)
W="$work/wallet"; mkdir -p "$W"
PASSWORD="$(head -c 24 /dev/urandom | od -An -tx1 | tr -d ' \n')"
wallet_for() { # url: the wallet's configuration, then its storage created by a first command, whose output (it prints the
                #      new wallet's recovery phrase) stays in the scratch directory, out of the records
  zl_wallet_config "$lez/lez/wallet/configs/debug/wallet_config.json" "$W/wallet_config.json" "$1" || die "wallet config"
  w account list > "$work/wallet-init.private" 2>&1 || true
  [ -s "$W/storage.json" ] || die "the wallet made no storage ($(grep -v -i -E 'phrase|^  [a-z]+( [a-z]+){11,}' "$work/wallet-init.private" | tail -n 3))"
  ! grep -q -i 'recovery phrase' "$work/logs/"*.log 2>/dev/null || die "a recovery phrase reached logs/"
}
w() { LEE_WALLET_HOME_DIR="$W" LEE_WALLET_PASSWORD="$PASSWORD" env -u RISC0_DEV_MODE wallet "$@"; }
import_published() { # the published local test accounts, by --secret (v0.4.0's form): their ids, one per line
  local s id
  for s in $(zl_published_secrets "$lez/Justfile"); do
    id="$(w account import public --secret "$s" 2>&1 | tee -a "$work/logs/wallet.log" | zl_public_id)"
    [ -n "$id" ] || die "importing a published test account (logs/wallet.log)"
    echo "$id"
  done
}

# ==== standalone ======================================================================================================
standalone() {
  local H="$work/seq" R=http://127.0.0.1:19390 cfg="$work/seq-config.json" h=""
  python3 -I - "$work/bundle/lez-config/local.json" "$cfg" <<'PY'
import json, sys
c = json.load(open(sys.argv[1]))
assert "finality_depth" in c["genesis"]["channel_params"] and "exit_delay" not in c["genesis"]["channel_params"]
c["gossip"] = {"listen_addr": "/ip4/127.0.0.1/udp/19190/quic-v1", "bootstrap_peers": []}
c["metrics_listen_address"] = "127.0.0.1:19290"
json.dump(c, open(sys.argv[2], "w"), indent=2)
PY
  sequencer_service_standalone setup "$H" --config "$cfg" --funding-public-key "$FUND" --create-channel > "$work/logs/setup.log" 2>&1 \
    || { tail -n 20 "$work/logs/setup.log"; die "setup"; }
  ( export RUST_LOG="info,risc0_zkvm=warn,kameo=warn"; exec env -u RISC0_DEV_MODE sequencer_service_standalone start "$H" \
      --rpc-listen-address 127.0.0.1:19390 ) > "$work/logs/sequencer.log" 2>&1 &
  PIDS+=($!); local t0; t0="$(date +%s)"
  for _ in $(seq 1 180); do h="$(rpc "$R" getLastBlockId)"; [ -n "$h" ] && [ "$h" -ge 2 ] 2>/dev/null && break; kill -0 "${PIDS[0]}" || die "the sequencer exited"; sleep 1; done
  [ -n "$h" ] && [ "$h" -ge 2 ] || die "no block 2 from the standalone sequencer"
  log "standalone sequencer up: block $h after $(( $(date +%s) - t0 )) s"; fact standalone_first_height "$h"
  wallet_for "$R"
  w check-health > "$work/logs/check-health.log" 2>&1 && fact check_health true || { fact check_health false; log "check-health: $(tail -n 3 "$work/logs/check-health.log")"; }
  local A="" B tx out rcpt
  local ids=()   # (through a file: a die inside a substitution would stop only the substitution)
  import_published > "$work/published.ids" || die "importing the published test accounts (logs/wallet.log)"
  mapfile -t ids < "$work/published.ids"; A="${ids[0]:-}"
  [ "${#ids[@]}" -ge 2 ] && [ -n "$A" ] && [ -n "${ids[1]}" ] || die "not every published test account was imported (logs/wallet.log)"
  log "published test account imported by --secret: $A"; fact_s test_account "$A"
  B="$(w account new public 2>&1 | tee -a "$work/logs/wallet.log" | zl_public_id)"; [ -n "$B" ] || die "account new public"
  fact_s new_account "$B"
  out="$(w auth-transfer send --from "$A" --to "$B" --amount 1000 2>&1 | tee -a "$work/logs/wallet.log")" || die "auth-transfer send: $(echo "$out" | tail -n 3)"
  tx="$(echo "$out" | zl_tx_hash)"; [ -n "$tx" ] || die "no transaction hash in the transfer's output"
  log "public transfer $A -> $B (1000): $tx"; fact_s transfer_tx "$tx"
  for _ in $(seq 1 60); do rcpt="$(rpc "$R" getTransactionReceipt "[\"$tx\"]")"; [ -n "$rcpt" ] && [ "$rcpt" != null ] && break; sleep 2; done
  [ -n "$rcpt" ] && [ "$rcpt" != null ] || die "no receipt for $tx (getTransactionReceipt)"
  log "receipt: $rcpt"; fact transfer_receipt "$rcpt"
  # v0.4.0's block format, as the sequencer serves it (getBlock: base64 Borsh), for the readers of blocks
  # (deploy/testnet/host/bin/ferrata-status, ferrata-bridge-identity): block 2 and the transfer's block
  local tb; tb="$(rpc "$R" getTransaction "[\"$tx\"]" | python3 -I -c 'import json,sys; d=json.load(sys.stdin); print(d[1] if isinstance(d, list) and len(d) == 2 else "")' 2>/dev/null || true)"
  [[ "$tb" =~ ^[0-9]+$ ]] || die "no block height for the transfer $tx (getTransaction)"
  mkdir -p "$RECS/blocks"
  for b in 2 "$tb"; do
    rpc "$R" getBlock "[$b]" > "$RECS/blocks/block-$b.json"
    python3 -I -c 'import base64, json, struct, sys; x = json.load(open(sys.argv[1])); assert isinstance(x, str) and struct.unpack("<Q", base64.b64decode(x)[:8])[0] == int(sys.argv[2])' \
      "$RECS/blocks/block-$b.json" "$b" || die "getBlock $b gave no base64 block $b"
  done
  fact transfer_block "$tb"
  w account get --account-id "$B" > "$work/logs/account-b.log" 2>&1 || w account show "$B" > "$work/logs/account-b.log" 2>&1 || true
  if [ "${ZONE_PRIVATE:-1}" = 1 ]; then fact private_enabled true; private_transfer "$R" "$A"; else fact private_enabled false; fi
  fact_s new_account_view "$(tr '\n' ' ' < "$work/logs/account-b.log" | cut -c1-400)"
  if zl_storage_sealed "$W/storage.json"; then fact wallet_storage_sealed true; log "the wallet's storage.json is sealed (version, kdf, nonce, ciphertext)"
  else fact wallet_storage_sealed false; die "the wallet's storage.json is not sealed"; fi
  if [ -n "${ZONE_PROGRAM:-}" ]; then deploy_program "$A"; fi
  if [ -n "${ZONE_CALL:-}" ]; then call_program "$A"; fi
  local left=$(( secs - ($(date +%s) - t0) )); [ "$left" -gt 0 ] && sleep "$left"
  h="$(rpc "$R" getLastBlockId)"; log "standalone sequencer at block $h after $(( $(date +%s) - t0 )) s"; fact standalone_last_height "$h"
  log "standalone done"
}

private_transfer() { # url payer: a public account's funds into a new private account of the wallet, the privacy
                     # circuit proven for real by LEZ's wallet (r0vm of the bundle, RISC0_DEV_MODE unset), verified by the
                     # sequencer; its receipt (the storage gas is the transaction's bytes since LEZ PR 1044)
  local R="$1" payer="$2" P out tx rcpt t0 secs
  P="$(w account new private 2>&1 | tee -a "$work/logs/wallet.log" | { grep -o -E 'Private/[1-9A-HJ-NP-Za-km-z]{32,44}' || true; } | sed -n 1p)"
  [ -n "$P" ] || die "account new private"
  t0="$(date +%s)"
  out="$(w auth-transfer send --from "$payer" --to "$P" --amount 500 2>&1 | tee -a "$work/logs/private.log")" || die "private transfer: $(echo "$out" | tail -n 3)"
  secs=$(( $(date +%s) - t0 )); tx="$(echo "$out" | zl_tx_hash)"; [ -n "$tx" ] || die "no transaction hash in the private transfer's output"
  log "private transfer $payer -> $P (500), proven and sent in $secs s: $tx"
  fact_s private_account "$P"; fact_s private_tx "$tx"; fact private_seconds "$secs"
  for _ in $(seq 1 60); do rcpt="$(rpc "$R" getTransactionReceipt "[\"$tx\"]")"; [ -n "$rcpt" ] && [ "$rcpt" != null ] && break; sleep 2; done
  [ -n "$rcpt" ] && [ "$rcpt" != null ] || die "no receipt for the private transfer $tx"
  log "private transfer's receipt: $rcpt"; fact private_receipt "$rcpt"
}

deploy_program() { # payer: ZONE_PROGRAM=<elf>[:<image id>] by `wallet program-loader deploy`, immutable, fresh accounts
  local payer="$1" elf="${ZONE_PROGRAM%%:*}" want="" got H segs=() n size t0 out
  [ "$elf" != "$ZONE_PROGRAM" ] && want="${ZONE_PROGRAM#*:}"
  got="$(r0vm --elf "$elf" --id 2>/dev/null | tail -n 1 || true)"
  if ! [[ "${got#0x}" =~ ^[0-9a-f]{64}$ ]]; then
    log "r0vm gave no image id for $(basename "$elf") ($got): deployed with the one given ($want), not checked here"; got="$want"
  elif [ -n "$want" ] && [ "0x${got#0x}" != "0x${want#0x}" ]; then die "$elf: image id $got, not $want"; fi
  size="$(stat -c %s "$elf")"; n=$(( (size + 98303) / 98304 ))
  H="$(w account new public 2>&1 | zl_public_id)"; for _ in $(seq 1 "$n"); do segs+=("$(w account new public 2>&1 | zl_public_id)"); done
  t0="$(date +%s)"
  if ! out="$(w program-loader deploy --elf "$elf" --header "$H" --segments "${segs[@]}" --immutable --payer "$payer" 2>&1)"; then
    echo "$out" >> "$work/logs/deploy.log"
    n="$(echo "$out" | sed -n 's/.*splits into \([0-9]*\) segment.*/\1/p' | tail -n 1)"; [ -n "$n" ] || die "program-loader deploy (logs/deploy.log)"
    while [ "${#segs[@]}" -lt "$n" ]; do segs+=("$(w account new public 2>&1 | zl_public_id)"); done; segs=("${segs[@]:0:$n}")
    out="$(w program-loader deploy --elf "$elf" --header "$H" --segments "${segs[@]}" --immutable --payer "$payer" 2>&1)" \
      || { echo "$out" >> "$work/logs/deploy.log"; die "program-loader deploy (logs/deploy.log)"; }
  fi
  echo "$out" >> "$work/logs/deploy.log"
  log "deployed $(basename "$elf") (image id $got, $size bytes) at header $H with ${#segs[@]} segments in $(( $(date +%s) - t0 )) s"
  fact program_deploy "$(python3 -I -c 'import json,sys; print(json.dumps({"elf": sys.argv[1], "sha256": sys.argv[2], "image_id": sys.argv[3], "header": sys.argv[4], "segments": int(sys.argv[5]), "seconds": int(sys.argv[6])}))' \
    "$(basename "$elf")" "$(sha256sum "$elf" | cut -c1-64)" "$got" "$H" "${#segs[@]}" "$(( $(date +%s) - t0 ))")"
  w account get --account-id "$H" > "$work/logs/header.log" 2>&1 || w account show "$H" > "$work/logs/header.log" 2>&1 || true
}

call_program() { # payer: ZONE_CALL=<program id>:<message hex>:<to actor> by `wallet program call`
  local payer="$1" prog to msg out tx
  prog="${ZONE_CALL%%:*}"; msg="${ZONE_CALL#*:}"; to="${msg#*:}"; msg="${msg%%:*}"
  out="$(w program call --program "$prog" --message-hex "$msg" --to "$to" --payer "$payer" 2>&1)" \
    || { echo "$out" >> "$work/logs/call.log"; die "program call (logs/call.log)"; }
  echo "$out" >> "$work/logs/call.log"; tx="$(echo "$out" | zl_tx_hash)"
  log "program call to $prog: $tx"; fact_s program_call_tx "$tx"
}

# ==== vote ============================================================================================================
seq_url() { echo "http://127.0.0.1:$((19300 + $1))"; }
start_seq() { # i [wait for the channel]
  ( export RUST_LOG="info,sequencer_core=debug,risc0_zkvm=warn,kameo=warn"; [ -n "${2:-}" ] && export SEQUENCER_WAIT_FOR_CHANNEL=true
    exec env -u RISC0_DEV_MODE sequencer_service start "$work/seq-$1" --rpc-listen-address "127.0.0.1:$((19300 + $1))" ) \
    > "$work/logs/seq-$1.log" 2>&1 &
  PIDS+=($!)
}
channel() { curl -fsS -m 5 "$API/channel/$CHANNEL" 2>/dev/null || echo '{}'; }
heights() { local i o=""; for i in 0 1 2 3; do o="$o$(rpc "$(seq_url $i)" getLastBlockId)/"; done; echo "${o%/}"; }

node_height() { curl -fsS -m 5 "$API/cryptarchia/info" 2>/dev/null | python3 -I -c 'import json,sys; d=json.load(sys.stdin); print(d.get("height", (d.get("cryptarchia_info") or {}).get("height", "")))' 2>/dev/null || true; }
# (empty when the wallet fails: one failed read during the release poll is retried, not the end of the run)
balance_of() { { w account get --account-id "$1" 2>/dev/null || true; } | sed -n 's/.*Balance \([0-9][0-9]*\),.*/\1/p' | sed -n 1p; }

unstake_final() { # key payer: LEZ PR 1026's L1 finality: the key's whole stake released to a fresh account of the wallet
                  # (funded first by payer: the stake program pays only an account that exists), which must not be paid
                  # before the configuration removing the key is finality_depth node blocks deep; the node's height when
                  # the unstake lands, when the channel drops the key, and when the release is paid
  local key="$1" payer="$2" D out fd h0 h1="" h2="" t0 t1="" t2="" n b="" base
  fd="$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["genesis"]["channel_params"]["finality_depth"])' "$work/cfg-0.json")" \
    || die "no finality_depth in the sequencers' genesis (cfg-0.json)"
  D="$(w account new public 2>&1 | zl_public_id)"; [ -n "$D" ] || die "account new public (the release's destination)"
  w auth-transfer send --from "$payer" --to "$D" --amount 1000 >> "$work/logs/unstake.log" 2>&1 || die "funding the release's destination"
  base="$(balance_of "$D")"; [ -n "$base" ] || die "no balance for $D after its funding"
  fact release_destination_before "$base"
  t0="$(date +%s)"
  out="$(LEE_WALLET_HOME_DIR="$W" LEE_WALLET_PASSWORD="$PASSWORD" env -u RISC0_DEV_MODE sequencer_service unstake --wallet-home "$W" \
    --sequencer-key "$key" --destination "$D" 2>&1 | tee -a "$work/logs/unstake.log")" || die "unstake: $(echo "$out" | tail -n 3)"
  h0="$(node_height)"
  log "unstake of ${key:0:16}… to $D landed at node height $h0 (finality_depth $fd): $(echo "$out" | grep -m1 '^Releasing' || echo "$out" | tail -n 1)"
  fact finality_depth "$fd"; fact_s unstake_out "$(echo "$out" | tail -n 2 | tr '\n' ' ')"; fact_s unstake_destination "$D"
  for _ in $(seq 1 180); do
    n="$(channel | zl_channel_summary | cut -d' ' -f1)"
    if [ -z "$t1" ] && [ "$n" = 3 ]; then t1="$(date +%s)"; h1="$(node_height)"; log "the channel dropped the key: 3 keys, node height $h1, $(( t1 - t0 )) s after the unstake"; fi
    b="$(balance_of "$D")"
    if [ -n "$b" ] && [ "$b" -gt "$base" ] 2>/dev/null; then t2="$(date +%s)"; h2="$(node_height)"; break; fi
    sleep 10
  done
  fact unstake_node_height "\"$h0\""; fact key_removed_node_height "\"$h1\""; fact key_removed_after_s "\"${t1:+$(( t1 - t0 ))}\""
  if [ -n "$t2" ]; then
    log "the stake ($(( b - base ))) was paid to $D at node height $h2, $(( t2 - t0 )) s after the unstake"
    fact release_paid "$(( b - base ))"; fact release_node_height "\"$h2\""; fact release_after_s "$(( t2 - t0 ))"
  else
    log "no release paid to $D in $(( $(date +%s) - t0 )) s"; fact release_paid 0
  fi
}

vote() {
  local dev="$lez/lez/configs/docker-all-in-one/devnet" i k keys=() acct=() pt pt_new n out
  mkdir -p "$work/bedrock"
  cp "$work/bundle/lez-config/node-config.yaml" "$work/bundle/lez-config/deployment-settings.yaml" "$work/bedrock/"
  zl_node_loopback "$work/bedrock/node-config.yaml" "$PORT"
  [ "$(zl_era_schema "$work/bedrock/deployment-settings.yaml")" = V2 ] || die "the deployment settings are not V2"
  ( cd "$work/bedrock" && POL_PROOF_DEV_MODE=true exec "$NODE" node-config.yaml --deployment deployment-settings.yaml ) \
    > "$work/logs/bedrock.log" 2>&1 &
  PIDS+=($!)
  for _ in $(seq 1 600); do curl -fsS -m 3 -o /dev/null "$API/cryptarchia/info" 2>/dev/null && break; sleep 0.2; done
  log "node up: $(curl -fsS -m 3 "$API/cryptarchia/info")"; fact node_info_start "$(curl -fsS -m 3 "$API/cryptarchia/info")"
  zl_no_founding_stakes "$dev/sequencer_config.json" "$work/devnet-no-stakes.json"
  for i in 0 1 2 3; do
    mkdir -p "$work/seq-$i"
    zl_seq_cfg "$work/devnet-no-stakes.json" "$work/bundle/lez-config/local.json" "$work/cfg-$i.json" "$API" "$i" 4 v0.4.0
    local flag=""; [ "$i" = 0 ] && flag="--create-channel"
    sequencer_service setup "$work/seq-$i" --config "$work/cfg-$i.json" --funding-public-key "$FUND" $flag > "$work/logs/setup-$i.log" 2>&1 \
      || { tail -n 20 "$work/logs/setup-$i.log"; die "setup $i"; }
    keys[$i]="$(grep -o 'channel signing public key *[0-9a-f]*' "$work/logs/setup-$i.log" | awk '{print $NF}')"
    log "seq-$i set up: channel key ${keys[$i]}; $(grep -m1 -E 'stakes that key|Pass that key' "$work/logs/setup-$i.log")"
  done
  fact channel_keys "$(printf '%s\n' "${keys[@]}" | python3 -I -c 'import json,sys; print(json.dumps(sys.stdin.read().split()))')"
  start_seq 0
  for _ in $(seq 1 150); do n="$(channel | zl_channel_summary | cut -d' ' -f1)"; [ "$n" = 1 ] && break; sleep 2; done
  [ "$n" = 1 ] || die "the channel does not accredit sequencer 0's key: $(channel)"
  for _ in $(seq 1 90); do k="$(rpc "$(seq_url 0)" getLastBlockId)"; [ -n "$k" ] && [ "$k" -ge 2 ] 2>/dev/null && break; sleep 2; done
  log "channel created by seq-0 (one key, its own genesis stake): $(channel | zl_channel_summary); height $k"
  fact channel_created "$(channel)"
  wallet_for "$(seq_url 0)"
  import_published > "$work/published.ids" || die "importing the published test accounts (logs/wallet.log)"
  mapfile -t acct < "$work/published.ids"
  [ "${#acct[@]}" -ge 2 ] && [ -n "${acct[0]}" ] && [ -n "${acct[1]}" ] || die "not every published test account was imported"
  log "published test accounts: ${acct[*]}"
  for i in 1 2 3; do
    out="$(LEE_WALLET_HOME_DIR="$W" LEE_WALLET_PASSWORD="$PASSWORD" env -u RISC0_DEV_MODE sequencer_service stake --wallet-home "$W" \
      --sequencer-key "${keys[$i]}" --from "${acct[$(( (i - 1) % ${#acct[@]} ))]}" 2>&1 | tee -a "$work/logs/stake.log")" \
      || die "stake $i: $(echo "$out" | tail -n 3)"
    log "seq-$i's key staked: $(echo "$out" | grep -m1 '^Staking' || echo "$out" | tail -n 1)"
    start_seq "$i" wait
  done
  local t0; t0="$(date +%s)"
  for _ in $(seq 1 90); do n="$(channel | zl_channel_summary | cut -d' ' -f1)"; [ "$n" = 4 ] && break; sleep 10; done
  [ "$n" = 4 ] || die "the channel does not accredit four keys $(( $(date +%s) - t0 )) s after the stakes: $(channel)"
  log "the channel accredits four keys $(( $(date +%s) - t0 )) s after the last stake; heights $(heights)"
  channel > "$RECS/channel-four-keys.json"; fact four_keys_after_s "$(( $(date +%s) - t0 ))"
  pt="$(channel | zl_channel_param posting_timeframe)"; [ -n "$pt" ] || die "no posting_timeframe in the channel: $(channel)"
  pt_new=$(( pt + 30 )); log "posting timeframe in force: $pt slots; the three owners vote for $pt_new"
  fact posting_timeframe_before "$pt"; fact posting_timeframe_voted "$pt_new"
  for i in 1 2 3; do
    out="$(LEE_WALLET_HOME_DIR="$W" LEE_WALLET_PASSWORD="$PASSWORD" env -u RISC0_DEV_MODE sequencer_service vote-params --wallet-home "$W" \
      --sequencer-key "${keys[$i]}" --posting-timeframe "$pt_new" 2>&1 | tee -a "$work/logs/vote.log")" || die "vote $i: $(echo "$out" | tail -n 3)"
    log "vote $i: $(echo "$out" | tail -n 1)"; fact_s "vote_$i" "$(echo "$out" | tail -n 2 | tr '\n' ' ')"
    sleep 2; fact "channel_posting_timeframe_after_vote_$i" "\"$(channel | zl_channel_param posting_timeframe)\""
  done
  t0="$(date +%s)"
  for _ in $(seq 1 120); do [ "$(channel | zl_channel_param posting_timeframe)" = "$pt_new" ] && break; sleep 10; done
  if [ "$(channel | zl_channel_param posting_timeframe)" = "$pt_new" ]; then
    log "the Bedrock channel's posting timeframe is $pt_new, $(( $(date +%s) - t0 )) s after the third vote"; fact bedrock_took_params true
  else
    log "the Bedrock channel's posting timeframe is still $(channel | zl_channel_param posting_timeframe) $(( $(date +%s) - t0 )) s after the third vote"
    fact bedrock_took_params false
  fi
  channel > "$RECS/channel-after-vote.json"
  if [ "${ZONE_UNSTAKE:-1}" = 1 ]; then fact unstake_enabled true; unstake_final "${keys[3]}" "${acct[1]}"; else fact unstake_enabled false; fi
  fact heights_end "\"$(heights)\""
  fact node_info_end "$(curl -fsS -m 3 "$API/cryptarchia/info")"
  log "vote done"
}

case "$MODE" in
  standalone) standalone ;;
  vote) vote ;;
  *) sed -n '2,30p' "${BASH_SOURCE[0]}"; exit 2 ;;
esac
