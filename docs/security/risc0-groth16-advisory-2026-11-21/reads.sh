#!/usr/bin/env bash
# Read-only state of the four Sepolia vaults, their emergency stops and the shared Groth16 verifier.
# eth_call / eth_getBalance only, at one pinned block, on one endpoint per run.
# Usage: reads.sh RPC_URL BLOCK OUTFILE
# A past BLOCK needs an endpoint that serves historical state (an archive node, e.g. https://rpc.sepolia.ethpandaops.io);
# publicnode refuses such reads. Exits 1, with "# N reads failed" as the last line of OUTFILE, if any read failed.
# Nothing is signed or sent: no `cast send`, no private key, no keystore.
set -u
RPC="$1"; B="$2"; OUT="$3"
: > "$OUT"

q() { # label addr sig [args...]
  local label="$1" addr="$2" sig="$3"; shift 3
  local v rc
  for attempt in 1 2 3; do
    v=$(cast call --block "$B" --rpc-url "$RPC" "$addr" "$sig" "$@" 2>&1); rc=$?
    [ $rc -eq 0 ] && break
    sleep 2
  done
  [ $rc -eq 0 ] || v="ERROR: $(echo "$v" | tr '\n' ' ' | cut -c1-200)"
  printf '%s\t%s\n' "$label" "$(echo "$v" | tr '\n' '|' | sed 's/|$//')" >> "$OUT"
  sleep 0.2
}
bal() { # label addr
  local v
  v=$(cast balance --block "$B" --rpc-url "$RPC" "$2" 2>&1) || v="ERROR: $v"
  printf '%s\t%s\n' "$1" "$v" >> "$OUT"; sleep 0.2
}

ETH=0x0000000000000000000000000000000000000000
G16=0x8f0Af3Bf302C64d57f840127F94d9c123a8Fa2A0
ADMIN=0x79A95aE9A3f9000eb41F7CBAB6a6F97826895C07

echo "# block=$B" >> "$OUT"

# Shared Groth16 verifier (risc0-ethereum v3.0.0, our deployment)
q g16.VERSION $G16 "VERSION()(string)"
q g16.SELECTOR $G16 "SELECTOR()(bytes4)"
q g16.CONTROL_ROOT_0 $G16 "CONTROL_ROOT_0()(bytes16)"
q g16.CONTROL_ROOT_1 $G16 "CONTROL_ROOT_1()(bytes16)"
q g16.BN254_CONTROL_ID $G16 "BN254_CONTROL_ID()(bytes32)"
bal admin.balance $ADMIN

# v5 vaults: name vault token stop(recorded)
v5() {
  local n="$1" V="$2" T="$3" S="$4"
  q $n.verifier $V "verifier()(address)"
  echo -e "$n.stop.recorded\t$S" >> "$OUT"
  q $n.stop.owner $S "owner()(address)"
  q $n.stop.paused $S "paused()(bool)"
  q $n.stop.verifier $S "verifier()(address)"
  q $n.mode $V "mode()(uint8)"
  q $n.checkpointImageId $V "checkpointImageId()(bytes32)"
  q $n.releaseImageId $V "releaseImageId()(bytes32)"
  q $n.admin $V "admin()(address)"
  q $n.pendingAdmin $V "pendingAdmin()(address)"
  q $n.freezeAuthority $V "freezeAuthority()(address)"
  q $n.watchers0 $V "watchers(uint256)(address)" 0
  q $n.watchers1 $V "watchers(uint256)(address)" 1
  q $n.watchers2 $V "watchers(uint256)(address)" 2
  q $n.roleProposal $V "roleProposal()(address,uint256)"
  q $n.feeRecipient $V "feeRecipient()(address)"
  q $n.retired $V "retired()(bool)"
  q $n.depositsFrozenUntil $V "depositsFrozenUntil()(uint256)"
  q $n.releasesFrozenUntil $V "releasesFrozenUntil()(uint256)"
  q $n.checkpointsFrozenUntil $V "checkpointsFrozenUntil()(uint256)"
  q $n.tokenDepositsFrozenUntil.ETH $V "tokenDepositsFrozenUntil(address)(uint256)" $ETH
  q $n.tokenReleasesFrozenUntil.ETH $V "tokenReleasesFrozenUntil(address)(uint256)" $ETH
  q $n.tokenDepositsFrozenUntil.token $V "tokenDepositsFrozenUntil(address)(uint256)" $T
  q $n.tokenReleasesFrozenUntil.token $V "tokenReleasesFrozenUntil(address)(uint256)" $T
  q $n.globalCap.deposit $V "globalCap(uint8)(uint256,uint256,uint256)" 0
  q $n.globalCap.release $V "globalCap(uint8)(uint256,uint256,uint256)" 1
  q $n.tokenCap.ETH.deposit $V "tokenCap(address,uint8)(uint256,uint256,uint256)" $ETH 0
  q $n.tokenCap.ETH.release $V "tokenCap(address,uint8)(uint256,uint256,uint256)" $ETH 1
  q $n.tokenCap.token.deposit $V "tokenCap(address,uint8)(uint256,uint256,uint256)" $T 0
  q $n.tokenCap.token.release $V "tokenCap(address,uint8)(uint256,uint256,uint256)" $T 1
  q $n.releaseCapFloor $V "releaseCapFloor()(uint256)"
  q $n.releaseRefillFloor $V "releaseRefillFloor()(uint256)"
  q $n.tokenConfig.ETH $V "tokenConfig(address)(bool,bool,uint256,uint256,uint256[])" $ETH
  q $n.tokenConfig.token $V "tokenConfig(address)(bool,bool,uint256,uint256,uint256[])" $T
  q $n.escrowed.ETH $V "escrowed(address)(uint256)" $ETH
  q $n.escrowed.token $V "escrowed(address)(uint256)" $T
  q $n.totalOwed.ETH $V "totalOwed(address)(uint256)" $ETH
  q $n.totalOwed.token $V "totalOwed(address)(uint256)" $T
  bal $n.balance.ETH $V
  q $n.balance.token $T "balanceOf(address)(uint256)" $V
  q $n.delta $V "delta()(uint256)"
  q $n.deltaFloor $V "deltaFloor()(uint256)"
  q $n.revokeWindow $V "revokeWindow()(uint256)"
  q $n.maxFreeze $V "maxFreeze()(uint256)"
  q $n.minReleaseDelay $V "minReleaseDelay()(uint256)"
  q $n.keylessFrom $V "keylessFrom()(uint64)"
  q $n.retireOpensAt $V "retireOpensAt()(uint256)"
  q $n.sunsetLeafCountSet $V "sunsetLeafCountSet()(bool)"
  q $n.finalSunset $V "finalSunset()(bool)"
  q $n.checkpointCount $V "checkpointCount()(uint256)"
  q $n.nextLeafIndex $V "nextLeafIndex()(uint64)"
  q $n.finalState $V "finalState()(bytes32)"
  q $n.revocationEpoch $V "revocationEpoch()(uint256)"
  q $n.releaseDelayNext.ETH $V "releaseDelayFor(address,uint256)(uint256)" $ETH 1000000
  q $n.releaseDelayNext.token $V "releaseDelayFor(address,uint256)(uint256)" $T 1000000
}

v5 live3 0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c 0x6b479b044501C8764a326EaBb64b5f441F96C3CB 0x1dB3fa8041E8178f1608c2F7dF627d7EC16C8221
v5 live1 0xfE3199fa2634413CbAC317371076Cae3EB2d9E54 0x8E1140D771892abdbe6b0Ef03aDd081F2153B73b 0x2Ea6457503024bB8dD99764F1bB5C7aFf9726Ea9
v5 live4 0x80555Fe53CCcEf46951B23A3c75643a4C9283470 0xD599644B3515C2f86EC5414a1aBE5Fd0d25bAb43 0xB24905b767c1b50dDe6b1B33aDb119CA7E2dEfCA

# v3 vault (source c6aee464)
V=0xd0B5fcC0B47D8E6f514f9D246Bb0b416b2048AF7; T=0xdfedf9A73Cfb337277b95a9a1C941c3496A4385F; S=0xe4830261A1D61Df5AF1be5579a3e3DB738331139; n=v3
q $n.verifier $V "verifier()(address)"
echo -e "$n.stop.recorded\t$S" >> "$OUT"
q $n.stop.owner $S "owner()(address)"
q $n.stop.paused $S "paused()(bool)"
q $n.stop.verifier $S "verifier()(address)"
q $n.checkpointImageId $V "checkpointImageId()(bytes32)"
q $n.releaseImageId $V "releaseImageId()(bytes32)"
q $n.admin $V "admin()(address)"
q $n.pendingAdmin $V "pendingAdmin()(address)"
q $n.freezeAuthority $V "freezeAuthority()(address)"
q $n.pendingFreezeAuthority $V "pendingFreezeAuthority()(address)"
q $n.retired $V "retired()(bool)"
q $n.depositsFrozenUntil $V "depositsFrozenUntil()(uint256)"
q $n.releasesFrozenUntil $V "releasesFrozenUntil()(uint256)"
q $n.checkpointsFrozenUntil $V "checkpointsFrozenUntil()(uint256)"
q $n.globalCap.deposit $V "globalCap(uint8)(uint256,uint256,uint256)" 0
q $n.globalCap.release $V "globalCap(uint8)(uint256,uint256,uint256)" 1
q $n.tokenCap.ETH.release $V "tokenCap(address,uint8)(uint256,uint256,uint256)" $ETH 1
q $n.tokenCap.token.release $V "tokenCap(address,uint8)(uint256,uint256,uint256)" $T 1
q $n.tokenConfig.ETH $V "tokenConfig(address)(bool,bool,uint256,uint256,uint256[])" $ETH
q $n.tokenConfig.token $V "tokenConfig(address)(bool,bool,uint256,uint256,uint256[])" $T
q $n.escrowed.ETH $V "escrowed(address)(uint256)" $ETH
q $n.escrowed.token $V "escrowed(address)(uint256)" $T
q $n.totalOwed.ETH $V "totalOwed(address)(uint256)" $ETH
bal $n.balance.ETH $V
q $n.balance.token $T "balanceOf(address)(uint256)" $V
q $n.delta $V "delta()(uint256)"
q $n.revokeWindow $V "revokeWindow()(uint256)"
q $n.maxFreeze $V "maxFreeze()(uint256)"
q $n.checkpointCount $V "checkpointCount()(uint256)"
q $n.nextLeafIndex $V "nextLeafIndex()(uint64)"
q $n.releaseDelay.ETH $V "releaseDelay(address)(uint256)" $ETH
FAILED=$(grep -c $'\tERROR' "$OUT")
if [ "$FAILED" -gt 0 ]; then
  echo "# $FAILED reads failed" >> "$OUT"
  echo "reads.sh: $FAILED reads failed (the ERROR lines of $OUT); a past block needs an endpoint that serves historical state" >&2
  exit 1
fi
echo "# done" >> "$OUT"
