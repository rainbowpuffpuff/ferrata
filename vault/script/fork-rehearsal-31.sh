#!/usr/bin/env bash
# The RISC Zero 3.1 deployment (docs/design/risc0-3.1-migration.md, phase 1) rehearsed on a local anvil fork of
# Sepolia with real proofs. No key is used and nothing is sent to Sepolia: anvil reads Sepolia's state over RPC and keeps
# every transaction to itself; the accounts are impersonated on the fork.
#
#   1. script/deploy.sh as the owner runs it on Sepolia (rule (B), gas rule eip8037, capped defaults), with decision D1
#      (a): our own RISC Zero Groth16 verifier 3.1.0 behind RISC Zero's emergency stop, its owner renounced; a freeze
#      authority and a watcher with keys (here fork-only addresses); FERRATA_EXPECT_VAULT holding the vault to the address
#      the zone's configuration names. The deployer is that of the recorded zone session e2e/runs/zone-bridge-rc3 (a
#      LEZ v0.4.0-rc3 sequencer on Logos Blockchain 0.3.5-rc.1: deployment, deposits imported, a public and a private
#      claim, a public and a private burn, its rule (B) checkpoint), whose nonce on Sepolia is 0, so the vault lands at
#      the address that session's configuration and proofs bind.
#   2. The round trip on it (script/Rehearsal31.s.sol): the binding checked; the session's two deposits; the session's
#      checkpoint with the seal its run made under risc0 3.0.5, refused (SelectorMismatch); the same checkpoint proven
#      again under RISC Zero 3.1 (test/real/fixtures/zone-bridge-rc3.json), submitted, final after Δ = 1 h; releases
#      frozen by the freeze authority and a release refused; unfrozen; both releases paid with their 3.1 seals; a replay
#      refused.
#
#   script/fork-rehearsal-31.sh [<log directory>]     FERRATA_FORK_URL: a public Sepolia RPC (default publicnode)
# Prints FORK REHEARSAL 31 OK at the end; <log directory>/transcript.txt has every step with its gas and time.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
cd "$here"
export FOUNDRY_DISABLE_NIGHTLY_WARNING=1
logs="${1:-broadcast/fork-rehearsal-31}"
mkdir -p "$logs"
logs="$(cd "$logs" && pwd)"
run="../e2e/runs/zone-bridge-rc3"
fixture="test/real/fixtures/zone-bridge-rc3.json"
fork_url="${FERRATA_FORK_URL:-https://ethereum-sepolia-rpc.publicnode.com}"
port="${FERRATA_FORK_PORT:-8545}"
rpc="http://127.0.0.1:$port"
T="$logs/transcript.txt"; : > "$T"
mkdir -p deployments
say() { echo "$(date -u +%H:%M:%S) $*" | tee -a "$T"; }
die() { say "STOP: $*"; exit 1; }
[ "$(jq -r '.seal[0:10]' "$fixture")" = 0xe71e6620 ] || die "$fixture is not proven under RISC Zero 3.1 (scripts/reprove_fixtures.py)"

anvil --fork-url "$fork_url" --port "$port" > "$logs/anvil.log" 2>&1 &
anvil_pid=$!
trap 'kill "$anvil_pid" 2>/dev/null || true' EXIT
for _ in $(seq 1 90); do cast chain-id --rpc-url "$rpc" > /dev/null 2>&1 && break; sleep 1; done
chain="$(cast chain-id --rpc-url "$rpc")"
[ "$chain" = 11155111 ] || die "the fork is of chain $chain, not Sepolia"
say "fork of Sepolia at block $(cast block-number --rpc-url "$rpc") ($(cast block --rpc-url "$rpc" -f timestamp | xargs -I{} date -u -d @{} +%FT%TZ))"

addr() { local h; h="$(cast keccak "$1")"; cast to-check-sum-address "0x${h:26:40}"; }
deployer="$(jq -r .deployer "$run/identity.json")"
expect="$(jq -r .vault "$run/identity.json")"
freeze="$(addr 'ferrata fork rehearsal 3.1: freeze authority')"
watcher="$(addr 'ferrata fork rehearsal 3.1: watcher')"
relayer="$(addr 'ferrata fork rehearsal 3.1: relayer')"
recipient="$(jq -r '.releases[0].recipient' "$fixture")"
nonce="$(cast nonce "$deployer" --rpc-url "$rpc")"
[ "$nonce" = 0 ] || die "the session's deployer $deployer has nonce $nonce on Sepolia, not 0: its vault address is taken"
[ "$(cast compute-address "$deployer" --nonce 3 | awk '{print $NF}')" = "$expect" ] || die "CREATE($deployer, 3) is not $expect"
for a in "$deployer" "$freeze" "$watcher" "$relayer"; do
  cast rpc --rpc-url "$rpc" anvil_impersonateAccount "$a" > /dev/null
  cast rpc --rpc-url "$rpc" anvil_setBalance "$a" 0x3635c9adc5dea00000 > /dev/null # 1000 ETH, on the fork only
done
say "deployer $deployer (nonce 0 on Sepolia), the vault expected at $expect; freeze authority $freeze, watcher $watcher"

# The session's configuration and genesis; the programs of RISC Zero 3.1 (the fixture's ids: docs/image-ids.md).
jq --arg b "$(jq -r .imageId "$fixture")" --arg r "$(jq -r .releaseImageId "$fixture")" \
  '._source = "fork-rehearsal-31.sh: e2e/runs/zone-bridge-rc3/params.json with the RISC Zero 3.1 programs" |
   .checkpointImageIdB = $b | .releaseImageId = $r |
   .checkpointImageIdA = "0x9140ec7479debc9b40529b0805095d4cfdef93a1e0500c6687a1d7ec4d7c42de"' \
  "$run/params.json" > deployments/fork-31-params.json   # forge reads ./deployments (foundry.toml fs_permissions)
cp deployments/fork-31-params.json "$logs/params.json"
export FERRATA_RPC_URL="$rpc" FERRATA_UNLOCKED=1 ETH_FROM="$deployer" FERRATA_LOG_DIR="$logs/deploy"
export FERRATA_MODE=B FERRATA_GAS_RULE=eip8037 FERRATA_PARAMS=deployments/fork-31-params.json FERRATA_DELTA=3600
export FERRATA_FREEZE_AUTHORITY="$freeze" FERRATA_WATCHERS="$watcher" FERRATA_EXPECT_VAULT="$expect"
export FERRATA_OUT="deployments/fork-31-$chain-B.json"
rm -f "$FERRATA_OUT"
s=$(date +%s)
script/deploy.sh > "$logs/deploy.txt" 2>&1 || { tail -n 30 "$logs/deploy.txt"; die "deploy.sh failed"; }
grep -E '^FERRATA_SENT' "$logs/deploy.txt" | tee -a "$T"
say "deployed in $(( $(date +%s) - s )) s: record $FERRATA_OUT"
cp "$FERRATA_OUT" "$logs/deployment.json"
vault="$(jq -r .vault "$FERRATA_OUT")"; stop="$(jq -r .emergencyStop "$FERRATA_OUT")"; g16="$(jq -r .groth16Verifier "$FERRATA_OUT")"
c() { cast call --rpc-url "$rpc" "$@"; }
[ "$vault" = "$expect" ] || die "vault at $vault"
[ "$(c "$g16" 'VERSION()(string)')" = '"3.1.0"' ] || die "verifier version"
[ "$(c "$g16" 'SELECTOR()(bytes4)')" = 0xe71e6620 ] || die "verifier selector"
[ "$(c "$stop" 'owner()(address)')" = 0x0000000000000000000000000000000000000000 ] || die "the stop has an owner"
[ "$(c "$vault" 'verifier()(address)')" = "$stop" ] || die "the vault's verifier is not the stop"
[ "$(c "$vault" 'freezeAuthority()(address)')" = "$freeze" ] || die "freeze authority"
say "verifier $g16 VERSION 3.1.0 SELECTOR 0xe71e6620; stop $stop, owner renounced; vault $vault; freeze authority set"
say "caps (capacity, refill/s, available): deposit $(c "$vault" 'globalCap(uint8)(uint256,uint256,uint256)' 0 | tr '\n' ' ')/ release $(c "$vault" 'globalCap(uint8)(uint256,uint256,uint256)' 1 | tr '\n' ' ')/ release floor $(c "$vault" 'releaseCapFloor()(uint256)')"

step() { # <signature> <sender> <send: 1|0> [args...]
  local sig="$1" from="$2" send="$3" name log b s; shift 3
  name="$(echo "$sig" | cut -d'(' -f1)"; log="$logs/$name-$(date +%s%N).log"
  b=(); [ "$send" = 1 ] && b=(--broadcast)
  s=$(date +%s.%N)
  if ! env -u ETH_FROM forge script script/Rehearsal31.s.sol:Rehearsal31 --sig "$sig" "$@" --rpc-url "$rpc" \
      --unlocked --sender "$from" --skip-simulation --slow ${b[@]+"${b[@]}"} -vv > "$log" 2>&1; then
    tail -n 25 "$log"; die "$sig failed (log $log)"
  fi
  local gas=""
  if [ "$send" = 1 ]; then
    gas="$(jq -r '[.receipts[] | (.gasUsed | tostring)] | join(",")' "broadcast/Rehearsal31.s.sol/$chain/$name-latest.json" \
      | python3 -I -c 'import sys; print(",".join(str(int(x, 16) if x.startswith("0x") else int(x)) for x in sys.stdin.read().strip().split(",") if x))')"
    gas=", gas $gas"
  fi
  say "$(grep -h -o 'FERRATA_OK.*' "$log" | tr '\n' ' ')($(python3 -I -c "print(round($(date +%s.%N) - $s, 1))") s$gas)"
}
warp() { cast rpc --rpc-url "$rpc" evm_increaseTime "$1" > /dev/null && cast rpc --rpc-url "$rpc" evm_mine > /dev/null; say "fork clock +$1 s"; }
export FERRATA_FIXTURE="$fixture"

step "checkBinding()" "$relayer" 0
step "deposits()" "$deployer" 1
step "oldSealRefused(bytes)" "$relayer" 0 "0x$(tr -d ' \n' < "$run/checkpoint/receipts/seal.hex" | sed 's/^0x//')"
step "submit()" "$relayer" 1
warp 3601
step "finalize()" "$relayer" 1
warp 121
step "setReleasesFrozen(bool)" "$freeze" 1 true
step "refused(uint256,bytes4)" "$relayer" 0 0 "$(cast sig 'ReleasesAreFrozen()')"
step "setReleasesFrozen(bool)" "$freeze" 1 false
before="$(cast balance "$recipient" --rpc-url "$rpc")"
step "release(uint256)" "$relayer" 1 0
step "release(uint256)" "$relayer" 1 1
step "refused(uint256,bytes4)" "$relayer" 0 0 "$(cast sig 'NullifierAlreadyUsed()')"
after="$(cast balance "$recipient" --rpc-url "$rpc")"
say "recipient $recipient received $(python3 -I -c "print($after - $before)") wei for the two releases"
say "FORK REHEARSAL 31 OK"
