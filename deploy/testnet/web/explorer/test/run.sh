#!/usr/bin/env bash
# The explorer bundle's tests, all local (loopback only; nothing leaves this machine):
#   1. decode.test.mjs: the decoders against the recorded LEZ v0.4.0-rc3 run and recorded private transactions;
#      eth.test.mjs: the in-browser proof checks on six recorded attest messages; alarms.test.mjs: the alarm rules;
#   2. fixtures.mjs: the offline test world (those blocks, account states built from real Sepolia data);
#   3. agents/ferrata_check.{py,mjs} and browser_test.py (every page in headless Chromium) against replay_rpc.py
#      serving that world.
#
#   bash deploy/testnet/web/explorer/test/run.sh [PORT]
#
# Needs node (18+), python3 with Playwright and its Chromium. Output: test/out/ (git-ignored), screenshots in
# test/out/shots/. Exits non-zero on the first failing stage.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../../../.." && pwd)"
PORT="${1:-18750}"
cd "$REPO"
mkdir -p "$HERE/out"
node "$HERE/decode.test.mjs"
node "$HERE/eth.test.mjs"
node "$HERE/alarms.test.mjs"
node "$HERE/fixtures.mjs"
python3 -I "$HERE/replay_rpc.py" --port "$PORT" --hold 1 > "$HERE/out/replay.log" 2>&1 &
SERVER=$!
trap 'kill $SERVER 2>/dev/null || true' EXIT
for _ in $(seq 50); do
  curl -sf "http://127.0.0.1:$PORT/config.json" > /dev/null && break
  sleep 0.2
done
# the agents' scripts, as AGENTS.md prints them, against the same world: both must find every check equal
python3 -I "$HERE/../agents/ferrata_check.py" "http://127.0.0.1:$PORT/"
node "$HERE/../agents/ferrata_check.mjs" "http://127.0.0.1:$PORT/"
python3 "$HERE/browser_test.py" --url "http://127.0.0.1:$PORT/"
