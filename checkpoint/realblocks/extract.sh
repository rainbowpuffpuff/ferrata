#!/usr/bin/env bash
# Reads the lab's Bedrock inscriptions from a COPY of one of its saved generations (never the lab):
# unpacks the lab's bundle and the generation into scratch, starts the copied Logos Blockchain node
# alone (no sequencer, as a lab session starts it: lez/lab/session.sh), asks it for the channel
# (/channel/<id>), its chain info and its immutable blocks (/cryptarchia/blocks, final by the node's
# own rule), stops it and leaves the answers in <out>. The copy is thrown away afterwards.
#   extract.sh <bundle.tar.zst> <gen-N.tar.zst> <out dir>
set -euo pipefail
bundle="$(realpath "$1")"; gen="$(realpath "$2")"; out="$(realpath -m "$3")"
work="${TMPDIR:?}/extract-work"; rm -rf "$work"; mkdir -p "$work/bundle" "$work/state" "$out"
zstd -q -d -c "$bundle" | tar -C "$work/bundle" -xf -
( cd "$work/bundle" && sha256sum -c --quiet SHA256SUMS )
zstd -q -d -c "$gen" | tar -C "$work/state" -xf -
node="$work/bundle/node/logos-blockchain-node"; chmod +x "$node"
export XDG_CACHE_HOME="$work/bundle/xdg-cache"
port=18080; api="http://127.0.0.1:$port"; channel=0101010101010101010101010101010101010101010101010101010101010101
( cd "$work/state/bedrock" && POL_PROOF_DEV_MODE=true exec "$node" node-config.yaml --deployment deployment-settings.yaml ) \
  > "$out/node.log" 2>&1 &
pid=$!
stop() { kill -TERM "$pid" 2>/dev/null || true; for i in $(seq 1 300); do kill -0 "$pid" 2>/dev/null || return 0; sleep 0.2; done; kill -KILL "$pid" 2>/dev/null || true; }
trap stop EXIT
for i in $(seq 1 600); do curl -fsS -m 3 -o "$out/info-start.json" "$api/cryptarchia/info" 2>/dev/null && break; kill -0 "$pid"; sleep 0.2; done
[ -s "$out/info-start.json" ] || { echo "node did not answer"; tail -n 20 "$out/node.log"; exit 2; }
curl -fsS -m 10 -o "$out/channel.json" "$api/channel/$channel" || echo "channel: no answer" >&2
curl -fsS -m 10 -o "$out/chain-id.json" "$api/chain/id" || echo "chain id: no answer" >&2
# The node's immutable (final) blocks, in slot windows, from before the channel's first inscription
# to the last irreversible block when the copy started (later blocks are the copy's own).
lib_slot=$(sed -n 's/.*"lib_slot":\([0-9]*\).*/\1/p' "$out/info-start.json")
mkdir -p "$out/blocks"; from="${SLOT_FROM:-8843000}"
while [ "$from" -le "$lib_slot" ]; do
  to=$((from + 499)); [ "$to" -gt "$lib_slot" ] && to="$lib_slot"
  curl -fsS -m 60 -o "$out/blocks/$from-$to.json" "$api/cryptarchia/blocks?slot_from=$from&slot_to=$to"
  from=$((to + 1))
done
echo "windows: $(ls "$out/blocks" | wc -l), bytes: $(cat "$out"/blocks/*.json | wc -c)"
curl -fsS -m 10 -o "$out/info-end.json" "$api/cryptarchia/info" || true
cat "$out/info-start.json"; echo; cat "$out/info-end.json"; echo
stop; trap - EXIT
rm -rf "$work"
