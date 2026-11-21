#!/usr/bin/env bash
# Rehearses, on a local anvil fork of Sepolia, (A) what the RISC Zero 3.0.x forgery (GHSA-mm6c-35cm-g4x2) allows
# against the live vaults and (B, C) what the owner's deployer key can do about it. Nothing is sent to Sepolia:
# anvil reads Sepolia's state at one block over RPC and keeps every transaction to itself; every `cast send` below
# goes to the loopback anvil only (checked: web3_clientVersion must name anvil). No key is used: anvil impersonates.
#
# The forgery itself is not built. Its effect, "the 3.0.0 Groth16 verifier accepts a chosen claim", is stood in for
# by replacing the code of our Groth16 verifier deployment (0x8f0A…A2A0) on the fork with a single STOP opcode, which
# returns success for every call, so every seal is accepted, as the advisory says a forged one is.
#
# Usage: fork-rehearsal.sh FORK_RPC_URL BLOCK OUTDIR
# FORK_RPC_URL must serve historical state at BLOCK (an archive node, e.g. https://rpc.sepolia.ethpandaops.io);
# publicnode does not, and anvil then exits at start.
set -u
FORK_URL="$1"; B="$2"; OUT="$3"
PORT=18645
L="http://127.0.0.1:$PORT"
mkdir -p "$OUT"
LOG="$OUT/fork-rehearsal.txt"
: > "$LOG"
say() { echo "$*" | tee -a "$LOG"; }

anvil --fork-url "$FORK_URL" --fork-block-number "$B" --port "$PORT" --auto-impersonate --no-storage-caching \
  --silent > "$OUT/anvil.log" 2>&1 &
APID=$!
trap 'kill $APID 2>/dev/null; wait $APID 2>/dev/null' EXIT
for i in $(seq 1 60); do cast chain-id --rpc-url "$L" >/dev/null 2>&1 && break; kill -0 $APID 2>/dev/null || break; sleep 1; done
kill -0 $APID 2>/dev/null || { echo "anvil exited: $(tail -n 3 "$OUT/anvil.log" | tr '\n' ' ')" >&2
  echo "the fork needs a reachable endpoint that serves historical state at block $B" >&2; exit 1; }
CV=$(cast rpc --rpc-url "$L" web3_clientVersion 2>&1)
case "$CV" in *anvil*) ;; *) echo "not anvil: $CV"; exit 1 ;; esac
say "# anvil fork of Sepolia at block $B ($(cast chain-id --rpc-url "$L")), client $CV"

ETH=0x0000000000000000000000000000000000000000
G16=0x8f0Af3Bf302C64d57f840127F94d9c123a8Fa2A0
ADMIN=0x79A95aE9A3f9000eb41F7CBAB6a6F97826895C07
V3=0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c     # crown-live-3 vault (v5)
V1=0xfE3199fa2634413CbAC317371076Cae3EB2d9E54     # crown-live-1 vault (v5)
V4=0x80555Fe53CCcEf46951B23A3c75643a4C9283470     # crown-live-4 vault (v5)
VOLD=0xd0B5fcC0B47D8E6f514f9D246Bb0b416b2048AF7   # v3 vault (source c6aee464)
VICTIM=0x1111111111111111111111111111111111111111
ATTACKER=0x2222222222222222222222222222222222222222
NEWFA=0x3333333333333333333333333333333333333333
WATCHER=0x4444444444444444444444444444444444444444
for a in $VICTIM $ATTACKER $NEWFA $WATCHER; do
  [ "$(cast code --rpc-url "$L" $a)" = "0x" ] || { echo "$a has code on Sepolia"; exit 1; }
  cast rpc --rpc-url "$L" anvil_setBalance $a 0x8AC7230489E80000 >/dev/null   # 10 ETH, on the fork only
done
UNL=115792089237316195423570985008687907853269984665640564039457584007913129639935

# error selectors, to name reverts
declare -A ERR
for e in GlobalReleaseCapExceeded GlobalDepositCapExceeded TokenReleaseCapExceeded ReleasesAreFrozen \
  CheckpointsAreFrozen DepositsAreFrozen VaultRetired TimelockNotOver InsufficientEscrow BurnRootNotAccepted \
  NotAdmin NotFreezeAuthority NotWatcher VerifierStopped WaitNotOver RetireNotOpen NullifierAlreadyUsed \
  ReleaseDelayNotElapsed CandidateMismatch UnknownCandidate StaleCandidate ParentNotFinal; do
  ERR[$(cast sig "$e()")]=$e
done
name_revert() {
  local sel
  sel=$(grep -o -E 'custom error 0x[0-9a-fA-F]{8}' <<<"$1" | head -1 | grep -o -E '0x[0-9a-fA-F]{8}')
  [ -n "$sel" ] || sel=$(grep -o -E 'data: \\?"0x[0-9a-fA-F]{8}' <<<"$1" | head -1 | grep -o -E '0x[0-9a-fA-F]{8}')
  if [ -n "$sel" ]; then echo "${ERR[$sel]:-$sel}"; else echo "$(tr '\n' ' ' <<<"$1" | cut -c1-160)"; fi
}

# [VALUE=wei] send FROM TO SIG ARGS...: prints "ok gas=N" or "REVERT <name>"
send() {
  local from="$1" to="$2"; shift 2
  local out val=()
  [ -n "${VALUE:-}" ] && val=(--value "$VALUE")
  if out=$(cast send --rpc-url "$L" --unlocked --from "$from" --json "${val[@]}" "$to" "$@" 2>&1); then
    local st gas
    st=$(python3 -I -c 'import json,sys; d=json.loads(sys.stdin.read()); print(int(d["status"],16), int(d["gasUsed"],16))' <<<"$out")
    set -- $st
    if [ "$1" = 1 ]; then echo "ok gas=$2"; else echo "REVERT status0 gas=$2"; fi
  else
    echo "REVERT $(name_revert "$out")"
  fi
}
call() { cast call --rpc-url "$L" "$@" 2>&1 | tr '\n' ' '; }
bal() { cast balance --rpc-url "$L" "$1"; }
snap() { cast rpc --rpc-url "$L" evm_snapshot | tr -d '"'; }
revert_to() { cast rpc --rpc-url "$L" evm_revert "$1" >/dev/null; }
warp() { cast rpc --rpc-url "$L" evm_increaseTime "$1" >/dev/null; cast rpc --rpc-url "$L" evm_mine >/dev/null; }
stub_verifier() { cast rpc --rpc-url "$L" anvil_setCode $G16 0x00 >/dev/null; }

FEEIDX=$(cast call --rpc-url "$L" $V3 "feeState(address)(uint64,uint64)" $ETH | head -1)
FEES=$(cast call --rpc-url "$L" $V3 "fees(address,uint256,uint8)(uint256,uint256,uint256,uint256)" $ETH "$FEEIDX" 0 | awk '{print $1}' | tr '\n' ' ')
set -- $FEES; MINT=$1; RELAYF=$2; BURNF=$3; CPF=$4
DEP=$(python3 -I -c "print(10**15 + $MINT + $RELAYF)")
say "# vault $V3: ETH fee schedule $FEEIDX, 0.001 ETH denomination: mint $MINT, relay $RELAYF, burn $BURNF, checkpoint $CPF; a deposit sends $DEP wei"
ROOT=$(cast call --rpc-url "$L" $V3 "checkpoints(uint256)(bytes32,bytes32,bytes32,uint64,uint64,bool,bool,uint64,bool,uint64,address)" 0 | sed -n 3p)
say "# the burn root its one checkpoint accepted: $ROOT"
COMMIT=$(cast keccak "advisory fork rehearsal: victim deposit")

release_req() { echo "($ETH,0,$ATTACKER,$ETH,0,$1,$2)"; }   # root nullifier
RSIG="release((address,uint8,address,address,uint256,bytes32,bytes32),bytes)"
CSIG_T="(uint32,uint32,bytes32,bytes32,bytes32,uint64,uint64,bool,bytes32,uint64,bytes32,uint32,uint32)"
forged_cp() { # newState burnRoot -> checkpoint tuple on the current finalState
  local fs il; fs=$(cast call --rpc-url "$L" $V3 "finalState()(bytes32)"); il=$(cast call --rpc-url "$L" $V3 "nextLeafIndex()(uint64)")
  echo "(1,1,$fs,$1,$2,7,$il,false,0x0000000000000000000000000000000000000000000000000000000000000000,1,$(cast keccak tip),1,1)"
}

S0=$(snap)

say ""
say "## A. Exposure: the advisory's forgery against vault $V3 as deployed (stand-in: verifier code replaced by STOP)"
stub_verifier
say "victim deposit 0 (0.001 ETH): $(VALUE=$DEP send $VICTIM $V3 'depositETH(uint8,bytes32,uint64)' 0 $COMMIT 0)"
say "victim deposit 1 (0.001 ETH): $(VALUE=$DEP send $VICTIM $V3 'depositETH(uint8,bytes32,uint64)' 0 $COMMIT 1)"
say "escrowed ETH now: $(call $V3 'escrowed(address)(uint256)' $ETH)"
A0=$(bal $ATTACKER)
say "forged release against the accepted root, fresh nullifier, attacker as recipient, junk seal: $(send $ATTACKER $V3 "$RSIG" "$(release_req $ROOT $(cast keccak n1))" 0x6a756e6b)"
A1=$(bal $ATTACKER)
say "attacker balance change (wei, after its gas): $(python3 -I -c "print($A1 - $A0)")"
NS=$(cast keccak "forged state 1"); NR=$(cast keccak "forged burn root 1")
CP=$(forged_cp $NS $NR)
say "forged checkpoint submitCandidate (junk seal): $(send $ATTACKER $V3 "submitCandidate($CSIG_T,bytes)" "$CP" 0x6a756e6b)"
warp 3601
say "finalizeAttested after the 3600 s wait: $(send $ATTACKER $V3 "finalizeAttested($CSIG_T)" "$CP")"
warp 121   # the release delay of 120 s
say "forged release against the forged root: $(send $ATTACKER $V3 "$RSIG" "$(release_req $NR $(cast keccak n2))" 0x6a756e6b)"
A2=$(bal $ATTACKER)
say "attacker balance change in A (wei, after gas): $(python3 -I -c "print($A2 - $A0)"); escrowed ETH left: $(call $V3 'escrowed(address)(uint256)' $ETH)"

revert_to "$S0"; S0=$(snap)
say ""
say "## B. The deployer key (vault admin), now: release and deposit caps to zero"
say "victim deposit 0 before the change (value already in the vault): $(VALUE=$DEP send $VICTIM $V3 'depositETH(uint8,bytes32,uint64)' 0 $COMMIT 0)"
for v in $V3 $V1 $V4 $VOLD; do
  say "$v setGlobalCap(Release, 0, 0) by the admin: $(send $ADMIN $v 'setGlobalCap(uint8,uint256,uint256)' 1 0 0)"
  say "$v setGlobalCap(Deposit, 0, 0) by the admin: $(send $ADMIN $v 'setGlobalCap(uint8,uint256,uint256)' 0 0 0)"
  say "$v globalCap(Release) now: $(call $v 'globalCap(uint8)(uint256,uint256,uint256)' 1)"
done
say "the same call from another address: $(send $ATTACKER $V3 'setGlobalCap(uint8,uint256,uint256)' 1 $UNL 0)"
stub_verifier
say "forged release against the accepted root: $(send $ATTACKER $V3 "$RSIG" "$(release_req $ROOT $(cast keccak n1))" 0x6a756e6b)"
say "new victim deposit: $(VALUE=$DEP send $VICTIM $V3 'depositETH(uint8,bytes32,uint64)' 0 $COMMIT 1)"
CP=$(forged_cp $NS $NR)
say "forged checkpoint submitCandidate (caps do not stop checkpoints): $(send $ATTACKER $V3 "submitCandidate($CSIG_T,bytes)" "$CP" 0x6a756e6b)"
warp 3601
say "finalizeAttested: $(send $ATTACKER $V3 "finalizeAttested($CSIG_T)" "$CP")"
warp 121
say "forged release against the forged root: $(send $ATTACKER $V3 "$RSIG" "$(release_req $NR $(cast keccak n2))" 0x6a756e6b)"
say "escrowed ETH still held: $(call $V3 'escrowed(address)(uint256)' $ETH)"
say "reversible: setGlobalCap(Release, UNLIMITED, 0) by the admin: $(send $ADMIN $V3 'setGlobalCap(uint8,uint256,uint256)' 1 $UNL 0)"
say "then the forged release goes through again: $(send $ATTACKER $V3 "$RSIG" "$(release_req $ROOT $(cast keccak n3))" 0x6a756e6b)"

revert_to "$S0"; S0=$(snap)
say ""
say "## C. The deployer key, slower: a freeze authority and a watcher through the 14-day role timelock"
say "victim deposit 0: $(VALUE=$DEP send $VICTIM $V3 'depositETH(uint8,bytes32,uint64)' 0 $COMMIT 0)"
say "proposeRoles([watcher,0,0], new freeze authority) by the admin: $(send $ADMIN $V3 'proposeRoles(address[3],address)' "[$WATCHER,$ETH,$ETH]" $NEWFA)"
say "applyRoles at once: $(send $ATTACKER $V3 'applyRoles()')"
warp $((14*86400 + 1))
say "applyRoles after 14 days + 1 s (anyone): $(send $ATTACKER $V3 'applyRoles()')"
say "freezeAuthority now: $(call $V3 'freezeAuthority()(address)'), watchers[0]: $(call $V3 'watchers(uint256)(address)' 0)"
say "setReleasesFrozen(true): $(send $NEWFA $V3 'setReleasesFrozen(bool)' true)"
say "setCheckpointsFrozen(true): $(send $NEWFA $V3 'setCheckpointsFrozen(bool)' true)"
say "retire(): $(send $NEWFA $V3 'retire()')"
say "watcher pauseCheckpoints(): $(send $WATCHER $V3 'pauseCheckpoints()')"
stub_verifier
say "forged release: $(send $ATTACKER $V3 "$RSIG" "$(release_req $ROOT $(cast keccak n1))" 0x6a756e6b)"
CP=$(forged_cp $NS $NR)
say "forged checkpoint: $(send $ATTACKER $V3 "submitCandidate($CSIG_T,bytes)" "$CP" 0x6a756e6b)"
say "deposit: $(VALUE=$DEP send $VICTIM $V3 'depositETH(uint8,bytes32,uint64)' 0 $COMMIT 1)"
say "releasesFrozenUntil: $(call $V3 'releasesFrozenUntil()(uint256)') (maxFreeze later; renewed by freezing again)"
warp $((30*86400 + 1))
say "after 30 days + 1 s without renewal, forged release: $(send $ATTACKER $V3 "$RSIG" "$(release_req $ROOT $(cast keccak n4))" 0x6a756e6b)"
say ""
say "# done"
