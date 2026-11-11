#!/usr/bin/env bash
# Rehearsal of the crown run's LEZ side on the build box (STAND-IN, labelled; nothing is sent to any public network:
# the Sepolia node is only read, by anvil's fork).
#
#   rehearse.sh a      a fresh "Volume" directory with the bundle; session.sh a (CROWN_CHAIN=stand-in)
#   rehearse.sh fork   anvil fork of Sepolia: the real v5 vault in mode (B) with session A's configId and genesisState
#                      (e2e/crown/sepolia.sh deploy, FERRATA_UNLOCKED), deposits j0 (public) and j1 (private) with a
#                      capture of the fork's block after each (capture-anvil.py); then crown-driver check
#   rehearse.sh b      session.sh b: the generation of A restored, the bridge flow over the two captures
#   rehearse.sh all    a, fork, b
#
# The beacon side is the stand-in's: a committee whose keys the driver holds finalizes the fork's blocks (the vault,
# its deposits, its storage proofs and the block headers are the fork's own). Records: e2e/runs/crown-lez-rehearsal/
# (Sepolia side), $T/vol/session-a, session-b (LEZ side; T = tmp/crown-lez/rehearsal beside the repository).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
BESIDE="$(cd "$REPO/.." && pwd)"
T="${CROWN_REHEARSAL:-$BESIDE/tmp/crown-lez/rehearsal}"
VOL="$T/vol"
export CROWN_LEZ_VOL="$VOL" CROWN_LEZ_WORK="$T/work" CROWN_CHAIN=stand-in
RUN="$REPO/e2e/runs/crown-lez-rehearsal"
UP="${CROWN_SEPOLIA_READ_RPC:-https://ethereum-sepolia-rpc.publicnode.com}"
DEPLOYER=0x79A95aE9A3f9000eb41F7CBAB6a6F97826895C07
VERIFIER=0x8f0Af3Bf302C64d57f840127F94d9c123a8Fa2A0   # RISC Zero's Groth16 verifier of sepolia-live-2 (reused)
IDS="$BESIDE/tmp/crown/build/ids-methods-hex.txt"
DRV="$BESIDE/tmp/crown-lez/bin/crown-driver"
say() { echo "[$(date -u +%H:%M:%S)] $*"; }

step_a() {
  rm -rf "$VOL" "$T/work"; mkdir -p "$VOL"
  cp "$BESIDE/tmp/crown-lez/bundle.tar.zst" "$VOL/"
  bash "$HERE/session.sh" a
  jq -c '{tip, channel: .channel.keys, bedrockGenesisId, instance: .instance.id, bridge: .bridge}' "$VOL/session-a/session-a.json"
}

step_fork() {
  export PATH="$HOME/.foundry/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
  local A id="$VOL/session-a/session-a.json"
  [ -f "$id" ] || { say "no session-a.json: run step a first"; exit 1; }
  rm -rf "$RUN" "$VOL/inputs"; mkdir -p "$RUN" "$VOL/inputs"
  local head; head="$(cast block-number --rpc-url "$UP")"
  local base=$((head - 5))
  anvil --fork-url "$UP" --fork-block-number "$base" --auto-impersonate --port 8545 > "$RUN/anvil.log" 2>&1 & A=$!
  trap 'kill '"$A"' 2>/dev/null || true' EXIT
  for _ in $(seq 90); do cast chain-id --rpc-url http://127.0.0.1:8545 > /dev/null 2>&1 && break; sleep 1; done
  export SEPOLIA_RPC_URL=http://127.0.0.1:8545 ETH_FROM="$DEPLOYER" FERRATA_UNLOCKED=1
  local R="$SEPOLIA_RPC_URL" N VAULT CID GS Z=0x0000000000000000000000000000000000000000000000000000000000000000
  N="$(cast nonce "$ETH_FROM" --rpc-url "$R")"
  VAULT="$(cast compute-address "$ETH_FROM" --nonce $((N + 2)) | awk '{print $NF}')"
  say "fork of Sepolia block $base; deployer nonce $N; vault will be $VAULT"
  CID="$("$DRV" config-id --identity "$id" --vault "$VAULT" | jq -r .configId)"
  GS="$(python3 -I "$REPO/e2e/crown/identity.py" genesis-state --tip-id "$(jq -r .tip.id "$id")" --tip-hash "$(jq -r .tip.hash "$id")" \
    --keys "$(jq -r '.channel.keys | join(",")' "$id")" --posting-timeframe "$(jq -r .channel.postingTimeframe "$id")" \
    --config-tip-hash "$(jq -r .channel.configTipHash "$id")")"
  say "configId $CID genesisState $GS"
  jq -n --arg v "$VAULT" --arg c "$CID" --arg g "$GS" --arg ver "$VERIFIER" '{label: "STAND-IN (crown-lez rehearsal: anvil fork, stand-in beacon committee)",
    chainId: 11155111, vault: $v, configId: $c, genesisState: $g, groth16Verifier: $ver, delta: 3600}' > "$RUN/identity.json"
  id_of() { awk -v k="$1" '$1 == k {print $2}' "$IDS"; }
  jq -n --arg c "$CID" --arg g "$GS" --arg a "$(id_of CHECKPOINT_ID)" --arg b "$(id_of CHECKPOINT_B_ID)" --arg r "$(id_of RELEASE_ID)" \
    --arg z "$Z" '{_source: "crown-lez rehearsal (stand-in identity from a real devnet session A)", checkpointImageIdA: $a,
    checkpointImageIdB: $b, releaseImageId: $r, lezConfigId: $c, genesisState: $g, nMin: 1, tMin: 1, postingTimeframe: 0,
    ethLezDefinition: $z, tokenLezDefinition: $z}' > "$RUN/params.json"
  local d0 d1 C0 C1
  d0="$("$DRV" destinations | jq -r .public0)"; d1="$("$DRV" destinations | jq -r .private0)"
  C0="$(python3 -I "$REPO/e2e/crown/identity.py" commitment --chain-id 11155111 --vault "$VAULT" --depositor "$ETH_FROM" --j 0 --kind public --dest "$d0")"
  C1="$(python3 -I "$REPO/e2e/crown/identity.py" commitment --chain-id 11155111 --vault "$VAULT" --depositor "$ETH_FROM" --j 1 --kind private --dest "$d1")"
  jq -n --arg v "$VAULT" --arg c0 "$C0" --arg c1 "$C1" '{chainId: 11155111, vault: $v,
    deposits: [{j: 0, denomIndex: 0, commitment: $c0}, {j: 1, denomIndex: 0, commitment: $c1}]}' > "$RUN/crown.json"
  local S="$REPO/e2e/crown/sepolia.sh" slots
  slots="$("$DRV" capture-slots | jq -r '.slots[].slot')"
  local sargs=(); for s in $slots; do sargs+=(--slot "$s"); done
  "$S" "$RUN" deploy
  "$S" "$RUN" deposit 0
  python3 -I "$HERE/capture-anvil.py" --rpc "$R" --vault "$VAULT" --from-block "$base" "${sargs[@]}" "$VOL/inputs/capture-1"
  "$S" "$RUN" deposit 1
  python3 -I "$HERE/capture-anvil.py" --rpc "$R" --vault "$VAULT" --from-block "$base" "${sargs[@]}" "$VOL/inputs/capture-2"
  "$S" "$RUN" status
  kill "$A" 2>/dev/null || true; trap - EXIT
  local B="$T/check-bundle"; rm -rf "$B"; mkdir -p "$B"; zstd -q -d -c "$VOL/bundle.tar.zst" | tar -C "$B" -xf -
  "$DRV" check --chain stand-in --program-bin "$B/program/ferrata_lez.bin" --bridge-bin "$B/program/ferrata_bridge.bin" \
    --capture "$VOL/inputs/capture-1" --capture "$VOL/inputs/capture-2" --identity "$id"
}

step_b() {
  bash "$HERE/session.sh" b
  jq -c '{tipT0, tipT1, blocks, configId, steps: [.steps[] | [.step, .status, .block]]}' "$VOL/session-b/session-b.json"
  jq -c '.[] | {i, kind, noteHash, nOut, block}' "$VOL/session-b/burn-notes.json"
}

case "${1:-all}" in
  a) step_a ;;
  fork) step_fork ;;
  b) step_b ;;
  all) step_a; step_fork; step_b ;;
  *) sed -n '2,15p' "${BASH_SOURCE[0]}"; exit 2 ;;
esac
