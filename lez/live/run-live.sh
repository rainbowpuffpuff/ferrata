# The run itself: sourced by run.sh (`run` and `stop`), which has loaded env.sh.
#
# One invocation starts a Bedrock (a Logos Blockchain node, or the sequencer's mock), creates a
# fresh LEZ zone with LEZ's own sequencer, drives the whole scenario through the driver, stops
# everything and writes the records into results/. Nothing started here outlives the invocation.

RUN_DIR="$FERRATA_LIVE_TMP/run"            # this run's scratch: node state, sequencer home, wallets, logs
OUT="${FERRATA_LIVE_OUT:-$LIVE_DIR/results}"
PIDS="$FERRATA_LIVE_TMP/run.pids"
PROOF_SECONDS="${FERRATA_LIVE_PROOF_SECONDS:-3600}"
PHASES="${FERRATA_LIVE_PHASES:-info deploy replay scenarios capacity private-stored private-fake export private-real export}"
PROGRAM_BIN="${FERRATA_PROGRAM_BIN:-$FERRATA_LIVE_TMP/program/ferrata_lez-from-prove_private.bin}"
SCRIPTED_BIN="${FERRATA_SCRIPTED_BIN:-$FERRATA_LIVE_TMP/program/scripted-from-prove_private.bin}"
STORED_TX="${FERRATA_STORED_TX:-$REPO/lez/results/stage2-private-tx.borsh}"

stop_all() {
  [ -f "$PIDS" ] || return 0
  # The sequencer first (it shuts down cleanly on SIGTERM), then the node.
  local pid
  for pid in $(tac "$PIDS"); do
    if kill -0 "$pid" 2>/dev/null; then
      kill -TERM "$pid" 2>/dev/null || true
      for _ in $(seq 1 40); do kill -0 "$pid" 2>/dev/null || break; sleep 0.5; done
      kill -KILL "$pid" 2>/dev/null || true
    fi
  done
  rm -f "$PIDS"
}

if [ "${1:-}" = stop ]; then stop_all; exit 0; fi

rpc() { # method [params-json]
  curl -fsS -m 10 -H 'content-type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$1\",\"params\":${2:-[]}}" "http://127.0.0.1:$RPC_PORT"
}

check_lez
[ -x "$BIN/live-driver" ] || die "build first: run.sh build"
[ -f "$PROGRAM_BIN" ] && [ -f "$SCRIPTED_BIN" ] || die "the guest binaries are missing (README.md says how to obtain them): $PROGRAM_BIN, $SCRIPTED_BIN"

BEDROCK="${FERRATA_LIVE_BEDROCK:-}"
if [ -z "$BEDROCK" ]; then if [ -x "$NODE_BIN" ]; then BEDROCK=node; else BEDROCK=mock; fi; fi
case "$BEDROCK" in
  node) SEQUENCER="$BIN/sequencer_service"; [ -x "$NODE_BIN" ] || die "run.sh build-bedrock first" ;;
  mock) SEQUENCER="$BIN/sequencer_service_standalone" ;;
  *) die "FERRATA_LIVE_BEDROCK must be node or mock" ;;
esac
[ -x "$SEQUENCER" ] || die "$SEQUENCER is not built (run.sh build / build-standalone)"

stop_all
trap stop_all EXIT
rm -rf "$RUN_DIR"
mkdir -p "$RUN_DIR/logs" "$RUN_DIR/config"
: > "$PIDS"
LOG="$RUN_DIR/logs"
STARTED="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# ---- Bedrock ----------------------------------------------------------------------------------
if [ "$BEDROCK" = node ]; then
  # LEZ's own node configuration and deployment (bedrock/), as its container entrypoint runs
  # them (bedrock/scripts/run_logos_blockchain_node.sh): one validator, leader proofs in dev mode.
  mkdir -p "$RUN_DIR/bedrock"
  cp "$LEZ_SRC/bedrock/node-config.yaml" "$LEZ_SRC/bedrock/deployment-settings.yaml" "$RUN_DIR/bedrock/"
  # Two changes, both about where the node listens: its API and its peer-to-peer port on
  # loopback (in LEZ's container they are private to the container).
  sed -i -e "s#listen_address: 0.0.0.0:18080#listen_address: 127.0.0.1:$BEDROCK_PORT#" \
         -e 's#^    swarm:$#    swarm:\n      host: 127.0.0.1#' "$RUN_DIR/bedrock/node-config.yaml"
  diff -u "$LEZ_SRC/bedrock/node-config.yaml" "$RUN_DIR/bedrock/node-config.yaml" \
    | sed -e "s#$LEZ_SRC/#lez/#" -e "s#$RUN_DIR/#run/#" -e 's#\t.*##' > "$RUN_DIR/config/bedrock-node-config.diff" || true
  ( cd "$RUN_DIR/bedrock" && POL_PROOF_DEV_MODE=true exec "$NODE_BIN" node-config.yaml --deployment deployment-settings.yaml ) \
    > "$LOG/bedrock.log" 2>&1 &
  echo $! >> "$PIDS"
  echo "== Bedrock: Logos Blockchain node $(cat "$NODE_DIR/.resolved-host/metadata.json" | tr -d ' \n' | sed 's/.*"resolved_sha":"\([0-9a-f]*\)".*/\1/'), pid $!, API 127.0.0.1:$BEDROCK_PORT"
  for i in $(seq 1 120); do
    curl -fsS -m 3 -o "$RUN_DIR/bedrock-info.json" "http://127.0.0.1:$BEDROCK_PORT/cryptarchia/info" 2>/dev/null && break
    kill -0 "$(tail -1 "$PIDS")" 2>/dev/null || { tail -40 "$LOG/bedrock.log"; die "the Bedrock node exited"; }
    sleep 1
  done
  [ -s "$RUN_DIR/bedrock-info.json" ] || { tail -40 "$LOG/bedrock.log"; die "the Bedrock node's API did not answer"; }
  echo "   cryptarchia/info: $(cat "$RUN_DIR/bedrock-info.json")"
else
  echo "== Bedrock: the sequencer's in-process mock (LEZ's \`standalone\` feature); no Logos Blockchain node runs"
fi

# ---- The sequencer ------------------------------------------------------------------------------
# LEZ's local network configuration (lez/sequencer/service/networks/local.json), with the
# sequencer's sockets on loopback and, if asked, another block interval.
python3 -I - "$LEZ_SRC/lez/sequencer/service/networks/local.json" "$RUN_DIR/config/sequencer_config.json" \
    "$BEDROCK_PORT" "${FERRATA_LIVE_BLOCK_TIME:-}" "${FERRATA_LIVE_METRICS_PORT:-9000}" <<'EOF'
import json, sys
src, dst, bedrock_port, block_time, metrics_port = sys.argv[1:6]
config = json.load(open(src))
config["bedrock_config"]["node_url"] = f"http://127.0.0.1:{bedrock_port}"
config["gossip"]["listen_addr"] = "/ip4/127.0.0.1/udp/0/quic-v1"
config["metrics_listen_address"] = f"127.0.0.1:{metrics_port}"
if block_time:
    config["block_create_timeout"] = block_time
json.dump(config, open(dst, "w"), indent=4)
open(dst, "a").write("\n")
EOF
diff -u "$LEZ_SRC/lez/sequencer/service/networks/local.json" "$RUN_DIR/config/sequencer_config.json" \
  | sed -e "s#$LEZ_SRC/#lez/#" -e "s#$RUN_DIR/config/#run/#" -e 's#\t.*##' > "$RUN_DIR/config/sequencer_config.diff" || true

# As LEZ's Justfile: `sequencer_service setup <home> --config <network> --funding-public-key <key>
# --create-channel`, then `start <home>`. RISC0_DEV_MODE is removed from the sequencer's
# environment: it must verify the receipts of privacy-preserving transactions.
"$SEQUENCER" setup "$RUN_DIR/sequencer_home" --config "$RUN_DIR/config/sequencer_config.json" \
  --funding-public-key "$BEDROCK_FUNDING_PUBLIC_KEY" --create-channel > "$LOG/sequencer-setup.log" 2>&1 \
  || { cat "$LOG/sequencer-setup.log"; die "sequencer setup failed"; }
env -u RISC0_DEV_MODE RUST_LOG="${FERRATA_LIVE_RUST_LOG:-info,sequencer_core=debug,risc0_zkvm=warn,kameo=warn}" \
  "$SEQUENCER" start "$RUN_DIR/sequencer_home" --rpc-listen-address "127.0.0.1:$RPC_PORT" > "$LOG/sequencer.log" 2>&1 &
SEQ_PID=$!
echo "$SEQ_PID" >> "$PIDS"
echo "== sequencer: $(basename "$SEQUENCER") ($(cat "$SEQUENCER.sha256" 2>/dev/null)), pid $SEQ_PID, RPC 127.0.0.1:$RPC_PORT"
for i in $(seq 1 300); do
  head_block="$(rpc getLastBlockId 2>/dev/null | sed -n 's/.*"result":\([0-9]*\).*/\1/p')" || true
  [ -n "$head_block" ] && [ "$head_block" -ge 1 ] && break
  kill -0 "$SEQ_PID" 2>/dev/null || { tail -40 "$LOG/sequencer.log"; die "the sequencer exited"; }
  sleep 1
done
[ -n "${head_block:-}" ] || { tail -40 "$LOG/sequencer.log"; die "the sequencer's RPC did not answer"; }
echo "   head block $head_block after ${i}s"
# What the sequencer process was started with, as the kernel reports it.
{
  echo "command: $(tr '\0' ' ' < /proc/$SEQ_PID/cmdline | sed "s#$BESIDE/#../#g")"
  echo "RISC0_* variables in the sequencer's environment: $(tr '\0' '\n' < /proc/$SEQ_PID/environ | grep '^RISC0_' | tr '\n' ' ' | sed "s#$BESIDE/#../#g")"
  echo "(RISC0_DEV_MODE absent: receipts of privacy-preserving transactions are verified)"
} > "$RUN_DIR/config/sequencer-process.txt"
cat "$RUN_DIR/config/sequencer-process.txt"

# ---- The driver -----------------------------------------------------------------------------------
DRIVER_OUT="$RUN_DIR/export"
driver() { # phase [extra options]
  local phase="$1"; shift
  "$BIN/live-driver" "$phase" --rpc "http://127.0.0.1:$RPC_PORT" --run-dir "$RUN_DIR/driver" --out "$DRIVER_OUT" \
    --program-bin "$PROGRAM_BIN" --scripted-bin "$SCRIPTED_BIN" --sequencer-log "$LOG/sequencer.log" \
    --harness-results "$REPO/lez/results" --stored-tx "$STORED_TX" ${FERRATA_LIVE_GAS_LIMIT:+--gas-limit "$FERRATA_LIVE_GAS_LIMIT"} "$@"
}
status=0
proof_note=""
set +e   # a failing phase must still be recorded and the processes stopped
for phase in $PHASES; do
  echo "== phase $phase ($(date -u +%H:%M:%S))"
  case "$phase" in
    private-fake) RISC0_DEV_MODE=1 driver "$phase" 2>&1 | tee -a "$LOG/driver.log"; rc=${PIPESTATUS[0]} ;;
    private-real)
      if [ "$PROOF_SECONDS" = 0 ]; then echo "   skipped (FERRATA_LIVE_PROOF_SECONDS=0)" | tee -a "$LOG/driver.log"; continue; fi
      proof_started=$(date +%s)
      # Its own session, so that a stopped proof takes its r0vm along.
      env -u RISC0_DEV_MODE setsid timeout --signal=TERM --kill-after=20 "$PROOF_SECONDS" \
        "$BIN/live-driver" "$phase" --rpc "http://127.0.0.1:$RPC_PORT" --run-dir "$RUN_DIR/driver" --out "$DRIVER_OUT" \
        --program-bin "$PROGRAM_BIN" --scripted-bin "$SCRIPTED_BIN" --sequencer-log "$LOG/sequencer.log" \
        --harness-results "$REPO/lez/results" 2>&1 | tee -a "$LOG/driver.log"; rc=${PIPESTATUS[0]}
      proof_note="private-real: exit $rc after $(( $(date +%s) - proof_started )) s (cap $PROOF_SECONDS s)"
      if [ "$rc" = 124 ] || [ "$rc" = 137 ]; then
        pkill -KILL -x r0vm 2>/dev/null || true
        echo "   the proof was stopped at the cap of $PROOF_SECONDS s; its prover input is recorded" | tee -a "$LOG/driver.log"
        rc=0
      fi ;;
    *) ( unset RISC0_DEV_MODE; driver "$phase" ) 2>&1 | tee -a "$LOG/driver.log"; rc=${PIPESTATUS[0]} ;;
  esac
  if [ "$rc" != 0 ]; then echo "== phase $phase FAILED (exit $rc)" | tee -a "$LOG/driver.log"; status=$rc; break; fi
done
# Whatever happened, write the records of what did run.
if [ "$status" != 0 ]; then ( unset RISC0_DEV_MODE; driver export ) 2>&1 | tee -a "$LOG/driver.log"; fi
head_block="$(rpc getLastBlockId 2>/dev/null | sed -n 's/.*"result":\([0-9]*\).*/\1/p')"
set -e

# ---- Stop, then collect ---------------------------------------------------------------------------
stop_all
trap - EXIT
FINISHED="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

scrub() { sed -e "s#$BESIDE/#../#g" -e "s#$HOME/#~/#g"; }   # no paths of this machine in the records
mkdir -p "$OUT/config" "$OUT/logs" "$OUT/private"
if [ -d "$DRIVER_OUT" ]; then cp "$DRIVER_OUT"/* "$OUT/"; fi
cp "$RUN_DIR/config/sequencer_config.diff" "$RUN_DIR/config/sequencer-process.txt" "$OUT/config/"
scrub < "$RUN_DIR/sequencer_home/sequencer_config.json" > "$OUT/config/sequencer_config.json"
[ -f "$RUN_DIR/config/bedrock-node-config.diff" ] && cp "$RUN_DIR/config/bedrock-node-config.diff" "$OUT/config/"
scrub < "$LOG/sequencer.log" > "$OUT/logs/sequencer.log"
scrub < "$LOG/sequencer-setup.log" > "$OUT/logs/sequencer-setup.log"
scrub < "$LOG/driver.log" > "$OUT/logs/driver.log"
if [ -f "$LOG/bedrock.log" ]; then
  # The node's log is large; keep its start, every line about the zone's channel, and its end.
  { head -200 "$LOG/bedrock.log"; echo "[...]"; grep -i -E 'channel|inscri' "$LOG/bedrock.log" | head -400; echo "[...]"; tail -100 "$LOG/bedrock.log"; } \
    | scrub > "$OUT/logs/bedrock-excerpt.log"
  wc -lc < "$LOG/bedrock.log" | awk '{print "the whole log: " $1 " lines, " $2 " bytes"}' >> "$OUT/logs/bedrock-excerpt.log"
fi
for kind in fake real; do
  d="$RUN_DIR/driver/private-$kind"
  [ -d "$d" ] || continue
  mkdir -p "$OUT/private/$kind"
  [ -f "$d/rpc-transcript.jsonl" ] && cp "$d/rpc-transcript.jsonl" "$OUT/private/$kind/"
  [ -f "$d/transaction.borsh" ] && cp "$d/transaction.borsh" "$OUT/private/$kind/"
done
{
  echo "# The run's environment"
  echo
  echo "- started $STARTED, finished $FINISHED (UTC); exit status of the phases: $status"
  echo "- phases: $PHASES"
  [ -n "$proof_note" ] && echo "- $proof_note"
  echo "- LEZ: $LEZ_TAG, commit $LEZ_COMMIT, source tree unmodified"
  echo "- sequencer: $(basename "$SEQUENCER"), sha256 $(cat "$SEQUENCER.sha256" 2>/dev/null), built with $(tr '\n' ' ' < "$SEQUENCER.toolchain" 2>/dev/null)"
  if [ "$BEDROCK" = node ]; then
    echo "- Bedrock: a Logos Blockchain node run as a process, resolved by LEZ's bedrock/tools/resolve_bedrock_node.py: $(tr -d '\n' < "$NODE_DIR/.resolved-host/metadata.json" | tr -s ' ')"
    echo "- Bedrock configuration: LEZ's bedrock/node-config.yaml (see config/bedrock-node-config.diff) and bedrock/deployment-settings.yaml, POL_PROOF_DEV_MODE=true as in LEZ's container entrypoint (leader proofs in dev mode)"
  else
    echo "- Bedrock: none; the sequencer's mock Bedrock actor (LEZ's \`standalone\` feature)"
  fi
  echo "- sequencer configuration: LEZ's lez/sequencer/service/networks/local.json with the changes of config/sequencer_config.diff"
  echo "- $(sed -n 2p "$RUN_DIR/config/sequencer-process.txt")"
  echo "- driver: live-driver, built from lez/live/driver at $(git -C "$REPO" rev-parse HEAD 2>/dev/null) (uncommitted paths: $(git -C "$REPO" status --short 2>/dev/null | grep -v ' lez/live/results/' | wc -l))"
  echo "- r0vm: $(r0vm --version 2>/dev/null)"
  echo "- guest binaries: program sha256 $(sha256sum "$PROGRAM_BIN" | cut -d' ' -f1), scripted sha256 $(sha256sum "$SCRIPTED_BIN" | cut -d' ' -f1)"
  echo "- machine: $(nproc) CPUs visible, $(sed -n 's/^model name[^:]*: //p' /proc/cpuinfo | head -1), $(awk '/MemTotal/{printf "%.0f GiB", $2/1048576}' /proc/meminfo), $(uname -sr)"
  echo "- ports: sequencer RPC 127.0.0.1:$RPC_PORT, metrics 127.0.0.1:${FERRATA_LIVE_METRICS_PORT:-9000}, Bedrock API 127.0.0.1:$BEDROCK_PORT"
  echo "- last block: ${head_block:-?}"
} > "$OUT/environment.md"
cat "$OUT/environment.md"
echo "== records in ${OUT#$REPO/}"
exit "$status"
