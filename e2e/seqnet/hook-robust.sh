#!/usr/bin/env bash
# e2e/seqnet's hook for the robustness run (RFP-022 U.03, R.07): lez/crown/session.sh x runs it while the zone (the LEZ
# sequencer and the Logos Blockchain node) runs; drive.sh has made the vault on anvil, the user's two deposits and
# capture-1. The user's round trip of hook-cli.sh, with:
#   a. U.03, the relayer's root import on the running sequencer: the operator's crown-b --operator-only yes
#      --import-roots no (the stand-in committee's finality update for capture-1's block, bridge Init, Register ETH; no
#      ImportRoot); then ferrata-relayer itself, --sequencer --import-roots (its own LEZ wallet: a published BIP-39 test
#      phrase, never a wallet; its public account /0 paid native LEZ by seqnet-fund --public-only), run as a daemon until
#      root(cid, 2) holds its import, then stopped with SIGTERM (its clean stop line)
#   b. U.05, the gas drop in the relayer daemon: ferrata-relayer run with --gasdrop-image-id and --drop-amount (its
#      private account /0 funded by seqnet-fund); every claim of the user names that relayer and its inbox
#      (--gas-drop-inbox): the tool makes a one-time private account, proves the gas-drop receipt over the claim's
#      witness, hands the job (the receipt and the account's public keys) to the relayer's inbox, waits for the
#      relayer's credit sealed to that account and sends the claim with that account as fee payer (its dry run must
#      sweep the credit); no stand-in pays a claim's fee. One drop per deposit; the daemon stopped by SIGTERM after
#      the claims. The burn's fee: the stand-in (seqnet-fund) still pays it (burns with a drop are not built)
#   c. R.07, a claim killed between its proof and its sending: `ferrata claim --send --yes` with its sequencer endpoint
#      behind e2e/netfault/holdproxy.py, which traps the claim's sendTransaction (built and proven by LEZ's wallet,
#      never forwarded; the gas drop's credit already applied); the tool SIGKILLed; the zone holds no new transaction
#      and the deposit is unclaimed; the same command again, straight to the sequencer: the drop an earlier run
#      recorded reused (no second job), the deposit claimed once (the second deposit, with its own drop, after it)
#   d. R.07, a burn killed after its sending: `ferrata burn --amount 0.001` (of the 0.002 the two claims minted) with
#      holdproxy holding its sendTransaction's answer (the burn reaches the sequencer, the tool never learns it); the
#      tool SIGKILLed; the burn applied; the same command again: the tool finds the burn its journal names on the zone
#      (`<LEZ wallet>/ferrata-burns.json`, cli/src/burns.rs) and does not burn again (the private holding 0.001 less,
#      once); the journal's entry resolved
#   e. the checkpoint, the release through the relayer, recover and status (as hook-cli.sh)
#   f. R.06, the gas drop resumed on the running sequencer (the drops daemon reaches the sequencer through a second
#      holdproxy): the first drop's credit held unsent and the daemon SIGKILLed; its next send answered 503 by the
#      endpoint and forwarded to the sequencer 35 s later (`late`), after the daemon sent it a third time on the same
#      account state: two credits of one drop reach LEZ, and one applies (the second review's M1); the second drop's
#      credit forwarded with its answer held and the daemon SIGKILLed, then booked after the restart from LEZ (its drop
#      account's nonce moved) with nothing sent again. Each claim's dry run claims exactly one pending credit.
#   g. the claim's gas drop asked of another relayer (review M3): a claim first naming a relayer that never answers
#      (an inbox nobody serves) stops after its wait; the same deposit's claim naming the real relayer reuses the
#      one-time account and hands that relayer a receipt of its own
#   h. R.07, a burn whose send is answered 503 and forwarded to the sequencer 90 s later (`late`: the tool never learns
#      it was sent), then a burn with other parameters (another recipient): it takes a fresh counter, never the
#      interrupted burn's (the final review's H1 of 22 November 2026: until 22 November 2026 it took the interrupted burn's counter, and the
#      bridge program applied two notes with one tag, one of them never releasable), and is applied; the late
#      transaction, built on the private account's state the second burn spent, is refused; the interrupted burn's
#      counter stays reserved in the journal
#   i. R.07, the deposit leg (drive.sh, deposit-leg.sh): the second deposit's tool SIGKILLed with its transaction trapped,
#      then with it mined and the answer held; the same command again finds it and deposits nothing
#   j. R.06/R.07, after the import and before the claim: the relayer's ImportRoot forwarded to the sequencer with its
#      answer held and the daemon SIGKILLed; started again, it books the import from LEZ (one ImportRoot)
#   k. R.06, the attestor, the prover and its client (robust-r06.sh): the checkpoint over the real attestor's own
#      attestation, the attestor SIGKILLed inside rounds and cut off from its node and its LEZ node meanwhile; proven
#      through the prover client, whose prover dies mid-proof and which is SIGKILLed mid-proof; one job, one candidate
#   l. R.07, the release leg: the relayer killed with the release built and its transaction trapped, then with it mined
#      and the answer held; started again, it books it from the chain (one Release)
# SEQNET_GAS_DROP=0: the claims' fees from the stand-in (seqnet-fund) instead of the relayer's drops (b, f, g skipped).
# The daemons' standard output (their JSON lines, and LEZ's wallet's plain sync lines) goes to the work directory; the
# records keep the JSON lines only.
# PASS/FAIL lines in $RUN/robust.md, the facts in $RUN/robust.json.
set -uo pipefail
. "$SEQNET_ENV"
. "$REPO/e2e/seqnet/common.sh"
. "$REPO/e2e/seqnet/late-credit.sh"   # late_credit_verdict (f)
. "$REPO/e2e/seqnet/robust-r06.sh"     # the attestor, the prover and its client, the release leg (k, l)
F=("$BIN/ferrata" --seed "$U/seed.txt" --eth-rpc "$RPC" --chain-id 11155111 --vault "$VAULT" --from-block "$FROM"
   --sequencer "$SEQ" --lez-wallet "$U/lez-wallet" --lez-password-file "$U/lez.pass" --bridge-account "$BRIDGE")
RELAYER_PHRASE="legal winner thank year wave sausage worth useful legal winner thank yellow"   # BIP-39 test vector
R7="$RUN/robust.md"; RJ="$W/robust.jsonl"; : > "$RJ"
printf '# seqnet robust: U.03 and R.07 on a running LEZ sequencer\n\n' > "$R7"
pass=0; fail=0
ok() { echo "- PASS $*" | tee -a "$R7"; pass=$((pass + 1)); say "PASS $*"; }
bad() { echo "- FAIL $*" | tee -a "$R7"; fail=$((fail + 1)); say "FAIL $*"; }
fact() { jq -n -c --arg k "$1" --argjson v "$2" '{($k): $v}' >> "$RJ"; }
# i. the deposit leg's verdicts, from drive.sh (deposit-leg.sh)
if [ -s "$W/deposit-leg.md" ]; then
  cat "$W/deposit-leg.md" >> "$R7"
  pass=$((pass + $(grep -c '^- PASS' "$W/deposit-leg.md"))); fail=$((fail + $(grep -c '^- FAIL' "$W/deposit-leg.md")))
  cat "$W/deposit-leg.json" >> "$RJ" 2>/dev/null
else
  bad "R.07 deposit: no record of the interrupted deposit (drive.sh's deposit-leg.sh)"
fi
# The daemons' JSON lines (LEZ's wallet prints its sync, plain text, to the same standard output).
jl() { grep '^{' "$1" 2>/dev/null; }
# The zone's privacy-preserving transactions (the user's claims and burns are; the operator's, the relayer's and any
# clock update are public), from seqnet-txs (LEZ's wallet library, a throwaway home and no keys).
zone_txs() { "$BIN/seqnet-txs" --sequencer "$SEQ" --home "$W/txs-wallet-r7" --bridge-account "$BRIDGE" --attest-account "$ATTEST" 2>/dev/null \
  | awk -F'\t' 'NR > 1 && $6 == "private"' | wc -l; }

# ---- the proxy in front of the sequencer, for the user's interrupted commands ----------------------------------------
PX="$W/holdproxy"; mkdir -p "$PX"; echo pass > "$PX/ctl"
python3 -I "$REPO/e2e/netfault/holdproxy.py" --upstream "$SEQ" --port-file "$PX/port" --ctl "$PX/ctl" --log "$PX/log.jsonl" \
  > "$PX/out" 2>&1 &
PXPID=$!
trap 'kill $PXPID 2>/dev/null || true' EXIT
for _ in $(seq 1 40); do [ -s "$PX/port" ] && break; sleep 0.25; done
SEQ_PROXY="http://127.0.0.1:$(cat "$PX/port")"
FP=("$BIN/ferrata" --seed "$U/seed.txt" --eth-rpc "$RPC" --chain-id 11155111 --vault "$VAULT" --from-block "$FROM"
    --sequencer "$SEQ_PROXY" --lez-wallet "$U/lez-wallet" --lez-password-file "$U/lez.pass" --bridge-account "$BRIDGE")
hits_of() { [ -f "$1/ctl.hit" ] && wc -l < "$1/ctl.hit" || echo 0; }
wait_hit_of() { local d=$1 before=$2 i; for i in $(seq 1 1200); do [ "$(hits_of "$d")" -gt "$before" ] && return 0; sleep 0.5; done; return 1; }
hits() { hits_of "$PX"; }
wait_hit() { wait_hit_of "$PX" "$1"; }
# A second proxy in front of the sequencer, for the relayer's gas-drop daemon (f).
PR="$W/holdproxy-relayer"; mkdir -p "$PR"; echo pass > "$PR/ctl"
python3 -I "$REPO/e2e/netfault/holdproxy.py" --upstream "$SEQ" --port-file "$PR/port" --ctl "$PR/ctl" --log "$PR/log.jsonl" \
  > "$PR/out" 2>&1 &
PRPID=$!
trap 'kill $PXPID $PRPID $(cat "$W/proxies.pid" 2>/dev/null) 2>/dev/null; at_stop 2>/dev/null; true' EXIT
for _ in $(seq 1 40); do [ -s "$PR/port" ] && break; sleep 0.25; done
SEQ_RELAYER="http://127.0.0.1:$(cat "$PR/port")"

# ---- a. U.03: the operator without ImportRoot, then the relayer's own import on the sequencer --------------------------
say "operator: crown-driver crown-b --operator-only yes --import-roots no over capture-1"
"$ZONE_BIN/crown-driver" crown-b "${D[@]}" --identity "$SA" --capture "$VOL/inputs/capture-1" --private fake \
  --operator-only yes --import-roots no --session-out "$W/session-b.json" --notes-out "$W/notes.json" \
  > "$RUN/lez/crown-b-operator.log" 2>&1 || { tail -n 20 "$RUN/lez/crown-b-operator.log"; die "crown-b --operator-only --import-roots no"; }
grep -E '^\[crown-b\]' "$RUN/lez/crown-b-operator.log" | tail -n 4 | tee -a "$RUN/run.log"
RL="$W/relayer-lez"; mkdir -p "$RL"
printf '%s\n' "$RELAYER_PHRASE" > "$RL/seed.txt"; printf 'seqnet-relayer\n' > "$RL/lez.pass"; chmod 600 "$RL/seed.txt" "$RL/lez.pass"
"${PROVE[@]}" "$BIN/seqnet-fund" --sequencer "$SEQ" --seed "$RL/seed.txt" --lez-wallet "$RL/wallet" --lez-password-file "$RL/lez.pass" \
  --faucet-home "$W/faucet-relayer" > "$RL/fund.out" 2> "$RL/fund.err" || { tail -n 20 "$RL/fund.err"; die "seqnet-fund (the relayer)"; }
RELAYER_LEZ="$(grep '^{' "$RL/fund.out" | jq -r '.to_public_0_hex' | head -n 1)"
grep '^{' "$RL/fund.out" > "$RUN/lez/relayer-fund.jsonl"
say "the relayer's LEZ public account $RELAYER_LEZ and its private account /0 (its gas drops' source) funded (seqnet-fund)"
RD_LOG="$W/relayer-daemon.out"; : > "$RD_LOG"
start_import() { # the import daemon, through the relayer's proxy (j)
  "$BIN/ferrata-relayer" --eth-rpc "$RPC" --vault "$VAULT" --chain-id 11155111 --unlocked "$RELAYER" --from-block "$FROM" \
    --state "$W/relayer" --sequencer "$SEQ_RELAYER" --seed "$RL/seed.txt" --lez-wallet "$RL/wallet" --lez-password-file "$RL/lez.pass" \
    --lez-account "$RELAYER_LEZ" --bridge-account "$BRIDGE" --import-roots --import-every 0 \
    run --inbox "$W/relayer-inbox-imports" --poll-secs 5 >> "$RD_LOG" 2>> "$W/relayer-daemon.err" &
  RD=$!
}
# j. the import's sendTransaction forwarded to the sequencer with its answer held; the daemon SIGKILLed meanwhile
pr_lines0="$(wc -l < "$PR/log.jsonl" 2>/dev/null || echo 0)"
hi0="$(hits_of "$PR")"; echo "hold sendTransaction 40" > "$PR/ctl"
start_import
if wait_hit_of "$PR" "$hi0"; then
  sleep 3; kill -9 "$RD" 2>/dev/null; wait "$RD" 2>/dev/null
  inflight_rec="$(jq -c '{leaf_count, tx}' "$W/relayer/root-import-inflight.json" 2>/dev/null)"
  fact importInflightAtKill "${inflight_rec:-null}"
  [ -n "$inflight_rec" ] && ok "R.06/R.07 import: the relayer SIGKILLed with its ImportRoot at the sequencer and the answer held; in flight in its state: $inflight_rec" \
    || bad "R.06/R.07 import: nothing in flight when the relayer was killed"
else
  kill -9 "$RD" 2>/dev/null; bad "R.06/R.07 import: the relayer's import never reached its proxy ($(jl "$RD_LOG" | tail -n 2 | jq -c '{event, reason}' 2>/dev/null | tr '\n' ' '))"
fi
sleep 45   # the held answer delivered to nobody; the import applied in the zone's blocks
echo pass > "$PR/ctl"
start_import
imported=""""
for _ in $(seq 1 180); do
  imported="$(jl "$RD_LOG" | jq -c 'select(.event == "root-imported" and .applied == true)' 2>/dev/null | head -n 1)"
  [ -n "$imported" ] && break
  kill -0 "$RD" 2>/dev/null || break
  sleep 5
done
kill -TERM "$RD" 2>/dev/null; wait "$RD" 2>/dev/null
jl "$RD_LOG" > "$RUN/lez/relayer-daemon.jsonl"
stopline="$(jl "$RD_LOG" | jq -c 'select(.event == "stop")' 2>/dev/null | tail -n 1)"
imports_sent="$(tail -n +$((pr_lines0 + 1)) "$PR/log.jsonl" | jq -s '[.[] | select(.method == "sendTransaction" and (.action | test("^forwarded")))] | length')"
import_roots="$("$BIN/seqnet-txs" --sequencer "$SEQ" --home "$W/txs-wallet-j" --bridge-account "$BRIDGE" --attest-account "$ATTEST" 2>/dev/null | awk -F'\t' 'NR > 1 && $9 == "ImportRoot"' | wc -l)"
fact importAfterKill "$(jq -n -c --argjson s "$imports_sent" --argjson r "$import_roots" '{sendsForwarded: $s, importRootsOnZone: $r}')"
if [ "$(echo "$imported" | jq -r '.recovered // false' 2>/dev/null)" = true ] && [ "$imports_sent" = 1 ] && [ "$import_roots" = 1 ]; then
  ok "R.06/R.07 import: started again, the relayer found its import applied on LEZ and booked it (recovered), sending nothing: $imports_sent ImportRoot sent across the kill, $import_roots on the zone"
else
  bad "R.06/R.07 import: recovered $(echo "$imported" | jq -c '.recovered' 2>/dev/null), sends $imports_sent, ImportRoot on the zone $import_roots"
fi
if [ -n "$imported" ]; then
  ok "U.03: ferrata-relayer imported root(cid, $(echo "$imported" | jq -r .leafCount)) on the running sequencer (tx $(echo "$imported" | jq -r .tx), recovered $(echo "$imported" | jq -r '.recovered // false')); $(jl "$RD_LOG" | jq -s 'map(select(.event == "root-import-unconfirmed" or .event == "root-import-pending")) | length') rounds before it was applied"
  fact rootImport "$imported"
else
  bad "U.03: no root-imported line from the relayer daemon ($(jl "$RD_LOG" | tail -n 3 | jq -c '{event, reason}' 2>/dev/null | tr '\n' ' '); $(tail -n 3 "$W/relayer-daemon.err"))"
  die "the relayer's root import"
fi
if [ -n "$stopline" ] && [ "$(echo "$stopline" | jq -r .reason)" = SIGTERM ]; then ok "U.03: SIGTERM: a clean stop line ($stopline)"; else bad "U.03: no clean stop line on SIGTERM ($stopline)"; fi
[ ! -e "$W/relayer/root-import-inflight.json" ] && ok "U.03: nothing left in flight in the relayer's state" || bad "U.03: an import left in flight"

# ---- k. R.06: the attestor beside the zone's sequencer, killed and cut off while the user's commands run -------------
at_start
at_chaos > "$AT/chaos.log" 2>&1 &
CHAOS=$!

# ---- b. U.05: the relayer daemon's gas drops ---------------------------------------------------------------------------
GAS_DROP="${SEQNET_GAS_DROP:-1}"
if [ "$GAS_DROP" = 1 ]; then
GASDROP_ELF="$BIN/gasdrop.bin"
GASDROP_ID="$(r0vm --id --elf "$GASDROP_ELF" 2>/dev/null | tail -n 1)"
[ -n "$GASDROP_ID" ] || die "the gas-drop guest's image id ($GASDROP_ELF)"
DROP_AMOUNT=50000000
DI="$W/relayer-inbox-drops"; mkdir -p "$DI"
DD_LOG="$W/relayer-drops.out"; : > "$DD_LOG"
# This run's receipts and private proofs are dev mode's (PROOFS=dev): the daemon refuses gas drops under RISC0_DEV_MODE
# unless told it is a local devnet.
DEVR=(); [ "$PROOFS" = dev ] && DEVR=(--accept-dev-mode-receipts)
start_drops() { # the gas-drop daemon (again), through the relayer's proxy; its pid in $W/dd.pid
  "${PROVE[@]}" "$BIN/ferrata-relayer" --eth-rpc "$RPC" --vault "$VAULT" --chain-id 11155111 --unlocked "$RELAYER" --from-block "$FROM" \
    --state "$W/relayer" --sequencer "$SEQ_RELAYER" --seed "$RL/seed.txt" --lez-wallet "$RL/wallet" --lez-password-file "$RL/lez.pass" \
    --lez-account "$RELAYER_LEZ" --bridge-account "$BRIDGE" --gasdrop-image-id "$GASDROP_ID" --drop-amount "$DROP_AMOUNT" \
    --drop-wait 20 "${DEVR[@]}" run --inbox "$DI" --poll-secs 2 >> "$DD_LOG" 2>> "$W/relayer-drops.err" &
  echo $! > "$W/dd.pid"
}
stop_drops() { # SIGNAL: the daemon stopped (TERM: its clean stop; KILL: as a crash), and waited for
  local pid; pid="$(cat "$W/dd.pid")"; kill -"$1" "$pid" 2>/dev/null
  for _ in $(seq 1 120); do kill -0 "$pid" 2>/dev/null || return 0; sleep 0.5; done
}
drop_lines() { jl "$DD_LOG" | jq -s -c '[.[] | select(.event == "job-done" and .result.kind == "gas-drop") | .result]' 2>/dev/null; }
say "the relayer daemon for gas drops: guest $GASDROP_ID, $DROP_AMOUNT native LEZ per drop, --drop-wait 20, inbox $DI, through $SEQ_RELAYER"
start_drops
GD=(--relayer "$RELAYER_LEZ" --gas-drop-inbox "$DI" --gas-drop-elf "$GASDROP_ELF")
else
  fund_private; GD=()
fi
u 06-scan "${F[@]}" scan || die "scan"
u 07-status "${F[@]}" status || die "status"
u 08-claim-held "${F[@]}" claim --bridge-program "$BRIDGE_BIN" "${GD[@]}" && die "the claim was not held by its delay"
if [ "$GAS_DROP" = 1 ]; then
  grep -q '^fee payer: a fresh one-time private account' "$RUN/user/08-claim-held.txt" \
    && ok "U.05: the claim's dry run preselects the gas drop: $(grep -m1 '^fee payer:' "$RUN/user/08-claim-held.txt" | cut -c1-120)" \
    || bad "U.05: the claim's dry run does not name the gas drop as its fee payer"
fi
let_claim_delay_pass "$RUN/user/08-claim-held.txt"

# ---- g. the gas drop asked of a relayer that never answers, then of the real one (review M3) ---------------------------
OTHER_ONE_TIME=""
if [ "$GAS_DROP" = 1 ]; then
  OTHER_RELAYER="0x$(printf 'b%.0s' $(seq 1 64))"   # a LEZ account no relayer here serves; its inbox nobody reads
  mkdir -p "$W/nobody-inbox"
  u 08b-claim-other-relayer "${PROVE[@]}" "${F[@]}" claim --bridge-program "$BRIDGE_BIN" --relayer "$OTHER_RELAYER" \
    --gas-drop-inbox "$W/nobody-inbox" --gas-drop-elf "$GASDROP_ELF" --gas-drop-wait 15 --send --yes \
    && die "a claim whose relayer never answers went through"
  OTHER_ONE_TIME="$(grep -o -E 'no credit to the one-time account [0-9A-Za-z]+' "$RUN/user/08b-claim-other-relayer.txt" | awk '{print $NF}' | head -n 1)"
  if [ -n "$OTHER_ONE_TIME" ] && ls "$W/nobody-inbox"/gas-drop-*.json > /dev/null 2>&1; then
    ok "M3: a claim naming a relayer that never answers handed it a job ($(ls "$W/nobody-inbox" | head -n 1)) and stopped after its 15 s wait, nothing sent (one-time account $OTHER_ONE_TIME)"
  else
    bad "M3: the claim naming the silent relayer: $(tail -n 3 "$RUN/user/08b-claim-other-relayer.txt" | tr '\n' ' ' | cut -c1-300)"
  fi
fi

# ---- c. R.07: a claim killed between its proof and its sending -----------------------------------------------------
h="$(hits)"; echo "trap sendTransaction" > "$PX/ctl"
hr="$(hits_of "$PR")"
[ "$GAS_DROP" = 1 ] && echo "trap sendTransaction" > "$PR/ctl"   # f: the relayer's first credit never leaves
say "R.07: the claim (its gas drop first), its sendTransaction trapped by the proxy; R.06: the relayer's credit trapped too"
( "${PROVE[@]}" "${FP[@]}" claim --bridge-program "$BRIDGE_BIN" "${GD[@]}" --send --yes > "$RUN/user/09-claim-killed.txt" 2>&1 ) &
CP=$!
if [ "$GAS_DROP" = 1 ]; then
  if wait_hit_of "$PR" "$hr"; then
    sleep 1; stop_drops KILL
    rec="$(jq -c '[.drops[]] | .[0] | {sends, settled, tx}' "$W/relayer/drops.json" 2>/dev/null)"
    fact drop1AtKill "${rec:-null}"
    if [ "$(echo "$rec" | jq -r '.settled')" = false ]; then
      ok "R.06 gas drop: the drops daemon SIGKILLed inside its first credit's send (held unsent by the proxy); its record: $rec"
    else
      bad "R.06 gas drop: no unsettled record when the daemon was killed ($rec)"
    fi
    echo "late sendTransaction 35" > "$PR/ctl"   # its next send: 503 to the daemon, at the sequencer 35 s later
    start_drops
  else
    bad "R.06 gas drop: the relayer's credit never reached its proxy"
  fi
fi
n0=""
if wait_hit "$h"; then
  n0="$(zone_txs)"   # the gas drop's credit is applied by now (the claim waits for it); the claim itself never left
  sleep 1; kill -9 "$CP" 2>/dev/null; wait "$CP" 2>/dev/null
  ok "R.07 claim: the tool SIGKILLed after its proof, with the claim's sendTransaction held unsent ($(tail -n 1 "$PX/ctl.hit"))"
else
  kill -9 "$CP" 2>/dev/null; bad "R.07 claim: the claim's send never reached the proxy ($(tail -n 5 "$RUN/user/09-claim-killed.txt" | tr '\n' ' '))"
fi
echo pass > "$PX/ctl"
sleep 40   # more than two of the zone's blocks
n1="$(zone_txs)"
u 09b-recover-after-kill "${F[@]}" recover || die "recover"
if [ -n "$n0" ] && [ "$n1" = "$n0" ] && grep -q 'unclaimed, claimable' "$RUN/user/09b-recover-after-kill.txt"; then
  ok "R.07 claim: nothing reached the zone ($n0 private transactions before and after) and recovery from the seed still finds the deposit claimable"
else
  bad "R.07 claim: the zone went from $n0 to $n1 private transactions, or the deposit is no longer claimable"
fi
u 10-claim "${PROVE[@]}" "${F[@]}" claim --bridge-program "$BRIDGE_BIN" "${GD[@]}" --send --yes || die "the private claim"
wait_tx "$RUN/user/10-claim.txt"
if [ "$GAS_DROP" != 1 ]; then :
elif grep -q 'recorded for this deposit by an earlier run' "$RUN/user/10-claim.txt" && grep -q '^  no new job' "$RUN/user/10-claim.txt"; then
  ok "R.07 claim + U.05: the claim again reused the one-time account and the credit the killed run had (no second job): $(grep -m1 '^gas drop:' "$RUN/user/10-claim.txt" | cut -c1-150)"
else
  bad "R.07 claim + U.05: the claim again did not reuse the earlier run's drop ($(grep -m1 '^gas drop:' "$RUN/user/10-claim.txt" | cut -c1-150))"
fi
if [ "$GAS_DROP" = 1 ]; then
  one1="$(grep -o -E '^gas drop: one-time account [0-9A-Za-z]+' "$RUN/user/09-claim-killed.txt" | awk '{print $NF}' | head -n 1)"
  if grep -q "asked before for this deposit, no credit from: $OTHER_RELAYER" "$RUN/user/09-claim-killed.txt" \
     && grep -q 'went to .*/relayer-inbox-drops/gas-drop-' "$RUN/user/09-claim-killed.txt" && [ -n "$one1" ] && [ "$one1" = "$OTHER_ONE_TIME" ]; then
    ok "M3: the same deposit's claim naming the real relayer reused the one-time account ($one1) and handed that relayer a receipt of its own; the output names the silent one"
  else
    bad "M3: the claim naming the real relayer: one-time account '$one1' (the silent relayer's: '$OTHER_ONE_TIME'); $(grep -E '^gas drop|^  ' "$RUN/user/09-claim-killed.txt" | head -n 4 | tr '\n' ' ' | cut -c1-300)"
  fi
  d1="$(drop_lines | jq -c '.[0] // empty')"
  rec1="$(jq -c '[.drops[]] | .[0] | {sends, settled, tx}' "$W/relayer/drops.json" 2>/dev/null)"
  fact drop1 "${d1:-null}"; fact drop1Record "${rec1:-null}"
  if [ -n "$d1" ] && [ "$(echo "$d1" | jq -r '.sends')" -ge 2 ] && [ "$(echo "$d1" | jq -r '.resolvedLater')" = true ] \
     && [ "$(hits_of "$PR")" -ge $((hr + 2)) ] && grep -q 'claims: *1 pending credit' "$RUN/user/10-claim.txt"; then
    ok "R.06 gas drop: after the kill and a send answered 503 the restarted daemon sent the credit again on the same account state and booked it from LEZ: $(echo "$d1" | jq -c '{sends, resolvedLater, tx}'); the claim's dry run claims exactly 1 pending credit"
  else
    bad "R.06 gas drop: the first drop after the kill: $d1; record $rec1; proxy hits $(hits_of "$PR") (from $hr); $(grep -m1 -o -E 'claims: +[0-9]+ pending credit' "$RUN/user/10-claim.txt")"
  fi
  # The credit answered 503 reached the sequencer late: two credits of the first drop at LEZ, one applied (the
  # second review's M1). Anything less fails, a late credit that applied before a third send included (the third
  # review's L-4; late-credit.sh, tested on robust-3's record by test-late-credit.sh).
  late=""
  for _ in $(seq 1 60); do
    late="$(jq -c 'select(.action == "late forward answered")' "$PR/log.jsonl" 2>/dev/null | head -n 1)"
    [ -n "$late" ] && break; sleep 1
  done
  fact drop1LateCredit "${late:-null}"
  if verdict="$(late_credit_verdict "$d1" "$late" "$(cat "$RUN/user/10-claim.txt" 2>/dev/null)")"; then
    ok "R.06 gas drop (the second review's M1): $verdict"
  else
    bad "R.06 gas drop (the second review's M1): $verdict"
  fi
  # The second drop: its credit forwarded with the answer held, the daemon killed; restarted 30 s later.
  hr2="$(hits_of "$PR")"; echo "hold sendTransaction 40" > "$PR/ctl"
  ( if wait_hit_of "$PR" "$hr2"; then sleep 2; stop_drops KILL; date -u +%T > "$W/dd2.killed"; sleep 30; start_drops; fi ) &
  WATCH=$!
fi
# The second deposit (its own delay, its own drop).
if ! u 10b-claim "${PROVE[@]}" "${F[@]}" claim --bridge-program "$BRIDGE_BIN" "${GD[@]}" --send --yes; then
  let_claim_delay_pass "$RUN/user/10b-claim.txt"
  u 10c-claim "${PROVE[@]}" "${F[@]}" claim --bridge-program "$BRIDGE_BIN" "${GD[@]}" --send --yes || die "the second private claim"
  C2="$RUN/user/10c-claim.txt"
else
  C2="$RUN/user/10b-claim.txt"
fi
wait_tx "$C2"
if [ "$GAS_DROP" = 1 ]; then
wait "$WATCH" 2>/dev/null
for _ in $(seq 1 60); do [ "$(drop_lines | jq 'length')" -ge 2 ] && break; sleep 2; done
stop_drops TERM
jl "$DD_LOG" > "$RUN/lez/relayer-drops.jsonl"
cp "$PR/log.jsonl" "$RUN/lez/holdproxy-relayer.jsonl" 2>/dev/null || true
d2="$(drop_lines | jq -c '.[1] // empty')"
fact drop2 "${d2:-null}"
fact dropBook "$(jq -c '.drops' "$W/relayer/drops.json" 2>/dev/null || echo null)"
if [ -s "$W/dd2.killed" ] && [ -n "$d2" ] && [ "$(echo "$d2" | jq -r '.sends')" = 1 ] && [ "$(echo "$d2" | jq -r '.resolvedLater')" = true ] \
   && grep -q 'claims: *1 pending credit' "$C2"; then
  ok "R.06 gas drop: the second credit forwarded with its answer held and the daemon SIGKILLed ($(cat "$W/dd2.killed")); restarted, it booked the drop from LEZ (its drop account's nonce moved), nothing sent again: $(echo "$d2" | jq -c '{sends, resolvedLater, tx}'); the claim's dry run claims exactly 1 pending credit"
else
  bad "R.06 gas drop: the second drop across the kill: $d2 (killed: $(cat "$W/dd2.killed" 2>/dev/null))"
fi
drops="$(drop_lines)"
ndrops="$(echo "${drops:-[]}" | jq 'length')"
nsealed="$(echo "${drops:-[]}" | jq '[.[] | select(.sealed == true)] | length')"
nref="$(jl "$DD_LOG" | jq -s '[.[] | select(.event == "job-refused")] | length' 2>/dev/null)"
fact gasDrops "${drops:-[]}"
# LEZ's wallet prints each claim's dry run: its fee payer (the one-time account) and the pending credit it claims.
sweeps="$(cat "$RUN/user/10-claim.txt" "$C2" | grep -c -E 'claims: +1 pending credit\(s\), [1-9][0-9]* to the payer' || true)"
fact claimSweeps "${sweeps:-0}"
if [ "$ndrops" = 2 ] && [ "$nsealed" = 2 ] && [ "$sweeps" = 2 ] && grep -q '^gas drop: .*(new).*sealed' "$C2"; then
  ok "U.05: the relayer daemon sent 2 gas drops, one per deposit, each sealed to the claim's one-time account (${nref:-0} jobs refused); both claims' dry runs claim that credit to their one-time fee payer, and both applied: $(grep -m1 '^gas drop:' "$C2" | cut -c1-150)"
else
  bad "U.05: the daemon's gas drops: $ndrops ($nsealed sealed, ${nref:-0} refused); claims sweeping a credit: $sweeps of 2; the second claim: $(grep -m1 '^gas drop:' "$C2" | cut -c1-150)"
fi
dstop="$(jl "$DD_LOG" | jq -c 'select(.event == "stop")' 2>/dev/null | tail -n 1)"
[ -n "$dstop" ] && ok "U.05: the gas-drop daemon stopped cleanly on SIGTERM ($dstop)" || bad "U.05: no stop line from the gas-drop daemon"
fund_private   # the burn's fee: the stand-in (burns with a drop are not built)
fi
u 11-recover "${F[@]}" recover || die "recover"
claimed="$(grep -c '  claimed$' "$RUN/user/11-recover.txt" || true)"
[ "$claimed" = 2 ] && ok "R.07 claim: after the interrupted claim, both deposits claimed once each (recover: $claimed claimed)" \
  || bad "R.07 claim: recover finds $claimed claimed deposits (expected 2)"
u 11b-status "${F[@]}" status || die "status"
FRESH="$(cast wallet new --json | jq -r '.[0].address')"
echo "FRESH=$FRESH" >> "$SEQNET_ENV"
say "the user's fresh Ethereum address for the release: $FRESH (cast wallet new; its key is not kept)"

# ---- d. R.07: a burn killed after its sending ------------------------------------------------------------------------
u 12-burn-dry-run "${F[@]}" burn --amount 0.001 --recipient "$FRESH" --bridge-program "$BRIDGE_BIN" || die "burn dry run"
nb0="$(zone_txs)"
h="$(hits)"; echo "hold sendTransaction 45" > "$PX/ctl"
say "R.07: the burn, its sendTransaction's answer held by the proxy"
( "${PROVE[@]}" "${FP[@]}" burn --amount 0.001 --recipient "$FRESH" --bridge-program "$BRIDGE_BIN" --send --yes \
    > "$RUN/user/13-burn-killed.txt" 2>&1 ) &
BP=$!
if wait_hit "$h"; then
  sleep 2; kill -9 "$BP" 2>/dev/null; wait "$BP" 2>/dev/null
  pending_rec="$(jq -c '[.wallets[].open[] | {i, amount, at, sent}]' "$U/lez-wallet/ferrata-burns.json" 2>/dev/null)"
  ok "R.07 burn: the tool SIGKILLed with its burn at the sequencer and the answer held; its journal: $pending_rec"
  fact pendingBurn "${pending_rec:-null}"
else
  kill -9 "$BP" 2>/dev/null; bad "R.07 burn: the burn's send never reached the proxy"
fi
for _ in $(seq 1 60); do [ "$(zone_txs)" -gt "$nb0" ] && break; sleep 5; done
nb1="$(zone_txs)"
[ "$nb1" = $((nb0 + 1)) ] && ok "R.07 burn: the burn the killed tool sent was applied (the zone: $nb0 -> $nb1 private transactions)" \
  || bad "R.07 burn: the zone went from $nb0 to $nb1 transactions"
echo pass > "$PX/ctl"
u 13-burn "${PROVE[@]}" "${F[@]}" burn --amount 0.001 --recipient "$FRESH" --bridge-program "$BRIDGE_BIN" --send --yes || die "the burn again"
sleep 40
nb2="$(zone_txs)"
open_d="$(jq '[.wallets[].open[]] | length' "$U/lez-wallet/ferrata-burns.json" 2>/dev/null)"
if grep -q "not burning again" "$RUN/user/13-burn.txt" && [ "$nb2" = "$nb1" ] && [ "$open_d" = 0 ]; then
  ok "R.07 burn: the same command again found that burn on the zone and burnt nothing more ($(grep -m1 'earlier run' "$RUN/user/13-burn.txt" | cut -c1-160)); the zone still $nb2 private transactions"
else
  bad "R.07 burn: the command again: $(grep -E 'earlier run|sent:' "$RUN/user/13-burn.txt" | head -n 2 | tr '\n' ' '); the zone $nb1 -> $nb2"
fi
u 14-recover "${F[@]}" recover || die "recover"
burns="$(grep -c -E 'burn .*counter|^  burn ' "$RUN/user/14-recover.txt" || true)"
fact recoverBurnsLines "$burns"
checkpoint_robust                                                          # e, k
printf '0x5ea15ea1\n' > "$W/seal.hex"   # the mock verifier's: the stand-in prover accepts the journal below first
u 15-release-held "${F[@]}" release --seal "$W/seal.hex" && say "note: the release was not held (its delay had passed)"
pass_time 121 "the release delay (120 s after finalizeAttested)"
u 16-release "${F[@]}" release --seal "$W/seal.hex" --job-out "$W/release-job.json" || die "release"
release_robust "$W/release-job.json"                                      # l
u 17-recover "${F[@]}" recover || die "recover"
u 18-status "${F[@]}" status || die "status"
paid="$(cast balance "$FRESH" --rpc-url "$RPC")"
[ "$paid" != 0 ] && ok "the release through the relayer paid $paid wei to the fresh address" || bad "nothing released to the fresh address"

# ---- h. R.07: a burn sent late, then a burn with other parameters (review L5; the final review's H1) ------------------
fund_private   # the burn's LEZ fee (the stand-in)
FRESH2="$(cast wallet new --json | jq -r '.[0].address')"
nb3="$(zone_txs)"; h="$(hits)"; echo "late sendTransaction 90" > "$PX/ctl"
say "R.07: a burn of the last 0.001, its sendTransaction answered 503 and forwarded to the sequencer 90 s later"
u 19-burn-late "${PROVE[@]}" "${FP[@]}" burn --amount 0.001 --recipient "$FRESH" --bridge-program "$BRIDGE_BIN" --send --yes \
  && bad "R.07 burn: the tool did not see the 503"
trapped_rec="$(jq -c '[.wallets[].open[] | select(.sent == null) | {i, amount, recipient}] | .[0]' "$U/lez-wallet/ferrata-burns.json" 2>/dev/null)"
fact trappedBurn "${trapped_rec:-null}"
ti="$(echo "$trapped_rec" | jq -r '.i // empty')"
[ -n "$ti" ] && ok "R.07 burn: the tool saw 503 for a burn whose transaction the proxy holds; its counter $ti stays reserved: $trapped_rec" \
  || bad "R.07 burn: no reserved counter for the burn answered 503"
nb4="$(zone_txs)"
u 20-burn-other "${PROVE[@]}" "${F[@]}" burn --amount 0.001 --recipient "$FRESH2" --bridge-program "$BRIDGE_BIN" --send --yes || die "the burn with other parameters"
wait_tx "$RUN/user/20-burn-other.txt"
nb5="$(zone_txs)"
for _ in $(seq 1 30); do jq -e 'select(.action == "late forward answered")' "$PX/log.jsonl" > /dev/null 2>&1 && break; sleep 5; done
lateans="$(jq -c 'select(.action == "late forward answered")' "$PX/log.jsonl" 2>/dev/null | tail -n 1 | cut -c1-200)"
sleep 20
nb6="$(zone_txs)"
oi="$(grep -o -E 'burn counter [0-9]+$' "$RUN/user/20-burn-other.txt" | tail -n 1 | grep -o -E '[0-9]+$')"
if [ -n "$ti" ] && [ -n "$oi" ] && [ "$oi" != "$ti" ] && [ "$nb4" = "$nb3" ] && [ "$nb5" = $((nb3 + 1)) ] && [ "$nb6" = "$nb5" ] && [ -n "$lateans" ] \
   && grep -q "interrupted before it was known to be sent" "$RUN/user/20-burn-other.txt"; then
  ok "R.07 burn: the burn with another recipient took counter $oi, not the interrupted burn's $ti, and was applied once ($nb4 -> $nb5); the late transaction reached the sequencer ($lateans) and was refused ($nb5 -> $nb6)"
else
  bad "R.07 burn: the zone $nb3 -> $nb4 -> $nb5 -> $nb6; counters interrupted '$ti', other '$oi'; late: '${lateans:-none}'; $(grep -E 'earlier|burn counter' "$RUN/user/20-burn-other.txt" | head -n 3 | tr '\n' ' ' | cut -c1-300)"
fi
u 21-recover "${F[@]}" recover || die "recover"
records
cp "$PX/log.jsonl" "$RUN/lez/holdproxy.jsonl" 2>/dev/null || true
jq -s 'add // {}' "$RJ" | jq --argjson p "$pass" --argjson f "$fail" '. + {pass: $p, fail: $f}' > "$RUN/robust.json"
printf '\n%s passed, %s failed\n' "$pass" "$fail" | tee -a "$R7"
say "hook-robust: done ($pass passed, $fail failed)"
[ "$fail" = 0 ]
