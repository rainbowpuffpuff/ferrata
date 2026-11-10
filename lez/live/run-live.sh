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
  # With a Bedrock node the block interval is 10 s (LEZ's testnet.json value; local.json has
  # 15 s): a divisor of the node's 100-slot epoch, so that the sequencer can be started in a
  # phase in which it never publishes a block in the second before an epoch begins (below).
  node) SEQUENCER="$BIN/sequencer_service"; [ -x "$NODE_BIN" ] || die "run.sh build-bedrock first"
        BLOCK_TIME="${FERRATA_LIVE_BLOCK_TIME:-10s}"; WAIT_FINAL=yes ;;
  mock) SEQUENCER="$BIN/sequencer_service_standalone"; BLOCK_TIME="${FERRATA_LIVE_BLOCK_TIME:-}"; WAIT_FINAL=no ;;
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
  # For an observation only (FERRATA_LIVE_BEDROCK_TRACE=1): let the node log at trace level, so
  # that its block proposer says why it leaves a transaction out (narrowed to that below).
  if [ "${FERRATA_LIVE_BEDROCK_TRACE:-0}" = 1 ]; then
    sed -i -e 's#^  level: "INFO"#  level: "TRACE"#' "$RUN_DIR/bedrock/node-config.yaml"
  fi
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
  if [ "${FERRATA_LIVE_BEDROCK_TRACE:-0}" = 1 ]; then
    # Trace level for the block proposer only; everything else as with level INFO.
    curl -fsS -m 5 -X PUT -H 'content-type: application/json' \
      -d '{"filters":{"*":"warn","logos_blockchain":"info","logos_blockchain::chain::leader":"trace"}}' \
      "http://127.0.0.1:$BEDROCK_PORT/admin/tracing/filter" > "$RUN_DIR/bedrock-trace-filter.txt" 2>&1 \
      && echo "   the node logs its block proposer at trace level" \
      || echo "   the node refused the tracing filter: $(cat "$RUN_DIR/bedrock-trace-filter.txt")"
  fi
  # The node's gas prices (its /mantle/gas-prices, at its tip), read four times a second; one
  # line whenever one of them changes.
  gas_prices() {
    curl -fsS -m 3 "http://127.0.0.1:$BEDROCK_PORT/mantle/gas-prices" 2>/dev/null \
      | tr -d '"{} ' | tr ',' '\n' | grep gas_price | tr '\n' ' ' | sed 's/ $//'
  }
  (
    last=""
    while :; do
      now="$(gas_prices)" || true
      if [ -n "$now" ] && [ "$now" != "$last" ]; then echo "$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ) $now"; last="$now"; fi
      sleep 0.25
    done
  ) > "$LOG/bedrock-gas-prices.log" 2>&1 &
  echo $! >> "$PIDS"
else
  echo "== Bedrock: the sequencer's in-process mock (LEZ's \`standalone\` feature); no Logos Blockchain node runs"
fi

# ---- The sequencer ------------------------------------------------------------------------------
# LEZ's local network configuration (lez/sequencer/service/networks/local.json), with the
# sequencer's sockets on loopback and, if asked, another block interval.
python3 -I - "$LEZ_SRC/lez/sequencer/service/networks/local.json" "$RUN_DIR/config/sequencer_config.json" \
    "$BEDROCK_PORT" "$BLOCK_TIME" "${FERRATA_LIVE_METRICS_PORT:-9000}" <<'EOF'
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

# The phase of the sequencer's blocks against Bedrock's epochs. On this node an epoch is 100
# slots of 1 s. An inscription the sequencer submits in the second before an epoch's first
# block is removed from that block by the node and never included; every later inscription
# chains on it; after 300 slots the sequencer gives them up and rewinds its head
# (observations/rewind/). So the sequencer is started at a second from which its block ticks
# fall TICK_AFTER seconds after the seconds at which an epoch can begin, never just before one.
phase_note=""
if [ "$BEDROCK" = node ] && [ "${FERRATA_LIVE_ALIGN:-1}" = 1 ]; then
  interval="${BLOCK_TIME%s}"
  TICK_AFTER="${FERRATA_LIVE_TICK_AFTER:-3}"
  # Seconds from the sequencer's start to its "Block production scheduler started": 1 to 3 seen.
  STARTUP="${FERRATA_LIVE_STARTUP_SECONDS:-2}"
  slot() { curl -fsS -m 3 "http://127.0.0.1:$BEDROCK_PORT/cryptarchia/info" 2>/dev/null | sed -n 's/.*"slot":\([0-9]*\).*/\1/p'; }
  # A slot starts on a whole second; the node proposes its block about 0.1 s later. Catch the
  # moment the tip's slot changes: that second minus the slot is the second of slot 0.
  last="$(slot)"; origin=""
  for _ in $(seq 1 400); do
    now_slot="$(slot)"
    if [ -n "$now_slot" ] && [ -n "$last" ] && [ "$now_slot" != "$last" ] && [ "$now_slot" -gt 0 ]; then
      origin=$(( $(date +%s) - now_slot )); break
    fi
    last="$now_slot"; sleep 0.1
  done
  [ -n "$origin" ] || die "could not read the Bedrock node's slot clock"
  # The first tick is one interval after the scheduler starts, so ticks are at
  # start + STARTUP (mod interval); they are wanted at origin + TICK_AFTER (mod interval).
  want=$(( ( (origin + TICK_AFTER - STARTUP) % interval + interval ) % interval ))
  while [ $(( $(date +%s) % interval )) != "$want" ]; do sleep 0.2; done
  phase_note="slot 0 of the Bedrock node is unix second $origin, so an epoch (100 slots) begins at seconds = $(( origin % 100 )) mod 100; sequencer started at a second = $want mod $interval"
  echo "== $phase_note"
fi
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
if [ -n "$phase_note" ]; then
  # Check the phase that resulted: the first block tick is one interval after the scheduler started.
  sched="$(grep -m1 'Block production scheduler started' "$LOG/sequencer.log" | sed -n 's/^\[\([0-9T:-]*\)Z.*/\1/p')"
  [ -n "$sched" ] || die "the sequencer did not start its block scheduler"
  tick=$(( $(date -u -d "${sched}Z" +%s) + interval ))
  after=$(( ( (tick - origin) % interval + interval ) % interval ))
  phase_note="$phase_note; its blocks are made $after s after the seconds at which an epoch can begin and $(( interval - after )) s before the next such second (interval $interval s)"
  echo "   $phase_note"
  if [ "${FERRATA_LIVE_PHASE_CHECK:-1}" = 1 ]; then
    [ "$after" -ge 2 ] && [ "$after" -le $(( interval / 2 )) ] || die "bad phase (blocks $after s after an epoch second): start again"
  fi
fi
# What the sequencer process was started with, as the kernel reports it.
{
  echo "command: $(tr '\0' ' ' < /proc/$SEQ_PID/cmdline | sed "s#$BESIDE/#../#g")"
  echo "RISC0_* variables in the sequencer's environment: $(tr '\0' '\n' < /proc/$SEQ_PID/environ | grep '^RISC0_' | tr '\n' ' ' | sed "s#$BESIDE/#../#g")"
  echo "(RISC0_DEV_MODE absent: receipts of privacy-preserving transactions are verified)"
} > "$RUN_DIR/config/sequencer-process.txt"
cat "$RUN_DIR/config/sequencer-process.txt"

# ---- The driver -----------------------------------------------------------------------------------
DRIVER_OUT="$RUN_DIR/export"
PHASE_SECONDS="${FERRATA_LIVE_PHASE_SECONDS:-0}"   # a cap on each phase, for observations that are cut short; 0: none
driver() { # phase [extra options]
  local phase="$1"; shift
  timeout --signal=TERM --kill-after=20 "$PHASE_SECONDS" "$BIN/live-driver" "$phase" --rpc "http://127.0.0.1:$RPC_PORT" --run-dir "$RUN_DIR/driver" --out "$DRIVER_OUT" \
    --program-bin "$PROGRAM_BIN" --scripted-bin "$SCRIPTED_BIN" --sequencer-log "$LOG/sequencer.log" \
    --harness-results "$REPO/lez/results" --stored-tx "$STORED_TX" --wait-final "$WAIT_FINAL" \
    ${FERRATA_LIVE_GAS_LIMIT:+--gas-limit "$FERRATA_LIVE_GAS_LIMIT"} "$@"
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
      # Its own session, so that a stopped proof takes its r0vm along; at a lower priority than
      # the node and the sequencer, which must keep their clocks.
      env -u RISC0_DEV_MODE setsid --wait nice -n "${FERRATA_LIVE_PROOF_NICE:-10}" timeout --signal=TERM --kill-after=20 "$PROOF_SECONDS" \
        "$BIN/live-driver" "$phase" --rpc "http://127.0.0.1:$RPC_PORT" --run-dir "$RUN_DIR/driver" --out "$DRIVER_OUT" \
        --program-bin "$PROGRAM_BIN" --scripted-bin "$SCRIPTED_BIN" --sequencer-log "$LOG/sequencer.log" \
        --harness-results "$REPO/lez/results" --wait-final "$WAIT_FINAL" 2>&1 | tee -a "$LOG/driver.log"; rc=${PIPESTATUS[0]}
      proof_note="private-real: exit $rc after $(( $(date +%s) - proof_started )) s (cap $PROOF_SECONDS s)"
      if [ "$rc" = 124 ] || [ "$rc" = 137 ]; then
        pkill -KILL -x r0vm 2>/dev/null || true
        echo "   the proof was stopped at the cap of $PROOF_SECONDS s; its prover input is recorded" | tee -a "$LOG/driver.log"
        rc=0
      fi ;;
    *) ( unset RISC0_DEV_MODE; driver "$phase" ) 2>&1 | tee -a "$LOG/driver.log"; rc=${PIPESTATUS[0]} ;;
  esac
  case "$rc" in
    124|137) echo "   phase $phase was stopped at FERRATA_LIVE_PHASE_SECONDS=$PHASE_SECONDS" | tee -a "$LOG/driver.log" ;;
  esac
  if [ "$rc" != 0 ]; then echo "== phase $phase FAILED (exit $rc)" | tee -a "$LOG/driver.log"; status=$rc; break; fi
done
# Whatever happened, write the records of what did run.
if [ "$status" != 0 ]; then ( unset RISC0_DEV_MODE; driver export ) 2>&1 | tee -a "$LOG/driver.log"; fi
head_block="$(rpc getLastBlockId 2>/dev/null | sed -n 's/.*"result":\([0-9]*\).*/\1/p')"
bedrock_end=""
if [ "$BEDROCK" = node ]; then bedrock_end="$(curl -fsS -m 5 "http://127.0.0.1:$BEDROCK_PORT/cryptarchia/info" 2>/dev/null)"; fi
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
[ -f "$LOG/bedrock-gas-prices.log" ] && cp "$LOG/bedrock-gas-prices.log" "$OUT/logs/"
if [ -f "$LOG/bedrock.log" ]; then
  # The node's log is large: keep its start, the epochs it began (one line each; it logs each
  # several times), the blocks it proposed that carry transactions (the zone's inscriptions are
  # the only transactions on this network) or from which it removed one, and its end.
  plain() { sed -e 's/\x1b\[[0-9;]*m//g'; }
  {
    # (sed and awk read to the end: under `pipefail` a reader that stops early kills the run)
    plain < "$LOG/bedrock.log" | sed -n '1,150p'
    echo "[...] epochs begun, blocks proposed with transactions, blocks proposed with transactions removed"
    echo "[...] (and, if the node logged at trace level, why a transaction was left out: once per transaction and reason):"
    plain < "$LOG/bedrock.log" \
      | { grep -E 'epoch transition|proposed block .* with [1-9][0-9]* transactions|\([1-9][0-9]* removed\)|during block assembly' || true; } \
      | awk '{ if ($0 ~ /epoch transition/) { match($0, /new_epoch=Epoch\([0-9]+\)/); k = substr($0, RSTART, RLENGTH); if (k in seen) next; seen[k] = 1 }
               else if ($0 ~ /during block assembly/) { k = substr($0, index($0, "tx ")); if (k in seen) next; seen[k] = 1 }
               print }'
    echo "[...] the end:"
    plain < "$LOG/bedrock.log" | tail -40
    echo "the whole log: $(wc -l < "$LOG/bedrock.log") lines, $(wc -c < "$LOG/bedrock.log") bytes; blocks proposed: $(grep -c 'proposed block' "$LOG/bedrock.log" || true), of which with transactions: $(grep -c -E 'proposed block .* with [1-9][0-9]* transactions' "$LOG/bedrock.log" || true), with a transaction removed: $(grep -c -E '\([1-9][0-9]* removed\)' "$LOG/bedrock.log" || true)"
  } | scrub > "$OUT/logs/bedrock-excerpt.log"
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
  if [ -n "$phase_note" ]; then
    echo "- block phase: $phase_note"
    python3 -I "$LIVE_DIR/phase.py" "$origin" "$interval" "$LOG/sequencer.log" "$LOG/bedrock.log" || true
  fi
  if [ -s "$LOG/bedrock-gas-prices.log" ]; then
    echo "- Bedrock gas prices during the run (logs/bedrock-gas-prices.log): $(wc -l < "$LOG/bedrock-gas-prices.log") values; first: $(head -1 "$LOG/bedrock-gas-prices.log"); last: $(tail -1 "$LOG/bedrock-gas-prices.log")"
  fi
  echo "- head rewinds in the sequencer's log: $(grep -c 'Head rewound' "$LOG/sequencer.log" || true); inscriptions it gave up on: $(grep -c 'expired unmined' "$LOG/sequencer.log" || true)"
  echo "- driver: live-driver, built from lez/live/driver at $(git -C "$REPO" rev-parse HEAD 2>/dev/null) (uncommitted paths: $(git -C "$REPO" status --short 2>/dev/null | grep -v ' lez/live/results/' | wc -l))"
  echo "- r0vm: $(r0vm --version 2>/dev/null)"
  echo "- guest binaries: program sha256 $(sha256sum "$PROGRAM_BIN" | cut -d' ' -f1), scripted sha256 $(sha256sum "$SCRIPTED_BIN" | cut -d' ' -f1)"
  echo "- machine: $(nproc) CPUs visible, $(sed -n 's/^model name[^:]*: //p' /proc/cpuinfo | sed -n 1p), $(awk '/MemTotal/{printf "%.0f GiB", $2/1048576}' /proc/meminfo), $(uname -sr)"
  echo "- ports: sequencer RPC 127.0.0.1:$RPC_PORT, metrics 127.0.0.1:${FERRATA_LIVE_METRICS_PORT:-9000}, Bedrock API 127.0.0.1:$BEDROCK_PORT"
  echo "- last block: ${head_block:-?}; blocks the sequencer published to Bedrock: $(grep -c 'Published block' "$LOG/sequencer.log" || true); its last channel update: $(grep 'Channel update' "$LOG/sequencer.log" | tail -1 | sed 's/^.*Channel update: //')"
  [ -n "$bedrock_end" ] && echo "- Bedrock node at the end (cryptarchia/info): $bedrock_end"
} > "$OUT/environment.md"
cat "$OUT/environment.md"
echo "== records in ${OUT#$REPO/}"
exit "$status"
