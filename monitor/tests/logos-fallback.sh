#!/usr/bin/env bash
# The monitor's `--send-via logos` fallback on a local anvil chain (nothing sent to any public network; RFP-022 R.06):
# what Logos Core's `tx_sender_module` cannot account for goes the plain way when the monitor holds the watcher key
# too. The module is `e2e/netfault/txsender_stub.py`, a stand-in that follows logos-evm-tx-sender-module f1754c3 for
# what the monitor calls (its nonce ledger never hands out a number it burnt: send.rs `reserve`), reached through
# `examples/monitor_logos_stub.rs` (the monitor with Logos Core's call replaced by a client of the stand-in). A rule (B)
# FerrataVault v5 (vault/script/MonitorFixture.s.sol, accept-all verifier) with the monitor's key as its watcher, and
# anvil-e2e.sh's honest node view; each case submits a candidate whose tip conflicts with it (cancelled at once).
#
#   L1  the module's broadcast fails after its leg began to leave (fault `fail send_status`: its history row written,
#       the number n burnt, nothing sent; `failed`, no hash, legs [{nonce: n, left: true}]): the monitor sends the
#       cancel the plain way at its node's next nonce, which is n: mined, the candidate gone
#   L2  the next wrong candidate: cancelled through the module at n + 1, mined (nothing waits behind a gap)
#   L3  stuck with nothing sent (`stuck-unsent send_status`: the row written, n + 2 burnt): plain, at n + 2, mined
#   L4  the next one: through the module at n + 3, mined
#   L5  the module burns again (`fail send_status`: n + 4) and the plain send right after it fails too (the watcher's
#       account empty for that one poll: the node refuses the transaction for its fee); the next run (the account
#       funded again, the module healthy): the module's request takes n + 5, which the node holds queued behind the
#       gap at n + 4; not mined within --tx-timeout, the monitor sends the same cancel the plain way at the pending
#       nonce, n + 4 (the gap filled), and the module's held request is mined after it (reverted: the candidate is
#       gone); the watcher's account ends at n + 6, nothing pending or queued
# Before the fix (an unknown outcome was an error, never the fallback) L1's cancel is not mined: every poll's new
# request takes the module's next number, behind the gap at n. Before the gap fill (chain::gap_to_fill), L5's cancel is
# not mined either: each poll's request is queued behind n + 4 and given up after --tx-timeout.
#
# Run from monitor/ (on the build box: mrun -- 'bash tests/logos-fallback.sh'; Foundry's vault/lib installed).
# Results: target/logos-fallback/ (summary.md, alerts.jsonl, the stand-in's log and history), or $MONITOR_LF_OUT; the
# committed record, results/logos-fallback/ (with its README and before-fix/), is copied from a run by hand.
set -euo pipefail

here="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$here/.." && pwd)"
vault_dir="$repo/vault"
out="${MONITOR_LF_OUT:-$here/target/logos-fallback}"
run="$out/run"
rm -rf "$out"
mkdir -p "$run"
port="${ANVIL_PORT:-8548}"
rpc="http://127.0.0.1:$port"
summary="$out/summary.md"
alerts="$out/alerts.jsonl"
tslog="$out/txsender.jsonl"
fails=0
passes=0

cargo build --release --locked --quiet --example monitor_logos_stub --example attestations
mon="$here/target/release/examples/monitor_logos_stub"

anvil --port "$port" --chain-id 31337 > "$run/anvil.log" 2>&1 &
anvil_pid=$!
ts_pid=
trap 'kill "$anvil_pid" 2>/dev/null || true; [ -n "$ts_pid" ] && kill "$ts_pid" 2>/dev/null || true' EXIT
for _ in $(seq 1 50); do cast block-number --rpc-url "$rpc" > /dev/null 2>&1 && break; sleep 0.2; done

mapfile -t accts < <(cast rpc --rpc-url "$rpc" eth_accounts | jq -r '.[]')
deployer=${accts[0]}; admin=${accts[1]}; prover=${accts[2]}; fees=${accts[4]}
newkey() { cast wallet new | awk '/Private key/ {print $3}'; }
export WATCHER_KEY
WATCHER_KEY=$(newkey); freeze_key=$(newkey)
watcher=$(cast wallet address --private-key "$WATCHER_KEY")
freezer=$(cast wallet address --private-key "$freeze_key")
cast rpc --rpc-url "$rpc" anvil_setBalance "$watcher" 0x56bc75e2d63100000 > /dev/null # 100 ETH, local only
# The stand-in sends with eth_sendTransaction from the watcher's address (anvil signs for it, as keystore_module would).
cast rpc --rpc-url "$rpc" anvil_impersonateAccount "$watcher" > /dev/null

label() { cast keccak "ferrata/monitor-rehearsal/$1"; }
zero=0x0000000000000000000000000000000000000000000000000000000000000000
genesis=$(label genesis)
( cd "$vault_dir" && FIXTURE_MODE_A=false FIXTURE_GENESIS=$genesis FIXTURE_ADMIN=$admin FIXTURE_FREEZER=$freezer \
  FIXTURE_WATCHER=$watcher FIXTURE_FEES=$fees forge script script/MonitorFixture.s.sol:MonitorFixture --rpc-url "$rpc" --slow \
  --broadcast --unlocked --sender "$deployer" >> "$run/forge.log" 2>&1 ) || { tail -30 "$run/forge.log"; exit 1; }
vault=$(jq -r '.transactions[] | select(.contractName=="FerrataVault") | .contractAddress' \
  "$vault_dir/broadcast/MonitorFixture.s.sol/31337/run-latest.json" | tail -1)
vault=$(cast to-check-sum-address "$vault")
deploy_block=$(cast block-number --rpc-url "$rpc")

# anvil-e2e.sh's honest node: final tier 1..20, four channel keys from block 0.
view="$out/node-view.json"
blocks='{}'
for i in $(seq 1 20); do blocks=$(jq -c --arg k "$i" --arg h "$(label "block/$i")" '. + {($k): $h}' <<< "$blocks"); done
lez_config=$(cast call --rpc-url "$rpc" "$vault" "lezConfigId()(bytes32)")
att=$("$here/target/release/examples/attestations" 31337 "$vault" "$lez_config" "$(label channel)" \
  10 "$(label block/10)" 12 "$(label forged/12)")
jq -n --argjson b "$blocks" --argjson k "$(jq -c .keys <<< "$att")" \
  '{node: "lez-honest", finalHeight: 20, finalBlocks: $b, keySets: [{fromBlock: 0, keys: $k, postingTimeframe: 900}]}' > "$view"

cp_sig="(uint32,uint32,bytes32,bytes32,bytes32,uint64,uint64,bool,bytes32,uint64,bytes32,uint32,uint32)"
journal() { echo "(3,2,$1,$2,$(label "burn-root/$3"),$3,0,false,$zero,$3,$4,$5,$6)"; } # PARENT NEWSTATE TIPID TIPHASH N T
submit() { cast send --unlocked --from "$prover" --rpc-url "$rpc" "$vault" "submitCandidate($cp_sig,bytes)" "$1" 0x > /dev/null; }
submitted_at() { cast call --rpc-url "$rpc" "$vault" "candidates(bytes32)(bytes32,bytes32,uint64,uint32,uint48,uint32,uint64,address)" "$1" | sed -n 5p | awk '{print $1}'; }
check() { # NAME CONDITION
  if eval "$2"; then echo "- PASS $1" >> "$summary"; passes=$((passes + 1)); else echo "- FAIL $1" >> "$summary"; fails=$((fails + 1)); fi
}

# The stand-in for tx_sender_module, with a durable history.
ctl="$run/txsender.ctl"
python3 "$repo/e2e/netfault/txsender_stub.py" --upstream "$rpc" --port-file "$run/txsender.port" --ctl "$ctl" \
  --log "$tslog" --history "$out/txsender-history.jsonl" 2> "$run/txsender-stderr.log" &
ts_pid=$!
for _ in $(seq 1 50); do [ -s "$run/txsender.port" ] && break; sleep 0.2; done
export FERRATA_LOGOS_STUB="127.0.0.1:$(cat "$run/txsender.port")"
state="$out/watcher-state.json"
margs=(run --rpc "$rpc" --vault "$vault" --node "file:$view" --state "$state" --key-env WATCHER_KEY --alert-log "$alerts"
  --start-block "$deploy_block" --poll 1 --confirmations 0 --b-lag-grace 300 --margin 120
  --send-via logos --approval-timeout 5 --tx-timeout 8)

{
  echo "# monitor --send-via logos: the fallback for what tx_sender_module cannot account for ($(date -u +%FT%TZ))"
  echo
  echo "$(anvil --version | head -1), chain id 31337; vault $vault (rule B, wait 1 h), watcher $watcher (its key held by"
  echo "the monitor, so the fallback is on; the stand-in sends from the same address). Each case: one candidate whose tip"
  echo "conflicts with the node's final tier, then \`monitor run\` for 3 polls (1 s apart) through the stand-in."
  echo
} > "$summary"

hits() { [ -f "$ctl.hit" ] && wc -l < "$ctl.hit" || echo 0; }
# (jq -s on a missing file prints [] and fails: test for the file first.)
requests() { if [ -f "$tslog" ]; then jq -s -c '[.[] | select(.method == "send" and .requestId != null)]' "$tslog"; else echo '[]'; fi; }
cancelled() { { jq -c --arg s "$1" 'select(.kind == "cancelled" and .newState == $s)' "$alerts" 2>/dev/null || true; } | tail -n 1; }
failed_alerts() { if [ -f "$alerts" ]; then jq -s --arg s "$1" '[.[] | select(.kind == "cancel-failed" and .newState == $s)] | length' "$alerts"; else echo 0; fi; }
nonce0=$(cast nonce --rpc-url "$rpc" "$watcher")
rows=()

case_run() { # NAME FAULT NEWSTATE: sets h0, r0 (fault hits and requests before)
  h0=$(hits); r0=$(requests | jq length)
  echo "$2" > "$ctl"
  submit "$(journal "$genesis" "$3" 12 "$(label forged/12)" 4 3)"
  "$mon" "${margs[@]}" --max-polls 3 2>> "$run/monitor-stderr.log" || echo "monitor run exited $? ($1)" >> "$run/monitor-stderr.log"
  echo pass > "$ctl"
  a=$(cancelled "$3"); [ -n "$a" ] || a='{}'
  tx=$(jq -r '.tx // empty' <<< "$a")
  txn=none; [ -z "$tx" ] || txn=$(cast tx --rpc-url "$rpc" "$tx" nonce 2>/dev/null || echo none)
  req=$(requests | jq -c --argjson r "$r0" '.[$r:]')
  reqn=$(jq -r '.[0].nonce // "none"' <<< "$req")
  path=$(jq -r 'if .via.fallback then "plain (fallback)" else .via.path end // "none"' <<< "$a")
  rows+=("| $1 | \`$2\` | $(jq length <<< "$req") | $reqn | $path | $txn | $(failed_alerts "$3") |")
}

# L1: failed after its leg began to leave.
n1=$(label logos-fallback/1)
case_run L1 "fail send_status" "$n1"
check "L1 the fault fired: tx_sender_module answered failed, no hash, its leg left at nonce $nonce0 (burnt)" \
  '[ "$(hits)" -gt "$h0" ] && jq -e "select(.method == \"send_status\" and .action == \"failed\")" "$tslog" > /dev/null && [ "$reqn" = "$nonce0" ]'
check "L1 cancelled the plain way (the fallback, its reason the module's failed request): the candidate gone" \
  '[ "$(submitted_at "$n1")" = 0 ] && [ "$(jq -r .via.fallback <<< "$a")" = true ] && jq -r .via.reason <<< "$a" | grep -q "failed"'
check "L1 the plain cancel was mined at the nonce the module burnt ($nonce0): the gap filled" '[ "$txn" = "$nonce0" ]'
check "L1 one request through the module, no cancel-failed alert" '[ "$(jq length <<< "$req")" = 1 ] && [ "$(failed_alerts "$n1")" = 0 ]'

# L2: the next one goes through the module and is mined.
n2=$(label logos-fallback/2)
case_run L2 pass "$n2"
check "L2 cancelled through tx_sender_module at nonce $((nonce0 + 1)), mined" \
  '[ "$(submitted_at "$n2")" = 0 ] && [ "$(jq -r .via.path <<< "$a")" = logos ] && [ "$reqn" = $((nonce0 + 1)) ] && [ "$txn" = $((nonce0 + 1)) ]'

# L3: stuck, nothing sent.
n3=$(label logos-fallback/3)
case_run L3 "stuck-unsent send_status" "$n3"
check "L3 the fault fired (stuck, nothing sent, nonce $((nonce0 + 2)) burnt); cancelled the plain way at that nonce" \
  '[ "$(hits)" -gt "$h0" ] && [ "$reqn" = $((nonce0 + 2)) ] && [ "$(submitted_at "$n3")" = 0 ] && [ "$(jq -r .via.fallback <<< "$a")" = true ] && [ "$txn" = $((nonce0 + 2)) ] && [ "$(failed_alerts "$n3")" = 0 ]'

# L4: and the next one through the module again.
n4=$(label logos-fallback/4)
case_run L4 pass "$n4"
check "L4 cancelled through tx_sender_module at nonce $((nonce0 + 3)), mined" \
  '[ "$(submitted_at "$n4")" = 0 ] && [ "$(jq -r .via.path <<< "$a")" = logos ] && [ "$txn" = $((nonce0 + 3)) ]'
# L5: a burnt number and a failed plain send after it (the gap README "With --send-via logos" called not handled).
n5=$(label logos-fallback/5)
h0=$(hits); r0=$(requests | jq length)
fund=$(cast balance --rpc-url "$rpc" "$watcher")
echo "fail send_status" > "$ctl"
submit "$(journal "$genesis" "$n5" 12 "$(label forged/12)" 4 3)"
cast rpc --rpc-url "$rpc" anvil_setBalance "$watcher" 0x0 > /dev/null
"$mon" "${margs[@]}" --max-polls 1 2>> "$run/monitor-stderr.log" || echo "monitor run exited $? (L5, the account empty)" >> "$run/monitor-stderr.log"
burnt=$(requests | jq -r --argjson r "$r0" '.[$r:][0].nonce // "none"')
check "L5 the module burnt nonce $burnt (failed, nothing sent) and the plain send after it failed (the account empty): the candidate still pending, nothing mined" \
  '[ "$(hits)" -gt "$h0" ] && [ "$burnt" = $((nonce0 + 4)) ] && [ "$(submitted_at "$n5")" != 0 ] && [ "$(cast nonce --rpc-url "$rpc" "$watcher" --block latest)" = $((nonce0 + 4)) ]'
cast rpc --rpc-url "$rpc" anvil_setBalance "$watcher" "$(cast to-hex "$fund")" > /dev/null
echo pass > "$ctl"
"$mon" "${margs[@]}" --max-polls 3 2>> "$run/monitor-stderr.log" || echo "monitor run exited $? (L5, funded again)" >> "$run/monitor-stderr.log"
a=$(cancelled "$n5"); [ -n "$a" ] || a='{}'
tx=$(jq -r '.tx // empty' <<< "$a")
txn=none; [ -z "$tx" ] || txn=$(cast tx --rpc-url "$rpc" "$tx" nonce 2>/dev/null || echo none)
req=$(requests | jq -c --argjson r "$r0" '.[$r:]')
held=$(jq -r '.[1].nonce // "none"' <<< "$req")
check "L5 then the module's next request ($held) was held behind the gap, and the cancel went the plain way at the burnt nonce $burnt, mined: the gap filled, the candidate gone" \
  '[ "$held" = $((nonce0 + 5)) ] && [ "$(submitted_at "$n5")" = 0 ] && [ "$txn" = "$burnt" ] && [ "$(jq -r .via.gapFill <<< "$a")" = true ] && [ "$(jq -r .via.nonce <<< "$a")" = "$burnt" ]'
path=$(jq -r 'if .via.gapFill then "plain (gap fill)" elif .via.fallback then "plain (fallback)" else .via.path end // "none"' <<< "$a")
rows+=("| L5 | \`fail send_status\`, the account empty for one poll; then \`pass\` | $(jq length <<< "$req") | $burnt, $held | $path | $txn | $(failed_alerts "$n5") |")
# The module's held request became ready when the gap fill was mined; anvil 1.3.6 (CI's) mines on a submission, not on
# such a promotion, so it waits for the next block as on a real chain: mine one (1.5.1 had already included it).
cast rpc --rpc-url "$rpc" evm_mine > /dev/null
latest=$(cast nonce --rpc-url "$rpc" "$watcher" --block latest)
pending=$(cast nonce --rpc-url "$rpc" "$watcher" --block pending)
# anvil's pending nonce does not count transactions queued behind a gap: ask its pool too (review-integ9d L6).
queued=$(cast rpc --rpc-url "$rpc" txpool_status | jq -r '.queued' | xargs printf '%d')
check "the watcher's account: $((nonce0 + 6)) transactions mined (L5's held request after the gap fill), nothing pending or queued behind a gap (latest $latest, pending $pending, queued $queued)" \
  '[ "$latest" = $((nonce0 + 6)) ] && [ "$pending" = "$latest" ] && [ "$queued" = 0 ]'

# The node view's path in the alerts, without the checkout's location (as anvil-e2e.sh).
sed -i -e 's#"file:[^"]*/monitor/\(results\|target\)/#"file:<checkout>/monitor/\1/#g' "$alerts"
{
  echo
  echo "| Case | Fault armed | Requests to the module | Request's nonce | The cancel went | Its nonce (mined) | cancel-failed alerts |"
  echo "|---|---|---|---|---|---|---|"
  printf '%s\n' "${rows[@]}"
  echo
  echo "$passes passed, $fails failed."
} >> "$summary"
cat "$summary"
# On a failure, what the monitor and the stand-in said (they write to files the summary does not show), so that a CI
# failure names its cause (28 November 2026: all ten checks failed once on GitHub, L1 without a request; the same
# tree passed on GitHub on another branch and locally on anvil 1.3.6 and 1.5.1).
if [ "$fails" -gt 0 ]; then
  for f in monitor-stderr.log txsender-stderr.log anvil.log; do
    [ -s "$run/$f" ] && { echo; echo "--- last 40 lines of $f"; tail -n 40 "$run/$f" | while IFS= read -r l; do printf '%s\n' "${l//"$repo"/<checkout>}"; done; }
  done
fi
exit "$fails"
