#!/usr/bin/env bash
# ferrata_attestor_module doing the attestor's job, on the four-sequencer zone the attestor's own run used
# (checkpoint/attestor/runs/zone: the node's saved answers of a real LEZ v0.4.0-rc1 zone, the devnet's
# channel keys): what attestor/zone-attest.sh does with the binary, through the module.
#
#   tier, base           the Bedrock node's final tier (tip 30, four keys) and the base input over its final blocks
#   keygen, certify,     each sequencer's attestor: its key, its channel key's certificate, its signature of
#   attest               the final tip, checked against a LEZ node (the zone saved none: its inscribed blocks
#                        stand in as the LEZ node's final blocks, recorded_node.py --save-lez); a first read
#                        starts at the LEZ node's highest final block (30); the signed bytes equal the run's
#   verify, collect      the four verify; 3 of 4 collected = the input the checkpoint program proved
#                        (byte for byte); 4 of 4 = that run's journal; 2 of 4 refused; the refusal input
#   refusals             a signed tip its LEZ node does not hold, another history at the signed height, a
#                        certificate of another key, another deployment
#   start/status/stop    the attestor's loop in the module, against a node served from the same answers
#                        over HTTP, with the LEZ stand-in's getBlock on the same port (recorded_node.py): it
#                        reads the node and asks its LEZ node each round, signs once, publishes
#
#   modules/scripts/attestor-e2e.sh OUT_DIR                  the methods called natively (call example)
#   CALL=logosctl LOGOSCTL=BIN SESSION=DIR modules/scripts/attestor-e2e.sh OUT_DIR
#                                                            through a running Logos Core with the module loaded
# Writes OUT_DIR/summary.md (one PASS/FAIL line per check), req/, rep/, work/. Exit 1 on a failed check.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
mkdir -p "$1"
out=$(cd "$1" && pwd)
rm -rf "$out/req" "$out/rep" "$out/work"
mkdir -p "$out/req" "$out/rep" "$out/work"
CALL=${CALL:-native}
M=ferrata_attestor_module
Z="$repo/checkpoint/attestor/runs/zone"
DEV="$repo/../upstream/lez-v0.4.0-rc1/lez/configs/docker-all-in-one/devnet"
PORT=${PORT:-$((20000 + ($$ + 7) % 20000))}
work="$out/work"
summary="$out/summary.md"
fails=0
echo "# ferrata_attestor_module on the zone's records ($CALL, $(date -u +%FT%TZ))" > "$summary"
echo >> "$summary"
printf 'name\tmethod\tseconds\n' > "$out/timings.tsv"

# call METHOD NAME: request req/NAME.json (or none, for version/status/stop), reply rep/NAME.json
call() {
  local t0 req=() rc=0
  t0=$(date +%s.%N)
  [ -f "$out/req/$2.json" ] && req=("@$out/req/$2.json")
  case "$CALL" in
    native)
      if [ "$1" = start ]; then # natively, the loop runs to its max_rounds and prints its status
        "$repo/modules/$M/rust-lib/target/release/examples/call" daemon "$out/req/$2.json" > "$out/rep/$2.json"
      else
        "$repo/modules/$M/rust-lib/target/release/examples/call" "$1" ${req:+"$out/req/$2.json"} > "$out/rep/$2.json"
      fi ;;
    logosctl)
      "$LOGOSCTL" --config-dir "$SESSION" --timeout 300000 call "$M" "$1" "${req[@]}" --json < /dev/null > "$out/rep/$2.raw" 2>&1 || rc=$?
      jq -c 'if (.result | type) == "string" then (.result | fromjson? // .result) else .result end' \
        "$out/rep/$2.raw" > "$out/rep/$2.json" 2>/dev/null || echo "{\"verdict\":\"call-failed\",\"exit\":$rc}" > "$out/rep/$2.json" ;;
  esac
  printf '%s\t%s\t%s\n' "$2" "$1" "$(awk -v a="$t0" -v b="$(date +%s.%N)" 'BEGIN { printf "%.2f", b - a }')" >> "$out/timings.tsv"
}
req() { cat > "$out/req/$1.json"; }
rep() { cat "$out/rep/$1.json"; }
# The zone saved no LEZ node's answers: its inscribed blocks (all applied there) stand in as the LEZ node's final blocks.
lez="$work/lez-standin"
python3 -I "$here/recorded_node.py" --save-lez "$Z/answers" "$lez" > "$out/lez-standin.log"
check() { # NAME CONDITION
  if eval "$2"; then echo "- PASS $1" >> "$summary"; else echo "- FAIL $1" >> "$summary"; fails=$((fails + 1)); fi
}
same() { [ "$(jq -S -c "$1" "$2")" = "$(jq -S -c "$3" "$4")" ]; }

call version version
check "version: the module names the attestor it was built from" '[ "$(jq -r .module "$out/rep/version.json")" = "$M" ] && jq -e ".ferrata_attestor.source_sha256" "$out/rep/version.json" > /dev/null'

# 1. The node's final tier, as the binary's `tier` wrote it in the run.
jq -n --arg a "$Z/answers" '{answers_dir: $a}' | req tier
call tier tier
check "tier: tip 30, four keys, the blocks per signer = the run's tier.json" \
  'same "{tip_id, tip_hash, final_msg_id, keys, first_block, blocks_checked, lib_slot, blocks_per_signer}" "$out/rep/tier.json" "{tip_id, tip_hash, final_msg_id, keys, first_block, blocks_checked, lib_slot, blocks_per_signer}" "$Z/tier.json"'

# 2. The base input (devnet set-up: the final blocks after the first inscribed one, to the tip; nMin 4, tMin 3).
jq -n --arg a "$Z/answers" --arg g "$Z/deployment-settings.yaml" --arg o "$work/base.input" \
  '{answers_dir: $a, genesis_file: $g, n_min: 4, t_min: 3, out: $o}' | req base
call base base
check "base: blocks 3..=30; its deployment = the run's deployment.json" \
  '[ "$(jq -c "[.from, .to, .n]" "$out/rep/base.json")" = "[3,30,4]" ] && same .deployment "$out/rep/base.json" . "$Z/deployment.json"'
dep=$(jq -c .deployment "$out/rep/base.json")

# 3. Each sequencer's attestor: key, certificate by its channel key, attestation after checking the tier.
for i in 0 1 2 3; do
  jq -n --arg o "$work/attestation-$i.key" --arg l "zone-attestation-$i" '{out: $o, seed_label: $l}' | req "keygen-$i"
  call keygen "keygen-$i"
  jq -n --arg k "$DEV/seq-$i/channel_signing_key" --arg p "$(jq -r .public_key "$out/rep/keygen-$i.json")" \
    --argjson d "$dep" --arg o "$work/cert-$i.json" '{channel_key_file: $k, att_pub: $p, deployment: $d, out: $o}' | req "certify-$i"
  call certify "certify-$i"
  check "certify $i: the channel key's certificate = the run's cert-$i.json" \
    'same "{channel_key, attestation_key, certificate, certificate_message}" "$work/cert-$i.json" "{channel_key, attestation_key, certificate, certificate_message}" "$Z/cert-$i.json"'
  jq -n --argjson d "$dep" --arg c "$work/cert-$i.json" --arg k "$work/attestation-$i.key" --arg a "$Z/answers" \
    --arg s "$work/state-$i.json" --arg o "$work/att-$i.json" --arg l "$lez" \
    '{deployment: $d, cert_file: $c, att_key_file: $k, answers_dir: $a, lez_dir: $l, state_file: $s, out: $o}' | req "attest-$i"
  call attest "attest-$i"
  check "attest $i: tip 30 signed, the LEZ node's highest final block (a first read starts there); the signed attestation = the run's att-$i.json (encoded)" \
    '[ "$(jq -c "[.verdict, .tip_id, .first_block, .blocks_checked]" "$out/rep/attest-$i.json")" = "[\"attested\",30,30,1]" ] && same .encoded "$work/att-$i.json" .encoded "$Z/att-$i.json"'
done

# 4. Anyone verifies them.
jq -n --argjson d "$dep" --arg w "$work" '{deployment: $d, att_files: [range(4) | "\($w)/att-\(.).json"]}' | req verify
call verify verify
check "verify: the four attestations verify" '[ "$(jq -c "[.verdict, .refused]" "$out/rep/verify.json")" = "[\"verified\",0]" ]'

# 5. The collector.
proven=$(sha256sum "$Z/rbz-zone-n4-t3.input" | cut -d' ' -f1)
jq -n --arg b "$work/base.input" --arg w "$work" --arg o "$work/n4-t3.input" \
  '{base_file: $b, att_files: ["\($w)/att-0.json", "\($w)/att-1.json", "\($w)/att-2.json"], name: "rbz-zone-n4-t3", out: $o}' | req collect-3
call collect collect-3
check "collect 3 of 4 (sequencer 3 silent): accepted natively, n = 4, t = 3, the input = the one the checkpoint program proved (sha256 $proven)" \
  '[ "$(jq -c "[.verdict, .n, .t, (.kept | length)]" "$out/rep/collect-3.json")" = "[\"collected\",4,3,3]" ] && [ "$(jq -r .input_sha256 "$out/rep/collect-3.json")" = "$proven" ] && jq -r .native "$out/rep/collect-3.json" | grep -q "^accepted"'
check "collect 3 of 4: its journal = the run's (rbz-zone-n4-t3.json)" 'same .journal_sha256 "$out/rep/collect-3.json" .journal_sha256 "$Z/rbz-zone-n4-t3.json"'
jq -n --arg b "$work/base.input" --arg w "$work" '{base_file: $b, att_files: [range(4) | "\($w)/att-\(.).json"], name: "rbz-zone-n4-all"}' | req collect-4
call collect collect-4
check "collect 4 of 4: accepted, its journal = the run's (rbz-zone-n4-all.json)" \
  '[ "$(jq -r .verdict "$out/rep/collect-4.json")" = collected ] && same .journal_sha256 "$out/rep/collect-4.json" .journal_sha256 "$Z/rbz-zone-n4-all.json"'
jq -n --arg b "$work/base.input" --arg w "$work" '{base_file: $b, att_files: ["\($w)/att-0.json", "\($w)/att-1.json"]}' | req collect-2
call collect collect-2
check "collect 2 of 4: refused (t(4) = 3)" '[ "$(jq -r .verdict "$out/rep/collect-2.json")" = refused ] && jq -r .error "$out/rep/collect-2.json" | grep -q "too few attestations: 2 valid"'
jq -n --arg b "$work/base.input" --arg w "$work" \
  '{base_file: $b, att_files: ["\($w)/att-0.json", "\($w)/att-1.json"], name: "rbz-refuse-two-of-four", expect: "InsufficientAttestations"}' | req assemble-2
call assemble assemble-2
check "assemble 2 of 4: the program's own rule refuses it (InsufficientAttestations { have: 2, need: 3 }), as the run's refusal input" \
  '[ "$(jq -r .verdict "$out/rep/assemble-2.json")" = assembled ] && same .meta.native "$out/rep/assemble-2.json" .native "$Z/rbz-refuse-two-of-four.json"'

# 6. What the attestor refuses to sign.
echo '{"tip_id": 31, "tip_hash": "0000000000000000000000000000000000000000000000000000000000000000"}' > "$work/state-ahead.json"
jq -n --argjson d "$dep" --arg c "$work/cert-0.json" --arg k "$work/attestation-0.key" --arg a "$Z/answers" --arg s "$work/state-ahead.json" \
  --arg l "$lez" '{deployment: $d, cert_file: $c, att_key_file: $k, answers_dir: $a, lez_dir: $l, state_file: $s}' | req refuse-behind
call attest refuse-behind
check "refused: it signed tip 31 and its LEZ node holds no block 31 (the node's final tip, 30, is below it): it never signs a lower tip" \
  '[ "$(jq -r .verdict "$out/rep/refuse-behind.json")" = refused ] && jq -r .error "$out/rep/refuse-behind.json" | grep -q "holds no block 31, which this attestor signed"'
jq -n --arg h "$(printf '%064d' 0)" '{tip_id: 30, tip_hash: $h}' > "$work/state-other.json"
jq -n --argjson d "$dep" --arg c "$work/cert-0.json" --arg k "$work/attestation-0.key" --arg a "$Z/answers" --arg s "$work/state-other.json" \
  --arg l "$lez" '{deployment: $d, cert_file: $c, att_key_file: $k, answers_dir: $a, lez_dir: $l, state_file: $s}' | req refuse-other
call attest refuse-other
check "refused: it signed another block 30 than the one its LEZ node holds final: it never signs two histories" \
  '[ "$(jq -r .verdict "$out/rep/refuse-other.json")" = refused ] && jq -r .error "$out/rep/refuse-other.json" | grep -q "another history"'
jq -n --argjson d "$dep" --arg c "$work/cert-1.json" --arg k "$work/attestation-0.key" --arg a "$Z/answers" --arg l "$lez" \
  '{deployment: $d, cert_file: $c, att_key_file: $k, answers_dir: $a, lez_dir: $l}' | req refuse-cert
call attest refuse-cert
check "refused: a certificate of another attestation key" \
  '[ "$(jq -r .verdict "$out/rep/refuse-cert.json")" = refused ] && jq -r .error "$out/rep/refuse-cert.json" | grep -q "does not certify"'
other=$(jq -c '.vault = "0x00000000000000000000000000000000000000aa"' <<< "$dep")
jq -n --argjson d "$other" --arg c "$work/cert-0.json" --arg k "$work/attestation-0.key" --arg a "$Z/answers" --arg l "$lez" \
  '{deployment: $d, cert_file: $c, att_key_file: $k, answers_dir: $a, lez_dir: $l}' | req refuse-deployment
call attest refuse-deployment
check "refused: the same certificate for another vault (a certificate binds chain, vault, configuration and channel)" \
  '[ "$(jq -r .verdict "$out/rep/refuse-deployment.json")" = refused ] && jq -r .error "$out/rep/refuse-deployment.json" | grep -q "does not certify"'

# 7. The attestor's loop in the module, reading a node over HTTP (the zone node's answers, served) and asking a LEZ
#    node's getBlock on the same port (the stand-in).
python3 -I "$here/recorded_node.py" "$Z/answers" "$PORT" "$out/node.log" &
node_pid=$!
trap 'kill $node_pid 2>/dev/null || true' EXIT
: > "$out/node.log"
for _ in $(seq 1 100); do (echo > "/dev/tcp/127.0.0.1/$PORT") 2>/dev/null && break; sleep 0.1; done
from=$(jq -s '[.[][] | .header.slot] | min' "$Z"/answers/blocks/*.json)
jq -n --argjson d "$dep" --arg c "$work/cert-0.json" --arg k "$work/attestation-0.key" --arg n "http://127.0.0.1:$PORT" \
  --arg w "$work/loop" --arg p "$work/published" --arg s "$work/loop-state.json" --argjson f "$from" \
  '{deployment: $d, cert_file: $c, att_key_file: $k, node: $n, lez: $n, work_dir: $w, publish_dir: $p, state_file: $s,
    slot_from: $f, poll_ms: 500, max_rounds: 3}' | req start
call start start
if [ "$CALL" = logosctl ]; then
  for _ in $(seq 1 300); do
    call status status
    [ "$(jq -r .running "$out/rep/status.json")" = false ] && break
    sleep 0.2
  done
  cp "$out/rep/status.json" "$out/rep/loop.json"
  call stop stop
else
  cp "$out/rep/start.json" "$out/rep/loop.json"
fi
check "loop: 3 rounds against the node over HTTP: signed tip 30 once, then found it unchanged twice; nothing refused" \
  '[ "$(jq -c "[.rounds, .attested, .unchanged, .refused, .errors, .last_attestation.tip_id]" "$out/rep/loop.json")" = "[3,1,2,0,0,30]" ]'
check "loop: its published attestation = the one-shot attest's (att-30.json, latest.json)" \
  'same .encoded "$work/published/att-30.json" .encoded "$work/att-0.json" && same .encoded "$work/published/latest.json" .encoded "$work/att-0.json"'
check "loop: it read the node each round (info, channel, blocks) and asked its LEZ node (getBlock)" \
  '[ "$(grep -c "^/cryptarchia/info " "$out/node.log")" -ge 3 ] && [ "$(grep -c "^/channel/" "$out/node.log")" -ge 3 ] && grep -q "^/cryptarchia/blocks?slot_from=" "$out/node.log" && [ "$(grep -c "^POST getBlock " "$out/node.log")" -ge 3 ]'
kill $node_pid 2>/dev/null || true
trap - EXIT

{
  echo
  echo "Checks failed: $fails"
} >> "$summary"
cat "$summary"
[ "$fails" -eq 0 ]
