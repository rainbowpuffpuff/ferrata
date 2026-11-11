#!/usr/bin/env bash
# The node's own answers for the crown run's checkpoint (stage 7, e2e/crown/README.md), from a RUNNING
# Logos Blockchain node of the run's devnet (at the end of LEZ session B, or a copy of its saved
# generation started alone as extract.sh does): /cryptarchia/info (lib_slot: the node's finality),
# /channel/<id>, /chain/id and the immutable blocks (/cryptarchia/blocks serves only those) in
# 500-slot windows from SLOT_FROM (default 0) to the LIB slot. realblocks-crown reads <out>.
#   crown-extract.sh <api url, e.g. http://127.0.0.1:8080> <out dir>
set -euo pipefail
api="${1:?api url}"; out="$(realpath -m "${2:?out dir}")"
channel=0101010101010101010101010101010101010101010101010101010101010101
mkdir -p "$out/blocks"
curl -fsS -m 10 -o "$out/info.json" "$api/cryptarchia/info"
curl -fsS -m 10 -o "$out/channel.json" "$api/channel/$channel"
curl -fsS -m 10 -o "$out/chain-id.json" "$api/chain/id" || echo "chain id: no answer" >&2
lib_slot=$(sed -n 's/.*"lib_slot":\([0-9]*\).*/\1/p' "$out/info.json")
[ -n "$lib_slot" ] || { echo "info.json has no lib_slot" >&2; exit 2; }
from="${SLOT_FROM:-0}"
while [ "$from" -le "$lib_slot" ]; do
  to=$((from + 499)); [ "$to" -gt "$lib_slot" ] && to="$lib_slot"
  curl -fsS -m 60 -o "$out/blocks/$from-$to.json" "$api/cryptarchia/blocks?slot_from=$from&slot_to=$to"
  from=$((to + 1))
done
echo "lib_slot $lib_slot, windows $(ls "$out/blocks" | wc -l), bytes $(cat "$out"/blocks/*.json | wc -c)"
