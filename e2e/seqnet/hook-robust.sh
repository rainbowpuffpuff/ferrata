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
#      tool SIGKILLed; the burn applied; the same command again: the tool finds the burn its record names on the zone
#      and does not burn again (the private holding 0.001 less, once)
#   e. the checkpoint, the release through the relayer, recover and status (as hook-cli.sh)
# SEQNET_GAS_DROP=0: the claims' fees from the stand-in (seqnet-fund) instead of the relayer's drops (b skipped).
# PASS/FAIL lines in $RUN/robust.md, the facts in $RUN/robust.json.
set -uo pipefail
. "$SEQNET_ENV"
. "$REPO/e2e/seqnet/common.sh"
F=("$BIN/ferrata" --seed "$U/seed.txt" --eth-rpc "$RPC" --chain-id 11155111 --vault "$VAULT" --from-block "$FROM"
   --sequencer "$SEQ" --lez-wallet "$U/lez-wallet" --lez-password-file "$U/lez.pass" --bridge-account "$BRIDGE")
RELAYER_PHRASE="legal winner thank year wave sausage worth useful legal winner thank yellow"   # BIP-39 test vector
R7="$RUN/robust.md"; RJ="$W/robust.jsonl"; : > "$RJ"
printf '# seqnet robust: U.03 and R.07 on a running LEZ sequencer\n\n' > "$R7"
pass=0; fail=0
ok() { echo "- PASS $*" | tee -a "$R7"; pass=$((pass + 1)); say "PASS $*"; }
bad() { echo "- FAIL $*" | tee -a "$R7"; fail=$((fail + 1)); say "FAIL $*"; }
fact() { jq -n -c --arg k "$1" --argjson v "$2" '{($k): $v}' >> "$RJ"; }
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
hits() { [ -f "$PX/ctl.hit" ] && wc -l < "$PX/ctl.hit" || echo 0; }
wait_hit() { local before=$1 i; for i in $(seq 1 1200); do [ "$(hits)" -gt "$before" ] && return 0; sleep 0.5; done; return 1; }

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
RD_LOG="$RUN/lez/relayer-daemon.jsonl"
"$BIN/ferrata-relayer" --eth-rpc "$RPC" --vault "$VAULT" --chain-id 11155111 --unlocked "$RELAYER" --from-block "$FROM" \
  --state "$W/relayer" --sequencer "$SEQ" --seed "$RL/seed.txt" --lez-wallet "$RL/wallet" --lez-password-file "$RL/lez.pass" \
  --lez-account "$RELAYER_LEZ" --bridge-account "$BRIDGE" --import-roots --import-every 0 \
  run --inbox "$W/relayer-inbox-imports" --poll-secs 5 > "$RD_LOG" 2> "$W/relayer-daemon.err" &
RD=$!
imported=""
for _ in $(seq 1 180); do
  imported="$(jl "$RD_LOG" | jq -c 'select(.event == "root-imported" and .applied == true)' 2>/dev/null | head -n 1)"
  [ -n "$imported" ] && break
  kill -0 "$RD" 2>/dev/null || break
  sleep 5
done
kill -TERM "$RD" 2>/dev/null; wait "$RD" 2>/dev/null
stopline="$(jl "$RD_LOG" | jq -c 'select(.event == "stop")' 2>/dev/null | tail -n 1)"
if [ -n "$imported" ]; then
  ok "U.03: ferrata-relayer imported root(cid, $(echo "$imported" | jq -r .leafCount)) on the running sequencer (tx $(echo "$imported" | jq -r .tx), recovered $(echo "$imported" | jq -r '.recovered // false')); $(jl "$RD_LOG" | jq -s 'map(select(.event == "root-import-unconfirmed" or .event == "root-import-pending")) | length') rounds before it was applied"
  fact rootImport "$imported"
else
  bad "U.03: no root-imported line from the relayer daemon ($(jl "$RD_LOG" | tail -n 3 | jq -c '{event, reason}' 2>/dev/null | tr '\n' ' '); $(tail -n 3 "$W/relayer-daemon.err"))"
  die "the relayer's root import"
fi
if [ -n "$stopline" ] && [ "$(echo "$stopline" | jq -r .reason)" = SIGTERM ]; then ok "U.03: SIGTERM: a clean stop line ($stopline)"; else bad "U.03: no clean stop line on SIGTERM ($stopline)"; fi
[ ! -e "$W/relayer/root-import-inflight.json" ] && ok "U.03: nothing left in flight in the relayer's state" || bad "U.03: an import left in flight"

# ---- b. U.05: the relayer daemon's gas drops ---------------------------------------------------------------------------
GAS_DROP="${SEQNET_GAS_DROP:-1}"
if [ "$GAS_DROP" = 1 ]; then
GASDROP_ELF="$BIN/gasdrop.bin"
GASDROP_ID="$(r0vm --id --elf "$GASDROP_ELF" 2>/dev/null | tail -n 1)"
[ -n "$GASDROP_ID" ] || die "the gas-drop guest's image id ($GASDROP_ELF)"
DROP_AMOUNT=50000000
DI="$W/relayer-inbox-drops"; mkdir -p "$DI"
DD_LOG="$RUN/lez/relayer-drops.jsonl"
say "the relayer daemon for gas drops: guest $GASDROP_ID, $DROP_AMOUNT native LEZ per drop, inbox $DI"
"${PROVE[@]}" "$BIN/ferrata-relayer" --eth-rpc "$RPC" --vault "$VAULT" --chain-id 11155111 --unlocked "$RELAYER" --from-block "$FROM" \
  --state "$W/relayer" --sequencer "$SEQ" --seed "$RL/seed.txt" --lez-wallet "$RL/wallet" --lez-password-file "$RL/lez.pass" \
  --lez-account "$RELAYER_LEZ" --bridge-account "$BRIDGE" --gasdrop-image-id "$GASDROP_ID" --drop-amount "$DROP_AMOUNT" \
  run --inbox "$DI" --poll-secs 2 > "$DD_LOG" 2> "$W/relayer-drops.err" &
DD=$!
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

# ---- c. R.07: a claim killed between its proof and its sending -----------------------------------------------------
h="$(hits)"; echo "trap sendTransaction" > "$PX/ctl"
say "R.07: the claim (its gas drop first), its sendTransaction trapped by the proxy"
( "${PROVE[@]}" "${FP[@]}" claim --bridge-program "$BRIDGE_BIN" "${GD[@]}" --send --yes > "$RUN/user/09-claim-killed.txt" 2>&1 ) &
CP=$!
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
kill -TERM "$DD" 2>/dev/null; wait "$DD" 2>/dev/null
drops="$(jl "$DD_LOG" | jq -s -c '[.[] | select(.event == "job-done" and .result.kind == "gas-drop") | .result]' 2>/dev/null)"
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
  pending_rec="$(cat "$U/lez-wallet/ferrata-pending-burn.json" 2>/dev/null | jq -c '{i, amount, at}')"
  ok "R.07 burn: the tool SIGKILLed with its burn at the sequencer and the answer held; its record: $pending_rec"
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
if grep -q "not burning again" "$RUN/user/13-burn.txt" && [ "$nb2" = "$nb1" ] && [ ! -e "$U/lez-wallet/ferrata-pending-burn.json" ]; then
  ok "R.07 burn: the same command again found that burn on the zone and burnt nothing more ($(grep -m1 'earlier run' "$RUN/user/13-burn.txt" | cut -c1-160)); the zone still $nb2 private transactions"
else
  bad "R.07 burn: the command again: $(grep -E 'earlier run|sent:' "$RUN/user/13-burn.txt" | head -n 2 | tr '\n' ' '); the zone $nb1 -> $nb2"
fi
u 14-recover "${F[@]}" recover || die "recover"
burns="$(grep -c -E 'burn .*counter|^  burn ' "$RUN/user/14-recover.txt" || true)"
fact recoverBurnsLines "$burns"
checkpoint                                                                 # e
printf '0x5ea15ea1\n' > "$W/seal.hex"   # the mock verifier's: the stand-in prover accepts the journal below first
u 15-release-held "${F[@]}" release --seal "$W/seal.hex" && say "note: the release was not held (its delay had passed)"
pass_time 121 "the release delay (120 s after finalizeAttested)"
u 16-release "${F[@]}" release --seal "$W/seal.hex" --job-out "$W/release-job.json" || die "release"
relay_release "$W/release-job.json"
u 17-recover "${F[@]}" recover || die "recover"
u 18-status "${F[@]}" status || die "status"
paid="$(cast balance "$FRESH" --rpc-url "$RPC")"
[ "$paid" != 0 ] && ok "the release through the relayer paid $paid wei to the fresh address" || bad "nothing released to the fresh address"
records
cp "$PX/log.jsonl" "$RUN/lez/holdproxy.jsonl" 2>/dev/null || true
jq -s 'add // {}' "$RJ" | jq --argjson p "$pass" --argjson f "$fail" '. + {pass: $p, fail: $f}' > "$RUN/robust.json"
printf '\n%s passed, %s failed\n' "$pass" "$fail" | tee -a "$R7"
say "hook-robust: done ($pass passed, $fail failed)"
[ "$fail" = 0 ]
