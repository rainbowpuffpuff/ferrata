# Sourced by box.sh (`box.sh metamask [NAME]`): the Bridge tab's deposit with the REAL MetaMask extension (pinned release,
# hash checked) in Chromium, against anvil (chain id 31337, loopback) with FerrataVault deployed as e2e.sh does. MetaMask
# imports anvil's published test mnemonic ("test test ... junk"), so its first account is anvil's funded account 0.
NAME="${1:-metamask-$(date -u +%Y-%m-%d)}"
OUT="$WALLET/test/results/$NAME"; SHOTS="${TMPDIR:-}/metamask-wallet/e2e-$NAME"
[ -n "${TMPDIR:-}" ] || die "set TMPDIR: the run's screenshots and logs go to \$TMPDIR/metamask-wallet"
rm -rf "$OUT" "$SHOTS"; mkdir -p "$OUT" "$SHOTS" "$WORK/run/logs"
MM_VER="${MM_VER:-13.58.0}"
case "$MM_VER" in   # GitHub's digests of the release assets (metamask-chrome-<ver>.zip)
  13.58.0) MM_SHA=35d1594d25e8eb0fc51f9a08ffcc3c8bf18965ce2e7d9026e3022a530a57f215 ;;
  13.57.0) MM_SHA=a718727215a32d18375470d71fd5e89fe2da997ab144c05f8d43654d96f00b69 ;;
  *) die "no pinned hash for MetaMask $MM_VER" ;;
esac
MM_ZIP="$WORK/mm-download/metamask-chrome-$MM_VER.zip"; MM_DIR="$WORK/mm-$MM_VER"
if [ ! -f "$MM_DIR/manifest.json" ]; then
  rm -rf "$WORK/mm-download" "$MM_DIR"; mkdir -p "$WORK/mm-download" "$MM_DIR"
  curl -fsSL -m 300 -o "$MM_ZIP" "https://github.com/MetaMask/metamask-extension/releases/download/v$MM_VER/metamask-chrome-$MM_VER.zip"
  echo "$MM_SHA  $MM_ZIP" | sha256sum -c - || die "the MetaMask download does not have the release's hash"
  python3 -I -c 'import sys, zipfile; zipfile.ZipFile(sys.argv[1]).extractall(sys.argv[2])' "$MM_ZIP" "$MM_DIR"
fi
[ -f "$MM_DIR/manifest.json" ] || die "no manifest.json in the MetaMask release"
ANVIL_PORT=8545; ANVIL_URL="http://127.0.0.1:$ANVIL_PORT"
anvil --host 127.0.0.1 --port "$ANVIL_PORT" --chain-id 31337 --code-size-limit 100000 > "$WORK/run/logs/anvil.log" 2>&1 &
ANVIL_PID=$!
trap stop_all EXIT
ejson() { curl -sS -m 5 -H 'Content-Type: application/json' --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$1\",\"params\":${2:-[]}}" "$ANVIL_URL"; }
for _ in $(seq 1 100); do ejson eth_chainId 2>/dev/null | grep -q 0x7a69 && break; sleep 0.2; done
ACCTS="$(ejson eth_accounts)"
DEPOSITOR="$(printf '%s' "$ACCTS" | python3 -I -c 'import json,sys; print(json.load(sys.stdin)["result"][0])')"
SENDER="$(printf '%s' "$ACCTS" | python3 -I -c 'import json,sys; print(json.load(sys.stdin)["result"][1])')"
( cd "$REPO/vault" && forge build --offline > "$WORK/run/logs/forge-build.log" 2>&1 ) || { tail -20 "$WORK/run/logs/forge-build.log"; die "forge build failed"; }
( cd "$REPO/vault" && WEB_DEPOSITOR_ACCOUNT="$DEPOSITOR" forge script script/WebDepositorLocal.s.sol --rpc-url "$ANVIL_URL" \
    --broadcast --unlocked --sender "$SENDER" --offline --slow > "$WORK/run/logs/forge-deploy.log" 2>&1 ) || { tail -20 "$WORK/run/logs/forge-deploy.log"; die "vault deployment failed"; }
VAULT="$(grep -Eo 'FERRATA_WEB_VAULT 0x[0-9a-fA-F]{40}' "$WORK/run/logs/forge-deploy.log" | head -1 | awk '{print $2}')"
[ -n "$VAULT" ] || die "no vault address in the deployment log"
CONFIG="$WORK/run/config-mm.json"
python3 -I - "$WALLET/config.json" "$CONFIG" "$VAULT" "$ANVIL_URL" <<'PY'
import json, sys
src, dst, vault, rpc = sys.argv[1:5]
c = json.load(open(src))
c["bridge"].update({"vault": vault, "evmChainId": 31337, "evmRpc": rpc, "evmChainName": "Anvil (local test chain)"})
c["network"] = "local test (MetaMask over anvil)"
json.dump(c, open(dst, "w"), indent=1)
PY
PORT=18650
python3 -I "$HERE/serve.py" --port "$PORT" --config "$CONFIG" --rpc "http://127.0.0.1:1" > "$WORK/run/logs/serve-mm.log" 2>&1 &
SERVE_PID=$!
for _ in $(seq 1 50); do curl -fsS -m 2 "http://127.0.0.1:$PORT/" > /dev/null 2>&1 && break; sleep 0.2; done
# Chromium puts its singleton socket under TMPDIR; a Unix socket path is at most 107 bytes, which this tree's scratch
# path exceeds (Chromium aborts: "Socket path too long"). A short link in tmp/ points at a directory in our scratch.
mkdir -p "$WORK/chrome-tmp"; ln -sfn "$WORK/chrome-tmp" "$BESIDE/tmp/twc"
RUNNER=()
if [ "${MM_HEADED:-1}" = 1 ] && command -v xvfb-run > /dev/null; then RUNNER=(xvfb-run -a -s "-screen 0 1280x960x24"); fi
set +e
TMPDIR="$BESIDE/tmp/twc" MM_HEADED="${MM_HEADED:-1}" MM_MODE="${MM_MODE:-home}" MM_EXT="$MM_DIR" MM_VERSION="$MM_VER" WALLET_URL="http://127.0.0.1:$PORT/" WALLET_ANVIL="$ANVIL_URL" WALLET_VAULT="$VAULT" \
  WALLET_OUT="$OUT" WALLET_SHOTS="$SHOTS" WALLET_PROFILE="$WORK/mm-profile-$NAME" \
  "${RUNNER[@]}" ${WALLET_PY:-python3 -I} "$HERE/metamask_test.py" 2>&1 | tee "$WORK/run/logs/metamask.log"
rc=${PIPESTATUS[0]}
set -e
grep -c 'eth_sendRawTransaction' "$WORK/run/logs/anvil.log" > "$OUT/anvil-raw-tx-count.txt" || true
cp "$WORK/run/logs/anvil.log" "$WORK/run/logs/metamask.log" "$SHOTS/" 2>/dev/null || true
echo "MetaMask $MM_VER (metamask-chrome-$MM_VER.zip, sha256 $MM_SHA)" > "$OUT/metamask.txt"
echo "== metamask exit $rc; record $OUT"
exit "$rc"
