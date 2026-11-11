#!/usr/bin/env bash
# The daemon in dev mode for 45 s: it follows the recorded mainnet data on the in-process LEZ
# chain; /health and /metrics are read while it runs; SIGTERM stops it after the round in hand.
# Run from lez/ (on the build box): RISC0_DEV_MODE=1 updater/dev/demo.sh OUT_DIR
set -u
out=${1:?output directory}
mkdir -p "$out"
rm -rf target/updater-dev-state
cargo build -q -p ferrata-updater --features harness --locked || exit 1
target/debug/ferrata-updater --config updater/dev/dev.toml > "$out/demo-daemon.log" 2>&1 &
pid=$!
sleep 45
curl -s -o "$out/demo-health.json" -w '%{http_code}\n' http://127.0.0.1:9464/health > "$out/demo-health.status"
curl -s http://127.0.0.1:9464/metrics > "$out/demo-metrics.txt"
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:9464/nope > "$out/demo-404.status"
kill -TERM $pid
wait $pid
echo "exit $?" > "$out/demo-exit.txt"
ls target/updater-dev-state/mainnet/ > "$out/demo-state.txt"
wc -l < target/updater-dev-state/mainnet/journal.tsv >> "$out/demo-state.txt"
