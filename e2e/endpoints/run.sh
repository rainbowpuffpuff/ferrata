#!/usr/bin/env bash
# RFP-022 U.9 inside Logos Core: the mini-app's "Apply and test" switching the LEZ sequencer and the Ethereum RPC through
# the modules that hold the connections, between live endpoints, in ONE headless Logos Core (e2e/endpoints/README.md).
# On the build box (spike/modal-build's mrun); never on a laptop.
#
#   e2e/endpoints/run.sh build       seqnet-chain (cli/) and crown-driver (lez/crown/driver), the vault (forge), the
#                                    module's native tests and its dev .lgx with logosctl (modules/scripts/check.sh),
#                                    eth_rpc_module's dev .lgx (modules/scripts/send-stack.sh, ONLY=eth_rpc_module), the
#                                    Qt Quick Test runner (qmltest-logos); /nix saved for the next container
#   e2e/endpoints/run.sh live NAME   the run; records in e2e/runs/endpoints-NAME/
#
# The live run:
#   1. A LEZ zone of four sequencers on one Logos Blockchain node (upstream's four-sequencer devnet, as
#      checkpoint/attestor/zone-bridge.sh sets it up; LEZ v0.4.0-rc1), the attestation program and the bridge program B1
#      deployed on it (crown-driver crown-a, the STAND-IN beacon chain), a local anvil (chain id 11155111, no fork) with
#      the v5 vault paired with that configuration, one deposit, and the operator's pairing, registration and import
#      (crown-driver crown-b --operator-only): the session's configuration is on the zone, which every one of the four
#      sequencers serves. A second anvil forked from the first (127.0.0.1:8546): another live endpoint of the same chain.
#   2. Scenario L (the plain path; local): seqnet-chain on sequencer 0 and anvil holds both connections. The view
#      switches the sequencer between sequencers 0, 1 and 2 and the Ethereum RPC between the two anvils, and is refused,
#      each time keeping the endpoints in use: a closed port, a sequencer of another zone (with U9_PUBLIC=1 our public
#      network, read only), Sepolia's public RPC (no vault there; read only), Hoodi's (another chain; read only), and a
#      pair of which one is bad (neither switches).
#   3. Scenario E (U9_PUBLIC=1; read only): the live deployment, the vault on Sepolia (0x19499061…2787, configuration
#      0x3773b588…67cd) and our public LEZ network (its RPC URL in U9_LIVE_LEZ_URL), through a second seqnet-chain;
#      the session's Ethereum side through Logos Core's eth_rpc_module (`wallet eth`). The view switches eth_rpc_module's
#      endpoint for chain 11155111 between two public Sepolia RPCs and back, and is refused: Hoodi's RPC, a closed port,
#      and a local sequencer (its zone does not hold that configuration). Nothing is sent to any network: the session
#      reads, and no transaction is built.
#   Every module call of the view is recorded (calls.jsonl, the runner's logosctl calls), each owner's state is read
#   independently after each step (ops.sh: the chain service's module.endpoints, eth_rpc_module's get_chain_config), and
#   the Endpoints tab is grabbed at each state.
#
# Inputs (put on the box with mrun --put): U9_BUNDLE (a crown LEZ bundle: sequencer_service, the node, r0vm, the
# attestation program; default spike/tmp/crown-fresh/lez/bundle.tar.zst, as checkpoint/attestor/zone-bridge.sh's) and
# U9_BRIDGE_BIN (this tree's bridge program of docs/image-ids.md, ferrata_bridge_lezfee, sha256 57b0c84b…, as scripts/reproduce-image-ids.sh builds
# it; default inputs/ferrata_bridge.bin in the scratch directory U9_TMP), which replaces the bundle's.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
SPIKE="$(cd "$REPO/.." && pwd)"
LEZ="$SPIKE/upstream/lez-v0.4.0-rc1"; DEV="$LEZ/lez/configs/docker-all-in-one/devnet"
T="${U9_TMP:-$SPIKE/tmp/endpoints}"
TGT="$T/target-lez"
BUNDLE="${U9_BUNDLE:-$SPIKE/tmp/crown-fresh/lez/bundle.tar.zst}"
BRIDGE_IN="${U9_BRIDGE_BIN:-$T/inputs/ferrata_bridge.bin}"
BRIDGE_SHA256=57b0c84b9775021b39fcfd92649fe68684d9e5d6bf2c65518699a4efb649c118            # docs/image-ids.md ferrata_bridge_lezfee (this tree's since ADR 0048; ADR 0046's and B1's before)
LOGOS_SHORT="$SPIKE/tmp/u9s"                                                          # Logos Core's sockets: 62 bytes at most
CHANNEL=0101010101010101010101010101010101010101010101010101010101010101
FUND=2e03b2eff5a45478e7e79668d2a146cf2c5c7925bce927f2b1c67f2ab4fc0d26                # the dev node's funded key (compose default)
PORT=18080; API="http://127.0.0.1:$PORT"; R=http://127.0.0.1:8545; R2=http://127.0.0.1:8546
INTERVAL=10; TICK_MIN=1; TICK_MAX=6; TICK_AFTER=3; MAX_STARTS=4                        # crown session.sh's phase rule
CHAIN_ID=11155111
# The live deployment (scenario E): the vault on Sepolia and its configuration on our public LEZ network.
LIVE_VAULT=0x19499061CeC0BaC2a3E1869Ba5AE6B403aA82787; LIVE_FROM=12196188
LIVE_BRIDGE=0x1f2e94ffdf25a3feca4e0750be66700be3812434c8cf53e0eb301bc493336195       # 36iqVMFp…H1rk, bridge B1
# Our public LEZ network's RPC (U9_PUBLIC=1): given by whoever runs it. Its host names a person, so the records write it
# as public-lez.example (records_final) and the repository does not hold it.
LIVE_LEZ="${U9_LIVE_LEZ_URL:-}"
SEPOLIA_A=https://ethereum-sepolia-rpc.publicnode.com; SEPOLIA_B=https://rpc.sepolia.ethpandaops.io
HOODI=https://ethereum-hoodi-rpc.publicnode.com
export PATH="$HOME/.foundry/bin:$HOME/.cargo/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1 CARGO_TERM_COLOR=never
unset PUBNET_SEPOLIA_RPC
say() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "${RUN:-$T}/run.log"; }
die() { say "STOP: $*"; exit 1; }

build() {
  local L="$T/logs/build"; mkdir -p "$L" "$T/bin" "$T/logos"
  say "cli: seqnet-chain; lez/crown/driver: crown-driver (LEZ's crates, Rust 1.98.1)"
  ( cd "$REPO/cli" && CARGO_TARGET_DIR="$TGT" cargo build --release --locked --features seqnet --bin seqnet-chain ) > "$L/chain.log" 2>&1 \
    || { grep -E -A7 '^error' "$L/chain.log" | head -60; die "seqnet-chain"; }
  ( cd "$REPO/lez/crown/driver" && { [ -f Cargo.lock ] || cp "$REPO/lez/live/driver/Cargo.lock" Cargo.lock; } \
      && RISC0_HOME="$SPIKE/risc0-home-lez" CARGO_TARGET_DIR="$TGT" cargo build --release ) > "$L/driver.log" 2>&1 \
    || { grep -E -A7 '^error' "$L/driver.log" | head -60; die "crown-driver"; }
  cp "$TGT/release/seqnet-chain" "$TGT/release/crown-driver" "$T/bin/"
  say "vault: libraries and build"
  ( cd "$REPO/vault" && { [ -d lib/risc0-ethereum ] || forge install --no-git foundry-rs/forge-std@v1.10.0 \
      OpenZeppelin/openzeppelin-contracts@v5.4.0 risc0/risc0-ethereum@v3.0.0; } && forge build ) > "$L/vault.log" 2>&1 \
    || { tail -n 20 "$L/vault.log"; die "vault build"; }
  say "the qmltest runner (Qt 6)"
  ( cmake -S "$REPO/modules/ferrata_bridge_ui/qmltest-logos" -B "$T/qmltest-build" -DCMAKE_BUILD_TYPE=Release \
      && cmake --build "$T/qmltest-build" -j 8 ) > "$L/runner.log" 2>&1 || { tail -n 30 "$L/runner.log"; die "the qmltest runner"; }
  cp "$T/qmltest-build/ferrata_miniapp_logos" "$T/bin/"
  say "ferrata_bridge_module: native tests and calls, the dev .lgx and logosctl (modules/scripts/check.sh)"
  local O="$T/modcheck" sdk="$REPO/modules/ferrata_bridge_module/logos-rust-sdk-src"
  # scripts/ci/modules-resolve.sh links the SDK's source from spike/upstream; check.sh builds it with Nix into that path.
  if [ -L "$sdk" ] && ! readlink "$sdk" | grep -q '^/nix/store/'; then rm -f "$sdk"; fi
  MODULES=ferrata_bridge_module PORTABLE=0 E2E=0 LOGOS_TMPDIR="$LOGOS_SHORT" RISC0_DEV_MODE=1 \
    bash "$REPO/modules/scripts/check.sh" "$O" > "$L/check.log" 2>&1 || { tail -n 40 "$L/check.log"; die "check.sh"; }
  tail -n 12 "$O/report/summary.md" 2>/dev/null || true
  rm -rf "$T/logos/lgx-ferrata_bridge_module"; cp -rL "$O/lgx-ferrata_bridge_module" "$T/logos/lgx-ferrata_bridge_module"
  ln -sfn "$(readlink -f "$O/logos/bin/logosctl")" "$T/logos/logosctl"
  say "eth_rpc_module (modules/scripts/send-stack.sh, ONLY=eth_rpc_module)"
  ONLY=eth_rpc_module bash "$REPO/modules/scripts/send-stack.sh" build "$T/stack" > "$L/send-stack.log" 2>&1 \
    || { tail -n 30 "$L/send-stack.log"; die "eth_rpc_module"; }
  [ -d "$T/stack/lgx-eth_rpc_module" ] || die "no eth_rpc_module .lgx"
  ( cd "$T" && sha256sum logos/lgx-ferrata_bridge_module/*.lgx stack/lgx-eth_rpc_module/*.lgx bin/*; logos/logosctl --version 2>&1 | head -1 ) \
    | tee "$T/logos/VERSIONS"
  say "caching /nix for the next container (/vol/nix/u9-endpoints.tar.zst; the setup script restores it)"
  mkdir -p /vol/nix && tar -C / -cf - nix | zstd -q -T0 -3 -o /vol/nix/u9-endpoints.tar.zst.part -f \
    && mv /vol/nix/u9-endpoints.tar.zst.part /vol/nix/u9-endpoints.tar.zst && ls -la /vol/nix/u9-endpoints.tar.zst
}

# ---- the zone (checkpoint/attestor/zone-bridge.sh's set-up, without attestors or the user's claims) -------------------
SEQ_PID=(); NODE_PID=""; ANVIL_PID=""; ANVIL2_PID=""; CHAIN_PIDS=(); LOOP_PID=""
stop_pid() { # pid name [seconds]: SIGTERM, then SIGKILL
  local pid="$1" name="$2" s="${3:-30}" i
  [ -n "$pid" ] || return 0
  kill -0 "$pid" 2>/dev/null || { wait "$pid" 2>/dev/null || true; return 0; }
  kill -TERM "$pid" 2>/dev/null || true
  for i in $(seq 1 $(( s * 2 ))); do kill -0 "$pid" 2>/dev/null || break; sleep 0.5; done
  if kill -0 "$pid" 2>/dev/null; then kill -KILL "$pid" 2>/dev/null || true; say "$name did not stop in $s s: killed"; fi
  wait "$pid" 2>/dev/null || true
}
scrub() { sed -e "s#$W/#work/#g" -e "s#$T/#tmp/#g" -e "s#$REPO/##g" -e "s#$SPIKE/tmp/[^/][^/]*/#tmp/#g" -e "s#$SPIKE/#spike/#g" -e "s#$HOME/#~/#g"; }
cleanup() {
  set +e
  [ -n "${LOGOSCTL:-}" ] && [ -d "${S:-/nonexistent}" ] && TMPDIR="$LOGOS_SHORT" "$LOGOSCTL" --config-dir "$S" daemon stop > /dev/null 2>&1
  for p in "${CHAIN_PIDS[@]:-}"; do [ -n "$p" ] && stop_pid "$p" chain-service 10; done
  [ -n "$LOOP_PID" ] && kill "$LOOP_PID" 2>/dev/null
  for p in "${SEQ_PID[@]:-}"; do [ -n "$p" ] && kill -TERM "$p" 2>/dev/null; done
  [ -n "$ANVIL2_PID" ] && kill -INT "$ANVIL2_PID" 2>/dev/null
  [ -n "$ANVIL_PID" ] && kill -INT "$ANVIL_PID" 2>/dev/null
  sleep 5
  for p in "${SEQ_PID[@]:-}"; do [ -n "$p" ] && kill -KILL "$p" 2>/dev/null; done
  [ -n "$NODE_PID" ] && stop_pid "$NODE_PID" node 10
  [ -n "$ANVIL2_PID" ] && kill -KILL "$ANVIL2_PID" 2>/dev/null
  [ -n "$ANVIL_PID" ] && kill -KILL "$ANVIL_PID" 2>/dev/null
  mkdir -p "$RUN/logs"
  for f in "$W"/logs/*.log; do
    [ -f "$f" ] || continue
    case "$(basename "$f")" in
      seq-*.log|bedrock.log) sed -e 's/\x1b\[[0-9;]*m//g' "$f" | scrub | tail -n 400 | gzip -c > "$RUN/logs/$(basename "$f").gz" ;;
      *) scrub < "$f" | tail -n 2000 > "$RUN/logs/$(basename "$f")" ;;
    esac
  done
  records_final
}

# Every file of the run (logs/ and net/ too, which e2e/crown/sepolia.sh writes): this machine's paths rewritten, our public
# network's host written public-lez.example, then refused if any of them, a home directory or an agent's folder remains.
records_final() {
  local host="" deny="${FERRATA_SCRUB_DENY:-}" f
  if [ -n "$LIVE_LEZ" ]; then
    host="$(echo "$LIVE_LEZ" | sed -E 's#^https?://([^/:]+).*#\1#')"
    grep -rlI --null -F "$host" "$RUN" | while IFS= read -r -d '' f; do sed -i "s#$host#public-lez.example#g" "$f"; done
    deny="$(printf '%s' "$host" | sed 's/[.]/\\./g')${deny:+|$deny}"
  fi
  FERRATA_SCRUB_DENY="$deny" python3 -I "$REPO/scripts/scrub_records.py" "$RUN" --map "$W/=work/" --map "$T/=tmp/" \
    --map "$REPO/=" --scratch "$SPIKE/tmp=tmp/" --map "$SPIKE/=spike/" --map "$HOME/=~/" || say "records_final: a record still names a path or a name (above)"
  # A screenshot showing the public network's address in a field cannot be rewritten: it is kept out of the records.
  if [ -n "$host" ] && [ "${U9_PUBLIC:-0}" = 1 ]; then
    mkdir -p "$W/screenshots-with-host"
    for f in "$RUN"/screenshots/0[46789]-*.png; do [ -f "$f" ] && mv "$f" "$W/screenshots-with-host/"; done
  fi
}

write_cfg() { # i: upstream's devnet sequencer configuration, loopback addresses, genesis plus local.json's funded accounts
  python3 -I - "$DEV/sequencer_config.json" "$LEZ/lez/sequencer/service/networks/local.json" "$W/cfg-$1.json" "$API" "$1" <<'PY'
import json, sys
src, local, dst, api, i = sys.argv[1:6]; i = int(i)
c = json.load(open(src)); l = json.load(open(local))
have = {json.dumps(a, sort_keys=True) for a in c["genesis"]["actions"]}
extra = [a for a in l["genesis"]["actions"] if json.dumps(a, sort_keys=True) not in have]
assert all("supply_account" in a for a in extra), "only local.json's public test accounts are added"
c["genesis"]["actions"] += extra
c["bedrock_config"]["node_url"] = api
c["gossip"] = {"listen_addr": f"/ip4/127.0.0.1/udp/{19100 + i}/quic-v1",
               "bootstrap_peers": [f"/ip4/127.0.0.1/udp/{19100 + j}/quic-v1" for j in range(4) if j != i]}
c["metrics_listen_address"] = f"127.0.0.1:{19200 + i}"
json.dump(c, open(dst, "w"), indent=2)
PY
}

seq_start() { # i marker: one launch of sequencer i
  local i="$1" marker="$2"
  echo "$marker" >> "$W/logs/seq-$i.log"
  ( export RUST_LOG="info,sequencer_core=debug,risc0_zkvm=warn,kameo=warn"; [ "$i" != 0 ] && export SEQUENCER_WAIT_FOR_CHANNEL=true
    exec env -u RISC0_DEV_MODE sequencer_service start "$W/seq-$i" --rpc-listen-address "127.0.0.1:$((19300 + i))" ) \
    >> "$W/logs/seq-$i.log" 2>&1 &
  SEQ_PID[$i]=$!
}

seq_aligned() { # i: started until its blocks fall TICK_MIN..TICK_MAX s after an epoch second (crown session.sh, lez/lab)
  local i="$1" starts=0 delay="" want launched sched after k marker
  while :; do
    starts=$((starts + 1))
    if [ -n "$delay" ]; then
      want=$(( ( (ORIGIN + TICK_AFTER - delay) % INTERVAL + INTERVAL ) % INTERVAL ))
      while [ $(( $(date +%s) % INTERVAL )) != "$want" ]; do sleep 0.1; done
    fi
    marker="u9 start $i.$starts"; launched="$(date +%s)"; seq_start "$i" "$marker"; sched=""
    for k in $(seq 1 300); do
      sched="$(sed -n "/$marker/,\$p" "$W/logs/seq-$i.log" | grep -m1 'Block production scheduler started' | sed -n 's/^\[\([0-9T:-]*\)Z.*/\1/p' || true)"
      [ -n "$sched" ] && break
      kill -0 "${SEQ_PID[$i]}" 2>/dev/null || break
      sleep 1
    done
    [ -n "$sched" ] || { tail -n 20 "$W/logs/seq-$i.log" | scrub; die "seq-$i did not start its block scheduler"; }
    sched="$(date -u -d "${sched}Z" +%s)"; delay=$(( sched - launched ))
    after=$(( ( (sched + INTERVAL - ORIGIN) % INTERVAL + INTERVAL ) % INTERVAL ))
    say "seq-$i start $starts: scheduler $delay s after the launch; its blocks fall $after s after an epoch second"
    if [ "$after" -ge "$TICK_MIN" ] && [ "$after" -le "$TICK_MAX" ]; then break; fi
    if [ "$starts" -ge "$MAX_STARTS" ]; then say "seq-$i: this phase kept after $starts starts"; break; fi
    stop_pid "${SEQ_PID[$i]}" "seq-$i"
  done
}

heights() { local i out=""; for i in 0 1 2 3; do out="$out$( { curl -fsS -m 5 -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getLastBlockId","params":[]}' "http://127.0.0.1:$((19300 + i))" 2>/dev/null || true; } \
  | sed -n 's/.*"result":\([0-9]*\).*/\1/p')/"; done; echo "${out%/}"; }

zone() {
  local B="$W/bundle"; mkdir -p "$B" "$W/bedrock" "$W/inputs"
  zstd -q -d -c "$BUNDLE" | tar -C "$B" -xf -
  ( cd "$B" && sha256sum -c --quiet SHA256SUMS ) || die "bundle SHA256SUMS"
  [ "$(sha256sum "$BRIDGE_IN" | cut -c1-64)" = "$BRIDGE_SHA256" ] || die "$BRIDGE_IN is not this tree's bridge program (ferrata_bridge_lezfee, sha256 $BRIDGE_SHA256)"
  cp "$BRIDGE_IN" "$B/program/ferrata_bridge.bin"
  BIN="$B/bin"; local NODE="$B/node/logos-blockchain-node"; chmod +x "$BIN"/* "$NODE"
  export PATH="$BIN:$PATH" RISC0_SERVER_PATH="$BIN/r0vm" XDG_CACHE_HOME="$B/xdg-cache"
  say "bundle $(sha256sum "$BUNDLE" | cut -c1-16)…; programs: ferrata_lez.bin $(sha256sum "$B/program/ferrata_lez.bin" | cut -c1-16)…, ferrata_bridge.bin $(sha256sum "$B/program/ferrata_bridge.bin" | cut -c1-16)… (B1)"
  cp "$LEZ/bedrock/node-config.yaml" "$LEZ/bedrock/deployment-settings.yaml" "$W/bedrock/"
  sed -i -e "s#listen_address: 0.0.0.0:18080#listen_address: 127.0.0.1:$PORT#" \
         -e 's#^    swarm:$#    swarm:\n      host: 127.0.0.1#' "$W/bedrock/node-config.yaml"
  cmp -s "$W/bedrock/deployment-settings.yaml" "$B/lez-config/deployment-settings.yaml" || die "deployment settings differ from the bundle's"
  ( cd "$W/bedrock" && POL_PROOF_DEV_MODE=true exec "$NODE" node-config.yaml --deployment deployment-settings.yaml ) > "$W/logs/bedrock.log" 2>&1 &
  NODE_PID=$!
  local i k last now_slot seen=0 v; ORIGIN=""
  for i in $(seq 1 600); do curl -fsS -m 3 -o /dev/null "$API/cryptarchia/info" 2>/dev/null && break; kill -0 "$NODE_PID"; sleep 0.2; done
  last="$(curl -fsS -m 3 "$API/cryptarchia/info" | sed -n 's/.*"slot":\([0-9]*\).*/\1/p')"
  for i in $(seq 1 900); do
    now_slot="$( { curl -fsS -m 3 "$API/cryptarchia/info" 2>/dev/null || true; } | sed -n 's/.*"slot":\([0-9]*\).*/\1/p')"
    if [ -n "$now_slot" ] && [ -n "$last" ] && [ "$now_slot" != "$last" ] && [ "$now_slot" -gt 0 ]; then
      v=$(( $(date +%s) - now_slot )); if [ -z "$ORIGIN" ] || [ "$v" -lt "$ORIGIN" ]; then ORIGIN="$v"; fi
      seen=$((seen + 1)); [ "$seen" -ge 3 ] && break
    fi
    last="$now_slot"; sleep 0.1
  done
  [ -n "$ORIGIN" ] || die "could not read the node's slot clock"
  for i in 0 1 2 3; do
    mkdir -p "$W/seq-$i"; cp "$DEV/seq-$i/channel_signing_key" "$W/seq-$i/channel_signing_key"
    write_cfg "$i"
    local flag=""; [ "$i" = 0 ] && flag="--create-channel"
    sequencer_service setup "$W/seq-$i" --config "$W/cfg-$i.json" --funding-public-key "$FUND" $flag > "$W/logs/setup-$i.log" 2>&1 \
      || { tail -n 20 "$W/logs/setup-$i.log"; die "setup $i"; }
  done
  seq_aligned 0
  for i in $(seq 1 150); do curl -fsS -m 5 "$API/channel/$CHANNEL" 2>/dev/null | grep -q '"accredited_keys":\["' && break; sleep 2; done
  for i in 1 2 3; do seq_aligned "$i"; done
  for i in $(seq 1 120); do
    k="$(curl -fsS -m 5 "$API/channel/$CHANNEL" 2>/dev/null | jq -r '.accredited_keys | length' 2>/dev/null || echo 0)"
    [ "$k" = 4 ] && break; sleep 5
  done
  [ "$k" = 4 ] || die "the channel does not accredit four keys"
  for i in $(seq 1 60); do
    h="$(heights)"; [ "$(echo "$h" | tr '/' '\n' | sort -u | wc -l)" = 1 ] && [ "${h%%/*}" -ge 2 ] 2>/dev/null && break; sleep 5
  done
  say "a zone of four sequencers: heights $(heights)"
  ( while :; do echo "[$(date -u +%H:%M:%S)] heights $(heights)"; sleep 30; done ) > "$W/logs/poll.log" 2>&1 &
  LOOP_PID=$!

  # Session A on the zone: the attestation program and the bridge program B1, the STAND-IN instance.
  D=(--rpc "http://127.0.0.1:19300" --run-dir "$W/driver" --program-bin "$B/program/ferrata_lez.bin"
     --bridge-bin "$B/program/ferrata_bridge.bin" --chain stand-in --fixture "$B/fixture" --sequencer-log "$W/logs/seq-0.log")
  curl -fsS -m 5 -o "$W/channel.json" "$API/channel/$CHANNEL"
  export FERRATA_DROP_AFTER_BLOCKS=40
  timeout 3600 env -u RISC0_DEV_MODE "$DRV" crown-a "${D[@]}" --channel "$W/channel.json" \
    --genesis "$W/bedrock/deployment-settings.yaml" --identity-out "$W/session-a.json" > "$W/logs/driver-a.log" 2>&1 \
    || { tail -n 25 "$W/logs/driver-a.log" | scrub; die "crown-a"; }
  cp "$W/session-a.json" "$RUN/session-a.json"
  BRIDGE="$(jq -r .bridge.header "$W/session-a.json")"
  local T0 T0H KEYS PT CTH; T0="$(jq -r .tip.id "$W/session-a.json")"; T0H="$(jq -r .tip.hash "$W/session-a.json")"
  KEYS="$(jq -r '.channel.keys | join(",")' "$W/session-a.json")"; PT="$(jq -r .channel.postingTimeframe "$W/session-a.json")"
  CTH="$(jq -r .channel.configTipHash "$W/session-a.json")"
  say "session A: bridge program $BRIDGE; T0 = $T0; heights $(heights)"

  # The Ethereum side: a local anvil (no fork), the vault paired with the zone's configuration, one deposit.
  anvil --chain-id "$CHAIN_ID" --port 8545 --gas-limit 100000000 --auto-impersonate > "$W/logs/anvil.log" 2>&1 &
  ANVIL_PID=$!
  for i in $(seq 1 60); do cast chain-id --rpc-url "$R" > /dev/null 2>&1 && break; sleep 1; done
  local N Z=0x0000000000000000000000000000000000000000000000000000000000000000
  DEPLOYER="$(cast to-check-sum-address "0x$(cast keccak 'ferrata/u9-endpoints/anvil-deployer' | cut -c27-66)")"
  cast rpc anvil_setBalance "$DEPLOYER" 0x3635C9ADC5DEA00000 --rpc-url "$R" > /dev/null
  N="$(cast nonce "$DEPLOYER" --rpc-url "$R")"
  VAULT="$(cast compute-address "$DEPLOYER" --nonce $(( N + 3 )) | awk '{print $NF}')"   # verifier N, stop N+1, registry N+2
  CID="$("$DRV" config-id --identity "$W/session-a.json" --vault "$VAULT" | jq -r .configId)"
  local GS; GS="$(python3 -I "$REPO/e2e/crown/identity.py" genesis-state --tip-id "$T0" --tip-hash "$T0H" --keys "$KEYS" \
        --posting-timeframe "$PT" --config-tip-hash "$CTH")"
  FROM="$(cast block-number --rpc-url "$R")"
  jq -n --arg v "$VAULT" --arg c "$CID" --arg g "$GS" --argjson ch "$CHAIN_ID" --arg d "$DEPLOYER" '{label:
    "STAND-IN Ethereum side: a local anvil (no fork, Sepolia chain id because crown-driver pins it); the zone, its sequencers and programs are real",
    chainId: $ch, vault: $v, configId: $c, genesisState: $g, delta: 3600, deployer: $d}' > "$RUN/identity.json"
  jq -n --arg c "$CID" --arg g "$GS" --arg z "$Z" '{_source: "u9-endpoints (e2e/endpoints/run.sh): the zone of four sequencers; image ids of docs/image-ids.md",
    checkpointImageIdA: "0xb9c1e6a834879f6be79d09d728523565846018629b82f66986eb58fab4f52c84",
    checkpointImageIdB: "0xf8116722ee692bcff038eed47d95f0de4b267704e45b1d11711ad84e860e626e",
    releaseImageId: "0xb64900bcf741668568b3e14ef902987e305b303873ff7258c109409969eb29e9",
    lezConfigId: $c, genesisState: $g, nMin: 4, tMin: 3, postingTimeframe: 0, ethLezDefinition: $z, tokenLezDefinition: $z}' > "$RUN/params.json"
  local d0 C0 base slots sargs=()
  d0="$("$DRV" destinations | jq -r .public0)"
  C0="$(python3 -I "$REPO/e2e/crown/identity.py" commitment --chain-id "$CHAIN_ID" --vault "$VAULT" --depositor "$DEPLOYER" --j 0 --kind public --dest "$d0")"
  jq -n --arg v "$VAULT" --arg c0 "$C0" --argjson ch "$CHAIN_ID" '{chainId: $ch, vault: $v, deposits: [{j: 0, denomIndex: 0, commitment: $c0}]}' > "$RUN/crown.json"
  base="$(cast block-number --rpc-url "$R")"
  slots="$("$DRV" capture-slots | jq -r '.slots[].slot')"; for s in $slots; do sargs+=(--slot "$s"); done
  local SP="$REPO/e2e/crown/sepolia.sh"
  SEPOLIA_RPC_URL="$R" ETH_FROM="$DEPLOYER" FERRATA_UNLOCKED=1 FERRATA_NONCE_GAP=0 "$SP" "$RUN" deploy > "$W/logs/deploy.log" 2>&1 \
    || { tail -n 20 "$W/logs/deploy.log" | scrub; die "vault deploy"; }
  SEPOLIA_RPC_URL="$R" ETH_FROM="$DEPLOYER" FERRATA_UNLOCKED=1 FERRATA_NONCE_GAP=0 "$SP" "$RUN" deposit 0 > "$W/logs/deposit.log" 2>&1 \
    || { tail -n 20 "$W/logs/deposit.log" | scrub; die "deposit"; }
  rm -f "$REPO/vault/deployments/crown-$(basename "$RUN").json"
  python3 -I "$REPO/lez/crown/capture-anvil.py" --rpc "$R" --vault "$VAULT" --from-block "$base" "${sargs[@]}" "$W/inputs/capture-1" \
    > "$W/logs/capture.log" 2>&1 || { tail -n 20 "$W/logs/capture.log"; die "capture"; }
  "$DRV" check --chain stand-in --program-bin "$B/program/ferrata_lez.bin" --bridge-bin "$B/program/ferrata_bridge.bin" \
    --capture "$W/inputs/capture-1" --identity "$W/session-a.json" --operator-only yes > "$W/logs/driver-check.log" 2>&1 \
    || { tail -n 20 "$W/logs/driver-check.log" | scrub; die "crown-driver check"; }
  say "vault $VAULT (anvil, chain $CHAIN_ID), configId $CID; one deposit; capture checked"

  # The operator: the stand-in committee's finality update, the bridge's Init (pairing), Register ETH, ImportRoot.
  timeout 3600 env -u RISC0_DEV_MODE "$DRV" crown-b "${D[@]}" --identity "$W/session-a.json" --capture "$W/inputs/capture-1" \
    --private fake --operator-only yes --session-out "$W/session-b.json" --notes-out "$W/notes.json" > "$W/logs/driver-b.log" 2>&1 \
    || { tail -n 25 "$W/logs/driver-b.log" | scrub; die "crown-b --operator-only"; }
  jq -c '[.steps[]? | [.step, .status, .block]]' "$W/session-b.json" > "$RUN/operator-steps.json" 2>/dev/null || true
  say "operator: $(cut -c1-400 "$RUN/operator-steps.json"); heights $(heights)"

  # A second live endpoint of the same chain: anvil forked from the first (its state at the fork, the vault included).
  anvil --fork-url "$R" --chain-id "$CHAIN_ID" --port 8546 > "$W/logs/anvil2.log" 2>&1 &
  ANVIL2_PID=$!
  for i in $(seq 1 60); do cast chain-id --rpc-url "$R2" > /dev/null 2>&1 && break; sleep 1; done
  say "anvil fork on $R2: chain $(cast chain-id --rpc-url "$R2"), vault code $(cast code "$VAULT" --rpc-url "$R2" | wc -c) hex chars"
}

# A chain service (seqnet-chain): PORT ETH_RPC VAULT FROM SEQUENCER BRIDGE USER LOG
chain_service() {
  local port="$1" eth="$2" vault="$3" from="$4" seq="$5" bridge="$6" user="$7" name="$8" d="$W/chain-$8"
  mkdir -p "$d/relayer-inbox" "$d/relayer"
  "$T/bin/seqnet-chain" --listen "127.0.0.1:$port" --eth-rpc "$eth" --chain-id "$CHAIN_ID" --vault "$vault" --from-block "$from" \
    --user "$user" --sequencer "$seq" --seed "$W/seed.txt" --lez-wallet "$d/lez-wallet" --lez-password-file "$W/lez.pass" \
    --bridge-account "$bridge" --bridge-program "$W/bundle/program/ferrata_bridge.bin" --relayer-bin /bin/false \
    --relayer 0x0000000000000000000000000000000000000001 --relayer-state "$d/relayer" --relayer-inbox "$d/relayer-inbox" \
    --mock 0x0000000000000000000000000000000000000002 --prover 0x0000000000000000000000000000000000000003 \
    --release-image-id 0xb64900bcf741668568b3e14ef902987e305b303873ff7258c109409969eb29e9 --log "$RUN/chain-$name-calls.jsonl" \
    > "$W/logs/chain-$name.log" 2>&1 &
  CHAIN_PIDS+=($!)
  local pid=$! i
  for i in $(seq 1 120); do curl -fsS -m 2 -d '{"method":"module.endpoints","params":{}}' "http://127.0.0.1:$port/" > /dev/null 2>&1 && break; sleep 1; done
  kill -0 "$pid" 2>/dev/null || { tail -n 20 "$W/logs/chain-$name.log" | scrub; die "seqnet-chain $name did not start"; }
  curl -fsS -m 10 -d '{"method":"module.endpoints","params":{}}' "http://127.0.0.1:$port/" | jq -c .result > "$RUN/chain-$name-start.json"
  say "chain service $name on 127.0.0.1:$port: $(cat "$RUN/chain-$name-start.json")"
}

live() {
  NAME="${1:?live NAME}"
  RUN="$REPO/e2e/runs/endpoints-$NAME"; W="$T/work/$NAME"
  [ -e "$RUN/identity.json" ] && { echo "$RUN holds a run already"; exit 2; }
  rm -rf "$W"; mkdir -p "$W/logs" "$RUN/screenshots" "$LOGOS_SHORT"
  DRV="$T/bin/crown-driver"; LOGOSCTL="$T/logos/logosctl"
  for f in "$DRV" "$T/bin/seqnet-chain" "$T/bin/ferrata_miniapp_logos" "$LOGOSCTL" "$BUNDLE" "$BRIDGE_IN"; do [ -e "$f" ] || die "missing $f (run.sh build; --put the inputs)"; done
  trap cleanup EXIT
  # The box's copy of the worktree is not a git checkout: the commit is given by whoever starts the run (U9_COMMIT).
  say "endpoints-$NAME: commit ${U9_COMMIT:-$(git -C "$REPO" rev-parse --short=12 HEAD 2>/dev/null || echo 'not given (U9_COMMIT)')}; U9_PUBLIC=${U9_PUBLIC:-0}"
  zone

  # The user's wallet files (a published test phrase; nothing of value) and the chain services.
  printf '%s\n' "test test test test test test test test test test test junk" > "$W/seed.txt"; printf 'u9-endpoints\n' > "$W/lez.pass"
  chmod 600 "$W/seed.txt" "$W/lez.pass"
  chain_service 18700 "$R" "$VAULT" "$FROM" "http://127.0.0.1:19300" "$BRIDGE" "$DEPLOYER" local
  if [ "${U9_PUBLIC:-0}" = 1 ]; then
    [ -n "$LIVE_LEZ" ] || die "U9_PUBLIC=1 needs U9_LIVE_LEZ_URL (our public LEZ network's RPC)"
    chain_service 18701 "$SEPOLIA_A" "$LIVE_VAULT" "$LIVE_FROM" "$LIVE_LEZ" "$LIVE_BRIDGE" 0x000000000000000000000000000000000000dEaD live
  fi

  # One headless Logos Core with eth_rpc_module and ferrata_bridge_module.
  S="$W/logos-session"; mkdir -p "$W/logos-home"
  ctl() { TMPDIR="$LOGOS_SHORT" "$LOGOSCTL" --config-dir "$S" "$@"; }
  j() { ctl --timeout 120000 call "$@" --json < /dev/null 2>&1 | jq -c 'if (.result | type) == "string" then (.result | fromjson? // .result) else (.result // .) end' 2>/dev/null; }
  TMPDIR="$LOGOS_SHORT" HOME="$W/logos-home" "$LOGOSCTL" --config-dir "$S" daemon start --detach > "$W/logs/logos-daemon.log" 2>&1 \
    || { cat "$W/logs/logos-daemon.log"; die "logosctl daemon start"; }
  for d in "$T/stack/lgx-eth_rpc_module" "$T/logos/lgx-ferrata_bridge_module"; do ctl install "$d"/*.lgx -y >> "$W/logs/logos-daemon.log" 2>&1 || die "install $d"; done
  for m in eth_rpc_module ferrata_bridge_module; do ctl module load "$m" >> "$W/logs/logos-daemon.log" 2>&1 || die "module load $m"; done
  ctl module list --json < /dev/null > "$RUN/modules-loaded.json" 2>&1 || true
  ( cd "$T" && sha256sum stack/lgx-eth_rpc_module/*.lgx logos/lgx-ferrata_bridge_module/*.lgx bin/seqnet-chain bin/crown-driver ) | scrub > "$RUN/artefacts-sha256.txt"
  # What the build made, logosctl's version with it (run.sh build's VERSIONS), in the records (review L-6).
  scrub < "$T/logos/VERSIONS" > "$RUN/versions.txt" 2>/dev/null || true
  j eth_rpc_module init_defaults > "$RUN/eth-rpc-init.json"
  j eth_rpc_module set_chain_enabled "$CHAIN_ID" true > /dev/null
  j eth_rpc_module get_chain_config "$CHAIN_ID" > "$RUN/eth-rpc-chain-start.json"
  say "Logos Core: $(jq -r '[.[]? | select(.status == "loaded") | .name] | join(", ")' "$RUN/modules-loaded.json" 2>/dev/null); eth_rpc_module chain $CHAIN_ID: $(jq -c '.config.endpoint' "$RUN/eth-rpc-chain-start.json")"

  # The view, driven headless by Qt Quick Test with this Logos Core behind it; ops.sh reads each owner directly.
  local closed; closed="http://127.0.0.1:$(python3 -I -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')"
  cat > "$W/ops-env.sh" <<ENV
LOGOSCTL="$LOGOSCTL"; S="$S"; LOGOS_SHORT="$LOGOS_SHORT"; RUN="$RUN"; CHAIN_ID=$CHAIN_ID
ENV
  say "the view (Qt Quick Test, offscreen): scenario L$( [ "${U9_PUBLIC:-0}" = 1 ] && echo ' and scenario E (read only)')"
  local t0 qrc; t0=$(date +%s)
  ( cd "$REPO/modules/ferrata_bridge_ui/qmltest-logos" && env QT_QPA_PLATFORM=offscreen LOGOSCTL="$LOGOSCTL" LOGOS_SESSION="$S" \
      TMPDIR="$LOGOS_SHORT" SEQNET_TMP="$W" SEQNET_CALL_LOG="$RUN/calls.jsonl" SEQNET_OPS="bash $HERE/ops.sh $W/ops-env.sh" \
      SEQNET_OPS_LOG="$RUN/ops.jsonl" SEQNET_SHOTS="$RUN/screenshots" \
      U9_CHAIN_LOCAL="http://127.0.0.1:18700/" U9_CHAIN_LIVE="http://127.0.0.1:18701/" U9_PUBLIC="${U9_PUBLIC:-0}" \
      U9_SEQ0="http://127.0.0.1:19300" U9_SEQ1="http://127.0.0.1:19301" U9_SEQ2="http://127.0.0.1:19302" U9_SEQ_CLOSED="http://127.0.0.1:19399" \
      U9_ETH_A="$R" U9_ETH_B="$R2" U9_CLOSED="$closed" U9_LIVE_LEZ="$LIVE_LEZ" U9_SEPOLIA_A="$SEPOLIA_A" U9_SEPOLIA_B="$SEPOLIA_B" \
      U9_HOODI="$HOODI" U9_LOCAL_CID="$CID" U9_LIVE_CID=0x3773b58836d2fa242e16b3eb580a15630de2a437218a432f52e5b01152ae67cd \
      "$T/bin/ferrata_miniapp_logos" -input tst_endpoints_logos.qml -o "$RUN/qmltest.txt,txt" -o "$RUN/qmltest.xml,xunitxml" -maxwarnings 0 ) \
    > "$W/logs/qmltest.log" 2>&1 && qrc=0 || qrc=$?
  say "qmltest: exit $qrc in $(( $(date +%s) - t0 )) s: $(grep -E '^Totals' "$RUN/qmltest.txt" 2>/dev/null)"
  grep -E '^(FAIL|PASS|SKIP)' "$RUN/qmltest.txt" 2>/dev/null | cut -c1-300 | tee -a "$RUN/run.log" || true
  j eth_rpc_module get_chain_config "$CHAIN_ID" > "$RUN/eth-rpc-chain-end.json" || true
  python3 -I "$HERE/summary.py" "$RUN" > "$RUN/summary.md" 2>&1 || say "summary.py failed"
  [ "$qrc" = 0 ] || die "the view's test failed"
  say "endpoints-$NAME: done"
}

case "${1:-}" in
  build) mkdir -p "$T"; build ;;
  live) live "${2:-}" ;;
  *) sed -n '2,40p' "${BASH_SOURCE[0]}"; exit 2 ;;
esac
