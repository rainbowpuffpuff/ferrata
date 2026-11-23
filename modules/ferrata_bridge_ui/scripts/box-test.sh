#!/usr/bin/env bash
# The mini-app's headless test on a Linux box (the Modal build box): builds the devnet backend
# (cli/examples/miniapp_devnet.rs), starts it (anvil, chain id 1, local only; the LEZ harness in RISC Zero
# dev mode), checks its methods with curl, drives the QML view through every screen with Qt Quick Test
# (offscreen, software rendering) and stops it. Results: modules/ferrata_bridge_ui/results/.
# FERRATA_DEVNET_PROOFS=real: the devnet proves its private transactions for real (r0vm, CPU; minutes
# each); results then go to results-real-proofs/ and results/ is left as it was.
# Needs: Foundry (anvil, forge) and vault/lib (cli/tests/e2e.sh installs it), RISC Zero's Rust 1.91.1 in
# RISC0_HOME (lez/RESULTS.md §8), Qt 6 with qmltestrunner (QT_BIN), curl and jq.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$here/../.." && pwd)"
out="$here/results"
QT_BIN="${QT_BIN:-/usr/lib/qt6/bin}"
[ -x "$QT_BIN/qmltestrunner" ] || { echo "no $QT_BIN/qmltestrunner: install Qt 6's QML test runner and modules (README, \"Tests\") or set QT_BIN" >&2; exit 1; }
export PATH="$HOME/.foundry/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1 RISC0_DEV_MODE=1
if [ -z "${RISC0_HOME:-}" ] && [ -d "$repo/../risc0-home-lez" ]; then export RISC0_HOME="$(cd "$repo/../risc0-home-lez" && pwd)"; fi
final="$out"
if [ "${FERRATA_DEVNET_PROOFS:-}" = real ]; then
  final="$here/results-real-proofs"
  rm -rf "$here/results.keep"; [ -d "$out" ] && mv "$out" "$here/results.keep"
  if [ -z "${RISC0_SERVER_PATH:-}" ]; then
    RISC0_SERVER_PATH="$(command -v r0vm || find "${RISC0_HOME:-$HOME/.risc0}" "$HOME/.risc0" -name r0vm -type f 2>/dev/null | head -n 1)"
    export RISC0_SERVER_PATH
  fi
  echo "real proofs: r0vm $("$RISC0_SERVER_PATH" --version 2>&1 | head -n 1), $(nproc) CPUs"
fi
devdir="$repo/../tmp/miniapp-private/devnet"
# results/ also holds basecamp-ui-tests.sh's two logs: replace only what this test writes.
rm -rf "$out/screenshots"
rm -f "$out"/{backend-contract.txt,calls.jsonl,calls-summary.jsonl,devnet.log,qmltest.txt,qmltest.xml}
mkdir -p "$out/screenshots"
port=18600
t0=$(date +%s)
(cd "$repo/vault" && { [ -d lib/risc0-ethereum ] || forge install --no-git foundry-rs/forge-std@v1.10.0 OpenZeppelin/openzeppelin-contracts@v5.4.0 risc0/risc0-ethereum@v3.1.0; } && forge build --quiet) || { echo "FAIL vault build"; exit 1; }
(cd "$repo/cli" && cargo build --example miniapp_devnet --offline 2>&1 | tail -n 3) || true
bin="$repo/cli/target/debug/examples/miniapp_devnet"
[ -x "$bin" ] || { echo "FAIL: no devnet binary"; exit 1; }
echo "built in $(( $(date +%s) - t0 )) s"
FERRATA_DEVNET_DIR="$devdir" FERRATA_DEVNET_PORT=$port "$bin" > "$out/devnet.log" 2>&1 &
devnet=$!
for _ in $(seq 1 240); do curl -sf "http://127.0.0.1:$port/" >/dev/null && break; sleep 0.5; done
curl -sf "http://127.0.0.1:$port/" >/dev/null || { echo "FAIL: devnet did not start"; tail -n 20 "$out/devnet.log"; exit 1; }
echo "devnet up after $(( $(date +%s) - t0 )) s"
rpc() { curl -s -d "{\"method\":\"$1\",\"params\":${2:-{\}}}" "http://127.0.0.1:$port/rpc"; }
# The backend's contract, method by method, before the UI uses it.
{
  rpc info | jq -c '{ok, chain: .result.chain_id, vault: .result.vault}'
  rpc registry | jq -c '{ok, denominations: [.result.tokens[0].denominations[].amount], lez: (.result.tokens[0].lez != null)}'
  rpc price '{"denomination":0}' | jq -c '{ok, total: .result.total}'
} > "$out/backend-contract.txt" 2>&1
cat "$out/backend-contract.txt"
cd "$here"
# Qt's runtime directory: one of the test's own (0700), so that Qt does not name a default one in the log.
xdg="$devdir/xdg-runtime"; mkdir -p "$xdg" && chmod 700 "$xdg"
XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-$xdg}" QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software QML_IMPORT_PATH= \
  timeout "${QT_TIMEOUT:-900}" "$QT_BIN/qmltestrunner" -input qmltest/tst_miniapp.qml -o "$out/qmltest.xml,junitxml" -o "-,txt" > "$out/qmltest.txt" 2>&1
qt=$?
echo "qmltestrunner exit $qt after $(( $(date +%s) - t0 )) s"
grep -E '^(PASS|FAIL|XFAIL|SKIP|Totals)|Actual|Expected|Loc:|QWARN|qml:' "$out/qmltest.txt" | head -80
rpc shutdown >/dev/null; wait $devnet 2>/dev/null
cp "$devdir/calls.jsonl" "$out/calls.jsonl" 2>/dev/null || true
jq -c '{method, ms, ok: .reply.ok, error: .reply.error}' "$out/calls.jsonl" > "$out/calls-summary.jsonl" 2>/dev/null
ls "$out/screenshots" | wc -l | xargs echo screenshots:
# No machine paths and no scratch folder names in committed logs.
sed -i -E "s|$(cd "$here/../../.." && pwd)/|<spike>/|g; s|<spike>/tmp/[^/' \"]+/|<scratch>/|g" "$out/qmltest.txt" "$out/qmltest.xml" "$out/devnet.log" 2>/dev/null
if [ "$final" != "$out" ]; then
  rm -rf "$final"; mv "$out" "$final"
  [ -d "$here/results.keep" ] && mv "$here/results.keep" "$out"
fi
exit $qt
