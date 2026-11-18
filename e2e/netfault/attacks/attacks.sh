#!/usr/bin/env bash
# The independent review's attacks on scripts/pubnet.py / pubnet.sh (review-rpc-robust, 18 Nov 2026), kept as
# regression cases, against a local anvil (nothing leaves the machine). liar.py is the misbehaving endpoint; model/ a
# stand-in of the vault's deposit bookkeeping (guarded: like depositETH; open: an effect every time, like a DeployVault
# creation step).
#   bash e2e/netfault/attacks/attacks.sh [case ...]     cases c1 c1b c2 c2b c3 c4 c5 c5b c6 c7 c8 c9 c10 c11 (default all)
# Each case prints PASS (the safe outcome) or FAIL with what was observed; exit 0 if all pass. Scratch:
# $NETFAULT_SCRATCH (default ../tmp/netfault-attacks beside the repository).
set -uo pipefail
T="$(cd "$(dirname "$0")" && pwd)"
WT="$(cd "$T/../../.." && pwd)"
S="${NETFAULT_SCRATCH:-$WT/../tmp/netfault-attacks}"; rm -rf "$S"; mkdir -p "$S"
FAILS=0
export TMPDIR="$S" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
K0=0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
K1=0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d
A1=0x70997970C51812dc3A010C7d01b50e0d17dc79C8
# small budgets so that a case ends in seconds
export PUBNET_REQUEST_BUDGET_S=5 PUBNET_TIMEOUT_S=3 PUBNET_STAGE_BUDGET_S=20 PUBNET_BACKOFF_BASE_S=1 \
  PUBNET_RESOLVE_BUDGET_S=10 PUBNET_REPLACED_AFTER_S=2 PUBNET_ATTEMPT_TIMEOUT_S=60 PUBNET_NETWORK=sepolia PUBNET_FIRST=
. "$WT/scripts/pubnet.sh"
PIDS=()
cleanup() { for p in "${PIDS[@]}"; do kill -9 "$p" 2>/dev/null; done; PIDS=(); }
trap cleanup EXIT
free_port() { python3 -I -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])'; }
waitfile() { for _ in $(seq 1 100); do [ -s "$1" ] && return 0; sleep 0.1; done; return 1; }
anvil_up() { # anvil_up <dir> [anvil args]: R=its URL
  local d="$1"; shift; local p; p="$(free_port)"; R="http://127.0.0.1:$p"
  anvil --port "$p" --chain-id 31337 "$@" > "$d/anvil.log" 2>&1 & PIDS+=($!)
  for _ in $(seq 1 100); do cast chain-id --rpc-url "$R" > /dev/null 2>&1 && break; sleep 0.1; done
}
liar_up() { # liar_up <dir> <name> <mode>: prints its URL
  local d="$1" n="$2" m="$3"
  python3 -I "$T/liar.py" --upstream "$R" --mode "$m" --ctl "$d/$n.ctl" --port-file "$d/$n.port" --log "$d/$n.jsonl" \
    --sender "${A1,,}" > "$d/$n.err" 2>&1 & PIDS+=($!)
  waitfile "$d/$n.port"; echo "http://127.0.0.1:$(cat "$d/$n.port")"
}
proxy_up() { # proxy_up <dir> <endpoints csv>: exports PUBNET_URL PUBNET_DIR PUBNET_SENDER PUBNET_PID
  local d="$1"; rm -f "$d/net/port"; mkdir -p "$d/net"
  PUBNET_SEPOLIA_RPC="$2" python3 -I "$WT/scripts/pubnet.py" proxy --network sepolia --journal "$d/net/journal.jsonl" \
    --sender "$A1" --port-file "$d/net/port" >> "$d/net/proxy.log" 2>&1 & PUBNET_PID=$!; PIDS+=($PUBNET_PID)
  waitfile "$d/net/port" || { echo "proxy did not start"; tail -n 3 "$d/net/proxy.log"; }
  export PUBNET_URL="http://127.0.0.1:$(cat "$d/net/port")" PUBNET_DIR="$d/net" PUBNET_SENDER="$A1" PUBNET_SEPOLIA_RPC="$2"
}
deploy_dep() { DEP="$(cd "$T/model" && forge create src/Dep.sol:Dep --rpc-url "$R" --private-key "$K0" --broadcast 2>/dev/null \
  | grep -o 'Deployed to: 0x[0-9a-fA-F]*' | awk '{print $3}')"; export DEP; }
stage() { # stage <dir> <log name> <sig> <j>: pubnet_stage around forge script, as sepolia.sh's crown() does
  local d="$1" rc=0
  (cd "$T/model" && pubnet_stage "$d/$2.log" forge script script/Dep.s.sol:DepScript --sig "$3" "$4" \
     --rpc-url "$PUBNET_URL/$2" --sender "$A1" --private-key "$K1" --broadcast --skip-simulation --slow) 2> "$d/$2.stage.err" || rc=$?
  echo "$rc"
}
effects() { cast call "$DEP" 'effects()(uint256)' --rpc-url "$R" | awk '{print $1}'; }
signed() { [ -f "$1/net/journal.jsonl" ] && grep -c '"event": "signed"' "$1/net/journal.jsonl" || echo 0; }
statuses() { [ -f "$1/net/journal.jsonl" ] && jq -r 'select(.event == "fate") | "\(.fate):\(.status // "-")"' "$1/net/journal.jsonl" | tr '\n' ' '; }
verdict() { # verdict <case> <ok?> <text>
  if [ "$2" = 1 ]; then echo "PASS  $1: $3"; else echo "FAIL  $1: $3"; FAILS=$((FAILS + 1)); fi
}
( cd "$T/model" && forge build > "$S/model-build.log" 2>&1 ) || { echo "model build failed: $S/model-build.log"; exit 1; }

c1() { # a selective liar (stale contract state at the pinned block, an honest nonce): a stage started again sends twice
  local d="$S/c1"; mkdir -p "$d"; anvil_up "$d"; deploy_dep
  local L; L="$(liar_up "$d" liar stale-state)"; proxy_up "$d" "$L,$R"
  local r1 r2 b pre; r1="$(stage "$d" deposit-0 'guarded(uint256)' 0)"
  b="$(cast block-number --rpc-url "$R")"; echo "$b" > "$d/liar.ctl" # from now on: state at >= b answered at b-1
  pre="$(cast call "$DEP" 'depositCount(address)(uint64)' "$A1" --rpc-url "$PUBNET_URL/drive" | awk '{print $1}')"
  r2="$(stage "$d" deposit-0-again 'guarded(uint256)' 0)"
  local n e; n="$(signed "$d")"; e="$(effects)"
  verdict c1 "$([ "$n" = 1 ] && echo 1)" "selective liar, guarded deposit: first run exit $r1, drive.py's pre-check through the proxy reads depositCount=$pre (truth 1), second run exit $r2; journaled sends $n (want 1); fates: $(statuses "$d"); effects on chain $e"
  cleanup
}
c2() { # the same with a call that has an effect every time (a DeployVault creation step)
  local d="$S/c2"; mkdir -p "$d"; anvil_up "$d"; deploy_dep
  local L; L="$(liar_up "$d" liar stale-state)"; proxy_up "$d" "$L,$R"
  local r1 r2 b; r1="$(stage "$d" step 'open(uint256)' 0)"
  b="$(cast block-number --rpc-url "$R")"; echo "$b" > "$d/liar.ctl"
  r2="$(stage "$d" step-again 'open(uint256)' 0)"
  local n e; n="$(signed "$d")"; e="$(effects)"
  verdict c2 "$([ "$e" = 1 ] && echo 1)" "selective liar, unguarded step: exits $r1, $r2; journaled sends $n; fates: $(statuses "$d"); effects on chain $e (want 1)"
  cleanup
}
c1b() { # the lying endpoint alone (no second endpoint to outvote it): the journal must refuse the repeated step
  local d="$S/c1b"; mkdir -p "$d"; anvil_up "$d"; deploy_dep
  local L; L="$(liar_up "$d" liar stale-state)"; proxy_up "$d" "$L"
  local r1 r2 b; r1="$(stage "$d" deposit-0 'guarded(uint256)' 0)"
  b="$(cast block-number --rpc-url "$R")"; echo "$b" > "$d/liar.ctl"
  r2="$(stage "$d" deposit-0-again 'guarded(uint256)' 0)"
  local n e; n="$(signed "$d")"; e="$(effects)"
  verdict c1b "$([ "$n" = 1 ] && [ "$e" = 1 ] && echo 1)" "the liar alone, guarded deposit: exits $r1, $r2; journaled sends $n (want 1); effects $e; refusal: $(grep -h -o 'this step was sent and mined already[^"]*' "$d"/deposit-0-again.log* "$d/net/proxy.log" 2>/dev/null | head -n 1 | cut -c1-70)"
  cleanup
}
c2b() { # the same, unguarded (a DeployVault creation step): the journal's refusal is the only defence
  local d="$S/c2b"; mkdir -p "$d"; anvil_up "$d"; deploy_dep
  local L; L="$(liar_up "$d" liar stale-state)"; proxy_up "$d" "$L"
  local r1 r2 b; r1="$(stage "$d" step 'open(uint256)' 0)"
  b="$(cast block-number --rpc-url "$R")"; echo "$b" > "$d/liar.ctl"
  r2="$(stage "$d" step-again 'open(uint256)' 0)"
  local n e; n="$(signed "$d")"; e="$(effects)"
  verdict c2b "$([ "$e" = 1 ] && [ "$n" = 1 ] && echo 1)" "the liar alone, unguarded step: exits $r1, $r2; journaled sends $n; effects on chain $e (want 1)"
  cleanup
}
c3() { # control: a consistent liar (one old block for everything, nonce included): the stale plan is refused
  local d="$S/c3"; mkdir -p "$d"; anvil_up "$d"; deploy_dep
  local L; L="$(liar_up "$d" liar stale-all)"; proxy_up "$d" "$L,$R"
  local r1 r2 b; r1="$(stage "$d" step 'open(uint256)' 0)"
  b="$(cast block-number --rpc-url "$R")"; echo "$b" > "$d/liar.ctl"
  PUBNET_STAGE_BUDGET_S=8 r2="$(stage "$d" step-again 'open(uint256)' 0)"
  local n e; n="$(signed "$d")"; e="$(effects)"
  verdict c3 "$([ "$e" = 1 ] && [ "$n" = 1 ] && echo 1)" "consistent liar: exits $r1, $r2; journaled sends $n; effects $e; refusal: $(grep -h -o 'stale plan[^"]*' "$d"/step-again.log* "$d/net/proxy.log" 2>/dev/null | head -n 1 | cut -c1-90)"
  cleanup
}
c4() { # a permanent failure (a revert in the plan, nothing sent) while the journal file does not exist yet
  local d="$S/c4"; mkdir -p "$d/net"; anvil_up "$d"
  export PUBNET_URL="$R" PUBNET_DIR="$d/net" PUBNET_SENDER="$A1" PUBNET_SEPOLIA_RPC="$R"; PUBNET_PID=""
  local t0 rc=0 runs; t0=$(date +%s)
  PUBNET_STAGE_BUDGET_S=12 pubnet_stage "$d/s.log" bash -c 'echo run >> "$0/runs"; echo "Error: script failed: execution reverted: the vault has no code"; exit 1' "$d" 2> "$d/s.err" || rc=$?
  runs="$(wc -l < "$d/runs")"
  local rc2=0; : > "$d/net/journal.jsonl"; rm -f "$d/runs"
  PUBNET_STAGE_BUDGET_S=12 pubnet_stage "$d/s2.log" bash -c 'echo run >> "$0/runs"; echo "Error: script failed: execution reverted: the vault has no code"; exit 1' "$d" 2> "$d/s2.err" || rc2=$?
  verdict c4 "$([ "$rc" = 1 ] && echo 1)" "execution reverted, no journal file: exit $rc (want 1), forge-like command run $runs time(s) in $(( $(date +%s) - t0 )) s; the same with an empty journal file: exit $rc2, run $(wc -l < "$d/runs") time(s)"
  cleanup
}
c5() { # two endpoints one transaction behind, their pending = their latest: the agreed nonce is stale
  local d="$S/c5"; mkdir -p "$d"; anvil_up "$d"
  cast send --private-key "$K1" --rpc-url "$R" 0x000000000000000000000000000000000000dEaD --value 1 > /dev/null # nonce 1 now
  local L1 L2; L1="$(liar_up "$d" n1 nonce-stale)"; L2="$(liar_up "$d" n2 nonce-stale)"; proxy_up "$d" "$L1,$L2,$R"
  local truth agreed raw out out2; truth="$(cast nonce "$A1" --rpc-url "$R")"
  agreed="$(cast rpc eth_getTransactionCount "$A1" pending --rpc-url "$PUBNET_URL/x" | tr -d '"')"
  raw="$(cast mktx --private-key "$K1" --rpc-url "$R" --nonce "$truth" 0x000000000000000000000000000000000000dEaD --value 2)"
  out="$(cast publish "$raw" --rpc-url "$PUBNET_URL/x" 2>&1 | tail -n 1 | cut -c1-140)"
  out2="$(timeout 20 cast send --async --private-key "$K1" --rpc-url "$PUBNET_URL/y" 0x000000000000000000000000000000000000dEaD --value 3 2>&1 | tail -n 1 | cut -c1-80)"
  local wrong; wrong="$(jq -r 'select(.event == "signed") | .nonce' "$d/net/journal.jsonl" 2>/dev/null | tr '\n' ' ')"
  # Two of three endpoints behind alike: no vote can find the truth, so the safe outcome is a transient refusal: no
  # nonce agreed below the chain's, the right transaction not refused as permanent, nothing journaled with nonce 0.
  local ok=1
  case "$agreed" in 0x*) [ "$((agreed))" = "$truth" ] || ok=0 ;; esac
  grep -q 'pubnet: permanent' <<< "$out" && ok=0
  grep -q -w '0' <<< "$wrong" && ok=0
  verdict c5 "$ok" "two endpoints behind, one right (chain nonce $truth): the proxy's pending nonce: ${agreed:0:60}; a tx with the right nonce: $out; cast send: $out2, journaled with nonce(s) [$wrong]"
  cleanup
}
c5b() { # one endpoint behind among three: the agreed nonce is the chain's
  local d="$S/c5b"; mkdir -p "$d"; anvil_up "$d"
  cast send --private-key "$K1" --rpc-url "$R" 0x000000000000000000000000000000000000dEaD --value 1 > /dev/null
  local L1 L2; L1="$(liar_up "$d" n1 nonce-stale)"; L2="$(liar_up "$d" h2 honest)"; proxy_up "$d" "$L1,$L2,$R"
  local truth agreed; truth="$(cast nonce "$A1" --rpc-url "$R")"
  agreed="$(cast rpc eth_getTransactionCount "$A1" pending --rpc-url "$PUBNET_URL/x" | tr -d '"')"
  verdict c5b "$([ "$((agreed))" = "$truth" ] && echo 1)" "one endpoint behind first in the list: chain nonce $truth, agreed $((agreed))"
  cleanup
}
c6() { # every endpoint answers the broadcast with an error, but one of them did take the bytes into its pool
  local d="$S/c6"; mkdir -p "$d"; anvil_up "$d" --no-mining
  local L1 L2; L1="$(liar_up "$d" rf reject-forward)"; L2="$(liar_up "$d" rj reject)"; proxy_up "$d" "$L1,$L2"
  local raw out h st rows rc=0
  raw="$(cast mktx --private-key "$K1" --rpc-url "$R" --nonce 0 0x000000000000000000000000000000000000dEaD --value 2)"
  # --async: the proxy now answers with the hash (the transaction is in a pool: its fate stays open), and this anvil
  # mines only when told
  out="$(timeout 30 cast publish --async "$raw" --rpc-url "$PUBNET_URL/s" 2>&1 | tail -n 1 | cut -c1-100)"
  cast rpc evm_mine --rpc-url "$R" > /dev/null; h="$(cast keccak "$raw")"
  st="$(cast receipt "$h" status --rpc-url "$R" 2>&1 | head -n 1)"
  python3 -I "$WT/scripts/pubnet.py" resolve --network sepolia --journal "$d/net/journal.jsonl" --sender "$A1" --budget 5 2> "$d/resolve.err" || rc=$?
  rows="$(python3 -I "$WT/scripts/pubnet.py" rows --journal "$d/net/journal.jsonl" --stage s)"
  verdict c6 "$([ "$rows" != '[]' ] && echo 1)" "client told: $out; the journal's fate: $(statuses "$d"); on chain: status $st; resolve exit $rc; manifest rows of the stage: $rows"
  cleanup
}
c7() { # the proxy killed (-9) after the journal's signed line, before any endpoint has the bytes; the run started again
  local d="$S/c7"; mkdir -p "$d"; anvil_up "$d"; deploy_dep
  local LH; LH="$(liar_up "$d" hang hang-send)"
  PUBNET_TIMEOUT_S=60 proxy_up "$d" "$LH"
  (cd "$T/model" && timeout 40 forge script script/Dep.s.sol:DepScript --sig 'guarded(uint256)' 0 --rpc-url "$PUBNET_URL/d0" \
     --sender "$A1" --private-key "$K1" --broadcast --skip-simulation --slow > "$d/first.log" 2>&1) &
  local f=$!; for _ in $(seq 1 150); do [ "$(signed "$d")" = 1 ] && break; sleep 0.1; done
  kill -9 "$PUBNET_PID"; kill "$f" 2>/dev/null; wait "$f" 2>/dev/null
  local before; before="$(cast nonce "$A1" --rpc-url "$R")"
  proxy_up "$d" "$R" # the endpoint healthy again
  local r; r="$(stage "$d" d0 'guarded(uint256)' 0)"
  local n e; n="$(signed "$d")"; e="$(effects)"
  verdict c7 "$([ "$e" = 1 ] && [ "$n" = 1 ] && [ "$r" = 0 ] && echo 1)" "killed between journal and broadcast (chain nonce then $before): the stage again exit $r; journaled $n; fates $(statuses "$d"); effects $e (want 1)"
  cleanup
}
c8() { # two runs at once against one deployer, each with its own proxy and journal, the same five deposits
  local d="$S/c8"; mkdir -p "$d/a" "$d/b"; anvil_up "$d" --block-time 1; deploy_dep
  proxy_up "$d/a" "$R"; local UA="$PUBNET_URL" PA="$PUBNET_PID"
  proxy_up "$d/b" "$R"; local UB="$PUBNET_URL" PB="$PUBNET_PID"
  for side in a b; do
    ( if [ $side = a ]; then export PUBNET_URL="$UA" PUBNET_DIR="$d/a/net" PUBNET_PID="$PA"; else export PUBNET_URL="$UB" PUBNET_DIR="$d/b/net" PUBNET_PID="$PB"; fi
      for j in 0 1 2 3 4; do echo "$side $j exit $(PUBNET_STAGE_BUDGET_S=40 stage "$d/$side" "dep-$j" 'guarded(uint256)' "$j")"; done ) > "$d/$side.out" 2>&1 &
  done
  wait $(jobs -p | tail -n 2) 2>/dev/null; sleep 2
  local e c na nb; e="$(effects)"; c="$(cast call "$DEP" 'depositCount(address)(uint64)' "$A1" --rpc-url "$R" | awk '{print $1}')"
  na="$(signed "$d/a")"; nb="$(signed "$d/b")"
  verdict c8 "$([ "$e" = 5 ] && echo 1)" "two runs: effects $e (want 5), depositCount $c, sends a $na + b $nb = $((na + nb)), fates a: $(statuses "$d/a")| b: $(statuses "$d/b")| exits: $(cat "$d/a.out" "$d/b.out" | tr '\n' ' ')"
  cleanup
}
c9() { # an endpoint that invents a receipt (status 1, block 1e9) for a transaction no block holds
  local d="$S/c9"; mkdir -p "$d"; anvil_up "$d" --no-mining
  local L; L="$(liar_up "$d" fr fake-receipt)"; proxy_up "$d" "$L,$R"
  local raw h out bn rc=0
  raw="$(cast mktx --private-key "$K1" --rpc-url "$R" --nonce 0 0x000000000000000000000000000000000000dEaD --value 2)"
  h="$(cast publish --async "$raw" --rpc-url "$PUBNET_URL/s" 2>&1 | tail -n 1)"
  timeout 15 cast receipt "$h" --rpc-url "$PUBNET_URL/s" > /dev/null 2>&1 # one endpoint's receipt is not a receipt
  bn="$(cast block-number --rpc-url "$PUBNET_URL/s" 2>&1 | tail -n 1)"
  python3 -I "$WT/scripts/pubnet.py" resolve --network sepolia --journal "$d/net/journal.jsonl" --sender "$A1" --budget 5 2> "$d/resolve.err" || rc=$?
  kill -9 "$PUBNET_PID"; proxy_up "$d" "$R" # restarted, with the honest endpoint alone
  local bn2 call; bn2="$(timeout 30 cast block-number --rpc-url "$PUBNET_URL/s" 2>&1 | tail -n 1 | cut -c1-80)"
  call="$(timeout 30 cast balance "$A1" --rpc-url "$PUBNET_URL/s" 2>&1 | tail -n 1 | cut -c1-80)"
  local pend; pend="$(cast rpc txpool_status --rpc-url "$R" | jq -r .pending)"
  verdict c9 "$([ "$rc" != 0 ] && [ "$bn" != 1000000000 ] && [ "$bn2" != 1000000000 ] && echo 1)" "fake receipt: the proxy's head then $bn; resolve exit $rc, fate $(statuses "$d")(the tx still pending upstream: pool $pend); after a restart with an honest endpoint: block-number $bn2; balance: $call"
  cleanup
}
c10() { # reads that must be at the head, with the public lag (3 behind): own nonce after a send, a contract just
        # deployed, a transaction sent around the proxy
  local d="$S/c10"; mkdir -p "$d"; anvil_up "$d" --block-time 1
  PUBNET_LAG=3 PUBNET_PIN_TTL_S=3 proxy_up_lag "$d" "$R"
  sleep 5
  local n0 n1 code n2 n3
  cast send --private-key "$K1" --rpc-url "$PUBNET_URL/s" 0x000000000000000000000000000000000000dEaD --value 1 > /dev/null
  n1="$(cast nonce "$A1" --rpc-url "$PUBNET_URL/s")"
  local created; created="$(cast send --private-key "$K1" --rpc-url "$PUBNET_URL/s" --create 0x600a600c600039600a6000f3602a60005260206000f3 --json | jq -r .contractAddress)"
  code="$(cast code "$created" --rpc-url "$PUBNET_URL/s")"
  cast send --private-key "$K1" --rpc-url "$R" 0x000000000000000000000000000000000000dEaD --value 1 > /dev/null # around the proxy
  n2="$(cast nonce "$A1" --rpc-url "$R")"; sleep 4; n3="$(cast nonce "$A1" --rpc-url "$PUBNET_URL/s")"
  verdict c10 "$([ "$n1" = 1 ] && [ "$code" != 0x ] && [ "$n3" = "$n2" ] && echo 1)" "lag 3, re-pin every 3 s: own nonce right after the send $n1 (want 1); a contract just created through the proxy: code ${code:0:12}...; sent around the proxy: chain nonce $n2, through the proxy after the re-pin $n3"
  cleanup
}
proxy_up_lag() { local d="$1"; rm -f "$d/net/port"; mkdir -p "$d/net"
  PUBNET_SEPOLIA_RPC="$2" python3 -I "$WT/scripts/pubnet.py" proxy --network sepolia --journal "$d/net/journal.jsonl" \
    --sender "$A1" --port-file "$d/net/port" --lag "$PUBNET_LAG" >> "$d/net/proxy.log" 2>&1 & PUBNET_PID=$!; PIDS+=($PUBNET_PID)
  waitfile "$d/net/port"; export PUBNET_URL="http://127.0.0.1:$(cat "$d/net/port")" PUBNET_DIR="$d/net" PUBNET_SENDER="$A1"; }
c11() { # a transaction signed by another key, sent through a proxy whose --sender is the deployer
  local d="$S/c11"; mkdir -p "$d"; anvil_up "$d"; proxy_up "$d" "$R"
  local K2=0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3faaed06ba3b3fd5 raw out
  raw="$(cast mktx --private-key "$K2" --rpc-url "$R" --nonce 0 0x000000000000000000000000000000000000dEaD --value 2)"
  out="$(cast publish "$raw" --rpc-url "$PUBNET_URL/s" 2>&1 | tail -n 1 | cut -c1-80)"
  verdict c11 "$([ "$(signed "$d")" = 0 ] && echo 1)" "another key's tx through the deployer's proxy: $out; journaled $(signed "$d") as from $(jq -r 'select(.event=="signed") | .from' "$d/net/journal.jsonl" 2>/dev/null)"
  cleanup
}
cases=("$@"); [ ${#cases[@]} -gt 0 ] || cases=(c1 c1b c2 c2b c3 c4 c5 c5b c6 c7 c8 c9 c10 c11)
for c in "${cases[@]}"; do "$c"; done
echo "attacks: $(( ${#cases[@]} - FAILS )) of ${#cases[@]} pass"
[ "$FAILS" = 0 ]
