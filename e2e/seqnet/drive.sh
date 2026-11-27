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
# MODE gasdrop (hook-gasdrop.sh): the cli mode's deposit; the network's gas-drop relayer by default, behind the gateway.
# MODE robust (hook-robust.sh): the cli mode's deposit twice; the relayer's root import and interrupted user commands.
set -euo pipefail
REPO="$SEQNET_REPO"; T="$SEQNET_T"; NAME="$SEQNET_NAME"; MODE="$SEQNET_MODE"
HERE="$REPO/e2e/seqnet"; BESIDE="$(cd "$REPO/.." && pwd)"
RUN="$REPO/e2e/runs/seqnet-$NAME"
W="$T/w-$NAME"; VOL="$W/vol"; BIN="$T/bin"; R0BF="${SEQNET_R0BF:-$T/inputs/r0bf}"
PROOFS="${SEQNET_PROOFS:-dev}"
RPC=http://127.0.0.1:8545
# The deployer and stand-in prover: an address nobody holds a key for, impersonated by anvil (DeployVault refuses
# anvil's public accounts on a chain id other than 31337); anvil's published dev accounts: the relayer,
DEPLOYER=0x000000000000000000000000000000005E9DE901
RELAYER=0x70997970C51812dc3A010C7d01b50e0d17dc79C8
DEPOSITOR=0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC  # the user's Ethereum account (a keystore of anvil's 3rd key)
# logoseth mode: the vault's admin (the deployer offers it the role; it accepts through the module and Logos's send stack),
# its freeze authority, and the account the user's release goes out from: anvil's 4th, 5th and 6th dev accounts, their
# keys imported into Logos Core's keystore_module by hook-logoseth.sh.
ADMIN=0x90F79bf6EB2c4f870365E785982E1f101E93b906; FREEZER=0x15d34AAf54267DB7D7c367839AAf71A00a2C6A65
RELEASE_SENDER=0x9965507D1a55bcC2695C58ba16FB37d819B0A4dc
ANVIL_MNEMONIC="test test test test test test test test test test test junk"   # anvil's published dev mnemonic
PHRASE="abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"  # the user's
# LEZ seed: the published BIP-39 test phrase (as the mini-app's devnet), never a wallet
CHECKPOINT_A_ID=0xb9c1e6a834879f6be79d09d728523565846018629b82f66986eb58fab4f52c84
CHECKPOINT_B_ID=0xf8116722ee692bcff038eed47d95f0de4b267704e45b1d11711ad84e860e626e
RELEASE_ID=0xb64900bcf741668568b3e14ef902987e305b303873ff7258c109409969eb29e9
ATTEST_ID=0x816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df
BRIDGE_ID=0xffb47f6eaec6ef13f49505e5e80209f73d38ea53792e77656994f8069dde7675   # since ADR 0046 (docs/image-ids.md ferrata_bridge_f13); B1's 0x847a4336…, the live runs' 0xccde6171…
Z=0x0000000000000000000000000000000000000000000000000000000000000000
export PATH="$HOME/.foundry/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
# This machine's paths, as the records write them (the same rewrite as at the end, for what is printed meanwhile), in
# both forms: absolute, and under `~/` as LEZ's session logs write them.
tilde() { case "$1" in "$HOME"/*) echo "~/${1#"$HOME"/}" ;; *) echo "$1" ;; esac; }
SCRUB=(-e "s#$W/#<work>/#g" -e "s#$T/#<tmp>/#g" -e "s#$REPO/#<repo>/#g" -e "s#$(tilde "$W")/#<work>/#g"
       -e "s#$(tilde "$T")/#<tmp>/#g" -e "s#$(tilde "$REPO")/#<repo>/#g" -e "s#$BESIDE/#<spike>/#g"
       -e "s#$(tilde "$BESIDE")/#<spike>/#g" -e "s#$HOME/#~/#g")
scrub() { sed "${SCRUB[@]}"; }
say() { echo "[$(date -u +%H:%M:%S)] $*" | scrub | tee -a "$RUN/run.log"; }
die() { say "STOP: $*"; exit 1; }
rm -rf "$W" "$RUN"; mkdir -p "$W" "$VOL/inputs" "$RUN/user" "$RUN/lez" "$RUN/eth" "$RUN/logs"
say "seqnet $NAME: mode $MODE, private proofs $PROOFS"
for f in checkpoint_b.bin release.bin; do
  id="$(r0vm --id --elf "$R0BF/$f" 2>/dev/null | tail -1 || true)"
  echo "$f sha256 $(sha256sum "$R0BF/$f" | cut -c1-64) r0vm id ${id:-?}" >> "$RUN/programs.txt"
done
cp "$T/crown-lez/bundle.tar.zst" "$VOL/bundle.tar.zst"
echo "bundle ($(basename "${SEQNET_BUNDLE:-bundle-crown-live-3.tar.zst}") repacked with this tree's crown-driver and session.sh) sha256 $(sha256sum "$VOL/bundle.tar.zst" | cut -c1-64)" >> "$RUN/programs.txt"
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

# ---- 2. anvil, the vault (and, but for the deployer mode, the mock verifier in its verifier's place) ---------------
. "$HERE/vault.sh"
anvil --chain-id 11155111 --port 8545 --gas-limit 100000000 --auto-impersonate > "$W/anvil.log" 2>&1 & ANVIL=$!
trap 'kill $ANVIL 2>/dev/null || true' EXIT
for _ in $(seq 60); do cast chain-id --rpc-url $RPC > /dev/null 2>&1 && break; sleep 1; done
cast rpc anvil_setBalance $DEPLOYER 0x56bc75e2d63100000 --rpc-url $RPC > /dev/null   # 100 ETH
# Every mode: DeployVault deploys RISC Zero's 3.1.0 Groth16 verifier itself (it refuses any other, the mock included),
# then the stop, the library and the vault, its fourth creation; every mode but `deployer` then puts the mock's code in
# the verifier's place on anvil (vault.sh; CI's job `vault` runs the same functions: e2e/seqnet/test-vault-deploy.sh).
MOCK=""; VAULT="$(seqnet_vault_address $RPC $DEPLOYER)" || die "the vault's address"
if [ "$MODE" = deployer ] && [ -z "${CROWN_ADMIN:-}${CROWN_FREEZE_AUTHORITY:-}${CROWN_ROLES_LABEL:-}" ]; then
  # The LEZ configuration's admin and freeze authority, unless the deployer names its own accounts (CROWN_ADMIN and
  # CROWN_FREEZE_AUTHORITY) or a label: two fresh random keys, kept in the work directory, never in the records.
  ROLES="$("$BIN/crown-driver" new-role-keys --out "$W/lez-role-keys")" || die "new-role-keys"
  CROWN_ADMIN="$(echo "$ROLES" | jq -r .admin)"; CROWN_FREEZE_AUTHORITY="$(echo "$ROLES" | jq -r .freezeAuthority)"
  export CROWN_ADMIN CROWN_FREEZE_AUTHORITY
  say "LEZ admin $CROWN_ADMIN, freeze authority $CROWN_FREEZE_AUTHORITY: fresh random keys in $W/lez-role-keys/"
fi
CID="$("$BIN/crown-driver" config-id --identity "$SA" --vault "$VAULT" | jq -r .configId)"
GS="$(python3 -I "$REPO/e2e/crown/identity.py" genesis-state --tip-id "$(jq -r .tip.id "$SA")" --tip-hash "$(jq -r .tip.hash "$SA")" \
  --keys "$(jq -r '.channel.keys | join(",")' "$SA")" --posting-timeframe "$(jq -r .channel.postingTimeframe "$SA")" \
  --config-tip-hash "$(jq -r .channel.configTipHash "$SA")")"
FROM="$(cast block-number --rpc-url $RPC)"
if [ "$MODE" = deployer ]; then VERIFIER="RISC Zero's Groth16 verifier, which DeployVault deploys first"
else VERIFIER="RISC Zero's Groth16 verifier deployed by DeployVault, its code then replaced on anvil by the vault tests' MockRiscZeroVerifier"; fi
seqnet_vault_inputs "$RUN" "$VAULT" "$CID" "$GS" "$VERIFIER" "$CHECKPOINT_A_ID" "$CHECKPOINT_B_ID" "$RELEASE_ID" "${SEQNET_DELTA:-3600}"
if [ -n "$MOCK" ]; then say "vault $VAULT (mock verifier $MOCK), configId $CID, genesisState $GS"
else say "vault $VAULT ($VERIFIER), configId $CID, genesisState $GS"; fi
[ "$MODE" = logoseth ] && export FERRATA_FREEZE_AUTHORITY=$FREEZER
seqnet_deploy_vault "$RUN" $RPC $DEPLOYER "$W/deploy.log" || { tail -n 30 "$W/deploy.log"; die "vault deployment"; }
if [ "$MODE" = logoseth ]; then
  # The deployer (impersonated: nobody holds its key) offers the admin role; the admin accepts it through the module.
  cast send --unlocked --from $DEPLOYER --rpc-url $RPC "$VAULT" "transferAdmin(address)" $ADMIN > "$W/transfer-admin.log" 2>&1 \
    || { tail -n 5 "$W/transfer-admin.log"; die "transferAdmin"; }
  say "logoseth: the vault's freeze authority $(cast call "$VAULT" 'freezeAuthority()(address)' --rpc-url $RPC), pending admin $(cast call "$VAULT" 'pendingAdmin()(address)' --rpc-url $RPC)"
fi
tail -n 1 "$W/deploy.log" | scrub | tee -a "$RUN/run.log"
rm -f "$REPO/vault/deployments/crown-seqnet-$NAME.json"   # the same record is $RUN/deploy.json
if [ "$MODE" = deployer ]; then
  # The verifier DeployVault deployed, from its record, into the run's identity.
  GV="$(jq -r '.groth16Verifier // empty' "$RUN/deploy.json")"; [ -n "$GV" ] || die "deploy.json names no Groth16 verifier"
  jq --arg g "$GV" '.groth16Verifier = $g' "$RUN/identity.json" > "$W/identity.json" && mv "$W/identity.json" "$RUN/identity.json"
  say "RISC Zero's Groth16 verifier $GV (DeployVault's first creation)"
fi
if [ "$MODE" != deployer ]; then
  MOCK="$(seqnet_mock_verifier "$RUN" $RPC)" || die "the mock verifier in the Groth16 verifier's place"
  say "RISC Zero's Groth16 verifier $MOCK deployed by DeployVault, its code replaced on anvil by MockRiscZeroVerifier's"
fi

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
ADMIN=$ADMIN; FREEZER=$FREEZER; RELEASE_SENDER=$RELEASE_SENDER
VAULT=$VAULT; MOCK=$MOCK; CID=$CID; GS=$GS; FROM=$FROM; SA=$SA; BRIDGE=$BRIDGE; ATTEST=$ATTEST
R0VM31=${R0VM31:-}; CHECKPOINT_B_ID=$CHECKPOINT_B_ID; RELEASE_ID=$RELEASE_ID; BESIDE_SHORT=${SEQNET_SHORT:-$(dirname "$T")/mseq}
EOF
. "$W/env.sh"; . "$HERE/common.sh"
F=("$BIN/ferrata" --seed "$U/seed.txt" --eth-rpc $RPC --chain-id 11155111 --vault "$VAULT" --from-block "$FROM")

# ---- 3b. cli: the private deposit with the tool, then capture-1 (miniapp: both in the hook, through the view) ------
if [ "$MODE" = cli ] || [ "$MODE" = deployer ] || [ "$MODE" = robust ] || [ "$MODE" = gasdrop ]; then
  u 01-keys "${F[@]}" keys --accounts 2 || die "keys"
  u 02-status "${F[@]}" status || die "status"
  u 03-deposit-dry-run "${F[@]}" deposit --denomination 0 --from "$DEPOSITOR" || die "deposit dry run"
  u 04-deposit "${F[@]}" deposit --denomination 0 --keystore "$KEYSTORE" --password-file "$U/eth.pass" --send --yes || die "deposit"
  # robust (hook-robust.sh): a second deposit, so that the burn killed after its sending leaves a holding to burn again;
  # the tool killed twice on its way (deposit-leg.sh: R.07, the deposit leg).
  if [ "$MODE" = robust ]; then
    . "$HERE/deposit-leg.sh"; deposit_leg
  fi
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
tail -n 6 "$W/session-x.log" | scrub | tee -a "$RUN/run.log"
cp "$VOL/session-x/logs/hook.log" "$RUN/logs/hook.log" 2>/dev/null || true
cp "$VOL/session-x/logs/session.log" "$RUN/logs/session-x.log" 2>/dev/null || true
cp "$VOL/session-a/logs/session.log" "$RUN/logs/session-a.log" 2>/dev/null || true
cp "$VOL/session-a/logs/driver.log" "$RUN/lez/crown-a.log" 2>/dev/null || true
[ -f "$VOL/session-x/export/chain.tsv" ] && cp "$VOL/session-x/export/chain.tsv" "$VOL/session-x/export/programs.tsv" "$RUN/lez/" 2>/dev/null

# ---- 5. Ethereum's transactions ---------------------------------------------------------------------
NAMES="deployer=$DEPLOYER,relayer=$RELAYER,user=$DEPOSITOR,vault=$VAULT${MOCK:+,mock verifier=$MOCK}"
if [ "$MODE" = deployer ]; then NAMES="$NAMES${SD_ADMIN:+,new admin=$SD_ADMIN}"
else NAMES="$NAMES,admin=$ADMIN,freeze authority=$FREEZER,release sender=$RELEASE_SENDER"; fi
python3 -I "$HERE/ethtxs.py" --rpc $RPC --vault-out "$REPO/vault/out" --names "$NAMES" \
  > "$RUN/eth/transactions.tsv" 2> "$W/ethtxs.err" || say "ethtxs.py failed: $(tail -n 3 "$W/ethtxs.err")"
kill $ANVIL 2>/dev/null || true; trap - EXIT
find "$RUN" -type f \( -name '*.txt' -o -name '*.log' -o -name '*.json' -o -name '*.tsv' -o -name '*.md' -o -name '*.jsonl' -o -name '*.xml' \) \
  -exec sed -i "${SCRUB[@]}" {} +
python3 -I "$HERE/summary.py" "$RUN" | scrub | tee -a "$RUN/run.log" || true
# The records are to be committed: none may keep a path of this machine.
if grep -rIl -F -e "$REPO/" -e "$T/" -e "$W/" -e "$BESIDE/" -e "$HOME/" -e "$(tilde "$REPO")/" -e "$(tilde "$T")/" -e "$(tilde "$BESIDE")/" "$RUN"; then
  die "the files above keep a path of this machine"
fi
[ "$xrc" = 0 ] || die "session X (hook) exit $xrc"
say "seqnet $NAME: DONE"
