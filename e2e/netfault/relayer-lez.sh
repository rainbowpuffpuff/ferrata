#!/usr/bin/env bash
# The relayer's root imports with the LEZ side failing under it (RFP-022 R.06): ferrata-relayer --import-roots on the
# mini-app's devnet (cli/examples/miniapp_devnet.rs: FerrataVault v5 on anvil, chain id 1; the zone in the LEZ harness
# with the bridge program, RISC Zero dev mode), its LEZ calls through e2e/netfault/holdproxy.py, whose faults this
# script sets as it goes. Local only. Each scenario ends with the zone's imported leaf counts checked: every count
# imported once, none twice.
#
#   A  outage: every LEZ call fails (503 or a closed connection) for a round; nothing imported, the round's failure
#      logged; the endpoint back, the next round imports n = 2
#   B  answer lost: the ImportRoot reaches the zone and is applied, its answer is lost (connection closed); the
#      daemon keeps it in flight; the next round finds root(cid, 3) written and books it ("recovered"), sends nothing
#   C  killed while the answer is held: the import reaches the zone, the daemon is SIGKILLed before the answer
#      comes; restarted, it books the import from its record, sends nothing
#   D  killed while the import is trapped (never sent): the daemon is SIGKILLed with its ImportRoot held unsent;
#      restarted (--import-wait 10), it waits out its record, then plans and sends the import once
#
#   e2e/netfault/relayer-lez.sh OUT_DIR
#
# Needs cargo, Foundry (anvil, forge) and vault/lib, python3, curl, jq. Writes OUT_DIR/summary.md (PASS/FAIL lines),
# OUT_DIR/results.json, the daemon's lines (OUT_DIR/rounds/), the proxy's log.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
mkdir -p "$1"
out=$(cd "$1" && pwd)
PORT=${PORT:-$((18900 + $$ % 300))}
ANVIL_PORT=${ANVIL_PORT:-$((19500 + $$ % 300))}
export PATH="$HOME/.foundry/bin:$HOME/.risc0/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
if [ -z "${RISC0_HOME:-}" ] && [ -d "$repo/../risc0-home-lez" ]; then export RISC0_HOME="$(cd "$repo/../risc0-home-lez" && pwd)"; fi
rm -rf "$out/rounds" "$out/rep" "$out/devnet" "$out/relayer-state" "$out/inbox" "$out/proxy".*
mkdir -p "$out/rounds" "$out/rep" "$out/devnet" "$out/inbox"
summary="$out/summary.md"
printf '# relayer-lez: root imports with the LEZ side failing (R.06)\n\n' > "$summary"
pass=0; fail=0
ok() { echo "- PASS $*" | tee -a "$summary"; pass=$((pass + 1)); }
bad() { echo "- FAIL $*" | tee -a "$summary"; fail=$((fail + 1)); }
check() { local name=$1; shift; if "$@"; then ok "$name"; else bad "$name"; fi; }

if ! (cd "$repo/cli" && cargo build --bin ferrata-relayer --example miniapp_devnet) > "$out/build.log" 2>&1; then
  bad "build: $(grep -m 3 -E '^error' "$out/build.log" | tr '\n' ' ')"; exit 1
fi
(cd "$repo/vault" && { [ -d lib/risc0-ethereum ] || forge install --no-git foundry-rs/forge-std@v1.10.0 OpenZeppelin/openzeppelin-contracts@v5.4.0 risc0/risc0-ethereum@v3.1.0; } && forge build --quiet) > "$out/forge.log" 2>&1
R="$repo/cli/target/debug/ferrata-relayer"
RISC0_DEV_MODE=1 FERRATA_DEVNET_DIR="$out/devnet" FERRATA_DEVNET_PORT=$PORT FERRATA_DEVNET_ANVIL_PORT=$ANVIL_PORT \
  "$repo/cli/target/debug/examples/miniapp_devnet" > "$out/devnet.log" 2>&1 &
devnet=$!
echo pass > "$out/proxy.ctl"
python3 -I "$here/holdproxy.py" --upstream "http://127.0.0.1:$PORT" --port-file "$out/proxy.port" --ctl "$out/proxy.ctl" \
  --log "$out/proxy.jsonl" > "$out/proxy.out" 2>&1 &
proxy=$!
daemon=""
stop_all() {
  [ -n "$daemon" ] && kill -9 "$daemon" 2>/dev/null
  curl -s -d '{"method":"shutdown","params":{}}' "http://127.0.0.1:$PORT/rpc" > /dev/null 2>&1 || true
  sleep 1; kill "$devnet" "$proxy" 2>/dev/null || true
}
trap stop_all EXIT
for _ in $(seq 1 480); do curl -sf "http://127.0.0.1:$PORT/" > /dev/null && break; sleep 0.5; done
for _ in $(seq 1 40); do [ -s "$out/proxy.port" ] && break; sleep 0.25; done
PROXY=$(cat "$out/proxy.port")
dev() { # NAME METHOD [PARAMS]: straight to the devnet (the test's own reads and the stand-in's writes)
  local body; body=$(jq -cn --arg m "$2" --argjson p "${3:-{\}}" '{method: $m, params: $p}')
  curl -s -d "$body" "http://127.0.0.1:$PORT/rpc" > "$out/rep/$1.raw"
  jq -e '.ok == true' "$out/rep/$1.raw" > /dev/null || { bad "devnet $2: $(head -c 300 "$out/rep/$1.raw")"; return 1; }
  jq -c '.result' "$out/rep/$1.raw" > "$out/rep/$1.json"
}
dev info info && dev inputs0 module.inputs || exit 1
PROGRAM=$(jq -r .program "$out/rep/inputs0.json"); VAULT=$(jq -r .vault "$out/rep/inputs0.json")
A=(--eth-rpc "http://127.0.0.1:$ANVIL_PORT" --vault "$VAULT" --chain-id 1
   --keystore "$out/devnet/keystore-relayer.json" --password-file "$out/devnet/password.txt"
   --state "$out/relayer-state" --lez-devnet "http://127.0.0.1:$PROXY/rpc" --bridge-account "$PROGRAM"
   --import-roots --import-every 0)
round() { # NAME [OPTIONS...]: one round
  local n=$1; shift
  "$R" "${A[@]}" "$@" run --inbox "$out/inbox" --rounds 1 --poll-secs 0 > "$out/rounds/$n.jsonl" 2> "$out/rounds/$n.err"
}
start() { # NAME [OPTIONS...]: the daemon in the background, one round a second
  local n=$1; shift
  "$R" "${A[@]}" "$@" run --inbox "$out/inbox" --poll-secs 1 > "$out/rounds/$n.jsonl" 2> "$out/rounds/$n.err" &
  daemon=$!
}
ev() { jq -s -c --arg e "$2" 'map(select(.event == $e))' "$out/rounds/$1.jsonl"; }
nev() { jq -s --arg e "$2" 'map(select(.event == $e)) | length' "$out/rounds/$1.jsonl"; }
imported() { dev "inputs_$1" module.inputs > /dev/null && jq -c '.imported' "$out/rep/inputs_$1.json"; }
applied() { dev "applied_$1" zone.applied "{\"program\":\"$PROGRAM\"}" > /dev/null && jq '.applied | length' "$out/rep/applied_$1.json"; }
hits() { [ -f "$out/proxy.ctl.hit" ] && wc -l < "$out/proxy.ctl.hit" || echo 0; }
wait_hit() { local before=$1 i; for i in $(seq 1 240); do [ "$(hits)" -gt "$before" ] && return 0; sleep 0.25; done; return 1; }
submits_after() { # LINE: module.submit calls the proxy forwarded after line LINE of its log
  tail -n +"$(( $1 + 1 ))" "$out/proxy.jsonl" | jq -s 'map(select(.method == "module.submit" and (.action | startswith("forwarded")))) | length'
}
plog() { wc -l < "$out/proxy.jsonl"; }

# ---- A: outage ---------------------------------------------------------------------------------------------
dev d0 deposit '{"kind":"private","denomination":0,"account":0}'
dev d1 deposit '{"kind":"private","denomination":0,"account":0}'
dev f1 lc.finalize
n0=$(applied a0)
echo outage > "$out/proxy.ctl"
round a1
echo pass > "$out/proxy.ctl"
n1=$(applied a1)
check "A: outage: the round's LEZ calls fail ($(jq -s -c 'map(select(.event | test("failed|refused"))) | .[0] | {event, reason: (.reason | .[0:100])}' "$out/rounds/a1.jsonl")), nothing imported" \
  bash -c "[ $n1 = $n0 ] && [ \"\$(jq -s 'map(select(.event | test(\"failed|refused\"))) | length' '$out/rounds/a1.jsonl')\" -ge 1 ] && [ $(nev a1 root-imported) = 0 ]"
round a2
check "A: the endpoint back: ImportRoot n = 2 applied once ($(ev a2 root-imported | jq -c '.[0] | {leafCount, applied}'))" \
  bash -c "[ \"\$(jq -s 'map(select(.event == \"root-imported\" and .leafCount == 2 and .applied == true)) | length' '$out/rounds/a2.jsonl')\" = 1 ] && [ '$(imported a2)' = '[2]' ] && [ $(applied a2b) = $((n0 + 1)) ]"

# ---- B: answer lost --------------------------------------------------------------------------------------
dev d2 deposit '{"kind":"private","denomination":0,"account":0}'
dev f2 lc.finalize
nb=$(applied b0)
echo "lose module.submit" > "$out/proxy.ctl"
round b1
check "B: the import's answer lost: kept in flight ($(ev b1 root-import-failed | jq -c '.[0] | {leafCount, keptInFlight, reason: (.reason | .[0:90])}')), applied on the zone" \
  bash -c "[ \"\$(jq -s 'map(select(.event == \"root-import-failed\" and .keptInFlight == true)) | length' '$out/rounds/b1.jsonl')\" = 1 ] && [ '$(imported b1)' = '[2,3]' ] && [ -s '$out/relayer-state/root-import-inflight.json' ]"
pl=$(plog)
round b2
check "B: next round: booked from the record ($(ev b2 root-imported | jq -c '.[0] | {leafCount, recovered}')), nothing sent again" \
  bash -c "[ \"\$(jq -s 'map(select(.event == \"root-imported\" and .leafCount == 3 and .recovered == true)) | length' '$out/rounds/b2.jsonl')\" = 1 ] && [ $(submits_after $pl) = 0 ] && [ '$(imported b2)' = '[2,3]' ] && [ $(applied b2b) = $((nb + 1)) ] && [ ! -e '$out/relayer-state/root-import-inflight.json' ]"

# ---- C: killed while the answer is held ---------------------------------------------------------------------
dev d3 deposit '{"kind":"private","denomination":0,"account":0}'
dev f3 lc.finalize
nc=$(applied c0)
h=$(hits)
echo "hold module.submit 8" > "$out/proxy.ctl"
start c1
if wait_hit "$h"; then sleep 1; kill -9 "$daemon"; wait "$daemon" 2>/dev/null; daemon=""; ok "C: SIGKILLed with its ImportRoot at the zone and the answer held"; else bad "C: the import never reached the proxy"; fi
sleep 9
pl=$(plog)
start c2
for _ in $(seq 1 60); do [ "$(nev c2 root-imported)" -ge 1 ] && break; sleep 0.5; done
sleep 2; kill -TERM "$daemon"; wait "$daemon" 2>/dev/null; daemon=""
check "C: restarted: booked from its record ($(ev c2 root-imported | jq -c '.[0] | {leafCount, recovered}')), nothing sent again, a clean stop ($(ev c2 stop | jq -c '.[0] | {reason}'))" \
  bash -c "[ \"\$(jq -s 'map(select(.event == \"root-imported\" and .leafCount == 4 and .recovered == true)) | length' '$out/rounds/c2.jsonl')\" = 1 ] && [ $(submits_after $pl) = 0 ] && [ '$(imported c2)' = '[2,3,4]' ] && [ $(applied c2b) = $((nc + 1)) ] && [ $(nev c2 stop) = 1 ]"

# ---- D: killed while the import is trapped (never sent) -------------------------------------------------------
dev d4 deposit '{"kind":"private","denomination":0,"account":0}'
dev f4 lc.finalize
nd=$(applied d0b)
h=$(hits)
echo "trap module.submit" > "$out/proxy.ctl"
start d1
if wait_hit "$h"; then sleep 1; kill -9 "$daemon"; wait "$daemon" 2>/dev/null; daemon=""; ok "D: SIGKILLed with its ImportRoot built and held unsent"; else bad "D: the import never reached the proxy"; fi
sleep 1
check "D: nothing reached the zone ($(imported d1a))" bash -c "[ '$(imported d1b)' = '[2,3,4]' ] && [ $(applied d1c) = $nd ]"
start d2 --import-wait 10   # long enough that its first round still sees the record pending
for _ in $(seq 1 60); do [ "$(nev d2 root-imported)" -ge 1 ] && break; sleep 0.5; done
sleep 2; kill -TERM "$daemon"; wait "$daemon" 2>/dev/null; daemon=""
check "D: restarted: its record waited out ($(nev d2 root-import-pending) pending, $(nev d2 root-import-expired) expired), then n = 5 imported once ($(ev d2 root-imported | jq -c '.[0] | {leafCount, applied, recovered}'))" \
  bash -c "[ $(nev d2 root-import-pending) -ge 1 ] && [ $(nev d2 root-import-expired) = 1 ] && [ \"\$(jq -s 'map(select(.event == \"root-imported\" and .leafCount == 5 and .applied == true)) | length' '$out/rounds/d2.jsonl')\" = 1 ] && [ '$(imported d2)' = '[2,3,4,5]' ] && [ $(applied d2b) = $((nd + 1)) ]"

jq -n --argjson pass $pass --argjson fail $fail --arg imported "$(imported end)" \
  --argjson proxy "$(jq -s 'group_by(.action) | map({action: .[0].action, n: length})' "$out/proxy.jsonl")" \
  '{pass: $pass, fail: $fail, imported: ($imported | fromjson), proxyActions: $proxy}' > "$out/results.json"
printf '\n%s passed, %s failed\n' "$pass" "$fail" | tee -a "$summary"
[ "$fail" = 0 ]
