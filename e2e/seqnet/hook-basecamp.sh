#!/usr/bin/env bash
# e2e/seqnet's hook for the mini-app INSIDE Logos Basecamp: lez/crown/session.sh x runs it while the zone runs; drive.sh
# has made the vault on anvil. The user's private round trip in the mini-app's view (Main.qml) as Basecamp hosts it,
# with ferrata_bridge_module (and lez_core) loaded in the same Basecamp:
#   1. seqnet-chain: Ethereum for the module (the vault's reads, the user's keystore, the relayer), the zone's history,
#      and the plain LEZ path beside lez_core's: the stand-in for eth_rpc_module, keystore_module and tx_sender_module;
#   2. Logos Basecamp (modules/ferrata_bridge_ui/scripts/basecamp-build.sh's OUT: logos-basecamp-dev at its pin, the
#      view's install root, a --modules-dir root with ferrata_bridge_module's .lgx and lez_core's patched .lgx), in its
#      standalone mode, headless (offscreen), with its QML inspector, running modules/ferrata_bridge_ui/basecamp/
#      roundtrip.mjs through `logos-basecamp-dev --test`: Basecamp's Module Inspector loads the core modules; the view
#      calls ferrata_bridge_module through Basecamp's `logos`; every user step is a click; the operator's and the
#      relayer's steps run between them (ops.sh); after each step the chains are read directly (ops.sh chain); a
#      picture of Basecamp's window at each state.
# BASECAMP_LEZ=lez_core (default: the session's LEZ reads, private holding, private claim and burn through lez_core,
# the module's `wallet lez`) or plain (all through seqnet-chain). BASECAMP_REOPEN=1: the view unloaded and loaded again
# while the burn's job proves (roundtrip.mjs). Records: $RUN/basecamp/.
set -uo pipefail
. "$SEQNET_ENV"
. "$REPO/e2e/seqnet/common.sh"
M="$RUN/basecamp"; mkdir -p "$M"
CHAIN_PORT=18700
BC="${BASECAMP_BUILD:-$(dirname "$T")/basecamp-box/build}"
DEV="$BC/basecamp-dev/bin/logos-basecamp-dev"
[ -x "$DEV" ] && [ -d "$BC/ui-install" ] && [ -d "$BC/modules-root/modules" ] \
  || die "modules/ferrata_bridge_ui/scripts/basecamp-build.sh OUT first (BASECAMP_BUILD=$BC)"
LEZ_ROUTE="${BASECAMP_LEZ:-lez_core}"

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
finish() {
  curl -fsS -m 5 -d '{"method":"module.stop","params":{}}' "http://127.0.0.1:$CHAIN_PORT/" > /dev/null 2>&1 || true
  sleep 1; kill $CHAIN_PID 2>/dev/null || true
}
trap finish EXIT

# 2. Basecamp. Its module processes inherit its environment: dev mode or not, the zone's r0vm for lez_core's proving,
# a short TMPDIR (Unix socket paths hold 107 bytes) and a home of its own.
LT="$BESIDE_SHORT"; mkdir -p "$LT" "$W/basecamp-home"
cp -r "$U/lez-wallet" "$W/lezcore-wallet"
LEZ_PARAMS="$(jq -c -n --arg c "$W/lezcore-wallet/wallet_config.json" --arg s "$W/lezcore-wallet/storage.json" --arg p "$U/lez.pass" \
  --arg b "$BRIDGE_BIN" '{via: "lez_core", wallet: {config: $c, storage: $s, password_file: $p}, program_bin: $b}')"
dmode=(env RISC0_DEV_MODE=1); [ "$PROOFS" = real ] && dmode=(env -u RISC0_DEV_MODE)
wait_ms=180000; [ "$PROOFS" = real ] && wait_ms=1800000
{ cat "$BC/VERSIONS"; echo "route: $LEZ_ROUTE; proofs: $PROOFS; reopen: ${BASECAMP_REOPEN:-0}"; } > "$M/basecamp-versions.txt"
say "Basecamp: $(head -n 1 "$BC/VERSIONS" | cut -c1-80); the round trip in the mini-app (LEZ through $LEZ_ROUTE, proofs $PROOFS)"
t0=$(date +%s)
( cd "$REPO/modules/ferrata_bridge_ui/basecamp" && "${dmode[@]}" RISC0_SERVER_PATH="${RISC0_SERVER_PATH:-}" \
    PATH="${ZONE_BIN:-/nonexistent}:$PATH" TMPDIR="$LT" HOME="$W/basecamp-home" LOGOS_TEST_TIMEOUT=10800 \
    BASECAMP_OUT="$M" BASECAMP_LEZ="$LEZ_ROUTE" BASECAMP_LEZ_PARAMS="$LEZ_PARAMS" BASECAMP_WAIT_MS="$wait_ms" BASECAMP_REOPEN="${BASECAMP_REOPEN:-0}" \
    SEQNET_CHAIN_URL="http://127.0.0.1:$CHAIN_PORT/" SEQNET_OPS="bash $REPO/e2e/seqnet/ops.sh" \
    "$DEV" --module "$BC/ui-install" --modules-dir "$BC/modules-root" --width 1440 --height 1250 \
    --test "$REPO/modules/ferrata_bridge_ui/basecamp/roundtrip.mjs" ) > "$M/basecamp-run.log" 2>&1
brc=$?
say "Basecamp round trip: exit $brc in $(( $(date +%s) - t0 )) s: $(grep -E '[0-9]+ passed, [0-9]+ failed' "$M/basecamp-run.log" | tail -n 1)"
grep -E '✓|✗|check failed|not run:' "$M/basecamp-run.log" | cut -c1-300 | head -40 >> "$RUN/run.log"
# No machine paths in the records (drive.sh scrubs the run's text files for the work, tmp and repository paths).
sp="$(cd "$REPO/.." && pwd)"
find "$M" -type f \( -name '*.log' -o -name '*.json' -o -name '*.jsonl' -o -name '*.txt' \) -exec sed -i -e "s#$LT/#<short-tmp>/#g" \
  -e "s#$BC/#<basecamp-build>/#g" -e "s#$sp/#<spike>/#g" {} +
records
[ "$brc" = 0 ] || die "the round trip in Basecamp failed"
say "hook-basecamp: done"
