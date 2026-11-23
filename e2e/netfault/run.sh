#!/usr/bin/env bash
# e2e/netfault/run.sh <seed>: e2e/crown/sepolia.sh's stages (deploy, deposit 0, deposit 1, submit, finalize, release 0,
# release 1) on a local anvil (1 s blocks), through faultnet.py's three bad endpoints (seeded), with a fresh keystore
# (forge signs: raw transactions, the live path). Stand-in identity and the accept-all verifier as crown-fork-3r
# (e2e/runs/crown-fork-3r/README.md). A transient stop (exit 75) is resumed up to 4 times, as drive.py does. Then
# check.py compares the chain (read from anvil directly) with the network journal. One JSON line per seed is appended
# to e2e/netfault/results/$NETFAULT_BATCH.jsonl (default seeds). NETFAULT_STAGE_BUDGET_S (90): a stage's budget; a
# small one turns the outages into transient stops (exit 75) and resumes. Nothing here talks to a public network.
set -euo pipefail
here="$(cd "$(dirname "$0")/../.." && pwd)"
seed="${1:?seed}"
name="netfault-$seed"; run="$here/e2e/runs/$name"; w="$run/w"
rm -rf "$run" "$here/vault/deployments/crown-$name.json"; mkdir -p "$w"
out="$here/e2e/netfault/results"; mkdir -p "$out"
cid="${NETFAULT_CHAIN_ID:-31337}" # DeployVault deploys on anvil, Sepolia, Hoodi, Holesky ids only (many.sh: one lane each)
export FOUNDRY_DISABLE_NIGHTLY_WARNING=1 TMPDIR="$w"
free_port() { python3 -I -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])'; }
aport="$(free_port)"; R="http://127.0.0.1:$aport"
anvil --port "$aport" --chain-id "$cid" --block-time 1 --gas-limit 100000000 > "$w/anvil.log" 2>&1 & A=$!
# The deployer: a fresh key in a keystore (funded on anvil below).
mkdir -p "$w/key"; cast wallet new "$w/key" --unsafe-password netfault > "$w/key.txt"
deployer="$(grep -o -E '0x[0-9a-fA-F]{40}' "$w/key.txt" | head -n 1)"
printf netfault > "$w/pw"
python3 -I "$here/e2e/netfault/faultnet.py" --upstream "$R" --seed "$seed" --port-file "$w/ports" --log "$w/faults.jsonl" \
  --ctl "$w/liar.ctl" --sender "$deployer" \
  > "$w/faultnet.json" 2> "$w/faultnet.err" & F=$!
trap 'kill $A $F 2>/dev/null || true; wait 2>/dev/null || true' EXIT
for _ in $(seq 1 100); do cast chain-id --rpc-url "$R" > /dev/null 2>&1 && [ -s "$w/ports" ] && break; sleep 0.2; done
mapfile -t P < "$w/ports"
t0=$(date +%s)
cast rpc anvil_setBalance "$deployer" 0x3635c9adc5dea00000 --rpc-url "$R" > /dev/null
ACC0=0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
( cd "$here/vault" && forge create script/ForkRehearsal.s.sol:RehearsalAcceptAllVerifier --rpc-url "$R" --unlocked \
    --from "$ACC0" --broadcast ) > "$w/verifier.log" 2>&1
VER="$(grep -o 'Deployed to: 0x[0-9a-fA-F]*' "$w/verifier.log" | awk '{print $3}')"
sleep 5 # the verifier older than the pinned block (reads lag 2 blocks): on Sepolia the verifier is months old
VAULT="$(cast compute-address "$deployer" --nonce 2 | awk '{print $NF}')"
h() { printf '%s' "$1" | sha256sum | cut -c1-64 | sed 's/^/0x/'; }
CID="$(h 'netfault: stand-in configId')"; TIP="$(h 'netfault: stand-in tip')"; KEY="$(h 'netfault: stand-in key')"
GS="$(python3 -I "$here/e2e/crown/identity.py" genesis-state --tip-id 9 --tip-hash "$TIP" --keys "$KEY")"
Z=0x0000000000000000000000000000000000000000000000000000000000000000
jq -n --arg v "$VAULT" --arg c "$CID" --arg g "$GS" --arg ver "$VER" --argjson cid "$cid" '{label: "STAND-IN (netfault test)",
  chainId: $cid, vault: $v, configId: $c, genesisState: $g, groth16Verifier: $ver, delta: 3600}' > "$run/identity.json"
jq -n --arg c "$CID" --arg g "$GS" --arg a "$(h 'netfault A')" --arg b "$(h 'netfault B')" --arg r "$(h 'netfault R')" \
  --arg z "$Z" '{_source: "STAND-IN (netfault test)", checkpointImageIdA: $a, checkpointImageIdB: $b, releaseImageId: $r,
  lezConfigId: $c, genesisState: $g, nMin: 1, tMin: 1, postingTimeframe: 0, ethLezDefinition: $z, tokenLezDefinition: $z}' \
  > "$run/params.json"
NEW="$(h 'netfault new state')"; BR="$(h 'netfault burn root')"; TIP2="$(h 'netfault tip 2')"
J="$(cast abi-encode 'f(bytes32,uint256,address,bytes32,(uint32,uint32,bytes32,bytes32,bytes32,uint64,uint64,bool,bytes32,uint64,bytes32,uint32,uint32))' \
  0xbdb3b52df781536a6bcce7088d156b3951a00a45ec8bcf2805ba8406fbeb5ec7 "$cid" "$VAULT" "$CID" "(1,1,$GS,$NEW,$BR,2,2,false,$Z,20,$TIP2,1,1)")"
jq -n --arg v "$VAULT" --argjson cid "$cid" --arg c0 "$(h 'netfault c0')" --arg c1 "$(h 'netfault c1')" --arg j "$J" \
  --arg br "$BR" --arg n0 "$(h 'netfault n0')" --arg n1 "$(h 'netfault n1')" --arg to "$deployer" '{chainId: $cid,
  vault: $v, deposits: [{j: 0, denomIndex: 0, commitment: $c0}, {j: 1, denomIndex: 0, commitment: $c1}],
  candidate: {journal: $j, seal: "0xe71e6620"}, releases: [range(2) as $i | {token: "0x0000000000000000000000000000000000000000",
  denomIndex: 0, recipient: $to, relayer: "0x0000000000000000000000000000000000000000", relayFee: "0", burnRoot: $br,
  nullifier: (if $i == 0 then $n0 else $n1 end), seal: "0xe71e6620"}]}' > "$run/crown.json"

# sepolia.sh through the three bad endpoints; the first is "the Secret's".
export SEPOLIA_RPC_URL="http://127.0.0.1:${P[0]}"
export PUBNET_SEPOLIA_RPC="http://127.0.0.1:${P[0]},http://127.0.0.1:${P[1]},http://127.0.0.1:${P[2]},http://127.0.0.1:${P[3]}"
export ETH_FROM="$deployer" ETH_KEYSTORE="$(ls "$w"/key/* | head -n 1)" ETH_PASSWORD="$w/pw"
export PUBNET_LAG=2 PUBNET_TIMEOUT_S=3 PUBNET_REQUEST_BUDGET_S=20 PUBNET_PIN_TTL_S=2 PUBNET_BACKOFF_BASE_S=2 \
  PUBNET_STAGE_BUDGET_S="${NETFAULT_STAGE_BUDGET_S:-90}" PUBNET_RESOLVE_BUDGET_S=60 PUBNET_ATTEMPT_TIMEOUT_S=240 \
  PUBNET_REBROADCAST_S=10
resumes=0
stage() {
  local k rc=0
  for k in 0 1 2 3 4; do
    rc=0
    echo "[$(date -u +%T)] == sepolia.sh $*" >> "$w/stages.log"
    "$here/e2e/crown/sepolia.sh" "$run" "$@" >> "$w/stages.log" 2>&1 || rc=$?
    [ "$rc" = 75 ] && [ "$k" -lt 4 ] || break
    resumes=$((resumes + 1)); echo "[$(date -u +%T)] transient stop at $*: resume $((k + 1)) of 4" >> "$w/stages.log"; sleep 3
  done
  return "$rc"
}
warp() { cast rpc evm_increaseTime "$1" --rpc-url "$R" > /dev/null; cast rpc evm_mine --rpc-url "$R" > /dev/null; sleep 4; }
outcome=complete; stopped=""; rc=0
for s in "deploy" "deposit 0" "deposit 1" "submit" "warp 3601" "finalize" "warp 121" "release 0" "release 1"; do
  case "$s" in warp*) warp "${s#warp }"; continue ;; esac
  # shellcheck disable=SC2086
  stage $s || { rc=$?; outcome="stopped-$([ "$rc" = 75 ] && echo transient || echo permanent)"; stopped="$s"; break; }
  # From now on the frozen liar (seeds 0 mod 4) answers contract state as it was right after the deployment.
  [ "$s" = deploy ] && cast block-number --rpc-url "$R" > "$w/liar.ctl"
done
# Replays, as a resumed stage plans again: the journal's skip off (PUBNET_NO_JOURNAL_SKIP=1), so only the reads (two
# endpoints alike) and the journal's refusal of a repeated call stand between a stale read and a second deposit or
# release. First through all four endpoints; then, for the liar seeds, through the liar and one honest endpoint only.
replays=""
if [ "$outcome" = complete ]; then
  for s in "deposit 0" "deposit 1" "release 0"; do
    rrc=0
    # shellcheck disable=SC2086
    PUBNET_NO_JOURNAL_SKIP=1 PUBNET_STAGE_BUDGET_S=20 "$here/e2e/crown/sepolia.sh" "$run" $s >> "$w/replays.log" 2>&1 || rrc=$?
    replays+="all:${s// /}=$rrc,"
  done
  liar="$(jq -r '.endpoints[] | select(.kind | startswith("liar")) | .port' "$w/faultnet.json")"
  honest="$(jq -r '.endpoints[] | select(.kind == "sends") | .port' "$w/faultnet.json")"
  if [ -n "$liar" ]; then
    for s in "deposit 0" "release 0"; do
      rrc=0
      # shellcheck disable=SC2086
      SEPOLIA_RPC_URL="http://127.0.0.1:$liar" PUBNET_SEPOLIA_RPC="http://127.0.0.1:$liar,http://127.0.0.1:$honest" \
        PUBNET_NO_JOURNAL_SKIP=1 PUBNET_STAGE_BUDGET_S=20 "$here/e2e/crown/sepolia.sh" "$run" $s >> "$w/replays.log" 2>&1 || rrc=$?
      replays+="liar+1:${s// /}=$rrc,"
    done
  fi
fi
python3 -I "$here/e2e/netfault/check.py" --rpc "$R" --run "$run" --deployer "$deployer" --vault "$VAULT" \
  --outcome "$outcome" --stopped "$stopped" --resumes "$resumes" --seed "$seed" --seconds "$(( $(date +%s) - t0 ))" \
  --replays "${replays%,}" --liar "$(jq -r '.endpoints[3].kind' "$w/faultnet.json")" \
  >> "$out/${NETFAULT_BATCH:-seeds}.jsonl"
tail -n 1 "$out/${NETFAULT_BATCH:-seeds}.jsonl"
