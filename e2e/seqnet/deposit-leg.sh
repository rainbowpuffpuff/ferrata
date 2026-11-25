# The robust run's interrupted deposit (RFP-022 R.07, the deposit leg), sourced by drive.sh in robust mode after the
# first deposit: the second deposit with the `ferrata` tool, its Ethereum endpoint behind e2e/netfault/holdproxy.py:
#   1. its eth_sendRawTransaction trapped (never forwarded) and the tool SIGKILLed: nothing on the chain, the deposit
#      journal (cli/src/deposits.rs, beside the keystore) holds it open;
#   2. the same command again, its transaction forwarded and mined with the answer held, the tool SIGKILLed: the
#      deposit on the chain, the tool never learnt it;
#   3. the same command again, straight to anvil: the journal's deposit found on the chain, nothing sent.
# Then (the hook) the import and the claim find it as any deposit. Verdicts (PASS/FAIL lines) in $W/deposit-leg.md and
# facts in $W/deposit-leg.json, which hook-robust.sh takes into robust.md and robust.json.
deposit_leg() {
  local errexit=0; case $- in *e*) errexit=1; set +e ;; esac   # drive.sh runs with -e; the kills here exit non-zero
  local PE="$W/holdproxy-eth-user" DL="$W/deposit-leg.md" J="$U/keystore/ferrata-deposits.json" pe_pid port i
  : > "$DL"
  dok() { echo "- PASS $*" >> "$DL"; say "PASS $*"; }
  dbad() { echo "- FAIL $*" >> "$DL"; say "FAIL $*"; }
  dcount() { cast call "$VAULT" 'depositCount(address)(uint64)' "$DEPOSITOR" --rpc-url "$RPC"; }
  ddeposits() { cast logs --rpc-url "$RPC" --address "$VAULT" --from-block "$FROM" "$(cast keccak 'Deposit(address,uint64,uint64,address,uint256,bytes32)')" --json | jq 'length'; }
  dhits() { [ -f "$PE/ctl.hit" ] && wc -l < "$PE/ctl.hit" || echo 0; }
  dwait() { for i in $(seq 1 240); do [ "$(dhits)" -gt "$1" ] && return 0; sleep 0.25; done; return 1; }
  mkdir -p "$PE"; echo pass > "$PE/ctl"
  python3 -I "$REPO/e2e/netfault/holdproxy.py" --upstream "$RPC" --port-file "$PE/port" --ctl "$PE/ctl" --log "$PE/log.jsonl" > "$PE/out" 2>&1 &
  pe_pid=$!
  for i in $(seq 1 40); do [ -s "$PE/port" ] && break; sleep 0.25; done
  port="$(cat "$PE/port")"
  local FE=("$BIN/ferrata" --seed "$U/seed.txt" --eth-rpc "http://127.0.0.1:$port" --chain-id 11155111 --vault "$VAULT" --from-block "$FROM")
  local c0 n0 e0 b0 h p
  c0="$(dcount)"; n0="$(cast nonce "$DEPOSITOR" --rpc-url "$RPC")"; e0="$(ddeposits)"; b0="$(cast balance "$DEPOSITOR" --rpc-url "$RPC")"
  # 1. trapped: built and signed, never sent
  h="$(dhits)"; echo "trap eth_sendRawTransaction" > "$PE/ctl"
  ( "${FE[@]}" deposit --denomination 0 --keystore "$KEYSTORE" --password-file "$U/eth.pass" --send --yes > "$RUN/user/04b-deposit-trapped.txt" 2>&1 ) &
  p=$!
  if dwait "$h"; then sleep 1; kill -9 "$p" 2>/dev/null; wait "$p" 2>/dev/null
    local open1; open1="$(jq -c '[.open[] | {j, nonce, hash}]' "$J" 2>/dev/null)"
    if [ "$(dcount)" = "$c0" ] && [ "$(cast nonce "$DEPOSITOR" --rpc-url "$RPC")" = "$n0" ] && [ "$(echo "$open1" | jq 'length')" = 1 ]; then
      dok "R.07 deposit: the tool SIGKILLed with its signed deposit trapped before any node (depositCount $c0, nonce $n0 unchanged); its journal: $open1"
    else dbad "R.07 deposit: after the trapped send: depositCount $(dcount) (was $c0), journal $open1"; fi
  else kill -9 "$p" 2>/dev/null; dbad "R.07 deposit: the deposit's send never reached the proxy"; fi
  # 2. held: mined, the answer never delivered
  echo pass > "$PE/ctl"; h="$(dhits)"; echo "hold eth_sendRawTransaction 40" > "$PE/ctl"
  ( "${FE[@]}" deposit --denomination 0 --keystore "$KEYSTORE" --password-file "$U/eth.pass" --send --yes > "$RUN/user/04c-deposit-held.txt" 2>&1 ) &
  p=$!
  if dwait "$h"; then sleep 3; kill -9 "$p" 2>/dev/null; wait "$p" 2>/dev/null
    if [ "$(dcount)" = $((c0 + 1)) ] && grep -q 'never reached a node: sent again with the same j and nonce' "$RUN/user/04c-deposit-held.txt" \
       && [ "$(jq '.open | length' "$J" 2>/dev/null)" = 1 ]; then
      dok "R.07 deposit: the same command again sent the trapped deposit with its j and nonce ($(grep -m1 'never reached' "$RUN/user/04c-deposit-held.txt" | cut -c1-160)); mined (depositCount $(dcount)), the answer held and the tool SIGKILLed: still open in its journal"
    else dbad "R.07 deposit: the held send: depositCount $(dcount); $(grep -E 'journal|sent' "$RUN/user/04c-deposit-held.txt" | head -n 3 | tr '\n' ' ' | cut -c1-300)"; fi
  else kill -9 "$p" 2>/dev/null; dbad "R.07 deposit: the second send never reached the proxy"; fi
  echo pass > "$PE/ctl"
  # 3. the same command again: found, nothing sent
  u 04d-deposit-again "${F[@]}" deposit --denomination 0 --keystore "$KEYSTORE" --password-file "$U/eth.pass" --send --yes || dbad "R.07 deposit: the command again failed"
  local c1 n1 e1 b1
  c1="$(dcount)"; n1="$(cast nonce "$DEPOSITOR" --rpc-url "$RPC")"; e1="$(ddeposits)"; b1="$(cast balance "$DEPOSITOR" --rpc-url "$RPC")"
  if grep -q 'is on the chain' "$RUN/user/04d-deposit-again.txt" && grep -q 'not depositing a second time' "$RUN/user/04d-deposit-again.txt" \
     && [ "$c1" = $((c0 + 1)) ] && [ "$n1" = $((n0 + 1)) ] && [ "$e1" = $((e0 + 1)) ] && [ "$(jq '.open | length' "$J" 2>/dev/null)" = 0 ]; then
    dok "R.07 deposit: the same command again found the interrupted deposit on the chain and sent nothing ($(grep -m1 'is on the chain' "$RUN/user/04d-deposit-again.txt" | cut -c1-170)); across the three runs one deposit (Deposit events $e0 -> $e1, depositCount $c0 -> $c1, the depositor's nonce $n0 -> $n1), the journal closed"
  else dbad "R.07 deposit: after the command again: depositCount $c0 -> $c1, Deposit events $e0 -> $e1, nonce $n0 -> $n1, open $(jq '.open | length' "$J" 2>/dev/null); $(head -n 12 "$RUN/user/04d-deposit-again.txt" | grep -E 'journal|sent|not dep' | tr '\n' ' ' | cut -c1-300)"; fi
  jq -n -c --argjson c0 "$c0" --argjson c1 "$c1" --argjson n0 "$n0" --argjson n1 "$n1" --argjson e0 "$e0" --argjson e1 "$e1" --arg b0 "$b0" --arg b1 "$b1" \
    '{depositLeg: {depositCount: [$c0, $c1], nonce: [$n0, $n1], depositEvents: [$e0, $e1], depositorWei: [$b0, $b1]}}' > "$W/deposit-leg.json"
  cp "$PE/log.jsonl" "$RUN/eth/holdproxy-deposit.jsonl" 2>/dev/null || true
  kill "$pe_pid" 2>/dev/null || true
  [ "$errexit" = 1 ] && set -e
  return 0
}
