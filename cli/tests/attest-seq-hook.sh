#!/usr/bin/env bash
# The hook of cli/tests/attest-seq.sh, run by lez/crown/session.sh x while the zone runs (RPC_PORT and the zone's paths
# in the environment). Every `ferrata attest` command against the zone's sequencer, on real data, each recorded in
# $ATTEST_SEQ_RUN/cli/ and attest.jsonl. Local only.
set -uo pipefail
RUN="${ATTEST_SEQ_RUN:?}"; BIN="${ATTEST_SEQ_BIN:?}"; REPO="${ATTEST_SEQ_REPO:?}"; SA="${ATTEST_SEQ_SA:?}"; W="${ATTEST_SEQ_WORK:?}"
mkdir -p "$W" "$RUN/cli"
say() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$RUN/run.log"; }
F="$REPO/crates/ferrata-spectests/fixtures"
M="$F/mainnet-attest-2026-11-10"; S12="$F/sepolia-2026-11-12"; LAB="$REPO/lez/lab/results/record/net/sepolia/updates"
LEZ="http://127.0.0.1:${RPC_PORT:-3040}"
ATTEST="$(jq -r .attestation.account "$SA")"
SEP="$(jq -r '.instance.id // .instance' "$SA")"
SEP_CP="$(jq -r '.instance.checkpoint // empty' "$SA")"
# The fee payer: the second of the two accounts the genesis of LEZ's local network funds, whose key LEZ publishes
# (lez/live/driver/src/keys.rs); it holds nothing of value.
echo f434f8741720014586ae43356d2aec6257da086222f604ddb75d69733b86fc4c > "$W/payer.key"
COMMON=(--lez-rpc "$LEZ" --program "$ATTEST" --key "$W/payer.key" --state-dir "$W/follower" --record "$RUN/attest.jsonl")
echo -e "step\texit\texpected\tcode\tresult" > "$RUN/cli/summary.tsv"
n=0
step() { # name expected args...; expected: 0 (applied, or nothing sent), or 3:CODE (sent and refused on the zone with CODE)
  local name="$1" expected="$2"; shift 2
  n=$((n + 1)); local f; f="$RUN/cli/$(printf %02d $n)-$name.txt"
  local shown="$*"; shown="${shown//"$REPO"/\$REPO}"
  echo "\$ ferrata attest $shown" > "$f"
  "$BIN/ferrata" attest "${COMMON[@]}" "$@" >> "$f" 2>&1
  local rc=$?
  # The code the zone refused the last sent transaction with ("block N (T s): refused with CODE ..."), if any.
  local code; code=$(grep -o 'block [0-9]* ([0-9]* s): refused with [0-9]*' "$f" | tail -n 1 | awk '{print $NF}')
  local result="as expected"
  [ "$rc${code:+:$code}" = "$expected" ] || result="NOT as expected"
  echo -e "$name\t$rc\t$expected\t${code:--}\t$result" >> "$RUN/cli/summary.tsv"
  say "$name: exit $rc${code:+, refused with $code} (expected $expected): $result"
  tail -n 4 "$f" | sed 's/^/    /' | tee -a "$RUN/run.log" > /dev/null
  return 0
}

say "zone $LEZ; attestation program $ATTEST; session A's Sepolia instance $SEP"
# ---- the Sepolia instance session A created: configuration, state, two real updates, a replay -----------------
step sepolia-config 0 config --instance "$SEP" ${SEP_CP:+--checkpoint "$SEP_CP"}
step sepolia-status 0 status --instance "$SEP"
step sepolia-advance-finality-dry-run 0 advance --instance "$SEP" --update "$LAB/00057.finality.gloas.ssz" --finality \
  --committee "bootstrap:$S12/bootstrap.ssz"
step sepolia-status-after-dry-run 0 status --instance "$SEP"
step sepolia-advance-finality 0 advance --instance "$SEP" --update "$LAB/00057.finality.gloas.ssz" --finality \
  --committee "bootstrap:$S12/bootstrap.ssz" --send --yes
step sepolia-advance-period-1412 0 advance --instance "$SEP" --update "$LAB/00064.period.gloas.ssz" \
  --committee "update:$S12/update_1411.ssz" --committee "bootstrap:$S12/bootstrap.ssz" --send --yes
# A replay on purpose: the journal in --state-dir would report the earlier run and send nothing; --again sends it.
step sepolia-advance-replayed 3:23 advance --instance "$SEP" --update "$LAB/00057.finality.gloas.ssz" --finality \
  --committee "bootstrap:$S12/bootstrap.ssz" --send --yes --again
step sepolia-status-after 0 status --instance "$SEP"

# ---- a mainnet instance created here, its record, and attestations under it (mainnet-attest-2026-11-10) -------------
BOOT_ROOT="$(jq -r .bootstrap_root "$M/fixture.json")"
python3 -I "$REPO/modules/scripts/recorded_rpc.py" "$M" 8645 "$W/recorded-rpc.log" > "$W/recorded-rpc.out" 2>&1 & RPC=$!
trap 'kill $RPC $P1 $P2 2>/dev/null || true' EXIT
sleep 1
step mainnet-init 0 init --preset mainnet --bootstrap "$M/bootstrap.ssz" --checkpoint "$BOOT_ROOT" --layout electra --parts 2 --send --yes
MID="$(grep -o -m1 'init: instance 0x[0-9a-f]*' "$RUN/cli/$(printf %02d $n)-mainnet-init.txt" | awk '{print $3}')"
say "mainnet instance $MID"
step mainnet-advance-finality 0 advance --instance "$MID" --update "$M/finality_update.ssz" --finality \
  --committee "bootstrap:$M/bootstrap.ssz" --send --yes
step mainnet-config 0 config --instance "$MID" --checkpoint "$BOOT_ROOT"
step mainnet-status 0 status --instance "$MID"
ETH=http://127.0.0.1:8645
WETH="$(jq -r .weth "$M/fixture.json")"; SLOT="$(jq -r .weth_balance_slot "$M/fixture.json")"
BAL="$(jq -r '.result // .' "$M/call_weth_balance_26384851.json")"
USDC="$(jq -r .transfer.token "$M/fixture.json")"
FAILED="$(jq -r .failed_tx "$M/fixture.json")"
A=(attest --instance "$MID" --record-slot 15624480 --eth "$ETH" --block 26384851)
step attest-storage-dry-run 0 "${A[@]}" --storage "$WETH:$SLOT:eq:$BAL"
step attest-storage 0 "${A[@]}" --storage "$WETH:$SLOT:eq:$BAL" --send --yes
step attest-storage-predicate-fails 3:130 "${A[@]}" --storage "$WETH:$SLOT:eq:0x01" --send --yes
step attest-storage-ge 0 "${A[@]}" --storage "$WETH:$SLOT:ge:0x01" --send --yes
step attest-log-transfer 0 "${A[@]}" --log "0:0:emitter=$USDC" --send --yes
step attest-log-wrong-emitter 3:130 "${A[@]}" --log "0:0:emitter=$WETH" --send --yes
step attest-account-codehash 0 "${A[@]}" --account "$WETH:codehash" --send --yes
step attest-log-failed-tx 3:124 "${A[@]}" --log "$FAILED:0" --send --yes
step attest-storage-and-log 0 "${A[@]}" --storage "$WETH:$SLOT:eq:$BAL" --log "0:0:emitter=$USDC" --send --yes
step mainnet-advance-replayed 3:52 advance --instance "$MID" --update "$M/finality_update.ssz" --finality \
  --committee "bootstrap:$M/bootstrap.ssz" --send --yes --again

# ---- the attestation side's monitor reads the mainnet instance through the sequencer's getAccount -------------------
cat > "$W/providers.py" <<'EOF'
import json, sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
header_file, port = sys.argv[1], int(sys.argv[2])
routes = {
    "/eth/v1/beacon/genesis": json.dumps({"data": {"genesis_validators_root": "0x4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95",
                                                   "genesis_time": "1606824023", "genesis_fork_version": "0x00000000"}}),
    "/eth/v1/beacon/headers/15624480": open(header_file).read(),
    "/eth/v1/beacon/headers/finalized": json.dumps({"data": {"root": "0x00", "canonical": True, "header": {"message": {"slot": "15624544"}}}}),
}
class H(BaseHTTPRequestHandler):
    def do_GET(self):
        body = routes.get(self.path)
        code = 200 if body else 404
        body = (body or '{"code":404}').encode()
        self.send_response(code); self.send_header("Content-Type", "application/json"); self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body)
    def log_message(self, *a): pass
ThreadingHTTPServer(("127.0.0.1", port), H).serve_forever()
EOF
python3 -I "$W/providers.py" "$M/beacon_header_15624480_second.json" 8701 & P1=$!
python3 -I "$W/providers.py" "$M/beacon_header_15624480_second.json" 8702 & P2=$!
sleep 1
# The record's time: its finality update's signature slot (15,624,545, the earliest the record can exist, 65 slots
# after its finalized header 15,624,480; mainnet-attest-2026-11-10/fixture.json) plus 600 s. (Runs r4, r6 and rf1 used
# the header's own time plus 600 s, a clock at which no honest instance holds that header: the monitor now calls that
# `instance-ahead`, too new.)
REC_TIME=$(( 1606824023 + 15624545 * 12 + 600 ))
for mode in at-the-record now; do
  f="$RUN/cli/monitor-$mode.json"
  args=(check --state "lez:$LEZ" --program "$ATTEST" --instance "$MID" --provider p1=http://127.0.0.1:8701 --provider p2=http://127.0.0.1:8702)
  [ "$mode" = at-the-record ] && args+=(--now "$REC_TIME")
  "$BIN/attest-monitor" "${args[@]}" > "$f" 2> "$RUN/cli/monitor-$mode.alerts"
  rc=$?
  echo -e "monitor-$mode\t$rc\t-\t-\ta read, no expectation" >> "$RUN/cli/summary.tsv"
  say "attest-monitor ($mode): exit $rc, divergence $(jq -r .divergence "$f"), alerts $(jq -c '[.alerts[].kind]' "$f")"
done
say "hook done: $(grep -c $'\tas expected$' "$RUN/cli/summary.tsv") of $n commands as expected (exit status and the zone's refusal code); $(grep -c '^monitor-' "$RUN/cli/summary.tsv") monitor reads"
