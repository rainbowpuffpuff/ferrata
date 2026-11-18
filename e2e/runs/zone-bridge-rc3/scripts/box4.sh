#!/usr/bin/env bash
# rc3-return box job 4 (stage 3, the vault), run from the worktree root by mrun after the Groth16 step (mwrap, laptop
# side, proof.json put back with --put): zone-bridge.sh's `groth16` for the checkpoint and each release, `vault` (the
# run's anvil from its saved state: submitCandidate, W warped, finalizeAttested, the release delay warped, both
# releases), the refusals on that anvil (a release replayed; the checkpoint's seal under the deployed checkpoint_b's id
# at RISC Zero's verifier), `fixture` (zone-bridge-rc3.json) and ZoneBridgeRc3Test, `verify`.
set -uo pipefail
W="$PWD"; SP="$(cd "$W/.." && pwd)"; S="$SP/tmp/rc3-return"; B="$S/box4"; P="$S/programs-rc3"
NAME="${RL_NAME:-zone-bridge-rc3}"; RUN="$W/e2e/runs/$NAME"
export ZB_TMP="$S/zb" ZB_PROGRAMS="$P" ZB_CHECKPOINT_B=checkpoint_b_0_3_5 ZB_N_MIN=1 ZB_T_MIN=1 ZB_LEAVE_OUT=none ZB_FIXTURE=zone-bridge-rc3
export CARGO_TERM_COLOR=never PATH="$HOME/.foundry/bin:$HOME/.cargo/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1; unset RISC0_DEV_MODE
mkdir -p "$B" "$ZB_TMP/work/$NAME/logs"; : > "$B/status.txt"   # zone-bridge.sh vault logs anvil there (its zone stage made it)
step() { local n="$1"; shift; local t0; t0=$(date +%s); ( "$@" ) > "$B/$n.log" 2>&1; local rc=$?
  echo "$n exit $rc $(( $(date +%s) - t0 )) s $(date -u +%T)" | tee -a "$B/status.txt"; return $rc; }
ZB="checkpoint/attestor/zone-bridge.sh"
g16() {
  bash "$ZB" groth16 "$RUN" checkpoint || return 1
  local p; for p in $(jq -r '.[].name' "$RUN/releases/releases.json"); do bash "$ZB" groth16 "$RUN" "release-$p" || return 1; done
}
refusals() { # on the vault's anvil after the releases (zone-bridge.sh vault dumps it to anvil-state-vault.json)
  local R=http://127.0.0.1:8545 WK="$ZB_TMP/work/$NAME" A VAULT ESTOP SEAL J OLD NEW rc
  [ -s "$WK/anvil-state-vault.json" ] || { echo "no anvil-state-vault.json"; return 1; }
  anvil --chain-id 11155111 --port 8545 --gas-limit 100000000 --auto-impersonate --load-state "$WK/anvil-state-vault.json" > "$B/anvil-refusals.log" 2>&1 & A=$!
  for _ in $(seq 60); do cast chain-id --rpc-url $R > /dev/null 2>&1 && break; sleep 1; done
  VAULT="$(jq -r .vault "$RUN/identity.json")"; ESTOP="$(jq -r .emergencyStop "$RUN/deploy.json")"
  SEAL="0x$(tr -d ' \n' < "$RUN/checkpoint/receipts/seal.hex" | sed 's/^0x//')"
  J="0x$(python3 -I -c 'import hashlib,sys; print(hashlib.sha256(bytes.fromhex(open(sys.argv[1]).read().strip().removeprefix("0x"))).hexdigest())' "$RUN/checkpoint/receipts/journal.hex")"
  NEW="$(jq -r 'select(.program=="checkpoint_b_0_3_5.bin") | .image_id' "$P/ids.jsonl")"
  OLD="$(jq -r 'select(.program=="checkpoint_b.bin") | .image_id' "$P/ids.jsonl")"
  echo "vault $VAULT: checkpointImageId() = $(cast call "$VAULT" 'checkpointImageId()(bytes32)' --rpc-url $R) (checkpoint_b_0_3_5 $NEW; the deployed checkpoint_b $OLD)"
  echo "RISC Zero's verifier (the vault's emergency stop $ESTOP), this checkpoint's seal and journal digest $J:"
  if cast call "$ESTOP" 'verify(bytes,bytes32,bytes32)' "$SEAL" "$NEW" "$J" --rpc-url $R > "$B/verify-new.txt" 2>&1; then echo "  under checkpoint_b_0_3_5's id: verifies"; else echo "  under checkpoint_b_0_3_5's id: REFUSED (unexpected): $(tail -n 1 "$B/verify-new.txt")"; rc=1; fi
  if cast call "$ESTOP" 'verify(bytes,bytes32,bytes32)' "$SEAL" "$OLD" "$J" --rpc-url $R > "$B/verify-old.txt" 2>&1; then echo "  under the deployed checkpoint_b's id: ACCEPTED (unexpected)"; rc=1; else echo "  under the deployed checkpoint_b's id: refused: $(grep -o 'execution reverted[^"]*' "$B/verify-old.txt" | head -n 1)"; fi
  export SEPOLIA_RPC_URL=$R ETH_FROM="$(jq -r .deployer "$RUN/identity.json")" FERRATA_UNLOCKED=1 FERRATA_NONCE_GAP=0
  # a release replayed: the vault itself, called with release 0's request and seal again (sepolia.sh's Crown script
  # would not send it: it skips a paid release), simulated with eth_call from the anvil's account
  local q; q="$(jq -r '.releases[0] | "(\(.token),\(.denomIndex),\(.recipient),\(.relayer),\(.relayFee),\(.burnRoot),\(.nullifier))"' "$RUN/crown.json")"
  if cast call "$VAULT" 'release((address,uint8,address,address,uint256,bytes32,bytes32),bytes)' "$q" "$(jq -r '.releases[0].seal' "$RUN/crown.json")" \
       --from "$ETH_FROM" --rpc-url $R > "$B/replay.txt" 2>&1; then echo "release 0 replayed: ACCEPTED (unexpected)"; rc=1; else
    echo "release 0 replayed: refused: $(grep -o 'custom error 0x[0-9a-f]*' "$B/replay.txt" | head -n 1) = NullifierAlreadyUsed() $(cast sig 'NullifierAlreadyUsed()')"; fi
  echo "nullifierUsed(release 0) = $(cast call "$VAULT" 'nullifierUsed(bytes32)(bool)' "$(jq -r '.releases[0].nullifier' "$RUN/crown.json")" --rpc-url $R)"
  kill -INT $A 2>/dev/null; wait $A 2>/dev/null
  return "${rc:-0}"
}
forgetest() { cd "$W/vault" && forge test --match-contract ZoneBridgeRc3Test -vv 2>&1 | grep -v '^Compiling\|^Solc\|^Compiler run'; return "${PIPESTATUS[0]}"; }
ONLY="${ONLY:-groth16 vault refusals fixture forgetest verify}"
for n in $ONLY; do case $n in groth16) step groth16 g16 ;; vault) step vault bash "$ZB" vault "$RUN" ;; refusals) step refusals refusals ;; fixture) step fixture bash "$ZB" fixture "$RUN" ;; forgetest) step forgetest forgetest ;; verify) step verify bash "$ZB" verify "$RUN" ;; esac; done
for n in $ONLY; do sed -e "s#$SP/#spike/#g" -e "s#$HOME/#~/#g" "$B/$n.log" > "$RUN/logs/box-$n.log"; done
cp "$B/status.txt" "$RUN/logs/box4-status.txt"
grep -rIl -e "$SP/" -e "$HOME/" -e "/root/" "$RUN" "$W/vault/test/real/fixtures/zone-bridge-rc3.json" 2>/dev/null && echo "MACHINE PATHS FOUND (listed above)" | tee -a "$B/status.txt"
echo "ALL DONE $(date -u +%T)" | tee -a "$B/status.txt"
