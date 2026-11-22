#!/usr/bin/env bash
# ferrata_attestor_module beside each sequencer of a running LEZ zone: checkpoint/attestor/zone.sh's real
# zone of four LEZ v0.4.0-rc1 sequencers on one Logos Blockchain node (upstream's devnet configuration,
# the lab bundle's binaries, no Docker), and four headless Logos Cores, one per sequencer, each with
# ferrata_attestor_module installed and loaded, its own attestation key (OS randomness) certified once by
# that sequencer's channel key. While the zone runs, each module's loop (`start`) reads the node over HTTP
# every POLL seconds, checks the final tier against its own sequencer's LEZ node (`lez`, the sequencer's
# JSON-RPC on 127.0.0.1:1930i: it signs only a tip that node holds final) and signs its tip as it advances
# (never a lower tip or another hash at a height it signed). When the run's seconds are over, while the
# sequencers still run (a stopped LEZ node answers nothing), the loops are stopped and each module's one-shot
# `attest` (node + lez + its loop's state file) runs, again until all four sign one tip T; then the sequencers
# stop, and over the node's saved answers the module's collector takes 3 of 4 (one sequencer's attestation left
# out), 4 of 4, and refuses 2 of 4, the base input ending at T.
#
#   LOGOSCTL=BIN LGX=PACKAGE modules/scripts/attestor-zone-live.sh BUNDLE OUT_DIR [SECONDS] [POLL]
#
# Run from the repository root on a Linux x86-64 machine that can run zone.sh (the lab bundle, libpcsclite);
# LGX: ferrata_attestor_module's package for that logosctl (dev or portable). Writes OUT_DIR/summary.md,
# OUT_DIR/zone/ (zone.sh's output) and OUT_DIR/work/. Exit 1 on a failed check.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
M=ferrata_attestor_module
Z="$repo/checkpoint/attestor/runs/zone"
DEV="$repo/../upstream/lez-v0.4.0-rc1/lez/configs/docker-all-in-one/devnet"
phase=main
# Logos Core keeps each module's socket in TMPDIR (logos_<module>_<id>). A Unix socket path holds at most
# 107 bytes; with a longer one every module's host process dies at load (SIGSEGV after "failed to listen",
# met on the build box). LOGOS_TMPDIR (default TMPDIR) must leave room for the longest module name.
LOGOS_TMPDIR=${LOGOS_TMPDIR:-${TMPDIR:-/tmp}}
if [ $(( ${#LOGOS_TMPDIR} + 45 )) -gt 107 ]; then
  echo "$(basename "$0"): LOGOS_TMPDIR=$LOGOS_TMPDIR is too long for Logos Core's sockets (62 bytes at most); set a shorter one" >&2
  exit 2
fi
mkdir -p "$LOGOS_TMPDIR"
export TMPDIR="$LOGOS_TMPDIR"
case "${1:-}" in during | before-stop | after) phase=$1; shift ;; esac

ctl() { local i=$1; shift; "$LOGOSCTL" --config-dir "$W/core-$i" "$@"; }
# callm I METHOD NAME: request $W/req/NAME.json (none if absent), reply $W/rep/NAME.json, through core I
callm() {
  local req=()
  [ -f "$W/req/$3.json" ] && req=("@$W/req/$3.json")
  ctl "$1" --timeout 300000 call "$M" "$2" "${req[@]}" --json < /dev/null > "$W/rep/$3.raw" 2>&1 || true
  jq -c 'if (.result | type) == "string" then (.result | fromjson? // .result) else .result end' "$W/rep/$3.raw" \
    > "$W/rep/$3.json" 2>/dev/null || echo '{"verdict":"call-failed"}' > "$W/rep/$3.json"
}
req() { cat > "$W/req/$1.json"; }

lez_of() { echo "http://127.0.0.1:$((19300 + $1))"; }   # sequencer i's JSON-RPC (zone.sh's --rpc-listen-address)

if [ "$phase" = during ]; then
  W=${LIVE_WORK:?}
  slot=$(curl -fsS -m 5 "$ZONE_API/cryptarchia/info" | jq .cryptarchia_info.slot)
  from=$(( slot > 120 ? slot - 120 : 0 ))
  echo "$from" > "$W/slot_from"
  for i in 0 1 2 3; do
    # The loop starts once its sequencer's LEZ node answers (the others wait for the channel before they serve).
    for _ in $(seq 1 120); do
      curl -fsS -m 3 -H 'content-type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"getLastBlockId","params":[]}' \
        "$(lez_of "$i")" > /dev/null 2>&1 && break
      sleep 2
    done
    jq -n --arg d "$Z/deployment.json" --arg c "$W/cert-$i.json" --arg k "$W/attestation-$i.key" --arg n "$ZONE_API" \
      --arg l "$(lez_of "$i")" --arg w "$W/loop-$i" --arg p "$W/published-$i" --arg s "$W/state-$i.json" --argjson f "$from" \
      --argjson poll "${LIVE_POLL:-15}" \
      '{deployment_file: $d, cert_file: $c, att_key_file: $k, node: $n, lez: $l, work_dir: $w, publish_dir: $p, state_file: $s,
        slot_from: $f, poll_secs: $poll}' | req "start-$i"
    callm "$i" start "start-$i"
    echo "attestor $i: $(jq -c '{verdict, error}' "$W/rep/start-$i.json")"
  done
  exit 0
fi

if [ "$phase" = before-stop ]; then
  # The sequencers still run. The loops end after the round in hand; then each module's one-shot attest (the
  # loop's state file, its sequencer's LEZ node) runs, the four in turn, again until all four sign one tip.
  W=${LIVE_WORK:?}
  for i in 0 1 2 3; do callm "$i" stop "stop-$i"; done
  for try in $(seq 1 40); do
    for i in 0 1 2 3; do
      rm -rf "$W/final-$i" "$W/final-$i.json"
      jq -n --arg d "$Z/deployment.json" --arg c "$W/cert-$i.json" --arg k "$W/attestation-$i.key" --arg n "$ZONE_API" \
        --arg l "$(lez_of "$i")" --arg v "$W/final-$i" --arg s "$W/state-$i.json" --arg o "$W/final-$i.json" \
        '{deployment_file: $d, cert_file: $c, att_key_file: $k, node: $n, lez: $l, save_dir: $v, state_file: $s, out: $o}' \
        | req "final-$i"
      callm "$i" attest "final-$i"
    done
    tips=$(for i in 0 1 2 3; do jq -r 'if .verdict == "attested" then "\(.tip_id) \(.tip_hash)" else "none" end' "$W/rep/final-$i.json"; done | sort -u)
    echo "final round $try: $(echo "$tips" | awk '{print $1}' | tr '\n' ' ')"
    if [ "$(echo "$tips" | wc -l)" = 1 ] && [ "${tips%% *}" != none ]; then echo "${tips%% *}" > "$W/final_tip"; break; fi
    sleep 5
  done
  exit 0
fi

if [ "$phase" = after ]; then
  W=${LIVE_WORK:?}
  jq -n --arg a "$ZONE_OUT/answers" '{answers_dir: $a}' | req tier
  callm 0 tier tier
  tip=$(cat "$W/final_tip" 2>/dev/null || echo none)
  echo "the attestors' common final tip $tip; the node's final tier after the sequencers stopped: $(jq -r .tip_id "$W/rep/tier.json")"
  # The base: the final blocks after the first inscribed one, to T = the tip the four signed.
  jq -n --arg a "$ZONE_OUT/answers" --arg g "$ZONE_OUT/deployment-settings.yaml" --arg o "$W/base.input" \
    --argjson f "$(( $(jq -r .first_block "$W/rep/tier.json") + 1 ))" --argjson t "${tip/none/0}" \
    '{answers_dir: $a, genesis_file: $g, n_min: 4, t_min: 3, from: $f, to: $t, out: $o}' | req base
  callm 0 base base
  jq -n --arg d "$Z/deployment.json" --arg w "$W" '{deployment_file: $d, att_files: [range(4) | "\($w)/final-\(.).json"]}' | req verify
  callm 0 verify verify
  # 3 of 4: the attestation of the sequencer that inscribed the fewest blocks is left out.
  out_i=$(python3 -I - "$W/rep/tier.json" "$W" <<'PY'
import json, sys
tier, w = json.load(open(sys.argv[1])), sys.argv[2]
counts = {k: v["count"] for k, v in tier["blocks_per_signer"].items()}
keys = [json.load(open(f"{w}/cert-{i}.json"))["channel_key"] for i in range(4)]
print(min(range(4), key=lambda i: (counts.get(keys[i], 0), -i)))
PY
)
  echo "$out_i" > "$W/left_out"
  jq -n --arg b "$W/base.input" --arg w "$W" --argjson x "$out_i" \
    '{base_file: $b, att_files: [range(4) | select(. != $x) | "\($w)/final-\(.).json"], name: "live-zone-n4-t3", out: "\($w)/live-zone-n4-t3.input"}' | req collect-3
  callm 0 collect collect-3
  jq -n --arg b "$W/base.input" --arg w "$W" '{base_file: $b, att_files: [range(4) | "\($w)/final-\(.).json"], name: "live-zone-n4-all"}' | req collect-4
  callm 0 collect collect-4
  jq -n --arg b "$W/base.input" --arg w "$W" '{base_file: $b, att_files: ["\($w)/final-0.json", "\($w)/final-1.json"]}' | req collect-2
  callm 0 collect collect-2
  exit 0
fi

# ---- main -------------------------------------------------------------------------------------------
: "${LOGOSCTL:?}" "${LGX:?}"
bundle=$(realpath "$1"); mkdir -p "$2"; out=$(cd "$2" && pwd); secs=${3:-300}
export LIVE_POLL=${4:-15}
W="$out/work"
export LIVE_WORK="$W" LOGOSCTL
rm -rf "$W" "$out/zone"
mkdir -p "$W/req" "$W/rep"
summary="$out/summary.md"
fails=0
check() { if eval "$2"; then echo "- PASS $1" >> "$summary"; else echo "- FAIL $1" >> "$summary"; fails=$((fails + 1)); fi; }
stop_cores() { for i in 0 1 2 3; do ctl "$i" daemon stop > /dev/null 2>&1 || true; done; }
trap stop_cores EXIT

# Four Logos Cores, one per sequencer; each installs the package and loads the module.
for i in 0 1 2 3; do
  mkdir -p "$W/home-$i"
  HOME="$W/home-$i" "$LOGOSCTL" --config-dir "$W/core-$i" daemon start --detach > "$W/core-$i.start.log" 2>&1
  ctl "$i" install "$LGX" -y > "$W/core-$i.install.log" 2>&1
  ctl "$i" module load "$M" > "$W/core-$i.load.log" 2>&1
  # The attestor's key (OS randomness) and its sequencer's channel key's one certificate of it, for the
  # deployment of the zone's configuration (upstream's genesis, as in runs/zone: checked below).
  jq -n --arg o "$W/attestation-$i.key" '{out: $o}' | req "keygen-$i"
  callm "$i" keygen "keygen-$i"
  jq -n --arg k "$DEV/seq-$i/channel_signing_key" --arg p "$(jq -r .public_key "$W/rep/keygen-$i.json")" --arg d "$Z/deployment.json" \
    --arg o "$W/cert-$i.json" '{channel_key_file: $k, att_pub: $p, deployment_file: $d, out: $o}' | req "certify-$i"
  callm "$i" certify "certify-$i"
done

t0=$(date +%s)
rc=0
(cd "$repo/checkpoint" && ZONE_DURING="$here/attestor-zone-live.sh during" ZONE_BEFORE_STOP="$here/attestor-zone-live.sh before-stop" \
  ZONE_AFTER="$here/attestor-zone-live.sh after" bash attestor/zone.sh "$bundle" "$out/zone" "$secs") > "$out/zone.out" 2>&1 || rc=$?
seconds=$(( $(date +%s) - t0 ))

{
  echo "# ferrata_attestor_module beside each sequencer of a running zone of four ($(date -u +%FT%TZ))"
  echo
  echo "$("$LOGOSCTL" --version 2>&1 | head -n 1); $secs s of sequencing, polls every $LIVE_POLL s; zone.sh and the hooks took $seconds s (exit $rc)."
  echo
} > "$summary"
tip=$(cat "$W/final_tip" 2>/dev/null || echo none)
check "the zone ran and its genesis is the one runs/zone's deployment was made for (deployment-settings sha256)" \
  '[ "$rc" = 0 ] && [ "$(cat "$out/zone/deployment-settings.sha256")" = "$(cat "$Z/deployment-settings.sha256")" ]'
check "four Logos Cores loaded the module; each attestation key certified by its sequencer's channel key" \
  '[ "$(for i in 0 1 2 3; do jq -r .verdict "$W/rep/certify-$i.json"; done | sort -u)" = certified ]'
check "the four loops started against the node's HTTP API and their sequencers' LEZ nodes" \
  '[ "$(for i in 0 1 2 3; do jq -r .verdict "$W/rep/start-$i.json"; done | sort -u)" = started ]'
check "the node's final tier: four keys, every sequencer inscribed (its tip after the sequencers stopped: $(jq -r .tip_id "$W/rep/tier.json" 2>/dev/null))" \
  '[ "$(jq -c "[(.keys | length), (.blocks_per_signer | length)]" "$W/rep/tier.json")" = "[4,4]" ]'
for i in 0 1 2 3; do
  s="$W/rep/stop-$i.json"
  tips=$(ls "$W/published-$i" | sed -n 's/^att-\([0-9]*\)\.json$/\1/p' | sort -n | tr '\n' ' ')
  check "attestor $i: $(jq -r .rounds "$s") rounds while the zone ran; signed $(jq -r .attested "$s") rising tips ($tips); refused $(jq -r .refused "$s") rounds (no final block yet), errors $(jq -r .errors "$s"); then its one-shot attest signed the common tip $tip" \
    '[ "$(jq -r .attested "$s")" -ge 2 ] && [ "$(jq -r .errors "$s")" = 0 ] && [ "$(jq -r .running "$s")" = false ] && [ "$(jq -r "[.verdict, .tip_id] | @tsv" "$W/rep/final-$i.json")" = "$(printf "attested\t%s" "$tip")" ]'
done
check "the base over the zone's final blocks has the deployment the attestors certified" \
  '[ "$(jq -S -c .deployment "$W/rep/base.json")" = "$(jq -S -c . "$Z/deployment.json")" ]'
check "verify: the four published attestations verify" '[ "$(jq -c "[.verdict, .refused]" "$W/rep/verify.json")" = "[\"verified\",0]" ]'
check "collect 3 of 4 (attestor $(cat "$W/left_out" 2>/dev/null) left out): accepted by the checkpoint program's rule natively, n = 4, t = 3" \
  '[ "$(jq -c "[.verdict, .n, .t]" "$W/rep/collect-3.json")" = "[\"collected\",4,3]" ] && jq -r .native "$W/rep/collect-3.json" | grep -q "^accepted"'
check "collect 4 of 4: accepted" '[ "$(jq -r .verdict "$W/rep/collect-4.json")" = collected ]'
check "collect 2 of 4: refused" '[ "$(jq -r .verdict "$W/rep/collect-2.json")" = refused ]'
{
  echo
  echo "Collected input (3 of 4): $(jq -r '.input_bytes' "$W/rep/collect-3.json" 2>/dev/null) bytes, sha256 $(jq -r '.input_sha256' "$W/rep/collect-3.json" 2>/dev/null); journal sha256 $(jq -r '.journal_sha256' "$W/rep/collect-3.json" 2>/dev/null); $(jq -r '.native' "$W/rep/collect-3.json" 2>/dev/null)."
  echo
  echo "Checks failed: $fails"
} >> "$summary"
cat "$summary"
[ "$fails" -eq 0 ]
