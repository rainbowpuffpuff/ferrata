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
# The lab's LEZ node is not recorded; its sequencer served every block Finalized (measured): stood in.
$bin devnet-lez-standin --answers "$work/answers-n3"

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
    --answers "$answers" --lez-answers "$work/answers-n3" --out "$work/att-1-n1.json" 2> "$work/refusal.txt"; then
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

echo "-- T = 200: the attestations (of 202) are of a descendant of T, along the path 201, 202"
$bin retip --base "$work/base-n3.input" --answers "$work/answers-n3" --tip 200 --out "$work/base-n3-t200.input"
$bin collect --base "$work/base-n3-t200.input" --att "$out/att-0.json" --att "$out/att-1.json" \
  --out fixtures/rbm-lab-n3-t200.input --meta fixtures/rbm-lab-n3-t200.json --name rbm-lab-n3-t200 \
  --description "as rbm-lab-n3-t2 with T = 200; the two attestations are of block 202, a descendant of T"
echo "-- an attestor whose node had finalised only up to 201 signs 201: it cannot support T = 202"
cp -r "$work/answers-n3" "$work/answers-behind"
python3 -I - "$work/answers-behind/info.json" "$work/answers-behind/info-start.json" <<'PY'
import json, os, sys
for p in sys.argv[1:]:
    if os.path.exists(p):
        i = json.load(open(p)); i["cryptarchia_info"]["lib_slot"] = 8856116; json.dump(i, open(p, "w"))
PY
$bin attest --deployment "$out/deployment.json" --cert "$out/cert-2.json" --att-key "$work/attestation-2.key" \
  --answers "$work/answers-behind" --out "$out/att-2-behind.json"
$bin assemble --base "$work/base-n3.input" --att "$out/att-0.json" --att "$out/att-2-behind.json" \
  --out fixtures/rbm-refuse-behind.input --meta fixtures/rbm-refuse-behind.json --name rbm-refuse-behind \
  --expect NotADescendant --description "must be refused: an attestation of 201 for T = 202 (its node was behind)"

echo "== 6b. a key rotation {lab, k1, k2} -> {lab, k1, k3} in the proof, signed by k1 and k2 (2 of 3)"
echo "   (stand-in posting timeframe 20: a rotation needs a non-zero turn, SPEC-v3 D3; the lab's channel has 0)"
k3=$($bin keygen --seed-label channel-3 --out "$work/channel-3.key")
$bin rekey --base "$work/base-n1.input" --keys "$lab,$k1,$k2" --n-min 3 --t-min 2 --posting-timeframe 20 \
  --out "$work/base-n3-p20.input"
$bin rotate --base "$work/base-n3-p20.input" --keys "$lab,$k1,$k3" --threshold 2 \
  --signer "1:$work/channel-1.key" --signer "2:$work/channel-2.key" --out "$work/base-rot.input"
$bin rotate --base "$work/base-n3-p20.input" --keys "$lab,$k1,$k3" --threshold 2 \
  --signer "1:$work/channel-1.key" --out "$work/base-rot-weak.input"
cp -r "$work/answers-n3" "$work/answers-rot"
python3 -I - "$work/answers-rot/channel.json" "$lab" "$k1" "$k3" <<'PY'
import json, sys
path, *keys = sys.argv[1:]
c = json.load(open(path)); c["accredited_keys"] = keys; c["posting_timeframe"] = 20; json.dump(c, open(path, "w"))
PY
for i in 0 1; do
  $bin attest --deployment "$out/deployment.json" --cert "$out/cert-$i.json" --att-key "$work/attestation-$i.key" \
    --answers "$work/answers-rot" --out "$out/att-rot-$i.json"
done
echo "-- sequencer 2, removed by the rotation, asks its node: it refuses"
if $bin attest --deployment "$out/deployment.json" --cert "$out/cert-2.json" --att-key "$work/attestation-2.key" \
    --answers "$work/answers-rot" --out "$work/x.json" 2> "$work/refusal.txt"; then echo "UNEXPECTED: signed"; exit 1
else tail -n 1 "$work/refusal.txt"; fi
$bin collect --base "$work/base-rot.input" --att "$out/att-rot-0.json" --att "$out/att-rot-1.json" \
  --out fixtures/rbm-lab-rot.input --meta fixtures/rbm-lab-rot.json --name rbm-lab-rot \
  --description "the lab's real blocks 186..=202 with a rotation {lab, k1, k2} -> {lab, k1, k3} signed by 2 of 3 (stand-in keys and posting timeframe 20); attestations of the lab's attestor and attestor 1 under the new set"
$bin assemble --base "$work/base-rot.input" --att "$out/att-rot-0.json" --att "$out/att-2.json" \
  --out fixtures/rbm-refuse-removed-key.input --meta fixtures/rbm-refuse-removed-key.json --name rbm-refuse-removed-key \
  --expect NotAccredited --description "must be refused: the attestation of k2, a key the rotation in the same proof removed"
$bin assemble --base "$work/base-rot-weak.input" --att "$out/att-rot-0.json" --att "$out/att-rot-1.json" \
  --out fixtures/rbm-refuse-rotation-one-signer.input --meta fixtures/rbm-refuse-rotation-one-signer.json \
  --name rbm-refuse-rotation-one-signer --expect ConfigThresholdUnmet --description "must be refused: the rotation signed by one key of three"
$bin assemble --base "$work/base-rot.input" --att "$out/att-0.json" --att "$out/att-1.json" \
  --out fixtures/rbm-refuse-old-set.input --meta fixtures/rbm-refuse-old-set.json --name rbm-refuse-old-set \
  --expect BadAttestation --description "must be refused: attestations over the key set before the rotation"

echo "== 7. inputs the program must refuse"
$refusals --base "$work/base-n3.input" --answers "$work/answers-n3" --out-dir fixtures
cp fixtures/rbm-lab-n3-t2.json fixtures/rbm-lab-n3-all.json fixtures/rbm-lab-n3-t200.json fixtures/rbm-lab-rot.json fixtures/rbm-refuse-*.json fixtures/rbm-lab-n3-t2.input "$out/"
echo "devnet run done: $out"
