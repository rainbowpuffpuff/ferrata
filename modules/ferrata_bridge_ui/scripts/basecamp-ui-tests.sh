#!/usr/bin/env bash
# Basecamp's headless UI tests of the mini-app: `nix run .#ui-tests` with Basecamp pinned (the flake is
# read as path:, so untracked files are seen). Retries only the Modal build box's two Nix failures that
# modules/scripts/check.sh names.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
cd "$here"
export LOGOS_BASECAMP="${LOGOS_BASECAMP:-github:logos-co/logos-basecamp/b642a9aba732b80343a3a7b622dad8c65e534bdb}"
if [ "$(ulimit -s)" = unlimited ]; then ulimit -s 1048576; fi
mkdir -p results
# The devnet backend beside Basecamp, if it is built (scripts/box-test.sh builds it): the test file's flow
# test runs only then.
repo="$(cd "$here/../.." && pwd)"
bin="$repo/cli/target/debug/examples/miniapp_devnet"
if [ -x "$bin" ]; then
  export PATH="$HOME/.foundry/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1 RISC0_DEV_MODE=1
  if [ -z "${RISC0_HOME:-}" ] && [ -d "$repo/../risc0-home-lez" ]; then export RISC0_HOME="$(cd "$repo/../risc0-home-lez" && pwd)"; fi
  FERRATA_DEVNET_PORT=18600 "$bin" > results/basecamp-devnet.log 2>&1 &
  devnet=$!
  for _ in $(seq 1 240); do curl -sf http://127.0.0.1:18600/ >/dev/null && break; sleep 0.5; done
  curl -sf http://127.0.0.1:18600/ >/dev/null && export FERRATA_DEVNET=1 && echo "devnet backend up"
fi
for attempt in 1 2 3; do
  if [ "$(id -u)" = 0 ] && [ -d /homeless-shelter ]; then rm -rf /homeless-shelter; fi
  t0=$(date +%s)
  QT_QPA_PLATFORM="${QT_QPA_PLATFORM:-offscreen}" timeout 1500 nix run -L "path:$here#ui-tests" > results/basecamp-ui-tests.txt 2>&1
  code=$?
  echo "ui-tests exit $code after $(( $(date +%s) - t0 )) s (attempt $attempt)"
  grep -q -E 'reading a line: Input/output error|unexpected EOF reading a line|/homeless-shelter. exists' results/basecamp-ui-tests.txt || break
done
grep -E -i 'pass|fail|✓|✗|ok |not ok|error' results/basecamp-ui-tests.txt | tail -n 40
[ -n "${devnet:-}" ] && { curl -s -d '{"method":"shutdown","params":{}}' http://127.0.0.1:18600/rpc >/dev/null; wait $devnet 2>/dev/null; }
sed -i "s|$(cd "$here/../../.." && pwd)/|<spike>/|g" results/basecamp-ui-tests.txt 2>/dev/null  # no machine paths in committed logs
exit $code
