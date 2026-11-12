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
#   session.sh u   restore the newest generation (after a); ferrata-updater, the build of lez/updater/sequencer
#                  ($CROWN_UPDATER_BIN, default $CROWN_LEZ_VOL/inputs/updater/ferrata-updater), keeps session A's Sepolia
#                  instance following Sepolia through the public beacon APIs (Lodestar's light-client data, publicnode's
#                  headers) on this zone's sequencer for CROWN_UPDATER_MIN minutes (default 60) or until
#                  /root/updater-stop exists; restarted once from its state after CROWN_UPDATER_RESTART_MIN minutes, as
#                  soon as it has opened a job (mid-job); /health and /metrics read every CROWN_UPDATER_PROBE_S (60) s;
#                  then stopped by SIGTERM, the chain exported by crown-driver, the zone saved; session-u/
#
# Environment: CROWN_LEZ_VOL (default /vol/lez), CROWN_LEZ_WORK (default /root/crown-lez, wiped), CROWN_CHAIN
# (sepolia | stand-in), CROWN_VAULT (a: optional), CROWN_CAPTURES (b: capture dirs in order; default
# $CROWN_LEZ_VOL/inputs/capture-1 and -2), CROWN_ETH_FEES_AT, CROWN_RECIPIENT, CROWN_PRIVATE (real | fake),
# CROWN_R0VM (the prover's r0vm; default the bundle's), CROWN_CLAIM_TIME_OFFSET_S (no effect
# since the time fix: a claim carries no time), CROWN_DEADLINE_MIN (120),
# CROWN_SAMPLE=1 (peak RSS and busy cores while the driver runs -> session-X/resources.txt).
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
case "$STEP" in a|b|dump|u) ;; *) echo "session.sh a | b | dump | u"; exit 2 ;; esac
case "$CHAIN" in sepolia|stand-in) ;; *) echo "CROWN_CHAIN: sepolia or stand-in"; exit 2 ;; esac

VOL="$(realpath -m "$VOL")"; WORK="$(realpath -m "$WORK")"
OUT="$VOL/session-$STEP"; [ "$STEP" = dump ] && OUT="$VOL/session-b"
# Each session U keeps its own records: session-u, then session-u2, session-u3, ...
if [ "$STEP" = u ]; then n=1; while [ -e "$OUT/updater.log" ]; do n=$(( n + 1 )); OUT="$VOL/session-u$n"; done; fi
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

# Resources while the driver runs (CROWN_SAMPLE=1): every 2 s the summed RSS of all processes and per command, and the
# busy cores from /proc/stat; the peaks go to $OUT/resources.txt (sizing the cloud container).
SAMPLER_PID=""
sample_start() {
  [ "${CROWN_SAMPLE:-0}" = 1 ] || return 0
  ( prev=""; while :; do
      # busy cores = the growth of all processes' summed CPU time over the wall time (nproc can understate a
      # container's CPU limit, so /proc/stat is not used); an exited process makes the sum drop: clamped at 0.
      now=$(date +%s.%N); cpu=$(ps -eo times= | awk '{t+=$1} END {print t+0}')
      if [ -n "$prev" ]; then set -- $prev; busy=$(awk -v c="$cpu" -v pc="$1" -v t="$now" -v pt="$2" 'BEGIN {b=(c-pc)/(t-pt); if (b<0) b=0; printf "%d", b*100}'); else busy=0; fi
      prev="$cpu $now"
      echo "$(date +%s) $busy $(ps -eo rss=,comm= | awk '{t+=$1; m[$2]+=$1} END {printf "%d", t; for (k in m) printf " %s=%d", k, m[k]}')"
      sleep 2
    done ) > "$LOG/resources.tsv" 2>/dev/null &
  SAMPLER_PID=$!
}
sample_stop() {
  [ -n "$SAMPLER_PID" ] || return 0
  kill "$SAMPLER_PID" 2>/dev/null; wait "$SAMPLER_PID" 2>/dev/null
  awk -v cores="$(nproc)" '{ if ($2 > pc) pc = $2; sc += $2; n++; if ($3 > pr) pr = $3;
      for (i = 4; i <= NF; i++) { split($i, kv, "="); if (kv[2] > mx[kv[1]]) mx[kv[1]] = kv[2] } }
    END { printf "samples %d (2 s), cores visible %d\npeak busy cores %.1f, mean busy cores %.1f\npeak summed RSS %d MiB\n", n, cores, pc/100, (n ? sc/n/100 : 0), pr/1024;
      for (k in mx) if (mx[k] > 102400) printf "  peak RSS %s %d MiB\n", k, mx[k]/1024 }' "$LOG/resources.tsv" > "$OUT/resources.txt"
  cat "$OUT/resources.txt" | tee -a "$LOG/session.log"
}

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

# ---- Session U: the updater daemon --------------------------------------------------------------
# LEZ's published test key of local.json's first genesis account (lez/live/driver/src/keys.rs: lez_test_payers()[2]);
# session A pays with [0] and [1], the bridge's fee recipient is [3].
UPDATER_PAYER=7f273098f25b71e6c005a9519f2678da8d1c7f01f6a27778e2d9948abdf901fb
UPD_PID=""; UPD_LIFE=0
updater_start() {
  UPD_LIFE=$(( UPD_LIFE + 1 ))
  echo "==== $(date -u +%FT%TZ) life $UPD_LIFE" >> "$LOG/updater.log"
  ( cd "$S/updater" && exec "$WORK/ferrata-updater" --config "$S/updater/updater.toml" ) >> "$LOG/updater.log" 2>&1 &
  UPD_PID=$!
  say "updater life $UPD_LIFE started (pid $UPD_PID)"
}
updater_stop() { # SIGTERM; the round in hand (a transaction waiting to be final) finishes first: up to 30 min
  local i rc
  [ -n "$UPD_PID" ] || return 0
  kill -TERM "$UPD_PID" 2>/dev/null
  for i in $(seq 1 1800); do kill -0 "$UPD_PID" 2>/dev/null || break; sleep 1; done
  if kill -0 "$UPD_PID" 2>/dev/null; then
    kill -KILL "$UPD_PID"; wait "$UPD_PID" 2>/dev/null; say "updater life $UPD_LIFE killed after 30 min"; UPD_PID=""; return 1
  fi
  wait "$UPD_PID"; rc=$?
  say "updater life $UPD_LIFE stopped by SIGTERM after $i s: exit $rc"
  echo "==== $(date -u +%FT%TZ) life $UPD_LIFE exit $rc" >> "$LOG/updater.log"
  UPD_PID=""
  return "$rc"
}
updater_probe() { # /health and /metrics: one JSON line per probe; the full texts every 10th probe and the last
  local t code n="$1" P="$OUT/probes"
  t="$(date -u +%FT%TZ)"
  code="$(curl -sS -m 5 -o "$WORK/health.json" -w '%{http_code}' http://127.0.0.1:9464/health 2>/dev/null)"
  curl -fsS -m 5 -o "$WORK/metrics.txt" http://127.0.0.1:9464/metrics 2>/dev/null || : > "$WORK/metrics.txt"
  python3 -I "$WORK/probe.py" "$t" "${code:-000}" "$WORK/health.json" "$WORK/metrics.txt" "$UPD_LIFE" >> "$P/health.jsonl"
  [ -s "$WORK/metrics.txt" ] && cp "$WORK/metrics.txt" "$OUT/metrics-last.txt"
  [ -s "$WORK/health.json" ] && cp "$WORK/health.json" "$OUT/health-last.json"
  if [ $(( n % 10 )) = 1 ] && [ -s "$WORK/metrics.txt" ]; then
    cp "$WORK/metrics.txt" "$P/metrics-$t.txt"; cp "$WORK/health.json" "$P/health-$t.json" 2>/dev/null
  fi
  return 0
}
updater_mirror() { # the records so far, to the Volume
  local J="$S/updater/state/sepolia"
  cp "$LOG/updater.log" "$OUT/updater.log" 2>/dev/null
  [ -f "$J/journal.tsv" ] && cp "$J/journal.tsv" "$OUT/journal.tsv"
  [ -f "$S/updater/state/ledger/run.tsv" ] && cp "$S/updater/state/ledger/run.tsv" "$OUT/run.tsv"
  return 0
}
journal_lines() { if [ -f "$S/updater/state/sepolia/journal.tsv" ]; then wc -l < "$S/updater/state/sepolia/journal.tsv"; else echo 0; fi; }
journal_tail() { tail -n 1 "$S/updater/state/sepolia/journal.tsv" 2>/dev/null | cut -f2,5 | tr '\t' ' '; }
updater_session() {
  local U="$S/updater" bin="${CROWN_UPDATER_BIN:-$VOL/inputs/updater/ferrata-updater}" minutes="${CROWN_UPDATER_MIN:-60}"
  local restart_min="${CROWN_UPDATER_RESTART_MIN:-}" probe_s="${CROWN_UPDATER_PROBE_S:-60}" poll_s="${CROWN_UPDATER_POLL_S:-24}"
  local program t0 end now last_probe=0 n=0 restart=0 armed_at=0 armed_lines=0 lines rc=0
  mkdir -p "$U" "$OUT/probes"
  cp "$bin" "$WORK/ferrata-updater" 2>/dev/null && chmod +x "$WORK/ferrata-updater" || { say "no updater binary at $bin"; return 3; }
  say "updater binary sha256 $(sha256sum "$WORK/ferrata-updater" | cut -c1-64)"
  program="$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["attestation"]["account"])' "$VOL/session-a/session-a.json")" \
    || { say "no attestation account in session-a/session-a.json"; return 3; }
  [ -f "$U/updater.toml" ] && say "the updater's state from an earlier session U is kept ($(du -sh "$U/state" 2>/dev/null | cut -f1))"
  printf '%s\n' "$UPDATER_PAYER" > "$U/payer.key"
  printf '%s\n' \
    '# Written by lez/crown/session.sh u: the zone of session A (crown-driver crown-a deployed the attestation program and' \
    '# initialised the Sepolia instance from crates/ferrata-spectests/fixtures/sepolia-2026-11-12 and its period-1411 update).' \
    '[daemon]' 'name = "updater-live"' 'state_dir = "state"' "poll_secs = $poll_s" 'listen = "127.0.0.1:9464"' 'ledger = "sequencer"' '' \
    '[sequencer]' "rpc = \"http://127.0.0.1:$RPC_PORT\"" "program = \"$program\"" 'wait_final = true' "log = \"$LOG/sequencer.log\"" '' \
    '[keys]' 'payer_key_file = "payer.key"' '' \
    '[fees]' 'gas_limit = 4800000' 'max_fee_per_tx = 1000000000' 'budget = 1000000000000' '' \
    '[policy]' 'defer_window = 1' 'max_lag_slots = 64' 'min_agreeing = 2' 'retry_base_ms = 1000' 'retry_max_ms = 60000' \
    'max_attempts = 5' 'demote_rounds = 50' 'max_periods_per_request = 4' 'finish_own_jobs = true' '' \
    '[[chain]]' 'name = "sepolia"' 'preset = "sepolia"' 'checkpoint = "3b901f5e6853b5cf5eb04d7ccfe1272e66c38291cb3a11dd159cacd1117ccf7c"' \
    'layout = "gloas"' 'adoption_parts = 2' '' \
    '  [[chain.source]]' '  name = "lodestar"' '  url = "https://lodestar-sepolia.chainsafe.io"' '  timeout_secs = 30' '' \
    '  [[chain.source]]' '  name = "publicnode"' '  url = "https://ethereum-sepolia-beacon-api.publicnode.com"' '  kind = "headers"' \
    '  timeout_secs = 30' > "$U/updater.toml"
  sed -e "s#$LOG/#work/logs/#" "$U/updater.toml" > "$OUT/updater.toml"
  printf '%s\n' \
    'import json, sys' \
    't, code, health, metrics, life = sys.argv[1:6]' \
    'try:' '    h = json.load(open(health))' 'except Exception:' '    h = None' \
    'm = {}' \
    'for line in open(metrics, errors="replace"):' \
    '    if line.startswith("ferrata_updater_") and " " in line:' \
    '        k, v = line.rsplit(" ", 1)' '        m[k] = v.strip()' \
    'def pick(name):' '    return {k.split("{", 1)[1].rstrip("}"): v for k, v in m.items() if k.startswith(name + "{")}' \
    'print(json.dumps({"t": t, "life": int(life), "http": int(code), "health": h, "tx": pick("ferrata_updater_tx_total"),' \
    '                  "fee": pick("ferrata_updater_fee_total"), "available": pick("ferrata_updater_source_available"),' \
    '                  "confirmed": pick("ferrata_updater_source_confirmed_total")}, separators=(",", ":")))' > "$WORK/probe.py"
  t0=$(date +%s); end=$(( t0 + minutes * 60 ))
  say "session U: $minutes min (until $(date -u -d "@$end" +%FT%TZ)); restart after ${restart_min:-never} min; probe every $probe_s s"
  updater_start
  while :; do
    now=$(date +%s)
    if [ -f /root/updater-stop ]; then say "stop requested: $(head -c 200 /root/updater-stop)"; break; fi
    if [ "$now" -ge "$end" ]; then say "the session's $minutes min are over"; break; fi
    if ! kill -0 "$UPD_PID" 2>/dev/null; then
      wait "$UPD_PID"; rc=$?; say "the updater exited by itself (life $UPD_LIFE, exit $rc)"; UPD_PID=""
      updater_mirror
      if [ "$UPD_LIFE" -ge 4 ]; then say "four lives: giving up"; break; fi
      sleep 30; updater_start; continue
    fi
    if [ $(( now - last_probe )) -ge "$probe_s" ]; then n=$(( n + 1 )); updater_probe "$n"; updater_mirror; last_probe=$now; fi
    # The restart: armed after restart_min; done as soon as the updater has applied an Open (a job half done), or
    # after 20 min armed, whatever it is doing.
    if [ -n "$restart_min" ] && [ "$restart" = 0 ] && [ "$now" -ge $(( t0 + restart_min * 60 )) ]; then
      restart=1; armed_at=$now; armed_lines=$(journal_lines)
      say "restart armed (journal at $armed_lines lines)"
    fi
    if [ "$restart" = 1 ]; then
      lines=$(journal_lines)
      if { [ "$lines" -gt "$armed_lines" ] && [ "$(journal_tail)" = "open applied" ]; } || [ $(( now - armed_at )) -ge 1200 ]; then
        say "restart: the journal's last line is '$(journal_tail)'; SIGTERM"
        n=$(( n + 1 )); updater_probe "$n"
        updater_stop; rc=$?
        updater_mirror
        cp "$U/state/sepolia/journal.tsv" "$OUT/journal-at-restart.tsv" 2>/dev/null
        ls -la "$U/state" "$U/state/sepolia" "$U/state/sepolia/open" "$U/state/ledger" > "$OUT/state-at-restart.txt" 2>&1
        restart=2; sleep 15
        updater_start
        last_probe=0
        continue
      fi
    fi
    sleep 1
  done
  n=$(( n + 1 )); updater_probe "$n"
  updater_stop; rc=$?
  updater_mirror
  ls -la "$U/state" "$U/state/sepolia" "$U/state/sepolia/open" "$U/state/ledger" > "$OUT/state-at-end.txt" 2>&1
  say "session U: updater lives $UPD_LIFE, last exit $rc, journal $(journal_lines) transactions"
  return "$rc"
}

# ---- The driver -----------------------------------------------------------------------------
D=(--rpc "http://127.0.0.1:$RPC_PORT" --run-dir "$S/driver" --program-bin "$B/program/ferrata_lez.bin"
   --bridge-bin "$B/program/ferrata_bridge.bin" --chain "$CHAIN" --fixture "$B/fixture" --sequencer-log "$LOG/sequencer.log")
DRIVER_RC=1
sample_start
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
elif [ "$STEP" = u ]; then
  updater_session
  DRIVER_RC=$?
else
  caps=(); for c in ${CROWN_CAPTURES:-$VOL/inputs/capture-1 $VOL/inputs/capture-2}; do caps+=(--capture "$c"); done
  extra=(); [ -n "${CROWN_ETH_FEES_AT:-}" ] && extra+=(--eth-fees-at "$CROWN_ETH_FEES_AT")
  [ -n "${CROWN_RECIPIENT:-}" ] && extra+=(--recipient "$CROWN_RECIPIENT")
  [ -n "${CROWN_CLAIM_TIME_OFFSET_S:-}" ] && extra+=(--claim-time-offset-s "$CROWN_CLAIM_TIME_OFFSET_S")
  prover="${CROWN_R0VM:-$BIN/r0vm}"
  privacy="${CROWN_PRIVATE:-real}"
  devmode=(env -u RISC0_DEV_MODE); [ "$privacy" = fake ] && devmode=(env RISC0_DEV_MODE=1)
  timeout $(( DEADLINE_MIN * 60 )) "${devmode[@]}" RISC0_SERVER_PATH="$prover" nice -n 10 "$BIN/crown-driver" crown-b "${D[@]}" \
    --identity "$VOL/session-a/session-a.json" "${caps[@]}" --private "$privacy" \
    --session-out "$WORK/session-b.json" --notes-out "$WORK/burn-notes.json" "${extra[@]}" > "$LOG/driver.log" 2>&1
  DRIVER_RC=$?
fi
sample_stop
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
elif [ "$STEP" = b ]; then
  [ -n "$SAVED" ] && cp "$WORK/session-b.json" "$WORK/burn-notes.json" "$OUT/"
fi
[ -d "$WORK/export" ] && { rm -rf "$OUT/export"; cp -r "$WORK/export" "$OUT/export"; }
echo "node slot 0 at unix second $ORIGIN; sequencer starts $STARTS; blocks $AFTER s after an epoch second; scheduler delay $DELAY s" > "$OUT/phase.txt"
records
status=ok; [ "$DRIVER_RC" = 0 ] || status="driver-exit-$DRIVER_RC"; [ -n "$SAVED" ] || status="not-saved"
say "session $STEP: $status"
records
[ "$status" = ok ]
