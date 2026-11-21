#!/usr/bin/env bash
# ferrata_attestor against a running Logos Blockchain node: a COPY of the lab's saved generation (never
# the lab), started alone as realblocks/extract.sh starts it (no sequencer). The attestors ask the node
# over HTTP (/cryptarchia/info, /channel/<id>, /cryptarchia/blocks), save what it answered, check the final
# tier and sign. Run from checkpoint/ on the build box, after attestor/devnet.sh:
#
#   attestor/live-node.sh <bundle.tar.zst> <gen-N.tar.zst> <out dir>
#
# The node's channel holds one key (the lab sequencer's), so the lab's attestor signs (n = 1) and the
# stand-in sequencers' attestors refuse: their channel keys are not in the live set the node reports.
set -euo pipefail
bundle="$(realpath "$1")"; gen="$(realpath "$2")"; out="$(realpath -m "$3")"
bin="$PWD/target/release/ferrata_attestor"; runs="$PWD/attestor/runs/devnet-n3"
work="${TMPDIR:?}/attestor-live"; rm -rf "$work"; mkdir -p "$work/bundle" "$work/state" "$out"
zstd -q -d -c "$bundle" | tar -C "$work/bundle" -xf -
( cd "$work/bundle" && sha256sum -c --quiet SHA256SUMS )
zstd -q -d -c "$gen" | tar -C "$work/state" -xf -
node="$work/bundle/node/logos-blockchain-node"; chmod +x "$node"
export XDG_CACHE_HOME="$work/bundle/xdg-cache"
api="http://127.0.0.1:18080"
( cd "$work/state/bedrock" && POL_PROOF_DEV_MODE=true exec "$node" node-config.yaml --deployment deployment-settings.yaml ) \
  > "$work/node.log" 2>&1 &
pid=$!
stop() { kill -TERM "$pid" 2>/dev/null || true; for i in $(seq 1 300); do kill -0 "$pid" 2>/dev/null || return 0; sleep 0.2; done; kill -KILL "$pid" 2>/dev/null || true; }
trap stop EXIT
for i in $(seq 1 600); do curl -fsS -m 3 -o /dev/null "$api/cryptarchia/info" 2>/dev/null && break; kill -0 "$pid"; sleep 0.2; done
echo "node up: $(curl -fsS -m 3 "$api/cryptarchia/info")"

echo "== the lab sequencer's attestor asks its node and signs"
python3 -I -c 'import hashlib,sys; open(sys.argv[1],"wb").write(hashlib.sha256(b"ferrata/realblocks/attestation-key/v1").digest())' "$work/attestation-0.key"
# Since the LEZ check (review H-2) a live attestor also asks its LEZ node: LEZ names one that follows this
# node's channel (the lab's sequencer); the run recorded in runs/live-node predates the check.
lez="${LEZ:?LEZ: the RPC of a LEZ node that follows this channel}"
$bin attest --node "$api" --lez "$lez" --save "$work/answers-0" --slot-from 8843000 --deployment "$runs/deployment.json" \
  --cert "$runs/cert-0.json" --att-key "$work/attestation-0.key" \
  --state "$work/state-0.json" --out "$out/live-att-0.json"
$bin verify --deployment "$runs/deployment.json" --att "$out/live-att-0.json"
echo "== a stand-in sequencer's attestor asks the same node: its channel key is not in the live set"
python3 -I -c 'import hashlib,sys; open(sys.argv[1],"wb").write(hashlib.sha256(b"ferrata/attestor/devnet-key/attestation-1").digest())' "$work/attestation-1.key"
if $bin attest --node "$api" --lez "$lez" --save "$work/answers-1" --slot-from 8843000 --deployment "$runs/deployment.json" \
    --cert "$runs/cert-1.json" --att-key "$work/attestation-1.key" --state "$work/state-1.json" \
    --out "$work/live-att-1.json" 2> "$work/refusal.txt"; then
  echo "UNEXPECTED: signed"; exit 1
else
  tail -n 1 "$work/refusal.txt"
fi
cp "$work/answers-0/channel.json" "$out/live-channel.json"; cp "$work/answers-0/info.json" "$out/live-info.json"
echo "windows saved: $(ls "$work/answers-0/blocks" | wc -l), bytes: $(cat "$work"/answers-0/blocks/*.json | wc -c)"
stop; trap - EXIT
rm -rf "$work"
