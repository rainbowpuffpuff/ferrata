# The run itself: sourced by run.sh (`run` and `stop`), which has loaded env.sh.
#
# One invocation starts a Bedrock (a Logos Blockchain node, or the sequencer's mock), creates a
# fresh LEZ zone with LEZ's own sequencer, drives the whole scenario through the driver, stops
# everything and writes the records into results/. Nothing started here outlives the invocation.

RUN_DIR="$FERRATA_LIVE_TMP/run"            # this run's scratch: node state, sequencer home, wallets, logs
OUT="${FERRATA_LIVE_OUT:-$LIVE_DIR/results$([ "$LEZ_TAG" = v0.4.0-rc1 ] || echo "/$LEZ_TAG")}"
PIDS="$FERRATA_LIVE_TMP/run.pids"
PROOF_SECONDS="${FERRATA_LIVE_PROOF_SECONDS:-3600}"
PHASES="${FERRATA_LIVE_PHASES:-info deploy replay scenarios capacity private-stored private-fake export private-real export}"
PROGRAM_BIN="${FERRATA_PROGRAM_BIN:-$FERRATA_LIVE_TMP/program/ferrata_lez-from-prove_private.bin}"
SCRIPTED_BIN="${FERRATA_SCRIPTED_BIN:-$FERRATA_LIVE_TMP/program/scripted-from-prove_private.bin}"
STORED_TX="${FERRATA_STORED_TX:-$REPO/lez/results/stage2-private-tx.borsh}"
# The harness tables (stage1-*.md) each transaction is compared with: those of the deployed image
# (the default holds the image the harness builds now).
HARNESS_RESULTS="${FERRATA_LIVE_HARNESS_RESULTS:-$REPO/lez/results}"
# scrub: no path of this machine and no name of the folders the run used in the records (scrub.sh).
. "$LIVE_DIR/scrub.sh"

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
  # 15 s): a divisor of the node's epoch (100 s on v0.4.0-rc1, 240 s on rc2 and rc3), so that the
  # sequencer can be started in a phase in which it never publishes a block in the second before
  # an epoch begins (below).
  node) SEQUENCER="$BIN/sequencer_service"; [ -x "$NODE_BIN" ] || die "run.sh build-bedrock first"
        BLOCK_TIME="${FERRATA_LIVE_BLOCK_TIME:-10s}"; WAIT_FINAL=yes ;;
  mock) SEQUENCER="$BIN/sequencer_service_standalone"; BLOCK_TIME="${FERRATA_LIVE_BLOCK_TIME:-}"; WAIT_FINAL=no ;;
  *) die "FERRATA_LIVE_BEDROCK must be node or mock" ;;
esac
[ -x "$SEQUENCER" ] || die "$SEQUENCER is not built (run.sh build / build-standalone)"

stop_all
trap stop_all EXIT
LOG="$RUN_DIR/logs"
STARTED="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# The phase of the sequencer's blocks against Bedrock's epochs. On LEZ's local node an epoch is
# 100 slots of 1 s (v0.4.0-rc1) or 60 slots of 4 s (rc2 and rc3: LEZ #1063), read from the deployment
# settings below, and the node sets its storage gas price anew when an epoch begins. An
# inscription the sequencer submits in the last second of an epoch can only enter the first
# block of the next one; if the price has risen by then, the node leaves it out of every block,
# every later inscription chains on it, and after 300 slots the sequencer gives them all up
# and rewinds its head (observations/). The sequencer makes a block every BLOCK_TIME from the
# moment its scheduler starts. So the zone is started in such a way that its blocks fall
# TICK_AFTER seconds after the seconds at which an epoch can begin; the phase that resulted is
# read from the sequencer's log, and the zone is started again (a fresh node, a fresh
# sequencer) until it is TICK_MIN to TICK_MAX seconds.
#
# The sequencer starts its scheduler 1 to 3 s after it is started if the node's last
# irreversible block is still its genesis block, and about a minute after otherwise. So the
# node is started NODE_LEAD seconds before the sequencer, not earlier.
ALIGN=0
if [ "$BEDROCK" = node ] && [ "${FERRATA_LIVE_ALIGN:-1}" = 1 ]; then ALIGN=1; fi
interval="${BLOCK_TIME%s}"
TICK_AFTER="${FERRATA_LIVE_TICK_AFTER:-3}"
read -r TICK_MIN TICK_MAX <<< "${FERRATA_LIVE_TICK_WINDOW:-1 6}"
STARTUP="${FERRATA_LIVE_STARTUP_SECONDS:-1}"     # the first guess of the sequencer's start-up time
NODE_LEAD="${FERRATA_LIVE_NODE_LEAD:-4}"
MAX_STARTS="${FERRATA_LIVE_MAX_STARTS:-10}"
phase_note=""
# Bedrock's slot and epoch, from the deployment settings the node runs with: an epoch is
# (stake distribution stabilization + nonce buffer + nonce stabilization) base periods of
# floor(security_param / slot_activation_coeff) slots (the node's cryptarchia-engine:
# `epoch_length`, `base_period_length`).
settings_value() { sed -n "s/^ *$1: *\([0-9][0-9]*\) *\$/\1/p" "$LEZ_SRC/bedrock/deployment-settings.yaml" | sed -n 1p; }
SLOT_SECONDS="$(settings_value slot_duration_in_seconds)"
EPOCH_SLOTS=$(( ( $(settings_value epoch_stake_distribution_stabilization) + $(settings_value epoch_period_nonce_buffer)
                  + $(settings_value epoch_period_nonce_stabilization) )
                * ( $(settings_value security_param) * $(settings_value denominator) / $(settings_value numerator) ) ))
EPOCH_SECONDS=$(( EPOCH_SLOTS * SLOT_SECONDS ))
[ "$EPOCH_SECONDS" -gt 0 ] || die "could not read Bedrock's slot and epoch from $LEZ_SRC/bedrock/deployment-settings.yaml"
if [ "$ALIGN" = 1 ] && [ $(( EPOCH_SECONDS % interval )) != 0 ]; then
  die "the block interval ($interval s) does not divide Bedrock's epoch ($EPOCH_SECONDS s): the phase cannot be held"
fi

fresh_run_dir() {
  rm -rf "$RUN_DIR"
  mkdir -p "$RUN_DIR/logs" "$RUN_DIR/config"
  : > "$PIDS"
}

# ---- Bedrock ----------------------------------------------------------------------------------
start_bedrock() {
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
  NODE_PID=$!
  echo "$NODE_PID" >> "$PIDS"
  echo "== Bedrock: Logos Blockchain node $(tr -d ' \n' < "$NODE_DIR/.resolved-host/metadata.json" | sed 's/.*"resolved_sha":"\([0-9a-f]*\)".*/\1/'), pid $NODE_PID, API 127.0.0.1:$BEDROCK_PORT"
  local i
  for i in $(seq 1 600); do
    curl -fsS -m 3 -o "$RUN_DIR/bedrock-info.json" "http://127.0.0.1:$BEDROCK_PORT/cryptarchia/info" 2>/dev/null && break
    kill -0 "$NODE_PID" 2>/dev/null || { tail -40 "$LOG/bedrock.log"; die "the Bedrock node exited"; }
    sleep 0.2
  done
  [ -s "$RUN_DIR/bedrock-info.json" ] || { tail -40 "$LOG/bedrock.log"; die "the Bedrock node's API did not answer"; }
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
  (
    last=""
    while :; do
      now="$(curl -fsS -m 3 "http://127.0.0.1:$BEDROCK_PORT/mantle/gas-prices" 2>/dev/null \
               | tr -d '"{} ' | tr ',' '\n' | grep gas_price | tr '\n' ' ' | sed 's/ $//')" || true
      if [ -n "$now" ] && [ "$now" != "$last" ]; then echo "$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ) $now"; last="$now"; fi
      sleep 0.25
    done
  ) > "$LOG/bedrock-gas-prices.log" 2>&1 &
  echo $! >> "$PIDS"
}

# The unix second of the node's slot 0. A slot starts on a whole second and the node proposes
# its block about 0.1 s later: at the moment the tip's slot changes, that second minus the slot's
# start (slot x SLOT_SECONDS; the least of three such moments, in case one was seen late).
bedrock_origin() {
  local last now_slot seen=0 least="" value
  slot() { curl -fsS -m 3 "http://127.0.0.1:$BEDROCK_PORT/cryptarchia/info" 2>/dev/null | sed -n 's/.*"slot":\([0-9]*\).*/\1/p'; }
  last="$(slot)" || true
  for _ in $(seq 1 900); do
    now_slot="$(slot)" || true
    if [ -n "$now_slot" ] && [ -n "$last" ] && [ "$now_slot" != "$last" ] && [ "$now_slot" -gt 0 ]; then
      value=$(( $(date +%s) - now_slot * SLOT_SECONDS ))
      if [ -z "$least" ] || [ "$value" -lt "$least" ]; then least="$value"; fi
      seen=$(( seen + 1 ))
      if [ "$seen" -ge 3 ]; then echo "$least"; return 0; fi
    fi
    last="$now_slot"; sleep 0.1
  done
  return 1
}

# ---- The zone: a fresh Bedrock, a fresh sequencer home, the sequencer -----------------------------
# With ALIGN it sets `after` (the seconds after an epoch second at which the sequencer makes
# its blocks; -1 if the node came up too late for this start) and `startup_seen`.
# (The body is not indented: it holds a here-document.)
start_zone() {
fresh_run_dir

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

after=-1
if [ "$BEDROCK" = node ]; then
  if [ "$ALIGN" = 1 ]; then
    # The sequencer is to be started at a second = want (mod interval): if it then takes STARTUP
    # seconds to start its scheduler, its blocks fall TICK_AFTER seconds after an epoch second.
    # The node is started NODE_LEAD seconds before that.
    want=$(( ( (origin + TICK_AFTER - STARTUP) % interval + interval ) % interval ))
    node_at=$(( ( (want - NODE_LEAD) % interval + interval ) % interval ))
    while [ $(( $(date +%s) % interval )) != "$node_at" ]; do sleep 0.1; done
  fi
  node_launched="$(date +%s)"
  start_bedrock
  if [ "$ALIGN" = 1 ]; then
    if [ "$(date +%s)" -ge $(( node_launched + NODE_LEAD )) ]; then
      echo "   the node took more than $NODE_LEAD s to answer"; startup_seen="$STARTUP"; return 0
    fi
    while [ "$(date +%s)" -lt $(( node_launched + NODE_LEAD )) ]; do sleep 0.05; done
  fi
else
  echo "== Bedrock: the sequencer's in-process mock (LEZ's \`standalone\` feature); no Logos Blockchain node runs"
fi

launched="$(date +%s)"
# FERRATA_LIVE_SEQUENCER_DEV_MODE=1: the sequencer runs with RISC0_DEV_MODE=1, as LEZ's Justfile documents for
# development ("to disable proof verification"), so that it takes the fake receipts of a driver in dev mode.
if [ "${FERRATA_LIVE_SEQUENCER_DEV_MODE:-0}" = 1 ]; then seq_env="RISC0_DEV_MODE=1"; else seq_env="-u RISC0_DEV_MODE"; fi
env $seq_env RUST_LOG="${FERRATA_LIVE_RUST_LOG:-info,sequencer_core=debug,risc0_zkvm=warn,kameo=warn}" \
  "$SEQUENCER" start "$RUN_DIR/sequencer_home" --rpc-listen-address "127.0.0.1:$RPC_PORT" > "$LOG/sequencer.log" 2>&1 &
SEQ_PID=$!
echo "$SEQ_PID" >> "$PIDS"
echo "== sequencer: $(basename "$SEQUENCER") ($(cat "$SEQUENCER.sha256" 2>/dev/null)), pid $SEQ_PID, RPC 127.0.0.1:$RPC_PORT"
head_block=""
for i in $(seq 1 300); do
  head_block="$(rpc getLastBlockId 2>/dev/null | sed -n 's/.*"result":\([0-9]*\).*/\1/p')" || true
  [ -n "$head_block" ] && [ "$head_block" -ge 1 ] && break
  kill -0 "$SEQ_PID" 2>/dev/null || { tail -40 "$LOG/sequencer.log"; die "the sequencer exited"; }
  sleep 1
done
[ -n "$head_block" ] || { tail -40 "$LOG/sequencer.log"; die "the sequencer's RPC did not answer"; }
echo "   head block $head_block, $(( $(date +%s) - launched )) s after the sequencer was started"
if [ "$ALIGN" = 1 ]; then
  # The phase that resulted: the first block tick is one interval after the scheduler started.
  sched="$(grep -m1 'Block production scheduler started' "$LOG/sequencer.log" | sed -n 's/^\[\([0-9T:-]*\)Z.*/\1/p')"
  [ -n "$sched" ] || die "the sequencer did not start its block scheduler"
  sched="$(date -u -d "${sched}Z" +%s)"
  startup_seen=$(( sched - launched ))
  after=$(( ( (sched + interval - origin) % interval + interval ) % interval ))
  echo "   the sequencer started its scheduler $startup_seen s after it was started: its blocks are made $after s after the seconds at which an epoch can begin, $(( interval - after )) s before the next such second"
  again="$(bedrock_origin)" || die "could not read the Bedrock node's slot clock"
  [ "$again" = "$origin" ] || die "the node's slot 0 moved: unix second $again, was $origin"
fi
}

if [ "$ALIGN" = 1 ]; then
  # The node's slot clock, from a node that is started for this only.
  fresh_run_dir
  start_bedrock
  origin="$(bedrock_origin)" || die "could not read the Bedrock node's slot clock"
  stop_all
  phase_note="slot 0 of the Bedrock node is unix second $origin, so an epoch ($EPOCH_SLOTS slots of $SLOT_SECONDS s) begins at seconds = $(( origin % EPOCH_SECONDS )) mod $EPOCH_SECONDS"
  echo "== $phase_note"
fi
starts=0
while :; do
  starts=$(( starts + 1 ))
  start_zone
  [ "$ALIGN" = 1 ] || break
  if [ "$after" -ge "$TICK_MIN" ] && [ "$after" -le "$TICK_MAX" ]; then
    phase_note="$phase_note; the sequencer makes its blocks $after s after the seconds at which an epoch can begin and $(( interval - after )) s before the next such second (interval $interval s; wanted: $TICK_MIN to $TICK_MAX s after); the zone was started $starts time(s) to get this"
    echo "== $phase_note"
    break
  fi
  [ "$starts" -lt "$MAX_STARTS" ] || die "the sequencer's blocks did not fall $TICK_MIN to $TICK_MAX s after an epoch second in $MAX_STARTS starts"
  echo "== start $starts: wanted $TICK_MIN to $TICK_MAX s after an epoch second; stopping this zone and starting another"
  STARTUP="$startup_seen"
  stop_all
done
# What the sequencer process was started with, as the kernel reports it.
{
  echo "command: $(tr '\0' ' ' < /proc/$SEQ_PID/cmdline | scrub)"
  echo "RISC0_* variables in the sequencer's environment: $(tr '\0' '\n' < /proc/$SEQ_PID/environ | grep '^RISC0_' | tr '\n' ' ' | scrub)"
  if [ "${FERRATA_LIVE_SEQUENCER_DEV_MODE:-0}" = 1 ]; then
    echo "(RISC0_DEV_MODE=1, FERRATA_LIVE_SEQUENCER_DEV_MODE: receipts of privacy-preserving transactions are NOT verified; fake receipts are taken)"
  else
    echo "(RISC0_DEV_MODE absent: receipts of privacy-preserving transactions are verified)"
  fi
} > "$RUN_DIR/config/sequencer-process.txt"
cat "$RUN_DIR/config/sequencer-process.txt"

# ---- The driver -----------------------------------------------------------------------------------
DRIVER_OUT="$RUN_DIR/export"
PHASE_SECONDS="${FERRATA_LIVE_PHASE_SECONDS:-0}"   # a cap on each phase, for observations that are cut short; 0: none
driver() { # phase [extra options]
  local phase="$1"; shift
  timeout --signal=TERM --kill-after=20 "$PHASE_SECONDS" "$BIN/live-driver" "$phase" --rpc "http://127.0.0.1:$RPC_PORT" --run-dir "$RUN_DIR/driver" --out "$DRIVER_OUT" \
    --program-bin "$PROGRAM_BIN" --scripted-bin "$SCRIPTED_BIN" --sequencer-log "$LOG/sequencer.log" \
    --harness-results "$HARNESS_RESULTS" --stored-tx "$STORED_TX" --wait-final "$WAIT_FINAL" \
    ${FERRATA_LIVE_GAS_LIMIT:+--gas-limit "$FERRATA_LIVE_GAS_LIMIT"} ${FERRATA_LIVE_DRIVER_EXTRA:-} "$@"
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
      # the node and the sequencer, which must keep their clocks. FERRATA_LIVE_PROOF_THREADS sets
      # the prover's thread count where the machine reports fewer CPUs than it may use.
      env -u RISC0_DEV_MODE ${FERRATA_LIVE_PROOF_THREADS:+RAYON_NUM_THREADS="$FERRATA_LIVE_PROOF_THREADS"} \
        setsid --wait nice -n "${FERRATA_LIVE_PROOF_NICE:-10}" timeout --signal=TERM --kill-after=20 "$PROOF_SECONDS" \
        "$BIN/live-driver" "$phase" --rpc "http://127.0.0.1:$RPC_PORT" --run-dir "$RUN_DIR/driver" --out "$DRIVER_OUT" \
        --program-bin "$PROGRAM_BIN" --scripted-bin "$SCRIPTED_BIN" --sequencer-log "$LOG/sequencer.log" \
        --harness-results "$HARNESS_RESULTS" --wait-final "$WAIT_FINAL" 2>&1 | tee -a "$LOG/driver.log"; rc=${PIPESTATUS[0]}
      proof_note="private-real: exit $rc after $(( $(date +%s) - proof_started )) s (cap $PROOF_SECONDS s; nice ${FERRATA_LIVE_PROOF_NICE:-10}; RAYON_NUM_THREADS ${FERRATA_LIVE_PROOF_THREADS:-not set})"
      if [ "$rc" = 124 ] || [ "$rc" = 137 ]; then
        pkill -KILL -x r0vm 2>/dev/null || true
        echo "   the proof was stopped at the cap of $PROOF_SECONDS s; its prover input is recorded" | tee -a "$LOG/driver.log"
        rc=0
      fi ;;
    inbound)   # ../inbound/run.sh: a real proof of the private claim unless FERRATA_INBOUND_PRIVATE=fake
      # The proof runs in the driver: its threads as private-real's (FERRATA_LIVE_PROOF_THREADS),
      # below the node and the sequencer (FERRATA_LIVE_PROOF_NICE).
      ( if [ "${FERRATA_INBOUND_PRIVATE:-real}" = fake ]; then export RISC0_DEV_MODE=1; else unset RISC0_DEV_MODE; fi
        if [ -n "${FERRATA_LIVE_PROOF_THREADS:-}" ]; then export RAYON_NUM_THREADS="$FERRATA_LIVE_PROOF_THREADS"; fi
        renice -n "${FERRATA_LIVE_PROOF_NICE:-10}" -p $BASHPID > /dev/null
        driver "$phase" ) 2>&1 | tee -a "$LOG/driver.log"; rc=${PIPESTATUS[0]} ;;
    attest-private)   # driver/src/attest.rs: the private attestation, proven for real in the driver; its threads and
      # priority as private-real's (FERRATA_LIVE_PROOF_THREADS, FERRATA_LIVE_PROOF_NICE); --consumer-bin through
      # FERRATA_LIVE_DRIVER_EXTRA
      ( unset RISC0_DEV_MODE
        if [ -n "${FERRATA_LIVE_PROOF_THREADS:-}" ]; then export RAYON_NUM_THREADS="$FERRATA_LIVE_PROOF_THREADS"; fi
        renice -n "${FERRATA_LIVE_PROOF_NICE:-10}" -p $BASHPID > /dev/null
        driver "$phase" ) 2>&1 | tee -a "$LOG/driver.log"; rc=${PIPESTATUS[0]} ;;
    attest-block)   # driver/src/block.rs (LEZ v0.4.0-rc3): several private reads in one block; fake receipts when the
      # sequencer runs in dev mode (FERRATA_LIVE_SEQUENCER_DEV_MODE=1), real ones otherwise (threads and priority as
      # private-real's); --consumer-bin through FERRATA_LIVE_DRIVER_EXTRA
      ( if [ "${FERRATA_LIVE_SEQUENCER_DEV_MODE:-0}" = 1 ]; then export RISC0_DEV_MODE=1; else unset RISC0_DEV_MODE; fi
        if [ -n "${FERRATA_LIVE_PROOF_THREADS:-}" ]; then export RAYON_NUM_THREADS="$FERRATA_LIVE_PROOF_THREADS"; fi
        renice -n "${FERRATA_LIVE_PROOF_NICE:-10}" -p $BASHPID > /dev/null
        driver "$phase" ) 2>&1 | tee -a "$LOG/driver.log"; rc=${PIPESTATUS[0]} ;;
    hook)   # FERRATA_LIVE_HOOK: a script run while the zone runs (hooks/crown.sh), with the zone in its environment
      if [ -z "${FERRATA_LIVE_HOOK:-}" ] || [ ! -f "$FERRATA_LIVE_HOOK" ]; then
        echo "   no FERRATA_LIVE_HOOK script" | tee -a "$LOG/driver.log"; rc=2
      else
        ( unset RISC0_DEV_MODE
          export ZONE_RPC="http://127.0.0.1:$RPC_PORT" ZONE_BEDROCK="http://127.0.0.1:$BEDROCK_PORT" ZONE_RUN_DIR="$RUN_DIR" \
            ZONE_LOG="$LOG" ZONE_BIN="$BIN" ZONE_OUT="$OUT" ZONE_WAIT_FINAL="$WAIT_FINAL" ZONE_PROGRAM_BIN="$PROGRAM_BIN" \
            ZONE_SEQUENCER_CONFIG="$RUN_DIR/config/sequencer_config.json" ZONE_DEPLOYMENT="$RUN_DIR/bedrock/deployment-settings.yaml"
          bash "$FERRATA_LIVE_HOOK" ) 2>&1 | tee -a "$LOG/hook.log"; rc=${PIPESTATUS[0]}
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

mkdir -p "$OUT/config" "$OUT/logs" "$OUT/private"
if [ -d "$DRIVER_OUT" ]; then   # the driver's records: text through scrub, borsh and other binary files as they are
  for f in "$DRIVER_OUT"/*; do
    case "$f" in
      *.borsh) cp "$f" "$OUT/" ;;
      *) if grep -Iq . "$f"; then scrub < "$f" > "$OUT/${f##*/}"; else cp "$f" "$OUT/"; fi ;;
    esac
  done
fi
cp "$RUN_DIR/config/sequencer_config.diff" "$RUN_DIR/config/sequencer-process.txt" "$OUT/config/"
scrub < "$RUN_DIR/sequencer_home/sequencer_config.json" > "$OUT/config/sequencer_config.json"
[ -f "$RUN_DIR/config/bedrock-node-config.diff" ] && cp "$RUN_DIR/config/bedrock-node-config.diff" "$OUT/config/"
scrub < "$LOG/sequencer.log" > "$OUT/logs/sequencer.log"
scrub < "$LOG/sequencer-setup.log" > "$OUT/logs/sequencer-setup.log"
[ -f "$LOG/driver.log" ] && scrub < "$LOG/driver.log" > "$OUT/logs/driver.log"   # a hook-only run has none
[ -f "$LOG/hook.log" ] && scrub < "$LOG/hook.log" > "$OUT/logs/hook.log"
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
for kind in fake real attest; do   # private-attest: phase attest-private
  d="$RUN_DIR/driver/private-$kind"
  [ -d "$d" ] || continue
  mkdir -p "$OUT/private/$kind"
  [ -f "$d/rpc-transcript.jsonl" ] && cp "$d/rpc-transcript.jsonl" "$OUT/private/$kind/"
  [ -f "$d/transaction.borsh" ] && cp "$d/transaction.borsh" "$OUT/private/$kind/"
done
if [ -d "$RUN_DIR/driver/private-block" ]; then   # phase attest-block: every transaction it held, as proven
  mkdir -p "$OUT/private/block"
  cp "$RUN_DIR/driver/private-block/"*.borsh "$OUT/private/block/" 2>/dev/null || true
fi
{
  echo "# The run's environment"
  echo
  echo "- started $STARTED, finished $FINISHED (UTC); exit status of the phases: $status"
  echo "- phases: $PHASES"
  if [ -n "$proof_note" ]; then echo "- $proof_note"; fi
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
    python3 -I "$LIVE_DIR/phase.py" "$origin" "$interval" "$EPOCH_SECONDS" "$LOG/sequencer.log" "$LOG/bedrock.log" || true
  fi
  if [ -s "$LOG/bedrock-gas-prices.log" ]; then
    echo "- Bedrock gas prices during the run (logs/bedrock-gas-prices.log): $(wc -l < "$LOG/bedrock-gas-prices.log") values; first: $(head -1 "$LOG/bedrock-gas-prices.log"); last: $(tail -1 "$LOG/bedrock-gas-prices.log")"
  fi
  echo "- head rewinds in the sequencer's log: $(grep -c 'Head rewound' "$LOG/sequencer.log" || true); inscriptions it gave up on: $(grep -c 'expired unmined' "$LOG/sequencer.log" || true)"
  # Where the copy of the repository has no history (a build box), FERRATA_HEAD and FERRATA_DIRTY
  # say what git would.
  echo "- driver: live-driver, built from lez/live/driver at ${FERRATA_HEAD:-$(git -C "$REPO" rev-parse HEAD 2>/dev/null)} (uncommitted paths: ${FERRATA_DIRTY:-$(git -C "$REPO" status --short 2>/dev/null | grep -v ' lez/live/results/' | wc -l)})"
  echo "- r0vm: $(r0vm --version 2>/dev/null)"
  echo "- guest binaries: program sha256 $(sha256sum "$PROGRAM_BIN" | cut -d' ' -f1), scripted sha256 $(sha256sum "$SCRIPTED_BIN" | cut -d' ' -f1)"
  echo "- machine: $(nproc) CPUs visible, $(sed -n 's/^model name[^:]*: //p' /proc/cpuinfo | sed -n 1p), $(awk '/MemTotal/{printf "%.0f GiB", $2/1048576}' /proc/meminfo), $(uname -sr)"
  echo "- ports: sequencer RPC 127.0.0.1:$RPC_PORT, metrics 127.0.0.1:${FERRATA_LIVE_METRICS_PORT:-9000}, Bedrock API 127.0.0.1:$BEDROCK_PORT"
  echo "- last block: ${head_block:-?}; blocks the sequencer published to Bedrock: $(grep -c 'Published block' "$LOG/sequencer.log" || true); its last channel update: $(grep 'Channel update' "$LOG/sequencer.log" | tail -1 | sed 's/^.*Channel update: //')"
  if [ -n "$bedrock_end" ]; then echo "- Bedrock node at the end (cryptarchia/info): $bedrock_end"; fi
} | scrub > "$OUT/environment.md"
cat "$OUT/environment.md"
echo "== records in ${OUT#$REPO/}"
exit "$status"
