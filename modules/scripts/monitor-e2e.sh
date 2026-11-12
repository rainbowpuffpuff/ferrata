#!/usr/bin/env bash
# The monitor doing its job on the devnet its own test uses: monitor/tests/anvil-e2e.sh (a local anvil
# chain, chain id 31337, nothing sent to any public network; a v5 vault deployed by
# vault/script/MonitorFixture.s.sol; the scenarios S1-S11, A0-A4, R1-R4 and the alerting check).
#
#   modules/scripts/monitor-e2e.sh OUT_DIR                 the monitor binary (the reference)
#   CALL=logosctl LOGOSCTL=BIN SESSION=DIR modules/scripts/monitor-e2e.sh OUT_DIR
#                                                          ferrata_monitor_module in a running Logos Core
#                                                          (monitor-module.sh in place of the binary)
# Needs Foundry (anvil, forge, cast), vault/lib and cargo. Writes OUT_DIR/summary.md (PASS/FAIL lines).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
mkdir -p "$1"
out=$(cd "$1" && pwd)
cd "$repo/monitor"
if [ "${CALL:-native}" = logosctl ]; then
  MONITOR_CMD="$here/monitor-module.sh" MONITOR_E2E_OUT="$out" bash tests/anvil-e2e.sh
else
  MONITOR_E2E_OUT="$out" bash tests/anvil-e2e.sh
fi
