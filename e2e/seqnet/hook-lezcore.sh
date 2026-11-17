#!/usr/bin/env bash
# e2e/seqnet's hook for the round trip through Logos Core's own LEZ module: lez/crown/session.sh x runs it while the zone
# runs; drive.sh has made the vault on anvil. ferrata_bridge_module inside a headless Logos Core, beside lez_core
# (logos-blockchain/logos-execution-zone-module, LEZ v0.4.0-rc1's wallet as a module, at its merged v0.4 state) built
# from its source with logos-module-builder (run.sh build-lezcore):
#   1. seqnet-chain, the plain path: the zone's history and Ethereum (the vault's reads, the user's keystore, the relayer),
#      and every LEZ read and send kept beside lez_core's (the stand-in for eth_rpc_module, keystore_module and
#      tx_sender_module, which this run does not route through);
#   2. a probe Logos Core with lez_core as merged (cc73ebf): what it answers for one of the bridge program's PDAs;
#   3. the run's Logos Core with ferrata_bridge_module and lez_core with modules/patches/lez_core-*.patch: the user's
#      LEZ wallet opened inside lez_core (the same storage seqnet-chain holds, copied; force_rpc: a client of this zone's
#      sequencer); the module's session routed through it (`wallet lez`): every bridge actor state it reads, the private
#      holding, the private claim and the private burn, each beside the plain path's (`wallet lez_report`);
#   4. the user's calls in the mini-app's order, made with logosctl (no view: the wallet's privacy delays, which the
#      view and the tool add, are not waited for); the operator's import, the gas drop's stand-in, the checkpoint and the
#      clocks by ops.sh between them, as the mini-app's run does.
# Records: $RUN/lezcore/ (calls.jsonl: every call of the module with its reply; lez-report.json; lez-probe.json).
set -uo pipefail
. "$SEQNET_ENV"
. "$REPO/e2e/seqnet/common.sh"
M="$RUN/lezcore"; mkdir -p "$M"
CHAIN_PORT=18700
LOGOSCTL="$T/logos/logosctl"; LGX_DIR="$T/logos/lgx-ferrata_bridge_module"
LEZ_UP="$T/logos/lgx-lez_core-upstream"; LEZ_PATCHED="$T/logos/lgx-lez_core-patched"
for x in "$LOGOSCTL" "$LGX_DIR" "$LEZ_UP" "$LEZ_PATCHED"; do [ -e "$x" ] || die "run.sh build-lezcore first ($x missing)"; done

# 1. The chain service (the plain path).
mkdir -p "$W/relayer-inbox"
"${PROVE[@]}" "$BIN/seqnet-chain" --listen "127.0.0.1:$CHAIN_PORT" --eth-rpc "$RPC" --chain-id 11155111 --vault "$VAULT" --from-block "$FROM" \
  --keystore "$KEYSTORE" --password-file "$U/eth.pass" --sequencer "$SEQ" --seed "$U/seed.txt" --lez-wallet "$U/lez-wallet" \
  --lez-password-file "$U/lez.pass" --bridge-account "$BRIDGE" --bridge-program "$BRIDGE_BIN" --relayer-bin "$BIN/ferrata-relayer" \
  --relayer "$RELAYER" --relayer-state "$W/relayer" --relayer-inbox "$W/relayer-inbox" --mock "$MOCK" --prover "$DEPLOYER" \
  --release-image-id "$RELEASE_ID" --log "$M/chain-calls.jsonl" --proofs "$PROOFS" > "$W/seqnet-chain.log" 2>&1 &
CHAIN_PID=$!
for _ in $(seq 120); do curl -fsS -m 2 -d '{"method":"module.info","params":{}}' "http://127.0.0.1:$CHAIN_PORT/" > /dev/null 2>&1 && break; sleep 1; done
kill -0 $CHAIN_PID 2>/dev/null || { tail -n 20 "$W/seqnet-chain.log"; die "seqnet-chain did not start"; }
say "seqnet-chain: $(head -n 1 "$W/seqnet-chain.log")"

# Logos Core sessions (sockets in a short TMPDIR: a Unix socket path holds 107 bytes).
LT="$BESIDE_SHORT"; mkdir -p "$LT"
dmode=(env RISC0_DEV_MODE=1); [ "$PROOFS" = real ] && dmode=(env -u RISC0_DEV_MODE)
core_start() { # DIR: a daemon with its own config directory
  mkdir -p "$1.home"
  # lez_core proves private transactions inside its module process: the zone's r0vm on its PATH and as RISC0_SERVER_PATH.
  "${dmode[@]}" RISC0_SERVER_PATH="${RISC0_SERVER_PATH:-}" PATH="${ZONE_BIN:-/nonexistent}:$PATH" TMPDIR="$LT" HOME="$1.home" \
    "$LOGOSCTL" --config-dir "$1" daemon start --detach \
    >> "$M/logos-daemon.txt" 2>&1
}
S=""
ctl() { TMPDIR="$LT" "$LOGOSCTL" --config-dir "$S" "$@"; }
# call METHOD JSON: one call of the module's wallet, as the mini-app's view makes it; the line in calls.jsonl.
n=0
call() {
  local method="$1" params="$2" req out rc t0 ms reply
  n=$((n + 1)); req="$W/req-$n.json"
  jq -c -n --arg m "$method" --argjson p "$params" '{method: $m, params: $p}' > "$req"
  t0=$(date +%s%N)
  out="$(ctl --timeout 3600000 call ferrata_bridge_module wallet "@$req" --json < /dev/null 2> "$W/call-$n.err")"; rc=$?
  ms=$(( ($(date +%s%N) - t0) / 1000000 ))
  reply="$(echo "$out" | jq -c '.result | if type == "string" then (fromjson? // .) else . end' 2>/dev/null)"
  [ -n "$reply" ] || reply="$(jq -c -n --arg o "$out" --arg e "$(head -c 400 "$W/call-$n.err")" '{ok: false, error: ("logosctl: " + $o + " " + $e)}')"
  jq -c -n --arg m "$method" --argjson p "$params" --argjson r "$reply" --argjson ms "$ms" --argjson rc "$rc" --arg t "$(date -u +%FT%TZ)" \
    '{t: $t, method: $m, request: $p, reply: $r, ms: $ms, logosctl_exit: $rc}' >> "$M/calls.jsonl"
  echo "$reply" > "$W/reply-$n.json"
  say "wallet $method: $(echo "$reply" | jq -r 'if .ok then (.result | if type == "object" then del(.accounts, .lez, .detail, .calls) else . end | tostring | .[0:220]) else ("ERROR " + (.error | tostring | .[0:300])) end')"
  echo "$reply" | jq -e '.ok == true' > /dev/null
}
finish() {
  for d in "$W/core-probe" "$W/core"; do TMPDIR="$LT" "$LOGOSCTL" --config-dir "$d" daemon stop >> "$M/logos-daemon.txt" 2>&1 || true; done
  for d in core-probe core; do
    ls "$W/$d"/logs/ > /dev/null 2>&1 && tail -n 200 "$W/$d"/logs/*.log > "$M/$d-logs-tail.txt" 2>/dev/null || true
  done
  curl -fsS -m 5 -d '{"method":"module.stop","params":{}}' "http://127.0.0.1:$CHAIN_PORT/" > /dev/null 2>&1 || true
  sleep 1; kill $CHAIN_PID 2>/dev/null || true
}
trap finish EXIT

# 2. The probe: lez_core as merged (cc73ebf) beside the module; one PDA as the bridge's private sends mention it.
S="$W/core-probe"; rm -rf "$S"; core_start "$S" || die "logosctl daemon start (probe)"
ctl install "$LGX_DIR"/*.lgx -y >> "$M/logos-daemon.txt" 2>&1 && ctl install "$LEZ_UP"/*.lgx -y >> "$M/logos-daemon.txt" 2>&1 \
  && ctl module load lez_core >> "$M/logos-daemon.txt" 2>&1 && ctl module load ferrata_bridge_module >> "$M/logos-daemon.txt" 2>&1 \
  || die "the probe's Logos Core (install, load)"
ctl module show lez_core --json < /dev/null > "$M/lez_core-upstream-show.json" 2>&1 || true
call open "$(jq -c -n --arg c "http://127.0.0.1:$CHAIN_PORT/" '{chain: $c}')" || die "open (probe)"
call lez_probe '{}'; cp "$W/reply-$n.json" "$M/lez-probe-upstream.json"
TMPDIR="$LT" "$LOGOSCTL" --config-dir "$S" daemon stop >> "$M/logos-daemon.txt" 2>&1 || true

# 3. The run's Logos Core: the module and lez_core with the patch.
S="$W/core"; rm -rf "$S"; core_start "$S" || die "logosctl daemon start"
ctl install "$LGX_DIR"/*.lgx -y >> "$M/logos-daemon.txt" 2>&1 && ctl install "$LEZ_PATCHED"/*.lgx -y >> "$M/logos-daemon.txt" 2>&1 \
  && ctl module load lez_core >> "$M/logos-daemon.txt" 2>&1 && ctl module load ferrata_bridge_module >> "$M/logos-daemon.txt" 2>&1 \
  || die "Logos Core (install, load)"
ctl module show lez_core --json < /dev/null > "$M/lez_core-patched-show.json" 2>&1 || true
ctl module list --json < /dev/null > "$M/modules-loaded.json" 2>&1 || true
( cd "$T/logos" && sha256sum lgx-ferrata_bridge_module/*.lgx lgx-lez_core-upstream/*.lgx lgx-lez_core-patched/*.lgx ) > "$M/lgx-sha256.txt"
say "Logos Core: ferrata_bridge_module and lez_core (patched) installed from their .lgx and loaded"

call open "$(jq -c -n --arg c "http://127.0.0.1:$CHAIN_PORT/" '{chain: $c}')" || die "open"
call lez_probe '{}'; cp "$W/reply-$n.json" "$M/lez-probe-patched.json"
# The user's LEZ wallet inside lez_core: the storage seqnet-chain made from the seed (its config names this zone's sequencer).
cp -r "$U/lez-wallet" "$W/lezcore-wallet"
call lez "$(jq -c -n --arg c "$W/lezcore-wallet/wallet_config.json" --arg s "$W/lezcore-wallet/storage.json" --arg p "$U/lez.pass" \
  --arg b "$BRIDGE_BIN" '{via: "lez_core", wallet: {config: $c, storage: $s, password_file: $p}, program_bin: $b, probe_unauthorized: true}')" \
  || die "lez_core did not open the user's wallet"
cp "$W/reply-$n.json" "$M/lez-open.json"
call info '{}' || die "info"
call scan '{}' || die "scan"
call registry '{}' || die "registry"
call price '{"denomination": 0}' || die "price"
call deposit '{"account": 0, "denomination": 0, "kind": "private"}' || die "deposit"
call scan '{}' || die "scan"
bash "$REPO/e2e/seqnet/ops.sh" import || die "ops.sh import"
call registry '{}' || die "registry"
bash "$REPO/e2e/seqnet/ops.sh" fund || die "ops.sh fund"
call scan '{}' || die "scan"
# The module's claim and burn are jobs (calls/src/jobs.rs); logosctl's call may last minutes (--timeout above), so these
# run in the call itself ("wait": true), answered as before.
call claim '{"leaf_index": 0, "private": true, "wait": true}' || die "the private claim"
call scan '{}' || die "scan"
FRESH="$(cast wallet new --json | jq -r '.[0].address')"
echo "FRESH=$FRESH" >> "$SEQNET_ENV"
say "the user's fresh Ethereum address for the release: $FRESH (cast wallet new; its key is not kept)"
call burn "$(jq -c -n --arg r "$FRESH" '{account: 0, amount: "1000000000000000", private: true, recipient: $r, wait: true}')" || die "the private burn"
call scan '{}' || die "scan"
bash "$REPO/e2e/seqnet/ops.sh" checkpoint || die "ops.sh checkpoint"
if call release '{"burn_index": 0}'; then
  say "note: the release was not held"
else
  bash "$REPO/e2e/seqnet/ops.sh" pass-time 121 "the release delay (120 s after finalizeAttested)" || die "pass-time"
  call release '{"burn_index": 0}' || die "release"
fi
say "the fresh address $FRESH holds $(cast balance "$FRESH" --rpc-url "$RPC") wei"
call scan '{}' || die "scan"
call lez_report '{}'; jq '.result' "$W/reply-$n.json" > "$M/lez-report.json"
say "lez_core: $(jq -c '{reads, reads_equal, reads_failed, private_reads, private_reads_equal, sends, sends_sent}' "$M/lez-report.json")"
records
say "hook-lezcore: done"
