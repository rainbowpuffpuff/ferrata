#!/usr/bin/env bash
# The Logos Core run of ferrata_attest_module on a running LEZ zone (RFP-021 U.1 and F.9): the hook of
# cli/tests/attest-seq.sh (ATTEST_SEQ_HOOK=this, ATTEST_LOGOS_BUILD=the OUT of scripts/logos-build.sh), run by
# lez/crown/session.sh x while the zone runs. Local only: a zone we started; Ethereum from recorded answers.
#
# 1. cli/tests/attest-seq-hook.sh first: `ferrata attest` creates the mainnet instance and its record (and attests
#    under it), the plain path every module call is compared with.
# 2. A headless Logos Core (logosctl): lez_core (patched), beacon_api_module and ferrata_attest_module installed from
#    their .lgx and loaded. A LEZ wallet holding the payer's key (`ferrata attest wallet`), opened inside lez_core
#    through the module (`lez`).
# 3. U.1, through Logos Core only: the module reads the Sepolia instance, the mainnet instance and its record through
#    lez_core (`lez_read`), each compared with the plain path; plans an update of the Sepolia instance (`advance`, on the
#    state lez_core read) and sends its job through lez_core (`lez_submit`); reads the instance again (moved); fetches a
#    log's proof (`fetch_log_proof`), dry-runs it on the record lez_core read (`dry_run_attest`), builds the `Attest`
#    (`build_attest_tx`) and sends it through lez_core; and a request the program refuses, the same way. The zone's
#    verdict on every transaction lez_core sent: `ferrata attest receipt` (the follower).
# 4. F.9: beacon_api_module configured with three providers on loopback that answer what Lodestar, publicnode and
#    Nimbus answered in verify_checkpoint's recorded live run; `verify_checkpoint` with its beacon checks through
#    beacon_api_module (`providers: {via: beacon_api_module}`).
# Records: $ATTEST_SEQ_RUN/logos/ (calls.jsonl: every call with its reply).
set -uo pipefail
RUN="${ATTEST_SEQ_RUN:?}"; BIN="${ATTEST_SEQ_BIN:?}"; REPO="${ATTEST_SEQ_REPO:?}"; SA="${ATTEST_SEQ_SA:?}"; W="${ATTEST_SEQ_WORK:?}"
LB="${ATTEST_LOGOS_BUILD:?the OUT of modules/ferrata_attest_module/scripts/logos-build.sh}"
M="$RUN/logos"; mkdir -p "$M" "$W"
hexof() { python3 -I -c 'import sys; print(open(sys.argv[1], "rb").read().hex())' "$1"; }
say() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$RUN/run.log"; }

# ---- 1. the plain path ----------------------------------------------------------------------------------------------
bash "$REPO/cli/tests/attest-seq-hook.sh"
LEZ="http://127.0.0.1:${RPC_PORT:-3040}"
ATTEST="$(jq -r .attestation.account "$SA")"
SEP="$(jq -r '.instance.id // .instance' "$SA")"
MID="$(grep -h -o -m1 'init: instance 0x[0-9a-f]*' "$RUN"/cli/*mainnet-init.txt | awk '{print $3}')"
F="$REPO/crates/ferrata-spectests/fixtures"; MF="$F/mainnet-attest-2026-11-10"; S12="$F/sepolia-2026-11-12"
LAB="$REPO/lez/lab/results/record/net/sepolia/updates"
CLI=("$BIN/ferrata" attest --lez-rpc "$LEZ" --program "$ATTEST" --key "$W/payer.key" --state-dir "$W/follower" --record "$M/cli.jsonl")
say "logos: zone $LEZ; program $ATTEST; Sepolia instance $SEP; mainnet instance ${MID:-none}"

# ---- 2. Logos Core --------------------------------------------------------------------------------------------------
LOGOSCTL="$LB/check/logos/bin/logosctl"
for x in "$LOGOSCTL" "$LB/check/lgx-ferrata_attest_module" "$LB/lgx-lez_core-patched" "$LB/lgx-beacon_api_module"; do
  [ -e "$x" ] || { say "logos: STOP: $x missing (run logos-build.sh)"; exit 1; }
done
LT="$(cd "$REPO/../tmp" && pwd)/lc"; mkdir -p "$LT"              # Logos Core's sockets: a short TMPDIR
S="$W/core"; rm -rf "$S"; mkdir -p "$S.home"
ctl() { TMPDIR="$LT" HOME="$S.home" "$LOGOSCTL" --config-dir "$S" "$@"; }
finish() { ctl daemon stop >> "$M/daemon.txt" 2>&1 || true; kill ${PIDS:-} 2>/dev/null || true;
           tail -n 200 "$S"/logs/*.log > "$M/core-logs-tail.txt" 2>/dev/null || true; }
trap finish EXIT
ctl daemon start --detach >> "$M/daemon.txt" 2>&1 || { say "logos: STOP: daemon start"; exit 1; }
for d in "$LB/lgx-lez_core-patched" "$LB/lgx-beacon_api_module" "$LB/check/lgx-ferrata_attest_module"; do
  ctl install "$d"/*.lgx -y >> "$M/daemon.txt" 2>&1 || { say "logos: STOP: install $d"; exit 1; }
done
for m in lez_core beacon_api_module ferrata_attest_module; do
  ctl module load "$m" >> "$M/daemon.txt" 2>&1 || { say "logos: STOP: load $m"; exit 1; }
done
ctl module list --json < /dev/null > "$M/modules-loaded.json" 2>&1 || true
( cd "$LB" && sha256sum check/lgx-ferrata_attest_module/*.lgx lgx-lez_core-patched/*.lgx lgx-beacon_api_module/*.lgx ) > "$M/lgx-sha256.txt"
say "logos: lez_core (patched), beacon_api_module and ferrata_attest_module installed from their .lgx and loaded"
n=0
call() { # module method json -> the reply (JSON) in $W/reply-N.json; one line in calls.jsonl
  local mod="$1" method="$2" params="$3" req
  n=$((n + 1)); req="$W/req-$n.json"; echo "$params" > "$req"
  callv "$mod" "$method" "@$req"
}
callv() { # module method args... (logosctl's: @file a file's contents as a string, decimal numbers integers)
  local mod="$1" method="$2" out rc reply; shift 2
  [ "${req:-}" ] || n=$((n + 1))
  out="$(ctl --timeout 3600000 call "$mod" "$method" "$@" --json < /dev/null 2> "$W/call-$n.err")"; rc=$?
  req=""
  reply="$(echo "$out" | jq -c '.result | if type == "string" then (fromjson? // .) else . end' 2>/dev/null)"
  [ -n "$reply" ] || reply="$(jq -c -n --arg o "$out" --arg e "$(head -c 400 "$W/call-$n.err")" '{verdict: "logosctl_error", out: $o, err: $e}')"
  jq -c -n --arg mod "$mod" --arg m "$method" --argjson r "$reply" --argjson rc "$rc" --arg t "$(date -u +%FT%TZ)" \
    '{t: $t, module: $mod, method: $m, reply: $r, logosctl_exit: $rc}' >> "$M/calls.jsonl"
  echo "$reply" > "$W/reply-$n.json"
  say "logos: $mod.$method: $(echo "$reply" | jq -c 'del(.steps, .decoded, .state, .requests, .providers, .bootstrap, .committee) | tostring | .[0:240]' -r)"
}
R() { cat "$W/reply-$n.json"; }

# The payer's wallet for lez_core: LEZ's own storage with the payer's key (the funded test account of LEZ's local net).
echo "ferrata-logos-run" > "$W/lez.pass"
"${CLI[@]}" wallet --home "$W/lez-wallet" --password-file "$W/lez.pass" >> "$M/cli.txt" 2>&1 || { say "logos: STOP: wallet"; exit 1; }
call ferrata_attest_module lez "$(jq -c -n --arg c "$W/lez-wallet/wallet_config.json" --arg s "$W/lez-wallet/storage.json" \
  --arg p "$W/lez.pass" '{via: "lez_core", wallet: {config: $c, storage: $s, password_file: $p}}')"
PAYER="$(R | jq -r '.public_accounts[0] // empty')"
[ -n "$PAYER" ] || { say "logos: STOP: lez_core opened no wallet"; exit 1; }

# ---- 3. U.1: reads and sends through lez_core ------------------------------------------------------------------------
read_state() { # instance kind [slot] -> reply
  call ferrata_attest_module lez_read "$(jq -c -n --arg p "$ATTEST" --arg i "$1" --arg k "$2" --argjson s "${3:-0}" \
    '{program: $p, instance_id: $i, kind: $k, slot: $s}')"
}
# Each read compared byte for byte with the plain path's (the follower's state of the zone's final blocks).
compare() { # label instance [slot]: the last lez_read reply against `ferrata attest state`
  local plain via
  plain="$("${CLI[@]}" state --instance "$2" ${3:+--slot "$3"} 2>/dev/null | tail -n 1)"
  via="$(R | jq -r .state)"
  if [ "$plain" = "$via" ]; then echo "equal"; else echo "DIFFERENT"; fi
  echo -e "$1\t$(R | jq -r .bytes)\t$( [ "$plain" = "$via" ] && echo equal || echo different)" >> "$M/reads.tsv"
}
echo -e "read\tbytes\tagainst the plain path" > "$M/reads.tsv"
read_state "$SEP" instance
SEP_STATE="$(R | jq -r .state)"
say "logos: Sepolia instance through lez_core: $(R | jq -r .bytes) bytes, $(compare sepolia-instance "$SEP") to the plain path's"
if [ -n "$MID" ]; then
  read_state "$MID" instance
  say "logos: mainnet instance through lez_core: $(R | jq -r .bytes) bytes, $(compare mainnet-instance "$MID") to the plain path's"
  read_state "$MID" record 15624480
  MREC="$(R | jq -c .record)"
  say "logos: mainnet record through lez_core: $(R | jq -r .bytes) bytes, $(compare mainnet-record "$MID" 15624480) to the plain path's"
fi
# An update of the Sepolia instance planned by the module on the state lez_core read, sent through lez_core.
UPD="$LAB/00065.finality.gloas.ssz"
# The committee that signs it (period 1412): update_1411's next committee, the keys then the aggregate key (a Gloas
# LightClientUpdate is fixed-size: the attested header's 496 bytes, then the next committee's 513 x 48).
C1412="$(python3 -I -c 'import sys; print(open(sys.argv[1], "rb").read()[496:496 + 513 * 48].hex())' "$S12/update_1411.ssz")"
call ferrata_attest_module advance "$(jq -c -n --arg p "$ATTEST" --arg st "$SEP_STATE" --arg u "0x$(hexof "$UPD")" \
  --arg b "0x$(hexof "$S12/bootstrap.ssz")" --arg c "0x$C1412" \
  '{program_id: $p, instance_state: $st, finality_update: $u, bootstrap: $b, committees: [$c]}')"
cp "$W/reply-$n.json" "$M/advance-plan.json"
if [ "$(R | jq -r .verdict)" = useful ]; then
  STEPS="$(R | jq -c .steps)"
  call ferrata_attest_module lez_submit "$(jq -c -n --arg p "$ATTEST" --arg pay "$PAYER" --argjson s "$STEPS" '{program: $p, payer: $pay, steps: $s}')"
  cp "$W/reply-$n.json" "$M/advance-submit.json"
  HASHES=($(R | jq -r '.steps[].tx_hash // empty'))
  [ ${#HASHES[@]} -gt 0 ] && "${CLI[@]}" receipt $(printf -- '--tx %s ' "${HASHES[@]}") > "$M/cli-advance-receipts.txt" 2>&1
  read_state "$SEP" instance
  say "logos: the Sepolia instance read again through lez_core: $(R | jq -r .bytes) bytes, $(compare sepolia-instance-after "$SEP") to the plain path's"
  "${CLI[@]}" status --instance "$SEP" > "$M/cli-sepolia-status-after.txt" 2>&1
  say "logos: after the job sent through lez_core: $(grep -o 'finalized slot [0-9]*' "$M/cli-sepolia-status-after.txt") (plain path); receipts: $(grep -c applied "$M/cli-advance-receipts.txt" 2>/dev/null || echo 0) applied"
else
  say "logos: advance did not plan a useful job: $(R | jq -c '{verdict, error}')"
fi
# A log attested through the module, sent through lez_core; then one the program refuses.
python3 -I "$REPO/modules/scripts/recorded_rpc.py" "$MF" 8646 "$W/recorded-rpc-logos.log" > /dev/null 2>&1 & PIDS="$!"
sleep 1
if [ -n "$MID" ] && [ -n "${MREC:-}" ]; then
  for case in good tampered; do
    call ferrata_attest_module fetch_log_proof '{"rpc_url": "http://127.0.0.1:8646", "block": 26384851, "tx_index": 0, "log_index": 0}'
    LOG="$(R | jq -c .log)"
    if [ "$case" = tampered ]; then
      LOG="$(echo "$LOG" | jq -c '.receipt_proof[0] |= (.[0:10] + (if .[10:12] == "00" then "01" else "00" end) + .[12:])')"
    fi
    call ferrata_attest_module dry_run_attest "$(jq -c -n --argjson r "$MREC" --argjson l "$LOG" '{chain_id: 1, record: $r, log: $l}')"
    PRED="$(R | jq -c '{verdict, code}')"
    call ferrata_attest_module build_attest_tx "$(jq -c -n --arg p "$ATTEST" --arg i "$MID" --argjson l "$LOG" \
      '{program_id: $p, instance_id: $i, chain_id: 1, slot: 15624480, log: $l}')"
    ACC="$(R | jq -r .account)"; MSG="$(R | jq -r .message)"
    call ferrata_attest_module lez_submit "$(jq -c -n --arg p "$ATTEST" --arg pay "$PAYER" --arg a "$ACC" --arg m "$MSG" \
      '{program: $p, payer: $pay, steps: [{name: "Attest", root: $a, accounts: [$a], message: $m}]}')"
    H="$(R | jq -r '.steps[0].tx_hash // empty')"
    [ -n "$H" ] && "${CLI[@]}" receipt --tx "$H" > "$M/cli-attest-$case-receipt.txt" 2>&1
    say "logos: attest ($case): the module's dry run $PRED; the zone: $(tail -n 1 "$M/cli-attest-$case-receipt.txt" 2>/dev/null)"
  done
fi

# ---- 4. F.9: verify_checkpoint with its beacon checks through beacon_api_module ------------------------------------
REC="$REPO/modules/ferrata_attest_module/rust-lib/calls/tests/fixtures/verify-checkpoint-sepolia.record.json"
cat > "$W/replay-provider.py" <<'EOF'
import json, sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
record, base, port = sys.argv[1], sys.argv[2], int(sys.argv[3])
answers = {e["url"][len(base):]: (int(e.get("status") or 0), e.get("response") or "")
           for e in json.load(open(record)) if e.get("method") == "GET" and e["url"].startswith(base)}
class H(BaseHTTPRequestHandler):
    def do_GET(self):
        status, body = answers.get(self.path, (404, '{"code":404,"message":"not recorded"}'))
        body = body.encode()
        self.send_response(status or 502); self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body)
    def log_message(self, *a): pass
ThreadingHTTPServer(("127.0.0.1", port), H).serve_forever()
EOF
port=8711
providers='[]'
for spec in "lodestar https://lodestar-sepolia.chainsafe.io" "publicnode https://ethereum-sepolia-beacon-api.publicnode.com" "nimbus http://unstable.sepolia.beacon-api.nimbus.team"; do
  set -- $spec
  python3 -I "$W/replay-provider.py" "$REC" "$2" "$port" > /dev/null 2>&1 & PIDS="$PIDS $!"
  providers="$(echo "$providers" | jq -c --arg i "$1" --arg e "http://127.0.0.1:$port" '. + [{id: $i, endpoint: $e}]')"
  port=$((port + 1))
done
sleep 1
jq -c -n --argjson p "$providers" '{providers: $p, proxyRequired: false}' > "$W/beacon-config.json"
callv beacon_api_module set_beacon_config 11155111 "@$W/beacon-config.json"
callv beacon_api_module list_providers 11155111
VREQ="$(jq -c --arg b "0x$(hexof "$S12/bootstrap.ssz")" \
  '.bootstrap = $b | .providers = {via: "beacon_api_module", chain_id: 11155111} | del(.execution) | .min_providers = 3' \
  "$REPO/modules/ferrata_attest_module/rust-lib/calls/tests/fixtures/verify-checkpoint-sepolia.json")"
call ferrata_attest_module verify_checkpoint "$VREQ"
cp "$W/reply-$n.json" "$M/verify-checkpoint-beacon-api-module.json"
say "logos: verify_checkpoint through beacon_api_module: $(R | jq -c '{verdict, failed, providers_agreeing, providers: [.providers[]? | {name, verdict, via, slot}]}')"
say "logos: done ($n module calls)"
