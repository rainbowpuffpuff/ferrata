#!/usr/bin/env bash
# A zone of four LEZ v0.4.0-rc1 sequencers on one Logos Blockchain node, without Docker: upstream's own
# four-sequencer devnet (lez/configs/docker-all-in-one/devnet: one config whose genesis stakes all four
# channel signing keys, and the four keys), run with the lab bundle's binaries (sequencer_service,
# logos-blockchain-node, r0vm) on loopback, gossip by explicit addresses instead of mDNS. seq-0 creates the
# channel; the others wait for it. After <seconds> the sequencers stop, the node runs on until its LIB has
# passed their last inscription, and the node's answers (/cryptarchia/info, /channel/<id>, /cryptarchia/blocks)
# are saved for the attestors. Run from checkpoint/ on the build box:
#
#   attestor/zone.sh <bundle.tar.zst> <out dir> [seconds]
#
# With SILENT=i SILENT_AFTER=s in the environment, sequencer i is stopped s seconds into the run (a silent
# sequencer: its turns come and nobody of its key posts).
#
# Hooks (modules/scripts/attestor-zone-live.sh uses them to run ferrata_attestor_module beside each sequencer):
# Another LEZ tag (RFP-022 F.7 on rc3): ZONE_LEZ names the LEZ tree whose devnet configuration is used (default
# upstream/lez-v0.4.0-rc1; rc3's is the same), ZONE_BEDROCK the directory of the node's node-config.yaml and
# deployment-settings.yaml (default the tree's bedrock/; `bundle`: the bundle's lez-config/, as crown-rc3's bundle has).
#
# LEZ v0.4.0 (released 27 Nov 2026): ZONE_LEZ=../../upstream/lez-v0.4.0 and a bundle from attestor/zone-bundle.sh v0.4.0
# (the node 0.4.0-rc.2, deployment settings in the V2 era schema, the sequencers' finality_depth in place of exit_delay,
# the devnet's own regenerated keys); the tag is read from the tree's directory name and checked against the bundle's
# lez-config/lez-tag.txt and the configurations (attestor/zone-lib.sh, tested by attestor/test_zone_lib.py).
#
# ZONE_DURING is run (bash -c) once the sequencers are started, ZONE_BEFORE_STOP when the <seconds> are over
# and the sequencers still run (sequencer i's LEZ node answers on 127.0.0.1:1930i: an attestor's last round
# needs it), ZONE_AFTER once the sequencers have stopped and the answers are saved, while the node still runs;
# all with ZONE_API (the node's HTTP API), ZONE_CHANNEL and ZONE_OUT (<out dir>) in their environment, their
# output in <out dir>/during.log, before-stop.log and after.log. ZONE_AFTER's exit status is the script's.
set -euo pipefail
bundle="$(realpath "$1")"; out="$(realpath -m "$2")"; secs="${3:-300}"
lez="$(realpath "${ZONE_LEZ:-../../upstream/lez-v0.4.0-rc1}")"; dev="$lez/lez/configs/docker-all-in-one/devnet"
. "$(dirname "${BASH_SOURCE[0]}")/zone-lib.sh"
TAG="$(zl_tag_of_tree "$lez")" && zl_known_tag "$TAG" || { echo "zone.sh: $lez is not a LEZ tree of a known tag"; exit 2; }
work="${TMPDIR:?}/attestor-zone"; rm -rf "$work"; mkdir -p "$work/bundle" "$work/bedrock" "$out"
zstd -q -d -c "$bundle" | tar -C "$work/bundle" -xf -
( cd "$work/bundle" && sha256sum -c --quiet SHA256SUMS )
# v0.4.0's configurations and binaries go together only with each other (rc1's devnet configuration with an rc3 bundle,
# as the F.7 runs used, stays allowed: the two are byte-identical)
btag=""; [ -f "$work/bundle/lez-config/lez-tag.txt" ] && btag="$(zl_tag_of_tree "$work/bundle/lez-config")"
if { zl_is_v040 "$TAG" || zl_is_v040 "$btag"; } && [ "$btag" != "$TAG" ]; then
  echo "zone.sh: the bundle is LEZ ${btag:-of no tag (no lez-config/lez-tag.txt)}, the tree $TAG"; exit 2; fi
BIN="$work/bundle/bin"; NODE="$work/bundle/node/logos-blockchain-node"; chmod +x "$NODE" "$BIN"/*
export PATH="$BIN:$PATH" RISC0_SERVER_PATH="$BIN/r0vm" XDG_CACHE_HOME="$work/bundle/xdg-cache"
FUND=2e03b2eff5a45478e7e79668d2a146cf2c5c7925bce927f2b1c67f2ab4fc0d26   # the dev node's funded key (compose default)
CHANNEL=0101010101010101010101010101010101010101010101010101010101010101
PORT=18080; api="http://127.0.0.1:$PORT"
log() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$out/zone.log"; }

bedrock_src="${ZONE_BEDROCK:-$lez/bedrock}"; [ "$bedrock_src" = bundle ] && bedrock_src="$work/bundle/lez-config"
cp "$bedrock_src/node-config.yaml" "$bedrock_src/deployment-settings.yaml" "$work/bedrock/"
zl_node_loopback "$work/bedrock/node-config.yaml" "$PORT"
if zl_is_v040 "$TAG"; then
  [ "$(zl_era_schema "$work/bedrock/deployment-settings.yaml")" = V2 ] || { echo "zone.sh: v0.4.0's deployment settings are not in the V2 era schema"; exit 2; }
fi
sha256sum "$work/bedrock/deployment-settings.yaml" | cut -d' ' -f1 > "$out/deployment-settings.sha256"
cp "$work/bedrock/deployment-settings.yaml" "$out/deployment-settings.yaml"
( cd "$work/bedrock" && POL_PROOF_DEV_MODE=true exec "$NODE" node-config.yaml --deployment deployment-settings.yaml ) \
  > "$work/bedrock.log" 2>&1 &
NODE_PID=$!; SEQ_PIDS=()
stop_seqs() { for p in "${SEQ_PIDS[@]}"; do kill -TERM "$p" 2>/dev/null || true; done; sleep 5;
              for p in "${SEQ_PIDS[@]}"; do kill -KILL "$p" 2>/dev/null || true; done; SEQ_PIDS=(); }
stop_all() { stop_seqs; kill -TERM "$NODE_PID" 2>/dev/null || true; sleep 3; kill -KILL "$NODE_PID" 2>/dev/null || true;
             mkdir -p "$out/logs"; for f in "$work"/*.log; do tail -n 300 "$f" > "$out/logs/$(basename "$f")"; done; }
trap stop_all EXIT
for i in $(seq 1 600); do curl -fsS -m 3 -o /dev/null "$api/cryptarchia/info" 2>/dev/null && break; kill -0 "$NODE_PID"; sleep 0.2; done
log "node up (LEZ $TAG): $(curl -fsS -m 3 "$api/cryptarchia/info")"

for i in 0 1 2 3; do
  home="$work/seq-$i"; mkdir -p "$home"; cp "$dev/seq-$i/channel_signing_key" "$home/channel_signing_key"
  zl_seq_cfg "$dev/sequencer_config.json" - "$work/cfg-$i.json" "$api" "$i" 4 "$TAG"
  flag=""; [ "$i" = 0 ] && flag="--create-channel"
  sequencer_service setup "$home" --config "$work/cfg-$i.json" --funding-public-key "$FUND" $flag > "$work/setup-$i.log" 2>&1 \
    || { log "setup $i failed"; tail -n 20 "$work/setup-$i.log"; exit 2; }
  log "seq-$i set up: $(grep -o 'channel signing public key *[0-9a-f]*' "$work/setup-$i.log" | awk '{print $NF}')"
done
for i in 0 1 2 3; do
  ( export RUST_LOG="info,sequencer_core=debug,risc0_zkvm=warn,kameo=warn"; [ "$i" != 0 ] && export SEQUENCER_WAIT_FOR_CHANNEL=true
    exec env -u RISC0_DEV_MODE sequencer_service start "$work/seq-$i" --rpc-listen-address "127.0.0.1:$((19300 + i))" ) \
    > "$work/seq-$i.log" 2>&1 &
  SEQ_PIDS+=($!)
  [ "$i" = 0 ] && sleep 15
done
if [ -n "${ZONE_DURING:-}" ]; then
  ZONE_API="$api" ZONE_CHANNEL="$CHANNEL" ZONE_OUT="$out" bash -c "$ZONE_DURING" > "$out/during.log" 2>&1 \
    || log "ZONE_DURING exited $?"
fi
rpc() { curl -fsS -m 5 -H 'content-type: application/json' -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$2\",\"params\":[]}" "http://127.0.0.1:$((19300 + $1))" 2>/dev/null | sed -n 's/.*"result":\([0-9]*\).*/\1/p'; }
start=$(date +%s); end=$(( start + secs )); FIRST=""; SILENCED=""
while [ "$(date +%s)" -lt "$end" ]; do
  ch="$(curl -fsS -m 5 "$api/channel/$CHANNEL" 2>/dev/null || echo '{}')"
  keys=$(echo "$ch" | python3 -I -c 'import json,sys; c=json.load(sys.stdin); print(len(c.get("accredited_keys",[])), c.get("tip_sequencer"), c.get("tip_slot"))' 2>/dev/null || echo "-")
  ts=$(echo "$ch" | sed -n 's/.*"tip_slot":\([0-9]*\).*/\1/p'); [ -z "$FIRST" ] && [ -n "$ts" ] && FIRST="$ts"
  if [ -n "${SILENT:-}" ] && [ -z "$SILENCED" ] && [ $(( $(date +%s) - start )) -ge "${SILENT_AFTER:-60}" ]; then
    kill -TERM "${SEQ_PIDS[$SILENT]}" 2>/dev/null || true; SILENCED=1; log "seq-$SILENT stopped (silent from now on)"
  fi
  alive=0; for p in "${SEQ_PIDS[@]}"; do kill -0 "$p" 2>/dev/null && alive=$((alive + 1)); done
  log "channel keys/tip_sequencer/tip_slot: $keys | heights: $(rpc 0 getLastBlockId)/$(rpc 1 getLastBlockId)/$(rpc 2 getLastBlockId)/$(rpc 3 getLastBlockId) | alive $alive | lib $(curl -fsS -m 3 "$api/cryptarchia/info" | sed -n 's/.*"lib_slot":\([0-9]*\).*/\1/p')"
  sleep 20
done
if [ -n "${ZONE_BEFORE_STOP:-}" ]; then
  ZONE_API="$api" ZONE_CHANNEL="$CHANNEL" ZONE_OUT="$out" bash -c "$ZONE_BEFORE_STOP" > "$out/before-stop.log" 2>&1 \
    || log "ZONE_BEFORE_STOP exited $?"
fi
stop_seqs
log "sequencers stopped; waiting for the LIB to pass the channel's tip"
tip_slot=$(curl -fsS -m 5 "$api/channel/$CHANNEL" | sed -n 's/.*"tip_slot":\([0-9]*\).*/\1/p')
for i in $(seq 1 90); do
  lib=$(curl -fsS -m 3 "$api/cryptarchia/info" | sed -n 's/.*"lib_slot":\([0-9]*\).*/\1/p')
  [ -n "$lib" ] && [ -n "$tip_slot" ] && [ "$lib" -ge "$tip_slot" ] && break; sleep 2
done
log "channel tip slot $tip_slot, lib $lib"
mkdir -p "$out/answers/blocks"
curl -fsS -m 10 -o "$out/answers/info.json" "$api/cryptarchia/info"
curl -fsS -m 10 -o "$out/answers/channel.json" "$api/channel/$CHANNEL"
# From before the channel's first inscription seen (its slots are the node's wall-clock slots).
lib=$(sed -n 's/.*"lib_slot":\([0-9]*\).*/\1/p' "$out/answers/info.json"); from=$(( ${FIRST:-0} > 600 ? FIRST - 600 : 0 ))
while [ "$from" -le "$lib" ]; do to=$((from + 499)); [ "$to" -gt "$lib" ] && to="$lib"
  curl -fsS -m 60 -o "$out/answers/blocks/$from-$to.json" "$api/cryptarchia/blocks?slot_from=$from&slot_to=$to"; from=$((to + 1)); done
python3 -I - "$out/answers/blocks" <<'EOF'
import json, os, sys
d = sys.argv[1]; files = sorted(os.listdir(d), key=lambda n: int(n.split("-")[0])); kept = []
for f in files:
    kept += json.load(open(os.path.join(d, f))); os.remove(os.path.join(d, f))
slots = [b["header"]["slot"] for b in kept]
json.dump(kept, open(os.path.join(d, f"{min(slots)}-{max(slots)}.json"), "w"), separators=(",", ":"))
EOF
log "answers saved: $(ls "$out/answers/blocks" | wc -l) windows, $(cat "$out"/answers/blocks/*.json | wc -c) bytes; channel: $(cat "$out/answers/channel.json")"
if [ -n "${ZONE_AFTER:-}" ]; then
  rc=0
  ZONE_API="$api" ZONE_CHANNEL="$CHANNEL" ZONE_OUT="$out" bash -c "$ZONE_AFTER" > "$out/after.log" 2>&1 || rc=$?
  log "ZONE_AFTER exited $rc"
  exit $rc
fi
