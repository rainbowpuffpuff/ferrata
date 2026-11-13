#!/usr/bin/env bash
# e2e/seqnet's hook for the mini-app: lez/crown/session.sh x runs it while the zone runs; drive.sh has made the vault on
# anvil. The user's round trip through the mini-app's view (modules/ferrata_bridge_ui/qml/Main.qml) driving
# ferrata_bridge_module inside a headless Logos Core:
#   1. seqnet-chain: the chains for the module (LEZ's wallet library on this zone's sequencer, the vault's reads and the
#      user's keystore on anvil, the relayer): the stand-in for Logos Core's own modules (lez_core, eth_rpc_module,
#      keystore_module, tx_sender_module), which are not available here;
#   2. a Logos Core daemon (logosctl, built from logos-logoscore-cli) with ferrata_bridge_module installed from its .lgx
#      (logos-module-builder) and loaded;
#   3. the view driven headless by Qt Quick Test (qmltest-logos: tst_miniapp_logos.qml), its `logos` the daemon
#      (logosctl call ferrata_bridge_module wallet ...), the operator's and relayer's steps run by ops.sh between the
#      user's actions; screenshots, the view's calls and the module's replies recorded.
set -uo pipefail
. "$SEQNET_ENV"
. "$REPO/e2e/seqnet/common.sh"
M="$RUN/miniapp"; mkdir -p "$M/screenshots"
CHAIN_PORT=18700
LOGOSCTL="$T/logos/logosctl"; LGX_DIR="$T/logos/lgx-ferrata_bridge_module"; RUNNER="$BIN/ferrata_miniapp_logos"
[ -x "$LOGOSCTL" ] && [ -d "$LGX_DIR" ] && [ -x "$RUNNER" ] || die "run.sh build-miniapp first (logosctl, the module's .lgx, the qmltest runner)"

# 1. The chain service.
mkdir -p "$W/relayer-inbox"
"${PROVE[@]}" "$BIN/seqnet-chain" --listen "127.0.0.1:$CHAIN_PORT" --eth-rpc "$RPC" --chain-id 11155111 --vault "$VAULT" --from-block "$FROM" \
  --keystore "$KEYSTORE" --password-file "$U/eth.pass" --sequencer "$SEQ" --seed "$U/seed.txt" --lez-wallet "$U/lez-wallet" \
  --lez-password-file "$U/lez.pass" --bridge-account "$BRIDGE" --bridge-program "$BRIDGE_BIN" --relayer-bin "$BIN/ferrata-relayer" \
  --relayer "$RELAYER" --relayer-state "$W/relayer" --relayer-inbox "$W/relayer-inbox" --mock "$MOCK" --prover "$DEPLOYER" \
  --release-image-id "$RELEASE_ID" --log "$M/chain-calls.jsonl" --proofs "$PROOFS" > "$W/seqnet-chain.log" 2>&1 &
CHAIN_PID=$!
for _ in $(seq 120); do curl -fsS -m 2 -d '{"method":"module.info","params":{}}' "http://127.0.0.1:$CHAIN_PORT/" > /dev/null 2>&1 && break; sleep 1; done
kill -0 $CHAIN_PID 2>/dev/null || { tail -n 20 "$W/seqnet-chain.log"; die "seqnet-chain did not start"; }
say "seqnet-chain: $(head -n 1 "$W/seqnet-chain.log")"

# 2. Logos Core with the module (its sockets in a short TMPDIR: a Unix socket path holds 107 bytes).
LT="$BESIDE_SHORT"; mkdir -p "$LT"
S="$W/logos-session"; rm -rf "$S"; mkdir -p "$W/logos-home"
ctl() { TMPDIR="$LT" "$LOGOSCTL" --config-dir "$S" "$@"; }
dmode=(env RISC0_DEV_MODE=1); [ "$PROOFS" = real ] && dmode=(env -u RISC0_DEV_MODE)
"${dmode[@]}" TMPDIR="$LT" HOME="$W/logos-home" "$LOGOSCTL" --config-dir "$S" daemon start --detach > "$M/logos-daemon.txt" 2>&1 \
  || { cat "$M/logos-daemon.txt"; die "logosctl daemon start"; }
finish() {
  ctl daemon stop >> "$M/logos-daemon.txt" 2>&1 || true
  tail -n 40 "$S"/logs/daemon_*.log > "$M/logos-daemon-log-tail.txt" 2>/dev/null || true
  curl -fsS -m 5 -d '{"method":"module.stop","params":{}}' "http://127.0.0.1:$CHAIN_PORT/" > /dev/null 2>&1 || true
  sleep 1; kill $CHAIN_PID 2>/dev/null || true
}
trap finish EXIT
ctl install "$LGX_DIR"/*.lgx -y >> "$M/logos-daemon.txt" 2>&1 || die "logosctl install"
ctl module load ferrata_bridge_module >> "$M/logos-daemon.txt" 2>&1 || die "logosctl module load"
ctl call ferrata_bridge_module version --json < /dev/null > "$M/module-version.json" 2>&1 || die "the module's version"
say "Logos Core: ferrata_bridge_module installed from $(ls "$LGX_DIR"/*.lgx | xargs -n1 basename) and loaded"

# 3. The view, headless.
say "the mini-app: Qt Quick Test (offscreen), the view's calls to ferrata_bridge_module in Logos Core"
t0=$(date +%s)
( cd "$REPO/modules/ferrata_bridge_ui/qmltest-logos" && env QT_QPA_PLATFORM=offscreen LOGOSCTL="$LOGOSCTL" LOGOS_SESSION="$S" \
    TMPDIR="$LT" SEQNET_TMP="$W" SEQNET_CALL_LOG="$M/calls.jsonl" SEQNET_OPS="bash $REPO/e2e/seqnet/ops.sh" \
    SEQNET_OPS_LOG="$M/ops.jsonl" SEQNET_CHAIN_URL="http://127.0.0.1:$CHAIN_PORT/" SEQNET_SHOTS="$M/screenshots" \
    "$RUNNER" -input tst_miniapp_logos.qml -o "$M/qmltest.txt,txt" -o "$M/qmltest.xml,xunitxml" -maxwarnings 0 ) \
  > "$W/qmltest.log" 2>&1
qrc=$?
say "qmltest: exit $qrc in $(( $(date +%s) - t0 )) s: $(grep -E '^Totals' "$M/qmltest.txt" 2>/dev/null)"
grep -E '^(FAIL|PASS)|QDEBUG|QWARN' "$M/qmltest.txt" 2>/dev/null | cut -c1-300 | head -40 >> "$RUN/run.log"
records
[ "$qrc" = 0 ] || die "the mini-app's test failed"
say "hook-miniapp: done"
