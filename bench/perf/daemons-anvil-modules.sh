#!/usr/bin/env bash
# RFP-022 P.6, the daemons as Logos modules: bench/perf/daemons-anvil.sh's run with `monitor` and `ferrata-relayer` as
# ferrata_monitor_module and ferrata_relayer_module, started in one headless Logos Core (logosctl daemon: the modules'
# `start`, each with the binary's own arguments), instead of the two binaries. The same anvil (chain id 31337, 12-s
# blocks; nothing is sent to any public network), the same rule (B) FerrataVault v5 fixture and honest node view, the
# same phases (IDLE_MIN minutes idle, an honest candidate the monitor checks and the relayer finalises, IDLE_MIN minutes
# idle). Sampled every 5 s with bench/perf/tree_sample.py: each module's host process (logos_host) with its children,
# and Logos Core's daemon; each module's JSON-RPC to anvil through its own counting proxy (tcp_count.py). Nothing polls
# the modules while they run (a status call at each phase mark only), so Logos Core's own figures are its idle cost.
#
#   bash bench/perf/daemons-anvil-modules.sh OUT_DIR IDLE_MIN LOGOSCTL MONITOR_LGX_DIR RELAYER_LGX_DIR
#
# On the build box (mrun), with the modules' dev packages and logosctl as modules/scripts/check.sh builds them. Needs
# vault/lib (bench/perf/vault-growth.sh installs it). LOGOS_TMPDIR: a short directory for Logos Core's sockets.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
HERE="$REPO/bench/perf"
out="$(realpath -m "${1:?usage: daemons-anvil-modules.sh OUT_DIR IDLE_MIN LOGOSCTL MONITOR_LGX_DIR RELAYER_LGX_DIR}")"
idle_min="${2:?IDLE_MIN}"; LOGOSCTL="$(realpath "${3:?LOGOSCTL}")"; mon_lgx="${4:?MONITOR_LGX_DIR}"; rel_lgx="${5:?RELAYER_LGX_DIR}"
export PATH="$HOME/.foundry/bin:$HOME/.cargo/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1 CARGO_TERM_COLOR=never
LOGOS_TMPDIR=${LOGOS_TMPDIR:-/var/tmp/mls}
SESSION="$LOGOS_TMPDIR/dam"
[ -e "$out" ] && [ -n "$(ls -A "$out")" ] && { echo "daemons-anvil-modules: OUT must be new or empty: $out" >&2; exit 1; }
mkdir -p "$out/run" "$out/rep"
rm -rf "$SESSION"; mkdir -p "$SESSION"
export TMPDIR="$LOGOS_TMPDIR"
say() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$out/log.txt"; }
ctl() { "$LOGOSCTL" --config-dir "$SESSION" "$@"; }
call() { # MODULE METHOD NAME [REQUEST_JSON]: the reply in rep/NAME.json
  local req=()
  if [ -n "${4:-}" ]; then printf '%s' "$4" > "$out/rep/$3.req.json"; req=("@$out/rep/$3.req.json"); fi
  ctl --timeout 600000 call "$1" "$2" "${req[@]}" --json < /dev/null > "$out/rep/$3.json" 2>&1 || true
}
port=8549; rpc="http://127.0.0.1:$port"
anvil --port "$port" --chain-id 31337 --block-time 12 > "$out/run/anvil.log" 2>&1 &
anvil_pid=$!
pids=()
cleanup() { ctl daemon stop > /dev/null 2>&1 || true; for p in "${pids[@]}"; do kill -TERM "$p" 2>/dev/null || true; done; kill "$anvil_pid" 2>/dev/null || true; }
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

# Logos Core with the two modules; the watcher key in a file only this user reads (as monitor-module.sh does).
env HOME="$out/run/home" "$LOGOSCTL" --config-dir "$SESSION" daemon start --detach > "$out/run/core-start.log" 2>&1
ctl install "$mon_lgx"/*.lgx -y >> "$out/run/install.log" 2>&1
ctl install "$rel_lgx"/*.lgx -y >> "$out/run/install.log" 2>&1
ctl module load ferrata_monitor_module >> "$out/run/install.log" 2>&1
ctl module load ferrata_relayer_module >> "$out/run/install.log" 2>&1
ctl module list --json > "$out/rep/modules-loaded.json" 2>&1 || true
( umask 077; mkdir -p "$SESSION/keys"; printf '%s\n' "$WATCHER_KEY" > "$SESSION/keys/watcher.key" )
python3 -I "$HERE/tcp_count.py" --listen 18601 --to "127.0.0.1:$port" --stats "$out/rpc-monitor.json" & pids+=($!)
python3 -I "$HERE/tcp_count.py" --listen 18602 --to "127.0.0.1:$port" --stats "$out/rpc-relayer.json" & pids+=($!)
python3 -I "$HERE/tree_sample.py" --out "$out/samples.csv" --every 5 \
  --find "monitor-module=logos_host --name ferrata_monitor_module " --find "relayer-module=logos_host --name ferrata_relayer_module " \
  --find "core=logosctl --config-dir $SESSION daemon" & pids+=($!)
sleep 1
mkdir -p "$out/run/inbox"
mon_args=(--rpc "http://127.0.0.1:18601" --vault "$vault" --node "file:$view" --state "$out/run/monitor-state.json"
  --key-file "$SESSION/keys/watcher.key" --alert-log "$out/run/alerts.jsonl" --alert-level critical --start-block "$deploy_block"
  --poll 12 --confirmations 0 --b-lag-grace 300 --margin 120 --channel-id "$(label channel)")
rel_args=(--eth-rpc "http://127.0.0.1:18602" --vault "$vault" --unlocked "$relayer" --state "$out/run/relayer-state"
  --from-block "$deploy_block" run --inbox "$out/run/inbox" --poll-secs 12)
mark() { echo -e "$(date +%s)\t$1" >> "$out/phases.tsv"; say "$1"; }
call ferrata_monitor_module start monitor-start "$(jq -n -c '{args: $ARGS.positional}' --args -- "${mon_args[@]}")"
call ferrata_relayer_module start relayer-start "$(jq -n -c '{args: $ARGS.positional}' --args -- "${rel_args[@]}")"
mark "idle-1 start"
sleep $(( idle_min * 60 ))
call ferrata_monitor_module status monitor-status-1; call ferrata_relayer_module status relayer-status-1 '{"since":0}'
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
call ferrata_monitor_module status monitor-status-2; call ferrata_relayer_module status relayer-status-2 '{"since":0}'
mark "idle-2 end"
call ferrata_monitor_module stop monitor-stop
call ferrata_relayer_module stop relayer-stop '{"reason":"SIGTERM","since":0}'
ctl daemon stop > "$out/run/core-stop.log" 2>&1; echo "logosctl daemon stop exit $?" >> "$out/run/core-stop.log"
sleep 6
for p in "${pids[@]}"; do kill -TERM "$p" 2>/dev/null || true; done; sleep 2
cp -L "$SESSION/logs/"*.log "$out/run/" 2>/dev/null || true
{ anvil --version | head -1; echo "nproc --all $(nproc --all)"; grep MemTotal /proc/meminfo; "$LOGOSCTL" --version 2>&1 | head -1
  for f in "$mon_lgx"/*.lgx "$rel_lgx"/*.lgx; do printf '%s  %s\n' "$(sha256sum < "$f" | cut -c1-64)" "$(basename "$f")"; done; } > "$out/machine.txt"
say "records in $out"
