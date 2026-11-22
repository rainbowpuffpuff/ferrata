#!/usr/bin/env bash
# The bridge's private claim and private burn on a running zone: the hook of ../run.sh's phase `hook`
# (FERRATA_LIVE_HOOK=lez/live/hooks/crown.sh), which runs it while the zone runs, with the zone in ZONE_* variables.
# On the build box only; nothing is sent to any public network.
#
# The crown run's LEZ side (lez/crown/README.md) on this zone, with the Ethereum side of e2e/seqnet: a local anvil
# (chain id 11155111; the stand-in committee of crown-driver finalizes its blocks), the vault tests'
# MockRiscZeroVerifier, FerrataVault v5 in mode (B) deployed with this zone's configId and genesisState
# (e2e/crown/sepolia.sh deploy), deposit j0 (public, to LEZ's public test account 0) and j1 (private, to LEZ's private
# test account 0), each captured (lez/crown/capture-anvil.py). Then crown-driver:
#   crown-a  deploy the attestation program (attest) and the bridge program (bridge), the stand-in instance;
#   crown-b  the instance follows the captures; the bridge's Init, Register ETH, ImportFees, ImportRoot n=1 and n=2,
#            Claim of j0 (public), ClaimPrivately of j1 and the private Burn, both executed and proven for real by
#            LEZ's wallet (RISC0_DEV_MODE unset; the sequencer verifies the receipts), and a public Burn;
#   export   the records.
# Records into $ZONE_OUT/crown/ (machine paths removed).
#
# Environment (from run-live.sh): ZONE_RPC, ZONE_BEDROCK, ZONE_RUN_DIR, ZONE_LOG, ZONE_BIN, ZONE_OUT, ZONE_WAIT_FINAL,
# ZONE_PROGRAM_BIN (the attestation program), ZONE_SEQUENCER_CONFIG, ZONE_DEPLOYMENT. Ours: CROWN_BRIDGE_BIN (the
# bridge program, required), CROWN_DRIVER (default $ZONE_BIN/crown-driver), CROWN_FIXTURE (default the 12 Nov 2026
# Sepolia checkpoint fixture), CROWN_PROOF_THREADS (RAYON_NUM_THREADS of the prover, default 32), CROWN_RUN_NAME
# (the Ethereum side's record directory under e2e/runs/, default live-crown-hook). Needs anvil, cast and forge
# (vault/lib installed and vault built: e2e/seqnet/run.sh build does both), jq, python3, r0vm 3.0.5.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../.." && pwd)"
BESIDE="$(cd "$REPO/.." && pwd)"
: "${ZONE_RPC:?run from run-live.sh phase hook}" "${CROWN_BRIDGE_BIN:?the bridge program binary}"
DRV="${CROWN_DRIVER:-$ZONE_BIN/crown-driver}"
FIXTURE="${CROWN_FIXTURE:-$REPO/crates/ferrata-spectests/fixtures/sepolia-2026-11-12}"
THREADS="${CROWN_PROOF_THREADS:-32}"
H="$ZONE_RUN_DIR/crown"; REC="$ZONE_OUT/crown"
RUN="$REPO/e2e/runs/${CROWN_RUN_NAME:-live-crown-hook}"
RPC=http://127.0.0.1:8545
# The deployer: an address nobody holds a key for, impersonated by anvil (as e2e/seqnet's).
DEPLOYER=0x000000000000000000000000000000005E9DE901
CHECKPOINT_A_ID=0xb9c1e6a834879f6be79d09d728523565846018629b82f66986eb58fab4f52c84
CHECKPOINT_B_ID=0xf8116722ee692bcff038eed47d95f0de4b267704e45b1d11711ad84e860e626e
RELEASE_ID=0xb64900bcf741668568b3e14ef902987e305b303873ff7258c109409969eb29e9
Z=0x0000000000000000000000000000000000000000000000000000000000000000
export PATH="$HOME/.foundry/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
say() { echo "[crown-hook $(date -u +%H:%M:%S)] $*"; }
die() { say "STOP: $*"; exit 1; }
rm -rf "$H" "$RUN"; mkdir -p "$H" "$RUN" "$REC"
[ -x "$DRV" ] || die "no crown-driver at $DRV"
[ -d "$REPO/vault/out" ] || die "the vault is not built (vault/out)"

# ---- LEZ session A on this zone ----------------------------------------------------------------------------
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

# ---- Ethereum: anvil, the mock verifier, the vault, the two deposits and their captures --------------------
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
jq -n --arg v "$VAULT" --arg c "$CID" --arg g "$GS" --arg m "$MOCK" '{label: "live crown hook: local anvil (chain id 11155111, the stand-in committee finalizes its blocks), the vault tests MockRiscZeroVerifier",
  chainId: 11155111, vault: $v, configId: $c, genesisState: $g, groth16Verifier: $m, delta: 3600}' > "$RUN/identity.json"
jq -n --arg c "$CID" --arg g "$GS" --arg a "$CHECKPOINT_A_ID" --arg b "$CHECKPOINT_B_ID" --arg r "$RELEASE_ID" --arg z "$Z" \
  '{_source: "live crown hook (the final programs of docs/image-ids.md; the identity of this zone session A)",
    checkpointImageIdA: $a, checkpointImageIdB: $b, releaseImageId: $r, lezConfigId: $c, genesisState: $g, nMin: 1, tMin: 1,
    postingTimeframe: 0, ethLezDefinition: $z, tokenLezDefinition: $z}' > "$RUN/params.json"
d0="$("$DRV" destinations | jq -r .public0)"; d1="$("$DRV" destinations | jq -r .private0)"
C0="$(python3 -I "$REPO/e2e/crown/identity.py" commitment --chain-id 11155111 --vault "$VAULT" --depositor "$DEPLOYER" --j 0 --kind public --dest "$d0")"
C1="$(python3 -I "$REPO/e2e/crown/identity.py" commitment --chain-id 11155111 --vault "$VAULT" --depositor "$DEPLOYER" --j 1 --kind private --dest "$d1")"
jq -n --arg v "$VAULT" --arg c0 "$C0" --arg c1 "$C1" '{chainId: 11155111, vault: $v,
  deposits: [{j: 0, denomIndex: 0, commitment: $c0}, {j: 1, denomIndex: 0, commitment: $c1}]}' > "$RUN/crown.json"
say "vault $VAULT (mock verifier $MOCK), configId $CID, genesisState $GS"
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
"$DRV" check --chain stand-in --program-bin "$ZONE_PROGRAM_BIN" --bridge-bin "$CROWN_BRIDGE_BIN" --capture "$H/capture-1" \
  --capture "$H/capture-2" --identity "$SA" > "$H/check.log" 2>&1 || { tail -n 20 "$H/check.log"; die "crown-driver check"; }
say "crown-driver check: $(tail -n 1 "$H/check.log")"

# ---- LEZ session B: the bridge, with the private claim and burn proven for real ----------------------------
say "crown-b: real private proofs, RAYON_NUM_THREADS=$THREADS"
crc=0
env -u RISC0_DEV_MODE RAYON_NUM_THREADS="$THREADS" RISC0_SERVER_PATH="$(command -v r0vm)" nice -n 10 "$DRV" crown-b "${D[@]}" \
  --identity "$SA" --capture "$H/capture-1" --capture "$H/capture-2" --private real \
  --session-out "$H/session-b.json" --notes-out "$H/burn-notes.json" > "$H/crown-b.log" 2>&1 || crc=$?
tail -n 15 "$H/crown-b.log"
"$DRV" export "${D[@]}" --out "$H/export" > "$H/export.log" 2>&1 || say "export failed ($?)"

# ---- records ------------------------------------------------------------------------------------------------
. "$HERE/../scrub.sh"   # scrub: no path of this machine and no name of its folders in the records
for f in crown-a.log crown-b.log check.log export.log capture-1.log capture-2.log eth-deploy.log eth-deposit-0.log eth-deposit-1.log \
         eth-status.log session-a.json session-b.json burn-notes.json channel.json; do
  [ -f "$H/$f" ] && scrub < "$H/$f" > "$REC/$f"
done
mkdir -p "$REC/eth"
for f in identity.json params.json crown.json deploy.json manifest.json; do [ -f "$RUN/$f" ] && scrub < "$RUN/$f" > "$REC/eth/$f"; done
if [ -d "$H/export" ]; then mkdir -p "$REC/export"; cp "$H/export"/* "$REC/export/" 2>/dev/null || true; fi
for f in "$H"/driver/out/crown-private-*-tx.borsh; do [ -f "$f" ] && cp "$f" "$REC/"; done   # the proven transactions
[ "$crc" = 0 ] || die "crown-b exit $crc"
say "done: $(jq -c '[.steps[] | [.step, .status, .block]]' "$H/session-b.json" 2>/dev/null | cut -c1-600)"
