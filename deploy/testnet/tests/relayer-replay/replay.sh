#!/usr/bin/env bash
# The live relayer's stall (23 Nov 2026: no finalizeAttested after the window), replayed: anvil forks Sepolia at the
# block before our submitCandidate (12,198,161); the deployed ferrata-relayer binary takes the very job it took
# (job.json, rebuilt from the chain). The relayer runs in a
# rootless mount namespace with its directories bind-mounted as the host's unit binds them (BindPaths).
#   A. state in one bind mount, inbox in another (the unit as deployed): submitted, then EXDEV, no finalize
#   C. A's state; the old inbox's job moved by hand; inbox now inside the state's mount (the fix): books and finalizes
#   B. fresh fork, inbox inside the relayer's own mount from the start: submit, then finalize after the window
#
#   replay.sh FERRATA_RELAYER [WORK_DIR]     needs anvil and cast (Foundry), unshare -rm, and an Ethereum endpoint that
#                                           serves Sepolia state at block 12,198,161 (FORK_URL; most free ones refuse
#                                           old state from cloud IPs). Record: results/2026-11-24-relayer-exdev-replay.txt
set -uo pipefail
W="$(cd "$(dirname "$0")" && pwd)"; REL="$(cd "$(dirname "${1:?usage: replay.sh FERRATA_RELAYER [WORK_DIR]}")" && pwd)/$(basename "$1")"
N="${2:-$W/work}"; mkdir -p "$N"; N="$(cd "$N" && pwd)"
UP=${FORK_URL:-https://ethereum-sepolia-rpc.publicnode.com}; RPC=http://127.0.0.1:8546
VAULT=0x19499061CeC0BaC2a3E1869Ba5AE6B403aA82787; FROM=0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266  # anvil's account 0
export FOUNDRY_DISABLE_NIGHTLY_WARNING=1
# the job, rebuilt from the chain (journal checked against the candidate's on-chain digest)
python3 -I "$W/job.py" "$UP" > "$N/job.json" || exit 1
echo "relayer sha256 $(sha256sum "$REL" | cut -c1-16)"
rm -rf "$N"/real-* "$N"/mnt-*; mkdir -p "$N"/{real-relayer/state,real-relayer/inbox,real-inbox,mnt-relayer,mnt-inbox,real-b/state,real-b/inbox,mnt-b}
APID=""
fork() { [ -n "$APID" ] && kill "$APID" 2>/dev/null && sleep 2
  anvil --port 8546 --fork-url "$UP" --fork-block-number 12198161 --auto-impersonate > "$N/anvil-$1.log" 2>&1 & APID=$!
  for _ in $(seq 90); do cast block-number --rpc-url $RPC >/dev/null 2>&1 && return 0; sleep 1; done
  echo "anvil did not start"; tail -n 5 "$N/anvil-$1.log"; exit 1; }
# round STATE INBOX BINDS...: one daemon round inside a mount namespace with the given "src:dst" bind mounts
round() { local st="$1" ib="$2"; shift 2; local m=""; for b in "$@"; do m="$m mount --bind ${b%%:*} ${b#*:} &&"; done
  unshare -rm bash -c "$m '$REL' --eth-rpc $RPC --vault $VAULT --chain-id 11155111 --from-block 12196211 --unlocked $FROM --state '$st' run --inbox '$ib' --rounds 1 --poll-secs 0" 2>&1 \
  | python3 -I -u -c '
import json,sys
for l in sys.stdin:
    try: e = json.loads(l)
    except Exception: print("    raw", l.strip()[:200]); continue
    if e.get("event") == "start": continue
    print("   ", json.dumps({k: e[k] for k in e if k in ("event","job","reason","tx","newState","readyAt","now","key","recovered","result")})[:330])'; }
warp() { cast rpc --rpc-url $RPC evm_increaseTime 3700 >/dev/null; cast rpc --rpc-url $RPC evm_mine >/dev/null; }
count() { echo "    checkpointCount on the fork: $(cast call --rpc-url $RPC $VAULT 'checkpointCount()(uint256)')"; }
echo "== A: state and inbox in two bind mounts (as deployed)"
fork a; cp "$N/job.json" "$N/real-inbox/zone-1225-3767.json"
A=("$N/real-relayer:$N/mnt-relayer" "$N/real-inbox:$N/mnt-inbox")
echo "  round 1"; round "$N/mnt-relayer/state" "$N/mnt-inbox" "${A[@]}"
warp; echo "  round 2 (after the 3,600 s window)"; round "$N/mnt-relayer/state" "$N/mnt-inbox" "${A[@]}"
echo "  round 3"; round "$N/mnt-relayer/state" "$N/mnt-inbox" "${A[@]}"; count
echo "    in flight: $(python3 -I -c 'import json,sys; d=json.load(open(sys.argv[1])); print(d["label"], "job", d["job"], "nonce", d["nonce"])' "$N/real-relayer/state/inflight.json" 2>&1)"
echo "== C: the fix on A's state: the old inbox's job moved by hand, the inbox inside the relayer's own mount"
mkdir -p "$N/real-relayer/state/done"; mv "$N/real-inbox/zone-1225-3767.json" "$N/real-relayer/state/done/zone-1225-3767.json"
echo "  round 4"; round "$N/mnt-relayer/state" "$N/mnt-relayer/inbox" "$N/real-relayer:$N/mnt-relayer"; count
echo "== B: fresh fork; inbox inside the relayer's own mount from the start"
fork b; cp "$N/job.json" "$N/real-b/inbox/zone-1225-3767.json"
echo "  round 1"; round "$N/mnt-b/state" "$N/mnt-b/inbox" "$N/real-b:$N/mnt-b"
warp; echo "  round 2 (after the window)"; round "$N/mnt-b/state" "$N/mnt-b/inbox" "$N/real-b:$N/mnt-b"; count
kill "$APID"
