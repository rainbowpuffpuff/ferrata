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
set -euo pipefail
bundle="$(realpath "$1")"; out="$(realpath -m "$2")"; secs="${3:-300}"
lez="$(realpath ../../upstream/lez-v0.4.0-rc1)"; dev="$lez/lez/configs/docker-all-in-one/devnet"
work="${TMPDIR:?}/attestor-zone"; rm -rf "$work"; mkdir -p "$work/bundle" "$work/bedrock" "$out"
zstd -q -d -c "$bundle" | tar -C "$work/bundle" -xf -
( cd "$work/bundle" && sha256sum -c --quiet SHA256SUMS )
BIN="$work/bundle/bin"; NODE="$work/bundle/node/logos-blockchain-node"; chmod +x "$NODE" "$BIN"/*
export PATH="$BIN:$PATH" RISC0_SERVER_PATH="$BIN/r0vm" XDG_CACHE_HOME="$work/bundle/xdg-cache"
FUND=2e03b2eff5a45478e7e79668d2a146cf2c5c7925bce927f2b1c67f2ab4fc0d26   # the dev node's funded key (compose default)
CHANNEL=0101010101010101010101010101010101010101010101010101010101010101
PORT=18080; api="http://127.0.0.1:$PORT"
log() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$out/zone.log"; }

cp "$lez/bedrock/node-config.yaml" "$lez/bedrock/deployment-settings.yaml" "$work/bedrock/"
sed -i -e "s#listen_address: 0.0.0.0:18080#listen_address: 127.0.0.1:$PORT#" \
       -e 's#^    swarm:$#    swarm:\n      host: 127.0.0.1#' "$work/bedrock/node-config.yaml"
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
log "node up: $(curl -fsS -m 3 "$api/cryptarchia/info")"

for i in 0 1 2 3; do
  home="$work/seq-$i"; mkdir -p "$home"; cp "$dev/seq-$i/channel_signing_key" "$home/channel_signing_key"
  python3 -I - "$dev/sequencer_config.json" "$work/cfg-$i.json" "$api" "$i" <<'EOF'
import json, sys
src, dst, api, i = sys.argv[1:5]; i = int(i)
c = json.load(open(src))
c["bedrock_config"]["node_url"] = api
c["gossip"] = {"listen_addr": f"/ip4/127.0.0.1/udp/{19100 + i}/quic-v1",
               "bootstrap_peers": [f"/ip4/127.0.0.1/udp/{19100 + j}/quic-v1" for j in range(4) if j != i]}
c["metrics_listen_address"] = f"127.0.0.1:{19200 + i}"
json.dump(c, open(dst, "w"), indent=2)
EOF
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
rpc() { curl -fsS -m 5 -H 'content-type: application/json' -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$2\",\"params\":[]}" "http://127.0.0.1:$((19300 + $1))" 2>/dev/null | sed -n 's/.*"result":\([0-9]*\).*/\1/p'; }
end=$(( $(date +%s) + secs )); FIRST=""
while [ "$(date +%s)" -lt "$end" ]; do
  ch="$(curl -fsS -m 5 "$api/channel/$CHANNEL" 2>/dev/null || echo '{}')"
  keys=$(echo "$ch" | python3 -I -c 'import json,sys; c=json.load(sys.stdin); print(len(c.get("accredited_keys",[])), c.get("tip_sequencer"), c.get("tip_slot"))' 2>/dev/null || echo "-")
  ts=$(echo "$ch" | sed -n 's/.*"tip_slot":\([0-9]*\).*/\1/p'); [ -z "$FIRST" ] && [ -n "$ts" ] && FIRST="$ts"
  alive=0; for p in "${SEQ_PIDS[@]}"; do kill -0 "$p" 2>/dev/null && alive=$((alive + 1)); done
  log "channel keys/tip_sequencer/tip_slot: $keys | heights: $(rpc 0 getLastBlockId)/$(rpc 1 getLastBlockId)/$(rpc 2 getLastBlockId)/$(rpc 3 getLastBlockId) | alive $alive | lib $(curl -fsS -m 3 "$api/cryptarchia/info" | sed -n 's/.*"lib_slot":\([0-9]*\).*/\1/p')"
  sleep 20
done
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
