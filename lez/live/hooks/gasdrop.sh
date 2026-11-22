#!/usr/bin/env bash
# The gas drop (M0 check 2) on a running zone: the hook of ../run.sh's phase `hook`
# (FERRATA_LIVE_HOOK=lez/live/hooks/gasdrop.sh), run while the zone runs, with the zone in ZONE_* variables.
# On the build box only; nothing is sent to any public network.
#
# The bridge as hooks/crown.sh sets it up (crown-a, a local anvil with the vault tests' MockRiscZeroVerifier and
# FerrataVault v5 in mode (B), crown-b with --operator-only yes: the updates, Init, Register ETH, ImportFees and
# ImportRoot n=1, n=2), with deposit j1 made to a claimant who has nothing on LEZ. Then (lez/crown/driver
# src/gasdrop.rs; cli/src/bin/gasdrop.rs):
#   gas-drop-keys     the claimant's new wallet: a private account (deposit j1's destination), a private one-time
#                     account, a public account;
#   gas-drop-prepare  the balances before; the negative control (the same ClaimPrivately with no credit: the
#                     wallet's dry run and its refusal); the gas-drop guest's input, the imported roots, the claimed N;
#   ferrata-gasdrop prove   the claimant's gas-drop receipt, proven for real;
#   ferrata-gasdrop relay   the relayer's drop (ferrata_cli::relayer::gas_drop: the receipt checked, one drop per N),
#                     whose credit command is crown-driver gas-drop-credit: one native credit from the relayer's
#                     private account, sealed to the one-time account, proven for real; the relayer's refusals of a
#                     second drop and of a receipt bound to another relayer;
#   gas-drop-claim    the claim that sweeps the sealed credit into the one-time account and pays its fee from it,
#                     proven for real (the sequencer verifies it), the relay fee paid to the relayer in wrapped ETH;
#                     the balances after; then the relayer refuses the receipt as claimed;
#   export            the records.
# A gas-drop step that fails waits (GASDROP_RETRY_WAIT seconds, default 2700) for $ZONE_RUN_DIR/gasdrop/RETRY (run it
# again, e.g. after crown-driver was rebuilt at the same path) or ABORT, the zone still running.
# Records into $ZONE_OUT/hook/ (machine paths removed; the claimant's wallet, which holds its mnemonic, and the
# gas-drop guest's input, which holds the deposit's secret, are not copied).
#
# Environment (from run-live.sh): ZONE_RPC, ZONE_BEDROCK, ZONE_RUN_DIR, ZONE_LOG, ZONE_BIN, ZONE_OUT, ZONE_WAIT_FINAL,
# ZONE_PROGRAM_BIN, ZONE_SEQUENCER_CONFIG, ZONE_DEPLOYMENT. Ours: CROWN_BRIDGE_BIN (the bridge program, required),
# GASDROP_ELF (the gas-drop guest, required), GASDROP_TOOL (ferrata-gasdrop, required), CROWN_DRIVER (default
# $ZONE_BIN/crown-driver), CROWN_FIXTURE, CROWN_PROOF_THREADS (default 32), GASDROP_AMOUNT (the drop, default
# 20000000), CROWN_RUN_NAME (default live-gasdrop-hook). Needs anvil, cast, forge (vault built), jq, python3, r0vm 3.0.5.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../.." && pwd)"
BESIDE="$(cd "$REPO/.." && pwd)"
: "${ZONE_RPC:?run from run-live.sh phase hook}" "${CROWN_BRIDGE_BIN:?the bridge program binary}"
: "${GASDROP_ELF:?the gas-drop guest binary}" "${GASDROP_TOOL:?ferrata-gasdrop}"
DRV="${CROWN_DRIVER:-$ZONE_BIN/crown-driver}"
FIXTURE="${CROWN_FIXTURE:-$REPO/crates/ferrata-spectests/fixtures/sepolia-2026-11-12}"
THREADS="${CROWN_PROOF_THREADS:-32}"
AMOUNT="${GASDROP_AMOUNT:-20000000}"
GASDROP_ID=0x895804161df14b00e1cb02e5d27fe136efb82b178db511c7697c030676fc9eb4
H="$ZONE_RUN_DIR/crown"; G="$ZONE_RUN_DIR/gasdrop"; REC="$ZONE_OUT/hook"
RUN="$REPO/e2e/runs/${CROWN_RUN_NAME:-live-gasdrop-hook}"
RPC=http://127.0.0.1:8545
DEPLOYER=0x000000000000000000000000000000005E9DE901
CHECKPOINT_A_ID=0xb9c1e6a834879f6be79d09d728523565846018629b82f66986eb58fab4f52c84
CHECKPOINT_B_ID=0xf8116722ee692bcff038eed47d95f0de4b267704e45b1d11711ad84e860e626e
RELEASE_ID=0xb64900bcf741668568b3e14ef902987e305b303873ff7258c109409969eb29e9
Z=0x0000000000000000000000000000000000000000000000000000000000000000
export PATH="$HOME/.foundry/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
say() { echo "[gasdrop-hook $(date -u +%H:%M:%S)] $*"; }
die() { say "STOP: $*"; exit 1; }
rm -rf "$H" "$G" "$RUN"; mkdir -p "$H" "$G" "$RUN" "$REC"
[ -x "$DRV" ] || die "no crown-driver at $DRV"
[ -x "$GASDROP_TOOL" ] || die "no ferrata-gasdrop at $GASDROP_TOOL"
[ -d "$REPO/vault/out" ] || die "the vault is not built (vault/out)"
[ "$(r0vm --elf "$GASDROP_ELF" --id | tail -n 1)" = "${GASDROP_ID#0x}" ] || die "$GASDROP_ELF is not the gas-drop guest of docs/image-ids.md"

# A gas-drop step: on failure, wait for RETRY (run it again) or ABORT while the zone keeps running.
step() { # name command...
  local name="$1"; shift
  while true; do
    say "$name"
    "$@" > "$G/$name.log" 2>&1 && return 0
    local rc=$?
    tail -n 25 "$G/$name.log"
    say "$name failed (exit $rc): waiting for $G/RETRY or $G/ABORT"
    echo "$name exit $rc $(date -u +%T)" > "$G/FAILED"
    rm -f "$G/RETRY"
    local waited=0
    while [ ! -f "$G/RETRY" ] && [ ! -f "$G/ABORT" ] && [ "$waited" -lt "${GASDROP_RETRY_WAIT:-2700}" ]; do sleep 10; waited=$((waited + 10)); done
    if [ -f "$G/RETRY" ]; then rm -f "$G/RETRY" "$G/FAILED"; say "$name: again"; continue; fi
    return "$rc"
  done
}

# ---- LEZ session A on this zone (as hooks/crown.sh) ---------------------------------------------------------
CHANNEL="$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["bedrock_config"]["channel_id"])' "$ZONE_SEQUENCER_CONFIG" 2>/dev/null \
  || echo 0101010101010101010101010101010101010101010101010101010101010101)"
for _ in $(seq 1 120); do
  curl -fsS -m 5 -o "$H/channel.json" "$ZONE_BEDROCK/channel/$CHANNEL" 2>/dev/null && grep -q '"accredited_keys":\["' "$H/channel.json" && break
  sleep 5
done
grep -q '"accredited_keys":\["' "$H/channel.json" 2>/dev/null || die "the node does not serve the channel's keys"
D=(--rpc "$ZONE_RPC" --run-dir "$H/driver" --program-bin "$ZONE_PROGRAM_BIN" --bridge-bin "$CROWN_BRIDGE_BIN" --chain stand-in
   --fixture "$FIXTURE" --sequencer-log "$ZONE_LOG/sequencer.log" --wait-final "$ZONE_WAIT_FINAL")
say "crown-a: deploy attest and bridge, the stand-in instance (channel $CHANNEL)"
env -u RISC0_DEV_MODE "$DRV" crown-a "${D[@]}" --channel "$H/channel.json" --genesis "$ZONE_DEPLOYMENT" \
  --identity-out "$H/session-a.json" > "$H/crown-a.log" 2>&1 || { tail -n 30 "$H/crown-a.log"; die "crown-a"; }
SA="$H/session-a.json"
say "session A: tip $(jq -c .tip "$SA"); attestation $(jq -r .attestation.imageId "$SA"); bridge $(jq -r .bridge.imageId "$SA")"

# ---- the claimant: a new wallet, nothing on LEZ ------------------------------------------------------------
GD=(--gasdrop-dir "$G")
step keys env -u RISC0_DEV_MODE "$DRV" gas-drop-keys "${D[@]}" "${GD[@]}" --keys "$G/keys.json" || die "gas-drop-keys"
say "claimant: $(jq -c '{main: .main.id, oneTime: .oneTime.id, public: .public.id, relayer: .relayer.id}' "$G/keys.json")"

# ---- Ethereum: anvil, the mock verifier, the vault, deposits j0 (public) and j1 (to the claimant) -----------
anvil --chain-id 11155111 --port 8545 --gas-limit 100000000 --auto-impersonate > "$H/anvil.log" 2>&1 & ANVIL=$!
trap 'kill $ANVIL 2>/dev/null || true' EXIT
for _ in $(seq 60); do cast chain-id --rpc-url $RPC > /dev/null 2>&1 && break; sleep 1; done
cast rpc anvil_setBalance $DEPLOYER 0x56bc75e2d63100000 --rpc-url $RPC > /dev/null   # 100 ETH
MOCK="$(cd "$REPO/vault" && forge create test/mocks/MockVerifiers.sol:MockRiscZeroVerifier --rpc-url $RPC --unlocked \
  --from $DEPLOYER --broadcast --json 2>/dev/null | jq -r .deployedTo)"
[ -n "$MOCK" ] && [ "$MOCK" != null ] || die "the mock verifier was not deployed"
N="$(cast nonce $DEPLOYER --rpc-url $RPC)"
VAULT="$(cast compute-address $DEPLOYER --nonce $((N + 2)) | awk '{print $NF}')"
CID="$("$DRV" config-id --identity "$SA" --vault "$VAULT" | jq -r .configId)"
GS="$(python3 -I "$REPO/e2e/crown/identity.py" genesis-state --tip-id "$(jq -r .tip.id "$SA")" --tip-hash "$(jq -r .tip.hash "$SA")" \
  --keys "$(jq -r '.channel.keys | join(",")' "$SA")" --posting-timeframe "$(jq -r .channel.postingTimeframe "$SA")" \
  --config-tip-hash "$(jq -r .channel.configTipHash "$SA")")"
FROM="$(cast block-number --rpc-url $RPC)"
jq -n --arg v "$VAULT" --arg c "$CID" --arg g "$GS" --arg m "$MOCK" '{label: "live gas-drop hook: local anvil (chain id 11155111, the stand-in committee finalizes its blocks), the vault tests MockRiscZeroVerifier",
  chainId: 11155111, vault: $v, configId: $c, genesisState: $g, groth16Verifier: $m, delta: 3600}' > "$RUN/identity.json"
jq -n --arg c "$CID" --arg g "$GS" --arg a "$CHECKPOINT_A_ID" --arg b "$CHECKPOINT_B_ID" --arg r "$RELEASE_ID" --arg z "$Z" \
  '{_source: "live gas-drop hook (the final programs of docs/image-ids.md; the identity of this zone session A)",
    checkpointImageIdA: $a, checkpointImageIdB: $b, releaseImageId: $r, lezConfigId: $c, genesisState: $g, nMin: 1, tMin: 1,
    postingTimeframe: 0, ethLezDefinition: $z, tokenLezDefinition: $z}' > "$RUN/params.json"
d0="$("$DRV" destinations | jq -r .public0)"; d1="$(jq -r .main.hex "$G/keys.json")"
C0="$(python3 -I "$REPO/e2e/crown/identity.py" commitment --chain-id 11155111 --vault "$VAULT" --depositor "$DEPLOYER" --j 0 --kind public --dest "$d0")"
C1="$(python3 -I "$REPO/e2e/crown/identity.py" commitment --chain-id 11155111 --vault "$VAULT" --depositor "$DEPLOYER" --j 1 --kind private --dest "$d1")"
jq -n --arg v "$VAULT" --arg c0 "$C0" --arg c1 "$C1" '{chainId: 11155111, vault: $v,
  deposits: [{j: 0, denomIndex: 0, commitment: $c0}, {j: 1, denomIndex: 0, commitment: $c1}]}' > "$RUN/crown.json"
say "vault $VAULT (mock verifier $MOCK), configId $CID; deposit j1 to the claimant's private account $d1"
export SEPOLIA_RPC_URL=$RPC ETH_FROM=$DEPLOYER FERRATA_UNLOCKED=1 FERRATA_NONCE_GAP=0
bash "$REPO/e2e/crown/sepolia.sh" "$RUN" deploy > "$H/eth-deploy.log" 2>&1 || { tail -n 30 "$H/eth-deploy.log"; die "vault deployment"; }
rm -f "$REPO/vault/deployments/crown-$(basename "$RUN").json"   # the same record is $RUN/deploy.json
slots="$("$DRV" capture-slots | jq -r '.slots[].slot')"
sargs=(); for s in $slots; do sargs+=(--slot "$s"); done
for j in 0 1; do
  bash "$REPO/e2e/crown/sepolia.sh" "$RUN" deposit "$j" > "$H/eth-deposit-$j.log" 2>&1 || { tail -n 30 "$H/eth-deposit-$j.log"; die "deposit $j"; }
  python3 -I "$REPO/lez/crown/capture-anvil.py" --rpc $RPC --vault "$VAULT" --from-block "$FROM" "${sargs[@]}" "$H/capture-$((j + 1))" \
    > "$H/capture-$((j + 1)).log" 2>&1 || { tail -n 20 "$H/capture-$((j + 1)).log"; die "capture $((j + 1))"; }
  say "deposit $j sent; capture-$((j + 1)): $(tail -n 1 "$H/capture-$((j + 1)).log")"
done
bash "$REPO/e2e/crown/sepolia.sh" "$RUN" status > "$H/eth-status.log" 2>&1 || true
kill $ANVIL 2>/dev/null || true; trap - EXIT
CAP=(--capture "$H/capture-1" --capture "$H/capture-2")
"$DRV" check --chain stand-in --program-bin "$ZONE_PROGRAM_BIN" --bridge-bin "$CROWN_BRIDGE_BIN" "${CAP[@]}" --identity "$SA" \
  --operator-only yes > "$H/check.log" 2>&1 || { tail -n 20 "$H/check.log"; die "crown-driver check"; }
say "crown-driver check: $(tail -n 1 "$H/check.log")"

# ---- LEZ session B: the operator's steps only ---------------------------------------------------------------
say "crown-b --operator-only yes: the updates, Init, Register ETH, ImportFees, ImportRoot n=1, n=2"
env -u RISC0_DEV_MODE "$DRV" crown-b "${D[@]}" --identity "$SA" "${CAP[@]}" --operator-only yes \
  --session-out "$H/session-b.json" --notes-out "$H/burn-notes.json" > "$H/crown-b.log" 2>&1 || { tail -n 30 "$H/crown-b.log"; die "crown-b"; }
tail -n 4 "$H/crown-b.log"

# ---- the gas drop ---------------------------------------------------------------------------------------------
PROVE=(env -u RISC0_DEV_MODE RAYON_NUM_THREADS="$THREADS" RISC0_SERVER_PATH="$(command -v r0vm)" RUST_LOG="${GASDROP_RUST_LOG:-warn,wallet=info}")
step prepare "${PROVE[@]}" "$DRV" gas-drop-prepare "${D[@]}" "${GD[@]}" --keys "$G/keys.json" --identity "$SA" "${CAP[@]}" \
  || die "gas-drop-prepare"
say "negative control: $(jq -c '.negativeControl | {refused, refusedBy, reasonIsFee, error: (.error[0:240])}' "$G/prepare.json")"
step receipt "${PROVE[@]}" "$GASDROP_TOOL" prove --elf "$GASDROP_ELF" --input "$G/gasdrop-input.hex" --out "$G/receipt.json" \
  --image-id "$GASDROP_ID" || die "the gas-drop receipt"
tail -n 1 "$G/receipt.log" > "$G/receipt-proof.json"
say "receipt: $(jq -c '{provenS, userCycles, receiptJsonBytes, verified}' "$G/receipt-proof.json")"
ME="$(jq -r .relayer "$G/prepare.json")"; VAULT_HEX="$(jq -r .vault "$G/prepare.json")"
printf -v CREDIT '%q ' "${PROVE[@]}" nice -n 10 "$DRV" gas-drop-credit "${D[@]}" "${GD[@]}" --request "$G/request.json"
CREDIT="$CREDIT --to {to} --amount {amount}"
RELAY=("$GASDROP_TOOL" relay --receipt "$G/receipt.json" --image-id "$GASDROP_ID" --chain-id 11155111 --vault "$VAULT_HEX"
       --roots "$G/roots.txt" --amount "$AMOUNT" --credit-command "$CREDIT")
rm -f "$G/relayer-drops.json" "$G/other-relayer-drops.json"
step relay "${RELAY[@]}" --me "$ME" --claimed "$G/claimed.txt" --book "$G/relayer-drops.json" || die "the relayer's drop"
tail -n 1 "$G/relay.log" > "$G/relay.json"
say "relayer: $(jq -c '{decision, tx, amount, at}' "$G/relay.json")"
# The relayer's refusals: the same receipt again (one drop per N) and at another relayer (bound to this one).
"${RELAY[@]}" --me "$ME" --claimed "$G/claimed.txt" --book "$G/relayer-drops.json" > "$G/relay-again.json" 2> "$G/relay-again.log" \
  && die "a second drop for N was sent" || true
"${RELAY[@]}" --me "0x$(printf '77%.0s' $(seq 32))" --book "$G/other-relayer-drops.json" --claimed "$G/claimed.txt" \
  > "$G/relay-other.json" 2> "$G/relay-other.log" && die "another relayer accepted the receipt" || true
say "refused: $(jq -r .reason "$G/relay-again.json"); $(jq -r .reason "$G/relay-other.json")"
step claim "${PROVE[@]}" nice -n 10 "$DRV" gas-drop-claim "${D[@]}" "${GD[@]}" --keys "$G/keys.json" --identity "$SA" "${CAP[@]}" \
  --amount "$AMOUNT" || die "the claim"
say "claim: $(jq -c '{tx, block, provenAndSentS, accounting}' "$G/claim.json")"
"${RELAY[@]}" --me "$ME" --claimed "$G/claimed-after.txt" --book "$G/relayer-drops.json" > "$G/relay-after-claim.json" \
  2> "$G/relay-after-claim.log" && die "a drop for a claimed N was sent" || true
say "after the claim: $(jq -r .reason "$G/relay-after-claim.json")"
"$DRV" export "${D[@]}" --out "$H/export" > "$H/export.log" 2>&1 || say "export failed ($?)"

# ---- records ------------------------------------------------------------------------------------------------
. "$HERE/../scrub.sh"   # scrub: no path of this machine and no name of its folders in the records
for f in crown-a.log crown-b.log check.log export.log capture-1.log capture-2.log eth-deploy.log eth-deposit-0.log eth-deposit-1.log \
         eth-status.log session-a.json session-b.json channel.json; do
  [ -f "$H/$f" ] && scrub < "$H/$f" > "$REC/$f"
done
for f in keys.json request.json prepare.json receipt-proof.json relay.json relay-again.json relay-other.json relay-after-claim.json \
         credit.json claim.json roots.txt claimed.txt claimed-after.txt relayer-drops.json keys.log prepare.log receipt.log relay.log \
         relay-again.log relay-other.log relay-after-claim.log claim.log; do
  [ -f "$G/$f" ] && scrub < "$G/$f" > "$REC/$f"
done
cp "$G/receipt.json" "$REC/gasdrop-receipt.json"
[ -f "$G/relayer/transaction.borsh" ] && cp "$G/relayer/transaction.borsh" "$REC/credit-tx.borsh"
[ -f "$G/claim-tx.borsh" ] && cp "$G/claim-tx.borsh" "$REC/claim-tx.borsh"
mkdir -p "$REC/private"
for f in prepare-rpc.jsonl claim-rpc.jsonl after-rpc.jsonl; do [ -f "$G/$f" ] && cp "$G/$f" "$REC/private/$f"; done
[ -f "$G/relayer/rpc-transcript.jsonl" ] && cp "$G/relayer/rpc-transcript.jsonl" "$REC/private/relayer-rpc.jsonl"
mkdir -p "$REC/eth"
for f in identity.json params.json crown.json deploy.json manifest.json; do [ -f "$RUN/$f" ] && scrub < "$RUN/$f" > "$REC/eth/$f"; done
if [ -d "$H/export" ]; then mkdir -p "$REC/export"; cp "$H/export"/* "$REC/export/" 2>/dev/null || true; fi
say "done"
