#!/usr/bin/env bash
# RFP-022 P.6: the CPU, memory and bandwidth of the two Ethereum-side daemons, `monitor` (the watchers') and
# `ferrata-relayer`, each its own process under GNU time, sampled every 5 s (bench/perf/proc_sample.py), its JSON-RPC
# traffic through its own counting proxy (bench/perf/tcp_count.py). On the build box (mrun; never on a laptop), on a
# local anvil only (chain id 31337; nothing is sent to any public network), with a rule (B) FerrataVault v5 and an
# accept-all verifier (vault/script/MonitorFixture.s.sol, as monitor/tests/anvil-e2e.sh deploys it), the monitor's
# honest node view of that script. Without a LEZ zone (the relayer's LEZ parts, root imports and the burn watch,
# need one: measured on the zone by bench/perf/gate-sequencer.sh for the updater).
#
#   bash bench/perf/daemons-anvil.sh OUT_DIR [IDLE_MIN]     # OUT_DIR new or empty
#
# Phases: IDLE_MIN minutes idle (both daemons polling: monitor every 12 s, relayer every 12 s), then activity (an
# honest candidate the monitor checks and passes; anvil's time moved past the vault's wait; the relayer finalises
# it), then IDLE_MIN minutes idle again. Needs vault/lib (bench/perf/vault-growth.sh installs it).
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
HERE="$REPO/bench/perf"
out="$(realpath -m "${1:?usage: daemons-anvil.sh OUT_DIR [IDLE_MIN]}")"; idle_min="${2:-10}"
export PATH="$HOME/.foundry/bin:$HOME/.cargo/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1 CARGO_TERM_COLOR=never
T="${PERF_GATE_TMP:-$REPO/../tmp/perf-gate}"
# Never delete what the caller names: OUT must be new or empty (`. 10` would have removed the checkout).
[ -e "$out" ] && [ -n "$(ls -A "$out")" ] && { echo "daemons-anvil: OUT must be new or empty: $out" >&2; exit 1; }
mkdir -p "$out/run"
say() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$out/log.txt"; }
say "build: monitor (release), ferrata-relayer (release)"
( cd "$REPO/monitor" && cargo build --release --locked ) > "$out/run/build-monitor.log" 2>&1 || { tail -20 "$out/run/build-monitor.log"; exit 1; }
( cd "$REPO/cli" && CARGO_TARGET_DIR="$T/target-lez" cargo build --release --locked --bin ferrata-relayer ) > "$out/run/build-relayer.log" 2>&1 \
  || { grep -E -A7 '^error' "$out/run/build-relayer.log" | head -40; exit 1; }
mon="$REPO/monitor/target/release/monitor"; rel="$T/target-lez/release/ferrata-relayer"
port=8549; rpc="http://127.0.0.1:$port"
anvil --port "$port" --chain-id 31337 --block-time 12 > "$out/run/anvil.log" 2>&1 &
anvil_pid=$!
pids=()
cleanup() { for p in "${pids[@]}"; do kill -TERM "$p" 2>/dev/null || true; done; kill "$anvil_pid" 2>/dev/null || true; }
trap cleanup EXIT
for _ in $(seq 1 50); do cast block-number --rpc-url "$rpc" > /dev/null 2>&1 && break; sleep 0.2; done
mapfile -t accts < <(cast rpc --rpc-url "$rpc" eth_accounts | jq -r '.[]')
deployer=${accts[0]}; admin=${accts[1]}; prover=${accts[2]}; relayer=${accts[3]}; fees=${accts[4]}
newkey() { cast wallet new | awk '/Private key/ {print $3}'; }
export WATCHER_KEY FREEZE_KEY
WATCHER_KEY=$(newkey); FREEZE_KEY=$(newkey)
watcher=$(cast wallet address --private-key "$WATCHER_KEY"); freezer=$(cast wallet address --private-key "$FREEZE_KEY")
for a in "$watcher" "$freezer"; do cast rpc --rpc-url "$rpc" anvil_setBalance "$a" 0x56bc75e2d63100000 > /dev/null; done
label() { cast keccak "ferrata/monitor-rehearsal/$1"; }
zero=0x0000000000000000000000000000000000000000000000000000000000000000
genesis=$(label genesis)
( cd "$REPO/vault" && FIXTURE_MODE_A=false FIXTURE_GENESIS=$genesis FIXTURE_ADMIN=$admin FIXTURE_FREEZER=$freezer \
  FIXTURE_WATCHER=$watcher FIXTURE_FEES=$fees forge script script/MonitorFixture.s.sol:MonitorFixture --rpc-url "$rpc" --slow \
  --broadcast --unlocked --sender "$deployer" ) > "$out/run/forge.log" 2>&1 || { tail -30 "$out/run/forge.log"; exit 1; }
vault=$(jq -r '.transactions[] | select(.contractName=="FerrataVault") | .contractAddress' \
  "$REPO/vault/broadcast/MonitorFixture.s.sol/31337/run-latest.json" | tail -1)
vault=$(cast to-check-sum-address "$vault"); deploy_block=$(cast block-number --rpc-url "$rpc")
say "vault $vault at block $deploy_block"
view="$out/run/node-view.json"; blocks='{}'
for i in $(seq 1 20); do blocks=$(jq -c --arg k "$i" --arg h "$(label "block/$i")" '. + {($k): $h}' <<< "$blocks"); done
lez_config=$(cast call --rpc-url "$rpc" "$vault" "lezConfigId()(bytes32)")
att=$(cd "$REPO/monitor" && cargo run --release --locked --quiet --example attestations -- 31337 "$vault" "$lez_config" "$(label channel)" \
  10 "$(label block/10)" 12 "$(label forged/12)")
jq -n --argjson b "$blocks" --argjson k "$(jq -c .keys <<< "$att")" --argjson a "$(jq -c .attestations <<< "$att")" \
  '{node: "lez-honest", finalHeight: 20, finalBlocks: $b, keySets: [{fromBlock: 0, keys: $k, postingTimeframe: 900}],
    attestations: $a}' > "$view"

# The daemons, each behind its own counting proxy to anvil.
python3 -I "$HERE/tcp_count.py" --listen 18601 --to "127.0.0.1:$port" --stats "$out/rpc-monitor.json" & pids+=($!)
python3 -I "$HERE/tcp_count.py" --listen 18602 --to "127.0.0.1:$port" --stats "$out/rpc-relayer.json" & pids+=($!)
sleep 1
/usr/bin/time -v -o "$out/time-monitor.txt" "$mon" run --rpc "http://127.0.0.1:18601" --vault "$vault" --node "file:$view" \
  --state "$out/run/monitor-state.json" --key-env WATCHER_KEY --alert-log "$out/run/alerts.jsonl" --alert-level critical \
  --start-block "$deploy_block" --poll 12 --confirmations 0 --b-lag-grace 300 --margin 120 --channel-id "$(label channel)" \
  > "$out/run/monitor.log" 2>&1 & mon_t=$!; pids+=($mon_t)
mkdir -p "$out/run/inbox"
/usr/bin/time -v -o "$out/time-relayer.txt" "$rel" --eth-rpc "http://127.0.0.1:18602" --vault "$vault" --unlocked "$relayer" \
  --state "$out/run/relayer-state" --from-block "$deploy_block" run --inbox "$out/run/inbox" --poll-secs 12 \
  > "$out/run/relayer.log" 2>&1 & rel_t=$!; pids+=($rel_t)
sleep 3
mon_pid=$(pgrep -P "$mon_t" | head -1); rel_pid=$(pgrep -P "$rel_t" | head -1)
python3 -I "$HERE/proc_sample.py" --pid "monitor=$mon_pid" --pid "relayer=$rel_pid" --out "$out/samples.tsv" --every 5 & pids+=($!)
mark() { echo -e "$(date +%s)\t$1" >> "$out/phases.tsv"; say "$1"; }
mark "idle-1 start"
sleep $(( idle_min * 60 ))
mark "activity start"
cp_sig="(uint32,uint32,bytes32,bytes32,bytes32,uint64,uint64,bool,bytes32,uint64,bytes32,uint32,uint32)"
n1=$(label state/1)
j1="(3,2,$genesis,$n1,$(label burn-root/10),10,0,false,$zero,10,$(label block/10),4,3)"
cast send --unlocked --from "$prover" --rpc-url "$rpc" "$vault" "submitCandidate($cp_sig,bytes)" "$j1" 0x > /dev/null
sleep 60
wait_s=$(( $(cast call --rpc-url "$rpc" "$vault" "delta()(uint256)" | awk '{print $1}') + 600 ))
cast rpc --rpc-url "$rpc" evm_increaseTime "$wait_s" > /dev/null; cast rpc --rpc-url "$rpc" evm_mine > /dev/null
for _ in $(seq 1 30); do [ "$(cast call --rpc-url "$rpc" "$vault" "finalState()(bytes32)")" = "$n1" ] && break; sleep 5; done
mark "activity end (final state $(cast call --rpc-url "$rpc" "$vault" "finalState()(bytes32)" | cut -c1-18)…; wanted ${n1:0:18}…)"
sleep $(( idle_min * 60 ))
mark "idle-2 end"
kill -TERM "$mon_pid" "$rel_pid" 2>/dev/null || true
wait "$mon_t" "$rel_t" 2>/dev/null || true
for p in "${pids[@]}"; do kill -TERM "$p" 2>/dev/null || true; done; sleep 2
{ anvil --version | head -1; echo "nproc --all $(nproc --all)"; grep MemTotal /proc/meminfo; sha256sum "$mon" "$rel"; } > "$out/machine.txt"
tail -n 3 "$out/run/relayer.log" "$out/run/monitor.log" | cut -c1-300
say "records in $out"
