#!/usr/bin/env bash
# One lab session, run by app.py on Modal (the lab's container; nothing here runs on a laptop).
#
# Restores the zone from the Volume (the newest saved generation: the Bedrock node's state, the
# sequencer's home, the driver's follower, block log, manifest and kept updates), starts the
# node and the sequencer, fetches the newest light-client data, lets the driver submit it to
# both instances and wait until it is final, stops both cleanly and saves a new generation.
# A session that does not end with both processes stopped cleanly saves nothing: the next one
# starts again from the last good generation.
#
# Layout: /vol (the Volume): bundle.tar.zst (binaries, program image), state/gen-N.tar.zst and
# state/current, records/ (latest/: the whole record; sessions/ID/: logs; sessions.tsv).
# /lab (the image): this script, fetch.py, LEZ's configuration files, the Sepolia fixture's
# bootstrap. Work on local disk under $WORK.
set -uo pipefail
VOL="${LAB_VOL:-/vol}"
WORK="${LAB_WORK:-/root/lab}"
LAB="${LAB_FILES:-/lab}"
DEADLINE_MIN="${LAB_DEADLINE_MIN:-20}"       # the driver submits no new job after this
KEEP_GENERATIONS="${LAB_KEEP_GENERATIONS:-4}"
RPC_PORT=3040
BEDROCK_PORT=18080
INTERVAL=10                                   # the sequencer's block interval (s)
TICK_MIN=1; TICK_MAX=6; TICK_AFTER=3          # where its blocks fall after an epoch second (lez/live)
MAX_STARTS="${LAB_MAX_STARTS:-4}"
BEDROCK_FUNDING_PUBLIC_KEY=2e03b2eff5a45478e7e79668d2a146cf2c5c7925bce927f2b1c67f2ab4fc0d26

SESSION="$(date -u +%Y%m%dT%H%M%SZ)"
STARTED="$(date +%s)"
REC="$VOL/records/sessions/$SESSION"
mkdir -p "$WORK" "$REC" "$VOL/state" "$VOL/records"
LOG="$WORK/logs"; rm -rf "$LOG"; mkdir -p "$LOG"
say() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$LOG/session.log"; }
PIDS=()
NODE_PID=""; SEQ_PID=""

stop_pid() { # pid name: SIGTERM, wait up to 60 s; prints how it ended
  local pid="$1" name="$2" i
  [ -n "$pid" ] || return 0
  kill -0 "$pid" 2>/dev/null || { wait "$pid" 2>/dev/null; say "$name had already exited ($?)"; return 1; }
  kill -TERM "$pid" 2>/dev/null
  for i in $(seq 1 120); do kill -0 "$pid" 2>/dev/null || break; sleep 0.5; done
  if kill -0 "$pid" 2>/dev/null; then kill -KILL "$pid" 2>/dev/null; wait "$pid" 2>/dev/null; say "$name did not stop in 60 s: killed"; return 1; fi
  wait "$pid" 2>/dev/null; local rc=$?
  say "$name stopped (exit $rc)"
  return 0
}

record_session() { # status note
  local head; head="$(cat "$WORK/head-block" 2>/dev/null || echo -)"
  [ -f "$VOL/records/sessions.tsv" ] || printf 'session\tstarted\tfinished\tstatus\tgeneration_saved\tsequencer_starts\tblock_phase_s\thead_block\tnote\n' > "$VOL/records/sessions.tsv"
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$SESSION" "$STARTED" "$(date +%s)" "$1" "${SAVED:--}" "${STARTS:-0}" "${AFTER:--}" "$head" "$2" >> "$VOL/records/sessions.tsv"
  cp "$LOG"/*.log "$REC/" 2>/dev/null || true
}

# ---- Restore --------------------------------------------------------------------------------
say "session $SESSION"
[ -f "$VOL/bundle.tar.zst" ] || { say "no bundle on the Volume (lab.sh deploy uploads it)"; record_session failed "no bundle"; exit 2; }
rm -rf "$WORK/bundle" "$WORK/state" "$WORK/inputs"; mkdir -p "$WORK/bundle" "$WORK/state" "$WORK/inputs"
zstd -q -d -c "$VOL/bundle.tar.zst" | tar -C "$WORK/bundle" -xf - || { record_session failed "bundle unreadable"; exit 2; }
( cd "$WORK/bundle" && sha256sum -c --quiet SHA256SUMS ) || { record_session failed "bundle SHA256SUMS"; exit 2; }
BIN="$WORK/bundle/bin"; NODE_BIN="$WORK/bundle/node/logos-blockchain-node"; PROGRAM="$WORK/bundle/program/ferrata_lez.bin"
chmod +x "$BIN"/* "$NODE_BIN"
export XDG_CACHE_HOME="$WORK/bundle/xdg-cache"
GEN=0
if [ -f "$VOL/state/current" ]; then
  GEN="$(cat "$VOL/state/current")"
  zstd -q -d -c "$VOL/state/gen-$GEN.tar.zst" | tar -C "$WORK/state" -xf - || { record_session failed "generation $GEN unreadable"; exit 2; }
  say "restored generation $GEN ($(du -sh "$WORK/state" | cut -f1))"
else
  say "no saved generation: a new zone"
fi
S="$WORK/state"
mkdir -p "$S/bedrock" "$S/driver" "$S/config"

# ---- The zone's configuration: LEZ's own files, the sockets on loopback (as lez/live) --------
if [ ! -f "$S/config/sequencer_config.json" ]; then
  python3 -I - "$LAB/lez-config/local.json" "$S/config/sequencer_config.json" "$BEDROCK_PORT" "${INTERVAL}s" <<'EOF'
import json, sys
src, dst, bedrock_port, block_time = sys.argv[1:5]
config = json.load(open(src))
config["bedrock_config"]["node_url"] = f"http://127.0.0.1:{bedrock_port}"
config["gossip"]["listen_addr"] = "/ip4/127.0.0.1/udp/0/quic-v1"
config["metrics_listen_address"] = "127.0.0.1:9000"
config["block_create_timeout"] = block_time
json.dump(config, open(dst, "w"), indent=4)
open(dst, "a").write("\n")
EOF
  cp "$LAB/lez-config/node-config.yaml" "$LAB/lez-config/deployment-settings.yaml" "$S/bedrock/"
  sed -i -e "s#listen_address: 0.0.0.0:18080#listen_address: 127.0.0.1:$BEDROCK_PORT#" \
         -e 's#^    swarm:$#    swarm:\n      host: 127.0.0.1#' "$S/bedrock/node-config.yaml"
  "$BIN/sequencer_service" setup "$S/sequencer_home" --config "$S/config/sequencer_config.json" \
    --funding-public-key "$BEDROCK_FUNDING_PUBLIC_KEY" --create-channel > "$LOG/sequencer-setup.log" 2>&1 \
    || { say "sequencer setup failed"; record_session failed "sequencer setup"; exit 2; }
fi

rpc() { curl -fsS -m 10 -H 'content-type: application/json' -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$1\",\"params\":${2:-[]}}" "http://127.0.0.1:$RPC_PORT"; }
head_block() { rpc getLastBlockId 2>/dev/null | sed -n 's/.*"result":\([0-9]*\).*/\1/p'; }

# ---- Bedrock --------------------------------------------------------------------------------
( cd "$S/bedrock" && POL_PROOF_DEV_MODE=true exec "$NODE_BIN" node-config.yaml --deployment deployment-settings.yaml ) > "$LOG/bedrock.log" 2>&1 &
NODE_PID=$!
for i in $(seq 1 600); do
  curl -fsS -m 3 -o "$WORK/bedrock-info.json" "http://127.0.0.1:$BEDROCK_PORT/cryptarchia/info" 2>/dev/null && break
  kill -0 "$NODE_PID" 2>/dev/null || break
  sleep 0.2
done
if [ ! -s "$WORK/bedrock-info.json" ]; then say "the Bedrock node did not answer"; tail -n 30 "$LOG/bedrock.log"; stop_pid "$NODE_PID" node; record_session failed "node did not start"; exit 2; fi
say "Bedrock node up: $(cat "$WORK/bedrock-info.json")"
# The unix second of the node's slot 0 (lez/live/run-live.sh, bedrock_origin).
slot() { curl -fsS -m 3 "http://127.0.0.1:$BEDROCK_PORT/cryptarchia/info" 2>/dev/null | sed -n 's/.*"slot":\([0-9]*\).*/\1/p'; }
ORIGIN=""; last="$(slot)"; seen=0
for i in $(seq 1 900); do
  now_slot="$(slot)"
  if [ -n "$now_slot" ] && [ -n "$last" ] && [ "$now_slot" != "$last" ] && [ "$now_slot" -gt 0 ]; then
    v=$(( $(date +%s) - now_slot )); if [ -z "$ORIGIN" ] || [ "$v" -lt "$ORIGIN" ]; then ORIGIN="$v"; fi
    seen=$(( seen + 1 )); [ "$seen" -ge 3 ] && break
  fi
  last="$now_slot"; sleep 0.1
done
[ -n "$ORIGIN" ] || { say "could not read the node's slot clock"; stop_pid "$NODE_PID" node; record_session failed "slot clock"; exit 2; }
say "node slot 0 is unix second $ORIGIN"

# ---- The sequencer, started until its blocks keep clear of an epoch's last seconds ----------
STARTS=0; AFTER=-1; DELAY=""
while :; do
  STARTS=$(( STARTS + 1 ))
  if [ -n "$DELAY" ]; then
    # Launch so that launch + DELAY (its scheduler start) falls TICK_AFTER s after an epoch second.
    want=$(( ( (ORIGIN + TICK_AFTER - DELAY) % INTERVAL + INTERVAL ) % INTERVAL ))
    while [ $(( $(date +%s) % INTERVAL )) != "$want" ]; do sleep 0.1; done
  fi
  echo "ferrata-lab start $STARTS" >> "$LOG/sequencer.log"
  launched="$(date +%s)"
  env -u RISC0_DEV_MODE RUST_LOG="info,sequencer_core=debug,risc0_zkvm=warn,kameo=warn" \
    "$BIN/sequencer_service" start "$S/sequencer_home" --rpc-listen-address "127.0.0.1:$RPC_PORT" >> "$LOG/sequencer.log" 2>&1 &
  SEQ_PID=$!
  sched=""
  for i in $(seq 1 300); do
    sched="$(sed -n "/ferrata-lab start $STARTS/,\$p" "$LOG/sequencer.log" | grep -m1 'Block production scheduler started' | sed -n 's/^\[\([0-9T:-]*\)Z.*/\1/p')"
    [ -n "$sched" ] && break
    kill -0 "$SEQ_PID" 2>/dev/null || break
    sleep 1
  done
  if [ -z "$sched" ]; then say "the sequencer did not start its scheduler (start $STARTS)"; tail -n 30 "$LOG/sequencer.log"; break; fi
  sched="$(date -u -d "${sched}Z" +%s)"; DELAY=$(( sched - launched ))
  AFTER=$(( ( (sched + INTERVAL - ORIGIN) % INTERVAL + INTERVAL ) % INTERVAL ))
  say "sequencer start $STARTS: scheduler after $DELAY s; its blocks fall $AFTER s after an epoch second"
  if [ "$AFTER" -ge "$TICK_MIN" ] && [ "$AFTER" -le "$TICK_MAX" ]; then break; fi
  if [ "$STARTS" -ge "$MAX_STARTS" ]; then say "keeping this phase after $STARTS starts"; break; fi
  stop_pid "$SEQ_PID" sequencer; SEQ_PID=""
done
if [ -z "$SEQ_PID" ] || ! kill -0 "$SEQ_PID" 2>/dev/null; then
  stop_pid "$SEQ_PID" sequencer; stop_pid "$NODE_PID" node; record_session failed "sequencer did not start"; exit 2
fi
for i in $(seq 1 120); do h="$(head_block)"; [ -n "$h" ] && break; sleep 1; done
say "sequencer up, head block ${h:-?}"

# ---- Inputs, then the driver ----------------------------------------------------------------
for chain in sepolia mainnet; do
  timeout 240 python3 -I "$LAB/fetch.py" --chain "$chain" --kept "$S/driver/net/$chain" --out "$WORK/inputs/$chain" --repo "$LAB/repo" \
    >> "$LOG/fetch.log" 2>&1 || say "fetch $chain: exit $? (the session goes on)"
  cp "$WORK/inputs/$chain/fetch.json" "$REC/fetch-$chain.json" 2>/dev/null || true
done
tail -n 2 "$LOG/fetch.log" | tee -a "$LOG/session.log"
DEADLINE=$(( $(date +%s) + DEADLINE_MIN * 60 ))
"$BIN/lab-driver" session --rpc "http://127.0.0.1:$RPC_PORT" --run-dir "$S/driver" --program-bin "$PROGRAM" \
  --inputs "$WORK/inputs" --out "$WORK/out" --deadline "$DEADLINE" --session "$SESSION" --sequencer-log "$LOG/sequencer.log" \
  > "$LOG/driver.log" 2>&1
DRIVER_RC=$?
say "driver exit $DRIVER_RC"; tail -n 8 "$LOG/driver.log" | tee -a "$LOG/session.log"
head_block > "$WORK/head-block" || true
curl -fsS -m 5 "http://127.0.0.1:$BEDROCK_PORT/cryptarchia/info" > "$WORK/bedrock-end.json" 2>/dev/null || true

# ---- Stop cleanly, then save ----------------------------------------------------------------
CLEAN=1
stop_pid "$SEQ_PID" sequencer || CLEAN=0
sleep 8   # an inscription the sequencer sent last is still mined
stop_pid "$NODE_PID" node || CLEAN=0
if grep -q -E 'Head rewound|expired unmined' "$LOG/sequencer.log"; then say "the sequencer's log reports a head rewind or an inscription given up"; fi
SAVED=""
if [ "$CLEAN" = 1 ]; then
  rm -rf "$S/bedrock/state/logs"
  NEXT=$(( GEN + 1 ))
  tar -C "$S" -cf - . | zstd -q -T0 -3 -o "$VOL/state/gen-$NEXT.tar.zst.part" -f \
    && mv "$VOL/state/gen-$NEXT.tar.zst.part" "$VOL/state/gen-$NEXT.tar.zst" \
    && echo "$NEXT" > "$VOL/state/current.part" && mv "$VOL/state/current.part" "$VOL/state/current" \
    && SAVED="$NEXT"
  say "saved generation ${SAVED:-NONE} ($(ls -l "$VOL/state/gen-$NEXT.tar.zst" 2>/dev/null | awk '{print $5}') bytes)"
  for old in $(ls "$VOL/state" | sed -n 's/^gen-\([0-9]*\)\.tar\.zst$/\1/p' | sort -n | head -n -"$KEEP_GENERATIONS"); do rm -f "$VOL/state/gen-$old.tar.zst"; done
else
  say "not saved: a process did not stop cleanly; the next session starts from generation $GEN"
fi

# ---- Records --------------------------------------------------------------------------------
scrub() { sed -e "s#$WORK/#lab/#g" -e "s#$HOME/#~/#g"; }
if [ -n "$SAVED" ] && [ -d "$WORK/out" ]; then
  rm -rf "$VOL/records/latest.part"; cp -r "$WORK/out" "$VOL/records/latest.part"
  rm -rf "$VOL/records/latest"; mv "$VOL/records/latest.part" "$VOL/records/latest"
  cp "$S/config/sequencer_config.json" "$VOL/records/latest/sequencer_config.json"
  cp "$WORK/bundle/SHA256SUMS" "$VOL/records/latest/bundle-SHA256SUMS"
fi
cp "$WORK/out/session.tsv" "$REC/" 2>/dev/null || true
for f in session driver fetch sequencer-setup; do [ -f "$LOG/$f.log" ] && scrub < "$LOG/$f.log" > "$REC/$f.log"; done
scrub < "$LOG/sequencer.log" | gzip -c > "$REC/sequencer.log.gz"
{ sed -e 's/\x1b\[[0-9;]*m//g' "$LOG/bedrock.log" | sed -n '1,60p'; echo "[...]"; sed -e 's/\x1b\[[0-9;]*m//g' "$LOG/bedrock.log" | tail -n 40; } | scrub > "$REC/bedrock-excerpt.log"
cp "$WORK/bedrock-info.json" "$REC/bedrock-start.json" 2>/dev/null; cp "$WORK/bedrock-end.json" "$REC/bedrock-end.json" 2>/dev/null
echo "node slot 0 at unix second $ORIGIN; sequencer starts $STARTS; blocks $AFTER s after an epoch second; scheduler delay $DELAY s" > "$REC/phase.txt"
status=ok; [ "$DRIVER_RC" = 0 ] || status="driver-exit-$DRIVER_RC"; [ -n "$SAVED" ] || status="not-saved"
record_session "$status" "$(tail -n 1 "$LOG/driver.log" | tr '\t' ' ' | cut -c1-200)"
say "session $SESSION: $status"
[ "$status" = ok ]
