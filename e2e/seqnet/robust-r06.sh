# Sourced by hook-robust.sh: the off-chain components of the return leg under faults on the running zone (RFP-022 R.06)
# and the release leg interrupted (R.07).
#   at_start / at_chaos / at_stop: the rule (B) attestor (checkpoint/attestor's ferrata_attestor, as
#     deploy/testnet/host/bin/ferrata-bridge-attestor runs it: its own key certified by the zone's channel key for this
#     deployment, a round every 2 s) with its node and its LEZ node each behind a holdproxy; while the user's commands
#     run, at_chaos SIGKILLs it inside a round (the loop and the running ferrata_attestor) and starts it again, and cuts
#     its node off and its LEZ node off for 20 s each; at_verdict reads every attestation it wrote: one signature per
#     tip, every tip the zone's block.
#   checkpoint_robust: the checkpoint over the attestor's own attestation (realblocks-crown --att), proven through the
#     prover client (deploy/testnet/host/bin/ferrata-bridge-prover watch) and the stand-in prover (standin_modal's
#     server, standin-prove.sh): its first call dies mid-proof, the client SIGKILLed during its second and started
#     again; one job in the relayer's inbox; the relayer submits it once and finalizes it.
#   release_robust: the release through ferrata-relayer with its Ethereum endpoint behind a holdproxy: killed with the
#     release built and its transaction trapped (never sent), started again and killed with the transaction mined and
#     its answer held, started again: booked from the chain, one Release.
AT="$W/attestor"; PS="$W/prover"
at_px() { # NAME UPSTREAM: a holdproxy; its URL on stdout
  local d="$W/holdproxy-$1" i; mkdir -p "$d"; echo pass > "$d/ctl"
  python3 -I "$REPO/e2e/netfault/holdproxy.py" --upstream "$2" --port-file "$d/port" --ctl "$d/ctl" --log "$d/log.jsonl" > "$d/out" 2>&1 &
  echo $! >> "$W/proxies.pid"
  for i in $(seq 1 40); do [ -s "$d/port" ] && break; sleep 0.25; done
  echo "http://127.0.0.1:$(cat "$d/port")"
}
at_ev() { echo -e "$(date -u +%FT%T.%3NZ)\t$*" >> "$AT/events.tsv"; }
at_loop() {
  while :; do
    local out="$AT/rounds/r-$(date +%s%N).json"
    rm -rf "$AT/answers"; mkdir -p "$AT/answers"
    if "$BIN/ferrata_attestor" attest --deployment "$AT/deployment.json" --cert "$AT/cert.json" --att-key "$AT/key" \
         --node "$AT_NODE" --lez "$AT_LEZ" --save "$AT/answers" --slot-from "$AT_FROM" --state "$AT/state.json" --out "$out" \
         >> "$AT/attest.log" 2>&1; then
      echo -e "$(date -u +%T)\tsigned\t$(jq -r .tip_id "$out" 2>/dev/null)" >> "$AT/rounds.tsv"
    else
      rm -f "$out"; echo -e "$(date -u +%T)\trefused\t$(tail -n 1 "$AT/attest.log" | tr '\t' ' ' | cut -c1-200)" >> "$AT/rounds.tsv"
    fi
    sleep 2
  done
}
at_run() { at_loop > /dev/null 2>&1 & echo $! > "$AT/loop.pid"; }
at_pat() { echo "ferrata_attestor attest .*$AT/state.json"; }
at_start() {
  mkdir -p "$AT/rounds"; : > "$AT/events.tsv"
  jq -n --arg c "$(printf '%064x' 11155111)" --arg v "$(echo "$VAULT" | tr 'A-F' 'a-f' | sed 's/^0x//')" --arg l "${CID#0x}" --arg ch "$CHANNEL" \
    '{evm_chain_id: $c, vault: $v, lez_config_id: $l, channel_id: $ch}' > "$AT/deployment.json"
  "$BIN/ferrata_attestor" keygen --out "$AT/key" > "$AT/pub.txt"
  "$BIN/ferrata_attestor" certify --channel-key "$ZONE_STATE/sequencer_home/channel_signing_key" --att-pub "$(cat "$AT/pub.txt")" \
    --deployment "$AT/deployment.json" --out "$AT/cert.json" > "$AT/certify.txt" 2>&1 || { cat "$AT/certify.txt"; die "ferrata_attestor certify"; }
  AT_NODE="$(at_px attestor-node "http://127.0.0.1:$BEDROCK_PORT")"; AT_LEZ="$(at_px attestor-lez "$SEQ")"
  AT_FROM="$(jq -r .tip_slot "$VOL/session-a/channel.json")"; AT_FROM=$(( AT_FROM > 3000 ? AT_FROM - 3000 : 0 ))
  at_run; at_ev start
  say "R.06 attestor: key $(cat "$AT/pub.txt" | cut -c1-16)..., certified by the zone's channel key; node $AT_NODE, LEZ $AT_LEZ"
}
at_kill_mid() { # inside a round when one runs within 10 s
  local i mid=0
  local pat; pat="$(at_pat)"
  for i in $(seq 1 1000); do pgrep -f "$pat" > /dev/null && { mid=1; break; }; sleep 0.01; done
  sleep "0.0$((RANDOM % 6))"
  pkill -9 -f "$pat" 2>/dev/null || mid=0   # nothing to kill: the round had ended
  kill -9 "$(cat "$AT/loop.pid")" 2>/dev/null
  at_ev "kill\tmid-round=$mid\tstate=$(jq -c '{tip_id, slot}' "$AT/state.json" 2>/dev/null | tr -d '\t')"
  sleep 3; at_run; at_ev restart
}
at_chaos() { # while $AT/chaos.stop is absent: kills, a node outage, a LEZ outage, in turn
  local k=0
  while [ ! -e "$AT/chaos.stop" ]; do
    sleep $(( 8 + RANDOM % 8 )); [ -e "$AT/chaos.stop" ] && break
    k=$((k + 1))
    case $(( k % 4 )) in
      1|3) at_kill_mid ;;
      2) echo outage > "$W/holdproxy-attestor-node/ctl"; at_ev "node-outage 20 s"; sleep 20; echo pass > "$W/holdproxy-attestor-node/ctl"; at_ev node-back ;;
      0) echo outage > "$W/holdproxy-attestor-lez/ctl"; at_ev "lez-outage 20 s"; sleep 20; echo pass > "$W/holdproxy-attestor-lez/ctl"; at_ev lez-back ;;
    esac
  done
}
at_stop() {
  pkill -9 -f "$(at_pat)" 2>/dev/null; kill -9 "$(cat "$AT/loop.pid")" 2>/dev/null
  for _ in $(seq 1 20); do pgrep -f "$(at_pat)" > /dev/null || break; sleep 0.25; done
  at_ev stop
}
# Every attestation the attestor wrote, against the checkpoint's blocks (blocks.tsv: id, hash).
at_verdict() {
  python3 -I - "$AT" "$1" <<'PY'
import glob, json, sys
at, blocks_tsv = sys.argv[1], sys.argv[2]
recs, partial = [], 0
for f in sorted(glob.glob(at + "/rounds/r-*.json")):
    try:
        recs.append(json.load(open(f)))
    except Exception:
        partial += 1
blocks = {}
for line in open(blocks_tsv).read().splitlines()[1:]:
    c = line.split("\t")
    blocks[int(c[0])] = c[1].lower().removeprefix("0x")
by = {}
for r in recs:
    by.setdefault(int(r["tip_id"]), set()).add((r["tip_hash"].lower().removeprefix("0x"), r["encoded"]))
double = {k: len(v) for k, v in by.items() if len(v) > 1}
checked = [k for k in by if k in blocks]
off = sorted(k for k in checked for (h, _) in by[k] if h != blocks[k])
ev = [l.split("\t") for l in open(at + "/events.tsv").read().splitlines()]
st = json.load(open(at + "/state.json"))
print(json.dumps({"attestations": len(recs), "partialFiles": partial, "tips": len(by), "firstTip": min(by), "lastTip": max(by),
                  "tipsCheckedAgainstBlocks": len(checked), "doubleSigned": double, "offChain": off,
                  "signers": sorted({r["attestation_key"] for r in recs}),
                  "kills": sum(e[1] == "kill" for e in ev), "killsMidRound": sum(e[1] == "kill" and e[2] == "mid-round=1" for e in ev),
                  "outages": [e[1] for e in ev if "outage" in e[1]], "stateTip": st.get("tip_id", st.get("tip"))}))
PY
}

# The checkpoint over the attestor's attestation, through the prover client.
checkpoint_robust() {
  local t0 t1 tip name att journal i C="$W/checkpoint" f t
  t0="$(jq -r .tip.id "$SA")"; t1="$(head_block)"
  touch "$AT/chaos.stop"; wait "$CHAOS" 2>/dev/null
  say "checkpoint: the attestor's own attestation of a tip at or above $t1 (the zone's head after the user's burns)"
  att=""
  for i in $(seq 1 120); do
    for f in $(ls -1t "$AT/rounds"/r-*.json 2>/dev/null); do
      t="$(jq -r .tip_id "$f" 2>/dev/null)"; [[ "$t" =~ ^[0-9]+$ ]] && [ "$t" -ge "$t1" ] && { att="$f"; break; }
    done
    [ -n "$att" ] && break; sleep 5
  done
  at_stop
  [ -n "$att" ] || die "the attestor signed no tip at or above $t1 in 10 min ($(tail -n 3 "$AT/rounds.tsv" | tr '\n' ' '))"
  tip="$(jq -r .tip_id "$att")"; name="seqnet-$t0-$tip"; cp "$att" "$W/attestation-final.json"
  mkdir -p "$C" "$W/relayer-inbox"
  for i in $(seq 1 30); do
    if node_dump; then
      rm -rf "$C/input"
      RISC0_SERVER_PATH="${R0VM31:-$RISC0_SERVER_PATH}" "$BIN/realblocks-crown" --node "$W/node" --genesis "$ZONE_STATE/bedrock/deployment-settings.yaml" --session-a "$SA" \
        --vault "$VAULT" --config-id "$CID" --genesis-state "$GS" --tip "$tip" --att "$W/attestation-final.json" \
        --name "$name" --out "$C/input" > "$C/realblocks-crown.txt" 2>&1 && break
    fi
    [ "$i" = 30 ] && { tail -n 15 "$C/realblocks-crown.txt"; die "realblocks-crown over the attestor's tip $tip"; }
    sleep 10
  done
  say "realblocks-crown (the attestor's record, --att): $(tail -n 2 "$C/realblocks-crown.txt" | tr '\n' ' ' | cut -c1-300)"
  local v; v="$(at_verdict "$C/input/blocks.tsv")"; fact attestor "$v"
  if [ "$(echo "$v" | jq '.doubleSigned | length')" = 0 ] && [ "$(echo "$v" | jq '.offChain | length')" = 0 ] \
     && [ "$(echo "$v" | jq .killsMidRound)" -ge 1 ] && [ "$(echo "$v" | jq '.outages | length')" -ge 2 ] && [ "$(echo "$v" | jq '.signers | length')" = 1 ]; then
    ok "R.06 attestor: SIGKILLed $(echo "$v" | jq .kills) times ($(echo "$v" | jq .killsMidRound) inside a round) and cut off from its node and its LEZ node ($(echo "$v" | jq -c .outages)); $(echo "$v" | jq .attestations) attestations over $(echo "$v" | jq .tips) tips ($(echo "$v" | jq .firstTip)..$(echo "$v" | jq .lastTip)), one signature per tip, every tip checked ($(echo "$v" | jq .tipsCheckedAgainstBlocks)) the zone's block; its tip $tip taken by the checkpoint"
  else bad "R.06 attestor: $v"; fi
  # The prover: the client and the stand-in service.
  mkdir -p "$PS/requests"; echo pass > "$PS/ctl"
  jq -n --arg id "$CHECKPOINT_B_ID" --arg sha "$(sha256sum "$R0BF/checkpoint_b.bin" | cut -c1-64)" \
    '{"checkpoint-b": ["checkpoint_b.bin", $id, $sha, "checkpoint"]}' > "$PS/pins.json"
  BIN="$BIN" RPC="$RPC" MOCK="$MOCK" DEPLOYER="$DEPLOYER" CHECKPOINT_B_ID="$CHECKPOINT_B_ID" STANDIN_DELAY=15 R0VM31="${R0VM31:-}" \
    python3 -I "$REPO/deploy/testnet/tests/standin_modal/server.py" --port-file "$PS/port" --log "$PS/prover.jsonl" --work "$PS/work" \
    --prove-cmd "bash $REPO/e2e/seqnet/standin-prove.sh" --ctl "$PS/ctl" > "$PS/server.out" 2>&1 &
  local SPID=$!
  for i in $(seq 1 40); do [ -s "$PS/port" ] && break; sleep 0.25; done
  ps_client() {
    PYTHONPATH="$REPO/deploy/testnet/tests/standin_modal" FERRATA_STANDIN_MODAL_URL="http://127.0.0.1:$(cat "$PS/port")" \
    FERRATA_PROVER_PROGRAMS_JSON="$PS/pins.json" FERRATA_PROGRAMS_31="$R0BF" FERRATA_PROVE_LIMIT_S=900 \
      python3 "$REPO/deploy/testnet/host/bin/ferrata-bridge-prover" watch --requests "$PS/requests" --inbox "$W/relayer-inbox" --poll 2 \
      >> "$PS/client.jsonl" 2>&1 &
    echo $! > "$PS/client.pid"
  }
  echo "die-next 4" > "$PS/ctl"   # the first call's prover dies 4 s in
  ps_client
  journal="$(jq -r .journal "$C/input/$name.json")"; journal="0x${journal#0x}"
  jq -n --arg i "$C/input/$name.input" --arg j "$journal" --arg m "$C/input/$name.json" '{kind: "checkpoint-b", input: $i, journal: $j, meta: $m}' \
    > "$PS/requests/.$name.tmp" && mv "$PS/requests/.$name.tmp" "$PS/requests/$name.json"
  say "R.06 prover: request $name to the prover client; its first call's prover dies 4 s in"
  for i in $(seq 1 300); do grep -q '"event": "spawned".*"attempt": 2' "$PS/client.jsonl" 2>/dev/null && break; sleep 0.5; done
  sleep 5; kill -9 "$(cat "$PS/client.pid")" 2>/dev/null; date -u +%T > "$PS/client.killed"
  say "R.06 prover: the client SIGKILLed 5 s into its second call; started again"
  sleep 2; ps_client
  for i in $(seq 1 240); do [ -s "$PS/requests/done/$name.json" ] || [ -s "$PS/requests/failed/$name.json" ] && break; sleep 2; done
  kill -TERM "$(cat "$PS/client.pid")" 2>/dev/null; sleep 3; kill "$SPID" 2>/dev/null
  local dn starts ends jobs
  dn="$(jq -c '{ok, attempts, resumed, call_id, seconds}' "$PS/requests/done/$name.json" 2>/dev/null)"
  starts="$(jq -s -c --arg n "$name" '[.[] | select(.name == $n and .event == "start")] | length' "$PS/prover.jsonl")"
  ends="$(jq -s -c --arg n "$name" '[.[] | select(.name == $n and .event == "end") | {ok, died: (.died // false)}]' "$PS/prover.jsonl")"
  jobs="$(ls "$W/relayer-inbox" | grep -c -F "$name" || true)"
  fact prover "$(jq -n -c --argjson d "${dn:-null}" --argjson s "$starts" --argjson e "$ends" --argjson j "$jobs" '{done: $d, proverStarts: $s, proverEnds: $e, inboxJobs: $j}')"
  if [ "$(echo "$dn" | jq -r .ok)" = true ] && [ "$(echo "$dn" | jq -r .resumed)" = true ] && [ "$(echo "$dn" | jq -r .attempts)" = 2 ] \
     && [ "$starts" = 2 ] && [ "$ends" = '[{"ok":false,"died":true},{"ok":true,"died":false}]' ] && [ "$jobs" = 1 ] \
     && grep -q '"event": "resumed"' "$PS/client.jsonl" && [ ! -e "$PS/requests/failed/$name.json" ]; then
    ok "R.06 prover: its first call died mid-proof and was started again; the client SIGKILLed during the second and started again waited for that same call ($(echo "$dn" | jq -r .call_id)): 2 calls for 1 proof, 1 job in the relayer's inbox, the request done ($dn)"
  else bad "R.06 prover: done $dn; prover starts $starts, ends $ends; inbox jobs $jobs; $(tail -n 4 "$PS/client.jsonl" | tr '\n' ' ' | cut -c1-400)"; fi
  local cand0; cand0="$(cast logs --rpc-url "$RPC" --address "$VAULT" --from-block "$FROM" "$(cast keccak 'CandidateSubmitted(bytes32,bytes32,bytes32,uint64,uint64,bool,uint256,uint64,bytes32,uint32,uint32)')" --json | jq length)"
  relayer_round
  relayer_round   # a second round: the job is done, nothing more is sent
  say "relayer: $(tail -n 3 "$RUN/eth/relayer.jsonl" | jq -c '{event, tx: (.tx // .hash // null), why: (.why // .reason // null)}' | tr '\n' ' ')"
  local cand1; cand1="$(cast logs --rpc-url "$RPC" --address "$VAULT" --from-block "$FROM" "$(cast keccak 'CandidateSubmitted(bytes32,bytes32,bytes32,uint64,uint64,bool,uint256,uint64,bytes32,uint32,uint32)')" --json | jq length)"
  [ "$cand1" = $((cand0 + 1)) ] && ok "R.06 prover: the relayer submitted the client's job once (CandidateSubmitted $cand0 -> $cand1 over two rounds)" \
    || bad "R.06 prover: CandidateSubmitted $cand0 -> $cand1"
  pass_time 3601 "the candidate's wait (Delta 3600 s)"
  relayer_round
  say "vault: finalState $(cast call "$VAULT" 'finalState()(bytes32)' --rpc-url "$RPC"), checkpoints $(cast call "$VAULT" 'checkpointCount()(uint256)' --rpc-url "$RPC")"
  mkdir -p "$RUN/checkpoint" "$RUN/attestor" "$RUN/prover"
  cp "$C/input/$name.json" "$C/input/burns.json" "$C/input/attestor.json" "$C/input/blocks.tsv" "$C/realblocks-crown.txt" "$RUN/checkpoint/" 2>/dev/null || true
  cp "$W/attestation-final.json" "$AT/events.tsv" "$AT/rounds.tsv" "$AT/deployment.json" "$AT/cert.json" "$RUN/attestor/" 2>/dev/null || true
  ( cd "$AT/rounds" && tar -cf - . ) | zstd -q -19 -o "$RUN/attestor/rounds.tar.zst" -f
  cp "$PS/client.jsonl" "$PS/prover.jsonl" "$PS/requests/done/$name.json" "$RUN/prover/" 2>/dev/null || true
  cp "$PS"/work/*/exec.txt "$RUN/prover/" 2>/dev/null || true
}

# The release through the relayer daemon, interrupted before and after its transaction.
release_robust() {
  local job="$1" journal dig PE PU h rel0 rel1 bal0 bal1 inn0 rp i nul
  journal="$(jq -r .journal "$job")"; journal="0x${journal#0x}"
  dig="0x$(python3 -I -c 'import hashlib, sys; print(hashlib.sha256(bytes.fromhex(sys.argv[1])).hexdigest())' "${journal#0x}")"
  cast send "$MOCK" "accept(bytes32,bytes32)" "$RELEASE_ID" "$dig" --rpc-url "$RPC" --unlocked --from "$DEPLOYER" > "$W/accept-release.txt" 2>&1 \
    || { cat "$W/accept-release.txt"; die "the stand-in prover's accept (release)"; }
  PU="$(at_px relayer-eth "$RPC")"; PE="$W/holdproxy-relayer-eth"
  local REL_T; REL_T="$(cast keccak 'Release(bytes32,address,address,uint256,address,uint256,bytes32)')"
  rels() { cast logs --rpc-url "$RPC" --address "$VAULT" --from-block "$FROM" "$REL_T" --json | jq length; }
  rel0="$(rels)"; bal0="$(cast balance "$FRESH" --rpc-url "$RPC")"; inn0="$(cast nonce "$RELAYER" --rpc-url "$RPC")"
  cp "$job" "$W/relayer-inbox/release-robust.json"
  rp_start() { # exec: run in the background (rp_start &), $! is the relayer itself, whom the kills must reach
    exec "$BIN/ferrata-relayer" --eth-rpc "$PU" --vault "$VAULT" --chain-id 11155111 --unlocked "$RELAYER" --from-block "$FROM" \
      --state "$W/relayer" run --inbox "$W/relayer-inbox" --poll-secs 1 "$@" >> "$RUN/eth/relayer.jsonl" 2>> "$W/relayer.err"
  }
  phits() { [ -f "$PE/ctl.hit" ] && wc -l < "$PE/ctl.hit" || echo 0; }
  # 1. after the proof, before the transaction: the release built and recorded, its eth_sendTransaction trapped
  h="$(phits)"; echo "trap eth_sendTransaction" > "$PE/ctl"
  rp_start & rp=$!
  for i in $(seq 1 240); do [ "$(phits)" -gt "$h" ] && break; sleep 0.25; done
  sleep 1; kill -9 "$rp" 2>/dev/null; wait "$rp" 2>/dev/null
  local f1; f1="$(jq -c '{key, nonce, hash}' "$W/relayer/inflight.json" 2>/dev/null)"
  if [ "$(rels)" = "$rel0" ] && [ "$(cast nonce "$RELAYER" --rpc-url "$RPC")" = "$inn0" ] && [ -n "$f1" ] && [ -e "$W/relayer-inbox/release-robust.json" ]; then
    ok "R.07 release: the relayer SIGKILLed with the release built and its transaction trapped before the node (nothing on the chain, nonce $inn0); in flight: $f1"
  else bad "R.07 release: after the trapped send: Release events $(rels) (was $rel0), in flight '$f1'"; fi
  # 2. after the transaction, before it is recorded: sent again with its nonce, mined, the answer held, killed
  echo pass > "$PE/ctl"; h="$(phits)"; echo "hold eth_sendTransaction 40" > "$PE/ctl"
  rp_start & rp=$!
  for i in $(seq 1 240); do [ "$(phits)" -gt "$h" ] && break; sleep 0.25; done
  sleep 3; kill -9 "$rp" 2>/dev/null; wait "$rp" 2>/dev/null
  local f2; f2="$(jq -c '{key, nonce, hash}' "$W/relayer/inflight.json" 2>/dev/null)"
  if [ "$(rels)" = $((rel0 + 1)) ] && [ -n "$f2" ] && [ -e "$W/relayer-inbox/release-robust.json" ]; then
    ok "R.07 release: started again, the relayer sent the lost release with its nonce; mined (Release events $rel0 -> $(rels)), the answer held and the relayer SIGKILLed before it booked it (in flight: $f2)"
  else bad "R.07 release: after the held send: Release events $(rels) (was $rel0), in flight '$f2'"; fi
  # 3. started again, straight through: booked from the chain
  echo pass > "$PE/ctl"
  ( rp_start --rounds 2 )
  rel1="$(rels)"; bal1="$(cast balance "$FRESH" --rpc-url "$RPC")"
  nul="$(cast logs --rpc-url "$RPC" --address "$VAULT" --from-block "$FROM" "$REL_T" --json | jq -c '[.[].topics[1]] | group_by(.) | map(length) | max')"
  local sends; sends="$(jq -s '[.[] | select(.method == "eth_sendTransaction" and (.action | test("^forwarded")))] | length' "$PE/log.jsonl")"
  local found; found="$(tail -n 40 "$RUN/eth/relayer.jsonl" | jq -c 'select(.event == "inflight-found" or .event == "inflight-lost" or .event == "inflight-resent")' 2>/dev/null | jq -s -c 'map(.event)')"
  fact releaseLeg "$(jq -n -c --argjson r0 "$rel0" --argjson r1 "$rel1" --arg b0 "$bal0" --arg b1 "$bal1" --argjson s "$sends" --argjson f "${found:-[]}" --argjson n "${nul:-0}" \
    '{releaseEvents: [$r0, $r1], freshWei: [$b0, $b1], sendsForwarded: $s, relayerEvents: $f, maxReleasesPerNullifier: $n}')"
  if [ "$rel1" = $((rel0 + 1)) ] && [ "$nul" = 1 ] && [ "$sends" = 1 ] && [ ! -e "$W/relayer/inflight.json" ] && [ ! -e "$W/relayer-inbox/release-robust.json" ] \
     && [ "$bal1" != "$bal0" ] && echo "$found" | grep -q inflight-found; then
    ok "R.07 release: started again, the relayer found its release mined under the recorded nonce and booked it (events $found); one release transaction reached the node, one Release (one per nullifier), the fresh address paid $bal0 -> $bal1 wei, nothing in flight"
  else bad "R.07 release: Release events $rel0 -> $rel1, per nullifier $nul, sends forwarded $sends, events $found, in flight $(cat "$W/relayer/inflight.json" 2>/dev/null | head -c 200)"; fi
  cp "$PE/log.jsonl" "$RUN/eth/holdproxy-relayer.jsonl" 2>/dev/null || true
  say "the fresh address $FRESH holds $(cast balance "$FRESH" --rpc-url "$RPC") wei"
}
