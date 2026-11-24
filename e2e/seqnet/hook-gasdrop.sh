#!/usr/bin/env bash
# e2e/seqnet's hook for the gas drop by default (RFP-022 U.5, Priv.7): `run.sh gasdrop NAME`. lez/crown/session.sh x runs
# it while the zone runs; drive.sh has made the vault on anvil, the user's private deposit and capture-1. Steps:
#   a. the operator: crown-driver crown-b --operator-only over capture-1 (Init, Register ETH, ImportRoot n)
#   b. the network's side, as deploy/testnet runs it: the gas-drop relayer `ferrata-relayer --gas-drops-only` (no
#      Ethereum key; its LEZ wallet a published BIP-39 test phrase; its public /0 and private /0 funded by seqnet-fund,
#      standing for the network's operator) and the test network's own gateway (deploy/testnet/host/bin/ferrata-gateway)
#      in front of the sequencer: /rpc, and /relayer/gas-drop (the terms, the guest, the inbox the relayer reads). The
#      user's --sequencer is the gateway's /rpc, as on the test network.
#   c. the user, with no gas-drop setting at all and no native LEZ of its own (seqnet-fund never pays the user): the
#      claim's dry run names the network's relayer, read from the gateway, as fee payer; the private claim (the tool
#      fetches the guest, checks its pinned image id, proves the receipt, posts the job to the gateway, waits for the
#      relayer's sealed credit, sends the claim paid by the one-time account); then the private burn, its fee paid by
#      what the credit left in that account. Both applied on the zone: nothing else could have paid their fees.
#   d. the opt-out: with --no-gas-drop the burn's dry run names the user's own private account.
# PASS/FAIL lines in $RUN/gasdrop.md, the facts in $RUN/gasdrop.json.
set -uo pipefail
. "$SEQNET_ENV"
. "$REPO/e2e/seqnet/common.sh"
RELAYER_PHRASE="legal winner thank year wave sausage worth useful legal winner thank yellow"   # BIP-39 test vector
G7="$RUN/gasdrop.md"; GJ="$W/gasdrop.jsonl"; : > "$GJ"
printf '# seqnet gasdrop: the network gas-drop relayer by default (U.5, Priv.7), on a running LEZ sequencer\n\n' > "$G7"
pass=0; fail=0
ok() { echo "- PASS $*" | tee -a "$G7"; pass=$((pass + 1)); say "PASS $*"; }
bad() { echo "- FAIL $*" | tee -a "$G7"; fail=$((fail + 1)); say "FAIL $*"; }
fact() { jq -n -c --arg k "$1" --argjson v "$2" '{($k): $v}' >> "$GJ"; }
jl() { grep '^{' "$1" 2>/dev/null; }
finish() {
  jq -s 'add // {}' "$GJ" > "$RUN/gasdrop.json"
  printf '\n%s passed, %s failed\n' "$pass" "$fail" >> "$G7"
  mkdir -p "$RUN/lez"; jl "$W/gasdrop.out" > "$RUN/lez/gasdrop-relayer.jsonl"; cp "$W/gateway.out" "$RUN/lez/gateway.log" 2>/dev/null
  records
  say "hook-gasdrop: $pass passed, $fail failed"
  [ "$fail" = 0 ]
}

# ---- a. the operator ---------------------------------------------------------------------------------------------------
operator_import

# ---- b. the network's relayer and gateway ------------------------------------------------------------------------------
RL="$W/relayer-lez"; mkdir -p "$RL"
printf '%s\n' "$RELAYER_PHRASE" > "$RL/seed.txt"; printf 'seqnet-relayer\n' > "$RL/lez.pass"; chmod 600 "$RL/seed.txt" "$RL/lez.pass"
"${PROVE[@]}" "$BIN/seqnet-fund" --sequencer "$SEQ" --seed "$RL/seed.txt" --lez-wallet "$RL/wallet" --lez-password-file "$RL/lez.pass" \
  --faucet-home "$W/faucet-relayer" > "$RL/fund.out" 2> "$RL/fund.err" || { tail -n 20 "$RL/fund.err"; die "seqnet-fund (the relayer)"; }
RELAYER_LEZ="$(jl "$RL/fund.out" | jq -r '.to_public_0_hex // empty' | head -n 1)"
[ -n "$RELAYER_LEZ" ] || die "the relayer's public account"
GASDROP_ELF="$BIN/gasdrop.bin"
GASDROP_ID="$(r0vm --id --elf "$GASDROP_ELF" 2>/dev/null | tail -n 1)"; case "$GASDROP_ID" in 0x*) ;; *) GASDROP_ID="0x$GASDROP_ID" ;; esac
DROP_AMOUNT=200000000
GD="$W/gasdrop"; mkdir -p "$GD/inbox" "$GD/state"
DEVR=(); [ "$PROOFS" = dev ] && DEVR=(--accept-dev-mode-receipts)
"${PROVE[@]}" "$BIN/ferrata-relayer" --gas-drops-only --eth-rpc "$RPC" --vault "$VAULT" --chain-id 11155111 --from-block "$FROM" \
  --state "$GD/state" --sequencer "$SEQ" --seed "$RL/seed.txt" --lez-wallet "$RL/wallet" --lez-password-file "$RL/lez.pass" \
  --lez-account "$RELAYER_LEZ" --bridge-account "$BRIDGE" --gasdrop-image-id "$GASDROP_ID" --drop-amount "$DROP_AMOUNT" \
  --drop-wait 30 "${DEVR[@]}" run --inbox "$GD/inbox" --poll-secs 2 > "$W/gasdrop.out" 2> "$W/gasdrop.err" &
DPID=$!
GWP="$(python3 -I -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')"
printf '{"relayer": "%s", "imageId": "%s", "amount": "%s", "covers": ["claim", "burn"]}\n' "$RELAYER_LEZ" "$GASDROP_ID" "$DROP_AMOUNT" > "$GD/terms.json"
# The test network's gateway as deployed, its JSON-RPC limits raised for one local client (the tool reads the zone's whole
# history; the rc1 tool reads it in ranges larger than the public network's 50).
env GATEWAY_LISTEN="127.0.0.1:$GWP" GATEWAY_UPSTREAM="$SEQ" GASDROP_INFO="$GD/terms.json" GASDROP_INBOX="$GD/inbox" \
  GASDROP_GUEST="$GASDROP_ELF" FAUCET_STATE="$W/gw-faucet" GATEWAY_MAX_RANGE=100000 GATEWAY_READ_RATE=100000 \
  GATEWAY_READ_BURST=100000 GATEWAY_SEND_RATE=100 GATEWAY_SEND_BURST=100 GATEWAY_SEND_GLOBAL_RATE=100 GATEWAY_SEND_GLOBAL_BURST=100 \
  GATEWAY_BYTES_RATE=1000000000 GATEWAY_BYTES_BURST=1000000000 GATEWAY_BYTES_GLOBAL_RATE=1000000000 \
  GATEWAY_BYTES_GLOBAL_BURST=1000000000 GATEWAY_BYTES_SHARE=1 GATEWAY_MAX_RESPONSE=100000000 GATEWAY_MAX_LIST=100000 \
  GATEWAY_PER_IP_INFLIGHT=32 GATEWAY_SOCKET_TIMEOUT=120 \
  python3 -I "$REPO/deploy/testnet/host/bin/ferrata-gateway" > "$W/gateway.out" 2>&1 &
GWPID=$!
trap 'kill $DPID $GWPID 2>/dev/null || true' EXIT
for _ in $(seq 1 40); do curl -fsS -m 2 "http://127.0.0.1:$GWP/relayer/gas-drop" > "$W/terms-served.json" 2>/dev/null && break; sleep 0.5; done
jq -e '.relayer and .inbox == "/relayer/gas-drop"' "$W/terms-served.json" > /dev/null \
  && ok "the gateway publishes the network's relayer $RELAYER_LEZ (guest $GASDROP_ID, $DROP_AMOUNT native LEZ per deposit, for claim and burn)" \
  || { bad "the gateway does not publish the relayer: $(head -c 300 "$W/terms-served.json") $(tail -n 3 "$W/gateway.out")"; finish; exit 1; }
GSEQ="http://127.0.0.1:$GWP/rpc"
fact relayer "$(jq -n --arg r "$RELAYER_LEZ" --arg g "$GASDROP_ID" --arg a "$DROP_AMOUNT" --arg u "http://127.0.0.1:<gw>/relayer/gas-drop" '{lez: $r, guest: $g, amount: $a, terms: $u}')"
say "the network: relayer $RELAYER_LEZ (--gas-drops-only, inbox $GD/inbox), gateway $GSEQ"

# ---- c. the user, no gas-drop setting, no native LEZ ---------------------------------------------------------------
# No FERRATA_GAS_DROP_INBOX, FERRATA_GASDROP_ELF or FERRATA_NO_GAS_DROP in the environment; no --relayer.
unset FERRATA_GAS_DROP_INBOX FERRATA_GASDROP_ELF FERRATA_NO_GAS_DROP
F=("$BIN/ferrata" --seed "$U/seed.txt" --eth-rpc "$RPC" --chain-id 11155111 --vault "$VAULT" --from-block "$FROM"
   --sequencer "$GSEQ" --lez-wallet "$U/lez-wallet" --lez-password-file "$U/lez.pass" --bridge-account "$BRIDGE")
u 06-scan "${F[@]}" scan || die "scan"
u 07-claim-held "${F[@]}" claim --bridge-program "$BRIDGE_BIN" && die "the claim was not held by its delay"
grep -q "^fee payer: a fresh one-time private account, credited (sealed) by the network's gas-drop relayer $RELAYER_LEZ (published at http://127.0.0.1:$GWP/relayer/gas-drop" "$RUN/user/07-claim-held.txt" \
  && ok "U.5/Priv.7: with no setting the claim's dry run names the network's relayer, read from the gateway, as fee payer: $(grep -m1 '^fee payer:' "$RUN/user/07-claim-held.txt" | cut -c1-160)…" \
  || bad "the claim's dry run does not name the network's relayer: $(grep -m1 '^fee payer:' "$RUN/user/07-claim-held.txt" | cut -c1-300) $(tail -n 2 "$RUN/user/07-claim-held.txt" | tr '\n' ' ' | cut -c1-300)"
let_claim_delay_pass "$RUN/user/07-claim-held.txt"
before_user="$(jl "$RL/fund.out" | jq -s 'length')"
u 08-claim "${PROVE[@]}" "${F[@]}" claim --bridge-program "$BRIDGE_BIN" --send --yes \
  || { bad "the private claim with the network's drop: $(tail -n 4 "$RUN/user/08-claim.txt" | tr '\n' ' ' | cut -c1-400)"; tail -n 5 "$W/gasdrop.err"; finish; exit 1; }
wait_tx "$RUN/user/08-claim.txt"
ONE_TIME="$(grep -o -E 'gas drop: one-time account 0x[0-9a-f]{64}' "$RUN/user/08-claim.txt" | awk '{print $NF}' | head -n 1)"
JOB="$(grep -o -E 'went to http://127.0.0.1:[0-9]+/relayer/gas-drop \(job gas-drop-[0-9a-f]{64}\.json\)' "$RUN/user/08-claim.txt" | head -n 1)"
DROP="$(jl "$W/gasdrop.out" | jq -s -c '[.[] | select(.event == "job-done" and .result.kind == "gas-drop") | .result] | first // empty')"
if [ -n "$ONE_TIME" ] && [ -n "$JOB" ] && [ -n "$DROP" ] && [ "$(echo "$DROP" | jq -r .oneTimeAddress)" = "$ONE_TIME" ]; then
  ok "U.5/Priv.7: the private claim applied, its fee paid by one-time account $ONE_TIME from the network relayer's sealed credit (job posted to the gateway: $JOB; relayer: $(echo "$DROP" | jq -c '{tx, amount, sealed, sends}'))"
else
  bad "the claim's drop: one-time '$ONE_TIME', job '$JOB', relayer '$DROP'"
fi
fact claim "$(jq -n --arg o "$ONE_TIME" --arg j "$JOB" --argjson d "${DROP:-null}" --arg tx "$(grep -o -E 'sent: [0-9a-fA-Fx]+' "$RUN/user/08-claim.txt" | tail -n 1 | awk '{print $2}')" '{oneTime: $o, job: $j, relayer: $d, tx: $tx}')"
jq -e 'length > 0' "$U/lez-wallet/ferrata-gas-drops.json" > /dev/null 2>&1 \
  && ok "the drop is recorded beside the wallet (ferrata-gas-drops.json: N -> the one-time account and the inbox URL)" \
  || bad "no drop record beside the wallet"
[ "$(jl "$RL/fund.out" | jq -s 'length')" = "$before_user" ] && ok "nothing funded the user: seqnet-fund ran for the relayer only" || bad "seqnet-fund ran again"

u 09-recover "${F[@]}" recover || die "recover"
FRESH="$(cast wallet new --json | jq -r '.[0].address')"
u 10-burn-dry-run "${F[@]}" burn --amount 0.001 --recipient "$FRESH" --bridge-program "$BRIDGE_BIN" || die "burn dry run"
grep -q "^fee payer: the one-time account $ONE_TIME of a gas drop" "$RUN/user/10-burn-dry-run.txt" \
  && ok "U.5/Priv.7: with no setting the burn's dry run names the drop's one-time account as fee payer: $(grep -m1 '^fee payer:' "$RUN/user/10-burn-dry-run.txt" | cut -c1-200)…" \
  || bad "the burn's dry run: $(grep -m1 '^fee payer:' "$RUN/user/10-burn-dry-run.txt" | cut -c1-300)"
u 11-burn-dry-run-opt-out "${F[@]}" burn --amount 0.001 --recipient "$FRESH" --bridge-program "$BRIDGE_BIN" --no-gas-drop || die "burn dry run (opt-out)"
grep -q '^fee payer: your private account that proves' "$RUN/user/11-burn-dry-run-opt-out.txt" \
  && ok "Priv.7: --no-gas-drop (the opt-out) names the user's own private account and what its funding reveals" \
  || bad "the opt-out's dry run: $(grep -m1 '^fee payer:' "$RUN/user/11-burn-dry-run-opt-out.txt" | cut -c1-300)"
u 12-burn "${PROVE[@]}" "${F[@]}" burn --amount 0.001 --recipient "$FRESH" --bridge-program "$BRIDGE_BIN" --send --yes \
  || { bad "the private burn paid by the drop: $(tail -n 4 "$RUN/user/12-burn.txt" | tr '\n' ' ' | cut -c1-400)"; finish; exit 1; }
wait_tx "$RUN/user/12-burn.txt"
FEE_FROM="$(grep -o -E '^fee payer: the one-time account 0x[0-9a-f]{64}' "$RUN/user/12-burn.txt" | awk '{print $NF}' | head -n 1)"
[ "$FEE_FROM" = "$ONE_TIME" ] \
  && ok "U.5/Priv.7: the private burn applied, its fee paid by the drop's one-time account $ONE_TIME (the user holds no native LEZ elsewhere)" \
  || bad "the burn's fee payer: '$FEE_FROM' (want $ONE_TIME); $(grep -m1 '^fee payer:' "$RUN/user/12-burn.txt" | cut -c1-200)"
fact burn "$(jq -n --arg tx "$(grep -o -E 'sent: [0-9a-fA-Fx]+' "$RUN/user/12-burn.txt" | tail -n 1 | awk '{print $2}')" --arg f "$FEE_FROM" '{tx: $tx, feeFrom: $f}')"
u 13-recover "${F[@]}" recover || die "recover"
finish
