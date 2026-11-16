# Shared by e2e/seqnet's drive.sh and hooks (sourced after the run's env.sh; in a hook lez/crown/session.sh x gives the
# zone's environment: RPC_PORT, BEDROCK_PORT, CHANNEL, ZONE_STATE, ZONE_BUNDLE, ZONE_BIN, ZONE_WORK, ZONE_DRIVER_ARGS).
SEQ="http://127.0.0.1:${RPC_PORT:-3040}"
read -r -a D <<< "${ZONE_DRIVER_ARGS:-}"
BRIDGE_BIN="${ZONE_BUNDLE:-}/program/ferrata_bridge.bin"
[ -n "${ZONE_BIN:-}" ] && export RISC0_SERVER_PATH="$ZONE_BIN/r0vm"
# How LEZ's wallet proves a private transaction: dev mode (a fake receipt, which this run's sequencer accepts) or
# for real on this machine's CPU (r0vm; the sequencer verifies it).
if [ "$PROOFS" = dev ]; then PROVE=(env RISC0_DEV_MODE=1); else PROVE=(env -u RISC0_DEV_MODE); fi
SEAL_MOCK=0x5ea15ea1

say() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$RUN/run.log"; }
die() { say "STOP: $*"; exit 1; }
# u NAME CMD...: one command of the user, its output (what the user saw) in user/NAME.txt; exit status and seconds in
# user/INDEX.
u() {
  local n="$1" rc t0; shift; t0=$(date +%s.%N)
  { echo "\$ ${*//$BIN\//}"; echo; "$@"; } > "$RUN/user/$n.txt" 2>&1; rc=$?
  local subs=(-e "s#$W/#<work>/#g" -e "s#$BIN/##g"); [ -n "${ZONE_BUNDLE:-}" ] && subs+=(-e "s#$ZONE_BUNDLE/#<bundle>/#g")
  sed -i "${subs[@]}" "$RUN/user/$n.txt"
  printf '%s\texit %s\t%.1f s\n' "$n" "$rc" "$(echo "$(date +%s.%N) - $t0" | bc)" >> "$RUN/user/INDEX"
  return $rc
}
head_block() { curl -fsS -m 10 -H 'content-type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"getLastBlockId","params":[]}' "$SEQ" | jq -r .result; }
eth_now() { cast block --rpc-url "$RPC" latest --json | jq -r '.timestamp' | xargs printf '%d\n'; }
pass_time() { # seconds what: anvil's clock moved on (the devnet's clock: a user waits this long)
  cast rpc evm_increaseTime "$1" --rpc-url "$RPC" > /dev/null && cast rpc evm_mine --rpc-url "$RPC" > /dev/null
  say "anvil's clock moved on $1 s: $2 (Ethereum time now $(eth_now))"
}

# a. The operator and the relayer: the stand-in committee's finality update for capture-1's block, bridge Init and
#    Register ETH (first capture), ImportRoot n.
operator_import() {
  say "operator: crown-driver crown-b --operator-only over capture-1"
  "$ZONE_BIN/crown-driver" crown-b "${D[@]}" --identity "$SA" --capture "$VOL/inputs/capture-1" --private fake \
    --operator-only yes --session-out "$W/session-b.json" --notes-out "$W/notes.json" > "$RUN/lez/crown-b-operator.log" 2>&1 \
    || { tail -n 20 "$RUN/lez/crown-b-operator.log"; die "crown-b --operator-only"; }
  grep -E '^\[crown-b\]' "$RUN/lez/crown-b-operator.log" | tail -n 4 | tee -a "$RUN/run.log"
  jq '{tipT0, tipT1, blocks, configId, vault, steps: [.steps[] | {step, status, block, tx_hash: (.tx_hash // .tx)}]}' "$W/session-b.json" \
    > "$RUN/lez/operator-steps.json" 2>/dev/null || cp "$W/session-b.json" "$RUN/lez/operator-steps.json"
}

# b. The gas drop's stand-in.
fund_private() {
  say "gas drop stand-in: native LEZ to the user's private account /0 (seqnet-fund)"
  # LEZ's wallet prints its sync to stdout: the tool's JSON lines are kept apart.
  "${PROVE[@]}" "$BIN/seqnet-fund" --sequencer "$SEQ" --seed "$U/seed.txt" --lez-wallet "$U/lez-wallet" \
    --lez-password-file "$U/lez.pass" --faucet-home "$W/faucet" > "$W/fund.out" 2> "$W/fund.err" \
    || { tail -n 20 "$W/fund.err"; die "seqnet-fund"; }
  grep '^{' "$W/fund.out" > "$RUN/lez/fund.jsonl"
  jq -c '{step, tx, amount, fee_paid_in_the_proof, send_ms, seen_ms}' "$RUN/lez/fund.jsonl" | tee -a "$RUN/run.log" || true
}

# The claim's delay let pass on anvil's clock: the tool's refusal names the first block it may be claimed in.
let_claim_delay_pass() {
  local until now n
  until="$(grep -o -E 'from Ethereum block [0-9]+' "$1" | head -n 1 | awk '{print $NF}')"
  [ -n "$until" ] || die "the held claim does not name its first block ($1)"
  now="$(cast block-number --rpc-url "$RPC")"; n=$(( until - now + 1 )); [ "$n" -lt 1 ] && n=1
  cast rpc anvil_mine "$(printf '0x%x' "$n")" --rpc-url "$RPC" > /dev/null
  say "the claim's delay let pass on anvil's clock: $n blocks mined (block $now -> $(cast block-number --rpc-url "$RPC"); the tool counts 12 s per block)"
}

# The node's own final blocks, as lez/crown/session.sh's node_dump: /cryptarchia/info, /channel, /cryptarchia/blocks.
node_dump() {
  local N="$W/node" api="http://127.0.0.1:$BEDROCK_PORT" from lib to
  rm -rf "$N"; mkdir -p "$N/blocks"
  from="$(jq -r .tip_slot "$VOL/session-a/channel.json")"; from=$(( from > 3000 ? from - 3000 : 0 ))
  curl -fsS -m 10 -o "$N/info.json" "$api/cryptarchia/info" && curl -fsS -m 10 -o "$N/channel.json" "$api/channel/$CHANNEL" || return 1
  curl -fsS -m 10 -o "$N/chain-id.json" "$api/chain/id" || true
  lib="$(jq -r '.cryptarchia_info.lib_slot // .lib_slot' "$N/info.json")"; echo "$from" > "$N/slot-from.txt"
  [[ "$lib" =~ ^[0-9]+$ ]] || return 1
  while [ "$from" -le "$lib" ]; do
    to=$(( from + 499 )); [ "$to" -gt "$lib" ] && to="$lib"
    curl -fsS -m 60 -o "$N/blocks/$from-$to.json" "$api/cryptarchia/blocks?slot_from=$from&slot_to=$to" || return 1
    from=$(( to + 1 ))
  done
}

relayer_round() { # one round of the relayer daemon (its inbox, the vault's candidates, its fees)
  "$BIN/ferrata-relayer" --eth-rpc "$RPC" --vault "$VAULT" --chain-id 11155111 --unlocked "$RELAYER" --from-block "$FROM" \
    --state "$W/relayer" run --inbox "$W/relayer-inbox" --poll-secs 1 --rounds 1 >> "$RUN/eth/relayer.jsonl" 2>> "$W/relayer.err" \
    || { tail -n 10 "$W/relayer.err"; die "relayer round"; }
}

# d. The checkpoint over T0+1..=T1 (T1: the zone's head after the user's burn, once final on Bedrock).
checkpoint() {
  local t0 t1 name i C="$W/checkpoint" journal dig
  t0="$(jq -r .tip.id "$SA")"; t1="$(head_block)"; name="seqnet-$t0-$t1"
  say "checkpoint: blocks $((t0 + 1))..=$t1, once block $t1 is final on Bedrock"
  mkdir -p "$C" "$W/relayer-inbox"
  for i in $(seq 1 30); do
    sleep 20
    node_dump || continue
    rm -rf "$C/input"
    "$BIN/realblocks-crown" --node "$W/node" --channel-key "$ZONE_STATE/sequencer_home/channel_signing_key" \
      --genesis "$ZONE_STATE/bedrock/deployment-settings.yaml" --session-a "$SA" --vault "$VAULT" --config-id "$CID" \
      --genesis-state "$GS" --tip "$t1" --name "$name" --out "$C/input" > "$C/realblocks-crown.txt" 2>&1 && break
    [ "$i" = 30 ] && { tail -n 15 "$C/realblocks-crown.txt"; die "realblocks-crown: block $t1 not final after 10 min"; }
  done
  say "realblocks-crown: $(tail -n 2 "$C/realblocks-crown.txt" | tr '\n' ' ' | cut -c1-300)"
  "$BIN/ferrata-e2e" exec --program "$R0BF/checkpoint_b.bin" --input "$C/input/$name.input" --meta "$C/input/$name.json" \
    > "$C/exec.txt" 2>&1 || { tail -n 15 "$C/exec.txt"; die "checkpoint_b's executor run"; }
  say "checkpoint_b executed (final program $CHECKPOINT_B_ID): $(tail -n 1 "$C/exec.txt" | cut -c1-300)"
  journal="$(jq -r .journal "$C/input/$name.json")"; journal="0x${journal#0x}"
  dig="0x$(python3 -I -c 'import hashlib, sys; print(hashlib.sha256(bytes.fromhex(sys.argv[1])).hexdigest())' "${journal#0x}")"
  cast send "$MOCK" "accept(bytes32,bytes32)" "$CHECKPOINT_B_ID" "$dig" --rpc-url "$RPC" --unlocked --from "$DEPLOYER" > "$C/accept.txt" 2>&1 \
    || { cat "$C/accept.txt"; die "the stand-in prover's accept (checkpoint)"; }
  say "stand-in prover: the mock verifier accepts checkpoint_b's journal (sha256 $dig)"
  jq -n --arg j "$journal" --arg s "$SEAL_MOCK" '{kind: "checkpoint", journal: $j, seal: $s}' > "$W/relayer-inbox/checkpoint-$t1.json"
  relayer_round
  say "relayer: $(tail -n 3 "$RUN/eth/relayer.jsonl" | jq -c '{event, tx: (.tx // .hash // null), why: (.why // .reason // null)}' | tr '\n' ' ')"
  pass_time 3601 "the candidate's wait (Delta 3600 s)"
  relayer_round
  say "relayer: $(tail -n 3 "$RUN/eth/relayer.jsonl" | jq -c '{event, tx: (.tx // .hash // null)}' | tr '\n' ' ')"
  say "vault: finalState $(cast call "$VAULT" 'finalState()(bytes32)' --rpc-url "$RPC"), checkpoints $(cast call "$VAULT" 'checkpointCount()(uint256)' --rpc-url "$RPC")"
  mkdir -p "$RUN/checkpoint"
  cp "$C/input/$name.json" "$C/input/burns.json" "$C/input/attestor.json" "$C/input/blocks.tsv" "$C/realblocks-crown.txt" "$C/exec.txt" \
    "$RUN/checkpoint/" 2>/dev/null || true
}

# e. A release job through the relayer: the stand-in prover accepts the release journal, the relayer sends it.
relay_release() {
  local job="$1" journal dig
  journal="$(jq -r .journal "$job")"; journal="0x${journal#0x}"
  dig="0x$(python3 -I -c 'import hashlib, sys; print(hashlib.sha256(bytes.fromhex(sys.argv[1])).hexdigest())' "${journal#0x}")"
  cast send "$MOCK" "accept(bytes32,bytes32)" "$RELEASE_ID" "$dig" --rpc-url "$RPC" --unlocked --from "$DEPLOYER" > "$W/accept-release.txt" 2>&1 \
    || { cat "$W/accept-release.txt"; die "the stand-in prover's accept (release)"; }
  say "stand-in prover: the mock verifier accepts the release journal (sha256 $dig)"
  cp "$job" "$W/relayer-inbox/release-$(date +%s).json"
  relayer_round
  say "relayer: $(tail -n 3 "$RUN/eth/relayer.jsonl" | jq -c '{event, tx: (.tx // .hash // null), why: (.why // .reason // null)}' | tr '\n' ' ')"
  say "the fresh address $FRESH holds $(cast balance "$FRESH" --rpc-url "$RPC") wei"
}

# wait_tx FILE: the transaction a command of the user sent ("sent: HASH" in its output) applied in a block of the zone.
wait_tx() {
  local h i line
  h="$(grep -o -E 'sent: [0-9a-fA-Fx]+' "$1" | tail -n 1 | awk '{print $2}')"
  [ -n "$h" ] || die "no transaction hash in $1"
  for i in $(seq 1 90); do
    line="$("$BIN/seqnet-txs" --sequencer "$SEQ" --home "$W/txs-wallet" --bridge-account "$BRIDGE" --attest-account "$ATTEST" 2>/dev/null | grep -F "$h")"
    if [ -n "$line" ]; then
      say "applied: $(echo "$line" | awk -F'\t' '{print "block " $1 ", " $6 " transaction " $5 ", reverted " $7 ", program " $8 ", visible bridge calls " $10}')"
      echo "$line" | awk -F'\t' '{print $7}' | grep -q false || die "transaction $h reverted"
      return 0
    fi
    sleep 4
  done
  die "transaction $h not in a block after 6 min"
}

records() {
  "$BIN/seqnet-txs" --sequencer "$SEQ" --home "$W/txs-wallet" --bridge-account "$BRIDGE" --attest-account "$ATTEST" \
    > "$RUN/lez/transactions.tsv" 2> "$W/txs.err" || say "seqnet-txs failed: $(tail -n 3 "$W/txs.err")"
  say "the zone: $(($(wc -l < "$RUN/lez/transactions.tsv") - 1)) transactions in blocks 1..=$(head_block)"
}

# The chains' own state as one JSON line (ops.sh chain [ADDR]): read directly from anvil (cast) and from the sequencer
# (seqnet-txs, LEZ's wallet library with a throwaway home and no keys), never through the module or the view. Log data
# words are kept as hex (the reader decodes them).
chain_state() {
  local fresh="${1:-}" topic logs txs n
  local j; j="$(jq -n -c --argjson b "$(cast block-number --rpc-url "$RPC")" --arg d "$DEPOSITOR" --arg u "$(cast balance "$DEPOSITOR" --rpc-url "$RPC")" \
    --arg v "$(cast balance "$VAULT" --rpc-url "$RPC")" '{eth: {block: $b, user: $d, user_balance_wei: $u, vault_balance_wei: $v}}')"
  for ev in 'Deposit(address,uint64,uint64,address,uint256,bytes32)' \
            'Release(bytes32,address,address,uint256,address,uint256,bytes32)' \
            'CheckpointFinalized(uint256,bytes32,bytes32,bytes32,uint64,uint64,bool,uint64,bytes32,uint32,uint32)'; do
    topic="$(cast keccak "$ev")"
    logs="$(cast logs --rpc-url "$RPC" --address "$VAULT" --from-block "$FROM" "$topic" --json 2>/dev/null \
      | jq -c '[.[] | {block: .blockNumber, tx: .transactionHash, topics: .topics[1:], data: .data}]' 2>/dev/null)"
    j="$(echo "$j" | jq -c --arg k "${ev%%(*}" --argjson l "${logs:-[]}" '.eth.logs[$k] = $l')"
  done
  if [ -n "$fresh" ]; then j="$(echo "$j" | jq -c --arg a "$fresh" --arg b "$(cast balance "$fresh" --rpc-url "$RPC")" '.eth.fresh = {address: $a, balance_wei: $b}')"; fi
  "$BIN/seqnet-txs" --sequencer "$SEQ" --home "$W/txs-wallet-check" --bridge-account "$BRIDGE" --attest-account "$ATTEST" \
    > "$W/chain-txs.tsv" 2> "$W/chain-txs.err"
  txs="$(tail -n +2 "$W/chain-txs.tsv" | jq -R -s -c 'split("\n") | map(select(length > 0) | split("\t")
    | {block: (.[0] | tonumber), tx: .[4], kind: .[5], reverted: (.[6] == "true"), program: .[7], variant: .[8], visible: .[9]})')"
  echo "$j" | jq -c --argjson h "$(head_block)" --argjson t "${txs:-[]}" '.lez = {head: $h, txs: $t}'
}
