#!/usr/bin/env bash
# One run of e2e/seqnet (started by run.sh cli|miniapp NAME, on the build box). Steps:
#   1. LEZ session A (lez/crown/session.sh a, CROWN_CHAIN=stand-in): a new zone, the final attestation and bridge
#      programs deployed, the stand-in light-client instance; the zone stopped and saved (generation 1).
#   2. anvil (chain id 11155111), the vault tests' MockRiscZeroVerifier, FerrataVault v5 in mode (B) with session A's
#      configId and genesisState (e2e/crown/sepolia.sh deploy, as crown-rehearsal-5).
#   3. The user's private deposit with the `ferrata` tool (cli) or the mini-app (miniapp); capture-1 of the vault.
#   4. The zone again (session.sh x) with the hook (hook-cli.sh or hook-miniapp.sh): the operator's crown-b
#      --operator-only (the stand-in committee's finality update, Init, Register ETH, ImportRoot), the gas drop's
#      stand-in, the user's private claim and private burn, the checkpoint, the release, the records.
#   5. Ethereum's transactions listed (ethtxs.py); anvil stopped.
# Records: e2e/runs/seqnet-NAME/ (committed); work: spike/tmp/miniapp-sequencer/w-NAME/ (not committed).
set -euo pipefail
REPO="$SEQNET_REPO"; T="$SEQNET_T"; NAME="$SEQNET_NAME"; MODE="$SEQNET_MODE"
HERE="$REPO/e2e/seqnet"
RUN="$REPO/e2e/runs/seqnet-$NAME"
W="$T/w-$NAME"; VOL="$W/vol"; BIN="$T/bin"; R0BF="$T/inputs/r0bf"
PROOFS="${SEQNET_PROOFS:-dev}"
RPC=http://127.0.0.1:8545
# The deployer and stand-in prover: an address nobody holds a key for, impersonated by anvil (DeployVault refuses
# anvil's public accounts on a chain id other than 31337); anvil's published dev accounts: the relayer,
DEPLOYER=0x000000000000000000000000000000005E9DE901
RELAYER=0x70997970C51812dc3A010C7d01b50e0d17dc79C8
DEPOSITOR=0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC  # the user's Ethereum account (a keystore of anvil's 3rd key)
ANVIL_MNEMONIC="test test test test test test test test test test test junk"   # anvil's published dev mnemonic
PHRASE="abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"  # the user's
# LEZ seed: the published BIP-39 test phrase (as the mini-app's devnet), never a wallet
CHECKPOINT_A_ID=0xb9c1e6a834879f6be79d09d728523565846018629b82f66986eb58fab4f52c84
CHECKPOINT_B_ID=0xf8116722ee692bcff038eed47d95f0de4b267704e45b1d11711ad84e860e626e
RELEASE_ID=0xb64900bcf741668568b3e14ef902987e305b303873ff7258c109409969eb29e9
ATTEST_ID=0x816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df
BRIDGE_ID=0xccde6171026cedff8e94fc20491b0d6e4fc825f127ce71f77016e2d96d3871dd
Z=0x0000000000000000000000000000000000000000000000000000000000000000
export PATH="$HOME/.foundry/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
say() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$RUN/run.log"; }
die() { say "STOP: $*"; exit 1; }
rm -rf "$W" "$RUN"; mkdir -p "$W" "$VOL/inputs" "$RUN/user" "$RUN/lez" "$RUN/eth" "$RUN/logs"
say "seqnet $NAME: mode $MODE, private proofs $PROOFS"
for f in checkpoint_b.bin release.bin; do
  id="$(r0vm --id --elf "$R0BF/$f" 2>/dev/null | tail -1 || true)"
  echo "$f sha256 $(sha256sum "$R0BF/$f" | cut -c1-64) r0vm id ${id:-?}" >> "$RUN/programs.txt"
done
cp "$T/crown-lez/bundle.tar.zst" "$VOL/bundle.tar.zst"
echo "bundle (crown-live-3's, this tree's crown-driver and session.sh) sha256 $(sha256sum "$VOL/bundle.tar.zst" | cut -c1-64)" >> "$RUN/programs.txt"
export CROWN_LEZ_VOL="$VOL" CROWN_LEZ_WORK="$W/zone" CROWN_CHAIN=stand-in

# ---- 1. LEZ session A -------------------------------------------------------------------------------
say "LEZ session A: a new zone, the programs, the stand-in instance"
bash "$REPO/lez/crown/session.sh" a > "$W/session-a.log" 2>&1 || { tail -n 30 "$W/session-a.log"; die "session A"; }
SA="$VOL/session-a/session-a.json"
jq '{tip, channel: .channel.id, bedrockGenesisId, instance: .instance.id, attestation, bridge, evmChainId}' "$SA" > "$RUN/lez/session-a.json"
cp "$VOL/session-a/channel.json" "$VOL/session-a/deployment-settings.yaml" "$RUN/lez/" 2>/dev/null || true
[ "$(jq -r .attestation.imageId "$SA")" = "$ATTEST_ID" ] || die "the attestation program is not the final one ($(jq -r .attestation.imageId "$SA"))"
[ "$(jq -r .bridge.imageId "$SA")" = "$BRIDGE_ID" ] || die "the bridge program is not the final one ($(jq -r .bridge.imageId "$SA"))"
BRIDGE="$(jq -r .bridge.header "$SA")"; ATTEST="$(jq -r .attestation.account "$SA")"
say "session A: tip $(jq -c .tip "$SA"); bridge program $BRIDGE (image $BRIDGE_ID), attestation program $ATTEST (image $ATTEST_ID)"

# ---- 2. anvil, the mock verifier, the vault ---------------------------------------------------------
anvil --chain-id 11155111 --port 8545 --gas-limit 100000000 --auto-impersonate > "$W/anvil.log" 2>&1 & ANVIL=$!
trap 'kill $ANVIL 2>/dev/null || true' EXIT
for _ in $(seq 60); do cast chain-id --rpc-url $RPC > /dev/null 2>&1 && break; sleep 1; done
cast rpc anvil_setBalance $DEPLOYER 0x56bc75e2d63100000 --rpc-url $RPC > /dev/null   # 100 ETH
MOCK="$(cd "$REPO/vault" && forge create test/mocks/MockVerifiers.sol:MockRiscZeroVerifier --rpc-url $RPC --unlocked \
  --from $DEPLOYER --broadcast --json 2>/dev/null | jq -r .deployedTo)"
[ -n "$MOCK" ] && [ "$MOCK" != null ] || die "the mock verifier was not deployed"
N="$(cast nonce $DEPLOYER --rpc-url $RPC)"
VAULT="$(cast compute-address $DEPLOYER --nonce $((N + 2)) | awk '{print $NF}')"
CID="$("$BIN/crown-driver" config-id --identity "$SA" --vault "$VAULT" | jq -r .configId)"
GS="$(python3 -I "$REPO/e2e/crown/identity.py" genesis-state --tip-id "$(jq -r .tip.id "$SA")" --tip-hash "$(jq -r .tip.hash "$SA")" \
  --keys "$(jq -r '.channel.keys | join(",")' "$SA")" --posting-timeframe "$(jq -r .channel.postingTimeframe "$SA")" \
  --config-tip-hash "$(jq -r .channel.configTipHash "$SA")")"
FROM="$(cast block-number --rpc-url $RPC)"
jq -n --arg v "$VAULT" --arg c "$CID" --arg g "$GS" --arg m "$MOCK" '{label: "seqnet: local anvil (chain id 11155111, the stand-in committee finalizes its blocks), the vault tests MockRiscZeroVerifier behind the emergency stop",
  chainId: 11155111, vault: $v, configId: $c, genesisState: $g, groth16Verifier: $m, delta: 3600}' > "$RUN/identity.json"
jq -n --arg c "$CID" --arg g "$GS" --arg a "$CHECKPOINT_A_ID" --arg b "$CHECKPOINT_B_ID" --arg r "$RELEASE_ID" --arg z "$Z" \
  '{_source: "seqnet (the final programs of docs/image-ids.md; a stand-in identity from a real devnet session A)",
    checkpointImageIdA: $a, checkpointImageIdB: $b, releaseImageId: $r, lezConfigId: $c, genesisState: $g, nMin: 1, tMin: 1,
    postingTimeframe: 0, ethLezDefinition: $z, tokenLezDefinition: $z}' > "$RUN/params.json"
jq -n --arg v "$VAULT" '{chainId: 11155111, vault: $v, deposits: []}' > "$RUN/crown.json"
say "vault $VAULT (mock verifier $MOCK), configId $CID, genesisState $GS"
SEPOLIA_RPC_URL=$RPC ETH_FROM=$DEPLOYER FERRATA_UNLOCKED=1 FERRATA_NONCE_GAP=0 bash "$REPO/e2e/crown/sepolia.sh" "$RUN" deploy \
  > "$W/deploy.log" 2>&1 || { tail -n 30 "$W/deploy.log"; die "vault deployment"; }
tail -n 1 "$W/deploy.log" | tee -a "$RUN/run.log"
rm -f "$REPO/vault/deployments/crown-seqnet-$NAME.json"   # the same record is $RUN/deploy.json

# ---- 3. The user: seed, LEZ wallet password, Ethereum keystore --------------------------------------
U="$W/user"; mkdir -p "$U"
printf '%s\n' "$PHRASE" > "$U/seed.txt"; printf 'seqnet-user\n' > "$U/lez.pass"; printf 'seqnet-depositor\n' > "$U/eth.pass"
chmod 600 "$U/seed.txt" "$U/lez.pass" "$U/eth.pass"
cast wallet import depositor --private-key "$(cast wallet private-key --mnemonic "$ANVIL_MNEMONIC" --mnemonic-index 2)" \
  --keystore-dir "$U/keystore" --unsafe-password seqnet-depositor > /dev/null
[ "$(cast wallet address --keystore "$U/keystore/depositor" --password seqnet-depositor)" = "$DEPOSITOR" ] || die "the user's keystore"
KEYSTORE="$U/keystore/depositor"
cat > "$W/env.sh" <<EOF
RUN=$RUN; W=$W; VOL=$VOL; BIN=$BIN; R0BF=$R0BF; PROOFS=$PROOFS; RPC=$RPC; MODE=$MODE; NAME=$NAME; REPO=$REPO; T=$T
DEPLOYER=$DEPLOYER; RELAYER=$RELAYER; DEPOSITOR=$DEPOSITOR; KEYSTORE=$KEYSTORE; U=$U
VAULT=$VAULT; MOCK=$MOCK; CID=$CID; GS=$GS; FROM=$FROM; SA=$SA; BRIDGE=$BRIDGE; ATTEST=$ATTEST
CHECKPOINT_B_ID=$CHECKPOINT_B_ID; RELEASE_ID=$RELEASE_ID; BESIDE_SHORT=$(dirname "$T")/mseq
EOF
. "$W/env.sh"; . "$HERE/common.sh"
F=("$BIN/ferrata" --seed "$U/seed.txt" --eth-rpc $RPC --chain-id 11155111 --vault "$VAULT" --from-block "$FROM")

# ---- 3b. cli: the private deposit with the tool, then capture-1 (miniapp: both in the hook, through the view) ------
if [ "$MODE" = cli ]; then
  u 01-keys "${F[@]}" keys --accounts 2 || die "keys"
  u 02-status "${F[@]}" status || die "status"
  u 03-deposit-dry-run "${F[@]}" deposit --denomination 0 --from "$DEPOSITOR" || die "deposit dry run"
  u 04-deposit "${F[@]}" deposit --denomination 0 --keystore "$KEYSTORE" --password-file "$U/eth.pass" --send --yes || die "deposit"
  u 05-scan "${F[@]}" scan || die "scan"
  slots="$("$BIN/crown-driver" capture-slots | jq -r '.slots[].slot')"
  sargs=(); for s in $slots; do sargs+=(--slot "$s"); done
  python3 -I "$REPO/lez/crown/capture-anvil.py" --rpc $RPC --vault "$VAULT" --from-block "$FROM" "${sargs[@]}" "$VOL/inputs/capture-1" \
    > "$W/capture-1.log" 2>&1 || { tail -n 20 "$W/capture-1.log"; die "capture-1"; }
  say "capture-1: $(tail -n 1 "$W/capture-1.log")"
  B="$W/check-bundle"; mkdir -p "$B"; zstd -q -d -c "$VOL/bundle.tar.zst" | tar -C "$B" -xf -
  "$BIN/crown-driver" check --chain stand-in --program-bin "$B/program/ferrata_lez.bin" --bridge-bin "$B/program/ferrata_bridge.bin" \
    --capture "$VOL/inputs/capture-1" --identity "$SA" --operator-only yes > "$W/check.log" 2>&1 || { tail -n 20 "$W/check.log"; die "crown-driver check"; }
  say "crown-driver check: $(tail -n 1 "$W/check.log")"
fi

# ---- 4. The zone again, with the hook ---------------------------------------------------------------
dev=0; [ "$PROOFS" = dev ] && dev=1
say "LEZ session X: the zone restored and running; hook-$MODE.sh"
xrc=0
SEQNET_ENV="$W/env.sh" CROWN_HOOK="$HERE/hook-$MODE.sh" CROWN_SEQUENCER_DEV_MODE=$dev CROWN_DEADLINE_MIN=170 \
  bash "$REPO/lez/crown/session.sh" x > "$W/session-x.log" 2>&1 || xrc=$?
tail -n 6 "$W/session-x.log" | tee -a "$RUN/run.log"
cp "$VOL/session-x/logs/hook.log" "$RUN/logs/hook.log" 2>/dev/null || true
cp "$VOL/session-x/logs/session.log" "$RUN/logs/session-x.log" 2>/dev/null || true
cp "$VOL/session-a/logs/session.log" "$RUN/logs/session-a.log" 2>/dev/null || true
cp "$VOL/session-a/logs/driver.log" "$RUN/lez/crown-a.log" 2>/dev/null || true
[ -f "$VOL/session-x/export/chain.tsv" ] && cp "$VOL/session-x/export/chain.tsv" "$VOL/session-x/export/programs.tsv" "$RUN/lez/" 2>/dev/null

# ---- 5. Ethereum's transactions ---------------------------------------------------------------------
python3 -I "$HERE/ethtxs.py" --rpc $RPC --vault-out "$REPO/vault/out" --names "deployer=$DEPLOYER,relayer=$RELAYER,user=$DEPOSITOR,vault=$VAULT,mock verifier=$MOCK" \
  > "$RUN/eth/transactions.tsv" 2> "$W/ethtxs.err" || say "ethtxs.py failed: $(tail -n 3 "$W/ethtxs.err")"
kill $ANVIL 2>/dev/null || true; trap - EXIT
find "$RUN" -type f \( -name '*.txt' -o -name '*.log' -o -name '*.json' -o -name '*.tsv' -o -name '*.md' -o -name '*.jsonl' -o -name '*.xml' \) \
  -exec sed -i -e "s#$W/#<work>/#g" -e "s#$T/#<tmp>/#g" -e "s#$REPO/#<repo>/#g" -e "s#$HOME/#~/#g" {} +
python3 -I "$HERE/summary.py" "$RUN" | tee -a "$RUN/run.log" || true
[ "$xrc" = 0 ] || die "session X (hook) exit $xrc"
say "seqnet $NAME: DONE"
