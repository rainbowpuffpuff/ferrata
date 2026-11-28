#!/usr/bin/env bash
# The v0.4.0 clients against a running LEZ v0.4.0 sequencer: LEZ's own `sequencer_service` built with its `standalone`
# feature (its mock Bedrock actor, no node) from LEZ's tree at v0.4.0, on 127.0.0.1 only, and this tree's clients:
#   1. `ferrata lez-version` (cli/v040/build.sh's build): the network is v0.4.0, as the build is (exit 0);
#   2. deploy/testnet/web/wallet/test/live040.mjs: the web wallet's v0.4.0 codec sends two public transfers from LEZ's
#      published local test account (the second at nonce 1, on the key the first rotated to), reads each receipt and
#      block, decoded by the wallet's and the explorer's v0.4.0 decoders;
#   3. scripts/lez-read.py (--lez-version auto): the head block and each transfer's block, decoded as v0.4.0 with
#      their receipts.
# For a build box (heavy: two release builds). Nothing leaves the machine; the sequencer is stopped at the end.
#
#   cli/v040/live.sh SRC OUT     SRC: a copy of the tree prepared as cli/v040/build.sh needs (LEZ v0.4.0 at
#                                SRC/../upstream/lez-v0.4.0); OUT: the records' directory (facts.json, logs/, blocks)
set -uo pipefail
SRC="$(cd "${1:?usage: $0 SRC OUT}" && pwd)"; OUT="$(mkdir -p "${2:?usage: $0 SRC OUT}" && cd "$2" && pwd)"
LEZ="$SRC/../upstream/lez-v0.4.0"; T="${LIVE_TARGET:-$OUT/../target-live}"; PORT="${LIVE_PORT:-19390}"
R="http://127.0.0.1:$PORT"; mkdir -p "$OUT/logs" "$T"
log() { echo "[$(date -u +%H:%M:%SZ)] $*" | tee -a "$OUT/logs/live.log"; }
die() { log "STOP: $*"; exit 1; }
PIDS=(); trap 'for p in "${PIDS[@]}"; do kill "$p" 2>/dev/null; done' EXIT

log "LEZ $(git -C "$LEZ" rev-parse HEAD 2>/dev/null || echo "v0.4.0 tree at $LEZ")"
log "build: sequencer_service --features standalone (LEZ's tree, release) and ferrata (cli/v040/build.sh build)"
( cd "$LEZ" && CARGO_TARGET_DIR="$T/lez" cargo build --release --locked -p sequencer_service --features standalone ) \
  > "$OUT/logs/build-sequencer.log" 2>&1 &
b1=$!
CARGO_TARGET_DIR="$T/cli" bash "$SRC/cli/v040/build.sh" "$SRC" build > "$OUT/logs/build-ferrata.log" 2>&1 &
b2=$!
wait $b1 || { tail -n 30 "$OUT/logs/build-sequencer.log"; die "sequencer_service did not build"; }
wait $b2 || { tail -n 30 "$OUT/logs/build-ferrata.log"; die "ferrata did not build"; }
SEQ="$T/lez/release/sequencer_service"; FERRATA="$T/cli/release/ferrata"
log "built: sequencer_service $(sha256sum "$SEQ" | cut -c1-16)…, ferrata $(sha256sum "$FERRATA" | cut -c1-16)…"
# LEZ v0.4.0 executes its genesis transactions in r0vm 3.1.0 (LEZ #1124): rzup's, as checkpoint/attestor/zone-bundle.sh
# installs it.
find_r0vm() { for f in "$HOME"/.risc0/extensions/v3.1.0-cargo-risczero-*/r0vm; do [ -x "$f" ] && { echo "$f"; return 0; }; done; return 1; }
R0VM="$(find_r0vm || true)"
if [ -z "$R0VM" ]; then RISC0_HOME="$HOME/.risc0" rzup install r0vm 3.1.0 > "$OUT/logs/rzup.log" 2>&1 || { tail -n 20 "$OUT/logs/rzup.log"; die "rzup install r0vm 3.1.0"; }; R0VM="$(find_r0vm)" || die "no r0vm 3.1.0 after rzup"; fi
export RISC0_SERVER_PATH="$R0VM"; log "r0vm: $("$R0VM" --version 2>&1 | head -1) ($R0VM)"

# LEZ's local network configuration, its gossip and metrics on loopback (as the zone scripts run it).
H="$OUT/seq"; cfg="$OUT/seq-config.json"
python3 -I - "$LEZ/lez/sequencer/service/networks/local.json" "$cfg" "$PORT" <<'PY'
import json, sys
c = json.load(open(sys.argv[1]))
p = int(sys.argv[3])
c["gossip"] = {"listen_addr": f"/ip4/127.0.0.1/udp/{p - 200}/quic-v1", "bootstrap_peers": []}
c["metrics_listen_address"] = f"127.0.0.1:{p - 100}"
json.dump(c, open(sys.argv[2], "w"), indent=2)
PY
FUND=2e03b2eff5a45478e7e79668d2a146cf2c5c7925bce927f2b1c67f2ab4fc0d26  # the dev node's funded key (LEZ's compose default)
"$SEQ" setup "$H" --config "$cfg" --funding-public-key "$FUND" --create-channel > "$OUT/logs/setup.log" 2>&1 \
  || { tail -n 20 "$OUT/logs/setup.log"; die "sequencer setup"; }
( export RUST_LOG="info,risc0_zkvm=warn,kameo=warn"; exec env -u RISC0_DEV_MODE "$SEQ" start "$H" --rpc-listen-address "127.0.0.1:$PORT" ) \
  > "$OUT/logs/sequencer.log" 2>&1 &
PIDS+=($!)
rpc() { curl -s -m 10 -H 'content-type: application/json' -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$1\",\"params\":${2:-[]}}" "$R"; }
h=""; for _ in $(seq 1 180); do
  h="$(rpc getLastBlockId | python3 -I -c 'import json,sys; print(json.load(sys.stdin).get("result",""))' 2>/dev/null)"
  [ -n "$h" ] && [ "$h" -ge 2 ] 2>/dev/null && break; kill -0 "${PIDS[0]}" 2>/dev/null || die "the sequencer exited"; sleep 1
done
[ -n "$h" ] && [ "$h" -ge 2 ] 2>/dev/null || die "no block 2 from the sequencer"
log "sequencer up at $R: block $h; getSequencerStatus: $(rpc getSequencerStatus)"

rc_all=0
log "1. ferrata lez-version"
"$FERRATA" --sequencer "$R" lez-version > "$OUT/logs/lez-version.out" 2>&1; rc=$?
cat "$OUT/logs/lez-version.out" | tee -a "$OUT/logs/live.log"; log "ferrata lez-version: exit $rc"; [ $rc = 0 ] || rc_all=1
log "2. the web wallet's codec: two transfers (live040.mjs)"
node "$SRC/deploy/testnet/web/wallet/test/live040.mjs" "$R" "$OUT/wallet.json" 2>&1 | tee "$OUT/logs/wallet.out" | tee -a "$OUT/logs/live.log"
rc=${PIPESTATUS[0]}; log "live040.mjs: exit $rc"; [ $rc = 0 ] || rc_all=1
log "3. lez-read"
for what in "head" $(python3 -I -c 'import json,sys; print(" ".join("block %d" % s["block"] for s in json.load(open(sys.argv[1])).get("sends", [])))' "$OUT/wallet.json" 2>/dev/null); do
  [ "$what" = block ] && continue
  case "$what" in head) args=(head) ;; *) args=(block "$what") ;; esac
  name="lez-read-$(IFS=-; echo "${args[*]}")"
  python3 -I "$SRC/scripts/lez-read.py" --lez "$R" --json "${args[@]}" > "$OUT/$name.json" 2> "$OUT/logs/$name.err"
  rc=$?; log "lez-read ${args[*]}: exit $rc $(head -c 200 "$OUT/logs/$name.err")"; [ $rc = 0 ] || rc_all=1
done
mkdir -p "$OUT/blocks"; h="$(rpc getLastBlockId | python3 -I -c 'import json,sys; print(json.load(sys.stdin)["result"])')"
log "sequencer at block $h; status $(rpc getSequencerStatus)"
python3 -I - "$OUT" "$rc_all" "$h" <<'PY'
import json, sys, hashlib
out, rc, h = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
w = json.load(open(f"{out}/wallet.json")) if __import__("os").path.exists(f"{out}/wallet.json") else {}
for s in w.get("sends", []):
    open(f"{out}/blocks/block-{s['block']}.json", "w").write(json.dumps(s.pop("block_b64")))
json.dump(w, open(f"{out}/wallet.json", "w"), indent=1)
json.dump({"ok": rc == 0, "last_height": h, "lez": "v0.4.0 a255b42c (standalone sequencer_service, LEZ's tree)",
           "wallet_ok": w.get("ok"), "sends": [{k: s[k] for k in ("hash", "nonce", "amount", "block", "receipt")} for s in w.get("sends", [])]},
          open(f"{out}/facts.json", "w"), indent=1)
PY
log "done: $([ $rc_all = 0 ] && echo ok || echo 'a check failed')"
exit $rc_all
