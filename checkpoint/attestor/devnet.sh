#!/usr/bin/env bash
# B1's rule (B) run with several attestors over one sequencer's real chain (run from checkpoint/, after
# `cargo build --release -p ferrata-attestor`):
#
#   attestor/devnet.sh <node answers dir> <out dir>
#
# <node answers dir>: what the lab's Logos Blockchain node answered (/channel/<id>, /cryptarchia/info,
# /cryptarchia/blocks), as realblocks/extract.sh saves it from a copy of a lab generation.
#
# The lab's LEZ v0.4.0-rc1 chain has one sequencer (channel key 3b14…b0a2, n = 1). LEZ v0.4.0-rc1 can run a
# committee (its integration_tests/tests/multi_sequencer.rs), but its fixtures start Bedrock under Docker
# and stake the sequencers at genesis with LEZ's own signing code; neither is available on the build box.
# So the key set is a STAND-IN: K = {the lab's key (the real producer of T), k1, k2}, where k1 and k2 are
# seeded throwaway channel keys of two sequencers that do not run, and the node's channel answer is given
# the same three keys. Everything else is real: the blocks 186..=202 and T's inscription as the node holds
# them, the lab key's certificate (realblocks/out/attestor.json), and the attestors read the node's whole
# final tier (blocks 1..=202) before they sign. nMin = 3, tMin = 2, so t(3) = 2.
#
# Writes <out dir>: deployment.json, cert-{0,1,2}.json, att-{0,1,2}.json, the collected inputs and their
# metas, and the refusal metas; the inputs also go to fixtures/ for `host bench` and `host prove`.
set -euo pipefail
answers="$(realpath "$1")"; out="$(realpath -m "$2")"
bin=target/release/ferrata_attestor; refusals=target/release/attestor-refusals
work="${TMPDIR:?}/attestor-devnet"; rm -rf "$work"; mkdir -p "$work" "$out" fixtures
channel=0101010101010101010101010101010101010101010101010101010101010101
lab=3b14e3980d05dd3651ff5956758dd4cfecc33b837775c9e3d90279d34d3fb0a2

echo "== 1. the base: the lab's real blocks 186..=202, T = 202 with its inscription from the node"
$bin upgrade --old realblocks/out/real-lab-186-202.input --answers "$answers" --channel $channel --tip 202 \
  --out "$work/base-n1.input"

echo "== 2. the stand-in key set {lab, k1, k2}, nMin 3, tMin 2"
k1=$($bin keygen --seed-label channel-1 --out "$work/channel-1.key")
k2=$($bin keygen --seed-label channel-2 --out "$work/channel-2.key")
$bin rekey --base "$work/base-n1.input" --keys "$lab,$k1,$k2" --n-min 3 --t-min 2 --out "$work/base-n3.input"
$bin deployment --input "$work/base-n3.input" --out "$out/deployment.json"
cp -r "$answers" "$work/answers-n3"
python3 -I - "$work/answers-n3/channel.json" "$lab" "$k1" "$k2" <<'EOF'
import json, sys
path, *keys = sys.argv[1:]
c = json.load(open(path))
c["accredited_keys"] = keys
c["configuration_threshold"] = 2
json.dump(c, open(path, "w"))
EOF

echo "== 3. each sequencer's channel key certifies its attestor's key, once"
a1=$($bin keygen --seed-label attestation-1 --out "$work/attestation-1.key")
a2=$($bin keygen --seed-label attestation-2 --out "$work/attestation-2.key")
$bin certify --channel-key "$work/channel-1.key" --att-pub "$a1" --deployment "$out/deployment.json" --out "$out/cert-1.json"
$bin certify --channel-key "$work/channel-2.key" --att-pub "$a2" --deployment "$out/deployment.json" --out "$out/cert-2.json"
# The lab sequencer's attestor: the certificate the lab's channel key gave in the real-blocks run, for this
# chain, vault, configuration and channel (lezConfigId does not cover nMin and tMin); its key from a label.
python3 -I - realblocks/out/attestor.json "$out/deployment.json" "$out/cert-0.json" "$work/attestation-0.key" <<'EOF'
import hashlib, json, sys
src, dep, dst, key = sys.argv[1:]
a, d = json.load(open(src)), json.load(open(dep))
assert d["lez_config_id"] == a["deployment"]["lez_config_id"] and d["vault"] == a["deployment"]["vault"]
open(key, "wb").write(hashlib.sha256(b"ferrata/realblocks/attestation-key/v1").digest())
json.dump({"channel_key": a["channel_key"], "attestation_key": a["attestation_key"], "deployment": d,
           "certificate_message": a["certificate_message"], "certificate": a["certificate"],
           "source": "realblocks/out/attestor.json (the lab channel key's certificate)"}, open(dst, "w"), indent=2)
EOF

echo "== 4. three attestors read their node's final tier and sign"
for i in 0 1 2; do
  $bin attest --deployment "$out/deployment.json" --cert "$out/cert-$i.json" --att-key "$work/attestation-$i.key" \
    --answers "$work/answers-n3" --state "$work/state-$i.json" --out "$out/att-$i.json"
done
echo "-- again, with each attestor's state of the first round: the same tip, signed again"
$bin attest --deployment "$out/deployment.json" --cert "$out/cert-1.json" --att-key "$work/attestation-1.key" \
  --answers "$work/answers-n3" --state "$work/state-1.json" --out "$work/att-1-again.json"
echo "-- the lab node's own answer (n = 1): attestor 1's channel key is not in that set, it refuses"
if $bin attest --deployment "$out/deployment.json" --cert "$out/cert-1.json" --att-key "$work/attestation-1.key" \
    --answers "$answers" --out "$work/att-1-n1.json" 2> "$work/refusal.txt"; then
  echo "UNEXPECTED: signed"; exit 1
else
  tail -n 1 "$work/refusal.txt"
fi

echo "== 5. anyone checks the published attestations"
$bin verify --deployment "$out/deployment.json" --att "$out/att-0.json" --att "$out/att-1.json" --att "$out/att-2.json"

echo "== 6. the collector: sequencer 2 silent (t = 2 of n = 3), and all three"
$bin collect --base "$work/base-n3.input" --att "$out/att-0.json" --att "$out/att-1.json" \
  --out fixtures/rbm-lab-n3-t2.input --meta fixtures/rbm-lab-n3-t2.json --name rbm-lab-n3-t2 \
  --description "rule (B) over the lab's real LEZ blocks 186..=202, T = 202, stand-in key set of three (the lab's key and two seeded), nMin 3, tMin 2; attestations of the lab's attestor and attestor 1 (sequencer 2 silent)"
$bin collect --base "$work/base-n3.input" --att "$out/att-0.json" --att "$out/att-1.json" --att "$out/att-2.json" \
  --out fixtures/rbm-lab-n3-all.input --meta fixtures/rbm-lab-n3-all.json --name rbm-lab-n3-all \
  --description "as rbm-lab-n3-t2 with all three attestations"
echo "-- below t: one attestation"
if $bin collect --base "$work/base-n3.input" --att "$out/att-1.json" --out "$work/x.input" --meta "$work/x.json" \
    --name x 2> "$work/refusal.txt"; then echo "UNEXPECTED: collected"; exit 1; else tail -n 1 "$work/refusal.txt"; fi

echo "== 7. inputs the program must refuse"
$refusals --base "$work/base-n3.input" --answers "$work/answers-n3" --out-dir fixtures
cp fixtures/rbm-lab-n3-t2.json fixtures/rbm-lab-n3-all.json fixtures/rbm-refuse-*.json fixtures/rbm-lab-n3-t2.input "$out/"
echo "devnet run done: $out"
