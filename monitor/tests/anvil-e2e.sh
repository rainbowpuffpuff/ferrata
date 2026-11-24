#!/usr/bin/env bash
# The watchers' rehearsal on a local anvil chain (no fork, nothing sent to any public network):
# a rule (B) FerrataVault v5 (vault/script/MonitorFixture.s.sol, accept-all verifier) with `monitor`'s
# key as its watcher, and a stand-in honest LEZ node view (blocks 1..20 final, four channel keys).
#
#   S1  an honest candidate passes untouched and is finalised after the wait
#   S2  a candidate whose tip conflicts with the node's final tier is cancelled in time
#   S3  a candidate whose key set is not the live one (n = 3, t = 2 against four keys) is cancelled
#   S4  an honest and a wrong candidate pending together: only the wrong one is cancelled
#   S5  the monitor is killed (SIGKILL) mid-window and restarted from its state file: it still cancels
#       (and while it runs, a second monitor on the same state file is refused)
#   S6  the state file is lost: a fresh monitor rescans the vault and still cancels, nothing else
#   S12 its own endpoint fails (RFP-022 R.6): out while a wrong candidate arrives, the monitor keeps polling and
#       alerting; back, it cancels, and its cancellation's answer lost (the transaction mined): one cancellation
#   S7  a wrong checkpoint finalised while no monitor ran: the watcher alerts, the freeze authority's
#       monitor revokes it within the window
#   S9  failure paths: a dry run only alerts; a watcher without gas money alerts, then cancels once funded
#   S10 clean shutdown on SIGTERM (exit 0, `stopped`, state saved, lock released)
#   S11 two node views in one monitor; the second has forked at the candidate's tip: cancelled
#   S8  a cancelled state submitted again: cancelled again, and the watcher pauses checkpoints (12 h)
#   Rule (A), a second vault (turnSeconds 900, delta 3 h 01 min, so W = 3 h 31 min for t = 3), Δ_g = 2 h:
#   A1  an honest candidate passes untouched and is finalised with a beacon recorded after its wait
#   A2  a candidate on a stale, smaller key set (shorter wait) is cancelled at once (D13)
#   A3  a tip not final by submittedAt + Δ_g: the monitor waits, then cancels before the deadline
#   A4  the node's final tier stalls past --stall-pause-after: alert and the 12 h checkpoint pause
#   R1-R4  offline: the node view from a real LEZ v0.4.0-rc1 sequencer's record (lez/lab), three verdicts
#
# Run from monitor/ (on the build box: mrun --put vault/lib -- 'bash tests/anvil-e2e.sh').
# Results: results/anvil-e2e/ (summary.md, scenarios.json, alerts.jsonl, pager.jsonl, state files).
#
# Through the Logos module (modules/ferrata_monitor_module, loaded in a running logosctl daemon):
#   MONITOR_CMD=modules/scripts/monitor-module.sh MONITOR_E2E_OUT=DIR LOGOSCTL=BIN SESSION=DIR bash tests/anvil-e2e.sh
# runs every scenario with the module in place of the binary; S5's SIGKILL then kills the module's host
# process (monitor-module.sh kill9) and the module is loaded again.
set -euo pipefail

here="$(cd "$(dirname "$0")/.." && pwd)"
vault_dir="$here/../vault"
out="${MONITOR_E2E_OUT:-$here/results/anvil-e2e}"
run="$out/run"
rm -rf "$out"
mkdir -p "$run"
port="${ANVIL_PORT:-8547}"
rpc="http://127.0.0.1:$port"
summary="$out/summary.md"
scen="$out/scenarios.jsonl"
fails=0

if [ -n "${MONITOR_CMD:-}" ]; then
  mon=$(realpath "$MONITOR_CMD")
else
  cargo build --release --locked --quiet
  mon="$here/target/release/monitor"
fi

anvil --port "$port" --chain-id 31337 > "$run/anvil.log" 2>&1 &
anvil_pid=$!
trap 'kill "$anvil_pid" 2>/dev/null || true; [ -n "${mon_pid:-}" ] && kill -9 "$mon_pid" 2>/dev/null || true' EXIT
for _ in $(seq 1 50); do cast block-number --rpc-url "$rpc" > /dev/null 2>&1 && break; sleep 0.2; done

mapfile -t accts < <(cast rpc --rpc-url "$rpc" eth_accounts | jq -r '.[]')
deployer=${accts[0]}; admin=${accts[1]}; prover=${accts[2]}; anyone=${accts[3]}; fees=${accts[4]}
newkey() { cast wallet new | awk '/Private key/ {print $3}'; }
export WATCHER_KEY FREEZE_KEY
WATCHER_KEY=$(newkey); FREEZE_KEY=$(newkey)
watcher=$(cast wallet address --private-key "$WATCHER_KEY")
freezer=$(cast wallet address --private-key "$FREEZE_KEY")
for a in "$watcher" "$freezer"; do cast rpc --rpc-url "$rpc" anvil_setBalance "$a" 0x56bc75e2d63100000 > /dev/null; done # 100 ETH, local only

label() { cast keccak "ferrata/monitor-rehearsal/$1"; }
zero=0x0000000000000000000000000000000000000000000000000000000000000000
genesis=$(label genesis)

deploy_vault() { # MODE_A(true|false): sets vault, deploy_block
  ( cd "$vault_dir" && FIXTURE_MODE_A=$1 FIXTURE_GENESIS=$genesis FIXTURE_ADMIN=$admin FIXTURE_FREEZER=$freezer \
    FIXTURE_WATCHER=$watcher FIXTURE_FEES=$fees forge script script/MonitorFixture.s.sol:MonitorFixture --rpc-url "$rpc" --slow \
    --broadcast --unlocked --sender "$deployer" >> "$run/forge.log" 2>&1 ) || { tail -30 "$run/forge.log"; exit 1; }
  vault=$(jq -r '.transactions[] | select(.contractName=="FerrataVault") | .contractAddress' \
    "$vault_dir/broadcast/MonitorFixture.s.sol/31337/run-latest.json" | tail -1)
  vault=$(cast to-check-sum-address "$vault")
  deploy_block=$(cast block-number --rpc-url "$rpc")
}
deploy_vault false

# The honest node: final tier 1..20, four channel keys from block 0, posting timeframe 900.
view="$out/node-view.json"
blocks='{}'
for i in $(seq 1 20); do blocks=$(jq -c --arg k "$i" --arg h "$(label "block/$i")" '. + {($k): $h}' <<< "$blocks"); done
# Its four channel keys and the attestors' published final-tier messages (examples/attestations.rs): keys 0-2
# attest block 10, key 3 (certified, live: a corrupted attestor) attests a forged block 12.
lez_config=$(cast call --rpc-url "$rpc" "$vault" "lezConfigId()(bytes32)")
att=$(cargo run --release --locked --quiet --example attestations -- 31337 "$vault" "$lez_config" "$(label channel)" \
  10 "$(label block/10)" 12 "$(label forged/12)")
jq -n --argjson b "$blocks" --argjson k "$(jq -c .keys <<< "$att")" --argjson a "$(jq -c .attestations <<< "$att")" \
  '{node: "lez-honest", finalHeight: 20, finalBlocks: $b, keySets: [{fromBlock: 0, keys: $k, postingTimeframe: 900}],
    attestations: $a}' > "$view"

cp_sig="(uint32,uint32,bytes32,bytes32,bytes32,uint64,uint64,bool,bytes32,uint64,bytes32,uint32,uint32)"
# journal PARENT NEWSTATE TIPID TIPHASH N T
journal() { echo "(3,2,$1,$2,$(label "burn-root/$3"),$3,0,false,$zero,$3,$4,$5,$6)"; }
submit() { cast send --unlocked --from "$prover" --rpc-url "$rpc" "$vault" "submitCandidate($cp_sig,bytes)" "$1" 0x > /dev/null; }
finalize() { cast send --unlocked --from "$anyone" --rpc-url "$rpc" "$vault" "finalizeAttested($cp_sig)" "$1" > /dev/null 2>&1; }
submitted_at() { cast call --rpc-url "$rpc" "$vault" "candidates(bytes32)(bytes32,bytes32,uint64,uint32,uint48,uint32,uint64,address)" "$1" | sed -n 5p | awk '{print $1}'; }
final_state() { cast call --rpc-url "$rpc" "$vault" "finalState()(bytes32)"; }
now() { cast block latest --field timestamp --rpc-url "$rpc"; }
warp() { cast rpc --rpc-url "$rpc" evm_increaseTime "$1" > /dev/null; cast rpc --rpc-url "$rpc" evm_mine > /dev/null; }
alerts="$out/alerts.jsonl"
pager="$out/pager.jsonl"
mon_cmd() { # STATE KEYENV: sets margs
  margs=(run --rpc "$rpc" --vault "$vault" --node "file:$view" --state "$1" --key-env "$2" --alert-log "$alerts"
    --alert-cmd "cat >> $pager; echo >> $pager" --alert-level critical --start-block "$deploy_block" --poll 1
    --confirmations 0 --b-lag-grace 300 --margin 120 --channel-id "$(label channel)")
}
monitor_polls() { # STATE KEYENV POLLS
  mon_cmd "$1" "$2"
  "$mon" "${margs[@]}" --max-polls "$3" 2>> "$run/monitor-stderr.log"
}
check() { # NAME CONDITION
  if eval "$2"; then echo "- PASS $1" >> "$summary"; else echo "- FAIL $1" >> "$summary"; fails=$((fails + 1)); fi
}
cancelled_alert() { jq -c --arg s "$1" 'select(.kind=="cancelled" and .newState==$s)' "$alerts"; }
record() { # SCENARIO NEWSTATE: the cancellation's timing from the alert and the receipt's gas
  local a; a=$(cancelled_alert "$2" | tail -1)
  local tx; tx=$(jq -r .tx <<< "$a")
  local gas; gas=$(cast receipt --rpc-url "$rpc" "$tx" gasUsed 2>/dev/null || echo 0)
  jq -c --arg s "$1" --arg gas "$gas" '{scenario: $s, newState, reason: .reason.kind, submittedAt, cancelledAt, deadline,
     secondsAfterSubmission, secondsBeforeDeadline, tx, gasUsed: ($gas | tonumber)}' <<< "$a" >> "$scen" || true
}
state="$out/watcher-state.json"

{
  echo "# monitor on a local anvil chain: $(date -u +%FT%TZ)"
  echo
  echo "$(anvil --version | head -1), chain id 31337; vault $vault (rule B, wait 1 h), watcher $watcher, freeze authority $freezer."
  echo
  if [ -n "${MONITOR_CMD:-}" ]; then
    echo "The monitor ran as the Logos module \`ferrata_monitor_module\` in a headless Logos Core ($("$LOGOSCTL" --version 2>&1 | head -n 1)), driven through \`modules/scripts/monitor-module.sh\`: \`run\` for the bounded runs, \`start\`/\`status\`/\`stop\` for the daemon of S5 and S10."
    echo
  fi
} > "$summary"

# S1: an honest candidate passes untouched.
s0=$genesis
n1=$(label state/1); j1=$(journal "$s0" "$n1" 10 "$(label block/10)" 4 3)
submit "$j1"
monitor_polls "$state" WATCHER_KEY 3
check "S1 honest candidate still pending after 3 polls" '[ "$(submitted_at "$n1")" != 0 ]'
check "S1 monitor passed it" '[ -n "$(jq -c --arg s "$n1" "select(.kind==\"candidate-passed\" and .newState==\$s)" "$alerts")" ]'
check "S1 no cancellation of it" '[ -z "$(cancelled_alert "$n1")" ]'
check "S1 its tip attested by 3 live certified keys (final-tier messages v2 checked with attestor-core)" '[ "$(jq -s --arg s "$n1" "[.[] | select(.kind==\"candidate-passed\" and .newState==\$s)][0].attestations[0].attestedBy" "$alerts")" = 3 ]'
check "S1 the certified key that signed a forged tip raised a critical attestation-finding (conflictingTip)" '[ "$(jq -s "[.[] | select(.kind==\"attestation-finding\" and .level==\"critical\" and .finding.finding==\"conflictingTip\" and .finding.tipId==12)] | length" "$alerts")" = 1 ]'
warp 3601
check "S1 finalised after the wait" 'finalize "$j1" && [ "$(final_state)" = "$n1" ]'

# S2: a conflicting tip is cancelled at once.
n2=$(label state/2); j2=$(journal "$n1" "$n2" 12 "$(label forged/12)" 4 3)
submit "$j2"
monitor_polls "$state" WATCHER_KEY 2
check "S2 conflicting candidate cancelled" '[ "$(submitted_at "$n2")" = 0 ] && [ -n "$(cancelled_alert "$n2")" ]'
check "S2 cancelled before its deadline" '[ "$(cancelled_alert "$n2" | jq .secondsBeforeDeadline)" -gt 0 ]'
record S2 "$n2"
warp 3601
check "S2 cannot be finalised after the wait" '! finalize "$j2"'

# S3: a key set that is not the live one.
n3=$(label state/3); j3=$(journal "$n1" "$n3" 13 "$(label block/13)" 3 2)
submit "$j3"
monitor_polls "$state" WATCHER_KEY 2
check "S3 stale key set cancelled (keySetNotLive)" '[ "$(cancelled_alert "$n3" | jq -r .reason.kind)" = keySetNotLive ]'
record S3 "$n3"

# S4: honest and wrong together.
n4=$(label state/4); j4=$(journal "$n1" "$n4" 14 "$(label block/14)" 4 3)
n5=$(label state/5); j5=$(journal "$n1" "$n5" 15 "$(label forged/15)" 4 3)
submit "$j4"; submit "$j5"
monitor_polls "$state" WATCHER_KEY 2
check "S4 wrong one cancelled" '[ "$(submitted_at "$n5")" = 0 ] && [ -n "$(cancelled_alert "$n5")" ]'
check "S4 honest one untouched" '[ "$(submitted_at "$n4")" != 0 ] && [ -z "$(cancelled_alert "$n4")" ]'
record S4 "$n5"
warp 3601
check "S4 honest one finalised" 'finalize "$j4" && [ "$(final_state)" = "$n4" ]'

# S5: killed mid-window, restarted from its state file.
n6=$(label state/6); j6=$(journal "$n4" "$n6" 30 "$(label block/30)" 4 3) # tip 30: not in the node's final tier
submit "$j6"
mon_cmd "$state" WATCHER_KEY
"$mon" "${margs[@]}" 2>> "$run/monitor-stderr.log" &
mon_pid=$!
for _ in $(seq 1 100); do
  jq -e --arg s "$n6" 'select(.kind=="candidate-waiting" and .newState==$s)' "$alerts" > /dev/null 2>&1 && break
  sleep 0.2
done
rss_pid=$mon_pid
if [ -n "${MONITOR_CMD:-}" ]; then rss_pid=$("$mon" host-pid); fi
rss_kb=$(awk '/^VmRSS:/ {print $2}' "/proc/$rss_pid/status")
polls_before_kill=$(jq .polls "$state")
check "S5 a second monitor on the same state file is refused while the first runs" '! "$mon" "${margs[@]}" --max-polls 1 2> "$run/second.err" && grep -q "held by another monitor" "$run/second.err"'
kill -9 "$mon_pid"; wait "$mon_pid" 2>/dev/null || true; mon_pid=
if [ -n "${MONITOR_CMD:-}" ]; then "$mon" kill9; fi
check "S5 monitor saw it and was killed while waiting (node lag allowance)" '[ -n "$(jq -c --arg s "$n6" "select(.kind==\"candidate-waiting\" and .newState==\$s)" "$alerts")" ] && [ "$(submitted_at "$n6")" != 0 ]'
check "S5 its state file survived the kill and names the candidate" 'jq -e --arg s "$n6" ".candidates[\$s].status == \"pending\"" "$state" > /dev/null'
warp 600 # past the 300 s allowance, 3000 s before the deadline
monitor_polls "$state" WATCHER_KEY 2
check "S5 restarted monitor cancelled it before the deadline" '[ "$(submitted_at "$n6")" = 0 ] && [ "$(cancelled_alert "$n6" | jq .secondsBeforeDeadline)" -gt 0 ]'
check "S5 the reason is tipNotFinal" '[ "$(cancelled_alert "$n6" | jq -r .reason.kind)" = tipNotFinal ]'
record S5 "$n6"

# S6: the state file is lost; a fresh monitor rescans from the deployment block.
n7=$(label state/7); j7=$(journal "$n4" "$n7" 16 "$(label forged/16)" 4 3)
submit "$j7"
mv "$state" "$run/watcher-state-before-loss.json"
before=$(jq -s '[.[] | select(.kind=="cancelled")] | length' "$alerts")
mon_cmd "$state" WATCHER_KEY
/usr/bin/time -o "$out/time-s6.txt" -f "%M %e %U %S" "$mon" "${margs[@]}" --max-polls 2 2>> "$run/monitor-stderr.log"
after=$(jq -s '[.[] | select(.kind=="cancelled")] | length' "$alerts")
check "S6 fresh monitor cancelled the pending wrong candidate" '[ "$(submitted_at "$n7")" = 0 ] && [ -n "$(cancelled_alert "$n7")" ]'
check "S6 and nothing else (one cancellation in that run)" '[ $((after - before)) -eq 1 ]'
check "S6 it audited the two finalised checkpoints as good" '[ "$(jq -r ".finalized[\"0\"], .finalized[\"1\"]" "$state" | sort -u)" = ok ]'
record S6 "$n7"

# S12: the monitor's own endpoint fails (e2e/netfault/holdproxy.py in front of anvil): an outage while a wrong
# candidate arrives, then the endpoint back with the cancellation's answer lost (forwarded and mined, no answer).
px="$run/holdproxy"; mkdir -p "$px"; echo pass > "$px/ctl"
python3 -I "$here/../e2e/netfault/holdproxy.py" --upstream "$rpc" --port-file "$px/port" --ctl "$px/ctl" --log "$px/log.jsonl" \
  > "$px/out" 2>&1 &
px_pid=$!
for _ in $(seq 1 40); do [ -s "$px/port" ] && break; sleep 0.25; done
direct_rpc=$rpc; rpc="http://127.0.0.1:$(cat "$px/port")"; mon_cmd "$state" WATCHER_KEY; rpc=$direct_rpc
"$mon" "${margs[@]}" 2>> "$run/monitor-stderr.log" &
mon_pid=$!
polls0=$(jq .polls "$state"); for _ in $(seq 1 100); do [ "$(jq .polls "$state" 2>/dev/null)" -gt "$polls0" ] 2>/dev/null && break; sleep 0.2; done
echo outage > "$px/ctl"
n12=$(label state/12); j12=$(journal "$n4" "$n12" 16 "$(label forged/12)" 4 3)
submit "$j12"
sleep 15
outages=$(jq -s '[.[] | select(.action == "outage")] | length' "$px/log.jsonl")
check "S12 the monitor kept running through its endpoint's outage ($outages requests refused) and alerted poll-failed" \
  'kill -0 "$mon_pid" 2>/dev/null && [ "$outages" -ge 10 ] && [ -n "$(jq -c "select(.kind==\"poll-failed\")" "$alerts")" ] && [ "$(submitted_at "$n12")" != 0 ]'
cancels0=$(cast logs --rpc-url "$rpc" --address "$vault" --from-block "$deploy_block" "$(cast keccak 'CandidateCancelled(bytes32,uint64,bytes32)')" "$n12" --json | jq length)
echo "lose eth_sendRawTransaction" > "$px/ctl"
for _ in $(seq 1 150); do [ "$(submitted_at "$n12")" = 0 ] && break; sleep 0.2; done
sleep 4
cancels1=$(cast logs --rpc-url "$rpc" --address "$vault" --from-block "$deploy_block" "$(cast keccak 'CandidateCancelled(bytes32,uint64,bytes32)')" "$n12" --json | jq length)
sends=$(jq -s '[.[] | select(.method == "eth_sendRawTransaction" and (.action | test("^forwarded")))] | length' "$px/log.jsonl")
check "S12 back, it cancelled the wrong candidate before the deadline, its cancellation's answer lost: one cancellation ($cancels0 -> $cancels1), one transaction sent ($sends)" \
  '[ "$(submitted_at "$n12")" = 0 ] && [ "$cancels0" = 0 ] && [ "$cancels1" = 1 ] && [ "$sends" = 1 ] && grep -q "forwarded, answer lost" "$px/log.jsonl"'
check "S12 it keeps running after the lost answer and stops cleanly on SIGTERM" 'kill -0 "$mon_pid" 2>/dev/null && kill -TERM "$mon_pid" && wait "$mon_pid"'
mon_pid=
check "S12 its state names the candidate as gone, not pending" 'jq -e --arg s "$n12" ".candidates[\$s].status != \"pending\"" "$state" > /dev/null'
jq -n -c --arg s "$n12" --argjson o "$outages" --argjson c "$cancels1" --argjson t "$sends" \
  '{scenario: "S12", newState: $s, outageRequests: $o, cancellations: $c, cancelSends: $t, status: "cancelled with the answer lost"}' >> "$scen"
kill "$px_pid" 2>/dev/null || true

# S7: a wrong checkpoint finalised while no monitor ran.
n8=$(label state/8); j8=$(journal "$n4" "$n8" 17 "$(label forged/17)" 4 3)
submit "$j8"
warp 3601
check "S7 wrong checkpoint finalised with nobody watching" 'finalize "$j8" && [ "$(final_state)" = "$n8" ]'
monitor_polls "$state" WATCHER_KEY 2
check "S7 watcher raised finalized-wrong with the revocation deadline" '[ "$(jq -s "[.[] | select(.kind==\"finalized-wrong\" and .index==2)] | length" "$alerts")" -ge 1 ]'
freeze_state="$out/freeze-state.json"
monitor_polls "$freeze_state" FREEZE_KEY 2
revoked=$(cast call --rpc-url "$rpc" "$vault" "checkpoints(uint256)(bytes32,bytes32,bytes32,uint64,uint64,bool,bool,uint64,bool,uint64,address)" 2 | sed -n 7p)
check "S7 the freeze authority's monitor revoked checkpoint 2" '[ "$revoked" = true ] && [ "$(final_state)" = "$n4" ]'
jq -c '{scenario: "S7", index, reason: .reason.kind, finalizedAt, revokeBy, revokedAt, tx}' \
  <(jq -c 'select(.kind=="revoked")' "$alerts" | tail -1) >> "$scen"

# S9: the failure paths: a dry run only alerts; a watcher without gas money fails, alerts, and cancels at the next
# poll once funded.
n10=$(label state/10); j10=$(journal "$n4" "$n10" 19 "$(label forged/19)" 4 3)
submit "$j10"
mon_cmd "$state" WATCHER_KEY
"$mon" "${margs[@]}" --dry-run --max-polls 1 2>> "$run/monitor-stderr.log"
check "S9 a dry run raises would-cancel and leaves the candidate" '[ -n "$(jq -c --arg s "$n10" "select(.kind==\"would-cancel\" and .newState==\$s)" "$alerts")" ] && [ "$(submitted_at "$n10")" != 0 ]'
cast rpc --rpc-url "$rpc" anvil_setBalance "$watcher" 0x0 > /dev/null
monitor_polls "$state" WATCHER_KEY 1
check "S9 without gas money: cancel-failed (critical), candidate still pending" '[ -n "$(jq -c --arg s "$n10" "select(.kind==\"cancel-failed\" and .level==\"critical\" and .newState==\$s)" "$alerts")" ] && [ "$(submitted_at "$n10")" != 0 ]'
cast rpc --rpc-url "$rpc" anvil_setBalance "$watcher" 0x56bc75e2d63100000 > /dev/null
monitor_polls "$state" WATCHER_KEY 1
check "S9 funded again: cancelled at the next poll, before the deadline" '[ "$(submitted_at "$n10")" = 0 ] && [ "$(cancelled_alert "$n10" | jq .secondsBeforeDeadline)" -gt 0 ]'
record S9 "$n10"

# S10: clean shutdown: SIGTERM ends the daemon with exit 0, a `stopped` alert and its state saved; the lock is free.
mon_cmd "$state" WATCHER_KEY
"$mon" "${margs[@]}" 2>> "$run/monitor-stderr.log" &
mon_pid=$!
stopped_before=$(jq -s '[.[] | select(.kind=="stopped")] | length' "$alerts")
started_before=$(jq -s '[.[] | select(.kind=="started")] | length' "$alerts")
for _ in $(seq 1 100); do
  [ "$(jq -s '[.[] | select(.kind=="started")] | length' "$alerts")" -gt "$started_before" ] && break
  sleep 0.2
done
sleep 1.5
kill -TERM "$mon_pid"
term_exit=0; wait "$mon_pid" || term_exit=$?; mon_pid=
check "S10 SIGTERM: exit 0, a stopped alert, the state file saved" '[ "$term_exit" = 0 ] && [ "$(jq -s "[.[] | select(.kind==\"stopped\")] | length" "$alerts")" -gt "$stopped_before" ] && jq -e ".polls > 0" "$state" > /dev/null'
check "S10 the lock is free at once: the next monitor starts" 'monitor_polls "$state" WATCHER_KEY 1'

# S11: two node views; the second has forked at the candidate's tip: a contradiction in any own node cancels.
forked="$out/node-view-forked.json"
jq '.node = "lez-forked" | .finalBlocks["19"] = "'"$(label fork/19)"'" | del(.attestations)' "$view" > "$forked"
n11=$(label state/11b); j11=$(journal "$n4" "$n11" 19 "$(label block/19)" 4 3)
submit "$j11"
mon_cmd "$state" WATCHER_KEY
"$mon" "${margs[@]}" --node "file:$forked" --max-polls 2 2>> "$run/monitor-stderr.log"
check "S11 cancelled on the forked node's contradiction (tipConflict in lez-forked)" '[ "$(cancelled_alert "$n11" | jq -r ".reason.kind + \" \" + .reason.node")" = "tipConflict lez-forked" ]'
record S11 "$n11"

# S8: a cancelled state submitted again: cancelled again, then the 12 h pause.
n9=$(label state/9); j9=$(journal "$n4" "$n9" 18 "$(label forged/18)" 4 3)
submit "$j9"
monitor_polls "$state" WATCHER_KEY 2
check "S8 first cancellation" '[ "$(submitted_at "$n9")" = 0 ]'
submit "$j9"
monitor_polls "$state" WATCHER_KEY 2
frozen=$(cast call --rpc-url "$rpc" "$vault" "checkpointsFrozenUntil()(uint256)" | awk '{print $1}')
check "S8 second cancellation and the watcher's pause (about 12 h)" '[ "$(submitted_at "$n9")" = 0 ] && [ $((frozen - $(now))) -gt 43000 ]'
check "S8 a submission is refused during the pause" '! submit "$(journal "$n4" "$(label state/11)" 19 "$(label block/19)" 4 3)" 2>/dev/null'
record S8 "$n9"

# Rule (A): a second vault; the same watcher key, its own state file, the node view without the attestations
# (they were certified for the first vault's deployment).
vault_b=$vault
deploy_vault true
jq 'del(.attestations)' "$view" > "$out/node-view-a.json"
view="$out/node-view-a.json"
state_a="$out/watcher-state-a.json"
check "A0 the second vault is rule (A) (mode 0), turnSeconds 900" '[ "$(cast call --rpc-url "$rpc" "$vault" "mode()(uint8)")" = 0 ] && [ "$(cast call --rpc-url "$rpc" "$vault" "turnSeconds()(uint256)" | awk "{print \$1}")" = 900 ]'
a1=$(label a-state/1); ja1=$(journal "$genesis" "$a1" 10 "$(label block/10)" 4 3)
submit "$ja1"
monitor_polls "$state_a" WATCHER_KEY 2
check "A1 honest candidate passed and untouched" '[ "$(submitted_at "$a1")" != 0 ] && [ -z "$(cancelled_alert "$a1")" ] && [ -n "$(jq -c --arg s "$a1" "select(.kind==\"candidate-passed\" and .newState==\$s)" "$alerts")" ]'
a2=$(label a-state/2); ja2=$(journal "$genesis" "$a2" 11 "$(label block/11)" 3 2)
submit "$ja2"
monitor_polls "$state_a" WATCHER_KEY 2
check "A2 stale key set cancelled at once (keySetNotLive)" '[ "$(submitted_at "$a2")" = 0 ] && [ "$(cancelled_alert "$a2" | jq -r .reason.kind)" = keySetNotLive ]'
record A2 "$a2"
a3=$(label a-state/3); ja3=$(journal "$genesis" "$a3" 30 "$(label block/30)" 4 3)
submit "$ja3"
monitor_polls "$state_a" WATCHER_KEY 2
check "A3 tip not final: waiting until submittedAt + delta_g" '[ "$(submitted_at "$a3")" != 0 ] && [ -n "$(jq -c --arg s "$a3" "select(.kind==\"candidate-waiting\" and .newState==\$s)" "$alerts")" ]'
warp 7201
monitor_polls "$state_a" WATCHER_KEY 2
check "A3 cancelled after delta_g, before its deadline (tipNotFinal)" '[ "$(submitted_at "$a3")" = 0 ] && [ "$(cancelled_alert "$a3" | jq -r .reason.kind)" = tipNotFinal ] && [ "$(cancelled_alert "$a3" | jq .secondsBeforeDeadline)" -gt 0 ]'
record A3 "$a3"
warp 5500 # A1's wait W = 10860 + 2 * 900 = 12660 s has passed
cast send --unlocked --from "$anyone" --rpc-url "$rpc" "$vault" "recordBeacon()" > /dev/null
bidx=$(( $(cast call --rpc-url "$rpc" "$vault" "beaconCount()(uint256)" | awk '{print $1}') - 1 ))
nonce=$(cast call --rpc-url "$rpc" "$vault" "beacons(uint256)(uint256,bytes32)" "$bidx" | sed -n 2p)
ja1e="(3,2,$genesis,$a1,$(label burn-root/10),10,0,false,$nonce,10,$(label block/10),4,3)"
check "A1 finalised with a beacon recorded after its wait" 'cast send --unlocked --from "$anyone" --rpc-url "$rpc" "$vault" "finalize(uint256,$cp_sig,bytes)" "$bidx" "$ja1e" 0x > /dev/null && [ "$(final_state)" = "$a1" ]'
monitor_polls "$state_a" WATCHER_KEY 1
check "A1 audited as good after finalisation" '[ "$(jq -r ".finalized[\"0\"]" "$state_a")" = ok ]'

# A4: the node's final tier has not advanced for longer than --stall-pause-after: under rule (A) the watcher pauses.
mon_cmd "$state_a" WATCHER_KEY
"$mon" "${margs[@]}" --stall-pause-after 600 --max-polls 2 2>> "$run/monitor-stderr.log"
frozen_a=$(cast call --rpc-url "$rpc" "$vault" "checkpointsFrozenUntil()(uint256)" | awk '{print $1}')
check "A4 final tier stalled: alert and the watcher's 12 h pause (rule A)" '[ -n "$(jq -c "select(.kind==\"final-tier-stalled\")" "$alerts")" ] && [ $((frozen_a - $(now))) -gt 43000 ]'

# R: the node view from a real LEZ v0.4.0-rc1 sequencer's record (lez/lab/results/record/chain.tsv), offline.
# The record grows while the lab runs and only appends: R uses its header line and first 258 blocks.
head -n 259 "$here/../lez/lab/results/record/chain.tsv" > "$run/lab-chain-258.tsv"
"$mon" node-view --chain-tsv "$run/lab-chain-258.tsv" --label lab-devnet > "$out/node-view-lab.json"
real_tip=$(jq -r '.finalBlocks["258"]' "$out/node-view-lab.json")
rcand() { # TIPID TIPHASH
  jq -n --arg z "$zero" --argjson id "$1" --arg h "$2" '{newState: $z, parentState: $z, burnRoot: $z, burnCount: 0,
    importedLeafCount: 0, sunset: false, deltaAtSubmit: 3600, tipId: $id, tipHash: $h, n: 1, t: 1, submittedAt: 1000}' > "$run/rcand.json"
  "$mon" check --rule B --candidate "$run/rcand.json" --node "file:$out/node-view-lab.json" --now 1000 | jq -c . | tee -a "$out/real-record-verdicts.jsonl" | jq -r '.verdict + "/" + (.reason.kind // "")'
}
check "R1 the lab record's final tier: 258 blocks" '[ "$(jq .finalHeight "$out/node-view-lab.json")" = 258 ]'
check "R2 a candidate at the real tip 258 passes" '[ "$(rcand 258 "$real_tip")" = "pass/" ]'
check "R3 the same id with another hash is cancelled" '[ "$(rcand 258 "$(label forged/258)")" = "cancel/tipConflict" ]'
check "R4 a tip above the final tier waits" '[ "$(rcand 259 "$(label block/259)")" = "wait/" ]'

# Alerting: every critical alert reached the operator's command.
crit=$(jq -s '[.[] | select(.level=="critical")] | length' "$alerts")
paged=$(grep -c . "$pager" || true)
check "alerts: all $crit critical alerts reached the pager command" '[ "$crit" -eq "$paged" ] && [ "$crit" -gt 0 ]'

{
  echo
  echo "## Cancellations and the revocation (chain time, seconds)"
  echo
  echo '| Scenario | Reason | After submission | Before deadline | cancelCandidate gas |'
  echo '|---|---|---|---|---|'
  jq -r 'select(.scenario != "S7") | "| \(.scenario) | \(.reason) | \(.secondsAfterSubmission) | \(.secondsBeforeDeadline) | \(.gasUsed) |"' "$scen"
  jq -r 'select(.scenario == "S7") | "\nS7: checkpoint \(.index) (\(.reason)) finalised at \(.finalizedAt), revoked at \(.revokedAt), revocable until \(.revokeBy)."' "$scen"
  echo
  echo "Alerts: $(jq -s 'group_by(.kind) | map("\(.[0].kind) \(length)") | join(", ")' "$alerts")."
  echo
  if [ -n "${MONITOR_CMD:-}" ]; then
    echo "Through the Logos module: the module's host process (logos_host, with ferrata_monitor_module loaded) held $rss_kb kB resident (VmRSS) when S5 killed it, after $polls_before_kill polls of the state file's life; the S6 run (2 polls, a full rescan from the deployment block, one cancellation) took $(awk '{print $2 " s wall"}' "$out/time-s6.txt") as the client saw it (logosctl call)."
  else
    echo "Resources (measured, build box): the daemon of S5 held $rss_kb kB resident (VmRSS) when it was killed, after $polls_before_kill polls of its state file's life; the S6 run (2 polls, a full rescan from the deployment block, one cancellation) took $(awk '{print $1 " kB max RSS, " $2 " s wall, " $3 " s user + " $4 " s system CPU"}' "$out/time-s6.txt")."
  fi
  echo
  echo "Under the B wait of 1 h, $(jq -s '[.[] | select(.kind=="cancelled" and .secondsBeforeDeadline != null)] | map(.secondsBeforeDeadline) | min' "$alerts") s was the smallest margin left before a deadline by any cancellation (the restart case)."
  echo
  echo "Failures: $fails"
} >> "$summary"
cp "$state" "$out/watcher-state.json" 2>/dev/null || true
# The alert log names the node file by its absolute path; the committed record names it from the checkout.
sed -i -e 's#"file:[^"]*/monitor/results/#"file:<checkout>/monitor/results/#g' -e 's#"file:[^"]*/modules/results/#"file:<checkout>/modules/results/#g' "$alerts"
cat "$summary"
[ "$fails" -eq 0 ]
