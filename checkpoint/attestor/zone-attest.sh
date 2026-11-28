#!/usr/bin/env bash
# After attestor/zone.sh: each of the four sequencers' attestors reads the zone node's final tier (the saved
# answers) and signs; the collector takes 3 of 4 (t(4) = 3; sequencer 3 silent) and 4 of 4; one input with 2
# of 4 must be refused. The channel keys are upstream's devnet keys (lez/configs/docker-all-in-one/devnet/seq-i),
# the ones the zone's genesis staked and the channel accredits. Run from checkpoint/ on the build box:
#
#   attestor/zone-attest.sh <zone dir> [<out dir>]   (the zone dir holds answers/ and deployment-settings.yaml)
#   ZONE_LEZ=../../upstream/lez-v0.4.0 attestor/zone-attest.sh <zone dir> [<out dir>]   (a zone.sh run of LEZ v0.4.0)
#
# The records (tier.json, deployment.json, cert-<i>.json, att-<i>.json, the collector's metas and the 3-of-4 input)
# go to <out dir>, by default the zone dir (a fresh zone run). A replay of a recorded zone gives another directory:
# the recorded attestations of runs/zone were written on 11 Nov (cefe24e3), before the attestor checked a LEZ node, and a
# replay writes the same 264 signed bytes (`encoded`) with today's `node` facts (first_block, blocks_checked, fork_digest).
set -euo pipefail
z="$(realpath "$1")"; o="$(realpath -m "${2:-$1}")"; mkdir -p "$o"; bin=target/release/ferrata_attestor
# ZONE_LEZ: the LEZ tree whose devnet keys the zone ran (as attestor/zone.sh; default rc1's, the same as rc3's; v0.4.0's
# devnet keys are regenerated: a zone of ZONE_LEZ=../../upstream/lez-v0.4.0 needs the same here)
lez="$(realpath "${ZONE_LEZ:-../../upstream/lez-v0.4.0-rc1}")"; dev="$lez/lez/configs/docker-all-in-one/devnet"
. "$(dirname "${BASH_SOURCE[0]}")/zone-lib.sh"
TAG="$(zl_tag_of_tree "$lez")" && zl_known_tag "$TAG" || { echo "zone-attest.sh: $lez is not a LEZ tree of a known tag"; exit 2; }
# The collector's inputs and metas go to fixtures/ (the committed rc1 ones); a v0.4.0 zone's go to <out dir>/fixtures/,
# never over the committed files
if zl_is_v040 "$TAG"; then FIX="$o/fixtures"; else FIX=fixtures; fi
work="${TMPDIR:?}/attestor-zone-attest"; rm -rf "$work"; mkdir -p "$work" "$FIX"

echo "== the node's final tier"
$bin tier --answers "$z/answers" --out "$o/tier.json"
tip=$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["tip_id"])' "$o/tier.json")
# The span starts after the first inscribed block (the zone's block 1 is not inscribed; its parent must be).
from=$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["first_block"] + 1)' "$o/tier.json")
python3 -I -c 'import json,sys; t=json.load(open(sys.argv[1])); print("tip", t["tip_id"], "keys", len(t["keys"]), "checked", t["blocks_checked"]); [print(" ", k[:16], v) for k, v in t["blocks_per_signer"].items()]' "$o/tier.json"

echo "== the base: final blocks $from..=$tip, parent block $((from - 1)), nMin 4, tMin 3"
$bin base --answers "$z/answers" --genesis "$z/deployment-settings.yaml" --from "$from" --to "$tip" --n-min 4 --t-min 3 \
  --out "$work/base.input"
$bin deployment --input "$work/base.input" --out "$o/deployment.json"

echo "== each sequencer's channel key certifies its attestor's key; the attestors sign"
# The zone run recorded no LEZ node's answers; its blocks (all applied there) stand in as final.
$bin devnet-lez-standin --answers "$z/answers" --out "$work"
for i in 0 1 2 3; do
  a=$($bin keygen --seed-label "zone-attestation-$i" --out "$work/attestation-$i.key")
  $bin certify --channel-key "$dev/seq-$i/channel_signing_key" --att-pub "$a" --deployment "$o/deployment.json" \
    --out "$o/cert-$i.json"
  $bin attest --deployment "$o/deployment.json" --cert "$o/cert-$i.json" --att-key "$work/attestation-$i.key" \
    --answers "$z/answers" --lez-answers "$work" --state "$work/state-$i.json" --out "$o/att-$i.json"
done
$bin verify --deployment "$o/deployment.json" --att "$o/att-0.json" --att "$o/att-1.json" --att "$o/att-2.json" --att "$o/att-3.json"

echo "== the collector: 3 of 4 (sequencer 3 silent), 4 of 4; 2 of 4 refused"
$bin collect --base "$work/base.input" --att "$o/att-0.json" --att "$o/att-1.json" --att "$o/att-2.json" \
  --out "$FIX"/rbz-zone-n4-t3.input --meta "$FIX"/rbz-zone-n4-t3.json --name rbz-zone-n4-t3 \
  --description "rule (B) over a real four-sequencer LEZ $TAG zone (upstream's devnet keys, round-robin turns): final blocks $from..=$tip, n = 4, t = 3; attestations of sequencers 0, 1, 2 (sequencer 3 silent)"
$bin collect --base "$work/base.input" --att "$o/att-0.json" --att "$o/att-1.json" --att "$o/att-2.json" --att "$o/att-3.json" \
  --out "$FIX"/rbz-zone-n4-all.input --meta "$FIX"/rbz-zone-n4-all.json --name rbz-zone-n4-all \
  --description "as rbz-zone-n4-t3 with all four attestations"
$bin assemble --base "$work/base.input" --att "$o/att-0.json" --att "$o/att-1.json" \
  --out "$FIX"/rbz-refuse-two-of-four.input --meta "$FIX"/rbz-refuse-two-of-four.json --name rbz-refuse-two-of-four \
  --expect InsufficientAttestations --description "must be refused: two attestations where n = 4 needs three"
cp "$FIX"/rbz-zone-n4-t3.json "$FIX"/rbz-zone-n4-all.json "$FIX"/rbz-refuse-two-of-four.json "$FIX"/rbz-zone-n4-t3.input "$o/"
echo "zone attestation done"
