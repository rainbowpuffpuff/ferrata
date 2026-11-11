#!/usr/bin/env bash
# `monitor` on an anvil fork of Sepolia at the live FerrataVault v5 (rule B) of the round trip of 11 November 2026
# (e2e/runs/crown-live-1). anvil reads Sepolia's state over a public RPC and keeps every transaction to itself:
# nothing is sent to Sepolia. Two changes exist on the fork only:
#   - the monitor's fresh key is written into the vault's watchers[0] slot (the live vault has no watchers);
#   - from F1 on, the verifier's code is replaced by an accept-all stop, so that a forged candidate can be submitted.
# The node view is the devnet's final tip as its attestor recorded it (tip 252), with its one channel key and the
# real final-tier attestation of that run (checkpoint/input/attestor.json).
#
#   F0  the monitor scans the live vault's own events from its deployment block, audits the live checkpoint
#       against the node view (ok), and checks the real attestation (certificate and final-tier message: good)
#   F1  a forged candidate on the live final state (tip 252, another hash) is cancelled at once
#   F2  a candidate on the devnet's real final tip 252 with the live key set (n = t = 1) passes untouched
#
# Run from monitor/ (on the build box: mrun --put vault/lib -- 'bash tests/fork-live-vault.sh').
# FERRATA_FORK_URL: a Sepolia RPC (default publicnode). Results: results/fork-live-vault/.
set -euo pipefail

here="$(cd "$(dirname "$0")/.." && pwd)"
vault_dir="$here/../vault"
rec="$here/../e2e/runs/crown-live-1"
out="$here/results/fork-live-vault"
run="$out/run"
rm -rf "$out"
mkdir -p "$run"
fork_url="${FERRATA_FORK_URL:-https://ethereum-sepolia-rpc.publicnode.com}"
port="${ANVIL_PORT:-8548}"
rpc="http://127.0.0.1:$port"
summary="$out/summary.md"
alerts="$out/alerts.jsonl"
state="$out/watcher-state.json"
view="$out/node-view.json"
fails=0
deploy_block=12111048 # the vault's creation (README.md, "Not audited. Not for production.")

cargo build --release --locked --quiet
mon="$here/target/release/monitor"

anvil --fork-url "$fork_url" --port "$port" > "$run/anvil.log" 2>&1 &
anvil_pid=$!
trap 'kill "$anvil_pid" 2>/dev/null || true' EXIT
for _ in $(seq 1 100); do cast block-number --rpc-url "$rpc" > /dev/null 2>&1 && break; sleep 0.3; done
fork_block=$(cast block-number --rpc-url "$rpc")
vault=$(jq -r .vault "$rec/deploy.json")
check() { # NAME CONDITION
  if eval "$2"; then echo "- PASS $1" >> "$summary"; else echo "- FAIL $1" >> "$summary"; fails=$((fails + 1)); fi
}
{
  echo "# monitor on an anvil fork of Sepolia at the live vault: $(date -u +%FT%TZ)"
  echo
  echo "Fork of chain $(cast chain-id --rpc-url "$rpc") at block $fork_block; vault $vault (deployed at block $deploy_block)."
  echo
} > "$summary"
check "the fork serves the live vault (rule B)" '[ "$(cast call --rpc-url "$rpc" "$vault" "mode()(uint8)")" = 1 ]'

# Fork only: the monitor's fresh key becomes watchers[0].
export WATCHER_KEY
WATCHER_KEY=$(cast wallet new | awk '/Private key/ {print $3}')
watcher=$(cast wallet address --private-key "$WATCHER_KEY")
cast rpc --rpc-url "$rpc" anvil_setBalance "$watcher" 0x56bc75e2d63100000 > /dev/null
slot=$(cd "$vault_dir" && forge inspect FerrataVault storageLayout --json | jq -r '.storage[] | select(.label=="watchers") | .slot')
padded="0x000000000000000000000000$(echo "${watcher#0x}" | tr 'A-F' 'a-f')"
cast rpc --rpc-url "$rpc" anvil_setStorageAt "$vault" "$(printf '0x%064x' "$slot")" "$padded" > /dev/null
check "fork only: watchers[0] (slot $slot) is the monitor's key" '[ "$(cast call --rpc-url "$rpc" "$vault" "watchers(uint256)(address)" 0 | tr A-F a-f)" = "$(echo "$watcher" | tr A-F a-f)" ]'

# The node view from the live run's attestor record.
att="$rec/checkpoint/input/attestor.json"
j() { jq -r "$1" "$att"; }
tip_id=$(j .tip_id)
tip_hash=$(j .tip_hash)
le=$(printf '%016x' "$tip_id" | sed 's/../& /g' | awk '{for (i = NF; i > 0; i--) printf "%s", $i}')
encoded="0x$(j .channel_key | cut -c3-)$(j .attestation_key | cut -c3-)$(j .certificate | cut -c3-)$le$(echo "$tip_hash" | cut -c3-)$(j .final_msg_id | cut -c3-)$(j .signature | cut -c3-)"
jq -n --argjson id "$tip_id" --arg h "$tip_hash" --arg k "$(j .channel_key)" --argjson pt "$(j .posting_timeframe)" --arg a "$encoded" \
  '{node: "crown-devnet", finalHeight: $id, finalBlocks: {($id | tostring): $h},
    keySets: [{fromBlock: 0, keys: [$k], postingTimeframe: $pt}], attestations: [$a]}' > "$view"
channel=$(j .deployment.channel_id)

monitor_polls() { # POLLS
  "$mon" run --rpc "$rpc" --vault "$vault" --node "file:$view" --state "$state" --key-env WATCHER_KEY \
    --alert-log "$alerts" --start-block "$deploy_block" --poll 1 --confirmations 0 --log-chunk 2000 \
    --channel-id "$channel" --max-polls "$1" 2>> "$run/monitor-stderr.log"
}

# F0: the live vault's own history.
monitor_polls 2
count=$(cast call --rpc-url "$rpc" "$vault" "checkpointCount()(uint256)" | awk '{print $1}')
check "F0 the monitor runs as a watcher of the live vault" '[ "$(jq -s "[.[] | select(.kind==\"started\")][0].isWatcher" "$alerts")" = true ]'
check "F0 every live checkpoint ($count) audited, all ok against the devnet's final tip" '[ "$count" -ge 1 ] && [ "$(jq "[.finalized[]] | length" "$state")" = "$count" ] && [ "$(jq -r "[.finalized[]] | unique | join(\",\")" "$state")" = ok ]'
check "F0 the real attestation (certificate, final-tier message v2) is good: no finding" '[ -z "$(jq -c "select(.kind==\"attestation-finding\")" "$alerts")" ]'
check "F0 no poll failed" '[ -z "$(jq -c "select(.kind==\"poll-failed\")" "$alerts")" ]'

# Fork only: an accept-all stop in place of the verifier, so that candidates with chosen journals can be submitted.
verifier=$(cast call --rpc-url "$rpc" "$vault" "verifier()(address)")
code=$(cd "$vault_dir" && forge inspect AcceptAllStop deployedBytecode)
cast rpc --rpc-url "$rpc" anvil_setCode "$verifier" "$code" > /dev/null
mapfile -t accts < <(cast rpc --rpc-url "$rpc" eth_accounts | jq -r '.[]')
prover=${accts[0]}
nmin=$(cast call --rpc-url "$rpc" "$vault" "nMin()(uint32)")
tmin=$(cast call --rpc-url "$rpc" "$vault" "tMin()(uint32)")
parent=$(cast call --rpc-url "$rpc" "$vault" "finalState()(bytes32)")
zero=0x0000000000000000000000000000000000000000000000000000000000000000
cp_sig="(uint32,uint32,bytes32,bytes32,bytes32,uint64,uint64,bool,bytes32,uint64,bytes32,uint32,uint32)"
label() { cast keccak "ferrata/monitor-fork/$1"; }
submit() { # a fixed gas limit: on the fork, cast's estimate left the second submission out of gas (it exits 0 on a revert)
  [ "$(cast send --unlocked --from "$prover" --rpc-url "$rpc" --gas-limit 1000000 --json "$vault" \
    "submitCandidate($cp_sig,bytes)" "$1" 0x | jq -r .status)" = 0x1 ]
}
submitted_at() { cast call --rpc-url "$rpc" "$vault" "candidates(bytes32)(bytes32,bytes32,uint64,uint32,uint48,uint32,uint64,address)" "$1" | sed -n 5p | awk '{print $1}'; }
f1=$(label forged); f2=$(label on-real-tip)
submit "($nmin,$tmin,$parent,$f2,$(label root),1,0,false,$zero,$tip_id,$tip_hash,1,1)"
submit "($nmin,$tmin,$parent,$f1,$(label root),1,0,false,$zero,$tip_id,$(label forged-tip),1,1)"
monitor_polls 2
check "F1 the forged candidate is cancelled (tipConflict) before its deadline" '[ "$(submitted_at "$f1")" = 0 ] && [ "$(jq -s --arg s "$f1" "[.[] | select(.kind==\"cancelled\" and .newState==\$s)][0] | .reason.kind + \" \" + (.secondsBeforeDeadline > 0 | tostring)" "$alerts")" = "\"tipConflict true\"" ]'
check "F2 the candidate on the real final tip passes untouched, attested by the live key" '[ "$(submitted_at "$f2")" != 0 ] && [ "$(jq -s --arg s "$f2" "[.[] | select(.kind==\"candidate-passed\" and .newState==\$s)][0].attestations[0].attestedBy" "$alerts")" = 1 ]'

{
  echo
  echo "Live checkpoints audited: $count; cancellation of F1: $(jq -c --arg s "$f1" 'select(.kind=="cancelled" and .newState==$s) | {secondsAfterSubmission, secondsBeforeDeadline, deadline, tx}' "$alerts")."
  echo
  echo "Alerts: $(jq -s 'group_by(.kind) | map("\(.[0].kind) \(length)") | join(", ")' "$alerts")."
  echo
  echo "Failures: $fails"
} >> "$summary"
cat "$summary"
[ "$fails" -eq 0 ]
