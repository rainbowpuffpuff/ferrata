#!/usr/bin/env bash
# RFP-022 R.06 on Logos Core's real send stack: ferrata-relayer as ferrata_relayer_module in one headless Logos Core
# with Logos's own tx_sender_module, signer_manager_module, keystore_module, fee_module and eth_rpc_module (built from
# their sources by modules/scripts/send-stack.sh), on a local anvil (chain id 31337, nothing public) reached only
# through e2e/netfault/holdproxy.py, killed and cut off where a send can be lost or made twice:
#
#   K0  no fault: a rule (B) checkpoint job, submitCandidate through tx_sender_module
#   K1  the broadcast trapped before the node (eth_sendRawTransaction held open, never forwarded), then every module
#       host of the session SIGKILLed (tx_sender_module loses the request it held; its history on disk stays), the
#       modules loaded again, the relayer's next round: the checkpoint's finalizeAttested
#   K2  the broadcast forwarded and its answer held 25 s, the relayer module's host alone SIGKILLed while it waits
#       (tx_sender_module keeps the request), loaded again: a holder's release, then the claimOwed that follows it
#   K3  anvil not mining: the transaction broadcast and pending, the whole Logos Core (daemon and module hosts)
#       SIGKILLed, a block mined, Logos Core started again: a second checkpoint
#   K4  the endpoint down for a whole round (every call 503 or cut, the relayer's reads and Logos Core's), then up:
#       the second checkpoint's finalizeAttested
#   K5  a third checkpoint whose broadcast is answered 503 at once and reaches the node 8 s later (the endpoint failed
#       in the client's eyes, not in the node's): the relayer must not send it twice
#   M1  ferrata_monitor_module (holding the watcher key too, its fallback) against a wrong candidate: its cancelCandidate's
#       broadcast trapped before the node, every module host SIGKILLed, the modules loaded again, the monitor started
#       again: the candidate cancelled once (a second cancel, if the module sent one, reverts: it changes nothing)
#
# After a restart nothing stays unlocked (Logos's signer manager): the person at the Signer app unlocks each locked
# wallet again, played here by Logos's headless approver evm_signer_cli (`wallets`, `unlock_wallet` with the run's
# password, from a 0600 file), then each daemon module's `custody` unlocks its account for tx_sender_module again.
# After each, from the chain: every transaction of the relayer's account (nonce, call, status), each effect counted
# (one each), and at the end the account's nonces 0..n-1 without a gap, every receipt status 1, no in-flight record
# left and every job in done/. checks.tsv (PASS/FAIL), calls.jsonl (every Logos Core call), run/ (the relayer's lines
# per step, the proxy's log, the chain's transactions, tx_sender_module's history of the account).
#
#   STACK=DIR FERRATA_LGX=DIR modules/scripts/send-stack-kill.sh OUT
#     STACK: send-stack.sh build's OUT (lgx-<name>/ for the five stack modules, logos/bin/logosctl);
#     FERRATA_LGX: check.sh's OUT (lgx-ferrata_relayer_module/, lgx-ferrata_monitor_module/)
# Box only (Nix-built modules, Foundry 1.3.6, a 1 GiB stack). Keys are fresh anvil-only keys, kept in OUT/run/keys.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
OUT=$(mkdir -p "${1:?OUT}" && cd "$1" && pwd)
: "${STACK:?STACK: send-stack.sh build OUT (lgx-*/, logos/)}"
: "${FERRATA_LGX:?FERRATA_LGX: check.sh OUT (lgx-ferrata_relayer_module/, lgx-ferrata_monitor_module/)}"
LOGOSCTL=${LOGOSCTL:-$STACK/logos/bin/logosctl}
export PATH="$HOME/.foundry/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
if [ "$(ulimit -s)" = unlimited ]; then ulimit -s 1048576; fi
LT="${LOGOS_TMPDIR:-/var/tmp/ssk}" S="$OUT/session" R="$OUT/run"
mkdir -p "$LT"; rm -rf "${S:?}" "${R:?}" "${OUT:?}/home"; mkdir -p "$OUT/home" "$R/keys"; chmod 700 "$R/keys"
: > "$OUT/calls.jsonl"; : > "$OUT/checks.tsv"
pass=0; fail=0
check() { # NAME EXPECTED ACTUAL
  if [ "$2" = "$3" ]; then pass=$((pass + 1)); echo "PASS $1"; else fail=$((fail + 1)); echo "FAIL $1: expected [$2], got [$3]"; fi
  printf '%s\t%s\t%s\t%s\n' "$([ "$2" = "$3" ] && echo PASS || echo FAIL)" "$1" "$2" "$3" >> "$OUT/checks.tsv"
}
note() { echo "== $*"; printf 'NOTE\t%s\n' "$*" >> "$OUT/checks.tsv"; }
ctl() { HOME="$OUT/home" TMPDIR="$LT" "$LOGOSCTL" --config-dir "$S" "$@"; }
# c MODULE METHOD ARGS...: one call through Logos Core, recorded in calls.jsonl; LAST holds its result.
c() {
  local m="$1" f="$2" out rc; shift 2
  out="$(ctl --timeout 900000 call "$m" "$f" "$@" --json < /dev/null 2>&1)"; rc=$?
  jq -c -n --args '$ARGS.positional' -- "$@" > "$OUT/.args" 2>/dev/null || echo '[]' > "$OUT/.args"
  jq -c -n --arg m "$m" --arg f "$f" --slurpfile a "$OUT/.args" --arg o "$out" --argjson rc "$rc" \
    '{module: $m, method: $f, args: $a[0], exit: $rc,
      reply: ($o | fromjson? // $o | if type == "object" and (.result | type) == "string" then .result |= (fromjson? // .) else . end
              | walk(if type == "object" then del(.password, .passwords, .secret) else . end))}' \
    >> "$OUT/calls.jsonl"
  LAST=$(jq -c 'if (.result | type) == "string" then (.result | fromjson? // .result) else (.result // .) end' <<< "$out" 2>/dev/null || echo "$out")
  echo "$m.$f -> exit $rc: ${LAST:0:300}"
}

# ---- anvil behind the fault proxy ------------------------------------------------------------------------------
anvil --port 18545 --chain-id 31337 > "$R/anvil.log" 2>&1 & anvil_pid=$!
up=http://127.0.0.1:18545
for _ in $(seq 60); do cast chain-id --rpc-url "$up" > /dev/null 2>&1 && break; sleep 0.5; done
CTL="$R/proxy.ctl"; echo pass > "$CTL"
python3 -I "$repo/e2e/netfault/holdproxy.py" --upstream "$up" --port-file "$R/proxy.port" --ctl "$CTL" --log "$R/proxy.jsonl" \
  > "$R/proxy.out" 2>&1 & proxy_pid=$!
for _ in $(seq 50); do [ -s "$R/proxy.port" ] && break; sleep 0.1; done
rpc="http://127.0.0.1:$(cat "$R/proxy.port")"
cleanup() { kill_core; kill "$proxy_pid" "$anvil_pid" 2>/dev/null; }
trap cleanup EXIT

# ---- the devnet: the vault (rule (B), the fixture's verifier accepts the empty seal), ETH registered, a deposit -
mapfile -t accts < <(cast rpc --rpc-url "$up" eth_accounts | jq -r '.[]')
deployer=${accts[0]} admin=${accts[1]} prover=${accts[2]} anyone=${accts[3]} fees=${accts[4]} recipient=${accts[5]} freezer=${accts[6]}
RELAYER_KEY=$(cast wallet new | awk '/Private key/ {print $3}'); relayer=$(cast wallet address --private-key "$RELAYER_KEY")
WATCHER_KEY=$(cast wallet new | awk '/Private key/ {print $3}'); watcher=$(cast wallet address --private-key "$WATCHER_KEY")
pw="$R/keys/password"
( umask 077; echo "$RELAYER_KEY" > "$R/keys/relayer.key"; echo "$WATCHER_KEY" > "$R/keys/watcher.key"; echo "ferrata devnet only" > "$pw" )
unset RELAYER_KEY WATCHER_KEY
for a in "$relayer" "$watcher"; do cast rpc --rpc-url "$up" anvil_setBalance "$a" 0x56bc75e2d63100000 > /dev/null; done # 100 ETH, local
label() { cast keccak "ferrata/send-stack-kill/$1"; }
zero=0x0000000000000000000000000000000000000000000000000000000000000000 eth=0x0000000000000000000000000000000000000000
genesis=$(label genesis)
( cd "$repo/vault" && { [ -d lib/risc0-ethereum ] || forge install --no-git foundry-rs/forge-std@v1.10.0 \
    OpenZeppelin/openzeppelin-contracts@v5.4.0 risc0/risc0-ethereum@v3.1.0; } && forge build --quiet \
  && FIXTURE_MODE_A=false FIXTURE_GENESIS=$genesis FIXTURE_ADMIN=$admin FIXTURE_FREEZER=$freezer FIXTURE_WATCHER=$watcher \
    FIXTURE_FEES=$fees forge script script/MonitorFixture.s.sol:MonitorFixture --rpc-url "$up" --slow --broadcast --unlocked \
    --sender "$deployer" ) > "$R/forge.log" 2>&1 || { tail -n 30 "$R/forge.log"; exit 1; }
vault=$(cast to-check-sum-address "$(jq -r '.transactions[] | select(.contractName=="FerrataVault") | .contractAddress' \
  "$repo/vault/broadcast/MonitorFixture.s.sol/31337/run-latest.json" | tail -1)")
deploy_block=$(cast block-number --rpc-url "$up"); cid=$(cast call --rpc-url "$up" "$vault" "lezConfigId()(bytes32)")
max=115792089237316195423570985008687907853269984665640564039457584007913129639935
denom=100000000000000000 relay_fee=1000000000000000 cp_fee=100000000000000
cast send --unlocked --from "$admin" --rpc-url "$up" "$vault" \
  "registerToken(address,(uint256[],uint256[],uint256,uint256,(uint256,uint256),(uint256,uint256),uint256,bytes32,uint8,uint256[]))" \
  "$eth" "([$denom],[$relay_fee],1000000000,1,($max,0),($max,0),0,$(label lez-eth),18,[$cp_fee])" > "$R/register.log" 2>&1
check devnet-register-eth 1 "$(grep -c '^status *1' "$R/register.log")"
cast send --unlocked --from "$anyone" --rpc-url "$up" --value $((denom + relay_fee)) "$vault" "depositETH(uint8,bytes32,uint64)" \
  0 "$(label commitment/0)" 0 > "$R/deposit.log" 2>&1
check devnet-deposit 1 "$(grep -c '^status *1' "$R/deposit.log")"
jq -n --arg rpc "$rpc" --arg vault "$vault" --arg cid "$cid" --argjson deployBlock "$deploy_block" --arg relayer "$relayer" \
  --arg watcher "$watcher" --arg recipient "$recipient" --arg anvil "$(anvil --version | head -n 1)" \
  '{chainId: 31337, rpcThroughProxy: $rpc, vault: $vault, lezConfigId: $cid, deployBlock: $deployBlock, relayer: $relayer,
    watcher: $watcher, recipient: $recipient, anvil: $anvil}' > "$R/devnet.json"

# ---- Logos Core ------------------------------------------------------------------------------------------------
MODS_ALL="eth_rpc_module fee_module keystore_module signer_manager_module tx_sender_module ferrata_relayer_module ferrata_monitor_module"
# The processes of this session: the daemon (--config-dir S) and every module host (logos_host ... S/...).
session_pids() {
  local p a
  for p in $(pgrep -f -- "$S" 2>/dev/null); do
    [ "$p" = "$$" ] && continue
    a=$(tr '\0' ' ' < "/proc/$p/cmdline" 2>/dev/null) || continue
    case "$a" in *logos_host*"--name "*|*"--config-dir $S"*) echo "$p" ;; esac
  done
}
host_pids() { # MODULE...: the hosts of these modules in this session
  local p a m
  for p in $(pgrep -f logos_host 2>/dev/null); do
    a=$(tr '\0' ' ' < "/proc/$p/cmdline" 2>/dev/null) || continue
    case "$a" in *"$S/"*) ;; *) continue ;; esac
    for m in "$@"; do case "$a" in *"--name $m "*) echo "$p" ;; esac; done
  done
}
# kill_pids PID...: SIGKILL, each process's command line first written to run/kills.log (what was killed, by step).
kill_pids() {
  local p
  for p in "$@"; do printf '%s\t%s\t%s\n' "${STEP:-?}" "$p" "$(tr '\0' ' ' < "/proc/$p/cmdline" 2>/dev/null | cut -c1-300)" >> "$R/kills.log"; done
  for p in "$@"; do kill -9 "$p" 2>/dev/null; done
  for p in "$@"; do for _ in $(seq 50); do kill -0 "$p" 2>/dev/null || break; sleep 0.1; done; done
}
kill_core() { kill_pids $(session_pids); }
# What was killed in STEP: "daemon N hosts M (names)" (N: the processes that are not module hosts: the daemon).
killed() {
  awk -F'\t' -v s="$1" '$1 == s {
      if ($3 ~ /logos_host/) { n++; if (match($3, /--name [a-z_]+/)) names = names substr($3, RSTART + 7, RLENGTH - 7) " " }
      else d++ }
    END { printf "daemon %d hosts %d (%s)", d, n, names }' "$R/kills.log" 2>/dev/null
}
core_up() { # [first | first-again]: install the packages too; first-again (and no argument): the approver unlocks first
  if [ -z "$(session_pids | head -n 1)" ]; then
    ctl daemon start --detach >> "$R/daemon.txt" 2>&1 || { ctl daemon stop >> "$R/daemon.txt" 2>&1; ctl daemon start --detach >> "$R/daemon.txt" 2>&1; }
  fi
  local d m
  if [ "${1:-}" = first ] || [ "${1:-}" = first-again ]; then   # first-again: after the whole Logos Core was killed
    for m in eth_rpc_module fee_module keystore_module signer_manager_module tx_sender_module evm_signer_cli; do ctl install "$STACK/lgx-$m"/*.lgx -y 2>&1 | tail -n 1; done
    for m in ferrata_relayer_module ferrata_monitor_module; do ctl install "$FERRATA_LGX/lgx-$m"/*.lgx -y 2>&1 | tail -n 1; done
  fi
  for m in tx_sender_module evm_signer_cli ferrata_relayer_module ferrata_monitor_module; do ctl module load "$m" 2>&1 | tail -n 1; done
  ctl module list --json > "$R/modules-$(date +%s%N).json" 2>&1
  c eth_rpc_module set_chain_config 31337 "{\"endpoint\":\"$rpc\",\"name\":\"anvil-behind-proxy\",\"testnet\":true}"
  c keystore_module configure '{"approvers":["evm_signer_ui","evm_signer_cli"],"custodians":["evm_keystore_ui","ferrata_relayer_module","ferrata_monitor_module"],"managers":["signer_manager_module"]}'
  c signer_manager_module configure '{"approvers":["evm_signer_ui","evm_signer_cli"],"custodians":["evm_keystore_ui","ferrata_relayer_module","ferrata_monitor_module"],"signers":["keystore_module"]}'
  [ "${1:-}" = first ] || unlock_wallets
  c ferrata_relayer_module custody "{\"key_file\":\"$R/keys/relayer.key\",\"password_file\":\"$pw\",\"ttl_ms\":7200000}"
  CUSTODY=$(jq -r '.verdict // "?"' <<< "$LAST" 2>/dev/null)
  c ferrata_monitor_module custody "{\"key_file\":\"$R/keys/watcher.key\",\"password_file\":\"$pw\",\"ttl_ms\":7200000}"
  CUSTODY_M=$(jq -r '.verdict // "?"' <<< "$LAST" 2>/dev/null)
}
# The person at the Signer app after a restart (evm_signer_cli): every locked wallet unlocked by its first way in, each
# password factor given the run's password (a 0600 file per wallet in run/unlock/, never in calls.jsonl); a step that
# asks for a password again is answered once. UNLOCKED: how many wallets were locked, and how many now are not.
unlock_wallets() {
  local f r h n0
  c evm_signer_cli wallets > /dev/null
  printf '%s\n' "$LAST" > "$R/wallets-${STEP:-start}.json"
  n0=$(jq '[.wallets[]? | select(.locked == true)] | length' <<< "$LAST" 2>/dev/null || echo "?")
  mkdir -p "$R/unlock"; chmod 700 "$R/unlock"
  python3 -I - "$R/wallets-${STEP:-start}.json" "$pw" "$R/unlock" <<'PY' > "$R/unlock/files.txt"
import json, os, sys
# argv[2] is the password FILE's path (0600), never the password itself.
w = json.load(open(sys.argv[1])); pw = open(sys.argv[2]).read().rstrip("\n"); out = sys.argv[3]
for x in w.get("wallets", []):
    if not x.get("locked") or not x.get("ways"):
        continue
    way = x["ways"][0]
    req = {"seed": x["seed"], "way": way["id"],
           "passwords": {f["id"]: pw for f in way.get("factors", []) if f.get("kind") == "password" and f.get("id")}}
    if any(f.get("kind") == "device" for f in way.get("factors", [])):
        req["secret"] = pw
    p = os.path.join(out, x["seed"] + ".json")
    fd = os.open(p, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w") as fh:
        json.dump(req, fh)
    print(p)
PY
  while read -r f; do
    [ -n "$f" ] || continue
    c evm_signer_cli unlock_wallet "@$f" > /dev/null
    if [ "$(jq -r '.state // empty' <<< "$LAST" 2>/dev/null)" = needs_password ]; then
      h=$(jq -r '.handle' <<< "$LAST")
      jq -c --arg h "$h" '{handle: $h, passwords: .passwords}' "$f" > "$f.again"; chmod 600 "$f.again"
      c evm_signer_cli unlock_wallet "@$f.again" > /dev/null
    fi
  done < "$R/unlock/files.txt"
  c evm_signer_cli wallets > /dev/null
  UNLOCKED="$n0 $(jq '[.wallets[]? | select(.locked == true)] | length' <<< "$LAST" 2>/dev/null || echo "?")"
}
# Modules a kill left dead, loaded again (the daemon still runs).
reload_hosts() {
  local m
  for m in "$@"; do
    for _ in $(seq 50); do [ -z "$(host_pids "$m")" ] && break; sleep 0.1; done
    ctl module reload "$m" > /dev/null 2>&1 || ctl module load "$m" > /dev/null 2>&1 || true
  done
  core_up
}

# ---- the relayer -----------------------------------------------------------------------------------------------
STATE="$R/relayer-state" INBOX="$R/inbox"; mkdir -p "$STATE" "$INBOX"
RARGS=(--eth-rpc "$rpc" --vault "$vault" --chain-id 31337 --from-block "$deploy_block" --send-via logos --from "$relayer"
       --state "$STATE" run --inbox "$INBOX" --rounds 1 --poll-secs 1)
start_round() { # STEP: the relayer module's daemon, one round (returns at once)
  c ferrata_relayer_module start "$(jq -c -n --args '{args: $ARGS.positional}' -- "${RARGS[@]}")"
  printf '%s\n' "$LAST" > "$R/$1-start.json"
}
wait_round() { # STEP: until the daemon no longer runs; its lines in STEP-lines.json
  local st
  for _ in $(seq 1200); do
    st=$(ctl --timeout 60000 call ferrata_relayer_module status '{"since":0}' --json < /dev/null 2>/dev/null |
      jq -c 'if (.result | type) == "string" then (.result | fromjson? // .result) else .result end' 2>/dev/null)
    [ "$(jq -r '.running' <<< "$st" 2>/dev/null)" = true ] || break
    sleep 0.5
  done
  printf '%s\n' "$st" > "$R/$1-lines.json"
  # A round still running when the wait ends, or a status never read, is a failure, never an end.
  [ "$(jq -r '.running' <<< "$st" 2>/dev/null)" = false ] || check "$1-round-ended" false "$(jq -r '.running // "unread"' <<< "$st" 2>/dev/null || echo unread)"
}
round() { start_round "$1"; wait_round "$1"; }
events() { jq -r '[.. | objects | .event? // empty] | join(",")' "$R/$1-lines.json" 2>/dev/null; }
# The fixture's checkpoint journal (relayer.rs `checkpoint_journal`), as send-stack.sh writes it.
cp_journal() { # PARENT NEWSTATE TIPID TIPHASH BURNROOT BURNCOUNT
  cast abi-encode "f(bytes32,uint256,address,bytes32,uint256,uint256,bytes32,bytes32,bytes32,uint256,uint256,uint256,bytes32,uint256,bytes32,uint256,uint256)" \
    0xbdb3b52df781536a6bcce7088d156b3951a00a45ec8bcf2805ba8406fbeb5ec7 31337 "$vault" "$cid" 3 2 "$1" "$2" "$5" "$6" 0 0 "$zero" "$3" "$4" 4 3
}
warp() { cast rpc --rpc-url "$up" evm_setNextBlockTimestamp $(( $(cast block latest --field timestamp --rpc-url "$up") + $1 )) > /dev/null; cast rpc --rpc-url "$up" evm_mine > /dev/null; }
wait_hit() { # SECS: until the proxy's one-shot fault fired
  for _ in $(seq $(( $1 * 10 ))); do [ -s "$CTL.hit" ] && return 0; sleep 0.1; done; return 1
}
# Every transaction of the relayer's account since the deployment: nonce, the vault method, status.
art="$repo/vault/out/FerrataVault.sol/FerrataVault.json"
jq '.methodIdentifiers | to_entries | map({key: ("0x" + .value), value: (.key | sub("\\(.*"; ""))}) | from_entries' "$art" > "$R/selectors.json"
# txs_of ADDR FILE: every mined transaction of ADDR since the deployment, one JSON line each.
txs_of() {
  local who=$1 out=$2 b h n from nonce input st; n=$(cast block-number --rpc-url "$up"); : > "$out"
  for b in $(seq $((deploy_block + 1)) "$n"); do
    for h in $(cast block "$b" --json --rpc-url "$up" | jq -r '.transactions[]'); do
      from=$(cast tx "$h" from --rpc-url "$up")
      [ "${from,,}" = "${who,,}" ] || continue
      nonce=$(cast tx "$h" nonce --rpc-url "$up"); input=$(cast tx "$h" input --rpc-url "$up")
      st=$(cast receipt "$h" status --rpc-url "$up" 2>/dev/null | awk '{print $1}')
      jq -c -n --slurpfile s "$R/selectors.json" --arg h "$h" --argjson b "$b" --argjson n "$nonce" --arg sel "${input:0:10}" \
        --arg st "$st" '{nonce: $n, hash: $h, block: $b, call: ($s[0][$sel] // $sel), status: ($st | tonumber? // $st)}' \
        >> "$out"
    done
  done
}
relayer_txs() { txs_of "$relayer" "$R/relayer-txs.jsonl"; }
count() { relayer_txs; jq -s --arg c "$1" '[.[] | select(.call == $c)] | length' "$R/relayer-txs.jsonl"; }
all_txs() { relayer_txs; jq -s 'length' "$R/relayer-txs.jsonl"; }
inflight() { [ -f "$STATE/inflight.json" ] && jq -c '{key, nonce, hash, request}' "$STATE/inflight.json" || echo none; }

core_up first > "$R/core-up-0.log" 2>&1
check logos-core-custody ok "$CUSTODY"
s1=$(label state/1) s2=$(label state/2)

# K0: a checkpoint, no fault.
jq -c -n --arg j "$(cp_journal "$genesis" "$s1" 10 "$(label block/10)" "$(label burn-root/1)" 1)" '{kind: "checkpoint", journal: $j, seal: "0x"}' > "$INBOX/k0-checkpoint.json"
round K0 > "$R/K0.log" 2>&1
check K0-submitCandidate-once 1 "$(count submitCandidate)"
check K0-via-tx_sender_module true "$(jq -r '[.. | objects | select(.via? != null) | .via.path?] | any(. == "logos")' "$R/K0-lines.json" 2>/dev/null)"

# K1: finalizeAttested, its broadcast trapped before the node, every module host SIGKILLed.
warp 3601
rm -f "$CTL.hit"; echo "trap eth_sendRawTransaction" > "$CTL"
start_round K1a > "$R/K1.log" 2>&1
hit=$(wait_hit 120 && echo yes || echo no); check K1-fault-fired yes "$hit"
if [ "$hit" = yes ]; then
  sleep 1; f1=$(inflight)
  STEP=K1 kill_pids $(host_pids $MODS_ALL)
  note "K1 killed: $(killed K1)"
  check K1-every-module-host-killed "daemon 0 hosts 7" "$(killed K1 | cut -d' ' -f1-4)"
  note "K1: the broadcast trapped at the proxy; in flight: $f1; every module host SIGKILLed"
  check K1-nothing-left-before-restart 0 "$(count finalizeAttested)"
  echo pass > "$CTL"
  STEP=K1 reload_hosts $MODS_ALL >> "$R/K1.log" 2>&1
  note "K1: after the restart the approver found locked wallets / still locked after its unlock: $UNLOCKED"
  check K1-custody-after-restart ok "$CUSTODY"
  round K1b >> "$R/K1.log" 2>&1
  note "K1: after the restart: $(events K1b)"
else
  note "K1: the trap never fired ($(cat "$CTL"))"; echo pass > "$CTL"; wait_round K1a >> "$R/K1.log" 2>&1
fi
check K1-finalizeAttested-once 1 "$(count finalizeAttested)"
check K1-no-inflight-left none "$(inflight)"

# K2: a release, its broadcast forwarded and the answer held, the relayer module's host alone SIGKILLed.
rj=$(cast abi-encode "f(bytes32,uint256,address,bytes32,bytes32,bytes32,address,uint256,address,address,uint256)" \
  0x6e04f4b1fa66535e182a1ef2e4f5bfdfe44f7257f1663ef81ffa4f38d7696e51 31337 "$vault" "$cid" "$(label burn-root/1)" \
  "$(label nullifier/1)" "$eth" "$denom" "$recipient" "$relayer" "$relay_fee")
jq -c -n --arg j "$rj" '{kind: "release", journal: $j, seal: "0x"}' > "$INBOX/k2-release.json"
bal0=$(cast balance --rpc-url "$up" "$recipient")
rm -f "$CTL.hit"; echo "hold eth_sendRawTransaction 25" > "$CTL"
start_round K2a > "$R/K2.log" 2>&1
hit=$(wait_hit 120 && echo yes || echo no); check K2-fault-fired yes "$hit"
if [ "$hit" = yes ]; then
  sleep 1; f2=$(inflight)
  STEP=K2 kill_pids $(host_pids ferrata_relayer_module)
  note "K2 killed: $(killed K2)"
  check K2-the-relayer-host-alone-killed "daemon 0 hosts 1 (ferrata_relayer_module" "$(killed K2 | cut -d' ' -f1-5)"
  note "K2: the release's broadcast reached the node, its answer held; in flight: $f2; the relayer module's host SIGKILLed"
  check K2-mined-while-held 1 "$(count release)"
  STEP=K2 reload_hosts ferrata_relayer_module >> "$R/K2.log" 2>&1
  check K2-custody-after-restart ok "$CUSTODY"
  round K2b >> "$R/K2.log" 2>&1
  note "K2: after the restart: $(events K2b)"
else
  note "K2: the hold never fired ($(cat "$CTL"))"; echo pass > "$CTL"; wait_round K2a >> "$R/K2.log" 2>&1
fi
check K2-release-once 1 "$(count release)"
check K2-recipient-paid-once "$((denom - relay_fee - cp_fee))" "$(( $(cast balance --rpc-url "$up" "$recipient") - bal0 ))"   # the denomination less the relay and checkpoint fees, once
check K2-nullifier-used true "$(cast call --rpc-url "$up" "$vault" 'nullifierUsed(bytes32)(bool)' "$(label nullifier/1)")"
[ "$(count claimOwed)" = 1 ] || round K2c >> "$R/K2.log" 2>&1   # the fee withdrawal, if the restarted round ended first
check K2-claimOwed-once 1 "$(count claimOwed)"
check K2-no-inflight-left none "$(inflight)"

# K3: a second checkpoint, broadcast and pending (anvil not mining), the whole Logos Core SIGKILLed.
jq -c -n --arg j "$(cp_journal "$s1" "$s2" 20 "$(label block/20)" "$(label burn-root/2)" 2)" '{kind: "checkpoint", journal: $j, seal: "0x"}' > "$INBOX/k3-checkpoint.json"
cast rpc --rpc-url "$up" evm_setAutomine false > /dev/null
start_round K3a > "$R/K3.log" 2>&1
for _ in $(seq 600); do
  [ "$(jq -r '.hash // empty' "$STATE/inflight.json" 2>/dev/null)" != "" ] && break; sleep 0.1
done
sleep 1; f3=$(inflight); pend=$(cast rpc --rpc-url "$up" txpool_status | jq -r '.pending')
STEP=K3 kill_core
note "K3 killed: $(killed K3)"
check K3-killed-with-its-transaction-broadcast-and-pending "yes 0x1" "$( [ -n "$(jq -r '.hash // empty' <<< "$f3" 2>/dev/null)" ] && echo yes || echo no) $pend"
check K3-daemon-and-every-host-killed "daemon 1" "$(killed K3 | cut -d' ' -f1-2)"
check K3-nothing-of-the-session-left "" "$(session_pids | paste -sd, -)"
note "K3: broadcast and pending (txpool $pend); in flight: $f3; Logos Core SIGKILLed (daemon and hosts)"
check K3-pending-not-mined 1 "$(count submitCandidate)"
cast rpc --rpc-url "$up" evm_mine > /dev/null; cast rpc --rpc-url "$up" evm_setAutomine true > /dev/null
STEP=K3 core_up first-again >> "$R/K3.log" 2>&1
note "K3: after the restart the approver found locked wallets / still locked after its unlock: $UNLOCKED"
check K3-custody-after-restart ok "$CUSTODY"
round K3b >> "$R/K3.log" 2>&1
note "K3: after the restart: $(events K3b)"
check K3-submitCandidate-once 2 "$(count submitCandidate)"
check K3-no-inflight-left none "$(inflight)"

# K4: the endpoint down for a whole round, then up: the second checkpoint's finalizeAttested.
warp 3601
echo outage > "$CTL"
outages() { local n; n=$(grep -c '"outage"' "$R/proxy.jsonl" 2>/dev/null); echo "${n:-0}"; }   # grep -c prints 0 itself
o0=$(outages)
round K4a > "$R/K4.log" 2>&1
check K4-the-outage-met-the-round yes "$( [ "$(outages)" -gt "$o0" ] && echo yes || echo no)"
check K4-nothing-sent-during-outage 1 "$(count finalizeAttested)"
note "K4: during the outage: $(jq -c '[.. | objects | select(.error? != null) | .error] | .[0:2]' "$R/K4a-lines.json" 2>/dev/null)"
echo pass > "$CTL"
round K4b >> "$R/K4.log" 2>&1
check K4-finalizeAttested-once 2 "$(count finalizeAttested)"

# K5: a third checkpoint, its broadcast answered 503 and forwarded to the node 8 s later.
s4=$(label state/4)
jq -c -n --arg j "$(cp_journal "$s2" "$s4" 40 "$(label block/40)" "$(label burn-root/4)" 4)" '{kind: "checkpoint", journal: $j, seal: "0x"}' > "$INBOX/k5-checkpoint.json"
rm -f "$CTL.hit"; echo "late eth_sendRawTransaction 8" > "$CTL"
round K5a > "$R/K5.log" 2>&1
check K5-fault-fired yes "$(wait_hit 30 && echo yes || echo no)"
for _ in $(seq 300); do grep -q 'late forward answered' "$R/proxy.jsonl" 2>/dev/null && break; sleep 0.1; done
late_n=$(jq -s '[.[] | select(.action == "late forward answered" and .status == 200 and ((.answer // "") | test("\"error\"") | not))] | length' "$R/proxy.jsonl" 2>/dev/null); late_n=${late_n:-0}   # forwarded and accepted by the node
check K5-delivered-late-to-the-node yes "$( [ "$late_n" -ge 1 ] && echo yes || echo no)"
note "K5: the broadcast answered 503 and forwarded late: $late_n late forward(s); the first round: $(events K5a); in flight: $(inflight)"
echo pass > "$CTL"
round K5b >> "$R/K5.log" 2>&1
note "K5: the next round: $(events K5b)"
[ "$(count submitCandidate)" = 3 ] && [ "$(inflight)" = none ] || { round K5c >> "$R/K5.log" 2>&1; note "K5: one more round: $(events K5c)"; }
check K5-submitCandidate-once 3 "$(count submitCandidate)"
check K5-no-inflight-left none "$(inflight)"

# M1: the monitor against a wrong candidate, its cancel's broadcast trapped, every module host SIGKILLed.
check monitor-custody ok "$CUSTODY_M"
s3=$(label state/3)
view="$R/node-view.json" blocks='{}'
for i in $(seq 1 40); do blocks=$(jq -c --arg k "$i" --arg h "$(label "block/$i")" '. + {($k): $h}' <<< "$blocks"); done
jq -n --argjson b "$blocks" --arg k1 "$(label key/1)" --arg k2 "$(label key/2)" --arg k3 "$(label key/3)" --arg k4 "$(label key/4)" \
  '{node: "lez-honest", finalHeight: 40, finalBlocks: $b, keySets: [{fromBlock: 0, keys: [$k1, $k2, $k3, $k4], postingTimeframe: 900}]}' > "$view"
MARGS=(--rpc "$rpc" --vault "$vault" --node "file:$view" --start-block "$deploy_block" --poll 1 --confirmations 0 --margin 120
       --alert-level critical --max-polls 2 --state "$R/monitor-state.json" --send-via logos --from "$watcher" --key-file "$R/keys/watcher.key")
mon_start() { # STEP
  c ferrata_monitor_module start "$(jq -c -n --args '{args: $ARGS.positional}' -- "${MARGS[@]}" --alert-log "$R/$1-alerts.jsonl")"
  printf '%s\n' "$LAST" > "$R/$1-start.json"
}
mon_wait() { # STEP
  local st
  for _ in $(seq 1200); do
    st=$(ctl --timeout 60000 call ferrata_monitor_module status --json < /dev/null 2>/dev/null |
      jq -c 'if (.result | type) == "string" then (.result | fromjson? // .result) else .result end' 2>/dev/null)
    [ "$(jq -r '.running' <<< "$st" 2>/dev/null)" = true ] || break
    sleep 0.5
  done
  printf '%s\n' "$st" > "$R/$1-status.json"
  [ "$(jq -r '.running' <<< "$st" 2>/dev/null)" = false ] || check "$1-monitor-ended" false "$(jq -r '.running // "unread"' <<< "$st" 2>/dev/null || echo unread)"
}
cancels() { txs_of "$watcher" "$R/watcher-txs.jsonl"; jq -s --argjson st "$1" '[.[] | select(.call == "cancelCandidate" and .status == $st)] | length' "$R/watcher-txs.jsonl"; }
cast send --unlocked --from "$prover" --rpc-url "$up" "$vault" \
  "submitCandidate((uint32,uint32,bytes32,bytes32,bytes32,uint64,uint64,bool,bytes32,uint64,bytes32,uint32,uint32),bytes)" \
  "(3,2,$s2,$s3,$(label burn-root/3),3,0,false,$zero,30,$(label forged/30),4,3)" 0x > "$R/M1-submit.log" 2>&1
check M1-wrong-candidate-submitted 1 "$(grep -c '^status *1' "$R/M1-submit.log")"
rm -f "$CTL.hit"; echo "trap eth_sendRawTransaction" > "$CTL"
mon_start M1a > "$R/M1.log" 2>&1
hit=$(wait_hit 120 && echo yes || echo no); check M1-fault-fired yes "$hit"
if [ "$hit" = yes ]; then
  sleep 1
  STEP=M1 kill_pids $(host_pids $MODS_ALL)
  note "M1: the monitor's cancel trapped at the proxy; killed: $(killed M1)"
  check M1-every-module-host-killed "daemon 0 hosts 7" "$(killed M1 | cut -d' ' -f1-4)"
  check M1-nothing-left-before-restart 0 "$(cancels 1)"
  echo pass > "$CTL"
  STEP=M1 reload_hosts $MODS_ALL >> "$R/M1.log" 2>&1
  check M1-custody-after-restart "ok ok" "$CUSTODY $CUSTODY_M"
  mon_start M1b >> "$R/M1.log" 2>&1; mon_wait M1b >> "$R/M1.log" 2>&1
  note "M1: after the restart, the alerts: $(jq -s -c 'map({kind, via}) | .[0:6]' "$R/M1b-alerts.jsonl" 2>/dev/null)"
else
  note "M1: the trap never fired ($(cat "$CTL"))"; echo pass > "$CTL"; mon_wait M1a >> "$R/M1.log" 2>&1
fi
check M1-cancelled-once 1 "$(cancels 1)"
# What the restarted stack may add: tx_sender_module burns, after a restart, the number its lost request was signed at
# (send.rs: seeding), so its new cancel waits behind a gap the monitor fills the plain way at that number (the gap fill);
# the module's cancel then reverts (UnknownCandidate) and changes nothing. One such reverted call at most.
check M1-at-most-one-reverted-extra-call yes "$( [ "$(jq -s 'length' "$R/watcher-txs.jsonl")" -le 2 ] && [ "$(cancels 0)" -le 1 ] && echo yes || echo no)"
note "M1: the watcher's transactions: $(jq -s -c 'map({nonce, call, status})' "$R/watcher-txs.jsonl")"
check M1-candidate-gone "$zero" "$(cast call --rpc-url "$up" "$vault" 'candidates(bytes32)(bytes32,bytes32,uint64,uint32,uint48,uint32,uint64,address)' "$s3" | head -n 1)"
check M1-watcher-nothing-queued "$(cast nonce --rpc-url "$up" "$watcher")" "$(cast nonce --block pending --rpc-url "$up" "$watcher")"
check M1-watcher-nonces-without-gap "$(seq 0 $(( $(jq -s 'length' "$R/watcher-txs.jsonl") - 1 )) | paste -sd, -)" "$(jq -s -r 'map(.nonce) | sort | map(tostring) | join(",")' "$R/watcher-txs.jsonl")"

# The whole account.
relayer_txs
check total-relayer-transactions 7 "$(jq -s 'length' "$R/relayer-txs.jsonl")"
check nonces-without-gap "$(seq 0 6 | paste -sd, -)" "$(jq -s -r 'map(.nonce) | sort | map(tostring) | join(",")' "$R/relayer-txs.jsonl")"
check every-receipt-status-1 7 "$(jq -s '[.[] | select(.status == 1)] | length' "$R/relayer-txs.jsonl")"
check jobs-booked "k0-checkpoint.json,k2-release.json,k3-checkpoint.json,k5-checkpoint.json" "$(ls "$STATE/done" 2>/dev/null | paste -sd, -)"
check inbox-empty "" "$(ls "$INBOX" 2>/dev/null | grep -v '^done$' | paste -sd, -)"
check vault-final-state "$s2" "$(cast call --rpc-url "$up" "$vault" 'finalState()(bytes32)')"
check relayer-no-inflight-at-the-end none "$(inflight)"
check relayer-nothing-queued "$(cast nonce --rpc-url "$up" "$relayer")" "$(cast nonce --block pending --rpc-url "$up" "$relayer")"
c tx_sender_module history "$relayer" 31337 > /dev/null
printf '%s\n' "$LAST" > "$R/tx_sender-history.json"
cp "$S"/logs/*.log "$R/" 2>/dev/null || true
printf 'PASS %d FAIL %d\n' "$pass" "$fail" | tee "$OUT/summary.txt"
[ "$fail" = 0 ]
