#!/usr/bin/env bash
# e2e/seqnet's hook for the mini-app with its Ethereum side through Logos Core's own modules (RFP-022 U.1, U.2):
# lez/crown/session.sh x runs it while the zone runs; drive.sh has made the vault on anvil (its freeze authority an
# account of the run, its admin role offered by the deployer to another). The user's round trip through the mini-app's
# view (modules/ferrata_bridge_ui/qml/Main.qml) driving ferrata_bridge_module in ONE headless Logos Core holding:
#   - Logos's Ethereum send stack (eth_rpc_module, fee_module, keystore_module, signer_manager_module, tx_sender_module)
#     and its headless approver and custodian (evm_signer_cli, evm_keystore_cli), built from their sources at the
#     revisions tx_sender_module f1754c3 pins (modules/scripts/send-stack.sh);
#   - lez_core (logos-execution-zone-module, LEZ's wallet as a module) with modules/patches/lez_core-*.patch, when
#     run.sh build-lezcore built it (else the LEZ side goes through seqnet-chain, as the miniapp mode);
#   - ferrata_bridge_module.
# The module's session (`wallet eth`) reads the vault through eth_rpc_module (chain 11155111 pointed at this anvil) and
# sends the user's deposit and release and the vault's admin calls through tx_sender_module; each send waits for ONE
# approval, given by the person at evm_signer_cli (ops.sh approve: the test plays the person), the keys imported with
# evm_keystore_cli into keystore_module (the module holds none). seqnet-chain stays for LEZ's history (imported roots,
# published burns: lez_core's wallet side serves no blocks) and as the release's stand-in prover (the vault's verifier
# is the tests' mock). The view is driven headless by Qt Quick Test (qmltest-logos, SEQNET_ETH=logos).
set -uo pipefail
. "$SEQNET_ENV"
. "$REPO/e2e/seqnet/common.sh"
M="$RUN/miniapp"; mkdir -p "$M/screenshots" "$RUN/approver"
CHAIN_PORT=18700
LOGOSCTL="$T/logos/logosctl"; LGX_DIR="$T/logos/lgx-ferrata_bridge_module"; RUNNER="$BIN/ferrata_miniapp_logos"
STACK="$T/logoseth/stack"; LEZ_PATCHED="$T/logos/lgx-lez_core-patched"
[ -x "$LOGOSCTL" ] && [ -d "$LGX_DIR" ] && [ -x "$RUNNER" ] && [ -d "$STACK/lgx-tx_sender_module" ] \
  || die "run.sh build-logoseth first (logosctl, the module's .lgx, the send stack's, the qmltest runner)"

# 1. The chain service: LEZ's history and the release's stand-in prover (the module reads and sends Ethereum itself);
#    its LEZ wallet also holds the configuration's role keys (the crown's derived throwaway keys), the stand-in for the
#    roles' own LEZ wallets: a LEZ admin message the module builds (module.submit) is HELD there until a person approves
#    exactly that message with the role password (module.approve_role; the test plays the person: ops.sh approve-lez).
#    The module never has the password.
mkdir -p "$W/relayer-inbox"
RK="$W/lez-role"; mkdir -p "$RK"; chmod 700 "$RK"; ( umask 077; printf '%s' "seqnet LEZ role wallet (local)" > "$RK/password" )
"${PROVE[@]}" "$BIN/seqnet-chain" --listen "127.0.0.1:$CHAIN_PORT" --eth-rpc "$RPC" --chain-id 11155111 --vault "$VAULT" --from-block "$FROM" \
  --keystore "$KEYSTORE" --password-file "$U/eth.pass" --sequencer "$SEQ" --seed "$U/seed.txt" --lez-wallet "$U/lez-wallet" \
  --lez-password-file "$U/lez.pass" --bridge-account "$BRIDGE" --bridge-program "$BRIDGE_BIN" --relayer-bin "$BIN/ferrata-relayer" \
  --relayer "$RELAYER" --relayer-state "$W/relayer" --relayer-inbox "$W/relayer-inbox" --mock "$MOCK" --prover "$DEPLOYER" \
  --release-image-id "$RELEASE_ID" --log "$M/chain-calls.jsonl" --proofs "$PROOFS" \
  --lez-role-key crown/admin --lez-role-key crown/freeze-authority --lez-role-password-file "$RK/password" > "$W/seqnet-chain.log" 2>&1 &
CHAIN_PID=$!
for _ in $(seq 120); do curl -fsS -m 2 -d '{"method":"module.info","params":{"lez_only":true}}' "http://127.0.0.1:$CHAIN_PORT/" > /dev/null 2>&1 && break; sleep 1; done
kill -0 $CHAIN_PID 2>/dev/null || { tail -n 20 "$W/seqnet-chain.log"; die "seqnet-chain did not start"; }

# 2. One Logos Core (sockets in a short TMPDIR: a Unix socket path holds 107 bytes).
LT="$BESIDE_SHORT"; mkdir -p "$LT"
S="$W/logos-session"; rm -rf "$S"; mkdir -p "$W/logos-home"
ctl() { TMPDIR="$LT" "$LOGOSCTL" --config-dir "$S" "$@"; }
unwrap() { jq -c 'if (.result | type) == "string" then (.result | fromjson? // .result) else (.result // .) end' 2>/dev/null; }
j() { ctl --timeout 600000 call "$@" --json < /dev/null 2>&1 | unwrap; }
dmode=(env RISC0_DEV_MODE=1); [ "$PROOFS" = real ] && dmode=(env -u RISC0_DEV_MODE)
# lez_core proves private transactions inside its module process: the zone's r0vm on its PATH and as RISC0_SERVER_PATH.
"${dmode[@]}" RISC0_SERVER_PATH="${RISC0_SERVER_PATH:-}" PATH="${ZONE_BIN:-/nonexistent}:$PATH" TMPDIR="$LT" HOME="$W/logos-home" \
  "$LOGOSCTL" --config-dir "$S" daemon start --detach > "$M/logos-daemon.txt" 2>&1 || { cat "$M/logos-daemon.txt"; die "logosctl daemon start"; }
WATCH_PID=""
finish() {
  [ -n "$WATCH_PID" ] && kill "$WATCH_PID" 2>/dev/null
  ctl daemon stop >> "$M/logos-daemon.txt" 2>&1 || true
  tail -n 60 "$S"/logs/daemon_*.log > "$M/logos-daemon-log-tail.txt" 2>/dev/null || true
  curl -fsS -m 5 -d '{"method":"module.stop","params":{}}' "http://127.0.0.1:$CHAIN_PORT/" > /dev/null 2>&1 || true
  sleep 1; kill $CHAIN_PID 2>/dev/null || true
}
trap finish EXIT
LEZ_MOD=""; [ -d "$LEZ_PATCHED" ] && [ "${SEQNET_LEZ:-lez_core}" = lez_core ] && LEZ_MOD=lez_core
for d in "$STACK"/lgx-*/ "$LGX_DIR/" ${LEZ_MOD:+"$LEZ_PATCHED/"}; do ctl install "$d"/*.lgx -y >> "$M/logos-daemon.txt" 2>&1 || die "install $d"; done
# The providers first, then the approver (up before the first send: an unclaimed request is swept after 60 s), the
# custodian, lez_core, the bridge module.
MODS="keystore_module signer_manager_module eth_rpc_module fee_module tx_sender_module evm_signer_cli evm_keystore_cli $LEZ_MOD ferrata_bridge_module"
for m in $MODS; do ctl module load "$m" >> "$M/logos-daemon.txt" 2>&1 || die "module load $m"; done
ctl module list --json < /dev/null > "$M/modules-loaded.json" 2>&1 || true
( for d in "$STACK"/lgx-*/ "$LGX_DIR/" ${LEZ_MOD:+"$LEZ_PATCHED/"}; do sha256sum "$d"/*.lgx; done | sed "s#$T/#<tmp>/#g" ) > "$M/lgx-sha256.txt"
say "Logos Core: $(jq -r '[.[] | select(.status == "loaded") | .name] | join(", ")' "$M/modules-loaded.json" 2>/dev/null)"
# Roles as Logos's own headless doc-test names them (configure is total; the Basecamp surfaces kept).
j keystore_module configure '{"approvers":["evm_signer_ui","evm_signer_cli"],"custodians":["evm_keystore_ui","evm_keystore_cli"],"managers":["signer_manager_module"]}' > "$M/configure-keystore.json"
j signer_manager_module configure '{"approvers":["evm_signer_ui","evm_signer_cli"],"custodians":["evm_keystore_ui"],"signers":["keystore_module"]}' > "$M/configure-signer-manager.json"
# eth_rpc_module: Sepolia's chain id pointed at this anvil (the stand-in light client's configuration is Sepolia's).
j eth_rpc_module init_defaults > /dev/null
j eth_rpc_module set_chain_config 11155111 "{\"endpoint\":\"$RPC\"}" > "$M/eth-rpc-chain.json"
j eth_rpc_module set_chain_enabled 11155111 true > /dev/null
j eth_rpc_module verify_chain_id 11155111 > "$M/eth-rpc-verify.json"
# The keys, imported by Logos's custodian CLI into keystore_module: the user (the depositor), the release sender, the
# vault's (offered) admin and its freeze authority. Files only; the password is the run's.
ANVIL_MNEMONIC="test test test test test test test test test test test junk"   # anvil's published dev mnemonic
K="$W/logos-keys"; mkdir -p "$K"; chmod 700 "$K"
( umask 077; printf '%s' "seqnet logos keystore (local)" > "$K/password"
  for i in 2 3 4 5; do printf '%s' "$(cast wallet private-key --mnemonic "$ANVIL_MNEMONIC" --mnemonic-index $i | sed 's/^0x//')" > "$K/$i.key"; done )
for i in 2 3 4 5; do j evm_keystore_cli import_private_key "@$K/$i.key" "@$K/password" > "$M/import-$i.json"; done
sleep 2
j signer_manager_module accounts > "$M/signer-accounts.json"
say "keystore_module holds: $(jq -c '[.accounts[]?.address]' "$M/signer-accounts.json" 2>/dev/null)"
ctl watch evm_signer_cli --event prompt --json > "$RUN/approver/prompts.jsonl" 2>&1 & WATCH_PID=$!

# The user's LEZ wallet inside lez_core: the storage seqnet-chain made from the seed (a client of this zone's sequencer).
LEZ_PARAMS=""
if [ -n "$LEZ_MOD" ]; then
  cp -r "$U/lez-wallet" "$W/lezcore-wallet"
  LEZ_PARAMS="$(jq -c -n --arg c "$W/lezcore-wallet/wallet_config.json" --arg s "$W/lezcore-wallet/storage.json" --arg p "$U/lez.pass" \
    --arg b "$BRIDGE_BIN" '{via: "lez_core", wallet: {config: $c, storage: $s, password_file: $p}, program_bin: $b}')"
fi

# 3. The view, headless; the person at the Logos signer is ops.sh approve.
say "the mini-app: Qt Quick Test (offscreen), Ethereum through Logos Core's modules, LEZ through ${LEZ_MOD:-seqnet-chain}"
t0=$(date +%s)
( cd "$REPO/modules/ferrata_bridge_ui/qmltest-logos" && env QT_QPA_PLATFORM=offscreen LOGOSCTL="$LOGOSCTL" LOGOS_SESSION="$S" \
    TMPDIR="$LT" LOGOS_TMPDIR_SHORT="$LT" LOGOS_PASSWORD_FILE="$K/password" SEQNET_TMP="$W" SEQNET_CALL_LOG="$M/calls.jsonl" \
    SEQNET_OPS="bash $REPO/e2e/seqnet/ops.sh" SEQNET_OPS_LOG="$M/ops.jsonl" SEQNET_CHAIN_URL="http://127.0.0.1:$CHAIN_PORT/" \
    SEQNET_SHOTS="$M/screenshots" SEQNET_ETH=logos SEQNET_FROM_BLOCK="$FROM" SEQNET_ADMIN="$ADMIN" SEQNET_FREEZE_AUTHORITY="$FREEZER" \
    SEQNET_RELEASE_SENDER="$RELEASE_SENDER" SEQNET_LEZ_PARAMS="$LEZ_PARAMS" SEQNET_LEZ_ROLE_PASSWORD_FILE="$RK/password" \
    "$RUNNER" -input tst_miniapp_logos.qml -o "$M/qmltest.txt,txt" -o "$M/qmltest.xml,xunitxml" -maxwarnings 0 ) \
  > "$W/qmltest.log" 2>&1
qrc=$?
say "qmltest: exit $qrc in $(( $(date +%s) - t0 )) s: $(grep -E '^Totals' "$M/qmltest.txt" 2>/dev/null)"
grep -E '^(FAIL|PASS|SKIP)|QDEBUG|QWARN' "$M/qmltest.txt" 2>/dev/null | cut -c1-300 | head -60 >> "$RUN/run.log"
j ferrata_bridge_module wallet "@$(f="$W/req-eth-report.json"; echo '{"method":"eth_report","params":{}}' > "$f"; echo "$f")" > "$M/eth-report.json" || true
[ -n "$LEZ_MOD" ] && { j ferrata_bridge_module wallet "@$(f="$W/req-lez-report.json"; echo '{"method":"lez_report","params":{}}' > "$f"; echo "$f")" > "$M/lez-report.json" || true; }
j tx_sender_module live_sends > "$M/tx-sender-live-sends.json" || true
records
[ "$qrc" = 0 ] || die "the mini-app's test failed"
say "hook-logoseth: done"
