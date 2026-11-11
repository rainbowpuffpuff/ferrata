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
for attempt in 1 2 3; do
  if [ "$(id -u)" = 0 ] && [ -d /homeless-shelter ]; then rm -rf /homeless-shelter; fi
  t0=$(date +%s)
  QT_QPA_PLATFORM="${QT_QPA_PLATFORM:-offscreen}" timeout 1500 nix run -L "path:$here#ui-tests" > results/basecamp-ui-tests.txt 2>&1
  code=$?
  echo "ui-tests exit $code after $(( $(date +%s) - t0 )) s (attempt $attempt)"
  grep -q -E 'reading a line: Input/output error|unexpected EOF reading a line|/homeless-shelter. exists' results/basecamp-ui-tests.txt || break
done
grep -E -i 'pass|fail|✓|✗|ok |not ok|error' results/basecamp-ui-tests.txt | tail -n 40
exit $code
