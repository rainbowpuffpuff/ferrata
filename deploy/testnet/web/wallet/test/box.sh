#!/usr/bin/env bash
# The web wallet's local test network, on a build machine (the Modal build box through mrun, or any Linux host with
# LEZ's build dependencies). Nothing here reaches a public network: the sequencer is LEZ v0.4.0-rc3's own, in its
# standalone mode (its in-process mock Bedrock), on loopback.
#
#   box.sh build           LEZ rc3's sequencer_service (feature `standalone`) and LEZ's wallet CLI, from the unmodified tag
#   box.sh probe           start the sequencer, record its RPC answers and a transfer signed by LEZ's own wallet CLI
#   box.sh e2e [NAME]      start the sequencer and test/serve.py, run the Playwright test (test/e2e_test.py), stop both
#
# Layout: LEZ at upstream/lez-v0.4.0-rc3 beside the repository (LEZ_SRC overrides); scratch in target/wallet-box
# inside the repository (git-ignored; WALLET_WORK overrides). Everything a command starts is stopped before it returns.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WALLET="$(cd "$HERE/.." && pwd)"
REPO="$(cd "$WALLET/../../../.." && pwd)"
BESIDE="$(cd "$REPO/.." && pwd)"
LEZ_SRC="${LEZ_SRC:-$BESIDE/upstream/lez-v0.4.0-rc3}"
LEZ_COMMIT=bafd07be0701398f9bc435f73ab05e9a7936c301
WORK="${WALLET_WORK:-$REPO/target/wallet-box}"
TARGET="$WORK/target"
BIN="$WORK/bin"
RUN="$WORK/run"
RPC_PORT="${WALLET_RPC_PORT:-3040}"
BEDROCK_FUNDING_PUBLIC_KEY=2e03b2eff5a45478e7e79668d2a146cf2c5c7925bce927f2b1c67f2ab4fc0d26   # LEZ's Justfile value
export CARGO_BUILD_JOBS="${WALLET_JOBS:-16}"
export TMPDIR="$WORK/tmpdir"
mkdir -p "$BIN" "$TMPDIR"
unset GITHUB_TOKEN GH_TOKEN
die() { echo "error: $*" >&2; exit 1; }

check_lez() {
  [ -f "$LEZ_SRC/Cargo.toml" ] || die "LEZ v0.4.0-rc3 not found at $LEZ_SRC"
  if [ -d "$LEZ_SRC/.git" ]; then
    [ "$(git -C "$LEZ_SRC" rev-parse HEAD)" = "$LEZ_COMMIT" ] || die "$LEZ_SRC is not at v0.4.0-rc3 ($LEZ_COMMIT)"
  fi
}

build() {
  check_lez
  ( cd "$LEZ_SRC" && CARGO_TARGET_DIR="$TARGET" cargo build --release --locked -p sequencer_service --features standalone )
  cp "$TARGET/release/sequencer_service" "$BIN/sequencer_service_standalone"
  ( cd "$LEZ_SRC" && CARGO_TARGET_DIR="$TARGET" cargo build --release --locked -p wallet )
  cp "$TARGET/release/wallet" "$BIN/wallet"
  ( cd "$LEZ_SRC" && rustc --version && cargo --version ) > "$BIN/toolchain.txt"
  ( cd "$BIN" && sha256sum sequencer_service_standalone wallet > SHA256SUMS && cat SHA256SUMS )
}

rpc() { # method, params (JSON)
  curl -sS -m 10 -H 'Content-Type: application/json' \
    --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$1\",\"params\":${2:-[]}}" "http://127.0.0.1:$RPC_PORT"
}

SEQ_PID=""
stop_all() {
  [ -n "${SERVE_PID:-}" ] && kill "$SERVE_PID" 2>/dev/null || true
  if [ -n "$SEQ_PID" ]; then kill -TERM "$SEQ_PID" 2>/dev/null || true; sleep 1; kill -KILL "$SEQ_PID" 2>/dev/null || true; fi
}

start_sequencer() { # BLOCK_TIME (e.g. 2s)
  [ -x "$BIN/sequencer_service_standalone" ] || die "build first"
  rm -rf "$RUN"; mkdir -p "$RUN/logs"
  python3 -I - "$LEZ_SRC/lez/sequencer/service/networks/local.json" "$RUN/sequencer_config.json" "${1:-}" <<'EOF'
import json, sys
src, dst, block_time = sys.argv[1:4]
c = json.load(open(src))
c["gossip"]["listen_addr"] = "/ip4/127.0.0.1/udp/0/quic-v1"
c["metrics_listen_address"] = "127.0.0.1:9000"
if block_time:
    c["block_create_timeout"] = block_time
json.dump(c, open(dst, "w"), indent=2)
EOF
  "$BIN/sequencer_service_standalone" setup "$RUN/sequencer_home" --config "$RUN/sequencer_config.json" \
    --funding-public-key "$BEDROCK_FUNDING_PUBLIC_KEY" --create-channel > "$RUN/logs/setup.log" 2>&1 \
    || { tail -30 "$RUN/logs/setup.log"; die "sequencer setup failed"; }
  env -u RISC0_DEV_MODE RUST_LOG="info,risc0_zkvm=warn,kameo=warn" \
    "$BIN/sequencer_service_standalone" start "$RUN/sequencer_home" --rpc-listen-address "127.0.0.1:$RPC_PORT" \
    > "$RUN/logs/sequencer.log" 2>&1 &
  SEQ_PID=$!
  trap stop_all EXIT
  local i head=""
  for i in $(seq 1 180); do
    head="$(rpc getLastBlockId 2>/dev/null | sed -n 's/.*"result":\([0-9]*\).*/\1/p')" || true
    [ -n "$head" ] && [ "$head" -ge 1 ] && break
    kill -0 "$SEQ_PID" 2>/dev/null || { tail -40 "$RUN/logs/sequencer.log"; die "the sequencer exited"; }
    sleep 1
  done
  [ -n "$head" ] || { tail -40 "$RUN/logs/sequencer.log"; die "the sequencer's RPC did not answer"; }
  echo "== sequencer up: head $head, pid $SEQ_PID, RPC 127.0.0.1:$RPC_PORT"
}

# LEZ's CLI wallet, its home in the run directory, pointed at the local sequencer; LEZ's published local test
# accounts come with a new wallet's storage.
cli_wallet() {
  export LEE_WALLET_HOME_DIR="$RUN/cli-wallet" LEE_WALLET_PASSWORD="local-test-only"
  if [ ! -f "$LEE_WALLET_HOME_DIR/wallet_config.json" ]; then
    mkdir -p "$LEE_WALLET_HOME_DIR"
    sed "s#http://127.0.0.1:3040#http://127.0.0.1:$RPC_PORT#" "$LEZ_SRC/lez/wallet/configs/debug/wallet_config.json" \
      > "$LEE_WALLET_HOME_DIR/wallet_config.json"
  fi
  "$BIN/wallet" "$@"
}

probe() {
  local OUT="$WORK/probe"
  rm -rf "$OUT"; mkdir -p "$OUT"
  start_sequencer "${WALLET_BLOCK_TIME:-2s}"
  cp "$RUN/sequencer_config.json" "$OUT/"
  for m in checkHealth getLastBlockId getFeeState getProgramIds getChannelId getCrossZoneDeadLetters; do
    rpc "$m" > "$OUT/$m.json" 2>&1 || true
  done
  rpc getBlock '[1]' > "$OUT/getBlock-1.json"; rpc getBlock '[2]' > "$OUT/getBlock-2.json"
  rpc getBlockRange '[1,4]' > "$OUT/getBlockRange-1-4.json"
  { echo "== account list"; cli_wallet account list 2>&1; echo "exit $?"; } > "$OUT/cli-account-list.txt" 2>&1 || true
  { echo "== help"; cli_wallet auth-transfer send --help 2>&1; } > "$OUT/cli-send-help.txt" 2>&1 || true
  cat "$OUT/cli-account-list.txt" | head -20
  # The first public test account of the list, and a fresh key's public key (from LEZ's own CLI).
  local A; A="$(grep -o -E 'Public/[1-9A-HJ-NP-Za-km-z]{32,44}' "$OUT/cli-account-list.txt" | head -1 | cut -d/ -f2)" || true
  echo "A=$A"
  if [ -n "$A" ]; then
    rpc getAccountBalance "[\"$A\"]" > "$OUT/getAccountBalance-A.json"
    rpc getAccount "[\"$A\"]" > "$OUT/getAccount-A.json"
    rpc getAccountsNonces "[[\"$A\"]]" > "$OUT/getAccountsNonces-A.json"
    rpc getAccountView "[{\"account_id\":\"$A\",\"program_account_id\":\"11111111111111111111111111111111\"}]" > "$OUT/getAccountView-A.json"
    { cli_wallet account new public 2>&1; echo "exit $?"; } > "$OUT/cli-new-public.txt" 2>&1 || true
    { cli_wallet account list 2>&1; } > "$OUT/cli-account-list-2.txt" 2>&1 || true
    local B; B="$(grep -o -E 'Public/[1-9A-HJ-NP-Za-km-z]{32,44}' "$OUT/cli-new-public.txt" | head -1 | cut -d/ -f2)" || true
    echo "B=$B"
    { cli_wallet account show-keys --account-id "Public/$B" 2>&1; echo "exit $?"; } > "$OUT/cli-show-keys-B.txt" 2>&1 || true
    { cli_wallet --dry-run auth-transfer send --from "Public/$A" --to "Public/$B" --amount 1000 2>&1; echo "exit $?"; } > "$OUT/cli-dry-run.txt" 2>&1 || true
    { cli_wallet auth-transfer send --from "Public/$A" --to "Public/$B" --amount 1000 2>&1; echo "exit $?"; } > "$OUT/cli-send.txt" 2>&1 || true
    local H; H="$(grep -o -E '[0-9a-f]{64}' "$OUT/cli-send.txt" | head -1)" || true
    echo "H=$H"
    sleep 6
    [ -n "$H" ] && rpc getTransaction "[\"$H\"]" > "$OUT/getTransaction.json"
    rpc getAccount "[\"$B\"]" > "$OUT/getAccount-B.json"
    rpc getAccountBalance "[\"$B\"]" > "$OUT/getAccountBalance-B.json"
    local last; last="$(rpc getLastBlockId | sed -n 's/.*"result":\([0-9]*\).*/\1/p')"
    rpc getBlockRange "[1,$last]" > "$OUT/getBlockRange-all.json"
  fi
  cp "$RUN/logs/sequencer.log" "$OUT/sequencer.log"
  { command -v node && node --version; python3 -c 'import playwright; print("playwright", playwright.__version__)'; } > "$OUT/tools.txt" 2>&1 || true
  ls -la "$OUT"
}

case "${1:-}" in
  build) build ;;
  probe) probe ;;
  e2e) shift; . "$HERE/e2e.sh" ;;
  *) sed -n '2,12p' "${BASH_SOURCE[0]}"; exit 2 ;;
esac
