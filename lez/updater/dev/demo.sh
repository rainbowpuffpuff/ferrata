#!/usr/bin/env bash
# The daemon in dev mode for 45 s: it follows the recorded mainnet data on the in-process LEZ
# chain; /health and /metrics are read while it runs; SIGTERM stops it after the round in hand.
# Run from lez/ (on the build box): RISC0_DEV_MODE=1 updater/dev/demo.sh OUT_DIR
set -euo pipefail
out=${1:?output directory}
mkdir -p "$out"
rm -rf target/updater-dev-state
cargo build -q -p ferrata-updater --features harness --locked
target/debug/ferrata-updater --config updater/dev/dev.toml > "$out/demo-daemon.log" 2>&1 &
pid=$!
cleanup() {
  if [ -n "$pid" ]; then
    kill -TERM "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  fi
}
trap cleanup EXIT
sleep 45
curl -fsS -o "$out/demo-health.json" -w '%{http_code}\n' http://127.0.0.1:9464/health > "$out/demo-health.status"
grep -q '"status": "ok"' "$out/demo-health.json"
curl -fsS http://127.0.0.1:9464/metrics > "$out/demo-metrics.txt"
grep -q '^ferrata_updater_rounds_total' "$out/demo-metrics.txt"
curl -sS -o /dev/null -w '%{http_code}\n' http://127.0.0.1:9464/nope > "$out/demo-404.status"
grep -qx 404 "$out/demo-404.status"
kill -TERM "$pid"
rc=0
wait "$pid" || rc=$?
pid=
echo "exit $rc" > "$out/demo-exit.txt"
[ "$rc" = 0 ]
ls target/updater-dev-state/mainnet/ > "$out/demo-state.txt"
wc -l < target/updater-dev-state/mainnet/journal.tsv >> "$out/demo-state.txt"
echo "demo: /health 200 ok; /metrics present; unknown route 404; SIGTERM exit 0; journal $(wc -l < target/updater-dev-state/mainnet/journal.tsv) lines"
