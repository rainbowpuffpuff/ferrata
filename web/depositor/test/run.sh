#!/usr/bin/env bash
# web/depositor: the full browser test of the deposit page.
#
#   bash web/depositor/test/run.sh
#
# Starts a local anvil (chain id 11155111, port 18645, loopback only), builds the vault once, deploys
# script/WebDepositorLocal.s.sol (the CLI e2e vault plus two registered test tokens, an ERC-20 and a
# Tether-style one, minted to anvil's account 1), serves the page with web/depositor/serve.py on
# http://127.0.0.1:18646/ (as the README says to open it), then runs depositor_test.py: headless Chromium, an
# injected EIP-1193 wallet that forwards to anvil. The run's output goes to
# web/depositor/results/anvil-<the run's UTC date>.txt and one screenshot of the summary to
# web/depositor/results/anvil-<date>.png. Exits non-zero on any failure; anvil and the server are stopped on
# exit. No network request leaves this machine: foundry talks only to the loopback anvil, the browser only to
# the loopback server, and the page only to the injected wallet.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
PORT=18645
CHAIN_ID=11155111
RPC="http://127.0.0.1:$PORT"
PAGE_PORT=18646
RESULTS="$ROOT/web/depositor/results"
DATE="$(date -u +%Y-%m-%d)"
RECORD="$RESULTS/anvil-$DATE.txt"
ANVIL_LOG="$RESULTS/anvil-$PORT.log"
DEPLOY_LOG="$RESULTS/deploy-$PORT.log"

command -v anvil >/dev/null || { echo "anvil (foundry) is not in PATH" >&2; exit 1; }
command -v forge >/dev/null || { echo "forge (foundry) is not in PATH" >&2; exit 1; }
command -v python3 >/dev/null || { echo "python3 is not in PATH (the test needs Playwright for python)" >&2; exit 1; }
python3 -c 'from playwright.sync_api import sync_playwright' 2>/dev/null \
  || { echo "python3 cannot import playwright (pip install playwright; playwright install chromium)" >&2; exit 1; }

mkdir -p "$RESULTS"

# ---- anvil, local only; the code-size limit is the one the CLI's e2e test uses (the linked vault is large)
anvil --port "$PORT" --chain-id "$CHAIN_ID" --code-size-limit 100000 >"$ANVIL_LOG" 2>&1 &
ANVIL_PID=$!
SERVE_PID=""
cleanup() {
  kill "$ANVIL_PID" 2>/dev/null || true; wait "$ANVIL_PID" 2>/dev/null || true
  if [ -n "$SERVE_PID" ]; then kill "$SERVE_PID" 2>/dev/null || true; wait "$SERVE_PID" 2>/dev/null || true; fi
}
trap cleanup EXIT

jrpc() { curl -s -H 'Content-Type: application/json' -d "$1" "$RPC"; }
jfield() { jrpc "$1" | python3 -c 'import json,sys
try:
    j = json.load(sys.stdin)
    v = j["result"] if "result" in j else "MISSING"
    print(v if isinstance(v, str) else json.dumps(v))
except Exception:
    print("MISSING")'; }
for _ in $(seq 1 100); do
  if [ "$(jfield '{"jsonrpc":"2.0","id":1,"method":"eth_chainId","params":[]}')" = "0xaa36a7" ]; then
    break
  fi
  sleep 0.2
done
echo "anvil: chain id $CHAIN_ID on $RPC"

ACCOUNTS="$(jfield '{"jsonrpc":"2.0","id":1,"method":"eth_accounts","params":[]}')"
SENDER="$(printf '%s' "$ACCOUNTS" | python3 -c 'import json,sys; print(json.load(sys.stdin)[0])')"
DEPOSITOR="$(printf '%s' "$ACCOUNTS" | python3 -c 'import json,sys; print(json.load(sys.stdin)[1])')"
echo "anvil: deployer $SENDER, the browser's wallet account $DEPOSITOR"

# ---- the vault: build once, then deploy the CLI e2e vault plus the registered test token
echo "forge: building the vault (cached after the first run)"
(cd "$ROOT/vault" && nice -n 19 forge build --offline >"$RESULTS/build-$PORT.log" 2>&1) \
  || { echo "forge build failed; see $RESULTS/build-$PORT.log" >&2; exit 1; }

echo "forge script: deploying script/WebDepositorLocal.s.sol"
(cd "$ROOT/vault" && WEB_DEPOSITOR_ACCOUNT="$DEPOSITOR" nice -n 19 forge script script/WebDepositorLocal.s.sol \
  --rpc-url "$RPC" --broadcast --unlocked --sender "$SENDER" --offline --slow) >"$DEPLOY_LOG" 2>&1 \
  || { echo "the deployment failed; see $DEPLOY_LOG" >&2; exit 1; }
# forge prints the console lines of the script indented under "== Logs =="; pick the labelled addresses.
deployed_label() { grep -Eo "$1 0x[0-9a-fA-F]{40}" "$DEPLOY_LOG" | head -1 | awk '{print $2}' || true; }
VAULT="$(deployed_label FERRATA_WEB_VAULT)"
TOKEN="$(deployed_label FERRATA_WEB_TOKEN)"
TETHER="$(deployed_label FERRATA_WEB_TETHER)"
[ -n "$VAULT" ] && [ -n "$TOKEN" ] && [ -n "$TETHER" ] || { echo "could not read the deployed addresses from $DEPLOY_LOG" >&2; exit 1; }
CODE="$(jfield "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"eth_getCode\",\"params\":[\"$VAULT\",\"latest\"]}")"
[ "$CODE" != "0x" ] && [ "$CODE" != "MISSING" ] || { echo "the vault $VAULT has no code on anvil" >&2; exit 1; }
echo "deployed: vault $VAULT, test token $TOKEN, Tether-style token $TETHER"

# ---- the page, served as the README says (127.0.0.1 only)
SERVE_LOG="$RESULTS/serve-$PAGE_PORT.log"
python3 -I "$ROOT/web/depositor/serve.py" --port "$PAGE_PORT" >"$SERVE_LOG" 2>&1 &
SERVE_PID=$!
PAGE_URL="http://127.0.0.1:$PAGE_PORT/"
for _ in $(seq 1 50); do
  curl -s -o /dev/null "$PAGE_URL" && break
  sleep 0.2
done
curl -s -o /dev/null "$PAGE_URL" || { echo "serve.py did not answer; see $SERVE_LOG" >&2; exit 1; }
echo "serve.py: $(head -1 "$SERVE_LOG")"

# ---- the browser test
echo
FERRATA_RPC="$RPC" FERRATA_VAULT="$VAULT" FERRATA_TOKEN="$TOKEN" FERRATA_TETHER="$TETHER" FERRATA_ACCOUNT="$DEPOSITOR" \
  FERRATA_PAGE_URL="$PAGE_URL" FERRATA_RECORD_DATE="$DATE" \
  python3 "$ROOT/web/depositor/test/depositor_test.py"

# ---- keep the scratch logs only when something failed
rm -f "$ANVIL_LOG" "$DEPLOY_LOG" "$RESULTS/build-$PORT.log" "$SERVE_LOG"
echo
echo "ok: every case passed; the record is $RECORD"
