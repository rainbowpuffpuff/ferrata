#!/usr/bin/env bash
# The independent review's attacks on scripts/pubnet.py / pubnet.sh (review-rpc-robust, 18 Nov 2026), kept as
# regression cases, against a local anvil (nothing leaves the machine). liar.py is the misbehaving endpoint; model/ a
# stand-in of the vault's deposit bookkeeping (guarded: like depositETH; open: an effect every time, like a DeployVault
# creation step).
#   bash e2e/netfault/attacks/attacks.sh [case ...]     cases c1 c1b c2 c2b c3 c4 c5 c5b c6 c7 c8 c9 c10 c11, and the
#   review's round 2: v1a v1b v2 v3 v4 v5 g1, and round 3 (attacks on `pubnet.py gone`, liar3.py): g2 g4 g4b g4c g5
#   g6, and g7 (a gone transaction mined after all) (default all)
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

# ---- the review's round 2 (tests/r2/variants.sh), kept ------------------------------------------------------------
dead_url() { echo "http://127.0.0.1:$(free_port)"; } # nothing listens: an endpoint that is down
gone() { # gone <dir> <hash>: the operator's `pubnet.py gone` against the proxy's endpoints
  PUBNET_SEPOLIA_RPC="$PUBNET_SEPOLIA_RPC" python3 -I "$WT/scripts/pubnet.py" gone --journal "$1/net/journal.jsonl" \
    --hash "$2" --sender "$A1" 2>&1 | tail -n 1 | cut -c1-120
}
last_hash() { jq -r 'select(.event == "signed") | .hash' "$1/net/journal.jsonl" | tail -n 1; }
v1a() { # two colluding liars (the same stale block), the honest endpoint down: the guarded deposit replayed
  local d="$S/v1a"; mkdir -p "$d"; anvil_up "$d"; deploy_dep
  local L1 L2; L1="$(liar_up "$d" l1 stale-state)"; L2="$(liar_up "$d" l2 stale-state)"; proxy_up "$d" "$L1,$L2,$R"
  local r1 r2 b; r1="$(stage "$d" deposit-0 'guarded(uint256)' 0)"; b="$(cast block-number --rpc-url "$R")"
  echo "$b" > "$d/l1.ctl"; echo "$b" > "$d/l2.ctl"
  kill -9 "$PUBNET_PID"; proxy_up "$d" "$L1,$L2,$(dead_url)"
  r2="$(stage "$d" deposit-0-again 'guarded(uint256)' 0)"
  verdict v1a "$([ "$(effects)" = 1 ] && [ "$(signed "$d")" = 1 ] && echo 1)" "two colluding liars, honest down, guarded deposit: exits $r1, $r2; sends $(signed "$d"); effects $(effects) (want 1)"
  cleanup
}
v1b() { # the same pair, a step whose calldata carries the sender's nonce: NETWORK.md's documented limit
  local d="$S/v1b"; mkdir -p "$d"; anvil_up "$d"; deploy_dep
  local L1 L2; L1="$(liar_up "$d" l1 stale-state)"; L2="$(liar_up "$d" l2 stale-state)"; proxy_up "$d" "$L1,$L2,$R"
  local r1 r2 b; r1="$(stage "$d" step 'tagged(uint256)' 0)"; b="$(cast block-number --rpc-url "$R")"
  echo "$b" > "$d/l1.ctl"; echo "$b" > "$d/l2.ctl"
  kill -9 "$PUBNET_PID"; proxy_up "$d" "$L1,$L2,$(dead_url)"
  r2="$(stage "$d" step-again 'tagged(uint256)' 0)"
  # Two endpoints lying alike outvote the truth; a replay whose calldata differs is not the same call, so it can go
  # out twice. PASS = the outcome is the one NETWORK.md's Limits state (at most twice), and they state it.
  local doc=0; grep -q 'calldata varies between plans' "$WT/e2e/NETWORK.md" && doc=1
  verdict v1b "$([ "$(effects)" -le 2 ] && [ "$doc" = 1 ] && echo 1)" "LIMIT (documented: $([ "$doc" = 1 ] && echo yes || echo NO)): two colluding liars, honest down, a nonce-tagged step: exits $r1, $r2; sends $(signed "$d"); effects $(effects)"
  cleanup
}
v3() { # a legitimate second call with the same calldata (two equal deposits): refused, naming the remedy
  local d="$S/v3"; mkdir -p "$d"; anvil_up "$d"; deploy_dep; proxy_up "$d" "$R"
  local r1 r2 why; r1="$(stage "$d" deposit-0 'open(uint256)' 0)"; r2="$(stage "$d" deposit-1 'open(uint256)' 1)"
  why="$(grep -h -o 'needs its own journal or distinct calldata' "$d"/deposit-1.log* 2>/dev/null | head -n 1)"
  verdict v3 "$([ "$(effects)" = 1 ] && [ "$r2" = 1 ] && [ -n "$why" ] && echo 1)" "LIMIT (documented): two equal deposits in one journal: exits $r1, $r2; effects $(effects); the refusal names the remedy: ${why:-NO}"
  cleanup
}
v2() { # a reorg below the fence (the deposit's block rolled back): a stop naming gone; after the operator's gone, once
  local d="$S/v2"; mkdir -p "$d"; anvil_up "$d"; deploy_dep; proxy_up "$d" "$R"
  local r1 r2 r3 h g; r1="$(stage "$d" deposit-0 'guarded(uint256)' 0)"; h="$(last_hash "$d")"
  cast rpc anvil_rollback 1 --rpc-url "$R" > /dev/null 2>&1; cast rpc evm_mine --rpc-url "$R" > /dev/null
  PUBNET_STAGE_BUDGET_S=10 r2="$(stage "$d" deposit-0 'guarded(uint256)' 0)"
  local named=0; grep -q 'pubnet.py gone' "$d/deposit-0.stage.err" 2>/dev/null && named=1
  g="$(gone "$d" "$h")"
  PUBNET_STAGE_BUDGET_S=10 r3="$(stage "$d" deposit-0 'guarded(uint256)' 0)"
  verdict v2 "$([ "$r2" != 0 ] && [ "$named" = 1 ] && [ "$r3" = 0 ] && [ "$(effects)" = 1 ] && echo 1)" "rolled back: the stage stops (exit $r2, names gone: $named); gone: $g; the stage again: exit $r3, effects $(effects) (want 1)"
  cleanup
}
v4() { # the same journal on a new chain instance: a stop naming gone; after gone, the step on the new chain, once
  local d="$S/v4"; mkdir -p "$d"; anvil_up "$d"; deploy_dep; proxy_up "$d" "$R"
  local r1 r2 r3 h g; r1="$(stage "$d" step 'open(uint256)' 0)"; h="$(last_hash "$d")"; kill -9 "$PUBNET_PID"; cleanup
  anvil_up "$d"; deploy_dep; proxy_up "$d" "$R"
  PUBNET_STAGE_BUDGET_S=10 r2="$(stage "$d" step 'open(uint256)' 0)"
  local named=0; grep -q 'pubnet.py gone' "$d/step.stage.err" 2>/dev/null && named=1
  g="$(gone "$d" "$h")"
  PUBNET_STAGE_BUDGET_S=10 r3="$(stage "$d" step 'open(uint256)' 0)"
  verdict v4 "$([ "$r2" != 0 ] && [ "$named" = 1 ] && [ "$r3" = 0 ] && [ "$(effects)" = 1 ] && echo 1)" "a fresh chain, the old journal: the stage stops (exit $r2, names gone: $named); gone: $g; the stage again: exit $r3, effects on the new chain $(effects) (want 1)"
  cleanup
}
v5() { # drive.py fork mode's anvil killed within its 30 s state dump after a send, started again from the dump
  local d="$S/v5"; mkdir -p "$d"; local p; p="$(free_port)"; R="http://127.0.0.1:$p"
  anvil --port "$p" --chain-id 31337 --state "$d/state.json" --state-interval 30 > "$d/anvil1.log" 2>&1 & local AP=$!; PIDS+=($AP)
  for _ in $(seq 1 100); do cast chain-id --rpc-url "$R" > /dev/null 2>&1 && break; sleep 0.1; done
  deploy_dep; sleep 31 # the dump holds the model contract, not yet the deposit
  proxy_up "$d" "$R"; local r1 h; r1="$(stage "$d" deposit-0 'guarded(uint256)' 0)"; h="$(last_hash "$d")"
  kill -9 "$AP"; sleep 1
  anvil --port "$p" --chain-id 31337 --state "$d/state.json" --state-interval 30 > "$d/anvil2.log" 2>&1 & PIDS+=($!)
  for _ in $(seq 1 100); do cast chain-id --rpc-url "$R" > /dev/null 2>&1 && break; sleep 0.1; done
  kill -9 "$PUBNET_PID"; proxy_up "$d" "$R"
  local r2 r3 g; PUBNET_STAGE_BUDGET_S=10 r2="$(stage "$d" deposit-0 'guarded(uint256)' 0)"
  local named=0; grep -q 'pubnet.py gone' "$d/deposit-0.stage.err" 2>/dev/null && named=1
  g="$(gone "$d" "$h")"
  PUBNET_STAGE_BUDGET_S=10 r3="$(stage "$d" deposit-0 'guarded(uint256)' 0)"
  verdict v5 "$([ "$r2" != 0 ] && [ "$named" = 1 ] && [ "$r3" = 0 ] && [ "$(effects)" = 1 ] && echo 1)" "anvil reloaded below the fence: the stage stops (exit $r2, names gone: $named); gone: $g; the stage again: exit $r3, effects $(effects) (want 1)"
  cleanup
}
g1() { # gone refuses a transaction still on chain (nothing may be sent twice)
  local d="$S/g1"; mkdir -p "$d"; anvil_up "$d"; deploy_dep; proxy_up "$d" "$R"
  local r1 h g st; r1="$(stage "$d" deposit-0 'guarded(uint256)' 0)"; h="$(last_hash "$d")"
  g="$(gone "$d" "$h")"; st="$(jq -r --arg h "$h" 'select(.hash == $h and .event == "gone") | .event' "$d/net/journal.jsonl" | head -n 1)"
  verdict g1 "$([ -z "$st" ] && [ "$(effects)" = 1 ] && grep -q 'may still be on the chain' <<< "$g" && echo 1)" "gone on a mined transaction still on chain: $g; marked: ${st:-no}"
  cleanup
}

# ---- the review's round 3 (tests/r3/gone.sh): attacks on `pubnet.py gone`, kept -----------------------------------
l3_up() { # l3_up <dir> <name>: a liar3 endpoint (lies switched by files <dir>/<name>.hide|.nlow|.stale|.down); its URL
  local d="$1" n="$2"; python3 -I "$T/liar3.py" --upstream "$R" --ctl "$d/$n" --port-file "$d/$n.port" > "$d/$n.err" 2>&1 & PIDS+=($!)
  waitfile "$d/$n.port"; echo "http://127.0.0.1:$(cat "$d/$n.port")"; }
not_marked() { case "$1" in *"marked gone"*) ;; *) echo 1 ;; esac; }
g2() { # gone on a transaction still in a pool (not mined yet): refused; if it were granted and the tx then mined, the
       # record would lose it
  local d="$S/g2"; mkdir -p "$d"; anvil_up "$d"; deploy_dep; cast rpc evm_setAutomine false --rpc-url "$R" > /dev/null
  proxy_up "$d" "$R"
  (cd "$T/model" && timeout 25 forge script script/Dep.s.sol:DepScript --sig 'guarded(uint256)' 0 --rpc-url "$PUBNET_URL/deposit-0" \
     --sender "$A1" --private-key "$K1" --broadcast --skip-simulation --slow > "$d/first.log" 2>&1)
  local h g; h="$(last_hash "$d")"; g="$(gone "$d" "$h")"
  cast rpc evm_mine --rpc-url "$R" > /dev/null; sleep 1
  local rc=0 rows; PUBNET_RESOLVE_BUDGET_S=5 python3 -I "$WT/scripts/pubnet.py" resolve --network sepolia \
    --journal "$d/net/journal.jsonl" --sender "$A1" --budget 5 > /dev/null 2>&1 || rc=$?
  rows="$(python3 -I "$WT/scripts/pubnet.py" rows --journal "$d/net/journal.jsonl" --stage deposit-0)"
  verdict g2 "$([ "$(not_marked "$g")" = 1 ] && [ "$rows" != '[]' ] && echo 1)" "a tx in the pool, not mined: gone -> '$g'; then mined; resolve exit $rc; the stage's rows ${rows:0:60}"
  cleanup
}
g4() { # two colluding liars hide the receipt and report a nonce one lower, the honest endpoint's receipts are pruned:
       # gone on a MINED deposit must be refused (else the liars serve the old state and the step goes out again)
  local sig="${G4SIG:-guarded(uint256)}" nm="${G4NAME:-g4}"; local d="$S/$nm"; mkdir -p "$d"; anvil_up "$d"; deploy_dep
  local L1 L2 P; L1="$(l3_up "$d" l1)"; L2="$(l3_up "$d" l2)"; P="$(l3_up "$d" p)"; proxy_up "$d" "$L1,$L2,$P"
  local r1 h b g r2; r1="$(stage "$d" deposit-0 "$sig" 0)"; h="$(last_hash "$d")"; b="$(cast block-number --rpc-url "$R")"
  touch "$d/l1.hide" "$d/l1.nlow" "$d/l2.hide" "$d/l2.nlow" "$d/p.hide"
  g="$(gone "$d" "$h")"
  rm -f "$d/l1.nlow" "$d/l2.nlow"; echo "$b" > "$d/l1.stale"; echo "$b" > "$d/l2.stale"
  kill -9 "$PUBNET_PID"; proxy_up "$d" "$L1,$L2,$P"
  r2="$(stage "$d" deposit-0-again "$sig" 0)"
  verdict "$nm" "$([ "$(signed "$d")" = 1 ] && [ "$(not_marked "$g")" = 1 ] && echo 1)" "$sig: two liars + pruned honest receipts: gone -> '$g'; the stage again: exit $r2; sends $(signed "$d") (want 1); effects $(effects)"
  cleanup
}
g4b() { G4SIG='open(uint256)' G4NAME=g4b g4; } # the same, an unguarded step
g4c() { # the same liars, no operator yet: the stage's own resolve must not call the mined deposit lost
  local d="$S/g4c"; mkdir -p "$d"; anvil_up "$d"; deploy_dep
  local L1 L2 P; L1="$(l3_up "$d" l1)"; L2="$(l3_up "$d" l2)"; P="$(l3_up "$d" p)"; proxy_up "$d" "$L1,$L2,$P"
  stage "$d" deposit-0 'guarded(uint256)' 0 > /dev/null
  touch "$d/l1.hide" "$d/l1.nlow" "$d/l2.hide" "$d/l2.nlow" "$d/p.hide"
  local rc=0 msg; msg="$(python3 -I "$WT/scripts/pubnet.py" resolve --network sepolia --journal "$d/net/journal.jsonl" --sender "$A1" --budget 5 2>&1)" || rc=$?
  verdict g4c "$([ "$rc" != 3 ] && echo 1)" "the deposit on chain, resolve through two liars: exit $rc $(grep -o 'not on the chain now' <<< "$msg")"
  cleanup
}
g5() { # gone with one endpoint down (it might have the receipt): refused
  local d="$S/g5"; mkdir -p "$d"; anvil_up "$d"; deploy_dep
  local L1 L2; L1="$(l3_up "$d" l1)"; L2="$(l3_up "$d" l2)"; proxy_up "$d" "$L1,$L2"
  stage "$d" deposit-0 'guarded(uint256)' 0 > /dev/null; local h; h="$(last_hash "$d")"
  touch "$d/l1.hide" "$d/l1.nlow" "$d/l2.down"
  local g; g="$(gone "$d" "$h")"
  verdict g5 "$(not_marked "$g")" "one endpoint down, the other hides the receipt and lies low: gone -> '$g'"
  cleanup
}
g6() { # receipts pruned on every endpoint, nonces honest: the used nonce refuses gone
  local d="$S/g6"; mkdir -p "$d"; anvil_up "$d"; deploy_dep
  local L1 L2; L1="$(l3_up "$d" l1)"; L2="$(l3_up "$d" l2)"; proxy_up "$d" "$L1,$L2"
  stage "$d" deposit-0 'guarded(uint256)' 0 > /dev/null; local h; h="$(last_hash "$d")"
  touch "$d/l1.hide" "$d/l2.hide"
  local g; g="$(gone "$d" "$h")"
  verdict g6 "$(not_marked "$g")" "no endpoint has the receipt, the nonce is used: gone -> '$g'"
  cleanup
}
g7() { # a transaction marked gone (written into the journal as an older check might have) that is mined after all:
       # resolve stops loudly (3) and records it in the journal and the manifest
  local d="$S/g7"; mkdir -p "$d"; anvil_up "$d"; deploy_dep; proxy_up "$d" "$R"
  stage "$d" deposit-0 'guarded(uint256)' 0 > /dev/null; local h; h="$(last_hash "$d")"
  printf '{"event": "gone", "hash": "%s", "why": "test: an older check"}\n' "$h" >> "$d/net/journal.jsonl"
  echo '{"stages": {}}' > "$d/manifest.json"
  local rc=0 msg; msg="$(python3 -I "$WT/scripts/pubnet.py" resolve --network sepolia --journal "$d/net/journal.jsonl" \
    --sender "$A1" --budget 5 --manifest "$d/manifest.json" 2>&1)" || rc=$?
  local rows; rows="$(jq -c '.stages["deposit-0"] | length' "$d/manifest.json")"
  verdict g7 "$([ "$rc" = 3 ] && [ "$rows" = 1 ] && grep -q 'was marked gone' <<< "$msg" && echo 1)" "marked gone, mined after all: resolve exit $rc; manifest rows of the stage $rows; $(grep -o 'was marked gone.*' <<< "$msg" | cut -c1-90)"
  cleanup
}
cases=("$@"); [ ${#cases[@]} -gt 0 ] || cases=(c1 c1b c2 c2b c3 c4 c5 c5b c6 c7 c8 c9 c10 c11 v1a v1b v2 v3 v4 v5 g1 g2 g4 g4b g4c g5 g6 g7)
for c in "${cases[@]}"; do "$c"; done
echo "attacks: $(( ${#cases[@]} - FAILS )) of ${#cases[@]} pass"
[ "$FAILS" = 0 ]
