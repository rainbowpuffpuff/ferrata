#!/usr/bin/env bash
# e2e/seqnet's hook for the gas drop by default (RFP-022 U.5, Priv.7): `run.sh gasdrop NAME`. lez/crown/session.sh x runs
# it while the zone runs; drive.sh has made the vault on anvil, the user's private deposit and capture-1. Steps:
#   a. the operator: crown-driver crown-b --operator-only over capture-1 (Init, Register ETH, ImportRoot n)
#   b. the network's side, as deploy/testnet runs it: the gas-drop relayer `ferrata-relayer --gas-drops-only` (no
#      Ethereum key; its LEZ wallet a published BIP-39 test phrase; its public /0 and private /0 funded by seqnet-fund,
#      standing for the network's operator) and the test network's own gateway (deploy/testnet/host/bin/ferrata-gateway)
#      in front of the sequencer: /rpc, and /relayer/gas-drop (the terms, the guest, the inbox, each job's receipt
#      checked by `ferrata-gasdrop check-job` before it counts). The user's --sequencer is the gateway's /rpc. Two more
#      gateways stand for a network that publishes no relayer (404) and one that answers 429; a listener that never
#      answers stands for a timeout.
#   c. the user, with no gas-drop setting at all: first, from its own wallet, that it holds no native LEZ (public /0 and
#      private /0 both 0: this check fails if anything funded it); the failure paths, each a claim dry run that must stop
#      naming --no-gas-drop and never print the own-account fee payer: the relayer paused (503), its terms unreadable
#      (503), 429, a timeout; a network publishing none (404): the own-account line, as without a gas drop; a junk job
#      refused by the gateway before it counts. Then the opt-out sent: `claim --no-gas-drop --send --yes` stops for want
#      of native LEZ, nothing sent and nothing asked of the relayer. Then the claim with no setting: the guest fetched and
#      checked, the receipt proven and posted, the relayer's sealed credit awaited, the claim sent paid by the one-time
#      account; the user's private /0 still holds no native LEZ; the same job posted again once the relayer finished it is
#      answered 409 (the gateway remembers every N it admitted), nothing checked or counted.
#   d. the burn: its dry run names the drop's one-time account; the opt-out names the user's own account; with the drop
#      record set aside (no drop left), the burn stops while a relayer is published (it needs --no-gas-drop) or the
#      network answers 429, and takes the own account only where none is published (404); then the burn with no setting,
#      paid from the drop's remainder; the user still holds no native LEZ of its own.
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
  mkdir -p "$RUN/lez"; jl "$W/gasdrop.out" > "$RUN/lez/gasdrop-relayer.jsonl"
  for f in "$W"/gateway-*.out; do [ -f "$f" ] && cp "$f" "$RUN/lez/$(basename "$f" .out).log"; done
  records
  say "hook-gasdrop: $pass passed, $fail failed"
  [ "$fail" = 0 ]
}
free_port() { python3 -I -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])'; }
: > "$W/gw.pids"
trap 'kill $(cat "$W/gw.pids" 2>/dev/null) 2>/dev/null || true' EXIT
# A gateway (NAME PORT [ENV=VALUE...]) as deployed, its JSON-RPC limits raised for one local client (the rc1 tool reads
# the zone's whole history, in ranges larger than the public network's 50).
start_gw() {
  local name=$1 port=$2; shift 2
  env -u RISC0_DEV_MODE GATEWAY_LISTEN="127.0.0.1:$port" GATEWAY_UPSTREAM="$SEQ" FAUCET_STATE="$W/gw-faucet-$name" GATEWAY_MAX_RANGE=100000 \
    GATEWAY_READ_RATE=100000 GATEWAY_READ_BURST=100000 GATEWAY_SEND_RATE=100 GATEWAY_SEND_BURST=100 GATEWAY_SEND_GLOBAL_RATE=100 \
    GATEWAY_SEND_GLOBAL_BURST=100 GATEWAY_BYTES_RATE=1000000000 GATEWAY_BYTES_BURST=1000000000 GATEWAY_BYTES_GLOBAL_RATE=1000000000 \
    GATEWAY_BYTES_GLOBAL_BURST=1000000000 GATEWAY_BYTES_SHARE=1 GATEWAY_MAX_RESPONSE=100000000 GATEWAY_MAX_LIST=100000 \
    GATEWAY_PER_IP_INFLIGHT=32 GATEWAY_SOCKET_TIMEOUT=120 GASDROP_ADMITTED="$W/gasdrop/admitted-$name.sqlite" "$@" \
    python3 -I "$REPO/deploy/testnet/host/bin/ferrata-gateway" > "$W/gateway-$name.out" 2>&1 &
  echo $! >> "$W/gw.pids"
  for _ in $(seq 1 40); do curl -fsS -m 2 -o /dev/null "http://127.0.0.1:$port/rpc" 2>/dev/null && return 0; sleep 0.5; done
  return 1
}

# ---- a. the operator ---------------------------------------------------------------------------------------------------
operator_import

# ---- b. the network's relayer and gateways ---------------------------------------------------------------------------
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
CHECK_DEV=(GASDROP_CHECK_DEV=0); [ "$PROOFS" = dev ] && CHECK_DEV=(GASDROP_CHECK_DEV=1 RISC0_DEV_MODE=1)  # its checker as the zone's proofs
"${PROVE[@]}" "$BIN/ferrata-relayer" --gas-drops-only --eth-rpc "$RPC" --vault "$VAULT" --chain-id 11155111 --from-block "$FROM" \
  --state "$GD/state" --sequencer "$SEQ" --seed "$RL/seed.txt" --lez-wallet "$RL/wallet" --lez-password-file "$RL/lez.pass" \
  --lez-account "$RELAYER_LEZ" --bridge-account "$BRIDGE" --gasdrop-image-id "$GASDROP_ID" --drop-amount "$DROP_AMOUNT" \
  --drop-wait 30 "${DEVR[@]}" run --inbox "$GD/inbox" --poll-secs 2 > "$W/gasdrop.out" 2> "$W/gasdrop.err" &
echo $! >> "$W/gw.pids"
TERMS="$GD/terms.json"
write_terms() { # [EXTRA_JSON_FIELDS]
  printf '{"relayer": "%s", "imageId": "%s", "amount": "%s", "covers": ["claim", "burn"], "chainId": 11155111, "vault": "%s"%s}\n' \
    "$RELAYER_LEZ" "$GASDROP_ID" "$DROP_AMOUNT" "$VAULT" "${1:-}" > "$TERMS"
}
write_terms
GWP="$(free_port)"; P404="$(free_port)"; P429="$(free_port)"; PHANG="$(free_port)"
start_gw main "$GWP" GASDROP_INFO="$TERMS" GASDROP_INBOX="$GD/inbox" GASDROP_GUEST="$GASDROP_ELF" GASDROP_CHECK="$BIN/ferrata-gasdrop" \
  "${CHECK_DEV[@]}" || die "the gateway"
start_gw none "$P404" GASDROP_INFO= || die "the gateway publishing no relayer"
start_gw limited "$P429" GASDROP_INFO="$TERMS" GASDROP_INBOX="$GD/inbox" GASDROP_GET_BURST=0 || die "the gateway answering 429"
cat > "$W/hang.py" <<'PY'
import socket, sys
s = socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(("127.0.0.1", int(sys.argv[1]))); s.listen(16)
held = []
while True:
    held.append(s.accept()[0])  # accepted, never answered
PY
python3 -I "$W/hang.py" "$PHANG" > /dev/null 2>&1 &
echo $! >> "$W/gw.pids"
curl -fsS -m 5 "http://127.0.0.1:$GWP/relayer/gas-drop" > "$W/terms-served.json" 2>/dev/null
jq -e --arg r "$RELAYER_LEZ" '.relayer == $r and .relayFeeTo == $r and .inbox == "/relayer/gas-drop" and .accepting' "$W/terms-served.json" > /dev/null \
  && ok "the gateway publishes the network's relayer $RELAYER_LEZ (guest $GASDROP_ID, $DROP_AMOUNT native LEZ per deposit, for claim and burn; relay fee to it)" \
  || { bad "the gateway does not publish the relayer: $(head -c 300 "$W/terms-served.json") $(tail -n 3 "$W/gateway-main.out")"; finish; exit 1; }
GSEQ="http://127.0.0.1:$GWP/rpc"
fact relayer "$(jq -n --arg r "$RELAYER_LEZ" --arg g "$GASDROP_ID" --arg a "$DROP_AMOUNT" '{lez: $r, guest: $g, amount: $a, terms: "http://127.0.0.1:<gw>/relayer/gas-drop"}')"
say "the network: relayer $RELAYER_LEZ (--gas-drops-only, inbox $GD/inbox), gateway $GSEQ; stand-ins: 404 :$P404, 429 :$P429, no answer :$PHANG"

# ---- c. the user, no gas-drop setting ----------------------------------------------------------------------------------
unset FERRATA_GAS_DROP_INBOX FERRATA_GASDROP_ELF FERRATA_NO_GAS_DROP
F0=("$BIN/ferrata" --seed "$U/seed.txt" --eth-rpc "$RPC" --chain-id 11155111 --vault "$VAULT" --from-block "$FROM"
    --lez-wallet "$U/lez-wallet" --lez-password-file "$U/lez.pass" --bridge-account "$BRIDGE")
F=("${F0[@]}" --sequencer "$GSEQ")
# The user's own native LEZ, read by its own wallet from the zone: {"public_0", "private_0"}.
balances() {
  "$BIN/seqnet-fund" --balances --sequencer "$SEQ" --seed "$U/seed.txt" --lez-wallet "$U/lez-wallet" --lez-password-file "$U/lez.pass" \
    2> "$W/balances.err" | grep '^{' | tail -n 1
}
private_txs() { "$BIN/seqnet-txs" --sequencer "$SEQ" --home "$W/txs-gd" --bridge-account "$BRIDGE" --attest-account "$ATTEST" 2>/dev/null \
  | awk -F'\t' 'NR > 1 && $6 == "private"' | wc -l; }
B0="$(balances)"
fact user_before "${B0:-null}"
[ "$(echo "$B0" | jq -r '.public_0 + "/" + .private_0' 2>/dev/null)" = "0/0" ] \
  && ok "the user holds no native LEZ before its claim (its wallet reads public /0 and private /0 as 0; nothing funded it)" \
  || bad "the user's native LEZ before its claim: '${B0:-unread}' $(tail -n 2 "$W/balances.err" | tr '\n' ' ')"
u 06-scan "${F[@]}" scan || die "scan"
u 07-claim-held "${F[@]}" claim --bridge-program "$BRIDGE_BIN" && die "the claim was not held by its delay"
L7="$(grep -m1 '^fee payer:' "$RUN/user/07-claim-held.txt")"
case "$L7" in
  "fee payer: a fresh one-time private account, credited (sealed) by the network's gas-drop relayer $RELAYER_LEZ (published at http://127.0.0.1:$GWP/relayer/gas-drop"*"relay fee"*)
    ok "U.5/Priv.7: with no setting the claim's dry run names the network's relayer, read from the gateway, as fee payer, and that the relay fee is paid to it: ${L7:0:160}…" ;;
  *) bad "the claim's dry run does not name the network's relayer: ${L7:0:300} $(tail -n 2 "$RUN/user/07-claim-held.txt" | tr '\n' ' ' | cut -c1-300)" ;;
esac
# The failure paths: each stops the claim, names --no-gas-drop, and never prints the own-account fee payer (review F1).
stops() { # NAME WHAT GREP: the dry run in user/NAME.txt exited non-zero, says GREP and --no-gas-drop, no own-account line
  local f="$RUN/user/$1.txt"
  if grep -q -- "$3" "$f" && grep -q -- '--no-gas-drop' "$f" && ! grep -q '^fee payer: your private account' "$f"; then
    ok "Priv.7, $2: the claim stops naming --no-gas-drop, never the user's own account ($(grep -m1 -o -- "$3.\{0,60\}" "$f" | head -n 1))"
  else
    bad "$2: $(tail -n 3 "$f" | tr '\n' ' ' | cut -c1-400)"
  fi
}
write_terms ', "paused": "the drop account is being refilled"'
u 07a-claim-paused "${F[@]}" claim --bridge-program "$BRIDGE_BIN" && bad "the claim went on while the relayer was paused"
stops 07a-claim-paused "the relayer paused (503)" "paused"
printf '{"relayer": "%s", "amount"' "$RELAYER_LEZ" > "$TERMS"
u 07b-claim-unreadable "${F[@]}" claim --bridge-program "$BRIDGE_BIN" && bad "the claim went on with unreadable terms"
stops 07b-claim-unreadable "the terms unreadable (503)" "cannot be read"
write_terms
u 07c-claim-429 "${F0[@]}" --sequencer "http://127.0.0.1:$P429/rpc" claim --bridge-program "$BRIDGE_BIN" && bad "the claim went on after a 429"
stops 07c-claim-429 "the gateway answering 429" "429"
u 07d-claim-timeout "${F0[@]}" --sequencer "http://127.0.0.1:$PHANG/rpc" claim --bridge-program "$BRIDGE_BIN" && bad "the claim went on after a timeout"
stops 07d-claim-timeout "no answer (a timeout)" "could not be read"
u 07e-claim-none "${F0[@]}" --sequencer "http://127.0.0.1:$P404/rpc" claim --bridge-program "$BRIDGE_BIN"
grep -q '^fee payer: your private account that proves' "$RUN/user/07e-claim-none.txt" && grep -q 'held by your claim delay' "$RUN/user/07e-claim-none.txt" \
  && ok "a network publishing no relayer (404): the dry run names the user's own account and what it reveals, as without a gas drop" \
  || bad "the 404 path: $(grep -m1 '^fee payer:' "$RUN/user/07e-claim-none.txt" | cut -c1-200) $(tail -n 2 "$RUN/user/07e-claim-none.txt" | tr '\n' ' ' | cut -c1-200)"
# A junk job (a receipt that is no proof, its journal well formed and naming this relayer): refused before it counts.
python3 -I - "$RELAYER_LEZ" > "$W/junk.json" <<'PY'
import hashlib, json, sys
relayer = bytes.fromhex(sys.argv[1].removeprefix("0x"))
journal = hashlib.sha256(b"ferrata/gas-drop/v1").digest() + bytes(116 - 32) + b"\x5a" * 32 + bytes(32) + relayer
print(json.dumps({"kind": "gas-drop", "receipt": {"inner": {"Garbage": {}}, "journal": {"bytes": list(journal)}, "metadata": {}},
                  "oneTimeNpk": "0x" + "ab" * 32, "oneTimeVpk": "0x" + "cd" * 600}))
PY
JC="$(curl -s -o "$W/junk.out" -w '%{http_code}' -H 'content-type: application/json' --data-binary @"$W/junk.json" "http://127.0.0.1:$GWP/relayer/gas-drop")"
curl -fsS -m 5 "http://127.0.0.1:$GWP/relayer/gas-drop" > "$W/terms-after-junk.json" 2>/dev/null
[ "$JC" = 400 ] && grep -q 'does not check' "$W/junk.out" && [ -z "$(ls -A "$GD/inbox")" ] && jq -e '.pending == 0 and .accepting' "$W/terms-after-junk.json" > /dev/null \
  && ok "the gateway refuses a job whose receipt is no proof (400, checked by ferrata-gasdrop check-job) before it counts: inbox empty, still accepting" \
  || bad "the junk job: $JC $(head -c 200 "$W/junk.out")"
let_claim_delay_pass "$RUN/user/07-claim-held.txt"
# The opt-out sent: the user's own account pays, and it has nothing: refused before anything is proven or sent.
before_txs="$(private_txs)"
u 08a-claim-opt-out "${PROVE[@]}" "${F[@]}" claim --bridge-program "$BRIDGE_BIN" --no-gas-drop --send --yes \
  && bad "the opt-out claim went through with no native LEZ of the user's own"
if grep -q '^fee payer: your private account that proves' "$RUN/user/08a-claim-opt-out.txt" && [ "$(private_txs)" = "$before_txs" ] \
   && [ -z "$(ls -A "$GD/inbox")" ] && [ -z "$(jl "$W/gasdrop.out" | jq -c 'select(.event == "job-done")')" ]; then
  ok "Priv.7, the opt-out sent (--no-gas-drop --send --yes): the user's own account is the fee payer and, holding no native LEZ, the claim stops before sending ($(grep -m1 -o -i 'fee reserve.\{0,80\}\|holds less.\{0,60\}' "$RUN/user/08a-claim-opt-out.txt" | head -n 1)); nothing sent, nothing asked of the relayer"
else
  bad "the opt-out claim: $(grep -m1 '^fee payer:' "$RUN/user/08a-claim-opt-out.txt" | cut -c1-120) $(tail -n 3 "$RUN/user/08a-claim-opt-out.txt" | tr '\n' ' ' | cut -c1-300)"
fi
u 08-claim "${PROVE[@]}" "${F[@]}" claim --bridge-program "$BRIDGE_BIN" --send --yes \
  || { bad "the private claim with the network's drop: $(tail -n 4 "$RUN/user/08-claim.txt" | tr '\n' ' ' | cut -c1-400)"; tail -n 5 "$W/gasdrop.err"; finish; exit 1; }
wait_tx "$RUN/user/08-claim.txt"
ONE_TIME="$(grep -o -E 'gas drop: one-time account 0x[0-9a-f]{64}' "$RUN/user/08-claim.txt" | awk '{print $NF}' | head -n 1)"
JOB="$(grep -o -E 'went to http://127.0.0.1:[0-9]+/relayer/gas-drop \(job gas-drop-[0-9a-f]{64}\.json\)' "$RUN/user/08-claim.txt" | head -n 1)"
DROP="$(jl "$W/gasdrop.out" | jq -s -c '[.[] | select(.event == "job-done" and .result.kind == "gas-drop") | .result] | first // empty')"
if [ -n "$ONE_TIME" ] && [ -n "$JOB" ] && [ -n "$DROP" ] && [ "$(echo "$DROP" | jq -r .oneTimeAddress)" = "$ONE_TIME" ]; then
  ok "U.5/Priv.7: the private claim applied, its fee paid by one-time account $ONE_TIME from the network relayer's sealed credit (job checked and posted through the gateway: $JOB; relayer: $(echo "$DROP" | jq -c '{tx, amount, sealed, sends}'))"
else
  bad "the claim's drop: one-time '$ONE_TIME', job '$JOB', relayer '$DROP'"
fi
fact claim "$(jq -n --arg o "$ONE_TIME" --arg j "$JOB" --argjson d "${DROP:-null}" --arg tx "$(grep -o -E 'sent: [0-9a-fA-Fx]+' "$RUN/user/08-claim.txt" | tail -n 1 | awk '{print $2}')" '{oneTime: $o, job: $j, relayer: $d, tx: $tx}')"
jq -e 'length > 0' "$U/lez-wallet/ferrata-gas-drops.json" > /dev/null 2>&1 \
  && ok "the drop is recorded beside the wallet (ferrata-gas-drops.json: N -> the one-time account and the inbox URL)" \
  || bad "no drop record beside the wallet"
# The claim's own job posted again once the relayer has finished it (review round 2, R1): 409 before any check or cap.
DONE_JOB="$(ls "$GD/state/done/"gas-drop-*.json 2>/dev/null | head -n 1)"
JOBS_BEFORE="$(jl "$W/gasdrop.out" | jq -s '[.[] | select(.event == "job-done" or .event == "job-refused")] | length')"
if [ -n "$DONE_JOB" ]; then
  RC="$(curl -s -o "$W/repost.out" -w '%{http_code}' -H 'content-type: application/json' --data-binary @"$DONE_JOB" \
    "http://127.0.0.1:$GWP/relayer/gas-drop")"
  sleep 5
  if [ "$RC" = 409 ] && grep -q 'taken already' "$W/repost.out" && [ -z "$(ls -A "$GD/inbox")" ] \
     && [ "$(jl "$W/gasdrop.out" | jq -s '[.[] | select(.event == "job-done" or .event == "job-refused")] | length')" = "$JOBS_BEFORE" ]; then
    ok "R1: the claim's own job posted again after the relayer finished it: 409 (taken already), nothing checked, counted or handed over"
  else
    bad "the re-post of the finished job: $RC $(head -c 200 "$W/repost.out")"
  fi
else
  bad "no finished job in the relayer's done/ to post again"
fi
B1="$(balances)"
fact user_after_claim "${B1:-null}"
[ "$(echo "$B1" | jq -r '.public_0 + "/" + .private_0' 2>/dev/null)" = "0/0" ] \
  && ok "after the claim the user still holds no native LEZ of its own (public /0 and private /0: 0): only the drop could have paid" \
  || bad "the user's native LEZ after its claim: '${B1:-unread}'"

# ---- d. the burn ---------------------------------------------------------------------------------------------------
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
# No drop left (the record set aside): the burn stops while a relayer is published or the network cannot be read.
REC="$U/lez-wallet/ferrata-gas-drops.json"; mv "$REC" "$REC.aside"
u 11a-burn-no-drop "${F[@]}" burn --amount 0.001 --recipient "$FRESH" --bridge-program "$BRIDGE_BIN" && bad "the burn went on with no drop left"
grep -q 'add --no-gas-drop to confirm' "$RUN/user/11a-burn-no-drop.txt" \
  && ok "Priv.7, no drop left while the network publishes a relayer: the burn stops until --no-gas-drop confirms the own account" \
  || bad "the burn with no drop left: $(tail -n 2 "$RUN/user/11a-burn-no-drop.txt" | tr '\n' ' ' | cut -c1-300)"
u 11b-burn-429 "${F0[@]}" --sequencer "http://127.0.0.1:$P429/rpc" burn --amount 0.001 --recipient "$FRESH" --bridge-program "$BRIDGE_BIN" \
  && bad "the burn went on after a 429"
grep -q '429' "$RUN/user/11b-burn-429.txt" && grep -q -- '--no-gas-drop' "$RUN/user/11b-burn-429.txt" \
  && ok "Priv.7, no drop left and the network answering 429: the burn stops naming --no-gas-drop" \
  || bad "the burn after a 429: $(tail -n 2 "$RUN/user/11b-burn-429.txt" | tr '\n' ' ' | cut -c1-300)"
u 11c-burn-none "${F0[@]}" --sequencer "http://127.0.0.1:$P404/rpc" burn --amount 0.001 --recipient "$FRESH" --bridge-program "$BRIDGE_BIN"
grep -q '^fee payer: your private account that proves' "$RUN/user/11c-burn-none.txt" \
  && ok "no drop left where no relayer is published (404): the burn's dry run names the user's own account, as without a gas drop" \
  || bad "the burn with no relayer published: $(tail -n 2 "$RUN/user/11c-burn-none.txt" | tr '\n' ' ' | cut -c1-300)"
mv "$REC.aside" "$REC"
u 12-burn "${PROVE[@]}" "${F[@]}" burn --amount 0.001 --recipient "$FRESH" --bridge-program "$BRIDGE_BIN" --send --yes \
  || { bad "the private burn paid by the drop: $(tail -n 4 "$RUN/user/12-burn.txt" | tr '\n' ' ' | cut -c1-400)"; finish; exit 1; }
wait_tx "$RUN/user/12-burn.txt"
FEE_FROM="$(grep -o -E '^fee payer: the one-time account 0x[0-9a-f]{64}' "$RUN/user/12-burn.txt" | awk '{print $NF}' | head -n 1)"
[ "$FEE_FROM" = "$ONE_TIME" ] \
  && ok "U.5/Priv.7: the private burn applied, its fee paid by the drop's one-time account $ONE_TIME" \
  || bad "the burn's fee payer: '$FEE_FROM' (want $ONE_TIME); $(grep -m1 '^fee payer:' "$RUN/user/12-burn.txt" | cut -c1-200)"
fact burn "$(jq -n --arg tx "$(grep -o -E 'sent: [0-9a-fA-Fx]+' "$RUN/user/12-burn.txt" | tail -n 1 | awk '{print $2}')" --arg f "$FEE_FROM" '{tx: $tx, feeFrom: $f}')"
B2="$(balances)"
fact user_after_burn "${B2:-null}"
[ "$(echo "$B2" | jq -r '.public_0 + "/" + .private_0' 2>/dev/null)" = "0/0" ] \
  && ok "after the burn the user still holds no native LEZ of its own: the drop's remainder paid it" \
  || bad "the user's native LEZ after its burn: '${B2:-unread}'"
u 13-recover "${F[@]}" recover || die "recover"
finish
