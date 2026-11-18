#!/usr/bin/env bash
# The scripts' guards (review-lez-v03-bridge findings 2, 4, 5, 12, 13), against stubs only: no network, no keys.
# testnet.sh and sepolia.sh are copied into a scratch tree whose bridge03, ferrata_ops, cast, anvil and curl are stubs
# that log what they are asked; each case asserts what was (not) sent. Usage: tests/guards.sh. Exit 0 if all pass.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"; SRC="$(cd "$HERE/.." && pwd)"
W="$HERE/work-guards"; rm -rf "$W"; T="$W/spike/wt/lez-v03-bridge"; B="$W/bin"
mkdir -p "$T" "$B" "$W/spike/wt/e2e/runs/crown-live-3"
cp "$SRC/testnet.sh" "$SRC/sepolia.sh" "$SRC/verify.py" "$T/"
echo '{"groth16Verifier":"0x1","registry":"0x2","token":"0x3"}' > "$W/spike/wt/e2e/runs/crown-live-3/deploy.json"
export SG_LOG="$W/log"; : > "$SG_LOG"
cat > "$B/bridge03" <<'S'
#!/usr/bin/env bash
run="$1"; cmd="$2"; echo "bridge03 $cmd" >> "$SG_LOG"; mkdir -p "$run"
case "$cmd" in
  setup) echo '{"payer":"P","wallet_home":"/home/someone/wallet","holder_private":"HP","holder_public":"HU","config":"c"}' > "$run/run.json" ;;
  config) echo 0xc1d ;;
  deposits) echo '[{"leaf_index":0,"s":"S0","dest":"D0","kind":2,"commitment":"aa","leaf":"L0"},{"leaf_index":1,"s":"S1","dest":"D1","kind":1,"commitment":"bb","leaf":"L1"}]' ;
            jq '.deposits = [{"leaf_index":0,"s":"S0","dest":"D0","kind":2},{"leaf_index":1,"s":"S1","dest":"D1","kind":1}]' "$run/run.json" > "$run/r" && mv "$run/r" "$run/run.json" ;;
  batch) echo '{}' > "${@: -1}" ;;
  state) echo '{"config_id":"c","holder_private (decoded)":"HP","accounts":{"nullifier_0":{"id":"N0"},"burns":{"id":"B","shard":"","program":"p","decoded":null}}}' ;;
  *) echo "bridge03 $cmd done" ;;
esac
S
cat > "$B/ferrata_ops" <<'S'
#!/usr/bin/env bash
echo "ferrata_ops $1 marked=$([ -f "$RUN/batch.queued" ] && echo yes || echo no)" >> "$SG_LOG"
S
cat > "$B/cast" <<'S'
#!/usr/bin/env bash
echo "cast $*" >> "$SG_LOG"
case "$1" in
  chain-id) echo 11155111 ;; nonce) echo 7 ;; compute-address) echo "Computed Address: 0x00000000000000000000000000000000000000a1" ;;
  call) echo "${SG_COUNT:-0}" ;; send) echo SEND >> "$SG_LOG"; echo '{"transactionHash":"0xt","blockNumber":"0x1","gasUsed":"0x1","effectiveGasPrice":"0x1","status":"0x1"}' ;;
esac
S
printf '#!/bin/sh\necho "anvil $*" >> "$SG_LOG"; sleep 30\n' > "$B/anvil"
printf '#!/bin/sh\necho "curl $*" >> "$SG_LOG"\n' > "$B/curl"
printf '#!/bin/sh\nshift 3; exec "$@"\n' > "$B/time"
chmod +x "$B"/*
export PATH="$B:$PATH" BRIDGE03="$B/bridge03" FERRATA_OPS="$B/ferrata_ops"
pass=0; fail=0
ok() { if "$@"; then pass=$((pass+1)); else fail=$((fail+1)); echo "FAIL: ${CASE}: $*"; fi; }
nok() { if "$@"; then fail=$((fail+1)); echo "FAIL: ${CASE}: not $*"; else pass=$((pass+1)); fi; }
sent() { grep -q "$1" "$SG_LOG"; }
case_() { CASE="$1"; : > "$SG_LOG"; R="$W/run-$((pass+fail))"; export RUN="$R"; rm -rf "$R"; }
tn() { env -u FERRATA_TESTNET_SEND FERRATA_NET=testnet FERRATA_RUN="$R" LEZ_RPC=http://127.0.0.1:9 ETH_RPC=http://127.0.0.1:9 "$@"; }

case_ "testnet run without FERRATA_TESTNET_SEND refuses, sends nothing"
tn "$T/testnet.sh" run > "$W/out" 2>&1; rc=$?
ok [ $rc = 2 ]; nok sent ferrata_ops; nok sent bridge03

case_ "testnet prepare without a secret seed refuses (no default seed off local)"
tn FERRATA_LC_INSTANCE=11 FERRATA_WALLET_HOME=/x FERRATA_PAYER_ID=P FERRATA_VAULT=0x1 FERRATA_LC_PROGRAM=L FERRATA_DEPOSITOR=0x2 \
  "$T/testnet.sh" prepare > "$W/out" 2>&1; rc=$?
ok [ $rc = 2 ]; ok grep -q "FERRATA_DEPOSIT_SEED must be a secret" "$W/out"; nok sent "bridge03 config"

case_ "testnet prepare without FERRATA_LC_INSTANCE refuses"
tn FERRATA_DEPOSIT_SEED=secret FERRATA_WALLET_HOME=/x FERRATA_PAYER_ID=P FERRATA_VAULT=0x1 FERRATA_LC_PROGRAM=L FERRATA_DEPOSITOR=0x2 \
  "$T/testnet.sh" prepare > "$W/out" 2>&1; rc=$?
ok [ $rc = 2 ]; ok grep -q "FERRATA_LC_INSTANCE" "$W/out"; nok sent "bridge03 config"

mkcap() { mkdir -p "$R/cap"; echo '{}' > "$R/cap/proof_0x01_77.json"; }
case_ "testnet run: the batch is marked BEFORE ferrata_ops sends; the record keeps no secret-derived value or machine path"
tn FERRATA_DEPOSIT_SEED=secret FERRATA_LC_INSTANCE=11 FERRATA_WALLET_HOME=/x FERRATA_PAYER_ID=P FERRATA_VAULT=0x1 FERRATA_LC_PROGRAM=L \
  FERRATA_DEPOSITOR=0x2 "$T/testnet.sh" prepare > "$W/out" 2>&1
mkcap
tn FERRATA_TESTNET_SEND=yes FERRATA_DEPOSIT_SEED=secret FERRATA_LC_INSTANCE=11 FERRATA_LC_SLOT=5 FERRATA_CAPTURE="$R/cap" \
  "$T/testnet.sh" run > "$W/out" 2>&1
ok grep -q "ferrata_ops batch marked=yes" "$SG_LOG"; ok [ -f "$R/batch.queued" ]
ok [ -f "$T/results/testnet/run.json" ]
nok grep -q -e '"s"' -e '"dest"' -e '"kind"' -e holder_private -e wallet_home -e /home/ "$T/results/testnet/run.json"
nok grep -q -e nullifier_ -e holder_private "$T/results/testnet/state.json"
nok grep -q -e '"s"' -e '"dest"' "$T/results/testnet/deposits.json"
case_ "testnet run again: a queued batch is neither rebuilt nor re-queued"
R="$W/run-$((pass+fail-11))"; R="$(ls -d "$W"/run-* | while read -r d; do [ -f "$d/batch.queued" ] && echo "$d"; done | head -1)"; export RUN="$R"
tn FERRATA_TESTNET_SEND=yes FERRATA_DEPOSIT_SEED=secret FERRATA_LC_INSTANCE=11 FERRATA_LC_SLOT=5 FERRATA_CAPTURE="$R/cap" \
  "$T/testnet.sh" run > "$W/out" 2>&1
nok sent ferrata_ops; nok sent "bridge03 batch"; ok grep -q "not rebuilt, not sent again" "$W/out"

case_ "local run with a non-loopback sequencer refuses"
env FERRATA_NET=local FERRATA_RUN="$R" LEZ_RPC=https://testnet.lez.logos.co FERRATA_TESTNET_SEND=yes "$T/testnet.sh" run > "$W/out" 2>&1; rc=$?
ok [ $rc = 1 ]; nok sent bridge03

sp() { env -u FERRATA_SEPOLIA_SEND -u FERRATA_SEPOLIA_FORK -u FERRATA_SEPOLIA_FORK_PID SEPOLIA_RPC_URL=http://127.0.0.1:9 ETH_FROM=0xd "$@"; }
spr() { mkdir -p "$R"; echo '{"vault":"0xv"}' > "$R/identity.json"; echo 0xc1d > "$R/config_id.txt"
        echo '[{"leaf_index":0,"commitment":"aa"},{"leaf_index":1,"commitment":"bb"}]' > "$R/deposits.json"; }
case_ "sepolia deposit without FERRATA_SEPOLIA_SEND refuses"
spr; sp "$T/sepolia.sh" "$R" deposit 0 > "$W/out" 2>&1; rc=$?
ok [ $rc = 1 ]; nok sent SEND
case_ "sepolia: FERRATA_SEPOLIA_FORK=1 on a loopback that is not the script's own anvil keeps the guard"
spr; sp FERRATA_SEPOLIA_FORK=1 SEPOLIA_RPC_URL=http://127.0.0.1:8547 FERRATA_SEPOLIA_FORK_PID=$$ "$T/sepolia.sh" "$R" deposit 0 > "$W/out" 2>&1; rc=$?
ok [ $rc = 1 ]; nok sent SEND
case_ "sepolia deposit: a depositor counter other than the record's stops, nothing sent"
spr; sp FERRATA_SEPOLIA_SEND=yes SG_COUNT=1 "$T/sepolia.sh" "$R" deposit 0 > "$W/out" 2>&1; rc=$?
ok [ $rc = 1 ]; nok sent SEND; ok grep -q "counter is 1" "$W/out"
case_ "sepolia deposit: a send that began without a receipt is not sent again"
spr; date > "$R/deposit-0.sending"; sp FERRATA_SEPOLIA_SEND=yes SG_COUNT=0 "$T/sepolia.sh" "$R" deposit 0 > "$W/out" 2>&1; rc=$?
ok [ $rc = 1 ]; nok sent SEND
case_ "sepolia deposit: the right counter sends once and clears its marker"
spr; sp FERRATA_SEPOLIA_SEND=yes SG_COUNT=0 "$T/sepolia.sh" "$R" deposit 0 > "$W/out" 2>&1; rc=$?
ok [ $rc = 0 ]; ok sent SEND; ok [ ! -f "$R/deposit-0.sending" ]

rm -rf "$W"
echo "guards: $pass passed, $fail failed"
[ "$fail" = 0 ]
