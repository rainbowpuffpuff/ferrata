#!/usr/bin/env bash
# One session of the crown run's LEZ devnet (lez/crown/README.md), on a Modal container or the build box; never on
# a laptop. Modelled on lez/lab/session.sh: the zone is saved as generations on a Volume between sessions.
#
#   session.sh a   a NEW zone (refuses if a generation exists): LEZ v0.4.0-rc1 sequencer + Logos Blockchain node
#                  (LEZ's local configuration, dev-mode leader proofs, `setup --create-channel`); crown-driver crown-a
#                  (deploy, the Sepolia instance and its catch-up); stop cleanly; save generation 1; session-a/
#   session.sh b   restore the newest generation; crown-driver crown-b over the captures (the bridge's pairing,
#                  registration, imports, claims and burns, with two real private proofs); the node's final blocks
#                  (session-b/node/, for the checkpoint); stop; save; session-b/
#   session.sh dump  restore the newest generation, start the node alone, write session-b/node/ again; saves nothing
#
# Environment: CROWN_LEZ_VOL (default /vol/lez), CROWN_LEZ_WORK (default /root/crown-lez, wiped), CROWN_CHAIN
# (sepolia | stand-in), CROWN_VAULT (a: optional), CROWN_CAPTURES (b: capture dirs in order; default
# $CROWN_LEZ_VOL/inputs/capture-1 and -2), CROWN_ETH_FEES_AT, CROWN_RECIPIENT, CROWN_PRIVATE (real | fake),
# CROWN_R0VM (the prover's r0vm; default the bundle's), CROWN_DEADLINE_MIN (default 120).
# Layout of $CROWN_LEZ_VOL: bundle.tar.zst (input), state/gen-N.tar.zst + state/current, inputs/, session-a/, session-b/.
set -uo pipefail
STEP="${1:?a or b}"
VOL="${CROWN_LEZ_VOL:-/vol/lez}"
WORK="${CROWN_LEZ_WORK:-/root/crown-lez}"
CHAIN="${CROWN_CHAIN:-sepolia}"
DEADLINE_MIN="${CROWN_DEADLINE_MIN:-120}"
RPC_PORT=3040
BEDROCK_PORT=18080
INTERVAL=10                                   # the sequencer's block interval (s)
TICK_MIN=1; TICK_MAX=6; TICK_AFTER=3          # where its blocks fall after an epoch second (lez/live, lez/lab)
MAX_STARTS="${CROWN_MAX_STARTS:-4}"
BEDROCK_FUNDING_PUBLIC_KEY=2e03b2eff5a45478e7e79668d2a146cf2c5c7925bce927f2b1c67f2ab4fc0d26
CHANNEL=0101010101010101010101010101010101010101010101010101010101010101
case "$STEP" in a|b|dump) ;; *) echo "session.sh a | b | dump"; exit 2 ;; esac
case "$CHAIN" in sepolia|stand-in) ;; *) echo "CROWN_CHAIN: sepolia or stand-in"; exit 2 ;; esac

VOL="$(realpath -m "$VOL")"; WORK="$(realpath -m "$WORK")"
OUT="$VOL/session-$STEP"; [ "$STEP" = dump ] && OUT="$VOL/session-b"
rm -rf "$WORK"; mkdir -p "$WORK" "$VOL/state" "$OUT"
LOG="$WORK/logs"; mkdir -p "$LOG"
say() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$LOG/session.log"; }
NODE_PID=""; SEQ_PID=""
stop_pid() { # pid name: SIGTERM, wait up to 60 s
  local pid="$1" name="$2" i
  [ -n "$pid" ] || return 0
  kill -0 "$pid" 2>/dev/null || { wait "$pid" 2>/dev/null; say "$name had already exited ($?)"; return 1; }
  kill -TERM "$pid" 2>/dev/null
  for i in $(seq 1 120); do kill -0 "$pid" 2>/dev/null || break; sleep 0.5; done
  if kill -0 "$pid" 2>/dev/null; then kill -KILL "$pid" 2>/dev/null; wait "$pid" 2>/dev/null; say "$name did not stop in 60 s: killed"; return 1; fi
  wait "$pid" 2>/dev/null; say "$name stopped (exit $?)"; return 0
}
scrub() { sed -e "s#$WORK/#work/#g" -e "s#$VOL/#vol/#g" -e "s#$HOME/#~/#g"; }
records() { # copy the logs to the Volume
  local f n L="$OUT/logs"; [ "$STEP" = dump ] && L="$OUT/logs-dump"
  mkdir -p "$L"
  for f in "$LOG"/*.log; do
    [ -f "$f" ] || continue; n="$(basename "$f")"
    case "$n" in
      sequencer.log) scrub < "$f" | gzip -c > "$L/sequencer.log.gz" ;;
      bedrock.log) { sed -e 's/\x1b\[[0-9;]*m//g' "$f" | sed -n '1,60p'; echo "[...]"; sed -e 's/\x1b\[[0-9;]*m//g' "$f" | tail -n 60; } | scrub > "$L/bedrock-excerpt.log" ;;
      *) scrub < "$f" > "$L/$n" ;;
    esac
  done
}
fail() { say "FAILED: $*"; stop_pid "$SEQ_PID" sequencer; stop_pid "$NODE_PID" node; records; exit 2; }

# ---- Restore --------------------------------------------------------------------------------
say "crown LEZ session $STEP (chain $CHAIN)"
[ -f "$VOL/bundle.tar.zst" ] || fail "no bundle at $VOL/bundle.tar.zst (lez/crown/build.sh pack)"
mkdir -p "$WORK/bundle" "$WORK/state"
zstd -q -d -c "$VOL/bundle.tar.zst" | tar -C "$WORK/bundle" -xf - || fail "bundle unreadable"
( cd "$WORK/bundle" && sha256sum -c --quiet SHA256SUMS ) || fail "bundle SHA256SUMS"
B="$WORK/bundle"; BIN="$B/bin"; NODE_BIN="$B/node/logos-blockchain-node"
chmod +x "$BIN"/* "$NODE_BIN"
export XDG_CACHE_HOME="$B/xdg-cache" PATH="$BIN:$PATH" RISC0_SERVER_PATH="$BIN/r0vm"
GEN=0
[ -f "$VOL/state/current" ] && GEN="$(cat "$VOL/state/current")"
if [ "$STEP" = a ]; then
  [ "$GEN" = 0 ] || fail "a generation exists ($GEN): session A starts a new zone (use another CROWN_LEZ_VOL)"
  say "a new zone"
else
  [ "$GEN" != 0 ] || fail "no saved generation: run session A first"
  zstd -q -d -c "$VOL/state/gen-$GEN.tar.zst" | tar -C "$WORK/state" -xf - || fail "generation $GEN unreadable"
  say "restored generation $GEN ($(du -sh "$WORK/state" | cut -f1))"
fi
S="$WORK/state"; mkdir -p "$S/bedrock" "$S/driver" "$S/config"

# ---- The zone's configuration: LEZ's own files, the sockets on loopback (as lez/live, lez/lab) -----
if [ "$STEP" = a ]; then
  python3 -I - "$B/lez-config/local.json" "$S/config/sequencer_config.json" "$BEDROCK_PORT" "${INTERVAL}s" <<'EOF'
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
  cp "$B/lez-config/node-config.yaml" "$B/lez-config/deployment-settings.yaml" "$S/bedrock/"
  sed -i -e "s#listen_address: 0.0.0.0:18080#listen_address: 127.0.0.1:$BEDROCK_PORT#" \
         -e 's#^    swarm:$#    swarm:\n      host: 127.0.0.1#' "$S/bedrock/node-config.yaml"
  "$BIN/sequencer_service" setup "$S/sequencer_home" --config "$S/config/sequencer_config.json" \
    --funding-public-key "$BEDROCK_FUNDING_PUBLIC_KEY" --create-channel > "$LOG/sequencer-setup.log" 2>&1 \
    || fail "sequencer setup"
fi

# The node's own final blocks, for the checkpoint (crown-proofs' realblocks-crown reads them; the layout of their
# checkpoint/realblocks/crown-extract.sh): /cryptarchia/info, /channel, /chain/id and /cryptarchia/blocks in 500-slot
# windows up to the LIB slot, from 3000 slots before the channel's tip slot when session A first read it (the node's
# slot clock starts long before this devnet: LEZ's local deployment settings).
node_dump() {
  local N="$OUT/node" api="http://127.0.0.1:$BEDROCK_PORT" from lib to
  rm -rf "$N"; mkdir -p "$N/blocks"
  from="$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["tip_slot"])' "$VOL/session-a/channel.json" 2>/dev/null)"
  [ -n "$from" ] || { say "node dump: no tip_slot in session-a/channel.json"; return 1; }
  from=$(( from > 3000 ? from - 3000 : 0 ))
  curl -fsS -m 10 -o "$N/info.json" "$api/cryptarchia/info" && curl -fsS -m 10 -o "$N/channel.json" "$api/channel/$CHANNEL" \
    || { say "node dump: no info/channel"; return 1; }
  curl -fsS -m 10 -o "$N/chain-id.json" "$api/chain/id" || say "node dump: no chain id"
  lib="$(sed -n 's/.*"lib_slot":\([0-9]*\).*/\1/p' "$N/info.json")"
  echo "$from" > "$N/slot-from.txt"
  while [ -n "$lib" ] && [ "$from" -le "$lib" ]; do
    to=$((from + 499)); [ "$to" -gt "$lib" ] && to="$lib"
    curl -fsS -m 60 -o "$N/blocks/$from-$to.json" "$api/cryptarchia/blocks?slot_from=$from&slot_to=$to" || { say "node dump: window $from-$to failed"; return 1; }
    from=$((to + 1))
  done
  say "node dump: slots $(cat "$N/slot-from.txt")..=${lib:-?}, $(ls "$N/blocks" | wc -l) windows, $(du -sb "$N/blocks" | cut -f1) bytes"
}

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
[ -s "$WORK/bedrock-info.json" ] || fail "the Bedrock node did not answer"
say "Bedrock node up: $(cat "$WORK/bedrock-info.json")"
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
[ -n "$ORIGIN" ] || fail "could not read the node's slot clock"
say "node slot 0 is unix second $ORIGIN"
if [ "$STEP" = dump ]; then
  for i in $(seq 1 120); do grep -q '"state":"Online"' <(curl -fsS -m 3 "http://127.0.0.1:$BEDROCK_PORT/cryptarchia/info" 2>/dev/null) && break; sleep 1; done
  node_dump; rc=$?
  stop_pid "$NODE_PID" node
  records; say "dump: exit $rc (nothing saved)"; exit "$rc"
fi

# ---- The sequencer, started until its blocks keep clear of an epoch's last seconds ----------
STARTS=0; AFTER=-1; DELAY=""
while :; do
  STARTS=$(( STARTS + 1 ))
  if [ -n "$DELAY" ]; then
    want=$(( ( (ORIGIN + TICK_AFTER - DELAY) % INTERVAL + INTERVAL ) % INTERVAL ))
    while [ $(( $(date +%s) % INTERVAL )) != "$want" ]; do sleep 0.1; done
  fi
  echo "ferrata-crown start $STARTS" >> "$LOG/sequencer.log"
  launched="$(date +%s)"
  env -u RISC0_DEV_MODE RUST_LOG="info,sequencer_core=debug,risc0_zkvm=warn,kameo=warn" \
    "$BIN/sequencer_service" start "$S/sequencer_home" --rpc-listen-address "127.0.0.1:$RPC_PORT" >> "$LOG/sequencer.log" 2>&1 &
  SEQ_PID=$!
  sched=""
  for i in $(seq 1 300); do
    sched="$(sed -n "/ferrata-crown start $STARTS/,\$p" "$LOG/sequencer.log" | grep -m1 'Block production scheduler started' | sed -n 's/^\[\([0-9T:-]*\)Z.*/\1/p')"
    [ -n "$sched" ] && break
    kill -0 "$SEQ_PID" 2>/dev/null || break
    sleep 1
  done
  [ -n "$sched" ] || { say "the sequencer did not start its scheduler (start $STARTS)"; tail -n 30 "$LOG/sequencer.log"; break; }
  sched="$(date -u -d "${sched}Z" +%s)"; DELAY=$(( sched - launched ))
  AFTER=$(( ( (sched + INTERVAL - ORIGIN) % INTERVAL + INTERVAL ) % INTERVAL ))
  say "sequencer start $STARTS: scheduler after $DELAY s; its blocks fall $AFTER s after an epoch second"
  if [ "$AFTER" -ge "$TICK_MIN" ] && [ "$AFTER" -le "$TICK_MAX" ]; then break; fi
  if [ "$STARTS" -ge "$MAX_STARTS" ]; then say "keeping this phase after $STARTS starts"; break; fi
  stop_pid "$SEQ_PID" sequencer; SEQ_PID=""
done
{ [ -n "$SEQ_PID" ] && kill -0 "$SEQ_PID" 2>/dev/null; } || fail "the sequencer did not start"
for i in $(seq 1 120); do h="$(head_block)"; [ -n "$h" ] && break; sleep 1; done
say "sequencer up, head block ${h:-?}"

# ---- The driver -----------------------------------------------------------------------------
D=(--rpc "http://127.0.0.1:$RPC_PORT" --run-dir "$S/driver" --program-bin "$B/program/ferrata_lez.bin"
   --bridge-bin "$B/program/ferrata_bridge.bin" --chain "$CHAIN" --fixture "$B/fixture" --sequencer-log "$LOG/sequencer.log")
DRIVER_RC=1
if [ "$STEP" = a ]; then
  # The channel's key set, once the channel's first inscription is on Bedrock.
  for i in $(seq 1 150); do
    curl -fsS -m 5 -o "$WORK/channel.json" "http://127.0.0.1:$BEDROCK_PORT/channel/$CHANNEL" 2>/dev/null \
      && grep -q '"accredited_keys":\["' "$WORK/channel.json" && break
    sleep 2
  done
  grep -q '"accredited_keys":\["' "$WORK/channel.json" 2>/dev/null || fail "the node does not serve the channel's keys"
  say "channel: $(cut -c1-300 "$WORK/channel.json")"
  extra=(); [ -n "${CROWN_VAULT:-}" ] && extra=(--vault "$CROWN_VAULT")
  timeout $(( DEADLINE_MIN * 60 )) "$BIN/crown-driver" crown-a "${D[@]}" --channel "$WORK/channel.json" \
    --genesis "$S/bedrock/deployment-settings.yaml" --identity-out "$WORK/session-a.json" "${extra[@]}" > "$LOG/driver.log" 2>&1
  DRIVER_RC=$?
else
  caps=(); for c in ${CROWN_CAPTURES:-$VOL/inputs/capture-1 $VOL/inputs/capture-2}; do caps+=(--capture "$c"); done
  extra=(); [ -n "${CROWN_ETH_FEES_AT:-}" ] && extra+=(--eth-fees-at "$CROWN_ETH_FEES_AT")
  [ -n "${CROWN_RECIPIENT:-}" ] && extra+=(--recipient "$CROWN_RECIPIENT")
  prover="${CROWN_R0VM:-$BIN/r0vm}"
  privacy="${CROWN_PRIVATE:-real}"
  devmode=(env -u RISC0_DEV_MODE); [ "$privacy" = fake ] && devmode=(env RISC0_DEV_MODE=1)
  timeout $(( DEADLINE_MIN * 60 )) "${devmode[@]}" RISC0_SERVER_PATH="$prover" nice -n 10 "$BIN/crown-driver" crown-b "${D[@]}" \
    --identity "$VOL/session-a/session-a.json" "${caps[@]}" --private "$privacy" \
    --session-out "$WORK/session-b.json" --notes-out "$WORK/burn-notes.json" "${extra[@]}" > "$LOG/driver.log" 2>&1
  DRIVER_RC=$?
fi
say "driver exit $DRIVER_RC"; tail -n 12 "$LOG/driver.log" | tee -a "$LOG/session.log"
if [ "$DRIVER_RC" = 0 ] && [ "$STEP" = b ]; then node_dump; fi
if [ "$DRIVER_RC" = 0 ]; then
  "$BIN/crown-driver" export "${D[@]}" --out "$WORK/export" > "$LOG/export.log" 2>&1 || say "export failed ($?)"
fi
head_block > "$WORK/head-block" || true
curl -fsS -m 5 "http://127.0.0.1:$BEDROCK_PORT/cryptarchia/info" > "$WORK/bedrock-end.json" 2>/dev/null || true
curl -fsS -m 5 "http://127.0.0.1:$BEDROCK_PORT/channel/$CHANNEL" > "$WORK/channel-end.json" 2>/dev/null || true

# ---- Stop cleanly, then save ----------------------------------------------------------------
CLEAN=1
stop_pid "$SEQ_PID" sequencer || CLEAN=0
sleep 8   # an inscription the sequencer sent last is still mined
stop_pid "$NODE_PID" node || CLEAN=0
grep -q -E 'Head rewound|expired unmined' "$LOG/sequencer.log" && say "the sequencer's log reports a head rewind or an inscription given up"
SAVED=""
if [ "$CLEAN" = 1 ] && [ "$DRIVER_RC" = 0 ]; then
  rm -rf "$S/bedrock/state/logs"
  NEXT=$(( GEN + 1 ))
  tar -C "$S" -cf - . | zstd -q -T0 -3 -o "$VOL/state/gen-$NEXT.tar.zst.part" -f \
    && mv "$VOL/state/gen-$NEXT.tar.zst.part" "$VOL/state/gen-$NEXT.tar.zst" \
    && echo "$NEXT" > "$VOL/state/current.part" && mv "$VOL/state/current.part" "$VOL/state/current" \
    && SAVED="$NEXT"
  say "saved generation ${SAVED:-NONE} ($(stat -c %s "$VOL/state/gen-$NEXT.tar.zst" 2>/dev/null) bytes; head block $(cat "$WORK/head-block" 2>/dev/null))"
else
  say "not saved (clean stop $CLEAN, driver exit $DRIVER_RC): the zone stays at generation $GEN"
fi

# ---- Records --------------------------------------------------------------------------------
cp "$WORK/bedrock-info.json" "$OUT/cryptarchia-info-start.json" 2>/dev/null
cp "$WORK/bedrock-end.json" "$OUT/cryptarchia-info.json" 2>/dev/null
cp "$WORK/channel-end.json" "$OUT/channel-end.json" 2>/dev/null
if [ "$STEP" = a ]; then
  cp "$WORK/channel.json" "$OUT/channel.json" 2>/dev/null
  cp "$S/bedrock/deployment-settings.yaml" "$OUT/deployment-settings.yaml"
  cp "$S/sequencer_home/channel_signing_key" "$OUT/channel_signing_key" 2>/dev/null || say "no channel_signing_key in the sequencer home"
  [ -n "$SAVED" ] && cp "$WORK/session-a.json" "$OUT/session-a.json"
else
  [ -n "$SAVED" ] && cp "$WORK/session-b.json" "$WORK/burn-notes.json" "$OUT/"
fi
[ -d "$WORK/export" ] && { rm -rf "$OUT/export"; cp -r "$WORK/export" "$OUT/export"; }
echo "node slot 0 at unix second $ORIGIN; sequencer starts $STARTS; blocks $AFTER s after an epoch second; scheduler delay $DELAY s" > "$OUT/phase.txt"
records
status=ok; [ "$DRIVER_RC" = 0 ] || status="driver-exit-$DRIVER_RC"; [ -n "$SAVED" ] || status="not-saved"
say "session $STEP: $status"
records
[ "$status" = ok ]
