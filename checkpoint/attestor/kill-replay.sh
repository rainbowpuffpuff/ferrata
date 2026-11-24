#!/usr/bin/env bash
# Each of the four attestors of the recorded four-sequencer zone (runs/zone, as zone-attest.sh replays it) SIGKILLed at
# random moments of its rounds and started again (RFP-022 R.6): KILLS rounds per attestor are killed after a random
# 5 to 400 ms (`timeout -s KILL`), then one round runs to its end. Checked: the state file is whole after every kill
# (it is replaced atomically, before the attestation is written); every attestation an attestor wrote, killed or not,
# carries the same signed bytes (`encoded`): one signature per key and tip, also when a kill fell between the state's
# write and the attestation's; those bytes equal the zone run's own records; the collector's 3 of 4 is the zone run's
# proven input byte for byte, and, when given, checkpoint_b's executor accepts it (the checkpoint completes).
# Run from checkpoint/ on the build box after `cargo build --release -p ferrata-attestor`:
#
#   attestor/kill-replay.sh <out dir> [KILLS] [<ferrata-e2e> <checkpoint_b.bin>]
set -euo pipefail
o="$(realpath -m "$1")"; kills="${2:-12}"; e2e="${3:-}"; prog="${4:-}"
z="$(realpath attestor/runs/zone)"; bin="$(realpath target/release/ferrata_attestor)"
dev="$(realpath ../../upstream/lez-v0.4.0-rc1/lez/configs/docker-all-in-one/devnet)"
work="${TMPDIR:?}/attestor-kill-replay"; rm -rf "$work" "$o"; mkdir -p "$work" "$o"
$bin tier --answers "$z/answers" --out "$work/tier.json" > /dev/null
tip=$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["tip_id"])' "$work/tier.json")
from=$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["first_block"] + 1)' "$work/tier.json")
$bin base --answers "$z/answers" --genesis "$z/deployment-settings.yaml" --from "$from" --to "$tip" --n-min 4 --t-min 3 --out "$work/base.input"
$bin deployment --input "$work/base.input" --out "$work/deployment.json"
$bin devnet-lez-standin --answers "$z/answers" --out "$work" > /dev/null
: > "$o/rounds.tsv"
for i in 0 1 2 3; do
  a=$($bin keygen --seed-label "zone-attestation-$i" --out "$work/attestation-$i.key")
  $bin certify --channel-key "$dev/seq-$i/channel_signing_key" --att-pub "$a" --deployment "$work/deployment.json" --out "$work/cert-$i.json" > /dev/null
  rm -f "$work/state-$i.json"
  for k in $(seq 1 "$kills"); do
    ms=$(( 5 + RANDOM % 396 )); out="$work/att-$i-k$k.json"; before="$(sha256sum "$work/state-$i.json" 2>/dev/null | cut -c1-16)"
    rc=0; timeout -s KILL "0.$(printf '%03d' "$ms")" "$bin" attest --deployment "$work/deployment.json" --cert "$work/cert-$i.json" \
      --att-key "$work/attestation-$i.key" --answers "$z/answers" --lez-answers "$work" --state "$work/state-$i.json" --out "$out" \
      > /dev/null 2>> "$work/attest-$i.log" || rc=$?
    after="$(sha256sum "$work/state-$i.json" 2>/dev/null | cut -c1-16)"
    whole=yes; [ -e "$work/state-$i.json" ] && { python3 -I -c 'import json,sys; json.load(open(sys.argv[1]))' "$work/state-$i.json" 2>/dev/null || whole=no; }
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$i" "$k" "$ms" "$rc" "${before:--}" "${after:--}" "$([ -e "$out" ] && echo written || echo none)	$whole" >> "$o/rounds.tsv"
  done
  t0=$(date +%s%N)
  $bin attest --deployment "$work/deployment.json" --cert "$work/cert-$i.json" --att-key "$work/attestation-$i.key" \
    --answers "$z/answers" --lez-answers "$work" --state "$work/state-$i.json" --out "$o/att-$i.json" > /dev/null
  echo -e "$i\tfinal\t$(( ($(date +%s%N) - t0) / 1000000 ))\t0\t-\t-\twritten\tyes" >> "$o/rounds.tsv"
  cp "$work/state-$i.json" "$o/state-$i.json"
done
$bin collect --base "$work/base.input" --att "$o/att-0.json" --att "$o/att-1.json" --att "$o/att-2.json" \
  --out "$work/n4-t3.input" --meta "$o/n4-t3.json" --name rbz-zone-n4-t3 > /dev/null
python3 -I - "$o" "$work" "$z" "$kills" <<'PY' | tee "$o/verdict.json"
import glob, json, sys, filecmp
o, work, z, kills = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])
rows = [l.split("\t") for l in open(o + "/rounds.tsv").read().splitlines()]
per = {}
for i in "0123":
    encs = {json.load(open(f))["encoded"] for f in glob.glob(f"{work}/att-{i}-k*.json") + [f"{o}/att-{i}.json"]}
    per[i] = {"distinctSignedBytes": len(encs), "equalsZoneRecord": encs == {json.load(open(f"{z}/att-{i}.json"))["encoded"]},
              "killed": sum(r[0] == i and r[1] != "final" and r[3] == "137" for r in rows),
              "killedAfterStateWrite": sum(r[0] == i and r[1] != "final" and r[3] == "137" and r[4] != r[5] for r in rows),
              "killedWithAttestationWritten": sum(r[0] == i and r[1] != "final" and r[3] == "137" and r[6] == "written" for r in rows)}
torn = sum(r[7] == "no" for r in rows)
same_input = filecmp.cmp(f"{work}/n4-t3.input", f"{z}/rbz-zone-n4-t3.input", shallow=False)
ok = torn == 0 and same_input and all(v["distinctSignedBytes"] == 1 and v["equalsZoneRecord"] for v in per.values())
print(json.dumps({"ok": ok, "killsPerAttestor": kills, "attestors": per, "tornStates": torn, "collected3of4EqualsProvenInput": same_input}))
PY
if [ -n "$e2e" ]; then
  "$e2e" exec --program "$prog" --input "$work/n4-t3.input" --meta "$o/n4-t3.json" | tail -n 1 | tee "$o/exec.json"
fi
python3 -I -c 'import json,sys; sys.exit(0 if json.load(open(sys.argv[1]))["ok"] else 1)' "$o/verdict.json"
